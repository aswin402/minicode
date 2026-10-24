use crate::agent::types::{ToolCall, ToolResult};
use crate::tools::concurrency::{
    classify_tool, is_parallelizable_file_mutation, normalize_mutation_target,
};
use crate::tools::ToolRegistry;
use futures::future::join_all;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{Mutex, Semaphore};
use tokio::task::JoinHandle;

/// RAII guard that emits periodic `AgentEvent::Heartbeat` progress ticks for long-running tool
/// executions after an initial quiet window (`after_ms`), automatically aborting its background
/// task when dropped (on completion, error, cancellation, or panic).
pub struct HeartbeatGuard {
    handle: JoinHandle<()>,
}

impl HeartbeatGuard {
    pub fn spawn(
        event_sender: tokio::sync::mpsc::UnboundedSender<crate::agent::types::AgentEvent>,
        turn_id: usize,
        label: String,
        after_ms: u64,
        every_ms: u64,
    ) -> Self {
        let start = Instant::now();
        let handle = tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(after_ms)).await;
            let mut interval =
                tokio::time::interval(std::time::Duration::from_millis(every_ms.max(250)));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                interval.tick().await;
                let elapsed_secs = start.elapsed().as_secs_f32();
                let ev = crate::agent::types::AgentEvent::Heartbeat {
                    timestamp: chrono::Utc::now().to_rfc3339(),
                    status: format!("Running {} ({:.1}s elapsed)...", label, elapsed_secs),
                    turn_id: Some(turn_id),
                };
                if event_sender.send(ev).is_err() {
                    break;
                }
            }
        });
        Self { handle }
    }
}

impl Drop for HeartbeatGuard {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

/// Atomic stage in a turn's tool execution plan.
#[derive(Debug, Clone, PartialEq)]
pub enum ExecutionStage {
    /// 1 or more consecutive read-only tool calls (or 2+ disjoint-path file mutations)
    /// safe to execute concurrently in parallel.
    Parallel(Vec<ToolCall>),
    /// A single mutating or barrier tool call that must execute in sequential isolation.
    Sequential(ToolCall),
}

/// Partitions an arbitrary sequence of tool calls into safe execution stages.
pub struct ExecutionPlanner;

impl ExecutionPlanner {
    fn flush_mutation_batch(
        stages: &mut Vec<ExecutionStage>,
        mutation_batch: &mut Vec<ToolCall>,
        mutation_paths: &mut HashSet<String>,
    ) {
        mutation_paths.clear();
        if mutation_batch.is_empty() {
            return;
        }
        let batch = std::mem::take(mutation_batch);
        if batch.len() == 1 {
            if let Some(single) = batch.into_iter().next() {
                stages.push(ExecutionStage::Sequential(single));
            }
        } else {
            stages.push(ExecutionStage::Parallel(batch));
        }
    }

    /// Partition tool calls into parallel read-only stages, parallel disjoint file-mutation
    /// stages (when 2+ consecutive `write_file`/`patch_file` calls target distinct files),
    /// and sequential barrier stages.
    pub fn plan(calls: Vec<ToolCall>) -> Vec<ExecutionStage> {
        let mut stages = Vec::new();
        let mut current_parallel = Vec::new();
        let mut mutation_batch: Vec<ToolCall> = Vec::new();
        let mut mutation_paths: HashSet<String> = HashSet::new();

        for call in calls {
            let safety = classify_tool(&call.name);
            if safety.is_read_only() {
                Self::flush_mutation_batch(&mut stages, &mut mutation_batch, &mut mutation_paths);
                current_parallel.push(call);
            } else if is_parallelizable_file_mutation(&call.name) {
                if !current_parallel.is_empty() {
                    stages.push(ExecutionStage::Parallel(std::mem::take(
                        &mut current_parallel,
                    )));
                }
                if let Some(norm_path) = normalize_mutation_target(&call.arguments) {
                    if mutation_paths.contains(&norm_path) {
                        Self::flush_mutation_batch(
                            &mut stages,
                            &mut mutation_batch,
                            &mut mutation_paths,
                        );
                    }
                    mutation_paths.insert(norm_path);
                    mutation_batch.push(call);
                } else {
                    Self::flush_mutation_batch(
                        &mut stages,
                        &mut mutation_batch,
                        &mut mutation_paths,
                    );
                    stages.push(ExecutionStage::Sequential(call));
                }
            } else {
                if !current_parallel.is_empty() {
                    stages.push(ExecutionStage::Parallel(std::mem::take(
                        &mut current_parallel,
                    )));
                }
                Self::flush_mutation_batch(&mut stages, &mut mutation_batch, &mut mutation_paths);
                stages.push(ExecutionStage::Sequential(call));
            }
        }

        if !current_parallel.is_empty() {
            stages.push(ExecutionStage::Parallel(current_parallel));
        }
        Self::flush_mutation_batch(&mut stages, &mut mutation_batch, &mut mutation_paths);

        stages
    }
}

/// Telemetry metrics for speculative and parallel tool execution.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct SpeculativeTelemetry {
    pub speculative_launched: usize,
    pub speculative_hits: usize,
    pub parallel_stages_executed: usize,
    pub parallel_tools_executed: usize,
    pub wall_clock_saved_ms: u64,
}

type InFlightMap = Arc<Mutex<HashMap<String, JoinHandle<ToolResult>>>>;

/// Engine for speculative early dispatch during SSE streaming and parallel read-only execution.
pub struct SpeculativeExecutor {
    workspace_root: PathBuf,
    semaphore: Arc<Semaphore>,
    speculative_enabled: bool,
    parallel_enabled: bool,
    barrier_encountered: Arc<AtomicBool>,
    in_flight: InFlightMap,
    telemetry: Arc<Mutex<SpeculativeTelemetry>>,
}

impl SpeculativeExecutor {
    /// Creates a new speculative executor.
    pub fn new(
        workspace_root: PathBuf,
        max_concurrency: usize,
        speculative_enabled: bool,
        parallel_enabled: bool,
    ) -> Self {
        let concurrency = max_concurrency.max(1);
        Self {
            workspace_root,
            semaphore: Arc::new(Semaphore::new(concurrency)),
            speculative_enabled,
            parallel_enabled,
            barrier_encountered: Arc::new(AtomicBool::new(false)),
            in_flight: Arc::new(Mutex::new(HashMap::new())),
            telemetry: Arc::new(Mutex::new(SpeculativeTelemetry::default())),
        }
    }

    /// Resets turn state before a new generation turn begins.
    pub async fn reset_turn(&self) {
        self.cancel_all().await;
        self.barrier_encountered.store(false, Ordering::SeqCst);
    }

    /// Invoked as soon as a `ToolCallChunk` arrives in the streaming response.
    /// If speculative pre-execution is enabled and the tool is read-only, spawns a
    /// background task to execute it immediately while the LLM continues streaming.
    pub async fn on_tool_call_streamed(&self, turn_id: usize, tool_call: &ToolCall) {
        if !self.speculative_enabled || self.barrier_encountered.load(Ordering::SeqCst) {
            return;
        }

        let safety = classify_tool(&tool_call.name);
        if safety.is_barrier() {
            tracing::debug!(
                tool = %tool_call.name,
                "Encountered mutating tool in stream; halting speculative pre-execution for this turn"
            );
            self.barrier_encountered.store(true, Ordering::SeqCst);
            return;
        }

        if safety.is_read_only() {
            let ws = self.workspace_root.clone();
            let sem = self.semaphore.clone();
            let id = tool_call.id.clone();
            let name = tool_call.name.clone();
            let args = tool_call.arguments.clone();

            tracing::debug!(
                tool = %name,
                id = %id,
                "Speculatively dispatching read-only tool in background"
            );

            let handle: JoinHandle<ToolResult> = tokio::spawn(async move {
                let _permit = sem.acquire().await;
                ToolRegistry::dispatch(&ws, &id, &name, &args, None, turn_id).await
            });

            {
                let mut guard = self.in_flight.lock().await;
                if let Some(prev) = guard.insert(tool_call.id.clone(), handle) {
                    prev.abort();
                }
            }

            {
                let mut telem = self.telemetry.lock().await;
                telem.speculative_launched += 1;
            }
        }
    }

    /// Executes a parallel stage of read-only or disjoint-file tool calls concurrently.
    /// If any calls were already dispatched speculatively during streaming, awaits them directly.
    /// Returns tool results strictly in the order of `calls`.
    #[allow(dead_code)]
    pub async fn execute_parallel_stage(
        &self,
        calls: &[ToolCall],
        turn_id: usize,
    ) -> Vec<ToolResult> {
        self.execute_parallel_stage_with_backup(calls, turn_id, None)
            .await
    }

    /// Executes a parallel stage with an optional `BackupManager` for disjoint file mutations.
    pub async fn execute_parallel_stage_with_backup(
        &self,
        calls: &[ToolCall],
        turn_id: usize,
        backup_manager: Option<&crate::session::backup::BackupManager>,
    ) -> Vec<ToolResult> {
        if calls.is_empty() {
            return Vec::new();
        }

        let stage_start = Instant::now();
        let active_semaphore = if self.parallel_enabled && calls.len() > 1 {
            self.semaphore.clone()
        } else {
            Arc::new(Semaphore::new(1))
        };
        let mut futures = Vec::with_capacity(calls.len());

        for call in calls {
            let existing_handle = {
                let mut in_flight_guard = self.in_flight.lock().await;
                in_flight_guard.remove(&call.id)
            };

            if let Some(existing_handle) = existing_handle {
                // Speculative hit: tool was already launched during streaming!
                let call_id = call.id.clone();
                let call_name = call.name.clone();
                futures.push(tokio::spawn(async move {
                    match existing_handle.await {
                        Ok(res) => (true, res),
                        Err(join_err) => (
                            true,
                            ToolResult {
                                tool_id: call_id,
                                tool_name: call_name,
                                success: false,
                                output: format!("Speculative task join error: {}", join_err),
                                display_output: String::new(),
                                duration_ms: 0,
                            },
                        ),
                    }
                }));
            } else {
                // Dispatch on worker pool bounded by active_semaphore (parallel or sequential)
                let sem = active_semaphore.clone();
                let ws = self.workspace_root.clone();
                let id = call.id.clone();
                let name = call.name.clone();
                let args = call.arguments.clone();
                let backup = backup_manager.cloned();
                futures.push(tokio::spawn(async move {
                    let _permit = sem.acquire().await;
                    let res =
                        ToolRegistry::dispatch(&ws, &id, &name, &args, backup.as_ref(), turn_id)
                            .await;
                    (false, res)
                }));
            }
        }

        let joined = join_all(futures).await;
        let mut results = Vec::with_capacity(calls.len());
        let mut hits = 0;

        for (idx, join_res) in joined.into_iter().enumerate() {
            let fallback_id = calls.get(idx).map(|c| c.id.clone()).unwrap_or_default();
            let fallback_name = calls.get(idx).map(|c| c.name.clone()).unwrap_or_default();

            match join_res {
                Ok((was_hit, tool_res)) => {
                    if was_hit {
                        hits += 1;
                    }
                    results.push(tool_res);
                }
                Err(join_err) => {
                    results.push(ToolResult {
                        tool_id: fallback_id,
                        tool_name: fallback_name,
                        success: false,
                        output: format!("Parallel execution task panicked: {}", join_err),
                        display_output: String::new(),
                        duration_ms: 0,
                    });
                }
            }
        }

        let wall_clock_ms = stage_start.elapsed().as_millis() as u64;
        let sum_durations_ms: u64 = results.iter().map(|r| r.duration_ms).sum();
        let saved_ms = sum_durations_ms.saturating_sub(wall_clock_ms);

        {
            let mut telem = self.telemetry.lock().await;
            telem.speculative_hits += hits;
            telem.parallel_stages_executed += 1;
            telem.parallel_tools_executed += calls.len();
            telem.wall_clock_saved_ms += saved_ms;
        }

        if calls.len() > 1 {
            let all_read_only = calls.iter().all(|c| classify_tool(&c.name).is_read_only());
            if all_read_only {
                tracing::info!(
                    count = calls.len(),
                    wall_clock_ms = wall_clock_ms,
                    sum_durations_ms = sum_durations_ms,
                    saved_ms = saved_ms,
                    speculative_hits = hits,
                    "Executed read-only tool batch in parallel"
                );
            } else {
                tracing::info!(
                    count = calls.len(),
                    wall_clock_ms = wall_clock_ms,
                    sum_durations_ms = sum_durations_ms,
                    saved_ms = saved_ms,
                    speculative_hits = hits,
                    "Executed independent tool batch in parallel"
                );
            }
        }

        results
    }

    /// Aborts any in-flight speculative background tasks.
    pub async fn cancel_all(&self) {
        let mut guard = self.in_flight.lock().await;
        for (_, handle) in guard.drain() {
            handle.abort();
        }
    }

    /// Retrieves current speculative telemetry.
    #[allow(dead_code)]
    pub async fn telemetry(&self) -> SpeculativeTelemetry {
        self.telemetry.lock().await.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_call(id: &str, name: &str) -> ToolCall {
        ToolCall {
            id: id.to_string(),
            name: name.to_string(),
            arguments: json!({}),
        }
    }

    #[test]
    fn test_planner_partitions_consecutive_read_tools() {
        let calls = vec![
            make_call("c1", "read_file"),
            make_call("c2", "locate_symbol"),
            make_call("c3", "git_status"),
        ];

        let stages = ExecutionPlanner::plan(calls);
        assert_eq!(stages.len(), 1);
        match &stages[0] {
            ExecutionStage::Parallel(items) => {
                assert_eq!(items.len(), 3);
                assert_eq!(items[0].id, "c1");
                assert_eq!(items[1].id, "c2");
                assert_eq!(items[2].id, "c3");
            }
            _ => panic!("Expected Parallel stage"),
        }
    }

    #[test]
    fn test_planner_partitions_mixed_stages_with_barriers() {
        let calls = vec![
            make_call("c1", "read_file"),
            make_call("c2", "locate_symbol"),
            make_call("c3", "patch_file"),
            make_call("c4", "git_status"),
            make_call("c5", "exec_cmd"),
        ];

        let stages = ExecutionPlanner::plan(calls);
        assert_eq!(stages.len(), 4);

        assert_eq!(
            stages[0],
            ExecutionStage::Parallel(vec![
                make_call("c1", "read_file"),
                make_call("c2", "locate_symbol")
            ])
        );
        assert_eq!(
            stages[1],
            ExecutionStage::Sequential(make_call("c3", "patch_file"))
        );
        assert_eq!(
            stages[2],
            ExecutionStage::Parallel(vec![make_call("c4", "git_status")])
        );
        assert_eq!(
            stages[3],
            ExecutionStage::Sequential(make_call("c5", "exec_cmd"))
        );
    }

    #[test]
    fn test_planner_empty_calls() {
        let stages = ExecutionPlanner::plan(vec![]);
        assert!(stages.is_empty());
    }

    #[test]
    fn test_planner_batches_disjoint_file_mutations_and_splits_collisions() {
        let calls = vec![
            ToolCall {
                id: "m1".to_string(),
                name: "patch_file".to_string(),
                arguments: json!({"path": "src/a.rs", "search_block": "a", "replace_block": "b"}),
            },
            ToolCall {
                id: "m2".to_string(),
                name: "write_file".to_string(),
                arguments: json!({"path": "src/b.rs", "content": "hello"}),
            },
            ToolCall {
                id: "m3".to_string(),
                name: "patch_file".to_string(),
                arguments: json!({"path": "./src/a.rs", "search_block": "b", "replace_block": "c"}),
            },
        ];

        let stages = ExecutionPlanner::plan(calls);
        assert_eq!(stages.len(), 2);
        match &stages[0] {
            ExecutionStage::Parallel(batch) => {
                assert_eq!(batch.len(), 2);
                assert_eq!(batch[0].id, "m1");
                assert_eq!(batch[1].id, "m2");
            }
            other => panic!(
                "Expected Parallel batch for disjoint paths, got {:?}",
                other
            ),
        }
        match &stages[1] {
            ExecutionStage::Sequential(call) => {
                assert_eq!(call.id, "m3");
            }
            other => panic!("Expected Sequential for colliding path, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_heartbeat_guard_emits_and_stops_on_drop() {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        {
            let _guard = HeartbeatGuard::spawn(tx, 1, "exec_cmd".to_string(), 20, 250);
            tokio::time::sleep(std::time::Duration::from_millis(60)).await;
        }
        let first = rx.try_recv().expect("expected at least one heartbeat tick");
        match first {
            crate::agent::types::AgentEvent::Heartbeat {
                status, turn_id, ..
            } => {
                assert!(status.contains("exec_cmd"));
                assert_eq!(turn_id, Some(1));
            }
            other => panic!("Unexpected event: {:?}", other),
        }
        // Drain any remaining buffered event and ensure no further ticks arrive after drop
        while rx.try_recv().is_ok() {}
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        assert!(rx.try_recv().is_err());
    }
}

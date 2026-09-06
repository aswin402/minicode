use crate::agent::types::{ToolCall, ToolResult};
use crate::tools::concurrency::classify_tool;
use crate::tools::ToolRegistry;
use futures::future::join_all;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{Mutex, Semaphore};
use tokio::task::JoinHandle;

/// Atomic stage in a turn's tool execution plan.
#[derive(Debug, Clone, PartialEq)]
pub enum ExecutionStage {
    /// 1 or more consecutive read-only tool calls safe to execute concurrently in parallel.
    Parallel(Vec<ToolCall>),
    /// A single mutating or barrier tool call that must execute in sequential isolation.
    Sequential(ToolCall),
}

/// Partitions an arbitrary sequence of tool calls into safe execution stages.
pub struct ExecutionPlanner;

impl ExecutionPlanner {
    /// Partition tool calls into parallel read-only stages and sequential barrier stages.
    /// Mutating tools and control barriers act as impermeable sequential boundaries.
    pub fn plan(calls: Vec<ToolCall>) -> Vec<ExecutionStage> {
        let mut stages = Vec::new();
        let mut current_parallel = Vec::new();

        for call in calls {
            let safety = classify_tool(&call.name);
            if safety.is_read_only() {
                current_parallel.push(call);
            } else {
                if !current_parallel.is_empty() {
                    stages.push(ExecutionStage::Parallel(std::mem::take(
                        &mut current_parallel,
                    )));
                }
                stages.push(ExecutionStage::Sequential(call));
            }
        }

        if !current_parallel.is_empty() {
            stages.push(ExecutionStage::Parallel(current_parallel));
        }

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
    max_concurrency: usize,
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
        Self {
            workspace_root,
            max_concurrency: max_concurrency.max(1),
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
        if !self.speculative_enabled {
            return;
        }

        if self.barrier_encountered.load(Ordering::SeqCst) {
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
            let id = tool_call.id.clone();
            let name = tool_call.name.clone();
            let args = tool_call.arguments.clone();

            tracing::debug!(
                tool = %name,
                id = %id,
                "Speculatively dispatching read-only tool in background"
            );

            let handle: JoinHandle<ToolResult> = tokio::spawn(async move {
                ToolRegistry::dispatch(&ws, &id, &name, &args, None, turn_id).await
            });

            {
                let mut guard = self.in_flight.lock().await;
                guard.insert(tool_call.id.clone(), handle);
            }

            {
                let mut telem = self.telemetry.lock().await;
                telem.speculative_launched += 1;
            }
        }
    }

    /// Executes a parallel stage of read-only tool calls concurrently.
    /// If any calls were already dispatched speculatively during streaming, awaits them directly.
    /// Returns tool results strictly in the order of `calls`.
    pub async fn execute_parallel_stage(
        &self,
        calls: &[ToolCall],
        turn_id: usize,
    ) -> Vec<ToolResult> {
        if calls.is_empty() {
            return Vec::new();
        }

        let stage_start = Instant::now();
        let semaphore = Arc::new(Semaphore::new(self.max_concurrency.max(1)));
        let mut futures = Vec::with_capacity(calls.len());

        for call in calls {
            let mut in_flight_guard = self.in_flight.lock().await;
            if let Some(existing_handle) = in_flight_guard.remove(&call.id) {
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
                                duration_ms: 0,
                            },
                        ),
                    }
                }));
            } else if self.parallel_enabled && calls.len() > 1 {
                // Concurrently spawn on worker pool bounded by semaphore
                let sem = semaphore.clone();
                let ws = self.workspace_root.clone();
                let id = call.id.clone();
                let name = call.name.clone();
                let args = call.arguments.clone();
                futures.push(tokio::spawn(async move {
                    let _permit = sem.acquire().await;
                    let res = ToolRegistry::dispatch(&ws, &id, &name, &args, None, turn_id).await;
                    (false, res)
                }));
            } else {
                // Sequential fallback (e.g. parallel_enabled = false or single tool)
                let ws = self.workspace_root.clone();
                let id = call.id.clone();
                let name = call.name.clone();
                let args = call.arguments.clone();
                futures.push(tokio::spawn(async move {
                    let res = ToolRegistry::dispatch(&ws, &id, &name, &args, None, turn_id).await;
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
            tracing::info!(
                count = calls.len(),
                wall_clock_ms = wall_clock_ms,
                sum_durations_ms = sum_durations_ms,
                saved_ms = saved_ms,
                speculative_hits = hits,
                "Executed read-only tool batch in parallel"
            );
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
}

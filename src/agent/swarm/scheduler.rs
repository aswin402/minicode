//! Dependency-aware DAG Scheduler & Swarm Execution Runtime.
//!
//! Executes swarm tasks in topological waves with concurrency bounding,
//! isolated git worktrees, artifact passing, live process telemetry, and reactive unblocking.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Instant;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::{Mutex, Semaphore};
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

use crate::agent::subagent::types::AgentId;
use crate::agent::swarm::arbitrator::SwarmArbitrator;
use crate::agent::swarm::models::{
    SwarmError, SwarmExecutionState, SwarmPlan, SwarmTaskOutcome, SwarmTaskSpec, SwarmTaskStatus,
};
use crate::agent::swarm::report::SwarmReporter;
use crate::sandbox::arbitration::MergeArbitrator;
use crate::sandbox::worktree::GitWorktreeManager;

/// Configuration options for executing a swarm run.
#[derive(Debug, Clone)]
pub struct SwarmRunOptions {
    pub max_workers: usize,
    pub auto_merge: bool,
    #[allow(dead_code)]
    pub json_stream: bool,
}

impl Default for SwarmRunOptions {
    fn default() -> Self {
        Self {
            max_workers: 4,
            auto_merge: true,
            json_stream: false,
        }
    }
}

/// Concurrency scheduler and executor for Swarm DAG plans.
pub struct SwarmScheduler;

impl SwarmScheduler {
    /// Executes an entire SwarmPlan DAG, returning the final execution state and report path.
    pub async fn execute_plan(
        workspace_root: &Path,
        plan: SwarmPlan,
        options: SwarmRunOptions,
        cancel_token: CancellationToken,
    ) -> Result<(SwarmExecutionState, PathBuf), SwarmError> {
        // 1. Validate DAG integrity
        plan.validate()?;

        let start_time = Instant::now();
        let swarm_dir = workspace_root
            .join(".minicode")
            .join("swarms")
            .join(&plan.id);
        fs::create_dir_all(&swarm_dir)?;

        let artifacts_root = swarm_dir.join("artifacts");
        fs::create_dir_all(&artifacts_root)?;

        // Save plan to disk
        let plan_json = serde_json::to_string_pretty(&plan)?;
        fs::write(swarm_dir.join("plan.json"), plan_json)?;

        let concurrency = options.max_workers.clamp(1, 16);
        let semaphore = Arc::new(Semaphore::new(concurrency));
        let state = Arc::new(Mutex::new(SwarmExecutionState::new(&plan)));

        info!(
            swarm_id = %plan.id,
            tasks = plan.tasks.len(),
            concurrency = concurrency,
            "Starting dependency-aware Swarm DAG execution"
        );

        let mut join_set = tokio::task::JoinSet::new();

        // Execution loop: continuously dispatch newly ready tasks until all complete or are blocked
        loop {
            if cancel_token.is_cancelled() {
                warn!(swarm_id = %plan.id, "Swarm execution cancelled by user");
                let mut st = state.lock().await;
                for status in st.task_statuses.values_mut() {
                    if *status == SwarmTaskStatus::Pending || *status == SwarmTaskStatus::Ready {
                        *status = SwarmTaskStatus::Cancelled;
                    }
                }
                break;
            }

            // Find all tasks that are currently Ready to be scheduled
            let ready_tasks: Vec<SwarmTaskSpec> = {
                let mut st = state.lock().await;
                let ready_ids: Vec<String> = st
                    .task_statuses
                    .iter()
                    .filter(|(_, &status)| status == SwarmTaskStatus::Ready)
                    .map(|(id, _)| id.clone())
                    .collect();

                for id in &ready_ids {
                    st.task_statuses
                        .insert(id.clone(), SwarmTaskStatus::Running);
                }

                ready_ids
                    .into_iter()
                    .filter_map(|id| plan.get_task(&id).cloned())
                    .collect()
            };

            // Spawn ready tasks into JoinSet
            for task in ready_tasks {
                let ws_root = workspace_root.to_path_buf();
                let art_root = artifacts_root.clone();
                let sem = Arc::clone(&semaphore);
                let token = cancel_token.clone();
                let plan_ref = plan.clone();

                join_set.spawn(async move {
                    let outcome =
                        Self::run_worker_task(&ws_root, &art_root, &plan_ref, task, sem, token)
                            .await;
                    outcome
                });
            }

            // If JoinSet is empty and no tasks are running, we're done
            if join_set.is_empty() {
                let st = state.lock().await;
                if st.is_complete() {
                    break;
                }
                // Check if any pending tasks exist but cannot run (e.g. all remaining are blocked)
                let pending_or_ready = st.task_statuses.values().any(|&s| {
                    s == SwarmTaskStatus::Pending
                        || s == SwarmTaskStatus::Ready
                        || s == SwarmTaskStatus::Running
                });
                if !pending_or_ready {
                    break;
                }
            }

            // Await next worker completion
            if let Some(join_res) = join_set.join_next().await {
                match join_res {
                    Ok(outcome) => {
                        info!(
                            task_id = %outcome.task_id,
                            status = %outcome.status.badge(),
                            duration_ms = outcome.duration_ms,
                            "Swarm worker task finished"
                        );

                        let mut st = state.lock().await;
                        st.task_statuses
                            .insert(outcome.task_id.clone(), outcome.status);
                        st.total_tokens += outcome.tokens_used;
                        st.outcomes.insert(outcome.task_id.clone(), outcome);

                        // Evaluate and unblock newly ready downstream tasks
                        let newly_ready = st.evaluate_ready_tasks(&plan);
                        if !newly_ready.is_empty() {
                            info!(
                                newly_ready = ?newly_ready,
                                "Dependencies satisfied; unblocked downstream swarm tasks"
                            );
                        }
                    }
                    Err(join_err) => {
                        error!(error = %join_err, "Swarm worker task panicked or failed to join");
                    }
                }
            }
        }

        // Finalize state
        let total_duration = start_time.elapsed().as_millis() as u64;
        let mut final_state = {
            let mut st = state.lock().await;
            st.end_time = Some(chrono::Utc::now().to_rfc3339());
            st.total_duration_ms = Some(total_duration);
            st.is_running = false;
            st.clone()
        };

        // Merge arbitration if auto-merge is active
        let merge_summary = if options.auto_merge {
            match SwarmArbitrator::arbitrate_and_merge(workspace_root, &plan, &mut final_state)
                .await
            {
                Ok(summary) => Some(summary),
                Err(e) => {
                    warn!(error = %e, "Swarm merge arbitration failed");
                    Some(format!("Merge Arbitration Error: {}", e))
                }
            }
        } else {
            None
        };

        // Save state and executive report
        let state_json = serde_json::to_string_pretty(&final_state)?;
        fs::write(swarm_dir.join("state.json"), state_json)?;

        let report_path = SwarmReporter::save_report(
            workspace_root,
            &plan,
            &final_state,
            merge_summary.as_deref(),
        )?;

        info!(
            swarm_id = %plan.id,
            total_duration_ms = total_duration,
            report = ?report_path,
            "Swarm execution completed"
        );

        Ok((final_state, report_path))
    }

    /// Executes an individual worker task inside an isolated Git worktree.
    async fn run_worker_task(
        workspace_root: &Path,
        artifacts_root: &Path,
        plan: &SwarmPlan,
        task: SwarmTaskSpec,
        semaphore: Arc<Semaphore>,
        cancel_token: CancellationToken,
    ) -> SwarmTaskOutcome {
        let task_start = Instant::now();

        // 1. Acquire concurrency permit
        let _permit = tokio::select! {
            _ = cancel_token.cancelled() => {
                return Self::cancelled_outcome(&task);
            }
            permit_res = semaphore.acquire() => {
                match permit_res {
                    Ok(p) => p,
                    Err(_) => {
                        return Self::failed_outcome(&task, 0, "Concurrency semaphore closed".to_string());
                    }
                }
            }
        };

        if cancel_token.is_cancelled() {
            return Self::cancelled_outcome(&task);
        }

        // 2. Setup isolated workspace worktree
        let agent_id = AgentId::new_subagent(&task.id);
        let branch_name = format!("swarm/{}/{}", plan.id, task.id);

        let (worktree_handle, target_dir) = match GitWorktreeManager::create_worktree(
            workspace_root,
            &agent_id,
        ) {
            Ok(handle) => {
                let path = handle.worktree_path.clone();
                (Some(handle), path)
            }
            Err(e) => {
                warn!(
                    task_id = %task.id,
                    error = %e,
                    "Failed to create Git worktree for swarm worker; falling back to shared workspace"
                );
                (None, workspace_root.to_path_buf())
            }
        };

        // 3. Collect upstream dependency artifacts to inject into worker prompt
        let mut upstream_artifact_context = String::new();
        for dep_id in &task.dependencies {
            let dep_art_dir = artifacts_root.join(dep_id);
            if dep_art_dir.exists() {
                if let Ok(entries) = fs::read_dir(&dep_art_dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.is_file() {
                            if let Ok(content) = fs::read_to_string(&path) {
                                let filename =
                                    path.file_name().unwrap_or_default().to_string_lossy();
                                upstream_artifact_context.push_str(&format!(
                                    "\n<upstream_artifact source_task=\"{}\" file=\"{}\">\n{}\n</upstream_artifact>\n",
                                    dep_id, filename, content
                                ));
                            }
                        }
                    }
                }
            }
        }

        // 4. Construct enriched worker prompt
        let mut enriched_prompt = task.prompt.clone();
        if !task.instructions.is_empty() {
            enriched_prompt = format!(
                "Role Instructions ({}):\n{}\n\nTask:\n{}",
                task.role_title, task.instructions, enriched_prompt
            );
        }
        if !upstream_artifact_context.is_empty() {
            enriched_prompt.push_str("\n\n### UPSTREAM DEPENDENCY ARTIFACTS:\n");
            enriched_prompt.push_str(&upstream_artifact_context);
        }
        if !task.file_boundaries.is_empty() {
            enriched_prompt.push_str(&format!(
                "\n\nConstraint: Restrict all edits strictly within these paths: `{}`",
                task.file_boundaries.join("`, `")
            ));
        }

        // 5. Spawn child minicode process
        let current_exe = match std::env::current_exe() {
            Ok(exe) => exe,
            Err(e) => {
                if let Some(ref handle) = worktree_handle {
                    let _ = GitWorktreeManager::remove_worktree(handle);
                }
                return Self::failed_outcome(
                    &task,
                    0,
                    format!("Failed to find current executable: {}", e),
                );
            }
        };

        let mut cmd = Command::new(current_exe);
        cmd.arg("run");
        cmd.arg("-d").arg(&target_dir);
        cmd.arg("-y");
        cmd.arg("--json-stream");

        if let Some(max_iter) = task.max_iterations {
            cmd.arg("--max-iterations").arg(max_iter.to_string());
        }

        cmd.arg(&enriched_prompt);
        cmd.stdin(Stdio::null());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        cmd.kill_on_drop(true);

        #[cfg(unix)]
        {
            cmd.process_group(0);
        }

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                if let Some(ref handle) = worktree_handle {
                    let _ = GitWorktreeManager::remove_worktree(handle);
                }
                return Self::failed_outcome(
                    &task,
                    0,
                    format!("Failed to spawn child worker: {}", e),
                );
            }
        };

        let child_id = child.id().unwrap_or(0);

        // Register in MiniDevRegistry so it appears in F7 /tasks and Activity Drawer
        let dev_registry = crate::dev::registry::get_global_dev_registry();
        let dev_id = crate::dev::models::DevProcessId::from(format!("swarm-worker-{}", task.id));
        let worker_label = format!("Swarm ({}) - {}", task.role_title, task.title);
        let token_cancel_hook = cancel_token.clone();

        let _dev_handle = dev_registry
            .register_worker(
                dev_id,
                worker_label,
                format!("minicode swarm worker: {}", task.id),
                target_dir.clone(),
                child_id,
                child_id,
                Some(Arc::new(move || {
                    token_cancel_hook.cancel();
                })),
            )
            .await;

        // Stream and capture output
        let stdout = match child.stdout.take() {
            Some(out) => out,
            None => {
                if let Some(ref handle) = worktree_handle {
                    let _ = GitWorktreeManager::remove_worktree(handle);
                }
                return Self::failed_outcome(
                    &task,
                    0,
                    "Failed to capture child stdout".to_string(),
                );
            }
        };

        let mut reader = BufReader::new(stdout).lines();
        let mut full_output = String::new();

        while let Ok(Some(line)) = reader.next_line().await {
            full_output.push_str(&line);
            full_output.push('\n');
        }

        let status = match child.wait().await {
            Ok(s) => s,
            Err(e) => {
                if let Some(ref handle) = worktree_handle {
                    let _ = GitWorktreeManager::remove_worktree(handle);
                }
                return Self::failed_outcome(
                    &task,
                    task_start.elapsed().as_millis() as u64,
                    e.to_string(),
                );
            }
        };

        let duration_ms = task_start.elapsed().as_millis() as u64;
        let worker_success = status.success();

        // 6. Automated Verification check (if check_command specified)
        let (verification_passed, verif_cmd_str) = match &task.check_command {
            Some(cmd) => {
                let report = MergeArbitrator::verify_worktree(&target_dir, Some(cmd));
                match report {
                    Ok(val) => (val.success, Some(cmd.clone())),
                    Err(_) => (false, Some(cmd.clone())),
                }
            }
            None => (worker_success, None),
        };

        // 7. Collect modified files from git worktree
        let modified_files = match std::process::Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(&target_dir)
            .output()
        {
            Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout)
                .lines()
                .filter_map(|l| {
                    let trimmed = l.trim();
                    if trimmed.len() > 3 {
                        Some(trimmed[3..].trim().to_string())
                    } else {
                        None
                    }
                })
                .collect(),
            _ => Vec::new(),
        };

        // 8. Publish expected artifacts to artifacts root
        let mut published_artifacts = HashMap::new();
        let task_art_dir = artifacts_root.join(&task.id);
        let _ = fs::create_dir_all(&task_art_dir);

        for rel_path in &task.expected_artifacts {
            let src = target_dir.join(rel_path);
            if src.exists() && src.is_file() {
                let dest = task_art_dir.join(Path::new(rel_path).file_name().unwrap_or_default());
                if fs::copy(&src, &dest).is_ok() {
                    if let Ok(preview) = fs::read_to_string(&src) {
                        let snippet = preview.chars().take(500).collect::<String>();
                        published_artifacts.insert(rel_path.clone(), snippet);
                    }
                }
            }
        }

        let overall_status = if worker_success && verification_passed {
            SwarmTaskStatus::Completed
        } else {
            SwarmTaskStatus::Failed
        };

        SwarmTaskOutcome {
            task_id: task.id,
            role_title: task.role_title,
            status: overall_status,
            duration_ms,
            tokens_used: 0,
            files_modified: modified_files,
            worktree_path: Some(target_dir),
            branch_name: Some(branch_name),
            verification_command: verif_cmd_str,
            verification_passed,
            summary: full_output
                .lines()
                .rev()
                .take(10)
                .collect::<Vec<_>>()
                .join("\n"),
            error: if !worker_success || !verification_passed {
                Some(
                    "Worker failed or verification command returned non-zero exit code".to_string(),
                )
            } else {
                None
            },
            generated_artifacts: published_artifacts,
        }
    }

    fn cancelled_outcome(task: &SwarmTaskSpec) -> SwarmTaskOutcome {
        SwarmTaskOutcome {
            task_id: task.id.clone(),
            role_title: task.role_title.clone(),
            status: SwarmTaskStatus::Cancelled,
            duration_ms: 0,
            tokens_used: 0,
            files_modified: Vec::new(),
            worktree_path: None,
            branch_name: None,
            verification_command: task.check_command.clone(),
            verification_passed: false,
            summary: "Task cancelled before execution".to_string(),
            error: Some("Cancelled".to_string()),
            generated_artifacts: HashMap::new(),
        }
    }

    fn failed_outcome(
        task: &SwarmTaskSpec,
        duration_ms: u64,
        error_msg: String,
    ) -> SwarmTaskOutcome {
        SwarmTaskOutcome {
            task_id: task.id.clone(),
            role_title: task.role_title.clone(),
            status: SwarmTaskStatus::Failed,
            duration_ms,
            tokens_used: 0,
            files_modified: Vec::new(),
            worktree_path: None,
            branch_name: None,
            verification_command: task.check_command.clone(),
            verification_passed: false,
            summary: format!("Execution failed: {}", error_msg),
            error: Some(error_msg),
            generated_artifacts: HashMap::new(),
        }
    }
}

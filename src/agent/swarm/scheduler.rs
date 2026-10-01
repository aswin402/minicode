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
    pub model: Option<String>,
    pub provider: Option<String>,
}

impl Default for SwarmRunOptions {
    fn default() -> Self {
        Self {
            max_workers: 4,
            auto_merge: true,
            json_stream: false,
            model: None,
            provider: None,
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

        // Initialize SwarmMessageBus
        let _bus = crate::agent::swarm::bus::SwarmMessageBus::new(&swarm_dir)?;

        let artifacts_root = swarm_dir.join("artifacts");
        fs::create_dir_all(&artifacts_root)?;

        // Save plan to disk
        let plan_json = serde_json::to_string_pretty(&plan)?;
        fs::write(swarm_dir.join("plan.json"), plan_json)?;

        let concurrency = options.max_workers.clamp(1, 16);
        let semaphore = Arc::new(Semaphore::new(concurrency));
        let state = Arc::new(Mutex::new(SwarmExecutionState::new(&plan)));
        let run_options = Arc::new(options.clone());

        // Register parent swarm orchestrator into MiniDevRegistry
        let dev_registry = crate::dev::registry::get_global_dev_registry();
        let swarm_dev_id = crate::dev::models::DevProcessId::from(format!("swarm-{}", plan.id));
        let cancel_hook_token = cancel_token.clone();
        let parent_handle = dev_registry
            .register_swarm_process(
                swarm_dev_id,
                format!("Swarm DAG: {}", plan.title),
                format!("minicode swarm run --auto-merge {}", plan.title),
                workspace_root.to_path_buf(),
                std::process::id(),
                std::process::id(),
                Some(Arc::new(move || {
                    cancel_hook_token.cancel();
                })),
            )
            .await;

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
                parent_handle
                    .update_status(crate::dev::models::DevProcessStatus::Killed)
                    .await;
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

            let ready_ids_list = ready_tasks.iter().map(|t| t.id.clone()).collect::<Vec<_>>();

            // Spawn ready tasks into JoinSet
            for task in ready_tasks {
                let ws_root = workspace_root.to_path_buf();
                let art_root = artifacts_root.clone();
                let sw_dir = swarm_dir.clone();
                let sem = Arc::clone(&semaphore);
                let token = cancel_token.clone();
                let plan_ref = plan.clone();
                let opts = Arc::clone(&run_options);
                let wave_peers = ready_ids_list.clone();
                let parent_ref = Arc::clone(&parent_handle);

                join_set.spawn(async move {
                    let outcome = Self::run_worker_task(
                        &ws_root,
                        &sw_dir,
                        &art_root,
                        &plan_ref,
                        task,
                        &wave_peers,
                        sem,
                        token,
                        opts,
                        Some(parent_ref),
                    )
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

        // Clean up ephemeral worktrees if auto-merge was active
        if options.auto_merge {
            let _ = GitWorktreeManager::cleanup_stale_worktrees(workspace_root);
        }

        // Save state and executive report
        let state_json = serde_json::to_string_pretty(&final_state)?;
        fs::write(swarm_dir.join("state.json"), state_json)?;

        let report_path = SwarmReporter::save_report(
            workspace_root,
            &plan,
            &final_state,
            merge_summary.as_deref(),
        )?;

        parent_handle
            .update_status(crate::dev::models::DevProcessStatus::Stopped)
            .await;

        info!(
            swarm_id = %plan.id,
            total_duration_ms = total_duration,
            report = ?report_path,
            "Swarm execution completed"
        );

        Ok((final_state, report_path))
    }

    /// Executes an individual worker task inside an isolated Git worktree.
    #[allow(clippy::too_many_arguments)]
    async fn run_worker_task(
        workspace_root: &Path,
        swarm_dir: &Path,
        artifacts_root: &Path,
        plan: &SwarmPlan,
        task: SwarmTaskSpec,
        active_wave_peers: &[String],
        semaphore: Arc<Semaphore>,
        cancel_token: CancellationToken,
        options: Arc<SwarmRunOptions>,
        parent_handle: Option<Arc<crate::dev::process::DevProcessHandle>>,
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

        let (worktree_handle, target_dir, active_branch) = match GitWorktreeManager::create_worktree(
            workspace_root,
            &agent_id,
        ) {
            Ok(handle) => {
                let path = handle.worktree_path.clone();
                let bname = handle.branch_name.clone();
                (Some(handle), path, Some(bname))
            }
            Err(e) => {
                warn!(
                    task_id = %task.id,
                    error = %e,
                    "Failed to create Git worktree for swarm worker; falling back to shared workspace"
                );
                (None, workspace_root.to_path_buf(), None)
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

        let peers_str = Self::compute_active_peers(active_wave_peers, &task.id);

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
        if !peers_str.is_empty() {
            enriched_prompt.push_str(&format!(
                "\n\n### SWARM PEER COORDINATION:\nYou are worker `{}` running in swarm `{}`.\nConcurrent peers active in this wave: `{}`.\nTo coordinate interfaces, types, or dependencies with peers, call the `send_worker_message` tool (`publish_contract`, `query_interface`, `coordination_note`). When you call `send_worker_message`, your message is placed onto the durable swarm bus and automatically delivered to peers at turn start.\n",
                task.id, plan.id, peers_str
            ));
        }
        enriched_prompt.push_str(
            "\n\nConstraint: Do NOT modify shared project documentation, task trackers, or scaffolding manifests (e.g. minikit.json, onpkg.json, AGENTS.md, minikit_docs/). Confine all changes strictly to your assigned code and test files to guarantee clean mergeability.",
        );

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

        if let Some(ref m) = options.model {
            cmd.arg("-m").arg(m);
        }
        if let Some(ref p) = options.provider {
            cmd.arg("-p").arg(p);
        }

        if let Some(max_iter) = task.max_iterations {
            cmd.arg("--max-iterations").arg(max_iter.to_string());
        }

        cmd.env("MINICODE_SWARM_ID", &plan.id);
        cmd.env("MINICODE_SWARM_DIR", swarm_dir);
        cmd.env("MINICODE_SWARM_TASK_ID", &task.id);
        cmd.env("MINICODE_SWARM_PEERS", &peers_str);

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

        let dev_handle = dev_registry
            .register_swarm_process(
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
                dev_handle
                    .update_status(crate::dev::models::DevProcessStatus::Exited(Some(1)))
                    .await;
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
            dev_handle.append_log(&line).await;
            if let Some(ref parent) = parent_handle {
                parent.append_log(format!("[{}] {}", task.id, &line)).await;
            }
            full_output.push_str(&line);
            full_output.push('\n');
        }

        let status = match child.wait().await {
            Ok(s) => s,
            Err(e) => {
                dev_handle
                    .update_status(crate::dev::models::DevProcessStatus::Killed)
                    .await;
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

        if worker_success {
            dev_handle
                .update_status(crate::dev::models::DevProcessStatus::Stopped)
                .await;
        } else {
            dev_handle
                .update_status(crate::dev::models::DevProcessStatus::Exited(status.code()))
                .await;
        }

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

        // 7. Collect modified files from git worktree and commit them to the worker branch
        // Ensure common ephemeral/compiled artifacts are ignored to prevent index pollution
        let gitignore_path = target_dir.join(".gitignore");
        let ignore_rules =
            "\n__pycache__/\n*.py[cod]\n*$py.class\nnode_modules/\ntarget/\n.minicode/\n";
        if gitignore_path.exists() {
            if let Ok(mut content) = fs::read_to_string(&gitignore_path) {
                if !content.contains("__pycache__") {
                    content.push_str(ignore_rules);
                    let _ = fs::write(&gitignore_path, content);
                }
            }
        } else {
            let _ = fs::write(&gitignore_path, ignore_rules);
        }

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

        if !modified_files.is_empty() && worktree_handle.is_some() {
            let _ = std::process::Command::new("git")
                .args(["add", "-A"])
                .current_dir(&target_dir)
                .output();
            let commit_msg = format!(
                "feat(swarm/{}): worker `{}` ({})",
                plan.id, task.id, task.title
            );
            let _ = std::process::Command::new("git")
                .args(["commit", "-m", &commit_msg])
                .current_dir(&target_dir)
                .output();
        }

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
            branch_name: active_branch,
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

    /// Computes comma-delimited active peer task IDs for a given task within a wave.
    #[allow(dead_code)]
    pub fn compute_active_peers(all_peers: &[String], current_task_id: &str) -> String {
        all_peers
            .iter()
            .filter(|id| id.as_str() != current_task_id)
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join(",")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_active_peers() {
        let task_ids = vec!["t1".to_string(), "t2".to_string(), "t3".to_string()];
        let peers_for_t1 = task_ids
            .iter()
            .filter(|id| *id != "t1")
            .cloned()
            .collect::<Vec<_>>()
            .join(",");
        assert_eq!(peers_for_t1, "t2,t3");
    }

    #[test]
    fn test_compute_active_peers_helper() {
        let task_ids = vec!["t1".to_string(), "t2".to_string(), "t3".to_string()];
        assert_eq!(
            SwarmScheduler::compute_active_peers(&task_ids, "t1"),
            "t2,t3"
        );
        assert_eq!(
            SwarmScheduler::compute_active_peers(&task_ids, "t2"),
            "t1,t3"
        );
        assert_eq!(
            SwarmScheduler::compute_active_peers(&task_ids, "t3"),
            "t1,t2"
        );

        // Single worker wave
        assert_eq!(
            SwarmScheduler::compute_active_peers(&["t1".to_string()], "t1"),
            ""
        );

        // Empty list
        assert_eq!(SwarmScheduler::compute_active_peers(&[], "t1"), "");
    }
}

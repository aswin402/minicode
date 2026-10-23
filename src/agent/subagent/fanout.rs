//! Parallel Subagent Swarm Fan-Out & Concurrency Runtime.
//!
//! Provides bounded concurrent execution of subagent swarms across isolated
//! Git worktrees, supporting both `all` and `race` join policies and
//! synthesizing unified map-reduce markdown reports.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

use crate::agent::subagent::mailbox::AgentMailbox;
use crate::agent::subagent::message::{AgentMessage, MessageIntent};
use crate::agent::subagent::types::{AgentId, SubagentConfig, SubagentRole, WorkspaceMode};
use crate::agent::subagent::worker::SubagentWorker;
use crate::error::ToolError;
use crate::sandbox::{ArbitrationError, GitWorktreeManager, MergeArbitrator, WorktreeHandle};

/// Specification for an individual `minipub` worker in a fanout swarm.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FanoutTaskItem {
    #[serde(alias = "prompt", default)]
    pub task: String,
    pub role: SubagentRole,
    #[serde(default)]
    pub workspace_mode: Option<WorkspaceMode>,
    #[serde(default)]
    pub max_iterations: Option<usize>,
    #[serde(default)]
    pub check_cmd: Option<String>,
    #[serde(default)]
    pub todos: Vec<String>,
    #[serde(default)]
    pub file_boundaries: Vec<String>,
}

/// Completion mode for the swarm.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FanoutJoinMode {
    #[default]
    All,
    Race,
}

impl fmt::Display for FanoutJoinMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::All => write!(f, "all"),
            Self::Race => write!(f, "race"),
        }
    }
}

/// Status of worktree merge arbitration for a worker.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MergeStatus {
    NotApplicable,
    Merged {
        commit_hash: Option<String>,
    },
    VerificationFailed {
        command: String,
        exit_code: i32,
        stderr: String,
    },
    Conflict {
        conflicted_files: Vec<String>,
    },
    RetainedUnmerged,
    SkippedCancelled,
}

impl fmt::Display for MergeStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MergeStatus::NotApplicable => write!(f, "Not Applicable (Read-Only)"),
            MergeStatus::Merged { commit_hash } => match commit_hash {
                Some(hash) => write!(f, "Merged ({})", hash),
                None => write!(f, "Merged"),
            },
            MergeStatus::VerificationFailed {
                command, exit_code, ..
            } => {
                write!(
                    f,
                    "Verification Failed (`{}` exited with code {})",
                    command, exit_code
                )
            }
            MergeStatus::Conflict { conflicted_files } => {
                write!(f, "Conflict ({} conflicted files)", conflicted_files.len())
            }
            MergeStatus::RetainedUnmerged => write!(f, "Retained Unmerged"),
            MergeStatus::SkippedCancelled => write!(f, "Skipped / Cancelled"),
        }
    }
}

/// Outcome of an individual worker within the swarm.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct WorkerResult {
    pub agent_id: AgentId,
    pub role: SubagentRole,
    pub task: String,
    pub success: bool,
    pub duration_ms: u64,
    pub tokens_used: usize,
    pub files_modified: Vec<String>,
    pub worktree_path: Option<PathBuf>,
    pub branch_name: Option<String>,
    pub merge_status: MergeStatus,
    pub summary: String,
    pub error: Option<String>,
    pub check_cmd: Option<String>,
}

impl WorkerResult {
    fn cancelled(
        agent_id: AgentId,
        task_item: FanoutTaskItem,
        duration_ms: u64,
        summary: impl Into<String>,
        error: impl Into<String>,
    ) -> Self {
        Self {
            agent_id,
            role: task_item.role,
            task: task_item.task,
            success: false,
            duration_ms,
            tokens_used: 0,
            files_modified: Vec::new(),
            worktree_path: None,
            branch_name: None,
            merge_status: MergeStatus::SkippedCancelled,
            summary: summary.into(),
            error: Some(error.into()),
            check_cmd: task_item.check_cmd,
        }
    }

    fn failed(
        agent_id: AgentId,
        task_item: &FanoutTaskItem,
        duration_ms: u64,
        tokens_used: usize,
        files_modified: Vec<String>,
        summary: impl Into<String>,
        error: impl Into<String>,
    ) -> Self {
        Self {
            agent_id,
            role: task_item.role,
            task: task_item.task.clone(),
            success: false,
            duration_ms,
            tokens_used,
            files_modified,
            worktree_path: None,
            branch_name: None,
            merge_status: MergeStatus::NotApplicable,
            summary: summary.into(),
            error: Some(error.into()),
            check_cmd: task_item.check_cmd.clone(),
        }
    }
}

/// Orchestrator for concurrent subagent swarm execution and map-reduce aggregation.
#[allow(dead_code)]
pub struct FanoutOrchestrator;

#[allow(dead_code)]
impl FanoutOrchestrator {
    /// Concurrently executes a batch of subagent tasks according to join mode and arbitration policy.
    pub async fn execute_fanout(
        workspace_root: &Path,
        tasks: Vec<FanoutTaskItem>,
        join_mode: FanoutJoinMode,
        auto_merge: bool,
        max_concurrency: usize,
    ) -> Result<String, ToolError> {
        if tasks.is_empty() {
            return Ok("No subagent tasks specified for fan-out.".to_string());
        }

        let start_time = std::time::Instant::now();
        let total_tasks = tasks.len();
        let concurrency = max_concurrency.clamp(
            crate::constants::MIN_PARALLEL_MINIPUBS,
            crate::constants::MAX_PARALLEL_MINIPUBS_CAP,
        );
        let semaphore = Arc::new(tokio::sync::Semaphore::new(concurrency));
        let cancel_token = CancellationToken::new();
        let app_cfg = Arc::new(crate::config::Config::load_for_workspace(workspace_root));

        let mut join_set = tokio::task::JoinSet::new();

        for (idx, task_item) in tasks.iter().cloned().enumerate() {
            let root = workspace_root.to_path_buf();
            let sem = Arc::clone(&semaphore);
            let token = cancel_token.clone();
            let cfg = Arc::clone(&app_cfg);

            join_set.spawn(async move {
                let res =
                    Self::run_single_worker_with_config(&root, task_item, sem, token, &cfg).await;
                (idx, res)
            });
        }

        let mut results: Vec<Option<WorkerResult>> = (0..total_tasks).map(|_| None).collect();

        while let Some(join_res) = join_set.join_next().await {
            match join_res {
                Ok((idx, Ok(worker_result))) => {
                    if join_mode == FanoutJoinMode::Race
                        && worker_result.success
                        && !cancel_token.is_cancelled()
                    {
                        cancel_token.cancel();
                    }
                    results[idx] = Some(worker_result);
                }
                Ok((idx, Err(e))) => {
                    tracing::error!(task_index = idx, error = %e, "Worker failed with error");
                    let task_item = &tasks[idx];
                    results[idx] = Some(WorkerResult::failed(
                        AgentId::new_subagent(task_item.role.as_str()),
                        task_item,
                        0,
                        0,
                        Vec::new(),
                        format!("Worker failed to execute: {}", e),
                        e.to_string(),
                    ));
                }
                Err(e) => {
                    tracing::error!(error = %e, "Task join error in fanout JoinSet");
                }
            }
        }

        let mut completed_results: Vec<WorkerResult> = results.into_iter().flatten().collect();

        if auto_merge {
            Self::arbitrate_mutating_workers(workspace_root, &mut completed_results, &tasks).await;
        }

        let total_duration_ms = start_time.elapsed().as_millis() as u64;
        let report = Self::format_fanout_report(
            &completed_results,
            join_mode,
            auto_merge,
            total_duration_ms,
        );
        Ok(report)
    }

    /// Executes an individual `minipub` worker in-process under semaphore bounds and cancellation control.
    pub async fn run_single_worker(
        workspace_root: &Path,
        task_item: FanoutTaskItem,
        semaphore: Arc<tokio::sync::Semaphore>,
        cancel_token: CancellationToken,
    ) -> Result<WorkerResult, ToolError> {
        let app_cfg = crate::config::Config::load_for_workspace(workspace_root);
        Self::run_single_worker_with_config(
            workspace_root,
            task_item,
            semaphore,
            cancel_token,
            &app_cfg,
        )
        .await
    }

    async fn run_single_worker_with_config(
        workspace_root: &Path,
        task_item: FanoutTaskItem,
        semaphore: Arc<tokio::sync::Semaphore>,
        cancel_token: CancellationToken,
        app_cfg: &crate::config::Config,
    ) -> Result<WorkerResult, ToolError> {
        let agent_id = AgentId::new_subagent(task_item.role.as_str());

        // 1. Acquire concurrency permit, respecting early cancellation
        let _permit = tokio::select! {
            _ = cancel_token.cancelled() => {
                return Ok(WorkerResult::cancelled(
                    agent_id,
                    task_item,
                    0,
                    "Task cancelled before acquiring concurrency permit.",
                    "Cancelled",
                ));
            }
            permit_res = semaphore.acquire() => {
                match permit_res {
                    Ok(p) => p,
                    Err(_) => {
                        return Err(ToolError::ExecutionFailed(
                            "Concurrency semaphore closed".to_string(),
                        ));
                    }
                }
            }
        };

        // 2. Check if cancelled before provisioning workspace
        if cancel_token.is_cancelled() {
            return Ok(WorkerResult::cancelled(
                agent_id,
                task_item,
                0,
                "Task cancelled before execution.",
                "Cancelled",
            ));
        }

        let mut config =
            SubagentConfig::for_role_with_minipub_config(task_item.role, &app_cfg.agent.minipub)
                .with_todos(task_item.todos.clone())
                .with_file_boundaries(task_item.file_boundaries.clone());

        if let Some(max_iter) = task_item.max_iterations {
            config.max_turns = max_iter.max(1);
        }

        // 3. Determine workspace isolation mode (with cooperative file-boundary lease support)
        let mut should_isolate = match task_item.workspace_mode {
            Some(WorkspaceMode::Worktree) => true,
            Some(WorkspaceMode::Shared) => false,
            Some(WorkspaceMode::Auto) | None => {
                match app_cfg
                    .agent
                    .minipub
                    .worktree_isolation
                    .trim()
                    .to_ascii_lowercase()
                    .as_str()
                {
                    "always" | "worktree" | "true" => true,
                    "never" | "shared" | "false" => false,
                    _ => task_item.role.default_workspace_mode() == WorkspaceMode::Worktree,
                }
            }
        };

        let mut acquired_lease = false;
        if !config.file_boundaries.is_empty() && !should_isolate {
            match crate::sandbox::worktree::FileLeaseManager::acquire_leases(
                workspace_root,
                &agent_id.0,
                &config.file_boundaries,
            ) {
                Ok(()) => acquired_lease = true,
                Err(_) => should_isolate = true,
            }
        }

        let (worktree_handle, target_dir) = if should_isolate {
            match GitWorktreeManager::create_worktree(workspace_root, &agent_id) {
                Ok(handle) => {
                    let path = handle.worktree_path.clone();
                    (Some(handle), path)
                }
                Err(e) => {
                    tracing::warn!(
                        agent_id = %agent_id,
                        error = %e,
                        "Worktree creation failed or repository is not Git; falling back to workspace root"
                    );
                    (None, workspace_root.to_path_buf())
                }
            }
        } else {
            (None, workspace_root.to_path_buf())
        };

        // 4. Initialize child agent mailbox directory
        let child_agent_dir = workspace_root
            .join(".minicode")
            .join("agents")
            .join(&agent_id.0);
        if let Ok(child_mailbox) = AgentMailbox::new(agent_id.clone(), &child_agent_dir) {
            let init_msg = AgentMessage::new(
                AgentId::parent(),
                agent_id.clone(),
                MessageIntent::TaskInit,
                &task_item.task,
            );
            let _ = child_mailbox.post(init_msg);
        }

        // 5. Register in MiniTask dev registry for live telemetry & supervision
        let dev_registry = crate::dev::registry::get_global_dev_registry();
        let dev_id = crate::dev::models::DevProcessId::from(format!("worker-{}", agent_id));
        let worker_name = format!(
            "MiniPub ({}) - {}",
            task_item.role.badge(),
            task_item.task.chars().take(40).collect::<String>()
        );
        let token_cancel_hook = cancel_token.clone();
        let pid_val = std::process::id();

        let dev_handle = dev_registry
            .register_worker(
                dev_id,
                worker_name,
                format!("minipub:{} ({})", agent_id, task_item.role.badge()),
                target_dir.clone(),
                pid_val,
                pid_val,
                Some(Arc::new(move || {
                    token_cancel_hook.cancel();
                })),
            )
            .await;

        let pool = crate::agent::subagent::get_global_subagent_pool(workspace_root);
        let provider = pool.get_or_create_provider().await;

        let worker = SubagentWorker::new(
            agent_id.0.clone(),
            task_item.task.clone(),
            config,
            &target_dir,
        );
        {
            let mut info_lock = worker.info.write().await;
            info_lock.isolate_worktree = worktree_handle.is_some();
        }
        pool.register_worker_handle(
            agent_id.0.clone(),
            Arc::clone(&worker.info),
            Arc::clone(&worker.cancel_flag),
        )
        .await;

        let start_time = std::time::Instant::now();
        let exec_outcome = tokio::select! {
            _ = cancel_token.cancelled() => {
                worker.cancel_flag.store(true, std::sync::atomic::Ordering::SeqCst);
                dev_handle
                    .update_status(crate::dev::models::DevProcessStatus::Stopped)
                    .await;
                if acquired_lease {
                    crate::sandbox::worktree::FileLeaseManager::release_leases(
                        workspace_root,
                        &agent_id.0,
                    );
                }
                if let Some(ref handle) = worktree_handle {
                    let _ = GitWorktreeManager::remove_worktree(handle);
                }
                pool.notify_completion();
                let duration_ms = start_time.elapsed().as_millis() as u64;
                return Ok(WorkerResult::cancelled(
                    agent_id,
                    task_item,
                    duration_ms,
                    "Worker execution was cancelled.",
                    "Worker cancelled",
                ));
            }
            res = worker.run(provider) => res,
        };

        if acquired_lease {
            crate::sandbox::worktree::FileLeaseManager::release_leases(workspace_root, &agent_id.0);
        }
        pool.notify_completion();

        let duration_ms = start_time.elapsed().as_millis() as u64;

        match exec_outcome {
            Ok(sub_res) if sub_res.success => {
                dev_handle
                    .append_log(format!(
                        "[{}] MiniPub finished successfully (tokens: {})",
                        agent_id, sub_res.tokens_used
                    ))
                    .await;
                dev_handle
                    .update_status(crate::dev::models::DevProcessStatus::Exited(Some(0)))
                    .await;

                let mut final_response = sub_res.final_summary.clone();
                let (wt_path, br_name) = if let Some(ref handle) = worktree_handle {
                    let diff = GitWorktreeManager::capture_diff(handle).unwrap_or_default();
                    if !diff.is_empty() {
                        final_response
                            .push_str(&format!("\n\n### Worktree Diff\n```diff\n{}\n```", diff));
                    }
                    let _ = GitWorktreeManager::commit_uncommitted_changes(
                        &handle.worktree_path,
                        &agent_id,
                    );
                    (
                        Some(handle.worktree_path.clone()),
                        Some(handle.branch_name.clone()),
                    )
                } else {
                    (None, None)
                };

                let merge_status = if wt_path.is_some() {
                    MergeStatus::RetainedUnmerged
                } else {
                    MergeStatus::NotApplicable
                };

                Ok(WorkerResult {
                    agent_id,
                    role: task_item.role,
                    task: task_item.task,
                    success: true,
                    duration_ms,
                    tokens_used: sub_res.tokens_used,
                    files_modified: sub_res.files_modified,
                    worktree_path: wt_path,
                    branch_name: br_name,
                    merge_status,
                    summary: final_response.trim().to_string(),
                    error: None,
                    check_cmd: task_item.check_cmd,
                })
            }
            other => {
                if let Some(ref handle) = worktree_handle {
                    let _ = GitWorktreeManager::remove_worktree(handle);
                }
                let (log_label, msg, tokens_used, files_modified) = match other {
                    Ok(sub_res) => (
                        "failed",
                        sub_res.final_summary,
                        sub_res.tokens_used,
                        sub_res.files_modified,
                    ),
                    Err(e) => ("error", e.to_string(), 0, Vec::new()),
                };
                dev_handle
                    .append_log(format!("[{}] MiniPub {}: {}", agent_id, log_label, msg))
                    .await;
                dev_handle
                    .update_status(crate::dev::models::DevProcessStatus::Degraded(msg.clone()))
                    .await;

                Ok(WorkerResult::failed(
                    agent_id,
                    &task_item,
                    duration_ms,
                    tokens_used,
                    files_modified,
                    msg.trim().to_string(),
                    msg,
                ))
            }
        }
    }

    /// Sequentially validates, conflict-checks, and merges mutating subagent worktrees.
    pub async fn arbitrate_mutating_workers(
        workspace_root: &Path,
        results: &mut [WorkerResult],
        tasks: &[FanoutTaskItem],
    ) {
        for (i, res) in results.iter_mut().enumerate() {
            if !res.success {
                continue;
            }
            let (worktree_path, branch_name) = match (&res.worktree_path, &res.branch_name) {
                (Some(wt), Some(br)) => (wt.clone(), br.clone()),
                _ => continue,
            };

            // 1. Pre-merge verification
            let check_cmd = res
                .check_cmd
                .as_deref()
                .or_else(|| tasks.get(i).and_then(|t| t.check_cmd.as_deref()));
            let v_res = MergeArbitrator::verify_worktree(&worktree_path, check_cmd);
            match v_res {
                Ok(v_rep) if !v_rep.success => {
                    res.merge_status = MergeStatus::VerificationFailed {
                        command: v_rep.command,
                        exit_code: v_rep.exit_code,
                        stderr: v_rep.stderr,
                    };
                    continue;
                }
                Ok(_) => {}
                Err(ArbitrationError::VerificationFailed(msg)) => {
                    res.merge_status = MergeStatus::VerificationFailed {
                        command: check_cmd.unwrap_or("auto").to_string(),
                        exit_code: 1,
                        stderr: msg,
                    };
                    continue;
                }
                Err(e) => {
                    res.merge_status = MergeStatus::VerificationFailed {
                        command: check_cmd.unwrap_or("auto").to_string(),
                        exit_code: 1,
                        stderr: e.to_string(),
                    };
                    continue;
                }
            }

            // Ensure any uncommitted write_file/patch_file edits in the worktree are committed onto branch_name
            let _ = GitWorktreeManager::commit_uncommitted_changes(&worktree_path, &res.agent_id);

            // 2. In-memory 3-way mergeability check against current HEAD
            let m_res = MergeArbitrator::check_mergeability(workspace_root, &branch_name);
            match m_res {
                Ok(report) => {
                    if !report.can_merge_cleanly {
                        res.merge_status = MergeStatus::Conflict {
                            conflicted_files: report.conflicted_files,
                        };
                        continue;
                    }
                }
                Err(e) => {
                    res.merge_status = MergeStatus::Conflict {
                        conflicted_files: vec![format!("Mergeability check error: {}", e)],
                    };
                    continue;
                }
            }

            // 3. Apply clean merge with commit: true
            let commit_msg = format!(
                "merge(subagent): integrate changes from worker `{}` ({})",
                res.agent_id,
                res.role.badge()
            );
            let apply_res =
                MergeArbitrator::apply_merge(workspace_root, &branch_name, true, Some(&commit_msg));
            match apply_res {
                Ok(merge_success) => {
                    res.merge_status = MergeStatus::Merged {
                        commit_hash: merge_success.commit_hash,
                    };
                    let handle = WorktreeHandle {
                        worktree_path: worktree_path.clone(),
                        branch_name: branch_name.clone(),
                        agent_id: res.agent_id.clone(),
                        repo_root: workspace_root.to_path_buf(),
                    };
                    if let Err(e) = GitWorktreeManager::remove_worktree(&handle) {
                        tracing::warn!(
                            agent_id = %res.agent_id,
                            error = %e,
                            "Failed to remove worktree after merge"
                        );
                    }
                }
                Err(ArbitrationError::MergeConflict(files)) => {
                    res.merge_status = MergeStatus::Conflict {
                        conflicted_files: files,
                    };
                }
                Err(e) => {
                    res.merge_status = MergeStatus::Conflict {
                        conflicted_files: vec![e.to_string()],
                    };
                }
            }
        }
    }

    /// Formats an executive map-reduce markdown report summarizing swarm outcomes and arbitration.
    pub fn format_fanout_report(
        results: &[WorkerResult],
        join_mode: FanoutJoinMode,
        auto_merge: bool,
        total_duration_ms: u64,
    ) -> String {
        let total_workers = results.len();
        let successful_workers = results.iter().filter(|r| r.success).count();
        let cancelled_workers = results
            .iter()
            .filter(|r| r.merge_status == MergeStatus::SkippedCancelled)
            .count();
        let total_tokens: usize = results.iter().map(|r| r.tokens_used).sum();

        let join_mode_str = match join_mode {
            FanoutJoinMode::All => "all (wait for all)",
            FanoutJoinMode::Race => "race (first success wins)",
        };
        let auto_merge_str = if auto_merge {
            "enabled (sequential arbitration)"
        } else {
            "disabled (retained for inspection)"
        };

        let mut out = format!(
            "### 🐝 Subagent Swarm Fan-Out Completed ({} / {} worker(s) finished in {:.2}s)\n\n\
             • **Join Policy**: {}\n\
             • **Auto-Merge**: {}\n\
             • **Aggregate Metrics**: {} tokens used across {} active worker(s)\n\n",
            successful_workers,
            total_workers,
            total_duration_ms as f64 / 1000.0,
            join_mode_str,
            auto_merge_str,
            total_tokens,
            total_workers.saturating_sub(cancelled_workers),
        );

        // Executive Markdown Matrix Table
        out.push_str(
            "| # | Worker ID | Role | Status | Duration | Tokens | Files | Merge Outcome |\n",
        );
        out.push_str("| :-: | :--- | :--- | :-: | :-: | :-: | :-: | :--- |\n");

        for (i, r) in results.iter().enumerate() {
            let status_badge = if r.success {
                "✔ Success"
            } else if r.merge_status == MergeStatus::SkippedCancelled {
                "⏹ Cancelled"
            } else {
                "✗ Failed"
            };

            let duration_str = format!("{:.1}s", r.duration_ms as f64 / 1000.0);
            let files_str = if r.files_modified.is_empty() {
                "None".to_string()
            } else {
                format!("{} file(s)", r.files_modified.len())
            };

            let merge_outcome_str = match &r.merge_status {
                MergeStatus::NotApplicable => "— (read-only)".to_string(),
                MergeStatus::Merged { commit_hash } => {
                    if let Some(ref hash) = commit_hash {
                        format!("✔ Merged (`{}`)", hash)
                    } else {
                        "✔ Merged".to_string()
                    }
                }
                MergeStatus::VerificationFailed {
                    command, exit_code, ..
                } => {
                    format!("❌ Verify Failed (`{}` exit {})", command, exit_code)
                }
                MergeStatus::Conflict { conflicted_files } => {
                    format!("⚠️ Conflict ({} file(s))", conflicted_files.len())
                }
                MergeStatus::RetainedUnmerged => {
                    if let Some(ref path) = r.worktree_path {
                        format!("📁 Retained (`{}`)", path.display())
                    } else {
                        "📁 Retained".to_string()
                    }
                }
                MergeStatus::SkippedCancelled => "⏹ Cancelled".to_string(),
            };

            out.push_str(&format!(
                "| {} | `{}` | **{}** | {} | {} | {} | {} | {} |\n",
                i + 1,
                r.agent_id,
                r.role.badge(),
                status_badge,
                duration_str,
                r.tokens_used,
                files_str,
                merge_outcome_str
            ));
        }

        // Diagnostics section for conflicts or verification failures
        let mut diagnostics = Vec::new();
        for r in results {
            match &r.merge_status {
                MergeStatus::VerificationFailed {
                    command,
                    exit_code,
                    stderr,
                } => {
                    diagnostics.push(format!(
                        "#### ❌ Worker `{}` Pre-Merge Verification Failed\n\
                         • **Command**: `{}`\n\
                         • **Exit Code**: {}\n\
                         • **Worktree Preserved At**: `{}`\n\n\
                         ```\n{}\n```\n",
                        r.agent_id,
                        command,
                        exit_code,
                        r.worktree_path
                            .as_ref()
                            .map(|p| p.display().to_string())
                            .unwrap_or_else(|| "N/A".to_string()),
                        stderr.trim()
                    ));
                }
                MergeStatus::Conflict { conflicted_files } => {
                    diagnostics.push(format!(
                        "#### ⚠️ Worker `{}` Merge Conflicts Detected\n\
                         • **Conflicted Files ({})**:\n{}\n\
                         • **Worktree Preserved At**: `{}`\n\
                         💡 *Changes were NOT applied to parent workspace to avoid corrupting working directory.*\n",
                        r.agent_id,
                        conflicted_files.len(),
                        conflicted_files.iter().map(|f| format!("  - `{}`", f)).collect::<Vec<_>>().join("\n"),
                        r.worktree_path.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| "N/A".to_string())
                    ));
                }
                _ => {}
            }
        }

        if !diagnostics.is_empty() {
            out.push_str("\n### ⚠️ Arbitration Diagnostics & Conflicts:\n\n");
            for diag in diagnostics {
                out.push_str(&diag);
                out.push('\n');
            }
        }

        // Executive Summaries & Findings
        out.push_str("\n### 📋 Executive Summaries & Findings:\n\n");
        for (i, r) in results.iter().enumerate() {
            out.push_str(&format!(
                "#### {}. `{}` — {}\n",
                i + 1,
                r.agent_id,
                r.role.badge()
            ));
            out.push_str(&format!("**Task**: {}\n\n", r.task));
            if let Some(ref e) = r.error {
                out.push_str(&format!("**Error**: {}\n\n", e));
            }
            if !r.summary.is_empty() {
                out.push_str(&format!("{}\n\n", r.summary.trim()));
            }
            if let Some(ref wt) = r.worktree_path {
                out.push_str(&format!("*Worktree*: `{}`\n\n", wt.display()));
            }
            out.push_str("---\n\n");
        }

        out
    }

    /// Synthesizes a baseline map-reduce markdown report summarizing swarm outcomes.
    pub fn format_baseline_report(results: &[WorkerResult], join_mode: FanoutJoinMode) -> String {
        let total = results.len();
        let successful = results.iter().filter(|r| r.success).count();
        let total_tokens: usize = results.iter().map(|r| r.tokens_used).sum();
        let total_duration_ms: u64 = results.iter().map(|r| r.duration_ms).sum();

        let mut report = String::new();
        report.push_str("## Parallel Swarm Fan-Out Execution Report\n\n");
        report.push_str(&format!("- **Join Mode**: `{}`\n", join_mode));
        report.push_str(&format!("- **Tasks Executed**: {}\n", total));
        report.push_str(&format!(
            "- **Successful Workers**: {}/{}\n",
            successful, total
        ));
        report.push_str(&format!("- **Total Tokens Used**: {}\n", total_tokens));
        report.push_str(&format!(
            "- **Aggregate Duration**: {}ms\n\n",
            total_duration_ms
        ));

        report.push_str("### Swarm Worker Outcomes\n\n");
        report.push_str(
            "| Worker ID | Role | Status | Duration | Tokens | Files Modified | Merge Status |\n",
        );
        report.push_str(
            "|-----------|------|--------|----------|--------|----------------|--------------|\n",
        );

        for r in results {
            let status_icon = if r.success {
                "✔ Success"
            } else if r.merge_status == MergeStatus::SkippedCancelled {
                "⏹ Cancelled"
            } else {
                "❌ Failed"
            };
            let files_str = if r.files_modified.is_empty() {
                "None".to_string()
            } else {
                r.files_modified.join(", ")
            };
            report.push_str(&format!(
                "| `{}` | {} | {} | {}ms | {} | {} | {} |\n",
                r.agent_id,
                r.role.badge(),
                status_icon,
                r.duration_ms,
                r.tokens_used,
                files_str,
                r.merge_status,
            ));
        }

        report.push_str("\n### Worker Summaries & Findings\n\n");
        for r in results {
            report.push_str(&format!("#### `[{}]` ({})\n", r.agent_id, r.role.badge()));
            report.push_str(&format!("**Task**: {}\n\n", r.task));
            if let Some(ref err) = r.error {
                report.push_str(&format!("**Error**: {}\n\n", err));
            }
            if !r.summary.is_empty() {
                report.push_str(&format!("{}\n\n", r.summary));
            }
            if let Some(ref wt) = r.worktree_path {
                report.push_str(&format!("*Worktree*: `{}`\n\n", wt.display()));
            }
            report.push_str("---\n\n");
        }

        report
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn test_fanout_join_mode_serialization() {
        let all_mode = FanoutJoinMode::All;
        let json_all = serde_json::to_string(&all_mode).expect("serialize All");
        assert_eq!(json_all, "\"all\"");
        let de_all: FanoutJoinMode = serde_json::from_str(&json_all).expect("deserialize All");
        assert_eq!(de_all, FanoutJoinMode::All);

        let race_mode = FanoutJoinMode::Race;
        let json_race = serde_json::to_string(&race_mode).expect("serialize Race");
        assert_eq!(json_race, "\"race\"");
        let de_race: FanoutJoinMode = serde_json::from_str(&json_race).expect("deserialize Race");
        assert_eq!(de_race, FanoutJoinMode::Race);

        assert_eq!(FanoutJoinMode::default(), FanoutJoinMode::All);
        assert_eq!(format!("{}", FanoutJoinMode::All), "all");
        assert_eq!(format!("{}", FanoutJoinMode::Race), "race");
    }

    #[tokio::test]
    async fn test_fanout_empty_tasks() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let result = FanoutOrchestrator::execute_fanout(
            temp_dir.path(),
            Vec::new(),
            FanoutJoinMode::All,
            false,
            4,
        )
        .await;

        assert!(
            result.is_ok(),
            "execute_fanout should return Ok for empty tasks"
        );
        assert_eq!(
            result.expect("result ok"),
            "No subagent tasks specified for fan-out."
        );
    }

    #[test]
    fn test_fanout_task_item_deserialization() {
        let json_str = r#"{
            "task": "Refactor authentication flow",
            "role": "coder",
            "workspace_mode": "worktree",
            "max_iterations": 10,
            "check_cmd": "cargo test"
        }"#;
        let item: FanoutTaskItem =
            serde_json::from_str(json_str).expect("deserialize FanoutTaskItem");
        assert_eq!(item.task, "Refactor authentication flow");
        assert_eq!(item.role, SubagentRole::Coder);
        assert_eq!(item.workspace_mode, Some(WorkspaceMode::Worktree));
        assert_eq!(item.max_iterations, Some(10));
        assert_eq!(item.check_cmd, Some("cargo test".to_string()));

        // Test alias 'prompt' and omitted defaults
        let json_alias = r#"{
            "prompt": "Inspect database schemas",
            "role": "scout"
        }"#;
        let item_alias: FanoutTaskItem =
            serde_json::from_str(json_alias).expect("deserialize alias prompt");
        assert_eq!(item_alias.task, "Inspect database schemas");
        assert_eq!(item_alias.role, SubagentRole::Scout);
        assert!(item_alias.workspace_mode.is_none());
        assert!(item_alias.max_iterations.is_none());
        assert!(item_alias.check_cmd.is_none());
    }

    #[test]
    fn test_merge_status_variants() {
        let not_app = MergeStatus::NotApplicable;
        let merged_with_hash = MergeStatus::Merged {
            commit_hash: Some("7fa81c0".to_string()),
        };
        let merged_no_hash = MergeStatus::Merged { commit_hash: None };
        let failed = MergeStatus::VerificationFailed {
            command: "cargo test".to_string(),
            exit_code: 101,
            stderr: "assertion failed".to_string(),
        };
        let conflict = MergeStatus::Conflict {
            conflicted_files: vec!["src/main.rs".to_string(), "Cargo.toml".to_string()],
        };
        let retained = MergeStatus::RetainedUnmerged;
        let skipped = MergeStatus::SkippedCancelled;

        assert_eq!(not_app, MergeStatus::NotApplicable);
        assert_eq!(retained, MergeStatus::RetainedUnmerged);
        assert_eq!(skipped, MergeStatus::SkippedCancelled);
        assert_ne!(not_app, retained);
        assert_ne!(retained, skipped);

        assert!(format!("{}", not_app).contains("Not Applicable"));
        assert!(format!("{}", merged_with_hash).contains("7fa81c0"));
        assert!(format!("{}", merged_no_hash).contains("Merged"));
        assert!(format!("{}", failed).contains("cargo test"));
        assert!(format!("{}", failed).contains("101"));
        assert!(format!("{}", conflict).contains("2 conflicted files"));
        assert!(format!("{}", retained).contains("Retained Unmerged"));
        assert!(format!("{}", skipped).contains("Skipped / Cancelled"));
    }

    #[tokio::test]
    async fn test_run_single_worker_cancelled_early() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let sem = Arc::new(tokio::sync::Semaphore::new(1));
        let token = CancellationToken::new();
        token.cancel();

        let task_item = FanoutTaskItem {
            task: "Inspect codebase".to_string(),
            role: SubagentRole::Scout,
            workspace_mode: Some(WorkspaceMode::Shared),
            max_iterations: None,
            check_cmd: None,
            todos: Vec::new(),
            file_boundaries: Vec::new(),
        };

        let res =
            FanoutOrchestrator::run_single_worker(temp_dir.path(), task_item, sem, token).await;

        assert!(
            res.is_ok(),
            "run_single_worker should return Ok with SkippedCancelled"
        );
        let worker_res = res.expect("worker result");
        assert_eq!(worker_res.merge_status, MergeStatus::SkippedCancelled);
        assert!(!worker_res.success);
        assert!(worker_res.summary.contains("cancelled"));
    }

    #[test]
    fn test_worker_result_and_report_formatting() {
        let worker = WorkerResult {
            agent_id: AgentId("coder-test".to_string()),
            role: SubagentRole::Coder,
            task: "Fix parser bug".to_string(),
            success: true,
            duration_ms: 1500,
            tokens_used: 500,
            files_modified: vec!["src/parser.rs".to_string()],
            worktree_path: Some(PathBuf::from("/tmp/wt-1")),
            branch_name: Some("minicode/subagent/coder-test".to_string()),
            merge_status: MergeStatus::RetainedUnmerged,
            summary: "Resolved edge case in token parser.".to_string(),
            error: None,
            check_cmd: None,
        };

        let report = FanoutOrchestrator::format_baseline_report(&[worker], FanoutJoinMode::All);
        assert!(report.contains("coder-test"));
        assert!(report.contains("Fix parser bug"));
        assert!(report.contains("Resolved edge case in token parser."));
        assert!(report.contains("src/parser.rs"));
        assert!(report.contains("Retained Unmerged"));
        assert!(report.contains("Parallel Swarm Fan-Out Execution Report"));
    }

    fn setup_test_repo_with_worktree(
        root: &Path,
        base_file: &str,
        base_content: &str,
        branch: &str,
        wt_name: &str,
        wt_file: &str,
        wt_content: &str,
    ) -> PathBuf {
        for args in [
            &["init"][..],
            &["config", "user.name", "test"],
            &["config", "user.email", "test@example.com"],
        ] {
            std::process::Command::new("git")
                .args(args)
                .current_dir(root)
                .output()
                .expect("git init/config");
        }

        std::fs::write(root.join(base_file), base_content).expect("write base");
        for args in [&["add", "."][..], &["commit", "-m", "initial commit"]] {
            std::process::Command::new("git")
                .args(args)
                .current_dir(root)
                .output()
                .expect("git commit base");
        }

        std::process::Command::new("git")
            .args(["branch", branch])
            .current_dir(root)
            .output()
            .expect("git branch");

        let wt_dir = root.join(wt_name);
        std::process::Command::new("git")
            .args(["worktree", "add", wt_dir.to_str().expect("to_str"), branch])
            .current_dir(root)
            .output()
            .expect("git worktree add");

        std::fs::write(wt_dir.join(wt_file), wt_content).expect("write wt file");
        for args in [&["add", "."][..], &["commit", "-m", "worker commit"]] {
            std::process::Command::new("git")
                .args(args)
                .current_dir(&wt_dir)
                .output()
                .expect("git commit wt file");
        }

        wt_dir
    }

    #[tokio::test]
    async fn test_arbitrate_mutating_workers_clean_merge() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        let branch = "minicode/subagent/worker-1";
        let wt_dir = setup_test_repo_with_worktree(
            root,
            "base.txt",
            "base content\n",
            branch,
            "wt-1",
            "file_a.txt",
            "file_a content\n",
        );

        let mut results = vec![WorkerResult {
            agent_id: AgentId("worker-1".to_string()),
            role: SubagentRole::Coder,
            task: "Add file_a.txt".to_string(),
            success: true,
            duration_ms: 120,
            tokens_used: 150,
            files_modified: vec!["file_a.txt".to_string()],
            worktree_path: Some(wt_dir.clone()),
            branch_name: Some(branch.to_string()),
            merge_status: MergeStatus::RetainedUnmerged,
            summary: "Successfully added file_a.txt".to_string(),
            error: None,
            check_cmd: Some("skip".to_string()),
        }];

        let tasks = vec![FanoutTaskItem {
            task: "Add file_a.txt".to_string(),
            role: SubagentRole::Coder,
            workspace_mode: Some(WorkspaceMode::Worktree),
            max_iterations: None,
            check_cmd: Some("skip".to_string()),
            todos: Vec::new(),
            file_boundaries: Vec::new(),
        }];

        FanoutOrchestrator::arbitrate_mutating_workers(root, &mut results, &tasks).await;

        assert!(
            matches!(results[0].merge_status, MergeStatus::Merged { .. }),
            "Expected worker-1 to be Merged, got: {:?}",
            results[0].merge_status
        );
        assert!(
            root.join("file_a.txt").exists(),
            "file_a.txt should exist in root repo after merge"
        );
        assert!(
            !wt_dir.exists(),
            "Worktree directory should be removed after clean merge"
        );
    }

    #[tokio::test]
    async fn test_arbitrate_mutating_workers_conflict() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        let branch = "minicode/subagent/worker-conflict";
        let wt_dir = setup_test_repo_with_worktree(
            root,
            "shared.txt",
            "base content\n",
            branch,
            "wt-conflict",
            "shared.txt",
            "worker conflicting content\n",
        );

        // Main modifies shared.txt
        std::fs::write(root.join("shared.txt"), "main conflicting content\n")
            .expect("write main shared");
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(root)
            .output()
            .expect("git add main shared");
        std::process::Command::new("git")
            .args(["commit", "-m", "main conflicting commit"])
            .current_dir(root)
            .output()
            .expect("git commit main shared");

        let mut results = vec![WorkerResult {
            agent_id: AgentId("worker-conflict".to_string()),
            role: SubagentRole::Coder,
            task: "Update shared.txt".to_string(),
            success: true,
            duration_ms: 150,
            tokens_used: 120,
            files_modified: vec!["shared.txt".to_string()],
            worktree_path: Some(wt_dir.clone()),
            branch_name: Some(branch.to_string()),
            merge_status: MergeStatus::RetainedUnmerged,
            summary: "Updated shared.txt".to_string(),
            error: None,
            check_cmd: Some("skip".to_string()),
        }];

        let tasks = vec![FanoutTaskItem {
            task: "Update shared.txt".to_string(),
            role: SubagentRole::Coder,
            workspace_mode: Some(WorkspaceMode::Worktree),
            max_iterations: None,
            check_cmd: Some("skip".to_string()),
            todos: Vec::new(),
            file_boundaries: Vec::new(),
        }];

        FanoutOrchestrator::arbitrate_mutating_workers(root, &mut results, &tasks).await;

        match &results[0].merge_status {
            MergeStatus::Conflict { conflicted_files } => {
                assert!(
                    conflicted_files.contains(&"shared.txt".to_string()),
                    "conflicted_files should contain shared.txt: {:?}",
                    conflicted_files
                );
            }
            other => panic!("Expected MergeStatus::Conflict, got: {:?}", other),
        }

        assert!(
            wt_dir.exists(),
            "Worktree directory must be preserved on disk for manual remediation"
        );
    }

    #[tokio::test]
    async fn test_arbitrate_mutating_workers_verification_failed() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        let branch = "minicode/subagent/worker-fail-verify";
        let wt_dir = setup_test_repo_with_worktree(
            root,
            "base.txt",
            "base content\n",
            branch,
            "wt-fail-verify",
            "broken.txt",
            "broken\n",
        );

        let mut results = vec![WorkerResult {
            agent_id: AgentId("worker-fail-verify".to_string()),
            role: SubagentRole::Coder,
            task: "Implement feature".to_string(),
            success: true,
            duration_ms: 100,
            tokens_used: 50,
            files_modified: vec!["broken.txt".to_string()],
            worktree_path: Some(wt_dir.clone()),
            branch_name: Some(branch.to_string()),
            merge_status: MergeStatus::RetainedUnmerged,
            summary: "Completed changes".to_string(),
            error: None,
            check_cmd: Some("false".to_string()),
        }];

        let tasks = vec![FanoutTaskItem {
            task: "Implement feature".to_string(),
            role: SubagentRole::Coder,
            workspace_mode: Some(WorkspaceMode::Worktree),
            max_iterations: None,
            check_cmd: Some("false".to_string()),
            todos: Vec::new(),
            file_boundaries: Vec::new(),
        }];

        FanoutOrchestrator::arbitrate_mutating_workers(root, &mut results, &tasks).await;

        match &results[0].merge_status {
            MergeStatus::VerificationFailed {
                command, exit_code, ..
            } => {
                assert_eq!(command, "false");
                assert_ne!(*exit_code, 0);
            }
            other => panic!("Expected MergeStatus::VerificationFailed, got: {:?}", other),
        }

        assert!(
            wt_dir.exists(),
            "Worktree directory must be preserved on disk when verification fails"
        );
    }

    #[test]
    fn test_format_fanout_report() {
        let results = vec![
            WorkerResult {
                agent_id: AgentId("scout-1".to_string()),
                role: SubagentRole::Scout,
                task: "Audit repo structure".to_string(),
                success: true,
                duration_ms: 800,
                tokens_used: 350,
                files_modified: vec![],
                worktree_path: None,
                branch_name: None,
                merge_status: MergeStatus::NotApplicable,
                summary: "Scouted directories and identified entrypoints.".to_string(),
                error: None,
                check_cmd: None,
            },
            WorkerResult {
                agent_id: AgentId("coder-1".to_string()),
                role: SubagentRole::Coder,
                task: "Implement feature A".to_string(),
                success: true,
                duration_ms: 2200,
                tokens_used: 900,
                files_modified: vec!["src/feature.rs".to_string()],
                worktree_path: Some(PathBuf::from("/tmp/wt-coder-1")),
                branch_name: Some("minicode/subagent/coder-1".to_string()),
                merge_status: MergeStatus::Merged {
                    commit_hash: Some("abc1234".to_string()),
                },
                summary: "Implemented feature A cleanly.".to_string(),
                error: None,
                check_cmd: None,
            },
            WorkerResult {
                agent_id: AgentId("coder-2".to_string()),
                role: SubagentRole::Coder,
                task: "Implement feature B".to_string(),
                success: true,
                duration_ms: 2100,
                tokens_used: 850,
                files_modified: vec!["src/feature.rs".to_string()],
                worktree_path: Some(PathBuf::from("/tmp/wt-coder-2")),
                branch_name: Some("minicode/subagent/coder-2".to_string()),
                merge_status: MergeStatus::Conflict {
                    conflicted_files: vec!["src/feature.rs".to_string()],
                },
                summary: "Implemented feature B with overlapping edits.".to_string(),
                error: None,
                check_cmd: None,
            },
            WorkerResult {
                agent_id: AgentId("coder-3".to_string()),
                role: SubagentRole::Coder,
                task: "Implement feature C".to_string(),
                success: true,
                duration_ms: 1500,
                tokens_used: 400,
                files_modified: vec!["src/bad.rs".to_string()],
                worktree_path: Some(PathBuf::from("/tmp/wt-coder-3")),
                branch_name: Some("minicode/subagent/coder-3".to_string()),
                merge_status: MergeStatus::VerificationFailed {
                    command: "cargo check -j 1".to_string(),
                    exit_code: 101,
                    stderr: "syntax error: expected `;`".to_string(),
                },
                summary: "Broke build during implementation.".to_string(),
                error: None,
                check_cmd: None,
            },
            WorkerResult {
                agent_id: AgentId("tester-1".to_string()),
                role: SubagentRole::Tester,
                task: "Race candidate test".to_string(),
                success: false,
                duration_ms: 300,
                tokens_used: 100,
                files_modified: vec![],
                worktree_path: None,
                branch_name: None,
                merge_status: MergeStatus::SkippedCancelled,
                summary: "Task cancelled due to race completion.".to_string(),
                error: Some("cancelled".to_string()),
                check_cmd: None,
            },
        ];

        let report =
            FanoutOrchestrator::format_fanout_report(&results, FanoutJoinMode::Race, true, 2500);

        assert!(report.contains("Subagent Swarm Fan-Out Completed"));
        assert!(report.contains("race (first success wins)"));
        assert!(report.contains("enabled (sequential arbitration)"));
        assert!(report.contains("✔ Merged (`abc1234`)"));
        assert!(report.contains("⚠️ Conflict (1 file(s))"));
        assert!(report.contains("❌ Verify Failed (`cargo check -j 1` exit 101)"));
        assert!(report.contains("⏹ Cancelled"));
        assert!(report.contains("Arbitration Diagnostics & Conflicts"));
        assert!(report.contains("src/feature.rs"));
        assert!(report.contains("syntax error: expected `;`"));
    }
}

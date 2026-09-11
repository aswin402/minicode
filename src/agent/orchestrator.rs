use crate::agent::subagent::{
    get_global_scratchpad, get_global_subagent_pool, SubAgentResult, SubagentConfig, SubagentRole,
    SubagentTaskSpec,
};
use crate::error::{MinicodeError, Result, ToolError};
use crate::git::worktree::WorktreeManager;
use std::path::Path;

/// Result of an individual subagent fanout worker.
#[derive(Debug, Clone)]
pub struct FanoutWorkerOutcome {
    pub id: String,
    pub role: String,
    pub isolate_worktree: bool,
    pub success: bool,
    pub result: SubAgentResult,
    pub merged: bool,
    pub error: Option<String>,
}

/// Orchestrator for delegating tasks to concurrent or isolated subagents.
pub struct MultiAgentOrchestrator;

impl MultiAgentOrchestrator {
    /// Spawns an autonomous subagent for the given task and returns its structured result.
    pub async fn delegate(
        workspace_root: &Path,
        task_prompt: &str,
        use_worktree: bool,
        timeout_secs: Option<u64>,
    ) -> Result<SubAgentResult> {
        Self::delegate_with_config(
            workspace_root,
            task_prompt,
            use_worktree,
            timeout_secs,
            None,
        )
        .await
    }

    /// Spawns an autonomous subagent with custom configuration.
    pub async fn delegate_with_config(
        workspace_root: &Path,
        task_prompt: &str,
        use_worktree: bool,
        timeout_secs: Option<u64>,
        config: Option<crate::agent::subagent::SubagentConfig>,
    ) -> Result<SubAgentResult> {
        let pool = get_global_subagent_pool(workspace_root);
        let provider = pool.get_or_create_provider().await;
        let role = config
            .as_ref()
            .map(|c| c.role.clone())
            .unwrap_or(SubagentRole::Researcher);
        let timeout = timeout_secs.unwrap_or(crate::constants::DEFAULT_SUBAGENT_TIMEOUT_SECS);

        let fut = pool.run_subagent_with_options(role, task_prompt, config, provider, use_worktree);

        match tokio::time::timeout(std::time::Duration::from_secs(timeout), fut).await {
            Ok(res) => res,
            Err(_) => Err(MinicodeError::Channel(format!(
                "Subagent task timed out after {} seconds",
                timeout
            ))),
        }
    }

    /// Concurrently executes a batch of subagent tasks with optional Git Worktree isolation.
    pub async fn fanout_tasks(
        workspace_root: &Path,
        tasks: Vec<SubagentTaskSpec>,
        wait_for_completion: bool,
        auto_merge: bool,
    ) -> Result<String> {
        if tasks.is_empty() {
            return Ok("No subagent tasks specified for fan-out.".to_string());
        }

        let pool = get_global_subagent_pool(workspace_root);
        let provider = pool.get_or_create_provider().await;
        let mut worker_specs = Vec::new();
        let mut launched_descriptions = Vec::new();

        for task in tasks {
            let role = task.role.clone();
            let prompt = task.prompt.clone();
            let timeout = task.timeout_secs.unwrap_or(120);

            let isolate_worktree = task.isolate_worktree.unwrap_or(!matches!(
                role,
                SubagentRole::Researcher
                    | SubagentRole::CodeReviewer
                    | SubagentRole::SecurityAuditor
            ));

            let mut config = SubagentConfig::for_role(role.clone());
            if let Some(m) = task.model {
                config.model = Some(m);
            }

            let task_id = pool
                .spawn_background_worker(
                    role.clone(),
                    &prompt,
                    Some(config),
                    std::sync::Arc::clone(&provider),
                    isolate_worktree,
                )
                .await?;

            launched_descriptions.push((
                task_id.clone(),
                role.badge().to_string(),
                isolate_worktree,
                prompt.clone(),
            ));

            worker_specs.push((task_id, role, isolate_worktree, timeout));
        }

        if !wait_for_completion {
            let mut out = format!(
                "### Subagent Swarm Fan-Out Launched ({} worker(s) in background)\n\n",
                launched_descriptions.len()
            );
            for (id, badge, isolate, prompt) in launched_descriptions {
                let mode = if isolate {
                    format!("Git Worktree: subagent/{}", id)
                } else {
                    "Shared Read-Only".to_string()
                };
                let short_prompt = crate::utils::truncate_ellipsis(&prompt, 60);
                out.push_str(&format!(
                    "- `{}` (**{}**) [{}]: \"{}\"\n",
                    id, badge, mode, short_prompt
                ));
            }
            out.push_str("\nWorkers are running concurrently in background. Press `Ctrl+S` or run `/swarm` to monitor live telemetry in the Activity Drawer.\n");
            return Ok(out);
        }

        // Wait for all workers to complete
        let scratchpad = get_global_scratchpad();
        let mut completed_results = Vec::new();
        let worktree_mgr = WorktreeManager::new(workspace_root);

        for (id, role, isolate_worktree, timeout_secs) in worker_specs {
            let start = std::time::Instant::now();
            let max_dur = std::time::Duration::from_secs(timeout_secs);

            let mut final_info = None;
            while start.elapsed() < max_dur {
                if let Some(info) = pool.get_subagent(&id).await {
                    if !matches!(
                        info.state,
                        crate::agent::subagent::types::SubagentState::Running
                            | crate::agent::subagent::types::SubagentState::Idle
                    ) {
                        final_info = Some(info);
                        break;
                    }
                }
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            }

            let info = match final_info {
                Some(i) => Some(i),
                None => pool.get_subagent(&id).await,
            };

            let summary = scratchpad
                .read_entry(&format!("subagent_{}", id))
                .or_else(|| scratchpad.read_entry(&format!("subagent/{}", id)))
                .map(|e| e.content)
                .or_else(|| info.as_ref().and_then(|i| i.final_summary.clone()))
                .unwrap_or_else(|| "Subagent completed without producing a summary.".to_string());

            let success = info
                .as_ref()
                .map(|i| {
                    matches!(
                        i.state,
                        crate::agent::subagent::types::SubagentState::Completed
                    )
                })
                .unwrap_or(false);

            let tokens_used = info.as_ref().map(|i| i.tokens_used).unwrap_or(0);
            let turns_executed = info.as_ref().map(|i| i.turns_executed).unwrap_or(0);

            let mut merged = false;
            if auto_merge
                && isolate_worktree
                && success
                && worktree_mgr.merge_worktree(&id).await.is_ok()
            {
                let _ = worktree_mgr.remove_worktree(&id).await;
                merged = true;
            }

            let sub_res = SubAgentResult {
                id: id.clone(),
                task_id: id.clone(),
                role: role.clone(),
                success,
                final_summary: summary.clone(),
                tokens_used,
                turns_executed,
                files_inspected: Vec::new(),
                files_modified: Vec::new(),
                worktree_branch: if isolate_worktree {
                    Some(format!("subagent/{}", id))
                } else {
                    None
                },
            };

            completed_results.push(FanoutWorkerOutcome {
                id: id.clone(),
                role: role.badge().to_string(),
                isolate_worktree,
                success,
                result: sub_res,
                merged,
                error: if success { None } else { Some(summary) },
            });
        }

        Ok(Self::format_fanout_summary(&completed_results))
    }

    /// Formats a clean markdown report summarizing fan-out results
    pub fn format_fanout_summary(results: &[FanoutWorkerOutcome]) -> String {
        let mut out = format!(
            "### Subagent Swarm Fan-Out Completed ({} worker(s) finished)\n\n",
            results.len()
        );
        out.push_str("| Worker ID | Role | Environment | Status | Tokens | Files Modified |\n");
        out.push_str("| :--- | :--- | :--- | :---: | :---: | :--- |\n");

        for item in results {
            let env = if item.isolate_worktree {
                if item.merged {
                    format!("`subagent/{}` *(merged)*", item.id)
                } else {
                    format!("`subagent/{}`", item.id)
                }
            } else {
                "Shared (Read-Only)".to_string()
            };

            let status = if item.success && item.result.success {
                "✔ Success"
            } else {
                "✗ Failed"
            };
            let modified = if item.result.files_modified.is_empty() {
                "None".to_string()
            } else {
                item.result.files_modified.join(", ")
            };

            out.push_str(&format!(
                "| `{}` | **{}** | {} | {} | {} | {} |\n",
                item.id, item.role, env, status, item.result.tokens_used, modified
            ));
        }

        out.push_str("\n#### Executive Summaries & Findings:\n\n");
        for item in results {
            out.push_str(&format!("##### `{}` — {}\n", item.id, item.role));
            if let Some(ref e) = item.error {
                out.push_str(&format!("**Error:** {}\n\n", e));
            } else {
                let summary_text = if item.result.final_summary.trim().is_empty() {
                    "(No summary output emitted)"
                } else {
                    item.result.final_summary.trim()
                };
                out.push_str(&format!("{}\n\n", summary_text));
            }
        }

        out
    }

    /// Merges an isolated subagent worktree branch into the current branch and cleans up.
    pub async fn merge_worktree(workspace_root: &Path, subagent_id: &str) -> Result<String> {
        let mgr = WorktreeManager::new(workspace_root);
        let merge_output = mgr.merge_worktree(subagent_id).await.map_err(|e| {
            MinicodeError::Tool(ToolError::CommandExec(format!(
                "Failed to merge subagent worktree branch `subagent/{}`: {}",
                subagent_id, e
            )))
        })?;

        let _ = mgr.remove_worktree(subagent_id).await;

        Ok(format!(
            "✔ Successfully integrated subagent worktree `[ID: {}]`!\n\
             • Branch `subagent/{}` merged into current active branch.\n\
             • Temporary worktree folder `.minicode/worktrees/{}` removed.\n\n\
             Git Merge Summary:\n{}",
            subagent_id,
            subagent_id,
            subagent_id,
            merge_output.trim()
        ))
    }

    /// Formats the SubAgentResult into a clean markdown summary for parent LLM tool results.
    pub fn format_result(result: &SubAgentResult) -> String {
        let status_emoji = if result.success { "✔" } else { "✗" };
        let mut out = format!(
            "{} SubAgent Task [{}] Finished (Tokens: {})\n",
            status_emoji, result.task_id, result.tokens_used
        );

        if let Some(ref branch) = result.worktree_branch {
            out.push_str(&format!("Branch: {}\n", branch));
        }

        if !result.files_modified.is_empty() {
            out.push_str(&format!(
                "Files Modified ({}):\n  • {}\n",
                result.files_modified.len(),
                result.files_modified.join("\n  • ")
            ));
        }

        out.push_str(&format!(
            "\nOutcome / Findings:\n{}",
            result.final_summary.trim()
        ));
        out
    }
}

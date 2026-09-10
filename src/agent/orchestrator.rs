use crate::agent::subagent::{
    get_global_scratchpad, get_global_subagent_pool, SubAgent, SubAgentResult, SubagentConfig,
    SubagentRole, SubagentTaskSpec,
};
use crate::error::{MinicodeError, Result, ToolError};
use crate::git::worktree::WorktreeManager;
use futures::future::join_all;
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
        let task_id = format!("task-{}", &uuid::Uuid::new_v4().to_string()[..8]);
        let timeout = timeout_secs.unwrap_or(120);

        let subagent =
            SubAgent::with_config(workspace_root, &task_id, use_worktree, timeout, config);
        subagent.run_task(task_prompt).await
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
        let mut handles = Vec::new();
        let mut launched_descriptions = Vec::new();

        for task in tasks {
            let task_id = pool.next_id(&task.role).await;
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

            launched_descriptions.push((
                task_id.clone(),
                role.badge().to_string(),
                isolate_worktree,
                prompt.clone(),
            ));

            let root = workspace_root.to_path_buf();
            let subagent =
                SubAgent::with_config(&root, &task_id, isolate_worktree, timeout, Some(config));

            let handle = tokio::spawn(async move {
                let res = subagent.run_task(&prompt).await;
                (task_id, role, isolate_worktree, res)
            });

            handles.push(handle);
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
            out.push_str("\nWorkers are running concurrently in background. Use 'manage_subagents' or 'scratchpad_list' to monitor progress.\n");
            return Ok(out);
        }

        // Wait for all workers to complete
        let results = join_all(handles).await;
        let scratchpad = get_global_scratchpad();
        let mut completed_results = Vec::new();
        let worktree_mgr = WorktreeManager::new(workspace_root);

        for join_res in results {
            match join_res {
                Ok((id, role, isolate_worktree, run_res)) => match run_res {
                    Ok(sub_res) => {
                        // Publish findings to SharedScratchpad
                        scratchpad.write_entry(
                            &format!("subagent/{}", id),
                            &format!("Findings from {} ({})", id, role.badge()),
                            &sub_res.final_summary,
                            &id,
                        );

                        let mut merged = false;
                        if auto_merge
                            && isolate_worktree
                            && sub_res.success
                            && !sub_res.files_modified.is_empty()
                            && worktree_mgr.merge_worktree(&id).await.is_ok()
                        {
                            let _ = worktree_mgr.remove_worktree(&id).await;
                            merged = true;
                        }

                        completed_results.push(FanoutWorkerOutcome {
                            id,
                            role: role.badge().to_string(),
                            isolate_worktree,
                            success: true,
                            result: sub_res,
                            merged,
                            error: None,
                        });
                    }
                    Err(e) => {
                        completed_results.push(FanoutWorkerOutcome {
                            id: id.clone(),
                            role: role.badge().to_string(),
                            isolate_worktree,
                            success: false,
                            result: SubAgentResult {
                                id: id.clone(),
                                task_id: id,
                                role,
                                success: false,
                                final_summary: format!("Execution failed: {}", e),
                                tokens_used: 0,
                                turns_executed: 0,
                                files_inspected: Vec::new(),
                                files_modified: Vec::new(),
                                worktree_branch: None,
                            },
                            merged: false,
                            error: Some(e.to_string()),
                        });
                    }
                },
                Err(join_err) => {
                    tracing::error!("Subagent task panicked or was cancelled: {}", join_err);
                }
            }
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

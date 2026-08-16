use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;

use crate::agent::types::AgentEvent;
use crate::error::{MinicodeError, Result};
use crate::git::worktree::WorktreeManager;

/// Summary result from an executed SubAgent task
#[derive(Debug, Clone)]
pub struct SubAgentResult {
    pub task_id: String,
    pub success: bool,
    pub final_response: String,
    pub files_modified: Vec<String>,
    pub tokens_used: usize,
    pub worktree_branch: Option<String>,
}

/// Executes an autonomous subagent in an isolated Git Worktree using machine-readable NDJSON streaming.
pub struct SubAgent {
    pub task_id: String,
    pub workspace_root: PathBuf,
    pub use_worktree: bool,
    pub timeout_secs: u64,
}

impl SubAgent {
    /// Creates a new SubAgent task runner.
    pub fn new(
        workspace_root: &Path,
        task_id: &str,
        use_worktree: bool,
        timeout_secs: u64,
    ) -> Self {
        Self {
            task_id: task_id.to_string(),
            workspace_root: workspace_root.to_path_buf(),
            use_worktree,
            timeout_secs,
        }
    }

    /// Executes the subagent task prompt and yields the final outcome.
    pub async fn run_task(&self, task_prompt: &str) -> Result<SubAgentResult> {
        let worktree_manager = WorktreeManager::new(&self.workspace_root);
        let target_dir = if self.use_worktree {
            worktree_manager
                .create_worktree(&self.task_id)
                .await
                .unwrap_or_else(|_| self.workspace_root.clone())
        } else {
            self.workspace_root.clone()
        };

        let current_exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("minicode"));

        let mut child = Command::new(current_exe)
            .args(["run", "--json-stream", "--yes", "--dir"])
            .arg(&target_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| {
                MinicodeError::Channel(format!("Failed to spawn subagent process: {}", e))
            })?;

        // Write prompt to subagent stdin
        if let Some(mut stdin) = child.stdin.take() {
            let mut prompt_json = serde_json::json!({
                "command": "prompt",
                "text": task_prompt
            })
            .to_string();
            prompt_json.push('\n');
            let _ = stdin.write_all(prompt_json.as_bytes()).await;
        }

        let mut final_response = String::new();
        let mut files_modified = Vec::new();
        let mut tokens_used = 0;
        let mut success = true;

        if let Some(stdout) = child.stdout.take() {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();

            let run_future = async {
                while let Ok(Some(line)) = lines.next_line().await {
                    if let Ok(event) = serde_json::from_str::<AgentEvent>(&line) {
                        match event {
                            AgentEvent::StreamDelta { delta, .. } => {
                                final_response.push_str(&delta);
                            }
                            AgentEvent::FileModified { path, .. } => {
                                if !files_modified.contains(&path) {
                                    files_modified.push(path);
                                }
                            }
                            AgentEvent::TurnEnd {
                                total_tokens_used, ..
                            } => {
                                tokens_used = total_tokens_used;
                            }
                            AgentEvent::Error {
                                message, retrying, ..
                            } => {
                                if !retrying {
                                    final_response.push_str(&format!("\nError: {}", message));
                                    success = false;
                                }
                            }
                            _ => {}
                        }
                    }
                }
            };

            let timeout_dur = Duration::from_secs(self.timeout_secs);
            if tokio::time::timeout(timeout_dur, run_future).await.is_err() {
                let _ = child.kill().await;
                return Ok(SubAgentResult {
                    task_id: self.task_id.clone(),
                    success: false,
                    final_response: format!("Subagent task timed out after {}s", self.timeout_secs),
                    files_modified,
                    tokens_used,
                    worktree_branch: if self.use_worktree {
                        Some(format!("subagent/{}", self.task_id))
                    } else {
                        None
                    },
                });
            }
        }

        let _ = child.wait().await;

        let worktree_branch = if self.use_worktree {
            Some(format!("subagent/{}", self.task_id))
        } else {
            None
        };

        Ok(SubAgentResult {
            task_id: self.task_id.clone(),
            success,
            final_response,
            files_modified,
            tokens_used,
            worktree_branch,
        })
    }
}

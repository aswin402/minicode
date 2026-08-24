pub mod pool;
pub mod scratchpad;
pub mod types;
pub mod worker;

#[allow(unused_imports)]
pub use pool::SubagentPool;
#[allow(unused_imports)]
pub use scratchpad::{ScratchpadEntry, SharedScratchpad, WorkerMessage, WorkerMessageBus};
pub use types::SubagentResult as SubAgentResult;
#[allow(unused_imports)]
pub use types::{SubagentConfig, SubagentInfo, SubagentResult, SubagentRole, SubagentState};
#[allow(unused_imports)]
pub use worker::SubagentWorker;

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::OnceLock;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;

static GLOBAL_POOL: OnceLock<SubagentPool> = OnceLock::new();
static GLOBAL_SCRATCHPAD: OnceLock<SharedScratchpad> = OnceLock::new();
static GLOBAL_MESSAGE_BUS: OnceLock<WorkerMessageBus> = OnceLock::new();

/// Returns a reference to the global SubagentPool for the workspace
pub fn get_global_subagent_pool(workspace_root: &Path) -> &'static SubagentPool {
    GLOBAL_POOL.get_or_init(|| SubagentPool::new(workspace_root))
}

/// Returns a reference to the global SharedScratchpad
pub fn get_global_scratchpad() -> &'static SharedScratchpad {
    GLOBAL_SCRATCHPAD.get_or_init(SharedScratchpad::new)
}

/// Returns a reference to the global WorkerMessageBus
pub fn get_global_message_bus() -> &'static WorkerMessageBus {
    GLOBAL_MESSAGE_BUS.get_or_init(WorkerMessageBus::new)
}

/// Returns a reference to the global SubagentPool if already initialized
pub fn try_get_global_subagent_pool() -> Option<&'static SubagentPool> {
    GLOBAL_POOL.get()
}

use crate::agent::types::AgentEvent;
use crate::error::{MinicodeError, Result};
use crate::git::worktree::WorktreeManager;

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
    pub async fn run_task(&self, task_prompt: &str) -> Result<SubagentResult> {
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
                                total_tokens_used,
                                files_modified: modified,
                                ..
                            } => {
                                tokens_used = total_tokens_used;
                                for f in modified {
                                    if !files_modified.contains(&f) {
                                        files_modified.push(f);
                                    }
                                }
                                break;
                            }
                            AgentEvent::Error { message, .. } => {
                                tracing::error!(message = %message, "Subagent encountered error");
                                success = false;
                            }
                            _ => {}
                        }
                    }
                }
            };

            let _ = tokio::time::timeout(
                std::time::Duration::from_secs(self.timeout_secs),
                run_future,
            )
            .await;
        }

        let _ = child.kill().await;

        let worktree_branch = if self.use_worktree {
            Some(format!("subagent/{}", self.task_id))
        } else {
            None
        };

        Ok(SubagentResult {
            id: self.task_id.clone(),
            task_id: self.task_id.clone(),
            role: SubagentRole::Custom("worktree_worker".to_string()),
            success,
            final_summary: final_response,
            tokens_used,
            turns_executed: 1,
            files_inspected: Vec::new(),
            files_modified,
            worktree_branch,
        })
    }
}

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
pub use types::{
    SubagentConfig, SubagentInfo, SubagentResult, SubagentRole, SubagentState, SubagentTaskSpec,
};
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
    pub config: Option<SubagentConfig>,
}

impl SubAgent {
    /// Creates a new SubAgent task runner.
    #[allow(dead_code)]
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
            config: None,
        }
    }

    /// Creates a new SubAgent task runner with role configuration.
    pub fn with_config(
        workspace_root: &Path,
        task_id: &str,
        use_worktree: bool,
        timeout_secs: u64,
        config: Option<SubagentConfig>,
    ) -> Self {
        Self {
            task_id: task_id.to_string(),
            workspace_root: workspace_root.to_path_buf(),
            use_worktree,
            timeout_secs,
            config,
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

        let mut cmd = Command::new(current_exe);
        cmd.args(["--json-stream", "--yes", "--dir"])
            .arg(&target_dir);

        if let Some(ref cfg) = self.config {
            if let Some(ref m) = cfg.model {
                cmd.arg("--model").arg(m);
            }
        }

        let mut child = cmd
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| {
                MinicodeError::Channel(format!("Failed to spawn subagent process: {}", e))
            })?;

        // Write prompt to subagent stdin via StdinCommand protocol
        if let Some(mut stdin) = child.stdin.take() {
            let cmd_obj = crate::agent::types::StdinCommand::UserInput {
                text: task_prompt.to_string(),
            };
            if let Ok(mut prompt_json) = serde_json::to_string(&cmd_obj) {
                prompt_json.push('\n');
                let _ = stdin.write_all(prompt_json.as_bytes()).await;
            }
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
                                if final_response.is_empty() {
                                    final_response =
                                        format!("Subagent execution error: {}", message);
                                }
                            }
                            _ => {}
                        }
                    }
                }
            };

            let timeout_res = tokio::time::timeout(
                std::time::Duration::from_secs(self.timeout_secs),
                run_future,
            )
            .await;

            if timeout_res.is_err() {
                tracing::warn!(task_id = %self.task_id, timeout_secs = self.timeout_secs, "Subagent process timed out");
                success = false;
                if final_response.is_empty() {
                    final_response = "Subagent process timed out before completion.".to_string();
                }
            }
        }

        if final_response.trim().is_empty() {
            success = false;
            final_response =
                "Subagent process exited without producing a response summary.".to_string();
        }

        let _ = child.kill().await;

        let worktree_branch = if self.use_worktree {
            Some(format!("subagent/{}", self.task_id))
        } else {
            None
        };

        let assigned_role = self
            .config
            .as_ref()
            .map(|c| c.role.clone())
            .unwrap_or_else(|| SubagentRole::Custom("worktree_worker".to_string()));

        Ok(SubagentResult {
            id: self.task_id.clone(),
            task_id: self.task_id.clone(),
            role: assigned_role,
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

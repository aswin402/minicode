//! Subagent process orchestrator and agent-to-agent communication dispatcher.
//!
//! Manages the execution lifecycle of autonomous child `minicode` processes in isolated
//! Git worktrees, wires reactive A2A mailbox channels, and synthesizes structured reports.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use tokio::process::Command;

use crate::agent::subagent::mailbox::AgentMailbox;
use crate::agent::subagent::message::{AgentMessage, MessageIntent};
use crate::agent::subagent::types::{AgentId, SubagentRole, WorkspaceMode};
use crate::agent::types::AgentEvent;
use crate::error::ToolError;
use crate::sandbox::worktree::GitWorktreeManager;

/// Orchestrator for spawning autonomous subagent child processes and routing A2A messages.
pub struct SubagentOrchestrator;

impl SubagentOrchestrator {
    /// Spawns an autonomous background subagent with a specialized role in an isolated workspace or worktree.
    pub async fn spawn_subagent(
        workspace_root: &Path,
        task: &str,
        role: SubagentRole,
        workspace_mode: WorkspaceMode,
        max_iterations: Option<usize>,
    ) -> Result<String, ToolError> {
        // 1. Generate child agent id
        let agent_id = AgentId::new_subagent(role.as_str());

        // 2. Determine workspace isolation mode
        let should_isolate = workspace_mode == WorkspaceMode::Worktree
            || (workspace_mode == WorkspaceMode::Auto
                && role.default_workspace_mode() == WorkspaceMode::Worktree);

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

        // 3. Initialize child agent mailbox directory
        let child_agent_dir = workspace_root
            .join(".minicode")
            .join("agents")
            .join(&agent_id.0);
        let child_mailbox = AgentMailbox::new(agent_id.clone(), &child_agent_dir).map_err(|e| {
            if let Some(ref handle) = worktree_handle {
                let _ = GitWorktreeManager::remove_worktree(handle);
            }
            ToolError::ExecutionFailed(format!(
                "Failed to initialize mailbox for subagent `{}`: {}",
                agent_id, e
            ))
        })?;

        // 4. Post initial task initialization message to child mailbox
        let init_msg = AgentMessage::new(
            AgentId::parent(),
            agent_id.clone(),
            MessageIntent::TaskInit,
            task,
        );
        if let Err(e) = child_mailbox.post(init_msg) {
            if let Some(ref handle) = worktree_handle {
                let _ = GitWorktreeManager::remove_worktree(handle);
            }
            return Err(ToolError::ExecutionFailed(format!(
                "Failed to post init message to subagent `{}` mailbox: {}",
                agent_id, e
            )));
        }

        // 5. Spawn headless child minicode process
        let current_exe = std::env::current_exe().map_err(|e| {
            if let Some(ref handle) = worktree_handle {
                let _ = GitWorktreeManager::remove_worktree(handle);
            }
            ToolError::ExecutionFailed(format!("Failed to determine current executable: {}", e))
        })?;

        let mut cmd = Command::new(current_exe);
        cmd.arg("run");
        cmd.arg("-d").arg(&target_dir);
        cmd.arg("-y");
        cmd.arg("--json-stream");
        cmd.arg("--tools").arg(role.tool_filter_mode());

        if let Some(max_iter) = max_iterations {
            cmd.arg("--max-iterations").arg(max_iter.to_string());
        }

        cmd.arg(task);
        cmd.stdin(Stdio::null());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        cmd.kill_on_drop(true);

        #[cfg(unix)]
        {
            cmd.process_group(0);
        }

        let mut child = cmd.spawn().map_err(|e| {
            if let Some(ref handle) = worktree_handle {
                let _ = GitWorktreeManager::remove_worktree(handle);
            }
            ToolError::ExecutionFailed(format!("Failed to spawn subagent process: {}", e))
        })?;

        // 6. Asynchronously read lines from child stdout using BufReader
        let stdout = child.stdout.take().ok_or_else(|| {
            if let Some(ref handle) = worktree_handle {
                let _ = GitWorktreeManager::remove_worktree(handle);
            }
            ToolError::ExecutionFailed("Failed to capture child process stdout".to_string())
        })?;

        let stderr = child.stderr.take();

        let mut final_response = String::new();
        let mut files_modified: Vec<String> = Vec::new();
        let mut tools_executed: Vec<String> = Vec::new();
        let mut tokens_used = 0;
        let mut child_success = true;
        let mut error_msg: Option<String> = None;

        let reader = BufReader::new(stdout);
        let mut lines = reader.lines();

        let read_stdout = async {
            while let Ok(Some(line)) = lines.next_line().await {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                if let Ok(event) = serde_json::from_str::<AgentEvent>(trimmed) {
                    match event {
                        AgentEvent::StreamDelta { delta, .. } => {
                            final_response.push_str(&delta);
                        }
                        AgentEvent::ToolCall { tool, .. } => {
                            if !tools_executed.contains(&tool) {
                                tools_executed.push(tool);
                            }
                        }
                        AgentEvent::FileModified { path, .. } => {
                            if !files_modified.contains(&path) {
                                files_modified.push(path);
                            }
                        }
                        AgentEvent::TurnEnd {
                            total_tokens_used,
                            files_modified: modified,
                            status,
                            ..
                        } => {
                            tokens_used = total_tokens_used;
                            for f in modified {
                                if !files_modified.contains(&f) {
                                    files_modified.push(f);
                                }
                            }
                            if status == crate::constants::TURN_STATUS_CIRCUIT_TRIPPED
                                || status == crate::constants::TURN_STATUS_CANCELLED
                            {
                                child_success = false;
                            }
                        }
                        AgentEvent::Error { message, .. } => {
                            tracing::error!(message = %message, "Subagent encountered error");
                            child_success = false;
                            error_msg = Some(message);
                        }
                        _ => {}
                    }
                }
            }
        };

        let mut captured_stderr = String::new();
        let read_stderr = async {
            if let Some(mut err_stream) = stderr {
                let _ = err_stream.read_to_string(&mut captured_stderr).await;
            }
        };

        // 7. Await child completion or timeout (120s)
        let timeout_dur = std::time::Duration::from_secs(120);
        let run_outcome = tokio::time::timeout(timeout_dur, async {
            tokio::join!(read_stdout, read_stderr);
            child.wait().await
        })
        .await;

        let exit_status = match run_outcome {
            Ok(Ok(status)) => status,
            Ok(Err(e)) => {
                if let Some(ref handle) = worktree_handle {
                    let _ = GitWorktreeManager::remove_worktree(handle);
                }
                return Err(ToolError::ExecutionFailed(format!(
                    "Subagent `{}` wait error: {}",
                    agent_id, e
                )));
            }
            Err(_) => {
                let _ = child.kill().await;
                if let Some(ref handle) = worktree_handle {
                    let _ = GitWorktreeManager::remove_worktree(handle);
                }
                return Err(ToolError::ExecutionFailed(format!(
                    "Subagent `{}` timed out after 120 seconds",
                    agent_id
                )));
            }
        };

        if !exit_status.success() {
            child_success = false;
        }

        if !child_success {
            if let Some(ref handle) = worktree_handle {
                let _ = GitWorktreeManager::remove_worktree(handle);
            }
            let reason = error_msg.unwrap_or_else(|| {
                let trimmed_stderr = captured_stderr.trim();
                if !trimmed_stderr.is_empty() {
                    trimmed_stderr.to_string()
                } else if !exit_status.success() {
                    format!("Process exited with status {:?}", exit_status.code())
                } else {
                    "Subagent execution failed".to_string()
                }
            });
            return Err(ToolError::ExecutionFailed(format!(
                "Subagent `{}` failed: {}",
                agent_id, reason
            )));
        }

        // Child succeeded: capture diff if worktree was used, and retain worktree for arbitration
        let diff = if let Some(ref handle) = worktree_handle {
            GitWorktreeManager::capture_diff(handle).unwrap_or_default()
        } else {
            String::new()
        };

        let summary_text = if final_response.trim().is_empty() {
            "Task executed successfully without stream output.".to_string()
        } else {
            final_response.trim().to_string()
        };

        let mut report = format!(
            "✔ Subagent `[ID: {} | Role: {}]` completed task successfully!\n• Tokens Used: {}\n• Tools Executed: {}\n• Files Modified: {}\n",
            agent_id,
            role.badge(),
            tokens_used,
            if tools_executed.is_empty() {
                "None".to_string()
            } else {
                tools_executed.join(", ")
            },
            if files_modified.is_empty() {
                "None (Read-Only)".to_string()
            } else {
                files_modified.join(", ")
            },
        );

        if let Some(ref handle) = worktree_handle {
            report.push_str(&format!(
                "\n• Worktree Retained: `{}` (branch `{}` — ready for `merge_subagent_worktree`)\n",
                handle.worktree_path.display(),
                handle.branch_name
            ));
        }

        if !diff.is_empty() {
            report.push_str(&format!("\n### Worktree Diff\n```diff\n{}\n```\n", diff));
        }

        report.push_str(&format!("\n### Summary & Findings\n{}", summary_text));

        Ok(report)
    }

    /// Sends a typed message to a target agent's durable mailbox.
    pub fn send_message(
        workspace_root: &Path,
        sender: &AgentId,
        recipient: &AgentId,
        message: &str,
        intent: Option<MessageIntent>,
    ) -> Result<String, ToolError> {
        // 1. Resolve recipient agent directory
        let agent_dir: PathBuf = if recipient.is_parent() {
            workspace_root
                .join(".minicode")
                .join("agents")
                .join("parent")
        } else {
            workspace_root
                .join(".minicode")
                .join("agents")
                .join(&recipient.0)
        };

        // 2. Create or load AgentMailbox
        let mailbox = AgentMailbox::new(recipient.clone(), &agent_dir).map_err(|e| {
            ToolError::ExecutionFailed(format!(
                "Failed to open mailbox for agent `{}`: {}",
                recipient, e
            ))
        })?;

        // 3. Construct AgentMessage with unique UUID, sender, recipient, intent, and timestamp
        let msg = AgentMessage::new(
            sender.clone(),
            recipient.clone(),
            intent.unwrap_or(MessageIntent::StatusUpdate),
            message,
        );

        // 4. Call mailbox.post(msg)
        mailbox.post(msg).map_err(|e| {
            ToolError::ExecutionFailed(format!(
                "Failed to deliver message to agent `{}` mailbox: {}",
                recipient, e
            ))
        })?;

        // 5. Return confirmation string
        Ok(format!(
            "✔ Message delivered to agent `{}` mailbox.",
            recipient
        ))
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn test_spawn_subagent_schema_valid() {
        let schemas = crate::tools::ToolRegistry::get_tool_schemas();
        let spawn_schema = schemas.iter().find(|s| s.name == "spawn_subagent");
        assert!(
            spawn_schema.is_some(),
            "spawn_subagent schema must be registered"
        );
        let schema = spawn_schema.unwrap();
        assert!(
            schema
                .description
                .contains("Spawn an autonomous background subagent"),
            "Description must match specification"
        );

        let props = schema
            .parameters
            .get("properties")
            .expect("properties object must exist");
        assert!(props.get("task").is_some(), "task property required");
        assert!(props.get("role").is_some(), "role property required");
        assert!(
            props.get("workspace_mode").is_some(),
            "workspace_mode property required"
        );
        assert!(
            props.get("max_iterations").is_some(),
            "max_iterations property required"
        );
    }

    #[test]
    fn test_total_tool_count_matches() {
        let schemas = crate::tools::ToolRegistry::get_tool_schemas();
        assert_eq!(
            schemas.len(),
            crate::constants::TOTAL_TOOL_COUNT,
            "Tool registry schemas length must equal TOTAL_TOOL_COUNT"
        );
    }

    #[test]
    fn test_send_message_routes_to_mailbox() {
        let temp_dir = tempfile::tempdir().expect("tempdir creation");
        let sender = AgentId::parent();
        let recipient = AgentId::new_subagent("coder");
        let msg_text = "Implement SubagentOrchestrator in orchestrator.rs";

        let result = SubagentOrchestrator::send_message(
            temp_dir.path(),
            &sender,
            &recipient,
            msg_text,
            Some(MessageIntent::TaskInit),
        );

        assert!(result.is_ok(), "send_message should succeed");
        let confirm = result.unwrap();
        assert_eq!(
            confirm,
            format!("✔ Message delivered to agent `{}` mailbox.", recipient)
        );

        let mailbox_file = temp_dir
            .path()
            .join(".minicode")
            .join("agents")
            .join(&recipient.0)
            .join("mailbox.jsonl");
        assert!(
            mailbox_file.exists(),
            "mailbox.jsonl must exist at {:?}",
            mailbox_file
        );

        let mailbox =
            AgentMailbox::new(recipient.clone(), &mailbox_file.parent().unwrap()).unwrap();
        let unread = mailbox.drain_unread().expect("drain unread messages");
        assert_eq!(unread.len(), 1);
        assert_eq!(unread[0].sender, sender);
        assert_eq!(unread[0].recipient, recipient);
        assert_eq!(unread[0].intent, MessageIntent::TaskInit);
        assert_eq!(unread[0].content, msg_text);
    }

    #[test]
    fn test_send_message_parent_recipient() {
        let temp_dir = tempfile::tempdir().expect("tempdir creation");
        let sender = AgentId::new_subagent("tester");
        let recipient = AgentId::parent();
        let msg_text = "All 42 unit tests passed cleanly";

        let result = SubagentOrchestrator::send_message(
            temp_dir.path(),
            &sender,
            &recipient,
            msg_text,
            Some(MessageIntent::TaskComplete),
        );

        assert!(result.is_ok(), "send_message to parent should succeed");
        let mailbox_file = temp_dir
            .path()
            .join(".minicode")
            .join("agents")
            .join("parent")
            .join("mailbox.jsonl");
        assert!(mailbox_file.exists(), "parent mailbox.jsonl must exist");

        let mailbox =
            AgentMailbox::new(recipient.clone(), &mailbox_file.parent().unwrap()).unwrap();
        let unread = mailbox.drain_unread().expect("drain unread messages");
        assert_eq!(unread.len(), 1);
        assert_eq!(unread[0].sender, sender);
        assert_eq!(unread[0].recipient, recipient);
        assert_eq!(unread[0].intent, MessageIntent::TaskComplete);
        assert_eq!(unread[0].content, msg_text);
    }
}

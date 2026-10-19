//! Subagent process orchestrator and agent-to-agent communication dispatcher.
//!
//! Manages the execution lifecycle of autonomous child `minicode` processes in isolated
//! Git worktrees, wires reactive A2A mailbox channels, and synthesizes structured reports.

use std::path::{Path, PathBuf};

use crate::agent::subagent::mailbox::AgentMailbox;
use crate::agent::subagent::message::{AgentMessage, MessageIntent};
use crate::agent::subagent::types::{AgentId, SubagentConfig, SubagentRole, WorkspaceMode};
use crate::error::ToolError;
use crate::sandbox::worktree::GitWorktreeManager;

/// Orchestrator for spawning autonomous `minipub` subagents in-process and routing A2A messages.
pub struct SubagentOrchestrator;

impl SubagentOrchestrator {
    /// Spawns an autonomous subagent (`minipub`) with a specialized role in an isolated workspace or worktree.
    #[allow(dead_code)]
    pub async fn spawn_subagent(
        workspace_root: &Path,
        task: &str,
        role: SubagentRole,
        workspace_mode: WorkspaceMode,
        max_iterations: Option<usize>,
    ) -> Result<String, ToolError> {
        Self::spawn_subagent_with_options(
            workspace_root,
            task,
            role,
            workspace_mode,
            max_iterations,
            Vec::new(),
            Vec::new(),
        )
        .await
    }

    /// Spawns an autonomous `minipub` subagent with optional Boomerang todo items and file boundary leases.
    #[allow(clippy::too_many_arguments)]
    pub async fn spawn_subagent_with_options(
        workspace_root: &Path,
        task: &str,
        role: SubagentRole,
        workspace_mode: WorkspaceMode,
        max_iterations: Option<usize>,
        todos: Vec<String>,
        file_boundaries: Vec<String>,
    ) -> Result<String, ToolError> {
        let app_cfg = crate::config::Config::load_for_workspace(workspace_root);
        let mut config = SubagentConfig::for_role_with_minipub_config(role, &app_cfg.agent.minipub)
            .with_todos(todos)
            .with_file_boundaries(file_boundaries);

        if let Some(max_iter) = max_iterations {
            config.max_turns = max_iter.max(1);
        }

        let should_isolate = match workspace_mode {
            WorkspaceMode::Worktree => true,
            WorkspaceMode::Shared => false,
            WorkspaceMode::Auto => {
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
                    _ => role.default_workspace_mode() == WorkspaceMode::Worktree,
                }
            }
        };

        let pool = crate::agent::subagent::get_global_subagent_pool(workspace_root);
        let provider = pool.get_or_create_provider().await;

        let res = pool
            .run_subagent_with_options(role, task, Some(config), provider, should_isolate)
            .await
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

        // Initialize child agent mailbox and record task init + completion for A2A auditability
        let agent_id = AgentId(res.id.clone());
        let child_agent_dir = workspace_root
            .join(".minicode")
            .join("agents")
            .join(&agent_id.0);
        if let Ok(child_mailbox) = AgentMailbox::new(agent_id.clone(), &child_agent_dir) {
            let init_msg = AgentMessage::new(
                AgentId::parent(),
                agent_id.clone(),
                MessageIntent::TaskInit,
                task,
            );
            let _ = child_mailbox.post(init_msg);
        }

        if !res.success {
            return Err(ToolError::ExecutionFailed(format!(
                "MiniPub `{}` failed: {}",
                res.id, res.final_summary
            )));
        }

        // Capture worktree diff if worktree exists on disk
        let worktree_opt =
            GitWorktreeManager::locate_worktree(workspace_root, &agent_id).map(|path| {
                let branch = GitWorktreeManager::resolve_branch_for(workspace_root, &agent_id);
                crate::sandbox::WorktreeHandle {
                    worktree_path: path,
                    branch_name: branch,
                    agent_id: agent_id.clone(),
                    repo_root: workspace_root.to_path_buf(),
                }
            });

        let diff = if let Some(ref handle) = worktree_opt {
            GitWorktreeManager::capture_diff(handle).unwrap_or_default()
        } else {
            String::new()
        };

        let mut report = format!(
            "✔ MiniPub Subagent `[ID: {} | Role: {}]` completed task successfully!\n• Tokens Used: {}\n• Turns Executed: {}\n• Files Modified: {}\n",
            res.id,
            role.badge(),
            res.tokens_used,
            res.turns_executed,
            if res.files_modified.is_empty() {
                "None (Read-Only)".to_string()
            } else {
                res.files_modified.join(", ")
            },
        );

        if let Some(ref handle) = worktree_opt {
            report.push_str(&format!(
                "\n• Worktree Retained: `{}` (branch `{}` — ready for `merge_subagent_worktree`)\n",
                handle.worktree_path.display(),
                handle.branch_name
            ));
        }

        if !diff.is_empty() {
            report.push_str(&format!("\n### Worktree Diff\n```diff\n{}\n```\n", diff));
        }

        report.push_str(&format!(
            "\n### Summary & Findings\n{}",
            res.final_summary.trim()
        ));

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

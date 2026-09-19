use crate::agent::provider::ToolSchema;
use crate::agent::subagent::fanout::{FanoutJoinMode, FanoutOrchestrator, FanoutTaskItem};
use crate::agent::subagent::types::{SubagentRole, WorkspaceMode};
use crate::error::{Result, ToolError};
use crate::tools::param;
use serde_json::json;
use std::path::Path;

pub fn get_schemas() -> Vec<ToolSchema> {
    vec![
        ToolSchema {
            name: "send_worker_message".to_string(),
            description: "Send an asynchronous message to another subagent worker or broadcast to the entire worker swarm.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "from_worker_id": {
                        "type": "string",
                        "description": "Sender worker ID"
                    },
                    "to_worker_id": {
                        "type": "string",
                        "description": "Recipient worker ID (omit or leave empty to broadcast to all swarm workers)"
                    },
                    "topic": {
                        "type": "string",
                        "description": "Message topic or classification (e.g. 'findings', 'error', 'sync')"
                    },
                    "payload": {
                        "type": "string",
                        "description": "Message content payload"
                    }
                },
                "required": ["from_worker_id", "topic", "payload"]
            }),
        },
        ToolSchema {
            name: "read_worker_messages".to_string(),
            description: "Fetch pending direct and broadcast messages for a specific subagent worker from the messaging bus.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "worker_id": {
                        "type": "string",
                        "description": "Worker ID whose inbox to check"
                    }
                },
                "required": ["worker_id"]
            }),
        },
        ToolSchema {
            name: "fanout_subagents".to_string(),
            description: "Concurrently dispatch a batch of specialized subagents across isolated Git Worktrees or shared repository threads. Supports race-to-first-success or all-worker join policies, sequential conflict-free merge arbitration, and executive map-reduce reporting.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "tasks": {
                        "type": "array",
                        "description": "List of subagent task specifications to execute concurrently",
                        "items": {
                            "type": "object",
                            "properties": {
                                "task": {
                                    "type": "string",
                                    "description": "Detailed task instructions or prompt for this worker"
                                },
                                "prompt": {
                                    "type": "string",
                                    "description": "Alias for task"
                                },
                                "role": {
                                    "type": "string",
                                    "enum": [
                                        "scout",
                                        "coder",
                                        "tester",
                                        "reviewer",
                                        "architect",
                                        "security",
                                        "researcher",
                                        "code_reviewer",
                                        "test_engineer",
                                        "security_auditor",
                                        "custom"
                                    ],
                                    "description": "Subagent specialized role preset defining worker capabilities and workspace isolation (default: coder)"
                                },
                                "workspace_mode": {
                                    "type": "string",
                                    "enum": ["auto", "worktree", "shared"],
                                    "description": "Workspace isolation mode (default: auto)"
                                },
                                "max_iterations": {
                                    "type": "integer",
                                    "description": "Maximum autonomous tool iteration steps for this worker"
                                },
                                "check_cmd": {
                                    "type": "string",
                                    "description": "Custom validation command to run before merge (e.g. 'cargo check', or 'skip')"
                                }
                            },
                            "required": ["task"]
                        }
                    },
                    "join_mode": {
                        "type": "string",
                        "enum": ["all", "race"],
                        "description": "Join policy: 'all' awaits all workers; 'race' cancels remaining workers upon first success (default: 'all')"
                    },
                    "auto_merge": {
                        "type": "boolean",
                        "description": "If true, sequentially arbitrates and merges successful mutating worktrees into current branch (default: false)"
                    },
                    "max_concurrency": {
                        "type": "integer",
                        "description": "Maximum concurrent workers running simultaneously (default: 4, min: 1, max: 16)"
                    }
                },
                "required": ["tasks"]
            }),
        },
    ]
}

pub async fn dispatch(
    tool_name: &str,
    args: &serde_json::Value,
    workspace_root: &Path,
) -> Option<Result<String>> {
    match tool_name {
        "send_worker_message" => Some(
            async {
                let from = param::require_str(args, "from_worker_id", "send_worker_message")?;
                let to = param::opt_str(args, "to_worker_id").filter(|s| !s.is_empty());
                let topic = param::require_str(args, "topic", "send_worker_message")?;
                let payload = param::require_str(args, "payload", "send_worker_message")?;

                let bus = crate::agent::subagent::get_global_message_bus();
                let msg = bus.send_message(from, to, topic, payload);

                let dest = to
                    .map(|t| format!("to worker `{}`", t))
                    .unwrap_or_else(|| "as swarm broadcast".to_string());
                Ok(format!(
                    "✔ Message `{}` posted {} on topic `{}`.",
                    msg.id, dest, msg.topic
                ))
            }
            .await,
        ),
        "read_worker_messages" => Some(
            async {
                let worker_id = param::require_str(args, "worker_id", "read_worker_messages")?;

                let bus = crate::agent::subagent::get_global_message_bus();
                let messages = bus.read_inbox(worker_id);

                if messages.is_empty() {
                    Ok(format!("ℹ Inbox for worker `{}` is empty.", worker_id))
                } else {
                    let mut out = format!(
                        "📬 Inbox for Worker `{}` ({} message(s)):\n\n",
                        worker_id,
                        messages.len()
                    );
                    for (i, m) in messages.iter().enumerate() {
                        let kind = if m.to_worker_id.is_none() {
                            "[Broadcast]"
                        } else {
                            "[Direct]"
                        };
                        out.push_str(&format!(
                            "{}. {} from `{}` (Topic: `{}`)\n```\n{}\n```\n\n",
                            i + 1,
                            kind,
                            m.from_worker_id,
                            m.topic,
                            m.payload
                        ));
                    }
                    Ok(out)
                }
            }
            .await,
        ),
        "fanout_subagents" => Some(
            async {
                let (task_items, join_mode, auto_merge, max_concurrency) = parse_fanout_args(args)?;

                FanoutOrchestrator::execute_fanout(
                    workspace_root,
                    task_items,
                    join_mode,
                    auto_merge,
                    max_concurrency,
                )
                .await
                .map_err(Into::into)
            }
            .await,
        ),
        _ => None,
    }
}

/// Helper to extract and validate `fanout_subagents` tool arguments.
pub(crate) fn parse_fanout_args(
    args: &serde_json::Value,
) -> std::result::Result<(Vec<FanoutTaskItem>, FanoutJoinMode, bool, usize), ToolError> {
    let tasks_arr = param::require_array(args, "tasks", "fanout_subagents")?;

    let mut task_items = Vec::new();
    for (i, t) in tasks_arr.iter().enumerate() {
        let task_text = t
            .get("task")
            .and_then(|v| v.as_str())
            .or_else(|| t.get("prompt").and_then(|v| v.as_str()))
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| ToolError::InvalidArguments {
                name: "fanout_subagents".to_string(),
                reason: format!(
                    "Task #{} missing required field 'task' (or 'prompt')",
                    i + 1
                ),
            })?;

        let role = if let Some(role_str) = t.get("role").and_then(|v| v.as_str()) {
            SubagentRole::from_str_loose(role_str)
        } else {
            SubagentRole::Coder
        };

        let workspace_mode = t
            .get("workspace_mode")
            .and_then(|v| v.as_str())
            .and_then(|m| match m.to_lowercase().as_str() {
                "worktree" => Some(WorkspaceMode::Worktree),
                "shared" => Some(WorkspaceMode::Shared),
                "auto" => Some(WorkspaceMode::Auto),
                _ => None,
            });

        let max_iterations = param::opt_u64(t, "max_iterations").map(|n| n as usize);
        let check_cmd = param::opt_str(t, "check_cmd").map(|s| s.to_string());

        task_items.push(FanoutTaskItem {
            task: task_text.to_string(),
            role,
            workspace_mode,
            max_iterations,
            check_cmd,
        });
    }

    let join_mode = match param::opt_str(args, "join_mode") {
        Some("race") => FanoutJoinMode::Race,
        _ => FanoutJoinMode::All,
    };

    let auto_merge = param::opt_bool(args, "auto_merge", false);
    let max_concurrency = param::opt_u64(args, "max_concurrency")
        .unwrap_or(4)
        .clamp(1, 16) as usize;

    Ok((task_items, join_mode, auto_merge, max_concurrency))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fanout_subagents_schema_structure() {
        let schemas = get_schemas();
        let fanout_schema = schemas
            .iter()
            .find(|s| s.name == "fanout_subagents")
            .expect("fanout_subagents schema present");

        assert_eq!(fanout_schema.name, "fanout_subagents");
        let props = fanout_schema.parameters["properties"]
            .as_object()
            .expect("properties object");

        assert!(props.contains_key("tasks"));
        assert!(props.contains_key("join_mode"));
        assert!(props.contains_key("auto_merge"));
        assert!(props.contains_key("max_concurrency"));

        let task_props = props["tasks"]["items"]["properties"]
            .as_object()
            .expect("task item properties");
        assert!(task_props.contains_key("task"));
        assert!(task_props.contains_key("prompt"));
        assert!(task_props.contains_key("role"));
        assert!(task_props.contains_key("workspace_mode"));
        assert!(task_props.contains_key("max_iterations"));
        assert!(task_props.contains_key("check_cmd"));

        let join_mode_enums = props["join_mode"]["enum"]
            .as_array()
            .expect("join_mode enum");
        assert_eq!(join_mode_enums, &vec![json!("all"), json!("race")]);

        let required = fanout_schema.parameters["required"]
            .as_array()
            .expect("required array");
        assert!(required.iter().any(|v| v == "tasks"));
    }

    #[test]
    fn test_fanout_subagents_argument_parsing() {
        let json_args = serde_json::json!({
            "tasks": [
                {
                    "task": "Inspect schema",
                    "role": "scout",
                    "workspace_mode": "shared"
                },
                {
                    "prompt": "Implement feature",
                    "role": "coder",
                    "workspace_mode": "worktree",
                    "max_iterations": 15,
                    "check_cmd": "cargo check -j 1"
                }
            ],
            "join_mode": "race",
            "auto_merge": true,
            "max_concurrency": 8
        });

        let (tasks, join_mode, auto_merge, max_concurrency) =
            parse_fanout_args(&json_args).expect("valid parse");

        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0].task, "Inspect schema");
        assert_eq!(tasks[0].role, SubagentRole::Scout);
        assert_eq!(tasks[0].workspace_mode, Some(WorkspaceMode::Shared));
        assert_eq!(tasks[0].check_cmd, None);

        assert_eq!(tasks[1].task, "Implement feature");
        assert_eq!(tasks[1].role, SubagentRole::Coder);
        assert_eq!(tasks[1].workspace_mode, Some(WorkspaceMode::Worktree));
        assert_eq!(tasks[1].max_iterations, Some(15));
        assert_eq!(tasks[1].check_cmd, Some("cargo check -j 1".to_string()));

        assert_eq!(join_mode, FanoutJoinMode::Race);
        assert!(auto_merge);
        assert_eq!(max_concurrency, 8);
    }

    #[tokio::test]
    async fn test_fanout_subagents_argument_parsing_and_dispatch() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();

        // Empty tasks test
        let empty_args = serde_json::json!({
            "tasks": []
        });
        let res = dispatch("fanout_subagents", &empty_args, root)
            .await
            .expect("handled");
        assert!(res.is_ok());
        assert!(res.unwrap().contains("No subagent tasks specified"));

        // Invalid task element (missing both task and prompt)
        let invalid_args = serde_json::json!({
            "tasks": [
                { "role": "coder" }
            ]
        });
        let res_err = dispatch("fanout_subagents", &invalid_args, root)
            .await
            .expect("handled");
        assert!(res_err.is_err());
    }
}

use crate::agent::provider::ToolSchema;
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
            description: "Concurrently dispatch a batch of specialized subagent workers across isolated Git Worktrees or read-only threads. Returns a condensed matrix summary of findings and automatically updates SharedScratchpad.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "tasks": {
                        "type": "array",
                        "description": "List of subagent task specifications to execute concurrently",
                        "items": {
                            "type": "object",
                            "properties": {
                                "role": {
                                    "type": "string",
                                    "enum": ["researcher", "code_reviewer", "test_engineer", "security_auditor", "custom"],
                                    "description": "Worker role capability whitelist preset"
                                },
                                "prompt": {
                                    "type": "string",
                                    "description": "Detailed instructions for this worker"
                                },
                                "isolate_worktree": {
                                    "type": "boolean",
                                    "description": "Whether to run inside an isolated Git Worktree branch"
                                },
                                "model": {
                                    "type": "string",
                                    "description": "Optional model override"
                                },
                                "timeout_secs": {
                                    "type": "integer",
                                    "description": "Timeout in seconds (default: 120)"
                                }
                            },
                            "required": ["role", "prompt"]
                        }
                    },
                    "wait_for_completion": {
                        "type": "boolean",
                        "description": "If true (default), awaits all workers and returns executive summary. If false, launches workers in background and returns IDs immediately."
                    },
                    "auto_merge": {
                        "type": "boolean",
                        "description": "If true, automatically merges successful worktree modifications into current branch (default: false)"
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
                let tasks_arr = param::require_array(args, "tasks", "fanout_subagents")?;

                let mut task_specs = Vec::new();
                for (i, t) in tasks_arr.iter().enumerate() {
                    let role_str = t.get("role").and_then(|v| v.as_str()).ok_or_else(|| {
                        ToolError::InvalidArguments {
                            name: "fanout_subagents".to_string(),
                            reason: format!("Task #{} missing required field 'role'", i + 1),
                        }
                    })?;
                    let prompt = t.get("prompt").and_then(|v| v.as_str()).ok_or_else(|| {
                        ToolError::InvalidArguments {
                            name: "fanout_subagents".to_string(),
                            reason: format!("Task #{} missing required field 'prompt'", i + 1),
                        }
                    })?;
                    let role = crate::agent::subagent::SubagentRole::from_str_loose(role_str);
                    let isolate_worktree = param::get_bool(t, "isolate_worktree");
                    let model = param::opt_str(t, "model").map(|s| s.to_string());
                    let timeout_secs = param::opt_u64(t, "timeout_secs");

                    task_specs.push(crate::agent::subagent::SubagentTaskSpec {
                        role,
                        prompt: prompt.to_string(),
                        isolate_worktree,
                        model,
                        timeout_secs,
                    });
                }

                let wait_for_completion = param::opt_bool(args, "wait_for_completion", true);
                let auto_merge = param::opt_bool(args, "auto_merge", false);

                crate::agent::orchestrator::MultiAgentOrchestrator::fanout_tasks(
                    workspace_root,
                    task_specs,
                    wait_for_completion,
                    auto_merge,
                )
                .await
            }
            .await,
        ),
        _ => None,
    }
}

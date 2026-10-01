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
                    "to_worker_id": {
                        "type": "string",
                        "description": "Recipient worker ID (omit or leave empty to broadcast to all swarm workers)"
                    },
                    "intent": {
                        "type": "string",
                        "enum": [
                            "publish_contract",
                            "query_interface",
                            "coordination_note"
                        ],
                        "description": "Typed intent of the message (default: coordination_note)"
                    },
                    "topic": {
                        "type": "string",
                        "description": "Message topic or classification (e.g. 'findings', 'error', 'sync')"
                    },
                    "payload": {
                        "type": "string",
                        "description": "Message content payload (max 800 characters)"
                    },
                    "from_worker_id": {
                        "type": "string",
                        "description": "Sender worker ID (inferred from MINICODE_SWARM_TASK_ID if omitted)"
                    }
                },
                "required": ["topic", "payload"]
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
                let swarm_dir_env = std::env::var("MINICODE_SWARM_DIR")
                    .ok()
                    .filter(|s| !s.trim().is_empty());
                let swarm_task_env = std::env::var("MINICODE_SWARM_TASK_ID")
                    .ok()
                    .filter(|s| !s.trim().is_empty());
                let swarm_id_env = std::env::var("MINICODE_SWARM_ID")
                    .ok()
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or_else(|| "swarm-standalone".to_string());

                let from = param::opt_str(args, "from_worker_id")
                    .filter(|s| !s.trim().is_empty())
                    .or(swarm_task_env.as_deref())
                    .unwrap_or("worker");

                let to = param::opt_str(args, "to_worker_id")
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty());
                let topic = param::require_str(args, "topic", "send_worker_message")?;
                let payload = param::require_str(args, "payload", "send_worker_message")?;

                let intent_str = param::opt_str(args, "intent").unwrap_or("coordination_note");
                let intent = match intent_str {
                    "publish_contract" => {
                        crate::agent::swarm::bus::SwarmMessageIntent::PublishContract
                    }
                    "query_interface" => {
                        crate::agent::swarm::bus::SwarmMessageIntent::QueryInterface
                    }
                    _ => crate::agent::swarm::bus::SwarmMessageIntent::CoordinationNote,
                };

                if let Some(swarm_dir_str) = swarm_dir_env {
                    let msg_to_post = crate::agent::swarm::bus::SwarmMessage::new(
                        swarm_id_env,
                        from,
                        to,
                        intent,
                        topic,
                        payload,
                    );
                    let msg_id = msg_to_post.id.clone();
                    let msg_topic = msg_to_post.topic.clone();

                    let post_res = tokio::task::spawn_blocking(move || {
                        let bus = crate::agent::swarm::bus::SwarmMessageBus::new(
                            std::path::Path::new(&swarm_dir_str),
                        )?;
                        bus.post_message(msg_to_post)
                    })
                    .await
                    .map_err(|e| ToolError::CommandExec(e.to_string()))?;

                    post_res.map_err(|e| ToolError::CommandExec(e.to_string()))?;

                    let dest = to
                        .map(|t| format!("to peer worker `{}`", t))
                        .unwrap_or_else(|| "as wave broadcast".to_string());

                    Ok(format!(
                        "✔ Message `{}` posted {} [{}]: `{}`",
                        msg_id,
                        dest,
                        intent.badge(),
                        msg_topic
                    ))
                } else {
                    // Fallback to in-memory bus for standalone non-swarm executions
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
pub fn parse_fanout_args(
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
    fn test_send_worker_message_schema() {
        let schemas = get_schemas();
        let tool = schemas
            .iter()
            .find(|s| s.name == "send_worker_message")
            .unwrap();
        assert!(tool.parameters["properties"]["intent"].is_object());
        assert!(tool.parameters["properties"]["payload"].is_object());
        assert!(tool.parameters["properties"]["topic"].is_object());
        assert!(tool.parameters["properties"]["to_worker_id"].is_object());
        assert!(tool.parameters["properties"]["from_worker_id"].is_object());

        let required = tool.parameters["required"].as_array().unwrap();
        assert!(required.iter().any(|v| v == "topic"));
        assert!(required.iter().any(|v| v == "payload"));
        assert!(!required.iter().any(|v| v == "from_worker_id"));

        let intent_enums = tool.parameters["properties"]["intent"]["enum"]
            .as_array()
            .unwrap();
        assert!(intent_enums.iter().any(|v| v == "publish_contract"));
        assert!(intent_enums.iter().any(|v| v == "query_interface"));
        assert!(intent_enums.iter().any(|v| v == "coordination_note"));
    }

    static TEST_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

    struct EnvGuard {
        vars: Vec<&'static str>,
    }

    impl EnvGuard {
        fn new(vars: Vec<&'static str>) -> Self {
            Self { vars }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            for var in &self.vars {
                std::env::remove_var(var);
            }
        }
    }

    #[tokio::test]
    async fn test_send_worker_message_standalone_dispatch() {
        let _lock = TEST_MUTEX.lock().unwrap();
        let _guard = EnvGuard::new(vec![
            "MINICODE_SWARM_DIR",
            "MINICODE_SWARM_TASK_ID",
            "MINICODE_SWARM_ID",
        ]);
        std::env::remove_var("MINICODE_SWARM_DIR");
        std::env::remove_var("MINICODE_SWARM_TASK_ID");

        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();

        let args = json!({
            "from_worker_id": "scout_1",
            "topic": "findings",
            "payload": "Architecture looks solid"
        });

        let res = dispatch("send_worker_message", &args, root)
            .await
            .expect("handled");
        assert!(res.is_ok());
        let msg = res.unwrap();
        assert!(msg.contains("✔ Message"));
        assert!(msg.contains("as swarm broadcast"));
        assert!(msg.contains("on topic `findings`"));
    }

    #[tokio::test]
    async fn test_send_worker_message_swarm_broadcast_dispatch() {
        let _lock = TEST_MUTEX.lock().unwrap();
        let _guard = EnvGuard::new(vec![
            "MINICODE_SWARM_DIR",
            "MINICODE_SWARM_TASK_ID",
            "MINICODE_SWARM_ID",
        ]);

        let temp_swarm = tempfile::tempdir().expect("swarm tempdir");
        std::env::set_var("MINICODE_SWARM_DIR", temp_swarm.path().to_str().unwrap());
        std::env::set_var("MINICODE_SWARM_TASK_ID", "t1_backend");
        std::env::set_var("MINICODE_SWARM_ID", "swarm-test-1");

        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();

        let args = json!({
            "intent": "publish_contract",
            "topic": "Auth Types",
            "payload": "export type Token = string;"
        });

        let res = dispatch("send_worker_message", &args, root)
            .await
            .expect("handled");
        assert!(res.is_ok());
        let msg = res.unwrap();
        assert!(msg.contains("✔ Message"));
        assert!(msg.contains("as wave broadcast"));
        assert!(msg.contains("📜 Contract"));
        assert!(msg.contains("Auth Types"));

        // Verify it was written to bus.jsonl
        let bus = crate::agent::swarm::bus::SwarmMessageBus::new(temp_swarm.path()).unwrap();
        let messages = bus.all_messages().unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].from_task, "t1_backend");
        assert_eq!(messages[0].to_task, None);
        assert_eq!(
            messages[0].intent,
            crate::agent::swarm::bus::SwarmMessageIntent::PublishContract
        );
        assert_eq!(messages[0].topic, "Auth Types");
        assert_eq!(messages[0].payload, "export type Token = string;");
    }

    #[tokio::test]
    async fn test_send_worker_message_swarm_direct_dispatch() {
        let _lock = TEST_MUTEX.lock().unwrap();
        let _guard = EnvGuard::new(vec![
            "MINICODE_SWARM_DIR",
            "MINICODE_SWARM_TASK_ID",
            "MINICODE_SWARM_ID",
        ]);

        let temp_swarm = tempfile::tempdir().expect("swarm tempdir");
        std::env::set_var("MINICODE_SWARM_DIR", temp_swarm.path().to_str().unwrap());
        std::env::set_var("MINICODE_SWARM_TASK_ID", "t2_frontend");
        std::env::set_var("MINICODE_SWARM_ID", "swarm-test-1");

        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();

        let args = json!({
            "to_worker_id": "t1_backend",
            "intent": "query_interface",
            "topic": "Token Refresh Route",
            "payload": "What is the token refresh path?"
        });

        let res = dispatch("send_worker_message", &args, root)
            .await
            .expect("handled");
        assert!(res.is_ok());
        let msg = res.unwrap();
        assert!(msg.contains("✔ Message"));
        assert!(msg.contains("to peer worker `t1_backend`"));
        assert!(msg.contains("❓ Query"));
        assert!(msg.contains("Token Refresh Route"));

        let bus = crate::agent::swarm::bus::SwarmMessageBus::new(temp_swarm.path()).unwrap();
        let messages = bus.all_messages().unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].from_task, "t2_frontend");
        assert_eq!(messages[0].to_task.as_deref(), Some("t1_backend"));
        assert_eq!(
            messages[0].intent,
            crate::agent::swarm::bus::SwarmMessageIntent::QueryInterface
        );
    }

    #[tokio::test]
    async fn test_send_worker_message_swarm_quota_and_payload_bounds() {
        let _lock = TEST_MUTEX.lock().unwrap();
        let _guard = EnvGuard::new(vec![
            "MINICODE_SWARM_DIR",
            "MINICODE_SWARM_TASK_ID",
            "MINICODE_SWARM_ID",
        ]);

        let temp_swarm = tempfile::tempdir().expect("swarm tempdir");
        std::env::set_var("MINICODE_SWARM_DIR", temp_swarm.path().to_str().unwrap());
        std::env::set_var("MINICODE_SWARM_TASK_ID", "t1_worker");

        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();

        // 1. Payload > 800 chars
        let long_payload = "X".repeat(801);
        let oversized_args = json!({
            "topic": "Too big",
            "payload": long_payload
        });
        let res_oversized = dispatch("send_worker_message", &oversized_args, root)
            .await
            .expect("handled");
        assert!(res_oversized.is_err());
        assert!(res_oversized
            .unwrap_err()
            .to_string()
            .contains("exceeds 800 characters"));

        // 2. Post 3 valid messages
        for i in 1..=3 {
            let valid_args = json!({
                "topic": format!("Note {}", i),
                "payload": format!("Valid message {}", i)
            });
            let res = dispatch("send_worker_message", &valid_args, root)
                .await
                .expect("handled");
            assert!(res.is_ok());
        }

        // 3. 4th message should fail quota
        let quota_exceeded_args = json!({
            "topic": "Note 4",
            "payload": "Quota check"
        });
        let res_quota = dispatch("send_worker_message", &quota_exceeded_args, root)
            .await
            .expect("handled");
        assert!(res_quota.is_err());
        assert!(res_quota
            .unwrap_err()
            .to_string()
            .contains("limit of 3 messages"));
    }

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

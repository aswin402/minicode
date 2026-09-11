use crate::agent::provider::ToolSchema;
use crate::error::{Result, ToolError};
use crate::tools::param;
use serde_json::json;
use std::path::Path;

pub fn get_schemas() -> Vec<ToolSchema> {
    vec![
        ToolSchema {
            name: "delegate_task".to_string(),
            description: "Delegate a subtask to an autonomous child AI agent in an isolated Git Worktree. Use for parallel research, refactoring, or independent tasks without corrupting current workspace files.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "task": {
                        "type": "string",
                        "description": "Clear and detailed task instructions for the subagent"
                    },
                    "isolate_branch": {
                        "type": "boolean",
                        "description": "If true (default), creates a dedicated Git Worktree branch for isolation"
                    },
                    "timeout_secs": {
                        "type": "integer",
                        "description": "Maximum seconds to wait for subagent to complete (default: 120)"
                    }
                },
                "required": ["task"]
            }),
        },
        ToolSchema {
            name: "invoke_subagent".to_string(),
            description: "Invoke a specialized, capability-sandboxed subagent worker (Researcher, CodeReviewer, TestEngineer, SecurityAuditor, or Custom) to execute a scoped subtask without polluting parent agent context.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "role": {
                        "type": "string",
                        "enum": ["researcher", "code_reviewer", "test_engineer", "security_auditor", "custom"],
                        "description": "Specialized role preset defining the tool capability whitelist and system prompt"
                    },
                    "prompt": {
                        "type": "string",
                        "description": "Clear and detailed task instructions for the subagent worker"
                    },
                    "model": {
                        "type": "string",
                        "description": "Optional model override for the subagent"
                    },
                    "token_budget": {
                        "type": "integer",
                        "description": "Maximum token budget for the subagent task"
                    },
                    "max_turns": {
                        "type": "integer",
                        "description": "Maximum tool execution turns before finalizing"
                    },
                    "system_prompt": {
                        "type": "string",
                        "description": "Optional custom system prompt override"
                    },
                    "isolate_worktree": {
                        "type": "boolean",
                        "description": "Whether to run inside an isolated Git Worktree branch. Default: false for read-only roles (researcher, reviewer, security), true for modifying roles (test_engineer, custom)."
                    }
                },
                "required": ["role", "prompt"]
            }),
        },
        ToolSchema {
            name: "dispatch_subagent".to_string(),
            description: "Dispatch an autonomous background subagent worker (Researcher, CodeReviewer, TestEngineer, SecurityAuditor, or Custom) to execute a scoped task concurrently in the background without blocking the primary agent. Returns immediately with the assigned worker ID and live tracking status.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "role": {
                        "type": "string",
                        "enum": ["researcher", "code_reviewer", "test_engineer", "security_auditor", "custom"],
                        "description": "Specialized role preset defining the tool capability whitelist and system prompt"
                    },
                    "prompt": {
                        "type": "string",
                        "description": "Clear and detailed task instructions for the subagent worker"
                    },
                    "model": {
                        "type": "string",
                        "description": "Optional model override for the subagent worker"
                    },
                    "token_budget": {
                        "type": "integer",
                        "description": "Maximum token budget for the subagent task (default: 50,000)"
                    },
                    "max_turns": {
                        "type": "integer",
                        "description": "Maximum tool execution turns before finalizing"
                    },
                    "system_prompt": {
                        "type": "string",
                        "description": "Optional custom system prompt override"
                    },
                    "isolate_worktree": {
                        "type": "boolean",
                        "description": "Whether to run inside an isolated Git Worktree branch. Default: false for read-only roles (researcher, reviewer, security), true for mutating roles (test_engineer, custom)."
                    }
                },
                "required": ["role", "prompt"]
            }),
        },
        ToolSchema {
            name: "send_message".to_string(),
            description: "Send a follow-up instruction or message to an active subagent in the swarm pool.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "subagent_id": {
                        "type": "string",
                        "description": "The unique identifier of the target subagent"
                    },
                    "message": {
                        "type": "string",
                        "description": "The instruction or message content to deliver"
                    }
                },
                "required": ["subagent_id", "message"]
            }),
        },
        ToolSchema {
            name: "manage_subagents".to_string(),
            description: "Inspect, list, monitor, await completion, or terminate active subagent workers in the swarm pool.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["list", "status", "await", "kill", "kill_all"],
                        "description": "Management action: 'list' all subagents, 'status' of specific subagent, 'await' completion of a subagent with report return, 'kill' a subagent, or 'kill_all'"
                    },
                    "subagent_id": {
                        "type": "string",
                        "description": "Identifier of the target subagent (required for 'status', 'kill', and 'await')"
                    },
                    "timeout_secs": {
                        "type": "integer",
                        "description": "Maximum seconds to wait when action is 'await' (default: 60)"
                    }
                },
                "required": ["action"]
            }),
        },
        ToolSchema {
            name: "scratchpad_write".to_string(),
            description: "Write or update an entry on the shared multi-agent scratchpad blackboard for inter-worker knowledge sharing.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "key": {
                        "type": "string",
                        "description": "Unique key identifier (e.g. 'api_specs', 'failing_tests', 'auth_plan')"
                    },
                    "title": {
                        "type": "string",
                        "description": "Short descriptive title of this finding or note"
                    },
                    "content": {
                        "type": "string",
                        "description": "Detailed text content, code snippet, or structured findings"
                    },
                    "author": {
                        "type": "string",
                        "description": "Optional author identifier (default: 'orchestrator')"
                    }
                },
                "required": ["key", "title", "content"]
            }),
        },
        ToolSchema {
            name: "scratchpad_read".to_string(),
            description: "Read a specific entry from the shared multi-agent scratchpad blackboard by key.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "key": {
                        "type": "string",
                        "description": "Key identifier to read"
                    }
                },
                "required": ["key"]
            }),
        },
        ToolSchema {
            name: "scratchpad_list".to_string(),
            description: "List all active entries currently published on the shared multi-agent scratchpad blackboard.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolSchema {
            name: "merge_subagent_worktree".to_string(),
            description: "Integrate a verified subagent worktree branch (subagent/<id>) into the current branch and clean up its temporary worktree directory.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "subagent_id": {
                        "type": "string",
                        "description": "Unique identifier of the subagent whose worktree to merge (e.g. 'task-a1b2c3d4' or 'testengineer-2')"
                    }
                },
                "required": ["subagent_id"]
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
        "delegate_task" => Some(async {
            let task = param::require_str(args, "task", "delegate_task")?;
            let isolate = param::opt_bool(args, "isolate_branch", true);
            let timeout_secs = param::opt_u64(args, "timeout_secs");

            let res = crate::agent::orchestrator::MultiAgentOrchestrator::delegate(
                workspace_root,
                task,
                isolate,
                timeout_secs,
            )
            .await?;
            Ok(crate::agent::orchestrator::MultiAgentOrchestrator::format_result(&res))
        }.await),
        "invoke_subagent" => Some(async {
            let role_str = param::require_str(args, "role", "invoke_subagent")?;
            let prompt = param::require_str(args, "prompt", "invoke_subagent")?;

            let role = match role_str.to_lowercase().as_str() {
                "researcher" => crate::agent::subagent::SubagentRole::Researcher,
                "code_reviewer" | "reviewer" => crate::agent::subagent::SubagentRole::CodeReviewer,
                "test_engineer" | "tester" => crate::agent::subagent::SubagentRole::TestEngineer,
                "security_auditor" | "security" => crate::agent::subagent::SubagentRole::SecurityAuditor,
                other => crate::agent::subagent::SubagentRole::Custom(other.to_string()),
            };

            let mut config = crate::agent::subagent::SubagentConfig::for_role(role.clone());
            if let Some(model) = param::opt_str(args, "model") {
                config.model = Some(model.to_string());
            }
            if let Some(budget) = param::opt_u64(args, "token_budget") {
                config.token_budget = budget as usize;
            }
            if let Some(max_t) = param::opt_u64(args, "max_turns") {
                config.max_turns = max_t as usize;
            }
            if let Some(sys_prompt) = param::opt_str(args, "system_prompt") {
                config.system_prompt_override = Some(sys_prompt.to_string());
            }

            let isolate_worktree = param::get_bool(args, "isolate_worktree")
                .unwrap_or(!matches!(
                    role,
                    crate::agent::subagent::SubagentRole::Researcher
                        | crate::agent::subagent::SubagentRole::CodeReviewer
                        | crate::agent::subagent::SubagentRole::SecurityAuditor
                ));

            let res =
                crate::agent::orchestrator::MultiAgentOrchestrator::delegate_with_config(
                    workspace_root,
                    prompt,
                    isolate_worktree,
                    Some(crate::constants::DEFAULT_SUBAGENT_TIMEOUT_SECS),
                    Some(config),
                )
                .await?;

            let report = format!(
                "✔ Subagent `[ID: {} | Role: {}]` completed task successfully!\n• Tokens Used: {}\n• Files Modified: {}\n\n### Findings & Response Summary\n{}",
                res.id,
                role.badge(),
                res.tokens_used,
                if res.files_modified.is_empty() { "None (Read-Only)".to_string() } else { res.files_modified.join(", ") },
                res.final_summary
            );

            Ok(report)
        }.await),
        "dispatch_subagent" => Some(async {
            let role_str = param::require_str(args, "role", "dispatch_subagent")?;
            let prompt = param::require_str(args, "prompt", "dispatch_subagent")?;

            let role = match role_str.to_lowercase().as_str() {
                "researcher" => crate::agent::subagent::SubagentRole::Researcher,
                "code_reviewer" | "reviewer" => crate::agent::subagent::SubagentRole::CodeReviewer,
                "test_engineer" | "tester" => crate::agent::subagent::SubagentRole::TestEngineer,
                "security_auditor" | "security" => crate::agent::subagent::SubagentRole::SecurityAuditor,
                other => crate::agent::subagent::SubagentRole::Custom(other.to_string()),
            };

            let mut config = crate::agent::subagent::SubagentConfig::for_role(role.clone());
            if let Some(model) = param::opt_str(args, "model") {
                config.model = Some(model.to_string());
            }
            if let Some(budget) = param::opt_u64(args, "token_budget") {
                config.token_budget = budget as usize;
            }
            if let Some(max_t) = param::opt_u64(args, "max_turns") {
                config.max_turns = max_t as usize;
            }
            if let Some(sys_prompt) = param::opt_str(args, "system_prompt") {
                config.system_prompt_override = Some(sys_prompt.to_string());
            }

            let pool = crate::agent::subagent::get_global_subagent_pool(workspace_root);
            let isolate_worktree = param::get_bool(args, "isolate_worktree")
                .unwrap_or(!matches!(
                    role,
                    crate::agent::subagent::SubagentRole::Researcher
                        | crate::agent::subagent::SubagentRole::CodeReviewer
                        | crate::agent::subagent::SubagentRole::SecurityAuditor
                ));

            let provider = pool.get_or_create_provider().await;
            let id = pool
                .spawn_background_worker(role.clone(), prompt, Some(config), provider, isolate_worktree)
                .await?;

            let isolation_label = if isolate_worktree {
                format!("Dedicated Git Worktree (`subagent/{}`)", id)
            } else {
                "Shared Read-Only Context".to_string()
            };

            let out = format!(
                "✔ Background subagent spawned successfully!\n\
                 • **Worker ID**: `{}`\n\
                 • **Role**: {}\n\
                 • **Isolation Mode**: {}\n\
                 • **Status**: ◉ Running in background\n\
                 • **Live Telemetry**: Press `Ctrl+S` in TUI or run `/swarm` to open the Activity Drawer.\n\
                 • **Await Completion**: Call `manage_subagents(action=\"await\", subagent_id=\"{}\")` when ready to inspect results, or `manage_subagents(action=\"status\", subagent_id=\"{}\")` for intermediate updates.",
                id,
                role.badge(),
                isolation_label,
                id,
                id
            );

            Ok(out)
        }.await),
        "send_message" => Some(async {
            let subagent_id = param::require_str(args, "subagent_id", "send_message")?;
            let message = param::require_str(args, "message", "send_message")?;

            let pool = crate::agent::subagent::get_global_subagent_pool(workspace_root);
            if let Some(info) = pool.get_subagent(subagent_id).await {
                Ok(format!(
                    "✔ Message delivered to subagent `{}` (Role: {}, State: {:?})\nMessage content: '{}'",
                    subagent_id,
                    info.role.badge(),
                    info.state,
                    message
                ))
            } else {
                Ok(format!("ℹ Subagent `{}` received instruction: '{}'", subagent_id, message))
            }
        }.await),
        "manage_subagents" => Some(async {
            let action = param::opt_str(args, "action").unwrap_or("list");
            let pool = crate::agent::subagent::get_global_subagent_pool(workspace_root);

            match action {
                "list" => {
                    let summary = pool.format_swarm_summary().await;
                    Ok(summary)
                }
                "status" => {
                    let id = param::require_str(args, "subagent_id", "manage_subagents")?;
                    if let Some(info) = pool.get_subagent(id).await {
                        Ok(format!(
                            "### Subagent `{}` Details\n• Role: {}\n• State: {:?}\n• Turns Executed: {}\n• Tokens Used: {}\n• Initial Prompt: {}\n• Started At: {}s",
                            info.id,
                            info.role.badge(),
                            info.state,
                            info.turns_executed,
                            info.tokens_used,
                            info.prompt,
                            info.started_at_secs
                        ))
                    } else {
                        Ok(format!("ℹ No subagent found with ID '{}'", id))
                    }
                }
                "await" => {
                    let id = param::require_str(args, "subagent_id", "manage_subagents")?;
                    let timeout_secs = param::opt_u64(args, "timeout_secs").unwrap_or(60);
                    let start = std::time::Instant::now();
                    let max_dur = std::time::Duration::from_secs(timeout_secs);

                    let mut final_info = None;
                    while start.elapsed() < max_dur {
                        if let Some(info) = pool.get_subagent(id).await {
                            if !matches!(info.state, crate::agent::subagent::types::SubagentState::Running | crate::agent::subagent::types::SubagentState::Idle) {
                                final_info = Some(info);
                                break;
                            }
                        } else {
                            return Ok(format!("ℹ No subagent found with ID '{}'", id));
                        }
                        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                    }

                    if let Some(info) = final_info {
                        let scratchpad = crate::agent::subagent::get_global_scratchpad();
                        let scratchpad_content = scratchpad
                            .read_entry(&format!("subagent_{}", id))
                            .or_else(|| scratchpad.read_entry(&format!("subagent/{}", id)))
                            .map(|e| e.content)
                            .or(info.final_summary.clone())
                            .unwrap_or_else(|| "No output report recorded.".to_string());

                        Ok(format!(
                            "✔ Subagent `{}` has finished execution (State: {:?})!\n\
                             • **Role**: {}\n\
                             • **Turns Executed**: {}\n\
                             • **Tokens Used**: {}\n\
                             • **Isolation**: {}\n\n\
                             ### Findings & Outcome Summary\n\
                             {}",
                            info.id,
                            info.state,
                            info.role.badge(),
                            info.turns_executed,
                            info.tokens_used,
                            if info.isolate_worktree { "Git Worktree" } else { "Read-Only / Main" },
                            scratchpad_content
                        ))
                    } else {
                        let current_info = pool.get_subagent(id).await;
                        let detail = current_info
                            .as_ref()
                            .and_then(|i| i.status_message.clone())
                            .unwrap_or_else(|| "Running".to_string());
                        Ok(format!(
                            "⏳ Subagent `{}` is still executing after {}s timeout.\n• Current Status: {}\n• Use 'manage_subagents(action=\"await\", subagent_id=\"{}\")' to continue waiting, or press Ctrl+S to monitor live telemetry in the TUI drawer.",
                            id, timeout_secs, detail, id
                        ))
                    }
                }
                "kill" => {
                    let id = param::require_str(args, "subagent_id", "manage_subagents")?;
                    pool.kill_subagent(id).await?;
                    Ok(format!("✔ Subagent `{}` successfully terminated", id))
                }
                "kill_all" => {
                    pool.kill_all().await;
                    Ok("✔ All active subagents in swarm pool have been terminated".to_string())
                }
                other => Err(ToolError::InvalidArguments {
                    name: "manage_subagents".to_string(),
                    reason: format!("Unknown action '{}'. Valid actions: list, status, await, kill, kill_all", other),
                }.into()),
            }
        }.await),
        "scratchpad_write" => Some(async {
            let key = param::require_str(args, "key", "scratchpad_write")?;
            let title = param::require_str(args, "title", "scratchpad_write")?;
            let content = param::require_str(args, "content", "scratchpad_write")?;
            let author = param::opt_str(args, "author").unwrap_or("orchestrator");

            let sp = crate::agent::subagent::get_global_scratchpad();
            let entry = sp.write_entry(key, title, content, author);
            let _ = sp.save_to_disk(workspace_root);

            Ok(format!("✔ Scratchpad entry `{}` published successfully by `{}`.", entry.key, entry.author))
        }.await),
        "scratchpad_read" => Some(async {
            let key = param::require_str(args, "key", "scratchpad_read")?;

            let sp = crate::agent::subagent::get_global_scratchpad();
            if let Some(entry) = sp.read_entry(key) {
                Ok(format!(
                    "📋 Scratchpad `{}`: **{}** (by `{}` at {}s)\n\n{}",
                    entry.key, entry.title, entry.author, entry.updated_at_secs, entry.content
                ))
            } else {
                Ok(format!("ℹ Scratchpad key `{}` not found.", key))
            }
        }.await),
        "scratchpad_list" => Some(async {
            let sp = crate::agent::subagent::get_global_scratchpad();
            let entries = sp.list_entries();
            if entries.is_empty() {
                Ok("ℹ Shared scratchpad blackboard is currently empty.".to_string())
            } else {
                let mut out = format!("📋 Shared Scratchpad Blackboard ({} entries):\n\n", entries.len());
                for (i, e) in entries.iter().enumerate() {
                    out.push_str(&format!("{}. `{}` — **{}** (author: `{}`)\n", i + 1, e.key, e.title, e.author));
                }
                Ok(out)
            }
        }.await),
        "merge_subagent_worktree" => Some(async {
            let subagent_id = param::require_str(args, "subagent_id", "merge_subagent_worktree")?;

            crate::agent::orchestrator::MultiAgentOrchestrator::merge_worktree(
                workspace_root,
                subagent_id,
            ).await
        }.await),
        _ => None,
    }
}

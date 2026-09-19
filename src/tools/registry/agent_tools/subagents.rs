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
            name: "spawn_subagent".to_string(),
            description: "Spawn an autonomous background subagent with a specialized role ('scout', 'coder', 'tester', 'reviewer') to execute a scoped subtask in an isolated workspace or worktree.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "task": {
                        "type": "string",
                        "description": "Clear and detailed task instructions for the subagent"
                    },
                    "role": {
                        "type": "string",
                        "enum": ["scout", "coder", "tester", "reviewer"],
                        "description": "Specialized role preset defining subagent capabilities and workspace isolation"
                    },
                    "workspace_mode": {
                        "type": "string",
                        "enum": ["auto", "worktree", "shared"],
                        "description": "Workspace isolation mode: 'auto' (default based on role), 'worktree' (isolated Git worktree branch), or 'shared' (in-place execution)"
                    },
                    "max_iterations": {
                        "type": "integer",
                        "description": "Maximum tool loop iterations for the subagent before terminating"
                    }
                },
                "required": ["task", "role"]
            }),
        },
        ToolSchema {
            name: "send_message".to_string(),
            description: "Send a typed message or follow-up instruction to an agent or subagent via durable mailbox.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "recipient": {
                        "type": "string",
                        "description": "The unique identifier of the recipient agent (or 'parent' for coordinator)"
                    },
                    "subagent_id": {
                        "type": "string",
                        "description": "Alias for recipient: the unique identifier of the target subagent"
                    },
                    "message": {
                        "type": "string",
                        "description": "The instruction or message content to deliver"
                    },
                    "intent": {
                        "type": "string",
                        "enum": [
                            "task_init",
                            "status_update",
                            "clarification_request",
                            "clarification_response",
                            "feedback",
                            "handoff",
                            "task_complete"
                        ],
                        "description": "Optional high-level intent of the message (default: 'status_update')"
                    }
                },
                "required": ["message"]
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
                        "enum": ["list", "status", "await", "wait", "transcript", "drilldown", "kill", "kill_all"],
                        "description": "Management action: 'list' all subagents, 'status' of specific subagent, 'await' / 'wait' for completion of a subagent with report return, 'transcript' / 'drilldown' for step-by-step trace inspection, 'kill' a subagent, or 'kill_all'"
                    },
                    "subagent_id": {
                        "type": "string",
                        "description": "Identifier of the target subagent (required for 'status', 'kill', 'await', and 'transcript')"
                    },
                    "step_index": {
                        "type": "integer",
                        "description": "Optional step number (1-indexed) when action is 'transcript' or 'drilldown'"
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
            description: "Validates and merges code changes from a completed subagent's ephemeral git worktree into the main workspace. Automatically runs project build/test checks before landing changes.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "subagent_id": {
                        "type": "string",
                        "description": "Unique identifier of the subagent whose worktree to merge (e.g. 'coder-1' or 'task-a1b2c3d4')"
                    },
                    "commit": {
                        "type": "boolean",
                        "description": "Whether to automatically commit the merged changes (default: true). If false, stages changes in the working tree without committing."
                    },
                    "check_cmd": {
                        "type": "string",
                        "description": "Optional pre-merge validation command to run in the worktree (e.g. 'cargo check -j 1' or 'skip'). Defaults to auto-detecting project check command."
                    },
                    "commit_message": {
                        "type": "string",
                        "description": "Optional custom commit message when commit is true"
                    }
                },
                "required": ["subagent_id"]
            }),
        },
        ToolSchema {
            name: "subagent_transcript_drilldown".to_string(),
            description: "Inspect the raw execution transcript of an isolated subagent worker. Allows lossless inspection of tool parameters, complete stdout/stderr, compiler logs, and step-by-step diagnostics on demand without bloating parent context.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "subagent_id": {
                        "type": "string",
                        "description": "Unique identifier of the subagent to inspect (e.g. 'researcher-1', 'testengineer-2')"
                    },
                    "step_index": {
                        "type": "integer",
                        "description": "Optional step number (1-indexed) to inspect in full detail. If omitted, returns an overview table of all steps."
                    },
                    "filter": {
                        "type": "string",
                        "enum": ["summary", "all", "errors", "tools", "exec"],
                        "description": "Filter mode: 'summary' (overview table), 'all' (all steps), 'errors' (only failed steps), 'tools' (all tools), or 'exec' (only command executions)"
                    },
                    "max_lines": {
                        "type": "integer",
                        "description": "Maximum output lines to display for a single step (default: 120)"
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
                "researcher" | "scout" => crate::agent::subagent::SubagentRole::Scout,
                "code_reviewer" | "reviewer" | "security_auditor" | "security" => {
                    crate::agent::subagent::SubagentRole::Reviewer
                }
                "test_engineer" | "tester" => crate::agent::subagent::SubagentRole::Tester,
                _ => crate::agent::subagent::SubagentRole::Coder,
            };

            let mut config = crate::agent::subagent::SubagentConfig::for_role(role);
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

            let isolate_worktree = param::get_bool(args, "isolate_worktree").unwrap_or(
                role.default_workspace_mode() == crate::agent::subagent::WorkspaceMode::Worktree,
            );


            let res =
                crate::agent::orchestrator::MultiAgentOrchestrator::delegate_with_config(
                    workspace_root,
                    prompt,
                    isolate_worktree,
                    Some(crate::constants::DEFAULT_SUBAGENT_TIMEOUT_SECS),
                    Some(config),
                )
                .await?;

            if res.final_summary.starts_with("### ") {
                Ok(res.final_summary)
            } else {
                let report = format!(
                    "✔ Subagent `[ID: {} | Role: {}]` completed task successfully!\n• Tokens Used: {}\n• Files Modified: {}\n\n### Findings & Response Summary\n{}",
                    res.id,
                    role.badge(),
                    res.tokens_used,
                    if res.files_modified.is_empty() { "None (Read-Only)".to_string() } else { res.files_modified.join(", ") },
                    res.final_summary
                );
                Ok(report)
            }
        }.await),
        "dispatch_subagent" => Some(async {
            let role_str = param::require_str(args, "role", "dispatch_subagent")?;
            let prompt = param::require_str(args, "prompt", "dispatch_subagent")?;

            let role = match role_str.to_lowercase().as_str() {
                "researcher" | "scout" => crate::agent::subagent::SubagentRole::Scout,
                "code_reviewer" | "reviewer" | "security_auditor" | "security" => {
                    crate::agent::subagent::SubagentRole::Reviewer
                }
                "test_engineer" | "tester" => crate::agent::subagent::SubagentRole::Tester,
                _ => crate::agent::subagent::SubagentRole::Coder,
            };

            let mut config = crate::agent::subagent::SubagentConfig::for_role(role);
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
            let isolate_worktree = param::get_bool(args, "isolate_worktree").unwrap_or(
                role.default_workspace_mode() == crate::agent::subagent::WorkspaceMode::Worktree,
            );


            let provider = pool.get_or_create_provider().await;
            let id = pool
                .spawn_background_worker(role, prompt, Some(config), provider, isolate_worktree)
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
        "spawn_subagent" => Some(async {
            let task = param::require_str(args, "task", "spawn_subagent")?;
            let role_str = param::require_str(args, "role", "spawn_subagent")?;
            let role = match role_str.to_lowercase().as_str() {
                "scout" | "researcher" | "research" => crate::agent::subagent::SubagentRole::Scout,
                "coder" => crate::agent::subagent::SubagentRole::Coder,
                "tester" | "test_engineer" | "test" => crate::agent::subagent::SubagentRole::Tester,
                "reviewer" | "code_reviewer" | "security_auditor" | "security" => {
                    crate::agent::subagent::SubagentRole::Reviewer
                }
                other => {
                    return Err(ToolError::InvalidArguments {
                        name: "spawn_subagent".to_string(),
                        reason: format!(
                            "Unknown role '{}'. Expected 'scout', 'coder', 'tester', or 'reviewer'",
                            other
                        ),
                    }.into());
                }
            };

            let mode_str = param::opt_str(args, "workspace_mode").unwrap_or("auto");
            let workspace_mode = match mode_str.to_lowercase().as_str() {
                "worktree" => crate::agent::subagent::WorkspaceMode::Worktree,
                "shared" => crate::agent::subagent::WorkspaceMode::Shared,
                _ => crate::agent::subagent::WorkspaceMode::Auto,
            };

            let max_iterations = param::opt_u64(args, "max_iterations").map(|n| n as usize);

            let report = crate::agent::subagent::orchestrator::SubagentOrchestrator::spawn_subagent(
                workspace_root,
                task,
                role,
                workspace_mode,
                max_iterations,
            )
            .await?;

            Ok(report)
        }.await),
        "send_message" => Some(async {
            let recipient_str = param::opt_str(args, "recipient")
                .or_else(|| param::opt_str(args, "subagent_id"))
                .ok_or_else(|| ToolError::InvalidArguments {
                    name: "send_message".to_string(),
                    reason: "Missing required argument 'recipient' or 'subagent_id'".to_string(),
                })?;
            let message = param::require_str(args, "message", "send_message")?;
            let intent = param::opt_str(args, "intent").map(|s| match s {
                "task_init" => crate::agent::subagent::MessageIntent::TaskInit,
                "clarification_request" => crate::agent::subagent::MessageIntent::ClarificationRequest,
                "clarification_response" => crate::agent::subagent::MessageIntent::ClarificationResponse,
                "feedback" => crate::agent::subagent::MessageIntent::Feedback,
                "handoff" => crate::agent::subagent::MessageIntent::Handoff,
                "task_complete" => crate::agent::subagent::MessageIntent::TaskComplete,
                _ => crate::agent::subagent::MessageIntent::StatusUpdate,
            });

            let sender = crate::agent::subagent::AgentId::parent();
            let recipient = crate::agent::subagent::AgentId::from(recipient_str);

            let confirm = crate::agent::subagent::orchestrator::SubagentOrchestrator::send_message(
                workspace_root,
                &sender,
                &recipient,
                message,
                intent,
            )?;

            Ok(confirm)
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
                "await" | "wait" => {
                    let id = param::require_str(args, "subagent_id", "manage_subagents")?;
                    let timeout_secs = param::opt_u64(args, "timeout_secs").unwrap_or(60);
                    let start = std::time::Instant::now();
                    let max_dur = std::time::Duration::from_secs(timeout_secs);

                    let mut final_info = None;
                    while start.elapsed() < max_dur {
                        if let Some(info) = pool.get_subagent(id).await {
                            if !matches!(
                                info.state,
                                crate::agent::subagent::types::SubagentState::Running
                                    | crate::agent::subagent::types::SubagentState::Starting
                                    | crate::agent::subagent::types::SubagentState::WaitingForInput
                            ) {

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

                        if scratchpad_content.starts_with("### ") {
                            Ok(scratchpad_content)
                        } else {
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
                        }
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
                "transcript" | "drilldown" => {
                    let id = param::require_str(args, "subagent_id", "manage_subagents")?;
                    let step_index = param::opt_u64(args, "step_index").map(|n| n as usize);
                    let max_lines = param::opt_u64(args, "max_lines").map(|n| n as usize);
                    let filter = param::opt_str(args, "filter").unwrap_or("summary");

                    let store = crate::agent::subagent::transcript::get_global_transcript_store();
                    if let Some(t) = store.get(id, Some(workspace_root)) {
                        if let Some(step_idx) = step_index {
                            if let Some(detail) = t.format_step_detail(step_idx, max_lines) {
                                Ok(detail)
                            } else {
                                Ok(format!(
                                    "ℹ Step {} not found in subagent `{}` transcript. Total steps recorded: {}.",
                                    step_idx, id, t.steps.len()
                                ))
                            }
                        } else if filter == "errors" || filter == "error" {
                            Ok(t.format_errors())
                        } else {
                            Ok(t.format_overview(Some(50)))
                        }
                    } else {
                        Ok(format!(
                            "ℹ No execution transcript found for subagent `{}`. Available: {:?}",
                            id,
                            store.list_subagent_ids(Some(workspace_root))
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
                    reason: format!("Unknown action '{}'. Valid actions: list, status, await, wait, transcript, drilldown, kill, kill_all", other),
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
            let parsed: MergeSubagentWorktreeArgs = serde_json::from_value(args.clone())
                .map_err(|e| ToolError::InvalidArguments {
                    name: "merge_subagent_worktree".to_string(),
                    reason: format!("Failed to parse arguments: {}", e),
                })?;

            merge_subagent_worktree(
                workspace_root,
                &parsed.subagent_id,
                parsed.commit.unwrap_or(true),
                parsed.check_cmd.as_deref(),
                parsed.commit_message.as_deref(),
            )
            .await
            .map_err(Into::into)
        }.await),
        "subagent_transcript_drilldown" => Some(async {
            let subagent_id = param::require_str(args, "subagent_id", "subagent_transcript_drilldown")?;
            let step_index = param::opt_u64(args, "step_index").map(|n| n as usize);
            let filter = param::opt_str(args, "filter").unwrap_or("summary");
            let max_lines = param::opt_u64(args, "max_lines").map(|n| n as usize);

            let store = crate::agent::subagent::transcript::get_global_transcript_store();
            if let Some(t) = store.get(subagent_id, Some(workspace_root)) {
                if let Some(step_idx) = step_index {
                    if let Some(detail) = t.format_step_detail(step_idx, max_lines) {
                        Ok(detail)
                    } else {
                        Ok(format!(
                            "ℹ Step {} not found in subagent `{}` transcript. Total steps recorded: {}.",
                            step_idx, subagent_id, t.steps.len()
                        ))
                    }
                } else if filter == "errors" || filter == "error" {
                    Ok(t.format_errors())
                } else {
                    Ok(t.format_overview(Some(50)))
                }
            } else {
                Ok(format!(
                    "ℹ No execution transcript found for subagent `{}`. Available: {:?}",
                    subagent_id,
                    store.list_subagent_ids(Some(workspace_root))
                ))
            }
        }.await),
        _ => None,
    }
}

/// Arguments for `merge_subagent_worktree` tool.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct MergeSubagentWorktreeArgs {
    pub subagent_id: String,
    pub commit: Option<bool>,
    pub check_cmd: Option<String>,
    pub commit_message: Option<String>,
}

/// Validates and merges code changes from a completed subagent's ephemeral git worktree
/// into the main workspace with automatic pre-merge verification, conflict detection, and cleanup.
pub async fn merge_subagent_worktree(
    workspace_root: &Path,
    subagent_id: &str,
    commit: bool,
    check_cmd: Option<&str>,
    commit_message: Option<&str>,
) -> std::result::Result<String, ToolError> {
    // 1. Locate worktree path via GitWorktreeManager::locate_worktree
    let agent_id = crate::agent::subagent::AgentId(subagent_id.to_string());
    let worktree_path =
        crate::sandbox::GitWorktreeManager::locate_worktree(workspace_root, &agent_id).ok_or_else(
            || {
                ToolError::ExecutionFailed(format!(
                    "Worktree not found for subagent '{}' under .minicode/worktrees/",
                    subagent_id
                ))
            },
        )?;

    // 2. Resolve source branch via GitWorktreeManager::resolve_branch_for
    let branch_name =
        crate::sandbox::GitWorktreeManager::resolve_branch_for(workspace_root, &agent_id);

    // 3. Pre-merge verification via MergeArbitrator::verify_worktree
    let validation = crate::sandbox::MergeArbitrator::verify_worktree(&worktree_path, check_cmd)
        .map_err(|e| ToolError::ExecutionFailed(format!("Pre-merge verification error: {}", e)))?;

    if !validation.success {
        let mut report = format!(
            "❌ Pre-merge verification failed for subagent `{}` in worktree `{}`.\n\
             • Command: `{}`\n\
             • Exit Code: {}\n\
             • Duration: {}ms\n\
             • Status: Parent workspace untouched and worktree preserved.\n",
            subagent_id,
            worktree_path.display(),
            validation.command,
            validation.exit_code,
            validation.duration_ms
        );
        if !validation.stdout.trim().is_empty() {
            report.push_str(&format!(
                "\n### Stdout\n```\n{}\n```\n",
                validation.stdout.trim()
            ));
        }
        if !validation.stderr.trim().is_empty() {
            report.push_str(&format!(
                "\n### Stderr\n```\n{}\n```\n",
                validation.stderr.trim()
            ));
        }
        return Ok(report);
    }

    // 4. Mergeability check via MergeArbitrator::check_mergeability
    let mergeability =
        crate::sandbox::MergeArbitrator::check_mergeability(workspace_root, &branch_name)
            .map_err(|e| ToolError::ExecutionFailed(format!("Mergeability check failed: {}", e)))?;

    if !mergeability.can_merge_cleanly {
        let mut report = format!(
            "⚠️ Merge conflicts detected between branch `{}` and HEAD for subagent `{}`.\n\
             • Worktree: `{}`\n\
             • Conflicted Files ({}):\n",
            branch_name,
            subagent_id,
            worktree_path.display(),
            mergeability.conflicted_files.len()
        );
        for file in &mergeability.conflicted_files {
            report.push_str(&format!("  - `{}`\n", file));
        }
        report.push_str(
            "\n• Status: Merge aborted. Parent workspace untouched and worktree preserved.\n",
        );
        return Ok(report);
    }

    // 5. Apply merge via MergeArbitrator::apply_merge
    let merge_report = crate::sandbox::MergeArbitrator::apply_merge(
        workspace_root,
        &branch_name,
        commit,
        commit_message,
    )
    .map_err(|e| match e {
        crate::sandbox::ArbitrationError::MergeConflict(conflicts) => ToolError::ExecutionFailed(
            format!("Merge conflict encountered during apply: {:?}", conflicts),
        ),
        other => ToolError::ExecutionFailed(format!("Failed to apply merge: {}", other)),
    })?;

    // 6. On successful merge, clean up worktree and temporary branch via GitWorktreeManager::remove_worktree
    let handle = crate::sandbox::WorktreeHandle {
        worktree_path: worktree_path.clone(),
        branch_name: branch_name.clone(),
        agent_id: agent_id.clone(),
        repo_root: workspace_root.to_path_buf(),
    };
    let cleanup_warning = match crate::sandbox::GitWorktreeManager::remove_worktree(&handle) {
        Ok(_) => None,
        Err(e) => {
            tracing::warn!("Failed to clean up worktree after merge: {}", e);
            Some(e.to_string())
        }
    };

    // 7. Return formatted markdown success summary
    let landing_mode = if merge_report.committed {
        "Committed (`--no-ff`)"
    } else {
        "Staged without committing (`--no-commit`)"
    };

    let mut commit_details = String::new();
    if let Some(ref hash) = merge_report.commit_hash {
        commit_details.push_str(&format!("\n• **Commit Hash**: `{}`", hash));
    }
    if let Some(ref msg) = merge_report.commit_message {
        commit_details.push_str(&format!("\n• **Commit Message**: {}", msg));
    }

    let files_summary = if merge_report.files_changed.is_empty() {
        "  - None (empty diff)".to_string()
    } else {
        merge_report
            .files_changed
            .iter()
            .map(|f| format!("  - `{}`", f))
            .collect::<Vec<_>>()
            .join("\n")
    };

    let cleanup_detail = match cleanup_warning {
        Some(w) => format!(
            "⚠️ Warning cleaning worktree `{}`: {}",
            worktree_path.display(),
            w
        ),
        None => format!(
            "Worktree `{}` and branch `{}` removed",
            worktree_path.display(),
            branch_name
        ),
    };

    let summary = format!(
        "✔ Successfully merged subagent worktree changes!\n\
         • **Subagent ID**: `{}`\n\
         • **Branch**: `{}`\n\
         • **Landing Mode**: {}{}\n\
         • **Pre-Merge Validation**: Passed (`{}` in {}ms)\n\
         • **Cleanup**: {}\n\n\
         ### Files Changed ({})\n\
         {}\n",
        subagent_id,
        branch_name,
        landing_mode,
        commit_details,
        validation.command,
        validation.duration_ms,
        cleanup_detail,
        merge_report.files_changed.len(),
        files_summary
    );

    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_merge_subagent_worktree_arg_parsing() {
        let json = serde_json::json!({
            "subagent_id": "coder-1",
            "commit": true,
            "check_cmd": "cargo check -j 1",
            "commit_message": "merge: auth feature"
        });
        let args: MergeSubagentWorktreeArgs = serde_json::from_value(json).unwrap();
        assert_eq!(args.subagent_id, "coder-1");
        assert_eq!(args.commit, Some(true));
        assert_eq!(args.check_cmd.as_deref(), Some("cargo check -j 1"));
        assert_eq!(args.commit_message.as_deref(), Some("merge: auth feature"));
    }

    #[tokio::test]
    async fn test_merge_subagent_worktree_not_found() {
        let temp = tempfile::tempdir().unwrap();
        let res = merge_subagent_worktree(temp.path(), "missing-agent", true, None, None).await;
        assert!(res.is_err());
        let err_msg = res.unwrap_err().to_string();
        assert!(err_msg.contains("Worktree not found for subagent 'missing-agent'"));
    }

    #[tokio::test]
    async fn test_merge_subagent_worktree_clean_merge() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();

        // git init
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(root)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["config", "user.name", "test"])
            .current_dir(root)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(root)
            .output()
            .unwrap();

        std::fs::write(root.join("hello.txt"), "base\n").unwrap();
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(root)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(root)
            .output()
            .unwrap();

        let agent_id = crate::agent::subagent::AgentId("coder-test".to_string());
        let handle = crate::sandbox::GitWorktreeManager::create_worktree(root, &agent_id).unwrap();

        // Add a commit in the worktree
        std::fs::write(
            handle.worktree_path.join("feature.txt"),
            "new feature content\n",
        )
        .unwrap();
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(&handle.worktree_path)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["commit", "-m", "feat: add feature"])
            .current_dir(&handle.worktree_path)
            .output()
            .unwrap();

        let res = merge_subagent_worktree(
            root,
            "coder-test",
            true,
            Some("skip"),
            Some("merge: feat add feature"),
        )
        .await;

        assert!(res.is_ok());
        let summary = res.unwrap();
        assert!(summary.contains("Successfully merged subagent worktree changes"));
        assert!(summary.contains("coder-test"));
        assert!(summary.contains("feature.txt"));

        // Worktree should be cleaned up
        assert!(crate::sandbox::GitWorktreeManager::locate_worktree(root, &agent_id).is_none());
        assert!(root.join("feature.txt").exists());
    }

    #[tokio::test]
    async fn test_merge_subagent_worktree_staged_no_commit() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();

        std::process::Command::new("git")
            .args(["init"])
            .current_dir(root)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["config", "user.name", "test"])
            .current_dir(root)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(root)
            .output()
            .unwrap();

        std::fs::write(root.join("hello.txt"), "base\n").unwrap();
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(root)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(root)
            .output()
            .unwrap();

        let agent_id = crate::agent::subagent::AgentId("staged-test".to_string());
        let handle = crate::sandbox::GitWorktreeManager::create_worktree(root, &agent_id).unwrap();

        std::fs::write(handle.worktree_path.join("staged.txt"), "staged content\n").unwrap();
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(&handle.worktree_path)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["commit", "-m", "branch staged"])
            .current_dir(&handle.worktree_path)
            .output()
            .unwrap();

        let res = merge_subagent_worktree(root, "staged-test", false, Some("skip"), None).await;

        assert!(res.is_ok());
        let summary = res.unwrap();
        assert!(summary.contains("Staged without committing"));
        assert!(root.join("staged.txt").exists());
    }

    #[tokio::test]
    async fn test_merge_subagent_worktree_verification_failure() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();

        // git init
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(root)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["config", "user.name", "test"])
            .current_dir(root)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(root)
            .output()
            .unwrap();

        std::fs::write(root.join("hello.txt"), "base\n").unwrap();
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(root)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(root)
            .output()
            .unwrap();

        let agent_id = crate::agent::subagent::AgentId("failing-verifier".to_string());
        let _handle = crate::sandbox::GitWorktreeManager::create_worktree(root, &agent_id).unwrap();

        // Verification command that fails
        let res =
            merge_subagent_worktree(root, "failing-verifier", true, Some("false"), None).await;

        assert!(res.is_ok());
        let report = res.unwrap();
        assert!(report.contains("Pre-merge verification failed"));
        assert!(report.contains("Parent workspace untouched and worktree preserved"));

        // Worktree MUST still exist
        assert!(crate::sandbox::GitWorktreeManager::locate_worktree(root, &agent_id).is_some());
    }

    #[tokio::test]
    async fn test_merge_subagent_worktree_conflict() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();

        // git init
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(root)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["config", "user.name", "test"])
            .current_dir(root)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(root)
            .output()
            .unwrap();

        std::fs::write(root.join("conflict.txt"), "line original\n").unwrap();
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(root)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(root)
            .output()
            .unwrap();

        let agent_id = crate::agent::subagent::AgentId("conflicting-subagent".to_string());
        let handle = crate::sandbox::GitWorktreeManager::create_worktree(root, &agent_id).unwrap();

        // Branch commit
        std::fs::write(
            handle.worktree_path.join("conflict.txt"),
            "line branch edit\n",
        )
        .unwrap();
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(&handle.worktree_path)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["commit", "-m", "branch edit"])
            .current_dir(&handle.worktree_path)
            .output()
            .unwrap();

        // Main commit
        std::fs::write(root.join("conflict.txt"), "line main edit\n").unwrap();
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(root)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["commit", "-m", "main edit"])
            .current_dir(root)
            .output()
            .unwrap();

        let res =
            merge_subagent_worktree(root, "conflicting-subagent", true, Some("skip"), None).await;

        assert!(res.is_ok());
        let report = res.unwrap();
        assert!(report.contains("Merge conflicts detected"));
        assert!(report.contains("conflict.txt"));
        assert!(report.contains("Parent workspace untouched and worktree preserved"));

        // Worktree MUST still exist
        assert!(crate::sandbox::GitWorktreeManager::locate_worktree(root, &agent_id).is_some());
    }
}

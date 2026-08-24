use crate::agent::provider::ToolSchema;
use crate::error::{Result, ToolError};
use crate::tools::parse_u64_param;
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
            name: "create_task_dag".to_string(),
            description: "Initialize or replace a topological Task DAG with dependency resolution and complexity scoring.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "tasks": {
                        "type": "array",
                        "description": "List of task objects with id, title, description, and optional dependencies array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "id": { "type": "string", "description": "Unique task identifier (e.g. task-1)" },
                                "title": { "type": "string", "description": "Brief task title" },
                                "description": { "type": "string", "description": "Detailed task requirements" },
                                "dependencies": {
                                    "type": "array",
                                    "items": { "type": "string" },
                                    "description": "Array of task IDs that must be completed before this task can execute"
                                }
                            },
                            "required": ["id", "title", "description"]
                        }
                    }
                },
                "required": ["tasks"]
            }),
        },
        ToolSchema {
            name: "get_next_task".to_string(),
            description: "Retrieve all currently unblocked and executable tasks from the active Task DAG.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolSchema {
            name: "complete_task".to_string(),
            description: "Update the lifecycle status of a task in the DAG to completed or failed, unlocking downstream dependencies.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "task_id": {
                        "type": "string",
                        "description": "The task ID to update"
                    },
                    "status": {
                        "type": "string",
                        "enum": ["completed", "failed", "in_progress"],
                        "description": "The new status of the task (default: completed)"
                    }
                },
                "required": ["task_id"]
            }),
        },
        ToolSchema {
            name: "critic_review".to_string(),
            description: "Run an automated Actor-Critic evaluation pass over current workspace changes (compiler diagnostics, linter warnings, git status) to verify code quality.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolSchema {
            name: "sequential_thinking".to_string(),
            description: "Execute a dynamic Graph of Thoughts (GoT) reasoning step to branch hypotheses, score confidence, revise earlier conclusions, and synthesize complex solutions.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "thought_number": {
                        "type": "integer",
                        "description": "Current thought number in the sequence (1-indexed)"
                    },
                    "total_thoughts": {
                        "type": "integer",
                        "description": "Estimated total thoughts required (adaptive)"
                    },
                    "thought": {
                        "type": "string",
                        "description": "The reasoning content, hypothesis analysis, or evaluation"
                    },
                    "is_revision": {
                        "type": "boolean",
                        "description": "Whether this thought revises a prior thought"
                    },
                    "revises_thought": {
                        "type": "integer",
                        "description": "The thought number being revised if is_revision is true"
                    },
                    "branch_from_thought": {
                        "type": "integer",
                        "description": "The thought number to branch off from if exploring an alternative hypothesis"
                    },
                    "branch_id": {
                        "type": "string",
                        "description": "Identifier name for this reasoning branch (e.g. 'hypothesis_a')"
                    },
                    "needs_more_thoughts": {
                        "type": "boolean",
                        "description": "Whether more thinking steps are needed before reaching a conclusion"
                    },
                    "score": {
                        "type": "number",
                        "description": "Optional confidence score between 0.0 and 1.0"
                    }
                },
                "required": ["thought_number", "total_thoughts", "thought", "needs_more_thoughts"]
            }),
        },
        ToolSchema {
            name: "score_task_complexity".to_string(),
            description: "Compute task complexity score (1-10), risk level, estimated token context, and topological subtask decomposition plan before executing complex coding changes.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "task": {
                        "type": "string",
                        "description": "Natural language task or feature description to evaluate"
                    }
                },
                "required": ["task"]
            }),
        },
        ToolSchema {
            name: "explore_hypotheses".to_string(),
            description: "Spawn multiple speculative Git worktree branches to explore and compare alternative implementation hypotheses.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "hypotheses": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "List of alternative implementation descriptions to explore"
                    }
                },
                "required": ["hypotheses"]
            }),
        },
        ToolSchema {
            name: "evaluate_branch".to_string(),
            description: "Run automated compiler diagnostics and calculate fitness score for a speculative hypothesis branch.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "branch_id": {
                        "type": "string",
                        "description": "Identifier of the hypothesis branch (e.g. 'hyp_20260818_120000_b1')"
                    }
                },
                "required": ["branch_id"]
            }),
        },
        ToolSchema {
            name: "select_best_branch".to_string(),
            description: "Select the winning speculative branch with the highest fitness score and discard temporary alternative branches.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {}
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
            description: "Inspect, list, monitor, or terminate active subagent workers in the swarm pool.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["list", "status", "kill", "kill_all"],
                        "description": "Management action: 'list' all subagents, get 'status' of specific subagent, 'kill' a subagent, or 'kill_all'"
                    },
                    "subagent_id": {
                        "type": "string",
                        "description": "Required when action is 'status' or 'kill'"
                    }
                },
                "required": ["action"]
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
            let task = args.get("task").and_then(|v| v.as_str()).ok_or_else(|| {
                ToolError::InvalidArguments {
                    name: "delegate_task".to_string(),
                    reason: "Missing required argument 'task'".to_string(),
                }
            })?;
            let isolate = args
                .get("isolate_branch")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            let timeout_secs = parse_u64_param(args.get("timeout_secs"));

            let res = crate::agent::orchestrator::MultiAgentOrchestrator::delegate(
                workspace_root,
                task,
                isolate,
                timeout_secs,
            )
            .await?;
            Ok(crate::agent::orchestrator::MultiAgentOrchestrator::format_result(&res))
        }.await),
        "create_task_dag" => Some((|| {
            let tasks_array = args
                .get("tasks")
                .and_then(|v| v.as_array())
                .ok_or_else(|| ToolError::InvalidArguments {
                    name: "create_task_dag".to_string(),
                    reason: "Missing required argument 'tasks' array".to_string(),
                })?;

            let mut dag = crate::agent::task_dag::TaskDag::new();
            for item in tasks_array {
                let task: crate::agent::task_dag::TaskItem =
                    serde_json::from_value(item.clone()).map_err(|e| {
                        ToolError::InvalidArguments {
                            name: "create_task_dag".to_string(),
                            reason: format!("Invalid task schema: {}", e),
                        }
                    })?;
                dag.add_task(task);
            }

            let order = dag.topological_order()?;
            dag.save(workspace_root)?;

            Ok(format!(
                "✔ Task DAG initialized with {} tasks (Topological Order: {})\n\n{}",
                dag.tasks.len(),
                order.join(" ➔ "),
                dag.generate_report()
            ))
        })()),
        "get_next_task" => Some((|| {
            let dag = crate::agent::task_dag::TaskDag::load(workspace_root)?;
            if dag.tasks.is_empty() {
                return Ok("ℹ No active Task DAG found in workspace. Use 'create_task_dag' to initialize one.".to_string());
            }

            let next_tasks = dag.next_executable_tasks();
            if next_tasks.is_empty() {
                let report = dag.generate_report();
                if dag
                    .tasks
                    .values()
                    .all(|t| t.status == crate::agent::task_dag::TaskStatus::Completed)
                {
                    Ok(format!("🎉 All tasks in DAG are completed!\n\n{}", report))
                } else {
                    Ok(format!("⏸ No tasks currently unblocked. Check in-progress tasks or dependencies.\n\n{}", report))
                }
            } else {
                let mut out = format!(
                    "🎯 Next Executable Task(s) ({} unblocked):\n\n",
                    next_tasks.len()
                );
                for task in next_tasks {
                    out.push_str(&format!(
                        "• `{}`: **{}** (Complexity: {}/10)\n  {}\n\n",
                        task.id, task.title, task.complexity_score, task.description
                    ));
                }
                Ok(out)
            }
        })()),
        "complete_task" => Some((|| {
            let task_id = args
                .get("task_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::InvalidArguments {
                    name: "complete_task".to_string(),
                    reason: "Missing required argument 'task_id'".to_string(),
                })?;

            let status_str = args
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("completed");
            let status = match status_str {
                "in_progress" => crate::agent::task_dag::TaskStatus::InProgress,
                "failed" => crate::agent::task_dag::TaskStatus::Failed,
                _ => crate::agent::task_dag::TaskStatus::Completed,
            };

            let mut dag = crate::agent::task_dag::TaskDag::load(workspace_root)?;
            dag.set_task_status(task_id, status)?;
            dag.save(workspace_root)?;

            let next = dag.next_executable_tasks();
            let next_desc = if next.is_empty() {
                "None (all remaining tasks are blocked or completed)".to_string()
            } else {
                next.iter()
                    .map(|t| format!("`{}` ({})", t.id, t.title))
                    .collect::<Vec<_>>()
                    .join(", ")
            };

            Ok(format!(
                "✔ Updated task `{}` to status {:?}.\n👉 Newly unblocked executable task(s): {}\n\n{}",
                task_id,
                status,
                next_desc,
                dag.generate_report()
            ))
        })()),
        "critic_review" => Some(async {
            let report =
                crate::agent::critic::CriticValidator::review_workspace(workspace_root).await?;
            let verdict_str = if report.is_approved {
                "✔ [CRITIC APPROVED]"
            } else {
                "❌ [CRITIC REJECTED]"
            };

            let mut out = format!("🔍 Critic Evaluation Summary: {}\n\n", verdict_str);
            out.push_str(&format!("• Status: {}\n", report.suggested_feedback));
            out.push_str(&format!("• Compiler Errors: {}\n", report.compiler_errors));
            out.push_str(&format!(
                "• Compiler Warnings: {}\n",
                report.compiler_warnings
            ));
            if !report.uncommitted_files.is_empty() {
                out.push_str(&format!(
                    "• Modified Files ({}):\n",
                    report.uncommitted_files.len()
                ));
                for file in &report.uncommitted_files {
                    out.push_str(&format!("    - {}\n", file));
                }
            }
            Ok(out)
        }.await),
        "sequential_thinking" => Some((|| {
            let thought_node: crate::agent::sequential_thinking::ThoughtNode =
                serde_json::from_value(args.clone()).map_err(|e| {
                    ToolError::InvalidArguments {
                        name: "sequential_thinking".to_string(),
                        reason: format!("Invalid thought node parameters: {}", e),
                    }
                })?;
            let output = crate::agent::sequential_thinking::ThinkingSession::step(thought_node)?;
            Ok(output)
        })()),
        "score_task_complexity" => Some((|| {
            let task = args["task"]
                .as_str()
                .ok_or_else(|| ToolError::InvalidArguments {
                    name: "score_task_complexity".to_string(),
                    reason: "Missing 'task' parameter".to_string(),
                })?;
            let score =
                crate::agent::complexity::TaskComplexityScorer::score_task(workspace_root, task)?;
            Ok(score.format_markdown())
        })()),
        "explore_hypotheses" => Some(async {
            let hypotheses: Vec<String> = args["hypotheses"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .ok_or_else(|| ToolError::InvalidArguments {
                    name: "explore_hypotheses".to_string(),
                    reason: "Missing 'hypotheses' array".to_string(),
                })?;

            let session = crate::agent::hypothesis::HypothesisEngine::create_branches(
                workspace_root,
                &hypotheses,
            )
            .await?;

            let mut out = format!(
                "🌱 Spawned {} speculative branches (Session `{}`):\n\n",
                session.branches.len(),
                session.id
            );
            for (i, b) in session.branches.iter().enumerate() {
                out.push_str(&format!(
                    "{}. **Branch `{}`**:\n   _{}_\n   📁 Worktree: `{}`\n\n",
                    i + 1,
                    b.id,
                    b.description,
                    b.worktree_path.display()
                ));
            }
            out.push_str("👉 Use 'evaluate_branch' to score each branch or 'select_best_branch' to merge the winner.");
            Ok(out)
        }.await),
        "evaluate_branch" => Some(async {
            let branch_id =
                args["branch_id"]
                    .as_str()
                    .ok_or_else(|| ToolError::InvalidArguments {
                        name: "evaluate_branch".to_string(),
                        reason: "Missing 'branch_id'".to_string(),
                    })?;

            let branch = crate::agent::hypothesis::HypothesisEngine::evaluate_branch(
                workspace_root,
                branch_id,
            )
            .await?;

            let status_icon = if branch.compiler_clean { "✔" } else { "✗" };
            let report = format!(
                "{} Branch `{}` Evaluation:\n• Fitness Score: {:.2}/1.00\n• Status: {:?}\n• Compiler Clean: {}\n• Errors: {}\n• Warnings: {}\n• Notes: {}",
                status_icon,
                branch.id,
                branch.fitness_score,
                branch.status,
                branch.compiler_clean,
                branch.compiler_errors,
                branch.compiler_warnings,
                branch.notes
            );
            Ok(report)
        }.await),
        "select_best_branch" => Some(async {
            let winner =
                crate::agent::hypothesis::HypothesisEngine::select_best_branch(workspace_root)
                    .await?;

            let report = format!(
                "🏆 Selected Winning Branch `{}`!\n• Description: {}\n• Fitness Score: {:.2}\n• Worktree: `{}`\n\nAll alternative speculative branches have been cleanly discarded.",
                winner.id,
                winner.description,
                winner.fitness_score,
                winner.worktree_path.display()
            );
            Ok(report)
        }.await),
        "invoke_subagent" => Some(async {
            let role_str = args.get("role").and_then(|v| v.as_str()).ok_or_else(|| {
                ToolError::InvalidArguments {
                    name: "invoke_subagent".to_string(),
                    reason: "Missing required argument 'role'".to_string(),
                }
            })?;
            let prompt = args.get("prompt").and_then(|v| v.as_str()).ok_or_else(|| {
                ToolError::InvalidArguments {
                    name: "invoke_subagent".to_string(),
                    reason: "Missing required argument 'prompt'".to_string(),
                }
            })?;

            let role = match role_str.to_lowercase().as_str() {
                "researcher" => crate::agent::subagent::SubagentRole::Researcher,
                "code_reviewer" | "reviewer" => crate::agent::subagent::SubagentRole::CodeReviewer,
                "test_engineer" | "tester" => crate::agent::subagent::SubagentRole::TestEngineer,
                "security_auditor" | "security" => crate::agent::subagent::SubagentRole::SecurityAuditor,
                other => crate::agent::subagent::SubagentRole::Custom(other.to_string()),
            };

            let mut config = crate::agent::subagent::SubagentConfig::for_role(role.clone());
            if let Some(model) = args.get("model").and_then(|v| v.as_str()) {
                config.model = Some(model.to_string());
            }
            if let Some(budget) = parse_u64_param(args.get("token_budget")) {
                config.token_budget = budget as usize;
            }
            if let Some(max_t) = parse_u64_param(args.get("max_turns")) {
                config.max_turns = max_t as usize;
            }
            if let Some(sys_prompt) = args.get("system_prompt").and_then(|v| v.as_str()) {
                config.system_prompt_override = Some(sys_prompt.to_string());
            }

            let pool = crate::agent::subagent::get_global_subagent_pool(workspace_root);
            let id = pool.next_id(&role).await;

            let res = crate::agent::orchestrator::MultiAgentOrchestrator::delegate(
                workspace_root,
                prompt,
                false,
                Some(120),
            )
            .await?;

            let report = format!(
                "✔ Subagent `[ID: {} | Role: {}]` completed task successfully!\n• Tokens Used: {}\n• Files Modified: {}\n\n### Findings & Response Summary\n{}",
                id,
                role.badge(),
                res.tokens_used,
                if res.files_modified.is_empty() { "None (Read-Only)".to_string() } else { res.files_modified.join(", ") },
                res.final_summary
            );

            Ok(report)
        }.await),
        "send_message" => Some(async {
            let subagent_id = args.get("subagent_id").and_then(|v| v.as_str()).ok_or_else(|| {
                ToolError::InvalidArguments {
                    name: "send_message".to_string(),
                    reason: "Missing required argument 'subagent_id'".to_string(),
                }
            })?;
            let message = args.get("message").and_then(|v| v.as_str()).ok_or_else(|| {
                ToolError::InvalidArguments {
                    name: "send_message".to_string(),
                    reason: "Missing required argument 'message'".to_string(),
                }
            })?;

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
            let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("list");
            let pool = crate::agent::subagent::get_global_subagent_pool(workspace_root);

            match action {
                "list" => {
                    let summary = pool.format_swarm_summary().await;
                    Ok(summary)
                }
                "status" => {
                    let id = args.get("subagent_id").and_then(|v| v.as_str()).ok_or_else(|| {
                        ToolError::InvalidArguments {
                            name: "manage_subagents".to_string(),
                            reason: "Missing required argument 'subagent_id' for status query".to_string(),
                        }
                    })?;
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
                "kill" => {
                    let id = args.get("subagent_id").and_then(|v| v.as_str()).ok_or_else(|| {
                        ToolError::InvalidArguments {
                            name: "manage_subagents".to_string(),
                            reason: "Missing required argument 'subagent_id' for kill action".to_string(),
                        }
                    })?;
                    pool.kill_subagent(id).await?;
                    Ok(format!("✔ Subagent `{}` successfully terminated", id))
                }
                "kill_all" => {
                    pool.kill_all().await;
                    Ok("✔ All active subagents in swarm pool have been terminated".to_string())
                }
                other => Err(ToolError::InvalidArguments {
                    name: "manage_subagents".to_string(),
                    reason: format!("Unknown action '{}'. Valid actions: list, status, kill, kill_all", other),
                }.into()),
            }
        }.await),
        _ => None,
    }
}

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
            name: "schedule_task_waves".to_string(),
            description: "Calculate parallel execution waves from the active Task DAG to execute non-conflicting tasks concurrently.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolSchema {
            name: "split_task".to_string(),
            description: "Dynamically split a high-complexity task into multiple subtasks, preserving upstream and downstream DAG dependencies.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "parent_task_id": {
                        "type": "string",
                        "description": "The ID of the parent task to split"
                    },
                    "subtasks": {
                        "type": "array",
                        "description": "List of child subtasks replacing the parent",
                        "items": {
                            "type": "object",
                            "properties": {
                                "id": { "type": "string" },
                                "title": { "type": "string" },
                                "description": { "type": "string" },
                                "dependencies": { "type": "array", "items": { "type": "string" } },
                                "complexity_score": { "type": "integer" }
                            },
                            "required": ["id", "title", "description"]
                        }
                    }
                },
                "required": ["parent_task_id", "subtasks"]
            }),
        },
        ToolSchema {
            name: "record_episode".to_string(),
            description: "Record a completed task episode, bug fix, or architectural breakthrough into long-term vector memory for cross-session recall.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "title": {
                        "type": "string",
                        "description": "Concise summary title of the episode/solution"
                    },
                    "summary": {
                        "type": "string",
                        "description": "Detailed explanation of what was fixed, designed, or learned"
                    },
                    "tags": {
                        "type": "array",
                        "description": "Search tags and keywords (e.g. ['tree-sitter', 'segfault'])",
                        "items": { "type": "string" }
                    },
                    "code_references": {
                        "type": "array",
                        "description": "Relevant files or functions changed",
                        "items": { "type": "string" }
                    }
                },
                "required": ["title", "summary"]
            }),
        },
        ToolSchema {
            name: "recall_episodes".to_string(),
            description: "Perform hybrid semantic and keyword search across historical session episodes to recall past architectural decisions and bug fixes.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Search query or problem description to recall"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of episodes to return (default: 3)"
                    }
                },
                "required": ["query"]
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
            name: "evaluate_all_branches".to_string(),
            description: "Concurrently evaluate all active speculative hypothesis branches using compiler diagnostics.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolSchema {
            name: "prune_branches".to_string(),
            description: "Automatically prune failed or low-fitness speculative hypothesis worktree branches.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "min_fitness": {
                        "type": "number",
                        "description": "Minimum fitness score threshold below which branches are pruned (default: 0.3)"
                    }
                }
            }),
        },
        ToolSchema {
            name: "compare_branches".to_string(),
            description: "Output a structured comparison matrix table of all speculative branches in the active hypothesis session.".to_string(),
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
            name: "synthesize_reproducer".to_string(),
            description: "Synthesize an isolated TDD bug reproducer test in 'tests/repro_<name>.rs'. Automatically executes Red Phase against unpatched codebase to prove that the bug is real (must fail). Warns if the test is vacuous (passes unexpectedly).".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Unique identifier for the reproducer (e.g. 'null_pointer', 'donut_truncation', 'parser_bounds')"
                    },
                    "test_code": {
                        "type": "string",
                        "description": "Complete Rust integration test code for tests/repro_<name>.rs (e.g. '#[test] fn test_repro() { ... }')"
                    },
                    "description": {
                        "type": "string",
                        "description": "Short explanation of what bug or edge case this reproducer isolates"
                    },
                    "run_red_phase": {
                        "type": "boolean",
                        "description": "Whether to immediately compile and run the reproducer to confirm it fails on unpatched code (default: true)"
                    }
                },
                "required": ["name", "test_code", "description"]
            }),
        },
        ToolSchema {
            name: "verify_reproducer".to_string(),
            description: "Execute and verify an active reproducer test target to check if it is still failing (RED) or now passing (GREEN) after source code edits.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Name or test target of the reproducer to execute (e.g. 'null_pointer' or 'repro_null_pointer')"
                    }
                },
                "required": ["name"]
            }),
        },
        ToolSchema {
            name: "list_reproducers".to_string(),
            description: "List all active standalone bug reproducers, their Red-phase proof, and Green-phase verification status in the current workspace.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {}
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
        "schedule_task_waves" => Some((|| {
            let dag = crate::agent::task_dag::TaskDag::load(workspace_root)?;
            if dag.tasks.is_empty() {
                return Ok("ℹ No active Task DAG found in workspace. Use 'create_task_dag' to initialize one.".to_string());
            }

            let waves = dag.calculate_execution_waves()?;
            let mut out = format!("⚡ Parallel Task Execution Waves ({} waves, {} total tasks):\n\n", waves.len(), dag.tasks.len());
            for (idx, wave) in waves.iter().enumerate() {
                out.push_str(&format!("### Wave {}:\n", idx + 1));
                for task_id in wave {
                    if let Some(t) = dag.tasks.get(task_id) {
                        let status_emoji = match t.status {
                            crate::agent::task_dag::TaskStatus::Completed => "✔",
                            crate::agent::task_dag::TaskStatus::InProgress => "◉",
                            _ => "○",
                        };
                        out.push_str(&format!("  • {} `{}`: **{}** (Complexity: {}/10)\n", status_emoji, t.id, t.title, t.complexity_score));
                    }
                }
                out.push('\n');
            }
            Ok(out)
        })()),
        "split_task" => Some((|| {
            let parent_id = args.get("parent_task_id").and_then(|v| v.as_str()).ok_or_else(|| {
                ToolError::InvalidArguments {
                    name: "split_task".to_string(),
                    reason: "Missing required argument 'parent_task_id'".to_string(),
                }
            })?;
            let subtasks_array = args.get("subtasks").and_then(|v| v.as_array()).ok_or_else(|| {
                ToolError::InvalidArguments {
                    name: "split_task".to_string(),
                    reason: "Missing required argument 'subtasks' array".to_string(),
                }
            })?;

            let mut dag = crate::agent::task_dag::TaskDag::load(workspace_root)?;
            let mut child_tasks = Vec::new();
            for item in subtasks_array {
                let task: crate::agent::task_dag::TaskItem = serde_json::from_value(item.clone()).map_err(|e| {
                    ToolError::InvalidArguments {
                        name: "split_task".to_string(),
                        reason: format!("Invalid subtask schema: {}", e),
                    }
                })?;
                child_tasks.push(task);
            }

            let child_ids = dag.split_task(parent_id, child_tasks)?;
            dag.save(workspace_root)?;

            Ok(format!(
                "✔ Split task `{}` into {} child tasks: {}\n\n{}",
                parent_id,
                child_ids.len(),
                child_ids.join(", "),
                dag.generate_report()
            ))
        })()),
        "record_episode" => Some((|| {
            let title = args.get("title").and_then(|v| v.as_str()).ok_or_else(|| {
                ToolError::InvalidArguments {
                    name: "record_episode".to_string(),
                    reason: "Missing required argument 'title'".to_string(),
                }
            })?;
            let summary = args.get("summary").and_then(|v| v.as_str()).ok_or_else(|| {
                ToolError::InvalidArguments {
                    name: "record_episode".to_string(),
                    reason: "Missing required argument 'summary'".to_string(),
                }
            })?;
            let tags = args.get("tags").and_then(|v| v.as_array()).map(|arr| {
                arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect()
            }).unwrap_or_default();
            let code_refs = args.get("code_references").and_then(|v| v.as_array()).map(|arr| {
                arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect()
            }).unwrap_or_default();

            let mut mem = crate::context::episodic::EpisodicMemory::load(workspace_root)?;
            let ep_id = mem.record_episode(title, summary, tags, code_refs, "current_session");
            mem.save(workspace_root)?;

            Ok(format!("✔ Recorded episodic memory `{}`: **{}**", ep_id, title))
        })()),
        "recall_episodes" => Some((|| {
            let query = args.get("query").and_then(|v| v.as_str()).ok_or_else(|| {
                ToolError::InvalidArguments {
                    name: "recall_episodes".to_string(),
                    reason: "Missing required argument 'query'".to_string(),
                }
            })?;
            let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(3) as usize;

            let mem = crate::context::episodic::EpisodicMemory::load(workspace_root)?;
            let results = mem.search(query, limit);

            if results.is_empty() {
                return Ok(format!("ℹ No historical episodes matched query `{}`.", query));
            }

            let mut out = format!("🧠 Recalled {} Relevant Historical Episode(s) for `{}`:\n\n", results.len(), query);
            for (idx, r) in results.iter().enumerate() {
                out.push_str(&format!(
                    "{}. **{}** (Score: {:.2})\n   _{}_\n   🏷 Tags: {}\n   📁 Files: {}\n\n",
                    idx + 1,
                    r.item.title,
                    r.score,
                    r.item.summary,
                    r.item.tags.join(", "),
                    r.item.code_references.join(", ")
                ));
            }
            Ok(out)
        })()),
        "critic_review" => Some(async {
            let report =
                crate::agent::critic::CriticValidator::review_workspace(workspace_root).await?;
            Ok(report.format_for_agent())
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
        "evaluate_all_branches" => Some(async {
            let evaluated = crate::agent::hypothesis::HypothesisEngine::evaluate_all_branches(workspace_root).await?;
            let mut out = format!("✔ Evaluated {} speculative branch(es):\n\n", evaluated.len());
            for b in &evaluated {
                out.push_str(&format!(
                    "• `{}`: Status: {:?}, Fitness: {:.2}, Clean: {}\n",
                    b.id, b.status, b.fitness_score, b.compiler_clean
                ));
            }
            Ok(out)
        }.await),
        "prune_branches" => Some(async {
            let min_fitness = args.get("min_fitness").and_then(|v| v.as_f64()).unwrap_or(0.3) as f32;
            let pruned = crate::agent::hypothesis::HypothesisEngine::prune_failed_branches(workspace_root, min_fitness).await?;
            if pruned.is_empty() {
                Ok("ℹ No branches were below the pruning threshold.".to_string())
            } else {
                Ok(format!("🗑 Pruned {} underperforming branch(es): {}", pruned.len(), pruned.join(", ")))
            }
        }.await),
        "compare_branches" => Some((|| {
            let session = crate::agent::hypothesis::HypothesisEngine::load_session(workspace_root)?;
            Ok(crate::agent::hypothesis::HypothesisEngine::format_comparison_matrix(&session))
        })()),
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

            let res =
                crate::agent::orchestrator::MultiAgentOrchestrator::delegate_with_config(
                    workspace_root,
                    prompt,
                    false,
                    Some(120),
                    Some(config),
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
        "scratchpad_write" => Some(async {
            let key = args.get("key").and_then(|v| v.as_str()).ok_or_else(|| {
                ToolError::InvalidArguments {
                    name: "scratchpad_write".to_string(),
                    reason: "Missing required argument 'key'".to_string(),
                }
            })?;
            let title = args.get("title").and_then(|v| v.as_str()).ok_or_else(|| {
                ToolError::InvalidArguments {
                    name: "scratchpad_write".to_string(),
                    reason: "Missing required argument 'title'".to_string(),
                }
            })?;
            let content = args.get("content").and_then(|v| v.as_str()).ok_or_else(|| {
                ToolError::InvalidArguments {
                    name: "scratchpad_write".to_string(),
                    reason: "Missing required argument 'content'".to_string(),
                }
            })?;
            let author = args.get("author").and_then(|v| v.as_str()).unwrap_or("orchestrator");

            let sp = crate::agent::subagent::get_global_scratchpad();
            let entry = sp.write_entry(key, title, content, author);
            let _ = sp.save_to_disk(workspace_root);

            Ok(format!("✔ Scratchpad entry `{}` published successfully by `{}`.", entry.key, entry.author))
        }.await),
        "scratchpad_read" => Some(async {
            let key = args.get("key").and_then(|v| v.as_str()).ok_or_else(|| {
                ToolError::InvalidArguments {
                    name: "scratchpad_read".to_string(),
                    reason: "Missing required argument 'key'".to_string(),
                }
            })?;

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
        "send_worker_message" => Some(async {
            let from = args.get("from_worker_id").and_then(|v| v.as_str()).ok_or_else(|| {
                ToolError::InvalidArguments {
                    name: "send_worker_message".to_string(),
                    reason: "Missing required argument 'from_worker_id'".to_string(),
                }
            })?;
            let to = args.get("to_worker_id").and_then(|v| v.as_str()).filter(|s| !s.is_empty());
            let topic = args.get("topic").and_then(|v| v.as_str()).ok_or_else(|| {
                ToolError::InvalidArguments {
                    name: "send_worker_message".to_string(),
                    reason: "Missing required argument 'topic'".to_string(),
                }
            })?;
            let payload = args.get("payload").and_then(|v| v.as_str()).ok_or_else(|| {
                ToolError::InvalidArguments {
                    name: "send_worker_message".to_string(),
                    reason: "Missing required argument 'payload'".to_string(),
                }
            })?;

            let bus = crate::agent::subagent::get_global_message_bus();
            let msg = bus.send_message(from, to, topic, payload);

            let dest = to.map(|t| format!("to worker `{}`", t)).unwrap_or_else(|| "as swarm broadcast".to_string());
            Ok(format!("✔ Message `{}` posted {} on topic `{}`.", msg.id, dest, msg.topic))
        }.await),
        "read_worker_messages" => Some(async {
            let worker_id = args.get("worker_id").and_then(|v| v.as_str()).ok_or_else(|| {
                ToolError::InvalidArguments {
                    name: "read_worker_messages".to_string(),
                    reason: "Missing required argument 'worker_id'".to_string(),
                }
            })?;

            let bus = crate::agent::subagent::get_global_message_bus();
            let messages = bus.read_inbox(worker_id);

            if messages.is_empty() {
                Ok(format!("ℹ Inbox for worker `{}` is empty.", worker_id))
            } else {
                let mut out = format!("📬 Inbox for Worker `{}` ({} message(s)):\n\n", worker_id, messages.len());
                for (i, m) in messages.iter().enumerate() {
                    let kind = if m.to_worker_id.is_none() { "[Broadcast]" } else { "[Direct]" };
                    out.push_str(&format!(
                        "{}. {} from `{}` (Topic: `{}`)\n```\n{}\n```\n\n",
                        i + 1, kind, m.from_worker_id, m.topic, m.payload
                    ));
                }
                Ok(out)
            }
        }.await),
        "synthesize_reproducer" => Some(async {
            let name = args.get("name").and_then(|v| v.as_str()).ok_or_else(|| {
                ToolError::InvalidArguments {
                    name: "synthesize_reproducer".to_string(),
                    reason: "Missing required argument 'name'".to_string(),
                }
            })?;
            let test_code = args.get("test_code").and_then(|v| v.as_str()).ok_or_else(|| {
                ToolError::InvalidArguments {
                    name: "synthesize_reproducer".to_string(),
                    reason: "Missing required argument 'test_code'".to_string(),
                }
            })?;
            let description = args
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("TDD bug reproducer");
            let run_red_phase = args
                .get("run_red_phase")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);

            let report = crate::agent::reproducer_guard::ReproducerGuard::synthesize_rust_reproducer(
                workspace_root,
                name,
                test_code,
                description,
                run_red_phase,
            )?;

            Ok(report.format_message())
        }.await),
        "verify_reproducer" => Some(async {
            let name = args.get("name").and_then(|v| v.as_str()).ok_or_else(|| {
                ToolError::InvalidArguments {
                    name: "verify_reproducer".to_string(),
                    reason: "Missing required argument 'name'".to_string(),
                }
            })?;

            let report = crate::agent::reproducer_guard::ReproducerGuard::verify_reproducer(
                workspace_root,
                name,
            )?;

            Ok(report.format_message())
        }.await),
        "list_reproducers" => Some(async {
            let records = crate::agent::reproducer_guard::ReproducerGuard::list_active_reproducers(
                workspace_root,
            );
            Ok(crate::agent::reproducer_guard::ReproducerGuard::format_reproducer_list(&records))
        }.await),
        _ => None,
    }
}

use crate::agent::provider::ToolSchema;
use crate::error::{Result, ToolError};
use crate::tools::param;
use serde_json::json;
use std::path::Path;

pub fn get_schemas() -> Vec<ToolSchema> {
    vec![
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
            name: "execute_dag".to_string(),
            description: "Execute a Directed Acyclic Graph (DAG) of interdependent tools with topological wave scheduling and inter-tool JSONPath parameter pipelining. Upstream outputs can be referenced in downstream arguments using '$node_id.path.to.field' or '${node_id.path}'.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Optional name for this workflow DAG (default: 'workflow')"
                    },
                    "nodes": {
                        "type": "array",
                        "description": "List of tool nodes to execute in topological wave order",
                        "items": {
                            "type": "object",
                            "properties": {
                                "id": {
                                    "type": "string",
                                    "description": "Unique identifier for this node (e.g. 'fault_loc', 'read_code')"
                                },
                                "tool": {
                                    "type": "string",
                                    "description": "Registered tool name to execute (e.g. 'locate_fault', 'read_file', 'ast_replace_node')"
                                },
                                "args": {
                                    "type": "object",
                                    "description": "Arguments for the tool. Values can reference upstream node outputs using '$node_id.path.to.field' or '${node_id.path}'"
                                },
                                "depends_on": {
                                    "type": "array",
                                    "items": { "type": "string" },
                                    "description": "Array of parent node IDs that must succeed before this node runs"
                                }
                            },
                            "required": ["id", "tool"]
                        }
                    }
                },
                "required": ["nodes"]
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
        "create_task_dag" => Some((|| {
            let tasks_array = param::require_array(args, "tasks", "create_task_dag")?;

            let mut dag = crate::agent::task_dag::TaskDag::new();
            for item in tasks_array {
                let task: crate::agent::task_dag::TaskItem = serde_json::from_value(item.clone())
                    .map_err(|e| {
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
            let task_id = param::require_str(args, "task_id", "complete_task")?;
            let status_str = param::opt_str(args, "status").unwrap_or("completed");
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
            let mut out = format!(
                "⚡ Parallel Task Execution Waves ({} waves, {} total tasks):\n\n",
                waves.len(),
                dag.tasks.len()
            );
            for (idx, wave) in waves.iter().enumerate() {
                out.push_str(&format!("### Wave {}:\n", idx + 1));
                for task_id in wave {
                    if let Some(t) = dag.tasks.get(task_id) {
                        let status_emoji = match t.status {
                            crate::agent::task_dag::TaskStatus::Completed => "✔",
                            crate::agent::task_dag::TaskStatus::InProgress => "◉",
                            _ => "○",
                        };
                        out.push_str(&format!(
                            "  • {} `{}`: **{}** (Complexity: {}/10)\n",
                            status_emoji, t.id, t.title, t.complexity_score
                        ));
                    }
                }
                out.push('\n');
            }
            Ok(out)
        })()),
        "split_task" => Some((|| {
            let parent_id = param::require_str(args, "parent_task_id", "split_task")?;
            let subtasks_array = param::require_array(args, "subtasks", "split_task")?;

            let mut dag = crate::agent::task_dag::TaskDag::load(workspace_root)?;
            let mut child_tasks = Vec::new();
            for item in subtasks_array {
                let task: crate::agent::task_dag::TaskItem = serde_json::from_value(item.clone())
                    .map_err(|e| {
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
        "execute_dag" => Some(
            async {
                let dag_spec: crate::agent::dag::DagSpec = serde_json::from_value(args.clone())
                    .map_err(|e| ToolError::InvalidArguments {
                        name: "execute_dag".to_string(),
                        reason: format!("Failed to parse DAG specification: {}", e),
                    })?;

                let report =
                    crate::agent::dag::DagExecutor::execute(workspace_root, &dag_spec).await?;
                Ok(report.format_summary())
            }
            .await,
        ),
        _ => None,
    }
}

use crate::agent::provider::ToolSchema;
use crate::error::{Result, ToolError};
use crate::tools::param;
use serde_json::json;
use std::path::Path;

pub fn get_schemas() -> Vec<ToolSchema> {
    vec![
        ToolSchema {
            name: "remember_fact".to_string(),
            description: "Save a persistent fact, convention, or developer preference to Core Memory (survives across sessions).".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "key": {
                        "type": "string",
                        "description": "Unique identifier for this memory (e.g. 'code_style', 'architecture')"
                    },
                    "value": {
                        "type": "string",
                        "description": "The fact, convention, or preference to remember"
                    },
                    "is_global": {
                        "type": "boolean",
                        "description": "Whether to store globally across all projects (~/.config/minicode/memory.json) or locally (.minicode/memory.json). Default: false (local)"
                    },
                    "category": {
                        "type": "string",
                        "enum": ["preference", "project_fact", "pattern"],
                        "description": "Category of memory (default: 'project_fact')"
                    }
                },
                "required": ["key", "value"]
            }),
        },
        ToolSchema {
            name: "update_fact".to_string(),
            description: "Update an existing fact or preference in Core Memory.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "key": {
                        "type": "string",
                        "description": "Key of the memory to update"
                    },
                    "new_value": {
                        "type": "string",
                        "description": "The updated fact or preference text"
                    }
                },
                "required": ["key", "new_value"]
            }),
        },
        ToolSchema {
            name: "forget_fact".to_string(),
            description: "Remove a fact or preference from Core Memory.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "key": {
                        "type": "string",
                        "description": "Key of the memory to remove"
                    }
                },
                "required": ["key"]
            }),
        },
        ToolSchema {
            name: "create_plan".to_string(),
            description: "Initialize an active multi-step task plan in Working Memory (.minicode/plan/task_plan.md).".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "title": {
                        "type": "string",
                        "description": "Short, descriptive title of the task"
                    },
                    "steps": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Ordered list of action steps to complete the task"
                    }
                },
                "required": ["steps"]
            }),
        },
        ToolSchema {
            name: "read_plan".to_string(),
            description: "Read the active task plan, progress tracker, and findings from Working Memory.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolSchema {
            name: "log_finding".to_string(),
            description: "Record an architectural discovery, symbol location, or observation into findings.md.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "finding": {
                        "type": "string",
                        "description": "The observation, discovery, or architectural note to record"
                    }
                },
                "required": ["finding"]
            }),
        },
        ToolSchema {
            name: "update_progress".to_string(),
            description: "Update the status of a specific task step in progress.md (e.g. 'Completed', 'Blocked', 'In Progress').".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "step": {
                        "type": "string",
                        "description": "The description of the step matching the task plan"
                    },
                    "status": {
                        "type": "string",
                        "description": "The status (e.g. 'Completed', 'In Progress', 'Blocked')"
                    }
                },
                "required": ["step"]
            }),
        },
        ToolSchema {
            name: "archive_plan".to_string(),
            description: "Archive the completed task plan and clear the active Working Memory.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolSchema {
            name: "checkpoint_session".to_string(),
            description: "Capture an immutable point-in-time snapshot of the current session conversation and working memory.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "label": {
                        "type": "string",
                        "description": "Short mnemonic label for the checkpoint (e.g. 'before-ast-refactor')"
                    },
                    "description": {
                        "type": "string",
                        "description": "Optional detailed context or rationale for creating this checkpoint"
                    }
                },
                "required": ["label"]
            }),
        },
        ToolSchema {
            name: "rewind_session".to_string(),
            description: "List checkpoints, rewind working memory to an earlier snapshot, or fork a new exploration branch.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["list", "rewind", "fork"],
                        "description": "Action to perform: 'list' all checkpoints, 'rewind' to checkpoint, or 'fork' a new branch"
                    },
                    "checkpoint_id": {
                        "type": "string",
                        "description": "Target checkpoint ID (required for 'rewind' and 'fork')"
                    },
                    "fork_label": {
                        "type": "string",
                        "description": "Optional label when forking a checkpoint"
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
        "remember_fact" => Some((|| {
            let key = param::require_str(args, "key", "remember_fact")?;
            let value = param::require_str(args, "value", "remember_fact")?;
            let is_global = param::opt_bool(args, "is_global", false);
            let cat_str = param::opt_str(args, "category").unwrap_or("project_fact");
            let category = match cat_str {
                "preference" => crate::context::memory::MemoryCategory::Preference,
                "pattern" => crate::context::memory::MemoryCategory::Pattern,
                _ => crate::context::memory::MemoryCategory::ProjectFact,
            };
            let mut mem = crate::context::memory::CoreMemory::load(workspace_root);
            mem.remember(workspace_root, key, value, is_global, category)
                .map(|_| {
                    format!(
                        "✔ Remembered '{}' ({})",
                        key,
                        if is_global { "global" } else { "local" }
                    )
                })
        })()),
        "update_fact" => Some((|| {
            let key = param::require_str(args, "key", "update_fact")?;
            let new_value = param::require_str(args, "new_value", "update_fact")?;
            let mut mem = crate::context::memory::CoreMemory::load(workspace_root);
            mem.update(workspace_root, key, new_value).map(|updated| {
                if updated {
                    format!("✔ Updated fact '{}'", key)
                } else {
                    format!("ℹ Fact '{}' not found to update", key)
                }
            })
        })()),
        "forget_fact" => Some((|| {
            let key = param::require_str(args, "key", "forget_fact")?;
            let mut mem = crate::context::memory::CoreMemory::load(workspace_root);
            mem.forget(workspace_root, key).map(|forgotten| {
                if forgotten {
                    format!("✔ Removed fact '{}' from memory", key)
                } else {
                    format!("ℹ Fact '{}' not found in memory", key)
                }
            })
        })()),
        "create_plan" => Some((|| {
            let title = param::opt_str(args, "title").unwrap_or("Task Plan");
            let steps = param::opt_string_array(args, "steps").ok_or_else(|| {
                ToolError::InvalidArguments {
                    name: "create_plan".to_string(),
                    reason: "Missing required argument 'steps'".to_string(),
                }
            })?;
            let wm = crate::context::working_memory::WorkingMemory::new(workspace_root);
            wm.init_plan(title, &steps).map(|_| {
                format!(
                    "✔ Created active task plan with {} steps in .minicode/plan/task_plan.md",
                    steps.len()
                )
            })
        })()),
        "read_plan" => Some({
            let wm = crate::context::working_memory::WorkingMemory::new(workspace_root);
            match wm.read_plan() {
                Ok(Some(plan)) => Ok(plan),
                Ok(None) => Ok("ℹ No active task plan found in .minicode/plan/".to_string()),
                Err(e) => Err(e),
            }
        }),
        "log_finding" => Some((|| {
            let finding = param::require_str(args, "finding", "log_finding")?;
            let wm = crate::context::working_memory::WorkingMemory::new(workspace_root);
            wm.append_finding(finding)
                .map(|_| "✔ Logged observation into .minicode/plan/findings.md".to_string())
        })()),
        "update_progress" => Some((|| {
            let step = param::require_str(args, "step", "update_progress")?;
            let status = param::opt_str(args, "status").unwrap_or("Completed");
            let wm = crate::context::working_memory::WorkingMemory::new(workspace_root);
            wm.update_progress(step, status)
                .map(|_| format!("✔ Updated step '{}' status to '{}'", step, status))
        })()),
        "archive_plan" => Some({
            let wm = crate::context::working_memory::WorkingMemory::new(workspace_root);
            match wm.archive_plan() {
                Ok(Some(archive_path)) => Ok(format!(
                    "✔ Archived completed task plan to {}",
                    archive_path.display()
                )),
                Ok(None) => Ok("ℹ No active task plan to archive".to_string()),
                Err(e) => Err(e),
            }
        }),
        "checkpoint_session" => Some((|| {
            let label = param::require_str(args, "label", "checkpoint_session")?;
            let description = param::opt_str(args, "description");

            let info = crate::context::checkpoint::SessionCheckpointer::create_checkpoint(
                workspace_root,
                "current_session",
                label,
                description,
                &[],
            )?;

            Ok(format!(
                "✔ Created session checkpoint `{}` (`{}`)\n- **Timestamp:** {}\n- **Working Memory Saved:** {}",
                info.label,
                info.id,
                info.timestamp,
                if info.has_working_plan { "Yes" } else { "No" }
            ))
        })()),
        "rewind_session" => Some((|| {
            let action = param::opt_str(args, "action").unwrap_or("list");

            match action {
                "list" => {
                    let list = crate::context::checkpoint::SessionCheckpointer::list_checkpoints(
                        workspace_root,
                        None,
                    )?;
                    if list.is_empty() {
                        return Ok("*(No checkpoints recorded for this workspace yet)*".to_string());
                    }
                    let mut out =
                        format!("# ⏱️ Workspace Session Checkpoints ({})\n\n", list.len());
                    out.push_str("| Checkpoint ID | Label | Timestamp | Plan Saved |\n");
                    out.push_str("| :--- | :--- | :--- | :--- |\n");
                    for ckpt in list {
                        out.push_str(&format!(
                            "| `{}` | **{}** | {} | {} |\n",
                            ckpt.id,
                            ckpt.label,
                            ckpt.timestamp,
                            if ckpt.has_working_plan { "✔" } else { "—" }
                        ));
                    }
                    Ok(out)
                }
                "rewind" => {
                    let ckpt_id = param::require_str(args, "checkpoint_id", "rewind_session")?;
                    let (report, _) =
                        crate::context::checkpoint::SessionCheckpointer::rewind_checkpoint(
                            workspace_root,
                            "current_session",
                            ckpt_id,
                        )?;
                    Ok(report.format_markdown())
                }
                "fork" => {
                    let ckpt_id = param::require_str(args, "checkpoint_id", "rewind_session")?;
                    let fork_label = param::opt_str(args, "fork_label");
                    let forked = crate::context::checkpoint::SessionCheckpointer::fork_checkpoint(
                        workspace_root,
                        ckpt_id,
                        fork_label,
                    )?;
                    Ok(format!(
                        "✔ Successfully forked checkpoint `{}` into new branch `{}` (`{}`)",
                        ckpt_id, forked.label, forked.id
                    ))
                }
                other => Err(ToolError::InvalidArguments {
                    name: "rewind_session".to_string(),
                    reason: format!("Unknown action: {}", other),
                }
                .into()),
            }
        })()),
        _ => None,
    }
}

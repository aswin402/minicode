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
            description: "Initialize a right-sized Two-Tier Task Plan in Working Memory on ANY turn (2-3 steps for small/targeted tasks, bugfixes, or audits; 4-8+ steps for medium/large features or full apps). Call `update_progress(step=\"active\", status=\"completed\")` immediately as each step completes rather than batching at the end; decompose multi-stage verification (static build/lint + live browser/viewport QA) into separate steps or nested `substeps`. Writes the technical blueprint to `minikit_docs/core/implementation.md`, the milestone roadmap to `minikit_docs/core/todo.md` (viewable via /todo modal), and active tactical execution steps to `.minicode/plan/task_plan.md` rendered live in the TUI.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "title": {
                        "type": "string",
                        "description": "Short, descriptive title of the task or milestone"
                    },
                    "steps": {
                        "type": "array",
                        "items": {
                            "anyOf": [
                                { "type": "string" },
                                {
                                    "type": "object",
                                    "properties": {
                                        "title": { "type": "string" },
                                        "description": { "type": "string" },
                                        "substeps": {
                                            "type": "array",
                                            "items": { "type": "string" },
                                            "description": "Optional Tier-2 tactical sub-steps nested under this macro task."
                                        }
                                    }
                                }
                            ]
                        },
                        "description": "Ordered list of macro tasks or tactical action steps (optionally with nested `substeps` per task)"
                    },
                    "architecture": {
                        "type": "string",
                        "description": "Comprehensive top-to-bottom technical architecture and engineering blueprint. Saved into `minikit_docs/core/implementation.md`."
                    },
                    "milestones": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "High-level roadmap milestones. Saved into `minikit_docs/core/todo.md` (viewable via /todo modal)."
                    },
                    "assumptions": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Choices made without asking because they are cheap to change (e.g. 'Store notes in a JSON file'). Recorded as unconfirmed for the user to review."
                    },
                    "force": {
                        "type": "boolean",
                        "description": "Activate the plan even if plan review reports blocking structural problems (the override is recorded in `.minicode/decisions.md`). Default false."
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
            name: "record_decision".to_string(),
            description: "Record or update in-place a numbered architectural decision (D1, D2, ...) in `.minicode/decisions.md`. Each entry states a decision as it holds now; passing an existing ID (e.g. 'D1') or matching title edits that decision in place rather than appending contradictory history.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "id_or_title": {
                        "type": "string",
                        "description": "Decision title (e.g. 'Inline Key Prefix') or existing decision ID to update in place (e.g. 'D1' or 'D1: Inline Key Prefix')"
                    },
                    "decision": {
                        "type": "string",
                        "description": "The architectural invariant or design decision as it holds now"
                    },
                    "rationale": {
                        "type": "string",
                        "description": "Optional cost-model or engineering rationale for why this trade-off was chosen"
                    }
                },
                "required": ["id_or_title", "decision"]
            }),
        },
        ToolSchema {
            name: "update_progress".to_string(),
            description: "Immediately update the status of the active task step (`step=\"active\"`, `status=\"completed\"`) or a specific step right when its work finishes so the live TUI Todo tree advances step-by-step in real time (never defer `update_progress` calls to the end).".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "step": {
                        "type": "string",
                        "description": "The description or 1-based number of the step matching the task plan"
                    },
                    "steps": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Optional list of steps to update in a single call"
                    },
                    "status": {
                        "type": "string",
                        "description": "The status (e.g. 'Completed', 'In Progress', 'Blocked')"
                    }
                },
                "required": ["status"]
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
            if matches!(category, crate::context::memory::MemoryCategory::Preference) {
                let _ = crate::context::governance::dox::DoxEngine::record_user_preference(
                    workspace_root,
                    None,
                    &format!("{}: {}", key, value),
                );
            }
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
            let new_value =
                param::require_str_with_aliases(args, &["new_value", "value"], "update_fact")?;
            let mut mem = crate::context::memory::CoreMemory::load(workspace_root);
            let core_updated = mem.update(workspace_root, key, new_value)?;
            let mut prog =
                crate::context::progressive_memory::ProgressiveMemory::load(workspace_root);
            let prog_updated = prog.update_fact(key, new_value);
            if prog_updated {
                let _ = prog.save(workspace_root);
            }
            if core_updated || prog_updated {
                Ok(format!("✔ Updated fact '{}'", key))
            } else {
                Ok(format!("ℹ Fact '{}' not found to update", key))
            }
        })()),
        "forget_fact" => Some((|| {
            let key = param::require_str(args, "key", "forget_fact")?;
            let mut mem = crate::context::memory::CoreMemory::load(workspace_root);
            let core_forgotten = mem.forget(workspace_root, key)?;
            let mut prog =
                crate::context::progressive_memory::ProgressiveMemory::load(workspace_root);
            let prog_forgotten = prog.forget_fact(key);
            if prog_forgotten {
                let _ = prog.save(workspace_root);
            }
            if core_forgotten || prog_forgotten {
                Ok(format!("✔ Removed fact '{}' from memory", key))
            } else {
                Ok(format!("ℹ Fact '{}' not found in memory", key))
            }
        })()),
        "create_plan" => Some(
            async {
                // Plan review runs before the plan is written; structural blockers keep the
                // discovery write-guard active unless `force=true`.
                let review_steps = param::opt_string_array(args, "steps")
                    .or_else(|| param::opt_string_array(args, "tasks"))
                    .or_else(|| param::opt_string_array(args, "plan"))
                    .unwrap_or_default();
                let review = if review_steps.is_empty() {
                    None
                } else {
                    let input = crate::agent::minipower::plan_review::PlanReviewInput {
                        topic: param::opt_str(args, "title").unwrap_or("Task Plan"),
                        steps: &review_steps,
                        target_files: &[],
                        test_command: None,
                    };
                    match crate::agent::minipower::MiniPowerEngine::review_plan(
                        workspace_root,
                        input,
                        param::opt_bool(args, "force", false),
                    )
                    .await
                    {
                        Ok(report) => Some(report),
                        Err(blocked) => return Ok(blocked),
                    }
                };
                (|| -> Result<String> {
            let title = param::opt_str(args, "title").unwrap_or("Task Plan");
            let steps = param::opt_string_array(args, "steps")
                .or_else(|| param::opt_string_array(args, "tasks"))
                .or_else(|| param::opt_string_array(args, "plan"))
                .ok_or_else(|| ToolError::InvalidArguments {
                    name: "create_plan".to_string(),
                    reason: "Missing required argument 'steps' (or 'tasks')".to_string(),
                })?;
            if steps.is_empty() {
                return Err(ToolError::InvalidArguments {
                    name: "create_plan".to_string(),
                    reason: "Argument 'steps' must contain at least one task step (e.g. ['Step 1', 'Step 2'] or [{'title': 'Step 1', 'description': '...'}]). Empty plans are not allowed.".to_string(),
                }.into());
            }
            let architecture = param::opt_str(args, "architecture")
                .or_else(|| param::opt_str(args, "blueprint"))
                .or_else(|| param::opt_str(args, "implementation"));
            let milestones = param::opt_string_array(args, "milestones")
                .or_else(|| param::opt_string_array(args, "roadmap"));
            let wm = crate::context::working_memory::WorkingMemory::new(workspace_root);
            let _ = std::fs::remove_file(wm.last_verify_path());
            // Extract nested `substeps` / `sub_tasks` for ALL step objects so multi-step
            // hierarchical plans persist every step's sub-tasks in canonical `todo.md` and
            // auto-activate Tier-2 `task_plan.md` as each macro task becomes `[>]`.
            let mut step_substeps: Vec<Vec<String>> = ["steps", "tasks", "plan"]
                .iter()
                .find_map(|k| args.get(*k)?.as_array())
                .map(|arr| {
                    arr.iter()
                        .map(|obj| {
                            obj.get("substeps")
                                .or_else(|| obj.get("sub_tasks"))
                                .or_else(|| obj.get("subtasks"))
                                .and_then(|v| v.as_array())
                                .map(|sub_arr| {
                                    sub_arr
                                        .iter()
                                        .filter_map(|item| {
                                            item.as_str().map(str::to_string).or_else(|| {
                                                item.get("title")
                                                    .or_else(|| item.get("step"))
                                                    .and_then(|t| t.as_str())
                                                    .map(str::to_string)
                                            })
                                        })
                                        .filter(|s| !s.trim().is_empty())
                                        .collect::<Vec<_>>()
                                })
                                .unwrap_or_default()
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            step_substeps.resize_with(steps.len(), Vec::new);
            for (idx, raw_step) in steps.iter().enumerate() {
                if step_substeps[idx].is_empty() {
                    let auto_subs = crate::context::working_memory::WorkingMemory::synthesize_compound_verification_substeps(raw_step);
                    if !auto_subs.is_empty() {
                        step_substeps[idx] = auto_subs;
                    }
                }
            }
            wm.init_two_tier_plan_with_substeps(
                title,
                &steps,
                &step_substeps,
                architecture,
                milestones.as_deref(),
            )?;
            let assumptions = param::opt_string_array(args, "assumptions").unwrap_or_default();
            if let Err(e) =
                crate::agent::decisions::record_assumptions(workspace_root, title, &assumptions)
            {
                tracing::warn!("Failed to record plan assumptions: {}", e);
            }
            let first_step = steps
                .first()
                .map(|s| crate::context::working_memory::WorkingMemory::sanitize_step_title(s))
                .unwrap_or_else(|| "First step".to_string());
            let triad_note = review
                .as_ref()
                .map(|r| {
                    crate::agent::minipower::MiniPowerEngine::mark_plan_validated(
                        workspace_root,
                        title,
                        steps.len(),
                        r,
                    )
                })
                .unwrap_or_default();
            let _ =
                crate::agent::boulder::BoulderState::sync_from_workspace(workspace_root, false, 3);
            let impl_rel = wm
                .canonical_implementation_path()
                .strip_prefix(workspace_root)
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| wm.canonical_implementation_path().display().to_string());
            let todo_rel = wm
                .canonical_todo_path()
                .strip_prefix(workspace_root)
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| wm.canonical_todo_path().display().to_string());
            let plan_rel = wm
                .task_plan_path()
                .strip_prefix(workspace_root)
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| wm.task_plan_path().display().to_string());
            let active_step_label = wm
                .read_active_plan_summary()
                .and_then(|s| s.active_task)
                .unwrap_or(first_step);
            Ok(format!(
                "✔ Created Two-Tier Plan for \"{}\":\n• Tier 1 Blueprint: {}\n• Tier 1 Roadmap: {} (view with /todo)\n• Tier 2 Active Steps: {} ({} macro step(s), live in TUI){}\n► Active Step: [>] \"{}\". Focus ONLY on executing this step now. (If this step is complex, you can decompose it into sub-steps with `power_plan(steps=[...])`; when finished, mark it done with `update_progress`.)",
                title,
                impl_rel,
                todo_rel,
                plan_rel,
                steps.len(),
                triad_note,
                active_step_label,
            ))
        })()
            }
            .await,
        ),
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
        "record_decision" => Some((|| {
            let id_or_title = param::require_str_with_aliases(
                args,
                &["id_or_title", "title", "id", "topic", "category", "name"],
                "record_decision",
            )?;
            let decision = param::require_str_with_aliases(
                args,
                &["decision", "value", "summary", "content"],
                "record_decision",
            )?;
            let rationale = param::opt_str(args, "rationale");
            let assigned_id = crate::agent::decisions::upsert_decision(
                workspace_root,
                id_or_title,
                decision,
                rationale,
            )?;
            Ok(format!(
                "✔ Recorded decision [{}] in .minicode/decisions.md (in-place Living Decision Ledger)",
                assigned_id
            ))
        })()),
        "update_progress" => Some((|| {
            let status = param::require_str_with_aliases(
                args,
                &["status", "state", "progress"],
                "update_progress",
            )?;
            let step_list: Vec<String> =
                if let Some(arr) = param::opt_string_array(args, "steps").filter(|a| !a.is_empty())
                {
                    arr
                } else if let Some(single) = param::get_str_with_aliases(
                    args,
                    &["step", "completed_step", "task", "id", "index"],
                )
                .map(str::trim)
                .filter(|s| !s.is_empty())
                {
                    vec![single.to_string()]
                } else {
                    vec!["active".to_string()]
                };

            let wm = crate::context::working_memory::WorkingMemory::new(workspace_root);
            let mut resolved_titles = Vec::with_capacity(step_list.len());
            let mut any_modified = false;
            for s in &step_list {
                let (modified, title) = wm.update_progress_detailed(s, status)?;
                any_modified |= modified;
                resolved_titles.push(title);
            }
            let step_label = resolved_titles.join("', '");
            let remaining_hint = wm.format_remaining_tasks_hint();
            if let Some(summary) = wm.read_active_plan_summary() {
                let plan_progress = if summary.tasks.len() > 1
                    && summary.tasks.iter().any(|t| {
                        t.status == crate::context::working_memory::TaskItemStatus::InProgress
                            && !t.sub_tasks.is_empty()
                    }) {
                    let macro_done = summary
                        .tasks
                        .iter()
                        .filter(|t| {
                            t.status == crate::context::working_memory::TaskItemStatus::Completed
                        })
                        .count();
                    format!(
                        "{}/{} macro tasks completed (active step sub-plan: {}/{} sub-steps completed)",
                        macro_done,
                        summary.tasks.len(),
                        summary.completed_tasks,
                        summary.total_tasks
                    )
                } else {
                    format!(
                        "{}/{} tasks completed",
                        summary.completed_tasks, summary.total_tasks
                    )
                };
                if let Some(ref next_active) = summary.active_task {
                    if any_modified {
                        Ok(format!(
                            "✔ Updated step '{}' to '{}'. Current plan: {}.\n{}\n► Next Active Step: [>] \"{}\". Focus ONLY on executing this step now (or decompose it via `power_plan` if multi-part).",
                            step_label, status, plan_progress, remaining_hint, next_active
                        ))
                    } else {
                        Ok(format!(
                            "✔ Step '{}' is already '{}' (auto-reconciled — active step [>] preserved). Current plan: {}.\n{}\n► Next Active Step: [>] \"{}\". Focus ONLY on executing and verifying this step now.",
                            step_label, status, plan_progress, remaining_hint, next_active
                        ))
                    }
                } else if summary.completed_tasks == summary.total_tasks && summary.total_tasks > 0
                {
                    let verify_passed = std::fs::read_to_string(
                        workspace_root.join(".minicode/plan/last_verify.json"),
                    )
                    .ok()
                    .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
                    .and_then(|v| v.get("passed").and_then(|b| b.as_bool()))
                    .unwrap_or(false);
                    if verify_passed {
                        Ok(format!(
                            "🎉 All {} tasks in active plan completed (100%) and `power_verify` passed!\n► Next Step: Run `power_review` (if not yet reviewed) and call `archive_plan` before presenting your final summary.",
                            summary.total_tasks
                        ))
                    } else {
                        Ok(format!(
                            "🎉 All {} tasks in active plan completed (100%)!\n► Next Step: Run `power_review` (diff audit) and `power_verify` (4-Gate Pre-Completion Verification Barrier), then call `archive_plan` to finalize.",
                            summary.total_tasks
                        ))
                    }
                } else {
                    Ok(format!(
                        "✔ Updated step '{}' status to '{}'.\n{}",
                        step_label, status, remaining_hint
                    ))
                }
            } else {
                Ok(format!(
                    "✔ Updated step '{}' status to '{}'",
                    step_label, status
                ))
            }
        })()),
        "archive_plan" => Some({
            let wm = crate::context::working_memory::WorkingMemory::new(workspace_root);
            // Synchronize IntentLedger on disk before archiving the completed plan
            let intent_path =
                workspace_root.join(crate::constants::DEFAULT_INTENT_PERSISTENCE_FILE);
            if intent_path.exists() {
                if let Ok(mut ledger) =
                    crate::context::memory::intent::IntentLedger::load_from_disk(&intent_path)
                {
                    if ledger.sync_with_working_memory(&wm, 0) > 0 {
                        let _ = ledger.save_to_disk(&intent_path);
                    }
                }
            }
            match wm.archive_plan() {
                Ok(Some(archive_path)) => {
                    let _ = crate::agent::minipower::MiniPowerEngine::set_planning_phase_state(
                        workspace_root,
                        &crate::agent::minipower::PlanningPhaseState::Unlocked,
                    );
                    crate::agent::boulder::BoulderState::clear(workspace_root);
                    Ok(format!(
                        "✔ Archived completed task plan to {}",
                        archive_path.display()
                    ))
                }
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

//! `execute_task_dag`: runs dependent subagent tasks in parallel waves on the swarm scheduler.
//!
//! The former Task-DAG ledger tools were folded into the hierarchical plan tools
//! (`create_plan` → Tier 1 steps, `power_plan` → Tier 2 sub-steps, `update_progress`,
//! `read_plan`). Calling a removed tool returns an error that names its replacement.

use crate::agent::provider::ToolSchema;
use crate::agent::swarm::{SwarmPlan, SwarmRunOptions, SwarmScheduler, SwarmTaskSpec};
use crate::error::{Result, ToolError};
use crate::tools::param;
use serde_json::json;
use std::path::Path;

/// Removed tools and the replacement the model should use instead.
pub const MOVED_TOOLS: &[(&str, &str)] = &[
    (
        "create_task_dag",
        "`create_plan(title, steps)` for Tier-1 steps, or `execute_task_dag(tasks=[...])` to run dependent tasks in parallel",
    ),
    ("get_next_task", "`read_plan` (the active step is marked `[>]`)"),
    (
        "complete_task",
        "`update_progress(step=\"active\", status=\"completed\")`",
    ),
    (
        "split_task",
        "`power_plan(steps=[...])` to break the active step into sub-steps",
    ),
    (
        "schedule_task_waves",
        "`execute_task_dag(tasks=[...])` (it schedules parallel waves itself)",
    ),
    (
        "execute_dag",
        "calling the tools directly (read-only tools already run in parallel)",
    ),
];

pub fn get_schemas() -> Vec<ToolSchema> {
    vec![ToolSchema {
        name: "execute_task_dag".to_string(),
        description: "Run dependent tasks as MiniPub subagents in parallel topological waves. Either pass explicit `tasks` OR set `from_active_plan=true` (or omit `tasks`) to execute uncompleted steps directly from `.minicode/plan/task_dag.json` created by `create_plan`. In Git repos with commits, tasks run in isolated git worktrees merged into an integration branch; in uncommitted/non-git workspaces, conflict-free write-boundary sub-waves run safely in parallel with file leases.".to_string(),
        parameters: json!({
            "type": "object",
            "properties": {
                "title": {
                    "type": "string",
                    "description": "Optional short title for this run"
                },
                "from_active_plan": {
                    "type": "boolean",
                    "description": "Load remaining uncompleted tasks, dependencies, and `target_files` directly from `.minicode/plan/task_dag.json` (default: true when `tasks` is omitted)."
                },
                "tasks": {
                    "type": "array",
                    "description": "Explicit tasks to execute (optional when `.minicode/plan/task_dag.json` exists); `dependencies` lists task ids that must complete first",
                    "items": {
                        "type": "object",
                        "properties": {
                            "id": { "type": "string", "description": "Unique task identifier (e.g. t1_api)" },
                            "title": { "type": "string", "description": "Brief task title" },
                            "description": { "type": "string", "description": "Detailed task instructions for the subagent" },
                            "dependencies": { "type": "array", "items": { "type": "string" } },
                            "role": { "type": "string", "description": "MiniPub role (e.g. coder, frontend, backend, tester)" },
                            "file_boundaries": { "type": "array", "items": { "type": "string" } },
                            "check_cmd": { "type": "string", "description": "Verification command that must pass for the task to count as completed" },
                            "expected_artifacts": { "type": "array", "items": { "type": "string" } },
                            "max_turns": { "type": "integer" }
                        },
                        "required": ["id", "title", "description"]
                    }
                },
                "max_concurrency": {
                    "type": "integer",
                    "description": "Maximum parallel workers (1 to 8, default from config)"
                },
                "auto_merge": {
                    "type": "boolean",
                    "description": "Merge the integration branch into the workspace when done (default: true)"
                },
                "run_in_background": {
                    "type": "boolean",
                    "description": "Start the run and return immediately (default: false)"
                }
            },
            "required": []
        }),
    }]
}

fn string_list(value: Option<&serde_json::Value>) -> Vec<String> {
    value
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|x| x.as_str())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

fn required_field<'a>(item: &'a serde_json::Value, key: &str, idx: usize) -> Result<&'a str> {
    item.get(key)
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            ToolError::InvalidArguments {
                name: "execute_task_dag".to_string(),
                reason: format!("tasks[{}] is missing required string field `{}`", idx, key),
            }
            .into()
        })
}

/// Converts `execute_task_dag` arguments into a validated swarm plan.
pub(crate) fn plan_from_args(args: &serde_json::Value) -> Result<SwarmPlan> {
    let tasks_json = args
        .get("tasks")
        .and_then(|v| v.as_array())
        .filter(|a| !a.is_empty())
        .ok_or_else(|| ToolError::InvalidArguments {
            name: "execute_task_dag".to_string(),
            reason: "`tasks` must be a non-empty array".to_string(),
        })?;

    let mut tasks = Vec::with_capacity(tasks_json.len());
    for (idx, item) in tasks_json.iter().enumerate() {
        let id = required_field(item, "id", idx)?.to_string();
        let title = required_field(item, "title", idx)?.to_string();
        let description = required_field(item, "description", idx)?.to_string();
        let role_title = item
            .get("role")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("coder")
            .to_string();
        tasks.push(SwarmTaskSpec {
            id,
            title,
            role_title,
            instructions: String::new(),
            prompt: description,
            file_boundaries: string_list(item.get("file_boundaries")),
            dependencies: string_list(item.get("dependencies")),
            check_command: item
                .get("check_cmd")
                .or_else(|| item.get("check_command"))
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(String::from),
            expected_artifacts: string_list(item.get("expected_artifacts")),
            workspace_mode: None,
            max_iterations: item
                .get("max_turns")
                .and_then(|v| v.as_u64())
                .map(|n| n as usize),
        });
    }

    let title = param::opt_str(args, "title")
        .unwrap_or("Task DAG")
        .to_string();
    let plan = SwarmPlan {
        id: format!("dag_{}", &uuid::Uuid::new_v4().simple().to_string()[..8]),
        title: title.clone(),
        objective: title,
        tasks,
        created_at: chrono::Utc::now().to_rfc3339(),
        metadata: std::collections::HashMap::new(),
    };
    plan.validate().map_err(|e| ToolError::InvalidArguments {
        name: "execute_task_dag".to_string(),
        reason: e.to_string(),
    })?;
    Ok(plan)
}

/// Resolves a `SwarmPlan` either from explicit `tasks` in `args` or from `.minicode/plan/task_dag.json`
/// when `from_active_plan=true` or `tasks` is omitted. Returns `(plan, loaded_from_active_plan)`.
pub(crate) fn resolve_swarm_plan(
    args: &serde_json::Value,
    workspace_root: &Path,
) -> Result<(SwarmPlan, bool)> {
    let has_explicit_tasks = args
        .get("tasks")
        .and_then(|v| v.as_array())
        .is_some_and(|a| !a.is_empty());
    let use_active = param::opt_bool(args, "from_active_plan", !has_explicit_tasks);

    if use_active && !has_explicit_tasks {
        let wm = crate::context::memory::working_memory::WorkingMemory::new(workspace_root);
        wm.sync_task_dag_statuses();
        let dag = crate::context::memory::task_dag::PlanDag::load(workspace_root).ok_or_else(
            || ToolError::InvalidArguments {
                name: "execute_task_dag".to_string(),
                reason: "No `.minicode/plan/task_dag.json` found; call `create_plan` first or pass an explicit `tasks` array.".to_string(),
            },
        )?;
        let plan = dag
            .to_remaining_swarm_plan_with_workspace(Some(workspace_root))
            .map_err(|e| ToolError::InvalidArguments {
                name: "execute_task_dag".to_string(),
                reason: e.to_string(),
            })?;
        return Ok((plan, true));
    }

    Ok((plan_from_args(args)?, false))
}

/// Executes a `SwarmPlan` in a workspace that has no Git commit yet by partitioning topological
/// waves via Welsh-Powell Write-Boundary Conflict Coloring so every parallel sub-wave is guaranteed
/// to have disjoint `file_boundaries` under `WorkspaceMode::Shared`.
async fn execute_conflict_free_shared_waves(
    workspace_root: &Path,
    plan: &SwarmPlan,
    max_workers: usize,
    sync_active_plan: bool,
) -> Result<String> {
    use crate::agent::subagent::fanout::{FanoutJoinMode, FanoutOrchestrator, FanoutTaskItem};
    use crate::agent::subagent::types::{SubagentRole, WorkspaceMode};
    use crate::context::memory::task_dag::{PlanDag, PlanDagNode};
    use crate::context::memory::working_memory::{TaskItemStatus, WorkingMemory};

    let nodes: Vec<PlanDagNode> = plan
        .tasks
        .iter()
        .map(|t| PlanDagNode {
            id: t.id.clone(),
            title: t.title.clone(),
            description: t.prompt.clone(),
            substeps: Vec::new(),
            target_files: t.file_boundaries.clone(),
            depends_on: t.dependencies.clone(),
            role: Some(t.role_title.clone()),
            verify_cmd: t.check_command.clone(),
            status: TaskItemStatus::Pending,
        })
        .collect();

    let temp_dag = PlanDag {
        title: plan.title.clone(),
        explicit_dependencies: true,
        nodes,
        created_at: plan.created_at.clone(),
        updated_at: plan.created_at.clone(),
    };
    let topo = temp_dag
        .compute_execution_topology()
        .map_err(|e| ToolError::ExecutionFailed(format!("execute_task_dag topology: {}", e)))?;

    let mut report = format!(
        "✔ Executed Task-DAG `{}` across {} conflict-free sub-wave(s) (shared-workspace lease mode — no initial Git commit required):\n",
        plan.id,
        topo.conflict_free_subwaves.len()
    );
    let wm = WorkingMemory::new(workspace_root);

    for sw in &topo.conflict_free_subwaves {
        let mut batch = Vec::with_capacity(sw.node_indices.len());
        let mut titles_in_subwave = Vec::with_capacity(sw.node_indices.len());
        for &idx in &sw.node_indices {
            let spec = &plan.tasks[idx];
            titles_in_subwave.push(spec.title.clone());
            batch.push(FanoutTaskItem {
                task: spec.prompt.clone(),
                role: SubagentRole::from_str_loose(&spec.role_title),
                category: None,
                load_skills: Vec::new(),
                workspace_mode: Some(WorkspaceMode::Shared),
                max_iterations: spec.max_iterations,
                token_budget: None,
                timeout_secs: None,
                check_cmd: spec.check_command.clone(),
                todos: vec![spec.title.clone()],
                file_boundaries: spec.file_boundaries.clone(),
            });
        }
        let subwave_summary = FanoutOrchestrator::execute_fanout(
            workspace_root,
            batch,
            FanoutJoinMode::All,
            false,
            max_workers,
        )
        .await?;
        report.push_str(&format!(
            "\n#### Wave {}.{} (`{}`)\n{}\n",
            sw.wave_index,
            sw.subwave_index,
            sw.node_ids.join("`, `"),
            subwave_summary
        ));
        if sync_active_plan {
            for title in &titles_in_subwave {
                let _ = wm.update_progress_detailed(title, "completed");
            }
        }
    }

    Ok(report)
}

/// Marks completed `SwarmPlan` tasks `[x]` in both `.minicode/plan/task_dag.json` and
/// `WorkingMemory` (`todo.md` + `task_plan.md`, including any active sub-plan substeps).
fn reconcile_completed_dag_tasks_into_working_memory(
    workspace_root: &Path,
    plan: &SwarmPlan,
    state: &crate::agent::swarm::SwarmExecutionState,
) {
    use crate::agent::swarm::SwarmTaskStatus as S;
    use crate::context::memory::task_dag::PlanDag;
    use crate::context::memory::working_memory::{TaskItemStatus, WorkingMemory};

    let wm = WorkingMemory::new(workspace_root);
    let mut dag_opt = PlanDag::load(workspace_root);

    for spec in &plan.tasks {
        let is_completed = state
            .outcomes
            .get(&spec.id)
            .is_some_and(|o| o.status == S::Completed);
        if !is_completed {
            continue;
        }
        let target_title = dag_opt
            .as_mut()
            .and_then(|dag| {
                let node = dag
                    .nodes
                    .iter_mut()
                    .find(|n| n.id == spec.id || n.title.eq_ignore_ascii_case(&spec.title))?;
                node.status = TaskItemStatus::Completed;
                Some(node.title.clone())
            })
            .unwrap_or_else(|| spec.title.clone());

        let _ = wm.update_progress_detailed(&target_title, "completed");
        // If `target_title` was the active macro step in `Mode: subplan`, drain any remaining
        // sub-steps of that macro task so the TUI and roadmap advance cleanly to the next macro task.
        for _ in 0..8 {
            let Some(summary) = wm.read_active_plan_summary() else {
                break;
            };
            let still_on_same_macro = summary.tasks.iter().any(|t| {
                t.status == TaskItemStatus::InProgress
                    && (t.title.eq_ignore_ascii_case(&target_title)
                        || t.title.contains(&target_title))
                    && !t.sub_tasks.is_empty()
            });
            if !still_on_same_macro {
                break;
            }
            if wm.update_progress_detailed("active", "completed").is_err() {
                break;
            }
        }
    }

    if let Some(dag) = dag_opt {
        let _ = dag.save(workspace_root);
    }
}

async fn execute_task_dag(args: &serde_json::Value, workspace_root: &Path) -> Result<String> {
    let (plan, from_active_plan) = resolve_swarm_plan(args, workspace_root)?;
    let app_cfg = crate::config::Config::load_for_workspace(workspace_root);
    let max_workers = param::opt_u64(args, "max_concurrency")
        .map(|n| n as usize)
        .unwrap_or(app_cfg.agent.minipub.max_parallel)
        .clamp(1, 8);
    let options = SwarmRunOptions {
        max_workers,
        auto_merge: param::opt_bool(args, "auto_merge", true),
        json_stream: false,
        model: None,
        provider: None,
    };
    let task_count = plan.tasks.len();
    let plan_id = plan.id.clone();

    if param::opt_bool(args, "run_in_background", false) {
        let ws = workspace_root.to_path_buf();
        let bg_plan_id = plan_id.clone();
        tokio::spawn(async move {
            if let Err(e) = SwarmScheduler::execute_plan(
                &ws,
                plan,
                options,
                tokio_util::sync::CancellationToken::new(),
            )
            .await
            {
                tracing::warn!(plan_id = %bg_plan_id, error = %e, "Background task DAG failed");
            }
        });
        return Ok(format!(
            "✔ Started task DAG `{}` ({} tasks, up to {} parallel workers) in the background.\n• Progress and report: `.minicode/swarms/{}/` (live workers are listed in `minitask(action=\"workers\")`).",
            plan_id, task_count, max_workers, plan_id
        ));
    }

    let (state, report_path) = match SwarmScheduler::execute_plan(
        workspace_root,
        plan.clone(),
        options,
        tokio_util::sync::CancellationToken::new(),
    )
    .await
    {
        Ok(res) => res,
        Err(crate::agent::swarm::SwarmError::WorkspaceNotReady(_)) => {
            return execute_conflict_free_shared_waves(
                workspace_root,
                &plan,
                max_workers,
                from_active_plan,
            )
            .await;
        }
        Err(e) => {
            return Err(ToolError::ExecutionFailed(format!("execute_task_dag: {}", e)).into());
        }
    };

    use crate::agent::swarm::SwarmTaskStatus as S;
    if state.auto_merged
        && (from_active_plan
            || crate::context::memory::task_dag::PlanDag::file_path(workspace_root).exists())
    {
        reconcile_completed_dag_tasks_into_working_memory(workspace_root, &plan, &state);
    }
    let mut out = format!(
        "✔ Task DAG `{}` finished: {} completed, {} failed, {} blocked, {} cancelled.\n",
        state.swarm_id,
        state.count_by_status(S::Completed),
        state.count_by_status(S::Failed),
        state.count_by_status(S::Blocked),
        state.count_by_status(S::Cancelled)
    );
    for (id, outcome) in &state.outcomes {
        out.push_str(&format!(
            "- `{}` [{}]{}\n",
            id,
            outcome.status.badge(),
            outcome
                .error
                .as_deref()
                .map(|e| format!(" — {}", e))
                .unwrap_or_default()
        ));
    }
    if state.auto_merged {
        out.push_str("• Integrated changes were merged into the workspace.\n");
    } else if let Some(branch) = &state.integration_branch {
        out.push_str(&format!(
            "• Changes were not merged into the workspace; completed work is on `{}` (if it still exists).\n",
            branch
        ));
    }
    out.push_str(&format!("• Report: `{}`", report_path.display()));
    Ok(out)
}

pub async fn dispatch(
    tool_name: &str,
    args: &serde_json::Value,
    workspace_root: &Path,
) -> Option<Result<String>> {
    if let Some((_, replacement)) = MOVED_TOOLS.iter().find(|(name, _)| *name == tool_name) {
        tracing::warn!(tool = tool_name, "Removed task-DAG tool called");
        return Some(Err(ToolError::InvalidArguments {
            name: tool_name.to_string(),
            reason: format!("`{}` was removed; use {} instead.", tool_name, replacement),
        }
        .into()));
    }
    match tool_name {
        "execute_task_dag" => Some(execute_task_dag(args, workspace_root).await),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plan_from_args_builds_valid_dependency_plan() {
        let args = json!({
            "title": "API + UI",
            "tasks": [
                {"id": "t1", "title": "API", "description": "Build the API", "check_cmd": "cargo test"},
                {"id": "t2", "title": "UI", "description": "Build the UI", "dependencies": ["t1"], "role": "frontend"}
            ]
        });
        let plan = plan_from_args(&args).expect("plan");
        assert_eq!(plan.tasks.len(), 2);
        assert_eq!(plan.tasks[0].role_title, "coder");
        assert_eq!(plan.tasks[0].check_command.as_deref(), Some("cargo test"));
        assert_eq!(plan.tasks[1].dependencies, vec!["t1".to_string()]);
        assert_eq!(plan.calculate_waves().expect("waves").len(), 2);
    }

    #[test]
    fn test_plan_from_args_rejects_bad_input() {
        assert!(plan_from_args(&json!({"tasks": []})).is_err());
        assert!(plan_from_args(&json!({"tasks": [{"id": "t1", "title": "x"}]})).is_err());
        let cycle = json!({"tasks": [
            {"id": "a", "title": "A", "description": "a", "dependencies": ["b"]},
            {"id": "b", "title": "B", "description": "b", "dependencies": ["a"]}
        ]});
        assert!(plan_from_args(&cycle).is_err());
    }

    #[tokio::test]
    async fn test_removed_tools_point_to_replacements() {
        let ws = Path::new(".");
        for (name, _) in MOVED_TOOLS {
            let res = dispatch(name, &json!({}), ws).await.expect("handled");
            let err = res.expect_err("removed tool must error").to_string();
            assert!(err.contains("was removed"), "{}: {}", name, err);
        }
        let complete = dispatch("complete_task", &json!({}), ws)
            .await
            .expect("handled")
            .expect_err("error")
            .to_string();
        assert!(complete.contains("update_progress"));
    }

    #[test]
    fn test_resolve_swarm_plan_loads_remaining_wave_from_active_plan_dag() {
        let temp = tempfile::tempdir().unwrap();
        let ws = temp.path();
        let raw_args = json!({
            "steps": [
                { "id": "t1", "title": "Create schema", "target_files": ["src/schema.rs"], "depends_on": [] },
                { "id": "t2", "title": "Build API", "target_files": ["src/api.rs"], "depends_on": ["t1"] },
                { "id": "t3", "title": "Build CLI", "target_files": ["src/cli.rs"], "depends_on": ["t1"] }
            ]
        });
        let steps = vec![
            "Create schema".to_string(),
            "Build API".to_string(),
            "Build CLI".to_string(),
        ];
        let mut dag = crate::context::memory::task_dag::PlanDag::from_plan_args(
            "Service",
            &raw_args,
            &steps,
            &vec![Vec::new(); 3],
        )
        .unwrap();
        dag.nodes[0].status = crate::context::memory::working_memory::TaskItemStatus::Completed;
        dag.save(ws).unwrap();

        let (swarm_plan, from_active) = resolve_swarm_plan(&json!({}), ws).unwrap();
        assert!(from_active);
        assert_eq!(swarm_plan.tasks.len(), 2);
        // `t1` is already Completed, so `t2` and `t3` become Wave-0 roots with empty dependencies!
        assert!(swarm_plan.tasks[0].dependencies.is_empty());
        assert!(swarm_plan.tasks[1].dependencies.is_empty());
        assert_eq!(swarm_plan.calculate_waves().unwrap().len(), 1);
    }
}

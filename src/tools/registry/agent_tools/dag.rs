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
        description: "Run a set of dependent tasks as MiniPub subagents in parallel waves. Each task runs in its own git worktree branched from a per-run integration branch, so a task sees the code of the tasks it depends on; completed tasks are merged into the integration branch immediately and (by default) into the workspace at the end. Requires a git repository with at least one commit. For a single task use `delegate_task`; for independent tasks without dependencies use `fanout_subagents`.".to_string(),
        parameters: json!({
            "type": "object",
            "properties": {
                "title": {
                    "type": "string",
                    "description": "Optional short title for this run"
                },
                "tasks": {
                    "type": "array",
                    "description": "Tasks to execute; `dependencies` lists task ids that must complete first",
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
            "required": ["tasks"]
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

async fn execute_task_dag(args: &serde_json::Value, workspace_root: &Path) -> Result<String> {
    let plan = plan_from_args(args)?;
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

    let (state, report_path) = SwarmScheduler::execute_plan(
        workspace_root,
        plan,
        options,
        tokio_util::sync::CancellationToken::new(),
    )
    .await
    .map_err(|e| ToolError::ExecutionFailed(format!("execute_task_dag: {}", e)))?;

    use crate::agent::swarm::SwarmTaskStatus as S;
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
}

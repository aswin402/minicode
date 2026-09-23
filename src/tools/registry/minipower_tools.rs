//! MiniPower autonomous methodology, verification barrier, and worktree execution tools.

use crate::agent::minipower::MiniPowerEngine;
use crate::agent::provider::ToolSchema;
use crate::agent::subagent::fanout::{FanoutJoinMode, FanoutOrchestrator, FanoutTaskItem};
use crate::agent::subagent::types::{SubagentRole, WorkspaceMode};
use crate::agent::verification_barrier::VerificationBarrier;
use crate::error::{Result, ToolError};
use crate::git::{GitReviewer, GitService};
use crate::tools::param::*;
use serde_json::json;
use std::path::Path;

/// Returns the complete list of MiniPower tool schemas for autonomous agent operations.
pub fn get_schemas() -> Vec<ToolSchema> {
    vec![
        ToolSchema {
            name: "power_status".to_string(),
            description: "Inspect the active MiniPower autonomous engineering methodology status, 6 core pillars, anti-rationalization guardrails ('Red Flags'), and verification barrier rules.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolSchema {
            name: "power_brainstorm".to_string(),
            description: "Execute Socratic brainstorming & spec refinement for complex, ambiguous, or multi-step requests. Analyzes trade-offs, poses high-leverage clarifying questions, and proposes 2-3 architectural approaches before writing code.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "topic": {
                        "type": "string",
                        "description": "Feature, architecture decision, bug, or topic to brainstorm and clarify"
                    }
                },
                "required": ["topic"]
            }),
        },
        ToolSchema {
            name: "power_plan".to_string(),
            description: "Generate or update a structured MiniPower implementation plan with bite-sized atomic tasks (2-5 min each), target file paths, verifiable acceptance criteria, and concrete verification commands. Saves to todo.md in project documentation.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "topic": {
                        "type": "string",
                        "description": "Feature, bug, or task specification to plan into bite-sized tasks"
                    },
                    "save_to_docs": {
                        "type": "boolean",
                        "description": "Whether to append/persist the plan into project documentation (todo.md / implementation.md). Defaults to true."
                    }
                },
                "required": ["topic"]
            }),
        },
        ToolSchema {
            name: "power_review".to_string(),
            description: "Run an adversarial multi-agent code review on current workspace changes across Stage 1 (Spec Compliance: acceptance criteria met) and Stage 2 (Code Quality: zero unwraps/panics, security, error handling, performance).".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "staged_only": {
                        "type": "boolean",
                        "description": "If true, only review staged changes (--cached). If false, review all uncommitted changes. Defaults to false."
                    }
                }
            }),
        },
        ToolSchema {
            name: "power_verify".to_string(),
            description: "Run the 4-Gate Pre-Completion Verification Barrier programmatically: Gate 1 (AST Syntax & Compiler Integrity), Gate 2 (Reproducer & Regression Test Suite), Gate 3 (Structural Integrity & Git Merge Conflicts), Gate 4 (Diff Sanity & Secret Leaks). Guarantees exit code 0 before completing tasks.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "files": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Optional list of modified file paths to verify. If omitted, automatically detects modified and staged files from git status."
                    }
                }
            }),
        },
        ToolSchema {
            name: "power_worktree_task".to_string(),
            description: "Execute an isolated mutating task inside an ephemeral Git worktree sandbox via a specialized subagent, with automated verification and safe 3-way merge arbitration back into the workspace.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "task": {
                        "type": "string",
                        "description": "Task instructions or feature description for the isolated worker"
                    },
                    "auto_merge": {
                        "type": "boolean",
                        "description": "If true, automatically merges the worktree branch into workspace if verification passes. Defaults to true."
                    }
                },
                "required": ["task"]
            }),
        },
    ]
}

/// Dispatches a tool invocation to the corresponding MiniPower handler.
pub async fn dispatch(
    tool_name: &str,
    args: &serde_json::Value,
    workspace_root: &Path,
) -> Option<Result<String>> {
    match tool_name {
        "power_status" | "minipower_status" => Some(Ok(MiniPowerEngine::format_status_summary())),
        "power_brainstorm" | "minipower_brainstorm" => Some(
            async {
                let topic = get_str_with_aliases(args, &["topic", "goal", "prompt", "idea"])
                    .ok_or_else(|| ToolError::InvalidArguments {
                        name: "power_brainstorm".to_string(),
                        reason: "Missing required argument 'topic'".to_string(),
                    })?;
                let prompt = MiniPowerEngine::format_brainstorm_prompt(workspace_root, topic);
                Ok(prompt)
            }
            .await,
        ),
        "power_plan" | "minipower_plan" => Some(
            async {
                let topic = get_str_with_aliases(args, &["topic", "goal", "prompt", "task"])
                    .ok_or_else(|| ToolError::InvalidArguments {
                        name: "power_plan".to_string(),
                        reason: "Missing required argument 'topic'".to_string(),
                    })?;
                let save_to_docs = opt_bool(args, "save_to_docs", true);
                let plan = MiniPowerEngine::format_plan_prompt(workspace_root, topic);

                if save_to_docs {
                    let docs_dir = crate::tools::minikit::resolve_docs_dir(workspace_root);
                    if let Ok(()) = std::fs::create_dir_all(&docs_dir) {
                        let todo_file = docs_dir.join("todo.md");
                        let append_content =
                            format!("\n\n<!-- MiniPower Plan: {} -->\n{}", topic, plan);
                        let _ = std::fs::OpenOptions::new()
                            .create(true)
                            .append(true)
                            .open(&todo_file)
                            .and_then(|mut f| {
                                std::io::Write::write_all(&mut f, append_content.as_bytes())
                            });
                    }
                }

                Ok(plan)
            }
            .await,
        ),
        "power_review" | "minipower_review" => Some(
            async {
                let staged_only =
                    opt_bool(args, "staged_only", false) || opt_bool(args, "staged", false);
                let report = GitReviewer::review_workspace(workspace_root, staged_only).await?;
                Ok(GitReviewer::format_report(&report))
            }
            .await,
        ),
        "power_verify" | "minipower_verify" => Some(
            async {
                let explicit_files: Option<Vec<String>> =
                    args.get("files").and_then(|v| v.as_array()).map(|arr| {
                        arr.iter()
                            .filter_map(|item| item.as_str().map(|s| s.to_string()))
                            .collect()
                    });

                let files = match explicit_files {
                    Some(fl) if !fl.is_empty() => fl,
                    _ => {
                        let git = GitService::new(workspace_root.to_path_buf());
                        if git.is_git_repo().await {
                            if let Ok(st) = git.get_status().await {
                                let mut all = st.staged;
                                all.extend(st.unstaged);
                                all.sort();
                                all.dedup();
                                all
                            } else {
                                vec![]
                            }
                        } else {
                            vec![]
                        }
                    }
                };

                let report = VerificationBarrier::verify(workspace_root, &files).await;
                Ok(report.format_report())
            }
            .await,
        ),
        "power_worktree_task" | "minipower_worktree_task" | "power_task" => Some(
            async {
                let task_prompt = get_str_with_aliases(args, &["task", "prompt", "instruction"])
                    .ok_or_else(|| ToolError::InvalidArguments {
                        name: "power_worktree_task".to_string(),
                        reason: "Missing required argument 'task'".to_string(),
                    })?;
                let auto_merge = opt_bool(args, "auto_merge", true);

                let task_item = FanoutTaskItem {
                    task: task_prompt.to_string(),
                    role: SubagentRole::Coder,
                    workspace_mode: Some(WorkspaceMode::Worktree),
                    max_iterations: Some(15),
                    check_cmd: None,
                };

                FanoutOrchestrator::execute_fanout(
                    workspace_root,
                    vec![task_item],
                    FanoutJoinMode::All,
                    auto_merge,
                    1,
                )
                .await
                .map_err(crate::error::MinicodeError::from)
            }
            .await,
        ),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_minipower_tools_schema_count() {
        let schemas = get_schemas();
        assert_eq!(schemas.len(), 6);
        let names: Vec<String> = schemas.into_iter().map(|s| s.name).collect();
        assert!(names.contains(&"power_status".to_string()));
        assert!(names.contains(&"power_brainstorm".to_string()));
        assert!(names.contains(&"power_plan".to_string()));
        assert!(names.contains(&"power_review".to_string()));
        assert!(names.contains(&"power_verify".to_string()));
        assert!(names.contains(&"power_worktree_task".to_string()));
    }

    #[tokio::test]
    async fn test_power_status_dispatch() {
        let temp = tempfile::tempdir().unwrap();
        let res = dispatch("power_status", &json!({}), temp.path()).await;
        assert!(res.is_some());
        let output = res.unwrap().unwrap();
        assert!(output.contains("MiniPower"));
        assert!(output.contains("Core Pillars"));
    }

    #[tokio::test]
    async fn test_power_brainstorm_dispatch() {
        let temp = tempfile::tempdir().unwrap();
        let res = dispatch(
            "power_brainstorm",
            &json!({"topic": "Distributed Cache"}),
            temp.path(),
        )
        .await;
        assert!(res.is_some());
        let output = res.unwrap().unwrap();
        assert!(output.contains("Distributed Cache"));
        assert!(output.contains("Socratic Brainstorming"));
    }
}

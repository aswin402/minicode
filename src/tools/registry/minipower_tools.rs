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
        // `power_status` is intentionally not advertised (static text already in
        // the system prompt); its dispatch handler remains for compatibility.
        ToolSchema {
            name: "power_brainstorm".to_string(),
            description: "Start the design phase for open-ended work (a new product or feature, a redesign, an architecture change) whose requirements or approach are not settled. Returns the procedure, which drives `ask_user`: clarify goals and constraints in batched rounds, offer 2-3 approaches with a recommendation, write the agreed spec to the project docs, get approval, then `create_plan`. Do NOT use for small, fully specified changes or bug fixes.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "topic": {
                        "type": "string",
                        "description": "The feature, product or change to design"
                    }
                },
                "required": ["topic"]
            }),
        },
        ToolSchema {
            name: "power_plan".to_string(),
            description: "Decompose the currently active `[>]` task from `minikit_docs/core/todo.md` into bite-sized tactical sub-steps (2-5 min each) in `.minicode/plan/task_plan.md`. While these tactical sub-steps are active, the inline TUI widget focuses exclusively on them and `/todo` nests them under the active milestone task. When all tactical sub-steps finish via `update_progress`, the parent task in `todo.md` automatically marks `[x]` and advances to the next task.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "topic": {
                        "type": "string",
                        "description": "Title of the active macro task or feature being decomposed into tactical sub-steps"
                    },
                    "steps": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Ordered list of bite-sized tactical sub-steps (e.g. ['1. Check theme tokens in index.css', '2. Search miniblocks for hero layout', '3. Implement HeroSection.tsx']) for this active task"
                    },
                    "save_to_docs": {
                        "type": "boolean",
                        "description": "Whether to persist architectural notes into minikit_docs/core/implementation.md. Defaults to true."
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
                let steps = opt_string_array(args, "steps")
                    .or_else(|| opt_string_array(args, "tasks"))
                    .unwrap_or_default();
                let save_to_docs = opt_bool(args, "save_to_docs", true);
                let mut plan = MiniPowerEngine::format_plan_prompt(workspace_root, topic);

                if !steps.is_empty() {
                    let wm = crate::context::working_memory::WorkingMemory::new(workspace_root);
                    wm.init_tactical_subplan(topic, &steps)?;
                    let first_step = steps
                        .first()
                        .map(|s| {
                            crate::context::working_memory::WorkingMemory::sanitize_step_title(s)
                        })
                        .unwrap_or_default();
                    plan.push_str(&format!(
                        "\n\n✔ Activated {} tactical sub-steps in `.minicode/plan/task_plan.md` for active task \"{}\".\n► Active Sub-Step: [>] \"{}\". Complete each sub-step with `update_progress`.",
                        steps.len(),
                        topic,
                        first_step
                    ));
                }

                if save_to_docs {
                    let impl_file = crate::tools::minikit::resolve_doc_path(
                        workspace_root,
                        "implementation.md",
                    );
                    if let Some(parent) = impl_file.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    let steps_md = if steps.is_empty() {
                        plan.clone()
                    } else {
                        steps
                            .iter()
                            .enumerate()
                            .map(|(i, s)| format!("{}. {}", i + 1, s))
                            .collect::<Vec<_>>()
                            .join("\n")
                    };
                    let append_impl =
                        format!("\n\n## 🛠️ MiniPower Sub-Plan: {}\n{}\n", topic, steps_md);
                    let _ = std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(&impl_file)
                        .and_then(|mut f| {
                            std::io::Write::write_all(&mut f, append_impl.as_bytes())
                        });
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
        assert_eq!(schemas.len(), 5);
        let names: Vec<String> = schemas.into_iter().map(|s| s.name).collect();
        assert!(!names.contains(&"power_status".to_string()));
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

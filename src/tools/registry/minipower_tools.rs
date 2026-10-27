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
            description: "Start or persist the design phase for open-ended work (a new product or feature, a redesign, an architecture change) whose requirements or approach are not settled. Inspects live workspace state (`minikit_docs/core/`, `.minicode/decisions.md`), sizes ceremony (`QuickTask`, `Spike`, `SmallChange`, `Project`), optionally saves a design specification (`spec`) to `minikit_docs/core/spec.md`, runs the cold-reader Builder Check (`run_builder_check=true`), and guides `ask_user` / `record_decision` -> `create_plan` / `power_plan`. Do NOT use for small, fully specified changes or bug fixes.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "topic": {
                        "type": "string",
                        "description": "The feature, product or change to design"
                    },
                    "spec": {
                        "type": "string",
                        "description": "Optional agreed design specification or architecture summary to persist to `minikit_docs/core/spec.md`."
                    },
                    "size": {
                        "type": "string",
                        "description": "Optional ceremony size classification: 'quick_task', 'spike', 'small_change', or 'project'."
                    },
                    "run_builder_check": {
                        "type": "boolean",
                        "description": "If true (or when size='project' and spec is provided), includes the cold-reader Builder Check prompt to surface missing builder decisions in one round."
                    },
                    "open_questions": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Optional list of unresolved design questions to clarify via `ask_user`."
                    }
                },
                "required": ["topic"]
            }),
        },
        ToolSchema {
            name: "power_plan".to_string(),
            description: "Decompose the currently active `[>]` task from `minikit_docs/core/todo.md` into bite-sized tactical sub-steps (2-5 min each) in `.minicode/plan/task_plan.md` (or initialize a full two-tier plan if no plan exists yet), with optional 5-item `review_focus` (implied inputs/failure modes not covered by task tests). While tactical sub-steps are active, the inline TUI widget focuses on them and `/todo` nests them under the active milestone task. When all tactical sub-steps finish via `update_progress`, the parent task in `todo.md` automatically marks `[x]` and advances to the next task.".to_string(),
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
                    "review_focus": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Optional list of up to 5 implied inputs or failure modes that the spec implies and task tests should pin (`obra/superpowers` Review Focus)."
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
            description: "Run an adversarial multi-agent code review on current workspace changes across Stage 1 (Spec Compliance: acceptance criteria met, 'spec is a vision document') and Stage 2 (Code Quality: zero unwraps/panics, security, error handling, test falsifiability, performance). Call after completing a major feature or before `archive_plan`.".to_string(),
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
            description: "Run the 4-Gate Pre-Completion Verification Barrier programmatically: Gate 1 (AST Syntax & Compiler Integrity), Gate 2 (Reproducer & Regression Test Suite), Gate 3 (Structural Integrity & Git Merge Conflicts), Gate 4 (Diff Sanity & Secret Leaks). Records verification status in `.minicode/plan/last_verify.json`. Call when all plan steps are completed before `archive_plan`.".to_string(),
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
        ToolSchema {
            name: "power_diagnose".to_string(),
            description: "Run a 7-dimension MiniPower session & workspace forensics audit (`diagnosing-minipower`): inspects `.minicode/plan/task_plan.md`, `.minicode/decisions.md` (`[D1]..[DN]`), `.minicode/plan/last_verify.json`, `minikit_docs/core/todo.md`, active skills, and search caches across `skill-timeline`, `plan-adherence`, `request-conflicts`, `quality-evidence`, `repeated-work`, `stumbles`, and `cost-and-time` with `path:line` evidence.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "complaint": {
                        "type": "string",
                        "description": "Optional symptom or question to focus the forensic audit (e.g. 'why did it take too long', 'it ignored the plan', 'skill never fired', 'repeated work')."
                    }
                }
            }),
        },
    ]
}

fn append_doc_entry(
    workspace_root: &Path,
    doc_name: &str,
    entry: &str,
) -> Result<std::path::PathBuf> {
    let doc_file = crate::tools::minikit::resolve_doc_path(workspace_root, doc_name);
    if let Some(parent) = doc_file.parent() {
        std::fs::create_dir_all(parent).map_err(|e| ToolError::FileOp {
            path: parent.display().to_string(),
            source: e,
        })?;
    }
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&doc_file)
        .and_then(|mut f| std::io::Write::write_all(&mut f, entry.as_bytes()))
        .map_err(|e| ToolError::FileOp {
            path: doc_file.display().to_string(),
            source: e,
        })?;
    Ok(doc_file)
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
                let topic = require_str_with_aliases(
                    args,
                    &["topic", "domain", "goal", "prompt", "idea"],
                    "power_brainstorm",
                )?;
                let open_questions = opt_string_array(args, "open_questions").unwrap_or_default();
                let ceremony_size = crate::agent::minipower::CeremonySize::classify(
                    workspace_root,
                    topic,
                    opt_str(args, "size"),
                    open_questions.len(),
                );
                let mut prompt = MiniPowerEngine::format_brainstorm_prompt(workspace_root, topic);
                prompt.push_str(&format!(
                    "\n**Detected Ceremony Size:** `{}`\n",
                    ceremony_size
                ));
                let is_project = ceremony_size == crate::agent::minipower::CeremonySize::Project;
                let want_builder_check = opt_bool(args, "run_builder_check", is_project);

                let mut rel_spec_path = "minikit_docs/core/spec.md".to_string();
                if let Some(spec_text) = opt_str(args, "spec") {
                    if !spec_text.trim().is_empty() {
                        let entry = format!(
                            "\n\n## Brainstorm Specification: {}\n\n{}\n",
                            topic,
                            spec_text.trim()
                        );
                        let spec_file = append_doc_entry(workspace_root, "spec.md", &entry)?;
                        prompt.push_str(&format!(
                            "\n\n✔ Persisted design specification for \"{}\" to `{}`.",
                            topic,
                            spec_file.display()
                        ));
                        rel_spec_path = spec_file
                            .strip_prefix(workspace_root)
                            .unwrap_or(&spec_file)
                            .display()
                            .to_string();
                    }
                }

                if want_builder_check {
                    prompt.push_str("\n\n");
                    prompt.push_str(&MiniPowerEngine::format_builder_check_prompt(
                        topic,
                        &rel_spec_path,
                    ));
                }

                let has_decisions =
                    crate::agent::decisions::read_for_prompt(workspace_root).is_some();
                if !open_questions.is_empty() {
                    prompt.push_str(&format!(
                        "\n\n► Next Step: Call `ask_user` to resolve the {} open design question(s) ({}), then call `create_plan` (or `power_plan`) to begin execution.",
                        open_questions.len(),
                        open_questions.join("; ")
                    ));
                } else if !has_decisions {
                    prompt.push_str(
                        "\n\n► Next Step: If high-impact architectural or stack choices remain open, call `ask_user` and `record_decision` to settle `.minicode/decisions.md`. Otherwise, call `create_plan` (or `power_plan`) to structure the implementation steps.",
                    );
                } else {
                    prompt.push_str(
                        "\n\n► Next Step: User decisions are recorded in `.minicode/decisions.md`. Call `create_plan` to initialize the milestone roadmap, or `power_plan(topic=..., steps=[...])` to decompose an active task into tactical sub-steps.",
                    );
                }

                Ok(prompt)
            }
            .await,
        ),
        "power_plan" | "minipower_plan" => Some(
            async {
                let topic = require_str_with_aliases(
                    args,
                    &["topic", "goal", "prompt", "task"],
                    "power_plan",
                )?;
                let steps = opt_string_array(args, "steps")
                    .or_else(|| opt_string_array(args, "tasks"))
                    .unwrap_or_default();
                let review_focus = opt_string_array(args, "review_focus").unwrap_or_default();
                let save_to_docs = opt_bool(args, "save_to_docs", true);
                let mut plan = MiniPowerEngine::format_plan_prompt(workspace_root, topic);

                if !review_focus.is_empty() {
                    plan.push_str("\n\n### 🎯 Review Focus (Implied Inputs & Failure Modes Pinned by Tests):\n");
                    for (idx, item) in review_focus.iter().take(5).enumerate() {
                        plan.push_str(&format!("{}. {}\n", idx + 1, item));
                    }
                }

                if !steps.is_empty() {
                    let wm = crate::context::working_memory::WorkingMemory::new(workspace_root);
                    let _ = std::fs::remove_file(
                        workspace_root.join(".minicode/plan/last_verify.json"),
                    );
                    let has_existing_roadmap = wm.read_active_plan_summary().is_some();
                    if has_existing_roadmap {
                        wm.init_tactical_subplan(topic, &steps)?;
                    } else {
                        wm.init_two_tier_plan(topic, &steps, None, None)?;
                    }
                    let first_step = steps
                        .first()
                        .map(|s| {
                            crate::context::working_memory::WorkingMemory::sanitize_step_title(s)
                        })
                        .unwrap_or_default();
                    let plan_audit =
                        crate::tools::registry::context_tools::memory::audit_plan_steps(
                            workspace_root,
                            &steps,
                        );
                    plan.push_str(&format!(
                        "\n\n✔ Activated {} tactical sub-steps in `.minicode/plan/task_plan.md` for active task \"{}\".\n► Active Sub-Step: [>] \"{}\". Complete each sub-step with `update_progress(step=\"active\", status=\"completed\")`.{}",
                        steps.len(),
                        topic,
                        first_step,
                        plan_audit
                    ));
                } else {
                    plan.push_str(
                        "\n\n► Tip: Pass `steps: [\"1. ...\", \"2. ...\"]` to `power_plan` to register tactical sub-steps in `.minicode/plan/task_plan.md`, or call `create_plan` for top-level milestones.",
                    );
                }

                if save_to_docs {
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
                    let _ = append_doc_entry(workspace_root, "implementation.md", &append_impl);
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
                let mut formatted = GitReviewer::format_report(&report);
                let actionable_count = report
                    .findings
                    .iter()
                    .filter(|f| f.severity == "HIGH" || f.severity == "MEDIUM")
                    .count();
                if actionable_count > 0 {
                    formatted.push_str(&format!(
                        "\n► Next Step: Resolve the {} HIGH/MEDIUM review finding(s) above, then re-run `power_verify` to confirm all 4 gates pass.\n",
                        actionable_count
                    ));
                } else {
                    formatted.push_str(
                        "\n► Next Step: Code review passed with 0 HIGH/MEDIUM findings! If `power_verify` has passed and all plan tasks are `[x]`, call `archive_plan` to finalize.\n",
                    );
                }
                Ok(formatted)
            }
            .await,
        ),
        "power_verify" | "minipower_verify" => Some(
            async {
                let explicit_files = opt_string_array(args, "files");

                let files = match explicit_files {
                    Some(fl) if !fl.is_empty() => fl,
                    _ => {
                        let git = GitService::new(workspace_root.to_path_buf());
                        let git_files = if git.is_git_repo().await {
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
                        };
                        if git_files.is_empty() {
                            GitReviewer::discover_workspace_source_files(workspace_root)
                        } else {
                            git_files
                        }
                    }
                };

                let report = VerificationBarrier::verify(workspace_root, &files).await;
                let verify_dir = workspace_root.join(".minicode/plan");
                let _ = std::fs::create_dir_all(&verify_dir);
                let verify_state = json!({
                    "passed": report.all_passed,
                    "files_checked": files.len(),
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                });
                let _ = std::fs::write(
                    verify_dir.join("last_verify.json"),
                    serde_json::to_string_pretty(&verify_state).unwrap_or_default(),
                );

                let mut out = report.format_report();
                if report.all_passed {
                    out.push_str(
                        "\n► Next Step: All 4 Verification Gates PASSED (`last_verify.json` recorded). Optionally run `power_review` to audit diff quality, or call `archive_plan` if all plan steps are complete.\n",
                    );
                } else {
                    out.push_str(
                        "\n► Next Step: Fix the verification failure(s) reported above, then re-run `power_verify` until all 4 gates pass.\n",
                    );
                }
                Ok(out)
            }
            .await,
        ),
        "power_worktree_task" | "minipower_worktree_task" | "power_task" => Some(
            async {
                let task_prompt = require_str_with_aliases(
                    args,
                    &["task", "prompt", "instruction"],
                    "power_worktree_task",
                )?;
                let auto_merge = opt_bool(args, "auto_merge", true);

                let task_item = FanoutTaskItem {
                    task: task_prompt.to_string(),
                    role: SubagentRole::Coder,
                    workspace_mode: Some(WorkspaceMode::Worktree),
                    max_iterations: Some(15),
                    check_cmd: None,
                    todos: Vec::new(),
                    file_boundaries: Vec::new(),
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
        "power_diagnose" | "minipower_diagnose" => Some({
            let complaint = opt_str(args, "complaint")
                .or_else(|| opt_str(args, "issue"))
                .or_else(|| opt_str(args, "symptom"));
            Ok(MiniPowerEngine::diagnose_workspace(
                workspace_root,
                complaint,
            ))
        }),
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
        assert!(!names.contains(&"power_status".to_string()));
        assert!(names.contains(&"power_brainstorm".to_string()));
        assert!(names.contains(&"power_plan".to_string()));
        assert!(names.contains(&"power_review".to_string()));
        assert!(names.contains(&"power_verify".to_string()));
        assert!(names.contains(&"power_worktree_task".to_string()));
        assert!(names.contains(&"power_diagnose".to_string()));
    }

    #[tokio::test]
    async fn test_power_status_and_diagnose_dispatch() {
        let temp = tempfile::tempdir().unwrap();
        let res = dispatch("power_status", &json!({}), temp.path()).await;
        assert!(res.is_some());
        let output = res.unwrap().unwrap();
        assert!(output.contains("MiniPower"));
        assert!(output.contains("Core Pillars"));

        let diag = dispatch(
            "power_diagnose",
            &json!({"complaint": "why did it repeat work"}),
            temp.path(),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(diag.contains("repeated-work + plan-adherence"));
        assert!(diag.contains("skill-timeline"));
    }

    #[tokio::test]
    async fn test_power_brainstorm_dispatch() {
        let temp = tempfile::tempdir().unwrap();
        let res = dispatch(
            "power_brainstorm",
            &json!({
                "topic": "Distributed Cache",
                "spec": "Use a 2-tier LRU + Redis write-through cache.",
                "size": "project",
                "open_questions": ["Eviction policy TTL?", "Max memory budget?"]
            }),
            temp.path(),
        )
        .await;
        assert!(res.is_some());
        let output = res.unwrap().unwrap();
        assert!(output.contains("Distributed Cache"));
        assert!(output.contains("Socratic Brainstorming"));
        assert!(output.contains("Persisted design specification"));
        assert!(output.contains("Builder Check"));
        assert!(output.contains("resolve the 2 open design question(s)"));
        let spec_file = crate::tools::minikit::resolve_doc_path(temp.path(), "spec.md");
        let spec_disk = std::fs::read_to_string(spec_file).unwrap();
        assert!(spec_disk.contains("Use a 2-tier LRU + Redis write-through cache."));
    }

    #[tokio::test]
    async fn test_power_plan_and_verify_lifecycle() {
        let temp = tempfile::tempdir().unwrap();
        let plan_res = dispatch(
            "power_plan",
            &json!({
                "topic": "Hero Section",
                "steps": ["1. Search miniblocks hero", "2. Scaffold hero component"]
            }),
            temp.path(),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(plan_res.contains("Activated 2 tactical sub-steps"));

        let verify_res = dispatch("power_verify", &json!({"files": []}), temp.path())
            .await
            .unwrap()
            .unwrap();
        assert!(verify_res.contains("last_verify.json"));
        assert!(temp.path().join(".minicode/plan/last_verify.json").exists());
    }
}

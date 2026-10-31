//! MiniPower autonomous methodology, verification barrier, and worktree execution tools.

use crate::agent::minipower::{
    ImplementerStatus, MiniPowerEngine, MiniPowerTask, ReviewFinding, ReviewSeverity, ReviewStage,
};
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
            description: "Start or persist the design phase for open-ended work (a new product or feature, a redesign, an architecture change) whose requirements or approach are not settled. Inspects live workspace state, sizes ceremony (`QuickTask`, `Spike`, `SmallChange`, `Project`), optionally saves a design specification (`spec`) to `<docs_dir>/core/spec.md` or a visual companion HTML mockup (`visual_mockup`) to `.minicode/brainstorm/`, runs the cold-reader Builder Check (`run_builder_check=true`), and guides `ask_user` / `record_decision` -> `create_plan` / `power_plan`. Do NOT use for small, fully specified changes or bug fixes.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "topic": {
                        "type": "string",
                        "description": "The feature, product or change to design"
                    },
                    "spec": {
                        "type": "string",
                        "description": "Optional agreed design specification or architecture summary to persist to `<docs_dir>/core/spec.md`."
                    },
                    "size": {
                        "type": "string",
                        "description": "Optional ceremony size classification: 'quick_task', 'spike', 'small_change', or 'project'."
                    },
                    "run_builder_check": {
                        "type": "boolean",
                        "description": "If true (or when size='project' and spec is provided), includes the cold-reader Builder Check prompt to surface missing builder decisions in one round."
                    },
                    "visual_mockup": {
                        "type": "string",
                        "description": "Optional HTML wireframe/mockup snippet to write to `.minicode/brainstorm/<slug>.html` for visual inspection via `browser_navigate` / `browser_screenshot`."
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
            description: "Decompose the currently active `[>]` task from `<docs_dir>/core/todo.md` into bite-sized tactical sub-steps (2-5 min each) in `.minicode/plan/task_plan.md` (or initialize a full two-tier plan if no plan exists yet), initialize the `.minicode/sdd/<slug>/` workspace + `progress.md` ledger + per-task briefs, and record up to 5 `review_focus` items (`obra/superpowers` Review Focus). When all tactical sub-steps finish via `update_progress`, the parent task in `todo.md` automatically marks `[x]` and advances.".to_string(),
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
                    "target_files": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Optional list of target file paths created/modified by this plan."
                    },
                    "consumes": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Optional exact signatures/types consumed from earlier tasks (`Interfaces: Consumes`)."
                    },
                    "produces": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Optional exact signatures/types produced for later tasks (`Interfaces: Produces`)."
                    },
                    "test_command": {
                        "type": "string",
                        "description": "Optional verification test command for this plan's tasks."
                    },
                    "save_to_docs": {
                        "type": "boolean",
                        "description": "Whether to persist architectural notes into `<docs_dir>/core/implementation.md`. Defaults to true."
                    }
                },
                "required": ["topic"]
            }),
        },
        ToolSchema {
            name: "power_review".to_string(),
            description: "Run an adversarial two-stage code review and static AST/security audit on workspace changes: Stage 1 (`SpecCompliance`: acceptance criteria met, 'spec is a vision document'), Stage 2 (`CodeQuality`: zero unwraps/panics, security, error handling, test falsifiability, performance), or `re-review` (scoped fix-diff verification with a 5-round circuit breaker). Writes a `.minicode/sdd/<slug>/review-<base>..<head>.diff` package and updates the SDD ledger.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "staged_only": {
                        "type": "boolean",
                        "description": "If true, only review staged changes (--cached). If false, review all uncommitted changes. Defaults to false."
                    },
                    "task_name": {
                        "type": "string",
                        "description": "Optional task ID or title being reviewed (defaults to active `[>]` step in `task_plan.md`)."
                    },
                    "stage": {
                        "type": "string",
                        "description": "Optional review stage: 'spec' (Stage 1), 'quality' (Stage 2), 'both' (default), or 're-review' (scoped fix verification)."
                    },
                    "base_sha": {
                        "type": "string",
                        "description": "Optional base commit SHA for the review package."
                    },
                    "head_sha": {
                        "type": "string",
                        "description": "Optional head commit SHA for the review package."
                    },
                    "acceptance_criteria": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Optional acceptance criteria to verify during Stage 1 Spec Compliance."
                    },
                    "findings": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Optional prior review findings to verify during a scoped `re-review` round."
                    },
                    "fix_round": {
                        "type": "integer",
                        "description": "Optional fix-loop round number (1..=5). Trips the circuit breaker at round 5."
                    }
                }
            }),
        },
        ToolSchema {
            name: "power_verify".to_string(),
            description: "Run the 4-Gate Pre-Completion Verification Barrier programmatically: Gate 1 (AST Syntax & Compiler Integrity), Gate 2 (Reproducer & Regression Test Suite), Gate 3 (Structural Integrity & Git Merge Conflicts), Gate 4 (Diff Sanity & Secret Leaks). Records verification status in `.minicode/plan/last_verify.json` and the SDD `progress.md` ledger. Call when all plan steps are completed before `archive_plan`.".to_string(),
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
            description: "Execute an isolated mutating task inside an ephemeral Git worktree sandbox via a specialized subagent using a structured MiniPower implementer brief (`.minicode/sdd/<slug>/task-<id>-brief.md`), with 4-status report parsing (`DONE`, `DONE_WITH_CONCERNS`, `NEEDS_CONTEXT`, `BLOCKED`), automated verification, SDD `progress.md` ledger updates, and safe 3-way merge arbitration.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "task": {
                        "type": "string",
                        "description": "Task instructions or feature description for the isolated worker"
                    },
                    "id": {
                        "type": "string",
                        "description": "Optional task identifier (e.g. 'task-1')."
                    },
                    "target_files": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Optional list of target files for the implementer brief and worktree file boundaries."
                    },
                    "acceptance_criteria": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Optional acceptance criteria for the implementer brief."
                    },
                    "consumes": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Optional exact signatures/types consumed from earlier tasks."
                    },
                    "produces": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Optional exact signatures/types produced for later tasks."
                    },
                    "test_command": {
                        "type": "string",
                        "description": "Optional verification test command for the worktree worker."
                    },
                    "fix_findings": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Optional list of blocking review findings to fix in this worktree round."
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
            description: "Run a 7-dimension MiniPower session & workspace forensics audit (`diagnosing-minipower`): streams `.minicode/sessions/*.jsonl`, `.minicode/plan/task_plan.md`, `.minicode/sdd/*/progress.md`, `.minicode/decisions.md` (`[D1]..[DN]`), `.minicode/plan/last_verify.json`, and `<docs_dir>/core/todo.md` across `skill-timeline`, `plan-adherence`, `request-conflicts`, `quality-evidence`, `repeated-work`, `stumbles`, and `cost-and-time` with exact `path:line` evidence.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "complaint": {
                        "type": "string",
                        "description": "Optional symptom or dimension to focus the forensic audit (e.g. 'stumbles', 'repeated-work', 'plan-adherence', 'quality-evidence', 'cost-and-time')."
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

fn build_active_minipower_task(
    workspace_root: &Path,
    args: &serde_json::Value,
    default_desc: &str,
) -> MiniPowerTask {
    let active_step = std::fs::read_to_string(workspace_root.join(".minicode/plan/task_plan.md"))
        .ok()
        .and_then(|content| {
            content
                .lines()
                .map(str::trim)
                .find(|l| l.contains("[>]"))
                .map(crate::context::working_memory::WorkingMemory::sanitize_step_title)
        });

    let title = opt_str(args, "task_name")
        .or_else(|| opt_str(args, "title"))
        .map(String::from)
        .or_else(|| active_step.clone())
        .unwrap_or_else(|| {
            default_desc
                .lines()
                .next()
                .unwrap_or("Active Task")
                .chars()
                .take(72)
                .collect()
        });

    let id = opt_str(args, "id")
        .or_else(|| opt_str(args, "task_id"))
        .map(String::from)
        .unwrap_or_else(|| MiniPowerEngine::sanitize_slug(&title));

    let target_files = opt_string_array(args, "target_files").unwrap_or_default();
    let mut acceptance_criteria = opt_string_array(args, "acceptance_criteria").unwrap_or_default();
    if acceptance_criteria.is_empty() {
        if let Some(step) = active_step {
            acceptance_criteria.push(format!("Fulfill active plan step: {}", step));
        } else {
            acceptance_criteria.push("Satisfy task specification with passing tests".to_string());
        }
    }

    MiniPowerTask {
        id,
        title,
        description: default_desc.to_string(),
        target_files,
        acceptance_criteria,
        test_command: opt_str(args, "test_command").map(String::from),
        completed: false,
        consumes: opt_string_array(args, "consumes").unwrap_or_default(),
        produces: opt_string_array(args, "produces").unwrap_or_default(),
    }
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
                    &["topic", "title", "domain", "goal", "prompt", "idea"],
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

                let resolved_spec = crate::tools::minikit::resolve_doc_path(workspace_root, "spec.md");
                let mut rel_spec_path = resolved_spec
                    .strip_prefix(workspace_root)
                    .unwrap_or(&resolved_spec)
                    .display()
                    .to_string();

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

                if let Some(mockup_html) = opt_str(args, "visual_mockup") {
                    if !mockup_html.trim().is_empty() {
                        let bs_dir = workspace_root.join(".minicode/brainstorm");
                        let _ = std::fs::create_dir_all(&bs_dir);
                        let slug = MiniPowerEngine::sanitize_slug(topic);
                        let mockup_path = bs_dir.join(format!("{}.html", slug));
                        if std::fs::write(&mockup_path, mockup_html).is_ok() {
                            prompt.push_str(&format!(
                                "\n\n🎨 **Visual Companion Mockup Saved:** `{}` (inspect via `browser_navigate` / `browser_screenshot`).",
                                mockup_path.display()
                            ));
                        }
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
                let target_files = opt_string_array(args, "target_files").unwrap_or_default();
                let consumes = opt_string_array(args, "consumes").unwrap_or_default();
                let produces = opt_string_array(args, "produces").unwrap_or_default();
                let test_command = opt_str(args, "test_command").map(String::from);
                let save_to_docs = opt_bool(args, "save_to_docs", true);
                let mut plan = MiniPowerEngine::format_plan_prompt(workspace_root, topic);

                let sdd_dir = MiniPowerEngine::ensure_sdd_workspace(workspace_root, Some(topic));
                let _ = MiniPowerEngine::append_sdd_ledger(
                    workspace_root,
                    Some(topic),
                    &format!("Plan initialized: {} ({} steps)", topic, steps.len()),
                );

                if !review_focus.is_empty() {
                    plan.push_str("\n\n### 🎯 Review Focus (Implied Inputs & Failure Modes Pinned by Tests):\n");
                    for (idx, item) in review_focus.iter().take(5).enumerate() {
                        plan.push_str(&format!("{}. {}\n", idx + 1, item));
                        let _ = MiniPowerEngine::append_sdd_ledger(
                            workspace_root,
                            Some(topic),
                            &format!("Review Focus {}: {}", idx + 1, item),
                        );
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

                    for (idx, step_raw) in steps.iter().enumerate() {
                        let clean_step =
                            crate::context::working_memory::WorkingMemory::sanitize_step_title(
                                step_raw,
                            );
                        let mp_task = MiniPowerTask {
                            id: format!("{}", idx + 1),
                            title: clean_step.clone(),
                            description: format!("Plan `{}` — Step {}: {}", topic, idx + 1, clean_step),
                            target_files: target_files.clone(),
                            acceptance_criteria: vec![clean_step],
                            test_command: test_command.clone(),
                            completed: false,
                            consumes: consumes.clone(),
                            produces: produces.clone(),
                        };
                        let _ = MiniPowerEngine::write_task_brief(
                            workspace_root,
                            Some(topic),
                            &mp_task,
                            None,
                        );
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
                    let rel_sdd = sdd_dir
                        .strip_prefix(workspace_root)
                        .unwrap_or(&sdd_dir)
                        .display();
                    plan.push_str(&format!(
                        "\n\n✔ Activated {} tactical sub-steps in `.minicode/plan/task_plan.md` (SDD workspace & task briefs ready at `{}`).\n► Active Sub-Step: [>] \"{}\". Complete each sub-step with `update_progress(step=\"active\", status=\"completed\")` or dispatch via `power_worktree_task`.{}",
                        steps.len(),
                        rel_sdd,
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
                let stage_arg = opt_str(args, "stage").unwrap_or("both").to_ascii_lowercase();
                let fix_round = opt_u64(args, "fix_round").unwrap_or(1).clamp(1, 10) as u8;
                let base_sha = opt_str(args, "base_sha");
                let head_sha = opt_str(args, "head_sha");

                let report = GitReviewer::review_workspace(workspace_root, staged_only).await?;
                let mut formatted = GitReviewer::format_report(&report);

                let mp_task =
                    build_active_minipower_task(workspace_root, args, "Workspace diff review");
                let diff_summary = if report.findings.is_empty() {
                    format!("Files reviewed: {} (0 static findings)", report.files_audited)
                } else {
                    report
                        .findings
                        .iter()
                        .map(|f| {
                            let loc = f.line_hint.as_deref().unwrap_or(&f.category);
                            format!("[{}] {} — {}: {}", f.severity, loc, f.title, f.details)
                        })
                        .collect::<Vec<_>>()
                        .join("\n")
                };

                if let Ok(pkg_path) = MiniPowerEngine::write_review_package(
                    workspace_root,
                    None,
                    base_sha,
                    head_sha,
                    &diff_summary,
                ) {
                    let rel_pkg = pkg_path
                        .strip_prefix(workspace_root)
                        .unwrap_or(&pkg_path)
                        .display();
                    formatted.push_str(&format!("\n📦 **Review Package:** `{}`\n", rel_pkg));
                }

                let prior_findings: Vec<ReviewFinding> = opt_string_array(args, "findings")
                    .unwrap_or_default()
                    .into_iter()
                    .map(|desc| ReviewFinding {
                        severity: ReviewSeverity::Important,
                        description: desc,
                        file: None,
                        line: None,
                    })
                    .collect();

                formatted.push_str("\n---\n");
                if stage_arg == "re-review" || stage_arg == "rereview" || !prior_findings.is_empty()
                {
                    let findings_to_check = if prior_findings.is_empty() {
                        report
                            .findings
                            .iter()
                            .map(|f| ReviewFinding {
                                severity: if f.severity == "HIGH" {
                                    ReviewSeverity::Critical
                                } else if f.severity == "MEDIUM" {
                                    ReviewSeverity::Important
                                } else {
                                    ReviewSeverity::Minor
                                },
                                description: format!("{}: {}", f.title, f.details),
                                file: f.line_hint.clone(),
                                line: None,
                            })
                            .collect::<Vec<_>>()
                    } else {
                        prior_findings
                    };
                    formatted.push_str(&MiniPowerEngine::format_scoped_rereview_prompt(
                        &mp_task,
                        &findings_to_check,
                        &diff_summary,
                        fix_round,
                    ));
                    if !findings_to_check.is_empty() && fix_round < MiniPowerEngine::MAX_FIX_LOOP_ROUNDS {
                        formatted.push_str("\n");
                        formatted.push_str(&MiniPowerEngine::format_fix_prompt(
                            &mp_task,
                            &findings_to_check,
                        ));
                    }
                } else {
                    if stage_arg == "spec" || stage_arg == "both" || stage_arg == "stage1" {
                        formatted.push_str(&MiniPowerEngine::format_reviewer_prompt(
                            &mp_task,
                            &diff_summary,
                            ReviewStage::SpecCompliance,
                        ));
                        formatted.push_str("\n");
                    }
                    if stage_arg == "quality" || stage_arg == "both" || stage_arg == "stage2" {
                        formatted.push_str(&MiniPowerEngine::format_reviewer_prompt(
                            &mp_task,
                            &diff_summary,
                            ReviewStage::CodeQuality,
                        ));
                    }
                }

                let actionable_count = report
                    .findings
                    .iter()
                    .filter(|f| f.severity == "HIGH" || f.severity == "MEDIUM")
                    .count();
                let minor_findings: Vec<String> = report
                    .findings
                    .iter()
                    .filter(|f| f.severity != "HIGH" && f.severity != "MEDIUM")
                    .map(|f| {
                        let loc = f.line_hint.as_deref().unwrap_or(&f.category);
                        format!("{} ({}: {})", loc, f.title, f.details)
                    })
                    .collect();

                if !minor_findings.is_empty() {
                    let _ = MiniPowerEngine::append_sdd_ledger(
                        workspace_root,
                        None,
                        &format!(
                            "Task {}: minor (deferred): {}",
                            mp_task.id,
                            minor_findings.join("; ")
                        ),
                    );
                }
                let _ = MiniPowerEngine::append_sdd_ledger(
                    workspace_root,
                    None,
                    &format!(
                        "Review `{}` (stage={}): {} blocking, {} minor findings",
                        mp_task.title,
                        stage_arg,
                        actionable_count,
                        minor_findings.len()
                    ),
                );

                let wm = crate::context::working_memory::WorkingMemory::new(workspace_root);
                let plan_hint = wm.format_remaining_tasks_hint();
                if !plan_hint.is_empty() {
                    formatted.push_str(&format!("\n{}\n", plan_hint));
                }
                if actionable_count > 0 {
                    formatted.push_str(&format!(
                        "\n► Next Step: Resolve the {} HIGH/MEDIUM review finding(s) above (max {} fix rounds), then re-run `power_review(stage=\"re-review\")` and `power_verify`.\n",
                        actionable_count,
                        MiniPowerEngine::MAX_FIX_LOOP_ROUNDS
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
                let _ = MiniPowerEngine::append_sdd_ledger(
                    workspace_root,
                    None,
                    &format!(
                        "Verification barrier (`power_verify`): passed={} ({} files checked)",
                        report.all_passed,
                        files.len()
                    ),
                );

                let mut out = report.format_report();
                let dox_closeout =
                    crate::context::governance::dox::DoxEngine::execute_closeout_pass(
                        workspace_root,
                        &files,
                        true,
                    );
                if !dox_closeout.touched_dox_chain.is_empty() {
                    out.push('\n');
                    out.push_str(&dox_closeout.to_markdown_report());
                }
                if report.all_passed {
                    let wm = crate::context::working_memory::WorkingMemory::new(workspace_root);
                    let _ = wm.reconcile_verification_tasks();
                    let plan_hint = wm.format_remaining_tasks_hint();
                    if !plan_hint.is_empty() {
                        out.push_str(&format!("\n{}\n", plan_hint));
                    }
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

                let mp_task = build_active_minipower_task(workspace_root, args, task_prompt);
                let brief_path =
                    MiniPowerEngine::write_task_brief(workspace_root, None, &mp_task, None).ok();

                let mut full_worker_prompt = MiniPowerEngine::format_implementer_prompt(&mp_task);
                if let Some(fix_list) = opt_string_array(args, "fix_findings") {
                    if !fix_list.is_empty() {
                        let findings: Vec<ReviewFinding> = fix_list
                            .into_iter()
                            .map(|d| ReviewFinding {
                                severity: ReviewSeverity::Important,
                                description: d,
                                file: None,
                                line: None,
                            })
                            .collect();
                        full_worker_prompt.push_str("\n\n");
                        full_worker_prompt
                            .push_str(&MiniPowerEngine::format_fix_prompt(&mp_task, &findings));
                    }
                }

                let task_item = FanoutTaskItem {
                    task: full_worker_prompt,
                    role: SubagentRole::Coder,
                    workspace_mode: Some(WorkspaceMode::Worktree),
                    max_iterations: Some(15),
                    check_cmd: mp_task.test_command.clone(),
                    todos: mp_task.acceptance_criteria.clone(),
                    file_boundaries: mp_task.target_files.clone(),
                };

                let mut result = FanoutOrchestrator::execute_fanout(
                    workspace_root,
                    vec![task_item],
                    FanoutJoinMode::All,
                    auto_merge,
                    1,
                )
                .await
                .map_err(crate::error::MinicodeError::from)?;

                let status = ImplementerStatus::parse_from_report(&result);
                let _ = MiniPowerEngine::append_sdd_ledger(
                    workspace_root,
                    None,
                    &format!("Task {} (`{}`): status={}", mp_task.id, mp_task.title, status),
                );
                if let Some(bp) = brief_path {
                    let rel_bp = bp.strip_prefix(workspace_root).unwrap_or(&bp).display();
                    result.push_str(&format!(
                        "\n\n📋 **SDD Task Brief:** `{}` | **Implementer Status:** `{}`",
                        rel_bp, status
                    ));
                }

                Ok(result)
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
            &json!({"complaint": "repeated-work"}),
            temp.path(),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(diag.contains("repeated-work"));
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
                "visual_mockup": "<section><h1>Cache Topology</h1></section>",
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
        assert!(output.contains("Visual Companion Mockup Saved"));
        assert!(output.contains("Builder Check"));
        assert!(output.contains("resolve the 2 open design question(s)"));
        let spec_file = crate::tools::minikit::resolve_doc_path(temp.path(), "spec.md");
        let spec_disk = std::fs::read_to_string(spec_file).unwrap();
        assert!(spec_disk.contains("Use a 2-tier LRU + Redis write-through cache."));
        assert!(temp
            .path()
            .join(".minicode/brainstorm/distributed-cache.html")
            .exists());
    }

    #[tokio::test]
    async fn test_power_plan_and_verify_lifecycle() {
        let temp = tempfile::tempdir().unwrap();
        let plan_res = dispatch(
            "power_plan",
            &json!({
                "topic": "Hero Section",
                "steps": ["1. Search miniblocks hero", "2. Scaffold hero component"],
                "review_focus": ["Empty title fallback", "Mobile viewport overflow"]
            }),
            temp.path(),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(plan_res.contains("Activated 2 tactical sub-steps"));
        assert!(temp
            .path()
            .join(".minicode/sdd/hero-section/task-1-brief.md")
            .exists());
        assert!(temp
            .path()
            .join(".minicode/sdd/hero-section/progress.md")
            .exists());

        let review_res = dispatch(
            "power_review",
            &json!({
                "task_name": "Hero Section",
                "stage": "re-review",
                "findings": ["Missing mobile overflow check"],
                "fix_round": 2
            }),
            temp.path(),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(review_res.contains("Scoped Re-Review"));
        assert!(review_res.contains("Fix Round 2/5"));

        let verify_res = dispatch("power_verify", &json!({"files": []}), temp.path())
            .await
            .unwrap()
            .unwrap();
        assert!(verify_res.contains("last_verify.json"));
        assert!(temp.path().join(".minicode/plan/last_verify.json").exists());
    }
}

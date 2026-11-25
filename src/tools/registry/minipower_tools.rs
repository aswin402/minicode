//! MiniPower autonomous methodology, verification barrier, and worktree execution tools.

use crate::agent::minipower::{
    MiniPowerEngine, MiniPowerTask, ReviewFinding, ReviewSeverity, ReviewStage,
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
            description: "Start or persist the design phase for open-ended work (a new product or feature, a redesign, an architecture change) whose requirements or approach are not settled. Extracts settled `[D1..DN]` decisions to prune redundant `open_questions`, scores candidate `approaches` via AST symbol reuse & 4-axis Pareto dominance, persists `<docs_dir>/core/spec.md` + responsive HTML visual comparisons in `.minicode/brainstorm/` (with optional live `MiniBrowse` preview), and optionally dispatches a cold-reader `Architect` gap pass (`deep_audit=true`). Do NOT use for small, fully specified changes or bug fixes.".to_string(),
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
                    "approaches": {
                        "type": "array",
                        "description": "Optional candidate architectural/product approaches (`2..=4` options) to rank via workspace AST symbol reuse & Pareto dominance, emitting ready-to-use `ask_user` trade-off cards and an HTML comparison board.",
                        "items": {
                            "type": "object",
                            "properties": {
                                "id": { "type": "string", "description": "Short identifier (e.g. 'opt_extend_graph')." },
                                "title": { "type": "string", "description": "Concise approach name." },
                                "summary": { "type": "string", "description": "1-2 sentence architectural summary." },
                                "pros": { "type": "array", "items": { "type": "string" } },
                                "cons": { "type": "array", "items": { "type": "string" } },
                                "files_touched": { "type": "array", "items": { "type": "string" } },
                                "complexity": { "type": "number", "description": "Estimated complexity on 1.0..=5.0 (default 2.5)." }
                            },
                            "required": ["title"]
                        }
                    },
                    "run_builder_check": {
                        "type": "boolean",
                        "description": "If true (or when size='project' and spec is provided), includes the cold-reader Builder Check prompt to surface missing builder decisions in one round."
                    },
                    "deep_audit": {
                        "type": "boolean",
                        "description": "If true, dispatches an isolated read-only `Architect` subagent (`<= 4` turns) to audit the proposed design with fresh eyes for unstated assumptions, failure modes, and YAGNI cuts."
                    },
                    "visual_mockup": {
                        "type": "string",
                        "description": "Optional HTML wireframe/mockup snippet to write to `.minicode/brainstorm/<slug>.html` for visual inspection via `MiniBrowse`."
                    },
                    "open_preview": {
                        "type": "boolean",
                        "description": "If true and a desktop GUI display is active, opens the generated `.minicode/brainstorm/<slug>.html` comparison board or visual mockup directly in `MiniBrowse`."
                    },
                    "open_questions": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Optional list of unresolved design questions (questions already answered in `.minicode/decisions.md` or `spec.md` are automatically pruned)."
                    },
                    "branches": {
                        "type": "array",
                        "description": "Optional parallel design exploration branches (octto-style branch tracking). Each branch tracks an independent design dimension (`id`, `scope`, `status`: 'exploring'|'done', optional `question`, optional `finding`). Settled branch findings (`status='done'`) are automatically recorded to `.minicode/decisions.md`.",
                        "items": {
                            "type": "object",
                            "properties": {
                                "id": { "type": "string", "description": "Branch identifier (e.g. 'auth', 'storage', 'ui_layout')." },
                                "scope": { "type": "string", "description": "Design dimension or question scope." },
                                "status": { "type": "string", "enum": ["exploring", "done"], "description": "Whether this design branch is still exploring or settled ('done')." },
                                "question": { "type": "string", "description": "Active question for this branch to ask via `ask_user`." },
                                "finding": { "type": "string", "description": "Settled decision/finding once status='done'." }
                            },
                            "required": ["id", "scope"]
                        }
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
                    },
                    "force": {
                        "type": "boolean",
                        "description": "Activate the plan even if plan review reports blocking structural problems (the override is recorded in `.minicode/decisions.md`). Default false."
                    }
                },
                "required": []
            }),
        },
        ToolSchema {
            name: "power_review".to_string(),
            description: "Run the native MiniReview Engine (deterministic filtering + secret-path guard + language/repo rules from `.minicode/review_rules.json` or `.minireview/rule.json` + AST-clustered parallel MiniPub Reviewers + exact line anchoring). Supports Stage 1 (`SpecCompliance`), Stage 2 (`CodeQuality`), `both` (default), `re-review` (scoped fix-diff verification with a 5-round circuit breaker), `preview=true` (zero-token dry-run), `background` (concise business/PR context), and `analysis=true` (separate Behavior & Impact brief). Writes a `.minicode/sdd/<slug>/review-<base>..<head>.diff` package and updates the SDD ledger.".to_string(),
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
                    "subagent_review": {
                        "type": "boolean",
                        "description": "Optional override for dispatching a dedicated read-only MiniPub `reviewer` subagent on top of the static audit. When omitted, auto-enabled on initial review and fix_round 1 when >= 3 files are audited, and auto-skipped on clean re-reviews (fix_round >= 2 with 0 blocking static findings). Pass true/false to override."
                    },
                    "preview": {
                        "type": "boolean",
                        "description": "If true, runs a zero-token deterministic dry-run listing reviewable files, blocked secret/noise paths, AST file clusters, and resolved language/repo rules without invoking any LLM reviewer. Default false."
                    },
                    "background": {
                        "type": "string",
                        "description": "Optional 1–3 sentence business/PR context (intent, constraints, invariants) injected into the Reviewer subagent prompt."
                    },
                    "analysis": {
                        "type": "boolean",
                        "description": "If true, instructs the Reviewer to include a separate `## Behavior & Impact` brief documenting material observable behavioral/API/config changes even when there are 0 defects. Default false."
                    },
                    "rule_file": {
                        "type": "string",
                        "description": "Optional path to a custom review rules JSON file (defaults to `.minicode/review_rules.json` or `.minireview/rule.json`)."
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
        ToolSchema {
            name: "power_speckit".to_string(),
            description: "Execute SpecKit Specification-Driven Development (SDD) algorithms natively in Rust (`github/spec-kit`): (1) `analyze`: Bipartite/Multipartite `petgraph::DiGraph` Cross-Artifact Traceability & Consistency Analyzer across `prd.md`, `spec.md`, `architecture.md`, `implementation.md`, and `todo.md`; (2) `checklist`: 5-dimension Requirements-Quality Checklist Evaluator ('Unit Tests for Requirements' saved to `core/checklists/requirements.md`); (3) `converge`: Append-Only Implementation Convergence Engine (`✅ CONVERGED` or append-only `## Convergence` delta tasks); (4) `bug`: Scoped 3-Stage Bug Triage State Machine (`assess` -> `fix` with set-difference `## Deviations from Assessment` -> `test` with `VERIFIED | PARTIAL | FAILED`); (5) `assess`: 5-Stage Evidence-Weighted Idea Assessment Scorecard (`intake`..`decide` -> `GO | NEEDS_CLARIFICATION | KILL`).".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["analyze", "checklist", "converge", "bug", "assess", "issues"],
                        "description": "SpecKit SDD operation to execute (`analyze`, `checklist`, `converge`, `bug`, `assess`, or `issues`). Defaults to `analyze`."
                    },
                    "topic": {
                        "type": "string",
                        "description": "Bug title or Idea Assessment topic (used when `action` is `bug` or `assess`)."
                    },
                    "stage": {
                        "type": "string",
                        "description": "Sub-stage for `bug` (`assess` | `fix` | `test`) or `assess` (`intake` | `research` | `define` | `shape` | `decide`)."
                    },
                    "root_cause": {
                        "type": "string",
                        "description": "Root cause diagnosis for `bug` (`assess` stage) or markdown body for `assess`."
                    },
                    "reproduction_command": {
                        "type": "string",
                        "description": "Reproduction test command for `bug` (`assess` stage)."
                    },
                    "target_files": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Assessed target file scope for `bug` (`assess` stage)."
                    },
                    "modified_files": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Actual modified files for `bug` (`fix` stage) used to compute `ModifiedFiles \\ AssessedFiles` deviations."
                    },
                    "repro_exercised": {
                        "type": "boolean",
                        "description": "Whether the original bug reproduction was exercised during `bug` (`test` stage). If false, verdict is downgraded to `PARTIAL`."
                    },
                    "tests_passed": {
                        "type": "boolean",
                        "description": "Whether the test suite passed during `bug` (`test` stage)."
                    },
                    "problem_validity": {
                        "type": "string",
                        "description": "Rating (`strong` | `adequate` | `weak` | `unknown`) for `assess`."
                    },
                    "evidence_strength": {
                        "type": "string",
                        "description": "Rating (`strong` | `adequate` | `weak` | `unknown`) for `assess`."
                    },
                    "concept_viability": {
                        "type": "string",
                        "description": "Rating (`strong` | `adequate` | `weak` | `unknown`) for `assess`."
                    },
                    "counter_evidence_documented": {
                        "type": "boolean",
                        "description": "Whether counter-evidence was explicitly documented during `assess`."
                    }
                }
            }),
        },
        ToolSchema {
            name: "project_doctor".to_string(),
            description: "Run MiniDoctor — universal native Rust codebase health diagnostic engine across React/Next.js, Hono/TS/JS/Prisma/Zod/Pino, Python/FastAPI/SQLAlchemy, Rust/Tokio/Ratatui, Flutter/Dart, and Go. Computes a 0–100 health score with category sub-scores, detects AST anti-patterns, security defects, dead code & duplicate blocks, and saves a prioritized human + AI checklist to `.minicode/minidoctor/NEEDEDFIX.md`.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "scope": {
                        "type": "string",
                        "enum": ["changed", "full"],
                        "description": "Scan scope: `changed` (git diff / working tree files) or `full` (entire workspace). Defaults to `full`."
                    },
                    "diff_ref": {
                        "type": "string",
                        "description": "Optional git reference for `changed` scope (e.g. `main`, `HEAD~1`)."
                    },
                    "category": {
                        "type": "string",
                        "enum": ["all", "correctness", "security", "performance", "architecture", "dead_code", "maintainability"],
                        "description": "Optional category filter for returned findings. Defaults to `all`."
                    },
                    "explain_rule": {
                        "type": "string",
                        "description": "Optional rule ID (e.g. `fastapi/sync-in-async-route`) to inspect detailed rationale and remediation instead of running a scan."
                    },
                    "write_neededfix": {
                        "type": "boolean",
                        "description": "Whether to write/update `.minicode/minidoctor/NEEDEDFIX.md`, `latest.json`, and `history.jsonl`. Defaults to true."
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
/// Dispatches a dedicated read-only MiniPub `reviewer` subagent for `power_review(subagent_review=true)`.
/// The reviewer receives the static audit as context and inspects the diff itself; its report is
/// appended to the review output. Failures are reported inline instead of failing the review.
/// Ranks workspace files for the Reviewer subagent using `CodeGraph` Personalized PageRank
/// seeded on files modified or created relative to the `WorkingMemory` cryptographic scaffold
/// baseline (`scaffold_baseline.json`), falling back to concise modified `WorkspaceWalker`
/// source files for non-AST markup/stylesheets without hardcoded path or lockfile lists.
pub(crate) fn rank_reviewer_workspace_files(workspace_root: &Path, limit: usize) -> Vec<String> {
    let wm = crate::context::working_memory::WorkingMemory::new(workspace_root);
    let walker_rel = crate::context::walker::WorkspaceWalker::new(workspace_root)
        .max_depth(5)
        .collect_relative_files();

    let mut modified_abs_paths: Vec<std::path::PathBuf> = Vec::new();
    let mut modified_rel_with_size: Vec<(String, u64)> = Vec::new();
    let mut modified_rel_set: std::collections::HashSet<String> = std::collections::HashSet::new();

    for rel in &walker_rel {
        let norm = rel.replace('\\', "/");
        if norm.starts_with("minikit_docs/") {
            continue;
        }
        let abs = workspace_root.join(rel);
        if wm.is_unmodified_scaffold_file(&abs) {
            continue;
        }
        let byte_len = std::fs::metadata(&abs).map(|m| m.len()).unwrap_or(0);
        if byte_len == 0 {
            continue;
        }
        modified_abs_paths.push(abs);
        modified_rel_set.insert(norm.clone());
        modified_rel_with_size.push((norm, byte_len));
    }

    let mut ranked: Vec<String> = Vec::with_capacity(limit);
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    if let Ok(graph) = crate::context::graph::CodeGraph::load_or_build(workspace_root) {
        let pr_files = graph.compute_pagerank(&modified_abs_paths);
        // Pass 1: Modified/authored AST source files ordered by Personalized PageRank centrality
        for (abs_path, _score) in &pr_files {
            if ranked.len() >= limit {
                break;
            }
            let Ok(rel) = abs_path.strip_prefix(workspace_root) else {
                continue;
            };
            let norm = rel.to_string_lossy().replace('\\', "/");
            if norm.starts_with("minikit_docs/") || !modified_rel_set.contains(&norm) {
                continue;
            }
            if seen.insert(norm.clone()) {
                ranked.push(norm);
            }
        }
        // Pass 2: High-centrality architectural neighbor AST files from `CodeGraph`
        for (abs_path, _score) in &pr_files {
            if ranked.len() >= limit {
                break;
            }
            let Ok(rel) = abs_path.strip_prefix(workspace_root) else {
                continue;
            };
            let norm = rel.to_string_lossy().replace('\\', "/");
            if norm.starts_with("minikit_docs/") {
                continue;
            }
            if seen.insert(norm.clone()) {
                ranked.push(norm);
            }
        }
    }

    // Pass 3: Concise non-AST authored files (e.g. `.css`, `.html`, `.toml`, `<= 64 KiB` so
    // machine-generated lockfiles/bundles never crowd out human-authored source files).
    if ranked.len() < limit {
        modified_rel_with_size.sort_by_key(|(_, sz)| *sz);
        for (norm, sz) in modified_rel_with_size {
            if ranked.len() >= limit {
                break;
            }
            if sz <= 65_536 && seen.insert(norm.clone()) {
                ranked.push(norm);
            }
        }
    }

    ranked
}

/// Extracts changed file paths from a unified diff (`diff --git a/... b/<path>` or `+++ b/<path>`)
/// so the Reviewer subagent only inspects files actually touched in this review scope instead of
/// dumping 30 unrelated workspace files into its prompt.
fn extract_changed_files_from_diff(raw_diff: &str) -> Vec<String> {
    let mut files = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for line in raw_diff.lines() {
        let candidate = if let Some(rest) = line.strip_prefix("+++ b/") {
            Some(rest.trim())
        } else if let Some(rest) = line.strip_prefix("+++ ") {
            let trimmed = rest.trim();
            if trimmed == "/dev/null" || trimmed.is_empty() {
                None
            } else {
                Some(trimmed.strip_prefix("b/").unwrap_or(trimmed))
            }
        } else if let Some(rest) = line.strip_prefix("diff --git ") {
            rest.split(" b/").nth(1).map(str::trim)
        } else {
            None
        };
        if let Some(path) = candidate {
            let clean = path.trim_matches('"').replace('\\', "/");
            if !clean.is_empty() && clean != "/dev/null" && seen.insert(clean.clone()) {
                files.push(clean);
            }
        }
    }
    files
}

/// Returns true iff `workspace_root` is a git repo with an existing `HEAD` commit AND non-empty
/// tracked diff output (`git diff --cached --name-only` when `staged_only`, else `git diff HEAD --name-only`).
/// Prevents 0-commit repositories (whose `power_review` synthetic diff includes untracked files) from
/// falsely telling the Reviewer subagent to run `git_diff` or truncating its file list to the diff cap.
fn workspace_has_tracked_git_diff(workspace_root: &Path, staged_only: bool) -> bool {
    if !workspace_root.join(".git").exists() {
        return false;
    }
    let has_head = std::process::Command::new("git")
        .args(["rev-parse", "--verify", "HEAD"])
        .current_dir(workspace_root)
        .output()
        .is_ok_and(|o| o.status.success());
    if !has_head {
        return false;
    }
    let args: &[&str] = if staged_only {
        &["diff", "--cached", "--name-only"]
    } else {
        &["diff", "HEAD", "--name-only"]
    };
    std::process::Command::new("git")
        .args(args)
        .current_dir(workspace_root)
        .output()
        .is_ok_and(|o| o.status.success() && !String::from_utf8_lossy(&o.stdout).trim().is_empty())
}

/// Lists untracked authored source files from `git status --porcelain --untracked-files=all`
/// so fresh 0-commit repositories (where `git diff` is empty) still scope the Reviewer to real
/// workspace files and tell it not to waste a turn calling `git_diff`.
fn collect_untracked_workspace_files(workspace_root: &Path, limit: usize) -> Vec<String> {
    let output = match std::process::Command::new("git")
        .args(["status", "--porcelain", "--untracked-files=all"])
        .current_dir(workspace_root)
        .output()
    {
        Ok(o) if o.status.success() => o,
        _ => return Vec::new(),
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut files = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for line in stdout.lines() {
        let Some(rest) = line.strip_prefix("?? ") else {
            continue;
        };
        let clean = rest.trim().trim_matches('"').replace('\\', "/");
        if clean.is_empty()
            || clean.starts_with(".minicode/")
            || clean.starts_with("node_modules/")
            || clean.starts_with("dist/")
            || clean.starts_with("target/")
            || clean.ends_with(".lock")
            || clean.ends_with("-lock.json")
            || clean.ends_with(".png")
            || clean.ends_with(".jpg")
            || clean.ends_with(".svg")
        {
            continue;
        }
        if seen.insert(clean.clone()) {
            files.push(clean);
        }
    }
    // Prioritize authored source code before docs/dotfiles so large repos never crowd out code files.
    files.sort_by_key(|p| {
        let rank = if p.starts_with("minikit_docs/") || p.starts_with('.') {
            2u8
        } else if p.ends_with(".md") || p.ends_with(".json") {
            1u8
        } else {
            0u8
        };
        (rank, p.clone())
    });
    files.truncate(limit);
    files
}

struct ReviewerRunContext<'a> {
    stage: &'a str,
    staged_only: bool,
    is_rereview: bool,
    static_report: &'a str,
    diff_excerpt: &'a str,
    target_files: &'a [String],
    has_tracked_diff: bool,
    background: Option<&'a str>,
    analysis: bool,
    rules_prompt: &'a str,
    cluster_label: Option<&'a str>,
}

/// Filters a unified git diff down to hunks belonging to `allowed_files` (or returns the full diff
/// if `allowed_files` is empty), capped cleanly at `max_chars` for fast inline Reviewer grounding.
fn extract_scoped_diff_excerpt(
    raw_diff: &str,
    allowed_files: &[String],
    max_chars: usize,
) -> String {
    if raw_diff.trim().is_empty() || max_chars == 0 {
        return String::new();
    }
    if allowed_files.is_empty() {
        return raw_diff.chars().take(max_chars).collect();
    }
    let allowed: std::collections::HashSet<String> =
        allowed_files.iter().map(|f| f.replace('\\', "/")).collect();
    let mut out = String::new();
    let mut include_block = false;
    for line in raw_diff.lines() {
        if line.starts_with("diff --git ") {
            let path = line
                .split_whitespace()
                .last()
                .unwrap_or("")
                .trim_start_matches("b/")
                .replace('\\', "/");
            include_block = allowed.contains(&path);
        } else if let Some(stripped) = line.strip_prefix("+++ b/") {
            let path = stripped.trim().replace('\\', "/");
            if !include_block && allowed.contains(&path) {
                include_block = true;
            }
        }
        if include_block {
            if out.len() + line.len() + 1 > max_chars {
                out.push_str("\n[... diff excerpt truncated — inspect remaining lines with batch_read ...]\n");
                break;
            }
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

/// Decides max parallel AST clusters for `power_review`: only fans out on initial reviews whose
/// filtered diff is large enough (`>= MIN_DIFF_LINES_FOR_FANOUT`) to justify parallel workers.
fn review_max_clusters(is_rereview: bool, raw_audited_diff: &str) -> usize {
    if is_rereview {
        return 1;
    }
    let diff_lines = raw_audited_diff.lines().count();
    if diff_lines >= crate::git::mini_review::ReviewFileClusterer::MIN_DIFF_LINES_FOR_FANOUT {
        crate::git::mini_review::ReviewFileClusterer::DEFAULT_MAX_CLUSTERS
    } else {
        1
    }
}

async fn run_reviewer_subagent(workspace_root: &Path, ctx: &ReviewerRunContext<'_>) -> String {
    let spec_path = crate::tools::minikit::resolve_doc_path(workspace_root, "spec.md");
    let spec_rel = spec_path
        .strip_prefix(workspace_root)
        .unwrap_or(&spec_path)
        .display()
        .to_string();
    let is_git_repo = workspace_root.join(".git").exists();
    let untracked_files = if is_git_repo && !ctx.has_tracked_diff && ctx.target_files.is_empty() {
        collect_untracked_workspace_files(workspace_root, 30)
    } else {
        Vec::new()
    };
    let all_untracked_only = is_git_repo && !ctx.has_tracked_diff && !untracked_files.is_empty();
    let candidate_files: Vec<String> = if !ctx.target_files.is_empty() {
        ctx.target_files.to_vec()
    } else if !untracked_files.is_empty() {
        untracked_files
    } else {
        rank_reviewer_workspace_files(workspace_root, 30)
    };
    let files_hint = if candidate_files.is_empty() {
        String::new()
    } else {
        format!(
            "\n### Workspace Files to Inspect (100% coverage required)\n{}\n",
            candidate_files
                .iter()
                .take(30)
                .map(|f| format!("- `{}`", f))
                .collect::<Vec<_>>()
                .join("\n")
        )
    };
    let has_inline_diff = !ctx.diff_excerpt.trim().is_empty();
    let step_one = if has_inline_diff {
        "1. Review the inline unified diff below FIRST; use a single `batch_read` (and targeted `grep_search` for callers) only where surrounding file context is needed. Do NOT waste turns running git status/diff or shell commands.".to_string()
    } else if !is_git_repo {
        "1. NOTE: This workspace has NO `.git` repository — do **NOT** call `git_diff` or git commands. Use `batch_read` / `read_file` directly on the workspace source files listed below.".to_string()
    } else if all_untracked_only {
        "1. NOTE: This is an initial repository where changes are untracked (`git diff` is empty) — do **NOT** waste a turn running `git_diff`. Inspect the untracked source files listed below directly with `batch_read` / `read_file`.".to_string()
    } else {
        format!(
            "1. Inspect the {} changes with `git_diff` (and `batch_read` / `read_file` for context).",
            if ctx.staged_only {
                "staged"
            } else {
                "uncommitted"
            }
        )
    };
    let diff_section = if has_inline_diff {
        format!(
            "\n### Scoped Unified Diff\n```diff\n{}\n```\n",
            ctx.diff_excerpt.trim()
        )
    } else {
        String::new()
    };
    let static_excerpt: String = ctx.static_report.chars().take(3500).collect();
    let scoped_rereview_note = if ctx.is_rereview {
        "\n- SCOPED RE-REVIEW ONLY: Inspect ONLY the listed prior findings and verify whether the fix diff introduced any regression. Do NOT invent new refactoring or test-extraction requests outside the listed findings; finish in <= 3 turns."
    } else {
        ""
    };
    let background_section = ctx
        .background
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|bg| format!("\n### Business Context\n{bg}\n"))
        .unwrap_or_default();
    let cluster_section = ctx
        .cluster_label
        .map(|lbl| {
            format!(
                "\n### Assigned AST Review Cluster\nScope your inspection strictly to cluster `{lbl}` (inspect every listed file; do not waste turns reviewing unrelated clusters).\n"
            )
        })
        .unwrap_or_default();
    let rules_section = if ctx.rules_prompt.trim().is_empty() {
        String::new()
    } else {
        format!("\n{}\n", ctx.rules_prompt.trim())
    };
    let behavior_section = if ctx.analysis {
        format!(
            "\n{}\n",
            crate::git::mini_review::ReviewPrecisionPrompts::BEHAVIOR_IMPACT_CONTRACT
        )
    } else {
        String::new()
    };
    let precision_section =
        crate::git::mini_review::ReviewPrecisionPrompts::ASYMMETRIC_FACT_CHECK_CONTRACT;
    let tool_budget_hint = if ctx.is_rereview {
        3
    } else if ctx.cluster_label.is_some() {
        4
    } else {
        5
    };
    let prompt = format!(
        "You are the dedicated read-only code reviewer for this workspace. Review stage: `{stage}`.\n\
         {step_one}\n\
         2. Check them against the spec (`{spec}`, if it exists) and `.minicode/decisions.md`.\n\
         3. Verify the static audit findings below are real; do not repeat false positives, do NOT run shell commands or inspect `.env`/secret files, and do NOT re-run slow build suites.\n\
         4. Complete your inspection concisely within <= {tool_budget_hint} turns using `batch_read`/`read_file`/`grep_search`/`locate_symbol` and report only concrete problems (correctness, missing edge cases, security, spec gaps) with file:line, \
            most severe first. Do not modify any files.{scoped_rereview_note}\n\
         End with a line `STATUS: DONE` if the changes are acceptable, otherwise `STATUS: DONE_WITH_CONCERNS`.\n\
         {background_section}{cluster_section}{files_hint}{rules_section}\n{precision_section}{behavior_section}{diff_section}\n\
         ### Static audit (context)\n{static_excerpt}",
        stage = ctx.stage,
        step_one = step_one,
        spec = spec_rel,
        tool_budget_hint = tool_budget_hint,
        scoped_rereview_note = scoped_rereview_note,
        background_section = background_section,
        cluster_section = cluster_section,
        files_hint = files_hint,
        rules_section = rules_section,
        precision_section = precision_section,
        behavior_section = behavior_section,
        diff_section = diff_section,
        static_excerpt = static_excerpt
    );
    let app_cfg = crate::config::Config::load_for_workspace(workspace_root);
    let mut config = crate::agent::subagent::SubagentConfig::for_role_with_minipub_config(
        SubagentRole::Reviewer,
        &app_cfg.agent.minipub,
    );
    // Pin explicit turn cap so dynamic scope scaling and elastic extensions cannot blow a concise
    // code review out to 20-30 turns; strip browser_* and shell execution tools (`exec_cmd`,
    // `minitask`) so read-only Reviewers stay fast (batch_read/grep_search only) and cannot
    // bypass the Secret-Path Guard via `cat .env`.
    let turn_cap = config.max_turns.min(tool_budget_hint);
    let reviewer_token_floor = if ctx.is_rereview { 45_000 } else { 70_000 };
    let reviewer_token_budget = config.token_budget.max(reviewer_token_floor);
    config = config
        .with_explicit_max_turns(Some(turn_cap))
        .with_explicit_token_budget(Some(reviewer_token_budget));
    config.tool_whitelist.retain(|t| {
        !t.starts_with("browser_")
            && t != "exec_cmd"
            && t != "run_command"
            && t != "minitask"
            && t != "shell"
    });
    match crate::agent::orchestrator::MultiAgentOrchestrator::delegate_with_config(
        workspace_root,
        &prompt,
        false,
        None,
        Some(config),
    )
    .await
    {
        Ok(result) => format!(
            "\n### 🧑‍⚖️ Reviewer Subagent Report\n{}\n",
            crate::agent::orchestrator::MultiAgentOrchestrator::format_result(&result)
        ),
        Err(e) => format!("\n! Reviewer subagent could not run: {}\n", e),
    }
}

async fn collect_review_preview_candidates(
    workspace_root: &Path,
    staged_only: bool,
    base_sha: Option<&str>,
    head_sha: Option<&str>,
) -> (String, Vec<String>) {
    let git = GitService::new(workspace_root.to_path_buf());
    if git.is_git_repo().await {
        if let Some(base) = base_sha.map(str::trim).filter(|s| !s.is_empty()) {
            let head = head_sha
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or("HEAD");
            if let Ok((_, files)) = git.diff_range(base, head).await {
                if !files.is_empty() {
                    return (format!("range {base}..{head}"), files);
                }
            }
        }
        if let Ok(st) = git.get_status().await {
            let mut files = if staged_only {
                st.staged
            } else {
                let mut all = st.staged;
                all.extend(st.unstaged);
                all.extend(
                    st.untracked
                        .into_iter()
                        .filter(|rel| workspace_root.join(rel).is_file()),
                );
                all
            };
            files.sort();
            files.dedup();
            if !files.is_empty() {
                let mode = if staged_only { "staged" } else { "workspace" };
                return (mode.to_string(), files);
            }
        }
        if !staged_only {
            if let Ok((_, head_files)) = git.diff_range("HEAD~1", "HEAD").await {
                if !head_files.is_empty() {
                    return ("HEAD~1..HEAD".to_string(), head_files);
                }
            }
        }
    }
    (
        "scan".to_string(),
        GitReviewer::discover_workspace_source_files(workspace_root),
    )
}

/// Durable file tracking how many Reviewer MiniPub subagents have already executed during the
/// current active plan. Unlike `last_review.json` (which is invalidated on every source edit),
/// this counter persists across fix edits and is only reset on `create_plan` / `power_plan` /
/// `archive_plan`, guaranteeing that repeated `power_review(stage="both")` calls cannot cause an
/// unbounded reviewer loop.
const REVIEW_SUBAGENT_RUNS_FILE: &str = ".minicode/plan/review_subagent_runs.json";

fn read_review_subagent_runs(workspace_root: &Path) -> u8 {
    std::fs::read_to_string(workspace_root.join(REVIEW_SUBAGENT_RUNS_FILE))
        .ok()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
        .and_then(|v| v.get("runs").and_then(|n| n.as_u64()))
        .map(|n| n.min(u8::MAX as u64) as u8)
        .unwrap_or(0)
}

fn increment_review_subagent_runs(workspace_root: &Path) -> u8 {
    let next = read_review_subagent_runs(workspace_root).saturating_add(1);
    let path = workspace_root.join(REVIEW_SUBAGENT_RUNS_FILE);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let payload = json!({
        "runs": next,
        "timestamp": chrono::Utc::now().to_rfc3339(),
    });
    let _ = std::fs::write(
        path,
        serde_json::to_string_pretty(&payload).unwrap_or_default(),
    );
    next
}

/// Decides whether `power_review` should dispatch a Reviewer MiniPub subagent.
/// Prevents endless multi-round perfection loops once static review passes with 0 blocking
/// findings on fix round >= 2 (unless the caller explicitly passed `subagent_review: true`).
fn should_spawn_power_review_subagent(
    explicit: Option<bool>,
    files_audited: usize,
    actionable_count: usize,
    is_rereview: bool,
    fix_round: u8,
    has_live_runtime: bool,
) -> bool {
    match explicit {
        Some(b) => b,
        None => {
            has_live_runtime
                && files_audited >= 3
                && !(is_rereview && actionable_count == 0 && fix_round >= 2)
        }
    }
}

/// Dispatches an isolated read-only `Architect` subagent (`max_turns <= 4`) as a fresh-eye
/// pre-planning gap analyst (`Metis` lens) to surface unstated assumptions, failure modes, and YAGNI cuts.
async fn run_architect_gap_subagent(
    workspace_root: &Path,
    topic: &str,
    spec_excerpt: Option<&str>,
    spec_rel: &str,
) -> String {
    let spec_block = spec_excerpt
        .map(|s| {
            let trimmed: String = s.chars().take(3200).collect();
            format!("\n### Proposed Design Summary\n{trimmed}\n")
        })
        .unwrap_or_default();
    let prompt = format!(
        "You are an isolated read-only Staff Architect reviewing the proposed design for `{topic}` with fresh eyes before implementation starts.\n\
         1. Inspect `.minicode/decisions.md` and `{spec_rel}` (plus at most 2-3 directly relevant source files via `batch_read` / `code_map` if needed; <= 4 tool calls total).\n\
         2. Identify ONLY concrete pre-planning blind spots:\n\
            - Unstated assumptions or boundary invariants that could break at integration time\n\
            - Missing failure-mode / concurrency / state-lifecycle handling\n\
            - Scope creep or YAGNI abstractions that should be cut in favor of reusing existing workspace modules\n\
         3. Keep your report concise (< 220 words), bulleted, and grounded in exact workspace files/symbols. Do not modify any files.\n\
         {spec_block}",
    );
    let app_cfg = crate::config::Config::load_for_workspace(workspace_root);
    let config = crate::agent::subagent::SubagentConfig::for_role_with_minipub_config(
        SubagentRole::Architect,
        &app_cfg.agent.minipub,
    );
    let cap = config.max_turns.min(4);
    let config = config.with_explicit_max_turns(Some(cap));
    match crate::agent::orchestrator::MultiAgentOrchestrator::delegate_with_config(
        workspace_root,
        &prompt,
        false,
        None,
        Some(config),
    )
    .await
    {
        Ok(result) => format!(
            "\n### 🔭 Cold-Reader Architect Gap Pass\n{}\n",
            crate::agent::orchestrator::MultiAgentOrchestrator::format_result(&result)
        ),
        Err(e) => format!("\n! Architect gap pass skipped: {}\n", e),
    }
}

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
                let mut open_questions =
                    opt_string_array(args, "open_questions").unwrap_or_default();

                // Persist and merge parallel design exploration branches, scoped per topic
                // (`.minicode/brainstorm/<topic-slug>/state.json`) so a new topic starts clean.
                let bs_dir = workspace_root
                    .join(".minicode/brainstorm")
                    .join(MiniPowerEngine::sanitize_slug(topic));
                let state_path = bs_dir.join("state.json");
                let mut existing_branches: Vec<serde_json::Value> =
                    std::fs::read_to_string(&state_path)
                        .ok()
                        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
                        .and_then(|v| v.get("branches").and_then(|b| b.as_array()).cloned())
                        .unwrap_or_default();

                if let Some(incoming_branches) = args.get("branches").and_then(|v| v.as_array()) {
                    for b in incoming_branches {
                        let Some(bid) = b.get("id").and_then(|v| v.as_str()).map(str::trim) else {
                            continue;
                        };
                        if bid.is_empty() {
                            continue;
                        }
                        let scope = b
                            .get("scope")
                            .and_then(|v| v.as_str())
                            .unwrap_or(bid)
                            .trim();
                        let status = b
                            .get("status")
                            .and_then(|v| v.as_str())
                            .unwrap_or("exploring")
                            .trim();
                        let question = b
                            .get("question")
                            .and_then(|v| v.as_str())
                            .map(str::trim)
                            .filter(|s| !s.is_empty());
                        let finding = b
                            .get("finding")
                            .and_then(|v| v.as_str())
                            .map(str::trim)
                            .filter(|s| !s.is_empty());

                        if status.eq_ignore_ascii_case("done") {
                            if let Some(f) = finding {
                                let _ = crate::agent::decisions::upsert_decision(
                                    workspace_root,
                                    &format!("{} ({})", scope, bid),
                                    f,
                                    Some("Settled design branch in power_brainstorm"),
                                );
                            }
                        } else if let Some(q) = question {
                            let formatted_q = format!("[{}] {}", bid, q);
                            if !open_questions.iter().any(|oq| oq == q || oq == &formatted_q) {
                                open_questions.push(formatted_q);
                            }
                        }

                        let entry_json = serde_json::json!({
                            "id": bid,
                            "scope": scope,
                            "status": if status.eq_ignore_ascii_case("done") { "done" } else { "exploring" },
                            "question": question,
                            "finding": finding,
                        });
                        if let Some(existing) = existing_branches
                            .iter_mut()
                            .find(|eb| eb.get("id").and_then(|v| v.as_str()) == Some(bid))
                        {
                            *existing = entry_json;
                        } else {
                            existing_branches.push(entry_json);
                        }
                    }

                    let state_json = serde_json::json!({
                        "topic": topic,
                        "branches": existing_branches,
                        "updated_at": chrono::Utc::now().to_rfc3339(),
                    });
                    let serialized = serde_json::to_string_pretty(&state_json)?;
                    std::fs::create_dir_all(&bs_dir)
                        .and_then(|_| std::fs::write(&state_path, serialized))
                        .map_err(|e| crate::error::ToolError::ExecutionFailed(format!(
                            "power_brainstorm: failed to persist branch state to `{}`: {}",
                            state_path.display(),
                            e
                        )))?;
                }

                // Set-algebraic pruning of open questions already settled in `.minicode/decisions.md` or `spec.md`
                let settled_decisions =
                    crate::agent::minipower::brainstorm_algebra::extract_settled_decisions(
                        workspace_root,
                    );
                let q_filter =
                    crate::agent::minipower::brainstorm_algebra::filter_redundant_questions(
                        &open_questions,
                        &settled_decisions,
                    );
                open_questions = q_filter.active_questions;

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

                if !q_filter.pruned_questions.is_empty() {
                    prompt.push_str("\n<pruned_settled_questions>\n");
                    prompt.push_str(
                        "### ✓ Pruned Already-Settled Questions (Do NOT re-ask via `ask_user`)\n",
                    );
                    for pq in &q_filter.pruned_questions {
                        prompt.push_str(&format!(
                            "- ~~{}~~ → already settled by **[{}]** _{}_ (`containment={:.0}%`)\n",
                            pq.question,
                            pq.matched_id,
                            pq.matched_heading,
                            pq.containment * 100.0
                        ));
                    }
                    prompt.push_str("</pruned_settled_questions>\n");
                }

                // AST-grounded multi-criteria & 4-axis Pareto trade-off evaluation of candidate approaches
                let approach_candidates =
                    crate::agent::minipower::brainstorm_algebra::parse_approach_candidates(args);
                let approach_eval =
                    crate::agent::minipower::brainstorm_algebra::evaluate_approaches_against_ast(
                        workspace_root,
                        &approach_candidates,
                    );
                if !approach_eval.ranked.is_empty() {
                    prompt.push_str(&approach_eval.format_markdown());
                }

                if !existing_branches.is_empty() {
                    let mut exploring_count = 0usize;
                    let mut done_count = 0usize;
                    let mut branch_lines = Vec::new();
                    for b in &existing_branches {
                        let bid = b.get("id").and_then(|v| v.as_str()).unwrap_or("branch");
                        let scope = b.get("scope").and_then(|v| v.as_str()).unwrap_or(bid);
                        let status = b
                            .get("status")
                            .and_then(|v| v.as_str())
                            .unwrap_or("exploring");
                        if status == "done" {
                            done_count += 1;
                            let finding = b
                                .get("finding")
                                .and_then(|v| v.as_str())
                                .unwrap_or("settled");
                            branch_lines.push(format!(
                                "- [✓] `{}` ({}): **DONE** — {}",
                                bid, scope, finding
                            ));
                        } else {
                            exploring_count += 1;
                            let q = b
                                .get("question")
                                .and_then(|v| v.as_str())
                                .unwrap_or("pending exploration");
                            branch_lines.push(format!(
                                "- [●] `{}` ({}): **EXPLORING** — {}",
                                bid, scope, q
                            ));
                        }
                    }
                    prompt.push_str(&format!(
                        "\n<brainstorm_branches active=\"{}\" settled=\"{}\">\n### 🌿 Design Exploration Branches ({} exploring, {} settled)\n{}\n</brainstorm_branches>\n",
                        exploring_count,
                        done_count,
                        exploring_count,
                        done_count,
                        branch_lines.join("\n")
                    ));
                }
                let is_project = ceremony_size == crate::agent::minipower::CeremonySize::Project;
                let want_builder_check = opt_bool(args, "run_builder_check", is_project);
                let enforce_strict_guard =
                    opt_bool(args, "write_guard", !open_questions.is_empty());
                let _ = MiniPowerEngine::set_planning_phase_state(
                    workspace_root,
                    &crate::agent::minipower::PlanningPhaseState::DiscoveryWriteGuard {
                        topic: topic.to_string(),
                        open_questions: open_questions.clone(),
                        enforce_strict: enforce_strict_guard,
                        updated_at: chrono::Utc::now().to_rfc3339(),
                    },
                );

                let resolved_spec = crate::tools::minikit::resolve_doc_path(workspace_root, "spec.md");
                let mut rel_spec_path = resolved_spec
                    .strip_prefix(workspace_root)
                    .unwrap_or(&resolved_spec)
                    .display()
                    .to_string();

                let spec_input = opt_str(args, "spec").filter(|s| !s.trim().is_empty());
                if is_project || spec_input.is_some() {
                    let _ = crate::tools::minikit::sync::MiniKitSyncEngine::ensure_core_docs_for_plan(
                        workspace_root,
                        topic,
                        &[],
                        spec_input,
                        None,
                        None,
                        None,
                        &[],
                    );
                }
                if let Some(spec_text) = spec_input {
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

                let explicit_mockup =
                    opt_str(args, "visual_mockup").filter(|m| !m.trim().is_empty());
                let rendered_companion_html: Option<String> = explicit_mockup
                    .map(String::from)
                    .or_else(|| {
                        (approach_eval.ranked.len() >= 2).then(|| {
                            approach_eval.render_comparison_html(
                                topic,
                                &settled_decisions,
                                &open_questions,
                            )
                        })
                    });

                if let Some(ref html_doc) = rendered_companion_html {
                    let bs_root = workspace_root.join(".minicode/brainstorm");
                    let _ = std::fs::create_dir_all(&bs_root);
                    let slug = MiniPowerEngine::sanitize_slug(topic);
                    let mockup_path = bs_root.join(format!("{}.html", slug));
                    if std::fs::write(&mockup_path, html_doc).is_ok() {
                        let want_preview = opt_bool(args, "open_preview", explicit_mockup.is_some());
                        let mut preview_note = String::new();
                        if want_preview
                            && crate::agent::minipower::brainstorm_algebra::has_interactive_gui_display()
                        {
                            if let Ok(abs_path) = std::fs::canonicalize(&mockup_path) {
                                if let Ok(file_url) = url::Url::from_file_path(&abs_path) {
                                    if crate::tools::browser::BrowserController::navigate_and_snapshot(
                                        file_url.as_str(),
                                        crate::tools::browser::BrowserMode::Gui,
                                        workspace_root,
                                    )
                                    .await
                                    .is_ok()
                                    {
                                        preview_note =
                                            " — opened live in `MiniBrowse` companion window".to_string();
                                    }
                                }
                            }
                        }
                        prompt.push_str(&format!(
                            "\n\n🎨 **Visual Companion Mockup Saved:** `{}`{} (inspect via `browser_navigate` / `browser_screenshot`).",
                            mockup_path.display(),
                            preview_note
                        ));
                    }
                }

                if want_builder_check {
                    prompt.push_str("\n\n");
                    prompt.push_str(&MiniPowerEngine::format_builder_check_prompt(
                        topic,
                        &rel_spec_path,
                    ));
                }

                let want_deep_audit = match args.get("deep_audit").and_then(|v| v.as_bool()) {
                    Some(explicit) => explicit,
                    None => is_project && spec_input.is_some(),
                };
                if want_deep_audit && crate::agent::subagent::has_live_non_mock_runtime() {
                    prompt.push_str(
                        &run_architect_gap_subagent(
                            workspace_root,
                            topic,
                            spec_input,
                            &rel_spec_path,
                        )
                        .await,
                    );
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
                // Hierarchical planning: without an explicit topic, power_plan decomposes the
                // active Tier-1 `[>]` step (e.g. "Create hero section") into Tier-2 sub-steps.
                let topic_owned: String = match get_str_with_aliases(
                    args,
                    &["topic", "goal", "prompt", "task"],
                )
                .map(str::trim)
                .filter(|s| !s.is_empty())
                {
                    Some(t) => t.to_string(),
                    None => crate::context::working_memory::WorkingMemory::new(workspace_root)
                        .read_active_plan_summary()
                        .and_then(|s| s.active_task)
                        .map(|t| {
                            crate::context::working_memory::WorkingMemory::sanitize_step_title(&t)
                        })
                        .filter(|t| !t.is_empty())
                        .ok_or_else(|| crate::error::ToolError::InvalidArguments {
                            name: "power_plan".to_string(),
                            reason: "Missing `topic` and no active `[>]` plan step to decompose; call `create_plan` first or pass `topic`.".to_string(),
                        })?,
                };
                let topic = topic_owned.as_str();
                let steps = opt_string_array(args, "steps")
                    .or_else(|| opt_string_array(args, "tasks"))
                    .unwrap_or_default();
                let review_focus = opt_string_array(args, "review_focus").unwrap_or_default();
                let target_files = opt_string_array(args, "target_files").unwrap_or_default();
                let consumes = opt_string_array(args, "consumes").unwrap_or_default();
                let produces = opt_string_array(args, "produces").unwrap_or_default();
                let test_command = opt_str(args, "test_command").map(String::from);
                let save_to_docs = opt_bool(args, "save_to_docs", true);

                // Plan review runs before anything is written; structural blockers keep the
                // discovery write-guard active unless `force=true`.
                let review_report = if steps.is_empty() {
                    None
                } else {
                    let input = crate::agent::minipower::plan_review::PlanReviewInput {
                        topic,
                        steps: &steps,
                        target_files: &target_files,
                        test_command: test_command.as_deref(),
                    };
                    match MiniPowerEngine::review_plan(
                        workspace_root,
                        input,
                        opt_bool(args, "force", false),
                    )
                    .await
                    {
                        Ok(report) => Some(report),
                        Err(blocked) => return Ok(blocked),
                    }
                };
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
                    let _ = std::fs::remove_file(
                        workspace_root.join(".minicode/plan/last_review.json"),
                    );
                    let _ = std::fs::remove_file(workspace_root.join(REVIEW_SUBAGENT_RUNS_FILE));
                    let has_existing_roadmap = wm.read_active_plan_summary().is_some();
                    if has_existing_roadmap {
                        wm.init_tactical_subplan(topic, &steps)?;
                        if let Some(mut existing_dag) =
                            crate::context::memory::task_dag::PlanDag::load(workspace_root)
                        {
                            if let Some(node) = existing_dag
                                .nodes
                                .iter_mut()
                                .find(|n| n.title.eq_ignore_ascii_case(topic))
                            {
                                node.substeps = steps
                                    .iter()
                                    .map(|s| {
                                        crate::context::working_memory::WorkingMemory::sanitize_step_title(s)
                                    })
                                    .collect();
                                let _ = existing_dag.save(workspace_root);
                            }
                        }
                    } else {
                        let step_substeps: Vec<Vec<String>> = steps
                            .iter()
                            .map(|raw| {
                                crate::context::working_memory::WorkingMemory::synthesize_compound_verification_substeps(raw)
                            })
                            .collect();
                        wm.init_two_tier_plan_with_substeps(
                            topic,
                            &steps,
                            &step_substeps,
                            None,
                            None,
                        )?;
                        if let Ok(dag) = crate::context::memory::task_dag::PlanDag::from_plan_args(
                            topic,
                            args,
                            &steps,
                            &step_substeps,
                        ) {
                            let _ = dag.save(workspace_root);
                        }
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
                    let rel_sdd = sdd_dir
                        .strip_prefix(workspace_root)
                        .unwrap_or(&sdd_dir)
                        .display();
                    let triad_note = review_report
                        .as_ref()
                        .map(|r| {
                            MiniPowerEngine::mark_plan_validated(
                                workspace_root,
                                topic,
                                steps.len(),
                                r,
                            )
                        })
                        .unwrap_or_default();
                    let _ = crate::agent::boulder::BoulderState::sync_from_workspace(
                        workspace_root,
                        false,
                        3,
                    );
                    plan.push_str(&format!(
                        "\n\n✔ Activated {} tactical sub-steps in `.minicode/plan/task_plan.md` (SDD workspace & task briefs ready at `{}`).{}\n► Active Sub-Step: [>] \"{}\". Complete each sub-step with `update_progress(step=\"active\", status=\"completed\")` or dispatch via `power_worktree_task`.",
                        steps.len(),
                        rel_sdd,
                        triad_note,
                        first_step,
                    ));
                } else {
                    let _ = MiniPowerEngine::set_planning_phase_state(
                        workspace_root,
                        &crate::agent::minipower::PlanningPhaseState::Unlocked,
                    );
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
                let preview_only = opt_bool(args, "preview", false);
                let background = opt_str(args, "background");
                let analysis = opt_bool(args, "analysis", false);
                let explicit_rule_path = opt_str(args, "rule_file").map(Path::new);

                if preview_only {
                    let (mode_label, candidates) = collect_review_preview_candidates(
                        workspace_root,
                        staged_only,
                        base_sha,
                        head_sha,
                    )
                    .await;
                    let preview_report = crate::git::mini_review::MiniReviewEngine::build_preview(
                        workspace_root,
                        &mode_label,
                        &candidates,
                        explicit_rule_path,
                    );
                    return Ok(crate::git::mini_review::MiniReviewEngine::format_preview(
                        &preview_report,
                        crate::git::mini_review::ReviewAudience::Human,
                    ));
                }

                let rule_engine = crate::git::mini_review::ReviewRuleEngine::load(
                    workspace_root,
                    explicit_rule_path,
                );

                let (report, raw_audited_diff) = GitReviewer::review_workspace_with_range(
                    workspace_root,
                    staged_only,
                    base_sha,
                    head_sha,
                )
                .await?;
                let anchored_static =
                    crate::git::mini_review::CommentAnchorEngine::anchor_static_report(
                        workspace_root,
                        &report,
                        &raw_audited_diff,
                    );
                let mut formatted =
                    crate::git::mini_review::CommentAnchorEngine::format_for_audience(
                        &report,
                        &anchored_static,
                        crate::git::mini_review::ReviewAudience::Human,
                    );
                if let Some(src) = rule_engine.config_source() {
                    formatted.push_str(&format!("\n📜 **Active Repo Review Rules:** `{}`\n", src));
                }

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
                let package_body = if raw_audited_diff.trim().is_empty() {
                    diff_summary.clone()
                } else {
                    let capped_diff: String = raw_audited_diff.chars().take(65_536).collect();
                    format!("{diff_summary}\n\n```diff\n{capped_diff}\n```")
                };

                if let Ok(pkg_path) = MiniPowerEngine::write_review_package(
                    workspace_root,
                    None,
                    base_sha,
                    head_sha,
                    &package_body,
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

                let actionable_count = report
                    .findings
                    .iter()
                    .filter(|f| f.severity == "HIGH" || f.severity == "MEDIUM")
                    .count();

                let mut reviewer_rubric = String::new();
                let mut implementer_fix_prompt = String::new();
                let is_rereview =
                    stage_arg == "re-review" || stage_arg == "rereview" || !prior_findings.is_empty();
                if is_rereview {
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
                    reviewer_rubric.push_str(&MiniPowerEngine::format_scoped_rereview_prompt(
                        &mp_task,
                        &findings_to_check,
                        &diff_summary,
                        fix_round,
                    ));
                    if !findings_to_check.is_empty()
                        && fix_round < MiniPowerEngine::MAX_FIX_LOOP_ROUNDS
                    {
                        // Keep the implementer's Fix-Loop Contract out of `reviewer_rubric` so the
                        // read-only Reviewer subagent is never instructed to edit files or re-run tests.
                        implementer_fix_prompt.push('\n');
                        implementer_fix_prompt.push_str(&MiniPowerEngine::format_fix_prompt(
                            &mp_task,
                            &findings_to_check,
                        ));
                    }
                } else {
                    if stage_arg == "spec" || stage_arg == "both" || stage_arg == "stage1" {
                        reviewer_rubric.push_str(&MiniPowerEngine::format_reviewer_prompt(
                            &mp_task,
                            &diff_summary,
                            ReviewStage::SpecCompliance,
                        ));
                        reviewer_rubric.push('\n');
                    }
                    if stage_arg == "quality" || stage_arg == "both" || stage_arg == "stage2" {
                        reviewer_rubric.push_str(&MiniPowerEngine::format_reviewer_prompt(
                            &mp_task,
                            &diff_summary,
                            ReviewStage::CodeQuality,
                        ));
                    }
                }

                let explicit_single_stage = opt_str(args, "stage")
                    .is_some_and(|s| !s.eq_ignore_ascii_case("both"));
                formatted.push_str("\n---\n");
                if is_rereview || actionable_count > 0 || explicit_single_stage {
                    formatted.push_str(&reviewer_rubric);
                } else {
                    formatted.push_str(
                        "✔ **Two-Stage Review Gate (`SpecCompliance` + `CodeQuality`)**: 0 blocking findings across audited files.\n",
                    );
                }

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

                let subagent_static_and_rubric = if formatted.contains(&reviewer_rubric) {
                    formatted.clone()
                } else {
                    format!("{formatted}\n---\n{reviewer_rubric}")
                };
                if !implementer_fix_prompt.is_empty() {
                    formatted.push_str(&implementer_fix_prompt);
                }

                let review_dir = workspace_root.join(".minicode/plan");
                let _ = std::fs::create_dir_all(&review_dir);

                let explicit_subagent = args.get("subagent_review").and_then(|v| v.as_bool());
                let has_live_runtime = crate::agent::subagent::has_live_non_mock_runtime();
                let existing_active_reviewer =
                    crate::agent::subagent::try_get_global_subagent_pool()
                        .is_some_and(|p| p.has_active_worker_with_role(SubagentRole::Reviewer));
                let prior_subagent_runs = read_review_subagent_runs(workspace_root);
                let effective_is_rereview = is_rereview || prior_subagent_runs >= 1;
                let effective_fix_round = fix_round.max(prior_subagent_runs);
                let should_run_subagent_review = should_spawn_power_review_subagent(
                    explicit_subagent,
                    report.files_audited,
                    actionable_count,
                    effective_is_rereview,
                    effective_fix_round,
                    has_live_runtime,
                );
                if existing_active_reviewer {
                    formatted.push_str(
                        "\n### 🧑‍⚖️ Reviewer Subagent Report\n⏳ **Reviewer Subagent Already Running**: An async Reviewer MiniPub worker is still executing in the background. Do NOT spawn a duplicate reviewer or archive the plan yet — wait for its `<completed_minipub_boomerang>` notification.\n",
                    );
                } else if should_run_subagent_review {
                    let subagent_payload = subagent_static_and_rubric;
                    let has_tracked_diff =
                        workspace_has_tracked_git_diff(workspace_root, staged_only);
                    let diff_changed_files = if has_tracked_diff {
                        extract_changed_files_from_diff(&raw_audited_diff)
                    } else {
                        Vec::new()
                    };
                    let raw_target_files: Vec<String> = if !mp_task.target_files.is_empty() {
                        mp_task.target_files.clone()
                    } else if !diff_changed_files.is_empty() {
                        diff_changed_files
                    } else if workspace_root.join(".git").exists() && !has_tracked_diff {
                        collect_untracked_workspace_files(workspace_root, 30)
                    } else {
                        rank_reviewer_workspace_files(workspace_root, 30)
                    };

                    // Deterministic secret-path & noise gate before sending files to LLM reviewers
                    let selection = rule_engine.filter_files(&raw_target_files);
                    let blocked_secrets = selection
                        .excluded_files
                        .iter()
                        .filter(|e| {
                            e.reason == crate::git::mini_review::ExclusionReason::SecretPath
                        })
                        .count();
                    if blocked_secrets > 0 {
                        formatted.push_str(&format!(
                            "\n🔒 **Secret-Path Guard:** Deterministically excluded {} credential/secret path(s) from LLM review.\n",
                            blocked_secrets
                        ));
                    }

                    let max_clusters = review_max_clusters(effective_is_rereview, &raw_audited_diff);
                    let clusters = crate::git::mini_review::ReviewFileClusterer::cluster_files(
                        workspace_root,
                        &selection.reviewable_files,
                        crate::git::mini_review::ReviewFileClusterer::DEFAULT_MAX_CLUSTER_SIZE,
                        max_clusters,
                    );

                    if clusters.len() > 1 {
                        formatted.push_str(&format!(
                            "\n🧩 **AST Parallel Cluster Review:** Partitioned {} files into {} cohesive cluster(s).\n",
                            selection.reviewable_files.len(),
                            clusters.len()
                        ));
                        let futs = clusters.iter().map(|cluster| {
                            let rules_prompt =
                                rule_engine.format_rules_for_prompt(&cluster.files, 2400);
                            let diff_excerpt = extract_scoped_diff_excerpt(
                                &raw_audited_diff,
                                &cluster.files,
                                10_000,
                            );
                            let label = format!("#{} ({})", cluster.id, cluster.label);
                            let subagent_payload_ref = &subagent_payload;
                            let stage_ref = &stage_arg;
                            async move {
                                let ctx = ReviewerRunContext {
                                    stage: stage_ref,
                                    staged_only,
                                    is_rereview: effective_is_rereview,
                                    static_report: subagent_payload_ref,
                                    diff_excerpt: &diff_excerpt,
                                    target_files: &cluster.files,
                                    has_tracked_diff,
                                    background,
                                    analysis,
                                    rules_prompt: &rules_prompt,
                                    cluster_label: Some(&label),
                                };
                                let report_part = run_reviewer_subagent(workspace_root, &ctx).await;
                                format!(
                                    "\n#### Cluster #{} — `{}` ({} file(s))\n{}",
                                    cluster.id,
                                    cluster.label,
                                    cluster.files.len(),
                                    report_part
                                )
                            }
                        });
                        let cluster_reports = futures::future::join_all(futs).await;
                        for part in cluster_reports {
                            formatted.push_str(&part);
                        }
                    } else {
                        let single_files = clusters
                            .first()
                            .map(|c| c.files.as_slice())
                            .unwrap_or(&selection.reviewable_files);
                        let rules_prompt = rule_engine.format_rules_for_prompt(single_files, 2400);
                        let diff_excerpt = extract_scoped_diff_excerpt(
                            &raw_audited_diff,
                            single_files,
                            12_000,
                        );
                        let ctx = ReviewerRunContext {
                            stage: &stage_arg,
                            staged_only,
                            is_rereview: effective_is_rereview,
                            static_report: &subagent_payload,
                            diff_excerpt: &diff_excerpt,
                            target_files: single_files,
                            has_tracked_diff,
                            background,
                            analysis,
                            rules_prompt: &rules_prompt,
                            cluster_label: None,
                        };
                        formatted.push_str(&run_reviewer_subagent(workspace_root, &ctx).await);
                    }
                    let _ = increment_review_subagent_runs(workspace_root);
                } else if explicit_subagent.is_none()
                    && has_live_runtime
                    && report.files_audited >= 3
                    && effective_is_rereview
                    && actionable_count == 0
                    && effective_fix_round >= 2
                {
                    formatted.push_str(&format!(
                        "\n✔ **Re-Review Gate Complete (fix_round={effective_fix_round}, prior_reviewer_runs={prior_subagent_runs})**: Static audit passed with 0 HIGH/MEDIUM findings — skipping another Reviewer subagent pass to prevent endless nitpick loops (pass `subagent_review=true` to force). Proceed directly to `power_verify` / `archive_plan`.\n"
                    ));
                }
                let reviewer_still_running = crate::agent::subagent::try_get_global_subagent_pool()
                    .is_some_and(|p| p.has_active_worker_with_role(SubagentRole::Reviewer))
                    || (should_run_subagent_review
                        && crate::agent::subagent::try_get_global_subagent_pool()
                            .is_some_and(|p| p.active_worker_count() > 0));
                let review_state = json!({
                    "passed": actionable_count == 0 && !reviewer_still_running,
                    "pending_reviewer": reviewer_still_running,
                    "actionable_findings": actionable_count,
                    "minor_findings": minor_findings.len(),
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                });
                let _ = std::fs::write(
                    review_dir.join("last_review.json"),
                    serde_json::to_string_pretty(&review_state).unwrap_or_default(),
                );
                if actionable_count > 0 {
                    let wm = crate::context::working_memory::WorkingMemory::new(workspace_root);
                    let plan_hint = wm.format_remaining_tasks_hint();
                    if !plan_hint.is_empty() && !plan_hint.contains("call `archive_plan` directly")
                    {
                        formatted.push_str(&format!("\n{}\n", plan_hint));
                    }
                    if reviewer_still_running {
                        formatted.push_str(&format!(
                            "\n► Next Step: Resolve the {} HIGH/MEDIUM static review finding(s) above (and incorporate findings once the active async Reviewer MiniPub subagent finishes; max {} fix rounds), then re-run `power_review(stage=\"re-review\")` and `power_verify`.\n",
                            actionable_count,
                            MiniPowerEngine::MAX_FIX_LOOP_ROUNDS
                        ));
                    } else {
                        formatted.push_str(&format!(
                            "\n► Next Step: Resolve the {} HIGH/MEDIUM review finding(s) above (max {} fix rounds), then re-run `power_review(stage=\"re-review\")` and `power_verify`.\n",
                            actionable_count,
                            MiniPowerEngine::MAX_FIX_LOOP_ROUNDS
                        ));
                    }
                } else if reviewer_still_running {
                    let wm = crate::context::working_memory::WorkingMemory::new(workspace_root);
                    let plan_hint = wm.format_remaining_tasks_hint();
                    if !plan_hint.is_empty() && !plan_hint.contains("call `archive_plan` directly")
                    {
                        formatted.push_str(&format!("\n{}\n", plan_hint));
                    }
                    formatted.push_str(
                        "\n► Next Step (Async Reviewer PENDING — Do NOT declare final completion yet): Static pre-check passed with 0 HIGH/MEDIUM findings, and the deep LLM Reviewer MiniPub subagent is still running asynchronously in `minitask`. Do **NOT** call `archive_plan` and do **NOT** claim code review is complete yet — briefly inform the user that the Reviewer subagent is running in the background and pause; `minicode` will automatically wake you via Boomerang notification as soon as the Reviewer completes.\n",
                    );
                } else {
                    let wm = crate::context::working_memory::WorkingMemory::new(workspace_root);
                    let _ = wm.reconcile_verification_tasks();
                    let plan_hint = wm.format_remaining_tasks_hint();
                    if !plan_hint.is_empty() {
                        formatted.push_str(&format!("\n{}\n", plan_hint));
                    }
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

                let mut files = match explicit_files {
                    Some(fl) if !fl.is_empty() => fl,
                    _ => {
                        let git = GitService::new(workspace_root.to_path_buf());
                        let git_files = if git.is_git_repo().await {
                            if let Ok(st) = git.get_status().await {
                                let mut all = st.staged;
                                all.extend(st.unstaged);
                                all.extend(
                                    st.untracked
                                        .into_iter()
                                        .filter(|rel| workspace_root.join(rel).is_file()),
                                );
                                all.sort();
                                all.dedup();
                                all
                            } else {
                                vec![]
                            }
                        } else {
                            vec![]
                        };
                        let filtered_git: Vec<String> = git_files
                            .into_iter()
                            .filter(|rel| {
                                !VerificationBarrier::is_ignored_artifact_or_vendor_path(
                                    workspace_root,
                                    rel,
                                )
                            })
                            .collect();
                        if filtered_git.is_empty() {
                            GitReviewer::discover_workspace_source_files(workspace_root)
                        } else {
                            filtered_git
                        }
                    }
                };
                files.retain(|rel| {
                    !VerificationBarrier::is_ignored_artifact_or_vendor_path(workspace_root, rel)
                });

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
                let _ = crate::agent::boulder::BoulderState::sync_from_workspace(
                    workspace_root,
                    false,
                    3,
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
                    wm.record_verification_tool_activity("power_verify", args, true);
                    let _ = wm.reconcile_verification_tasks();
                    let plan_hint = wm.format_remaining_tasks_hint();
                    if !plan_hint.is_empty() {
                        out.push_str(&format!("\n{}\n", plan_hint));
                    }
                    let review_passed = std::fs::read_to_string(verify_dir.join("last_review.json"))
                        .ok()
                        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
                        .and_then(|v| v.get("passed").and_then(|b| b.as_bool()))
                        .unwrap_or(false);
                    if review_passed {
                        out.push_str(
                            "\n► Next Step: All 4 Verification Gates PASSED (`last_verify.json` recorded) and `power_review` passed. Call `archive_plan` if all plan steps are complete.\n",
                        );
                    } else {
                        out.push_str(
                            "\n► Next Step: All 4 Verification Gates PASSED (`last_verify.json` recorded). Run `power_review` now to audit diff quality before calling `archive_plan`.\n",
                        );
                    }
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
                    category: None,
                    load_skills: Vec::new(),
                    workspace_mode: Some(WorkspaceMode::Worktree),
                    max_iterations: Some(15),
                    token_budget: None,
                    timeout_secs: None,
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

                // A report without an explicit status is never treated as DONE.
                let status = crate::agent::subagent::types::WorkerReportStatus::parse_explicit(
                    &result,
                )
                .unwrap_or(crate::agent::subagent::types::WorkerReportStatus::DoneWithConcerns)
                .as_str();
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
            let mut diag = MiniPowerEngine::diagnose_workspace(workspace_root, complaint);
            diag.push_str(
                &crate::agent::verification_barrier::SessionQualityTelemetry::format_markdown_report(
                    workspace_root,
                ),
            );
            let caveman_waste =
                crate::context::budget_optimizer::CavemanWasteAnalyzer::analyze(workspace_root, &[]);
            diag.push_str(&caveman_waste.format_markdown());
            Ok(diag)
        }),
        "project_doctor" | "minidoctor" | "doctor" => Some({
            if let Some(rule_id) = opt_str(args, "explain_rule") {
                if let Some(meta) = minidoctor::explain_rule(rule_id) {
                    Ok(format!(
                        "Rule `{}` (stack: {}, category: {}, severity: {})\nSummary: {}\nWhy: {}\nHow to fix: {}",
                        meta.id,
                        meta.stack,
                        meta.category.label(),
                        meta.default_severity.badge(),
                        meta.summary,
                        meta.why,
                        meta.how_to_fix
                    ))
                } else {
                    Ok(format!(
                        "Unknown MiniDoctor rule `{rule_id}`. Available rules: {}",
                        minidoctor::list_rules()
                            .iter()
                            .map(|r| r.id)
                            .collect::<Vec<_>>()
                            .join(", ")
                    ))
                }
            } else {
                let scope_str = opt_str(args, "scope").unwrap_or("full");
                let diff_ref = opt_str(args, "diff_ref").map(String::from);
                let scope = if scope_str.eq_ignore_ascii_case("changed") {
                    minidoctor::ScanScope::Changed(diff_ref)
                } else {
                    minidoctor::ScanScope::Full
                };
                let write_md = args
                    .get("write_neededfix")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);
                let cat_filter = opt_str(args, "category")
                    .map(|c| c.trim().to_ascii_lowercase())
                    .filter(|c| !c.is_empty() && c != "all");
                let ws = workspace_root.to_path_buf();
                let opts = minidoctor::DoctorOptions {
                    scope,
                    run_dead_code: true,
                    run_security: true,
                    write_workspace_artifacts: write_md,
                    export_md_path: None,
                    threshold_override: None,
                    max_files: 2500,
                };
                match tokio::task::spawn_blocking(move || minidoctor::run_doctor(&ws, &opts)).await
                {
                    Ok(Ok(mut rep)) => {
                        if let Some(ref cf) = cat_filter {
                            rep.diagnostics.retain(|d| {
                                let label = format!("{:?}", d.category).to_ascii_lowercase();
                                label == *cf || label.replace('_', "") == cf.replace('_', "")
                            });
                        }
                        Ok(minidoctor::format_agent_compact_summary(&rep, 25))
                    }
                    Ok(Err(e)) => Err(crate::error::MinicodeError::from(
                        ToolError::ExecutionFailed(format!("MiniDoctor failed: {e}")),
                    )),
                    Err(e) => Err(crate::error::MinicodeError::from(
                        ToolError::ExecutionFailed(format!("MiniDoctor task join error: {e}")),
                    )),
                }
            }
        }),
        "power_speckit" | "minipower_speckit" | "speckit" => Some({
            use crate::agent::minipower::speckit::{
                analyze_workspace_artifacts, converge_workspace, evaluate_requirements_checklist,
                execute_bug_workflow_stage, execute_idea_assessment, AssessmentRating,
                AssessmentStage, BugStage, BugStageInput, IdeaAssessmentInput,
            };

            let action = opt_str(args, "action")
                .unwrap_or("analyze")
                .trim()
                .to_ascii_lowercase();
            match action.as_str() {
                "checklist" => {
                    let report = evaluate_requirements_checklist(workspace_root, true);
                    Ok(report.format_markdown())
                }
                "converge" => {
                    let apply = args
                        .get("apply")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(true);
                    let report = converge_workspace(workspace_root, apply);
                    Ok(report.format_markdown())
                }
                "bug" => {
                    let title = opt_str(args, "topic")
                        .or_else(|| opt_str(args, "title"))
                        .unwrap_or("Active Bug Investigation")
                        .to_string();
                    let stage = opt_str(args, "stage").map(BugStage::from_str_name);
                    let input = BugStageInput {
                        title,
                        stage,
                        root_cause: opt_str(args, "root_cause").map(String::from),
                        reproduction_command: opt_str(args, "reproduction_command")
                            .map(String::from),
                        target_files: opt_string_array(args, "target_files").unwrap_or_default(),
                        modified_files: opt_string_array(args, "modified_files").unwrap_or_default(),
                        fix_summary: opt_str(args, "fix_summary")
                            .or_else(|| opt_str(args, "root_cause"))
                            .map(String::from),
                        deviation_rationale: opt_str(args, "deviation_rationale").map(String::from),
                        repro_exercised: args.get("repro_exercised").and_then(|v| v.as_bool()),
                        tests_passed: args.get("tests_passed").and_then(|v| v.as_bool()),
                    };
                    execute_bug_workflow_stage(workspace_root, input)
                        .map(|state| state.format_markdown())
                        .map_err(|e| {
                            crate::error::MinicodeError::from(ToolError::ExecutionFailed(
                                e.to_string(),
                            ))
                        })
                }
                "assess" => {
                    let topic = opt_str(args, "topic")
                        .or_else(|| opt_str(args, "title"))
                        .unwrap_or("Feature Proposal")
                        .to_string();
                    let stage = opt_str(args, "stage").map(AssessmentStage::from_str_name);
                    let input = IdeaAssessmentInput {
                        topic,
                        stage,
                        content: opt_str(args, "content")
                            .or_else(|| opt_str(args, "root_cause"))
                            .map(String::from),
                        problem_validity: opt_str(args, "problem_validity")
                            .map(AssessmentRating::from_str_name),
                        evidence_strength: opt_str(args, "evidence_strength")
                            .map(AssessmentRating::from_str_name),
                        counter_evidence_documented: args
                            .get("counter_evidence_documented")
                            .and_then(|v| v.as_bool()),
                        concept_viability: opt_str(args, "concept_viability")
                            .map(AssessmentRating::from_str_name),
                        assumptions: opt_string_array(args, "assumptions").unwrap_or_default(),
                    };
                    execute_idea_assessment(workspace_root, input)
                        .map(|report| report.format_markdown())
                        .map_err(|e| {
                            crate::error::MinicodeError::from(ToolError::ExecutionFailed(
                                e.to_string(),
                            ))
                        })
                }
                "issues" | "taskstoissues" => {
                    let report = analyze_workspace_artifacts(workspace_root);
                    let issues = report.synthesize_traceable_issues();
                    let mut out = format!(
                        "### 📌 SpecKit Traceable Tasks-to-Issues (`{}` open task(s))\n\n",
                        issues.len()
                    );
                    for issue in &issues {
                        out.push_str(&format!("#### {}\n{}\n", issue.title, issue.body_markdown));
                    }
                    Ok(out)
                }
                _ => {
                    let report = analyze_workspace_artifacts(workspace_root);
                    Ok(report.format_markdown())
                }
            }
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
        assert_eq!(schemas.len(), 8);
        let names: Vec<String> = schemas.into_iter().map(|s| s.name).collect();
        assert!(!names.contains(&"power_status".to_string()));
        assert!(names.contains(&"power_brainstorm".to_string()));
        assert!(names.contains(&"power_plan".to_string()));
        assert!(names.contains(&"power_review".to_string()));
        assert!(names.contains(&"power_verify".to_string()));
        assert!(names.contains(&"power_worktree_task".to_string()));
        assert!(names.contains(&"power_diagnose".to_string()));
        assert!(names.contains(&"power_speckit".to_string()));
        assert!(names.contains(&"project_doctor".to_string()));
    }

    #[tokio::test]
    async fn test_project_doctor_dispatch_and_neededfix_artifact() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(
            temp.path().join("app.py"),
            "import time\nasync def handler():\n    time.sleep(1)\n",
        )
        .unwrap();
        let res = dispatch("project_doctor", &json!({"scope": "full"}), temp.path())
            .await
            .unwrap()
            .unwrap();
        assert!(res.contains("MiniDoctor Health Score:"));
        assert!(res.contains("fastapi/sync-in-async-route"));
        assert!(temp
            .path()
            .join(".minicode/minidoctor/NEEDEDFIX.md")
            .is_file());

        let explain = dispatch(
            "project_doctor",
            &json!({"explain_rule": "fastapi/sync-in-async-route"}),
            temp.path(),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(explain.contains("asyncio.sleep"));
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
    async fn test_power_brainstorm_prunes_settled_questions_and_scores_approaches() {
        let temp = tempfile::tempdir().unwrap();
        let ws = temp.path();
        crate::agent::decisions::upsert_decision(
            ws,
            "Database Storage Engine",
            "Use SQLite with rusqlite WAL mode for local persistence",
            Some("Zero external server setup"),
        )
        .unwrap();

        let out = dispatch(
            "power_brainstorm",
            &json!({
                "topic": "Local Sync Engine",
                "open_questions": [
                    "Should database storage use SQLite persistence?",
                    "Should conflict resolution use Lamport timestamps or CRDTs?"
                ],
                "approaches": [
                    {
                        "id": "opt_crdt",
                        "title": "Full Op-Based CRDT Log",
                        "summary": "Maintain causal op log per field",
                        "pros": ["Automatic offline merge"],
                        "cons": ["Tombstone memory growth", "Complex schema migration"],
                        "files_touched": ["src/sync/crdt.rs", "src/sync/log.rs", "src/sync/gc.rs"],
                        "complexity": 4.2
                    },
                    {
                        "id": "opt_lww",
                        "title": "Hybrid Logical Clock LWW Rows",
                        "summary": "Per-record HLC column inside SQLite tables",
                        "pros": ["Minimal SQLite schema overhead", "Deterministic merge", "Zero tombstones"],
                        "cons": ["Row-level granularity"],
                        "files_touched": ["src/sync/hlc.rs"],
                        "complexity": 1.8
                    }
                ]
            }),
            ws,
        )
        .await
        .unwrap()
        .unwrap();

        assert!(out.contains("<pruned_settled_questions>"));
        assert!(out.contains("[D1]"));
        assert!(out.contains("resolve the 1 open design question(s)"));
        assert!(out.contains("<approach_pareto_matrix>"));
        assert!(out.contains("Hybrid Logical Clock LWW Rows"));
        assert!(out.contains("Ready `ask_user` trade-off options"));
        assert!(ws
            .join(".minicode/brainstorm/local-sync-engine.html")
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

    #[tokio::test]
    async fn test_power_speckit_dispatch() {
        let temp = tempfile::tempdir().unwrap();
        let core_dir = temp.path().join("minikit_docs/core");
        std::fs::create_dir_all(&core_dir).unwrap();
        std::fs::write(
            core_dir.join("spec.md"),
            "# Spec\n## Requirements\n- Must validate JWT tokens in `src/jwt.rs`.\n## Non-Goals\n- OAuth1 is out of scope.\n",
        )
        .unwrap();
        std::fs::write(
            core_dir.join("todo.md"),
            "# Tasks\n- [ ] Validate JWT tokens in `src/jwt.rs`\n",
        )
        .unwrap();

        let analyze_out = dispatch("power_speckit", &json!({"action": "analyze"}), temp.path())
            .await
            .unwrap()
            .unwrap();
        assert!(analyze_out.contains("Cross-Artifact Traceability Analysis"));
        assert!(analyze_out.contains("100%"));

        let checklist_out = dispatch(
            "power_speckit",
            &json!({"action": "checklist"}),
            temp.path(),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(checklist_out.contains("Requirements Quality Checklist"));
        assert!(core_dir.join("checklists/requirements.md").exists());

        let bug_out = dispatch(
            "power_speckit",
            &json!({
                "action": "bug",
                "topic": "Token Expiry Overflow",
                "stage": "test",
                "tests_passed": true,
                "repro_exercised": false
            }),
            temp.path(),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(bug_out.contains("PARTIAL"));

        let assess_out = dispatch(
            "power_speckit",
            &json!({
                "action": "assess",
                "topic": "Zero-Copy Parser",
                "stage": "decide",
                "problem_validity": "strong",
                "evidence_strength": "strong",
                "concept_viability": "adequate",
                "counter_evidence_documented": true
            }),
            temp.path(),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(assess_out.contains("Decision: **`GO`**"));
    }

    #[tokio::test]
    async fn test_power_review_emits_single_plan_status_and_compact_gate_when_clean() {
        let temp = tempfile::tempdir().unwrap();
        let _ = dispatch(
            "power_plan",
            &json!({
                "topic": "Clean Review Check",
                "steps": ["1. Implement clean handler"]
            }),
            temp.path(),
        )
        .await
        .unwrap()
        .unwrap();

        let review_out = dispatch(
            "power_review",
            &json!({"task_name": "Clean Review Check"}),
            temp.path(),
        )
        .await
        .unwrap()
        .unwrap();

        assert_eq!(
            review_out.matches("📋 Plan Status:").count(),
            1,
            "power_review must emit `📋 Plan Status:` once, got:\n{}",
            review_out
        );
        assert!(review_out.contains("Two-Stage Review Gate"));
    }

    #[test]
    fn test_power_review_subagent_gate_prevents_endless_rereview_loops() {
        // Initial review on >= 3 files runs subagent when live runtime is available
        assert!(should_spawn_power_review_subagent(
            None, 5, 1, false, 0, true
        ));
        // Re-review fix_round 1 still runs one concise scoped re-review
        assert!(should_spawn_power_review_subagent(
            None, 5, 0, true, 1, true
        ));
        // Re-review fix_round >= 2 with 0 actionable static findings skips spawning another subagent
        assert!(!should_spawn_power_review_subagent(
            None, 5, 0, true, 2, true
        ));
        assert!(!should_spawn_power_review_subagent(
            None, 5, 0, true, 3, true
        ));
        // Still runs if actionable static findings remain or if explicitly requested
        assert!(should_spawn_power_review_subagent(
            None, 5, 1, true, 2, true
        ));
        assert!(should_spawn_power_review_subagent(
            Some(true),
            5,
            0,
            true,
            3,
            true
        ));
        assert!(!should_spawn_power_review_subagent(
            Some(false),
            5,
            2,
            false,
            0,
            true
        ));
    }

    #[test]
    fn test_extract_changed_files_from_diff_scopes_reviewer_to_touched_files() {
        let diff = "\
diff --git a/src/App.tsx b/src/App.tsx
--- a/src/App.tsx
+++ b/src/App.tsx
@@ -1,3 +1,4 @@
+import { DocsPage } from '@/pages/DocsPage';
diff --git a/src/pages/DocsPage.tsx b/src/pages/DocsPage.tsx
--- /dev/null
+++ b/src/pages/DocsPage.tsx
@@ -0,0 +1,5 @@
+export function DocsPage() { return null; }
diff --git a/old.ts b/old.ts
--- a/old.ts
+++ /dev/null
@@ -1,2 +0,0 @@
-export const x = 1;
";
        let files = extract_changed_files_from_diff(diff);
        assert_eq!(
            files,
            vec![
                "src/App.tsx".to_string(),
                "src/pages/DocsPage.tsx".to_string(),
                "old.ts".to_string()
            ]
        );
    }

    #[test]
    fn test_zero_commit_repo_reports_no_tracked_diff_and_prioritizes_source_files() {
        let temp = tempfile::tempdir().unwrap();
        let init_ok = std::process::Command::new("git")
            .args(["init", "-q"])
            .current_dir(temp.path())
            .status()
            .is_ok_and(|s| s.success());
        if !init_ok {
            return;
        }
        std::fs::create_dir_all(temp.path().join("js")).unwrap();
        std::fs::write(temp.path().join(".gitignore"), "node_modules/\n").unwrap();
        std::fs::write(temp.path().join("README.md"), "# Demo\n").unwrap();
        std::fs::write(temp.path().join("index.html"), "<!doctype html>\n").unwrap();
        std::fs::write(temp.path().join("style.css"), "body {}\n").unwrap();
        std::fs::write(temp.path().join("js/site.js"), "console.log('ok');\n").unwrap();

        // 0-commit git repo must report has_tracked_diff == false so Reviewer skips `git_diff`
        assert!(!workspace_has_tracked_git_diff(temp.path(), false));
        assert!(!workspace_has_tracked_git_diff(temp.path(), true));

        // And untracked file collection must return all authored files with source prioritized before dotfiles
        let untracked = collect_untracked_workspace_files(temp.path(), 30);
        assert!(untracked.contains(&"index.html".to_string()));
        assert!(untracked.contains(&"style.css".to_string()));
        assert!(untracked.contains(&"js/site.js".to_string()));
        assert_eq!(untracked.last().map(String::as_str), Some(".gitignore"));
    }

    #[test]
    fn test_power_review_schema_documents_auto_subagent_behavior() {
        let schemas = get_schemas();
        let review_schema = schemas
            .iter()
            .find(|s| s.name == "power_review")
            .expect("power_review schema");
        let desc = review_schema.parameters["properties"]["subagent_review"]["description"]
            .as_str()
            .unwrap_or("");
        assert!(
            desc.contains("auto-enabled") && !desc.contains("Default false"),
            "subagent_review schema description must match actual default behavior, got: {desc}"
        );
    }

    #[test]
    fn test_durable_review_subagent_runs_caps_repeated_stage_both_calls() {
        let temp = tempfile::tempdir().unwrap();
        let ws = temp.path();

        assert_eq!(read_review_subagent_runs(ws), 0);
        // Round 0: full review spawns subagent
        let prior0 = read_review_subagent_runs(ws);
        assert!(should_spawn_power_review_subagent(
            None,
            10,
            0,
            prior0 >= 1,
            0u8.max(prior0),
            true
        ));
        assert_eq!(increment_review_subagent_runs(ws), 1);

        // Round 1 (even if caller passes stage="both" with fix_round=0): auto-downgraded to scoped re-review, allowed once
        let prior1 = read_review_subagent_runs(ws);
        assert!(prior1 >= 1);
        assert!(should_spawn_power_review_subagent(
            None,
            10,
            0,
            prior1 >= 1,
            0u8.max(prior1),
            true
        ));
        assert_eq!(increment_review_subagent_runs(ws), 2);

        // Round 2+ with 0 actionable static findings: MUST skip spawning reviewer-3 / reviewer-4 even when stage="both" & fix_round=0
        let prior2 = read_review_subagent_runs(ws);
        assert!(!should_spawn_power_review_subagent(
            None,
            10,
            0,
            prior2 >= 1,
            0u8.max(prior2),
            true
        ));
    }

    #[tokio::test]
    async fn test_power_review_preview_blocks_secrets_loads_repo_rules_and_uses_zero_subagents() {
        let temp = tempfile::tempdir().unwrap();
        let ws = temp.path();
        std::fs::create_dir_all(ws.join("src")).unwrap();
        std::fs::create_dir_all(ws.join(".minireview")).unwrap();
        std::fs::write(ws.join("src/auth.rs"), "pub fn verify() -> bool { true }\n").unwrap();
        std::fs::write(ws.join(".env"), "SECRET_KEY=supersecret\n").unwrap();
        std::fs::write(
            ws.join(".minireview/rule.json"),
            r#"{"rules":[{"path":"src/*.rs","rule":"Require constant-time secret checks."}]}"#,
        )
        .unwrap();

        let out = dispatch("power_review", &json!({"preview": true}), ws)
            .await
            .unwrap()
            .unwrap();

        assert!(out.contains("MiniReview Preview"));
        assert!(out.contains("src/auth.rs"));
        assert!(out.contains(".minireview/rule.json"));
        assert!(!out.contains(".env` —") || out.contains("secret/credential"));
        assert_eq!(
            read_review_subagent_runs(ws),
            0,
            "preview=true must never increment reviewer subagent runs"
        );
    }

    #[test]
    fn test_review_max_clusters_gates_fanout_by_diff_size() {
        let small_diff = "+line\n".repeat(120);
        let large_diff = "+line\n".repeat(450);
        assert_eq!(review_max_clusters(false, &small_diff), 1);
        assert_eq!(
            review_max_clusters(false, &large_diff),
            crate::git::mini_review::ReviewFileClusterer::DEFAULT_MAX_CLUSTERS
        );
        assert_eq!(review_max_clusters(true, &large_diff), 1);
    }
}

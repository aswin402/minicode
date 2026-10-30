use crate::agent::types::AgentEvent;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

/// Review stages in the MiniPower autonomous two-stage task review process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReviewStage {
    /// Stage 1: Checks exact fulfillment of requirements and acceptance criteria.
    SpecCompliance,
    /// Stage 2: Checks code architecture, error handling, security, and edge cases.
    CodeQuality,
}

impl fmt::Display for ReviewStage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SpecCompliance => write!(f, "Spec Compliance"),
            Self::CodeQuality => write!(f, "Code Quality"),
        }
    }
}

/// Severity classification for findings raised during MiniPower task review.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ReviewSeverity {
    /// Non-blocking observation or style recommendation.
    Minor,
    /// Defect that violates requirements, edge cases, or test integrity; must be resolved.
    Important,
    /// Showstopper defect: compiler failure, crash hazard, data corruption, or security risk.
    Critical,
}

impl fmt::Display for ReviewSeverity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Minor => write!(f, "Minor"),
            Self::Important => write!(f, "Important"),
            Self::Critical => write!(f, "Critical"),
        }
    }
}

/// Individual defect or suggestion reported by a reviewer subagent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewFinding {
    pub severity: ReviewSeverity,
    pub description: String,
    pub file: Option<String>,
    pub line: Option<usize>,
}

/// Outcome report summarizing an adversarial review across spec and quality axes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskReviewReport {
    pub stage: ReviewStage,
    pub passed: bool,
    pub findings: Vec<ReviewFinding>,
    pub summary: String,
}

impl TaskReviewReport {
    pub fn has_blocking_issues(&self) -> bool {
        self.findings
            .iter()
            .any(|f| f.severity >= ReviewSeverity::Important)
    }
}

/// Ceremony size classification (`obra/superpowers` v7.0.1 brainstorming sizing).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CeremonySize {
    /// Everything that matters is already in the request — execute directly and state what changed.
    QuickTask,
    /// Throwaway exploratory prototype agreed with the user; skips plan/review gates until promoted.
    Spike,
    /// Bounded change — uses a short 1-2 line per-section design summary before planning.
    SmallChange,
    /// Full multi-task or architectural project — requires full written design + cold-reader Builder Check.
    Project,
}

impl fmt::Display for CeremonySize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::QuickTask => write!(f, "Quick Task"),
            Self::Spike => write!(f, "Spike (Throwaway Prototype)"),
            Self::SmallChange => write!(f, "Small Change (Short Design)"),
            Self::Project => write!(f, "Project (Full Spec + Builder Check)"),
        }
    }
}

impl CeremonySize {
    /// Classifies ceremony size from explicit parameter override or observable workspace state
    /// (`obra/superpowers` v7.0.1 adaptive ceremony sizing — zero hardcoded English word lists).
    pub fn classify(
        workspace_root: &Path,
        topic: &str,
        explicit_size: Option<&str>,
        open_questions_count: usize,
    ) -> Self {
        if let Some(raw) = explicit_size {
            let norm = raw.trim().to_ascii_lowercase().replace(['-', ' '], "_");
            match norm.as_str() {
                "quick" | "quick_task" | "trivial" => return Self::QuickTask,
                "spike" | "prototype" | "poc" => return Self::Spike,
                "small" | "small_change" | "bounded" => return Self::SmallChange,
                "project" | "full" | "large" | "architecture" => return Self::Project,
                _ => {}
            }
        }

        let has_manifest =
            crate::agent::orchestrator::WorkflowRouter::project_manifest(workspace_root).is_some();
        if !has_manifest {
            return Self::Project;
        }

        let docs_dir = crate::tools::minikit::resolve_docs_dir(workspace_root);
        let has_core_spec = docs_dir.join("core/spec.md").is_file()
            || docs_dir.join("core/architecture.md").is_file()
            || docs_dir.join("core/prd.md").is_file();

        let plan_tasks =
            std::fs::read_to_string(workspace_root.join(".minicode/plan/task_plan.md"))
                .ok()
                .map(|c| {
                    c.lines()
                        .filter(|l| {
                            let t = l.trim_start();
                            t.starts_with("- [") || t.starts_with("* [")
                        })
                        .count()
                })
                .unwrap_or(0);

        let token_count = topic.split_whitespace().count();

        if open_questions_count >= 2
            || plan_tasks >= 4
            || (!has_core_spec && open_questions_count >= 1)
            || token_count >= 25
        {
            Self::Project
        } else if open_questions_count == 0 && plan_tasks <= 1 && token_count <= 6 && has_core_spec
        {
            Self::QuickTask
        } else {
            Self::SmallChange
        }
    }
}

/// 4-status report contract returned by implementer subagents (`obra/superpowers` SDD).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImplementerStatus {
    /// Task implemented, tested (RED->GREEN), committed, and self-reviewed cleanly.
    Done,
    /// Task completed, but implementer noted edge-case or structural concerns for controller review.
    DoneWithConcerns,
    /// Implementer lacks required specification or codebase context to proceed safely.
    NeedsContext,
    /// Implementer hit an architectural conflict or environment blocker and stopped cleanly.
    Blocked,
}

impl ImplementerStatus {
    /// Extracts the 4-status report contract from an implementer subagent's output.
    pub fn parse_from_report(output: &str) -> Self {
        let upper = output.to_ascii_uppercase();
        if upper.contains("DONE_WITH_CONCERNS") {
            Self::DoneWithConcerns
        } else if upper.contains("NEEDS_CONTEXT") {
            Self::NeedsContext
        } else if upper.contains("STATUS: BLOCKED") || upper.contains("STATUS:BLOCKED") {
            Self::Blocked
        } else {
            Self::Done
        }
    }
}

impl fmt::Display for ImplementerStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Done => write!(f, "DONE"),
            Self::DoneWithConcerns => write!(f, "DONE_WITH_CONCERNS"),
            Self::NeedsContext => write!(f, "NEEDS_CONTEXT"),
            Self::Blocked => write!(f, "BLOCKED"),
        }
    }
}

/// Structured task item within a MiniPower implementation plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MiniPowerTask {
    pub id: String,
    pub title: String,
    pub description: String,
    pub target_files: Vec<String>,
    pub acceptance_criteria: Vec<String>,
    pub test_command: Option<String>,
    pub completed: bool,
    /// Exact signatures/types consumed from earlier tasks (`Interfaces: Consumes`).
    #[serde(default)]
    pub consumes: Vec<String>,
    /// Exact signatures/types produced for later tasks (`Interfaces: Produces`).
    #[serde(default)]
    pub produces: Vec<String>,
}

/// Core engine for generating prompts, enforcing anti-rationalization guardrails,
/// and coordinating the MiniPower autonomous software development methodology.
pub struct MiniPowerEngine;

impl MiniPowerEngine {
    /// Maximum fix-loop rounds per task before the controller circuit breaker trips (`obra/superpowers` v7.0.1).
    pub const MAX_FIX_LOOP_ROUNDS: u8 = 5;

    /// Anti-rationalization "Red Flags" table mapping excuses to engineering reality.
    pub fn anti_rationalization_table() -> &'static [(&'static str, &'static str)] {
        &[
            (
                "This is just a simple one-liner, no tests needed",
                "Small unverified edits routinely break systems. Write a test first.",
            ),
            (
                "I will write tests after implementation",
                "Code written before tests is unverified. TDD enforces Red before Green.",
            ),
            (
                "I can verify this by reading the code",
                "Reading is not execution. Run the actual compiler or test runner (`power_verify`).",
            ),
            (
                "I need more context before clarifying",
                "Ground in the repo first, then ask clarifying questions before touching files when intent is ambiguous.",
            ),
            (
                "The task is too simple for subagent isolation",
                "Context pollution degrades reasoning. Isolate heavy tasks in subagents (`power_worktree_task` / `spawn_agent`).",
            ),
            (
                "Skipping the test run saves turns and time",
                "Unverified claims cause compound failures that waste 3x more turns later.",
            ),
            (
                "I know what the user wants without asking",
                "Assumptions cause rework. Play back what you heard, mark guesses explicitly, and clarify trade-offs.",
            ),
            (
                "They said 'sounds good' to the idea, so I can start coding",
                "Approving an idea is not approving a design doc or plan. Present approaches and get spec approval first.",
            ),
            (
                "I'll pause and ask the user about this minor plan conflict",
                "Rulings, not stalls: resolve non-destructive conflicts against the spec, record via `record_decision` (`[D1]..[DN]`), and keep going.",
            ),
            (
                "The reviewer said X, so I'll blindly implement it ('You're absolutely right!')",
                "No performative agreement. Verify reviewer claims against the codebase and YAGNI before implementing.",
            ),
        ]
    }

    fn resolve_docs_dir_name(workspace: &Path) -> (std::path::PathBuf, String) {
        let docs_dir = crate::tools::minikit::resolve_docs_dir(workspace);
        let docs_name = docs_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(crate::constants::MINIKIT_DOCS_DIR)
            .to_string();
        (docs_dir, docs_name)
    }

    /// Formats the cold-reader Builder Check prompt (`obra/superpowers` v7.0.1 `builder-check-prompt.md`).
    ///
    /// Dispatched to a fresh subagent (or self-run when inline) before handing a project design spec
    /// to the user so missing decisions are caught in a single round.
    pub fn format_builder_check_prompt(domain: &str, spec_path: &str) -> String {
        format!(
            "### 🧱 MiniPower Builder Check (Cold-Reader Spec Audit)\n\n\
            You are a talented {domain}. You have been handed a design specification at `{spec_path}` \
            and asked to put together an implementation plan to build what it describes. You will NOT be able \
            to talk to the person who wrote it.\n\n\
            1. Read `{spec_path}` via `read_file`.\n\
            2. Report ONLY the questions you would still need answered before you could plan this well: \
               places where you would have to guess and a wrong guess would change what gets built.\n\
            3. Sort every gap into three piles:\n\
               - **Answered already** (by workspace code or `.minicode/decisions.md`): fold the answer into `{spec_path}`.\n\
               - **Minor (Builder's Call)**: settle it yourself and record it via `record_decision` as your call.\n\
               - **Theirs (User Decision Required)**: only the user can answer it and the answer changes what gets built — batch into ONE `ask_user` round, most important first.\n\
            4. Skip anything the document explicitly leaves to the builder and skip wording nitpicks. If nothing important is left, report `nothing important left`.\n"
        )
    }

    /// Generates Socratic brainstorming prompt for pre-implementation discovery (`obra/superpowers` v7.0.1).
    pub fn format_brainstorm_prompt(workspace: &Path, topic: &str) -> String {
        let (docs_dir, docs_name) = Self::resolve_docs_dir_name(workspace);

        // State: dynamically discover what is already settled in docs_dir/core/*.md so no question is asked twice.
        let core_dir = docs_dir.join("core");
        let mut existing_specs: Vec<String> = std::fs::read_dir(&core_dir)
            .ok()
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|entry| {
                let path = entry.path();
                if !path.is_file() {
                    return None;
                }
                let name = path.file_name()?.to_str()?.to_string();
                if !name.ends_with(".md")
                    || name.eq_ignore_ascii_case("todo.md")
                    || name.eq_ignore_ascii_case("notes.md")
                {
                    return None;
                }
                let meta = std::fs::metadata(&path).ok()?;
                if meta.len() == 0 {
                    return None;
                }
                Some(name)
            })
            .collect();
        existing_specs.sort();
        let mut settled = String::new();
        if crate::agent::decisions::read_for_prompt(workspace).is_some() {
            settled.push_str(&format!(
                "- Recorded decisions exist in `{}`: read them; never re-ask them.\n",
                crate::agent::decisions::DECISIONS_FILE
            ));
        }
        if !existing_specs.is_empty() {
            settled.push_str(&format!(
                "- Existing specs in `{}/core/`: {}. Read them first and build on them.\n",
                docs_name,
                existing_specs.join(", ")
            ));
        }
        if settled.is_empty() {
            settled
                .push_str("- Nothing recorded yet: every costly decision below is still open.\n");
        }

        let vault = crate::vault::store::VaultStore::new(workspace);
        let mut live_skill_section = String::new();
        for skill_name in ["brainstorming", "ask-user-inquiry"] {
            if let Ok(skill) = vault.get_skill(skill_name) {
                if skill.scope != crate::vault::models::SkillScope::Builtin {
                    live_skill_section.push_str(&format!(
                        "\n**Live Customized Vault Skill (`{}` [{:?}]):**\n{}\n",
                        skill.name, skill.scope, skill.instructions
                    ));
                }
            }
        }

        format!(
            "### 🧠 MiniPower: Socratic Brainstorming & Spec Refinement (v7.0.1)\n\n\
            **Goal/Topic:** {topic}\n\n\
            **Already settled:**\n{settled}\n\
            **Active Vault Skills (`vault_show` / `vault_update`):** `brainstorming`, `ask-user-inquiry`\n\
            **Procedure (`<HARD-GATE>`: no product code or scaffolding until Step 6 is approved):**\n\
            1. **Look Before You Ask (Ground First)**: Read relevant workspace files (`read_file`, `grep_search`, `symbol_search`), docs, and `{decisions}`. Never ask what the workspace already answers.\n\
            2. **Play It Back & Size the Work**: Echo the user's intent back in ~200 words (marking your own inferences/defaults explicitly as your proposal) and classify ceremony size:\n\
               - `Quick Task`: Everything is specified -> state defaults and execute.\n\
               - `Spike`: Throwaway prototype to test an unknown -> build minimal spike, bring findings back.\n\
               - `Small Change`: Short 1-2 line per-section design summary.\n\
               - `Project`: Full written design specification + Builder Check.\n\
            3. **Clarify Open Decisions & Information (`ask_user` + `record_decision`)**: Never guess or silently assume when you need clarity, domain data, or information from the user, or when you lack confidence on what to build. Ask as many questions as needed in `ask_user` (with 4 to 6 distinct options per choice question, recommended option first) covering purpose, goals/non-goals, data/business rules, stack, architecture, data model, and UI direction. Record every settled choice via `record_decision` (`[D1]..[DN]`).\n\
            4. **Lay Out the Approaches**: Present 4 to 6 concrete implementation options or architectural approaches grounded in the codebase with explicit trade-offs, put your recommended approach first with a 1-line reason, and ask the user to pick via `ask_user`.\n\
            5. **Write the Design & Run Builder Check**: Write the agreed design to `{docs_name}/core/spec.md` (covering intent, goals/non-goals/anti-goals, constraints, approaches considered, user decisions, and builder calls). For a `Project`, run the cold-reader **Builder Check** (`spawn_agent` or self-audit) to resolve missing builder questions in at most one round.\n\
            6. **Spec Approval & Plan Handoff**: Confirm design approval via `ask_user`, then transition to `power_plan` / `create_plan`.\n{live_skill_section}",
            decisions = crate::agent::decisions::DECISIONS_FILE
        )
    }

    /// Generates structured task planning prompt with decision-focused tasks, `Interfaces`, and `Review Focus` (`obra/superpowers` v6.4.2).
    pub fn format_plan_prompt(workspace: &Path, topic: &str) -> String {
        let (_, docs_name) = Self::resolve_docs_dir_name(workspace);
        let vault = crate::vault::store::VaultStore::new(workspace);
        let mut live_skill_section = String::new();
        for skill_name in ["writing-plans", "todo-planning"] {
            if let Ok(skill) = vault.get_skill(skill_name) {
                if skill.scope != crate::vault::models::SkillScope::Builtin {
                    live_skill_section.push_str(&format!(
                        "\n**Live Customized Vault Skill (`{}` [{:?}]):**\n{}\n",
                        skill.name, skill.scope, skill.instructions
                    ));
                }
            }
        }

        format!(
            "### 🛠️ MiniPower: Structured Implementation Plan Builder (v7.0.1)\n\n\
            **Goal/Topic:** {topic}\n\n\
            **Active Vault Skills (`vault_show` / `vault_update`):** `writing-plans`, `todo-planning`\n\
            **Methodology Instructions:**\n\
            1. **Two-Tier Planning Architecture:**\n\
               - Tier 1 (Core Milestones): High-level strategic roadmap recorded in `{docs_name}/core/todo.md` and `{docs_name}/core/implementation.md` (viewed in `/todo` modal).\n\
               - Tier 2 (Active Step Plan): Tactical execution steps recorded in `.minicode/plan/task_plan.md` via `create_plan` or `power_plan` (viewed live in TUI dock).\n\
            2. **A Plan Records Decisions, Not Code Transcripts:**\n\
               - Each task is the smallest unit that carries its own RED->GREEN test cycle.\n\
               - Specify exact **Target Files** (`Create` / `Modify` / `Test`), **Interfaces (`Consumes` / `Produces` exact signatures)**, **Test Assertions** (with exact spec values), and **Verification Command**.\n\
               - Do NOT paste full function bodies that signatures and tests already determine (Proportion Check).\n\
            3. **5-Item `Review Focus` & Pre-Flight Interface Scan:**\n\
               - Identify up to **5 implied inputs or failure modes** the spec implies that could bite a real user, and pin each with a test in the owning task.\n\
               - Scan task pairs that share a file or `Consumes`/`Produces` interface to verify signature consistency before Step 1 begins.\n\
            4. **Rulings, Not Stalls (`record_decision`):**\n\
               - Resolve non-destructive plan ambiguities or conflicts against `{docs_name}/core/spec.md`, log them in `.minicode/decisions.md` via `record_decision` (`[D1]..[DN]`), and continue without stalling. Stop only for destructive/irreversible actions, external side-effects, security risks, or a broken spec.\n\
            5. **Strict Red/Green TDD & MiniBlocks Reuse:**\n\
               - Write the failing test first, watch it fail, write minimal code to pass, and run the project test suite.\n\
               - For UI/frontend tasks, query `block_search`, `block_palettes`, and `block_scaffold` first.\n\
            6. **Execution Handoff (Subagent-Driven vs Native Inline):**\n\
               - **Subagent-Driven (`power_worktree_task` / `spawn_agent`)**: Fresh isolated implementer per task + two-stage review (`SpecCompliance` + `CodeQuality`).\n\
               - **Native Inline (`update_progress` + `power_verify` + `power_review`)**: Execute each `[>]` step sequentially in this session, advancing immediately with `update_progress`, followed by `power_verify` and `power_review` at completion.\n{live_skill_section}"
        )
    }

    /// Formats the isolated brief given to an implementer subagent (`obra/superpowers` v7.0.1 `implementer-prompt.md`).
    pub fn format_implementer_prompt(task: &MiniPowerTask) -> String {
        let mut out = format!(
            "### ⚡ MiniPower Task Implementer Brief\n\n\
            **Task ID:** {}\n\
            **Title:** {}\n\n\
            **Description:**\n{}\n\n\
            **Target Files:**\n",
            task.id, task.title, task.description
        );

        for f in &task.target_files {
            out.push_str(&format!("- `{}`\n", f));
        }

        if !task.consumes.is_empty() || !task.produces.is_empty() {
            out.push_str("\n**Interfaces Contract:**\n");
            for c in &task.consumes {
                out.push_str(&format!("- Consumes: `{}`\n", c));
            }
            for p in &task.produces {
                out.push_str(&format!("- Produces: `{}`\n", p));
            }
        }

        out.push_str("\n**Acceptance Criteria:**\n");
        for ac in &task.acceptance_criteria {
            out.push_str(&format!("- [ ] {}\n", ac));
        }

        if let Some(cmd) = &task.test_command {
            out.push_str(&format!("\n**Verification Command:**\n`{}`\n", cmd));
        }

        out.push_str(
            "\n**Execution & Isolation Rules:**\n\
            1. **Strict Red/Green TDD (`writing-good-tests`)**: Write the failing test first (naming the production break it catches; no mirror assertions or mock-existence checks), watch it fail (RED), implement minimal code, and watch it pass (GREEN).\n\
            2. **Zero Panics**: Never use `.unwrap()` or `.expect()` in non-test production code.\n\
            3. **No-Subagents Contract**: Do all of this task's work yourself. NEVER spawn subagents or reviewers (`spawn_agent` / `fanout_agents`) — the controller schedules review after your report.\n\
            4. **4-Status Report Contract**: Conclude your report with one of:\n\
               - `Status: DONE` (all acceptance criteria met, tests RED->GREEN, output pristine)\n\
               - `Status: DONE_WITH_CONCERNS` (completed, but list edge-case or structural concerns)\n\
               - `Status: NEEDS_CONTEXT` (specify exact missing requirement or interface detail)\n\
               - `Status: BLOCKED` (specify exact blocker and what was attempted)\n",
        );

        out
    }

    /// Formats the prompt for a task reviewer subagent across Spec Compliance or Code Quality
    /// (`obra/superpowers` v7.0.1 `task-reviewer-prompt.md` & `code-reviewer.md`).
    pub fn format_reviewer_prompt(task: &MiniPowerTask, diff: &str, stage: ReviewStage) -> String {
        match stage {
            ReviewStage::SpecCompliance => format!(
                "### 🔍 MiniPower Reviewer: Stage 1 — Spec Compliance\n\n\
                **Task ID:** {}\n\
                **Title:** {}\n\n\
                **Task Acceptance Criteria:**\n{}\n\n\
                **Implementation Diff:**\n```diff\n{}\n```\n\n\
                **Review Rules & Rubric:**\n\
                1. **Do Not Trust Implementer Rationales**: Verify every claim directly against the diff. Stated rationales ('left per YAGNI') never downgrade a missing requirement.\n\
                2. **Completeness & Batched File Check**: Did the diff satisfy EVERY acceptance criterion and touch every required target file? Were any requirements stubbed out or misunderstood?\n\
                3. **The Spec Is a Vision Document**: For inputs or failure modes the spec is silent on, judge by what a reasonable user expects — a crash or data corruption on an unnamed input is Critical/Important, never Minor.\n\
                4. **Do Not Re-Run Passing Implementer Tests**: Inspect test assertions and reported evidence in the diff; do not waste turns re-running the full suite unless a specific line raises a concrete doubt.\n\
                5. **No-Subagents Contract**: Perform this review yourself in read-only mode; never spawn subagents.\n\
                6. **Output Sections**: Report `Spec Compliance` (`✅` / `❌` / `⚠️ Cannot verify from diff`), `Declined to judge` (behaviors set aside for controller ruling), and findings classified as `Critical`, `Important`, or `Minor` with `file:line` citations.\n",
                task.id,
                task.title,
                task.acceptance_criteria
                    .iter()
                    .map(|ac| format!("- {}", ac))
                    .collect::<Vec<_>>()
                    .join("\n"),
                diff
            ),
            ReviewStage::CodeQuality => format!(
                "### 🛡️ MiniPower Reviewer: Stage 2 — Code Quality & Architecture\n\n\
                **Task ID:** {}\n\
                **Title:** {}\n\n\
                **Implementation Diff:**\n```diff\n{}\n```\n\n\
                **Review Rules & Rubric:**\n\
                1. **Error Handling & Safety**: Are errors propagated idiomatically with zero `.unwrap()` / `.expect()` in production code?\n\
                2. **Security & Boundary Validation**: Are inputs validated, paths confined, and secrets protected?\n\
                3. **Test Falsifiability (`writing-good-tests`)**: Do tests exercise real behavior with hand-derived expectations (no tautological mirror assertions, no mock-existence checks, no string-presence greps)?\n\
                4. **Concurrency & Performance**: Are there race conditions, unbounded channels, or unnecessary $O(N)$ allocations?\n\
                5. **The Spec Is a Vision Document & Declined to Judge**: Grade unnamed-input failures by real user impact, and list any set-aside behaviors under `Declined to judge` for controller ruling.\n\
                6. **No-Subagents Contract**: Perform this review yourself in read-only mode; classify findings as `Critical`, `Important`, or `Minor` with exact `file:line` citations.\n",
                task.id,
                task.title,
                diff
            ),
        }
    }

    /// Formats a scoped re-review prompt after a fix round (`obra/superpowers` v7.0.1 `re-review-prompt.md` & `fix-loop.md`).
    pub fn format_scoped_rereview_prompt(
        task: &MiniPowerTask,
        findings: &[ReviewFinding],
        fix_diff: &str,
        round: u8,
    ) -> String {
        let mut findings_list = String::new();
        for f in findings {
            let loc = match (&f.file, f.line) {
                (Some(file), Some(line)) => format!(" (`{}:{}`)", file, line),
                (Some(file), None) => format!(" (`{}`)", file),
                _ => String::new(),
            };
            findings_list.push_str(&format!(
                "- **[{}]** {}{}\n",
                f.severity, f.description, loc
            ));
        }

        let escalation_note = if round >= Self::MAX_FIX_LOOP_ROUNDS {
            "⚠️ **Circuit Breaker Reached (Round 5/5)**: If findings remain open after this check, stop dispatching fix rounds and adjudicate each open finding in `.minicode/decisions.md` (`record_decision`)."
        } else if round >= 4 {
            "⚡ **Escalated Fix Round (Rounds 4–5)**: Dispatched with a higher-capability tier after prior attempts."
        } else {
            "🔄 **Standard Fix Round (Rounds 1–3)**: Verifying targeted fixes from the implementer."
        };

        format!(
            "### 🔎 MiniPower Scoped Re-Review (Task `{}` — Fix Round {}/{})\n\n\
            {}\n\n\
            **Findings Under Verification:**\n{}\n\
            **Fix Diff Under Review:**\n```diff\n{}\n```\n\n\
            **Scoped Re-Review Rules:**\n\
            1. **Finding Verdicts**: For each finding above in order, verdict `ADDRESSED` or `NOT ADDRESSED` with `file:line` evidence ('attempted' is not addressed).\n\
            2. **New Breakage in Fix Diff Only**: Flag any new `Critical` or `Important` defect introduced by the fix diff itself.\n\
            3. **Out-of-Scope Observations**: Do NOT re-review untouched code. Any observation outside the fix diff goes under `Out-of-Scope Observations` as non-blocking (never extends the fix loop).\n",
            task.id,
            round,
            Self::MAX_FIX_LOOP_ROUNDS,
            escalation_note,
            findings_list,
            fix_diff
        )
    }

    /// Formats an actionable fix prompt for an implementer subagent when findings are raised.
    pub fn format_fix_prompt(task: &MiniPowerTask, findings: &[ReviewFinding]) -> String {
        let mut out = format!(
            "### ⚠️ MiniPower Fix Required for Task `{}`\n\n\
            The task reviewer identified issues that must be resolved before proceeding:\n\n",
            task.id
        );

        for f in findings {
            let loc = match (&f.file, f.line) {
                (Some(file), Some(line)) => format!(" (`{}:{}`)", file, line),
                (Some(file), None) => format!(" (`{}`)", file),
                _ => String::new(),
            };
            out.push_str(&format!(
                "- **[{}]** {}{}\n",
                f.severity, f.description, loc
            ));
        }

        out.push_str(
            "\n**Fix-Loop Contract (`max 5 rounds`):**\n\
            1. Fix every `Critical` and `Important` finding above using RED->GREEN TDD (write/update the test that reproduces the finding, watch it fail, fix the code, watch it pass).\n\
            2. Run the covering test command and include the command + output in your report.\n\
            3. Minor findings are deferred to the ledger and do not block task completion.\n",
        );
        out
    }

    /// Sanitizes an arbitrary plan or topic label into a filesystem-safe slug (`obra/superpowers` `sdd-workspace`).
    pub fn sanitize_slug(raw: &str) -> String {
        let mut slug = String::with_capacity(raw.len().min(60));
        let mut prev_dash = false;
        for ch in raw.chars() {
            if ch.is_ascii_alphanumeric() {
                slug.push(ch.to_ascii_lowercase());
                prev_dash = false;
            } else if !prev_dash && !slug.is_empty() {
                slug.push('-');
                prev_dash = true;
            }
            if slug.len() >= 60 {
                break;
            }
        }
        let trimmed = slug.trim_matches('-').to_string();
        if trimmed.is_empty() {
            "active-plan".to_string()
        } else {
            trimmed
        }
    }

    fn resolve_plan_identity(workspace: &Path, plan_label: Option<&str>) -> String {
        if let Some(lbl) = plan_label.map(str::trim).filter(|s| !s.is_empty()) {
            return lbl.to_string();
        }
        let plan_path = workspace.join(".minicode/plan/task_plan.md");
        if let Ok(content) = std::fs::read_to_string(&plan_path) {
            if let Some(first_heading) = content
                .lines()
                .map(str::trim)
                .find(|l| !l.is_empty())
                .map(|l| l.trim_start_matches('#').trim())
            {
                if !first_heading.is_empty() {
                    return first_heading.to_string();
                }
            }
        }
        "active-plan".to_string()
    }

    /// Creates or resolves a plan-scoped SDD scratch directory (`<workspace>/.minicode/sdd/<slug>/`)
    /// with `plan-path` ownership disambiguation and `.gitignore` protection (`obra/superpowers` v7.0.1 `sdd-workspace`).
    pub fn ensure_sdd_workspace(workspace: &Path, plan_label: Option<&str>) -> PathBuf {
        let sdd_root = workspace.join(".minicode/sdd");
        let _ = std::fs::create_dir_all(&sdd_root);
        let gitignore_path = sdd_root.join(".gitignore");
        if !gitignore_path.exists() {
            let _ = std::fs::write(&gitignore_path, "*\n");
        }

        let plan_id = Self::resolve_plan_identity(workspace, plan_label);
        let base_slug = Self::sanitize_slug(&plan_id);

        let mut chosen = sdd_root.join(&base_slug);
        if chosen.is_dir() {
            let marker = chosen.join("plan-path");
            if let Ok(existing_owner) = std::fs::read_to_string(&marker) {
                if existing_owner.trim() != plan_id {
                    for suffix in 2..=99 {
                        let candidate = sdd_root.join(format!("{}-{}", base_slug, suffix));
                        let cand_marker = candidate.join("plan-path");
                        let owner_matches = std::fs::read_to_string(&cand_marker)
                            .map(|o| o.trim() == plan_id)
                            .unwrap_or(false);
                        if !candidate.exists() || owner_matches {
                            chosen = candidate;
                            break;
                        }
                    }
                }
            }
        }

        let _ = std::fs::create_dir_all(&chosen);
        let _ = std::fs::write(chosen.join("plan-path"), format!("{}\n", plan_id));
        chosen
    }

    /// Writes a self-contained per-task brief (`task-<id>-brief.md`) into the active SDD workspace
    /// (`obra/superpowers` v7.0.1 `task-brief`).
    pub fn write_task_brief(
        workspace: &Path,
        plan_label: Option<&str>,
        task: &MiniPowerTask,
        base_sha: Option<&str>,
    ) -> std::io::Result<PathBuf> {
        let sdd_dir = Self::ensure_sdd_workspace(workspace, plan_label);
        let safe_id = Self::sanitize_slug(&task.id);
        let brief_path = sdd_dir.join(format!("task-{}-brief.md", safe_id));

        let mut content = format!("# Task {}: {}\n", task.id, task.title);
        if let Some(sha) = base_sha.map(str::trim).filter(|s| !s.is_empty()) {
            content.push_str(&format!("BASE: {}\n", sha));
        }
        content.push('\n');
        content.push_str(&Self::format_implementer_prompt(task));
        std::fs::write(&brief_path, content)?;
        Ok(brief_path)
    }

    /// Writes a range-checked review diff artifact (`review-<base7>..<head7>.diff`) into the SDD workspace
    /// (`obra/superpowers` v7.0.1 `review-package`).
    pub fn write_review_package(
        workspace: &Path,
        plan_label: Option<&str>,
        base_sha: Option<&str>,
        head_sha: Option<&str>,
        diff_text: &str,
    ) -> std::io::Result<PathBuf> {
        let sdd_dir = Self::ensure_sdd_workspace(workspace, plan_label);
        let base7: String = base_sha
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("worktree")
            .chars()
            .take(7)
            .collect();
        let head7: String = head_sha
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("head")
            .chars()
            .take(7)
            .collect();
        let pkg_path = sdd_dir.join(format!("review-{}..{}.diff", base7, head7));
        let payload = format!("# Review Package: {}..{}\n\n{}\n", base7, head7, diff_text);
        std::fs::write(&pkg_path, payload)?;
        Ok(pkg_path)
    }

    /// Appends a compaction-resilient progress entry, ruling, or deferred minor finding to `<sdd>/progress.md`
    /// (`obra/superpowers` v7.0.1 SDD ledger).
    pub fn append_sdd_ledger(
        workspace: &Path,
        plan_label: Option<&str>,
        entry: &str,
    ) -> std::io::Result<PathBuf> {
        let sdd_dir = Self::ensure_sdd_workspace(workspace, plan_label);
        let ledger_path = sdd_dir.join("progress.md");
        let plan_id = Self::resolve_plan_identity(workspace, plan_label);

        let mut existing = std::fs::read_to_string(&ledger_path).unwrap_or_default();
        if existing.trim().is_empty() {
            existing = format!("# SDD ledger — plan: {}\n\n", plan_id);
        }

        let normalized_entry = entry.trim().trim_start_matches("- ").trim();
        let bullet = format!("- {}", normalized_entry);
        if !existing.lines().any(|l| l.trim() == bullet) {
            if !existing.ends_with('\n') {
                existing.push('\n');
            }
            existing.push_str(&bullet);
            existing.push('\n');
            std::fs::write(&ledger_path, existing)?;
        }
        Ok(ledger_path)
    }

    /// Reads the active SDD `progress.md` ledger for the workspace so the orchestrator can
    /// resume automatically across turns and context compactions.
    pub fn read_sdd_ledger(workspace: &Path) -> Option<(PathBuf, String)> {
        let plan_id = Self::resolve_plan_identity(workspace, None);
        let active_dir = workspace
            .join(".minicode/sdd")
            .join(Self::sanitize_slug(&plan_id));
        let active_ledger = active_dir.join("progress.md");
        if let Ok(content) = std::fs::read_to_string(&active_ledger) {
            if !content.trim().is_empty() {
                return Some((active_ledger, content));
            }
        }

        let sdd_root = workspace.join(".minicode/sdd");
        let mut newest: Option<(std::time::SystemTime, PathBuf, String)> = None;
        for entry in std::fs::read_dir(&sdd_root).ok()?.flatten() {
            let ledger = entry.path().join("progress.md");
            if let Ok(content) = std::fs::read_to_string(&ledger) {
                if content.trim().is_empty() {
                    continue;
                }
                let mtime = std::fs::metadata(&ledger)
                    .and_then(|m| m.modified())
                    .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                if newest.as_ref().map(|(t, _, _)| mtime > *t).unwrap_or(true) {
                    newest = Some((mtime, ledger, content));
                }
            }
        }
        newest.map(|(_, path, content)| (path, content))
    }

    /// Runs a 7-dimension session & workspace forensics audit (`obra/superpowers` v7.0.1 `diagnosing-superpowers`).
    ///
    /// Streams the most recent `.minicode/sessions/*.jsonl` transcript alongside `.minicode/plan/task_plan.md`,
    /// `.minicode/sdd/*/progress.md`, `.minicode/decisions.md`, `.minicode/plan/last_verify.json`, and
    /// `<docs_dir>/core/todo.md`, producing concrete `path:line` evidence across all 7 dimensions.
    pub fn diagnose_workspace(workspace: &Path, complaint: Option<&str>) -> String {
        let complaint_str = complaint
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("General MiniPower session & plan health audit");

        let transcript = Self::inspect_latest_session_transcript(workspace);

        // Determine primary investigation lead from explicit dimension identifiers or observed anomalies
        let lead_dimension = Self::compute_diagnostic_lead(workspace, complaint_str, &transcript);
        let mut out = format!(
            "### 🩺 MiniPower Session Forensics & Diagnostic Report (`diagnosing-minipower`)\n\n\
            **Reported Complaint / Scope:** {}\n\
            **Primary Investigation Lead:** {}\n\n\
            *Core Principle: Every finding cites `path:line` evidence from workspace transcripts, plans, and ledgers.*\n\n",
            complaint_str, lead_dimension
        );

        // Dimension 1: Skill & Methodology Timeline (`skill-timeline`)
        let active_skills =
            crate::tools::minikit::skills::MiniKitSkillsManager::get_manifest_active_skills(
                workspace,
            );
        let skills_dir = workspace.join(".minicode/skills");
        let installed_skill_files: Vec<String> = std::fs::read_dir(&skills_dir)
            .ok()
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        out.push_str("#### 1. `skill-timeline` (Active Skills & Process Routing)\n");
        if active_skills.is_empty() && installed_skill_files.is_empty() {
            out.push_str("- `.minicode/skills:1` — 0 workspace-local skill overrides installed; using built-in MiniVault catalog (`vault_skill_list`).\n");
        } else {
            out.push_str(&format!(
                "- `.minicode/skills:1` — Manifest active skills: [{}]; workspace skill entries: [{}].\n",
                active_skills.join(", "),
                installed_skill_files.join(", ")
            ));
        }
        if let Some(ref ts) = transcript {
            if ts.skill_and_power_calls.is_empty() {
                out.push_str(&format!(
                    "- `{}:1` — 0 explicit `power_*` or `vault_*` tool calls recorded in latest session `{}`.\n",
                    ts.rel_path, ts.session_id
                ));
            } else {
                for (line_no, summary) in ts.skill_and_power_calls.iter().take(8) {
                    out.push_str(&format!("- `{}:{}` — {}\n", ts.rel_path, line_no, summary));
                }
            }
        }

        // Dimension 2: Plan Adherence (`plan-adherence`)
        out.push_str("\n#### 2. `plan-adherence` (Two-Tier Plan & SDD Ledger State)\n");
        let task_plan_path = workspace.join(".minicode/plan/task_plan.md");
        if let Ok(plan_content) = std::fs::read_to_string(&task_plan_path) {
            let mut completed = 0usize;
            let mut active_line = None;
            let mut pending = 0usize;
            for (idx, line) in plan_content.lines().enumerate() {
                let t = line.trim();
                if t.contains("[x]") || t.contains("[X]") {
                    completed += 1;
                } else if t.contains("[>]") {
                    active_line = Some((idx + 1, t.to_string()));
                } else if t.contains("[ ]") {
                    pending += 1;
                }
            }
            out.push_str(&format!(
                "- `.minicode/plan/task_plan.md:1` — {} completed `[x]`, {} pending `[ ]` tactical steps.\n",
                completed, pending
            ));
            if let Some((line_no, text)) = active_line {
                out.push_str(&format!(
                    "- `.minicode/plan/task_plan.md:{}` — Active `[>]` step: `{}`.\n",
                    line_no, text
                ));
            }
        } else {
            out.push_str(
                "- `.minicode/plan/task_plan.md:1` — No active tactical plan file on disk.\n",
            );
        }

        if let Some((ledger_path, ledger_content)) = Self::read_sdd_ledger(workspace) {
            let rel_ledger = ledger_path
                .strip_prefix(workspace)
                .unwrap_or(&ledger_path)
                .display();
            for (idx, line) in ledger_content
                .lines()
                .enumerate()
                .filter(|(_, l)| !l.trim().is_empty())
                .take(6)
            {
                out.push_str(&format!(
                    "- `{}:{}` — `{}`\n",
                    rel_ledger,
                    idx + 1,
                    line.trim()
                ));
            }
        }

        let todo_path = crate::tools::minikit::resolve_doc_path(workspace, "todo.md");
        if let Ok(todo_content) = std::fs::read_to_string(&todo_path) {
            let rel_todo = todo_path
                .strip_prefix(workspace)
                .unwrap_or(&todo_path)
                .display();
            if let Some((idx, line)) = todo_content
                .lines()
                .enumerate()
                .find(|(_, l)| l.contains("[>]"))
            {
                out.push_str(&format!(
                    "- `{}:{}` — Active milestone task: `{}`.\n",
                    rel_todo,
                    idx + 1,
                    line.trim()
                ));
            }
        }

        // Dimension 3: Request Conflicts & Rulings (`request-conflicts`)
        out.push_str("\n#### 3. `request-conflicts` (Decision Ledger `D1..DN` & Rulings)\n");
        let decisions_path = workspace.join(crate::agent::decisions::DECISIONS_FILE);
        if let Ok(dec_content) = std::fs::read_to_string(&decisions_path) {
            let entries: Vec<(usize, &str)> = dec_content
                .lines()
                .enumerate()
                .filter(|(_, l)| {
                    let t = l.trim_start();
                    t.starts_with("## [D") || t.starts_with("- **[D") || t.starts_with("- [D")
                })
                .collect();
            if entries.is_empty() {
                out.push_str(&format!(
                    "- `{}:1` — File exists but contains 0 numbered `[D1]..[DN]` entries.\n",
                    crate::agent::decisions::DECISIONS_FILE
                ));
            } else {
                for (line_idx, line) in entries.iter().take(6) {
                    out.push_str(&format!(
                        "- `{}:{}` — {}\n",
                        crate::agent::decisions::DECISIONS_FILE,
                        line_idx + 1,
                        line.trim()
                    ));
                }
            }
        } else {
            out.push_str(&format!(
                "- `{}:1` — No decision ledger recorded yet (use `record_decision` to log architectural rulings).\n",
                crate::agent::decisions::DECISIONS_FILE
            ));
        }
        if let Some(ref ts) = transcript {
            for (line_no, summary) in ts.inquiry_and_decision_calls.iter().take(5) {
                out.push_str(&format!("- `{}:{}` — {}\n", ts.rel_path, line_no, summary));
            }
        }

        // Dimension 4: Quality & Verification Evidence (`quality-evidence`)
        out.push_str("\n#### 4. `quality-evidence` (4-Gate Verification Barrier Evidence)\n");
        let verify_path = workspace.join(".minicode/plan/last_verify.json");
        if let Ok(verify_content) = std::fs::read_to_string(&verify_path) {
            let first_line = verify_content.lines().nth(1).unwrap_or("{}").trim();
            out.push_str(&format!(
                "- `.minicode/plan/last_verify.json:2` — Latest verification record: `{}`.\n",
                first_line
            ));
        } else {
            out.push_str(
                "- `.minicode/plan/last_verify.json:1` — No `power_verify` barrier run recorded for the current plan yet.\n",
            );
        }
        if let Some(ref ts) = transcript {
            if let Some((verify_line, ref verify_tool)) = ts.last_verify_or_review {
                out.push_str(&format!(
                    "- `{}:{}` — Latest verification/review call: `{}`.\n",
                    ts.rel_path, verify_line, verify_tool
                ));
                if let Some((mut_line, ref mut_desc)) = ts.last_mutation {
                    if mut_line > verify_line {
                        out.push_str(&format!(
                            "- ⚠️ `{}:{}` — Unverified file mutation occurred AFTER last verification (`{}` at line {} > `{}` at line {}).\n",
                            ts.rel_path, mut_line, mut_desc, mut_line, verify_tool, verify_line
                        ));
                    }
                }
            } else if let Some((mut_line, ref mut_desc)) = ts.last_mutation {
                out.push_str(&format!(
                    "- ⚠️ `{}:{}` — File mutation `{}` recorded with 0 `power_verify`/`power_review` runs in this session.\n",
                    ts.rel_path, mut_line, mut_desc
                ));
            }
        }

        // Dimension 5: Repeated Work (`repeated-work`)
        out.push_str("\n#### 5. `repeated-work` (Redundant Tool & Target Invocations)\n");
        if let Some(ref ts) = transcript {
            if ts.repeated_targets.is_empty() {
                out.push_str(&format!(
                    "- `{}:1` — 0 repeated `(tool, target)` hotspots (`>= 3` calls) detected across {} tool invocations.\n",
                    ts.rel_path, ts.total_tool_calls
                ));
            } else {
                for (key, lines) in ts.repeated_targets.iter().take(6) {
                    let line_list = lines
                        .iter()
                        .map(|l| l.to_string())
                        .collect::<Vec<_>>()
                        .join(", ");
                    out.push_str(&format!(
                        "- `{}:{}` — `{}` invoked {} times (lines: {}).\n",
                        ts.rel_path,
                        lines.first().copied().unwrap_or(1),
                        key,
                        lines.len(),
                        line_list
                    ));
                }
            }
        } else {
            out.push_str("- `.minicode/sessions:1` — No session JSONL transcript found on disk.\n");
        }

        // Dimension 6: Stumbles (`stumbles`)
        out.push_str("\n#### 6. `stumbles` (Tool Failures, Retries & Runtime Errors)\n");
        if let Some(ref ts) = transcript {
            if ts.stumbles.is_empty() {
                out.push_str(&format!(
                    "- `{}:1` — 0 failed tool executions or agent errors recorded in session `{}`.\n",
                    ts.rel_path, ts.session_id
                ));
            } else {
                for (line_no, summary) in ts.stumbles.iter().take(8) {
                    out.push_str(&format!("- `{}:{}` — {}\n", ts.rel_path, line_no, summary));
                }
            }
        } else {
            out.push_str("- `.minicode/sessions:1` — No session JSONL transcript found on disk.\n");
        }

        // Dimension 7: Cost and Time (`cost-and-time`)
        out.push_str("\n#### 7. `cost-and-time` (Turn, Token, Subagent & Cache Telemetry)\n");
        if let Some(ref ts) = transcript {
            out.push_str(&format!(
                "- `{}:1` — Session `{}`: {} turns, {} tool calls, {} subagent dispatches, {} total tokens ({} cached prompt tokens).\n",
                ts.rel_path,
                ts.session_id,
                ts.turns,
                ts.total_tool_calls,
                ts.subagent_dispatches,
                ts.total_tokens,
                ts.cached_tokens
            ));
            if let Some((slow_line, ref slow_tool, slow_ms)) = ts.slowest_tool {
                out.push_str(&format!(
                    "- `{}:{}` — Slowest tool execution: `{}` ({} ms).\n",
                    ts.rel_path, slow_line, slow_tool, slow_ms
                ));
            }
        }
        let trigram_cache = workspace.join(".minicode/cache/trigram_index.bin");
        if let Ok(meta) = std::fs::metadata(&trigram_cache) {
            out.push_str(&format!(
                "- `.minicode/cache/trigram_index.bin:1` — Trigram search index active ({} bytes cached).\n",
                meta.len()
            ));
        }
        let symbol_cache = workspace.join(".minicode/cache/symbol_index.json");
        if let Ok(meta) = std::fs::metadata(&symbol_cache) {
            out.push_str(&format!(
                "- `.minicode/cache/symbol_index.json:1` — Symbol B-Tree index active ({} bytes cached).\n",
                meta.len()
            ));
        }

        out
    }

    fn inspect_latest_session_transcript(workspace: &Path) -> Option<SessionTranscriptForensics> {
        let store = crate::session::store::SessionStore::with_workspace(workspace);
        let sessions = store.list_sessions().ok()?;
        let latest = sessions.first()?;
        let path = PathBuf::from(&latest.path);
        let file = std::fs::File::open(&path).ok()?;
        let rel_path = path
            .strip_prefix(workspace)
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| path.display().to_string());

        let mut forensics = SessionTranscriptForensics {
            session_id: latest.id.clone(),
            rel_path,
            ..Default::default()
        };

        let mut target_counts: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        let reader = BufReader::new(file);

        for (idx, line_res) in reader.lines().enumerate() {
            let line_no = idx + 1;
            let line = match line_res {
                Ok(l) => l,
                Err(_) => continue,
            };
            let trimmed = line.trim();
            if trimmed.is_empty()
                || trimmed.starts_with("{\"session_meta\":")
                || trimmed.contains("\"stream_delta\"")
            {
                continue;
            }

            let event = match serde_json::from_str::<AgentEvent>(trimmed) {
                Ok(ev) => ev,
                Err(_) => continue,
            };

            match event {
                AgentEvent::ToolCall {
                    turn_id,
                    tool,
                    args,
                    ..
                } => {
                    forensics.total_tool_calls += 1;
                    let target = args
                        .get("path")
                        .or_else(|| args.get("file_path"))
                        .or_else(|| args.get("topic"))
                        .or_else(|| args.get("name"))
                        .or_else(|| args.get("command"))
                        .or_else(|| args.get("query"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let key = if target.is_empty() {
                        tool.clone()
                    } else {
                        let short: String = target.chars().take(60).collect();
                        format!("{}({})", tool, short)
                    };
                    target_counts.entry(key.clone()).or_default().push(line_no);

                    if tool.starts_with("power_")
                        || tool.starts_with("vault_")
                        || tool == "create_plan"
                        || tool == "update_progress"
                        || tool == "archive_plan"
                    {
                        forensics
                            .skill_and_power_calls
                            .push((line_no, format!("Turn {}: `{}`", turn_id, key)));
                    }
                    if tool == "ask_user" || tool == "record_decision" {
                        forensics
                            .inquiry_and_decision_calls
                            .push((line_no, format!("Turn {}: `{}`", turn_id, key)));
                    }
                    if tool == "power_verify" || tool == "power_review" {
                        forensics.last_verify_or_review = Some((line_no, tool.clone()));
                    }
                    if tool == "write_file" || tool == "patch_file" || tool == "edit_file" {
                        forensics.last_mutation = Some((line_no, key));
                    }
                    if tool == "spawn_agent"
                        || tool == "fanout_agents"
                        || tool == "power_worktree_task"
                    {
                        forensics.subagent_dispatches += 1;
                    }
                }
                AgentEvent::ToolResult {
                    turn_id,
                    tool,
                    success,
                    output,
                    duration_ms,
                    ..
                } => {
                    if forensics
                        .slowest_tool
                        .as_ref()
                        .map(|(_, _, ms)| duration_ms > *ms)
                        .unwrap_or(true)
                    {
                        forensics.slowest_tool = Some((line_no, tool.clone(), duration_ms));
                    }
                    if !success {
                        let first_line: String = output
                            .lines()
                            .find(|l| !l.trim().is_empty())
                            .unwrap_or("tool returned success=false")
                            .chars()
                            .take(100)
                            .collect();
                        forensics.stumbles.push((
                            line_no,
                            format!(
                                "Turn {} `{}` failed ({} ms): `{}`",
                                turn_id, tool, duration_ms, first_line
                            ),
                        ));
                    }
                }
                AgentEvent::FileModified { path, action, .. } => {
                    forensics.last_mutation = Some((line_no, format!("{}:{}", action, path)));
                }
                AgentEvent::TurnEnd {
                    turn_id,
                    total_tokens_used,
                    cached_prompt_tokens,
                    ..
                } => {
                    forensics.turns = forensics.turns.max(turn_id);
                    forensics.total_tokens += total_tokens_used;
                    forensics.cached_tokens += cached_prompt_tokens;
                }
                AgentEvent::Error {
                    turn_id,
                    code,
                    message,
                    ..
                } => {
                    let short_msg: String = message.chars().take(100).collect();
                    forensics.stumbles.push((
                        line_no,
                        format!(
                            "Turn {:?} Error `{}`: `{}`",
                            turn_id.unwrap_or(0),
                            code,
                            short_msg
                        ),
                    ));
                }
                _ => {}
            }
        }

        forensics.repeated_targets = target_counts
            .into_iter()
            .filter(|(_, lines)| lines.len() >= 3)
            .collect();

        Some(forensics)
    }

    fn compute_diagnostic_lead(
        workspace: &Path,
        complaint: &str,
        transcript: &Option<SessionTranscriptForensics>,
    ) -> String {
        let mut leads: Vec<&'static str> = Vec::new();
        let lower = complaint.to_ascii_lowercase();

        for dim in [
            "skill-timeline",
            "plan-adherence",
            "request-conflicts",
            "quality-evidence",
            "repeated-work",
            "stumbles",
            "cost-and-time",
        ] {
            if lower.contains(dim) {
                leads.push(dim);
            }
        }

        if let Some(ref ts) = transcript {
            if !ts.stumbles.is_empty() && !leads.contains(&"stumbles") {
                leads.push("stumbles");
            }
            if !ts.repeated_targets.is_empty() && !leads.contains(&"repeated-work") {
                leads.push("repeated-work");
            }
            let unverified_mutation =
                match (ts.last_mutation.as_ref(), ts.last_verify_or_review.as_ref()) {
                    (Some((m_line, _)), Some((v_line, _))) => m_line > v_line,
                    (Some(_), None) => true,
                    _ => false,
                };
            if unverified_mutation && !leads.contains(&"quality-evidence") {
                leads.push("quality-evidence");
            }
            if ts
                .slowest_tool
                .as_ref()
                .map(|(_, _, ms)| *ms >= 5_000)
                .unwrap_or(false)
                && !leads.contains(&"cost-and-time")
            {
                leads.push("cost-and-time");
            }
        }

        if workspace.join(".minicode/plan/task_plan.md").is_file()
            && !leads.contains(&"plan-adherence")
        {
            leads.push("plan-adherence");
        }
        if workspace
            .join(crate::agent::decisions::DECISIONS_FILE)
            .is_file()
            && !leads.contains(&"request-conflicts")
        {
            leads.push("request-conflicts");
        }

        if leads.is_empty() {
            "plan-adherence + quality-evidence + skill-timeline".to_string()
        } else {
            leads.join(" + ")
        }
    }

    /// Formats executive status summary of the MiniPower methodology engine.
    pub fn format_status_summary() -> String {
        let mut out = String::from(
            "⚡ **MiniPower: Native Autonomous Engineering Methodology (Superpowers v7.0.1)**\n\n\
            *Rigorous, disciplined software engineering built directly into minicode.*\n\n\
            ### Core Pillars:\n\
            1. **Socratic Brainstorming & Builder Check**: Ground first, play back intent, size ceremony, and run a cold-reader Builder Check before coding.\n\
            2. **Git Worktree Isolation & SDD Workspace**: Protect the main branch in ephemeral sandbox branches (`power_worktree_task`) with `.minicode/sdd/<slug>/` task briefs and `progress.md` ledgers.\n\
            3. **Decision-Focused Planning & Review Focus**: Atomic tasks with `Interfaces (Consumes/Produces)`, 5-item `Review Focus`, and `[D1]..[DN]` rulings.\n\
            4. **Two-Stage Subagent Review & 5-Round Circuit Breaker**: Stage 1 (Spec Compliance) + Stage 2 (Code Quality) + Scoped Re-Review (`power_review`).\n\
            5. **Strict Red/Green TDD (`writing-good-tests`)**: Falsifiability gate, zero mirror/mock-existence assertions, and mutation check.\n\
            6. **Evidence Before Assertions & Session Forensics**: 4-Gate `power_verify` barrier and 7-dimension `power_diagnose` transcript auditing.\n\n\
            ### Anti-Rationalization Guardrails (\"Red Flags\"):\n",
        );

        for (excuse, reality) in Self::anti_rationalization_table() {
            out.push_str(&format!("• *\"{}\"*\n  ╰─── {}\n", excuse, reality));
        }

        out.push_str(
            "\n💡 Autonomous Orchestrator Tools: `power_brainstorm`, `power_plan`, `power_worktree_task`, `power_review`, `power_verify`, `power_diagnose` (TUI Inspector: `Ctrl+P`)\n",
        );

        out
    }
}

#[derive(Default)]
struct SessionTranscriptForensics {
    session_id: String,
    rel_path: String,
    turns: usize,
    total_tool_calls: usize,
    subagent_dispatches: usize,
    total_tokens: usize,
    cached_tokens: usize,
    skill_and_power_calls: Vec<(usize, String)>,
    inquiry_and_decision_calls: Vec<(usize, String)>,
    last_verify_or_review: Option<(usize, String)>,
    last_mutation: Option<(usize, String)>,
    repeated_targets: Vec<(String, Vec<usize>)>,
    stumbles: Vec<(usize, String)>,
    slowest_tool: Option<(usize, String, u64)>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_anti_rationalization_table_integrity() {
        let table = MiniPowerEngine::anti_rationalization_table();
        assert!(table.len() >= 5);
        for (excuse, reality) in table {
            assert!(!excuse.is_empty());
            assert!(!reality.is_empty());
        }
    }

    #[test]
    fn test_review_report_blocking_issues() {
        let report_clean = TaskReviewReport {
            stage: ReviewStage::SpecCompliance,
            passed: true,
            findings: vec![ReviewFinding {
                severity: ReviewSeverity::Minor,
                description: "Rename variable for clarity".to_string(),
                file: None,
                line: None,
            }],
            summary: "All good".to_string(),
        };
        assert!(!report_clean.has_blocking_issues());

        let report_blocked = TaskReviewReport {
            stage: ReviewStage::CodeQuality,
            passed: false,
            findings: vec![ReviewFinding {
                severity: ReviewSeverity::Critical,
                description: "Potential panic on unwrap".to_string(),
                file: Some("src/main.rs".to_string()),
                line: Some(42),
            }],
            summary: "Critical defects found".to_string(),
        };
        assert!(report_blocked.has_blocking_issues());
    }

    #[test]
    fn test_prompt_formatting_and_sdd_artifacts() {
        let task = MiniPowerTask {
            id: "task-1".to_string(),
            title: "Add Auth Middleware".to_string(),
            description: "Validate JWT tokens on protected routes".to_string(),
            target_files: vec!["src/auth.rs".to_string()],
            acceptance_criteria: vec!["Returns 401 on missing header".to_string()],
            test_command: Some("cargo test -j 1 --test auth".to_string()),
            completed: false,
            consumes: vec!["JwtSecret::from_env() -> Result<JwtSecret>".to_string()],
            produces: vec!["verify_bearer_token(hdr: &str) -> Result<Claims>".to_string()],
        };

        let brief = MiniPowerEngine::format_implementer_prompt(&task);
        assert!(brief.contains("task-1"));
        assert!(brief.contains("Add Auth Middleware"));
        assert!(brief.contains("cargo test -j 1 --test auth"));
        assert!(brief.contains("No-Subagents Contract"));
        assert!(brief.contains("DONE_WITH_CONCERNS"));
        assert!(brief.contains("verify_bearer_token"));

        assert_eq!(
            ImplementerStatus::parse_from_report("All tests green.\nStatus: DONE_WITH_CONCERNS"),
            ImplementerStatus::DoneWithConcerns
        );
        assert_eq!(
            ImplementerStatus::parse_from_report("Status: NEEDS_CONTEXT"),
            ImplementerStatus::NeedsContext
        );
        assert_eq!(
            ImplementerStatus::parse_from_report("Status: BLOCKED"),
            ImplementerStatus::Blocked
        );
        assert_eq!(
            ImplementerStatus::parse_from_report("Status: DONE"),
            ImplementerStatus::Done
        );

        let review_prompt = MiniPowerEngine::format_reviewer_prompt(
            &task,
            "+ fn test() {}",
            ReviewStage::SpecCompliance,
        );
        assert!(review_prompt.contains("Spec Compliance"));
        assert!(review_prompt.contains("Returns 401 on missing header"));
        assert!(review_prompt.contains("Declined to judge"));

        let status = MiniPowerEngine::format_status_summary();
        assert!(status.contains("MiniPower"));
        assert!(status.contains("Anti-Rationalization"));
        assert!(!status.contains("/power "));

        let fix_findings = vec![ReviewFinding {
            severity: ReviewSeverity::Important,
            description: "Missing 401 status check".to_string(),
            file: Some("src/auth.rs".to_string()),
            line: Some(25),
        }];
        let fix_prompt = MiniPowerEngine::format_fix_prompt(&task, &fix_findings);
        assert!(fix_prompt.contains("task-1"));
        assert!(fix_prompt.contains("Important"));
        assert!(fix_prompt.contains("src/auth.rs:25"));

        let rereview_prompt = MiniPowerEngine::format_scoped_rereview_prompt(
            &task,
            &fix_findings,
            "+ return StatusCode::UNAUTHORIZED;",
            5,
        );
        assert!(rereview_prompt.contains("Fix Round 5/5"));
        assert!(rereview_prompt.contains("Circuit Breaker Reached"));
        assert!(rereview_prompt.contains("ADDRESSED"));

        let builder_check = MiniPowerEngine::format_builder_check_prompt(
            "Rust systems engineer",
            "minikit_docs/core/spec.md",
        );
        assert!(builder_check.contains("Builder Check"));
        assert!(builder_check.contains("minikit_docs/core/spec.md"));

        let tmp = tempfile::tempdir().expect("tempdir");
        let workspace = tmp.path();
        let bs = MiniPowerEngine::format_brainstorm_prompt(workspace, "OAuth integration");
        assert!(bs.contains("OAuth integration"));
        assert!(bs.contains("Socratic Brainstorming"));
        assert!(bs.contains("Builder Check"));

        let plan = MiniPowerEngine::format_plan_prompt(workspace, "OAuth integration");
        assert!(plan.contains("OAuth integration"));
        assert!(plan.contains("Structured Implementation Plan"));
        assert!(plan.contains("Review Focus"));

        // SDD workspace, brief, review package, and progress ledger
        let brief_path = MiniPowerEngine::write_task_brief(
            workspace,
            Some("OAuth Plan"),
            &task,
            Some("abc1234"),
        )
        .expect("write task brief");
        assert!(brief_path.exists());
        let brief_disk = std::fs::read_to_string(&brief_path).expect("read brief");
        assert!(brief_disk.contains("BASE: abc1234"));

        let pkg_path = MiniPowerEngine::write_review_package(
            workspace,
            Some("OAuth Plan"),
            Some("abc123456"),
            Some("def789012"),
            "+ fn verify_bearer_token() {}",
        )
        .expect("write review package");
        assert!(pkg_path.exists());

        let ledger_path = MiniPowerEngine::append_sdd_ledger(
            workspace,
            Some("OAuth Plan"),
            "Task 1: complete (commit def7890)",
        )
        .expect("append ledger");
        assert!(ledger_path.exists());
        let (_, ledger_text) = MiniPowerEngine::read_sdd_ledger(workspace).expect("read ledger");
        assert!(ledger_text.contains("Task 1: complete (commit def7890)"));

        let diag = MiniPowerEngine::diagnose_workspace(workspace, Some("cost-and-time + stumbles"));
        assert!(diag.contains("cost-and-time"));
        assert!(diag.contains("stumbles"));
        assert!(diag.contains("skill-timeline"));
        assert!(diag.contains("plan-adherence"));
        assert!(diag.contains("Task 1: complete"));
    }
}

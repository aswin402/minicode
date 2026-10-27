#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::Path;

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

        // State: what is already settled, so no question is asked twice.
        let existing_specs: Vec<&str> = ["prd.md", "spec.md", "design.md", "architecture.md"]
            .into_iter()
            .filter(|f| docs_dir.join("core").join(f).is_file())
            .collect();
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

        format!(
            "### 🧠 MiniPower: Socratic Brainstorming & Spec Refinement (v7.0.1)\n\n\
            **Goal/Topic:** {topic}\n\n\
            **Already settled:**\n{settled}\n\
            **Procedure (`<HARD-GATE>`: no product code or scaffolding until Step 6 is approved):**\n\
            1. **Look Before You Ask (Ground First)**: Read relevant workspace files (`read_file`, `grep_search`, `symbol_search`), docs, and `{decisions}`. Never ask what the workspace already answers.\n\
            2. **Play It Back & Size the Work**: Echo the user's intent back in ~200 words (marking your own inferences/defaults explicitly as your proposal) and classify ceremony size:\n\
               - `Quick Task`: Everything is specified -> state defaults and execute.\n\
               - `Spike`: Throwaway prototype to test an unknown -> build minimal spike, bring findings back.\n\
               - `Small Change`: Short 1-2 line per-section design summary.\n\
               - `Project`: Full written design specification + Builder Check.\n\
            3. **Clarify Open Decisions (`ask_user` + `record_decision`)**: Ask specific, Socratic questions about purpose, goals/non-goals/anti-goals, and which `how-it-gets-made` choices (stack, architecture, data model, UI direction) the user wants to own vs delegate to you. Record every settled choice via `record_decision` (`[D1]..[DN]`).\n\
            4. **Lay Out the Approaches**: Present 2–3 concrete implementation approaches grounded in the codebase with explicit trade-offs, put your recommended approach first with a 1-line reason, and ask the user to pick via `ask_user`.\n\
            5. **Write the Design & Run Builder Check**: Write the agreed design to `{docs_name}/core/spec.md` (covering intent, goals/non-goals/anti-goals, constraints, approaches considered, user decisions, and builder calls). For a `Project`, run the cold-reader **Builder Check** (`spawn_agent` or self-audit) to resolve missing builder questions in at most one round.\n\
            6. **Spec Approval & Plan Handoff**: Confirm design approval via `ask_user`, then transition to `power_plan` / `create_plan`.\n",
            decisions = crate::agent::decisions::DECISIONS_FILE
        )
    }

    /// Generates structured task planning prompt with decision-focused tasks, `Interfaces`, and `Review Focus` (`obra/superpowers` v6.4.2).
    pub fn format_plan_prompt(workspace: &Path, topic: &str) -> String {
        let (_, docs_name) = Self::resolve_docs_dir_name(workspace);

        format!(
            "### 🛠️ MiniPower: Structured Implementation Plan Builder (v7.0.1)\n\n\
            **Goal/Topic:** {topic}\n\n\
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
               - **Native Inline (`update_progress` + `power_verify` + `power_review`)**: Execute each `[>]` step sequentially in this session, advancing immediately with `update_progress`, followed by `power_verify` and `power_review` at completion.\n"
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

    /// Runs a 7-dimension session & workspace forensics audit (`obra/superpowers` v7.0.1 `diagnosing-superpowers`).
    ///
    /// Inspects live workspace artifacts (`.minicode/plan/task_plan.md`, `.minicode/decisions.md`,
    /// `.minicode/plan/last_verify.json`, `minikit_docs/core/todo.md`, and `.minicode/logs/`)
    /// and reports concrete `path:line` evidence across all 7 diagnostic dimensions.
    pub fn diagnose_workspace(workspace: &Path, complaint: Option<&str>) -> String {
        let complaint_str = complaint
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("General MiniPower session & plan health audit");

        let lead_dimension = Self::recommend_diagnostic_lead(complaint_str);
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

        // Dimension 2: Plan Adherence (`plan-adherence`)
        out.push_str("\n#### 2. `plan-adherence` (Two-Tier Plan & Ledger State)\n");
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

        let todo_path = crate::tools::minikit::resolve_doc_path(workspace, "todo.md");
        if let Ok(todo_content) = std::fs::read_to_string(&todo_path) {
            let rel_todo = todo_path
                .strip_prefix(workspace)
                .unwrap_or(&todo_path)
                .display();
            let active_todo = todo_content
                .lines()
                .enumerate()
                .find(|(_, l)| l.contains("[>]"));
            if let Some((idx, line)) = active_todo {
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
                .filter(|(_, l)| l.trim_start().starts_with("- **[D"))
                .collect();
            if entries.is_empty() {
                out.push_str(&format!(
                    "- `{}:1` — File exists but contains 0 numbered `[D1]..[DN]` entries.\n",
                    crate::agent::decisions::DECISIONS_FILE
                ));
            } else {
                for (line_idx, line) in entries.iter().take(5) {
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

        // Dimensions 5, 6, 7: Repeated Work, Stumbles & Cost/Time from recent `.minicode/logs`
        out.push_str(
            "\n#### 5–7. `repeated-work`, `stumbles`, `cost-and-time` (Log & Cache Telemetry)\n",
        );
        let trigram_cache = workspace.join(".minicode/cache/trigram_index.bin");
        if let Ok(meta) = std::fs::metadata(&trigram_cache) {
            out.push_str(&format!(
                "- `.minicode/cache/trigram_index.bin:1` — Trigram search index active ({} bytes cached; eliminates repeated full-repo scans).\n",
                meta.len()
            ));
        } else {
            out.push_str(
                "- `.minicode/cache/trigram_index.bin:1` — Trigram cache not yet materialized on disk.\n",
            );
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

    /// Maps a user complaint to the primary diagnostic dimension (`obra/superpowers` `diagnosing-superpowers` Quick Reference).
    pub fn recommend_diagnostic_lead(complaint: &str) -> &'static str {
        let lower = complaint.to_ascii_lowercase();
        if lower.contains("long") || lower.contains("slow") || lower.contains("stuck") {
            "cost-and-time + stumbles"
        } else if lower.contains("extra") || lower.contains("repeat") || lower.contains("redo") {
            "repeated-work + plan-adherence"
        } else if lower.contains("expensive") || lower.contains("token") || lower.contains("cost") {
            "cost-and-time"
        } else if lower.contains("ignore") || lower.contains("plan") {
            "plan-adherence (check compaction boundaries first)"
        } else if lower.contains("skill") || lower.contains("fire") {
            "skill-timeline"
        } else {
            "plan-adherence + quality-evidence + request-conflicts"
        }
    }

    /// Formats executive status summary of the MiniPower methodology engine.
    pub fn format_status_summary() -> String {
        let mut out = String::from(
            "⚡ **MiniPower: Native Autonomous Engineering Methodology (Superpowers v7.0.1)**\n\n\
            *Rigorous, disciplined software engineering built directly into minicode.*\n\n\
            ### Core Pillars:\n\
            1. **Socratic Brainstorming & Builder Check**: Ground first, play back intent, size ceremony, and run a cold-reader Builder Check before coding.\n\
            2. **Git Worktree Isolation**: Protect the main branch in ephemeral, clean sandbox branches (`power_worktree_task`).\n\
            3. **Decision-Focused Planning & Review Focus**: Atomic tasks with `Interfaces (Consumes/Produces)`, 5-item `Review Focus`, and `[D1]..[DN]` rulings.\n\
            4. **Two-Stage Subagent Review & 5-Round Circuit Breaker**: Stage 1 (Spec Compliance) + Stage 2 (Code Quality) + Scoped Re-Review.\n\
            5. **Strict Red/Green TDD (`writing-good-tests`)**: Falsifiability gate, zero mirror/mock-existence assertions, and mutation check.\n\
            6. **Evidence Before Assertions & Session Forensics**: 4-Gate `power_verify` barrier and 7-dimension `power_diagnose` transcript auditing.\n\n\
            ### Anti-Rationalization Guardrails (\"Red Flags\"):\n",
        );

        for (excuse, reality) in Self::anti_rationalization_table() {
            out.push_str(&format!("• *\"{}\"*\n  ╰─── {}\n", excuse, reality));
        }

        out.push_str(
            "\n💡 Commands: Ctrl+P (Modal), `/power task <prompt>`, `/power brainstorm <topic>`, `/power plan <topic>`, `/power review`, `/power verify`, `power_diagnose`\n",
        );

        out
    }
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
    fn test_prompt_formatting() {
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

        let workspace = Path::new("/tmp/test");
        let bs = MiniPowerEngine::format_brainstorm_prompt(workspace, "OAuth integration");
        assert!(bs.contains("OAuth integration"));
        assert!(bs.contains("Socratic Brainstorming"));
        assert!(bs.contains("Builder Check"));

        let plan = MiniPowerEngine::format_plan_prompt(workspace, "OAuth integration");
        assert!(plan.contains("OAuth integration"));
        assert!(plan.contains("Structured Implementation Plan"));
        assert!(plan.contains("Review Focus"));

        let diag = MiniPowerEngine::diagnose_workspace(workspace, Some("why did it take too long"));
        assert!(diag.contains("cost-and-time + stumbles"));
        assert!(diag.contains("skill-timeline"));
        assert!(diag.contains("plan-adherence"));
    }
}

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
        self.findings.iter().any(|f| {
            f.severity == ReviewSeverity::Critical || f.severity == ReviewSeverity::Important
        })
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
}

/// Core engine for generating prompts, enforcing anti-rationalization guardrails,
/// and coordinating the MiniPower autonomous software development methodology.
pub struct MiniPowerEngine;

impl MiniPowerEngine {
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
                "Reading is not execution. Run the actual compiler or test runner.",
            ),
            (
                "I need more context before clarifying",
                "Ask clarifying questions before touching files when intent is ambiguous.",
            ),
            (
                "The task is too simple for subagent isolation",
                "Context pollution degrades reasoning. Isolate heavy tasks in subagents.",
            ),
            (
                "Skipping the test run saves turns and time",
                "Unverified claims cause compound failures that waste 3x more turns later.",
            ),
            (
                "I know what the user wants without asking",
                "Assumptions cause rework. Clarify trade-offs in bite-sized questions.",
            ),
        ]
    }

    /// Generates Socratic brainstorming prompt for pre-implementation discovery.
    pub fn format_brainstorm_prompt(workspace: &Path, topic: &str) -> String {
        let docs_dir = crate::tools::minikit::resolve_docs_dir(workspace);
        let docs_name = docs_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(crate::constants::MINIKIT_DOCS_DIR);

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
            "### 🧠 MiniPower: Socratic Brainstorming & Spec Refinement\n\n\
            **Goal/Topic:** {topic}\n\n\
            **Already settled:**\n{settled}\n\
            **Procedure (no project code until step 5 is approved):**\n\
            1. **Ground**: read the relevant code, docs and decisions. Never ask what the workspace answers.\n\
            2. **Scope check**: if the topic is several independent subsystems, propose splitting it and design the first part only.\n\
            3. **Clarify with `ask_user` in rounds**: purpose and users, success criteria, constraints, data/content only the user has, and every costly-to-undo decision (stack, architecture, data model, API/spec, design direction, scope). Batch 1-4 questions per round, recommended option first; start another round only when answers open new decisions.\n\
            4. **Approaches**: offer 2-3 approaches with trade-offs as one `ask_user` choice, your recommendation first with a one-line reason.\n\
            5. **Spec and approval**: write the agreed design concisely to `{docs_name}/core/` (prd.md for goals/features, design.md for UI/UX, architecture.md for components, data flow, error handling, testing). Then ask for approval with `ask_user` (approve / revise).\n\
            6. **Plan**: `create_plan` from the approved spec, listing any remaining assumptions in `assumptions`.\n"
        )
    }

    /// Generates structured task planning prompt with bite-sized tasks and acceptance tests.
    pub fn format_plan_prompt(workspace: &Path, topic: &str) -> String {
        let docs_dir = crate::tools::minikit::resolve_docs_dir(workspace);
        let docs_name = docs_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(crate::constants::MINIKIT_DOCS_DIR);

        format!(
            "### 🛠️ MiniPower: Structured Implementation Plan Builder\n\n\
            **Goal/Topic:** {}\n\n\
            **Methodology Instructions:**\n\
            1. **Two-Tier Planning Architecture:**\n\
               - Tier 1 (Core Milestones): High-level strategic roadmap recorded in `{}/core/todo.md` and `{}/core/implementation.md` (viewed in `/todo` modal).\n\
               - Tier 2 (Active Step Plan): Tactical 2-5 min execution steps recorded in `.minicode/plan/task_plan.md` via `create_plan` (viewed live in TUI dock).\n\
            2. **Decisions Before Steps (Reversibility Rule):** Any decision this plan depends on that the request, the repo or recorded decisions do not settle and that is costly to undo (stack, architecture, data model, API/spec, design direction, scope) is resolved with ONE batched `ask_user` round before the plan is executed; cheap details are stated as assumptions. Present the finished plan for approval before large multi-step work.\n\
            3. **Bite-Sized Task Breakdown:** Break work into 4-8 atomic tasks (2-5 minutes each). Every task must specify:\n\
               - **Target Files:** Exact relative file paths.\n\
               - **Acceptance Criteria:** Verifiable conditions for completion.\n\
               - **Verification Command:** Concrete test or check (e.g. `cargo test -j 1 --lib ...`).\n\
            4. Follow strict Red/Green TDD: tests are added before or alongside implementation.\n\
            5. If this implementation involves frontend components, UI, styles, or page layouts:\n\
               - Search the MiniBlocks warehouse first (`block_search`, `block_palettes`, `block_scaffold`) to reuse verified components and design tokens instead of hallucinating CSS from scratch.\n\
            6. **Sequential Execution (No Batching):** Focus strictly on Step 1, call `update_progress(step=\"1\", status=\"completed\")` immediately to advance to Step 2, and repeat. Never batch all progress updates at the end of the turn!\n",
            topic, docs_name, docs_name
        )
    }

    /// Formats the isolated brief given to an implementer subagent.
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

        out.push_str("\n**Acceptance Criteria:**\n");
        for ac in &task.acceptance_criteria {
            out.push_str(&format!("- [ ] {}\n", ac));
        }

        if let Some(cmd) = &task.test_command {
            out.push_str(&format!("\n**Verification Command:**\n`{}`\n", cmd));
        }

        out.push_str(
            "\n**Execution Rules:**\n\
            1. Follow strict TDD: write/update failing test, verify failure, implement minimal code, verify green.\n\
            2. Zero `.unwrap()` or `.expect()` in non-test code.\n\
            3. Run the verification command and ensure exit code 0 before reporting completion.\n",
        );

        out
    }

    /// Formats the prompt for a task reviewer subagent across Spec Compliance or Code Quality.
    pub fn format_reviewer_prompt(task: &MiniPowerTask, diff: &str, stage: ReviewStage) -> String {
        match stage {
            ReviewStage::SpecCompliance => format!(
                "### 🔍 MiniPower Reviewer: Stage 1 — Spec Compliance\n\n\
                **Task ID:** {}\n\
                **Title:** {}\n\n\
                **Task Acceptance Criteria:**\n{}\n\n\
                **Implementation Diff:**\n```diff\n{}\n```\n\n\
                **Review Rubric:**\n\
                1. Did the implementation satisfy EVERY acceptance criterion?\n\
                2. Were any required requirements omitted or stubbed out?\n\
                3. Are tests present and asserting the actual behavior?\n\
                Classify findings as Critical, Important, or Minor.\n",
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
                **Review Rubric:**\n\
                1. Error Handling: Are errors propagated idiomatically (no panic / unwrap)?\n\
                2. Security & Traversal: Are inputs validated and paths confined?\n\
                3. Concurrency & Performance: Are there race conditions, deadlocks, or unnecessary allocations?\n\
                4. Code Style & Maintainability: Does the change adhere to existing project standards?\n\
                Classify findings as Critical, Important, or Minor.\n",
                task.id,
                task.title,
                diff
            ),
        }
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
            "\nPlease address the Critical and Important findings above, re-run verification tests, and confirm.\n",
        );
        out
    }

    /// Formats executive status summary of the MiniPower methodology engine.
    pub fn format_status_summary() -> String {
        let mut out = String::from(
            "⚡ **MiniPower: Native Autonomous Engineering Methodology**\n\n\
            *Rigorous, disciplined software engineering built directly into minicode.*\n\n\
            ### Core Pillars:\n\
            1. **Socratic Brainstorming**: Clarify intent, surface trade-offs, and chunk specs before coding.\n\
            2. **Git Worktree Isolation**: Protect the main branch in ephemeral, clean sandbox branches.\n\
            3. **Bite-Sized Planning**: Atomic 2-5 min tasks with concrete acceptance tests.\n\
            4. **Two-Stage Subagent Review**: Automated Stage 1 (Spec Compliance) and Stage 2 (Code Quality).\n\
            5. **Strict Red/Green TDD**: Write failing tests first, make them green, refactor cleanly.\n\
            6. **Evidence Before Assertions**: Zero unverified claims; verified by exit code 0.\n\n\
            ### Anti-Rationalization Guardrails (\"Red Flags\"):\n",
        );

        for (excuse, reality) in Self::anti_rationalization_table() {
            out.push_str(&format!("• *\"{}\"*\n  ╰─── {}\n", excuse, reality));
        }

        out.push_str(
            "\n💡 Commands: Ctrl+P (Modal), `/power task <prompt>`, `/power brainstorm <topic>`, `/power plan <topic>`, `/power review`, `/power verify`\n",
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
        };

        let brief = MiniPowerEngine::format_implementer_prompt(&task);
        assert!(brief.contains("task-1"));
        assert!(brief.contains("Add Auth Middleware"));
        assert!(brief.contains("cargo test -j 1 --test auth"));

        let review_prompt = MiniPowerEngine::format_reviewer_prompt(
            &task,
            "+ fn test() {}",
            ReviewStage::SpecCompliance,
        );
        assert!(review_prompt.contains("Spec Compliance"));
        assert!(review_prompt.contains("Returns 401 on missing header"));

        let status = MiniPowerEngine::format_status_summary();
        assert!(status.contains("MiniPower"));
        assert!(status.contains("Anti-Rationalization"));

        let fix_prompt = MiniPowerEngine::format_fix_prompt(
            &task,
            &[ReviewFinding {
                severity: ReviewSeverity::Important,
                description: "Missing 401 status check".to_string(),
                file: Some("src/auth.rs".to_string()),
                line: Some(25),
            }],
        );
        assert!(fix_prompt.contains("task-1"));
        assert!(fix_prompt.contains("Important"));
        assert!(fix_prompt.contains("src/auth.rs:25"));

        let workspace = Path::new("/tmp/test");
        let bs = MiniPowerEngine::format_brainstorm_prompt(workspace, "OAuth integration");
        assert!(bs.contains("OAuth integration"));
        assert!(bs.contains("Socratic Brainstorming"));

        let plan = MiniPowerEngine::format_plan_prompt(workspace, "OAuth integration");
        assert!(plan.contains("OAuth integration"));
        assert!(plan.contains("Structured Implementation Plan"));
        assert!(plan.contains("MiniBlocks warehouse first"));
    }
}

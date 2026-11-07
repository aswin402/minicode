//! Scoped 3-Stage Bug Triage State Machine (`bug.assess` → `bug.fix` → `bug.test`)
//! and 5-Stage Evidence-Weighted Idea Assessment Scorecard (`intake` → `research` → `define` → `shape` → `decide`).
//!
//! Uses set algebra ($\text{ModifiedFiles} \setminus \text{AssessedFiles}$) for deviation detection,
//! declarative truth tables for bug verification verdicts, and multi-criteria weighted scorecards
//! for pre-spec idea assessment.

use crate::agent::minipower::MiniPowerEngine;
use crate::constants::{
    SPECKIT_AMBIGUITY_MARKERS, SPECKIT_ASSESSMENTS_DIR, SPECKIT_ASSESSMENT_MAX_SCORE,
    SPECKIT_ASSESSMENT_STAGE_FILES, SPECKIT_BUGS_DIR, SPECKIT_COUNTER_EVIDENCE_BONUS,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fmt;
use std::path::{Path, PathBuf};

// ============================================================================
// 1. Scoped 3-Stage Bug Triage State Machine (`bug.assess` -> `bug.fix` -> `bug.test`)
// ============================================================================

/// Stage in the SpecKit 3-stage Bugfix workflow (`docs/reference/agentic-bugfix.md`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BugStage {
    /// Stage 1 (`assessment.md`): Read-only root-cause investigation, reproduction command, and target file boundary.
    Assess,
    /// Stage 2 (`fix.md`): Scoped implementation tracking file boundary adherence and `## Deviations from Assessment`.
    Fix,
    /// Stage 3 (`test.md`): Verification against the reproduction and regression suite (`VERIFIED | PARTIAL | FAILED`).
    Test,
}

impl BugStage {
    pub fn from_str_name(s: &str) -> Self {
        const STAGE_TABLE: &[(&str, BugStage)] = &[
            ("assess", BugStage::Assess),
            ("assessment", BugStage::Assess),
            ("fix", BugStage::Fix),
            ("test", BugStage::Test),
            ("verify", BugStage::Test),
        ];
        let lower = s.trim().to_ascii_lowercase();
        STAGE_TABLE
            .iter()
            .find_map(|&(k, stage)| (k == lower).then_some(stage))
            .unwrap_or(BugStage::Assess)
    }
}

/// Three-tier verification verdict for `bug.test` (`docs/reference/agentic-bugfix.md`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BugVerdict {
    /// Both the reproduction and the test suite were exercised and passed (`VERIFIED`).
    Verified,
    /// Tests passed, or fix appears sound, but the original reproduction was NOT exercised (`PARTIAL`).
    Partial,
    /// Tests failed or the defect still reproduces (`FAILED`).
    Failed,
}

impl fmt::Display for BugVerdict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Verified => write!(f, "VERIFIED"),
            Self::Partial => write!(f, "PARTIAL"),
            Self::Failed => write!(f, "FAILED"),
        }
    }
}

/// Declarative truth-table row mapping `(tests_passed, repro_exercised)` to `BugVerdict`.
struct BugVerdictRow {
    tests_passed: bool,
    repro_exercised: bool,
    verdict: BugVerdict,
    explanation: &'static str,
}

const BUG_VERDICT_TRUTH_TABLE: &[BugVerdictRow] = &[
    BugVerdictRow {
        tests_passed: true,
        repro_exercised: true,
        verdict: BugVerdict::Verified,
        explanation: "Original reproduction was exercised and resolved; regression tests passed.",
    },
    BugVerdictRow {
        tests_passed: true,
        repro_exercised: false,
        verdict: BugVerdict::Partial,
        explanation: "Tests passed, but original reproduction was NOT exercised — strictly downgraded to PARTIAL per SpecKit bugfix invariant.",
    },
    BugVerdictRow {
        tests_passed: false,
        repro_exercised: true,
        verdict: BugVerdict::Failed,
        explanation: "Reproduction or regression test suite failed.",
    },
    BugVerdictRow {
        tests_passed: false,
        repro_exercised: false,
        verdict: BugVerdict::Failed,
        explanation: "Neither reproduction nor test suite passed.",
    },
];

/// Persistent state for a scoped bugfix investigation under `.minicode/sdd/bugs/<slug>/bug_state.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BugWorkflowState {
    pub slug: String,
    pub title: String,
    pub stage: BugStage,
    pub root_cause: String,
    pub reproduction_command: Option<String>,
    pub assessed_target_files: Vec<String>,
    pub modified_files: Vec<String>,
    pub out_of_scope_deviations: Vec<String>,
    pub deviation_rationale: Option<String>,
    pub verdict: Option<BugVerdict>,
    pub verdict_explanation: Option<String>,
    pub artifact_path: PathBuf,
}

impl BugWorkflowState {
    pub fn format_markdown(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "### 🐛 SpecKit Bug Triage (`{}` — Stage: `{:?}`)\n\n",
            self.title, self.stage
        ));
        out.push_str(&format!(
            "- **Artifact:** `{}`\n- **Root Cause:** {}\n- **Assessed Target Scope:** `{}`\n",
            self.artifact_path.display(),
            if self.root_cause.is_empty() {
                "(pending assessment)"
            } else {
                &self.root_cause
            },
            if self.assessed_target_files.is_empty() {
                "(none specified)".to_string()
            } else {
                self.assessed_target_files.join("`, `")
            }
        ));
        if !self.out_of_scope_deviations.is_empty() {
            out.push_str(&format!(
                "- **⚠️ Deviations from Assessment (`Modified \\ Assessed`):** `{}`\n",
                self.out_of_scope_deviations.join("`, `")
            ));
        }
        if let Some(verdict) = self.verdict {
            out.push_str(&format!(
                "- **Verification Verdict:** **`{}`** — {}\n",
                verdict,
                self.verdict_explanation.as_deref().unwrap_or_default()
            ));
        }
        out
    }
}

/// Input parameters for advancing or recording a stage in the 3-stage Bug Triage workflow.
#[derive(Debug, Clone, Default)]
pub struct BugStageInput {
    pub title: String,
    pub stage: Option<BugStage>,
    pub root_cause: Option<String>,
    pub reproduction_command: Option<String>,
    pub target_files: Vec<String>,
    pub modified_files: Vec<String>,
    pub fix_summary: Option<String>,
    pub deviation_rationale: Option<String>,
    pub repro_exercised: Option<bool>,
    pub tests_passed: Option<bool>,
}

/// Advances or records a stage in the Scoped 3-Stage Bug Triage State Machine (`bug.assess` -> `bug.fix` -> `bug.test`).
pub fn execute_bug_workflow_stage(
    workspace_root: &Path,
    input: BugStageInput,
) -> std::io::Result<BugWorkflowState> {
    let slug = MiniPowerEngine::sanitize_slug(&input.title);
    let bug_dir = workspace_root.join(SPECKIT_BUGS_DIR).join(&slug);
    std::fs::create_dir_all(&bug_dir)?;

    let state_file = bug_dir.join("bug_state.json");
    let mut state: BugWorkflowState = std::fs::read_to_string(&state_file)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_else(|| BugWorkflowState {
            slug: slug.clone(),
            title: input.title.clone(),
            stage: BugStage::Assess,
            root_cause: String::new(),
            reproduction_command: None,
            assessed_target_files: Vec::new(),
            modified_files: Vec::new(),
            out_of_scope_deviations: Vec::new(),
            deviation_rationale: None,
            verdict: None,
            verdict_explanation: None,
            artifact_path: bug_dir.join("assessment.md"),
        });

    let active_stage = input.stage.unwrap_or(state.stage);
    state.stage = active_stage;

    if let Some(rc) = input.root_cause.filter(|s| !s.trim().is_empty()) {
        state.root_cause = rc;
    }
    if let Some(repro) = input.reproduction_command.filter(|s| !s.trim().is_empty()) {
        state.reproduction_command = Some(repro);
    }
    if !input.target_files.is_empty() {
        state.assessed_target_files = input.target_files;
    }
    if !input.modified_files.is_empty() {
        state.modified_files = input.modified_files;
    }
    if let Some(rationale) = input.deviation_rationale.filter(|s| !s.trim().is_empty()) {
        state.deviation_rationale = Some(rationale);
    }

    match active_stage {
        BugStage::Assess => {
            let assessment_path = bug_dir.join("assessment.md");
            let mut md = format!("# Bug Assessment: {}\n\n", state.title);
            md.push_str("## Root Cause Analysis\n");
            let rc_line = if state.root_cause.is_empty() {
                "Pending root-cause diagnosis.\n\n".to_string()
            } else {
                format!("{}\n\n", state.root_cause)
            };
            md.push_str(&rc_line);
            md.push_str("## Reproduction Command\n");
            md.push_str(&format!(
                "`{}`\n\n",
                state
                    .reproduction_command
                    .as_deref()
                    .unwrap_or("cargo test")
            ));
            md.push_str("## Assessed Target Files (Allowed Scope)\n");
            for f in &state.assessed_target_files {
                md.push_str(&format!("- `{}`\n", f));
            }
            std::fs::write(&assessment_path, md)?;
            state.artifact_path = assessment_path;
        }
        BugStage::Fix => {
            let assessed_set: BTreeSet<&str> = state
                .assessed_target_files
                .iter()
                .map(String::as_str)
                .collect();
            let modified_set: BTreeSet<&str> =
                state.modified_files.iter().map(String::as_str).collect();

            // Set difference: Deviations = ModifiedFiles \ AssessedFiles
            let deviations: Vec<String> = if assessed_set.is_empty() {
                Vec::new()
            } else {
                modified_set
                    .difference(&assessed_set)
                    .map(|s| (*s).to_string())
                    .collect()
            };
            state.out_of_scope_deviations = deviations;

            let fix_path = bug_dir.join("fix.md");
            let mut md = format!("# Bug Fix Implementation: {}\n\n", state.title);
            md.push_str("## Summary of Changes\n");
            md.push_str(
                input
                    .fix_summary
                    .as_deref()
                    .unwrap_or("Applied scoped defect fix."),
            );
            md.push_str("\n\n## Modified Files\n");
            for f in &state.modified_files {
                md.push_str(&format!("- `{}`\n", f));
            }
            if !state.out_of_scope_deviations.is_empty() {
                md.push_str("\n## Deviations from Assessment\n");
                for dev in &state.out_of_scope_deviations {
                    md.push_str(&format!(
                        "- Out-of-scope file modified (`{}`): {}\n",
                        dev,
                        state
                            .deviation_rationale
                            .as_deref()
                            .unwrap_or("Required to satisfy transitive caller/type constraints.")
                    ));
                }
            }
            std::fs::write(&fix_path, md)?;
            state.artifact_path = fix_path;
        }
        BugStage::Test => {
            let tests_passed = input.tests_passed.unwrap_or(false);
            let repro_exercised = input.repro_exercised.unwrap_or(false);
            let row = BUG_VERDICT_TRUTH_TABLE
                .iter()
                .find(|r| r.tests_passed == tests_passed && r.repro_exercised == repro_exercised)
                .unwrap_or(&BUG_VERDICT_TRUTH_TABLE[3]);

            state.verdict = Some(row.verdict);
            state.verdict_explanation = Some(row.explanation.to_string());

            let test_path = bug_dir.join("test.md");
            let mut md = format!("# Bug Verification Report: {}\n\n", state.title);
            md.push_str(&format!(
                "- **Verdict:** **`{}`**\n- **Reproduction Exercised:** `{}`\n- **Test Suite Passed:** `{}`\n- **Rationale:** {}\n",
                row.verdict, repro_exercised, tests_passed, row.explanation
            ));
            std::fs::write(&test_path, md)?;
            state.artifact_path = test_path;
        }
    }

    if let Ok(json) = serde_json::to_string_pretty(&state) {
        let _ = std::fs::write(&state_file, json);
    }

    Ok(state)
}

// ============================================================================
// 2. 5-Stage Evidence-Weighted Idea Assessment Scorecard (`intake`..`decide`)
// ============================================================================

/// Stage in the SpecKit 5-stage Pre-Spec Idea Assessment workflow (`docs/reference/agentic-assessment.md`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssessmentStage {
    /// Stage 1 (`intake.md`): Capture raw idea, goals, and `[ASSUMPTION]` items.
    Intake,
    /// Stage 2 (`research.md`): Gather supporting AND counter-evidence with confidence ratings.
    Research,
    /// Stage 3 (`problem.md`): Frame evidence-backed problem statement.
    Define,
    /// Stage 4 (`concept.md`): Shape high-level solution concept and non-goals.
    Shape,
    /// Stage 5 (`decision.md`): Compute deterministic Go / Needs-Clarification / Kill recommendation.
    Decide,
}

impl AssessmentStage {
    pub fn from_str_name(s: &str) -> Self {
        const TABLE: &[(&str, AssessmentStage)] = &[
            ("intake", AssessmentStage::Intake),
            ("research", AssessmentStage::Research),
            ("define", AssessmentStage::Define),
            ("problem", AssessmentStage::Define),
            ("shape", AssessmentStage::Shape),
            ("concept", AssessmentStage::Shape),
            ("decide", AssessmentStage::Decide),
            ("decision", AssessmentStage::Decide),
        ];
        let lower = s.trim().to_ascii_lowercase();
        TABLE
            .iter()
            .find_map(|&(k, stage)| (k == lower).then_some(stage))
            .unwrap_or(AssessmentStage::Decide)
    }

    pub fn artifact_filename(self) -> &'static str {
        match self {
            Self::Intake => "intake.md",
            Self::Research => "research.md",
            Self::Define => "problem.md",
            Self::Shape => "concept.md",
            Self::Decide => "decision.md",
        }
    }
}

/// Ordinal rating (`STRONG=3`, `ADEQUATE=2`, `WEAK=1`, `UNKNOWN=0`) for assessment criteria.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AssessmentRating {
    Unknown = 0,
    Weak = 1,
    Adequate = 2,
    Strong = 3,
}

impl Default for AssessmentRating {
    fn default() -> Self {
        Self::Adequate
    }
}

impl AssessmentRating {
    pub fn from_str_name(s: &str) -> Self {
        const RATING_TABLE: &[(&str, AssessmentRating)] = &[
            ("strong", AssessmentRating::Strong),
            ("high", AssessmentRating::Strong),
            ("adequate", AssessmentRating::Adequate),
            ("medium", AssessmentRating::Adequate),
            ("weak", AssessmentRating::Weak),
            ("low", AssessmentRating::Weak),
            ("unknown", AssessmentRating::Unknown),
            ("none", AssessmentRating::Unknown),
        ];
        let lower = s.trim().to_ascii_lowercase();
        RATING_TABLE
            .iter()
            .find_map(|&(k, rating)| (k == lower).then_some(rating))
            .unwrap_or(AssessmentRating::Adequate)
    }

    pub fn weight(self) -> u8 {
        self as u8
    }
}

/// Final recommendation of the 5-stage Idea Assessment workflow (`decision.md`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AssessmentDecision {
    /// Proceed to `speckit` specification (`minikit_docs/core/prd.md` & `spec.md`).
    Go,
    /// Unresolved `[ASSUMPTION]` / `[NEEDS CLARIFICATION]` markers or `Unknown` criteria require user input.
    NeedsClarification,
    /// Problem validity or evidence is `Weak` — stop before wasting engineering cycles.
    Kill,
}

impl fmt::Display for AssessmentDecision {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Go => write!(f, "GO"),
            Self::NeedsClarification => write!(f, "NEEDS_CLARIFICATION"),
            Self::Kill => write!(f, "KILL"),
        }
    }
}

/// Scorecard state persisted under `.minicode/sdd/assessments/<slug>/assessment_state.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdeaAssessmentReport {
    pub slug: String,
    pub topic: String,
    pub stage: AssessmentStage,
    pub problem_validity: AssessmentRating,
    pub evidence_strength: AssessmentRating,
    pub counter_evidence_documented: bool,
    pub concept_viability: AssessmentRating,
    pub unresolved_assumptions: Vec<String>,
    pub weighted_score: u8,
    pub max_score: u8,
    pub decision: AssessmentDecision,
    pub rationale: String,
    pub artifact_path: PathBuf,
}

impl IdeaAssessmentReport {
    pub fn format_markdown(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "### 🧭 SpecKit Idea Assessment (`{}` — Decision: **`{}`**)\n\n",
            self.topic, self.decision
        ));
        out.push_str(&format!(
            "- **Stage:** `{:?}` (`{}`)\n- **Weighted Score:** `{}/{}`\n- **Problem Validity:** `{:?}` | **Evidence Strength:** `{:?}` | **Concept Viability:** `{:?}` | **Counter-Evidence Documented:** `{}`\n- **Unresolved Assumptions:** `{}`\n- **Rationale:** {}\n",
            self.stage,
            self.artifact_path.display(),
            self.weighted_score,
            self.max_score,
            self.problem_validity,
            self.evidence_strength,
            self.concept_viability,
            self.counter_evidence_documented,
            self.unresolved_assumptions.len(),
            self.rationale
        ));
        out
    }
}

/// Input payload for executing an Idea Assessment stage (`intake`, `research`, `define`, `shape`, `decide`).
#[derive(Debug, Clone, Default)]
pub struct IdeaAssessmentInput {
    pub topic: String,
    pub stage: Option<AssessmentStage>,
    pub content: Option<String>,
    pub problem_validity: Option<AssessmentRating>,
    pub evidence_strength: Option<AssessmentRating>,
    pub counter_evidence_documented: Option<bool>,
    pub concept_viability: Option<AssessmentRating>,
    pub assumptions: Vec<String>,
}

/// Evaluates and persists a stage in the 5-stage Evidence-Weighted Idea Assessment workflow.
pub fn execute_idea_assessment(
    workspace_root: &Path,
    input: IdeaAssessmentInput,
) -> std::io::Result<IdeaAssessmentReport> {
    let slug = MiniPowerEngine::sanitize_slug(&input.topic);
    let assess_dir = workspace_root.join(SPECKIT_ASSESSMENTS_DIR).join(&slug);
    std::fs::create_dir_all(&assess_dir)?;

    let stage = input.stage.unwrap_or(AssessmentStage::Decide);
    let artifact_path = assess_dir.join(stage.artifact_filename());

    if let Some(body) = input.content.as_deref().filter(|s| !s.trim().is_empty()) {
        let header = format!("# {} — {:?}\n\n{}\n", input.topic, stage, body.trim());
        std::fs::write(&artifact_path, header)?;
    }

    // Scan all persisted stage artifacts in `assess_dir` for unresolved `[ASSUMPTION]` / `[NEEDS CLARIFICATION]` markers
    let mut unresolved_assumptions: Vec<String> = input.assumptions;
    for &filename in SPECKIT_ASSESSMENT_STAGE_FILES {
        if let Ok(text) = std::fs::read_to_string(assess_dir.join(filename)) {
            for line in text.lines() {
                let upper = line.to_ascii_uppercase();
                if SPECKIT_AMBIGUITY_MARKERS.iter().any(|m| upper.contains(m)) {
                    unresolved_assumptions.push(format!("{}: {}", filename, line.trim()));
                }
            }
        }
    }

    let problem_validity = input.problem_validity.unwrap_or(AssessmentRating::Adequate);
    let evidence_strength = input
        .evidence_strength
        .unwrap_or(AssessmentRating::Adequate);
    let concept_viability = input
        .concept_viability
        .unwrap_or(AssessmentRating::Adequate);
    let counter_evidence_documented = input.counter_evidence_documented.unwrap_or(true);

    let counter_bonus: u8 = match counter_evidence_documented {
        true => SPECKIT_COUNTER_EVIDENCE_BONUS,
        false => 0,
    };
    let weighted_score = problem_validity.weight()
        + evidence_strength.weight()
        + concept_viability.weight()
        + counter_bonus;
    let max_score = SPECKIT_ASSESSMENT_MAX_SCORE;

    let core_ratings = [problem_validity, evidence_strength, concept_viability];
    let has_weak_core = core_ratings.contains(&AssessmentRating::Weak);
    let has_unknown_core = core_ratings.contains(&AssessmentRating::Unknown);
    let needs_clarification =
        !unresolved_assumptions.is_empty() || has_unknown_core || !counter_evidence_documented;

    let (decision, rationale) = match (has_weak_core, needs_clarification) {
        (true, _) => (
            AssessmentDecision::Kill,
            "One or more core validation criteria (problem validity, evidence strength, or concept viability) rated WEAK — recommend stopping before specification."
                .to_string(),
        ),
        (false, true) => (
            AssessmentDecision::NeedsClarification,
            format!(
                "Assessment has {} unresolved assumption/clarification marker(s), unknown criteria, or missing counter-evidence review.",
                unresolved_assumptions.len()
            ),
        ),
        (false, false) => (
            AssessmentDecision::Go,
            format!(
                "Problem validity, supporting/counter-evidence, and concept viability met threshold ({}/{} with 0 unresolved assumptions) — ready for specification.",
                weighted_score, max_score
            ),
        ),
    };

    let report = IdeaAssessmentReport {
        slug,
        topic: input.topic,
        stage,
        problem_validity,
        evidence_strength,
        counter_evidence_documented,
        concept_viability,
        unresolved_assumptions,
        weighted_score,
        max_score,
        decision,
        rationale,
        artifact_path: artifact_path.clone(),
    };

    if stage == AssessmentStage::Decide || !artifact_path.exists() {
        let _ = std::fs::write(&artifact_path, report.format_markdown());
    }
    if let Ok(json) = serde_json::to_string_pretty(&report) {
        let _ = std::fs::write(assess_dir.join("assessment_state.json"), json);
    }

    Ok(report)
}

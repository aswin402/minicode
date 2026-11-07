//! Bipartite & Multipartite Cross-Artifact Traceability Graph (`speckit.analyze`),
//! Project Constitution Rule Extractor (`speckit.constitution`),
//! Traceable Tasks-to-Issues Synthesizer (`speckit.taskstoissues`),
//! and Requirements-Quality Checklist Evaluator (`speckit.checklist` — "Unit Tests for Requirements").
//!
//! Uses `pulldown-cmark` AST event streams, morphological stem normalization,
//! Inverse Document Frequency ($\text{IDF}(t) = \ln(1 + N / (1 + \text{df}(t)))$),
//! and `petgraph::graph::DiGraph` reachability without regular expressions or `if-else` ladders.

use crate::constants::{
    SPECKIT_ACCEPTANCE_CUES, SPECKIT_AMBIGUITY_MARKERS, SPECKIT_AMBIGUITY_PENALTY_PER_HIT,
    SPECKIT_CHECKLIST_DIMENSION_WEIGHT, SPECKIT_CHECKLIST_PASS_RATIO_DEN,
    SPECKIT_CHECKLIST_PASS_RATIO_NUM, SPECKIT_CHECKLIST_PASS_SCORE, SPECKIT_CHECKLIST_REL_PATH,
    SPECKIT_CONSTITUTION_FILES, SPECKIT_FILE_OVERLAP_BONUS, SPECKIT_MIN_STEM_LEN,
    SPECKIT_NON_GOAL_CUES, SPECKIT_ORPHAN_EXEMPT_STEMS, SPECKIT_PLAN_DOCS,
    SPECKIT_REQUIREMENT_DOCS, SPECKIT_SNIPPET_MAX_CHARS, SPECKIT_TASK_PLAN_REL_PATH,
    SPECKIT_TASK_PREFIX_TABLE, SPECKIT_TODO_REL_PATH, SPECKIT_TRACE_EDGE_THRESHOLD,
};
use crate::context::working_memory::WorkingMemory;
use crate::vault::models::VaultSkill;
use petgraph::algo::has_path_connecting;
use petgraph::graph::{DiGraph, NodeIndex};
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// Layer of an extracted specification/plan/task/constitution artifact node in the multipartite graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactLayer {
    /// Governing invariant from `.minicode/minicode.md`, `AGENTS.md`, or `CLAUDE.md`.
    ConstitutionRule,
    /// Functional or non-functional requirement from `prd.md` or `spec.md`.
    Requirement,
    /// Architectural component or implementation section from `architecture.md` or `implementation.md`.
    PlanComponent,
    /// Actionable task item from `todo.md` or `.minicode/plan/task_plan.md`.
    Task,
}

/// Extracted structural node from a Markdown specification, architecture, constitution, or task document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactNode {
    pub id: String,
    pub layer: ArtifactLayer,
    pub source_file: String,
    pub line: usize,
    pub section: String,
    pub text: String,
    pub stems: BTreeSet<String>,
    pub candidate_files: BTreeSet<String>,
    pub completed: Option<bool>,
}

/// Explicit ambiguity or unverified assumption marker (`[NEEDS CLARIFICATION]`, `[TBD]`, `[ASSUMPTION]`, etc.).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AmbiguityMarkerHit {
    pub marker: String,
    pub source_file: String,
    pub line: usize,
    pub snippet: String,
}

/// Directed traceability edge connecting `Requirement -> PlanComponent`, `PlanComponent -> Task`, or `Requirement -> Task`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TraceabilityEdge {
    pub from_id: String,
    pub to_id: String,
    pub similarity: f64,
    pub shared_stems: Vec<String>,
    pub shared_files: Vec<String>,
}

/// Structured GitHub Issue payload synthesized from an unchecked `Task` node (`speckit.taskstoissues`)
/// enriched with incoming `REQ-*` and `PLAN-*` traceability backlinks from the `petgraph::DiGraph`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceableIssuePayload {
    pub task_id: String,
    pub title: String,
    pub source_location: String,
    pub linked_requirements: Vec<String>,
    pub linked_plan_components: Vec<String>,
    pub target_files: Vec<String>,
    pub body_markdown: String,
}

/// Cross-artifact consistency and coverage report produced by `speckit.analyze`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CrossArtifactAnalysisReport {
    pub constitution_rules: Vec<ArtifactNode>,
    pub requirements: Vec<ArtifactNode>,
    pub plan_components: Vec<ArtifactNode>,
    pub tasks: Vec<ArtifactNode>,
    pub edges: Vec<TraceabilityEdge>,
    pub unmapped_requirements: Vec<ArtifactNode>,
    pub orphan_tasks: Vec<ArtifactNode>,
    pub ambiguity_hits: Vec<AmbiguityMarkerHit>,
    pub constitution_present: bool,
    pub coverage_pct: u8,
    pub passed: bool,
}

impl CrossArtifactAnalysisReport {
    /// Renders a concise Markdown report with exact `file:line` evidence.
    pub fn format_markdown(&self) -> String {
        let mut out = String::new();
        let badge = match self.passed {
            true => "✅ PASS",
            false => "⚠️ GAPS DETECTED",
        };
        out.push_str(&format!(
            "### 🔬 SpecKit Cross-Artifact Traceability Analysis (`{}` — Coverage: `{}%`)\n\n",
            badge, self.coverage_pct
        ));
        out.push_str(&format!(
            "- **Extracted Nodes:** `{}` constitution invariant(s) | `{}` requirement(s) | `{}` plan component(s) | `{}` task(s) | `{}` traceability edge(s)\n",
            self.constitution_rules.len(),
            self.requirements.len(),
            self.plan_components.len(),
            self.tasks.len(),
            self.edges.len()
        ));
        let constitution_summary = match self.constitution_present {
            true => format!(
                "Present (`{}` rule(s) indexed)",
                self.constitution_rules.len()
            ),
            false => String::from("Not found"),
        };
        out.push_str(&format!(
            "- **Project Constitution (`.minicode/minicode.md` / `AGENTS.md`):** {}\n",
            constitution_summary
        ));

        if !self.ambiguity_hits.is_empty() {
            out.push_str(&format!(
                "\n#### ❗ Unresolved Ambiguity / Assumption Markers (`{}`)\n",
                self.ambiguity_hits.len()
            ));
            for hit in &self.ambiguity_hits {
                out.push_str(&format!(
                    "- `{}:{}` (`{}`): {}\n",
                    hit.source_file, hit.line, hit.marker, hit.snippet
                ));
            }
        }

        if !self.unmapped_requirements.is_empty() {
            out.push_str(&format!(
                "\n#### 🔴 Unmapped Requirements — Zero Task Coverage (`{}`)\n",
                self.unmapped_requirements.len()
            ));
            for req in &self.unmapped_requirements {
                out.push_str(&format!(
                    "- `{}` (`{}:{}` — *{}*): {}\n",
                    req.id, req.source_file, req.line, req.section, req.text
                ));
            }
        }

        if !self.orphan_tasks.is_empty() {
            out.push_str(&format!(
                "\n#### 🟡 Orphan Tasks — Unlinked to Any Spec Requirement (`{}`)\n",
                self.orphan_tasks.len()
            ));
            for task in &self.orphan_tasks {
                out.push_str(&format!(
                    "- `{}` (`{}:{}`): {}\n",
                    task.id, task.source_file, task.line, task.text
                ));
            }
        }

        if self.unmapped_requirements.is_empty()
            && self.orphan_tasks.is_empty()
            && self.ambiguity_hits.is_empty()
        {
            out.push_str(
                "\n✨ All specification requirements have verified bipartite paths to implementation tasks with zero unresolved ambiguity markers.\n",
            );
        }

        out
    }

    /// Synthesizes traceable GitHub Issue payloads (`speckit.taskstoissues`) for all incomplete tasks
    /// using the `petgraph` traceability edges (`REQ-*` and `PLAN-*` backlinks).
    pub fn synthesize_traceable_issues(&self) -> Vec<TraceableIssuePayload> {
        self.tasks
            .iter()
            .filter(|t| t.completed != Some(true))
            .map(|task| {
                let mut linked_requirements = Vec::new();
                let mut linked_plan_components = Vec::new();
                for edge in self.edges.iter().filter(|e| e.to_id == task.id) {
                    if edge.from_id.starts_with("REQ-") {
                        linked_requirements.push(edge.from_id.clone());
                    } else if edge.from_id.starts_with("PLAN-") {
                        linked_plan_components.push(edge.from_id.clone());
                    }
                }
                let target_files: Vec<String> = task.candidate_files.iter().cloned().collect();
                let source_location = format!("{}:{}", task.source_file, task.line);
                let req_str = match linked_requirements.is_empty() {
                    true => String::from("None (Foundational / Standalone)"),
                    false => linked_requirements.join(", "),
                };
                let plan_str = match linked_plan_components.is_empty() {
                    true => String::from("None"),
                    false => linked_plan_components.join(", "),
                };
                let files_str = match target_files.is_empty() {
                    true => String::from("(inferred during implementation)"),
                    false => target_files.join("`, `"),
                };
                let body_markdown = format!(
                    "## Task Specification (`{}`)\n{}\n\n## Traceability Backlinks\n- **Source:** `{}` (*{}*)\n- **Linked Requirements:** `{}`\n- **Linked Architecture Components:** `{}`\n- **Target Deliverable Files:** `{}`\n",
                    task.id, task.text, source_location, task.section, req_str, plan_str, files_str
                );
                TraceableIssuePayload {
                    task_id: task.id.clone(),
                    title: format!("[{}] {}", task.id, task.text),
                    source_location,
                    linked_requirements,
                    linked_plan_components,
                    target_files,
                    body_markdown,
                }
            })
            .collect()
    }
}

/// Evaluation result for a single dimension of the Requirements-Quality Checklist ("Unit Tests for Requirements").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChecklistDimensionResult {
    pub id: String,
    pub title: String,
    pub passed: bool,
    pub score: u8,
    pub max_score: u8,
    pub evidence: String,
}

/// Complete Requirements-Quality Checklist report (`speckit.checklist`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequirementsChecklistReport {
    pub dimensions: Vec<ChecklistDimensionResult>,
    pub total_score: u8,
    pub passed: bool,
    pub checklist_path: Option<PathBuf>,
}

impl RequirementsChecklistReport {
    /// Renders the checklist as a Markdown document suitable for `minikit_docs/core/checklists/requirements.md`.
    pub fn format_markdown(&self) -> String {
        let mut out = String::new();
        let verdict = match self.passed {
            true => "PASS (Ready for Planning / Execution)",
            false => "NEEDS REFINEMENT (Resolve Failing Requirement Dimensions)",
        };
        out.push_str("# Requirements Quality Checklist (\"Unit Tests for Requirements\")\n\n");
        out.push_str(&format!(
            "- **Overall Score:** `{}/100` (Pass Threshold: `{}`)\n- **Verdict:** **{}**\n\n",
            self.total_score, SPECKIT_CHECKLIST_PASS_SCORE, verdict
        ));
        out.push_str("| ID | Dimension | Status | Score | Evidence |\n");
        out.push_str("| :--- | :--- | :---: | :---: | :--- |\n");
        for dim in &self.dimensions {
            let mark = match dim.passed {
                true => "✅ PASS",
                false => "❌ FAIL",
            };
            out.push_str(&format!(
                "| `{}` | **{}** | {} | `{}/{}` | {} |\n",
                dim.id, dim.title, mark, dim.score, dim.max_score, dim.evidence
            ));
        }
        out.push_str("\n## Checklist Items\n\n");
        for dim in &self.dimensions {
            let box_mark = match dim.passed {
                true => "x",
                false => " ",
            };
            out.push_str(&format!(
                "- [{}] **{} ({})**: {}\n",
                box_mark, dim.id, dim.title, dim.evidence
            ));
        }
        if let Some(p) = &self.checklist_path {
            out.push_str(&format!("\n*Persisted to `{}`.*\n", p.display()));
        }
        out
    }
}

/// Declarative dimension definition for the Requirements-Quality Checklist table.
struct ChecklistDimensionSpec {
    id: &'static str,
    title: &'static str,
    evaluate: fn(&CrossArtifactAnalysisReport, &str) -> (bool, u8, String),
}

const CHECKLIST_DIMENSION_TABLE: &[ChecklistDimensionSpec] = &[
    ChecklistDimensionSpec {
        id: "RQ-1",
        title: "Zero Unresolved Ambiguity Markers",
        evaluate: eval_zero_ambiguity,
    },
    ChecklistDimensionSpec {
        id: "RQ-2",
        title: "Testable Acceptance Criteria Presence",
        evaluate: eval_testable_acceptance,
    },
    ChecklistDimensionSpec {
        id: "RQ-3",
        title: "Bounded Scope & Non-Goals Defined",
        evaluate: eval_bounded_scope,
    },
    ChecklistDimensionSpec {
        id: "RQ-4",
        title: "Architecture & Interface Contract Alignment",
        evaluate: eval_architecture_alignment,
    },
    ChecklistDimensionSpec {
        id: "RQ-5",
        title: "Bipartite Task Decomposition Coverage",
        evaluate: eval_task_coverage,
    },
];

#[inline]
const fn checklist_dimension_pass_threshold() -> u8 {
    (SPECKIT_CHECKLIST_DIMENSION_WEIGHT * SPECKIT_CHECKLIST_PASS_RATIO_NUM)
        / SPECKIT_CHECKLIST_PASS_RATIO_DEN
}

fn eval_zero_ambiguity(
    analysis: &CrossArtifactAnalysisReport,
    _combined_spec_lower: &str,
) -> (bool, u8, String) {
    let count = analysis.ambiguity_hits.len();
    match (analysis.requirements.is_empty(), count) {
        (true, _) => (
            false,
            0,
            "No requirements found in `prd.md` or `spec.md`.".to_string(),
        ),
        (false, 0) => (
            true,
            SPECKIT_CHECKLIST_DIMENSION_WEIGHT,
            "0 unresolved `[NEEDS CLARIFICATION]`, `[TBD]`, or `[ASSUMPTION]` markers across spec artifacts."
                .to_string(),
        ),
        (false, n) => {
            let penalty = (n as u8).saturating_mul(SPECKIT_AMBIGUITY_PENALTY_PER_HIT);
            let score = SPECKIT_CHECKLIST_DIMENSION_WEIGHT.saturating_sub(penalty);
            (
                false,
                score,
                format!(
                    "{} unresolved marker(s) detected (first at `{}:{}`).",
                    n, analysis.ambiguity_hits[0].source_file, analysis.ambiguity_hits[0].line
                ),
            )
        }
    }
}

fn eval_testable_acceptance(
    analysis: &CrossArtifactAnalysisReport,
    _combined_spec_lower: &str,
) -> (bool, u8, String) {
    if analysis.requirements.is_empty() {
        return (
            false,
            0,
            "No requirement statements extracted to evaluate for testable criteria.".to_string(),
        );
    }
    let testable_count = analysis
        .requirements
        .iter()
        .filter(|req| {
            let lower = req.text.to_ascii_lowercase();
            !req.candidate_files.is_empty()
                || SPECKIT_ACCEPTANCE_CUES
                    .iter()
                    .any(|cue| lower.contains(cue))
        })
        .count();
    let ratio = testable_count as f64 / analysis.requirements.len() as f64;
    let score = ((ratio * f64::from(SPECKIT_CHECKLIST_DIMENSION_WEIGHT)).round() as u8)
        .min(SPECKIT_CHECKLIST_DIMENSION_WEIGHT);
    let passed = score >= checklist_dimension_pass_threshold();
    (
        passed,
        score,
        format!(
            "{}/{} requirement(s) include verifiable acceptance cues or concrete target deliverables.",
            testable_count,
            analysis.requirements.len()
        ),
    )
}

fn eval_bounded_scope(
    analysis: &CrossArtifactAnalysisReport,
    combined_spec_lower: &str,
) -> (bool, u8, String) {
    let matched_cues: Vec<&str> = SPECKIT_NON_GOAL_CUES
        .iter()
        .copied()
        .filter(|cue| combined_spec_lower.contains(cue))
        .collect();
    let passed = !analysis.requirements.is_empty() && !matched_cues.is_empty();
    let (score, evidence) = match passed {
        true => (
            SPECKIT_CHECKLIST_DIMENSION_WEIGHT,
            format!(
                "Explicit scope boundaries / invariants documented (`{}`).",
                matched_cues.join("`, `")
            ),
        ),
        false => (
            0,
            "Missing explicit Non-Goals, Out-of-Scope boundaries, or Technical Invariants section in `prd.md`/`spec.md`."
                .to_string(),
        ),
    };
    (passed, score, evidence)
}

fn eval_architecture_alignment(
    analysis: &CrossArtifactAnalysisReport,
    _combined_spec_lower: &str,
) -> (bool, u8, String) {
    let has_plan = !analysis.plan_components.is_empty();
    let has_req_files = analysis
        .requirements
        .iter()
        .any(|r| !r.candidate_files.is_empty());
    let passed = has_plan || has_req_files;
    let score = match (has_plan, has_req_files) {
        (true, true) => SPECKIT_CHECKLIST_DIMENSION_WEIGHT,
        (true, false) | (false, true) => checklist_dimension_pass_threshold(),
        (false, false) => 0,
    };
    let evidence = format!(
        "{} architecture/implementation component(s) indexed; concrete deliverable paths referenced={}.",
        analysis.plan_components.len(),
        has_req_files
    );
    (passed, score, evidence)
}

fn eval_task_coverage(
    analysis: &CrossArtifactAnalysisReport,
    _combined_spec_lower: &str,
) -> (bool, u8, String) {
    let score = ((f64::from(analysis.coverage_pct) / 100.0)
        * f64::from(SPECKIT_CHECKLIST_DIMENSION_WEIGHT))
    .round() as u8;
    let passed = analysis.coverage_pct >= SPECKIT_CHECKLIST_PASS_SCORE
        && analysis.unmapped_requirements.is_empty();
    let evidence = format!(
        "Traceability coverage `{}%` ({} unmapped requirement(s), {} orphan task(s)).",
        analysis.coverage_pct,
        analysis.unmapped_requirements.len(),
        analysis.orphan_tasks.len()
    );
    (passed, score, evidence)
}

/// Normalizes a text snippet into a sorted set of morphological stems (`len >= SPECKIT_MIN_STEM_LEN`).
pub fn extract_normalized_stems(text: &str) -> BTreeSet<String> {
    text.split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|tok| tok.len() >= SPECKIT_MIN_STEM_LEN)
        .map(VaultSkill::normalize_stem)
        .filter(|stem| stem.len() >= SPECKIT_MIN_STEM_LEN)
        .map(|stem| stem.to_ascii_lowercase())
        .collect()
}

/// Converts a byte offset in `content` to a 1-based line number using binary search over newline offsets.
fn byte_offset_to_line(newline_offsets: &[usize], byte_offset: usize) -> usize {
    match newline_offsets.binary_search(&byte_offset) {
        Ok(idx) | Err(idx) => idx + 1,
    }
}

/// State machine target for `pulldown-cmark` AST text/code accumulation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MarkdownCaptureTarget {
    None,
    Heading,
    ListItem,
}

/// Parses a Markdown document via `pulldown-cmark` AST events and extracts `ArtifactNode`s
/// for the given `ArtifactLayer` alongside any explicit `AmbiguityMarkerHit`s.
pub fn parse_markdown_artifact_nodes(
    content: &str,
    rel_path: &str,
    layer: ArtifactLayer,
    id_prefix: &str,
    start_index: usize,
) -> (Vec<ArtifactNode>, Vec<AmbiguityMarkerHit>) {
    let newline_offsets: Vec<usize> = content
        .bytes()
        .enumerate()
        .filter_map(|(idx, b)| (b == b'\n').then_some(idx))
        .collect();

    let mut ambiguity_hits = Vec::new();
    for (idx, raw_line) in content.lines().enumerate() {
        let upper = raw_line.to_ascii_uppercase();
        if let Some(&marker) = SPECKIT_AMBIGUITY_MARKERS
            .iter()
            .find(|&&m| upper.contains(m))
        {
            ambiguity_hits.push(AmbiguityMarkerHit {
                marker: marker.trim_start_matches('[').to_string(),
                source_file: rel_path.to_string(),
                line: idx + 1,
                snippet: raw_line
                    .trim()
                    .chars()
                    .take(SPECKIT_SNIPPET_MAX_CHARS)
                    .collect(),
            });
        }
    }

    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TASKLISTS);
    opts.insert(Options::ENABLE_TABLES);

    let parser = Parser::new_ext(content, opts).into_offset_iter();

    let mut current_section = String::from("Overview");
    let mut capture_target = MarkdownCaptureTarget::None;
    let mut heading_buf = String::new();

    let mut item_start_line = 1usize;
    let mut item_buf = String::new();
    let mut item_completed: Option<bool> = None;

    let mut nodes = Vec::new();
    let mut seq = start_index;

    for (event, range) in parser {
        match event {
            Event::Start(Tag::Heading { .. }) => {
                capture_target = MarkdownCaptureTarget::Heading;
                heading_buf.clear();
            }
            Event::End(TagEnd::Heading(_)) => {
                capture_target = MarkdownCaptureTarget::None;
                let trimmed = heading_buf.trim();
                if !trimmed.is_empty() {
                    current_section = trimmed.to_string();
                }
            }
            Event::Start(Tag::Item) => {
                capture_target = MarkdownCaptureTarget::ListItem;
                item_start_line = byte_offset_to_line(&newline_offsets, range.start);
                item_buf.clear();
                item_completed = None;
            }
            Event::End(TagEnd::Item) => {
                capture_target = MarkdownCaptureTarget::None;
                let cleaned = item_buf.split_whitespace().collect::<Vec<_>>().join(" ");
                let section_lower = current_section.to_ascii_lowercase();
                let is_non_goal_section = SPECKIT_NON_GOAL_CUES
                    .iter()
                    .any(|cue| section_lower.contains(cue));

                // Skip non-goal bullet items when extracting actionable requirements
                if !cleaned.is_empty()
                    && !(layer == ArtifactLayer::Requirement && is_non_goal_section)
                {
                    let (final_text, inferred_done) =
                        normalize_task_prefix(&cleaned, item_completed);
                    let combined_context = format!("{} {}", current_section, final_text);
                    let stems = extract_normalized_stems(&combined_context);
                    let candidate_files: BTreeSet<String> =
                        WorkingMemory::extract_candidate_files(&final_text)
                            .into_iter()
                            .collect();

                    if !stems.is_empty() || !candidate_files.is_empty() {
                        seq += 1;
                        nodes.push(ArtifactNode {
                            id: format!("{}-{}", id_prefix, seq),
                            layer,
                            source_file: rel_path.to_string(),
                            line: item_start_line,
                            section: current_section.clone(),
                            text: final_text,
                            stems,
                            candidate_files,
                            completed: inferred_done,
                        });
                    }
                }
            }
            Event::TaskListMarker(checked) => {
                if capture_target == MarkdownCaptureTarget::ListItem {
                    item_completed = Some(checked);
                }
            }
            Event::Text(t) => match capture_target {
                MarkdownCaptureTarget::Heading => heading_buf.push_str(&t),
                MarkdownCaptureTarget::ListItem => {
                    item_buf.push_str(&t);
                    item_buf.push(' ');
                }
                MarkdownCaptureTarget::None => {}
            },
            Event::Code(c) => match capture_target {
                MarkdownCaptureTarget::Heading => {
                    heading_buf.push('`');
                    heading_buf.push_str(&c);
                    heading_buf.push('`');
                }
                MarkdownCaptureTarget::ListItem => {
                    item_buf.push('`');
                    item_buf.push_str(&c);
                    item_buf.push('`');
                    item_buf.push(' ');
                }
                MarkdownCaptureTarget::None => {}
            },
            _ => {}
        }
    }

    (nodes, ambiguity_hits)
}

/// Normalizes custom MiniCode task markers (`[>]`, `[~]`, `[!]`, `[x]`, `[ ]`) using [`SPECKIT_TASK_PREFIX_TABLE`].
fn normalize_task_prefix(text: &str, pulldown_checked: Option<bool>) -> (String, Option<bool>) {
    SPECKIT_TASK_PREFIX_TABLE
        .iter()
        .find_map(|&(prefix, done)| {
            text.strip_prefix(prefix)
                .map(|rest| (rest.trim().to_string(), Some(done)))
        })
        .unwrap_or_else(|| (text.to_string(), pulldown_checked))
}

/// Computes IDF weights $\text{IDF}(t) = \ln(1 + \frac{N}{1 + \text{df}(t)})$ across all extracted artifact nodes.
fn compute_stem_idf(nodes: &[&ArtifactNode]) -> BTreeMap<String, f64> {
    let total_docs = nodes.len().max(1) as f64;
    let mut doc_freq: BTreeMap<String, usize> = BTreeMap::new();
    for node in nodes {
        for stem in &node.stems {
            *doc_freq.entry(stem.clone()).or_insert(0) += 1;
        }
    }
    doc_freq
        .into_iter()
        .map(|(stem, df)| {
            let idf = (1.0 + total_docs / (1.0 + df as f64)).ln();
            (stem, idf)
        })
        .collect()
}

/// Computes IDF-weighted Jaccard stem similarity plus deliverable file overlap bonus between two nodes.
fn compute_node_similarity(
    a: &ArtifactNode,
    b: &ArtifactNode,
    idf_map: &BTreeMap<String, f64>,
) -> (f64, Vec<String>, Vec<String>) {
    let shared_stems: Vec<String> = a.stems.intersection(&b.stems).cloned().collect();
    let union_weight: f64 = a
        .stems
        .union(&b.stems)
        .map(|s| idf_map.get(s).copied().unwrap_or(1.0))
        .sum();
    let inter_weight: f64 = shared_stems
        .iter()
        .map(|s| idf_map.get(s).copied().unwrap_or(1.0))
        .sum();

    let stem_sim = if union_weight > f64::EPSILON {
        inter_weight / union_weight
    } else {
        0.0
    };

    let shared_files: Vec<String> = a
        .candidate_files
        .iter()
        .filter(|fa| {
            b.candidate_files
                .iter()
                .any(|fb| fa == &fb || fa.ends_with(fb.as_str()) || fb.ends_with(fa.as_str()))
        })
        .cloned()
        .collect();

    let file_bonus = (shared_files.len() as f64) * SPECKIT_FILE_OVERLAP_BONUS;
    let total_sim = (stem_sim + file_bonus).min(1.0);
    (total_sim, shared_stems, shared_files)
}

/// Runs the Bipartite / Multipartite Cross-Artifact Traceability & Consistency Analyzer (`speckit.analyze`)
/// across constitution files, `<docs_dir>/core/{prd,spec,architecture,implementation,todo}.md`, and `.minicode/plan/task_plan.md`.
pub fn analyze_workspace_artifacts(workspace_root: &Path) -> CrossArtifactAnalysisReport {
    let docs_dir = crate::tools::minikit::resolve_docs_dir(workspace_root);
    let docs_rel_prefix = docs_dir
        .strip_prefix(workspace_root)
        .unwrap_or(&docs_dir)
        .display()
        .to_string();

    let mut constitution_rules = Vec::new();
    let mut requirements = Vec::new();
    let mut plan_components = Vec::new();
    let mut tasks = Vec::new();
    let mut ambiguity_hits = Vec::new();

    for &const_file in SPECKIT_CONSTITUTION_FILES {
        let p = workspace_root.join(const_file);
        if let Ok(content) = std::fs::read_to_string(&p) {
            let (mut nodes, _) = parse_markdown_artifact_nodes(
                &content,
                const_file,
                ArtifactLayer::ConstitutionRule,
                "CONST",
                constitution_rules.len(),
            );
            constitution_rules.append(&mut nodes);
        }
    }

    for &doc_name in SPECKIT_REQUIREMENT_DOCS {
        let p = docs_dir.join("core").join(doc_name);
        if let Ok(content) = std::fs::read_to_string(&p) {
            let rel = format!("{}/core/{}", docs_rel_prefix, doc_name);
            let (mut nodes, mut amb) = parse_markdown_artifact_nodes(
                &content,
                &rel,
                ArtifactLayer::Requirement,
                "REQ",
                requirements.len(),
            );
            requirements.append(&mut nodes);
            ambiguity_hits.append(&mut amb);
        }
    }

    for &doc_name in SPECKIT_PLAN_DOCS {
        let p = docs_dir.join("core").join(doc_name);
        if let Ok(content) = std::fs::read_to_string(&p) {
            let rel = format!("{}/core/{}", docs_rel_prefix, doc_name);
            let (mut nodes, mut amb) = parse_markdown_artifact_nodes(
                &content,
                &rel,
                ArtifactLayer::PlanComponent,
                "PLAN",
                plan_components.len(),
            );
            plan_components.append(&mut nodes);
            ambiguity_hits.append(&mut amb);
        }
    }

    let todo_path = docs_dir.join(SPECKIT_TODO_REL_PATH);
    if let Ok(content) = std::fs::read_to_string(&todo_path) {
        let rel = format!("{}/{}", docs_rel_prefix, SPECKIT_TODO_REL_PATH);
        let (mut nodes, mut amb) =
            parse_markdown_artifact_nodes(&content, &rel, ArtifactLayer::Task, "TASK", tasks.len());
        tasks.append(&mut nodes);
        ambiguity_hits.append(&mut amb);
    }

    let task_plan_path = workspace_root.join(SPECKIT_TASK_PLAN_REL_PATH);
    if let Ok(content) = std::fs::read_to_string(&task_plan_path) {
        let (mut nodes, mut amb) = parse_markdown_artifact_nodes(
            &content,
            SPECKIT_TASK_PLAN_REL_PATH,
            ArtifactLayer::Task,
            "TASK",
            tasks.len(),
        );
        tasks.append(&mut nodes);
        ambiguity_hits.append(&mut amb);
    }

    let all_nodes: Vec<&ArtifactNode> = requirements
        .iter()
        .chain(plan_components.iter())
        .chain(tasks.iter())
        .collect();
    let idf_map = compute_stem_idf(&all_nodes);

    // Build petgraph::DiGraph<String, f64>
    let mut graph: DiGraph<String, f64> = DiGraph::new();
    let req_indices: Vec<NodeIndex> = requirements
        .iter()
        .map(|r| graph.add_node(r.id.clone()))
        .collect();
    let plan_indices: Vec<NodeIndex> = plan_components
        .iter()
        .map(|p| graph.add_node(p.id.clone()))
        .collect();
    let task_indices: Vec<NodeIndex> = tasks.iter().map(|t| graph.add_node(t.id.clone())).collect();

    let mut edges = Vec::new();

    // Requirement -> PlanComponent edges
    for (r_idx, req) in requirements.iter().enumerate() {
        for (p_idx, plan) in plan_components.iter().enumerate() {
            let (sim, shared_stems, shared_files) = compute_node_similarity(req, plan, &idf_map);
            if sim >= SPECKIT_TRACE_EDGE_THRESHOLD {
                graph.add_edge(req_indices[r_idx], plan_indices[p_idx], sim);
                edges.push(TraceabilityEdge {
                    from_id: req.id.clone(),
                    to_id: plan.id.clone(),
                    similarity: sim,
                    shared_stems,
                    shared_files,
                });
            }
        }
    }

    // PlanComponent -> Task edges
    for (p_idx, plan) in plan_components.iter().enumerate() {
        for (t_idx, task) in tasks.iter().enumerate() {
            let (sim, shared_stems, shared_files) = compute_node_similarity(plan, task, &idf_map);
            if sim >= SPECKIT_TRACE_EDGE_THRESHOLD {
                graph.add_edge(plan_indices[p_idx], task_indices[t_idx], sim);
                edges.push(TraceabilityEdge {
                    from_id: plan.id.clone(),
                    to_id: task.id.clone(),
                    similarity: sim,
                    shared_stems,
                    shared_files,
                });
            }
        }
    }

    // Direct Requirement -> Task bipartite edges
    for (r_idx, req) in requirements.iter().enumerate() {
        for (t_idx, task) in tasks.iter().enumerate() {
            let (sim, shared_stems, shared_files) = compute_node_similarity(req, task, &idf_map);
            if sim >= SPECKIT_TRACE_EDGE_THRESHOLD {
                graph.add_edge(req_indices[r_idx], task_indices[t_idx], sim);
                edges.push(TraceabilityEdge {
                    from_id: req.id.clone(),
                    to_id: task.id.clone(),
                    similarity: sim,
                    shared_stems,
                    shared_files,
                });
            }
        }
    }

    // Identify unmapped requirements (no directed path in DiGraph to any Task node)
    let mut unmapped_requirements = Vec::new();
    for (r_idx, req) in requirements.iter().enumerate() {
        let mapped = task_indices
            .iter()
            .any(|&t_node| has_path_connecting(&graph, req_indices[r_idx], t_node, None));
        if !mapped {
            unmapped_requirements.push(req.clone());
        }
    }

    // Identify orphan tasks (not reachable from any Requirement and not a foundational exempt task)
    let mut orphan_tasks = Vec::new();
    if !requirements.is_empty() {
        for (t_idx, task) in tasks.iter().enumerate() {
            let reachable = req_indices
                .iter()
                .any(|&r_node| has_path_connecting(&graph, r_node, task_indices[t_idx], None));
            let is_exempt = task
                .stems
                .iter()
                .any(|s| SPECKIT_ORPHAN_EXEMPT_STEMS.contains(&s.as_str()));
            if !reachable && !is_exempt {
                orphan_tasks.push(task.clone());
            }
        }
    }

    let coverage_pct = if requirements.is_empty() {
        0
    } else {
        let covered = requirements
            .len()
            .saturating_sub(unmapped_requirements.len());
        ((covered as f64 / requirements.len() as f64) * 100.0).round() as u8
    };

    let constitution_present = SPECKIT_CONSTITUTION_FILES
        .iter()
        .any(|f| workspace_root.join(f).is_file());

    let passed = !requirements.is_empty()
        && unmapped_requirements.is_empty()
        && ambiguity_hits.is_empty()
        && coverage_pct >= SPECKIT_CHECKLIST_PASS_SCORE;

    CrossArtifactAnalysisReport {
        constitution_rules,
        requirements,
        plan_components,
        tasks,
        edges,
        unmapped_requirements,
        orphan_tasks,
        ambiguity_hits,
        constitution_present,
        coverage_pct,
        passed,
    }
}

/// Evaluates the 5-dimension Requirements-Quality Checklist (`speckit.checklist` — "Unit Tests for Requirements")
/// and optionally persists `<docs_dir>/core/checklists/requirements.md`.
pub fn evaluate_requirements_checklist(
    workspace_root: &Path,
    persist: bool,
) -> RequirementsChecklistReport {
    let analysis = analyze_workspace_artifacts(workspace_root);
    let docs_dir = crate::tools::minikit::resolve_docs_dir(workspace_root);

    let mut combined_spec_lower = String::new();
    for &doc_name in SPECKIT_REQUIREMENT_DOCS
        .iter()
        .chain(SPECKIT_PLAN_DOCS.iter())
    {
        if let Ok(content) = std::fs::read_to_string(docs_dir.join("core").join(doc_name)) {
            combined_spec_lower.push_str(&content.to_ascii_lowercase());
            combined_spec_lower.push('\n');
        }
    }

    let dimensions: Vec<ChecklistDimensionResult> = CHECKLIST_DIMENSION_TABLE
        .iter()
        .map(|spec| {
            let (passed, score, evidence) = (spec.evaluate)(&analysis, &combined_spec_lower);
            ChecklistDimensionResult {
                id: spec.id.to_string(),
                title: spec.title.to_string(),
                passed,
                score,
                max_score: SPECKIT_CHECKLIST_DIMENSION_WEIGHT,
                evidence,
            }
        })
        .collect();

    let total_score: u8 = dimensions
        .iter()
        .map(|d| u16::from(d.score))
        .sum::<u16>()
        .min(100) as u8;
    let passed = total_score >= SPECKIT_CHECKLIST_PASS_SCORE
        && dimensions.iter().all(|d| d.id != "RQ-1" || d.passed);

    let mut checklist_path = None;
    let mut report = RequirementsChecklistReport {
        dimensions,
        total_score,
        passed,
        checklist_path: None,
    };

    if persist {
        let out_path = docs_dir.join(SPECKIT_CHECKLIST_REL_PATH);
        if let Some(parent) = out_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        report.checklist_path = Some(out_path.clone());
        let markdown = report.format_markdown();
        if std::fs::write(&out_path, markdown).is_ok() {
            checklist_path = Some(out_path);
        }
        report.checklist_path = checklist_path;
    }

    report
}

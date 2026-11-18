//! Set-Algebraic & AST-Grounded Brainstorming Engine (`brainstorm_algebra`).
//!
//! Replaces static prompt echoing with deterministic, zero-regex mathematical primitives:
//! 1. `pulldown-cmark` AST extraction of settled architectural decisions (`[D1..DN]` & `spec.md`).
//! 2. Morphological stem Jaccard & containment filtering to prune already-settled `open_questions`.
//! 3. Multi-criteria utility & 4-axis Pareto frontier ranking of candidate `approaches` grounded
//!    against cached `CodeGraph` AST symbol shards (`.minicode/graph_shards.bin`).
//! 4. Self-contained responsive dark-mode HTML comparison board generation for live `MiniBrowse` inspection.

use crate::context::graph::graph_store::GraphStore;
use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeSet, HashSet};
use std::path::Path;

/// A settled architectural or product decision parsed structurally from Markdown AST.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettledDecisionNode {
    pub id: String,
    pub heading: String,
    pub body: String,
    pub stems: BTreeSet<String>,
}

/// An open question pruned because it is already answered by a settled decision node.
#[derive(Debug, Clone, PartialEq)]
pub struct PrunedQuestion {
    pub question: String,
    pub matched_id: String,
    pub matched_heading: String,
    pub jaccard: f64,
    pub containment: f64,
}

/// Result of filtering candidate `open_questions` against settled workspace decisions.
#[derive(Debug, Clone, Default)]
pub struct QuestionFilterReport {
    pub active_questions: Vec<String>,
    pub pruned_questions: Vec<PrunedQuestion>,
}

/// Candidate architectural or product approach supplied to `power_brainstorm`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApproachCandidate {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub pros: Vec<String>,
    #[serde(default)]
    pub cons: Vec<String>,
    #[serde(default)]
    pub files_touched: Vec<String>,
    /// Subjective/estimated implementation complexity on `[1.0, 5.0]` (defaults to `2.5`).
    #[serde(default = "default_complexity")]
    pub complexity: f64,
}

fn default_complexity() -> f64 {
    2.5
}

/// Evaluated and Pareto-ranked candidate approach grounded against workspace AST symbols.
#[derive(Debug, Clone, Serialize)]
pub struct ScoredApproach {
    pub candidate: ApproachCandidate,
    pub utility_score: f64,
    pub pareto_optimal: bool,
    pub ast_reuse_overlap: f64,
    pub existing_file_ratio: f64,
    pub matched_ast_symbols: Vec<String>,
}

/// Complete multi-criteria & Pareto evaluation across candidate approaches.
#[derive(Debug, Clone, Default)]
pub struct ApproachEvaluationReport {
    pub ranked: Vec<ScoredApproach>,
    pub total_workspace_ast_symbols: usize,
}

/// Splits camelCase / PascalCase / snake_case / punctuation boundaries into normalized
/// morphological stems without any domain keyword lists or regular expressions.
#[must_use]
pub fn extract_morphological_stems(text: &str) -> BTreeSet<String> {
    let mut raw_words: Vec<String> = Vec::new();
    let mut current = String::new();
    let chars: Vec<char> = text.chars().collect();

    for (idx, &ch) in chars.iter().enumerate() {
        if ch.is_ascii_alphanumeric() {
            let prev_is_lower = idx > 0 && chars[idx - 1].is_ascii_lowercase();
            let next_is_lower = idx + 1 < chars.len() && chars[idx + 1].is_ascii_lowercase();
            let prev_is_upper = idx > 0 && chars[idx - 1].is_ascii_uppercase();
            if ch.is_ascii_uppercase()
                && !current.is_empty()
                && (prev_is_lower || (prev_is_upper && next_is_lower))
            {
                raw_words.push(std::mem::take(&mut current));
            }
            current.push(ch.to_ascii_lowercase());
        } else if !current.is_empty() {
            raw_words.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        raw_words.push(current);
    }

    let mut stems = BTreeSet::new();
    for word in raw_words {
        if word.len() < 3 || word.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        if is_structural_function_word(&word) {
            continue;
        }
        let stem = stem_english_inflection(&word);
        if stem.len() >= 3 && !is_structural_function_word(&stem) {
            stems.insert(stem);
        }
    }
    stems
}

/// Closed-class English grammatical function words (pronouns, prepositions, auxiliaries,
/// interrogatives) carrying zero domain entropy.
fn is_structural_function_word(word: &str) -> bool {
    matches!(
        word,
        "the"
            | "and"
            | "for"
            | "with"
            | "from"
            | "that"
            | "this"
            | "these"
            | "those"
            | "what"
            | "which"
            | "where"
            | "when"
            | "who"
            | "whom"
            | "whose"
            | "why"
            | "how"
            | "should"
            | "would"
            | "could"
            | "shall"
            | "will"
            | "can"
            | "may"
            | "might"
            | "must"
            | "have"
            | "has"
            | "had"
            | "been"
            | "being"
            | "are"
            | "was"
            | "were"
            | "does"
            | "did"
            | "use"
            | "used"
            | "using"
            | "into"
            | "onto"
            | "over"
            | "under"
            | "about"
            | "between"
            | "through"
            | "during"
            | "before"
            | "after"
            | "above"
            | "below"
            | "their"
            | "there"
            | "your"
            | "our"
            | "need"
            | "want"
            | "prefer"
            | "choose"
            | "select"
    )
}

fn stem_english_inflection(word: &str) -> String {
    for suffix in [
        "ization", "isation", "ational", "fulness", "ousness", "iveness",
    ] {
        if word.len() > suffix.len() + 2 {
            if let Some(base) = word.strip_suffix(suffix) {
                return base.to_string();
            }
        }
    }
    if word.len() > 5 {
        if let Some(base) = word.strip_suffix("ies") {
            return format!("{base}y");
        }
    }
    for suffix in [
        "ing", "tion", "sion", "ment", "ness", "able", "ible", "ally",
    ] {
        if word.len() > suffix.len() + 2 {
            if let Some(base) = word.strip_suffix(suffix) {
                return base.to_string();
            }
        }
    }
    if word.len() > 4 {
        if let Some(base) = word.strip_suffix("ed") {
            return base.to_string();
        }
        if let Some(base) = word.strip_suffix("es") {
            return base.to_string();
        }
    }
    if word.len() > 3 && !word.ends_with("ss") && !word.ends_with("us") && !word.ends_with("is") {
        if let Some(base) = word.strip_suffix('s') {
            return base.to_string();
        }
    }
    word.to_string()
}

/// Parses settled decision sections from Markdown content via `pulldown-cmark` AST traversal.
#[must_use]
pub fn parse_markdown_decision_sections(
    markdown: &str,
    default_prefix: &str,
) -> Vec<SettledDecisionNode> {
    let parser = Parser::new_ext(markdown, Options::all());
    let mut sections: Vec<SettledDecisionNode> = Vec::new();

    let mut in_target_heading = false;
    let mut current_heading = String::new();
    let mut current_body = String::new();
    let mut auto_idx = 0usize;

    let flush_section = |heading: &mut String,
                         body: &mut String,
                         idx: &mut usize,
                         out: &mut Vec<SettledDecisionNode>| {
        let h = heading.trim().to_string();
        let b = body.trim().to_string();
        heading.clear();
        body.clear();
        if h.is_empty() && b.is_empty() {
            return;
        }
        let h_lower = h.to_ascii_lowercase();
        if h_lower.contains("not user-confirmed")
            || h_lower.starts_with("assumed for ")
            || b.contains("_Placeholder")
            || b.contains("Core invariants, interfaces, and constraints")
        {
            return;
        }
        *idx += 1;
        let id = extract_bracketed_decision_id(&h)
            .unwrap_or_else(|| format!("{}{}", default_prefix, *idx));
        let clean_heading = strip_provenance_suffix(&h);
        let combined = format!("{clean_heading} {b}");
        let stems = extract_morphological_stems(&combined);
        if stems.len() >= 2 {
            out.push(SettledDecisionNode {
                id,
                heading: clean_heading,
                body: b,
                stems,
            });
        }
    };

    for event in parser {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                if matches!(level, HeadingLevel::H2 | HeadingLevel::H3) {
                    flush_section(
                        &mut current_heading,
                        &mut current_body,
                        &mut auto_idx,
                        &mut sections,
                    );
                    in_target_heading = true;
                }
            }
            Event::End(TagEnd::Heading(level)) => {
                if matches!(level, HeadingLevel::H2 | HeadingLevel::H3) {
                    in_target_heading = false;
                }
            }
            Event::Text(t) | Event::Code(t) => {
                if in_target_heading {
                    if !current_heading.is_empty() && !current_heading.ends_with(' ') {
                        current_heading.push(' ');
                    }
                    current_heading.push_str(&t);
                } else if !current_heading.is_empty() {
                    if !current_body.is_empty()
                        && !current_body.ends_with(' ')
                        && !current_body.ends_with('\n')
                    {
                        current_body.push(' ');
                    }
                    current_body.push_str(&t);
                }
            }
            Event::SoftBreak
            | Event::HardBreak
            | Event::End(TagEnd::Item)
            | Event::End(TagEnd::Paragraph) => {
                if !in_target_heading
                    && !current_heading.is_empty()
                    && !current_body.ends_with('\n')
                {
                    current_body.push('\n');
                }
            }
            _ => {}
        }
    }
    flush_section(
        &mut current_heading,
        &mut current_body,
        &mut auto_idx,
        &mut sections,
    );
    sections
}

fn extract_bracketed_decision_id(heading: &str) -> Option<String> {
    let trimmed = heading.trim();
    let rest = trimmed.strip_prefix('[')?;
    let (inside, _) = rest.split_once(']')?;
    let clean = inside.trim();
    if !clean.is_empty()
        && clean.len() <= 12
        && clean
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        Some(clean.to_string())
    } else {
        None
    }
}

fn strip_provenance_suffix(heading: &str) -> String {
    let mut s = heading.trim();
    if let Some(rest) = s.strip_prefix('[') {
        if let Some((_, after)) = rest.split_once(']') {
            s = after.trim();
        }
    }
    if let Some(paren_idx) = s.rfind(" (") {
        let tail = &s[paren_idx + 2..];
        if tail.starts_with("answered by ")
            || tail.starts_with("settled by ")
            || tail.contains("UTC)")
        {
            s = s[..paren_idx].trim();
        }
    }
    s.to_string()
}

/// Extracts all settled user/architectural decisions from `.minicode/decisions.md` and `spec.md`.
#[must_use]
pub fn extract_settled_decisions(workspace_root: &Path) -> Vec<SettledDecisionNode> {
    let mut nodes = Vec::new();
    let decisions_path = workspace_root.join(".minicode/decisions.md");
    if let Ok(md) = std::fs::read_to_string(&decisions_path) {
        nodes.extend(parse_markdown_decision_sections(&md, "D"));
    }

    let spec_path = crate::tools::minikit::resolve_doc_path(workspace_root, "spec.md");
    if let Ok(spec_md) = std::fs::read_to_string(&spec_path) {
        if spec_md.lines().filter(|l| !l.trim().is_empty()).count() > 16 {
            nodes.extend(parse_markdown_decision_sections(&spec_md, "SPEC-"));
        }
    }
    nodes
}

/// Partitions `open_questions` into genuinely unresolved questions vs. questions already
/// entailed by settled decisions (`J(q, d) >= 0.35` or directional containment `C(q, d) >= 0.65`
/// with at least 2 shared domain stems).
#[must_use]
pub fn filter_redundant_questions(
    open_questions: &[String],
    settled: &[SettledDecisionNode],
) -> QuestionFilterReport {
    let mut report = QuestionFilterReport::default();
    if settled.is_empty() {
        report.active_questions = open_questions.to_vec();
        return report;
    }

    for q in open_questions {
        let q_stems = extract_morphological_stems(q);
        if q_stems.len() < 2 {
            report.active_questions.push(q.clone());
            continue;
        }

        let mut best_match: Option<(&SettledDecisionNode, f64, f64, usize)> = None;
        for d in settled {
            let inter = q_stems.intersection(&d.stems).count();
            if inter < 2 {
                continue;
            }
            let union = q_stems.union(&d.stems).count().max(1);
            let jaccard = inter as f64 / union as f64;
            let containment = inter as f64 / q_stems.len() as f64;
            if jaccard >= 0.35 || containment >= 0.65 {
                let score = jaccard + containment;
                if best_match.is_none_or(|(_, bj, bc, _)| score > bj + bc) {
                    best_match = Some((d, jaccard, containment, inter));
                }
            }
        }

        if let Some((d, jaccard, containment, _)) = best_match {
            report.pruned_questions.push(PrunedQuestion {
                question: q.clone(),
                matched_id: d.id.clone(),
                matched_heading: d.heading.clone(),
                jaccard,
                containment,
            });
        } else {
            report.active_questions.push(q.clone());
        }
    }

    report
}

/// Parses candidate `approaches` from JSON tool arguments.
#[must_use]
pub fn parse_approach_candidates(args: &Value) -> Vec<ApproachCandidate> {
    let Some(arr) = args.get("approaches").and_then(|v| v.as_array()) else {
        return Vec::new();
    };
    let mut out = Vec::with_capacity(arr.len());
    for (idx, item) in arr.iter().enumerate() {
        let Some(obj) = item.as_object() else {
            continue;
        };
        let title = obj
            .get("title")
            .or_else(|| obj.get("name"))
            .or_else(|| obj.get("label"))
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or_default();
        if title.is_empty() {
            continue;
        }
        let id = obj
            .get("id")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from)
            .unwrap_or_else(|| format!("opt_{}", idx + 1));
        let summary = obj
            .get("summary")
            .or_else(|| obj.get("description"))
            .and_then(|v| v.as_str())
            .map(str::trim)
            .unwrap_or_default()
            .to_string();
        let str_vec = |k1: &str, k2: &str| -> Vec<String> {
            obj.get(k1)
                .or_else(|| obj.get(k2))
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str())
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect()
                })
                .unwrap_or_default()
        };
        let complexity = obj
            .get("complexity")
            .and_then(|v| v.as_f64())
            .unwrap_or(2.5)
            .clamp(1.0, 5.0);

        out.push(ApproachCandidate {
            id,
            title: title.to_string(),
            summary,
            pros: str_vec("pros", "strengths"),
            cons: str_vec("cons", "risks"),
            files_touched: str_vec("files_touched", "target_files"),
            complexity,
        });
    }
    out
}

/// Loads cached AST symbol identifiers from `.minicode/graph_shards.bin` (or `graph.json` fallback)
/// in O(1) cached time without triggering a blocking full-workspace parse.
#[must_use]
pub fn load_cached_ast_symbol_set(workspace_root: &Path) -> HashSet<String> {
    let mut symbols = HashSet::new();
    if let Some(shards) = GraphStore::load_shards(workspace_root) {
        for shard in shards.values() {
            for sym in &shard.all_symbols {
                let clean = sym.name.trim();
                if clean.len() >= 3 {
                    symbols.insert(clean.to_ascii_lowercase());
                }
            }
        }
        if !symbols.is_empty() {
            return symbols;
        }
    }
    if let Ok(Some(snap)) = GraphStore::load_snapshot(workspace_root) {
        for node in snap.nodes {
            let clean = node.name.trim();
            if clean.len() >= 3 {
                symbols.insert(clean.to_ascii_lowercase());
            }
        }
    }
    symbols
}

fn extract_code_identifiers(text: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for raw in text.split(|c: char| {
        c.is_whitespace()
            || matches!(
                c,
                '(' | ')' | '[' | ']' | '{' | '}' | ',' | ';' | ':' | '"' | '\'' | '<' | '>'
            )
    }) {
        let was_backticked = raw.starts_with('`') && raw.ends_with('`') && raw.len() >= 5;
        let token = raw
            .trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '/' && c != '.');
        if token.len() < 3 {
            continue;
        }
        let stem_without_ext = match token.rsplit_once('.') {
            Some((prefix, ext)) if !prefix.is_empty() && (1..=5).contains(&ext.len()) => prefix,
            _ => token,
        };
        let leaf = stem_without_ext
            .rsplit(['/', '.'])
            .next()
            .unwrap_or(stem_without_ext);
        let has_internal_camel = leaf.chars().any(|c| c.is_ascii_lowercase())
            && leaf.chars().skip(1).any(|c| c.is_ascii_uppercase());
        let has_code_shape = leaf.contains('_') || has_internal_camel || was_backticked;
        if has_code_shape && leaf.len() >= 3 {
            out.insert(leaf.to_string());
        }
    }
    out
}

/// Evaluates candidate approaches using AST symbol reuse grounding, multi-criteria utility,
/// and 4-axis Pareto dominance sorting.
#[must_use]
pub fn evaluate_approaches_against_ast(
    workspace_root: &Path,
    approaches: &[ApproachCandidate],
) -> ApproachEvaluationReport {
    if approaches.is_empty() {
        return ApproachEvaluationReport::default();
    }
    let ast_symbols = load_cached_ast_symbol_set(workspace_root);
    let total_ast = ast_symbols.len();

    #[derive(Clone, Copy)]
    struct Axes {
        pros: f64,
        neg_cons: f64,
        neg_footprint: f64,
        grounding: f64,
    }

    let mut prelim: Vec<(ScoredApproach, Axes)> = Vec::with_capacity(approaches.len());

    for cand in approaches {
        let combined_text = format!(
            "{} {} {} {}",
            cand.title,
            cand.summary,
            cand.pros.join(" "),
            cand.files_touched.join(" ")
        );
        let idents = extract_code_identifiers(&combined_text);
        let mut matched_ast_symbols = Vec::new();
        for ident in &idents {
            if ast_symbols.contains(&ident.to_ascii_lowercase()) {
                matched_ast_symbols.push(ident.clone());
            }
        }
        let ast_reuse_overlap = if idents.is_empty() || ast_symbols.is_empty() {
            0.0
        } else {
            matched_ast_symbols.len() as f64 / idents.len() as f64
        };

        let existing_file_ratio = if cand.files_touched.is_empty() {
            0.0
        } else {
            let existing = cand
                .files_touched
                .iter()
                .filter(|rel| {
                    let clean = rel.trim().trim_start_matches("./");
                    !clean.is_empty() && workspace_root.join(clean).exists()
                })
                .count();
            existing as f64 / cand.files_touched.len() as f64
        };

        let pros_f = cand.pros.len() as f64;
        let cons_f = cand.cons.len() as f64;
        let files_f = cand.files_touched.len() as f64;
        let utility_score =
            2.0 * pros_f - 1.5 * cons_f - 0.6 * (1.0 + files_f).ln() - 0.5 * cand.complexity
                + 2.2 * ast_reuse_overlap
                + 0.8 * existing_file_ratio;

        let axes = Axes {
            pros: pros_f,
            neg_cons: -cons_f,
            neg_footprint: -(files_f + cand.complexity),
            grounding: ast_reuse_overlap + existing_file_ratio,
        };

        prelim.push((
            ScoredApproach {
                candidate: cand.clone(),
                utility_score,
                pareto_optimal: true,
                ast_reuse_overlap,
                existing_file_ratio,
                matched_ast_symbols,
            },
            axes,
        ));
    }

    const EPS: f64 = 1e-9;
    for i in 0..prelim.len() {
        let a = prelim[i].1;
        let dominated = prelim.iter().enumerate().any(|(j, (_, b))| {
            if i == j {
                return false;
            }
            let ge_all = b.pros + EPS >= a.pros
                && b.neg_cons + EPS >= a.neg_cons
                && b.neg_footprint + EPS >= a.neg_footprint
                && b.grounding + EPS >= a.grounding;
            let gt_any = b.pros > a.pros + EPS
                || b.neg_cons > a.neg_cons + EPS
                || b.neg_footprint > a.neg_footprint + EPS
                || b.grounding > a.grounding + EPS;
            ge_all && gt_any
        });
        prelim[i].0.pareto_optimal = !dominated;
    }

    let mut ranked: Vec<ScoredApproach> = prelim.into_iter().map(|(s, _)| s).collect();
    ranked.sort_by(|a, b| {
        b.pareto_optimal
            .cmp(&a.pareto_optimal)
            .then_with(|| b.utility_score.total_cmp(&a.utility_score))
    });

    ApproachEvaluationReport {
        ranked,
        total_workspace_ast_symbols: total_ast,
    }
}

impl ApproachEvaluationReport {
    /// Renders a structured Markdown trade-off matrix + ready-to-pass `ask_user` option cards.
    #[must_use]
    pub fn format_markdown(&self) -> String {
        if self.ranked.is_empty() {
            return String::new();
        }
        let mut out = String::from("\n<approach_pareto_matrix>\n");
        out.push_str(&format!(
            "### ⚖️ AST-Grounded Approach Trade-Off Matrix ({} AST symbols indexed)\n",
            self.total_workspace_ast_symbols
        ));
        for (idx, item) in self.ranked.iter().enumerate() {
            let badge = if idx == 0 && item.pareto_optimal {
                "★ Recommended · Pareto-Optimal"
            } else if item.pareto_optimal {
                "◆ Pareto-Optimal"
            } else {
                "○ Dominated"
            };
            out.push_str(&format!(
                "{}. **{}** (`{}`) — [{}] · Utility `{:.2}` · Complexity `{:.1}/5` · Files `{}` · AST Reuse `{:.0}%`\n",
                idx + 1,
                item.candidate.title,
                item.candidate.id,
                badge,
                item.utility_score,
                item.candidate.complexity,
                item.candidate.files_touched.len(),
                item.ast_reuse_overlap * 100.0
            ));
            if !item.candidate.summary.is_empty() {
                out.push_str(&format!("   - Summary: {}\n", item.candidate.summary));
            }
            if !item.matched_ast_symbols.is_empty() {
                out.push_str(&format!(
                    "   - Grounded AST Symbols: `{}`\n",
                    item.matched_ast_symbols.join("`, `")
                ));
            }
            if !item.candidate.pros.is_empty() {
                out.push_str(&format!("   - Pros: {}\n", item.candidate.pros.join("; ")));
            }
            if !item.candidate.cons.is_empty() {
                out.push_str(&format!("   - Cons: {}\n", item.candidate.cons.join("; ")));
            }
        }
        let ask_cards = self.ask_user_options_json();
        if let Ok(compact) = serde_json::to_string(&ask_cards) {
            out.push_str(&format!(
                "\n- **Ready `ask_user` trade-off options**: `{}`\n",
                compact
            ));
        }
        out.push_str("</approach_pareto_matrix>\n");
        out
    }

    /// Formats ranked approaches directly as `ask_user` `options` JSON with `pros` & `cons`.
    #[must_use]
    pub fn ask_user_options_json(&self) -> Value {
        let items: Vec<Value> = self
            .ranked
            .iter()
            .enumerate()
            .map(|(idx, item)| {
                let label = if idx == 0 {
                    format!("{} (Recommended)", item.candidate.title)
                } else {
                    item.candidate.title.clone()
                };
                json!({
                    "id": item.candidate.id,
                    "label": label,
                    "description": item.candidate.summary,
                    "pros": item.candidate.pros,
                    "cons": item.candidate.cons,
                })
            })
            .collect();
        Value::Array(items)
    }

    /// Renders a self-contained responsive dark-mode HTML architecture comparison board for `MiniBrowse`.
    #[must_use]
    pub fn render_comparison_html(
        &self,
        topic: &str,
        settled: &[SettledDecisionNode],
        open_questions: &[String],
    ) -> String {
        let esc = html_escape;
        let max_u = self
            .ranked
            .iter()
            .map(|r| r.utility_score)
            .fold(1.0_f64, f64::max)
            .max(1.0);

        let mut cards_html = String::new();
        for (idx, item) in self.ranked.iter().enumerate() {
            let badge = if idx == 0 && item.pareto_optimal {
                "<span class=\"badge rec\">★ Recommended · Pareto-Optimal</span>"
            } else if item.pareto_optimal {
                "<span class=\"badge pareto\">◆ Pareto-Optimal</span>"
            } else {
                "<span class=\"badge dom\">○ Dominated</span>"
            };
            let meter_pct = ((item.utility_score.max(0.2) / max_u) * 100.0).clamp(8.0, 100.0);
            let pros_li: String = item
                .candidate
                .pros
                .iter()
                .map(|p| format!("<li>+ {}</li>", esc(p)))
                .collect();
            let cons_li: String = item
                .candidate
                .cons
                .iter()
                .map(|c| format!("<li>− {}</li>", esc(c)))
                .collect();
            let files_pills: String = item
                .candidate
                .files_touched
                .iter()
                .map(|f| format!("<code>{}</code>", esc(f)))
                .collect::<Vec<_>>()
                .join(" ");

            cards_html.push_str(&format!(
                r#"<article class="card {rec_cls}">
  <header>
    <div class="card-top">{badge}<span class="score">Utility {u:.2}</span></div>
    <h2>{rank}. {title}</h2>
    <p class="summary">{summary}</p>
  </header>
  <div class="meter"><div class="meter-fill" style="width:{pct:.0}%"></div></div>
  <div class="metrics">
    <span>Complexity: <strong>{cplx:.1}/5</strong></span>
    <span>AST Reuse: <strong>{reuse:.0}%</strong></span>
    <span>Files: <strong>{fcount}</strong></span>
  </div>
  <div class="columns">
    <div class="pros"><h3>Pros</h3><ul>{pros}</ul></div>
    <div class="cons"><h3>Trade-offs</h3><ul>{cons}</ul></div>
  </div>
  <footer class="files">{files}</footer>
</article>"#,
                rec_cls = if idx == 0 { "featured" } else { "" },
                badge = badge,
                u = item.utility_score,
                rank = idx + 1,
                title = esc(&item.candidate.title),
                summary = esc(&item.candidate.summary),
                pct = meter_pct,
                cplx = item.candidate.complexity,
                reuse = item.ast_reuse_overlap * 100.0,
                fcount = item.candidate.files_touched.len(),
                pros = pros_li,
                cons = cons_li,
                files = files_pills,
            ));
        }

        let settled_html: String = if settled.is_empty() {
            "<p class=\"muted\">No settled decisions recorded yet.</p>".to_string()
        } else {
            let items: String = settled
                .iter()
                .take(8)
                .map(|d| {
                    format!(
                        "<li><strong>[{}]</strong> {}</li>",
                        esc(&d.id),
                        esc(&d.heading)
                    )
                })
                .collect();
            format!("<ul>{items}</ul>")
        };

        let open_html: String = if open_questions.is_empty() {
            "<p class=\"muted\">0 unresolved design blockers.</p>".to_string()
        } else {
            let items: String = open_questions
                .iter()
                .map(|q| format!("<li>{}</li>", esc(q)))
                .collect();
            format!("<ul>{items}</ul>")
        };

        format!(
            r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Brainstorm Architecture Companion — {topic}</title>
<style>
  :root {{ color-scheme: dark; --bg: #0b0f17; --panel: #131a27; --border: #243047; --text: #e6edf7; --muted: #8fa1bd; --accent: #38bdf8; --good: #34d399; --warn: #fbbf24; }}
  * {{ box-sizing: border-box; }}
  body {{ margin: 0; padding: 32px; font-family: system-ui, -apple-system, sans-serif; background: var(--bg); color: var(--text); line-height: 1.5; }}
  header.hero {{ max-width: 1200px; margin: 0 auto 28px; border-bottom: 1px solid var(--border); padding-bottom: 18px; }}
  h1 {{ margin: 0 0 6px; font-size: 1.65rem; }}
  .sub {{ color: var(--muted); font-size: 0.92rem; margin: 0; }}
  .grid {{ display: grid; grid-template-columns: repeat(auto-fit, minmax(340px, 1fr)); gap: 20px; max-width: 1200px; margin: 0 auto 28px; }}
  .card {{ background: var(--panel); border: 1px solid var(--border); border-radius: 12px; padding: 20px; display: flex; flex-direction: column; gap: 14px; }}
  .card.featured {{ border-color: var(--accent); box-shadow: 0 0 0 1px rgba(56,189,248,0.3); }}
  .card-top {{ display: flex; justify-content: space-between; align-items: center; gap: 8px; font-size: 0.8rem; }}
  .badge {{ padding: 3px 9px; border-radius: 999px; font-weight: 600; }}
  .badge.rec {{ background: rgba(52,211,153,0.16); color: var(--good); }}
  .badge.pareto {{ background: rgba(56,189,248,0.16); color: var(--accent); }}
  .badge.dom {{ background: rgba(143,161,189,0.15); color: var(--muted); }}
  .score {{ color: var(--muted); font-family: ui-monospace, monospace; }}
  .card h2 {{ margin: 6px 0 4px; font-size: 1.15rem; }}
  .summary {{ margin: 0; color: var(--muted); font-size: 0.9rem; }}
  .meter {{ height: 6px; background: #1d273b; border-radius: 999px; overflow: hidden; }}
  .meter-fill {{ height: 100%; background: linear-gradient(90deg, var(--accent), var(--good)); }}
  .metrics {{ display: flex; gap: 14px; font-size: 0.82rem; color: var(--muted); flex-wrap: wrap; }}
  .columns {{ display: grid; grid-template-columns: 1fr 1fr; gap: 12px; font-size: 0.86rem; }}
  .columns h3 {{ margin: 0 0 6px; font-size: 0.8rem; text-transform: uppercase; letter-spacing: 0.04em; color: var(--muted); }}
  .columns ul {{ margin: 0; padding-left: 16px; }}
  .pros li {{ color: #bbf7d0; }}
  .cons li {{ color: #fde68a; }}
  .files {{ display: flex; flex-wrap: wrap; gap: 6px; margin-top: auto; }}
  code {{ background: #1a2335; padding: 2px 6px; border-radius: 5px; font-size: 0.78rem; color: var(--accent); }}
  .meta-panels {{ display: grid; grid-template-columns: 1fr 1fr; gap: 20px; max-width: 1200px; margin: 0 auto; }}
  .meta-box {{ background: var(--panel); border: 1px solid var(--border); border-radius: 10px; padding: 16px 20px; font-size: 0.88rem; }}
  .meta-box h3 {{ margin: 0 0 8px; font-size: 0.95rem; }}
  .muted {{ color: var(--muted); margin: 0; }}
</style>
</head>
<body>
  <header class="hero">
    <h1>{topic}</h1>
    <p class="sub">minicode Visual Architecture Companion · Pareto Frontier &amp; AST Reuse Matrix ({ast_count} workspace symbols indexed)</p>
  </header>
  <section class="grid">{cards}</section>
  <section class="meta-panels">
    <div class="meta-box"><h3>Settled Decisions</h3>{settled}</div>
    <div class="meta-box"><h3>Active Open Questions</h3>{open}</div>
  </section>
</body>
</html>"#,
            topic = esc(topic),
            ast_count = self.total_workspace_ast_symbols,
            cards = cards_html,
            settled = settled_html,
            open = open_html,
        )
    }
}

fn html_escape(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

/// Returns `true` when an interactive desktop GUI session is available for live `MiniBrowse` preview.
#[must_use]
pub fn has_interactive_gui_display() -> bool {
    if cfg!(test) {
        return false;
    }
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        true
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        std::env::var_os("DISPLAY").is_some_and(|v| !v.is_empty())
            || std::env::var_os("WAYLAND_DISPLAY").is_some_and(|v| !v.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::graph::CodeGraph;

    #[test]
    fn test_pulldown_cmark_decision_extraction_and_jaccard_question_pruning() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = tmp.path();
        std::fs::create_dir_all(ws.join(".minicode")).unwrap();
        std::fs::write(
            ws.join(".minicode/decisions.md"),
            "# Project Decisions\n\n\
             ## [D1] Styling & CSS Architecture (answered by user, 2026-11-18 10:00 UTC)\n\
             - Styling framework: Tailwind CSS v4 with semantic CSS custom properties\n\n\
             ## [D2] Database Persistence Layer (answered by user, 2026-11-18 10:02 UTC)\n\
             - Storage engine: Embedded SQLite via rusqlite with WAL journaling\n\n\
             ## [D3] Assumed for Demo (auto-selected default (headless; not user-confirmed), 2026-11-18)\n\
             - Authentication provider: OAuth GitHub PKCE\n",
        )
        .unwrap();

        let settled = extract_settled_decisions(ws);
        // D3 is unconfirmed auto-default and must NOT prune open user questions!
        assert_eq!(settled.len(), 2);
        assert_eq!(settled[0].id, "D1");
        assert_eq!(settled[1].id, "D2");

        let questions = vec![
            "Which CSS styling framework should we use?".to_string(),
            "Should database storage use SQLite persistence?".to_string(),
            "Which OAuth authentication provider should we support?".to_string(),
        ];
        let filtered = filter_redundant_questions(&questions, &settled);
        assert_eq!(filtered.pruned_questions.len(), 2);
        assert_eq!(filtered.pruned_questions[0].matched_id, "D1");
        assert_eq!(filtered.pruned_questions[1].matched_id, "D2");
        assert_eq!(
            filtered.active_questions,
            vec!["Which OAuth authentication provider should we support?".to_string()]
        );
    }

    #[test]
    fn test_ast_grounded_pareto_scoring_ranks_reusable_approach_first() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = tmp.path();
        std::fs::create_dir_all(ws.join("src/context")).unwrap();
        std::fs::write(
            ws.join("src/context/working_memory.rs"),
            "pub struct WorkingMemory;\npub struct SwarmScheduler;\n",
        )
        .unwrap();

        // Populate `.minicode/graph_shards.bin` + `graph.json` with real Tree-sitter AST symbols
        let mut graph = CodeGraph::new();
        graph.build_graph(ws).unwrap();
        let snap = GraphStore::create_snapshot(&graph, ws);
        GraphStore::save_snapshot(&snap, ws).unwrap();

        let args = json!({
            "approaches": [
                {
                    "id": "bespoke_daemon",
                    "title": "New Standalone External Daemon",
                    "summary": "Build BrandNewCoordinatorDaemon from scratch across 6 new modules",
                    "pros": ["Complete isolation"],
                    "cons": ["Duplicates scheduler logic", "High maintenance burden", "IPC serialization overhead"],
                    "files_touched": ["src/daemon/a.rs", "src/daemon/b.rs", "src/daemon/c.rs", "src/daemon/d.rs"],
                    "complexity": 4.5
                },
                {
                    "id": "extend_working_memory",
                    "title": "Extend WorkingMemory & SwarmScheduler",
                    "summary": "Integrate topological waves directly into WorkingMemory and SwarmScheduler",
                    "pros": ["Reuses WorkingMemory persistence", "Zero duplicate state", "Clean single-binary flow"],
                    "cons": ["Requires schema migration guard"],
                    "files_touched": ["src/context/working_memory.rs"],
                    "complexity": 2.0
                }
            ]
        });

        let candidates = parse_approach_candidates(&args);
        assert_eq!(candidates.len(), 2);

        let eval = evaluate_approaches_against_ast(ws, &candidates);
        assert_eq!(eval.ranked.len(), 2);
        assert_eq!(eval.ranked[0].candidate.id, "extend_working_memory");
        assert!(eval.ranked[0].pareto_optimal);
        assert!(!eval.ranked[1].pareto_optimal);
        assert!(eval.ranked[0].ast_reuse_overlap > 0.5);
        assert!(eval.ranked[0]
            .matched_ast_symbols
            .contains(&"WorkingMemory".to_string()));

        let html = eval.render_comparison_html("DAG Unification", &[], &[]);
        assert!(html.contains("Extend WorkingMemory &amp; SwarmScheduler"));
        assert!(html.contains("Pareto-Optimal"));
    }
}

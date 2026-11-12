//! Live-Metadata BM25 Intent Classifier & Signal Vector Projector.
//!
//! Replaces static keyword/phrase rule tables with a runtime BM25 inverted index constructed
//! directly from registered [`ToolCategory`] descriptions, [`ToolSchema`] definitions (names,
//! positive descriptions, JSON parameter properties & enums), and [`VaultSkill`] frontmatter
//! (`triggers`, `description`, `allowed_tools`).

use crate::agent::provider::ToolSchema;
use crate::constants::{BM25_B, BM25_K1};
use crate::tools::category::ToolCategory;
use crate::vault::models::VaultSkill;
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

/// Minimum BM25F score required to auto-promote a domain `ToolCategory` from user prompt text.
const MIN_CATEGORY_BM25_SCORE: f64 = 3.0;
/// Relative score ratio against the top-scoring category to prevent noisy tail categories.
const RELATIVE_CATEGORY_CUTOFF: f64 = 0.12;
/// Field length normalization factors for 2-Field BM25F (Identifier/Action vs Prose).
const BM25F_ID_B: f64 = 0.25;
const BM25F_PROSE_B: f64 = 0.50;
const BM25F_ID_BOOST: f64 = 1.35;
/// Minimum BM25 score required to auto-activate an MCP server via skill/schema association.
const MIN_MCP_BM25_SCORE: f64 = 3.0;

/// Standard English function words, conversational greetings, and generic meta tokens
/// excluded from BM25 unigram query terms so conversational turns carry zero domain score.
const QUERY_STOPWORDS: &[&str] = &[
    "a",
    "an",
    "the",
    "to",
    "for",
    "with",
    "in",
    "on",
    "of",
    "and",
    "or",
    "is",
    "it",
    "its",
    "that",
    "this",
    "these",
    "those",
    "from",
    "into",
    "by",
    "as",
    "at",
    "be",
    "are",
    "was",
    "were",
    "so",
    "ok",
    "okay",
    "please",
    "me",
    "my",
    "you",
    "your",
    "u",
    "we",
    "our",
    "hello",
    "hi",
    "hii",
    "hey",
    "yo",
    "good",
    "morning",
    "afternoon",
    "evening",
    "thanks",
    "thank",
    "lot",
    "there",
    "world",
    "minicode",
    "what",
    "how",
    "much",
    "many",
    "who",
    "where",
    "when",
    "why",
    "can",
    "could",
    "would",
    "should",
    "do",
    "does",
    "did",
    "using",
    "use",
    "first",
    "new",
    "latest",
    "modern",
    "changes",
    "change",
    "taking",
    "take",
    "work",
    "works",
    "against",
    "some",
    "any",
    "all",
    "about",
    "current",
    "now",
];

#[inline]
fn is_stopword(token: &str) -> bool {
    token.len() < 2 || QUERY_STOPWORDS.contains(&token)
}

#[inline]
fn canonical_stem(lower_word: &str) -> &str {
    let singular = lower_word
        .strip_suffix('s')
        .filter(|s| s.len() >= 4 && !s.ends_with('s'))
        .unwrap_or(lower_word);
    VaultSkill::normalize_stem(singular)
}

/// Extracts normalized unigram stems and adjacent stemmed bigrams from text.
fn extract_terms_and_bigrams(text: &str) -> Vec<String> {
    let raw_words: Vec<String> = text
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .filter(|w| !w.is_empty())
        .map(|w| w.to_ascii_lowercase())
        .collect();

    let mut out = Vec::with_capacity(raw_words.len() * 2);
    for w in &raw_words {
        if w.contains('_') {
            out.push(w.clone());
            let parts: Vec<&str> = w.split('_').filter(|p| !p.is_empty()).collect();
            for p in &parts {
                if !is_stopword(p) {
                    out.push(canonical_stem(p).to_string());
                }
            }
            for pair in parts.windows(2) {
                let s0 = canonical_stem(pair[0]);
                let s1 = canonical_stem(pair[1]);
                out.push(format!("{} {}", s0, s1));
            }
        } else if !is_stopword(w) {
            out.push(canonical_stem(w).to_string());
        }
    }

    for pair in raw_words.windows(2) {
        if is_stopword(&pair[0]) && is_stopword(&pair[1]) {
            continue;
        }
        let s0 = canonical_stem(&pair[0]);
        let s1 = canonical_stem(&pair[1]);
        if !s0.is_empty() && !s1.is_empty() {
            out.push(format!("{} {}", s0, s1));
        }
    }
    out
}

/// Splits schema/skill prose on sentence boundaries (`\n` or `. ` followed by an uppercase letter)
/// without shattering inline abbreviations (`e.g.`, `etc.`) or filenames (`http.server`, `todo.md`),
/// and filters out negative disclaimer clauses (`DO NOT use...`, `Never...`).
fn positive_prose_clauses(text: &str) -> Vec<(usize, &str)> {
    let bytes = text.as_bytes();
    let mut clauses = Vec::new();
    let mut start = 0;
    let mut i = 0;
    while i < bytes.len() {
        let is_newline = bytes[i] == b'\n';
        let is_sentence_dot = bytes[i] == b'.' && {
            let mut j = i + 1;
            while j < bytes.len() && bytes[j] == b' ' {
                j += 1;
            }
            j > i + 1 && j < bytes.len() && bytes[j].is_ascii_uppercase()
        };
        if is_newline || is_sentence_dot {
            let slice = text[start..i].trim();
            if !slice.is_empty() {
                clauses.push(slice);
            }
            start = i + 1;
        }
        i += 1;
    }
    let tail = text[start..].trim();
    if !tail.is_empty() {
        clauses.push(tail);
    }

    clauses
        .into_iter()
        .filter(|s| {
            let lower = s.to_ascii_lowercase();
            !lower.contains("do not use")
                && !lower.contains("don't use")
                && !lower.starts_with("never ")
                && !lower.contains("not responsible for")
        })
        .enumerate()
        .collect()
}

type TermFreqMap = HashMap<String, f64>;
type CategoryFieldMaps = HashMap<ToolCategory, (TermFreqMap, TermFreqMap)>;

struct CategoryDoc {
    category: ToolCategory,
    id_tf: HashMap<String, f64>,
    id_len: f64,
    prose_tf: HashMap<String, f64>,
    prose_len: f64,
}

struct SkillMcpProfile {
    skill_tokens: HashSet<String>,
    tf: HashMap<String, f64>,
    len: f64,
}

struct MetadataBm25Index {
    docs: Vec<CategoryDoc>,
    id_df: HashMap<String, usize>,
    id_avgdl: f64,
    prose_df: HashMap<String, usize>,
    prose_avgdl: f64,
    mcp_profiles: Vec<SkillMcpProfile>,
    mcp_df: HashMap<String, usize>,
    mcp_avgdl: f64,
}

impl MetadataBm25Index {
    fn global() -> &'static Self {
        static INDEX: OnceLock<MetadataBm25Index> = OnceLock::new();
        INDEX.get_or_init(Self::build)
    }

    fn add_weighted_terms(tf: &mut HashMap<String, f64>, text: &str, weight: f64) {
        for term in extract_terms_and_bigrams(text) {
            *tf.entry(term).or_insert(0.0) += weight;
        }
    }

    fn index_schema(
        id_tf: &mut HashMap<String, f64>,
        prose_tf: &mut HashMap<String, f64>,
        schema: &ToolSchema,
    ) {
        Self::add_weighted_terms(id_tf, &schema.name, 4.0);
        for (idx, clause) in positive_prose_clauses(&schema.description) {
            let w = if idx == 0 { 3.0 } else { 1.0 };
            Self::add_weighted_terms(prose_tf, clause, w);
        }
        if let Some(props) = schema
            .parameters
            .get("properties")
            .and_then(|p| p.as_object())
        {
            for (prop_name, prop_val) in props {
                Self::add_weighted_terms(prose_tf, prop_name, 2.0);
                let is_action_prop = matches!(prop_name.as_str(), "action" | "subcommand" | "mode");
                if let Some(enums) = prop_val.get("enum").and_then(|e| e.as_array()) {
                    for val in enums.iter().filter_map(|v| v.as_str()) {
                        if is_action_prop {
                            Self::add_weighted_terms(id_tf, val, 3.0);
                        } else {
                            Self::add_weighted_terms(prose_tf, val, 2.5);
                        }
                    }
                }
                if let Some(desc) = prop_val.get("description").and_then(|d| d.as_str()) {
                    for (_, clause) in positive_prose_clauses(desc) {
                        Self::add_weighted_terms(prose_tf, clause, 1.0);
                    }
                }
            }
        }
    }

    fn category_serde_aliases(cat: ToolCategory) -> &'static str {
        match cat {
            ToolCategory::MiniKit => "onpkg minikit kit",
            ToolCategory::MiniPower => "power minipower superpower",
            ToolCategory::Blocks => "miniblocks blocks",
            ToolCategory::Dev => "minidev minitask dev",
            ToolCategory::Vault => "minivault vault",
            _ => "",
        }
    }

    fn build() -> Self {
        // Domain categories eligible for prompt-driven auto-activation (`Files` core tools are always active)
        let domain_cats: Vec<ToolCategory> = ToolCategory::ALL
            .into_iter()
            .filter(|&c| c != ToolCategory::Files)
            .collect();

        let mut cat_fields: CategoryFieldMaps = HashMap::new();
        for &cat in &domain_cats {
            let mut id_tf = HashMap::new();
            let mut prose_tf = HashMap::new();
            Self::add_weighted_terms(&mut id_tf, cat.name(), 5.0);
            Self::add_weighted_terms(&mut id_tf, Self::category_serde_aliases(cat), 5.0);
            Self::add_weighted_terms(&mut prose_tf, cat.description(), 4.0);
            for schema in cat.get_schemas() {
                Self::index_schema(&mut id_tf, &mut prose_tf, &schema);
            }
            if cat == ToolCategory::Codegraph {
                for schema in crate::tools::registry::context_tools::graph::get_schemas() {
                    Self::index_schema(&mut id_tf, &mut prose_tf, &schema);
                }
            }
            cat_fields.insert(cat, (id_tf, prose_tf));
        }

        // Snapshot each category's intrinsic schema vocabulary so multi-category skills project
        // domain-specific triggers only onto the categories whose schemas anchor those terms.
        let own_vocab: HashMap<ToolCategory, HashSet<String>> = cat_fields
            .iter()
            .map(|(&cat, (id_tf, prose_tf))| {
                let mut set: HashSet<String> = id_tf.keys().cloned().collect();
                set.extend(prose_tf.keys().cloned());
                (cat, set)
            })
            .collect();

        // Index built-in SKILL.md metadata onto owning ToolCategories via inverse tool-ubiquity weighting
        let builtin_skills = crate::vault::builtin::get_all_builtin_skills();
        let total_skills = builtin_skills.len().max(1) as f64;
        let mut tool_skill_df: HashMap<String, usize> = HashMap::new();
        for skill in &builtin_skills {
            for tool in &skill.frontmatter.allowed_tools {
                *tool_skill_df.entry(tool.clone()).or_insert(0) += 1;
            }
        }

        let mut mcp_profiles = Vec::with_capacity(builtin_skills.len());
        for skill in &builtin_skills {
            // Project skill triggers & description onto its specialized domain categories
            let mut cat_weights: HashMap<ToolCategory, (f64, usize)> = HashMap::new();
            let mut non_core_total = 0usize;
            for tool in &skill.frontmatter.allowed_tools {
                if let Some(cat) = crate::tools::meta::category_of(tool) {
                    if cat == ToolCategory::Files
                        || crate::constants::CORE_ACTIVE_TOOL_NAMES.contains(&tool.as_str())
                    {
                        continue;
                    }
                    non_core_total += 1;
                    let df_t = *tool_skill_df.get(tool).unwrap_or(&1) as f64;
                    let inv_ubiq = (1.0 + total_skills / df_t).ln();
                    let entry = cat_weights.entry(cat).or_insert((0.0, 0));
                    if inv_ubiq > entry.0 {
                        entry.0 = inv_ubiq;
                    }
                    entry.1 += 1;
                }
            }
            let multi_cat = cat_weights.len() > 1;
            for (&cat, &(w_scale, cat_tool_count)) in &cat_weights {
                let scale = (w_scale / 2.5).clamp(0.5, 1.5);
                let share = if multi_cat {
                    (cat_tool_count as f64 / non_core_total.max(1) as f64).clamp(0.25, 1.0)
                } else {
                    1.0
                };
                if let Some((id_tf, prose_tf)) = cat_fields.get_mut(&cat) {
                    Self::add_weighted_terms(
                        id_tf,
                        &skill.name.replace('-', " "),
                        2.5 * scale * share,
                    );
                    for (_, clause) in positive_prose_clauses(&skill.description) {
                        Self::add_weighted_terms(prose_tf, clause, 2.0 * scale * share);
                    }
                }
            }
            for trig in &skill.frontmatter.triggers {
                if !multi_cat {
                    for (&cat, &(w_scale, _)) in &cat_weights {
                        let scale = (w_scale / 2.5).clamp(0.5, 1.5);
                        if let Some((id_tf, _)) = cat_fields.get_mut(&cat) {
                            Self::add_weighted_terms(id_tf, trig, 4.0 * scale);
                        }
                    }
                } else {
                    let trig_terms = extract_terms_and_bigrams(trig);
                    let anchored_cats: Vec<ToolCategory> = cat_weights
                        .keys()
                        .copied()
                        .filter(|cat| {
                            own_vocab
                                .get(cat)
                                .is_some_and(|vocab| trig_terms.iter().any(|t| vocab.contains(t)))
                        })
                        .collect();
                    if !anchored_cats.is_empty() {
                        for cat in anchored_cats {
                            if let Some(&(w_scale, _)) = cat_weights.get(&cat) {
                                let scale = (w_scale / 2.5).clamp(0.5, 1.5);
                                if let Some((id_tf, _)) = cat_fields.get_mut(&cat) {
                                    Self::add_weighted_terms(id_tf, trig, 4.0 * scale);
                                }
                            }
                        }
                    } else {
                        for (&cat, &(w_scale, cat_tool_count)) in &cat_weights {
                            let scale = (w_scale / 2.5).clamp(0.5, 1.5);
                            let share = (cat_tool_count as f64 / non_core_total.max(1) as f64)
                                .clamp(0.35, 1.0);
                            if let Some((id_tf, _)) = cat_fields.get_mut(&cat) {
                                Self::add_weighted_terms(id_tf, trig, 4.0 * scale * share);
                            }
                        }
                    }
                }
            }

            // Build skill MCP association profile for schema-less server discovery
            let mut skill_tokens = HashSet::new();
            for part in skill.name.split(['-', '_']) {
                if !part.is_empty() {
                    skill_tokens.insert(part.to_ascii_lowercase());
                }
            }
            for trig in &skill.frontmatter.triggers {
                for w in trig.split(|c: char| !c.is_ascii_alphanumeric()) {
                    if w.len() >= 2 {
                        skill_tokens.insert(w.to_ascii_lowercase());
                    }
                }
            }
            let mut stf = HashMap::new();
            for trig in &skill.frontmatter.triggers {
                Self::add_weighted_terms(&mut stf, trig, 4.0);
            }
            for (_, clause) in positive_prose_clauses(&skill.description) {
                Self::add_weighted_terms(&mut stf, clause, 2.0);
            }
            let slen: f64 = stf.values().sum();
            if slen > 0.0 {
                mcp_profiles.push(SkillMcpProfile {
                    skill_tokens,
                    tf: stf,
                    len: slen,
                });
            }
        }

        let mut id_df: HashMap<String, usize> = HashMap::new();
        let mut prose_df: HashMap<String, usize> = HashMap::new();
        let mut docs = Vec::with_capacity(domain_cats.len());
        let mut total_id_len = 0.0;
        let mut total_prose_len = 0.0;
        for cat in domain_cats {
            let (id_tf, prose_tf) = cat_fields.remove(&cat).unwrap_or_default();
            let id_len: f64 = id_tf.values().sum();
            let prose_len: f64 = prose_tf.values().sum();
            for term in id_tf.keys() {
                *id_df.entry(term.clone()).or_insert(0) += 1;
            }
            for term in prose_tf.keys() {
                *prose_df.entry(term.clone()).or_insert(0) += 1;
            }
            total_id_len += id_len;
            total_prose_len += prose_len;
            docs.push(CategoryDoc {
                category: cat,
                id_tf,
                id_len,
                prose_tf,
                prose_len,
            });
        }
        let doc_count = docs.len().max(1) as f64;
        let id_avgdl = (total_id_len / doc_count).max(1.0);
        let prose_avgdl = (total_prose_len / doc_count).max(1.0);

        let mut mcp_df: HashMap<String, usize> = HashMap::new();
        let mut mcp_total_len = 0.0;
        for p in &mcp_profiles {
            for term in p.tf.keys() {
                *mcp_df.entry(term.clone()).or_insert(0) += 1;
            }
            mcp_total_len += p.len;
        }
        let mcp_avgdl = if mcp_profiles.is_empty() {
            1.0
        } else {
            (mcp_total_len / mcp_profiles.len() as f64).max(1.0)
        };

        Self {
            docs,
            id_df,
            id_avgdl,
            prose_df,
            prose_avgdl,
            mcp_profiles,
            mcp_df,
            mcp_avgdl,
        }
    }

    fn score_categories(&self, prompt: &str) -> Vec<(f64, ToolCategory)> {
        let lower = prompt.to_ascii_lowercase();
        let opening_end = lower.floor_char_boundary(
            lower
                .len()
                .min(crate::constants::INTENT_OPENING_CLAUSE_CHAR_WINDOW),
        );
        let opening_terms: HashSet<String> = extract_terms_and_bigrams(&lower[..opening_end])
            .into_iter()
            .collect();
        let all_terms: HashSet<String> = extract_terms_and_bigrams(&lower).into_iter().collect();
        if all_terms.is_empty() {
            return Vec::new();
        }

        let n_docs = self.docs.len() as f64;
        let w_open = crate::constants::INTENT_OPENING_CLAUSE_WEIGHT as f64;

        let mut scored: Vec<(f64, ToolCategory)> = self
            .docs
            .iter()
            .filter_map(|doc| {
                let mut score = 0.0;
                for term in &all_terms {
                    let q_weight = if opening_terms.contains(term) {
                        w_open
                    } else {
                        1.0
                    };
                    let mut s_id = 0.0;
                    if let Some(&tf_id) = doc.id_tf.get(term) {
                        let df = *self.id_df.get(term).unwrap_or(&1) as f64;
                        let idf = (1.0 + (n_docs - df + 0.5) / (df + 0.5)).ln();
                        let denom = tf_id
                            + BM25_K1
                                * (1.0 - BM25F_ID_B + BM25F_ID_B * (doc.id_len / self.id_avgdl));
                        let bm25_tf = (tf_id * (BM25_K1 + 1.0)) / denom.max(1e-6);
                        s_id = BM25F_ID_BOOST * q_weight * idf * bm25_tf;
                    }
                    let mut s_pr = 0.0;
                    if let Some(&tf_pr) = doc.prose_tf.get(term) {
                        let df = *self.prose_df.get(term).unwrap_or(&1) as f64;
                        let idf = (1.0 + (n_docs - df + 0.5) / (df + 0.5)).ln();
                        let denom = tf_pr
                            + BM25_K1
                                * (1.0 - BM25F_PROSE_B
                                    + BM25F_PROSE_B * (doc.prose_len / self.prose_avgdl));
                        let bm25_tf = (tf_pr * (BM25_K1 + 1.0)) / denom.max(1e-6);
                        s_pr = q_weight * idf * bm25_tf;
                    }
                    score += s_id.max(s_pr) + 0.25 * s_id.min(s_pr);
                }
                // URL literal directly signals Web fetch/navigation
                if doc.category == ToolCategory::Web
                    && (lower.contains("http://") || lower.contains("https://"))
                {
                    score += 8.0;
                }
                (score >= MIN_CATEGORY_BM25_SCORE).then_some((score, doc.category))
            })
            .collect();

        scored.sort_by(|a, b| b.0.total_cmp(&a.0));
        scored
    }

    fn score_mcp_server_by_skills(&self, server_lower: &str, query_terms: &HashSet<String>) -> f64 {
        if query_terms.is_empty() || self.mcp_profiles.is_empty() {
            return 0.0;
        }
        let n_docs = self.mcp_profiles.len() as f64;
        let mut best = 0.0_f64;

        for profile in &self.mcp_profiles {
            if !profile.skill_tokens.contains(server_lower) {
                continue;
            }
            let mut score = 0.0;
            for term in query_terms {
                if term == server_lower {
                    continue;
                }
                let Some(&tf) = profile.tf.get(term) else {
                    continue;
                };
                let df = *self.mcp_df.get(term).unwrap_or(&1) as f64;
                let idf = (1.0 + (n_docs - df + 0.5) / (df + 0.5)).ln();
                let denom = tf + BM25_K1 * (1.0 - BM25_B + BM25_B * (profile.len / self.mcp_avgdl));
                score += idf * ((tf * (BM25_K1 + 1.0)) / denom.max(1e-6));
            }
            if score > best {
                best = score;
            }
        }
        best
    }
}

/// Multi-dimensional signal vector extracted from a user prompt in O(N) time using
/// live metadata BM25 domain scoring, `META_TASK_ACTION_VERBS` token sets, code morphology markers,
/// and active plan lexical overlap. Drives proportional context allocation, progressive
/// skill disclosure, and plan-continuation gating without regexes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PromptSignalVector {
    pub word_count: usize,
    pub domain_score: usize,
    pub action_score: usize,
    pub code_morphology_score: usize,
    pub plan_continuation_score: usize,
    pub is_meta_query: bool,
}

impl PromptSignalVector {
    /// Total actionable engineering signal energy $E(P) = S_{\text{domain}} + S_{\text{action}} + S_{\text{code}} + S_{\text{plan}}$.
    #[inline]
    #[must_use]
    pub const fn task_energy(&self) -> usize {
        self.domain_score
            .saturating_add(self.action_score)
            .saturating_add(self.code_morphology_score)
            .saturating_add(self.plan_continuation_score)
    }

    /// Returns `true` when the prompt carries positive engineering/task signal and is not a meta-introspection query.
    #[inline]
    #[must_use]
    pub const fn has_actionable_task_intent(&self) -> bool {
        !self.is_meta_query && self.task_energy() > 0
    }

    /// Returns `true` when the prompt introduces a substantive engineering goal or specification
    /// (positive domain, action, or code-morphology signal), rather than a bare continuation nudge
    /// (`"continue"`, `"proceed"`) that carries only `plan_continuation_score`.
    #[inline]
    #[must_use]
    pub const fn has_substantive_goal_intent(&self) -> bool {
        !self.is_meta_query
            && self
                .domain_score
                .saturating_add(self.action_score)
                .saturating_add(self.code_morphology_score)
                > 0
    }

    /// Returns `true` when the prompt exhibits lexical or semantic affinity with an active plan/todo.
    #[inline]
    #[must_use]
    pub const fn has_plan_continuation_affinity(&self) -> bool {
        !self.is_meta_query && self.plan_continuation_score > 0
    }

    /// Returns `true` when the prompt carries sufficient structural/action density to warrant AST/CodeGraph grounding.
    #[inline]
    #[must_use]
    pub const fn has_code_exploration_intent(&self) -> bool {
        !self.is_meta_query
            && self
                .action_score
                .saturating_add(self.code_morphology_score)
                .saturating_add(self.domain_score)
                >= crate::constants::CODE_EXPLORATION_MIN_ENERGY
    }
}

/// High-speed, metadata-indexed BM25 intent classifier for dynamic tool gating and signal vector projection.
pub struct IntentClassifier;

#[inline]
fn tokenize_words(lower: &str) -> HashSet<&str> {
    lower
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .filter(|w| !w.is_empty())
        .collect()
}

#[inline]
fn has_file_extension_shape(token: &str) -> bool {
    let clean = token.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '.');
    clean.rsplit_once('.').is_some_and(|(stem, ext)| {
        !stem.is_empty()
            && (1..=crate::constants::FILE_EXT_MAX_LEN).contains(&ext.len())
            && ext.chars().all(|c| c.is_ascii_alphanumeric())
            && ext.chars().any(|c| c.is_ascii_alphabetic())
    })
}

#[inline]
fn count_code_morphology_tokens(prompt: &str) -> usize {
    prompt
        .split_whitespace()
        .filter(|raw| {
            let has_structural_punct = crate::constants::CODE_MORPHOLOGY_CHAR_MARKERS
                .iter()
                .any(|&ch| raw.contains(ch))
                || crate::constants::CODE_MORPHOLOGY_STR_MARKERS
                    .iter()
                    .any(|&pat| raw.contains(pat));
            let has_camel_transition = raw
                .as_bytes()
                .windows(2)
                .any(|w| w[0].is_ascii_lowercase() && w[1].is_ascii_uppercase());
            has_structural_punct || has_camel_transition || has_file_extension_shape(raw)
        })
        .count()
}

impl IntentClassifier {
    /// Returns `true` when the user prompt is a concise conversational meta-introspection query about
    /// the agent's own tools, features, or skills — never a multi-line product specification or
    /// an imperative engineering build instruction.
    #[must_use]
    pub fn is_meta_introspection_query(prompt: &str) -> bool {
        let non_empty_lines = prompt.lines().filter(|l| !l.trim().is_empty()).count();
        if non_empty_lines > 2 {
            return false;
        }

        let lower = prompt.to_ascii_lowercase();
        let seq_words: Vec<&str> = lower
            .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
            .filter(|w| !w.is_empty())
            .collect();
        if seq_words.is_empty() || seq_words.len() > 28 {
            return false;
        }

        // If the utterance opens directly with a task action verb (e.g. "create a landing page for minicode..."),
        // it is an imperative engineering command rather than a conversational capability question.
        let first_substantive = seq_words.iter().copied().find(|w| {
            !matches!(
                *w,
                "please" | "now" | "ok" | "okay" | "so" | "hey" | "hi" | "hii" | "minicode"
            )
        });
        if let Some(opening) = first_substantive {
            if crate::constants::META_TASK_ACTION_VERBS.contains(&opening) {
                return false;
            }
        }

        let words: HashSet<&str> = seq_words.iter().copied().collect();
        let has_subject = crate::constants::META_INTROSPECTION_SUBJECT_TOKENS
            .iter()
            .any(|s| words.contains(s));
        let has_cue = crate::constants::META_INTROSPECTION_QUERY_CUES
            .iter()
            .any(|c| words.contains(c));
        let has_action_verb = crate::constants::META_TASK_ACTION_VERBS
            .iter()
            .any(|v| words.contains(v));
        let has_agent_state_predication = crate::constants::META_AGENT_PRONOUN_TOKENS
            .iter()
            .any(|p| words.contains(p))
            && crate::constants::META_STATE_PREDICATE_TOKENS
                .iter()
                .any(|s| words.contains(s));
        let has_cardinality_interrogative = words.contains("how") && words.contains("many");

        has_subject
            && has_cue
            && (!has_action_verb || has_agent_state_predication || has_cardinality_interrogative)
    }

    /// Projects a user prompt (and optional active plan/todo text) into a `PromptSignalVector`
    /// using live metadata BM25 scoring, action-verb morphology, code markers, and plan overlap.
    #[must_use]
    pub fn analyze_signal(prompt: &str, active_plan_context: Option<&str>) -> PromptSignalVector {
        let is_meta_query = Self::is_meta_introspection_query(prompt);
        let lower = prompt.to_ascii_lowercase();
        let words = tokenize_words(&lower);
        let word_count = words.len();
        let active_mask = usize::from(!is_meta_query);

        let scored = MetadataBm25Index::global().score_categories(prompt);
        let raw_bm25_sum: f64 = scored.iter().map(|(s, _)| *s).sum();
        let domain_score = (raw_bm25_sum.round() as usize) * active_mask;

        let action_score = crate::constants::META_TASK_ACTION_VERBS
            .iter()
            .filter(|&&v| words.contains(v))
            .count()
            * active_mask;

        let code_morphology_score = count_code_morphology_tokens(prompt) * active_mask;

        let cue_hits = crate::constants::PLAN_CONTINUATION_CUE_TOKENS
            .iter()
            .filter(|&&c| words.contains(c))
            .count();

        let plan_overlap = active_plan_context
            .map(|plan_text| {
                let plan_lower = plan_text.to_ascii_lowercase();
                let plan_stems: HashSet<&str> = tokenize_words(&plan_lower)
                    .into_iter()
                    .filter(|w| w.len() >= crate::constants::PLAN_AFFINITY_MIN_TOKEN_LEN)
                    .map(VaultSkill::normalize_stem)
                    .collect();
                words
                    .iter()
                    .filter(|&&w| {
                        w.len() >= crate::constants::PLAN_AFFINITY_MIN_TOKEN_LEN
                            && plan_stems.contains(VaultSkill::normalize_stem(w))
                    })
                    .count()
            })
            .unwrap_or(0);

        let has_non_continuation_content = words.iter().any(|&w| {
            !is_stopword(w) && !crate::constants::PLAN_CONTINUATION_CUE_TOKENS.contains(&w)
        });
        let is_pure_continuation =
            cue_hits > 0 && !has_non_continuation_content && code_morphology_score == 0;
        let effective_domain_score = if is_pure_continuation {
            0
        } else {
            domain_score
        };
        let effective_action_score = if is_pure_continuation {
            0
        } else {
            action_score
        };

        let has_intent_carrier =
            usize::from(effective_action_score + effective_domain_score + cue_hits > 0);
        let plan_continuation_score = (cue_hits + plan_overlap * has_intent_carrier) * active_mask;

        PromptSignalVector {
            word_count,
            domain_score: effective_domain_score,
            action_score: effective_action_score,
            code_morphology_score,
            plan_continuation_score,
            is_meta_query,
        }
    }

    /// Detects relevant tool categories from user prompt text via opening-clause-weighted BM25
    /// over live tool/skill metadata, capped at `MAX_INTENT_AUTO_CATEGORIES`.
    pub fn detect(prompt: &str) -> HashSet<ToolCategory> {
        let mut categories = HashSet::new();
        if Self::is_meta_introspection_query(prompt) {
            return categories;
        }

        let scored = MetadataBm25Index::global().score_categories(prompt);
        let Some(&(best_score, _)) = scored.first() else {
            return categories;
        };
        let min_cutoff = (best_score * RELATIVE_CATEGORY_CUTOFF).max(MIN_CATEGORY_BM25_SCORE);

        for (score, cat) in scored {
            if score < min_cutoff
                || categories.len() >= crate::constants::MAX_INTENT_AUTO_CATEGORIES
            {
                break;
            }
            categories.insert(cat);
        }

        categories
    }

    /// Detects relevant MCP server names using direct server-name matching + BM25 over live MCP
    /// `ToolSchema`s (and falls back to built-in `SKILL.md` profiles when schemas are absent).
    #[must_use]
    pub fn detect_mcp_servers_with_schemas(
        prompt: &str,
        mcp_tools_by_server: &HashMap<String, Vec<ToolSchema>>,
    ) -> HashSet<String> {
        let mut matched = HashSet::new();
        if Self::is_meta_introspection_query(prompt) || mcp_tools_by_server.is_empty() {
            return matched;
        }
        let lower = prompt.to_ascii_lowercase();
        let words = tokenize_words(&lower);
        let query_terms: HashSet<String> = extract_terms_and_bigrams(&lower).into_iter().collect();
        if query_terms.is_empty() {
            return matched;
        }

        let index = MetadataBm25Index::global();
        let mut server_docs: Vec<(&String, HashMap<String, f64>, f64)> = Vec::new();
        let mut df: HashMap<String, usize> = HashMap::new();
        let mut total_len = 0.0;

        for (srv, schemas) in mcp_tools_by_server {
            let s_lower = srv.to_ascii_lowercase();
            let s_stem = canonical_stem(&s_lower);
            if words.contains(s_lower.as_str()) || query_terms.contains(s_stem) {
                matched.insert(srv.clone());
                continue;
            }
            if schemas.is_empty() {
                if index.score_mcp_server_by_skills(&s_lower, &query_terms) >= MIN_MCP_BM25_SCORE {
                    matched.insert(srv.clone());
                }
                continue;
            }

            let mut tf = HashMap::new();
            MetadataBm25Index::add_weighted_terms(&mut tf, srv, 5.0);
            for schema in schemas {
                let short_name = schema
                    .name
                    .strip_prefix("mcp__")
                    .and_then(|s| s.split_once("__").map(|(_, rest)| rest))
                    .unwrap_or(&schema.name);
                MetadataBm25Index::add_weighted_terms(&mut tf, short_name, 5.0);
                MetadataBm25Index::add_weighted_terms(&mut tf, &schema.description, 2.0);
            }
            let len: f64 = tf.values().sum();
            for term in tf.keys() {
                *df.entry(term.clone()).or_insert(0) += 1;
            }
            total_len += len;
            server_docs.push((srv, tf, len));
        }

        if server_docs.is_empty() {
            return matched;
        }

        // Smooth corpus size with a background prior of 5 virtual servers so IDF remains
        // well-calibrated even when only 1 or 2 MCP servers are connected.
        let effective_n = (server_docs.len() + 5) as f64;
        let avgdl = (total_len / server_docs.len() as f64).max(1.0);
        for (srv, tf, len) in server_docs {
            let mut score = 0.0;
            for term in &query_terms {
                let Some(&f) = tf.get(term) else {
                    continue;
                };
                let doc_freq = *df.get(term).unwrap_or(&1) as f64;
                let idf = (1.0 + (effective_n - doc_freq + 0.5) / (doc_freq + 0.5)).ln();
                let denom = f + BM25_K1 * (1.0 - BM25_B + BM25_B * (len / avgdl));
                score += idf * ((f * (BM25_K1 + 1.0)) / denom.max(1e-6));
            }
            if score >= MIN_MCP_BM25_SCORE {
                matched.insert(srv.clone());
            }
        }

        matched
    }

    /// Detects relevant MCP server names from user prompt text and available servers via
    /// server-name matching and associative BM25 over built-in `SKILL.md` profiles.
    #[must_use]
    pub fn detect_mcp_servers<I, S>(prompt: &str, available_servers: I) -> HashSet<String>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut matched = HashSet::new();
        if Self::is_meta_introspection_query(prompt) {
            return matched;
        }
        let lower = prompt.to_ascii_lowercase();
        let words = tokenize_words(&lower);
        let query_terms: HashSet<String> = extract_terms_and_bigrams(&lower).into_iter().collect();
        let index = MetadataBm25Index::global();

        for server in available_servers {
            let s_ref = server.as_ref();
            let s_lower = s_ref.to_ascii_lowercase();
            let s_stem = canonical_stem(&s_lower);
            if words.contains(s_lower.as_str()) || query_terms.contains(s_stem) {
                matched.insert(s_ref.to_string());
                continue;
            }
            if index.score_mcp_server_by_skills(&s_lower, &query_terms) >= MIN_MCP_BM25_SCORE {
                matched.insert(s_ref.to_string());
            }
        }

        matched
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_intent_detection_empty_or_greeting() {
        let detected = IntentClassifier::detect("hello world");
        assert!(detected.is_empty());

        let detected = IntentClassifier::detect("hii minicode");
        assert!(detected.is_empty());
    }

    #[test]
    fn test_intent_detection_git() {
        let detected = IntentClassifier::detect("please commit these changes with git");
        assert!(detected.contains(&ToolCategory::Git));
    }

    #[test]
    fn test_intent_detection_web() {
        let detected = IntentClassifier::detect("search web for latest ratatui examples");
        assert!(detected.contains(&ToolCategory::Web));
    }

    #[test]
    fn test_intent_detection_codegraph() {
        let detected =
            IntentClassifier::detect("analyze the blast radius of changing this function");
        assert!(detected.contains(&ToolCategory::Codegraph));
    }

    #[test]
    fn test_intent_detection_minikit() {
        let detected = IntentClassifier::detect("scaffold a new stack with minikit");
        assert!(detected.contains(&ToolCategory::MiniKit));
        let detected2 = IntentClassifier::detect("scaffold a new stack with onpkg");
        assert!(detected2.contains(&ToolCategory::MiniKit));
    }

    #[test]
    fn test_intent_detection_minipower() {
        let detected =
            IntentClassifier::detect("verify these changes against the verification barrier");
        assert!(detected.contains(&ToolCategory::MiniPower));
        let detected2 =
            IntentClassifier::detect("execute this task in an isolated worktree with minipower");
        assert!(detected2.contains(&ToolCategory::MiniPower));
        let detected3 =
            IntentClassifier::detect("brainstorm the architecture first using socratic method");
        assert!(detected3.contains(&ToolCategory::MiniPower));
    }

    #[test]
    fn test_intent_detection_multiple() {
        let detected = IntentClassifier::detect("check git diff and search web for docs");
        assert!(detected.contains(&ToolCategory::Git));
        assert!(detected.contains(&ToolCategory::Web));
    }

    #[test]
    fn test_intent_detection_blocks() {
        let detected = IntentClassifier::detect("search for a responsive navbar component");
        assert!(detected.contains(&ToolCategory::Blocks));
        let detected2 = IntentClassifier::detect("show me modern color palettes with hex tokens");
        assert!(detected2.contains(&ToolCategory::Blocks));
        let detected3 = IntentClassifier::detect("scaffold a hero section from miniblocks");
        assert!(detected3.contains(&ToolCategory::Blocks));
        let detected4 = IntentClassifier::detect(
            "create a modern landing page with a pricing table in dark mode",
        );
        assert!(detected4.contains(&ToolCategory::Blocks));
        assert!(detected4.contains(&ToolCategory::MiniKit));
        let detected5 = IntentClassifier::detect("style the sidebar and button with tailwind");
        assert!(detected5.contains(&ToolCategory::Blocks));
    }

    #[test]
    fn test_intent_detection_natural_prompts() {
        let det_ast =
            IntentClassifier::detect("where is process_payment defined and who calls it?");
        assert!(det_ast.contains(&ToolCategory::Codegraph));
        assert!(det_ast.contains(&ToolCategory::Search));

        let det_dev = IntentClassifier::detect("start the dev server on port 3000");
        assert!(det_dev.contains(&ToolCategory::Dev));
        assert!(det_dev.contains(&ToolCategory::Exec));

        let det_res = IntentClassifier::detect("so how much resource its taking");
        assert!(det_res.contains(&ToolCategory::Dev));

        let det_stop = IntentClassifier::detect("ok stop that");
        assert!(det_stop.contains(&ToolCategory::Dev));

        let det_browser = IntentClassifier::detect("launch website and close browser");
        assert!(det_browser.contains(&ToolCategory::Web));

        let det_eng = IntentClassifier::detect("implement user authentication feature with tests");
        assert!(det_eng.contains(&ToolCategory::MiniPower));
        assert!(det_eng.contains(&ToolCategory::Memory));
    }

    #[test]
    fn test_detect_mcp_servers() {
        let servers = vec!["figma", "github", "postgres"];

        let matched = IntentClassifier::detect_mcp_servers("inspect figma document", &servers);
        assert!(matched.contains("figma"));
        assert!(!matched.contains("github"));

        let matched =
            IntentClassifier::detect_mcp_servers("create a new frame and component", &servers);
        assert!(matched.contains("figma"));

        let matched =
            IntentClassifier::detect_mcp_servers("open a pull request for this branch", &servers);
        assert!(matched.contains("github"));

        let matched =
            IntentClassifier::detect_mcp_servers("run this sql migration query", &servers);
        assert!(matched.contains("postgres"));

        let matched =
            IntentClassifier::detect_mcp_servers("fix compiler error in main.rs", &servers);
        assert!(matched.is_empty());
    }

    #[test]
    fn test_meta_introspection_queries_do_not_trigger_domain_categories() {
        for query in [
            "what are the features u have",
            "what are the tools u have",
            "so how many tools were active now",
            "so how many tools u have in total",
            "so for this project u make what are the skills,tools,features u used",
        ] {
            assert!(
                IntentClassifier::is_meta_introspection_query(query),
                "Expected '{}' to be classified as meta introspection",
                query
            );
            assert!(
                IntentClassifier::detect(query).is_empty(),
                "Expected '{}' to return empty tool categories",
                query
            );
        }
    }

    #[test]
    fn test_intent_detection_capped_at_max_auto_categories() {
        let noisy_prompt = "build a landing page with navbar hero pricing table git commit push search web url fetch minitask dev server";
        let detected = IntentClassifier::detect(noisy_prompt);
        assert!(
            detected.len() <= crate::constants::MAX_INTENT_AUTO_CATEGORIES,
            "Expected at most {} categories, got {}: {:?}",
            crate::constants::MAX_INTENT_AUTO_CATEGORIES,
            detected.len(),
            detected
        );
    }

    #[test]
    fn test_prompt_signal_vector_zero_energy_vs_actionable_and_plan_affinity() {
        let active_plan = Some("Implement OAuth2 JWT token validation in src/auth/jwt.rs");

        for conversational in ["hii", "good morning", "hey there", "thanks a lot", "yo"] {
            let sig = IntentClassifier::analyze_signal(conversational, active_plan);
            assert_eq!(
                sig.task_energy(),
                0,
                "Expected zero task energy for '{}'",
                conversational
            );
            assert!(!sig.has_actionable_task_intent());
            assert!(!sig.has_plan_continuation_affinity());
            assert!(!sig.has_code_exploration_intent());
        }

        let unrelated = IntentClassifier::analyze_signal(
            "explain how the rust borrow checker works",
            active_plan,
        );
        assert!(unrelated.has_actionable_task_intent());
        assert!(!unrelated.has_plan_continuation_affinity());

        let cont = IntentClassifier::analyze_signal("continue the todo plan", active_plan);
        assert!(cont.has_actionable_task_intent());
        assert!(cont.has_plan_continuation_affinity());

        let overlap =
            IntentClassifier::analyze_signal("implement the oauth2 token validation", active_plan);
        assert!(overlap.has_actionable_task_intent());
        assert!(overlap.has_plan_continuation_affinity());

        let code_sig =
            IntentClassifier::analyze_signal("fix bug in src/auth/jwt.rs validateToken", None);
        assert!(code_sig.code_morphology_score >= 2);
        assert!(code_sig.has_code_exploration_intent());
        assert!(code_sig.has_substantive_goal_intent());

        // Bare continuation cue ("continue") carries plan continuation intent ONLY, never substantive goal intent
        let bare_cont = IntentClassifier::analyze_signal("continue", active_plan);
        assert!(bare_cont.has_actionable_task_intent());
        assert!(!bare_cont.has_substantive_goal_intent());
    }

    #[test]
    fn test_multiline_minicode_landing_page_spec_is_not_meta_introspection() {
        let spec = r#"Create a premium, modern landing page for minicode — an ultra-fast, lightweight Pure Rust AI Coding Agent built for terminal-native and dual-mode human/AI workflows.

Core capabilities to highlight:
- Sub-10ms cold start and <15MB idle RAM footprint
- What tools, skills, and subagent swarm features you get out of the box
- Tree-sitter AST CodeGraph and 4-Gate Verification Barrier
"#;
        assert!(
            !IntentClassifier::is_meta_introspection_query(spec),
            "Multi-line product specification mentioning minicode/tools/features must NOT be misclassified as meta query"
        );
        let sig = IntentClassifier::analyze_signal(spec, None);
        assert!(sig.has_actionable_task_intent());
        assert!(sig.has_substantive_goal_intent());
        let detected = IntentClassifier::detect(spec);
        assert!(
            !detected.is_empty(),
            "Product spec must auto-activate relevant engineering/UI categories"
        );
    }
}

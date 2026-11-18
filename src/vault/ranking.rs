//! Purely algorithmic multi-field BM25F skill retrieval, set-theoretic stack & capability algebra,
//! glob specificity scoring, and calibrated knee-threshold candidate partitioning for the Orchestrator.
//!
//! Zero regexes, zero hardcoded stop-word lists, and zero hardcoded skill/category string branches:
//! - Term specificity emerges automatically from Robertson–Sparck Jones Smooth IDF over the live
//!   skill corpus (`Built-in ∪ Global ∪ Project`).
//! - Multi-word triggers match via contiguous stem-sequence verification in token space.
//! - Stack matching uses exact set inclusion ($S_{\text{id}}(d) \subseteq T_{\text{pkg}}$).
//! - Toolset gating uses set algebra ($\text{requires} \subseteq \mathcal{T}_{\text{avail}}$ and
//!   $\text{fallback} \cap \mathcal{T}_{\text{active}} = \emptyset$).

use crate::agent::orchestrator::RepoState;
use crate::vault::models::{SkillContextMode, SkillKind, VaultSkill};
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// BM25F saturation hyperparameter ($k_1$).
const BM25F_K1: f64 = 1.2;

/// Declarative BM25F field weights: `[Identifier, Triggers, Description, CategoryGlobs]`.
const FIELD_WEIGHTS: [f64; 4] = [3.8, 3.3, 1.15, 1.05];

/// Declarative BM25F per-field length normalization ($b_f$): `[Identifier, Triggers, Description, CategoryGlobs]`.
const FIELD_B: [f64; 4] = [0.15, 0.25, 0.65, 0.20];

const FIELD_ID: usize = 0;
const FIELD_TRIGGERS: usize = 1;
const FIELD_DESC: usize = 2;
const FIELD_CAT: usize = 3;
const NUM_FIELDS: usize = 4;

/// 64-bit FNV-1a hash for deterministic $O(1)$ stem comparisons without heap allocations.
#[inline]
#[must_use]
pub fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

#[inline]
#[must_use]
fn hash_bigram(a: u64, b: u64) -> u64 {
    let mut h = a ^ 0x9e37_79b9_7f4a_7c15;
    h = h.wrapping_mul(0x0100_0000_01b3) ^ b;
    h.wrapping_mul(0x0100_0000_01b3)
}

/// Tokenizes text into `(lowercase_word, stem_hash)` pairs using character predicates (zero regex).
#[must_use]
pub fn tokenize_with_stems(text: &str) -> Vec<(String, u64)> {
    text.split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .filter(|raw| raw.len() >= 2)
        .map(|raw| {
            let lower = raw.to_ascii_lowercase();
            let stem = VaultSkill::normalize_stem(&lower);
            let h = fnv1a64(stem.as_bytes());
            (lower, h)
        })
        .collect()
}

/// Returns `true` when an identifier segment represents a pure numeric or `v<digits>` version tag
/// (e.g. `"v4"`, `"19"`, `"2024"`).
#[inline]
#[must_use]
pub fn is_version_segment(seg: &str) -> bool {
    let bytes = seg.as_bytes();
    !bytes.is_empty()
        && (bytes.iter().all(u8::is_ascii_digit)
            || (bytes[0] == b'v' && bytes.len() > 1 && bytes[1..].iter().all(u8::is_ascii_digit)))
}

/// Extracts the non-version distinguishing identifier tokens $S_{\text{id}}(d)$ of a skill name.
#[must_use]
pub fn extract_skill_identifier_tokens(skill_name: &str) -> Vec<String> {
    skill_name
        .to_ascii_lowercase()
        .split(['-', '_'])
        .filter(|seg| seg.len() >= 2 && !is_version_segment(seg))
        .map(str::to_string)
        .collect()
}

/// Pure set-theoretic stack dependency match:
/// Returns `true` iff $S_{\text{id}}(d) \neq \emptyset$ and $S_{\text{id}}(d) \subseteq T_{\text{pkg}}$
/// (or the full canonical skill slug itself is an element of $T_{\text{pkg}}$).
///
/// Eliminates partial single-word false positives (e.g. `react-native` never matches plain `react`,
/// and `rust-tokio` only matches when both `rust` and `tokio` are present in project manifests).
#[must_use]
pub fn matches_stack_set_inclusion(skill: &VaultSkill, stack_tokens: &HashSet<String>) -> bool {
    if stack_tokens.is_empty() || skill.kind() == SkillKind::Core {
        return false;
    }
    let full_name = skill.name.to_ascii_lowercase();
    if stack_tokens.contains(&full_name) {
        return true;
    }
    let id_tokens = extract_skill_identifier_tokens(&full_name);
    !id_tokens.is_empty() && id_tokens.iter().all(|tok| stack_tokens.contains(tok))
}

/// Evaluates set-theoretic tool capability eligibility for a skill:
/// 1. Prerequisite inclusion: $\text{requires\_tools} \subseteq \mathcal{T}_{\text{available}}$
///    (falls back to `crate::tools::meta::is_registered` when no session `available_tools` override is attached)
/// 2. Fallback disjointness: $\text{fallback\_for\_tools} \cap \mathcal{T}_{\text{active}} = \emptyset$
#[must_use]
pub fn is_capability_eligible(
    skill: &VaultSkill,
    available_tools: Option<&HashSet<String>>,
    active_tools: Option<&HashSet<String>>,
) -> bool {
    if !skill.frontmatter.requires_tools.is_empty() {
        let prereqs_met = skill.frontmatter.requires_tools.iter().all(|req| {
            let clean = crate::tools::meta::canonical_name(req.trim());
            let lower = req.trim().to_ascii_lowercase();
            match available_tools {
                Some(avail) => avail.contains(clean) || avail.contains(&lower),
                None => crate::tools::meta::is_registered(clean),
            }
        });
        if !prereqs_met {
            return false;
        }
    }
    if !skill.frontmatter.fallback_for_tools.is_empty() {
        if let Some(active) = active_tools {
            let primary_active = skill.frontmatter.fallback_for_tools.iter().any(|prim| {
                let clean = crate::tools::meta::canonical_name(prim.trim());
                active.contains(clean) || active.contains(&prim.trim().to_ascii_lowercase())
            });
            if primary_active {
                return false;
            }
        }
    }
    true
}

/// Computes the structural specificity $g(p) \in [0.0, 1.0]$ of the most specific concrete glob
/// on `skill` that matches at least one path in `active_files`.
#[must_use]
pub fn compute_file_glob_specificity<P: AsRef<Path>>(
    skill: &VaultSkill,
    workspace_root: &Path,
    active_files: &[P],
) -> f64 {
    if active_files.is_empty() {
        return 0.0;
    }
    let mut peak_specificity = 0.0_f64;
    for raw_glob in &skill.frontmatter.globs {
        let g = raw_glob.trim();
        if g.is_empty() || g == "*" || g == "**/*" {
            continue;
        }
        let mut builder = ignore::overrides::OverrideBuilder::new(workspace_root);
        if builder.add(g).is_err() {
            continue;
        }
        let Ok(matcher) = builder.build() else {
            continue;
        };
        let hit = active_files.iter().any(|f| {
            let p = f.as_ref();
            let rel = p.strip_prefix(workspace_root).unwrap_or(p);
            matcher.matched(rel, false).is_whitelist()
        });
        if hit {
            let literal_chars = g
                .chars()
                .filter(|c| !matches!(c, '*' | '?' | '[' | ']' | '{' | '}'))
                .count();
            let depth_bonus = (g.matches('/').count().min(3) as f64) * 0.08;
            let spec = (0.42 + 0.50 * (literal_chars as f64 / g.len().max(1) as f64) + depth_bonus)
                .clamp(0.0, 1.0);
            if spec > peak_specificity {
                peak_specificity = spec;
            }
        }
    }
    peak_specificity
}

/// Evaluates whether `haystack_stems` contains `needle_stems` as an ordered contiguous sequence.
#[inline]
#[must_use]
fn contains_contiguous_stems(haystack: &[u64], needle: &[u64]) -> bool {
    !needle.is_empty()
        && haystack.len() >= needle.len()
        && haystack.windows(needle.len()).any(|w| w == needle)
}

#[derive(Debug, Clone)]
struct IndexedSkillDoc {
    field_tf: [HashMap<u64, u32>; NUM_FIELDS],
    field_len: [usize; NUM_FIELDS],
    /// Ordered stem sequences for multi-token triggers and compound skill names.
    phrase_triggers: Vec<Vec<u64>>,
    /// Single-token trigger `(raw_lower, stem_hash)` pairs for morphological prefix matching.
    single_triggers: Vec<(String, u64)>,
    /// Canonical identifier sequence of `skill.name`.
    id_stem_seq: Vec<u64>,
    /// Bigram hashes across identifier + triggers + description.
    bigrams: HashSet<u64>,
}

/// Multi-field BM25F corpus index over a live slice of `VaultSkill`s.
pub struct SkillCorpusIndex<'a> {
    skills: &'a [VaultSkill],
    docs: Vec<IndexedSkillDoc>,
    df: HashMap<u64, usize>,
    bigram_df: HashMap<u64, usize>,
    avg_field_len: [f64; NUM_FIELDS],
}

/// Rich relevance breakdown for Orchestrator candidate selection and telemetry.
#[derive(Debug, Clone)]
pub struct RankedSkillCandidate {
    pub skill: VaultSkill,
    /// Calibrated composite relevance score.
    pub total_score: f64,
    /// Combined query + objective BM25F component.
    pub lexical_score: f64,
    /// Direct user-prompt BM25F component (excluding background session objective).
    pub prompt_bm25: f64,
    /// Normalized confidence in `[0.0, 1.0]` relative to top candidate and saturation scale.
    pub confidence: f64,
    pub matched_stack: bool,
    pub glob_specificity: f64,
}

/// Partitioned turn skill recommendations separating immediate high-confidence inline rules
/// from the ranked Orchestrator Candidate Slate (`vault_show` / forked subagent execution).
#[derive(Debug, Clone, Default)]
pub struct OrchestratorSkillPartition {
    pub inline_skills: Vec<RankedSkillCandidate>,
    pub candidate_slate: Vec<RankedSkillCandidate>,
}

impl<'a> SkillCorpusIndex<'a> {
    /// Indexes `skills` in a single linear pass, computing per-field lengths and corpus document
    /// frequencies for Robertson–Sparck Jones Smooth IDF.
    #[must_use]
    pub fn build(skills: &'a [VaultSkill]) -> Self {
        let n = skills.len();
        let mut docs = Vec::with_capacity(n);
        let mut df: HashMap<u64, usize> = HashMap::new();
        let mut bigram_df: HashMap<u64, usize> = HashMap::new();
        let mut total_field_len = [0usize; NUM_FIELDS];

        for skill in skills {
            let mut field_tf: [HashMap<u64, u32>; NUM_FIELDS] = Default::default();
            let mut field_len = [0usize; NUM_FIELDS];
            let mut doc_unique_stems: HashSet<u64> = HashSet::new();
            let mut bigrams: HashSet<u64> = HashSet::new();
            let mut phrase_triggers: Vec<Vec<u64>> = Vec::new();
            let mut single_triggers: Vec<(String, u64)> = Vec::new();

            // Field 0: Identifier (`skill.name`)
            let id_tokens = tokenize_with_stems(&skill.name);
            let id_stem_seq: Vec<u64> = id_tokens.iter().map(|(_, h)| *h).collect();
            for (_, h) in &id_tokens {
                *field_tf[FIELD_ID].entry(*h).or_insert(0) += 1;
                field_len[FIELD_ID] += 1;
                doc_unique_stems.insert(*h);
            }
            if id_stem_seq.len() >= 2 {
                phrase_triggers.push(id_stem_seq.clone());
                for pair in id_stem_seq.windows(2) {
                    bigrams.insert(hash_bigram(pair[0], pair[1]));
                }
            } else if let Some((raw, h)) = id_tokens.first() {
                single_triggers.push((raw.clone(), *h));
            }

            // Field 1: Explicit frontmatter triggers
            for trig in &skill.frontmatter.triggers {
                let trig_tokens = tokenize_with_stems(trig);
                if trig_tokens.is_empty() {
                    continue;
                }
                for (_, h) in &trig_tokens {
                    *field_tf[FIELD_TRIGGERS].entry(*h).or_insert(0) += 1;
                    field_len[FIELD_TRIGGERS] += 1;
                    doc_unique_stems.insert(*h);
                }
                if trig_tokens.len() >= 2 {
                    let seq: Vec<u64> = trig_tokens.iter().map(|(_, h)| *h).collect();
                    for pair in seq.windows(2) {
                        bigrams.insert(hash_bigram(pair[0], pair[1]));
                    }
                    phrase_triggers.push(seq);
                } else {
                    single_triggers.push(trig_tokens[0].clone());
                }
            }

            // Field 2: Description
            let desc_tokens = tokenize_with_stems(&skill.description);
            for (_, h) in &desc_tokens {
                *field_tf[FIELD_DESC].entry(*h).or_insert(0) += 1;
                field_len[FIELD_DESC] += 1;
                doc_unique_stems.insert(*h);
            }
            for pair in desc_tokens.windows(2) {
                bigrams.insert(hash_bigram(pair[0].1, pair[1].1));
            }

            // Field 3: Category, concrete glob tokens, and structural Markdown headings (`# ...`)
            for (_, h) in tokenize_with_stems(skill.category()) {
                *field_tf[FIELD_CAT].entry(h).or_insert(0) += 1;
                field_len[FIELD_CAT] += 1;
                doc_unique_stems.insert(h);
            }
            for glob in &skill.frontmatter.globs {
                for (_, h) in tokenize_with_stems(glob) {
                    *field_tf[FIELD_CAT].entry(h).or_insert(0) += 1;
                    field_len[FIELD_CAT] += 1;
                    doc_unique_stems.insert(h);
                }
            }
            for line in skill.instructions.lines() {
                let trimmed = line.trim_start();
                if trimmed.starts_with("# ") {
                    for (_, h) in tokenize_with_stems(trimmed) {
                        *field_tf[FIELD_CAT].entry(h).or_insert(0) += 1;
                        field_len[FIELD_CAT] += 1;
                        doc_unique_stems.insert(h);
                    }
                    break;
                }
            }

            for f in 0..NUM_FIELDS {
                total_field_len[f] += field_len[f];
            }
            for h in doc_unique_stems {
                *df.entry(h).or_insert(0) += 1;
            }
            for bg in &bigrams {
                *bigram_df.entry(*bg).or_insert(0) += 1;
            }

            docs.push(IndexedSkillDoc {
                field_tf,
                field_len,
                phrase_triggers,
                single_triggers,
                id_stem_seq,
                bigrams,
            });
        }

        let denom = n.max(1) as f64;
        let mut avg_field_len = [1.0; NUM_FIELDS];
        for f in 0..NUM_FIELDS {
            avg_field_len[f] = (total_field_len[f] as f64 / denom).max(1.0);
        }

        Self {
            skills,
            docs,
            df,
            bigram_df,
            avg_field_len,
        }
    }

    /// Robertson–Sparck Jones Smooth IDF with Empirical Bayes prior virtual corpus floor ($N_0 = 16$)
    /// so small test corpora and large multi-tier catalogs share a stable IDF scale.
    #[inline]
    #[must_use]
    fn smooth_idf(&self, doc_freq: usize) -> f64 {
        let n = self.skills.len().max(16) as f64;
        let df = doc_freq as f64;
        (1.0 + ((n - df + 0.5).max(0.1) / (df + 0.5))).ln()
    }

    /// Computes calibrated BM25F + phrase-coherence lexical score for document `doc_idx` against `query`.
    /// Enforces the Universal Coordination Quorum: lexical score is non-zero iff at least one
    /// identifier/trigger anchor matched OR at least two discriminative corpus stems matched.
    #[must_use]
    pub fn score_query_bm25f(&self, doc_idx: usize, query: &str) -> f64 {
        let q_tokens = tokenize_with_stems(query);
        if q_tokens.is_empty() || doc_idx >= self.docs.len() {
            return 0.0;
        }
        let doc = &self.docs[doc_idx];
        let skill = &self.skills[doc_idx];

        let q_stem_seq: Vec<u64> = q_tokens.iter().map(|(_, h)| *h).collect();
        let mut unique_q_stems: Vec<u64> = Vec::with_capacity(q_stem_seq.len());
        for &h in &q_stem_seq {
            if !unique_q_stems.contains(&h) {
                unique_q_stems.push(h);
            }
        }

        let max_anchor_df = ((self.skills.len().max(10) as f64) * 0.35).ceil().max(2.0) as usize;
        // Prose-only terms (description / H1 title without identifier or trigger match) require high
        // corpus specificity (df <= 12% of catalog) so generic words like "default", "update", "number",
        // or "optional" across 20KB third-party skill manuals never manufacture false positives.
        let max_prose_df = ((self.skills.len().max(16) as f64) * 0.12).ceil().max(2.0) as usize;

        let mut bm25f_sum = 0.0_f64;
        let mut anchor_hits = 0usize;
        let mut discriminative_prose_hits = 0usize;

        // 1. Exact full skill-name or contiguous identifier stem sequence match
        let name_lower = skill.name.to_ascii_lowercase();
        let query_lower = query.to_ascii_lowercase();
        if (!doc.id_stem_seq.is_empty() && contains_contiguous_stems(&q_stem_seq, &doc.id_stem_seq))
            || (name_lower.contains('-') && query_lower.contains(&name_lower))
        {
            anchor_hits += 2;
            bm25f_sum += self.smooth_idf(1) * 2.4;
        }

        // 2. Multi-token contiguous phrase triggers (ordered stem sequence verification)
        for phrase in &doc.phrase_triggers {
            if contains_contiguous_stems(&q_stem_seq, phrase) {
                anchor_hits += 1;
                bm25f_sum += self.smooth_idf(1) * 1.85;
            }
        }

        // 3. Single-token triggers with morphological prefix sharing (e.g. `auth` <-> `authentication`)
        for (trig_raw, trig_hash) in &doc.single_triggers {
            let exact_stem = unique_q_stems.contains(trig_hash);
            let morph_prefix = !exact_stem
                && trig_raw.len() >= 4
                && q_tokens.iter().any(|(qw, _)| {
                    qw.len() >= trig_raw.len()
                        && qw.len() <= trig_raw.len() + 12
                        && qw.starts_with(trig_raw.as_str())
                });
            if exact_stem || morph_prefix {
                anchor_hits += 1;
                if morph_prefix {
                    let df = self.df.get(trig_hash).copied().unwrap_or(1);
                    bm25f_sum += self.smooth_idf(df) * 1.35;
                }
            }
        }

        // 4. Standard Robertson BM25F across all 4 weighted fields
        for &qh in &unique_q_stems {
            let mut pseudo_tf = 0.0_f64;
            let mut matched_in_anchor = false;
            let mut matched_in_prose = false;

            for f in 0..NUM_FIELDS {
                if let Some(&tf_raw) = doc.field_tf[f].get(&qh) {
                    let dl = doc.field_len[f] as f64;
                    let avg_dl = self.avg_field_len[f];
                    let norm = 1.0 - FIELD_B[f] + FIELD_B[f] * (dl / avg_dl);
                    pseudo_tf += FIELD_WEIGHTS[f] * (f64::from(tf_raw) / norm.max(0.15));
                    if f <= FIELD_TRIGGERS {
                        matched_in_anchor = true;
                    } else {
                        matched_in_prose = true;
                    }
                }
            }

            if pseudo_tf > 0.0 {
                let df = self.df.get(&qh).copied().unwrap_or(1);
                let idf = self.smooth_idf(df);
                if matched_in_anchor && df <= max_anchor_df {
                    anchor_hits += 1;
                } else if matched_in_prose && df <= max_prose_df {
                    discriminative_prose_hits += 1;
                }
                bm25f_sum += idf * (pseudo_tf / (BM25F_K1 + pseudo_tf));
            }
        }

        // 5. Adjacent stem bigram coherence bonus
        for pair in q_stem_seq.windows(2) {
            let bg = hash_bigram(pair[0], pair[1]);
            if doc.bigrams.contains(&bg) {
                let bg_df = self.bigram_df.get(&bg).copied().unwrap_or(1);
                bm25f_sum += 0.55 * self.smooth_idf(bg_df);
            }
        }

        // Universal Coordination Quorum: suppresses incidental description overlap
        if anchor_hits == 0 {
            if discriminative_prose_hits < 2 {
                return 0.0;
            }
            // Pivoted sublinear query-length normalization for prose-only matches: prevents 50-word
            // coding prompts from accumulating accidental boilerplate score on large 20KB skill manuals.
            let q_len_norm = ((unique_q_stems.len() as f64) / 5.0).max(1.0).sqrt();
            bm25f_sum /= q_len_norm;
        }

        bm25f_sum
    }

    /// Searches and ranks skills by BM25F relevance for `vault_search` (supporting natural-language
    /// multi-term queries as well as exact substring/glob lookups).
    #[must_use]
    pub fn search_ranked(&self, query: &str, category_filter: Option<&str>) -> Vec<VaultSkill> {
        let cat_clean = category_filter.map(str::trim).filter(|c| !c.is_empty());
        let q_trim = query.trim();

        if q_trim.is_empty() {
            return self
                .skills
                .iter()
                .filter(|s| cat_clean.is_none_or(|c| s.category().eq_ignore_ascii_case(c)))
                .cloned()
                .collect();
        }

        let q_lower = q_trim.to_ascii_lowercase();
        let mut scored: Vec<(f64, VaultSkill)> = self
            .skills
            .iter()
            .enumerate()
            .filter(|(_, s)| cat_clean.is_none_or(|c| s.category().eq_ignore_ascii_case(c)))
            .filter_map(|(idx, s)| {
                let mut score = self.score_query_bm25f(idx, q_trim);
                if s.name.to_ascii_lowercase() == q_lower {
                    score += 25.0;
                } else if s.name.to_ascii_lowercase().contains(&q_lower) {
                    score += 8.0;
                } else if s
                    .frontmatter
                    .globs
                    .iter()
                    .any(|g| g.to_ascii_lowercase().contains(&q_lower))
                {
                    score += 5.0;
                } else if score == 0.0 && s.description.to_ascii_lowercase().contains(&q_lower) {
                    score += 3.0;
                }
                (score > 0.0).then(|| (score, s.clone()))
            })
            .collect();

        scored.sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| a.1.name.cmp(&b.1.name)));
        scored.into_iter().map(|(_, s)| s).collect()
    }

    /// Computes multi-signal relevance across all skills given workspace dependencies, repo state,
    /// user prompt, active objective, active files, toolset capabilities, and empirical outcome telemetry.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn rank_all<P: AsRef<Path>>(
        &self,
        workspace_root: &Path,
        stack_tokens: &HashSet<String>,
        repo_state: Option<RepoState>,
        prompt: Option<&str>,
        session_objective: Option<&str>,
        active_files: &[P],
        available_tools: Option<&HashSet<String>>,
        active_tools: Option<&HashSet<String>>,
        telemetry: Option<&HashMap<String, crate::vault::models::SkillOutcomeStats>>,
    ) -> Vec<RankedSkillCandidate> {
        let mut ranked = Vec::new();
        let now_unix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        for (idx, skill) in self.skills.iter().enumerate() {
            if !is_capability_eligible(skill, available_tools, active_tools) {
                continue;
            }

            let prompt_bm25 = prompt.map_or(0.0, |p| self.score_query_bm25f(idx, p));
            let obj_discount = if prompt_bm25 > 0.0 { 0.55 } else { 0.35 };
            let obj_bm25 =
                session_objective.map_or(0.0, |o| self.score_query_bm25f(idx, o) * obj_discount);
            let lexical_score = prompt_bm25.max(obj_bm25) + 0.25 * prompt_bm25.min(obj_bm25);

            // Repo-state declarative compatibility factor
            let repo_factor = repo_state_compatibility(skill, repo_state);
            if repo_factor == 0.0 && prompt_bm25 < 6.0 && !skill.is_active_in_project {
                continue;
            }

            let matched_stack = matches_stack_set_inclusion(skill, stack_tokens);
            let glob_specificity =
                compute_file_glob_specificity(skill, workspace_root, active_files);

            // Passive auto-generated package reference stubs only surface passively when explicitly
            // loaded or lexically relevant to the prompt/objective.
            let is_passive_pkg_stub = skill.kind() == SkillKind::Reference
                && skill
                    .path
                    .as_ref()
                    .is_some_and(|p| p.components().any(|c| c.as_os_str() == "packages"));
            if is_passive_pkg_stub && lexical_score < 2.5 && !skill.is_active_in_project {
                continue;
            }

            let project_lexical_synergy =
                if skill.scope == crate::vault::models::SkillScope::Project && lexical_score >= 2.0
                {
                    9.5
                } else {
                    0.0
                };
            let pin_prior = match (skill.is_active_in_project, skill.frontmatter.always_apply) {
                (true, true) => 8.5,
                (true, false) => 6.0 + project_lexical_synergy,
                (false, true) => 5.0,
                (false, false) => 0.0,
            };
            let stack_signal = if matched_stack { 4.5 } else { 0.0 };
            let glob_signal = glob_specificity * 4.0;
            // Conjunctive workspace grounding (stack dependency ∧ active edited file match)
            let conjunctive_grounding = if matched_stack && glob_specificity > 0.0 {
                4.8
            } else {
                0.0
            };
            let state_bonus = if repo_factor > 1.0 { 2.2 } else { 0.0 };
            let lower_key = skill.name.to_ascii_lowercase();
            let is_auto_learned =
                lower_key.starts_with("learned-") && lower_key.ends_with("-invariants");
            let empirical_bonus = telemetry
                .and_then(|m| m.get(&lower_key))
                .map_or(0.0, |stats| {
                    stats.empirical_utility_bonus(is_auto_learned, now_unix)
                });

            let raw_total = (lexical_score * 1.65
                + pin_prior
                + stack_signal
                + glob_signal
                + conjunctive_grounding
                + state_bonus
                + empirical_bonus)
                .max(0.0);
            if raw_total <= 0.0 {
                continue;
            }

            // Logistic saturation into [0.0, 1.0] confidence
            let confidence = raw_total / (raw_total + 6.0);
            ranked.push(RankedSkillCandidate {
                skill: skill.clone(),
                total_score: raw_total,
                lexical_score,
                prompt_bm25,
                confidence,
                matched_stack,
                glob_specificity,
            });
        }

        ranked.sort_by(|a, b| {
            b.total_score
                .total_cmp(&a.total_score)
                .then_with(|| a.skill.name.cmp(&b.skill.name))
        });
        ranked.dedup_by(|a, b| a.skill.name.eq_ignore_ascii_case(&b.skill.name));
        ranked
    }

    /// Partitions turn-relevant skills using the Candidate-Slate-First + Single-Pin discipline:
    /// 1. `inline_skills`: At most **1** decisive, unseen skill (`inline_skills.is_empty()`), preventing
    ///    multi-KB pre-turn stuffing and eliminating cross-turn duplication once recorded in `session_active_skills`.
    /// 2. `candidate_slate`: Up to 6 BM25F-ranked candidate skills surfaced as a compact Orchestrator menu
    ///    so the LLM pulls only what it needs via `vault_show` or isolates via `delegate_task`.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn partition_turn_skills<P: AsRef<Path>>(
        &self,
        workspace_root: &Path,
        stack_tokens: &HashSet<String>,
        repo_state: Option<RepoState>,
        prompt: Option<&str>,
        session_objective: Option<&str>,
        active_files: &[P],
        exclude_baseline: &HashSet<String>,
        available_tools: Option<&HashSet<String>>,
        active_tools: Option<&HashSet<String>>,
        telemetry: Option<&HashMap<String, crate::vault::models::SkillOutcomeStats>>,
        max_inline_skill_chars: usize,
        total_inline_budget_chars: usize,
    ) -> OrchestratorSkillPartition {
        let ranked = self.rank_all(
            workspace_root,
            stack_tokens,
            repo_state,
            prompt,
            session_objective,
            active_files,
            available_tools,
            active_tools,
            telemetry,
        );

        let eligible: Vec<RankedSkillCandidate> = ranked
            .into_iter()
            .filter(|cand| {
                if cand.skill.kind() == SkillKind::Core {
                    return false;
                }
                let lower_name = cand.skill.name.to_ascii_lowercase();
                if exclude_baseline.contains(&lower_name) {
                    return false;
                }
                let strong_lexical = cand.lexical_score >= 2.6;
                if cand.skill.frontmatter.always_apply {
                    return strong_lexical;
                }
                strong_lexical || cand.matched_stack || cand.glob_specificity > 0.0
            })
            .collect();

        if eligible.is_empty() {
            return OrchestratorSkillPartition::default();
        }

        let is_inlinable = |c: &RankedSkillCandidate| -> bool {
            c.skill.frontmatter.context_mode() != SkillContextMode::Fork
                && (c.skill.is_active_in_project
                    || (c.confidence >= 0.68
                        && c.lexical_score >= 3.2
                        && c.skill.instructions.trim().len()
                            <= max_inline_skill_chars.saturating_mul(3)))
        };

        let mut inline_skills = Vec::new();
        let mut candidate_slate = Vec::new();
        let mut used_chars = 0usize;

        for cand in eligible {
            let body_len = cand.skill.instructions.trim().len();
            let fits_per_skill = is_inlinable(&cand);
            let remaining_budget = total_inline_budget_chars.saturating_sub(used_chars);
            let effective_slice_len = body_len.min(max_inline_skill_chars).min(remaining_budget);

            // Single-Pin Rule: auto-inline at most 1 decisive skill per turn; all other candidates
            // go to the compact Orchestrator Candidate Slate for LLM-driven `vault_show` selection.
            let can_inline = fits_per_skill
                && inline_skills.is_empty()
                && cand.total_score >= 5.0
                && remaining_budget >= 1_200;

            if can_inline {
                used_chars = used_chars.saturating_add(effective_slice_len);
                inline_skills.push(cand);
            } else if candidate_slate.len() < 6 && cand.total_score >= 2.6 {
                candidate_slate.push(cand);
            }
        }

        OrchestratorSkillPartition {
            inline_skills,
            candidate_slate,
        }
    }
}

fn repo_state_compatibility(skill: &VaultSkill, repo_state: Option<RepoState>) -> f64 {
    if skill.frontmatter.when_repo_state.is_empty() {
        return 1.0;
    }
    let Some(rs) = repo_state else {
        return 1.0;
    };
    let state_slug = match rs {
        RepoState::FreshWorkspace => "fresh",
        RepoState::DocsOnly => "docs_only",
        RepoState::ExistingCodebase => "existing",
    };
    if skill
        .frontmatter
        .when_repo_state
        .iter()
        .any(|w| w.trim().eq_ignore_ascii_case(state_slug))
    {
        1.35
    } else {
        0.0
    }
}

/// Standalone single-skill BM25F scorer (used by `VaultSkill::prompt_match_score` when called outside
/// a pre-built `SkillCorpusIndex`), scaled to integer points (`0..=300+`).
#[must_use]
pub fn score_single_skill_bm25f(skill: &VaultSkill, prompt: &str) -> usize {
    let slice = std::slice::from_ref(skill);
    let idx = SkillCorpusIndex::build(slice);
    let raw = idx.score_query_bm25f(0, prompt);
    (raw * 18.0).round() as usize
}

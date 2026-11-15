//! Pure-Rust Information-Theoretic & Automaton Lexical Primitives.
//!
//! Eliminates ad-hoc `if / else if` score staircases, hardcoded file-extension whitelists, and
//! $O(N \times C)$ substring scans in favor of:
//! 1. **`AhoCorasickWordMatcher`**: Linear-time $O(N)$ multi-pattern byte-trie automaton with BFS
//!    suffix failure links and identifier word-boundary verification.
//! 2. **`Bm25fScorer` + `sorensen_dice_bigrams`**: Multi-field Robertson–Zaragoza BM25F with
//!    character 2-gram Sørensen–Dice morphological similarity.
//! 3. **`reciprocal_rank_fusion`**: Parameter-free Cormack–Clarke–Buettcher RRF rank aggregation.
//! 4. **`sniff_utf8_text_bytes`**: Content-based binary vs UTF-8 text classifier (NUL-byte + UTF-8
//!    boundary validation + control-byte density).

use std::collections::{HashMap, VecDeque};

#[inline]
fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

#[derive(Clone, Default)]
struct AcNode {
    /// Sorted `(ascii_lower_byte, next_state)` transitions for cache-compact sparse branching.
    transitions: Vec<(u8, usize)>,
    fail: usize,
    outputs: Vec<usize>,
}

impl AcNode {
    #[inline]
    fn next_state(&self, byte: u8) -> Option<usize> {
        self.transitions
            .binary_search_by_key(&byte, |&(b, _)| b)
            .ok()
            .map(|idx| self.transitions[idx].1)
    }

    fn get_or_insert(&mut self, byte: u8, new_idx: usize) -> (usize, bool) {
        match self.transitions.binary_search_by_key(&byte, |&(b, _)| b) {
            Ok(pos) => (self.transitions[pos].1, false),
            Err(pos) => {
                self.transitions.insert(pos, (byte, new_idx));
                (new_idx, true)
            }
        }
    }
}

/// Pure-Rust case-insensitive Aho–Corasick multi-pattern automaton with word-boundary verification.
///
/// Scans an input buffer of length $N$ in a single $O(N)$ linear pass regardless of pattern count,
/// reporting pattern IDs only when bounded by non-identifier bytes (`!is_word_byte`).
pub struct AhoCorasickWordMatcher {
    nodes: Vec<AcNode>,
    pattern_lens: Vec<usize>,
}

impl AhoCorasickWordMatcher {
    /// Builds the trie and BFS failure/output links for `patterns` (`(pattern_id, text)`).
    pub fn build<'a>(patterns: impl IntoIterator<Item = (usize, &'a str)>) -> Self {
        let mut match_slots: Vec<(usize, usize)> = Vec::new();
        Self::build_slots(patterns, vec![AcNode::default()], &mut match_slots)
    }

    fn build_slots<'a>(
        patterns: impl IntoIterator<Item = (usize, &'a str)>,
        mut nodes: Vec<AcNode>,
        match_slots: &mut Vec<(usize, usize)>,
    ) -> Self {
        for (caller_id, raw_pat) in patterns {
            let trimmed = raw_pat.trim();
            // Require >= 3 bytes (supports 3-letter technical concepts like OKF, QJL, IVF, AST, WAL)
            if trimmed.len() < 3 {
                continue;
            }
            let slot_id = match_slots.len();
            match_slots.push((caller_id, trimmed.len()));

            let mut state = 0usize;
            for &b in trimmed.as_bytes() {
                let lower = b.to_ascii_lowercase();
                let candidate_next = nodes.len();
                let (next_s, inserted) = nodes[state].get_or_insert(lower, candidate_next);
                if inserted {
                    nodes.push(AcNode::default());
                }
                state = next_s;
            }
            nodes[state].outputs.push(slot_id);
        }

        // BFS construction of Aho–Corasick suffix failure links and dictionary suffix outputs
        let mut queue = VecDeque::new();
        let root_edges = nodes[0].transitions.clone();
        for (_, child) in root_edges {
            nodes[child].fail = 0;
            queue.push_back(child);
        }

        while let Some(r) = queue.pop_front() {
            let edges = nodes[r].transitions.clone();
            for (byte, u) in edges {
                queue.push_back(u);
                let mut f = nodes[r].fail;
                while f != 0 && nodes[f].next_state(byte).is_none() {
                    f = nodes[f].fail;
                }
                let fail_target = nodes[f].next_state(byte).unwrap_or(0);
                nodes[u].fail = if fail_target == u { 0 } else { fail_target };
                if !nodes[nodes[u].fail].outputs.is_empty() {
                    let inherited = nodes[nodes[u].fail].outputs.clone();
                    nodes[u].outputs.extend(inherited);
                }
            }
        }

        // Encode `(caller_id, len)` into parallel vectors
        let mut encoded_lens = Vec::with_capacity(match_slots.len() * 2);
        for &(cid, len) in match_slots.iter() {
            encoded_lens.push(cid);
            encoded_lens.push(len);
        }

        Self {
            nodes,
            pattern_lens: encoded_lens,
        }
    }

    /// Scans `text` in a single $O(N)$ pass and returns deduplicated caller pattern IDs in sorted order
    /// whose matched span satisfies strict word boundaries.
    pub fn find_word_bounded_ids(&self, text: &str) -> Vec<usize> {
        if self.nodes.len() <= 1 || text.is_empty() {
            return Vec::new();
        }
        let bytes = text.as_bytes();
        let mut state = 0usize;
        let mut matched_ids = Vec::new();

        for (i, &b) in bytes.iter().enumerate() {
            let lower = b.to_ascii_lowercase();
            while state != 0 && self.nodes[state].next_state(lower).is_none() {
                state = self.nodes[state].fail;
            }
            state = self.nodes[state].next_state(lower).unwrap_or(0);

            if !self.nodes[state].outputs.is_empty() {
                let right_ok = bytes.get(i + 1).is_none_or(|&nb| !is_word_byte(nb));
                if right_ok {
                    for &slot_id in &self.nodes[state].outputs {
                        let caller_id = self.pattern_lens[slot_id * 2];
                        let pat_len = self.pattern_lens[slot_id * 2 + 1];
                        if i + 1 >= pat_len {
                            let start = i + 1 - pat_len;
                            let left_ok = start == 0 || !is_word_byte(bytes[start - 1]);
                            if left_ok {
                                matched_ids.push(caller_id);
                            }
                        }
                    }
                }
            }
        }

        matched_ids.sort_unstable();
        matched_ids.dedup();
        matched_ids
    }
}

/// Tokenizes text into normalized lowercase alphanumeric/underscore terms (`len >= 2`).
pub fn tokenize_terms(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|s| s.len() >= 2)
        .map(str::to_lowercase)
        .collect()
}

/// Deterministic morphological suffix normalizer for code/documentation search intents,
/// replacing hardcoded plural/conjugation word arrays.
pub fn normalize_morph_stem(token: &str) -> &str {
    const SUFFIXES: &[&str] = &[
        "ations", "ation", "ments", "ment", "ings", "ing", "ers", "ed", "es", "s",
    ];
    let lower = token.trim();
    for &suf in SUFFIXES {
        if lower.len() >= suf.len() + 3 && lower.ends_with(suf) {
            return &lower[..lower.len() - suf.len()];
        }
    }
    lower
}

/// Computes the Sørensen–Dice similarity coefficient $\frac{2 |X \cap Y|}{|X| + |Y|} \in [0.0, 1.0]$
/// over sorted ASCII-lowercased character bigrams in $O(|a| + |b|)$ merge time (zero HashMaps).
pub fn sorensen_dice_bigrams(a: &str, b: &str) -> f32 {
    fn sorted_bigrams(s: &str) -> Vec<u16> {
        let filtered: Vec<u8> = s
            .bytes()
            .filter_map(|ch| {
                let l = ch.to_ascii_lowercase();
                (l.is_ascii_alphanumeric() || l == b' ').then_some(l)
            })
            .collect();
        if filtered.len() < 2 {
            return Vec::new();
        }
        let mut grams: Vec<u16> = filtered
            .windows(2)
            .filter(|w| !(w[0] == b' ' && w[1] == b' '))
            .map(|w| u16::from_le_bytes([w[0], w[1]]))
            .collect();
        grams.sort_unstable();
        grams
    }

    let ga = sorted_bigrams(a);
    let gb = sorted_bigrams(b);
    if ga.is_empty() || gb.is_empty() {
        return 0.0;
    }

    let (mut i, mut j, mut inter) = (0usize, 0usize, 0usize);
    while i < ga.len() && j < gb.len() {
        match ga[i].cmp(&gb[j]) {
            std::cmp::Ordering::Equal => {
                inter += 1;
                i += 1;
                j += 1;
            }
            std::cmp::Ordering::Less => i += 1,
            std::cmp::Ordering::Greater => j += 1,
        }
    }

    (2.0 * inter as f32) / ((ga.len() + gb.len()) as f32)
}

/// Multi-field BM25F document representation (`fields` weighted by structural importance, e.g.
/// `(title, 3.0)`, `(breadcrumb, 1.8)`, `(body_or_summary, 1.0)`).
pub struct Bm25FieldDoc<'a> {
    pub fields: &'a [(&'a str, f32)],
}

/// Computes continuous BM25F scores across a slice of multi-field candidate documents for `query`.
///
/// Uses Robertson–Zaragoza field-weighted term saturation ($k_1 = 1.2$, $b = 0.75$) so rare query
/// terms and concise heading titles rank naturally without arbitrary `if / else if` score tiers.
pub fn score_bm25f_batch(query: &str, docs: &[Bm25FieldDoc<'_>]) -> Vec<f32> {
    let n_docs = docs.len();
    if n_docs == 0 {
        return Vec::new();
    }

    let q_terms: Vec<String> = tokenize_terms(query);
    let q_stems: Vec<String> = q_terms
        .iter()
        .map(|t| normalize_morph_stem(t).to_string())
        .collect();
    if q_stems.is_empty() {
        return vec![0.0; n_docs];
    }

    // Pre-tokenize each document's weighted pseudo-TF per stem and weighted length
    let mut doc_tf_maps: Vec<(f32, HashMap<String, f32>)> = Vec::with_capacity(n_docs);
    let mut df_counts: HashMap<String, usize> = HashMap::new();
    let mut total_weighted_len = 0.0f32;

    for doc in docs {
        let mut tf_map: HashMap<String, f32> = HashMap::new();
        let mut doc_len = 0.0f32;
        for &(text, weight) in doc.fields {
            let toks = tokenize_terms(text);
            doc_len += (toks.len() as f32) * weight;
            for tok in toks {
                let stem = normalize_morph_stem(&tok).to_string();
                *tf_map.entry(stem).or_insert(0.0) += weight;
            }
        }
        for stem in tf_map.keys() {
            *df_counts.entry(stem.clone()).or_insert(0) += 1;
        }
        total_weighted_len += doc_len.max(1.0);
        doc_tf_maps.push((doc_len.max(1.0), tf_map));
    }

    let avg_dl = (total_weighted_len / (n_docs as f32)).max(1.0);
    let k1 = 1.2f32;
    let b = 0.75f32;

    doc_tf_maps
        .iter()
        .map(|(dl, tf_map)| {
            let norm_denom = k1 * (1.0 - b + b * (*dl / avg_dl));
            let mut score = 0.0f32;
            for stem in &q_stems {
                if let Some(&wtf) = tf_map.get(stem) {
                    let df = *df_counts.get(stem).unwrap_or(&1) as f32;
                    let idf = (1.0 + ((n_docs as f32) - df + 0.5) / (df + 0.5)).ln();
                    score += idf * ((wtf * (k1 + 1.0)) / (wtf + norm_denom));
                }
            }
            score
        })
        .collect()
}

/// Cormack–Clarke–Buettcher Reciprocal Rank Fusion: $\text{RRF}(d) = \sum_{r} \frac{w_r}{k + \text{rank}_r(d)}$.
#[inline]
pub fn reciprocal_rank_fusion(ranks: &[(usize, f32)], k_smooth: f32) -> f32 {
    ranks
        .iter()
        .map(|&(zero_based_rank, weight)| weight / (k_smooth + (zero_based_rank as f32) + 1.0))
        .sum()
}

/// Content-based UTF-8 text vs binary classifier (zero file-extension whitelist).
///
/// Inspects up to the first 8 KiB:
/// 1. Any `0x00` NUL byte immediately classifies the payload as binary.
/// 2. Validates UTF-8 (gracefully permitting an incomplete multi-byte codepoint truncated right at the 8 KiB window edge).
/// 3. Rejects payloads where non-whitespace control bytes exceed 2% of inspected bytes.
pub fn sniff_utf8_text_bytes(bytes: &[u8]) -> bool {
    if bytes.is_empty() {
        return false;
    }
    let sample = &bytes[..bytes.len().min(8192)];
    if sample.contains(&0u8) {
        return false;
    }
    let valid_utf8 = match std::str::from_utf8(sample) {
        Ok(_) => true,
        Err(err) => err.error_len().is_none() && sample.len() - err.valid_up_to() < 4,
    };
    if !valid_utf8 {
        return false;
    }
    let ctrl_count = sample
        .iter()
        .filter(|&&b| b < 0x20 && !matches!(b, b'\t' | b'\n' | b'\r' | 0x0C))
        .count();
    ctrl_count * 50 <= sample.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_aho_corasick_word_matcher_handles_3_letter_acronyms_and_rejects_substrings() {
        let patterns = vec![
            (0usize, "auth"),
            (1usize, "OKF"),
            (2usize, "QJL"),
            (3usize, "TUI Frame Scheduler"),
        ];
        let matcher = AhoCorasickWordMatcher::build(patterns);

        // "author" and "oauth" must NOT trigger pattern 0 ("auth"), while "OKF", "qjl", and phrase match!
        let hits = matcher.find_word_bounded_ids(
            "The author combined OKF provenance with qjl residuals and the TUI frame scheduler in oauth.",
        );
        assert_eq!(hits, vec![1, 2, 3]);

        let exact_auth = matcher.find_word_bounded_ids("Updated auth middleware.");
        assert_eq!(exact_auth, vec![0]);
    }

    #[test]
    fn test_bm25f_and_sorensen_dice_rank_morphological_variants_continuously() {
        let docs = vec![
            Bm25FieldDoc {
                fields: &[
                    ("Binary Screening", 3.0),
                    ("Search > Binary Screening", 1.5),
                    ("Popcount stage 1 filter", 1.0),
                ],
            },
            Bm25FieldDoc {
                fields: &[
                    ("FastScan LUT", 3.0),
                    ("Search > FastScan LUT", 1.5),
                    ("L1 lookup table", 1.0),
                ],
            },
        ];
        // Plural "Binary Screenings" still ranks doc 0 cleanly above doc 1 with zero if/else ladders!
        let scores = score_bm25f_batch("Binary Screenings", &docs);
        assert!(scores[0] > scores[1] && scores[0] > 0.5);
        assert!(sorensen_dice_bigrams("Binary Screenings", "Binary Screening") > 0.85);
    }

    #[test]
    fn test_sniff_utf8_text_bytes_without_extension_whitelist() {
        assert!(sniff_utf8_text_bytes(
            b"SELECT * FROM events WHERE id = 1;\n"
        ));
        assert!(sniff_utf8_text_bytes("🚀 Pure Rust UTF-8 RFC\n".as_bytes()));
        assert!(!sniff_utf8_text_bytes(
            b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR"
        ));
        assert!(!sniff_utf8_text_bytes(b"\x00asm\x01\x00\x00\x00"));
    }
}

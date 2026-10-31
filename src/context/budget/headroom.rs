//! Native Rust implementation of the `headroomlabs-ai/headroom` (`headroom-core`)
//! context compression and KV-cache alignment subsystems:
//! 1. `AdaptiveSizer`: 64-bit 4-gram SimHash deduplication + Kneedle unique-bigram saturation knee detection.
//! 2. `AnchorSelector`: Pattern-aware front/back anchor allocation + middle-region information density + numeric outlier preservation.
//! 3. `DiffCompressor`: Unified `git diff` parser, file/hunk ranker, and context-line trimmer with CCR recovery.
//! 4. `SearchCompressor`: Grep/ripgrep (`file:line:content`) parser, `LineImportanceDetector` scorer, and adaptive per-file match compressor.
//! 5. `CodeCompressor`: Tree-sitter AST-preserving function-body folder with CCR recovery.
//! 6. `TagProtector`: Single-pass O(N) custom XML workflow tag shield (`protect_tags` / `restore_tags`).
//! 7. `CacheAligner`: Deterministic tool-definition normalizer (Invariant I7) & token-aware validation (Invariant I5).

use crate::agent::provider::ToolSchema;
use crate::context::budget::ccr_cache::{sha256_digest, CcrCache};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

// ============================================================================
// 1. AdaptiveSizer — Kneedle Information Saturation & 4-Gram SimHash
// ============================================================================

pub struct AdaptiveSizer;

impl AdaptiveSizer {
    /// Computes the optimal number of items `K` to retain from a sequence of text/JSON items
    /// using 4-gram SimHash deduplication and Kneedle knee-point detection on the cumulative
    /// unique word-bigram saturation curve.
    #[must_use]
    pub fn compute_optimal_k(items: &[String], min_keep: usize, max_keep: usize) -> usize {
        let n = items.len();
        if n <= 8 || n <= min_keep {
            return n;
        }
        let effective_max = max_keep.max(min_keep).min(n);

        // Tier 1: 64-bit 4-gram SimHash near-total redundancy check
        let mut unique_simhashes = HashSet::new();
        for item in items {
            unique_simhashes.insert(Self::simhash_64(item));
        }
        if unique_simhashes.len() <= 3 {
            return unique_simhashes.len().max(min_keep.min(n));
        }

        // Tier 2: Cumulative unique word-bigram coverage curve + Kneedle
        let curve = Self::compute_unique_bigram_curve(items);
        let knee_k = Self::find_knee(&curve).unwrap_or_else(|| {
            // Fallback: square-root scaling bounded by [min_keep, effective_max]
            ((n as f64).sqrt().ceil() as usize * 2).clamp(min_keep, effective_max)
        });

        let mut k = knee_k.clamp(min_keep, effective_max);

        // Tier 3: Vocabulary coverage sanity check — if the first K items cover < 45% of unique bigrams
        // while K < effective_max, bump K by 20%
        if let Some(&total_bigrams) = curve.last() {
            if total_bigrams > 0 && k < effective_max {
                let cov_at_k = curve.get(k.saturating_sub(1)).copied().unwrap_or(0) as f64
                    / total_bigrams as f64;
                if cov_at_k < 0.45 {
                    k = ((k as f64 * 1.2).ceil() as usize).clamp(min_keep, effective_max);
                }
            }
        }

        k
    }

    /// Computes a deterministic 64-bit SimHash over character 4-grams using SHA-256 bit voting.
    #[must_use]
    pub fn simhash_64(text: &str) -> u64 {
        let chars: Vec<char> = text.chars().collect();
        if chars.is_empty() {
            return 0;
        }
        let mut weights = [0i32; 64];
        let window = chars.len().min(4);
        for slice in chars.windows(window) {
            let gram: String = slice.iter().collect();
            let digest = sha256_digest(gram.as_bytes());
            let mut bytes = [0u8; 8];
            bytes.copy_from_slice(&digest[..8]);
            let hash = u64::from_be_bytes(bytes);
            for (bit, weight) in weights.iter_mut().enumerate() {
                if (hash >> bit) & 1 == 1 {
                    *weight += 1;
                } else {
                    *weight -= 1;
                }
            }
        }
        let mut out = 0u64;
        for (bit, &w) in weights.iter().enumerate() {
            if w > 0 {
                out |= 1u64 << bit;
            }
        }
        out
    }

    /// Returns the bit-level Hamming distance between two 64-bit SimHashes.
    #[inline]
    #[must_use]
    pub fn hamming_distance(a: u64, b: u64) -> u32 {
        (a ^ b).count_ones()
    }

    /// Computes cumulative unique word-bigram counts across items.
    #[must_use]
    pub fn compute_unique_bigram_curve(items: &[String]) -> Vec<usize> {
        let mut seen_bigrams: HashSet<(String, String)> = HashSet::new();
        let mut curve = Vec::with_capacity(items.len());

        for item in items {
            let words: Vec<&str> = item.split_whitespace().collect();
            if words.is_empty() {
                seen_bigrams.insert((String::new(), String::new()));
            } else if words.len() == 1 {
                seen_bigrams.insert((words[0].to_string(), String::new()));
            } else {
                for pair in words.windows(2) {
                    seen_bigrams.insert((pair[0].to_string(), pair[1].to_string()));
                }
            }
            curve.push(seen_bigrams.len());
        }
        curve
    }

    /// Kneedle knee-point detection on a monotonically non-decreasing cumulative curve.
    /// Returns `Some(1-based count)` if the maximum deviation from the normalized diagonal exceeds `0.05`.
    #[must_use]
    pub fn find_knee(curve: &[usize]) -> Option<usize> {
        let n = curve.len();
        if n < 3 {
            return None;
        }
        let min_y = *curve.first()? as f64;
        let max_y = *curve.last()? as f64;
        let range_y = max_y - min_y;
        if range_y <= f64::EPSILON {
            return Some(1);
        }

        let mut max_dev = 0.0f64;
        let mut best_idx = 0usize;

        for (i, &y_val) in curve.iter().enumerate() {
            let x_norm = i as f64 / (n - 1) as f64;
            let y_norm = (y_val as f64 - min_y) / range_y;
            let dev = y_norm - x_norm;
            if dev > max_dev {
                max_dev = dev;
                best_idx = i;
            }
        }

        if max_dev > 0.05 {
            Some(best_idx + 1)
        } else {
            None
        }
    }
}

// ============================================================================
// 2. AnchorSelector & Numeric Summary Statistics
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NumericFieldStats {
    pub count: usize,
    pub min: f64,
    pub max: f64,
    pub mean: f64,
    pub p50: f64,
    pub p95: f64,
}

use serde::{Deserialize, Serialize};

pub struct AnchorSelector;

impl AnchorSelector {
    /// Selects up to `target_k` indices (plus any mandatory error/anomaly indices) from an array
    /// of JSON objects using:
    /// 1. Mandatory error/failure/anomaly preservation
    /// 2. Statistical numeric outlier preservation (`|z| > 2.2`)
    /// 3. Front/back positional anchors
    /// 4. Information-density scoring for middle slots (field-value rarity + structural novelty)
    /// 5. Exact item-hash deduplication
    #[must_use]
    pub fn select_indices(items: &[Value], target_k: usize) -> BTreeSet<usize> {
        let n = items.len();
        if n <= target_k {
            return (0..n).collect();
        }

        let mut selected = BTreeSet::new();
        let mut seen_hashes = HashSet::new();

        // 1. Always preserve first 2 and last 2 positional anchors
        for idx in [0, 1, n.saturating_sub(2), n.saturating_sub(1)] {
            if idx < n {
                let h = Self::item_hash(&items[idx]);
                if seen_hashes.insert(h) {
                    selected.insert(idx);
                }
            }
        }

        // 2. Always preserve error/anomaly items
        for (idx, item) in items.iter().enumerate() {
            if Self::is_error_or_anomaly_item(item) {
                let h = Self::item_hash(item);
                seen_hashes.insert(h);
                selected.insert(idx);
            }
        }

        // 3. Preserve extreme numeric outliers (|z-score| > 2.2)
        for outlier_idx in Self::detect_numeric_outliers(items) {
            if selected.len() < target_k + 4 {
                let h = Self::item_hash(&items[outlier_idx]);
                if seen_hashes.insert(h) {
                    selected.insert(outlier_idx);
                }
            }
        }

        if selected.len() >= target_k {
            return selected;
        }

        // 4. Score remaining middle candidates by information density (value rarity + content length)
        let mut value_freq: HashMap<String, usize> = HashMap::new();
        for item in items {
            if let Some(obj) = item.as_object() {
                for (k, v) in obj {
                    let pair_key = format!("{}={}", k, v);
                    *value_freq.entry(pair_key).or_insert(0) += 1;
                }
            }
        }

        let mut scored_middle: Vec<(usize, f64)> = Vec::new();
        for (idx, item) in items.iter().enumerate() {
            if selected.contains(&idx) {
                continue;
            }
            let mut rarity_sum = 0.0f64;
            let mut field_count = 0usize;
            if let Some(obj) = item.as_object() {
                for (k, v) in obj {
                    let pair_key = format!("{}={}", k, v);
                    let freq = *value_freq.get(&pair_key).unwrap_or(&1) as f64;
                    rarity_sum += 1.0 / freq;
                    field_count += 1;
                }
            }
            let rarity_score = if field_count > 0 {
                rarity_sum / field_count as f64
            } else {
                0.0
            };
            let len_score = (item.to_string().len() as f64).ln_1p() * 0.05;
            scored_middle.push((idx, rarity_score + len_score));
        }

        scored_middle.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.0.cmp(&b.0))
        });

        for (idx, _) in scored_middle {
            if selected.len() >= target_k {
                break;
            }
            let h = Self::item_hash(&items[idx]);
            if seen_hashes.insert(h) {
                selected.insert(idx);
            }
        }

        selected
    }

    #[must_use]
    pub fn is_error_or_anomaly_item(item: &Value) -> bool {
        let Some(obj) = item.as_object() else {
            return false;
        };
        obj.iter().any(|(k, v)| {
            let k_lower = k.to_ascii_lowercase();
            if matches!(
                k_lower.as_str(),
                "error" | "err" | "exception" | "panic" | "fatal" | "traceback" | "failure"
            ) && !v.is_null()
                && v != &Value::Bool(false)
                && v != &Value::String(String::new())
            {
                return true;
            }
            if (k_lower == "status" || k_lower == "status_code" || k_lower == "code")
                && v.as_i64().map(|code| code >= 400).unwrap_or(false)
            {
                return true;
            }
            if let Some(s) = v.as_str() {
                let s_lower = s.to_ascii_lowercase();
                if s_lower.contains("error")
                    || s_lower.contains("failed")
                    || s_lower.contains("timeout")
                    || s_lower.contains("panic")
                    || s_lower.contains("fatal")
                    || s_lower.contains("exception")
                {
                    return true;
                }
            }
            false
        })
    }

    #[must_use]
    pub fn detect_numeric_outliers(items: &[Value]) -> Vec<usize> {
        let mut numeric_columns: BTreeMap<String, Vec<(usize, f64)>> = BTreeMap::new();
        for (idx, item) in items.iter().enumerate() {
            if let Some(obj) = item.as_object() {
                for (k, v) in obj {
                    if let Some(num) = v.as_f64() {
                        if num.is_finite() {
                            numeric_columns
                                .entry(k.clone())
                                .or_default()
                                .push((idx, num));
                        }
                    }
                }
            }
        }

        let mut outliers = BTreeSet::new();
        for series in numeric_columns.values() {
            if series.len() < 6 {
                continue;
            }
            let n = series.len() as f64;
            let mean = series.iter().map(|(_, v)| *v).sum::<f64>() / n;
            let variance = series
                .iter()
                .map(|(_, v)| {
                    let d = *v - mean;
                    d * d
                })
                .sum::<f64>()
                / n;
            let std_dev = variance.sqrt();
            if std_dev <= f64::EPSILON {
                continue;
            }
            for &(idx, val) in series {
                let z = (val - mean).abs() / std_dev;
                if z > 2.2 {
                    outliers.insert(idx);
                }
            }
        }
        outliers.into_iter().collect()
    }

    /// Detects text lines in a log/table slice containing `key=value` or suffixed numeric metrics
    /// (e.g. `latency=950ms`, `duration: 4200ms`) whose value is a statistical outlier ($|z| > 2.5$).
    /// Note: Uniformly spaced sequential counters have $\max |z| \le \sqrt{3} \approx 1.732$, so they
    /// never trigger false-positive outlier extraction.
    #[must_use]
    pub fn detect_line_numeric_outliers(lines: &[&str]) -> Vec<usize> {
        if lines.len() < 12 {
            return Vec::new();
        }
        let mut metric_series: BTreeMap<String, Vec<(usize, f64)>> = BTreeMap::new();

        for (idx, line) in lines.iter().enumerate() {
            for word in line.split_whitespace() {
                let clean =
                    word.trim_matches(|c: char| matches!(c, '[' | ']' | '(' | ')' | ',' | ';'));
                if let Some((k, raw_v)) = clean.split_once('=') {
                    let num_part: String = raw_v
                        .chars()
                        .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-')
                        .collect();
                    if !num_part.is_empty() {
                        if let Ok(val) = num_part.parse::<f64>() {
                            if val.is_finite() {
                                metric_series
                                    .entry(k.to_ascii_lowercase())
                                    .or_default()
                                    .push((idx, val));
                            }
                        }
                    }
                }
            }
        }

        let mut outliers = BTreeSet::new();
        for series in metric_series.values() {
            if series.len() < 8 {
                continue;
            }
            let n = series.len() as f64;
            let mean = series.iter().map(|(_, v)| *v).sum::<f64>() / n;
            let variance = series
                .iter()
                .map(|(_, v)| {
                    let d = *v - mean;
                    d * d
                })
                .sum::<f64>()
                / n;
            let std_dev = variance.sqrt();
            if std_dev <= f64::EPSILON {
                continue;
            }
            for &(idx, val) in series {
                let z = (val - mean).abs() / std_dev;
                if z > 2.5 {
                    outliers.insert(idx);
                }
            }
        }

        outliers.into_iter().collect()
    }

    /// Computes summary statistics (`min`, `max`, `mean`, `p50`, `p95`) for numeric fields across an array of objects.
    #[must_use]
    pub fn compute_numeric_summaries(items: &[Value]) -> Map<String, Value> {
        let mut columns: BTreeMap<String, Vec<f64>> = BTreeMap::new();
        for item in items {
            if let Some(obj) = item.as_object() {
                for (k, v) in obj {
                    if k == "id" || k.ends_with("_id") {
                        continue;
                    }
                    if let Some(num) = v.as_f64() {
                        if num.is_finite() {
                            columns.entry(k.clone()).or_default().push(num);
                        }
                    }
                }
            }
        }

        let mut stats_map = Map::new();
        for (k, mut vals) in columns {
            if vals.len() < 4 {
                continue;
            }
            vals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let min_v = vals[0];
            let max_v = vals[vals.len() - 1];
            if (max_v - min_v).abs() <= f64::EPSILON {
                continue;
            }
            let mean_v = vals.iter().sum::<f64>() / vals.len() as f64;
            let p50_v = vals[vals.len() / 2];
            let p95_idx = ((vals.len() as f64 * 0.95).floor() as usize).min(vals.len() - 1);
            let p95_v = vals[p95_idx];

            stats_map.insert(
                k,
                serde_json::json!({
                    "min": (min_v * 100.0).round() / 100.0,
                    "max": (max_v * 100.0).round() / 100.0,
                    "mean": (mean_v * 100.0).round() / 100.0,
                    "p50": (p50_v * 100.0).round() / 100.0,
                    "p95": (p95_v * 100.0).round() / 100.0,
                }),
            );
        }
        stats_map
    }

    fn item_hash(item: &Value) -> u64 {
        let canonical = CacheAligner::normalize_json_value(item);
        let serialized = serde_json::to_string(&canonical).unwrap_or_default();
        let digest = sha256_digest(serialized.as_bytes());
        let mut bytes = [0u8; 8];
        bytes.copy_from_slice(&digest[..8]);
        u64::from_be_bytes(bytes)
    }
}

// ============================================================================
// 3. DiffCompressor — Unified Git Diff Compressor (`diff_compressor.rs` Parity)
// ============================================================================

pub struct DiffCompressor;

#[derive(Debug, Clone)]
struct ParsedDiffFile {
    header_lines: Vec<String>,
    file_path: String,
    hunks: Vec<Vec<String>>,
    added_lines: usize,
    removed_lines: usize,
}

impl DiffCompressor {
    /// Detects if text is a unified diff (`diff --git` or `--- a/` + `+++ b/` + `@@`) and compresses it:
    /// 1. Ranks files by total `+`/`-` changes + error/signature importance and caps at `max_files` (15).
    /// 2. Caps per-file hunks at `max_hunks_per_file` (6), keeping first + last + highest-scored middle hunks.
    /// 3. Trims unchanged context lines around `+`/`-` edits to `max_context_lines` (3).
    /// 4. Caches the lossless original in `CcrCache` when savings >= 20%.
    #[must_use]
    pub fn compress(raw_diff: &str) -> Option<(String, String)> {
        if !Self::is_unified_diff(raw_diff) {
            return None;
        }
        let total_lines = raw_diff.lines().count();
        if total_lines < 30 || raw_diff.len() < 1_000 {
            return None;
        }

        let files = Self::parse_unified_diff(raw_diff);
        if files.is_empty() {
            return None;
        }

        let max_files = 15usize;
        let max_hunks_per_file = 6usize;
        let max_context_lines = 3usize;

        let mut indexed_files: Vec<(usize, &ParsedDiffFile, usize)> = files
            .iter()
            .enumerate()
            .map(|(i, f)| (i, f, f.added_lines + f.removed_lines))
            .collect();

        let mut dropped_files = Vec::new();
        let kept_indices: BTreeSet<usize> = if indexed_files.len() > max_files {
            indexed_files.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| a.0.cmp(&b.0)));
            let mut kept = BTreeSet::new();
            for (rank, (orig_idx, file, changes)) in indexed_files.into_iter().enumerate() {
                if rank < max_files {
                    kept.insert(orig_idx);
                } else {
                    dropped_files.push(format!("`{}` ({} changes)", file.file_path, changes));
                }
            }
            kept
        } else {
            (0..files.len()).collect()
        };

        let mut out_lines: Vec<String> = Vec::new();
        let mut total_hunks_dropped = 0usize;

        for (idx, file) in files.iter().enumerate() {
            if !kept_indices.contains(&idx) {
                continue;
            }
            for h in &file.header_lines {
                out_lines.push(h.clone());
            }

            let hunks = &file.hunks;
            let selected_hunk_indices: BTreeSet<usize> = if hunks.len() > max_hunks_per_file {
                let mut set = BTreeSet::new();
                set.insert(0);
                set.insert(hunks.len() - 1);
                let mut middle_scored: Vec<(usize, usize)> = (1..hunks.len() - 1)
                    .map(|hi| (hi, Self::score_hunk(&hunks[hi])))
                    .collect();
                middle_scored.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
                for (hi, _) in middle_scored.into_iter().take(max_hunks_per_file - 2) {
                    set.insert(hi);
                }
                total_hunks_dropped += hunks.len() - set.len();
                set
            } else {
                (0..hunks.len()).collect()
            };

            let mut prev_hi: Option<usize> = None;
            for hi in selected_hunk_indices {
                if let Some(p) = prev_hi {
                    if hi > p + 1 {
                        out_lines.push(format!(
                            "@@ ... [{} intermediate hunk(s) in `{}` omitted] ... @@",
                            hi - p - 1,
                            file.file_path
                        ));
                    }
                }
                let trimmed_hunk = Self::trim_hunk_context(&hunks[hi], max_context_lines);
                out_lines.extend(trimmed_hunk);
                prev_hi = Some(hi);
            }
        }

        if !dropped_files.is_empty() {
            out_lines.push(format!(
                "\n[DiffCompressor: {} lower-churn file(s) omitted: {}]",
                dropped_files.len(),
                dropped_files.join(", ")
            ));
        }

        let mut compressed = out_lines.join("\n");
        let orig_len = raw_diff.len();
        let comp_len = compressed.len();

        // Only emit compressed diff if savings >= 20% and at least 200 bytes saved
        if comp_len + 200 >= orig_len || (orig_len - comp_len) * 100 / orig_len < 20 {
            return None;
        }

        let ccr_id = CcrCache::store(raw_diff);
        compressed.push_str(&format!(
            "\n\n[Diff compressed: {} -> {} bytes ({} files, {} hunks omitted). Use retrieve_observation(id=\"{}\") or retrieve_observation(id=\"{}\", query=\"...\") for full diff.]",
            orig_len,
            comp_len,
            files.len(),
            total_hunks_dropped,
            ccr_id,
            ccr_id
        ));

        Some((compressed, ccr_id))
    }

    #[must_use]
    pub fn is_unified_diff(text: &str) -> bool {
        let has_diff_git = text.contains("diff --git ");
        let has_hunk = text
            .lines()
            .any(|l| l.starts_with("@@ ") && l.contains(" @@"));
        let has_minus_plus = text.lines().any(|l| l.starts_with("--- "))
            && text.lines().any(|l| l.starts_with("+++ "));
        (has_diff_git || has_minus_plus) && has_hunk
    }

    fn parse_unified_diff(raw_diff: &str) -> Vec<ParsedDiffFile> {
        let mut files: Vec<ParsedDiffFile> = Vec::new();
        let mut current_file: Option<ParsedDiffFile> = None;
        let mut current_hunk: Vec<String> = Vec::new();

        for line in raw_diff.lines() {
            if line.starts_with("diff --git ") {
                if let Some(mut f) = current_file.take() {
                    if !current_hunk.is_empty() {
                        f.hunks.push(std::mem::take(&mut current_hunk));
                    }
                    files.push(f);
                }
                let path = line
                    .split_whitespace()
                    .last()
                    .unwrap_or("unknown")
                    .trim_start_matches("b/")
                    .to_string();
                current_file = Some(ParsedDiffFile {
                    header_lines: vec![line.to_string()],
                    file_path: path,
                    hunks: Vec::new(),
                    added_lines: 0,
                    removed_lines: 0,
                });
            } else if line.starts_with("@@ ") {
                if current_file.is_none() {
                    current_file = Some(ParsedDiffFile {
                        header_lines: Vec::new(),
                        file_path: "patch".to_string(),
                        hunks: Vec::new(),
                        added_lines: 0,
                        removed_lines: 0,
                    });
                }
                if let Some(ref mut f) = current_file {
                    if !current_hunk.is_empty() {
                        f.hunks.push(std::mem::take(&mut current_hunk));
                    }
                }
                current_hunk.push(line.to_string());
            } else if let Some(ref mut f) = current_file {
                if current_hunk.is_empty() {
                    if let Some(stripped) = line.strip_prefix("+++ b/") {
                        f.file_path = stripped.trim().to_string();
                    }
                    f.header_lines.push(line.to_string());
                } else {
                    if line.starts_with('+') && !line.starts_with("+++") {
                        f.added_lines += 1;
                    } else if line.starts_with('-') && !line.starts_with("---") {
                        f.removed_lines += 1;
                    }
                    current_hunk.push(line.to_string());
                }
            }
        }

        if let Some(mut f) = current_file.take() {
            if !current_hunk.is_empty() {
                f.hunks.push(current_hunk);
            }
            files.push(f);
        }

        files
    }

    fn score_hunk(hunk: &[String]) -> usize {
        let mut score = 0usize;
        for line in hunk.iter().skip(1) {
            if line.starts_with('+') || line.starts_with('-') {
                score += 2;
                let lower = line.to_ascii_lowercase();
                if lower.contains("fn ")
                    || lower.contains("pub ")
                    || lower.contains("struct ")
                    || lower.contains("enum ")
                    || lower.contains("trait ")
                    || lower.contains("class ")
                    || lower.contains("def ")
                    || lower.contains("interface ")
                    || lower.contains("error")
                {
                    score += 5;
                }
            }
        }
        score
    }

    fn trim_hunk_context(hunk: &[String], max_context: usize) -> Vec<String> {
        if hunk.len() <= max_context * 2 + 4 {
            return hunk.to_vec();
        }
        let mut out = Vec::new();
        if let Some(header) = hunk.first() {
            out.push(header.clone());
        }
        let body = &hunk[1..];
        let change_positions: Vec<usize> = body
            .iter()
            .enumerate()
            .filter_map(|(i, l)| {
                if (l.starts_with('+') && !l.starts_with("+++"))
                    || (l.starts_with('-') && !l.starts_with("---"))
                {
                    Some(i)
                } else {
                    None
                }
            })
            .collect();

        if change_positions.is_empty() {
            out.extend(body.iter().take(max_context * 2).cloned());
            return out;
        }

        let mut keep = vec![false; body.len()];
        for &pos in &change_positions {
            let start = pos.saturating_sub(max_context);
            let end = (pos + max_context).min(body.len() - 1);
            for slot in &mut keep[start..=end] {
                *slot = true;
            }
        }

        let mut skipped = 0usize;
        for (i, line) in body.iter().enumerate() {
            if keep[i] {
                if skipped > 0 {
                    out.push(format!(
                        " ... [{} unchanged context lines trimmed] ...",
                        skipped
                    ));
                    skipped = 0;
                }
                out.push(line.clone());
            } else {
                skipped += 1;
            }
        }
        if skipped > 0 {
            out.push(format!(
                " ... [{} unchanged context lines trimmed] ...",
                skipped
            ));
        }
        out
    }
}

// ============================================================================
// 4. SearchCompressor — Grep / Ripgrep Output Compressor (`search_compressor.rs` Parity)
// ============================================================================

pub struct SearchCompressor;

#[derive(Debug, Clone)]
struct SearchMatchLine {
    line_num: usize,
    separator: char,
    content: String,
    score: usize,
}

impl SearchCompressor {
    /// Detects and compresses large `grep -n` / `ripgrep` (`file:line:content` or `file-line-context`)
    /// search outputs using `LineImportanceDetector` scoring and `AdaptiveSizer::compute_optimal_k`,
    /// preserving lossless recovery via `CcrCache`.
    #[must_use]
    pub fn compress(raw_search: &str) -> Option<(String, String)> {
        let lines: Vec<&str> = raw_search.lines().collect();
        if lines.len() < 25 || raw_search.len() < 1_200 {
            return None;
        }

        let mut file_groups: BTreeMap<String, Vec<SearchMatchLine>> = BTreeMap::new();
        let mut header_lines: Vec<String> = Vec::new();
        let mut parsed_count = 0usize;

        for line in &lines {
            let trimmed = line.trim_end();
            if trimmed.is_empty() || trimmed == "--" {
                continue;
            }
            if let Some((file, line_num, sep, content)) = Self::parse_grep_line(trimmed) {
                file_groups.entry(file).or_default().push(SearchMatchLine {
                    line_num,
                    separator: sep,
                    content,
                    score: 1,
                });
                parsed_count += 1;
            } else if parsed_count == 0 && header_lines.len() < 3 {
                header_lines.push(trimmed.to_string());
            }
        }

        // Require at least 60% of non-empty lines to match grep/ripgrep format and >= 20 matches
        if parsed_count < 20 || parsed_count * 10 / lines.len().max(1) < 6 {
            return None;
        }

        // Score all match lines using corpus-wide Inverse Document Frequency (IDF) + Shannon entropy
        Self::score_corpus_matches(&mut file_groups, parsed_count);

        // Compute adaptive budget K across all matches
        let all_contents: Vec<String> = file_groups
            .values()
            .flat_map(|v| v.iter().map(|m| m.content.clone()))
            .collect();
        let optimal_total_k = AdaptiveSizer::compute_optimal_k(&all_contents, 15, 60);

        let max_files = 18usize;
        let mut ranked_files: Vec<(String, usize, usize)> = file_groups
            .iter()
            .map(|(f, matches)| {
                let total_score: usize = matches.iter().map(|m| m.score).sum();
                (f.clone(), total_score, matches.len())
            })
            .collect();
        ranked_files.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

        let kept_file_count = ranked_files.len().min(max_files);
        let per_file_cap = (optimal_total_k / kept_file_count.max(1)).clamp(3, 12);

        let mut out_lines = header_lines;
        let mut omitted_files = 0usize;
        let mut omitted_matches = 0usize;

        for (rank, (file_name, _, _)) in ranked_files.into_iter().enumerate() {
            let Some(matches) = file_groups.get(&file_name) else {
                continue;
            };
            if rank >= max_files {
                omitted_files += 1;
                omitted_matches += matches.len();
                continue;
            }

            if matches.len() <= per_file_cap {
                for m in matches {
                    out_lines.push(format!(
                        "{}{}{}{}{}",
                        file_name, m.separator, m.line_num, m.separator, m.content
                    ));
                }
            } else {
                // Keep first, last, and highest-scored middle matches in line order
                let mut selected_idx = BTreeSet::new();
                selected_idx.insert(0);
                selected_idx.insert(matches.len() - 1);

                let mut middle: Vec<(usize, usize)> = (1..matches.len() - 1)
                    .map(|idx| (idx, matches[idx].score))
                    .collect();
                middle.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
                for (idx, _) in middle.into_iter().take(per_file_cap.saturating_sub(2)) {
                    selected_idx.insert(idx);
                }

                for idx in &selected_idx {
                    let m = &matches[*idx];
                    out_lines.push(format!(
                        "{}{}{}{}{}",
                        file_name, m.separator, m.line_num, m.separator, m.content
                    ));
                }
                let dropped_in_file = matches.len() - selected_idx.len();
                omitted_matches += dropped_in_file;
                out_lines.push(format!(
                    "  [... and {} more match(es) in `{}` ...]",
                    dropped_in_file, file_name
                ));
            }
        }

        if omitted_files > 0 {
            out_lines.push(format!(
                "[... and {} more file(s) with {} match(es) omitted ...]",
                omitted_files, omitted_matches
            ));
        }

        let mut compressed = out_lines.join("\n");
        let orig_len = raw_search.len();
        let comp_len = compressed.len();

        if comp_len + 150 >= orig_len || (orig_len - comp_len) * 100 / orig_len < 20 {
            return None;
        }

        let ccr_id = CcrCache::store(raw_search);
        compressed.push_str(&format!(
            "\n\n[Search results compressed: {} -> {} matches ({} -> {} bytes). Use retrieve_observation(id=\"{}\", query=\"...\") for full matches.]",
            parsed_count,
            parsed_count.saturating_sub(omitted_matches),
            orig_len,
            comp_len,
            ccr_id
        ));

        Some((compressed, ccr_id))
    }

    /// Hand-rolled parser for `file:line:content` and `file-line-context` that properly handles
    /// Windows drive-letter paths (`C:\path\file.rs:42:code`) and Unix paths (`src/lib.rs:42:code`).
    fn parse_grep_line(line: &str) -> Option<(String, usize, char, String)> {
        let bytes = line.as_bytes();
        // Skip Windows drive prefix like `C:\` or `D:/` when scanning for separator
        let start_scan = if bytes.len() >= 3
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && (bytes[2] == b'\\' || bytes[2] == b'/')
        {
            3
        } else {
            0
        };

        for (offset, ch) in line[start_scan..].char_indices() {
            let idx = start_scan + offset;
            if ch == ':' || ch == '-' {
                let file_part = &line[..idx];
                if file_part.is_empty() || file_part.contains(' ') {
                    return None;
                }
                let rest = &line[idx + 1..];
                let num_end = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(0);
                if num_end == 0 {
                    continue;
                }
                let sep2 = rest[num_end..].chars().next()?;
                if sep2 != ch {
                    continue;
                }
                let line_num: usize = rest[..num_end].parse().ok()?;
                let content = rest[num_end + 1..].to_string();
                return Some((file_part.to_string(), line_num, ch, content));
            }
        }
        None
    }

    /// Scores search match lines across the result corpus using:
    /// 1. Inverse Document Frequency (IDF) of identifier tokens: $\sum_{t \in L} \ln(1 + N / \text{df}(t))$
    /// 2. Shannon character entropy $H(L) = -\sum p(c)\log_2 p(c)$
    /// 3. Top-level structural signature density (low indentation + CamelCase / declaration punctuation)
    fn score_corpus_matches(
        file_groups: &mut BTreeMap<String, Vec<SearchMatchLine>>,
        total_docs: usize,
    ) {
        let mut doc_freq: BTreeMap<String, usize> = BTreeMap::new();
        for matches in file_groups.values() {
            for m in matches {
                let mut unique_tokens = BTreeSet::new();
                for tok in Self::extract_identifier_tokens(&m.content) {
                    unique_tokens.insert(tok);
                }
                for tok in unique_tokens {
                    *doc_freq.entry(tok).or_insert(0) += 1;
                }
            }
        }

        let n = total_docs.max(1) as f64;
        for matches in file_groups.values_mut() {
            for m in matches.iter_mut() {
                let tokens = Self::extract_identifier_tokens(&m.content);
                let mut idf_sum = 0.0f64;
                let mut has_type_ident = false;

                for tok in &tokens {
                    let df = (*doc_freq.get(tok).unwrap_or(&1)).max(1) as f64;
                    idf_sum += (1.0 + n / df).ln();
                    if tok.chars().next().is_some_and(|c| c.is_ascii_uppercase())
                        && tok.chars().any(|c| c.is_ascii_lowercase())
                    {
                        has_type_ident = true;
                    }
                }

                let avg_idf = if tokens.is_empty() {
                    0.0
                } else {
                    idf_sum / (tokens.len() as f64).sqrt()
                };
                let entropy = Self::shannon_entropy(m.content.trim());

                // Top-level structural declaration weight (low leading indent + signature delimiters)
                let leading_spaces = m.content.chars().take_while(|c| c.is_whitespace()).count();
                let trimmed = m.content.trim();
                let is_top_level_sig = leading_spaces <= 4
                    && (trimmed.ends_with('{')
                        || trimmed.ends_with(':')
                        || (trimmed.contains('(') && trimmed.contains(')')));

                let structural_bonus =
                    if is_top_level_sig { 6usize } else { 0 } + if has_type_ident { 3 } else { 0 };
                let computed =
                    1 + (avg_idf * 2.5) as usize + (entropy * 1.5) as usize + structural_bonus;
                m.score = computed;
            }
        }
    }

    fn extract_identifier_tokens(s: &str) -> Vec<String> {
        let mut tokens = Vec::new();
        let mut cur = String::new();
        for ch in s.chars() {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                cur.push(ch);
            } else if !cur.is_empty() {
                if cur.len() >= 2 && !cur.chars().all(|c| c.is_ascii_digit()) {
                    tokens.push(std::mem::take(&mut cur));
                } else {
                    cur.clear();
                }
            }
        }
        if cur.len() >= 2 && !cur.chars().all(|c| c.is_ascii_digit()) {
            tokens.push(cur);
        }
        tokens
    }

    #[must_use]
    pub fn shannon_entropy(s: &str) -> f64 {
        if s.is_empty() {
            return 0.0;
        }
        let mut counts = [0u32; 128];
        let mut total = 0u32;
        for b in s.bytes() {
            let bucket = (b as usize) & 0x7F;
            counts[bucket] += 1;
            total += 1;
        }
        if total == 0 {
            return 0.0;
        }
        let n = total as f64;
        let mut h = 0.0f64;
        for &c in &counts {
            if c > 0 {
                let p = c as f64 / n;
                h -= p * p.log2();
            }
        }
        h
    }
}

// ============================================================================
// 5. CodeCompressor — Tree-sitter AST Function-Body Folder (`code_compressor.rs` Parity)
// ============================================================================

pub struct CodeCompressor;

impl CodeCompressor {
    /// AST-preserving source code compressor:
    /// Parses source code via Tree-sitter (`RepoMapExtractor`), preserves 100% of imports,
    /// struct/enum/trait/type definitions, constants, docstrings, and function signatures,
    /// and folds long function bodies (> 18 lines) with a lossless `CcrCache` recovery key.
    #[must_use]
    pub fn compress(ext: &str, raw_code: &str) -> Option<(String, String)> {
        let lines: Vec<&str> = raw_code.lines().collect();
        if lines.len() < 50 || raw_code.len() < 1_500 {
            return None;
        }

        let extractor = crate::context::ast::repomap::RepoMapExtractor::new();
        let symbols = extractor.extract_symbols_from_str(ext, raw_code).ok()?;
        let foldable_fns: Vec<_> = symbols
            .into_iter()
            .filter(|s| {
                matches!(s.kind.as_str(), "function" | "method")
                    && s.end_line > s.line_number + 18
                    && s.end_line <= lines.len()
            })
            .collect();

        if foldable_fns.len() < 2 {
            return None;
        }

        let ccr_id = CcrCache::store(raw_code);
        let mut fold_ranges: BTreeMap<usize, (usize, String)> = BTreeMap::new();
        for sym in foldable_fns {
            // Keep signature line(s) + first 2 lines of body, fold middle up to closing line
            let fold_start = sym.line_number + 2;
            let fold_end = sym.end_line.saturating_sub(1);
            if fold_end > fold_start + 10 {
                // Never fold if body contains an explicit panic/error diagnostic marker
                let body_slice = &lines[fold_start - 1..fold_end];
                let has_critical = body_slice
                    .iter()
                    .any(|l| l.contains("FIXME") || l.contains("compile_error!"));
                if !has_critical {
                    fold_ranges.insert(fold_start, (fold_end, sym.name));
                }
            }
        }

        if fold_ranges.is_empty() {
            return None;
        }

        let mut out_lines = Vec::new();
        let mut line_no = 1usize;
        let mut folded_count = 0usize;

        while line_no <= lines.len() {
            if let Some((end_line, fn_name)) = fold_ranges.get(&line_no) {
                let omitted = end_line - line_no + 1;
                out_lines.push(format!(
                    "    /* [... `{}` body: {} lines (L{}-L{}) folded — retrieve_observation(id=\"{}\", offset={}, limit={}) ...] */",
                    fn_name,
                    omitted,
                    line_no,
                    end_line,
                    ccr_id,
                    line_no.saturating_sub(1),
                    omitted + 4
                ));
                folded_count += 1;
                line_no = *end_line + 1;
            } else {
                out_lines.push(lines[line_no - 1].to_string());
                line_no += 1;
            }
        }

        let compressed = out_lines.join("\n");
        if folded_count == 0 || compressed.len() + 200 >= raw_code.len() {
            return None;
        }

        Some((compressed, ccr_id))
    }
}

// ============================================================================
// 6. TagProtector — Single-Pass O(N) Custom Workflow Tag Shield (`tag_protector.rs` Parity)
// ============================================================================

const HTML5_TAGS: &[&str] = &[
    "a",
    "abbr",
    "address",
    "area",
    "article",
    "aside",
    "audio",
    "b",
    "base",
    "bdi",
    "bdo",
    "blockquote",
    "body",
    "br",
    "button",
    "canvas",
    "caption",
    "cite",
    "code",
    "col",
    "colgroup",
    "data",
    "datalist",
    "dd",
    "del",
    "details",
    "dfn",
    "dialog",
    "div",
    "dl",
    "dt",
    "em",
    "embed",
    "fieldset",
    "figcaption",
    "figure",
    "footer",
    "form",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "head",
    "header",
    "hgroup",
    "hr",
    "html",
    "i",
    "iframe",
    "img",
    "input",
    "ins",
    "kbd",
    "label",
    "legend",
    "li",
    "link",
    "main",
    "map",
    "mark",
    "menu",
    "meta",
    "meter",
    "nav",
    "noscript",
    "object",
    "ol",
    "optgroup",
    "option",
    "output",
    "p",
    "param",
    "picture",
    "pre",
    "progress",
    "q",
    "rp",
    "rt",
    "ruby",
    "s",
    "samp",
    "script",
    "section",
    "select",
    "small",
    "source",
    "span",
    "strong",
    "style",
    "sub",
    "summary",
    "sup",
    "svg",
    "table",
    "tbody",
    "td",
    "template",
    "textarea",
    "tfoot",
    "th",
    "thead",
    "time",
    "title",
    "tr",
    "track",
    "u",
    "ul",
    "var",
    "video",
    "wbr",
];

pub struct TagProtector;

#[derive(Debug, Clone)]
pub struct ProtectedText {
    pub text_with_placeholders: String,
    pub replacements: Vec<(String, String)>,
}

impl TagProtector {
    /// Single-pass $O(N)$ walker that swaps custom XML workflow tags (`<system-reminder>`,
    /// `<scoped_developer_rules>`, `<minipower_active_plan>`, `<tool_call>`, `<thinking>`, etc.)
    /// with opaque placeholders (`__HR_TAG_0000__`) so text/markdown compressors never corrupt them,
    /// while leaving standard HTML5 tags (`<div>`, `<p>`, `<span>`) untouched.
    #[must_use]
    pub fn protect_tags(input: &str) -> ProtectedText {
        let mut out = String::with_capacity(input.len());
        let mut replacements = Vec::new();
        let bytes = input.as_bytes();
        let len = bytes.len();
        let mut i = 0usize;

        while i < len {
            if bytes[i] == b'<' {
                if let Some((tag_name, is_closing, is_self_closing, tag_end)) =
                    Self::parse_xml_tag_at(input, i)
                {
                    let lower_name = tag_name.to_ascii_lowercase();
                    if !HTML5_TAGS.contains(&lower_name.as_str()) && !is_closing {
                        if is_self_closing {
                            let placeholder = format!("{{{{HR_TAG_{:04}}}}}", replacements.len());
                            replacements.push((placeholder.clone(), input[i..tag_end].to_string()));
                            out.push_str(&placeholder);
                            i = tag_end;
                            continue;
                        }
                        let close_needle = format!("</{}>", tag_name);
                        if let Some(rel_close) = input[tag_end..].find(&close_needle) {
                            let span_end = tag_end + rel_close + close_needle.len();
                            let placeholder = format!("{{{{HR_TAG_{:04}}}}}", replacements.len());
                            replacements
                                .push((placeholder.clone(), input[i..span_end].to_string()));
                            out.push_str(&placeholder);
                            i = span_end;
                            continue;
                        }
                    }
                }
            }
            let ch = input[i..].chars().next().unwrap_or('\0');
            out.push(ch);
            i += ch.len_utf8().max(1);
        }

        ProtectedText {
            text_with_placeholders: out,
            replacements,
        }
    }

    /// Restores protected custom XML workflow tags back into the compressed text.
    #[must_use]
    pub fn restore_tags(compressed: &str, protected: &ProtectedText) -> String {
        if protected.replacements.is_empty() {
            return compressed.to_string();
        }
        let mut restored = compressed.to_string();
        for (placeholder, original) in &protected.replacements {
            restored = restored.replace(placeholder, original);
        }
        restored
    }

    fn parse_xml_tag_at(input: &str, start: usize) -> Option<(String, bool, bool, usize)> {
        let rest = &input[start..];
        if !rest.starts_with('<') || rest.starts_with("<!--") || rest.starts_with("<![") {
            return None;
        }
        let close_bracket = rest.find('>')?;
        if close_bracket > 240 {
            return None;
        }
        let inner = rest[1..close_bracket].trim();
        if inner.is_empty() {
            return None;
        }
        let is_closing = inner.starts_with('/');
        let is_self_closing = inner.ends_with('/');
        let name_slice = inner
            .trim_start_matches('/')
            .trim_end_matches('/')
            .split_whitespace()
            .next()?;
        if name_slice.is_empty() {
            return None;
        }
        let first_ch = name_slice.chars().next()?;
        if !first_ch.is_ascii_alphabetic() && first_ch != '_' {
            return None;
        }
        if !name_slice
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | ':' | '.'))
        {
            return None;
        }
        Some((
            name_slice.to_string(),
            is_closing,
            is_self_closing,
            start + close_bracket + 1,
        ))
    }
}

// ============================================================================
// 7. CacheAligner — Deterministic Tool Schema Normalizer (Invariant I7)
// ============================================================================

pub struct CacheAligner;

impl CacheAligner {
    /// Deterministically normalizes a slice of `ToolSchema` definitions (Headroom Invariant I7):
    /// 1. Sorts tools alphabetically by `name`.
    /// 2. Normalizes description whitespace.
    /// 3. Recursively sorts JSON Schema object keys so serialized tool definitions are 100%
    ///    byte-identical across turns and never bust provider KV-caches.
    pub fn normalize_tool_schemas(schemas: &mut [ToolSchema]) {
        schemas.sort_by(|a, b| a.name.cmp(&b.name));
        for schema in schemas.iter_mut() {
            schema.description = Self::collapse_inline_whitespace(&schema.description);
            schema.parameters = Self::normalize_json_value(&schema.parameters);
        }
    }

    /// Recursively normalizes a `serde_json::Value` by sorting object keys and normalizing description strings.
    #[must_use]
    pub fn normalize_json_value(val: &Value) -> Value {
        match val {
            Value::Object(map) => {
                let mut sorted_keys: Vec<&String> = map.keys().collect();
                sorted_keys.sort();
                let mut out_map = Map::with_capacity(map.len());
                for k in sorted_keys {
                    if let Some(v) = map.get(k) {
                        let norm_v = if k == "description" {
                            if let Some(s) = v.as_str() {
                                Value::String(Self::collapse_inline_whitespace(s))
                            } else {
                                Self::normalize_json_value(v)
                            }
                        } else {
                            Self::normalize_json_value(v)
                        };
                        out_map.insert(k.clone(), norm_v);
                    }
                }
                Value::Object(out_map)
            }
            Value::Array(arr) => Value::Array(arr.iter().map(Self::normalize_json_value).collect()),
            other => other.clone(),
        }
    }

    fn collapse_inline_whitespace(s: &str) -> String {
        s.split_whitespace().collect::<Vec<_>>().join(" ")
    }
}

// ============================================================================
// 8. CsvCompressor — Tabular CSV/TSV Dataset Compressor (`aswin402/headroom-mcp` Parity)
// ============================================================================

pub struct CsvCompressor;

impl CsvCompressor {
    /// Detects and compresses multi-row CSV/TSV datasets:
    /// 1. Validates consistent delimiter count (`,` or `\t`, >= 2 delimiters per row) across rows.
    /// 2. Preserves the header row, front/back anchor rows, and any anomaly/error rows.
    /// 3. Computes numeric column summary statistics (`min`, `max`, `mean`) across all rows.
    /// 4. Stores the full dataset in `CcrCache` for lossless retrieval or query search.
    #[must_use]
    pub fn compress(raw_csv: &str) -> Option<(String, String)> {
        let non_empty: Vec<&str> = raw_csv
            .lines()
            .map(str::trim_end)
            .filter(|l| !l.trim().is_empty())
            .collect();
        if non_empty.len() < 15 || raw_csv.len() < 500 {
            return None;
        }

        let header = non_empty[0];
        // Reject obvious non-CSV formats (JSON, diffs, Rust/JS/Python code blocks)
        if header.starts_with('{')
            || header.starts_with('[')
            || header.starts_with("diff --git")
            || header.starts_with("pub ")
            || header.starts_with("fn ")
            || header.starts_with("use ")
            || header.starts_with("import ")
            || header.starts_with('#')
            || header.starts_with("//")
        {
            return None;
        }

        let comma_count = Self::count_delimiters(header, ',');
        let tab_count = Self::count_delimiters(header, '\t');
        let (delim, expected_delims) = if tab_count >= 2 && tab_count >= comma_count {
            ('\t', tab_count)
        } else if comma_count >= 2 {
            (',', comma_count)
        } else {
            return None;
        };

        let sample_size = non_empty.len().min(25);
        let matching_rows = non_empty[1..sample_size]
            .iter()
            .filter(|row| Self::count_delimiters(row, delim) == expected_delims)
            .count();
        if matching_rows * 10 / (sample_size - 1).max(1) < 8 {
            return None;
        }

        let headers: Vec<&str> = header.split(delim).map(str::trim).collect();
        let data_rows = &non_empty[1..];
        let total_rows = data_rows.len();

        // Compute numeric column summaries
        let mut col_values: Vec<Vec<f64>> = vec![Vec::new(); headers.len()];
        let mut selected_indices = BTreeSet::new();

        // Always keep first 4 and last 2 rows
        for idx in [
            0,
            1,
            2,
            3,
            total_rows.saturating_sub(2),
            total_rows.saturating_sub(1),
        ] {
            if idx < total_rows {
                selected_indices.insert(idx);
            }
        }

        for (row_idx, &row) in data_rows.iter().enumerate() {
            let lower = row.to_ascii_lowercase();
            if lower.contains("error")
                || lower.contains("fail")
                || lower.contains("fatal")
                || lower.contains("timeout")
                || lower.contains("panic")
            {
                if selected_indices.len() < 14 {
                    selected_indices.insert(row_idx);
                }
            }
            for (col_idx, cell) in row.split(delim).enumerate() {
                if col_idx < col_values.len() {
                    if let Ok(val) = cell.trim().trim_matches('"').parse::<f64>() {
                        if val.is_finite() {
                            col_values[col_idx].push(val);
                        }
                    }
                }
            }
        }

        let mut out = String::new();
        out.push_str(&format!("Headers: {}\n", header));
        let mut prev_idx: Option<usize> = None;
        for &idx in &selected_indices {
            if let Some(prev) = prev_idx {
                if idx > prev + 1 {
                    out.push_str(&format!("  ... [{} rows omitted] ...\n", idx - prev - 1));
                }
            }
            out.push_str(&format!("Row {}: {}\n", idx + 1, data_rows[idx]));
            prev_idx = Some(idx);
        }

        // Append numeric column stats if available
        let mut col_stats = Vec::new();
        for (col_idx, vals) in col_values.iter().enumerate() {
            let col_name = headers.get(col_idx).copied().unwrap_or("col");
            if col_name.eq_ignore_ascii_case("id") || vals.len() < 4 {
                continue;
            }
            let min_v = vals.iter().copied().fold(f64::INFINITY, f64::min);
            let max_v = vals.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            if (max_v - min_v).abs() <= f64::EPSILON {
                continue;
            }
            let mean_v = vals.iter().sum::<f64>() / vals.len() as f64;
            col_stats.push(format!(
                "{}(min={:.2}, max={:.2}, mean={:.2})",
                col_name, min_v, max_v, mean_v
            ));
        }
        if !col_stats.is_empty() {
            out.push_str(&format!("Numeric Summary: {}\n", col_stats.join(", ")));
        }

        let ccr_id = CcrCache::store(raw_csv);
        out.push_str(&format!(
            "[CCR Summary: CSV contains {} data rows total ({} shown). Use retrieve_observation(id=\"{}\", query=\"...\") for full dataset.]",
            total_rows,
            selected_indices.len(),
            ccr_id
        ));

        if out.len() + 80 >= raw_csv.len() {
            return None;
        }

        Some((out, ccr_id))
    }

    fn count_delimiters(line: &str, delim: char) -> usize {
        let mut in_quotes = false;
        let mut count = 0usize;
        for ch in line.chars() {
            if ch == '"' {
                in_quotes = !in_quotes;
            } else if ch == delim && !in_quotes {
                count += 1;
            }
        }
        count
    }
}

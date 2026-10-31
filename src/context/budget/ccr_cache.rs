use regex::RegexBuilder;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::{OnceLock, RwLock};

/// Pure-Rust FIPS 180-4 SHA-256 implementation for deterministic content addressing (Headroom Invariant I4)
/// with zero external C/crypto crate dependencies.
#[must_use]
pub fn sha256_digest(data: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];

    let bit_len = (data.len() as u64).wrapping_mul(8);
    let mut padded = Vec::with_capacity(data.len() + 72);
    padded.extend_from_slice(data);
    padded.push(0x80);
    while (padded.len() % 64) != 56 {
        padded.push(0x00);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in padded.chunks_exact(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                chunk[i * 4],
                chunk[i * 4 + 1],
                chunk[i * 4 + 2],
                chunk[i * 4 + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let mut a = h[0];
        let mut b = h[1];
        let mut c = h[2];
        let mut d = h[3];
        let mut e = h[4];
        let mut f = h[5];
        let mut g = h[6];
        let mut hh = h[7];

        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }

    let mut out = [0u8; 32];
    for (i, word) in h.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    out
}

const DEFAULT_MAX_CACHE_BYTES: usize = 8 * 1024 * 1024; // 8 MB in-memory LRU
const DEFAULT_MAX_ENTRIES: usize = 256;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedObservation {
    pub id: String,
    pub full_text: String,
    pub byte_size: usize,
    pub line_count: usize,
}

/// Real-time telemetry for Headroom Compress-Cache-Retrieve (CCR) & Live-Zone transforms.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CcrCompressionStats {
    pub total_compressions: usize,
    pub total_bytes_before: usize,
    pub total_bytes_after: usize,
    pub total_tokens_before: usize,
    pub total_tokens_after: usize,
    pub modality_counts: BTreeMap<String, usize>,
}

impl CcrCompressionStats {
    #[must_use]
    pub fn tokens_saved(&self) -> usize {
        self.total_tokens_before
            .saturating_sub(self.total_tokens_after)
    }

    #[must_use]
    pub fn compression_ratio_pct(&self) -> f64 {
        if self.total_tokens_before == 0 {
            0.0
        } else {
            (self.tokens_saved() as f64 / self.total_tokens_before as f64) * 100.0
        }
    }

    /// Estimates monetary cost savings (in USD) for the given model family based on input token pricing
    /// (`aswin402/headroom-mcp` `pricing.rs` parity), resolved via longest-substring specificity matching.
    #[must_use]
    pub fn estimated_usd_saved(&self, model: &str) -> f64 {
        const MODEL_PRICING_PER_MTOK: &[(&str, f64)] = &[
            ("opus", 15.0),
            ("sonnet", 3.0),
            ("haiku", 0.80),
            ("gpt-4o-mini", 0.15),
            ("gpt-4.1-mini", 0.15),
            ("gpt-4.1-nano", 0.10),
            ("gpt-4o", 2.50),
            ("gpt-4.1", 2.00),
            ("o3-mini", 1.10),
            ("o1-mini", 1.10),
            ("gemini-2.5-flash", 0.15),
            ("gemini-2.0-flash", 0.15),
            ("gemini-2.5-pro", 1.25),
            ("gemini-1.5-pro", 1.25),
            ("deepseek-reasoner", 0.55),
            ("deepseek", 0.27),
        ];

        let m = model.to_ascii_lowercase();
        let input_per_mtok = MODEL_PRICING_PER_MTOK
            .iter()
            .filter(|(pattern, _)| m.contains(*pattern))
            .max_by_key(|(pattern, _)| pattern.len())
            .map(|(_, rate)| *rate)
            .unwrap_or(3.0);

        (self.tokens_saved() as f64 / 1_000_000.0) * input_per_mtok
    }
}

#[derive(Debug)]
pub struct CcrCacheInner {
    entries: HashMap<String, CachedObservation>,
    order: VecDeque<String>,
    total_bytes: usize,
    max_bytes: usize,
    max_entries: usize,
    disk_dir: Option<PathBuf>,
    stats: CcrCompressionStats,
}

impl CcrCacheInner {
    pub fn new(max_bytes: usize, max_entries: usize) -> Self {
        Self {
            entries: HashMap::new(),
            order: VecDeque::new(),
            total_bytes: 0,
            max_bytes,
            max_entries,
            disk_dir: None,
            stats: CcrCompressionStats::default(),
        }
    }

    /// Computes a deterministic content-addressed CCR ID (`ccr_<12hex>`) using SHA-256 (Headroom Invariant I4).
    #[must_use]
    pub fn compute_deterministic_id(content: &str) -> String {
        let digest = sha256_digest(content.as_bytes());
        format!(
            "ccr_{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
            digest[0], digest[1], digest[2], digest[3], digest[4], digest[5]
        )
    }

    pub fn insert(&mut self, content: &str) -> String {
        let id = Self::compute_deterministic_id(content);

        if self.entries.contains_key(&id) {
            // Already cached; refresh LRU order
            self.order.retain(|k| k != &id);
            self.order.push_back(id.clone());
            return id;
        }

        let byte_size = content.len();
        let line_count = content.lines().count();

        // Persist to disk spillover if configured
        if let Some(ref dir) = self.disk_dir {
            let _ = std::fs::create_dir_all(dir);
            let _ = std::fs::write(dir.join(format!("{}.cache", id)), content.as_bytes());
        }

        // Evict oldest in-memory entries if capacity exceeded
        while (self.total_bytes + byte_size > self.max_bytes
            || self.entries.len() >= self.max_entries)
            && !self.order.is_empty()
        {
            if let Some(oldest_id) = self.order.pop_front() {
                if let Some(removed) = self.entries.remove(&oldest_id) {
                    self.total_bytes = self.total_bytes.saturating_sub(removed.byte_size);
                }
            }
        }

        self.entries.insert(
            id.clone(),
            CachedObservation {
                id: id.clone(),
                full_text: content.to_string(),
                byte_size,
                line_count,
            },
        );
        self.order.push_back(id.clone());
        self.total_bytes += byte_size;

        id
    }

    pub fn get(&mut self, id: &str) -> Option<&CachedObservation> {
        let clean_id = id
            .trim()
            .trim_start_matches("<<ccr:")
            .trim_end_matches(">>");
        if self.entries.contains_key(clean_id) {
            self.order.retain(|k| k != clean_id);
            self.order.push_back(clean_id.to_string());
            return self.entries.get(clean_id);
        }

        // Check disk spillover if evicted from RAM
        if let Some(ref dir) = self.disk_dir {
            let disk_path = dir.join(format!("{}.cache", clean_id));
            if let Ok(content) = std::fs::read_to_string(&disk_path) {
                let byte_size = content.len();
                let line_count = content.lines().count();
                self.entries.insert(
                    clean_id.to_string(),
                    CachedObservation {
                        id: clean_id.to_string(),
                        full_text: content,
                        byte_size,
                        line_count,
                    },
                );
                self.order.push_back(clean_id.to_string());
                self.total_bytes += byte_size;
                return self.entries.get(clean_id);
            }
        }

        None
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.order.clear();
        self.total_bytes = 0;
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

static GLOBAL_CCR_CACHE: OnceLock<RwLock<CcrCacheInner>> = OnceLock::new();

fn get_cache() -> &'static RwLock<CcrCacheInner> {
    GLOBAL_CCR_CACHE.get_or_init(|| {
        RwLock::new(CcrCacheInner::new(
            DEFAULT_MAX_CACHE_BYTES,
            DEFAULT_MAX_ENTRIES,
        ))
    })
}

pub struct CcrCache;

impl CcrCache {
    /// Configures optional workspace disk spillover directory (`.minicode/ccr`) for persistent recovery.
    pub fn set_disk_dir(dir: PathBuf) {
        let mut cache = get_cache().write().unwrap_or_else(|e| e.into_inner());
        cache.disk_dir = Some(dir);
    }

    /// Stores an observation in the cache and returns its deterministic CCR reference id (`ccr_xxxxxxxxxxxx`).
    pub fn store(content: &str) -> String {
        let mut cache = get_cache().write().unwrap_or_else(|e| e.into_inner());
        cache.insert(content)
    }

    /// Records compression telemetry for Headroom observability.
    pub fn record_compression(
        modality: &str,
        bytes_before: usize,
        bytes_after: usize,
        tokens_before: usize,
        tokens_after: usize,
    ) {
        let mut cache = get_cache().write().unwrap_or_else(|e| e.into_inner());
        cache.stats.total_compressions += 1;
        cache.stats.total_bytes_before += bytes_before;
        cache.stats.total_bytes_after += bytes_after;
        cache.stats.total_tokens_before += tokens_before;
        cache.stats.total_tokens_after += tokens_after;
        *cache
            .stats
            .modality_counts
            .entry(modality.to_string())
            .or_insert(0) += 1;
    }

    /// Returns a snapshot of cumulative Headroom CCR compression telemetry.
    #[must_use]
    pub fn stats() -> CcrCompressionStats {
        let cache = get_cache().read().unwrap_or_else(|e| e.into_inner());
        cache.stats.clone()
    }

    /// Losslessly retrieves an observation by its CCR reference id with optional offset and limit (line-based).
    pub fn retrieve(id: &str, offset: Option<usize>, limit: Option<usize>) -> Option<String> {
        let mut cache = get_cache().write().unwrap_or_else(|e| e.into_inner());
        let obs = cache.get(id)?;
        let full = &obs.full_text;

        if offset.is_none() && limit.is_none() {
            return Some(full.clone());
        }

        let lines: Vec<&str> = full.lines().collect();
        let start = offset.unwrap_or(0).min(lines.len());
        let count = limit.unwrap_or(lines.len().saturating_sub(start));
        let end = (start + count).min(lines.len());

        let sliced = lines[start..end].join("\n");
        Some(sliced)
    }

    /// Searches inside a cached CCR observation by keyword or regex without loading the entire raw
    /// payload into context (Headroom `headroom_retrieve` & `headroom-mcp` `search_cache` parity).
    /// Passing `id = "*"` or `id = "all"` searches across all cached observations in the session.
    pub fn search(
        id: &str,
        query: &str,
        context_lines: usize,
        max_matches: usize,
    ) -> Option<String> {
        let trimmed_id = id.trim();
        if trimmed_id == "*" || trimmed_id.eq_ignore_ascii_case("all") {
            let cache = get_cache().read().unwrap_or_else(|e| e.into_inner());
            if cache.entries.is_empty() {
                return Some("CCR cache is currently empty.".to_string());
            }
            let regex = RegexBuilder::new(query).case_insensitive(true).build().ok();
            let query_lower = query.to_ascii_lowercase();
            let cap = max_matches.clamp(1, 200);
            let mut total_hits = 0usize;
            let mut matched_entries = 0usize;
            let mut out = String::new();

            for entry_id in cache.order.iter().rev() {
                if total_hits >= cap {
                    break;
                }
                let Some(obs) = cache.entries.get(entry_id) else {
                    continue;
                };
                let lines: Vec<&str> = obs.full_text.lines().collect();
                let mut entry_hits = Vec::new();
                for (idx, line) in lines.iter().enumerate() {
                    let is_hit = match &regex {
                        Some(re) => re.is_match(line),
                        None => line.to_ascii_lowercase().contains(&query_lower),
                    };
                    if is_hit {
                        entry_hits.push(idx);
                        total_hits += 1;
                        if total_hits >= cap {
                            break;
                        }
                    }
                }
                if !entry_hits.is_empty() {
                    matched_entries += 1;
                    out.push_str(&format!(
                        "\n## Observation `{}` ({} matches, {} total lines)\n",
                        obs.id,
                        entry_hits.len(),
                        lines.len()
                    ));
                    for &hit_idx in &entry_hits {
                        let start = hit_idx.saturating_sub(context_lines);
                        let end = (hit_idx + context_lines).min(lines.len().saturating_sub(1));
                        for i in start..=end {
                            let marker = if i == hit_idx { ">" } else { " " };
                            out.push_str(&format!("{}{:>5} | {}\n", marker, i + 1, lines[i]));
                        }
                        out.push_str("  ---\n");
                    }
                }
            }

            if total_hits == 0 {
                return Some(format!(
                    "No matches for query `{}` across {} cached CCR observations.",
                    query,
                    cache.entries.len()
                ));
            }
            return Some(format!(
                "# Global CCR Cache Search (`{}`: {} matches across {} observations){}",
                query, total_hits, matched_entries, out
            ));
        }

        let mut cache = get_cache().write().unwrap_or_else(|e| e.into_inner());
        let obs = cache.get(trimmed_id)?;
        let lines: Vec<&str> = obs.full_text.lines().collect();
        if lines.is_empty() {
            return Some(String::new());
        }

        let regex = RegexBuilder::new(query).case_insensitive(true).build().ok();
        let query_lower = query.to_ascii_lowercase();
        let cap = max_matches.clamp(1, 200);

        let mut matched_indices = Vec::new();
        for (idx, line) in lines.iter().enumerate() {
            let is_hit = match &regex {
                Some(re) => re.is_match(line),
                None => line.to_ascii_lowercase().contains(&query_lower),
            };
            if is_hit {
                matched_indices.push(idx);
                if matched_indices.len() >= cap {
                    break;
                }
            }
        }

        if matched_indices.is_empty() {
            return Some(format!(
                "No matches for query `{}` in CCR observation `{}` ({} total lines).",
                query,
                obs.id,
                lines.len()
            ));
        }

        let mut included_lines = std::collections::BTreeSet::new();
        for &hit_idx in &matched_indices {
            let start = hit_idx.saturating_sub(context_lines);
            let end = (hit_idx + context_lines).min(lines.len().saturating_sub(1));
            for i in start..=end {
                included_lines.insert(i);
            }
        }

        let mut out = format!(
            "# CCR Query Search in `{}` ({} matches for `{}` across {} lines)\n",
            obs.id,
            matched_indices.len(),
            query,
            lines.len()
        );
        let mut prev_line: Option<usize> = None;
        for line_idx in included_lines {
            if let Some(prev) = prev_line {
                if line_idx > prev + 1 {
                    out.push_str(&format!(
                        "  ... ({} lines omitted) ...\n",
                        line_idx - prev - 1
                    ));
                }
            }
            let marker = if matched_indices.binary_search(&line_idx).is_ok() {
                ">"
            } else {
                " "
            };
            out.push_str(&format!(
                "{}{:>5} | {}\n",
                marker,
                line_idx + 1,
                lines[line_idx]
            ));
            prev_line = Some(line_idx);
        }

        Some(out)
    }

    /// Returns the number of cached items in the CCR cache.
    pub fn len() -> usize {
        let cache = get_cache().read().unwrap_or_else(|e| e.into_inner());
        cache.len()
    }

    /// Returns the total byte footprint of observations in the CCR cache.
    pub fn total_bytes() -> usize {
        let cache = get_cache().read().unwrap_or_else(|e| e.into_inner());
        cache.total_bytes
    }

    /// Clears the global CCR cache.
    pub fn clear() {
        let mut cache = get_cache().write().unwrap_or_else(|e| e.into_inner());
        cache.clear();
    }
}

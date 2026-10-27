use crate::constants::{
    BM25_B, BM25_K1, SYMBOL_DEF_KIND_BOOST, SYMBOL_EXACT_MATCH_SCORE, SYMBOL_FUNC_KIND_BOOST,
    SYMBOL_PREFIX_MATCH_SCORE, SYMBOL_TEST_PENALTY_FACTOR,
};
use crate::context::repomap::{RepoMapExtractor, SymbolDef};
use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

/// A ranked symbol match from the inverted symbol index
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolMatch {
    pub name: String,
    pub kind: String,
    pub signature: String,
    pub file_path: PathBuf,
    pub line_number: usize,
    pub score: f64,
    pub doc_comment: Option<String>,
    /// Precomputed 8-byte big-endian ASCII-lowercased key prefix (`bbtree` `design/04-inline-key.md` pattern)
    /// enabling 1-instruction register prefix rejection before touching heap strings.
    #[serde(default)]
    pub inline_prefix: u64,
}

const SYMBOL_INDEX_FORMAT_VERSION: u32 = 1;
const SYMBOL_INDEX_CACHE_REL_PATH: &str = ".minicode/cache/symbol_index.json";

/// Persisted disk snapshot of `SymbolIndex` (`bbtree` `design/15-index-file.md` pattern).
#[derive(Debug, Serialize, Deserialize)]
struct SymbolIndexDiskSnapshot {
    version: u32,
    fingerprint: u64,
    symbols: Vec<SymbolMatch>,
    postings: BTreeMap<String, Vec<usize>>,
    doc_lengths: Vec<usize>,
    doc_count: usize,
    avg_doc_len: f64,
}

/// In-memory inverted symbol and identifier index for sub-millisecond code navigation
pub struct SymbolIndex {
    symbols: Vec<SymbolMatch>,
    postings: BTreeMap<String, Vec<usize>>, // token -> Vec<symbol_idx>
    doc_lengths: Vec<usize>,                // token count per symbol
    doc_count: usize,                       // total indexed symbols
    avg_doc_len: f64,                       // average token length per symbol
}

impl Default for SymbolIndex {
    fn default() -> Self {
        Self::new()
    }
}

type CachedSymbolEntry = (
    PathBuf,
    std::time::Instant,
    u64,
    std::sync::Arc<SymbolIndex>,
);
static GLOBAL_SYMBOL_INDEX_CACHE: std::sync::OnceLock<
    std::sync::RwLock<Option<CachedSymbolEntry>>,
> = std::sync::OnceLock::new();

impl SymbolIndex {
    pub fn new() -> Self {
        Self {
            symbols: Vec::new(),
            postings: BTreeMap::new(),
            doc_lengths: Vec::new(),
            doc_count: 0,
            avg_doc_len: 1.0,
        }
    }

    /// Computes a big-endian 8-byte ASCII-lowercased inline key prefix (inspired by `bbtree` `design/04-inline-key.md`).
    /// Lexicographical byte order on the first 8 bytes is preserved under unsigned 64-bit integer comparison.
    #[inline]
    pub fn compute_inline_prefix(s: &str) -> u64 {
        let mut buf = [0u8; 8];
        let bytes = s.as_bytes();
        let len = bytes.len().min(8);
        let mut i = 0;
        while i < len {
            buf[i] = bytes[i].to_ascii_lowercase();
            i += 1;
        }
        u64::from_be_bytes(buf)
    }

    /// Computes `(masked_prefix, bitmask)` for 1-instruction `u64` prefix matching of `query`.
    #[inline]
    pub fn compute_prefix_mask(query: &str) -> (u64, u64) {
        let prefix = Self::compute_inline_prefix(query);
        let len = query.len().min(8);
        let mask = if len == 0 {
            0u64
        } else if len >= 8 {
            !0u64
        } else {
            !0u64 << ((8 - len) * 8)
        };
        (prefix & mask, mask)
    }

    /// Computes a deterministic 64-bit FNV-1a fingerprint over workspace source file paths, sizes, and mtimes.
    pub fn compute_workspace_fingerprint(workspace_root: &Path, files: &[PathBuf]) -> u64 {
        const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
        const FNV_PRIME: u64 = 0x0100_0000_01b3;

        let mut hash = FNV_OFFSET;
        let mix_u64 = |h: &mut u64, val: u64| {
            for b in val.to_le_bytes() {
                *h ^= u64::from(b);
                *h = h.wrapping_mul(FNV_PRIME);
            }
        };

        mix_u64(&mut hash, u64::from(SYMBOL_INDEX_FORMAT_VERSION));
        mix_u64(&mut hash, files.len() as u64);

        for path in files {
            let rel = path.strip_prefix(workspace_root).unwrap_or(path);
            for &b in rel.as_os_str().as_encoded_bytes() {
                hash ^= u64::from(b);
                hash = hash.wrapping_mul(FNV_PRIME);
            }
            if let Ok(meta) = std::fs::metadata(path) {
                mix_u64(&mut hash, meta.len());
                if let Ok(modified) = meta.modified() {
                    if let Ok(dur) = modified.duration_since(std::time::UNIX_EPOCH) {
                        mix_u64(&mut hash, dur.as_secs());
                        mix_u64(&mut hash, u64::from(dur.subsec_nanos()));
                    }
                }
            }
        }

        hash
    }

    /// Loads a persisted `SymbolIndex` snapshot from disk if its format version and workspace fingerprint match.
    pub fn load_from_disk(cache_path: &Path, expected_fingerprint: u64) -> Option<Self> {
        let bytes = std::fs::read(cache_path).ok()?;
        let mut snap: SymbolIndexDiskSnapshot = serde_json::from_slice(&bytes).ok()?;
        if snap.version != SYMBOL_INDEX_FORMAT_VERSION || snap.fingerprint != expected_fingerprint {
            return None;
        }
        for sym in &mut snap.symbols {
            if sym.inline_prefix == 0 && !sym.name.is_empty() {
                sym.inline_prefix = Self::compute_inline_prefix(&sym.name);
            }
        }
        Some(Self {
            symbols: snap.symbols,
            postings: snap.postings,
            doc_lengths: snap.doc_lengths,
            doc_count: snap.doc_count,
            avg_doc_len: snap.avg_doc_len,
        })
    }

    /// Atomically saves the current `SymbolIndex` snapshot to `cache_path` with `fingerprint`.
    pub fn save_to_disk(&self, cache_path: &Path, fingerprint: u64) -> Result<()> {
        if let Some(parent) = cache_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let snap = SymbolIndexDiskSnapshot {
            version: SYMBOL_INDEX_FORMAT_VERSION,
            fingerprint,
            symbols: self.symbols.clone(),
            postings: self.postings.clone(),
            doc_lengths: self.doc_lengths.clone(),
            doc_count: self.doc_count,
            avg_doc_len: self.avg_doc_len,
        };
        let bytes = serde_json::to_vec(&snap)?;
        let tmp_path = cache_path.with_extension(format!(
            "tmp.{}.{}",
            std::process::id(),
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::write(&tmp_path, &bytes)?;
        if let Err(e) = std::fs::rename(&tmp_path, cache_path) {
            let _ = std::fs::remove_file(&tmp_path);
            return Err(e.into());
        }
        Ok(())
    }

    /// Returns a shared `Arc<SymbolIndex>` for `workspace_root`, reusing the in-memory AST index
    /// across the Orchestrator and parallel `MiniPub` workers (TTL: 30s or fingerprint-verified)
    /// and falling back to the compacted disk index cache (`.minicode/cache/symbol_index.json`).
    pub fn get_or_build_cached(workspace_root: &Path) -> Result<std::sync::Arc<Self>> {
        let cache = GLOBAL_SYMBOL_INDEX_CACHE.get_or_init(|| std::sync::RwLock::new(None));
        if let Ok(guard) = cache.read() {
            if let Some((ref cached_root, built_at, _fp, ref arc_idx)) = *guard {
                if cached_root == workspace_root
                    && built_at.elapsed() < std::time::Duration::from_secs(30)
                {
                    return Ok(std::sync::Arc::clone(arc_idx));
                }
            }
        }

        let files = crate::context::walker::WorkspaceWalker::new(workspace_root)
            .extensions(crate::constants::SUPPORTED_LANG_EXTENSIONS)
            .collect_absolute_files();
        let fingerprint = Self::compute_workspace_fingerprint(workspace_root, &files);

        // If in-memory cache expired its 30s TTL but the workspace fingerprint is unchanged, reuse it!
        if let Ok(mut guard) = cache.write() {
            if let Some((ref cached_root, ref mut built_at, cached_fp, ref arc_idx)) = *guard {
                if cached_root == workspace_root && cached_fp == fingerprint {
                    *built_at = std::time::Instant::now();
                    return Ok(std::sync::Arc::clone(arc_idx));
                }
            }
        }

        let disk_cache_path = workspace_root.join(SYMBOL_INDEX_CACHE_REL_PATH);
        let fresh = if let Some(loaded) = Self::load_from_disk(&disk_cache_path, fingerprint) {
            loaded
        } else {
            let mut built = Self::new();
            built.build_index_from_files(&files)?;
            // Persist disk cache if `.minicode` exists in the workspace
            if workspace_root.join(".minicode").is_dir() {
                let _ = built.save_to_disk(&disk_cache_path, fingerprint);
            }
            built
        };

        let arc_fresh = std::sync::Arc::new(fresh);
        if let Ok(mut guard) = cache.write() {
            *guard = Some((
                workspace_root.to_path_buf(),
                std::time::Instant::now(),
                fingerprint,
                std::sync::Arc::clone(&arc_fresh),
            ));
        }
        Ok(arc_fresh)
    }

    /// Invalidates the shared in-memory `SymbolIndex` and `TrigramIndex` caches when workspace source files are mutated.
    pub fn invalidate_cache() {
        if let Some(cache) = GLOBAL_SYMBOL_INDEX_CACHE.get() {
            if let Ok(mut guard) = cache.write() {
                *guard = None;
            }
        }
        crate::context::search::trigram::TrigramIndex::invalidate_cache();
    }

    /// Splits an identifier (`camelCase`, `PascalCase`, `HTMLParser`, `snake_case`, `SCREAMING_SNAKE`) into searchable tokens
    pub fn tokenize(text: &str) -> Vec<String> {
        let mut tokens = HashSet::new();
        let lower_full = text.to_lowercase();
        tokens.insert(lower_full.clone());

        // Split by non-alphanumeric (underscores, dots, dashes, colons)
        for part in text
            .split(|c: char| !c.is_alphanumeric())
            .filter(|p| !p.is_empty())
        {
            tokens.insert(part.to_lowercase());

            // CamelCase & acronym split (e.g. HTMLParser -> html + parser)
            let chars: Vec<char> = part.chars().collect();
            let mut current = String::new();
            for i in 0..chars.len() {
                let c = chars[i];
                if i > 0 && c.is_uppercase() && !current.is_empty() {
                    let prev_upper = chars[i - 1].is_uppercase();
                    let next_lower = chars.get(i + 1).is_some_and(|nc| nc.is_lowercase());
                    if !prev_upper || next_lower {
                        tokens.insert(current.to_lowercase());
                        current.clear();
                    }
                }
                current.push(c);
            }
            if !current.is_empty() {
                tokens.insert(current.to_lowercase());
            }
        }

        // Keep tokens >= 2 chars, or single char if it matches the full identifier exactly
        tokens
            .into_iter()
            .filter(|t| t.len() >= 2 || t == &lower_full)
            .collect()
    }

    /// Builds the symbol index by scanning the workspace and extracting AST symbols
    pub fn build_index(&mut self, workspace_root: &Path) -> Result<()> {
        let files = crate::context::walker::WorkspaceWalker::new(workspace_root)
            .extensions(crate::constants::SUPPORTED_LANG_EXTENSIONS)
            .collect_absolute_files();
        self.build_index_from_files(&files)
    }

    /// Builds the symbol index from a pre-collected list of absolute file paths
    pub fn build_index_from_files(&mut self, files: &[PathBuf]) -> Result<()> {
        self.symbols.clear();
        self.postings.clear();
        self.doc_lengths.clear();
        self.doc_count = 0;
        self.avg_doc_len = 1.0;

        let mut extractor = RepoMapExtractor::new();

        for path in files {
            if let Ok(file_symbols) = extractor.extract_file_symbols(path) {
                for sym in file_symbols {
                    if sym.kind != "import" {
                        self.add_symbol(path.clone(), sym);
                    }
                }
            }
        }

        self.recompute_corpus_stats();
        Ok(())
    }

    /// Recomputes global corpus statistics (doc count & average doc length)
    fn recompute_corpus_stats(&mut self) {
        self.doc_count = self.symbols.len();
        if self.doc_count > 0 {
            let total_len: usize = self.doc_lengths.iter().sum();
            self.avg_doc_len = total_len as f64 / self.doc_count as f64;
        } else {
            self.avg_doc_len = 1.0;
        }
    }

    /// Adds a single symbol entry to the index
    pub fn add_symbol(&mut self, file_path: PathBuf, sym: SymbolDef) {
        let sym_idx = self.symbols.len();
        let tokens = Self::tokenize(&sym.name);
        let doc_len = tokens.len().max(1);
        let prev_total = self.avg_doc_len * (self.doc_count as f64);
        self.doc_lengths.push(doc_len);

        for token in &tokens {
            self.postings
                .entry(token.clone())
                .or_default()
                .push(sym_idx);
        }

        let inline_prefix = Self::compute_inline_prefix(&sym.name);
        self.symbols.push(SymbolMatch {
            name: sym.name,
            kind: sym.kind,
            signature: sym.signature,
            file_path,
            line_number: sym.line_number,
            score: 0.0,
            doc_comment: sym.doc_comment,
            inline_prefix,
        });

        self.doc_count = self.symbols.len();
        self.avg_doc_len = (prev_total + doc_len as f64) / (self.doc_count as f64);
    }

    /// Locates an exact or prefix match for a symbol name using $O(\log N + K)$ B-Tree prefix range scan
    /// and 8-byte inline key prefix filtering (`bbtree` `design/04-inline-key.md` pattern).
    pub fn locate_symbol(&self, name: &str) -> Vec<SymbolMatch> {
        let query_lower = name.trim().to_lowercase();
        if query_lower.is_empty() {
            return Vec::new();
        }

        let (query_prefix, query_mask) = Self::compute_prefix_mask(&query_lower);
        let mut candidate_indices: Vec<usize> = Vec::new();

        for (_token, postings) in self
            .postings
            .range(query_lower.clone()..)
            .take_while(|(token, _)| token.starts_with(&query_lower))
        {
            candidate_indices.extend_from_slice(postings);
        }

        candidate_indices.sort_unstable();
        candidate_indices.dedup();

        let query_is_ascii = query_lower.is_ascii();
        let query_len = query_lower.len();
        let mut results = Vec::new();

        for sym_idx in candidate_indices {
            let Some(sym) = self.symbols.get(sym_idx) else {
                continue;
            };

            // 1-instruction 8-byte inline key prefix fast rejection (rejects interior sub-token hits without heap access)
            if (sym.inline_prefix & query_mask) != query_prefix {
                continue;
            }

            let base_score = if query_is_ascii && sym.name.is_ascii() {
                let sym_len = sym.name.len();
                if sym_len < query_len {
                    None
                } else if query_len <= 8 {
                    // All `query_len` bytes are already verified by the 8-byte `inline_prefix` check above
                    if sym_len == query_len {
                        Some(SYMBOL_EXACT_MATCH_SCORE)
                    } else {
                        Some(SYMBOL_PREFIX_MATCH_SCORE)
                    }
                } else if sym.name.as_bytes()[8..query_len]
                    .eq_ignore_ascii_case(&query_lower.as_bytes()[8..])
                {
                    if sym_len == query_len {
                        Some(SYMBOL_EXACT_MATCH_SCORE)
                    } else {
                        Some(SYMBOL_PREFIX_MATCH_SCORE)
                    }
                } else {
                    None
                }
            } else {
                let sym_name_lower = sym.name.to_lowercase();
                if sym_name_lower == query_lower {
                    Some(SYMBOL_EXACT_MATCH_SCORE)
                } else if sym_name_lower.starts_with(&query_lower) {
                    Some(SYMBOL_PREFIX_MATCH_SCORE)
                } else {
                    None
                }
            };

            if let Some(mut score) = base_score {
                let path_str = sym.file_path.to_string_lossy();
                let is_test = path_str
                    .as_bytes()
                    .windows(4)
                    .any(|w| w.eq_ignore_ascii_case(b"test") || w.eq_ignore_ascii_case(b"mock"));
                if is_test {
                    score *= SYMBOL_TEST_PENALTY_FACTOR;
                }
                let mut matched = sym.clone();
                matched.score = score;
                results.push(matched);
            }
        }

        results.sort_by(|a, b| b.score.total_cmp(&a.score));
        results
    }

    /// Searches the symbol index using BM25 ranking, kind boosts, and test file penalties
    pub fn search_symbols(&self, query: &str, limit: usize) -> Vec<SymbolMatch> {
        let query_tokens = Self::tokenize(query);
        if query_tokens.is_empty() || self.doc_count == 0 {
            return Vec::new();
        }

        let mut scores: HashMap<usize, f64> = HashMap::new();
        let n = self.doc_count as f64;
        let avg_dl = if self.avg_doc_len > 0.0 {
            self.avg_doc_len
        } else {
            1.0
        };

        for q_token in &query_tokens {
            let mut matched_symbols = HashMap::new();

            if let Some(postings) = self.postings.get(q_token) {
                for &sym_idx in postings {
                    matched_symbols.insert(sym_idx, 1.0);
                }
            } else {
                // O(log N + K) BTreeMap prefix range search
                for (_vocab_token, postings) in self
                    .postings
                    .range(q_token.clone()..)
                    .take_while(|(vocab_token, _)| vocab_token.starts_with(q_token))
                {
                    for &sym_idx in postings {
                        matched_symbols
                            .entry(sym_idx)
                            .or_insert(crate::constants::BM25_PREFIX_WEIGHT);
                    }
                }
            }

            if !matched_symbols.is_empty() {
                let df = (matched_symbols.len() as f64).max(1.0);
                // Standard Robertson-Spärck Jones IDF with smoothing
                let idf = (((n - df + 0.5) / (df + 0.5)) + 1.0).ln().max(0.1);

                for (sym_idx, weight) in matched_symbols {
                    let dl = self.doc_lengths.get(sym_idx).copied().unwrap_or(1) as f64;
                    let tf = 1.0 * weight;
                    let bm25 = idf * (tf * (BM25_K1 + 1.0))
                        / (tf + BM25_K1 * (1.0 - BM25_B + BM25_B * (dl / avg_dl)));

                    *scores.entry(sym_idx).or_insert(0.0) += bm25;
                }
            }
        }

        let mut ranked = Vec::new();
        for (sym_idx, base_score) in scores {
            if let Some(sym) = self.symbols.get(sym_idx) {
                let mut score = base_score;

                // Kind boosts
                match sym.kind.as_str() {
                    "struct" | "class" | "interface" | "trait" | "enum" => {
                        score += SYMBOL_DEF_KIND_BOOST;
                    }
                    "function" => {
                        score += SYMBOL_FUNC_KIND_BOOST;
                    }
                    _ => {}
                }

                // Test file down-ranking
                let path_str = sym.file_path.to_string_lossy();
                let is_test = path_str
                    .as_bytes()
                    .windows(4)
                    .any(|w| w.eq_ignore_ascii_case(b"test") || w.eq_ignore_ascii_case(b"mock"));
                if is_test {
                    score *= SYMBOL_TEST_PENALTY_FACTOR;
                }

                let mut matched = sym.clone();
                matched.score = score;
                ranked.push(matched);
            }
        }

        ranked.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        ranked.truncate(limit);
        ranked
    }

    /// Formats search results into a clean markdown block for the agent
    pub fn format_matches(&self, matches: &[SymbolMatch], workspace_root: &Path) -> String {
        if matches.is_empty() {
            return "No matching symbols found.".to_string();
        }

        let mut lines = Vec::new();
        lines.push(format!("Found {} symbol declaration(s):", matches.len()));

        for m in matches {
            let rel_path = m
                .file_path
                .strip_prefix(workspace_root)
                .unwrap_or(&m.file_path)
                .display();

            lines.push(format!(
                "- `{}` ({}) at `{}:{}`",
                m.name, m.kind, rel_path, m.line_number
            ));
            lines.push(format!("  Signature: `{}`", m.signature));
            if let Some(ref doc) = m.doc_comment {
                lines.push(format!("  Doc: *{}*", doc));
            }
        }

        lines.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenize_identifiers() {
        let tokens = SymbolIndex::tokenize("executeTurn");
        assert!(tokens.contains(&"execute".to_string()));
        assert!(tokens.contains(&"turn".to_string()));

        let tokens_snake = SymbolIndex::tokenize("user_profile_handler");
        assert!(tokens_snake.contains(&"user".to_string()));
        assert!(tokens_snake.contains(&"profile".to_string()));
        assert!(tokens_snake.contains(&"handler".to_string()));

        // Single character identifier preserved
        let tokens_single = SymbolIndex::tokenize("x");
        assert!(tokens_single.contains(&"x".to_string()));
    }

    #[test]
    fn test_symbol_index_lookup_and_ranking() {
        let mut index = SymbolIndex::new();

        index.add_symbol(
            PathBuf::from("src/auth.rs"),
            SymbolDef {
                name: "verify_token".to_string(),
                kind: "function".to_string(),
                signature: "pub fn verify_token(token: &str) -> bool".to_string(),
                line_number: 12,
                end_line: 18,
                doc_comment: Some("Verifies JWT token".to_string()),
            },
        );

        index.add_symbol(
            PathBuf::from("tests/auth_test.rs"),
            SymbolDef {
                name: "test_verify_token".to_string(),
                kind: "function".to_string(),
                signature: "fn test_verify_token()".to_string(),
                line_number: 5,
                end_line: 10,
                doc_comment: None,
            },
        );

        let exact = index.locate_symbol("verify_token");
        assert_eq!(exact.len(), 1);
        assert_eq!(exact[0].file_path, PathBuf::from("src/auth.rs"));

        let search = index.search_symbols("verify token", 10);
        assert!(!search.is_empty());
        // Production file ranked higher than test file due to down-ranking
        assert_eq!(search[0].file_path, PathBuf::from("src/auth.rs"));
    }

    #[test]
    fn test_inline_prefix_and_disk_snapshot_roundtrip() {
        // 1. Lexicographical ordering preserved by big-endian 8-byte inline key prefix
        let p_alpha = SymbolIndex::compute_inline_prefix("AlphaNode");
        let p_beta = SymbolIndex::compute_inline_prefix("BetaNode");
        assert!(p_alpha < p_beta);

        // 2. Prefix mask filters interior sub-token hits in 1 register operation
        let mut index = SymbolIndex::new();
        index.add_symbol(
            PathBuf::from("src/tree.rs"),
            SymbolDef {
                name: "BTreeNodeHeader".to_string(),
                kind: "struct".to_string(),
                signature: "pub struct BTreeNodeHeader".to_string(),
                line_number: 10,
                end_line: 25,
                doc_comment: None,
            },
        );
        index.add_symbol(
            PathBuf::from("src/tree.rs"),
            SymbolDef {
                name: "BTreeNode".to_string(),
                kind: "struct".to_string(),
                signature: "pub struct BTreeNode".to_string(),
                line_number: 30,
                end_line: 60,
                doc_comment: None,
            },
        );
        index.add_symbol(
            PathBuf::from("src/leaf.rs"),
            SymbolDef {
                name: "LeafBTreeNode".to_string(),
                kind: "struct".to_string(),
                signature: "pub struct LeafBTreeNode".to_string(),
                line_number: 1,
                end_line: 15,
                doc_comment: None,
            },
        );

        // Exact match ("BTreeNode") ranks above prefix match ("BTreeNodeHeader"), and "LeafBTreeNode" is excluded
        let located = index.locate_symbol("BTreeNode");
        assert_eq!(located.len(), 2);
        assert_eq!(located[0].name, "BTreeNode");
        assert_eq!(located[1].name, "BTreeNodeHeader");

        // 3. Disk snapshot save & load with fingerprint verification
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let cache_path = temp_dir.path().join(".minicode/cache/symbol_index.json");
        let fp = 0xDEAD_BEEF_CAFE_BABEu64;
        index.save_to_disk(&cache_path, fp).expect("save snapshot");

        // Mismatched fingerprint returns None
        assert!(SymbolIndex::load_from_disk(&cache_path, fp ^ 1).is_none());

        // Matching fingerprint restores full index and inline prefixes
        let loaded = SymbolIndex::load_from_disk(&cache_path, fp).expect("load snapshot");
        let loaded_located = loaded.locate_symbol("btreenode");
        assert_eq!(loaded_located.len(), 2);
        assert_eq!(loaded_located[0].name, "BTreeNode");
    }

    #[test]
    fn test_bm25_idf_discrimination() {
        let mut index = SymbolIndex::new();

        // 3 symbols share "handler", but only 1 has "crypto"
        for i in 1..=3 {
            index.add_symbol(
                PathBuf::from(format!("src/handler_{}.rs", i)),
                SymbolDef {
                    name: format!("request_handler_{}", i),
                    kind: "function".to_string(),
                    signature: "fn handler()".to_string(),
                    line_number: 1,
                    end_line: 5,
                    doc_comment: None,
                },
            );
        }

        index.add_symbol(
            PathBuf::from("src/crypto.rs"),
            SymbolDef {
                name: "crypto_handler".to_string(),
                kind: "struct".to_string(),
                signature: "struct CryptoHandler;".to_string(),
                line_number: 10,
                end_line: 15,
                doc_comment: None,
            },
        );

        // Searching "crypto handler" should rank crypto_handler first because "crypto" has much higher IDF
        let results = index.search_symbols("crypto handler", 5);
        assert!(!results.is_empty());
        assert_eq!(results[0].name, "crypto_handler");
    }
}

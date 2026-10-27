//! Trigram-indexed code search engine inspired by `microsoft/tgrep`.
//!
//! Key architectural invariants ported from `microsoft/tgrep`:
//! 1. **24-Bit Injective Trigram Packing (`TrigramHash = u32`)**:
//!    Packs every overlapping 3-byte ASCII-lowercased window into `(a << 16) | (b << 8) | c`
//!    with a 1-instruction multiply-xorshift [`TrigramHasher`] (`0x9E37_79B9_7F4A_7C15`).
//! 2. **2-Byte Positional & Next-Byte Bloom Masks ([`TrigramMasks`])**:
//!    Each `(trigram, file_id)` posting carries:
//!    - `loc_mask: u8` (`1 << (offset % 8)`): consecutive trigrams must satisfy
//!      `(prev_loc.rotate_left(1) & next_loc) != 0`.
//!    - `next_mask: u8` (`1 << ((next_byte * 0x9E >> 5) & 7)`): 8-bit Bloom filter of the
//!      byte immediately following the trigram at `offset + 3`.
//! 3. **Literal & Regex [`QueryPlan`] Decomposition**:
//!    Extracts required trigram runs + `expected_next` bytes from fixed strings and regular
//!    expressions (concatenations and top-level alternations), intersecting posting lists
//!    **smallest-first** to collapse candidate files before touching disk.
//! 4. **Shared File Table & Compacted Binary Disk Cache (`.minicode/cache/trigram_index.bin`)**:
//!    Serves both `grep_search` candidate narrowing and `file_search` path queries from memory,
//!    backed by an FNV-1a workspace fingerprint snapshot.

use crate::error::Result;
use ignore::WalkBuilder;
use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock, RwLock};
use std::time::{Duration, Instant, UNIX_EPOCH};

/// Packed 24-bit ASCII-lowercased 3-byte trigram: `(a << 16) | (b << 8) | c`.
pub type TrigramHash = u32;

const TRIGRAM_INDEX_MAGIC: &[u8; 8] = b"TGRPIDX1";
const TRIGRAM_INDEX_VERSION: u32 = 1;
const TRIGRAM_CACHE_REL_PATH: &str = ".minicode/cache/trigram_index.bin";
/// Maximum file size indexed into trigram postings (4 MiB). Larger text files are tracked
/// in `Passthrough` mode so they are still scanned without bloating posting lists.
const MAX_INDEXED_FILE_BYTES: u64 = 4 * 1024 * 1024;
/// Number of leading bytes inspected for NUL (`\0`) to classify binary files.
const BINARY_SNIFF_LEN: usize = 8192;

/// Fast multiply-xorshift hasher for 24-bit injective [`TrigramHash`] keys (`microsoft/tgrep` pattern).
///
/// Because a 24-bit trigram key is already injective, SipHash is unnecessary overhead on the
/// per-byte indexing hot path. Folding the upper 32 bits down mixes all 24 bits into both
/// hashbrown's low bucket index bits and top-7 H2 control bits.
#[derive(Default, Clone, Copy)]
pub struct TrigramHasher(u64);

impl std::hash::Hasher for TrigramHasher {
    #[inline]
    fn finish(&self) -> u64 {
        self.0
    }

    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.0 = (self.0 ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3);
        }
    }

    #[inline]
    fn write_u32(&mut self, value: u32) {
        let mixed = u64::from(value).wrapping_mul(0x9E37_79B9_7F4A_7C15);
        self.0 = mixed ^ (mixed >> 32);
    }
}

/// [`std::hash::BuildHasher`] for [`TrigramHasher`].
pub type BuildTrigramHasher = std::hash::BuildHasherDefault<TrigramHasher>;

/// Per-file trigram mask map using [`TrigramHasher`].
pub type TrigramMaskMap = HashMap<TrigramHash, TrigramMasks, BuildTrigramHasher>;

/// Packs 3 bytes (folded to ASCII lowercase) into a 24-bit [`TrigramHash`].
#[inline]
pub fn pack_trigram(a: u8, b: u8, c: u8) -> TrigramHash {
    (u32::from(a.to_ascii_lowercase()) << 16)
        | (u32::from(b.to_ascii_lowercase()) << 8)
        | u32::from(c.to_ascii_lowercase())
}

/// Maps a byte offset into a bit position in an 8-bit `loc_mask` (`offset % 8`).
/// Consecutive offsets map to adjacent bits, enabling `rotate_left(1) & next_loc` adjacency checks.
#[inline]
pub fn loc_bit(offset: usize) -> u8 {
    1u8 << (offset & 7)
}

/// Maps an ASCII-lowercased byte to one of 8 Bloom filter bits for `next_mask`.
#[inline]
pub fn bloom_hash(byte: u8) -> u8 {
    let b = byte.to_ascii_lowercase();
    1u8 << ((b.wrapping_mul(0x9E) >> 5) & 0x07)
}

/// Per-trigram positional and next-byte Bloom masks for a single file.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TrigramMasks {
    /// Bit `i` is set if the trigram occurs at a byte offset where `offset % 8 == i`.
    pub loc_mask: u8,
    /// 8-bit Bloom filter of bytes that immediately follow this trigram (`offset + 3`).
    pub next_mask: u8,
}

/// A compact 6-byte posting entry in a trigram's sorted posting list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PostingEntry {
    pub file_id: u32,
    pub loc_mask: u8,
    pub next_mask: u8,
}

/// A single trigram constraint extracted from a query literal run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrigramQuery {
    pub hash: TrigramHash,
    /// Expected next byte (`offset + 3`) in the literal run for `next_mask` Bloom filtering.
    pub expected_next: Option<u8>,
}

/// Boolean query plan decomposed from a literal or regular expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueryPlan {
    /// All trigram queries and literal runs must match in the candidate file.
    And {
        queries: Vec<TrigramQuery>,
        /// Contiguous literal trigram chains (`Vec<TrigramHash>`) used for `loc_mask` rotate-left adjacency verification.
        literal_runs: Vec<Vec<TrigramHash>>,
    },
    /// Any branch can match (union of candidate file IDs).
    Or(Vec<QueryPlan>),
    /// Query has no extractable 3-byte trigrams; caller must scan all searchable text files.
    MatchAll,
}

impl QueryPlan {
    #[inline]
    pub fn is_match_all(&self) -> bool {
        matches!(self, Self::MatchAll)
    }
}

/// Checks whether a sequence of consecutive trigrams from a literal run can appear adjacently
/// in a file based on their `loc_mask` bits (`prev_loc.rotate_left(1) & next_loc != 0`).
#[inline]
#[allow(dead_code)]
pub fn check_adjacency(masks: &[TrigramMasks]) -> bool {
    if masks.len() <= 1 {
        return true;
    }
    for pair in masks.windows(2) {
        if (pair[0].loc_mask.rotate_left(1) & pair[1].loc_mask) == 0 {
            return false;
        }
    }
    true
}

/// Extracts all unique ASCII-lowercased trigrams and their merged `(loc_mask, next_mask)` from `content`.
/// Returns `None` if `content` appears to be binary (contains a NUL byte in the first 8 KiB).
pub fn extract_file_trigrams(content: &[u8]) -> Option<TrigramMaskMap> {
    let sniff_len = content.len().min(BINARY_SNIFF_LEN);
    if content[..sniff_len].contains(&0) {
        return None;
    }

    let mut per_tri = TrigramMaskMap::default();
    if content.len() < 3 {
        return Some(per_tri);
    }

    for (i, window) in content.windows(3).enumerate() {
        let h = pack_trigram(window[0], window[1], window[2]);
        let entry = per_tri.entry(h).or_default();
        entry.loc_mask |= loc_bit(i);
        if i + 3 < content.len() {
            entry.next_mask |= bloom_hash(content[i + 3]);
        }
    }

    Some(per_tri)
}

/// Converts a contiguous literal byte slice into a [`QueryPlan`].
pub fn literals_to_query_plan(bytes: &[u8]) -> QueryPlan {
    if bytes.len() < 3 {
        return QueryPlan::MatchAll;
    }
    let mut queries = Vec::with_capacity(bytes.len() - 2);
    let mut run = Vec::with_capacity(bytes.len() - 2);

    for i in 0..bytes.len() - 2 {
        let hash = pack_trigram(bytes[i], bytes[i + 1], bytes[i + 2]);
        let expected_next = if i + 3 < bytes.len() {
            Some(bytes[i + 3].to_ascii_lowercase())
        } else {
            None
        };
        queries.push(TrigramQuery {
            hash,
            expected_next,
        });
        run.push(hash);
    }

    simplify_plan(QueryPlan::And {
        queries,
        literal_runs: if run.len() >= 2 {
            vec![run]
        } else {
            Vec::new()
        },
    })
}

/// Builds a [`QueryPlan`] from either a fixed literal string or a regular expression pattern.
pub fn build_query_plan(pattern: &str, is_regex: bool) -> QueryPlan {
    if pattern.len() < 3 {
        return QueryPlan::MatchAll;
    }
    if !is_regex {
        return literals_to_query_plan(pattern.as_bytes());
    }
    simplify_plan(decompose_regex_pattern(pattern))
}

/// Decomposes a regex pattern into top-level alternations and required literal runs.
/// Conservative by design: any construct whose literal presence is optional (`?`, `*`, `{0,...}`,
/// character classes, lookarounds) breaks the current literal run and never produces false negatives.
fn decompose_regex_pattern(pattern: &str) -> QueryPlan {
    // Strip leading inline flags like `(?i)`, `(?m)`, `(?s)`
    let mut pat = pattern;
    while let Some(rest) = pat.strip_prefix("(?") {
        if let Some(close) = rest.find(')') {
            let flags = &rest[..close];
            if !flags.is_empty()
                && flags
                    .chars()
                    .all(|c| matches!(c, 'i' | 'm' | 's' | 'U' | 'u' | '-'))
            {
                pat = &rest[close + 1..];
                continue;
            }
        }
        break;
    }

    let branches = split_top_level_alternations(pat);
    if branches.len() > 1 {
        let mut plans = Vec::with_capacity(branches.len());
        for branch in branches {
            let sub = decompose_regex_branch(branch);
            if sub.is_match_all() {
                return QueryPlan::MatchAll;
            }
            plans.push(sub);
        }
        return QueryPlan::Or(plans);
    }

    decompose_regex_branch(pat)
}

/// Splits `pattern` on top-level `|` tokens (outside `(...)` and `[...]`).
fn split_top_level_alternations(pattern: &str) -> Vec<&str> {
    let mut branches = Vec::new();
    let mut paren_depth = 0usize;
    let mut in_class = false;
    let mut start = 0usize;
    let bytes = pattern.as_bytes();
    let mut i = 0usize;

    while i < bytes.len() {
        match bytes[i] {
            b'\\' => {
                i += 2;
                continue;
            }
            b'[' if !in_class => {
                in_class = true;
            }
            b']' if in_class => {
                in_class = false;
            }
            b'(' if !in_class => {
                paren_depth += 1;
            }
            b')' if !in_class => {
                paren_depth = paren_depth.saturating_sub(1);
            }
            b'|' if !in_class && paren_depth == 0 => {
                branches.push(&pattern[start..i]);
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    branches.push(&pattern[start..]);
    branches
}

/// Extracts required literal runs from a single regex branch (no top-level `|`).
fn decompose_regex_branch(branch: &str) -> QueryPlan {
    let mut all_queries = Vec::new();
    let mut literal_runs = Vec::new();
    let mut current_run: Vec<u8> = Vec::new();
    let mut or_branches_opt: Option<Vec<QueryPlan>> = None;

    let flush_run =
        |run: &mut Vec<u8>, queries: &mut Vec<TrigramQuery>, runs: &mut Vec<Vec<TrigramHash>>| {
            if run.len() >= 3 {
                let mut hashes = Vec::with_capacity(run.len() - 2);
                for idx in 0..run.len() - 2 {
                    let h = pack_trigram(run[idx], run[idx + 1], run[idx + 2]);
                    let expected_next = if idx + 3 < run.len() {
                        Some(run[idx + 3].to_ascii_lowercase())
                    } else {
                        None
                    };
                    queries.push(TrigramQuery {
                        hash: h,
                        expected_next,
                    });
                    hashes.push(h);
                }
                if hashes.len() >= 2 {
                    runs.push(hashes);
                }
            }
            run.clear();
        };

    let chars: Vec<char> = branch.chars().collect();
    let mut i = 0usize;

    while i < chars.len() {
        let c = chars[i];
        match c {
            '\\' => {
                if i + 1 < chars.len() {
                    let next_c = chars[i + 1];
                    // Meta-escapes that represent character classes or boundaries break the literal run
                    if matches!(
                        next_c,
                        'd' | 'D'
                            | 'w'
                            | 'W'
                            | 's'
                            | 'S'
                            | 'b'
                            | 'B'
                            | 'A'
                            | 'z'
                            | 'Z'
                            | 'p'
                            | 'P'
                            | 'x'
                            | 'u'
                            | 'U'
                            | '0'..='9'
                    ) {
                        flush_run(&mut current_run, &mut all_queries, &mut literal_runs);
                    } else {
                        let mapped = match next_c {
                            'n' => Some('\n'),
                            'r' => Some('\r'),
                            't' => Some('\t'),
                            other => Some(other),
                        };
                        if let Some(lit_ch) = mapped {
                            // Check if followed by `?`, `*`, or `{0,...}` which makes this char optional
                            let optional_quant = chars
                                .get(i + 2)
                                .is_some_and(|&q| q == '?' || q == '*' || q == '{');
                            if optional_quant {
                                flush_run(&mut current_run, &mut all_queries, &mut literal_runs);
                            } else {
                                let mut buf = [0u8; 4];
                                let s = lit_ch.encode_utf8(&mut buf);
                                current_run.extend_from_slice(s.as_bytes());
                            }
                        }
                    }
                    i += 2;
                    continue;
                }
                flush_run(&mut current_run, &mut all_queries, &mut literal_runs);
                i += 1;
            }
            '^' | '$' => {
                flush_run(&mut current_run, &mut all_queries, &mut literal_runs);
                i += 1;
            }
            '.' | '+' | '?' | '*' => {
                flush_run(&mut current_run, &mut all_queries, &mut literal_runs);
                i += 1;
            }
            '[' => {
                flush_run(&mut current_run, &mut all_queries, &mut literal_runs);
                i += 1;
                while i < chars.len() {
                    if chars[i] == '\\' {
                        i += 2;
                        continue;
                    }
                    if chars[i] == ']' {
                        i += 1;
                        break;
                    }
                    i += 1;
                }
            }
            '{' => {
                // Conservatively pop the preceding character and flush run on repetition bounds
                let _ = current_run.pop();
                flush_run(&mut current_run, &mut all_queries, &mut literal_runs);
                while i < chars.len() && chars[i] != '}' {
                    i += 1;
                }
                if i < chars.len() {
                    i += 1;
                }
            }
            '(' => {
                flush_run(&mut current_run, &mut all_queries, &mut literal_runs);
                // Find matching `)`
                let mut depth = 0usize;
                let mut in_cls = false;
                let group_start = i + 1;
                let mut group_end = None;
                let mut j = i;
                while j < chars.len() {
                    match chars[j] {
                        '\\' => {
                            j += 2;
                            continue;
                        }
                        '[' if !in_cls => in_cls = true,
                        ']' if in_cls => in_cls = false,
                        '(' if !in_cls => depth += 1,
                        ')' if !in_cls => {
                            depth = depth.saturating_sub(1);
                            if depth == 0 {
                                group_end = Some(j);
                                break;
                            }
                        }
                        _ => {}
                    }
                    j += 1;
                }
                if let Some(end_idx) = group_end {
                    let after_group_quant = chars
                        .get(end_idx + 1)
                        .is_some_and(|&q| q == '?' || q == '*' || q == '{');
                    let inner_raw: String = chars[group_start..end_idx].iter().collect();
                    let inner_slice = inner_raw.strip_prefix("?:").unwrap_or(&inner_raw);
                    let is_special_group = inner_slice.starts_with('?');
                    if !after_group_quant && !is_special_group {
                        let sub = decompose_regex_pattern(inner_slice);
                        match sub {
                            QueryPlan::And {
                                queries: sub_q,
                                literal_runs: sub_runs,
                            } => {
                                all_queries.extend(sub_q);
                                literal_runs.extend(sub_runs);
                            }
                            QueryPlan::Or(sub_branches) => {
                                if or_branches_opt.is_none() {
                                    or_branches_opt = Some(sub_branches);
                                }
                            }
                            QueryPlan::MatchAll => {}
                        }
                    }
                    i = end_idx + 1;
                } else {
                    i += 1;
                }
            }
            ')' | '|' => {
                flush_run(&mut current_run, &mut all_queries, &mut literal_runs);
                i += 1;
            }
            lit_ch => {
                // Check if the next char is `?`, `*`, or `{`, which makes `lit_ch` optional or repeated
                let next_is_optional_or_brace = chars
                    .get(i + 1)
                    .is_some_and(|&q| q == '?' || q == '*' || q == '{');
                let next_is_plus = chars.get(i + 1) == Some(&'+');
                if next_is_optional_or_brace {
                    flush_run(&mut current_run, &mut all_queries, &mut literal_runs);
                    i += 2;
                } else if next_is_plus {
                    // `a+` guarantees at least one `a`, then breaks the contiguous run
                    let mut buf = [0u8; 4];
                    let s = lit_ch.encode_utf8(&mut buf);
                    current_run.extend_from_slice(s.as_bytes());
                    flush_run(&mut current_run, &mut all_queries, &mut literal_runs);
                    i += 2;
                } else {
                    let mut buf = [0u8; 4];
                    let s = lit_ch.encode_utf8(&mut buf);
                    current_run.extend_from_slice(s.as_bytes());
                    i += 1;
                }
            }
        }
    }

    flush_run(&mut current_run, &mut all_queries, &mut literal_runs);

    if let Some(or_branches) = or_branches_opt {
        if all_queries.is_empty() {
            return QueryPlan::Or(or_branches);
        }
        let distributed: Vec<QueryPlan> = or_branches
            .into_iter()
            .map(|b| match b {
                QueryPlan::And {
                    mut queries,
                    literal_runs: mut b_runs,
                } => {
                    queries.extend_from_slice(&all_queries);
                    b_runs.extend_from_slice(&literal_runs);
                    QueryPlan::And {
                        queries,
                        literal_runs: b_runs,
                    }
                }
                QueryPlan::Or(_) | QueryPlan::MatchAll => QueryPlan::And {
                    queries: all_queries.clone(),
                    literal_runs: literal_runs.clone(),
                },
            })
            .collect();
        return QueryPlan::Or(distributed);
    }

    if all_queries.is_empty() {
        QueryPlan::MatchAll
    } else {
        QueryPlan::And {
            queries: all_queries,
            literal_runs,
        }
    }
}

/// Deduplicates trigram queries in `QueryPlan::And` while preserving `expected_next` safety
/// (`microsoft/tgrep` `simplify` invariant: if a trigram appears multiple times with different
/// `expected_next` bytes, clear `expected_next` on the retained query to avoid false negatives).
fn simplify_plan(plan: QueryPlan) -> QueryPlan {
    match plan {
        QueryPlan::And {
            mut queries,
            literal_runs,
        } => {
            queries.sort_by_key(|q| q.hash);
            queries.dedup_by(|retained, dup| {
                if retained.hash == dup.hash {
                    if retained.expected_next != dup.expected_next {
                        retained.expected_next = None;
                    }
                    true
                } else {
                    false
                }
            });
            if queries.is_empty() {
                QueryPlan::MatchAll
            } else {
                QueryPlan::And {
                    queries,
                    literal_runs,
                }
            }
        }
        QueryPlan::Or(plans) => {
            let mut simplified = Vec::with_capacity(plans.len());
            for p in plans {
                let s = simplify_plan(p);
                if s.is_match_all() {
                    return QueryPlan::MatchAll;
                }
                simplified.push(s);
            }
            match simplified.len() {
                0 => QueryPlan::MatchAll,
                1 => simplified.pop().unwrap_or(QueryPlan::MatchAll),
                _ => QueryPlan::Or(simplified),
            }
        }
        QueryPlan::MatchAll => QueryPlan::MatchAll,
    }
}

/// Workspace Trigram & File Path Index for instant candidate narrowing and file discovery.
pub struct TrigramIndex {
    /// All non-ignored relative file paths in the workspace (indexed by `file_id: u32`).
    all_files: Vec<String>,
    /// Subset of `file_id`s that are searchable text files (excludes binary files).
    searchable_text_files: Vec<u32>,
    /// Text files > `MAX_INDEXED_FILE_BYTES` (4 MiB) that bypass trigram postings and are always included as candidates.
    passthrough_large_files: Vec<u32>,
    /// Trigram -> sorted `Vec<PostingEntry>` (ordered by `file_id`).
    postings: HashMap<TrigramHash, Vec<PostingEntry>, BuildTrigramHasher>,
}

type CachedTrigramEntry = (PathBuf, Instant, u64, Arc<TrigramIndex>);
static GLOBAL_TRIGRAM_INDEX_CACHE: OnceLock<RwLock<Option<CachedTrigramEntry>>> = OnceLock::new();

impl Default for TrigramIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl TrigramIndex {
    pub fn new() -> Self {
        Self {
            all_files: Vec::new(),
            searchable_text_files: Vec::new(),
            passthrough_large_files: Vec::new(),
            postings: HashMap::with_hasher(BuildTrigramHasher::default()),
        }
    }

    /// Invalidates the shared in-memory `TrigramIndex` cache when workspace files are modified.
    pub fn invalidate_cache() {
        if let Some(cache) = GLOBAL_TRIGRAM_INDEX_CACHE.get() {
            if let Ok(mut guard) = cache.write() {
                *guard = None;
            }
        }
    }

    /// Returns a shared `Arc<TrigramIndex>` for `workspace_root`, reusing the in-memory index
    /// (30s TTL or matching workspace FNV-1a fingerprint) or loading `.minicode/cache/trigram_index.bin`.
    pub fn get_or_build_cached(workspace_root: &Path) -> Result<Arc<Self>> {
        let cache = GLOBAL_TRIGRAM_INDEX_CACHE.get_or_init(|| RwLock::new(None));
        if let Ok(guard) = cache.read() {
            if let Some((ref cached_root, built_at, _fp, ref arc_idx)) = *guard {
                if cached_root == workspace_root && built_at.elapsed() < Duration::from_secs(30) {
                    return Ok(Arc::clone(arc_idx));
                }
            }
        }

        let (abs_files, rel_files, fingerprint) = Self::collect_workspace_files(workspace_root);

        if let Ok(mut guard) = cache.write() {
            if let Some((ref cached_root, ref mut built_at, cached_fp, ref arc_idx)) = *guard {
                if cached_root == workspace_root && cached_fp == fingerprint {
                    *built_at = Instant::now();
                    return Ok(Arc::clone(arc_idx));
                }
            }
        }

        let disk_cache_path = workspace_root.join(TRIGRAM_CACHE_REL_PATH);
        let fresh = if let Some(loaded) = Self::load_from_disk(&disk_cache_path, fingerprint) {
            loaded
        } else {
            let mut built = Self::new();
            built.build_from_file_list(&abs_files, rel_files);
            if workspace_root.join(".minicode").is_dir() {
                let _ = built.save_to_disk(&disk_cache_path, fingerprint);
            }
            built
        };

        let arc_fresh = Arc::new(fresh);
        if let Ok(mut guard) = cache.write() {
            *guard = Some((
                workspace_root.to_path_buf(),
                Instant::now(),
                fingerprint,
                Arc::clone(&arc_fresh),
            ));
        }
        Ok(arc_fresh)
    }

    /// Walks `workspace_root` respecting `.gitignore` and excluded directories, returning
    /// `(abs_paths, rel_paths, fnv1a_fingerprint)`.
    pub fn collect_workspace_files(workspace_root: &Path) -> (Vec<PathBuf>, Vec<String>, u64) {
        const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
        const FNV_PRIME: u64 = 0x0100_0000_01b3;

        let mut builder = WalkBuilder::new(workspace_root);
        builder
            .hidden(true)
            .parents(true)
            .git_ignore(true)
            .git_global(true)
            .filter_entry(|entry| {
                if let Some(name) = entry.file_name().to_str() {
                    !crate::context::walker::is_excluded_dir_name(name)
                } else {
                    true
                }
            });

        let mut pairs: Vec<(PathBuf, String)> = Vec::new();
        for entry in builder.build().flatten() {
            if !entry.file_type().is_some_and(|ft| ft.is_file()) {
                continue;
            }
            let path = entry.path().to_path_buf();
            let rel = path
                .strip_prefix(workspace_root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            pairs.push((path, rel));
        }

        // Sort deterministically by relative path so file_ids and fingerprints are 100% stable
        pairs.sort_by(|a, b| a.1.cmp(&b.1));

        let mut hash = FNV_OFFSET;
        let mix_u64 = |h: &mut u64, val: u64| {
            for b in val.to_le_bytes() {
                *h ^= u64::from(b);
                *h = h.wrapping_mul(FNV_PRIME);
            }
        };

        mix_u64(&mut hash, u64::from(TRIGRAM_INDEX_VERSION));
        mix_u64(&mut hash, pairs.len() as u64);

        let mut abs_files = Vec::with_capacity(pairs.len());
        let mut rel_files = Vec::with_capacity(pairs.len());

        for (abs_path, rel_path) in pairs {
            for &b in rel_path.as_bytes() {
                hash ^= u64::from(b);
                hash = hash.wrapping_mul(FNV_PRIME);
            }
            if let Ok(meta) = fs::metadata(&abs_path) {
                mix_u64(&mut hash, meta.len());
                if let Ok(modified) = meta.modified() {
                    if let Ok(dur) = modified.duration_since(UNIX_EPOCH) {
                        mix_u64(&mut hash, dur.as_secs());
                        mix_u64(&mut hash, u64::from(dur.subsec_nanos()));
                    }
                }
            }
            abs_files.push(abs_path);
            rel_files.push(rel_path);
        }

        (abs_files, rel_files, hash)
    }

    /// Builds the trigram index from a sorted list of `(abs_files, rel_files)`.
    pub fn build_from_file_list(&mut self, abs_files: &[PathBuf], rel_files: Vec<String>) {
        self.all_files = rel_files;
        self.searchable_text_files.clear();
        self.passthrough_large_files.clear();
        self.postings.clear();

        for (idx, path) in abs_files.iter().enumerate() {
            let Ok(file_id) = u32::try_from(idx) else {
                break;
            };
            let Ok(meta) = fs::metadata(path) else {
                continue;
            };
            if meta.len() > MAX_INDEXED_FILE_BYTES {
                // Check first 8 KiB for NUL byte before admitting as passthrough text file
                if let Ok(mut f) = fs::File::open(path) {
                    let mut buf = [0u8; BINARY_SNIFF_LEN];
                    if let Ok(n) = f.read(&mut buf) {
                        if !buf[..n].contains(&0) {
                            self.searchable_text_files.push(file_id);
                            self.passthrough_large_files.push(file_id);
                        }
                    }
                }
                continue;
            }

            let Ok(bytes) = fs::read(path) else {
                continue;
            };
            let Some(file_trigrams) = extract_file_trigrams(&bytes) else {
                // Binary file: kept in `all_files` for `file_search`, excluded from text `grep_search`
                continue;
            };

            self.searchable_text_files.push(file_id);
            for (trigram_hash, masks) in file_trigrams {
                self.postings
                    .entry(trigram_hash)
                    .or_default()
                    .push(PostingEntry {
                        file_id,
                        loc_mask: masks.loc_mask,
                        next_mask: masks.next_mask,
                    });
            }
        }
    }

    /// Returns all indexed relative file paths in the workspace (`path_index` table).
    #[inline]
    pub fn all_files(&self) -> &[String] {
        &self.all_files
    }

    /// Evaluates `plan` against the trigram index and returns the ordered list of candidate
    /// relative file paths to verify with the line/regex matcher.
    pub fn query_candidate_files(&self, plan: &QueryPlan, file_pattern: Option<&str>) -> Vec<&str> {
        let candidate_ids: Vec<u32> = match self.evaluate_plan_ids(plan) {
            Some(mut ids) => {
                if !self.passthrough_large_files.is_empty() {
                    ids.extend_from_slice(&self.passthrough_large_files);
                    ids.sort_unstable();
                    ids.dedup();
                }
                ids
            }
            None => self.searchable_text_files.clone(),
        };

        let mut out = Vec::with_capacity(candidate_ids.len());
        for fid in candidate_ids {
            let Some(rel_path) = self.all_files.get(fid as usize) else {
                continue;
            };
            if let Some(pat) = file_pattern {
                let file_name = Path::new(rel_path)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(rel_path);
                if !crate::utils::strings::wildcard_match(rel_path, pat)
                    && !crate::utils::strings::wildcard_match(file_name, pat)
                {
                    continue;
                }
            }
            out.push(rel_path.as_str());
        }
        out
    }

    /// Evaluates a [`QueryPlan`] to `Some(sorted_candidate_file_ids)`, or `None` if `MatchAll`.
    fn evaluate_plan_ids(&self, plan: &QueryPlan) -> Option<Vec<u32>> {
        match plan {
            QueryPlan::MatchAll => None,
            QueryPlan::Or(branches) => {
                let mut union_ids = Vec::new();
                for branch in branches {
                    let branch_ids = self.evaluate_plan_ids(branch)?;
                    union_ids.extend(branch_ids);
                }
                union_ids.sort_unstable();
                union_ids.dedup();
                Some(union_ids)
            }
            QueryPlan::And {
                queries,
                literal_runs,
            } => {
                if queries.is_empty() {
                    return None;
                }

                // Fetch posting slices and sort smallest-first (`microsoft/tgrep` intersection order)
                let mut ordered_queries: Vec<(&TrigramQuery, &[PostingEntry])> =
                    Vec::with_capacity(queries.len());
                for q in queries {
                    match self.postings.get(&q.hash) {
                        Some(list) if !list.is_empty() => {
                            ordered_queries.push((q, list.as_slice()))
                        }
                        _ => return Some(Vec::new()), // Short-circuit: a required trigram occurs in 0 files!
                    }
                }
                ordered_queries.sort_by_key(|(_, slice)| slice.len());

                // Seed candidates from the rarest trigram posting list, applying `next_mask` Bloom check
                let (first_q, first_slice) = ordered_queries[0];
                let first_bloom = first_q.expected_next.map(bloom_hash).unwrap_or(0);
                let mut candidates: Vec<u32> = first_slice
                    .iter()
                    .filter(|e| first_bloom == 0 || (e.next_mask & first_bloom) != 0)
                    .map(|e| e.file_id)
                    .collect();

                if candidates.is_empty() {
                    return Some(Vec::new());
                }

                // Intersect in-place with remaining posting lists in ascending length order
                for &(q, slice) in &ordered_queries[1..] {
                    let bloom = q.expected_next.map(bloom_hash).unwrap_or(0);
                    let mut next_candidates = Vec::with_capacity(candidates.len());
                    let (mut i, mut j) = (0usize, 0usize);
                    while i < candidates.len() && j < slice.len() {
                        let fid_a = candidates[i];
                        let entry_b = slice[j];
                        match fid_a.cmp(&entry_b.file_id) {
                            std::cmp::Ordering::Less => i += 1,
                            std::cmp::Ordering::Greater => j += 1,
                            std::cmp::Ordering::Equal => {
                                if bloom == 0 || (entry_b.next_mask & bloom) != 0 {
                                    next_candidates.push(fid_a);
                                }
                                i += 1;
                                j += 1;
                            }
                        }
                    }
                    candidates = next_candidates;
                    if candidates.is_empty() {
                        return Some(Vec::new());
                    }
                }

                // Verify positional `loc_mask` rotate-left adjacency across literal trigram chains
                if !literal_runs.is_empty() {
                    candidates.retain(|&fid| {
                        for run in literal_runs {
                            if run.len() < 2 {
                                continue;
                            }
                            let mut prev_loc: Option<u8> = None;
                            for &tri_hash in run {
                                let Some(list) = self.postings.get(&tri_hash) else {
                                    return false;
                                };
                                let Ok(pos) = list.binary_search_by_key(&fid, |e| e.file_id) else {
                                    return false;
                                };
                                let curr_loc = list[pos].loc_mask;
                                if let Some(prev) = prev_loc {
                                    if (prev.rotate_left(1) & curr_loc) == 0 {
                                        return false;
                                    }
                                }
                                prev_loc = Some(curr_loc);
                            }
                        }
                        true
                    });
                }

                Some(candidates)
            }
        }
    }

    /// Saves the `TrigramIndex` to a compact binary snapshot on disk atomically.
    pub fn save_to_disk(&self, cache_path: &Path, fingerprint: u64) -> Result<()> {
        if let Some(parent) = cache_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut buf = Vec::with_capacity(64 * 1024);
        buf.extend_from_slice(TRIGRAM_INDEX_MAGIC);
        buf.extend_from_slice(&TRIGRAM_INDEX_VERSION.to_le_bytes());
        buf.extend_from_slice(&fingerprint.to_le_bytes());

        // 1. all_files
        buf.extend_from_slice(&(self.all_files.len() as u32).to_le_bytes());
        for path in &self.all_files {
            let bytes = path.as_bytes();
            buf.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
            buf.extend_from_slice(bytes);
        }

        // 2. searchable_text_files
        buf.extend_from_slice(&(self.searchable_text_files.len() as u32).to_le_bytes());
        for &fid in &self.searchable_text_files {
            buf.extend_from_slice(&fid.to_le_bytes());
        }

        // 3. passthrough_large_files
        buf.extend_from_slice(&(self.passthrough_large_files.len() as u32).to_le_bytes());
        for &fid in &self.passthrough_large_files {
            buf.extend_from_slice(&fid.to_le_bytes());
        }

        // 4. postings
        buf.extend_from_slice(&(self.postings.len() as u32).to_le_bytes());
        for (&tri, entries) in &self.postings {
            buf.extend_from_slice(&tri.to_le_bytes());
            buf.extend_from_slice(&(entries.len() as u32).to_le_bytes());
            for e in entries {
                buf.extend_from_slice(&e.file_id.to_le_bytes());
                buf.push(e.loc_mask);
                buf.push(e.next_mask);
            }
        }

        let tmp_path = cache_path.with_extension(format!(
            "tmp.{}.{}",
            std::process::id(),
            uuid::Uuid::new_v4().simple()
        ));
        let mut file = fs::File::create(&tmp_path)?;
        file.write_all(&buf)?;
        file.flush()?;
        drop(file);

        if let Err(e) = fs::rename(&tmp_path, cache_path) {
            let _ = fs::remove_file(&tmp_path);
            return Err(e.into());
        }
        Ok(())
    }

    /// Loads a persisted `TrigramIndex` binary snapshot if its magic, version, and fingerprint match.
    pub fn load_from_disk(cache_path: &Path, expected_fingerprint: u64) -> Option<Self> {
        let bytes = fs::read(cache_path).ok()?;
        if bytes.len() < 20 || &bytes[0..8] != TRIGRAM_INDEX_MAGIC {
            return None;
        }
        let mut pos = 8usize;
        let read_u32 = |b: &[u8], p: &mut usize| -> Option<u32> {
            let s = b.get(*p..*p + 4)?;
            *p += 4;
            Some(u32::from_le_bytes(s.try_into().ok()?))
        };
        let read_u64 = |b: &[u8], p: &mut usize| -> Option<u64> {
            let s = b.get(*p..*p + 8)?;
            *p += 8;
            Some(u64::from_le_bytes(s.try_into().ok()?))
        };

        let version = read_u32(&bytes, &mut pos)?;
        let fp = read_u64(&bytes, &mut pos)?;
        if version != TRIGRAM_INDEX_VERSION || fp != expected_fingerprint {
            return None;
        }

        let file_count = read_u32(&bytes, &mut pos)? as usize;
        let mut all_files = Vec::with_capacity(file_count.min(65536));
        for _ in 0..file_count {
            let len = read_u32(&bytes, &mut pos)? as usize;
            let slice = bytes.get(pos..pos + len)?;
            pos += len;
            all_files.push(std::str::from_utf8(slice).ok()?.to_string());
        }

        let text_count = read_u32(&bytes, &mut pos)? as usize;
        let mut searchable_text_files = Vec::with_capacity(text_count.min(65536));
        for _ in 0..text_count {
            searchable_text_files.push(read_u32(&bytes, &mut pos)?);
        }

        let large_count = read_u32(&bytes, &mut pos)? as usize;
        let mut passthrough_large_files = Vec::with_capacity(large_count.min(1024));
        for _ in 0..large_count {
            passthrough_large_files.push(read_u32(&bytes, &mut pos)?);
        }

        let trigram_count = read_u32(&bytes, &mut pos)? as usize;
        let mut postings = HashMap::with_capacity_and_hasher(
            trigram_count.min(262_144),
            BuildTrigramHasher::default(),
        );
        for _ in 0..trigram_count {
            let tri = read_u32(&bytes, &mut pos)?;
            let entry_count = read_u32(&bytes, &mut pos)? as usize;
            let byte_len = entry_count.checked_mul(6)?;
            let raw_entries = bytes.get(pos..pos + byte_len)?;
            pos += byte_len;

            let mut list = Vec::with_capacity(entry_count);
            for chunk in raw_entries.chunks_exact(6) {
                let file_id = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                list.push(PostingEntry {
                    file_id,
                    loc_mask: chunk[4],
                    next_mask: chunk[5],
                });
            }
            postings.insert(tri, list);
        }

        Some(Self {
            all_files,
            searchable_text_files,
            passthrough_large_files,
            postings,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trigram_packing_and_masks() {
        // Case-insensitive 24-bit packing
        assert_eq!(
            pack_trigram(b'F', b'n', b' '),
            pack_trigram(b'f', b'n', b' ')
        );
        assert_ne!(
            pack_trigram(b'f', b'n', b' '),
            pack_trigram(b'f', b'n', b'_')
        );

        // Binary detection via NUL byte
        assert!(extract_file_trigrams(b"hello\0world").is_none());

        // Adjacency check via loc_mask rotate_left(1)
        let masks = extract_file_trigrams(b"pub fn execute_turn()").expect("text file");
        let t1 = masks[&pack_trigram(b'e', b'x', b'e')];
        let t2 = masks[&pack_trigram(b'x', b'e', b'c')];
        let t3 = masks[&pack_trigram(b'e', b'c', b'u')];
        assert!(check_adjacency(&[t1, t2, t3]));
        assert_ne!(t1.next_mask & bloom_hash(b'c'), 0);
    }

    #[test]
    fn test_regex_and_literal_query_plan_narrowing() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        fs::create_dir_all(temp_dir.path().join("src")).expect("create src");
        fs::create_dir_all(temp_dir.path().join(".minicode")).expect("create .minicode");

        fs::write(
            temp_dir.path().join("src/alpha.rs"),
            "pub fn alpha_unique_target() -> usize { 42 }\n",
        )
        .unwrap();
        fs::write(
            temp_dir.path().join("src/beta.rs"),
            "pub fn beta_other_helper() -> usize { 99 }\n",
        )
        .unwrap();
        // False-positive trap: contains "alp", "lph", "pha" scattered at non-adjacent offsets
        fs::write(
            temp_dir.path().join("src/scattered.rs"),
            "// alp ... lph ... pha scattered tokens without adjacency\n",
        )
        .unwrap();

        let idx = TrigramIndex::get_or_build_cached(temp_dir.path()).expect("build index");
        assert_eq!(idx.all_files().len(), 3);

        // 1. Literal query narrows to `src/alpha.rs` and rejects `src/beta.rs` and `src/scattered.rs`
        let plan_lit = build_query_plan("alpha_unique_target", false);
        let candidates_lit = idx.query_candidate_files(&plan_lit, None);
        assert_eq!(candidates_lit, vec!["src/alpha.rs"]);

        // 2. Regex with alternation narrows to the union of matching files
        let plan_or = build_query_plan("(alpha_unique_target|beta_other_helper)", true);
        let candidates_or = idx.query_candidate_files(&plan_or, None);
        assert_eq!(candidates_or, vec!["src/alpha.rs", "src/beta.rs"]);

        // 3. Disk cache snapshot roundtrip
        let cache_path = temp_dir.path().join(TRIGRAM_CACHE_REL_PATH);
        assert!(cache_path.exists());
        let (_, _, fp) = TrigramIndex::collect_workspace_files(temp_dir.path());
        let loaded = TrigramIndex::load_from_disk(&cache_path, fp).expect("load disk snapshot");
        assert_eq!(
            loaded.query_candidate_files(&plan_lit, None),
            vec!["src/alpha.rs"]
        );
    }
}

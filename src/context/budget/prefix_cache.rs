//! Protocol-Level KV Prefix Ledger, Append-Only Tool Prefix Stabilizer, and CacheBlend-Inspired
//! 2-Tier Context Block Reuse Engine (inspired by `LMCache/LMCache` and `CacheBlend`).
//!
//! Operates completely automatically inside `AgentLoop` and subagent workers with zero `/commands`:
//! 1. [`PrefixChunkLedger`]: Rolling content-addressed FNV-1a chain hashes over `[system, tools..., messages...]`
//!    to compute the exact Longest Common Cached Prefix (LCCP) and prefix hit ratio on every turn.
//! 2. [`append_only_merge_tools`]: Stabilizes dynamic tool schema additions strictly at the tail so
//!    previously cached `system + tools[..k]` tokens remain byte-identical across turns.
//! 3. [`shared_subagent_prompt_cache_key`]: Derives a workspace+role-aligned `prompt_cache_key` so
//!    parallel fan-out workers share one hot KV prefix instead of paying cold prefill $N$ times.
//! 4. [`ContextChunkCache`]: 2-tier content-addressed observation chunk store (L1 hot in-memory LRU
//!    and L2 `.minicode/cache/context_chunks.bin` disk tier with high-watermark eviction) supporting
//!    CacheBlend selective delta re-injection when a previously cached file block mutates slightly.

use crate::agent::provider::ToolSchema;
use crate::agent::types::{Message, Role};
use crate::constants::{FNV_OFFSET_BASIS, FNV_PRIME};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

/// Domain separation tags for rolling prefix chunk hashes.
const TAG_SYSTEM: u8 = 0x01;
const TAG_TOOL: u8 = 0x02;
const TAG_MESSAGE: u8 = 0x03;

/// Default L1 hot-memory byte budget for [`ContextChunkCache`] (2 MiB).
const DEFAULT_L1_MAX_BYTES: usize = 2 * 1024 * 1024;
/// Default L2 disk persistence byte budget for [`ContextChunkCache`] (4 MiB).
const DEFAULT_L2_MAX_BYTES: usize = 4 * 1024 * 1024;
/// Maximum number of lines to run full DP Myers/LCS line-diff over before falling back to full block.
const MAX_DELTA_DIFF_LINES: usize = 1200;

// ============================================================================
// 1. Rolling FNV-1a Hashing Helpers
// ============================================================================

#[inline]
fn fnv_feed(mut h: u64, bytes: &[u8]) -> u64 {
    for &b in bytes {
        h ^= u64::from(b);
        h = h.wrapping_mul(FNV_PRIME);
    }
    h
}

#[inline]
fn fnv_chain(prev_hash: u64, tag: u8, payload: &[u8]) -> u64 {
    let mut h = FNV_OFFSET_BASIS;
    h = fnv_feed(h, &prev_hash.to_le_bytes());
    h = fnv_feed(h, &[tag]);
    h = fnv_feed(h, &(payload.len() as u64).to_le_bytes());
    fnv_feed(h, payload)
}

#[inline]
fn hash_bytes_64(bytes: &[u8]) -> u64 {
    fnv_feed(FNV_OFFSET_BASIS, bytes)
}

fn hash_tool_schema(prev_hash: u64, tool: &ToolSchema) -> u64 {
    let params_json = serde_json::to_string(&tool.parameters).unwrap_or_default();
    let mut h = fnv_chain(prev_hash, TAG_TOOL, tool.name.as_bytes());
    h = fnv_feed(h, &[0x1F]);
    h = fnv_feed(h, tool.description.as_bytes());
    h = fnv_feed(h, &[0x1F]);
    fnv_feed(h, params_json.as_bytes())
}

fn hash_message(prev_hash: u64, msg: &Message) -> u64 {
    let role_byte: u8 = match msg.role {
        Role::System => 1,
        Role::User => 2,
        Role::Assistant => 3,
        Role::Tool => 4,
    };
    let mut h = fnv_chain(prev_hash, TAG_MESSAGE, &[role_byte]);
    h = fnv_feed(h, &(msg.content.len() as u64).to_le_bytes());
    h = fnv_feed(h, msg.content.as_bytes());
    if let Some(ref name) = msg.tool_name {
        h = fnv_feed(h, &[0x1D]);
        h = fnv_feed(h, name.as_bytes());
    }
    if let Some(ref cid) = msg.tool_call_id {
        h = fnv_feed(h, &[0x1C]);
        h = fnv_feed(h, cid.as_bytes());
    }
    if let Some(ref calls) = msg.tool_calls {
        h = fnv_feed(h, &(calls.len() as u64).to_le_bytes());
        for tc in calls {
            h = fnv_feed(h, tc.id.as_bytes());
            h = fnv_feed(h, &[0x1E]);
            h = fnv_feed(h, tc.name.as_bytes());
            h = fnv_feed(h, &[0x1E]);
            let arg_s = serde_json::to_string(&tc.arguments).unwrap_or_default();
            h = fnv_feed(h, arg_s.as_bytes());
        }
    }
    if let Some(ref imgs) = msg.images {
        h = fnv_feed(h, &(imgs.len() as u64).to_le_bytes());
        for img in imgs {
            h = fnv_feed(h, img.media_type.as_bytes());
            h = fnv_feed(h, &(img.data_base64.len() as u64).to_le_bytes());
            // Hash boundary + stride sample of base64 payload for O(1) image change detection
            let bytes = img.data_base64.as_bytes();
            let head = bytes.len().min(64);
            h = fnv_feed(h, &bytes[..head]);
            if bytes.len() > 64 {
                h = fnv_feed(h, &bytes[bytes.len() - 64..]);
            }
        }
    }
    h
}

// ============================================================================
// 2. PrefixChunkLedger — Exact Longest Common Cached Prefix (LCCP) Tracker
// ============================================================================

/// Exact prefix-alignment telemetry comparing the current request payload against the
/// previously committed KV-cached prefix chain.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PrefixMatchReport {
    /// Total prefix chunks evaluated (`1 (system) + tools.len() + messages.len()`).
    pub total_chunks: usize,
    /// Longest contiguous prefix chunk count matching the prior committed request.
    pub matched_chunks: usize,
    /// Exact fraction `matched_chunks / total_chunks` in `[0.0, 1.0]`.
    pub hit_ratio: f64,
    /// Whether Zone 1 (system prompt) matched identically.
    pub system_hit: bool,
    /// Number of leading Zone-0 tool schemas that remained byte-identical in order.
    pub matched_tool_schemas: usize,
    /// Number of leading conversation messages preserved identically in the cached prefix.
    pub lccp_message_count: usize,
    /// Index in `messages` where prefix divergence first occurred (if divergence happened inside messages).
    pub first_divergence_msg_idx: Option<usize>,
}

/// Rolling content-addressed KV prefix ledger tracking byte-identical prefix stability across
/// `[system_prompt, tool_schemas..., messages...]` (LMCache chunk-hash architecture).
#[derive(Debug, Clone, Default)]
pub struct PrefixChunkLedger {
    last_hashes: Vec<u64>,
    last_tool_count: usize,
    last_msg_count: usize,
    total_turns: usize,
    cumulative_matched_chunks: usize,
    cumulative_total_chunks: usize,
    last_report: Option<PrefixMatchReport>,
}

impl PrefixChunkLedger {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Resets the prefix ledger (e.g., on `reset_session` or after full history wipe).
    pub fn reset(&mut self) {
        self.last_hashes.clear();
        self.last_tool_count = 0;
        self.last_msg_count = 0;
        self.last_report = None;
    }

    /// Computes rolling chain hashes and compares against the previously committed KV prefix
    /// without mutating ledger state.
    #[must_use]
    pub fn evaluate_prefix(
        &self,
        system_prompt: &str,
        tools: &[ToolSchema],
        messages: &[Message],
    ) -> (Vec<u64>, PrefixMatchReport) {
        let total_chunks = 1 + tools.len() + messages.len();
        let mut current = Vec::with_capacity(total_chunks);

        let mut acc = fnv_chain(0, TAG_SYSTEM, system_prompt.as_bytes());
        current.push(acc);

        for tool in tools {
            acc = hash_tool_schema(acc, tool);
            current.push(acc);
        }

        for msg in messages {
            acc = hash_message(acc, msg);
            current.push(acc);
        }

        let matched_chunks = current
            .iter()
            .zip(self.last_hashes.iter())
            .take_while(|(a, b)| a == b)
            .count();

        let system_hit = matched_chunks >= 1;
        let matched_tool_schemas = matched_chunks.saturating_sub(1).min(tools.len());
        let msg_start_offset = 1 + tools.len();
        let lccp_message_count =
            if matched_chunks > msg_start_offset && self.last_tool_count == tools.len() {
                (matched_chunks - msg_start_offset).min(messages.len())
            } else {
                0
            };
        let first_divergence_msg_idx = if matched_chunks >= msg_start_offset
            && self.last_tool_count == tools.len()
            && lccp_message_count < messages.len()
        {
            Some(lccp_message_count)
        } else if !self.last_hashes.is_empty() && matched_chunks < msg_start_offset {
            Some(0)
        } else {
            None
        };

        let hit_ratio = if total_chunks > 0 {
            matched_chunks as f64 / total_chunks as f64
        } else {
            0.0
        };

        let report = PrefixMatchReport {
            total_chunks,
            matched_chunks,
            hit_ratio,
            system_hit,
            matched_tool_schemas,
            lccp_message_count,
            first_divergence_msg_idx,
        };
        (current, report)
    }

    /// Evaluates and commits the transmitted prompt prefix to the ledger after a successful
    /// provider call, returning the exact LCCP match report for this turn.
    pub fn commit_prefix(
        &mut self,
        system_prompt: &str,
        tools: &[ToolSchema],
        messages: &[Message],
    ) -> PrefixMatchReport {
        let (hashes, report) = self.evaluate_prefix(system_prompt, tools, messages);
        self.last_hashes = hashes;
        self.last_tool_count = tools.len();
        self.last_msg_count = messages.len();
        self.total_turns += 1;
        self.cumulative_matched_chunks += report.matched_chunks;
        self.cumulative_total_chunks += report.total_chunks;
        self.last_report = Some(report.clone());
        report
    }

    /// Returns the most recent prefix match report, if any turn has been committed.
    #[allow(dead_code)]
    #[must_use]
    pub fn last_report(&self) -> Option<&PrefixMatchReport> {
        self.last_report.as_ref()
    }

    /// Returns cumulative prefix hit ratio across all committed turns in this session.
    #[allow(dead_code)]
    #[must_use]
    pub fn cumulative_hit_ratio(&self) -> f64 {
        if self.cumulative_total_chunks == 0 {
            0.0
        } else {
            self.cumulative_matched_chunks as f64 / self.cumulative_total_chunks as f64
        }
    }
}

// ============================================================================
// 3. Append-Only Tool Schema Stabilization & Shared Subagent Cache Keys
// ============================================================================

/// Merges `rebuilt` tool schemas into `existing` while preserving the exact prefix order of
/// already-cached schemas whenever all existing tools remain intact (monotonic append-only tail).
///
/// Returns `(merged_schemas, changed, prefix_preserved)`:
/// - When every schema in `existing` is still present in `rebuilt` with identical `description`
///   and `parameters`, `existing[0..existing.len()]` is kept byte-identical in order and any
///   newly added schemas are sorted alphabetically and appended strictly at the tail
///   (`prefix_preserved = true`).
/// - When any existing schema was removed or its body changed (e.g., compaction toggle), returns
///   `rebuilt` directly (`prefix_preserved = false`).
#[must_use]
pub fn append_only_merge_tools(
    existing: &[ToolSchema],
    rebuilt: Vec<ToolSchema>,
) -> (Vec<ToolSchema>, bool, bool) {
    if existing.is_empty() {
        let changed = !rebuilt.is_empty();
        return (rebuilt, changed, true);
    }

    let mut rebuilt_by_name: HashMap<&str, &ToolSchema> = HashMap::with_capacity(rebuilt.len());
    for schema in &rebuilt {
        rebuilt_by_name.insert(schema.name.as_str(), schema);
    }

    // Verify monotonic prefix preservation: every previously emitted tool still exists unchanged.
    let prefix_intact = existing.iter().all(|prev| {
        rebuilt_by_name.get(prev.name.as_str()).is_some_and(|cur| {
            cur.description == prev.description && cur.parameters == prev.parameters
        })
    });

    if !prefix_intact {
        let changed = existing.len() != rebuilt.len()
            || existing.iter().zip(rebuilt.iter()).any(|(a, b)| {
                a.name != b.name || a.description != b.description || a.parameters != b.parameters
            });
        return (rebuilt, changed, false);
    }

    let existing_names: HashSet<&str> = existing.iter().map(|t| t.name.as_str()).collect();
    let mut additions: Vec<ToolSchema> = rebuilt
        .into_iter()
        .filter(|t| !existing_names.contains(t.name.as_str()))
        .collect();

    if additions.is_empty() {
        return (existing.to_vec(), false, true);
    }

    // Sort only the newly appended tail alphabetically for determinism across runs.
    additions.sort_by(|a, b| a.name.cmp(&b.name));

    let mut merged = Vec::with_capacity(existing.len() + additions.len());
    merged.extend_from_slice(existing);
    merged.extend(additions);
    (merged, true, true)
}

/// Aligns a freshly built tool list against a session's sticky append-only tool name order so
/// previously seen tools remain at their exact earlier positions and new tools append at the tail.
pub fn align_tools_to_session_order(session_order: &mut Vec<String>, tools: &mut Vec<ToolSchema>) {
    if tools.is_empty() {
        return;
    }

    let mut by_name: HashMap<String, ToolSchema> = HashMap::with_capacity(tools.len());
    for t in tools.drain(..) {
        by_name.insert(t.name.clone(), t);
    }

    let mut ordered = Vec::with_capacity(by_name.len());
    let mut retained_order = Vec::with_capacity(session_order.len());

    for name in session_order.iter() {
        if let Some(schema) = by_name.remove(name) {
            retained_order.push(name.clone());
            ordered.push(schema);
        }
    }

    let mut new_additions: Vec<ToolSchema> = by_name.into_values().collect();
    new_additions.sort_by(|a, b| a.name.cmp(&b.name));
    for schema in new_additions {
        retained_order.push(schema.name.clone());
        ordered.push(schema);
    }

    *session_order = retained_order;
    *tools = ordered;
}

/// Computes a deterministic workspace + role-scoped `prompt_cache_key` for parallel subagent
/// fan-out workers so siblings of the same role share a single hot provider KV prefix instead
/// of paying cold prefill $N$ times.
#[must_use]
pub fn shared_subagent_prompt_cache_key(workspace_root: &Path, role_badge: &str) -> String {
    let canonical =
        fs::canonicalize(workspace_root).unwrap_or_else(|_| workspace_root.to_path_buf());
    let ws_str = canonical.to_string_lossy();
    let ws_hash = hash_bytes_64(ws_str.as_bytes());
    let role_lower = role_badge.trim().to_ascii_lowercase();
    format!("minicode:ws:{:016x}:role:{}", ws_hash, role_lower)
}

// ============================================================================
// 4. ContextChunkCache — CacheBlend 2-Tier Store & Selective Delta Blending
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedChunkEntry {
    key: String,
    content_hash: u64,
    content: String,
    /// Message index where the full block was injected into the active session's live prefix.
    /// Set to `usize::MAX` when hydrated from L2 across sessions (not yet anchored in current KV prefix).
    #[serde(default = "unanchored_idx")]
    prefix_msg_idx: usize,
    last_access_tick: u64,
    byte_len: usize,
}

#[inline]
const fn unanchored_idx() -> usize {
    usize::MAX
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct L2ChunkStoreDisk {
    version: u32,
    entries: Vec<CachedChunkEntry>,
}

/// Decision returned by [`ContextChunkCache::blend_or_record`] when a file/observation block
/// is about to be appended to the live prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlendDecision {
    /// No prior live-prefix anchor exists or changes exceed the CacheBlend breakeven ratio;
    /// emit the full observation text unchanged.
    FullBlock,
    /// The exact same content hash is already anchored inside the live KV prefix (`< kv_watermark_idx`).
    IdenticalPrefixRef {
        prefix_msg_idx: usize,
        lines: usize,
        bytes_saved: usize,
        rendered_ref: String,
    },
    /// A slightly modified version of an already-live-prefix block; emitting only the compact
    /// line-level delta saves significant tokens while keeping the base prefix hot.
    SelectiveDelta {
        prefix_msg_idx: usize,
        bytes_saved: usize,
        changed_lines: usize,
        rendered_delta: String,
    },
}

/// CacheBlend-inspired 2-tier content-addressed observation chunk cache:
/// - **L1 (Hot RAM)**: bounded LRU map tracking blocks anchored inside the current KV prefix.
/// - **L2 (Disk `.minicode/cache/context_chunks.bin`)**: bounded JSON-serialized store persisted
///   atomically across sessions for cross-turn warm starts and high-watermark LRU eviction.
pub struct ContextChunkCache {
    entries: HashMap<String, CachedChunkEntry>,
    total_bytes: usize,
    l1_max_bytes: usize,
    l2_max_bytes: usize,
    tick: u64,
    disk_path: Option<PathBuf>,
    dirty_for_disk: bool,
}

impl Default for ContextChunkCache {
    fn default() -> Self {
        Self::in_memory(DEFAULT_L1_MAX_BYTES)
    }
}

impl ContextChunkCache {
    /// Creates an in-memory-only chunk cache (primarily for tests or read-only workspaces).
    #[must_use]
    pub fn in_memory(l1_max_bytes: usize) -> Self {
        Self {
            entries: HashMap::new(),
            total_bytes: 0,
            l1_max_bytes: l1_max_bytes.max(4096),
            l2_max_bytes: DEFAULT_L2_MAX_BYTES,
            tick: 0,
            disk_path: None,
            dirty_for_disk: false,
        }
    }

    /// Opens or initializes the 2-tier cache rooted at `<workspace_root>/.minicode/cache/context_chunks.bin`.
    #[must_use]
    pub fn open_for_workspace(workspace_root: &Path) -> Self {
        let disk_path = workspace_root
            .join(".minicode")
            .join("cache")
            .join("context_chunks.bin");
        let mut cache = Self {
            entries: HashMap::new(),
            total_bytes: 0,
            l1_max_bytes: DEFAULT_L1_MAX_BYTES,
            l2_max_bytes: DEFAULT_L2_MAX_BYTES,
            tick: 0,
            disk_path: Some(disk_path.clone()),
            dirty_for_disk: false,
        };
        cache.load_l2(&disk_path);
        cache
    }

    /// Clears live-prefix message anchors (e.g., after `reset_session` or context compaction)
    /// while preserving L1/L2 chunk payloads safely without dangling message references.
    pub fn invalidate_prefix_anchors_from(&mut self, from_msg_idx: usize) {
        for entry in self.entries.values_mut() {
            if entry.prefix_msg_idx != usize::MAX && entry.prefix_msg_idx >= from_msg_idx {
                entry.prefix_msg_idx = usize::MAX;
            }
        }
    }

    /// Invalidates all live-prefix anchors (called on `reset_session`).
    pub fn clear_all_prefix_anchors(&mut self) {
        for entry in self.entries.values_mut() {
            entry.prefix_msg_idx = usize::MAX;
        }
    }

    /// Evaluates whether `new_content` for logical block `key` can be served via a live-prefix
    /// reference or CacheBlend selective delta, updating the 2-tier cache accordingly.
    pub fn blend_or_record(
        &mut self,
        key: &str,
        new_content: &str,
        current_msg_idx: usize,
        kv_watermark_idx: usize,
    ) -> BlendDecision {
        let trimmed_key = key.trim();
        if trimmed_key.is_empty() || new_content.is_empty() {
            return BlendDecision::FullBlock;
        }

        self.tick = self.tick.saturating_add(1);
        let now_tick = self.tick;
        let new_hash = hash_bytes_64(new_content.as_bytes());
        let new_line_count = new_content.lines().count();

        // Check if an anchored live-prefix entry exists strictly below `kv_watermark_idx`.
        if let Some(prev) = self.entries.get_mut(trimmed_key) {
            prev.last_access_tick = now_tick;
            let is_live_in_prefix =
                prev.prefix_msg_idx != usize::MAX && prev.prefix_msg_idx < kv_watermark_idx;

            if is_live_in_prefix {
                let anchor_idx = prev.prefix_msg_idx;
                if prev.content_hash == new_hash {
                    let rendered_ref = format!(
                        "[CacheBlend: `{}` unchanged ({} lines); reuse exact block already in KV prefix at message #{}]",
                        trimmed_key,
                        new_line_count,
                        anchor_idx + 1
                    );
                    // Exact breakeven cost model: emit reference iff payload shrinks by >50% after header overhead.
                    if rendered_ref.len().saturating_mul(2) < new_content.len() {
                        let bytes_saved = new_content.len() - rendered_ref.len();
                        return BlendDecision::IdenticalPrefixRef {
                            prefix_msg_idx: anchor_idx,
                            lines: new_line_count,
                            bytes_saved,
                            rendered_ref,
                        };
                    }
                } else if let Some((delta_body, changed_lines)) =
                    compute_selective_line_delta(&prev.content, new_content)
                {
                    let delta_header = format!(
                        "[CacheBlend Selective Delta for `{}` relative to KV-cached base at message #{} ({} changed lines; unchanged lines omitted)]:\n",
                        trimmed_key,
                        anchor_idx + 1,
                        changed_lines,
                    );
                    let rendered_delta = format!("{}{}", delta_header, delta_body);
                    // Exact breakeven cost model: emit selective delta iff serialized delta + header
                    // is strictly smaller than half the full block and saves more than the header cost itself.
                    let bytes_saved = new_content.len().saturating_sub(rendered_delta.len());
                    if rendered_delta.len().saturating_mul(2) < new_content.len()
                        && bytes_saved > delta_header.len()
                    {
                        // Keep `prev.content` & `prev.prefix_msg_idx` anchored to the full base
                        // block in the prefix so subsequent reads also diff cleanly against it.
                        return BlendDecision::SelectiveDelta {
                            prefix_msg_idx: anchor_idx,
                            bytes_saved,
                            changed_lines,
                            rendered_delta,
                        };
                    }
                }
            }
        }

        // Otherwise record the full block as the new live-prefix anchor at `current_msg_idx`.
        self.upsert_entry(
            trimmed_key,
            new_hash,
            new_content,
            current_msg_idx,
            now_tick,
        );
        BlendDecision::FullBlock
    }

    fn upsert_entry(
        &mut self,
        key: &str,
        content_hash: u64,
        content: &str,
        prefix_msg_idx: usize,
        tick: u64,
    ) {
        let byte_len = key.len() + content.len();
        if byte_len > self.l1_max_bytes / 2 {
            // Avoid letting a single gigantic file evict the entire L1 working set.
            return;
        }
        if let Some(old) = self.entries.remove(key) {
            self.total_bytes = self.total_bytes.saturating_sub(old.byte_len);
        }
        self.entries.insert(
            key.to_string(),
            CachedChunkEntry {
                key: key.to_string(),
                content_hash,
                content: content.to_string(),
                prefix_msg_idx,
                last_access_tick: tick,
                byte_len,
            },
        );
        self.total_bytes += byte_len;
        self.dirty_for_disk = true;
        self.evict_lru_to_watermark();
    }

    /// High-watermark LRU eviction: drops least-recently-used entries until `total_bytes <= l1_max_bytes`.
    fn evict_lru_to_watermark(&mut self) {
        if self.total_bytes <= self.l1_max_bytes {
            return;
        }
        let target_bytes = (self.l1_max_bytes * 4) / 5;
        let mut order: Vec<(u64, String)> = self
            .entries
            .values()
            .map(|e| (e.last_access_tick, e.key.clone()))
            .collect();
        order.sort_by_key(|(tick, _)| *tick);

        for (_, k) in order {
            if self.total_bytes <= target_bytes {
                break;
            }
            if let Some(removed) = self.entries.remove(&k) {
                self.total_bytes = self.total_bytes.saturating_sub(removed.byte_len);
            }
        }
    }

    fn load_l2(&mut self, path: &Path) {
        let Ok(raw) = fs::read(path) else {
            return;
        };
        let Ok(store) = serde_json::from_slice::<L2ChunkStoreDisk>(&raw) else {
            return;
        };
        if store.version != 1 {
            return;
        }
        for mut entry in store.entries {
            // Hydrated entries from prior sessions are never pre-anchored in a fresh KV prefix.
            entry.prefix_msg_idx = usize::MAX;
            let len = entry.key.len() + entry.content.len();
            entry.byte_len = len;
            self.tick = self.tick.max(entry.last_access_tick);
            self.total_bytes += len;
            self.entries.insert(entry.key.clone(), entry);
        }
        self.evict_lru_to_watermark();
        self.dirty_for_disk = false;
    }

    /// Atomically flushes L1 entries to L2 disk cache if modified (bounded by `l2_max_bytes`).
    pub fn persist_l2(&mut self) {
        if !self.dirty_for_disk {
            return;
        }
        let Some(ref path) = self.disk_path else {
            return;
        };
        // Only persist inside workspaces that already initialized `.minicode/` to avoid polluting clean repos.
        let Some(minicode_dir) = path.parent().and_then(|p| p.parent()) else {
            return;
        };
        if !minicode_dir.is_dir() {
            return;
        }
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        let mut entries: Vec<CachedChunkEntry> = self.entries.values().cloned().collect();
        entries.sort_by(|a, b| b.last_access_tick.cmp(&a.last_access_tick));
        let mut kept = Vec::new();
        let mut acc_bytes = 0usize;
        for mut e in entries {
            if acc_bytes + e.byte_len > self.l2_max_bytes {
                break;
            }
            acc_bytes += e.byte_len;
            e.prefix_msg_idx = usize::MAX;
            kept.push(e);
        }

        let payload = L2ChunkStoreDisk {
            version: 1,
            entries: kept,
        };
        let Ok(bytes) = serde_json::to_vec(&payload) else {
            return;
        };

        let tmp = path.with_extension(format!(
            "bin.tmp.{}.{}",
            std::process::id(),
            uuid::Uuid::new_v4().simple()
        ));
        if fs::write(&tmp, &bytes).is_ok() {
            let _ = fs::rename(&tmp, path);
            self.dirty_for_disk = false;
        }
    }

    #[allow(dead_code)]
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[allow(dead_code)]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Computes a minimal line-level unified hunk delta between `old_text` and `new_text` using
/// $O(N)$ common prefix/suffix trimming + exact LCS dynamic programming on the changed interior.
///
/// Returns `Some((formatted_hunks, changed_line_count))` when a bounded selective delta exists,
/// or `None` if the change spans too many lines for a compact delta.
fn compute_selective_line_delta(old_text: &str, new_text: &str) -> Option<(String, usize)> {
    let old_lines: Vec<&str> = old_text.lines().collect();
    let new_lines: Vec<&str> = new_text.lines().collect();

    // 1. Strip identical prefix and suffix in O(N)
    let mut prefix = 0usize;
    let min_len = old_lines.len().min(new_lines.len());
    while prefix < min_len && old_lines[prefix] == new_lines[prefix] {
        prefix += 1;
    }

    let mut suffix = 0usize;
    while suffix < (min_len - prefix)
        && old_lines[old_lines.len() - 1 - suffix] == new_lines[new_lines.len() - 1 - suffix]
    {
        suffix += 1;
    }

    let old_mid = &old_lines[prefix..old_lines.len() - suffix];
    let new_mid = &new_lines[prefix..new_lines.len() - suffix];
    if old_mid.is_empty() && new_mid.is_empty() {
        return None;
    }
    if old_mid.len() > MAX_DELTA_DIFF_LINES || new_mid.len() > MAX_DELTA_DIFF_LINES {
        return None;
    }
    // Guard quadratic DP table memory (`old_mid.len() * new_mid.len() <= 250_000`)
    if old_mid.len().saturating_mul(new_mid.len()) > 250_000 {
        return None;
    }

    // 2. Exact LCS DP on the changed middle window
    let n = old_mid.len();
    let m = new_mid.len();
    let cols = m + 1;
    let mut dp = vec![0u32; (n + 1) * cols];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            dp[i * cols + j] = if old_mid[i] == new_mid[j] {
                dp[(i + 1) * cols + (j + 1)] + 1
            } else {
                dp[(i + 1) * cols + j].max(dp[i * cols + (j + 1)])
            };
        }
    }

    // 3. Reconstruct compact diff ops with 1-line context around edits
    #[derive(Clone, Copy)]
    enum DiffOp<'a> {
        Equal(usize, usize, &'a str),
        Delete(usize, &'a str),
        Insert(usize, &'a str),
    }

    let mut ops: Vec<DiffOp<'_>> = Vec::with_capacity(n + m);
    let mut i = 0usize;
    let mut j = 0usize;
    let mut changed_lines = 0usize;
    while i < n || j < m {
        if i < n && j < m && old_mid[i] == new_mid[j] {
            ops.push(DiffOp::Equal(prefix + i + 1, prefix + j + 1, new_mid[j]));
            i += 1;
            j += 1;
        } else if j < m && (i == n || dp[i * cols + (j + 1)] >= dp[(i + 1) * cols + j]) {
            ops.push(DiffOp::Insert(prefix + j + 1, new_mid[j]));
            changed_lines += 1;
            j += 1;
        } else {
            ops.push(DiffOp::Delete(prefix + i + 1, old_mid[i]));
            changed_lines += 1;
            i += 1;
        }
    }

    if changed_lines == 0 {
        return None;
    }

    // Render compact hunks omitting long unchanged spans inside the middle window
    let mut out = String::new();
    let mut last_emitted_changed = false;
    let mut skipped_eq = 0usize;

    if prefix > 0 {
        out.push_str(&format!(
            "@@ L{} ({} unchanged leading lines omitted) @@\n",
            prefix + 1,
            prefix
        ));
        if let Some(ctx) = old_lines.get(prefix - 1) {
            out.push_str(&format!("  {} | {}\n", prefix, ctx));
        }
    }

    for (idx, op) in ops.iter().enumerate() {
        match *op {
            DiffOp::Equal(old_ln, new_ln, line) => {
                let next_is_change = ops
                    .get(idx + 1)
                    .is_some_and(|o| !matches!(o, DiffOp::Equal(..)));
                if last_emitted_changed || next_is_change {
                    if skipped_eq > 0 {
                        out.push_str(&format!(
                            "@@ L{} ({} unchanged lines omitted) @@\n",
                            new_ln, skipped_eq
                        ));
                        skipped_eq = 0;
                    }
                    let _ = old_ln;
                    out.push_str(&format!("  {} | {}\n", new_ln, line));
                    last_emitted_changed = false;
                } else {
                    skipped_eq += 1;
                }
            }
            DiffOp::Delete(old_ln, line) => {
                if skipped_eq > 0 {
                    out.push_str(&format!("@@ ({} unchanged lines omitted) @@\n", skipped_eq));
                    skipped_eq = 0;
                }
                out.push_str(&format!("- {} | {}\n", old_ln, line));
                last_emitted_changed = true;
            }
            DiffOp::Insert(new_ln, line) => {
                if skipped_eq > 0 {
                    out.push_str(&format!(
                        "@@ L{} ({} unchanged lines omitted) @@\n",
                        new_ln, skipped_eq
                    ));
                    skipped_eq = 0;
                }
                out.push_str(&format!("+ {} | {}\n", new_ln, line));
                last_emitted_changed = true;
            }
        }
    }

    if suffix > 0 {
        let tail_start = new_lines.len() - suffix;
        if let Some(ctx) = new_lines.get(tail_start) {
            out.push_str(&format!("  {} | {}\n", tail_start + 1, ctx));
        }
        if suffix > 1 {
            out.push_str(&format!(
                "@@ ({} unchanged trailing lines omitted) @@",
                suffix - 1
            ));
        }
    }

    Some((out.trim_end().to_string(), changed_lines))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn mock_tool(name: &str, desc: &str) -> ToolSchema {
        ToolSchema {
            name: name.to_string(),
            description: desc.to_string(),
            parameters: json!({"type": "object", "properties": {}}),
        }
    }

    #[test]
    fn test_prefix_chunk_ledger_lccp_and_divergence_detection() {
        let mut ledger = PrefixChunkLedger::new();
        let sys = "You are minicode.";
        let tools = vec![mock_tool("read_file", "Read a file")];
        let msgs1 = vec![
            Message::user("Inspect main.rs"),
            Message::assistant("Reading main.rs now."),
        ];

        let r1 = ledger.commit_prefix(sys, &tools, &msgs1);
        assert_eq!(r1.total_chunks, 4);
        assert_eq!(r1.matched_chunks, 0);
        assert!(!r1.system_hit);

        // Append a new user turn: all 4 prior chunks match identically (100% LCCP of prior prefix)
        let mut msgs2 = msgs1.clone();
        msgs2.push(Message::user("Now fix the bug"));
        let r2 = ledger.commit_prefix(sys, &tools, &msgs2);
        assert_eq!(r2.total_chunks, 5);
        assert_eq!(r2.matched_chunks, 4);
        assert!(r2.system_hit);
        assert_eq!(r2.matched_tool_schemas, 1);
        assert_eq!(r2.lccp_message_count, 2);
        assert_eq!(r2.first_divergence_msg_idx, Some(2));
        assert!((r2.hit_ratio - 0.8).abs() < 1e-6);

        // Mutating message index 1 breaks prefix strictly at message 1 while preserving system + tools + msg 0
        let mut msgs3 = msgs2.clone();
        msgs3[1].content = "Rewritten assistant reply".to_string();
        let (_, r3) = ledger.evaluate_prefix(sys, &tools, &msgs3);
        assert!(r3.system_hit);
        assert_eq!(r3.matched_tool_schemas, 1);
        assert_eq!(r3.lccp_message_count, 1);
        assert_eq!(r3.first_divergence_msg_idx, Some(1));
    }

    #[test]
    fn test_append_only_merge_tools_preserves_cached_prefix_order() {
        let initial = vec![
            mock_tool("grep_search", "Search"),
            mock_tool("read_file", "Read"),
        ];
        // Rebuilt list inserts `activate_tools` alphabetically BEFORE `grep_search`!
        let rebuilt = vec![
            mock_tool("activate_tools", "Activate"),
            mock_tool("grep_search", "Search"),
            mock_tool("read_file", "Read"),
            mock_tool("write_file", "Write"),
        ];

        let (merged, changed, prefix_preserved) = append_only_merge_tools(&initial, rebuilt);
        assert!(changed);
        assert!(prefix_preserved);
        let names: Vec<&str> = merged.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(
            names,
            vec!["grep_search", "read_file", "activate_tools", "write_file"],
            "Existing cached tools must keep exact prefix indices while new tools append sorted at tail"
        );

        // Verify ledger sees 100% prefix match on `system + existing[..2]`
        let mut ledger = PrefixChunkLedger::new();
        ledger.commit_prefix("SYS", &initial, &[]);
        let (_, rep) = ledger.evaluate_prefix("SYS", &merged, &[]);
        assert!(rep.system_hit);
        assert_eq!(rep.matched_tool_schemas, 2);
    }

    #[test]
    fn test_shared_subagent_prompt_cache_key_aligns_siblings() {
        let ws = Path::new("/tmp/workspace_a");
        let k1 = shared_subagent_prompt_cache_key(ws, "Coder");
        let k2 = shared_subagent_prompt_cache_key(ws, "coder");
        let k3 = shared_subagent_prompt_cache_key(ws, "Reviewer");
        assert_eq!(k1, k2);
        assert_ne!(k1, k3);
        assert!(k1.starts_with("minicode:ws:"));
        assert!(k1.ends_with(":role:coder"));
    }

    #[test]
    fn test_context_chunk_cache_identical_ref_and_cacheblend_selective_delta() {
        let mut cache = ContextChunkCache::in_memory(256 * 1024);

        let mut base_lines = Vec::new();
        for i in 1..=80 {
            base_lines.push(format!("fn helper_{}() -> usize {{ {} * 42 }}", i, i));
        }
        let base = base_lines.join("\n");

        // Turn 1: first read -> FullBlock, anchored at message index 2
        let d1 = cache.blend_or_record("src/lib.rs", &base, 2, 0);
        assert_eq!(d1, BlendDecision::FullBlock);

        // Turn 2: KV watermark advanced past msg 2 -> identical re-read collapses to IdenticalPrefixRef
        let d2 = cache.blend_or_record("src/lib.rs", &base, 5, 4);
        match d2 {
            BlendDecision::IdenticalPrefixRef {
                prefix_msg_idx,
                lines,
                bytes_saved,
                ..
            } => {
                assert_eq!(prefix_msg_idx, 2);
                assert_eq!(lines, 80);
                assert!(bytes_saved > 1000);
            }
            other => panic!("Expected IdenticalPrefixRef, got {:?}", other),
        }

        // Turn 3: 2 lines edited in an 80-line file -> CacheBlend SelectiveDelta!
        let mut edited_lines = base_lines.clone();
        edited_lines[40] = "fn helper_41() -> usize { 41 * 99 }".to_string();
        edited_lines[41] = "fn helper_42() -> usize { 42 * 99 }".to_string();
        let edited = edited_lines.join("\n");

        let d3 = cache.blend_or_record("src/lib.rs", &edited, 7, 6);
        match d3 {
            BlendDecision::SelectiveDelta {
                prefix_msg_idx,
                bytes_saved,
                changed_lines,
                rendered_delta,
            } => {
                assert_eq!(prefix_msg_idx, 2);
                assert!(bytes_saved > 1000);
                assert_eq!(changed_lines, 4); // 2 deletions + 2 insertions
                assert!(rendered_delta.contains("- 41 | fn helper_41() -> usize { 41 * 42 }"));
                assert!(rendered_delta.contains("+ 41 | fn helper_41() -> usize { 41 * 99 }"));
            }
            other => panic!("Expected SelectiveDelta, got {:?}", other),
        }

        // After compaction resets kv_watermark_idx to 0, must never emit a dangling prefix reference!
        cache.clear_all_prefix_anchors();
        let d4 = cache.blend_or_record("src/lib.rs", &edited, 1, 0);
        assert_eq!(d4, BlendDecision::FullBlock);
    }

    #[test]
    fn test_context_chunk_cache_l2_persistence_and_lru_eviction() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join(".minicode")).unwrap();

        let mut cache = ContextChunkCache::open_for_workspace(dir.path());
        cache.l1_max_bytes = 512;
        cache.blend_or_record("a.rs", &"a".repeat(200), 1, 0);
        cache.blend_or_record("b.rs", &"b".repeat(200), 2, 0);
        cache.blend_or_record("c.rs", &"c".repeat(200), 3, 0);
        // High-watermark LRU eviction must have evicted `a.rs`
        assert!(cache.len() <= 2);
        assert!(!cache.entries.contains_key("a.rs"));

        cache.persist_l2();
        let reopened = ContextChunkCache::open_for_workspace(dir.path());
        assert_eq!(reopened.len(), cache.len());
        // Hydrated L2 entries are never falsely pre-anchored into a new session's KV prefix
        for entry in reopened.entries.values() {
            assert_eq!(entry.prefix_msg_idx, usize::MAX);
        }
    }
}

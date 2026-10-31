use crate::agent::models::get_model_context_limit;
use crate::agent::types::{Message, Role};
use crate::constants::{
    COMPACT_MAX_DECISIONS_IN_ANCHOR, COMPACT_MAX_DECISIONS_PER_TURN, COMPACT_MAX_DECISION_CHARS,
    COMPACT_MAX_ERRORS_PER_TURN, COMPACT_MAX_ERROR_CHARS, COMPACT_MAX_FILES_IN_ANCHOR,
    COMPACT_PRESERVE_RECENT_MESSAGES,
};
use crate::context::compressor::ContextCompressor;
use crate::error::Result;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Unified compaction tier configuration.
/// All threshold ratios are expressed as a fraction of the model's context window limit.
#[derive(Debug, Clone, Copy)]
pub struct CompactionConfig {
    /// Ratio that triggers Tier 1 (Observation Masking). Default: 0.60.
    pub tier1_ratio: f64,
    /// Ratio that triggers Tier 2 (Turn Summarization). Default: 0.80.
    pub tier2_ratio: f64,
    /// Ratio that triggers Tier 3 (Memory Anchor + Aggressive Prune). Default: 0.95.
    pub tier3_ratio: f64,
    /// Safety headroom subtracted from warning_threshold in simple compress mode. Default: 0.15.
    pub safety_margin: f64,
}

impl Default for CompactionConfig {
    fn default() -> Self {
        use crate::constants::{
            COMPACT_TIER1_RATIO, COMPACT_TIER2_RATIO, COMPACT_TIER3_RATIO, COMPRESSOR_SAFETY_MARGIN,
        };
        Self {
            tier1_ratio: COMPACT_TIER1_RATIO,
            tier2_ratio: COMPACT_TIER2_RATIO,
            tier3_ratio: COMPACT_TIER3_RATIO,
            safety_margin: COMPRESSOR_SAFETY_MARGIN,
        }
    }
}

/// Structured representation of compressed turn history.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct TurnSummary {
    pub turn_range: (usize, usize),
    pub files_read: Vec<String>,
    pub files_modified: Vec<String>,
    pub decisions: Vec<String>,
    pub errors_resolved: Vec<String>,
    pub tools_used: HashMap<String, usize>,
    #[serde(default)]
    pub ccr_id: Option<String>,
}

impl TurnSummary {
    /// Formats the summary into a clean, compact markdown block for prompt context.
    pub fn to_markdown(&self) -> String {
        let mut md = format!(
            "📋 **Context Summary (Turns {}-{})**:\n",
            self.turn_range.0, self.turn_range.1
        );

        if !self.files_read.is_empty() {
            md.push_str(&format!(
                "• **Files Read**: {}\n",
                self.files_read.join(", ")
            ));
        }

        if !self.files_modified.is_empty() {
            md.push_str(&format!(
                "• **Files Modified**: {}\n",
                self.files_modified.join(", ")
            ));
        }

        if !self.decisions.is_empty() {
            md.push_str("• **Decisions & Actions**:\n");
            for d in &self.decisions {
                md.push_str(&format!("  - {}\n", d));
            }
        }

        if !self.errors_resolved.is_empty() {
            md.push_str("• **Errors Resolved**:\n");
            for e in &self.errors_resolved {
                md.push_str(&format!("  - {}\n", e));
            }
        }

        if !self.tools_used.is_empty() {
            let mut tool_strs: Vec<String> = self
                .tools_used
                .iter()
                .map(|(t, count)| format!("{t} ({count}x)"))
                .collect();
            tool_strs.sort();
            md.push_str(&format!("• **Tools Used**: {}\n", tool_strs.join(", ")));
        }

        if let Some(ref ccr) = self.ccr_id {
            md.push_str(&format!(
                "• **Lossless Turn Archive**: Call `retrieve_observation(id=\"{}\", query=\"...\")` to search or restore full pre-compaction turns.\n",
                ccr
            ));
        }

        md
    }
}

/// Rolling persistent session memory anchor injected into system prompts.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct MemoryAnchor {
    pub working_context: Option<String>,
    pub key_decisions: Vec<String>,
    pub file_state: IndexMap<String, String>,
    pub unresolved_errors: Vec<String>,
    /// Deterministic ledger of all workspace files read during the session (survives compaction).
    #[serde(default)]
    pub files_read: std::collections::BTreeSet<String>,
    /// Deterministic ledger of all workspace files modified/written during the session (survives compaction).
    #[serde(default)]
    pub files_modified: std::collections::BTreeSet<String>,
}

impl MemoryAnchor {
    /// Records a relative file path as read in the deterministic session tracker.
    pub fn record_file_read(&mut self, path: &str) {
        let clean = path.trim().trim_start_matches("./");
        if !clean.is_empty() && self.files_read.len() < 128 {
            self.files_read.insert(clean.to_string());
        }
    }

    /// Records a relative file path as modified in the deterministic session tracker.
    pub fn record_file_modified(&mut self, path: &str) {
        let clean = path.trim().trim_start_matches("./");
        if !clean.is_empty() && self.files_modified.len() < 128 {
            self.files_modified.insert(clean.to_string());
            self.file_state
                .insert(clean.to_string(), "modified".to_string());
            if self.file_state.len() > COMPACT_MAX_FILES_IN_ANCHOR {
                self.file_state.shift_remove_index(0);
            }
        }
    }

    /// Renders the anchor into a system prompt section if non-empty.
    pub fn to_prompt_block(&self) -> String {
        if self.working_context.is_none()
            && self.key_decisions.is_empty()
            && self.file_state.is_empty()
            && self.unresolved_errors.is_empty()
            && self.files_read.is_empty()
            && self.files_modified.is_empty()
        {
            return String::new();
        }

        let mut block = String::from("\n# Session Memory Anchor (Persistent Context):\n");

        if let Some(ref ctx) = self.working_context {
            block.push_str(&format!("- **Active Goal**: {}\n", ctx));
        }

        if !self.key_decisions.is_empty() {
            block.push_str("- **Key Decisions**:\n");
            for d in &self.key_decisions {
                block.push_str(&format!("  • {}\n", d));
            }
        }

        if !self.file_state.is_empty() {
            let mut entries: Vec<(&String, &String)> = self.file_state.iter().collect();
            entries.sort_by_key(|a| a.0);
            block.push_str("- **Tracked File State**:\n");
            for (f, state) in entries {
                block.push_str(&format!("  • `{}`: {}\n", f, state));
            }
        }

        if !self.files_read.is_empty() || !self.files_modified.is_empty() {
            block.push_str("<file-operations>\n");
            if !self.files_read.is_empty() {
                let list: Vec<&str> = self
                    .files_read
                    .iter()
                    .map(|s| s.as_str())
                    .take(40)
                    .collect();
                block.push_str(&format!(
                    "Read ({}): {}\n",
                    self.files_read.len(),
                    list.join(", ")
                ));
            }
            if !self.files_modified.is_empty() {
                let list: Vec<&str> = self
                    .files_modified
                    .iter()
                    .map(|s| s.as_str())
                    .take(40)
                    .collect();
                block.push_str(&format!(
                    "Modified ({}): {}\n",
                    self.files_modified.len(),
                    list.join(", ")
                ));
            }
            block.push_str("</file-operations>\n");
        }

        if !self.unresolved_errors.is_empty() {
            block.push_str("- **Unresolved Issues**:\n");
            for e in &self.unresolved_errors {
                block.push_str(&format!("  • {}\n", e));
            }
        }

        block
    }

    /// Formats the persistent session state as a structured Continuity Ledger Markdown document
    /// (`.minicode/ledgers/CONTINUITY.md`).
    pub fn to_continuity_ledger_markdown(&self, session_id: &str) -> String {
        let mut md = format!("# Session Continuity Ledger (`{}`)\n\n", session_id);
        md.push_str("## Goal\n");
        md.push_str(
            self.working_context
                .as_deref()
                .unwrap_or("*(active session)*"),
        );
        md.push_str("\n\n## Key Decisions & Progress\n");
        if self.key_decisions.is_empty() {
            md.push_str("- *(none recorded yet)*\n");
        } else {
            for d in &self.key_decisions {
                md.push_str(&format!("- {}\n", d));
            }
        }

        md.push_str("\n## File Operations\n<file-operations>\n");
        if self.files_read.is_empty() {
            md.push_str("Read: (none)\n");
        } else {
            let list: Vec<&str> = self.files_read.iter().map(|s| s.as_str()).collect();
            md.push_str(&format!("Read: {}\n", list.join(", ")));
        }
        if self.files_modified.is_empty() {
            md.push_str("Modified: (none)\n");
        } else {
            let list: Vec<&str> = self.files_modified.iter().map(|s| s.as_str()).collect();
            md.push_str(&format!("Modified: {}\n", list.join(", ")));
        }
        md.push_str("</file-operations>\n");

        if !self.unresolved_errors.is_empty() {
            md.push_str("\n## Unresolved Issues\n");
            for e in &self.unresolved_errors {
                md.push_str(&format!("- {}\n", e));
            }
        }
        md
    }
}

/// Statistics emitted when conversation history is compacted.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompactionMetrics {
    pub tier: usize,
    pub turns_summarized: usize,
    pub tokens_before: usize,
    pub tokens_after: usize,
    pub savings_percent: usize,
}

/// Smart 4-Tier Auto-Compaction Engine.
pub struct AutoCompactor {
    compressor: ContextCompressor,
    anchor: MemoryAnchor,
    model: String,
    custom_limit: Option<usize>,
    compaction_config: CompactionConfig,
}

impl AutoCompactor {
    /// Creates a new `AutoCompactor` for the specified LLM model.
    pub fn new(model: &str) -> Result<Self> {
        let compressor = ContextCompressor::new()?;
        Ok(Self {
            compressor,
            anchor: MemoryAnchor::default(),
            model: model.to_string(),
            custom_limit: None,
            compaction_config: CompactionConfig::default(),
        })
    }

    /// Infallible fallback constructor that never panics or fails.
    pub fn default_safe() -> Self {
        Self {
            compressor: ContextCompressor::default_safe(),
            anchor: MemoryAnchor::default(),
            model: "default".to_string(),
            custom_limit: None,
            compaction_config: CompactionConfig::default(),
        }
    }

    /// Overrides model context limit for testing or specific provider configurations.
    #[allow(dead_code)]
    #[must_use]
    pub fn with_custom_limit(mut self, limit: usize) -> Self {
        self.custom_limit = Some(limit);
        self
    }

    /// Updates the active model and optional custom context window limit while preserving the session memory anchor.
    pub fn update_model(&mut self, model: &str, custom_limit: Option<usize>) {
        self.model = model.to_string();
        self.custom_limit = custom_limit;
    }

    /// Returns the effective max context window length in tokens.
    pub fn model_token_limit(&self) -> usize {
        self.custom_limit
            .unwrap_or_else(|| get_model_context_limit(&self.model))
    }

    /// Counts total tokens for a slice of messages using the internal compressor.
    #[must_use]
    pub fn count_tokens(&self, messages: &[Message]) -> usize {
        self.compressor.count_messages_tokens(messages)
    }

    /// Returns a reference to the internal context compressor.
    #[allow(dead_code)]
    #[must_use]
    pub fn compressor(&self) -> &ContextCompressor {
        &self.compressor
    }

    pub fn anchor(&self) -> &MemoryAnchor {
        &self.anchor
    }

    #[allow(dead_code)]
    pub fn anchor_mut(&mut self) -> &mut MemoryAnchor {
        &mut self.anchor
    }

    /// Sets the high-level working goal in the persistent memory anchor.
    pub fn set_working_context(&mut self, goal: &str) {
        if !goal.trim().is_empty() {
            self.anchor.working_context = Some(goal.trim().to_string());
        }
    }

    /// Deterministically records file read/write operations from a completed tool execution.
    pub fn record_tool_file_ops(&mut self, tool_name: &str, args: &serde_json::Value) {
        if tool_name == "batch_read" {
            if let Some(paths) = args.get("paths").and_then(|v| v.as_array()) {
                for p in paths.iter().filter_map(|v| v.as_str()) {
                    self.anchor.record_file_read(p);
                }
            }
            return;
        }

        if let Some(p) =
            crate::context::budget::micro_compact::MicroCompactor::extract_target_path_from_args(
                args,
            )
        {
            if crate::context::budget::micro_compact::is_mutation_tool(tool_name) {
                self.anchor.record_file_modified(&p);
            } else if crate::context::budget::micro_compact::is_inspection_tool(tool_name) {
                self.anchor.record_file_read(&p);
            }
        }
    }

    /// Persists the Continuity Ledger (`.minicode/ledgers/CONTINUITY.md`) to disk so session
    /// goals, decisions, and `<file-operations>` survive compaction and session reloads.
    pub fn write_continuity_ledger(&self, workspace_root: &std::path::Path, session_id: &str) {
        let ledgers_dir = workspace_root.join(".minicode").join("ledgers");
        if std::fs::create_dir_all(&ledgers_dir).is_ok() {
            let md = self.anchor.to_continuity_ledger_markdown(session_id);
            let _ = std::fs::write(ledgers_dir.join("CONTINUITY.md"), &md);
            let short_id: String = session_id
                .chars()
                .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
                .take(16)
                .collect();
            if !short_id.is_empty() {
                let _ =
                    std::fs::write(ledgers_dir.join(format!("CONTINUITY_{}.md", short_id)), &md);
            }
        }
    }

    /// Algorithmic extraction of structured facts from a slice of messages without LLM calls.
    pub fn extract_turn_summary(
        messages: &[Message],
        turn_start: usize,
        turn_end: usize,
    ) -> TurnSummary {
        let mut summary = TurnSummary {
            turn_range: (turn_start, turn_end),
            ..Default::default()
        };

        let mut read_set: HashSet<String> = HashSet::new();
        let mut mod_set: HashSet<String> = HashSet::new();
        let mut raw_decision_candidates: Vec<(bool, String)> = Vec::new();
        let mut errors_vec: Vec<String> = Vec::new();
        let mut error_hashes: Vec<u64> = Vec::new();
        let mut raw_archive = String::new();

        for msg in messages {
            if !msg.content.trim().is_empty() {
                raw_archive.push_str(&format!("[{:?}]: {}\n", msg.role, msg.content.trim()));
            }
            // Count tool calls if available and extract target paths via canonical classifiers
            if let Some(ref calls) = msg.tool_calls {
                for c in calls {
                    *summary.tools_used.entry(c.name.clone()).or_insert(0) += 1;

                    if c.name == "batch_read" {
                        if let Some(paths) = c.arguments.get("paths").and_then(|v| v.as_array()) {
                            for p in paths.iter().filter_map(|v| v.as_str()) {
                                read_set.insert(p.to_string());
                            }
                        }
                    } else if let Some(p) =
                        crate::context::budget::micro_compact::MicroCompactor::extract_target_path(
                            c,
                        )
                    {
                        if crate::context::budget::micro_compact::is_mutation_tool(&c.name) {
                            mod_set.insert(p);
                        } else if crate::context::budget::micro_compact::is_inspection_tool(&c.name)
                        {
                            read_set.insert(p);
                        }
                    }
                }
            }

            match msg.role {
                Role::User | Role::Assistant => {
                    let clean_body = Self::strip_thought_tags(&msg.content);
                    for line in clean_body.lines() {
                        let clean = line
                            .trim()
                            .trim_start_matches('-')
                            .trim_start_matches('*')
                            .trim_start_matches('•')
                            .trim();
                        if clean.len() < 12 || clean.len() > COMPACT_MAX_DECISION_CHARS {
                            continue;
                        }
                        raw_decision_candidates.push((msg.role == Role::User, clean.to_string()));
                    }
                }
                Role::Tool => {
                    // Extract file read headers (e.g., "File Content (path):")
                    let is_file_read_payload = msg.content.starts_with("File Content (")
                        || msg
                            .tool_name
                            .as_deref()
                            .map(crate::context::budget::micro_compact::is_read_tool)
                            .unwrap_or(false);
                    if let Some(start) = msg.content.find("File Content (") {
                        if let Some(end) = msg.content[start..].find(')') {
                            let path = &msg.content[start + "File Content (".len()..start + end];
                            read_set.insert(path.trim().to_string());
                        }
                    }

                    // Extract diagnostic / error lines via SmartDonutTruncator + SimHash deduplication
                    if !is_file_read_payload {
                        for line in msg.content.lines() {
                            let trimmed = line.trim();
                            if !trimmed.is_empty()
                                && trimmed.len() <= COMPACT_MAX_ERROR_CHARS
                                && !trimmed.starts_with("Successfully ")
                                && crate::context::donut::SmartDonutTruncator::is_diagnostic_line(
                                    trimmed,
                                )
                            {
                                let h = crate::context::budget::headroom::AdaptiveSizer::simhash_64(
                                    trimmed,
                                );
                                if error_hashes.iter().all(|&existing| {
                                    crate::context::budget::headroom::AdaptiveSizer::hamming_distance(
                                        h, existing,
                                    ) > 6
                                }) {
                                    error_hashes.push(h);
                                    errors_vec.push(trimmed.to_string());
                                    if errors_vec.len() >= COMPACT_MAX_ERRORS_PER_TURN {
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
                Role::System => {}
            }
        }

        summary.files_read = read_set.into_iter().collect();
        summary.files_read.sort();

        summary.files_modified = mod_set.into_iter().collect();
        summary.files_modified.sort();

        // Build turn-corpus document frequency table for IDF-weighted decision scoring
        let total_candidates = raw_decision_candidates.len().max(1) as f64;
        let mut token_df: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        for (_, cand) in &raw_decision_candidates {
            let mut seen_in_line: HashSet<String> = HashSet::new();
            for tok in cand
                .split(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '-')
                .filter(|t| t.len() >= 3)
            {
                seen_in_line.insert(tok.to_ascii_lowercase());
            }
            for tok in seen_in_line {
                *token_df.entry(tok).or_insert(0) += 1;
            }
        }

        let mut scored_decisions: Vec<(usize, String)> = Vec::new();
        for (is_user, cand) in raw_decision_candidates {
            let score = Self::score_decision_candidate_with_idf(
                &cand,
                is_user,
                &token_df,
                total_candidates,
            );
            if score >= 6 {
                scored_decisions.push((score, cand));
            }
        }

        // Rank candidate decisions by score descending and deduplicate via 64-bit 4-gram SimHash
        scored_decisions.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
        let mut kept_decisions: Vec<String> = Vec::new();
        let mut kept_hashes: Vec<u64> = Vec::new();
        for (_, cand) in scored_decisions {
            let h = crate::context::budget::headroom::AdaptiveSizer::simhash_64(&cand);
            if kept_hashes.iter().all(|&existing| {
                crate::context::budget::headroom::AdaptiveSizer::hamming_distance(h, existing) > 6
            }) {
                kept_hashes.push(h);
                kept_decisions.push(cand);
                if kept_decisions.len() >= COMPACT_MAX_DECISIONS_PER_TURN {
                    break;
                }
            }
        }
        kept_decisions.sort();
        summary.decisions = kept_decisions;

        errors_vec.sort();
        summary.errors_resolved = errors_vec;

        if !raw_archive.is_empty() {
            summary.ccr_id = Some(crate::context::budget::ccr_cache::CcrCache::store(
                &raw_archive,
            ));
        }

        summary
    }

    /// Scores a user or assistant line for architectural decision / constraint significance
    /// by combining imperative stem weighting, turn-corpus token IDF rarity, Shannon character entropy,
    /// and structural code-symbol density.
    fn score_decision_candidate_with_idf(
        line: &str,
        is_user: bool,
        token_df: &std::collections::HashMap<String, usize>,
        total_docs: f64,
    ) -> usize {
        const DECISION_STEMS: &[(&str, usize)] = &[
            ("decisi", 6),
            ("plan:", 6),
            ("fix:", 6),
            ("chose", 5),
            ("replac", 4),
            ("refactor", 4),
            ("implement", 4),
            ("architect", 4),
            ("invariant", 5),
            ("contract", 4),
            ("must", 4),
            ("never", 4),
            ("always", 4),
            ("avoid", 4),
            ("instead", 4),
            ("because", 4),
            ("please", 4),
            ("let's", 4),
            ("do not", 5),
            ("don't", 5),
            ("use ", 4),
            ("wired", 4),
            ("resolved", 4),
        ];

        let lower = line.to_ascii_lowercase();
        let mut stem_score = 0usize;
        for (stem, weight) in DECISION_STEMS {
            if lower.contains(stem) {
                stem_score += *weight;
            }
        }
        if stem_score == 0 {
            return 0;
        }

        let mut score = stem_score + usize::from(is_user);

        // Mean token IDF across informative tokens (len >= 4)
        let mut idf_sum = 0.0f64;
        let mut tok_count = 0usize;
        for tok in lower
            .split(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '-')
            .filter(|t| t.len() >= 4)
        {
            let df = *token_df.get(tok).unwrap_or(&1) as f64;
            let idf = (1.0 + (total_docs - df + 0.5) / (df + 0.5)).ln();
            idf_sum += idf;
            tok_count += 1;
        }
        if tok_count > 0 {
            let mean_idf = idf_sum / (tok_count as f64);
            score += (mean_idf * 2.0).round() as usize;
        }

        // Shannon character entropy bonus (rewards information-rich technical prose over repetitive text)
        let entropy = crate::context::budget::headroom::SearchCompressor::shannon_entropy(line);
        if entropy >= 3.6 {
            score += 2;
        } else if entropy >= 3.0 {
            score += 1;
        }

        // Structural code-symbol or file-path density bonus
        if line.contains('`')
            || line.contains('/')
            || line.contains("::")
            || line.contains(".rs")
            || line.contains(".ts")
            || line.contains(".py")
        {
            score += 2;
        }
        score
    }

    /// Updates persistent memory anchor with newly summarized turn data.
    pub fn update_anchor_from_summary(&mut self, summary: &TurnSummary) {
        for r in &summary.files_read {
            self.anchor.record_file_read(r);
        }
        // Track file states (IndexMap preserves insertion order -> shift_remove_index(0) evicts oldest FIFO)
        for f in &summary.files_modified {
            self.anchor.record_file_modified(f);
        }

        // Track key decisions
        for d in &summary.decisions {
            if !self.anchor.key_decisions.contains(d) {
                self.anchor.key_decisions.push(d.clone());
                if self.anchor.key_decisions.len() > COMPACT_MAX_DECISIONS_IN_ANCHOR {
                    self.anchor.key_decisions.remove(0);
                }
            }
        }
    }

    pub const THOUGHT_TAG_PAIRS: [(&'static str, &'static str); 12] = [
        ("<thought>", "</thought>"),
        ("<think>", "</think>"),
        ("<thinking>", "</thinking>"),
        ("<reasoning>", "</reasoning>"),
        ("<antThinking>", "</antThinking>"),
        ("<scratchpad>", "</scratchpad>"),
        ("<Thought>", "</Thought>"),
        ("<Thinking>", "</Thinking>"),
        ("<THINK>", "</THINK>"),
        ("<minimax:tool_call>", "</minimax:tool_call>"),
        ("<tool_call>", "</tool_call>"),
        ("<tool_use>", "</tool_use>"),
    ];

    /// Checks if a string contains any recognized thinking or reasoning XML tags.
    #[must_use]
    pub fn has_thought_tags(content: &str) -> bool {
        content.contains('<')
            && Self::THOUGHT_TAG_PAIRS
                .iter()
                .any(|(open, _)| content.contains(open))
    }

    /// Single-pass $O(N)$ lexical span stripper for XML tag blocks (`<open>...</close>`).
    /// Avoids $O(N^2)$ `String::replace_range` buffer shifting by scanning forward once
    /// and copying only surviving slices into a pre-allocated output string.
    #[must_use]
    pub fn strip_xml_tag_blocks(
        content: &str,
        tag_pairs: &[(&str, &str)],
        strip_unclosed: bool,
    ) -> String {
        if !content.contains('<') {
            return content.trim().to_string();
        }
        let bytes = content.as_bytes();
        let len = bytes.len();
        let mut out = String::with_capacity(len);
        let mut copy_from = 0usize;
        let mut idx = 0usize;

        while idx < len {
            if bytes[idx] == b'<' {
                let tail = &content[idx..];
                if let Some(&(open_tag, close_tag)) =
                    tag_pairs.iter().find(|&&(open, _)| tail.starts_with(open))
                {
                    if copy_from < idx {
                        out.push_str(&content[copy_from..idx]);
                    }
                    let after_open = idx + open_tag.len();
                    if let Some(rel_end) = content[after_open..].find(close_tag) {
                        idx = after_open + rel_end + close_tag.len();
                        copy_from = idx;
                        continue;
                    } else if strip_unclosed {
                        copy_from = len;
                        break;
                    }
                }
            }
            idx += content[idx..]
                .chars()
                .next()
                .map(|c| c.len_utf8())
                .unwrap_or(1);
        }

        if copy_from < len {
            out.push_str(&content[copy_from..]);
        }
        out.trim().to_string()
    }

    /// Strips XML thinking tags (`<thought>`, `<think>`, `<thinking>`, `<reasoning>`, `<antThinking>`, `<scratchpad>`)
    /// from a content string in a single $O(N)$ pass.
    #[must_use]
    pub fn strip_thought_tags(content: &str) -> String {
        Self::strip_xml_tag_blocks(content, &Self::THOUGHT_TAG_PAIRS, true)
    }

    /// Strips bulky thought/reasoning blocks from older assistant turns,
    /// preserving thought metadata only on the most recent 2 assistant messages.
    /// This prevents O(N^2) context bloat while respecting model provider reasoning continuity.
    pub fn strip_older_reasoning(messages: &mut [Message]) {
        let mut assistant_seen = 0;
        for msg in messages.iter_mut().rev() {
            if msg.role == Role::Assistant {
                if assistant_seen < 2 {
                    assistant_seen += 1;
                } else {
                    msg.reasoning_content = None;
                    if Self::has_thought_tags(&msg.content) {
                        msg.content = Self::strip_thought_tags(&msg.content);
                    }
                }
            }
        }
    }

    /// Strips stale `<workspace_context>...</workspace_context>` blocks from older user turns
    /// in a single $O(N)$ pass, preserving the live workspace telemetry only on the most recent user message.
    pub fn strip_older_workspace_context(messages: &mut [Message]) {
        const WORKSPACE_TAG: &[(&str, &str)] = &[("<workspace_context>", "</workspace_context>")];
        let mut user_seen = 0usize;
        for msg in messages.iter_mut().rev() {
            if msg.role == Role::User {
                if user_seen < 1 {
                    user_seen += 1;
                } else if msg.content.contains("<workspace_context>") {
                    msg.content = Self::strip_xml_tag_blocks(&msg.content, WORKSPACE_TAG, false);
                }
            }
        }
    }

    /// Main compaction entrypoint: applies progressive 4-tier compaction if message history grows large.
    pub fn compact(
        &mut self,
        messages: &mut Vec<Message>,
        current_turn: usize,
    ) -> Option<CompactionMetrics> {
        // Strip older reasoning blocks and stale workspace snapshots across multi-turn sessions
        Self::strip_older_reasoning(messages);
        Self::strip_older_workspace_context(messages);

        let initial_tokens = self.compressor.count_messages_tokens(messages);
        let limit = self.model_token_limit();

        let tier1_threshold = (limit as f64 * self.compaction_config.tier1_ratio) as usize;
        let tier2_threshold = (limit as f64 * self.compaction_config.tier2_ratio) as usize;
        let tier3_threshold = (limit as f64 * self.compaction_config.tier3_ratio) as usize;

        // Invariant: Never compact if token count is safely below Tier 1 or message count cannot be partitioned
        if initial_tokens <= tier1_threshold || messages.len() <= 2 {
            return None;
        }

        // Scale preserve_count dynamically:
        // For short conversations (<= 6 messages) under pressure, preserve at most the 2 most recent messages;
        // for standard/longer conversations, preserve COMPACT_PRESERVE_RECENT_MESSAGES (6).
        let preserve_count = if messages.len() <= COMPACT_PRESERVE_RECENT_MESSAGES {
            2.min(messages.len().saturating_sub(2).max(1))
        } else {
            COMPACT_PRESERVE_RECENT_MESSAGES
        };
        let mut cutoff = messages.len().saturating_sub(preserve_count.max(1));

        // Snap cutoff backward across Role::Tool messages so the preserved recent slice
        // begins at the Role::Assistant message that issued those tool calls (or a Role::User message),
        // never separating tool results from their originating assistant tool_calls.
        while cutoff > 0 && messages[cutoff].role == Role::Tool {
            cutoff -= 1;
        }

        if cutoff == 0 {
            return None;
        }

        tracing::info!(
            initial_tokens = initial_tokens,
            limit = limit,
            tier1_threshold = tier1_threshold,
            tier2_threshold = tier2_threshold,
            tier3_threshold = tier3_threshold,
            "AutoCompactor: evaluating progressive context compaction"
        );

        // --- TIER 1: Semantic Micro-Compaction + Observation Masking on older tool messages ---
        let _ =
            crate::context::budget::micro_compact::MicroCompactor::compact_messages(messages, 2);
        for msg in messages.iter_mut().take(cutoff) {
            if msg.role == Role::Tool {
                msg.content = ContextCompressor::mask_observation(
                    &msg.content,
                    crate::constants::COMPRESSOR_MASK_LINES,
                );
            }
        }

        let tokens_after_tier1 = self.compressor.count_messages_tokens(messages);
        if tokens_after_tier1 <= tier2_threshold && tokens_after_tier1 < initial_tokens {
            return Some(Self::build_metrics(
                1,
                0,
                initial_tokens,
                tokens_after_tier1,
            ));
        }

        // --- TIER 2: Turn Group Summarization ---
        if tokens_after_tier1 > tier2_threshold && cutoff >= 2 {
            let turn_start = 1;
            let turn_end = current_turn.saturating_sub(1).max(1);

            // Extract structured summary from the older slice
            let summary = Self::extract_turn_summary(&messages[..cutoff], turn_start, turn_end);
            self.update_anchor_from_summary(&summary);

            let summary_markdown = summary.to_markdown();
            let summary_msg = Message::system(summary_markdown);

            // Replace the older slice with the single summary message
            let recent_slice: Vec<Message> = messages.drain(cutoff..).collect();
            messages.clear();
            messages.push(summary_msg);
            messages.extend(recent_slice);
            Self::strip_older_reasoning(messages);

            let tokens_after_tier2 = self.compressor.count_messages_tokens(messages);
            if tokens_after_tier2 <= tier3_threshold {
                return Some(Self::build_metrics(
                    2,
                    turn_end.saturating_sub(turn_start) + 1,
                    initial_tokens,
                    tokens_after_tier2,
                ));
            }
        }

        // --- TIER 3: Aggressive Anchor Synthesis & Message Pruning ---
        let mut current_tokens = self.compressor.count_messages_tokens(messages);
        if current_tokens > tier3_threshold && messages.len() > 4 {
            let mut pruned_count = 0;

            while messages.len() > 4 && current_tokens > tier3_threshold {
                // If message 0 is a system summary, remove message 1 instead of removing the summary
                let remove_idx = if messages[0].role == Role::System && messages.len() > 2 {
                    1
                } else {
                    0
                };

                let removed = messages.remove(remove_idx);
                // Harvest decisions and file ops from the evicted message into MemoryAnchor before dropping
                let single_summary = Self::extract_turn_summary(
                    std::slice::from_ref(&removed),
                    current_turn,
                    current_turn,
                );
                self.update_anchor_from_summary(&single_summary);
                let removed_tokens = self
                    .compressor
                    .count_messages_tokens(std::slice::from_ref(&removed));
                current_tokens = current_tokens.saturating_sub(removed_tokens);
                pruned_count += 1;
            }

            // Ensure no orphaned tool results at the beginning
            loop {
                let orphan_idx = if messages
                    .first()
                    .map(|m| m.role == Role::System)
                    .unwrap_or(false)
                {
                    if messages
                        .get(1)
                        .map(|m| m.role == Role::Tool)
                        .unwrap_or(false)
                    {
                        Some(1)
                    } else {
                        None
                    }
                } else if messages
                    .first()
                    .map(|m| m.role == Role::Tool)
                    .unwrap_or(false)
                {
                    Some(0)
                } else {
                    None
                };

                match orphan_idx {
                    Some(idx) if messages.len() > 1 => {
                        let removed = messages.remove(idx);
                        let removed_tokens = self
                            .compressor
                            .count_messages_tokens(std::slice::from_ref(&removed));
                        current_tokens = current_tokens.saturating_sub(removed_tokens);
                    }
                    _ => break,
                }
            }

            let final_tokens = self.compressor.count_messages_tokens(messages);
            return Some(Self::build_metrics(
                3,
                pruned_count,
                initial_tokens,
                final_tokens,
            ));
        }

        None
    }

    fn build_metrics(
        tier: usize,
        turns_summarized: usize,
        tokens_before: usize,
        tokens_after: usize,
    ) -> CompactionMetrics {
        let savings = tokens_before.saturating_sub(tokens_after);
        let savings_percent = if tokens_before > 0 {
            (savings * 100) / tokens_before
        } else {
            0
        };
        CompactionMetrics {
            tier,
            turns_summarized,
            tokens_before,
            tokens_after,
            savings_percent,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_turn_summary_heuristics() {
        let msgs = vec![
            Message::user("Please use thiserror for all custom errors"),
            Message::tool_result(
                "tc1",
                "read_file",
                "File Content (src/main.rs):\nfn main() {}\n",
            ),
            Message::assistant("I've analyzed main.rs.\n- Decision: Chose thiserror over anyhow"),
            Message::tool_result("tc2", "patch_file", "Successfully patched src/error.rs"),
            Message::user("Now let's check compilation"),
            Message::tool_result(
                "tc3",
                "exec_cmd",
                "error[E0308]: mismatched types in src/error.rs:42",
            ),
        ];

        let summary = AutoCompactor::extract_turn_summary(&msgs, 1, 3);
        assert!(summary.files_read.contains(&"src/main.rs".to_string()));
        assert!(summary.decisions.iter().any(|d| d.contains("thiserror")));
        assert!(summary
            .errors_resolved
            .iter()
            .any(|e| e.contains("error[E0308]")));
    }

    #[test]
    fn test_memory_anchor_prompt_block() {
        let mut anchor = MemoryAnchor::default();
        anchor.working_context = Some("Build context auto-compaction".to_string());
        anchor
            .key_decisions
            .push("Use 4-tier progressive compaction".to_string());
        anchor.file_state.insert(
            "src/context/auto_compact.rs".to_string(),
            "created".to_string(),
        );

        let block = anchor.to_prompt_block();
        assert!(block.contains("Session Memory Anchor"));
        assert!(block.contains("Active Goal"));
        assert!(block.contains("Build context auto-compaction"));
        assert!(block.contains("4-tier progressive compaction"));
        assert!(block.contains("src/context/auto_compact.rs"));
    }

    #[test]
    fn test_tier1_observation_masking() {
        let mut compactor = AutoCompactor::new("liquid/lfm-2.5-2.6b:free")
            .unwrap()
            .with_custom_limit(1000);

        let mut long_output = String::new();
        for i in 1..=50 {
            long_output.push_str(&format!("Diagnostic line {}\n", i));
        }

        let mut msgs = vec![
            Message::user("Inspect diagnostics"),
            Message::tool_result("tc1", "exec_cmd", long_output.clone()),
            Message::assistant("Inspecting line 1"),
            Message::tool_result("tc2", "exec_cmd", long_output.clone()),
            Message::user("Inspect diagnostics again"),
            Message::tool_result("tc3", "exec_cmd", long_output.clone()),
            Message::assistant("Recent message 1"),
            Message::user("Recent message 2"),
        ];

        let metrics = compactor.compact(&mut msgs, 2);
        assert!(metrics.is_some());
        let m = metrics.unwrap();
        assert_eq!(m.tier, 1);
        assert!(m.tokens_after < m.tokens_before);
    }

    #[test]
    fn test_strip_older_workspace_context_and_tool_call_compaction() {
        let mut msgs = vec![
            Message::user("<workspace_context>\nOld bulky context 1\n</workspace_context>\n\n<user_request>\nFirst prompt\n</user_request>"),
            Message::assistant("Done turn 1"),
            Message::user("<workspace_context>\nNew live context 2\n</workspace_context>\n\n<user_request>\nSecond prompt\n</user_request>"),
        ];

        AutoCompactor::strip_older_workspace_context(&mut msgs);
        assert_eq!(
            msgs[0].content,
            "<user_request>\nFirst prompt\n</user_request>"
        );
        assert!(msgs[2]
            .content
            .contains("<workspace_context>\nNew live context 2\n</workspace_context>"));
    }

    #[test]
    fn test_file_ops_tracker_and_continuity_ledger() {
        let mut compactor = AutoCompactor::default_safe();
        compactor.set_working_context("Implement native Rust file operations ledger");
        compactor.record_tool_file_ops(
            "batch_read",
            &serde_json::json!({ "paths": ["src/a.rs", "src/b.rs"] }),
        );
        compactor.record_tool_file_ops("patch_file", &serde_json::json!({ "path": "src/a.rs" }));

        let block = compactor.anchor().to_prompt_block();
        assert!(block.contains("<file-operations>"));
        assert!(block.contains("Read (2): src/a.rs, src/b.rs"));
        assert!(block.contains("Modified (1): src/a.rs"));

        let temp_dir =
            std::env::temp_dir().join(format!("minicode_ledger_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();
        compactor.write_continuity_ledger(&temp_dir, "sess-123456");

        let ledger_path = temp_dir.join(".minicode/ledgers/CONTINUITY.md");
        assert!(ledger_path.exists());
        let ledger_text = std::fs::read_to_string(&ledger_path).unwrap();
        assert!(ledger_text.contains("Implement native Rust file operations ledger"));
        assert!(ledger_text.contains("Read: src/a.rs, src/b.rs"));
        assert!(ledger_text.contains("Modified: src/a.rs"));

        std::fs::remove_dir_all(&temp_dir).ok();
    }
}

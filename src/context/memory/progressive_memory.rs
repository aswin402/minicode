use crate::constants::{
    CONFIG_DIR_NAME, GLOBAL_PROGRESSIVE_MEMORY_FILE, MAX_PROGRESSIVE_TIER_ENTRIES,
    MEMORY_DECAY_MILESTONE_HALF_LIFE_SECS, MEMORY_DECAY_STABILITY_STEP,
    MEMORY_DECAY_TRANSIENT_HALF_LIFE_SECS, PROGRESSIVE_MEMORY_FILE, WORKSPACE_DIR_NAME,
};
use crate::context::memory::decay::MemoryScope;
use crate::error::{ContextError, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{OnceLock, RwLock};

static MEMORY_CACHE: OnceLock<RwLock<HashMap<PathBuf, ProgressiveMemory>>> = OnceLock::new();

fn get_memory_cache() -> &'static RwLock<HashMap<PathBuf, ProgressiveMemory>> {
    MEMORY_CACHE.get_or_init(|| RwLock::new(HashMap::new()))
}

/// The 4 structural tiers of progressive memory
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum MemoryTier {
    /// L0: Ephemeral turn working scratchpad & active sub-goals
    L0Working,
    /// L1: Session-level milestone rollups and compaction anchors
    L1SessionAnchor,
    /// L2: Project-specific architectural facts & conventions (.minicode/progressive_memory.json)
    L2ProjectFact,
    /// L3: Global developer preferences & hardware constraints (~/.config/minicode/global_memory.json)
    L3GlobalPreference,
}

impl MemoryTier {
    #[allow(dead_code)]
    pub fn badge(&self) -> &'static str {
        match self {
            Self::L0Working => "[L0 Working]",
            Self::L1SessionAnchor => "[L1 Session]",
            Self::L2ProjectFact => "[L2 Project]",
            Self::L3GlobalPreference => "[L3 Global]",
        }
    }
    pub fn default_scope(self) -> MemoryScope {
        match self {
            Self::L3GlobalPreference => MemoryScope::Permanent,
            Self::L2ProjectFact | Self::L1SessionAnchor => MemoryScope::Milestone,
            Self::L0Working => MemoryScope::Transient,
        }
    }
}

/// An individual memory unit in the progressive memory hierarchy
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProgressiveMemoryEntry {
    pub id: String,
    pub tier: MemoryTier,
    pub key: String,
    pub value: String,
    pub confidence: f32,
    pub source: String,
    pub created_at: String,
    pub updated_at: String,
    pub access_count: usize,
    pub last_accessed: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<MemoryScope>,
}

impl ProgressiveMemoryEntry {
    pub fn new(
        tier: MemoryTier,
        key: impl Into<String>,
        value: impl Into<String>,
        confidence: f32,
        source: impl Into<String>,
    ) -> Self {
        let now = Utc::now().to_rfc3339();
        Self {
            id: format!("mem-{}", uuid::Uuid::new_v4().simple()),
            tier,
            key: key.into(),
            value: value.into(),
            confidence,
            source: source.into(),
            created_at: now.clone(),
            updated_at: now.clone(),
            access_count: 1,
            last_accessed: now,
            scope: Some(tier.default_scope()),
        }
    }

    /// Calculates current biological retention strength (0.0 to 1.0)
    pub fn calculate_retention(&self, current_time_secs: u64) -> f32 {
        let scope = self.scope.unwrap_or_else(|| self.tier.default_scope());

        if scope == MemoryScope::Permanent {
            return 1.0;
        }

        let last_accessed_secs = chrono::DateTime::parse_from_rfc3339(&self.last_accessed)
            .map(|dt| dt.timestamp() as u64)
            .unwrap_or(current_time_secs);

        let elapsed_secs = current_time_secs.saturating_sub(last_accessed_secs) as f32;
        let stability_factor = 1.0 + (self.access_count as f32 * MEMORY_DECAY_STABILITY_STEP);

        let half_life_secs = match scope {
            MemoryScope::Permanent => f32::MAX,
            MemoryScope::Milestone => MEMORY_DECAY_MILESTONE_HALF_LIFE_SECS,
            MemoryScope::Transient => MEMORY_DECAY_TRANSIENT_HALF_LIFE_SECS,
        };

        let effective_half_life = half_life_secs * stability_factor;
        let exponent = -std::f32::consts::LN_2 * (elapsed_secs / effective_half_life);
        exponent.exp().clamp(0.0, 1.0)
    }

    pub fn is_active(&self, current_time_secs: u64, threshold: f32) -> bool {
        self.calculate_retention(current_time_secs) >= threshold
    }
}

/// 4-Tier Progressive Memory Engine
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProgressiveMemory {
    #[serde(default)]
    pub l0_working: HashMap<String, String>,
    #[serde(default)]
    pub l1_session_anchors: Vec<ProgressiveMemoryEntry>,
    #[serde(default)]
    pub l2_project_facts: Vec<ProgressiveMemoryEntry>,
    #[serde(default)]
    pub l3_global_preferences: Vec<ProgressiveMemoryEntry>,
}

impl ProgressiveMemory {
    pub fn new() -> Self {
        Self::default()
    }

    /// Local project memory file: `<workspace>/.minicode/progressive_memory.json`
    pub fn local_path(workspace_root: &Path) -> PathBuf {
        workspace_root
            .join(WORKSPACE_DIR_NAME)
            .join(PROGRESSIVE_MEMORY_FILE)
    }

    /// Global developer preferences file: `~/.config/minicode/global_memory.json`
    pub fn global_path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join(CONFIG_DIR_NAME).join(GLOBAL_PROGRESSIVE_MEMORY_FILE))
    }

    /// Invalidates the in-memory cache for a given workspace root.
    #[allow(dead_code)]
    pub fn invalidate_cache(workspace_root: &Path) {
        let cache = get_memory_cache();
        if let Ok(mut guard) = cache.write() {
            guard.remove(workspace_root);
        }
    }

    /// Loads progressive memory from cache or disk (local workspace and global config directories).
    pub fn load(workspace_root: &Path) -> Self {
        let cache = get_memory_cache();
        if let Ok(guard) = cache.read() {
            if let Some(cached) = guard.get(workspace_root) {
                return cached.clone();
            }
        }

        let mut memory = Self::new();

        // 1. Load Local Project Facts (L2) and Session Anchors (L1)
        let local_file = Self::local_path(workspace_root);
        if let Ok(content) = fs::read_to_string(&local_file) {
            if let Ok(loaded) = serde_json::from_str::<ProgressiveMemory>(&content) {
                memory.l1_session_anchors = loaded.l1_session_anchors;
                memory.l2_project_facts = loaded.l2_project_facts;
            }
        }

        // 2. Load Global Developer Preferences (L3)
        if let Some(global_file) = Self::global_path() {
            if let Ok(content) = fs::read_to_string(&global_file) {
                if let Ok(entries) = serde_json::from_str::<Vec<ProgressiveMemoryEntry>>(&content) {
                    memory.l3_global_preferences = entries;
                }
            }
        }

        // Cache in memory for zero-IO subsequent reads
        if let Ok(mut guard) = cache.write() {
            guard.insert(workspace_root.to_path_buf(), memory.clone());
        }

        memory
    }

    /// Persists local (L1 + L2) memory to workspace and global (L3) memory to config directory
    pub fn save(&self, workspace_root: &Path) -> Result<()> {
        // Update in-memory cache first and synchronize L3 global preferences across cached workspaces
        let cache = get_memory_cache();
        if let Ok(mut guard) = cache.write() {
            for cached in guard.values_mut() {
                cached.l3_global_preferences = self.l3_global_preferences.clone();
            }
            guard.insert(workspace_root.to_path_buf(), self.clone());
        }

        // Save Local Memory (L1 + L2)
        let local_file = Self::local_path(workspace_root);
        if let Some(parent) = local_file.parent() {
            fs::create_dir_all(parent).map_err(|e| ContextError::Memory(e.to_string()))?;
        }

        let local_data = ProgressiveMemory {
            l0_working: HashMap::new(),
            l1_session_anchors: self.l1_session_anchors.clone(),
            l2_project_facts: self.l2_project_facts.clone(),
            l3_global_preferences: Vec::new(),
        };

        let json = serde_json::to_string_pretty(&local_data)
            .map_err(|e| ContextError::Memory(e.to_string()))?;
        Self::atomic_write(&local_file, &json)?;

        // Save Global Memory (L3)
        if let Some(global_file) = Self::global_path() {
            if let Some(parent) = global_file.parent() {
                fs::create_dir_all(parent).map_err(|e| ContextError::Memory(e.to_string()))?;
            }
            let global_json = serde_json::to_string_pretty(&self.l3_global_preferences)
                .map_err(|e| ContextError::Memory(e.to_string()))?;
            Self::atomic_write(&global_file, &global_json)?;
        }

        Ok(())
    }

    fn atomic_write(path: &Path, content: &str) -> Result<()> {
        let tmp_path = path.with_extension(format!(
            "tmp.{}.{}",
            std::process::id(),
            uuid::Uuid::new_v4().simple()
        ));
        fs::write(&tmp_path, content).map_err(|e| ContextError::Memory(e.to_string()))?;
        fs::rename(&tmp_path, path).map_err(|e| {
            let _ = fs::remove_file(&tmp_path);
            ContextError::Memory(e.to_string())
        })?;
        Ok(())
    }

    // === L0: Ephemeral Working Memory ===

    #[allow(dead_code)]
    pub fn set_l0(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.l0_working.insert(key.into(), value.into());
    }

    #[allow(dead_code)]
    pub fn clear_l0(&mut self) {
        self.l0_working.clear();
    }

    // === L1: Session Anchors ===

    pub fn record_l1_anchor(
        &mut self,
        summary: &str,
        files_modified: &[String],
        active_goal: &str,
        source: &str,
    ) {
        let files_str = if files_modified.is_empty() {
            "none".to_string()
        } else {
            files_modified.join(", ")
        };

        let value = format!(
            "Goal: {}\nSummary: {}\nModified Files: {}",
            active_goal, summary, files_str
        );

        // Deduplicate near-identical session anchors via 64-bit 4-gram SimHash
        let cand_hash = crate::context::budget::headroom::AdaptiveSizer::simhash_64(&value);
        if let Some(existing) = self.l1_session_anchors.iter_mut().find(|a| {
            let h = crate::context::budget::headroom::AdaptiveSizer::simhash_64(&a.value);
            crate::context::budget::headroom::AdaptiveSizer::hamming_distance(cand_hash, h) <= 6
        }) {
            let now = Utc::now().to_rfc3339();
            existing.value = value;
            existing.updated_at = now.clone();
            existing.last_accessed = now;
            existing.access_count += 1;
            return;
        }

        let entry = ProgressiveMemoryEntry::new(
            MemoryTier::L1SessionAnchor,
            format!(
                "anchor-{}-{}",
                Utc::now().format("%H%M%S"),
                &uuid::Uuid::new_v4().simple().to_string()[..4]
            ),
            value,
            1.0,
            source,
        );

        self.l1_session_anchors.push(entry);
        if self.l1_session_anchors.len() > MAX_PROGRESSIVE_TIER_ENTRIES {
            self.l1_session_anchors.remove(0);
        }
    }

    // === L2: Project Facts ===

    pub fn add_l2_fact(
        &mut self,
        key: impl Into<String>,
        value: impl Into<String>,
        source: &str,
        confidence: f32,
    ) {
        let k = key.into();
        let v = value.into();
        let new_hash = crate::context::budget::headroom::AdaptiveSizer::simhash_64(&v);

        if let Some(existing) = self.l2_project_facts.iter_mut().find(|e| {
            e.key == k
                || crate::context::budget::headroom::AdaptiveSizer::hamming_distance(
                    new_hash,
                    crate::context::budget::headroom::AdaptiveSizer::simhash_64(&e.value),
                ) <= 6
        }) {
            let now = Utc::now().to_rfc3339();
            existing.value = v;
            existing.updated_at = now.clone();
            existing.last_accessed = now;
            existing.confidence = confidence;
            existing.access_count += 1;
            return;
        }

        let entry =
            ProgressiveMemoryEntry::new(MemoryTier::L2ProjectFact, k, v, confidence, source);
        self.l2_project_facts.push(entry);

        if self.l2_project_facts.len() > MAX_PROGRESSIVE_TIER_ENTRIES {
            self.l2_project_facts.remove(0);
        }
    }

    // === L3: Global Developer Preferences ===

    pub fn add_l3_preference(
        &mut self,
        key: impl Into<String>,
        value: impl Into<String>,
        source: &str,
    ) {
        let k = key.into();
        let v = value.into();
        let new_hash = crate::context::budget::headroom::AdaptiveSizer::simhash_64(&v);

        if let Some(existing) = self.l3_global_preferences.iter_mut().find(|e| {
            e.key == k
                || crate::context::budget::headroom::AdaptiveSizer::hamming_distance(
                    new_hash,
                    crate::context::budget::headroom::AdaptiveSizer::simhash_64(&e.value),
                ) <= 6
        }) {
            let now = Utc::now().to_rfc3339();
            existing.value = v;
            existing.updated_at = now.clone();
            existing.last_accessed = now;
            existing.access_count += 1;
            return;
        }

        let entry = ProgressiveMemoryEntry::new(MemoryTier::L3GlobalPreference, k, v, 1.0, source);
        self.l3_global_preferences.push(entry);
        if self.l3_global_preferences.len() > MAX_PROGRESSIVE_TIER_ENTRIES {
            self.l3_global_preferences.remove(0);
        }
    }

    /// Prunes decayed facts from L1 and L2 whose retention is below `threshold` (e.g. 0.15).
    pub fn prune_decayed(&mut self, threshold: f32) {
        let now = chrono::Utc::now().timestamp() as u64;
        self.l1_session_anchors
            .retain(|a| a.is_active(now, threshold));
        self.l2_project_facts
            .retain(|f| f.is_active(now, threshold));
    }

    /// Deletes a fact by key across project facts and global preferences.
    pub fn forget_fact(&mut self, key: &str) -> bool {
        let before_l2 = self.l2_project_facts.len();
        self.l2_project_facts.retain(|f| f.key != key);
        let removed_l2 = self.l2_project_facts.len() < before_l2;

        let before_l3 = self.l3_global_preferences.len();
        self.l3_global_preferences.retain(|p| p.key != key);
        let removed_l3 = self.l3_global_preferences.len() < before_l3;

        removed_l2 || removed_l3
    }

    /// Updates an existing fact's value across project facts and global preferences.
    pub fn update_fact(&mut self, key: &str, new_value: &str) -> bool {
        let now = chrono::Utc::now().to_rfc3339();
        let mut updated = false;
        for f in &mut self.l2_project_facts {
            if f.key == key {
                f.value = new_value.to_string();
                f.updated_at = now.clone();
                f.last_accessed = now.clone();
                f.access_count += 1;
                updated = true;
            }
        }
        for p in &mut self.l3_global_preferences {
            if p.key == key {
                p.value = new_value.to_string();
                p.updated_at = now.clone();
                p.last_accessed = now.clone();
                p.access_count += 1;
                updated = true;
            }
        }
        updated
    }

    /// Strips all reasoning/thought XML blocks (`<thought>`, `<think>`, `<thinking>`, `<reasoning>`, `<antThinking>`, `<scratchpad>`)
    /// in a single $O(N)$ pass so internal scratchpad lines are never scraped into persistent memory.
    fn strip_thought_blocks(text: &str) -> String {
        crate::context::auto_compact::AutoCompactor::strip_thought_tags(text)
    }

    /// Algorithmic directive and fact classifier that evaluates modal imperative density,
    /// structural `Prefix: Value` headers, and domain token affinity to route statements
    /// into L3 Global Preferences or L2 Project Facts without hardcoded developer strings.
    pub fn extract_and_store_facts(&mut self, text: &str, source: &str) {
        let is_raw_turn_stream = source.starts_with("turn_") && !source.ends_with("_recovery");
        if is_raw_turn_stream {
            return;
        }
        let cleaned = Self::strip_thought_blocks(text);
        for line in cleaned.lines() {
            let trimmed = line
                .trim()
                .trim_start_matches('-')
                .trim_start_matches('*')
                .trim_start_matches('•')
                .trim();
            if trimmed.len() < 8 || trimmed.len() > 300 || trimmed.ends_with('?') {
                continue;
            }

            if let Some((tier, key, val, confidence)) = Self::classify_memory_line(trimmed) {
                match tier {
                    MemoryTier::L3GlobalPreference => {
                        self.add_l3_preference(key, val, source);
                    }
                    MemoryTier::L2ProjectFact => {
                        self.add_l2_fact(key, val, source, confidence);
                    }
                    _ => {}
                }
            }
        }
    }

    fn classify_memory_line(trimmed: &str) -> Option<(MemoryTier, String, String, f32)> {
        const MODAL_STEMS: &[(&str, usize)] = &[
            ("always", 4),
            ("alway", 4),
            ("never", 4),
            ("must", 4),
            ("do not", 4),
            ("don't", 4),
            ("dont", 4),
            ("prefer", 3),
            ("avoid", 3),
            ("mandatory", 4),
            ("strictly", 4),
        ];

        const L3_DOMAINS: &[(&str, &[(&str, usize)])] = &[
            (
                "resource_limit",
                &[
                    ("-j 1", 6),
                    ("-j1", 6),
                    ("nice -n", 5),
                    ("resource", 4),
                    ("constrain", 4),
                    ("full test", 4),
                    ("cpu", 3),
                    ("ram", 3),
                    ("memory limit", 5),
                ],
            ),
            (
                "release_workflow",
                &[
                    ("commit", 3),
                    ("push", 3),
                    ("tag", 3),
                    ("origin", 3),
                    ("main", 2),
                    ("release", 3),
                    ("version", 3),
                ],
            ),
            (
                "changelog_structure",
                &[
                    ("changelog", 5),
                    ("inspir", 4),
                    ("insper", 4),
                    ("ideas", 3),
                    ("source", 3),
                    ("reference", 3),
                ],
            ),
            (
                "verification_workflow",
                &[
                    ("clippy", 4),
                    ("cargo fmt", 4),
                    ("-d warnings", 5),
                    ("tsc --noemit", 5),
                    ("lint", 3),
                ],
            ),
        ];

        const FACT_PREFIXES: &[&str] = &[
            "rule",
            "note",
            "fact",
            "invariant",
            "convention",
            "constraint",
            "architecture",
        ];

        let lower = trimmed.to_ascii_lowercase();
        let modal_score: usize = MODAL_STEMS
            .iter()
            .filter(|(stem, _)| lower.contains(*stem))
            .map(|(_, w)| *w)
            .sum();

        // 1. Evaluate L3 Global Preference domain affinity
        let mut best_l3: Option<(&str, usize)> = None;
        for (domain_key, stems) in L3_DOMAINS {
            let domain_score: usize = stems
                .iter()
                .filter(|(stem, _)| lower.contains(*stem))
                .map(|(_, w)| *w)
                .sum();
            if domain_score > 0 {
                let combined = domain_score + modal_score;
                if combined >= 6 && best_l3.map(|(_, s)| combined > s).unwrap_or(true) {
                    best_l3 = Some((*domain_key, combined));
                }
            }
        }
        if let Some((domain_key, _)) = best_l3 {
            let clean_val = trimmed
                .split_once(':')
                .and_then(|(p, rest)| {
                    if FACT_PREFIXES.contains(&p.trim().to_ascii_lowercase().as_str()) {
                        Some(rest.trim())
                    } else {
                        None
                    }
                })
                .unwrap_or(trimmed);
            return Some((
                MemoryTier::L3GlobalPreference,
                domain_key.to_string(),
                clean_val.to_string(),
                1.0,
            ));
        }

        // 2. Evaluate explicit structural `Prefix: Value` L2 Project Facts
        if let Some((prefix, rest)) = trimmed.split_once(':') {
            let base = prefix.trim().to_ascii_lowercase();
            let val = rest.trim();
            if FACT_PREFIXES.contains(&base.as_str()) && !val.is_empty() {
                let slug = Self::slugify_tokens(val, 4);
                let key = if slug.is_empty() {
                    base
                } else {
                    format!("{}_{}", base, slug)
                };
                return Some((MemoryTier::L2ProjectFact, key, val.to_string(), 0.9));
            }
        }

        // 3. Evaluate unlabelled high-confidence architectural invariants (modal directive + code symbol/path)
        let has_code_anchor = trimmed.contains('`')
            || lower.contains(".rs")
            || lower.contains(".ts")
            || lower.contains(".py")
            || lower.contains("src/");
        if modal_score >= 4 && has_code_anchor {
            let slug = Self::slugify_tokens(trimmed, 4);
            if !slug.is_empty() {
                return Some((
                    MemoryTier::L2ProjectFact,
                    format!("invariant_{}", slug),
                    trimmed.to_string(),
                    0.85,
                ));
            }
        }

        None
    }

    fn slugify_tokens(text: &str, max_words: usize) -> String {
        const STOP_WORDS: &[&str] = &[
            "in", "inside", "into", "on", "at", "to", "for", "of", "with", "by", "from", "the",
            "and", "or",
        ];
        text.split_whitespace()
            .map(|w| {
                w.chars()
                    .filter(|c| c.is_ascii_alphanumeric())
                    .collect::<String>()
                    .to_ascii_lowercase()
            })
            .filter(|s| s.len() >= 2 && !STOP_WORDS.contains(&s.as_str()))
            .take(max_words)
            .collect::<Vec<_>>()
            .join("_")
    }

    /// Formats the progressive memory hierarchy into structured prompt context
    pub fn to_prompt_block(&self) -> String {
        let mut out = String::new();

        if self.l3_global_preferences.is_empty()
            && self.l2_project_facts.is_empty()
            && self.l1_session_anchors.is_empty()
            && self.l0_working.is_empty()
        {
            return out;
        }

        out.push_str("<progressive_memory>\n");

        // Tier 3: Global Preferences
        if !self.l3_global_preferences.is_empty() {
            out.push_str("  <tier3_global_preferences>\n");
            for p in &self.l3_global_preferences {
                out.push_str(&format!("    • [Global Rule] {}: {}\n", p.key, p.value));
            }
            out.push_str("  </tier3_global_preferences>\n");
        }

        // Tier 2: Project Knowledge Base (SimHash deduplicated)
        if !self.l2_project_facts.is_empty() {
            out.push_str("  <tier2_project_facts>\n");
            let mut seen_hashes: Vec<u64> = Vec::new();
            for f in &self.l2_project_facts {
                let h = crate::context::budget::headroom::AdaptiveSizer::simhash_64(&f.value);
                if seen_hashes.iter().all(|&existing| {
                    crate::context::budget::headroom::AdaptiveSizer::hamming_distance(h, existing)
                        > 6
                }) {
                    seen_hashes.push(h);
                    out.push_str(&format!("    • [Project Fact] {}: {}\n", f.key, f.value));
                }
            }
            out.push_str("  </tier2_project_facts>\n");
        }

        // Tier 1: Session Anchors (SimHash deduplicated)
        if !self.l1_session_anchors.is_empty() {
            out.push_str("  <tier1_session_anchors>\n");
            let mut seen_hashes: Vec<u64> = Vec::new();
            let mut count = 0usize;
            for a in self.l1_session_anchors.iter().rev() {
                let h = crate::context::budget::headroom::AdaptiveSizer::simhash_64(&a.value);
                if seen_hashes.iter().all(|&existing| {
                    crate::context::budget::headroom::AdaptiveSizer::hamming_distance(h, existing)
                        > 6
                }) {
                    seen_hashes.push(h);
                    out.push_str(&format!("    • {}\n", a.value.replace('\n', " | ")));
                    count += 1;
                    if count >= 3 {
                        break;
                    }
                }
            }
            out.push_str("  </tier1_session_anchors>\n");
        }

        // Tier 0: Ephemeral Working Memory
        if !self.l0_working.is_empty() {
            out.push_str("  <tier0_working_memory>\n");
            for (k, v) in &self.l0_working {
                out.push_str(&format!("    • {}: {}\n", k, v));
            }
            out.push_str("  </tier0_working_memory>\n");
        }

        out.push_str("</progressive_memory>\n");
        out
    }

    /// Queries all memory tiers matching search terms
    #[allow(dead_code)]
    pub fn query(&self, term: &str, max_results: usize) -> Vec<&ProgressiveMemoryEntry> {
        let term_lower = term.to_lowercase();
        let mut results = Vec::new();

        for e in self
            .l3_global_preferences
            .iter()
            .chain(&self.l2_project_facts)
            .chain(&self.l1_session_anchors)
        {
            if e.key.to_lowercase().contains(&term_lower)
                || e.value.to_lowercase().contains(&term_lower)
            {
                results.push(e);
                if results.len() >= max_results {
                    break;
                }
            }
        }

        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_and_store_facts_ignores_thought_blocks_and_turn_note_monologue() {
        let mut mem = ProgressiveMemory::default();
        let turn_stream = "<thought>\nNote: `section#interfaces > div` is 20px top, 360px wide — fine.\nRule: I'll check the next element now.\n</thought>\nNote: Wait — let's check the inner grid inside `section#interfaces`! I'll inspect its children.";
        mem.extract_and_store_facts(turn_stream, "turn_1");
        assert!(mem.l2_project_facts.is_empty());

        mem.extract_and_store_facts(
            "Note: `browser_navigate` failed with connection refused and was resolved by `minitask`.",
            "turn_1_recovery",
        );
        assert_eq!(mem.l2_project_facts.len(), 1);
    }

    #[test]
    fn test_algorithmic_directive_and_fact_classification_with_simhash_dedup() {
        let mut mem = ProgressiveMemory::default();
        let compaction_notes = "<think>Internal reasoning</think>\n\
            - Always pass -j 1 to cargo commands on resource-constrained machines\n\
            - Always commit, tag, and push to origin main on release\n\
            - Never use `.unwrap()` inside `src/context/budget/auto_compact.rs`\n\
            - Never use `.unwrap()` in `src/context/budget/auto_compact.rs`";
        mem.extract_and_store_facts(compaction_notes, "compaction_summary");

        assert!(mem
            .l3_global_preferences
            .iter()
            .any(|p| p.key == "resource_limit"));
        assert!(mem
            .l3_global_preferences
            .iter()
            .any(|p| p.key == "release_workflow"));
        // Near-duplicate architectural invariants should be deduplicated by SimHash into 1 L2 entry
        assert_eq!(mem.l2_project_facts.len(), 1);
    }
}

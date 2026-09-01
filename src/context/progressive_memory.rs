use crate::constants::{
    CONFIG_DIR_NAME, GLOBAL_PROGRESSIVE_MEMORY_FILE, MAX_PROGRESSIVE_TIER_ENTRIES,
    PROGRESSIVE_MEMORY_FILE, WORKSPACE_DIR_NAME,
};
use crate::error::{ContextError, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

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
        }
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

    /// Loads progressive memory from local workspace and global config directories
    pub fn load(workspace_root: &Path) -> Self {
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

        memory
    }

    /// Persists local (L1 + L2) memory to workspace and global (L3) memory to config directory
    pub fn save(&self, workspace_root: &Path) -> Result<()> {
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

        let entry = ProgressiveMemoryEntry::new(
            MemoryTier::L1SessionAnchor,
            format!("anchor-{}", Utc::now().format("%H%M%S")),
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

        if let Some(existing) = self.l2_project_facts.iter_mut().find(|e| e.key == k) {
            existing.value = v;
            existing.updated_at = Utc::now().to_rfc3339();
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

        if let Some(existing) = self.l3_global_preferences.iter_mut().find(|e| e.key == k) {
            existing.value = v;
            existing.updated_at = Utc::now().to_rfc3339();
            existing.access_count += 1;
            return;
        }

        let entry = ProgressiveMemoryEntry::new(MemoryTier::L3GlobalPreference, k, v, 1.0, source);
        self.l3_global_preferences.push(entry);
    }

    /// Automatically extracts facts, guidelines, and tool preferences from text/compaction
    pub fn extract_and_store_facts(&mut self, text: &str, source: &str) {
        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            // Detect rule / preference patterns
            let lower = trimmed.to_lowercase();
            if lower.contains("always pass -j 1")
                || lower.contains("dont run the full test")
                || lower.contains("resource-constrained")
            {
                self.add_l3_preference(
                    "resource_limit",
                    "Laptop resource constrained: ALWAYS pass -j 1 to all cargo commands",
                    source,
                );
            } else if lower.contains("alway commit add push to main")
                || lower.contains("tag and push")
            {
                self.add_l3_preference(
                    "release_workflow",
                    "On version bump: always commit, tag with vX.Y.Z, and push to origin main --tags",
                    source,
                );
            } else if trimmed.starts_with("Rule:")
                || trimmed.starts_with("Note:")
                || trimmed.starts_with("Fact:")
            {
                let parts: Vec<&str> = trimmed.splitn(2, ':').collect();
                if parts.len() == 2 {
                    let key = parts[0].trim().to_lowercase();
                    let val = parts[1].trim();
                    self.add_l2_fact(key, val, source, 0.9);
                }
            }
        }
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

        // Tier 2: Project Knowledge Base
        if !self.l2_project_facts.is_empty() {
            out.push_str("  <tier2_project_facts>\n");
            for f in &self.l2_project_facts {
                out.push_str(&format!("    • [Project Fact] {}: {}\n", f.key, f.value));
            }
            out.push_str("  </tier2_project_facts>\n");
        }

        // Tier 1: Session Anchors
        if !self.l1_session_anchors.is_empty() {
            out.push_str("  <tier1_session_anchors>\n");
            for a in self.l1_session_anchors.iter().rev().take(3) {
                out.push_str(&format!("    • {}\n", a.value.replace('\n', " | ")));
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

use crate::constants::{CONFIG_DIR_NAME, MEMORY_FILE, WORKSPACE_DIR_NAME};
use crate::error::{ContextError, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MemoryCategory {
    Preference,
    ProjectFact,
    Pattern,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemoryEntry {
    pub key: String,
    pub value: String,
    pub category: MemoryCategory,
    pub created_at: String,
    pub updated_at: String,
}

impl MemoryEntry {
    pub fn new(key: impl Into<String>, value: impl Into<String>, category: MemoryCategory) -> Self {
        let now = Utc::now().to_rfc3339();
        Self {
            key: key.into(),
            value: value.into(),
            category,
            created_at: now.clone(),
            updated_at: now,
        }
    }
}

/// Persistent 2-Tier Core Memory.
/// - Global: `~/.config/minicode/memory.json` (developer-wide preferences & rules)
/// - Local: `.minicode/memory.json` (workspace-specific architectural facts)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CoreMemory {
    pub global_entries: Vec<MemoryEntry>,
    pub local_entries: Vec<MemoryEntry>,
}

impl CoreMemory {
    /// Returns the global memory file path `~/.config/minicode/memory.json`
    pub fn global_path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join(CONFIG_DIR_NAME).join(MEMORY_FILE))
    }

    /// Returns the local workspace memory file path `.minicode/memory.json`
    pub fn local_path(workspace_root: &Path) -> PathBuf {
        workspace_root.join(WORKSPACE_DIR_NAME).join(MEMORY_FILE)
    }

    /// Loads and merges both global and local core memory
    pub fn load(workspace_root: &Path) -> Self {
        let global_entries = Self::load_from_path(Self::global_path().as_deref());
        let local_entries = Self::load_from_path(Some(&Self::local_path(workspace_root)));

        Self {
            global_entries,
            local_entries,
        }
    }

    fn load_from_path(path: Option<&Path>) -> Vec<MemoryEntry> {
        let Some(p) = path else {
            return Vec::new();
        };

        match fs::read_to_string(p) {
            Ok(content) => match serde_json::from_str::<Vec<MemoryEntry>>(&content) {
                Ok(entries) => entries,
                Err(e) => {
                    tracing::warn!(path = %p.display(), error = %e, "Corrupt memory file encountered; ignoring");
                    Vec::new()
                }
            },
            Err(e) => {
                if e.kind() != ErrorKind::NotFound {
                    tracing::warn!(path = %p.display(), error = %e, "Failed to read memory file");
                }
                Vec::new()
            }
        }
    }

    /// Atomically writes content to a file via a temporary sibling file and rename
    fn atomic_write(path: &Path, content: &str) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| ContextError::Memory(e.to_string()))?;
        }
        let tmp_path = path.with_extension(format!(
            "tmp.{}.{}",
            std::process::id(),
            uuid::Uuid::new_v4().simple()
        ));
        fs::write(&tmp_path, content).map_err(|e| ContextError::Memory(e.to_string()))?;
        if let Err(e) = fs::rename(&tmp_path, path) {
            if let Err(cleanup_err) = fs::remove_file(&tmp_path) {
                tracing::warn!(
                    path = %tmp_path.display(),
                    error = %cleanup_err,
                    "Failed to clean up temporary memory file after rename failure"
                );
            }
            return Err(ContextError::Memory(e.to_string()).into());
        }
        Ok(())
    }

    /// Saves local entries to `.minicode/memory.json`
    pub fn save_local(&self, workspace_root: &Path) -> Result<()> {
        let path = Self::local_path(workspace_root);
        let json = serde_json::to_string_pretty(&self.local_entries)
            .map_err(|e| ContextError::Memory(e.to_string()))?;
        Self::atomic_write(&path, &json)
    }

    /// Saves global entries to `~/.config/minicode/memory.json`
    pub fn save_global(&self) -> Result<()> {
        let Some(path) = Self::global_path() else {
            return Err(ContextError::Memory("Global config directory not resolved".into()).into());
        };
        let json = serde_json::to_string_pretty(&self.global_entries)
            .map_err(|e| ContextError::Memory(e.to_string()))?;
        Self::atomic_write(&path, &json)
    }

    /// Formats the memory as an in-context XML `<core_memory>` prompt block (~150-250 tokens)
    pub fn to_prompt_block(&self) -> String {
        if self.global_entries.is_empty() && self.local_entries.is_empty() {
            return String::new();
        }

        let mut block = String::with_capacity(512);
        block.push_str("<core_memory>\n");

        if !self.global_entries.is_empty() {
            block.push_str("# Developer Preferences (Global):\n");
            for entry in &self.global_entries {
                block.push_str(&format!("- {}: {}\n", entry.key, entry.value));
            }
        }

        if !self.local_entries.is_empty() {
            if !self.global_entries.is_empty() {
                block.push('\n');
            }
            block.push_str("# Project Architecture & Facts (Local):\n");
            for entry in &self.local_entries {
                block.push_str(&format!("- {}: {}\n", entry.key, entry.value));
            }
        }

        block.push_str("</core_memory>");
        block
    }

    /// Adds or updates a memory fact/preference
    pub fn remember(
        &mut self,
        workspace_root: &Path,
        key: &str,
        value: &str,
        is_global: bool,
        category: MemoryCategory,
    ) -> Result<()> {
        if is_global {
            self.local_entries.retain(|e| e.key != key);
            if let Some(existing) = self.global_entries.iter_mut().find(|e| e.key == key) {
                existing.value = value.to_string();
                existing.updated_at = Utc::now().to_rfc3339();
            } else {
                self.global_entries
                    .push(MemoryEntry::new(key, value, category));
            }
            self.save_global()?;
            if let Err(e) = self.save_local(workspace_root) {
                tracing::warn!(error = %e, "Failed to save complementary local memory (non-fatal)");
            }
        } else {
            self.global_entries.retain(|e| e.key != key);
            if let Some(existing) = self.local_entries.iter_mut().find(|e| e.key == key) {
                existing.value = value.to_string();
                existing.updated_at = Utc::now().to_rfc3339();
            } else {
                self.local_entries
                    .push(MemoryEntry::new(key, value, category));
            }
            self.save_local(workspace_root)?;
            if let Err(e) = self.save_global() {
                tracing::warn!(error = %e, "Failed to save complementary global memory (non-fatal)");
            }
        }

        Ok(())
    }

    /// Updates an existing memory fact/preference in either local or global store
    pub fn update(&mut self, workspace_root: &Path, key: &str, new_value: &str) -> Result<bool> {
        let now = Utc::now().to_rfc3339();
        if let Some(entry) = self.local_entries.iter_mut().find(|e| e.key == key) {
            entry.value = new_value.to_string();
            entry.updated_at = now;
            self.save_local(workspace_root)?;
            return Ok(true);
        }
        if let Some(entry) = self.global_entries.iter_mut().find(|e| e.key == key) {
            entry.value = new_value.to_string();
            entry.updated_at = now;
            self.save_global()?;
            return Ok(true);
        }
        Ok(false)
    }

    /// Removes a memory entry by key from local and/or global memory
    pub fn forget(&mut self, workspace_root: &Path, key: &str) -> Result<bool> {
        let initial_local = self.local_entries.len();
        self.local_entries.retain(|e| e.key != key);
        let removed_local = self.local_entries.len() < initial_local;
        if removed_local {
            self.save_local(workspace_root)?;
        }

        let initial_global = self.global_entries.len();
        self.global_entries.retain(|e| e.key != key);
        let removed_global = self.global_entries.len() < initial_global;
        if removed_global {
            self.save_global()?;
        }

        Ok(removed_local || removed_global)
    }

    /// Lists all active memories formatted for user inspection
    #[allow(dead_code)]
    pub fn list_all(&self) -> Vec<MemoryEntry> {
        let mut all = self.global_entries.clone();
        all.extend(self.local_entries.clone());
        all
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_core_memory_crud() {
        let root = std::env::temp_dir().join(format!("minicode_test_mem_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();

        let mut memory = CoreMemory::default();
        memory
            .remember(
                &root,
                "project_type",
                "Rust CLI agent",
                false,
                MemoryCategory::ProjectFact,
            )
            .unwrap();

        assert_eq!(memory.local_entries.len(), 1);
        assert_eq!(memory.local_entries[0].key, "project_type");
        assert_eq!(memory.local_entries[0].value, "Rust CLI agent");

        // Reload from disk
        let loaded = CoreMemory::load(&root);
        assert_eq!(loaded.local_entries.len(), 1);
        assert_eq!(loaded.local_entries[0].key, "project_type");

        // Update
        let updated = memory
            .update(&root, "project_type", "Rust TUI/CLI agent")
            .unwrap();
        assert!(updated);
        assert_eq!(memory.local_entries.len(), 1);
        assert_eq!(memory.local_entries[0].value, "Rust TUI/CLI agent");

        // Forget
        let removed = memory.forget(&root, "project_type").unwrap();
        assert!(removed);
        assert!(memory.local_entries.is_empty());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_core_memory_prompt_block() {
        let mut memory = CoreMemory::default();
        memory.global_entries.push(MemoryEntry::new(
            "code_style",
            "Always use pure Rust",
            MemoryCategory::Preference,
        ));
        memory.local_entries.push(MemoryEntry::new(
            "database",
            "Sqlite + Turso",
            MemoryCategory::ProjectFact,
        ));

        let block = memory.to_prompt_block();
        assert!(block.contains("<core_memory>"));
        assert!(block.contains("code_style: Always use pure Rust"));
        assert!(block.contains("database: Sqlite + Turso"));
        assert!(block.contains("</core_memory>"));
    }
}

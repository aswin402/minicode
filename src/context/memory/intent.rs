//! Intent Anchoring & Execution Tracking (Phase 128).
//!
//! Provides data structures (`RequirementStatus`, `RequirementItem`, `IntentLedger`)
//! and persistence methods for goal anchoring and execution tracking.
#![allow(dead_code)]

use serde::{Deserialize, Serialize};

/// Execution status of an individual requirement or goal item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RequirementStatus {
    #[default]
    Pending,
    InProgress,
    Completed,
    Blocked,
    Skipped,
}

/// An individual granular requirement or sub-task tracked within an `IntentLedger`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RequirementItem {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub status: RequirementStatus,
    #[serde(default)]
    pub related_files: Vec<String>,
    #[serde(default)]
    pub created_turn: usize,
    #[serde(default)]
    pub updated_turn: usize,
}

/// Living execution ledger that maintains the root goal anchor, active items, and drift telemetry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct IntentLedger {
    pub root_objective: String,
    pub items: Vec<RequirementItem>,
    pub active_item_id: Option<String>,
    pub consecutive_turns_without_progress: usize,
}

impl IntentLedger {
    /// Creates a new `IntentLedger` initialized with the given root objective.
    pub fn new(root_objective: &str) -> Self {
        Self {
            root_objective: root_objective.to_string(),
            items: Vec::new(),
            active_item_id: None,
            consecutive_turns_without_progress: 0,
        }
    }

    /// Appends a new requirement item to the ledger and returns its generated ID.
    pub fn add_item(
        &mut self,
        title: &str,
        description: Option<&str>,
        related_files: Vec<String>,
    ) -> String {
        let id = format!("req-{}", uuid::Uuid::new_v4().simple());
        let item = RequirementItem {
            id: id.clone(),
            title: title.to_string(),
            description: description.map(|d| d.to_string()),
            status: RequirementStatus::Pending,
            related_files,
            created_turn: 0,
            updated_turn: 0,
        };
        self.items.push(item);
        id
    }

    /// Updates the status of an existing requirement item by ID.
    ///
    /// - If moved to `InProgress`, sets `active_item_id` to this item and resets drift counter.
    /// - If moved to `Completed`, clears `active_item_id` (if matching) and resets drift counter.
    /// - If moved to any other status, clears `active_item_id` if it was pointing to this item.
    ///
    /// Returns `true` if the item was found and updated, or `false` otherwise.
    pub fn set_status(&mut self, id: &str, status: RequirementStatus) -> bool {
        let Some(item) = self.items.iter_mut().find(|item| item.id == id) else {
            return false;
        };
        item.status = status;
        match status {
            RequirementStatus::InProgress => {
                self.active_item_id = Some(id.to_string());
                self.consecutive_turns_without_progress = 0;
            }
            RequirementStatus::Completed => {
                if self.active_item_id.as_deref() == Some(id) {
                    self.active_item_id = None;
                }
                self.consecutive_turns_without_progress = 0;
            }
            RequirementStatus::Pending
            | RequirementStatus::Blocked
            | RequirementStatus::Skipped => {
                if self.active_item_id.as_deref() == Some(id) {
                    self.active_item_id = None;
                }
            }
        }
        true
    }

    /// Retrieves an immutable reference to a requirement item by ID.
    pub fn get_item(&self, id: &str) -> Option<&RequirementItem> {
        self.items.iter().find(|item| item.id == id)
    }

    /// Atomically persists the `IntentLedger` to disk via a temporary file and rename.
    pub fn save_to_disk(&self, path: &std::path::Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        let tmp_path = match path.file_name() {
            Some(fname) => {
                let mut fname_os = fname.to_os_string();
                fname_os.push(format!(
                    ".tmp.{}.{}",
                    std::process::id(),
                    uuid::Uuid::new_v4().simple()
                ));
                path.with_file_name(fname_os)
            }
            None => path.with_extension(format!(
                "tmp.{}.{}",
                std::process::id(),
                uuid::Uuid::new_v4().simple()
            )),
        };

        std::fs::write(&tmp_path, json)?;
        if let Err(e) = std::fs::rename(&tmp_path, path) {
            let _ = std::fs::remove_file(&tmp_path);
            return Err(e);
        }
        Ok(())
    }

    /// Loads and deserializes an `IntentLedger` from disk.
    pub fn load_from_disk(path: &std::path::Path) -> std::io::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let ledger = serde_json::from_str::<Self>(&content)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        Ok(ledger)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_intent_ledger_lifecycle() {
        let mut ledger = IntentLedger::new("Build AgentBench sandbox website");
        assert_eq!(ledger.root_objective, "Build AgentBench sandbox website");
        assert_eq!(ledger.items.len(), 0);

        let id = ledger.add_item(
            "Dashboard with metrics cards",
            Some("Render revenue and activity"),
            vec!["src/pages/Dashboard.tsx".to_string()],
        );
        assert_eq!(ledger.items.len(), 1);
        assert_eq!(ledger.items[0].status, RequirementStatus::Pending);

        assert!(ledger.get_item(&id).is_some());
        assert_eq!(
            ledger.get_item(&id).unwrap().title,
            "Dashboard with metrics cards"
        );

        ledger.set_status(&id, RequirementStatus::InProgress);
        assert_eq!(ledger.items[0].status, RequirementStatus::InProgress);
        assert_eq!(ledger.active_item_id, Some(id.clone()));

        ledger.set_status(&id, RequirementStatus::Completed);
        assert_eq!(ledger.items[0].status, RequirementStatus::Completed);
        assert_eq!(ledger.active_item_id, None);
    }

    #[test]
    fn test_intent_ledger_status_and_drift_reset() {
        let mut ledger = IntentLedger::new("Test objective");
        let id1 = ledger.add_item("Task 1", None, vec![]);
        let id2 = ledger.add_item("Task 2", None, vec![]);

        ledger.consecutive_turns_without_progress = 5;

        // InProgress resets drift counter
        assert!(ledger.set_status(&id1, RequirementStatus::InProgress));
        assert_eq!(ledger.consecutive_turns_without_progress, 0);
        assert_eq!(ledger.active_item_id, Some(id1.clone()));

        // Blocked clears active_item_id without resetting drift counter
        ledger.consecutive_turns_without_progress = 3;
        assert!(ledger.set_status(&id1, RequirementStatus::Blocked));
        assert_eq!(ledger.consecutive_turns_without_progress, 3);
        assert_eq!(ledger.active_item_id, None);

        // Setting id2 to InProgress sets active_item_id to id2
        assert!(ledger.set_status(&id2, RequirementStatus::InProgress));
        assert_eq!(ledger.active_item_id, Some(id2.clone()));
        assert_eq!(ledger.consecutive_turns_without_progress, 0);

        // Non-existent id returns false
        assert!(!ledger.set_status("non-existent-id", RequirementStatus::Completed));
        assert!(ledger.get_item("non-existent-id").is_none());
    }

    #[test]
    fn test_intent_ledger_persistence_roundtrip() {
        let temp_dir = std::env::temp_dir().join(format!("intent_test_{}", uuid::Uuid::new_v4()));
        let file_path = temp_dir.join(".minicode").join("intent_anchor.json");

        let mut ledger = IntentLedger::new("Persistent Root Objective");
        let id1 = ledger.add_item(
            "Feature A",
            Some("Description A"),
            vec!["src/a.rs".to_string()],
        );
        ledger.set_status(&id1, RequirementStatus::InProgress);

        ledger
            .save_to_disk(&file_path)
            .expect("Failed to save ledger to disk");
        assert!(file_path.exists());

        let loaded =
            IntentLedger::load_from_disk(&file_path).expect("Failed to load ledger from disk");
        assert_eq!(loaded, ledger);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_intent_config_defaults() {
        let config = crate::config::IntentConfig::default();
        assert!(config.enabled);
        assert_eq!(
            config.persistence_file,
            crate::constants::DEFAULT_INTENT_PERSISTENCE_FILE
        );
        assert_eq!(
            config.drift_warning_turns,
            crate::constants::DEFAULT_INTENT_DRIFT_WARNING_TURNS
        );
        assert!(config.auto_extract);
        assert_eq!(
            config.max_ledger_items,
            crate::constants::DEFAULT_INTENT_MAX_ITEMS
        );
    }

    #[test]
    fn test_requirement_status_serde() {
        let statuses = vec![
            (RequirementStatus::Pending, "\"pending\""),
            (RequirementStatus::InProgress, "\"in_progress\""),
            (RequirementStatus::Completed, "\"completed\""),
            (RequirementStatus::Blocked, "\"blocked\""),
            (RequirementStatus::Skipped, "\"skipped\""),
        ];

        for (status, json_str) in statuses {
            let serialized = serde_json::to_string(&status).expect("Serialization failed");
            assert_eq!(serialized, json_str);
            let deserialized: RequirementStatus =
                serde_json::from_str(json_str).expect("Deserialization failed");
            assert_eq!(deserialized, status);
        }
    }
}

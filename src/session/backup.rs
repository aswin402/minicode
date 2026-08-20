use crate::constants::{BACKUPS_DIR_NAME, WORKSPACE_DIR_NAME};
use crate::error::{Result, SessionError};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupManifest {
    pub turn_id: usize,
    pub timestamp: String,
    #[serde(default)]
    pub user_prompt: Option<String>,
    #[serde(default)]
    pub message_index: usize,
    #[serde(default)]
    pub working_memory_plan: Option<String>,
    pub files: Vec<BackedUpFile>,
}

impl BackupManifest {
    pub fn new(turn_id: usize) -> Self {
        Self {
            turn_id,
            timestamp: chrono::Utc::now().to_rfc3339(),
            user_prompt: None,
            message_index: 0,
            working_memory_plan: None,
            files: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackedUpFile {
    pub original_path: String,
    pub backup_path: String,
    pub existed_before: bool,
}

pub struct BackupManager {
    workspace_root: PathBuf,
    backup_root: PathBuf,
}

impl BackupManager {
    pub fn new(workspace_root: &Path) -> Self {
        Self {
            workspace_root: workspace_root.to_path_buf(),
            backup_root: workspace_root
                .join(WORKSPACE_DIR_NAME)
                .join(BACKUPS_DIR_NAME),
        }
    }

    pub fn workspace_root(&self) -> &Path {
        &self.workspace_root
    }

    #[allow(dead_code)]
    pub fn backup_root(&self) -> &Path {
        &self.backup_root
    }

    /// Creates a safety checkpoint of a file before modification in a given turn.
    ///
    /// If the file exists, copies it into `.minicode/backups/<turn_id>/...`.
    /// If the file does not exist (new file creation), records `existed_before = false`.
    pub fn create_checkpoint(
        &self,
        workspace_root: &Path,
        file_path: &Path,
        turn_id: usize,
    ) -> Result<BackedUpFile> {
        let turn_dir = self.backup_root.join(turn_id.to_string());
        std::fs::create_dir_all(&turn_dir)?;

        let full_file =
            crate::sandbox::path::validate_path_in_workspace(workspace_root, file_path)?;
        let canonical_ws =
            std::fs::canonicalize(workspace_root).unwrap_or_else(|_| workspace_root.to_path_buf());

        let relative_path = full_file
            .strip_prefix(&canonical_ws)
            .or_else(|_| full_file.strip_prefix(workspace_root))
            .map_err(|_| {
                SessionError::WriteCheckpoint(format!(
                    "Path '{}' escapes workspace boundary '{}'",
                    file_path.display(),
                    workspace_root.display()
                ))
            })?;

        let backup_dest = turn_dir.join(relative_path);

        let backed_up_file = if full_file.exists() {
            if let Some(parent) = backup_dest.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(&full_file, &backup_dest).map_err(|e| {
                SessionError::WriteCheckpoint(format!(
                    "Failed to copy {} to backup {}: {}",
                    full_file.display(),
                    backup_dest.display(),
                    e
                ))
            })?;

            tracing::debug!(
                original = %full_file.display(),
                backup = %backup_dest.display(),
                turn = turn_id,
                "File safety checkpoint created"
            );

            BackedUpFile {
                original_path: full_file.display().to_string(),
                backup_path: backup_dest.display().to_string(),
                existed_before: true,
            }
        } else {
            BackedUpFile {
                original_path: full_file.display().to_string(),
                backup_path: backup_dest.display().to_string(),
                existed_before: false,
            }
        };

        // Auto-persist / update turn manifest so /undo is always guaranteed to work immediately
        let manifest_path = turn_dir.join("manifest.json");
        let mut manifest = if manifest_path.exists() {
            match std::fs::read_to_string(&manifest_path) {
                Ok(content) => serde_json::from_str::<BackupManifest>(&content)
                    .unwrap_or_else(|_| BackupManifest::new(turn_id)),
                Err(_) => BackupManifest::new(turn_id),
            }
        } else {
            BackupManifest::new(turn_id)
        };

        if !manifest
            .files
            .iter()
            .any(|f| f.original_path == backed_up_file.original_path)
        {
            manifest.files.push(backed_up_file.clone());
            let data = serde_json::to_string_pretty(&manifest)?;
            std::fs::write(&manifest_path, data)?;
        }

        Ok(backed_up_file)
    }

    /// Saves the turn manifest containing all backed up files in this turn.
    #[allow(dead_code)]
    pub fn save_turn_manifest(&self, manifest: &BackupManifest) -> Result<()> {
        let turn_dir = self.backup_root.join(manifest.turn_id.to_string());
        std::fs::create_dir_all(&turn_dir)?;
        let manifest_path = turn_dir.join("manifest.json");

        let data = serde_json::to_string_pretty(manifest)?;
        std::fs::write(&manifest_path, data)?;
        Ok(())
    }

    /// Loads the turn manifest for a given turn.
    pub fn load_turn_manifest(&self, turn_id: usize) -> Result<BackupManifest> {
        let manifest_path = self
            .backup_root
            .join(turn_id.to_string())
            .join("manifest.json");

        let content = match std::fs::read_to_string(&manifest_path) {
            Ok(c) => c,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(SessionError::NoBackupAvailable { turn_id }.into());
            }
            Err(e) => return Err(e.into()),
        };
        let manifest: BackupManifest = serde_json::from_str(&content)?;
        Ok(manifest)
    }

    /// Returns the most recent turn ID with an existing backup manifest
    #[allow(dead_code)]
    pub fn latest_turn_id(&self) -> Option<usize> {
        if !self.backup_root.exists() {
            return None;
        }

        let mut turn_ids: Vec<usize> = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&self.backup_root) {
            for entry in entries.flatten() {
                if let Ok(file_type) = entry.file_type() {
                    if file_type.is_dir() {
                        if let Ok(id) = entry.file_name().to_string_lossy().parse::<usize>() {
                            if entry.path().join("manifest.json").exists() {
                                turn_ids.push(id);
                            }
                        }
                    }
                }
            }
        }

        turn_ids.sort();
        turn_ids.last().copied()
    }

    /// Records the initiation of a new user turn with prompt text and message boundary.
    pub fn record_turn_start(
        &self,
        turn_id: usize,
        user_prompt: &str,
        message_index: usize,
    ) -> Result<()> {
        let turn_dir = self.backup_root.join(turn_id.to_string());
        std::fs::create_dir_all(&turn_dir)?;
        let manifest_path = turn_dir.join("manifest.json");

        let mut manifest = if manifest_path.exists() {
            match std::fs::read_to_string(&manifest_path) {
                Ok(content) => {
                    serde_json::from_str::<BackupManifest>(&content).unwrap_or_else(|_| {
                        BackupManifest {
                            turn_id,
                            timestamp: chrono::Utc::now().to_rfc3339(),
                            user_prompt: Some(user_prompt.to_string()),
                            message_index,
                            working_memory_plan: None,
                            files: Vec::new(),
                        }
                    })
                }
                Err(_) => BackupManifest {
                    turn_id,
                    timestamp: chrono::Utc::now().to_rfc3339(),
                    user_prompt: Some(user_prompt.to_string()),
                    message_index,
                    working_memory_plan: None,
                    files: Vec::new(),
                },
            }
        } else {
            BackupManifest {
                turn_id,
                timestamp: chrono::Utc::now().to_rfc3339(),
                user_prompt: Some(user_prompt.to_string()),
                message_index,
                working_memory_plan: None,
                files: Vec::new(),
            }
        };

        manifest.user_prompt = Some(user_prompt.to_string());
        manifest.message_index = message_index;
        let data = serde_json::to_string_pretty(&manifest)?;
        std::fs::write(&manifest_path, data)?;
        Ok(())
    }

    /// Discovers and lists all recorded turn checkpoints sorted descending (newest first).
    pub fn list_checkpoints(&self) -> Vec<BackupManifest> {
        if !self.backup_root.exists() {
            return Vec::new();
        }

        let mut checkpoints = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&self.backup_root) {
            for entry in entries.flatten() {
                if let Ok(file_type) = entry.file_type() {
                    if file_type.is_dir() {
                        let manifest_path = entry.path().join("manifest.json");
                        if manifest_path.exists() {
                            if let Ok(content) = std::fs::read_to_string(&manifest_path) {
                                if let Ok(manifest) =
                                    serde_json::from_str::<BackupManifest>(&content)
                                {
                                    checkpoints.push(manifest);
                                }
                            }
                        }
                    }
                }
            }
        }

        checkpoints.sort_by(|a, b| b.turn_id.cmp(&a.turn_id));
        checkpoints
    }

    /// Prunes backup folders older than `max_turns` to prevent disk bloat.
    #[allow(dead_code)]
    pub fn prune_old_backups(&self, max_turns: usize) -> Result<()> {
        if !self.backup_root.exists() {
            return Ok(());
        }

        let mut turn_ids: Vec<usize> = Vec::new();
        for entry in std::fs::read_dir(&self.backup_root)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                if let Ok(id) = entry.file_name().to_string_lossy().parse::<usize>() {
                    turn_ids.push(id);
                }
            }
        }

        turn_ids.sort();
        if turn_ids.len() > max_turns {
            let to_remove = turn_ids.len() - max_turns;
            for id in &turn_ids[..to_remove] {
                let dir_to_delete = self.backup_root.join(id.to_string());
                if let Err(e) = std::fs::remove_dir_all(&dir_to_delete) {
                    tracing::warn!(turn_id = id, path = %dir_to_delete.display(), error = %e, "Failed to prune old backup directory");
                } else {
                    tracing::debug!(turn_id = id, "Pruned old backup directory");
                }
            }
        }

        Ok(())
    }

    /// Removes the backup folder for a specific turn (e.g. after rollback)
    pub fn remove_turn_backup(&self, turn_id: usize) -> Result<()> {
        let turn_dir = self.backup_root.join(turn_id.to_string());
        if turn_dir.exists() {
            std::fs::remove_dir_all(&turn_dir)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_checkpoint_and_manifest() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_backup_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let test_file = temp_dir.join("test.txt");
        std::fs::write(&test_file, "Original Content").unwrap();

        let mgr = BackupManager::new(&temp_dir);
        let backed_up = mgr.create_checkpoint(&temp_dir, &test_file, 1).unwrap();
        assert!(backed_up.existed_before);

        let mut manifest = BackupManifest::new(1);
        manifest.files = vec![backed_up];
        mgr.save_turn_manifest(&manifest).unwrap();

        let loaded = mgr.load_turn_manifest(1).unwrap();
        assert_eq!(loaded.files.len(), 1);
        assert_eq!(
            loaded.files[0].original_path,
            test_file.display().to_string()
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_backup_rejects_path_escape() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_backup_test_esc_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let outside_dir =
            std::env::temp_dir().join(format!("minicode_backup_outside_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&outside_dir).unwrap();
        let outside_file = outside_dir.join("outside.txt");
        std::fs::write(&outside_file, "outside").unwrap();

        let mgr = BackupManager::new(&temp_dir);
        let result = mgr.create_checkpoint(&temp_dir, &outside_file, 1);
        assert!(result.is_err());

        let _ = std::fs::remove_dir_all(&temp_dir);
        let _ = std::fs::remove_dir_all(&outside_dir);
    }

    #[test]
    fn test_checkpoint_automatically_persists_manifest() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_backup_auto_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let file_a = temp_dir.join("a.txt");
        let file_b = temp_dir.join("b.txt");
        std::fs::write(&file_a, "Alpha").unwrap();
        std::fs::write(&file_b, "Beta").unwrap();

        let mgr = BackupManager::new(&temp_dir);
        // Call checkpoint on both files in turn 42
        mgr.create_checkpoint(&temp_dir, &file_a, 42).unwrap();
        mgr.create_checkpoint(&temp_dir, &file_b, 42).unwrap();

        // Manifest should automatically exist and contain both files
        let manifest = mgr.load_turn_manifest(42).unwrap();
        assert_eq!(manifest.turn_id, 42);
        assert_eq!(manifest.files.len(), 2);
        assert_eq!(mgr.latest_turn_id(), Some(42));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_record_turn_start_and_list_checkpoints() {
        let temp_dir = std::env::temp_dir().join(format!(
            "minicode_backup_checkpoints_{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let mgr = BackupManager::new(&temp_dir);
        mgr.record_turn_start(1, "initial setup", 0).unwrap();
        mgr.record_turn_start(2, "add authentication", 4).unwrap();
        mgr.record_turn_start(3, "fix thought timer", 8).unwrap();

        let checkpoints = mgr.list_checkpoints();
        assert_eq!(checkpoints.len(), 3);
        assert_eq!(checkpoints[0].turn_id, 3);
        assert_eq!(
            checkpoints[0].user_prompt.as_deref(),
            Some("fix thought timer")
        );
        assert_eq!(checkpoints[0].message_index, 8);

        assert_eq!(checkpoints[1].turn_id, 2);
        assert_eq!(
            checkpoints[1].user_prompt.as_deref(),
            Some("add authentication")
        );

        assert_eq!(checkpoints[2].turn_id, 1);
        assert_eq!(checkpoints[2].user_prompt.as_deref(), Some("initial setup"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}

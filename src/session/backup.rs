#![allow(dead_code)]

use crate::error::{Result, SessionError};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupManifest {
    pub turn_id: usize,
    pub timestamp: String,
    pub files: Vec<BackedUpFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackedUpFile {
    pub original_path: String,
    pub backup_path: String,
    pub existed_before: bool,
}

pub struct BackupManager {
    backup_root: PathBuf,
}

impl BackupManager {
    pub fn new(workspace_root: &Path) -> Self {
        Self {
            backup_root: workspace_root.join(".minicode").join("backups"),
        }
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

        let relative_path = file_path.strip_prefix(workspace_root).unwrap_or(file_path);

        let backup_dest = turn_dir.join(relative_path);

        if file_path.exists() {
            if let Some(parent) = backup_dest.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(file_path, &backup_dest).map_err(|e| {
                SessionError::WriteCheckpoint(format!(
                    "Failed to copy {} to backup {}: {}",
                    file_path.display(),
                    backup_dest.display(),
                    e
                ))
            })?;

            tracing::debug!(
                original = %file_path.display(),
                backup = %backup_dest.display(),
                turn = turn_id,
                "File safety checkpoint created"
            );

            Ok(BackedUpFile {
                original_path: file_path.display().to_string(),
                backup_path: backup_dest.display().to_string(),
                existed_before: true,
            })
        } else {
            Ok(BackedUpFile {
                original_path: file_path.display().to_string(),
                backup_path: backup_dest.display().to_string(),
                existed_before: false,
            })
        }
    }

    /// Saves the turn manifest containing all backed up files in this turn.
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

        if !manifest_path.exists() {
            return Err(SessionError::NoBackupAvailable { turn_id }.into());
        }

        let content = std::fs::read_to_string(&manifest_path)?;
        let manifest: BackupManifest = serde_json::from_str(&content)?;
        Ok(manifest)
    }

    /// Prunes backup folders older than `max_turns` to prevent disk bloat.
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
                std::fs::remove_dir_all(&dir_to_delete).ok();
                tracing::debug!(turn_id = id, "Pruned old backup directory");
            }
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

        let manifest = BackupManifest {
            turn_id: 1,
            timestamp: chrono::Utc::now().to_rfc3339(),
            files: vec![backed_up],
        };
        mgr.save_turn_manifest(&manifest).unwrap();

        let loaded = mgr.load_turn_manifest(1).unwrap();
        assert_eq!(loaded.files.len(), 1);
        assert_eq!(
            loaded.files[0].original_path,
            test_file.display().to_string()
        );

        std::fs::remove_dir_all(&temp_dir).ok();
    }
}

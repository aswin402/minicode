#![allow(dead_code)]

use crate::error::{Result, SessionError};
use crate::session::backup::BackupManager;
use std::path::Path;

pub struct UndoEngine;

impl UndoEngine {
    /// Reverts all file changes made in a specific turn back to their pre-turn state.
    pub fn rollback_turn(backup_manager: &BackupManager, turn_id: usize) -> Result<Vec<String>> {
        let manifest = backup_manager.load_turn_manifest(turn_id)?;
        let mut restored_paths = Vec::new();

        for file in manifest.files {
            let orig = Path::new(&file.original_path);
            let backup = Path::new(&file.backup_path);

            if file.existed_before {
                if backup.exists() {
                    if let Some(parent) = orig.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    std::fs::copy(backup, orig).map_err(|e| {
                        SessionError::WriteCheckpoint(format!(
                            "Failed to restore {} from {}: {}",
                            file.original_path, file.backup_path, e
                        ))
                    })?;
                    tracing::info!(path = %file.original_path, "Restored file from backup");
                    restored_paths.push(file.original_path);
                }
            } else {
                // File did not exist before this turn — delete it
                if orig.exists() {
                    if let Err(e) = std::fs::remove_file(orig) {
                        tracing::warn!(path = %file.original_path, error = %e, "Failed to remove newly created file during undo rollback");
                    } else {
                        tracing::info!(path = %file.original_path, "Removed file created in rolled-back turn");
                        restored_paths.push(format!("(deleted) {}", file.original_path));
                    }
                }
            }
        }

        Ok(restored_paths)
    }
}

#[derive(Debug, Clone)]
pub struct UndoResult {
    pub turn_id: usize,
    pub restored_count: usize,
    pub deleted_count: usize,
    pub files: Vec<String>,
}

/// Convenience function to rollback changes made in the latest recorded turn
pub fn rollback_turn(workspace_root: &Path) -> Result<UndoResult> {
    let backup_manager = BackupManager::new(workspace_root);
    if let Some(turn_id) = backup_manager.latest_turn_id() {
        let files = UndoEngine::rollback_turn(&backup_manager, turn_id)?;
        if let Err(e) = backup_manager.remove_turn_backup(turn_id) {
            tracing::warn!(turn = turn_id, error = %e, "Failed to remove turn backup directory after rollback");
        }
        let mut restored = 0;
        let mut deleted = 0;
        for f in &files {
            if f.starts_with("(deleted)") {
                deleted += 1;
            } else {
                restored += 1;
            }
        }
        Ok(UndoResult {
            turn_id,
            restored_count: restored,
            deleted_count: deleted,
            files,
        })
    } else {
        Ok(UndoResult {
            turn_id: 0,
            restored_count: 0,
            deleted_count: 0,
            files: vec![],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::backup::BackupManifest;

    #[test]
    fn test_undo_rollback() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_undo_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let test_file = temp_dir.join("test.txt");
        std::fs::write(&test_file, "Before Change").unwrap();

        let mgr = BackupManager::new(&temp_dir);
        let backed_up = mgr.create_checkpoint(&temp_dir, &test_file, 1).unwrap();

        let manifest = BackupManifest {
            turn_id: 1,
            timestamp: chrono::Utc::now().to_rfc3339(),
            files: vec![backed_up],
        };
        mgr.save_turn_manifest(&manifest).unwrap();

        // Mutate the file
        std::fs::write(&test_file, "After Destructive Change").unwrap();
        assert_eq!(
            std::fs::read_to_string(&test_file).unwrap(),
            "After Destructive Change"
        );

        // Rollback
        let restored = UndoEngine::rollback_turn(&mgr, 1).unwrap();
        assert_eq!(restored.len(), 1);
        assert_eq!(
            std::fs::read_to_string(&test_file).unwrap(),
            "Before Change"
        );

        std::fs::remove_dir_all(&temp_dir).ok();
    }
}

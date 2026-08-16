use crate::error::{Result, SessionError};
use crate::session::backup::BackupManager;
use std::path::Path;

pub struct UndoEngine;

impl UndoEngine {
    /// Reverts all file changes made in a specific turn back to their pre-turn state.
    pub fn rollback_turn(backup_manager: &BackupManager, turn_id: usize) -> Result<Vec<String>> {
        let manifest = backup_manager.load_turn_manifest(turn_id)?;
        let mut restored_paths = Vec::new();
        let ws_root = backup_manager.workspace_root();

        for file in manifest.files {
            let orig = Path::new(&file.original_path);
            let backup = Path::new(&file.backup_path);

            // Defense in depth: validate that the target path is strictly confined within workspace
            if let Err(e) = crate::sandbox::path::validate_path_in_workspace(ws_root, orig) {
                tracing::warn!(
                    path = %file.original_path,
                    error = %e,
                    "Skipping untrusted manifest path escaping workspace during undo rollback"
                );
                continue;
            }

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
                // File or directory did not exist before this turn — delete it
                if orig.exists() {
                    let remove_res = if orig.is_dir() {
                        std::fs::remove_dir_all(orig)
                    } else {
                        std::fs::remove_file(orig)
                    };
                    if let Err(e) = remove_res {
                        tracing::warn!(path = %file.original_path, error = %e, "Failed to remove newly created path during undo rollback");
                    } else {
                        tracing::info!(path = %file.original_path, "Removed path created in rolled-back turn");
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
    #[allow(dead_code)]
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

        // If repository has a git commit from this turn, gently soft-reset it
        let _ = std::process::Command::new("git")
            .arg("--no-pager")
            .args(["reset", "--soft", "HEAD~1"])
            .current_dir(workspace_root)
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_PAGER", "cat")
            .env("LC_ALL", "C")
            .output();

        let mut restored = 0;
        let mut deleted = 0;
        for f in &files {
            if f.starts_with("(deleted)") {
                deleted += 1;
            } else {
                restored += 1;
            }
        }
        crate::ui::status::StatusWidgets::invalidate_git_cache();
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

    #[test]
    fn test_undo_rollback_deletes_created_directory() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_undo_dir_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let new_sub_dir = temp_dir.join("created_dir");
        std::fs::create_dir_all(&new_sub_dir).unwrap();
        std::fs::write(new_sub_dir.join("file.txt"), "hello").unwrap();

        let mgr = BackupManager::new(&temp_dir);
        let backed_up = crate::session::backup::BackedUpFile {
            original_path: new_sub_dir.to_string_lossy().to_string(),
            backup_path: String::new(),
            existed_before: false,
        };

        let manifest = BackupManifest {
            turn_id: 2,
            timestamp: chrono::Utc::now().to_rfc3339(),
            files: vec![backed_up],
        };
        mgr.save_turn_manifest(&manifest).unwrap();

        assert!(new_sub_dir.exists());

        let res = rollback_turn(&temp_dir).unwrap();
        assert_eq!(res.deleted_count, 1);
        assert!(!new_sub_dir.exists());

        std::fs::remove_dir_all(&temp_dir).ok();
    }
}

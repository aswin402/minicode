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
    #[allow(dead_code)]
    pub turn_id: usize,
    pub restored_count: usize,
    pub deleted_count: usize,
    #[allow(dead_code)]
    pub files: Vec<String>,
}

/// Convenience function to rollback changes made in the latest recorded turn
#[allow(dead_code)]
pub fn rollback_turn(workspace_root: &Path) -> Result<UndoResult> {
    let backup_manager = BackupManager::new(workspace_root);
    if let Some(turn_id) = backup_manager.latest_turn_id() {
        rollback_to_checkpoint(workspace_root, turn_id)
    } else {
        Ok(UndoResult {
            turn_id: 0,
            restored_count: 0,
            deleted_count: 0,
            files: vec![],
        })
    }
}

/// Rolls back changes from the latest recorded turn down to and including `target_turn_id`.
pub fn rollback_to_checkpoint(workspace_root: &Path, target_turn_id: usize) -> Result<UndoResult> {
    let backup_manager = BackupManager::new(workspace_root);
    let checkpoints = backup_manager.list_checkpoints();

    let mut total_restored = 0;
    let mut total_deleted = 0;
    let mut all_files = Vec::new();
    let mut turns_reverted = 0;

    for cp in checkpoints {
        if cp.turn_id >= target_turn_id {
            let files = UndoEngine::rollback_turn(&backup_manager, cp.turn_id)?;
            let _ = backup_manager.remove_turn_backup(cp.turn_id);
            for f in &files {
                if f.starts_with("(deleted)") {
                    total_deleted += 1;
                } else {
                    total_restored += 1;
                }
            }
            all_files.extend(files);
            turns_reverted += 1;
        }
    }

    if turns_reverted > 0 {
        // Soft reset git commits if in a repo
        let reset_arg = format!("HEAD~{}", turns_reverted);
        let _ = std::process::Command::new("git")
            .arg("--no-pager")
            .args(["reset", "--soft", &reset_arg])
            .current_dir(workspace_root)
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_PAGER", "cat")
            .env("LC_ALL", "C")
            .output();

        crate::ui::status::StatusWidgets::invalidate_git_cache();
    }

    Ok(UndoResult {
        turn_id: target_turn_id,
        restored_count: total_restored,
        deleted_count: total_deleted,
        files: all_files,
    })
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

        let mut manifest = BackupManifest::new(1);
        manifest.files = vec![backed_up];
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

        let mut manifest = BackupManifest::new(2);
        manifest.files = vec![backed_up];
        mgr.save_turn_manifest(&manifest).unwrap();

        assert!(new_sub_dir.exists());

        let res = rollback_turn(&temp_dir).unwrap();
        assert_eq!(res.deleted_count, 1);
        assert!(!new_sub_dir.exists());

        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_rollback_to_multi_turn_checkpoint() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_undo_multi_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let file_a = temp_dir.join("a.txt");
        let file_b = temp_dir.join("b.txt");
        std::fs::write(&file_a, "A initial").unwrap();
        std::fs::write(&file_b, "B initial").unwrap();

        let mgr = BackupManager::new(&temp_dir);

        // Turn 1: modify file_a
        let backed_up_1 = mgr.create_checkpoint(&temp_dir, &file_a, 1).unwrap();
        let mut manifest_1 = BackupManifest::new(1);
        manifest_1.files = vec![backed_up_1];
        manifest_1.user_prompt = Some("modify a".to_string());
        mgr.save_turn_manifest(&manifest_1).unwrap();
        std::fs::write(&file_a, "A modified in turn 1").unwrap();

        // Turn 2: modify file_b
        let backed_up_2 = mgr.create_checkpoint(&temp_dir, &file_b, 2).unwrap();
        let mut manifest_2 = BackupManifest::new(2);
        manifest_2.files = vec![backed_up_2];
        manifest_2.user_prompt = Some("modify b".to_string());
        mgr.save_turn_manifest(&manifest_2).unwrap();
        std::fs::write(&file_b, "B modified in turn 2").unwrap();

        // Rollback to checkpoint 1 (reverting Turn 2 and Turn 1)
        let res = rollback_to_checkpoint(&temp_dir, 1).unwrap();
        assert_eq!(res.restored_count, 2);
        assert_eq!(std::fs::read_to_string(&file_a).unwrap(), "A initial");
        assert_eq!(std::fs::read_to_string(&file_b).unwrap(), "B initial");

        std::fs::remove_dir_all(&temp_dir).ok();
    }
}

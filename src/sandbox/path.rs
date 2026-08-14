#![allow(dead_code)]

use crate::error::{Result, SecurityError};
use std::path::{Path, PathBuf};

/// Validates that a requested path does not escape the workspace root.
///
/// Handles relative paths, `..` traversals, and symbolic link dereferencing
/// by canonicalizing both the workspace root and the target path.
pub fn validate_path_in_workspace(workspace_root: &Path, user_path: &Path) -> Result<PathBuf> {
    let canonical_root =
        std::fs::canonicalize(workspace_root).map_err(|e| SecurityError::PathEscapesWorkspace {
            path: workspace_root.display().to_string(),
            workspace_root: format!("Failed to canonicalize workspace root: {}", e),
        })?;

    let resolved_path = if user_path.is_absolute() {
        user_path.to_path_buf()
    } else {
        workspace_root.join(user_path)
    };

    // If the file/path exists, canonicalize it directly (resolves symlinks)
    if resolved_path.exists() {
        let canonical_target = std::fs::canonicalize(&resolved_path).map_err(|e| {
            SecurityError::PathEscapesWorkspace {
                path: resolved_path.display().to_string(),
                workspace_root: format!("Failed to canonicalize path: {}", e),
            }
        })?;

        if !canonical_target.starts_with(&canonical_root) {
            return Err(SecurityError::PathEscapesWorkspace {
                path: resolved_path.display().to_string(),
                workspace_root: canonical_root.display().to_string(),
            }
            .into());
        }

        Ok(canonical_target)
    } else {
        // If file does not exist yet (e.g. for write_file / new file creation),
        // canonicalize the nearest existing parent directory
        let mut curr = resolved_path.as_path();
        while let Some(parent) = curr.parent() {
            if parent.exists() {
                let canonical_parent = std::fs::canonicalize(parent).map_err(|e| {
                    SecurityError::PathEscapesWorkspace {
                        path: parent.display().to_string(),
                        workspace_root: format!("Failed to canonicalize parent: {}", e),
                    }
                })?;

                if !canonical_parent.starts_with(&canonical_root) {
                    return Err(SecurityError::PathEscapesWorkspace {
                        path: resolved_path.display().to_string(),
                        workspace_root: canonical_root.display().to_string(),
                    }
                    .into());
                }
                break;
            }
            curr = parent;
        }

        Ok(resolved_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_path_inside_workspace() {
        let temp_dir = std::env::temp_dir();
        let valid_file = temp_dir.join("test_file.txt");
        let result = validate_path_in_workspace(&temp_dir, &valid_file);
        assert!(result.is_ok());
    }

    #[test]
    fn test_path_traversal_blocked() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_sandbox_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let malicious_path = PathBuf::from("../../etc/shadow");
        let result = validate_path_in_workspace(&temp_dir, &malicious_path);
        assert!(result.is_err());

        std::fs::remove_dir_all(&temp_dir).ok();
    }
}

use crate::error::{Result, SecurityError};
use std::path::{Path, PathBuf};

/// Lexically normalizes a path resolving `.`, `..`, and prefix/root components without filesystem access.
pub fn normalize_path(path: &Path) -> PathBuf {
    let mut components = Vec::new();
    for component in path.components() {
        match component {
            std::path::Component::Prefix(..) => components.push(component),
            std::path::Component::RootDir => {
                components.clear();
                components.push(component);
            }
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                if let Some(last) = components.last() {
                    match last {
                        std::path::Component::Normal(..) => {
                            components.pop();
                        }
                        _ => components.push(component),
                    }
                } else {
                    components.push(component);
                }
            }
            std::path::Component::Normal(..) => components.push(component),
        }
    }
    components.into_iter().collect()
}

/// Validates that a requested path does not escape the workspace root.
///
/// Handles relative paths, `..` traversals, and symbolic link dereferencing
/// by canonicalizing both the workspace root and the target path.
///
/// **Security Note**: This is a user-space validation check subject to TOCTOU races.
/// Linux Landlock kernel-level sandboxing (applied in `exec.rs`) is the primary
/// enforcement mechanism. This function serves as a defense-in-depth layer.
pub fn validate_path_in_workspace(workspace_root: &Path, user_path: &Path) -> Result<PathBuf> {
    let canonical_root =
        std::fs::canonicalize(workspace_root).map_err(|e| SecurityError::PathEscapesWorkspace {
            path: workspace_root.display().to_string(),
            workspace_root: format!("Failed to canonicalize workspace root: {}", e),
        })?;

    let raw_resolved = if user_path.is_absolute() {
        user_path.to_path_buf()
    } else {
        canonical_root.join(user_path)
    };

    let normalized = normalize_path(&raw_resolved);

    // Walk up to the nearest existing ancestor and resolve IT. Comparing
    // canonical forms on both sides keeps symlinked roots (e.g. /tmp ->
    // /private/tmp on macOS) working for existing AND not-yet-existing paths.
    let mut probe = normalized.clone();
    while !probe.exists() {
        match probe.parent() {
            Some(parent) => probe = parent.to_path_buf(),
            None => break,
        }
    }
    let canonical_probe =
        std::fs::canonicalize(&probe).map_err(|e| SecurityError::PathEscapesWorkspace {
            path: probe.display().to_string(),
            workspace_root: format!("Failed to canonicalize path: {}", e),
        })?;

    if !canonical_probe.starts_with(&canonical_root) {
        return Err(SecurityError::PathEscapesWorkspace {
            path: user_path.display().to_string(),
            workspace_root: canonical_root.display().to_string(),
        }
        .into());
    }

    if probe == normalized {
        // Path exists: return its fully resolved form.
        Ok(canonical_probe)
    } else {
        // Not-yet-existing path: re-anchor the non-existing tail onto the
        // canonical ancestor (e.g. for write_file / new file creation).
        let tail =
            normalized
                .strip_prefix(&probe)
                .map_err(|_| SecurityError::PathEscapesWorkspace {
                    path: user_path.display().to_string(),
                    workspace_root: canonical_root.display().to_string(),
                })?;
        Ok(canonical_probe.join(tail))
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

use crate::error::{Result, ToolError};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Client abstraction for interacting with the local `onpkg` CLI.
pub struct OnpkgClient;

impl OnpkgClient {
    /// Locates the `onpkg` executable binary on the system.
    pub fn find_binary() -> Option<PathBuf> {
        // 1. Check standard PATH via `which onpkg`
        if let Ok(output) = Command::new("which").arg("onpkg").output() {
            if output.status.success() {
                let path_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !path_str.is_empty() {
                    let path = PathBuf::from(path_str);
                    if path.exists() {
                        return Some(path);
                    }
                }
            }
        }

        // 2. Check standard user cargo / local bin locations
        if let Some(home) = dirs::home_dir() {
            let candidates = [
                home.join(".cargo/bin/onpkg"),
                home.join(".local/bin/onpkg"),
                PathBuf::from("/usr/local/bin/onpkg"),
                PathBuf::from("/usr/bin/onpkg"),
            ];

            for c in candidates {
                if c.exists() {
                    return Some(c);
                }
            }
        }

        None
    }

    /// Checks if `onpkg` is installed and executable.
    #[allow(dead_code)]
    pub fn is_installed() -> bool {
        Self::find_binary().is_some()
    }

    /// Executes an `onpkg` subcommand with the given arguments.
    pub fn exec(args: &[&str], workspace_root: &Path) -> Result<String> {
        let binary = Self::find_binary().ok_or_else(|| {
            ToolError::CommandExec(
                "The `onpkg` binary was not found in PATH or standard bin directories. Please ensure onpkg is installed via `cargo install onpkg` or built locally.".to_string(),
            )
        })?;

        let output = Command::new(&binary)
            .current_dir(workspace_root)
            .args(args)
            .output()
            .map_err(|e| {
                ToolError::CommandExec(format!("Failed to execute `onpkg {:?}`: {}", args, e))
            })?;

        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

        if !output.status.success() {
            let error_detail = if !stderr.is_empty() {
                stderr
            } else if !stdout.is_empty() {
                stdout
            } else {
                format!("Process exited with status code {:?}", output.status.code())
            };
            return Err(ToolError::CommandExec(format!("`onpkg` error: {}", error_detail)).into());
        }

        if stdout.is_empty() && !stderr.is_empty() {
            Ok(stderr)
        } else {
            Ok(stdout)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_binary_does_not_panic() {
        let _ = OnpkgClient::find_binary();
    }
}

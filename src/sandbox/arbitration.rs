//! Merge and Conflict Arbitration Engine for isolated subagent worktrees.
//!
//! Provides pre-merge sandboxed verification, clean mergeability checking
//! via `git merge-tree`, and conflict-free branch merging into the main workspace.

use std::path::Path;
use std::process::Command;

/// Structured validation report from pre-merge test or compiler execution.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ValidationReport {
    pub success: bool,
    pub command: String,
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: u64,
}

/// Structured report of branch mergeability against HEAD.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MergeabilityReport {
    pub can_merge_cleanly: bool,
    pub conflicted_files: Vec<String>,
    pub merge_base: Option<String>,
}

/// Structured report summarizing a successfully applied branch merge.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MergeSuccessReport {
    pub subagent_id: String,
    pub branch_name: String,
    pub files_changed: Vec<String>,
    pub committed: bool,
    pub commit_hash: Option<String>,
    pub commit_message: Option<String>,
}

/// Typed error conditions encountered during worktree arbitration and merging.
#[allow(dead_code)]
#[derive(Debug, thiserror::Error)]
pub enum ArbitrationError {
    #[error("Worktree directory not found: {0}")]
    WorktreeNotFound(String),

    #[error("Pre-merge verification failed: {0}")]
    VerificationFailed(String),

    #[error("Merge conflicts detected in files: {0:?}")]
    MergeConflict(Vec<String>),

    #[error("Git execution failed: {0}")]
    GitError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

/// Subagent Merge and Conflict Arbitration Engine.
#[allow(dead_code)]
pub struct MergeArbitrator;

#[allow(dead_code)]
impl MergeArbitrator {
    /// Detects standard project validation command based on manifest files in the directory.
    ///
    /// - `Cargo.toml`: `["cargo", "check", "-j", "1"]`
    /// - `package.json`: `["bun", "test"]` (if `bun.lockb`), `["pnpm", "test"]` (if `pnpm-lock.yaml`), else `["npm", "test"]`
    /// - `pyproject.toml` or `pytest.ini`: `["pytest"]`
    /// - `go.mod`: `["go", "test", "./..."]`
    pub fn detect_project_validation_cmd(path: &Path) -> Option<Vec<String>> {
        if path.join("Cargo.toml").exists() {
            Some(vec![
                "cargo".to_string(),
                "check".to_string(),
                "-j".to_string(),
                "1".to_string(),
            ])
        } else if path.join("package.json").exists() {
            if path.join("bun.lockb").exists() {
                Some(vec!["bun".to_string(), "test".to_string()])
            } else if path.join("pnpm-lock.yaml").exists() {
                Some(vec!["pnpm".to_string(), "test".to_string()])
            } else {
                Some(vec!["npm".to_string(), "test".to_string()])
            }
        } else if path.join("pyproject.toml").exists() || path.join("pytest.ini").exists() {
            Some(vec!["pytest".to_string()])
        } else if path.join("go.mod").exists() {
            Some(vec![
                "go".to_string(),
                "test".to_string(),
                "./...".to_string(),
            ])
        } else {
            None
        }
    }

    /// Verifies that code inside the worktree passes build or test validation before merging.
    ///
    /// - If `check_cmd` is `Some("skip")`, returns immediate success without running any command.
    /// - If `check_cmd` is `Some(custom)`, executes the custom command in `worktree_path`.
    /// - If `check_cmd` is `None`, automatically detects project validation command; if none detected, returns success.
    /// - Enforces a 60-second timeout on command execution.
    pub fn verify_worktree(
        worktree_path: &Path,
        check_cmd: Option<&str>,
    ) -> Result<ValidationReport, ArbitrationError> {
        if !worktree_path.exists() {
            return Err(ArbitrationError::WorktreeNotFound(
                worktree_path.display().to_string(),
            ));
        }

        if let Some(cmd) = check_cmd {
            if cmd.trim() == "skip" {
                return Ok(ValidationReport {
                    success: true,
                    command: "skip".to_string(),
                    exit_code: 0,
                    stdout: String::new(),
                    stderr: String::new(),
                    duration_ms: 0,
                });
            }
        }

        let cmd_args = match check_cmd {
            Some(custom) => {
                let parts: Vec<String> = custom.split_whitespace().map(|s| s.to_string()).collect();
                if parts.is_empty() {
                    return Ok(ValidationReport {
                        success: true,
                        command: String::new(),
                        exit_code: 0,
                        stdout: String::new(),
                        stderr: String::new(),
                        duration_ms: 0,
                    });
                }
                parts
            }
            None => match Self::detect_project_validation_cmd(worktree_path) {
                Some(detected) => detected,
                None => {
                    return Ok(ValidationReport {
                        success: true,
                        command: "none".to_string(),
                        exit_code: 0,
                        stdout: "No project validation command detected".to_string(),
                        stderr: String::new(),
                        duration_ms: 0,
                    });
                }
            },
        };

        let command_str = cmd_args.join(" ");
        let start = std::time::Instant::now();
        let timeout = std::time::Duration::from_secs(60);

        let mut cmd = Command::new(&cmd_args[0]);
        if cmd_args.len() > 1 {
            cmd.args(&cmd_args[1..]);
        }
        cmd.current_dir(worktree_path);

        let output_res = run_command_with_timeout(cmd, timeout);
        let duration_ms = start.elapsed().as_millis() as u64;

        match output_res {
            Ok(output) => {
                let success = output.status.success();
                let exit_code = output.status.code().unwrap_or(if success { 0 } else { -1 });
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                Ok(ValidationReport {
                    success,
                    command: command_str,
                    exit_code,
                    stdout,
                    stderr,
                    duration_ms,
                })
            }
            Err(e) if e.kind() == std::io::ErrorKind::TimedOut => Ok(ValidationReport {
                success: false,
                command: command_str,
                exit_code: -1,
                stdout: String::new(),
                stderr: format!("Validation command timed out after {}s", timeout.as_secs()),
                duration_ms,
            }),
            Err(e) => Err(ArbitrationError::IoError(e)),
        }
    }

    /// Checks whether `branch_name` can be cleanly merged into `HEAD` in `repo_root`
    /// without conflicts, using `git merge-tree` to avoid dirtying the working directory.
    pub fn check_mergeability(
        repo_root: &Path,
        branch_name: &str,
    ) -> Result<MergeabilityReport, ArbitrationError> {
        if !repo_root.exists() {
            return Err(ArbitrationError::IoError(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Repository root does not exist: {}", repo_root.display()),
            )));
        }

        // 1. Determine merge-base between HEAD and branch
        let merge_base_output = Command::new("git")
            .args(["merge-base", "HEAD", branch_name])
            .current_dir(repo_root)
            .output()?;

        if !merge_base_output.status.success() {
            let stderr = String::from_utf8_lossy(&merge_base_output.stderr).to_string();
            return Err(ArbitrationError::GitError(format!(
                "Failed to compute merge-base for HEAD and '{}': {}",
                branch_name,
                stderr.trim()
            )));
        }

        let base_raw = String::from_utf8_lossy(&merge_base_output.stdout)
            .trim()
            .to_string();
        let merge_base = if base_raw.is_empty() {
            None
        } else {
            Some(base_raw)
        };

        // 2. Perform 3-way merge inspection using git merge-tree --write-tree
        let merge_tree_output = Command::new("git")
            .args(["merge-tree", "--write-tree", "HEAD", branch_name])
            .current_dir(repo_root)
            .output()?;

        if merge_tree_output.status.success() {
            return Ok(MergeabilityReport {
                can_merge_cleanly: true,
                conflicted_files: Vec::new(),
                merge_base,
            });
        }

        // Exit code 1 indicates merge conflicts were found
        if merge_tree_output.status.code() == Some(1) {
            let stdout = String::from_utf8_lossy(&merge_tree_output.stdout);
            let mut conflicted_files = Vec::new();

            for line in stdout.lines() {
                // Line format in git merge-tree unmerged section: <mode> <hash> <stage>\t<path>
                if let Some((info, path)) = line.split_once('\t') {
                    let parts: Vec<&str> = info.split_whitespace().collect();
                    if parts.len() == 3 && (parts[2] == "1" || parts[2] == "2" || parts[2] == "3") {
                        let trimmed = path.trim().to_string();
                        if !trimmed.is_empty() && !conflicted_files.contains(&trimmed) {
                            conflicted_files.push(trimmed);
                        }
                    }
                } else if let Some(idx) = line.find("CONFLICT (") {
                    if let Some(colon_idx) = line[idx..].find(':') {
                        let conflict_info = line[idx + colon_idx + 1..].trim();
                        if let Some(rest) = conflict_info.strip_prefix("Merge conflict in ") {
                            let trimmed = rest.trim().to_string();
                            if !trimmed.is_empty() && !conflicted_files.contains(&trimmed) {
                                conflicted_files.push(trimmed);
                            }
                        } else if let Some((first_word, _)) = conflict_info.split_once(' ') {
                            let candidate = first_word.trim().to_string();
                            if !candidate.is_empty() && !conflicted_files.contains(&candidate) {
                                conflicted_files.push(candidate);
                            }
                        }
                    }
                }
            }

            return Ok(MergeabilityReport {
                can_merge_cleanly: false,
                conflicted_files,
                merge_base,
            });
        }

        // Other non-zero exit code: real git error
        let stderr = String::from_utf8_lossy(&merge_tree_output.stderr).to_string();
        Err(ArbitrationError::GitError(format!(
            "git merge-tree failed: {}",
            stderr.trim()
        )))
    }

    /// Merges changes from `branch_name` into the main workspace at `repo_root`.
    ///
    /// - If `commit` is `true`, commits the merge with `--no-ff` and the specified or default message.
    /// - If `commit` is `false`, leaves changes in the working tree without committing.
    /// - Validates mergeability first; returns `ArbitrationError::MergeConflict` if conflicts exist.
    pub fn apply_merge(
        repo_root: &Path,
        branch_name: &str,
        commit: bool,
        commit_msg: Option<&str>,
    ) -> Result<MergeSuccessReport, ArbitrationError> {
        let mergeability = Self::check_mergeability(repo_root, branch_name)?;
        if !mergeability.can_merge_cleanly {
            return Err(ArbitrationError::MergeConflict(
                mergeability.conflicted_files,
            ));
        }

        // Capture list of changed files
        let diff_out = Command::new("git")
            .args(["diff", "--name-only", "HEAD...", branch_name])
            .current_dir(repo_root)
            .output()?;

        let mut files_changed: Vec<String> = if diff_out.status.success() {
            String::from_utf8_lossy(&diff_out.stdout)
                .lines()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        } else {
            Vec::new()
        };

        if files_changed.is_empty() {
            let fallback = Command::new("git")
                .args(["diff", "--name-only", "HEAD", branch_name])
                .current_dir(repo_root)
                .output()?;
            if fallback.status.success() {
                files_changed = String::from_utf8_lossy(&fallback.stdout)
                    .lines()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
            }
        }

        let subagent_id = extract_subagent_id(branch_name);

        if commit {
            let default_msg = format!(
                "merge(subagent-{}): land verified subagent changes",
                subagent_id
            );
            let msg = commit_msg.unwrap_or(&default_msg);

            let merge_out = Command::new("git")
                .args(["merge", "--no-ff", "-m", msg, branch_name])
                .current_dir(repo_root)
                .output()?;

            if !merge_out.status.success() {
                let _ = Command::new("git")
                    .args(["merge", "--abort"])
                    .current_dir(repo_root)
                    .output();
                let stderr = String::from_utf8_lossy(&merge_out.stderr).to_string();
                return Err(ArbitrationError::GitError(format!(
                    "git merge failed: {}",
                    stderr.trim()
                )));
            }

            let rev_out = Command::new("git")
                .args(["rev-parse", "HEAD"])
                .current_dir(repo_root)
                .output()?;

            let commit_hash = if rev_out.status.success() {
                let hash = String::from_utf8_lossy(&rev_out.stdout).trim().to_string();
                if hash.is_empty() {
                    None
                } else {
                    Some(hash)
                }
            } else {
                None
            };

            Ok(MergeSuccessReport {
                subagent_id,
                branch_name: branch_name.to_string(),
                files_changed,
                committed: true,
                commit_hash,
                commit_message: Some(msg.to_string()),
            })
        } else {
            let merge_out = Command::new("git")
                .args(["merge", "--no-commit", "--no-ff", branch_name])
                .current_dir(repo_root)
                .output()?;

            if !merge_out.status.success() {
                let _ = Command::new("git")
                    .args(["merge", "--abort"])
                    .current_dir(repo_root)
                    .output();
                let stderr = String::from_utf8_lossy(&merge_out.stderr).to_string();
                return Err(ArbitrationError::GitError(format!(
                    "git merge failed: {}",
                    stderr.trim()
                )));
            }

            Ok(MergeSuccessReport {
                subagent_id,
                branch_name: branch_name.to_string(),
                files_changed,
                committed: false,
                commit_hash: None,
                commit_message: None,
            })
        }
    }
}

/// Extracts clean subagent identifier from branch name conventions.
#[allow(dead_code)]
fn extract_subagent_id(branch_name: &str) -> String {
    if let Some(rest) = branch_name.strip_prefix("minicode-task-") {
        rest.to_string()
    } else if let Some(rest) = branch_name.strip_prefix("minicode/subagent/") {
        rest.to_string()
    } else {
        branch_name.to_string()
    }
}

/// Helper to execute a command with a timeout without blocking indefinitely.
#[allow(dead_code)]
fn run_command_with_timeout(
    mut cmd: Command,
    timeout: std::time::Duration,
) -> Result<std::process::Output, std::io::Error> {
    let child = cmd
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;

    let (tx, rx) = std::sync::mpsc::channel();
    let child_id = child.id();
    std::thread::spawn(move || {
        let res = child.wait_with_output();
        let _ = tx.send(res);
    });

    match rx.recv_timeout(timeout) {
        Ok(res) => res,
        Err(_) => {
            #[cfg(unix)]
            unsafe {
                libc::kill(child_id as libc::pid_t, libc::SIGKILL);
            }
            #[cfg(not(unix))]
            {
                let _ = Command::new("taskkill")
                    .args(["/PID", &child_id.to_string(), "/F"])
                    .output();
            }
            Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                format!("Process timed out after {}s", timeout.as_secs()),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_project_validation_cmd_cargo() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(
            temp.path().join("Cargo.toml"),
            "[package]\nname = \"demo\"\n",
        )
        .unwrap();
        let cmd = MergeArbitrator::detect_project_validation_cmd(temp.path()).unwrap();
        assert_eq!(cmd, vec!["cargo", "check", "-j", "1"]);
    }

    #[test]
    fn test_detect_project_validation_cmd_package_json() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("package.json"), "{\"name\":\"demo\"}").unwrap();
        let cmd = MergeArbitrator::detect_project_validation_cmd(temp.path()).unwrap();
        assert_eq!(cmd, vec!["npm", "test"]);
    }

    #[test]
    fn test_verify_worktree_skip() {
        let temp = tempfile::tempdir().unwrap();
        let report = MergeArbitrator::verify_worktree(temp.path(), Some("skip")).unwrap();
        assert!(report.success);
        assert_eq!(report.command, "skip");
    }

    #[test]
    fn test_mergeability_and_apply_clean() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();

        // git init
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(root)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["config", "user.name", "test"])
            .current_dir(root)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(root)
            .output()
            .unwrap();

        std::fs::write(root.join("hello.txt"), "base\n").unwrap();
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(root)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(root)
            .output()
            .unwrap();

        // create branch
        let branch = "minicode/subagent/coder-1";
        std::process::Command::new("git")
            .args(["branch", branch])
            .current_dir(root)
            .output()
            .unwrap();

        // modify on branch
        let worktree_dir = root.join("wt");
        std::process::Command::new("git")
            .args(["worktree", "add", worktree_dir.to_str().unwrap(), branch])
            .current_dir(root)
            .output()
            .unwrap();
        std::fs::write(worktree_dir.join("hello.txt"), "base\nupdated\n").unwrap();
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(&worktree_dir)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["commit", "-m", "branch commit"])
            .current_dir(&worktree_dir)
            .output()
            .unwrap();

        // check mergeability
        let mergeability = MergeArbitrator::check_mergeability(root, branch).unwrap();
        assert!(mergeability.can_merge_cleanly);
        assert!(mergeability.conflicted_files.is_empty());

        // apply merge
        let success =
            MergeArbitrator::apply_merge(root, branch, true, Some("merge subagent")).unwrap();
        assert!(success.committed);
        assert!(success.files_changed.contains(&"hello.txt".to_string()));

        let content = std::fs::read_to_string(root.join("hello.txt")).unwrap();
        assert!(content.contains("updated"));
    }
}

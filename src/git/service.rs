use std::path::PathBuf;
use std::time::Duration;
use tokio::process::Command;

use crate::constants::GIT_TIMEOUT_SECS;
use crate::error::{GitError, Result};
use crate::git::diff_filter::DiffFilter;

/// Structured representation of the git working tree status
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitStatus {
    pub branch: String,
    pub is_clean: bool,
    pub staged: Vec<String>,
    pub unstaged: Vec<String>,
    pub untracked: Vec<String>,
    pub conflicted: Vec<String>,
}

/// Information about a merge conflict in a file
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConflictFile {
    pub path: String,
    pub conflict_markers_count: usize,
    pub snippet: String,
}

/// Hardened Git service providing zero-dependency CLI operations with subprocess isolation.
pub struct GitService {
    workspace_root: PathBuf,
}

impl GitService {
    /// Creates a new GitService bound to the specified workspace directory.
    pub fn new(workspace_root: PathBuf) -> Self {
        Self { workspace_root }
    }

    /// Checks if the workspace root is a valid git repository.
    pub async fn is_git_repo(&self) -> bool {
        self.run_git(&["rev-parse", "--is-inside-work-tree"])
            .await
            .map(|out| out.trim() == "true")
            .unwrap_or(false)
    }

    /// Ensures that `.minicode/` internal directories are ignored in `.git/info/exclude`.
    pub fn ensure_git_exclude(&self) {
        let exclude_file = self
            .workspace_root
            .join(".git")
            .join("info")
            .join("exclude");
        if let Ok(content) = std::fs::read_to_string(&exclude_file) {
            if !content.contains(".minicode") {
                let mut updated = content;
                if !updated.is_empty() && !updated.ends_with('\n') {
                    updated.push('\n');
                }
                updated.push_str(".minicode/\n");
                let _ = std::fs::write(&exclude_file, updated);
            }
        }
    }

    /// Returns the current active git branch name (e.g. `main`).
    pub async fn get_current_branch(&self) -> Result<String> {
        let branch = self.run_git(&["rev-parse", "--abbrev-ref", "HEAD"]).await?;
        Ok(branch.trim().to_string())
    }

    /// Returns structured git status with porcelain parsing.
    pub async fn get_status(&self) -> Result<GitStatus> {
        let branch = self
            .get_current_branch()
            .await
            .unwrap_or_else(|_| "HEAD".to_string());
        let output = self.run_git(&["status", "--porcelain=v1"]).await?;

        let mut staged = Vec::new();
        let mut unstaged = Vec::new();
        let mut untracked = Vec::new();
        let mut conflicted = Vec::new();

        for line in output.lines() {
            if line.len() < 3 {
                continue;
            }
            let index_status = line.chars().next().unwrap_or(' ');
            let worktree_status = line.chars().nth(1).unwrap_or(' ');
            let file_path = line[3..].trim().to_string();

            // Check merge conflicts
            if matches!(
                (index_status, worktree_status),
                ('U', 'U') | ('A', 'A') | ('D', 'D') | ('A', 'U') | ('U', 'D') | ('D', 'U')
            ) {
                conflicted.push(file_path.clone());
                continue;
            }

            if index_status == '?' && worktree_status == '?' {
                untracked.push(file_path);
                continue;
            }

            if index_status != ' ' && index_status != '?' {
                staged.push(file_path.clone());
            }

            if worktree_status != ' ' && worktree_status != '?' {
                unstaged.push(file_path);
            }
        }

        let is_clean = staged.is_empty()
            && unstaged.is_empty()
            && untracked.is_empty()
            && conflicted.is_empty();

        Ok(GitStatus {
            branch,
            is_clean,
            staged,
            unstaged,
            untracked,
            conflicted,
        })
    }

    /// Returns the filtered and token-budgeted git diff.
    pub async fn diff(&self, staged_only: bool, paths: Option<&[String]>) -> Result<String> {
        let mut args = vec!["diff"];
        if staged_only {
            args.push("--staged");
        }

        let mut string_args = Vec::new();
        if let Some(specific_paths) = paths {
            if !specific_paths.is_empty() {
                args.push("--");
                for p in specific_paths {
                    string_args.push(p.as_str());
                }
                args.extend(&string_args);
            }
        }

        let raw_diff = self.run_git(&args).await?;
        Ok(DiffFilter::filter_diff(&raw_diff))
    }

    /// Returns the content of a file at git HEAD (useful for AST diffs).
    pub async fn show_file_at_head(&self, relative_path: &str) -> Result<String> {
        let target = format!("HEAD:{}", relative_path.trim_start_matches('/'));
        self.run_git(&["show", &target]).await
    }

    /// Returns the recent git commit log.
    pub async fn log(&self, count: usize) -> Result<String> {
        let count_arg = format!("-n{}", count);
        let args = ["log", "--oneline", "--decorate", &count_arg];
        self.run_git(&args).await
    }

    /// Stages modified files (or all if none specified).
    pub async fn stage_files(&self, paths: Option<&[String]>) -> Result<()> {
        if let Some(file_list) = paths {
            if !file_list.is_empty() {
                let mut args = vec!["add", "--"];
                for p in file_list {
                    args.push(p.as_str());
                }
                self.run_git(&args).await?;
                return Ok(());
            }
        }

        self.run_git(&["add", "-A"]).await?;
        Ok(())
    }

    /// Unstages modified files (or all if none specified).
    pub async fn unstage_files(&self, paths: Option<&[String]>) -> Result<()> {
        if let Some(file_list) = paths {
            if !file_list.is_empty() {
                let mut args = vec!["restore", "--staged", "--"];
                for p in file_list {
                    args.push(p.as_str());
                }
                self.run_git(&args).await?;
                return Ok(());
            }
        }

        self.run_git(&["restore", "--staged", "."]).await?;
        Ok(())
    }

    /// Scans workspace files for unresolved git merge conflicts (`<<<<<<<`, `=======`, `>>>>>>>`).
    pub async fn find_conflicts(&self) -> Result<Vec<ConflictFile>> {
        let status = self.get_status().await?;
        let mut conflict_files = Vec::new();

        // Check conflicted files from status first, otherwise scan unstaged/modified
        let candidate_files = if !status.conflicted.is_empty() {
            status.conflicted
        } else {
            status.unstaged
        };

        for rel_path in candidate_files {
            let full_path = self.workspace_root.join(&rel_path);
            if let Ok(content) = tokio::fs::read_to_string(&full_path).await {
                if content.contains("<<<<<<<") && content.contains(">>>>>>>") {
                    let mut markers_count = 0;
                    let mut snippet = String::new();
                    let mut recording = false;
                    let mut snippet_lines = 0;

                    for line in content.lines() {
                        if line.starts_with("<<<<<<<") {
                            markers_count += 1;
                            recording = true;
                        }

                        if recording && snippet_lines < 20 {
                            snippet.push_str(line);
                            snippet.push('\n');
                            snippet_lines += 1;
                        }

                        if line.starts_with(">>>>>>>") {
                            recording = false;
                        }
                    }

                    conflict_files.push(ConflictFile {
                        path: rel_path,
                        conflict_markers_count: markers_count,
                        snippet,
                    });
                }
            }
        }

        Ok(conflict_files)
    }

    /// Creates a pull request using the system's `gh` (GitHub CLI) binary if available.
    pub async fn create_pull_request(
        &self,
        title: &str,
        body: &str,
        base: Option<&str>,
        draft: bool,
    ) -> Result<String> {
        // Verify gh CLI presence
        let check_gh = Command::new("gh")
            .arg("--version")
            .output()
            .await
            .map_err(|_| GitError::GhCliNotFound)?;

        if !check_gh.status.success() {
            return Err(GitError::GhCliNotFound.into());
        }

        let mut args = vec!["pr", "create", "--title", title, "--body", body];
        if let Some(base_branch) = base {
            args.push("--base");
            args.push(base_branch);
        }
        if draft {
            args.push("--draft");
        }

        let output = Command::new("gh")
            .args(&args)
            .current_dir(&self.workspace_root)
            .env("GIT_TERMINAL_PROMPT", "0")
            .output()
            .await
            .map_err(|e| GitError::CommandFailed {
                cmd: format!("gh {}", args.join(" ")),
                code: None,
                stderr: e.to_string(),
            })?;

        if !output.status.success() {
            return Err(GitError::CommandFailed {
                cmd: format!("gh {}", args.join(" ")),
                code: output.status.code(),
                stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            }
            .into());
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    /// Executes a Git command with mandatory non-interactive environment isolation.
    pub async fn run_git(&self, args: &[&str]) -> Result<String> {
        let mut cmd = Command::new("git");
        cmd.arg("--no-pager")
            .args(args)
            .current_dir(&self.workspace_root)
            .env("GIT_TERMINAL_PROMPT", "0") // Prevent blocking on credentials
            .env("GIT_PAGER", "cat") // Prevent interactive pager (less/more)
            .env("LC_ALL", "C"); // Deterministic English output

        let child = cmd.output();
        let timeout_dur = Duration::from_secs(GIT_TIMEOUT_SECS);

        let output = match tokio::time::timeout(timeout_dur, child).await {
            Ok(res) => res.map_err(|e| GitError::CommandFailed {
                cmd: format!("git {}", args.join(" ")),
                code: None,
                stderr: e.to_string(),
            })?,
            Err(_) => {
                return Err(GitError::Timeout {
                    timeout_secs: GIT_TIMEOUT_SECS,
                }
                .into());
            }
        };

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let combined = if stderr.trim().is_empty() {
                stdout
            } else {
                stderr
            };

            if combined.contains("not a git repository") {
                return Err(GitError::NotARepo {
                    path: self.workspace_root.display().to_string(),
                }
                .into());
            }

            return Err(GitError::CommandFailed {
                cmd: format!("git {}", args.join(" ")),
                code: output.status.code(),
                stderr: combined,
            }
            .into());
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
}

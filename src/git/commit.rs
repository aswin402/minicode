use crate::constants::GIT_COMMIT_MSG_MAX_LEN;
use crate::error::{GitError, Result};
use crate::git::service::GitService;

/// Service for committing, branching, and rolling back git changes.
pub struct GitCommitService<'a> {
    git: &'a GitService,
}

impl<'a> GitCommitService<'a> {
    /// Creates a new GitCommitService backed by the given GitService.
    pub fn new(git: &'a GitService) -> Self {
        Self { git }
    }

    /// Stages modified files and creates a git commit, returning the created commit hash.
    pub async fn commit(&self, message: &str, paths: Option<&[String]>) -> Result<String> {
        let trimmed_msg = message.trim();
        if trimmed_msg.is_empty() {
            return Err(GitError::CommandFailed {
                cmd: "git commit".into(),
                code: None,
                stderr: "Commit message cannot be empty".into(),
            }
            .into());
        }

        // 1. Stage files
        self.git.stage_files(paths).await?;

        // 2. Execute commit
        self.git.run_git(&["commit", "-m", trimmed_msg]).await?;

        // 3. Extract new commit hash
        let hash = self.git.run_git(&["rev-parse", "HEAD"]).await?;
        Ok(hash.trim().to_string())
    }

    /// Rolls back the latest commit while preserving all modified files in the working tree (`git reset --soft HEAD~1`).
    pub async fn undo_last_commit(&self) -> Result<()> {
        let parent_check = self.git.run_git(&["rev-parse", "HEAD~1"]).await;
        if parent_check.is_err() {
            return Err(GitError::NoCommits.into());
        }

        self.git.run_git(&["reset", "--soft", "HEAD~1"]).await?;
        Ok(())
    }

    /// Creates and switches to a new git branch.
    pub async fn create_branch(&self, branch_name: &str) -> Result<()> {
        self.git.run_git(&["checkout", "-b", branch_name]).await?;
        Ok(())
    }

    /// Switches to an existing git branch.
    pub async fn checkout_branch(&self, branch_name: &str) -> Result<()> {
        self.git.run_git(&["checkout", branch_name]).await?;
        Ok(())
    }

    /// Lists all local branch names.
    pub async fn list_branches(&self) -> Result<Vec<String>> {
        let output = self
            .git
            .run_git(&["branch", "--list", "--format=%(refname:short)"])
            .await?;
        let branches = output
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect();
        Ok(branches)
    }

    /// Generates a Conventional Commit message from modified file paths and optional turn summary.
    pub fn generate_conventional_message(
        files_modified: &[String],
        task_summary: Option<&str>,
    ) -> String {
        if files_modified.is_empty() {
            return task_summary
                .map(|s| s.to_string())
                .unwrap_or_else(|| "chore: update workspace".to_string());
        }

        // Determine scope and prefix based on file patterns
        let has_tests = files_modified
            .iter()
            .any(|f| f.contains("test") || f.ends_with("_test.rs"));
        let has_docs = files_modified
            .iter()
            .any(|f| f.ends_with(".md") || f.starts_with("docs/"));
        let has_src = files_modified.iter().any(|f| {
            f.starts_with("src/") || f.ends_with(".rs") || f.ends_with(".ts") || f.ends_with(".py")
        });
        let has_config = files_modified.iter().any(|f| {
            f.ends_with(".toml")
                || f.ends_with(".json")
                || f.ends_with(".yml")
                || f.ends_with(".yaml")
        });

        let prefix = if let Some(summary) = task_summary {
            let lower = summary.to_lowercase();
            if lower.contains("fix") || lower.contains("bug") || lower.contains("error") {
                "fix"
            } else if lower.contains("test") {
                "test"
            } else if lower.contains("doc") || lower.contains("readme") {
                "docs"
            } else if lower.contains("refactor") || lower.contains("clean") {
                "refactor"
            } else if has_src {
                "feat"
            } else {
                "chore"
            }
        } else if has_tests && !has_src {
            "test"
        } else if has_docs && !has_src {
            "docs"
        } else if has_config && !has_src {
            "chore"
        } else {
            "feat"
        };

        // Determine concise summary
        let summary_text = if let Some(summary) = task_summary {
            let cleaned = summary
                .trim_start_matches(|c: char| !c.is_alphanumeric())
                .trim();
            if cleaned.len() > GIT_COMMIT_MSG_MAX_LEN - 15 {
                let mut truncated = cleaned[..GIT_COMMIT_MSG_MAX_LEN - 18].to_string();
                truncated.push_str("...");
                truncated
            } else {
                cleaned.to_string()
            }
        } else if files_modified.len() == 1 {
            format!("update {}", files_modified[0])
        } else {
            format!("update {} files", files_modified.len())
        };

        format!("{}: {}", prefix, summary_text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_conventional_message_with_task() {
        let files = vec!["src/agent/loop.rs".to_string()];
        let msg =
            GitCommitService::generate_conventional_message(&files, Some("Fix streaming tool bug"));
        assert_eq!(msg, "fix: Fix streaming tool bug");
    }

    #[test]
    fn test_generate_conventional_message_docs_only() {
        let files = vec!["README.md".to_string()];
        let msg = GitCommitService::generate_conventional_message(&files, None);
        assert_eq!(msg, "docs: update README.md");
    }
}

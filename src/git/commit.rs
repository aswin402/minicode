use crate::constants::GIT_COMMIT_MSG_MAX_LEN;
use crate::error::{GitError, Result};
use crate::git::service::GitService;

/// Service for committing, branching, and rolling back git changes.
pub struct GitCommitService<'a> {
    git: &'a GitService,
    /// When true, `commit` stages ALL workspace changes (`git add -A`)
    /// instead of only the explicitly listed paths (dirty_commit config).
    stage_all: bool,
}

impl<'a> GitCommitService<'a> {
    /// Creates a new GitCommitService backed by the given GitService.
    pub fn new(git: &'a GitService) -> Self {
        Self {
            git,
            stage_all: false,
        }
    }

    /// Builder: stage every workspace change, not just the listed files.
    pub fn with_stage_all(mut self, yes: bool) -> Self {
        self.stage_all = yes;
        self
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
        if self.stage_all {
            self.git.run_git(&["add", "-A"]).await?;
        } else {
            self.git.stage_files(paths).await?;
        }

        // 2. Execute commit
        match self.git.run_git(&["commit", "-m", trimmed_msg]).await {
            Ok(_) => {}
            Err(e) => {
                let err_str = e.to_string();
                if err_str.contains("nothing to commit") || err_str.contains("working tree clean") {
                    let hash = self
                        .git
                        .run_git(&["rev-parse", "HEAD"])
                        .await
                        .unwrap_or_else(|_| "HEAD".to_string());
                    return Ok(format!(
                        "{} (clean working tree, nothing new to commit)",
                        hash.trim()
                    ));
                }
                return Err(e);
            }
        }

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

    /// Extracts a clean, single-line primary headline from a user prompt or plan title.
    /// Returns `None` if the input is empty or is a single-word continuation/acknowledgement
    /// cue (e.g. `"continue"`, `"proceed"`, `"ok"`, `"yes"`).
    pub fn extract_meaningful_headline(raw: &str) -> Option<String> {
        let first_line = raw
            .lines()
            .map(str::trim)
            .find(|l| !l.is_empty() && !l.starts_with("```"))?;

        // Strip markdown inline decorators (*, `, #, >) and normalize whitespace
        let stripped: String = first_line
            .chars()
            .filter(|&c| !matches!(c, '*' | '`' | '#' | '>'))
            .collect();
        let first_sentence = stripped
            .split(". ")
            .next()
            .unwrap_or(&stripped)
            .trim_start_matches(|c: char| !c.is_alphanumeric())
            .trim();

        let word_count = first_sentence
            .split_whitespace()
            .filter(|w| w.chars().any(|c| c.is_alphanumeric()))
            .count();
        if word_count < 2 {
            return None;
        }
        Some(first_sentence.to_string())
    }

    /// Generates a Conventional Commit message from modified file paths and optional turn summary.
    pub fn generate_conventional_message(
        files_modified: &[String],
        task_summary: Option<&str>,
    ) -> String {
        let headline = task_summary.and_then(Self::extract_meaningful_headline);
        if files_modified.is_empty() {
            return headline.unwrap_or_else(|| "chore: update workspace".to_string());
        }

        // Determine scope and prefix based on file patterns
        let has_tests = files_modified
            .iter()
            .any(|f| f.contains("test") || f.ends_with("_test.rs") || f.contains(".spec."));
        let has_docs = files_modified
            .iter()
            .any(|f| f.ends_with(".md") || f.starts_with("docs/"));
        let has_src = files_modified.iter().any(|f| {
            f.starts_with("src/")
                || f.ends_with(".rs")
                || f.ends_with(".ts")
                || f.ends_with(".tsx")
                || f.ends_with(".js")
                || f.ends_with(".jsx")
                || f.ends_with(".py")
                || f.ends_with(".go")
                || f.ends_with(".html")
                || f.ends_with(".css")
        });
        let has_config = files_modified.iter().any(|f| {
            f.ends_with(".toml")
                || f.ends_with(".json")
                || f.ends_with(".yml")
                || f.ends_with(".yaml")
        });

        let prefix = if let Some(ref head) = headline {
            let lead_tokens: Vec<String> = head
                .split(|c: char| !c.is_alphanumeric())
                .filter(|w| !w.is_empty())
                .take(8)
                .map(|w| w.to_ascii_lowercase())
                .collect();

            let has_lead =
                |candidates: &[&str]| lead_tokens.iter().any(|t| candidates.contains(&t.as_str()));

            if has_lead(&[
                "create",
                "build",
                "implement",
                "add",
                "scaffold",
                "design",
                "develop",
                "introduce",
                "ship",
            ]) {
                "feat"
            } else if has_lead(&[
                "fix", "bug", "bugfix", "hotfix", "repair", "resolve", "patch", "crash", "error",
                "broken",
            ]) {
                "fix"
            } else if has_lead(&["refactor", "restructure", "simplify", "clean", "cleanup"]) {
                "refactor"
            } else if has_lead(&["test", "tests", "testing", "spec", "coverage"]) {
                "test"
            } else if has_lead(&["doc", "docs", "document", "readme", "changelog"]) {
                "docs"
            } else if has_src {
                "feat"
            } else if has_tests {
                "test"
            } else if has_docs {
                "docs"
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
        let summary_text = if let Some(ref head) = headline {
            crate::utils::strings::truncate_display(head, GIT_COMMIT_MSG_MAX_LEN - 15)
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

    #[test]
    fn test_generate_conventional_message_ignores_body_bug_mentions_and_single_word_continue() {
        let files = vec![
            "src/components/Hero.tsx".to_string(),
            "src/index.css".to_string(),
        ];
        let prompt = "Create a premium, modern landing page for **minicode**, an autonomous AI coding agent.\n\nExplain that minicode does not simply claim a bug is fixed without verification.";
        let msg = GitCommitService::generate_conventional_message(&files, Some(prompt));
        assert!(msg.starts_with("feat: Create a premium, modern landing page for minicode"));
        assert!(!msg.contains("**"));

        let cont_msg = GitCommitService::generate_conventional_message(&files, Some("continue"));
        assert_eq!(cont_msg, "feat: update 2 files");
    }
}

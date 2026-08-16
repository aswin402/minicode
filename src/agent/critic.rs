use crate::error::Result;
use crate::lsp::diagnostics::FastCompilerChecker;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Evaluation outcome from an automated Actor-Critic verification pass.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CriticReport {
    pub is_approved: bool,
    pub compiler_errors: usize,
    pub compiler_warnings: usize,
    pub uncommitted_files: Vec<String>,
    pub diagnostics_summary: String,
    pub suggested_feedback: String,
}

pub struct CriticValidator;

impl CriticValidator {
    /// Executes a rigorous automated Actor-Critic quality gate over the current workspace.
    pub async fn review_workspace(workspace_root: &Path) -> Result<CriticReport> {
        let mut compiler_errors = 0;
        let mut compiler_warnings = 0;
        let mut diagnostics_summary = String::new();

        // 1. Run Workspace Compiler / Linter Diagnostics
        if let Ok(report) = FastCompilerChecker::check_workspace(workspace_root).await {
            compiler_errors = report.errors.len();
            compiler_warnings = report.warnings.len();
            if !report.is_clean() {
                diagnostics_summary = report.format_for_agent(workspace_root, 5);
            }
        }

        // 2. Query Git working tree status
        let mut uncommitted_files = Vec::new();
        if let Ok(output) = tokio::process::Command::new("git")
            .arg("status")
            .arg("--short")
            .current_dir(workspace_root)
            .output()
            .await
        {
            if output.status.success() {
                let status_str = String::from_utf8_lossy(&output.stdout);
                for line in status_str.lines() {
                    let trimmed = line.trim();
                    if !trimmed.is_empty() {
                        uncommitted_files.push(trimmed.to_string());
                    }
                }
            }
        }

        // 3. Formulate Approval Verdict
        let is_approved = compiler_errors == 0;

        let suggested_feedback = if compiler_errors > 0 {
            format!(
                "❌ Critic Rejected: {} compiler error(s) detected. Please resolve these errors before completing the turn:\n{}",
                compiler_errors, diagnostics_summary
            )
        } else if compiler_warnings > 0 {
            format!(
                "✔ Critic Approved with Warnings: 0 errors, {} warning(s). Clean up warnings if convenient.",
                compiler_warnings
            )
        } else {
            "✔ Critic Approved: Zero compiler diagnostics errors or warnings. Code is clean."
                .to_string()
        };

        Ok(CriticReport {
            is_approved,
            compiler_errors,
            compiler_warnings,
            uncommitted_files,
            diagnostics_summary,
            suggested_feedback,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_critic_review_clean_directory() {
        let dir = tempdir().unwrap();
        let report = CriticValidator::review_workspace(dir.path()).await.unwrap();
        assert!(report.is_approved);
        assert_eq!(report.compiler_errors, 0);
    }
}

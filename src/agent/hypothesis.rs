use crate::error::{Result, ToolError};
use crate::git::worktree::WorktreeManager;
use crate::lsp::diagnostics::FastCompilerChecker;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Status of a speculative hypothesis branch.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum BranchStatus {
    Pending,
    Evaluating,
    Passed,
    Failed,
    Selected,
    Discarded,
}

/// Metadata and evaluation results for a single speculative branch.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HypothesisBranch {
    pub id: String,
    pub description: String,
    pub worktree_path: PathBuf,
    pub status: BranchStatus,
    pub compiler_clean: bool,
    pub compiler_errors: usize,
    pub compiler_warnings: usize,
    pub fitness_score: f32,
    pub notes: String,
}

/// Session manager for multi-branch hypothesis exploration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HypothesisSession {
    pub id: String,
    pub branches: Vec<HypothesisBranch>,
}

pub struct HypothesisEngine;

impl HypothesisEngine {
    /// Creates a set of parallel speculative Git worktree branches to explore alternative hypotheses.
    pub async fn create_branches(
        workspace_root: &Path,
        descriptions: &[String],
    ) -> Result<HypothesisSession> {
        let session_id = format!("hyp_{}", chrono::Utc::now().format("%Y%m%d_%H%M%S"));
        let mut branches = Vec::new();
        let wt_mgr = WorktreeManager::new(workspace_root);

        for (i, desc) in descriptions.iter().enumerate() {
            let branch_id = format!("{}_b{}", session_id, i + 1);
            let worktree_dir = workspace_root
                .join(".minicode")
                .join("worktrees")
                .join(&branch_id);

            // Create worktree for this hypothesis branch if git is initialized
            if workspace_root.join(".git").exists() {
                let _ = wt_mgr.create_worktree(&branch_id).await;
            } else {
                let _ = fs::create_dir_all(&worktree_dir);
            }

            branches.push(HypothesisBranch {
                id: branch_id,
                description: desc.clone(),
                worktree_path: worktree_dir,
                status: BranchStatus::Pending,
                compiler_clean: false,
                compiler_errors: 0,
                compiler_warnings: 0,
                fitness_score: 0.0,
                notes: String::new(),
            });
        }

        let session = HypothesisSession {
            id: session_id,
            branches,
        };

        Self::save_session(workspace_root, &session)?;
        Ok(session)
    }

    /// Evaluates a branch by running compiler diagnostics and scoring fitness.
    pub async fn evaluate_branch(
        workspace_root: &Path,
        branch_id: &str,
    ) -> Result<HypothesisBranch> {
        let mut session = Self::load_session(workspace_root)?;
        let branch = session
            .branches
            .iter_mut()
            .find(|b| b.id == branch_id)
            .ok_or_else(|| ToolError::NotFound {
                name: format!("branch:{}", branch_id),
            })?;

        branch.status = BranchStatus::Evaluating;

        // Check compiler diagnostics on the branch's worktree
        let target_dir = if branch.worktree_path.exists() {
            &branch.worktree_path
        } else {
            workspace_root
        };

        let diag = FastCompilerChecker::check_workspace(target_dir)
            .await
            .unwrap_or_default();
        branch.compiler_clean = diag.is_clean();
        branch.compiler_errors = diag.errors.len();
        branch.compiler_warnings = diag.warnings.len();

        // Compute fitness score: 1.0 (perfect clean) down to 0.0
        let error_penalty = (branch.compiler_errors as f32 * 0.4).min(0.8);
        let warning_penalty = (branch.compiler_warnings as f32 * 0.05).min(0.2);
        let score = (1.0 - error_penalty - warning_penalty).clamp(0.0, 1.0);

        branch.fitness_score = score;
        branch.status = if branch.compiler_clean {
            BranchStatus::Passed
        } else {
            BranchStatus::Failed
        };
        branch.notes = format!(
            "Compiler check: {} errors, {} warnings",
            diag.errors.len(),
            diag.warnings.len()
        );

        let evaluated = branch.clone();
        Self::save_session(workspace_root, &session)?;
        Ok(evaluated)
    }

    /// Selects the highest scoring branch and cleans up temporary speculative branches.
    pub async fn select_best_branch(workspace_root: &Path) -> Result<HypothesisBranch> {
        let mut session = Self::load_session(workspace_root)?;
        if session.branches.is_empty() {
            return Err(ToolError::NotFound {
                name: "No active hypothesis session found".to_string(),
            }
            .into());
        }

        // Sort by fitness score descending
        session.branches.sort_by(|a, b| {
            b.fitness_score
                .partial_cmp(&a.fitness_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let wt_mgr = WorktreeManager::new(workspace_root);
        let winner_id = session.branches[0].id.clone();
        for branch in &mut session.branches {
            if branch.id == winner_id {
                branch.status = BranchStatus::Selected;
            } else {
                branch.status = BranchStatus::Discarded;
                // Clean up discarded worktree
                if workspace_root.join(".git").exists() {
                    let _ = wt_mgr.remove_worktree(&branch.id).await;
                } else if branch.worktree_path.exists() {
                    let _ = fs::remove_dir_all(&branch.worktree_path);
                }
            }
        }

        let winner = session.branches[0].clone();
        Self::save_session(workspace_root, &session)?;
        Ok(winner)
    }

    fn session_file(workspace_root: &Path) -> PathBuf {
        workspace_root
            .join(".minicode")
            .join("cache")
            .join("hypothesis_session.json")
    }

    fn load_session(workspace_root: &Path) -> Result<HypothesisSession> {
        let file = Self::session_file(workspace_root);
        if !file.exists() {
            return Ok(HypothesisSession {
                id: String::new(),
                branches: Vec::new(),
            });
        }
        let raw = fs::read_to_string(&file).map_err(|e| ToolError::FileOp {
            path: file.display().to_string(),
            source: e,
        })?;
        let session = serde_json::from_str(&raw).unwrap_or(HypothesisSession {
            id: String::new(),
            branches: Vec::new(),
        });
        Ok(session)
    }

    fn save_session(workspace_root: &Path, session: &HypothesisSession) -> Result<()> {
        let file = Self::session_file(workspace_root);
        if let Some(parent) = file.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let raw = serde_json::to_string_pretty(session)
            .map_err(|e| ToolError::CommandExec(e.to_string()))?;
        fs::write(&file, raw).map_err(|e| ToolError::FileOp {
            path: file.display().to_string(),
            source: e,
        })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_hypothesis_session_lifecycle() {
        let dir = tempdir().unwrap();
        let ws = dir.path();

        let descs = vec![
            "Hypothesis A: Use mutex lock".to_string(),
            "Hypothesis B: Use lock-free atomics".to_string(),
        ];

        let session = HypothesisEngine::create_branches(ws, &descs).await.unwrap();
        assert_eq!(session.branches.len(), 2);

        let branch1 = HypothesisEngine::evaluate_branch(ws, &session.branches[0].id)
            .await
            .unwrap();
        assert!(branch1.fitness_score >= 0.0);

        let winner = HypothesisEngine::select_best_branch(ws).await.unwrap();
        assert_eq!(winner.status, BranchStatus::Selected);
    }
}

use crate::error::{GitError, Result};
use crate::git::service::GitService;
use std::path::{Path, PathBuf};

/// Manages isolated Git Worktrees for parallel subagent execution without workspace race conditions.
pub struct WorktreeManager {
    workspace_root: PathBuf,
    worktrees_dir: PathBuf,
}

impl WorktreeManager {
    /// Creates a new WorktreeManager for the specified workspace.
    pub fn new(workspace_root: &Path) -> Self {
        let worktrees_dir = workspace_root.join(".minicode").join("worktrees");
        Self {
            workspace_root: workspace_root.to_path_buf(),
            worktrees_dir,
        }
    }

    /// Path to the root directory where all active worktrees reside.
    pub fn worktrees_dir(&self) -> &Path {
        &self.worktrees_dir
    }

    /// Creates an isolated git worktree and dedicated branch for a subagent by delegating
    /// to the canonical [`crate::sandbox::worktree::GitWorktreeManager`].
    pub async fn create_worktree(&self, subagent_id: &str) -> Result<PathBuf> {
        let git = GitService::new(self.workspace_root.clone());
        if !git.is_git_repo().await {
            return Err(GitError::NotARepo {
                path: self.workspace_root.display().to_string(),
            }
            .into());
        }

        let ws_root = self.workspace_root.clone();
        let agent_id = crate::agent::subagent::types::AgentId(subagent_id.to_string());
        let handle = tokio::task::spawn_blocking(move || {
            crate::sandbox::worktree::GitWorktreeManager::create_worktree(&ws_root, &agent_id)
        })
        .await
        .map_err(|e| GitError::CommandFailed {
            cmd: "spawn_blocking(create_worktree)".to_string(),
            code: None,
            stderr: e.to_string(),
        })?
        .map_err(|e| GitError::CommandFailed {
            cmd: "git worktree add".to_string(),
            code: None,
            stderr: e.to_string(),
        })?;

        Ok(handle.worktree_path)
    }

    /// Removes an isolated worktree and deletes its temporary subagent branch via [`crate::sandbox::worktree::GitWorktreeManager`].
    pub async fn remove_worktree(&self, subagent_id: &str) -> Result<()> {
        let ws_root = self.workspace_root.clone();
        let worktrees_dir = self.worktrees_dir.clone();
        let id_owned = subagent_id.to_string();

        tokio::task::spawn_blocking(move || {
            let agent_id = crate::agent::subagent::types::AgentId(id_owned.clone());
            let worktree_path =
                crate::sandbox::worktree::GitWorktreeManager::locate_worktree(&ws_root, &agent_id)
                    .unwrap_or_else(|| worktrees_dir.join(&id_owned));
            let branch_name = crate::sandbox::worktree::GitWorktreeManager::resolve_branch_for(
                &ws_root, &agent_id,
            );
            let handle = crate::sandbox::worktree::WorktreeHandle {
                worktree_path,
                branch_name,
                agent_id,
                repo_root: ws_root,
            };
            crate::sandbox::worktree::GitWorktreeManager::remove_worktree(&handle)
        })
        .await
        .map_err(|e| GitError::CommandFailed {
            cmd: "spawn_blocking(remove_worktree)".to_string(),
            code: None,
            stderr: e.to_string(),
        })?
        .map_err(|e| GitError::CommandFailed {
            cmd: "git worktree remove".to_string(),
            code: None,
            stderr: e.to_string(),
        })?;

        Ok(())
    }

    /// Merges a subagent's branch into the current active branch.
    pub async fn merge_worktree(&self, subagent_id: &str) -> Result<String> {
        let git = GitService::new(self.workspace_root.clone());
        let ws_root = self.workspace_root.clone();
        let id_owned = subagent_id.to_string();
        let branch_name = tokio::task::spawn_blocking(move || {
            let agent_id = crate::agent::subagent::types::AgentId(id_owned);
            if let Some(wt_path) =
                crate::sandbox::worktree::GitWorktreeManager::locate_worktree(&ws_root, &agent_id)
            {
                let _ = crate::sandbox::worktree::GitWorktreeManager::commit_uncommitted_changes(
                    &wt_path, &agent_id,
                );
            }
            crate::sandbox::worktree::GitWorktreeManager::resolve_branch_for(&ws_root, &agent_id)
        })
        .await
        .map_err(|e| GitError::CommandFailed {
            cmd: "spawn_blocking(resolve_branch_for)".to_string(),
            code: None,
            stderr: e.to_string(),
        })?;

        let commit_msg = format!("merge: integrate minipub task {}", subagent_id);
        let res = git
            .run_git(&["merge", "--no-ff", "-m", &commit_msg, &branch_name])
            .await;
        if res.is_ok() {
            crate::context::index::SymbolIndex::invalidate_cache();
            crate::context::graph::CodeGraph::invalidate_cache();
        }
        res
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use tokio::process::Command;

    async fn init_git_repo() -> (tempfile::TempDir, PathBuf) {
        let dir = tempdir().unwrap();
        let path = dir.path().to_path_buf();

        Command::new("git")
            .arg("init")
            .current_dir(&path)
            .output()
            .await
            .unwrap();

        Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(&path)
            .output()
            .await
            .unwrap();

        Command::new("git")
            .args(["config", "user.email", "test@user.ai"])
            .current_dir(&path)
            .output()
            .await
            .unwrap();

        tokio::fs::write(path.join("init.txt"), "base")
            .await
            .unwrap();
        Command::new("git")
            .args(["add", "-A"])
            .current_dir(&path)
            .output()
            .await
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(&path)
            .output()
            .await
            .unwrap();

        (dir, path)
    }

    #[tokio::test]
    async fn test_worktree_lifecycle_create_and_remove() {
        let (_dir, ws_path) = init_git_repo().await;
        let manager = WorktreeManager::new(&ws_path);

        let subagent_id = "agent-test-1";
        let wt_path = manager.create_worktree(subagent_id).await.unwrap();
        assert!(wt_path.exists());
        assert!(wt_path.join("init.txt").exists());

        // Make an isolated edit in worktree
        tokio::fs::write(wt_path.join("agent_file.txt"), "hello from agent")
            .await
            .unwrap();

        // Remove worktree
        manager.remove_worktree(subagent_id).await.unwrap();
        assert!(!wt_path.exists());
    }
}

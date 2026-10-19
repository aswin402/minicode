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

    /// Creates an isolated git worktree and dedicated branch for a subagent.
    pub async fn create_worktree(&self, subagent_id: &str) -> Result<PathBuf> {
        let git = GitService::new(self.workspace_root.clone());
        if !git.is_git_repo().await {
            return Err(GitError::NotARepo {
                path: self.workspace_root.display().to_string(),
            }
            .into());
        }

        tokio::fs::create_dir_all(&self.worktrees_dir).await?;

        let worktree_path = self.worktrees_dir.join(subagent_id);
        let branch_name = format!("subagent/{}", subagent_id);

        // If the worktree already exists, remove it first
        if worktree_path.exists() {
            let _ = self.remove_worktree(subagent_id).await;
        }

        // git worktree add .minicode/worktrees/<id> -b subagent/<id>
        let worktree_path_str = worktree_path.to_string_lossy().to_string();
        git.run_git(&["worktree", "add", &worktree_path_str, "-b", &branch_name])
            .await?;

        // Copy project configuration files into worktree if they exist in root
        for config_file in &[
            ".env",
            "minikit.json",
            "onpkg.json",
            "Cargo.toml",
            "package.json",
        ] {
            let src = self.workspace_root.join(config_file);
            let dst = worktree_path.join(config_file);
            if src.exists() && !dst.exists() {
                let _ = tokio::fs::copy(&src, &dst).await;
            }
        }

        // Provision warm build caches (symlink node_modules, update .git/info/exclude)
        crate::sandbox::worktree::GitWorktreeManager::provision_worktree_environment(
            &self.workspace_root,
            &worktree_path,
        );

        Ok(worktree_path)
    }

    /// Removes an isolated worktree and deletes its temporary subagent branch.
    pub async fn remove_worktree(&self, subagent_id: &str) -> Result<()> {
        let git = GitService::new(self.workspace_root.clone());
        let agent_id = crate::agent::subagent::types::AgentId(subagent_id.to_string());
        let worktree_path = crate::sandbox::worktree::GitWorktreeManager::locate_worktree(
            &self.workspace_root,
            &agent_id,
        )
        .unwrap_or_else(|| self.worktrees_dir.join(subagent_id));

        if worktree_path.exists() {
            let worktree_path_str = worktree_path.to_string_lossy().to_string();
            let _ = git
                .run_git(&["worktree", "remove", "--force", &worktree_path_str])
                .await;
            let _ = tokio::fs::remove_dir_all(&worktree_path).await;
        }

        // Delete any temporary branch variant
        for branch_name in [
            format!("subagent/{}", subagent_id),
            format!("minicode/subagent/{}", subagent_id),
            format!("minicode/minipub/{}", subagent_id),
        ] {
            let _ = git.run_git(&["branch", "-D", &branch_name]).await;
        }
        let _ = git.run_git(&["worktree", "prune"]).await;

        Ok(())
    }

    /// Merges a subagent's branch into the current active branch.
    pub async fn merge_worktree(&self, subagent_id: &str) -> Result<String> {
        let git = GitService::new(self.workspace_root.clone());
        let agent_id = crate::agent::subagent::types::AgentId(subagent_id.to_string());
        if let Some(wt_path) = crate::sandbox::worktree::GitWorktreeManager::locate_worktree(
            &self.workspace_root,
            &agent_id,
        ) {
            let _ = crate::sandbox::worktree::GitWorktreeManager::commit_uncommitted_changes(
                &wt_path, &agent_id,
            );
        }
        let branch_name = crate::sandbox::worktree::GitWorktreeManager::resolve_branch_for(
            &self.workspace_root,
            &agent_id,
        );
        let commit_msg = format!("merge: integrate minipub task {}", subagent_id);

        let res = git
            .run_git(&["merge", "--no-ff", "-m", &commit_msg, &branch_name])
            .await;
        if res.is_ok() {
            crate::context::index::SymbolIndex::invalidate_cache();
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

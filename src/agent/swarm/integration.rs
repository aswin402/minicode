//! Swarm integration branch.
//!
//! Every swarm run owns one integration branch (checked out in a dedicated worktree).
//! Each completed task branch is merged into it immediately, and every newly scheduled
//! worker branches from it, so dependent waves see the code produced by upstream waves.
//! The user's checked-out branch is only touched by the optional final merge.

use std::path::{Path, PathBuf};
use std::process::Command;
use tracing::{info, warn};

use crate::agent::subagent::types::AgentId;
use crate::agent::swarm::models::SwarmError;
use crate::sandbox::arbitration::{ArbitrationError, MergeArbitrator};
use crate::sandbox::worktree::GitWorktreeManager;

/// Handle to a swarm run's integration branch and its worktree.
#[derive(Debug, Clone)]
pub struct IntegrationBranch {
    pub branch: String,
    pub worktree_path: PathBuf,
    pub repo_root: PathBuf,
}

impl IntegrationBranch {
    /// Creates the integration branch at the repository's current `HEAD`.
    ///
    /// Fails (instead of silently degrading to a shared workspace) when the workspace is not a
    /// git repository with at least one commit, because isolated parallel execution is impossible.
    pub fn create(repo_root: &Path, swarm_id: &str) -> Result<Self, SwarmError> {
        let head_ok = Command::new("git")
            .args(["rev-parse", "--verify", "HEAD"])
            .current_dir(repo_root)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if !head_ok {
            return Err(SwarmError::WorkspaceNotReady(format!(
                "`{}` is not a git repository with at least one commit; swarm workers require isolated git worktrees",
                repo_root.display()
            )));
        }

        let agent_id = AgentId(format!("swarm-{}-integration", swarm_id));
        let handle = GitWorktreeManager::create_worktree_from(repo_root, &agent_id, "HEAD")
            .map_err(|e| {
                SwarmError::WorkspaceNotReady(format!(
                    "failed to create integration worktree: {}",
                    e
                ))
            })?;

        info!(branch = %handle.branch_name, "Created swarm integration branch");
        Ok(Self {
            branch: handle.branch_name,
            worktree_path: handle.worktree_path,
            repo_root: repo_root.to_path_buf(),
        })
    }

    /// Merges a completed task branch into the integration branch (no-ff commit).
    /// On conflict the merge is aborted and the conflicting files are returned as the error.
    pub fn merge_task_branch(&self, task_branch: &str, message: &str) -> Result<(), String> {
        match MergeArbitrator::apply_merge(&self.worktree_path, task_branch, true, Some(message)) {
            Ok(_) => Ok(()),
            Err(ArbitrationError::MergeConflict(files)) => Err(format!(
                "merge conflict with upstream tasks in: {}",
                files.join(", ")
            )),
            Err(e) => Err(format!("integration merge failed: {}", e)),
        }
    }

    /// Returns true if the integration branch has commits that are not in the repository `HEAD`.
    pub fn has_changes(&self) -> bool {
        GitWorktreeManager::has_commits_ahead_of_head(&self.repo_root, &self.branch)
    }

    /// Removes the integration worktree directory (the branch itself is kept unless
    /// `delete_branch` is set, so unmerged results stay inspectable).
    pub fn cleanup(&self, delete_branch: bool) {
        remove_worktree_path(&self.repo_root, &self.worktree_path);
        if delete_branch {
            delete_branch_ref(&self.repo_root, &self.branch);
        }
    }
}

/// Force-removes a git worktree directory registered under the repository.
pub fn remove_worktree_path(repo_root: &Path, worktree_path: &Path) {
    if !worktree_path.exists() {
        return;
    }
    let out = Command::new("git")
        .args(["worktree", "remove", "--force"])
        .arg(worktree_path)
        .current_dir(repo_root)
        .output();
    if !matches!(out, Ok(ref o) if o.status.success()) {
        warn!(path = %worktree_path.display(), "git worktree remove failed; deleting directory");
        let _ = std::fs::remove_dir_all(worktree_path);
        let _ = Command::new("git")
            .args(["worktree", "prune"])
            .current_dir(repo_root)
            .output();
    }
}

/// Force-deletes a local branch (ignored if it does not exist).
pub fn delete_branch_ref(repo_root: &Path, branch: &str) {
    let _ = Command::new("git")
        .args(["branch", "-D", branch])
        .current_dir(repo_root)
        .output();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git(dir: &Path, args: &[&str]) {
        let out = Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .expect("git runs");
        assert!(
            out.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
    }

    fn init_repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        git(root, &["init", "-q"]);
        git(root, &["config", "user.email", "t@example.com"]);
        git(root, &["config", "user.name", "tester"]);
        std::fs::write(root.join("base.txt"), "base\n").expect("write");
        git(root, &["add", "."]);
        git(root, &["commit", "-q", "-m", "init"]);
        dir
    }

    fn commit_in_worktree(worktree: &Path, file: &str, content: &str) {
        std::fs::write(worktree.join(file), content).expect("write");
        git(worktree, &["add", "."]);
        git(worktree, &["commit", "-q", "-m", file]);
    }

    #[test]
    fn test_create_fails_without_commits() {
        let dir = tempfile::tempdir().expect("tempdir");
        let res = IntegrationBranch::create(dir.path(), "nocommit");
        assert!(matches!(res, Err(SwarmError::WorkspaceNotReady(_))));
    }

    #[test]
    fn test_dependent_worker_sees_upstream_commit() {
        let repo = init_repo();
        let root = repo.path();
        let integ = IntegrationBranch::create(root, "s1").expect("integration");

        // Wave 1 task branches from integration, commits a.txt, gets merged.
        let t1 = GitWorktreeManager::create_worktree_from(
            root,
            &AgentId("t1-test".to_string()),
            &integ.branch,
        )
        .expect("t1 worktree");
        commit_in_worktree(&t1.worktree_path, "a.txt", "from t1\n");
        integ
            .merge_task_branch(&t1.branch_name, "merge t1")
            .expect("merge t1");

        // Wave 2 task branches from integration and must see a.txt.
        let t2 = GitWorktreeManager::create_worktree_from(
            root,
            &AgentId("t2-test".to_string()),
            &integ.branch,
        )
        .expect("t2 worktree");
        assert!(t2.worktree_path.join("a.txt").is_file());
        assert!(integ.has_changes());
        // The user's checked-out workspace is untouched until a final merge.
        assert!(!root.join("a.txt").exists());

        integ.cleanup(true);
        assert!(!integ.worktree_path.exists());
    }

    #[test]
    fn test_conflicting_task_branch_is_rejected() {
        let repo = init_repo();
        let root = repo.path();
        let integ = IntegrationBranch::create(root, "s2").expect("integration");

        let t1 = GitWorktreeManager::create_worktree_from(
            root,
            &AgentId("c1-test".to_string()),
            &integ.branch,
        )
        .expect("t1");
        let t2 = GitWorktreeManager::create_worktree_from(
            root,
            &AgentId("c2-test".to_string()),
            &integ.branch,
        )
        .expect("t2");
        commit_in_worktree(&t1.worktree_path, "base.txt", "t1 edit\n");
        commit_in_worktree(&t2.worktree_path, "base.txt", "t2 edit\n");

        integ
            .merge_task_branch(&t1.branch_name, "m1")
            .expect("t1 merges");
        let err = integ
            .merge_task_branch(&t2.branch_name, "m2")
            .expect_err("t2 conflicts");
        assert!(err.contains("base.txt"), "unexpected error: {}", err);
        integ.cleanup(true);
    }
}

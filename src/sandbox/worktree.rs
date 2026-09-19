//! Ephemeral Git Worktree Sandboxing Engine for subagent isolation.
//!
//! Provisions, manages, inspects, and cleans up isolated Git worktrees
//! for mutating subagents (e.g. `coder`, `tester`).

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::agent::subagent::types::AgentId;

/// Represents an active Git worktree provisioned for an isolated subagent worker.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeHandle {
    pub worktree_path: PathBuf,
    pub branch_name: String,
    pub agent_id: AgentId,
    pub repo_root: PathBuf,
}

/// Manager for provisioning, inspecting, and tearing down ephemeral Git worktrees.
#[allow(dead_code)]
pub struct GitWorktreeManager;

#[allow(dead_code)]
impl GitWorktreeManager {
    /// Creates an isolated Git worktree and dedicated branch for a subagent.
    ///
    /// 1. Worktree path: `repo_root/.minicode/worktrees/<agent_id>`
    /// 2. Branch name: `minicode-task-<agent_id>`
    /// 3. Ensures `.minicode/worktrees/` directory exists.
    /// 4. If `worktree_path` exists, cleans up stale worktree and branch first.
    /// 5. Runs `git worktree add -b <branch_name> <worktree_path> HEAD`.
    pub fn create_worktree(
        repo_root: &Path,
        agent_id: &AgentId,
    ) -> std::io::Result<WorktreeHandle> {
        let worktrees_dir = repo_root.join(".minicode").join("worktrees");
        let worktree_path = worktrees_dir.join(&agent_id.0);
        let branch_name = format!("minicode-task-{}", &agent_id.0);

        std::fs::create_dir_all(&worktrees_dir)?;

        let handle = WorktreeHandle {
            worktree_path: worktree_path.clone(),
            branch_name: branch_name.clone(),
            agent_id: agent_id.clone(),
            repo_root: repo_root.to_path_buf(),
        };

        if worktree_path.exists() {
            let _ = Self::remove_worktree(&handle);
        } else {
            let _ = Command::new("git")
                .args(["branch", "-D", &branch_name])
                .current_dir(repo_root)
                .output();
        }

        let output = Command::new("git")
            .args(["worktree", "add", "-b", &branch_name])
            .arg(&worktree_path)
            .arg("HEAD")
            .current_dir(repo_root)
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(std::io::Error::other(format!(
                "git worktree add failed: {}",
                stderr
            )));
        }

        Ok(handle)
    }

    /// Captures the working tree diff relative to HEAD in the worktree.
    ///
    /// Runs `git add -N .` so untracked files are visible to diff, then runs
    /// `git diff HEAD`. If that fails, falls back to `git diff`. Returns the trimmed diff.
    pub fn capture_diff(handle: &WorktreeHandle) -> std::io::Result<String> {
        let _ = Command::new("git")
            .args(["add", "-N", "."])
            .current_dir(&handle.worktree_path)
            .output();

        let output = Command::new("git")
            .args(["diff", "HEAD"])
            .current_dir(&handle.worktree_path)
            .output();

        let diff_str = match output {
            Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout).to_string(),
            _ => {
                let fallback = Command::new("git")
                    .arg("diff")
                    .current_dir(&handle.worktree_path)
                    .output()?;
                String::from_utf8_lossy(&fallback.stdout).to_string()
            }
        };

        Ok(diff_str.trim().to_string())
    }

    /// Removes an isolated worktree and deletes its dedicated branch.
    ///
    /// 1. Runs `git worktree remove --force <worktree_path>`.
    /// 2. Runs `git branch -D <branch_name>`.
    /// 3. Removes `worktree_path` from filesystem if still present.
    /// 4. Runs `git worktree prune`.
    pub fn remove_worktree(handle: &WorktreeHandle) -> std::io::Result<()> {
        let _ = Command::new("git")
            .args(["worktree", "remove", "--force"])
            .arg(&handle.worktree_path)
            .current_dir(&handle.repo_root)
            .output();

        let _ = Command::new("git")
            .args(["branch", "-D", &handle.branch_name])
            .current_dir(&handle.repo_root)
            .output();

        if handle.worktree_path.exists() {
            if let Err(e) = std::fs::remove_dir_all(&handle.worktree_path) {
                if e.kind() != std::io::ErrorKind::NotFound {
                    return Err(e);
                }
            }
        }

        let _ = Command::new("git")
            .args(["worktree", "prune"])
            .current_dir(&handle.repo_root)
            .output();

        Ok(())
    }

    /// Cleans up any stale worktrees under `.minicode/worktrees/`.
    ///
    /// 1. Checks `.minicode/worktrees/`. Returns `Ok(0)` if it does not exist.
    /// 2. Runs `git worktree prune` in `repo_root`.
    /// 3. Removes any remaining directories inside `.minicode/worktrees/`.
    /// 4. Returns count of directories cleaned.
    pub fn cleanup_stale_worktrees(repo_root: &Path) -> std::io::Result<usize> {
        let worktrees_dir = repo_root.join(".minicode").join("worktrees");
        if !worktrees_dir.exists() {
            return Ok(0);
        }

        let _ = Command::new("git")
            .args(["worktree", "prune"])
            .current_dir(repo_root)
            .output();

        let mut count = 0;
        let entries = std::fs::read_dir(&worktrees_dir)?;
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                if let Err(e) = std::fs::remove_dir_all(&path) {
                    if e.kind() != std::io::ErrorKind::NotFound {
                        return Err(e);
                    }
                } else {
                    count += 1;
                }
            }
        }

        let _ = Command::new("git")
            .args(["worktree", "prune"])
            .current_dir(repo_root)
            .output();

        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use tempfile::tempdir;

    fn setup_git_repo() -> tempfile::TempDir {
        let dir = tempdir().expect("failed to create temp dir");
        let path = dir.path();

        let run = |args: &[&str]| {
            let output = Command::new("git")
                .args(args)
                .current_dir(path)
                .output()
                .expect("failed to execute git command");
            assert!(
                output.status.success(),
                "git {:?} failed: {}",
                args,
                String::from_utf8_lossy(&output.stderr)
            );
        };

        run(&["init"]);
        run(&["config", "user.name", "Test User"]);
        run(&["config", "user.email", "test@example.com"]);
        run(&["config", "commit.gpgsign", "false"]);

        std::fs::write(path.join("file.txt"), "hello world\n")
            .expect("failed to write initial file");
        run(&["add", "file.txt"]);
        run(&["commit", "-m", "init"]);

        dir
    }

    #[test]
    fn test_worktree_lifecycle_in_git_repo() {
        let temp_dir = setup_git_repo();
        let repo_root = temp_dir.path();

        let agent_id = AgentId("coder-test".to_string());
        let handle = GitWorktreeManager::create_worktree(repo_root, &agent_id)
            .expect("create_worktree failed");

        assert!(handle.worktree_path.exists());
        assert_eq!(handle.branch_name, "minicode-task-coder-test");

        // Verifies worktree directory exists and branch is created
        let branch_out = Command::new("git")
            .args(["branch", "--list", &handle.branch_name])
            .current_dir(repo_root)
            .output()
            .expect("failed to list branches");
        let stdout = String::from_utf8_lossy(&branch_out.stdout);
        assert!(stdout.contains(&handle.branch_name));

        // Modifies file in worktree
        std::fs::write(handle.worktree_path.join("file.txt"), "hello worktree\n")
            .expect("failed to modify file");

        // Calls capture_diff and asserts diff contains modification
        let diff = GitWorktreeManager::capture_diff(&handle).expect("diff capture failed");
        assert!(diff.contains("-hello world"));
        assert!(diff.contains("+hello worktree"));

        // Calls remove_worktree
        GitWorktreeManager::remove_worktree(&handle).expect("remove worktree failed");

        // Asserts worktree directory no longer exists
        assert!(!handle.worktree_path.exists());

        // Branch should also be deleted
        let branch_after = Command::new("git")
            .args(["branch", "--list", &handle.branch_name])
            .current_dir(repo_root)
            .output()
            .expect("failed to list branches");
        let stdout_after = String::from_utf8_lossy(&branch_after.stdout);
        assert!(!stdout_after.contains(&handle.branch_name));
    }

    #[test]
    fn test_non_git_repo_worktree_error() {
        let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
        let non_git = temp_dir.path();
        let agent_id = AgentId("tester-non-git".to_string());
        let res = GitWorktreeManager::create_worktree(non_git, &agent_id);
        assert!(res.is_err());
        let err_msg = res.unwrap_err().to_string();
        assert!(err_msg.contains("git worktree add failed"));
    }

    #[test]
    fn test_cleanup_stale_worktrees() {
        let temp_dir = setup_git_repo();
        let repo_root = temp_dir.path();

        // With no .minicode/worktrees directory
        let cleaned =
            GitWorktreeManager::cleanup_stale_worktrees(repo_root).expect("cleanup failed");
        assert_eq!(cleaned, 0);

        // Create orphaned worktree directories
        let stale1 = repo_root
            .join(".minicode")
            .join("worktrees")
            .join("stale-1");
        let stale2 = repo_root
            .join(".minicode")
            .join("worktrees")
            .join("stale-2");
        std::fs::create_dir_all(&stale1).expect("failed to create stale1");
        std::fs::create_dir_all(&stale2).expect("failed to create stale2");
        assert!(stale1.exists());
        assert!(stale2.exists());

        let cleaned =
            GitWorktreeManager::cleanup_stale_worktrees(repo_root).expect("cleanup failed");
        assert_eq!(cleaned, 2);
        assert!(!stale1.exists());
        assert!(!stale2.exists());
    }

    #[test]
    fn test_create_worktree_recreates_if_already_exists() {
        let temp_dir = setup_git_repo();
        let repo_root = temp_dir.path();

        let agent_id = AgentId("coder-recreate".to_string());
        let handle1 = GitWorktreeManager::create_worktree(repo_root, &agent_id)
            .expect("first create_worktree failed");
        assert!(handle1.worktree_path.exists());

        // Calling create_worktree again with same agent_id recreates cleanly
        let handle2 = GitWorktreeManager::create_worktree(repo_root, &agent_id)
            .expect("second create_worktree failed");
        assert!(handle2.worktree_path.exists());
        assert_eq!(handle1.worktree_path, handle2.worktree_path);

        GitWorktreeManager::remove_worktree(&handle2).expect("remove_worktree failed");
        assert!(!handle2.worktree_path.exists());
    }

    #[test]
    fn test_capture_diff_untracked_file() {
        let temp_dir = setup_git_repo();
        let repo_root = temp_dir.path();

        let agent_id = AgentId("coder-untracked".to_string());
        let handle = GitWorktreeManager::create_worktree(repo_root, &agent_id)
            .expect("create_worktree failed");

        // Add an untracked file in the worktree
        std::fs::write(
            handle.worktree_path.join("untracked.txt"),
            "untracked file content\n",
        )
        .expect("failed to write untracked file");

        let diff = GitWorktreeManager::capture_diff(&handle).expect("capture_diff failed");
        assert!(diff.contains("untracked.txt"));
        assert!(diff.contains("+untracked file content"));

        GitWorktreeManager::remove_worktree(&handle).expect("remove_worktree failed");
        assert!(!handle.worktree_path.exists());
    }
}

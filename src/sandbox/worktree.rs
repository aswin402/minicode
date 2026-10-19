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
    /// Derives the dedicated branch name for a subagent.
    pub fn branch_name_for(agent_id: &AgentId) -> String {
        format!("minicode/subagent/{}", agent_id.0)
    }

    /// Locates an existing worktree directory for a subagent under `.minicode/worktrees/`.
    ///
    /// Checks for `subagent-<agent_id>`, `minipub-<agent_id>`, and `<agent_id>`.
    pub fn locate_worktree(repo_root: &Path, agent_id: &AgentId) -> Option<PathBuf> {
        let worktrees_dir = repo_root
            .join(crate::constants::WORKSPACE_DIR_NAME)
            .join(crate::constants::WORKTREES_DIR_NAME);
        [
            worktrees_dir.join(format!("subagent-{}", agent_id.0)),
            worktrees_dir.join(format!("minipub-{}", agent_id.0)),
            worktrees_dir.join(&agent_id.0),
        ]
        .into_iter()
        .find(|candidate| candidate.exists())
    }

    /// Resolves the Git branch associated with a subagent's worktree or branch history.
    ///
    /// Inspects `HEAD` inside the worktree if present; otherwise checks local refs for
    /// `minicode/subagent/<id>`, `minicode/minipub/<id>`, and `subagent/<id>`;
    /// falls back to `branch_name_for(agent_id)`.
    pub fn resolve_branch_for(repo_root: &Path, agent_id: &AgentId) -> String {
        if let Some(wt) = Self::locate_worktree(repo_root, agent_id) {
            if let Ok(output) = Command::new("git")
                .args(["rev-parse", "--abbrev-ref", "HEAD"])
                .current_dir(&wt)
                .output()
            {
                if output.status.success() {
                    let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    if !branch.is_empty() && branch != "HEAD" {
                        return branch;
                    }
                }
            }
        }
        for candidate in [
            format!("minicode/subagent/{}", agent_id.0),
            format!("minicode/minipub/{}", agent_id.0),
            format!("subagent/{}", agent_id.0),
        ] {
            if let Ok(out) = Command::new("git")
                .args([
                    "show-ref",
                    "--verify",
                    "--quiet",
                    &format!("refs/heads/{}", candidate),
                ])
                .current_dir(repo_root)
                .output()
            {
                if out.status.success() {
                    return candidate;
                }
            }
        }
        Self::branch_name_for(agent_id)
    }

    /// Provisions warm build caches and local environment files inside a newly created worktree
    /// without polluting git status or diffs.
    pub fn provision_worktree_environment(repo_root: &Path, worktree_path: &Path) {
        // Ensure .git/info/exclude ignores symlinked node_modules and copied .env
        let exclude_path = repo_root.join(".git").join("info").join("exclude");
        if let Some(parent) = exclude_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let existing_exclude = std::fs::read_to_string(&exclude_path).unwrap_or_default();
        let mut updated_exclude = existing_exclude.clone();
        for pattern in ["node_modules", "node_modules/", ".env"] {
            if !updated_exclude.lines().any(|l| l.trim() == pattern) {
                if !updated_exclude.ends_with('\n') && !updated_exclude.is_empty() {
                    updated_exclude.push('\n');
                }
                updated_exclude.push_str(pattern);
                updated_exclude.push('\n');
            }
        }
        if updated_exclude != existing_exclude {
            let _ = std::fs::write(&exclude_path, updated_exclude);
        }

        // Copy .env if present in repo_root
        let env_src = repo_root.join(".env");
        let env_dst = worktree_path.join(".env");
        if env_src.is_file() && !env_dst.exists() {
            let _ = std::fs::copy(&env_src, &env_dst);
        }

        // Symlink node_modules from repo_root so JS/TS/Bun/Vite tasks don't reinstall dependencies
        #[cfg(unix)]
        {
            let nm_src = repo_root.join("node_modules");
            let nm_dst = worktree_path.join("node_modules");
            if nm_src.is_dir() && !nm_dst.exists() {
                let _ = std::os::unix::fs::symlink(&nm_src, &nm_dst);
            }
        }
    }

    /// Creates an isolated Git worktree and dedicated branch for a subagent.
    ///
    /// 1. Worktree path: `repo_root/.minicode/worktrees/subagent-<agent_id>`
    /// 2. Branch name: `minicode/subagent/<agent_id>`
    /// 3. Ensures `.minicode/worktrees/` directory exists.
    /// 4. If worktree or branch exists, cleans up stale worktree and branch first.
    /// 5. Runs `git worktree add -b <branch_name> <worktree_path> HEAD`.
    /// 6. Provisions warm build caches (`node_modules` symlink, `.env`).
    pub fn create_worktree(
        repo_root: &Path,
        agent_id: &AgentId,
    ) -> std::io::Result<WorktreeHandle> {
        let worktrees_dir = repo_root
            .join(crate::constants::WORKSPACE_DIR_NAME)
            .join(crate::constants::WORKTREES_DIR_NAME);
        let worktree_path = worktrees_dir.join(format!("subagent-{}", &agent_id.0));
        let branch_name = Self::branch_name_for(agent_id);

        std::fs::create_dir_all(&worktrees_dir)?;

        let handle = WorktreeHandle {
            worktree_path: worktree_path.clone(),
            branch_name: branch_name.clone(),
            agent_id: agent_id.clone(),
            repo_root: repo_root.to_path_buf(),
        };

        if let Some(existing_wt) = Self::locate_worktree(repo_root, agent_id) {
            let existing_branch = Self::resolve_branch_for(repo_root, agent_id);
            let existing_handle = WorktreeHandle {
                worktree_path: existing_wt,
                branch_name: existing_branch,
                agent_id: agent_id.clone(),
                repo_root: repo_root.to_path_buf(),
            };
            let _ = Self::remove_worktree(&existing_handle);
        }
        if worktree_path.exists() {
            let _ = Self::remove_worktree(&handle);
        }
        for candidate_branch in [
            branch_name.clone(),
            format!("minicode/minipub/{}", agent_id.0),
            format!("subagent/{}", agent_id.0),
        ] {
            let _ = Command::new("git")
                .args(["branch", "-D", &candidate_branch])
                .current_dir(repo_root)
                .output();
        }
        let _ = Command::new("git")
            .args(["worktree", "prune"])
            .current_dir(repo_root)
            .output();

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

        Self::provision_worktree_environment(repo_root, &worktree_path);

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

    /// Stages and commits any uncommitted working-tree changes inside an isolated worktree
    /// so that `git merge-tree` and `git merge` on the worktree branch include all `write_file`/`patch_file` edits.
    pub fn commit_uncommitted_changes(
        worktree_path: &Path,
        agent_id: &AgentId,
    ) -> std::io::Result<bool> {
        if !worktree_path.exists() {
            return Ok(false);
        }

        let status_out = Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(worktree_path)
            .output()?;
        if !status_out.status.success()
            || String::from_utf8_lossy(&status_out.stdout)
                .trim()
                .is_empty()
        {
            return Ok(false);
        }

        let _ = Command::new("git")
            .args(["add", "-A"])
            .current_dir(worktree_path)
            .output();
        // Never stage symlinked node_modules or local .env
        let _ = Command::new("git")
            .args(["reset", "-q", "HEAD", "--", "node_modules", ".env"])
            .current_dir(worktree_path)
            .output();

        let commit_msg = format!(
            "chore(minipub-{}): auto-commit worktree changes",
            agent_id.0
        );
        let commit_out = Command::new("git")
            .args(["commit", "-m", &commit_msg, "--no-verify"])
            .current_dir(worktree_path)
            .output()?;

        Ok(commit_out.status.success())
    }

    /// Commits any pending worktree edits, checks mergeability via `MergeArbitrator`,
    /// merges cleanly into `repo_root`, and removes the temporary worktree on success.
    pub fn auto_commit_and_merge(
        repo_root: &Path,
        agent_id: &AgentId,
    ) -> std::result::Result<Option<String>, String> {
        let Some(worktree_path) = Self::locate_worktree(repo_root, agent_id) else {
            return Ok(None);
        };
        let branch_name = Self::resolve_branch_for(repo_root, agent_id);
        let _ = Self::commit_uncommitted_changes(&worktree_path, agent_id);

        let mergeability = crate::sandbox::arbitration::MergeArbitrator::check_mergeability(
            repo_root,
            &branch_name,
        )
        .map_err(|e| e.to_string())?;
        if !mergeability.can_merge_cleanly {
            return Err(format!(
                "Merge conflict in files {:?} (worktree retained at {})",
                mergeability.conflicted_files,
                worktree_path.display()
            ));
        }

        let commit_msg = format!(
            "merge(minipub-{}): integrate isolated worker changes",
            agent_id.0
        );
        let report = crate::sandbox::arbitration::MergeArbitrator::apply_merge(
            repo_root,
            &branch_name,
            true,
            Some(&commit_msg),
        )
        .map_err(|e| e.to_string())?;

        let handle = WorktreeHandle {
            worktree_path,
            branch_name,
            agent_id: agent_id.clone(),
            repo_root: repo_root.to_path_buf(),
        };
        let _ = Self::remove_worktree(&handle);
        crate::context::index::SymbolIndex::invalidate_cache();

        Ok(report.commit_hash)
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

        for branch in [
            handle.branch_name.as_str(),
            &format!("minicode/subagent/{}", handle.agent_id.0),
            &format!("minicode/minipub/{}", handle.agent_id.0),
            &format!("subagent/{}", handle.agent_id.0),
        ] {
            let _ = Command::new("git")
                .args(["branch", "-D", branch])
                .current_dir(&handle.repo_root)
                .output();
        }

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

    /// Cleans up any stale worktrees under `.minicode/worktrees/` and prunes orphaned subagent branches.
    pub fn cleanup_stale_worktrees(repo_root: &Path) -> std::io::Result<usize> {
        let worktrees_dir = repo_root
            .join(crate::constants::WORKSPACE_DIR_NAME)
            .join(crate::constants::WORKTREES_DIR_NAME);
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
                let _ = Command::new("git")
                    .args(["worktree", "remove", "--force"])
                    .arg(&path)
                    .current_dir(repo_root)
                    .output();
                if path.exists() {
                    if let Err(e) = std::fs::remove_dir_all(&path) {
                        if e.kind() != std::io::ErrorKind::NotFound {
                            return Err(e);
                        }
                    }
                }
                count += 1;
            }
        }

        let _ = Command::new("git")
            .args(["worktree", "prune"])
            .current_dir(repo_root)
            .output();

        // Also delete any orphaned local minicode/subagent/*, minicode/minipub/*, or subagent/* branches
        if let Ok(out) = Command::new("git")
            .args([
                "for-each-ref",
                "--format=%(refname:short)",
                "refs/heads/minicode/subagent/",
                "refs/heads/minicode/minipub/",
                "refs/heads/subagent/",
            ])
            .current_dir(repo_root)
            .output()
        {
            if out.status.success() {
                for line in String::from_utf8_lossy(&out.stdout).lines() {
                    let branch = line.trim();
                    if !branch.is_empty() {
                        let _ = Command::new("git")
                            .args(["branch", "-D", branch])
                            .current_dir(repo_root)
                            .output();
                    }
                }
            }
        }

        Ok(count)
    }
}

/// Cooperative file-boundary lease manager stored in `.minicode/minipub/leases.json`.
/// Allows parallel `minipub` workers with disjoint file boundaries to operate safely
/// in the shared workspace without conflicting edits.
pub struct FileLeaseManager;

impl FileLeaseManager {
    fn leases_file_path(repo_root: &Path) -> PathBuf {
        repo_root
            .join(crate::constants::WORKSPACE_DIR_NAME)
            .join(crate::constants::MINIPUB_DIR_NAME)
            .join(crate::constants::MINIPUB_LEASES_FILE)
    }

    /// Loads current active file-boundary leases (`agent_id -> Vec<boundary_prefix>`).
    pub fn load_leases(repo_root: &Path) -> std::collections::HashMap<String, Vec<String>> {
        let path = Self::leases_file_path(repo_root);
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    fn save_leases(repo_root: &Path, leases: &std::collections::HashMap<String, Vec<String>>) {
        let path = Self::leases_file_path(repo_root);
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(leases) {
            let _ = std::fs::write(&path, json);
        }
    }

    /// Acquires cooperative file-boundary leases for `agent_id`.
    /// Returns `Err` if another active `minipub` holds an overlapping path boundary.
    pub fn acquire_leases(
        repo_root: &Path,
        agent_id: &str,
        boundaries: &[String],
    ) -> std::result::Result<(), String> {
        if boundaries.is_empty() {
            return Ok(());
        }
        let mut leases = Self::load_leases(repo_root);
        let normalized: Vec<String> = boundaries
            .iter()
            .map(|b| {
                b.trim()
                    .trim_start_matches("./")
                    .trim_end_matches('/')
                    .to_string()
            })
            .filter(|b| !b.is_empty())
            .collect();

        for (other_id, other_bounds) in &leases {
            if other_id == agent_id {
                continue;
            }
            for my_b in &normalized {
                for their_b in other_bounds {
                    if Self::boundaries_overlap(my_b, their_b) {
                        return Err(format!(
                            "File boundary '{}' overlaps with active minipub '{}' boundary '{}'",
                            my_b, other_id, their_b
                        ));
                    }
                }
            }
        }

        leases.insert(agent_id.to_string(), normalized);
        Self::save_leases(repo_root, &leases);
        Ok(())
    }

    /// Releases any cooperative file-boundary leases held by `agent_id`.
    pub fn release_leases(repo_root: &Path, agent_id: &str) {
        let mut leases = Self::load_leases(repo_root);
        if leases.remove(agent_id).is_some() {
            Self::save_leases(repo_root, &leases);
        }
    }

    /// Clears all cooperative file-boundary leases for the workspace (e.g., on fresh pool init or `kill_all`).
    pub fn clear_all_leases(repo_root: &Path) {
        let path = Self::leases_file_path(repo_root);
        if path.exists() {
            let _ = std::fs::remove_file(&path);
        }
    }

    /// Checks whether a target file path is permitted by the worker's assigned `file_boundaries`.
    /// If `boundaries` is empty, all workspace paths are permitted.
    pub fn verify_write_allowed(
        boundaries: &[String],
        workspace_root: &Path,
        target_path_str: &str,
    ) -> std::result::Result<(), String> {
        if boundaries.is_empty() {
            return Ok(());
        }
        let target_path = Path::new(target_path_str);
        let rel = if target_path.is_absolute() {
            target_path
                .strip_prefix(workspace_root)
                .unwrap_or(target_path)
                .to_string_lossy()
                .to_string()
        } else {
            target_path_str.trim().trim_start_matches("./").to_string()
        };
        let rel_norm = rel.trim_start_matches("./").trim_end_matches('/');

        let allowed = boundaries.iter().any(|b| {
            let b_norm = b.trim().trim_start_matches("./").trim_end_matches('/');
            b_norm.is_empty() || rel_norm == b_norm || rel_norm.starts_with(&format!("{}/", b_norm))
        });

        if allowed {
            Ok(())
        } else {
            Err(format!(
                "Write to '{}' blocked: outside assigned minipub file_boundaries {:?}",
                rel_norm, boundaries
            ))
        }
    }

    fn boundaries_overlap(a: &str, b: &str) -> bool {
        a == b || a.starts_with(&format!("{}/", b)) || b.starts_with(&format!("{}/", a))
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
    fn test_branch_name_and_locate_worktree() {
        let agent_id = AgentId("coder-test-99".to_string());
        let branch = GitWorktreeManager::branch_name_for(&agent_id);
        assert_eq!(branch, "minicode/subagent/coder-test-99");

        let repo_dir = tempfile::tempdir().unwrap();
        let expected_dir = repo_dir
            .path()
            .join(".minicode")
            .join("worktrees")
            .join("subagent-coder-test-99");
        std::fs::create_dir_all(&expected_dir).unwrap();

        let located = GitWorktreeManager::locate_worktree(repo_dir.path(), &agent_id);
        assert_eq!(located, Some(expected_dir));

        let legacy_agent_id = AgentId("coder-legacy".to_string());
        let legacy_dir = repo_dir
            .path()
            .join(".minicode")
            .join("worktrees")
            .join("coder-legacy");
        std::fs::create_dir_all(&legacy_dir).unwrap();
        let located_legacy = GitWorktreeManager::locate_worktree(repo_dir.path(), &legacy_agent_id);
        assert_eq!(located_legacy, Some(legacy_dir));
    }

    #[test]
    fn test_worktree_lifecycle_in_git_repo() {
        let temp_dir = setup_git_repo();
        let repo_root = temp_dir.path();

        let agent_id = AgentId("coder-test".to_string());
        let handle = GitWorktreeManager::create_worktree(repo_root, &agent_id)
            .expect("create_worktree failed");

        assert!(handle.worktree_path.exists());
        assert_eq!(handle.branch_name, "minicode/subagent/coder-test");

        // Verifies resolve_branch_for matches branch
        let resolved = GitWorktreeManager::resolve_branch_for(repo_root, &agent_id);
        assert_eq!(resolved, "minicode/subagent/coder-test");

        // Verifies locate_worktree returns the handle's worktree path
        let located = GitWorktreeManager::locate_worktree(repo_root, &agent_id);
        assert_eq!(located, Some(handle.worktree_path.clone()));

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

    #[test]
    fn test_auto_commit_and_merge_uncommitted_worktree_edits() {
        let temp_dir = setup_git_repo();
        let repo_root = temp_dir.path();

        let agent_id = AgentId("coder-automerge".to_string());
        let handle = GitWorktreeManager::create_worktree(repo_root, &agent_id)
            .expect("create_worktree failed");

        // Simulate a MiniPub worker writing a file via write_file without running git commit
        std::fs::write(
            handle.worktree_path.join("feature.rs"),
            "pub fn compute() -> i32 { 42 }\n",
        )
        .expect("failed to write feature.rs");

        // Auto-commit and merge back into repo_root
        let merge_hash = GitWorktreeManager::auto_commit_and_merge(repo_root, &agent_id)
            .expect("auto_commit_and_merge failed");
        assert!(merge_hash.is_some());

        // Worktree should be cleaned up and file should be present in main workspace
        assert!(!handle.worktree_path.exists());
        let merged_content =
            std::fs::read_to_string(repo_root.join("feature.rs")).expect("feature.rs should exist");
        assert!(merged_content.contains("pub fn compute() -> i32 { 42 }"));
    }
}

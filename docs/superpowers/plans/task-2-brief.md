# Task 2 Brief: Worktree Retention and Lifecycle Management

## Overview
Enable the parent agent to inspect and merge a subagent's changes by retaining worktrees upon child process completion, and provide discovery helpers on `GitWorktreeManager`.

## Files to Modify:
- `src/sandbox/worktree.rs`
- `src/agent/subagent/orchestrator.rs`

## Interfaces & Requirements:

### 1. `src/sandbox/worktree.rs`
- Add public helper:
  ```rust
  pub fn branch_name_for(agent_id: &AgentId) -> String {
      format!("minicode/subagent/{}", agent_id.0)
  }
  ```
- Add public helper:
  ```rust
  pub fn locate_worktree(repo_root: &Path, agent_id: &AgentId) -> Option<PathBuf> {
      let worktrees_dir = repo_root.join(".minicode").join("worktrees");
      let prefixed = worktrees_dir.join(format!("subagent-{}", agent_id.0));
      if prefixed.exists() {
          return Some(prefixed);
      }
      let direct = worktrees_dir.join(&agent_id.0);
      if direct.exists() {
          return Some(direct);
      }
      None
  }
  ```
- Add public helper:
  ```rust
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
      Self::branch_name_for(agent_id)
  }
  ```
- In `GitWorktreeManager::create_worktree`:
  - Path: `repo_root.join(".minicode").join("worktrees").join(format!("subagent-{}", &agent_id.0))`
  - Branch name: `Self::branch_name_for(agent_id)`
  - Recreates cleanly if existing worktree or branch is present.
- Unit tests:
  - Implement `test_branch_name_and_locate_worktree()`:
    ```rust
    #[test]
    fn test_branch_name_and_locate_worktree() {
        let agent_id = AgentId("coder-test-99".to_string());
        let branch = GitWorktreeManager::branch_name_for(&agent_id);
        assert_eq!(branch, "minicode/subagent/coder-test-99");

        let repo_dir = tempfile::tempdir().unwrap();
        let expected_dir = repo_dir.path().join(".minicode").join("worktrees").join("subagent-coder-test-99");
        std::fs::create_dir_all(&expected_dir).unwrap();

        let located = GitWorktreeManager::locate_worktree(repo_dir.path(), &agent_id);
        assert_eq!(located, Some(expected_dir));
    }
    ```
  - Update `test_worktree_lifecycle_in_git_repo()` to reflect `minicode/subagent/coder-test` and ensure all tests in `sandbox::worktree::tests` pass.

### 2. `src/agent/subagent/orchestrator.rs`
- In `spawn_subagent`:
  - When `worktree_handle` is active and the child subagent succeeds (`child_success == true`), **do NOT remove the worktree**.
  - Capture diff via `GitWorktreeManager::capture_diff(handle).unwrap_or_default()`.
  - In the success report, note the retained worktree:
    ```rust
    if let Some(ref handle) = worktree_handle {
        report.push_str(&format!("\n• Worktree Retained: `{}` (branch `{}` — ready for `merge_subagent_worktree`)\n", handle.worktree_path.display(), handle.branch_name));
    }
    ```
  - Preserve worktree removal on all failure / error / timeout exit paths.

## Constraints:
- ONLY run targeted test: `cargo test -j 1 --lib sandbox::worktree::tests`
- Zero `.unwrap()` or `.expect()` in non-test code.
- Run `cargo fmt && cargo clippy -j 1 --bin minicode -- -D warnings`.
- Commit with message: `feat(sandbox): retain worktrees for arbitration and add resolution helpers (Phase 133)`
- Write execution report to `docs/superpowers/plans/task-2-report.md`.

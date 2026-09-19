# Task 2 Brief: Ephemeral Git Worktree Sandboxing Engine

## Scope & Objective
Implement the Git worktree sandboxing engine in `src/sandbox/worktree.rs` that provisions, manages, inspects, and cleans up isolated Git worktrees for mutating subagents (`coder`, `tester`).

## Files to Create/Modify
- Create: `src/sandbox/worktree.rs`
- Modify: `src/sandbox/mod.rs` (expose `pub mod worktree;`)

## Specifications & Requirements

### 1. `WorktreeHandle` Data Structure
In `src/sandbox/worktree.rs`:
```rust
use std::path::PathBuf;
use crate::agent::subagent::types::AgentId;

#[derive(Debug, Clone)]
pub struct WorktreeHandle {
    pub worktree_path: PathBuf,
    pub branch_name: String,
    pub agent_id: AgentId,
    pub repo_root: PathBuf,
}
```

### 2. `GitWorktreeManager` Implementation
Provide the following functions / methods on `GitWorktreeManager`:
- `pub fn create_worktree(repo_root: &Path, agent_id: &AgentId) -> std::io::Result<WorktreeHandle>`:
  1. Determine worktree path: `repo_root.join(".minicode").join("worktrees").join(&agent_id.0)`.
  2. Branch name: `format!("minicode-task-{}", &agent_id.0)`.
  3. Ensure `.minicode/worktrees/` parent exists.
  4. If `worktree_path` already exists, invoke `remove_worktree` or remove it first.
  5. Run `git worktree add -b <branch_name> <worktree_path> HEAD` with `current_dir(repo_root)`.
  6. If the command fails (e.g. not a git repo, or HEAD invalid), return `std::io::Error::new(std::io::ErrorKind::Other, format!("git worktree add failed: {}", stderr))`.
  7. Return `WorktreeHandle`.

- `pub fn capture_diff(handle: &WorktreeHandle) -> std::io::Result<String>`:
  1. In `handle.worktree_path`, run `git add -N .` (so untracked files are visible to diff) then `git diff HEAD`.
  2. If `git diff HEAD` fails or has no commits yet, fallback to `git diff` or return the diff output.
  3. Return the diff string (trimmed).

- `pub fn remove_worktree(handle: &WorktreeHandle) -> std::io::Result<()>`:
  1. Run `git worktree remove --force <worktree_path>` with `current_dir(&handle.repo_root)`.
  2. Run `git branch -D <branch_name>` with `current_dir(&handle.repo_root)`.
  3. If `handle.worktree_path.exists()`, remove it with `std::fs::remove_dir_all`.
  4. Run `git worktree prune` with `current_dir(&handle.repo_root)`.
  5. Return `Ok(())`.

- `pub fn cleanup_stale_worktrees(repo_root: &Path) -> std::io::Result<usize>`:
  1. Check `.minicode/worktrees/`. If it doesn't exist, return `Ok(0)`.
  2. Run `git worktree prune` in `repo_root`.
  3. Remove any remaining directories inside `.minicode/worktrees/`.
  4. Return count of directories cleaned.

### 3. Unit Tests Required
In `src/sandbox/worktree.rs`:
- `test_worktree_lifecycle_in_git_repo`:
  - Creates a temporary git repository using `tempfile::tempdir()`.
  - Runs `git init`, creates `file.txt`, commits `init`.
  - Calls `GitWorktreeManager::create_worktree`.
  - Verifies worktree directory exists and branch is created.
  - Modifies file in worktree.
  - Calls `GitWorktreeManager::capture_diff` and asserts diff contains modification.
  - Calls `GitWorktreeManager::remove_worktree`.
  - Asserts worktree directory no longer exists.
- `test_non_git_repo_worktree_error`:
  - Calls `create_worktree` on an empty non-git directory.
  - Asserts that it returns an `Err`.

## Critical Constraints
1. ONLY run targeted tests: `cargo test -j 1 --lib sandbox::worktree::tests`. NEVER run the full test suite.
2. Error handling: `std::io::Result`. Zero `.unwrap()` or `.expect()` in non-test code.
3. Concurrency: Use `-j 1` for `cargo check` and `cargo test`.
4. Verification: `cargo fmt` and `cargo clippy -j 1 --bin minicode -- -D warnings`.
5. Commit message: `feat(sandbox): implement ephemeral Git worktree manager for subagent isolation`.

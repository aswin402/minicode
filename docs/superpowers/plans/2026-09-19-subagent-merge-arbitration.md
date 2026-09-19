# Subagent Merge & Conflict Arbitration Engine Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Provide an autonomous conflict arbitration and merge engine with a new `merge_subagent_worktree` tool primitive that validates, checks mergeability, and merges ephemeral git worktree changes from child subagents into the main workspace.

**Architecture:** A dedicated `MergeArbitrator` inside `src/sandbox/arbitration.rs` auto-detects repo validation runners (`Cargo.toml` -> `cargo check -j 1`, `package.json` -> `bun/npm test`, etc.), runs sandboxed compilation/test checks inside the worktree, performs 3-way merge conflict detection against `HEAD`, and lands changes cleanly (either as a Git commit or working-tree patch) before pruning the worktree.

**Tech Stack:** Rust 2021, `tokio::process::Command`, `std::process::Command`, Git CLI, `thiserror`, `serde`, `serde_json`.

## Global Constraints

- **No full test suite:** NEVER run `cargo test` across the whole workspace. Only run targeted tests:
  - `cargo test -j 1 --lib sandbox::arbitration::tests`
  - `cargo test -j 1 --lib tools::tests::test_total_tool_count`
  - `cargo test -j 1 --test integration_subagent_merge`
- **Concurrency:** Use `-j 1` for `cargo check` and `cargo test`, `-j 2` for `cargo build`.
- **Pure Rust & Zero Unwraps:** Zero `.unwrap()` or `.expect()` in non-test code. Map errors cleanly to `ArbitrationError` and `ToolError`.
- **Resource & Disk Protection:** Ensure all created worktrees and temporary branches are safely removed after successful merge or on session teardown.
- **Tool Count Sync:** `TOTAL_TOOL_COUNT` must be accurately maintained in `src/constants.rs` (135 -> 136).

---

### Task 1: `MergeArbitrator` Core & Sandboxed Verification

**Files:**
- Create: `src/sandbox/arbitration.rs`
- Modify: `src/sandbox/mod.rs`

**Interfaces:**
- Consumes: `std::path::Path`, `std::process::Command`, `thiserror::Error`.
- Produces:
  - `ValidationReport`: `success: bool`, `command: String`, `exit_code: i32`, `stdout: String`, `stderr: String`, `duration_ms: u64`.
  - `MergeabilityReport`: `can_merge_cleanly: bool`, `conflicted_files: Vec<String>`, `merge_base: Option<String>`.
  - `MergeSuccessReport`: `subagent_id: String`, `branch_name: String`, `files_changed: Vec<String>`, `committed: bool`, `commit_hash: Option<String>`, `commit_message: Option<String>`.
  - `ArbitrationError`: Typed error variants for `WorktreeNotFound`, `VerificationFailed`, `MergeConflict`, `GitError`, `IoError`.
  - `MergeArbitrator::detect_project_validation_cmd(path: &Path) -> Option<Vec<String>>`
  - `MergeArbitrator::verify_worktree(worktree_path: &Path, check_cmd: Option<&str>) -> Result<ValidationReport, ArbitrationError>`
  - `MergeArbitrator::check_mergeability(repo_root: &Path, branch_name: &str) -> Result<MergeabilityReport, ArbitrationError>`
  - `MergeArbitrator::apply_merge(repo_root: &Path, branch_name: &str, commit: bool, commit_msg: Option<&str>) -> Result<MergeSuccessReport, ArbitrationError>`

- [ ] **Step 1: Write failing unit tests for project validation detection, mergeability checking, and merge application**

In `src/sandbox/arbitration.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_project_validation_cmd_cargo() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("Cargo.toml"), "[package]\nname = \"demo\"\n").unwrap();
        let cmd = MergeArbitrator::detect_project_validation_cmd(temp.path()).unwrap();
        assert_eq!(cmd, vec!["cargo", "check", "-j", "1"]);
    }

    #[test]
    fn test_detect_project_validation_cmd_package_json() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("package.json"), "{\"name\":\"demo\"}").unwrap();
        let cmd = MergeArbitrator::detect_project_validation_cmd(temp.path()).unwrap();
        assert_eq!(cmd, vec!["npm", "test"]);
    }

    #[test]
    fn test_verify_worktree_skip() {
        let temp = tempfile::tempdir().unwrap();
        let report = MergeArbitrator::verify_worktree(temp.path(), Some("skip")).unwrap();
        assert!(report.success);
        assert_eq!(report.command, "skip");
    }

    #[test]
    fn test_mergeability_and_apply_clean() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();

        // git init
        std::process::Command::new("git").args(["init"]).current_dir(root).output().unwrap();
        std::process::Command::new("git").args(["config", "user.name", "test"]).current_dir(root).output().unwrap();
        std::process::Command::new("git").args(["config", "user.email", "test@example.com"]).current_dir(root).output().unwrap();

        std::fs::write(root.join("hello.txt"), "base\n").unwrap();
        std::process::Command::new("git").args(["add", "."]).current_dir(root).output().unwrap();
        std::process::Command::new("git").args(["commit", "-m", "init"]).current_dir(root).output().unwrap();

        // create branch
        let branch = "minicode/subagent/coder-1";
        std::process::Command::new("git").args(["branch", branch]).current_dir(root).output().unwrap();

        // modify on branch
        let worktree_dir = root.join("wt");
        std::process::Command::new("git").args(["worktree", "add", worktree_dir.to_str().unwrap(), branch]).current_dir(root).output().unwrap();
        std::fs::write(worktree_dir.join("hello.txt"), "base\nupdated\n").unwrap();
        std::process::Command::new("git").args(["add", "."]).current_dir(&worktree_dir).output().unwrap();
        std::process::Command::new("git").args(["commit", "-m", "branch commit"]).current_dir(&worktree_dir).output().unwrap();

        // check mergeability
        let mergeability = MergeArbitrator::check_mergeability(root, branch).unwrap();
        assert!(mergeability.can_merge_cleanly);
        assert!(mergeability.conflicted_files.is_empty());

        // apply merge
        let success = MergeArbitrator::apply_merge(root, branch, true, Some("merge subagent")).unwrap();
        assert!(success.committed);
        assert!(success.files_changed.contains(&"hello.txt".to_string()));

        let content = std::fs::read_to_string(root.join("hello.txt")).unwrap();
        assert!(content.contains("updated"));
    }
}
```

- [ ] **Step 2: Run targeted tests to verify failure**
Run: `cargo test -j 1 --lib sandbox::arbitration::tests`
Expected: FAIL with missing module `sandbox::arbitration`.

- [ ] **Step 3: Implement `MergeArbitrator` and export in `src/sandbox/mod.rs`**
Implement:
- `ArbitrationError` with `thiserror::Error`.
- Project detector inspecting `Cargo.toml`, `package.json`, `pyproject.toml`, `go.mod`.
- Sandboxed command executor with 60s timeout.
- Git mergeability checker using `git merge-tree` or temporary dry-run merge.
- Safe branch merger / patch applier.

- [ ] **Step 4: Run targeted tests to verify they pass**
Run: `cargo test -j 1 --lib sandbox::arbitration::tests`
Expected: PASS (4 passed).

- [ ] **Step 5: Verify formatting and clippy**
Run: `cargo fmt && cargo clippy -j 1 --bin minicode -- -D warnings`
Expected: Clean with 0 warnings.

- [ ] **Step 6: Commit**
```bash
git add src/sandbox/arbitration.rs src/sandbox/mod.rs
git commit -m "feat(sandbox): implement MergeArbitrator with sandboxed verification and mergeability checks (Phase 133)"
```

---

### Task 2: Worktree Retention and Lifecycle Management

**Files:**
- Modify: `src/sandbox/worktree.rs`
- Modify: `src/agent/subagent/orchestrator.rs`

**Interfaces:**
- Consumes: `GitWorktreeManager`, `SubagentOrchestrator`, `WorkspaceMode`.
- Produces:
  - Preservation of `.minicode/worktrees/subagent-<id>` when child subagent process exits, so the parent can inspect and merge.
  - Expose helper `GitWorktreeManager::locate_worktree(repo_root: &Path, subagent_id: &AgentId) -> Option<PathBuf>`.
  - Expose helper `GitWorktreeManager::branch_name_for(subagent_id: &AgentId) -> String`.

- [ ] **Step 1: Write failing unit tests for worktree location and retention helpers**

In `src/sandbox/worktree.rs`:
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

- [ ] **Step 2: Run targeted tests to verify failure**
Run: `cargo test -j 1 --lib sandbox::worktree::tests`
Expected: FAIL with missing methods on `GitWorktreeManager`.

- [ ] **Step 3: Implement worktree location helpers and adjust orchestrator retention**
In `src/sandbox/worktree.rs`:
- Add `branch_name_for(agent_id: &AgentId) -> String`.
- Add `locate_worktree(repo_root: &Path, agent_id: &AgentId) -> Option<PathBuf>`.
In `src/agent/subagent/orchestrator.rs`:
- For mutating subagents (`WorkspaceMode::Worktree`), retain worktree on successful completion so `merge_subagent_worktree` can operate on it.
- Log retention path in subagent completion summary.

- [ ] **Step 4: Run targeted tests to verify they pass**
Run: `cargo test -j 1 --lib sandbox::worktree::tests`
Expected: PASS.

- [ ] **Step 5: Verify formatting and clippy**
Run: `cargo fmt && cargo clippy -j 1 --bin minicode -- -D warnings`
Expected: Clean with 0 warnings.

- [ ] **Step 6: Commit**
```bash
git add src/sandbox/worktree.rs src/agent/subagent/orchestrator.rs
git commit -m "feat(sandbox): retain worktrees for arbitration and add resolution helpers (Phase 133)"
```

---

### Task 3: `merge_subagent_worktree` Tool Primitive & Registry Registration

**Files:**
- Modify: `src/tools/registry/agent_tools/subagents.rs`
- Modify: `src/constants.rs`
- Modify: `src/tools/registry/agent_tools/mod.rs` (if exports needed)

**Interfaces:**
- Consumes: `MergeArbitrator`, `GitWorktreeManager`, `ToolRegistry`.
- Produces:
  - `MergeSubagentWorktreeArgs`: `subagent_id: String`, `commit: Option<bool>`, `check_cmd: Option<String>`, `commit_message: Option<String>`.
  - Tool registration `merge_subagent_worktree` in `ToolRegistry::get_tool_schemas()`.
  - Tool execution in `dispatch_tool()`.
  - Updates `TOTAL_TOOL_COUNT` from 135 to 136 in `src/constants.rs`.

- [ ] **Step 1: Write failing unit tests for arg parsing and total tool count**

In `src/tools/registry/agent_tools/subagents.rs`:
```rust
#[test]
fn test_merge_subagent_worktree_arg_parsing() {
    let json = serde_json::json!({
        "subagent_id": "coder-1",
        "commit": true,
        "check_cmd": "cargo check -j 1",
        "commit_message": "merge: auth feature"
    });
    let args: MergeSubagentWorktreeArgs = serde_json::from_value(json).unwrap();
    assert_eq!(args.subagent_id, "coder-1");
    assert_eq!(args.commit, Some(true));
    assert_eq!(args.check_cmd.as_deref(), Some("cargo check -j 1"));
    assert_eq!(args.commit_message.as_deref(), Some("merge: auth feature"));
}
```

- [ ] **Step 2: Run targeted tests to verify failure**
Run: `cargo test -j 1 --lib tools::tests::test_total_tool_count`
Expected: FAIL due to tool count mismatch (135 != 136) or missing struct.

- [ ] **Step 3: Implement `merge_subagent_worktree` handler, update tool count to 136, and register in schema**
In `src/tools/registry/agent_tools/subagents.rs`:
- Define `MergeSubagentWorktreeArgs`.
- Implement `merge_subagent_worktree()` tool function:
  1. Validate workspace root and resolve worktree path via `GitWorktreeManager::locate_worktree`.
  2. Run `MergeArbitrator::verify_worktree(worktree_path, check_cmd)`.
  3. If verification fails, return formatted error diagnostic.
  4. Run `MergeArbitrator::check_mergeability(repo_root, &branch_name)`.
  5. If conflict detected, return structured conflict diagnostic with list of conflicted files.
  6. Run `MergeArbitrator::apply_merge(repo_root, &branch_name, commit, commit_message)`.
  7. Teardown worktree and delete temporary branch via `GitWorktreeManager::remove_worktree`.
  8. Return markdown summary of files merged, commit hash, and verification duration.
- Register `merge_subagent_worktree` in `ToolRegistry::get_tool_schemas()` and `dispatch_tool`.
- Update `TOTAL_TOOL_COUNT = 136` in `src/constants.rs`.

- [ ] **Step 4: Run targeted tests to verify they pass**
Run: `cargo test -j 1 --lib tools::tests::test_total_tool_count`
Run: `cargo test -j 1 --lib tools::registry::agent_tools::subagents::tests`
Expected: PASS.

- [ ] **Step 5: Verify formatting and clippy**
Run: `cargo fmt && cargo clippy -j 1 --bin minicode -- -D warnings`
Expected: Clean with 0 warnings.

- [ ] **Step 6: Commit**
```bash
git add src/tools/registry/agent_tools/subagents.rs src/constants.rs
git commit -m "feat(tools): add merge_subagent_worktree tool primitive (136 tools) (Phase 133)"
```

---

### Task 4: End-to-End Integration Test Suite

**Files:**
- Create: `tests/integration_subagent_merge.rs`

**Interfaces:**
- Consumes: `MergeArbitrator`, `GitWorktreeManager`, `AgentId`.
- Produces: Full lifecycle verification tests for clean merge, pre-merge verification failure, and merge conflict handling.

- [ ] **Step 1: Write integration tests covering all 3 lifecycle paths**

In `tests/integration_subagent_merge.rs`:
```rust
use minicode::agent::subagent::types::AgentId;
use minicode::sandbox::arbitration::MergeArbitrator;
use minicode::sandbox::worktree::GitWorktreeManager;

#[test]
fn test_integration_subagent_worktree_clean_merge() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    // Init git repo
    std::process::Command::new("git").args(["init"]).current_dir(root).output().unwrap();
    std::process::Command::new("git").args(["config", "user.name", "test"]).current_dir(root).output().unwrap();
    std::process::Command::new("git").args(["config", "user.email", "test@test.com"]).current_dir(root).output().unwrap();

    std::fs::write(root.join("lib.rs"), "// initial\n").unwrap();
    std::process::Command::new("git").args(["add", "."]).current_dir(root).output().unwrap();
    std::process::Command::new("git").args(["commit", "-m", "init"]).current_dir(root).output().unwrap();

    let agent_id = AgentId("coder-test".to_string());
    let handle = GitWorktreeManager::create_worktree(root, &agent_id).unwrap();

    // Modify file inside worktree
    std::fs::write(handle.worktree_path.join("lib.rs"), "// initial\npub fn hello() {}\n").unwrap();
    std::process::Command::new("git").args(["add", "."]).current_dir(&handle.worktree_path).output().unwrap();
    std::process::Command::new("git").args(["commit", "-m", "coder add hello"]).current_dir(&handle.worktree_path).output().unwrap();

    // Verify worktree with skip
    let v_report = MergeArbitrator::verify_worktree(&handle.worktree_path, Some("skip")).unwrap();
    assert!(v_report.success);

    // Check mergeability
    let m_report = MergeArbitrator::check_mergeability(root, &handle.branch_name).unwrap();
    assert!(m_report.can_merge_cleanly);

    // Apply merge
    let res = MergeArbitrator::apply_merge(root, &handle.branch_name, true, Some("merge subagent coder")).unwrap();
    assert!(res.committed);

    // Teardown
    GitWorktreeManager::remove_worktree(&handle).unwrap();
    assert!(!handle.worktree_path.exists());

    // Verify parent has changes
    let final_content = std::fs::read_to_string(root.join("lib.rs")).unwrap();
    assert!(final_content.contains("pub fn hello()"));
}

#[test]
fn test_integration_subagent_worktree_conflict_detection() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    std::process::Command::new("git").args(["init"]).current_dir(root).output().unwrap();
    std::process::Command::new("git").args(["config", "user.name", "test"]).current_dir(root).output().unwrap();
    std::process::Command::new("git").args(["config", "user.email", "test@test.com"]).current_dir(root).output().unwrap();

    std::fs::write(root.join("conflict.txt"), "line1\nline2\n").unwrap();
    std::process::Command::new("git").args(["add", "."]).current_dir(root).output().unwrap();
    std::process::Command::new("git").args(["commit", "-m", "init"]).current_dir(root).output().unwrap();

    let agent_id = AgentId("coder-conflict".to_string());
    let handle = GitWorktreeManager::create_worktree(root, &agent_id).unwrap();

    // Worktree changes line2 to worktree
    std::fs::write(handle.worktree_path.join("conflict.txt"), "line1\nworktree edit\n").unwrap();
    std::process::Command::new("git").args(["add", "."]).current_dir(&handle.worktree_path).output().unwrap();
    std::process::Command::new("git").args(["commit", "-m", "worktree commit"]).current_dir(&handle.worktree_path).output().unwrap();

    // Main workspace changes line2 to main
    std::fs::write(root.join("conflict.txt"), "line1\nmain edit\n").unwrap();
    std::process::Command::new("git").args(["add", "."]).current_dir(root).output().unwrap();
    std::process::Command::new("git").args(["commit", "-m", "main concurrent commit"]).current_dir(root).output().unwrap();

    // Check mergeability -> should detect conflict
    let m_report = MergeArbitrator::check_mergeability(root, &handle.branch_name).unwrap();
    assert!(!m_report.can_merge_cleanly);
    assert!(m_report.conflicted_files.contains(&"conflict.txt".to_string()));

    // Cleanup
    GitWorktreeManager::remove_worktree(&handle).unwrap();
}
```

- [ ] **Step 2: Run integration tests to verify they pass**
Run: `cargo test -j 1 --test integration_subagent_merge`
Expected: PASS (2 passed).

- [ ] **Step 3: Verify formatting and clippy**
Run: `cargo fmt && cargo clippy -j 1 --bin minicode -- -D warnings`
Expected: Clean with 0 warnings.

- [ ] **Step 4: Commit**
```bash
git add tests/integration_subagent_merge.rs
git commit -m "test(sandbox): add integration tests for subagent merge and conflict arbitration (Phase 133)"
```

---

### Task 5: Quality Gates, Release Bump (v0.3.34), Binary Build & Real-World Validation

**Files:**
- Modify: `Cargo.toml`
- Modify: `onpkg_docs/todo.md`

- [ ] **Step 1: Bump version in `Cargo.toml` to `0.3.34`**
- [ ] **Step 2: Update `onpkg_docs/todo.md` with Phase 133 tracker checklist**
- [ ] **Step 3: Run clippy and format checks**
Run: `cargo clippy -j 1 --bin minicode -- -D warnings && cargo fmt --check`
Expected: Zero warnings, 100% formatted.
- [ ] **Step 4: Compile release binary via `./localupdate.sh`**
Run: `./localupdate.sh`
Expected: Binary successfully compiled and installed to `~/.local/bin/minicode`.
- [ ] **Step 5: Validate real-world autonomous execution with `minicode run`**
Run: `minicode run -d test_playground/realworld_subagent_test -p minimax -m MiniMax-M2.7 -y --json-stream "Check git status and summarize the latest commits in this repository."`
Expected: Output streamed, status complete, exit code 0.
- [ ] **Step 6: Commit release**
```bash
git add Cargo.toml Cargo.lock onpkg_docs/todo.md docs/superpowers/plans/
git commit -m "release(v0.3.34): deliver autonomous subagent merge and conflict arbitration engine (Phase 133)"
```

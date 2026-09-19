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


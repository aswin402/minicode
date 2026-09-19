# Task 4 Brief: End-to-End Integration Test Suite

## Overview
Implement an end-to-end integration test suite in `tests/integration_subagent_merge.rs` validating the complete arbitration and worktree merge lifecycle under clean merges, pre-merge verification failures, and merge conflicts.

## Files to Create:
- `tests/integration_subagent_merge.rs`

## Requirements:
1. **Clean Merge Lifecycle (`test_integration_subagent_worktree_clean_merge`)**:
   - Initialize temporary Git repo with user name and email.
   - Create worktree via `GitWorktreeManager::create_worktree`.
   - Modify and commit changes in the worktree.
   - Run `MergeArbitrator::verify_worktree(&handle.worktree_path, Some("skip"))`.
   - Run `MergeArbitrator::check_mergeability(root, &handle.branch_name)`.
   - Assert `can_merge_cleanly == true`.
   - Apply merge via `MergeArbitrator::apply_merge(root, &handle.branch_name, true, Some("merge commit"))`.
   - Clean up worktree via `GitWorktreeManager::remove_worktree`.
   - Assert file modifications are present in parent workspace.

2. **Conflict Detection (`test_integration_subagent_worktree_conflict_detection`)**:
   - Initialize temporary Git repo with initial base commit modifying `conflict.txt`.
   - Create worktree and commit changes to `conflict.txt` on subagent branch.
   - Make conflicting commit to `conflict.txt` on main branch in parent workspace.
   - Run `MergeArbitrator::check_mergeability(root, &handle.branch_name)`.
   - Assert `can_merge_cleanly == false` and `conflicted_files` contains `"conflict.txt"`.
   - Assert `MergeArbitrator::apply_merge` returns `Err(ArbitrationError::MergeConflict(_))`.
   - Clean up worktree and verify parent workspace is unharmed.

3. **Tool Primitive End-to-End (`test_integration_subagent_tool_full_lifecycle`)**:
   - Test `minicode::tools::registry::agent_tools::subagents::merge_subagent_worktree`.
   - Create worktree for subagent `coder-integration`.
   - Modify and commit files in worktree.
   - Execute `merge_subagent_worktree(root, "coder-integration", true, Some("skip"), Some("feat: integrated"))`.
   - Assert result is `Ok(summary)` containing success message and files changed.
   - Assert worktree is automatically cleaned up and files exist in main repo.

4. **Pre-Merge Verification Failure Handling (`test_integration_subagent_worktree_verification_failure`)**:
   - Create worktree and run `MergeArbitrator::verify_worktree` with a failing command (e.g. `false` or non-existent command).
   - Assert it returns `Err(ArbitrationError::VerificationFailed { .. })`.
   - Verify worktree path still exists on disk for remediation.

## Constraints:
- ONLY run targeted test: `cargo test -j 1 --test integration_subagent_merge`.
- Zero `.unwrap()` or `.expect()` in non-test code.
- Run `cargo fmt && cargo clippy -j 1 --bin minicode -- -D warnings`.
- Commit with message: `test(subagent): add integration test suite for worktree merge and conflict arbitration (Phase 133)`
- Write execution report to `docs/superpowers/plans/task-4-report.md`.

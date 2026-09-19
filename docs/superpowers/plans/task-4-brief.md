# Task 4 Brief: End-to-End Integration Test Suite

## Overview
Implement an end-to-end integration test suite in `tests/integration_subagent_fanout.rs` validating the complete swarm fan-out and sequential arbitration lifecycle:
1. Tool dispatch and argument parsing integration.
2. Sequential multi-worker clean arbitration and automatic worktree teardown.
3. Competing concurrent modifications and safe conflict isolation without parent corruption.
4. Pre-merge verification failure isolation and worktree retention.
5. Executive map-reduce matrix reporting across all worker outcome variants.

## Files to Create:
- `tests/integration_subagent_fanout.rs`

## Requirements:
1. **Tool Dispatch & Argument Parsing (`test_integration_fanout_dispatch_and_validation`)**:
   - Verify `minicode::tools::registry::agent_tools::swarms::dispatch` with empty tasks returns advisory message.
   - Verify invalid arguments (e.g. missing both `task` and `prompt`) returns `ToolError::InvalidArguments`.
   - Verify valid payload parsing with `parse_fanout_args` correctly handles roles, workspace modes, and aliases.

2. **Sequential Multi-Worker Clean Merge Arbitration (`test_integration_fanout_sequential_multi_worker_merge`)**:
   - Set up temporary Git repository.
   - Provision 2 concurrent mutating workers (`worker-alpha` and `worker-beta`) on distinct branches and worktrees modifying different files (`alpha.rs` and `beta.rs`).
   - Run `FanoutOrchestrator::arbitrate_mutating_workers`.
   - Assert both workers achieve `MergeStatus::Merged { commit_hash: Some(_) }`.
   - Assert both `alpha.rs` and `beta.rs` exist in the root repository.
   - Assert both worktrees are cleanly removed from disk.

3. **Competing Modifications & Conflict Isolation (`test_integration_fanout_conflict_isolation_retains_worktree`)**:
   - Set up temporary Git repository with initial commit containing `shared.txt`.
   - Worker 1 modifies `shared.txt` to "alpha content" and commits.
   - Worker 2 modifies `shared.txt` to "beta content" and commits.
   - Run `FanoutOrchestrator::arbitrate_mutating_workers`.
   - Assert Worker 1 merges cleanly and its worktree is removed.
   - Assert Worker 2 is marked with `MergeStatus::Conflict { conflicted_files }` where `conflicted_files` contains `"shared.txt"`.
   - Assert Worker 2's worktree directory is preserved on disk for manual remediation.
   - Assert parent workspace working tree is clean and contains Worker 1's version without conflict markers.

4. **Pre-Merge Verification Failure Handling (`test_integration_fanout_verification_failure_isolation`)**:
   - Provision worker with `check_cmd: Some("false")`.
   - Run `FanoutOrchestrator::arbitrate_mutating_workers`.
   - Assert worker is marked with `MergeStatus::VerificationFailed { .. }`.
   - Assert worker's worktree directory is preserved on disk.
   - Assert parent workspace is not modified.

5. **Executive Map-Reduce Matrix Report Formatting (`test_integration_fanout_matrix_reporting`)**:
   - Construct swarm results including Merged, Conflict, VerificationFailed, RetainedUnmerged, and SkippedCancelled.
   - Run `FanoutOrchestrator::format_fanout_report`.
   - Verify Markdown table structure, metrics header, diagnostics callout blocks, and individual summaries.

## Constraints:
- ONLY run targeted test: `cargo test -j 1 --test integration_subagent_fanout`.
- Zero `.unwrap()` or `.expect()` in non-test code.
- Run `cargo fmt && cargo clippy -j 1 --bin minicode -- -D warnings`.
- Commit with message: `test(subagent): add integration test suite for swarm fanout and aggregate arbitration (Phase 134)`.
- Write execution report to `docs/superpowers/plans/task-4-report.md`.

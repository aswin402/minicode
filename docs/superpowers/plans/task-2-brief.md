# Task 2 Brief: Sequential Arbitration & Auto-Merge Pipeline

## Overview
Implement sequential conflict-free arbitration and auto-merging of mutating subagent worktrees in `src/agent/subagent/fanout.rs`, and generate a comprehensive map-reduce markdown report.

## Files to Modify:
- `src/agent/subagent/fanout.rs`

## Interfaces & Requirements:

### 1. `arbitrate_mutating_workers`:
Implement `pub async fn arbitrate_mutating_workers(workspace_root: &Path, results: &mut [WorkerResult], tasks: &[FanoutTaskItem])`:
- Iterates sequentially through `results`.
- For each worker with `success == true`, `worktree_path.is_some()`, and `branch_name.is_some()`:
  1. Runs `MergeArbitrator::verify_worktree(worktree_path, check_cmd)`.
     - On failure: sets `res.merge_status = MergeStatus::VerificationFailed { command, exit_code, stderr }`.
     - Worktree directory is preserved on disk for remediation. Skips to next worker.
  2. Runs `MergeArbitrator::check_mergeability(workspace_root, branch_name)`.
     - If `!report.can_merge_cleanly`: sets `res.merge_status = MergeStatus::Conflict { conflicted_files: report.conflicted_files }`.
     - Worktree directory is preserved on disk. Skips to next worker.
  3. Runs `MergeArbitrator::apply_merge(workspace_root, branch_name, true, Some(&commit_message))`.
     - On success: sets `res.merge_status = MergeStatus::Merged { commit_hash: report.commit_hash }`.
     - Tears down worktree and branch via `GitWorktreeManager::remove_worktree`.
     - On conflict: sets `res.merge_status = MergeStatus::Conflict { conflicted_files }`.

### 2. Wire into `execute_fanout`:
In `FanoutOrchestrator::execute_fanout`:
- If `auto_merge == true`:
  Calls `Self::arbitrate_mutating_workers(workspace_root, &mut completed_results, &tasks).await;`.
- Formats final report via `Self::format_fanout_report(&completed_results, join_mode, auto_merge, total_duration_ms)`.

### 3. `format_fanout_report`:
Implement `pub fn format_fanout_report(results: &[WorkerResult], join_mode: FanoutJoinMode, auto_merge: bool, total_duration_ms: u64) -> String`:
- Displays swarm header with worker count, total elapsed time, join mode, and auto-merge status.
- Renders an executive markdown matrix table:
  `| # | Worker ID | Role | Status | Duration | Tokens | Files | Merge Outcome |`
- Formats merge outcomes cleanly:
  - `✔ Merged (hash)`
  - `❌ Verification Failed (cmd)`
  - `⚠️ Conflict (files)`
  - `📁 Retained (path)`
  - `⏹ Cancelled (race)`
  - `—` (Not Applicable)
- Includes per-worker executive summaries and diffs.
- Includes clear diagnostic alerts for any conflicts or failed verifications with preserved worktree paths.

### 4. Unit Tests in `src/agent/subagent/fanout.rs`:
- `test_arbitrate_mutating_workers_clean_merge`:
  - Sets up temp git repo.
  - Provisions worktree for worker 1 modifying `file_a.txt` and commits.
  - Runs `arbitrate_mutating_workers`.
  - Asserts `res.merge_status` is `MergeStatus::Merged { .. }`.
  - Asserts `file_a.txt` exists in parent workspace and worktree directory is removed.
- `test_arbitrate_mutating_workers_conflict`:
  - Sets up temp git repo.
  - Creates base commit with `shared.txt`.
  - Main modifies `shared.txt` to B and commits.
  - Worktree modifies `shared.txt` to A and commits.
  - Runs `arbitrate_mutating_workers`.
  - Asserts `res.merge_status` is `MergeStatus::Conflict { .. }`.
  - Asserts worktree directory still exists on disk.
- `test_format_fanout_report`:
  - Tests rendering of `format_fanout_report` across various `MergeStatus` variants.

## Constraints:
- ONLY run targeted test: `cargo test -j 1 --lib agent::subagent::fanout::tests`.
- Zero `.unwrap()` or `.expect()` in non-test code.
- Run `cargo fmt && cargo clippy -j 1 --bin minicode -- -D warnings`.
- Commit with message: `feat(subagent): implement sequential merge arbitration and map-reduce reporting (Phase 134)`
- Write execution report to `docs/superpowers/plans/task-2-report.md`.

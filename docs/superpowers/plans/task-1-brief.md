# Task 1 Brief: Core Types, Concurrency Engine & `FanoutOrchestrator` Foundation

## Overview
Implement the core data structures and concurrency runtime for parallel subagent swarm execution in `src/agent/subagent/fanout.rs`, providing bounded concurrency and race/all join policies.

## Files to Create/Modify:
- Create: `src/agent/subagent/fanout.rs`
- Modify: `src/agent/subagent/mod.rs` (register and export `pub mod fanout;`)

## Interfaces & Requirements:

### 1. Data Structures (`src/agent/subagent/fanout.rs`):
```rust
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use crate::agent::subagent::types::{AgentId, SubagentRole, WorkspaceMode};
use crate::error::ToolError;

/// Specification for an individual worker in a fanout swarm.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FanoutTaskItem {
    pub task: String,
    pub role: SubagentRole,
    #[serde(default)]
    pub workspace_mode: Option<WorkspaceMode>,
    #[serde(default)]
    pub max_iterations: Option<usize>,
    #[serde(default)]
    pub check_cmd: Option<String>,
}

/// Completion mode for the swarm.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FanoutJoinMode {
    All,
    Race,
}

impl Default for FanoutJoinMode {
    fn default() -> Self {
        Self::All
    }
}

/// Status of worktree merge arbitration for a worker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MergeStatus {
    NotApplicable,
    Merged { commit_hash: Option<String> },
    VerificationFailed { command: String, exit_code: i32, stderr: String },
    Conflict { conflicted_files: Vec<String> },
    RetainedUnmerged,
    SkippedCancelled,
}

/// Outcome of an individual worker within the swarm.
#[derive(Debug, Clone)]
pub struct WorkerResult {
    pub agent_id: AgentId,
    pub role: SubagentRole,
    pub task: String,
    pub success: bool,
    pub duration_ms: u64,
    pub tokens_used: usize,
    pub files_modified: Vec<String>,
    pub worktree_path: Option<PathBuf>,
    pub branch_name: Option<String>,
    pub merge_status: MergeStatus,
    pub summary: String,
    pub error: Option<String>,
}
```

### 2. `FanoutOrchestrator` Implementation:
```rust
pub struct FanoutOrchestrator;

impl FanoutOrchestrator {
    /// Concurrently executes a batch of subagent tasks according to join mode and arbitration policy.
    pub async fn execute_fanout(
        workspace_root: &Path,
        tasks: Vec<FanoutTaskItem>,
        join_mode: FanoutJoinMode,
        auto_merge: bool,
        max_concurrency: usize,
    ) -> Result<String, ToolError> {
        if tasks.is_empty() {
            return Ok("No subagent tasks specified for fan-out.".to_string());
        }

        let concurrency = max_concurrency.clamp(1, 16);
        let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(concurrency));
        let cancel_token = tokio_util::sync::CancellationToken::new();

        // Spawn workers and join according to join_mode
        // ...
    }
}
```

- In `run_single_worker`:
  - Acquires permit from `semaphore`.
  - Checks if `cancel_token.is_cancelled()`. If cancelled, returns early with `MergeStatus::SkippedCancelled`.
  - Determines workspace isolation: `workspace_mode` or role default (`WorkspaceMode::Worktree` for `coder`/`tester`).
  - Spawns child process via `tokio::process::Command` (calling `std::env::current_exe()`) with `kill_on_drop(true)`, `process_group(0)`, and pipes for stdout/stderr.
  - Monitors stdout for NDJSON stream events, accumulating tokens, modified files, and final summary.
  - If `cancel_token` is cancelled during execution, kills the child process and cleans up any provisioned worktree.
  - On child success, if worktree was used, captures diff and preserves worktree for arbitration.
  - On child failure, cleans up worktree.
- If `join_mode == FanoutJoinMode::Race`:
  - When any worker completes with `success == true`, calls `cancel_token.cancel()`.
- Generates a baseline report summarizing worker outcomes.

### 3. Unit Tests in `src/agent/subagent/fanout.rs`:
- `test_fanout_join_mode_serialization`: tests JSON round-trip of `FanoutJoinMode::All` and `FanoutJoinMode::Race`.
- `test_fanout_empty_tasks`: tests `execute_fanout` with empty tasks vector returns "No subagent tasks specified for fan-out.".
- `test_fanout_task_item_deserialization`: tests deserialization of a full `FanoutTaskItem` from JSON.
- `test_merge_status_variants`: tests `MergeStatus` variants equality and formatting.

## Constraints:
- ONLY run targeted test: `cargo test -j 1 --lib agent::subagent::fanout::tests`.
- Zero `.unwrap()` or `.expect()` in non-test code.
- Run `cargo fmt && cargo clippy -j 1 --bin minicode -- -D warnings`.
- Commit with message: `feat(subagent): implement FanoutOrchestrator engine with bounded concurrency and race join (Phase 134)`
- Write execution report to `docs/superpowers/plans/task-1-report.md`.

# Design Specification: Parallel Subagent Swarm Fan-Out & Aggregate Arbitration Engine (Phase 134)

## 1. Executive Summary & Architecture

In Phase 132 and 133, `minicode` established single-subagent delegation (`spawn_subagent`), reactive mailboxes (`AgentMailbox`), ephemeral Git worktrees (`GitWorktreeManager`), and autonomous merge arbitration (`MergeArbitrator` / `merge_subagent_worktree`).

**Phase 134** unifies and modernizes multi-agent swarm execution by delivering **Parallel Subagent Swarm Fan-Out & Aggregate Arbitration (`fanout_subagents`)**. 

```
┌────────────────────────────────────────────────────────────────────────────────────────┐
│                               Parent Agent Loop / Coordinator                          │
└───────────────────────────────────────────┬────────────────────────────────────────────┘
                                            │ fanout_subagents(tasks, join_mode, auto_merge)
                                            ▼
┌────────────────────────────────────────────────────────────────────────────────────────┐
│                               FanoutOrchestrator Engine                                │
│  - Concurrency Limiter: tokio::sync::Semaphore(max_concurrency)                        │
│  - Task Join Modes: JoinMode::All (default) | JoinMode::Race (first success wins)       │
│  - Process Management: tokio::task::JoinSet with cancellation token                    │
└───────┬───────────────────────────────────┬────────────────────────────────────┬───────┘
        │ Worker 1 (scout)                  │ Worker 2 (coder)                   │ Worker 3 (reviewer)
        ▼                                   ▼                                    ▼
┌──────────────────┐               ┌──────────────────┐                 ┌──────────────────┐
│ Child minicode   │               │ Child minicode   │                 │ Child minicode   │
│ Shared Read-Only │               │ Worktree wt-1    │                 │ Worktree wt-2    │
└───────┬──────────┘               └────────┬─────────┘                 └────────┬─────────┘
        │                                   │                                    │
        └───────────────────────────────────┼────────────────────────────────────┘
                                            ▼
┌────────────────────────────────────────────────────────────────────────────────────────┐
│                        Sequential Arbitration Pipeline (if auto_merge: true)           │
│                                                                                        │
│ For each mutating worker in completion order:                                          │
│   1. MergeArbitrator::verify_worktree(wt, check_cmd)                                   │
│      └─ Fail -> Retain worktree, record diagnostic failure                             │
│   2. MergeArbitrator::check_mergeability(repo_root, branch)                            │
│      └─ Conflict -> Retain worktree, record conflicted files                           │
│   3. MergeArbitrator::apply_merge(repo_root, branch, commit: true)                     │
│      └─ Clean -> Commit changes to main workspace                                      │
│   4. GitWorktreeManager::remove_worktree(handle)                                       │
│      └─ Prune branch and worktree directory                                            │
└───────────────────────────────────────────┬────────────────────────────────────────────┘
                                            ▼
┌────────────────────────────────────────────────────────────────────────────────────────┐
│                       Aggregated Map-Reduce Markdown Report                            │
│  - Summary Table: Worker ID, Role, Status, Time, Tokens, Files, Merge Outcome          │
│  - Sectioned Findings & Summaries                                                      │
│  - Diagnostics for conflicts or failed verifications                                   │
└────────────────────────────────────────────────────────────────────────────────────────┘
```

---

## 2. Core Requirements & Invariants

1. **Parallel Worker Execution:**
   - Tasks execute concurrently using `tokio::task::JoinSet`.
   - Concurrency is bounded by `max_concurrency` (default: 4, configurable from 1 to 16) via an `Arc<Semaphore>`.
2. **Flexible Join Modes (`join_mode`):**
   - `"all"` (default): Awaits all workers to finish (or timeout). Compiles results into an aggregated matrix.
   - `"race"`: The first worker to successfully complete with exit code 0 wins. All remaining active workers are cancelled, their processes terminated (`child.kill()`), and their unmerged worktrees cleaned up.
3. **Sequential Arbitration & Landing (`auto_merge: true`):**
   - Parallel Git branches cannot be merged concurrently into the same working tree due to index locking and commit race conditions.
   - Mutating workers with worktrees are processed sequentially through `MergeArbitrator`.
   - If worker $A$ merges cleanly, worker $B$ is checked against the updated `HEAD`. If worker $B$ conflicts with worker $A$'s changes, worker $B$'s worktree is preserved on disk, and a detailed diagnostic is returned in the report.
4. **Pure Rust, Zero Panics:**
   - No `.unwrap()` or `.expect()` in non-test code.
   - All errors mapped to `ToolError` / `MinicodeError`.
5. **Accurate Tool Count:**
   - `fanout_subagents` is already registered in `src/tools/registry/agent_tools/swarms.rs`. `TOTAL_TOOL_COUNT` remains at 135.

---

## 3. Detailed Component Architecture

### 3.1. Data Models (`src/agent/subagent/fanout.rs`)

```rust
use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use crate::agent::subagent::types::{AgentId, SubagentRole, WorkspaceMode};

/// Specification for an individual worker in a fanout swarm.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FanoutTaskItem {
    pub task: String,
    pub role: SubagentRole,
    pub workspace_mode: Option<WorkspaceMode>,
    pub max_iterations: Option<usize>,
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

/// Status of the worker's worktree merge arbitration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MergeStatus {
    NotApplicable,      // Read-only / shared mode
    Merged { commit_hash: Option<String> },
    VerificationFailed { command: String, exit_code: i32, stderr: String },
    Conflict { conflicted_files: Vec<String> },
    RetainedUnmerged,   // auto_merge was false
    SkippedCancelled,   // Cancelled due to race mode
}
```

---

### 3.2. `FanoutOrchestrator` Engine

Located in `src/agent/subagent/fanout.rs`:

```rust
pub struct FanoutOrchestrator;

impl FanoutOrchestrator {
    /// Executes a concurrent batch of subagent tasks according to join mode and arbitration policy.
    pub async fn execute_fanout(
        workspace_root: &Path,
        tasks: Vec<FanoutTaskItem>,
        join_mode: FanoutJoinMode,
        auto_merge: bool,
        max_concurrency: usize,
    ) -> Result<String, ToolError>;
}
```

#### Execution Logic:
1. **Validation**: If `tasks.is_empty()`, return an early advisory note.
2. **Concurrency Limiter**: Bounded via `Arc<tokio::sync::Semaphore>::new(max_concurrency.clamp(1, 16))`.
3. **Execution Loop**:
   - Spawns tasks into a `tokio::task::JoinSet`.
   - Uses `tokio_util::sync::CancellationToken` to signal cancellation to other workers if `join_mode == FanoutJoinMode::Race`.
4. **Worker Execution (`run_worker`)**:
   - Provisions worktree if `workspace_mode` is `Worktree` (or `Auto` with mutating role `coder`/`tester`).
   - Invokes child `minicode run -d <target_dir> -y --json-stream` under isolation.
   - Collects tokens, modified files, diff, and summary findings.
   - On cancellation, kills child process group and tears down unmerged worktree.
5. **Sequential Arbitration (`arbitrate_mutating_workers`)**:
   - If `auto_merge == true`, iterates through successful mutating workers in order.
   - Calls `MergeArbitrator::verify_worktree(worktree_path, check_cmd)`.
   - Calls `MergeArbitrator::check_mergeability(repo_root, branch_name)`.
   - Calls `MergeArbitrator::apply_merge(repo_root, branch_name, commit: true, None)`.
   - On success: cleans up worktree and branch via `GitWorktreeManager::remove_worktree`. Sets `MergeStatus::Merged`.
   - On conflict or verification error: leaves worktree intact on disk. Sets `MergeStatus::Conflict` or `MergeStatus::VerificationFailed`.
6. **Report Synthesis**:
   - Synthesizes an executive Markdown table + detailed per-worker findings and error diagnostics.

---

### 3.3. Tool Parameter Upgrade in `src/tools/registry/agent_tools/swarms.rs`

Update `fanout_subagents` schema:
```json
{
  "name": "fanout_subagents",
  "description": "Concurrently dispatch a batch of specialized subagents across isolated Git Worktrees or read-only workspaces. Automatically aggregates findings, and optionally arbitrates and merges worktrees sequentially without conflict race conditions.",
  "parameters": {
    "type": "object",
    "properties": {
      "tasks": {
        "type": "array",
        "description": "List of subagent task specifications to execute concurrently",
        "items": {
          "type": "object",
          "properties": {
            "task": {
              "type": "string",
              "description": "Detailed prompt/instructions for this worker"
            },
            "prompt": {
              "type": "string",
              "description": "Alias for task"
            },
            "role": {
              "type": "string",
              "enum": ["scout", "coder", "tester", "reviewer", "researcher", "code_reviewer", "test_engineer", "security_auditor"],
              "description": "Specialized role preset defining worker capabilities and workspace isolation"
            },
            "workspace_mode": {
              "type": "string",
              "enum": ["auto", "worktree", "shared"],
              "description": "Workspace isolation mode (default: auto)"
            },
            "max_iterations": {
              "type": "integer",
              "description": "Maximum tool loop iterations before terminating"
            },
            "check_cmd": {
              "type": "string",
              "description": "Optional pre-merge validation command override"
            }
          },
          "required": ["role"]
        }
      },
      "join_mode": {
        "type": "string",
        "enum": ["all", "race"],
        "description": "Join policy: 'all' awaits all workers (default); 'race' returns immediately when the first worker succeeds and cancels the rest"
      },
      "auto_merge": {
        "type": "boolean",
        "description": "If true, automatically arbitrates and merges completed worktree modifications sequentially (default: false)"
      },
      "max_concurrency": {
        "type": "integer",
        "description": "Maximum concurrent workers (default: 4, min: 1, max: 16)"
      }
    },
    "required": ["tasks"]
  }
}
```

---

## 4. Verification & Testing Strategy

1. **Unit Tests (`src/agent/subagent/fanout.rs`)**:
   - `test_fanout_join_mode_all_aggregation`: Verifies parallel execution of multiple read-only tasks and matrix synthesis.
   - `test_fanout_join_mode_race`: Verifies that as soon as one worker succeeds, slower workers are cancelled.
   - `test_fanout_concurrency_bounding`: Verifies semaphore limits active tasks.
   - `test_fanout_auto_merge_sequential_clean`: Verifies two mutating workers landing distinct files sequentially.
   - `test_fanout_auto_merge_conflict_isolation`: Verifies that when worker 2 conflicts with worker 1's landed changes, worker 2's worktree is preserved and reported as conflicted.
2. **Integration Test (`tests/integration_subagent_fanout.rs`)**:
   - End-to-end execution of `fanout_subagents` tool through `dispatch()`.
3. **Quality Gates**:
   - `cargo test -j 1 --lib agent::subagent::fanout::tests`
   - `cargo test -j 1 --test integration_subagent_fanout`
   - `cargo clippy -j 1 --bin minicode -- -D warnings`
   - `cargo fmt --check`
   - Real-world `minicode run` validation.

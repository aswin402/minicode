# Implementation Plan: Parallel Subagent Swarm Fan-Out & Aggregate Arbitration Engine (Phase 134)

## Overview
Phase 134 implements `FanoutOrchestrator` and upgrades `fanout_subagents` to empower parent agents with concurrent subagent execution across isolated Git worktrees, bounded concurrency, race/all join policies, sequential conflict-free merge arbitration, and unified map-reduce reporting.

---

## User Review Checkpoints
- **Spec Review**: [`docs/superpowers/specs/2026-09-19-parallel-swarm-fanout-design.md`](file:///home/aswin/programming/vscode/myProjects/ai_agent_tools/minicode/docs/superpowers/specs/2026-09-19-parallel-swarm-fanout-design.md)
- **Task 1 Review**: Core Types, Concurrency Engine & `FanoutOrchestrator` Foundation
- **Task 2 Review**: Sequential Arbitration & Auto-Merge Pipeline
- **Task 3 Review**: Tool Schema Upgrade & Registry Dispatch in `swarms.rs`
- **Task 4 Review**: End-to-End Integration Test Suite
- **Task 5 Review**: Release v0.3.35 & Real-World Validation

---

## Tasks

### Task 1: Core Types, Concurrency Engine & `FanoutOrchestrator` Foundation

**Files:**
- Create: `src/agent/subagent/fanout.rs`
- Modify: `src/agent/subagent/mod.rs`

**Interfaces:**
- Consumes: `AgentId`, `SubagentRole`, `WorkspaceMode`, `SubagentOrchestrator`, `GitWorktreeManager`.
- Produces:
  - `FanoutTaskItem`, `FanoutJoinMode`, `WorkerResult`, `MergeStatus`.
  - `FanoutOrchestrator::execute_fanout(workspace_root, tasks, join_mode, auto_merge, max_concurrency)`.
  - Bounded concurrency with `tokio::sync::Semaphore`.
  - `JoinMode::Race` support with cancellation tokens and child process termination.

- [ ] **Step 1: Write failing unit tests for `FanoutJoinMode` and task execution**
- [ ] **Step 2: Implement core types and `FanoutOrchestrator` execution loop**
- [ ] **Step 3: Run targeted tests: `cargo test -j 1 --lib agent::subagent::fanout::tests`**
- [ ] **Step 4: Verify formatting & clippy: `cargo fmt && cargo clippy -j 1 --bin minicode -- -D warnings`**
- [ ] **Step 5: Commit: `feat(subagent): implement FanoutOrchestrator engine with bounded concurrency and race join (Phase 134)`**

---

### Task 2: Sequential Arbitration & Auto-Merge Pipeline

**Files:**
- Modify: `src/agent/subagent/fanout.rs`

**Interfaces:**
- Consumes: `MergeArbitrator`, `GitWorktreeManager`, `WorkerResult`, `MergeStatus`.
- Produces:
  - `arbitrate_mutating_workers(&Path, &mut [WorkerResult])`.
  - Sequential pre-merge verification and 3-way conflict checking for mutating workers.
  - Worktree cleanup on clean merge, retention on conflict/failure.
  - `format_fanout_report(&[WorkerResult], FanoutJoinMode, bool, u64) -> String`.

- [ ] **Step 1: Write unit tests for sequential arbitration and report generation**
- [ ] **Step 2: Implement `arbitrate_mutating_workers` and `format_fanout_report`**
- [ ] **Step 3: Run targeted tests: `cargo test -j 1 --lib agent::subagent::fanout::tests`**
- [ ] **Step 4: Verify formatting & clippy: `cargo fmt && cargo clippy -j 1 --bin minicode -- -D warnings`**
- [ ] **Step 5: Commit: `feat(subagent): implement sequential merge arbitration and map-reduce reporting (Phase 134)`**

---

### Task 3: Tool Schema Upgrade & Registry Dispatch in `swarms.rs`

**Files:**
- Modify: `src/tools/registry/agent_tools/swarms.rs`

**Interfaces:**
- Consumes: `FanoutOrchestrator`, `FanoutTaskItem`, `FanoutJoinMode`.
- Produces:
  - Updated `fanout_subagents` ToolSchema supporting `join_mode`, `max_concurrency`, `auto_merge`, modern roles.
  - Dispatch handling routing to `FanoutOrchestrator::execute_fanout`.
  - Preserves `TOTAL_TOOL_COUNT` = 135.

- [ ] **Step 1: Write unit tests for `fanout_subagents` argument parsing in `swarms.rs`**
- [ ] **Step 2: Update ToolSchema and wire `dispatch`**
- [ ] **Step 3: Run targeted tests: `cargo test -j 1 --lib tools::tests::test_total_tool_count`**
- [ ] **Step 4: Verify formatting & clippy: `cargo fmt && cargo clippy -j 1 --bin minicode -- -D warnings`**
- [ ] **Step 5: Commit: `feat(tools): upgrade fanout_subagents tool primitive with modern swarm orchestration (Phase 134)`**

---

### Task 4: End-to-End Integration Test Suite

**Files:**
- Create: `tests/integration_subagent_fanout.rs`

**Interfaces:**
- Consumes: `minicode::tools::registry::agent_tools::swarms::dispatch`.
- Produces:
  - Integration tests for parallel fanout, race mode, and sequential auto-merge.

- [ ] **Step 1: Write integration tests covering all fanout execution paths**
- [ ] **Step 2: Run targeted tests: `cargo test -j 1 --test integration_subagent_fanout`**
- [ ] **Step 3: Verify formatting & clippy: `cargo fmt && cargo clippy -j 1 --bin minicode -- -D warnings`**
- [ ] **Step 4: Commit: `test(subagent): add integration test suite for swarm fanout and aggregate arbitration (Phase 134)`**

---

### Task 5: Quality Gates, Release Bump (v0.3.35), Binary Build & Real-World Validation

**Files:**
- Modify: `Cargo.toml`
- Modify: `onpkg_docs/todo.md`

- [ ] **Step 1: Bump version in `Cargo.toml` to `0.3.35`**
- [ ] **Step 2: Update `onpkg_docs/todo.md` with Phase 134 checklist**
- [ ] **Step 3: Run clippy and format checks**
- [ ] **Step 4: Compile release binary via `./localupdate.sh`**
- [ ] **Step 5: Validate real-world autonomous execution with `minicode run`**
- [ ] **Step 6: Commit release: `release(v0.3.35): deliver parallel subagent swarm fan-out & aggregate arbitration engine (Phase 134)`**

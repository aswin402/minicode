# Semantic Micro-Compaction of Tool Observations Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement an autonomous, zero-data-loss Semantic Micro-Compactor that continuously detects and condenses superseded file reads, duplicate tool executions, historical mutation echoes, and stale search floods into concise 1-line semantic receipts backed by `CcrCache`, slashing multi-turn token consumption by 40–70% without losing context.

**Architecture:**
- **MicroCompactor Engine (`src/context/budget/micro_compact.rs`):** Scans the `Vec<Message>` conversational history to build a turn-indexed artifact lifecycle map. Identifies file reads that have subsequently been modified (`patch_file`, `write_file`, `replace_file_content`), duplicate consecutive reads of identical targets, historical large mutation echoes (>2 turns old), and bulky search results (>30 lines) whose targets have already been visited.
- **Lossless CCR Backing:** Every micro-compacted tool payload is hashed and stored in `CcrCache` prior to replacement, ensuring the agent can retrieve the exact verbatim raw observation on demand via `retrieve_observation(id="ccr_...")`.
- **AgentLoop Lifecycle Integration (`src/agent/loop.rs`):** Invoked at turn boundaries and immediately following successful file mutation passes, preserving the current active turn's observations with 100% fidelity while compressing stale historical turns.
- **Zero-Hardcoding Principles:** Discovers tool argument keys dynamically (`path`, `target_file`, `file_path`, `query`, `pattern`), supports extensible tool sets (MCP tools, custom commands), and scales across all token windows from local 8k models to 2M+ frontier models.

**Tech Stack:**
- Rust 2021 Edition, Tokio async runtime
- `CcrCache` (`crate::context::budget::ccr_cache::CcrCache`)
- `Message`, `ToolCall`, `Role` (`crate::agent::types`)
- `thiserror`, `tracing`, `serde_json`

## Global Constraints
- Always use `cargo test -j 1` and `cargo check -j 1` to respect system resource constraints.
- NEVER run the full test suite (`cargo test -j 1`). ONLY run targeted tests for modified modules.
- Zero `.unwrap()` or `.expect()` in non-test library code.
- Pure Rust, no external non-Rust CLI dependencies.
- Preserve the active (most recent) turn completely uncompacted so the LLM has full immediate context.
- Every compacted observation MUST be losslessly recoverable via `retrieve_observation`.

---

### Task 1: MicroCompactor Engine & Superseded File Read Detection

**Files:**
- Create: `src/context/budget/micro_compact.rs`
- Modify: `src/context/budget/mod.rs`
- Test: `src/context/budget/micro_compact.rs` (inline unit tests)

**Interfaces:**
- Produces:
  ```rust
  #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
  pub struct MicroCompactMetrics {
      pub superseded_reads_compacted: usize,
      pub duplicate_reads_compacted: usize,
      pub mutation_echoes_compacted: usize,
      pub search_results_compacted: usize,
      pub tokens_saved_estimate: usize,
  }

  pub struct MicroCompactor;

  impl MicroCompactor {
      pub fn compact_messages(messages: &mut [Message], preserve_recent_turns: usize) -> MicroCompactMetrics;
      pub fn extract_target_path(tool_call: &ToolCall) -> Option<String>;
      pub fn extract_search_query(tool_call: &ToolCall) -> Option<String>;
  }
  ```
- Consumes: `CcrCache::store`, `Message`, `ToolCall`, `Role`

- [ ] **Step 1: Write the failing unit tests for `MicroCompactor`**
  - In `src/context/budget/micro_compact.rs`:
    - `test_extract_target_path_various_schemas`: verifies extraction across `path`, `target_file`, `file_path`, `file`, `TargetFile`, `AbsolutePath`.
    - `test_superseded_file_read_compaction`: sets up a message history where `read_file("src/lib.rs")` in turn 1 is followed by `patch_file("src/lib.rs")` in turn 2. Verifies turn 1's tool result is replaced with `[read_file: src/lib.rs (superseded by modification. Use retrieve_observation(id="ccr_..."))]`.
    - `test_recent_turn_preserved_uncompacted`: verifies that if the read is in the most recent turn, it is not compacted even if previously modified.

- [ ] **Step 2: Run targeted test to verify it fails**
  - Run: `cargo test -j 1 --lib context::budget::micro_compact::tests`
  - Expected: FAIL with module/struct not found.

- [ ] **Step 3: Implement `MicroCompactor` core and register in `src/context/budget/mod.rs`**
  - Implement `extract_target_path` inspecting `tool_call.arguments` object for dynamic file path keys.
  - Implement two-pass scanner:
    - Pass 1: Collect all modified files across history with message indices.
    - Pass 2: For each tool result message prior to `preserve_recent_turns`, if it was a file read for a file that was modified later, store the raw output in `CcrCache::store` and replace with a concise receipt.
  - Export `pub mod micro_compact;` and `pub use micro_compact::{MicroCompactMetrics, MicroCompactor};` in `src/context/budget/mod.rs`.

- [ ] **Step 4: Run targeted test to verify it passes**
  - Run: `cargo test -j 1 --lib context::budget::micro_compact::tests`
  - Expected: PASS

- [ ] **Step 5: Git commit**
  - `git add src/context/budget/micro_compact.rs src/context/budget/mod.rs`
  - `git commit -m "feat(budget): implement MicroCompactor core with superseded read detection"`

---

### Task 2: Duplicate Read & Historical Mutation Echo Condensation

**Files:**
- Modify: `src/context/budget/micro_compact.rs`
- Test: `src/context/budget/micro_compact.rs` (inline unit tests)

**Interfaces:**
- Produces:
  - Detection of duplicate consecutive reads of the same file path where the earlier reads add zero new information.
  - Condensation of large mutation tool results (e.g. `write_file`, `patch_file` echoing full 500-line files or large unified diffs) when older than `preserve_recent_turns` into `[write_file: <path> (successfully applied, <len> bytes. CCR: <id>)]`.

- [ ] **Step 1: Write the failing unit tests for duplicate reads and mutation echo condensation**
  - `test_duplicate_consecutive_reads`: read `src/main.rs` in turn 1 and read `src/main.rs` again in turn 3 without edits. Turn 1 should be condensed as duplicate.
  - `test_historical_mutation_echo_condensation`: `write_file("src/large.rs")` returning 200 lines in turn 1 followed by turn 2 and turn 3. Turn 1 mutation output should be condensed to a 1-line receipt with CCR id.

- [ ] **Step 2: Run targeted test to verify it fails**
  - Run: `cargo test -j 1 --lib context::budget::micro_compact::tests`
  - Expected: FAIL

- [ ] **Step 3: Implement duplicate read detection & mutation echo compaction**
  - Track read versions per path. When a path is re-read without modification, condense older read outputs.
  - For tool results where `tool_name` is in `["write_file", "patch_file", "replace_file_content", "edit_file"]` and `content.len() > 256`, if the message index is older than `preserve_recent_turns`, store full output in `CcrCache` and replace with a structured 1-line receipt.

- [ ] **Step 4: Run targeted test to verify it passes**
  - Run: `cargo test -j 1 --lib context::budget::micro_compact::tests`
  - Expected: PASS

- [ ] **Step 5: Git commit**
  - `git add src/context/budget/micro_compact.rs`
  - `git commit -m "feat(budget): add duplicate read and historical mutation echo compaction"`

---

### Task 3: Bulky Search/Grep Result Condensation

**Files:**
- Modify: `src/context/budget/micro_compact.rs`
- Test: `src/context/budget/micro_compact.rs` (inline unit tests)

**Interfaces:**
- Produces:
  - Detection of historical search outputs (`grep_search`, `find_by_name`, `file_search`, `glob`) exceeding 25 lines.
  - If a search is older than `preserve_recent_turns`, condense to `[<tool>: query "<query>" returned <count> lines. Stored in CCR cache: <id>]`.

- [ ] **Step 1: Write the failing unit test for search condensation**
  - `test_historical_search_result_condensation`: `grep_search("AuthService")` returning 80 lines in turn 1 followed by turn 2. Verifies turn 1 is condensed into a concise summary with query, line count, and CCR id.

- [ ] **Step 2: Run targeted test to verify it fails**
  - Run: `cargo test -j 1 --lib context::budget::micro_compact::tests`
  - Expected: FAIL

- [ ] **Step 3: Implement search query extraction and search condensation**
  - Support extracting queries from `query`, `pattern`, `term`, `regex`.
  - Condense search outputs > 25 lines when outside the preserved recent turn window.

- [ ] **Step 4: Run targeted test to verify it passes**
  - Run: `cargo test -j 1 --lib context::budget::micro_compact::tests`
  - Expected: PASS

- [ ] **Step 5: Git commit**
  - `git add src/context/budget/micro_compact.rs`
  - `git commit -m "feat(budget): condense historical search and grep observations"`

---

### Task 4: Wire Micro-Compactor into `AgentLoop` Lifecycle

**Files:**
- Modify: `src/agent/loop.rs`
- Test: `tests/integration_micro_compaction.rs`

**Interfaces:**
- Invokes `MicroCompactor::compact_messages(&mut self.messages, 2)`:
  1. At turn start in `AgentLoop::execute_turn` before calling LLM.
  2. Immediately following successful file mutation tool execution (`write_file`, `patch_file`, `replace_file_content`).
- Emits tracing telemetry (`tracing::debug!`) when tokens are saved.

- [ ] **Step 1: Write integration test `tests/integration_micro_compaction.rs`**
  - Test end-to-end flow:
    - Create synthetic multi-turn conversation with `read_file`, `write_file`, `grep_search`.
    - Run `MicroCompactor::compact_messages`.
    - Assert superseded `read_file` is condensed.
    - Assert `CcrCache::retrieve` successfully recovers the exact original text.
    - Assert `preserve_recent_turns = 1` keeps the newest tool result completely untouched.

- [ ] **Step 2: Run integration test to verify it fails/compiles**
  - Run: `cargo test -j 1 --test integration_micro_compaction`
  - Expected: Initial compilation check.

- [ ] **Step 3: Wire into `src/agent/loop.rs`**
  - In `execute_turn`: invoke `let micro_metrics = MicroCompactor::compact_messages(&mut self.messages, 2);` before context budgeting.
  - If `micro_metrics.tokens_saved_estimate > 0`, log telemetry: `tracing::info!(tokens_saved = micro_metrics.tokens_saved_estimate, "Applied semantic micro-compaction")`.

- [ ] **Step 4: Run integration test to verify it passes**
  - Run: `cargo test -j 1 --test integration_micro_compaction`
  - Expected: PASS

- [ ] **Step 5: Git commit**
  - `git add src/agent/loop.rs tests/integration_micro_compaction.rs`
  - `git commit -m "feat(agent): wire semantic micro-compactor into agent loop execution lifecycle"`

---

### Task 5: Quality Gates, Version Bump (`v0.3.32`), and Real-World Autonomous Verification

**Files:**
- Modify: `Cargo.toml`
- Modify: `onpkg_docs/todo.md`
- Test: All targeted tests, clippy, fmt, and live autonomous validation

- [ ] **Step 1: Run all targeted tests for modified modules**
  - `cargo test -j 1 --lib context::budget::micro_compact::tests`
  - `cargo test -j 1 --test integration_micro_compaction`
  - `cargo test -j 1 --test integration_intent_routing`
  - `cargo test -j 1 --test integration_context_modal`

- [ ] **Step 2: Run clippy and format checks**
  - `cargo clippy -j 1 --bin minicode -- -D warnings`
  - `cargo fmt --check`

- [ ] **Step 3: Bump version to `0.3.32` and update `onpkg_docs/todo.md`**
  - Update `Cargo.toml` version to `0.3.32`.
  - Record Phase 131 in `onpkg_docs/todo.md`.

- [ ] **Step 4: Recompile release binary via `./localupdate.sh`**
  - Run: `./localupdate.sh`
  - Confirm `minicode --version` reports `v0.3.32`.

- [ ] **Step 5: Real-World Autonomous Execution Test**
  - In `test_playground/realworld_micro_compact_test`, run an autonomous multi-turn task with minicode where a file is read, modified, and verified.
  - Confirm via output or logs that the superseded read is micro-compacted without breaking the agent loop.

- [ ] **Step 6: Final Git Commit**
  - `git add Cargo.toml Cargo.lock onpkg_docs/todo.md`
  - `git commit -m "release(v0.3.32): deliver semantic micro-compaction of tool observations (Phase 131)"`

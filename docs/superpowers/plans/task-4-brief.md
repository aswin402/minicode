# Task 4 Brief: Wire Micro-Compactor into AgentLoop Lifecycle & Integration Testing

## Overview
Wire `MicroCompactor` into `src/agent/loop.rs` during the turn execution lifecycle, and create a comprehensive integration test suite in `tests/integration_micro_compaction.rs`.

## Files
- Modify: `src/agent/loop.rs`
- Create: `tests/integration_micro_compaction.rs`

## Constraints
1. **Targeted Tests ONLY**:
   - `cargo test -j 1 --test integration_micro_compaction`
   - `cargo test -j 1 --lib context::budget::micro_compact::tests`
   NEVER run the full test suite.
2. **Resource limits**: `-j 1` on cargo check/test, `-j 2` on build.
3. **Pure Rust**: Zero non-test `.unwrap()` or `.expect()`.
4. **Preserve active turn**: Pass `preserve_recent_turns = 2` (or at least 1) so the current turn's tool observations are never altered.
5. **Lossless retrieval**: Verify all compacted observations are retrievable via `CcrCache::retrieve`.

## Detailed Requirements

### 1. Wiring into `src/agent/loop.rs`
- In `AgentLoop::execute_turn`:
  - Before building recency context / prompt (around line 374 before or alongside `self.prune_context()`):
    ```rust
    let micro_metrics = crate::context::budget::MicroCompactor::compact_messages(&mut self.messages, 2);
    if micro_metrics.tokens_saved_estimate > 0 {
        tracing::info!(
            superseded_reads = micro_metrics.superseded_reads_compacted,
            duplicate_reads = micro_metrics.duplicate_reads_compacted,
            mutation_echoes = micro_metrics.mutation_echoes_compacted,
            search_results = micro_metrics.search_results_compacted,
            tokens_saved = micro_metrics.tokens_saved_estimate,
            "Applied semantic micro-compaction to conversation history"
        );
    }
    ```
  - Inside the tool execution loop, after executing a file mutation tool (`write_file`, `patch_file`, `replace_file_content`):
    If the mutation succeeded, trigger `MicroCompactor::compact_messages(&mut self.messages, 2);` so that any prior `read_file` for that file is immediately compacted before the next model call in that turn.

### 2. Integration Test (`tests/integration_micro_compaction.rs`)
Write comprehensive integration tests:
1. `test_end_to_end_multi_turn_micro_compaction`:
   - Simulate a realistic 4-turn coding conversation:
     - Turn 1: `read_file` on `src/service.rs` (300 lines of code)
     - Turn 2: `grep_search` for `handle_request` (50 lines of matches)
     - Turn 3: `patch_file` on `src/service.rs` (succeeds)
     - Turn 4: `cargo test` in active turn
   - Run `MicroCompactor::compact_messages(&mut messages, 1)`.
   - Assert Turn 1 `read_file` is condensed as superseded by modification, and points to a `ccr_` ID.
   - Assert Turn 2 `grep_search` is condensed as historical search, and points to a `ccr_` ID.
   - Assert Turn 4 `cargo test` is in the active turn and is 100% UNTOUCHED.
   - Assert `CcrCache::retrieve` on both CCR IDs recovers the exact raw text verbatim.
2. `test_multi_file_mutation_and_selective_compaction`:
   - Read `file_a.rs` and `file_b.rs`.
   - Modify only `file_a.rs`.
   - Run `MicroCompactor::compact_messages(&mut messages, 1)`.
   - Assert `file_a.rs` read is compacted (superseded).
   - Assert `file_b.rs` read is NOT compacted (since it was never modified).
3. `test_cumulative_tokens_saved_metric`:
   - Verify `metrics.tokens_saved_estimate > 0` and matches expected formula `(raw_len - receipt_len) / 4`.

## Success Criteria
- Targeted integration tests pass: `cargo test -j 1 --test integration_micro_compaction`
- Unit tests pass: `cargo test -j 1 --lib context::budget::micro_compact::tests`
- Clippy passes: `cargo clippy -j 1 --bin minicode -- -D warnings`
- Code formatting passes: `cargo fmt --check`
- Commit with message: `feat(agent): wire semantic micro-compactor into agent loop execution lifecycle`

# Task 3 Brief: Bulky Search/Grep Result Condensation

## Overview
Extend `MicroCompactor` in `src/context/budget/micro_compact.rs` to detect and condense historical bulky search and grep observations (>25 lines or >300 bytes) outside the preserved recent turn window, and implement the Task 2 Reviewer suggestions (zero-allocation receipt prefix checking and shared backward scan helper).

## Files
- Modify: `src/context/budget/micro_compact.rs`
- Test: `src/context/budget/micro_compact.rs` (inline unit tests)

## Constraints
1. **Targeted Tests ONLY**: `cargo test -j 1 --lib context::budget::micro_compact::tests`. NEVER run the full test suite.
2. **Resource limits**: `-j 1` on cargo check/test, `-j 2` on build.
3. **Pure Rust**: Zero non-test `.unwrap()` or `.expect()`.
4. **Lossless CCR**: Use `CcrCache::store(&raw_output)` before replacing content.
5. **Preserve recent turns**: If `preserve_recent_turns > 0`, the tool results belonging to the last `preserve_recent_turns` turns MUST remain 100% untouched.

## Detailed Requirements

### 1. Task 2 Reviewer Refinements
- **Zero-allocation receipt prefix check**:
  Instead of allocating temporary formatted strings, check if a message starts with `[` followed by the tool name and `:` without heap allocations:
  ```rust
  fn is_already_receipt(content: &str, tool_name: &str) -> bool {
      content
          .strip_prefix('[')
          .and_then(|s| s.strip_prefix(tool_name))
          .map(|s| s.starts_with(':'))
          .unwrap_or(false)
  }
  ```
- **Helper for Backward Resolution**:
  Deduplicate the backwards search for tool call metadata into a helper function:
  `resolve_tool_call_meta(messages: &[Message], idx: usize, tool_meta_by_id: &HashMap<String, CompactToolMeta>) -> Option<CompactToolMeta>`

### 2. Search & Grep Observation Condensation
- For each tool result message in `messages` where:
  - `idx < cutoff` (outside the preserved recent turn window).
  - `tool_name` is in `["grep_search", "find_by_name", "file_search", "glob", "grep", "search"]` or contains `grep` or `search`.
  - Not already a receipt (`!is_already_receipt(&msg.content, &tname)`).
  - Line count `lines().count() > 25` OR byte length `content.len() > 300`:
    - Extract query using `meta.query.as_deref().unwrap_or("...")`.
    - Store full raw search output in `CcrCache::store(&msg.content)`.
    - Replace `msg.content` with:
      `format!("[{}: query \"{}\" returned {} lines. Use retrieve_observation(id=\"{}\") for full matches]", tname, query, line_count, ccr_id)`
    - Increment `metrics.search_results_compacted`.
    - Add estimated saved tokens (`(raw_len - receipt_len) / 4`) to `metrics.tokens_saved_estimate`.

## Unit Tests to Implement
1. `test_historical_search_result_condensation`:
   - Turn 1: `grep_search(query="AuthService")` returning 80 lines of matches (>1000 bytes).
   - Turn 2: User prompt + assistant action.
   - Turn 3: Recent turn.
   - Run `MicroCompactor::compact_messages(&mut messages, 1)`.
   - Assert Turn 1 is condensed to `[grep_search: query "AuthService" returned 80 lines. Use retrieve_observation(id="ccr_...")]`.
   - Assert raw 80 lines is losslessly retrievable via `CcrCache::retrieve`.
   - Assert `metrics.search_results_compacted == 1`.
2. `test_recent_search_result_uncompacted`:
   - Search in the most recent turn remains uncompacted.
3. `test_small_search_result_uncompacted`:
   - A search result with only 3 lines / 100 bytes is not condensed (negative compression prevention).

## Success Criteria
- Targeted test passes: `cargo test -j 1 --lib context::budget::micro_compact::tests`
- Clippy passes: `cargo clippy -j 1 --bin minicode -- -D warnings`
- Code formatting passes: `cargo fmt --check`
- Commit with message: `feat(budget): condense historical search and grep observations`

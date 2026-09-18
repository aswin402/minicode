# Task 2 Brief: Duplicate Read & Historical Mutation Echo Condensation

## Overview
Extend `MicroCompactor` in `src/context/budget/micro_compact.rs` to detect and condense duplicate consecutive reads and historical large mutation echoes (>2 turns old), while also implementing the Reviewer's optimization suggestions (lightweight metadata, cross-platform path normalization, and preventing negative compression).

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

### 1. Address Task 1 Reviewer Suggestions
- **Lightweight Tool Metadata**:
  Do NOT clone full `ToolCall` structs (which contain heavy `arguments: serde_json::Value`). Instead define a lightweight internal struct:
  ```rust
  #[derive(Debug, Clone)]
  struct CompactToolMeta {
      name: String,
      target_path: Option<String>,
      query: Option<String>,
  }
  ```
  Extract `target_path` and `query` once during Pass 1 and store only `CompactToolMeta`.
- **Cross-Platform Path Normalization**:
  In `normalize_path_for_compare(path: &str) -> String`:
  Replace all `\\` with `/`, strip leading `./`, and trim leading `/` so Windows and Unix paths compare identically.
- **Prevent Negative Compression**:
  Never replace a tool output if `msg.content.len() <= 128` (or if raw content is shorter than the receipt template), because replacing a 30-byte observation with a 130-byte receipt would inflate the context window!

### 2. Duplicate Read Condensation
- If a file `path` was read in message $i$, and read again in message $j > i$ WITHOUT any modifying tool call (`write_file`, `patch_file`, etc.) between $i$ and $j$:
  - Message $i$ is redundant (message $j$ has the same or fresher content).
  - If message $i$ is outside the preserved recent turn window and longer than 128 bytes:
    - Store message $i$'s raw content in `CcrCache::store`.
    - Replace message $i$'s content with:
      `format!("[read_file: {} (superseded by subsequent read. Use retrieve_observation(id=\"{}\") for raw content)]", path, ccr_id)`
    - Increment `metrics.duplicate_reads_compacted`.
    - Estimate tokens saved and add to `metrics.tokens_saved_estimate`.

### 3. Historical Mutation Echo Condensation
- When a file writing tool (`write_file`, `patch_file`, `replace_file_content`, `edit_file`) executes, its tool output often contains a large diff, the echoed new content, or verbose confirmation (> 256 bytes).
- For such mutation tool results that are outside the preserved recent turn window:
  - If the output is already a condensed receipt (`starts_with("[write_file:")` or `starts_with("[patch_file:")`), skip.
  - If `content.len() > 256`:
    - Store raw output in `CcrCache::store`.
    - Replace with:
      `format!("[{}: {} (successfully applied, {} bytes. Use retrieve_observation(id=\"{}\") for details)]", tool_name, path, content.len(), ccr_id)`
    - Increment `metrics.mutation_echoes_compacted`.
    - Add saved tokens to `metrics.tokens_saved_estimate`.

## Unit Tests to Implement
1. `test_duplicate_consecutive_reads`:
   - Turn 1: `read_file("src/main.rs")` (50 lines)
   - Turn 2: User asks question, assistant answers
   - Turn 3: `read_file("src/main.rs")` (50 lines)
   - Turn 4: Recent turn
   - Run `MicroCompactor::compact_messages(&mut messages, 1)`.
   - Assert Turn 1 read is condensed with `superseded by subsequent read`.
   - Assert Turn 3 read is preserved (or preserved because within recent window).
2. `test_historical_mutation_echo_condensation`:
   - Turn 1: `write_file("src/main.rs")` returning 500 bytes of unified diff / echo.
   - Turn 2: User prompt + assistant action
   - Turn 3: Recent turn
   - Run `MicroCompactor::compact_messages(&mut messages, 1)`.
   - Assert Turn 1 mutation output is condensed to `[write_file: src/main.rs (successfully applied, 500 bytes...)]`.
   - Assert raw 500 bytes is retrievable via `CcrCache::retrieve`.
3. `test_no_negative_compression_on_tiny_output`:
   - A tool result of `"ok"` or 20 bytes is NOT replaced with a 130-byte receipt.
4. `test_cross_platform_path_normalization`:
   - `read_file` with `"src\\main.rs"` is recognized as superseded by `write_file` with `"src/main.rs"`.

## Success Criteria
- Targeted test passes: `cargo test -j 1 --lib context::budget::micro_compact::tests`
- Clippy passes: `cargo clippy -j 1 --bin minicode -- -D warnings`
- Code formatting passes: `cargo fmt --check`
- Commit with message: `feat(budget): add duplicate read and historical mutation echo compaction`

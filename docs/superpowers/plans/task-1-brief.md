# Task 1 Brief: MicroCompactor Engine & Superseded File Read Detection

## Overview
Implement the core `MicroCompactor` struct and data structures in `src/context/budget/micro_compact.rs`, register it in `src/context/budget/mod.rs`, and implement the detection and condensation of superseded file reads backed by `CcrCache`.

## Files
- Create: `src/context/budget/micro_compact.rs`
- Modify: `src/context/budget/mod.rs`
- Test: `src/context/budget/micro_compact.rs` (inline unit tests)

## Constraints
1. **Targeted Tests ONLY**: `cargo test -j 1 --lib context::budget::micro_compact::tests`. NEVER run the full test suite.
2. **Resource limits**: `-j 1` on cargo check/test, `-j 2` on build.
3. **Pure Rust**: Zero non-test `.unwrap()` or `.expect()`.
4. **Error handling**: Return `Option` / `Result` where appropriate.
5. **Lossless CCR**: Use `crate::context::budget::ccr_cache::CcrCache::store(&raw_output)` to store the exact raw output before replacing it with a concise 1-line receipt:
   `[read_file: <path> (<lines> lines read, superseded by modification. Use retrieve_observation(id="<ccr_id>") for raw content)]`
6. **Recent Turn Preservation**: If `preserve_recent_turns > 0`, the tool results belonging to the last `preserve_recent_turns` turns MUST be left 100% untouched.

## Interfaces
```rust
use crate::agent::types::{Message, Role, ToolCall};
use crate::context::budget::ccr_cache::CcrCache;
use serde::{Deserialize, Serialize};

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

## Implementation Requirements
1. `extract_target_path(tool_call: &ToolCall) -> Option<String>`:
   Check `tool_call.arguments` (JSON Object) dynamically for keys: `path`, `target_file`, `file_path`, `file`, `TargetFile`, `AbsolutePath`, `target`. Return cleaned trimmed path string.
2. `extract_search_query(tool_call: &ToolCall) -> Option<String>`:
   Check `tool_call.arguments` for keys: `query`, `pattern`, `term`, `regex`, `Query`, `Pattern`. Return cleaned trimmed query string.
3. Turn boundaries in `messages`:
   A "turn" can be demarcated by user messages (`role == Role::User`) or by assistant messages. Count user messages or distinct assistant-user rounds to identify turn indices. The messages belonging to the last `preserve_recent_turns` turns must not have their tool results modified.
4. Pass 1: Build map of modified files across the conversation:
   - Identify all tool calls where `tool_name` is in `["write_file", "patch_file", "replace_file_content", "edit_file"]` or starts with `write_` / `patch_` / `edit_`.
   - Extract the target file path and record the message index of the mutation.
5. Pass 2: For each tool result message (`role == Role::Tool`) outside the preserved recent window:
   - If `tool_name` is in `["read_file", "view_file", "cat"]` or starts with `read_` / `view_`:
     - If the target file was modified in a later message:
       - Compute line count of raw content.
       - If raw content is already a condensed receipt (`starts_with("[read_file:")`), skip.
       - Store raw content in `CcrCache::store(&msg.content)`.
       - Estimate tokens saved (~1 token per 4 chars).
       - Replace `msg.content` with receipt:
         `format!("[read_file: {} ({} lines read, superseded by modification. Use retrieve_observation(id=\"{}\") for raw content)]", path, line_count, ccr_id)`
       - Increment `metrics.superseded_reads_compacted`.

## Unit Tests to Write
1. `test_extract_target_path_various_schemas`:
   Test with `{"path": "foo.rs"}`, `{"target_file": "bar.rs"}`, `{"file_path": "baz.rs"}`, `{"TargetFile": "qux.rs"}`.
2. `test_superseded_file_read_compaction`:
   Message 0: User "Edit main.rs"
   Message 1: Assistant calls `read_file(path="src/main.rs")`
   Message 2: Tool result with 50 lines of code
   Message 3: Assistant calls `write_file(path="src/main.rs")`
   Message 4: Tool result "ok"
   Message 5: User "Now test it"
   Message 6: Assistant "Testing"
   Run `MicroCompactor::compact_messages(&mut messages, 1)`.
   Assert Message 2 was replaced with receipt containing `[read_file: src/main.rs` and `ccr_`.
   Assert `CcrCache::retrieve(&ccr_id, None, None)` returns the original 50 lines.
3. `test_recent_turn_preserved_uncompacted`:
   Ensure tool result in the most recent turn is never touched even if the file was modified in an earlier turn.

## Success Criteria
- Targeted test passes: `cargo test -j 1 --lib context::budget::micro_compact::tests`
- Clippy passes: `cargo clippy -j 1 --bin minicode -- -D warnings`
- Code formatting passes: `cargo fmt --check`
- Commit with message: `feat(budget): implement MicroCompactor core with superseded read detection`

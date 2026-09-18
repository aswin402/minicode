# Task 2 Brief: Dynamic, Non-Hardcoded Requirement Extractor

## Goal
Implement a robust, dynamic requirement extractor `IntentLedger::from_prompt(prompt: &str, max_items: usize) -> IntentLedger` in `src/context/memory/intent.rs` that automatically parses any user prompt into a root objective and actionable requirement items without any hardcoding. Also incorporate the minor suggestions from the Task 1 review.

## Files to Touch
- Modify: `src/context/memory/intent.rs`
- Test: `src/context/memory/intent.rs` (inline `mod tests`)

## Detailed Requirements

### 1. Dynamic Requirement Extraction Heuristics (NO hardcoded benchmark names!)
Implement:
```rust
impl IntentLedger {
    /// Dynamically extracts the root objective and actionable requirement items
    /// from a freeform user prompt across diverse formatting styles (headers, checklists, numbered lists, bullet points).
    pub fn from_prompt(prompt: &str, max_items: usize) -> Self
```

The algorithm must handle:
1. **Root Objective Extraction:**
   - Scan for the first non-empty line or header (e.g. `# ...` or `Build a ...`).
   - Clean markdown formatting and strip outer delimiters.
   - If prompt is empty/whitespace, fall back to "General Assistance".
2. **Requirement Item Parsing:**
   - Detect sections and items via line prefixes:
     - Markdown sub-headers: `### `, `#### `, `## `
     - Checkbox items: `- [ ]`, `* [ ]`, `+ [ ]`, `- [x]`, `* [x]`
     - Numbered list items: `1. `, `2. `, `1) `, `2) `
     - Bullet list items: `* `, `- `, `+ `
   - When a header like `#### 1. Dashboard` is found, set title = "Dashboard" and collect subsequent description lines (e.g. "Show: Total customers, open tickets...") into `item.description`.
   - When a bullet or numbered line is found, use it as title/description.
   - **Related Files Extraction:**
     - Scan item title and description for file paths or code extensions (`.rs`, `.ts`, `.tsx`, `.js`, `.jsx`, `.py`, `.json`, `.toml`, `.css`, `src/...`, `/...`).
     - Populate `related_files` dynamically.
   - **Deduplication & Capping:**
     - Deduplicate items with identical or sub-string titles.
     - Cap to `max_items` (default 32).
   - **Single-Sentence / Short Prompt Handling:**
     - If no explicit items or sub-sections are found in the prompt, synthesize a single requirement item matching the root objective (e.g. "Complete objective: <root_objective>").

### 2. Helper & Reviewer Improvements
- Add `pub fn get_item_mut(&mut self, id: &str) -> Option<&mut RequirementItem>`.
- In `set_status`: Only reset `consecutive_turns_without_progress = 0` if transitioning to `Completed` from a non-`Completed` state (or from `Pending` to `InProgress`).
- In `save_to_disk`: If `std::fs::remove_file(&tmp_path)` fails in error cleanup, log a warning: `tracing::warn!(error = %e, path = %tmp_path.display(), "Failed to clean up temporary intent ledger file");`.

### 3. Verification Constraints
- Strict: ONLY run targeted test: `cargo test -j 1 --lib context::memory::intent::tests`
- Zero clippy warnings: `cargo clippy -j 1 --bin minicode -- -D warnings`
- Zero non-test unwraps, pure safe Rust, clean formatting.
- TDD: write tests covering markdown headers, checkboxes, numbered lists, single-sentence prompts, and file extraction.

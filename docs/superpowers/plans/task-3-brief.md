# Task 3 Brief: Living Execution Ledger Prompt Rendering & Drift Detection

## Goal
Implement compact, token-efficient prompt rendering (`to_prompt_block`), drift detection (`check_drift`), and progress counting (`completed_count`, `total_count`) in `IntentLedger`, and wire `IntentLedger` into `PromptBuilder::build_recency_context` in `src/agent/prompt.rs`.

## Files to Touch
- Modify: `src/context/memory/intent.rs`
- Modify: `src/agent/prompt.rs`
- Test: `src/context/memory/intent.rs` (inline `mod tests`)

## Detailed Requirements

### 1. Methods in `src/context/memory/intent.rs`
Implement:
```rust
impl IntentLedger {
    /// Returns the number of completed requirement items.
    pub fn completed_count(&self) -> usize { ... }

    /// Returns the total number of tracked requirement items.
    pub fn total_count(&self) -> usize { ... }

    /// Checks if consecutive turns without progress exceed the drift threshold.
    /// If so, returns a high-priority course-correction reminder string.
    pub fn check_drift(&self, threshold: usize) -> Option<String> { ... }

    /// Renders the immutable root goal anchor and the living execution ledger
    /// into a compact, token-efficient XML prompt block for LLM context injection.
    pub fn to_prompt_block(&self) -> String { ... }
}
```

Format of `to_prompt_block`:
```xml
  <goal_anchor>
    <root_objective>Build AgentBench — a sandbox website designed specifically for testing AI agents.</root_objective>
  </goal_anchor>
  <execution_ledger progress="2/7 completed">
    [x] Dashboard (metrics cards, activity feed)
    [x] Mock Database & Schemas
    [-] Customers Page (search, filter, pagination)
    [ ] Support Tickets (ticket submission, status badges)
    [ ] Orders Management (order table, detail modal)
    [ ] Analytics & Reports (revenue graphs)
    [ ] Settings & Profile (theme toggle, preferences)
  </execution_ledger>
```
Status markers:
- `Completed`: `[x]`
- `InProgress`: `[-]`
- `Blocked`: `[!]`
- `Skipped`: `[s]`
- `Pending`: `[ ]`

Format of `check_drift`:
If `self.consecutive_turns_without_progress >= threshold && self.total_count() > self.completed_count()`:
Returns:
`format!("⚠️ Task Drift Warning: {} turns have passed without requirement progress (completed {}/{}). Active task: {}. Do not get distracted by tangential edits; focus on completing remaining requirements.", self.consecutive_turns_without_progress, self.completed_count(), self.total_count(), active_title)`

### 2. Wiring into `src/agent/prompt.rs`
In `PromptBuilder::build_recency_context`:
Add `intent_ledger: Option<&crate::context::memory::intent::IntentLedger>` parameter (or handle `None` gracefully).
Inject the `<goal_anchor>` and `<execution_ledger>` right into Zone 1 of recency context (e.g. after working set or alongside working memory), and if `intent_ledger.and_then(|l| l.check_drift(drift_threshold))` yields a warning, inject `<intent_focus>` at the tail of recency context!
Note: Also update any existing callers of `build_recency_context` in `src/agent/loop.rs` and `src/agent/prompt.rs` tests to pass `None` for now so everything compiles.

### 3. Verification Constraints
- Strict: ONLY run targeted test: `cargo test -j 1 --lib context::memory::intent::tests` and `cargo test -j 1 --lib agent::prompt::tests`.
- Zero clippy warnings: `cargo clippy -j 1 --bin minicode -- -D warnings`.
- Zero non-test unwraps, pure safe Rust, clean formatting.
- TDD: write unit tests for `to_prompt_block`, `check_drift`, and prompt builder injection.

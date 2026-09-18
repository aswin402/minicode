# Task 4 Brief: Autonomous Progress Tracking & Auto-Advance in Agent Loop

## Goal
Wire `IntentLedger` into `AgentLoop` in `src/agent/loop.rs` so that user goals and checklists are automatically initialized from user prompts, persisted to `.minicode/intent_anchor.json`, and autonomously advanced as files are created and modified during tool execution.

## Files to Touch
- Modify: `src/context/memory/intent.rs` (add `update_from_file_activity`)
- Modify: `src/agent/loop.rs` (wire `intent_ledger` state, initialization, recency context passing, file activity updates, turn-end drift increments, and disk persistence)
- Create: `tests/integration_intent_tracking.rs`

## Detailed Requirements

### 1. `src/context/memory/intent.rs`
Implement `update_from_file_activity`:
```rust
impl IntentLedger {
    /// Updates requirement item statuses based on file system actions taken by tools.
    /// If an item's related files match any created or modified files, advances its status.
    /// Returns the number of items whose status or activity was updated.
    pub fn update_from_file_activity(
        &mut self,
        files_created: &[String],
        files_modified: &[String],
        current_turn: usize,
    ) -> usize
```
Matching heuristic:
- Matches if `related_file == created_or_modified_file` OR if `related_file.ends_with(path)` OR `path.ends_with(related_file)` (normalizing slashes).
- If all related files for a requirement are touched, mark `Completed`. If at least one is touched and status was `Pending`, mark `InProgress`.
- If an item transitions to `Completed` or `InProgress`, reset `self.consecutive_turns_without_progress = 0`.
- Update `item.updated_turn = current_turn`.

Also implement:
```rust
impl IntentLedger {
    /// Loads an existing ledger from disk or creates a new one from prompt.
    pub fn load_or_create(workspace_dir: &std::path::Path, prompt: &str, max_items: usize) -> Self
}
```

### 2. `src/agent/loop.rs`
1. Add field to `AgentLoop`:
   ```rust
   intent_ledger: Option<crate::context::memory::intent::IntentLedger>,
   ```
2. In `AgentLoop::new`:
   Attempt to load existing ledger from `.minicode/intent_anchor.json` via `IntentLedger::load_from_disk`.
3. In `AgentLoop::execute_turn`:
   - If `self.config.agent.intent.enabled`:
     - If `self.intent_ledger.is_none()` && `self.config.agent.intent.auto_extract`:
       - `self.intent_ledger = Some(IntentLedger::from_prompt(user_prompt, self.config.agent.intent.max_ledger_items));`
       - Persist to disk using `.minicode/intent_anchor.json`.
     - Pass `self.intent_ledger.as_ref()` to `PromptBuilder::build_recency_context`.
4. In tool execution handling (after tool results are processed):
   - When `write_file`, `patch_file`, or file modifications occur:
     - If `let Some(ref mut ledger) = self.intent_ledger`:
       - Call `ledger.update_from_file_activity(&created_files, &modified_files, turn.turn_id)`.
       - Persist ledger to disk.
5. At turn end:
   - If `let Some(ref mut ledger) = self.intent_ledger`:
     - If no items progressed this turn, increment `ledger.consecutive_turns_without_progress += 1`.
     - Persist ledger to disk.
6. Provide public getter:
   ```rust
   pub fn intent_ledger(&self) -> Option<&crate::context::memory::intent::IntentLedger> {
       self.intent_ledger.as_ref()
   }
   pub fn intent_ledger_mut(&mut self) -> Option<&mut crate::context::memory::intent::IntentLedger> {
       self.intent_ledger.as_mut()
   }
   ```

### 3. Integration Test `tests/integration_intent_tracking.rs`
Write integration test with `MockProvider`:
- Initialize `AgentLoop` in a temp dir.
- Prompt: `"Build AgentBench\n#### 1. Dashboard\nCreate src/pages/Dashboard.tsx\n#### 2. Settings\nCreate src/pages/Settings.tsx"`.
- Mock response: invokes `write_file(path="src/pages/Dashboard.tsx", content="export const Dashboard = () => <div>Dashboard</div>;")`.
- Assert `agent.intent_ledger().is_some()`.
- Assert Dashboard requirement is marked `Completed`.
- Assert `.minicode/intent_anchor.json` was written to disk and can be deserialized.

### 4. Verification Constraints
- Strict: ONLY run targeted tests:
  `cargo test -j 1 --test integration_intent_tracking`
  `cargo test -j 1 --lib context::memory::intent::tests`
- Zero clippy warnings: `cargo clippy -j 1 --bin minicode -- -D warnings`.
- Zero non-test unwraps, pure safe Rust, clean formatting.

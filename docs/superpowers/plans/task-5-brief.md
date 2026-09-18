# Task 5 Brief: Interactive `/goal` and `/intent` Commands in TUI/CLI

## Goal
Implement rich interactive `/goal` and `/intent` slash command handling in `src/app/commands.rs` (and register in command catalog & help modal) so users can inspect the active goal and living execution ledger, add new requirements, mark items complete, reset the ledger, or run autonomous execution.

## Files to Touch
- Modify: `src/app/commands.rs`
- Modify: `src/ui/modals/command_catalog.rs`
- Modify: `src/ui/modals/help.rs`
- Test: `src/app/commands.rs` (inline `mod tests`)

## Detailed Requirements

### 1. Slash Command Handling in `src/app/commands.rs`
Handle both `/goal` and `/intent`:
- `/goal` or `/intent` (no arguments):
  Load the active `IntentLedger` from `.minicode/intent_anchor.json` in `self.workspace_root`.
  - If no ledger exists:
    `self.timeline.add_status("ℹ No active goal anchor found. Start a task or use /goal add <task>".to_string());`
  - If ledger exists:
    Format a clean, beautiful timeline message:
    ```
    🎯 Active Goal: <root_objective>
    📋 Living Execution Ledger (<completed>/<total> completed):
       [x] 1. Dashboard (metrics cards)
       [-] 2. Customers Page (search, filter)
       [ ] 3. Support Tickets
    💡 Commands: /goal add <task> | /goal done <index> | /goal reset | /goal run <prompt>
    ```
    Add to timeline with `self.timeline.add_status(...)` or `self.timeline.add_assistant_message(...)`.
- `/goal add <text>` or `/intent add <text>`:
  Loads or creates the ledger, calls `ledger.add_item(title, None, vec![])`, saves to disk, and displays confirmation in timeline.
- `/goal done <index_or_id>` or `/intent done <index_or_id>`:
  Loads the ledger. If argument is a number (1-based index `1..=items.len()`), resolve that item's ID.
  Call `ledger.set_status(&id, RequirementStatus::Completed)`. Save to disk.
  Display confirmation in timeline.
- `/goal reset` or `/intent reset`:
  If `.minicode/intent_anchor.json` exists, remove it and display confirmation in timeline.
- `/goal run <prompt>` or `/goal <freeform prompt>` (if not `add`, `done`, `reset`):
  Preserve autonomous execution behavior:
  Initializes/updates `IntentLedger` from the prompt, saves to disk, dispatches agent execution via `AgentCommand::Prompt`.

### 2. Update Catalog & Help Modal
In `src/ui/modals/command_catalog.rs`:
Update `/goal` entry:
- Category: "Agent & Automation"
- Shortcut: ""
- Description: "Inspect, manage, or execute the active Goal Anchor and Living Execution Ledger"
- Example: "/goal | /goal add <item> | /goal done <id> | /goal reset | /goal run <prompt>"
Add `/intent` alias.

In `src/ui/modals/help.rs`:
List `/goal, /intent` in the interactive commands help list.

### 3. Unit Tests in `src/app/commands.rs`
Add unit tests verifying:
- Parsing of `/goal`, `/goal add ...`, `/goal done 1`, `/goal reset`, `/goal run ...`.
- Parsing of `/intent` equivalents.

### 4. Verification Constraints
- Strict: ONLY run targeted test: `cargo test -j 1 --lib app::commands::tests`
- Zero clippy warnings: `cargo clippy -j 1 --bin minicode -- -D warnings`.
- Zero non-test unwraps, pure safe Rust, clean formatting.

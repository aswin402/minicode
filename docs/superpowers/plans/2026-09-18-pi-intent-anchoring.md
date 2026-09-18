# Pi/OhMyPi Intent Anchoring & Dynamic Execution Ledger Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a compaction-resistant Goal Anchor, dynamic Living Execution Ledger, and drift-prevention reminder system in minicode inspired by Pi, OhMyPi, Devin, and Claude Code to eliminate goal amnesia during multi-turn 20+ turn runs without any hardcoding.

**Architecture:** Implement a modular `IntentLedger` in `src/context/memory/intent.rs` that dynamically extracts and tracks requirements from any user prompt without hardcoded assumptions. Anchor the root objective and live checklist into Zone-1/Recency context (`src/agent/prompt.rs`) so it survives 4-tier context compaction, auto-advance task status based on file system modifications and test runs in `src/agent/loop.rs`, and expose interactive `/goal` inspection and override commands in the TUI/CLI.

**Tech Stack:** Rust (2021 edition), Tokio, Serde/Serde JSON, IndexMap, Tracing, Ratatui, Crossterm.

## Global Constraints
- **Targeted Testing ONLY:** Do NOT run full workspace `cargo test -j 1`. Always run targeted tests for the specific module being tested (`cargo test -j 1 --lib context::memory::intent::tests`).
- **No Hardcoding:** All extraction, requirement parsing, drift thresholds, and file tracking heuristics must be dynamic, configurable via `config.toml`, and generalizable across any benchmark or real-world prompt.
- **Error Handling:** Use `thiserror` for internal errors, `anyhow::Result` at CLI boundaries. Zero `.unwrap()` or `.expect()` in non-test code.
- **Logging:** Use `tracing` macros (`tracing::info!`, `tracing::debug!`). Never use `println!` in library code.
- **Concurrency & Compiles:** `-j 1` for `cargo check`/`cargo test`, `-j 2` for `cargo build`.

---

### Task 1: Data Structures and Configuration for Intent Tracking

**Files:**
- Create: `src/context/memory/intent.rs`
- Modify: `src/context/memory/mod.rs`
- Modify: `src/config.rs`
- Modify: `src/constants.rs`
- Test: `src/context/memory/intent.rs` (inline test module)

**Interfaces:**
- Consumes: `serde::{Serialize, Deserialize}`, `std::path::Path`
- Produces:
  - `RequirementStatus`: Enum (`Pending`, `InProgress`, `Completed`, `Blocked`, `Skipped`)
  - `RequirementItem`: Struct (`id`, `title`, `description`, `status`, `related_files`, `created_turn`, `updated_turn`)
  - `IntentLedger`: Struct (`root_objective`, `items`, `active_item_id`, `consecutive_turns_without_progress`)
  - `IntentConfig`: Struct in `src/config.rs` (`enabled`, `persistence_file`, `drift_warning_turns`, `auto_extract`, `max_ledger_items`)

- [ ] **Step 1: Define Intent constants in `src/constants.rs`**
Add configuration constants:
```rust
pub const DEFAULT_INTENT_PERSISTENCE_FILE: &str = ".minicode/intent_anchor.json";
pub const DEFAULT_INTENT_DRIFT_WARNING_TURNS: usize = 4;
pub const DEFAULT_INTENT_MAX_ITEMS: usize = 32;
```

- [ ] **Step 2: Add `IntentConfig` in `src/config.rs`**
Add `IntentConfig` to `AgentConfig` and `RawAgentConfig` with serde defaults and merging logic.

- [ ] **Step 3: Write failing unit test for `IntentLedger` serialization and state transitions in `src/context/memory/intent.rs`**
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_intent_ledger_lifecycle() {
        let mut ledger = IntentLedger::new("Build AgentBench sandbox website");
        assert_eq!(ledger.root_objective, "Build AgentBench sandbox website");
        assert_eq!(ledger.items.len(), 0);

        let id = ledger.add_item("Dashboard with metrics cards", Some("Render revenue and activity"), vec!["src/pages/Dashboard.tsx".to_string()]);
        assert_eq!(ledger.items.len(), 1);
        assert_eq!(ledger.items[0].status, RequirementStatus::Pending);

        ledger.set_status(&id, RequirementStatus::InProgress);
        assert_eq!(ledger.items[0].status, RequirementStatus::InProgress);
        assert_eq!(ledger.active_item_id, Some(id.clone()));

        ledger.set_status(&id, RequirementStatus::Completed);
        assert_eq!(ledger.items[0].status, RequirementStatus::Completed);
        assert_eq!(ledger.active_item_id, None);
    }
}
```

- [ ] **Step 4: Implement `RequirementStatus`, `RequirementItem`, and core `IntentLedger` methods in `src/context/memory/intent.rs`**
Implement constructor, getters, item management, and json serialization (`save_to_disk`, `load_from_disk`).

- [ ] **Step 5: Run targeted test to verify it passes**
Run: `cargo test -j 1 --lib context::memory::intent::tests::test_intent_ledger_lifecycle`
Expected: PASS

- [ ] **Step 6: Register module in `src/context/memory/mod.rs`**
Add `pub mod intent;` and re-export `pub use intent::*;`.

- [ ] **Step 7: Commit Task 1**
```bash
git add src/constants.rs src/config.rs src/context/memory/intent.rs src/context/memory/mod.rs
git commit -m "feat(intent): define IntentLedger data structures and configuration (Phase 128)"
```

---

### Task 2: Dynamic, Non-Hardcoded Requirement Extractor

**Files:**
- Modify: `src/context/memory/intent.rs`
- Test: `src/context/memory/intent.rs` (inline test module)

**Interfaces:**
- Consumes: `raw_prompt: &str`, `max_items: usize`
- Produces: `IntentLedger::from_prompt(prompt: &str, max_items: usize) -> IntentLedger`

- [ ] **Step 1: Write failing test for dynamic requirement extraction across diverse prompt formats**
Test across 3 distinct real-world formats:
1. Markdown headers and sub-items (like AgentBench: `### Pages \n #### 1. Dashboard \n #### 2. Customers`)
2. Markdown checkboxes (`- [ ] Setup database \n - [ ] Add API routes`)
3. Numbered lists (`1. Create models \n 2. Write tests \n 3. Build UI`)
4. Freeform single-sentence goal (`Refactor error handling to use thiserror`)
```rust
#[test]
fn test_dynamic_prompt_requirement_extraction() {
    let complex_prompt = r#"
Build a modern, realistic web application called AgentBench.
### Pages
#### 1. Dashboard
Show total customers, open tickets, monthly revenue.
#### 2. Customers
Customer list with search and filter.
#### 3. Tickets
Support ticket queue.
"#;
    let ledger = IntentLedger::from_prompt(complex_prompt, 16);
    assert_eq!(ledger.root_objective, "Build a modern, realistic web application called AgentBench.");
    assert!(ledger.items.len() >= 3);
    assert!(ledger.items.iter().any(|i| i.title.contains("Dashboard")));
    assert!(ledger.items.iter().any(|i| i.title.contains("Customers")));
    assert!(ledger.items.iter().any(|i| i.title.contains("Tickets")));
}
```

- [ ] **Step 2: Run test to verify it fails**
Run: `cargo test -j 1 --lib context::memory::intent::tests::test_dynamic_prompt_requirement_extraction`
Expected: FAIL (method not implemented)

- [ ] **Step 3: Implement dynamic parser in `IntentLedger::from_prompt`**
- Extract root objective: First meaningful sentence or header.
- Extract checklist items dynamically using line-by-line scanning:
  - Markdown headings (`###`, `####`, `##`)
  - Checklist markers (`- [ ]`, `* [ ]`)
  - Bullet points (`* `, `- `)
  - Numbered lists (`1. `, `2. `)
  - Heuristic keyword recognition for acceptance criteria (`Show:`, `Requirements:`, `Pages:`)
  - Deduplicate and normalize whitespace.
  - Limit to `max_items` (configurable).

- [ ] **Step 4: Run test to verify it passes**
Run: `cargo test -j 1 --lib context::memory::intent::tests::test_dynamic_prompt_requirement_extraction`
Expected: PASS

- [ ] **Step 5: Commit Task 2**
```bash
git add src/context/memory/intent.rs
git commit -m "feat(intent): implement dynamic requirement parser for diverse prompt structures"
```

---

### Task 3: Living Execution Ledger Prompt Rendering & Drift Detection

**Files:**
- Modify: `src/context/memory/intent.rs`
- Modify: `src/agent/prompt.rs`
- Test: `src/context/memory/intent.rs` (inline test module)

**Interfaces:**
- Consumes: `IntentLedger`, `drift_threshold: usize`
- Produces:
  - `IntentLedger::to_prompt_block(&self) -> String`
  - `IntentLedger::check_drift(&self, threshold: usize) -> Option<String>`
  - `PromptBuilder::build_recency_context(...)` modified to accept `Option<&IntentLedger>`

- [ ] **Step 1: Write failing test for prompt rendering and drift warning**
```rust
#[test]
fn test_intent_ledger_prompt_block_and_drift() {
    let mut ledger = IntentLedger::new("Implement authentication system");
    ledger.add_item("JWT token generation", None, vec!["src/auth/jwt.rs".to_string()]);
    ledger.add_item("Login endpoint", None, vec!["src/routes/login.rs".to_string()]);

    let block = ledger.to_prompt_block();
    assert!(block.contains("<goal_anchor>"));
    assert!(block.contains("Implement authentication system"));
    assert!(block.contains("<execution_ledger>"));
    assert!(block.contains("[ ]")); // pending items

    // Drift test: 5 turns with 0 completed items
    ledger.consecutive_turns_without_progress = 5;
    let warning = ledger.check_drift(4);
    assert!(warning.is_some());
    assert!(warning.unwrap().contains("⚠️ Task Drift Warning"));
}
```

- [ ] **Step 2: Run test to verify it fails**
Run: `cargo test -j 1 --lib context::memory::intent::tests::test_intent_ledger_prompt_block_and_drift`
Expected: FAIL

- [ ] **Step 3: Implement `to_prompt_block` and `check_drift` in `src/context/memory/intent.rs`**
Format as compact, token-efficient XML tags:
```xml
<goal_anchor>
Build AgentBench — a sandbox website designed specifically for testing AI agents.
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

- [ ] **Step 4: Wire `IntentLedger` into `src/agent/prompt.rs`**
In `PromptBuilder::build_recency_context`, place `<goal_anchor>` and `<execution_ledger>` in Zone-1 (right after progressive memory and working memory), and if drift is detected, append `<intent_focus>` to the tail.

- [ ] **Step 5: Run targeted test to verify it passes**
Run: `cargo test -j 1 --lib context::memory::intent::tests::test_intent_ledger_prompt_block_and_drift`
Expected: PASS

- [ ] **Step 6: Commit Task 3**
```bash
git add src/context/memory/intent.rs src/agent/prompt.rs
git commit -m "feat(intent): render compaction-resistant Goal Anchor and Execution Ledger into recency context"
```

---

### Task 4: Autonomous Progress Tracking & Auto-Advance in Agent Loop

**Files:**
- Modify: `src/agent/loop.rs`
- Test: `tests/integration_intent_tracking.rs`

**Interfaces:**
- Consumes: Tool execution events (`write_file`, `patch_file`, `exec_cmd`), `IntentLedger`
- Produces: Auto-advanced checklist statuses, disk persistence, and drift reset

- [ ] **Step 1: Write integration test for agent loop intent tracking in `tests/integration_intent_tracking.rs`**
Verify that:
1. Agent initializes ledger on turn 1 from user prompt.
2. When agent writes `src/pages/Dashboard.tsx`, the dashboard requirement automatically transitions to `InProgress` or `Completed`.
3. The `.minicode/intent_anchor.json` file is persisted to disk.
4. Subsequent turns maintain the anchor and checklist across turns.

- [ ] **Step 2: Run test to verify it fails**
Run: `cargo test -j 1 --test integration_intent_tracking`
Expected: FAIL

- [ ] **Step 3: Implement ledger lifecycle in `AgentLoop` (`src/agent/loop.rs`)**
- Add `intent_ledger: Option<IntentLedger>` field to `AgentLoop`.
- In `execute_turn`:
  - If `intent_ledger` is None and `config.agent.intent.enabled`, initialize via `IntentLedger::load_or_create(workspace_dir, user_prompt, max_items)`.
  - Pass `intent_ledger.as_ref()` into `PromptBuilder::build_recency_context`.
- In tool execution processor:
  - When `write_file` or `patch_file` succeeds, invoke `ledger.update_from_file_activity(&created_files, &modified_files)`.
  - When tests pass (`cargo test`, `npm test`, `pytest`), record verification evidence and mark in-progress items as `Completed`.
  - Reset `consecutive_turns_without_progress = 0` when any item progresses.
  - Persist ledger to disk asynchronously.

- [ ] **Step 4: Run targeted test to verify it passes**
Run: `cargo test -j 1 --test integration_intent_tracking`
Expected: PASS

- [ ] **Step 5: Commit Task 4**
```bash
git add src/agent/loop.rs tests/integration_intent_tracking.rs
git commit -m "feat(intent): auto-advance execution ledger from tool activity and persist session anchor"
```

---

### Task 5: Interactive `/goal` and `/intent` Commands in TUI/CLI

**Files:**
- Modify: `src/app/commands.rs`
- Modify: `src/app/mod.rs`
- Modify: `src/ui/modals/command_catalog.rs`
- Modify: `src/ui/modals/help.rs`
- Test: `src/app/commands.rs` (inline test module)

**Interfaces:**
- Consumes: `/goal`, `/intent`, `/goal add <item>`, `/goal done <id>`, `/goal reset`
- Produces: Formatted interactive ledger rendering in TUI timeline and state manipulation

- [ ] **Step 1: Write test for `/goal` command parsing in `src/app/commands.rs`**
```rust
#[test]
fn test_goal_command_parsing() {
    let cmd = parse_slash_command("/goal");
    assert_eq!(cmd, Some(SlashCommand::Goal(None)));

    let cmd_add = parse_slash_command("/goal add Implement dark mode toggle");
    assert_eq!(cmd_add, Some(SlashCommand::Goal(Some("add Implement dark mode toggle".to_string()))));
}
```

- [ ] **Step 2: Implement `/goal` and `/intent` handlers in `src/app/commands.rs` and `src/app/mod.rs`**
- Handle:
  - `/goal`: Renders active goal, completed count, progress bar, and list of items with status indicators.
  - `/goal add <title>`: Dynamically appends a new item to the active ledger.
  - `/goal done <index_or_id>`: Manually marks an item as completed.
  - `/goal reset`: Resets the ledger for a fresh goal.
- Register `/goal` in command catalog and help modal.

- [ ] **Step 3: Run targeted test to verify it passes**
Run: `cargo test -j 1 --lib app::commands::tests::test_goal_command_parsing`
Expected: PASS

- [ ] **Step 4: Commit Task 5**
```bash
git add src/app/commands.rs src/app/mod.rs src/ui/modals/command_catalog.rs src/ui/modals/help.rs
git commit -m "feat(ui): add interactive /goal and /intent commands with real-time checklist management"
```

---

### Task 6: Comprehensive Verification & Release (v0.3.29)

**Files:**
- Modify: `Cargo.toml`
- Modify: `onpkg_docs/todo.md`
- Run: `./localupdate.sh`

- [ ] **Step 1: Run targeted unit and integration tests**
Run: `cargo test -j 1 --lib context::memory::intent::tests`
Run: `cargo test -j 1 --test integration_intent_tracking`
Expected: All tests PASS.

- [ ] **Step 2: Check formatting and clippy**
Run: `cargo fmt --check`
Run: `cargo clippy -j 1 --bin minicode -- -D warnings`
Expected: Zero warnings, clean format.

- [ ] **Step 3: Bump version to `v0.3.29` and build global release**
Run: `./localupdate.sh`
Expected: Global release compiled with resource constraints and installed to `/home/aswin/.local/bin/minicode`.

- [ ] **Step 4: Commit and tag v0.3.29**
```bash
git commit -am "release: v0.3.29 with Pi/OhMyPi Intent Anchoring and Living Execution Ledger"
```

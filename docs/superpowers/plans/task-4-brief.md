# Task 4 Brief: AgentLoop Ingestion, ToolFilterMode Alignment & Real-Time Timeline Cards

## Scope & Objective
Wire the reactive A2A mailbox into `AgentLoop` so incoming messages from subagents are automatically ingested at turn start, extend `ToolFilterMode` in `src/config.rs` with `ReadOnly` and `Standard` variants for child subagents, format subagent progress cards in `src/ui/view.rs`, and write comprehensive integration tests in `tests/integration_subagent_delegation.rs`.

## Files to Create/Modify
- Modify: `src/config.rs` (extend `ToolFilterMode` with `ReadOnly` and `Standard` variants and string parsing)
- Modify: `src/agent/types.rs` (add `SubagentProgress` and `SubagentCompleted` variants to `AgentEvent` if not present)
- Modify: `src/agent/loop.rs` (wire `AgentMailbox` into `AgentLoop`, poll and inject unread messages into `self.messages` at start of `execute_turn`)
- Modify: `src/ui/view.rs` (ensure subagent progress and completion events render cleanly in the conversation timeline)
- Create: `tests/integration_subagent_delegation.rs`

## Specifications & Requirements

### 1. `src/config.rs`
- Add variants to `ToolFilterMode`:
  ```rust
  pub enum ToolFilterMode {
      #[default]
      Dynamic,
      CoreOnly,
      Full,
      ReadOnly,
      Standard,
  }
  ```
- In `Display` implementation:
  - `Self::ReadOnly => write!(f, "read_only")`
  - `Self::Standard => write!(f, "standard")`
- In `FromStr` implementation:
  - `"read_only" | "readonly"` => `Ok(Self::ReadOnly)`
  - `"standard"` => `Ok(Self::Standard)`
- In tool filtering logic (e.g. `ToolRegistry::filter_tools` or `agent/loop.rs`):
  - When `ToolFilterMode::ReadOnly` is active, keep only tools where `crate::tools::is_read_only(&schema.name)` is true.
  - When `ToolFilterMode::Standard` is active, filter out dangerous meta-tools or permit standard developer tools.

### 2. `src/agent/loop.rs`
- Add `mailbox: Option<crate::agent::subagent::mailbox::AgentMailbox>` to `AgentLoop`.
- In `AgentLoop::new(...)` or builder:
  - Initialize `mailbox = AgentMailbox::new(AgentId::parent(), &workspace_root.join(".minicode").join("agents").join("parent")).ok()`.
- In `AgentLoop::execute_turn`:
  - At the very beginning of the turn (before building recency context):
    ```rust
    if let Some(ref mb) = self.mailbox {
        if let Ok(unread) = mb.drain_unread() {
            for msg in unread {
                tracing::info!(from = %msg.sender.0, intent = ?msg.intent, "Ingested incoming A2A message");
                self.messages.push(Message::user(msg.format_for_prompt()));
            }
        }
    }
    ```

### 3. `src/ui/view.rs`
- Ensure `AgentEvent::SubagentProgress` and `SubagentCompleted` (or `TimelineEntry::SubagentSwarm`) format cleanly into timeline entries without crashing or misaligning ANSI boundaries.

### 4. Integration Tests in `tests/integration_subagent_delegation.rs`
Write comprehensive async integration tests:
1. `test_subagent_mailbox_drain_and_prompt_formatting`:
   - Creates an `AgentMailbox` for parent in a tempdir.
   - Posts a `TaskComplete` message from a subagent (`coder-1`).
   - Verifies `drain_unread()` retrieves the message.
   - Verifies `format_for_prompt()` contains the `<agent_message>` block with sender, intent, and message content.
2. `test_tool_filter_mode_parsing`:
   - Asserts `"read_only".parse::<ToolFilterMode>() == Ok(ToolFilterMode::ReadOnly)`.
   - Asserts `"standard".parse::<ToolFilterMode>() == Ok(ToolFilterMode::Standard)`.
3. `test_subagent_roles_and_workspace_modes`:
   - Asserts `SubagentRole::Scout.default_workspace_mode() == WorkspaceMode::Shared`.
   - Asserts `SubagentRole::Coder.default_workspace_mode() == WorkspaceMode::Worktree`.
   - Asserts `SubagentRole::Tester.default_workspace_mode() == WorkspaceMode::Worktree`.
   - Asserts `SubagentRole::Reviewer.default_workspace_mode() == WorkspaceMode::Shared`.

## Critical Constraints
1. ONLY run targeted tests:
   - `cargo test -j 1 --test integration_subagent_delegation`
   - `cargo test -j 1 --lib agent::subagent::orchestrator::tests`
   NEVER run the full test suite.
2. Error handling: zero `.unwrap()` or `.expect()` in non-test code.
3. Concurrency: Use `-j 1` for `cargo check` and `cargo test`.
4. Verification: `cargo fmt` and `cargo clippy -j 1 --bin minicode -- -D warnings`.
5. Commit message: `feat(agent): wire subagent event streaming and reactive mailbox injection into AgentLoop`.

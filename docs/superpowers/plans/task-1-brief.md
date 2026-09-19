# Task 1 Brief: Subagent Core Types & A2A Messaging Infrastructure

## Scope & Objective
Implement the foundational data structures and reactive file-backed mailbox for the multi-agent delegation runtime in `minicode`.

## Files to Create/Modify
- Create: `src/agent/subagent/types.rs`
- Create: `src/agent/subagent/message.rs`
- Create: `src/agent/subagent/mailbox.rs`
- Create: `src/agent/subagent/mod.rs`
- Modify: `src/agent/mod.rs` (expose `pub mod subagent;`)

## Specifications & Requirements

### 1. `src/agent/subagent/types.rs`
- `AgentId(pub String)`:
  - `parent() -> Self`: returns `AgentId("parent".to_string())`.
  - `new_subagent(prefix: &str) -> Self`: generates a unique ID like `format!("{}-{}", prefix, &uuid::Uuid::new_v4().to_string()[..8])`.
  - `is_parent(&self) -> bool`: returns `self.0 == "parent"`.
  - Implements: `Debug`, `Clone`, `PartialEq`, `Eq`, `Hash`, `Serialize`, `Deserialize`.
- `SubagentRole`:
  - Variants: `Scout`, `Coder`, `Tester`, `Reviewer`.
  - `default_workspace_mode(&self) -> WorkspaceMode`:
    - `Scout | Reviewer` -> `WorkspaceMode::Shared`
    - `Coder | Tester` -> `WorkspaceMode::Worktree`
  - `tool_filter_mode(&self) -> &'static str`:
    - `Scout | Reviewer` -> `"read_only"`
    - `Coder | Tester` -> `"standard"`
  - Implements: `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`, `Serialize`, `Deserialize` with `#[serde(rename_all = "snake_case")]`.
- `WorkspaceMode`:
  - Variants: `Auto`, `Worktree`, `Shared`.
  - Implements: `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`, `Serialize`, `Deserialize` with `#[serde(rename_all = "snake_case")]`.
- `SubagentState`:
  - Variants: `Starting`, `Running`, `WaitingForInput`, `Completed`, `Failed(String)`, `Terminated`.
  - Implements: `Debug`, `Clone`, `PartialEq`, `Eq`, `Serialize`, `Deserialize` with `#[serde(rename_all = "snake_case")]`.

### 2. `src/agent/subagent/message.rs`
- `MessageIntent`:
  - Variants: `TaskInit`, `StatusUpdate`, `ClarificationRequest`, `ClarificationResponse`, `Feedback`, `Handoff`, `TaskComplete`.
  - Implements: `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`, `Serialize`, `Deserialize` with `#[serde(rename_all = "snake_case")]`.
- `AgentMessage`:
  - Fields:
    - `pub id: String`
    - `pub sender: AgentId`
    - `pub recipient: AgentId`
    - `pub intent: MessageIntent`
    - `pub content: String`
    - `pub timestamp: chrono::DateTime<chrono::Utc>`
    - `pub metadata: Option<serde_json::Value>`
  - `format_for_prompt(&self) -> String`:
    Formats as:
    ```rust
    format!(
        "<agent_message from=\"{}\" intent=\"{:?}\" timestamp=\"{}\">\n{}\n</agent_message>",
        self.sender.0,
        serde_json::to_string(&self.intent).unwrap_or_else(|_| "message".to_string()).trim_matches('"'),
        self.timestamp.to_rfc3339(),
        self.content.trim()
    )
    ```
  - Implements: `Debug`, `Clone`, `Serialize`, `Deserialize`.

### 3. `src/agent/subagent/mailbox.rs`
- `AgentMailbox`:
  - Fields:
    - `agent_id: AgentId`
    - `mailbox_path: std::path::PathBuf`
  - `new(agent_id: AgentId, agent_dir: &std::path::Path) -> std::io::Result<Self>`:
    - Ensures `agent_dir` exists via `std::fs::create_dir_all`.
    - Sets `mailbox_path = agent_dir.join("mailbox.jsonl")`.
  - `post(&self, msg: AgentMessage) -> std::io::Result<()>`:
    - Serializes `msg` as JSON line.
    - Opens `mailbox_path` with `OpenOptions::new().create(true).append(true).open(...)`.
    - Writes line + newline, calls `file.sync_all()`.
  - `drain_unread(&self) -> std::io::Result<Vec<AgentMessage>>`:
    - If `mailbox_path` does not exist, return `Ok(vec![])`.
    - Safely renames `mailbox_path` to a temporary processing file (e.g. `mailbox.processing.<timestamp>.jsonl`) to ensure no race condition on concurrent appends, reads all lines, deserializes each line, and removes the processing file.
  - `unread_count(&self) -> std::io::Result<usize>`:
    - If `mailbox_path` does not exist, returns `Ok(0)`.
    - Reads line count of `mailbox_path`.

### 4. `src/agent/subagent/mod.rs` & `src/agent/mod.rs`
- `pub mod types;`
- `pub mod message;`
- `pub mod mailbox;`
- Re-export common types.
- In `src/agent/mod.rs`, add `pub mod subagent;`.

## Unit Tests Required
In `src/agent/subagent/types.rs` and `src/agent/subagent/mailbox.rs`:
- `test_agent_id_and_roles`
- `test_agent_message_format_for_prompt`
- `test_agent_mailbox_fifo_and_disk_persistence`

## Critical Constraints
1. ONLY run targeted tests: `cargo test -j 1 --lib agent::subagent::tests`. NEVER run the full test suite.
2. Error handling: `std::io::Result` / `Result<T, E>`. Zero `.unwrap()` or `.expect()` in non-test code.
3. Concurrency: Use `-j 1` for `cargo check` and `cargo test`.
4. Verification: `cargo fmt` and `cargo clippy -j 1 --bin minicode -- -D warnings`.
5. Commit message: `feat(subagent): implement core types, A2A message schema, and reactive mailbox (Phase 132)`.

# Multi-Agent Subagent Delegation & A2A Communication Runtime Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build an autonomous multi-agent delegation runtime in `minicode` with ephemeral Git worktree isolation, typed agent-to-agent (A2A) mailboxes, process-level sandboxing, and real-time nested timeline activity cards.

**Architecture:** A Hub-and-Spoke model where the coordinator agent delegates tasks via a `spawn_subagent` tool. Mutating subagents execute in ephemeral git worktree branches (`.minicode/worktrees/subagent-<id>`) with process-level Landlock sandboxes. Agents communicate bidirectionally using `send_message` and reactive file-backed mailboxes (`.minicode/agents/<id>/mailbox.jsonl`), with child progress streamed live to Ratatui timeline cards.

**Tech Stack:** Rust 2021 Edition, Tokio async subprocesses, Git worktrees, serde/serde_json, chrono, ratatui.

## Global Constraints
- ONLY run targeted tests with `-j 1`. NEVER run the full workspace test suite.
- Zero non-test `.unwrap()` or `.expect()`. Use `thiserror` for internal crate errors and `anyhow::Result` at CLI boundaries.
- Pure Rust networking and cross-platform compatibility (`\\` to `/` path normalization).
- Use `tracing` macros for all logging. Never use `println!` or `eprintln!` in library code.
- Ephemeral worktrees must always be cleaned up on success, failure, or cancellation.
- Update `TOTAL_TOOL_COUNT` constant and schema registry when adding tools.

---

### Task 1: Subagent Core Types & A2A Messaging Infrastructure

**Files:**
- Create: `src/agent/subagent/types.rs`
- Create: `src/agent/subagent/message.rs`
- Create: `src/agent/subagent/mailbox.rs`
- Create: `src/agent/subagent/mod.rs`
- Modify: `src/agent/mod.rs`

**Interfaces:**
- Consumes: `uuid`, `chrono`, `serde`, `serde_json`.
- Produces:
  - `AgentId`: Unique identifier (`"parent"`, `"scout-..."`, `"coder-..."`).
  - `SubagentRole`: Enum (`Scout`, `Coder`, `Tester`, `Reviewer`) with `default_workspace_mode()` and `tool_filter_mode()`.
  - `WorkspaceMode`: Enum (`Auto`, `Worktree`, `Shared`).
  - `SubagentState`: Enum (`Starting`, `Running`, `WaitingForInput`, `Completed`, `Failed(String)`, `Terminated`).
  - `MessageIntent`: Enum (`TaskInit`, `StatusUpdate`, `ClarificationRequest`, `ClarificationResponse`, `Feedback`, `Handoff`, `TaskComplete`).
  - `AgentMessage`: Structured message with `format_for_prompt()`.
  - `AgentMailbox`: Thread-safe FIFO queue backed by `.minicode/agents/<id>/mailbox.jsonl`.

- [ ] **Step 1: Write failing unit tests for types, message formatting, and mailbox FIFO ordering**

In `src/agent/subagent/types.rs` and `src/agent/subagent/mailbox.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_id_and_roles() {
        let parent = AgentId::parent();
        assert_eq!(parent.0, "parent");
        assert!(parent.is_parent());

        let scout_id = AgentId::new_subagent("scout");
        assert!(scout_id.0.starts_with("scout-"));

        assert_eq!(SubagentRole::Scout.default_workspace_mode(), WorkspaceMode::Shared);
        assert_eq!(SubagentRole::Coder.default_workspace_mode(), WorkspaceMode::Worktree);
        assert_eq!(SubagentRole::Tester.default_workspace_mode(), WorkspaceMode::Worktree);
        assert_eq!(SubagentRole::Reviewer.default_workspace_mode(), WorkspaceMode::Shared);
    }

    #[test]
    fn test_agent_message_format_for_prompt() {
        let msg = AgentMessage {
            id: "msg-1".to_string(),
            sender: AgentId("scout-1".to_string()),
            recipient: AgentId::parent(),
            intent: MessageIntent::StatusUpdate,
            content: "Found 3 auth files".to_string(),
            timestamp: chrono::Utc::now(),
            metadata: None,
        };
        let formatted = msg.format_for_prompt();
        assert!(formatted.contains("<agent_message from=\"scout-1\" intent=\"status_update\""));
        assert!(formatted.contains("Found 3 auth files"));
        assert!(formatted.contains("</agent_message>"));
    }

    #[tokio::test]
    async fn test_agent_mailbox_fifo_and_disk_persistence() {
        let temp_dir = tempfile::tempdir().unwrap();
        let agent_dir = temp_dir.path().join(".minicode").join("agents").join("test-agent");
        let mailbox = AgentMailbox::new(AgentId("test-agent".to_string()), &agent_dir).unwrap();

        assert_eq!(mailbox.unread_count().unwrap(), 0);

        let msg1 = AgentMessage {
            id: "m1".to_string(),
            sender: AgentId::parent(),
            recipient: AgentId("test-agent".to_string()),
            intent: MessageIntent::TaskInit,
            content: "Start search".to_string(),
            timestamp: chrono::Utc::now(),
            metadata: None,
        };
        let msg2 = AgentMessage {
            id: "m2".to_string(),
            sender: AgentId::parent(),
            recipient: AgentId("test-agent".to_string()),
            intent: MessageIntent::Feedback,
            content: "Also check tests/".to_string(),
            timestamp: chrono::Utc::now(),
            metadata: None,
        };

        mailbox.post(msg1).unwrap();
        mailbox.post(msg2).unwrap();
        assert_eq!(mailbox.unread_count().unwrap(), 2);

        let drained = mailbox.drain_unread().unwrap();
        assert_eq!(drained.len(), 2);
        assert_eq!(drained[0].content, "Start search");
        assert_eq!(drained[1].content, "Also check tests/");
        assert_eq!(mailbox.unread_count().unwrap(), 0);
    }
}
```

- [ ] **Step 2: Run targeted tests to verify failure**
Run: `cargo test -j 1 --lib agent::subagent::tests`
Expected: FAIL with unresolvable module `agent::subagent`.

- [ ] **Step 3: Implement `types.rs`, `message.rs`, `mailbox.rs`, and export in `mod.rs`**
Implement the data structures, JSON serialization/deserialization, disk-backed atomic file append in `mailbox.rs`, and register `pub mod subagent;` in `src/agent/mod.rs`.

- [ ] **Step 4: Run targeted tests to verify they pass**
Run: `cargo test -j 1 --lib agent::subagent::tests`
Expected: PASS (3 passed).

- [ ] **Step 5: Verify formatting and clippy**
Run: `cargo fmt && cargo clippy -j 1 --bin minicode -- -D warnings`
Expected: Clean with 0 warnings.

- [ ] **Step 6: Commit**
```bash
git add src/agent/subagent/ src/agent/mod.rs
git commit -m "feat(subagent): implement core types, A2A message schema, and reactive mailbox (Phase 132)"
```

---

### Task 2: Ephemeral Git Worktree Sandboxing Engine

**Files:**
- Create: `src/sandbox/worktree.rs`
- Modify: `src/sandbox/mod.rs`

**Interfaces:**
- Consumes: `AgentId`, `std::process::Command`, `std::path::Path`.
- Produces:
  - `WorktreeHandle`: Encapsulates `worktree_path`, `branch_name`, and `agent_id`.
  - `GitWorktreeManager::create_worktree(repo_root, agent_id) -> Result<WorktreeHandle>`: executes `git worktree add -b <branch> <path> HEAD`.
  - `GitWorktreeManager::capture_diff(handle) -> Result<String>`: runs `git diff HEAD` inside the worktree directory.
  - `GitWorktreeManager::remove_worktree(handle) -> Result<()>`: runs `git worktree remove --force` and deletes temporary branch.
  - `GitWorktreeManager::cleanup_stale_worktrees(repo_root) -> Result<usize>`: cleans orphaned worktree directories in `.minicode/worktrees/`.

- [ ] **Step 1: Write failing unit tests for worktree creation, diff capture, and teardown**

In `src/sandbox/worktree.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_worktree_lifecycle_in_git_repo() {
        let temp_dir = tempfile::tempdir().unwrap();
        let repo_path = temp_dir.path();

        // Initialize git repo
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(repo_path)
            .output()
            .unwrap();
        std::fs::write(repo_path.join("file.txt"), "hello world\n").unwrap();
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(repo_path)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["-c", "user.name=test", "-c", "user.email=test@test.com", "commit", "-m", "init"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        let agent_id = AgentId("coder-test".to_string());
        let handle = GitWorktreeManager::create_worktree(repo_path, &agent_id).unwrap();

        assert!(handle.worktree_path.exists());
        assert!(handle.branch_name.contains("coder-test"));

        // Modify file inside worktree
        std::fs::write(handle.worktree_path.join("file.txt"), "hello worktree\n").unwrap();

        // Capture diff
        let diff = GitWorktreeManager::capture_diff(&handle).unwrap();
        assert!(diff.contains("-hello world"));
        assert!(diff.contains("+hello worktree"));

        // Cleanup
        GitWorktreeManager::remove_worktree(&handle).unwrap();
        assert!(!handle.worktree_path.exists());
    }
}
```

- [ ] **Step 2: Run targeted tests to verify failure**
Run: `cargo test -j 1 --lib sandbox::worktree::tests`
Expected: FAIL with missing module.

- [ ] **Step 3: Implement `GitWorktreeManager` and `WorktreeHandle`**
Implement worktree provisioning, error handling for non-git directories, diff generation, branch cleanup, and export in `src/sandbox/mod.rs`.

- [ ] **Step 4: Run targeted tests to verify they pass**
Run: `cargo test -j 1 --lib sandbox::worktree::tests`
Expected: PASS (1 passed).

- [ ] **Step 5: Verify formatting and clippy**
Run: `cargo fmt && cargo clippy -j 1 --bin minicode -- -D warnings`
Expected: Clean with 0 warnings.

- [ ] **Step 6: Commit**
```bash
git add src/sandbox/worktree.rs src/sandbox/mod.rs
git commit -m "feat(sandbox): implement ephemeral Git worktree manager for subagent isolation"
```

---

### Task 3: Subagent Tool Primitives & Child Process Orchestrator

**Files:**
- Create: `src/agent/subagent/orchestrator.rs`
- Create: `src/tools/subagent.rs`
- Modify: `src/tools/mod.rs`
- Modify: `src/constants.rs`

**Interfaces:**
- Consumes: `AgentId`, `SubagentRole`, `WorkspaceMode`, `GitWorktreeManager`, `AgentMailbox`.
- Produces:
  - `SubagentOrchestrator`: Spawns child `minicode run -d <dir> -y --json-stream` process, attaches piped stdout, reads NDJSON lines, and returns `SubagentResult`.
  - Tool `spawn_subagent`: Schema, JSON args parser, auto-worktree detection, execution.
  - Tool `send_message`: Schema, posts message to target mailbox.
  - Tool `manage_subagents`: Schema, lists active child subagents, queries status, cancels process.
  - Updates `TOTAL_TOOL_COUNT` (134 -> 137).

- [ ] **Step 1: Write failing unit tests for tool schemas, arg parsing, and role resolution**

In `src/tools/subagent.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spawn_subagent_arg_parsing() {
        let json = serde_json::json!({
            "task": "Find auth references",
            "role": "scout",
            "workspace_mode": "shared"
        });
        let args: SpawnSubagentArgs = serde_json::from_value(json).unwrap();
        assert_eq!(args.task, "Find auth references");
        assert_eq!(args.role, "scout");
        assert_eq!(args.workspace_mode.as_deref(), Some("shared"));
    }

    #[test]
    fn test_send_message_arg_parsing() {
        let json = serde_json::json!({
            "recipient": "coder-1",
            "message": "Fix the null pointer",
            "intent": "feedback"
        });
        let args: SendMessageArgs = serde_json::from_value(json).unwrap();
        assert_eq!(args.recipient, "coder-1");
        assert_eq!(args.intent.as_deref(), Some("feedback"));
    }

    #[test]
    fn test_manage_subagents_arg_parsing() {
        let json = serde_json::json!({
            "action": "status",
            "subagent_id": "scout-1"
        });
        let args: ManageSubagentsArgs = serde_json::from_value(json).unwrap();
        assert_eq!(args.action, "status");
        assert_eq!(args.subagent_id.as_deref(), Some("scout-1"));
    }
}
```

- [ ] **Step 2: Run targeted tests to verify failure**
Run: `cargo test -j 1 --lib tools::subagent::tests`
Expected: FAIL with unresolvable module.

- [ ] **Step 3: Implement `orchestrator.rs`, `subagent.rs`, register in `tools/mod.rs`, and update `constants.rs`**
Implement tools, parameter schemas, process spawn with `kill_on_drop(true)`, `process_group(0)` on Unix, stdout line stream decoding, and register in `ToolRegistry::get_tool_schemas()` and `dispatch_tool`. Bump `TOTAL_TOOL_COUNT` to 137.

- [ ] **Step 4: Run targeted tests to verify they pass**
Run: `cargo test -j 1 --lib tools::subagent::tests`
Expected: PASS (3 passed).

- [ ] **Step 5: Verify formatting and clippy**
Run: `cargo fmt && cargo clippy -j 1 --bin minicode -- -D warnings`
Expected: Clean with 0 warnings.

- [ ] **Step 6: Commit**
```bash
git add src/agent/subagent/orchestrator.rs src/tools/subagent.rs src/tools/mod.rs src/constants.rs
git commit -m "feat(tools): add spawn_subagent, send_message, and manage_subagents tools (137 tools)"
```

---

### Task 4: AgentLoop Ingestion & Real-Time Timeline Cards

**Files:**
- Modify: `src/agent/types.rs`
- Modify: `src/agent/loop.rs`
- Modify: `src/ui/view.rs`
- Create: `tests/integration_subagent_delegation.rs`

**Interfaces:**
- Consumes: `AgentEvent::SubagentProgress`, `AgentEvent::SubagentCompleted`, `AgentMailbox`.
- Produces:
  - `AgentLoop`: Polls mailbox at start of turn, dequeues unread messages into context as `<agent_message>` blocks.
  - `src/ui/view.rs`: Formats subagent activity cards into conversation view with badge, role, action, and duration.
  - End-to-end integration test verifying child spawning, message passing, worktree isolation, and diff reporting.

- [ ] **Step 1: Write integration tests for subagent delegation**

In `tests/integration_subagent_delegation.rs`:
```rust
use minicode::agent::subagent::types::{AgentId, SubagentRole, WorkspaceMode};
use minicode::agent::subagent::message::{AgentMessage, MessageIntent};
use minicode::agent::subagent::mailbox::AgentMailbox;

#[tokio::test]
async fn test_subagent_mailbox_turn_injection() {
    let temp_dir = tempfile::tempdir().unwrap();
    let agent_dir = temp_dir.path().join(".minicode").join("agents").join("parent");
    let mailbox = AgentMailbox::new(AgentId::parent(), &agent_dir).unwrap();

    let msg = AgentMessage {
        id: "m1".to_string(),
        sender: AgentId("coder-1".to_string()),
        recipient: AgentId::parent(),
        intent: MessageIntent::TaskComplete,
        content: "Completed unit tests for auth module. All 4 tests passing.".to_string(),
        timestamp: chrono::Utc::now(),
        metadata: None,
    };
    mailbox.post(msg).unwrap();

    let unread = mailbox.drain_unread().unwrap();
    assert_eq!(unread.len(), 1);
    let prompt_snippet = unread[0].format_for_prompt();
    assert!(prompt_snippet.contains("<agent_message from=\"coder-1\" intent=\"task_complete\""));
    assert!(prompt_snippet.contains("All 4 tests passing"));
}

#[test]
fn test_subagent_role_tool_isolation() {
    assert_eq!(SubagentRole::Scout.tool_filter_mode(), "read_only");
    assert_eq!(SubagentRole::Reviewer.tool_filter_mode(), "read_only");
    assert_eq!(SubagentRole::Coder.tool_filter_mode(), "standard");
    assert_eq!(SubagentRole::Tester.tool_filter_mode(), "standard");
}
```

- [ ] **Step 2: Run integration tests to verify failure/missing items**
Run: `cargo test -j 1 --test integration_subagent_delegation`
Expected: Verify compilation and behavior.

- [ ] **Step 3: Wire mailbox drain into `AgentLoop::execute_turn` and format subagent cards in `src/ui/view.rs`**
In `src/agent/loop.rs`:
- Drain unread messages from `self.mailbox` at the start of each iteration.
- Insert them as `Message::user(msg.format_for_prompt())`.
- Emit `AgentEvent::SubagentProgress` when child events arrive.
In `src/ui/view.rs`:
- Render nested subagent card with borders, role styling, live tools, and duration.

- [ ] **Step 4: Run targeted tests to verify they pass**
Run: `cargo test -j 1 --test integration_subagent_delegation`
Expected: PASS (2 passed).

- [ ] **Step 5: Verify formatting and clippy**
Run: `cargo fmt && cargo clippy -j 1 --bin minicode -- -D warnings`
Expected: Clean with 0 warnings.

- [ ] **Step 6: Commit**
```bash
git add src/agent/types.rs src/agent/loop.rs src/ui/view.rs tests/integration_subagent_delegation.rs
git commit -m "feat(agent): wire subagent event streaming and reactive mailbox injection into AgentLoop"
```

---

### Task 5: Quality Gates, Release Bump (v0.3.33), Binary Build & Real-World Validation

**Files:**
- Modify: `Cargo.toml`
- Modify: `onpkg_docs/todo.md`

- [ ] **Step 1: Bump version in `Cargo.toml` to `0.3.33`**
- [ ] **Step 2: Update `onpkg_docs/todo.md` with Phase 132 tracker checklist**
- [ ] **Step 3: Run clippy and format checks**
Run: `cargo clippy -j 1 --bin minicode -- -D warnings && cargo fmt --check`
Expected: Zero warnings, 100% formatted.
- [ ] **Step 4: Compile release binary via `./localupdate.sh`**
Run: `./localupdate.sh`
Expected: Binary successfully compiled and installed to `~/.local/bin/minicode`.
- [ ] **Step 5: Validate real-world autonomous execution with `minicode run`**
Run: `minicode run -d test_playground/realworld_subagent_test -p minimax -m MiniMax-M2.7 -y --json-stream "Check git status and summarize the latest commits in this repository."`
Expected: Output streamed, status complete, exit code 0.
- [ ] **Step 6: Commit release**
```bash
git add Cargo.toml Cargo.lock onpkg_docs/todo.md docs/superpowers/plans/
git commit -m "release(v0.3.33): deliver multi-agent subagent delegation and A2A communication runtime (Phase 132)"
```

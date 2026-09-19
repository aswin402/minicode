# Design Specification: Multi-Agent Subagent Delegation & A2A Communication Runtime

- **Status**: Approved
- **Target Version**: `v0.3.33` (Phase 132)
- **Author**: Antigravity & Aswin
- **Date**: 2026-09-19
- **Domain**: Autonomous Multi-Agent Swarms, Ephemeral Git Worktrees, Agent-to-Agent Messaging

---

## 1. Overview & Motivation

As software engineering tasks scale in complexity (e.g. cross-crate refactoring, large feature additions, test suite authoring, and adversarial reviews), single-threaded linear agent loops encounter three severe bottlenecks:
1. **Context Window Exhaustion & Dilution**: Reading dozens of files and running extensive test suites dumps tens of thousands of tokens into a single context window, degrading reasoning quality.
2. **Workspace Pollution**: Experimental or half-baked file mutations made during exploration or testing dirty the developer's working tree and risk breaking active compilation.
3. **Lack of Parallelization & Specialization**: A single agent must sequentially read, write, review, and test without division of labor.

**Phase 132 introduces the Multi-Agent Subagent Delegation Runtime**:
- **Autonomous Subagent Spawning**: The coordinator agent autonomously delegates scoped tasks to specialized child agents (`scout`, `coder`, `tester`, `reviewer`).
- **Ephemeral Git Worktrees**: Mutating subagents execute in isolated, disposable git worktree branches (`.minicode/worktrees/subagent-<id>`), guaranteeing zero pollution of the developer's active workspace.
- **Agent-to-Agent (A2A) Reactive Messaging**: Agents communicate bidirectionally (`parent <-> subagent` and `subagent <-> subagent`) via structured typed mailboxes (`send_message`, `manage_subagents`).
- **Zero-Friction Autonomous UX**: No manual `/delegate` slash commands are needed. The agent orchestrates delegation and collaboration automatically based on task scope.
- **Real-Time TUI Activity Cards**: The parent Ratatui timeline renders live, non-intrusive nested activity badges streaming child progress.

---

## 2. Core Architectural Principles

1. **Hub-and-Spoke Coordinator Pattern**:
   - The **Parent Agent (Hub)** maintains the primary conversation, overall user intent, and high-level goal ledger.
   - **Subagents (Spokes)** are ephemeral worker instances with isolated context windows that distill heavy exploration/coding into concise structured summaries.
2. **Clean Process & Sandbox Boundaries**:
   - Subagents are executed as headless child processes (`minicode run -d <dir> --tools <mode> -y --json-stream`) with dedicated process groups, OS Landlock sandboxing, and SIGTERM/SIGKILL escalation on cancellation.
   - If a subagent crashes, panics, or OOMs, the parent agent and TUI are completely unaffected.
3. **Least-Privilege Role Specialization**:
   - `scout`: Read-only discovery. Tools: `read_file`, `grep_search`, `locate_symbol`, `impact_analysis`, `find_by_name`. Workspace: shared.
   - `coder`: Implementation. Tools: `read_file`, `write_file`, `patch_file`, `exec_cmd`, `locate_symbol`. Workspace: isolated git worktree.
   - `tester`: Validation & QA. Tools: `read_file`, `write_file`, `patch_file`, `exec_cmd`, `sandbox_exec`. Workspace: isolated git worktree.
   - `reviewer`: Adversarial code review. Tools: `read_file`, `grep_search`, `git diff`. Workspace: shared read-only.
4. **Reactive A2A Mailboxes**:
   - Every agent instance (parent and subagents) is assigned a unique `agent_id`.
   - Mailboxes are stored in `.minicode/agents/<agent_id>/mailbox.jsonl` and routed via an in-process and file-backed bus.
   - Incoming messages are automatically dequeued at the beginning of each LLM turn iteration and formatted into prompt context.

---

## 3. Data Structures & Protocol Schemas

### 3.1. Subagent Identifier & Status
Located in `src/agent/subagent/types.rs`:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AgentId(pub String);

impl AgentId {
    pub fn parent() -> Self {
        Self("parent".to_string())
    }

    pub fn new_subagent(prefix: &str) -> Self {
        let id = &uuid::Uuid::new_v4().to_string()[..8];
        Self(format!("{}-{}", prefix, id))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubagentRole {
    Scout,
    Coder,
    Tester,
    Reviewer,
}

impl SubagentRole {
    pub fn default_workspace_mode(&self) -> WorkspaceMode {
        match self {
            Self::Scout | Self::Reviewer => WorkspaceMode::Shared,
            Self::Coder | Self::Tester => WorkspaceMode::Worktree,
        }
    }

    pub fn tool_filter_mode(&self) -> &'static str {
        match self {
            Self::Scout => "read_only",
            Self::Reviewer => "read_only",
            Self::Coder => "standard",
            Self::Tester => "standard",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceMode {
    Auto,
    Worktree,
    Shared,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubagentState {
    Starting,
    Running,
    WaitingForInput,
    Completed,
    Failed(String),
    Terminated,
}
```

### 3.2. Agent-to-Agent (A2A) Message Schema
Located in `src/agent/subagent/message.rs`:

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use super::types::AgentId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageIntent {
    TaskInit,
    StatusUpdate,
    ClarificationRequest,
    ClarificationResponse,
    Feedback,
    Handoff,
    TaskComplete,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMessage {
    pub id: String,
    pub sender: AgentId,
    pub recipient: AgentId,
    pub intent: MessageIntent,
    pub content: String,
    pub timestamp: DateTime<Utc>,
    pub metadata: Option<serde_json::Value>,
}

impl AgentMessage {
    pub fn format_for_prompt(&self) -> String {
        format!(
            "<agent_message from=\"{}\" intent=\"{:?}\" timestamp=\"{}\">\n{}\n</agent_message>",
            self.sender.0, self.intent, self.timestamp.to_rfc3339(), self.content
        )
    }
}
```

### 3.3. Subagent Metadata & Execution Record
Located in `src/agent/subagent/record.rs`:

```rust
use std::path::PathBuf;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use super::types::{AgentId, SubagentRole, SubagentState, WorkspaceMode};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubagentRecord {
    pub id: AgentId,
    pub parent_id: AgentId,
    pub role: SubagentRole,
    pub task: String,
    pub workspace_path: PathBuf,
    pub workspace_mode: WorkspaceMode,
    pub git_branch: Option<String>,
    pub state: SubagentState,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub total_tokens_used: usize,
    pub tools_executed: usize,
    pub final_summary: Option<String>,
}
```

---

## 4. Subagent Tool Primitives

### 4.1. `spawn_subagent`
Registered in `src/tools/mod.rs` and dispatched by `AgentLoop`:

- **Tool Name**: `spawn_subagent`
- **Description**: Spawns an autonomous background subagent with a specialized role (`scout`, `coder`, `tester`, `reviewer`) to perform an isolated task. For mutating tasks, an ephemeral Git worktree is created automatically.
- **Parameters**:
  - `task` (string, required): Clear, actionable prompt describing the task.
  - `role` (string, required): Specialization: `"scout"`, `"coder"`, `"tester"`, `"reviewer"`.
  - `workspace_mode` (string, optional): `"auto"` (default), `"worktree"`, or `"shared"`.
  - `max_iterations` (integer, optional): Maximum tool loop iterations (default: 10).

### 4.2. `send_message`
- **Tool Name**: `send_message`
- **Description**: Sends a structured message or question to another agent (`"parent"`, specific subagent ID, or `"all"`).
- **Parameters**:
  - `recipient` (string, required): Recipient agent ID (`"parent"` or subagent ID).
  - `message` (string, required): Content of the message.
  - `intent` (string, optional): `"status_update"`, `"clarification_request"`, `"clarification_response"`, `"feedback"`, `"handoff"`.

### 4.3. `manage_subagents`
- **Tool Name**: `manage_subagents`
- **Description**: Inspects, waits for, or cancels child subagents.
- **Parameters**:
  - `action` (string, required): `"list"`, `"status"`, `"wait"`, `"kill"`.
  - `subagent_id` (string, optional): Target subagent ID (required for `"status"` and `"kill"`).

---

## 5. Ephemeral Git Worktree Manager

Located in `src/sandbox/worktree.rs`:

1. **Creation**:
   - `git worktree add -b minicode-task-<id> .minicode/worktrees/<id> HEAD`
   - Validates that git is clean or allows uncommitted reads from HEAD.
   - Symlinks or mirrors target build cache if available to prevent cold compilation.
2. **Execution**:
   - Child subagent's working directory is pinned to `.minicode/worktrees/<id>`.
   - All tool calls (`write_file`, `patch_file`, `exec_cmd`) run inside the worktree directory.
3. **Capture & Integration**:
   - When the subagent finishes:
     - `git diff HEAD` is captured from the worktree to generate a patch.
     - Commits on the temporary branch are recorded.
     - The parent agent receives the diff and commit list, and can automatically merge or cherry-pick into the main working tree if the task succeeded.
4. **Cleanup**:
   - `git worktree remove --force .minicode/worktrees/<id>`
   - Deletes the temporary branch `minicode-task-<id>`.
   - On process crash or abort, teardown hooks remove dangling worktrees in `.minicode/worktrees/`.

---

## 6. A2A Mailbox Router & AgentLoop Integration

Located in `src/agent/subagent/mailbox.rs` and `src/agent/loop.rs`:

1. **Mailbox Ingestion at Turn Start**:
   - At the beginning of `AgentLoop::execute_turn`, the loop polls its inbox:
     ```rust
     let unread_messages = self.mailbox.drain_unread()?;
     for msg in unread_messages {
         self.messages.push(Message::user(msg.format_for_prompt()));
     }
     ```
2. **Child Process NDJSON Streaming**:
   - Parent spawns:
     ```rust
     Command::new(std::env::current_exe()?)
         .arg("run")
         .arg("-d").arg(&worktree_or_shared_dir)
         .arg("-y")
         .arg("--json-stream")
         .arg("--tools").arg(role.tool_filter_mode())
         .arg(&task)
         .stdout(Stdio::piped())
         .spawn()?;
     ```
   - Parent reads stdout line-by-line using `tokio::io::BufReader`.
   - Translates child NDJSON events (`tool_call`, `tool_result`, `stream_delta`, `turn_end`) into `AgentEvent::SubagentProgress`.

---

## 7. Timeline Presentation (Ratatui TUI)

Located in `src/ui/view.rs`:

- Subagents are rendered inline in the conversation stream using interactive nested cards:
  ```text
  ┌─ ⚡ Subagent [coder: subagent-a1b2] ───────────────────── RUNNING ─┐
  │ Task: Implement argon2 password hashing in user_service.rs          │
  │ Worktree: .minicode/worktrees/subagent-a1b2 (branch: agent/a1b2)   │
  │ Current Action: patch_file("src/user_service.rs") [OK: 45ms]       │
  │ Stats: 3 tools | 1,420 tokens | Duration: 4.2s                     │
  └────────────────────────────────────────────────────────────────────┘
  ```
- Upon completion, the card collapses into a compact badge:
  `✔ Subagent [coder] completed in 6.1s (3 files modified, 12 tests passed)`

---

## 8. Verification & Quality Gates

1. **Unit Tests**:
   - `AgentId` and role parsing, default workspace modes.
   - `AgentMessage` serialization, deserialization, and XML prompt formatting.
   - Mailbox FIFO ordering, thread safety, and persistence to JSONL.
   - Worktree path validation and lexical sanitization.
2. **Integration Tests**:
   - `tests/integration_subagent_delegation.rs`: End-to-end test with a mock subagent verifying spawn, message passing, worktree creation, diff collection, and cleanup.
   - Test child error recovery: subagent failure does not panic or stop parent execution.
3. **Quality Standards**:
   - Zero non-test `.unwrap()` or `.expect()`.
   - Strict `-j 1` on `cargo check` and `cargo test`.
   - Zero clippy warnings (`cargo clippy -j 1 --bin minicode -- -D warnings`).
   - Clean formatting (`cargo fmt --check`).
   - Real-world autonomous execution test using compiled binary.

# Swarm Cross-Worker Inter-Agent Messaging Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement a durable, process-safe, asynchronous Pub-Sub Event Bus allowing concurrent multi-agent swarm workers executing in isolated Git worktrees to exchange typed contract definitions, query interfaces, and share findings with zero polling overhead and strict anti-chatter token quotas.

**Architecture:** A durable file-backed append-only JSONL log (`.minicode/swarms/<swarm_id>/bus.jsonl`) serves as the IPC substrate across worker child processes. `SwarmScheduler` propagates swarm identity and peer IDs via environment variables. `AgentLoop` automatically reads unread peer messages at turn start and injects a `<peer_messages>` context block with zero tool polling. The existing `send_worker_message` tool is upgraded in place to support typed intents (`publish_contract`, `query_interface`, `coordination_note`) with a hard 3-message quota and 800-character payload limit.

**Tech Stack:** Rust 2021 Edition, Tokio multi-threaded async runtime, serde_json, chrono, uuid, thiserror, petgraph.

## Global Constraints

- Pure Rust 2021 Edition with Tokio multi-threaded async engine.
- Zero `.unwrap()` or `.expect()` in non-test library code (`src/`). Use `thiserror` for `SwarmError` and `Result<T, SwarmError>` / `anyhow::Result` at boundaries.
- Tool count invariant: `TOTAL_TOOL_COUNT = 186` in `src/constants.rs` must remain strictly synchronized with `ToolRegistry::get_tool_schemas().len()`.
- Concurrency flags: All cargo commands must use `-j 1` on check/test/clippy (`cargo check -j 1`, `cargo test -j 1`, `cargo clippy -j 1 -- -D warnings`); `-j 2` for release build.
- Anti-Chatter Limits: Hard limit of max 3 outgoing messages per worker per wave; max 800 characters per message payload.
- Zero-Polling Ingestion: Unread messages automatically injected at the start of each LLM reasoning turn into `<peer_messages>`.
- Auditability: Complete inter-worker message history rendered in `.minicode/swarms/<swarm_id>/report.md`.

---

### Task 1: Core Domain Models & Process-Safe Durable Bus

**Files:**
- Create: `src/agent/swarm/bus.rs`
- Modify: `src/agent/swarm/mod.rs:1-22`
- Modify: `src/agent/swarm/models.rs:16-57`
- Test: `src/agent/swarm/bus.rs:tests`

**Interfaces:**
- Consumes: `SwarmError` from `src/agent/swarm/models.rs`
- Produces: `SwarmMessageIntent`, `SwarmMessage`, `SwarmMessageBus` exported from `src/agent/swarm/bus.rs`

- [ ] **Step 1: Add message-related variants to `SwarmError`**

In `src/agent/swarm/models.rs`:
```rust
    #[error("Message payload exceeds 800 characters (actual: {0})")]
    MessagePayloadTooLarge(usize),

    #[error("Message quota exceeded: worker '{0}' has reached the limit of 3 messages for this wave")]
    MessageQuotaExceeded(String),

    #[error("Invalid recipient: worker cannot send a message to itself")]
    SelfMessageNotAllowed,
```

- [ ] **Step 2: Write failing unit tests for `SwarmMessageBus`**

In `src/agent/swarm/bus.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_bus_post_and_read_unread() {
        let dir = tempdir().unwrap();
        let bus = SwarmMessageBus::new(dir.path()).unwrap();

        let msg1 = SwarmMessage::new(
            "swarm-123",
            "t1_backend",
            Some("t2_frontend"),
            SwarmMessageIntent::PublishContract,
            "Auth API",
            "export interface AuthToken { token: string; }",
        );
        bus.post_message(msg1.clone()).unwrap();

        // Reading unread for t2_frontend should return msg1
        let unread = bus.read_unread("t2_frontend", None).unwrap();
        assert_eq!(unread.len(), 1);
        assert_eq!(unread[0].topic, "Auth API");

        // Reading unread with cursor msg1.id should return 0
        let unread_next = bus.read_unread("t2_frontend", Some(&msg1.id)).unwrap();
        assert_eq!(unread_next.len(), 0);

        // Sender t1_backend should not receive its own message
        let unread_sender = bus.read_unread("t1_backend", None).unwrap();
        assert_eq!(unread_sender.len(), 0);
    }

    #[test]
    fn test_bus_quota_enforcement() {
        let dir = tempdir().unwrap();
        let bus = SwarmMessageBus::new(dir.path()).unwrap();

        for i in 1..=3 {
            let msg = SwarmMessage::new(
                "swarm-123",
                "t1_worker",
                None,
                SwarmMessageIntent::CoordinationNote,
                format!("Note {}", i),
                format!("Payload {}", i),
            );
            assert!(bus.post_message(msg).is_ok());
        }

        // 4th message should fail quota
        let msg4 = SwarmMessage::new(
            "swarm-123",
            "t1_worker",
            None,
            SwarmMessageIntent::CoordinationNote,
            "Note 4",
            "Payload 4",
        );
        let res = bus.post_message(msg4);
        assert!(matches!(res, Err(SwarmError::MessageQuotaExceeded(_))));
    }

    #[test]
    fn test_bus_payload_limit() {
        let dir = tempdir().unwrap();
        let bus = SwarmMessageBus::new(dir.path()).unwrap();

        let big_payload = "A".repeat(801);
        let msg = SwarmMessage::new(
            "swarm-123",
            "t1_worker",
            None,
            SwarmMessageIntent::CoordinationNote,
            "Big",
            big_payload,
        );
        let res = bus.post_message(msg);
        assert!(matches!(res, Err(SwarmError::MessagePayloadTooLarge(_))));
    }

    #[test]
    fn test_bus_self_message_rejected() {
        let dir = tempdir().unwrap();
        let bus = SwarmMessageBus::new(dir.path()).unwrap();

        let msg = SwarmMessage::new(
            "swarm-123",
            "t1_worker",
            Some("t1_worker"),
            SwarmMessageIntent::CoordinationNote,
            "Self",
            "Hello me",
        );
        let res = bus.post_message(msg);
        assert!(matches!(res, Err(SwarmError::SelfMessageNotAllowed)));
    }
}
```

- [ ] **Step 3: Run test to verify it fails**

Run: `cargo test -j 1 --lib agent::swarm::bus::tests`
Expected: FAIL (module not found / functions not implemented)

- [ ] **Step 4: Implement `SwarmMessageIntent`, `SwarmMessage`, and `SwarmMessageBus`**

In `src/agent/swarm/bus.rs`:
```rust
//! Durable, process-safe Swarm Message Bus for cross-worker inter-agent messaging.

use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use crate::agent::swarm::models::SwarmError;

/// Maximum payload length allowed in a single peer message.
pub const MAX_SWARM_MESSAGE_PAYLOAD_LEN: usize = 800;

/// Maximum outgoing messages a single worker may send during a wave.
pub const MAX_SWARM_WORKER_MESSAGES_PER_WAVE: usize = 3;

/// Typed classification of inter-agent messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SwarmMessageIntent {
    /// Exposing exported function signatures, structs, types, or API endpoints.
    PublishContract,
    /// Asking a peer worker for the signature or contract of an uncommitted symbol.
    QueryInterface,
    /// Sharing a critical environmental or architectural discovery with peers.
    CoordinationNote,
}

impl SwarmMessageIntent {
    pub fn badge(&self) -> &'static str {
        match self {
            Self::PublishContract => "📜 Contract",
            Self::QueryInterface => "❓ Query",
            Self::CoordinationNote => "📢 Note",
        }
    }
}

/// A structured peer message exchanged between workers in a swarm wave.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SwarmMessage {
    pub id: String,
    pub swarm_id: String,
    pub from_task: String,
    pub to_task: Option<String>,
    pub intent: SwarmMessageIntent,
    pub topic: String,
    pub payload: String,
    pub timestamp: String,
}

impl SwarmMessage {
    pub fn new(
        swarm_id: impl Into<String>,
        from_task: impl Into<String>,
        to_task: Option<impl Into<String>>,
        intent: SwarmMessageIntent,
        topic: impl Into<String>,
        payload: impl Into<String>,
    ) -> Self {
        let from_task_str = from_task.into();
        let now_millis = Utc::now().timestamp_millis();
        let unique_suffix = uuid::Uuid::new_v4().to_string();
        let short_suffix = &unique_suffix[..6];
        let id = format!("msg-{}-{}-{}", from_task_str, now_millis, short_suffix);

        Self {
            id,
            swarm_id: swarm_id.into(),
            from_task: from_task_str,
            to_task: to_task.map(|t| t.into()),
            intent,
            topic: topic.into(),
            payload: payload.into(),
            timestamp: Utc::now().to_rfc3339(),
        }
    }

    /// Formats the message into an XML context block for turn-start ingestion.
    pub fn format_for_prompt(&self) -> String {
        let recipient_str = self
            .to_task
            .as_deref()
            .unwrap_or("all_peers (broadcast)");
        format!(
            "<peer_message id=\"{}\" from=\"{}\" to=\"{}\" intent=\"{}\" topic=\"{}\" time=\"{}\">\n{}\n</peer_message>",
            self.id,
            self.from_task,
            recipient_str,
            serde_json::to_string(&self.intent).unwrap_or_default().trim_matches('"'),
            self.topic,
            self.timestamp,
            self.payload.trim()
        )
    }
}

/// Durable, process-safe Swarm Message Bus.
#[derive(Debug, Clone)]
pub struct SwarmMessageBus {
    bus_path: PathBuf,
}

impl SwarmMessageBus {
    pub fn new(swarm_dir: &Path) -> Result<Self, SwarmError> {
        fs::create_dir_all(swarm_dir)?;
        let bus_path = swarm_dir.join("bus.jsonl");
        Ok(Self { bus_path })
    }

    pub fn bus_path(&self) -> &Path {
        &self.bus_path
    }

    /// Appends a new message atomically to the bus with strict quota and size validation.
    pub fn post_message(&self, msg: SwarmMessage) -> Result<(), SwarmError> {
        if msg.payload.len() > MAX_SWARM_MESSAGE_PAYLOAD_LEN {
            return Err(SwarmError::MessagePayloadTooLarge(msg.payload.len()));
        }

        if let Some(ref recipient) = msg.to_task {
            if recipient == &msg.from_task {
                return Err(SwarmError::SelfMessageNotAllowed);
            }
        }

        let all = self.all_messages()?;
        let sender_count = all
            .iter()
            .filter(|m| m.from_task == msg.from_task)
            .count();

        if sender_count >= MAX_SWARM_WORKER_MESSAGES_PER_WAVE {
            return Err(SwarmError::MessageQuotaExceeded(msg.from_task));
        }

        let mut line = serde_json::to_string(&msg)?;
        line.push('\n');

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.bus_path)?;

        file.write_all(line.as_bytes())?;
        file.sync_data()?;

        Ok(())
    }

    /// Reads all messages posted after `last_seen_id` targeted to `task_id` or broadcasted.
    pub fn read_unread(
        &self,
        task_id: &str,
        last_seen_id: Option<&str>,
    ) -> Result<Vec<SwarmMessage>, SwarmError> {
        let all = self.all_messages()?;
        let mut past_cursor = last_seen_id.is_none();
        let mut unread = Vec::new();

        for msg in all {
            if !past_cursor {
                if Some(msg.id.as_str()) == last_seen_id {
                    past_cursor = true;
                }
                continue;
            }

            // Exclude messages authored by the caller
            if msg.from_task == task_id {
                continue;
            }

            // Include if addressed to task_id or broadcast (None)
            let is_recipient = match &msg.to_task {
                Some(recipient) => recipient == task_id,
                None => true,
            };

            if is_recipient {
                unread.push(msg);
            }
        }

        Ok(unread)
    }

    /// Reads all historical messages from `bus.jsonl`.
    pub fn all_messages(&self) -> Result<Vec<SwarmMessage>, SwarmError> {
        if !self.bus_path.exists() {
            return Ok(Vec::new());
        }

        let file = fs::File::open(&self.bus_path)?;
        let reader = BufReader::new(file);
        let mut messages = Vec::new();

        for line_res in reader.lines() {
            let line = line_res?;
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if let Ok(msg) = serde_json::from_str::<SwarmMessage>(trimmed) {
                messages.push(msg);
            }
        }

        Ok(messages)
    }
}
```

- [ ] **Step 5: Export `bus` module in `src/agent/swarm/mod.rs`**

```rust
pub mod bus;
pub use bus::{SwarmMessage, SwarmMessageBus, SwarmMessageIntent};
```

- [ ] **Step 6: Run tests and verify GREEN**

Run: `cargo test -j 1 --lib agent::swarm::bus::tests`
Expected: PASS (4/4 tests pass)

- [ ] **Step 7: Format and Clippy**

Run: `cargo fmt`
Run: `cargo clippy -j 1 --bin minicode -- -D warnings`

- [ ] **Step 8: Commit**

```bash
git add src/agent/swarm/bus.rs src/agent/swarm/mod.rs src/agent/swarm/models.rs
git commit -m "feat(swarm): implement durable process-safe SwarmMessageBus and typed models"
```

---

### Task 2: Worker Environment Propagation in Scheduler

**Files:**
- Modify: `src/agent/swarm/scheduler.rs:80-145,246-375`
- Test: `tests/integration_swarm_messaging.rs`

**Interfaces:**
- Consumes: `SwarmMessageBus` from `src/agent/swarm/bus.rs`, `SwarmPlan` from `src/agent/swarm/models.rs`
- Produces: Environment variables `MINICODE_SWARM_ID`, `MINICODE_SWARM_DIR`, `MINICODE_SWARM_TASK_ID`, and `MINICODE_SWARM_PEERS` injected into worker child processes.

- [ ] **Step 1: Write test verifying environment variables are computed for worker tasks**

In `src/agent/swarm/scheduler.rs` (under `#[cfg(test)] mod tests`):
```rust
    #[test]
    fn test_compute_active_peers() {
        let task_ids = vec!["t1".to_string(), "t2".to_string(), "t3".to_string()];
        let peers_for_t1 = task_ids
            .iter()
            .filter(|id| *id != "t1")
            .cloned()
            .collect::<Vec<_>>()
            .join(",");
        assert_eq!(peers_for_t1, "t2,t3");
    }
```

- [ ] **Step 2: Update `SwarmScheduler::execute_plan` to calculate wave peers and initialize bus**

In `src/agent/swarm/scheduler.rs`:
```rust
        // Initialize SwarmMessageBus
        let _bus = crate::agent::swarm::bus::SwarmMessageBus::new(&swarm_dir)?;
```

When building ready tasks, collect the ready task IDs in the current wave and pass them to `run_worker_task`:
```rust
        let ready_ids_list = ready_tasks.iter().map(|t| t.id.clone()).collect::<Vec<_>>();
```

- [ ] **Step 3: In `run_worker_task`, inject swarm environment variables into child process**

In `src/agent/swarm/scheduler.rs`:
```rust
        let peers_str = active_wave_peers
            .iter()
            .filter(|id| *id != &task.id)
            .cloned()
            .collect::<Vec<_>>()
            .join(",");

        cmd.env("MINICODE_SWARM_ID", &plan.id);
        cmd.env("MINICODE_SWARM_DIR", &swarm_dir);
        cmd.env("MINICODE_SWARM_TASK_ID", &task.id);
        cmd.env("MINICODE_SWARM_PEERS", &peers_str);
```

- [ ] **Step 4: Run targeted tests**

Run: `cargo test -j 1 --lib agent::swarm::scheduler::tests`
Expected: PASS

- [ ] **Step 5: Format and Clippy**

Run: `cargo fmt`
Run: `cargo clippy -j 1 --bin minicode -- -D warnings`

- [ ] **Step 6: Commit**

```bash
git add src/agent/swarm/scheduler.rs
git commit -m "feat(swarm): propagate swarm ID, directory, task ID, and peer list via env vars"
```

---

### Task 3: Turn-Start Reactive Ingestion in Agent Loop

**Files:**
- Modify: `src/agent/loop.rs`
- Modify: `src/agent/prompt.rs`
- Test: `tests/integration_swarm_messaging.rs`

**Interfaces:**
- Consumes: `SwarmMessageBus` and `SwarmMessage` from `src/agent/swarm/bus.rs`
- Produces: `<peer_messages>` context block dynamically ingested into LLM prompt at turn start.

- [ ] **Step 1: Write unit test for turn-start message context block generation**

In `src/agent/prompt.rs` (under tests):
```rust
    #[test]
    fn test_format_peer_messages_block() {
        let msg = crate::agent::swarm::bus::SwarmMessage::new(
            "swarm-1",
            "t1_api",
            None,
            crate::agent::swarm::bus::SwarmMessageIntent::PublishContract,
            "User Schema",
            "export interface User { id: string; }",
        );
        let block = crate::agent::prompt::format_peer_messages_prompt(&[msg]);
        assert!(block.contains("<peer_messages>"));
        assert!(block.contains("export interface User"));
        assert!(block.contains("</peer_messages>"));
    }
```

- [ ] **Step 2: Implement `format_peer_messages_prompt` in `src/agent/prompt.rs`**

```rust
/// Formats unread peer messages into a structured XML block for turn-start context injection.
pub fn format_peer_messages_prompt(messages: &[crate::agent::swarm::bus::SwarmMessage]) -> String {
    if messages.is_empty() {
        return String::new();
    }

    let mut out = String::from("\n<peer_messages>\n### INCOMING PEER WORKER COORDINATION MESSAGES:\n");
    for msg in messages {
        out.push_str(&msg.format_for_prompt());
        out.push('\n');
    }
    out.push_str("</peer_messages>\n");
    out
}
```

- [ ] **Step 3: Add `last_seen_swarm_message_id` to `AgentLoop` state**

In `src/agent/loop.rs`:
Add field `last_seen_swarm_msg_id: Option<String>` to `AgentLoop`.
In `AgentLoop::execute_turn`:
```rust
        // Swarm Turn-Start Ingestion: Check for unread peer messages
        if let (Ok(swarm_dir_str), Ok(task_id)) = (
            std::env::var("MINICODE_SWARM_DIR"),
            std::env::var("MINICODE_SWARM_TASK_ID"),
        ) {
            let swarm_dir = std::path::Path::new(&swarm_dir_str);
            if let Ok(bus) = crate::agent::swarm::bus::SwarmMessageBus::new(swarm_dir) {
                if let Ok(unread) = bus.read_unread(&task_id, self.last_seen_swarm_msg_id.as_deref()) {
                    if !unread.is_empty() {
                        if let Some(newest) = unread.last() {
                            self.last_seen_swarm_msg_id = Some(newest.id.clone());
                        }
                        let peer_block = crate::agent::prompt::format_peer_messages_prompt(&unread);
                        system_prompt.push_str(&peer_block);
                    }
                }
            }
        }
```

- [ ] **Step 4: Run targeted tests**

Run: `cargo test -j 1 --lib agent::prompt::tests`
Expected: PASS

- [ ] **Step 5: Format and Clippy**

Run: `cargo fmt`
Run: `cargo clippy -j 1 --bin minicode -- -D warnings`

- [ ] **Step 6: Commit**

```bash
git add src/agent/loop.rs src/agent/prompt.rs
git commit -m "feat(agent): implement autonomous turn-start peer message ingestion"
```

---

### Task 4: Tool Schema & Dispatch Upgrade for `send_worker_message`

**Files:**
- Modify: `src/tools/registry/agent_tools/swarms.rs:9-50,125-170`
- Test: `tests/integration_swarm_messaging.rs`

**Interfaces:**
- Consumes: `SwarmMessageBus`, `SwarmMessageIntent`, `SwarmMessage`
- Produces: Upgraded `send_worker_message` tool handling typed intents, auto-inferring `from_worker_id`, and writing to `SwarmMessageBus` when in swarm mode.

- [ ] **Step 1: Write test for `send_worker_message` schema and dispatch**

In `src/tools/registry/agent_tools/swarms.rs` (under tests):
```rust
    #[test]
    fn test_send_worker_message_schema() {
        let schemas = get_schemas();
        let tool = schemas.iter().find(|s| s.name == "send_worker_message").unwrap();
        assert!(tool.parameters["properties"]["intent"].is_object());
        assert!(tool.parameters["properties"]["payload"].is_object());
    }
```

- [ ] **Step 2: Update `send_worker_message` tool schema**

In `src/tools/registry/agent_tools/swarms.rs`:
Upgrade parameters to:
- `to_worker_id`: optional string (recipient or null for broadcast)
- `intent`: string enum (`publish_contract`, `query_interface`, `coordination_note`)
- `topic`: string (short summary)
- `payload`: string (content snippet, max 800 chars)
- `from_worker_id`: optional string (inferred from `MINICODE_SWARM_TASK_ID` if omitted)

- [ ] **Step 3: Update `send_worker_message` dispatch logic**

In `src/tools/registry/agent_tools/swarms.rs`:
```rust
        "send_worker_message" => Some(
            async {
                let swarm_dir_env = std::env::var("MINICODE_SWARM_DIR").ok();
                let swarm_task_env = std::env::var("MINICODE_SWARM_TASK_ID").ok();
                let swarm_id_env = std::env::var("MINICODE_SWARM_ID").unwrap_or_else(|_| "swarm-standalone".to_string());

                let from = param::opt_str(args, "from_worker_id")
                    .or(swarm_task_env.as_deref())
                    .unwrap_or("worker");

                let to = param::opt_str(args, "to_worker_id").filter(|s| !s.is_empty());
                let topic = param::require_str(args, "topic", "send_worker_message")?;
                let payload = param::require_str(args, "payload", "send_worker_message")?;

                let intent_str = param::opt_str(args, "intent").unwrap_or("coordination_note");
                let intent = match intent_str {
                    "publish_contract" => crate::agent::swarm::bus::SwarmMessageIntent::PublishContract,
                    "query_interface" => crate::agent::swarm::bus::SwarmMessageIntent::QueryInterface,
                    _ => crate::agent::swarm::bus::SwarmMessageIntent::CoordinationNote,
                };

                if let Some(swarm_dir_str) = swarm_dir_env {
                    let bus = crate::agent::swarm::bus::SwarmMessageBus::new(std::path::Path::new(&swarm_dir_str))
                        .map_err(|e| ToolError::CommandExec(e.to_string()))?;

                    let msg = crate::agent::swarm::bus::SwarmMessage::new(
                        swarm_id_env,
                        from,
                        to,
                        intent,
                        topic,
                        payload,
                    );

                    bus.post_message(msg.clone())
                        .map_err(|e| ToolError::CommandExec(e.to_string()))?;

                    let dest = to
                        .map(|t| format!("to peer worker `{}`", t))
                        .unwrap_or_else(|| "as wave broadcast".to_string());

                    Ok(format!(
                        "✔ Message `{}` posted {} [{}]: `{}`",
                        msg.id, dest, intent.badge(), msg.topic
                    ))
                } else {
                    // Fallback to in-memory bus for standalone non-swarm executions
                    let bus = crate::agent::subagent::get_global_message_bus();
                    let msg = bus.send_message(from, to, topic, payload);
                    let dest = to
                        .map(|t| format!("to worker `{}`", t))
                        .unwrap_or_else(|| "as swarm broadcast".to_string());
                    Ok(format!("✔ Message `{}` posted {} on topic `{}`.", msg.id, dest, msg.topic))
                }
            }
            .await,
        ),
```

- [ ] **Step 4: Verify `TOTAL_TOOL_COUNT = 186`**

Run: `cargo test -j 1 --lib constants::tool_count_validation::total_tool_count_matches_registry`
Expected: PASS (186 tools)

- [ ] **Step 5: Format and Clippy**

Run: `cargo fmt`
Run: `cargo clippy -j 1 --bin minicode -- -D warnings`

- [ ] **Step 6: Commit**

```bash
git add src/tools/registry/agent_tools/swarms.rs
git commit -m "feat(tools): upgrade send_worker_message with typed intents and durable bus routing"
```

---

### Task 5: Swarm Reporter Log Integration & End-to-End Integration Suite

**Files:**
- Modify: `src/agent/swarm/report.rs:1-120`
- Create: `tests/integration_swarm_messaging.rs`
- Modify: `onpkg_docs/core/todo.md`

**Interfaces:**
- Consumes: `SwarmMessageBus::all_messages()`, `SwarmReporter::save_report`
- Produces: `## 💬 Inter-Worker Coordination Log` table in executive report, passing integration test suite.

- [ ] **Step 1: In `SwarmReporter::generate_report_markdown`, add Inter-Worker Coordination Log table**

In `src/agent/swarm/report.rs`:
```rust
        let bus = crate::agent::swarm::bus::SwarmMessageBus::new(&swarm_dir).ok();
        let messages = bus.and_then(|b| b.all_messages().ok()).unwrap_or_default();

        if !messages.is_empty() {
            report.push_str("## 💬 Inter-Worker Coordination Log\n\n");
            report.push_str("| Timestamp | From | To | Intent | Topic | Preview |\n");
            report.push_str("| :--- | :--- | :--- | :--- | :--- | :--- |\n");
            for msg in &messages {
                let target = msg.to_task.as_deref().unwrap_or("Wave Broadcast");
                let time_fmt = msg.timestamp.split('T').nth(1).unwrap_or(&msg.timestamp);
                let clean_time = time_fmt.split('.').next().unwrap_or(time_fmt);
                let preview = msg.payload.replace('\n', " ");
                let truncated_preview = if preview.len() > 60 {
                    format!("{}...", &preview[..57])
                } else {
                    preview
                };
                report.push_str(&format!(
                    "| `{}` | `{}` | `{}` | {} | {} | `{}` |\n",
                    clean_time, msg.from_task, target, msg.intent.badge(), msg.topic, truncated_preview
                ));
            }
            report.push('\n');
        }
```

- [ ] **Step 2: Create `tests/integration_swarm_messaging.rs` with comprehensive cross-process tests**

```rust
use minicode::agent::swarm::bus::{SwarmMessage, SwarmMessageBus, SwarmMessageIntent};
use minicode::agent::swarm::models::SwarmError;
use tempfile::tempdir;

#[tokio::test]
async fn test_concurrent_cross_worker_messaging() {
    let dir = tempdir().unwrap();
    let swarm_dir = dir.path().to_path_buf();
    let bus = SwarmMessageBus::new(&swarm_dir).unwrap();

    let mut handles = Vec::new();
    for i in 0..3 {
        let bus_clone = bus.clone();
        let sender = format!("worker_{}", i);
        handles.push(tokio::spawn(async move {
            let msg = SwarmMessage::new(
                "swarm-conc",
                &sender,
                None,
                SwarmMessageIntent::PublishContract,
                format!("Topic {}", i),
                format!("Payload from {}", sender),
            );
            bus_clone.post_message(msg).unwrap();
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    let all = bus.all_messages().unwrap();
    assert_eq!(all.len(), 3);

    // Worker 0 should see messages from worker_1 and worker_2
    let unread_w0 = bus.read_unread("worker_0", None).unwrap();
    assert_eq!(unread_w0.len(), 2);
    assert!(!unread_w0.iter().any(|m| m.from_task == "worker_0"));
}

#[tokio::test]
async fn test_quota_and_payload_limits() {
    let dir = tempdir().unwrap();
    let bus = SwarmMessageBus::new(dir.path()).unwrap();

    // 1. Payload > 800 chars rejected
    let big = "X".repeat(801);
    let msg_big = SwarmMessage::new("s", "w1", None, SwarmMessageIntent::CoordinationNote, "T", big);
    assert!(matches!(bus.post_message(msg_big), Err(SwarmError::MessagePayloadTooLarge(_))));

    // 2. Max 3 messages allowed
    for i in 0..3 {
        let msg = SwarmMessage::new("s", "w1", None, SwarmMessageIntent::CoordinationNote, format!("T{}", i), "P");
        assert!(bus.post_message(msg).is_ok());
    }

    let msg_overflow = SwarmMessage::new("s", "w1", None, SwarmMessageIntent::CoordinationNote, "T_over", "P");
    assert!(matches!(bus.post_message(msg_overflow), Err(SwarmError::MessageQuotaExceeded(_))));
}

#[tokio::test]
async fn test_direct_addressing_privacy() {
    let dir = tempdir().unwrap();
    let bus = SwarmMessageBus::new(dir.path()).unwrap();

    let direct = SwarmMessage::new(
        "s",
        "w1",
        Some("w2"),
        SwarmMessageIntent::QueryInterface,
        "Secret Interface Query",
        "How to call fn bar()?",
    );
    bus.post_message(direct).unwrap();

    // w2 sees it
    let unread_w2 = bus.read_unread("w2", None).unwrap();
    assert_eq!(unread_w2.len(), 1);

    // w3 does NOT see it (addressed directly to w2)
    let unread_w3 = bus.read_unread("w3", None).unwrap();
    assert_eq!(unread_w3.len(), 0);
}
```

- [ ] **Step 3: Run targeted integration tests**

Run: `cargo test -j 1 --test integration_swarm_messaging`
Expected: PASS (all 3 tests pass)

- [ ] **Step 4: Update `onpkg_docs/core/todo.md`**

Mark Phase 152 tasks complete in `onpkg_docs/core/todo.md`.

- [ ] **Step 5: Format and Clippy verification**

Run: `cargo fmt`
Run: `cargo clippy -j 1 --bin minicode -- -D warnings`
Expected: 0 warnings, 0 diffs.

- [ ] **Step 6: Commit**

```bash
git add src/agent/swarm/report.rs tests/integration_swarm_messaging.rs onpkg_docs/core/todo.md
git commit -m "feat(swarm): add executive report coordination table and cross-worker messaging integration tests"
```

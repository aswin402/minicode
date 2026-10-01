# Swarm Cross-Worker Inter-Agent Messaging Design Specification

> **Phase:** Phase 152 Swarm Cross-Worker Inter-Agent Messaging  
> **Status:** Approved Design Document  
> **Date:** 2026-10-01  
> **Author:** Aswin & Engineering Lead Agent  

---

## 1. Executive Summary & Problem Statement

In `minicode swarm` (`v0.3.43`), multi-agent swarms execute decomposed DAG tasks concurrently across isolated Git worktrees. While upstream dependency artifacts are passed sequentially across execution waves via enriched prompts, **parallel workers executing within the same wave operate without inter-worker communication**.

When two workers in the same wave touch related modules (e.g. Worker A implementing an API service and Worker B implementing an API client, or Worker A creating data schemas and Worker B building query engines), they cannot coordinate on function signatures, exported types, or shared environment ports in flight.

This specification establishes **Swarm Cross-Worker Inter-Agent Messaging**: a durable, process-safe, asynchronous Pub-Sub Event Bus allowing concurrent workers to exchange typed contract definitions, query interfaces, and share coordination findings with zero polling overhead and strict anti-chatter token quotas.

---

## 2. Competitive & Industry Analysis

| Framework | Architecture | Inter-Agent Mechanism | Advantages | Failure Modes & Pitfalls |
| :--- | :--- | :--- | :--- | :--- |
| **MetaGPT** | SOP-Driven Role Network | Pub-Sub Message Pool | Role-based decoupled subscriptions | Rigorous SOP required; static roles |
| **AutoGen** | Conversational Agents | Turn-based Direct Chat | Dynamic conversational negotiation | **$O(N^2)$ quadratic token bloat**, infinite ping-pong loops |
| **CrewAI** | Hierarchical Delegation | Sequential/Manager Handoffs | Clear chain of command | No parallel peer-to-peer collaboration in the same tier |
| **OpenAI Swarms** | Stateless Function Handoffs | `transfer_to_agent` | Simple sequential model | Exactly 1 active agent at a time |
| **minicode Phase 152** | **Multi-Process Git Swarm** | **Durable File Bus + Turn-Start Ingestion** | **Zero polling tool turns, cross-process safe, strict token quotas** | None (bounded payload & max 3 msgs/wave) |

---

## 3. Architecture & Data Flow

### 3.1 Storage Substrate
The message bus is persisted to the swarm's active directory:
```text
.minicode/swarms/<swarm_id>/bus.jsonl
```
Every line in this file is a serialized JSON object of type `SwarmMessage`.

### 3.2 Domain Models (`src/agent/swarm/bus.rs`)

```rust
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

/// A structured peer message exchanged between workers in a swarm wave.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SwarmMessage {
    /// Unique identifier for this message (e.g. "msg-t1-1727829100-01").
    pub id: String,
    /// Swarm run ID this message belongs to.
    pub swarm_id: String,
    /// Sender task ID (e.g. "t1_api_backend").
    pub from_task: String,
    /// Recipient task ID (e.g. "t2_frontend_ui"), or None for wave broadcast.
    pub to_task: Option<String>,
    /// Typed classification of this message.
    pub intent: SwarmMessageIntent,
    /// Compact summary / topic header (e.g. "Exported format_currency signature").
    pub topic: String,
    /// Structured payload content (bounded to max 800 characters).
    pub payload: String,
    /// ISO 8601 UTC timestamp.
    pub timestamp: String,
}

/// Durable, process-safe Swarm Message Bus.
pub struct SwarmMessageBus {
    bus_path: PathBuf,
}
```

### 3.3 Core Operations
- **`post_message(&self, msg: SwarmMessage) -> Result<(), SwarmError>`**:
  1. Validates that `msg.payload.len() <= 800`.
  2. Reads existing messages from `bus.jsonl` to verify sender's current message count `< 3`.
  3. Rejects sending messages to self (`msg.to_task.as_deref() == Some(&msg.from_task)`).
  4. Appends JSON line atomically with `sync_data()` file durability.
- **`read_unread(&self, task_id: &str, last_seen_id: Option<&str>) -> Result<Vec<SwarmMessage>, SwarmError>`**:
  1. Filters messages posted after `last_seen_id`.
  2. Selects messages where `to_task == task_id` or `to_task.is_none()` (broadcasts).
  3. Excludes messages authored by `task_id` itself.
- **`all_messages(&self) -> Result<Vec<SwarmMessage>, SwarmError>`**:
  Reads complete chronological history for executive report compilation.

---

## 4. Multi-Process Environment & Turn-Start Ingestion

### 4.1 Environment Variable Propagation
When `SwarmScheduler` spawns worker child processes (`minicode run -d <worktree> ...`), it passes:
- `MINICODE_SWARM_ID`: Swarm run identifier.
- `MINICODE_SWARM_DIR`: Absolute path to `.minicode/swarms/<swarm_id>`.
- `MINICODE_SWARM_TASK_ID`: Current worker's task ID (e.g. `t1_backend`).
- `MINICODE_SWARM_PEERS`: Comma-separated list of concurrent task IDs in the same wave (e.g. `t2_frontend,t3_tests`).

### 4.2 Turn-Start Autonomous Ingestion (`src/agent/loop.rs`)
In `AgentLoop::execute_turn`:
1. Check if `MINICODE_SWARM_DIR` and `MINICODE_SWARM_TASK_ID` are present.
2. If present, load `SwarmMessageBus` and call `read_unread(&task_id, last_seen_msg_id)`.
3. If new messages exist:
   - Update `last_seen_msg_id` to the latest message ID.
   - Format messages into an XML context block:
     ```xml
     <peer_messages>
       <message from="t1_backend" intent="publish_contract" topic="UserAuth API" timestamp="...">
         export interface UserAuth { id: string; token: string; }
       </message>
     </peer_messages>
     ```
   - Ingest into the turn's prompt context.
4. **Result**: Zero polling tool calls. The worker instantly discovers peer updates at the start of each reasoning iteration.

---

## 5. Tool Dispatch & Anti-Chatter Guardrails

### 5.1 Tool Schema Update (`src/tools/registry/agent_tools/swarms.rs`)
Upgrade `send_worker_message`:
- `to_worker_id` (optional string): Target peer task ID or null for wave broadcast.
- `intent` (enum: `publish_contract`, `query_interface`, `coordination_note`).
- `topic` (string): Short summary title.
- `payload` (string): Maximum 800 characters.

If `from_worker_id` is omitted by the model, it is automatically populated from `MINICODE_SWARM_TASK_ID`.

### 5.2 Anti-Chatter Guardrails
1. **Wave Quota**: Max 3 messages per worker per wave. Returns error `✖ Message quota reached (3/3). Focus on implementing your assigned code boundaries.` if exceeded.
2. **Payload Size Bound**: Max 800 characters (~150-200 tokens). Returns error if exceeded.
3. **Peer Validation**: Verifies recipient exists in active swarm plan.

---

## 6. Audit & Executive Reporting (`src/agent/swarm/report.rs`)

When `SwarmScheduler` finishes executing the plan, `SwarmReporter` reads all messages from `SwarmMessageBus::all_messages()` and appends a dedicated section to `.minicode/swarms/<swarm_id>/report.md`:

```markdown
## 💬 Inter-Worker Coordination Log

| Timestamp | From | To | Intent | Topic | Preview |
| :--- | :--- | :--- | :--- | :--- | :--- |
| 17:15:02 | `t1_backend` | Wave Broadcast | 📜 `publish_contract` | Auth Tokens | `export interface AuthToken...` |
| 17:15:20 | `t2_frontend` | `t1_backend` | ❓ `query_interface` | Refresh Route | `Is refresh endpoint /auth/refresh?` |
```

---

## 7. Quality Invariants & Verification

1. **Tool Count Invariant**: `TOTAL_TOOL_COUNT = 186` in `src/constants.rs` (upgrades `send_worker_message` in place).
2. **Targeted Tests**:
   - Unit tests in `src/agent/swarm/bus.rs`.
   - Integration test in `tests/integration_swarm_messaging.rs`.
3. **Clippy & Formatting**:
   - `cargo clippy -j 1 --bin minicode -- -D warnings` (0 warnings).
   - `cargo fmt --check`.
4. **Pure Rust**:
   - Zero external C libraries.
   - Zero `.unwrap()` or `.expect()` in non-test library code.

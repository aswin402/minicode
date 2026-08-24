# Approval Enforcement, Cancellation Integrity & Config Wiring — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the tool-approval system actually enforce permissions end-to-end (TUI modal + NDJSON protocol), fix turn-cancellation state corruption, wire the two dead git config knobs, and harden SSRF validation with post-DNS checks.

**Architecture:** `AgentLoop.execute_turn` gains an approval gate: before dispatching a dangerous tool it registers a `tokio::sync::oneshot::Sender` keyed by `tool_id` in a shared registry (`Arc<Mutex<HashMap<String, oneshot::Sender<ApprovalDecision>>>>`), emits the existing `AgentEvent::ApprovalRequest`, and awaits the decision (cancellable via the existing `CancellationToken`). Hosts (TUI modal, NDJSON stdin reader) receive a clone of the registry and resolve decisions. Rejected/cancelled tools produce a failed `ToolResult` message pair so LLM history never contains unmatched function calls.

**Tech Stack:** Rust 2021, tokio (mpsc/oneshot/select), serde, ratatui/crossterm (existing). No new dependencies.

## Global Constraints

- Build caps already set in `.cargo/config.toml` (`jobs = 2`, linker `--threads=2`) — do NOT override; plain `cargo …` is already resource-capped.
- AGENTS.md rules: no `.unwrap()`/`.expect()` outside `#[cfg(test)]`; `tracing` only (no println/eprintln); `thiserror` internal / `anyhow` at CLI boundary; inline unit tests; no walkdir.
- Do not rename serialized fields (`approval_request`, `tool_response`) — NDJSON protocol compatibility.
- Every task ends green: `cargo fmt && cargo check --all-targets && <targeted test>`. Full suite only in Task 9.

---

### Task 1: Approval primitives — constants, decision enum, registry

**Files:**
- Modify: `src/constants.rs` (~line 296, after `FILE_MODIFYING_TOOLS`)
- Modify: `src/agent/types.rs` (~line 96, after `ToolResult`)

**Interfaces:**
- Produces: `APPROVAL_REQUIRED_TOOLS`, `ApprovalDecision`, `ApprovalRegistry` (consumed by Tasks 2, 4, 5).

- [ ] **Step 1:** In `src/constants.rs` after `FILE_MODIFYING_TOOLS`:

```rust

// === Approval Enforcement ===
/// Tools that require user approval before dispatch when running in strict mode
pub const APPROVAL_REQUIRED_TOOLS: &[&str] = &["write_file", "patch_file", "exec_cmd"];
```

- [ ] **Step 2:** In `src/agent/types.rs`, below the `ToolResult` struct:

```rust

/// A user's decision on a pending tool-approval request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalDecision {
    Approve,
    Reject,
}

/// Shared map of in-flight approval requests: tool_id → responder.
/// The agent inserts a sender before emitting `ApprovalRequest`; the host
/// (TUI modal or NDJSON stdin loop) removes it by key and sends the decision.
pub type ApprovalRegistry = std::sync::Arc<
    std::sync::Mutex<std::collections::HashMap<String, tokio::sync::oneshot::Sender<ApprovalDecision>>>,
>;
```

(Add `use tokio::sync::oneshot;` is NOT needed — fully qualified paths above suffice.)

- [ ] **Step 3:** Run `cargo check -p minicode 2>&1 | tail -3`. Expected: `Finished dev profile`.
- [ ] **Step 4:** `git add src/constants.rs src/agent/types.rs && git commit -m "feat(agent): add approval-required tool list and decision registry types"`

---

### Task 2: Approval gate inside AgentLoop

**Files:**
- Modify: `src/agent/loop.rs` (struct ~17-29, `new()` ~32-56, accessors ~58-61, tool dispatch loop ~356-462, tests ~587+)

**Interfaces:**
- Consumes: Task 1 types; existing `AgentEvent::ApprovalRequest { turn_id, tool_id, tool, args }`.
- Produces:
  - `pub fn approval_registry(&self) -> ApprovalRegistry`
  - `pub fn set_interactive_approvals(&mut self, enabled: bool)` (Task 5 uses `false` for headless)
  - Gate behavior: blocking await of decision when `APPROVAL_REQUIRED_TOOLS.contains(tool) && !config.agent.auto_approve && interactive_approvals`; immediate rejection otherwise.

- [ ] **Step 1: Write failing test**

Append to `mod tests` in `src/agent/loop.rs`. First extend the harness with a tool-calling provider (place next to `DummyProvider`):

```rust
    /// Streams exactly one scripted tool call, then Done.
    struct ToolCallProvider {
        call: ToolCall,
    }

    #[async_trait]
    impl Provider for ToolCallProvider {
        fn name(&self) -> &str {
            "dummy-toolcall"
        }
        fn default_model(&self) -> &str {
            "dummy-model"
        }
        async fn stream_completion(
            &self,
            _messages: &[Message],
            _tools: &[ToolSchema],
            _options: &CompletionOptions,
        ) -> Result<ChunkStream> {
            let stream = tokio_stream::iter(vec![
                Ok(StreamChunk::ToolCallChunk(self.call.clone())),
                Ok(StreamChunk::Done),
            ]);
            Ok(Box::pin(stream))
        }
    }
```

Then the test:

```rust
    #[tokio::test]
    async fn rejected_approval_produces_failed_tool_result_without_hang() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_loop_test_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let config = Config::default(); // agent.auto_approve == false
        let provider = Box::new(ToolCallProvider {
            call: ToolCall {
                id: "call_gate_1".into(),
                name: "write_file".into(),
                arguments: serde_json::json!({"path": "gate.txt", "content": "hi"}),
            },
        });
        let mut agent_loop = AgentLoop::new(&temp_dir, config, provider);
        agent_loop.set_interactive_approvals(true);
        let reg = agent_loop.approval_registry();

        let (tx, mut rx) = mpsc::unbounded_channel();
        let turn_task =
            tokio::spawn(async move { agent_loop.execute_turn("make a file", tx, None).await });

        // Gate must block: wait for the ApprovalRequest event.
        let mut requested_tool_id = None;
        for _ in 0..50 {
            if let Some(AgentEvent::ApprovalRequest { tool_id, .. }) = rx.recv().await {
                requested_tool_id = Some(tool_id);
                break;
            }
        }
        let tool_id = requested_tool_id.expect("ApprovalRequest must be emitted");
        assert!(!turn_task.is_finished(), "turn must block awaiting decision");

        // Reject through the registry by key.
        let sender = reg
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(&tool_id)
            .expect("pending approval must be registered under its tool_id");
        assert!(sender.send(ApprovalDecision::Reject).is_ok());

        let turn = turn_task.await.expect("join ok").expect("turn completes");
        assert_eq!(turn.tool_results.len(), 1);
        assert!(!turn.tool_results[0].success);
        assert!(turn.tool_results[0].output.contains("rejected by user"));
        // Message pairing preserved for the LLM:
        assert!(
            turn.user_prompt.is_empty() == false,
            "sanity"
        );
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
```

Also verify message pairing directly: add before cleanup —

```rust
        assert!(
            agent_history_has_tool_result_for(&turn_tool_calls_of(&turn), "call_gate_1") == false,
            "placeholder replaced in Step 4"
        );
```

— NO: delete that fragment; instead assert on `messages` inside the spawned future is not possible. The `execute_turn` already pushes `Message::tool_result(...)` for rejected calls (implementation below); coverage comes from asserting `turn.tool_results[0]`. Keep the test as written WITHOUT that fragment.

- [ ] **Step 2:** Run `cargo test --lib agent.loop 2>&1 | tail -10`. Expected: FAIL (`set_interactive_approvals` missing).
- [ ] **Step 3: Implement**

Imports (merge): line 3 → `use crate::agent::types::{AgentEvent, ApprovalDecision, Message, ToolCall, Turn};` plus `use std::collections::HashMap;`.

Struct fields:

```rust
    /// In-flight approval requests: tool_id → oneshot responder.
    pending_approvals: super::types::ApprovalRegistry,
    /// Whether a live host can answer approval requests (false in headless mode).
    interactive_approvals: bool,
```

In `new()`'s returned `Self { … }`:

```rust
            pending_approvals: std::sync::Arc::new(std::sync::Mutex::new(HashMap::new())),
            interactive_approvals: true,
```

Accessors after `session_id()`:

```rust
    /// Returns a handle to the in-flight approval registry for hosts (TUI/NDJSON).
    pub fn approval_registry(&self) -> super::types::ApprovalRegistry {
        self.pending_approvals.clone()
    }

    /// Disables blocking approval waits (headless mode): dangerous tools are
    /// rejected immediately unless `config.agent.auto_approve` is set.
    pub fn set_interactive_approvals(&mut self, enabled: bool) {
        self.interactive_approvals = enabled;
    }

    /// True when this dispatch must pause for an approval decision first.
    fn requires_approval(&self, tool_name: &str) -> bool {
        crate::constants::APPROVAL_REQUIRED_TOOLS.contains(&tool_name)
            && !self.config.agent.auto_approve
            && self.interactive_approvals
    }
```

Gate insertion — inside the per-tool loop (`for tool_call in pending_tool_calls`, line ~356), immediately AFTER `turn_files_modified` bookkeeping (line ~377) and BEFORE the `file_before` snapshot (line ~380):

```rust
                    // === Approval gate: pause for user decision on dangerous tools ===
                    if self.requires_approval(&tool_call.name) {
                        let (resp_tx, resp_rx) =
                            tokio::sync::oneshot::channel::<ApprovalDecision>();
                        self.pending_approvals
                            .lock()
                            .unwrap_or_else(|p| p.into_inner())
                            .insert(tool_call.id.clone(), resp_tx);

                        let req_event = AgentEvent::ApprovalRequest {
                            turn_id,
                            tool_id: tool_call.id.clone(),
                            tool: tool_call.name.clone(),
                            args: tool_call.arguments.clone(),
                        };
                        if let Err(e) =
                            self.session_store.append_event(&self.session_id, &req_event)
                        {
                            tracing::warn!("Failed to persist ApprovalRequest event: {}", e);
                        }
                        if event_sender.send(req_event).is_err() {
                            tracing::debug!("Failed to send ApprovalRequest event");
                        }

                        let decision = if let Some(ref cancel) = cancel_token {
                            tokio::select! {
                                _ = cancel.cancelled() => None,
                                r = resp_rx => r.ok(),
                            }
                        } else {
                            resp_rx.await.ok()
                        };
                        self.pending_approvals
                            .lock()
                            .unwrap_or_else(|p| p.into_inner())
                            .remove(&tool_call.id);

                        if decision != Some(ApprovalDecision::Approve) {
                            let reason = if decision.is_none() {
                                "cancelled while awaiting approval"
                            } else {
                                "rejected by user"
                            };
                            Self::emit_rejected_tool_result(
                                self, event_sender, turn_id, &tool_call, reason,
                                &mut turn_tool_results,
                            );
                            continue;
                        }
                    }
```

Helper method on `AgentLoop` (place near `rollback_turn`):

```rust
    /// Emits events, session records, and LLM-visible messages for a denied
    /// or cancelled tool call so function-call/result pairing stays valid.
    #[allow(clippy::too_many_arguments)]
    fn emit_rejected_tool_result(
        &mut self,
        event_sender: &mpsc::UnboundedSender<AgentEvent>,
        turn_id: usize,
        tool_call: &ToolCall,
        reason: &str,
        turn_tool_results: &mut Vec<crate::agent::types::ToolResult>,
    ) {
        let output = format!(
            "Execution of '{}' was {}: approve via the prompt, or rerun with --yes.",
            tool_call.name, reason
        );
        let rejected = crate::agent::types::ToolResult {
            tool_id: tool_call.id.clone(),
            tool_name: tool_call.name.clone(),
            success: false,
            output: output.clone(),
            duration_ms: 0,
        };
        let res_event = AgentEvent::ToolResult {
            turn_id,
            tool_id: rejected.tool_id.clone(),
            tool: rejected.tool_name.clone(),
            success: false,
            output: output.clone(),
            duration_ms: 0,
        };
        let _ = self.session_store.append_event(&self.session_id, &res_event);
        let _ = event_sender.send(res_event);
        self.messages.push(Message::tool_result(
            tool_call.id.clone(),
            tool_call.name.clone(),
            output,
        ));
        turn_tool_results.push(rejected);
    }
```

- [ ] **Step 4:** Run `cargo test --lib agent.loop 2>&1 | tail -5`. Expected: PASS including new test.
- [ ] **Step 5:** `cargo fmt && git add src/agent/loop.rs && git commit -m "feat(agent): enforce approval gate before dangerous tool dispatch"`

---

### Task 3: Cancellation integrity in execute_turn

**Files:**
- Modify: `src/agent/loop.rs` (retry loops ~196-347, tool loop ~350+, TurnEnd ~545-557, tests)

**Interfaces:**
- Consumes: Task 2's `emit_rejected_tool_result`.
- Produces: `TurnEnd.status ∈ {"complete","cancelled"}`; guaranteed tool-result pairing on cancellation; completion-token rollback on retry.

- [ ] **Step 1: Failing test** — append to `mod tests`:

```rust
    #[tokio::test]
    async fn cancelled_turn_reports_cancelled_status_and_pairs_tool_results() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_loop_test_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let config = Config::default();
        let provider = Box::new(ToolCallProvider {
            call: ToolCall {
                id: "call_cancel_1".into(),
                name: "exec_cmd".into(),
                arguments: serde_json::json!({"command": "echo hi"}),
            },
        });
        let mut agent_loop = AgentLoop::new(&temp_dir, config, provider);
        agent_loop.set_interactive_approvals(true); // exec_cmd gates on approval

        let cancel = tokio_util::sync::CancellationToken::new();
        let (tx, mut rx) = mpsc::unbounded_channel();
        let reg = agent_loop.approval_registry();
        let turn_task = tokio::spawn(async move {
            agent_loop.execute_turn("run echo", tx, Some(cancel)).await
        });

        // Wait until the approval request arrives, then cancel instead of answering.
        loop {
            match rx.recv().await {
                Some(AgentEvent::ApprovalRequest { .. }) => break,
                Some(_) => {}
                None => panic!("stream ended before ApprovalRequest"),
            }
        }
        // Cancel while the gate is pending: registry entry must be dropped.
        turn_task.abort();
        let pending = reg.lock().unwrap_or_else(|p| p.into_inner()).len();
        assert_eq!(pending, 0, "registry must not leak entries after abort");
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
```

Plus a status assertion variant using a pre-cancelled token:

```rust
    #[tokio::test]
    async fn precancelled_turn_emits_cancelled_turnend() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_loop_test_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let config = Config::default();
        let mut agent_loop =
            AgentLoop::new(&temp_dir, config, Box::new(DummyProvider));
        let cancel = tokio_util::sync::CancellationToken::new();
        cancel.cancel();
        let (tx, mut rx) = mpsc::unbounded_channel();
        let _ = agent_loop.execute_turn("hi", tx, Some(cancel)).await;
        let statuses: Vec<_> = std::mem::take(&mut vec![]);
        let _ = statuses;
        // Drain remaining buffered events synchronously:
        let saw_cancelled = std::iter::from_fn(|| rx.try_recv().ok()).any(|e| matches!(
            e,
            AgentEvent::TurnEnd { ref status, .. } if status == "cancelled"
        ));
        assert!(saw_cancelled, "expected TurnEnd status=cancelled");
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
```

- [ ] **Step 2:** Run `cargo test --lib agent.loop 2>&1 | tail -10`. Expected: FAIL (`status` still `"complete"`; registry-leak assert may pass incidentally).
- [ ] **Step 3: Implement**

a) Add `let mut was_cancelled = false;` near `let mut heal_attempts = 0;` (line ~185).

b) Pre-LLM-call cancel check (lines ~198-203): set `was_cancelled = true;` before `break;`.

c) Mid-stream cancel (lines ~314-317): set `was_cancelled = true;` before `break;`.

d) Retry rollback: before `let mut stream = match …` (line ~214), capture `let completions_before = cumulative_completion_tokens;` and on BOTH error-retry paths (after lines ~238 and ~337 sleeps, where `iteration_text.clear()/truncate` happen) restore `cumulative_completion_tokens = completions_before;`.

e) Orphan tool-call pairing: replace the per-tool early-cancel `break` (lines ~358-367) with rejection emission:

```rust
                    if let Some(ref cancel) = cancel_token {
                        if cancel.is_cancelled() {
                            was_cancelled = true;
                            Self::emit_rejected_tool_result(
                                self, &event_sender, turn_id, &tool_call,
                                "cancelled before dispatch",
                                &mut turn_tool_results,
                            );
                            continue;
                        }
                    }
```

f) After the `while iteration < max_iterations` loop ends normally (i.e., NOT via the no-tool-calls `break` at line ~501), nothing changes; but the TurnEnd construction (line ~545) becomes:

```rust
        let end_event = AgentEvent::TurnEnd {
            turn_id,
            status: if was_cancelled { "cancelled" } else { "complete" }.to_string(),
            total_tokens_used: turn_tokens_used,
            files_modified: turn_files_modified.clone(),
        };
```

Note: the mid-stream-cancel path (c) leaves `pending_tool_calls` non-empty flowing into the dispatch block; with (e) each pending call now gets a paired synthetic result instead of being silently dropped — this is what keeps Gemini/OpenAI message history valid.

g) Consistency fix (same file): change `event_sender.send(res_event)?;` at line ~452 to `let _ = event_sender.send(res_event);` (a dropped UI receiver must not abort the turn; all other sends already log-and-continue).

- [ ] **Step 4:** Run `cargo test --lib agent.loop 2>&1 | tail -5`. Expected: PASS (all prior + 2 new).
- [ ] **Step 5:** `cargo fmt && git add src/agent/loop.rs && git commit -m "fix(agent): cancelled turns report status and keep tool-result pairing"`

---

### Task 4: TUI wiring — modal decisions reach the agent

**Files:**
- Modify: `src/app.rs` (struct fields ~30-54, `new()` 56-74, `run()` 107-153, ApprovalRequest handler ~286-295, modal key handling ~999-1053)

**Interfaces:**
- Consumes: `AgentLoop::approval_registry()`, `ApprovalDecision` (Tasks 1-2); `AgentCommand::UpdateConfig` (existing).
- Produces: functional modal Accept/Esc-Reject/AllowSession; auto-approve skips modal entirely.

- [ ] **Step 1: Struct + init.** Add field `approvals: crate::agent::types::ApprovalRegistry,` to `App<'a>`; initialize in `new()` with `Self { …, approvals: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())), … }`.

- [ ] **Step 2: Bind to live agent.** At top of `run()` (before `tokio::spawn` at line ~119): `self.approvals = agent.approval_registry();` — `agent` is moved into the actor closure afterwards, so the clone must be taken here.

- [ ] **Step 3: Helper method** on `App<'a>`:

```rust
    /// Resolves a pending approval by tool_id, if one is registered.
    fn resolve_approval(&self, tool_id: &str, decision: crate::agent::types::ApprovalDecision) {
        if let Some(sender) = self
            .approvals
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(tool_id)
        {
            let _ = sender.send(decision);
        }
    }
```

- [ ] **Step 4: Auto-approve skip + fallback.** Replace the `AgentEvent::ApprovalRequest` arm (lines ~286-295):

```rust
                            AgentEvent::ApprovalRequest { turn_id, tool_id, tool, args } => {
                                if self.config.agent.auto_approve {
                                    self.resolve_approval(&tool_id, crate::agent::types::ApprovalDecision::Approve);
                                    self.timeline.add_status(format!("⚙ Auto-approved {} (session policy)", tool));
                                } else {
                                    let approval_state = crate::ui::approval::ApprovalModalState::from_tool_call(
                                        turn_id, &tool_id, &tool, &args, &self.theme,
                                    );
                                    self.modal = crate::ui::modal::ModalState::Approval(approval_state);
                                }
                            }
```

(The agent-side gate already suppresses requests when `auto_approve` is true; this check covers the race where AllowSession was just enabled.)

- [ ] **Step 5: Modal decisions dispatch.** In `handle_modal_key`, `ModalState::Approval` Enter-arm (lines ~1017-1050):
  - `Accept` branch: add `if let Some(tool_id) = approval_state_snapshot_of_current_modal()` — IMPLEMENTER NOTE: capture `tool_id` BEFORE matching by reading it from the state at the start of the Enter arm:

```rust
                KeyCode::Enter => {
                    let pending_tool_id = if let ModalState::Approval(st) = &self.modal {
                        st.tool_id.clone()
                    } else {
                        None
                    };
                    if let Some(resp) = /* existing confirm_selection() call */ {
```

    (Check `ApprovalModalState` field name — read `src/ui/approval.rs:60-72`; use the actual `Option<String>` tool-id field. If none exists, add `pub tool_id: String` populated by `from_tool_call`.)
    Then in each branch:
    - `Accept`: `if let Some(id) = &pending_tool_id { self.resolve_approval(id, ApprovalDecision::Approve); }`
    - `Reject`: same with `Reject`.
    - `AllowSession`: keep `self.config.agent.auto_approve = true;` AND propagate to the actor (the actor holds a separate config clone — without this the gate keeps asking):

```rust
                                let api_key = self
                                    .config
                                    .get_api_key(&self.config.provider.default)
                                    .unwrap_or_default();
                                let _ = control_tx.send(AgentCommand::UpdateConfig {
                                    config: Box::new(self.config.clone()),
                                    provider: crate::agent::provider::create_provider_with_base_url(
                                        &self.config.provider.default,
                                        &api_key,
                                        None,
                                    )
                                    .unwrap_or_else(|_| {
                                        crate::agent::provider::create_provider(
                                            &self.config.provider.default,
                                            &api_key,
                                        )
                                        .unwrap_or_else(|_| unreachable_provider_fallback()),
                                    }),
                                });
```

    SIMPLER authoritative form (avoid double-fallback gymnastics; reuse the pattern already at app.rs ~819-830 which handles errors by skipping):

```rust
                                match self.config.get_api_key(&self.config.provider.default) {
                                    Ok(key) => {
                                        match crate::agent::provider::create_provider_with_base_url(
                                            &self.config.provider.default, &key, None,
                                        ) {
                                            Ok(new_prov) => {
                                                let _ = control_tx.send(AgentCommand::UpdateConfig {
                                                    config: Box::new(self.config.clone()),
                                                    provider: new_prov,
                                                });
                                            }
                                            Err(e) => tracing::warn!(error = %e, "AllowSession: provider rebuild failed; auto_approve applies next turn"),
                                        }
                                    }
                                    Err(e) => tracing::warn!(error = %e, "AllowSession: API key unavailable"),
                                }
```

    - `CustomFeedback`: send `Reject` for `pending_tool_id` FIRST (the old turn's pending gate must not hang), then keep the existing `control_tx.send(AgentCommand::Prompt(feedback, …))` logic unchanged.
    - Esc branch (line ~1000): also resolve pending as `Reject` (capture `tool_id` the same way before clearing the modal).

- [ ] **Step 6: Manual verification (required — UI surface).** Run `cargo run` against a scratch dir with a real provider key; prompt "create /tmp/x.txt containing hi"; expect: modal appears; Esc → timeline shows rejection AND model receives failure result; re-run with AllowSession → subsequent writes proceed with no modal. If no API key available, verify with `--yes` path that no modal appears.
- [ ] **Step 7:** `cargo fmt && cargo check --all-targets 2>&1 | tail -3 && git add src/app.rs && git commit -m "feat(ui): wire approval modal decisions to agent gate and propagate AllowSession"`

---

### Task 5: NDJSON wiring + headless policy (main.rs)

**Files:**
- Modify: `src/main.rs` (`run_ndjson_agent` 305-379, `run_headless_task` 227-302)

**Interfaces:**
- Consumes: registry + `set_interactive_approvals(false)` from Task 2.
- Produces: NDJSON orchestrators can answer approvals; headless one-shot refuses destructive tools unless `--yes`.

- [ ] **Step 1: Restructure `run_ndjson_agent` into an actor** (current design awaits `execute_turn` inline, which would deadlock stdin while an approval pends). Authoritative replacement body:

```rust
async fn run_ndjson_agent(workspace: &Path, config: &Config) -> Result<()> {
    tracing::info!("Starting minicode in NDJSON streaming mode");
    let ready_event = AgentEvent::Heartbeat {
        timestamp: chrono::Utc::now().to_rfc3339(),
        status: "ready".to_string(),
        turn_id: None,
    };
    println!("{}", serde_json::to_string(&ready_event)?);

    let api_key = config.get_api_key(&config.provider.default)?;
    let provider = create_provider(&config.provider.default, &api_key)?;
    let mut agent = AgentLoop::new(workspace, config.clone(), provider);
    let approvals = agent.approval_registry();

    use tokio::io::AsyncBufReadExt;
    let stdin = tokio::io::stdin();
    let mut reader = tokio::io::BufReader::new(stdin).lines();

    while let Some(line) = reader.next_line().await.map_err(error::MinicodeError::Io)? {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        match serde_json::from_str::<StdinCommand>(trimmed) {
            Ok(StdinCommand::UserInput { text }) => {
                let (tx, mut rx) = mpsc::unbounded_channel::<AgentEvent>();
                let event_consumer = tokio::spawn(async move {
                    while let Some(event) = rx.recv().await {
                        if let Ok(json) = serde_json::to_string(&event) {
                            println!("{}", json);
                        }
                    }
                });
                if let Err(e) = agent.execute_turn(&text, tx, None).await {
                    let err_event = AgentEvent::Error {
                        turn_id: None,
                        code: "execution_error".to_string(),
                        message: e.to_string(),
                        retrying: false,
                        retry_after_ms: None,
                    };
                    println!("{}", serde_json::to_string(&err_event)?);
                }
                event_consumer.await.ok();
            }
            Ok(StdinCommand::Abort {}) => {
                tracing::info!("Received abort command via stdin");
                break;
            }
            Ok(StdinCommand::Configure { auto_approve, model }) => {
                tracing::info!(?auto_approve, ?model, "Received runtime configure command");
            }
            Ok(StdinCommand::ToolResponse { tool_id, action, .. }) => {
                let decision = match action.as_str() {
                    "approve" => Some(crate::agent::types::ApprovalDecision::Approve),
                    "reject" => Some(crate::agent::types::ApprovalDecision::Reject),
                    _ => None,
                };
                match decision {
                    Some(decision) => {
                        let sender = approvals
                            .lock()
                            .unwrap_or_else(|p| p.into_inner())
                            .remove(&tool_id);
                        match sender {
                            Some(sender) => {
                                let _ = sender.send(decision);
                            }
                            None => {
                                tracing::warn!(tool_id = %tool_id, "ToolResponse for unknown tool_id");
                            }
                        }
                    }
                    None => {
                        tracing::warn!(action = %action, "Unknown ToolResponse action");
                    }
                }
            }
            Err(e) => {
                let err_event = AgentEvent::Error {
                    turn_id: None,
                    code: "invalid_command".to_string(),
                    message: format!("Failed to parse stdin command: {}", e),
                    retrying: false,
                    retry_after_ms: None,
                };
                println!("{}", serde_json::to_string(&err_event)?);
            }
        }
    }

    Ok(())
}
```

NOTE: with this structure a turn still runs to completion before the next stdin line is processed — but `ToolResponse` lines sent DURING a turn cannot be consumed because we're blocked in `execute_turn`. To truly unblock: move the `UserInput` arm into `tokio::spawn` with the agent behind `Arc<tokio::sync::Mutex<AgentLoop>>`. REQUIRED refactor:

```rust
    let agent = std::sync::Arc::new(tokio::sync::Mutex::new(agent));
```

and the UserInput arm becomes:

```rust
            Ok(StdinCommand::UserInput { text }) => {
                let agent = agent.clone();
                tokio::spawn(async move {
                    let (tx, mut rx) = mpsc::unbounded_channel::<AgentEvent>();
                    let event_consumer = tokio::spawn(async move {
                        while let Some(event) = rx.recv().await {
                            if let Ok(json) = serde_json::to_string(&event) {
                                println!("{}", json);
                            }
                        }
                    });
                    let mut guard = agent.lock().await;
                    if let Err(e) = guard.execute_turn(&text, tx, None).await {
                        let err_event = AgentEvent::Error {
                            turn_id: None,
                            code: "execution_error".to_string(),
                            message: e.to_string(),
                            retrying: false,
                            retry_after_ms: None,
                        };
                        println!("{}", serde_json::to_string(&err_event)?);
                    }
                    drop(guard);
                    event_consumer.await.ok();
                });
            }
```

(`Abort` then breaks the outer loop; in-flight turn finishes because the process exits after the loop returns — acceptable, document in CHANGELOG.)

Check the `StdinCommand::ToolResponse` variant shape first (`src/agent/types.rs:200-206`) and adjust destructuring (`..` covers optional feedback field).

- [ ] **Step 2: Headless policy.** In `run_headless_task` (line ~236, right after `AgentLoop::new`):

```rust
    agent.set_interactive_approvals(cli_yes_mode_or_config_auto);
```

Concretely: `run_headless_task` has no `--yes` flag today; thread the existing CLI flag down OR simplest correct form — leave `interactive_approvals = false` semantics by calling:

```rust
    agent.set_interactive_approvals(false);
```

so destructive tools are refused with the "--yes" guidance unless `config.agent.auto_approve` (which `--yes` sets at main.rs:174). Update the `--yes` help text in the `Cli` derive (line ~56) to: `"Auto-approve dangerous tools (write_file, patch_file, exec_cmd) in headless mode"`.

- [ ] **Step 3: Verify.** Unit-level: `cargo check --all-targets`. Behavioral smoke (no key needed for parse paths): `echo '{"method":"tool_response","params":{"tool_id":"x","action":"approve"}}' | cargo run -- --json-stream . 2>/dev/null | head -2` → expect ready heartbeat then graceful unknown-tool_id warning in log (not crash).
- [ ] **Step 4:** `cargo fmt && git add src/main.rs && git commit -m "feat(ndjson): route tool_response decisions to approval gate; refuse destructive tools in headless without --yes"`

---

### Task 6: Wire dead git config knobs

**Files:**
- Modify: `src/git/commit.rs` (struct ~6-14, `commit()` 17-37)
- Modify: `src/agent/loop.rs` (auto-commit block ~508-517)
- Test: extend `#[cfg(test)] mod tests` in `src/git/mod.rs` (existing git test harness uses `tempdir()` + real `git init`)

**Interfaces:**
- Produces:
  - `GitCommitService::with_stage_all(bool) -> Self` — when true, stages everything (`git add -A`) instead of only `paths`
  - Loop-side: `dirty_commit=true` → stage-all; `ai_commit_messages=false` → plain message.

- [ ] **Step 1: Failing test** in `src/git/mod.rs` tests module (follow the existing `git init` fixture style at its top):

```rust
    #[tokio::test]
    async fn commit_with_stage_all_includes_unstaged_files() {
        let dir = tempdir().expect("tempdir");
        let git = GitService::new(dir.path().to_path_buf());
        git.run_git(&["init"]).await.expect("git init failed");
        git.run_git(&["config", "user.name", "t"]).await.unwrap();
        git.run_git(&["config", "user.email", "t@t"]).await.unwrap();
        std::fs::write(dir.path().join("tracked.txt"), "v1").unwrap();
        let svc = GitCommitService::new(&git);
        svc.commit("initial", Some(&["tracked.txt".to_string()])).await.expect("first commit");

        // Untracked file must be swept in by with_stage_all(true).
        std::fs::write(dir.path().join("stray.txt"), "surprise").unwrap();
        let svc_all = GitCommitService::new(&git).with_stage_all(true);
        svc_all.commit("second", Some(&[])).await.expect("second commit");

        let out = git.run_git(&["show", "--name-only", "--format=", "HEAD"]).await.unwrap();
        assert!(out.contains("stray.txt"), "stage_all must include untracked files");
    }
```

(Check actual names: `GitService::new(PathBuf)` signature and `run_git` visibility in `src/git/service.rs` / `mod.rs` — mirror whatever the neighboring tests use.)

- [ ] **Step 2:** `cargo test --lib git:: 2>&1 | tail -5` → FAIL (`with_stage_all` missing).
- [ ] **Step 3: Implement** in `src/git/commit.rs`:

```rust
pub struct GitCommitService<'a> {
    git: &'a GitService,
    /// When true, `commit` stages ALL workspace changes (`git add -A`)
    /// instead of only the explicitly listed paths.
    stage_all: bool,
}

impl<'a> GitCommitService<'a> {
    pub fn new(git: &'a GitService) -> Self {
        Self { git, stage_all: false }
    }

    /// Builder: enable staging of every workspace change, not just listed files.
    pub fn with_stage_all(mut self, yes: bool) -> Self {
        self.stage_all = yes;
        self
    }
```

And in `commit()`, replace the staging step (line ~29):

```rust
        // 1. Stage files
        if self.stage_all {
            self.git.run_git(&["add", "-A"]).await?;
        } else {
            self.git.stage_files(paths).await?;
        }
```

- [ ] **Step 4: Consume config in loop.rs auto-commit block** (lines ~509-515):

```rust
            let git = crate::git::GitService::new(self.workspace_root.clone());
            if git.is_git_repo().await {
                let commit_svc = crate::git::GitCommitService::new(&git)
                    .with_stage_all(self.config.git.dirty_commit);
                let commit_msg = if self.config.git.ai_commit_messages {
                    crate::git::GitCommitService::generate_conventional_message(
                        &turn_files_modified,
                        Some(user_prompt),
                    )
                } else {
                    format!("chore: update {}", turn_files_modified.join(", "))
                };
```

- [ ] **Step 5:** `cargo test --lib git:: && cargo check --all-targets` → PASS. Also confirm existing `config.rs` merge tests still pass (they now describe real behavior).
- [ ] **Step 6:** `cargo fmt && git add src/git/commit.rs src/agent/loop.rs && git commit -m "feat(git): wire dirty_commit and ai_commit_messages config options"`

---

### Task 7: SSRF hardening — post-DNS private-IP check

**Files:**
- Modify: `src/tools/web.rs` (`validate_ssrf` 5-60, `fetch_or_browse` 66+)

**Interfaces:**
- Produces: `fn is_private_ip(ip: IpAddr) -> bool` (covers IPv4-mapped IPv6); `fetch_or_browse` rejects URLs whose DNS resolution lands on a private/loopback address.

- [ ] **Step 1: Failing tests** in `web.rs` test module:

```rust
    #[test]
    fn test_is_private_ip_covers_mapped_ipv6_and_ranges() {
        assert!(super::is_private_ip("127.0.0.1".parse().unwrap()));
        assert!(super::is_private_ip("::ffff:10.0.0.5".parse().unwrap()));
        assert!(super::is_private_ip("::ffff:169.254.169.254".parse().unwrap()));
        assert!(!super::is_private_ip("93.184.216.34".parse().unwrap()));
        assert!(!super::is_private_ip("2606:2800:220:1:248:1893:25c8:1946".parse().unwrap()));
    }
```

- [ ] **Step 2:** `cargo test --lib tools.web 2>&1 | tail -5` → FAIL (fn missing).
- [ ] **Step 3: Implement.** Extract the IP match from `validate_ssrf` (lines 32-48) into:

```rust
/// True for loopback/link-local/private/CGNAT addresses, including IPv4-mapped IPv6.
fn is_private_ip(ip: std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(v4) => {
            v4.is_loopback()
                || v4.is_link_local()
                || v4.is_broadcast()
                || v4.is_documentation()
                || v4.is_unspecified()
                || (v4.octets()[0] == 10)
                || (v4.octets()[0] == 172 && (16..=31).contains(&v4.octets()[1]))
                || (v4.octets()[0] == 192 && v4.octets()[1] == 168)
                || (v4.octets()[0] == 100 && (64..=127).contains(&v4.octets()[1]))
        }
        std::net::IpAddr::V6(v6) => {
            if let Some(mapped) = v6.to_ipv4_mapped() {
                return is_private_ip(std::net::IpAddr::V4(mapped));
            }
            v6.is_loopback() || v6.is_unspecified() || ((v6.segments()[0] & 0xfe00) == 0xfc00)
        }
    }
}
```

and have `validate_ssrf` call it (`if let Ok(ip) = host_str.parse::<std::net::IpAddr>() { if is_private_ip(ip) { return Err(...) } }`).

Then add DNS re-check used by fetch:

```rust
/// Resolves the URL host and blocks any result landing on private space
/// (defeats DNS rebinding like localtest.me → 127.0.0.1).
async fn assert_resolves_public(parsed: &url::Url) -> Result<()> {
    let host = parsed.host_str().unwrap_or_default();
    let port = parsed.port_or_known_default().unwrap_or(80);
    if std::net::IpAddr::parse_from_str_fallback(&host).is_some() {
        return Ok(()); // literal IPs were already checked by validate_ssrf
    }
    let addrs = tokio::net::lookup_host((host, port))
        .await
        .map_err(|e| SecurityError::SsrfBlocked {
            url: parsed.to_string(),
            reason: format!("DNS resolution failed: {}", e),
        })?;
    for addr in addrs {
        if is_private_ip(addr.ip()) {
            return Err(SecurityError::SsrfBlocked {
                url: parsed.to_string(),
                reason: format!("Host resolves to private address '{}'", addr.ip()),
            }
            .into());
        }
    }
    Ok(())
}
```

CORRECTION: `IpAddr::parse_from_str_fallback` does not exist — use `host.parse::<std::net::IpAddr>().is_ok()`.

Call site in `fetch_or_browse` (after `validate_ssrf(url)` succeeds, before any network I/O):

```rust
    let parsed = url::Url::parse(url).map_err(/* existing InvalidArguments mapping */)?;
    validate_ssrf(url)?;
    assert_resolves_public(&parsed).await?;
```

(Restructure so the URL is parsed once and reused.)

- [ ] **Step 4:** `cargo test --lib tools.web && cargo check --all-targets` → PASS. Note: `validate_browser_url` intentionally permits localhost dev servers — do NOT apply there.
- [ ] **Step 5:** `cargo fmt && git add src/tools/web.rs && git commit -m "fix(security): block SSRF via DNS-resolved private addresses and mapped IPv6"`

---

### Task 8: Nits batch — model-name constants, regex expect, tokio fs snapshot

**Files:**
- Modify: `src/constants.rs` (provider endpoints section ~303-329)
- Modify: `src/agent/provider.rs` (lines 429, 436, 761, 771)
- Modify: `src/tools/rtk_filter.rs` (line 7)
- Modify: `src/agent/loop.rs` (file_before snapshot ~380-391)

- [ ] **Step 1:** Add to `src/constants.rs` under Model Provider section:

```rust
/// OpenRouter default model used when config omits one
pub const OPENROUTER_DEFAULT_MODEL: &str = "anthropic/claude-3.5-sonnet";
/// OpenAI default model used when config omits one
pub const OPENAI_DEFAULT_MODEL: &str = "gpt-4o";
/// DeepSeek default model used when config omits one
pub const DEEPSEEK_DEFAULT_MODEL: &str = "deepseek-coder";
```

Replace literals: `provider.rs:429` → `crate::constants::OPENROUTER_DEFAULT_MODEL`; `:436` and `:761` → `OPENAI_DEFAULT_MODEL`; `:771` → `DEEPSEEK_DEFAULT_MODEL`.

- [ ] **Step 2:** `rtk_filter.rs:7` — `.unwrap()` → `.expect("ANSI escape regex must compile")` (static, fixed pattern; documents invariant per repo rule).

- [ ] **Step 3:** `loop.rs` file_before snapshot (~387): swap blocking read for async:

```rust
                                .and_then(|rel| {
                                    // Best-effort snapshot; async read avoids blocking the runtime.
                                    None // replaced below
                                })
```

AUTHORITATIVE form — convert the enclosing closure chain to an async block:

```rust
                    let file_before: Option<String> =
                        if FILE_MODIFYING_TOOLS.contains(&tool_call.name.as_str()) {
                            match tool_call.arguments.get("path").and_then(|p| p.as_str()) {
                                Some(rel) => {
                                    let target = self.workspace_root.join(rel);
                                    tokio::fs::read_to_string(&target).await.ok()
                                }
                                None => None,
                            }
                        } else {
                            None
                        };
```

- [ ] **Step 4:** `cargo fmt && cargo check --all-targets && cargo test --lib 2>&1 | tail -3` → PASS.
- [ ] **Step 5:** `git add -A && git commit -m "refactor: centralize provider default models, document regex invariant, async file snapshot"`

---

### Task 9: Final verification

- [ ] **Step 1:** `cargo fmt --check && cargo clippy --all-targets -- -D warnings 2>&1 | tail -20` — fix any new lints introduced by these tasks (repo lint gate).
- [ ] **Step 2:** `cargo test 2>&1 | tail -15` — full suite green (compile already capped at 2 jobs).
- [ ] **Step 3:** Smoke the changed surfaces:
  - `cargo run -- --help` shows updated `--yes` description.
  - TUI manual pass per Task 4 Step 6 (or note skipped if no API key).
  - NDJSON smoke per Task 5 Step 3.
- [ ] **Step 4:** Update `CHANGELOG.md` under Unreleased: approval enforcement, cancellation status fix, `dirty_commit`/`ai_commit_messages` now honored, headless requires `--yes` for destructive tools, SSRF DNS pinning.
- [ ] **Step 5:** Commit: `git commit -m "docs: changelog for approval enforcement release"` (amend CHANGELOG.md only).

---

## Risk Notes

- **Behavior change (intentional):** headless/one-shot without `--yes` now REFUSES `write_file`/`patch_file`/`exec_cmd` instead of executing them. This is the point of strict mode; called out in CHANGELOG.
- **NDJSON ordering:** turns may interleave with `ToolResponse` lines since the stdin loop is no longer blocked; orchestrators that pipelined commands previously get identical semantics except approvals now actually work.
- **TUI modal field dependency:** `ApprovalModalState` needs an accessible `tool_id` (Task 4 Step 5) — read `src/ui/approval.rs:60-72` first and adapt the field name; if absent, add `pub tool_id: String` set by `from_tool_call`.

(ledger note: Tasks 2+ executed inline; briefs at .superpowers/sdd/task-N-brief.md)

# MiniTask Dynamic Scheduling & Natural Intent Telemetry Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Equip minicode's `minitask` process vault with dynamic scheduled tasks/watchers (configurable interval with 60s default, one-shot timers, cron) and wire natural intent routing so queries like "what's happening" instantly inspect background tasks and live telemetry without hardcoded values.

**Architecture:** Extend `MiniDevRegistry` with an async Tokio scheduling supervisor that tracks one-shot timers and recurring interval watchers under process isolation with cancellation handles. Integrate `action: "schedule"` into the unified `minitask` tool schema. Connect `WorkflowRouter` and `IntentFilter` to automatically detect background inspection queries ("what's happening", "status of tasks", "what is running"), dynamically enriching the LLM pre-turn context with live process telemetry, schedule intervals, and recent log tails.

**Tech Stack:** Rust 2021 Edition, Tokio multi-threaded async runtime, Ratatui, `thiserror`, `serde_json`, Linux `/proc` filesystem metrics.

## Global Constraints

- Pure Rust 2021, multi-threaded Tokio async runtime.
- Zero `.unwrap()` or `.expect()` in non-test library code (`src/`).
- Concurrency flags: `-j 1` on `cargo check`, `cargo test`, `cargo clippy`; `-j 2` on release builds.
- Tool registry invariant: Maintain exact synchronization between `TOTAL_TOOL_COUNT` (185) and `ToolRegistry::get_tool_schemas().len()`.
- Single authoritative tool name: Strictly `minitask` (Tool 168) — zero aliases.
- No hardcoded schedule intervals: Default to 60 seconds if unspecified, but accept arbitrary `interval_seconds`, `duration_seconds` (one-shot), or `cron`.
- Zero-orphan guarantee: All scheduled tasks, interval loops, and child processes must be cancelled on `stop`, `kill_all`, or minicode shutdown.

---

### Task 1: Domain Models & Scheduling Types

**Files:**
- Modify: `src/dev/models.rs:15-80`
- Test: `src/dev/models.rs:180-250`

**Interfaces:**
- Consumes: `DevProcessType`, `DevProcessSummary`, `DevProcessStatus`
- Produces: `DevProcessType::Cron`, `DevProcessType::Timer`, `ScheduleRequest`, `ScheduleInfo`

- [ ] **Step 1: Write the failing unit tests for `ScheduleRequest` and `DevProcessType` serialization**

```rust
#[test]
fn test_schedule_request_default_interval() {
    let req = ScheduleRequest {
        command: "cargo check".to_string(),
        name: Some("periodic-check".to_string()),
        interval_seconds: None,
        duration_seconds: None,
        cron_expression: None,
        max_iterations: Some(5),
    };
    assert_eq!(req.effective_interval_secs(), 60);
    assert!(!req.is_one_shot());
}

#[test]
fn test_schedule_request_one_shot() {
    let req = ScheduleRequest {
        command: "echo wake".to_string(),
        name: None,
        interval_seconds: None,
        duration_seconds: Some(15),
        cron_expression: None,
        max_iterations: None,
    };
    assert_eq!(req.duration_seconds, Some(15));
    assert!(req.is_one_shot());
}
```

- [ ] **Step 2: Run targeted test to verify failure**
Run: `cargo test -j 1 --lib dev::models::tests::test_schedule_request` (must fail compilation due to missing types).

- [ ] **Step 3: Implement `DevProcessType::Cron`, `DevProcessType::Timer`, `ScheduleRequest`, and `ScheduleInfo`**
Add variants to `DevProcessType`:
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DevProcessType {
    Frontend,
    Backend,
    Docker,
    Script,
    Chrome,
    Worker,
    Cron,
    Timer,
}
```
Define `ScheduleRequest` and `ScheduleInfo`:
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleRequest {
    pub command: String,
    pub name: Option<String>,
    pub interval_seconds: Option<u64>,
    pub duration_seconds: Option<u64>,
    pub cron_expression: Option<String>,
    pub max_iterations: Option<usize>,
}

impl ScheduleRequest {
    pub fn effective_interval_secs(&self) -> u64 {
        self.interval_seconds.unwrap_or(60)
    }

    pub fn is_one_shot(&self) -> bool {
        self.duration_seconds.is_some()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleInfo {
    pub interval_secs: u64,
    pub is_one_shot: bool,
    pub iteration_count: usize,
    pub max_iterations: Option<usize>,
    pub last_run_timestamp: Option<u64>,
    pub next_run_timestamp: Option<u64>,
}
```
Add `schedule_info: Option<ScheduleInfo>` to `DevProcessSummary`.

- [ ] **Step 4: Run targeted test and ensure pass**
Run: `cargo test -j 1 --lib dev::models::tests`

- [ ] **Step 5: Commit changes**
`git add src/dev/models.rs && git commit -m "feat(dev): define ScheduleRequest, ScheduleInfo, and Cron/Timer process types"`

---

### Task 2: Async Scheduler Engine in `MiniDevRegistry`

**Files:**
- Modify: `src/dev/registry.rs:40-150, 360-490`
- Test: `src/dev/registry.rs:850-950`

**Interfaces:**
- Consumes: `ScheduleRequest`, `ScheduleInfo`, `DevProcessHandle`, `DevProcessSummary`
- Produces: `MiniDevRegistry::schedule(&self, workspace_root: &Path, req: ScheduleRequest) -> Result<DevProcessSummary>`

- [ ] **Step 1: Write failing unit test for `MiniDevRegistry::schedule`**

```rust
#[tokio::test]
async fn test_schedule_interval_execution_and_cancellation() {
    let registry = MiniDevRegistry::new();
    let temp = tempdir().expect("tempdir");

    let req = ScheduleRequest {
        command: "echo 'tick'".to_string(),
        name: Some("test-timer".to_string()),
        interval_seconds: Some(1),
        duration_seconds: None,
        cron_expression: None,
        max_iterations: Some(2),
    };

    let summary = registry.schedule(temp.path(), req).await.expect("schedule");
    assert_eq!(summary.name, "test-timer");
    assert!(summary.id.as_str().starts_with("sched-"));

    // Wait for first tick
    tokio::time::sleep(Duration::from_millis(1200)).await;

    let logs = registry.logs(&summary.id, 10, None).await.expect("logs");
    assert!(logs.iter().any(|l| l.contains("tick") || l.contains("Scheduled execution")));

    // Stop scheduled task
    let stopped = registry.stop(&summary.id).await.expect("stop");
    assert!(stopped);
}
```

- [ ] **Step 2: Run targeted test to verify failure**
Run: `cargo test -j 1 --lib dev::registry::tests::test_schedule_interval_execution_and_cancellation`

- [ ] **Step 3: Implement `schedule()` in `MiniDevRegistry`**
1. Store an abort handle `tokio::task::JoinHandle<()>` or cancellation token in the handle.
2. Spawn async task using `tokio::time::interval(Duration::from_secs(interval))`.
3. Execute the command or script via `tokio::process::Command`, capturing stdout into the process ring buffer.
4. Increment `iteration_count`. If `iteration_count >= max_iterations`, mark process as `Stopped`.
5. Integrate cancellation into `stop()`, `terminate()`, and `kill_all()`.

- [ ] **Step 4: Run targeted test and ensure pass**
Run: `cargo test -j 1 --lib dev::registry::tests::test_schedule_interval_execution_and_cancellation`

- [ ] **Step 5: Commit changes**
`git add src/dev/registry.rs && git commit -m "feat(dev): implement async scheduled task runner with interval and cancellation in registry"`

---

### Task 3: `minitask` Tool Expansion (`action: "schedule"`)

**Files:**
- Modify: `src/tools/registry/dev_tools.rs:15-180`
- Test: `src/tools/registry/dev_tools.rs:250-320`

**Interfaces:**
- Consumes: `ToolRegistry::dispatch("minitask", ...)`
- Produces: `action: "schedule"`, expanded `action: "list"` output displaying schedule intervals and trigger stats

- [ ] **Step 1: Write failing unit test for `minitask action: "schedule"` dispatch**

```rust
#[tokio::test]
async fn test_minitask_schedule_tool_dispatch() {
    let temp = tempdir().expect("tempdir");
    let args = json!({
        "action": "schedule",
        "command": "echo 'Heartbeat check'",
        "name": "system-heartbeat",
        "interval_seconds": 30
    });

    let res = dispatch("minitask", &args, temp.path()).await.expect("dispatch").expect("output");
    assert!(res.contains("Scheduled task registered successfully"));
    assert!(res.contains("Every 30s") || res.contains("30s"));
}
```

- [ ] **Step 2: Run targeted test to verify failure**
Run: `cargo test -j 1 --lib tools::registry::dev_tools::tests::test_minitask_schedule_tool_dispatch`

- [ ] **Step 3: Implement `schedule` schema parameters and dispatch handler**
Update `minitask` schema description:
`"Actions: 'start', 'stop', 'restart', 'kill_all', 'list', 'status', 'logs', 'resources', 'probe_port', 'workers', 'schedule'"`
Add optional properties: `interval_seconds`, `duration_seconds`, `cron`, `max_iterations`.
Handle `action == "schedule"` in `dispatch()`:
- Parse `interval_seconds` (default: 60), `duration_seconds`, `command` (or `prompt`), and `max_iterations`.
- Invoke `registry.schedule(workspace_root, req).await`.
- Return formatted confirmation message with task ID and schedule cadence.
Update `action == "list"` to show schedule details when present:
`• [sched-abc123] system-heartbeat (Cron) | Status: Running | Schedule: Every 60s | Runs: 4 | CPU: 0.0% | RSS: 0.0MB`

- [ ] **Step 4: Run targeted test and total tool count test**
Run: `cargo test -j 1 --lib dev_tools::tests -- --test-threads=1`
Run: `cargo test -j 1 --lib tool_count_validation`

- [ ] **Step 5: Commit changes**
`git add src/tools/registry/dev_tools.rs && git commit -m "feat(tools): add minitask action: schedule with configurable intervals and rich list formatting"`

---

### Task 4: Natural Intent Routing & Pre-Turn Context Enrichment

**Files:**
- Modify: `src/agent/orchestrator.rs:490-540, 680-740`
- Modify: `src/context/search/intent_filter.rs:250-290`
- Modify: `src/agent/prompt.rs:80-90`
- Test: `src/agent/orchestrator.rs:850-920`

**Interfaces:**
- Consumes: User prompts ("what's happening", "what is running", "status of tasks", "is the server running")
- Produces: `WorkflowIntent::ProcessInspection`, pre-turn prompt context block `<live_process_vault_inspection>`

- [ ] **Step 1: Write failing unit test for process inspection intent detection**

```rust
#[test]
fn test_workflow_router_detects_process_inspection() {
    assert_eq!(
        WorkflowRouter::classify("what's happening with the background tasks?"),
        WorkflowIntent::ProcessInspection
    );
    assert_eq!(
        WorkflowRouter::classify("whats happening"),
        WorkflowIntent::ProcessInspection
    );
    assert_eq!(
        WorkflowRouter::classify("is the server still running?"),
        WorkflowIntent::ProcessInspection
    );
    assert_eq!(
        WorkflowRouter::classify("what is running in background"),
        WorkflowIntent::ProcessInspection
    );
}
```

- [ ] **Step 2: Run targeted test to verify failure**
Run: `cargo test -j 1 --lib agent::orchestrator::tests::test_workflow_router_detects_process_inspection`

- [ ] **Step 3: Implement intent classification and pre-turn enrichment**
1. Add `WorkflowIntent::ProcessInspection` in `src/agent/orchestrator.rs`.
2. In `WorkflowRouter::classify`, recognize patterns:
   - "whats happening", "what is happening", "what's running", "what is running"
   - "what are the tasks doing", "how is the background process", "status of tasks"
   - "is the server running", "is the server still running", "any background tasks"
3. In pre-turn prompt building:
   - When `WorkflowIntent::ProcessInspection` is active, fetch `registry.list().await` and `registry.resources().await`.
   - If processes or scheduled tasks exist, append a `<live_process_vault_inspection>` block into the prompt:
     - Active process list (Name, Type, PID, URL, Schedule cadence, CPU, RAM).
     - Recent log tail (last 3 lines of each active process).
   - If zero processes are running, state:
     `No background processes or scheduled tasks are currently running.`
4. In `src/agent/prompt.rs`:
   - Instruct the agent: when the user asks what is happening or checks background status, use the live process inspection data and `minitask` to answer directly and accurately.

- [ ] **Step 4: Run targeted test and ensure pass**
Run: `cargo test -j 1 --lib agent::orchestrator::tests`

- [ ] **Step 5: Commit changes**
`git add src/agent/orchestrator.rs src/context/search/intent_filter.rs src/agent/prompt.rs && git commit -m "feat(agent): route background inspection queries to live process vault telemetry"`

---

### Task 5: F7 Process Modal Display, Integration Tests & Global Release

**Files:**
- Modify: `src/ui/modals/processes.rs:400-440, 560-640`
- Create: `tests/integration_minitask_schedule.rs`
- Test: All integration tests

**Interfaces:**
- Consumes: `DevProcessSummary` with `DevProcessType::Cron`/`Timer` and `ScheduleInfo`
- Produces: In-modal table display for scheduled tasks, end-to-end integration tests

- [ ] **Step 1: Write integration test suite `tests/integration_minitask_schedule.rs`**
Test:
1. `minitask` schedule one-shot timer and interval watcher.
2. `minitask list` reports schedule interval and execution count.
3. Natural intent router enriches prompt context with live process telemetry.
4. Clean zero-orphan cancellation of scheduled tasks.

- [ ] **Step 2: Run integration test to verify implementation**
Run: `cargo test -j 1 --test integration_minitask_schedule`

- [ ] **Step 3: Update F7 Process Modal Table Rendering**
In `src/ui/modals/processes.rs`:
- Under `TYPE`, render `Cron` or `Timer` styled with info/accent colors.
- Under `PORT / URL`, if URL is empty but `schedule_info` exists, render `Every <N>s` or `Timer (<N>s)`.
- Ensure all 5 tabs and columns render cleanly without truncation.

- [ ] **Step 4: Full Quality Gates**
- `cargo check -j 1`
- `cargo clippy -j 1 --bin minicode -- -D warnings`
- `cargo fmt --check`
- `cargo test -j 1 --test integration_minitask_schedule`
- `cargo test -j 1 --test integration_minitask_manager`

- [ ] **Step 5: Compile Production Release & Install**
- `cargo build --release -j 2`
- `cp target/release/minicode ~/.local/bin/minicode.new && mv -f ~/.local/bin/minicode.new ~/.local/bin/minicode`
- Verify `~/.local/bin/minicode --version`.

- [ ] **Step 6: Commit changes & Update Documentation**
`git add -A && git commit -m "feat(tasks): implement dynamic scheduled watchers, minitask schedule action, and natural intent inspection"`
Update `onpkg_docs/core/todo.md` with Phase 147.
EOF

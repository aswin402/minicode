# MiniDev Runtime Orchestration & Guaranteed Process Cleanup (Phase 140)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build `mini_dev`, a comprehensive runtime orchestrator for `minicode` that manages long-running dev servers, backend processes, scripts, Docker containers, and browser instances with full CRUD, port auto-discovery, resource telemetry (CPU/RAM), and a guaranteed zero-orphan shutdown invariant.

**Architecture:** A thread-safe global registry (`MiniDevRegistry`) manages isolated child process groups (`setpgid`). It enforces multi-tier lifecycle protection (Linux `PR_SET_PDEATHSIG`, POSIX death-pipe sentinel, signal interceptors for `SIGINT`/`SIGTERM`/panic) to guarantee that all spawned processes, containers, and browser instances are deterministically terminated when `minicode` exits. Tool 168 (`mini_dev`) exposes structured CRUD and resource telemetry to the agent, while `/dev` and `/serve` provide human TUI controls.

**Tech Stack:** Rust 2021, Tokio Multi-threaded Async Runtime, `nix`/`libc` (signals, process groups, `setpgid`), Linux `/proc` filesystem metrics, `ratatui` TUI.

---

## Global Constraints

- **Language & Runtime:** Rust 2021 Edition, Tokio async runtime.
- **Panic Safety:** Zero `.unwrap()` or `.expect()` in non-test production code. Strict `Result<T, DevError>`.
- **Concurrency:** `cargo check -j 1`, `cargo test -j 1`, release builds max `-j 2`.
- **Logging:** Use `tracing::` macros. Zero `println!` or `eprintln!` in library modules.
- **Tool Count Synchrony:** `TOTAL_TOOL_COUNT` updated from 167 to 168 and synchronized across constants, registry schemas, intent filter, and test assertions.
- **Portability:** Zero system `libssl-dev` requirements. Clean compilation and fallback behavior on Linux and non-Linux platforms.

---

## File Structure & Responsibilities

| File Path | Responsibility |
| :--- | :--- |
| `src/dev/mod.rs` | Exposes `pub mod models; pub mod registry; pub mod process; pub mod ports; pub mod metrics; pub mod lifecycle;` and re-exports public API. |
| `src/dev/models.rs` | Domain models: `DevProcessId`, `DevProcessType`, `DevProcessStatus`, `DevProcessInfo`, `DevProcessSummary`, `DevProcessResourceUsage`, `SpawnDevRequest`. |
| `src/dev/process.rs` | Process spawning, isolated process groups (`setpgid`), ring-buffered stdout/stderr captures, and two-phase termination (`SIGTERM` -> grace -> `SIGKILL`). |
| `src/dev/ports.rs` | Dev server port auto-discovery (stdout streaming regex + `/proc/net/tcp` socket table inspection). |
| `src/dev/metrics.rs` | Resource usage telemetry: Linux `/proc/<pid>/stat` (CPU) and `/proc/<pid>/status` (`VmRSS` memory in MB) with cross-platform fallback. |
| `src/dev/lifecycle.rs` | Global RAII teardown guard, signal interceptors (`tokio::signal::ctrl_c`, `SIGTERM`), and panic hook registration. |
| `src/dev/registry.rs` | In-memory thread-safe `MiniDevRegistry` singleton implementing CRUD (`spawn`, `list`, `get`, `logs`, `restart`, `stop`, `kill_all`, `resources`). |
| `src/tools/registry/dev_tools.rs` | Tool 168: `mini_dev` schema definition and dispatch handler (`start`, `list`, `logs`, `stop`, `restart`, `resources`). |
| `src/app/commands.rs` | Slash command routing for `/dev`, `/serve`, `/processes` with quick actions. |
| `src/main.rs` | Wire lifecycle shutdown hook into application startup and exit paths. |
| `tests/integration_minidev_orchestrator.rs` | End-to-end integration tests: spawn, port detection, ring-buffer logs, resource query, process group termination, and exit cleanup. |

---

### Task 1: Domain Models & Error Definitions for `mini_dev`

**Files:**
- Create: `src/dev/models.rs`
- Create: `src/dev/mod.rs`
- Modify: `src/lib.rs` (expose `pub mod dev;`)
- Modify: `src/error.rs` (add `DevError` variant)

**Interfaces:**
- Produces: `DevProcessId`, `DevProcessType`, `DevProcessStatus`, `DevProcessInfo`, `DevProcessSummary`, `DevProcessResourceUsage`, `SpawnDevRequest`, `DevError`.

- [ ] **Step 1: Write unit tests for models**
  - Verify serialization/deserialization of `DevProcessType` (`Frontend`, `Backend`, `Script`, `Docker`, `Chrome`, `Worker`).
  - Verify `DevProcessStatus` transitions (`Running`, `Healthy`, `Failed`, `Stopped`, `Killed`).
  - Verify `DevProcessId` generation and validation.
- [ ] **Step 2: Implement models and error types**
  - Define `enum DevProcessType` with `Display` and `serde`.
  - Define `enum DevProcessStatus` with `Display` and `serde`.
  - Define `struct SpawnDevRequest` with command, name, process type, working dir, env, health check port/path.
  - Define `struct DevProcessResourceUsage` with `cpu_percent`, `memory_rss_mb`, `ports`.
  - Add `DevError` to `src/error.rs`.
- [ ] **Step 3: Run targeted test and format**
  - `cargo test -j 1 --lib dev::models::tests`
  - `cargo fmt --check`

---

### Task 2: Process Group Spawning, Death-Pipe Sentinel & Graceful Teardown

**Files:**
- Create: `src/dev/process.rs`
- Create: `src/dev/ports.rs`
- Test: `src/dev/process.rs` tests module

**Interfaces:**
- Produces: `DevProcessHandle`, `spawn_process_group`, `terminate_process_group`, `scan_ports_from_output`.

- [ ] **Step 1: Write unit tests for process group execution and termination**
  - Test spawning a sleep loop in a separate process group.
  - Test that terminating the group kills the process tree.
  - Test stdout ring-buffer capture.
- [ ] **Step 2: Implement process group spawning**
  - On Unix: apply `setpgid(0, 0)` in `pre_exec`.
  - On Linux: call `libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM)`.
  - Wire pipe sentinel for automatic death on parent EOF.
  - Implement two-phase termination: send `SIGTERM` to `-pgid`, wait up to 1,500ms, then fallback to `SIGKILL`.
- [ ] **Step 3: Implement streaming port auto-discovery**
  - Parse `http://localhost:<port>`, `http://127.0.0.1:<port>`, and `listening on port <port>`.
- [ ] **Step 4: Run targeted test and format**
  - `cargo test -j 1 --lib dev::process::tests`
  - `cargo fmt --check`

---

### Task 3: Resource Telemetry (CPU / RAM) & Global `MiniDevRegistry`

**Files:**
- Create: `src/dev/metrics.rs`
- Create: `src/dev/registry.rs`
- Test: `src/dev/registry.rs` tests module

**Interfaces:**
- Produces: `MiniDevRegistry`, `get_global_dev_registry()`, `sample_process_metrics`.

- [ ] **Step 1: Write unit tests for registry CRUD and resource sampling**
  - Test `spawn`, `list`, `get`, `logs`, `stop`, `restart`, `resources`.
- [ ] **Step 2: Implement `/proc` resource telemetry**
  - Read `/proc/<pid>/stat` (calculate user/system CPU ticks over time delta).
  - Read `/proc/<pid>/status` (parse `VmRSS` in kB and convert to MB).
- [ ] **Step 3: Implement `MiniDevRegistry`**
  - Thread-safe storage with `Arc<RwLock<HashMap<DevProcessId, DevProcessHandle>>>`.
  - Support Docker container registration and stop commands (`docker stop minicode_<id>`).
  - Provide global singleton `get_global_dev_registry()`.
- [ ] **Step 4: Run targeted test and format**
  - `cargo test -j 1 --lib dev::registry::tests`
  - `cargo fmt --check`

---

### Task 4: Global RAII Teardown Guard & Signal Hooks

**Files:**
- Create: `src/dev/lifecycle.rs`
- Modify: `src/main.rs`
- Test: `src/dev/lifecycle.rs` tests module

**Interfaces:**
- Produces: `install_lifecycle_hooks()`, `DevShutdownGuard`.

- [ ] **Step 1: Implement lifecycle signal interception**
  - Listen for `tokio::signal::ctrl_c()`, `SIGTERM`, and `SIGHUP`.
  - On signal trigger: call `MiniDevRegistry::kill_all()` synchronously before exit.
  - Set custom panic hook that invokes cleanup before printing panic backtrace.
- [ ] **Step 2: Wire into `src/main.rs`**
  - Install lifecycle hooks at the beginning of `main()`.
  - Ensure normal exit paths (`/exit`, return `Ok(())`) invoke cleanup.
- [ ] **Step 3: Run targeted test and format**
  - `cargo test -j 1 --lib dev::lifecycle::tests`
  - `cargo fmt --check`

---

### Task 5: Tool 168 (`mini_dev`) Integration & Slash Commands

**Files:**
- Modify: `src/constants.rs` (`TOTAL_TOOL_COUNT` = 168)
- Create: `src/tools/registry/dev_tools.rs`
- Modify: `src/tools/registry/mod.rs`
- Modify: `src/tools/mod.rs`
- Modify: `src/tools/concurrency.rs`
- Modify: `src/context/search/intent_filter.rs`
- Modify: `src/app/commands.rs` (handle `/dev`, `/serve`, `/processes`)

**Interfaces:**
- Produces: Tool 168 `mini_dev` schema and dispatch.

- [ ] **Step 1: Write unit tests for Tool 168**
  - Assert `TOTAL_TOOL_COUNT` == 168.
  - Assert schema validity and dispatch routing for `start`, `list`, `logs`, `stop`, `restart`, `resources`.
- [ ] **Step 2: Implement Tool 168 `mini_dev`**
  - Define JSON schema with parameters (`action`, `command`, `name`, `process_type`, `process_id`, `tail_lines`, etc.).
  - Implement execution dispatch in `dev_tools.rs`.
- [ ] **Step 3: Wire slash commands in `src/app/commands.rs`**
  - `/dev` / `/processes` lists all active daemons, ports, CPU, and RAM.
  - `/serve [command]` quick-starts a managed dev server.
- [ ] **Step 4: Run targeted tests and check**
  - `cargo test -j 1 --lib constants::tool_count_validation`
  - `cargo test -j 1 --lib tools::registry::dev_tools::tests`
  - `cargo fmt --check`

---

### Task 6: End-to-End Integration Testing & Verification

**Files:**
- Create: `tests/integration_minidev_orchestrator.rs`
- Modify: `onpkg_docs/core/todo.md` (record Phase 140)

- [ ] **Step 1: Implement integration tests**
  - Test 1: Spawn background HTTP/script server and auto-detect port.
  - Test 2: Query ring-buffered logs via `mini_dev` tool.
  - Test 3: Query real-time CPU & RSS RAM metrics.
  - Test 4: Restart process and verify new PID and healthy status.
  - Test 5: Verify clean process group teardown on `kill_all` (zero orphans).
- [ ] **Step 2: Run test suite and compiler checks**
  - `cargo test -j 1 --test integration_minidev_orchestrator`
  - `cargo clippy -j 1 --bin minicode -- -D warnings`
  - `cargo fmt --check`
- [ ] **Step 3: Build release binary & deploy**
  - `cargo build --release -j 2`
  - Copy to `/home/aswin/.local/bin/minicode`
  - Verify with `/home/aswin/.local/bin/minicode --version`

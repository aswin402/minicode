# Deep Technical Research: Sandboxing, Security, TUI/CLI Crates & Infrastructure for `minicode`

**Target Project:** `minicode` — Fast, minimalist TUI + CLI AI coding agent in Pure Rust  
**Author:** Technical Research Subagent  
**Date:** 2026-08-15  
**Output Path:** `/home/aswin/programming/vscode/myProjects/ai_agent_tools/minicode/research/for_sandbox_and_tui.md`

---

## Executive Summary

This research report evaluates **13 state-of-the-art projects, crates, and systems** across four critical operational domains:
1. **Sandboxing & Kernel Isolation**: Hardware-level microVM execution, Landlock filesystem containment, Copy-on-Write snapshots, and credential vaulting.
2. **Security, Architectural Governance & Code Quality**: Real-time AST structural sensors, quality gate feedback loops, and symlink-safe file operations.
3. **Headless Browser & Scraping Infrastructure**: Pure Rust V8-embedded browser engines with stealth anti-fingerprinting and CDP protocol handling.
4. **Agent Memory, Task Scheduling & Terminal UI**: Semantic/graph memories, navigational cache-busting, Tokio cron scheduling, and zero-overhead CLI/TUI crates.

---

## Part 1: Sandboxing & MicroVM Runtimes

---

### 1. MicroSandbox

* **Repository:** [https://github.com/superradcompany/microsandbox](https://github.com/superradcompany/microsandbox)
* **What It Does:**  
  MicroSandbox is a fast, local-first microVM runtime and embeddable Rust library that executes untrusted workloads (AI agent code, scrapers, plugins) inside hardware-isolated virtual machines with sub-100ms cold boot times.
* **Key Technical Highlights:**
  * **Multi-Platform Hypervisor Abstraction:** Implements native hardware virtualization across Linux (`KVM`), macOS (`Apple Silicon Virtualization.framework`), and Windows (`Hyper-V / WHP`).
  * **OCI Container Compatibility:** Pulls and unpacks standard OCI container images (Docker Hub, GHCR) directly into microVM root filesystems without requiring a background Docker daemon.
  * **Egress Credential Isolation:** Employs an external security proxy where API keys and tokens never enter the guest VM's environment or filesystem; sensitive headers are injected dynamically on outgoing network traffic.
  * **Process-Level Embedding:** The SDK allows spawning sandboxes as direct child processes (`Sandbox::builder("env").image("python").create().await?`).
* **Inspiration for `minicode`:**
  * **Tier-2 Execution Sandbox:** `minicode` currently uses Linux `landlock` for process-level filesystem restrictions. For running arbitrary user-generated scripts or testing untrusted dependencies, `minicode` can offer an optional microVM isolation mode using MicroSandbox patterns.
  * **Zero-Leak Secret Injection:** Adopt the egress security proxy pattern in `exec_cmd` to inject API tokens without exposing them to child process environment variables or shell logs.
* **Rust Crates / Dependencies Worth Noting:**
  * `microsandbox` (SDK), `tokio`, `hyper`, `tar`, `flate2`, `oci-spec`.

---

### 2. CubeSandbox

* **Repository:** [https://github.com/TencentCloud/CubeSandbox](https://github.com/TencentCloud/CubeSandbox)
* **What It Does:**  
  CubeSandbox is a high-performance, enterprise-grade sandbox engine built on `RustVMM` and KVM for AI agent code execution, delivering <60ms startup latency, <5MB base memory footprint, and high-frequency Copy-on-Write snapshot/rollback.
* **Key Technical Highlights:**
  * **RustVMM Core:** Stripped-down virtual machine monitor customized for instant execution of ephemeral agent commands with near-zero memory overhead.
  * **CubeCoW Engine:** Sub-100ms Copy-on-Write snapshot and memory/disk rollback engine, enabling AI agents to checkpoint state, execute speculative code, and revert instantaneously.
  * **eBPF Network Hardening:** Kernel-level egress filtering, per-sandbox traffic tokens, and L7 policy routing with automatic credential vaulting.
  * **E2B Protocol Compatibility:** Full drop-in API compatibility with the E2B sandbox interface.
* **Inspiration for `minicode`:**
  * **Instant Turn Checkpointing & Rollback:** The CubeCoW pattern of lightweight CoW snapshots can inspire `minicode`'s `/undo` subsystem. Beyond simple file backups, speculative multi-step tasks can be branched and reverted at the filesystem layer.
  * **eBPF-Inspired Local Port/Host Whitelisting:** Enforce strict socket-level egress rules in `exec_cmd` on Linux hosts.
* **Rust Crates / Dependencies Worth Noting:**
  * `vmm-sys-util`, `kvm-ioctls`, `kvm-bindings`, `aya` (eBPF), `tokio`, `tonic`.

---

## Part 2: Security, Governance & Real-Time Sensors

---

### 3. Sentrux

* **Repository:** [https://github.com/sentrux/sentrux](https://github.com/sentrux/sentrux)
* **What It Does:**  
  Sentrux is a Pure Rust real-time architectural sensor and code quality governor that continuously analyzes codebase structure across 52 languages to detect architectural decay, coupling spikes, and dependency cycles introduced by AI coding agents.
* **Key Technical Highlights:**
  * **5 Root-Cause Metrics:** Computes modularity, acyclicity, hierarchy depth, component equality, and code redundancy into a single deterministic 0–10000 quality score in milliseconds.
  * **Declarative Tree-Sitter Plugin Architecture:** Decouples language parsing into `plugin.toml` and Tree-sitter `tags.scm` query files, allowing support for 52 languages with zero language-specific Rust code in the core engine.
  * **Agent Gating & MCP Server:** Exposes 9 MCP tools (`scan`, `session_start`, `session_end`, `check_rules`, `dsm`, etc.) to establish quality baselines before an agent edits code and reject sessions that degrade architecture.
  * **Visual Treemap & WGPU Rendering:** Multi-backend GPU renderer (Vulkan/OpenGL) displaying live dependency graph treemaps with visual glowing diffs as files are modified.
* **Inspiration for `minicode`:**
  * **Pre/Post-Turn Architecture Gating:** Integrate a lightweight dependency graph check into `minicode`'s ReAct loop. Before a complex coding task starts, record an acyclicity/dependency baseline; after the turn, ensure no illegal cross-boundary dependencies or circular imports were created.
  * **Decoupled AST Query Files:** Adopt the `tags.scm` pattern for `minicode`'s Tree-sitter AST queries in `src/context/`, separating query logic from Rust compilation.
  * **Architectural Boundaries in Configuration:** Support a lightweight `.minicode/rules.toml` or `AGENTS.md` rule parser enforcing module layer constraints.
* **Rust Crates / Dependencies Worth Noting:**
  * `tree-sitter`, `petgraph`, `wgpu`, `glyphon`, `winit`, `toml`, `serde`.

---

### 4. Caveman RS

* **Repository:** [https://github.com/aswin402/caveman-rs](https://github.com/aswin402/caveman-rs)
* **What It Does:**  
  Caveman RS is a Pure Rust CLI and multi-agent configuration injector that compresses AI agent prompts and communication into high-density "caveman-speak", cutting token consumption by up to 75% while preserving technical fidelity.
* **Key Technical Highlights:**
  * **AST-Aware Markdown Compression:** Uses `pulldown-cmark` to parse the Markdown Abstract Syntax Tree, isolating prose `Event::Text` nodes while strictly preserving code blocks, headers, tables, links, and inline symbols.
  * **Symlink-Safe File Manipulation:** Mitigates symlink-clobber vulnerabilities by validating parent directory ownership, resolving real paths, and verifying `uid` ownership via `std::os::unix::fs::MetadataExt`.
  * **Multi-Agent Hook Orchestration:** Automatically detects and safely injects configurations into Claude Code (`settings.json`), Cursor (`.mdc`), Windsurf, and OpenClaw.
* **Inspiration for `minicode`:**
  * **AST-Safe Context Compaction:** In `minicode`'s `src/context/` token compactor, use `pulldown-cmark` AST extraction to strip redundant conversational filler from long session histories and tool outputs before forwarding to the LLM.
  * **Hardened File Operations:** Apply Caveman RS's symlink and permission validation logic in `minicode`'s `write_file` and `patch_file` tools to eliminate arbitrary path traversal and symlink-redirection risks.
* **Rust Crates / Dependencies Worth Noting:**
  * `pulldown-cmark`, `clap`, `serde_json`, `dirs`, `reqwest`.

---

## Part 3: Browser Engine & Headless Infrastructure

---

### 5. Obscura

* **Repository:** [https://github.com/h4ckf0r0day/obscura](https://github.com/h4ckf0r0day/obscura)
* **What It Does:**  
  Obscura is an open-source, Pure Rust headless browser engine designed specifically for AI agents and web scraping, featuring native V8 JavaScript execution, CSS layout/paint rendering, and deep anti-bot stealth without requiring Chromium.
* **Key Technical Highlights:**
  * **Embedded V8 Runtime:** Directly embeds the V8 engine (`v8` crate) for zero-dependency JavaScript execution and DOM manipulation.
  * **Pure Rust Layout & Paint Pipeline:** Implements CSS layout (flex, grid, block, table) and raster painting for viewport/full-page PNG screenshots and PDF exports without launching an external browser process.
  * **CDP Server & Protocol Emulation:** Speaks Chrome DevTools Protocol over WebSockets (`ws://127.0.0.1:9222`), serving as a drop-in backend for Puppeteer, Playwright, and MCP tools.
  * **Stealth & Anti-Fingerprinting:** Session-level fingerprint randomization (GPU, canvas, audio, battery), `navigator.userAgentData` spoofing, native function masking (`Function.prototype.toString`), and an integrated 3,520-domain ad/tracker blocker.
  * **Streaming Network Responses:** Implements `Fetch.takeResponseBodyAsStream` and chunked buffer handling to process massive assets without loading entire payloads into memory.
* **Inspiration for `minicode`:**
  * **Lean Alternative to Chromium:** `minicode` currently lists `chromiumoxide` as an optional feature, which requires an external Google Chrome/Chromium installation. Obscura's model offers a standalone, headless browsing solution.
  * **Integrated Web Scraping in `fetch_or_browse`:** Borrow Obscura's streaming response chunking and tracker-blocking heuristics to speed up web document extraction while saving memory.
* **Rust Crates / Dependencies Worth Noting:**
  * `v8`, `rustls`, `wreq` (BoringSSL), `tokio`, `image`, `tokio-tungstenite`.

---

## Part 4: Semantic & Navigational Memory Infrastructure

---

### 6. Sediment

* **Repository:** [https://github.com/rendro/sediment](https://github.com/rendro/sediment)
* **What It Does:**  
  Sediment is an embedded, local-first semantic memory system for AI agents written in Rust, combining vector embeddings with an SQLite relationship graph, time-decay scoring, and automatic deduplication.
* **Key Technical Highlights:**
  * **Two-Database Hybrid Storage:**
    1. **LanceDB:** Vector database for semantic similarity embeddings.
    2. **SQLite (`access.db`):** Graph relationships (`RELATED`, `SUPERSEDES`, `CO_ACCESSED`), access timestamps, decay calculations, and consolidation queues.
  * **Local Embeddings with Candle:** Uses HuggingFace's `candle` framework running `all-MiniLM-L6-v2` locally (384-dimensional vectors) with zero external API calls and SHA-256 model verification.
  * **Memory Decay & Graph Re-ranking:** Re-ranks query results using a 30-day exponential half-life curve combined with access frequency and 1-hop graph neighbor expansions.
  * **Background Consolidation:** Merges near-duplicates (≥0.95 cosine similarity) and links related context (0.85–0.95 similarity) in background worker tasks.
* **Inspiration for `minicode`:**
  * **Long-Term Session Memory:** Implement a minimalist embedded memory layer in `minicode` (`.minicode/memory.db`) allowing the agent to recall project-specific architectural decisions, past bug fixes, and user preferences across distinct sessions.
  * **Hybrid Vector + Recency Scoring:** Apply exponential decay weighting to prevent stale context from crowding active token budgets.
* **Rust Crates / Dependencies Worth Noting:**
  * `lancedb`, `rusqlite`, `candle-core`, `candle-transformers`, `tokenizers`, `tokio`.

---

### 7. Capn Hook

* **Repository:** [https://github.com/CyrusNuevoDia/capn-hook](https://github.com/CyrusNuevoDia/capn-hook)
* **What It Does:**  
  Capn Hook is a persistent navigational memory tool for coding agents that maps high-effort codebase discoveries (question → relevant files) and automatically invalidates (cache-busts) saved charts whenever backing files are modified.
* **Key Technical Highlights:**
  * **Coastline Model of Code:** Treats codebase navigation as exploratory charts. Rather than heavy static indexing, it records exact answers to specific questions (`capn chart "<query>" --files <a,b> --details "<gotcha>"`).
  * **SHA-256 Cache-Busting:** Every referenced file is fingerprinted at save time. If any backing file changes or is deleted, the chart is pruned immediately before it can return stale guidance.
  * **77% Token Savings on Repeat Exploration:** Prevents agents from repeatedly executing costly grep, find, and file traversal operations for recurring conceptual questions.
  * **Zero-Wrapper Hook Injection:** Injects a lightweight context snippet via `.claude/settings.json` and `.codex/hooks.json` nudging the agent to ask before searching.
* **Inspiration for `minicode`:**
  * **Navigational Discovery Cache:** In `minicode`, create an internal navigational cache in `.minicode/nav_cache.json`. When `minicode` completes an exploratory search (e.g., finding where authentication middleware is implemented), store the query and matching file paths with their SHA-256 hashes.
  * **Automatic Invalidation on Mutation:** Hook into `minicode`'s `write_file` and `patch_file` tools to instantly invalidate any cached navigational entries that include the modified file.
* **Dependencies Worth Noting:**
  * Node/Bun CLI, `qmd` (hybrid BM25 + vector search), `sqlite3`.

---

## Part 5: Background Task & Async Scheduling

---

### 8. Tokio Cron Scheduler

* **Repository:** [https://github.com/mvniekerk/tokio-cron-scheduler](https://github.com/mvniekerk/tokio-cron-scheduler)
* **What It Does:**  
  Tokio Cron Scheduler is an async task scheduling library in Rust that enables cron-like schedules, recurring intervals, and one-shot delayed executions directly on the Tokio runtime.
* **Key Technical Highlights:**
  * **Native Tokio Integration:** Spawns asynchronous, non-blocking jobs using `tokio::spawn` and timer queues without dedicated OS threads.
  * **Flexible Cron Grammar:** Supports standard 5-field and 6/7-field expressions (including seconds) via the `croner` crate, with timezone awareness via `chrono-tz`.
  * **Natural Language Parsing:** Optional `english` feature allowing human phrases like `"every 10 seconds"` or `"every day at midnight"` via `english-to-cron`.
  * **Lifecycle Notifications:** Provides async hooks (`on_start_notification_add`, `on_stop_notification_add`, `on_removed_notification_add`) for telemetry and status tracking.
  * **Graceful Signal Handling:** Built-in `shutdown_on_ctrl_c()` and signal monitors for clean teardown.
* **Inspiration for `minicode`:**
  * **Agent Background Timers & Cron Tasks:** `minicode`'s tool ecosystem includes `schedule` for timers and cron jobs. `tokio-cron-scheduler` is the ideal native engine to drive background liveness checks, automated file watcher syncs, periodic indexing, and timer wakeups.
  * **Natural Language Timer Support:** Parse human time intervals in the TUI (e.g., `/timer 5m check build`).
* **Rust Crates / Dependencies Worth Noting:**
  * `croner`, `tokio`, `chrono`, `chrono-tz`, `uuid`, `async-trait`.

---

## Part 6: Core CLI & TUI Terminal Crates

---

### 9. Colorchoice

* **Crate:** [https://crates.io/crates/colorchoice](https://crates.io/crates/colorchoice)
* **What It Does:**  
  A lightweight, zero-dependency Rust crate that provides a standardized global color choice configuration enum (`ColorChoice::Auto`, `Always`, `Never`) across CLI applications and library ecosystems.
* **Key Technical Highlights:**
  * Standardizes color override logic across CLI flags (`--color=auto|always|never`) and environment variables (`NO_COLOR`, `CLICOLOR`, `CLICOLOR_FORCE`).
  * Integrates seamlessly with `anstream`, `anstyle`, and `clap`.
* **Inspiration for `minicode`:**
  * Standardize terminal styling behavior across `minicode`'s TUI, Plain REPL, and NDJSON streaming modes with unified CLI flags.

---

### 10. Anstyle-Query

* **Crate:** [https://crates.io/crates/anstyle-query](https://crates.io/crates/anstyle-query)
* **What It Does:**  
  A low-level, `no-std` compatible utility crate for querying terminal capabilities, determining whether a terminal supports ANSI styling, 256 colors, or 24-bit TrueColor.
* **Key Technical Highlights:**
  * Fast terminal probing via `term_supports_ansi_color()`, `term_supports_color()`, and Windows console mode API checks.
  * `no-std` compatible with zero runtime allocation overhead.
* **Inspiration for `minicode`:**
  * Pre-flight terminal capability detection before initializing the Ratatui full-color Aura Theme. If running in a dumb terminal, restricted SSH session, or redirected pipe, seamlessly fall back to Plain ASCII mode without panicking.

---

### 11. Crossterm

* **Crate:** [https://crates.io/crates/crossterm](https://crates.io/crates/crossterm)
* **What It Does:**  
  The industry-standard, pure Rust cross-platform terminal manipulation library powering modern TUIs across Linux, macOS, and Windows.
* **Key Technical Highlights:**
  * **Raw Mode & Alternate Screen:** Complete control over terminal buffers, raw input handling, and mouse/cursor capture without ncurses or C runtime dependencies.
  * **Async EventStream:** Native integration with Tokio via `crossterm::event::EventStream`, yielding keyboard, mouse, paste, and resize events asynchronously.
  * **Bracketed Paste:** Supports `crossterm::event::EnableBracketedPaste` to safely distinguish multi-line pastes from rapid keyboard typing.
* **Inspiration for `minicode`:**
  * `minicode` already uses `crossterm = "0.28"`. Best practices to ensure:
    1. Always register a custom panic hook to restore the terminal (`DisableRawMode`, `LeaveAlternateScreen`, `ShowCursor`) to prevent freezing user terminals on panic.
    2. Enable bracketed paste for flawless multi-line code prompt pasting.

---

### 12. Clearscreen

* **Crate:** [https://crates.io/crates/clearscreen](https://crates.io/crates/clearscreen)
* **What It Does:**  
  A resilient cross-platform screen clearing crate for Rust that cleanly clears terminal viewports and scrollback buffers across various terminals, SSH clients, and multiplexers (tmux/screen).
* **Key Technical Highlights:**
  * Multi-tiered fallback strategy: uses ANSI escape sequences (`\x1b[2J\x1b[3J\x1b[H`), Windows console buffer APIs, and native fallback execution (`clear` / `cls`).
  * Clears both the visible screen and the terminal scrollback history.
* **Inspiration for `minicode`:**
  * Implement the `/clear` command in `minicode`'s TUI and Plain REPL modes using `clearscreen::clear()` to guarantee zero artifact residue when resetting timelines.

---

### 13. Pico-Args

* **Crate:** [https://crates.io/crates/pico-args](https://crates.io/crates/pico-args)
* **What It Does:**  
  An ultra-minimalist, zero-dependency command-line argument parser for Rust with near-zero compilation overhead and tiny binary footprint.
* **Key Technical Highlights:**
  * Zero dependencies, <1000 LoC, sub-10ms compilation time.
  * Sequential streaming API: parses flags (`contains`), options (`value_from_str`), and subcommands directly from `std::env::args_os()`.
* **Inspiration for `minicode`:**
  * While `minicode` uses `clap` for its main rich CLI and derive macros, `pico-args` is the benchmark choice for any internal companion binaries, test runners, or MCP shim utilities where minimal binary size and instant compile times are mandatory.

---

## Comparison Matrix

| Project / Crate | Primary Domain | Core Tech / Language | Key Architecture Pattern | minicode Relevance |
| :--- | :--- | :--- | :--- | :--- |
| **Sentrux** | Architectural Governance | Pure Rust, Tree-sitter, WGPU | Continuous structural feedback loop & 0-10000 quality score | Quality gating & AST query plugins |
| **MicroSandbox** | Sandboxing | Rust, KVM/WHP/Hypervisor | Embeddable microVM with zero-leak secret proxy | Tier-2 hardware isolation & egress proxy |
| **CubeSandbox** | Cloud Sandboxing | Rust, RustVMM, KVM, eBPF | Sub-60ms boot, <5MB RAM, CoW snapshot/rollback | Rollback engine & lean VMM patterns |
| **Obscura** | Headless Browser | Pure Rust, Embedded V8 | Standalone CDP browser, CSS layout/paint & stealth | Chromiumoxide replacement & lean scraping |
| **Sediment** | Agent Semantic Memory | Pure Rust, LanceDB, SQLite | Vector search + SQLite relationship graph + decay | Long-term memory & recency decay |
| **Capn Hook** | Navigational Memory | TypeScript/Bun, QMD | Atomic question-file charts with SHA-256 invalidation | Fast discovery caching with auto-bust |
| **Caveman RS** | Token Optimization | Pure Rust, pulldown-cmark | Markdown AST text compression & symlink-safe fs | AST context compaction & safe fs ops |
| **Tokio Cron Scheduler** | Async Scheduling | Pure Rust, Tokio, croner | Non-blocking async cron & interval job dispatcher | Native schedule tool & background workers |
| **Colorchoice** | Terminal Styling | Pure Rust | Standardized `ColorChoice` enum for CLI & libs | Unified `--color` CLI argument handling |
| **Anstyle-Query** | Terminal Probing | Pure Rust, `no-std` | Low-level ANSI/color capability detection | Graceful TUI/Plain mode degradation |
| **Crossterm** | Terminal I/O | Pure Rust, WinAPI/termios | Raw mode, alternate screens & async EventStream | TUI engine & panic recovery hooks |
| **Clearscreen** | Terminal Manipulation | Pure Rust | Multi-strategy screen & scrollback clearing | Clean `/clear` command execution |
| **Pico-Args** | CLI Parsing | Pure Rust, 0-deps | Sequential streaming CLI argument extraction | Ultra-lightweight helper binaries |

---

## Top 5 Actionable Ideas for `minicode`

### 1. Ephemeral Navigational Discovery Cache (Inspired by Capn Hook & Caveman RS)
* **Problem:** `minicode` repeatedly executes `grep_search` and Tree-sitter AST traversals across multi-turn sessions when trying to locate specific domain logic (e.g., "Where are auth routes defined?").
* **Solution:** Implement a local navigational cache in `.minicode/cache/nav_index.json`.
  * When a search succeeds, map the query phrase to the list of matching file paths along with their SHA-256 content hashes.
  * On subsequent turns, check the cache first. If all referenced files match their stored SHA-256 hashes, return the paths in <1ms without token-heavy search turns.
  * Whenever `write_file` or `patch_file` modifies a file, immediately prune all cache entries referencing that file path.

### 2. Pre/Post-Task Architectural Quality Gating (Inspired by Sentrux)
* **Problem:** AI coding agents frequently introduce circular dependencies, tangle modular boundaries, or create monolithic "god files" during complex refactoring tasks.
* **Solution:** Leverage `minicode`'s existing `petgraph` and `tree-sitter` pipeline in `src/context/`:
  * Before starting a multi-step task, take an architectural snapshot of the workspace module graph (calculating cycle count and maximum in/out coupling).
  * After the task completes, evaluate the modified graph. If new circular dependency cycles were introduced or layer boundaries breached (e.g. `src/tools/` importing private internals of `src/agent/`), warn the agent immediately to self-correct before presenting the final diff to the user.

### 3. Outbound Egress Credential Vault (Inspired by MicroSandbox & CubeSandbox)
* **Problem:** Passing API keys, tokens, and secrets via environment variables into `exec_cmd` risks leaking sensitive credentials to child processes, terminal outputs, or agent logs.
* **Solution:** Adopt a local loopback security proxy pattern for outbound web requests and command execution.
  * Replace real API tokens in the agent's execution environment with dummy placeholder tokens (e.g., `MINICODE_VAULT_OPENROUTER_KEY`).
  * In the network proxy layer (`fetch_or_browse` or sandboxed HTTP clients), automatically replace the placeholder tokens with the real secrets loaded from `.env` or system keyring before the request leaves localhost.

### 4. Terminal Capability Probing & Bulletproof TUI Recovery (Inspired by anstyle-query, clearscreen & crossterm)
* **Problem:** TUI applications can crash or render unreadable ANSI artifacts when invoked in constrained terminal environments (e.g., non-TrueColor terminals, SSH sessions, dumb pipes), or leave the user's terminal in a broken raw state upon panics.
* **Solution:**
  * Use `anstyle-query` during startup in `src/main.rs` to detect if the terminal supports TrueColor and ANSI styling. Automatically degrade to `--plain` mode if stdout is not a TTY or colors are unsupported.
  * Install a global `std::panic::set_hook` that calls `clearscreen::clear()` and restores crossterm terminal state (`disable_raw_mode`, `LeaveAlternateScreen`, `ShowCursor`) to guarantee the user's terminal is never corrupted.
  * Add `crossterm::event::EnableBracketedPaste` to allow pasting large multi-line prompts and code blocks into the Aura TUI without triggering accidental command execution.

### 5. Native In-Process Task & Cron Scheduling Engine (Inspired by tokio-cron-scheduler)
* **Problem:** Agent workflows often require scheduled background execution (e.g., checking test runners, periodic git stash backups, or polling external CI webhooks) without blocking the interactive TUI event loop.
* **Solution:** Embed `tokio-cron-scheduler` inside `minicode`'s async runtime.
  * Power the `schedule` tool natively: allow agents and users to set one-shot delayed timers (`Job::new_one_shot`) or recurring cron checks (`Job::new_async`).
  * Run periodic background housekeeping (e.g., cleaning up old snapshot backups in `.minicode/backups/` older than 7 days) without impacting interactive response latency.

---

## Conclusion

By adopting these patterns from the broader Rust systems and AI tooling ecosystem, `minicode` can significantly strengthen its sandboxing guarantees, prevent architectural code decay, slash token overhead on repetitive explorations, and deliver a resilient, glitch-free terminal experience.

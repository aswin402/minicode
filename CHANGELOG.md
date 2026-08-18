# Changelog 📜

All notable changes to **minicode** will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.0.22] — 2026-08-18

### 🛠️ Dynamic Skill Creation & Hot-Reloading Engine
- **Dynamic Skill Forge (`src/context/skill_forge.rs`)**:
  - Allows autonomous agents and developers to forge, validate, and hot-load project-specific skill packages into `.minicode/skills/<name>/SKILL.md`.
  - Automatic YAML frontmatter serialization with metadata (`name`, `description`, `version`, `author`, `allowed_tools`).
  - Hot-reloading integration with `SkillDiscoverer` for zero-restart skill activation.
  - Added 3 new tools: `create_skill`, `list_skills`, and `inspect_skill` (expanding built-in tools to **41 agent tools total**).
- **Integration Test Suite**:
  - Added `tests/integration_skill_forge.rs` validating skill creation, metadata parsing, directory inspection, and tool execution.
  - Test suite expanded to **118 tests passing 100% green** with zero clippy warnings.

---

## [0.0.21] — 2026-08-16

### ⚡ Interactive Embedded PTY Terminal Drawer
- **Embedded PTY Terminal Drawer (`src/ui/pty_drawer.rs`)**:
  - Implemented interactive bottom drawer overlay (bottom 40% viewport) with Aura Mint Green borders.
  - Bounded 1000-line output ring buffer with auto-scroll and color-coded status styling.
  - Direct shell command execution without leaving the Ratatui TUI session.
  - Fast keyboard shortcuts: `Ctrl+T` to toggle, `Esc` to close, `Enter` to run.
  - Added `/terminal` slash command for terminal drawer management.
- **Integration Test Suite**:
  - Added `tests/integration_pty_drawer.rs` validating drawer state toggle, line wrapping, ring buffer limits, and test backend rendering.
  - Test suite expanded to **115 tests passing 100% green** with zero clippy warnings.

---

## [0.0.20] — 2026-08-16

### 🌐 ARIA Web Browser & Local UI Dev Inspector
- **ARIA Accessibility Tree Generator (`src/tools/browser.rs`)**:
  - Implemented pure-Rust DOM & accessibility tree extractor for live web pages and local dev servers (`http://localhost:3000`, `http://localhost:8080`).
  - Converts interactive controls (`<button>`, `<a>`, `<input>`, `<select>`, `<textarea>`) into numbered element references (`@e1`, `@e2`, ...).
  - Extracts form actions, input names/types, and links for testing web applications.
  - Added tools: `browser_navigate` and `browser_snapshot` (expanding built-in tools to **38 agent tools total**).
- **Integration Test Suite**:
  - Added `tests/integration_browser.rs` validating HTML accessibility tree generation, element reference resolution, and tool dispatch.
  - Test suite expanded to **112 tests passing 100% green** with zero clippy warnings.

---

## [0.0.19] — 2026-08-16

### 📚 Cognitive Memory Decay & Repository Knowledge Wiki
- **Exponential Cognitive Memory Decay (`src/context/decay.rs`)**:
  - Implemented Ebbinghaus-inspired temporal memory retention modeling ($R(t) = \exp(-\ln(2) \cdot t / (H \cdot S))$).
  - Scope partitioning: `Permanent` (zero decay for repo rules), `Milestone` (7-day half-life for active goals), and `Transient` (60-minute half-life for episodic debug traces).
  - Automatic reinforcement boosts stability factor upon re-access.
- **Compounding Knowledge Wiki Engine (`src/context/wiki.rs`)**:
  - Filesystem-backed Markdown knowledge base in `.minicode/wiki/<topic>.md`.
  - Automatic `.minicode/wiki/index.md` cataloging and topic frontmatter serialization.
  - Added tools: `wiki_write`, `wiki_read`, and `wiki_search` (expanding built-in tools to **36 agent tools total**).
- **Integration Test Suite**:
  - Added `tests/integration_wiki_and_decay.rs` validating biological memory retention decay, pruning, and full wiki CRUD tool lifecycle.
  - Test suite expanded to **109 tests passing 100% green** with zero clippy warnings.

---

## [0.0.18] — 2026-08-16

### 🔀 Sequential Thinking & Graph of Thoughts (GoT) Reasoning
- **Graph of Thoughts Reasoning Engine (`src/agent/sequential_thinking.rs`)**:
  - Implemented dynamic hypothesis branching, revision tracking, and confidence scoring (`score: 0.0`–`1.0`).
  - Tracks non-linear thinking trajectories in a directed graph using `petgraph`.
  - Added tool: `sequential_thinking` (expanding built-in tools to **33 agent tools total**).
  - Generates synthesized outline summaries upon convergence for complex debugging and architectural planning turns.
- **Integration Test Suite**:
  - Added `tests/integration_sequential_thinking.rs` validating thought node progression, hypothesis branching, and tool dispatch.
  - Test suite expanded to **104 tests passing 100% green** with zero clippy warnings.

---

## [0.0.17] — 2026-08-16

### 🧠 Topological Task DAG & Actor-Critic Verification Engine
- **Petgraph-Powered Task DAG Engine (`src/agent/task_dag.rs`)**:
  - Implemented dependency-managed Directed Acyclic Graph (DAG) for multi-step feature execution.
  - Cycle detection and topological execution ordering via `petgraph::algo::toposort`.
  - Heuristic complexity scoring engine (1–10 scale) based on scope, affected file depth, and risk keywords.
  - Added tools: `create_task_dag`, `get_next_task`, and `complete_task`.
- **Actor-Critic Quality Gate (`src/agent/critic.rs`)**:
  - Automated dual-agent verification pass running compiler diagnostics, linter checks, and git modification analysis.
  - Added tool: `critic_review` (expanding built-in tools to **32 agent tools total**).
- **Integration Test Suite**:
  - Added `tests/integration_task_dag.rs` validating dependency graphs, topological order resolution, unblocked task queries, and critic reviews.
  - Test suite expanded to **101 tests passing 100% green** with zero clippy warnings.

---

## [0.0.16] — 2026-08-16

### 📦 Multi-Platform Release Matrix & Single-Command Distribution
- **Automated Multi-Platform Release Matrix (`.github/workflows/release.yml`)**:
  - Implemented GitHub Actions release workflow triggering on version tags (`v*`).
  - Cross-compilation and automated asset packaging for 5 tier-1 platforms:
    - `x86_64-unknown-linux-gnu` (Linux x86_64 tar.gz + sha256)
    - `aarch64-unknown-linux-gnu` (Linux ARM64 tar.gz + sha256)
    - `aarch64-apple-darwin` (macOS Apple Silicon tar.gz + sha256)
    - `x86_64-apple-darwin` (macOS Intel tar.gz + sha256)
    - `x86_64-pc-windows-msvc` (Windows x86_64 zip + sha256)
- **One-Line Curl Installer (`install.sh`)**:
  - Auto-detects operating system (`uname -s`) and CPU architecture (`uname -m`).
  - Downloads the matching release archive directly from GitHub Releases and installs to `~/.local/bin/minicode`.
  - Gracefully falls back to `cargo install` if GitHub API rate limits occur.
- **Documentation & PRD Synchronization**:
  - Updated all feature tracker checklists, tool inventories (28 agent tools), and architecture specs in `onpkg_docs/`.

---

## [0.0.15] — 2026-08-16

### 🔍 Native Web Search Engine & Anti-Scrape Cache
- **Native Web Search Engine (`src/tools/web_search.rs`)**:
  - Implemented multi-provider web search tool `search_web` (expanding built-in tools to **28 agent tools total**).
  - **Zero-API-key DuckDuckGo Scraper**: High-resilience HTML endpoint scraping with HTML parser, title & snippet extraction, and clean Markdown links.
  - **In-Memory TTL Search Cache**: Thread-safe 15-minute query caching to avoid rate-limiting during multi-turn planning sessions.
  - **Optional API Fallbacks**: Seamlessly integrates with Tavily (`TAVILY_API_KEY`) and Brave Search (`BRAVE_API_KEY`) when environment variables are set.
- **Integration Test Suite**:
  - Added `tests/integration_web_search.rs` verifying markdown search result compilation, link formatting, and empty query rejection.
  - Full test suite expanded to **96 tests passing 100% green** with zero clippy warnings.

---

## [0.0.14] — 2026-08-16

### 🛡️ Interactive TUI Diff Inspector & Permission Menu Modal
- **Syntax-Highlighted Unified Diff Viewer (`src/ui/diff_viewer.rs`)**:
  - Implemented terminal diff engine powered by `similar`, rendering clean colored unified diff lines matching Dalton Menezes' Aura Theme (`+` Aura Mint Green additions, `-` Aura Coral Red deletions, muted context).
  - Truncates oversized diff hunks gracefully with line counts.
- **Interactive 4-Option Permission Selection Modal (`src/ui/approval.rs`, `src/ui/modal.rs`, `src/app.rs`)**:
  - Replaced legacy single-key prompts with an interactive Aura modal menu displaying target action details and proposed changes.
  - Interactive navigable options (`↑` / `↓` / `j` / `k` or direct numbers `1`–`4`, `Enter` to confirm):
    - `[1] Accept & Apply (Execute action)`
    - `[2] Reject (Decline this action)`
    - `[3] Allow for this Session (Auto-approve subsequent turns)`
    - `[4] Type Feedback / Custom Instructions (Guide agent)`
  - Direct steering support: When option 4 is selected, an inline feedback input dock opens, sending custom instructions back to the agent loop.
- **Integration Test Suite**:
  - Added `tests/integration_diff_modal.rs` testing diff formatting, modal navigation, and custom feedback typing.
  - Test suite expanded to **92 tests passing 100% green** with zero clippy warnings.

---

## [0.0.13] — 2026-08-16

### 🚀 Language Server Protocol (LSP) Engine & 2-Tier Compiler Diagnostics
- **Pure-Rust Stdio JSON-RPC 2.0 Client (`src/lsp/protocol.rs`, `src/lsp/client.rs`)**:
  - Implemented async stdio JSON-RPC 2.0 framing with strict `Content-Length:` header reading and serialization.
  - Auto-discovery for language servers: `rust-analyzer` (Rust), `pyright` (Python), `typescript-language-server` (TypeScript/JavaScript), and `gopls` (Go).
  - Handles initialization handshakes, non-blocking requests with timeouts, and clean child process lifecycle management (`SIGTERM` / `SIGKILL` on drop).
- **2-Tier Hybrid Compiler Diagnostics Engine (`src/lsp/diagnostics.rs`, `src/lsp/mod.rs`)**:
  - **Tier 1 (Instant Fast-Path)**: Direct compiler CLI checks (`cargo check --message-format=json`, `tsc --noEmit`, `ruff check`) running in < 200ms with zero RAM overhead.
  - **Tier 2 (Deep Semantic LSP)**: Asynchronous LSP client providing live diagnostics, `lsp_goto_definition`, and `lsp_find_references`.
  - Added 3 new agent tools: `lsp_diagnostics`, `lsp_goto_definition`, and `lsp_find_references` (expanding built-in tools to **27 agent tools total**).
- **Autonomous Compiler Self-Healing Loop (`src/agent/loop.rs`, `src/config.rs`)**:
  - Automatically queries workspace compiler diagnostics after file modifications.
  - If compiler errors are detected, the agent receives an immediate structured feedback prompt with line, column, and rustc/tsc error spans, automatically fixing syntax or type errors before completing the turn.
  - Configurable via `[agent] auto_heal = true` in `config.toml`.
- **Integration Test Suite**:
  - Added `tests/integration_lsp_diagnostics.rs` validating protocol framing, diagnostic formatting, and tool dispatch.
  - Total test count expanded to **86 tests passing 100% green** with zero clippy warnings.

---

## [0.0.12] — 2026-08-16

### 🚀 Autonomous Git Engine & Worktree Orchestration
- **Autonomous Git Service Engine (`src/git/service.rs`, `src/git/commit.rs`, `src/git/diff_filter.rs`, `src/git/worktree.rs`)**:
  - Implemented hardened Git engine using isolated async subprocesses (`tokio::process::Command`) with mandatory flags: `GIT_TERMINAL_PROMPT=0`, `GIT_PAGER=cat`, `LC_ALL=C`, and `--no-pager`.
  - Added token-budgeted `DiffFilter` that automatically collapses multi-thousand line lockfiles (`Cargo.lock`, `package-lock.json`, `pnpm-lock.yaml`, `yarn.lock`, etc.) and enforces strict byte budgets (`GIT_DIFF_MAX_BYTES`).
  - Added 6 autonomous agent git tools: `git_status`, `git_diff`, `git_commit`, `git_log`, `git_conflicts`, and `create_pr` (24 built-in agent tools total).
  - Implemented `WorktreeManager` supporting isolated parallel subagent branches (`subagent/<id>`) and zero-conflict branch merging.

### 🤖 Autonomous Post-Turn Auto-Commit & Reversible Rollbacks
- **Autonomous Auto-Commit Loop (`src/agent/loop.rs`, `src/config.rs`)**:
  - Automatically commits modified files after successful agent turns using generated Conventional Commits messages (`feat: ...`, `fix: ...`, `docs: ...`).
  - Configurable in `config.toml` (`[git] auto_commit = true, dirty_commit = false, ai_commit_messages = true`).
  - Synchronized `/undo` rollback to execute `git reset --soft HEAD~1` alongside filesystem checkpoint restoration.

### 🧪 Comprehensive Integration Test Suite & CI/CD Pipeline
- **GitHub Actions CI (`.github/workflows/ci.yml`)**:
  - Full CI pipeline validating code formatting (`cargo fmt --check`), strict clippy lints (`cargo clippy -- -D warnings`), and cross-platform tests across Linux (`Ubuntu`) and macOS (`macOS`).
- **Two-Tier Integration Test Framework (`tests/`)**:
  - Created `src/lib.rs` and `tests/common/mock_provider.rs` for deterministic multi-turn simulation without network overhead.
  - Added `tests/integration_agent_loop.rs` verifying multi-turn read-and-patch turns and user cancellation.
  - Added `tests/integration_git_tools.rs` verifying autonomous git tools, auto-commits, and undo rollbacks.
  - Added `tests/integration_subagents.rs` verifying parallel worktrees and branch merging.
  - **80/80 tests passing 100% green**.

### 🖥️ Session Controls & Codebase Observability
- **Extended Slash Commands (`src/ui/input.rs`, `src/app.rs`)**:
  - `/retry`: Re-submits the last prompt to the agent loop.
  - `/save [path]`: Exports the current session timeline to formatted Markdown.
  - `/load [id]`: Hydrates timeline from past session stores.
  - `/map`: Renders the AST PageRank repository map directly in the timeline.
  - `/compact`: Triggers context token compaction.
  - `/tokens`: Displays detailed token breakdown and context window metrics.

---

## [0.0.11] — 2026-08-16

### 🚀 Critical Fixes & Resilience Upgrades
- **Gemini Multi-Tool Turn Merging & ID Support (`src/agent/provider.rs`)**:
  - Merged adjacent `Role::Tool` messages into a single `role: "user"` message with multiple `functionResponse` parts to adhere strictly to the Gemini multi-turn API format and prevent HTTP 400 Bad Request errors.
  - Attached call `id` to both `functionCall` and `functionResponse` for seamless Gemini 2.0+ tool call matching.
- **CodeGraph Dangling Node Mass Redistribution & L1 Normalization (`src/context/graph.rs`)**:
  - Resolved graph probability leakage by redistributing dangling node mass across all nodes during PageRank power iteration, followed by exact L1 normalization ($\sum P_i = 1.0$).
  - Upgraded symbol-to-file lookup to support multiple declarations of identical identifiers across distinct files with keyword noise filtering.
- **Cooperative Agent Cancellation (`src/agent/loop.rs`, `src/app.rs`, `Cargo.toml`)**:
  - Integrated `tokio_util::sync::CancellationToken` into `AgentLoop::execute_turn` to enable immediate and clean cancellation on `Esc` / `Ctrl+C` across LLM streaming, token processing, and tool dispatching.
  - Fixed token usage inflation by tracking `last_prompt_tokens + cumulative_completion_tokens`.
- **Automatic Backup Manifest Persistence (`src/session/backup.rs`)**:
  - Safety checkpoint creation now automatically creates or updates the turn's `manifest.json` on disk using validated absolute workspace paths.

### 🛡️ Security & Sandbox Hardening
- **SSRF Network Protection (`src/tools/web.rs`, `src/constants.rs`, `src/error.rs`)**:
  - Added strict SSRF validation to `fetch_or_browse` blocking localhost, IPv4 loopback (`127.0.0.0/8`), IPv6 loopback (`::1`), link-local (`169.254.0.0/16`), private subnets (`10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16`, `100.64.0.0/10`), and cloud metadata endpoints (`169.254.169.254`).
- **Landlock Kernel Compatibility & Graceful Degradation (`src/sandbox/landlock.rs`)**:
  - Added robust detection for unsupported host kernels (`ENOSYS`, `EOPNOTSUPP`), allowing WSL2, Docker containers, and older Linux kernels to degrade gracefully without crashing.
- **Process Group Termination with SIGKILL Escalation (`src/tools/exec.rs`, `Cargo.toml`)**:
  - Subprocess timeouts now send `SIGTERM` followed by a grace period before escalating to `SIGKILL` directly via `libc::kill`.

### ⚡ Context Engine, UI & Configuration
- **Expanded Tree-sitter Queries (`src/context/repomap.rs`)**:
  - Extended JavaScript and TypeScript queries to extract class methods, arrow functions, `enum_declaration`, and interface properties.
  - Hardened AST cache invalidation using a `(modified_time, file_size)` tuple.
- **Gemini Header Authentication & Stream Error Extraction (`src/agent/provider.rs`, `src/agent/models.rs`)**:
  - Switched Gemini API authentication to standard `x-goog-api-key` header instead of URL query parameters.
  - Extracted prompt-level policy blocks (`blockReason`), API stream errors, and candidate `finishReason` stops (`SAFETY`, `RECITATION`, `BLOCKLIST`).
- **Ollama TOML Configuration & Status Display (`src/config.rs`, `src/ui/status.rs`, `src/app.rs`)**:
  - Added `pub ollama: Option<OllamaConfig>` to `RawProviderConfig` and merged in `merge_raw`.
  - Updated Aura TUI status bar to display the active provider along with the model name (e.g., `gemini:gemini-2.5-pro`).

---

## [0.0.9] — 2026-08-16

### 🚀 Critical Bug Fixes & Reliability Hardening
- **Google Gemini Function Calling Name Alignment (`src/agent/types.rs`, `src/agent/loop.rs`, `src/agent/provider.rs`)**:
  - Attached real `tool_name` to `Message::tool_result` constructor so Gemini receives declared tool names (`read_file`, `write_file`) in `functionResponse.name` rather than synthetic UUIDs, preventing API 400 Bad Request errors.
- **MCP Client Multi-Server Tool Discovery & Stdio Streaming (`src/mcp/client.rs`, `src/constants.rs`)**:
  - Implemented `discover_server_tools` to dynamically query connected stdio and HTTP/SSE MCP servers via `tools/list` on initialization.
  - Replaced blocking `wait_with_output()` with asynchronous `BufReader::lines()` streaming reader to support persistent stdio MCP daemons without timing out.
  - Added robust `extract_tool_output` with `isError: true` payload extraction.
- **Child Process Pipe Drain & Process Group Termination (`src/tools/exec.rs`)**:
  - Concurrently drains subprocess pipe buffer overflow to `tokio::io::sink()` to prevent child processes from blocking on full OS pipe buffers when output exceeds 512 KB.
  - Enforces process group isolation with `process_group(0)` on Unix and terminates full process trees on timeout.
- **MCP Server Full Tool Parity (`src/mcp/server.rs`)**:
  - Delegated MCP server tool execution directly to `ToolRegistry::dispatch` to expose all 17 coding tools with backup snapshots, sandbox enforcement, and standard `isError` response payloads.
- **Backup & Undo Workspace Path Confinement (`src/session/backup.rs`, `src/session/undo.rs`)**:
  - Enforced `validate_path_in_workspace` on safety checkpoint creation and undo rollbacks to eliminate path traversal vulnerabilities.

### 🧠 Context Engine & Search Enhancements
- **Context Pruning Conversation Turn Invariant (`src/agent/loop.rs`)**:
  - Guaranteed that pruning never leaves an orphaned `Role::Tool` message at the beginning of LLM conversation history.
- **Observation Masking Dynamic Budget (`src/context/compressor.rs`)**:
  - Derived head and tail lines dynamically from `max_lines` to prevent zero or negative truncation slices.
- **BM25 Prefix Range Query Deduplication & IDF Calculation (`src/context/index.rs`)**:
  - Deduplicated postings per query token during BTreeMap prefix range scans and computed correct document frequency for prefix matches.
- **Deterministic Parallel Tool Call Ordering (`src/agent/provider.rs`)**:
  - Switched tool call streaming accumulator to `BTreeMap<usize, ...>` to guarantee deterministic execution order across parallel tool dispatches.
- **Landlock Sandbox Permitted Paths Expansion (`src/sandbox/landlock.rs`)**:
  - Added `/dev`, `/proc`, `/opt`, `/usr/local`, and user toolchain directories (`~/.cargo`, `~/.rustup`, `~/.nvm`, `~/.local`) to Landlock permitted read paths.
- **Search File Pattern Matching for Nested Paths (`src/tools/search.rs`)**:
  - Supported filename and relative path glob matching for patterns like `*.rs` and `client.rs` across nested directories.

### ⚡ Interactive TUI State Sync
- **Runtime Configuration Synchronization (`src/app.rs`, `src/agent/loop.rs`)**:
  - Implemented `AgentCommand::UpdateConfig` channel to immediately sync provider, model, and API key updates from the in-TUI modal dialog to the active background agent loop.
- **Doc Comment Attribute Parsing (`src/context/repomap.rs`)**:
  - Skipped Rust attributes (`#[...]`, `#![...]`) and Python decorators (`@...`) when extracting doc comments in Tree-sitter AST repo mapping.
- **Working Memory Findings Prompt Injection (`src/context/working_memory.rs`)**:
  - Linked discoveries and key findings from `findings.md` into the active `<working_memory>` system prompt block.

---

## [0.0.8] — 2026-08-15

### 🧠 Context Window Management & Memory Protection
- **Sliding Window Conversation Context Pruner (`src/agent/loop.rs`, `src/constants.rs`)**:
  - Implemented `prune_context()` in `AgentLoop` which actively monitors token budget using `ContextCompressor`.
  - Automatically compacts older tool observations and prunes excess oldest conversation messages when approaching `CONTEXT_WINDOW_PRUNE_THRESHOLD` (100,000 tokens) while guaranteeing preservation of recent turns (`CONTEXT_MIN_PRESERVED_MESSAGES`).
  - Added unit tests `test_prune_context_preserves_minimum_messages` and `test_prune_context_compacts_and_prunes_large_history`.

### 🛡️ Error Visibility & Resilience
- **Model Cache I/O Warning Logs (`src/agent/models.rs`)**:
  - Replaced silent `.ok()` error swallowing with structured `tracing::warn!` logging for model cache directory creation, file write, and corrupted JSON read recovery.
- **Temp File Atomic Write Cleanup (`src/tools/fs.rs`, `src/context/memory.rs`)**:
  - Logged warnings on temporary file removal failures during atomic write rollback instead of discarding errors silently.

### 🧹 Dead Code & Interface Polish
- **Removed Blanket `#![allow(dead_code)]` (`14 files`)**:
  - Eliminated file-level blanket dead code suppression across `agent/`, `context/`, `session/`, `sandbox/`, and `ui/` modules.
  - Replaced with targeted, item-level `#[allow(dead_code)]` attributes on specific public API structs and methods.
- **Provider & Directory Constants Centralization (`src/constants.rs`, `src/agent/provider.rs`, `src/agent/models.rs`, `src/tools/exec.rs`, `src/tools/mod.rs`)**:
  - Centralized `GEMINI_BASE_URL`, `OPENROUTER_BASE_URL`, `PROJECT_REPO_URL`, `PROVIDER_STREAM_TIMEOUT_SECS`, `PROVIDER_REQUEST_TIMEOUT_SECS`, `MODELS_CACHE_FILE`, `SIGNAL_KILLED_EXIT_CODE`.
  - Replaced hardcoded provider URLs, timeouts, and magic numbers across the agent and tool subsystems.
- **Updated Tool Registry Documentation (`src/tools/mod.rs`)**:
  - Corrected stale tool count doc comment in `ToolRegistry::get_tool_schemas()`.

---

## [0.0.7] — 2026-08-15

### 🔒 Reliability & Security Hardening
- **Stream Retry State Reset (`src/agent/loop.rs`)**:
  - Automatically clears `iteration_text`, `pending_tool_calls`, and truncates `turn_response` upon stream errors before retrying, preventing output duplication and malformed tool calls.
- **Strict Landlock Error Propagation (`src/tools/exec.rs`)**:
  - Propagated `apply_landlock_sandbox` errors in `pre_exec` hook as `PermissionDenied` so command execution strictly aborts if kernel-level sandboxing fails.
- **Landlock Network Fallback Warning (`src/sandbox/landlock.rs`)**:
  - Emits visible `tracing::warn!` when running on Linux kernels lacking Landlock ABI V4 where TCP network restriction cannot be enforced.
- **Web Response Payload Limit (`src/tools/web.rs`, `src/constants.rs`)**:
  - Enforced `MAX_WEB_RESPONSE_BYTES` (10 MB) via `Content-Length` and byte buffering to prevent out-of-memory denial-of-service on massive web downloads.
- **Plan Archiving Integrity (`src/context/working_memory.rs`)**:
  - Propagated I/O errors during archive reading to guarantee active plans and progress files are never deleted if reading fails.

### ⚡ Performance & Caching Optimizations
- **Single-Pass AST File Content Caching (`src/context/graph.rs`)**:
  - Cached file contents in memory during Tree-sitter AST extraction to eliminate duplicate disk reads during dependency edge building.
- **$O(1)$ Test Coverage Deduplication (`src/context/graph.rs`)**:
  - Replaced $O(N^2)$ `Vec` lookups with `HashSet` in blast radius test coverage correlation.
- **HTTP `Retry-After` Header Parsing (`src/agent/provider.rs`)**:
  - Extracted and honored provider-specified `Retry-After` seconds on HTTP 429 rate limit responses.
- **TUI Ticker Drift Protection (`src/app.rs`)**:
  - Configured `MissedTickBehavior::Skip` on TUI 50ms interval ticker to avoid UI frame bursts.

### 🧹 Cleanliness, Centralization & Polish
- **Environment API Key Trimming (`src/config.rs`)**:
  - Trimmed surrounding whitespace and newlines from all `.env` and environment variable API keys.
- **Undo Directory Removal (`src/session/undo.rs`)**:
  - Added recursive `remove_dir_all` cleanup for directories created during rolled-back turns.
- **Constants Centralization (`src/constants.rs`, `src/agent/models.rs`, `src/context/index.rs`, `src/context/compressor.rs`, `src/mcp/server.rs`)**:
  - Consolidated model provider URLs, fetch timeouts, `BM25_PREFIX_WEIGHT`, `COMPRESSOR_MASK_LINES`, `DEFAULT_LOCATE_SYMBOL_LIMIT`, and `SUPPORTED_LANG_EXTENSIONS`.
- **Grep Skipped File Tracing (`src/tools/search.rs`)**:
  - Added `tracing::debug!` logging when unreadable files are skipped during search.

---

## [0.0.6] — 2026-08-15

### ⚡ Performance & Indexing Optimization
- **$O(V \times I)$ Graph Edge Construction (`src/context/graph.rs`)**:
  - Replaced $O(V \times S)$ quadratic full-corpus substring searches with a single-pass `HashSet` identifier lookup, avoiding massive CPU stalls on large repositories.
- **Robertson-Spärck Jones BM25 Scoring (`src/context/index.rs`)**:
  - Implemented the standard BM25 formula incorporating document frequency (IDF), document length normalization ($k_1 = 1.2, b = 0.75$), type definition boosts (+3.0 for struct/class/interface/trait/enum, +2.0 for functions), and test/mock file down-ranking (*0.5).
- **$O(\log N + K)$ Prefix Range Lookups (`src/context/index.rs`)**:
  - Replaced linear `HashMap` scan with `BTreeMap` range queries (`.range(prefix..prefix_end)`).
  - Preserved single-character domain variables/identifiers (e.g. `x`, `y`, `e`).
- **Zero-Allocation Observation Masking (`src/context/compressor.rs`)**:
  - Optimized `mask_observation` to stream head and tail lines with iterators (`.take()`, `.skip()`) without collecting the full output into a temporary `Vec<&str>`.

### 🛠️ Tool Registry & Protocol Alignment
- **Tool Suite Synchronization (`src/tools/mod.rs`, `src/mcp/server.rs`)**:
  - Synchronized `repo_map` into `ToolRegistry::get_tool_schemas()` and `dispatch_tool` (totaling 17 built-in LLM tools and 8 MCP protocol endpoints).
  - Added `parse_u64_param` helper across built-in and MCP tool dispatchers to robustly parse both JSON integers and stringified numbers.
- **JSON-RPC Protocol Hardening (`src/mcp/server.rs`)**:
  - Added strict object shape validation for `params` and `arguments` in `tools/call`.
- **Search ReDoS Guard (`src/tools/search.rs`)**:
  - Added `MAX_REGEX_QUERY_LEN` protection (1024 characters) against oversized regex patterns.

### 📦 Centralized Constants & Architecture Cleanliness
- **Constants Centralization (`src/constants.rs`, `src/sandbox/env.rs`, `src/session/`, `src/context/skills.rs`)**:
  - Consolidated Blast Radius risk thresholds, BM25 tuning parameters, directory names (`SESSIONS_DIR_NAME`, `BACKUPS_DIR_NAME`, `SKILL_MD_FILE`, `SKILLS_DIR_NAME`, `MCP_TOOL_PREFIX`), and sandbox environment sanitization arrays (`WHITELIST_ENV_VARS`, `SECRET_PATTERNS`, `BLOCKED_PREFIXES`).
  - Added `#[must_use]` on `build_system_prompt` (`src/agent/prompt.rs`).

---

## [0.0.5] — 2026-08-15

### 🧠 AST Code Intelligence & Symbol Extraction
- **Rich AST Signature & Doc Extraction (`src/context/repomap.rs`)**:
  - Upgraded `SymbolDef` to extract clean single-line signatures (e.g. `pub fn compute_sum(...) -> i32`, `class UserService:`, `export interface UserProfile`), line spans (`start_line`, `end_line`), and preceding documentation comments (`///`, `//!`, `#`).
  - Added Tree-sitter query extraction for Rust, Python, JavaScript, and TypeScript encompassing functions, structs, classes, interfaces, traits, enums, type aliases, and module imports.

### 🌐 Code Knowledge Graph & Blast Radius Analysis
- **Blast Radius & Impact Analysis (`src/context/graph.rs`)**:
  - Implemented `get_blast_radius` evaluating downstream ripple effects of modifying symbols or files across the codebase.
  - Multi-hop transitive dependency BFS traversal ($k=3$).
  - Automated test suite correlation (`tests/`, `*_test.rs`, `test_*`) identifying test coverage.
  - **Tarjan SCC Cycle Detection**: Utilized `petgraph::algo::tarjan_scc` to identify mutual recursive dependency cycles.
  - Formatted architectural risk ratings (`LOW`, `MEDIUM`, `HIGH`, `CRITICAL`) with Markdown report generation.

### ⚡ Sub-Millisecond Inverted Symbol Index
- **Inverted Symbol Index with BM25 Scoring (`src/context/index.rs`)**:
  - Subword tokenization supporting `camelCase`, `snake_case`, `SCREAMING_SNAKE_CASE`, and kebab notation.
  - Definition boosts (+30% for structs, classes, interfaces, traits) and automated penalty down-ranking for test/mock files.
  - Fast `locate_symbol` and `search_symbols` routines.

### 🛠️ New Agent & MCP Protocol Tools
- **Architectural Tools Integration (`src/tools/mod.rs`, `src/mcp/server.rs`)**:
  - Registered `impact_analysis` and `locate_symbol` in the built-in LLM tool registry.
  - Exposed `impact_analysis` and `locate_symbol` over the Model Context Protocol (MCP) `tools/list` and `tools/call`.

---

## [0.0.4] — 2026-08-15

### 🛡️ Security & Sandboxing
- **Landlock Async Isolation (`src/sandbox/landlock.rs`, `src/tools/exec.rs`)**:
  - Moved Landlock kernel rule enforcement (`ruleset.restrict_self()`) into Linux `std_cmd.pre_exec(...)` hooks after `fork()` so Tokio async worker threads are never restricted.
  - Added explicit read-write access to `/tmp` for build tools and test suites while restricting `/usr`, `/lib`, `/etc`, and `/bin` to read-only.
- **Environment Variable Sanitization (`src/sandbox/env.rs`)**:
  - Expanded secret stripping with 19 vendor blocked prefixes and sensitive patterns (`DATABASE_URL`, `SENTRY_DSN`, `KUBECONFIG`, `DOCKER_HOST`, `SSH_AUTH_SOCK`, `SIGNING`, `CERTIFICATE`).
- **Lexical Path Validation (`src/sandbox/path.rs`)**:
  - Added lexical path normalization resolving `.` and `..` before filesystem existence checks.

### ⚡ Critical Runtime Safety & LLM Protocol
- **UTF-8 Char Boundary Safety**:
  - Replaced unsafe byte slicing with `floor_char_boundary()` across output compactors (`src/tools/exec.rs`), web body truncation (`src/tools/web.rs`), system prompt truncation (`src/agent/prompt.rs`), API key masking (`src/ui/configure.rs`), and modal positioning (`src/ui/modal.rs`).
- **Tool Protocol Conformance (`src/agent/loop.rs`)**:
  - Fixed `Message::tool_result` to pass `tool_call.id` instead of `tool_call.name`.
- **Command Compactor Hardening (`src/tools/compactor.rs`)**:
  - Made `compact_cargo_check` and `compact_cargo_test` strictly preserve full error output whenever exit code is non-zero.
  - Made subcommand detection flag-aware (skipping flags like `+nightly`, `--quiet`, `-C`).
- **Process Safety (`src/tools/exec.rs`, `src/mcp/client.rs`)**:
  - Enabled `.kill_on_drop(true)` on `tokio::process::Command` to eliminate zombie child processes on timeout.
  - Added process group isolation (`process_group(0)`) on Unix for clean process tree teardowns.

### 📁 Filesystem & Patching Robustness
- **Empty File Read Support (`src/tools/fs.rs`)**:
  - Added explicit handling for 0-byte file reads without index out-of-bounds errors.
- **Atomic File Writes (`src/tools/fs.rs`, `src/context/memory.rs`)**:
  - All file writes now write to temporary sibling files (`.tmp_<pid>_<uuid>`) and atomically rename.
- **Sliding-Window Fuzzy Patch Matching (`src/tools/fs.rs`)**:
  - Replaced full-file diffing with sliding-window similarity scoring via `similar::TextDiff::ratio()`.
- **POSIX Trailing Newline Preservation (`src/tools/fs.rs`)**:
  - Preserved trailing newlines across all patch matching strategies.

### 🧠 AST Code Graph & Memory Engineering
- **Cross-File Dependency Graph (`src/context/graph.rs`)**:
  - Populated directed graph edges between modules based on extracted Tree-sitter symbol references.
- **Cross-Tier Memory Synchronization (`src/context/memory.rs`)**:
  - Synchronized and deduplicated preference keys across local and global memory stores.
- **Line-Anchored Task Plan Checkboxes (`src/context/working_memory.rs`)**:
  - Line-by-line matching for progress status updates without accidental substring replacements.
- **Float Standardization (`src/context/compressor.rs`)**:
  - Standardized threshold and margin math to `f64`.

### 🔄 Session Management & Multi-Turn Undo
- **Multi-Turn Consecutive `/undo` (`src/session/undo.rs`, `src/session/backup.rs`)**:
  - Canonicalized paths in backup checkpointing to seamlessly support relative paths.
  - Added automatic cleanup of rolled-back turn backup directories to allow consecutive undo operations.
- **Session Resume & Continue (`src/main.rs`, `src/app.rs`)**:
  - Wired CLI `--resume <session_id>` and `--continue` flags to hydrate previous session history into both interactive TUI and plain REPL modes.

### 🔌 Model Context Protocol (MCP) & UI Consistency
- **MCP Discovery & Compliance (`src/config.rs`, `src/mcp/server.rs`, `src/mcp/client.rs`)**:
  - Discovered both global (`~/.config/minicode/mcp.json`) and workspace (`.minicode/mcp.json`) MCP configurations.
  - Implemented standard JSON-RPC 2.0 error handling and response formatting.
- **UI Auto-Scroll Clamping (`src/ui/view.rs`)**:
  - Clamped manual and auto-scroll offsets to prevent viewport underflows and overflows.
- **Centralized Constants Documentation (`src/constants.rs`)**:
  - Documented all 45 constants with comprehensive rustdoc comments.

---

## [0.0.3] — 2026-08-14

### 🚀 Added
- **3D Retro-Futuristic Isometric Wordmark (`src/ui/view.rs`)**:
  - Implemented crisp 3D isometric block ASCII typography for the `minicode` hero intro banner in full Aura Theme colors.
  - Multi-tier color shading: Top highlights in Aura Purple (`#a277ff`), mid face in Pink (`#f694ff`), and base shadow in Mint Green (`#61ffca`).
- **Dynamic Git & Workspace Discovery (`src/ui/status.rs`, `src/ui/view.rs`)**:
  - Dynamic git branch detection returning `Option<String>` that gracefully displays active branch only when running inside an initialized git repository.
  - Dynamic `$HOME` path shortening (`~/...`) in both the welcome intro screen and bottom status bar.

### 🧹 Refactored & Polished
- **Zero-Hardcoding Enforcement**:
  - Replaced static version string literals with compile-time dynamic `env!("CARGO_PKG_VERSION")` and Clap's `#[command(version)]` attribute.
  - Modularized `TimelineContext` struct cleanly bundling theme, runtime timer, and workspace metadata without exceeding function argument limits.
  - Aligned repository attribution headers for OpenRouter API requests.

---

## [0.0.2] — 2026-08-14

### 🚀 Added
- **Dynamic Live Model Fetcher (`src/agent/models.rs`)**:
  - Automatically queries live provider API endpoints (OpenRouter `/api/v1/models`, Gemini `v1beta/models`, OpenAI-compatible `<base_url>/models`) to dynamically list all available models with zero hardcoding.
  - Automatically identifies free-tier models (`[FREE]`) and context window limits (`(Xk ctx)`).
  - Implemented local disk caching at `~/.config/minicode/models_cache.json` for instant UI loading and offline resilience.
- **In-TUI Interactive Modal System (`src/ui/modal.rs`)**:
  - Added floating Aura modal dialogs for switching providers and models mid-session (`/model`).
  - Added live fuzzy search filter in the model selector allowing developers to filter models as they type.
  - Full keyboard navigation with `↑`/`↓` arrows, `Enter` to select, and `Esc` to go back or close.
- **In-TUI Slash Command Recommendations & Arrow Navigation (`src/ui/input.rs`)**:
  - Real-time slash command recommendation popups (`/model`, `/provider`, `/undo`, `/clear`, `/help`, `/exit`) matching the reference UI design.
  - **`↑`/`↓` Arrow Navigation**: Interactive arrow key navigation across autocomplete recommendation rows with active `› ` indicator and elevated background styling.
  - **Instant Execution**: Pressing `Tab` or `Enter` immediately autocompletes or launches the highlighted slash command.
- **Enhanced Configuration Wizard (`src/ui/configure.rs`)**:
  - Hierarchical navigation with explicit back options (`[0] ◄ Back`, `b`, `Esc`) at every stage.
  - **Custom Provider Onboarding**: Support for adding custom OpenAI-compatible endpoints (vLLM, LM Studio, Ollama, LocalAI, etc.) with custom base URLs and live connection tests.
- **Safety Rollback Engine (`/undo`)**:
  - Seamless `/undo` command restoring previous turn backups directly inside the interactive TUI.

### 🛠️ Fixed
- Removed `tui_textarea` default cursor line underline from the input dock.
- Removed text underline from slash command recommendation highlights.
- Fixed UI event loop blocking during agent inference by introducing a non-blocking background Tokio actor task.

---

## [0.0.1] — 2026-08-14

### 🚀 Added
- **Dual-Mode Execution Architecture**:
  - **Interactive TUI Mode**: Full-screen Ratatui terminal application featuring a vertical streaming timeline, real-time token gauge, collapsible tool output folds, live execution timer, and multiline textarea input dock.
  - **Machine-Readable NDJSON IPC Mode (`--json-stream`)**: Bidirectional streaming over stdin/stdout for AI orchestrators, subagent swarms, and CI/CD pipelines.
  - **Accessible Plain REPL (`--plain`)**: Zero-alternate-screen scrolling terminal output.
- **Aura Theme Design System**:
  - Complete dark color palette inspired by Dalton Menezes' [Aura Theme](https://github.com/daltonmenezes/aura-theme) (`#15141b` dark background, `#29263c` elevated input container, `#a277ff` purple accent, `#61ffca` mint green success, `#ffca85` warm orange tool tags).
  - Clean, minimal inline timeline replacing cluttered multi-pane dashboards.
- **Interactive Configuration Wizard (`minicode configure`)**:
  - Step-by-step CLI wizard to select default providers, models, API keys, and execution approval policies.
  - Persistent settings stored in `~/.config/minicode/minicode.toml` and `.env`.
- **Universal Multi-Provider Engine**:
  - **OpenRouter Support**: Direct access to 100+ models (including Claude 3.5 Sonnet, DeepSeek-R1, Qwen2.5-Coder, Gemma, LFM) with free-tier compatibility and automatic exponential backoff retries on rate limits (HTTP 429).
  - **OpenAI & OpenAI-Compatible**: Support for OpenAI, DeepSeek, Groq, Together AI, and local vLLM/Ollama endpoints.
  - **Google Gemini**: Native reference provider with SSE streaming and `functionDeclarations`.
- **6 Core Coding Primitives**:
  - `read_file`: Line-ranged file inspection with boundary validation.
  - `write_file`: Atomic file writes with automatic parent directory creation.
  - `patch_file`: 3-stage search-and-replace block matching (exact match → whitespace normalization → fuzzy match via `similar`).
  - `exec_cmd`: Sandboxed shell execution with 30s timeout guard and 50KB output bounds.
  - `grep_search`: Fast regex codebase searching respecting `.gitignore` rules.
  - `fetch_or_browse`: HTML-to-Markdown scraper with Readability content extraction.
- **Security Sandbox Subsystem**:
  - Linux kernel Landlock filesystem isolation restricting file access strictly to the workspace root.
  - Landlock TCP network confinement by default for executed shell commands.
  - `env_clear()` environment sanitization with automatic API key/token stripping (`*_KEY`, `*_SECRET`, `*_TOKEN`).
  - Path canonicalization with symlink escape prevention.
- **AST Code Graph & Context Engine**:
  - Multi-language Tree-sitter AST queries for Rust, Python, JavaScript, and TypeScript with `mtime` caching.
  - Petgraph dependency graph builder with Personalized PageRank (biased toward active working files).
  - AST skeletonizer with token budget packing.
  - Exact BPE token counting via `tiktoken-rs` (cl100k_base / o200k_base).
  - Observation masking to truncate noisy tool outputs while preserving semantic head/tail context.
  - Dynamic `AGENTS.md` and workspace guideline discovery.
- **Session Persistence & Safety Rollback**:
  - Automatic turn-level file backup snapshots saved to `.minicode/backups/<turn_id>/` with JSON manifests.
  - `/undo` rollback engine restoring modified files and deleting created files.
  - Append-only JSONL session logging in `~/.config/minicode/sessions/`.
- **Low-Resource Developer Tooling**:
  - Developer `Justfile` with low-resource commands (`just check`, `just test`, `just clippy`, `just ci`, `just fmt`).
  - `.cargo/config.toml` hardware boundaries (`jobs = 2`, linker thread capping).
  - 23 unit tests verifying all core subsystems with 100% clean CI passes.

# Changelog 📜

All notable changes to **minicode** will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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

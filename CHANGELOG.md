# Changelog 📜

All notable changes to **minicode** will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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

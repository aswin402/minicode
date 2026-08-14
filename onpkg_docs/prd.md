# Product Requirements Document (PRD) — `minicode` ⚡

## 1. Executive Summary & Vision
`minicode` is an ultra-fast, lightweight, single-binary AI coding agent built from the ground up in **Rust**. It is designed with a **dual-audience architecture**:
1. **For Humans:** A distraction-free, minimalist terminal user interface (**TUI**) built on `ratatui` with an inline streaming timeline (inspired by OpenAI Codex CLI and Claude Code, avoiding bloated multi-widget dashboards).
2. **For AI Agents & Automation:** A headless, machine-readable streaming engine (**CLI / NDJSON-RPC over stdio**) that allows subagent swarms, IDE plugins, and CI/CD pipelines to invoke and orchestrate `minicode` programmatically.

`minicode` eliminates token waste, tool sprawl, and slow runtime startup times by restricting its tool surface to **6 high-precision coding primitives** and employing **AST Tree-Sitter Graph Engineering (Repo-Map)** and **Context Compaction**.

---

## 2. Target Personas & Primary Use Cases

| Persona / Use Case | Interaction Mode | Core Value Proposition |
| :--- | :--- | :--- |
| **Terminal Power Developer** | Interactive `ratatui` TUI | Sub-15ms startup, instant diff previews, prompt streaming, slash commands (`/diff`, `/skill`, `/compact`, `/undo`), zero dashboard clutter. |
| **AI Orchestrator (e.g. Antigravity, Claude Code, Swarms)** | Headless NDJSON (`--json-stream`) | Fast code execution sandbox, structured turn-by-turn events, bidirectional stdin/stdout IPC, deterministic tool outputs, and safe workspace confinement. |
| **CI/CD & Scripting Automation** | Headless CLI (`minicode run "refactor"`) | Non-interactive autonomous bug fixing, automated PR reviews, migration scripts, and documentation auditing. |

---

## 3. Non-Goals (What `minicode` is NOT)
- ❌ **No Heavy Dashboards:** No multi-panel bento boxes or complex analytics widgets that waste screen space and GPU/CPU cycles.
- ❌ **No Tool Bloat:** No sprawling collection of 30+ ad-hoc plugins. Everything routes through 6 minimal, highly tested primitives. (MCP extensibility planned for v2.)
- ❌ **No Cloud-Lock-in:** Seamlessly switch between Google Gemini, Anthropic Claude, OpenAI, and local Ollama/vLLM endpoints.

---

## 4. Core Functional Requirements

### 4.1. Dual-Audience Interface
- **Human Interactive Mode (`minicode`)**:
  - Full-screen alternate or inline streaming mode using `ratatui` + `crossterm`.
  - Syntax-highlighted Markdown responses and color-coded unified diff views.
  - Multi-line prompt editing with history navigation (`Up`/`Down`) and slash command autocompletion (`Tab`).
  - Real-time token usage gauge and live agent state spinner.
  - `--plain` / `--accessible` flag to bypass full-screen TUI for screen reader compatibility and CI/CD piping.
- **Agent Headless Mode (`minicode --json-stream` / `minicode run "<prompt>"`)**:
  - Emits newline-delimited JSON (NDJSON) events for every step (`turn_start`, `plan`, `tool_call`, `tool_result`, `stream_delta`, `turn_end`, `error`, `heartbeat`, `approval_request`).
  - Accepts structured JSON commands via `stdin` (user messages, tool approvals/rejections, abort signals).

### 4.2. Minimalist 6-Tool Runtime
To maximize LLM reasoning reliability and prevent tool hallucination, `minicode` is strictly limited to 6 core tools:
1. `read_file(path, start_line?, end_line?)`: High-performance random access file reading with line numbers.
2. `patch_file(path, search_block, replace_block)`: **Search-and-replace block patching** (inspired by Aider). The LLM provides the exact code block to find and its replacement. Fallback: whitespace-insensitive fuzzy matching via `similar` crate with human confirmation on low-confidence matches. (Unified diffs are NOT used — LLMs hallucinate line numbers and context lines.)
3. `write_file(path, content)`: Atomic file creation or full overwrite.
4. `exec_cmd(command, timeout_ms?)`: Sandboxed shell execution with environment sanitization, live output streaming, and timeout enforcement.
5. `grep_search(query, regex?, file_pattern?)`: Ripgrep-powered in-memory code search respecting `.gitignore`.
6. `fetch_or_browse(url, mode)`: Dual-mode web reader (fast Readability HTML-to-Markdown distillation for docs, plus optional headless Chrome automation via `chromiumoxide` for web apps).

### 4.3. AST Graph Engineering & Repo-Mapping Engine (Inspired by Aider)
- **Tree-sitter AST Extraction:** Parses Rust, Python, JavaScript, TypeScript using language-specific `.scm` query files to extract function signatures, type definitions, exports, and imports.
- **Petgraph Code Dependency Graph:** Constructs a directed graph where nodes are files/symbols and edges are import/call dependencies.
- **Personalized PageRank Ranking:** Runs PageRank with personalization biased toward files in the active conversation context, ensuring domain-relevant symbols rank higher than generic utilities.
- **AST Skeletonizer with Token Budget:** Uses binary-search packing (like Aider's `--map-tokens 1024`) to compress the top-ranked symbols into a strict token budget (~1,024 tokens for a 500-file repo).
- **Incremental Cache:** Only re-parses files whose `mtime` has changed since the last index, avoiding full workspace re-scans.

### 4.4. Context Engineering & Dynamic Compression
- **Token Counting:** Uses `tiktoken-rs` for OpenAI-compatible token counting with `cl100k_base` as the default tokenizer. For non-OpenAI providers, applies a 15% safety margin on estimated counts.
- **Hybrid Compaction Strategy** (inspired by Claude Code `/compact` + Aider pruning):
  1. **Observation Masking:** Truncates noisy tool outputs (e.g. `cargo build` logs or long `grep` dumps) to the first 15 and last 15 lines with an on-disk scratch file reference.
  2. **Importance-Weighted Pruning:** Replaces dropped file contents with the compressed Repo-Map skeleton.
  3. **LLM-Anchored Summarization:** When context exceeds 70% of the model's window, older turns are summarized into a compact "working state" (intent, decisions, progress) while preserving system prompt, AGENTS.md rules, and recent 3 turns verbatim.
- **Dynamic Skills & Instruction Loader:** Discovers and parses `SKILL.md`, `AGENTS.md`, and `.skills/` files, injecting relevant skill workflows into prompt context on-demand.

### 4.5. Multi-Provider LLM Adapter
- Supported Providers: **Google Gemini API**, **Anthropic Claude API**, **OpenAI API**, **OpenAI-Compatible (DeepSeek, Groq, Together)**, and **Ollama / Local LLM**.
- **Tool Schema Translation Layer:** Internal tool schemas (standard JSON Schema) are automatically translated to provider-specific formats at runtime:
  - OpenAI: `{"type": "function", "function": {"name": "...", "parameters": {...}}}`
  - Anthropic: `{"name": "...", "input_schema": {...}}`
  - Gemini: `{"functionDeclarations": [{"name": "...", "parameters": {...}}]}`
  - Ollama (no native tool calling): XML-in-prompt fallback with client-side regex extraction.
- Unified SSE streaming parser with automatic exponential backoff retry for rate limits (HTTP 429).

---

## 5. Undo, Rollback & Session Management

### 5.1. File Safety Checkpoints (Inspired by Claude Code `/rewind` & Aider Git Commits)
Before any tool call that modifies the filesystem (`patch_file`, `write_file`, `exec_cmd`), `minicode` automatically:
1. Copies the original file to `.minicode/backups/<turn_id>/<relative_path>`.
2. Records the modification in a turn-level manifest (`.minicode/backups/<turn_id>/manifest.json`).
- **`/undo` command:** Reverts all file changes from the last turn by restoring from backup copies and truncating conversation history.
- **`/undo <turn_id>` command:** Reverts to a specific checkpoint.
- Backups are pruned after 50 turns to prevent disk bloat.

### 5.2. Session Persistence & Resume (Inspired by Codex CLI Threads)
- **Storage Format:** JSONL (JSON Lines) — append-only, crash-safe, line-by-line parseable.
- **Location:** `~/.config/minicode/sessions/<session_id>.jsonl`
- **Resume:** `minicode --resume <session_id>` or `minicode --continue` (resumes last session).
- Each JSONL line records: user messages, assistant responses, tool calls + results, token counts, and file modification manifests.

---

## 6. Security, Sandboxing & Safety

### 6.1. Path Confinement & Symlink Protection
- All file paths are resolved via `std::fs::canonicalize()` before any read/write operation.
- The canonical path MUST start with the canonical workspace root. This prevents both `../` traversal and symlink-based escapes.
- For files that don't exist yet (new file creation), use lexical normalization + parent directory canonicalization.

### 6.2. Environment Variable Sanitization (Inspired by Codex CLI `shell_environment_policy`)
`exec_cmd` **never** inherits the parent's full environment. Instead:
1. Clear all inherited env vars via `Command::env_clear()`.
2. Selectively whitelist safe variables: `PATH`, `HOME`, `USER`, `LANG`, `LC_ALL`, `TERM`, `SHELL`, `EDITOR`, `TMPDIR`.
3. Explicitly strip any variable matching patterns: `*_KEY`, `*_SECRET`, `*_TOKEN`, `*_PASSWORD`, `*_CREDENTIAL`.
4. Inject workspace-specific variables: `MINICODE_WORKSPACE`, `MINICODE_SESSION_ID`.

### 6.3. Permission Guardrails
- `Strict Mode` (Default for Humans): Prompt for confirmation on destructive file writes and shell execution.
- `Auto-Approve Mode` (`--yes` / `--dangerously-skip-permissions`): For trusted agent-to-agent automation.

### 6.4. OS-Level Sandboxing (Inspired by Codex CLI)
- **Linux:** Uses the `landlock` crate to restrict child processes:
  - Filesystem: Read/write confined to workspace root only.
  - Network: TCP bind/connect denied by default (blocks data exfiltration via `curl`).
  - Restrictions are inherited by all child processes automatically.
- **macOS:** Seatbelt profile support (planned for v2).
- **Fallback:** If Landlock is unavailable (older kernels), use working-directory confinement + timeout guards.

### 6.5. Command Timeouts
- Strict 30-second default execution timeout on shell commands to prevent hung processes.
- Configurable via `--timeout <seconds>` or `config.toml`.

---

## 7. Configuration System

### 7.1. Config File Format (TOML)
- **Location hierarchy** (highest priority first):
  1. CLI flags (`--model`, `--provider`, etc.)
  2. Environment variables (`MINICODE_MODEL`, `MINICODE_PROVIDER`)
  3. Project-local: `.minicode/config.toml` (in workspace root)
  4. Global: `~/.config/minicode/config.toml`

### 7.2. API Key Management
- API keys are loaded from environment variables ONLY (`GEMINI_API_KEY`, `ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, `OLLAMA_HOST`).
- Keys are NEVER stored in config files or committed to version control.
- `.env` file support in workspace root (loaded at startup).

---

## 8. Success & Performance Metrics (SLAs)

| Metric | Target SLA |
| :--- | :--- |
| **Binary Startup Time** | < 15 ms (instant response on CLI invocation) |
| **Idle Memory Footprint** | < 50 MB RAM (with lazy-loaded syntax themes and Tree-sitter parsers) |
| **AST Repo Indexing Speed** | < 250 ms for 500 files (~100k LoC) |
| **Streaming First-Token Latency** | < 400 ms (provider dependent) |
| **Token Efficiency** | ≥ 40% reduction in token consumption compared to naive full-file agents |

---

## 9. Future Scope (v2 Roadmap)
- **MCP (Model Context Protocol):** Extensible tool registration beyond the core 6 primitives.
- **LSP Integration:** Language Server Protocol for richer code intelligence (hover, go-to-definition, diagnostics).
- **macOS Seatbelt Sandboxing:** Full Seatbelt profile support for macOS shell execution.
- **Git-Aware Operations:** Read uncommitted diffs, auto-stage files, create commits and PRs.
- **Multi-Agent Orchestration:** Built-in subagent spawning and task delegation.

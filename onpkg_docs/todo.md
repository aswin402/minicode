# Task Tracker (todo.md) — `minicode` 📋

## Phase 1: CLI Scaffold, Config & First Provider
- [x] Scaffold project foundation via `onpkg` stack `rust-cli`
- [x] Configure pure-Rust `Cargo.toml` with `rustls-tls-webpki-roots`
- [x] Set up workspace isolation and low-resource limits in `Cargo.toml` & `.cargo/config.toml` (`jobs = 2`)
- [x] Create developer `Justfile` with low-resource commands (`just check`, `just test`, `just clippy`, `just ci`, `just fmt`)
- [x] Author comprehensive PRD with undo/rollback/security sections
- [x] Author UI/Terminal Design Spec with accessibility + NDJSON protocol
- [x] Author Technical Implementation Plan with phased roadmap
- [x] Author CLI & Protocol Reference with bidirectional stdin/stdout IPC
- [x] Synchronize `onpkg.json` and `AGENTS.md`
- [x] Set up `main.rs` with `clap` CLI dispatch (`minicode`, `minicode configure`, `minicode run`, `--json-stream`)
- [x] Implement `config.rs` (TOML loader + `.env` support + env var overrides)
- [x] Define shared error types using `thiserror` for each module (`src/error.rs`)
- [x] Define agent types (`Message`, `ToolCall`, `ToolResult`, `AgentEvent`, `StdinCommand`)
- [x] Implement `Provider` trait with SSE streaming
- [x] Implement Gemini API provider (reference implementation with function declarations)
- [x] Set up `tracing` + `tracing-appender` file-based logging
- [x] Implement basic headless NDJSON stdout emitter for testing

## Phase 2: Tool Trait, 6 Tools & Security Sandbox
- [x] Define `ToolSchema` and `ToolRegistry` dispatcher (`src/tools/mod.rs`)
- [x] Implement `read_file` with line ranges and bounds checking (`src/tools/fs.rs`)
- [x] Implement `write_file` with atomic write + directory creation (`src/tools/fs.rs`)
- [x] Implement `patch_file` with search-and-replace block matching (`src/tools/fs.rs`)
  - [x] Exact string match (primary)
  - [x] Whitespace-insensitive match (fallback 1)
  - [x] Fuzzy match via `similar` crate with confidence threshold (fallback 2)
- [x] Implement `exec_cmd` with sandboxing (`src/tools/exec.rs`)
  - [x] `env_clear()` + whitelist-based env var sanitization (`src/sandbox/env.rs`)
  - [x] Landlock filesystem confinement (workspace root only) (`src/sandbox/landlock.rs`)
  - [x] Landlock network restriction (TCP deny by default)
  - [x] 30s timeout guard with process kill
- [x] Implement `grep_search` using `ignore` + `regex` (`src/tools/search.rs`)
- [x] Implement `fetch_or_browse` with HTML-to-Markdown extraction (`src/tools/web.rs`)
- [x] Implement path canonicalization + symlink protection (`src/sandbox/path.rs`)
- [x] Implement file safety checkpoint system (`src/session/backup.rs`)
  - [x] Auto-copy originals to `.minicode/backups/<turn_id>/`
  - [x] Turn-level manifest for rollback
- [x] Implement `/undo` rollback engine (`src/session/undo.rs`)
- [x] Implement JSONL session persistence store (`src/session/store.rs`)
- [x] Wire tools into headless NDJSON runner for end-to-end testing

## Phase 3: ReAct Agent Loop & Multi-Provider Support
- [x] Build ReAct state machine (`src/agent/loop.rs`)
  - [x] Turn lifecycle: prompt → stream → tool_call → result → continue/end
  - [x] Multi-step tool call loop with auto-continuation
  - [x] Real-time event propagation via MPSC channel
  - [x] Automatic retry with exponential backoff on HTTP 429 rate limits
- [x] Build tool schema adapter layer for Gemini (`functionDeclarations`)
- [x] Build tool schema adapter layer for OpenAI & OpenRouter (`tools: [...]`)
- [x] Implement OpenRouter provider with support for 100+ models & free tier
- [x] Implement OpenAI / OpenAI-compatible provider (DeepSeek, Groq, Together, Ollama)
- [x] Build system prompt assembler with dynamic workspace injection (`src/agent/prompt.rs`)
- [x] Implement stdin JSON command parser (user_input, tool_response, abort, configure)

## Phase 4: AST Graph Engineering & Context Compactor
- [x] Multi-language Tree-sitter AST queries (`src/context/repomap.rs`)
  - [x] Rust: functions, structs, enums, traits, impls
  - [x] Python: classes, functions
  - [x] JavaScript: functions, classes
  - [x] TypeScript: functions, classes, interfaces, types
- [x] Build Petgraph dependency graph from AST edges (`src/context/graph.rs`)
- [x] Implement Personalized PageRank with active-file bias
- [x] Implement AST Skeletonizer with token budget packing
- [x] Implement incremental mtime-based cache for parsed ASTs
- [x] Implement `tiktoken-rs` token counter integration (`src/context/compressor.rs`)
- [x] Implement ContextCompressor
  - [x] Observation masking (head/tail truncation for tool outputs > 30 lines)
  - [x] Context history compaction
- [x] Implement Skills & AGENTS.md dynamic loader (`src/context/skills.rs`)

## Phase 5: Stream-Centric Ratatui TUI & Aura Theme
- [x] Terminal raw mode + crossterm event loop + signal handling (`src/app.rs`)
- [x] Official Aura Theme implementation inspired by Dalton Menezes (`src/ui/theme.rs`)
  - [x] Aura Dark (`#15141b`), Aura Soft Dark (`#121016`), and ANSI-256 palettes
  - [x] Aura Purple (`#a277ff`), Mint Green (`#61ffca`), Warm Orange (`#ffca85`), Pink (`#f694ff`)
- [x] Non-blocking background Agent actor with smooth 20 FPS redraws (`src/app.rs`)
- [x] Streaming conversation timeline with Markdown + tool execution folds (`src/ui/view.rs`)
  - [x] `• Running <cmd>` / `✔ You approved minicode to run <cmd> this time`
  - [x] `• Ran <cmd>` with indented `└ ...` output details
  - [x] Live execution timer `• Working (Xs • esc to interrupt)`
- [x] Elevated input dock with `› ` chevron and multiline support (`src/ui/input.rs`)
  - [x] `Enter` / `Shift+Enter` prompt submission
  - [x] `Ctrl+J` / `Alt+Enter` newline insertion
- [x] Minimal status bar widget (`<model> · <path> · <git_branch> [default]`) (`src/ui/status.rs`)
- [x] Interactive configuration menu wizard (`src/ui/configure.rs` / `minicode configure`)
- [x] `--plain` / `--accessible` mode: bypass ratatui, scrolling REPL output

## Phase 6: Session Persistence, Polish & Release
- [x] Implement JSONL session writer/reader (`src/session/store.rs`)
- [x] Implement `--resume <id>` and `--continue` CLI flags
- [x] Unit tests for all modules (23 tests passing in 1.05s)
- [x] Set up low-resource CI pipeline in `Justfile` (`just ci`)
- [x] Zero-warning compilation with `cargo clippy -- -D warnings`
- [x] Authored comprehensive `CHANGELOG.md` for `v0.0.1` release
- [x] Authored production-grade `README.md` with official Aura logo artifact

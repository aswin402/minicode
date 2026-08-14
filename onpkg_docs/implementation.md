# Technical Implementation Plan — `minicode` 🛠️

## 1. Technology Stack & Crate Ecosystem

| Component | Library / Crate | Version | Key Purpose |
| :--- | :--- | :--- | :--- |
| **Language & Runtime** | Rust 2021 Edition + Tokio | `1.40` (full) | Zero-overhead, memory-safe, multi-threaded async actor model. |
| **CLI & Config** | `clap` (derive, env), `serde`, `toml` | `4.5` / `1.0` | Sub-command parsing, env override, JSON-RPC streaming. |
| **TUI Interface** | `ratatui` + `crossterm` + `tui-textarea`| `0.29` / `0.28` | Stream-centric minimalist terminal, 60+ FPS rendering, keyboard event streams. |
| **AST & Code Graph** | `tree-sitter` + multi-lang grammars + `petgraph` | `0.23` (aligned) | Extract symbol ASTs, construct dependency graphs, PageRank ranking. |
| **Code Search** | `ignore` + `grep-regex` + `grep-searcher` | `0.4` / `0.1` | Embedded Ripgrep speed respecting `.gitignore`. |
| **Networking & LLM** | `reqwest` (`rustls-tls-webpki-roots`) + `reqwest-eventsource` | `0.12` | Pure Rust TLS with trusted root certs, high-throughput SSE streaming. |
| **Token Counting** | `tiktoken-rs` | `0.6` | Exact BPE token counting for context budget enforcement. |
| **Diffs & Highlighting**| `similar` + `syntect` + `pulldown-cmark` | `2.6` / `5.2` | Syntax-colored code blocks, color-coded diffs, Markdown rendering. |
| **Web & Browser** | `scraper` + `chromiumoxide` (feature-gated) | `0.20` / `0.7` | HTML-to-Markdown distillation + headless Chrome automation. |
| **Logging** | `tracing` + `tracing-subscriber` + `tracing-appender` | `0.3` | File-based logging (avoids corrupting ratatui alt-screen). |
| **Sandbox** | `landlock` | `0.4` | Linux filesystem + network restriction for child processes. |
| **Theme Detection** | `terminal-colorsaurus` | `0.4` | Safe OSC 10/11 light/dark background detection without stdin corruption. |

### Crate Corrections from Review
- **Removed `walkdir`**: Redundant — `ignore::WalkBuilder` provides the same functionality + `.gitignore` support.
- **Removed `eventsource-stream`**: Replaced with `reqwest-eventsource` for tighter `reqwest` integration.
- **Added `tiktoken-rs`**: Required for context compression token budgeting.
- **Added `tracing-appender`**: File-based log writer that doesn't corrupt ratatui's alternate screen.
- **Added `landlock`**: Linux OS-level sandbox for child process confinement.
- **Added `terminal-colorsaurus`**: Safe terminal theme detection.
- **Fixed `reqwest` features**: Changed from `rustls-tls` to `rustls-tls-webpki-roots` for proper root cert loading.
- **Aligned tree-sitter versions**: ALL tree-sitter crates must use the same ABI version to prevent segfaults.

---

## 2. Modular Architecture & Source Layout

```text
minicode/
├── Cargo.toml
├── src/
│   ├── main.rs                   # CLI dispatch: TUI vs Headless vs Run command
│   ├── app.rs                    # Terminal raw mode setup, event pump, signal handling
│   ├── config.rs                 # Config loader (~/.config/minicode/config.toml + .env + env vars)
│   │
│   ├── agent/                    # Core Agent Logic
│   │   ├── mod.rs                # Public agent interfaces
│   │   ├── loop.rs               # ReAct State Machine (Turn -> Stream -> Tool -> Result)
│   │   ├── provider.rs           # LLM Providers (Gemini, Claude, OpenAI, Ollama) + Tool Schema Adapter
│   │   ├── prompt.rs             # System prompt generator + dynamic context injection
│   │   └── types.rs              # Message, ToolCall, ToolResult, Turn, AgentEvent enums
│   │
│   ├── context/                  # Context & Graph Engineering
│   │   ├── mod.rs
│   │   ├── repomap.rs            # Tree-sitter AST signature extraction with .scm queries
│   │   ├── graph.rs              # Petgraph code dependency graph & Personalized PageRank
│   │   ├── compressor.rs         # Token counter (tiktoken-rs) + observation masker + history summarizer
│   │   └── skills.rs             # SKILL.md & AGENTS.md dynamic loader
│   │
│   ├── tools/                    # Minimal 6-Tool Runtime
│   │   ├── mod.rs                # Tool trait definition, registry & dispatch
│   │   ├── fs.rs                 # read_file, write_file, patch_file (search-and-replace)
│   │   ├── exec.rs               # Sandboxed shell: env sanitization, Landlock, timeout guard
│   │   ├── search.rs             # Embedded ripgrep search engine
│   │   ├── web.rs                # Doc fetcher with readability markdown converter
│   │   └── browser.rs            # Headless browser (ChromiumOxide) controller
│   │
│   ├── session/                  # Session Persistence & Undo
│   │   ├── mod.rs
│   │   ├── store.rs              # JSONL session writer/reader
│   │   ├── backup.rs             # File safety checkpoints (.minicode/backups/)
│   │   └── undo.rs               # Turn-level rollback engine
│   │
│   ├── sandbox/                  # Security & Isolation
│   │   ├── mod.rs
│   │   ├── path.rs               # Canonicalize + workspace root enforcement + symlink protection
│   │   ├── env.rs                # Environment variable sanitization (whitelist policy)
│   │   └── landlock.rs           # Linux Landlock filesystem + network restriction
│   │
│   └── ui/                       # Minimalist Ratatui TUI
│       ├── mod.rs                # TUI entry and render loop
│       ├── view.rs               # Conversation timeline & Markdown renderer
│       ├── input.rs              # Textarea editor with slash command autocompletion
│       ├── confirm.rs            # Confirmation dialog overlay for destructive actions
│       ├── status.rs             # Header bar + token gauge + footer keybinds
│       └── theme.rs              # Adaptive theme (dark/light, TrueColor/256/16 fallback)
```

---

## 3. Core Engine Mechanics

### 3.1. ReAct Agent Event Loop (`src/agent/loop.rs`)
```rust
pub enum AgentEvent {
    TurnStart { turn_id: usize, model: String, context_tokens: usize },
    DeltaText { text: String },
    ToolCallStart { tool_id: String, tool: String, args: serde_json::Value },
    ToolCallResult { tool_id: String, tool: String, success: bool, output: String, duration_ms: u64 },
    ApprovalRequest { tool_id: String, tool: String, args: serde_json::Value },
    FileModified { path: String, action: String, backup_path: String },
    TurnEnd { turn_id: usize, tokens_used: usize, files_modified: Vec<String> },
    Error { code: String, message: String, retrying: bool },
    Heartbeat { status: String },
}
```

**Loop mechanics:**
1. **User Prompt Received:** Injected alongside the compressed Repo Map (AST symbols), active Skills, and recent conversation history.
2. **Token Budget Check:** `tiktoken-rs` counts the full prompt. If over 70% of model window, trigger `ContextCompressor`.
3. **Streaming Turn:** Provider streams deltas to the UI/NDJSON channel.
4. **Tool Invocation:** If the model outputs a structured tool call:
   a. Check if the tool requires approval (destructive actions in strict mode).
   b. Create file safety checkpoint (backup originals).
   c. Execute the sandboxed tool asynchronously via `tokio::spawn`.
   d. Record `tool_id` to map results back (supports parallel tool calls).
   e. Feed `ToolResult` back into the context.
5. **Token Compaction:** Before each subsequent turn, the `ContextCompressor` evaluates the budget and prunes as needed.

### 3.2. Tool Schema Translation (`src/agent/provider.rs`)
```rust
/// Internal tool schema — provider-agnostic
pub struct ToolSchema {
    pub name: &'static str,
    pub description: &'static str,
    pub parameters: serde_json::Value, // Standard JSON Schema
}

/// Adapts internal schema to provider-specific format
pub trait ToolSchemaAdapter {
    fn format_tools(&self, tools: &[ToolSchema]) -> serde_json::Value;
    fn parse_tool_call(&self, response: &serde_json::Value) -> Option<Vec<ToolCall>>;
}

// Implementations: OpenAiAdapter, AnthropicAdapter, GeminiAdapter, OllamaXmlAdapter
```

For Ollama models without native tool calling, `OllamaXmlAdapter` injects XML instructions into the system prompt and parses tool calls via regex: `<tool_call><name>...</name><args>...</args></tool_call>`.

### 3.3. Search-and-Replace File Patching (`src/tools/fs.rs`)
Inspired by Aider's edit format. The LLM outputs:
```
<<<<<<< SEARCH
pub fn old_function() -> i32 {
    42
}
=======
pub fn new_function() -> i64 {
    84
}
>>>>>>> REPLACE
```

**Matching cascade:**
1. **Exact match**: Direct string comparison in file contents.
2. **Whitespace-insensitive match**: Normalize whitespace before comparison.
3. **Fuzzy match** (via `similar` crate): If exact fails, find the closest block with ≥85% similarity score. Apply only with human confirmation.

### 3.4. AST Code Graph & Personalized PageRank (`src/context/repomap.rs`)
**Tree-sitter `.scm` query examples:**
```scheme
;; Rust function signatures
((function_item name: (identifier) @name) @definition.function)

;; Rust struct definitions
((struct_item name: (type_identifier) @name) @definition.class)

;; Python class definitions
((class_definition name: (identifier) @name) @definition.class)

;; TypeScript import references
((import_statement (import_clause (named_imports (import_specifier name: (identifier) @name)))) @reference)
```

**Personalized PageRank algorithm:**
1. Build directed graph: nodes = files, edges = import/call dependencies.
2. Set personalization vector: bias toward files currently in the active chat context.
3. Run PageRank iterations (damping factor = 0.85, convergence threshold = 1e-6).
4. Binary-search pack top-ranked symbols into the token budget (default: 1,024 tokens).
5. Cache results keyed by file `mtime` — only re-parse changed files.

### 3.5. Context Compression Pipeline (`src/context/compressor.rs`)
```rust
pub struct ContextCompressor {
    tokenizer: tiktoken_rs::CoreBPE,
    max_tokens: usize,        // Model's context window
    warning_threshold: f32,   // 0.70 — trigger compaction at 70%
    safety_margin: f32,       // 0.15 — for non-tiktoken providers
}
```

**Compression strategy (ordered by priority):**
1. **Observation Masking:** Tool outputs > 30 lines → keep first 15 + last 15 + `[... N lines truncated, saved to .minicode/scratch/<hash>]`.
2. **Repo-Map Refresh:** Drop raw file contents from old turns, replace with refreshed PageRank Repo-Map skeleton.
3. **History Summarization:** Turns older than 3 → LLM-free extraction of key decisions/changes as bullet points (no API call needed — rule-based extraction of tool names, file paths, and status).
4. **Emergency Truncation:** If still over budget, drop oldest turns entirely (preserving system prompt + AGENTS.md + most recent 2 turns).

---

## 4. Phased Implementation Roadmap (Reordered for Testability)

### Phase 1: CLI Scaffold, Config & First Provider
- [ ] Set up `main.rs` with `clap` CLI dispatch (TUI / headless / run subcommands)
- [ ] Implement `config.rs` (TOML loader, .env support, env var overrides)
- [ ] Define agent types (`src/agent/types.rs`: Message, ToolCall, AgentEvent)
- [ ] Define error types using `thiserror` for each module
- [ ] Implement `Provider` trait + ONE reference: Google Gemini SSE streaming
- [ ] Set up `tracing` + `tracing-appender` (file-based logging)
- [ ] Implement basic headless NDJSON emitter for testing

### Phase 2: Tool Trait, 6 Tools & Headless Runner
- [ ] Define `Tool` trait and `ToolRegistry` dispatcher (`src/tools/mod.rs`)
- [ ] Implement `read_file`, `write_file` with path canonicalization
- [ ] Implement `patch_file` with search-and-replace + fuzzy fallback
- [ ] Implement `exec_cmd` with env sanitization, Landlock sandbox, timeout
- [ ] Implement `grep_search` using `ignore` + `grep-regex`
- [ ] Implement `fetch_or_browse` (doc fetcher + optional chromiumoxide)
- [ ] Build sandbox module (`src/sandbox/`: path.rs, env.rs, landlock.rs)
- [ ] Build session/backup module (`src/session/`: store.rs, backup.rs, undo.rs)
- [ ] Wire tools into headless NDJSON runner for end-to-end testing

### Phase 3: ReAct Agent Loop & Multi-Provider
- [ ] Build ReAct state machine (`src/agent/loop.rs`)
- [ ] Build tool schema adapter layer (OpenAI, Anthropic, Gemini, Ollama XML)
- [ ] Implement system prompt assembler with dynamic skill injection
- [ ] Add remaining providers (Anthropic Claude, OpenAI, Ollama)
- [ ] Implement stdin JSON command parser for headless bidirectional IPC
- [ ] Add retry logic with exponential backoff for rate limits

### Phase 4: AST Graph Engineering & Context Compactor
- [ ] Integrate Tree-sitter parsers (Rust, Python, JS, TS) with `.scm` query files
- [ ] Build Petgraph dependency graph + Personalized PageRank
- [ ] Implement AST Skeletonizer with binary-search token budget packing
- [ ] Implement incremental cache (mtime-based re-parsing)
- [ ] Build ContextCompressor with tiktoken-rs token counting
- [ ] Implement observation masking + history summarization

### Phase 5: Ratatui TUI & Interactive Experience
- [ ] Terminal raw mode + crossterm event loop (`src/app.rs`)
- [ ] Adaptive theme engine with terminal-colorsaurus detection (`src/ui/theme.rs`)
- [ ] Streaming conversation timeline with Markdown + syntax highlighting (`src/ui/view.rs`)
- [ ] Confirmation dialog overlay for destructive actions (`src/ui/confirm.rs`)
- [ ] tui-textarea input dock with Ctrl+J newline + slash commands (`src/ui/input.rs`)
- [ ] Header + token gauge + footer keybinds (`src/ui/status.rs`)
- [ ] Terminal resize handling + narrow terminal fallback
- [ ] `--plain` / `--accessible` mode (bypasses ratatui, scrolling REPL output)

### Phase 6: Polish, Testing & Release
- [ ] Add remaining slash commands (`/undo`, `/retry`, `/save`, `/load`, `/map`, `/help`)
- [ ] Implement JSONL session persistence + resume (`--resume`, `--continue`)
- [ ] Write unit tests for: compressor, patch_file fuzzy matching, path canonicalization, env sanitization
- [ ] Write integration tests with mock provider (deterministic tool call sequences)
- [ ] Set up CI: `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`
- [ ] Benchmark startup time (< 15ms) and memory footprint (< 50MB)
- [ ] Finalize README, AGENTS.md, and onpkg_docs

# Technical Implementation Plan — `minicode` 🛠️

## 1. Technology Stack & Crate Ecosystem

| Component | Library / Crate | Version | Key Purpose |
| :--- | :--- | :--- | :--- |
| **Language & Runtime** | Rust 2021 Edition + Tokio | `1.40` (full) | Zero-overhead, memory-safe, multi-threaded async actor model. |
| **CLI & Config** | `clap` (derive, env), `serde`, `toml` | `4.5` / `1.0` | Sub-command parsing, env override, JSON-RPC streaming. |
| **TUI Interface** | `ratatui` + `crossterm` + `tui-textarea`| `0.29` / `0.28` | Stream-centric minimalist terminal, 60+ FPS rendering, keyboard event streams. |
| **AST & Code Graph** | `tree-sitter` + multi-lang grammars + `petgraph` | `0.23` (aligned) | Extract symbol ASTs, construct dependency graphs, PageRank ranking. |
| **Code Search** | `ignore` + `regex` | `0.4` / `1.11` | Parallel gitignore-aware regex search engine. |
| **Networking & LLM** | `reqwest` (`rustls-tls-webpki-roots`) + `reqwest-eventsource` | `0.12` | Pure Rust TLS with trusted root certs, high-throughput SSE streaming. |
| **Token Counting** | `tiktoken-rs` | `0.6` | Exact BPE token counting for context budget enforcement. |
| **Diffs & Highlighting**| `similar` + `syntect` + `pulldown-cmark` | `2.6` / `5.2` | Syntax-colored code blocks, color-coded diffs, Markdown rendering. |
| **Web & Browser** | `scraper` + `tokio-tungstenite` (CDP) | `0.20` / `0.24` | HTML-to-Markdown distillation + headless CDP browser automation. |
| **Logging** | `tracing` + `tracing-subscriber` + `tracing-appender` | `0.3` | File-based logging (avoids corrupting ratatui alt-screen). |
| **Sandbox** | `landlock` | `0.4` | Linux filesystem + network restriction for child processes. |
| **Theme Detection** | `crossterm` + `COLORFGBG` | `0.28` | Safe terminal light/dark background detection without stdin corruption. |

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
│   ├── tools/                    # 214-Tool Runtime (8 Core + Dynamic Categories)
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
- [x] Add remaining slash commands (`/undo`, `/retry`, `/save`, `/load`, `/map`, `/help`)
- [x] Implement JSONL session persistence + resume (`--resume`, `--continue`)
- [x] Write unit tests for: compressor, patch_file fuzzy matching, path canonicalization, env sanitization
- [x] Write integration tests with mock provider (deterministic tool call sequences)
- [x] Set up CI: `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`
- [x] Benchmark startup time (< 15ms) and memory footprint (< 50MB)
- [x] Finalize README, AGENTS.md, and minikit_docs

---

## 5. `oh-my-opencode` & Autonomous Multi-Agent Architecture Overhaul (`v0.5.4` – `v0.5.7`)

### Design Invariants
- **Zero New Slash Commands**: All capabilities operate autonomously via state machines, lifecycle hooks, and declarative tool schemas without requiring manual `/commands`.
- **Zero Hardcoding / Zero Unwanted `if-else` or Regex Sprawl**: Use declarative static metadata tables (`const` struct slices), state machine transitions, morphological token sets, and deterministic hashing (`FNV-1a` / `bincode` / `serde`).

### Step 1 (`v0.5.4`): Orchestrator & Subagent (`MiniPub`) Engine Overhaul
1. **Live Orchestrator Provider & Model Synchronization (`src/agent/subagent/pool.rs`, `src/agent/subagent/worker.rs`, `src/agent/loop.rs`)**:
   - Store the Orchestrator's live `(Arc<dyn Provider>, String)` runtime pair inside `SubagentPool` (`set_live_orchestrator_runtime`) and synchronize it automatically whenever `AgentLoop` initializes or switches models/providers.
   - Update `SubagentWorker::resolve_worker_provider_chain` so workers without an explicit role override inherit the Orchestrator's live active provider and model rather than reloading a potentially stale disk default.
2. **Real-Time Subagent Event Telemetry (`src/agent/subagent/pool.rs`, `src/agent/subagent/worker.rs`, `src/agent/loop.rs`)**:
   - Wire `event_sender: Arc<RwLock<Option<mpsc::UnboundedSender<AgentEvent>>>>` into `SubagentPool` and `SubagentWorker`.
   - Emit `AgentEvent::SubagentProgress` on worker spawn, turn start, and tool execution, and `AgentEvent::SubagentCompleted` on worker completion/failure/cancellation so `TimelineView` (`SubagentTree`), `ToastManager` (`ToastKind::Subagent`), and `--json-stream` receive live telemetry.
3. **Automatic Background Subagent Join & Boomerang Barrier (`src/agent/loop.rs`)**:
   - In `AgentLoop::execute_turn`, before allowing turn termination or firing a Boulder continuation nudge, inspect `SubagentPool` for active background workers (`active_worker_count() > 0`) or unconsumed completed worker reports (`format_orchestrator_context_block()`).
   - Automatically await active background workers (`pool.await_workers(&[], "all", timeout)`) and inject their Boomerang synthesis reports into the Orchestrator's conversation loop so background workers are never abandoned or raced.
4. **First-Class Subagent Tool Exposure & Parallel Worker Tool Execution (`src/tools/category.rs`, `src/agent/orchestrator.rs`, `src/agent/subagent/fanout.rs`, `src/agent/subagent/worker.rs`)**:
   - Expose `delegate_task`, `dispatch_subagent`, `fanout_subagents`, and `manage_subagents` directly in `get_core_schemas()` and include `ToolCategory::Agent` in `WorkflowRouter::state_driven_tool_categories()`.
   - Unify `FanoutTaskItem` and `fanout_subagents` schema to support all 14 `SubagentRole`s, `category`, and `load_skills`, sharing `SubagentPool`'s concurrency semaphore.
   - Upgrade `SubagentWorker::run` to partition multi-tool turns via `ExecutionPlanner::plan` and execute read-only tool batches concurrently via `futures::future::join_all`.

### Step 2 (`v0.5.5`): Brainstorming, Interactive Discovery (`ask_user`) & Prometheus/Metis/Momus State Machine
1. **Declarative Interactive Tool Classification & Sequential Gate (`src/tools/concurrency.rs`, `src/agent/orchestrator.rs`, `src/agent/loop.rs`)**:
   - Classify `ask_user` as strictly `ToolConcurrency::InteractiveSequential` in `src/tools/concurrency.rs` and `ExecutionPlanner::is_parallel_safe` (`false`).
   - In `AgentLoop::execute_turn` (`ExecutionStage::Parallel`), partition any `ask_user` call into `sequential_approvals` so it always runs through `ExecutionStage::Sequential`'s interactive `AgentEvent::UserInquiry` modal channel and never auto-resolves defaults in TUI mode.
2. **Core Schema Promotion & Option Bounds (`src/tools/registry/agent_tools/inquiry.rs`, `src/tools/category.rs`, `src/agent/prompt.rs`)**:
   - Relax `ask_user` option count bounds from `4..=6` to `2..=6` and promote `power_brainstorm` + `record_decision` into `get_core_schemas()`.
   - Resolve the Axiom #1 vs. Axiom #7 ordering in `STATIC_SYSTEM_PROMPT` and `WorkflowRouter::enrich_orchestrator_guidance` so **Discovery (`ask_user` / `power_brainstorm`) strictly precedes Planning (`create_plan` / `power_plan`)** when requirements or stack choices are unsettled.
3. **Prometheus / Metis / Momus Planning State Machine (`src/agent/minipower/mod.rs`, `src/tools/registry/fs_tools.rs`, `src/agent/loop.rs`)**:
   - Implement a workspace-scoped `PlanningPhaseState` (`DiscoveryWriteGuard` -> `PlanValidated` -> `ExecutionUnlocked`) persisted in `.minicode/plan/phase_state.json`.
   - When `power_brainstorm` starts discovery (Metis gap analysis + Prometheus interview), activate the markdown-only write guard (`is_allowed_discovery_write_path`: `.md`/`.txt`/`.json` inside `.minicode/` or `minikit_docs/` or `docs/`) blocking source-code mutations until `create_plan` / `power_plan` passes Momus structural validation.

### Step 3 (`v0.5.6`): Notifications Overhaul (Declarative In-TUI Toasts + OS Desktop & Terminal Bell Alerts)
1. **Declarative Lifecycle Toast Table (`src/ui/toast.rs`, `src/agent/loop.rs`)**:
   - Expand `SKILL_TOOL_TOAST_RULES` and `ToastManager::ingest_agent_event` with declarative table entries for `power_brainstorm`, `power_plan`, `power_review`, `power_verify`, `record_decision`, `delegate_task`, `dispatch_subagent`, `fanout_subagents`, `AgentEvent::UserInquiry`, and `AgentEvent::ApprovalRequest`.
   - Emit structured `AgentEvent::SkillActivity` / `Heartbeat` toasts when Boulder Continuation, Verification Barrier, or `ultrawork` (`ulw`) mode activates, and key tool-scoped `dedup_key`s by `(category, tool_name)` so concurrent tools do not overwrite each other.
2. **Cross-Platform System Attention Notifier (`src/ui/notifier.rs`, `src/app/mod.rs`)**:
   - Implement a zero-dependency `SystemNotifier` emitting OSC 9 / OSC 777 + terminal bell (`\x07`) and non-blocking `notify-send` (Linux) / `osascript` (macOS) desktop notifications when `AgentEvent::UserInquiry` (`ask_user`), `AgentEvent::ApprovalRequest`, or a long-running turn (`>= 15s`) completes.

### Step 4 (`v0.5.7`): `oh-my-opencode` Core Primitives (`.minicode/boulder.json` + `Ralph Loop` + `Hashline` Edits)
1. **Persistent `.minicode/boulder.json` & `Ralph Loop` Engine (`src/agent/boulder.rs`, `src/agent/loop.rs`)**:
   - Persist `BoulderState` (`.minicode/boulder.json`) tracking `active_plan_path`, `session_id`, `continuation_count`, `max_continuations`, `ralph_loop_active`, `completion_promise` (`<promise>DONE</promise>`), and `verified_complete` across turns and sessions.
   - Wire `BoulderState` into `AgentLoop::execute_turn` so multi-turn plans and self-referential Ralph Loops continue deterministically until all tasks are verified complete or the completion promise is emitted.
2. **Deterministic `Hashline` (`LINE#HASH|`) Content-Addressed Line Hashes (`src/tools/hashline.rs`, `src/tools/fs.rs`, `src/tools/registry/fs_tools.rs`)**:
   - Compute a fast 2-character base32 FNV-1a content hash (`compute_line_hash(line) -> [u8; 2]`) for stripped line content.
   - Support `hashline: true` / hash-prefixed line formatting (`11#VK| ...`) in `read_file` and hash-verified line-range edits (`start_line`, `end_line`, `start_hash`, `end_hash`) + automatic `11#VK|` prefix stripping in `patch_file` so LLMs copying line-numbered or hash-tagged snippets into `search`/`replace` blocks never corrupt files or fail exact matching.


# minicode — Todo Tracker

> **Current Phase:** v0.0.9 Release & Reliability Engine Hardening | **Status:** ✅ Completed (56 Tests Passing, 0 Warnings, Clean Clippy)

---

## ✅ Phase 1: Core Agent Infrastructure (COMPLETE)
- [x] Step 1: Terminal Hardening & Capability Probing
- [x] Step 2: Tool Output Compaction (ANSI + Exit-Code Aware)
- [x] Step 3: 2-Tier Core Memory (Global + Local)
- [x] Step 4: Filesystem Working Memory & Plan Archiving
- [x] Step 5: MCP Client (Official `rmcp 3.1.2`)
- [x] Step 6: MCP Server (`minicode serve`)

## ✅ Phase 1 Audit Fixes — Round 1 (COMPLETE)
- [x] Centralized constants module
- [x] Critical bug fixes (unwraps, mock_key, Box::leak)
- [x] MCP timeouts & process safety
- [x] MCP server protocol compliance
- [x] Error handling & tracing
- [x] Agent loop & main wiring
- [x] Performance & safety polish
- [x] All 35 tests pass, 0 clippy warnings

---

## ✅ Phase 1 Post-Audit Fixes — Round 2 (COMPLETE)

### Step 1: Security — Directory Escape in Backup (C1)
- [x] Replace `strip_prefix().unwrap_or()` with strict error in `session/backup.rs`
- [x] Add `test_backup_rejects_path_escape` unit test
- [x] Pass `cargo test -- backup` + clippy

### Step 2: Data Integrity — Session fsync (C3)
- [x] Add `file.sync_all()` after `writeln!` in `session/store.rs`
- [x] Replace `create_dir_all().ok()` with logged error (W2)
- [x] Replace `list_sessions().ok()?` with proper error handling (W3)
- [x] Pass `cargo test -- session` + clippy

### Step 3: Protocol — JSON-RPC 2.0 Notification Handling (C4 + I8)
- [x] Skip response for requests with `id: None` in `mcp/server.rs`
- [x] Replace `as usize` with `usize::try_from().ok()` for 32-bit safety
- [x] Pass `cargo test -- mcp` + clippy

### Step 4: Config System Fixes (C5 + W4 + W6 + W21 + W22 + I3)
- [x] Implement `Option<T>` wrapper types for config merge in `config.rs`
- [x] Remove TOCTOU `path.exists()` checks — use direct read + match NotFound
- [x] Log `.env` parse errors instead of `.ok()`
- [x] Remove redundant provider→env_var mapping in `main.rs` (use `get_api_key()`)
- [x] Extract `CONFIG_FILE_NAME` and `ENV_FILE_NAME` to `constants.rs`
- [x] Pass `cargo test -- config` + clippy

### Step 5: Auth & UI Thread Safety (C2 + C6 + W5 + W9)
- [x] Fix auth bypass in `app.rs` modal — use `config.get_api_key()`
- [x] Move git branch check to `tokio::task::spawn_blocking` in `ui/status.rs`
- [x] Log model-fetching errors instead of `unwrap_or_default()`
- [x] Pass `cargo test` + clippy

### Step 6: Tree-sitter ABI Guard (C7)
- [x] Add `test_treesitter_abi_versions_match` test in `context/repomap.rs`
- [x] Assert all language parser versions match core `LANGUAGE_VERSION`
- [x] Pass `cargo test -- abi` + clippy

### Step 7: Silent Error Swallow Sweep (12 files)
- [x] `tools/mod.rs` — Validate tool args properly, log checkpoint failures
- [x] `tools/exec.rs` — Log landlock sandbox errors
- [x] `tools/compactor.rs` — Log ANSI regex compile failure
- [x] `tools/web.rs` — Log CSS selector parse failures
- [x] `context/graph.rs` — Log AST parse failures during indexing
- [x] `context/skills.rs` — Log skill file read errors
- [x] `agent/loop.rs` — Log session creation failure + channel disconnections
- [x] `app.rs` — Log channel disconnections
- [x] `mcp/client.rs` — Log HTTP client build failure
- [x] `session/undo.rs` — Log `remove_file` failure during undo
- [x] `session/backup.rs` — Log `remove_dir_all` failure during pruning
- [x] Pass `cargo test` + clippy

### Step 8: Constants Consolidation (6 files)
- [x] Add `FUZZY_MATCH_THRESHOLD`, `MAX_SEARCH_RESULTS`, exec/web/compressor/graph constants
- [x] Replace inline values in `fs.rs`, `search.rs`, `exec.rs`, `web.rs`, `compressor.rs`, `graph.rs`
- [x] Move ASCII wordmark from `ui/view.rs` to `constants.rs`
- [x] Extract `CONFIG_FILE_NAME`, `ENV_FILE_NAME`
- [x] Pass `cargo test` + clippy

### Step 9: Process Safety — Zombie Prevention (W23)
- [x] Add `process_group(0)` on Unix for MCP child processes in `mcp/client.rs`
- [x] Kill entire process group on timeout/cleanup
- [x] Pass clippy + manual test

### Step 10: Dead Code Cleanup & Polish (I1 + I2 + I4)
- [x] Remove blanket `#[allow(dead_code)]` from error enums, prune unused variants
- [x] Remove or mark dead `AgentEvent` variants
- [x] Upgrade landlock degradation log from `debug!` to `warn!`
- [x] Final `cargo fmt` + `cargo test` + `cargo clippy -- -D warnings`
- [x] All tests pass, 0 warnings

---

## 🚀 Phase 1 Post-Audit Fixes — Round 3 (COMPLETED)

### Phase 1: Critical Runtime Panics & LLM Protocol (P0)
- [x] 1.1 UTF-8 character boundary guards (`exec.rs`, `web.rs`, `prompt.rs`, `configure.rs`, `modal.rs`)
- [x] 1.2 Fix `Message::tool_result` passing `tool_call.id` in `loop.rs`
- [x] 1.3 Strict exit code check & flag awareness in `compactor.rs`
- [x] 1.4 Add `kill_on_drop(true)` to child processes in `exec.rs`

### Phase 2: Sandbox & Security Hardening (P0 & P1)
- [x] 2.1 Move Landlock `restrict_self` to child `pre_exec` hook on Unix in `landlock.rs` / `exec.rs`
- [x] 2.2 Allow read-write access to `/tmp` in `landlock.rs`
- [x] 2.3 Expand sensitive environment variable sanitization in `env.rs`
- [x] 2.4 Lexical path normalization in `sandbox/path.rs`

### Phase 3: Tools & Filesystem Correctness (P1 & P2)
- [x] 3.1 Empty file read support in `tools/fs.rs`
- [x] 3.2 Trailing newline preservation in `tools/fs.rs`
- [x] 3.3 Sliding-window fuzzy patch matching in `tools/fs.rs`
- [x] 3.4 Atomic file writes via tempfile + rename in `tools/fs.rs`
- [x] 3.5 Accurate glob-to-regex conversion in `tools/search.rs`
- [x] 3.6 Path validation before checkpoint in `tools/mod.rs`
- [x] 3.7 Web DOM selector deduplication in `tools/web.rs`

### Phase 4: AST Graph, Memory & Compressor (P1 & P2)
- [x] 4.1 Populate directed graph edges from symbols in `context/graph.rs`
- [x] 4.2 Safe unique temp files in `context/memory.rs`
- [x] 4.3 Line-anchored checkbox updates in `context/working_memory.rs`
- [x] 4.4 Float standardization & cleanup in `context/compressor.rs`

### Phase 5: Configuration & Session Management (P0, P1, P2)
- [x] 5.1 Config wizard filename alignment to `CONFIG_FILE_NAME` in `ui/configure.rs`
- [x] 5.2 Skip invalid MCP servers in `config.rs`
- [x] 5.3 Add `MINICODE_APPROVAL_POLICY` to `apply_env_overrides()` in `config.rs`
- [x] 5.4 Always load workspace MCP configs in `config.rs`
- [x] 5.5 Multi-turn `/undo` directory cleanup in `session/undo.rs`
- [x] 5.6 Relative path handling in `session/backup.rs`
- [x] 5.7 CLI `--resume` and `--continue` wiring in `main.rs`

### Phase 6: MCP Client Tool Discovery & Protocol (P1 & P2)
- [x] 6.1 Global and workspace MCP config discovery in `config.rs` & `mcp/client.rs`
- [x] 6.2 Stdio JSON-RPC 2.0 error handling and response formatting in `mcp/server.rs`
- [x] 6.3 Process group isolation and cleanup on stdio MCP client drop

### Phase 7: UI Polish, Constants Centralization & Documentation (P2 & P3)
- [x] 7.1 Auto-scroll boundary clamping in `ui/view.rs`
- [x] 7.2 Non-blocking background git branch probing & invalidation in `ui/status.rs`
- [x] 7.3 Centralize constants & add doc comments across crate in `constants.rs`
- [x] 7.4 Full test suite validation (43/43 unit tests passing, zero warnings)

---

## ✅ Phase 2: AST Code Intelligence & Symbol Search Indexing (COMPLETED)
- [x] Enhanced Tree-sitter multi-language parsing with signatures & doc comments in `src/context/repomap.rs`
- [x] Directed dependency knowledge graph & Tarjan SCC cycle detection in `src/context/graph.rs`
- [x] Blast radius impact analysis (`get_blast_radius`) with test coverage correlation in `src/context/graph.rs`
- [x] Fast in-memory inverted symbol index with subword tokenization & BM25 ranking in `src/context/index.rs`
- [x] New agent tools (`impact_analysis` & `locate_symbol`) in `src/tools/mod.rs`
- [x] Protocol exposure in MCP server `tools/list` & `tools/call` in `src/mcp/server.rs`
- [x] 46/46 unit tests passing with zero clippy warnings and clean formatting

---

## ✅ Phase 2 Post-Audit Fixes & Optimizations (COMPLETED)
- [x] **C1 (P0)**: Inverted graph edge construction from O(V×S) full scan to O(V×I) single-pass `HashSet` identifier lookup in `src/context/graph.rs`
- [x] **H1 (P1)**: Implemented standard Robertson-Spärck Jones BM25 formula (IDF, TF, document length normalization, k1/b tuning) in `src/context/index.rs`
- [x] **H2 (P1)**: Replaced `HashMap` linear prefix scan with `BTreeMap` O(log N + K) range query in `src/context/index.rs`
- [x] **M1 (P1)**: Verified and tested TypeScript AST class declaration grammar in `src/context/repomap.rs`
- [x] **M2 (P2)**: Centralized Blast Radius risk thresholds into `src/constants.rs`
- [x] **M3 (P1)**: Synchronized `repo_map` in `ToolRegistry::get_tool_schemas()` (17 built-in tools) and `dispatch_tool`
- [x] **M4 (P2)**: Centralized sandbox environment whitelist, secret patterns, and blocked prefixes into `src/constants.rs`
- [x] **L2 (P2)**: Optimized compressor `mask_observation` to stream head and tail lines without giant intermediate `Vec<&str>` allocations in `src/context/compressor.rs`
- [x] **L3 & L9 (P2)**: Added `parse_u64_param` helper for robust string/number numeric parsing across `src/tools/mod.rs` and `src/mcp/server.rs`
- [x] **L4 (P2)**: Added strict object shape validation for MCP `params` and `arguments` in `src/mcp/server.rs`
- [x] **L5 (P2)**: Added `MAX_REGEX_QUERY_LEN` protection against ReDoS in `src/tools/search.rs`
- [x] **L6, L7, L8, L10, L11 (P3)**: Added `MCP_TOOL_PREFIX`, directory constants in `store.rs`/`backup.rs`/`skills.rs`, single-char symbol preservation in `index.rs`, and `#[must_use]` on `build_system_prompt` in `prompt.rs`
- [x] **Verification**: **50/50 unit tests passing, zero clippy warnings, clean formatting**

---

## ✅ Phase 2 Post-Audit Fixes — Round 2 (v0.0.7 COMPLETED)
- [x] **C1 (Critical)**: Reset `iteration_text`, `pending_tool_calls`, and truncate `turn_response` on stream retry in `src/agent/loop.rs`
- [x] **C2 (Critical)**: Propagate `apply_landlock_sandbox` errors in `pre_exec` hook in `src/tools/exec.rs`
- [x] **H1 (High)**: Log warnings on complementary local/global save errors instead of swallowing with `.ok()` in `src/context/memory.rs`
- [x] **M1 (Medium)**: Parse `Retry-After` HTTP header from response on HTTP 429 rate limit in `src/agent/provider.rs`
- [x] **M2 (Medium)**: Trim API keys retrieved from environment variables in `src/config.rs` + added `test_get_api_key_trims_whitespace`
- [x] **M3 (Medium)**: Cache file contents during AST extraction in `src/context/graph.rs` to eliminate redundant disk reads during edge building
- [x] **M4 (Medium)**: Replaced O(N²) `test_coverage` `Vec` lookup with `HashSet` in `src/context/graph.rs`
- [x] **M5 (Medium)**: Added `tracing::debug!` for unreadable files skipped during `grep_search` in `src/tools/search.rs`
- [x] **M6 (Medium)**: Propagate file read errors during plan archiving in `src/context/working_memory.rs` to prevent data loss
- [x] **M7 (Medium)**: Added `tracing::warn!` when Landlock network restriction falls back on kernels without ABI V4 in `src/sandbox/landlock.rs`
- [x] **M8 (Medium)**: Added `MAX_WEB_RESPONSE_BYTES` (10 MB) size check to prevent OOM on large web responses in `src/tools/web.rs`
- [x] **L1 (Low)**: Centralized model provider API URLs and fetch timeouts in `src/constants.rs` and `src/agent/models.rs`
- [x] **L3 (Low)**: Added `MissedTickBehavior::Skip` on TUI interval ticker in `src/app.rs`
- [x] **L4 (Low)**: Centralized `BM25_PREFIX_WEIGHT` in `src/constants.rs` and `src/context/index.rs`
- [x] **L5 (Low)**: Centralized `COMPRESSOR_MASK_LINES` in `src/constants.rs` and `src/context/compressor.rs`
- [x] **L6 (Low)**: Centralized `SUPPORTED_LANG_EXTENSIONS` in `src/constants.rs` and `src/context/graph.rs`
- [x] **L7 (Low)**: Added security documentation comment clarifying TOCTOU user-space limitation in `src/sandbox/path.rs`
- [x] **L8 (Low)**: Supported recursive directory deletion for paths created during rolled-back turns in `src/session/undo.rs` + added `test_undo_rollback_deletes_created_directory`
- [x] **L9 (Low)**: Centralized `DEFAULT_LOCATE_SYMBOL_LIMIT` in `src/constants.rs` and `src/mcp/server.rs`
- [x] **Verification**: **52/52 unit tests passing, zero clippy warnings, clean formatting**

---

## ✅ Phase 2 Post-Audit Fixes — Round 3 (v0.0.8 COMPLETED)
- [x] **H1 (High)**: Implemented sliding window conversation context pruner (`prune_context()`) in `src/agent/loop.rs` to prevent OOM and context window exhaustion in long sessions
- [x] **H2 & M3 (High/Medium)**: Handled and logged filesystem errors in model cache loading (`load_cache`) and saving (`save_cache`) in `src/agent/models.rs`
- [x] **H3 (High)**: Logged warnings on temporary file cleanup failures during atomic write rollbacks in `src/tools/fs.rs` and `src/context/memory.rs`
- [x] **M1 & L1 (Medium/Low)**: Replaced hardcoded provider URLs and timeouts with centralized constants (`GEMINI_BASE_URL`, `OPENROUTER_BASE_URL`, `PROJECT_REPO_URL`, `PROVIDER_REQUEST_TIMEOUT_SECS`, `PROVIDER_STREAM_TIMEOUT_SECS`) in `src/agent/provider.rs`
- [x] **M2 (Medium)**: Used centralized directory and filename constants (`CONFIG_DIR_NAME`, `WORKSPACE_DIR_NAME`, `MODELS_CACHE_FILE`) in `src/agent/models.rs`
- [x] **M4 (Medium)**: Fixed stale tool count doc comment in `src/tools/mod.rs`
- [x] **M5 (Medium)**: Removed blanket `#![allow(dead_code)]` directives across 14 modules, replacing with targeted item-level attributes
- [x] **L4 & L5 (Low)**: Replaced magic exit code `-1` with `SIGNAL_KILLED_EXIT_CODE` in `src/tools/exec.rs` and hardcoded limit `10` with `DEFAULT_LOCATE_SYMBOL_LIMIT` in `src/tools/mod.rs`
- [x] **Verification**: **54/54 unit tests passing, zero clippy warnings, clean formatting**

---

## ✅ Phase 2 Post-Audit Fixes — Round 4 (v0.0.9 COMPLETED)
- [x] **C1 (Critical)**: Fixed silent error swallowing in configuration wizard file writes (`save_all`, `update_dotenv`) using `ConfigError::FileWrite` and `ConfigError::TomlSerialize` in `src/ui/configure.rs`
- [x] **C2 (Critical)**: Standardized MCP client protocol handshake (`initialize` -> `notifications/initialized`) and attached `_meta` attribution for stdio and HTTP tool calls in `src/mcp/client.rs`
- [x] **H1 (High)**: Handled streaming tool call JSON syntax errors with structured error marker (`__json_parse_error`) providing actionable feedback to LLM in `src/agent/provider.rs` and `src/tools/mod.rs`
- [x] **H2 (High)**: Replaced unbounded web response buffering with stream chunking using `bytes_stream()` and `MAX_WEB_RESPONSE_BYTES` in `src/tools/web.rs`
- [x] **H3 (High)**: Implemented deadlock-free bounded subprocess stream reading with `tokio::io::AsyncReadExt::take()` in `src/tools/exec.rs` to bound memory to constant $O(1)$
- [x] **H4 (High)**: Centralized extension matching in symbol index using `SUPPORTED_LANG_EXTENSIONS` in `src/context/index.rs`
- [x] **M1 (Medium)**: Dynamically projected MCP server `tools/list` from `ToolRegistry::get_tool_schemas()` in `src/mcp/server.rs` (100% DRY)
- [x] **M2 (Medium)**: Added uniqueness checks to whitespace and fuzzy patch replacement in `src/tools/fs.rs` to prevent ambiguous replacements
- [x] **M3 (Medium)**: Removed system prompt index assumption in `ContextCompressor::compact_history` in `src/context/compressor.rs`
- [x] **M4 (Medium)**: Handled and logged channel send errors for `prompt_tx` and `event_tx` in `src/app.rs`
- [x] **L1 (Low)**: Reset turn retry count per iteration in `src/agent/loop.rs`
- [x] **L2 (Low)**: Centralized `UI_MAX_TOOL_OUTPUT_LINES` in `src/constants.rs` and `src/ui/view.rs`
- [x] **L3 (Low)**: Added warning logging for MCP notification tool execution failures in `src/mcp/server.rs`
- [x] **L4 (Low)**: Used `tokio::task::spawn_blocking` for background git branch probing in `src/ui/status.rs`
- [x] **Verification**: **56/56 unit tests passing, zero clippy warnings, clean formatting**

---

## ✅ Phase 2 Post-Audit Fixes — Round 5 (v0.0.10 COMPLETED)
- [x] **C1 (Critical)**: Merged consecutive `Role::Tool` messages into single `role: "user"` message containing all `functionResponse` parts in `src/agent/provider.rs` to satisfy Gemini multi-turn format.
- [x] **C2 (Critical)**: Implemented PageRank dangling node mass redistribution and exact L1 normalization ($\sum P_i = 1.0$) in `src/context/graph.rs` + multi-declaration symbol lookup.
- [x] **C3 (Critical)**: Integrated `tokio_util::sync::CancellationToken` for cooperative agent task cancellation on `Esc`/`Ctrl+C` across streaming and tool execution in `src/agent/loop.rs` and `src/app.rs`.
- [x] **C4 (Critical)**: Safety checkpoint creation automatically persists/updates `manifest.json` on disk using validated absolute paths in `src/session/backup.rs`.
- [x] **H1 (High)**: Direct execution for stdio MCP servers without `sh -c` argument loss in `src/mcp/client.rs`.
- [x] **H3 (High)**: Added graceful fallback for unsupported Landlock host kernels (`ENOSYS`, `EOPNOTSUPP`) in `src/sandbox/landlock.rs`.
- [x] **H4 (High)**: Expanded JavaScript/TypeScript Tree-sitter queries for methods, arrow functions, and enums in `src/context/repomap.rs`.
- [x] **H5 (High)**: Merged `OllamaConfig` in `RawProviderConfig` and `merge_raw` in `src/config.rs`.
- [x] **H6 (High)**: Gemini SSE stream error extraction (`promptFeedback.blockReason`, stream `error`, and candidate `finishReason`) in `src/agent/provider.rs`.
- [x] **M1 (Medium)**: Switched Gemini authentication to `x-goog-api-key` header in `src/agent/provider.rs` and `src/agent/models.rs`.
- [x] **M3 (Medium)**: Added SSRF protection for `fetch_or_browse` in `src/tools/web.rs` blocking loopback, link-local, private subnets, and cloud metadata.
- [x] **M5 (Medium)**: Fixed OpenAI streaming tool name deduplication in `src/agent/provider.rs`.
- [x] **M6 (Medium)**: Hardened AST cache invalidation with `(mtime, file_size)` tuple in `src/context/repomap.rs`.
- [x] **M8 (Medium)**: Process group timeout termination with SIGTERM and SIGKILL escalation via `libc::kill` in `src/tools/exec.rs`.
- [x] **M9 (Medium)**: Resilient session listing with directory entry flattening in `src/session/store.rs`.
- [x] **M11 (Medium)**: Dynamic provider and model display in Aura TUI status bar in `src/ui/status.rs` and `src/app.rs`.
- [x] **Constants Extraction**: Added `SSRF_BLOCKED_HOSTS`, `INDEX_CACHE_MAX_ENTRIES`, and `PROCESS_KILL_GRACE_PERIOD_MS` to `src/constants.rs`.
- [x] **Verification**: **63/63 unit tests passing, zero clippy warnings, clean formatting**

---

## ✅ Phase 3: CI/CD Pipeline (v0.0.12 COMPLETED)

- [x] Create `.github/workflows/ci.yml` (lint, test-linux, test-macos matrix)
- [x] Add CI status badge to `README.md`
- [x] Add `tempfile` to `[dev-dependencies]` for testing

---

## ✅ Phase 3: Hardened Git Service Engine (v0.0.12 COMPLETED)

### Step 1: Git Service & Subprocess Hardening (`src/git/service.rs`)
- [x] Create `src/git/mod.rs` with `pub mod service; pub mod commit; pub mod diff_filter;`
- [x] Implement `GitService` with mandatory env vars (`GIT_TERMINAL_PROMPT=0`, `GIT_PAGER=cat`, `LC_ALL=C`, `--no-pager`)
- [x] Implement `GitCommitService` (commit, commit_files, undo_last_commit, create_branch, checkout_branch, merge conflict detection)
- [x] Implement `DiffFilter` (lockfile summarization for `Cargo.lock`, `package-lock.json`, etc.)
- [x] Add `GitError` variants to `src/error.rs`
- [x] Add git constants (`GIT_COMMIT_MSG_MAX_LEN`, `GIT_DIFF_MAX_BYTES`, `GIT_LOG_DEFAULT_COUNT`, `GIT_TIMEOUT_SECS`) to `src/constants.rs`
- [x] Write 10 unit tests using temp git repos (all passing)
- [x] `cargo test && cargo clippy -- -D warnings`

### Step 2: Autonomous Git Agent Tools (`src/tools/mod.rs`)
- [x] Register `git_status` tool schema (branch, clean/dirty, staged/unstaged/untracked)
- [x] Register `git_diff` tool schema (with smart lockfile filtering)
- [x] Register `git_commit` tool schema (commit message, optional path filter)
- [x] Register `git_log` tool schema (recent commit history)
- [x] Register `git_conflicts` tool schema (detect and extract conflict markers)
- [x] Register `create_pr` tool schema (auto-probes `gh` binary for PR creation)
- [x] Wire tool dispatchers in `ToolRegistry::dispatch_tool()`
- [x] Invalidate git branch status cache after commit
- [x] Update tool schema count in unit tests (23 tools)
- [x] `cargo test && cargo clippy -- -D warnings` (73 tests passing)

---

## ✅ Phase 3: Autonomous Commit Loop & Rollback (v0.0.13 COMPLETED)

### Step 1: Auto-Commit in Agent Loop
- [x] Add `GitConfig` struct to `src/config.rs` (auto_commit, dirty_commit, ai_commit_messages)
- [x] Wire git config section in TOML parsing and `merge_raw`
- [x] Add `AgentEvent::GitCommit` variant to `src/agent/types.rs`
- [x] Integrate post-turn auto-commit in `execute_turn` when `files_modified > 0`
- [x] Enhance `/undo` to run `git reset --soft HEAD~1` when auto-commit was active
- [x] Write unit & integration tests for auto-commit flow
- [x] `cargo test && cargo clippy -- -D warnings`

### Step 2: Two-Tier Integration Test Framework
- [x] Add `base_url` override parameter to provider constructors in `src/agent/provider.rs`
- [x] Create `tests/common/mod.rs` and `tests/common/mock_provider.rs`
- [x] Implement `MockProvider` with scripted responses and `Provider` trait impl
- [x] Write `tests/integration_agent_loop.rs` (read-edit-commit turn, tool errors, max iterations, cancellation)
- [x] Write `tests/integration_git_tools.rs` (status, diff, commit, auto-commit, undo rollback)
- [x] Verify all integration tests pass with `cargo test --test '*'`
- [x] `cargo test && cargo clippy -- -D warnings`

---

## ✅ Phase 3: UI Session Controls & Codebase View (v0.0.14 COMPLETED)

- [x] Register `/retry`, `/save`, `/load`, `/map`, `/compact`, `/tokens` in `SLASH_COMMANDS`
- [x] Implement `/retry` — re-send last user prompt to agent actor
- [x] Implement `/save <file>` — export session history to Markdown
- [x] Implement `/load <id>` — hydrate timeline from stored session
- [x] Implement `/map` — render AST PageRank repo map in timeline
- [x] Implement `/compact` — trigger manual context compression
- [x] Implement `/tokens` — show context token breakdown card
- [x] `cargo test && cargo clippy -- -D warnings`

---

## ✅ Phase 3: Multi-Agent Worktree Orchestration (v0.0.12 COMPLETED)

- [x] Create `src/git/worktree.rs` (`git worktree add .minicode/worktrees/<id> -b subagent/<id>`, cleanup, merge)
- [x] Create `src/agent/subagent.rs` (SubAgent struct, spawn, NDJSON stdio streaming, cancel)
- [x] Create `src/agent/orchestrator.rs` (spawn, wait_all, abort, list)
- [x] Register `delegate_task` tool schema in `src/tools/mod.rs` (24 built-in agent tools total)
- [x] Wire `delegate_task` dispatcher with isolated worktree execution
- [x] Write integration test for parallel worktree subagents (`tests/integration_subagents.rs`)
- [x] `cargo test && cargo clippy -- -D warnings` (80 tests passing, 0 clippy warnings)

---

## ✅ Phase 4: Language Server Protocol (LSP) Engine & 2-Tier Compiler Diagnostics (v0.0.13)

- [x] Add `lsp-types = "0.97"` to `Cargo.toml`
- [x] Create `src/lsp/mod.rs`, `src/lsp/client.rs`, `src/lsp/protocol.rs`, `src/lsp/diagnostics.rs`
- [x] Implement stdio JSON-RPC 2.0 client with `Content-Length` header framing
- [x] Implement Tier 1 Fast-Path Compiler Checker (`cargo check --message-format=json`, `tsc --noEmit`, `ruff`)
- [x] Implement Tier 2 LSP Auto-discovery (`rust-analyzer`, `pyright`/`pylsp`, `typescript-language-server`, `gopls`)
- [x] Implement `lsp_diagnostics`, `lsp_goto_definition`, and `lsp_find_references` tools in `src/tools/mod.rs` (27 tools total)
- [x] Integrate autonomous compiler self-healing check in `AgentLoop::execute_turn`
- [x] Write unit & integration tests for LSP protocol, compiler diagnostics, and self-healing (`tests/integration_lsp_diagnostics.rs`)
- [x] `cargo test && cargo clippy -- -D warnings` (86 tests passing, 0 clippy warnings)

---

## 🔲 Phase 5: Interactive TUI Diff Inspector & Permission Modal (v0.0.14)

- [ ] Create `src/ui/diff_viewer.rs` for colored unified diff rendering
- [ ] Create `src/ui/approval.rs` modal widget for permission gates
- [ ] Wire approval request handling in `src/app.rs` and `AgentEvent::ApprovalRequest`
- [ ] Add `[y] Accept`, `[n] Reject`, `[e] Edit`, `[a] Allow All` keyboard event handlers
- [ ] `cargo test && cargo clippy -- -D warnings`

---

## 🔲 Phase 6: Native Web Search Engine with Anti-Scrape Resilience (v0.0.15)

- [ ] Create `src/tools/web_search.rs`
- [ ] Implement DuckDuckGo zero-API-key HTML scraper with clean markdown formatting
- [ ] Add in-memory TTL search cache (15-minute expiration) to avoid rate-limiting
- [ ] Implement Tavily / Brave Search API fallbacks with `.env` keys
- [ ] Register `search_web` in `ToolRegistry` (26 built-in agent tools total)
- [ ] Write unit tests for search query construction and response parsing
- [ ] `cargo test && cargo clippy -- -D warnings`

---

## 🔲 Phase 7: Release Matrix & Distribution (v0.0.16)

- [ ] Create `.github/workflows/release.yml` with cross-compilation matrix
- [ ] Create `install.sh` one-line installation script
- [ ] Update documentation, PRD, and README with new version features and quickstart guides
- [ ] `cargo test -j 2 -- --test-threads=2` (all targets green)



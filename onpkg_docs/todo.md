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

## ✅ Phase 5: Interactive TUI Diff Inspector & Permission Menu Modal (v0.0.14)

- [x] Create `src/ui/diff_viewer.rs` for syntax-highlighted Aura Theme colored unified diff rendering
- [x] Create `src/ui/approval.rs` modal widget with interactive 4-option selection menu and custom feedback input
- [x] Wire approval request handling in `src/app.rs` and `AgentEvent::ApprovalRequest`
- [x] Add `↑/↓/j/k` menu navigation, `1-4` direct keys, and `Enter` confirmation (`[1] Accept`, `[2] Reject`, `[3] Allow Session`, `[4] Type Feedback`)
- [x] Write unit & integration tests (`tests/integration_diff_modal.rs`)
- [x] `cargo test && cargo clippy -- -D warnings` (92 tests passing, 0 clippy warnings)

---

## ✅ Phase 6: Native Web Search Engine with Anti-Scrape Resilience (v0.0.15)

- [x] Create `src/tools/web_search.rs`
- [x] Implement DuckDuckGo zero-API-key HTML scraper with clean markdown formatting
- [x] Add in-memory TTL search cache (15-minute expiration) to avoid rate-limiting
- [x] Implement Tavily / Brave Search API fallbacks with `.env` keys
- [x] Register `search_web` in `ToolRegistry` (28 built-in agent tools total)
- [x] Write unit & integration tests (`tests/integration_web_search.rs`)
- [x] `cargo test && cargo clippy -- -D warnings` (96 tests passing, 0 clippy warnings)

---

## ✅ Phase 7: Release Matrix & Distribution (v0.0.16)

- [x] Create `.github/workflows/release.yml` with cross-compilation matrix (Linux x86_64, Linux ARM64, macOS Apple Silicon, macOS Intel, Windows x86_64)
- [x] Create `install.sh` one-line installation script with auto-architecture detection
- [x] Update documentation, PRD, and README with new version features and quickstart guides
- [x] `cargo test -j 2 -- --test-threads=2` (96 tests passing, 0 clippy warnings)

---

## ✅ Phase 8: Topological Task DAG & Actor-Critic Loop (v0.0.17)

- [x] Create `src/agent/task_dag.rs` with `petgraph` dependency resolution, cycle detection & complexity scoring heuristics (1–10)
- [x] Create `src/agent/critic.rs` for automated Actor-Critic quality gates (compiler diagnostics, git status, test verification)
- [x] Register 4 new agent tools: `create_task_dag`, `get_next_task`, `complete_task`, and `critic_review` (expanding to **32 agent tools total**)
- [x] Write unit & integration tests (`tests/integration_task_dag.rs`)
- [x] `cargo test && cargo clippy -- -D warnings` (101 tests passing, 0 clippy warnings)

---

## ✅ Phase 9: Sequential Thinking & Graph of Thoughts (GoT) (v0.0.18)

- [x] Create `src/agent/sequential_thinking.rs` for dynamic Graph of Thoughts reasoning with petgraph DAG tracking
- [x] Implement hypothesis branching, confidence scoring, thought revisions, and synthesized trajectory outlines
- [x] Register `sequential_thinking` in `ToolRegistry` (expanding built-in tools to **33 agent tools total**)
- [x] Write unit & integration tests (`tests/integration_sequential_thinking.rs`)
- [x] `cargo test && cargo clippy -- -D warnings` (104 tests passing, 0 clippy warnings)

---

## ✅ Phase 10: Cognitive Memory Decay & Knowledge Wiki Engine (v0.0.19)

- [x] Create `src/context/decay.rs` with biological exponential memory decay modeling (`Permanent`, `Milestone`, `Transient`)
- [x] Create `src/context/wiki.rs` for persistent Markdown knowledge wiki cataloging (`.minicode/wiki/`)
- [x] Register 3 new agent tools: `wiki_write`, `wiki_read`, and `wiki_search` (expanding built-in tools to **36 agent tools total**)
- [x] Write unit & integration tests (`tests/integration_wiki_and_decay.rs`)
- [x] `cargo test && cargo clippy -- -D warnings` (109 tests passing, 0 clippy warnings)

---

## ✅ Phase 11: ARIA Web Browser & Local UI Dev Inspector (v0.0.20)

- [x] Create `src/tools/browser.rs` with ARIA accessibility tree generator and numbered element references (`@e1`, `@e2`, ...)
- [x] Implement page snapshots with interactive control parsing (`<button>`, `<a>`, `<input>`, `<select>`, `<textarea>`)
- [x] Register 2 new agent tools: `browser_navigate` and `browser_snapshot` (expanding built-in tools to **38 agent tools total**)
- [x] Write unit & integration tests (`tests/integration_browser.rs`)
- [x] `cargo test && cargo clippy -- -D warnings` (112 tests passing, 0 clippy warnings)

---

## ✅ Phase 12: Interactive Embedded PTY Terminal Drawer (v0.0.21)

- [x] Create `src/ui/pty_drawer.rs` with 1000-line bounded ring buffer, scrollable viewport & input prompt
- [x] Wire `Ctrl+T` shortcut and `/terminal` slash command into `src/app.rs` event loop
- [x] Integrate asynchronous shell execution directly inside bottom 40% viewport drawer
- [x] Write unit & integration tests (`tests/integration_pty_drawer.rs`)
- [x] `cargo test && cargo clippy -- -D warnings` (115 tests passing, 0 clippy warnings)

---

## ✅ Phase 13: Dynamic Skill Creation & Hot-Reloading Engine (v0.0.22)

- [x] Create `src/context/skill_forge.rs` for dynamic skill creation, YAML frontmatter formatting, and hot-reload discovery
- [x] Update `src/context/skills.rs` with frontmatter field parser and `.minicode/skills/` directory traversal
- [x] Register 3 new agent tools: `create_skill`, `list_skills`, `inspect_skill` (expanding built-in tools to **41 agent tools total**)
- [x] Write unit & integration tests (`tests/integration_skill_forge.rs`)
- [x] `cargo test && cargo clippy -- -D warnings` (118 tests passing, 0 clippy warnings)

---

## ✅ Phase 14: Sub-millisecond Local Semantic Code Search (v0.0.23)

- [x] Create `src/context/semantic.rs` with 128-dim character n-gram + subword hashing projections and cosine similarity ranking
- [x] Implement incremental source chunking (~25 lines) and disk cache persistence in `.minicode/cache/semantic_index.json`
- [x] Register `semantic_search` agent tool (expanding built-in tools to **42 agent tools total**)
- [x] Write unit & integration tests (`tests/integration_semantic_search.rs`)
- [x] `cargo test && cargo clippy -- -D warnings` (122 tests passing, 0 clippy warnings)

---

## ✅ Phase 15: Tree-sitter AST Pattern Matching & Symbol Extraction (v0.0.24)

- [x] Create `src/context/ast_transform.rs` with multi-language Tree-sitter AST query engine (Rust, Python, JS, TS)
- [x] Implement node kind and name filtering, visibility modifier detection, and symbol body extraction
- [x] Register 2 new agent tools: `ast_query` and `ast_extract_symbol` (expanding built-in tools to **44 agent tools total**)
- [x] Write unit & integration tests (`tests/integration_ast_transform.rs`)
- [x] `cargo test && cargo clippy -- -D warnings` (125 tests passing, 0 clippy warnings)

---

## ✅ Phase 16: Multi-Turn Context Observation Deduplication (v0.0.25)

- [x] Create `src/context/dedup.rs` with fingerprint hash tracking for repetitive file reads and identical compiler check logs
- [x] Hook automatic observation deduplication into `prune_context()` in `src/agent/loop.rs`
- [x] Register `prune_context` agent tool (expanding built-in tools to **45 agent tools total**)
- [x] Write unit & integration tests (`tests/integration_context_dedup.rs`)
- [x] `cargo test && cargo clippy -- -D warnings` (128 tests passing, 0 clippy warnings)

---

## ✅ Phase 17: Dynamic Multi-Branch Hypothesis Search & Speculative Rollout (v0.0.26)

- [x] Create `src/agent/hypothesis.rs` for parallel speculative branch generation in isolated Git worktrees
- [x] Implement automated compiler diagnostic scoring, test pass rates, and normalized branch fitness metrics (0.0–1.0)
- [x] Implement optimal branch selection with automatic cleanup of rejected speculative branches
- [x] Register 3 new agent tools: `explore_hypotheses`, `evaluate_branch`, `select_best_branch` (expanding built-in tools to **48 agent tools total**)
- [x] Write unit & integration tests (`tests/integration_hypothesis_tree.rs`)
- [x] `cargo test && cargo clippy -- -D warnings` (131 tests passing, 0 clippy warnings)

---

## ✅ Phase 18: Smooth Mouse Wheel & Multi-Key TUI Timeline Scrolling (v0.0.27)

- [x] Enable native mouse capture (`crossterm::EnableMouseCapture`) for mouse wheel and trackpad timeline scrolling
- [x] Implement dynamic auto-scroll state management with `Cell<u16>` and `Cell<bool>` in `src/ui/view.rs`
- [x] Add multi-key keyboard navigation (`PageUp`/`PageDown`, `Shift+↑/↓`, `Ctrl+↑/↓`, `Alt+↑/↓`, empty input dock `↑/↓`, `Home`/`End`)
- [x] Fix terminal drawer (`Ctrl+T`) scroll offset calculation in `src/ui/pty_drawer.rs`
- [x] Update in-TUI Help modal (`/help`) with full scrolling shortcuts documentation
- [x] Write unit tests (`test_timeline_scrolling_and_auto_scroll_resumption` in `src/ui/view.rs`)
- [x] `cargo test && cargo clippy -- -D warnings` (132 tests passing, 0 clippy warnings)

---

## ✅ Phase 19: Context-Aware AST Diffing & Deterministic Mock Provider (v0.0.29)
- [x] Create `src/context/ast_diff.rs` for structural AST deltas and breaking change rule validation
- [x] Create `src/agent/mock_provider.rs` & `src/agent/replay.rs` for deterministic simulation and offline regression replay
- [x] Register `ast_diff` agent tool (expanding to 49 agent tools total)
- [x] Pass all 144 tests 100% green

---

## ✅ Phase 20: Sentrux Architecture Governance & RTK Output Filter (v0.0.30)
- [x] Create `src/context/governance.rs` for layer boundary checking, cyclic dependency detection (Tarjan SCC), and modularity scoring
- [x] Create `src/tools/rtk_filter.rs` cutting 60–90% token waste from compiler and test outputs
- [x] Register `check_architecture` agent tool (expanding to 50 agent tools milestone)
- [x] Pass all 148 tests 100% green

---

## ✅ Phase 21: Task Complexity Scorer & Dollar Spend Telemetry (v0.0.31)
- [x] Create `src/agent/complexity.rs` for task difficulty ($1$–$10$) and auto-decomposition planning
- [x] Create `src/agent/pricing.rs` with per-model token pricing across Claude, GPT, Gemini, DeepSeek, and Ollama
- [x] Register `score_task_complexity` agent tool (expanding to 51 agent tools total)
- [x] Pass all 153 tests 100% green

---

## ✅ Phase 22: Fluid Thinking Animation & Configurable Cost Spend (v0.0.32)
- [x] Implement smooth 80ms millisecond-based animated spinner in TUI timeline
- [x] Implement clean, borderless `• Thought for {s}s` thought blocks in muted italics without emojis
- [x] Add configurable `show_cost: bool` in `UiConfig` (defaulting to disabled for a clean status bar)
- [x] Pass all 154 tests 100% green

---

## ✅ Phase 23: Codebase Modularization & Refactoring (v0.0.33)
- [x] **Wave 1: Unified Workspace File Traversal (`src/context/walker.rs`)**
  - [x] Create canonical `WorkspaceWalker` with standardized exclusions (`target/`, `.git/`, `node_modules/`, `.venv/`, `dist/`, `build/`)
  - [x] Refactor `index.rs`, `governance.rs`, `complexity.rs`, and `semantic.rs` to eliminate duplicate directory walkers
- [x] **Wave 2: UI Timeline Decomposition & Selection Extraction**
  - [x] Extract mouse drag text selection, coordinate tracking, and visual highlights into `src/ui/selection.rs` (`TimelineSelection`)
  - [x] Embed `TimelineSelection` into `src/ui/view.rs` with clean delegation
- [x] **Wave 3: Modular Domain Tool Registry (`src/tools/registry/`)**
  - [x] Partition monolithic `src/tools/mod.rs` (2,160 LOC) into 7 domain modules: `fs_tools.rs`, `exec_tools.rs`, `search_tools.rs`, `git_tools.rs`, `agent_tools.rs`, `context_tools.rs`, `web_tools.rs`
  - [x] Keep `src/tools/mod.rs` as clean facade router while preserving 100% parameter signature parity across all 51 agent tools
- [x] **Wave 4: Centralization of Remaining Heuristic Constants**
  - [x] Move task complexity risk keywords and memory decay parameters to `src/constants.rs`
- [x] **Wave 5: Verification & Architectural Benchmark**
  - [x] Run full test suite (163 tests passing, 0 clippy warnings)
  - [x] Run `cargo fmt` and verify formatting

---

## ✅ Phase 24: Interactive Timeline Checkpoint Undo Engine (`/undo`) (v0.0.34)
- [x] **Task 1: Checkpoint Metadata & Storage Engine (`src/session/backup.rs`)**
  - [x] Add `user_prompt`, `message_index`, and `working_memory_plan` to `BackupManifest`
  - [x] Implement `record_turn_start(...)` and `list_checkpoints(...)` in `BackupManager`
  - [x] Unit tests for multi-turn checkpoint discovery and metadata persistence
- [x] **Task 2: Multi-Turn Rollback Engine (`src/session/undo.rs`)**
  - [x] Implement `UndoEngine::rollback_to_checkpoint(backup_manager, target_turn_id) -> Result<UndoResult>`
  - [x] Unit tests for cascading multi-turn file restoration and newly created file deletion
- [x] **Task 3: Timeline Graph Modal UI (`src/ui/modal.rs`)**
  - [x] Add `TurnCheckpointInfo` with relative time formatting (`2m ago`, `15s ago`)
  - [x] Add `ModalState::UndoCheckpoint` rendering Style 4 Timeline Graph (`◉───○───○`)
  - [x] Visual styling with solid active nodes `◉`, hollow past nodes `○`, connecting lines `│`, and turn badges
- [x] **Task 4: App Wiring & State Truncation (`src/app.rs`)**
  - [x] Hook `/undo` command to initialize and display `ModalState::UndoCheckpoint`
  - [x] Wire `Enter` key on `UndoCheckpoint` to execute file rollback, truncate `session.messages`, and update timeline
  - [x] Wire `Up`, `Down`, and `Esc` for modal navigation and dismissal
- [x] **Task 5: Integration Tests & Quality Gates**
  - [x] Create `tests/integration_undo_checkpoint.rs`
  - [x] Pass `cargo check && cargo fmt && cargo clippy -- -D warnings && cargo test` (168 tests passing, 0 warnings)

---

## ✅ Phase 25: Interactive Live Theme Switcher Modal (`/theme`) (v0.0.35)
- [x] **Task 1: Theme Palette Extension (`src/ui/theme.rs`)**
  - [x] Implement `tokyo_night()`, `catppuccin_mocha()`, `nord_frost()`, `gruvbox_dark()`, `dracula()`, `cyberpunk_matrix()`
  - [x] Add `ThemeInfo` struct and `Theme::list_themes()`
  - [x] Update `Theme::detect` to resolve all theme aliases
- [x] **Task 2: Theme Selector Modal UI (`src/ui/modal.rs`)**
  - [x] Add `ModalState::ThemeSelect` enum variant and `new_theme_select` constructor
  - [x] Render interactive list with colored swatch glyphs `■`, descriptions, and footer hints
- [x] **Task 3: App Wiring & Persistence (`src/app.rs`)**
  - [x] Hook `/theme` and `/themes` commands to instantiate and display `ThemeSelect`
  - [x] Handle `Enter`, `Up`, `Down`, `Esc` in `handle_modal_key` with live theme switching and config save
- [x] **Task 4: Integration Tests & Quality Gates**
  - [x] Create `tests/integration_theme_switcher.rs`
  - [x] Pass `cargo check && cargo fmt && cargo clippy -- -D warnings && cargo test` (171 tests passing, 0 warnings)

---

## ✅ Phase 26: Zero-Leak Secret Redaction Proxy (v0.0.36)
- [x] **Task 1: Secret Redaction Engine (`src/sandbox/redact.rs`)**
  - [x] Implement `SecretRedactor` with `OnceLock` global lazy-init
  - [x] Add 16 regex patterns for common secret formats (OpenAI, Anthropic, GitHub, AWS, Google, Stripe, Slack, Bearer, JWT, PEM, generic assignments, connection passwords, hex secrets)
  - [x] Implement env-var harvesting from `SECRET_PATTERNS` and `BLOCKED_PREFIXES`
  - [x] Add exact-match redaction with 8-char minimum length guard
- [x] **Task 2: Central Redaction Hook (`src/agent/loop.rs`)**
  - [x] Insert `SecretRedactor::global().redact()` after tool dispatch, before all 3 output sinks
  - [x] Cover built-in tools (51), MCP tools, and auto-heal compiler output
- [x] **Task 3: Constants & Module Registration**
  - [x] Add `REDACTED_PLACEHOLDER` to `src/constants.rs`
  - [x] Register `pub mod redact` in `src/sandbox/mod.rs`
- [x] **Task 4: Integration Tests & Quality Gates**
  - [x] Create `tests/integration_secret_redaction.rs` (7 tests)
  - [x] 20 unit tests in `src/sandbox/redact.rs`
  - [x] Pass `cargo check && cargo fmt && cargo clippy -- -D warnings && cargo test` (all tests passing, 0 warnings)

---

## ✅ Phase 27: Workspace-Local Sessions & Interactive Session Browser (v0.0.37)
- [x] **Task 1: Workspace-Local Session Storage Engine (`src/session/store.rs`)**
  - [x] Implement `SessionStore::with_workspace(workspace_root)` directing sessions to `.minicode/sessions/`
  - [x] Implement `list_sessions_rich()` with token statistics, turn counts, preview snippets, and active session flag
- [x] **Task 2: `/sessions` Interactive Browser Modal (`src/ui/modal.rs`)**
  - [x] Implement `ModalState::SessionBrowser` with rich status summary and formatted date badges
  - [x] Wire `Up`, `Down`, `Enter` (session switch and hydration), and `Esc`
- [x] **Task 3: Slash Command & Autocomplete Wiring (`src/app.rs`, `src/ui/input.rs`)**
  - [x] Wire `/sessions` and `/history` commands to open `ModalState::SessionBrowser`
- [x] **Task 4: Integration Tests & Quality Gates**
  - [x] Create `tests/integration_session_browser.rs` (7 tests)
  - [x] Pass `cargo check && cargo fmt && cargo clippy -- -D warnings && cargo test`

---

## ✅ Phase 28: Composable Tool Middleware Pipeline (v0.0.38)
- [x] **Task 1: `ToolMiddleware` Trait & `ToolPipeline` (`src/tools/middleware.rs`)**
  - [x] Implement `ToolMiddleware` trait with `before()` and `after()` hooks
  - [x] Implement `TimingMiddleware` measuring precise tool execution latency
  - [x] Implement `RedactMiddleware` executing zero-leak credential masking
  - [x] Implement `CheckpointMiddleware` logging file backup manifests
- [x] **Task 2: Pipeline Integration (`src/agent/loop.rs`)**
  - [x] Replace inline redaction with `tool_pipeline.run()` in main ReAct loop
- [x] **Task 3: Integration Tests & Quality Gates**
  - [x] Create `tests/integration_tool_middleware.rs` (9 tests)
  - [x] 8 unit tests in `src/tools/middleware.rs`

---

## ✅ Phase 29: Inline Unified Diff Preview on File Edits (v0.0.39)
- [x] **Task 1: Diff Engine (`src/tools/diff.rs`)**
  - [x] Implement `compute_diff(old, new)`, `has_changes()`, `format_diff_plain()` using `similar` crate
- [x] **Task 2: `DiffMiddleware` Pipeline Stage (`src/tools/middleware.rs`)**
  - [x] Snapshot file before dispatch in `loop.rs`, attach diff with `MINICODE_DIFF_BLOCK:` prefix
- [x] **Task 3: TUI Timeline Rendering (`src/ui/view.rs`)**
  - [x] Render `+` additions in green and `-` deletions in red inside `ToolFinished` blocks
- [x] **Task 4: Integration Tests & Quality Gates**
  - [x] Create `tests/integration_inline_diff.rs` (11 tests)
  - [x] 5 unit tests in `src/tools/diff.rs`

---

## ✅ Phase 30: Dual-Mode Browser Engine Core & Multi-Browser Manager (v0.0.40)
- [x] **Task 1: Engine Types & Priorities (`src/tools/browser/engine.rs`)**
  - [x] Implement `BrowserEngine` (`Obscura`, `Firefox`, `Chrome`), `BrowserMode` (`Headless`, `Gui`)
  - [x] Implement `HEADLESS_PRIORITY` (`Obscura` $\rightarrow$ `Firefox` $\rightarrow$ `Chrome`)
  - [x] Implement `GUI_PRIORITY` (`Firefox` $\rightarrow$ `Chrome`)
- [x] **Task 2: Process Supervisor & Sandbox Profiles (`src/tools/browser/manager.rs`)**
  - [x] Binary discovery via `which` across system PATH
  - [x] Profile isolation under `.minicode/browser_profiles/<engine>_<mode>/`
  - [x] Engine argument generator with stealth, file access, and debugging ports
  - [x] Process spawning with `kill_on_drop(true)` and `process_group(0)`
  - [x] CDP readiness polling against `/json/version`
- [x] **Task 3: Page-Target CDP WebSocket Driver (`src/tools/browser/driver.rs`)**
  - [x] Lightweight async CDP client using `tokio-tungstenite`
  - [x] Page Target WebSocket discovery via `GET /json/list` / `PUT /json/new`
  - [x] Auto-dismissal of JavaScript alert dialogs (`Page.javascriptDialogOpening`)
  - [x] Implement `navigate()`, `get_document_html()`, `evaluate_js()`, `take_screenshot()`
- [x] **Task 4: Public Facade & Tool Registration (`src/tools/browser/mod.rs`, `web_tools.rs`)**
  - [x] Expose `navigate_and_snapshot(url, mode, workspace_root)`
  - [x] Permit loopback/`localhost` for dev server debugging while blocking cloud metadata
  - [x] Register `browser_navigate`, `browser_snapshot`, `browser_eval`, `browser_screenshot`
- [x] **Task 5: Integration Tests & Quality Gates**
  - [x] Create `tests/integration_browser_engine.rs` (9 tests)
  - [x] Pass `cargo check && cargo fmt && cargo clippy -- -D warnings && cargo test --test integration_browser_engine`

---

## ✅ Phase 31: Interactive Browser Automation & Live Dev-Server Debugger (v0.0.41)
- [x] **Task 1: Versioned ARIA Tree & Stale Reference Guards (`src/tools/browser/accessibility.rs`)**
  - [x] Implement `AccessibilityManager` with `@v{rev}:e{idx}` numbering
  - [x] Sort interactive elements by document position to maintain DOM source order
  - [x] Stale reference detector rejecting outdated revision actions
- [x] **Task 2: Actions with Immediate Snapshot Return (`src/tools/browser/interaction.rs`)**
  - [x] Implement `BrowserInteractor::click_element()` with instant updated snapshot
  - [x] Implement `BrowserInteractor::fill_element()` with input/change events
  - [x] Implement `BrowserInteractor::scroll_page()` with directional scroll
- [x] **Task 3: Live Dev-Server Debugger (`src/tools/browser/debug.rs`)**
  - [x] Implement `DebugCollector` recording console logs, uncaught exceptions, and HTTP 4xx/5xx errors
  - [x] Format clean Markdown diagnostic report
- [x] **Task 4: Public Facade & Tool Registration (`src/tools/browser/mod.rs`, `web_tools.rs`)**
  - [x] Expose `click_and_snapshot()`, `fill_and_snapshot()`, `scroll()`, `get_debug_logs()`
  - [x] Register `browser_click`, `browser_fill`, `browser_scroll`, `browser_debug_logs`
- [x] **Task 5: Integration Tests & Quality Gates**
  - [x] Create `tests/integration_browser_interaction.rs` (5 tests)
  - [x] Pass `cargo check && cargo fmt && cargo clippy -- -D warnings && cargo test --test integration_browser_interaction`

---

## ✅ Phase 32: Fit Markdown & Intelligent Documentation Ingestion (v0.0.42)
- [x] **Task 1: 3-Step Smart Markdown Pipeline (`src/tools/browser/markdown.rs`)**
  - [x] Implement Step 1: Content negotiation with `Accept: text/markdown`
  - [x] Implement Step 2: `llms.txt` and `llms-full.txt` hierarchy probing
  - [x] Implement Step 3: Fast HTML to Markdown via `htmd` + noise pruning (stripping scripts, nav, footers)
  - [x] Implement query-based paragraph ranking and filtering
- [x] **Task 2: Upgrade `fetch_or_browse` Tool (`src/tools/web.rs`, `web_tools.rs`)**
  - [x] Route web documentation extraction through `SmartMarkdownExtractor`
  - [x] Support optional `query` parameter for filtered search
  - [x] Preserve strict SSRF protection on all fetch requests
- [x] **Task 3: Integration Tests & Quality Gates**
  - [x] Create `tests/integration_browser_markdown.rs` (4 tests)
  - [x] Pass `cargo check && cargo fmt && cargo clippy -- -D warnings && cargo test --test integration_browser_markdown`

---

## ✅ Phase 33: Subagent Swarm Core Engine & Capability Sandboxing (v0.0.43)
- [x] **Task 1: Types & Role Presets (`src/agent/subagent/types.rs`)**
  - [x] Implement `SubagentRole` presets (`Researcher`, `CodeReviewer`, `TestEngineer`, `SecurityAuditor`, `Custom`)
  - [x] Implement role-specific capability tool whitelists, token budgets, and max turns
  - [x] Implement `SubagentState`, `SubagentConfig`, `SubagentInfo`, and `SubagentResult`
- [x] **Task 2: Scoped Worker Task & Isolation (`src/agent/subagent/worker.rs`)**
  - [x] Implement `SubagentWorker` with private message history
  - [x] Implement role-specific system prompts
  - [x] Enforce tool capability whitelisting on every turn
  - [x] Implement token counting via `tiktoken-rs` with budget enforcement
- [x] **Task 3: Supervisor Pool & Lifecycle Manager (`src/agent/subagent/pool.rs`, `mod.rs`)**
  - [x] Implement `SubagentPool` worker registry
  - [x] Implement `run_subagent`, `list_subagents`, `get_subagent`, `kill_subagent`, `kill_all`
  - [x] Implement `format_swarm_summary` Markdown generator
- [x] **Task 4: New Agent Tools & Registry Wiring (`src/tools/registry/agent_tools.rs`)**
  - [x] Register `invoke_subagent` schema and dispatch
  - [x] Register `send_message` schema and dispatch
  - [x] Register `manage_subagents` schema and dispatch
- [x] **Task 5: Integration Tests & Quality Gates**
  - [x] Create `tests/integration_subagent_swarm.rs` (6 tests)
  - [x] Pass `cargo check && cargo fmt && cargo clippy -- -D warnings && cargo test --test integration_subagent_swarm`

---

## ✅ Phase 34: Actor-Critic Dual-Agent Code Verification Loop (v0.0.44)
- [x] **Task 1: Multi-Dimensional Critic Engine (`src/agent/critic.rs`)**
  - [x] Implement compiler diagnostics inspection with structured `CriticIssue` findings
  - [x] Implement anti-pattern scanning (detecting `println!` in non-test library code)
  - [x] Implement zero-leak secret detection via `SecretRedactor` integration
  - [x] Implement structured `CriticVerdict` (`Approved`, `ApprovedWithWarnings`, `Rejected`)
- [x] **Task 2: Critic Tool & Registry Wiring (`src/tools/registry/agent_tools.rs`)**
  - [x] Update `critic_review` tool schema and dispatch with rich multi-axis Markdown report
- [x] **Task 3: Integration Tests & Quality Gates**
  - [x] Create `tests/integration_actor_critic.rs` (4 tests)
  - [x] Pass `cargo check && cargo fmt && cargo clippy -- -D warnings && cargo test --test integration_actor_critic`

---

## ✅ Phase 35: Adaptive Inline Subagent UI & Swarm Live Stream Engine (v0.0.45)
- [x] **Task 1: Inline Tree Hierarchy Model (`src/ui/view.rs`)**
  - [x] Implement `SubagentItemStatus`, `SubagentTreeItem`, `SubagentTreeBlock`, `SwarmMatrixBlock`
  - [x] Add `TimelineEntry::SubagentTree` and `TimelineEntry::SubagentSwarm` variants
  - [x] Implement `add_subagent_tree`, `update_subagent_tree_item`, `complete_subagent_tree`, `add_subagent_swarm`, `toggle_subagent_swarm`
- [x] **Task 2: Custom Role Theme & Palettes (`src/ui/theme.rs`)**
  - [x] Implement `Theme::role_accent_color(role)` mapping Researcher, CodeReviewer, TestEngineer, SecurityAuditor, Custom
  - [x] Filled ` Task ` badge styling and tree branch connectors (`├─── `, `╰─── `)
- [x] **Task 3: Statusline Swarm Indicator (`src/ui/status.rs`)**
  - [x] Implement `swarm:N workers` in the bottom statusbar when background subagents exist
- [x] **Task 4: Integration Tests & Quality Gates**
  - [x] Create `tests/integration_subagent_ui.rs` (3 tests)
  - [x] Pass `cargo check && cargo fmt && cargo clippy -- -D warnings && cargo test --test integration_subagent_ui`

---

## ✅ Phase 36: Task DAG & Dynamic Dependency Graph Engine (v0.0.46)
- [x] **Task 1: Wave Calculation Engine (`src/agent/task_dag.rs`)**
  - [x] Implement `TaskDag::calculate_execution_waves` using Petgraph DiGraphs
  - [x] Implement `TaskDag::save` and `TaskDag::load` for workspace persistence
- [x] **Task 2: Dynamic Task Splitting (`src/agent/task_dag.rs`)**
  - [x] Implement `TaskDag::split_task` preserving upstream & downstream dependencies
- [x] **Task 3: Agent Tools & Schemas (`src/tools/registry/agent_tools.rs`)**
  - [x] Register `schedule_task_waves` and `split_task` schemas & dispatch
- [x] **Task 4: Integration Tests & Quality Gates**
  - [x] Create `tests/integration_task_dag_waves.rs` (3 tests)
  - [x] Pass `cargo check && cargo fmt && cargo clippy -- -D warnings && cargo test --test integration_task_dag_waves`

---

## ✅ Phase 37: RTK-Style Token Output Compactor & Multi-Language Diagnostic Distiller (v0.0.47)
- [x] **Task 1: Multi-Language Distillers (`src/tools/compactor.rs`)**
  - [x] Implement Pytest & Unittest compaction
  - [x] Implement Go test pass & failure traces compaction
  - [x] Enhance Rust compiler error diagnostic extraction
- [x] **Task 2: Compaction Metrics & Token Savings Tracker (`src/tools/compactor.rs`)**
  - [x] Implement `calculate_compaction_stats` (measuring raw vs compact byte reduction)
- [x] **Task 3: Integration Tests & Quality Gates**
  - [x] Create `tests/integration_token_compactor.rs` (4 tests)
  - [x] Pass `cargo check && cargo fmt && cargo clippy -- -D warnings && cargo test --test integration_token_compactor`

---

## ✅ Phase 38: Episodic Vector Memory & Long-Term Recall Engine (v0.0.48)
- [x] **Task 1: Episodic Memory Core & Embedder (`src/context/episodic.rs`)**
  - [x] Implement `EpisodicItem` and `EpisodicMemory`
  - [x] Dense vector embedding via `SemanticIndex::embed`
- [x] **Task 2: Hybrid Search & Persistence (`src/context/episodic.rs`)**
  - [x] Implement hybrid cosine similarity (0.7) + keyword token overlap (0.3)
  - [x] Implement `.minicode/episodic_memory.json` save / load
- [x] **Task 3: Agent Tools & Schemas (`src/tools/registry/agent_tools.rs`)**
  - [x] Register `record_episode` and `recall_episodes` schemas & dispatch
- [x] **Task 4: Integration Tests & Quality Gates**
  - [x] Create `tests/integration_episodic_memory.rs` (3 tests)
  - [x] Pass `cargo check && cargo fmt && cargo clippy -- -D warnings && cargo test --test integration_episodic_memory`

---

## ✅ Phase 39: Speculative Multi-Branch Hypothesis Auto-Pruner & Parallel Evaluator (v0.0.49)
- [x] **Task 1: Parallel Branch Evaluator (`src/agent/hypothesis.rs`)**
  - [x] Implement `HypothesisEngine::evaluate_all_branches`
- [x] **Task 2: Auto-Pruning & Comparison Matrix (`src/agent/hypothesis.rs`)**
  - [x] Implement `prune_failed_branches` (worktree cleanup below min_fitness)
  - [x] Implement `format_comparison_matrix` table formatter
- [x] **Task 3: Agent Tools & Schemas (`src/tools/registry/agent_tools.rs`)**
  - [x] Register `evaluate_all_branches`, `prune_branches`, `compare_branches`
- [x] **Task 4: Integration Tests & Quality Gates**
  - [x] Create `tests/integration_hypothesis_pruner.rs` (2 tests)
  - [x] Pass `cargo check && cargo fmt && cargo clippy -- -D warnings && cargo test --test integration_hypothesis_pruner`

---

## ✅ Phase 40: Milestone v0.0.50 — Resilient Stream Re-Connection & Network Circuit Breaker (v0.0.50)
- [x] **Task 1: Network Circuit Breaker (`src/agent/circuit_breaker.rs`)**
  - [x] Implement `CircuitBreaker`, `CircuitState` (`Closed`, `Open`, `HalfOpen`)
  - [x] Implement cooldown timeout and canary probe recovery
- [x] **Task 2: Retry Policy with Exponential Backoff (`src/agent/circuit_breaker.rs`)**
  - [x] Implement `RetryPolicy` with transient error classification (429, 502/503/504, network reset)
- [x] **Task 3: Resilient Provider Wrapper (`src/agent/provider.rs`)**
  - [x] Implement `ResilientProvider<P>` decorator for LLM stream completion
- [x] **Task 4: Integration Tests & Quality Gates**
  - [x] Create `tests/integration_resilient_network.rs` (3 tests)
  - [x] Pass `cargo check && cargo fmt && cargo clippy -- -D warnings && cargo test --test integration_resilient_network`

---

## ✅ Phase 41: Semantic AST Code-Chunk Semantic Embedder & Symbol Indexer (v0.0.51)
- [x] **Task 1: AST-Aware Symbol Chunking (`src/context/semantic.rs`)**
  - [x] Implement `chunk_source_code_ast` with Tree-sitter symbol boundaries
  - [x] Implement symbol signature boost during vector embedding
- [x] **Task 2: Semantic Symbol Search Engine (`src/context/semantic.rs`)**
  - [x] Implement `SemanticIndex::search_symbols`
- [x] **Task 3: Search Tool & Schemas (`src/tools/registry/search_tools.rs`)**
  - [x] Register `search_symbols_semantic` tool schema and dispatch
- [x] **Task 4: Integration Tests & Quality Gates**
  - [x] Create `tests/integration_ast_semantic_indexer.rs` (2 tests)
  - [x] Pass `cargo check && cargo fmt && cargo clippy -- -D warnings && cargo test --test integration_ast_semantic_indexer`

---

## ✅ Phase 43: Subagent Shared Scratchpad & Inter-Worker Messaging Bus (v0.0.52)
- [x] **Task 1: Shared Scratchpad Blackboard (`src/agent/subagent/scratchpad.rs`)**
  - [x] Implement `SharedScratchpad` and `ScratchpadEntry` with thread-safe CRUD
  - [x] Implement `.minicode/scratchpad.json` disk persistence
- [x] **Task 2: Inter-Worker Messaging Bus (`src/agent/subagent/scratchpad.rs`)**
  - [x] Implement `WorkerMessageBus` supporting direct and broadcast pub/sub
- [x] **Task 3: Agent Tools & Schemas (`src/tools/registry/agent_tools.rs`)**
  - [x] Register `scratchpad_write`, `scratchpad_read`, `scratchpad_list`
  - [x] Register `send_worker_message`, `read_worker_messages`
- [x] **Task 4: Integration Tests & Quality Gates**
  - [x] Create `tests/integration_scratchpad_bus.rs` (3 tests)
  - [x] Pass `cargo check && cargo fmt && cargo clippy -- -D warnings && cargo test --test integration_scratchpad_bus`

---

## ✅ Phase 44: Deep Recursive Web Crawler & Documentation Ingestion Engine (v0.0.53)
- [x] **Task 1: Core Crawler Engine & Types (`src/tools/crawler/`)**
  - [x] Implement `CrawledPage`, `CrawlReport`, and `CrawlerConfig` in `src/tools/crawler/types.rs`
  - [x] Implement BFS queue with domain and path-prefix boundary restrictions in `src/tools/crawler/engine.rs`
  - [x] Implement `tokio::sync::Semaphore` bounded concurrency (4 simultaneous fetches)
- [x] **Task 2: Sitemap & llms.txt Discovery (`src/tools/crawler/sitemap.rs`)**
  - [x] Auto-discover `/sitemap.xml`, `/sitemap_index.xml`, and `/llms.txt` / `/llms-full.txt`
  - [x] Parse XML route URLs and seed the BFS crawler frontier
- [x] **Task 3: Fit-Markdown Boilerplate Distiller (`src/tools/crawler/markdown.rs`)**
  - [x] Extract primary semantic content (`main`, `article`, `.markdown-body`, `.content`)
  - [x] Strip boilerplate (`<nav>`, `<header>`, `<footer>`, `<aside>`, ads, cookie modals)
  - [x] Convert clean HTML to GitHub Flavored Markdown (GFM)
- [x] **Task 4: Tool Schemas & Dispatch (`src/tools/registry/web_tools.rs`)**
  - [x] Register `crawl_documentation`, `crawl_sitemap`, `search_crawled_docs`
  - [x] Implement disk persistence in `.minicode/crawled/`
- [x] **Task 5: Integration Tests & Quality Gates**
  - [x] Create `tests/integration_doc_crawler.rs` (4 tests)
  - [x] Pass `cargo check && cargo fmt && cargo clippy -- -D warnings && cargo test --test integration_doc_crawler`

---

## ✅ Phase 45: Native GitHub Integration & CI Workflow Diagnoser (v0.0.54)
- [x] **Task 1: GitHub Client & Auth Discovery (`src/tools/github/client.rs`)**
  - [x] Detect `gh` CLI credentials (`gh auth status`) with JSON subprocess fallback
  - [x] Implement REST API client using `GITHUB_TOKEN` / `GH_TOKEN` env vars
  - [x] Auto-detect repository name and owner from git remote origin
- [x] **Task 2: Issue & PR Operations (`src/tools/github/mod.rs`)**
  - [x] Implement `github_issue_view`, `github_issue_list`, `github_issue_create`
  - [x] Implement `github_pr_view`, `github_pr_diff`, `github_pr_create`, `github_pr_review_comments`
- [x] **Task 3: GitHub Actions CI Diagnoser (`src/tools/github/mod.rs`)**
  - [x] Implement `github_ci_status` (workflow run states: success, failure, running)
  - [x] Implement `github_ci_logs` (fetch and compact failing job/step error logs)
- [x] **Task 4: Tool Schemas & Dispatch (`src/tools/registry/git_tools.rs`)**
  - [x] Register all GitHub schemas and dispatch handlers
- [x] **Task 5: Integration Tests & Quality Gates**
  - [x] Create `tests/integration_github_tools.rs` (3 tests)
  - [x] Pass `cargo check && cargo fmt && cargo clippy -- -D warnings && cargo test --test integration_github_tools`

---

## ✅ Phase 46: Embedded Native onpkg Template Engine & Autonomous Spec Syncer (v0.0.56)
- [x] **Task 1: 100% Embedded Pure-Rust Stack Templates (`src/tools/onpkg/templates/builtin/`, `src/tools/onpkg/stacks.rs`)**
  - [x] Embed built-in stacks in memory: React Vite, React Vite Full, React Vite GSAP, Next.js 16, FastAPI, Flutter Riverpod, Hono API, Hono Full, MERN, PERN
  - [x] Implement `OnpkgScaffolder` with zero external binary dependencies
- [x] **Task 2: Native Multi-Runtime Scaffolder & Auto-Installer (`src/tools/onpkg/scaffolder.rs`)**
  - [x] Scaffolding with atomic directory and file creation
  - [x] Automatic generation of `onpkg.json`, `AGENTS.md`, and `onpkg_docs/` (`prd.md`, `design.md`, `implementation.md`, `todo.md`)
  - [x] Multi-runtime post-scaffold auto-installer (`bun install`, `uv sync`, `cargo check`, `flutter pub get`)
- [x] **Task 3: Autonomous Spec Syncer, Skills Manager & Diagnostics (`src/tools/onpkg/`)**
  - [x] Implement `OnpkgSyncEngine` in `src/tools/onpkg/sync.rs`
  - [x] Implement `OnpkgDoctor` multi-runtime environment health checks in `src/tools/onpkg/doctor.rs`
  - [x] Implement `OnpkgSkillsManager` in `src/tools/onpkg/skills.rs`
- [x] **Task 4: Tool Schemas & Registry Integration (`src/tools/registry/onpkg_tools.rs`)**
  - [x] Register `onpkg_stack_list`, `onpkg_stack_show`, `onpkg_stack_add`
  - [x] Register `onpkg_skill_list`, `onpkg_skill_install`
  - [x] Register `onpkg_sync`, `onpkg_doctor`
  - [x] Integrate into `ToolRegistry::get_tool_schemas()` and `dispatch_tool()` in `src/tools/mod.rs`
- [x] **Task 5: Integration Tests & Quality Gates**
  - [x] Create comprehensive `tests/integration_onpkg_tools.rs` (5 tests)
  - [x] Pass `cargo check && cargo fmt && cargo clippy -- -D warnings && cargo test --test integration_onpkg_tools`

---

## ✅ Phase 47: Interactive TUI Stack Wizard & Autonomous Goal/Plan Commands (v0.0.57)
- [x] **Task 1: Interactive TUI Stack Wizard (`src/ui/modal.rs`)**
  - [x] Implement `ModalState::StackSelect` with 2-column live stack browser and file tree preview
  - [x] Implement real-time fuzzy filtering by stack name, runtime, and technology tags
  - [x] Native asynchronous background scaffolding on `Enter`
- [x] **Task 2: Slash Commands Registry & Autocomplete (`src/ui/input.rs`)**
  - [x] Register `/stack`, `/plan`, `/goal` in `SLASH_COMMANDS`
  - [x] Add instant recommendations and tab-completion in `InputDock`
- [x] **Task 3: Autonomous Goal & Planning Dispatch (`src/app.rs`)**
  - [x] Handle `/plan` command with structured milestone breakdown in `onpkg_docs/todo.md`
  - [x] Handle `/goal` command with autonomous self-directed execution loop
- [x] **Task 4: Autonomous Intent Protocols in Agent Prompt (`src/agent/prompt.rs`)**
  - [x] Update `DEFAULT_SYSTEM_PROMPT` for autonomous natural language mapping
- [x] **Task 5: Integration Tests & Release Verification**
  - [x] Add unit and integration tests in `tests/integration_onpkg_tools.rs` (8 tests)
  - [x] Pass `cargo check && cargo fmt && cargo clippy -- -D warnings && cargo test`

---

## ✅ Phase 48: Interactive TUI Git Diff Viewer & Multi-Agent Code Reviewer (v0.0.58)
- [x] **Task 1: Git Diff Parser & Data Models (`src/git/diff_viewer.rs`)**
  - [x] Implement `GitDiffViewer` and `GitDiffFile` / `GitDiffLine` models
  - [x] Parse additions, deletions, line numbers, and hunk headers
  - [x] Support staged (`--cached`) and unstaged working tree diffs
- [x] **Task 2: Multi-Agent Adversarial Code Reviewer (`src/git/reviewer.rs`, `src/tools/registry/git_tools.rs`)**
  - [x] Implement `GitReviewer` evaluating security, correctness, architecture, and tests
  - [x] Register `git_review` tool in `ToolRegistry`
  - [x] Register `/review` slash command in `InputDock` and `src/app.rs`
- [x] **Task 3: Interactive TUI Git Diff Modal (`src/ui/modal.rs`, `src/app.rs`)**
  - [x] Implement `ModalState::GitDiff` with 2-column layout, line numbers, and syntax styling
  - [x] Support <kbd>Tab</kbd> toggle between staged/unstaged views
  - [x] Support <kbd>s</kbd> file staging/unstaging and <kbd>r</kbd> direct code review
  - [x] Bind `Ctrl+D` shortcut and `/diff` command
- [x] **Task 4: Integration Tests & Quality Gates**
  - [x] Create comprehensive `tests/integration_git_diff_review.rs` (5 tests)
  - [x] 23/23 tests passing across all 5 integration test suites
  - [x] 0 clippy warnings (`cargo clippy -- -D warnings`)

---

## ✅ Phase 49: Dual-Mode CLI Subcommands for Humans & AI Agents (v0.0.59)
- [x] **Task 1: First-Class CLI Subcommands (`src/main.rs`)**
  - [x] Implement `minicode stack [list | show | add]` with `--json` option
  - [x] Implement `minicode diff [--staged] [--json]`
  - [x] Implement `minicode review [--staged] [--json]`
  - [x] Implement `minicode doctor`, `minicode sync`, `minicode plan [prompt]`
- [x] **Task 2: Machine-Readable Serde Interoperability (`src/git/reviewer.rs`, `src/git/diff_viewer.rs`)**
  - [x] Derive `serde::Serialize` and `serde::Deserialize` on `GitDiffFile`, `GitDiffLine`, `ReviewReport`, `ReviewFinding`
- [x] **Task 3: MCP Server & Headless Streaming Compatibility (`src/mcp/server.rs`)**
  - [x] Verify MCP stdio JSON-RPC server exposes all tools
- [x] **Task 4: Verification & Release**
  - [x] 23/23 integration tests passing across all 5 suites
  - [x] 0 clippy warnings

---

## ✅ Phase 50: Interactive Visual Session History & Time-Travel Explorer (v0.0.60)
- [x] **Task 1: SessionStore Analytical & Manipulation Engine (`src/session/store.rs`)**
  - [x] Implement `SessionSummary` with model, turns, events, tokens, duration, tool call map, files touched, and conversation excerpts
  - [x] Implement `get_session_summary(&self, session_id: &str)`
  - [x] Implement `fork_session(&self, source_id: &str, workspace: &Path)` for time-travel branching
  - [x] Implement `export_markdown(&self, session_id: &str, output_path: &Path)`
  - [x] Implement `delete_session(&self, session_id: &str)`
- [x] **Task 2: 2-Column Visual History & Analytical Inspector (`src/ui/modal.rs`, `src/app.rs`)**
  - [x] Implement 2-column split `ModalState::SessionBrowser` (36% session list, 64% analytical preview)
  - [x] Render live preview with model, turns, token metrics, duration, tool distribution, and touched files
  - [x] Wire hotkeys: <kbd>Enter</kbd> Load, <kbd>f</kbd> Fork, <kbd>e</kbd> Export MD, <kbd>d</kbd> Delete, <kbd>j</kbd>/<kbd>k</kbd> Scroll
  - [x] Bind <kbd>Ctrl+H</kbd> and register `/history` & `/export` in `src/ui/input.rs`
- [x] **Task 3: Dual-Mode CLI Subcommands (`src/main.rs`)**
  - [x] Implement `minicode history [--json]`
  - [x] Implement `minicode export [session_id] [-o output.md]`
- [x] **Task 4: Integration Tests & Quality Gates (`tests/integration_session_history.rs`)**
  - [x] 4 new integration tests covering summaries, forking isolation, markdown exports, and deletion
  - [x] 27/27 tests passing across all 6 test suites
  - [x] 0 clippy warnings (`cargo clippy -- -D warnings`)

---

## ✅ Phase 51: Session History Hardening & Quality Fixes (v0.0.61)
> Fixes all 20 issues from the Phase 50 code review. Informed by research on Codex CLI, Claude Code, Aider, Cline, and Zed AI session architectures.

- [x] **Task 1: UTF-8 Safe Truncation (`src/session/store.rs`, `src/ui/modal.rs`)**
  - [x] Add `truncate_safe(s, max_bytes, suffix)` helper using `str::floor_char_boundary()`
  - [x] Fix `export_markdown()` — replace `&output[..1000]` with safe truncation (prevents panic on non-ASCII)
  - [x] Fix `modal.rs` — replace `&summary.last_response[..180]` with safe truncation
  - [x] Fix `modal.rs` — replace `&s.id[..18]` with safe truncation
  - [x] Fix `store.rs` — replace `&hash[..7.min(hash.len())]` with safe truncation
- [x] **Task 2: Session Store Hardening (`src/session/store.rs`, `src/error.rs`)**
  - [x] Add `InvalidId` variant to `SessionError` enum
  - [x] Add `validate_session_id()` — reject `session_id` containing `/`, `\`, `..`, or null bytes
  - [x] Fix TOCTOU race in `delete_session()` — use `remove_file()` + match `NotFound` pattern
  - [x] Optimize `fork_session()` — use `BufWriter` for batch write with single `sync_all()`
  - [x] Wire `validate_session_id()` into `load_session()`, `delete_session()`, `get_session_summary()`, `export_markdown()`, `fork_session()`
- [x] **Task 3: Persist StreamDelta Events (`src/agent/loop.rs`, `src/session/store.rs`)**
  - [x] Persist `AgentEvent::StreamDelta` to session JSONL in the agent loop (like ToolCall already is)
  - [x] Fix `first_prompt` — extract from first StreamDelta content instead of metadata header
  - [x] Fix `list_sessions_rich()` serde tag mismatch — use `val.get("event")` not `val.get("TurnStart")`
- [x] **Task 4: Slash Command & CLI Fixes (`src/app.rs`)**
  - [x] Fix `/export` fallback — show "no sessions" message instead of using `"current"` as session ID
  - [x] Fix `/export` prefix overreach — use `prompt == "/export" || prompt.starts_with("/export ")`
  - [x] Fix `/load` store mismatch — use `SessionStore::with_workspace()` instead of `SessionStore::new()`
- [x] **Task 5: SessionBrowser UX Improvements (`src/ui/modal.rs`, `src/app.rs`)**
  - [x] Add viewport scrolling to SessionBrowser using `scroll_offset` (like ThemeSelect/UndoCheckpoint modals)
  - [x] Show error status on Ctrl+H instead of silent `unwrap_or_default()`
  - [x] Add error feedback on delete failure — show message for `Ok(false)` and `Err(e)` cases
  - [x] Use `sessions.remove(*selected_index)` instead of re-listing from disk on delete
- [x] **Task 6: Performance & Label Fixes (`src/session/store.rs`)**
  - [x] Add `get_session_summary_with_events()` to avoid triple file read in `export_markdown()`
  - [x] Label `total_duration_ms` as "Tool Execution Time" instead of "Duration" in export and UI
- [x] **Task 7: Expanded Test Coverage (`tests/integration_session_history.rs`)**
  - [x] `test_utf8_safe_truncation_and_markdown_export` — emoji/CJK content in StreamDelta, verify no panic
  - [x] `test_session_summary_analytics` — multi-event analytics, tool counts, touched files
  - [x] `test_corrupted_jsonl_resilience` — invalid JSON lines, verify graceful skip
  - [x] `test_empty_session_summary` — session with no events, verify sane defaults
  - [x] `test_path_traversal_rejection` — `"../etc/passwd"` as session_id, verify error
  - [x] `test_delete_nonexistent_session` — verify returns `Ok(false)`
  - [x] `test_session_forking_and_isolation` — fork session and verify isolation
  - [x] `test_session_export_markdown_transcript` — markdown export verification

---

## ✅ Phase 52: Session I/O Performance, Event Schema & Quality Polish (v0.0.62)

**Research:** Codex CLI, Aider, Cline, Claude Code all batch writes at turn boundaries — no project calls `fsync` per streaming token. Every major agent records user prompts as first-class events.

### Wave 1 — Quick Fixes (P0)
- [x] **Task 1:** Remove `sync_all()` from `append_event()` in `src/session/store.rs` — eliminates ~200 fsync syscalls per turn, 200x I/O speedup
- [x] **Task 2:** Fix `/save` prefix overreach in `src/app.rs` — change `starts_with("/save")` to `== "/save" || starts_with("/save ")`
- [x] **Task 3:** Upgrade StreamDelta persistence log from `debug!` to `warn!` in `src/agent/loop.rs`

### Wave 2 — Core Improvements (P0/P1)
- [x] **Task 4:** Add `AgentEvent::UserPrompt { turn_id, timestamp, prompt }` variant to `src/agent/types.rs`
  - [x] Persist `UserPrompt` event before `TurnStart` in `src/agent/loop.rs`
  - [x] Extract `first_prompt` from `UserPrompt` in `get_session_summary_with_events()`
  - [x] Add `"### 👤 User"` section in `export_markdown()`
  - [x] Prioritize `user_prompt` event in `list_sessions_rich()` preview extraction
  - [x] Add integration test `test_user_prompt_in_summary_and_export`
- [x] **Task 5:** Eliminate double file read — create `load_session_with_metadata()` that returns `(Option<SessionMetadata>, Vec<AgentEvent>)` in a single pass

### Wave 3 — Polish (P2)
- [x] **Task 6:** Add `truncate_display()` using `unicode-width` for TUI column-aware truncation
  - [x] Add `unicode-width = "0.2"` explicit dep in `Cargo.toml`
  - [x] Create `truncate_display(s, max_cols, suffix)` function
  - [x] Use in `modal.rs` for session ID and response snippet rendering
  - [x] Add unit test for CJK/emoji display width truncation
- [x] **Task 7:** Extract magic numbers into `src/constants.rs` `// === Session & History ===` section
  - [x] `SESSION_PREVIEW_MAX_BYTES`, `SESSION_FIRST_PROMPT_MAX_BYTES`, `SESSION_TOOL_OUTPUT_MAX_BYTES`
  - [x] `SESSION_ID_DISPLAY_COLS`, `SESSION_RESPONSE_SNIPPET_COLS`, `SESSION_MAX_FILES_PREVIEW`
  - [x] `GIT_SHORT_HASH_BYTES`, `SESSION_LIST_ITEM_HEIGHT`, `SESSION_LOAD_LIST_MAX`, `SESSION_DEFAULT_MODEL`
  - [x] Replace all magic numbers in `store.rs` and `modal.rs`

### Verification
- [x] `cargo check` — exit 0
- [x] `cargo clippy -- -D warnings` — 0 warnings
- [x] `cargo fmt --check` — clean
- [x] `cargo test -j 1` — passed cleanly
- [x] `cargo build --release -j 1` — exit 0
- [x] `./localupdate.sh` — `minicode --version` → `v0.0.62`
- [x] `git commit + tag v0.0.62 + push origin main --tags`

---

## ✅ Phase 53: Autonomous Intent Routing & Interactive Command Catalog (v0.0.63)

### Wave 1 — Intent Classification Engine & Protocol (P0)
- [x] **Task 1:** Create `src/agent/intent.rs` — `AgentIntent` enum with 10 categories, regex/keyword heuristics, `IntentMatch` struct
- [x] **Task 2:** Update `DEFAULT_SYSTEM_PROMPT` in `src/agent/prompt.rs` with domain-specific autonomous execution protocols

### Wave 2 — Interactive Command Catalog Modal (P1)
- [x] **Task 3:** Implement `ModalState::CommandCatalog` in `src/ui/modal.rs` — 2-column searchable command browser with keybinding shortcuts
- [x] **Task 4:** Wire `/commands` into `src/ui/input.rs` and `src/app.rs` with TUI intent detection indicator

### Wave 3 — Dual-Mode NDJSON-RPC & Integration Tests (P1/P2)
- [x] **Task 5:** Add RPC commands (`ExecuteCommand`, `ListCommands`) and `AgentEvent::IntentRouted` to `src/agent/types.rs`
- [x] **Task 6:** Wire machine-readable command handler into `src/main.rs` (`--json-stream`)
- [x] **Task 7:** Create `tests/integration_intent_routing.rs` covering intent classification, catalog listing, and NDJSON-RPC commands

### Verification
- [x] `cargo check -j 1` — exit 0
- [x] `cargo clippy -- -D warnings` — 0 warnings
- [x] `cargo fmt --check` — clean
- [x] `cargo test -j 1` — all 10 tests pass
- [x] `cargo build --release -j 1` — exit 0
- [x] `./localupdate.sh` — `minicode --version` → `v0.0.63`
- [x] `git commit + tag v0.0.63 + push origin main --tags`

---

## ✅ Phase 54: CodeGraph Surgical Context, Architectural Layers & OKF v0.2 Knowledge System (v0.0.64)

### Wave 1 — Dense `code_explore` Surgical Context Engine (P0)
- [x] **Task 1:** Create `src/context/explorer.rs` (`CodeExploreEngine`) with symbol extraction, caller/callee traversal, and blast radius
- [x] **Task 2:** Create `src/tools/registry/explore_tools.rs` for `code_explore` and wire dispatch in `src/tools/mod.rs`

### Wave 2 — Architectural Layering & `diff_impact` Analysis (P1)
- [x] **Task 3:** Create `src/context/layers.rs` (`LayerClassifier`) for `Ui`, `Api`, `Service`, `Data`, `Utility` tagging
- [x] **Task 4:** Implement `diff_impact` tool to compute blast radius from uncommitted git diffs

### Wave 3 — OKF v0.2 Knowledge System & Ledgers (P1)
- [x] **Task 5:** Create `src/context/okf.rs` for OKF v0.2 YAML frontmatter parsing, `index.md` TOC, and `log.md` ledger
- [x] **Task 6:** Update `src/tools/onpkg/sync.rs` to validate and synchronize OKF v0.2 manifests

### Wave 4 — Interactive TUI Code Explorer Modal & Commands (P2)
- [x] **Task 7:** Implement `ModalState::CodeExplorer` in `src/ui/modal.rs` with 2-column layer & call graph browser
- [x] **Task 8:** Register `/explore` in `src/ui/input.rs`, `COMMAND_CATALOG_ITEMS`, `src/app.rs`, and `AgentIntent::CodeExplore`

### Wave 5 — Verification, Tests & Release (P0)
- [x] **Task 9:** Create `tests/integration_code_explore.rs` test suite
- [x] **Task 10:** Run `cargo check -j 1`, `cargo clippy -- -D warnings -j 1`, `cargo fmt --check`, and targeted integration tests
- [x] **Task 11:** Bump version to `0.0.64` in `Cargo.toml`, update `CHANGELOG.md`, build release (`cargo build --release -j 1`), run `./localupdate.sh`, commit, tag `v0.0.64`, and push to `origin main --tags`

---

## ✅ Phase 55: Smart Context Window Auto-Compaction Engine & Memory Anchors (v0.0.65)

### Wave 1 — Core Auto-Compaction & Turn Summarizer (P0)
- [x] **Task 1:** Create `src/context/auto_compact.rs` (`AutoCompactor`, `TurnSummary`, `MemoryAnchor`, `CompactionMetrics`)
- [x] **Task 2:** Add adaptive model-aware thresholds & ratio constants in `src/constants.rs` and `src/agent/models.rs`
- [x] **Task 3:** Expose `auto_compact` module in `src/context/mod.rs`

### Wave 2 — Prompt & Event Integration (P0)
- [x] **Task 4:** Add `AgentEvent::ContextCompacted` in `src/agent/types.rs`
- [x] **Task 5:** Support `MemoryAnchor` injection in `PromptBuilder` (`src/agent/prompt.rs`)
- [x] **Task 6:** Wire `AutoCompactor` into `AgentLoop::prune_context()` and `execute_turn()` in `src/agent/loop.rs`

### Wave 3 — UI Timeline & Slash Command Enhancements (P1)
- [x] **Task 7:** Implement `TimelineEntry::ContextCompaction` rendering in `src/ui/view.rs` and `src/app.rs`
- [x] **Task 8:** Enhance `/compact` slash command with visual model limits & progressive tier status in `src/app.rs`
- [x] **Task 9:** Add `ContextCompacted` event consumer rendering in `src/main.rs`

### Wave 4 — Integration Tests, Release & Quality Gates (P0)
- [x] **Task 10:** Create comprehensive integration test suite in `tests/integration_auto_compact.rs` (7 tests passing)
- [x] **Task 11:** Verify formatting with `cargo fmt --check` and clean linting with `cargo clippy -j 1 -- -D warnings`
- [x] **Task 12:** Bump version to `0.0.65` in `Cargo.toml`, update `CHANGELOG.md`, build release (`cargo build --release -j 1`), run `./localupdate.sh`, commit, tag `v0.0.65`, and push to `origin main --tags`

---

## ✅ Phase 56: Code Health, Panic Hardening & Deterministic Eviction (v0.0.66)

### Wave 1 — Critical: Zero-Panic Guarantee & Rule Compliance (P0)
- [x] **Task 1:** Replace `.expect("fallback compactor")` in `src/agent/loop.rs` with `AutoCompactor::default_safe()`
- [x] **Task 2:** Replace 4× `.unwrap()` in `src/tools/web_search.rs` with safe `?` error propagation and `ToolError::CommandExec`
- [x] **Task 3:** Replace `.unwrap()` on `child_ids.last()` in `src/agent/task_dag.rs` with safe `Option` matching
- [x] **Task 4:** Replace unsafe byte-index string slicing in `src/app.rs` with safe `.chars().take(25)` UTF-8 truncation
- [x] **Task 5:** Replace `.expect()` regex in `src/tools/rtk_filter.rs` with infallible `OnceLock<Option<Regex>>`

### Wave 2 — Important: Bugs & Error Handling (P0)
- [x] **Task 6:** Switch `MemoryAnchor.file_state` to `IndexMap` with `shift_remove_index(0)` for deterministic FIFO eviction
- [x] **Task 7:** Wrap retry backoff sleeps in `tokio::select!` with `cancel_token.cancelled()` in `src/agent/loop.rs`
- [x] **Task 8:** Check and log errors for `create_dir_all` in `src/app.rs` and `src/main.rs`
- [x] **Task 9:** Bound inline diff file snapshot size with `MAX_DIFF_SNAPSHOT_BYTES` in `src/agent/loop.rs`

### Wave 3 — Performance & Optimization (P1)
- [x] **Task 10:** Pre-allocate and reuse PageRank score buffers in `src/context/graph.rs`
- [x] **Task 11:** Replace per-keystroke `.to_lowercase()` allocations in `src/ui/modal.rs` with zero-allocation `contains_ci`
- [x] **Task 12:** Update tool registry capacity to `TOTAL_TOOL_COUNT` (94) in `src/tools/mod.rs`

### Wave 4 — Constants Extraction & Polish (P1)
- [x] **Task 13:** Extract 15+ hardcoded heuristics to named constants in `src/constants.rs`
- [x] **Task 14:** Replace magic numbers across `auto_compact.rs`, `compressor.rs`, `dedup.rs`, `explorer.rs`, `view.rs`, `app.rs`
- [x] **Task 15:** Make free model pricing detection robust with float parsing and fix `/review` flag matching

### Wave 5 — Verification, Release & Ship (P0)
- [x] **Task 16:** Run targeted integration test suites (`integration_auto_compact`, `integration_context_dedup`, `integration_token_compactor`, `integration_code_explore`) with `-j 1`
- [x] **Task 17:** Run `cargo fmt --check` and `cargo clippy -j 1 -- -D warnings` (0 errors, 0 warnings)
- [x] **Task 18:** Bump version to `0.0.66` in `Cargo.toml`, update `CHANGELOG.md` and `onpkg_docs/todo.md`
- [x] **Task 19:** Build release (`cargo build --release -j 1`), run `./localupdate.sh`, commit, tag `v0.0.66`, and push to `origin main --tags`

---

## ✅ Phase 57: Unicode Safety, Timeout Standardization & Hardening Polish (v0.0.67)

### Wave 1 — Unicode Slicing Safety (P0)
- [x] **Task 1:** Update `src/tools/onpkg/doctor.rs` with safe `.chars().take(40)` UTF-8 truncation
- [x] **Task 2:** Update `src/ui/modal.rs` with safe `.chars().take(52)` UTF-8 truncation for checkpoint prompts

### Wave 2 — Timeout Standardization & Idiom Polish (P1)
- [x] **Task 3:** Standardize HTTP timeouts across `web_search.rs`, `crawler/engine.rs`, and `web_tools.rs` to `WEB_TIMEOUT_SECS`
- [x] **Task 4:** Replace `Duration::from_secs(0)` with `Duration::ZERO` in `decay.rs` and `wiki.rs`

### Wave 3 — Verification, Release & Ship (P0)
- [x] **Task 5:** Verify with `cargo fmt --check` and `cargo clippy -j 1 -- -D warnings` (0 warnings)
- [x] **Task 6:** Run targeted single-job integration test suites (`integration_auto_compact`, `integration_code_explore`)
- [x] **Task 7:** Bump version to `0.0.67` in `Cargo.toml`, update `CHANGELOG.md` and `onpkg_docs/todo.md`
- [x] **Task 8:** Build release (`cargo build --release -j 1`), run `./localupdate.sh`, commit, tag `v0.0.67`, and push to `origin main --tags`

---

## 🚀 Code Intelligence Upgrade Roadmap (Phases 58–63)

> **Research basis:** 9 open-source code graph repositories analyzed (suatkocar/codegraph, petgraph, tree-sitter-graph, codegraph-rust, codebase-memory-mcp, Understand-Anything, TencentDB-Agent-Memory, code-review-graph, colbymchenry/codegraph)

### Phase 58: Symbol-Level Graph + Incremental Updates (v0.0.68)
- [x] 58.1: Define `SymbolNode`, `SymbolKind`, `EdgeKind` types in `graph.rs`
- [x] 58.2: Define `FileHashTracker` for incremental change detection
- [x] 58.3: Add symbol graph constants to `constants.rs`
- [x] 58.4: Refactor `CodeGraph` struct to `DiGraph<SymbolNode, EdgeKind>`
- [x] 58.5: Update `compute_pagerank` for symbol-level nodes
- [x] 58.6: Update `get_blast_radius` for symbol-level precision
- [x] 58.7: Update `CodeExploreEngine::explore()` consumers
- [x] 58.8: Update `format_repomap` to use symbol graph
- [x] 58.9: Update all tool registrations (backward compat)
- [x] 58.10: Run all tests + clippy
- [x] 58.11: Version bump → `v0.0.68`, changelog, commit, tag, push

### Phase 59: Graph Persistence to Disk (v0.0.69)
- [x] 59.1: Define `GraphSnapshot` serialization format in new `graph_store.rs`
- [x] 59.2: Integrate `load_cached` / `incremental_update` / `save_to_disk` into `CodeGraph`
- [x] 59.3: Register `graph_store` module in `mod.rs`
- [x] 59.4: Version bump → `v0.0.69`, changelog, commit, tag, push

### Phase 60: Workspace Analysis Onboarding Menu & Cache-Aware Indexing (v0.0.70)
- [x] 60.1: Implement `ModalState::WorkspaceAnalysis` with Design 1 minimal rounded card layout
- [x] 60.2: Add startup detection in unindexed repos with Quick Index, Deep Scan, and Skip options
- [x] 60.3: Add F2 keybinding and `/init`, `/index`, `/analyze` slash commands with cache-aware indexing
- [x] 60.4: Intercept natural language analysis intents ("analyze project", "index codebase", etc.)
- [x] 60.5: Version bump → `v0.0.70`, changelog, commit, tag, push

### Phase 60.1: Global Non-Destructive Configuration, Multi-Provider Support & Graceful Startup (v0.0.71)
- [x] 60.1.1: Global persistent `[provider.api_keys]` and `[provider.custom_endpoints]` in `~/.config/minicode/config.toml` & `~/.config/minicode/.env`
- [x] 60.1.2: Automatic backup of `.env` in `localupdate.sh` with 10-backup rotation retention
- [x] 60.1.3: Eliminate missing API key startup crash, auto-launch setup wizard in interactive mode, exit gracefully if aborted
- [x] 60.1.4: Add first-class support for MiniMax, Z.ai / Zhipu GLM, and Mistral AI providers
- [x] 60.1.5: Interactive API Key Manager submenu `[2]` to view and edit keys without switching active provider
- [x] 60.1.6: Custom OpenAI-compatible endpoint registration and live model discovery with `[FREE]` badges
- [x] 60.1.7: Add `minicode config` and `minicode setup` CLI aliases
- [x] 60.1.8: Version bump → `v0.0.71`, changelog, commit, tag, push

### Phase 60.2: Ultra-Compact Permission Dialog (Variant 2A) & Argument Extraction Fix (v0.0.72)
- [x] 60.2.1: Fix `"command"` vs `"cmd"` tool argument extraction so bash commands are highlighted properly
- [x] 60.2.2: Implement **Variant 2A: Modern Rounded & Pill-Highlighted Single-Block Strip** (height ~8–9 lines)
- [x] 60.2.3: Add full-line vibrant pill highlight on active selection (`theme.brand_accent` + bold text)
- [x] 60.2.4: Auto-responsive width & height with text truncation and ellipsis support
- [x] 60.2.5: Instant hotkey response on `y` (Approve), `n`/`Esc` (Reject), `a` (Session Allow), `f` (Feedback)
- [x] 60.2.6: Version bump → `v0.0.72`, changelog, commit, tag, push

### Phase 61: Diff-to-Symbol Projection for `/review` (v0.0.73)
- [ ] 61.1: Create `DiffProjector` module with symbol-level diff mapping
- [ ] 61.2: Integrate into `diff_impact` tool with enhanced output
- [ ] 61.3: Add 6th review pillar to `GitReviewer`: Structural Impact Analysis
- [ ] 61.4: Version bump → `v0.0.73`, changelog, commit, tag, push

### Phase 62: Hybrid BM25 + Vector + PageRank Retrieval (v0.0.74)
- [ ] 62.1: Create `HybridIndex` with BM25 inverted index + RRF fusion
- [ ] 62.2: Update `semantic_search` and `search_symbols_semantic` tools
- [ ] 62.3: Add RRF constants to `constants.rs`
- [ ] 62.4: Version bump → `v0.0.74`, changelog, commit, tag, push

### Phase 63: Progressive Memory Tiers (v0.0.75)
- [ ] 63.1: Create `ProgressiveMemory` module with L0–L3 tiers
- [ ] 63.2: Implement fact extraction from compaction
- [ ] 63.3: Inject memory tiers into system prompt
- [ ] 63.4: Persistence to `.minicode/progressive_memory.json`
- [ ] 63.5: Version bump → `v0.0.75`, changelog, commit, tag, push

### Phase 64: Test Gap Detection + Composite Risk Scoring (v0.0.76)
- [ ] 64.1: Implement `TestGapAnalyzer` with call-graph reachability
- [ ] 64.2: Enhance `BlastRadiusReport` with composite risk formula
- [ ] 64.3: Expose `test_coverage_gaps` tool
- [ ] 64.4: Version bump → `v0.0.76`, changelog, commit, tag, push



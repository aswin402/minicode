# minicode — Todo Tracker

> **Current Phase:** Phase 1 Post-Audit Fixes | **Status:** ✅ Completed (39 Tests Passing, 0 Warnings)

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

## ⏳ Phase 2: AST Code Intelligence & Search Indexing (PLANNED)
- [ ] Enhanced Tree-sitter multi-language parsing
- [ ] Tantivy-based full-text search index
- [ ] Cross-file symbol resolution
- [ ] Semantic code search

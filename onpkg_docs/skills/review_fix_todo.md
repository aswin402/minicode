# Review Fix Todo — `minicode` ⚡
> Based on: `onpkg_docs/review_fix_plan.md` | Created: 2026-09-02

---

## PHASE 1 — Critical Security & Correctness

### 🔴 1.1 Approval Registry Leak on Rollback
**File**: `src/agent/loop.rs`
**Severity**: HIGH
**Effort**: ~2 hours

- [ ] **1.1a** Audit `execute_turn` to confirm `tool_id` format embeds `turn_id` prefix
  - Search: `format!("{}_{}", self.current_turn_id,` in `loop.rs`
  - Confirm format: `"{turn_id}_{uuid}"` or similar
- [ ] **1.1b** Add rollback drain logic to `rollback_turn()`
  ```rust
  let to_remove: Vec<String> = self.pending_approvals
      .lock().keys()
      .filter(|k| k.starts_with(&format!("{}_", target_turn_id)))
      .cloned()
      .collect();
  let mut guard = self.pending_approvals.lock();
  for key in to_remove { guard.remove(&key); }
  ```
- [ ] **1.1c** Add test: `test_approval_registry_drained_on_rollback`
  - Start turn → request approval → call `rollback_turn` → verify registry empty
- [ ] **1.1d** Run: `cargo test -p minicode -- loop::tests -- test_approval`

---

### 🔴 1.2 Hardcoded Model Names (3x drift)
**Files**: `src/config.rs`, `src/agent/provider.rs`, `src/constants.rs`
**Severity**: HIGH
**Effort**: ~3 hours

- [ ] **1.2a** Add all provider default model constants to `constants.rs`
  ```rust
  pub const DEFAULT_PROVIDER: &str = "gemini";
  pub const DEFAULT_MODEL_GEMINI: &str = "gemini-2.5-pro";
  pub const DEFAULT_MODEL_OPENAI: &str = "gpt-4o";
  pub const DEFAULT_MODEL_ANTHROPIC: &str = "anthropic/claude-3.5-sonnet";
  pub const DEFAULT_MODEL_DEEPSEEK: &str = "deepseek-coder";
  pub const DEFAULT_MODEL_OLLAMA: &str = "llama3";
  pub const DEFAULT_MODEL_OPENROUTER: &str = "anthropic/claude-3.5-sonnet";
  pub const DEFAULT_MODEL_MINIMAX: &str = "MiniMax-Text-01";
  pub const DEFAULT_MODEL_ZHIPU: &str = "glm-4-flash";
  pub const DEFAULT_MODEL_MISTRAL: &str = "codestral-latest";
  ```
- [ ] **1.2b** Update `config.rs:default_model_name()` → `GEMINI_DEFAULT_MODEL`
- [ ] **1.2c** Update `provider.rs:GeminiProvider::default_model()` → `GEMINI_DEFAULT_MODEL`
- [ ] **1.2d** Update `config.rs:default_provider_name()` → `DEFAULT_PROVIDER`
- [ ] **1.2e** Add compile-time assertion in `constants.rs`:
  ```rust
  const _: () = assert!(DEFAULT_MODEL_GEMINI.starts_with("gemini"));
  ```
- [ ] **1.2f** Update all test fixtures in `config.rs`, `provider.rs`, `session/store.rs`, `agent/types.rs` that use hardcoded `"gemini-2.5-pro"`
  - Search: `"gemini-2.5-pro"` across all `src/`
  - Replace with `constants::DEFAULT_MODEL_GEMINI` or constant reference
- [ ] **1.2g** Run: `cargo test -p minicode -- config::tests -- test_`
- [ ] **1.2h** Run: `cargo test -p minicode -- provider::tests -- test_`
- [ ] **1.2i** Run: `cargo test -p minicode -- session::store::tests -- test_`
- [ ] **1.2j** Run: `cargo test -p minicode -- agent::types::tests -- test_`

---

### 🟡 1.3 Path Validation TOCTOU Warning
**Files**: `src/sandbox/path.rs`, `src/tools/exec.rs`
**Severity**: MED
**Effort**: ~1 hour

- [ ] **1.3a** Check if `landlock.rs` has Landlock support detection
  - Search: `is_supported`, `supports_landlock`, `landlock_is_available`
- [ ] **1.3b** If not, add support detection:
  ```rust
  #[cfg(target_os = "linux")]
  pub fn is_landlock_supported() -> bool {
      // Probe by checking /proc/sys/kernel/unprivileged_userns_clone or kernel version
      // OR wrap the Landlock create_rule syscall and catch ENOSYS
  }
  ```
- [ ] **1.3c** Add warning in `exec_cmd` before first command:
  ```rust
  #[cfg(target_os = "linux")]
  if !landlock::is_supported() {
      tracing::warn!(
          "Landlock unavailable (kernel < 5.13). Path validation is \
           defense-in-depth only — TOCTOU race window exists."
      );
  }
  ```
- [ ] **1.3d** Also surface in `minicode doctor` output
- [ ] **1.3e** Run: `cargo test -p minicode -- exec::tests`

---

### 🟡 1.4 Missing Disk Space Check in Backup
**File**: `src/session/backup.rs`
**Severity**: MED
**Effort**: ~2 hours

- [ ] **1.4a** Add `check_available_space()` helper:
  ```rust
  fn check_available_space(path: &Path, required_bytes: u64) -> Result<()> {
      let available = fs::available_space(path)
          .map_err(|e| SessionError::BackupFailed(
              format!("Cannot determine disk space: {}", e)))?;
      if available < required_bytes {
          return Err(SessionError::BackupFailed(format!(
              "Insufficient disk space: {} bytes needed, {} available", 
              required_bytes, available)));
      }
      Ok(())
  }
  ```
- [ ] **1.4b** Estimate bytes needed in `create_checkpoint`:
  - Read file size + 10% buffer for copy overhead
  - Add manifest write (~1KB)
- [ ] **1.4c** Handle `io::ErrorKind::Other` (filesystems without `available_space`)
  - Fall back to warn-and-continue (disk full will fail naturally on write)
- [ ] **1.4d** Add test: mock filesystem with < 100 bytes available
  - Verify graceful error message instead of cryptic write failure
- [ ] **1.4e** Run: `cargo test -p minicode -- session::backup::tests -- test_`

---

## PHASE 2 — Hardening & Correctness

### 🟡 2.1 GitHub Token Env Var Duplication
**Files**: `src/tools/github/client.rs`, `src/tools/github/mod.rs`
**Severity**: MED
**Effort**: ~1 hour

- [ ] **2.1a** Add to `src/tools/github/mod.rs`:
  ```rust
  pub const GITHUB_TOKEN_VARS: &[&str] = &["GITHUB_TOKEN", "GH_TOKEN"];
  
  pub fn get_github_token() -> Result<String> {
      GITHUB_TOKEN_VARS.iter()
          .find_map(|v| std::env::var(v).ok())
          .ok_or_else(|| ToolError::CommandExec(
              "GitHub auth required: set GITHUB_TOKEN or GH_TOKEN".into()))
  }
  ```
- [ ] **2.1b** Update `client.rs:85-86` → `get_github_token()`
- [ ] **2.1c** Check `service.rs` for duplicate token reads → update if found
- [ ] **2.1d** Add test: mock `GH_TOKEN` env var → resolves correctly
- [ ] **2.1e** Run: `cargo test -p minicode -- github::tests`

---

### 🟡 2.2 Compaction Threshold Inconsistency
**Files**: `src/context/auto_compact.rs`, `src/context/compressor.rs`, `src/constants.rs`
**Severity**: MED
**Effort**: ~2 hours

- [ ] **2.2a** Extract `CompactionConfig` struct:
  ```rust
  pub struct CompactionConfig {
      pub warning_ratio: f64,    // 0.60 — tier 1
      pub danger_ratio: f64,     // 0.80 — tier 2
      pub critical_ratio: f64,   // 0.95 — tier 3
      pub safety_margin: f64,    // 0.15
  }
  
  impl Default for CompactionConfig {
      fn default() -> Self {
          Self {
              warning_ratio: COMPACT_TIER1_RATIO,
              danger_ratio: COMPACT_TIER2_RATIO,
              critical_ratio: COMPACT_TIER3_RATIO,
              safety_margin: COMPRESSOR_SAFETY_MARGIN,
          }
      }
  }
  ```
- [ ] **2.2b** Update `AutoCompactor::new()` to accept `CompactionConfig`
- [ ] **2.2c** Update `ContextCompressor` to use `CompactionConfig::default().warning_ratio`
- [ ] **2.2d** Remove redundant `COMPRESSOR_WARNING_THRESHOLD` if only `COMPACT_TIER1_RATIO` is used
- [ ] **2.2e** Run: `cargo test -p minicode -- auto_compact::tests`
- [ ] **2.2f** Run: `cargo test -p minicode -- compressor::tests`

---

### 🟡 2.3 Token Usage Silent Drop
**Files**: `src/agent/provider.rs`
**Severity**: MED
**Effort**: ~1 hour

- [ ] **2.3a** Add `tracing::debug!` when usage metadata is zero but response succeeded
  ```rust
  if prompt_tokens == 0 && completion_tokens == 0 {
      tracing::debug!(
          "Usage metadata absent or zero in {} response (prompt_tokens={}, completion_tokens={})",
          provider_name, prompt_tokens, completion_tokens
      );
  }
  ```
- [ ] **2.3b** Add `StreamChunk::UsageMetadataMissing` variant (optional — for observability)
- [ ] **2.3c** Add test: mock provider returns JSON without `usage` field → graceful 0, no panic
- [ ] **2.3d** Run: `cargo test -p minicode -- provider::tests`

---

### 🟡 2.4 Unbounded ApprovalRegistry
**Files**: `src/agent/types.rs`, `src/agent/loop.rs`
**Severity**: MED
**Effort**: ~2 hours

- [ ] **2.4a** Add `prune_stale_approvals()`:
  ```rust
  pub fn prune_stale_approvals(registry: &ApprovalRegistry) {
      let mut guard = registry.lock();
      guard.retain(|_id, sender| !sender.is_closed());
  }
  ```
- [ ] **2.4b** Call in `execute_turn` at start of each turn iteration
- [ ] **2.4c** Add metric: log count of pruned stale approvals at `debug` level
- [ ] **2.4d** Add test: dropped sender → prune cleans it up
- [ ] **2.4e** Run: `cargo test -p minicode -- loop::tests -- test_prune`

---

### 🟡 2.5 JSONL Crash Safety
**File**: `src/session/store.rs`
**Severity**: MED
**Effort**: ~3 hours

- [ ] **2.5a** Add `write_atomic_jsonl()` helper:
  ```rust
  fn write_atomic_jsonl(path: &Path, line: &str) -> Result<()> {
      let tmp = path.with_extension("tmp");
      let mut file = OpenOptions::new()
          .write(true)
          .create_new(true)
          .open(&tmp)?;
      file.write_all(line.as_bytes())?;
      file.write_all(b"\n")?;
      #[cfg(unix)] file.sync_all()?;
      std::fs::rename(&tmp, path)?;
      Ok(())
  }
  ```
- [ ] **2.5b** Replace all direct `write().append()` calls in `append_event()`
- [ ] **2.5c** Benchmark: write 1000 events → ensure < 50ms overhead (1 rename syscall)
- [ ] **2.5d** Add test: simulate crash mid-write → verify no partial lines on reload
- [ ] **2.5e** Run: `cargo test -p minicode -- session::store::tests -- test_atomic`

---

## PHASE 3 — Cleanup & Polish

### 🟢 3.1 Tool Count Validation
**File**: `src/constants.rs`
**Severity**: LOW
**Effort**: ~1 hour

- [ ] **3.1a** Create `#[cfg(test)]` module in `constants.rs`:
  ```rust
  #[cfg(test)]
  mod tool_count_validation {
      #[test]
      fn total_tool_count_is_accurate() {
          let schema_count = crate::tools::registry::ToolRegistry::get_tool_schemas().len();
          assert_eq!(
              schema_count, crate::constants::TOTAL_TOOL_COUNT,
              "TOTAL_TOOL_COUNT ({}) != actual schemas ({})",
              crate::constants::TOTAL_TOOL_COUNT, schema_count
          );
      }
  }
  ```
- [ ] **3.1b** Run test → it will fail → update `TOTAL_TOOL_COUNT` to match
- [ ] **3.1c** Now constant is derived from reality, not manually maintained

---

### 🟢 3.2 Dead Code Audit
**File**: `src/constants.rs`
**Severity**: LOW
**Effort**: ~2 hours

- [ ] **3.2a** List all `#[allow(dead_code)]` in `constants.rs`:
  ```
  grep -n "allow(dead_code)" src/constants.rs
  ```
- [ ] **3.2b** For each, run:
  ```
  grep -r "SYMBOL_GRAPH_MAX_NODES" src/
  ```
  to check actual usage
- [ ] **3.2c** Categorize each:
  - **Unused + planned**: add `// TODO(phase-N): uncomment when X lands`
  - **Unused + never used**: remove constant and `#[allow]`
  - **Intentionally public**: keep with comment explaining purpose
- [ ] **3.2d** Common culprits to check:
  - `SYMBOL_GRAPH_MAX_NODES`, `SYMBOL_GRAPH_MAX_EDGES`
  - `INDEX_CACHE_MAX_ENTRIES`
  - `MAX_WEB_RESPONSE_BYTES`
  - `MESSAGE_FRAMING_TOKEN_OVERHEAD`
  - `AGENT_EVENT_CHANNEL_CAPACITY`
  - `MAX_AUTOCOMPLETE_ROWS`
  - `SKILL_MD_FILE`, `SKILLS_DIR_NAME` (may be for future skill discovery)
- [ ] **3.2e** Run: `cargo clippy -- -D warnings` after cleanup

---

### 🟢 3.3 Intent Confidence Bounds
**File**: `src/agent/intent.rs`
**Severity**: LOW
**Effort**: ~1 hour

- [ ] **3.3a** Add `.clamp(0.0, 1.0)` to confidence result in `match_intent()`
- [ ] **3.3b** Add test: high-confidence keyword match → returns ≤ 1.0
- [ ] **3.3c** Add test: edge case empty string → handled gracefully
- [ ] **3.3d** Run: `cargo test -p minicode -- intent::tests`

---

### 🟢 3.4 Landlock Unavailability Warning
**Files**: `src/tools/exec.rs`, `src/app.rs`
**Severity**: LOW
**Effort**: ~1 hour

- [ ] **3.4a** Detect Landlock support at startup in `exec_cmd`
- [ ] **3.4b** Log warning once (use `static ONCE` or first-call flag)
- [ ] **3.4c** Add to `minicode doctor` command output:
  ```
  OS Sandbox: ⚠ Landlock unavailable (kernel 5.11) — path checks only
  ```
- [ ] **3.4d** Run: `cargo test -p minicode -- exec::tests`

---

### 🟢 3.5 Pern Template Password
**File**: `src/tools/onpkg/templates/builtin/pern.rs`
**Severity**: LOW
**Effort**: ~1 hour

- [ ] **3.5a** Replace hardcoded passwords:
  ```rust
  content: format!(
      "DATABASE_URL=\"postgresql://postgres:{}@localhost:5432/pern\"\n\
       PORT=5000\n\
       JWT_SECRET={}",
      uuid::Uuid::new_v4(),
      uuid::Uuid::new_v4()
  )
  ```
- [ ] **3.5b** Update test fixtures that assert on specific JWT_SECRET value
- [ ] **3.5c** Run: `cargo test -p minicode -- pern_template`
- [ ] **3.5d** Check other templates (`react_vite.rs`, `next_template.rs`) for similar issues
  - `react_vite.rs:5874`: `const [email, password]` — that's JS code, not a real password ✓
  - `next_template.rs`: check if any real secrets

---

### 🟡 3.6 Rate Limiting on exec_cmd
**File**: `src/agent/loop.rs`, `src/tools/exec.rs`
**Severity**: MED
**Effort**: ~3 hours

- [ ] **3.6a** Create `ToolRateLimiter` struct:
  ```rust
  pub struct ToolRateLimiter {
      window_start: Instant,
      window_secs: u64,
      max_calls: usize,
      current_calls: usize,
  }
  
  impl ToolRateLimiter {
      pub fn new(window_secs: u64, max_calls: usize) -> Self { ... }
      pub fn check(&mut self) -> Result<()> { ... }
      pub fn reset_if_window_expired(&mut self) { ... }
  }
  ```
- [ ] **3.6b** Add to `AgentLoop` struct:
  ```rust
  exec_rate_limiter: ToolRateLimiter,
  ```
- [ ] **3.6c** Wire into `execute_turn`: check before `ToolRegistry::dispatch` for `exec_cmd`
- [ ] **3.6d** Add config field: `agent.max_exec_per_minute: 60` (default)
- [ ] **3.6e** Return `ToolError::RateLimited` with retry-after hint
- [ ] **3.6f** Add test: exceed limit → subsequent calls rejected
- [ ] **3.6g** Run: `cargo test -p minicode -- loop::tests -- test_rate_limit`

---

## PHASE 4 — Testing & Validation

### 🚀 Run Full Validation Suite
**Effort**: ~2 hours

- [ ] **4.1** Run full test suite:
  ```bash
  cargo test -p minicode -- --test-threads=4
  ```
- [ ] **4.2** Run clippy:
  ```bash
  cargo clippy -- -D warnings
  ```
- [ ] **4.3** Run fmt:
  ```bash
  cargo fmt --check
  ```
- [ ] **4.4** Run integration tests (if separate):
  ```bash
  cargo test --test integration_*
  ```
- [ ] **4.5** Manual verification:
  - [ ] `minicode --help` → all flags present
  - [ ] `minicode doctor` → no panics
  - [ ] `minicode --json-stream` → starts without error
  - [ ] Start TUI → verify Aura theme renders correctly

---

## CHECKLIST: Pre-PR Gate

- [ ] All HIGH severity items resolved (1.1, 1.2)
- [ ] All MED severity items resolved or acknowledged (1.3, 1.4, 2.1–2.5, 3.6)
- [ ] All LOW severity items resolved or deferred with `// TODO` comment
- [ ] No new `unwrap()` / `expect()` in production paths
- [ ] No new `unsafe` code introduced
- [ ] All 37+ integration tests pass
- [ ] `cargo clippy -- -D warnings` clean
- [ ] `cargo fmt` applied
- [ ] CHANGELOG.md updated
- [ ] PR description links to `onpkg_docs/review_fix_plan.md`

---

## Progress Tracker

| Item | Status | Notes |
|:-----|:-------|:------|
| 1.1 Approval leak | ⬜ TODO | |
| 1.2 Hardcoded models | ⬜ TODO | |
| 1.3 TOCTOU warning | ⬜ TODO | |
| 1.4 Disk space check | ⬜ TODO | |
| 2.1 GitHub token DRY | ⬜ TODO | |
| 2.2 Compaction config | ⬜ TODO | |
| 2.3 Token logging | ⬜ TODO | |
| 2.4 Approval cleanup | ⬜ TODO | |
| 2.5 JSONL atomic | ⬜ TODO | |
| 3.1 Tool count test | ⬜ TODO | |
| 3.2 Dead code audit | ⬜ TODO | |
| 3.3 Intent clamp | ⬜ TODO | |
| 3.4 Landlock warning | ⬜ TODO | |
| 3.5 Pern template | ⬜ TODO | |
| 3.6 Rate limiter | ⬜ TODO | |
| 4.x Full validation | ⬜ TODO | |

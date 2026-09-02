# Review Fix Implementation Plan — `minicode` ⚡

## Metadata
- **Created**: 2026-09-02
- **Review Severity**: 22 findings across 5 priority levels
- **Estimated Effort**: ~3-4 days of focused work

---

## PHASE 1 — Critical Security & Correctness (Day 1)

### 1.1 🔴 Approval Registry Leak on Rollback
**File**: `src/agent/loop.rs`
**Severity**: HIGH — hung approvals after `/undo`

```rust
// PROBLEM: rollback_turn() doesn't drain pending_approvals
pub fn rollback_turn(&mut self, target_turn_id: usize, message_index: usize) {
    self.messages.truncate(message_index);
    self.current_turn_id = target_turn_id;
    // ❌ Missing: drain and drop all pending approval senders for turns > target
}
```

**Fix**:
```rust
pub fn rollback_turn(&mut self, target_turn_id: usize, message_index: usize) {
    self.messages.truncate(message_index);
    self.current_turn_id = target_turn_id;
    
    // Drop all in-flight approvals from rolled-back turns
    let to_remove: Vec<String> = self.pending_approvals
        .lock()
        .keys()
        .filter(|k| k.starts_with(&format!("{}_", target_turn_id))) // tool_ids embed turn_id prefix
        .cloned()
        .collect();
    
    let mut guard = self.pending_approvals.lock();
    for key in to_remove {
        guard.remove(&key);
    }
}
```

**Tasks**:
- [ ] Add `tool_id` format `"{turn_id}_{uuid}"` in `execute_turn` if not already
- [ ] Add rollback drain logic in `rollback_turn()`
- [ ] Add integration test: approve request → rollback → verify no leaked senders
- [ ] Verify with `cargo test -p minicode -- loop::tests`

---

### 1.2 🔴 Hardcoded Model Names (3x drift)
**Files**: `src/config.rs`, `src/agent/provider.rs`, `src/constants.rs`
**Severity**: HIGH — maintenance hazard, inconsistent defaults

**Fix**: Single source of truth in `constants.rs`

```rust
// constants.rs
pub const DEFAULT_PROVIDER: &str = "gemini";
pub const DEFAULT_MODEL_GEMINI: &str = "gemini-2.5-pro";
pub const DEFAULT_MODEL_OPENAI: &str = "gpt-4o";
pub const DEFAULT_MODEL_ANTHROPIC: &str = "anthropic/claude-3.5-sonnet";
// ... etc
```

**Tasks**:
- [ ] Add all provider default model constants to `constants.rs`
- [ ] Update `config.rs:default_model_name()` → `constants::DEFAULT_MODEL_GEMINI`
- [ ] Update `provider.rs:GeminiProvider::default_model()` → `constants::DEFAULT_MODEL_GEMINI`
- [ ] Update all test fixtures referencing hardcoded model names
- [ ] Add compile-time check: `const _: () = assert!(DEFAULT_MODEL_GEMINI.contains("gemini"));`
- [ ] `cargo test -p minicode` — verify all green

---

### 1.3 🟡 Path Validation TOCTOU Documentation
**File**: `src/sandbox/path.rs`
**Severity**: MED — acknowledged but needs explicit runtime warning

**Fix**: Add runtime warning when Landlock is unavailable

```rust
// In exec.rs or app.rs startup
#[cfg(target_os = "linux")]
{
    if !landlock::is_supported() {
        tracing::warn!(
            "Landlock sandbox unavailable (kernel < 5.13). \
             Path validation is defense-in-depth only — TOCTOU races possible."
        );
    }
}
```

**Tasks**:
- [ ] Check if `landlock.rs` has `is_supported()` detection
- [ ] If not, add it using `std::process::Command` to probe syscall
- [ ] Add warning in `exec_cmd` before first command execution
- [ ] Log at `WARN` level, not `ERROR` (degrades gracefully)

---

### 1.4 🟡 Missing Disk Space Check in Backup
**File**: `src/session/backup.rs`
**Severity**: MED — silent failure on full disk

**Fix**:

```rust
use std::fs;

fn check_available_space(path: &Path, required_bytes: u64) -> Result<()> {
    let available = fs::available_space(path)
        .map_err(|e| SessionError::BackupFailed(format!("Cannot statfs: {}", e)))?;
    if available < required_bytes {
        return Err(SessionError::BackupFailed(
            format!("Insufficient disk space: {} bytes needed, {} available", 
                    required_bytes, available)
        ));
    }
    Ok(())
}
```

**Tasks**:
- [ ] Add `check_available_space()` helper to `backup.rs`
- [ ] Call before `create_checkpoint()` writes
- [ ] Estimate required bytes from file size + 10% overhead
- [ ] Handle `io::ErrorKind::Other` for filesystems without `available_space()`
- [ ] Add test with mock filesystem quota

---

## PHASE 2 — Hardening & Correctness (Day 2)

### 2.1 🟡 GitHub Token Env Var Duplication
**Files**: `src/tools/github/client.rs`, `src/tools/github/mod.rs`
**Severity**: MED — DRY violation, easy to desync

**Fix**:
```rust
// src/tools/github/mod.rs or new src/tools/github/env.rs
pub const GITHUB_TOKEN_VARS: &[&str] = &["GITHUB_TOKEN", "GH_TOKEN"];

pub fn get_github_token() -> Result<String> {
    for var in GITHUB_TOKEN_VARS {
        if let Ok(token) = std::env::var(var) {
            return Ok(token);
        }
    }
    Err(ToolError::CommandExec("GitHub auth required: set GITHUB_TOKEN or GH_TOKEN".into()))
}
```

**Tasks**:
- [ ] Create shared token retrieval in `src/tools/github/mod.rs`
- [ ] Update `client.rs:85-86` to use it
- [ ] Update `service.rs` if it also reads GitHub tokens
- [ ] Add test: mock env var resolution
- [ ] `cargo test -p minicode -- github::tests`

---

### 2.2 🟡 Compaction Threshold Inconsistency
**Files**: `src/context/auto_compact.rs`, `src/context/compressor.rs`, `src/constants.rs`
**Severity**: MED — confusing dual-threshold system

**Fix**: Unify into single `CompactionConfig` struct

```rust
// src/context/auto_compact.rs
pub struct CompactionConfig {
    pub warning_ratio: f64,       // 0.60 — tier 1 trigger
    pub danger_ratio: f64,       // 0.80 — tier 2 trigger  
    pub critical_ratio: f64,     // 0.95 — tier 3 trigger
    pub safety_margin: f64,      // 0.15 — provider headroom
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

**Tasks**:
- [ ] Extract `CompactionConfig` from `AutoCompactor::new()`
- [ ] Remove duplicate `COMPRESSOR_WARNING_THRESHOLD` from `compressor.rs`
- [ ] Ensure `ContextCompressor` uses `CompactionConfig::default().warning_ratio`
- [ ] All constants already in `constants.rs` — just reference them
- [ ] `cargo test -p minicode -- context::auto_compact::tests`

---

### 2.3 🟡 `unwrap_or(0)` Silent Token Data Loss
**Files**: `src/agent/provider.rs:347-348, 681-682`
**Severity**: MED — usage metadata silently dropped on JSON shape mismatch

**Fix**:
```rust
// Instead of: .unwrap_or(0)
// Use: .and_then(|t| t.as_u64()).unwrap_or(0)  // already correct
// BUT add explicit logging when tokens are 0 but response succeeded

if prompt_tokens == 0 && completion_tokens == 0 {
    tracing::debug!("Usage metadata absent in provider response");
}
```

**Tasks**:
- [ ] Add `tracing::debug!` for missing usage metadata
- [ ] Consider adding `StreamChunk::UsageMetadataMissing` variant for observability
- [ ] Add test with mocked JSON response missing `usage` field

---

### 2.4 🟡 Unbounded ApprovalRegistry Channel
**Files**: `src/agent/types.rs`, `src/agent/loop.rs`
**Severity**: MED — HashMap grows forever if approvals are never resolved

**Fix**:
```rust
// Add periodic cleanup in execute_turn loop
fn prune_stale_approvals(registry: &ApprovalRegistry) {
    let mut guard = registry.lock();
    // Remove entries where sender is disconnected (already dropped)
    // oneshot::Sender dropped = channel closed = approval never answered
    guard.retain(|_id, sender| !sender.is_closed());
}
```

**Tasks**:
- [ ] Add `prune_stale_approvals()` to `ApprovalRegistry` impl
- [ ] Call in `execute_turn` at turn start
- [ ] Add metric: `approval_registry_size` to observability
- [ ] `cargo test -p minicode -- loop::tests::test_approval_cleanup`

---

### 2.5 🟡 JSONL Crash Safety
**File**: `src/session/store.rs`
**Severity**: MED — partial line loss on crash

**Fix**: Use rename-overwrite pattern

```rust
// Instead of direct append:
let tmp_path = session_path.with_extension("tmp");
let mut file = OpenOptions::new()
    .create_new(true)
    .write(true)
    .open(&tmp_path)?;
// Write line to tmp
file.write_all(line.as_bytes())?;
// fsync to ensure durability
file.sync_all()?;
// Atomic rename
std::fs::rename(&tmp_path, &session_path)?;
// Now append is crash-safe
```

**Tasks**:
- [ ] Add `write_atomic_jsonl()` helper
- [ ] Use for every `append_event()` call
- [ ] Add `#[cfg(unix)]` `fsync` call after rename
- [ ] Benchmark: ensure rename overhead is acceptable (<1ms)
- [ ] `cargo test -p minicode -- session::store::tests`

---

## PHASE 3 — Cleanup & Polish (Day 3)

### 3.1 🟢 Manually Maintained Tool Count
**File**: `src/constants.rs:554`
**Severity**: LOW — easy to desync

**Fix**: Compile-time assertion

```rust
// At bottom of constants.rs
#[cfg(test)]
mod tool_count_validation {
    use crate::tools::registry::context_tools::ContextTools;
    
    #[test]
    fn total_tool_count_matches_registry() {
        let schemas = ContextTools::get_tool_schemas();
        assert_eq!(
            schemas.len(), 
            crate::constants::TOTAL_TOOL_COUNT,
            "TOTAL_TOOL_COUNT is out of sync with actual tool registry"
        );
    }
}
```

**Tasks**:
- [ ] Import tool registry in `constants.rs` test module
- [ ] Write test that counts actual tool schemas
- [ ] Remove hardcoded `94` — let test drive the constant
- [ ] Run: `cargo test -p minicode -- tool_count_validation`

---

### 3.2 🟢 `#[allow(dead_code)]` Audit
**Files**: `src/constants.rs` (~15 instances)
**Severity**: LOW — technical debt

**Tasks**:
- [ ] List all `#[allow(dead_code)]` in `constants.rs`
- [ ] For each, determine: used? planned? removable?
- [ ] Categories:
  - **Actually unused**: remove constant + `#[allow]` attribute
  - **Used in incomplete feature**: add `// TODO(phase-N): uncomment when X lands`
  - **Intentionally public API**: keep `#[allow]` with explanation
- [ ] Run `cargo clippy -- -D warnings` after audit

---

### 3.3 🟢 `IntentMatch` Confidence Bounds
**File**: `src/agent/intent.rs`
**Severity**: LOW — malformed input could produce OOB confidence

**Fix**:
```rust
// In match_intent(), clamp output
IntentMatch {
    intent,
    confidence: confidence.clamp(0.0, 1.0),  // Add clamp
    query: query.clone(),
    suggested_command,
}
```

**Tasks**:
- [ ] Add `.clamp(0.0, 1.0)` to confidence result
- [ ] Add test: intentional OOB confidence input → clamped output
- [ ] `cargo test -p minicode -- intent::tests`

---

### 3.4 🟢 Landlock Unavailability Warning
**Files**: `src/tools/exec.rs`, `src/app.rs`
**Severity**: LOW — silent degradation

**Tasks**:
- [ ] Detect Landlock support at startup
- [ ] Log warning if running without OS sandbox
- [ ] Include in `minicode doctor` output
- [ ] `cargo test -p minicode -- exec::tests`

---

### 3.5 🟢 Pern Template Password
**File**: `src/tools/onpkg/templates/builtin/pern.rs:178`
**Severity**: LOW — predictable default in generated project

**Fix**:
```rust
content: format!(
    "DATABASE_URL=\"postgresql://postgres:{}@localhost:5432/pern\"\n\
     PORT=5000\n\
     JWT_SECRET={}",
    uuid::Uuid::new_v4(),          // Random password per scaffold
    uuid::Uuid::new_v4()           // Random secret per scaffold
)
```

**Tasks**:
- [ ] Replace `password` and `your_jwt_secret` with `uuid::Uuid::new_v4()`
- [ ] Update `pern.rs` test fixtures
- [ ] `cargo test -p minicode -- pern_template_tests`

---

### 3.6 🟢 Rate Limiting on exec_cmd
**Files**: `src/agent/loop.rs`, `src/tools/exec.rs`
**Severity**: MED — DoS potential from spam

**Fix**: Use existing `CircuitBreaker` pattern

```rust
// In AgentLoop
struct ToolRateLimiter {
    calls_per_window: usize,
    window_secs: u64,
    // ... sliding window counter
}

impl AgentLoop {
    fn check_tool_rate_limit(&self, tool: &str) -> Result<()> {
        // Apply to exec_cmd specifically — highest risk
        if tool == "exec_cmd" {
            self.rate_limiter.check("exec_cmd")?;
        }
        Ok(())
    }
}
```

**Tasks**:
- [ ] Create `ToolRateLimiter` using sliding window
- [ ] Wire into `execute_turn` before `ToolRegistry::dispatch`
- [ ] Configurable via `config.toml` (`agent.max_exec_per_minute`)
- [ ] Return `ToolError::RateLimited` when exceeded
- [ ] `cargo test -p minicode -- tool_rate_limiter_tests`

---

## PHASE 4 — Testing & Validation (Day 4)

### 4.1 Integration Test Coverage
**Priority order**:
1. `test_approval_registry_leak_on_rollback` — covers 1.1
2. `test_github_token_resolution` — covers 2.1
3. `test_jsonl_crash_atomic` — covers 2.5
4. `test_landlock_warning_at_startup` — covers 3.4
5. `test_tool_rate_limit_exceeded` — covers 3.6

### 4.2 Regression Suite
```bash
# Run full suite with threading limit
cargo test -p minicode -- --test-threads=4
cargo clippy -- -D warnings
cargo fmt --check
```

### 4.3 Performance Regression
- Session store: ensure atomic rename doesn't add >1ms per event
- Auto-compactor: ensure `prune_stale_approvals` doesn't block concurrent approvals

---

## Implementation Order

```
Day 1:  1.1 (approval leak) → 1.2 (hardcoded models) → 1.3 (TOCTOU warning)
Day 2:  2.1 (GitHub token) → 2.2 (compaction config) → 2.3 (token logging) → 2.4 (approval cleanup)
Day 3:  2.5 (JSONL atomic) → 3.1 (tool count test) → 3.2 (dead code audit) → 3.3 (intent clamp) → 3.4 (landlock warning) → 3.5 (pern template) → 3.6 (rate limit)
Day 4:  Full test suite → clippy → fmt → regression checks → PR
```

---

## Files to Modify

| File | Phase(s) | Change Size |
|:-----|:---------|:-----------|
| `src/agent/loop.rs` | 1.1, 2.4, 3.6 | Medium |
| `src/constants.rs` | 1.2, 2.2 | Large |
| `src/config.rs` | 1.2 | Small |
| `src/agent/provider.rs` | 1.2, 2.3 | Small |
| `src/agent/types.rs` | 2.4 | Small |
| `src/session/backup.rs` | 1.4 | Medium |
| `src/session/store.rs` | 2.5, 4.1 | Medium |
| `src/tools/github/mod.rs` | 2.1 | Small |
| `src/tools/github/client.rs` | 2.1 | Small |
| `src/tools/exec.rs` | 1.3, 3.4 | Small |
| `src/sandbox/path.rs` | 1.3 | Small |
| `src/agent/intent.rs` | 3.3 | Small |
| `src/tools/onpkg/templates/builtin/pern.rs` | 3.5 | Small |
| `src/context/auto_compact.rs` | 2.2 | Medium |

**Total**: ~14 files, ~500 lines changed across 4 days.

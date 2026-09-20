# Autonomous Configuration, Per-Directory Memory & Zero-Leak Diagnostics Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement dynamic provider/model resolution, dual-layer per-directory workspace memory, tabbed in-TUI `/settings` command, 4 dedicated `agent_config` tool primitives (Tools 136–139) with interactive permission confirmation, and universal zero-leak API key masking.

**Architecture:** A 6-tier provider resolution hierarchy eliminates hardcoded fallbacks and reads from `<workspace>/.minicode/config.toml` or `~/.config/minicode/workspaces.toml`. An interactive `/settings` modal manages per-provider default models, agent autonomy, and connection probes. 4 new `agent_config` tools enable the AI agent to inspect and safely reconfigure minicode with explicit user confirmation. Universal key masking ensures credentials never leak into prompt context or logs.

**Tech Stack:** Rust 2021 Edition, Tokio async runtime, Ratatui 0.29 / Crossterm 0.28, Reqwest HTTP client with `rustls-tls-webpki-roots`, Serde / TOML.

## Global Constraints

- Never use `.unwrap()` or `.expect()` in non-test code. Use `thiserror` for crate errors and `anyhow::Result` at boundaries.
- Run `cargo check -j 1` and `cargo test -j 1` on all checks and tests. Never run workspace-wide `cargo test`.
- Logging: use `tracing::*` macros. Never use `println!` or `eprintln!` in library code.
- Tool count invariant: Total tool count must increase from 135 to 139 and sync with `TOTAL_TOOL_COUNT` in `src/constants.rs`.
- Security: Plaintext API keys must never be emitted into tool return values, LLM prompt context, or logs.

---

### Task 1: Universal API Key Masking & Dual-Layer Workspace Registry (`workspaces.toml`)

**Files:**
- Modify: `src/constants.rs:430-445`
- Modify: `src/config.rs:1040-1090`
- Test: `src/config.rs` (unit tests in `mod tests`)

**Interfaces:**
- Consumes: `dirs::config_dir()`, `crate::constants::WORKSPACES_FILE_NAME`
- Produces:
  - `pub fn mask_api_key(key: &str) -> String`
  - `pub struct WorkspacePreference { pub provider: String, pub model: String, pub last_used: String }`
  - `pub fn load_workspace_preference(workspace_root: &Path) -> Option<WorkspacePreference>`
  - `pub fn save_workspace_preference(workspace_root: &Path, provider: &str, model: &str) -> anyhow::Result<()>`

- [ ] **Step 1: Write failing unit tests for key masking and workspace preference persistence**

Add in `src/config.rs` inside `#[cfg(test)] mod tests`:
```rust
#[test]
fn test_mask_api_key_variations() {
    assert_eq!(mask_api_key(""), "");
    assert_eq!(mask_api_key("short"), "••••••••");
    assert_eq!(mask_api_key("12345678"), "••••••••");
    assert_eq!(mask_api_key("sk-ant-1234567890abcdef"), "sk-a...cdef");
    assert_eq!(mask_api_key("AIzaSyD-1234567890XYZ"), "AIza...0XYZ");
}

#[test]
fn test_workspace_preference_roundtrip() {
    let temp_dir = tempfile::tempdir().unwrap();
    let ws_path = temp_dir.path();

    // Initially none
    let pref = load_workspace_preference_from_file(
        ws_path,
        &ws_path.join("workspaces.toml"),
    );
    assert!(pref.is_none());

    // Save preference
    save_workspace_preference_to_file(
        ws_path,
        "anthropic",
        "claude-3-7-sonnet-20250219",
        &ws_path.join("workspaces.toml"),
    )
    .unwrap();

    let pref = load_workspace_preference_from_file(
        ws_path,
        &ws_path.join("workspaces.toml"),
    )
    .unwrap();
    assert_eq!(pref.provider, "anthropic");
    assert_eq!(pref.model, "claude-3-7-sonnet-20250219");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -j 1 --lib config::tests::test_mask_api_key_variations`
Expected: FAIL with function not found.

- [ ] **Step 3: Implement `mask_api_key`, `WORKSPACES_FILE_NAME`, and workspace preference persistence**

In `src/constants.rs`:
```rust
/// Global workspaces registry file name
pub const WORKSPACES_FILE_NAME: &str = "workspaces.toml";
```

In `src/config.rs`:
```rust
/// Universally masks sensitive credentials for safe display and tool returns
pub fn mask_api_key(key: &str) -> String {
    let trimmed = key.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if trimmed.len() <= 8 {
        return "••••••••".to_string();
    }
    let prefix = &trimmed[..4];
    let suffix = &trimmed[trimmed.len() - 4..];
    format!("{}...{}", prefix, suffix)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkspacePreference {
    pub provider: String,
    pub model: String,
    #[serde(default)]
    pub last_used: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorkspaceRegistry {
    #[serde(default)]
    pub workspaces: std::collections::HashMap<String, WorkspacePreference>,
}

impl Config {
    pub fn get_workspace_registry_path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| {
            d.join(crate::constants::CONFIG_DIR_NAME)
                .join(crate::constants::WORKSPACES_FILE_NAME)
        })
    }

    pub fn load_workspace_preference(workspace_root: &Path) -> Option<WorkspacePreference> {
        let registry_path = Self::get_workspace_registry_path()?;
        let canonical = workspace_root.canonicalize().unwrap_or_else(|_| workspace_root.to_path_buf());
        load_workspace_preference_from_file(&canonical, &registry_path)
    }

    pub fn save_workspace_preference(
        workspace_root: &Path,
        provider: &str,
        model: &str,
    ) -> anyhow::Result<()> {
        let registry_path = Self::get_workspace_registry_path()
            .ok_or_else(|| anyhow::anyhow!("Unable to determine config directory"))?;
        let canonical = workspace_root.canonicalize().unwrap_or_else(|_| workspace_root.to_path_buf());
        save_workspace_preference_to_file(&canonical, provider, model, &registry_path)
    }
}
```

- [ ] **Step 4: Run tests and verify they pass**

Run: `cargo test -j 1 --lib config::tests::test_mask_api_key_variations`
Run: `cargo test -j 1 --lib config::tests::test_workspace_preference_roundtrip`
Expected: PASS

- [ ] **Step 5: Commit changes**

```bash
git add src/constants.rs src/config.rs
git commit -m "feat(config): implement universal API key masking and dual-layer workspace registry"
```

---

### Task 2: Dynamic 6-Tier Provider Resolution & Default Models Map

**Files:**
- Modify: `src/config.rs:50-130,520-540,860-910`
- Modify: `src/app/commands.rs:1660-1695`
- Test: `src/config.rs` (unit tests)

**Interfaces:**
- Consumes: `WorkspaceRegistry`, `Config::get_api_key`, `Config::is_local_provider`
- Produces:
  - `pub default_models: HashMap<String, String>` in `ProviderConfig`
  - `pub fn resolve_active_provider_and_model(&self, workspace_root: Option<&Path>) -> (String, String)`
  - `pub fn get_default_model_for_provider(&self, provider_name: &str) -> String`

- [ ] **Step 1: Write failing unit test for dynamic provider resolution**

In `src/config.rs` inside `#[cfg(test)] mod tests`:
```rust
#[test]
fn test_dynamic_provider_resolution_hierarchy() {
    let temp_dir = tempfile::tempdir().unwrap();
    let ws_path = temp_dir.path();

    let mut config = Config::default();
    config.provider.default = String::new();
    config.provider.model = String::new();

    // 1. With workspace preference saved
    save_workspace_preference_to_file(
        ws_path,
        "anthropic",
        "claude-3-7-sonnet-20250219",
        &ws_path.join("workspaces.toml"),
    )
    .unwrap();

    let (prov, model) = config.resolve_active_provider_and_model_with_registry(
        Some(ws_path),
        Some(&ws_path.join("workspaces.toml")),
    );
    assert_eq!(prov, "anthropic");
    assert_eq!(model, "claude-3-7-sonnet-20250219");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -j 1 --lib config::tests::test_dynamic_provider_resolution_hierarchy`
Expected: FAIL.

- [ ] **Step 3: Implement `default_models` map and 6-tier resolution**

Add `default_models: HashMap<String, String>` to `ProviderConfig` and `RawProviderConfig`.
Implement `resolve_active_provider_and_model`:
1. Check CLI env overrides (`MINICODE_PROVIDER`, `MINICODE_MODEL`).
2. Check local `.minicode/config.toml`.
3. Check `workspaces.toml` for `workspace_root`.
4. Check global `self.provider.default` / `self.provider.model`.
5. Check configured providers auto-discovery (`self.find_first_configured_provider()`).
6. If none found, return `("", "")`.

Update `get_default_model_for_provider(&self, provider_name: &str) -> String`:
Checks `self.provider.default_models.get(provider_name)` first before falling back to static default catalog.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -j 1 --lib config::tests::test_dynamic_provider_resolution_hierarchy`
Expected: PASS

- [ ] **Step 5: Commit changes**

```bash
git add src/config.rs src/app/commands.rs
git commit -m "feat(config): eliminate hardcoded defaults with 6-tier dynamic provider resolution"
```

---

### Task 3: Dedicated `agent_config` Tool Category (Tools 136 – 139)

**Files:**
- Create: `src/tools/registry/agent_tools/config_tools.rs`
- Modify: `src/tools/registry/agent_tools/mod.rs`
- Modify: `src/tools/registry/mod.rs`
- Modify: `src/constants.rs:40-60` (`TOTAL_TOOL_COUNT = 139`)
- Test: `src/tools/registry/agent_tools/config_tools.rs` (unit tests)
- Test: `src/tools/tests.rs` (`test_total_tool_count`)

**Interfaces:**
- Consumes: `crate::config::{Config, mask_api_key}`, `crate::tools::core::{Tool, ToolResult, ToolError}`
- Produces:
  - `GetAgentConfigTool` (Tool 136)
  - `UpdateAgentConfigTool` (Tool 137)
  - `TestProviderConnectionTool` (Tool 138)
  - `ListAvailableModelsTool` (Tool 139)

- [ ] **Step 1: Update TOTAL_TOOL_COUNT to 139 and write failing tool count test**

In `src/constants.rs`:
```rust
pub const TOTAL_TOOL_COUNT: usize = 139;
```
Run: `cargo test -j 1 --lib tools::tests::test_total_tool_count`
Expected: FAIL with count mismatch (135 != 139).

- [ ] **Step 2: Create `src/tools/registry/agent_tools/config_tools.rs`**

Implement tool schemas and handlers:
1. `get_agent_config`: returns JSON with active provider, model, auto_approve, thinking_budget, theme, and masked keys.
2. `update_agent_config`: validates params and returns structured proposal `{ "action": "update_config", "changes": { ... }, "scope": ... }` requiring user confirmation.
3. `test_provider_connection`: executes internal HTTP ping via `reqwest` to provider models endpoint, measuring latency without exposing credentials.
4. `list_available_models`: queries models for provider.

- [ ] **Step 3: Register new tools in `src/tools/registry/mod.rs` and `agent_tools/mod.rs`**

Add `pub mod config_tools;` to `agent_tools/mod.rs`.
Register `GetAgentConfigTool`, `UpdateAgentConfigTool`, `TestProviderConnectionTool`, `ListAvailableModelsTool` into tool registry.

- [ ] **Step 4: Run tests to verify tool count and tool execution**

Run: `cargo test -j 1 --lib tools::tests::test_total_tool_count`
Run: `cargo test -j 1 --lib tools::registry::agent_tools::config_tools::tests`
Expected: PASS (139 tools verified).

- [ ] **Step 5: Commit changes**

```bash
git add src/constants.rs src/tools/registry/agent_tools/config_tools.rs src/tools/registry/agent_tools/mod.rs src/tools/registry/mod.rs
git commit -m "feat(tools): implement agent_config tool suite (Tools 136-139)"
```

---

### Task 4: Interactive In-TUI `/settings` Command Modal & Permission Confirmation Card

**Files:**
- Create: `src/ui/modals/settings.rs`
- Modify: `src/ui/modals/mod.rs`
- Modify: `src/ui/modal.rs`
- Modify: `src/app/commands.rs`
- Modify: `src/app/modals.rs`

**Interfaces:**
- Consumes: `ModalState::Settings`, `ModalState::ConfigApproval`
- Produces:
  - `ModalState::new_settings(config: &Config, workspace_root: &Path) -> ModalState`
  - `ModalState::new_config_approval(proposal: ConfigChangeProposal) -> ModalState`
  - In-TUI Tabbed Settings UI with keyboard navigation (`Tab`, `↑`/`↓`, `Enter`, `Esc`)

- [ ] **Step 1: Define `ModalState::Settings` and `ModalState::ConfigApproval` in `src/ui/modal.rs`**

Add enum variants with tab states, selected index, and proposal fields.

- [ ] **Step 2: Create renderer `src/ui/modals/settings.rs`**

Implement Ratatui rendering for:
- Tab 1: Providers & Default Models
- Tab 2: Workspace Memory & Scope
- Tab 3: Agent Autonomy & Thinking Budget
- Tab 4: Connection Probes

- [ ] **Step 3: Wire keyboard dispatch in `src/app/modals.rs`**

Handle tab switching, provider model selection, saving to workspace/global config, and permission card approval/denial.

- [ ] **Step 4: Wire `/settings`, `/config` slash commands in `src/app/commands.rs`**

Open `ModalState::new_settings(&self.config, &self.workspace_root)`.

- [ ] **Step 5: Verify compilation and commit**

Run: `cargo check -j 1`
```bash
git add src/ui/modal.rs src/ui/modals/settings.rs src/ui/modals/mod.rs src/app/commands.rs src/app/modals.rs
git commit -m "feat(ui): implement interactive in-TUI /settings modal and config permission card"
```

---

### Task 5: End-to-End Integration Test Suite & Subagent Verification

**Files:**
- Create: `tests/integration_agent_config.rs`
- Modify: `onpkg_docs/todo.md`
- Modify: `Cargo.toml` (`v0.3.39`)

**Interfaces:**
- Consumes: `App`, `Config`, `AgentCommand`, `ModalState`
- Produces: Comprehensive automated integration tests covering all Phase 137 features

- [ ] **Step 1: Write integration tests in `tests/integration_agent_config.rs`**

Cover:
1. `test_per_directory_model_memory`: Switching provider/model in Dir A remembers it when reopened.
2. `test_settings_command_modal`: Slash command `/settings` opens `ModalState::Settings`.
3. `test_agent_config_tools_and_masking`: `get_agent_config` returns masked keys and accurate state.
4. `test_update_agent_config_permission_gate`: Mutating config requires user approval before updating actors.
5. `test_connection_probe_diagnostic_format`: Probing connection yields structured diagnostics.

- [ ] **Step 2: Run integration tests**

Run: `cargo test -j 1 --test integration_agent_config`
Expected: PASS (all tests pass).

- [ ] **Step 3: Run quality gates & clippy**

Run: `cargo clippy -j 1 --bin minicode -- -D warnings`
Run: `cargo fmt --check`
Expected: 0 warnings, clean exit.

- [ ] **Step 4: Bump version to `v0.3.39` and recompile release binary**

Update `Cargo.toml` and `onpkg_docs/todo.md`.
Run: `./localupdate.sh`
Expected: Global binary updated to `v0.3.39`.

- [ ] **Step 5: Commit and push**

```bash
git add tests/integration_agent_config.rs Cargo.toml Cargo.lock onpkg_docs/todo.md
git commit -m "feat(config): complete Phase 137 autonomous configuration and workspace memory (v0.3.39)"
git push origin main
```

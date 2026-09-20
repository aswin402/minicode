# Phase 137: Autonomous Configuration, Per-Directory Memory & Zero-Leak Diagnostics — Design Specification

> **Version:** 1.0  
> **Status:** Approved  
> **Date:** 2026-09-20  
> **Target Release:** `minicode v0.3.39`  

---

## 1. Executive Summary

minicode currently hardcodes `"gemini"` and `"gemini-2.5-pro"` as global defaults, does not persist per-directory provider/model choices across sessions, lacks an in-TUI `/settings` interface for managing per-provider default models, and offers no mechanism for the AI agent to inspect, probe, or safely modify its own configuration upon user request.

Phase 137 introduces:
1. **Dynamic Provider & Model Resolution:** Eliminates hardcoded fallbacks in favor of a 6-tier resolution hierarchy.
2. **Dual-Layer Workspace Memory:** Automatically remembers the last-used provider and model per repository in both `.minicode/config.toml` and a global registry `~/.config/minicode/workspaces.toml`.
3. **In-TUI Interactive `/settings` Command:** A tabbed settings modal allowing users to configure per-provider default models, agent autonomy, reasoning budgets, and run connection probes.
4. **Dedicated `agent_config` Tool Category (Tools 136–139):** 4 new tool primitives (`get_agent_config`, `update_agent_config`, `test_provider_connection`, `list_available_models`) protected by an interactive TUI permission card for any mutating operations.
5. **Zero-Leak API Key Masking & Diagnostic Probing:** Strict universal masking across all tool returns and logs, with an internal HTTP probe mechanism that verifies credential validity without exposing secret keys.
6. **Real-World Multi-Agent Verification:** Rigorous end-to-end integration test coverage and subagent execution testing.

---

## 2. Architecture & System Flow

```
+-------------------------------------------------------------------------+
|                           User / TUI / CLI                              |
+-------------------------------------------------------------------------+
           |                                             |
           | (CLI / TUI Prompt)                          | (/settings)
           v                                             v
+-----------------------+                    +---------------------------+
| 6-Tier Provider       |                    | Interactive Settings      |
| Resolution Engine     |                    | Modal (Tabbed TUI)        |
+-----------------------+                    +---------------------------+
           |                                             |
           v                                             v
+-------------------------------------------------------------------------+
| Dual-Layer Persistence Layer:                                           |
| 1. Project Local: <workspace>/.minicode/config.toml                     |
| 2. Global Registry: ~/.config/minicode/workspaces.toml                  |
+-------------------------------------------------------------------------+
           ^                                             ^
           |                                             |
+-------------------------------------------------------------------------+
| Dedicated agent_config Tool Category (Tools 136 - 139)                 |
|                                                                         |
| • get_agent_config (Read-only, masked API keys)                         |
| • update_agent_config (Mutating -> TUI Permission Card Gate)            |
| • test_provider_connection (Zero-leak HTTP credential probe)            |
| • list_available_models (Live/curated model enumeration)                |
+-------------------------------------------------------------------------+
```

---

## 3. Dynamic Provider & Model Resolution

### 3.1 The 6-Tier Resolution Hierarchy

When minicode starts in any directory, the active provider and model are resolved in the following deterministic order:

1. **Explicit CLI Flags:** `--provider <name>`, `--model <name>` (highest precedence).
2. **Environment Variables:** `MINICODE_PROVIDER`, `MINICODE_MODEL`.
3. **Workspace-Scoped Memory (Per-Directory):**
   - Check `<workspace>/.minicode/config.toml` `[provider]` section.
   - If not found, check `~/.config/minicode/workspaces.toml` for the canonical path of `<workspace>`.
4. **Global User Configuration:** `~/.config/minicode/config.toml` `[provider]` section.
5. **Configured Provider Auto-Discovery:**
   - minicode checks known providers for existing credentials in the environment or global config (`ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, `GEMINI_API_KEY`, `GROQ_API_KEY`, `DEEPSEEK_API_KEY`, `MISTRAL_API_KEY`, `TOGETHER_API_KEY`, `MINIMAX_API_KEY`, `ZHIPU_API_KEY`).
   - If exactly one cloud provider is configured, minicode selects it.
   - If local Ollama or LM Studio is detected on localhost, it can be auto-selected.
6. **Deferred Onboarding (Empty Fallback):**
   - If no provider or key is found anywhere, `provider.default = ""` and `provider.model = ""`.
   - minicode boots silently into the intro screen without error. When the user enters their first prompt, **Gate 1** (`ProviderSetupRequired`) naturally prompts the user to configure their key via the setup wizard.

---

## 4. Per-Directory Workspace Memory & Dual-Layer Persistence

### 4.1 Schema: `~/.config/minicode/workspaces.toml`

```toml
[workspaces."/home/aswin/programming/myProjects/project-alpha"]
provider = "anthropic"
model = "claude-3-7-sonnet-20250219"
last_used = "2026-09-20T21:30:00Z"

[workspaces."/home/aswin/programming/myProjects/project-beta"]
provider = "ollama"
model = "qwen2.5-coder:latest"
last_used = "2026-09-20T19:15:00Z"
```

### 4.2 Synchronization Rules
- Whenever a user selects or switches provider/model:
  1. If `<workspace>/.minicode/` exists, write to `<workspace>/.minicode/config.toml`.
  2. Always upsert the entry in `~/.config/minicode/workspaces.toml`.
- When switching workspaces or starting minicode:
  - Canonicalize `std::env::current_dir()`.
  - Look up entry in `workspaces.toml`.
  - If found and valid, load it as default provider and model.

---

## 5. In-TUI `/settings` Command

### 5.1 Trigger & Key Bindings
- **Commands:** `/settings`, `/config`, `/preferences`.
- **Keyboard Navigation:**
  - `Tab` / `BackTab`: Switch active category tabs.
  - `↑` / `↓` / `k` / `j`: Move selection within the active tab.
  - `Enter`: Select, open submenu, or toggle option.
  - `Esc` / `q`: Exit settings modal back to chat.

### 5.2 Category Tabs
1. **Tab 1: Providers & Default Models**
   - Lists all supported providers (`Anthropic`, `OpenAI`, `Google Gemini`, `OpenRouter`, `Groq`, `DeepSeek`, `Mistral`, `Together`, `MiniMax`, `Zhipu AI`, `Ollama`, `LM Studio / Local`).
   - Selecting a provider displays its model selector to set that provider's persistent default model.
   - Saved in `[provider.default_models]` table in config.
2. **Tab 2: Workspace Memory & Scope**
   - Shows current workspace directory and its assigned provider/model.
   - Toggle: "Save changes to Workspace only" vs "Save changes Globally".
3. **Tab 3: Autonomy & Reasoning Parameters**
   - Toggle `auto_approve` (`true`/`false`).
   - Configure `thinking_budget` (`Off`, `4096`, `8192`, `16384`, `32768`).
   - Set `approval_policy` (`all`, `danger`, `mutation_only`).
4. **Tab 4: Connection Probes**
   - "Test All Configured Providers" action.
   - Executes live zero-leak HTTP pings, rendering a live status card with response times and verification badges.

### 5.3 CLI Shortcuts
- `/settings model <provider> <model>`: Directly set default model for provider.
- `/settings auto_approve <on|off>`: Directly toggle auto-approve.
- `/settings thinking <tokens>`: Directly configure reasoning token budget.

---

## 6. Dedicated `agent_config` Tool Category

### 6.1 Tool Definitions (Tools 136 – 139)

#### 1. `get_agent_config` (Tool 136)
- **Category:** `agent_config`
- **Mutability:** Read-Only (No permission prompt)
- **Parameters:** None
- **Return Value:**
  ```json
  {
    "active_provider": "anthropic",
    "active_model": "claude-3-7-sonnet-20250219",
    "workspace_scope": "/home/aswin/programming/myProjects/project-alpha",
    "auto_approve": false,
    "approval_policy": "danger",
    "thinking_budget": 8192,
    "theme": "dark",
    "configured_providers": ["anthropic", "ollama"],
    "masked_keys": {
      "anthropic": "sk-a...3456"
    }
  }
  ```

#### 2. `update_agent_config` (Tool 137)
- **Category:** `agent_config`
- **Mutability:** Mutating (**Requires User Permission**)
- **Parameters:**
  - `provider`: `Option<String>`
  - `model`: `Option<String>`
  - `auto_approve`: `Option<bool>`
  - `thinking_budget`: `Option<usize>`
  - `theme`: `Option<String>`
  - `scope`: `Option<String>` (`"workspace"` | `"global"`, defaults to `"workspace"`)
- **Permission Gate:**
  Pauses the agent loop and presents an in-TUI confirmation dialog:
  ```text
  ⚙️ minicode requests permission to modify configuration:
     • Active Provider : ollama → anthropic
     • Active Model    : qwen2.5-coder → claude-3-7-sonnet-20250219
     • Scope           : Workspace (.minicode/config.toml)
  [1] Allow Change   [2] Deny Change
  ```
  On confirmation, updates running actor state (`AgentCommand::UpdateConfig`), writes to disk, and resumes turn.

#### 3. `test_provider_connection` (Tool 138)
- **Category:** `agent_config`
- **Mutability:** Read-Only (No permission prompt)
- **Parameters:**
  - `provider`: `String` (e.g. `"anthropic"`, `"gemini"`, `"all"`)
- **Execution:**
  Resolves API key internally from environment or config. Initiates a non-destructive ping (e.g. `/v1/models` or lightweight probe).
- **Return Value:**
  ```json
  {
    "provider": "anthropic",
    "status": "connected",
    "latency_ms": 138,
    "http_status": 200,
    "key_source": "environment (ANTHROPIC_API_KEY)",
    "key_masked": "sk-a...3456",
    "model_count": 8,
    "error": null
  }
  ```

#### 4. `list_available_models` (Tool 139)
- **Category:** `agent_config`
- **Mutability:** Read-Only (No permission prompt)
- **Parameters:**
  - `provider`: `String`
- **Return Value:**
  Returns list of available model IDs, friendly names, context lengths, and reasoning support.

---

## 7. Zero-Leak API Key Masking & Safety Invariants

1. **Universal Masking Function:**
   ```rust
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
   ```
2. **Zero-Leak Invariants:**
   - Raw API keys are NEVER passed as arguments to tools.
   - Raw API keys are NEVER returned in tool results.
   - Raw API keys are NEVER written to logs or displayed in plaintext on screen.
   - In `get_agent_config` and `test_provider_connection`, all keys are sanitized via `mask_api_key`.

---

## 8. Total Tool Count Update

- Previous tool count: **135**
- New tools added:
  1. `get_agent_config` (+1)
  2. `update_agent_config` (+1)
  3. `test_provider_connection` (+1)
  4. `list_available_models` (+1)
- New total tool count: **139**
- Synchronized in `src/constants.rs` (`TOTAL_TOOL_COUNT = 139`) and validated via `test_total_tool_count`.

---

## 9. Testing & Verification Strategy

1. **Unit Tests:**
   - `test_mask_api_key`: Asserts correct masking across short, standard, and long keys.
   - `test_workspace_registry_persistence`: Asserts save/load round-trip in `workspaces.toml`.
   - `test_dynamic_provider_resolution`: Asserts correct 6-tier fallback behavior.
   - `test_total_tool_count`: Asserts `TOTAL_TOOL_COUNT == 139`.
2. **Integration Tests (`tests/integration_agent_config.rs`):**
   - `test_per_directory_model_memory`: Switches model in Dir A, verifies Dir B is unaffected, returns to Dir A and verifies memory restored.
   - `test_settings_command_modal`: Verifies `/settings` updates per-provider default models.
   - `test_agent_config_tools`: Verifies `get_agent_config`, `test_provider_connection`, `update_agent_config` with confirmation gate.
   - `test_api_key_never_leaked`: Scans all tool outputs for plaintext credentials.
3. **Subagent Real-World Validation:**
   - Spawns a subagent instructing it to configure settings, probe connections, and report diagnostics.
4. **Quality Gates:**
   - `cargo check -j 1`
   - `cargo test -j 1` (targeted test suites)
   - `cargo clippy -j 1 --bin minicode -- -D warnings`
   - `cargo fmt --check`
   - Production build via `./localupdate.sh`.

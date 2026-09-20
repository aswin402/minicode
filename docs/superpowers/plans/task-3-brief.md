# Task 3 Brief: Dedicated `agent_config` Tool Category (Tools 136 – 139)

## Requirements
1. In `src/constants.rs`:
   - Update `TOTAL_TOOL_COUNT` from 135 to 139:
     ```rust
     pub const TOTAL_TOOL_COUNT: usize = 139;
     ```

2. Create `src/tools/registry/agent_tools/config_tools.rs`:
   Implement the 4 tools with ToolSchema, parameter parsing, execution, and zero plaintext credential leakage:

   - **Tool 136: `get_agent_config`**
     - Description: "Inspect current agent configuration, active provider/model, workspace scope, execution preferences, and masked provider API keys."
     - Parameters: empty object.
     - Execution: Loads `Config::load(Some(workspace_root))` (or fallback to `Config::default()`).
     - Returns JSON with:
       - `active_provider`: `config.provider.default`
       - `active_model`: `config.provider.model`
       - `workspace_scope`: `workspace_root.display().to_string()`
       - `auto_approve`: `config.agent.auto_approve`
       - `approval_policy`: `config.agent.approval_policy`
       - `thinking_budget`: `config.agent.thinking_budget`
       - `theme`: `config.theme`
       - `configured_providers`: list of configured cloud and local providers
       - `masked_keys`: map of `{ provider: crate::config::mask_api_key(&key) }` across env vars and `config.provider.api_keys`. NEVER expose raw keys!

   - **Tool 137: `update_agent_config`**
     - Description: "Request an update to the agent configuration (active provider, model, auto_approve, thinking budget, theme). Proposes structured modifications requiring explicit user approval."
     - Parameters:
       - `provider`: optional string
       - `model`: optional string
       - `auto_approve`: optional boolean
       - `thinking_budget`: optional integer
       - `theme`: optional string
       - `scope`: optional string enum `["workspace", "global"]`, defaults to `"workspace"`
     - Execution:
       - Validates that at least one field to change is provided.
       - Constructs `ConfigChangeProposal`.
       - Provides helper `apply_proposal(proposal: &ConfigChangeProposal, workspace_root: &Path) -> Result<()>` that can persist to workspace `.minicode/config.toml` (and `workspaces.toml`) or global config.
       - Returns structured proposal JSON with status `proposal_generated`, `requires_confirmation: true`.

   - **Tool 138: `test_provider_connection`**
     - Description: "Test connectivity, latency, and credential validity for an AI model provider endpoint without exposing raw API keys."
     - Parameters:
       - `provider`: required string (e.g. `"anthropic"`, `"openai"`, `"gemini"`, `"openrouter"`, `"deepseek"`, `"groq"`, `"mistral"`, `"ollama"`, `"lmstudio"`, or `"all"`)
     - Execution:
       - For specified provider (or each provider if `"all"`):
         - Resolves API key via `config.get_api_key(p)` and custom base URL if any.
         - Measures elapsed latency in milliseconds.
         - Pings endpoint via `ModelFetcher::new().fetch_models(p, &key, custom_url).await`.
         - Determines key source: `"environment (<VAR_NAME>)"` or `"config.toml ([provider.api_keys])"` or `"none (local endpoint)"`.
         - Returns:
           ```json
           {
             "provider": "...",
             "status": "connected" / "disconnected" / "unconfigured",
             "latency_ms": 123,
             "http_status": 200,
             "key_source": "...",
             "key_masked": "sk-a...3456",
             "model_count": 8,
             "error": null
           }
           ```
         - Zero plaintext credential leakage!

   - **Tool 139: `list_available_models`**
     - Description: "List available AI models from a provider with context window lengths and capabilities."
     - Parameters:
       - `provider`: required string
     - Execution:
       - Resolves API key and custom base URL.
       - Fetches live models using `ModelFetcher::new().fetch_models(&provider, &key, custom_url).await`.
       - If live fetch fails or empty, falls back gracefully to known model catalog defaults for that provider.
       - Returns JSON array of models with `id`, `name`, `context_length`, `supports_reasoning`.

3. Register Tools:
   - In `src/tools/registry/agent_tools/mod.rs`:
     - Add `pub mod config_tools;`
     - Include `config_tools::get_schemas()` in `get_schemas()` (increase capacity to 38)
     - Include `config_tools::dispatch` in `dispatch()`
   - Note: Category is automatically `ToolCategory::Agent`.

4. Follow TDD:
   - Add unit tests in `src/tools/registry/agent_tools/config_tools.rs`:
     - `test_config_tools_schemas` (verifies 4 schemas present with required fields)
     - `test_get_agent_config_masked_keys` (verifies raw keys are NEVER present, only masked)
     - `test_update_agent_config_proposal` (verifies proposal structure and validation)
     - `test_test_provider_connection_zero_leak` (verifies test connection output schema and masking)
   - Targeted test runs:
     `cargo test -j 1 --lib tools::tests::test_total_tool_count`
     `cargo test -j 1 --lib tools::registry::agent_tools::config_tools::tests`
     `cargo test -j 1 --lib constants::tests::test_total_tool_count`
   - Quality checks:
     `cargo fmt --check`
     `cargo clippy -j 1 --bin minicode -- -D warnings`

5. Commit message:
   `feat(tools): implement agent_config tool suite (Tools 136-139)`

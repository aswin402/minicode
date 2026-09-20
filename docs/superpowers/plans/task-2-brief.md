# Task 2 Brief: Dynamic 6-Tier Provider Resolution & Default Models Map

## Requirements
1. In `src/config.rs`:
   - Add `default_models: std::collections::HashMap<String, String>` to `ProviderConfig` (with `#[serde(default)]`) and `RawProviderConfig`.
   - Update `ProviderConfig::default()` so it does NOT hardcode "gemini" or "gemini-2.5-pro":
     ```rust
     default: String::new(),
     model: String::new(),
     default_models: std::collections::HashMap::new(),
     ```
   - Update `RawConfig::merge_raw` to merge `default_models`.
2. Implement 6-Tier Resolution on `Config`:
   - `pub fn find_first_configured_provider(&self) -> Option<(&str, &str)>`
     Checks known providers in order: `anthropic`, `gemini`, `openai`, `openrouter`, `deepseek`, `groq`, `mistral`, `together`, `minimax`, `z.ai`, and local providers (`ollama`, `lmstudio`). If any has a valid key (or is local), returns `(provider_name, default_model)`.
   - `pub fn resolve_active_provider_and_model(&self, workspace_root: Option<&Path>) -> (String, String)`:
     1. If `!self.provider.default.is_empty() && !self.provider.model.is_empty()` (e.g. from CLI flag or env var override): return `(self.provider.default.clone(), self.provider.model.clone())`.
     2. Check workspace preference: if `workspace_root` has a saved preference in `load_workspace_preference(ws)`: return that provider and model!
     3. Check `find_first_configured_provider()`: if found, return it!
     4. If none found, return `(String::new(), String::new())` (empty fallback, triggering Gate 1 deferred setup when prompt arrives).
   - Implement `resolve_active_provider_and_model_with_registry(workspace_root: Option<&Path>, registry_path: Option<&Path>) -> (String, String)` for deterministic testing.
   - Update `get_default_model_for_provider(&self, provider_name: &str) -> String`:
     First checks `self.provider.default_models.get(provider_name)`; if present and not empty, returns it! Otherwise calls `Config::get_default_model_for_provider(provider_name)`.
3. In `src/app/commands.rs`:
   - In Gate 1 check:
     If `self.config.provider.default.is_empty()`: automatically trigger Gate 1 `ModalState::new_provider_setup_required("", &prompt_to_run)`.
4. In `Config::load`:
   - After merging raw configs, call `resolve_active_provider_and_model(workspace_dir)`:
     If `config.provider.default.is_empty()` or `config.provider.model.is_empty()`:
     fill them in with the resolved provider and model!
5. Follow TDD:
   - Add unit test `test_dynamic_provider_resolution_hierarchy` and `test_default_models_override` in `src/config.rs`.
   - Targeted tests:
     `cargo test -j 1 --lib config::tests::test_dynamic_provider_resolution_hierarchy`
     `cargo test -j 1 --lib config::tests::test_default_models_override`
   - Run formatting: `cargo fmt`
   - Run clippy: `cargo clippy -j 1 --bin minicode -- -D warnings`
6. Commit with message: `feat(config): eliminate hardcoded defaults with 6-tier dynamic provider resolution`

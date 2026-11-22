use super::{
    set_active_provider_timeouts, Config, IntentConfig, McpConfig, McpServerConfig,
    MiniPubModelChain, ModelSlot, OllamaConfig, RawBrowserConfig, ToolFilterMode,
    RUNTIME_TIMEOUTS_OVERRIDDEN,
};
use crate::constants::env_vars;
use crate::error::{ConfigError, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct McpJsonFile {
    #[serde(default, rename = "mcpServers")]
    pub mcp_servers: std::collections::HashMap<String, McpServerConfig>,
    #[serde(default)]
    pub servers: std::collections::HashMap<String, McpServerConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RawGitConfig {
    pub auto_commit: Option<bool>,
    pub dirty_commit: Option<bool>,
    pub ai_commit_messages: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RawProviderConfig {
    pub default: Option<String>,
    pub model: Option<String>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<usize>,
    pub ollama: Option<OllamaConfig>,
    pub api_keys: Option<std::collections::HashMap<String, String>>,
    pub custom_endpoints: Option<std::collections::HashMap<String, String>>,
    pub context_window: Option<usize>,
    pub prompt_cost_per_m: Option<f64>,
    pub completion_cost_per_m: Option<f64>,
    pub thinking_budget: Option<usize>,
    pub reasoning_effort: Option<String>,
    pub default_models: Option<std::collections::HashMap<String, String>>,
    pub fallback_1: Option<ModelSlot>,
    pub fallback_2: Option<ModelSlot>,
    pub global_last_fallback: Option<ModelSlot>,
    pub favorite_models: Option<Vec<ModelSlot>>,
    pub connect_timeout_secs: Option<u64>,
    pub stream_idle_timeout_secs: Option<u64>,
    pub adaptive_timeout: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RawMiniPubConfig {
    pub max_parallel: Option<usize>,
    pub default_token_budget: Option<usize>,
    pub default_max_turns: Option<usize>,
    pub inactivity_timeout_secs: Option<u64>,
    pub worktree_isolation: Option<String>,
    pub extra_allowed_tools: Option<Vec<String>>,
    pub sync_with_orchestrator: Option<bool>,
    pub default_chain: Option<MiniPubModelChain>,
    pub role_models: Option<std::collections::HashMap<String, MiniPubModelChain>>,
    pub category_models: Option<std::collections::HashMap<String, MiniPubModelChain>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RawAgentConfig {
    pub auto_approve: Option<bool>,
    pub approval_policy: Option<String>,
    pub timeout: Option<u64>,
    pub map_tokens: Option<usize>,
    pub warning_threshold: Option<f32>,
    pub auto_heal: Option<bool>,
    pub streaming: Option<bool>,
    pub caveman_mode: Option<bool>,
    pub caveman_level: Option<String>,
    pub ponytail_mode: Option<bool>,
    pub ponytail_level: Option<String>,
    pub tool_mode: Option<ToolFilterMode>,
    pub syntax_barrier: Option<bool>,
    pub auto_lint: Option<bool>,
    pub parallel_tools: Option<bool>,
    pub speculative_execution: Option<bool>,
    pub max_parallel_tools: Option<usize>,
    pub compact_tool_schemas: Option<bool>,
    pub max_tool_iterations: Option<usize>,
    pub auto_continue: Option<bool>,
    pub max_auto_continues: Option<usize>,
    pub intent: Option<IntentConfig>,
    pub plan_approval: Option<bool>,
    pub plan_review: Option<String>,
    #[serde(alias = "subagent", alias = "subagents")]
    pub minipub: Option<RawMiniPubConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RawUiConfig {
    pub plain: Option<bool>,
    pub theme: Option<String>,
    pub animation: Option<String>,
    pub todo_style: Option<String>,
    pub swarm_style: Option<String>,
    pub toast_style: Option<String>,
    pub toast_symbols: Option<String>,
    pub activity_style: Option<String>,
    pub menu_style: Option<String>,
    pub input_box_style: Option<String>,
    pub status_bar_style: Option<String>,
    pub status_model_style: Option<String>,
    pub status_effort_style: Option<String>,
    pub status_path_style: Option<String>,
    pub status_git_style: Option<String>,
    pub status_badges_style: Option<String>,
    pub context_meter_style: Option<String>,
    pub mode_bar_style: Option<String>,
    pub activity_hint_style: Option<String>,
    pub tool_card_style: Option<String>,
    pub tool_icon_style: Option<String>,
    pub max_width: Option<usize>,
    pub show_cost: Option<bool>,
    pub show_ponytail_badge: Option<bool>,
    pub show_caveman_badge: Option<bool>,
    pub show_streaming_badge: Option<bool>,
    pub show_kv_cache: Option<bool>,
    pub show_context_meter: Option<bool>,
    pub show_status_model: Option<bool>,
    pub show_status_effort: Option<bool>,
    pub show_status_path: Option<bool>,
    pub show_status_git: Option<bool>,
    pub show_status_badges: Option<bool>,
    pub show_interrupt_hint: Option<bool>,
    pub show_thought_tokens: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RawLoggingConfig {
    pub level: Option<String>,
    pub file: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RawConfig {
    #[serde(default)]
    pub provider: RawProviderConfig,
    #[serde(default)]
    pub agent: RawAgentConfig,
    #[serde(default)]
    pub ui: RawUiConfig,
    #[serde(default)]
    pub logging: RawLoggingConfig,
    #[serde(default)]
    pub mcp: McpConfig,
    #[serde(default)]
    pub git: RawGitConfig,
    #[serde(default)]
    pub browser: RawBrowserConfig,
}

impl Config {
    /// Synchronizes configured provider and command execution timeouts into the process-wide runtime atomics.
    pub fn sync_runtime_timeouts(&self) {
        if !RUNTIME_TIMEOUTS_OVERRIDDEN.load(std::sync::atomic::Ordering::Relaxed) {
            set_active_provider_timeouts(
                self.provider.effective_connect_timeout_secs(),
                self.provider.effective_stream_idle_timeout_secs(),
            );
        }
        crate::tools::exec::set_configured_exec_timeout(self.agent.timeout);
    }

    /// Convenience helper that loads configuration for `workspace_dir` or falls back to defaults.
    pub fn load_for_workspace(workspace_dir: &Path) -> Self {
        Self::load(Some(workspace_dir), None).unwrap_or_default()
    }

    /// Returns `true` if `exe_path` looks like a Cargo unit/integration test binary
    /// (`.../deps/<crate_or_test>-<hex_hash>[.exe]`), preventing integration tests compiled without
    /// `cfg(test)` inside `libminicode` from touching the developer's `~/.config/minicode`.
    pub(crate) fn exe_looks_like_cargo_test_binary(exe_path: &Path) -> bool {
        let in_deps = exe_path
            .parent()
            .and_then(|d| d.file_name())
            .is_some_and(|name| name == "deps");
        if !in_deps {
            return false;
        }
        exe_path
            .file_stem()
            .and_then(|s| s.to_str())
            .and_then(|stem| stem.rsplit_once('-'))
            .is_some_and(|(_, hash)| hash.len() >= 8 && hash.chars().all(|c| c.is_ascii_hexdigit()))
    }

    /// Returns `true` when running inside a Rust unit/integration test harness or when
    /// `MINICODE_ISOLATED_TEST` is explicitly set.
    pub(crate) fn is_isolated_test_env() -> bool {
        cfg!(test)
            || std::env::var_os("MINICODE_ISOLATED_TEST").is_some()
            || std::env::current_exe()
                .ok()
                .is_some_and(|p| Self::exe_looks_like_cargo_test_binary(&p))
    }

    /// Evaluates whether `workspace_root` should be treated as an isolated temporary test workspace.
    pub(crate) fn is_isolated_temp_workspace_with_env(
        workspace_root: &Path,
        is_test_env: bool,
    ) -> bool {
        is_test_env && workspace_root.starts_with(std::env::temp_dir())
    }

    /// Resolves the global configuration directory (`~/.config/minicode`), honoring `MINICODE_CONFIG_DIR`.
    ///
    /// In unit/integration test harnesses (`is_isolated_test_env()`), returns `None` unless
    /// `MINICODE_CONFIG_DIR` is explicitly set so tests never clobber or read the developer's
    /// live `~/.config/minicode/config.toml`. Real CLI binaries (including `--dir /tmp/...`)
    /// always resolve `~/.config/minicode` normally.
    #[must_use]
    pub fn resolve_global_config_dir() -> Option<std::path::PathBuf> {
        if let Ok(custom) = std::env::var("MINICODE_CONFIG_DIR") {
            let trimmed = custom.trim();
            if !trimmed.is_empty() {
                return Some(std::path::PathBuf::from(trimmed));
            }
        }
        if Self::is_isolated_test_env() {
            None
        } else {
            dirs::config_dir().map(|d| d.join(crate::constants::CONFIG_DIR_NAME))
        }
    }

    /// Loads configuration respecting the hierarchy:
    /// 1. Global `~/.config/minicode/config.toml` (if present)
    /// 2. Project-local `.minicode/config.toml` (if present; newer global `[ui]`/autonomy preferences win over stale snapshots)
    /// 3. Global `~/.config/minicode/.env` (without letting stale `.env` keys shadow newer `config.toml` `[provider.api_keys]`) and workspace `.env`
    /// 4. Project-local `mcp.json` or `.minicode/mcp.json` (if present)
    /// 5. Environment variable overrides (`MINICODE_*` and explicit shell API keys)
    pub fn load(workspace_dir: Option<&Path>, custom_config_path: Option<&Path>) -> Result<Self> {
        let global_cfg_dir = Self::resolve_global_config_dir();
        let mut config = Config::default();
        let mut global_toml_mtime: Option<std::time::SystemTime> = None;

        // 1. Try custom config path if explicitly specified
        if let Some(path) = custom_config_path {
            if let Some(raw) = Self::load_raw_from_file(path)? {
                config.merge_raw(raw);
            }
        } else {
            // 2. Global config: ~/.config/minicode/config.toml
            let mut global_loaded: Option<(RawConfig, std::time::SystemTime)> = None;
            if let Some(ref global_dir) = global_cfg_dir {
                let global_config = global_dir.join(crate::constants::CONFIG_FILE_NAME);
                let g_mtime = std::fs::metadata(&global_config)
                    .ok()
                    .and_then(|m| m.modified().ok());
                if let Some(raw) = Self::load_raw_from_file(&global_config)? {
                    if let Some(mt) = g_mtime {
                        global_toml_mtime = Some(mt);
                        global_loaded = Some((raw.clone(), mt));
                    }
                    config.merge_raw(raw);
                }
            }

            // 3. Project-local config: <workspace>/.minicode/config.toml
            if let Some(dir) = workspace_dir {
                let local_config = dir
                    .join(crate::constants::WORKSPACE_DIR_NAME)
                    .join(crate::constants::CONFIG_FILE_NAME);
                let l_mtime = std::fs::metadata(&local_config)
                    .ok()
                    .and_then(|m| m.modified().ok());
                if let Some(raw) = Self::load_raw_from_file(&local_config)? {
                    config.merge_raw(raw);
                    // If the user updated Global `~/.config/minicode/config.toml` MORE RECENTLY than an older
                    // workspace `.minicode/config.toml`, preserve their newer global UI theme & autonomy modes!
                    if let (Some((g_raw, g_mt)), Some(l_mt)) = (global_loaded, l_mtime) {
                        if g_mt > l_mt {
                            config.merge_ui_and_autonomy_from_raw(&g_raw);
                        }
                    }
                }
            }
        }

        // 3b. Load global ~/.config/minicode/.env (preserving newer `config.toml` keys & shell env)
        if let Some(ref global_dir) = global_cfg_dir {
            let global_env = global_dir.join(crate::constants::ENV_FILE_NAME);
            config.load_global_dotenv_respecting_toml(&global_env, global_toml_mtime);
        }

        // 3c. Load workspace .env if present
        if let Some(dir) = workspace_dir {
            let env_file = dir.join(crate::constants::ENV_FILE_NAME);
            match dotenvy::from_path(&env_file) {
                Ok(_) => {}
                Err(dotenvy::Error::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => {
                    tracing::warn!(path = %env_file.display(), error = %e, "Failed to parse workspace .env file");
                }
            }
        }
        if workspace_dir.is_none() {
            if let Err(e) = dotenvy::dotenv() {
                if !matches!(e, dotenvy::Error::Io(ref io_err) if io_err.kind() == std::io::ErrorKind::NotFound)
                {
                    tracing::warn!(error = %e, "Failed to parse default .env file");
                }
            }
        }

        // 4. Always load global and workspace MCP configs
        if let Some(ref global_dir) = global_cfg_dir {
            config.load_mcp_json_if_exists(&global_dir.join(crate::constants::MCP_CONFIG_FILE));
        }

        if let Some(dir) = workspace_dir {
            config.load_mcp_json_if_exists(
                &dir.join(crate::constants::WORKSPACE_DIR_NAME)
                    .join(crate::constants::MCP_CONFIG_FILE),
            );
            config.load_mcp_json_if_exists(&dir.join(crate::constants::MCP_CONFIG_FILE));
        }

        // 5. Apply environment variable overrides
        config.apply_env_overrides();

        // 6. Dynamic 6-tier provider resolution
        let (resolved_provider, resolved_model) =
            config.resolve_active_provider_and_model(workspace_dir);
        config.provider.default = resolved_provider;
        config.provider.model = resolved_model;

        // 7. Sync runtime timeouts for providers and command execution
        config.sync_runtime_timeouts();

        Ok(config)
    }

    /// Returns `true` when `workspace_root` lives inside `std::env::temp_dir()` inside a test
    /// environment (or with `MINICODE_ISOLATED_TEST` set).
    fn is_isolated_temp_workspace(workspace_root: &Path) -> bool {
        Self::is_isolated_temp_workspace_with_env(workspace_root, Self::is_isolated_test_env())
    }

    /// Explicitly writes configuration to `<workspace>/.minicode/config.toml`.
    /// For real (non-tempdir) workspaces, strips plaintext `[provider.api_keys]` so global
    /// credentials are never leaked into a project workspace directory.
    pub fn save_workspace(&self, workspace_root: &Path) -> Result<()> {
        let local_path = workspace_root
            .join(crate::constants::WORKSPACE_DIR_NAME)
            .join(crate::constants::CONFIG_FILE_NAME);
        let strip_secrets = !Self::is_isolated_temp_workspace(workspace_root);
        self.write_config_to_path_with_options(&local_path, strip_secrets)
    }

    /// Maps a known provider API-key environment variable name to its canonical provider ID.
    fn provider_id_for_env_var(env_name: &str) -> Option<&'static str> {
        match env_name {
            "ANTHROPIC_API_KEY" => Some("anthropic"),
            "OPENROUTER_API_KEY" | "OPENROUTER_KEY" => Some("openrouter"),
            "GEMINI_API_KEY" | "GOOGLE_API_KEY" => Some("gemini"),
            "OPENAI_API_KEY" => Some("openai"),
            "DEEPSEEK_API_KEY" => Some("deepseek"),
            "GROQ_API_KEY" => Some("groq"),
            "MINIMAX_API_KEY" => Some("minimax"),
            "ZHIPU_API_KEY" | "Z_AI_API_KEY" | "GLM_API_KEY" | "BIGMODEL_API_KEY" => Some("z.ai"),
            "TOGETHER_API_KEY" => Some("together"),
            "MISTRAL_API_KEY" => Some("mistral"),
            "HF_TOKEN" | "HUGGINGFACE_API_KEY" | "HUGGINGFACE_TOKEN" => Some("huggingface"),
            "NVIDIA_API_KEY" | "NGC_API_KEY" => Some("nvidia"),
            "OPENCODE_API_KEY" | "OPENCODE_ZEN_API_KEY" => Some("opencode"),
            "CEREBRAS_API_KEY" => Some("cerebras"),
            "AGNES_API_KEY" => Some("agnes"),
            "NARA_API_KEY" | "NARAROUTER_API_KEY" => Some("nara"),
            "OFOX_API_KEY" | "OFOXAI_API_KEY" => Some("ofox"),
            "AIHUBMIX_API_KEY" => Some("aihubmix"),
            "KIMI_API_KEY" | "MOONSHOT_API_KEY" => Some("kimi"),
            "OLLAMA_API_KEY" => Some("ollama"),
            "LMSTUDIO_API_KEY" => Some("lmstudio"),
            "VLLM_API_KEY" => Some("vllm"),
            "LOCALHOST_API_KEY" => Some("localhost"),
            _ => None,
        }
    }

    /// Loads global `~/.config/minicode/.env` without letting stale `.env` entries shadow
    /// newer `[provider.api_keys]` saved in `~/.config/minicode/config.toml`, while also
    /// importing `.env`-only provider keys into `self.provider.api_keys`.
    pub(crate) fn load_global_dotenv_respecting_toml(
        &mut self,
        global_env: &Path,
        global_toml_mtime: Option<std::time::SystemTime>,
    ) {
        let env_mtime = std::fs::metadata(global_env)
            .ok()
            .and_then(|m| m.modified().ok());
        let toml_is_newer_or_equal = match (global_toml_mtime, env_mtime) {
            (Some(t_mt), Some(e_mt)) => t_mt >= e_mt,
            _ => false,
        };
        let iter = match dotenvy::from_path_iter(global_env) {
            Ok(it) => it,
            Err(dotenvy::Error::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => return,
            Err(e) => {
                tracing::warn!(
                    path = %global_env.display(),
                    error = %e,
                    "Failed to parse global .env file"
                );
                return;
            }
        };

        for item in iter {
            let Ok((key, val)) = item else {
                continue;
            };
            let trimmed_val = val.trim();
            if trimmed_val.is_empty() {
                continue;
            }

            let mapped_provider: Option<String> = Self::provider_id_for_env_var(&key)
                .map(str::to_string)
                .or_else(|| {
                    self.provider
                        .api_keys
                        .keys()
                        .find(|p| crate::ui::setup::SetupWizard::custom_env_var(p) == key)
                        .cloned()
                });

            if let Some(prov_id) = mapped_provider {
                let toml_has_key = self
                    .provider
                    .api_keys
                    .get(&prov_id)
                    .is_some_and(|k| !k.trim().is_empty());

                if toml_has_key && toml_is_newer_or_equal {
                    continue;
                }

                self.provider
                    .api_keys
                    .insert(prov_id, trimmed_val.to_string());
            }

            if std::env::var_os(&key).is_none() {
                std::env::set_var(&key, trimmed_val);
            }
        }
    }

    /// Syncs all non-empty entries in `self.provider.api_keys` to `<global_dir>/.env` with
    /// owner-only permissions (`0o600` on Unix) so global `config.toml` and `.env` stay 100%
    /// aligned across `/settings`, `/switch`, `ApiKeyInput`, and `minicode setup`.
    pub(crate) fn sync_global_dotenv(&self, global_dir: &Path) {
        if self.provider.api_keys.is_empty() {
            return;
        }
        let global_env_path = global_dir.join(crate::constants::ENV_FILE_NAME);
        for (provider, key) in &self.provider.api_keys {
            let trimmed = key.trim();
            if trimmed.is_empty() {
                continue;
            }
            let env_name = crate::ui::setup::SetupWizard::custom_env_var(provider);
            if !env_name.is_empty() {
                let _ =
                    crate::ui::configure::update_dotenv_file(&global_env_path, &env_name, trimmed);
            }
        }
        #[cfg(unix)]
        if global_env_path.exists() {
            use std::os::unix::fs::PermissionsExt;
            let _ =
                std::fs::set_permissions(&global_env_path, std::fs::Permissions::from_mode(0o600));
        }
    }

    /// Persists ONLY global credentials (`[provider.api_keys]` and `[provider.custom_endpoints]`)
    /// to global `~/.config/minicode/config.toml` & `.env` so API keys added while Workspace scope
    /// is toggled in `/settings` are never dropped or trapped inside a single project.
    pub fn save_global_credentials(&self) -> Result<()> {
        if self.provider.api_keys.is_empty() && self.provider.custom_endpoints.is_empty() {
            return Ok(());
        }
        if let Some(global_dir) = Self::resolve_global_config_dir() {
            let global_path = global_dir.join(crate::constants::CONFIG_FILE_NAME);
            let mut patch_root = toml::map::Map::new();
            let mut provider_tbl = toml::map::Map::new();
            if !self.provider.api_keys.is_empty() {
                let mut keys_tbl = toml::map::Map::new();
                for (k, v) in &self.provider.api_keys {
                    if !v.trim().is_empty() {
                        keys_tbl.insert(k.clone(), toml::Value::String(v.trim().to_string()));
                    }
                }
                if !keys_tbl.is_empty() {
                    provider_tbl.insert("api_keys".to_string(), toml::Value::Table(keys_tbl));
                }
            }
            if !self.provider.custom_endpoints.is_empty() {
                let mut ep_tbl = toml::map::Map::new();
                for (k, v) in &self.provider.custom_endpoints {
                    if !v.trim().is_empty() {
                        ep_tbl.insert(k.clone(), toml::Value::String(v.trim().to_string()));
                    }
                }
                if !ep_tbl.is_empty() {
                    provider_tbl.insert("custom_endpoints".to_string(), toml::Value::Table(ep_tbl));
                }
            }
            if !provider_tbl.is_empty() {
                patch_root.insert("provider".to_string(), toml::Value::Table(provider_tbl));
                let patch_val = toml::Value::Table(patch_root);
                let _ = std::fs::create_dir_all(&global_dir);
                let final_val = if let Ok(existing_str) = std::fs::read_to_string(&global_path) {
                    if let Ok(mut existing_val) = toml::from_str::<toml::Value>(&existing_str) {
                        merge_config_preserving_existing_maps(&mut existing_val, &patch_val);
                        existing_val
                    } else {
                        patch_val
                    }
                } else {
                    patch_val
                };
                let toml_str =
                    toml::to_string_pretty(&final_val).map_err(ConfigError::TomlSerialize)?;
                let tmp_path =
                    global_path.with_extension(format!("toml.tmp.{}", uuid::Uuid::new_v4()));
                std::fs::write(&tmp_path, &toml_str)?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let _ =
                        std::fs::set_permissions(&tmp_path, std::fs::Permissions::from_mode(0o600));
                }
                if let Err(e) = std::fs::rename(&tmp_path, &global_path) {
                    let _ = std::fs::remove_file(&tmp_path);
                    return Err(e.into());
                }
            }
            self.sync_global_dotenv(&global_dir);
        }
        Ok(())
    }

    /// Saves the current configuration to disk:
    /// - For isolated tempdir/test workspaces (`std::env::temp_dir()` inside a test harness),
    ///   writes only to `<workspace>/.minicode/config.toml` so tests never mutate the developer's
    ///   real global config.
    /// - For real workspaces: ALWAYS persists full configuration (including `[provider.api_keys]`)
    ///   to global `~/.config/minicode/config.toml` & `~/.config/minicode/.env`, and if a
    ///   project-local `.minicode/config.toml` already exists, also updates it with plaintext
    ///   `[provider.api_keys]` stripped so secrets never leak into project directories.
    pub fn save(&self, workspace_root: Option<&Path>) -> Result<()> {
        let global_cfg_dir = Self::resolve_global_config_dir();

        if let Some(ws) = workspace_root {
            let local_path = ws
                .join(crate::constants::WORKSPACE_DIR_NAME)
                .join(crate::constants::CONFIG_FILE_NAME);
            if Self::is_isolated_temp_workspace(ws) {
                return self.write_config_to_path_with_options(&local_path, false);
            }

            if let Some(ref global_dir) = global_cfg_dir {
                let global_path = global_dir.join(crate::constants::CONFIG_FILE_NAME);
                self.write_config_to_path_with_options(&global_path, false)?;
                self.sync_global_dotenv(global_dir);
                if local_path.exists() {
                    self.write_config_to_path_with_options(&local_path, true)?;
                }
                return Ok(());
            }

            return self.write_config_to_path_with_options(&local_path, true);
        }

        if let Some(global_dir) = global_cfg_dir {
            let global_path = global_dir.join(crate::constants::CONFIG_FILE_NAME);
            self.write_config_to_path_with_options(&global_path, false)?;
            self.sync_global_dotenv(&global_dir);
        }
        Ok(())
    }

    pub(crate) fn write_config_to_path_with_options(
        &self,
        path: &Path,
        strip_secrets: bool,
    ) -> Result<()> {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let mut new_val =
            toml::Value::try_from(self.clone()).map_err(ConfigError::TomlSerialize)?;
        if strip_secrets {
            strip_secrets_from_toml_value(&mut new_val);
        }

        let mut final_val = if let Ok(existing_str) = std::fs::read_to_string(path) {
            if let Ok(mut existing_val) = toml::from_str::<toml::Value>(&existing_str) {
                merge_config_preserving_existing_maps(&mut existing_val, &new_val);
                prune_cleared_optional_fields(&mut existing_val, self);
                existing_val
            } else {
                new_val
            }
        } else {
            new_val
        };

        if strip_secrets {
            strip_secrets_from_toml_value(&mut final_val);
        }

        let toml_str = toml::to_string_pretty(&final_val).map_err(ConfigError::TomlSerialize)?;
        let tmp_path = path.with_extension(format!("toml.tmp.{}", uuid::Uuid::new_v4()));
        std::fs::write(&tmp_path, &toml_str)?;
        #[cfg(unix)]
        if !strip_secrets {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&tmp_path, std::fs::Permissions::from_mode(0o600));
        }
        if let Err(e) = std::fs::rename(&tmp_path, path) {
            let _ = std::fs::remove_file(&tmp_path);
            return Err(e.into());
        }
        tracing::info!(
            path = %path.display(),
            secrets_stripped = strip_secrets,
            "Configuration saved atomically to disk"
        );
        Ok(())
    }

    fn load_mcp_json_if_exists(&mut self, path: &Path) {
        match std::fs::read_to_string(path) {
            Ok(content) => match serde_json::from_str::<McpJsonFile>(&content) {
                Ok(parsed) => {
                    for (name, server) in parsed.mcp_servers {
                        if let Err(err) = server.validate(&name) {
                            tracing::warn!(path = %path.display(), error = %err, "Invalid MCP server config; skipping");
                            continue;
                        }
                        self.mcp.servers.insert(name, server);
                    }
                    for (name, server) in parsed.servers {
                        if let Err(err) = server.validate(&name) {
                            tracing::warn!(path = %path.display(), error = %err, "Invalid MCP server config; skipping");
                            continue;
                        }
                        self.mcp.servers.insert(name, server);
                    }
                }
                Err(e) => {
                    tracing::warn!(path = %path.display(), error = %e, "Failed to parse mcp.json");
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                tracing::warn!(path = %path.display(), error = %e, "Failed to read mcp.json");
            }
        }
    }

    fn load_raw_from_file(path: &Path) -> Result<Option<RawConfig>> {
        match std::fs::read_to_string(path) {
            Ok(content) => {
                let parsed: RawConfig = toml::from_str(&content).map_err(ConfigError::TomlParse)?;
                tracing::debug!(path = %path.display(), "Loaded configuration from file");
                Ok(Some(parsed))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(ConfigError::FileRead {
                path: path.display().to_string(),
                source: e,
            }
            .into()),
        }
    }

    pub fn merge_raw(&mut self, other: RawConfig) {
        if let Some(default) = other.provider.default {
            self.provider.default = default;
        }
        if let Some(model) = other.provider.model {
            self.provider.model = model;
        }
        if let Some(temperature) = other.provider.temperature {
            self.provider.temperature = temperature;
        }
        if let Some(max_tokens) = other.provider.max_tokens {
            self.provider.max_tokens = max_tokens;
        }
        if let Some(ollama) = other.provider.ollama {
            self.provider.ollama = ollama;
        }
        if let Some(context_window) = other.provider.context_window {
            self.provider.context_window = Some(context_window);
        }
        if let Some(prompt_cost) = other.provider.prompt_cost_per_m {
            self.provider.prompt_cost_per_m = Some(prompt_cost);
        }
        if let Some(comp_cost) = other.provider.completion_cost_per_m {
            self.provider.completion_cost_per_m = Some(comp_cost);
        }
        if let Some(thinking_budget) = other.provider.thinking_budget {
            self.provider.thinking_budget = Some(thinking_budget);
        }
        if let Some(reasoning_effort) = other.provider.reasoning_effort {
            let trimmed = reasoning_effort.trim().to_string();
            self.provider.reasoning_effort = (!trimmed.is_empty()).then_some(trimmed);
        }
        if let Some(f1) = other.provider.fallback_1 {
            self.provider.fallback_1 = f1.is_valid().then_some(f1);
        }
        if let Some(f2) = other.provider.fallback_2 {
            self.provider.fallback_2 = f2.is_valid().then_some(f2);
        }
        if let Some(glf) = other.provider.global_last_fallback {
            self.provider.global_last_fallback = glf.is_valid().then_some(glf);
        }
        if let Some(favs) = other.provider.favorite_models {
            for fav in favs {
                if fav.is_valid()
                    && !self.provider.favorite_models.iter().any(|existing| {
                        existing.provider.eq_ignore_ascii_case(&fav.provider)
                            && existing.model.eq_ignore_ascii_case(&fav.model)
                    })
                {
                    self.provider.favorite_models.push(fav);
                }
            }
        }
        if let Some(connect_timeout) = other.provider.connect_timeout_secs {
            self.provider.connect_timeout_secs = Some(connect_timeout.max(5));
        }
        if let Some(stream_idle) = other.provider.stream_idle_timeout_secs {
            self.provider.stream_idle_timeout_secs = Some(stream_idle.max(15));
        }
        if let Some(adaptive) = other.provider.adaptive_timeout {
            self.provider.adaptive_timeout = adaptive;
        }
        if let Some(auto_approve) = other.agent.auto_approve {
            self.agent.auto_approve = auto_approve;
        }
        if let Some(approval_policy) = other.agent.approval_policy {
            self.agent.approval_policy = approval_policy;
        }
        if let Some(timeout) = other.agent.timeout {
            self.agent.timeout = timeout;
        }
        if let Some(map_tokens) = other.agent.map_tokens {
            self.agent.map_tokens = map_tokens;
        }
        if let Some(warning_threshold) = other.agent.warning_threshold {
            self.agent.warning_threshold = warning_threshold;
        }
        if let Some(auto_heal) = other.agent.auto_heal {
            self.agent.auto_heal = auto_heal;
        }
        if let Some(streaming) = other.agent.streaming {
            self.agent.streaming = streaming;
        }
        if let Some(caveman_mode) = other.agent.caveman_mode {
            self.agent.caveman_mode = caveman_mode;
        }
        if let Some(caveman_level) = other.agent.caveman_level {
            if !caveman_level.trim().is_empty() {
                self.agent.caveman_level = caveman_level.trim().to_lowercase();
            }
        }
        if let Some(ponytail_mode) = other.agent.ponytail_mode {
            self.agent.ponytail_mode = ponytail_mode;
        }
        if let Some(ponytail_level) = other.agent.ponytail_level {
            if !ponytail_level.trim().is_empty() {
                self.agent.ponytail_level = ponytail_level.trim().to_lowercase();
            }
        }
        if let Some(tool_mode) = other.agent.tool_mode {
            self.agent.tool_mode = tool_mode;
        }
        if let Some(syntax_barrier) = other.agent.syntax_barrier {
            self.agent.syntax_barrier = syntax_barrier;
        }
        if let Some(auto_lint) = other.agent.auto_lint {
            self.agent.auto_lint = auto_lint;
        }
        if let Some(parallel_tools) = other.agent.parallel_tools {
            self.agent.parallel_tools = parallel_tools;
        }
        if let Some(speculative_execution) = other.agent.speculative_execution {
            self.agent.speculative_execution = speculative_execution;
        }
        if let Some(max_parallel_tools) = other.agent.max_parallel_tools {
            self.agent.max_parallel_tools = max_parallel_tools.clamp(
                crate::constants::MIN_PARALLEL_TOOLS,
                crate::constants::MAX_PARALLEL_TOOLS_CAP,
            );
        }
        if let Some(compact_tool_schemas) = other.agent.compact_tool_schemas {
            self.agent.compact_tool_schemas = compact_tool_schemas;
        }
        if let Some(max_iter) = other.agent.max_tool_iterations {
            self.agent.max_tool_iterations = max_iter;
        }
        if let Some(ac) = other.agent.auto_continue {
            self.agent.auto_continue = ac;
        }
        if let Some(mac) = other.agent.max_auto_continues {
            self.agent.max_auto_continues = mac;
        }
        if let Some(intent) = other.agent.intent {
            self.agent.intent = intent;
        }
        if let Some(plan_approval) = other.agent.plan_approval {
            self.agent.plan_approval = plan_approval;
        }
        if let Some(plan_review) = other.agent.plan_review {
            if !plan_review.trim().is_empty() {
                self.agent.plan_review = plan_review.trim().to_ascii_lowercase();
            }
        }
        if let Some(mp) = other.agent.minipub {
            if let Some(max_parallel) = mp.max_parallel {
                self.agent.minipub.max_parallel = max_parallel.clamp(
                    crate::constants::MIN_PARALLEL_MINIPUBS,
                    crate::constants::MAX_PARALLEL_MINIPUBS_CAP,
                );
            }
            if let Some(budget) = mp.default_token_budget {
                self.agent.minipub.default_token_budget = budget.max(4_000);
            }
            if let Some(turns) = mp.default_max_turns {
                self.agent.minipub.default_max_turns = turns.clamp(1, 100);
            }
            if let Some(inact) = mp.inactivity_timeout_secs {
                self.agent.minipub.inactivity_timeout_secs = inact.max(10);
            }
            if let Some(iso) = mp.worktree_isolation {
                self.agent.minipub.worktree_isolation = iso;
            }
            if let Some(extra) = mp.extra_allowed_tools {
                self.agent.minipub.extra_allowed_tools = extra;
            }
            if let Some(sync) = mp.sync_with_orchestrator {
                self.agent.minipub.sync_with_orchestrator = sync;
            }
            if let Some(dc) = mp.default_chain {
                if dc.primary.is_some() {
                    self.agent.minipub.default_chain.primary = dc.primary.filter(|s| s.is_valid());
                }
                if dc.fallback_1.is_some() {
                    self.agent.minipub.default_chain.fallback_1 =
                        dc.fallback_1.filter(|s| s.is_valid());
                }
                if dc.fallback_2.is_some() {
                    self.agent.minipub.default_chain.fallback_2 =
                        dc.fallback_2.filter(|s| s.is_valid());
                }
            }
            if let Some(roles) = mp.role_models {
                for (role_key, chain) in roles {
                    let norm_role = role_key.trim().to_lowercase().replace('-', "_");
                    if chain.is_empty() {
                        self.agent.minipub.role_models.remove(&norm_role);
                    } else {
                        self.agent.minipub.role_models.insert(norm_role, chain);
                    }
                }
            }
            if let Some(categories) = mp.category_models {
                for (cat_key, chain) in categories {
                    let norm_cat = crate::config::MiniPubConfig::normalize_category(&cat_key);
                    if chain.is_empty() {
                        self.agent.minipub.category_models.remove(&norm_cat);
                    } else {
                        self.agent.minipub.category_models.insert(norm_cat, chain);
                    }
                }
            }
        }
        if let Some(plain) = other.ui.plain {
            self.ui.plain = plain;
        }
        if let Some(theme) = other.ui.theme {
            self.ui.theme = theme;
        }
        if let Some(animation) = other.ui.animation {
            self.ui.animation = animation;
        }
        if let Some(todo_style) = other.ui.todo_style {
            self.ui.todo_style = todo_style;
        }
        if let Some(swarm_style) = other.ui.swarm_style {
            self.ui.swarm_style = swarm_style;
        }
        if let Some(toast_style) = other.ui.toast_style {
            self.ui.toast_style = toast_style;
        }
        if let Some(toast_symbols) = other.ui.toast_symbols {
            self.ui.toast_symbols = toast_symbols;
        }
        if let Some(activity_style) = other.ui.activity_style {
            self.ui.activity_style = activity_style;
        }
        if let Some(menu_style) = other.ui.menu_style {
            self.ui.menu_style = menu_style;
        }
        if let Some(input_box_style) = other.ui.input_box_style {
            self.ui.input_box_style = input_box_style;
        }
        if let Some(status_bar_style) = other.ui.status_bar_style {
            self.ui.status_bar_style = status_bar_style;
        }
        if let Some(v) = other.ui.status_model_style {
            self.ui.status_model_style = v;
        }
        if let Some(v) = other.ui.status_effort_style {
            self.ui.status_effort_style = v;
        }
        if let Some(v) = other.ui.status_path_style {
            self.ui.status_path_style = v;
        }
        if let Some(v) = other.ui.status_git_style {
            self.ui.status_git_style = v;
        }
        if let Some(v) = other.ui.status_badges_style {
            self.ui.status_badges_style = v;
        }
        if let Some(context_meter_style) = other.ui.context_meter_style {
            self.ui.context_meter_style = context_meter_style;
        }
        if let Some(mode_bar_style) = other.ui.mode_bar_style {
            self.ui.mode_bar_style = mode_bar_style;
        }
        if let Some(activity_hint_style) = other.ui.activity_hint_style {
            self.ui.activity_hint_style = activity_hint_style;
        }
        if let Some(tool_card_style) = other.ui.tool_card_style {
            self.ui.tool_card_style = tool_card_style;
        }
        if let Some(tool_icon_style) = other.ui.tool_icon_style {
            self.ui.tool_icon_style = tool_icon_style;
        }
        if let Some(max_width) = other.ui.max_width {
            self.ui.max_width = max_width;
        }
        if let Some(show_cost) = other.ui.show_cost {
            self.ui.show_cost = show_cost;
        }
        if let Some(v) = other.ui.show_ponytail_badge {
            self.ui.show_ponytail_badge = v;
        }
        if let Some(v) = other.ui.show_caveman_badge {
            self.ui.show_caveman_badge = v;
        }
        if let Some(v) = other.ui.show_streaming_badge {
            self.ui.show_streaming_badge = v;
        }
        if let Some(v) = other.ui.show_kv_cache {
            self.ui.show_kv_cache = v;
        }
        if let Some(v) = other.ui.show_context_meter {
            self.ui.show_context_meter = v;
        }
        if let Some(v) = other.ui.show_status_model {
            self.ui.show_status_model = v;
        }
        if let Some(v) = other.ui.show_status_effort {
            self.ui.show_status_effort = v;
        }
        if let Some(v) = other.ui.show_status_path {
            self.ui.show_status_path = v;
        }
        if let Some(v) = other.ui.show_status_git {
            self.ui.show_status_git = v;
        }
        if let Some(v) = other.ui.show_status_badges {
            self.ui.show_status_badges = v;
        }
        if let Some(v) = other.ui.show_interrupt_hint {
            self.ui.show_interrupt_hint = v;
        }
        if let Some(v) = other.ui.show_thought_tokens {
            self.ui.show_thought_tokens = v;
        }
        if let Some(level) = other.logging.level {
            self.logging.level = level;
        }
        if let Some(file) = other.logging.file {
            self.logging.file = file;
        }
        if let Some(auto_commit) = other.git.auto_commit {
            self.git.auto_commit = auto_commit;
        }
        if let Some(dirty_commit) = other.git.dirty_commit {
            self.git.dirty_commit = dirty_commit;
        }
        if let Some(ai_commit_messages) = other.git.ai_commit_messages {
            self.git.ai_commit_messages = ai_commit_messages;
        }
        if let Some(keys) = other.provider.api_keys {
            for (k, v) in keys {
                if !v.trim().is_empty() {
                    self.provider.api_keys.insert(k.to_lowercase(), v);
                }
            }
        }
        if let Some(endpoints) = other.provider.custom_endpoints {
            for (k, v) in endpoints {
                if !v.trim().is_empty() {
                    self.provider.custom_endpoints.insert(k.to_lowercase(), v);
                }
            }
        }
        if let Some(models) = other.provider.default_models {
            for (k, v) in models {
                if !v.trim().is_empty() {
                    self.provider.default_models.insert(k.to_lowercase(), v);
                }
            }
        }
        if let Some(policy) = other.mcp.approval_policy {
            self.mcp.approval_policy = Some(policy);
        }
        for (name, srv) in other.mcp.servers {
            self.mcp.servers.insert(name, srv);
        }
        if let Some(hl) = other.browser.headless_priority {
            self.browser.headless_priority = hl;
        }
        if let Some(gp) = other.browser.gui_priority {
            self.browser.gui_priority = gp;
        }
        if let Some(de) = other.browser.default_engine {
            self.browser.default_engine = if de.trim().is_empty() || de.eq_ignore_ascii_case("auto")
            {
                None
            } else {
                Some(de)
            };
        }
        if let Some(dbg) = other.browser.debug_engine {
            if !dbg.trim().is_empty() {
                self.browser.debug_engine = dbg;
            }
        }
    }

    fn env_bool(key: &str) -> Option<bool> {
        std::env::var(key)
            .ok()
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
    }

    fn env_parse<T: std::str::FromStr>(key: &str) -> Option<T> {
        std::env::var(key).ok().and_then(|v| v.parse::<T>().ok())
    }

    fn apply_env_overrides(&mut self) {
        if let Ok(model) = std::env::var(env_vars::MINICODE_MODEL) {
            self.provider.model = model;
        }
        if let Ok(provider) = std::env::var(env_vars::MINICODE_PROVIDER) {
            self.provider.default = provider;
        }
        if let Some(auto_approve) = Self::env_bool(env_vars::MINICODE_AUTO_APPROVE) {
            self.agent.auto_approve = auto_approve;
        }
        if let Ok(policy) = std::env::var(env_vars::MINICODE_APPROVAL_POLICY) {
            self.agent.approval_policy = policy;
        }
        if let Some(temp) = Self::env_parse::<f32>(env_vars::MINICODE_TEMPERATURE) {
            self.provider.temperature = temp;
        }
        if let Some(tokens) = Self::env_parse::<usize>(env_vars::MINICODE_MAX_TOKENS) {
            self.provider.max_tokens = tokens;
        }
        if let Some(timeout) = Self::env_parse::<u64>(env_vars::MINICODE_TIMEOUT) {
            self.agent.timeout = timeout;
        }
        if let Some(conn) = Self::env_parse::<u64>(env_vars::MINICODE_CONNECT_TIMEOUT) {
            self.provider.connect_timeout_secs = Some(conn.max(5));
        }
        if let Some(idle) = Self::env_parse::<u64>(env_vars::MINICODE_STREAM_IDLE_TIMEOUT) {
            self.provider.stream_idle_timeout_secs = Some(idle.max(15));
        }
        if let Some(plain) = Self::env_bool(env_vars::MINICODE_PLAIN) {
            self.ui.plain = plain;
        }
        if let Ok(theme) = std::env::var(env_vars::MINICODE_THEME) {
            self.ui.theme = theme;
        }
        if let Ok(animation) = std::env::var(env_vars::MINICODE_ANIMATION) {
            self.ui.animation = animation;
        }
        if let Ok(todo_style) = std::env::var(env_vars::MINICODE_TODO_STYLE) {
            self.ui.todo_style = todo_style;
        }
        if let Ok(plan_review) = std::env::var(env_vars::MINICODE_PLAN_REVIEW) {
            if !plan_review.trim().is_empty() {
                self.agent.plan_review = plan_review.trim().to_ascii_lowercase();
            }
        }
        if let Ok(level) = std::env::var(env_vars::MINICODE_LOG_LEVEL) {
            self.logging.level = level;
        }
        if let Some(parallel) = Self::env_bool(env_vars::MINICODE_PARALLEL_TOOLS) {
            self.agent.parallel_tools = parallel;
        }
        if let Some(spec) = Self::env_bool(env_vars::MINICODE_SPECULATIVE_EXECUTION) {
            self.agent.speculative_execution = spec;
        }
        if let Some(val) = Self::env_parse::<usize>(env_vars::MINICODE_MAX_PARALLEL_TOOLS) {
            self.agent.max_parallel_tools = val.clamp(
                crate::constants::MIN_PARALLEL_TOOLS,
                crate::constants::MAX_PARALLEL_TOOLS_CAP,
            );
        }
        if let Some(val) = Self::env_parse::<usize>(env_vars::MINICODE_MAX_MINIPUBS) {
            self.agent.minipub.max_parallel = val.clamp(
                crate::constants::MIN_PARALLEL_MINIPUBS,
                crate::constants::MAX_PARALLEL_MINIPUBS_CAP,
            );
        }
        if let Some(val) = Self::env_parse::<u64>(env_vars::MINICODE_MINIPUB_TIMEOUT) {
            self.agent.minipub.inactivity_timeout_secs = val.max(10);
        }
        if let Some(val) = Self::env_parse::<usize>(env_vars::MINICODE_MINIPUB_TOKEN_BUDGET) {
            self.agent.minipub.default_token_budget = val.max(4_000);
        }
        if let Some(val) = Self::env_parse::<usize>(env_vars::MINICODE_MINIPUB_MAX_TURNS) {
            self.agent.minipub.default_max_turns = val.clamp(1, 100);
        }
        if let Ok(mp_iso) = std::env::var(env_vars::MINICODE_MINIPUB_ISOLATION) {
            self.agent.minipub.worktree_isolation = mp_iso;
        }
        if let Some(compact) = Self::env_bool(env_vars::MINICODE_COMPACT_TOOL_SCHEMAS) {
            self.agent.compact_tool_schemas = compact;
        }
        if let Some(val) = Self::env_parse::<usize>(env_vars::MINICODE_MAX_TOOL_ITERATIONS) {
            self.agent.max_tool_iterations = val;
        }
        if let Some(ac) = Self::env_bool(env_vars::MINICODE_AUTO_CONTINUE) {
            self.agent.auto_continue = ac;
        }
    }

    /// Re-applies newer global `[ui]` visual settings and `ponytail`/`caveman` autonomy toggles
    /// so an older workspace `.minicode/config.toml` never reverts recently saved global preferences.
    fn merge_ui_and_autonomy_from_raw(&mut self, global: &RawConfig) {
        if let Some(plain) = global.ui.plain {
            self.ui.plain = plain;
        }
        if let Some(ref theme) = global.ui.theme {
            self.ui.theme = theme.clone();
        }
        if let Some(ref animation) = global.ui.animation {
            self.ui.animation = animation.clone();
        }
        if let Some(ref todo_style) = global.ui.todo_style {
            self.ui.todo_style = todo_style.clone();
        }
        if let Some(ref swarm_style) = global.ui.swarm_style {
            self.ui.swarm_style = swarm_style.clone();
        }
        if let Some(ref toast_style) = global.ui.toast_style {
            self.ui.toast_style = toast_style.clone();
        }
        if let Some(ref toast_symbols) = global.ui.toast_symbols {
            self.ui.toast_symbols = toast_symbols.clone();
        }
        if let Some(ref activity_style) = global.ui.activity_style {
            self.ui.activity_style = activity_style.clone();
        }
        if let Some(ref menu_style) = global.ui.menu_style {
            self.ui.menu_style = menu_style.clone();
        }
        if let Some(ref input_box_style) = global.ui.input_box_style {
            self.ui.input_box_style = input_box_style.clone();
        }
        if let Some(ref status_bar_style) = global.ui.status_bar_style {
            self.ui.status_bar_style = status_bar_style.clone();
        }
        if let Some(ref v) = global.ui.status_model_style {
            self.ui.status_model_style = v.clone();
        }
        if let Some(ref v) = global.ui.status_effort_style {
            self.ui.status_effort_style = v.clone();
        }
        if let Some(ref v) = global.ui.status_path_style {
            self.ui.status_path_style = v.clone();
        }
        if let Some(ref v) = global.ui.status_git_style {
            self.ui.status_git_style = v.clone();
        }
        if let Some(ref v) = global.ui.status_badges_style {
            self.ui.status_badges_style = v.clone();
        }
        if let Some(ref context_meter_style) = global.ui.context_meter_style {
            self.ui.context_meter_style = context_meter_style.clone();
        }
        if let Some(ref mode_bar_style) = global.ui.mode_bar_style {
            self.ui.mode_bar_style = mode_bar_style.clone();
        }
        if let Some(ref activity_hint_style) = global.ui.activity_hint_style {
            self.ui.activity_hint_style = activity_hint_style.clone();
        }
        if let Some(ref tool_card_style) = global.ui.tool_card_style {
            self.ui.tool_card_style = tool_card_style.clone();
        }
        if let Some(ref tool_icon_style) = global.ui.tool_icon_style {
            self.ui.tool_icon_style = tool_icon_style.clone();
        }
        if let Some(show_cost) = global.ui.show_cost {
            self.ui.show_cost = show_cost;
        }
        if let Some(v) = global.ui.show_ponytail_badge {
            self.ui.show_ponytail_badge = v;
        }
        if let Some(v) = global.ui.show_caveman_badge {
            self.ui.show_caveman_badge = v;
        }
        if let Some(v) = global.ui.show_streaming_badge {
            self.ui.show_streaming_badge = v;
        }
        if let Some(v) = global.ui.show_kv_cache {
            self.ui.show_kv_cache = v;
        }
        if let Some(v) = global.ui.show_context_meter {
            self.ui.show_context_meter = v;
        }
        if let Some(v) = global.ui.show_status_model {
            self.ui.show_status_model = v;
        }
        if let Some(v) = global.ui.show_status_effort {
            self.ui.show_status_effort = v;
        }
        if let Some(v) = global.ui.show_status_path {
            self.ui.show_status_path = v;
        }
        if let Some(v) = global.ui.show_status_git {
            self.ui.show_status_git = v;
        }
        if let Some(v) = global.ui.show_status_badges {
            self.ui.show_status_badges = v;
        }
        if let Some(v) = global.ui.show_interrupt_hint {
            self.ui.show_interrupt_hint = v;
        }
        if let Some(v) = global.ui.show_thought_tokens {
            self.ui.show_thought_tokens = v;
        }
        if let Some(caveman_mode) = global.agent.caveman_mode {
            self.agent.caveman_mode = caveman_mode;
        }
        if let Some(ref caveman_level) = global.agent.caveman_level {
            if !caveman_level.trim().is_empty() {
                self.agent.caveman_level = caveman_level.trim().to_lowercase();
            }
        }
        if let Some(ponytail_mode) = global.agent.ponytail_mode {
            self.agent.ponytail_mode = ponytail_mode;
        }
        if let Some(ref ponytail_level) = global.agent.ponytail_level {
            if !ponytail_level.trim().is_empty() {
                self.agent.ponytail_level = ponytail_level.trim().to_lowercase();
            }
        }
    }
}

/// Removes explicitly cleared (`None`) optional config keys from the merged TOML tree so
/// resetting fields like `browser.default_engine`, `provider.fallback_1`, or `thinking_budget`
/// in `/settings` actually deletes the stale value on disk instead of resurrecting it from the
/// prior file. Protected credential/endpoint maps and unknown forward-compatible keys remain untouched.
fn prune_cleared_optional_fields(target: &mut toml::Value, cfg: &Config) {
    let toml::Value::Table(root) = target else {
        return;
    };
    if let Some(toml::Value::Table(browser)) = root.get_mut("browser") {
        if cfg.browser.default_engine.is_none() {
            browser.remove("default_engine");
        }
    }
    if let Some(toml::Value::Table(provider)) = root.get_mut("provider") {
        if cfg.provider.fallback_1.is_none() {
            provider.remove("fallback_1");
        }
        if cfg.provider.fallback_2.is_none() {
            provider.remove("fallback_2");
        }
        if cfg.provider.global_last_fallback.is_none() {
            provider.remove("global_last_fallback");
        }
        if cfg.provider.thinking_budget.is_none() {
            provider.remove("thinking_budget");
        }
        if cfg.provider.reasoning_effort.is_none() {
            provider.remove("reasoning_effort");
        }
    }
}

/// Removes plaintext `[provider.api_keys]` from a serialized TOML config tree so
/// project-local `.minicode/config.toml` files never retain or leak global API keys.
fn strip_secrets_from_toml_value(val: &mut toml::Value) {
    if let toml::Value::Table(root) = val {
        if let Some(toml::Value::Table(provider)) = root.get_mut("provider") {
            provider.remove("api_keys");
        }
    }
}

/// Deeply merges `source` TOML onto `target`, updating all explicit fields while preserving
/// existing non-empty credential/endpoint maps and forward-compatible unknown keys if `source`
/// holds empty maps.
fn merge_config_preserving_existing_maps(target: &mut toml::Value, source: &toml::Value) {
    const PROTECTED_MAP_KEYS: &[&str] = &["api_keys", "custom_endpoints", "default_models"];
    if let (toml::Value::Table(target_tbl), toml::Value::Table(source_tbl)) = (&mut *target, source)
    {
        for (k, v) in source_tbl {
            if PROTECTED_MAP_KEYS.contains(&k.as_str()) {
                if let (Some(toml::Value::Table(existing_map)), toml::Value::Table(new_map)) =
                    (target_tbl.get_mut(k), v)
                {
                    for (mk, mv) in new_map {
                        if mv.as_str().is_none_or(|s| !s.trim().is_empty()) {
                            existing_map.insert(mk.clone(), mv.clone());
                        }
                    }
                    continue;
                }
            }
            if let Some(target_child) = target_tbl.get_mut(k) {
                if target_child.is_table() && v.is_table() {
                    merge_config_preserving_existing_maps(target_child, v);
                } else {
                    *target_child = v.clone();
                }
            } else {
                target_tbl.insert(k.clone(), v.clone());
            }
        }
    } else {
        *target = source.clone();
    }
}

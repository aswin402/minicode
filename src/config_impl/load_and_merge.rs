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
    pub activity_style: Option<String>,
    pub menu_style: Option<String>,
    pub max_width: Option<usize>,
    pub show_cost: Option<bool>,
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

    /// Loads configuration respecting the hierarchy:
    /// 1. Project-local `.minicode/config.toml` (if present)
    /// 2. Global `~/.config/minicode/config.toml` (if present)
    /// 3. Project-local `mcp.json` or `.minicode/mcp.json` (if present)
    /// 4. Built-in defaults
    /// 5. Environment variable overrides (MINICODE_*)
    pub fn load(workspace_dir: Option<&Path>, custom_config_path: Option<&Path>) -> Result<Self> {
        // 0. Load global ~/.config/minicode/.env if present
        if let Some(global_dir) = dirs::config_dir() {
            let global_env = global_dir
                .join(crate::constants::CONFIG_DIR_NAME)
                .join(crate::constants::ENV_FILE_NAME);
            match dotenvy::from_path(&global_env) {
                Ok(_) => {}
                Err(dotenvy::Error::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => {
                    tracing::warn!(path = %global_env.display(), error = %e, "Failed to parse global .env file");
                }
            }
        }

        // 1. Load workspace .env if present
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

        let mut config = Config::default();

        // 1. Try custom config path if explicitly specified
        if let Some(path) = custom_config_path {
            if let Some(raw) = Self::load_raw_from_file(path)? {
                config.merge_raw(raw);
            }
        } else {
            // 2. Global config: ~/.config/minicode/config.toml
            if let Some(global_dir) = dirs::config_dir() {
                let global_config = global_dir
                    .join(crate::constants::CONFIG_DIR_NAME)
                    .join(crate::constants::CONFIG_FILE_NAME);
                if let Some(raw) = Self::load_raw_from_file(&global_config)? {
                    config.merge_raw(raw);
                }
            }

            // 3. Project-local config: <workspace>/.minicode/config.toml
            if let Some(dir) = workspace_dir {
                let local_config = dir
                    .join(crate::constants::WORKSPACE_DIR_NAME)
                    .join(crate::constants::CONFIG_FILE_NAME);
                if let Some(raw) = Self::load_raw_from_file(&local_config)? {
                    config.merge_raw(raw);
                }
            }
        }

        // 4. Always load global and workspace MCP configs
        if let Some(global_dir) = dirs::config_dir() {
            config.load_mcp_json_if_exists(
                &global_dir
                    .join(crate::constants::CONFIG_DIR_NAME)
                    .join(crate::constants::MCP_CONFIG_FILE),
            );
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

    /// Saves the current configuration to disk:
    /// If project-local `.minicode/config.toml` exists, updates it.
    /// Otherwise writes to global `~/.config/minicode/config.toml`.
    pub fn save(&self, workspace_root: Option<&Path>) -> Result<()> {
        let path = if let Some(ws) = workspace_root {
            let ws_minicode_dir = ws.join(crate::constants::WORKSPACE_DIR_NAME);
            let local_path = ws_minicode_dir.join(crate::constants::CONFIG_FILE_NAME);
            if local_path.exists() || ws_minicode_dir.exists() {
                local_path
            } else if let Some(global_dir) = dirs::config_dir() {
                global_dir
                    .join(crate::constants::CONFIG_DIR_NAME)
                    .join(crate::constants::CONFIG_FILE_NAME)
            } else {
                local_path
            }
        } else if let Some(global_dir) = dirs::config_dir() {
            global_dir
                .join(crate::constants::CONFIG_DIR_NAME)
                .join(crate::constants::CONFIG_FILE_NAME)
        } else {
            return Ok(());
        };

        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let toml_str = toml::to_string_pretty(self).map_err(ConfigError::TomlSerialize)?;
        std::fs::write(&path, toml_str)?;
        tracing::info!(path = %path.display(), "Configuration saved to disk");
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
        if let Some(activity_style) = other.ui.activity_style {
            self.ui.activity_style = activity_style;
        }
        if let Some(menu_style) = other.ui.menu_style {
            self.ui.menu_style = menu_style;
        }
        if let Some(max_width) = other.ui.max_width {
            self.ui.max_width = max_width;
        }
        if let Some(show_cost) = other.ui.show_cost {
            self.ui.show_cost = show_cost;
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
            self.browser.default_engine = Some(de);
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
}

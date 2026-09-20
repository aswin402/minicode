use crate::constants::env_vars;
use crate::error::{ConfigError, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Config {
    #[serde(default)]
    pub provider: ProviderConfig,

    #[serde(default)]
    pub agent: AgentConfig,

    #[serde(default)]
    pub ui: UiConfig,

    #[serde(default)]
    pub logging: LoggingConfig,

    #[serde(default)]
    pub mcp: McpConfig,

    #[serde(default)]
    pub git: GitConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProviderConfig {
    #[serde(default)]
    pub default: String,

    #[serde(default)]
    pub model: String,

    #[serde(default)]
    pub ollama: OllamaConfig,

    #[serde(default = "default_temperature")]
    pub temperature: f32,

    #[serde(default = "default_max_tokens")]
    pub max_tokens: usize,

    #[serde(default)]
    pub api_keys: std::collections::HashMap<String, String>,

    #[serde(default)]
    pub custom_endpoints: std::collections::HashMap<String, String>,

    #[serde(default)]
    pub thinking_budget: Option<usize>,

    #[serde(default)]
    pub reasoning_effort: Option<String>,

    /// Explicit user override for context window size in tokens
    #[serde(default)]
    pub context_window: Option<usize>,

    /// Custom prompt rate per 1,000,000 tokens in USD
    #[serde(default)]
    pub prompt_cost_per_m: Option<f64>,

    /// Custom completion rate per 1,000,000 tokens in USD
    #[serde(default)]
    pub completion_cost_per_m: Option<f64>,

    #[serde(default)]
    pub default_models: std::collections::HashMap<String, String>,
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            default: String::new(),
            model: String::new(),
            default_models: std::collections::HashMap::new(),
            ollama: OllamaConfig::default(),
            temperature: default_temperature(),
            max_tokens: default_max_tokens(),
            api_keys: std::collections::HashMap::new(),
            custom_endpoints: std::collections::HashMap::new(),
            thinking_budget: None,
            reasoning_effort: None,
            context_window: None,
            prompt_cost_per_m: None,
            completion_cost_per_m: None,
        }
    }
}

#[allow(dead_code)]
impl ProviderConfig {
    /// Returns whether extended thinking / reasoning is currently enabled
    pub fn is_thinking_enabled(&self) -> bool {
        self.thinking_budget
            .map(|b| b >= crate::constants::MIN_THINKING_BUDGET_TOKENS)
            .unwrap_or(false)
    }

    /// Returns the effective thinking budget in tokens
    pub fn effective_thinking_budget(&self) -> Option<usize> {
        self.thinking_budget
            .filter(|&b| b >= crate::constants::MIN_THINKING_BUDGET_TOKENS)
    }
}

fn default_temperature() -> f32 {
    0.2
}

fn default_max_tokens() -> usize {
    8192
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OllamaConfig {
    #[serde(default = "default_ollama_host")]
    pub host: String,
}

impl Default for OllamaConfig {
    fn default() -> Self {
        Self {
            host: default_ollama_host(),
        }
    }
}

fn default_ollama_host() -> String {
    crate::constants::DEFAULT_OLLAMA_HOST.to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentConfig {
    #[serde(default)]
    pub auto_approve: bool,

    #[serde(default = "default_approval_policy")]
    pub approval_policy: String,

    #[serde(default = "default_timeout_secs")]
    pub timeout: u64,

    #[serde(default = "default_map_tokens")]
    pub map_tokens: usize,

    #[serde(default = "default_warning_threshold")]
    pub warning_threshold: f32,

    #[serde(default = "default_true")]
    pub auto_heal: bool,

    #[serde(default = "default_true")]
    pub streaming: bool,

    #[serde(default = "default_tool_mode")]
    pub tool_mode: ToolFilterMode,

    #[serde(default = "default_true")]
    pub syntax_barrier: bool,

    #[serde(default = "default_true")]
    pub auto_lint: bool,

    #[serde(default = "default_true")]
    pub parallel_tools: bool,

    #[serde(default = "default_true")]
    pub speculative_execution: bool,

    #[serde(default = "default_max_parallel_tools")]
    pub max_parallel_tools: usize,

    #[serde(default = "default_true")]
    pub compact_tool_schemas: bool,

    /// Maximum tool calling iterations per turn (0 = unbounded continuous autonomous execution)
    #[serde(default = "default_max_tool_iterations")]
    pub max_tool_iterations: usize,

    #[serde(default = "default_true")]
    pub auto_continue: bool,

    #[serde(default = "default_max_auto_continues")]
    pub max_auto_continues: usize,

    #[serde(default)]
    pub intent: IntentConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ToolFilterMode {
    #[default]
    Dynamic,
    CoreOnly,
    Full,
    ReadOnly,
    Standard,
}

impl std::fmt::Display for ToolFilterMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Dynamic => write!(f, "dynamic"),
            Self::CoreOnly => write!(f, "core_only"),
            Self::Full => write!(f, "full"),
            Self::ReadOnly => write!(f, "read_only"),
            Self::Standard => write!(f, "standard"),
        }
    }
}

impl std::str::FromStr for ToolFilterMode {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().trim() {
            "dynamic" => Ok(Self::Dynamic),
            "core_only" | "core" => Ok(Self::CoreOnly),
            "full" | "all" => Ok(Self::Full),
            "read_only" | "readonly" => Ok(Self::ReadOnly),
            "standard" => Ok(Self::Standard),
            other => Err(format!(
                "Invalid tool_mode '{}'. Must be 'dynamic', 'core_only', 'full', 'read_only', or 'standard'",
                other
            )),
        }
    }
}

fn default_tool_mode() -> ToolFilterMode {
    ToolFilterMode::Dynamic
}

fn default_max_parallel_tools() -> usize {
    crate::constants::DEFAULT_MAX_PARALLEL_TOOLS
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            auto_approve: false,
            approval_policy: default_approval_policy(),
            timeout: default_timeout_secs(),
            map_tokens: default_map_tokens(),
            warning_threshold: default_warning_threshold(),
            auto_heal: true,
            streaming: true,
            tool_mode: default_tool_mode(),
            syntax_barrier: true,
            auto_lint: true,
            parallel_tools: true,
            speculative_execution: true,
            max_parallel_tools: crate::constants::DEFAULT_MAX_PARALLEL_TOOLS,
            compact_tool_schemas: true,
            max_tool_iterations: default_max_tool_iterations(),
            auto_continue: true,
            max_auto_continues: default_max_auto_continues(),
            intent: IntentConfig::default(),
        }
    }
}

fn default_max_tool_iterations() -> usize {
    crate::constants::DEFAULT_MAX_TOOL_ITERATIONS
}

fn default_max_auto_continues() -> usize {
    crate::constants::DEFAULT_MAX_AUTO_CONTINUES
}

fn default_approval_policy() -> String {
    "strict".to_string()
}

fn default_timeout_secs() -> u64 {
    30
}

fn default_map_tokens() -> usize {
    1024
}

fn default_warning_threshold() -> f32 {
    0.70
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IntentConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_intent_persistence_file")]
    pub persistence_file: String,
    #[serde(default = "default_intent_drift_warning_turns")]
    pub drift_warning_turns: usize,
    #[serde(default = "default_true")]
    pub auto_extract: bool,
    #[serde(default = "default_intent_max_items")]
    pub max_ledger_items: usize,
}

impl Default for IntentConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            persistence_file: default_intent_persistence_file(),
            drift_warning_turns: default_intent_drift_warning_turns(),
            auto_extract: true,
            max_ledger_items: default_intent_max_items(),
        }
    }
}

fn default_intent_persistence_file() -> String {
    crate::constants::DEFAULT_INTENT_PERSISTENCE_FILE.to_string()
}

fn default_intent_drift_warning_turns() -> usize {
    crate::constants::DEFAULT_INTENT_DRIFT_WARNING_TURNS
}

fn default_intent_max_items() -> usize {
    crate::constants::DEFAULT_INTENT_MAX_ITEMS
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UiConfig {
    #[serde(default = "default_theme")]
    pub theme: String,

    #[serde(default = "default_animation")]
    pub animation: String,

    #[serde(default)]
    pub plain: bool,

    #[serde(default = "default_max_width")]
    pub max_width: usize,

    #[serde(default)]
    pub show_cost: bool,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            theme: default_theme(),
            animation: default_animation(),
            plain: false,
            max_width: default_max_width(),
            show_cost: false,
        }
    }
}

fn default_theme() -> String {
    "auto".to_string()
}

fn default_animation() -> String {
    "dual_pillars".to_string()
}

fn default_max_width() -> usize {
    100
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LoggingConfig {
    #[serde(default = "default_log_level")]
    pub level: String,

    #[serde(default = "default_log_file")]
    pub file: bool,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: default_log_level(),
            file: default_log_file(),
        }
    }
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_log_file() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct McpConfig {
    #[serde(default)]
    pub servers: std::collections::HashMap<String, McpServerConfig>,

    /// Policy for dangerous tools (exec_cmd, write_file, patch_file) when
    /// minicode runs as an MCP server: Some("deny") blocks them; None/Some
    /// ("allow") permits everything.
    #[serde(default)]
    pub approval_policy: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McpServerConfig {
    #[serde(default = "default_mcp_transport")]
    pub transport: McpTransport,

    #[serde(default)]
    pub command: Option<String>,

    #[serde(default)]
    pub args: Option<Vec<String>>,

    #[serde(default)]
    pub url: Option<String>,

    #[serde(default)]
    pub env: Option<std::collections::HashMap<String, String>>,

    #[serde(default = "default_true")]
    pub enabled: bool,

    #[serde(default)]
    pub timeout_secs: Option<u64>,
}

impl McpServerConfig {
    pub fn validate(&self, server_name: &str) -> std::result::Result<(), String> {
        match self.transport {
            McpTransport::Stdio => {
                if self.command.as_deref().unwrap_or("").trim().is_empty() {
                    return Err(format!(
                        "MCP server '{}' with stdio transport missing 'command'",
                        server_name
                    ));
                }
            }
            McpTransport::Sse | McpTransport::Http => {
                if self.url.as_deref().unwrap_or("").trim().is_empty() {
                    return Err(format!(
                        "MCP server '{}' with http/sse transport missing 'url'",
                        server_name
                    ));
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum McpTransport {
    Stdio,
    Sse,
    Http,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GitConfig {
    #[serde(default = "default_true")]
    pub auto_commit: bool,

    #[serde(default)]
    pub dirty_commit: bool,

    #[serde(default = "default_true")]
    pub ai_commit_messages: bool,
}

impl Default for GitConfig {
    fn default() -> Self {
        Self {
            auto_commit: true,
            dirty_commit: false,
            ai_commit_messages: true,
        }
    }
}

fn default_mcp_transport() -> McpTransport {
    McpTransport::Stdio
}

impl McpConfig {
    /// Resolved approval policy for dangerous tools in MCP serve mode.
    pub fn effective_approval_policy(&self) -> &str {
        self.approval_policy.as_deref().unwrap_or("allow")
    }
}

fn default_true() -> bool {
    true
}

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
    pub default_models: Option<std::collections::HashMap<String, String>>,
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
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RawUiConfig {
    pub plain: Option<bool>,
    pub theme: Option<String>,
    pub animation: Option<String>,
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
}

impl Config {
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
        if let Err(e) = dotenvy::dotenv() {
            if !matches!(e, dotenvy::Error::Io(ref io_err) if io_err.kind() == std::io::ErrorKind::NotFound)
            {
                tracing::warn!(error = %e, "Failed to parse default .env file");
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
        if let Some(plain) = other.ui.plain {
            self.ui.plain = plain;
        }
        if let Some(theme) = other.ui.theme {
            self.ui.theme = theme;
        }
        if let Some(animation) = other.ui.animation {
            self.ui.animation = animation;
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
    }

    fn apply_env_overrides(&mut self) {
        if let Ok(model) = std::env::var(env_vars::MINICODE_MODEL) {
            self.provider.model = model;
        }
        if let Ok(provider) = std::env::var(env_vars::MINICODE_PROVIDER) {
            self.provider.default = provider;
        }
        if let Ok(auto_approve) = std::env::var(env_vars::MINICODE_AUTO_APPROVE) {
            self.agent.auto_approve =
                auto_approve == "1" || auto_approve.eq_ignore_ascii_case("true");
        }
        if let Ok(policy) = std::env::var(env_vars::MINICODE_APPROVAL_POLICY) {
            self.agent.approval_policy = policy;
        }
        if let Ok(temp_str) = std::env::var(env_vars::MINICODE_TEMPERATURE) {
            if let Ok(temp) = temp_str.parse::<f32>() {
                self.provider.temperature = temp;
            }
        }
        if let Ok(tokens_str) = std::env::var(env_vars::MINICODE_MAX_TOKENS) {
            if let Ok(tokens) = tokens_str.parse::<usize>() {
                self.provider.max_tokens = tokens;
            }
        }
        if let Ok(timeout_str) = std::env::var(env_vars::MINICODE_TIMEOUT) {
            if let Ok(timeout) = timeout_str.parse::<u64>() {
                self.agent.timeout = timeout;
            }
        }
        if let Ok(plain) = std::env::var(env_vars::MINICODE_PLAIN) {
            self.ui.plain = plain == "1" || plain.eq_ignore_ascii_case("true");
        }
        if let Ok(theme) = std::env::var(env_vars::MINICODE_THEME) {
            self.ui.theme = theme;
        }
        if let Ok(animation) = std::env::var(env_vars::MINICODE_ANIMATION) {
            self.ui.animation = animation;
        }
        if let Ok(level) = std::env::var(env_vars::MINICODE_LOG_LEVEL) {
            self.logging.level = level;
        }
        if let Ok(parallel) = std::env::var(env_vars::MINICODE_PARALLEL_TOOLS) {
            self.agent.parallel_tools = parallel == "1" || parallel.eq_ignore_ascii_case("true");
        }
        if let Ok(spec) = std::env::var(env_vars::MINICODE_SPECULATIVE_EXECUTION) {
            self.agent.speculative_execution = spec == "1" || spec.eq_ignore_ascii_case("true");
        }
        if let Ok(max_p_str) = std::env::var(env_vars::MINICODE_MAX_PARALLEL_TOOLS) {
            if let Ok(val) = max_p_str.parse::<usize>() {
                self.agent.max_parallel_tools = val.clamp(
                    crate::constants::MIN_PARALLEL_TOOLS,
                    crate::constants::MAX_PARALLEL_TOOLS_CAP,
                );
            }
        }
        if let Ok(compact) = std::env::var(env_vars::MINICODE_COMPACT_TOOL_SCHEMAS) {
            self.agent.compact_tool_schemas =
                compact == "1" || compact.eq_ignore_ascii_case("true");
        }
        if let Ok(val_str) = std::env::var(env_vars::MINICODE_MAX_TOOL_ITERATIONS) {
            if let Ok(val) = val_str.parse::<usize>() {
                self.agent.max_tool_iterations = val;
            }
        }
        if let Ok(ac_str) = std::env::var(env_vars::MINICODE_AUTO_CONTINUE) {
            self.agent.auto_continue = ac_str == "1" || ac_str.eq_ignore_ascii_case("true");
        }
    }

    /// Returns true if the provider runs locally (e.g. Ollama, LM Studio, vLLM, LocalAI, or localhost endpoint)
    /// where API keys are strictly optional.
    pub fn is_local_provider(&self, provider_name: &str) -> bool {
        let norm = provider_name.to_lowercase();
        if matches!(
            norm.as_str(),
            "ollama"
                | "localhost"
                | "local"
                | "localai"
                | "lmstudio"
                | "lm-studio"
                | "vllm"
                | "llama.cpp"
                | "llamacpp"
                | "jan"
                | "text-generation-webui"
                | "oobabooga"
        ) {
            return true;
        }

        // Also check if custom_endpoints for this provider points to localhost or a loopback address
        if let Some(endpoint) = self.provider.custom_endpoints.get(&norm) {
            let lower = endpoint.to_lowercase();
            if lower.contains("localhost")
                || lower.contains("127.0.0.1")
                || lower.contains("0.0.0.0")
                || lower.contains("::1")
                || lower.contains("[::1]")
            {
                return true;
            }
        }

        false
    }

    /// Resolves the base URL for a provider, taking into account custom_endpoints,
    /// provider-specific defaults (Ollama, LM Studio, vLLM, etc.), or None.
    pub fn get_provider_base_url(&self, provider_name: &str) -> Option<String> {
        let norm = provider_name.to_lowercase();
        if let Some(url) = self.provider.custom_endpoints.get(&norm) {
            return Some(url.clone());
        }

        match norm.as_str() {
            "ollama" => Some(self.provider.ollama.host.clone()),
            "lmstudio" | "lm-studio" => {
                Some(crate::constants::LMSTUDIO_DEFAULT_BASE_URL.to_string())
            }
            "vllm" => Some(crate::constants::VLLM_DEFAULT_BASE_URL.to_string()),
            "local" | "localhost" | "localai" | "llama.cpp" | "llamacpp" | "jan" => {
                Some(crate::constants::LOCALAI_DEFAULT_BASE_URL.to_string())
            }
            _ => None,
        }
    }

    /// Returns default fallback model for a provider from the static catalog
    #[allow(dead_code)]
    pub fn static_default_model_for_provider(provider_name: &str) -> &'static str {
        match provider_name.to_lowercase().as_str() {
            "gemini" | "google" => "gemini-2.5-pro",
            "anthropic" | "claude" => "claude-3-7-sonnet-20250219",
            "openrouter" => "anthropic/claude-3.7-sonnet",
            "openai" => "gpt-4o",
            "deepseek" => "deepseek-chat",
            "groq" => "llama-3.3-70b-versatile",
            "together" => "meta-llama/Llama-3.3-70B-Instruct-Turbo",
            "minimax" => "MiniMax-Text-01",
            "z.ai" | "z_ai" | "zhipu" | "glm" | "bigmodel" => "glm-4-plus",
            "mistral" => "codestral-latest",
            "ollama" => "qwen2.5-coder",
            "lmstudio" | "lm-studio" | "vllm" | "local" | "localhost" | "localai" => "local-model",
            _ => "default-model",
        }
    }

    /// Associated function returning default fallback model from the static catalog
    #[allow(dead_code)]
    pub fn default_model_for_provider(provider_name: &str) -> &'static str {
        Self::static_default_model_for_provider(provider_name)
    }

    /// Returns the effective default model for a provider:
    /// First checks `self.provider.default_models.get(provider_name)`;
    /// if present and non-empty, returns it. Otherwise returns static catalog default.
    pub fn get_default_model_for_provider(&self, provider_name: &str) -> String {
        let norm = provider_name.to_lowercase();
        if let Some(model) = self
            .provider
            .default_models
            .get(&norm)
            .or_else(|| self.provider.default_models.get(provider_name))
        {
            let trimmed = model.trim();
            if !trimmed.is_empty() {
                return trimmed.to_string();
            }
        }
        Self::static_default_model_for_provider(provider_name).to_string()
    }

    /// Returns true if a local provider has been explicitly configured by the user
    /// (via custom_endpoints, api_keys, default_models, host override, or env vars).
    pub fn is_local_provider_configured(&self, provider_name: &str) -> bool {
        let norm = provider_name.to_lowercase();
        if self.provider.custom_endpoints.contains_key(&norm)
            || self.provider.api_keys.contains_key(&norm)
            || self.provider.default_models.contains_key(&norm)
        {
            return true;
        }

        match norm.as_str() {
            "ollama" => {
                std::env::var("OLLAMA_HOST").is_ok()
                    || std::env::var("OLLAMA_API_KEY").is_ok()
                    || self.provider.ollama.host != crate::constants::DEFAULT_OLLAMA_HOST
            }
            "lmstudio" | "lm-studio" => {
                std::env::var("LMSTUDIO_BASE_URL").is_ok()
                    || std::env::var("LMSTUDIO_API_KEY").is_ok()
            }
            _ => false,
        }
    }

    /// Scans known providers in deterministic priority order:
    /// `anthropic`, `gemini`, `openai`, `openrouter`, `deepseek`, `groq`, `mistral`, `together`, `minimax`, `z.ai`,
    /// followed by local providers (`ollama`, `lmstudio`).
    /// If any has a valid key (or is local and configured), returns `(provider_name, default_model)`.
    pub fn find_first_configured_provider(&self) -> Option<(&str, &str)> {
        const CLOUD_PROVIDERS: [&str; 10] = [
            "anthropic",
            "gemini",
            "openai",
            "openrouter",
            "deepseek",
            "groq",
            "mistral",
            "together",
            "minimax",
            "z.ai",
        ];

        for &provider in &CLOUD_PROVIDERS {
            if let Ok(key) = self.get_api_key(provider) {
                if !key.trim().is_empty() {
                    let norm = provider.to_lowercase();
                    let model = if let Some(custom) = self
                        .provider
                        .default_models
                        .get(&norm)
                        .or_else(|| self.provider.default_models.get(provider))
                    {
                        let trimmed = custom.trim();
                        if !trimmed.is_empty() {
                            trimmed
                        } else {
                            Self::static_default_model_for_provider(provider)
                        }
                    } else {
                        Self::static_default_model_for_provider(provider)
                    };
                    return Some((provider, model));
                }
            }
        }

        const LOCAL_PROVIDERS: [&str; 2] = ["ollama", "lmstudio"];
        for &provider in &LOCAL_PROVIDERS {
            if self.is_local_provider_configured(provider) {
                let norm = provider.to_lowercase();
                let model = if let Some(custom) = self
                    .provider
                    .default_models
                    .get(&norm)
                    .or_else(|| self.provider.default_models.get(provider))
                {
                    let trimmed = custom.trim();
                    if !trimmed.is_empty() {
                        trimmed
                    } else {
                        Self::static_default_model_for_provider(provider)
                    }
                } else {
                    Self::static_default_model_for_provider(provider)
                };
                return Some((provider, model));
            }
        }

        None
    }

    /// Resolves active provider and model using the 6-tier resolution hierarchy:
    /// 1. CLI flag / env var override (`provider.default` and `provider.model` already set)
    /// 2. Workspace preference from `workspaces.toml` (or `.minicode/config.toml` merged earlier)
    /// 3. Auto-discovery of first configured provider
    /// 4. Empty fallback `("", "")` for deferred onboarding Gate 1 setup
    pub fn resolve_active_provider_and_model(
        &self,
        workspace_root: Option<&Path>,
    ) -> (String, String) {
        self.resolve_active_provider_and_model_with_registry(
            workspace_root,
            Self::get_workspace_registry_path().as_deref(),
        )
    }

    /// Version of `resolve_active_provider_and_model` with customizable registry file path for deterministic testing.
    pub fn resolve_active_provider_and_model_with_registry(
        &self,
        workspace_root: Option<&Path>,
        registry_path: Option<&Path>,
    ) -> (String, String) {
        // 1. Explicit provider override (e.g. from CLI flag or env var override):
        if !self.provider.default.is_empty() {
            let model = if !self.provider.model.is_empty() {
                self.provider.model.clone()
            } else {
                self.get_default_model_for_provider(&self.provider.default)
            };
            return (self.provider.default.clone(), model);
        }

        // 2. Check workspace preference in workspaces.toml
        if let Some(ws) = workspace_root {
            let pref = if let Some(reg) = registry_path {
                load_workspace_preference_from_file(ws, reg)
            } else {
                Self::load_workspace_preference(ws)
            };

            if let Some(pref) = pref {
                if !pref.provider.trim().is_empty() && !pref.model.trim().is_empty() {
                    let model = if !self.provider.model.is_empty() {
                        self.provider.model.clone()
                    } else {
                        pref.model
                    };
                    return (pref.provider, model);
                }
            }
        }

        // 3. Check find_first_configured_provider()
        if let Some((prov, default_m)) = self.find_first_configured_provider() {
            let model = if !self.provider.model.is_empty() {
                self.provider.model.clone()
            } else {
                default_m.to_string()
            };
            return (prov.to_string(), model);
        }

        // 4. Empty fallback (triggering Gate 1 deferred setup when prompt arrives)
        (String::new(), String::new())
    }

    /// Resolves the API key for a specific provider.
    /// Checks environment variables first, then persistent `[provider.api_keys]` in config.toml.
    /// For local models (Ollama, LM Studio, vLLM, localhost endpoints), API keys are strictly optional.
    pub fn get_api_key(&self, provider_name: &str) -> Result<String> {
        let env_trimmed = |key: &str| std::env::var(key).map(|v| v.trim().to_string());
        let norm = provider_name.to_lowercase();

        // 1. Check environment variables
        let env_val = match norm.as_str() {
            "gemini" | "google" => {
                env_trimmed("GEMINI_API_KEY").or_else(|_| env_trimmed("GOOGLE_API_KEY"))
            }
            "anthropic" | "claude" => env_trimmed("ANTHROPIC_API_KEY"),
            "openrouter" => {
                env_trimmed("OPENROUTER_API_KEY").or_else(|_| env_trimmed("OPENROUTER_KEY"))
            }
            "openai" => env_trimmed("OPENAI_API_KEY"),
            "deepseek" => env_trimmed("DEEPSEEK_API_KEY"),
            "groq" => env_trimmed("GROQ_API_KEY"),
            "together" => env_trimmed("TOGETHER_API_KEY"),
            "minimax" => env_trimmed("MINIMAX_API_KEY"),
            "z.ai" | "z_ai" | "zhipu" | "glm" | "bigmodel" => env_trimmed("ZHIPU_API_KEY")
                .or_else(|_| env_trimmed("Z_AI_API_KEY"))
                .or_else(|_| env_trimmed("GLM_API_KEY"))
                .or_else(|_| env_trimmed("BIGMODEL_API_KEY")),
            "mistral" => env_trimmed("MISTRAL_API_KEY"),
            "ollama" => return Ok(String::new()),
            custom => {
                let sanitized_custom = custom.to_uppercase().replace(['-', '.'], "_");
                let env_var = format!("{}_API_KEY", sanitized_custom);
                env_trimmed(&env_var)
            }
        };

        if let Ok(key) = env_val {
            if !key.is_empty() {
                return Ok(key);
            }
        }

        // 2. Check persistent api_keys table in config.toml
        if let Some(key) = self
            .provider
            .api_keys
            .get(&norm)
            .or_else(|| match norm.as_str() {
                "gemini" | "google" => self
                    .provider
                    .api_keys
                    .get("gemini")
                    .or_else(|| self.provider.api_keys.get("google")),
                "z.ai" | "z_ai" | "zhipu" | "glm" | "bigmodel" => self
                    .provider
                    .api_keys
                    .get("z.ai")
                    .or_else(|| self.provider.api_keys.get("z_ai"))
                    .or_else(|| self.provider.api_keys.get("zhipu"))
                    .or_else(|| self.provider.api_keys.get("glm")),
                "openrouter" => self.provider.api_keys.get("openrouter"),
                _ => None,
            })
        {
            if !key.trim().is_empty() {
                return Ok(key.trim().to_string());
            }
        }

        // 3. Localhost and local models run without requiring an API key (it is optional)
        if self.is_local_provider(provider_name) {
            return Ok(String::new());
        }

        let env_var = match norm.as_str() {
            "gemini" | "google" => "GEMINI_API_KEY",
            "anthropic" | "claude" => "ANTHROPIC_API_KEY",
            "openrouter" => "OPENROUTER_API_KEY",
            "openai" => "OPENAI_API_KEY",
            "deepseek" => "DEEPSEEK_API_KEY",
            "groq" => "GROQ_API_KEY",
            "together" => "TOGETHER_API_KEY",
            "minimax" => "MINIMAX_API_KEY",
            "z.ai" | "z_ai" | "zhipu" | "glm" | "bigmodel" => "ZHIPU_API_KEY",
            "mistral" => "MISTRAL_API_KEY",
            _ => "",
        };

        let env_var_string = if env_var.is_empty() {
            let sanitized_custom = norm.to_uppercase().replace(['-', '.'], "_");
            format!("{}_API_KEY", sanitized_custom)
        } else {
            env_var.to_string()
        };

        Err(ConfigError::MissingApiKey {
            provider: provider_name.to_string(),
            env_var: env_var_string,
        }
        .into())
    }

    /// Returns the path to the global workspaces registry file (~/.config/minicode/workspaces.toml)
    #[allow(dead_code)]
    pub fn get_workspace_registry_path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| {
            d.join(crate::constants::CONFIG_DIR_NAME)
                .join(crate::constants::WORKSPACES_FILE_NAME)
        })
    }

    /// Loads the stored preference for the given workspace root from the global registry.
    #[allow(dead_code)]
    pub fn load_workspace_preference(workspace_root: &Path) -> Option<WorkspacePreference> {
        let registry_path = Self::get_workspace_registry_path()?;
        let canonical = match workspace_root.canonicalize() {
            Ok(c) => c,
            Err(_) => workspace_root.to_path_buf(),
        };
        load_workspace_preference_from_file(&canonical, &registry_path)
            .or_else(|| load_workspace_preference_from_file(workspace_root, &registry_path))
    }

    /// Saves the preferred provider and model for the given workspace root into the global registry.
    #[allow(dead_code)]
    pub fn save_workspace_preference(
        workspace_root: &Path,
        provider: &str,
        model: &str,
    ) -> anyhow::Result<()> {
        let registry_path = match Self::get_workspace_registry_path() {
            Some(p) => p,
            None => anyhow::bail!("Unable to determine config directory"),
        };
        let canonical = match workspace_root.canonicalize() {
            Ok(c) => c,
            Err(_) => workspace_root.to_path_buf(),
        };
        save_workspace_preference_to_file(&canonical, provider, model, &registry_path)
    }
}

/// Universally masks sensitive credentials for safe display and tool returns.
/// Empty or whitespace-only keys return empty string.
/// Keys <= 8 chars return 8 bullet points "••••••••".
/// Keys > 8 chars return first 4 chars + "..." + last 4 chars (e.g. "sk-a...cdef").
#[allow(dead_code)]
pub fn mask_api_key(key: &str) -> String {
    let trimmed = key.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let char_count = trimmed.chars().count();
    if char_count <= 8 {
        return "••••••••".to_string();
    }
    let prefix: String = trimmed.chars().take(4).collect();
    let suffix: String = trimmed.chars().skip(char_count.saturating_sub(4)).collect();
    format!("{}...{}", prefix, suffix)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[allow(dead_code)]
pub struct WorkspacePreference {
    pub provider: String,
    pub model: String,
    #[serde(default)]
    pub last_used: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[allow(dead_code)]
pub struct WorkspaceRegistry {
    #[serde(default)]
    pub workspaces: std::collections::HashMap<String, WorkspacePreference>,
}

/// Loads a workspace preference from a specific registry file path.
#[allow(dead_code)]
pub fn load_workspace_preference_from_file(
    workspace_root: &Path,
    registry_path: &Path,
) -> Option<WorkspacePreference> {
    if !registry_path.exists() {
        return None;
    }
    let content = match std::fs::read_to_string(registry_path) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(path = %registry_path.display(), error = %e, "Failed to read workspaces registry");
            return None;
        }
    };
    let registry: WorkspaceRegistry = match toml::from_str(&content) {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(path = %registry_path.display(), error = %e, "Failed to parse workspaces registry TOML");
            return None;
        }
    };
    let key = workspace_root.to_string_lossy().to_string();
    if let Some(pref) = registry.workspaces.get(&key) {
        return Some(pref.clone());
    }
    if let Ok(canon) = workspace_root.canonicalize() {
        let canon_key = canon.to_string_lossy().to_string();
        if let Some(pref) = registry.workspaces.get(&canon_key) {
            return Some(pref.clone());
        }
    }
    None
}

/// Saves or updates a workspace preference into a specific registry file path.
#[allow(dead_code)]
pub fn save_workspace_preference_to_file(
    workspace_root: &Path,
    provider: &str,
    model: &str,
    registry_path: &Path,
) -> anyhow::Result<()> {
    let mut registry = if registry_path.exists() {
        let content = std::fs::read_to_string(registry_path)?;
        toml::from_str::<WorkspaceRegistry>(&content).map_err(|e| {
            anyhow::anyhow!(
                "Failed to parse workspaces registry at {}: {}",
                registry_path.display(),
                e
            )
        })?
    } else {
        WorkspaceRegistry::default()
    };

    if let Some(parent) = registry_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let key = workspace_root.to_string_lossy().to_string();
    registry.workspaces.insert(
        key,
        WorkspacePreference {
            provider: provider.to_string(),
            model: model.to_string(),
            last_used: chrono::Utc::now().to_rfc3339(),
        },
    );

    let content = toml::to_string_pretty(&registry)?;
    std::fs::write(registry_path, content)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.provider.default, "");
        assert_eq!(config.provider.model, "");
        assert_eq!(config.agent.timeout, 30);
        assert_eq!(config.agent.map_tokens, 1024);
        assert_eq!(config.agent.max_tool_iterations, 0);
        assert!(config.agent.auto_continue);
        assert_eq!(config.agent.max_auto_continues, 5);
        assert!(!config.ui.plain);
    }

    #[test]
    fn test_agent_iteration_config_merge() {
        let toml_str = r#"
            [agent]
            max_tool_iterations = 25
            auto_continue = false
            max_auto_continues = 2
        "#;
        let raw: RawConfig = toml::from_str(toml_str).unwrap();
        let mut config = Config::default();
        config.merge_raw(raw);
        assert_eq!(config.agent.max_tool_iterations, 25);
        assert!(!config.agent.auto_continue);
        assert_eq!(config.agent.max_auto_continues, 2);

        // Test explicit 0 (unbounded) merge
        let unbounded_toml = r#"
            [agent]
            max_tool_iterations = 0
        "#;
        let raw_unbounded: RawConfig = toml::from_str(unbounded_toml).unwrap();
        config.merge_raw(raw_unbounded);
        assert_eq!(config.agent.max_tool_iterations, 0);
    }

    #[test]
    fn test_toml_parsing() {
        let toml_content = r#"
            [provider]
            default = "anthropic"
            model = "claude-3-7-sonnet"
            temperature = 0.5
            max_tokens = 4096

            [agent]
            auto_approve = true
            timeout = 60
            map_tokens = 2048

            [ui]
            theme = "dark"
            plain = true
        "#;

        let config: Config = toml::from_str(toml_content).unwrap();
        assert_eq!(config.provider.default, "anthropic");
        assert_eq!(config.provider.model, "claude-3-7-sonnet");
        assert!(config.agent.auto_approve);
        assert_eq!(config.agent.timeout, 60);
        assert_eq!(config.agent.map_tokens, 2048);
        assert!(config.ui.plain);
        assert_eq!(config.ui.theme, "dark");
    }

    #[test]
    fn test_raw_config_merge_animation_and_ui_preferences() {
        let mut config = Config::default();
        assert_eq!(config.ui.animation, "dual_pillars");

        let override_toml = r#"
            [ui]
            animation = "braille_wave"
            theme = "monokai"
            show_cost = true

            [agent]
            tool_mode = "full"
            syntax_barrier = false
            auto_lint = false
        "#;
        let raw: RawConfig = toml::from_str(override_toml).unwrap();
        config.merge_raw(raw);
        assert_eq!(config.ui.animation, "braille_wave");
        assert_eq!(config.ui.theme, "monokai");
        assert!(config.ui.show_cost);
        assert_eq!(config.agent.tool_mode, ToolFilterMode::Full);
        assert!(!config.agent.syntax_barrier);
        assert!(!config.agent.auto_lint);
    }

    #[test]
    fn test_raw_config_merge_override_false() {
        let mut config = Config::default();
        config.agent.auto_approve = true;

        let override_toml = r#"
            [agent]
            auto_approve = false
        "#;
        let raw: RawConfig = toml::from_str(override_toml).unwrap();
        config.merge_raw(raw);
        assert!(!config.agent.auto_approve);
    }

    #[test]
    fn test_get_api_key_trims_whitespace() {
        let config = Config::default();
        std::env::set_var("GEMINI_API_KEY", "  sk-test-gemini-key-12345\n\t ");
        let key = config.get_api_key("gemini").unwrap();
        assert_eq!(key, "sk-test-gemini-key-12345");
        std::env::remove_var("GEMINI_API_KEY");
    }

    #[test]
    fn test_config_merges_ollama_from_toml() {
        let mut config = Config::default();
        let override_toml = r#"
            [provider.ollama]
            host = "http://192.168.1.100:11434"
        "#;
        let raw: RawConfig = toml::from_str(override_toml).unwrap();
        config.merge_raw(raw);
        assert_eq!(config.provider.ollama.host, "http://192.168.1.100:11434");
    }

    #[test]
    fn test_config_merges_git_from_toml() {
        let mut config = Config::default();
        assert!(config.git.auto_commit);
        assert!(!config.git.dirty_commit);

        let override_toml = r#"
            [git]
            auto_commit = false
            dirty_commit = true
            ai_commit_messages = false
        "#;
        let raw: RawConfig = toml::from_str(override_toml).unwrap();
        config.merge_raw(raw);
        assert!(!config.git.auto_commit);
        assert!(config.git.dirty_commit);
        assert!(!config.git.ai_commit_messages);
    }

    #[test]
    fn test_get_api_key_from_persistent_map() {
        let mut config = Config::default();
        config.provider.api_keys.insert(
            "openrouter".to_string(),
            "sk-or-v1-persist-test".to_string(),
        );
        config
            .provider
            .api_keys
            .insert("minimax".to_string(), "mm-test-key".to_string());
        config
            .provider
            .api_keys
            .insert("z.ai".to_string(), "glm-test-key".to_string());

        assert_eq!(
            config.get_api_key("openrouter").unwrap(),
            "sk-or-v1-persist-test"
        );
        assert_eq!(config.get_api_key("minimax").unwrap(), "mm-test-key");
        assert_eq!(config.get_api_key("z.ai").unwrap(), "glm-test-key");
        assert_eq!(config.get_api_key("zhipu").unwrap(), "glm-test-key");
        assert_eq!(config.get_api_key("ollama").unwrap(), "");
    }

    #[test]
    fn test_local_providers_key_optional() {
        let mut config = Config::default();
        // Ollama, LM Studio, vLLM, localhost all return Ok("") without requiring API keys
        assert_eq!(config.get_api_key("ollama").unwrap(), "");
        assert_eq!(config.get_api_key("localhost").unwrap(), "");
        assert_eq!(config.get_api_key("local").unwrap(), "");
        assert_eq!(config.get_api_key("lmstudio").unwrap(), "");
        assert_eq!(config.get_api_key("vllm").unwrap(), "");
        assert_eq!(config.get_api_key("localai").unwrap(), "");
        assert_eq!(config.get_api_key("llama.cpp").unwrap(), "");

        // Custom endpoint on localhost is also detected as local
        config.provider.custom_endpoints.insert(
            "my-local-server".to_string(),
            "http://127.0.0.1:9090/v1".to_string(),
        );
        assert!(config.is_local_provider("my-local-server"));
        assert_eq!(config.get_api_key("my-local-server").unwrap(), "");

        // Base URL resolution
        assert_eq!(
            config.get_provider_base_url("lmstudio").unwrap(),
            "http://localhost:1234/v1"
        );
        assert_eq!(
            config.get_provider_base_url("vllm").unwrap(),
            "http://localhost:8000/v1"
        );
        assert_eq!(
            config.get_provider_base_url("my-local-server").unwrap(),
            "http://127.0.0.1:9090/v1"
        );

        // Default models
        assert_eq!(
            config.get_default_model_for_provider("ollama"),
            "qwen2.5-coder"
        );
        assert_eq!(
            config.get_default_model_for_provider("lmstudio"),
            "local-model"
        );
    }

    #[test]
    fn test_mask_api_key_variations() {
        assert_eq!(mask_api_key(""), "");
        assert_eq!(mask_api_key("   "), "");
        assert_eq!(mask_api_key("short"), "••••••••");
        assert_eq!(mask_api_key("12345678"), "••••••••");
        assert_eq!(mask_api_key("123456789"), "1234...6789");
        assert_eq!(mask_api_key("🔑1234567890🦀"), "🔑123...890🦀");
        assert_eq!(mask_api_key("sk-ant-1234567890abcdef"), "sk-a...cdef");
        assert_eq!(mask_api_key("AIzaSyD-1234567890XYZ"), "AIza...0XYZ");
    }

    #[test]
    fn test_workspace_preference_roundtrip() {
        let temp_dir = tempfile::tempdir().unwrap();
        let ws_path = temp_dir.path();

        // Initially none
        let pref = load_workspace_preference_from_file(ws_path, &ws_path.join("workspaces.toml"));
        assert!(pref.is_none());

        // Save preference
        save_workspace_preference_to_file(
            ws_path,
            "anthropic",
            "claude-3-7-sonnet-20250219",
            &ws_path.join("workspaces.toml"),
        )
        .unwrap();

        let pref =
            load_workspace_preference_from_file(ws_path, &ws_path.join("workspaces.toml")).unwrap();
        assert_eq!(pref.provider, "anthropic");
        assert_eq!(pref.model, "claude-3-7-sonnet-20250219");
    }

    #[test]
    fn test_dynamic_provider_resolution_hierarchy() {
        let temp_dir = tempfile::tempdir().unwrap();
        let ws_path = temp_dir.path();
        let reg_path = ws_path.join("workspaces.toml");

        let mut config = Config::default();
        config.provider.default = String::new();
        config.provider.model = String::new();

        // 0. Empty fallback when nothing is set and no workspace pref
        let empty_reg_dir = tempfile::tempdir().unwrap();
        let empty_ws = empty_reg_dir.path().join("empty_ws");
        let empty_reg = empty_reg_dir.path().join("workspaces.toml");
        let (empty_p, empty_m) = config
            .resolve_active_provider_and_model_with_registry(Some(&empty_ws), Some(&empty_reg));
        assert_eq!(empty_p, "");
        assert_eq!(empty_m, "");

        // 1. Auto-discovery from configured API keys when no workspace pref exists
        config
            .provider
            .api_keys
            .insert("openai".to_string(), "sk-test-key".to_string());
        let (disc_p, disc_m) = config
            .resolve_active_provider_and_model_with_registry(Some(&empty_ws), Some(&empty_reg));
        assert_eq!(disc_p, "openai");
        assert_eq!(disc_m, "gpt-4o");
        config.provider.api_keys.clear();

        // 2. With workspace preference saved
        save_workspace_preference_to_file(
            ws_path,
            "anthropic",
            "claude-3-7-sonnet-20250219",
            &reg_path,
        )
        .unwrap();

        let (prov, model) =
            config.resolve_active_provider_and_model_with_registry(Some(ws_path), Some(&reg_path));
        assert_eq!(prov, "anthropic");
        assert_eq!(model, "claude-3-7-sonnet-20250219");

        // 3. Explicit provider without model overrides workspace preference and gets default model
        config.provider.default = "mistral".to_string();
        config.provider.model = String::new();
        let (prov, model) =
            config.resolve_active_provider_and_model_with_registry(Some(ws_path), Some(&reg_path));
        assert_eq!(prov, "mistral");
        assert_eq!(model, config.get_default_model_for_provider("mistral"));

        // 4. CLI / explicit override with both takes highest precedence over workspace preference
        config.provider.default = "deepseek".to_string();
        config.provider.model = "deepseek-chat".to_string();
        let (prov, model) =
            config.resolve_active_provider_and_model_with_registry(Some(ws_path), Some(&reg_path));
        assert_eq!(prov, "deepseek");
        assert_eq!(model, "deepseek-chat");
    }

    #[test]
    fn test_default_models_override() {
        let mut config = Config::default();
        assert_eq!(
            config.get_default_model_for_provider("anthropic"),
            "claude-3-7-sonnet-20250219"
        );
        assert_eq!(config.get_default_model_for_provider("openai"), "gpt-4o");

        config.provider.default_models.insert(
            "anthropic".to_string(),
            "claude-3-5-haiku-20241022".to_string(),
        );
        config
            .provider
            .default_models
            .insert("openai".to_string(), "o3-mini".to_string());

        assert_eq!(
            config.get_default_model_for_provider("anthropic"),
            "claude-3-5-haiku-20241022"
        );
        assert_eq!(config.get_default_model_for_provider("openai"), "o3-mini");
        assert_eq!(
            config.get_default_model_for_provider("gemini"),
            "gemini-2.5-pro"
        );
    }
}

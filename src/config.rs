use crate::constants::{
    DEFAULT_MODEL_GEMINI, DEFAULT_PROVIDER,
};
use crate::error::{ConfigError, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
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
    #[serde(default = "default_provider_name")]
    pub default: String,

    #[serde(default = "default_model_name")]
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
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            default: default_provider_name(),
            model: default_model_name(),
            ollama: OllamaConfig::default(),
            temperature: default_temperature(),
            max_tokens: default_max_tokens(),
            api_keys: std::collections::HashMap::new(),
            custom_endpoints: std::collections::HashMap::new(),
        }
    }
}

fn default_provider_name() -> String {
    DEFAULT_PROVIDER.to_string()
}

fn default_model_name() -> String {
    DEFAULT_MODEL_GEMINI.to_string()
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
    "http://localhost:11434".to_string()
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
        }
    }
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UiConfig {
    #[serde(default = "default_theme")]
    pub theme: String,

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
            plain: false,
            max_width: default_max_width(),
            show_cost: false,
        }
    }
}

fn default_theme() -> String {
    "auto".to_string()
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
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RawUiConfig {
    pub plain: Option<bool>,
    pub theme: Option<String>,
    pub max_width: Option<usize>,
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
        if let Some(plain) = other.ui.plain {
            self.ui.plain = plain;
        }
        if let Some(theme) = other.ui.theme {
            self.ui.theme = theme;
        }
        if let Some(max_width) = other.ui.max_width {
            self.ui.max_width = max_width;
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
        if let Some(policy) = other.mcp.approval_policy {
            self.mcp.approval_policy = Some(policy);
        }
        for (name, srv) in other.mcp.servers {
            self.mcp.servers.insert(name, srv);
        }
    }

    fn apply_env_overrides(&mut self) {
        if let Ok(model) = std::env::var("MINICODE_MODEL") {
            self.provider.model = model;
        }
        if let Ok(provider) = std::env::var("MINICODE_PROVIDER") {
            self.provider.default = provider;
        }
        if let Ok(auto_approve) = std::env::var("MINICODE_AUTO_APPROVE") {
            self.agent.auto_approve =
                auto_approve == "1" || auto_approve.eq_ignore_ascii_case("true");
        }
        if let Ok(policy) = std::env::var("MINICODE_APPROVAL_POLICY") {
            self.agent.approval_policy = policy;
        }
        if let Ok(temp_str) = std::env::var("MINICODE_TEMPERATURE") {
            if let Ok(temp) = temp_str.parse::<f32>() {
                self.provider.temperature = temp;
            }
        }
        if let Ok(tokens_str) = std::env::var("MINICODE_MAX_TOKENS") {
            if let Ok(tokens) = tokens_str.parse::<usize>() {
                self.provider.max_tokens = tokens;
            }
        }
        if let Ok(timeout_str) = std::env::var("MINICODE_TIMEOUT") {
            if let Ok(timeout) = timeout_str.parse::<u64>() {
                self.agent.timeout = timeout;
            }
        }
        if let Ok(plain) = std::env::var("MINICODE_PLAIN") {
            self.ui.plain = plain == "1" || plain.eq_ignore_ascii_case("true");
        }
        if let Ok(theme) = std::env::var("MINICODE_THEME") {
            self.ui.theme = theme;
        }
        if let Ok(level) = std::env::var("MINICODE_LOG_LEVEL") {
            self.logging.level = level;
        }
    }

    /// Resolves the API key for a specific provider.
    /// Checks environment variables first, then persistent `[provider.api_keys]` in config.toml.
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

        // 3. Ollama runs locally without requiring an API key
        if norm == "ollama" {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.provider.default, "gemini");
        assert_eq!(config.provider.model, "gemini-2.5-pro");
        assert_eq!(config.agent.timeout, 30);
        assert_eq!(config.agent.map_tokens, 1024);
        assert!(!config.ui.plain);
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
}

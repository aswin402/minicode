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
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            default: default_provider_name(),
            model: default_model_name(),
            ollama: OllamaConfig::default(),
            temperature: default_temperature(),
            max_tokens: default_max_tokens(),
        }
    }
}

fn default_provider_name() -> String {
    "gemini".to_string()
}

fn default_model_name() -> String {
    "gemini-2.5-pro".to_string()
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
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            auto_approve: false,
            approval_policy: default_approval_policy(),
            timeout: default_timeout_secs(),
            map_tokens: default_map_tokens(),
            warning_threshold: default_warning_threshold(),
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
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            theme: default_theme(),
            plain: false,
            max_width: default_max_width(),
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

impl Config {
    /// Loads configuration respecting the hierarchy:
    /// 1. Project-local `.minicode/config.toml` (if present)
    /// 2. Global `~/.config/minicode/config.toml` (if present)
    /// 3. Built-in defaults
    /// 4. Environment variable overrides (MINICODE_*)
    pub fn load(workspace_dir: Option<&Path>, custom_config_path: Option<&Path>) -> Result<Self> {
        // Load .env if present
        if let Some(dir) = workspace_dir {
            let env_file = dir.join(".env");
            if env_file.exists() {
                dotenvy::from_path(&env_file).ok();
            }
        }
        dotenvy::dotenv().ok();

        let mut config = Config::default();

        // 1. Try custom config path if explicitly specified
        if let Some(path) = custom_config_path {
            if path.exists() {
                config = Self::load_from_file(path)?;
            }
        } else {
            // 2. Global config: ~/.config/minicode/config.toml
            if let Some(global_dir) = dirs::config_dir() {
                let global_config = global_dir.join("minicode").join("config.toml");
                if global_config.exists() {
                    config = Self::load_from_file(&global_config)?;
                }
            }

            // 3. Project-local config: <workspace>/.minicode/config.toml
            if let Some(dir) = workspace_dir {
                let local_config = dir.join(".minicode").join("config.toml");
                if local_config.exists() {
                    let local = Self::load_from_file(&local_config)?;
                    config.merge(local);
                }
            }
        }

        // 4. Apply environment variable overrides
        config.apply_env_overrides();

        Ok(config)
    }

    fn load_from_file(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path).map_err(|e| ConfigError::FileRead {
            path: path.display().to_string(),
            source: e,
        })?;

        let parsed: Config = toml::from_str(&content).map_err(ConfigError::TomlParse)?;
        tracing::debug!(path = %path.display(), "Loaded configuration from file");
        Ok(parsed)
    }

    fn merge(&mut self, other: Config) {
        if other.provider.default != default_provider_name() {
            self.provider.default = other.provider.default;
        }
        if other.provider.model != default_model_name() {
            self.provider.model = other.provider.model;
        }
        if other.agent.auto_approve {
            self.agent.auto_approve = true;
        }
        if other.agent.timeout != default_timeout_secs() {
            self.agent.timeout = other.agent.timeout;
        }
        if other.agent.map_tokens != default_map_tokens() {
            self.agent.map_tokens = other.agent.map_tokens;
        }
        if other.ui.plain {
            self.ui.plain = true;
        }
        if other.ui.theme != default_theme() {
            self.ui.theme = other.ui.theme;
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

    /// Resolves the API key for a specific provider from the environment.
    pub fn get_api_key(&self, provider_name: &str) -> Result<String> {
        match provider_name.to_lowercase().as_str() {
            "gemini" | "google" => std::env::var("GEMINI_API_KEY")
                .or_else(|_| std::env::var("GOOGLE_API_KEY"))
                .map_err(|_| {
                    ConfigError::MissingApiKey {
                        provider: "gemini".to_string(),
                        env_var: "GEMINI_API_KEY".to_string(),
                    }
                    .into()
                }),
            "anthropic" | "claude" => std::env::var("ANTHROPIC_API_KEY").map_err(|_| {
                ConfigError::MissingApiKey {
                    provider: "anthropic".to_string(),
                    env_var: "ANTHROPIC_API_KEY".to_string(),
                }
                .into()
            }),
            "openrouter" => std::env::var("OPENROUTER_API_KEY")
                .or_else(|_| std::env::var("OPENROUTER_KEY"))
                .map_err(|_| {
                    ConfigError::MissingApiKey {
                        provider: "openrouter".to_string(),
                        env_var: "OPENROUTER_API_KEY".to_string(),
                    }
                    .into()
                }),
            "openai" => std::env::var("OPENAI_API_KEY").map_err(|_| {
                ConfigError::MissingApiKey {
                    provider: "openai".to_string(),
                    env_var: "OPENAI_API_KEY".to_string(),
                }
                .into()
            }),
            "ollama" => {
                // Ollama runs locally and does not require an API key by default
                Ok(String::new())
            }
            custom => {
                let env_var = format!("{}_API_KEY", custom.to_uppercase());
                std::env::var(&env_var).map_err(|_| {
                    ConfigError::MissingApiKey {
                        provider: custom.to_string(),
                        env_var,
                    }
                    .into()
                })
            }
        }
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
        assert_eq!(config.ui.plain, false);
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
        assert_eq!(config.agent.auto_approve, true);
        assert_eq!(config.agent.timeout, 60);
        assert_eq!(config.agent.map_tokens, 2048);
        assert_eq!(config.ui.plain, true);
        assert_eq!(config.ui.theme, "dark");
    }
}

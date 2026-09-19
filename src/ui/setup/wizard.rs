use std::io::{self, IsTerminal, Write};
use std::path::Path;

use crate::config::Config;
use crate::error::{MinicodeError, Result};
use crate::ui::configure::ConfigMenu;
use crate::ui::setup::{
    mask_api_key, prompt_api_key, prompt_text, InteractiveSelector, SelectorItem,
};

/// Modern interactive setup wizard for minicode.
pub struct SetupWizard;

impl SetupWizard {
    /// Returns the static catalog of available LLM providers.
    ///
    /// Each entry contains: `(id, display_label, hint)`.
    pub fn provider_catalog() -> &'static [(&'static str, &'static str, &'static str)] {
        &[
            (
                "openrouter",
                "OpenRouter",
                "100+ models: Claude, DeepSeek, Qwen",
            ),
            ("gemini", "Google Gemini", "Gemini 2.5 Pro & Flash"),
            ("openai", "OpenAI", "GPT-4o, o3-mini"),
            ("deepseek", "DeepSeek", "DeepSeek-V3, R1 — ultra-low cost"),
            ("groq", "Groq", "Llama 3.3, Qwen — fast inference"),
            ("minimax", "MiniMax", "MiniMax-M2.7, Text-01"),
            ("z.ai", "Z.ai / Zhipu GLM", "GLM-4-Plus, GLM-4-Flash"),
            ("together", "Together AI", "Open-source model hosting"),
            ("mistral", "Mistral AI", "Codestral, Mistral Large"),
            (
                "ollama",
                "Ollama (Local)",
                "100% Free local at localhost:11434",
            ),
        ]
    }

    /// Maps standard provider IDs to their canonical environment variable names.
    pub fn env_var_for_provider(id: &str) -> &'static str {
        match id {
            "openrouter" => "OPENROUTER_API_KEY",
            "gemini" => "GEMINI_API_KEY",
            "openai" => "OPENAI_API_KEY",
            "deepseek" => "DEEPSEEK_API_KEY",
            "groq" => "GROQ_API_KEY",
            "minimax" => "MINIMAX_API_KEY",
            "z.ai" => "ZHIPU_API_KEY",
            "together" => "TOGETHER_API_KEY",
            "mistral" => "MISTRAL_API_KEY",
            "ollama" => "OLLAMA_API_KEY",
            _ => "",
        }
    }

    /// Computes environment variable name for a provider ID (standard or custom).
    pub fn custom_env_var(id: &str) -> String {
        let static_var = Self::env_var_for_provider(id);
        if !static_var.is_empty() {
            static_var.to_string()
        } else {
            format!("{}_API_KEY", id.to_uppercase().replace(['-', '.'], "_"))
        }
    }

    /// Computes the status badge for a provider given the current configuration.
    pub fn provider_badge(config: &Config, id: &str) -> String {
        if config.provider.default == id {
            "\x1b[32m● Active\x1b[0m".to_string()
        } else {
            let key_opt = match config.get_api_key(id) {
                Ok(k) if !k.trim().is_empty() => Some(k),
                _ => None,
            };
            if let Some(k) = key_opt {
                let masked = mask_api_key(&k);
                format!("\x1b[36m✔ Configured ({masked})\x1b[0m")
            } else if id == "ollama" {
                "\x1b[33m○ Localhost\x1b[0m".to_string()
            } else {
                "\x1b[90m○ Not Set\x1b[0m".to_string()
            }
        }
    }

    /// Builds the list of selector items for the Available Providers menu.
    pub fn build_available_provider_items(config: &Config) -> (Vec<SelectorItem>, usize) {
        let catalog = Self::provider_catalog();
        let mut items = Vec::with_capacity(catalog.len() + 1);
        let mut initial_index = 0;

        for (i, (id, label, hint)) in catalog.iter().enumerate() {
            let badge = Self::provider_badge(config, id);
            items.push(
                SelectorItem::new(*id, *label)
                    .with_badge(badge)
                    .with_hint(*hint),
            );
            if config.provider.default == *id {
                initial_index = i;
            }
        }
        items.push(SelectorItem::new("back", "◄ Back").with_hint("Return to provider menu"));
        (items, initial_index)
    }

    /// Verifies that stdin is connected to an interactive TTY.
    pub fn verify_interactive_terminal() -> Result<()> {
        if !io::stdin().is_terminal() {
            return Err(MinicodeError::Ui(
                "'minicode setup' requires an interactive terminal.".to_string(),
            ));
        }
        Ok(())
    }

    /// Prints a formatted confirmation receipt after a provider change.
    pub fn print_receipt(provider: &str, model: &str, message: &str) {
        let mut stdout = io::stdout();
        let _ = writeln!(stdout, "\n  \x1b[1;32m✔\x1b[0m \x1b[1m{}\x1b[0m", message);
        let _ = writeln!(
            stdout,
            "    \x1b[90mActive Provider:\x1b[0m \x1b[36m{}\x1b[0m",
            provider
        );
        let _ = writeln!(
            stdout,
            "    \x1b[90mActive Model:   \x1b[0m \x1b[33m{}\x1b[0m",
            model
        );
        let _ = writeln!(
            stdout,
            "    \x1b[90mConfiguration saved to ~/.config/minicode/config.toml & .env\x1b[0m\n"
        );
        let _ = stdout.flush();
    }

    /// Prints the final exit confirmation receipt.
    pub fn print_exit_receipt(config: &Config) {
        let mut stdout = io::stdout();
        let _ = writeln!(
            stdout,
            "\n  \x1b[1;32m✔ Configuration saved successfully.\x1b[0m"
        );
        let _ = writeln!(
            stdout,
            "    \x1b[90mActive Provider:\x1b[0m \x1b[36m{}\x1b[0m",
            config.provider.default
        );
        let _ = writeln!(
            stdout,
            "    \x1b[90mActive Model:   \x1b[0m \x1b[33m{}\x1b[0m\n",
            config.provider.model
        );
        let _ = stdout.flush();
    }

    /// Handles selection of a provider from the Available Providers catalog.
    pub fn handle_available_provider_selection(
        config: &mut Config,
        workspace: &Path,
        id: &str,
        label: &str,
    ) -> Result<()> {
        if id == "ollama" {
            config.provider.default = "ollama".to_string();
            ConfigMenu::save_all(config, workspace)?;
            Self::print_receipt(
                &config.provider.default,
                &config.provider.model,
                "Local Ollama provider activated!",
            );
            return Ok(());
        }

        let existing_key = match config.get_api_key(id) {
            Ok(k) if !k.trim().is_empty() => Some(k),
            _ => None,
        };

        match existing_key {
            None => {
                let key_input = prompt_api_key(label, None)?;
                if let Some(key) = key_input {
                    let env_var = Self::custom_env_var(id);
                    std::env::set_var(&env_var, &key);
                    config.provider.api_keys.insert(id.to_string(), key);
                    config.provider.default = id.to_string();
                    ConfigMenu::save_all(config, workspace)?;
                    Self::print_receipt(
                        &config.provider.default,
                        &config.provider.model,
                        &format!("Provider '{}' configured and activated!", label),
                    );
                }
            }
            Some(cur_key) => {
                let manage_prompt = format!(
                    "=== Manage {} ===\n\x1b[90mStatus: Configured ({})\x1b[0m",
                    label,
                    mask_api_key(&cur_key)
                );
                let manage_items = vec![
                    SelectorItem::new("activate", "⚡ Set as Active Provider")
                        .with_hint("Make this your primary LLM provider"),
                    SelectorItem::new("reconfigure", "🔑 Reconfigure API Key")
                        .with_hint("Enter or paste a replacement key"),
                    SelectorItem::new("back", "◄ Back").with_hint("Return to provider list"),
                ];

                let selector = InteractiveSelector::new();
                let manage_choice = selector.select(&manage_prompt, &manage_items, 0)?;

                match manage_choice {
                    Some(0) => {
                        config.provider.default = id.to_string();
                        ConfigMenu::save_all(config, workspace)?;
                        Self::print_receipt(
                            &config.provider.default,
                            &config.provider.model,
                            &format!("Provider '{}' set as active!", label),
                        );
                    }
                    Some(1) => {
                        let key_input = prompt_api_key(label, Some(&cur_key))?;
                        if let Some(new_key) = key_input {
                            let env_var = Self::custom_env_var(id);
                            std::env::set_var(&env_var, &new_key);
                            config.provider.api_keys.insert(id.to_string(), new_key);
                            config.provider.default = id.to_string();
                            ConfigMenu::save_all(config, workspace)?;
                            Self::print_receipt(
                                &config.provider.default,
                                &config.provider.model,
                                &format!("API key updated and '{}' set as active!", label),
                            );
                        }
                    }
                    _ => {}
                }
            }
        }

        Ok(())
    }

    /// Applies custom provider settings to config, sets as active, and saves.
    pub fn apply_custom_provider(
        config: &mut Config,
        workspace: &Path,
        name: &str,
        base_url: &str,
        api_key: Option<&str>,
    ) -> Result<()> {
        config
            .provider
            .custom_endpoints
            .insert(name.to_string(), base_url.to_string());

        if let Some(key) = api_key {
            let env_var = format!("{}_API_KEY", name.to_uppercase().replace(['-', '.'], "_"));
            std::env::set_var(&env_var, key);
            config
                .provider
                .api_keys
                .insert(name.to_string(), key.to_string());
        }

        config.provider.default = name.to_string();
        ConfigMenu::save_all(config, workspace)?;

        Self::print_receipt(
            &config.provider.default,
            &config.provider.model,
            &format!("Custom provider '{}' configured and activated!", name),
        );

        Ok(())
    }

    /// Level 3: Custom Provider interactive input workflow.
    pub fn menu_custom_provider(config: &mut Config, workspace: &Path) -> Result<()> {
        let name_res = prompt_text("Provider Identifier Name (e.g. 'vllm-local')", None, false)?;
        let name = match name_res {
            Some(n) if !n.trim().is_empty() => n.trim().to_lowercase(),
            _ => return Ok(()),
        };

        let url_res = prompt_text(
            "OpenAI-Compatible Base URL",
            Some("http://localhost:8000/v1"),
            false,
        )?;
        let base_url = match url_res {
            Some(u) if !u.trim().is_empty() => u.trim().to_string(),
            _ => return Ok(()),
        };

        let key_res = prompt_text("API Key (optional, press Enter to skip)", None, true)?;
        let api_key = match key_res {
            Some(k) if !k.trim().is_empty() => Some(k.trim().to_string()),
            Some(_) => None,
            None => return Ok(()),
        };

        Self::apply_custom_provider(config, workspace, &name, &base_url, api_key.as_deref())
    }

    /// Level 3: Available Providers menu loop.
    pub fn menu_available_providers(config: &mut Config, workspace: &Path) -> Result<()> {
        loop {
            let (items, initial_index) = Self::build_available_provider_items(config);
            let catalog = Self::provider_catalog();

            let prompt = format!(
                "=== Available Providers ===\n\x1b[90mActive Provider: \x1b[36m{}\x1b[90m | Active Model: \x1b[33m{}\x1b[0m",
                config.provider.default, config.provider.model
            );

            let selector = InteractiveSelector::new();
            let choice = selector.select(&prompt, &items, initial_index)?;

            match choice {
                Some(idx) => {
                    if idx >= catalog.len() || items[idx].id == "back" {
                        break;
                    }
                    let (id, label, _) = catalog[idx];
                    Self::handle_available_provider_selection(config, workspace, id, label)?;
                }
                None => {
                    break;
                }
            }
        }
        Ok(())
    }

    /// Level 2: Provider Configuration menu loop.
    pub fn menu_providers(config: &mut Config, workspace: &Path) -> Result<()> {
        loop {
            let prompt = format!(
                "=== Provider Configuration ===\n\x1b[90mActive Provider: \x1b[36m{}\x1b[90m | Active Model: \x1b[33m{}\x1b[0m",
                config.provider.default, config.provider.model
            );
            let items = vec![
                SelectorItem::new("available", "🌐 Available Providers")
                    .with_hint("MiniMax, Z.ai, OpenRouter, Gemini, OpenAI, etc."),
                SelectorItem::new("custom", "🔌 Custom Provider")
                    .with_hint("OpenAI-compatible endpoints (vLLM, Ollama, etc.)"),
                SelectorItem::new("back", "◄ Back").with_hint("Return to main menu"),
            ];

            let selector = InteractiveSelector::new();
            let choice = selector.select(&prompt, &items, 0)?;

            match choice {
                Some(0) => {
                    Self::menu_available_providers(config, workspace)?;
                }
                Some(1) => {
                    Self::menu_custom_provider(config, workspace)?;
                }
                Some(2) | None => {
                    break;
                }
                _ => break,
            }
        }
        Ok(())
    }

    /// Entrypoint for the interactive setup wizard.
    pub async fn run(workspace: &Path) -> Result<()> {
        Self::verify_interactive_terminal()?;

        let mut config = Config::load(Some(workspace), None).unwrap_or_default();

        loop {
            let prompt = format!(
                "⚡ minicode — Interactive Setup Wizard\n\x1b[90mActive Provider: \x1b[36m{}\x1b[90m | Active Model: \x1b[33m{}\x1b[0m",
                config.provider.default, config.provider.model
            );
            let items = vec![
                SelectorItem::new("provider", "⚡ Provider")
                    .with_hint("Setup & manage AI providers"),
                SelectorItem::new("back", "◄ Back / Exit")
                    .with_hint("Save changes and return to shell"),
            ];

            let selector = InteractiveSelector::new();
            let choice = selector.select(&prompt, &items, 0)?;

            match choice {
                Some(0) => {
                    Self::menu_providers(&mut config, workspace)?;
                }
                Some(1) | None => {
                    ConfigMenu::save_all(&config, workspace)?;
                    Self::print_exit_receipt(&config);
                    break;
                }
                _ => break,
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_catalog_contains_standard_providers() {
        let catalog = SetupWizard::provider_catalog();
        let ids: Vec<&str> = catalog.iter().map(|(id, _, _)| *id).collect();
        assert!(ids.contains(&"minimax"));
        assert!(ids.contains(&"z.ai"));
        assert!(ids.contains(&"openrouter"));
        assert!(ids.contains(&"gemini"));
        assert!(ids.contains(&"openai"));
        assert!(ids.contains(&"deepseek"));
        assert!(ids.contains(&"groq"));
        assert!(ids.contains(&"ollama"));
        assert!(ids.contains(&"together"));
        assert!(ids.contains(&"mistral"));
        assert_eq!(catalog.len(), 10);
    }

    #[test]
    fn test_env_var_mapping() {
        assert_eq!(
            SetupWizard::env_var_for_provider("openrouter"),
            "OPENROUTER_API_KEY"
        );
        assert_eq!(SetupWizard::env_var_for_provider("z.ai"), "ZHIPU_API_KEY");
        assert_eq!(
            SetupWizard::env_var_for_provider("ollama"),
            "OLLAMA_API_KEY"
        );
        assert_eq!(
            SetupWizard::env_var_for_provider("minimax"),
            "MINIMAX_API_KEY"
        );
        assert_eq!(SetupWizard::env_var_for_provider("unknown"), "");

        assert_eq!(SetupWizard::custom_env_var("z.ai"), "ZHIPU_API_KEY");
        assert_eq!(
            SetupWizard::custom_env_var("vllm-local"),
            "VLLM_LOCAL_API_KEY"
        );
        assert_eq!(
            SetupWizard::custom_env_var("my.cool.llm"),
            "MY_COOL_LLM_API_KEY"
        );
    }

    #[test]
    fn test_provider_badge_states() {
        let mut config = Config::default();
        config.provider.default = "gemini".to_string();

        // 1. Active provider
        let badge_active = SetupWizard::provider_badge(&config, "gemini");
        assert!(badge_active.contains("● Active"));

        // 2. Not set provider
        let badge_not_set = SetupWizard::provider_badge(&config, "minimax");
        assert!(badge_not_set.contains("○ Not Set"));

        // 3. Configured provider (with key)
        config
            .provider
            .api_keys
            .insert("minimax".to_string(), "sk-minimax-123456789".to_string());
        let badge_configured = SetupWizard::provider_badge(&config, "minimax");
        assert!(badge_configured.contains("✔ Configured"));
        assert!(badge_configured.contains("sk-min••••6789"));

        // 4. Ollama (when not active)
        let badge_ollama = SetupWizard::provider_badge(&config, "ollama");
        assert!(badge_ollama.contains("○ Localhost"));

        // 5. Ollama (when active)
        config.provider.default = "ollama".to_string();
        let badge_ollama_active = SetupWizard::provider_badge(&config, "ollama");
        assert!(badge_ollama_active.contains("● Active"));
    }

    #[test]
    fn test_build_available_provider_items() {
        let mut config = Config::default();
        config.provider.default = "groq".to_string();

        let (items, initial_index) = SetupWizard::build_available_provider_items(&config);
        // 10 catalog items + 1 back button
        assert_eq!(items.len(), 11);

        // Find groq
        let groq_idx = items
            .iter()
            .position(|it| it.id == "groq")
            .expect("groq present");
        assert_eq!(initial_index, groq_idx);
        assert!(items[groq_idx]
            .badge
            .as_deref()
            .unwrap_or("")
            .contains("● Active"));

        // Back button is last
        assert_eq!(items.last().expect("last item").id, "back");
    }

    #[test]
    fn test_apply_custom_provider() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let mut config = Config::default();

        let res = SetupWizard::apply_custom_provider(
            &mut config,
            temp_dir.path(),
            "local-vllm",
            "http://localhost:8000/v1",
            Some("sk-test-123456"),
        );
        assert!(res.is_ok());

        assert_eq!(config.provider.default, "local-vllm");
        assert_eq!(
            config
                .provider
                .custom_endpoints
                .get("local-vllm")
                .map(String::as_str),
            Some("http://localhost:8000/v1")
        );
        assert_eq!(
            config
                .provider
                .api_keys
                .get("local-vllm")
                .map(String::as_str),
            Some("sk-test-123456")
        );
    }
}

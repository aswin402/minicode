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
    /// Returns the static catalog of all canonical LLM providers matching `/settings` (`SUPPORTED_PROVIDERS`).
    ///
    /// Each entry contains: `(id, display_label, hint)`.
    pub fn provider_catalog() -> &'static [(&'static str, &'static str, &'static str)] {
        &[
            ("anthropic", "Anthropic", "Claude Opus 4, Sonnet 4 & Haiku"),
            (
                "openrouter",
                "OpenRouter",
                "100+ models: Claude, DeepSeek, Qwen",
            ),
            ("gemini", "Google Gemini", "Gemini 2.5 Pro & Flash"),
            ("openai", "OpenAI", "GPT-4o, o3, GPT-5"),
            ("deepseek", "DeepSeek", "DeepSeek-V3, R1 — ultra-low cost"),
            ("groq", "Groq", "Llama 3.3, Qwen — fast inference"),
            ("z.ai", "Z.ai / Zhipu GLM", "GLM-4.7, GLM-4.5, GLM-4-Flash"),
            ("kimi", "Kimi / Moonshot", "Kimi K2, Moonshot-v1"),
            (
                "huggingface",
                "Hugging Face",
                "Inference Providers — Qwen, Llama, DeepSeek",
            ),
            (
                "nvidia",
                "NVIDIA NIM",
                "NIM cloud endpoints — Llama, DeepSeek, Nemotron",
            ),
            ("opencode", "OpenCode Zen", "Curated coding models gateway"),
            ("cerebras", "Cerebras", "Ultra-fast wafer-scale inference"),
            ("agnes", "Agnes AI", "Agnes cloud gateway"),
            ("nara", "Nara Router", "Multi-model smart router"),
            ("ofox", "Ofox AI", "Ofox OpenAI-compatible gateway"),
            ("aihubmix", "AIHubMix", "Unified multi-provider API hub"),
            ("minimax", "MiniMax", "MiniMax-M3.1-Flash, MiniMax-M2.7"),
            ("together", "Together AI", "Open-source model hosting"),
            (
                "mistral",
                "Mistral AI",
                "Codestral, Devstral, Mistral Large",
            ),
            (
                "ollama",
                "Ollama (Local)",
                "100% Free local at localhost:11434",
            ),
            (
                "lmstudio",
                "LM Studio (Local)",
                "Local OpenAI server at localhost:1234",
            ),
            (
                "vllm",
                "vLLM (Local)",
                "Local vLLM server at localhost:8000",
            ),
            (
                "localhost",
                "Localhost (Custom)",
                "Local OpenAI-compatible server",
            ),
        ]
    }

    /// Maps standard provider IDs to their canonical environment variable names.
    pub fn env_var_for_provider(id: &str) -> &'static str {
        match id {
            "anthropic" | "claude" => "ANTHROPIC_API_KEY",
            "openrouter" => "OPENROUTER_API_KEY",
            "gemini" | "google" => "GEMINI_API_KEY",
            "openai" => "OPENAI_API_KEY",
            "deepseek" => "DEEPSEEK_API_KEY",
            "groq" => "GROQ_API_KEY",
            "minimax" => "MINIMAX_API_KEY",
            "z.ai" | "z_ai" | "zhipu" | "glm" | "bigmodel" => "ZHIPU_API_KEY",
            "together" => "TOGETHER_API_KEY",
            "mistral" => "MISTRAL_API_KEY",
            "huggingface" | "hf" => "HF_TOKEN",
            "nvidia" | "nim" | "nvidia-nim" => "NVIDIA_API_KEY",
            "opencode" | "opencode-zen" | "zen" => "OPENCODE_API_KEY",
            "cerebras" => "CEREBRAS_API_KEY",
            "agnes" | "agnes-ai" => "AGNES_API_KEY",
            "nara" | "nararouter" | "bynara" => "NARA_API_KEY",
            "ofox" | "ofoxai" => "OFOX_API_KEY",
            "aihubmix" => "AIHUBMIX_API_KEY",
            "kimi" | "moonshot" | "moonshotai" => "KIMI_API_KEY",
            "ollama" => "OLLAMA_API_KEY",
            "lmstudio" => "LMSTUDIO_API_KEY",
            "vllm" => "VLLM_API_KEY",
            "localhost" => "LOCALHOST_API_KEY",
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
        let key_opt = match config.get_api_key(id) {
            Ok(k) if !k.trim().is_empty() => Some(k),
            _ => None,
        };
        let is_local = config.is_local_provider(id);
        if config.provider.default.eq_ignore_ascii_case(id) {
            if let Some(k) = key_opt {
                let masked = mask_api_key(&k);
                format!("\x1b[32m● Active\x1b[0m \x1b[36m({masked})\x1b[0m")
            } else if is_local {
                "\x1b[32m● Active (Local)\x1b[0m".to_string()
            } else {
                "\x1b[32m● Active\x1b[0m \x1b[33m(No Key)\x1b[0m".to_string()
            }
        } else if let Some(k) = key_opt {
            let masked = mask_api_key(&k);
            format!("\x1b[36m✔ Configured ({masked})\x1b[0m")
        } else if is_local {
            "\x1b[33m○ Localhost\x1b[0m".to_string()
        } else {
            "\x1b[90m○ Not Set\x1b[0m".to_string()
        }
    }

    /// Counts how many non-local providers currently have a configured API key.
    pub fn count_configured_cloud_providers(config: &Config) -> usize {
        let mut seen = std::collections::HashSet::new();
        let mut count = 0usize;
        for (id, _, _) in Self::provider_catalog() {
            if seen.insert((*id).to_string())
                && !config.is_local_provider(id)
                && config
                    .get_api_key(id)
                    .ok()
                    .is_some_and(|k| !k.trim().is_empty())
            {
                count += 1;
            }
        }
        for (custom_id, key) in &config.provider.api_keys {
            let norm = custom_id.to_lowercase();
            if seen.insert(norm.clone())
                && !config.is_local_provider(&norm)
                && !key.trim().is_empty()
            {
                count += 1;
            }
        }
        count
    }

    /// Loads global configuration for `minicode setup` (preferring global `~/.config/minicode/config.toml`
    /// & `.env`, while rescuing any legacy `[provider.api_keys]` / `[provider.custom_endpoints]`
    /// left behind in `<workspace>/.minicode/config.toml` so nothing added in `/settings` is missed).
    pub fn load_global_config_for_setup(workspace: &Path) -> Config {
        let mut global_cfg = Config::load(None, None).unwrap_or_default();
        if let Ok(ws_cfg) = Config::load(Some(workspace), None) {
            for (k, v) in ws_cfg.provider.api_keys {
                if !v.trim().is_empty() {
                    global_cfg.provider.api_keys.entry(k).or_insert(v);
                }
            }
            for (k, v) in ws_cfg.provider.custom_endpoints {
                if !v.trim().is_empty() {
                    global_cfg.provider.custom_endpoints.entry(k).or_insert(v);
                }
            }
        }
        global_cfg
    }

    /// Builds the list of selector items for the Available Providers menu, covering all canonical
    /// providers (matching `/settings`) plus any user-defined custom providers.
    pub fn build_available_provider_items(config: &Config) -> (Vec<SelectorItem>, usize) {
        let catalog = Self::provider_catalog();
        let mut items =
            Vec::with_capacity(catalog.len() + config.provider.custom_endpoints.len() + 1);
        let mut initial_index = 0;
        let mut seen = std::collections::HashSet::new();

        for (id, label, hint) in catalog.iter() {
            seen.insert((*id).to_string());
            let badge = Self::provider_badge(config, id);
            if config.provider.default.eq_ignore_ascii_case(id) {
                initial_index = items.len();
            }
            items.push(
                SelectorItem::new(*id, *label)
                    .with_badge(badge)
                    .with_hint(*hint),
            );
        }

        // Also surface any user-configured custom providers from `custom_endpoints` or `api_keys`
        let mut custom_ids: Vec<String> = config
            .provider
            .custom_endpoints
            .keys()
            .chain(config.provider.api_keys.keys())
            .map(|k| k.to_lowercase())
            .filter(|k| !seen.contains(k))
            .collect();
        custom_ids.sort();
        custom_ids.dedup();
        for custom_id in custom_ids {
            let badge = Self::provider_badge(config, &custom_id);
            let hint = config
                .provider
                .custom_endpoints
                .get(&custom_id)
                .cloned()
                .unwrap_or_else(|| "Custom OpenAI-compatible provider".to_string());
            if config.provider.default.eq_ignore_ascii_case(&custom_id) {
                initial_index = items.len();
            }
            items.push(
                SelectorItem::new(custom_id.clone(), format!("{custom_id} (Custom)"))
                    .with_badge(badge)
                    .with_hint(hint),
            );
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

    /// Activates `id` as the default provider and synchronizes `config.provider.model` when
    /// switching away from a different provider so the active model always matches the provider.
    fn activate_provider(config: &mut Config, id: &str) {
        let switching = !config.provider.default.eq_ignore_ascii_case(id);
        config.provider.default = id.to_string();
        if switching || config.provider.model.trim().is_empty() {
            config.provider.model = config.get_default_model_for_provider(id);
        }
    }

    /// Handles selection of a provider from the Available Providers catalog (built-in or custom).
    pub fn handle_available_provider_selection(
        config: &mut Config,
        workspace: &Path,
        id: &str,
        label: &str,
    ) -> Result<()> {
        if config.is_local_provider(id) {
            Self::activate_provider(config, id);
            ConfigMenu::save_all(config, workspace)?;
            Self::print_receipt(
                &config.provider.default,
                &config.provider.model,
                &format!("Local '{}' provider activated!", label),
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
                    Self::activate_provider(config, id);
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
                        Self::activate_provider(config, id);
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
                            Self::activate_provider(config, id);
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
        Self::apply_custom_provider_with_dirs(config, workspace, name, base_url, api_key, None)
    }

    /// Applies custom provider settings to config, sets as active, and saves to specified config directory.
    pub fn apply_custom_provider_with_dirs(
        config: &mut Config,
        workspace: &Path,
        name: &str,
        base_url: &str,
        api_key: Option<&str>,
        custom_config_dir: Option<&Path>,
    ) -> Result<()> {
        config
            .provider
            .custom_endpoints
            .insert(name.to_string(), base_url.to_string());

        if let Some(key) = api_key {
            let env_var = Self::custom_env_var(name);
            std::env::set_var(&env_var, key);
            config
                .provider
                .api_keys
                .insert(name.to_string(), key.to_string());
        }

        config.provider.default = name.to_string();
        ConfigMenu::save_all_with_dirs(config, workspace, custom_config_dir)?;

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
            let configured_count = Self::count_configured_cloud_providers(config);

            let prompt = format!(
                "=== Available Providers ({} total · {} API keys configured) ===\n\x1b[90mActive Provider: \x1b[36m{}\x1b[90m | Active Model: \x1b[33m{}\x1b[0m",
                items.len().saturating_sub(1),
                configured_count,
                config.provider.default,
                config.provider.model
            );

            let selector = InteractiveSelector::new();
            let choice = selector.select(&prompt, &items, initial_index)?;

            match choice {
                Some(idx) => {
                    let Some(selected) = items.get(idx) else {
                        break;
                    };
                    if selected.id == "back" {
                        break;
                    }
                    let id = selected.id.clone();
                    let label = selected.label.clone();
                    Self::handle_available_provider_selection(config, workspace, &id, &label)?;
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
            let configured_count = Self::count_configured_cloud_providers(config);
            let prompt = format!(
                "=== Provider Configuration ({} API keys configured) ===\n\x1b[90mActive Provider: \x1b[36m{}\x1b[90m | Active Model: \x1b[33m{}\x1b[0m",
                configured_count, config.provider.default, config.provider.model
            );
            let items = vec![
                SelectorItem::new("available", "🌐 Available Providers")
                    .with_hint("All 23 providers: Anthropic, OpenRouter, Gemini, OpenAI, MiniMax, Kimi, NVIDIA, Groq, etc."),
                SelectorItem::new("custom", "🔌 Custom Provider")
                    .with_hint("OpenAI-compatible endpoints (vLLM, LM Studio, Ollama, proxies)"),
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

        let mut config = Self::load_global_config_for_setup(workspace);

        loop {
            let configured_count = Self::count_configured_cloud_providers(&config);
            let prompt = format!(
                "⚡ minicode — Interactive Setup Wizard ({} API keys configured)\n\x1b[90mActive Provider: \x1b[36m{}\x1b[90m | Active Model: \x1b[33m{}\x1b[0m",
                configured_count, config.provider.default, config.provider.model
            );
            let items = vec![
                SelectorItem::new("provider", "⚡ Provider")
                    .with_hint("Setup & manage AI providers and API keys"),
                SelectorItem::new("back", "◄ Back / Exit")
                    .with_hint("Save changes to global config and return to shell"),
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
        // Exact 1:1 parity with /settings (`SUPPORTED_PROVIDERS`)
        assert_eq!(catalog.len(), crate::constants::SUPPORTED_PROVIDERS.len());
        for &supported in crate::constants::SUPPORTED_PROVIDERS {
            assert!(
                ids.contains(&supported),
                "Provider '{supported}' from SUPPORTED_PROVIDERS missing in SetupWizard::provider_catalog"
            );
        }
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
        assert_eq!(
            SetupWizard::env_var_for_provider("anthropic"),
            "ANTHROPIC_API_KEY"
        );
        assert_eq!(
            SetupWizard::env_var_for_provider("lmstudio"),
            "LMSTUDIO_API_KEY"
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
        std::env::remove_var("MINIMAX_API_KEY");
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

        // 4. Local providers (ollama, lmstudio)
        let badge_ollama = SetupWizard::provider_badge(&config, "ollama");
        assert!(badge_ollama.contains("○ Localhost"));
        let badge_lmstudio = SetupWizard::provider_badge(&config, "lmstudio");
        assert!(badge_lmstudio.contains("○ Localhost"));

        // 5. Active local provider
        config.provider.default = "ollama".to_string();
        let badge_ollama_active = SetupWizard::provider_badge(&config, "ollama");
        assert!(badge_ollama_active.contains("● Active (Local)"));
    }

    #[test]
    fn test_build_available_provider_items() {
        let mut config = Config::default();
        config.provider.default = "groq".to_string();
        config.provider.custom_endpoints.insert(
            "my-gateway".to_string(),
            "https://gw.example.com/v1".to_string(),
        );
        config
            .provider
            .api_keys
            .insert("my-gateway".to_string(), "sk-custom-987654321".to_string());

        let (items, initial_index) = SetupWizard::build_available_provider_items(&config);
        // All 23 canonical providers + 1 custom provider + 1 back button
        assert_eq!(items.len(), crate::constants::SUPPORTED_PROVIDERS.len() + 2);

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

        // Custom provider surfaced with configured badge
        let custom = items
            .iter()
            .find(|it| it.id == "my-gateway")
            .expect("custom provider present");
        assert!(custom
            .badge
            .as_deref()
            .unwrap_or("")
            .contains("✔ Configured"));

        // Back button is last
        assert_eq!(items.last().expect("last item").id, "back");
    }

    #[test]
    fn test_apply_custom_provider() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let mut config = Config::default();

        let res = SetupWizard::apply_custom_provider_with_dirs(
            &mut config,
            temp_dir.path(),
            "local-vllm",
            "http://localhost:8000/v1",
            Some("sk-test-123456"),
            Some(temp_dir.path()),
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

    #[test]
    fn test_load_global_config_for_setup_rescues_legacy_workspace_keys_and_activates_model() {
        let dir = tempfile::tempdir().expect("tempdir");
        let ws = dir.path();
        let ws_cfg_dir = ws.join(".minicode");
        std::fs::create_dir_all(&ws_cfg_dir).expect("mkdir .minicode");
        std::fs::write(
            ws_cfg_dir.join("config.toml"),
            "[provider]\ndefault = \"minimax\"\nmodel = \"MiniMax-M3.1-Flash\"\n\n[provider.api_keys]\nminimax = \"sk-legacy-mm-key\"\nnvidia = \"nvapi-legacy-key\"\n\n[provider.custom_endpoints]\nmyproxy = \"https://proxy.example.com/v1\"\n",
        )
        .expect("write legacy workspace config");

        let mut loaded = SetupWizard::load_global_config_for_setup(ws);
        assert_eq!(
            loaded.provider.api_keys.get("minimax").map(String::as_str),
            Some("sk-legacy-mm-key")
        );
        assert_eq!(
            loaded.provider.api_keys.get("nvidia").map(String::as_str),
            Some("nvapi-legacy-key")
        );
        assert_eq!(
            loaded
                .provider
                .custom_endpoints
                .get("myproxy")
                .map(String::as_str),
            Some("https://proxy.example.com/v1")
        );

        // Switching providers via activate_provider must also sync config.provider.model
        loaded.provider.default = "openrouter".to_string();
        loaded.provider.model = "anthropic/claude-sonnet-4".to_string();
        SetupWizard::activate_provider(&mut loaded, "minimax");
        assert_eq!(loaded.provider.default, "minimax");
        assert_eq!(
            loaded.provider.model,
            loaded.get_default_model_for_provider("minimax")
        );
    }
}

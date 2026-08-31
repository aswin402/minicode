use crate::agent::models::ModelFetcher;
use crate::config::Config;
use crate::error::Result;
use std::io::{self, BufRead, Write};
use std::path::Path;

pub struct ConfigMenu;

impl ConfigMenu {
    pub async fn run_interactive(workspace: &Path) -> Result<()> {
        let stdout = io::stdout();
        let mut handle = stdout.lock();
        let stdin = io::stdin();
        let mut reader = stdin.lock();

        let fetcher = ModelFetcher::new();
        let mut config = Config::load(Some(workspace), None).unwrap_or_default();

        loop {
            writeln!(
                handle,
                "\n\x1b[1;35m⚡ minicode — Interactive Configuration Wizard\x1b[0m"
            )?;
            writeln!(
                handle,
                "\x1b[90mActive Provider: \x1b[36m{}\x1b[90m | Active Model: \x1b[33m{}\x1b[90m | Policy: \x1b[32m{}\x1b[0m",
                config.provider.default, config.provider.model, config.agent.approval_policy
            )?;
            writeln!(
                handle,
                "\x1b[90m─────────────────────────────────────────────────────────\x1b[0m"
            )?;
            writeln!(
                handle,
                "  \x1b[1m[1]\x1b[0m ⚡ Setup / Switch Active Provider"
            )?;
            writeln!(
                handle,
                "  \x1b[1m[2]\x1b[0m 🔑 Manage / Reconfigure API Keys"
            )?;
            writeln!(
                handle,
                "  \x1b[1m[3]\x1b[0m 🔌 Add Custom OpenAI-Compatible Provider"
            )?;
            writeln!(
                handle,
                "  \x1b[1m[4]\x1b[0m 🤖 Select Model (Live Fetch from Provider)"
            )?;
            writeln!(handle, "  \x1b[1m[5]\x1b[0m 🛡️ Toggle Tool Approval Policy")?;
            writeln!(
                handle,
                "  \x1b[1m[6]\x1b[0m 💰 Toggle Cost Display in Status Bar (currently: \x1b[36m{}\x1b[0m)",
                if config.ui.show_cost { "ENABLED" } else { "DISABLED" }
            )?;
            writeln!(handle, "  \x1b[1m[0]\x1b[0m 💾 Save & Exit")?;
            write!(handle, "\n  Enter choice [0-6]: ")?;
            handle.flush()?;

            let mut input = String::new();
            if reader.read_line(&mut input)? == 0 {
                break;
            }
            let choice = input.trim();

            match choice {
                "1" => {
                    Self::submenu_setup_provider(
                        &mut handle,
                        &mut reader,
                        &fetcher,
                        &mut config,
                        workspace,
                    )
                    .await?;
                }
                "2" => {
                    Self::submenu_manage_keys(
                        &mut handle,
                        &mut reader,
                        &fetcher,
                        &mut config,
                        workspace,
                    )
                    .await?;
                }
                "3" => {
                    Self::submenu_add_custom_provider(
                        &mut handle,
                        &mut reader,
                        &fetcher,
                        &mut config,
                        workspace,
                    )
                    .await?;
                }
                "4" => {
                    Self::submenu_select_model(&mut handle, &mut reader, &fetcher, &mut config)
                        .await?;
                }
                "5" => {
                    Self::submenu_toggle_policy(&mut handle, &mut reader, &mut config)?;
                }
                "6" => {
                    config.ui.show_cost = !config.ui.show_cost;
                    writeln!(
                        handle,
                        "\x1b[32m✔ Cost display in status bar set to: {}\x1b[0m",
                        if config.ui.show_cost {
                            "ENABLED"
                        } else {
                            "DISABLED"
                        }
                    )?;
                }
                "0" | "exit" | "quit" | "q" => {
                    Self::save_all(&config, workspace)?;
                    writeln!(
                        handle,
                        "\n\x1b[32m✔ Configuration saved successfully to ~/.config/minicode/config.toml and .env\x1b[0m"
                    )?;
                    break;
                }
                _ => {
                    writeln!(handle, "\x1b[31mInvalid option. Please choose 0-6.\x1b[0m")?;
                }
            }
        }

        Ok(())
    }

    async fn submenu_setup_provider<W: Write, R: BufRead>(
        handle: &mut W,
        reader: &mut R,
        fetcher: &ModelFetcher,
        config: &mut Config,
        workspace: &Path,
    ) -> Result<()> {
        loop {
            writeln!(handle, "\n\x1b[1;34m=== Select LLM Provider ===\x1b[0m")?;
            writeln!(
                handle,
                "  \x1b[1m[ 1]\x1b[0m ⚡ OpenRouter (100+ models: Claude 3.7, DeepSeek-R1, Qwen 2.5, Free tier)"
            )?;
            writeln!(
                handle,
                "  \x1b[1m[ 2]\x1b[0m 🧠 Google Gemini (Gemini 2.5 Pro, Flash — Free tier on AI Studio)"
            )?;
            writeln!(
                handle,
                "  \x1b[1m[ 3]\x1b[0m 🟢 OpenAI (GPT-4o, o3-mini, o1)"
            )?;
            writeln!(
                handle,
                "  \x1b[1m[ 4]\x1b[0m 🐋 DeepSeek (DeepSeek-V3, DeepSeek-R1 — Ultra-low cost API)"
            )?;
            writeln!(
                handle,
                "  \x1b[1m[ 5]\x1b[0m ⚡ Groq (Llama 3.3 70B, Qwen 2.5 Coder 32B — Free tier & ultra-fast)"
            )?;
            writeln!(
                handle,
                "  \x1b[1m[ 6]\x1b[0m 🚀 MiniMax (MiniMax-Text-01, abab6.5s)"
            )?;
            writeln!(
                handle,
                "  \x1b[1m[ 7]\x1b[0m 🌟 Z.ai / Zhipu GLM (GLM-4-Plus, GLM-4-Flash — Free tier)"
            )?;
            writeln!(
                handle,
                "  \x1b[1m[ 8]\x1b[0m 🤝 Together AI (Llama 3.3, DeepSeek, Qwen)"
            )?;
            writeln!(
                handle,
                "  \x1b[1m[ 9]\x1b[0m 💨 Mistral AI (Codestral, Mistral Large)"
            )?;
            writeln!(
                handle,
                "  \x1b[1m[10]\x1b[0m 🦙 Ollama (100% Free & Local at http://localhost:11434/v1)"
            )?;
            writeln!(
                handle,
                "  \x1b[1m[11]\x1b[0m 🔌 + Add Custom OpenAI-Compatible Provider..."
            )?;
            writeln!(handle, "  \x1b[1m[ 0]\x1b[0m ◄ Back to Main Menu")?;
            write!(handle, "\n  Select provider [0-11]: ")?;
            handle.flush()?;

            let mut input = String::new();
            if reader.read_line(&mut input)? == 0 {
                break;
            }
            let choice = input.trim();

            let (provider_name, env_var) = match choice {
                "1" => ("openrouter", "OPENROUTER_API_KEY"),
                "2" => ("gemini", "GEMINI_API_KEY"),
                "3" => ("openai", "OPENAI_API_KEY"),
                "4" => ("deepseek", "DEEPSEEK_API_KEY"),
                "5" => ("groq", "GROQ_API_KEY"),
                "6" => ("minimax", "MINIMAX_API_KEY"),
                "7" => ("z.ai", "ZHIPU_API_KEY"),
                "8" => ("together", "TOGETHER_API_KEY"),
                "9" => ("mistral", "MISTRAL_API_KEY"),
                "10" => ("ollama", "OLLAMA_API_KEY"),
                "11" => {
                    return Self::submenu_add_custom_provider(
                        handle, reader, fetcher, config, workspace,
                    )
                    .await;
                }
                "0" | "b" | "back" | "q" => break,
                _ => {
                    writeln!(handle, "\x1b[31mInvalid option. Please choose 0-11.\x1b[0m")?;
                    continue;
                }
            };

            config.provider.default = provider_name.to_string();

            if provider_name == "ollama" {
                writeln!(
                    handle,
                    "\x1b[32m✔ Selected local Ollama provider (no API key required).\x1b[0m"
                )?;
                write!(
                    handle,
                    "  Fetch available local models from Ollama now? [Y/n/0 to back]: "
                )?;
                handle.flush()?;
                input.clear();
                reader.read_line(&mut input)?;
                let fetch_choice = input.trim();
                if fetch_choice != "0"
                    && fetch_choice != "b"
                    && !fetch_choice.eq_ignore_ascii_case("n")
                {
                    Self::fetch_and_select_model(
                        handle, reader, fetcher, config, "ollama", "", None,
                    )
                    .await?;
                }
                Self::save_all(config, workspace)?;
                break;
            }

            // Setup API key
            let current_key = config.get_api_key(provider_name).unwrap_or_default();
            let key_display = if !current_key.is_empty() {
                let chars: Vec<char> = current_key.chars().collect();
                if chars.len() <= 10 {
                    "********".to_string()
                } else {
                    let prefix: String = chars.iter().take(6).collect();
                    let suffix: String = chars.iter().rev().take(4).rev().collect();
                    format!("{}...{}", prefix, suffix)
                }
            } else {
                "None configured".to_string()
            };

            writeln!(
                handle,
                "\n\x1b[1mConfigure API Key for {}\x1b[0m (Current: \x1b[90m{}\x1b[0m)",
                provider_name, key_display
            )?;
            write!(
                handle,
                "  Paste API key for {} (press Enter to keep current, 'del' to clear, '0' to back): ",
                provider_name
            )?;
            handle.flush()?;

            input.clear();
            reader.read_line(&mut input)?;
            let key_input = input.trim();

            let active_key = if key_input == "0" || key_input == "b" {
                continue;
            } else if key_input.eq_ignore_ascii_case("del") {
                config.provider.api_keys.remove(provider_name);
                std::env::remove_var(env_var);
                writeln!(handle, "\x1b[33mKey cleared for {}.\x1b[0m", provider_name)?;
                String::new()
            } else if !key_input.is_empty() {
                config
                    .provider
                    .api_keys
                    .insert(provider_name.to_string(), key_input.to_string());
                std::env::set_var(env_var, key_input);
                key_input.to_string()
            } else {
                current_key
            };

            // Prompt to select model dynamically
            write!(
                handle,
                "\n  Fetch available models from {} now? [Y/n/0 to back]: ",
                provider_name
            )?;
            handle.flush()?;
            input.clear();
            reader.read_line(&mut input)?;
            let fetch_choice = input.trim();

            if fetch_choice != "0" && fetch_choice != "b" && !fetch_choice.eq_ignore_ascii_case("n")
            {
                Self::fetch_and_select_model(
                    handle,
                    reader,
                    fetcher,
                    config,
                    provider_name,
                    &active_key,
                    None,
                )
                .await?;
            }

            Self::save_all(config, workspace)?;
            writeln!(
                handle,
                "\x1b[32m✔ Active provider set to '{}' with model '{}'\x1b[0m",
                config.provider.default, config.provider.model
            )?;
            break;
        }
        Ok(())
    }

    async fn submenu_manage_keys<W: Write, R: BufRead>(
        handle: &mut W,
        reader: &mut R,
        _fetcher: &ModelFetcher,
        config: &mut Config,
        workspace: &Path,
    ) -> Result<()> {
        let providers = [
            ("openrouter", "OpenRouter", "OPENROUTER_API_KEY"),
            ("gemini", "Google Gemini", "GEMINI_API_KEY"),
            ("openai", "OpenAI", "OPENAI_API_KEY"),
            ("deepseek", "DeepSeek", "DEEPSEEK_API_KEY"),
            ("groq", "Groq", "GROQ_API_KEY"),
            ("minimax", "MiniMax", "MINIMAX_API_KEY"),
            ("z.ai", "Z.ai / Zhipu GLM", "ZHIPU_API_KEY"),
            ("together", "Together AI", "TOGETHER_API_KEY"),
            ("mistral", "Mistral AI", "MISTRAL_API_KEY"),
        ];

        loop {
            writeln!(
                handle,
                "\n\x1b[1;36m=== Manage & Reconfigure API Keys ===\x1b[0m"
            )?;
            for (idx, (p_id, label, _env)) in providers.iter().enumerate() {
                let current_key = config.get_api_key(p_id).unwrap_or_default();
                let status = if !current_key.is_empty() {
                    let chars: Vec<char> = current_key.chars().collect();
                    if chars.len() <= 8 {
                        "\x1b[32m[CONFIGURED]\x1b[0m".to_string()
                    } else {
                        let prefix: String = chars.iter().take(4).collect();
                        let suffix: String = chars.iter().rev().take(3).rev().collect();
                        format!("\x1b[32m[CONFIGURED: {}...{}]\x1b[0m", prefix, suffix)
                    }
                } else {
                    "\x1b[90m[NOT SET]\x1b[0m".to_string()
                };

                let active_marker = if config.provider.default == *p_id {
                    " \x1b[33m(Active)\x1b[0m"
                } else {
                    ""
                };

                writeln!(
                    handle,
                    "  \x1b[1m[{:2}]\x1b[0m {:<18} {}{}",
                    idx + 1,
                    label,
                    status,
                    active_marker
                )?;
            }
            writeln!(handle, "  \x1b[1m[ 0]\x1b[0m ◄ Back to Main Menu")?;
            write!(handle, "\n  Select provider to update key [1-9, 0]: ")?;
            handle.flush()?;

            let mut input = String::new();
            if reader.read_line(&mut input)? == 0 {
                break;
            }
            let choice = input.trim();

            if choice == "0" || choice == "b" || choice == "back" || choice == "q" {
                break;
            }

            if let Ok(num) = choice.parse::<usize>() {
                if num >= 1 && num <= providers.len() {
                    let (p_id, label, env_var) = providers[num - 1];
                    write!(
                        handle,
                        "\n  Paste API key for {} (press Enter to keep, 'del' to clear, '0' to back): ",
                        label
                    )?;
                    handle.flush()?;

                    input.clear();
                    reader.read_line(&mut input)?;
                    let key_input = input.trim();

                    if key_input == "0" || key_input == "b" {
                        continue;
                    } else if key_input.eq_ignore_ascii_case("del") {
                        config.provider.api_keys.remove(p_id);
                        std::env::remove_var(env_var);
                        Self::save_all(config, workspace)?;
                        writeln!(handle, "\x1b[33m✔ Cleared key for {}.\x1b[0m", label)?;
                    } else if !key_input.is_empty() {
                        config
                            .provider
                            .api_keys
                            .insert(p_id.to_string(), key_input.to_string());
                        std::env::set_var(env_var, key_input);
                        Self::save_all(config, workspace)?;
                        writeln!(handle, "\x1b[32m✔ Saved key for {}.\x1b[0m", label)?;
                    }
                }
            }
        }
        Ok(())
    }

    async fn submenu_add_custom_provider<W: Write, R: BufRead>(
        handle: &mut W,
        reader: &mut R,
        fetcher: &ModelFetcher,
        config: &mut Config,
        workspace: &Path,
    ) -> Result<()> {
        writeln!(
            handle,
            "\n\x1b[1;35m=== Add Custom OpenAI-Compatible Provider ===\x1b[0m"
        )?;
        writeln!(
            handle,
            "\x1b[90m(Supports vLLM, LM Studio, Ollama, LocalAI, Azure, OpenCode, etc.)\x1b[0m"
        )?;

        write!(
            handle,
            "  1. Provider Identifier Name (e.g. 'local-vllm', '0' to back): "
        )?;
        handle.flush()?;
        let mut input = String::new();
        reader.read_line(&mut input)?;
        let name = input.trim().to_lowercase();
        if name.is_empty() || name == "0" || name == "b" {
            return Ok(());
        }

        write!(
            handle,
            "  2. OpenAI-Compatible Base URL (e.g. 'http://localhost:8000/v1'): "
        )?;
        handle.flush()?;
        input.clear();
        reader.read_line(&mut input)?;
        let base_url = input.trim().to_string();
        if base_url.is_empty() {
            writeln!(handle, "\x1b[31mBase URL cannot be empty.\x1b[0m")?;
            return Ok(());
        }

        write!(
            handle,
            "  3. API Key (optional, press Enter if not required): "
        )?;
        handle.flush()?;
        input.clear();
        reader.read_line(&mut input)?;
        let api_key = input.trim().to_string();

        let env_var = format!("{}_API_KEY", name.to_uppercase().replace('-', "_"));
        if !api_key.is_empty() {
            config
                .provider
                .api_keys
                .insert(name.clone(), api_key.clone());
            std::env::set_var(&env_var, &api_key);
        }

        config
            .provider
            .custom_endpoints
            .insert(name.clone(), base_url.clone());

        // Set as active provider
        config.provider.default = name.clone();

        writeln!(
            handle,
            "\x1b[90mTesting connection & fetching available models from {}...\x1b[0m",
            base_url
        )?;
        Self::fetch_and_select_model(
            handle,
            reader,
            fetcher,
            config,
            &name,
            &api_key,
            Some(&base_url),
        )
        .await?;

        Self::save_all(config, workspace)?;

        writeln!(
            handle,
            "\x1b[32m✔ Custom provider '{}' configured and activated successfully!\x1b[0m",
            name
        )?;
        Ok(())
    }

    async fn submenu_select_model<W: Write, R: BufRead>(
        handle: &mut W,
        reader: &mut R,
        fetcher: &ModelFetcher,
        config: &mut Config,
    ) -> Result<()> {
        let provider = config.provider.default.clone();
        let api_key = config.get_api_key(&provider).unwrap_or_default();
        let custom_url = config.provider.custom_endpoints.get(&provider).cloned();

        Self::fetch_and_select_model(
            handle,
            reader,
            fetcher,
            config,
            &provider,
            &api_key,
            custom_url.as_deref(),
        )
        .await
    }

    async fn fetch_and_select_model<W: Write, R: BufRead>(
        handle: &mut W,
        reader: &mut R,
        fetcher: &ModelFetcher,
        config: &mut Config,
        provider: &str,
        api_key: &str,
        base_url: Option<&str>,
    ) -> Result<()> {
        writeln!(
            handle,
            "\n\x1b[90mFetching live models list from {} API...\x1b[0m",
            provider
        )?;

        let models_res = fetcher.fetch_models(provider, api_key, base_url).await;

        let models = match models_res {
            Ok(m) if !m.is_empty() => m,
            Ok(_) => {
                writeln!(
                    handle,
                    "\x1b[33mNo models returned from API. Please enter model ID manually.\x1b[0m"
                )?;
                return Self::manual_model_input(handle, reader, config);
            }
            Err(e) => {
                writeln!(
                    handle,
                    "\x1b[33mCould not fetch live models ({}). You can enter model ID manually.\x1b[0m",
                    e
                )?;
                return Self::manual_model_input(handle, reader, config);
            }
        };

        writeln!(
            handle,
            "\n\x1b[1;36mAvailable Models for {}:\x1b[0m",
            provider
        )?;

        // Show top 25 models
        let display_count = models.len().min(25);
        for (i, m) in models.iter().take(display_count).enumerate() {
            let free_badge = if m.is_free {
                " \x1b[1;32m[FREE]\x1b[0m"
            } else {
                ""
            };
            let ctx_badge = m
                .context_length
                .map(|c| format!(" \x1b[90m({}k ctx)\x1b[0m", c / 1000))
                .unwrap_or_default();
            writeln!(
                handle,
                "  \x1b[1m[{:2}]\x1b[0m {}{}{}",
                i + 1,
                m.id,
                free_badge,
                ctx_badge
            )?;
        }

        if models.len() > display_count {
            writeln!(
                handle,
                "  \x1b[90m... and {} more models\x1b[0m",
                models.len() - display_count
            )?;
        }

        writeln!(
            handle,
            "  \x1b[1m[ m]\x1b[0m Enter custom/other model ID manually"
        )?;
        writeln!(
            handle,
            "  \x1b[1m[ 0]\x1b[0m ◄ Keep current ({})",
            config.provider.model
        )?;

        write!(handle, "\n  Select model [1-{}, m, or 0]: ", display_count)?;
        handle.flush()?;

        let mut input = String::new();
        reader.read_line(&mut input)?;
        let choice = input.trim();

        if choice == "0" || choice == "b" || choice == "back" {
            return Ok(());
        }

        if choice.eq_ignore_ascii_case("m") {
            return Self::manual_model_input(handle, reader, config);
        }

        if let Ok(num) = choice.parse::<usize>() {
            if num >= 1 && num <= display_count {
                config.provider.model = models[num - 1].id.clone();
                writeln!(
                    handle,
                    "\x1b[32m✔ Selected model: {}\x1b[0m",
                    config.provider.model
                )?;
                return Ok(());
            }
        }

        // If user directly typed model ID
        if !choice.is_empty() {
            config.provider.model = choice.to_string();
            writeln!(
                handle,
                "\x1b[32m✔ Set model: {}\x1b[0m",
                config.provider.model
            )?;
        }

        Ok(())
    }

    fn manual_model_input<W: Write, R: BufRead>(
        handle: &mut W,
        reader: &mut R,
        config: &mut Config,
    ) -> Result<()> {
        write!(
            handle,
            "  Enter model ID (current: '{}', '0' to back): ",
            config.provider.model
        )?;
        handle.flush()?;
        let mut input = String::new();
        reader.read_line(&mut input)?;
        let model_input = input.trim();
        if !model_input.is_empty() && model_input != "0" && model_input != "b" {
            config.provider.model = model_input.to_string();
            writeln!(
                handle,
                "\x1b[32m✔ Model updated to: {}\x1b[0m",
                config.provider.model
            )?;
        }
        Ok(())
    }

    fn submenu_toggle_policy<W: Write, R: BufRead>(
        handle: &mut W,
        reader: &mut R,
        config: &mut Config,
    ) -> Result<()> {
        writeln!(handle, "\n\x1b[1;33m=== Tool Approval Policy ===\x1b[0m")?;
        writeln!(
            handle,
            "  [1] Strict (Always ask user confirmation before file writes and shell execution)"
        )?;
        writeln!(
            handle,
            "  [2] Auto-Approve (Automatically execute tools inside Landlock sandbox)"
        )?;
        writeln!(
            handle,
            "  [0] ◄ Back (Current: {})",
            config.agent.approval_policy
        )?;
        write!(handle, "\n  Select policy [1-2, 0]: ")?;
        handle.flush()?;

        let mut input = String::new();
        reader.read_line(&mut input)?;
        let choice = input.trim();

        match choice {
            "1" => {
                config.agent.approval_policy = "strict".to_string();
                writeln!(handle, "\x1b[32m✔ Approval policy set to 'strict'\x1b[0m")?;
            }
            "2" => {
                config.agent.approval_policy = "auto-approve".to_string();
                writeln!(
                    handle,
                    "\x1b[32m✔ Approval policy set to 'auto-approve'\x1b[0m"
                )?;
            }
            _ => {}
        }
        Ok(())
    }

    pub fn save_all(config: &Config, workspace: &Path) -> Result<()> {
        // 1. Save to global ~/.config/minicode/config.toml
        if let Some(config_dir) = dirs::config_dir() {
            let app_dir = config_dir.join(crate::constants::CONFIG_DIR_NAME);
            let _ = std::fs::create_dir_all(&app_dir);
            let toml_path = app_dir.join(crate::constants::CONFIG_FILE_NAME);
            let content =
                toml::to_string_pretty(config).map_err(crate::error::ConfigError::TomlSerialize)?;
            let _ = std::fs::write(&toml_path, content);

            // Also write all configured API keys to global ~/.config/minicode/.env
            let global_env_path = app_dir.join(crate::constants::ENV_FILE_NAME);
            for (p, k) in &config.provider.api_keys {
                let env_name = match p.as_str() {
                    "openrouter" => "OPENROUTER_API_KEY",
                    "gemini" | "google" => "GEMINI_API_KEY",
                    "openai" => "OPENAI_API_KEY",
                    "deepseek" => "DEEPSEEK_API_KEY",
                    "groq" => "GROQ_API_KEY",
                    "minimax" => "MINIMAX_API_KEY",
                    "z.ai" | "z_ai" | "zhipu" | "glm" | "bigmodel" => "ZHIPU_API_KEY",
                    "together" => "TOGETHER_API_KEY",
                    "mistral" => "MISTRAL_API_KEY",
                    _ => "",
                };
                let env_name_str = if env_name.is_empty() {
                    format!("{}_API_KEY", p.to_uppercase().replace(['-', '.'], "_"))
                } else {
                    env_name.to_string()
                };
                let _ = update_dotenv_file(&global_env_path, &env_name_str, k);
            }
            let _ = update_dotenv_file(
                &global_env_path,
                "MINICODE_PROVIDER",
                &config.provider.default,
            );
            let _ = update_dotenv_file(&global_env_path, "MINICODE_MODEL", &config.provider.model);
        }

        // 2. Also update workspace .env if workspace exists
        let env_path = workspace.join(crate::constants::ENV_FILE_NAME);
        let _ = update_dotenv_file(&env_path, "MINICODE_PROVIDER", &config.provider.default);
        let _ = update_dotenv_file(&env_path, "MINICODE_MODEL", &config.provider.model);
        let _ = update_dotenv_file(
            &env_path,
            "MINICODE_APPROVAL_POLICY",
            &config.agent.approval_policy,
        );

        Ok(())
    }
}

pub fn update_dotenv_file(env_path: &Path, key: &str, value: &str) -> Result<()> {
    if let Some(parent) = env_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let mut lines = Vec::new();
    let mut found = false;

    if env_path.exists() {
        if let Ok(content) = std::fs::read_to_string(env_path) {
            for line in content.lines() {
                if line.starts_with(&format!("{}=", key)) {
                    lines.push(format!("{}={}", key, value));
                    found = true;
                } else {
                    lines.push(line.to_string());
                }
            }
        }
    }

    if !found {
        lines.push(format!("{}={}", key, value));
    }

    std::fs::write(env_path, lines.join("\n") + "\n").map_err(|e| {
        crate::error::ConfigError::FileWrite {
            path: env_path.display().to_string(),
            source: e,
        }
        .into()
    })
}

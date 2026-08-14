use crate::config::Config;
use crate::error::Result;
use std::io::{self, BufRead, Write};
use std::path::Path;

pub struct ConfigMenu;

impl ConfigMenu {
    pub fn run_interactive(workspace: &Path) -> Result<()> {
        let stdout = io::stdout();
        let mut handle = stdout.lock();
        let stdin = io::stdin();
        let mut reader = stdin.lock();

        writeln!(
            handle,
            "\x1b[1;35m⚡ minicode — Interactive Configuration\x1b[0m"
        )?;
        writeln!(
            handle,
            "\x1b[90mSettings will be saved to ~/.config/minicode/minicode.toml and .env\x1b[0m\n"
        )?;

        let mut config = Config::load(Some(workspace), None).unwrap_or_default();

        // 1. Select Provider
        writeln!(handle, "\x1b[1m1. Select Default Provider:\x1b[0m")?;
        writeln!(
            handle,
            "   [1] OpenRouter (100+ models: Claude, DeepSeek, GPT-4o, Qwen) [Default]"
        )?;
        writeln!(handle, "   [2] Google Gemini")?;
        writeln!(
            handle,
            "   [3] OpenAI / OpenAI-Compatible (DeepSeek, Groq, Together)"
        )?;
        writeln!(handle, "   [4] Anthropic Claude")?;
        writeln!(handle, "   [5] Ollama (Local LLM)")?;
        write!(
            handle,
            "   Enter choice [1-5] (current: {}): ",
            config.provider.default
        )?;
        handle.flush()?;

        let mut input = String::new();
        reader.read_line(&mut input)?;
        let choice = input.trim();
        let (provider, default_model) = match choice {
            "2" => ("gemini".to_string(), "gemini-2.5-pro".to_string()),
            "3" => ("openai".to_string(), "gpt-4o".to_string()),
            "4" => (
                "anthropic".to_string(),
                "claude-3-5-sonnet-20241022".to_string(),
            ),
            "5" => ("ollama".to_string(), "qwen2.5-coder".to_string()),
            _ => (
                "openrouter".to_string(),
                "anthropic/claude-3.5-sonnet".to_string(),
            ),
        };
        config.provider.default = provider;

        // 2. Select Model
        writeln!(handle, "\n\x1b[1m2. Default Model Name:\x1b[0m")?;
        write!(handle, "   Enter model ID (default: {}): ", default_model)?;
        handle.flush()?;
        input.clear();
        reader.read_line(&mut input)?;
        let model_input = input.trim();
        if !model_input.is_empty() {
            config.provider.model = model_input.to_string();
        } else {
            config.provider.model = default_model;
        }

        // 3. API Key setup
        writeln!(
            handle,
            "\n\x1b[1m3. API Key for {}:\x1b[0m",
            config.provider.default
        )?;
        let env_key_name = match config.provider.default.as_str() {
            "openrouter" => "OPENROUTER_API_KEY",
            "gemini" => "GEMINI_API_KEY",
            "openai" => "OPENAI_API_KEY",
            "anthropic" => "ANTHROPIC_API_KEY",
            _ => "API_KEY",
        };
        write!(
            handle,
            "   Enter API key for {} (leave empty to keep current): ",
            env_key_name
        )?;
        handle.flush()?;
        input.clear();
        reader.read_line(&mut input)?;
        let key_input = input.trim();
        if !key_input.is_empty() {
            // Append or update .env
            let env_path = workspace.join(".env");
            let mut env_content = if env_path.exists() {
                std::fs::read_to_string(&env_path).unwrap_or_default()
            } else {
                String::new()
            };

            let key_line = format!("{}={}", env_key_name, key_input);
            if env_content.contains(env_key_name) {
                let lines: Vec<String> = env_content
                    .lines()
                    .map(|l| {
                        if l.starts_with(env_key_name) {
                            key_line.clone()
                        } else {
                            l.to_string()
                        }
                    })
                    .collect();
                env_content = lines.join("\n");
            } else {
                env_content.push_str(&format!("\n{}\n", key_line));
            }
            std::fs::write(&env_path, env_content).ok();
            writeln!(handle, "   \x1b[32m✔ Saved key to .env\x1b[0m")?;
        }

        // 4. Auto approve toggle
        writeln!(handle, "\n\x1b[1m4. Tool Execution Approval Policy:\x1b[0m")?;
        writeln!(
            handle,
            "   [1] Strict Mode (Ask confirmation before running shell commands / modifying files)"
        )?;
        writeln!(
            handle,
            "   [2] Auto-Approve Mode (Automatically allow agent tools)"
        )?;
        write!(
            handle,
            "   Enter choice [1-2] (current: {}): ",
            if config.agent.auto_approve { "2" } else { "1" }
        )?;
        handle.flush()?;
        input.clear();
        reader.read_line(&mut input)?;
        config.agent.auto_approve = input.trim() == "2";

        // 5. Theme
        config.ui.theme = "aura-dark".to_string();

        // Save TOML config to ~/.config/minicode/minicode.toml
        if let Some(config_dir) = dirs::config_dir() {
            let target_dir = config_dir.join("minicode");
            std::fs::create_dir_all(&target_dir).ok();
            let toml_path = target_dir.join("minicode.toml");
            let toml_str = toml::to_string_pretty(&config).unwrap_or_default();
            std::fs::write(&toml_path, toml_str).ok();
            writeln!(
                handle,
                "\n\x1b[32m✔ Configuration saved to {}\x1b[0m",
                toml_path.display()
            )?;
        }

        writeln!(
            handle,
            "\x1b[1;32m⚡ Configuration complete! Run `minicode` to launch the agent.\x1b[0m\n"
        )?;

        Ok(())
    }
}

# Task 3 Brief: Provider Workflow & Interactive Menu Hierarchy

## Objective
Implement `SetupWizard` in `src/ui/setup/wizard.rs` providing the modern hierarchical interactive menu flow (Main Menu -> Provider Menu -> Available Providers / Custom Provider), wire inline API key input and auto-activation, and connect `ConfigMenu::run_interactive` to `SetupWizard::run`.

## Files to Create / Modify
- Create: `src/ui/setup/wizard.rs`
- Modify: `src/ui/setup/mod.rs` (export `pub mod wizard;` and re-export `SetupWizard`)
- Modify: `src/ui/configure.rs` (delegate `ConfigMenu::run_interactive` to `SetupWizard::run(workspace)`)
- Test: `src/ui/setup/wizard.rs` (inline unit tests)

## Constraints & Requirements
1. **Compilation Concurrency:** ONLY run `cargo check -j 1` and `cargo test -j 1`.
2. **Targeted Test Execution:** ONLY run `cargo test -j 1 --lib ui::setup::wizard::tests`. NEVER run the full test suite.
3. **Zero Unwraps:** No `.unwrap()` or `.expect()` in non-test code. Propagate `crate::error::Result<T>`.
4. **Interactive TTY Check:**
   - Detect `!std::io::stdin().is_terminal()`. If running non-interactively, return `crate::error::ConfigError` or `anyhow::bail!("'minicode setup' requires an interactive terminal.")`.
5. **Menu Hierarchy & Flow:**
   - **Level 1: Main Menu**
     - Header:
       ```text
       ⚡ minicode — Interactive Setup Wizard
       Active Provider: {provider} | Active Model: {model}
       ```
     - Items:
       - `SelectorItem::new("provider", "⚡ Provider").with_hint("Setup & manage AI providers")`
       - `SelectorItem::new("back", "◄ Back / Exit").with_hint("Save changes and return to shell")`
     - On "back" / Esc / Ctrl+C: save with `ConfigMenu::save_all(&config, workspace)?`, print confirmation receipt, and exit cleanly.
   - **Level 2: Provider Menu**
     - Header: `=== Provider Configuration ===`
     - Items:
       - `SelectorItem::new("available", "🌐 Available Providers").with_hint("MiniMax, Z.ai, OpenRouter, Gemini, OpenAI, etc.")`
       - `SelectorItem::new("custom", "🔌 Custom Provider").with_hint("OpenAI-compatible endpoints (vLLM, Ollama, etc.)")`
       - `SelectorItem::new("back", "◄ Back").with_hint("Return to main menu")`
   - **Level 3: Available Providers**
     - Catalog (10 providers):
       - `openrouter` ("OpenRouter", "OPENROUTER_API_KEY", "100+ models: Claude, DeepSeek, Qwen")
       - `gemini` ("Google Gemini", "GEMINI_API_KEY", "Gemini 2.5 Pro & Flash")
       - `openai` ("OpenAI", "OPENAI_API_KEY", "GPT-4o, o3-mini")
       - `deepseek` ("DeepSeek", "DEEPSEEK_API_KEY", "DeepSeek-V3, R1 — ultra-low cost")
       - `groq` ("Groq", "GROQ_API_KEY", "Llama 3.3, Qwen — fast inference")
       - `minimax` ("MiniMax", "MINIMAX_API_KEY", "MiniMax-M2.7, Text-01")
       - `z.ai` ("Z.ai / Zhipu GLM", "ZHIPU_API_KEY", "GLM-4-Plus, GLM-4-Flash")
       - `together` ("Together AI", "TOGETHER_API_KEY", "Open-source model hosting")
       - `mistral` ("Mistral AI", "MISTRAL_API_KEY", "Codestral, Mistral Large")
       - `ollama` ("Ollama (Local)", "OLLAMA_API_KEY", "100% Free local at localhost:11434")
     - Live badges:
       - If `config.provider.default == id`: `● Active`
       - Else if key exists: `✔ Configured (sk-••••)`
       - Else if `ollama`: `○ Localhost`
       - Else: `○ Not Set`
     - Selection logic:
       - If `ollama`: activate immediately, save, print receipt.
       - If NOT configured: call `prompt_api_key`. If key entered, save key, set default, call `ConfigMenu::save_all`, print receipt.
       - If ALREADY configured: prompt 2-choice selector:
         - `[1] ⚡ Set as Active Provider` -> set default, save, print receipt.
         - `[2] 🔑 Reconfigure API Key` -> prompt new key, update key, set default, save, print receipt.
         - `[0] ◄ Back` -> return to list.
   - **Level 3: Custom Provider**
     - Prompt name: `prompt_text("Provider Identifier Name (e.g. 'vllm-local')", None, false)`
     - Prompt base URL: `prompt_text("OpenAI-Compatible Base URL", Some("http://localhost:8000/v1"), false)`
     - Prompt key: `prompt_text("API Key (optional, press Enter to skip)", None, true)`
     - Store in `config.provider.custom_endpoints`, `config.provider.api_keys`, set default, save, print receipt.
6. **Code Quality:**
   - `cargo fmt`
   - `cargo clippy -j 1 --bin minicode -- -D warnings`
7. **Commit:**
   - `feat(ui): implement modern hierarchical interactive setup wizard`

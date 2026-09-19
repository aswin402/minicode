use minicode::ui::setup::SetupWizard;
use std::process::{Command, Stdio};

#[test]
fn test_setup_help_command() {
    let bin = env!("CARGO_BIN_EXE_minicode");
    let output = Command::new(bin)
        .args(["setup", "--help"])
        .output()
        .expect("Failed to execute minicode setup --help");

    assert!(
        output.status.success(),
        "minicode setup --help failed with status: {:?}",
        output.status
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("setup") || stdout.contains("Interactive configuration wizard"),
        "Help output should describe setup command: {}",
        stdout
    );
}

#[test]
fn test_configure_alias_help() {
    let bin = env!("CARGO_BIN_EXE_minicode");
    let output = Command::new(bin)
        .args(["configure", "--help"])
        .output()
        .expect("Failed to execute minicode configure --help");

    assert!(
        output.status.success(),
        "minicode configure --help failed with status: {:?}",
        output.status
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("setup") || stdout.contains("Interactive configuration wizard"),
        "Help output should describe setup/configure command: {}",
        stdout
    );
}

#[test]
fn test_config_alias_help() {
    let bin = env!("CARGO_BIN_EXE_minicode");
    let output = Command::new(bin)
        .args(["config", "--help"])
        .output()
        .expect("Failed to execute minicode config --help");

    assert!(
        output.status.success(),
        "minicode config --help failed with status: {:?}",
        output.status
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("setup") || stdout.contains("Interactive configuration wizard"),
        "Help output should describe setup/config command: {}",
        stdout
    );
}

#[test]
fn test_setup_non_interactive_fails_gracefully() {
    let bin = env!("CARGO_BIN_EXE_minicode");
    let output = Command::new(bin)
        .arg("setup")
        .stdin(Stdio::null())
        .output()
        .expect("Failed to execute minicode setup with Stdio::null");

    assert!(
        !output.status.success(),
        "minicode setup must fail when executed non-interactively"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let combined = format!("{stdout}\n{stderr}");

    assert!(
        !stderr.contains("panicked at"),
        "minicode setup should not panic in non-interactive environment: {}",
        stderr
    );
    assert!(
        combined.contains("interactive terminal"),
        "Expected error message mentioning interactive terminal, got: {}",
        combined
    );
}

#[test]
fn test_setup_wizard_provider_catalog_completeness() {
    let catalog = SetupWizard::provider_catalog();
    assert_eq!(
        catalog.len(),
        10,
        "Expected exactly 10 canonical providers in catalog"
    );

    let expected_providers = [
        ("openrouter", "OPENROUTER_API_KEY"),
        ("gemini", "GEMINI_API_KEY"),
        ("openai", "OPENAI_API_KEY"),
        ("deepseek", "DEEPSEEK_API_KEY"),
        ("groq", "GROQ_API_KEY"),
        ("minimax", "MINIMAX_API_KEY"),
        ("z.ai", "ZHIPU_API_KEY"),
        ("together", "TOGETHER_API_KEY"),
        ("mistral", "MISTRAL_API_KEY"),
        ("ollama", "OLLAMA_API_KEY"),
    ];

    for (id, expected_env) in expected_providers {
        let entry = catalog.iter().find(|(cat_id, _, _)| *cat_id == id);
        assert!(
            entry.is_some(),
            "Provider '{}' must be present in provider_catalog",
            id
        );

        let env_var = SetupWizard::env_var_for_provider(id);
        assert_eq!(
            env_var, expected_env,
            "Environment variable for '{}' must match {}",
            id, expected_env
        );

        let custom_env = SetupWizard::custom_env_var(id);
        assert_eq!(
            custom_env, expected_env,
            "custom_env_var for '{}' must match {}",
            id, expected_env
        );
    }
}

#[test]
fn test_main_help_lists_setup_subcommand() {
    let bin = env!("CARGO_BIN_EXE_minicode");
    let output = Command::new(bin)
        .arg("--help")
        .output()
        .expect("Failed to execute minicode --help");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("setup") && stdout.contains("Interactive configuration wizard"),
        "minicode --help should list 'setup' subcommand: {}",
        stdout
    );
}

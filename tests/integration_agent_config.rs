//! End-to-end integration tests for Autonomous Configuration & Workspace Memory (Phase 137).
//!
//! Covers:
//! 1. Per-directory workspace model memory across multiple workspaces.
//! 2. In-TUI /settings command modal and parametric subcommands (`model`, `auto_approve`, `thinking`).
//! 3. Dedicated `agent_config` tool suite with zero-leak credential masking.
//! 4. Autonomous `update_agent_config` permission gate and proposal persistence.
//! 5. Provider connection probe diagnostics and zero plaintext key leakage.

use minicode::app::{AgentCommand, App, CommandAction};
use minicode::config::{
    load_workspace_preference_from_file, mask_api_key, save_workspace_preference_to_file, Config,
};
use minicode::tools::registry::agent_tools::config_tools::{
    apply_proposal, dispatch as dispatch_config_tools, test_all_provider_connections,
    ConfigChangeProposal,
};
use minicode::ui::modal::ModalState;
use minicode::ui::modals::settings::SettingsTab;
use serde_json::json;
use std::fs;
use tempfile::tempdir;
use tokio::sync::mpsc;

#[tokio::test]
async fn test_per_directory_model_memory() {
    let temp_dir = tempdir().expect("tempdir");
    let ws_a = temp_dir.path().join("project_alpha");
    let ws_b = temp_dir.path().join("project_beta");
    let reg_path = temp_dir.path().join("workspaces.toml");

    fs::create_dir_all(&ws_a).expect("create ws_a");
    fs::create_dir_all(&ws_b).expect("create ws_b");

    // 1. Save preferences for Workspace A and Workspace B in the shared registry
    save_workspace_preference_to_file(&ws_a, "anthropic", "claude-3-7-sonnet-20250219", &reg_path)
        .expect("save ws_a");

    save_workspace_preference_to_file(&ws_b, "ollama", "qwen2.5-coder:latest", &reg_path)
        .expect("save ws_b");

    // 2. Verify independent roundtrip from disk registry
    let pref_a =
        load_workspace_preference_from_file(&ws_a, &reg_path).expect("pref_a exists in registry");
    assert_eq!(pref_a.provider, "anthropic");
    assert_eq!(pref_a.model, "claude-3-7-sonnet-20250219");

    let pref_b =
        load_workspace_preference_from_file(&ws_b, &reg_path).expect("pref_b exists in registry");
    assert_eq!(pref_b.provider, "ollama");
    assert_eq!(pref_b.model, "qwen2.5-coder:latest");

    // 3. Verify resolution hierarchy loads workspace preference dynamically
    let mut config = Config::default();
    config.provider.default = String::new();
    config.provider.model = String::new();

    let (prov_a, model_a) =
        config.resolve_active_provider_and_model_with_registry(Some(&ws_a), Some(&reg_path));
    assert_eq!(prov_a, "anthropic");
    assert_eq!(model_a, "claude-3-7-sonnet-20250219");

    let (prov_b, model_b) =
        config.resolve_active_provider_and_model_with_registry(Some(&ws_b), Some(&reg_path));
    assert_eq!(prov_b, "ollama");
    assert_eq!(model_b, "qwen2.5-coder:latest");

    // 4. Update preference in Workspace A and ensure Workspace B is unaffected
    save_workspace_preference_to_file(&ws_a, "deepseek", "deepseek-chat", &reg_path)
        .expect("update ws_a");

    let (updated_prov_a, updated_model_a) =
        config.resolve_active_provider_and_model_with_registry(Some(&ws_a), Some(&reg_path));
    assert_eq!(updated_prov_a, "deepseek");
    assert_eq!(updated_model_a, "deepseek-chat");

    let (intact_prov_b, intact_model_b) =
        config.resolve_active_provider_and_model_with_registry(Some(&ws_b), Some(&reg_path));
    assert_eq!(intact_prov_b, "ollama");
    assert_eq!(intact_model_b, "qwen2.5-coder:latest");
}

#[tokio::test]
async fn test_settings_command_modal_and_subcommands() {
    let dir = tempdir().expect("tempdir");
    let config = Config::default();
    let mut app = App::new(dir.path(), config);
    let (control_tx, _control_rx) = mpsc::unbounded_channel::<AgentCommand>();

    // 1. Plain /settings command opens ModalState::Settings
    let action = app
        .handle_command_or_prompt("/settings", None, &control_tx)
        .await
        .expect("handle /settings");
    assert_eq!(action, CommandAction::Continue);

    match &app.modal {
        ModalState::Settings(state) => {
            assert_eq!(state.active_tab, SettingsTab::Providers);
            assert_eq!(state.selected_index, 0);
        }
        other => panic!("Expected ModalState::Settings, got {:?}", other),
    }

    // Close modal
    app.modal = ModalState::None;

    // 2. /settings auto_approve on / off
    app.handle_command_or_prompt("/settings auto_approve on", None, &control_tx)
        .await
        .expect("auto_approve on");
    assert!(app.config.agent.auto_approve);

    app.handle_command_or_prompt("/settings auto_approve off", None, &control_tx)
        .await
        .expect("auto_approve off");
    assert!(!app.config.agent.auto_approve);

    // 3. /settings thinking <tokens>
    app.handle_command_or_prompt("/settings thinking 8192", None, &control_tx)
        .await
        .expect("thinking 8192");
    assert_eq!(app.config.provider.thinking_budget, Some(8192));

    app.handle_command_or_prompt("/settings thinking 0", None, &control_tx)
        .await
        .expect("thinking 0");
    assert_eq!(app.config.provider.thinking_budget, None);

    // 4. /settings model <provider> <model>
    app.handle_command_or_prompt(
        "/settings model anthropic claude-3-5-haiku-20241022",
        None,
        &control_tx,
    )
    .await
    .expect("model anthropic");
    assert_eq!(
        app.config
            .provider
            .default_models
            .get("anthropic")
            .map(|s| s.as_str()),
        Some("claude-3-5-haiku-20241022")
    );
}

#[tokio::test]
async fn test_agent_config_tools_and_masking() {
    std::env::remove_var("MINICODE_PROVIDER");
    std::env::remove_var("MINICODE_MODEL");

    let dir = tempdir().expect("tempdir");
    let ws_path = dir.path();
    let minicode_dir = ws_path.join(".minicode");
    fs::create_dir_all(&minicode_dir).expect("create .minicode dir");

    // 1. Prepare a config with a test secret
    let raw_secret = "sk-ant-api03-1234567890abcdef";
    let mut config = Config::default();
    config.provider.default = "anthropic".to_string();
    config.provider.model = "claude-3-7-sonnet-20250219".to_string();
    config
        .provider
        .api_keys
        .insert("anthropic".to_string(), raw_secret.to_string());
    config.save(Some(ws_path)).expect("save config");

    // 2. Dispatch get_agent_config tool
    let res = dispatch_config_tools("get_agent_config", &json!({}), ws_path)
        .await
        .expect("dispatch get_agent_config")
        .expect("tool execution success");

    let parsed: serde_json::Value = serde_json::from_str(&res).expect("parse json");
    assert_eq!(parsed["active_provider"], "anthropic");
    assert_eq!(parsed["active_model"], "claude-3-7-sonnet-20250219");

    // 3. Verify Zero-Leak Credential Invariant:
    // Raw secret must never appear in output; masked version must be present
    let masked_expected = mask_api_key(raw_secret);
    assert_eq!(
        parsed["masked_keys"]["anthropic"], masked_expected,
        "Masked key should match mask_api_key helper"
    );
    assert!(
        !res.contains(raw_secret),
        "Raw plaintext API key leaked into get_agent_config response!"
    );
    assert!(
        !res.contains("1234567890"),
        "Raw secret substring leaked into response!"
    );
}

#[tokio::test]
async fn test_update_agent_config_permission_gate_and_persistence() {
    std::env::remove_var("MINICODE_PROVIDER");
    std::env::remove_var("MINICODE_MODEL");

    let dir = tempdir().expect("tempdir");
    let ws_path = dir.path();
    let minicode_dir = ws_path.join(".minicode");
    fs::create_dir_all(&minicode_dir).expect("create .minicode dir");

    // 1. Calling update_agent_config returns a structured proposal requiring confirmation
    let args = json!({
        "provider": "deepseek",
        "model": "deepseek-chat",
        "auto_approve": true,
        "thinking_budget": 4096,
        "theme": "nord",
        "scope": "workspace"
    });

    let res = dispatch_config_tools("update_agent_config", &args, ws_path)
        .await
        .expect("dispatch update_agent_config")
        .expect("proposal generated");

    let parsed: serde_json::Value = serde_json::from_str(&res).expect("parse json");
    assert_eq!(parsed["status"], "proposal_generated");
    assert_eq!(parsed["requires_confirmation"], true);
    assert_eq!(parsed["proposal"]["provider"], "deepseek");
    assert_eq!(parsed["proposal"]["model"], "deepseek-chat");

    // 2. Applying the proposal persists to workspace config and workspace registry
    let proposal = ConfigChangeProposal {
        provider: Some("deepseek".to_string()),
        model: Some("deepseek-chat".to_string()),
        auto_approve: Some(true),
        thinking_budget: Some(4096),
        theme: Some("nord".to_string()),
        scope: "workspace".to_string(),
    };

    apply_proposal(&proposal, ws_path).expect("apply proposal");

    // 3. Reload config from workspace and verify changes took effect
    let reloaded = Config::load(Some(ws_path), None).expect("reload config");
    assert_eq!(reloaded.provider.default, "deepseek");
    assert_eq!(reloaded.provider.model, "deepseek-chat");
    assert!(reloaded.agent.auto_approve);
    assert_eq!(reloaded.provider.thinking_budget, Some(4096));
    assert_eq!(reloaded.ui.theme, "nord");
}

#[tokio::test]
async fn test_connection_probe_diagnostic_format_and_zero_leak() {
    let dir = tempdir().expect("tempdir");
    let ws_path = dir.path();

    // 1. Dispatch test_provider_connection for a local endpoint
    let args = json!({ "provider": "ollama" });
    let res = dispatch_config_tools("test_provider_connection", &args, ws_path)
        .await
        .expect("dispatch test_provider_connection")
        .expect("probe executed");

    let parsed: serde_json::Value = serde_json::from_str(&res).expect("parse json");
    assert_eq!(parsed["provider"], "ollama");
    assert!(parsed["status"].is_string());
    assert!(parsed["latency_ms"].is_number());
    assert!(parsed["key_source"].is_string());

    // 2. Dispatch list_available_models with graceful static fallback
    let models_args = json!({ "provider": "anthropic" });
    let models_res = dispatch_config_tools("list_available_models", &models_args, ws_path)
        .await
        .expect("dispatch list_available_models")
        .expect("models query succeeded");

    let models: Vec<serde_json::Value> =
        serde_json::from_str(&models_res).expect("parse models array");
    assert!(!models.is_empty(), "Expected available models list");
    assert!(models
        .iter()
        .any(|m| m["id"] == "claude-3-7-sonnet-20250219"));

    // 3. Verify concurrent test_all_provider_connections completes successfully
    let config = Config::default();
    let all_reports = test_all_provider_connections(&config).await;
    assert_eq!(
        all_reports.len(),
        12,
        "Should test all 12 cloud & local providers"
    );
}

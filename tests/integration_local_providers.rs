use minicode::agent::provider::{create_provider_or_fallback, create_provider_with_base_url};
use minicode::agent::r#loop::AgentLoop;
use minicode::config::Config;
use tempfile::tempdir;

#[test]
fn test_local_provider_key_exemption() {
    let mut config = Config::default();

    // Standard local provider names have optional API keys
    assert!(config.is_local_provider("ollama"));
    assert!(config.is_local_provider("lmstudio"));
    assert!(config.is_local_provider("vllm"));
    assert!(config.is_local_provider("localhost"));
    assert!(config.is_local_provider("localai"));
    assert!(config.is_local_provider("llama.cpp"));

    assert_eq!(config.get_api_key("ollama").unwrap(), "");
    assert_eq!(config.get_api_key("lmstudio").unwrap(), "");
    assert_eq!(config.get_api_key("vllm").unwrap(), "");
    assert_eq!(config.get_api_key("localhost").unwrap(), "");

    // Custom endpoints pointing to localhost/127.0.0.1 are also recognized
    config.provider.custom_endpoints.insert(
        "custom-inference".to_string(),
        "http://127.0.0.1:11434/v1".to_string(),
    );
    assert!(config.is_local_provider("custom-inference"));
    assert_eq!(config.get_api_key("custom-inference").unwrap(), "");
}

#[test]
fn test_create_local_providers_without_keys() {
    // Creating local providers with empty API keys must succeed
    let ollama_res = create_provider_with_base_url("ollama", "", None);
    assert!(ollama_res.is_ok());

    let lmstudio_res = create_provider_with_base_url("lmstudio", "", None);
    assert!(lmstudio_res.is_ok());

    let vllm_res = create_provider_with_base_url("vllm", "", None);
    assert!(vllm_res.is_ok());

    let localhost_res = create_provider_with_base_url("localhost", "", None);
    assert!(localhost_res.is_ok());
}

#[tokio::test]
async fn test_unconfigured_provider_fallback_resilience() {
    let temp = tempdir().unwrap();
    let mut config = Config::default();
    config.provider.default = "nonexistent_provider".to_string();

    let key_res = config.get_api_key(&config.provider.default);
    let (provider, startup_err) =
        create_provider_or_fallback(&config.provider.default, key_res, None);

    // Fallback provider must be created and report error message
    assert!(startup_err.is_some());
    assert_eq!(provider.name(), "nonexistent_provider");
    assert_eq!(provider.default_model(), "unconfigured");

    // AgentLoop should initialize successfully with the fallback provider without crashing
    let mut agent = AgentLoop::new(temp.path(), config.clone(), provider);

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let res = agent.execute_turn("hello agent", tx, None).await;

    // Agent should return a polite error directing user to switch models, not panic
    assert!(res.is_err());
    let err_str = res.unwrap_err().to_string();
    assert!(err_str.contains("not configured") || err_str.contains("/model"));

    // Verify error event was sent to receiver
    if let Some(minicode::agent::types::AgentEvent::Error { message, .. }) = rx.recv().await {
        assert!(message.contains("not configured") || message.contains("/model"));
    }
}

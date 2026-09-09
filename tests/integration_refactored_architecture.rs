//! Integration test verifying refactored architecture, centralized constants,
//! string utilities, modularized providers, and decomposed modals.

use minicode::agent::providers::{create_provider, UnconfiguredProvider};
use minicode::agent::types::AgentEvent;
use minicode::agent::Provider;
use minicode::constants::{
    env_vars, CB_DEFAULT_COOLDOWN_SECS, CB_DEFAULT_FAILURE_THRESHOLD, CB_DEFAULT_HALF_OPEN_SUCCESS,
    DEFAULT_LOCAL_MODEL_NAME, LMSTUDIO_DEFAULT_BASE_URL, LOCALAI_DEFAULT_BASE_URL,
    VLLM_DEFAULT_BASE_URL,
};
use minicode::context::syntax_guard::SyntaxGuard;
use minicode::session::store::SessionStore;
use minicode::ui::layout_utils::compute_scroll_offset;
use minicode::ui::modals::ModalState;
use minicode::utils::strings::{mask_secret, truncate_chars, truncate_display};

#[test]
fn test_centralized_constants_integrity() {
    assert_eq!(LMSTUDIO_DEFAULT_BASE_URL, "http://localhost:1234/v1");
    assert_eq!(VLLM_DEFAULT_BASE_URL, "http://localhost:8000/v1");
    assert_eq!(LOCALAI_DEFAULT_BASE_URL, "http://localhost:8080/v1");
    assert_eq!(DEFAULT_LOCAL_MODEL_NAME, "local-model");

    assert_eq!(CB_DEFAULT_FAILURE_THRESHOLD, 3);
    assert_eq!(CB_DEFAULT_COOLDOWN_SECS, 10);
    assert_eq!(CB_DEFAULT_HALF_OPEN_SUCCESS, 2);

    assert_eq!(env_vars::MINICODE_MODEL, "MINICODE_MODEL");
    assert_eq!(env_vars::MINICODE_PROVIDER, "MINICODE_PROVIDER");
    assert_eq!(env_vars::MINICODE_THEME, "MINICODE_THEME");
}

#[test]
fn test_reusable_string_and_layout_utilities() {
    // UTF-8 multi-byte truncation: '🦀' is 1 char, ' ' is 1 char, 'R' is 1 char, 'o' is 1 char, 'c' is 1 char
    let unicode_str = "🦀 Rocket 🚀 Sparkles ✨";
    assert_eq!(truncate_chars(unicode_str, 5), "🦀 Roc");
    assert_eq!(truncate_display(unicode_str, 5), "🦀 Ro…");
    assert_eq!(truncate_display("hello", 10), "hello");

    // Secret masking
    let api_key = "sk-ant-api03-123456789abcdef";
    let masked = mask_secret(api_key, 4);
    assert!(masked.ends_with("cdef"));
    assert!(masked.contains("****"));

    // Viewport scrolling
    assert_eq!(compute_scroll_offset(0, 10), 0);
    assert_eq!(compute_scroll_offset(4, 10), 0);
    assert_eq!(compute_scroll_offset(9, 10), 0);
    assert_eq!(compute_scroll_offset(10, 10), 1);
    assert_eq!(compute_scroll_offset(15, 10), 6);
}

#[test]
fn test_syntax_guard_language_for_extension_dedup() {
    assert!(SyntaxGuard::language_for_extension("rs").is_some());
    assert!(SyntaxGuard::language_for_extension("py").is_some());
    assert!(SyntaxGuard::language_for_extension("js").is_some());
    assert!(SyntaxGuard::language_for_extension("ts").is_some());
    assert!(SyntaxGuard::language_for_extension("unknown").is_none());
}

#[test]
fn test_modularized_provider_submodules() {
    let provider = create_provider("anthropic", "test-key");
    assert!(provider.is_ok());
    let provider = provider.unwrap();
    assert_eq!(provider.name(), "anthropic");

    let direct_unconfigured = UnconfiguredProvider::new("custom", "missing key");
    assert_eq!(direct_unconfigured.name(), "custom");
}

#[test]
fn test_modularized_modals_instantiation() {
    let _ = ModalState::None;
    let _ = ModalState::ApiKeyInput {
        provider: "anthropic".to_string(),
        env_var: "ANTHROPIC_API_KEY".to_string(),
        input: "sk-ant-test".to_string(),
        cursor: 11,
    };
    let _ = ModalState::ProviderSelect {
        providers: vec!["gemini".to_string(), "anthropic".to_string()],
        selected_index: 0,
    };
    let _ = ModalState::ExitConfirm {
        workspace_name: "minicode".to_string(),
        selected_yes: false,
    };
    let _ = ModalState::Help;
    let _ = ModalState::SessionBrowser {
        sessions: vec![],
        selected_index: 0,
        cached_summary: None,
    };
    let _ = ModalState::new_theme_select("aura_dark", "dual_pillars");
    let _ = ModalState::UndoCheckpoint {
        checkpoints: vec![],
        selected_index: 0,
    };
}

#[test]
fn test_session_store_atomic_append_round_trip() {
    let temp_dir =
        std::env::temp_dir().join(format!("minicode_arch_test_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&temp_dir).unwrap();

    let store = SessionStore::with_dir(temp_dir.clone());
    let session_id = store.create_session(&temp_dir).unwrap();

    // Verify initial load
    let events = store.load_session(&session_id).unwrap();
    assert_eq!(events.len(), 0);

    // Append an event
    let event = AgentEvent::TurnStart {
        turn_id: 1,
        timestamp: chrono::Utc::now().to_rfc3339(),
        model: "dummy".to_string(),
        context_tokens: 100,
    };
    store.append_event(&session_id, &event).unwrap();

    // Verify loaded with appended event
    let loaded_again = store.load_session(&session_id).unwrap();
    assert_eq!(loaded_again.len(), 1);

    let _ = std::fs::remove_dir_all(&temp_dir);
}

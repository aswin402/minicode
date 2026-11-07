use super::*;

#[test]
fn test_default_config() {
    let config = Config::default();
    assert_eq!(config.provider.default, "");
    assert_eq!(config.provider.model, "");
    assert_eq!(config.agent.timeout, 30);
    assert_eq!(config.agent.map_tokens, 1024);
    assert_eq!(config.agent.max_tool_iterations, 0);
    assert!(config.agent.auto_continue);
    assert_eq!(config.agent.max_auto_continues, 5);
    assert!(!config.ui.plain);
}

#[test]
fn test_agent_iteration_config_merge() {
    let toml_str = r#"
        [agent]
        max_tool_iterations = 25
        auto_continue = false
        max_auto_continues = 2
    "#;
    let raw: RawConfig = toml::from_str(toml_str).unwrap();
    let mut config = Config::default();
    config.merge_raw(raw);
    assert_eq!(config.agent.max_tool_iterations, 25);
    assert!(!config.agent.auto_continue);
    assert_eq!(config.agent.max_auto_continues, 2);

    // Test explicit 0 (unbounded) merge
    let unbounded_toml = r#"
        [agent]
        max_tool_iterations = 0
    "#;
    let raw_unbounded: RawConfig = toml::from_str(unbounded_toml).unwrap();
    config.merge_raw(raw_unbounded);
    assert_eq!(config.agent.max_tool_iterations, 0);
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
    assert!(config.agent.auto_approve);
    assert_eq!(config.agent.timeout, 60);
    assert_eq!(config.agent.map_tokens, 2048);
    assert!(config.ui.plain);
    assert_eq!(config.ui.theme, "dark");
}

#[test]
fn test_raw_config_merge_animation_and_ui_preferences() {
    let mut config = Config::default();
    assert_eq!(config.ui.animation, "dual_pillars");

    let override_toml = r#"
        [ui]
        animation = "braille_wave"
        theme = "monokai"
        show_cost = true
        todo_style = "rail"

        [agent]
        tool_mode = "full"
        syntax_barrier = false
        auto_lint = false
    "#;
    let raw: RawConfig = toml::from_str(override_toml).unwrap();
    config.merge_raw(raw);
    assert_eq!(config.ui.animation, "braille_wave");
    assert_eq!(config.ui.theme, "monokai");
    assert_eq!(config.ui.todo_style, "rail");
    assert_eq!(config.ui.todo_style_enum(), TodoWidgetStyle::Rail);
    assert!(config.ui.show_cost);
    assert_eq!(config.agent.tool_mode, ToolFilterMode::Full);
    assert!(!config.agent.syntax_barrier);
    assert!(!config.agent.auto_lint);
}

#[test]
fn test_todo_widget_style_enum_mapping() {
    let mut ui = UiConfig::default();
    assert_eq!(ui.todo_style, "tree");
    assert_eq!(ui.todo_style_enum(), TodoWidgetStyle::Tree);

    ui.todo_style = "card".to_string();
    assert_eq!(ui.todo_style_enum(), TodoWidgetStyle::Card);

    ui.todo_style = "box".to_string();
    assert_eq!(ui.todo_style_enum(), TodoWidgetStyle::Card);

    ui.todo_style = "rail".to_string();
    assert_eq!(ui.todo_style_enum(), TodoWidgetStyle::Rail);

    ui.todo_style = "gutter".to_string();
    assert_eq!(ui.todo_style_enum(), TodoWidgetStyle::Rail);

    ui.todo_style = "minimal".to_string();
    assert_eq!(ui.todo_style_enum(), TodoWidgetStyle::Minimal);

    ui.todo_style = "open".to_string();
    assert_eq!(ui.todo_style_enum(), TodoWidgetStyle::Minimal);

    ui.todo_style = "unknown".to_string();
    assert_eq!(ui.todo_style_enum(), TodoWidgetStyle::Tree);
}

#[test]
fn test_raw_config_merge_override_false() {
    let mut config = Config::default();
    config.agent.auto_approve = true;

    let override_toml = r#"
        [agent]
        auto_approve = false
    "#;
    let raw: RawConfig = toml::from_str(override_toml).unwrap();
    config.merge_raw(raw);
    assert!(!config.agent.auto_approve);
}

static ENV_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn test_get_api_key_trims_whitespace() {
    let _guard = ENV_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let config = Config::default();
    std::env::set_var("GEMINI_API_KEY", "  sk-test-gemini-key-12345\n\t ");
    let key = config.get_api_key("gemini").unwrap();
    assert_eq!(key, "sk-test-gemini-key-12345");
    std::env::remove_var("GEMINI_API_KEY");
}

#[test]
fn test_config_merges_ollama_from_toml() {
    let mut config = Config::default();
    let override_toml = r#"
        [provider.ollama]
        host = "http://192.168.1.100:11434"
    "#;
    let raw: RawConfig = toml::from_str(override_toml).unwrap();
    config.merge_raw(raw);
    assert_eq!(config.provider.ollama.host, "http://192.168.1.100:11434");
}

#[test]
fn test_config_merges_git_from_toml() {
    let mut config = Config::default();
    assert!(config.git.auto_commit);
    assert!(!config.git.dirty_commit);

    let override_toml = r#"
        [git]
        auto_commit = false
        dirty_commit = true
        ai_commit_messages = false
    "#;
    let raw: RawConfig = toml::from_str(override_toml).unwrap();
    config.merge_raw(raw);
    assert!(!config.git.auto_commit);
    assert!(config.git.dirty_commit);
    assert!(!config.git.ai_commit_messages);
}

const TEST_PROVIDER_ENV_KEYS: &[&str] = &[
    "ANTHROPIC_API_KEY",
    "GEMINI_API_KEY",
    "GOOGLE_API_KEY",
    "OPENAI_API_KEY",
    "OPENROUTER_API_KEY",
    "DEEPSEEK_API_KEY",
    "GROQ_API_KEY",
    "MISTRAL_API_KEY",
    "TOGETHER_API_KEY",
    "MINIMAX_API_KEY",
    "ZAI_API_KEY",
    "ZHIPU_API_KEY",
    "GLM_API_KEY",
    "HF_TOKEN",
    "HUGGINGFACE_API_KEY",
    "NVIDIA_API_KEY",
    "OPENCODE_API_KEY",
    "CEREBRAS_API_KEY",
    "AGNES_API_KEY",
    "NARA_API_KEY",
    "OFOX_API_KEY",
    "AIHUBMIX_API_KEY",
    "KIMI_API_KEY",
    "MOONSHOT_API_KEY",
];

fn with_clean_provider_env<R>(f: impl FnOnce() -> R) -> R {
    let _guard = ENV_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let saved: Vec<(&str, Option<String>)> = TEST_PROVIDER_ENV_KEYS
        .iter()
        .map(|&k| {
            let v = std::env::var(k).ok();
            std::env::remove_var(k);
            (k, v)
        })
        .collect();
    let res = f();
    for (k, v) in saved {
        if let Some(val) = v {
            std::env::set_var(k, val);
        } else {
            std::env::remove_var(k);
        }
    }
    res
}

#[test]
fn test_get_api_key_from_persistent_map() {
    with_clean_provider_env(|| {
        let mut config = Config::default();
        config.provider.api_keys.insert(
            "openrouter".to_string(),
            "sk-or-v1-persist-test".to_string(),
        );
        config
            .provider
            .api_keys
            .insert("minimax".to_string(), "mm-test-key".to_string());
        config
            .provider
            .api_keys
            .insert("z.ai".to_string(), "glm-test-key".to_string());

        assert_eq!(
            config.get_api_key("openrouter").unwrap(),
            "sk-or-v1-persist-test"
        );
        assert_eq!(config.get_api_key("minimax").unwrap(), "mm-test-key");
        assert_eq!(config.get_api_key("z.ai").unwrap(), "glm-test-key");
        assert_eq!(config.get_api_key("zhipu").unwrap(), "glm-test-key");
        assert_eq!(config.get_api_key("ollama").unwrap(), "");
    });
}

#[test]
fn test_local_providers_key_optional() {
    let mut config = Config::default();
    // Ollama, LM Studio, vLLM, localhost all return Ok("") without requiring API keys
    assert_eq!(config.get_api_key("ollama").unwrap(), "");
    assert_eq!(config.get_api_key("localhost").unwrap(), "");
    assert_eq!(config.get_api_key("local").unwrap(), "");
    assert_eq!(config.get_api_key("lmstudio").unwrap(), "");
    assert_eq!(config.get_api_key("vllm").unwrap(), "");
    assert_eq!(config.get_api_key("localai").unwrap(), "");
    assert_eq!(config.get_api_key("llama.cpp").unwrap(), "");

    // Custom endpoint on localhost is also detected as local
    config.provider.custom_endpoints.insert(
        "my-local-server".to_string(),
        "http://127.0.0.1:9090/v1".to_string(),
    );
    assert!(config.is_local_provider("my-local-server"));
    assert_eq!(config.get_api_key("my-local-server").unwrap(), "");

    // Base URL resolution
    assert_eq!(
        config.get_provider_base_url("lmstudio").unwrap(),
        "http://localhost:1234/v1"
    );
    assert_eq!(
        config.get_provider_base_url("vllm").unwrap(),
        "http://localhost:8000/v1"
    );
    assert_eq!(
        config.get_provider_base_url("my-local-server").unwrap(),
        "http://127.0.0.1:9090/v1"
    );

    // Default models
    assert!(!config.get_default_model_for_provider("ollama").is_empty());
    assert_eq!(
        config.get_default_model_for_provider("lmstudio"),
        "local-model"
    );
}

#[test]
fn test_mask_api_key_variations() {
    assert_eq!(mask_api_key(""), "");
    assert_eq!(mask_api_key("   "), "");
    assert_eq!(mask_api_key("short"), "••••••••");
    assert_eq!(mask_api_key("12345678"), "••••••••");
    assert_eq!(mask_api_key("123456789"), "1234...6789");
    assert_eq!(mask_api_key("🔑1234567890🦀"), "🔑123...890🦀");
    assert_eq!(mask_api_key("sk-ant-1234567890abcdef"), "sk-a...cdef");
    assert_eq!(mask_api_key("AIzaSyD-1234567890XYZ"), "AIza...0XYZ");
}

#[test]
fn test_workspace_preference_roundtrip() {
    let temp_dir = tempfile::tempdir().unwrap();
    let ws_path = temp_dir.path();

    // Initially none
    let pref = load_workspace_preference_from_file(ws_path, &ws_path.join("workspaces.toml"));
    assert!(pref.is_none());

    // Save preference
    save_workspace_preference_to_file(
        ws_path,
        "anthropic",
        "claude-3-7-sonnet-20250219",
        &ws_path.join("workspaces.toml"),
    )
    .unwrap();

    let pref =
        load_workspace_preference_from_file(ws_path, &ws_path.join("workspaces.toml")).unwrap();
    assert_eq!(pref.provider, "anthropic");
    assert_eq!(pref.model, "claude-3-7-sonnet-20250219");
}

#[test]
fn test_dynamic_provider_resolution_hierarchy() {
    with_clean_provider_env(|| {
        let temp_dir = tempfile::tempdir().unwrap();
        let ws_path = temp_dir.path();
        let reg_path = ws_path.join("workspaces.toml");

        let mut config = Config::default();
        config.provider.default = String::new();
        config.provider.model = String::new();

        // 0. Empty fallback when nothing is set and no workspace pref
        let empty_reg_dir = tempfile::tempdir().unwrap();
        let empty_ws = empty_reg_dir.path().join("empty_ws");
        let empty_reg = empty_reg_dir.path().join("workspaces.toml");
        let (empty_p, empty_m) = config
            .resolve_active_provider_and_model_with_registry(Some(&empty_ws), Some(&empty_reg));
        assert_eq!(empty_p, "");
        assert_eq!(empty_m, "");

        // 1. Auto-discovery from configured API keys when no workspace pref exists
        config
            .provider
            .api_keys
            .insert("openai".to_string(), "sk-test-key".to_string());
        let (disc_p, disc_m) = config
            .resolve_active_provider_and_model_with_registry(Some(&empty_ws), Some(&empty_reg));
        assert_eq!(disc_p, "openai");
        assert_eq!(disc_m, "gpt-4o");
        config.provider.api_keys.clear();

        // 2. With workspace preference saved
        save_workspace_preference_to_file(
            ws_path,
            "anthropic",
            "claude-3-7-sonnet-20250219",
            &reg_path,
        )
        .unwrap();

        let (prov, model) =
            config.resolve_active_provider_and_model_with_registry(Some(ws_path), Some(&reg_path));
        assert_eq!(prov, "anthropic");
        assert_eq!(model, "claude-3-7-sonnet-20250219");

        // 3. Explicit provider without model overrides workspace preference and gets default model
        config.provider.default = "mistral".to_string();
        config.provider.model = String::new();
        let (prov, model) =
            config.resolve_active_provider_and_model_with_registry(Some(ws_path), Some(&reg_path));
        assert_eq!(prov, "mistral");
        assert_eq!(model, config.get_default_model_for_provider("mistral"));

        // 4. CLI / explicit override with both takes highest precedence over workspace preference
        config.provider.default = "deepseek".to_string();
        config.provider.model = "deepseek-chat".to_string();
        let (prov, model) =
            config.resolve_active_provider_and_model_with_registry(Some(ws_path), Some(&reg_path));
        assert_eq!(prov, "deepseek");
        assert_eq!(model, "deepseek-chat");
    });
}

#[test]
fn test_default_models_override() {
    let mut config = Config::default();
    assert!(!config
        .get_default_model_for_provider("anthropic")
        .is_empty());
    assert!(!config.get_default_model_for_provider("openai").is_empty());

    config.provider.default_models.insert(
        "anthropic".to_string(),
        "claude-3-5-haiku-20241022".to_string(),
    );
    config
        .provider
        .default_models
        .insert("openai".to_string(), "o3-mini".to_string());

    assert_eq!(
        config.get_default_model_for_provider("anthropic"),
        "claude-3-5-haiku-20241022"
    );
    assert_eq!(config.get_default_model_for_provider("openai"), "o3-mini");
    assert!(!config.get_default_model_for_provider("gemini").is_empty());
}

#[test]
fn test_favorite_models_and_effort_level_cycling() {
    let mut config = Config::default();
    assert_eq!(config.provider.effective_level_label(), "medium");
    assert_eq!(config.provider.cycle_effort_level(), "high");
    assert_eq!(config.provider.effective_level_label(), "high");
    assert_eq!(config.provider.cycle_effort_level(), "low");
    assert_eq!(config.provider.effective_level_label(), "low");
    assert_eq!(config.provider.cycle_effort_level(), "medium");
    assert_eq!(config.provider.effective_level_label(), "medium");

    assert!(!config.is_favorite_model("openai", "gpt-5.4-mini"));
    assert!(config.toggle_favorite_model("openai", "gpt-5.4-mini"));
    assert!(config.is_favorite_model("openai", "gpt-5.4-mini"));
    assert_eq!(config.provider.favorite_models.len(), 1);
    assert!(!config.toggle_favorite_model("openai", "gpt-5.4-mini"));
    assert!(!config.is_favorite_model("openai", "gpt-5.4-mini"));
    assert!(config.provider.favorite_models.is_empty());
}

#[test]
fn test_four_slot_orchestrator_and_minipub_routing() {
    let mut config = Config::default();
    config.provider.default = "anthropic".to_string();
    config.provider.model = "claude-3-7-sonnet-20250219".to_string();
    config.provider.fallback_1 = Some(ModelSlot::new("openai", "gpt-4o"));
    config.provider.fallback_2 = Some(ModelSlot::new("gemini", "gemini-2.5-pro"));
    config.provider.global_last_fallback = Some(ModelSlot::new("ollama", "qwen2.5-coder:7b"));

    // 1. Orchestrator 4-slot chain resolves in order [1] Primary -> [2] 1st FB -> [3] 2nd FB -> [4] Global Last ★
    let orch_chain = config.resolve_orchestrator_chain(None);
    assert_eq!(orch_chain.len(), 4);
    assert_eq!(orch_chain[0].provider, "anthropic");
    assert_eq!(orch_chain[0].model, "claude-3-7-sonnet-20250219");
    assert_eq!(orch_chain[1].provider, "openai");
    assert_eq!(orch_chain[1].model, "gpt-4o");
    assert_eq!(orch_chain[2].provider, "gemini");
    assert_eq!(orch_chain[2].model, "gemini-2.5-pro");
    assert_eq!(orch_chain[3].provider, "ollama");
    assert_eq!(orch_chain[3].model, "qwen2.5-coder:7b");

    // 2. By default, all 14 MiniPubs inherit the Orchestrator's 4-slot chain
    for role in &[
        "scout",
        "architect",
        "planner",
        "designer",
        "frontend",
        "backend",
        "coder",
        "vision",
        "content_writer",
        "debugger",
        "reviewer",
        "security",
        "tester",
        "simplifier",
    ] {
        assert_eq!(config.resolve_minipub_chain(role, None), orch_chain);
        assert!(!config.has_custom_role_chain(role));
    }

    // 3. Customizing a specific MiniPub role (e.g. 'vision' or 'coder') overrides slots 1-3
    // while keeping Slot 4 (Global Last Fallback ★) shared across all agents!
    config.set_slot_for_target(
        "vision",
        1,
        Some(ModelSlot::new("gemini", "gemini-2.5-pro")),
    );
    config.set_slot_for_target("vision", 2, Some(ModelSlot::new("openai", "gpt-4o")));
    assert!(config.has_custom_role_chain("vision"));

    let vision_chain = config.resolve_minipub_chain("vision", None);
    assert_eq!(vision_chain[0].provider, "gemini");
    assert_eq!(vision_chain[0].model, "gemini-2.5-pro");
    assert_eq!(vision_chain[1].provider, "openai");
    assert_eq!(vision_chain[1].model, "gpt-4o");
    // Shared Slot 4 is always at the tail
    assert_eq!(vision_chain.last().unwrap().provider, "ollama");
    assert_eq!(vision_chain.last().unwrap().model, "qwen2.5-coder:7b");

    // 4. Setting Slot 4 from ANY target updates the shared Global Last Fallback ★ for everyone
    config.set_slot_for_target(
        "coder",
        4,
        Some(ModelSlot::new("openrouter", "deepseek/deepseek-chat")),
    );
    assert_eq!(
        config
            .resolve_orchestrator_chain(None)
            .last()
            .unwrap()
            .display_short(),
        "openrouter:deepseek/deepseek-chat"
    );
    assert_eq!(
        config
            .resolve_minipub_chain("vision", None)
            .last()
            .unwrap()
            .display_short(),
        "openrouter:deepseek/deepseek-chat"
    );

    // 5. One-click sync_all_minipubs_to_orchestrator resets all MiniPubs back to the Global chain
    config.sync_all_minipubs_to_orchestrator();
    assert!(!config.has_custom_role_chain("vision"));
    assert_eq!(
        config.resolve_minipub_chain("vision", None),
        config.resolve_orchestrator_chain(None)
    );
}

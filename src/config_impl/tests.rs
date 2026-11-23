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

    ui.todo_style = "nerdfont".to_string();
    assert_eq!(ui.todo_style_enum(), TodoWidgetStyle::NerdFont);
    ui.todo_style = "codicons".to_string();
    assert_eq!(ui.todo_style_enum(), TodoWidgetStyle::NerdFont);

    ui.todo_style = "kaomoji".to_string();
    assert_eq!(ui.todo_style_enum(), TodoWidgetStyle::Kaomoji);
    ui.todo_style = "kamoji".to_string();
    assert_eq!(ui.todo_style_enum(), TodoWidgetStyle::Kaomoji);

    ui.todo_style = "emoji".to_string();
    assert_eq!(ui.todo_style_enum(), TodoWidgetStyle::Emoji);

    ui.todo_style = "ascii".to_string();
    assert_eq!(ui.todo_style_enum(), TodoWidgetStyle::Ascii);

    ui.todo_style = "posh".to_string();
    assert_eq!(ui.todo_style_enum(), TodoWidgetStyle::Posh);
    ui.todo_style = "oh-my-posh".to_string();
    assert_eq!(ui.todo_style_enum(), TodoWidgetStyle::Posh);

    ui.todo_style = "unknown".to_string();
    assert_eq!(ui.todo_style_enum(), TodoWidgetStyle::Tree);

    // SwarmDashboardStyle new styles + loose parsing + next() cycle
    assert_eq!(
        SwarmDashboardStyle::from_str_loose("nerdfont"),
        SwarmDashboardStyle::NerdFont
    );
    assert_eq!(
        SwarmDashboardStyle::from_str_loose("kamoji"),
        SwarmDashboardStyle::Kaomoji
    );
    assert_eq!(
        SwarmDashboardStyle::from_str_loose("emoji"),
        SwarmDashboardStyle::Emoji
    );
    assert_eq!(
        SwarmDashboardStyle::from_str_loose("ascii"),
        SwarmDashboardStyle::Ascii
    );
    assert_eq!(
        SwarmDashboardStyle::from_str_loose("oh-my-posh"),
        SwarmDashboardStyle::Posh
    );
    assert_eq!(
        SwarmDashboardStyle::Posh.next(),
        SwarmDashboardStyle::Stylish
    );

    // ToastStyle new styles + loose parsing
    assert_eq!(ToastStyle::from_str_loose("nerdfont"), ToastStyle::NerdFont);
    assert_eq!(ToastStyle::from_str_loose("kamoji"), ToastStyle::Kaomoji);
    assert_eq!(ToastStyle::from_str_loose("emoji"), ToastStyle::Emoji);
    assert_eq!(ToastStyle::from_str_loose("ascii"), ToastStyle::Ascii);
    assert_eq!(ToastStyle::from_str_loose("posh"), ToastStyle::PoshPill);
    assert_eq!(
        ToastStyle::from_str_loose("posh_pill"),
        ToastStyle::PoshPill
    );

    // DoctorStyle all 10 styles + loose aliases + next() cycle + default UI toggles
    assert_eq!(ui.doctor_style_enum(), DoctorStyle::Aura);
    assert!(ui.show_doctor_sparkline);
    assert!(ui.show_doctor_subscores);
    assert!(ui.show_doctor_detail_drawer);
    assert!(ui.doctor_animate_reveal);
    assert_eq!(
        DoctorStyle::from_str_loose("nerdfont"),
        DoctorStyle::NerdFont
    );
    assert_eq!(DoctorStyle::from_str_loose("kamoji"), DoctorStyle::Kaomoji);
    assert_eq!(DoctorStyle::from_str_loose("emoji"), DoctorStyle::Emoji);
    assert_eq!(DoctorStyle::from_str_loose("ascii"), DoctorStyle::Ascii);
    assert_eq!(DoctorStyle::from_str_loose("oh-my-posh"), DoctorStyle::Posh);
    assert_eq!(
        DoctorStyle::from_str_loose("flutter_doctor"),
        DoctorStyle::Flutter
    );
    assert_eq!(DoctorStyle::from_str_loose("minimal"), DoctorStyle::Minimal);
    assert_eq!(DoctorStyle::from_str_loose("hud"), DoctorStyle::Cyber);
    assert_eq!(DoctorStyle::from_str_loose("compact"), DoctorStyle::Compact);
    assert_eq!(DoctorStyle::Compact.next(), DoctorStyle::Aura);
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
        config.get_provider_base_url("ollama").unwrap(),
        "http://localhost:11434/v1"
    );
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

#[test]
fn test_ui_theme_toast_symbols_and_non_destructive_map_preservation() {
    let dir = tempfile::tempdir().unwrap();
    let ws = dir.path();

    let mut cfg = Config::default();
    cfg.ui.theme = "catppuccin".to_string();
    cfg.ui.menu_style = "icons".to_string();
    cfg.ui.toast_symbols = "emoji".to_string();
    cfg.agent.ponytail_mode = true;
    cfg.agent.ponytail_level = "ultra".to_string();
    cfg.agent.caveman_mode = true;
    cfg.agent.caveman_level = "full".to_string();
    cfg.provider
        .api_keys
        .insert("minimax".to_string(), "sk-preserve-me".to_string());
    cfg.save_workspace(ws).unwrap();

    // Saving a fresh Config with empty `api_keys` over the same file preserves existing `api_keys`
    // while updating modified UI/agent fields
    let mut cfg2 = Config::load(Some(ws), None).unwrap();
    assert_eq!(cfg2.ui.theme, "catppuccin");
    assert_eq!(cfg2.ui.menu_style, "icons");
    assert_eq!(cfg2.ui.toast_symbols, "emoji");
    assert_eq!(cfg2.agent.ponytail_level, "ultra");
    assert_eq!(cfg2.agent.caveman_level, "full");

    cfg2.provider.api_keys.clear();
    cfg2.ui.theme = "tokyo-night".to_string();
    cfg2.save_workspace(ws).unwrap();

    let reloaded = Config::load(Some(ws), None).unwrap();
    assert_eq!(reloaded.ui.theme, "tokyo-night");
    assert_eq!(reloaded.ui.toast_symbols, "emoji");
    assert_eq!(
        reloaded
            .provider
            .api_keys
            .get("minimax")
            .map(String::as_str),
        Some("sk-preserve-me")
    );
}

#[test]
fn test_workspace_config_strips_and_scrubs_plaintext_api_keys() {
    let dir = tempfile::tempdir().unwrap();
    let ws_cfg_path = dir.path().join(".minicode").join("config.toml");

    // Pre-seed a previously leaked workspace config containing plaintext API keys
    std::fs::create_dir_all(ws_cfg_path.parent().unwrap()).unwrap();
    std::fs::write(
        &ws_cfg_path,
        "[provider]\ndefault = \"minimax\"\nmodel = \"MiniMax-M3.1-Flash\"\n\n[provider.api_keys]\ngemini = \"AIza-leaked-secret\"\ngroq = \"gsk_leaked_secret\"\n",
    )
    .unwrap();

    let mut cfg = Config::default();
    cfg.ui.theme = "aura-dark-plus".to_string();
    cfg.provider.default = "openrouter".to_string();
    cfg.provider.model = "deepseek/deepseek-chat".to_string();
    cfg.provider.api_keys.insert(
        "openrouter".to_string(),
        "sk-or-secret-never-write".to_string(),
    );
    cfg.provider
        .api_keys
        .insert("gemini".to_string(), "AIza-secret-never-write".to_string());

    cfg.write_config_to_path_with_options(&ws_cfg_path, true)
        .unwrap();

    let saved_raw = std::fs::read_to_string(&ws_cfg_path).unwrap();
    assert!(
        !saved_raw.contains("api_keys"),
        "Workspace config must not contain [provider.api_keys]:\n{saved_raw}"
    );
    assert!(!saved_raw.contains("AIza-leaked-secret"));
    assert!(!saved_raw.contains("gsk_leaked_secret"));
    assert!(!saved_raw.contains("sk-or-secret-never-write"));
    assert!(saved_raw.contains("aura-dark-plus"));
    assert!(saved_raw.contains("openrouter"));
}

#[test]
fn test_temp_dir_workspace_only_isolated_in_test_harness() {
    let tmp_ws = std::env::temp_dir().join("minicode_real_user_scratch");
    // Real CLI binary (is_test_env = false) targeting /tmp/... must NOT skip global config
    assert!(!Config::is_isolated_temp_workspace_with_env(&tmp_ws, false));
    // Test harness (is_test_env = true) targeting /tmp/... stays isolated
    assert!(Config::is_isolated_temp_workspace_with_env(&tmp_ws, true));

    // Real binaries vs Cargo test binaries under target/.../deps/<name>-<hex>
    assert!(!Config::exe_looks_like_cargo_test_binary(
        std::path::Path::new("/home/user/.local/bin/minicode")
    ));
    assert!(!Config::exe_looks_like_cargo_test_binary(
        std::path::Path::new("/repo/target/release/minicode")
    ));
    assert!(!Config::exe_looks_like_cargo_test_binary(
        std::path::Path::new("/repo/target/debug/minicode")
    ));
    assert!(Config::exe_looks_like_cargo_test_binary(
        std::path::Path::new("/repo/target/debug/deps/integration_agent_config-a1b2c3d4e5f67890")
    ));
    assert!(Config::exe_looks_like_cargo_test_binary(
        std::path::Path::new("/repo/target/debug/deps/minicode-deadbeef0123.exe")
    ));
}

#[test]
fn test_sync_global_dotenv_writes_all_configured_api_keys() {
    let dir = tempfile::tempdir().unwrap();
    let mut cfg = Config::default();
    cfg.provider
        .api_keys
        .insert("anthropic".to_string(), "sk-ant-test-111".to_string());
    cfg.provider
        .api_keys
        .insert("minimax".to_string(), "sk-mm-test-222".to_string());
    cfg.provider
        .api_keys
        .insert("kimi".to_string(), "sk-kimi-test-333".to_string());

    cfg.sync_global_dotenv(dir.path());

    let env_text = std::fs::read_to_string(dir.path().join(".env")).unwrap();
    assert!(env_text.contains("ANTHROPIC_API_KEY=sk-ant-test-111"));
    assert!(env_text.contains("MINIMAX_API_KEY=sk-mm-test-222"));
    assert!(env_text.contains("KIMI_API_KEY=sk-kimi-test-333"));
}

#[test]
fn test_load_global_dotenv_respects_newer_toml_and_imports_missing_keys() {
    let dir = tempfile::tempdir().unwrap();
    let env_path = dir.path().join(".env");
    std::env::remove_var("CEREBRAS_API_KEY");
    std::env::remove_var("AGNES_API_KEY");

    std::fs::write(
        &env_path,
        "CEREBRAS_API_KEY=old-stale-dotenv-key\nAGNES_API_KEY=dotenv-only-key\n",
    )
    .unwrap();

    // Simulate config.toml having a newer mtime than .env with an updated cerebras key
    let newer_toml_mtime = std::time::SystemTime::now() + std::time::Duration::from_secs(60);
    let mut cfg = Config::default();
    cfg.provider
        .api_keys
        .insert("cerebras".to_string(), "new-toml-key".to_string());

    cfg.load_global_dotenv_respecting_toml(&env_path, Some(newer_toml_mtime));

    // Newer config.toml key must NOT be shadowed by older .env
    assert_eq!(
        cfg.provider.api_keys.get("cerebras").map(String::as_str),
        Some("new-toml-key")
    );
    assert_eq!(cfg.get_api_key("cerebras").unwrap(), "new-toml-key");

    // Key only present in .env MUST be imported into api_keys so /settings & setup see it
    assert_eq!(
        cfg.provider.api_keys.get("agnes").map(String::as_str),
        Some("dotenv-only-key")
    );
    assert_eq!(cfg.get_api_key("agnes").unwrap(), "dotenv-only-key");

    std::env::remove_var("CEREBRAS_API_KEY");
    std::env::remove_var("AGNES_API_KEY");
}

#[test]
fn test_saving_cleared_optional_fields_removes_stale_browser_engine_and_fallbacks() {
    let dir = tempfile::tempdir().unwrap();
    let cfg_path = dir.path().join("config.toml");

    // Pre-seed disk with stale browser.default_engine = "firefox", fallback_1, and api_keys
    std::fs::write(
        &cfg_path,
        "[provider]\ndefault = \"minimax\"\nmodel = \"MiniMax-M2.7\"\nthinking_budget = 8192\n\n[provider.fallback_1]\nprovider = \"openrouter\"\nmodel = \"deepseek/deepseek-chat\"\n\n[provider.api_keys]\nminimax = \"sk-keep-secret\"\n\n[browser]\ndefault_engine = \"firefox\"\nheadless_priority = [\"minibrowse\", \"chrome\", \"firefox\"]\n",
    )
    .unwrap();

    // Reset default_engine, fallback_1, and thinking_budget back to None (auto / cleared)
    let mut cfg = Config::default();
    cfg.provider.default = "minimax".to_string();
    cfg.provider.model = "MiniMax-M2.7".to_string();
    cfg.browser.default_engine = None;
    cfg.provider.fallback_1 = None;
    cfg.provider.thinking_budget = None;

    cfg.write_config_to_path_with_options(&cfg_path, false)
        .unwrap();

    let saved_raw = std::fs::read_to_string(&cfg_path).unwrap();
    assert!(
        !saved_raw.contains("default_engine"),
        "Cleared browser.default_engine = None must remove stale 'firefox' from disk:\n{saved_raw}"
    );
    assert!(
        !saved_raw.contains("fallback_1"),
        "Cleared provider.fallback_1 = None must remove stale fallback from disk:\n{saved_raw}"
    );
    assert!(
        !saved_raw.contains("thinking_budget"),
        "Cleared provider.thinking_budget = None must remove stale budget from disk:\n{saved_raw}"
    );
    // Protected api_keys map must still be preserved!
    assert!(
        saved_raw.contains("sk-keep-secret"),
        "Protected [provider.api_keys] must still be preserved:\n{saved_raw}"
    );
}

#[test]
fn test_empty_provider_name_returns_clear_setup_error() {
    let cfg = Config::default();
    let err = cfg
        .get_api_key("")
        .expect_err("empty provider must return error")
        .to_string();
    assert!(
        err.contains("minicode setup"),
        "Expected friendly setup hint for empty provider, got: {err}"
    );
    assert!(
        !err.contains("_API_KEY"),
        "Must never emit bogus '_API_KEY' placeholder for empty provider, got: {err}"
    );
}

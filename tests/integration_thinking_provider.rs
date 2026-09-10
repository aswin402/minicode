use minicode::agent::provider::{
    create_provider_with_base_url, AnthropicProvider, CompletionOptions, OpenAiCompatibleProvider,
    StreamChunk, ToolSchema,
};
use minicode::agent::types::{Message, Role, ToolCall};
use minicode::config::{Config, ProviderConfig};
use minicode::constants::{
    ANTHROPIC_BASE_URL, ANTHROPIC_DEFAULT_MODEL, ANTHROPIC_VERSION_HEADER,
    DEFAULT_THINKING_BUDGET_TOKENS, MAX_THINKING_BUDGET_TOKENS, MIN_THINKING_BUDGET_TOKENS,
};
use std::collections::BTreeMap;

#[test]
fn test_anthropic_thinking_budget_request_serialization() {
    let messages = vec![
        Message::system("You are an expert systems engineer."),
        Message::user("Please analyze this architecture."),
    ];

    let tools = vec![ToolSchema {
        name: "view_file".to_string(),
        description: "View file contents".to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "AbsolutePath": { "type": "string" }
            },
            "required": ["AbsolutePath"]
        }),
    }];

    // 1. With thinking enabled (16,000 tokens)
    let options_with_thinking = CompletionOptions {
        model: ANTHROPIC_DEFAULT_MODEL.to_string(),
        temperature: 0.2,
        max_tokens: 32_000,
        system_instruction: Some("Always write safe code.".to_string()),
        thinking_budget: Some(16_000),
        reasoning_effort: None,
    };

    let body = AnthropicProvider::build_request_body(
        &messages,
        &tools,
        &options_with_thinking,
        ANTHROPIC_DEFAULT_MODEL,
    );

    assert_eq!(body["model"], ANTHROPIC_DEFAULT_MODEL);
    assert_eq!(body["max_tokens"], 32_000);
    assert_eq!(body["stream"], true);
    assert_eq!(body["thinking"]["type"], "enabled");
    assert_eq!(body["thinking"]["budget_tokens"], 16_000);
    // Anthropic requires temperature 1.0 when thinking is enabled
    assert_eq!(body["temperature"], 1.0);

    // System prompt combined
    let sys = body["system"].as_str().unwrap();
    assert!(sys.contains("Always write safe code."));
    assert!(sys.contains("You are an expert systems engineer."));

    // 2. With thinking disabled (budget = None)
    let options_without_thinking = CompletionOptions {
        model: ANTHROPIC_DEFAULT_MODEL.to_string(),
        temperature: 0.2,
        max_tokens: 8192,
        system_instruction: None,
        thinking_budget: None,
        reasoning_effort: None,
    };

    let body_no_think = AnthropicProvider::build_request_body(
        &messages,
        &tools,
        &options_without_thinking,
        ANTHROPIC_DEFAULT_MODEL,
    );

    assert!(body_no_think.get("thinking").is_none());
    assert!((body_no_think["temperature"].as_f64().unwrap() - 0.2).abs() < 1e-4);
    assert_eq!(body_no_think["max_tokens"], 8192);
}

#[test]
fn test_anthropic_thinking_budget_clamping() {
    let messages = vec![Message::user("Refactor database layer")];

    // Case A: max_tokens <= thinking_budget (violates Anthropic rules)
    // build_request_body must automatically ensure max_tokens > budget
    let options = CompletionOptions {
        model: ANTHROPIC_DEFAULT_MODEL.to_string(),
        temperature: 0.7,
        max_tokens: 8000,
        system_instruction: None,
        thinking_budget: Some(16000),
        reasoning_effort: None,
    };

    let body =
        AnthropicProvider::build_request_body(&messages, &[], &options, ANTHROPIC_DEFAULT_MODEL);
    assert_eq!(body["thinking"]["budget_tokens"], 16000);
    assert!(body["max_tokens"].as_u64().unwrap() > 16000);
    assert_eq!(body["max_tokens"], 16000 + 4096);

    // Case B: budget below minimum (e.g. 500 < MIN_THINKING_BUDGET_TOKENS = 1024)
    let options_under_min = CompletionOptions {
        model: ANTHROPIC_DEFAULT_MODEL.to_string(),
        temperature: 0.5,
        max_tokens: 8000,
        system_instruction: None,
        thinking_budget: Some(500),
        reasoning_effort: None,
    };

    let body_under = AnthropicProvider::build_request_body(
        &messages,
        &[],
        &options_under_min,
        ANTHROPIC_DEFAULT_MODEL,
    );
    assert!(body_under.get("thinking").is_none());
    assert_eq!(body_under["temperature"], 0.5);
}

#[test]
fn test_anthropic_messages_alternating_and_tool_result_grouping() {
    // Simulate parallel tool calls and consecutive tool responses
    let messages = vec![
        Message::user("Search files and inspect Cargo.toml"),
        Message {
            role: Role::Assistant,
            content: "I will view the files.".to_string(),
            tool_calls: Some(vec![
                ToolCall {
                    id: "call_a".to_string(),
                    name: "view_file".to_string(),
                    arguments: serde_json::json!({ "path": "Cargo.toml" }),
                },
                ToolCall {
                    id: "call_b".to_string(),
                    name: "list_dir".to_string(),
                    arguments: serde_json::json!({ "path": "src" }),
                },
            ]),
            tool_call_id: None,
            tool_name: None,
        },
        Message {
            role: Role::Tool,
            content: "[package]\nname = \"minicode\"".to_string(),
            tool_calls: None,
            tool_call_id: Some("call_a".to_string()),
            tool_name: Some("view_file".to_string()),
        },
        Message {
            role: Role::Tool,
            content: "main.rs\nlib.rs".to_string(),
            tool_calls: None,
            tool_call_id: Some("call_b".to_string()),
            tool_name: Some("list_dir".to_string()),
        },
    ];

    let formatted = AnthropicProvider::format_messages(&messages);

    // Anthropic requirement: exactly alternating User -> Assistant -> User
    assert_eq!(formatted.len(), 3);
    assert_eq!(formatted[0]["role"], "user");
    assert_eq!(formatted[1]["role"], "assistant");
    assert_eq!(formatted[2]["role"], "user");

    // Assistant message contains text block + 2 tool_use blocks
    let assistant_content = formatted[1]["content"].as_array().unwrap();
    assert_eq!(assistant_content.len(), 3);
    assert_eq!(assistant_content[0]["type"], "text");
    assert_eq!(assistant_content[1]["type"], "tool_use");
    assert_eq!(assistant_content[1]["id"], "call_a");
    assert_eq!(assistant_content[1]["name"], "view_file");
    assert_eq!(assistant_content[2]["type"], "tool_use");
    assert_eq!(assistant_content[2]["id"], "call_b");
    assert_eq!(assistant_content[2]["name"], "list_dir");

    // User message contains both tool_result blocks merged into a single turn
    let user_content = formatted[2]["content"].as_array().unwrap();
    assert_eq!(user_content.len(), 2);
    assert_eq!(user_content[0]["type"], "tool_result");
    assert_eq!(user_content[0]["tool_use_id"], "call_a");
    assert_eq!(user_content[1]["type"], "tool_result");
    assert_eq!(user_content[1]["tool_use_id"], "call_b");
}

#[test]
fn test_anthropic_sse_event_parsing_and_thought_blocks() {
    let mut tool_accumulator = BTreeMap::new();
    let mut prompt_tokens = 0usize;
    let mut completion_tokens = 0usize;
    let mut in_thinking_mode = false;

    // 1. message_start with prompt usage
    let start_event = serde_json::json!({
        "type": "message_start",
        "message": {
            "id": "msg_01ABC",
            "type": "message",
            "role": "assistant",
            "usage": { "input_tokens": 320, "output_tokens": 1 }
        }
    });
    let chunks = AnthropicProvider::parse_anthropic_event(
        "message_start",
        &start_event,
        &mut tool_accumulator,
        &mut prompt_tokens,
        &mut completion_tokens,
        &mut in_thinking_mode,
    );
    assert!(chunks.is_empty());
    assert_eq!(prompt_tokens, 320);

    // 2. content_block_start for thinking
    let think_start = serde_json::json!({
        "type": "content_block_start",
        "index": 0,
        "content_block": { "type": "thinking", "thinking": "" }
    });
    let chunks = AnthropicProvider::parse_anthropic_event(
        "content_block_start",
        &think_start,
        &mut tool_accumulator,
        &mut prompt_tokens,
        &mut completion_tokens,
        &mut in_thinking_mode,
    );
    assert_eq!(chunks, vec![StreamChunk::Delta("<thought>".to_string())]);

    // 3. content_block_delta with thinking_delta
    let think_delta = serde_json::json!({
        "type": "content_block_delta",
        "index": 0,
        "delta": {
            "type": "thinking_delta",
            "thinking": "Analyzing graph dependencies for circular references..."
        }
    });
    let chunks = AnthropicProvider::parse_anthropic_event(
        "content_block_delta",
        &think_delta,
        &mut tool_accumulator,
        &mut prompt_tokens,
        &mut completion_tokens,
        &mut in_thinking_mode,
    );
    assert_eq!(
        chunks,
        vec![StreamChunk::Delta(
            "Analyzing graph dependencies for circular references...".to_string()
        )]
    );

    // 3b. content_block_stop for thinking block
    let think_stop = serde_json::json!({
        "type": "content_block_stop",
        "index": 0
    });
    let stop_think_chunks = AnthropicProvider::parse_anthropic_event(
        "content_block_stop",
        &think_stop,
        &mut tool_accumulator,
        &mut prompt_tokens,
        &mut completion_tokens,
        &mut in_thinking_mode,
    );
    assert_eq!(
        stop_think_chunks,
        vec![StreamChunk::Delta("</thought>".to_string())]
    );

    // 4. content_block_start for tool_use
    let tool_start = serde_json::json!({
        "type": "content_block_start",
        "index": 1,
        "content_block": {
            "type": "tool_use",
            "id": "toolu_01XYZ",
            "name": "run_command"
        }
    });
    let _ = AnthropicProvider::parse_anthropic_event(
        "content_block_start",
        &tool_start,
        &mut tool_accumulator,
        &mut prompt_tokens,
        &mut completion_tokens,
        &mut in_thinking_mode,
    );

    // 5. content_block_delta with input_json_delta
    let tool_delta = serde_json::json!({
        "type": "content_block_delta",
        "index": 1,
        "delta": {
            "type": "input_json_delta",
            "partial_json": "{\"CommandLine\": \"cargo check\"}"
        }
    });
    let _ = AnthropicProvider::parse_anthropic_event(
        "content_block_delta",
        &tool_delta,
        &mut tool_accumulator,
        &mut prompt_tokens,
        &mut completion_tokens,
        &mut in_thinking_mode,
    );

    // 6. content_block_stop for tool_use
    let tool_stop = serde_json::json!({
        "type": "content_block_stop",
        "index": 1
    });
    let tool_chunks = AnthropicProvider::parse_anthropic_event(
        "content_block_stop",
        &tool_stop,
        &mut tool_accumulator,
        &mut prompt_tokens,
        &mut completion_tokens,
        &mut in_thinking_mode,
    );
    assert_eq!(tool_chunks.len(), 1);
    match &tool_chunks[0] {
        StreamChunk::ToolCallChunk(tc) => {
            assert_eq!(tc.id, "toolu_01XYZ");
            assert_eq!(tc.name, "run_command");
            assert_eq!(tc.arguments["CommandLine"], "cargo check");
        }
        other => panic!("Expected ToolCallChunk, got {:?}", other),
    }

    // 7. message_delta with output token count
    let msg_delta = serde_json::json!({
        "type": "message_delta",
        "delta": { "stop_reason": "tool_use" },
        "usage": { "output_tokens": 128 }
    });
    let _ = AnthropicProvider::parse_anthropic_event(
        "message_delta",
        &msg_delta,
        &mut tool_accumulator,
        &mut prompt_tokens,
        &mut completion_tokens,
        &mut in_thinking_mode,
    );
    assert_eq!(completion_tokens, 128);

    // 8. message_stop finishes the turn
    let stop_event = serde_json::json!({ "type": "message_stop" });
    let stop_chunks = AnthropicProvider::parse_anthropic_event(
        "message_stop",
        &stop_event,
        &mut tool_accumulator,
        &mut prompt_tokens,
        &mut completion_tokens,
        &mut in_thinking_mode,
    );
    assert_eq!(stop_chunks.len(), 2);
    assert_eq!(
        stop_chunks[0],
        StreamChunk::Usage {
            prompt_tokens: 320,
            completion_tokens: 128
        }
    );
    assert_eq!(stop_chunks[1], StreamChunk::Done);
}

#[test]
fn test_openai_reasoning_models_payload_generation() {
    let messages = vec![Message::user("Optimize algorithm")];

    // Model: o3-mini with thinking budget
    let options_o3 = CompletionOptions {
        model: "o3-mini".to_string(),
        temperature: 0.2,
        max_tokens: 8192,
        system_instruction: None,
        thinking_budget: Some(16_000),
        reasoning_effort: None,
    };

    let body_o3 = OpenAiCompatibleProvider::build_request_body(
        "openai",
        &messages,
        &[],
        &options_o3,
        "gpt-4o",
    );

    // OpenAI reasoning models do not allow custom temperature
    assert!(body_o3.get("temperature").is_none());
    // Uses max_completion_tokens
    assert_eq!(body_o3["max_completion_tokens"], 8192);
    // Automatic reasoning_effort mapping: 16k tokens -> "medium"
    assert_eq!(body_o3["reasoning_effort"], "medium");

    // Model: o1 with explicit reasoning_effort "high"
    let options_o1 = CompletionOptions {
        model: "o1".to_string(),
        temperature: 0.7,
        max_tokens: 16_000,
        system_instruction: None,
        thinking_budget: None,
        reasoning_effort: Some("high".to_string()),
    };

    let body_o1 = OpenAiCompatibleProvider::build_request_body(
        "openai",
        &messages,
        &[],
        &options_o1,
        "gpt-4o",
    );
    assert!(body_o1.get("temperature").is_none());
    assert_eq!(body_o1["max_completion_tokens"], 16_000);
    assert_eq!(body_o1["reasoning_effort"], "high");

    // Regular model (gpt-4o) preserves temperature and max_tokens
    let options_regular = CompletionOptions {
        model: "gpt-4o".to_string(),
        temperature: 0.3,
        max_tokens: 4096,
        system_instruction: None,
        thinking_budget: None,
        reasoning_effort: None,
    };

    let body_regular = OpenAiCompatibleProvider::build_request_body(
        "openai",
        &messages,
        &[],
        &options_regular,
        "gpt-4o",
    );
    assert!((body_regular["temperature"].as_f64().unwrap() - 0.3).abs() < 1e-4);
    assert_eq!(body_regular["max_tokens"], 4096);
    assert!(body_regular.get("reasoning_effort").is_none());
}

#[test]
fn test_openrouter_reasoning_and_thinking_payload() {
    let messages = vec![Message::user("Plan refactor")];

    // OpenRouter with Claude 3.7 and thinking budget
    let options_openrouter_claude = CompletionOptions {
        model: "anthropic/claude-3.7-sonnet".to_string(),
        temperature: 0.2,
        max_tokens: 32_000,
        system_instruction: None,
        thinking_budget: Some(16_000),
        reasoning_effort: None,
    };

    let body_claude = OpenAiCompatibleProvider::build_request_body(
        "openrouter",
        &messages,
        &[],
        &options_openrouter_claude,
        "anthropic/claude-3.7-sonnet",
    );
    assert_eq!(body_claude["thinking"]["type"], "enabled");
    assert_eq!(body_claude["thinking"]["budget_tokens"], 16_000);
    assert_eq!(body_claude["temperature"], 1.0);

    // OpenRouter with DeepSeek R1 and reasoning effort
    let options_r1 = CompletionOptions {
        model: "deepseek/deepseek-r1".to_string(),
        temperature: 0.6,
        max_tokens: 16_000,
        system_instruction: None,
        thinking_budget: None,
        reasoning_effort: Some("high".to_string()),
    };

    let body_r1 = OpenAiCompatibleProvider::build_request_body(
        "openrouter",
        &messages,
        &[],
        &options_r1,
        "deepseek/deepseek-r1",
    );
    assert_eq!(body_r1["reasoning"]["effort"], "high");
}

#[test]
fn test_provider_factory_and_config_support() {
    // 1. Factory recognizes "anthropic" and "claude"
    let p_anthropic = create_provider_with_base_url("anthropic", "test-key-1", None);
    assert!(p_anthropic.is_ok());
    let prov = p_anthropic.unwrap();
    assert_eq!(prov.name(), "anthropic");
    assert_eq!(prov.default_model(), ANTHROPIC_DEFAULT_MODEL);

    let p_claude = create_provider_with_base_url("claude", "test-key-2", None);
    assert!(p_claude.is_ok());
    assert_eq!(p_claude.unwrap().name(), "anthropic");

    // 2. Config resolves defaults correctly
    assert_eq!(
        Config::get_default_model_for_provider("anthropic"),
        ANTHROPIC_DEFAULT_MODEL
    );
    assert_eq!(
        Config::get_default_model_for_provider("claude"),
        ANTHROPIC_DEFAULT_MODEL
    );

    // 3. ProviderConfig thinking methods
    let mut config = ProviderConfig::default();
    assert!(!config.is_thinking_enabled());
    assert_eq!(config.effective_thinking_budget(), None);

    config.thinking_budget = Some(8192);
    assert!(config.is_thinking_enabled());
    assert_eq!(config.effective_thinking_budget(), Some(8192));

    // Under minimum threshold (1024) is treated as disabled
    config.thinking_budget = Some(500);
    assert!(!config.is_thinking_enabled());
    assert_eq!(config.effective_thinking_budget(), None);

    // 4. Constants
    assert_eq!(ANTHROPIC_BASE_URL, "https://api.anthropic.com/v1");
    assert_eq!(ANTHROPIC_VERSION_HEADER, "2023-06-01");
    assert_eq!(DEFAULT_THINKING_BUDGET_TOKENS, 16_000);
    assert_eq!(MIN_THINKING_BUDGET_TOKENS, 1024);
    assert_eq!(MAX_THINKING_BUDGET_TOKENS, 64_000);
}

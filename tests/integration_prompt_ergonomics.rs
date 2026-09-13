use minicode::agent::models::get_model_context_limit;
use minicode::agent::prompt::{
    sanitize_past_user_message, strip_thought_blocks, PromptBuilder, STATIC_SYSTEM_PROMPT,
};
use minicode::agent::types::Message;
use minicode::context::auto_compact::AutoCompactor;
use minicode::context::budget::ContextBudget;
use tempfile::tempdir;

#[test]
fn test_strip_thought_blocks_removes_various_tag_styles() {
    let input = "<thought>I should inspect Cargo.toml first.</thought>Here is the plan.";
    assert_eq!(strip_thought_blocks(input), "Here is the plan.");

    let deepseek_style = "<think>Calculating dependencies\nLine 2</think>Execution complete.";
    assert_eq!(strip_thought_blocks(deepseek_style), "Execution complete.");

    let capitalized = "<Thinking>Analyzing AST</Thinking>Ready.";
    assert_eq!(strip_thought_blocks(capitalized), "Ready.");

    let multi_thought = "<thought>Step 1</thought>Part A <reasoning>Step 2</reasoning>Part B";
    assert_eq!(strip_thought_blocks(multi_thought), "Part A Part B");

    let pure_thought = "<thought>Only internal reasoning here</thought>";
    assert_eq!(strip_thought_blocks(pure_thought), "");

    let unclosed = "prefix <thought>unclosed thought stream";
    assert_eq!(strip_thought_blocks(unclosed), "prefix");

    let clean = "Just normal response text without tags";
    assert_eq!(strip_thought_blocks(clean), clean);
}

#[test]
fn test_sanitize_past_user_message_strips_stale_workspace_context() {
    // New inverted format
    let inverted_msg = r#"<workspace_context>
  <context_budget used="5000" limit="128000" pct="3.9%" headroom="123000" cumulative="5000">
    3.9% (5000/128000 tokens) | Headroom: 123000 tokens | Session Total: 5000
    Note: HEALTHY
  </context_budget>
  <git_status branch="main" clean="true">
  </git_status>
</workspace_context>

<user_request>
Please add a unit test for HTTP client timeout
</user_request>"#;

    let sanitized = sanitize_past_user_message(inverted_msg);
    assert_eq!(
        sanitized, "Please add a unit test for HTTP client timeout",
        "Should unwrap <user_request> and discard stale <workspace_context>"
    );

    // Legacy format (user prompt first, workspace_context appended)
    let legacy_msg = r#"Please refactor the database connector

<workspace_context>
  <git_status branch="feature" clean="false">
  </git_status>
</workspace_context>"#;

    let sanitized_legacy = sanitize_past_user_message(legacy_msg);
    assert_eq!(
        sanitized_legacy, "Please refactor the database connector",
        "Should strip appended <workspace_context> from legacy turns"
    );

    // Plain message without workspace_context
    let plain = "Can you explain how the cache works?";
    assert_eq!(sanitize_past_user_message(plain), plain);
}

#[test]
fn test_recency_inverted_turn_assembly_structure() {
    let temp = tempdir().unwrap();
    let workspace = temp.path();

    let budget = ContextBudget::new(10_000, 128_000, 25_000);
    let recency_block =
        PromptBuilder::build_recency_context(workspace, None, &[], None, Some(&budget));

    let user_prompt = "Implement graceful shutdown";

    let assembled = format!(
        "{}\n\n<user_request>\n{}\n</user_request>",
        recency_block.trim(),
        user_prompt.trim()
    );

    // Verification: workspace_context must appear before user_request
    let ws_pos = assembled.find("<workspace_context>").unwrap();
    let req_pos = assembled.find("<user_request>").unwrap();
    assert!(
        ws_pos < req_pos,
        "<workspace_context> must precede <user_request> for recency inversion"
    );

    // user_request must be the final recency token at the tail
    assert!(assembled.ends_with("</user_request>"));
    assert!(assembled.contains("<user_request>\nImplement graceful shutdown\n</user_request>"));
}

#[test]
fn test_system_prompt_high_signal_and_few_shot() {
    // 1. Concrete 3-line patch_file few-shot example exists
    assert!(
        STATIC_SYSTEM_PROMPT.contains("# Example patch_file usage:"),
        "Must include few-shot patch_file section"
    );
    assert!(
        STATIC_SYSTEM_PROMPT.contains("patch_file(path=\"src/main.rs\""),
        "Must include concrete patch_file invocation"
    );
    assert!(
        STATIC_SYSTEM_PROMPT.contains("search_block="),
        "Must demonstrate search_block"
    );
    assert!(
        STATIC_SYSTEM_PROMPT.contains("replace_block="),
        "Must demonstrate replace_block"
    );

    // 2. Zero ghost tools in system prompt
    assert!(!STATIC_SYSTEM_PROMPT.contains("execute_dag"));
    assert!(!STATIC_SYSTEM_PROMPT.contains("repair_diagnostics"));
    assert!(!STATIC_SYSTEM_PROMPT.contains("audit_architecture"));
    assert!(!STATIC_SYSTEM_PROMPT.contains("hybrid_retrieve"));
    assert!(!STATIC_SYSTEM_PROMPT.contains("sandbox_exec"));
    assert!(!STATIC_SYSTEM_PROMPT.contains("begin_transaction"));

    // 3. Zero human CLI slash commands in system prompt
    assert!(!STATIC_SYSTEM_PROMPT.contains("/stack"));
    assert!(!STATIC_SYSTEM_PROMPT.contains("/plan"));
    assert!(!STATIC_SYSTEM_PROMPT.contains("/goal"));
    assert!(!STATIC_SYSTEM_PROMPT.contains("/review"));
    assert!(!STATIC_SYSTEM_PROMPT.contains("/map"));
    assert!(!STATIC_SYSTEM_PROMPT.contains("/explore"));
    assert!(!STATIC_SYSTEM_PROMPT.contains("/retrieve"));
    assert!(!STATIC_SYSTEM_PROMPT.contains("/route"));
    assert!(!STATIC_SYSTEM_PROMPT.contains("/quarantine"));
    assert!(!STATIC_SYSTEM_PROMPT.contains("/commit"));
    assert!(!STATIC_SYSTEM_PROMPT.contains("/arch"));

    // 4. Zero internal Rust struct leakage
    assert!(
        !STATIC_SYSTEM_PROMPT.contains("MinicodeError"),
        "Must not leak internal MinicodeError struct into LLM system prompt"
    );
}

#[test]
fn test_context_budget_no_unicode_progress_bar_in_prompt_block() {
    let budget = ContextBudget::new(45_000, 100_000, 90_000);
    let prompt_block = budget.to_prompt_block();

    // Must not contain visual terminal block characters that waste tokens
    assert!(
        !prompt_block.contains('█'),
        "Prompt block must not contain filled block character"
    );
    assert!(
        !prompt_block.contains('░'),
        "Prompt block must not contain empty block character"
    );

    // Must retain semantic stats
    assert!(prompt_block.contains("used=\"45000\""));
    assert!(prompt_block.contains("limit=\"100000\""));
    assert!(prompt_block.contains("pct=\"45.0%\""));
    assert!(prompt_block.contains("headroom=\"55000\""));
}

#[test]
fn test_auto_compactor_unblocked_on_small_message_count() {
    let mut compactor = AutoCompactor::new("ollama/qwen2.5-coder:1.5b")
        .unwrap()
        .with_custom_limit(500);

    let mut long_output = String::new();
    for i in 1..=100 {
        long_output.push_str(&format!("Compiler diagnostic log stream line {}\n", i));
    }

    // Only 4 messages (previously blocked by hard messages.len() <= 6)
    let mut msgs = vec![
        Message::user("Please build the crate"),
        Message::tool_result("tc1", "exec_cmd", long_output.clone()),
        Message::assistant("Analyzing compiler errors"),
        Message::user("What failed?"),
    ];

    let metrics = compactor.compact(&mut msgs, 1);
    assert!(
        metrics.is_some(),
        "Compactor should not be blocked on 4 messages when tokens exceed threshold on an 8k model"
    );
    let m = metrics.unwrap();
    assert!(
        m.tokens_after < m.tokens_before,
        "Compaction must reduce token volume"
    );
    assert_eq!(m.tier, 1);
}

#[test]
fn test_model_context_limit_ollama_and_local() {
    // Small local models (<7B) clamped to 8,192
    assert_eq!(get_model_context_limit("ollama/qwen2.5-coder:1.5b"), 8_192);
    assert_eq!(get_model_context_limit("qwen2.5-coder:1.5b"), 8_192);
    assert_eq!(get_model_context_limit("llama3.2:3b"), 8_192);
    assert_eq!(get_model_context_limit("localhost:11434/model"), 8_192);
    assert_eq!(get_model_context_limit("lmstudio/deepseek-coder"), 8_192);

    // Mid-size local models (7B/8B/14B) clamped to 16,384
    assert_eq!(get_model_context_limit("ollama/qwen2.5-coder:7b"), 16_384);
    assert_eq!(get_model_context_limit("llama3.1:8b"), 16_384);
    assert_eq!(get_model_context_limit("qwen2.5-coder:14b"), 16_384);

    // Frontier cloud models retain standard large limits
    assert_eq!(get_model_context_limit("gpt-4o"), 128_000);
    assert_eq!(get_model_context_limit("claude-3-5-sonnet"), 200_000);
    assert_eq!(get_model_context_limit("gemini-2.0-flash"), 1_000_000);
}

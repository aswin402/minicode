use minicode::agent::models::get_model_context_limit;
use minicode::agent::prompt::PromptBuilder;
use minicode::agent::types::{AgentEvent, Message, Role};
use minicode::context::auto_compact::{AutoCompactor, MemoryAnchor};

#[test]
fn test_model_aware_context_limits() {
    assert_eq!(get_model_context_limit("gemini-2.0-flash"), 1_000_000);
    assert_eq!(get_model_context_limit("claude-3-7-sonnet"), 200_000);
    assert_eq!(get_model_context_limit("gpt-4o"), 128_000);
    assert_eq!(get_model_context_limit("liquid/lfm-2.5-2.6b:free"), 65_536);
    assert_eq!(
        get_model_context_limit("cohere/north-mini-code:free"),
        65_536
    );
    assert_eq!(get_model_context_limit("google/gemma-2-9b"), 8_192);
}

#[test]
fn test_turn_summary_extraction_and_markdown() {
    let msgs = vec![
        Message::user("Please use tokio async runtime and do not use unwrap"),
        Message::tool_result(
            "call_1",
            "read_file",
            "File Content (src/main.rs):\nfn main() {}\n",
        ),
        Message::assistant(
            "I will implement this.\n- Decision: Chose pure Rust networking with rustls",
        ),
        Message::tool_result("call_2", "write_file", "File written to src/agent/loop.rs"),
        Message::user("Check compilation"),
        Message::tool_result(
            "call_3",
            "exec_cmd",
            "error[E0432]: unresolved import `crate::context`",
        ),
    ];

    let summary = AutoCompactor::extract_turn_summary(&msgs, 1, 3);
    assert!(summary.files_read.contains(&"src/main.rs".to_string()));
    assert!(summary
        .decisions
        .iter()
        .any(|d| d.contains("rustls") || d.contains("tokio")));
    assert!(summary
        .errors_resolved
        .iter()
        .any(|e| e.contains("error[E0432]")));

    let md = summary.to_markdown();
    assert!(md.contains("📋 **Context Summary (Turns 1-3)**"));
    assert!(md.contains("Files Read"));
    assert!(md.contains("src/main.rs"));
}

#[test]
fn test_memory_anchor_prompt_injection() {
    let mut anchor = MemoryAnchor::default();
    anchor.working_context = Some("Implement Tiered Context Auto-Compactor".to_string());
    anchor
        .key_decisions
        .push("Use algorithmic extraction without extra LLM cost".to_string());
    anchor.file_state.insert(
        "src/context/auto_compact.rs".to_string(),
        "modified".to_string(),
    );
    anchor.unresolved_errors.push("None".to_string());

    let temp_dir = std::env::temp_dir();
    let prompt = PromptBuilder::build_system_prompt_with_anchor(
        &temp_dir,
        Some("Turn instructions"),
        Some(&anchor.to_prompt_block()),
    );

    assert!(prompt.contains("# Session Memory Anchor (Persistent Context):"));
    assert!(prompt.contains("Active Goal"));
    assert!(prompt.contains("Implement Tiered Context Auto-Compactor"));
    assert!(prompt.contains("algorithmic extraction without extra LLM cost"));
    assert!(prompt.contains("src/context/auto_compact.rs"));
}

#[test]
fn test_tier1_observation_masking_reduces_tokens() {
    let mut compactor = AutoCompactor::new("liquid/lfm-2.5-2.6b:free")
        .expect("create compactor")
        .with_custom_limit(500);

    let verbose_log = (1..=60)
        .map(|i| format!("Log entry #{} detailed compiler output trace\n", i))
        .collect::<String>();

    let mut msgs = vec![
        Message::user("Run long build 1"),
        Message::tool_result("tc1", "exec_cmd", verbose_log),
        Message::assistant("Recent 1"),
        Message::user("Recent 2"),
        Message::assistant("Recent 3"),
        Message::user("Recent 4"),
        Message::assistant("Recent 5"),
        Message::user("Recent 6"),
    ];

    let metrics = compactor.compact(&mut msgs, 3);
    assert!(metrics.is_some(), "Expected Tier 1 compaction to trigger");
    let m = metrics.unwrap();
    assert_eq!(m.tier, 1);
    assert!(m.tokens_after < m.tokens_before);
    assert!(m.savings_percent > 0);
}

#[test]
fn test_tier2_turn_group_summarization_collapses_history() {
    let mut compactor = AutoCompactor::new("liquid/lfm-2.5-2.6b:free")
        .expect("create compactor")
        .with_custom_limit(300);

    let verbose_code = (1..=40)
        .map(|i| format!("pub fn function_{}() -> usize {{ {} }}\n", i, i))
        .collect::<String>();

    let mut msgs = vec![
        Message::user("Please read main.rs and decide on architecture"),
        Message::tool_result(
            "tc1",
            "read_file",
            format!("File Content (src/main.rs):\n{}", verbose_code),
        ),
        Message::assistant("I analyzed main.rs.\n- Decision: Use Tokio async engine"),
        Message::tool_result(
            "tc2",
            "write_file",
            "Successfully written src/agent/loop.rs",
        ),
        Message::user("Check compiler errors"),
        Message::tool_result(
            "tc3",
            "exec_cmd",
            "error[E0308]: mismatched types in loop.rs:50",
        ),
        // Recent preserved messages (6 messages)
        Message::user("Recent turn user"),
        Message::assistant("Recent turn assistant"),
        Message::user("Recent turn 2 user"),
        Message::assistant("Recent turn 2 assistant"),
        Message::user("Recent turn 3 user"),
        Message::assistant("Recent turn 3 assistant"),
    ];

    let initial_msg_count = msgs.len();
    let metrics = compactor.compact(&mut msgs, 4);

    assert!(metrics.is_some(), "Expected Tier 2 compaction to trigger");
    let m = metrics.unwrap();
    assert!(m.tier >= 2);
    assert!(msgs.len() < initial_msg_count);

    // The first message should now be a system summary
    assert_eq!(msgs[0].role, Role::System);
    assert!(msgs[0].content.contains("Context Summary"));
    assert!(msgs[0].content.contains("src/main.rs"));

    // Memory anchor should have captured the decision
    assert!(compactor
        .anchor()
        .key_decisions
        .iter()
        .any(|d| d.contains("Tokio") || d.contains("architecture")));
}

#[test]
fn test_tier3_aggressive_pruning_with_persistent_anchor() {
    let mut compactor = AutoCompactor::new("google/gemma-2-9b")
        .expect("create compactor")
        .with_custom_limit(300);

    let huge_content = "extremely large content line that takes many tokens ".repeat(50);

    let mut msgs = vec![
        Message::user("Huge turn 1"),
        Message::assistant(huge_content.clone()),
        Message::user("Huge turn 2"),
        Message::assistant(huge_content.clone()),
        Message::user("Huge turn 3"),
        Message::assistant(huge_content.clone()),
        Message::user("Huge turn 4"),
        Message::assistant(huge_content.clone()),
        Message::user("Recent 1"),
        Message::assistant("Recent 2"),
    ];

    let metrics = compactor.compact(&mut msgs, 5);
    assert!(metrics.is_some());
    let m = metrics.unwrap();
    assert_eq!(m.tier, 3);
    assert!(m.tokens_after < m.tokens_before);
    assert!(msgs.len() <= 6);
}

#[test]
fn test_agent_event_context_compacted_serialization() {
    let event = AgentEvent::ContextCompacted {
        turn_id: 3,
        tier: 2,
        turns_summarized: 4,
        tokens_before: 52_000,
        tokens_after: 18_500,
        savings_percent: 64,
    };

    let serialized = serde_json::to_string(&event).expect("serialize event");
    assert!(serialized.contains("\"event\":\"context_compacted\""));
    assert!(serialized.contains("\"tier\":2"));
    assert!(serialized.contains("\"turns_summarized\":4"));
    assert!(serialized.contains("\"tokens_before\":52000"));
    assert!(serialized.contains("\"tokens_after\":18500"));
    assert!(serialized.contains("\"savings_percent\":64"));

    let deserialized: AgentEvent = serde_json::from_str(&serialized).expect("deserialize event");
    assert_eq!(event, deserialized);
}

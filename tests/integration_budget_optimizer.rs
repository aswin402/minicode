use minicode::agent::types::Message;
use minicode::context::budget_optimizer::TokenBudgetOptimizer;
use serde_json::json;
use tempfile::tempdir;

#[test]
fn test_forecast_from_history() {
    let turn_tokens = vec![400, 600, 800, 1000];
    let context_limit = 128_000;
    let current_tokens = 2800;

    let forecast =
        TokenBudgetOptimizer::forecast_from_history(&turn_tokens, context_limit, current_tokens);

    assert_eq!(forecast.total_turns, 4);
    assert_eq!(forecast.avg_tokens_per_turn, 700);
    assert_eq!(forecast.recent_burn_velocity, 800); // (600 + 800 + 1000) / 3
    assert_eq!(forecast.projected_tokens_next_5_turns, 4000);
    assert!(forecast.turns_until_exhaustion > 100);
}

#[test]
fn test_analyze_messages_and_recommendations() {
    let mut messages = Vec::new();

    messages.push(Message::user("Find all test files and run them"));

    // Add a long tool observation
    let long_tool_output = (0..50)
        .map(|i| format!("Line {}: test passing ok", i))
        .collect::<Vec<_>>()
        .join("\n");

    messages.push(Message::tool_result("call_1", "run_test", long_tool_output));

    let report = TokenBudgetOptimizer::analyze_messages(&messages, "gpt-4o");

    assert_eq!(report.model_name, "gpt-4o");
    assert!(report.current_tokens > 0);
    assert!(!report.actions.is_empty());

    let has_masking = report
        .actions
        .iter()
        .any(|a| a.title == "Observation Masking");
    assert!(has_masking);
}

#[tokio::test]
async fn test_optimize_token_budget_tool_dispatch() {
    let dir = tempdir().expect("tempdir");

    let args = json!({
        "model_name": "claude-3-5-sonnet"
    });

    let res = minicode::tools::registry::context_tools::dispatch(
        "optimize_token_budget",
        &args,
        dir.path(),
    )
    .await;

    assert!(res.is_some());
    let output = res.unwrap().expect("tool execution success");
    assert!(output.contains("Predictive Multi-Turn Token Budget Report"));
    assert!(output.contains("claude-3-5-sonnet"));
}

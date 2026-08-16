use minicode::agent::sequential_thinking::{ThinkingSession, ThoughtNode};
use minicode::tools::ToolRegistry;
use serde_json::json;
use std::path::PathBuf;

#[test]
fn test_sequential_thinking_session_branching_and_revision() {
    let mut session = ThinkingSession::new();

    let r1 = session
        .add_thought(ThoughtNode {
            thought_number: 1,
            total_thoughts: 3,
            thought: "Analyze architectural decoupling options".to_string(),
            is_revision: false,
            revises_thought: None,
            branch_from_thought: None,
            branch_id: None,
            needs_more_thoughts: true,
            score: Some(0.85),
        })
        .unwrap();

    assert_eq!(r1.thought_number, 1);
    assert_eq!(r1.current_branch, "main");

    let r2 = session
        .add_thought(ThoughtNode {
            thought_number: 2,
            total_thoughts: 3,
            thought: "Option B: Use trait-based dynamic dispatch with async_trait".to_string(),
            is_revision: false,
            revises_thought: None,
            branch_from_thought: Some(1),
            branch_id: Some("trait_dispatch".to_string()),
            needs_more_thoughts: false,
            score: Some(0.95),
        })
        .unwrap();

    assert_eq!(r2.total_branches, 2);
    assert!(r2.is_complete);
}

#[tokio::test]
async fn test_sequential_thinking_tool_dispatch() {
    ThinkingSession::reset_session();
    let ws = PathBuf::from("/workspace");

    let res = ToolRegistry::dispatch(
        &ws,
        "call_think_1",
        "sequential_thinking",
        &json!({
            "thought_number": 1,
            "total_thoughts": 2,
            "thought": "Evaluate lock-free ring buffer vs crossbeam channel",
            "needs_more_thoughts": true,
            "score": 0.9
        }),
        None,
        1,
    )
    .await;

    assert!(res.success);
    assert!(res.output.contains("Thought Step 1/2"));
    assert!(res.output.contains("Reasoning in progress"));

    let res2 = ToolRegistry::dispatch(
        &ws,
        "call_think_2",
        "sequential_thinking",
        &json!({
            "thought_number": 2,
            "total_thoughts": 2,
            "thought": "Conclusion: Crossbeam provides better ergonomics and bounded memory guarantees",
            "needs_more_thoughts": false,
            "score": 0.99
        }),
        None,
        1,
    )
    .await;

    assert!(res2.success);
    assert!(res2.output.contains("Reasoning Completed"));
    assert!(res2.output.contains("Crossbeam"));
}

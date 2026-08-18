use minicode::agent::hypothesis::{BranchStatus, HypothesisEngine};
use minicode::tools::ToolRegistry;
use serde_json::json;
use tempfile::tempdir;

#[tokio::test]
async fn test_hypothesis_tree_branch_creation_and_winner_selection() {
    let dir = tempdir().unwrap();
    let ws = dir.path();

    let hypotheses = vec![
        "Approach 1: Implement iterative DFS traversal".to_string(),
        "Approach 2: Implement recursive BFS traversal".to_string(),
    ];

    let session = HypothesisEngine::create_branches(ws, &hypotheses)
        .await
        .unwrap();
    assert_eq!(session.branches.len(), 2);
    assert_eq!(session.branches[0].status, BranchStatus::Pending);

    let evaluated = HypothesisEngine::evaluate_branch(ws, &session.branches[0].id)
        .await
        .unwrap();
    assert!(evaluated.fitness_score >= 0.0);

    let winner = HypothesisEngine::select_best_branch(ws).await.unwrap();
    assert_eq!(winner.status, BranchStatus::Selected);
}

#[tokio::test]
async fn test_hypothesis_tools_dispatch() {
    let dir = tempdir().unwrap();
    let ws = dir.path();

    // 1. Dispatch explore_hypotheses
    let explore_args = json!({
        "hypotheses": [
            "Option A: Single-threaded event loop",
            "Option B: Multi-threaded worker pool"
        ]
    });
    let explore_res = ToolRegistry::dispatch(
        ws,
        "call_hyp1",
        "explore_hypotheses",
        &explore_args,
        None,
        1,
    )
    .await;
    assert!(explore_res.success);
    assert!(explore_res
        .output
        .contains("Spawned 2 speculative branches"));

    // 2. Dispatch select_best_branch
    let select_res =
        ToolRegistry::dispatch(ws, "call_hyp2", "select_best_branch", &json!({}), None, 1).await;
    assert!(select_res.success);
    assert!(select_res.output.contains("Selected Winning Branch"));
}

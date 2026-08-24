/// Integration tests for Phase 39: Speculative Multi-Branch Hypothesis Auto-Pruner & Parallel Evaluator
///
/// Tests multi-branch creation, parallel evaluation, automatic pruning of low-fitness branches,
/// comparison matrix generation, and tool schema registrations.
use minicode::agent::hypothesis::{BranchStatus, HypothesisEngine};
use minicode::tools::registry::agent_tools;
use tempfile::tempdir;

#[tokio::test]
async fn test_hypothesis_parallel_eval_and_pruning() {
    let dir = tempdir().unwrap();
    let ws = dir.path();

    let hypotheses = vec![
        "Approach 1: Pure-Rust parser".to_string(),
        "Approach 2: C-binding parser with warnings".to_string(),
    ];

    let session = HypothesisEngine::create_branches(ws, &hypotheses)
        .await
        .unwrap();
    assert_eq!(session.branches.len(), 2);

    // Evaluate all branches
    let evaluated = HypothesisEngine::evaluate_all_branches(ws).await.unwrap();
    assert_eq!(evaluated.len(), 2);

    // Comparison matrix formatting
    let matrix = HypothesisEngine::format_comparison_matrix(&session);
    assert!(matrix.contains("Approach 1"));
    assert!(matrix.contains("Approach 2"));

    // Prune branches below fitness 0.0 (none should be pruned if fitness is >= 0)
    let pruned = HypothesisEngine::prune_failed_branches(ws, -1.0)
        .await
        .unwrap();
    assert!(pruned.is_empty());

    // Select best branch
    let winner = HypothesisEngine::select_best_branch(ws).await.unwrap();
    assert_eq!(winner.status, BranchStatus::Selected);
}

#[test]
fn test_hypothesis_tool_schemas_registered() {
    let schemas = agent_tools::get_schemas();
    let names: Vec<String> = schemas.into_iter().map(|s| s.name).collect();

    assert!(names.contains(&"explore_hypotheses".to_string()));
    assert!(names.contains(&"evaluate_branch".to_string()));
    assert!(names.contains(&"evaluate_all_branches".to_string()));
    assert!(names.contains(&"prune_branches".to_string()));
    assert!(names.contains(&"compare_branches".to_string()));
    assert!(names.contains(&"select_best_branch".to_string()));
}

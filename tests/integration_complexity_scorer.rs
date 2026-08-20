use minicode::agent::complexity::TaskComplexityScorer;
use minicode::tools::ToolRegistry;
use serde_json::json;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_complexity_scorer_assesses_task_and_recommends_stages() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    let src = root.join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(src.join("auth.rs"), "pub fn authenticate() {}\n").unwrap();
    fs::write(src.join("db.rs"), "pub fn query() {}\n").unwrap();

    let score = TaskComplexityScorer::score_task(
        root,
        "Refactor database security and auth module architecture",
    )
    .unwrap();

    assert!(score.score >= 5);
    assert_ne!(score.risk_level, "LOW");
    assert!(!score.subtask_recommendations.is_empty());

    let md = score.format_markdown();
    assert!(md.contains("Task Complexity Assessment"));
    assert!(md.contains("Recommended Task Decomposition"));
}

#[tokio::test]
async fn test_score_task_complexity_tool_dispatch() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    let args = json!({
        "task": "Add a new helper function in utils.rs"
    });

    let res = ToolRegistry::dispatch(
        root,
        "call_score_1",
        "score_task_complexity",
        &args,
        None,
        1,
    )
    .await;

    assert!(res.success);
    assert!(res.output.contains("Task Complexity Assessment"));
}

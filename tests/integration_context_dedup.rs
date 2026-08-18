use minicode::agent::types::Message;
use minicode::context::dedup::ObservationDeduplicator;
use minicode::tools::ToolRegistry;
use serde_json::json;
use tempfile::tempdir;

#[test]
fn test_observation_deduplicator_collapses_repeats() {
    let large_file_content = (0..30)
        .map(|i| format!("pub fn handler_{}() {{ println!(\"line {}\"); }}", i, i))
        .collect::<Vec<_>>()
        .join("\n");

    let mut messages = vec![
        Message::user("Read database module"),
        Message::tool_result(
            "call_1",
            "read_file",
            format!("File Content (src/db.rs):\n{}", large_file_content),
        ),
        Message::user("Read database module again"),
        Message::tool_result(
            "call_2",
            "read_file",
            format!("File Content (src/db.rs):\n{}", large_file_content),
        ),
    ];

    let stats = ObservationDeduplicator::deduplicate_messages(&mut messages);
    assert_eq!(stats.redundant_reads_collapsed, 1);
    assert!(stats.characters_saved > 200);
    assert!(messages[3].content.contains("Observation Deduplicated"));
}

#[tokio::test]
async fn test_prune_context_tool_dispatch() {
    let dir = tempdir().unwrap();
    let ws = dir.path();

    let res = ToolRegistry::dispatch(ws, "call_prune", "prune_context", &json!({}), None, 1).await;
    assert!(res.success);
    assert!(res.output.contains("Multi-turn observation deduplication"));
}

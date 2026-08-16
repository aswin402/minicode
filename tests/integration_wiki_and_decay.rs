use minicode::context::decay::{CognitiveMemoryManager, MemoryScope};
use minicode::tools::ToolRegistry;
use serde_json::json;
use tempfile::tempdir;

#[test]
fn test_cognitive_decay_retention_and_pruning() {
    let mut manager = CognitiveMemoryManager::new();

    // 1. Insert permanent fact
    manager.insert_or_reinforce(
        "rust_rule",
        MemoryScope::Permanent,
        "Never use unwrap in non-test code",
    );

    // 2. Insert transient fact
    manager.insert_or_reinforce(
        "temp_debug",
        MemoryScope::Transient,
        "Checked port 8080 - active",
    );

    let start_time = manager.facts[0].created_at;
    let block1 = manager.format_prompt_block(start_time, 0.4);
    assert!(block1.contains("Never use unwrap"));
    assert!(block1.contains("Checked port 8080"));

    // 3. Fast forward time by 10 hours
    let future_time = start_time + (10 * 3600);
    manager.prune_decayed(future_time, 0.4);

    let block2 = manager.format_prompt_block(future_time, 0.4);
    assert!(block2.contains("Never use unwrap"));
    assert!(!block2.contains("Checked port 8080")); // Pruned due to exponential half-life decay
}

#[tokio::test]
async fn test_wiki_tool_dispatch_lifecycle() {
    let dir = tempdir().unwrap();
    let ws = dir.path().to_path_buf();

    // 1. Write knowledge entry via dispatch
    let res_write = ToolRegistry::dispatch(
        &ws,
        "call_wiki_w",
        "wiki_write",
        &json!({
            "topic": "tokio-broadcast-pattern",
            "title": "Tokio Broadcast Event Streaming",
            "content": "All internal agent events stream over a 256-buffered tokio broadcast channel.",
            "tags": ["tokio", "async", "events"],
            "references": ["src/agent/loop.rs"]
        }),
        None,
        1,
    )
    .await;

    assert!(res_write.success);
    assert!(res_write.output.contains("tokio-broadcast-pattern.md"));

    // 2. Read knowledge entry via dispatch
    let res_read = ToolRegistry::dispatch(
        &ws,
        "call_wiki_r",
        "wiki_read",
        &json!({
            "topic": "tokio-broadcast-pattern"
        }),
        None,
        1,
    )
    .await;

    assert!(res_read.success);
    assert!(res_read.output.contains("Tokio Broadcast Event Streaming"));
    assert!(res_read.output.contains("256-buffered"));

    // 3. Search knowledge entries via dispatch
    let res_search = ToolRegistry::dispatch(
        &ws,
        "call_wiki_s",
        "wiki_search",
        &json!({
            "query": "broadcast"
        }),
        None,
        1,
    )
    .await;

    assert!(res_search.success);
    assert!(res_search
        .output
        .contains("Tokio Broadcast Event Streaming"));
    assert!(res_search.output.contains("tokio-broadcast-pattern"));
}

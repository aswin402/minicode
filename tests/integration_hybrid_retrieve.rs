use minicode::context::episodic::EpisodicMemory;
use minicode::context::fusion::KnowledgeFusionEngine;
use minicode::context::wiki::WikiManager;
use serde_json::json;
use std::fs;

#[test]
fn test_hybrid_retrieve_basic_code_fusion() {
    let temp_dir = tempfile::tempdir().unwrap();
    let src_dir = temp_dir.path().join("src");
    fs::create_dir_all(&src_dir).unwrap();

    let calc_file = src_dir.join("calc.rs");
    fs::write(
        &calc_file,
        r#"
pub struct AdvancedCalculator {
    precision: u32,
}

impl AdvancedCalculator {
    pub fn new(precision: u32) -> Self {
        Self { precision }
    }

    pub fn compute_sum(&self, a: i64, b: i64) -> i64 {
        a + b
    }
}
"#,
    )
    .unwrap();

    let bundle = KnowledgeFusionEngine::retrieve(
        temp_dir.path(),
        "AdvancedCalculator",
        5,
        false,
        false,
        false,
    )
    .unwrap();

    assert_eq!(bundle.query, "AdvancedCalculator");
    assert!(!bundle.primary_code_matches.is_empty());

    let top_match = &bundle.primary_code_matches[0];
    assert!(top_match.file_path.contains("calc.rs"));
    assert!(
        top_match.symbol_name.as_deref() == Some("AdvancedCalculator")
            || top_match.snippet.contains("AdvancedCalculator")
    );
    assert!(top_match.fused_score > 0.0);
}

#[test]
fn test_hybrid_retrieve_with_wiki() {
    let temp_dir = tempfile::tempdir().unwrap();
    let src_dir = temp_dir.path().join("src");
    fs::create_dir_all(&src_dir).unwrap();

    fs::write(
        src_dir.join("auth.rs"),
        "pub fn verify_jwt_token() -> bool { true }",
    )
    .unwrap();

    // Create a wiki entry
    let tags = vec!["security".to_string(), "auth".to_string()];
    let refs = vec!["src/auth.rs".to_string()];
    WikiManager::write_entry(
        temp_dir.path(),
        "authentication",
        "Authentication & JWT Policy",
        "All incoming requests must provide a cryptographically signed JWT token with valid expiry.",
        &tags,
        &refs,
    )
    .unwrap();

    let bundle = KnowledgeFusionEngine::retrieve(
        temp_dir.path(),
        "JWT token authentication",
        5,
        false,
        true,
        false,
    )
    .unwrap();

    assert!(!bundle.wiki_insights.is_empty());
    let wiki = &bundle.wiki_insights[0];
    assert_eq!(wiki.topic, "authentication");
    assert!(wiki.title.contains("Authentication"));
    assert!(wiki.relevance_score > 0.0);
}

#[test]
fn test_hybrid_retrieve_with_episodic_memory() {
    let temp_dir = tempfile::tempdir().unwrap();
    let src_dir = temp_dir.path().join("src");
    fs::create_dir_all(&src_dir).unwrap();

    fs::write(
        src_dir.join("cache.rs"),
        "pub struct LruCache; impl LruCache { pub fn purge(&self) {} }",
    )
    .unwrap();

    // Create and persist episodic memory
    let mut mem = EpisodicMemory::new();
    mem.record_episode(
        "Resolved Memory Leak in Cache Purge",
        "Fixed circular reference during LRU cache eviction by switching to Weak pointers.",
        vec!["cache".to_string(), "memory-leak".to_string()],
        vec!["src/cache.rs".to_string()],
        "session-100",
    );
    mem.save(temp_dir.path()).unwrap();

    let bundle = KnowledgeFusionEngine::retrieve(
        temp_dir.path(),
        "cache memory leak eviction",
        5,
        false,
        false,
        true,
    )
    .unwrap();

    assert!(!bundle.episodic_memories.is_empty());
    let ep = &bundle.episodic_memories[0];
    assert!(ep.title.contains("Memory Leak"));
    assert!(ep.summary.contains("LRU cache"));
    assert!(ep.relevance_score > 0.0);
}

#[test]
fn test_hybrid_retrieve_filters_toggle() {
    let temp_dir = tempfile::tempdir().unwrap();

    let bundle =
        KnowledgeFusionEngine::retrieve(temp_dir.path(), "general test", 5, false, false, false)
            .unwrap();

    assert!(bundle.call_graph_context.is_empty());
    assert!(bundle.wiki_insights.is_empty());
    assert!(bundle.episodic_memories.is_empty());
}

#[tokio::test]
async fn test_tool_registry_hybrid_retrieve_dispatch() {
    let temp_dir = tempfile::tempdir().unwrap();
    let src_dir = temp_dir.path().join("src");
    fs::create_dir_all(&src_dir).unwrap();

    fs::write(
        src_dir.join("lib.rs"),
        "pub fn initialize_engine() -> bool { true }",
    )
    .unwrap();

    let args = json!({
        "query": "initialize engine",
        "limit": 3,
        "include_graph": true,
        "include_wiki": true,
        "include_memory": true
    });

    let opt_res = minicode::tools::registry::context_tools::dispatch(
        "hybrid_retrieve",
        &args,
        temp_dir.path(),
    )
    .await;

    assert!(opt_res.is_some());
    let res_str = opt_res.unwrap().unwrap();
    assert!(res_str.contains("Multi-Modal Knowledge Retrieval"));
    assert!(res_str.contains("initialize engine"));
}

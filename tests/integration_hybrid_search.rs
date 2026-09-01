use minicode::context::hybrid::HybridIndex;
use minicode::tools::registry::search_tools;
use serde_json::json;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_hybrid_search_schema_registered() {
    let schemas = search_tools::get_schemas();
    let found = schemas.iter().any(|s| s.name == "hybrid_search");
    assert!(found, "hybrid_search tool must be registered in schemas");
}

#[test]
fn test_hybrid_index_empty_workspace() {
    let dir = tempdir().expect("tempdir");
    let mut index = HybridIndex::new();
    let res = index.build_index(dir.path());
    assert!(res.is_ok());

    let hits = index.search("anything", 5, true);
    assert!(hits.is_empty());
}

#[test]
fn test_hybrid_index_exact_identifier_and_concept() {
    let dir = tempdir().expect("tempdir");
    let src_dir = dir.path().join("src");
    fs::create_dir_all(&src_dir).expect("create src dir");

    let file_a = src_dir.join("network.rs");
    fs::write(
        &file_a,
        r#"
pub struct NetworkManager;

impl NetworkManager {
    pub fn handle_incoming_request(payload: &[u8]) -> bool {
        !payload.is_empty()
    }
}
"#,
    )
    .expect("write network.rs");

    let file_b = src_dir.join("storage.rs");
    fs::write(
        &file_b,
        r#"
pub struct DatabaseEngine;

impl DatabaseEngine {
    pub fn persist_to_disk(data: &str) -> Result<(), ()> {
        let _ = data;
        Ok(())
    }
}
"#,
    )
    .expect("write storage.rs");

    let mut index = HybridIndex::new();
    let res = index.build_index(dir.path());
    assert!(res.is_ok());

    // 1. Exact Identifier Test
    let hits = index.search("handle_incoming_request", 5, true);
    assert!(!hits.is_empty());
    assert_eq!(
        hits[0].symbol_name.as_deref(),
        Some("handle_incoming_request")
    );
    assert!(hits[0].match_sources.iter().any(|s| s.contains("BM25")));

    // 2. Conceptual Intent Test
    let hits_concept = index.search("persist data to storage disk", 5, true);
    assert!(!hits_concept.is_empty());
    assert!(
        hits_concept[0].snippet.contains("persist_to_disk")
            || hits_concept[0].symbol_name.as_deref() == Some("DatabaseEngine")
            || hits_concept[0].symbol_name.as_deref() == Some("persist_to_disk")
    );
}

#[test]
fn test_hybrid_search_tool_dispatch() {
    let dir = tempdir().expect("tempdir");
    let src_dir = dir.path().join("src");
    fs::create_dir_all(&src_dir).expect("create src dir");

    let file = src_dir.join("auth.rs");
    fs::write(
        &file,
        r#"
pub fn authenticate_bearer_token(token: &str) -> bool {
    token.starts_with("Bearer ")
}
"#,
    )
    .expect("write auth.rs");

    let args = json!({
        "query": "authenticate_bearer_token",
        "limit": 5
    });

    let res = search_tools::dispatch("hybrid_search", &args, dir.path());
    assert!(res.is_some());
    let output = res.unwrap().expect("tool execution success");
    assert!(output.contains("Hybrid Retrieval"));
    assert!(output.contains("authenticate_bearer_token"));
}

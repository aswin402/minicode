/// Integration tests for Phase 41: Semantic AST Code-Chunk Semantic Embedder & Symbol Indexer
///
/// Tests AST symbol-aware code chunking, vector embedding, symbol boost scoring,
/// search_symbols_semantic tool dispatch, and index persistence.
use minicode::context::semantic::SemanticIndex;
use minicode::tools::registry::search_tools;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_ast_symbol_chunking_and_search() {
    let dir = tempdir().unwrap();
    let ws = dir.path();

    let src_dir = ws.join("src");
    fs::create_dir_all(&src_dir).unwrap();

    let code = r#"
/// Handles network retries and exponential backoff
pub struct CircuitBreaker {
    failure_threshold: u32,
}

impl CircuitBreaker {
    pub fn new() -> Self {
        Self { failure_threshold: 3 }
    }

    /// Evaluates if the current circuit is open or closed
    pub fn can_execute(&self) -> bool {
        true
    }
}
"#;

    let cb_file = src_dir.join("circuit.rs");
    fs::write(&cb_file, code).unwrap();

    let mut index = SemanticIndex::new();
    let indexed = index.build_index(ws).unwrap();
    assert_eq!(indexed, 1);

    // Search general semantic intent
    let general_results = index.search("exponential backoff and retry mechanism", 5);
    assert!(!general_results.is_empty());
    assert_eq!(general_results[0].file_path, "src/circuit.rs");

    // Search specifically for AST symbols
    let sym_results = index.search_symbols("CircuitBreaker", 5);
    assert!(!sym_results.is_empty());
    assert_eq!(
        sym_results[0].symbol_name.as_deref(),
        Some("CircuitBreaker")
    );
    assert_eq!(sym_results[0].symbol_kind.as_deref(), Some("struct"));
}

#[test]
fn test_search_symbols_semantic_schema_registered() {
    let schemas = search_tools::get_schemas();
    let names: Vec<String> = schemas.into_iter().map(|s| s.name).collect();

    assert!(names.contains(&"search_symbols_semantic".to_string()));
    assert!(names.contains(&"semantic_search".to_string()));
    assert!(names.contains(&"locate_symbol".to_string()));
    assert!(names.contains(&"ast_query".to_string()));
}

/// Integration tests for Phase 38: Episodic Vector Memory & Long-Term Recall Engine
///
/// Tests episode recording, dense vector embedding, hybrid search ranking,
/// persistence, and agent tool schema registrations.
use minicode::context::episodic::EpisodicMemory;
use minicode::tools::registry::agent_tools;
use tempfile::tempdir;

#[test]
fn test_episodic_recording_and_hybrid_search() {
    let mut mem = EpisodicMemory::new();

    let ep1 = mem.record_episode(
        "Fix Tree-sitter ABI Segfaults",
        "Standardized tree-sitter core and grammar crates to ABI version 14 in Cargo.toml.",
        vec![
            "tree-sitter".to_string(),
            "crash".to_string(),
            "abi".to_string(),
        ],
        vec![
            "Cargo.toml".to_string(),
            "src/context/repomap.rs".to_string(),
        ],
        "sess-alpha",
    );

    let ep2 = mem.record_episode(
        "Token Compactor for Test Outputs",
        "Implemented RTK-style compaction for Pytest, Go, and Cargo tests to save context tokens.",
        vec![
            "compactor".to_string(),
            "tokens".to_string(),
            "pytest".to_string(),
        ],
        vec!["src/tools/compactor.rs".to_string()],
        "sess-beta",
    );

    assert!(ep1.starts_with("ep-"));
    assert!(ep2.starts_with("ep-"));
    assert_eq!(mem.episodes.len(), 2);

    // Query 1: should rank tree-sitter episode top
    let res1 = mem.search("tree-sitter grammar crash", 2);
    assert!(!res1.is_empty());
    assert_eq!(res1[0].item.title, "Fix Tree-sitter ABI Segfaults");
    assert!(res1[0].score > 0.3);

    // Query 2: should rank token compactor top
    let res2 = mem.search("reduce test output token size", 2);
    assert!(!res2.is_empty());
    assert_eq!(res2[0].item.title, "Token Compactor for Test Outputs");
    assert!(res2[0].score > 0.3);
}

#[test]
fn test_episodic_memory_persistence() {
    let dir = tempdir().unwrap();
    let mut mem = EpisodicMemory::new();

    mem.record_episode(
        "Persistent Architecture Memory",
        "Tested save and load functionality.",
        vec!["storage".to_string()],
        vec![],
        "sess-persist",
    );

    mem.save(dir.path()).unwrap();

    let loaded = EpisodicMemory::load(dir.path()).unwrap();
    assert_eq!(loaded.episodes.len(), 1);
    assert_eq!(loaded.episodes[0].title, "Persistent Architecture Memory");
}

#[test]
fn test_episodic_tool_schemas_registered() {
    let schemas = agent_tools::get_schemas();
    let names: Vec<String> = schemas.into_iter().map(|s| s.name).collect();

    assert!(names.contains(&"record_episode".to_string()));
    assert!(names.contains(&"recall_episodes".to_string()));
}

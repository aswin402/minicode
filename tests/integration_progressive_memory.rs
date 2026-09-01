use minicode::context::progressive_memory::{MemoryTier, ProgressiveMemory};
use tempfile::tempdir;

#[test]
fn test_progressive_memory_tiers_separation() {
    let mut mem = ProgressiveMemory::new();

    // L0
    mem.set_l0("active_subgoal", "refactor parser");
    assert_eq!(
        mem.l0_working.get("active_subgoal").map(|s| s.as_str()),
        Some("refactor parser")
    );

    // L1
    mem.record_l1_anchor(
        "Refactored AST parser module",
        &["src/parser.rs".to_string()],
        "Refactor parser",
        "test_turn",
    );
    assert_eq!(mem.l1_session_anchors.len(), 1);
    assert_eq!(mem.l1_session_anchors[0].tier, MemoryTier::L1SessionAnchor);

    // L2
    mem.add_l2_fact("runtime", "Pure Rust with Tokio async", "manual", 1.0);
    assert_eq!(mem.l2_project_facts.len(), 1);
    assert_eq!(mem.l2_project_facts[0].key, "runtime");

    // L3
    mem.add_l3_preference(
        "resource_limit",
        "Laptop resource constrained: pass -j 1",
        "developer_rule",
    );
    assert_eq!(mem.l3_global_preferences.len(), 1);
    assert_eq!(mem.l3_global_preferences[0].key, "resource_limit");
}

#[test]
fn test_progressive_memory_local_persistence() {
    let dir = tempdir().expect("tempdir");
    let mut mem = ProgressiveMemory::new();

    mem.record_l1_anchor(
        "Completed milestone 1",
        &["src/main.rs".to_string()],
        "Milestone 1",
        "turn_1",
    );
    mem.add_l2_fact("database", "Embedded Sled KV engine", "turn_1", 0.95);

    let res = mem.save(dir.path());
    assert!(res.is_ok());

    let loaded = ProgressiveMemory::load(dir.path());
    assert_eq!(loaded.l1_session_anchors.len(), 1);
    assert_eq!(loaded.l2_project_facts.len(), 1);
    assert_eq!(loaded.l2_project_facts[0].key, "database");
    assert_eq!(loaded.l2_project_facts[0].value, "Embedded Sled KV engine");
}

#[test]
fn test_progressive_memory_fact_extraction() {
    let mut mem = ProgressiveMemory::new();

    let text = r#"
Summary of turn:
Rule: All API handlers must validate request headers
Note: Use bun test instead of jest
Laptop resource-constrained: always pass -j 1 to all builds
"#;

    mem.extract_and_store_facts(text, "compaction_summary");

    // Check extracted L3 preference
    assert!(mem
        .l3_global_preferences
        .iter()
        .any(|p| p.key == "resource_limit"));

    // Check extracted L2 project facts
    assert!(mem.l2_project_facts.iter().any(|f| f.key == "rule"));
    assert!(mem.l2_project_facts.iter().any(|f| f.key == "note"));
}

#[test]
fn test_progressive_memory_prompt_block_formatting() {
    let mut mem = ProgressiveMemory::new();
    mem.set_l0("task", "write test suite");
    mem.add_l2_fact("framework", "Axum web framework", "manual", 1.0);
    mem.add_l3_preference("theme", "Dark mode preferred", "profile");

    let block = mem.to_prompt_block();
    assert!(block.contains("<progressive_memory>"));
    assert!(block.contains("<tier3_global_preferences>"));
    assert!(block.contains("<tier2_project_facts>"));
    assert!(block.contains("<tier0_working_memory>"));
    assert!(block.contains("framework: Axum web framework"));
    assert!(block.contains("theme: Dark mode preferred"));
    assert!(block.contains("task: write test suite"));
    assert!(block.contains("</progressive_memory>"));
}

#[test]
fn test_progressive_memory_query() {
    let mut mem = ProgressiveMemory::new();
    mem.add_l2_fact("auth_protocol", "OAuth2 PKCE flow", "spec", 1.0);
    mem.add_l2_fact("cache_ttl", "300 seconds Redis expiration", "spec", 1.0);

    let hits = mem.query("OAuth2", 5);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].key, "auth_protocol");

    let hits_all = mem.query("seconds", 5);
    assert_eq!(hits_all.len(), 1);
    assert_eq!(hits_all[0].key, "cache_ttl");
}

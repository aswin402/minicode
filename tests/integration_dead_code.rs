use minicode::context::dead_code::{DeadCodeEliminator, DeadCodeKind};
use minicode::context::graph::CodeGraph;
use serde_json::json;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_unreachable_dead_function_detection() {
    let dir = tempdir().expect("tempdir");
    let src_dir = dir.path().join("src");
    fs::create_dir_all(&src_dir).expect("create src");

    // Main entrypoint
    fs::write(
        src_dir.join("main.rs"),
        r#"
pub fn main() {
    active_fn();
}

pub fn active_fn() {}
"#,
    )
    .expect("write main.rs");

    // Dead orphan file
    fs::write(
        src_dir.join("orphan.rs"),
        r#"
pub fn dead_obsolete_calculator() -> i32 {
    100 + 200
}
"#,
    )
    .expect("write orphan.rs");

    let mut graph = CodeGraph::new();
    let _ = graph.build_graph(dir.path());

    let report = DeadCodeEliminator::analyze_workspace(dir.path(), Some(&graph), None, None)
        .expect("analyze workspace dead code");

    let dead_fn = report
        .dead_symbols
        .iter()
        .find(|s| s.name == "dead_obsolete_calculator");
    assert!(dead_fn.is_some());
    assert_eq!(dead_fn.unwrap().kind, DeadCodeKind::DeadFunction);
}

#[test]
fn test_dead_island_cluster_detection() {
    let dir = tempdir().expect("tempdir");
    let src_dir = dir.path().join("src");
    fs::create_dir_all(&src_dir).expect("create src");

    fs::write(
        src_dir.join("main.rs"),
        r#"
pub fn main() {}
"#,
    )
    .expect("write main.rs");

    fs::write(
        src_dir.join("island.rs"),
        r#"
pub fn island_alpha() {
    island_beta();
}

pub fn island_beta() {
    island_alpha();
}
"#,
    )
    .expect("write island.rs");

    let mut graph = CodeGraph::new();
    let _ = graph.build_graph(dir.path());

    let report = DeadCodeEliminator::analyze_workspace(dir.path(), Some(&graph), None, None)
        .expect("analyze workspace dead code");

    let cluster_item = report
        .dead_symbols
        .iter()
        .find(|s| s.kind == DeadCodeKind::DeadIslandCluster);
    assert!(cluster_item.is_some());
}

#[tokio::test]
async fn test_dead_code_sweep_tool_dispatch() {
    let dir = tempdir().expect("tempdir");
    let src_dir = dir.path().join("src");
    fs::create_dir_all(&src_dir).expect("create src");

    fs::write(
        src_dir.join("main.rs"),
        r#"
pub fn main() {}
"#,
    )
    .expect("write main.rs");

    let args = json!({
        "min_confidence": "all"
    });

    let res =
        minicode::tools::registry::context_tools::dispatch("dead_code_sweep", &args, dir.path())
            .await;

    assert!(res.is_some());
    let output = res.unwrap().expect("tool execution success");
    assert!(output.contains("AST-Guided Dead Code & Redundant Symbol Report"));
}

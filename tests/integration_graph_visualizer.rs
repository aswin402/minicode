use minicode::context::graph::CodeGraph;
use minicode::context::graph_visualizer::{GraphVisualizer, VisualizeMode};
use serde_json::json;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_graph_visualizer_box_rendering() {
    let dir = tempdir().expect("tempdir");
    let src_dir = dir.path().join("src");
    fs::create_dir_all(&src_dir).expect("create src");

    fs::write(
        src_dir.join("pipeline.rs"),
        r#"
pub fn execute_pipeline() {
    run_step();
}

pub fn run_step() {}
"#,
    )
    .expect("write pipeline.rs");

    let mut graph = CodeGraph::new();
    let _ = graph.build_graph(dir.path());

    let output = GraphVisualizer::render(
        dir.path(),
        &graph,
        "execute_pipeline",
        VisualizeMode::Box,
        3,
    )
    .expect("render graph visualizer");

    assert!(output.contains("┌"));
    assert!(output.contains("└"));
    assert!(output.contains("execute_pipeline"));
    assert!(output.contains("PageRank Centrality"));
    assert!(output.contains("Risk Assessment"));
}

#[test]
fn test_graph_visualizer_trees() {
    let dir = tempdir().expect("tempdir");
    let src_dir = dir.path().join("src");
    fs::create_dir_all(&src_dir).expect("create src");

    fs::write(
        src_dir.join("service.rs"),
        r#"
pub fn caller_fn() {
    target_fn();
}

pub fn target_fn() {
    callee_fn();
}

pub fn callee_fn() {}
"#,
    )
    .expect("write service.rs");

    let mut graph = CodeGraph::new();
    let _ = graph.build_graph(dir.path());

    let output = GraphVisualizer::render(dir.path(), &graph, "target_fn", VisualizeMode::Both, 3)
        .expect("render graph visualizer");

    assert!(output.contains("Upstream Callers"));
    assert!(output.contains("Downstream Dependencies"));
    assert!(output.contains("caller_fn"));
    assert!(output.contains("callee_fn"));
}

#[tokio::test]
async fn test_graph_visualize_tool_dispatch() {
    let dir = tempdir().expect("tempdir");
    let src_dir = dir.path().join("src");
    fs::create_dir_all(&src_dir).expect("create src");

    fs::write(
        src_dir.join("main.rs"),
        r#"
pub fn app_entry() {}
"#,
    )
    .expect("write main.rs");

    let args = json!({
        "target": "app_entry",
        "mode": "both"
    });

    let res =
        minicode::tools::registry::context_tools::dispatch("graph_visualize", &args, dir.path())
            .await;

    assert!(res.is_some());
    let output = res.unwrap().expect("tool execution success");
    assert!(output.contains("app_entry"));
    assert!(output.contains("```text"));
}

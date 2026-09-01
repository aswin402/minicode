use minicode::context::graph::CodeGraph;
use minicode::context::test_gap::{TestCoverageKind, TestGapAnalyzer};
use serde_json::json;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_test_gap_analysis_reachability() {
    let dir = tempdir().expect("tempdir");
    let src_dir = dir.path().join("src");
    let tests_dir = dir.path().join("tests");
    fs::create_dir_all(&src_dir).expect("create src");
    fs::create_dir_all(&tests_dir).expect("create tests");

    // 1. App code in src/math.rs
    fs::write(
        src_dir.join("math.rs"),
        r#"
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}

pub fn multiply(a: i32, b: i32) -> i32 {
    a * b
}

pub fn divide(a: i32, b: i32) -> i32 {
    if b == 0 { 0 } else { a / b }
}
"#,
    )
    .expect("write math.rs");

    // 2. Test code in tests/math_test.rs calling `add`
    fs::write(
        tests_dir.join("math_test.rs"),
        r#"
use crate::math::add;

#[test]
fn test_add_operation() {
    let result = add(2, 3);
    assert_eq!(result, 5);
}
"#,
    )
    .expect("write math_test.rs");

    let mut graph = CodeGraph::new();
    let _ = graph.build_graph(dir.path());

    let report =
        TestGapAnalyzer::analyze(dir.path(), &graph, None, false, None).expect("analyze test gaps");

    assert!(report.total_symbols >= 3);

    // Verify `add` is directly tested
    let add_gap = report.gaps.iter().find(|g| g.symbol_name == "add");
    assert!(add_gap.is_some());
    let add_gap = add_gap.unwrap();
    assert!(matches!(
        add_gap.coverage_kind,
        TestCoverageKind::DirectlyTested { .. }
    ));

    // Verify `multiply` or `divide` is untested
    let mult_gap = report.gaps.iter().find(|g| g.symbol_name == "multiply");
    assert!(mult_gap.is_some());
    let mult_gap = mult_gap.unwrap();
    assert_eq!(mult_gap.coverage_kind, TestCoverageKind::Untested);

    // Verify untested symbol has higher composite risk than directly tested symbol
    assert!(mult_gap.composite_risk > add_gap.composite_risk);
}

#[tokio::test]
async fn test_test_coverage_gaps_tool_dispatch() {
    let dir = tempdir().expect("tempdir");
    let src_dir = dir.path().join("src");
    fs::create_dir_all(&src_dir).expect("create src");

    fs::write(
        src_dir.join("service.rs"),
        r#"
pub fn run_service() -> bool {
    true
}
"#,
    )
    .expect("write service.rs");

    let args = json!({
        "untested_only": true
    });

    let res = minicode::tools::registry::context_tools::dispatch(
        "test_coverage_gaps",
        &args,
        dir.path(),
    )
    .await;

    assert!(res.is_some());
    let output = res.unwrap().expect("tool execution success");
    assert!(output.contains("Test Gap & Reachability Analysis"));
    assert!(output.contains("run_service"));
}

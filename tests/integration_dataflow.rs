use minicode::context::dataflow::DataflowAnalyzer;
use serde_json::json;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_forward_dataflow_and_taint_detection() {
    let dir = tempdir().expect("tempdir");
    let src_dir = dir.path().join("src");
    fs::create_dir_all(&src_dir).expect("create src");

    fs::write(
        src_dir.join("pipeline.rs"),
        r#"
pub fn parse_input(raw: &str) {
    let clean = sanitize_text(raw);
    execute_system_command(&clean);
}

pub fn sanitize_text(s: &str) -> String {
    s.trim().to_string()
}

pub fn execute_system_command(cmd: &str) {
    // sensitive sink
}
"#,
    )
    .expect("write pipeline.rs");

    let report = DataflowAnalyzer::trace(dir.path(), "parse_input", "forward", 5, true)
        .expect("trace dataflow");

    assert_eq!(report.target_symbol, "parse_input");
    assert_eq!(report.direction, "forward");
    assert!(!report.traces.is_empty());

    let command_trace = report
        .traces
        .iter()
        .find(|t| t.sink_symbol == "execute_system_command");
    assert!(command_trace.is_some());
    let ct = command_trace.unwrap();
    assert!(ct.is_tainted);
    assert!(ct.taint_warning.is_some());
}

#[test]
fn test_backward_dataflow_slicing() {
    let dir = tempdir().expect("tempdir");
    let src_dir = dir.path().join("src");
    fs::create_dir_all(&src_dir).expect("create src");

    fs::write(
        src_dir.join("api.rs"),
        r#"
pub fn handle_request() {
    dispatch_worker();
}

pub fn dispatch_worker() {
    save_to_database();
}

pub fn save_to_database() {
    // sink
}
"#,
    )
    .expect("write api.rs");

    let report = DataflowAnalyzer::trace(dir.path(), "save_to_database", "backward", 5, false)
        .expect("backward slice");

    assert_eq!(report.target_symbol, "save_to_database");
    assert_eq!(report.direction, "backward");
}

#[tokio::test]
async fn test_trace_dataflow_tool_dispatch() {
    let dir = tempdir().expect("tempdir");
    let src_dir = dir.path().join("src");
    fs::create_dir_all(&src_dir).expect("create src");

    fs::write(
        src_dir.join("main.rs"),
        "fn main() { helper(); }\nfn helper() {}",
    )
    .expect("write main.rs");

    let args = json!({
        "target_symbol": "main",
        "direction": "forward",
        "max_depth": 3,
        "taint_check": true
    });

    let res =
        minicode::tools::registry::context_tools::dispatch("trace_dataflow", &args, dir.path())
            .await;

    assert!(res.is_some());
    let output = res.unwrap().expect("tool execution success");
    assert!(output.contains("Type-Flow & Dataflow Reachability Report"));
    assert!(output.contains("main"));
}

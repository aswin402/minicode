use minicode::lsp::diagnostics::{DiagnosticItem, DiagnosticReport};
use minicode::lsp::protocol::{decode_message, encode_message};
use minicode::tools::ToolRegistry;
use serde_json::json;
use std::path::PathBuf;
use tempfile::tempdir;

#[test]
fn test_lsp_jsonrpc_framing_roundtrip() {
    let request = json!({
        "jsonrpc": "2.0",
        "id": 42,
        "method": "textDocument/definition",
        "params": {
            "textDocument": { "uri": "file:///workspace/src/lib.rs" },
            "position": { "line": 10, "character": 5 }
        }
    });

    let framed = encode_message(&request).expect("Encoding message failed");
    assert!(framed.starts_with(b"Content-Length: "));

    let (decoded, consumed) = decode_message(&framed)
        .expect("Decoding message failed")
        .expect("Expected complete frame");

    assert_eq!(consumed, framed.len());
    assert_eq!(decoded["id"], 42);
    assert_eq!(decoded["method"], "textDocument/definition");
}

#[test]
fn test_diagnostic_report_summary_formatting() {
    let mut report = DiagnosticReport::default();
    assert!(report.is_clean());
    assert_eq!(report.total_issues(), 0);

    report.errors.push(DiagnosticItem {
        file: PathBuf::from("/workspace/src/calc.rs"),
        line: 14,
        column: 9,
        severity: "error".to_string(),
        code: Some("E0308".to_string()),
        message: "mismatched types: expected `i32`, found `&str`".to_string(),
        rendered: Some("   |\n14 | let res: i32 = \"value\";\n   |                ^^^^^^^ expected `i32`, found `&str`".to_string()),
    });

    assert!(!report.is_clean());
    assert_eq!(report.total_issues(), 1);

    let ws = std::path::Path::new("/workspace");
    let formatted = report.format_for_agent(ws, 5);
    assert!(formatted.contains("src/calc.rs:14:9 [E0308]"));
    assert!(formatted.contains("mismatched types"));
    assert!(formatted.contains("let res: i32 = \"value\""));
}

#[tokio::test]
async fn test_lsp_diagnostics_tool_dispatch() {
    let dir = tempdir().unwrap();
    let ws_path = dir.path().to_path_buf();

    // Clean workspace should report clean
    let result = ToolRegistry::dispatch(
        &ws_path,
        "call_diag",
        "lsp_diagnostics",
        &json!({ "max_items": 5 }),
        None,
        1,
    )
    .await;

    assert!(result.success);
    assert!(result.output.contains("compiles cleanly") || result.output.contains("zero errors"));
}

use minicode::agent::types::ToolResult;
/// Integration tests for Phase 29: Inline Diff Preview
///
/// Tests the diff engine, DiffMiddleware, and DIFF_MARKER detection.
use minicode::tools::diff::{compute_diff, format_diff_plain, has_changes};
use minicode::tools::middleware::{
    DiffMiddleware, ToolContext, ToolMiddleware, ToolPipeline, DIFF_MARKER,
};
use serde_json::json;
use std::io::Write;
use std::path::Path;

// ── diff engine ───────────────────────────────────────────────────────────────

#[test]
fn test_diff_engine_detects_added_line() {
    let old = "fn main() {}\n";
    let new = "fn main() {}\nfn helper() {}\n";
    let diff = compute_diff(old, new);
    assert!(has_changes(&diff));
    assert!(diff
        .iter()
        .any(|l| l.tag == '+' && l.content.contains("helper")));
}

#[test]
fn test_diff_engine_detects_removed_line() {
    let old = "line a\nline b\nline c\n";
    let new = "line a\nline c\n";
    let diff = compute_diff(old, new);
    assert!(has_changes(&diff));
    assert!(diff
        .iter()
        .any(|l| l.tag == '-' && l.content.contains("line b")));
}

#[test]
fn test_diff_engine_no_changes_when_identical() {
    let content = "no changes here\n";
    let diff = compute_diff(content, content);
    assert!(!has_changes(&diff));
}

#[test]
fn test_diff_engine_new_file_from_empty() {
    let old = "";
    let new = "fn new_fn() {}\n";
    let diff = compute_diff(old, new);
    assert!(has_changes(&diff));
    assert!(diff.iter().all(|l| l.tag != '-'));
}

#[test]
fn test_format_diff_plain_header_format() {
    let old = "old\n";
    let new = "new\n";
    let diff = compute_diff(old, new);
    let formatted = format_diff_plain(&diff, "src/main.rs");
    assert!(formatted.starts_with("--- src/main.rs\n+++ src/main.rs\n"));
    assert!(formatted.contains("+ new"));
    assert!(formatted.contains("- old"));
}

// ── DiffMiddleware ────────────────────────────────────────────────────────────

fn tmp_file_with_content(content: &str) -> (tempfile::NamedTempFile, std::path::PathBuf) {
    let mut f = tempfile::NamedTempFile::new().unwrap();
    f.write_all(content.as_bytes()).unwrap();
    let path = f.path().to_path_buf();
    (f, path)
}

#[test]
fn test_diff_middleware_skips_non_file_tool() {
    let mw = DiffMiddleware;
    let args = json!({});
    let result = ToolResult {
        tool_id: "x".into(),
        tool_name: "exec_cmd".into(),
        success: true,
        output: "output".into(),
        duration_ms: 1,
    };
    let ctx = ToolContext {
        tool_name: "exec_cmd",
        workspace_root: Path::new("."),
        args: &args,
        file_before: None,
    };
    let out = mw.after(&ctx, result.clone());
    assert_eq!(out.output, result.output);
}

#[test]
fn test_diff_middleware_skips_failed_tool() {
    let mw = DiffMiddleware;
    let args = json!({"path": "src/main.rs"});
    let result = ToolResult {
        tool_id: "x".into(),
        tool_name: "write_file".into(),
        success: false,
        output: "error occurred".into(),
        duration_ms: 1,
    };
    let ctx = ToolContext {
        tool_name: "write_file",
        workspace_root: Path::new("."),
        args: &args,
        file_before: None,
    };
    let out = mw.after(&ctx, result.clone());
    // No diff should be attached to failed tool result
    assert!(!out.output.contains(DIFF_MARKER));
    assert_eq!(out.output, result.output);
}

#[test]
fn test_diff_middleware_attaches_diff_for_write_file() {
    // Write a temp file to disk (simulates the "after" state)
    let after_content = "fn main() {}\nfn helper() {}\n";
    let (tmp_file, full_path) = tmp_file_with_content(after_content);
    let workspace = full_path.parent().unwrap();
    let rel_path = full_path.file_name().unwrap().to_str().unwrap();

    let before_content = "fn main() {}\n";
    let args = json!({"path": rel_path});
    let result = ToolResult {
        tool_id: "x".into(),
        tool_name: "write_file".into(),
        success: true,
        output: "written 32 bytes".into(),
        duration_ms: 5,
    };
    let ctx = ToolContext {
        tool_name: "write_file",
        workspace_root: workspace,
        args: &args,
        file_before: Some(before_content),
    };
    let mw = DiffMiddleware;
    let out = mw.after(&ctx, result);
    assert!(
        out.output.starts_with(DIFF_MARKER),
        "expected DIFF_MARKER prefix"
    );
    assert!(out.output.contains("helper"), "expected new line in diff");

    drop(tmp_file);
}

#[test]
fn test_diff_middleware_no_diff_when_no_changes() {
    let content = "fn main() {}\n";
    // Write the same content (no changes)
    let (tmp_file, full_path) = tmp_file_with_content(content);
    let workspace = full_path.parent().unwrap();
    let rel_path = full_path.file_name().unwrap().to_str().unwrap();

    let args = json!({"path": rel_path});
    let result = ToolResult {
        tool_id: "x".into(),
        tool_name: "write_file".into(),
        success: true,
        output: "written".into(),
        duration_ms: 1,
    };
    let ctx = ToolContext {
        tool_name: "write_file",
        workspace_root: workspace,
        args: &args,
        file_before: Some(content),
    };
    let mw = DiffMiddleware;
    let out = mw.after(&ctx, result.clone());
    // No diff block — content unchanged
    assert!(!out.output.starts_with(DIFF_MARKER));
    assert_eq!(out.output, result.output);

    drop(tmp_file);
}

// ── Pipeline integration ───────────────────────────────────────────────────────

#[test]
fn test_pipeline_run_signature_accepts_file_before() {
    let pipeline = ToolPipeline::default();
    let args = json!({});
    let result = ToolResult {
        tool_id: "t1".into(),
        tool_name: "exec_cmd".into(),
        success: true,
        output: "clean output".into(),
        duration_ms: 10,
    };
    // Pass Some("old content") — should not affect non-file-modifying tool
    let out = pipeline.run(
        result,
        "exec_cmd",
        Path::new("."),
        &args,
        Some("old content"),
    );
    assert_eq!(out.output, "clean output");
}

#[test]
fn test_pipeline_run_none_file_before_is_valid() {
    let pipeline = ToolPipeline::default();
    let args = json!({});
    let result = ToolResult {
        tool_id: "t1".into(),
        tool_name: "grep_search".into(),
        success: true,
        output: "3 matches".into(),
        duration_ms: 2,
    };
    let out = pipeline.run(result, "grep_search", Path::new("."), &args, None);
    assert_eq!(out.output, "3 matches");
}

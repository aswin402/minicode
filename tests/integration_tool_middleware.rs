/// Integration tests for Phase 28: Tool Middleware Pipeline
///
/// Exercises the `ToolPipeline` with all three built-in middlewares through
/// real `ToolResult` values, ensuring the public API is stable and composed
/// behaviour is correct.
use minicode::agent::types::ToolResult;
use minicode::tools::middleware::{
    CheckpointMiddleware, RedactMiddleware, TimingMiddleware, ToolContext, ToolMiddleware,
    ToolPipeline,
};
use serde_json::json;
use std::path::Path;

// ── Helpers ────────────────────────────────────────────────────────────────────

fn make_result(output: &str, success: bool, duration_ms: u64) -> ToolResult {
    ToolResult {
        tool_id: "integration-call-1".into(),
        tool_name: "test_tool".into(),
        success,
        output: output.to_string(),
        duration_ms,
    }
}

// ── RedactMiddleware ───────────────────────────────────────────────────────────

#[test]
fn test_redact_strips_openai_key_from_output() {
    let mw = RedactMiddleware;
    let args = json!({});
    let ctx = ToolContext {
        tool_name: "exec_cmd",
        workspace_root: Path::new("."),
        args: &args,
    };
    let r = make_result(
        "OPENAI_API_KEY=sk-proj-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA result: ok",
        true,
        10,
    );
    let out = mw.after(&ctx, r);
    assert!(
        out.output.contains("[REDACTED]"),
        "expected redaction in: {}",
        out.output
    );
    assert!(!out.output.contains("sk-proj-"));
}

#[test]
fn test_redact_strips_anthropic_key() {
    let mw = RedactMiddleware;
    let args = json!({});
    let ctx = ToolContext {
        tool_name: "exec_cmd",
        workspace_root: Path::new("."),
        args: &args,
    };
    let r = make_result("key: sk-ant-api03-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA", true, 5);
    let out = mw.after(&ctx, r);
    assert!(out.output.contains("[REDACTED]"));
}

#[test]
fn test_redact_preserves_clean_output() {
    let mw = RedactMiddleware;
    let args = json!({});
    let ctx = ToolContext {
        tool_name: "read_file",
        workspace_root: Path::new("."),
        args: &args,
    };
    let content = "fn hello() -> &'static str { \"world\" }";
    let r = make_result(content, true, 3);
    let out = mw.after(&ctx, r);
    assert_eq!(out.output, content);
}

// ── TimingMiddleware ───────────────────────────────────────────────────────────

#[test]
fn test_timing_middleware_is_identity() {
    let mw = TimingMiddleware;
    let args = json!({});
    let ctx = ToolContext {
        tool_name: "grep_search",
        workspace_root: Path::new("."),
        args: &args,
    };
    let r = make_result("3 matches found", true, 99);
    let out = mw.after(&ctx, r.clone());
    assert_eq!(out.output, r.output);
    assert_eq!(out.duration_ms, r.duration_ms);
    assert_eq!(out.success, r.success);
}

// ── CheckpointMiddleware ───────────────────────────────────────────────────────

#[test]
fn test_checkpoint_middleware_does_not_mutate_output() {
    let mw = CheckpointMiddleware;
    let args = json!({"path": "src/lib.rs", "content": "// new"});
    let ctx = ToolContext {
        tool_name: "write_file",
        workspace_root: Path::new("."),
        args: &args,
    };
    let r = make_result("written 512 bytes", true, 12);
    let out = mw.after(&ctx, r.clone());
    assert_eq!(out.output, r.output);
    assert_eq!(out.success, r.success);
}

#[test]
fn test_checkpoint_read_only_tool_not_affected() {
    let mw = CheckpointMiddleware;
    let args = json!({"path": "src/main.rs"});
    let ctx = ToolContext {
        tool_name: "read_file",
        workspace_root: Path::new("."),
        args: &args,
    };
    let r = make_result("// file contents", true, 2);
    let out = mw.after(&ctx, r.clone());
    assert_eq!(out.output, r.output);
}

// ── ToolPipeline ──────────────────────────────────────────────────────────────

#[test]
fn test_pipeline_redacts_and_preserves_metadata() {
    let pipeline = ToolPipeline::default();
    let args = json!({});
    let r = make_result(
        "ENV=sk-proj-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA finished",
        true,
        250,
    );
    let out = pipeline.run(r, "exec_cmd", Path::new("."), &args);
    assert!(out.output.contains("[REDACTED]"));
    assert!(out.success);
    assert_eq!(out.duration_ms, 250);
}

#[test]
fn test_empty_pipeline_is_identity() {
    let pipeline = ToolPipeline::new(vec![]);
    let args = json!({});
    let r = make_result("hello world", false, 5);
    let out = pipeline.run(r.clone(), "read_file", Path::new("."), &args);
    assert_eq!(out.output, r.output);
    assert_eq!(out.success, r.success);
    assert_eq!(out.duration_ms, r.duration_ms);
}

#[test]
fn test_pipeline_failure_flag_preserved() {
    let pipeline = ToolPipeline::default();
    let args = json!({});
    let r = make_result("permission denied", false, 1);
    let out = pipeline.run(r, "exec_cmd", Path::new("."), &args);
    assert!(!out.success);
}

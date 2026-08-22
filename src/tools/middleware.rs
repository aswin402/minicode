//! Tool Middleware Pipeline
//!
//! Provides a composable `ToolMiddleware` trait and a `ToolPipeline` runner
//! that wraps every tool execution — both built-in and MCP — with ordered
//! before/after hooks.
//!
//! Built-in middlewares (applied in order):
//! 1. [`TimingMiddleware`]   — records execution span via `tracing`
//! 2. [`RedactMiddleware`]   — strips secrets from tool output
//! 3. [`CheckpointMiddleware`] — logs destructive tool calls for telemetry
//!
//! # Usage
//!
//! ```rust,ignore
//! let pipeline = ToolPipeline::default();
//! let result = pipeline.run(raw_tool_result, &tool_name, &workspace_root);
//! ```

use crate::agent::types::ToolResult;
use crate::constants::FILE_MODIFYING_TOOLS;
use std::path::Path;

// ── Context ───────────────────────────────────────────────────────────────────

/// Read-only context passed to each middleware.
pub struct ToolContext<'a> {
    /// Name of the tool that was called.
    pub tool_name: &'a str,
    /// Workspace root directory — available for middlewares that need path context.
    #[allow(dead_code)]
    pub workspace_root: &'a Path,
    /// Raw arguments passed to the tool (JSON).
    pub args: &'a serde_json::Value,
}

// ── Trait ─────────────────────────────────────────────────────────────────────

/// A composable hook that runs before and/or after every tool execution.
///
/// Both hooks receive a `ToolContext` plus the current `ToolResult` for
/// `after`. Middlewares are applied in declaration order; returning `None`
/// from `before` lets execution continue, returning `Some(result)` short-
/// circuits the rest of the pipeline with that result.
pub trait ToolMiddleware: Send + Sync {
    /// Called with the completed `ToolResult` to optionally transform it.
    /// Implementations MUST be pure transforms (no side-effects on `ctx`).
    fn after(&self, ctx: &ToolContext<'_>, result: ToolResult) -> ToolResult;
}

// ── Pipeline ──────────────────────────────────────────────────────────────────

/// Ordered pipeline of [`ToolMiddleware`] implementations.
///
/// Call [`ToolPipeline::run`] after every tool execution to apply all
/// middlewares in sequence.
pub struct ToolPipeline {
    middlewares: Vec<Box<dyn ToolMiddleware>>,
}

impl Default for ToolPipeline {
    /// Returns the default pipeline used by `AgentLoop`:
    /// `TimingMiddleware → RedactMiddleware → CheckpointMiddleware`
    fn default() -> Self {
        Self {
            middlewares: vec![
                Box::new(TimingMiddleware),
                Box::new(RedactMiddleware),
                Box::new(CheckpointMiddleware),
            ],
        }
    }
}

impl ToolPipeline {
    /// Construct a pipeline with an explicit list of middlewares.
    #[allow(dead_code)]
    pub fn new(middlewares: Vec<Box<dyn ToolMiddleware>>) -> Self {
        Self { middlewares }
    }

    /// Run all middlewares' `after` hooks against `result`, returning the
    /// transformed result. Applies middlewares in declaration order.
    pub fn run(
        &self,
        result: ToolResult,
        tool_name: &str,
        workspace_root: &Path,
        args: &serde_json::Value,
    ) -> ToolResult {
        let ctx = ToolContext {
            tool_name,
            workspace_root,
            args,
        };
        self.middlewares
            .iter()
            .fold(result, |acc, mw| mw.after(&ctx, acc))
    }
}

// ── Built-in middlewares ───────────────────────────────────────────────────────

/// Emits a `tracing::debug!` span with tool name, success flag, and wall-clock
/// duration so every execution appears in structured logs.
pub struct TimingMiddleware;

impl ToolMiddleware for TimingMiddleware {
    fn after(&self, ctx: &ToolContext<'_>, result: ToolResult) -> ToolResult {
        tracing::debug!(
            tool = ctx.tool_name,
            success = result.success,
            duration_ms = result.duration_ms,
            output_bytes = result.output.len(),
            "tool executed"
        );
        result
    }
}

/// Scrubs secrets from tool output using the global `SecretRedactor`.
///
/// This is the canonical, single redaction hook — the ad-hoc inline call in
/// `agent/loop.rs` delegates here via `ToolPipeline::run`.
pub struct RedactMiddleware;

impl ToolMiddleware for RedactMiddleware {
    fn after(&self, _ctx: &ToolContext<'_>, result: ToolResult) -> ToolResult {
        let redacted = crate::sandbox::redact::SecretRedactor::global().redact(&result.output);
        ToolResult {
            output: redacted,
            ..result
        }
    }
}

/// Emits a structured `tracing::info!` event whenever a destructive (file-
/// modifying) tool completes. Provides an immutable audit trail without
/// blocking execution.
pub struct CheckpointMiddleware;

impl ToolMiddleware for CheckpointMiddleware {
    fn after(&self, ctx: &ToolContext<'_>, result: ToolResult) -> ToolResult {
        if FILE_MODIFYING_TOOLS.contains(&ctx.tool_name) {
            let file_path = ctx
                .args
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or("<unknown>");
            tracing::info!(
                tool = ctx.tool_name,
                file = file_path,
                success = result.success,
                duration_ms = result.duration_ms,
                "destructive tool completed — checkpoint logged"
            );
        }
        result
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_result(output: &str, success: bool) -> ToolResult {
        ToolResult {
            tool_id: "t1".into(),
            tool_name: "test_tool".into(),
            success,
            output: output.to_string(),
            duration_ms: 42,
        }
    }

    #[test]
    fn test_redact_middleware_strips_openai_key() {
        let mw = RedactMiddleware;
        let args = json!({});
        let ctx = ToolContext {
            tool_name: "exec_cmd",
            workspace_root: Path::new("."),
            args: &args,
        };
        let result = make_result("key=sk-proj-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA", true);
        let out = mw.after(&ctx, result);
        assert!(out.output.contains("[REDACTED]"));
        assert!(!out.output.contains("sk-proj-"));
    }

    #[test]
    fn test_redact_middleware_passthrough_clean_output() {
        let mw = RedactMiddleware;
        let args = json!({});
        let ctx = ToolContext {
            tool_name: "read_file",
            workspace_root: Path::new("."),
            args: &args,
        };
        let result = make_result("fn main() { println!(\"hello\"); }", true);
        let out = mw.after(&ctx, result);
        assert_eq!(out.output, "fn main() { println!(\"hello\"); }");
    }

    #[test]
    fn test_timing_middleware_is_transparent() {
        let mw = TimingMiddleware;
        let args = json!({});
        let ctx = ToolContext {
            tool_name: "read_file",
            workspace_root: Path::new("."),
            args: &args,
        };
        let result = make_result("some output", true);
        let out = mw.after(&ctx, result.clone());
        assert_eq!(out.output, result.output);
        assert_eq!(out.duration_ms, result.duration_ms);
    }

    #[test]
    fn test_checkpoint_middleware_passthrough_non_destructive() {
        let mw = CheckpointMiddleware;
        let args = json!({});
        let ctx = ToolContext {
            tool_name: "read_file",
            workspace_root: Path::new("."),
            args: &args,
        };
        let result = make_result("file contents", true);
        let out = mw.after(&ctx, result.clone());
        // No mutation expected for read-only tool
        assert_eq!(out.output, result.output);
    }

    #[test]
    fn test_checkpoint_middleware_logs_write_file() {
        let mw = CheckpointMiddleware;
        let args = json!({"path": "src/main.rs"});
        let ctx = ToolContext {
            tool_name: "write_file",
            workspace_root: Path::new("."),
            args: &args,
        };
        let result = make_result("written 512 bytes", true);
        let out = mw.after(&ctx, result.clone());
        // Output must remain unmodified — checkpoint is side-effect only
        assert_eq!(out.output, result.output);
    }

    #[test]
    fn test_pipeline_default_applies_all_middlewares() {
        let pipeline = ToolPipeline::default();
        let args = json!({});
        let result = make_result(
            "token: sk-proj-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA — done",
            true,
        );
        let out = pipeline.run(result, "exec_cmd", Path::new("."), &args);
        assert!(out.output.contains("[REDACTED]"));
        assert!(!out.output.contains("sk-proj-"));
    }

    #[test]
    fn test_pipeline_preserves_failure_flag() {
        let pipeline = ToolPipeline::default();
        let args = json!({});
        let result = make_result("command not found", false);
        let out = pipeline.run(result, "exec_cmd", Path::new("."), &args);
        assert!(!out.success);
    }

    #[test]
    fn test_pipeline_preserves_duration_ms() {
        let pipeline = ToolPipeline::default();
        let args = json!({});
        let mut result = make_result("ok", true);
        result.duration_ms = 1337;
        let out = pipeline.run(result, "read_file", Path::new("."), &args);
        assert_eq!(out.duration_ms, 1337);
    }

    #[test]
    fn test_empty_pipeline_is_identity() {
        let pipeline = ToolPipeline::new(vec![]);
        let args = json!({});
        let result = make_result("raw output", true);
        let out = pipeline.run(result.clone(), "read_file", Path::new("."), &args);
        assert_eq!(out.output, result.output);
        assert_eq!(out.success, result.success);
    }
}

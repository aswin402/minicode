//! Tool Middleware Pipeline
//!
//! Provides a composable `ToolMiddleware` trait and a `ToolPipeline` runner
//! that wraps every tool execution — both built-in and MCP — with ordered
//! before/after hooks.
//!
//! Built-in middlewares (applied in order):
//! 1. [`TimingMiddleware`]     — records execution span via `tracing`
//! 2. [`RedactMiddleware`]     — strips secrets from tool output
//! 3. [`CheckpointMiddleware`] — logs destructive tool calls for telemetry
//! 4. [`DiffMiddleware`]       — prepends inline diff for file-modifying tools
//!
//! # Usage
//!
//! ```rust,ignore
//! // Read file before dispatch (for diff preview):
//! let file_before = std::fs::read_to_string(&path).ok();
//! // ... dispatch tool ...
//! let result = pipeline.run(raw_result, &tool_name, &workspace_root, &args, file_before.as_deref());
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
    /// Contents of the target file **before** tool execution.
    /// `Some(content)` for `write_file` / `patch_file` when the file existed
    /// prior to dispatch; `None` otherwise.
    pub file_before: Option<&'a str>,
}

// ── Trait ─────────────────────────────────────────────────────────────────────

/// A composable hook applied to every completed tool execution.
///
/// Implementations receive a `ToolContext` plus the current `ToolResult` and
/// return a (possibly mutated) `ToolResult`. Middlewares run in declaration
/// order via `Iterator::fold`.
pub trait ToolMiddleware: Send + Sync {
    fn after(&self, ctx: &ToolContext<'_>, result: ToolResult) -> ToolResult;
}

// ── Pipeline ──────────────────────────────────────────────────────────────────

/// Ordered pipeline of [`ToolMiddleware`] implementations.
pub struct ToolPipeline {
    middlewares: Vec<Box<dyn ToolMiddleware>>,
}

impl Default for ToolPipeline {
    /// Default pipeline: `Timing → Redact → Checkpoint → Diff`
    fn default() -> Self {
        Self {
            middlewares: vec![
                Box::new(TimingMiddleware),
                Box::new(RedactMiddleware),
                Box::new(CheckpointMiddleware),
                Box::new(DiffMiddleware),
            ],
        }
    }
}

impl ToolPipeline {
    #[allow(dead_code)]
    pub fn new(middlewares: Vec<Box<dyn ToolMiddleware>>) -> Self {
        Self { middlewares }
    }

    /// Run all middlewares' `after` hooks in order.
    ///
    /// `file_before`: contents of the target file read **before** dispatch,
    /// used by `DiffMiddleware` to compute a before/after diff.
    pub fn run(
        &self,
        result: ToolResult,
        tool_name: &str,
        workspace_root: &Path,
        args: &serde_json::Value,
        file_before: Option<&str>,
    ) -> ToolResult {
        let ctx = ToolContext {
            tool_name,
            workspace_root,
            args,
            file_before,
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

/// Appends an inline unified diff block to the tool output for `write_file`
/// and `patch_file` results.
///
/// Requires `ctx.file_before` to be populated by the call site **before**
/// dispatch. Reads the new file content from disk after execution and computes
/// a diff using the `similar` crate. The diff is prepended to the tool output
/// in a machine-readable `DIFF:` marker block so the TUI renderer can detect
/// and colour it.
///
/// No-ops when:
/// - The tool is not a file-modifying tool
/// - `file_before` is `None` (new file creation)
/// - The tool failed
/// - There are no actual changes in the diff
pub struct DiffMiddleware;

/// Marker prefix embedded in tool output to signal a diff block to the TUI.
pub const DIFF_MARKER: &str = "MINICODE_DIFF_BLOCK:";

impl ToolMiddleware for DiffMiddleware {
    fn after(&self, ctx: &ToolContext<'_>, result: ToolResult) -> ToolResult {
        // Only apply to successful file-modifying tools
        if !result.success || !FILE_MODIFYING_TOOLS.contains(&ctx.tool_name) {
            return result;
        }

        let file_path = match ctx.args.get("path").and_then(|v| v.as_str()) {
            Some(p) => p,
            None => return result,
        };

        // Resolve full path for reading new content
        let full_path = ctx.workspace_root.join(file_path);
        let file_after = match std::fs::read_to_string(&full_path) {
            Ok(c) => c,
            Err(e) => {
                tracing::debug!(
                    "DiffMiddleware: could not read '{}' after write: {}",
                    file_path,
                    e
                );
                return result;
            }
        };

        let before = match ctx.file_before {
            Some(b) => b,
            None => {
                // New file — show entire content as additions
                let diff_lines = crate::tools::diff::compute_diff("", &file_after);
                if !crate::tools::diff::has_changes(&diff_lines) {
                    return result;
                }
                let diff_text = crate::tools::diff::format_diff_plain(&diff_lines, file_path);
                let output = format!("{}{}\n{}", DIFF_MARKER, diff_text, result.output);
                tracing::debug!(
                    tool = ctx.tool_name,
                    file = file_path,
                    "DiffMiddleware: attached new-file diff ({} diff lines)",
                    diff_lines.len()
                );
                return ToolResult { output, ..result };
            }
        };

        let diff_lines = crate::tools::diff::compute_diff(before, &file_after);
        if !crate::tools::diff::has_changes(&diff_lines) {
            tracing::debug!("DiffMiddleware: no changes detected for '{}'", file_path);
            return result;
        }

        let diff_text = crate::tools::diff::format_diff_plain(&diff_lines, file_path);
        let output = format!("{}{}\n{}", DIFF_MARKER, diff_text, result.output);
        tracing::debug!(
            tool = ctx.tool_name,
            file = file_path,
            "DiffMiddleware: attached diff ({} diff lines)",
            diff_lines.len()
        );
        ToolResult { output, ..result }
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
            file_before: None,
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
            file_before: None,
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
            file_before: None,
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
            file_before: None,
        };
        let result = make_result("file contents", true);
        let out = mw.after(&ctx, result.clone());
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
            file_before: None,
        };
        let result = make_result("written 512 bytes", true);
        let out = mw.after(&ctx, result.clone());
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
        let out = pipeline.run(result, "exec_cmd", Path::new("."), &args, None);
        assert!(out.output.contains("[REDACTED]"));
        assert!(!out.output.contains("sk-proj-"));
    }

    #[test]
    fn test_pipeline_preserves_failure_flag() {
        let pipeline = ToolPipeline::default();
        let args = json!({});
        let result = make_result("command not found", false);
        let out = pipeline.run(result, "exec_cmd", Path::new("."), &args, None);
        assert!(!out.success);
    }

    #[test]
    fn test_pipeline_preserves_duration_ms() {
        let pipeline = ToolPipeline::default();
        let args = json!({});
        let mut result = make_result("ok", true);
        result.duration_ms = 1337;
        let out = pipeline.run(result, "read_file", Path::new("."), &args, None);
        assert_eq!(out.duration_ms, 1337);
    }

    #[test]
    fn test_empty_pipeline_is_identity() {
        let pipeline = ToolPipeline::new(vec![]);
        let args = json!({});
        let result = make_result("raw output", true);
        let out = pipeline.run(result.clone(), "read_file", Path::new("."), &args, None);
        assert_eq!(out.output, result.output);
        assert_eq!(out.success, result.success);
    }
}

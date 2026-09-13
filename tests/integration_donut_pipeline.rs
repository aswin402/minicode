use minicode::agent::types::ToolResult;
use minicode::constants::{DONUT_MAX_LINE_CHARS, DONUT_STANDARD_THRESHOLD_LINES};
use minicode::context::budget::donut::SmartDonutTruncator;
use minicode::tools::exec::exec_cmd;
use minicode::tools::fs::{read_file, write_file};
use minicode::tools::middleware::{DiffMiddleware, ToolContext, ToolMiddleware, DIFF_MARKER};
use minicode::tools::rtk_filter::RtkFilter;
use serde_json::json;
use tempfile::tempdir;

#[test]
fn test_donut_truncation_preserves_middle_compiler_errors() {
    let mut lines = Vec::new();
    for i in 1..=200 {
        if i == 75 {
            lines
                .push("error[E0425]: cannot find value `unresolved_var` in this scope".to_string());
            lines.push("  --> src/main.rs:75:13".to_string());
        } else if i == 110 {
            lines.push("FAILED test_something ...".to_string());
        } else {
            lines.push(format!("    Compiling package v0.1.0 log line {}", i));
        }
    }
    let input = lines.join("\n");

    let res =
        SmartDonutTruncator::truncate_custom(&input, DONUT_STANDARD_THRESHOLD_LINES, 30, 50, 40);

    assert!(
        res.was_truncated,
        "Output exceeding threshold should be truncated"
    );
    assert!(
        res.extracted_error_count >= 2,
        "Should extract compiler errors from omitted middle section"
    );
    assert!(res
        .content
        .contains("error[E0425]: cannot find value `unresolved_var`"));
    assert!(res.content.contains("Line 76:   --> src/main.rs:75:13"));
    assert!(res.content.contains("FAILED test_something"));
    assert!(res.content.contains("[... Smart Donut Truncation: Omitted"));
}

#[test]
fn test_rtk_filter_generic_delegates_to_donut_and_preserves_middle_errors() {
    let mut lines = Vec::new();
    for i in 1..=180 {
        if i == 60 {
            lines.push("error: aborting due to 1 previous error".to_string());
            lines.push("  --> src/lib.rs:60:5".to_string());
        } else {
            lines.push(format!("Generic log message {}", i));
        }
    }
    let input = lines.join("\n");

    // Generic command (not cargo test/pytest)
    let filtered = RtkFilter::filter("custom_build.sh", &input, Some(101));

    assert!(filtered
        .content
        .contains("error: aborting due to 1 previous error"));
    assert!(filtered.content.contains("Smart Donut Truncation"));
}

#[test]
fn test_donut_clamp_excessively_wide_lines() {
    let wide_line = "x".repeat(1000);
    let clamped = SmartDonutTruncator::clamp_line_width(&wide_line);

    assert!(
        clamped.contains(&format!(
            "[... line clamped at {} chars ...]",
            DONUT_MAX_LINE_CHARS
        )),
        "Wide line must be clamped at DONUT_MAX_LINE_CHARS"
    );
    assert!(clamped.chars().count() < 400);
}

#[test]
fn test_read_file_pipe_gutter_format() {
    let temp = tempdir().unwrap();
    let workspace = temp.path();
    let rel_path = "src/code.rs";
    let content = "fn main() {\n    let x = 42;\n    println!(\"{}\", x);\n}\n";
    write_file(workspace, rel_path, content).unwrap();

    let output = read_file(workspace, rel_path, Some(1), Some(3)).unwrap();

    // Must use {:>4} | {} formatting
    assert!(output.contains("   1 | fn main() {"));
    assert!(output.contains("   2 |     let x = 42;"));
    assert!(output.contains("   3 |     println!(\"{}\", x);"));
}

#[test]
fn test_read_file_unbounded_pagination_window_over_250_lines() {
    let temp = tempdir().unwrap();
    let workspace = temp.path();
    let rel_path = "src/large.rs";

    let mut lines = Vec::new();
    for i in 1..=300 {
        lines.push(format!("pub fn line_{}() {{}}", i));
    }
    write_file(workspace, rel_path, &lines.join("\n")).unwrap();

    // Call read_file without start_line/end_line
    let output = read_file(workspace, rel_path, None, None).unwrap();

    assert!(output.contains("   1 | pub fn line_1() {}"));
    assert!(output.contains(" 200 | pub fn line_200() {}"));
    assert!(
        !output.contains(" 201 | pub fn line_201() {}"),
        "Should not display past line 200 on unbounded read"
    );
    assert!(
        output.contains("[... File has 300 lines. Showing lines 1-200. Call read_file with start_line=201 to view next chunk ...]"),
        "Must append pagination notice"
    );
}

#[tokio::test]
async fn test_exec_cmd_writes_last_exec_log_on_oversized_output() {
    let temp = tempdir().unwrap();
    let workspace = temp.path();

    // Generate command producing 150 lines of output (> DONUT_STANDARD_THRESHOLD_LINES = 120)
    let cmd = "for i in $(seq 1 150); do echo \"Log stream line $i\"; done";
    let out = exec_cmd(workspace, cmd, Some(5)).await.unwrap();

    let log_file = workspace
        .join(".minicode")
        .join("logs")
        .join("last_exec.log");
    assert!(log_file.exists(), "last_exec.log must be created on disk");

    let saved_content = std::fs::read_to_string(&log_file).unwrap();
    assert!(saved_content.contains("Log stream line 1"));
    assert!(saved_content.contains("Log stream line 150"));

    // Verify recovery notice appended to output
    assert!(
        out.contains("saved to .minicode/logs/last_exec.log"),
        "Execution result must contain recovery notice pointing to log file"
    );
}

#[test]
fn test_diff_middleware_decouples_display_output_from_llm_context() {
    let temp = tempdir().unwrap();
    let workspace = temp.path();
    let rel_path = "src/lib.rs";

    let initial = "pub fn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n";
    let updated = "pub fn add(a: i32, b: i32) -> i32 {\n    // Updated\n    a + b\n}\n";
    write_file(workspace, rel_path, updated).unwrap();

    let mw = DiffMiddleware;
    let args = json!({"path": rel_path});
    let result = ToolResult {
        tool_id: "call_42".to_string(),
        tool_name: "patch_file".to_string(),
        success: true,
        output: "File src/lib.rs patched successfully.".to_string(),
        display_output: String::new(),
        duration_ms: 25,
    };

    let ctx = ToolContext {
        tool_name: "patch_file",
        workspace_root: workspace,
        args: &args,
        file_before: Some(initial),
    };

    let processed = mw.after(&ctx, result);

    // LLM context message must receive clean output
    assert_eq!(
        processed.output, "File src/lib.rs patched successfully.",
        "LLM context output must not contain diff markup or MINICODE_DIFF_BLOCK"
    );

    // TUI display output must contain the diff block
    assert!(
        processed.display_output.starts_with(DIFF_MARKER),
        "TUI display output must be tagged with DIFF_MARKER"
    );
    assert!(
        processed.display_output.contains("Updated"),
        "TUI display output must contain the diff content"
    );
    assert_eq!(
        processed.display_output(),
        &processed.display_output,
        "display_output() helper must return display_output when non-empty"
    );
}

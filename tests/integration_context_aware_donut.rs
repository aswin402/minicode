use minicode::constants::{CONTEXT_WINDOW_200K, CONTEXT_WINDOW_8K};
use minicode::context::budget::donut::{
    get_active_context_limit, is_extended_context, set_active_context_limit, SmartDonutTruncator,
};
use minicode::tools::compactor::compact_tool_output_for_context;
use minicode::tools::exec::{exec_cmd, exec_cmd_with_context};
use minicode::tools::rtk_filter::RtkFilter;
use std::sync::Mutex;
use tempfile::tempdir;

static TEST_LOCK: Mutex<()> = Mutex::new(());

#[tokio::test]
async fn test_context_aware_donut_extended_preserves_250_lines_without_truncation() {
    let _guard = TEST_LOCK.lock().unwrap();
    let temp = tempdir().unwrap();
    let workspace = temp.path();

    // Configure extended context limit (e.g. Claude 3.5 Sonnet = 200k)
    set_active_context_limit(CONTEXT_WINDOW_200K);
    assert!(is_extended_context());
    assert_eq!(get_active_context_limit(), CONTEXT_WINDOW_200K);

    // Command generating 220 lines (> 120 standard threshold, but < 300 extended threshold)
    let cmd = "for i in $(seq 1 220); do echo \"Stream payload line $i\"; done";
    let out = exec_cmd(workspace, cmd, Some(5)).await.unwrap();

    let log_file = workspace
        .join(".minicode")
        .join("logs")
        .join("last_exec.log");

    assert!(
        !log_file.exists(),
        "last_exec.log should NOT be created when output is within extended threshold (220 <= 300)"
    );
    assert!(
        !out.contains("saved to .minicode/logs/last_exec.log"),
        "Should not append truncation notice when running on extended context window"
    );
    assert!(out.contains("Stream payload line 1"));
    assert!(out.contains("Stream payload line 220"));

    // Reset back to standard
    set_active_context_limit(0);
}

#[tokio::test]
async fn test_context_aware_donut_small_slm_truncates_and_saves_last_exec_log() {
    let _guard = TEST_LOCK.lock().unwrap();
    let temp = tempdir().unwrap();
    let workspace = temp.path();

    // Explicitly set SLM context (8k)
    set_active_context_limit(CONTEXT_WINDOW_8K);
    assert!(!is_extended_context());

    // Command generating 160 lines (> 120 standard threshold)
    let cmd = "for i in $(seq 1 160); do echo \"SLM diagnostic line $i\"; done";
    let out = exec_cmd(workspace, cmd, Some(5)).await.unwrap();

    let log_file = workspace
        .join(".minicode")
        .join("logs")
        .join("last_exec.log");

    assert!(
        log_file.exists(),
        "last_exec.log MUST be created when output exceeds SLM standard threshold (160 > 120)"
    );
    assert!(
        out.contains("saved to .minicode/logs/last_exec.log"),
        "Truncation recovery notice must be present for SLM context"
    );

    let saved = std::fs::read_to_string(&log_file).unwrap();
    assert!(saved.contains("SLM diagnostic line 1"));
    assert!(saved.contains("SLM diagnostic line 160"));

    set_active_context_limit(0);
}

#[tokio::test]
async fn test_exec_cmd_with_explicit_context_overrides_global() {
    let _guard = TEST_LOCK.lock().unwrap();
    let temp = tempdir().unwrap();
    let workspace = temp.path();

    // Global is set to 8k
    set_active_context_limit(CONTEXT_WINDOW_8K);

    // Call with explicit 200k context override
    let cmd = "for i in $(seq 1 200); do echo \"Explicit line $i\"; done";
    let out = exec_cmd_with_context(workspace, cmd, Some(5), Some(CONTEXT_WINDOW_200K))
        .await
        .unwrap();

    let log_file = workspace
        .join(".minicode")
        .join("logs")
        .join("last_exec.log");

    assert!(
        !log_file.exists(),
        "Explicit context parameter must override global setting"
    );
    assert!(!out.contains("last_exec.log"));

    set_active_context_limit(0);
}

#[test]
fn test_smart_donut_extended_truncation_over_300_lines() {
    let mut lines = Vec::new();
    for i in 1..=450 {
        if i == 180 {
            lines.push("error[E0308]: mismatched types in compiler output".to_string());
        } else if i == 250 {
            lines.push("FAILED test_heavy_stress ...".to_string());
        } else {
            lines.push(format!("Build step progress trace line {}", i));
        }
    }
    let total_count = lines.len();
    let input = lines.join("\n");

    let res = SmartDonutTruncator::truncate_for_context(&input, CONTEXT_WINDOW_200K);

    assert!(res.was_truncated);
    assert_eq!(res.original_lines, total_count);
    // Extended preserves 100 head + 200 tail = 300 lines preserved; 150 omitted
    assert_eq!(res.omitted_lines, total_count - 300);
    assert!(
        res.extracted_error_count >= 2,
        "Should extract compiler errors from the omitted middle section"
    );

    assert!(res.content.contains("error[E0308]: mismatched types"));
    assert!(res.content.contains("FAILED test_heavy_stress"));
    assert!(res.content.contains(&format!(
        "Smart Donut Truncation: Omitted {} lines",
        res.omitted_lines
    )));
    assert!(res.content.contains("Extracted 2 diagnostic/error lines"));
}

#[test]
fn test_rtk_filter_for_context_scaling() {
    let mut lines = Vec::new();
    for i in 1..=220 {
        lines.push(format!("Generic server telemetry line {}", i));
    }
    let input = lines.join("\n");

    // Extended context (200k) should NOT truncate 220 lines
    let extended_res =
        RtkFilter::filter_for_context("run_server", &input, Some(0), CONTEXT_WINDOW_200K);
    assert_eq!(extended_res.filtered_lines, 220);
    assert_eq!(extended_res.saved_pct, 0.0);

    // Standard SLM context (8k) MUST truncate 220 lines
    let slm_res = RtkFilter::filter_for_context("run_server", &input, Some(0), CONTEXT_WINDOW_8K);
    assert!(slm_res.filtered_lines < 220);
    assert!(slm_res.saved_pct > 0.0);
}

#[test]
fn test_compactor_git_diff_scales_with_context_window() {
    let mut diff_lines = Vec::new();
    diff_lines.push("diff --git a/file.rs b/file.rs".to_string());
    diff_lines.push("--- a/file.rs".to_string());
    diff_lines.push("+++ b/file.rs".to_string());
    diff_lines.push("@@ -1,150 +1,150 @@".to_string());
    for i in 1..=150 {
        diff_lines.push(format!(" context line {}", i));
    }
    let input = diff_lines.join("\n");

    // Under standard context (8k), diff is folded because lines > 100
    let compacted_slm =
        compact_tool_output_for_context("git diff", &input, None, CONTEXT_WINDOW_8K);
    assert!(
        compacted_slm.contains("..."),
        "Standard context must fold long diff hunks over 100 lines"
    );

    // Under extended context (200k), diff is NOT folded because lines (154) <= 300
    let compacted_ext =
        compact_tool_output_for_context("git diff", &input, None, CONTEXT_WINDOW_200K);
    assert!(
        !compacted_ext.contains("..."),
        "Extended context must preserve full diff up to 300 lines without premature folding"
    );
}

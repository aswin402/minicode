use minicode::agent::prompt::PromptBuilder;
use minicode::context::budget::ContextBudget;
use minicode::context::compressor::ContextCompressor;
use minicode::context::donut::SmartDonutTruncator;
use tempfile::tempdir;

#[test]
fn test_context_budget_metrics_and_advice() {
    // 1. Healthy headroom (< 60%)
    let b_healthy = ContextBudget::new(30_000, 100_000, 45_000);
    assert_eq!(b_healthy.headroom_tokens(), 70_000);
    assert!((b_healthy.percentage() - 30.0).abs() < f64::EPSILON);
    assert!(b_healthy.advice().contains("HEALTHY"));

    // 2. Moderate pressure (60% - 80%)
    let b_mod = ContextBudget::new(70_000, 100_000, 95_000);
    assert_eq!(b_mod.headroom_tokens(), 30_000);
    assert!((b_mod.percentage() - 70.0).abs() < f64::EPSILON);
    assert!(b_mod.advice().contains("WARNING"));

    // 3. Critical pressure (>= 80%)
    let b_crit = ContextBudget::new(85_000, 100_000, 120_000);
    assert_eq!(b_crit.headroom_tokens(), 15_000);
    assert!((b_crit.percentage() - 85.0).abs() < f64::EPSILON);
    assert!(b_crit.advice().contains("CRITICAL"));

    // 4. Saturated / Over-budget
    let b_sat = ContextBudget::new(105_000, 100_000, 150_000);
    assert_eq!(b_sat.headroom_tokens(), 0);
    assert!(b_sat.advice().contains("CRITICAL"));
}

#[test]
fn test_context_budget_progress_bar_render() {
    let b0 = ContextBudget::new(0, 100_000, 0);
    assert_eq!(b0.render_progress_bar(20), "[░░░░░░░░░░░░░░░░░░░░]");

    let b25 = ContextBudget::new(25_000, 100_000, 0);
    assert_eq!(b25.render_progress_bar(20), "[█████░░░░░░░░░░░░░░░]");

    let b50 = ContextBudget::new(50_000, 100_000, 0);
    assert_eq!(b50.render_progress_bar(20), "[██████████░░░░░░░░░░]");

    let b100 = ContextBudget::new(100_000, 100_000, 0);
    assert_eq!(b100.render_progress_bar(20), "[████████████████████]");
}

#[test]
fn test_context_budget_recency_context_injection() {
    let temp = tempdir().unwrap();
    let workspace = temp.path();

    let budget = ContextBudget::new(42_500, 128_000, 68_000);
    let working_set = vec!["src/lib.rs".to_string()];

    let recency =
        PromptBuilder::build_recency_context(workspace, None, &working_set, None, Some(&budget));

    assert!(recency.contains("<workspace_context>"));
    assert!(recency.contains("<context_budget used=\"42500\" limit=\"128000\""));
    assert!(recency.contains("pct=\"33.2%\""));
    assert!(recency.contains("headroom=\"85500\""));
    assert!(recency.contains("cumulative=\"68000\""));
    assert!(recency.contains("HEALTHY: Ample context headroom available"));
    assert!(recency.contains("</context_budget>"));
    assert!(recency.contains("</workspace_context>"));
}

#[test]
fn test_smart_donut_under_threshold_untouched() {
    let mut lines = Vec::new();
    for i in 1..=150 {
        lines.push(format!("Cargo test stdout line {}", i));
    }
    let input = lines.join("\n");

    let res = SmartDonutTruncator::truncate_with_result(&input);
    assert!(!res.was_truncated);
    assert_eq!(res.omitted_lines, 0);
    assert_eq!(res.content, input);
}

#[test]
fn test_smart_donut_truncation_structure_and_bounds() {
    let mut lines = Vec::new();
    for i in 1..=500 {
        lines.push(format!("Build stream info line {}", i));
    }
    let input = lines.join("\n");

    let res = SmartDonutTruncator::truncate_with_result(&input);
    assert!(res.was_truncated);
    assert_eq!(res.original_lines, 500);
    // Head = 100, Tail = 200, Omitted = 500 - (100 + 200) = 200 lines
    assert_eq!(res.omitted_lines, 200);
    assert_eq!(res.extracted_error_count, 0);

    // Verify head lines are preserved
    assert!(res.content.starts_with("Build stream info line 1"));
    assert!(res.content.contains("Build stream info line 100"));

    // Verify middle omission banner
    assert!(res.content.contains("[... Smart Donut Truncation: Omitted 200 lines (lines 101 to 300) — zero errors detected in omitted section ...]"));

    // Verify tail lines are preserved
    assert!(res.content.contains("Build stream info line 301"));
    assert!(res.content.ends_with("Build stream info line 500"));
}

#[test]
fn test_smart_donut_extracts_critical_middle_errors_with_location_pointers() {
    let mut lines = Vec::new();
    for i in 1..=600 {
        lines.push(format!("Normal compiling crate dependency step {}", i));
    }
    // Set exact lines with 1-based indices
    lines[141] = "error[E0432]: unresolved import `crate::foo::bar`".to_string(); // Line 142
    lines[142] = "  --> src/agent/mod.rs:14:5".to_string(); // Line 143
    lines[249] = "thread 'main' panicked at 'assertion failed: index < len', src/buffer.rs:88:12"
        .to_string(); // Line 250
    lines[309] =
        "fatal: unable to access 'https://github.com/repo': Could not resolve host".to_string(); // Line 310

    let input = lines.join("\n");

    let res = SmartDonutTruncator::truncate_with_result(&input);
    assert!(res.was_truncated);
    assert_eq!(res.original_lines, 600);
    assert_eq!(res.omitted_lines, 300); // 600 - (100 + 200)
    assert!(res.extracted_error_count >= 3);

    // Verify exact line numbers and error text in omitted middle section
    assert!(res
        .content
        .contains("Line 142: error[E0432]: unresolved import `crate::foo::bar`"));
    assert!(res
        .content
        .contains("Line 143:   --> src/agent/mod.rs:14:5"));
    assert!(res
        .content
        .contains("Line 250: thread 'main' panicked at 'assertion failed"));
    assert!(res.content.contains("Line 310: fatal: unable to access"));
    assert!(res.content.contains("[End of extracted diagnostics]"));
}

#[test]
fn test_smart_donut_wide_character_clamping_safe_unicode() {
    let long_emoji_string = "🚀🔥✨".repeat(1500); // 4500 emojis = 18,000 bytes
    let clamped = SmartDonutTruncator::clamp_line_width(&long_emoji_string);

    assert!(clamped.contains("[... line clamped at 2000 chars ...]"));
    assert!(clamped.chars().count() > 2000);
}

#[test]
fn test_compressor_mask_observation_uses_donut_and_extracts_errors() {
    let mut long_output = String::new();
    for i in 1..=200 {
        if i == 80 {
            long_output.push_str("error: failed to load module 'config'\n");
        } else {
            long_output.push_str(&format!("Standard verbose terminal output line {}\n", i));
        }
    }

    let masked = ContextCompressor::mask_observation(&long_output, 30);
    assert!(masked.contains("Standard verbose terminal output line 1"));
    assert!(masked.contains("error: failed to load module 'config'"));
    assert!(masked.contains("Standard verbose terminal output line 200"));
}

use crate::constants::{
    GENERIC_COMPACT_THRESHOLD, GENERIC_HEAD_LINES, GENERIC_TAIL_LINES, GIT_DIFF_COMPACT_THRESHOLD,
    GIT_LOG_MAX_LINES,
};

/// Strips ANSI color and control escape sequences from text using the shared state machine.
pub fn strip_ansi_codes(input: &str) -> String {
    crate::utils::strings::strip_ansi_codes(input)
}

/// Compact strategy determined by the executed command string
#[derive(Debug, PartialEq, Eq)]
pub enum CompactStrategy {
    CargoCheck,
    CargoTest,
    GitDiff,
    GitLog,
    Npm,
    Pytest,
    GoTest,
    Generic,
}

/// Token compaction statistics
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub struct CompactionStats {
    pub raw_bytes: usize,
    pub compacted_bytes: usize,
    pub savings_percent: usize,
}

/// Helper to extract the primary substantive command from chained/prefixed shell strings
/// (e.g. `cd /some/path && npm run build 2>&1` -> `npm run build`, `RUST_LOG=info cargo check` -> `cargo check`)
pub fn extract_primary_command(command: &str) -> &str {
    let mut s = command.trim();

    // Split on shell separators `&&`, `||`, or `;` and take the last command segment
    if let Some(last_segment) = s.rsplit("&&").next() {
        s = last_segment.trim();
    }
    if let Some(last_segment) = s.rsplit("||").next() {
        s = last_segment.trim();
    }
    if let Some(last_segment) = s.rsplit(';').next() {
        s = last_segment.trim();
    }

    // Strip trailing redirections like `2>&1`
    if let Some(pos) = s.rfind("2>&1") {
        s = s[..pos].trim();
    }

    // Strip leading environment variable assignments like `FOO=bar ` or `VAR=1 `
    while let Some(first) = s.split_whitespace().next() {
        if first.contains('=') && !first.starts_with('-') && !first.starts_with('/') {
            if let Some(rest) = s.strip_prefix(first) {
                s = rest.trim_start();
                continue;
            }
        }
        break;
    }

    s
}

/// Detects the compaction strategy for a given command line
pub fn detect_strategy(command: &str) -> CompactStrategy {
    let primary = extract_primary_command(command);
    let parts: Vec<&str> = primary.split_whitespace().collect();

    if parts.is_empty() {
        return CompactStrategy::Generic;
    }

    match parts[0] {
        "cargo" => {
            for &arg in &parts[1..] {
                if arg.starts_with('-') || arg.starts_with('+') {
                    continue;
                }
                match arg {
                    "test" => return CompactStrategy::CargoTest,
                    "check" | "build" | "clippy" | "run" => return CompactStrategy::CargoCheck,
                    _ => break,
                }
            }
            CompactStrategy::Generic
        }
        "git" => {
            let mut skip_next = false;
            for &arg in &parts[1..] {
                if skip_next {
                    skip_next = false;
                    continue;
                }
                if arg == "-C" || arg == "--git-dir" || arg == "--work-tree" {
                    skip_next = true;
                    continue;
                }
                if arg.starts_with('-') {
                    continue;
                }
                match arg {
                    "diff" | "show" => return CompactStrategy::GitDiff,
                    "log" => return CompactStrategy::GitLog,
                    _ => break,
                }
            }
            CompactStrategy::Generic
        }
        "npm" | "yarn" | "pnpm" | "bun" => CompactStrategy::Npm,
        "pytest" | "python" | "python3" => {
            if primary.contains("pytest") || primary.contains("unittest") {
                CompactStrategy::Pytest
            } else {
                CompactStrategy::Generic
            }
        }
        "go" => {
            if parts.get(1) == Some(&"test") {
                CompactStrategy::GoTest
            } else {
                CompactStrategy::Generic
            }
        }
        _ => {
            if primary.starts_with("pytest") {
                CompactStrategy::Pytest
            } else {
                CompactStrategy::Generic
            }
        }
    }
}

/// Main entrypoint: strips ANSI escape sequences and applies exit-code-aware compaction using active context limit.
pub fn compact_tool_output(command: &str, raw_output: &str, exit_code: Option<i32>) -> String {
    let context_window = crate::context::donut::get_active_context_limit();
    compact_tool_output_for_context(command, raw_output, exit_code, context_window)
}

/// Main entrypoint with explicit context window limit.
pub fn compact_tool_output_for_context(
    command: &str,
    raw_output: &str,
    exit_code: Option<i32>,
    context_window: usize,
) -> String {
    let clean = strip_ansi_codes(raw_output);
    let strategy = detect_strategy(command);

    match strategy {
        CompactStrategy::CargoCheck => compact_cargo_check(&clean, exit_code, context_window),
        CompactStrategy::CargoTest => compact_cargo_test(&clean, exit_code, context_window),
        CompactStrategy::GitDiff => compact_git_diff(&clean, context_window),
        CompactStrategy::GitLog => compact_git_log(&clean),
        CompactStrategy::Npm => compact_npm(&clean, exit_code, context_window),
        CompactStrategy::Pytest => compact_pytest(&clean, exit_code, context_window),
        CompactStrategy::GoTest => compact_go_test(&clean, exit_code, context_window),
        CompactStrategy::Generic => compact_generic(&clean, context_window),
    }
}

/// Calculates token compaction savings metrics
#[allow(dead_code)]
pub fn calculate_compaction_stats(raw: &str, compacted: &str) -> CompactionStats {
    let raw_bytes = raw.len();
    let compacted_bytes = compacted.len();
    let savings_percent = if raw_bytes > 0 && raw_bytes >= compacted_bytes {
        ((raw_bytes - compacted_bytes) * 100) / raw_bytes
    } else {
        0
    };

    CompactionStats {
        raw_bytes,
        compacted_bytes,
        savings_percent,
    }
}

/// Compacts `cargo check`, `cargo build`, `cargo clippy` output with `cargo-pretty` summary metrics.
fn compact_cargo_check(output: &str, exit_code: Option<i32>, context_window: usize) -> String {
    let is_success =
        exit_code == Some(0) && !output.contains("error:") && !output.contains("error[");

    let mut libs_compiled = 0usize;
    let mut warning_messages: Vec<String> = Vec::new();
    let mut pending_warning: Option<String> = None;

    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("Compiling ") || trimmed.starts_with("Checking ") {
            libs_compiled += 1;
        } else if let Some(msg) = trimmed.strip_prefix("warning: ") {
            if !msg.contains("generated ") && !msg.starts_with('`') {
                if let Some(prev) = pending_warning.take() {
                    warning_messages.push(prev);
                }
                pending_warning = Some(msg.to_string());
            }
        } else if trimmed.starts_with("--> ") {
            if let Some(warn_msg) = pending_warning.take() {
                let loc = trimmed.trim_start_matches("--> ").trim();
                warning_messages.push(format!("{} ({})", warn_msg, loc));
            }
        }
    }
    if let Some(prev) = pending_warning.take() {
        warning_messages.push(prev);
    }

    if is_success {
        for line in output.lines().rev() {
            let trimmed = line.trim();
            if trimmed.starts_with("Finished") {
                let mut summary = if warning_messages.is_empty() {
                    format!("✔ {}", trimmed)
                } else {
                    format!(
                        "✔ {} ({} warning{})",
                        trimmed,
                        warning_messages.len(),
                        if warning_messages.len() == 1 { "" } else { "s" }
                    )
                };
                if libs_compiled > 0 {
                    summary.push_str(&format!("\n  libs compiled  {}", libs_compiled));
                }
                for warn in warning_messages.iter().take(3) {
                    summary.push_str(&format!("\n  ⚠ {}", warn));
                }
                if warning_messages.len() > 3 {
                    summary.push_str(&format!(
                        "\n  ⚠ ... +{} more warnings",
                        warning_messages.len() - 3
                    ));
                }
                return summary;
            }
        }
        // Fallback for short successful output
        return output.lines().take(5).collect::<Vec<_>>().join("\n");
    }

    // On failure: filter out "Compiling ...", "Downloading ...", "Downloaded ...", "Checking ..."
    let mut relevant_lines = Vec::new();
    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("Compiling ")
            || trimmed.starts_with("Downloading ")
            || trimmed.starts_with("Downloaded ")
            || trimmed.starts_with("Checking ")
            || trimmed.starts_with("Locking ")
            || trimmed.starts_with("Updating ")
        {
            continue;
        }
        relevant_lines.push(line);
    }

    if relevant_lines.is_empty() {
        compact_generic(output, context_window)
    } else if libs_compiled > 0 {
        format!(
            "✗ build failed after compiling {} crate(s)\n{}",
            libs_compiled,
            relevant_lines.join("\n")
        )
    } else {
        relevant_lines.join("\n")
    }
}

/// Compacts `cargo test` output with `cargo-pretty` panic and `left:` / `right:` diff highlighting.
fn compact_cargo_test(output: &str, exit_code: Option<i32>, context_window: usize) -> String {
    let is_success = exit_code == Some(0)
        && !output.contains("test result: FAILED")
        && !output.contains("... FAILED");

    if is_success {
        let mut summaries = Vec::new();
        for line in output.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("test result: ok.") {
                summaries.push(trimmed.to_string());
            }
        }
        if !summaries.is_empty() {
            return format!("✔ {}", summaries.join("\n✔ "));
        }
    }

    // On failure: extract failures section, panic locations, left/right diffs, and summary lines
    let mut failure_lines = Vec::new();
    let mut in_failures = false;
    let mut after_panic = false;

    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("failures:") {
            in_failures = true;
        }
        let is_panic_line = trimmed.contains("panicked at");
        let is_diff_line = trimmed.starts_with("left:")
            || trimmed.starts_with("right:")
            || trimmed.starts_with("assertion ");
        if in_failures
            || is_panic_line
            || after_panic
            || is_diff_line
            || trimmed.starts_with("test result: FAILED")
            || trimmed.ends_with("... FAILED")
            || trimmed.starts_with("error:")
            || trimmed.starts_with("error[")
        {
            failure_lines.push(line);
        }
        after_panic = is_panic_line;
    }

    if !in_failures && (output.contains("error[") || output.contains("error: could not compile")) {
        return compact_cargo_check(output, exit_code, context_window);
    }

    if !failure_lines.is_empty() {
        failure_lines.join("\n")
    } else {
        compact_generic(output, context_window)
    }
}

/// Compacts `pytest` / `unittest` Python output.
fn compact_pytest(output: &str, exit_code: Option<i32>, context_window: usize) -> String {
    let is_success = exit_code == Some(0)
        && (output.contains("passed") || output.contains("OK"))
        && !output.contains("FAILED")
        && !output.contains("ERROR");

    if is_success {
        for line in output.lines().rev() {
            let trimmed = line.trim();
            if trimmed.starts_with("===") && trimmed.contains("passed") {
                return format!("✔ {}", trimmed.trim_matches('='));
            }
            if trimmed.starts_with("OK") {
                return format!("✔ {}", trimmed);
            }
        }
    }

    // On failure: extract FAILURES / ERRORS block and summary
    let mut relevant = Vec::new();
    let mut in_failure_section = false;

    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("=== FAILURES ===") || trimmed.starts_with("=== ERRORS ===") {
            in_failure_section = true;
        }
        if in_failure_section
            || trimmed.starts_with("FAILED ")
            || trimmed.starts_with("ERROR ")
            || (trimmed.starts_with("===") && trimmed.contains("failed"))
        {
            relevant.push(line);
        }
    }

    if !relevant.is_empty() {
        relevant.join("\n")
    } else {
        compact_generic(output, context_window)
    }
}

/// Compacts `go test` output.
fn compact_go_test(output: &str, exit_code: Option<i32>, context_window: usize) -> String {
    let is_success = exit_code == Some(0) && !output.contains("FAIL");

    if is_success {
        let mut passed_pkgs = Vec::new();
        for line in output.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("ok  ") {
                passed_pkgs.push(trimmed.to_string());
            }
        }
        if !passed_pkgs.is_empty() {
            return format!("✔ {}", passed_pkgs.join("\n✔ "));
        }
    }

    // On failure: extract failing tests
    let mut failure_lines = Vec::new();
    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("--- FAIL:")
            || trimmed.starts_with("FAIL")
            || trimmed.contains("error:")
        {
            failure_lines.push(line);
        }
    }

    if !failure_lines.is_empty() {
        failure_lines.join("\n")
    } else {
        compact_generic(output, context_window)
    }
}

/// Compacts `git diff` / `git show` output with fast hunk folding.
fn compact_git_diff(output: &str, context_window: usize) -> String {
    let lines: Vec<&str> = output.lines().collect();
    let threshold = if context_window >= crate::constants::CONTEXT_WINDOW_128K {
        crate::constants::DONUT_EXTENDED_THRESHOLD_LINES
    } else {
        GIT_DIFF_COMPACT_THRESHOLD
    };

    if lines.len() <= threshold {
        return output.to_string();
    }

    let mut compacted = Vec::new();
    let mut unchanged_count = 0;

    for line in lines {
        if line.starts_with('+')
            || line.starts_with('-')
            || line.starts_with('@')
            || line.starts_with("diff ")
        {
            unchanged_count = 0;
            compacted.push(line);
        } else {
            unchanged_count += 1;
            if unchanged_count <= 2 {
                compacted.push(line);
            } else if unchanged_count == 3 {
                compacted.push("    ...");
            }
        }
    }

    compacted.join("\n")
}

/// Compacts `git log` output to first commits.
fn compact_git_log(output: &str) -> String {
    let mut iter = output.lines();
    let top: Vec<&str> = iter.by_ref().take(GIT_LOG_MAX_LINES).collect();
    if iter.next().is_none() {
        return output.to_string();
    }
    format!("{}\n\n... [Older commits truncated] ...", top.join("\n"))
}

/// Compacts npm/yarn/pnpm/bun output.
fn compact_npm(output: &str, exit_code: Option<i32>, context_window: usize) -> String {
    let is_success = exit_code == Some(0);
    if is_success {
        for line in output.lines().rev() {
            let trimmed = line.trim();
            if trimmed.starts_with("added ")
                || trimmed.starts_with("up to date")
                || trimmed.contains("passed")
            {
                return format!("✔ {}", trimmed);
            }
        }
    }
    compact_generic(output, context_window)
}

/// Generic fallback: delegates to SmartDonutTruncator as the unified authority.
fn compact_generic(output: &str, context_window: usize) -> String {
    if context_window > 0 {
        crate::context::budget::donut::SmartDonutTruncator::truncate_for_context(
            output,
            context_window,
        )
        .content
    } else {
        crate::context::budget::donut::SmartDonutTruncator::truncate_custom(
            output,
            GENERIC_COMPACT_THRESHOLD,
            GENERIC_HEAD_LINES,
            GENERIC_TAIL_LINES,
            crate::constants::DONUT_STANDARD_MAX_ERROR_LINES,
        )
        .content
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pytest_compaction() {
        let pass_out = "============================= test session starts ==============================\n... [100%]\n============================== 14 passed in 0.42s ==============================\n";
        let res = compact_tool_output("pytest tests/", pass_out, Some(0));
        assert!(res.starts_with("✔"));
        assert!(res.contains("14 passed in 0.42s"));

        let fail_out = "=== FAILURES ===\n___ test_math ___\nAssertionError: 1 != 2\n=== 1 failed, 13 passed in 0.50s ===";
        let fail_res = compact_tool_output("pytest", fail_out, Some(1));
        assert!(fail_res.contains("=== FAILURES ==="));
        assert!(fail_res.contains("AssertionError"));
    }

    #[test]
    fn test_go_test_compaction() {
        let pass_out =
            "ok  github.com/example/pkg/core 0.045s\nok  github.com/example/pkg/utils 0.012s\n";
        let res = compact_tool_output("go test ./...", pass_out, Some(0));
        assert!(res.starts_with("✔"));
        assert!(res.contains("github.com/example/pkg/core"));

        let fail_out = "--- FAIL: TestAdd (0.00s)\n    math_test.go:12: expected 3, got 4\nFAIL\nFAIL\tgithub.com/example/pkg/core\t0.015s\n";
        let fail_res = compact_tool_output("go test ./...", fail_out, Some(1));
        assert!(fail_res.contains("--- FAIL: TestAdd"));
    }

    #[test]
    fn test_compaction_stats() {
        let raw = "a".repeat(1000);
        let compacted = "a".repeat(100);
        let stats = calculate_compaction_stats(&raw, &compacted);
        assert_eq!(stats.raw_bytes, 1000);
        assert_eq!(stats.compacted_bytes, 100);
        assert_eq!(stats.savings_percent, 90);
    }

    #[test]
    fn test_chained_command_strategy_detection() {
        assert_eq!(
            detect_strategy("cd /path/to/project && npm run build 2>&1"),
            CompactStrategy::Npm
        );
        assert_eq!(
            detect_strategy("cd /path && cargo check"),
            CompactStrategy::CargoCheck
        );
        assert_eq!(
            detect_strategy("RUST_BACKTRACE=1 cargo test --lib"),
            CompactStrategy::CargoTest
        );
        assert_eq!(detect_strategy("git diff HEAD~1"), CompactStrategy::GitDiff);
    }

    #[test]
    fn test_cargo_pretty_check_and_test_compaction() {
        let build_out = "\
   Compiling serde v1.0.210
   Compiling tokio v1.40.0
warning: unused variable: `x`
  --> src/main.rs:14:9
   |
14 |     let x = 42;
   |         ^ help: if this is intentional, prefix it with an underscore: `_x`
   Compiling minicode v0.3.95 (/workspace/minicode)
    Finished `release` profile [optimized] target(s) in 12.34s
";
        let res = compact_tool_output("cargo build --release", build_out, Some(0));
        assert!(res.starts_with("✔ Finished `release` profile"));
        assert!(res.contains("(1 warning)"));
        assert!(res.contains("libs compiled  3"));
        assert!(res.contains("⚠ unused variable: `x` (src/main.rs:14:9)"));

        let test_fail_out = "\
running 2 tests
test tests::it_works ... ok
test tests::it_fails ... FAILED

failures:

---- tests::it_fails stdout ----
thread 'tests::it_fails' panicked at src/lib.rs:20:5:
assertion `left == right` failed
  left: 42
 right: 99

failures:
    tests::it_fails

test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
";
        let fail_res = compact_tool_output("cargo test", test_fail_out, Some(101));
        assert!(fail_res.contains("panicked at src/lib.rs:20:5:"));
        assert!(fail_res.contains("left: 42"));
        assert!(fail_res.contains("right: 99"));
        assert!(fail_res.contains("test result: FAILED."));
    }
}

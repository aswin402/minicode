use crate::constants::{
    GENERIC_COMPACT_THRESHOLD, GENERIC_HEAD_LINES, GENERIC_TAIL_LINES, GIT_DIFF_COMPACT_THRESHOLD,
    GIT_LOG_MAX_LINES,
};
use regex::Regex;
use std::sync::OnceLock;

/// Strips ANSI color and control escape sequences from text.
pub fn strip_ansi_codes(input: &str) -> String {
    static ANSI_REGEX: OnceLock<Option<Regex>> = OnceLock::new();
    let re_opt = ANSI_REGEX.get_or_init(|| {
        match Regex::new(r"\x1B\[[0-9;]*[a-zA-Z]|\x1B\].*?(\x07|\x1B\\)|\x1B[()][A-Za-z0-9]") {
            Ok(re) => Some(re),
            Err(e) => {
                tracing::error!(error = %e, "Failed to compile ANSI strip regex");
                None
            }
        }
    });
    match re_opt {
        Some(re) => re.replace_all(input, "").to_string(),
        None => input.to_string(),
    }
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

/// Detects the compaction strategy for a given command line
pub fn detect_strategy(command: &str) -> CompactStrategy {
    let trimmed = command.trim();
    let parts: Vec<&str> = trimmed.split_whitespace().collect();

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
            if trimmed.contains("pytest") || trimmed.contains("unittest") {
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
            if trimmed.starts_with("pytest") {
                CompactStrategy::Pytest
            } else {
                CompactStrategy::Generic
            }
        }
    }
}

/// Main entrypoint: strips ANSI escape sequences and applies exit-code-aware compaction.
pub fn compact_tool_output(command: &str, raw_output: &str, exit_code: Option<i32>) -> String {
    let clean = strip_ansi_codes(raw_output);
    let strategy = detect_strategy(command);

    match strategy {
        CompactStrategy::CargoCheck => compact_cargo_check(&clean, exit_code),
        CompactStrategy::CargoTest => compact_cargo_test(&clean, exit_code),
        CompactStrategy::GitDiff => compact_git_diff(&clean),
        CompactStrategy::GitLog => compact_git_log(&clean),
        CompactStrategy::Npm => compact_npm(&clean, exit_code),
        CompactStrategy::Pytest => compact_pytest(&clean, exit_code),
        CompactStrategy::GoTest => compact_go_test(&clean, exit_code),
        CompactStrategy::Generic => compact_generic(&clean),
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

/// Compacts `cargo check`, `cargo build`, `cargo clippy` output.
fn compact_cargo_check(output: &str, exit_code: Option<i32>) -> String {
    let is_success =
        exit_code == Some(0) && !output.contains("error:") && !output.contains("error[");

    if is_success {
        for line in output.lines().rev() {
            let trimmed = line.trim();
            if trimmed.starts_with("Finished") {
                return format!("✔ {}", trimmed);
            }
        }
        // Fallback for short successful output
        return output.lines().take(5).collect::<Vec<_>>().join("\n");
    }

    // On failure: filter out "Compiling ...", "Downloading ...", "Downloaded ..."
    let mut relevant_lines = Vec::new();
    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("Compiling ")
            || trimmed.starts_with("Downloading ")
            || trimmed.starts_with("Downloaded ")
            || trimmed.starts_with("Checking ")
        {
            continue;
        }
        relevant_lines.push(line);
    }

    if relevant_lines.is_empty() {
        compact_generic(output)
    } else {
        relevant_lines.join("\n")
    }
}

/// Compacts `cargo test` output.
fn compact_cargo_test(output: &str, exit_code: Option<i32>) -> String {
    let is_success = exit_code == Some(0)
        && !output.contains("test result: FAILED")
        && !output.contains("FAILED");

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

    // On failure: extract failures section and summary lines
    let mut failure_lines = Vec::new();
    let mut in_failures = false;

    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("failures:") {
            in_failures = true;
        }
        if in_failures
            || trimmed.starts_with("test result: FAILED")
            || trimmed.contains("FAILED")
            || trimmed.starts_with("error:")
            || trimmed.starts_with("error[")
        {
            failure_lines.push(line);
        }
    }

    if !failure_lines.is_empty() {
        failure_lines.join("\n")
    } else {
        compact_generic(output)
    }
}

/// Compacts `pytest` / `unittest` Python output.
fn compact_pytest(output: &str, exit_code: Option<i32>) -> String {
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
        compact_generic(output)
    }
}

/// Compacts `go test` output.
fn compact_go_test(output: &str, exit_code: Option<i32>) -> String {
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
        compact_generic(output)
    }
}

/// Compacts `git diff` / `git show` output with fast hunk folding.
fn compact_git_diff(output: &str) -> String {
    let lines: Vec<&str> = output.lines().collect();
    if lines.len() <= GIT_DIFF_COMPACT_THRESHOLD {
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
    let lines: Vec<&str> = output.lines().collect();
    if lines.len() <= GIT_LOG_MAX_LINES {
        return output.to_string();
    }
    let top: Vec<&str> = lines.into_iter().take(GIT_LOG_MAX_LINES).collect();
    format!("{}\n\n... [Older commits truncated] ...", top.join("\n"))
}

/// Compacts npm/yarn/pnpm/bun output.
fn compact_npm(output: &str, exit_code: Option<i32>) -> String {
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
    compact_generic(output)
}

/// Generic fallback: keeps head + tail lines if output exceeds threshold.
fn compact_generic(output: &str) -> String {
    let lines: Vec<&str> = output.lines().collect();
    if lines.len() <= GENERIC_COMPACT_THRESHOLD {
        return output.to_string();
    }

    let head: Vec<&str> = lines.iter().take(GENERIC_HEAD_LINES).copied().collect();
    let tail: Vec<&str> = lines
        .iter()
        .skip(lines.len().saturating_sub(GENERIC_TAIL_LINES))
        .copied()
        .collect();

    let omitted_count = lines
        .len()
        .saturating_sub(GENERIC_HEAD_LINES + GENERIC_TAIL_LINES);

    format!(
        "{}\n\n... [{} lines omitted for brevity] ...\n\n{}",
        head.join("\n"),
        omitted_count,
        tail.join("\n")
    )
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
}

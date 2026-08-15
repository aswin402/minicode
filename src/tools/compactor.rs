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
    Generic,
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
        _ => CompactStrategy::Generic,
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
        CompactStrategy::Generic => compact_generic(&clean),
    }
}

/// Compacts `cargo check`, `cargo build`, `cargo clippy` output.
/// If successful, condenses to the final `Finished` line.
/// If failed, strips noise compiling lines and keeps `error[...]` diagnostics.
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
/// If successful, condenses to summary lines (e.g. `test result: ok. 18 passed; 0 failed`).
/// If failed, extracts failing tests, panic messages, and the summary.
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

/// Compacts `git diff` / `git show` output.
fn compact_git_diff(output: &str) -> String {
    let lines: Vec<&str> = output.lines().collect();
    if lines.len() <= GIT_DIFF_COMPACT_THRESHOLD {
        return output.to_string();
    }

    // Keep diff headers and changes, limit unchanged lines
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

    let total = lines.len();
    let head = &lines[..GENERIC_HEAD_LINES.min(total)];
    let tail = if total >= GENERIC_TAIL_LINES {
        &lines[total - GENERIC_TAIL_LINES..]
    } else {
        &[]
    };
    let omitted = total.saturating_sub(GENERIC_HEAD_LINES + GENERIC_TAIL_LINES);

    format!(
        "{}\n\n... [{} lines omitted by minicode compactor] ...\n\n{}",
        head.join("\n"),
        omitted,
        tail.join("\n")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_ansi_codes() {
        let colored =
            "\x1B[31mError:\x1B[0m something went wrong \x1B[38;2;162;119;255m[E0308]\x1B[0m";
        let clean = strip_ansi_codes(colored);
        assert_eq!(clean, "Error: something went wrong [E0308]");
    }

    #[test]
    fn test_detect_strategy() {
        assert_eq!(
            detect_strategy("cargo check --all"),
            CompactStrategy::CargoCheck
        );
        assert_eq!(
            detect_strategy("cargo clippy -- -D warnings"),
            CompactStrategy::CargoCheck
        );
        assert_eq!(
            detect_strategy("cargo test --lib"),
            CompactStrategy::CargoTest
        );
        assert_eq!(
            detect_strategy("cargo +nightly test --lib"),
            CompactStrategy::CargoTest
        );
        assert_eq!(
            detect_strategy("git -C /some/dir diff"),
            CompactStrategy::GitDiff
        );
        assert_eq!(detect_strategy("git diff HEAD~1"), CompactStrategy::GitDiff);
        assert_eq!(detect_strategy("git log -n 50"), CompactStrategy::GitLog);
        assert_eq!(detect_strategy("npm install express"), CompactStrategy::Npm);
        assert_eq!(detect_strategy("ls -la /tmp"), CompactStrategy::Generic);
    }

    #[test]
    fn test_compact_cargo_check_failure() {
        let output = "\
   Compiling minicode v0.0.3 (/path)
error[E0425]: cannot find value `xyz` in this scope
 --> src/main.rs:10:5
error: aborting due to 1 previous error";
        let compacted = compact_cargo_check(output, Some(101));
        assert!(compacted.contains("error[E0425]"));
        assert!(!compacted.starts_with("✔"));
    }

    #[test]
    fn test_compact_cargo_test_failure() {
        let output = "\
running 2 tests
test tests::test_one ... ok
test tests::test_two ... FAILED

failures:

---- tests::test_two stdout ----
thread 'tests::test_two' panicked at 'assertion failed: `(left == right)`'

failures:
    tests::test_two

test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out";
        let compacted = compact_cargo_test(output, Some(101));
        assert!(compacted.contains("FAILED"));
        assert!(compacted.contains("thread 'tests::test_two' panicked"));
        assert!(!compacted.starts_with("✔"));
    }

    #[test]
    fn test_compact_cargo_check_success() {
        let output = "\
   Compiling minicode v0.0.3 (/path)
   Compiling ratatui v0.29.0
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.45s";
        let compacted = compact_cargo_check(output, Some(0));
        assert_eq!(
            compacted,
            "✔ Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.45s"
        );
    }

    #[test]
    fn test_compact_cargo_test_success() {
        let output = "\
running 5 tests
test tests::test_one ... ok
test tests::test_two ... ok
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out";
        let compacted = compact_cargo_test(output, Some(0));
        assert!(compacted.contains("✔ test result: ok. 5 passed; 0 failed"));
    }

    #[test]
    fn test_compact_generic_truncation() {
        let mut lines = Vec::new();
        for i in 1..=100 {
            lines.push(format!("Line {}", i));
        }
        let raw = lines.join("\n");
        let compacted = compact_generic(&raw);
        assert!(compacted.contains("Line 1"));
        assert!(compacted.contains("Line 30"));
        assert!(compacted.contains("lines omitted"));
        assert!(compacted.contains("Line 100"));
        assert!(!compacted.contains("Line 50\n"));
    }
}

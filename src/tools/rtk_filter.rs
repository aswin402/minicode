use regex::Regex;
use std::sync::OnceLock;

static ANSI_REGEX: OnceLock<Option<Regex>> = OnceLock::new();

fn get_ansi_regex() -> Option<&'static Regex> {
    ANSI_REGEX
        .get_or_init(|| Regex::new(r"\x1b\[[0-9;]*[a-zA-Z]").ok())
        .as_ref()
}

/// Strip ANSI escape codes from output
pub fn strip_ansi(input: &str) -> String {
    if let Some(re) = get_ansi_regex() {
        re.replace_all(input, "").to_string()
    } else {
        input.to_string()
    }
}

/// Token-efficient filtering engine (inspired by RTK - Rust Token Killer)
pub struct RtkFilter;

#[derive(Debug, Clone, PartialEq)]
pub struct FilterResult {
    pub content: String,
    pub original_lines: usize,
    pub filtered_lines: usize,
    pub saved_pct: f32,
}

impl RtkFilter {
    /// Intercepts and condenses command output based on command type, using active context window.
    pub fn filter(command: &str, output: &str, exit_code: Option<i32>) -> FilterResult {
        let context_window = crate::context::donut::get_active_context_limit();
        Self::filter_for_context(command, output, exit_code, context_window)
    }

    /// Intercepts and condenses command output using a specific context window limit.
    pub fn filter_for_context(
        command: &str,
        output: &str,
        exit_code: Option<i32>,
        context_window: usize,
    ) -> FilterResult {
        let clean = strip_ansi(output);
        let orig_lines = clean.lines().count();

        if orig_lines <= 10 {
            return FilterResult {
                content: clean.clone(),
                original_lines: orig_lines,
                filtered_lines: orig_lines,
                saved_pct: 0.0,
            };
        }

        let cmd_lower = command.to_lowercase();

        let filtered = if cmd_lower.contains("cargo test") || cmd_lower.contains("cargo nextest") {
            Self::filter_cargo_test(&clean, exit_code)
        } else if cmd_lower.contains("pytest") || cmd_lower.contains("python -m unittest") {
            Self::filter_pytest(&clean, exit_code)
        } else if cmd_lower.contains("git log") {
            Self::filter_git_log(&clean)
        } else if cmd_lower.contains("npm test")
            || cmd_lower.contains("yarn test")
            || cmd_lower.contains("jest")
        {
            Self::filter_jest(&clean, exit_code)
        } else if context_window > 0 {
            Self::filter_generic_for_context(&clean, context_window)
        } else {
            Self::filter_generic(&clean, crate::constants::DONUT_STANDARD_THRESHOLD_LINES)
        };

        let filt_lines = filtered.lines().count();
        let saved_pct = if orig_lines > 0 {
            ((orig_lines.saturating_sub(filt_lines)) as f32 / orig_lines as f32) * 100.0
        } else {
            0.0
        };

        FilterResult {
            content: filtered,
            original_lines: orig_lines,
            filtered_lines: filt_lines,
            saved_pct,
        }
    }

    /// Filters cargo test output, preserving failed assertions, stack traces, and test summary
    pub fn filter_cargo_test(output: &str, exit_code: Option<i32>) -> String {
        let is_success = exit_code == Some(0);
        if is_success {
            // When all tests pass, keep only running test lines and final summaries
            let mut summary_lines = Vec::new();
            for line in output.lines() {
                if line.contains("test result:")
                    || line.contains("Doc-tests")
                    || line.contains("running ")
                {
                    summary_lines.push(line);
                }
            }
            if summary_lines.is_empty() {
                output.lines().take(15).collect::<Vec<_>>().join("\n")
            } else {
                format!(
                    "✔ All tests passed successfully:\n{}",
                    summary_lines.join("\n")
                )
            }
        } else {
            // On failure, isolate failures section, panics, and summary
            let mut out = Vec::new();
            let mut in_failures = false;
            let mut summary = String::new();

            for line in output.lines() {
                if line.contains("failures:") {
                    in_failures = true;
                    out.push(line);
                    continue;
                }
                if line.starts_with("test result:") {
                    summary = line.to_string();
                    in_failures = false;
                }
                if in_failures
                    || line.contains("FAILED")
                    || line.contains("panicked at")
                    || line.contains("assertion `left == right` failed")
                {
                    out.push(line);
                }
            }

            if out.is_empty() {
                // Fallback: preserve last 30 lines
                output
                    .lines()
                    .rev()
                    .take(30)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect::<Vec<_>>()
                    .join("\n")
            } else {
                if !summary.is_empty() {
                    out.push("");
                    out.push(&summary);
                }
                out.join("\n")
            }
        }
    }

    /// Filters pytest output, isolating failures, errors, and summary line
    pub fn filter_pytest(output: &str, exit_code: Option<i32>) -> String {
        if exit_code == Some(0) {
            let last_lines: Vec<&str> = output.lines().filter(|l| !l.trim().is_empty()).collect();
            last_lines
                .iter()
                .rev()
                .take(3)
                .rev()
                .copied()
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            let mut failures = Vec::new();
            let mut capture = false;
            for line in output.lines() {
                if line.starts_with("FAILURES")
                    || line.starts_with("ERRORS")
                    || line.contains("FAILED ")
                {
                    capture = true;
                }
                if capture {
                    failures.push(line);
                }
                if line.starts_with("=====") && line.contains("failed") {
                    capture = false;
                }
            }
            if failures.is_empty() {
                output
                    .lines()
                    .rev()
                    .take(25)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect::<Vec<_>>()
                    .join("\n")
            } else {
                failures.join("\n")
            }
        }
    }

    /// Filters git log into clean one-line summaries
    pub fn filter_git_log(output: &str) -> String {
        let mut lines = Vec::new();
        for line in output.lines().take(30) {
            if !line.trim().is_empty() {
                lines.push(line);
            }
        }
        lines.join("\n")
    }

    /// Filters jest / npm test outputs
    pub fn filter_jest(output: &str, exit_code: Option<i32>) -> String {
        if exit_code == Some(0) {
            output
                .lines()
                .filter(|l| l.contains("Tests:") || l.contains("Snapshots:") || l.contains("Time:"))
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            output
                .lines()
                .filter(|l| {
                    l.contains("●")
                        || l.contains("FAIL")
                        || l.contains("Error:")
                        || l.contains("Tests:")
                })
                .collect::<Vec<_>>()
                .join("\n")
        }
    }

    /// Generic head + tail truncation via SmartDonutTruncator preserving critical error/diagnostic lines
    pub fn filter_generic(output: &str, max_lines: usize) -> String {
        let head = max_lines.saturating_mul(3) / 10;
        let tail = max_lines.saturating_mul(5) / 10;
        let max_errors = max_lines.saturating_mul(3) / 10;
        crate::context::budget::donut::SmartDonutTruncator::truncate_custom(
            output,
            max_lines,
            head.max(10),
            tail.max(15),
            max_errors.max(10),
        )
        .content
    }

    /// Generic truncation via SmartDonutTruncator dynamically scaling to model context window
    pub fn filter_generic_for_context(output: &str, context_window: usize) -> String {
        crate::context::budget::donut::SmartDonutTruncator::truncate_for_context(
            output,
            context_window,
        )
        .content
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rtk_filter_cargo_test_success() {
        let raw = r#"
running 10 tests
test tools::test_1 ... ok
test tools::test_2 ... ok
test tools::test_3 ... ok
test tools::test_4 ... ok
test tools::test_5 ... ok
test tools::test_6 ... ok
test tools::test_7 ... ok
test tools::test_8 ... ok
test tools::test_9 ... ok
test tools::test_10 ... ok

test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s
"#;

        let res = RtkFilter::filter("cargo test", raw, Some(0));
        assert!(res.content.contains("All tests passed"));
        assert!(res.content.contains("test result: ok. 10 passed"));
        assert!(res.saved_pct > 0.0);
    }

    #[test]
    fn test_rtk_filter_cargo_test_failure() {
        let raw = r#"
running 3 tests
test test_ok_1 ... ok
test test_fail_2 ... FAILED
test test_ok_3 ... ok

failures:

---- test_fail_2 stdout ----
thread 'test_fail_2' panicked at src/lib.rs:42:9:
assertion `left == right` failed
  left: 1
 right: 2

failures:
    test_fail_2

test result: FAILED. 2 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
"#;

        let res = RtkFilter::filter("cargo test", raw, Some(101));
        assert!(res.content.contains("failures:"));
        assert!(res.content.contains("test_fail_2"));
        assert!(res.content.contains("assertion `left == right` failed"));
    }
}

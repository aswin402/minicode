use crate::context::budget::ccr_cache::CcrCache;

/// Intelligent log and compiler output pruner inspired by Headroom-MCP.
/// Removes terminal ANSI escape sequences, collapses repetitive progress lines,
/// and folds external runtime stacktrace frames while preserving compiler diagnostics and failure summaries.
pub struct LogPruner;

impl LogPruner {
    /// Strips ANSI terminal escape sequences and normalizes carriage return updates.
    pub fn strip_ansi(input: &str) -> String {
        static ANSI_RE: std::sync::OnceLock<Option<regex::Regex>> = std::sync::OnceLock::new();
        let ansi_re = ANSI_RE.get_or_init(|| {
            regex::Regex::new(r"\x1b\[[0-9;]*[a-zA-Z]|\x1b\([a-zA-Z]|\x1b\][^\x07]*\x07").ok()
        });

        let cleaned = if let Some(re) = ansi_re {
            re.replace_all(input, "").to_string()
        } else {
            input.to_string()
        };

        // Normalize carriage returns (\r\n -> \n, and \r redraws -> keep last segment)
        let mut out = String::with_capacity(cleaned.len());
        for line in cleaned.lines() {
            if let Some(last_seg) = line.split('\r').rfind(|s| !s.is_empty()) {
                out.push_str(last_seg.trim_end());
                out.push('\n');
            } else {
                out.push('\n');
            }
        }
        out
    }

    /// Prunes large logs, compiler outputs, or stacktraces.
    /// If significant compression is achieved (>= 150 bytes and >= 20%, or structural warnings/tests collapsed),
    /// caches the lossless original into `CcrCache` and returns `(pruned_log, ccr_id)`.
    pub fn prune(raw_log: &str) -> Option<(String, String)> {
        let cleaned = Self::strip_ansi(raw_log);
        let lines: Vec<&str> = cleaned.lines().collect();

        let has_warning = cleaned.contains("warning:") || cleaned.contains(" - warning TS");
        let has_tests = cleaned.contains("... ok") || cleaned.contains("PASS");
        if lines.len() < 10 && cleaned.len() < 800 && !has_warning && !has_tests {
            return None;
        }

        // 1. Condense compiler warning cascades
        let (lines_after_warnings, warnings_collapsed) = Self::condense_compiler_warnings(&lines);
        let str_lines1: Vec<&str> = lines_after_warnings.iter().map(|s| s.as_str()).collect();

        // 2. Condense test runner passing floods
        let (lines_after_tests, tests_collapsed) = Self::condense_test_runner_output(&str_lines1);
        let str_lines2: Vec<&str> = lines_after_tests.iter().map(|s| s.as_str()).collect();

        // 3. Collapse repetitive build & progress lines
        let collapsed_lines = Self::collapse_repetitive_lines(&str_lines2);

        // 4. Fold external runtime stacktrace frames
        let folded_lines = Self::fold_runtime_stack_frames(&collapsed_lines);

        let mut pruned = folded_lines.join("\n");
        pruned = pruned.trim().to_string();

        let original_len = raw_log.len();
        let pruned_len = pruned.len();

        let has_structural_reduction = warnings_collapsed > 0 || tests_collapsed > 0;
        let meets_threshold = (original_len > pruned_len + 150
            && (original_len - pruned_len) * 100 / original_len >= 20)
            || (has_structural_reduction && original_len > pruned_len + 100);

        if meets_threshold {
            let ccr_id = CcrCache::store(raw_log);
            let mut summary_badge = format!(
                "\n\n[Log pruned: {} bytes -> {} bytes",
                original_len, pruned_len
            );
            if warnings_collapsed > 0 {
                summary_badge.push_str(&format!(" (collapsed {} warnings)", warnings_collapsed));
            }
            if tests_collapsed > 0 {
                summary_badge.push_str(&format!(" (collapsed {} passing tests)", tests_collapsed));
            }
            summary_badge.push_str(&format!(
                ". Use retrieve_observation(id=\"{}\") for full raw output.]",
                ccr_id
            ));
            pruned.push_str(&summary_badge);
            Some((pruned, ccr_id))
        } else {
            None
        }
    }

    /// Condenses compiler warning cascades (e.g. rustc, tsc, gcc) into a single badge,
    /// while preserving all error diagnostics, syntax errors, and build status intact.
    pub fn condense_compiler_warnings(lines: &[&str]) -> (Vec<String>, usize) {
        #[allow(dead_code)]
        enum Diag {
            Warning(Vec<String>),
            Error(Vec<String>),
            Other(String),
        }

        let mut diags: Vec<Diag> = Vec::new();
        let mut i = 0;
        let mut warning_count = 0;

        while i < lines.len() {
            let line = lines[i];
            if Self::is_warning_header(line) {
                warning_count += 1;
                let mut block = vec![line.to_string()];
                i += 1;
                while i < lines.len() {
                    let next = lines[i];
                    if Self::is_warning_header(next)
                        || Self::is_error_header(next)
                        || Self::is_summary_line(next)
                    {
                        break;
                    }
                    if Self::is_diagnostic_continuation(next)
                        || (next.trim().is_empty()
                            && i + 1 < lines.len()
                            && Self::is_diagnostic_continuation(lines[i + 1]))
                    {
                        block.push(next.to_string());
                        i += 1;
                    } else {
                        break;
                    }
                }
                diags.push(Diag::Warning(block));
            } else if Self::is_error_header(line) {
                let mut block = vec![line.to_string()];
                i += 1;
                while i < lines.len() {
                    let next = lines[i];
                    if Self::is_warning_header(next)
                        || Self::is_error_header(next)
                        || Self::is_summary_line(next)
                    {
                        break;
                    }
                    if Self::is_diagnostic_continuation(next)
                        || (next.trim().is_empty()
                            && i + 1 < lines.len()
                            && Self::is_diagnostic_continuation(lines[i + 1]))
                    {
                        block.push(next.to_string());
                        i += 1;
                    } else {
                        break;
                    }
                }
                diags.push(Diag::Error(block));
            } else {
                diags.push(Diag::Other(line.to_string()));
                i += 1;
            }
        }

        if warning_count >= 3 {
            let mut out = Vec::new();
            let mut inserted_badge = false;
            for diag in diags {
                match diag {
                    Diag::Warning(_) => {
                        if !inserted_badge {
                            out.push(format!(
                                "⚠️  [{} compiler warnings collapsed — original cached in CCR]",
                                warning_count
                            ));
                            inserted_badge = true;
                        }
                    }
                    Diag::Error(err_lines) => {
                        out.extend(err_lines);
                    }
                    Diag::Other(l) => {
                        let trimmed = l.trim();
                        if trimmed.starts_with("warning: `") && trimmed.contains("warnings") {
                            continue;
                        }
                        out.push(l);
                    }
                }
            }
            (out, warning_count)
        } else {
            (lines.iter().map(|s| s.to_string()).collect(), 0)
        }
    }

    fn is_warning_header(line: &str) -> bool {
        let trimmed = line.trim();
        trimmed.starts_with("warning:")
            || trimmed.starts_with("warning[")
            || trimmed.starts_with("warn:")
            || trimmed.contains(" - warning TS")
            || (trimmed.contains(": warning:") && !trimmed.contains("error"))
    }

    fn is_error_header(line: &str) -> bool {
        let trimmed = line.trim();
        trimmed.starts_with("error:")
            || trimmed.starts_with("error[")
            || trimmed.starts_with("error TS")
            || trimmed.contains(" - error TS")
            || trimmed.contains(": error:")
            || trimmed.contains(": fatal error:")
            || trimmed.starts_with("SyntaxError:")
            || trimmed.starts_with("TypeError:")
            || trimmed.starts_with("ReferenceError:")
            || trimmed.starts_with("panicked at")
            || trimmed.starts_with("panic:")
    }

    fn is_summary_line(line: &str) -> bool {
        let trimmed = line.trim();
        trimmed.starts_with("Finished ")
            || trimmed.starts_with("error: could not compile")
            || trimmed.starts_with("test result:")
            || trimmed.starts_with("failures:")
    }

    fn is_diagnostic_continuation(line: &str) -> bool {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return false;
        }
        line.starts_with("  --> ")
            || line.starts_with(" --> ")
            || line.starts_with("   |")
            || line.starts_with("  |")
            || line.starts_with(" |")
            || line.starts_with("  = ")
            || line.starts_with("   = ")
            || line.starts_with("    ")
            || trimmed.starts_with('^')
            || trimmed.starts_with('|')
            || trimmed.starts_with("help:")
            || trimmed.starts_with("note:")
    }

    /// Condenses high-volume test runner passing outputs (e.g. `cargo test`, `pytest`, `jest`, `vitest`)
    /// into concise badges while preserving all failed tests, assertions, stack traces, and test summaries.
    pub fn condense_test_runner_output(lines: &[&str]) -> (Vec<String>, usize) {
        let mut total_passing = 0;
        for line in lines {
            if Self::is_passing_test_line(line) {
                total_passing += 1;
            }
        }

        if total_passing < 4 {
            return (lines.iter().map(|s| s.to_string()).collect(), 0);
        }

        let mut result = Vec::new();
        let mut i = 0;
        let mut collapsed_count = 0;

        while i < lines.len() {
            let line = lines[i];
            if Self::is_passing_test_line(line) {
                let start = i;
                while i < lines.len() && Self::is_passing_test_line(lines[i]) {
                    i += 1;
                }
                let span = i - start;
                if span >= 3 || total_passing >= 5 {
                    result.push(format!("✅  [{} passing tests collapsed]", span));
                    collapsed_count += span;
                } else {
                    result.extend(lines[start..i].iter().map(|l| l.to_string()));
                }
            } else {
                result.push(line.to_string());
                i += 1;
            }
        }

        (result, collapsed_count)
    }

    fn is_passing_test_line(line: &str) -> bool {
        let trimmed = line.trim();
        // cargo test
        (trimmed.starts_with("test ") && (trimmed.ends_with("... ok") || trimmed.ends_with("... OK")))
        // pytest
        || (trimmed.ends_with("PASSED") || (trimmed.contains(" PASSED [") && trimmed.ends_with('%')) || (trimmed.contains("::") && trimmed.contains(" PASSED")))
        // jest / vitest / bun
        || (trimmed.starts_with("✓ ") || trimmed.starts_with("✔ ") || trimmed.starts_with("PASS ") || (trimmed.contains(" PASS ") && !trimmed.contains("FAIL")))
        // go test
        || trimmed.starts_with("--- PASS:")
    }

    /// Collapses consecutive repetitive lines (e.g. `Compiling ...`, `Downloaded ...`, or progress bars).
    pub fn collapse_repetitive_lines(lines: &[&str]) -> Vec<String> {
        let mut result = Vec::new();
        let mut i = 0;

        while i < lines.len() {
            let line = lines[i];
            let kind = Self::classify_repetitive_line(line);

            if let Some(k) = kind {
                // Count how many consecutive lines match this repetition kind
                let mut j = i + 1;
                while j < lines.len() && Self::classify_repetitive_line(lines[j]) == Some(k) {
                    j += 1;
                }

                let count = j - i;
                if count >= 4 {
                    // Retain first 2 lines
                    result.push(lines[i].to_string());
                    result.push(lines[i + 1].to_string());
                    // Collapse middle
                    let omitted = count - 3;
                    result.push(format!("  [... {} {} lines collapsed ...]", omitted, k));
                    // Retain last line
                    result.push(lines[j - 1].to_string());
                    i = j;
                    continue;
                }
            }

            result.push(line.to_string());
            i += 1;
        }

        result
    }

    fn classify_repetitive_line(line: &str) -> Option<&'static str> {
        let trimmed = line.trim();
        if trimmed.starts_with("Compiling ") {
            Some("compilation")
        } else if trimmed.starts_with("Downloaded ") || trimmed.starts_with("Downloading ") {
            Some("download")
        } else if trimmed.starts_with("Fetching ") || trimmed.starts_with("Updating ") {
            Some("package update")
        } else if (trimmed.contains("[==") || trimmed.contains("[--")) && trimmed.contains('%') {
            Some("progress bar")
        } else {
            None
        }
    }

    /// Folds long chains of internal runtime/system stack frames while preserving user-code frames.
    pub fn fold_runtime_stack_frames(lines: &[String]) -> Vec<String> {
        let mut result = Vec::new();
        let mut i = 0;

        while i < lines.len() {
            let line = &lines[i];

            if Self::is_runtime_frame(line) {
                let mut j = i + 1;
                while j < lines.len() && Self::is_runtime_frame(&lines[j]) {
                    j += 1;
                }

                let count = j - i;
                if count >= 4 {
                    result.push(lines[i].clone());
                    let omitted = count - 2;
                    result.push(format!(
                        "  [... {} runtime/system stack frames folded ...]",
                        omitted
                    ));
                    result.push(lines[j - 1].clone());
                    i = j;
                    continue;
                }
            }

            result.push(line.clone());
            i += 1;
        }

        result
    }

    fn is_runtime_frame(line: &str) -> bool {
        let trimmed = line.trim();

        // Must look like a backtrace frame
        let is_frame = (trimmed.starts_with("at ")
            || trimmed.starts_with("at:")
            || trimmed.starts_with("File "))
            || (trimmed
                .chars()
                .next()
                .map(|c| c.is_ascii_digit())
                .unwrap_or(false)
                && trimmed.contains(": 0x"));

        if !is_frame {
            return false;
        }

        let lower = trimmed.to_lowercase();

        // If it refers to user project files, it is NOT an external runtime frame
        let is_user_project = lower.contains("src/")
            || lower.contains("crates/")
            || lower.contains("tests/")
            || lower.contains("examples/")
            || lower.contains("app/");

        if is_user_project {
            return false;
        }

        // Common external runtime and system namespaces
        lower.contains("tokio::")
            || lower.contains("core::panicking")
            || lower.contains("core::ptr")
            || lower.contains("std::panicking")
            || lower.contains("std::sys::")
            || lower.contains("std::rt::")
            || lower.contains("node_modules/")
            || lower.contains("site-packages/")
            || lower.contains("<unknown>")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_ansi() {
        let ansi_text = "\x1b[32mSuccess\x1b[0m in \x1b[1;34m12ms\x1b[0m\n";
        let cleaned = LogPruner::strip_ansi(ansi_text);
        assert_eq!(cleaned.trim(), "Success in 12ms");
    }

    #[test]
    fn test_collapse_repetitive_lines() {
        let lines = vec![
            "Compiling serde v1.0.1",
            "Compiling tokio v1.28.0",
            "Compiling reqwest v0.11.0",
            "Compiling hyper v0.14.0",
            "Compiling minicode v0.3.19",
        ];

        let collapsed = LogPruner::collapse_repetitive_lines(&lines);
        assert_eq!(collapsed.len(), 4);
        assert_eq!(collapsed[0], "Compiling serde v1.0.1");
        assert_eq!(collapsed[1], "Compiling tokio v1.28.0");
        assert!(collapsed[2].contains("2 compilation lines collapsed"));
        assert_eq!(collapsed[3], "Compiling minicode v0.3.19");
    }

    #[test]
    fn test_fold_runtime_stack_frames() {
        let lines = vec![
            "  at src/main.rs:42:10".to_string(),
            "  at tokio::runtime::task::core:100".to_string(),
            "  at tokio::runtime::task::harness:200".to_string(),
            "  at std::sys::pal::unix::thread:300".to_string(),
            "  at std::rt::lang_start:400".to_string(),
            "  at src/lib.rs:88:5".to_string(),
        ];

        let folded = LogPruner::fold_runtime_stack_frames(&lines);
        assert_eq!(folded.len(), 5);
        assert_eq!(folded[0], "  at src/main.rs:42:10");
        assert!(folded[2].contains("2 runtime/system stack frames folded"));
        assert_eq!(folded[4], "  at src/lib.rs:88:5");
    }

    #[test]
    fn test_log_prune_compression_and_ccr() {
        let mut log = String::new();
        log.push_str("\x1b[31merror[E0425]: cannot find value `foo` in this scope\x1b[0m\n");
        log.push_str("  --> src/agent/runner.rs:42:15\n");
        log.push_str("   |\n42 |     let x = foo;\n   |             ^^^\n");
        for i in 0..30 {
            log.push_str(&format!("Compiling crate_{} v0.1.0\n", i));
        }
        for i in 0..15 {
            log.push_str(&format!("  at tokio::runtime::thread_{}:{}\n", i, i * 10));
        }
        log.push_str("test result: FAILED. 1 failed; 0 passed\n");

        let result = LogPruner::prune(&log);
        assert!(result.is_some());
        let (pruned, ccr_id) = result.unwrap();

        assert!(pruned.contains("error[E0425]"));
        assert!(pruned.contains("src/agent/runner.rs:42:15"));
        assert!(pruned.contains("test result: FAILED"));
        assert!(pruned.contains("Use retrieve_observation"));
        assert!(!ccr_id.is_empty());

        // Lossless retrieval verification
        let retrieved = CcrCache::retrieve(&ccr_id, None, None).unwrap();
        assert_eq!(retrieved, log);
    }

    #[test]
    fn test_condense_compiler_warnings() {
        let mut lines = Vec::new();
        for i in 0..30 {
            lines.push(format!("warning: unused variable: `var_{}`", i));
            lines.push(format!("  --> src/file_{}.rs:10:9", i));
            lines.push("   |".to_string());
            lines.push(format!("10 |     let var_{} = 42;", i));
            lines.push("   |         ^^^^^^ help: prefix with underscore".to_string());
            lines.push("   =".to_string());
            lines.push("   = note: `#[warn(unused_variables)]` on by default".to_string());
            lines.push("".to_string());
        }
        lines.push("error[E0382]: use of moved value: `target`".to_string());
        lines.push("  --> src/main.rs:99:15".to_string());
        lines.push("   |".to_string());
        lines.push("99 |     let y = target;".to_string());
        lines.push("   |             ^^^^^^ value moved here".to_string());
        lines.push("error: could not compile `minicode` due to 1 previous error".to_string());

        let line_refs: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
        let (condensed, collapsed_count) = LogPruner::condense_compiler_warnings(&line_refs);

        assert_eq!(collapsed_count, 30);
        let joined = condensed.join("\n");
        assert!(joined.contains("30 compiler warnings collapsed"));
        assert!(joined.contains("error[E0382]: use of moved value: `target`"));
        assert!(joined.contains("src/main.rs:99:15"));
        assert!(joined.contains("99 |     let y = target;"));
        assert!(!joined.contains("unused variable: `var_0`"));
        assert!(!joined.contains("unused variable: `var_29`"));
    }

    #[test]
    fn test_condense_test_runner_output() {
        let mut lines = Vec::new();
        lines.push("running 149 tests".to_string());
        for i in 0..148 {
            lines.push(format!("test context::module::test_{} ... ok", i));
        }
        lines.push("test context::module::test_failure ... FAILED".to_string());
        lines.push("".to_string());
        lines.push("failures:".to_string());
        lines.push("---- context::module::test_failure stdout ----".to_string());
        lines.push("thread 'test_failure' panicked at src/lib.rs:50:5:".to_string());
        lines.push("assertion `left == right` failed: expected true, got false".to_string());
        lines.push("failures:".to_string());
        lines.push("    context::module::test_failure".to_string());
        lines.push(
            "test result: FAILED. 148 passed; 1 failed; 0 ignored; 0 measured; finished in 0.15s"
                .to_string(),
        );

        let line_refs: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
        let (condensed, collapsed_count) = LogPruner::condense_test_runner_output(&line_refs);

        assert_eq!(collapsed_count, 148);
        let joined = condensed.join("\n");
        assert!(joined.contains("148 passing tests collapsed"));
        assert!(joined.contains("test context::module::test_failure ... FAILED"));
        assert!(joined.contains("assertion `left == right` failed"));
        assert!(joined.contains("test result: FAILED. 148 passed; 1 failed"));
        assert!(!joined.contains("test context::module::test_0 ... ok"));
    }
}

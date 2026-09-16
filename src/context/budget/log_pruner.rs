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
    /// If significant compression is achieved (>= 200 bytes and >= 25%), caches
    /// the lossless original into `CcrCache` and returns `(pruned_log, ccr_id)`.
    pub fn prune(raw_log: &str) -> Option<(String, String)> {
        let cleaned = Self::strip_ansi(raw_log);
        let lines: Vec<&str> = cleaned.lines().collect();

        if lines.len() < 15 && cleaned.len() < 1200 {
            return None;
        }

        // 1. Collapse repetitive build & progress lines
        let collapsed_lines = Self::collapse_repetitive_lines(&lines);

        // 2. Fold external runtime stacktrace frames
        let folded_lines = Self::fold_runtime_stack_frames(&collapsed_lines);

        let mut pruned = folded_lines.join("\n");
        pruned = pruned.trim().to_string();

        let original_len = raw_log.len();
        let pruned_len = pruned.len();

        // Check if compression saved significant context (> 200 bytes and > 25% savings)
        if original_len > pruned_len + 200 && (original_len - pruned_len) * 100 / original_len >= 25
        {
            let ccr_id = CcrCache::store(raw_log);
            pruned.push_str(&format!(
                "\n\n[Log pruned: {} bytes -> {} bytes. Use retrieve_observation(id=\"{}\") for full raw output.]",
                original_len, pruned_len, ccr_id
            ));
            Some((pruned, ccr_id))
        } else {
            None
        }
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
}

use crate::constants::{
    DONUT_ERROR_CUES, DONUT_HEAD_LINES, DONUT_MAX_ERROR_LINES, DONUT_MAX_LINE_CHARS,
    DONUT_TAIL_LINES, DONUT_THRESHOLD_LINES,
};

/// Result metadata from Smart Donut Truncation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DonutTruncationResult {
    /// Resulting content string (either untouched or donut-truncated)
    pub content: String,
    /// Total lines in original input
    pub original_lines: usize,
    /// Number of lines omitted in the middle donut hole
    pub omitted_lines: usize,
    /// Number of critical error/diagnostic lines extracted from the omitted middle section
    pub extracted_error_count: usize,
    /// Whether any truncation was performed
    pub was_truncated: bool,
}

/// Smart Donut Truncator for massive tool outputs.
///
/// Unlike naive head-only or tail-only truncation that silently discards critical compiler
/// or runtime errors buried in the middle of build logs, Smart Donut truncation:
/// 1. Preserves the initial execution context (`head` lines)
/// 2. Deep-scans the middle "donut hole" for errors/panics/exceptions with exact line numbers
/// 3. Preserves the terminal exit summary and recent output (`tail` lines)
pub struct SmartDonutTruncator;

impl SmartDonutTruncator {
    /// Truncates a tool output string using standard default constants.
    #[must_use]
    pub fn truncate(input: &str) -> String {
        Self::truncate_with_result(input).content
    }

    /// Truncates a tool output string using standard default constants, returning structured metadata.
    #[must_use]
    pub fn truncate_with_result(input: &str) -> DonutTruncationResult {
        Self::truncate_custom(
            input,
            DONUT_THRESHOLD_LINES,
            DONUT_HEAD_LINES,
            DONUT_TAIL_LINES,
            DONUT_MAX_ERROR_LINES,
        )
    }

    /// Custom configurable Smart Donut truncation.
    #[must_use]
    pub fn truncate_custom(
        input: &str,
        threshold_lines: usize,
        head_count: usize,
        tail_count: usize,
        max_error_lines: usize,
    ) -> DonutTruncationResult {
        // Collect raw lines and sanitize individual excessively wide lines
        let raw_lines: Vec<&str> = input.lines().collect();
        let total_lines = raw_lines.len();

        if total_lines <= threshold_lines || head_count + tail_count >= total_lines {
            // Check if any single line requires wide-character truncation
            let needs_line_clamping = raw_lines.iter().any(|l| l.len() > DONUT_MAX_LINE_CHARS);
            if !needs_line_clamping {
                return DonutTruncationResult {
                    content: input.to_string(),
                    original_lines: total_lines,
                    omitted_lines: 0,
                    extracted_error_count: 0,
                    was_truncated: false,
                };
            }

            let clamped_content = raw_lines
                .into_iter()
                .map(Self::clamp_line_width)
                .collect::<Vec<String>>()
                .join("\n");

            return DonutTruncationResult {
                content: clamped_content,
                original_lines: total_lines,
                omitted_lines: 0,
                extracted_error_count: 0,
                was_truncated: true,
            };
        }

        let head_slice = &raw_lines[..head_count];
        let tail_slice = &raw_lines[total_lines - tail_count..];
        let middle_slice = &raw_lines[head_count..total_lines - tail_count];
        let omitted_count = middle_slice.len();

        let omitted_start_line = head_count + 1;
        let omitted_end_line = total_lines - tail_count;

        // Scan middle slice for error/diagnostic cues with original 1-indexed line numbers
        let mut extracted_diagnostics: Vec<String> = Vec::new();
        let mut i = 0;
        while i < middle_slice.len() && extracted_diagnostics.len() < max_error_lines {
            let line = middle_slice[i];
            let original_line_num = omitted_start_line + i;

            if Self::is_diagnostic_line(line) {
                let clamped = Self::clamp_line_width(line);
                extracted_diagnostics.push(format!("  Line {}: {}", original_line_num, clamped));

                // Check if subsequent line is a code pointer (e.g. `--> src/main.rs:12:4`)
                if i + 1 < middle_slice.len()
                    && extracted_diagnostics.len() < max_error_lines
                    && Self::is_location_pointer(middle_slice[i + 1])
                {
                    i += 1;
                    let next_line = middle_slice[i];
                    let next_line_num = omitted_start_line + i;
                    let next_clamped = Self::clamp_line_width(next_line);
                    extracted_diagnostics
                        .push(format!("  Line {}: {}", next_line_num, next_clamped));
                }
            }
            i += 1;
        }

        let extracted_count = extracted_diagnostics.len();

        let head_rendered: Vec<String> = head_slice
            .iter()
            .map(|&l| Self::clamp_line_width(l))
            .collect();
        let tail_rendered: Vec<String> = tail_slice
            .iter()
            .map(|&l| Self::clamp_line_width(l))
            .collect();

        let mut out = String::with_capacity(input.len().min(64 * 1024));
        out.push_str(&head_rendered.join("\n"));
        out.push_str("\n\n");

        if extracted_count > 0 {
            out.push_str(&format!(
                "[... Smart Donut Truncation: Omitted {} lines (lines {} to {}) ...]\n\
                 [Extracted {} diagnostic/error lines from omitted middle section:]\n{}\n\
                 [End of extracted diagnostics]\n",
                omitted_count,
                omitted_start_line,
                omitted_end_line,
                extracted_count,
                extracted_diagnostics.join("\n")
            ));
        } else {
            out.push_str(&format!(
                "[... Smart Donut Truncation: Omitted {} lines (lines {} to {}) — zero errors detected in omitted section ...]\n",
                omitted_count, omitted_start_line, omitted_end_line
            ));
        }

        out.push('\n');
        out.push_str(&tail_rendered.join("\n"));

        DonutTruncationResult {
            content: out,
            original_lines: total_lines,
            omitted_lines: omitted_count,
            extracted_error_count: extracted_count,
            was_truncated: true,
        }
    }

    /// Checks whether a line contains diagnostic or error markers.
    #[must_use]
    pub fn is_diagnostic_line(line: &str) -> bool {
        let lower = line.to_lowercase();
        DONUT_ERROR_CUES.iter().any(|&cue| lower.contains(cue))
    }

    /// Checks whether a line is a code location pointer or traceback source reference.
    #[must_use]
    pub fn is_location_pointer(line: &str) -> bool {
        let trimmed = line.trim_start();
        trimmed.starts_with("-->")
            || trimmed.starts_with("File \"")
            || trimmed.starts_with("at ")
            || trimmed.starts_with("stack backtrace:")
    }

    /// Clamps a single line to `DONUT_MAX_LINE_CHARS` safely on unicode character boundaries.
    #[must_use]
    pub fn clamp_line_width(line: &str) -> String {
        if line.len() <= DONUT_MAX_LINE_CHARS {
            line.to_string()
        } else {
            let clamped: String = line.chars().take(DONUT_MAX_LINE_CHARS).collect();
            format!(
                "{} [... line clamped at {} chars ...]",
                clamped, DONUT_MAX_LINE_CHARS
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_donut_no_truncation_under_threshold() {
        let mut lines = Vec::new();
        for i in 1..=50 {
            lines.push(format!("Line {}", i));
        }
        let input = lines.join("\n");
        let res = SmartDonutTruncator::truncate_with_result(&input);
        assert!(!res.was_truncated);
        assert_eq!(res.omitted_lines, 0);
        assert_eq!(res.content, input);
    }

    #[test]
    fn test_donut_truncation_without_errors() {
        let mut lines = Vec::new();
        for i in 1..=400 {
            lines.push(format!("Normal output info line {}", i));
        }
        let input = lines.join("\n");
        let res = SmartDonutTruncator::truncate_custom(&input, 300, 100, 200, 60);

        assert!(res.was_truncated);
        assert_eq!(res.original_lines, 400);
        assert_eq!(res.omitted_lines, 100);
        assert_eq!(res.extracted_error_count, 0);

        assert!(res.content.starts_with("Normal output info line 1"));
        assert!(res.content.contains("Normal output info line 100"));
        assert!(res.content.contains("[... Smart Donut Truncation: Omitted 100 lines (lines 101 to 200) — zero errors detected in omitted section ...]"));
        assert!(res.content.contains("Normal output info line 201"));
        assert!(res.content.ends_with("Normal output info line 400"));
    }

    #[test]
    fn test_donut_truncation_extracts_errors_with_location_pointers() {
        let mut lines = Vec::new();
        for i in 1..=500 {
            if i == 150 {
                lines.push("error[E0425]: cannot find value `foo` in this scope".to_string());
                lines.push("  --> src/main.rs:150:5".to_string());
            } else if i == 250 {
                lines.push("fatal: repository not found".to_string());
            } else {
                lines.push(format!("Build step log item {}", i));
            }
        }
        let input = lines.join("\n");
        let res = SmartDonutTruncator::truncate_custom(&input, 300, 100, 200, 60);

        assert!(res.was_truncated);
        assert!(res.extracted_error_count >= 2);
        assert!(res
            .content
            .contains("Line 150: error[E0425]: cannot find value `foo`"));
        assert!(res.content.contains("Line 151:   --> src/main.rs:150:5"));
        assert!(res.content.contains("fatal: repository not found"));
    }

    #[test]
    fn test_clamp_line_width_safe_unicode() {
        let wide_line = "🦀".repeat(3000);
        let clamped = SmartDonutTruncator::clamp_line_width(&wide_line);
        assert!(clamped.contains("[... line clamped at 2000 chars ...]"));
        assert!(clamped.chars().count() > 2000);
    }
}

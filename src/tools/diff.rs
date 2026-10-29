//! Inline diff engine for file-modifying tool results.
//!
//! Produces compact unified-style diffs using the `similar` crate and
//! renders them as coloured ratatui `Line` spans.

use similar::{ChangeTag, TextDiff};

/// Maximum number of diff lines rendered in the TUI timeline.
/// Lines beyond this are folded with a summary.
const MAX_DIFF_LINES: usize = 60;

/// A single parsed diff line ready for rendering.
#[derive(Debug, Clone)]
pub struct DiffLine {
    /// Prefix character: `+`, `-`, or ` ` (context).
    pub tag: char,
    /// The line content (without the prefix).
    pub content: String,
}

/// Computes a unified diff between `old` and `new` file content.
///
/// Uses hunk grouping (`grouped_ops(3)`) so edits anywhere in a file (even
/// well past line 60) are captured with up to 3 lines of surrounding context.
/// Returns an empty `Vec` if there are no changes.
/// Lines are capped at [`MAX_DIFF_LINES`].
pub fn compute_diff(old: &str, new: &str) -> Vec<DiffLine> {
    let diff = TextDiff::from_lines(old, new);
    let mut out = Vec::new();

    'groups: for (group_idx, group) in diff.grouped_ops(3).iter().enumerate() {
        if group_idx > 0 && !out.is_empty() {
            if out.len() > MAX_DIFF_LINES {
                break;
            }
            out.push(DiffLine {
                tag: ' ',
                content: "...".to_string(),
            });
        }
        for op in group {
            for change in diff.iter_changes(op) {
                let (tag, content) = match change.tag() {
                    ChangeTag::Delete => ('-', change.to_string()),
                    ChangeTag::Insert => ('+', change.to_string()),
                    ChangeTag::Equal => (' ', change.to_string()),
                };
                // Strip trailing newline from content
                let content = content.trim_end_matches('\n').to_string();
                out.push(DiffLine { tag, content });

                if out.len() > MAX_DIFF_LINES {
                    break 'groups;
                }
            }
        }
    }

    // Trim trailing context lines
    while out.last().map(|l| l.tag == ' ').unwrap_or(false) {
        out.pop();
    }

    out
}

/// Returns `true` if `diff_lines` contains at least one addition or deletion.
pub fn has_changes(diff_lines: &[DiffLine]) -> bool {
    diff_lines.iter().any(|l| l.tag == '+' || l.tag == '-')
}

/// Formats a diff as a plain string for export / session save.
pub fn format_diff_plain(diff_lines: &[DiffLine], file_path: &str) -> String {
    let mut out = format!("--- {}\n+++ {}\n", file_path, file_path);
    for line in diff_lines {
        out.push(line.tag);
        out.push(' ');
        out.push_str(&line.content);
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_diff_detects_additions() {
        let old = "fn main() {}\n";
        let new = "fn main() {}\nfn helper() {}\n";
        let diff = compute_diff(old, new);
        assert!(has_changes(&diff));
        assert!(diff
            .iter()
            .any(|l| l.tag == '+' && l.content.contains("helper")));
    }

    #[test]
    fn test_compute_diff_detects_deletions() {
        let old = "line one\nline two\nline three\n";
        let new = "line one\nline three\n";
        let diff = compute_diff(old, new);
        assert!(has_changes(&diff));
        assert!(diff
            .iter()
            .any(|l| l.tag == '-' && l.content.contains("two")));
    }

    #[test]
    fn test_compute_diff_identical_is_empty_of_changes() {
        let content = "unchanged\n";
        let diff = compute_diff(content, content);
        assert!(!has_changes(&diff));
    }

    #[test]
    fn test_format_diff_plain_includes_header() {
        let diff = vec![
            DiffLine {
                tag: '-',
                content: "old line".into(),
            },
            DiffLine {
                tag: '+',
                content: "new line".into(),
            },
        ];
        let formatted = format_diff_plain(&diff, "src/lib.rs");
        assert!(formatted.contains("--- src/lib.rs"));
        assert!(formatted.contains("+++ src/lib.rs"));
        assert!(formatted.contains("- old line"));
        assert!(formatted.contains("+ new line"));
    }

    #[test]
    fn test_large_diff_capped() {
        let old = (0..100)
            .map(|i| format!("old line {}\n", i))
            .collect::<String>();
        let new = (0..100)
            .map(|i| format!("new line {}\n", i))
            .collect::<String>();
        let diff = compute_diff(&old, &new);
        assert!(diff.len() <= MAX_DIFF_LINES + 1);
    }

    #[test]
    fn test_compute_diff_detects_changes_past_max_diff_lines() {
        let mut old_lines: Vec<String> = (1..=120).map(|i| format!("line {}\n", i)).collect();
        let mut new_lines = old_lines.clone();
        new_lines[99] = "updated line 100\n".to_string();
        let old = old_lines.drain(..).collect::<String>();
        let new = new_lines.drain(..).collect::<String>();

        let diff = compute_diff(&old, &new);
        assert!(has_changes(&diff));
        assert!(diff.iter().any(|l| l.tag == '-' && l.content == "line 100"));
        assert!(diff
            .iter()
            .any(|l| l.tag == '+' && l.content == "updated line 100"));
    }
}

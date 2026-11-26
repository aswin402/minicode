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

// ============================================================================
// Algorithmic Unified Diff Parser & 3-Tier Sliding-Window Applier (`O(diff)`)
// ============================================================================

/// A single line inside a parsed unified diff hunk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HunkLine {
    Context(String),
    Remove(String),
    Add(String),
}

/// A parsed unified diff hunk (`@@ -old_start,old_count +new_start,new_count @@`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnifiedHunk {
    /// 1-indexed original start line (0 when omitted, e.g. bare `@@ @@` context hunk).
    pub old_start: usize,
    pub old_count: usize,
    pub new_start: usize,
    pub new_count: usize,
    pub lines: Vec<HunkLine>,
}

/// Pure state-machine parser for standard unified diffs (zero regexes).
/// Accepts optional `diff --git` / `index` / `---` / `+++` headers and one or more `@@ ... @@` hunks.
pub fn parse_unified_diff(diff_text: &str) -> Result<Vec<UnifiedHunk>, String> {
    let normalized = diff_text.replace("\r\n", "\n");
    let mut hunks: Vec<UnifiedHunk> = Vec::new();
    let mut current: Option<UnifiedHunk> = None;

    for raw_line in normalized.lines() {
        // Skip optional git/diff preamble lines when outside a hunk (or file headers before first hunk)
        if current.is_none()
            && (raw_line.starts_with("diff --git ")
                || raw_line.starts_with("index ")
                || raw_line.starts_with("--- ")
                || raw_line.starts_with("+++ ")
                || raw_line.starts_with("```")
                || raw_line.trim().is_empty())
        {
            continue;
        }

        if raw_line.starts_with("@@") {
            if let Some(prev) = current.take() {
                if !prev.lines.is_empty() {
                    hunks.push(prev);
                }
            }
            let (old_start, old_count, new_start, new_count) = parse_hunk_header(raw_line)?;
            current = Some(UnifiedHunk {
                old_start,
                old_count,
                new_start,
                new_count,
                lines: Vec::new(),
            });
            continue;
        }

        if raw_line.starts_with("\\ No newline at end of file") || raw_line.starts_with("```") {
            continue;
        }

        let Some(ref mut hunk) = current else {
            return Err(format!(
                "Unexpected line outside unified diff hunk header (`@@ ... @@`): `{raw_line}`"
            ));
        };

        if let Some(rest) = raw_line.strip_prefix(' ') {
            hunk.lines.push(HunkLine::Context(rest.to_string()));
        } else if let Some(rest) = raw_line.strip_prefix('-') {
            hunk.lines.push(HunkLine::Remove(rest.to_string()));
        } else if let Some(rest) = raw_line.strip_prefix('+') {
            hunk.lines.push(HunkLine::Add(rest.to_string()));
        } else if raw_line.is_empty() {
            // Blank context line whose leading space was stripped by formatter/LLM
            hunk.lines.push(HunkLine::Context(String::new()));
        } else {
            return Err(format!(
                "Malformed unified diff line (expected leading ' ', '-', or '+'): `{raw_line}`"
            ));
        }
    }

    if let Some(last) = current {
        if !last.lines.is_empty() {
            hunks.push(last);
        }
    }

    if hunks.is_empty() {
        return Err("Unified diff contains no `@@ ... @@` hunks".to_string());
    }

    // Verify at least one mutation (`+` or `-`) exists across hunks
    let has_mutation = hunks.iter().any(|h| {
        h.lines
            .iter()
            .any(|l| matches!(l, HunkLine::Add(_) | HunkLine::Remove(_)))
    });
    if !has_mutation {
        return Err("Unified diff has no additions (`+`) or deletions (`-`)".to_string());
    }

    Ok(hunks)
}

fn parse_hunk_header(line: &str) -> Result<(usize, usize, usize, usize), String> {
    let after_open = line
        .strip_prefix("@@")
        .ok_or_else(|| format!("Invalid hunk header: `{line}`"))?;
    let end_marker = after_open
        .find("@@")
        .ok_or_else(|| format!("Unterminated hunk header (missing closing `@@`): `{line}`"))?;
    let inner = after_open[..end_marker].trim();
    if inner.is_empty() {
        return Ok((0, 0, 0, 0));
    }

    let mut old_start = 0usize;
    let mut old_count = 0usize;
    let mut new_start = 0usize;
    let mut new_count = 0usize;

    for tok in inner.split_whitespace() {
        if let Some(spec) = tok.strip_prefix('-') {
            let (s, c) = parse_range_spec(spec)?;
            old_start = s;
            old_count = c;
        } else if let Some(spec) = tok.strip_prefix('+') {
            let (s, c) = parse_range_spec(spec)?;
            new_start = s;
            new_count = c;
        }
    }
    Ok((old_start, old_count, new_start, new_count))
}

fn parse_range_spec(spec: &str) -> Result<(usize, usize), String> {
    if let Some((start_s, count_s)) = spec.split_once(',') {
        let start = start_s
            .parse::<usize>()
            .map_err(|_| format!("Invalid hunk line number `{start_s}`"))?;
        let count = count_s
            .parse::<usize>()
            .map_err(|_| format!("Invalid hunk line count `{count_s}`"))?;
        Ok((start, count))
    } else {
        let start = spec
            .parse::<usize>()
            .map_err(|_| format!("Invalid hunk line number `{spec}`"))?;
        Ok((start, 1))
    }
}

/// Applies all hunks of a unified diff atomically in memory using a 3-tier matching algorithm:
/// 1. Exact offset match at `old_start + cumulative_delta`
/// 2. Sliding-window FNV-64 hash-filtered exact context match nearest to `expected_idx`
/// 3. Sliding-window trailing-whitespace-normalized match nearest to `expected_idx`
///
/// Returns `(patched_content, hunks_applied, match_tier_label)`.
pub fn apply_unified_diff_in_memory(
    original: &str,
    diff_text: &str,
) -> Result<(String, usize, &'static str), String> {
    let hunks = parse_unified_diff(diff_text)?;
    let uses_crlf = original.contains("\r\n");
    let had_trailing_newline = original.ends_with('\n');
    let clean_orig = original.replace("\r\n", "\n");
    let mut lines: Vec<String> = if clean_orig.is_empty() {
        Vec::new()
    } else {
        clean_orig.lines().map(str::to_string).collect()
    };

    let mut cumulative_delta: isize = 0;
    let mut worst_tier: u8 = 1; // 1 = exact offset, 2 = relocated exact, 3 = whitespace-normalized

    for (hunk_idx, hunk) in hunks.iter().enumerate() {
        let expected_old: Vec<&str> = hunk
            .lines
            .iter()
            .filter_map(|l| match l {
                HunkLine::Context(s) | HunkLine::Remove(s) => Some(s.as_str()),
                HunkLine::Add(_) => None,
            })
            .collect();
        let replacement_new: Vec<String> = hunk
            .lines
            .iter()
            .filter_map(|l| match l {
                HunkLine::Context(s) | HunkLine::Add(s) => Some(s.clone()),
                HunkLine::Remove(_) => None,
            })
            .collect();

        // Pure insertion into empty file or at explicit start
        if expected_old.is_empty() {
            let insert_at = if hunk.old_start == 0 {
                0
            } else {
                let shifted = (hunk.old_start as isize - 1 + cumulative_delta).max(0) as usize;
                shifted.min(lines.len())
            };
            let added = replacement_new.len() as isize;
            lines.splice(insert_at..insert_at, replacement_new);
            cumulative_delta += added;
            continue;
        }

        let target_hint: Option<usize> = if hunk.old_start > 0 {
            let shifted = (hunk.old_start as isize - 1 + cumulative_delta).max(0) as usize;
            Some(shifted.min(lines.len()))
        } else {
            None
        };

        // Tier 1: Direct exact check at expected offset
        let matched = if let Some(idx) = target_hint {
            if matches_exact_at(&lines, idx, &expected_old) {
                Some((idx, 1u8))
            } else {
                None
            }
        } else {
            None
        }
        // Tier 2: Hash-accelerated exact sliding window across file
        .or_else(|| {
            find_sliding_match(&lines, &expected_old, target_hint, false).map(|idx| (idx, 2u8))
        })
        // Tier 3: Trailing-whitespace-normalized sliding window
        .or_else(|| {
            find_sliding_match(&lines, &expected_old, target_hint, true).map(|idx| (idx, 3u8))
        });

        let Some((match_idx, tier)) = matched else {
            let preview: String = expected_old
                .iter()
                .take(3)
                .copied()
                .collect::<Vec<_>>()
                .join(" | ");
            return Err(format!(
                "Unified diff hunk #{} (@@ -{},{} @@) could not match context in file: `{preview}`",
                hunk_idx + 1,
                hunk.old_start,
                hunk.old_count
            ));
        };

        worst_tier = worst_tier.max(tier);
        let old_len = expected_old.len();
        let new_len = replacement_new.len();
        lines.splice(match_idx..match_idx + old_len, replacement_new);
        cumulative_delta += new_len as isize - old_len as isize;
    }

    let line_sep = if uses_crlf { "\r\n" } else { "\n" };
    let mut out = lines.join(line_sep);
    if (had_trailing_newline || original.is_empty()) && !out.is_empty() {
        out.push_str(line_sep);
    }

    let tier_label = match worst_tier {
        1 => "unified-diff exact",
        2 => "unified-diff offset-relocated",
        _ => "unified-diff whitespace-normalized",
    };

    Ok((out, hunks.len(), tier_label))
}

#[inline]
fn line_fnv64(s: &str, trim_ws: bool) -> u64 {
    let bytes = if trim_ws {
        s.trim_end().as_bytes()
    } else {
        s.as_bytes()
    };
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in bytes {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

fn matches_exact_at(lines: &[String], start: usize, needle: &[&str]) -> bool {
    if start + needle.len() > lines.len() {
        return false;
    }
    lines[start..start + needle.len()]
        .iter()
        .zip(needle.iter())
        .all(|(a, b)| a.as_str() == *b)
}

fn find_sliding_match(
    lines: &[String],
    needle: &[&str],
    hint: Option<usize>,
    trim_ws: bool,
) -> Option<usize> {
    if needle.is_empty() || needle.len() > lines.len() {
        return None;
    }
    let line_hashes: Vec<u64> = lines.iter().map(|l| line_fnv64(l, trim_ws)).collect();
    let needle_hashes: Vec<u64> = needle.iter().map(|l| line_fnv64(l, trim_ws)).collect();
    let max_start = lines.len() - needle.len();

    let mut candidates = Vec::new();
    for start in 0..=max_start {
        if line_hashes[start..start + needle.len()] != needle_hashes[..] {
            continue;
        }
        let verified = lines[start..start + needle.len()]
            .iter()
            .zip(needle.iter())
            .all(|(a, b)| {
                if trim_ws {
                    a.trim_end() == b.trim_end()
                } else {
                    a.as_str() == *b
                }
            });
        if verified {
            candidates.push(start);
        }
    }

    match candidates.len() {
        0 => None,
        1 => Some(candidates[0]),
        _ => {
            // Multiple occurrences: pick strictly closest candidate to `hint` only if unique winner
            let h = hint?;
            candidates.sort_by_key(|&idx| idx.abs_diff(h));
            if candidates[0].abs_diff(h) < candidates[1].abs_diff(h) {
                Some(candidates[0])
            } else {
                None
            }
        }
    }
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

    #[test]
    fn test_apply_unified_diff_multi_hunk_exact_and_offset_relocated() {
        let orig =
            "fn a() -> i32 {\n    1\n}\n\nfn b() -> i32 {\n    2\n}\n\nfn c() -> i32 {\n    3\n}\n";
        let diff = "\
--- a/demo.rs
+++ b/demo.rs
@@ -1,3 +1,4 @@
 fn a() -> i32 {
-    1
+    let v = 10;
+    v
 }
@@ -9,3 +10,3 @@
 fn c() -> i32 {
-    3
+    30
 }
";
        let (patched, hunks, tier) =
            apply_unified_diff_in_memory(orig, diff).expect("unified diff applies");
        assert_eq!(hunks, 2);
        assert_eq!(tier, "unified-diff exact");
        assert!(patched.contains("let v = 10;\n    v"));
        assert!(patched.contains("fn c() -> i32 {\n    30\n}"));

        // Shifted/inaccurate line header still resolves cleanly via Tier 2 sliding-window hash match
        let shifted = "\
@@ -50,3 +50,3 @@
 fn b() -> i32 {
-    2
+    22
 }
";
        let (patched2, hunks2, tier2) =
            apply_unified_diff_in_memory(&patched, shifted).expect("shifted hunk applies");
        assert_eq!(hunks2, 1);
        assert_eq!(tier2, "unified-diff offset-relocated");
        assert!(patched2.contains("fn b() -> i32 {\n    22\n}"));
    }
}

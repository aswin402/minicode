//! Deterministic `Hashline` (`LINE#HASH|`) Content-Addressed Line Hashing & Surgical Range Patching (`oh-my-opencode` parity).
//!
//! Computes a fast, zero-allocation 2-character Base32 FNV-1a content hash for each source line
//! (`11#VK| fn main() {`) so agents can anchor edits by line number + content hash without
//! corrupting files when line numbers shift or when copying line-numbered `read_file` output.

use crate::error::{Result, ToolError};
use std::path::Path;

/// Unambiguous 32-character uppercase alphabet (excludes `0`, `O`, `1`, `I`).
const HASHLINE_ALPHABET: &[u8; 32] = b"23456789ABCDEFGHJKLMNPQRSTUVWXYZ";

/// Computes a deterministic 2-character content hash for a single line using 64-bit FNV-1a.
#[must_use]
pub fn compute_line_hash(line: &str) -> [u8; 2] {
    let trimmed = line.trim();
    let bytes = if trimmed.is_empty() {
        line.as_bytes()
    } else {
        trimmed.as_bytes()
    };
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    let folded = ((hash ^ (hash >> 10) ^ (hash >> 20)) & 0x03ff) as usize;
    let hi = HASHLINE_ALPHABET[(folded >> 5) & 0x1f];
    let lo = HASHLINE_ALPHABET[folded & 0x1f];
    [hi, lo]
}

/// Returns the 2-character `Hashline` tag as a `String` (e.g. `"VK"`).
#[must_use]
pub fn line_hash_str(line: &str) -> String {
    let pair = compute_line_hash(line);
    String::from_utf8_lossy(&pair).into_owned()
}

/// Formats a 1-indexed line with its `LINE#HASH|` anchor (`"  11#VK| fn main() {\n"`).
#[must_use]
pub fn format_hashline(line_num: usize, line: &str) -> String {
    let pair = compute_line_hash(line);
    let tag = std::str::from_utf8(&pair).unwrap_or("22");
    format!("{line_num:>4}#{tag}| {line}\n")
}

/// Strips accidental `read_file` prefixes (`"  11#VK| "` or `"  11 | "`) from a multi-line code block
/// using a pure byte scanner (no regexes) when every non-empty line in the block carries a line prefix.
#[must_use]
pub fn strip_line_prefixes(block: &str) -> String {
    let lines: Vec<&str> = block.lines().collect();
    if lines.is_empty() {
        return block.to_string();
    }

    let non_empty: Vec<&str> = lines
        .iter()
        .copied()
        .filter(|l| !l.trim().is_empty())
        .collect();
    if non_empty.is_empty()
        || !non_empty
            .iter()
            .all(|l| split_single_line_prefix(l).is_some())
    {
        return block.to_string();
    }

    let mut stripped = Vec::with_capacity(lines.len());
    for line in &lines {
        if let Some(rest) = split_single_line_prefix(line) {
            stripped.push(rest);
        } else {
            stripped.push(*line);
        }
    }
    let mut out = stripped.join("\n");
    if block.ends_with('\n') {
        out.push('\n');
    }
    out
}

fn split_single_line_prefix(line: &str) -> Option<&str> {
    let trimmed_start = line.trim_start_matches(' ');
    let digit_len = trimmed_start
        .bytes()
        .take_while(|b| b.is_ascii_digit())
        .count();
    if digit_len == 0 || digit_len > 6 {
        return None;
    }
    let after_digits = &trimmed_start[digit_len..];
    // Pattern 1: `<digits>#<2-char-hash>| ` or `<digits>#<2-char-hash>|`
    if let Some(after_hash_sep) = after_digits.strip_prefix('#') {
        if after_hash_sep.len() >= 3 {
            let tag_bytes = &after_hash_sep.as_bytes()[..2];
            if tag_bytes.iter().all(|b| b.is_ascii_alphanumeric())
                && after_hash_sep.as_bytes()[2] == b'|'
            {
                let rest = &after_hash_sep[3..];
                return Some(rest.strip_prefix(' ').unwrap_or(rest));
            }
        }
    }
    // Pattern 2: `<digits> | `
    if let Some(after_pipe) = after_digits.strip_prefix(" |") {
        return Some(after_pipe.strip_prefix(' ').unwrap_or(after_pipe));
    }
    None
}

/// Applies a content-addressed `Hashline` range replacement (`start_line..=end_line`) to `relative_path`,
/// verifying optional `start_hash` and `end_hash` anchors before mutating the file.
pub fn patch_file_by_hashline_range(
    workspace_root: &Path,
    relative_path: &str,
    start_line: usize,
    end_line: usize,
    start_hash: Option<&str>,
    end_hash: Option<&str>,
    replace_block: &str,
) -> Result<String> {
    let target_path =
        crate::sandbox::path::validate_path_in_workspace(workspace_root, Path::new(relative_path))?;
    let original = std::fs::read_to_string(&target_path).map_err(|e| ToolError::FileOp {
        path: relative_path.to_string(),
        source: e,
    })?;

    let lines: Vec<&str> = original.lines().collect();
    if start_line == 0 || end_line == 0 || start_line > end_line || end_line > lines.len() {
        return Err(ToolError::PatchFailed {
            path: relative_path.to_string(),
            reason: format!(
                "Invalid hashline range {}..={} (file has {} lines).",
                start_line,
                end_line,
                lines.len()
            ),
        }
        .into());
    }

    if let Some(expected_start) = start_hash.map(str::trim).filter(|s| !s.is_empty()) {
        let expected_clean = expected_start
            .rsplit_once('#')
            .map(|(_, h)| h)
            .unwrap_or(expected_start)
            .trim_end_matches('|');
        let actual_start = line_hash_str(lines[start_line - 1]);
        if !actual_start.eq_ignore_ascii_case(expected_clean) {
            return Err(ToolError::PatchFailed {
                path: relative_path.to_string(),
                reason: format!(
                    "Hashline mismatch at start_line {}: expected `{}#{}`, found `{}#{}` (`{}`). Re-read lines {}..={} with `read_file(hashline=true)`.",
                    start_line,
                    start_line,
                    expected_clean,
                    start_line,
                    actual_start,
                    lines[start_line - 1].trim(),
                    start_line.saturating_sub(2).max(1),
                    (end_line + 2).min(lines.len())
                ),
            }
            .into());
        }
    }

    if let Some(expected_end) = end_hash.map(str::trim).filter(|s| !s.is_empty()) {
        let expected_clean = expected_end
            .rsplit_once('#')
            .map(|(_, h)| h)
            .unwrap_or(expected_end)
            .trim_end_matches('|');
        let actual_end = line_hash_str(lines[end_line - 1]);
        if !actual_end.eq_ignore_ascii_case(expected_clean) {
            return Err(ToolError::PatchFailed {
                path: relative_path.to_string(),
                reason: format!(
                    "Hashline mismatch at end_line {}: expected `{}#{}`, found `{}#{}` (`{}`). Re-read lines {}..={} with `read_file(hashline=true)`.",
                    end_line,
                    end_line,
                    expected_clean,
                    end_line,
                    actual_end,
                    lines[end_line - 1].trim(),
                    start_line.saturating_sub(2).max(1),
                    (end_line + 2).min(lines.len())
                ),
            }
            .into());
        }
    }

    let cleaned_replace = strip_line_prefixes(replace_block);
    let mut new_lines: Vec<String> = Vec::with_capacity(lines.len());
    for l in &lines[..start_line - 1] {
        new_lines.push((*l).to_string());
    }
    if !cleaned_replace.is_empty() {
        for r_line in cleaned_replace.lines() {
            new_lines.push(r_line.to_string());
        }
    }
    for l in &lines[end_line..] {
        new_lines.push((*l).to_string());
    }

    let mut new_content = new_lines.join("\n");
    if original.ends_with('\n') && !new_content.ends_with('\n') {
        new_content.push('\n');
    }

    let write_res = crate::tools::fs::write_file(workspace_root, relative_path, &new_content)?;
    Ok(format!(
        "Successfully patched {} via Hashline range L{}..=L{} ({})",
        relative_path, start_line, end_line, write_res
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hashline_computation_and_prefix_stripping() {
        let h1 = line_hash_str("fn authenticate(token: &str) -> bool {");
        let h2 = line_hash_str("    fn authenticate(token: &str) -> bool {   ");
        assert_eq!(h1.len(), 2);
        assert_eq!(h1, h2, "Trimmed lines should produce identical 2-char hash");

        let formatted = format_hashline(11, "let x = 42;");
        assert!(formatted.starts_with(&format!(
            "  11#{}| let x = 42;\n",
            line_hash_str("let x = 42;")
        )));

        let copied = format!(
            "  11#{}| let x = 42;\n  12#{}| let y = x + 1;",
            line_hash_str("let x = 42;"),
            line_hash_str("let y = x + 1;")
        );
        assert_eq!(strip_line_prefixes(&copied), "let x = 42;\nlet y = x + 1;");

        let copied_std = "  11 | let x = 42;\n  12 | let y = x + 1;";
        assert_eq!(
            strip_line_prefixes(copied_std),
            "let x = 42;\nlet y = x + 1;"
        );
    }

    #[test]
    fn test_hashline_range_patch_and_mismatch_guard() {
        let dir = tempfile::tempdir().expect("tempdir");
        let ws = dir.path();
        let file_path = ws.join("sample.rs");
        std::fs::write(
            &file_path,
            "fn a() {}\nfn b() {\n    let old = 1;\n}\nfn c() {}\n",
        )
        .expect("write sample");

        let h_start = line_hash_str("fn b() {");
        let h_end = line_hash_str("}");

        // Mismatched hash must fail safely without mutating the file
        let bad = patch_file_by_hashline_range(
            ws,
            "sample.rs",
            2,
            4,
            Some("ZZ"),
            Some(&h_end),
            "fn b() {\n    let updated = 2;\n}",
        );
        assert!(bad.is_err());

        // Matching hashes succeed
        let ok = patch_file_by_hashline_range(
            ws,
            "sample.rs",
            2,
            4,
            Some(&h_start),
            Some(&h_end),
            "fn b() {\n    let updated = 2;\n}",
        )
        .expect("hashline range patch");
        assert!(ok.contains("Hashline range L2..=L4"));
        let disk = std::fs::read_to_string(&file_path).expect("read patched");
        assert!(disk.contains("let updated = 2;"));
        assert!(!disk.contains("let old = 1;"));
    }
}

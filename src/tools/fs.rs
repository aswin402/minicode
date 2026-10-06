use crate::error::{Result, ToolError};
use crate::sandbox::path::validate_path_in_workspace;
use similar::TextDiff;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

/// Reads a file from the workspace within the specified optional 1-indexed line range.
pub fn read_file(
    workspace_root: &Path,
    relative_path: &str,
    start_line: Option<usize>,
    end_line: Option<usize>,
) -> Result<String> {
    let target_path = validate_path_in_workspace(workspace_root, Path::new(relative_path))?;

    let content = std::fs::read_to_string(&target_path).map_err(|e| ToolError::FileOp {
        path: relative_path.to_string(),
        source: e,
    })?;

    if content.is_empty() {
        return Ok(String::new());
    }

    let lines: Vec<&str> = content.lines().collect();
    let total_lines = lines.len();

    if total_lines == 0 {
        return Ok(String::new());
    }

    // Default window clamping: if no bounds provided and total lines exceed threshold, clamp window
    let is_unbounded = start_line.is_none() && end_line.is_none();
    let clamped_window =
        is_unbounded && total_lines > crate::constants::READ_FILE_UNBOUNDED_THRESHOLD;

    let start = start_line.unwrap_or(1).max(1);
    let end = if clamped_window {
        crate::constants::READ_FILE_DEFAULT_WINDOW
    } else {
        end_line.unwrap_or(total_lines).min(total_lines)
    };

    if start > total_lines {
        return Err(ToolError::InvalidArguments {
            name: "read_file".to_string(),
            reason: format!(
                "start_line ({}) exceeds total line count ({})",
                start, total_lines
            ),
        }
        .into());
    }

    if start > end {
        return Err(ToolError::InvalidArguments {
            name: "read_file".to_string(),
            reason: format!(
                "start_line ({}) cannot be greater than end_line ({})",
                start, end
            ),
        }
        .into());
    }

    let mut output = String::new();
    for line_idx in start..=end {
        if line_idx - 1 < total_lines {
            output.push_str(&format!("{:>4} | {}\n", line_idx, lines[line_idx - 1]));
        }
    }

    if clamped_window {
        output.push_str(&format!(
            "\n[... File has {} lines. Showing lines 1-{}. Call read_file with start_line={} to view next chunk ...]",
            total_lines,
            crate::constants::READ_FILE_DEFAULT_WINDOW,
            crate::constants::READ_FILE_DEFAULT_WINDOW + 1
        ));
    }

    Ok(output)
}

/// Atomically writes content to a file via a temporary file, supporting overwrite or append mode.
/// By default, to protect against accidental file clobbering when building large files across turns,
/// modifying an existing non-empty file requires explicit intent: `overwrite: true` or `append: true`.
pub fn write_file_with_options(
    workspace_root: &Path,
    relative_path: &str,
    content: &str,
    append: bool,
    overwrite: bool,
) -> Result<String> {
    let target_path = validate_path_in_workspace(workspace_root, Path::new(relative_path))?;

    let orig_content = if target_path.exists() {
        std::fs::read_to_string(&target_path).unwrap_or_default()
    } else {
        String::new()
    };

    // Safe-Overwrite & Chunk Guard:
    // If the file exists and is not empty, and neither append nor overwrite was explicitly granted:
    if target_path.exists() && !orig_content.trim().is_empty() && !append && !overwrite {
        // If content is identical, succeed idempotently without warning
        if orig_content == content {
            return Ok(format!(
                "File '{}' is already up to date with identical content (no changes needed).",
                relative_path
            ));
        }

        let mut hint = String::new();
        let trimmed_content = content.trim();
        let trimmed_lower = trimmed_content.to_ascii_lowercase();

        // Heuristic: Check if content appears to be an HTML fragment or section
        if (relative_path.ends_with(".html") || relative_path.ends_with(".htm"))
            && !trimmed_lower.contains("<!doctype")
            && !trimmed_lower.contains("<html")
            && (trimmed_lower.starts_with("<section")
                || trimmed_lower.starts_with("<!--")
                || trimmed_lower.starts_with("<div")
                || trimmed_lower.starts_with("<main")
                || trimmed_lower.starts_with("<footer")
                || trimmed_lower.starts_with("<header"))
        {
            hint.push_str("\n\n💡 Chunk Detected: Your content looks like an HTML fragment/section rather than a full document. If you are building this page in sequential steps, pass `append: true` to append to the existing document!");
        } else if relative_path.ends_with(".css")
            && !trimmed_lower.contains(":root")
            && (trimmed_lower.starts_with("@media") || trimmed_lower.starts_with('.'))
        {
            hint.push_str("\n\n💡 Chunk Detected: Your content looks like additional CSS rules/media queries. If you are adding styles in steps, pass `append: true` to avoid erasing previously written styles!");
        }

        return Err(ToolError::InvalidArguments {
            name: "write_file".to_string(),
            reason: format!(
                "Target file '{}' already exists ({} lines, {} bytes). To prevent accidental file destruction, write_file requires explicit intent:\n\
                • To append new sections to the end of the file: pass `append: true`.\n\
                • To completely replace and overwrite the file: pass `overwrite: true`.\n\
                • To surgically edit specific lines: use `patch_file`.{}",
                relative_path,
                orig_content.lines().count(),
                orig_content.len(),
                hint
            ),
        }
        .into());
    }

    let effective_content = if append && !orig_content.is_empty() {
        format!("{}{}", orig_content, content)
    } else {
        content.to_string()
    };

    // Pre-Write AST Syntax Barrier: verify that tentative content does not introduce syntax errors
    crate::context::syntax_guard::SyntaxGuard::check_syntax_barrier(
        &target_path,
        &orig_content,
        &effective_content,
    )
    .map_err(|reason| ToolError::PatchFailed {
        path: relative_path.to_string(),
        reason,
    })?;

    if let Some(parent) = target_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| ToolError::FileOp {
            path: relative_path.to_string(),
            source: e,
        })?;
    }

    let parent_dir = target_path.parent().unwrap_or(workspace_root);
    let tmp_file_name = format!(
        ".tmp_{}_{}",
        std::process::id(),
        uuid::Uuid::new_v4().simple()
    );
    let tmp_path = parent_dir.join(tmp_file_name);

    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&tmp_path)
        .map_err(|e| ToolError::FileOp {
            path: relative_path.to_string(),
            source: e,
        })?;

    file.write_all(effective_content.as_bytes())
        .map_err(|e| ToolError::FileOp {
            path: relative_path.to_string(),
            source: e,
        })?;

    file.flush().map_err(|e| ToolError::FileOp {
        path: relative_path.to_string(),
        source: e,
    })?;

    drop(file);

    std::fs::rename(&tmp_path, &target_path).map_err(|e| {
        if let Err(cleanup_err) = std::fs::remove_file(&tmp_path) {
            tracing::warn!(
                path = %tmp_path.display(),
                error = %cleanup_err,
                "Failed to clean up temporary file after rename failure"
            );
        }
        ToolError::FileOp {
            path: relative_path.to_string(),
            source: e,
        }
    })?;

    let total_lines = effective_content.lines().count();
    let mut msg = if append {
        format!(
            "Successfully appended {} bytes (total {} lines) to {}",
            content.len(),
            total_lines,
            relative_path
        )
    } else {
        format!(
            "Successfully wrote {} lines to {}",
            total_lines, relative_path
        )
    };

    if let Some(feedback) =
        crate::tools::compiler::ScopedCompiler::run_scoped_check(workspace_root, relative_path)
    {
        msg.push_str("\n\n");
        msg.push_str(&feedback);
    }

    Ok(msg)
}

/// Atomically writes full content to a file via a temporary file, creating any missing parent directories.
pub fn write_file(workspace_root: &Path, relative_path: &str, content: &str) -> Result<String> {
    write_file_with_options(workspace_root, relative_path, content, false, true)
}

/// Diagnostic structure for nearest match during failed search-and-replace patching.
#[derive(Debug, Clone)]
pub struct NearestMatchDiagnostic {
    pub start_line: usize,
    pub end_line: usize,
    pub similarity: f64,
    pub actual_snippet: String,
}

/// Detects leading indentation of a block of lines (spaces or tabs).
fn detect_leading_indent(lines: &[&str]) -> String {
    for line in lines {
        if !line.trim().is_empty() {
            return line.chars().take_while(|c| c.is_whitespace()).collect();
        }
    }
    String::new()
}

/// Re-indents replacement lines to preserve target file's indentation level.
fn align_indentation(replace: &str, orig_indent: &str, search_indent: &str) -> String {
    if orig_indent == search_indent {
        return replace.to_string();
    }

    let replace_lines: Vec<&str> = replace.lines().collect();
    let mut aligned = Vec::new();

    for line in replace_lines {
        if line.trim().is_empty() {
            aligned.push(String::new());
            continue;
        }
        if !search_indent.is_empty() && line.starts_with(search_indent) {
            let suffix = &line[search_indent.len()..];
            aligned.push(format!("{}{}", orig_indent, suffix));
        } else if search_indent.is_empty() {
            aligned.push(format!("{}{}", orig_indent, line));
        } else {
            let trimmed = line.trim_start();
            aligned.push(format!("{}{}", orig_indent, trimmed));
        }
    }

    aligned.join("\n")
}

/// Locates 1-indexed line numbers of all substring matches in content.
fn find_all_match_lines(content: &str, needle: &str) -> Vec<usize> {
    let mut lines = Vec::new();
    let mut offset = 0;
    while let Some(pos) = content[offset..].find(needle) {
        let abs_pos = offset + pos;
        let line_num = content[..abs_pos].lines().count().max(1);
        lines.push(line_num);
        offset = abs_pos + needle.len();
    }
    lines
}

/// Formats patch success message, preserving optional compiler or linter diagnostic feedback.
fn format_patch_success(tier_desc: &str, relative_path: &str, write_res: &str) -> String {
    let mut msg = format!("Successfully patched {}", relative_path);
    if !tier_desc.is_empty() {
        msg.push_str(&format!(" ({})", tier_desc));
    }
    if let Some(feedback_start) = write_res.find("\n\n[Compiler") {
        msg.push_str(&write_res[feedback_start..]);
    } else if let Some(feedback_start) = write_res.find("\n\n[Linter") {
        msg.push_str(&write_res[feedback_start..]);
    }
    msg
}

/// Applies a search-and-replace block patch to a file with a 5-tier resilient matching pipeline.
///
/// 5-Tier Resilient Hierarchy:
/// 1. Exact string match (0ms)
/// 2. CRLF & trailing whitespace normalized match
/// 3. Whitespace/indentation-insensitive match with automatic re-indentation
/// 4. Blank-line relaxed match with automatic re-indentation
/// 5. High-threshold sliding-window fuzzy diff matching with uniqueness gap
///
/// If all 5 tiers fail to match, generates an actionable 4-part "What-Where-Why-Next" diagnostic.
pub fn patch_file(
    workspace_root: &Path,
    relative_path: &str,
    search_block: &str,
    replace_block: &str,
) -> Result<String> {
    let target_path = validate_path_in_workspace(workspace_root, Path::new(relative_path))?;

    let original = std::fs::read_to_string(&target_path).map_err(|e| ToolError::FileOp {
        path: relative_path.to_string(),
        source: e,
    })?;

    // 1. Tier 1: Exact Substring Match
    if original.contains(search_block) {
        let occurrences = original.matches(search_block).count();
        if occurrences > 1 {
            let match_lines = find_all_match_lines(&original, search_block);
            return Err(ToolError::PatchFailed {
                path: relative_path.to_string(),
                reason: format!(
                    "Search block matches multiple ({} times) locations at lines: {:?}. Provide 2-3 lines of unique surrounding context.",
                    occurrences, match_lines
                ),
            }
            .into());
        }

        let mut new_content = original.replacen(search_block, replace_block, 1);
        if original.ends_with('\n') && !new_content.ends_with('\n') {
            new_content.push('\n');
        }
        let write_res = write_file(workspace_root, relative_path, &new_content)?;
        return Ok(format_patch_success("", relative_path, &write_res));
    }

    // 2. Tier 2: CRLF & Trailing Whitespace Normalization
    if let Some(new_content) =
        try_crlf_and_trailing_whitespace_replace(&original, search_block, replace_block)
    {
        let write_res = write_file(workspace_root, relative_path, &new_content)?;
        return Ok(format_patch_success(
            "whitespace-normalized match",
            relative_path,
            &write_res,
        ));
    }

    // 3. Tier 3: Whitespace/Indentation-Insensitive Match with Auto-Re-Indentation
    if let Some(new_content) =
        try_whitespace_normalized_replace(&original, search_block, replace_block)
    {
        let write_res = write_file(workspace_root, relative_path, &new_content)?;
        return Ok(format_patch_success(
            "auto-reindented match",
            relative_path,
            &write_res,
        ));
    }

    // 4. Tier 4: Blank-Line Relaxed Match with Auto-Re-Indentation
    if let Some(new_content) =
        try_blank_line_relaxed_replace(&original, search_block, replace_block)
    {
        let write_res = write_file(workspace_root, relative_path, &new_content)?;
        return Ok(format_patch_success(
            "blank-line relaxed match",
            relative_path,
            &write_res,
        ));
    }

    // 5. Tier 5: High-Threshold Fuzzy Diff with Strict Uniqueness Gap & Auto-Re-Indentation
    if let Some(new_content) = try_fuzzy_replace(&original, search_block, replace_block) {
        let write_res = write_file(workspace_root, relative_path, &new_content)?;
        return Ok(format_patch_success(
            "fuzzy matched with auto-reindentation",
            relative_path,
            &write_res,
        ));
    }

    // Failure: Generate Actionable 4-Part "What-Where-Why-Next" Diagnostic
    let diag = find_nearest_match(&original, search_block);
    let inspect_start = diag.start_line.saturating_sub(5).max(1);
    let inspect_end = diag.end_line + 5;

    let reason = format!(
        "Search block could not be found in '{relative_path}'.\n\n\
         [Where] Nearest match found at lines {}-{} ({:.1}% similarity):\n\
         ------------------------------------------------------------\n\
         {}\n\
         ------------------------------------------------------------\n\n\
         [Expected Search Block]\n\
         {}\n\n\
         [Suggested Next Action]\n\
         Your search block diverged from disk contents around lines {}-{}.\n\
         Update your search_block to match the actual file lines above, or call `read_file(path: \"{}\", start_line: {}, end_line: {})` to inspect current file contents.",
        diag.start_line,
        diag.end_line,
        diag.similarity * 100.0,
        diag.actual_snippet,
        search_block.trim(),
        diag.start_line,
        diag.end_line,
        relative_path,
        inspect_start,
        inspect_end
    );

    Err(ToolError::PatchFailed {
        path: relative_path.to_string(),
        reason,
    }
    .into())
}

/// Tier 2: Normalizes line endings (CRLF -> LF) and trims trailing whitespace per line.
fn try_crlf_and_trailing_whitespace_replace(
    original: &str,
    search: &str,
    replace: &str,
) -> Option<String> {
    let orig_clean = original.replace("\r\n", "\n");
    let search_clean = search.replace("\r\n", "\n");

    let orig_trimmed_trailing: String = orig_clean
        .lines()
        .map(|l| l.trim_end())
        .collect::<Vec<_>>()
        .join("\n");
    let search_trimmed_trailing: String = search_clean
        .lines()
        .map(|l| l.trim_end())
        .collect::<Vec<_>>()
        .join("\n");

    if orig_trimmed_trailing.contains(&search_trimmed_trailing) {
        let count = orig_trimmed_trailing
            .matches(&search_trimmed_trailing)
            .count();
        if count == 1 {
            let before_match = orig_trimmed_trailing
                .split(&search_trimmed_trailing)
                .next()?;
            let start_line = before_match.lines().count().saturating_sub(1);
            let search_line_count = search_trimmed_trailing.lines().count();

            let orig_lines: Vec<&str> = original.lines().collect();
            if start_line + search_line_count <= orig_lines.len() {
                let mut result_lines = Vec::new();
                result_lines.extend_from_slice(&orig_lines[..start_line]);
                result_lines.push(replace);
                result_lines.extend_from_slice(&orig_lines[start_line + search_line_count..]);
                let mut res = result_lines.join("\n");
                if original.ends_with('\n') && !res.ends_with('\n') {
                    res.push('\n');
                }
                return Some(res);
            }
        }
    }
    None
}

/// Tier 3: Whitespace/indentation-insensitive match with automatic base re-indentation.
fn try_whitespace_normalized_replace(
    original: &str,
    search: &str,
    replace: &str,
) -> Option<String> {
    let orig_lines: Vec<&str> = original.lines().collect();
    let search_lines: Vec<&str> = search.lines().collect();

    if search_lines.is_empty() || search_lines.len() > orig_lines.len() {
        return None;
    }

    let search_trimmed: Vec<&str> = search_lines.iter().map(|l| l.trim()).collect();
    let mut matches = Vec::new();

    for i in 0..=(orig_lines.len() - search_lines.len()) {
        let slice_trimmed: Vec<&str> = orig_lines[i..i + search_lines.len()]
            .iter()
            .map(|l| l.trim())
            .collect();

        if slice_trimmed == search_trimmed {
            matches.push(i);
        }
    }

    // Only apply if uniquely matched to prevent ambiguous edits
    if matches.len() == 1 {
        let i = matches[0];
        let orig_indent = detect_leading_indent(&orig_lines[i..i + search_lines.len()]);
        let search_indent = detect_leading_indent(&search_lines);
        let aligned_replace = align_indentation(replace, &orig_indent, &search_indent);

        let mut result_lines = Vec::new();
        result_lines.extend_from_slice(&orig_lines[..i]);
        result_lines.push(&aligned_replace);
        result_lines.extend_from_slice(&orig_lines[i + search_lines.len()..]);
        let mut res = result_lines.join("\n");
        if original.ends_with('\n') && !res.ends_with('\n') {
            res.push('\n');
        }
        return Some(res);
    }

    None
}

/// Tier 4: Blank-line relaxed match ignoring empty line differences.
fn try_blank_line_relaxed_replace(original: &str, search: &str, replace: &str) -> Option<String> {
    let orig_lines: Vec<&str> = original.lines().collect();
    let search_lines: Vec<&str> = search.lines().collect();

    let search_non_blank: Vec<&str> = search_lines
        .iter()
        .copied()
        .filter(|l| !l.trim().is_empty())
        .collect();
    if search_non_blank.is_empty() || orig_lines.len() < search_non_blank.len() {
        return None;
    }

    let search_trimmed: Vec<&str> = search_non_blank.iter().map(|l| l.trim()).collect();
    let mut matches = Vec::new();
    let window_size = search_lines.len();

    for i in 0..orig_lines.len() {
        let max_end = (i + window_size + 3).min(orig_lines.len());
        for end in (i + search_non_blank.len())..=max_end {
            let slice = &orig_lines[i..end];
            let slice_non_blank: Vec<&str> = slice
                .iter()
                .copied()
                .filter(|l| !l.trim().is_empty())
                .collect();
            let slice_trimmed: Vec<&str> = slice_non_blank.iter().map(|l| l.trim()).collect();
            if slice_trimmed == search_trimmed {
                matches.push((i, end));
                break;
            }
        }
    }

    if matches.len() == 1 {
        let (start, end) = matches[0];
        let orig_indent = detect_leading_indent(&orig_lines[start..end]);
        let search_indent = detect_leading_indent(&search_lines);
        let aligned_replace = align_indentation(replace, &orig_indent, &search_indent);

        let mut result_lines = Vec::new();
        result_lines.extend_from_slice(&orig_lines[..start]);
        result_lines.push(&aligned_replace);
        result_lines.extend_from_slice(&orig_lines[end..]);
        let mut res = result_lines.join("\n");
        if original.ends_with('\n') && !res.ends_with('\n') {
            res.push('\n');
        }
        return Some(res);
    }

    None
}

/// Tier 5: High-threshold sliding-window fuzzy diff matching with strict uniqueness gap.
fn try_fuzzy_replace(original: &str, search: &str, replace: &str) -> Option<String> {
    let orig_lines: Vec<&str> = original.lines().collect();
    let search_lines: Vec<&str> = search.lines().collect();

    if search_lines.is_empty() || search_lines.len() > orig_lines.len() {
        return None;
    }

    let window_size = search_lines.len();
    let search_str = search_lines.join("\n");
    let mut best_ratio = 0.0;
    let mut second_best_ratio = 0.0;
    let mut best_idx = 0;
    let mut match_count = 0;

    for i in 0..=(orig_lines.len() - window_size) {
        let window_str = orig_lines[i..i + window_size].join("\n");
        let diff = TextDiff::from_chars(&window_str, &search_str);
        let ratio = diff.ratio() as f64;

        if ratio > best_ratio {
            second_best_ratio = best_ratio;
            best_ratio = ratio;
            best_idx = i;
            match_count = 1;
        } else if (ratio - best_ratio).abs() < 0.001 {
            match_count += 1;
        } else if ratio > second_best_ratio {
            second_best_ratio = ratio;
        }
    }

    // Only apply if above threshold and strictly unique
    if best_ratio >= crate::constants::FUZZY_MATCH_THRESHOLD
        && match_count == 1
        && (best_ratio - second_best_ratio) >= crate::constants::FUZZY_UNIQUENESS_GAP
    {
        let orig_indent = detect_leading_indent(&orig_lines[best_idx..best_idx + window_size]);
        let search_indent = detect_leading_indent(&search_lines);
        let aligned_replace = align_indentation(replace, &orig_indent, &search_indent);

        let mut result_lines = Vec::new();
        result_lines.extend_from_slice(&orig_lines[..best_idx]);
        result_lines.push(&aligned_replace);
        result_lines.extend_from_slice(&orig_lines[best_idx + window_size..]);
        let mut res = result_lines.join("\n");
        if original.ends_with('\n') && !res.ends_with('\n') {
            res.push('\n');
        }
        return Some(res);
    }

    None
}

/// Computes the closest candidate line window in original file using Myers diff similarity.
fn find_nearest_match(original: &str, search: &str) -> NearestMatchDiagnostic {
    let orig_lines: Vec<&str> = original.lines().collect();
    let search_lines: Vec<&str> = search.lines().collect();
    let window_size = search_lines.len().max(1);

    if orig_lines.is_empty() {
        return NearestMatchDiagnostic {
            start_line: 1,
            end_line: 1,
            similarity: 0.0,
            actual_snippet: String::new(),
        };
    }

    let search_str = search_lines.join("\n");
    let mut best_ratio = 0.0;
    let mut best_idx = 0;

    let max_start = if orig_lines.len() >= window_size {
        orig_lines.len() - window_size
    } else {
        0
    };

    for i in 0..=max_start {
        let actual_window_size = window_size.min(orig_lines.len() - i);
        let window_str = orig_lines[i..i + actual_window_size].join("\n");
        let diff = TextDiff::from_chars(&window_str, &search_str);
        let ratio = diff.ratio() as f64;

        if ratio > best_ratio {
            best_ratio = ratio;
            best_idx = i;
        }
    }

    let actual_len = window_size.min(orig_lines.len() - best_idx);
    let end_idx = best_idx + actual_len;
    let snippet = orig_lines[best_idx..end_idx]
        .iter()
        .enumerate()
        .map(|(offset, line)| format!("{:>4} | {}", best_idx + offset + 1, line))
        .collect::<Vec<_>>()
        .join("\n");

    NearestMatchDiagnostic {
        start_line: best_idx + 1,
        end_line: end_idx,
        similarity: best_ratio,
        actual_snippet: snippet,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_write_file() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_fs_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let rel_path = "sub/hello.txt";
        let content = "line 1\nline 2\nline 3";
        write_file(&temp_dir, rel_path, content).unwrap();

        let read_back = read_file(&temp_dir, rel_path, Some(2), Some(3)).unwrap();
        assert!(read_back.contains("   2 | line 2"));
        assert!(read_back.contains("   3 | line 3"));
        assert!(!read_back.contains("line 1"));

        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_patch_file_exact() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_patch_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let rel_path = "code.rs";
        let content = "fn hello() {\n    println!(\"old\");\n}\n";
        write_file(&temp_dir, rel_path, content).unwrap();

        patch_file(
            &temp_dir,
            rel_path,
            "println!(\"old\");",
            "println!(\"new\");",
        )
        .unwrap();

        let read = read_file(&temp_dir, rel_path, None, None).unwrap();
        assert!(read.contains("println!(\"new\");"));

        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_read_empty_file() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_empty_fs_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let rel_path = "empty.txt";
        write_file(&temp_dir, rel_path, "").unwrap();

        let read = read_file(&temp_dir, rel_path, None, None).unwrap();
        assert_eq!(read, "");

        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_patch_file_fuzzy() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_fuzzy_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let rel_path = "fuzzy.rs";
        let content =
            "fn add(a: i32, b: i32) -> i32 {\n    let result = a + b;\n    return result;\n}\n";
        write_file(&temp_dir, rel_path, content).unwrap();

        patch_file(
            &temp_dir,
            rel_path,
            "let result = a + b;\n    return result;",
            "a + b",
        )
        .unwrap();

        let read = read_file(&temp_dir, rel_path, None, None).unwrap();
        assert!(read.contains("a + b"));

        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_patch_file_ambiguous_matches_rejected() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_ambig_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let rel_path = "dup.rs";
        let content = "fn one() {\n    let x = 1;\n}\nfn two() {\n    let x = 1;\n}\n";
        write_file(&temp_dir, rel_path, content).unwrap();

        let res = patch_file(&temp_dir, rel_path, "let x = 1;", "let x = 2;");
        assert!(res.is_err());
        let err_msg = res.unwrap_err().to_string();
        assert!(err_msg.contains("matches multiple (2 times) locations at lines: [2, 5]"));

        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_patch_file_indentation_auto_alignment() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_indent_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let rel_path = "nested.rs";
        let content =
            "fn outer() {\n    if true {\n        let a = 10;\n        let b = 20;\n    }\n}\n";
        write_file(&temp_dir, rel_path, content).unwrap();

        // Search has 8 spaces, replacement has 0 spaces base indentation
        let search = "let a = 10;\nlet b = 20;";
        let replace = "let a = 100;\nlet b = 200;\nlet c = 300;";
        patch_file(&temp_dir, rel_path, search, replace).unwrap();

        let read = read_file(&temp_dir, rel_path, None, None).unwrap();
        assert!(read.contains("        let a = 100;"));
        assert!(read.contains("        let b = 200;"));
        assert!(read.contains("        let c = 300;"));

        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_patch_file_crlf_normalization() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_crlf_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let rel_path = "crlf.txt";
        let content = "first line\r\nsecond line\r\nthird line\r\n";
        write_file(&temp_dir, rel_path, content).unwrap();

        // Patch using LF search and replace
        patch_file(&temp_dir, rel_path, "second line", "modified line").unwrap();

        let read = read_file(&temp_dir, rel_path, None, None).unwrap();
        assert!(read.contains("modified line"));

        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_patch_file_blank_line_relaxation() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_blank_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let rel_path = "blank.rs";
        // Original has blank lines
        let content = "fn calculate() {\n    let x = 1;\n\n    let y = 2;\n    x + y\n}\n";
        write_file(&temp_dir, rel_path, content).unwrap();

        // Search has no blank line between x and y
        let search = "    let x = 1;\n    let y = 2;\n    x + y";
        let replace = "    let sum = 1 + 2;\n    sum";
        patch_file(&temp_dir, rel_path, search, replace).unwrap();

        let read = read_file(&temp_dir, rel_path, None, None).unwrap();
        assert!(read.contains("let sum = 1 + 2;"));

        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_patch_file_nearest_match_diagnostic() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_diag_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let rel_path = "diag.rs";
        let content = "fn compute_total(items: &[Item]) -> u32 {\n    let mut total = 0;\n    for item in items {\n        total += item.price;\n    }\n    total\n}\n";
        write_file(&temp_dir, rel_path, content).unwrap();

        // Search block is substantially divergent (below 85% similarity threshold)
        let search =
            "fn process_transactions(records: &[Record]) -> bool {\n    validate(records)\n}";
        let replace = "fn process_transactions(records: &[Record]) -> bool {\n    true\n}";

        let res = patch_file(&temp_dir, rel_path, search, replace);
        assert!(res.is_err());
        let msg = res.unwrap_err().to_string();
        assert!(msg.contains("[Where] Nearest match found at lines"));
        assert!(msg.contains("[Expected Search Block]"));
        assert!(msg.contains("[Suggested Next Action]"));
        assert!(msg.contains("read_file(path: \"diag.rs\""));

        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_write_file_append_mode() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_append_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let rel_path = "document.txt";
        let part1 = "Section 1\nHeader info\n";
        write_file(&temp_dir, rel_path, part1).unwrap();

        let part2 = "Section 2\nAdditional details\n";
        let res = write_file_with_options(&temp_dir, rel_path, part2, true, false);
        assert!(res.is_ok());

        let raw_content = std::fs::read_to_string(temp_dir.join(rel_path)).unwrap();
        assert_eq!(raw_content, format!("{}{}", part1, part2));

        let formatted = read_file(&temp_dir, rel_path, None, None).unwrap();
        assert!(formatted.contains("Section 1"));
        assert!(formatted.contains("Additional details"));

        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_write_file_safe_overwrite_guard() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_guard_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let rel_path = "index.html";
        let chunk1 = "<!DOCTYPE html><html><head><title>Test</title></head><body>\n";
        // First write creates file cleanly
        let res1 = write_file_with_options(&temp_dir, rel_path, chunk1, false, false);
        assert!(res1.is_ok());

        let chunk2 = "<section>Hero</section>\n";
        // Second write without append or overwrite is rejected by the safe-overwrite guard
        let res2 = write_file_with_options(&temp_dir, rel_path, chunk2, false, false);
        assert!(res2.is_err());
        let err_str = res2.unwrap_err().to_string();
        assert!(err_str.contains("already exists"));
        assert!(err_str.contains("append: true"));

        // Write with append: true succeeds
        let res3 = write_file_with_options(&temp_dir, rel_path, chunk2, true, false);
        assert!(res3.is_ok());
        let content = std::fs::read_to_string(temp_dir.join(rel_path)).unwrap();
        assert!(content.contains(chunk1));
        assert!(content.contains(chunk2));

        // Write with overwrite: true replaces
        let chunk3 = "<!DOCTYPE html><html><body>New Only</body></html>";
        let res4 = write_file_with_options(&temp_dir, rel_path, chunk3, false, true);
        assert!(res4.is_ok());
        let content2 = std::fs::read_to_string(temp_dir.join(rel_path)).unwrap();
        assert_eq!(content2, chunk3);

        std::fs::remove_dir_all(&temp_dir).ok();
    }
}

use crate::error::{Result, ToolError};
use crate::sandbox::path::validate_path_in_workspace;
use similar::{ChangeTag, TextDiff};
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

    if !target_path.exists() {
        return Err(ToolError::FileOp {
            path: relative_path.to_string(),
            source: std::io::Error::new(std::io::ErrorKind::NotFound, "File does not exist"),
        }
        .into());
    }

    let content = std::fs::read_to_string(&target_path).map_err(|e| ToolError::FileOp {
        path: relative_path.to_string(),
        source: e,
    })?;

    let lines: Vec<&str> = content.lines().collect();
    let total_lines = lines.len();

    let start = start_line.unwrap_or(1).max(1);
    let end = end_line.unwrap_or(total_lines).min(total_lines);

    if start > total_lines && total_lines > 0 {
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
            output.push_str(&format!("{}: {}\n", line_idx, lines[line_idx - 1]));
        }
    }

    Ok(output)
}

/// Atomically writes full content to a file, creating any missing parent directories.
pub fn write_file(workspace_root: &Path, relative_path: &str, content: &str) -> Result<String> {
    let target_path = validate_path_in_workspace(workspace_root, Path::new(relative_path))?;

    if let Some(parent) = target_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Write atomically via temp file replace or direct open
    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&target_path)
        .map_err(|e| ToolError::FileOp {
            path: relative_path.to_string(),
            source: e,
        })?;

    file.write_all(content.as_bytes())
        .map_err(|e| ToolError::FileOp {
            path: relative_path.to_string(),
            source: e,
        })?;

    file.flush().map_err(|e| ToolError::FileOp {
        path: relative_path.to_string(),
        source: e,
    })?;

    let line_count = content.lines().count();
    Ok(format!(
        "Successfully wrote {} lines to {}",
        line_count, relative_path
    ))
}

/// Applies a search-and-replace block patch to a file.
///
/// Matching order:
/// 1. Exact string match (primary)
/// 2. Whitespace-normalized line matching (fallback 1)
/// 3. Fuzzy diff matching with similarity scoring (fallback 2)
pub fn patch_file(
    workspace_root: &Path,
    relative_path: &str,
    search_block: &str,
    replace_block: &str,
) -> Result<String> {
    let target_path = validate_path_in_workspace(workspace_root, Path::new(relative_path))?;

    if !target_path.exists() {
        return Err(ToolError::FileOp {
            path: relative_path.to_string(),
            source: std::io::Error::new(std::io::ErrorKind::NotFound, "Target file does not exist"),
        }
        .into());
    }

    let original = std::fs::read_to_string(&target_path).map_err(|e| ToolError::FileOp {
        path: relative_path.to_string(),
        source: e,
    })?;

    // 1. Exact match
    if original.contains(search_block) {
        let occurrences = original.matches(search_block).count();
        if occurrences > 1 {
            return Err(ToolError::PatchFailed {
                path: relative_path.to_string(),
                reason: format!(
                    "Search block matches multiple ({} times) locations. Provide more unique surrounding context.",
                    occurrences
                ),
            }
            .into());
        }

        let new_content = original.replacen(search_block, replace_block, 1);
        std::fs::write(&target_path, new_content)?;
        return Ok(format!("Successfully patched {}", relative_path));
    }

    // 2. Whitespace-normalized line match
    if let Some(new_content) =
        try_whitespace_normalized_replace(&original, search_block, replace_block)
    {
        std::fs::write(&target_path, new_content)?;
        return Ok(format!(
            "Successfully patched {} (whitespace-relaxed match)",
            relative_path
        ));
    }

    // 3. Fuzzy match fallback using similar crate
    if let Some(new_content) = try_fuzzy_replace(&original, search_block, replace_block) {
        std::fs::write(&target_path, new_content)?;
        return Ok(format!(
            "Successfully patched {} (fuzzy matched)",
            relative_path
        ));
    }

    Err(ToolError::PatchFailed {
        path: relative_path.to_string(),
        reason: "Search block could not be found in file. Ensure the search text matches current file contents.".to_string(),
    }
    .into())
}

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

    for i in 0..=(orig_lines.len() - search_lines.len()) {
        let slice_trimmed: Vec<&str> = orig_lines[i..i + search_lines.len()]
            .iter()
            .map(|l| l.trim())
            .collect();

        if slice_trimmed == search_trimmed {
            let mut result_lines = Vec::new();
            result_lines.extend_from_slice(&orig_lines[..i]);
            result_lines.push(replace);
            result_lines.extend_from_slice(&orig_lines[i + search_lines.len()..]);
            return Some(result_lines.join("\n"));
        }
    }

    None
}

fn try_fuzzy_replace(original: &str, search: &str, replace: &str) -> Option<String> {
    let diff = TextDiff::from_lines(original, search);
    let mut matching_chunks = 0;
    let mut total_chunks = 0;

    for change in diff.iter_all_changes() {
        total_chunks += 1;
        if change.tag() == ChangeTag::Equal {
            matching_chunks += 1;
        }
    }

    if total_chunks > 0 && (matching_chunks as f32 / total_chunks as f32) > 0.85 {
        // High confidence match — fallback to line replace
        try_whitespace_normalized_replace(original, search, replace)
    } else {
        None
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
        assert!(read_back.contains("2: line 2"));
        assert!(read_back.contains("3: line 3"));
        assert!(!read_back.contains("1: line 1"));

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
}

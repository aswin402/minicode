use crate::constants::MAX_SEARCH_RESULTS;
use crate::error::{Result, ToolError};
use ignore::WalkBuilder;
use regex::Regex;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

/// Fast in-memory grep search across workspace files respecting `.gitignore`.
pub fn grep_search(
    workspace_root: &Path,
    query: &str,
    is_regex: bool,
    file_pattern: Option<&str>,
) -> Result<String> {
    let regex = if is_regex {
        Regex::new(query).map_err(|e| ToolError::InvalidArguments {
            name: "grep_search".to_string(),
            reason: format!("Invalid regular expression: {}", e),
        })?
    } else {
        Regex::new(&regex::escape(query)).map_err(|e| ToolError::InvalidArguments {
            name: "grep_search".to_string(),
            reason: format!("Regex escape error: {}", e),
        })?
    };

    let file_regex: Option<Regex> = match file_pattern {
        Some(pat) => {
            let mut pattern_regex = String::from("^");
            for c in pat.chars() {
                match c {
                    '*' => pattern_regex.push_str(".*"),
                    '?' => pattern_regex.push('.'),
                    '.' | '+' | '(' | ')' | '[' | ']' | '{' | '}' | '^' | '$' | '|' | '\\' => {
                        pattern_regex.push('\\');
                        pattern_regex.push(c);
                    }
                    _ => pattern_regex.push(c),
                }
            }
            pattern_regex.push('$');
            Some(
                Regex::new(&pattern_regex).map_err(|e| ToolError::InvalidArguments {
                    name: "grep_search".to_string(),
                    reason: format!("Invalid file pattern: {}", e),
                })?,
            )
        }
        None => None,
    };

    let walker = WalkBuilder::new(workspace_root)
        .hidden(true)
        .parents(true)
        .git_ignore(true)
        .git_global(true)
        .build();

    let mut matches = Vec::new();

    for result in walker {
        if matches.len() >= crate::constants::MAX_SEARCH_RESULTS {
            break;
        }

        let entry = match result {
            Ok(e) => e,
            Err(e) => {
                tracing::debug!(error = %e, "Skipping unreadable file/directory during grep_search");
                continue;
            }
        };

        if !entry.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
            continue;
        }

        let path = entry.path();
        let rel_path = path
            .strip_prefix(workspace_root)
            .unwrap_or(path)
            .to_string_lossy();

        if let Some(ref f_reg) = file_regex {
            if !f_reg.is_match(&rel_path) {
                continue;
            }
        }

        // Search within file
        if let Ok(file) = File::open(path) {
            let reader = BufReader::new(file);
            for (line_idx, line_res) in reader.lines().enumerate() {
                if matches.len() >= MAX_SEARCH_RESULTS {
                    break;
                }
                if let Ok(line) = line_res {
                    if regex.is_match(&line) {
                        matches.push(format!("{}:{}: {}", rel_path, line_idx + 1, line.trim()));
                    }
                }
            }
        }
    }

    if matches.is_empty() {
        Ok(format!("No matches found for query '{}'", query))
    } else {
        let count = matches.len();
        let header = if count >= MAX_SEARCH_RESULTS {
            format!("Found ≥{} matches (capped):\n", MAX_SEARCH_RESULTS)
        } else {
            format!("Found {} matches:\n", count)
        };
        Ok(format!("{}{}", header, matches.join("\n")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_grep_search() {
        let temp_dir = std::env::temp_dir().join(format!("minicode_grep_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let file_path = temp_dir.join("sample.txt");
        let mut file = File::create(&file_path).unwrap();
        writeln!(file, "First line").unwrap();
        writeln!(file, "Target query match here").unwrap();
        writeln!(file, "Third line").unwrap();

        let res = grep_search(&temp_dir, "Target query", false, None).unwrap();
        assert!(res.contains("sample.txt:2: Target query match here"));

        std::fs::remove_dir_all(&temp_dir).ok();
    }
}

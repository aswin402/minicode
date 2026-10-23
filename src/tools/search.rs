use crate::constants::{MAX_REGEX_QUERY_LEN, MAX_SEARCH_RESULTS};
use crate::error::{Result, ToolError};
use ignore::WalkBuilder;
use regex::Regex;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

fn build_walker(root: &Path) -> ignore::Walk {
    let mut builder = WalkBuilder::new(root);
    builder
        .hidden(true)
        .parents(true)
        .git_ignore(true)
        .git_global(true)
        .filter_entry(|entry| {
            if let Some(name) = entry.file_name().to_str() {
                !crate::context::walker::is_excluded_dir_name(name)
            } else {
                true
            }
        });
    builder.build()
}

/// Fast in-memory grep search across workspace files respecting `.gitignore`.
pub fn grep_search(
    workspace_root: &Path,
    query: &str,
    is_regex: bool,
    file_pattern: Option<&str>,
) -> Result<String> {
    if query.len() > MAX_REGEX_QUERY_LEN {
        return Err(ToolError::InvalidArguments {
            name: "grep_search".to_string(),
            reason: format!(
                "Search query exceeds maximum length of {} characters (received {})",
                MAX_REGEX_QUERY_LEN,
                query.len()
            ),
        }
        .into());
    }

    let compiled_regex = if is_regex {
        Some(Regex::new(query).map_err(|e| ToolError::InvalidArguments {
            name: "grep_search".to_string(),
            reason: format!("Invalid regular expression: {}", e),
        })?)
    } else {
        None
    };

    let walker = build_walker(workspace_root);
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
        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy())
            .unwrap_or_default();

        if let Some(pat) = file_pattern {
            if !crate::utils::strings::wildcard_match(&rel_path, pat)
                && !crate::utils::strings::wildcard_match(&file_name, pat)
            {
                continue;
            }
        }

        // Search within file
        match File::open(path) {
            Ok(file) => {
                let reader = BufReader::new(file);
                for (line_idx, line_res) in reader.lines().enumerate() {
                    if matches.len() >= MAX_SEARCH_RESULTS {
                        break;
                    }
                    if let Ok(line) = line_res {
                        let matched = match compiled_regex {
                            Some(ref re) => re.is_match(&line),
                            None => line.contains(query),
                        };
                        if matched {
                            matches.push(format!(
                                "{}:{}: {}",
                                rel_path,
                                line_idx + 1,
                                line.trim_end()
                            ));
                        }
                    }
                }
            }
            Err(e) => {
                tracing::debug!(
                    path = %path.display(),
                    error = %e,
                    "Skipping unreadable file during grep search"
                );
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

/// Converts a glob pattern (with support for *, **, ?, [...], and {a,b,c} brace expansion) into a case-insensitive regex.
pub fn glob_to_regex(pattern: &str) -> Option<Regex> {
    let mut regex_str = String::from("(?i)^");
    let mut chars = pattern.chars().peekable();
    let mut brace_depth: usize = 0;

    while let Some(c) = chars.next() {
        match c {
            '*' => {
                if chars.peek() == Some(&'*') {
                    chars.next();
                    if chars.peek() == Some(&'/') {
                        chars.next();
                        regex_str.push_str("(?:.*/)?");
                    } else {
                        regex_str.push_str(".*");
                    }
                } else {
                    regex_str.push_str(".*");
                }
            }
            '?' => regex_str.push('.'),
            '{' => {
                brace_depth += 1;
                regex_str.push_str("(?:");
            }
            '}' if brace_depth > 0 => {
                brace_depth -= 1;
                regex_str.push(')');
            }
            ',' if brace_depth > 0 => {
                regex_str.push('|');
            }
            '[' => {
                let mut class_content = String::new();
                let mut closed = false;
                if chars.peek() == Some(&'!') {
                    chars.next();
                    class_content.push('^');
                }
                while let Some(nc) = chars.next() {
                    if nc == ']' && !class_content.is_empty() {
                        closed = true;
                        break;
                    }
                    if nc == '\\' {
                        class_content.push('\\');
                        if let Some(esc) = chars.next() {
                            class_content.push(esc);
                        }
                    } else {
                        class_content.push(nc);
                    }
                }
                if closed {
                    regex_str.push('[');
                    regex_str.push_str(&class_content);
                    regex_str.push(']');
                } else {
                    regex_str.push_str("\\[");
                    regex_str.push_str(&regex::escape(&class_content));
                }
            }
            '.' | '+' | '(' | ')' | ']' | '^' | '$' | '|' | '\\' => {
                regex_str.push('\\');
                regex_str.push(c);
            }
            '}' => {
                regex_str.push('\\');
                regex_str.push('}');
            }
            _ => regex_str.push(c),
        }
    }
    while brace_depth > 0 {
        regex_str.push(')');
        brace_depth -= 1;
    }
    regex_str.push('$');
    Regex::new(&regex_str).ok()
}

/// Fast file locator finding files by name or glob pattern respecting `.gitignore`.
pub fn file_search(
    workspace_root: &Path,
    pattern: &str,
    subpath: Option<&str>,
    limit: Option<usize>,
) -> Result<String> {
    let max_results = limit.unwrap_or(50).min(200);

    let search_dir = match subpath {
        Some(sub) => {
            let clean = sub.trim().trim_start_matches('/');
            if clean.is_empty() {
                workspace_root.to_path_buf()
            } else {
                if Path::new(clean)
                    .components()
                    .any(|c| matches!(c, std::path::Component::ParentDir))
                {
                    return Err(ToolError::InvalidArguments {
                        name: "file_search".to_string(),
                        reason: "Search subpath cannot contain '..' traversal".to_string(),
                    }
                    .into());
                }
                let candidate = workspace_root.join(clean);
                if !candidate.exists() {
                    return Ok(format!("Search directory '{}' does not exist.", sub));
                }
                candidate
            }
        }
        None => workspace_root.to_path_buf(),
    };

    let is_glob = pattern.contains('*')
        || pattern.contains('?')
        || pattern.contains('{')
        || pattern.contains('[');

    let file_regex: Option<Regex> = if is_glob {
        glob_to_regex(pattern)
    } else {
        Regex::new(&format!("(?i){}", regex::escape(pattern))).ok()
    };
    let pat_lower = pattern.to_lowercase();

    let walker = build_walker(&search_dir);
    let mut matches = Vec::new();

    for result in walker {
        if matches.len() >= max_results {
            break;
        }

        let entry = match result {
            Ok(e) => e,
            Err(_) => continue,
        };

        if !entry.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
            continue;
        }

        let path = entry.path();
        let rel_path = path
            .strip_prefix(workspace_root)
            .unwrap_or(path)
            .to_string_lossy();
        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy())
            .unwrap_or_default();

        let matched = if let Some(ref reg) = file_regex {
            reg.is_match(&file_name) || reg.is_match(&rel_path)
        } else {
            file_name.to_lowercase().contains(&pat_lower)
                || rel_path.to_lowercase().contains(&pat_lower)
        };

        if matched {
            matches.push(rel_path.to_string());
        }
    }

    if matches.is_empty() {
        Ok(format!("No files found matching pattern '{}'", pattern))
    } else {
        let count = matches.len();
        let header = if count >= max_results {
            format!(
                "Found ≥{} files matching '{}' (capped):\n",
                max_results, pattern
            )
        } else {
            format!("Found {} file(s) matching '{}':\n", count, pattern)
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

    #[test]
    fn test_grep_search_rejects_oversized_query() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_grep_len_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let long_query = "a".repeat(MAX_REGEX_QUERY_LEN + 1);
        let res = grep_search(&temp_dir, &long_query, false, None);
        assert!(res.is_err());

        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_file_search() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_fsearch_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(temp_dir.join("src/sub")).unwrap();

        let f1 = temp_dir.join("src/main.rs");
        let f2 = temp_dir.join("src/sub/view.rs");
        let f3 = temp_dir.join("README.md");
        File::create(&f1).unwrap();
        File::create(&f2).unwrap();
        File::create(&f3).unwrap();

        let res = file_search(&temp_dir, "*.rs", None, None).unwrap();
        assert!(res.contains("src/main.rs"));
        assert!(res.contains("src/sub/view.rs"));
        assert!(!res.contains("README.md"));

        let res_sub = file_search(&temp_dir, "*.rs", Some("src/sub"), None).unwrap();
        assert!(res_sub.contains("src/sub/view.rs"));
        assert!(!res_sub.contains("src/main.rs"));

        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_glob_to_regex_brace_expansion() {
        let re = glob_to_regex("**/*.{tsx,jsx,ts,js,css}").unwrap();
        assert!(re.is_match("src/components/App.tsx"));
        assert!(re.is_match("index.css"));
        assert!(re.is_match("main.ts"));
        assert!(re.is_match("utils.js"));
        assert!(!re.is_match("cargo.toml"));
        assert!(!re.is_match("image.png"));

        let re_multi = glob_to_regex("*.{rs,toml}").unwrap();
        assert!(re_multi.is_match("main.rs"));
        assert!(re_multi.is_match("Cargo.toml"));
        assert!(!re_multi.is_match("main.py"));
    }
}

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

/// Fast trigram-accelerated grep search across workspace files respecting `.gitignore`.
///
/// Uses `TrigramIndex` (`microsoft/tgrep` architecture: 24-bit folded trigram hashes +
/// 8-bit `loc_mask` / `next_mask` Bloom filters + smallest-first posting intersection)
/// to narrow candidate files before scanning lines.
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

    let mut matches = Vec::new();
    let mut enriched_files_count = 0usize;

    if let Ok(trigram_idx) =
        crate::context::search::trigram::TrigramIndex::get_or_build_cached(workspace_root)
    {
        let plan = crate::context::search::trigram::build_query_plan(query, is_regex);
        let candidate_rel_paths = trigram_idx.query_candidate_files(&plan, file_pattern);

        for rel_path in candidate_rel_paths {
            if matches.len() >= MAX_SEARCH_RESULTS {
                break;
            }
            let path = workspace_root.join(rel_path);
            scan_file_for_matches(
                &path,
                rel_path,
                query,
                compiled_regex.as_ref(),
                &mut matches,
                &mut enriched_files_count,
            );
        }
    } else {
        let walker = build_walker(workspace_root);
        for result in walker {
            if matches.len() >= MAX_SEARCH_RESULTS {
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

            scan_file_for_matches(
                path,
                &rel_path,
                query,
                compiled_regex.as_ref(),
                &mut matches,
                &mut enriched_files_count,
            );
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

/// Maximum number of matched files per `grep_search` call to enrich with AST/Markdown structural containers
/// (`zvec-grep` `RG_STRUCTURE_ENRICH_FILE_LIMIT = 50`).
const RG_STRUCTURE_ENRICH_FILE_LIMIT: usize = 50;

struct StructuralContainer {
    kind: String,
    name: String,
    start_line: usize,
    end_line: usize,
}

fn extract_file_containers(path: &Path, content: &str) -> Vec<StructuralContainer> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    if matches!(ext, "md" | "mdx") {
        return crate::context::search::semantic::extract_markdown_sections(content)
            .into_iter()
            .filter_map(|sec| {
                let kind = format!("h{}", sec.level.max(1));
                let name = if !sec.breadcrumb.is_empty() {
                    sec.breadcrumb
                } else {
                    sec.heading
                };
                if name.is_empty() {
                    return None;
                }
                Some(StructuralContainer {
                    kind,
                    name,
                    start_line: sec.start_line,
                    end_line: sec.end_line,
                })
            })
            .collect();
    }

    if matches!(ext, "rs" | "py" | "js" | "jsx" | "ts" | "tsx") {
        let extractor = crate::context::repomap::RepoMapExtractor::new();
        if let Ok(symbols) = extractor.extract_symbols_from_str(ext, content) {
            return symbols
                .into_iter()
                .map(|sym| StructuralContainer {
                    kind: sym.kind,
                    name: sym.name,
                    start_line: sym.line_number,
                    end_line: sym.end_line,
                })
                .collect();
        }
    }

    Vec::new()
}

fn find_smallest_enclosing_container(
    containers: &[StructuralContainer],
    line_num: usize,
) -> Option<&StructuralContainer> {
    // Follow `zvec-grep` `isRedundantDeclarationBlock`: only annotate lines strictly inside the container body
    // (`start_line < line_num && line_num <= end_line`), skipping the declaration line itself.
    containers
        .iter()
        .filter(|c| c.start_line < line_num && line_num <= c.end_line)
        .min_by_key(|c| c.end_line.saturating_sub(c.start_line))
}

fn scan_file_for_matches(
    path: &Path,
    rel_path: &str,
    query: &str,
    compiled_regex: Option<&Regex>,
    matches: &mut Vec<String>,
    enriched_files_count: &mut usize,
) {
    match File::open(path) {
        Ok(file) => {
            let reader = BufReader::new(file);
            let mut raw_hits: Vec<(usize, String)> = Vec::new();
            for (line_idx, line_res) in reader.lines().enumerate() {
                if matches.len() + raw_hits.len() >= MAX_SEARCH_RESULTS {
                    break;
                }
                if let Ok(line) = line_res {
                    let matched = match compiled_regex {
                        Some(re) => re.is_match(&line),
                        None => line.contains(query),
                    };
                    if matched {
                        raw_hits.push((line_idx + 1, line.trim_end().to_string()));
                    }
                }
            }

            if raw_hits.is_empty() {
                return;
            }

            let containers = if *enriched_files_count < RG_STRUCTURE_ENRICH_FILE_LIMIT {
                *enriched_files_count += 1;
                std::fs::read_to_string(path)
                    .ok()
                    .map(|content| extract_file_containers(path, &content))
                    .unwrap_or_default()
            } else {
                Vec::new()
            };

            for (line_num, line_text) in raw_hits {
                if let Some(container) = find_smallest_enclosing_container(&containers, line_num) {
                    matches.push(format!(
                        "{}:{}: {}  [in {} {} L{}-L{}]",
                        rel_path,
                        line_num,
                        line_text,
                        container.kind,
                        container.name,
                        container.start_line,
                        container.end_line
                    ));
                } else {
                    matches.push(format!("{}:{}: {}", rel_path, line_num, line_text));
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
///
/// Serves queries directly from the cached `TrigramIndex::all_files()` path table
/// (`microsoft/tgrep` `path_index` optimization) instead of walking the filesystem on every call.
pub fn file_search(
    workspace_root: &Path,
    pattern: &str,
    subpath: Option<&str>,
    limit: Option<usize>,
) -> Result<String> {
    let max_results = limit.unwrap_or(50).min(200);

    let (search_dir, clean_subpath) = match subpath {
        Some(sub) => {
            let clean = sub.trim().trim_start_matches('/').trim_end_matches('/');
            if clean.is_empty() {
                (workspace_root.to_path_buf(), None)
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
                (candidate, Some(clean))
            }
        }
        None => (workspace_root.to_path_buf(), None),
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

    let mut matches = Vec::new();

    if let Ok(trigram_idx) =
        crate::context::search::trigram::TrigramIndex::get_or_build_cached(workspace_root)
    {
        for rel_path in trigram_idx.all_files() {
            if matches.len() >= max_results {
                break;
            }

            if let Some(sub_prefix) = clean_subpath {
                if !Path::new(rel_path).starts_with(sub_prefix) {
                    continue;
                }
            }

            let file_name = Path::new(rel_path)
                .file_name()
                .map(|n| n.to_string_lossy())
                .unwrap_or_default();

            let matched = if let Some(ref reg) = file_regex {
                reg.is_match(&file_name) || reg.is_match(rel_path)
            } else {
                file_name.to_lowercase().contains(&pat_lower)
                    || rel_path.to_lowercase().contains(&pat_lower)
            };

            if matched {
                matches.push(rel_path.clone());
            }
        }
    } else {
        let walker = build_walker(&search_dir);

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
    fn test_grep_search_trigram_regex_alternation_and_skip_binary() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_grep_tri_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let mut f1 = File::create(temp_dir.join("alpha.rs")).unwrap();
        writeln!(f1, "pub fn handle_alpha_request() {{}}").unwrap();

        let mut f2 = File::create(temp_dir.join("beta.rs")).unwrap();
        writeln!(f2, "pub fn process_beta_response() {{}}").unwrap();

        let mut f3 = File::create(temp_dir.join("blob.bin")).unwrap();
        f3.write_all(b"binary\0handle_alpha_request\0payload")
            .unwrap();

        let res = grep_search(
            &temp_dir,
            "handle_alpha_request|process_beta_response",
            true,
            Some("*.rs"),
        )
        .unwrap();
        assert!(res.contains("alpha.rs:1: pub fn handle_alpha_request() {}"));
        assert!(res.contains("beta.rs:1: pub fn process_beta_response() {}"));
        assert!(!res.contains("blob.bin"));

        // Even with a short 1-char query (`MatchAll`), binary files with NUL bytes are skipped
        let res_short = grep_search(&temp_dir, "h", false, None).unwrap();
        assert!(res_short.contains("alpha.rs"));
        assert!(!res_short.contains("blob.bin"));

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

    #[test]
    fn test_grep_search_structure_enrichment() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_grep_struct_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();

        std::fs::write(
            temp_dir.join("service.rs"),
            "pub fn process_payment() {\n    let status = charge_card();\n    let receipt = status;\n}\n",
        )
        .unwrap();

        std::fs::write(
            temp_dir.join("guide.md"),
            "# Deployment\n\n## Rollback Procedure\n\nRun the emergency rollback script immediately.\n",
        )
        .unwrap();

        let res_code = grep_search(&temp_dir, "charge_card", false, Some("*.rs")).unwrap();
        assert!(
            res_code.contains("[in function process_payment L1-L4]"),
            "Expected AST structural container tag in grep output, got: {}",
            res_code
        );

        let res_md = grep_search(&temp_dir, "emergency rollback", false, Some("*.md")).unwrap();
        assert!(
            res_md.contains("[in h2 Deployment > Rollback Procedure L3-L5]"),
            "Expected Markdown heading breadcrumb tag in grep output, got: {}",
            res_md
        );

        std::fs::remove_dir_all(&temp_dir).ok();
    }
}

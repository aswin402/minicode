use crate::error::Result;
use crate::git::service::GitService;
use std::path::Path;

/// A single line in a git unified diff
#[derive(Debug, Clone)]
pub struct GitDiffLine {
    /// Line prefix/type: `+` (addition), `-` (deletion), ` ` (context), `@` (hunk header)
    pub tag: char,
    /// Line text content
    pub content: String,
    /// Line number in the original file
    pub old_lineno: Option<usize>,
    /// Line number in the new file
    pub new_lineno: Option<usize>,
}

/// A modified file and its parsed diff lines
#[derive(Debug, Clone)]
pub struct GitDiffFile {
    /// Relative path to modified file
    pub path: String,
    /// Status indicator: 'M' (Modified), 'A' (Added), 'D' (Deleted), '?' (Untracked)
    pub status_char: char,
    /// Whether changes are staged in index
    pub is_staged: bool,
    /// Total lines added
    pub additions: usize,
    /// Total lines deleted
    pub deletions: usize,
    /// Parsed unified diff lines
    pub lines: Vec<GitDiffLine>,
}

/// Engine to load, parse, and render interactive git diffs.
pub struct GitDiffViewer;

impl GitDiffViewer {
    /// Loads and parses all staged or unstaged diffs for the given workspace.
    pub async fn load_diffs(workspace_root: &Path, staged_only: bool) -> Result<Vec<GitDiffFile>> {
        let git = GitService::new(workspace_root.to_path_buf());
        if !git.is_git_repo().await {
            return Ok(vec![]);
        }

        let raw_diff = git.diff(staged_only, None).await?;
        let status = git.get_status().await?;

        let mut diff_files = Self::parse_raw_diff(&raw_diff, staged_only);

        // Also add untracked files if unstaged
        if !staged_only {
            for untracked in status.untracked {
                if !diff_files.iter().any(|f| f.path == untracked) {
                    let full_path = workspace_root.join(&untracked);
                    let mut lines = Vec::new();
                    let mut additions = 0;
                    if let Ok(content) = tokio::fs::read_to_string(&full_path).await {
                        for (idx, line) in content.lines().enumerate() {
                            additions += 1;
                            lines.push(GitDiffLine {
                                tag: '+',
                                content: line.to_string(),
                                old_lineno: None,
                                new_lineno: Some(idx + 1),
                            });
                        }
                    }
                    diff_files.push(GitDiffFile {
                        path: untracked,
                        status_char: '?',
                        is_staged: false,
                        additions,
                        deletions: 0,
                        lines,
                    });
                }
            }
        }

        diff_files.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(diff_files)
    }

    /// Parses raw unified diff string (`diff --git a/... b/...`) into structured `GitDiffFile` list.
    pub fn parse_raw_diff(raw_diff: &str, is_staged: bool) -> Vec<GitDiffFile> {
        let mut files = Vec::new();
        let mut current_file: Option<GitDiffFile> = None;
        let mut old_line_cur = 0;
        let mut new_line_cur = 0;

        for line in raw_diff.lines() {
            if line.starts_with("diff --git ") {
                if let Some(f) = current_file.take() {
                    files.push(f);
                }

                // Extract filename from "diff --git a/path/to/file b/path/to/file"
                let parts: Vec<&str> = line.split_whitespace().collect();
                let path = if parts.len() >= 4 {
                    parts[3].trim_start_matches("b/").to_string()
                } else {
                    "unknown".to_string()
                };

                current_file = Some(GitDiffFile {
                    path,
                    status_char: 'M',
                    is_staged,
                    additions: 0,
                    deletions: 0,
                    lines: Vec::new(),
                });
                continue;
            }

            if let Some(ref mut cur) = current_file {
                if line.starts_with("new file mode ") {
                    cur.status_char = 'A';
                    continue;
                }
                if line.starts_with("deleted file mode ") {
                    cur.status_char = 'D';
                    continue;
                }
                if line.starts_with("index ")
                    || line.starts_with("--- ")
                    || line.starts_with("+++ ")
                {
                    continue;
                }

                if line.starts_with("@@ ") {
                    // Hunk header: e.g. @@ -10,5 +10,8 @@
                    cur.lines.push(GitDiffLine {
                        tag: '@',
                        content: line.to_string(),
                        old_lineno: None,
                        new_lineno: None,
                    });

                    // Parse line numbers from @@ -old_start,old_count +new_start,new_count @@
                    let hunk_parts: Vec<&str> = line.split("@@").collect();
                    if hunk_parts.len() >= 2 {
                        let numbers = hunk_parts[1].trim();
                        for token in numbers.split_whitespace() {
                            if token.starts_with('-') {
                                let start = token
                                    .trim_start_matches('-')
                                    .split(',')
                                    .next()
                                    .unwrap_or("1");
                                old_line_cur = start.parse::<usize>().unwrap_or(1);
                            } else if token.starts_with('+') {
                                let start = token
                                    .trim_start_matches('+')
                                    .split(',')
                                    .next()
                                    .unwrap_or("1");
                                new_line_cur = start.parse::<usize>().unwrap_or(1);
                            }
                        }
                    }
                    continue;
                }

                if let Some(first_char) = line.chars().next() {
                    match first_char {
                        '+' => {
                            cur.additions += 1;
                            cur.lines.push(GitDiffLine {
                                tag: '+',
                                content: line[1..].to_string(),
                                old_lineno: None,
                                new_lineno: Some(new_line_cur),
                            });
                            new_line_cur += 1;
                        }
                        '-' => {
                            cur.deletions += 1;
                            cur.lines.push(GitDiffLine {
                                tag: '-',
                                content: line[1..].to_string(),
                                old_lineno: Some(old_line_cur),
                                new_lineno: None,
                            });
                            old_line_cur += 1;
                        }
                        ' ' => {
                            cur.lines.push(GitDiffLine {
                                tag: ' ',
                                content: line[1..].to_string(),
                                old_lineno: Some(old_line_cur),
                                new_lineno: Some(new_line_cur),
                            });
                            old_line_cur += 1;
                            new_line_cur += 1;
                        }
                        _ => {
                            // Raw text or fallback
                            cur.lines.push(GitDiffLine {
                                tag: ' ',
                                content: line.to_string(),
                                old_lineno: None,
                                new_lineno: None,
                            });
                        }
                    }
                }
            }
        }

        if let Some(f) = current_file {
            files.push(f);
        }

        files
    }
}

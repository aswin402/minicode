use crate::error::{ContextError, Result, ToolError};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// A single atomic text/AST edit applied during refactoring
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RefactorEdit {
    pub file_path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub original_text: String,
    pub replacement_text: String,
}

/// The result and unified diff preview of an automated AST refactoring action
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RefactorResult {
    pub action: String,
    pub files_modified: Vec<String>,
    pub edits: Vec<RefactorEdit>,
    pub diff_preview: String,
}

pub struct AstRefactorer;

impl AstRefactorer {
    /// Extracts a line span into a standalone helper function and replaces the span with a call
    #[allow(clippy::too_many_arguments)]
    pub fn extract_function(
        workspace_root: &Path,
        file_path: &str,
        start_line: usize,
        end_line: usize,
        new_fn_name: &str,
        params: &str,
        call_args: &str,
        return_type: Option<&str>,
        is_pub: bool,
    ) -> Result<RefactorResult> {
        let full_path = Self::resolve_workspace_path(workspace_root, file_path)?;
        let content = fs::read_to_string(&full_path).map_err(|e| ToolError::FileOp {
            path: file_path.to_string(),
            source: e,
        })?;

        let lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
        if start_line == 0 || end_line < start_line || end_line > lines.len() {
            return Err(ContextError::TreeSitter(format!(
                "Invalid line range {}-{} for file with {} lines",
                start_line,
                end_line,
                lines.len()
            ))
            .into());
        }

        let original_slice = lines[start_line - 1..end_line].join("\n");
        let base_indent = lines[start_line - 1]
            .chars()
            .take_while(|&c| c == ' ')
            .collect::<String>();

        // Build new function definition
        let vis = if is_pub { "pub " } else { "" };
        let ret_clause = return_type
            .filter(|r| !r.is_empty())
            .map(|r| format!(" -> {}", r))
            .unwrap_or_default();

        let new_fn_body = lines[start_line - 1..end_line]
            .iter()
            .map(|l| format!("    {}", l.trim_start()))
            .collect::<Vec<_>>()
            .join("\n");

        let new_fn_def = format!(
            "\n\n{}fn {}({}){} {{\n{}\n}}",
            vis, new_fn_name, params, ret_clause, new_fn_body
        );

        // Build replacement call
        let call_replacement = format!("{}{}({});", base_indent, new_fn_name, call_args);

        // Splice modified content
        let mut new_lines = Vec::new();
        for (i, line) in lines.iter().enumerate() {
            let line_no = i + 1;
            if line_no < start_line || line_no > end_line {
                new_lines.push(line.clone());
            } else if line_no == start_line {
                new_lines.push(call_replacement.clone());
            }
        }

        let mut final_content = new_lines.join("\n");
        final_content.push_str(&new_fn_def);
        final_content.push('\n');

        fs::write(&full_path, &final_content).map_err(|e| ToolError::FileOp {
            path: file_path.to_string(),
            source: e,
        })?;

        let diff_preview = format!(
            "--- a/{}\n+++ b/{}\n@@ -{},{} +{},{} @@\n-{}\n+{}\n+{}",
            file_path,
            file_path,
            start_line,
            end_line - start_line + 1,
            start_line,
            1,
            original_slice,
            call_replacement,
            new_fn_def.trim()
        );

        Ok(RefactorResult {
            action: format!("extract_function `{}`", new_fn_name),
            files_modified: vec![file_path.to_string()],
            edits: vec![RefactorEdit {
                file_path: file_path.to_string(),
                start_line,
                end_line,
                original_text: original_slice,
                replacement_text: call_replacement,
            }],
            diff_preview,
        })
    }

    /// Performs an AST-aware token renaming across a file or workspace
    pub fn rename_symbol(
        workspace_root: &Path,
        target_symbol: &str,
        new_name: &str,
        file_scope: Option<&str>,
    ) -> Result<RefactorResult> {
        let files = if let Some(scope) = file_scope {
            vec![Self::resolve_workspace_path(workspace_root, scope)?]
        } else {
            Self::discover_source_files(workspace_root)
        };

        let mut files_modified = Vec::new();
        let mut edits = Vec::new();
        let mut diff_preview = String::new();

        for file_path in files {
            if let Ok(content) = fs::read_to_string(&file_path) {
                if !content.contains(target_symbol) {
                    continue;
                }

                let mut new_lines = Vec::new();
                let mut file_changed = false;
                let rel = file_path
                    .strip_prefix(workspace_root)
                    .unwrap_or(&file_path)
                    .display()
                    .to_string();

                for (idx, line) in content.lines().enumerate() {
                    let line_no = idx + 1;
                    if line.contains(target_symbol) {
                        let replaced = Self::replace_identifier_word(line, target_symbol, new_name);
                        if replaced != line {
                            file_changed = true;
                            edits.push(RefactorEdit {
                                file_path: rel.clone(),
                                start_line: line_no,
                                end_line: line_no,
                                original_text: line.to_string(),
                                replacement_text: replaced.clone(),
                            });
                            new_lines.push(replaced);
                            continue;
                        }
                    }
                    new_lines.push(line.to_string());
                }

                if file_changed {
                    let mut updated = new_lines.join("\n");
                    if content.ends_with('\n') {
                        updated.push('\n');
                    }
                    fs::write(&file_path, &updated).map_err(|e| ToolError::FileOp {
                        path: rel.clone(),
                        source: e,
                    })?;
                    files_modified.push(rel.clone());
                    diff_preview.push_str(&format!(
                        "--- a/{}\n+++ b/{}\n (Renamed `{}` → `{}` across occurrences)\n",
                        rel, rel, target_symbol, new_name
                    ));
                }
            }
        }

        Ok(RefactorResult {
            action: format!("rename_symbol `{}` → `{}`", target_symbol, new_name),
            files_modified,
            edits,
            diff_preview,
        })
    }

    /// Inlines a local variable declaration into subsequent usage sites
    pub fn inline_variable(
        workspace_root: &Path,
        file_path: &str,
        var_name: &str,
    ) -> Result<RefactorResult> {
        let full_path = Self::resolve_workspace_path(workspace_root, file_path)?;
        let content = fs::read_to_string(&full_path).map_err(|e| ToolError::FileOp {
            path: file_path.to_string(),
            source: e,
        })?;

        let lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
        let mut decl_line_idx: Option<usize> = None;
        let mut var_value: Option<String> = None;

        let pattern_let = format!("let {} =", var_name);
        let pattern_mut = format!("let mut {} =", var_name);

        for (idx, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with(&pattern_let) || trimmed.starts_with(&pattern_mut) {
                if let Some(eq_pos) = trimmed.find('=') {
                    let val = trimmed[eq_pos + 1..].trim_end_matches(';').trim();
                    decl_line_idx = Some(idx);
                    var_value = Some(val.to_string());
                    break;
                }
            }
        }

        let decl_idx = decl_line_idx.ok_or_else(|| {
            ContextError::TreeSitter(format!(
                "Variable declaration for `{}` not found in `{}`",
                var_name, file_path
            ))
        })?;

        let val_expr = var_value.ok_or_else(|| {
            ContextError::TreeSitter(format!("Value expression for `{}` missing", var_name))
        })?;

        let mut new_lines = Vec::new();
        let mut edits = Vec::new();

        for (idx, line) in lines.iter().enumerate() {
            if idx == decl_idx {
                edits.push(RefactorEdit {
                    file_path: file_path.to_string(),
                    start_line: idx + 1,
                    end_line: idx + 1,
                    original_text: line.clone(),
                    replacement_text: String::new(),
                });
                continue; // Remove declaration line
            }

            if idx > decl_idx && line.contains(var_name) {
                let replaced = Self::replace_identifier_word(line, var_name, &val_expr);
                if replaced != *line {
                    edits.push(RefactorEdit {
                        file_path: file_path.to_string(),
                        start_line: idx + 1,
                        end_line: idx + 1,
                        original_text: line.clone(),
                        replacement_text: replaced.clone(),
                    });
                    new_lines.push(replaced);
                    continue;
                }
            }
            new_lines.push(line.clone());
        }

        let mut final_content = new_lines.join("\n");
        if content.ends_with('\n') {
            final_content.push('\n');
        }

        fs::write(&full_path, &final_content).map_err(|e| ToolError::FileOp {
            path: file_path.to_string(),
            source: e,
        })?;

        let diff_preview = format!(
            "--- a/{}\n+++ b/{}\n (Inlined variable `{}` with value `{}`)\n",
            file_path, file_path, var_name, val_expr
        );

        Ok(RefactorResult {
            action: format!("inline_variable `{}`", var_name),
            files_modified: vec![file_path.to_string()],
            edits,
            diff_preview,
        })
    }

    fn replace_identifier_word(line: &str, target: &str, replacement: &str) -> String {
        let mut result = String::with_capacity(line.len() + 16);
        let mut chars = line.char_indices().peekable();

        while let Some((idx, _)) = chars.next() {
            if line[idx..].starts_with(target) {
                // Check left word boundary
                let left_ok = if idx == 0 {
                    true
                } else {
                    let prev = line[..idx].chars().next_back().unwrap_or(' ');
                    !prev.is_alphanumeric() && prev != '_'
                };

                let end_idx = idx + target.len();
                let right_ok = if end_idx >= line.len() {
                    true
                } else {
                    let next = line[end_idx..].chars().next().unwrap_or(' ');
                    !next.is_alphanumeric() && next != '_'
                };

                if left_ok && right_ok {
                    result.push_str(replacement);
                    // Fast-forward chars
                    for _ in 1..target.len() {
                        chars.next();
                    }
                    continue;
                }
            }
            result.push(line[idx..].chars().next().unwrap_or(' '));
        }

        result
    }

    fn resolve_workspace_path(workspace_root: &Path, file_path: &str) -> Result<PathBuf> {
        let p = if Path::new(file_path).is_absolute() {
            PathBuf::from(file_path)
        } else {
            workspace_root.join(file_path)
        };

        if !p.exists() {
            return Err(ToolError::NotFound {
                name: file_path.to_string(),
            }
            .into());
        }

        Ok(p)
    }

    fn discover_source_files(root: &Path) -> Vec<PathBuf> {
        let mut results = Vec::new();
        let src_dir = root.join("src");
        if src_dir.exists() {
            let walker = ignore::WalkBuilder::new(&src_dir).build();
            for entry in walker.flatten() {
                let p = entry.path();
                if p.is_file() {
                    if let Some(ext) = p.extension() {
                        if ext == "rs" || ext == "ts" || ext == "js" || ext == "py" {
                            results.push(p.to_path_buf());
                        }
                    }
                }
            }
        }
        results
    }
}

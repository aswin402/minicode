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
        Self::invalidate_caches();

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

                let rel = file_path
                    .strip_prefix(workspace_root)
                    .unwrap_or(&file_path)
                    .display()
                    .to_string();
                let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");

                if let Some(spans) = crate::context::pattern::find_identifier_occurrences_ast(
                    ext,
                    &content,
                    target_symbol,
                ) {
                    if spans.is_empty() {
                        continue;
                    }
                    let mut updated = content.clone();
                    for &(start_b, end_b) in spans.iter().rev() {
                        if start_b <= end_b
                            && end_b <= updated.len()
                            && updated.is_char_boundary(start_b)
                            && updated.is_char_boundary(end_b)
                        {
                            updated.replace_range(start_b..end_b, new_name);
                        }
                    }
                    if updated != content {
                        for (idx, (orig_line, new_line)) in
                            content.lines().zip(updated.lines()).enumerate()
                        {
                            if orig_line != new_line {
                                edits.push(RefactorEdit {
                                    file_path: rel.clone(),
                                    start_line: idx + 1,
                                    end_line: idx + 1,
                                    original_text: orig_line.to_string(),
                                    replacement_text: new_line.to_string(),
                                });
                            }
                        }
                        fs::write(&file_path, &updated).map_err(|e| ToolError::FileOp {
                            path: rel.clone(),
                            source: e,
                        })?;
                        files_modified.push(rel.clone());
                        diff_preview.push_str(&format!(
                            "--- a/{}\n+++ b/{}\n (Renamed `{}` → `{}` across {} AST identifier occurrence(s))\n",
                            rel,
                            rel,
                            target_symbol,
                            new_name,
                            spans.len()
                        ));
                    }
                    continue;
                }

                let mut new_lines = Vec::new();
                let mut file_changed = false;

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

        if !files_modified.is_empty() {
            Self::invalidate_caches();
        }

        Ok(RefactorResult {
            action: format!("rename_symbol `{}` → `{}`", target_symbol, new_name),
            files_modified,
            edits,
            diff_preview,
        })
    }

    /// Inlines a local variable declaration into subsequent usage sites within its lexical scope
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

        for (idx, line) in lines.iter().enumerate() {
            if let Some(val) = Self::parse_let_binding(line, var_name) {
                decl_line_idx = Some(idx);
                var_value = Some(val.to_string());
                break;
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
        let mut scope_depth = 0isize;
        let mut in_scope = true;

        for (idx, line) in lines.iter().enumerate() {
            if idx == decl_idx {
                for ch in line.chars() {
                    if ch == '{' {
                        scope_depth += 1;
                    } else if ch == '}' {
                        scope_depth -= 1;
                    }
                }
                edits.push(RefactorEdit {
                    file_path: file_path.to_string(),
                    start_line: idx + 1,
                    end_line: idx + 1,
                    original_text: line.clone(),
                    replacement_text: String::new(),
                });
                continue; // Remove declaration line
            }

            if idx > decl_idx && in_scope {
                if Self::parse_let_binding(line, var_name).is_some() {
                    in_scope = false;
                } else {
                    for ch in line.chars() {
                        if ch == '{' {
                            scope_depth += 1;
                        } else if ch == '}' {
                            scope_depth -= 1;
                            if scope_depth < 0 {
                                in_scope = false;
                            }
                        }
                    }
                }
            }

            if idx > decl_idx && in_scope && line.contains(var_name) {
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
        Self::invalidate_caches();

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

    fn parse_let_binding<'a>(line: &'a str, var_name: &str) -> Option<&'a str> {
        let trimmed = line.trim();
        let rest = trimmed
            .strip_prefix("let mut ")
            .or_else(|| trimmed.strip_prefix("let "))?;
        let (lhs, rhs) = rest.split_once('=')?;
        let ident = lhs.split(':').next()?.trim();
        if ident == var_name {
            let val = rhs.trim().trim_end_matches(';').trim();
            if !val.is_empty() {
                return Some(val);
            }
        }
        None
    }

    fn replace_identifier_word(line: &str, target: &str, replacement: &str) -> String {
        let mut result = String::with_capacity(line.len() + 16);
        let mut chars = line.char_indices().peekable();
        let target_char_len = target.chars().count();

        while let Some((idx, ch)) = chars.next() {
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
                    // Fast-forward chars by character count (not byte count)
                    for _ in 1..target_char_len {
                        chars.next();
                    }
                    continue;
                }
            }
            result.push(ch);
        }

        result
    }

    fn invalidate_caches() {
        crate::context::graph::CodeGraph::invalidate_cache();
        crate::context::index::SymbolIndex::invalidate_cache();
        crate::context::search::trigram::TrigramIndex::invalidate_cache();
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
        crate::context::walker::WorkspaceWalker::new(root)
            .extensions(&["rs", "ts", "tsx", "js", "jsx", "py"])
            .collect_absolute_files()
    }
}

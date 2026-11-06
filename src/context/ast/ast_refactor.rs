use crate::constants::{
    CSS_CLASS_TARGET_EXTENSIONS, CSS_VAR_TARGET_EXTENSIONS, REFACTOR_SOURCE_EXTENSIONS,
    TRIGRAM_MIN_QUERY_LEN,
};
use crate::context::pattern::{
    classify_symbol_occurrences_ast, extract_string_literal_ranges_ast,
    find_identifier_occurrences_ast, locate_local_variable_binding_ast, AstOccurrenceRole,
};
use crate::context::repomap::{RepoMapExtractor, SymbolDef};
use crate::context::syntax_guard::SyntaxGuard;
use crate::error::{ContextError, Result, ToolError};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

/// Declarative language syntax specification for `extract_function`.
struct FunctionSyntaxSpec {
    extensions: &'static [&'static str],
    pub_prefix: &'static str,
    fn_keyword: &'static str,
    return_sep: &'static str,
    block_open: &'static str,
    block_close: &'static str,
    call_terminator: &'static str,
}

const FUNCTION_SYNTAX_SPECS: &[FunctionSyntaxSpec] = &[
    FunctionSyntaxSpec {
        extensions: &["rs"],
        pub_prefix: "pub ",
        fn_keyword: "fn",
        return_sep: " -> ",
        block_open: " {",
        block_close: "\n}",
        call_terminator: ";",
    },
    FunctionSyntaxSpec {
        extensions: &["ts", "tsx", "js", "jsx"],
        pub_prefix: "export ",
        fn_keyword: "function",
        return_sep: ": ",
        block_open: " {",
        block_close: "\n}",
        call_terminator: ";",
    },
    FunctionSyntaxSpec {
        extensions: &["py"],
        pub_prefix: "",
        fn_keyword: "def",
        return_sep: " -> ",
        block_open: ":",
        block_close: "",
        call_terminator: "",
    },
];

/// Standard entrypoint and trait method names excluded from unused-declaration flagging.
const ENTRYPOINT_SYMBOLS: &[&str] = &[
    "main",
    "new",
    "default",
    "from",
    "into",
    "try_from",
    "try_into",
    "drop",
    "fmt",
    "clone",
    "eq",
    "ne",
    "cmp",
    "partial_cmp",
    "hash",
    "deref",
    "deref_mut",
    "index",
    "index_mut",
    "build",
    "run",
    "execute",
    "dispatch",
    "init",
    "setup",
];

/// Outcome tag inspired by `ripide` (`Preview`, `Applied`, `Refused`, `Empty`, `Result`).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum RefactorOutcomeTag {
    Preview,
    Applied,
    Refused,
    Empty,
    Result,
}

impl RefactorOutcomeTag {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Preview => "Preview",
            Self::Applied => "Applied",
            Self::Refused => "Refused",
            Self::Empty => "Empty",
            Self::Result => "Result",
        }
    }
}

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
    #[serde(default = "default_outcome_tag")]
    pub outcome_tag: String,
    pub action: String,
    pub files_modified: Vec<String>,
    pub edits: Vec<RefactorEdit>,
    pub diff_preview: String,
}

fn default_outcome_tag() -> String {
    RefactorOutcomeTag::Applied.as_str().to_string()
}

/// Internal in-memory file mutation plan verified by [`SyntaxGuard`] prior to atomic commit.
struct PlannedFileMutation {
    full_path: PathBuf,
    rel_path: String,
    original_content: String,
    updated_content: String,
    rename_to: Option<(PathBuf, String)>,
    delete_original: bool,
    edits: Vec<RefactorEdit>,
    diff_block: String,
}

pub struct AstRefactorer;

impl AstRefactorer {
    /// Validates all planned file mutations with [`SyntaxGuard::check_syntax_barrier`] before writing.
    /// If any file fails syntax validation, refuses the entire refactor (`outcome_tag = "Refused"`).
    /// When `dry_run` is `true`, returns `outcome_tag = "Preview"` without modifying disk.
    /// When `dry_run` is `false`, writes all files atomically with automatic rollback on OS write errors.
    fn commit_or_preview_plan(
        action: String,
        mutations: Vec<PlannedFileMutation>,
        dry_run: bool,
    ) -> Result<RefactorResult> {
        if mutations.is_empty() {
            return Ok(RefactorResult {
                outcome_tag: RefactorOutcomeTag::Empty.as_str().to_string(),
                action,
                files_modified: Vec::new(),
                edits: Vec::new(),
                diff_preview: "No matching files or symbols required changes.".to_string(),
            });
        }

        // Phase 1: Pre-commit Tree-sitter SyntaxGuard verification across all touched files
        for m in &mutations {
            if m.delete_original && m.updated_content.is_empty() {
                continue;
            }
            let target_check_path = m
                .rename_to
                .as_ref()
                .map(|(p, _)| p.as_path())
                .unwrap_or(m.full_path.as_path());
            if let Err(diag) = SyntaxGuard::check_syntax_barrier(
                target_check_path,
                &m.original_content,
                &m.updated_content,
            ) {
                return Ok(RefactorResult {
                    outcome_tag: RefactorOutcomeTag::Refused.as_str().to_string(),
                    action,
                    files_modified: Vec::new(),
                    edits: Vec::new(),
                    diff_preview: format!(
                        "Refused refactor: syntax verification failed on `{}`:\n{}",
                        m.rel_path, diag
                    ),
                });
            }
        }

        let mut files_modified = Vec::with_capacity(mutations.len());
        let mut all_edits = Vec::new();
        let mut diff_preview = String::new();

        for m in &mutations {
            if let Some((_, ref new_rel)) = m.rename_to {
                files_modified.push(format!("{} -> {}", m.rel_path, new_rel));
            } else {
                files_modified.push(m.rel_path.clone());
            }
            all_edits.extend(m.edits.clone());
            diff_preview.push_str(&m.diff_block);
        }

        if dry_run {
            return Ok(RefactorResult {
                outcome_tag: RefactorOutcomeTag::Preview.as_str().to_string(),
                action,
                files_modified,
                edits: all_edits,
                diff_preview,
            });
        }

        // Phase 2: Atomic commit with rollback log
        let mut written_snapshots: Vec<(PathBuf, Option<String>)> = Vec::new();
        for m in &mutations {
            let existed_before = m.full_path.exists();
            if let Some((ref dest_full, ref dest_rel)) = m.rename_to {
                let dest_existed = dest_full.exists();
                let prev_dest = if dest_existed {
                    fs::read_to_string(dest_full).ok()
                } else {
                    None
                };
                if let Some(parent) = dest_full.parent() {
                    if let Err(e) = fs::create_dir_all(parent) {
                        Self::rollback_writes(&written_snapshots);
                        return Err(ToolError::FileOp {
                            path: dest_rel.clone(),
                            source: e,
                        }
                        .into());
                    }
                }
                if let Err(e) = fs::write(dest_full, &m.updated_content) {
                    Self::rollback_writes(&written_snapshots);
                    return Err(ToolError::FileOp {
                        path: dest_rel.clone(),
                        source: e,
                    }
                    .into());
                }
                written_snapshots.push((dest_full.clone(), prev_dest));
                if m.delete_original && existed_before && m.full_path != *dest_full {
                    let _ = fs::remove_file(&m.full_path);
                    written_snapshots.push((m.full_path.clone(), Some(m.original_content.clone())));
                }
            } else {
                if let Some(parent) = m.full_path.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                if let Err(e) = fs::write(&m.full_path, &m.updated_content) {
                    Self::rollback_writes(&written_snapshots);
                    return Err(ToolError::FileOp {
                        path: m.rel_path.clone(),
                        source: e,
                    }
                    .into());
                }
                written_snapshots.push((
                    m.full_path.clone(),
                    existed_before.then(|| m.original_content.clone()),
                ));
            }
        }

        Self::invalidate_caches();

        Ok(RefactorResult {
            outcome_tag: RefactorOutcomeTag::Applied.as_str().to_string(),
            action,
            files_modified,
            edits: all_edits,
            diff_preview,
        })
    }

    fn rollback_writes(snapshots: &[(PathBuf, Option<String>)]) {
        for (path, orig_opt) in snapshots.iter().rev() {
            match orig_opt {
                Some(content) => {
                    let _ = fs::write(path, content);
                }
                None => {
                    let _ = fs::remove_file(path);
                }
            }
        }
    }

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
        Self::extract_function_with_options(
            workspace_root,
            file_path,
            start_line,
            end_line,
            new_fn_name,
            params,
            call_args,
            return_type,
            is_pub,
            false,
        )
    }

    /// Extracts a line span into a standalone helper function with `dry_run` preview and `SyntaxGuard` verification.
    #[allow(clippy::too_many_arguments)]
    pub fn extract_function_with_options(
        workspace_root: &Path,
        file_path: &str,
        start_line: usize,
        end_line: usize,
        new_fn_name: &str,
        params: &str,
        call_args: &str,
        return_type: Option<&str>,
        is_pub: bool,
        dry_run: bool,
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

        let ext = full_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("rs");
        let spec = FUNCTION_SYNTAX_SPECS
            .iter()
            .find(|s| s.extensions.contains(&ext))
            .unwrap_or(&FUNCTION_SYNTAX_SPECS[0]);

        let vis = if is_pub { spec.pub_prefix } else { "" };
        let ret_clause = return_type
            .filter(|r| !r.is_empty())
            .map(|r| format!("{}{}", spec.return_sep, r))
            .unwrap_or_default();

        let new_fn_body = lines[start_line - 1..end_line]
            .iter()
            .map(|l| format!("    {}", l.trim_start()))
            .collect::<Vec<_>>()
            .join("\n");

        let new_fn_def = format!(
            "\n\n{}{} {}({}){}{}\n{}{}",
            vis,
            spec.fn_keyword,
            new_fn_name,
            params,
            ret_clause,
            spec.block_open,
            new_fn_body,
            spec.block_close
        );

        let call_replacement = format!(
            "{}{}({}){}",
            base_indent, new_fn_name, call_args, spec.call_terminator
        );

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

        let diff_block = format!(
            "--- a/{}\n+++ b/{}\n@@ -{},{} +{},{} @@\n-{}\n+{}\n+{}\n",
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

        let mutation = PlannedFileMutation {
            full_path,
            rel_path: file_path.to_string(),
            original_content: content,
            updated_content: final_content,
            rename_to: None,
            delete_original: false,
            edits: vec![RefactorEdit {
                file_path: file_path.to_string(),
                start_line,
                end_line,
                original_text: original_slice,
                replacement_text: call_replacement,
            }],
            diff_block,
        };

        Self::commit_or_preview_plan(
            format!("extract_function `{}`", new_fn_name),
            vec![mutation],
            dry_run,
        )
    }

    /// Performs an AST-aware token renaming across a file or workspace
    pub fn rename_symbol(
        workspace_root: &Path,
        target_symbol: &str,
        new_name: &str,
        file_scope: Option<&str>,
    ) -> Result<RefactorResult> {
        Self::rename_symbol_with_options(workspace_root, target_symbol, new_name, file_scope, false)
    }

    /// Performs an AST-aware symbol rename across a file or workspace with `dry_run` preview and `SyntaxGuard` verification.
    pub fn rename_symbol_with_options(
        workspace_root: &Path,
        target_symbol: &str,
        new_name: &str,
        file_scope: Option<&str>,
        dry_run: bool,
    ) -> Result<RefactorResult> {
        let files = Self::candidate_files_for_symbol(workspace_root, target_symbol, file_scope)?;
        let mut mutations = Vec::new();

        for file_path in files {
            let Ok(content) = fs::read_to_string(&file_path) else {
                continue;
            };
            if !content.contains(target_symbol) {
                continue;
            }

            let rel = file_path
                .strip_prefix(workspace_root)
                .unwrap_or(&file_path)
                .display()
                .to_string();
            let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");

            if let Some(spans) = find_identifier_occurrences_ast(ext, &content, target_symbol) {
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
                    let mut file_edits = Vec::new();
                    let mut diff_block = format!(
                        "--- a/{}\n+++ b/{}\n (Renamed `{}` → `{}` across {} AST identifier occurrence(s))\n",
                        rel,
                        rel,
                        target_symbol,
                        new_name,
                        spans.len()
                    );
                    for (idx, (orig_line, new_line)) in
                        content.lines().zip(updated.lines()).enumerate()
                    {
                        if orig_line != new_line {
                            diff_block.push_str(&format!(
                                "@@ -{},1 +{},1 @@\n-{}\n+{}\n",
                                idx + 1,
                                idx + 1,
                                orig_line,
                                new_line
                            ));
                            file_edits.push(RefactorEdit {
                                file_path: rel.clone(),
                                start_line: idx + 1,
                                end_line: idx + 1,
                                original_text: orig_line.to_string(),
                                replacement_text: new_line.to_string(),
                            });
                        }
                    }
                    mutations.push(PlannedFileMutation {
                        full_path: file_path,
                        rel_path: rel,
                        original_content: content,
                        updated_content: updated,
                        rename_to: None,
                        delete_original: false,
                        edits: file_edits,
                        diff_block,
                    });
                }
                continue;
            }

            let mut new_lines = Vec::new();
            let mut file_edits = Vec::new();
            let mut file_changed = false;

            for (idx, line) in content.lines().enumerate() {
                let line_no = idx + 1;
                if line.contains(target_symbol) {
                    let replaced = Self::replace_identifier_word(line, target_symbol, new_name);
                    if replaced != line {
                        file_changed = true;
                        file_edits.push(RefactorEdit {
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
                let diff_block = format!(
                    "--- a/{}\n+++ b/{}\n (Renamed `{}` → `{}` across {} occurrence(s))\n",
                    rel,
                    rel,
                    target_symbol,
                    new_name,
                    file_edits.len()
                );
                mutations.push(PlannedFileMutation {
                    full_path: file_path,
                    rel_path: rel,
                    original_content: content,
                    updated_content: updated,
                    rename_to: None,
                    delete_original: false,
                    edits: file_edits,
                    diff_block,
                });
            }
        }

        Self::commit_or_preview_plan(
            format!("rename_symbol `{}` → `{}`", target_symbol, new_name),
            mutations,
            dry_run,
        )
    }

    /// Inlines a local variable declaration into subsequent usage sites within its lexical scope
    pub fn inline_variable(
        workspace_root: &Path,
        file_path: &str,
        var_name: &str,
    ) -> Result<RefactorResult> {
        Self::inline_variable_with_options(workspace_root, file_path, var_name, false)
    }

    /// Inlines a local variable declaration with `dry_run` preview and `SyntaxGuard` verification.
    /// Uses Tree-sitter AST binding resolution (`locate_local_variable_binding_ast`) to identify the
    /// declaration statement, initializer expression node, enclosing lexical block, and non-shadowed references.
    pub fn inline_variable_with_options(
        workspace_root: &Path,
        file_path: &str,
        var_name: &str,
        dry_run: bool,
    ) -> Result<RefactorResult> {
        let full_path = Self::resolve_workspace_path(workspace_root, file_path)?;
        let content = fs::read_to_string(&full_path).map_err(|e| ToolError::FileOp {
            path: file_path.to_string(),
            source: e,
        })?;
        let ext = full_path.extension().and_then(|e| e.to_str()).unwrap_or("");

        let binding =
            locate_local_variable_binding_ast(ext, &content, var_name).ok_or_else(|| {
                ContextError::TreeSitter(format!(
                    "Variable declaration for `{}` not found in `{}`",
                    var_name, file_path
                ))
            })?;

        // Phase 1: Splice non-shadowed AST usage byte spans in reverse byte order
        let mut spliced = content.clone();
        for &(start_b, end_b) in binding.usage_byte_spans.iter().rev() {
            if start_b <= end_b
                && end_b <= spliced.len()
                && spliced.is_char_boundary(start_b)
                && spliced.is_char_boundary(end_b)
            {
                spliced.replace_range(start_b..end_b, &binding.value_expr);
            }
        }

        // Phase 2: Remove the declaration statement line span (`decl_start_line..=decl_end_line`)
        let orig_lines: Vec<&str> = content.lines().collect();
        let spliced_lines: Vec<&str> = spliced.lines().collect();
        let mut new_lines = Vec::with_capacity(spliced_lines.len());
        let mut edits = Vec::new();

        for (idx, &spliced_line) in spliced_lines.iter().enumerate() {
            let line_no = idx + 1;
            let orig_line = orig_lines.get(idx).copied().unwrap_or(spliced_line);
            if line_no >= binding.decl_start_line && line_no <= binding.decl_end_line {
                edits.push(RefactorEdit {
                    file_path: file_path.to_string(),
                    start_line: line_no,
                    end_line: line_no,
                    original_text: orig_line.to_string(),
                    replacement_text: String::new(),
                });
                continue;
            }
            if spliced_line != orig_line {
                edits.push(RefactorEdit {
                    file_path: file_path.to_string(),
                    start_line: line_no,
                    end_line: line_no,
                    original_text: orig_line.to_string(),
                    replacement_text: spliced_line.to_string(),
                });
            }
            new_lines.push(spliced_line.to_string());
        }

        let mut final_content = new_lines.join("\n");
        if content.ends_with('\n') && !final_content.is_empty() {
            final_content.push('\n');
        }

        let diff_block = format!(
            "--- a/{}\n+++ b/{}\n (Inlined variable `{}` with value `{}`)\n",
            file_path, file_path, var_name, binding.value_expr
        );

        let mutation = PlannedFileMutation {
            full_path,
            rel_path: file_path.to_string(),
            original_content: content,
            updated_content: final_content,
            rename_to: None,
            delete_original: false,
            edits,
            diff_block,
        };

        Self::commit_or_preview_plan(
            format!("inline_variable `{}`", var_name),
            vec![mutation],
            dry_run,
        )
    }

    /// Scans the workspace (or `file_scope`) and classifies every AST occurrence of `target_symbol`
    /// by syntactic role (`declaration`, `import-specifier`, `call-reference`, `type-reference`,
    /// `property`, `identifier-reference`, `string-literal`), with optional Mermaid dependency graph (`ripide scan`).
    pub fn scan_symbol(
        workspace_root: &Path,
        target_symbol: &str,
        kind_filter: Option<&str>,
        file_scope: Option<&str>,
        include_mermaid: bool,
    ) -> Result<RefactorResult> {
        let files = Self::candidate_files_for_symbol(workspace_root, target_symbol, file_scope)?;
        let filter_str = kind_filter.unwrap_or("all");

        let mut grouped: BTreeMap<String, Vec<(usize, usize, AstOccurrenceRole, String)>> =
            BTreeMap::new();
        let mut declaration_files: BTreeSet<String> = BTreeSet::new();
        let mut consumer_files: BTreeSet<String> = BTreeSet::new();
        let mut total_hits = 0usize;

        for file_path in files {
            let Ok(content) = fs::read_to_string(&file_path) else {
                continue;
            };
            if !content.contains(target_symbol) {
                continue;
            }
            let rel = file_path
                .strip_prefix(workspace_root)
                .unwrap_or(&file_path)
                .display()
                .to_string();
            let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");

            if let Some(occurrences) = classify_symbol_occurrences_ast(ext, &content, target_symbol)
            {
                for occ in occurrences {
                    if occ.role == AstOccurrenceRole::Declaration {
                        declaration_files.insert(rel.clone());
                    } else if occ.role != AstOccurrenceRole::StringLiteral {
                        consumer_files.insert(rel.clone());
                    }
                    if occ.role.matches_filter(filter_str) {
                        total_hits += 1;
                        grouped.entry(rel.clone()).or_default().push((
                            occ.line,
                            occ.column,
                            occ.role,
                            occ.line_snippet,
                        ));
                    }
                }
            }
        }

        if grouped.is_empty() {
            return Ok(RefactorResult {
                outcome_tag: RefactorOutcomeTag::Empty.as_str().to_string(),
                action: format!("scan_symbol `{}`", target_symbol),
                files_modified: Vec::new(),
                edits: Vec::new(),
                diff_preview: format!(
                    "No AST occurrences of `{}` matched filter `{}`.",
                    target_symbol, filter_str
                ),
            });
        }

        let mut report_lines = vec![format!(
            "Found {} AST occurrence(s) of `{}` across {} file(s) (filter: `{}`):",
            total_hits,
            target_symbol,
            grouped.len(),
            filter_str
        )];

        for (file, hits) in &grouped {
            report_lines.push(format!("\n### `{}` ({} hit(s))", file, hits.len()));
            for (line, col, role, snippet) in hits {
                report_lines.push(format!(
                    "- `L{}:{}` [{}] `{}`",
                    line,
                    col,
                    role.as_str(),
                    snippet
                ));
            }
        }

        if include_mermaid && !declaration_files.is_empty() && !consumer_files.is_empty() {
            report_lines.push("\n```mermaid\nflowchart LR".to_string());
            for (d_idx, decl_file) in declaration_files.iter().enumerate() {
                for (c_idx, cons_file) in consumer_files.iter().enumerate() {
                    if decl_file != cons_file {
                        report_lines.push(format!(
                            "    C{}[\"{}\"] -->|\"{}\"| D{}[\"{}\"]",
                            c_idx, cons_file, target_symbol, d_idx, decl_file
                        ));
                    }
                }
            }
            report_lines.push("```".to_string());
        }

        Ok(RefactorResult {
            outcome_tag: RefactorOutcomeTag::Result.as_str().to_string(),
            action: format!("scan_symbol `{}`", target_symbol),
            files_modified: grouped.keys().cloned().collect(),
            edits: Vec::new(),
            diff_preview: report_lines.join("\n"),
        })
    }

    /// Deletes a top-level symbol declaration from `file_path` ONLY after verifying that zero
    /// external code references to `target_symbol` remain in the workspace (`ripide delete`).
    pub fn delete_symbol(
        workspace_root: &Path,
        file_path: &str,
        target_symbol: &str,
        dry_run: bool,
    ) -> Result<RefactorResult> {
        let full_path = Self::resolve_workspace_path(workspace_root, file_path)?;
        let content = fs::read_to_string(&full_path).map_err(|e| ToolError::FileOp {
            path: file_path.to_string(),
            source: e,
        })?;
        let rel_target = full_path
            .strip_prefix(workspace_root)
            .unwrap_or(&full_path)
            .display()
            .to_string();

        let (decl_start, decl_end) =
            Self::locate_declaration_span_with_docs(&full_path, &content, target_symbol)?;

        // Scan workspace for any remaining external code references (`role != StringLiteral`)
        let candidate_files =
            Self::candidate_files_for_symbol(workspace_root, target_symbol, None)?;
        let mut blocking_refs = Vec::new();

        for cand_path in candidate_files {
            let Ok(cand_content) = fs::read_to_string(&cand_path) else {
                continue;
            };
            if !cand_content.contains(target_symbol) {
                continue;
            }
            let cand_rel = cand_path
                .strip_prefix(workspace_root)
                .unwrap_or(&cand_path)
                .display()
                .to_string();
            let ext = cand_path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if let Some(occurrences) =
                classify_symbol_occurrences_ast(ext, &cand_content, target_symbol)
            {
                for occ in occurrences {
                    if occ.role == AstOccurrenceRole::StringLiteral {
                        continue;
                    }
                    let inside_target_decl =
                        cand_rel == rel_target && occ.line >= decl_start && occ.line <= decl_end;
                    if !inside_target_decl {
                        blocking_refs.push(format!(
                            "- `{}:{}:{}` [{}] `{}`",
                            cand_rel,
                            occ.line,
                            occ.column,
                            occ.role.as_str(),
                            occ.line_snippet
                        ));
                    }
                }
            }
        }

        if !blocking_refs.is_empty() {
            return Ok(RefactorResult {
                outcome_tag: RefactorOutcomeTag::Refused.as_str().to_string(),
                action: format!("delete_symbol `{}` from `{}`", target_symbol, rel_target),
                files_modified: Vec::new(),
                edits: Vec::new(),
                diff_preview: format!(
                    "Refused deletion of `{}` in `{}`: {} active reference(s) remain:\n{}",
                    target_symbol,
                    rel_target,
                    blocking_refs.len(),
                    blocking_refs.join("\n")
                ),
            });
        }

        let lines: Vec<&str> = content.lines().collect();
        let removed_slice = lines[decl_start - 1..decl_end].join("\n");
        let mut remaining_lines = Vec::with_capacity(lines.len());
        for (idx, &line) in lines.iter().enumerate() {
            let line_no = idx + 1;
            if line_no < decl_start || line_no > decl_end {
                remaining_lines.push(line);
            }
        }
        let mut updated_content = remaining_lines.join("\n");
        if content.ends_with('\n') && !updated_content.is_empty() {
            updated_content.push('\n');
        }

        let diff_block = format!(
            "--- a/{}\n+++ b/{}\n@@ -{},{} +{},0 @@\n{}\n",
            rel_target,
            rel_target,
            decl_start,
            decl_end - decl_start + 1,
            decl_start,
            removed_slice
                .lines()
                .map(|l| format!("-{}", l))
                .collect::<Vec<_>>()
                .join("\n")
        );

        let mutation = PlannedFileMutation {
            full_path,
            rel_path: rel_target.clone(),
            original_content: content,
            updated_content,
            rename_to: None,
            delete_original: false,
            edits: vec![RefactorEdit {
                file_path: rel_target,
                start_line: decl_start,
                end_line: decl_end,
                original_text: removed_slice,
                replacement_text: String::new(),
            }],
            diff_block,
        };

        Self::commit_or_preview_plan(
            format!("delete_symbol `{}`", target_symbol),
            vec![mutation],
            dry_run,
        )
    }

    /// Finds unreferenced top-level declarations across the workspace or `file_scope` (`ripide unused`).
    pub fn find_unused(
        workspace_root: &Path,
        file_scope: Option<&str>,
        exports_filter: Option<&str>,
    ) -> Result<RefactorResult> {
        let scope_files = if let Some(scope) = file_scope {
            vec![Self::resolve_workspace_path(workspace_root, scope)?]
        } else {
            Self::discover_source_files(workspace_root)
        };
        let all_workspace_files = Self::discover_source_files(workspace_root);
        let mut workspace_contents: Vec<(String, String, String)> =
            Vec::with_capacity(all_workspace_files.len());
        for wp in all_workspace_files {
            if let Ok(c) = fs::read_to_string(&wp) {
                let rel = wp
                    .strip_prefix(workspace_root)
                    .unwrap_or(&wp)
                    .display()
                    .to_string();
                let ext = wp
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_string();
                workspace_contents.push((rel, ext, c));
            }
        }

        let mut extractor = RepoMapExtractor::new();
        let mode = exports_filter.unwrap_or("all").trim().to_ascii_lowercase();
        let mut unused_entries = Vec::new();
        let mut unused_files = BTreeSet::new();

        for file_path in scope_files {
            let Ok(content) = fs::read_to_string(&file_path) else {
                continue;
            };
            let rel = file_path
                .strip_prefix(workspace_root)
                .unwrap_or(&file_path)
                .display()
                .to_string();
            let is_test_file = Path::new(&rel).components().any(|comp| {
                comp.as_os_str()
                    .to_str()
                    .is_some_and(|s| s.eq_ignore_ascii_case("tests") || s.ends_with("_test.rs"))
            });
            if is_test_file {
                continue;
            }
            let Ok(symbols) = extractor.extract_file_symbols(&file_path) else {
                continue;
            };
            let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");

            for sym in symbols {
                if sym.kind == "import"
                    || sym.kind == "impl"
                    || sym.kind == "module"
                    || ENTRYPOINT_SYMBOLS.contains(&sym.name.as_str())
                    || sym.name.starts_with("test_")
                {
                    continue;
                }
                let is_exported = Self::is_symbol_exported(ext, &content, &sym);
                if mode == "exported" && !is_exported {
                    continue;
                }
                if mode == "local" && is_exported {
                    continue;
                }

                // Check if any reference exists outside `(rel, sym.line_number..=sym.end_line)`
                let mut has_external_ref = false;
                for (w_rel, w_ext, w_content) in &workspace_contents {
                    if !w_content.contains(&sym.name) {
                        continue;
                    }
                    if let Some(occs) = classify_symbol_occurrences_ast(w_ext, w_content, &sym.name)
                    {
                        if occs.into_iter().any(|occ| {
                            occ.role != AstOccurrenceRole::StringLiteral
                                && !(w_rel == &rel
                                    && occ.line >= sym.line_number
                                    && occ.line <= sym.end_line)
                        }) {
                            has_external_ref = true;
                            break;
                        }
                    }
                }

                if !has_external_ref {
                    unused_files.insert(rel.clone());
                    unused_entries.push(format!(
                        "- `{}:{}` [{} {}] `{}` ({})",
                        rel,
                        sym.line_number,
                        if is_exported { "exported" } else { "local" },
                        sym.kind,
                        sym.name,
                        sym.signature
                    ));
                }
            }
        }

        if unused_entries.is_empty() {
            return Ok(RefactorResult {
                outcome_tag: RefactorOutcomeTag::Empty.as_str().to_string(),
                action: "find_unused".to_string(),
                files_modified: Vec::new(),
                edits: Vec::new(),
                diff_preview: "No unreferenced top-level declarations found.".to_string(),
            });
        }

        Ok(RefactorResult {
            outcome_tag: RefactorOutcomeTag::Result.as_str().to_string(),
            action: "find_unused".to_string(),
            files_modified: unused_files.into_iter().collect(),
            edits: Vec::new(),
            diff_preview: format!(
                "Found {} unreferenced top-level declaration(s):\n{}",
                unused_entries.len(),
                unused_entries.join("\n")
            ),
        })
    }

    /// Moves a top-level symbol declaration from `from_file` to `to_file`, refusing if the symbol
    /// depends on unexported private helpers in `from_file`, and rewrites consumer import paths (`ripide move`).
    pub fn move_symbol(
        workspace_root: &Path,
        target_symbol: &str,
        from_file: &str,
        to_file: &str,
        dry_run: bool,
    ) -> Result<RefactorResult> {
        let from_full = Self::resolve_workspace_path(workspace_root, from_file)?;
        let to_full = if Path::new(to_file).is_absolute() {
            PathBuf::from(to_file)
        } else {
            workspace_root.join(to_file)
        };
        let from_rel = from_full
            .strip_prefix(workspace_root)
            .unwrap_or(&from_full)
            .display()
            .to_string();
        let to_rel = to_full
            .strip_prefix(workspace_root)
            .unwrap_or(&to_full)
            .display()
            .to_string();

        let from_content = fs::read_to_string(&from_full).map_err(|e| ToolError::FileOp {
            path: from_rel.clone(),
            source: e,
        })?;
        let to_content = if to_full.exists() {
            fs::read_to_string(&to_full).map_err(|e| ToolError::FileOp {
                path: to_rel.clone(),
                source: e,
            })?
        } else {
            String::new()
        };

        let (decl_start, decl_end) =
            Self::locate_declaration_span_with_docs(&from_full, &from_content, target_symbol)?;
        let from_lines: Vec<&str> = from_content.lines().collect();
        let moved_block = from_lines[decl_start - 1..decl_end].join("\n");

        // `ripide move` invariant: refuse move if `moved_block` references unexported local symbols in `from_file`
        let mut extractor = RepoMapExtractor::new();
        let from_ext = from_full.extension().and_then(|e| e.to_str()).unwrap_or("");
        if let Ok(local_syms) = extractor.extract_file_symbols(&from_full) {
            let mut private_deps = Vec::new();
            for sym in local_syms {
                if sym.name == target_symbol || sym.kind == "import" {
                    continue;
                }
                if !Self::is_symbol_exported(from_ext, &from_content, &sym) {
                    if let Some(spans) =
                        find_identifier_occurrences_ast(from_ext, &moved_block, &sym.name)
                    {
                        if !spans.is_empty() {
                            private_deps.push(format!("`{}` (L{})", sym.name, sym.line_number));
                        }
                    }
                }
            }
            if !private_deps.is_empty() {
                return Ok(RefactorResult {
                    outcome_tag: RefactorOutcomeTag::Refused.as_str().to_string(),
                    action: format!(
                        "move_symbol `{}` from `{}` to `{}`",
                        target_symbol, from_rel, to_rel
                    ),
                    files_modified: Vec::new(),
                    edits: Vec::new(),
                    diff_preview: format!(
                        "Refused move of `{}` from `{}` to `{}`: depends on unexported local helper(s) {}",
                        target_symbol,
                        from_rel,
                        to_rel,
                        private_deps.join(", ")
                    ),
                });
            }
        }

        let mut remaining_from: Vec<&str> = Vec::with_capacity(from_lines.len());
        for (idx, &l) in from_lines.iter().enumerate() {
            let line_no = idx + 1;
            if line_no < decl_start || line_no > decl_end {
                remaining_from.push(l);
            }
        }
        let mut updated_from = remaining_from.join("\n");
        if from_content.ends_with('\n') && !updated_from.is_empty() {
            updated_from.push('\n');
        }

        let mut updated_to = to_content.clone();
        if !updated_to.is_empty() && !updated_to.ends_with('\n') {
            updated_to.push('\n');
        }
        if !updated_to.is_empty() {
            updated_to.push('\n');
        }
        updated_to.push_str(&moved_block);
        updated_to.push('\n');

        let to_start_line = to_content.lines().count() + 1;
        let mut mutations = vec![
            PlannedFileMutation {
                full_path: from_full.clone(),
                rel_path: from_rel.clone(),
                original_content: from_content,
                updated_content: updated_from,
                rename_to: None,
                delete_original: false,
                edits: vec![RefactorEdit {
                    file_path: from_rel.clone(),
                    start_line: decl_start,
                    end_line: decl_end,
                    original_text: moved_block.clone(),
                    replacement_text: String::new(),
                }],
                diff_block: format!(
                    "--- a/{}\n+++ b/{}\n@@ -{},{} +{},0 @@\n{}\n",
                    from_rel,
                    from_rel,
                    decl_start,
                    decl_end - decl_start + 1,
                    decl_start,
                    moved_block
                        .lines()
                        .map(|l| format!("-{}", l))
                        .collect::<Vec<_>>()
                        .join("\n")
                ),
            },
            PlannedFileMutation {
                full_path: to_full.clone(),
                rel_path: to_rel.clone(),
                original_content: to_content,
                updated_content: updated_to,
                rename_to: None,
                delete_original: false,
                edits: vec![RefactorEdit {
                    file_path: to_rel.clone(),
                    start_line: to_start_line,
                    end_line: to_start_line + moved_block.lines().count().saturating_sub(1),
                    original_text: String::new(),
                    replacement_text: moved_block.clone(),
                }],
                diff_block: format!(
                    "--- a/{}\n+++ b/{}\n@@ -{},0 +{},{} @@\n{}\n",
                    to_rel,
                    to_rel,
                    to_start_line,
                    to_start_line,
                    moved_block.lines().count(),
                    moved_block
                        .lines()
                        .map(|l| format!("+{}", l))
                        .collect::<Vec<_>>()
                        .join("\n")
                ),
            },
        ];

        // Rewrite consumer import paths that import `target_symbol` from `from_file`
        let consumer_mutations = Self::plan_import_rewrites_for_files(
            workspace_root,
            &from_rel,
            &to_rel,
            Some(target_symbol),
            &[from_full, to_full],
        )?;
        mutations.extend(consumer_mutations);

        Self::commit_or_preview_plan(
            format!(
                "move_symbol `{}`: `{}` → `{}`",
                target_symbol, from_rel, to_rel
            ),
            mutations,
            dry_run,
        )
    }

    /// Moves/renames a file from `from_file` to `to_file` and updates all module/relative import statements
    /// across consumer files atomically (`ripide rename-file`).
    pub fn rename_file(
        workspace_root: &Path,
        from_file: &str,
        to_file: &str,
        dry_run: bool,
    ) -> Result<RefactorResult> {
        let from_full = Self::resolve_workspace_path(workspace_root, from_file)?;
        let to_full = if Path::new(to_file).is_absolute() {
            PathBuf::from(to_file)
        } else {
            workspace_root.join(to_file)
        };
        let from_rel = from_full
            .strip_prefix(workspace_root)
            .unwrap_or(&from_full)
            .display()
            .to_string();
        let to_rel = to_full
            .strip_prefix(workspace_root)
            .unwrap_or(&to_full)
            .display()
            .to_string();

        let from_content = fs::read_to_string(&from_full).map_err(|e| ToolError::FileOp {
            path: from_rel.clone(),
            source: e,
        })?;

        let mut mutations = vec![PlannedFileMutation {
            full_path: from_full.clone(),
            rel_path: from_rel.clone(),
            original_content: from_content.clone(),
            updated_content: from_content,
            rename_to: Some((to_full.clone(), to_rel.clone())),
            delete_original: true,
            edits: Vec::new(),
            diff_block: format!(
                "--- a/{}\n+++ b/{}\n (Renamed file `{}` → `{}`)\n",
                from_rel, to_rel, from_rel, to_rel
            ),
        }];

        let consumer_mutations = Self::plan_import_rewrites_for_files(
            workspace_root,
            &from_rel,
            &to_rel,
            None,
            &[from_full, to_full],
        )?;
        mutations.extend(consumer_mutations);

        Self::commit_or_preview_plan(
            format!("rename_file `{}` → `{}`", from_rel, to_rel),
            mutations,
            dry_run,
        )
    }

    /// Renames Tailwind / CSS utility class tokens across frontend & template files while preserving
    /// responsive/state variant prefixes (`hover:dark:md:`) and `!` important markers (`ripide css-class-rename`).
    pub fn css_class_rename(
        workspace_root: &Path,
        rename_map: &HashMap<String, String>,
        file_scope: Option<&str>,
        dry_run: bool,
    ) -> Result<RefactorResult> {
        let files = if let Some(scope) = file_scope {
            vec![Self::resolve_workspace_path(workspace_root, scope)?]
        } else {
            crate::context::walker::WorkspaceWalker::new(workspace_root)
                .extensions(CSS_CLASS_TARGET_EXTENSIONS)
                .collect_absolute_files()
        };

        let mut mutations = Vec::new();
        for file_path in files {
            let Ok(content) = fs::read_to_string(&file_path) else {
                continue;
            };
            if !rename_map.keys().any(|k| content.contains(k.as_str())) {
                continue;
            }
            let rel = file_path
                .strip_prefix(workspace_root)
                .unwrap_or(&file_path)
                .display()
                .to_string();
            let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");

            // Prefer Tree-sitter AST string/template literal byte spans when supported so code comments
            // are strictly ignored and multi-line strings work natively.
            let updated =
                if let Some(string_ranges) = extract_string_literal_ranges_ast(ext, &content) {
                    let mut buf = content.clone();
                    for &(start_b, end_b) in string_ranges.iter().rev() {
                        if start_b + 2 <= end_b
                            && end_b <= buf.len()
                            && buf.is_char_boundary(start_b + 1)
                            && buf.is_char_boundary(end_b - 1)
                        {
                            let inner = &buf[start_b + 1..end_b - 1];
                            let rewritten =
                                Self::rewrite_whitespace_tokens_in_segment(inner, rename_map);
                            if rewritten != inner {
                                buf.replace_range(start_b + 1..end_b - 1, &rewritten);
                            }
                        }
                    }
                    buf
                } else {
                    let mut new_lines = Vec::new();
                    for line in content.lines() {
                        new_lines.push(Self::rewrite_css_classes_in_line(line, rename_map));
                    }
                    let mut buf = new_lines.join("\n");
                    if content.ends_with('\n') {
                        buf.push('\n');
                    }
                    buf
                };

            if updated != content {
                let mut file_edits = Vec::new();
                let mut diff_block = format!("--- a/{}\n+++ b/{}\n", rel, rel);
                for (idx, (orig_line, new_line)) in content.lines().zip(updated.lines()).enumerate()
                {
                    if orig_line != new_line {
                        diff_block.push_str(&format!(
                            "@@ -{},1 +{},1 @@\n-{}\n+{}\n",
                            idx + 1,
                            idx + 1,
                            orig_line,
                            new_line
                        ));
                        file_edits.push(RefactorEdit {
                            file_path: rel.clone(),
                            start_line: idx + 1,
                            end_line: idx + 1,
                            original_text: orig_line.to_string(),
                            replacement_text: new_line.to_string(),
                        });
                    }
                }
                mutations.push(PlannedFileMutation {
                    full_path: file_path,
                    rel_path: rel,
                    original_content: content,
                    updated_content: updated,
                    rename_to: None,
                    delete_original: false,
                    edits: file_edits,
                    diff_block,
                });
            }
        }

        Self::commit_or_preview_plan(
            format!("css_class_rename ({} token mapping(s))", rename_map.len()),
            mutations,
            dry_run,
        )
    }

    /// Renames CSS custom properties (`--old-var` → `--new-var`) across declarations and `var(...)`
    /// usages (`ripide css-custom-property-rename`).
    pub fn css_var_rename(
        workspace_root: &Path,
        old_var: &str,
        new_var: &str,
        file_scope: Option<&str>,
        dry_run: bool,
    ) -> Result<RefactorResult> {
        let norm_old = if old_var.starts_with("--") {
            old_var.to_string()
        } else {
            format!("--{}", old_var)
        };
        let norm_new = if new_var.starts_with("--") {
            new_var.to_string()
        } else {
            format!("--{}", new_var)
        };

        let files = if let Some(scope) = file_scope {
            vec![Self::resolve_workspace_path(workspace_root, scope)?]
        } else {
            crate::context::walker::WorkspaceWalker::new(workspace_root)
                .extensions(CSS_VAR_TARGET_EXTENSIONS)
                .collect_absolute_files()
        };

        let mut mutations = Vec::new();
        for file_path in files {
            let Ok(content) = fs::read_to_string(&file_path) else {
                continue;
            };
            if !content.contains(&norm_old) {
                continue;
            }
            let rel = file_path
                .strip_prefix(workspace_root)
                .unwrap_or(&file_path)
                .display()
                .to_string();

            let mut new_lines = Vec::new();
            let mut file_edits = Vec::new();
            let mut diff_block = format!("--- a/{}\n+++ b/{}\n", rel, rel);

            for (idx, line) in content.lines().enumerate() {
                let replaced =
                    Self::replace_css_custom_property_in_line(line, &norm_old, &norm_new);
                if replaced != line {
                    diff_block.push_str(&format!(
                        "@@ -{},1 +{},1 @@\n-{}\n+{}\n",
                        idx + 1,
                        idx + 1,
                        line,
                        replaced
                    ));
                    file_edits.push(RefactorEdit {
                        file_path: rel.clone(),
                        start_line: idx + 1,
                        end_line: idx + 1,
                        original_text: line.to_string(),
                        replacement_text: replaced.clone(),
                    });
                }
                new_lines.push(replaced);
            }

            if !file_edits.is_empty() {
                let mut updated = new_lines.join("\n");
                if content.ends_with('\n') {
                    updated.push('\n');
                }
                mutations.push(PlannedFileMutation {
                    full_path: file_path,
                    rel_path: rel,
                    original_content: content,
                    updated_content: updated,
                    rename_to: None,
                    delete_original: false,
                    edits: file_edits,
                    diff_block,
                });
            }
        }

        Self::commit_or_preview_plan(
            format!("css_var_rename `{}` → `{}`", norm_old, norm_new),
            mutations,
            dry_run,
        )
    }

    /// Splits a Tailwind/CSS utility token into `(variant_and_important_prefix, core_utility_class)`
    /// by tracking `[...]` arbitrary-value bracket depth so colons inside brackets are ignored.
    fn split_tailwind_utility_token(token: &str) -> (&str, &str) {
        let mut bracket_depth = 0usize;
        let mut last_colon_end = 0usize;
        for (idx, ch) in token.char_indices() {
            match ch {
                '[' => bracket_depth += 1,
                ']' => bracket_depth = bracket_depth.saturating_sub(1),
                ':' if bracket_depth == 0 => {
                    last_colon_end = idx + 1;
                }
                _ => {}
            }
        }
        let rest = &token[last_colon_end..];
        let prefix_end = if rest.starts_with('!') {
            last_colon_end + 1
        } else {
            last_colon_end
        };
        (&token[..prefix_end], &token[prefix_end..])
    }

    fn rewrite_css_classes_in_line(line: &str, rename_map: &HashMap<String, String>) -> String {
        let trimmed = line.trim_start();
        if trimmed.starts_with("@apply ") {
            return Self::rewrite_whitespace_tokens_in_segment(line, rename_map);
        }

        // Rewrite tokens inside quoted string literals (`"..."`, `'...'`, `` `...` ``) and `.foo` CSS selectors
        let mut out = String::with_capacity(line.len());
        let chars: Vec<(usize, char)> = line.char_indices().collect();
        let mut i = 0;

        while i < chars.len() {
            let (byte_idx, ch) = chars[i];
            if ch == '"' || ch == '\'' || ch == '`' {
                let quote = ch;
                out.push(quote);
                i += 1;
                let start_b = if i < chars.len() {
                    chars[i].0
                } else {
                    line.len()
                };
                while i < chars.len() && chars[i].1 != quote {
                    if chars[i].1 == '\\' && i + 1 < chars.len() {
                        i += 2;
                    } else {
                        i += 1;
                    }
                }
                let end_b = if i < chars.len() {
                    chars[i].0
                } else {
                    line.len()
                };
                let inner = &line[start_b..end_b];
                out.push_str(&Self::rewrite_whitespace_tokens_in_segment(
                    inner, rename_map,
                ));
                if i < chars.len() {
                    out.push(chars[i].1);
                    i += 1;
                }
                continue;
            }
            if ch == '.' {
                let rest = &line[byte_idx + 1..];
                let tok_len = rest
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
                    .map(char::len_utf8)
                    .sum::<usize>();
                if tok_len > 0 {
                    let selector_tok = &rest[..tok_len];
                    if let Some(mapped) = rename_map.get(selector_tok) {
                        out.push('.');
                        out.push_str(mapped);
                        let end_target = byte_idx + 1 + tok_len;
                        while i < chars.len() && chars[i].0 < end_target {
                            i += 1;
                        }
                        continue;
                    }
                }
            }
            out.push(ch);
            i += 1;
        }
        out
    }

    fn rewrite_whitespace_tokens_in_segment(
        segment: &str,
        rename_map: &HashMap<String, String>,
    ) -> String {
        let mut out = String::with_capacity(segment.len());
        let mut idx = 0;
        let bytes = segment.as_bytes();
        while idx < bytes.len() {
            if bytes[idx].is_ascii_whitespace() || bytes[idx] == b';' {
                out.push(bytes[idx] as char);
                idx += 1;
                continue;
            }
            let start = idx;
            while idx < bytes.len() && !bytes[idx].is_ascii_whitespace() && bytes[idx] != b';' {
                idx += 1;
            }
            let tok = &segment[start..idx];
            let (prefix, core) = Self::split_tailwind_utility_token(tok);
            if let Some(new_core) = rename_map.get(core) {
                out.push_str(prefix);
                out.push_str(new_core);
            } else {
                out.push_str(tok);
            }
        }
        out
    }

    fn replace_css_custom_property_in_line(line: &str, old_var: &str, new_var: &str) -> String {
        let mut out = String::with_capacity(line.len());
        let mut i = 0;
        while i < line.len() {
            if line[i..].starts_with(old_var) {
                let end = i + old_var.len();
                let right_boundary = line[end..]
                    .chars()
                    .next()
                    .is_none_or(|c| !c.is_ascii_alphanumeric() && c != '-' && c != '_');
                if right_boundary {
                    out.push_str(new_var);
                    i = end;
                    continue;
                }
            }
            if let Some(ch) = line[i..].chars().next() {
                out.push(ch);
                i += ch.len_utf8();
            } else {
                break;
            }
        }
        out
    }

    fn locate_declaration_span_with_docs(
        full_path: &Path,
        content: &str,
        target_symbol: &str,
    ) -> Result<(usize, usize)> {
        let mut extractor = RepoMapExtractor::new();
        let symbols = extractor.extract_file_symbols(full_path)?;
        let matched = symbols
            .into_iter()
            .find(|s| s.name == target_symbol && s.kind != "import")
            .ok_or_else(|| {
                ContextError::TreeSitter(format!(
                    "Top-level symbol declaration `{}` not found in `{}`",
                    target_symbol,
                    full_path.display()
                ))
            })?;

        let mut start_line = matched.line_number.max(1);
        let ext = full_path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if let Some(lang) = SyntaxGuard::language_for_extension(ext) {
            let mut parser = tree_sitter::Parser::new();
            if parser.set_language(&lang).is_ok() {
                if let Some(tree) = parser.parse(content, None) {
                    let root = tree.root_node();
                    let mut cursor = root.walk();
                    let children: Vec<tree_sitter::Node<'_>> = root.children(&mut cursor).collect();
                    for idx in (0..children.len()).rev() {
                        let node = children[idx];
                        let n_end = node.end_position().row + 1;
                        let n_start = node.start_position().row + 1;
                        let kind = node.kind();
                        let is_meta_node = kind.contains("comment")
                            || kind.contains("attribute")
                            || kind == "decorator";
                        if is_meta_node && n_end + 1 >= start_line && n_start < start_line {
                            start_line = n_start;
                        }
                    }
                }
            }
        }

        let total_lines = content.lines().count().max(1);
        let end_line = matched.end_line.clamp(start_line, total_lines);
        Ok((start_line, end_line))
    }

    fn is_symbol_exported(ext: &str, content: &str, sym: &SymbolDef) -> bool {
        if ext == "py" {
            return !sym.name.starts_with('_');
        }
        if let Some(lang) = SyntaxGuard::language_for_extension(ext) {
            let mut parser = tree_sitter::Parser::new();
            if parser.set_language(&lang).is_ok() {
                if let Some(tree) = parser.parse(content, None) {
                    let target_row = sym.line_number.saturating_sub(1);
                    let root = tree.root_node();
                    let mut cursor = root.walk();
                    for top_node in root.children(&mut cursor) {
                        if top_node.start_position().row <= target_row
                            && target_row <= top_node.end_position().row
                        {
                            if top_node.kind() == "export_statement" {
                                return true;
                            }
                            let mut child_cursor = top_node.walk();
                            if top_node
                                .children(&mut child_cursor)
                                .any(|c| c.kind() == "visibility_modifier" || c.kind() == "export")
                            {
                                return true;
                            }
                        }
                    }
                    return false;
                }
            }
        }
        sym.signature
            .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
            .any(|tok| tok == "pub" || tok == "export")
    }

    fn plan_import_rewrites_for_files(
        workspace_root: &Path,
        from_rel: &str,
        to_rel: &str,
        moved_symbol_filter: Option<&str>,
        skip_paths: &[PathBuf],
    ) -> Result<Vec<PlannedFileMutation>> {
        let from_stem = Path::new(from_rel)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        let to_stem = Path::new(to_rel)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        if from_stem.is_empty() || to_stem.is_empty() || from_stem == to_stem {
            return Ok(Vec::new());
        }

        let files = Self::candidate_files_for_symbol(workspace_root, from_stem, None)?;
        let mut mutations = Vec::new();

        for file_path in files {
            if skip_paths.iter().any(|p| p == &file_path) {
                continue;
            }
            let Ok(content) = fs::read_to_string(&file_path) else {
                continue;
            };
            if !content.contains(from_stem) {
                continue;
            }
            if let Some(sym) = moved_symbol_filter {
                if !content.contains(sym) {
                    continue;
                }
            }
            let rel = file_path
                .strip_prefix(workspace_root)
                .unwrap_or(&file_path)
                .display()
                .to_string();
            let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");

            // Use Tree-sitter AST occurrence classification (`AstOccurrenceRole::Import`) to locate exact import byte spans
            let Some(occurrences) = classify_symbol_occurrences_ast(ext, &content, from_stem)
            else {
                continue;
            };
            let import_spans: Vec<(usize, usize)> = occurrences
                .into_iter()
                .filter(|occ| {
                    occ.role == AstOccurrenceRole::Import
                        && moved_symbol_filter.is_none_or(|sym| {
                            crate::utils::strings::has_word(&occ.line_snippet, sym)
                        })
                })
                .map(|occ| (occ.start_byte, occ.end_byte))
                .collect();

            if import_spans.is_empty() {
                continue;
            }

            let mut updated = content.clone();
            for &(start_b, end_b) in import_spans.iter().rev() {
                if start_b <= end_b
                    && end_b <= updated.len()
                    && updated.is_char_boundary(start_b)
                    && updated.is_char_boundary(end_b)
                {
                    updated.replace_range(start_b..end_b, to_stem);
                }
            }

            if updated != content {
                let mut file_edits = Vec::new();
                let mut diff_block = format!("--- a/{}\n+++ b/{}\n", rel, rel);
                for (idx, (orig_line, new_line)) in content.lines().zip(updated.lines()).enumerate()
                {
                    if orig_line != new_line {
                        diff_block.push_str(&format!(
                            "@@ -{},1 +{},1 @@\n-{}\n+{}\n",
                            idx + 1,
                            idx + 1,
                            orig_line,
                            new_line
                        ));
                        file_edits.push(RefactorEdit {
                            file_path: rel.clone(),
                            start_line: idx + 1,
                            end_line: idx + 1,
                            original_text: orig_line.to_string(),
                            replacement_text: new_line.to_string(),
                        });
                    }
                }
                mutations.push(PlannedFileMutation {
                    full_path: file_path,
                    rel_path: rel,
                    original_content: content,
                    updated_content: updated,
                    rename_to: None,
                    delete_original: false,
                    edits: file_edits,
                    diff_block,
                });
            }
        }

        Ok(mutations)
    }

    fn candidate_files_for_symbol(
        workspace_root: &Path,
        symbol: &str,
        file_scope: Option<&str>,
    ) -> Result<Vec<PathBuf>> {
        if let Some(scope) = file_scope.filter(|s| !s.trim().is_empty()) {
            return Ok(vec![Self::resolve_workspace_path(workspace_root, scope)?]);
        }
        if symbol.len() >= TRIGRAM_MIN_QUERY_LEN {
            if let Ok(trigram_idx) =
                crate::context::search::trigram::TrigramIndex::get_or_build_cached(workspace_root)
            {
                let plan = crate::context::search::trigram::build_query_plan(symbol, false);
                let rel_paths = trigram_idx.query_candidate_files(&plan, None);
                if !rel_paths.is_empty() {
                    return Ok(rel_paths
                        .into_iter()
                        .map(|rel| workspace_root.join(rel))
                        .collect());
                }
            }
        }
        Ok(Self::discover_source_files(workspace_root))
    }

    fn replace_identifier_word(line: &str, target: &str, replacement: &str) -> String {
        let mut result = String::with_capacity(line.len() + 16);
        let mut chars = line.char_indices().peekable();
        let target_char_len = target.chars().count();

        while let Some((idx, ch)) = chars.next() {
            if line[idx..].starts_with(target) {
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
            .extensions(REFACTOR_SOURCE_EXTENSIONS)
            .collect_absolute_files()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_dry_run_preview_and_syntax_guard_refusal() {
        let dir = tempdir().unwrap();
        let ws = dir.path();
        let file = ws.join("store.rs");
        fs::write(
            &file,
            "pub fn use_store() -> i32 {\n    1\n}\npub fn caller() -> i32 {\n    use_store()\n}\n",
        )
        .unwrap();

        // 1. Dry-run preview returns `Preview` tag and leaves disk unchanged
        let preview = AstRefactorer::rename_symbol_with_options(
            ws,
            "use_store",
            "use_app_store",
            Some("store.rs"),
            true,
        )
        .unwrap();
        assert_eq!(preview.outcome_tag, "Preview");
        assert_eq!(preview.edits.len(), 2);
        assert!(fs::read_to_string(&file)
            .unwrap()
            .contains("pub fn use_store()"));

        // 2. Invalid syntax replacement (`fn`) is refused by SyntaxGuard before writing to disk
        let refused = AstRefactorer::rename_symbol_with_options(
            ws,
            "use_store",
            "fn (",
            Some("store.rs"),
            false,
        )
        .unwrap();
        assert_eq!(refused.outcome_tag, "Refused");
        assert!(fs::read_to_string(&file)
            .unwrap()
            .contains("pub fn use_store()"));

        // 3. Valid commit writes changes and returns `Applied`
        let applied = AstRefactorer::rename_symbol_with_options(
            ws,
            "use_store",
            "use_app_store",
            Some("store.rs"),
            false,
        )
        .unwrap();
        assert_eq!(applied.outcome_tag, "Applied");
        assert!(fs::read_to_string(&file)
            .unwrap()
            .contains("pub fn use_app_store()"));
    }

    #[test]
    fn test_scan_delete_and_unused_declarations() {
        let dir = tempdir().unwrap();
        let ws = dir.path();
        fs::write(
            ws.join("lib.rs"),
            "pub fn active_helper() -> i32 {\n    42\n}\n\npub fn dead_helper() -> i32 {\n    0\n}\n",
        )
        .unwrap();
        fs::write(
            ws.join("consumer.rs"),
            "use crate::lib::active_helper;\npub fn run_task() -> i32 {\n    let label = \"active_helper\";\n    active_helper()\n}\n",
        )
        .unwrap();

        // 1. scan_symbol classifies declaration, import, call, and string-literal roles
        let scan = AstRefactorer::scan_symbol(ws, "active_helper", None, None, true).unwrap();
        assert_eq!(scan.outcome_tag, "Result");
        assert!(scan.diff_preview.contains("[declaration]"));
        assert!(scan.diff_preview.contains("[import-specifier]"));
        assert!(scan.diff_preview.contains("[call-reference]"));
        assert!(scan.diff_preview.contains("[string-literal]"));
        assert!(scan.diff_preview.contains("flowchart LR"));

        // 2. delete_symbol refuses deleting `active_helper` because `consumer.rs` references it
        let refused_del =
            AstRefactorer::delete_symbol(ws, "lib.rs", "active_helper", false).unwrap();
        assert_eq!(refused_del.outcome_tag, "Refused");
        assert!(refused_del.diff_preview.contains("consumer.rs"));

        // 3. find_unused identifies `dead_helper`
        let unused = AstRefactorer::find_unused(ws, Some("lib.rs"), Some("exported")).unwrap();
        assert_eq!(unused.outcome_tag, "Result");
        assert!(unused.diff_preview.contains("dead_helper"));
        assert!(!unused.diff_preview.contains("active_helper"));

        // 4. delete_symbol cleanly deletes `dead_helper`
        let ok_del = AstRefactorer::delete_symbol(ws, "lib.rs", "dead_helper", false).unwrap();
        assert_eq!(ok_del.outcome_tag, "Applied");
        let lib_after = fs::read_to_string(ws.join("lib.rs")).unwrap();
        assert!(!lib_after.contains("dead_helper"));
        assert!(lib_after.contains("active_helper"));
    }

    #[test]
    fn test_move_symbol_private_guard_and_css_refactoring() {
        let dir = tempdir().unwrap();
        let ws = dir.path();
        fs::write(
            ws.join("source.rs"),
            "fn secret_local() -> i32 {\n    1\n}\n\npub fn DependentFn() -> i32 {\n    secret_local()\n}\n\npub fn StandaloneFn() -> i32 {\n    99\n}\n",
        )
        .unwrap();
        fs::write(
            ws.join("consumer.rs"),
            "use crate::source::StandaloneFn;\npub fn call_it() -> i32 {\n    StandaloneFn()\n}\n",
        )
        .unwrap();

        // 1. Moving `DependentFn` is refused because it calls unexported `secret_local`
        let refused_move =
            AstRefactorer::move_symbol(ws, "DependentFn", "source.rs", "dest.rs", false).unwrap();
        assert_eq!(refused_move.outcome_tag, "Refused");
        assert!(refused_move.diff_preview.contains("secret_local"));

        // 2. Moving `StandaloneFn` moves the declaration to `dest.rs` and rewrites `consumer.rs`'s `use` statement
        let ok_move =
            AstRefactorer::move_symbol(ws, "StandaloneFn", "source.rs", "dest.rs", false).unwrap();
        assert_eq!(ok_move.outcome_tag, "Applied");
        assert!(fs::read_to_string(ws.join("dest.rs"))
            .unwrap()
            .contains("pub fn StandaloneFn()"));
        assert!(fs::read_to_string(ws.join("consumer.rs"))
            .unwrap()
            .contains("use crate::dest::StandaloneFn;"));

        // 3. Variant-preserving Tailwind class rename & CSS custom property rename
        fs::write(
            ws.join("card.tsx"),
            "export const Card = () => <div className=\"p-4 hover:dark:md:!bg-gray-500 text-sm\" style={{ color: 'var(--brand-old)' }} />;\n",
        )
        .unwrap();
        let mut class_map = HashMap::new();
        class_map.insert("bg-gray-500".to_string(), "bg-neutral-500".to_string());
        let css_res =
            AstRefactorer::css_class_rename(ws, &class_map, Some("card.tsx"), false).unwrap();
        assert_eq!(css_res.outcome_tag, "Applied");
        let card_after = fs::read_to_string(ws.join("card.tsx")).unwrap();
        assert!(card_after.contains("hover:dark:md:!bg-neutral-500"));

        let var_res = AstRefactorer::css_var_rename(
            ws,
            "--brand-old",
            "--brand-new",
            Some("card.tsx"),
            false,
        )
        .unwrap();
        assert_eq!(var_res.outcome_tag, "Applied");
        assert!(fs::read_to_string(ws.join("card.tsx"))
            .unwrap()
            .contains("var(--brand-new)"));
    }

    #[test]
    fn test_ast_inline_variable_and_ts_import_rewrite() {
        let dir = tempdir().unwrap();
        let ws = dir.path();

        // 1. AST variable inlining in TypeScript with brace-containing strings and lexical shadowing
        fs::write(
            ws.join("calc.ts"),
            "export function compute(x: number): number {\n    const factor = 10;\n    const msg = \"} factor {\";\n    const first = x * factor;\n    const factor = 99;\n    return first + factor;\n}\n",
        )
        .unwrap();
        let inlined =
            AstRefactorer::inline_variable_with_options(ws, "calc.ts", "factor", false).unwrap();
        assert_eq!(inlined.outcome_tag, "Applied");
        let calc_after = fs::read_to_string(ws.join("calc.ts")).unwrap();
        assert!(calc_after.contains("const first = x * 10;"));
        // Shadowed `const factor = 99;` and its subsequent usage must remain intact
        assert!(calc_after.contains("const factor = 99;"));
        assert!(calc_after.contains("return first + factor;"));
        // String literal containing `factor` and braces must remain untouched
        assert!(calc_after.contains("const msg = \"} factor {\";"));

        // 2. TypeScript relative import string rewriting on `rename_file`
        fs::write(ws.join("old_widget.ts"), "export const Widget = 1;\n").unwrap();
        fs::write(
            ws.join("app.ts"),
            "import { Widget } from './old_widget';\nexport const val = Widget;\n",
        )
        .unwrap();
        let renamed =
            AstRefactorer::rename_file(ws, "old_widget.ts", "new_widget.ts", false).unwrap();
        assert_eq!(renamed.outcome_tag, "Applied");
        assert!(!ws.join("old_widget.ts").exists());
        assert!(ws.join("new_widget.ts").exists());
        assert!(fs::read_to_string(ws.join("app.ts"))
            .unwrap()
            .contains("import { Widget } from './new_widget';"));
    }
}

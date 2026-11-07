use crate::error::{ContextError, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use tree_sitter::{Language, Parser, Query, QueryCursor};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SymbolDef {
    pub name: String,
    pub kind: String, // "function", "class", "struct", "trait", "enum", "interface", "type_alias", "import"
    pub signature: String,
    pub line_number: usize,
    pub end_line: usize,
    pub doc_comment: Option<String>,
}

#[derive(Debug, Clone)]
pub struct FileAst {
    #[allow(dead_code)]
    pub path: PathBuf,
    pub mtime: SystemTime,
    pub file_size: u64,
    pub symbols: Vec<SymbolDef>,
}

pub struct RepoMapExtractor {
    rust_lang: Language,
    python_lang: Language,
    js_lang: Language,
    ts_lang: Language,
    cache: HashMap<PathBuf, FileAst>,
}

impl Default for RepoMapExtractor {
    fn default() -> Self {
        Self::new()
    }
}

impl RepoMapExtractor {
    pub fn new() -> Self {
        Self {
            rust_lang: tree_sitter_rust::LANGUAGE.into(),
            python_lang: tree_sitter_python::LANGUAGE.into(),
            js_lang: tree_sitter_javascript::LANGUAGE.into(),
            ts_lang: tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            cache: HashMap::new(),
        }
    }

    /// Helper to extract clean single-line signature from definition node text
    fn extract_signature(def_text: &str) -> String {
        let first_line = def_text.lines().next().unwrap_or("").trim();
        first_line
            .trim_end_matches('{')
            .trim_end_matches(':')
            .trim_end_matches(';')
            .trim()
            .to_string()
    }

    /// Helper to extract preceding doc comment (///, //!, //, or #)
    fn extract_doc_comment(code_lines: &[&str], start_row: usize) -> Option<String> {
        if start_row == 0 {
            return None;
        }
        let mut doc_lines = Vec::new();
        let mut curr = start_row;
        while curr > 0 {
            curr -= 1;
            let line = code_lines[curr].trim();
            if line.is_empty() {
                continue;
            }
            // Skip Rust outer and inner attributes (#[...], #![...]) and Python/TS decorators (@...)
            if line.starts_with("#[") || line.starts_with("#![") || line.starts_with('@') {
                continue;
            }
            if line.starts_with("///") || line.starts_with("//!") {
                let trimmed = line
                    .trim_start_matches("///")
                    .trim_start_matches("//!")
                    .trim();
                doc_lines.push(trimmed);
            } else if line.starts_with('#') {
                let trimmed = line.trim_start_matches('#').trim();
                doc_lines.push(trimmed);
            } else if line.starts_with("//") {
                let trimmed = line.trim_start_matches("//").trim();
                doc_lines.push(trimmed);
            } else {
                break;
            }
            if doc_lines.len() >= 3 {
                break;
            }
        }
        if doc_lines.is_empty() {
            None
        } else {
            doc_lines.reverse();
            Some(doc_lines.join(" "))
        }
    }

    /// Parses in-memory source code and extracts definition symbols using Tree-sitter without redundant disk I/O.
    pub fn extract_symbols_from_str(&self, extension: &str, code: &str) -> Result<Vec<SymbolDef>> {
        let (lang, query_str) = match extension {
            "rs" => (
                self.rust_lang.clone(),
                r#"
                ((function_item name: (identifier) @name) @def)
                ((struct_item name: (type_identifier) @name) @def)
                ((enum_item name: (type_identifier) @name) @def)
                ((trait_item name: (type_identifier) @name) @def)
                ((impl_item type: (type_identifier) @name) @def)
                ((use_declaration argument: (_) @name) @def)
                "#,
            ),
            "py" => (
                self.python_lang.clone(),
                r#"
                ((function_definition name: (identifier) @name) @def)
                ((class_definition name: (identifier) @name) @def)
                "#,
            ),
            "js" | "jsx" => (
                self.js_lang.clone(),
                r#"
                ((function_declaration name: (identifier) @name) @def)
                ((class_declaration name: (identifier) @name) @def)
                ((method_definition name: (property_identifier) @name) @def)
                ((variable_declarator
                    name: (identifier) @name
                    value: [(arrow_function) (function_expression)]) @def)
                "#,
            ),
            "ts" | "tsx" => (
                self.ts_lang.clone(),
                r#"
                ((function_declaration name: (identifier) @name) @def)
                ((class_declaration name: (type_identifier) @name) @def)
                ((interface_declaration name: (type_identifier) @name) @def)
                ((type_alias_declaration name: (type_identifier) @name) @def)
                ((enum_declaration name: (identifier) @name) @def)
                ((method_definition name: (property_identifier) @name) @def)
                ((variable_declarator
                    name: (identifier) @name
                    value: [(arrow_function) (function_expression)]) @def)
                "#,
            ),
            _ => {
                return Ok(Vec::new());
            }
        };

        let mut parser = Parser::new();
        parser
            .set_language(&lang)
            .map_err(|e| ContextError::TreeSitter(e.to_string()))?;

        let tree = parser
            .parse(code, None)
            .ok_or_else(|| ContextError::TreeSitter("Failed to generate AST tree".to_string()))?;

        let query =
            Query::new(&lang, query_str).map_err(|e| ContextError::TreeSitter(e.to_string()))?;

        let mut cursor = QueryCursor::new();
        let mut symbols = Vec::new();

        let name_idx = query.capture_index_for_name("name");
        let def_idx = query.capture_index_for_name("def");

        let code_bytes = code.as_bytes();
        let code_lines: Vec<&str> = code.lines().collect();

        let matches = cursor.matches(&query, tree.root_node(), code_bytes);
        for m in matches {
            let mut name_opt = None;
            let mut def_node_opt = None;

            for capture in m.captures {
                if Some(capture.index) == name_idx {
                    if let Ok(name_str) = capture.node.utf8_text(code_bytes) {
                        name_opt = Some((name_str.to_string(), capture.node));
                    }
                }
                if Some(capture.index) == def_idx {
                    def_node_opt = Some(capture.node);
                }
            }

            if let Some((name_str, name_node)) = name_opt {
                let (kind, signature, start_line, end_line, doc_comment) = if let Some(def_node) =
                    def_node_opt
                {
                    let def_text = def_node.utf8_text(code_bytes).unwrap_or("");
                    let sig = Self::extract_signature(def_text);
                    let start = def_node.start_position().row + 1;
                    let end = def_node.end_position().row + 1;
                    let doc = Self::extract_doc_comment(&code_lines, def_node.start_position().row);
                    let k = match def_node.kind() {
                        "function_item" | "function_definition" | "function_declaration" => {
                            "function"
                        }
                        "struct_item" => "struct",
                        "enum_item" => "enum",
                        "trait_item" => "trait",
                        "impl_item" => "impl",
                        "class_definition" | "class_declaration" => "class",
                        "interface_declaration" => "interface",
                        "type_alias_declaration" => "type_alias",
                        "use_declaration" | "import_statement" | "import_from_statement" => {
                            "import"
                        }
                        other => other,
                    };
                    (k.to_string(), sig, start, end, doc)
                } else {
                    let start = name_node.start_position().row + 1;
                    let end = name_node.end_position().row + 1;
                    ("definition".to_string(), name_str.clone(), start, end, None)
                };

                symbols.push(SymbolDef {
                    name: name_str,
                    kind,
                    signature,
                    line_number: start_line,
                    end_line,
                    doc_comment,
                });
            }
        }

        Ok(symbols)
    }

    /// Parses a source code file and extracts definition symbols using Tree-sitter.
    pub fn extract_file_symbols(&mut self, file_path: &Path) -> Result<Vec<SymbolDef>> {
        let metadata = std::fs::metadata(file_path)?;
        let mtime = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        let file_size = metadata.len();

        // Check incremental cache
        if let Some(cached) = self.cache.get(file_path) {
            if cached.mtime == mtime && cached.file_size == file_size {
                return Ok(cached.symbols.clone());
            }
        }

        let extension = file_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or_default();

        let code = std::fs::read_to_string(file_path)?;
        let symbols = self.extract_symbols_from_str(extension, &code)?;

        // Cache result
        self.cache.insert(
            file_path.to_path_buf(),
            FileAst {
                path: file_path.to_path_buf(),
                mtime,
                file_size,
                symbols: symbols.clone(),
            },
        );

        Ok(symbols)
    }

    /// Extracts called function/method/constructor identifier names from `code` using Tree-sitter AST traversal
    /// (with a token-boundary call fallback `ident(` when a partial snippet does not form a full root AST node).
    /// Uses zero regular expressions.
    #[must_use]
    pub fn extract_called_identifiers_ast(
        &self,
        extension: &str,
        code: &str,
    ) -> std::collections::BTreeSet<String> {
        let mut calls = std::collections::BTreeSet::new();
        let lang_opt = match extension {
            "rs" => Some(self.rust_lang.clone()),
            "py" => Some(self.python_lang.clone()),
            "js" | "jsx" => Some(self.js_lang.clone()),
            "ts" | "tsx" => Some(self.ts_lang.clone()),
            _ => None,
        };

        if let Some(lang) = lang_opt {
            let mut parser = Parser::new();
            if parser.set_language(&lang).is_ok() {
                if let Some(tree) = parser.parse(code, None) {
                    collect_ast_call_identifiers(tree.root_node(), code, &mut calls);
                }
            }
        }

        // Token-boundary `ident(` scan on stripped code lines to catch calls inside partial diff fragments
        let stripped = crate::utils::strings::strip_code_strings_and_comments(code);
        for line in stripped.lines() {
            let trimmed = line.trim();
            if crate::constants::FUNCTION_DECL_LINE_PREFIXES
                .iter()
                .any(|prefix| trimmed.starts_with(prefix))
            {
                continue;
            }
            extract_token_calls_before_paren(trimmed, &mut calls);
        }

        calls
    }
}

fn collect_ast_call_identifiers(
    node: tree_sitter::Node<'_>,
    source: &str,
    out: &mut std::collections::BTreeSet<String>,
) {
    let kind = node.kind();
    if matches!(
        kind,
        "call_expression" | "call" | "new_expression" | "macro_invocation"
    ) {
        let callee_opt = node
            .child_by_field_name("function")
            .or_else(|| node.child_by_field_name("macro"))
            .or_else(|| node.child_by_field_name("constructor"))
            .or_else(|| node.named_child(0));
        if let Some(callee) = callee_opt {
            if let Some(ident) = rightmost_identifier_in_node(callee, source) {
                if ident.len() >= crate::constants::REUSE_MIN_IDENTIFIER_LEN
                    && !is_language_control_keyword(ident)
                {
                    out.insert(ident.to_string());
                }
            }
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_ast_call_identifiers(child, source, out);
    }
}

fn rightmost_identifier_in_node<'a>(
    node: tree_sitter::Node<'_>,
    source: &'a str,
) -> Option<&'a str> {
    let kind = node.kind();
    if node.child_count() == 0
        && (kind.contains("identifier")
            || kind == "field_identifier"
            || kind == "property_identifier")
    {
        return source.get(node.start_byte()..node.end_byte());
    }
    let mut cursor = node.walk();
    let children: Vec<_> = node.children(&mut cursor).collect();
    for child in children.into_iter().rev() {
        if let Some(found) = rightmost_identifier_in_node(child, source) {
            return Some(found);
        }
    }
    None
}

fn extract_token_calls_before_paren(line: &str, out: &mut std::collections::BTreeSet<String>) {
    let bytes = line.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'(' && i > 0 {
            let mut end = i;
            while end > 0 && bytes[end - 1].is_ascii_whitespace() {
                end -= 1;
            }
            let mut start = end;
            while start > 0
                && (bytes[start - 1].is_ascii_alphanumeric() || bytes[start - 1] == b'_')
            {
                start -= 1;
            }
            if end > start {
                let first = bytes[start];
                if (first.is_ascii_alphabetic() || first == b'_')
                    && (end - start) >= crate::constants::REUSE_MIN_IDENTIFIER_LEN
                {
                    if let Some(ident) = line.get(start..end) {
                        if !is_language_control_keyword(ident) {
                            out.insert(ident.to_string());
                        }
                    }
                }
            }
        }
        i += 1;
    }
}

fn is_language_control_keyword(ident: &str) -> bool {
    crate::constants::CALL_IGNORED_KEYWORDS
        .binary_search(&ident)
        .is_ok()
}

/// An orphaned caller reference pointing to a symbol whose definition was removed or renamed without updating the caller.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenCallerHit {
    pub symbol: String,
    pub file_path: String,
    pub line: usize,
    pub role: String,
}

/// Ponytail-inspired AST telemetry for an individual file edit (`reused`, `created`, `reuse_pct`, `open_callers`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditReuseAndCallerReport {
    pub reused_symbols: Vec<String>,
    pub created_symbols: Vec<String>,
    pub removed_symbols: Vec<String>,
    pub reuse_pct: Option<u32>,
    pub open_callers: Vec<OpenCallerHit>,
}

impl EditReuseAndCallerReport {
    /// Formats a concise 1-line telemetry badge for tool output / turn observability when meaningful.
    #[must_use]
    pub fn format_telemetry_badge(&self) -> Option<String> {
        if self.reuse_pct.is_none() && self.open_callers.is_empty() {
            return None;
        }
        let mut parts = Vec::new();
        if let Some(pct) = self.reuse_pct {
            parts.push(format!(
                "reuse {}% ({} reused, {} new)",
                pct,
                self.reused_symbols.len(),
                self.created_symbols.len()
            ));
        } else if !self.reused_symbols.is_empty() || !self.created_symbols.is_empty() {
            parts.push(format!(
                "{} reused, {} new",
                self.reused_symbols.len(),
                self.created_symbols.len()
            ));
        }
        if !self.open_callers.is_empty() {
            let preview: Vec<String> = self
                .open_callers
                .iter()
                .take(3)
                .map(|c| format!("`{}` at {}:{}", c.symbol, c.file_path, c.line))
                .collect();
            parts.push(format!(
                "! open_callers: {} ({})",
                self.open_callers.len(),
                preview.join(", ")
            ));
        }
        if parts.is_empty() {
            None
        } else {
            Some(format!(
                "[AST Reuse & Caller Telemetry: {}]",
                parts.join(" | ")
            ))
        }
    }
}

/// Analyzes a file modification (`before` -> `after`) using Tree-sitter AST symbol extraction and
/// occurrence classification to compute:
/// 1. `created_symbols` (`new_defs - old_defs`)
/// 2. `removed_symbols` (`old_defs - new_defs`)
/// 3. `reused_symbols` (existing workspace definitions called in newly added lines) & `reuse_pct`
/// 4. `open_callers` (callers across the workspace still referencing a symbol in `removed_symbols`
///    when no definition of that symbol remains in the repository).
#[must_use]
pub fn analyze_edit_reuse_and_open_callers(
    workspace_root: &Path,
    file_path: &Path,
    before: &str,
    after: &str,
    extra_known_symbols: Option<&std::collections::HashSet<String>>,
) -> EditReuseAndCallerReport {
    let ext = file_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default();
    if !crate::constants::REFACTOR_SOURCE_EXTENSIONS.contains(&ext) {
        return EditReuseAndCallerReport::default();
    }

    let extractor = RepoMapExtractor::new();
    let old_defs: std::collections::BTreeSet<String> = extractor
        .extract_symbols_from_str(ext, before)
        .unwrap_or_default()
        .into_iter()
        .filter(|s| {
            s.kind != "import" && s.name.len() >= crate::constants::REUSE_MIN_IDENTIFIER_LEN
        })
        .map(|s| s.name)
        .collect();
    let new_defs: std::collections::BTreeSet<String> = extractor
        .extract_symbols_from_str(ext, after)
        .unwrap_or_default()
        .into_iter()
        .filter(|s| {
            s.kind != "import" && s.name.len() >= crate::constants::REUSE_MIN_IDENTIFIER_LEN
        })
        .map(|s| s.name)
        .collect();

    let created_symbols: Vec<String> = new_defs.difference(&old_defs).cloned().collect();
    let removed_symbols: Vec<String> = old_defs.difference(&new_defs).cloned().collect();

    // Gather known workspace definitions from cached CodeGraph + old_defs + optional extra_known_symbols
    let mut known_workspace_defs: std::collections::HashSet<String> =
        old_defs.iter().cloned().collect();
    if let Some(extra) = extra_known_symbols {
        known_workspace_defs.extend(extra.iter().cloned());
    }
    let cached_graph = crate::context::graph::CodeGraph::get_or_build_cached(workspace_root).ok();
    if let Some(ref graph) = cached_graph {
        for sym_name in graph.symbol_to_file().keys() {
            if sym_name.len() >= crate::constants::REUSE_MIN_IDENTIFIER_LEN {
                known_workspace_defs.insert(sym_name.clone());
            }
        }
    }

    // Extract added lines (`after` lines not present in `before`)
    let before_lines: std::collections::HashSet<&str> = before
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    let added_code: String = after
        .lines()
        .filter(|l| {
            let t = l.trim();
            !t.is_empty() && !before_lines.contains(t)
        })
        .collect::<Vec<_>>()
        .join("\n");

    let called_in_added = extractor.extract_called_identifiers_ast(ext, &added_code);
    let reused_symbols: Vec<String> = called_in_added
        .into_iter()
        .filter(|sym| known_workspace_defs.contains(sym) && !created_symbols.contains(sym))
        .collect();

    let total_sampled = reused_symbols.len() + created_symbols.len();
    let reuse_pct = if total_sampled >= crate::constants::REUSE_METRIC_MIN_SYMBOL_SAMPLE {
        Some(((reused_symbols.len() * 100 + total_sampled / 2) / total_sampled) as u32)
    } else {
        None
    };

    // Detect open_callers for any removed symbol that no longer has a declaration anywhere in the workspace
    let mut open_callers = Vec::new();
    if !removed_symbols.is_empty() {
        let edited_abs = if file_path.is_absolute() {
            file_path.to_path_buf()
        } else {
            workspace_root.join(file_path)
        };
        let edited_rel = edited_abs
            .strip_prefix(workspace_root)
            .unwrap_or(&edited_abs)
            .to_string_lossy()
            .replace('\\', "/");

        let source_files = crate::context::walker::WorkspaceWalker::new(workspace_root)
            .extensions(crate::constants::REFACTOR_SOURCE_EXTENSIONS)
            .collect_absolute_files();

        for removed_sym in &removed_symbols {
            let mut still_declared_elsewhere = new_defs.contains(removed_sym);
            let mut candidate_callers = Vec::new();

            // Check the edited file's new content first
            if !still_declared_elsewhere {
                if let Some(occs) = crate::context::pattern::classify_symbol_occurrences_ast(
                    ext,
                    after,
                    removed_sym,
                ) {
                    if occs
                        .iter()
                        .any(|o| o.role == crate::context::pattern::AstOccurrenceRole::Declaration)
                    {
                        still_declared_elsewhere = true;
                    } else {
                        for occ in occs {
                            if matches!(
                                occ.role,
                                crate::context::pattern::AstOccurrenceRole::Call
                                    | crate::context::pattern::AstOccurrenceRole::Import
                                    | crate::context::pattern::AstOccurrenceRole::TypeRef
                                    | crate::context::pattern::AstOccurrenceRole::IdentifierRef
                            ) {
                                candidate_callers.push(OpenCallerHit {
                                    symbol: removed_sym.clone(),
                                    file_path: edited_rel.clone(),
                                    line: occ.line,
                                    role: occ.role.as_str().to_string(),
                                });
                            }
                        }
                    }
                }
            }

            if !still_declared_elsewhere {
                for other_path in &source_files {
                    if *other_path == edited_abs {
                        continue;
                    }
                    let Ok(other_code) = std::fs::read_to_string(other_path) else {
                        continue;
                    };
                    if !crate::utils::strings::has_word(&other_code, removed_sym) {
                        continue;
                    }
                    let other_ext = other_path
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or_default();
                    let Some(occs) = crate::context::pattern::classify_symbol_occurrences_ast(
                        other_ext,
                        &other_code,
                        removed_sym,
                    ) else {
                        continue;
                    };
                    if occs
                        .iter()
                        .any(|o| o.role == crate::context::pattern::AstOccurrenceRole::Declaration)
                    {
                        still_declared_elsewhere = true;
                        break;
                    }
                    let other_rel = other_path
                        .strip_prefix(workspace_root)
                        .unwrap_or(other_path)
                        .to_string_lossy()
                        .replace('\\', "/");
                    for occ in occs {
                        if matches!(
                            occ.role,
                            crate::context::pattern::AstOccurrenceRole::Call
                                | crate::context::pattern::AstOccurrenceRole::Import
                                | crate::context::pattern::AstOccurrenceRole::TypeRef
                                | crate::context::pattern::AstOccurrenceRole::IdentifierRef
                        ) {
                            candidate_callers.push(OpenCallerHit {
                                symbol: removed_sym.clone(),
                                file_path: other_rel.clone(),
                                line: occ.line,
                                role: occ.role.as_str().to_string(),
                            });
                        }
                    }
                }
            }

            if !still_declared_elsewhere {
                open_callers.extend(candidate_callers);
            }
        }
    }

    EditReuseAndCallerReport {
        reused_symbols,
        created_symbols,
        removed_symbols,
        reuse_pct,
        open_callers,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_rust_symbols() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_ast_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let rs_file = temp_dir.join("sample.rs");
        let code = r#"
            /// Computes the sum of two integers
            pub fn compute_sum(a: i32, b: i32) -> i32 {
                a + b
            }

            /// User Account structure
            pub struct UserAccount {
                pub id: u64,
            }
        "#;
        std::fs::write(&rs_file, code).unwrap();

        let mut extractor = RepoMapExtractor::new();
        let symbols = extractor.extract_file_symbols(&rs_file).unwrap();

        assert_eq!(symbols.len(), 2);
        let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"compute_sum"));
        assert!(names.contains(&"UserAccount"));

        let fn_sym = symbols.iter().find(|s| s.name == "compute_sum").unwrap();
        assert_eq!(fn_sym.kind, "function");
        assert!(fn_sym.signature.contains("fn compute_sum"));
        assert_eq!(
            fn_sym.doc_comment.as_deref(),
            Some("Computes the sum of two integers")
        );

        let struct_sym = symbols.iter().find(|s| s.name == "UserAccount").unwrap();
        assert_eq!(struct_sym.kind, "struct");
        assert!(struct_sym.signature.contains("struct UserAccount"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_extract_python_and_ts_symbols() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_ast_py_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let py_file = temp_dir.join("service.py");
        let py_code = r#"
# UserService class
class UserService:
    # Authenticates user
    def authenticate(self, user_id):
        pass
"#;
        std::fs::write(&py_file, py_code).unwrap();

        let ts_file = temp_dir.join("types.ts");
        let ts_code = r#"
export interface UserProfile {
    id: string;
    name: string;
}

export class AccountManager {
    account_id: string;
}

export function formatProfile(user: UserProfile): string {
    return user.name;
}
"#;
        std::fs::write(&ts_file, ts_code).unwrap();

        let mut extractor = RepoMapExtractor::new();
        let py_symbols = extractor.extract_file_symbols(&py_file).unwrap();
        assert!(py_symbols
            .iter()
            .any(|s| s.name == "UserService" && s.kind == "class"));
        assert!(py_symbols
            .iter()
            .any(|s| s.name == "authenticate" && s.kind == "function"));

        let ts_symbols = extractor.extract_file_symbols(&ts_file).unwrap();
        assert!(ts_symbols
            .iter()
            .any(|s| s.name == "UserProfile" && s.kind == "interface"));
        assert!(ts_symbols
            .iter()
            .any(|s| s.name == "AccountManager" && s.kind == "class"));
        assert!(ts_symbols
            .iter()
            .any(|s| s.name == "formatProfile" && s.kind == "function"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_treesitter_abi_versions_match() {
        let core_version = tree_sitter::LANGUAGE_VERSION;
        let rust_lang: Language = tree_sitter_rust::LANGUAGE.into();
        let python_lang: Language = tree_sitter_python::LANGUAGE.into();
        let js_lang: Language = tree_sitter_javascript::LANGUAGE.into();
        let ts_lang: Language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();

        let languages = [
            ("rust", rust_lang.version()),
            ("python", python_lang.version()),
            ("javascript", js_lang.version()),
            ("typescript", ts_lang.version()),
        ];

        for (name, version) in &languages {
            assert!(
                *version <= core_version && *version >= tree_sitter::MIN_COMPATIBLE_LANGUAGE_VERSION,
                "Tree-sitter ABI incompatibility for {}: version {} not compatible with core version {} (min {})",
                name, version, core_version, tree_sitter::MIN_COMPATIBLE_LANGUAGE_VERSION
            );
        }
    }

    #[test]
    fn test_extract_ts_arrow_functions_and_methods() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_ts_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let ts_file = temp_dir.join("component.tsx");
        let ts_code = r#"
export enum Status {
    Active,
    Inactive,
}

export class ServiceClient {
    public connect() {
        return true;
    }
}

export const fetchUserData = async (id: string) => {
    return { id };
};
"#;
        std::fs::write(&ts_file, ts_code).unwrap();

        let mut extractor = RepoMapExtractor::new();
        let symbols = extractor.extract_file_symbols(&ts_file).unwrap();

        assert!(symbols.iter().any(|s| s.name == "Status"));
        assert!(symbols.iter().any(|s| s.name == "ServiceClient"));
        assert!(symbols.iter().any(|s| s.name == "connect"));
        assert!(symbols.iter().any(|s| s.name == "fetchUserData"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_analyze_edit_reuse_and_open_callers() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_reuse_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let helpers_rs = temp_dir.join("helpers.rs");
        let before = r#"
pub fn parse_token(s: &str) -> &str { s.trim() }
pub fn validate_checksum(s: &str) -> bool { !s.is_empty() }
pub fn normalize_slug(s: &str) -> String { s.to_lowercase() }
pub fn legacy_dispatch(s: &str) -> bool { validate_checksum(s) }
"#;
        std::fs::write(&helpers_rs, before).unwrap();

        let caller_rs = temp_dir.join("caller.rs");
        std::fs::write(
            &caller_rs,
            "pub fn run_flow(input: &str) -> bool {\n    crate::helpers::legacy_dispatch(input)\n}\n",
        )
        .unwrap();

        // Edit helpers.rs: remove `legacy_dispatch`, add `process_pipeline` that reuses 3 existing helpers
        let after = r#"
pub fn parse_token(s: &str) -> &str { s.trim() }
pub fn validate_checksum(s: &str) -> bool { !s.is_empty() }
pub fn normalize_slug(s: &str) -> String { s.to_lowercase() }
pub fn process_pipeline(raw: &str) -> Option<String> {
    let tok = parse_token(raw);
    if !validate_checksum(tok) {
        return None;
    }
    Some(normalize_slug(tok))
}
"#;
        std::fs::write(&helpers_rs, after).unwrap();

        let report =
            analyze_edit_reuse_and_open_callers(&temp_dir, &helpers_rs, before, after, None);

        assert_eq!(report.created_symbols, vec!["process_pipeline".to_string()]);
        assert_eq!(report.removed_symbols, vec!["legacy_dispatch".to_string()]);
        assert_eq!(
            report.reused_symbols,
            vec![
                "normalize_slug".to_string(),
                "parse_token".to_string(),
                "validate_checksum".to_string()
            ]
        );
        // 3 reused / (3 reused + 1 created) = 75%
        assert_eq!(report.reuse_pct, Some(75));
        assert_eq!(report.open_callers.len(), 1);
        assert_eq!(report.open_callers[0].symbol, "legacy_dispatch");
        assert_eq!(report.open_callers[0].file_path, "caller.rs");
        assert_eq!(report.open_callers[0].line, 2);

        let badge = report.format_telemetry_badge().unwrap();
        assert!(badge.contains("reuse 75% (3 reused, 1 new)"));
        assert!(badge.contains("open_callers: 1"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}

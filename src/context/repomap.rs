#![allow(dead_code)]

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
    pub path: PathBuf,
    pub mtime: SystemTime,
    pub symbols: Vec<SymbolDef>,
}

pub struct RepoMapExtractor {
    rust_lang: Language,
    python_lang: Language,
    js_lang: Language,
    ts_lang: Language,
    cache: HashMap<PathBuf, FileAst>,
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

    /// Helper to extract preceding doc comment (/// or #)
    fn extract_doc_comment(code_lines: &[&str], start_row: usize) -> Option<String> {
        if start_row == 0 {
            return None;
        }
        let mut doc_lines = Vec::new();
        let mut curr = start_row;
        while curr > 0 {
            curr -= 1;
            let line = code_lines[curr].trim();
            if line.starts_with("///") || line.starts_with("//!") {
                let trimmed = line
                    .trim_start_matches("///")
                    .trim_start_matches("//!")
                    .trim();
                doc_lines.push(trimmed);
            } else if line.starts_with('#') {
                let trimmed = line.trim_start_matches('#').trim();
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

    /// Parses a source code file and extracts definition symbols using Tree-sitter.
    pub fn extract_file_symbols(&mut self, file_path: &Path) -> Result<Vec<SymbolDef>> {
        let metadata = std::fs::metadata(file_path)?;
        let mtime = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);

        // Check incremental cache
        if let Some(cached) = self.cache.get(file_path) {
            if cached.mtime == mtime {
                return Ok(cached.symbols.clone());
            }
        }

        let extension = file_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or_default();

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
                "#,
            ),
            "ts" | "tsx" => (
                self.ts_lang.clone(),
                r#"
                ((function_declaration name: (identifier) @name) @def)
                ((class_declaration name: (type_identifier) @name) @def)
                ((interface_declaration name: (type_identifier) @name) @def)
                ((type_alias_declaration name: (type_identifier) @name) @def)
                "#,
            ),
            _ => {
                return Ok(Vec::new());
            }
        };

        let code = std::fs::read_to_string(file_path)?;
        let mut parser = Parser::new();
        parser
            .set_language(&lang)
            .map_err(|e| ContextError::TreeSitter(e.to_string()))?;

        let tree = parser
            .parse(&code, None)
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

        // Cache result
        self.cache.insert(
            file_path.to_path_buf(),
            FileAst {
                path: file_path.to_path_buf(),
                mtime,
                symbols: symbols.clone(),
            },
        );

        Ok(symbols)
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
}

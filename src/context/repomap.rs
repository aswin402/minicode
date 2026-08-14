#![allow(dead_code)]

use crate::error::{ContextError, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use tree_sitter::{Language, Parser, Query, QueryCursor};

#[derive(Debug, Clone, PartialEq)]
pub struct SymbolDef {
    pub name: String,
    pub kind: String, // "function", "class", "struct", "trait", "import"
    pub line_number: usize,
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
                ((class_declaration name: (identifier) @name) @def)
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

        let name_idx = query.capture_index_for_name("name").unwrap_or(0);

        let code_bytes = code.as_bytes();

        let matches = cursor.matches(&query, tree.root_node(), code_bytes);
        for m in matches {
            for capture in m.captures {
                if capture.index == name_idx {
                    if let Ok(name_str) = capture.node.utf8_text(code_bytes) {
                        let line_number = capture.node.start_position().row + 1;
                        let kind = capture
                            .node
                            .parent()
                            .map(|p| p.kind())
                            .unwrap_or("definition");

                        symbols.push(SymbolDef {
                            name: name_str.to_string(),
                            kind: kind.to_string(),
                            line_number,
                        });
                    }
                }
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
            pub fn compute_sum(a: i32, b: i32) -> i32 {
                a + b
            }

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

        std::fs::remove_dir_all(&temp_dir).ok();
    }
}

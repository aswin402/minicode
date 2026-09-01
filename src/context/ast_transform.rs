use crate::error::{Result, ToolError};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// Metadata representing an extracted AST node.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AstNodeInfo {
    pub kind: String,
    pub name: String,
    pub start_line: usize,
    pub end_line: usize,
    pub start_byte: usize,
    pub end_byte: usize,
    pub is_public: bool,
    pub snippet: String,
}

pub struct AstTransformer;

impl AstTransformer {
    /// Queries the AST of a source file for symbols/nodes matching the given criteria.
    pub fn query_nodes(
        workspace_root: &Path,
        file_path: &str,
        node_kind_filter: Option<&str>,
        name_filter: Option<&str>,
    ) -> Result<Vec<AstNodeInfo>> {
        let raw_path = workspace_root.join(file_path);
        let full_path =
            crate::sandbox::path::validate_path_in_workspace(workspace_root, &raw_path)?;
        if !full_path.exists() {
            return Err(ToolError::NotFound {
                name: file_path.to_string(),
            }
            .into());
        }

        let content = fs::read_to_string(&full_path).map_err(|e| ToolError::FileOp {
            path: file_path.to_string(),
            source: e,
        })?;

        let ext = full_path.extension().and_then(|e| e.to_str()).unwrap_or("");

        let mut nodes = Vec::new();
        match ext {
            "rs" => {
                let mut parser = tree_sitter::Parser::new();
                parser
                    .set_language(&tree_sitter_rust::LANGUAGE.into())
                    .map_err(|e| {
                        ToolError::CommandExec(format!("Tree-sitter parser error: {}", e))
                    })?;
                if let Some(tree) = parser.parse(&content, None) {
                    Self::traverse_rust(tree.root_node(), &content, &mut nodes);
                }
            }
            "py" => {
                let mut parser = tree_sitter::Parser::new();
                parser
                    .set_language(&tree_sitter_python::LANGUAGE.into())
                    .map_err(|e| {
                        ToolError::CommandExec(format!("Tree-sitter parser error: {}", e))
                    })?;
                if let Some(tree) = parser.parse(&content, None) {
                    Self::traverse_python(tree.root_node(), &content, &mut nodes);
                }
            }
            "ts" | "tsx" => {
                let mut parser = tree_sitter::Parser::new();
                parser
                    .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
                    .map_err(|e| {
                        ToolError::CommandExec(format!("Tree-sitter parser error: {}", e))
                    })?;
                if let Some(tree) = parser.parse(&content, None) {
                    Self::traverse_ts(tree.root_node(), &content, &mut nodes);
                }
            }
            "js" | "jsx" => {
                let mut parser = tree_sitter::Parser::new();
                parser
                    .set_language(&tree_sitter_javascript::LANGUAGE.into())
                    .map_err(|e| {
                        ToolError::CommandExec(format!("Tree-sitter parser error: {}", e))
                    })?;
                if let Some(tree) = parser.parse(&content, None) {
                    Self::traverse_ts(tree.root_node(), &content, &mut nodes);
                }
            }
            _ => {
                return Err(ToolError::InvalidArguments {
                    name: "ast_query".to_string(),
                    reason: format!("Unsupported language extension: '{}'", ext),
                }
                .into());
            }
        }

        // Apply filters
        let filtered = nodes
            .into_iter()
            .filter(|n| {
                if let Some(kind) = node_kind_filter {
                    if !n.kind.eq_ignore_ascii_case(kind) {
                        return false;
                    }
                }
                if let Some(name) = name_filter {
                    if !n.name.to_lowercase().contains(&name.to_lowercase()) {
                        return false;
                    }
                }
                true
            })
            .collect();

        Ok(filtered)
    }

    /// Extracts the full definition body for a specific symbol by name from the AST.
    pub fn extract_symbol(
        workspace_root: &Path,
        file_path: &str,
        symbol_name: &str,
    ) -> Result<AstNodeInfo> {
        let nodes = Self::query_nodes(workspace_root, file_path, None, Some(symbol_name))?;
        nodes
            .into_iter()
            .find(|n| n.name == symbol_name)
            .ok_or_else(|| {
                ToolError::NotFound {
                    name: format!("symbol '{}' in file '{}'", symbol_name, file_path),
                }
                .into()
            })
    }

    fn traverse_rust(node: tree_sitter::Node, content: &str, out: &mut Vec<AstNodeInfo>) {
        let kind = node.kind();
        if matches!(
            kind,
            "function_item" | "struct_item" | "enum_item" | "trait_item" | "impl_item"
        ) {
            let name = node
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(content.as_bytes()).ok())
                .unwrap_or_else(|| {
                    if kind == "impl_item" {
                        node.child_by_field_name("type")
                            .and_then(|n| n.utf8_text(content.as_bytes()).ok())
                            .unwrap_or("impl")
                    } else {
                        "anonymous"
                    }
                })
                .to_string();

            let is_pub = node.child_by_field_name("visibility_modifier").is_some()
                || node
                    .children(&mut node.walk())
                    .any(|c| c.kind() == "visibility_modifier");

            let snippet = node.utf8_text(content.as_bytes()).unwrap_or("").to_string();

            out.push(AstNodeInfo {
                kind: kind.to_string(),
                name,
                start_line: node.start_position().row + 1,
                end_line: node.end_position().row + 1,
                start_byte: node.start_byte(),
                end_byte: node.end_byte(),
                is_public: is_pub,
                snippet,
            });
        }

        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                Self::traverse_rust(child, content, out);
            }
        }
    }

    fn traverse_python(node: tree_sitter::Node, content: &str, out: &mut Vec<AstNodeInfo>) {
        let kind = node.kind();
        if matches!(kind, "function_definition" | "class_definition") {
            let name = node
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(content.as_bytes()).ok())
                .unwrap_or("anonymous")
                .to_string();

            let is_pub = !name.starts_with('_');
            let snippet = node.utf8_text(content.as_bytes()).unwrap_or("").to_string();

            out.push(AstNodeInfo {
                kind: kind.to_string(),
                name,
                start_line: node.start_position().row + 1,
                end_line: node.end_position().row + 1,
                start_byte: node.start_byte(),
                end_byte: node.end_byte(),
                is_public: is_pub,
                snippet,
            });
        }

        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                Self::traverse_python(child, content, out);
            }
        }
    }

    fn traverse_ts(node: tree_sitter::Node, content: &str, out: &mut Vec<AstNodeInfo>) {
        let kind = node.kind();
        if matches!(
            kind,
            "function_declaration"
                | "class_declaration"
                | "interface_declaration"
                | "type_alias_declaration"
                | "method_definition"
        ) {
            let name = node
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(content.as_bytes()).ok())
                .unwrap_or("anonymous")
                .to_string();

            let is_pub = node
                .parent()
                .map(|p| p.kind() == "export_statement")
                .unwrap_or(false);

            let snippet = node.utf8_text(content.as_bytes()).unwrap_or("").to_string();

            out.push(AstNodeInfo {
                kind: kind.to_string(),
                name,
                start_line: node.start_position().row + 1,
                end_line: node.end_position().row + 1,
                start_byte: node.start_byte(),
                end_byte: node.end_byte(),
                is_public: is_pub,
                snippet,
            });
        }

        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                Self::traverse_ts(child, content, out);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_ast_transformer_rust_queries() {
        let dir = tempdir().unwrap();
        let ws = dir.path();

        let rs_path = ws.join("main.rs");
        fs::write(
            &rs_path,
            r#"
pub struct User {
    pub id: u64,
}

pub fn create_user(id: u64) -> User {
    User { id }
}

fn internal_helper() {}
"#,
        )
        .unwrap();

        let all_nodes = AstTransformer::query_nodes(ws, "main.rs", None, None).unwrap();
        assert_eq!(all_nodes.len(), 3);

        let struct_nodes =
            AstTransformer::query_nodes(ws, "main.rs", Some("struct_item"), None).unwrap();
        assert_eq!(struct_nodes.len(), 1);
        assert_eq!(struct_nodes[0].name, "User");
        assert!(struct_nodes[0].is_public);

        let symbol = AstTransformer::extract_symbol(ws, "main.rs", "create_user").unwrap();
        assert_eq!(symbol.name, "create_user");
        assert!(symbol.snippet.contains("User { id }"));
    }
}

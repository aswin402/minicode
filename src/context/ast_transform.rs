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

/// Detailed result emitted after an AST node is successfully replaced.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AstReplaceResult {
    pub file_path: String,
    pub symbol_name: String,
    pub symbol_kind: String,
    pub old_start_line: usize,
    pub old_end_line: usize,
    pub new_start_line: usize,
    pub new_end_line: usize,
    pub bytes_replaced: usize,
    pub ast_diff_summary: String,
    pub breaking_changes: Vec<crate::context::ast_diff::BreakingChange>,
}

impl AstReplaceResult {
    pub fn format_receipt(&self) -> String {
        let mut out = format!(
            "✅ Successfully replaced AST node `{}` ({}) in `{}`.\n\
             • Replaced lines {}-{} with lines {}-{} ({} bytes replaced)\n\
             • In-memory Tree-sitter syntax validation: VALID\n\
             • Indentation harmonized with enclosing scope\n\n",
            self.symbol_name,
            self.symbol_kind,
            self.file_path,
            self.old_start_line,
            self.old_end_line,
            self.new_start_line,
            self.new_end_line,
            self.bytes_replaced,
        );

        if !self.breaking_changes.is_empty() {
            out.push_str("⚠️ **Potential Breaking Signature Changes**:\n");
            for bc in &self.breaking_changes {
                out.push_str(&format!(
                    "  • [{}] `{}`: {}\n",
                    bc.severity, bc.symbol_name, bc.reason
                ));
            }
            out.push('\n');
        }

        out.push_str(&self.ast_diff_summary);
        out
    }
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

    /// Surgically replaces an AST node by symbol name in a source file, with
    /// in-memory pre-disk Tree-sitter syntax validation and auto-aligned indentation.
    pub fn replace_node(
        workspace_root: &Path,
        file_path: &str,
        symbol_name: &str,
        replacement_code: &str,
        kind_filter: Option<&str>,
    ) -> Result<AstReplaceResult> {
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

        // 1. Locate target AST symbol in the file
        let nodes = Self::query_nodes(workspace_root, file_path, kind_filter, Some(symbol_name))?;
        let target_node = nodes
            .into_iter()
            .find(|n| n.name == symbol_name)
            .ok_or_else(|| {
                // Construct informative error with available symbols in the file
                let all =
                    Self::query_nodes(workspace_root, file_path, None, None).unwrap_or_default();
                let available: Vec<String> = all
                    .into_iter()
                    .take(12)
                    .map(|n| {
                        format!(
                            "{} `{}` (lines {}-{})",
                            n.kind, n.name, n.start_line, n.end_line
                        )
                    })
                    .collect();
                ToolError::NotFound {
                    name: if available.is_empty() {
                        format!("symbol '{}' in file '{}'", symbol_name, file_path)
                    } else {
                        format!(
                            "symbol '{}' in file '{}'. Available symbols:\n• {}",
                            symbol_name,
                            file_path,
                            available.join("\n• ")
                        )
                    },
                }
            })?;

        // 2. Pre-disk in-memory Tree-sitter syntax validation
        Self::validate_syntax_in_memory(ext, replacement_code, &target_node.kind)?;

        let start_b = target_node.start_byte;
        let end_b = target_node.end_byte;

        if start_b > end_b || end_b > content.len() {
            return Err(ToolError::CommandExec(format!(
                "Corrupt AST node byte boundaries for '{}': [{}..{}] exceeds file length {}",
                symbol_name,
                start_b,
                end_b,
                content.len()
            ))
            .into());
        }

        if !content.is_char_boundary(start_b) || !content.is_char_boundary(end_b) {
            return Err(ToolError::CommandExec(format!(
                "AST node byte boundaries [{}..{}] do not lie on UTF-8 char boundaries",
                start_b, end_b
            ))
            .into());
        }

        // 3. Auto-align indentation with enclosing scope
        let target_indent = Self::get_target_indentation(&content, start_b);
        let aligned_code = Self::align_indentation(replacement_code, &target_indent);

        // 4. Construct new content and write to disk
        let mut new_content = String::with_capacity(content.len() + aligned_code.len());
        new_content.push_str(&content[..start_b]);
        new_content.push_str(&aligned_code);
        new_content.push_str(&content[end_b..]);

        fs::write(&full_path, &new_content).map_err(|e| ToolError::FileOp {
            path: file_path.to_string(),
            source: e,
        })?;

        // 5. Compute structural AST diff report
        let delta = crate::context::ast_diff::AstDiffEngine::diff_sources(
            file_path,
            ext,
            &content,
            &new_content,
        )?;

        let new_lines_count = aligned_code.lines().count();
        let new_end_line = target_node.start_line + new_lines_count.saturating_sub(1);

        Ok(AstReplaceResult {
            file_path: file_path.to_string(),
            symbol_name: symbol_name.to_string(),
            symbol_kind: target_node.kind,
            old_start_line: target_node.start_line,
            old_end_line: target_node.end_line,
            new_start_line: target_node.start_line,
            new_end_line,
            bytes_replaced: end_b.saturating_sub(start_b),
            ast_diff_summary: delta.format_markdown(),
            breaking_changes: delta.breaking_changes,
        })
    }

    /// Extracts leading whitespace on the line containing `start_byte`.
    pub fn get_target_indentation(content: &str, start_byte: usize) -> String {
        if start_byte == 0 || start_byte > content.len() {
            return String::new();
        }
        let before = &content[..start_byte];
        let line_start = before.rfind('\n').map(|idx| idx + 1).unwrap_or(0);
        before[line_start..]
            .chars()
            .take_while(|c| *c == ' ' || *c == '\t')
            .collect()
    }

    /// Aligns replacement code indentation with the target indentation.
    /// Note: line 0 uses the indentation already present before `start_byte`,
    /// while subsequent lines have `target_indent` prepended.
    pub fn align_indentation(replacement: &str, target_indent: &str) -> String {
        let trimmed_code = replacement.trim_matches(|c| c == '\r' || c == '\n');
        let lines: Vec<&str> = trimmed_code.lines().collect();
        if lines.is_empty() {
            return String::new();
        }

        let min_indent = lines
            .iter()
            .filter(|l| !l.trim().is_empty())
            .map(|l| l.chars().take_while(|c| *c == ' ' || *c == '\t').count())
            .min()
            .unwrap_or(0);

        let mut result = String::new();
        for (i, line) in lines.iter().enumerate() {
            if i > 0 {
                result.push('\n');
            }
            if line.trim().is_empty() {
                continue;
            }

            let byte_offset = line
                .char_indices()
                .nth(min_indent)
                .map(|(idx, _)| idx)
                .unwrap_or(line.len());

            let stripped = &line[byte_offset..];

            if i > 0 {
                result.push_str(target_indent);
            }
            result.push_str(stripped);
        }

        result
    }

    fn check_syntax_errors(node: tree_sitter::Node, code: &str) -> Option<String> {
        if node.is_error() || node.is_missing() {
            let start = node.start_position();
            let snippet = node
                .utf8_text(code.as_bytes())
                .unwrap_or("<unknown>")
                .chars()
                .take(30)
                .collect::<String>();
            return Some(format!(
                "syntax error near '{}' at line {}, column {}",
                snippet,
                start.row + 1,
                start.column + 1
            ));
        }

        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                if let Some(err) = Self::check_syntax_errors(child, code) {
                    return Some(err);
                }
            }
        }
        None
    }

    /// In-memory syntax verification before disk write.
    pub fn validate_syntax_in_memory(ext: &str, code: &str, _symbol_kind: &str) -> Result<()> {
        let mut parser = tree_sitter::Parser::new();
        match ext {
            "rs" => {
                parser
                    .set_language(&tree_sitter_rust::LANGUAGE.into())
                    .map_err(|e| {
                        ToolError::CommandExec(format!("Tree-sitter parser error: {}", e))
                    })?;
                if let Some(tree) = parser.parse(code, None) {
                    if let Some(err) = Self::check_syntax_errors(tree.root_node(), code) {
                        // If symbol is inside an impl or struct, try wrapping in impl Dummy
                        let wrapped = format!("impl Dummy {{\n{}\n}}", code);
                        if let Some(wrapped_tree) = parser.parse(&wrapped, None) {
                            if Self::check_syntax_errors(wrapped_tree.root_node(), &wrapped)
                                .is_none()
                            {
                                return Ok(());
                            }
                        }
                        return Err(ToolError::InvalidArguments {
                            name: "ast_replace_node".to_string(),
                            reason: format!("In-memory syntax validation failed: {}", err),
                        }
                        .into());
                    }
                }
            }
            "py" => {
                parser
                    .set_language(&tree_sitter_python::LANGUAGE.into())
                    .map_err(|e| {
                        ToolError::CommandExec(format!("Tree-sitter parser error: {}", e))
                    })?;
                if let Some(tree) = parser.parse(code, None) {
                    if let Some(err) = Self::check_syntax_errors(tree.root_node(), code) {
                        return Err(ToolError::InvalidArguments {
                            name: "ast_replace_node".to_string(),
                            reason: format!("In-memory syntax validation failed: {}", err),
                        }
                        .into());
                    }
                }
            }
            "ts" | "tsx" => {
                parser
                    .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
                    .map_err(|e| {
                        ToolError::CommandExec(format!("Tree-sitter parser error: {}", e))
                    })?;
                if let Some(tree) = parser.parse(code, None) {
                    if let Some(err) = Self::check_syntax_errors(tree.root_node(), code) {
                        let wrapped = format!("class Dummy {{\n{}\n}}", code);
                        if let Some(wrapped_tree) = parser.parse(&wrapped, None) {
                            if Self::check_syntax_errors(wrapped_tree.root_node(), &wrapped)
                                .is_none()
                            {
                                return Ok(());
                            }
                        }
                        return Err(ToolError::InvalidArguments {
                            name: "ast_replace_node".to_string(),
                            reason: format!("In-memory syntax validation failed: {}", err),
                        }
                        .into());
                    }
                }
            }
            "js" | "jsx" => {
                parser
                    .set_language(&tree_sitter_javascript::LANGUAGE.into())
                    .map_err(|e| {
                        ToolError::CommandExec(format!("Tree-sitter parser error: {}", e))
                    })?;
                if let Some(tree) = parser.parse(code, None) {
                    if let Some(err) = Self::check_syntax_errors(tree.root_node(), code) {
                        let wrapped = format!("class Dummy {{\n{}\n}}", code);
                        if let Some(wrapped_tree) = parser.parse(&wrapped, None) {
                            if Self::check_syntax_errors(wrapped_tree.root_node(), &wrapped)
                                .is_none()
                            {
                                return Ok(());
                            }
                        }
                        return Err(ToolError::InvalidArguments {
                            name: "ast_replace_node".to_string(),
                            reason: format!("In-memory syntax validation failed: {}", err),
                        }
                        .into());
                    }
                }
            }
            _ => {
                return Err(ToolError::InvalidArguments {
                    name: "ast_replace_node".to_string(),
                    reason: format!("Unsupported file extension '{}' for AST replacement", ext),
                }
                .into());
            }
        }
        Ok(())
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

    #[test]
    fn test_replace_node_rust_function() {
        let dir = tempdir().unwrap();
        let ws = dir.path();
        let rs_path = ws.join("lib.rs");
        fs::write(
            &rs_path,
            r#"
pub fn calculate(x: u32) -> u32 {
    x * 2
}

pub fn other() -> bool {
    true
}
"#,
        )
        .unwrap();

        let new_code = r#"pub fn calculate(x: u32) -> u32 {
    x * 4 + 1
}"#;

        let res = AstTransformer::replace_node(ws, "lib.rs", "calculate", new_code, None).unwrap();
        assert_eq!(res.symbol_name, "calculate");
        assert_eq!(res.symbol_kind, "function_item");
        assert!(res.bytes_replaced > 0);

        let content = fs::read_to_string(&rs_path).unwrap();
        assert!(content.contains("x * 4 + 1"));
        assert!(!content.contains("x * 2"));
        assert!(content.contains("pub fn other() -> bool"));

        let receipt = res.format_receipt();
        assert!(receipt.contains("calculate"));
        assert!(receipt.contains("VALID"));
    }

    #[test]
    fn test_replace_node_rust_method_with_auto_indent() {
        let dir = tempdir().unwrap();
        let ws = dir.path();
        let rs_path = ws.join("service.rs");
        fs::write(
            &rs_path,
            r#"
pub struct Service;

impl Service {
    pub fn process(&self) -> u32 {
        42
    }
}
"#,
        )
        .unwrap();

        // Pass code with 0 leading indentation; auto-align should add the 4 spaces!
        let new_code = r#"pub fn process(&self) -> u32 {
    let result = 100;
    result * 2
}"#;

        let res =
            AstTransformer::replace_node(ws, "service.rs", "process", new_code, None).unwrap();
        assert_eq!(res.symbol_name, "process");

        let content = fs::read_to_string(&rs_path).unwrap();
        assert!(content.contains("    pub fn process(&self) -> u32 {"));
        assert!(content.contains("        let result = 100;"));
        assert!(content.contains("        result * 2"));
        assert!(content.contains("    }"));
    }

    #[test]
    fn test_replace_node_python_function() {
        let dir = tempdir().unwrap();
        let ws = dir.path();
        let py_path = ws.join("app.py");
        fs::write(
            &py_path,
            r#"
class Controller:
    def handle_request(self, req):
        return "old response"
"#,
        )
        .unwrap();

        let new_code = r#"def handle_request(self, req):
    token = req.get("token")
    return f"processed: {token}""#;

        let res =
            AstTransformer::replace_node(ws, "app.py", "handle_request", new_code, None).unwrap();
        assert_eq!(res.symbol_name, "handle_request");

        let content = fs::read_to_string(&py_path).unwrap();
        assert!(content.contains("    def handle_request(self, req):"));
        assert!(content.contains("        token = req.get(\"token\")"));
        assert!(content.contains("        return f\"processed: {token}\""));
    }

    #[test]
    fn test_replace_node_syntax_error_rejected() {
        let dir = tempdir().unwrap();
        let ws = dir.path();
        let rs_path = ws.join("buggy.rs");
        let initial_content = "pub fn stable() -> bool {\n    true\n}\n";
        fs::write(&rs_path, initial_content).unwrap();

        // Pass invalid Rust syntax with unclosed braces/syntax error
        let broken_code = "pub fn stable( { let x = ; }";
        let err =
            AstTransformer::replace_node(ws, "buggy.rs", "stable", broken_code, None).unwrap_err();
        let err_str = err.to_string();
        assert!(err_str.contains("syntax validation failed") || err_str.contains("syntax error"));

        // Crucial safety check: file on disk was NOT modified!
        let content_after = fs::read_to_string(&rs_path).unwrap();
        assert_eq!(content_after, initial_content);
    }

    #[test]
    fn test_replace_node_symbol_not_found() {
        let dir = tempdir().unwrap();
        let ws = dir.path();
        let rs_path = ws.join("test.rs");
        fs::write(&rs_path, "pub fn existing_fn() {}\n").unwrap();

        let err = AstTransformer::replace_node(
            ws,
            "test.rs",
            "nonexistent_fn",
            "pub fn nonexistent_fn() {}",
            None,
        )
        .unwrap_err();

        let err_str = err.to_string();
        assert!(err_str.contains("nonexistent_fn"));
        assert!(err_str.contains("existing_fn")); // Helpful available symbols list
    }
}

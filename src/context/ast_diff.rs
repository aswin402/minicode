use crate::context::ast_transform::AstNodeInfo;
use crate::error::{Result, ToolError};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AstSymbolDelta {
    pub kind: String,
    pub name: String,
    pub is_public: bool,
    pub start_line: usize,
    pub end_line: usize,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AstModifiedDelta {
    pub kind: String,
    pub name: String,
    pub is_public: bool,
    pub old_start_line: usize,
    pub old_end_line: usize,
    pub new_start_line: usize,
    pub new_end_line: usize,
    pub signature_changed: bool,
    pub body_changed: bool,
    pub old_signature: String,
    pub new_signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BreakingChange {
    pub symbol_name: String,
    pub kind: String,
    pub reason: String,
    pub severity: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AstDeltaReport {
    pub file_path: String,
    pub added: Vec<AstSymbolDelta>,
    pub removed: Vec<AstSymbolDelta>,
    pub modified: Vec<AstModifiedDelta>,
    pub breaking_changes: Vec<BreakingChange>,
    pub unchanged_count: usize,
}

impl AstDeltaReport {
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty() && self.modified.is_empty()
    }

    pub fn format_markdown(&self) -> String {
        let mut out = format!("# Semantic AST Diff: `{}`\n\n", self.file_path);

        if !self.breaking_changes.is_empty() {
            out.push_str("### ⚠️ Potential Breaking Changes\n");
            for b in &self.breaking_changes {
                out.push_str(&format!(
                    "- **[{}]** `{}` ({}) — {}\n",
                    b.severity, b.symbol_name, b.kind, b.reason
                ));
            }
            out.push('\n');
        }

        if !self.added.is_empty() {
            out.push_str("### ➕ Added Symbols\n");
            for a in &self.added {
                let vis = if a.is_public { "pub " } else { "" };
                out.push_str(&format!(
                    "- `{}` **`{}{}`** (lines {}-{})\n  ```\n  {}\n  ```\n",
                    a.kind, vis, a.name, a.start_line, a.end_line, a.signature
                ));
            }
            out.push('\n');
        }

        if !self.removed.is_empty() {
            out.push_str("### ➖ Removed Symbols\n");
            for r in &self.removed {
                let vis = if r.is_public { "pub " } else { "" };
                out.push_str(&format!(
                    "- `{}` **`{}{}`** (was lines {}-{})\n  ```\n  {}\n  ```\n",
                    r.kind, vis, r.name, r.start_line, r.end_line, r.signature
                ));
            }
            out.push('\n');
        }

        if !self.modified.is_empty() {
            out.push_str("### 🔄 Modified Symbols\n");
            for m in &self.modified {
                let change_type = if m.signature_changed {
                    "Signature & Body Modified"
                } else {
                    "Body Modified (Signature Stable)"
                };
                let vis = if m.is_public { "pub " } else { "" };
                out.push_str(&format!(
                    "- `{}` **`{}{}`** — *{}* (lines {}->{})\n",
                    m.kind, vis, m.name, change_type, m.old_start_line, m.new_start_line
                ));
                if m.signature_changed {
                    out.push_str(&format!(
                        "  - **Old:** `{}`\n  - **New:** `{}`\n",
                        m.old_signature, m.new_signature
                    ));
                }
            }
            out.push('\n');
        }

        out.push_str(&format!(
            "📊 **Summary:** {} added, {} removed, {} modified, {} unchanged.\n",
            self.added.len(),
            self.removed.len(),
            self.modified.len(),
            self.unchanged_count
        ));

        out
    }
}

pub struct AstDiffEngine;

impl AstDiffEngine {
    /// Extracts symbols from in-memory source content based on language extension
    pub fn parse_source(ext: &str, content: &str) -> Result<Vec<AstNodeInfo>> {
        let mut nodes = Vec::new();
        match ext {
            "rs" => {
                let mut parser = tree_sitter::Parser::new();
                parser
                    .set_language(&tree_sitter_rust::LANGUAGE.into())
                    .map_err(|e| {
                        ToolError::CommandExec(format!("Tree-sitter parser error: {}", e))
                    })?;
                if let Some(tree) = parser.parse(content, None) {
                    Self::traverse_rust(tree.root_node(), content, &mut nodes);
                }
            }
            "py" => {
                let mut parser = tree_sitter::Parser::new();
                parser
                    .set_language(&tree_sitter_python::LANGUAGE.into())
                    .map_err(|e| {
                        ToolError::CommandExec(format!("Tree-sitter parser error: {}", e))
                    })?;
                if let Some(tree) = parser.parse(content, None) {
                    Self::traverse_python(tree.root_node(), content, &mut nodes);
                }
            }
            "ts" | "tsx" => {
                let mut parser = tree_sitter::Parser::new();
                parser
                    .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
                    .map_err(|e| {
                        ToolError::CommandExec(format!("Tree-sitter parser error: {}", e))
                    })?;
                if let Some(tree) = parser.parse(content, None) {
                    Self::traverse_ts(tree.root_node(), content, &mut nodes);
                }
            }
            "js" | "jsx" => {
                let mut parser = tree_sitter::Parser::new();
                parser
                    .set_language(&tree_sitter_javascript::LANGUAGE.into())
                    .map_err(|e| {
                        ToolError::CommandExec(format!("Tree-sitter parser error: {}", e))
                    })?;
                if let Some(tree) = parser.parse(content, None) {
                    Self::traverse_ts(tree.root_node(), content, &mut nodes);
                }
            }
            _ => {
                return Err(ToolError::InvalidArguments {
                    name: "ast_diff".to_string(),
                    reason: format!("Unsupported file extension for AST diff: '{}'", ext),
                }
                .into());
            }
        }
        Ok(nodes)
    }

    /// Computes structural AST delta between old source code and new source code
    pub fn diff_sources(
        file_path: &str,
        ext: &str,
        old_source: &str,
        new_source: &str,
    ) -> Result<AstDeltaReport> {
        let old_nodes = Self::parse_source(ext, old_source)?;
        let new_nodes = Self::parse_source(ext, new_source)?;

        let mut old_map: HashMap<(String, String), AstNodeInfo> = HashMap::new();
        for node in old_nodes {
            old_map.insert((node.kind.clone(), node.name.clone()), node);
        }

        let mut new_map: HashMap<(String, String), AstNodeInfo> = HashMap::new();
        for node in new_nodes {
            new_map.insert((node.kind.clone(), node.name.clone()), node);
        }

        let mut added = Vec::new();
        let mut removed = Vec::new();
        let mut modified = Vec::new();
        let mut breaking_changes = Vec::new();
        let mut unchanged_count = 0;

        // Find added & modified
        for (key, new_node) in &new_map {
            if let Some(old_node) = old_map.get(key) {
                if old_node.snippet.trim() == new_node.snippet.trim() {
                    unchanged_count += 1;
                } else {
                    let old_sig = Self::extract_signature(&old_node.snippet);
                    let new_sig = Self::extract_signature(&new_node.snippet);
                    let sig_changed = old_sig != new_sig;

                    if new_node.is_public && sig_changed {
                        breaking_changes.push(BreakingChange {
                            symbol_name: new_node.name.clone(),
                            kind: new_node.kind.clone(),
                            reason: format!(
                                "Public signature changed from `{}` to `{}`",
                                old_sig, new_sig
                            ),
                            severity: "HIGH".to_string(),
                        });
                    }

                    modified.push(AstModifiedDelta {
                        kind: new_node.kind.clone(),
                        name: new_node.name.clone(),
                        is_public: new_node.is_public,
                        old_start_line: old_node.start_line,
                        old_end_line: old_node.end_line,
                        new_start_line: new_node.start_line,
                        new_end_line: new_node.end_line,
                        signature_changed: sig_changed,
                        body_changed: true,
                        old_signature: old_sig,
                        new_signature: new_sig,
                    });
                }
            } else {
                added.push(AstSymbolDelta {
                    kind: new_node.kind.clone(),
                    name: new_node.name.clone(),
                    is_public: new_node.is_public,
                    start_line: new_node.start_line,
                    end_line: new_node.end_line,
                    signature: Self::extract_signature(&new_node.snippet),
                });
            }
        }

        // Find removed
        for (key, old_node) in &old_map {
            if !new_map.contains_key(key) {
                if old_node.is_public {
                    breaking_changes.push(BreakingChange {
                        symbol_name: old_node.name.clone(),
                        kind: old_node.kind.clone(),
                        reason: "Public symbol was removed from definition scope".to_string(),
                        severity: "HIGH".to_string(),
                    });
                }

                removed.push(AstSymbolDelta {
                    kind: old_node.kind.clone(),
                    name: old_node.name.clone(),
                    is_public: old_node.is_public,
                    start_line: old_node.start_line,
                    end_line: old_node.end_line,
                    signature: Self::extract_signature(&old_node.snippet),
                });
            }
        }

        // Sort for deterministic reporting
        added.sort_by_key(|c| c.start_line);
        removed.sort_by_key(|c| c.start_line);
        modified.sort_by_key(|c| c.new_start_line);

        Ok(AstDeltaReport {
            file_path: file_path.to_string(),
            added,
            removed,
            modified,
            breaking_changes,
            unchanged_count,
        })
    }

    /// Diffs a workspace file against its git HEAD baseline or provided new content
    pub fn diff_file(
        workspace_root: &Path,
        file_path: &str,
        new_content_override: Option<&str>,
    ) -> Result<AstDeltaReport> {
        let full_path = workspace_root.join(file_path);
        let ext = full_path.extension().and_then(|e| e.to_str()).unwrap_or("");

        let new_content = if let Some(override_text) = new_content_override {
            override_text.to_string()
        } else if full_path.exists() {
            std::fs::read_to_string(&full_path).map_err(|e| ToolError::FileOp {
                path: file_path.to_string(),
                source: e,
            })?
        } else {
            String::new()
        };

        // Try reading git HEAD version as baseline; if untracked or git fails, use empty string
        let old_content = match std::process::Command::new("git")
            .current_dir(workspace_root)
            .args(["show", &format!("HEAD:{}", file_path)])
            .output()
        {
            Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout).to_string(),
            _ => String::new(),
        };

        Self::diff_sources(file_path, ext, &old_content, &new_content)
    }

    fn extract_signature(snippet: &str) -> String {
        let first_line = snippet.lines().next().unwrap_or("").trim();
        if let Some(pos) = first_line.find('{') {
            first_line[..pos].trim().to_string()
        } else {
            first_line.to_string()
        }
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

    #[test]
    fn test_ast_diff_rust_add_remove_modify() {
        let old_rs = r#"
pub fn calculate_total(a: i32, b: i32) -> i32 {
    a + b
}

fn helper() {
    println!("old");
}
"#;

        let new_rs = r#"
pub fn calculate_total(a: i32, b: i32, factor: f32) -> f32 {
    (a + b) as f32 * factor
}

pub struct Invoice {
    pub id: u64,
}
"#;

        let report = AstDiffEngine::diff_sources("src/lib.rs", "rs", old_rs, new_rs).unwrap();

        // 1. Check added
        assert_eq!(report.added.len(), 1);
        assert_eq!(report.added[0].name, "Invoice");
        assert_eq!(report.added[0].kind, "struct_item");

        // 2. Check removed
        assert_eq!(report.removed.len(), 1);
        assert_eq!(report.removed[0].name, "helper");

        // 3. Check modified
        assert_eq!(report.modified.len(), 1);
        assert_eq!(report.modified[0].name, "calculate_total");
        assert!(report.modified[0].signature_changed);

        // 4. Check breaking change detection
        assert_eq!(report.breaking_changes.len(), 1);
        assert_eq!(report.breaking_changes[0].symbol_name, "calculate_total");
        assert_eq!(report.breaking_changes[0].severity, "HIGH");

        // 5. Markdown formatting
        let md = report.format_markdown();
        assert!(md.contains("Semantic AST Diff"));
        assert!(md.contains("Breaking Changes"));
    }

    #[test]
    fn test_ast_diff_python_classes_and_functions() {
        let old_py = r#"
class AuthManager:
    def login(self, user):
        pass

def fetch_data():
    return 42
"#;

        let new_py = r#"
class AuthManager:
    def login(self, user, token):
        pass

def fetch_data():
    return 42
"#;

        let report = AstDiffEngine::diff_sources("auth.py", "py", old_py, new_py).unwrap();
        assert_eq!(report.unchanged_count, 1); // fetch_data is unchanged
        assert_eq!(report.modified.len(), 2);
        assert!(report.modified.iter().any(|m| m.name == "AuthManager"));
        assert!(report.modified.iter().any(|m| m.name == "login"));
    }
}

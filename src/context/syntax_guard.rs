use std::path::Path;
use tree_sitter::{Language, Node, Parser};

/// Detailed description of a syntax error detected by Tree-sitter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxErrorDetail {
    pub line: usize,
    pub column: usize,
    pub is_missing: bool,
    pub kind: String,
    pub snippet: String,
}

/// In-memory AST pre-write barrier that prevents syntax-breaking edits from corrupting disk files.
pub struct SyntaxGuard;

impl SyntaxGuard {
    /// Maps a file path to its corresponding Tree-sitter Language definition.
    pub fn language_for_path(path: &Path) -> Option<Language> {
        let ext = path.extension().and_then(|e| e.to_str())?;
        match ext {
            "rs" => Some(tree_sitter_rust::LANGUAGE.into()),
            "py" => Some(tree_sitter_python::LANGUAGE.into()),
            "js" | "jsx" | "mjs" | "cjs" => Some(tree_sitter_javascript::LANGUAGE.into()),
            "ts" | "tsx" | "mts" | "cts" => {
                Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            }
            _ => None,
        }
    }

    /// Recursively walks the AST to locate the first ERROR or MISSING node.
    fn find_first_error(node: Node, code: &str) -> Option<SyntaxErrorDetail> {
        if node.is_error() || node.is_missing() {
            let start = node.start_position();
            let snippet = code
                .lines()
                .nth(start.row)
                .unwrap_or("")
                .trim_end()
                .to_string();

            let kind = if node.is_missing() {
                format!("missing '{}'", node.kind())
            } else {
                let text = node.utf8_text(code.as_bytes()).unwrap_or("<syntax>");
                let display_text = if text.len() > 30 {
                    format!("{}...", &text[..30])
                } else if text.is_empty() {
                    "<token>".to_string()
                } else {
                    text.to_string()
                };
                format!("unexpected token '{}'", display_text)
            };

            return Some(SyntaxErrorDetail {
                line: start.row + 1,
                column: start.column + 1,
                is_missing: node.is_missing(),
                kind,
                snippet,
            });
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.has_error() {
                if let Some(err) = Self::find_first_error(child, code) {
                    return Some(err);
                }
            }
        }
        None
    }

    /// Verifies whether the proposed candidate content introduces new syntax errors.
    ///
    /// - Returns `Ok(())` if:
    ///   1. File extension is not a supported programming language (e.g. .md, .txt, .json).
    ///   2. The proposed content parses cleanly without syntax errors.
    ///   3. The original content already contained syntax errors (allows repairing broken files).
    /// - Returns `Err(diagnostic)` if the original file was syntactically valid but the proposed
    ///   content introduces syntax errors (broken braces, unclosed quotes, incomplete statements).
    pub fn check_syntax_barrier(
        path: &Path,
        original_content: &str,
        new_content: &str,
    ) -> Result<(), String> {
        let lang = match Self::language_for_path(path) {
            Some(l) => l,
            None => return Ok(()),
        };

        // If original content was empty (new file), treat original as clean
        let original_has_error = if original_content.trim().is_empty() {
            false
        } else {
            let mut orig_parser = Parser::new();
            if orig_parser.set_language(&lang).is_ok() {
                orig_parser
                    .parse(original_content, None)
                    .map(|tree| tree.root_node().has_error())
                    .unwrap_or(false)
            } else {
                false
            }
        };

        // If the original file was already broken, allow write so model can fix syntax
        if original_has_error {
            return Ok(());
        }

        let mut new_parser = Parser::new();
        if new_parser.set_language(&lang).is_err() {
            return Ok(());
        }

        let new_tree = match new_parser.parse(new_content, None) {
            Some(tree) => tree,
            None => return Ok(()),
        };

        let root = new_tree.root_node();
        if !root.has_error() {
            return Ok(());
        }

        let file_display = path.display();

        let err_detail = Self::find_first_error(root, new_content);
        let (line, col, kind, snippet) = match err_detail {
            Some(err) => (err.line, err.column, err.kind, err.snippet),
            None => (1, 1, "unparseable syntax".to_string(), String::new()),
        };

        let col_indent = " ".repeat(col.saturating_sub(1));
        let diagnostic = format!(
            "[AST Syntax Barrier Rejected]:\n\
             The proposed edit to '{file_display}' introduces a syntax error at line {line}:{col}:\n\
             ------------------------------------------------------------\n\
             {line:>4} | {snippet}\n\
                  | {col_indent}^ {kind}\n\
             ------------------------------------------------------------\n\
             The original file on disk was preserved untouched.\n\
             Suggested Next Action: Correct the syntax (check for missing or unclosed braces/parentheses) and retry the edit."
        );

        Err(diagnostic)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_valid_rust_code_passes() {
        let path = PathBuf::from("src/main.rs");
        let orig = "fn main() {\n    println!(\"hello\");\n}\n";
        let new = "fn main() {\n    println!(\"hello, world!\");\n}\n";

        let res = SyntaxGuard::check_syntax_barrier(&path, orig, new);
        assert!(res.is_ok());
    }

    #[test]
    fn test_syntax_breaking_rust_edit_rejected() {
        let path = PathBuf::from("src/lib.rs");
        let orig = "fn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n";
        // Missing closing brace
        let new = "fn add(a: i32, b: i32) -> i32 {\n    a + b\n";

        let res = SyntaxGuard::check_syntax_barrier(&path, orig, new);
        assert!(res.is_err());
        let msg = res.unwrap_err();
        assert!(msg.contains("[AST Syntax Barrier Rejected]"));
        assert!(msg.contains("src/lib.rs"));
        assert!(msg.contains("at line"));
    }

    #[test]
    fn test_syntax_repair_of_broken_original_allowed() {
        let path = PathBuf::from("src/broken.rs");
        // Original was broken
        let orig = "fn broken( {";
        // New is fixed
        let new = "fn broken() {}\n";

        let res = SyntaxGuard::check_syntax_barrier(&path, orig, new);
        assert!(res.is_ok());
    }

    #[test]
    fn test_unsupported_language_transparently_passed() {
        let path = PathBuf::from("README.md");
        let orig = "# Hello";
        let new = "# Hello\n```broken { [";

        let res = SyntaxGuard::check_syntax_barrier(&path, orig, new);
        assert!(res.is_ok());
    }

    #[test]
    fn test_python_syntax_barrier() {
        let path = PathBuf::from("script.py");
        let orig = "def greet(name):\n    return f'Hello {name}'\n";
        // Broken python syntax
        let new = "def greet(name:\n    return f'Hello {name}'\n";

        let res = SyntaxGuard::check_syntax_barrier(&path, orig, new);
        assert!(res.is_err());
        let msg = res.unwrap_err();
        assert!(msg.contains("[AST Syntax Barrier Rejected]"));
    }
}

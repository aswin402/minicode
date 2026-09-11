//! AST-driven import extraction and canonical module resolution across languages.

use serde::{Deserialize, Serialize};
use std::path::Path;

/// Structured representation of an import reference found within source code.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ImportReference {
    /// The raw text snippet from the import statement.
    pub raw: String,
    /// Canonical module path within the project (e.g. "src/ui/modals" or "src/agent/loop").
    pub canonical_module: String,
    /// Top-level architectural module category (e.g. "src/ui", "src/agent", "src/tools").
    pub top_level_module: String,
    /// 1-indexed source line number where the import was declared.
    pub line_number: usize,
}

/// Parser that inspects source code lines and extracts canonical module dependencies.
pub struct ArchParser;

impl ArchParser {
    /// Extracts all internal project import references from source content.
    #[must_use]
    pub fn extract_imports(rel_path: &str, content: &str, ext: &str) -> Vec<ImportReference> {
        match ext {
            "rs" => Self::extract_rust_imports(rel_path, content),
            "py" => Self::extract_python_imports(rel_path, content),
            "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" => {
                Self::extract_js_ts_imports(rel_path, content)
            }
            _ => Vec::new(),
        }
    }

    /// Extracts and canonicalizes Rust `use` statements.
    fn extract_rust_imports(rel_path: &str, content: &str) -> Vec<ImportReference> {
        let mut results = Vec::new();
        let lines: Vec<&str> = content.lines().collect();
        let mut i = 0;

        while i < lines.len() {
            let line = lines[i].trim();
            // Skip comments and attributes
            if line.starts_with("//") || line.starts_with("/*") || line.starts_with("#[") {
                i += 1;
                continue;
            }

            // Detect start of use statement
            if line.starts_with("use ")
                || line.starts_with("pub use ")
                || line.starts_with("pub(crate) use ")
                || line.starts_with("pub(super) use ")
            {
                let start_line = i + 1;
                let mut accumulated = line.to_string();

                // Accumulate until semicolon if multi-line
                while !accumulated.contains(';') && i + 1 < lines.len() {
                    i += 1;
                    accumulated.push(' ');
                    accumulated.push_str(lines[i].trim());
                }

                // Strip visibility keywords and trailing semicolon
                let clean = accumulated
                    .trim_start_matches("pub(crate) ")
                    .trim_start_matches("pub(super) ")
                    .trim_start_matches("pub ")
                    .trim_start_matches("use ")
                    .trim_end_matches(';')
                    .trim();

                let expanded = Self::expand_rust_use_tree("", clean);
                for raw_path in expanded {
                    let raw_path = raw_path.trim();
                    if let Some(canonical) = Self::canonicalize_rust_path(rel_path, raw_path) {
                        let top_level = Self::get_top_level_module(&canonical);
                        results.push(ImportReference {
                            raw: raw_path.to_string(),
                            canonical_module: canonical,
                            top_level_module: top_level,
                            line_number: start_line,
                        });
                    }
                }
            }

            i += 1;
        }

        results
    }

    /// Expands nested Rust use groups like `crate::{a, b::{c, d}}` into flat paths.
    pub fn expand_rust_use_tree(prefix: &str, tree: &str) -> Vec<String> {
        let tree = tree.trim();
        if tree.is_empty() {
            return Vec::new();
        }

        // Check if there is an opening brace
        if let Some(open_idx) = tree.find('{') {
            let base = tree[..open_idx].trim_end_matches("::").trim();
            let new_prefix = if prefix.is_empty() {
                base.to_string()
            } else if base.is_empty() {
                prefix.to_string()
            } else {
                format!("{}::{}", prefix, base)
            };

            // Find matching closing brace
            let mut depth = 0;
            let mut close_idx = None;
            for (idx, ch) in tree[open_idx..].char_indices() {
                if ch == '{' {
                    depth += 1;
                } else if ch == '}' {
                    depth -= 1;
                    if depth == 0 {
                        close_idx = Some(open_idx + idx);
                        break;
                    }
                }
            }

            if let Some(close_idx) = close_idx {
                let inside = &tree[open_idx + 1..close_idx];
                let items = Self::split_comma_top_level(inside);
                let mut results = Vec::new();
                for item in items {
                    let sub_expanded = Self::expand_rust_use_tree(&new_prefix, &item);
                    results.extend(sub_expanded);
                }
                return results;
            }
        }

        // Base case: no braces
        let full = if prefix.is_empty() {
            tree.to_string()
        } else {
            format!("{}::{}", prefix, tree)
        };

        // Remove aliases: "foo as bar" -> "foo"
        let without_alias = full
            .split_whitespace()
            .next()
            .unwrap_or(&full)
            .replace("r#", "");

        vec![without_alias]
    }

    /// Splits string by comma only at the top level of curly braces.
    fn split_comma_top_level(s: &str) -> Vec<String> {
        let mut items = Vec::new();
        let mut depth = 0;
        let mut current = String::new();

        for ch in s.chars() {
            if ch == '{' {
                depth += 1;
                current.push(ch);
            } else if ch == '}' {
                depth -= 1;
                current.push(ch);
            } else if ch == ',' && depth == 0 {
                let trimmed = current.trim();
                if !trimmed.is_empty() {
                    items.push(trimmed.to_string());
                }
                current.clear();
            } else {
                current.push(ch);
            }
        }

        let trimmed = current.trim();
        if !trimmed.is_empty() {
            items.push(trimmed.to_string());
        }

        items
    }

    /// Converts a Rust import path (e.g. `crate::ui::view` or `super::types`) to a canonical relative path.
    fn canonicalize_rust_path(rel_path: &str, raw_path: &str) -> Option<String> {
        // Internal project imports start with crate:: or minicode::
        if raw_path.starts_with("crate::") || raw_path.starts_with("minicode::") {
            let sub = raw_path
                .trim_start_matches("crate::")
                .trim_start_matches("minicode::")
                .trim_end_matches("::*");

            let mut parts: Vec<&str> = sub
                .split("::")
                .map(|s| s.trim_start_matches("r#"))
                .filter(|s| !s.is_empty())
                .collect();
            if parts.is_empty() {
                return None;
            }

            // If the last component starts with uppercase (Struct/Enum/Constant), pop it
            if parts.len() > 1
                && parts
                    .last()
                    .is_some_and(|s| s.chars().next().is_some_and(|c| c.is_ascii_uppercase()))
            {
                parts.pop();
            }

            let path_str = format!("src/{}", parts.join("/"));
            return Some(path_str);
        }

        // Relative imports starting with super::
        if raw_path.starts_with("super::") {
            let file_path = Path::new(rel_path);
            let is_mod_or_lib = file_path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|name| name == "mod.rs" || name == "lib.rs" || name == "main.rs");

            let mut current = if is_mod_or_lib {
                file_path
                    .parent()
                    .and_then(|p| p.parent())
                    .unwrap_or_else(|| Path::new("src"))
                    .to_path_buf()
            } else {
                file_path
                    .parent()
                    .unwrap_or_else(|| Path::new("src"))
                    .to_path_buf()
            };

            let mut remaining = raw_path;
            // First super:: is consumed (points to containing module)
            remaining = &remaining["super::".len()..];

            // Additional super:: calls climb to higher ancestors
            while remaining.starts_with("super::") {
                remaining = &remaining["super::".len()..];
                if let Some(p) = current.parent() {
                    current = p.to_path_buf();
                }
            }

            let sub = remaining.trim_end_matches("::*");
            let mut parts: Vec<&str> = sub
                .split("::")
                .map(|s| s.trim_start_matches("r#"))
                .filter(|s| !s.is_empty())
                .collect();

            if parts.len() > 1
                && parts
                    .last()
                    .is_some_and(|s| s.chars().next().is_some_and(|c| c.is_ascii_uppercase()))
            {
                parts.pop();
            }

            for part in parts {
                current = current.join(part);
            }

            let norm = current.to_string_lossy().to_string();
            if norm.starts_with("src") {
                return Some(norm);
            }
        }

        None
    }

    /// Extracts Python imports.
    fn extract_python_imports(rel_path: &str, content: &str) -> Vec<ImportReference> {
        let mut results = Vec::new();
        let file_dir = Path::new(rel_path)
            .parent()
            .unwrap_or_else(|| Path::new(""));

        for (line_idx, line) in content.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with('#') {
                continue;
            }

            // e.g. "import app.services as s" or "import os, sys"
            if trimmed.starts_with("import ") {
                let rest = trimmed.trim_start_matches("import ").trim();
                for part in rest.split(',') {
                    let mod_name = part.split_whitespace().next().unwrap_or("").trim();
                    if !mod_name.is_empty() && !is_builtin_python_module(mod_name) {
                        let path_str = mod_name.replace('.', "/");
                        let canonical = if path_str.starts_with("src/") {
                            path_str
                        } else {
                            format!("src/{}", path_str)
                        };
                        let top_level = Self::get_top_level_module(&canonical);
                        results.push(ImportReference {
                            raw: trimmed.to_string(),
                            canonical_module: canonical,
                            top_level_module: top_level,
                            line_number: line_idx + 1,
                        });
                    }
                }
            }
            // e.g. "from app.models import User" or "from .service import Worker"
            else if trimmed.starts_with("from ") {
                let parts: Vec<&str> = trimmed.split_whitespace().collect();
                if parts.len() >= 4 && parts[2] == "import" {
                    let source = parts[1];
                    let canonical = if source.starts_with('.') {
                        // Relative import
                        let dots = source.chars().take_while(|&c| c == '.').count();
                        let subpath = &source[dots..];
                        let mut dir = file_dir.to_path_buf();
                        for _ in 1..dots {
                            if let Some(p) = dir.parent() {
                                dir = p.to_path_buf();
                            }
                        }
                        if !subpath.is_empty() {
                            dir = dir.join(subpath.replace('.', "/"));
                        }
                        dir.to_string_lossy().to_string()
                    } else if !is_builtin_python_module(source) {
                        let sub = source.replace('.', "/");
                        if sub.starts_with("src/") {
                            sub
                        } else {
                            format!("src/{}", sub)
                        }
                    } else {
                        continue;
                    };

                    let top_level = Self::get_top_level_module(&canonical);
                    results.push(ImportReference {
                        raw: trimmed.to_string(),
                        canonical_module: canonical,
                        top_level_module: top_level,
                        line_number: line_idx + 1,
                    });
                }
            }
        }

        results
    }

    /// Extracts TypeScript and JavaScript import statements.
    fn extract_js_ts_imports(rel_path: &str, content: &str) -> Vec<ImportReference> {
        let mut results = Vec::new();
        let file_dir = Path::new(rel_path)
            .parent()
            .unwrap_or_else(|| Path::new(""));

        for (line_idx, line) in content.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") || trimmed.starts_with("/*") {
                continue;
            }

            let is_import = (trimmed.starts_with("import ") || trimmed.starts_with("export "))
                && trimmed.contains("from ");
            let is_require = trimmed.contains("require(") && trimmed.contains(')');

            if is_import || is_require {
                if let Some(specifier) = extract_quote_target(trimmed) {
                    // Only process relative project imports (starting with ./ or ../) or internal aliases (@/)
                    if specifier.starts_with('.') || specifier.starts_with("@/") {
                        let canonical = if let Some(stripped) = specifier.strip_prefix("@/") {
                            format!("src/{}", stripped)
                        } else {
                            resolve_relative_path(file_dir, specifier)
                        };

                        let top_level = Self::get_top_level_module(&canonical);
                        results.push(ImportReference {
                            raw: trimmed.to_string(),
                            canonical_module: canonical,
                            top_level_module: top_level,
                            line_number: line_idx + 1,
                        });
                    }
                }
            }
        }

        results
    }

    /// Extracts top-level module name (e.g. "src/ui/modals/session" -> "src/ui").
    #[must_use]
    pub fn get_top_level_module(canonical_path: &str) -> String {
        let parts: Vec<&str> = canonical_path.split('/').collect();
        if parts.len() >= 2 && parts[0] == "src" {
            format!("{}/{}", parts[0], parts[1])
        } else if !parts.is_empty() {
            parts[0].to_string()
        } else {
            canonical_path.to_string()
        }
    }
}

/// Helper extracting string between single, double, or backtick quotes.
fn extract_quote_target(line: &str) -> Option<&str> {
    for quote in ['"', '\'', '`'] {
        if let Some(first) = line.find(quote) {
            let rest = &line[first + 1..];
            if let Some(second) = rest.find(quote) {
                return Some(&rest[..second]);
            }
        }
    }
    None
}

/// Resolves a relative import path against a source directory.
fn resolve_relative_path(base_dir: &Path, rel: &str) -> String {
    let mut current = base_dir.to_path_buf();
    for part in rel.split('/') {
        if part == "." || part.is_empty() {
            continue;
        } else if part == ".." {
            if let Some(p) = current.parent() {
                current = p.to_path_buf();
            }
        } else {
            current = current.join(part);
        }
    }
    current.to_string_lossy().to_string()
}

/// Checks if Python module name is standard library.
fn is_builtin_python_module(name: &str) -> bool {
    matches!(
        name.split('.').next().unwrap_or(""),
        "os" | "sys"
            | "json"
            | "re"
            | "math"
            | "time"
            | "datetime"
            | "pathlib"
            | "typing"
            | "collections"
            | "itertools"
            | "functools"
            | "io"
            | "shutil"
            | "subprocess"
            | "threading"
            | "asyncio"
            | "logging"
            | "unittest"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expand_rust_use_single() {
        let res = ArchParser::expand_rust_use_tree("", "crate::ui::view::render_timeline");
        assert_eq!(res, vec!["crate::ui::view::render_timeline"]);
    }

    #[test]
    fn test_expand_rust_use_grouped() {
        let res = ArchParser::expand_rust_use_tree("crate", "{tools::exec, ui::modal}");
        assert_eq!(res, vec!["crate::tools::exec", "crate::ui::modal"]);
    }

    #[test]
    fn test_expand_rust_use_nested() {
        let res =
            ArchParser::expand_rust_use_tree("crate", "{agent::{providers, r#loop}, ui::view}");
        assert_eq!(
            res,
            vec![
                "crate::agent::providers",
                "crate::agent::loop",
                "crate::ui::view"
            ]
        );
    }

    #[test]
    fn test_extract_rust_imports_multiline() {
        let content = r#"
use std::collections::HashMap;
use crate::constants::{CB_DEFAULT_FAILURE_THRESHOLD, CB_DEFAULT_COOLDOWN_SECS};
pub(crate) use crate::ui::modals::{
    SessionBrowserModal,
    ApiKeyModal,
};
use super::layout_utils::compute_scroll_offset;
"#;
        let imports = ArchParser::extract_imports("src/ui/input.rs", content, "rs");
        assert!(imports
            .iter()
            .any(|i| i.canonical_module == "src/constants"));
        assert!(imports
            .iter()
            .any(|i| i.canonical_module == "src/ui/modals"));
        assert!(imports
            .iter()
            .any(|i| i.canonical_module == "src/ui/layout_utils/compute_scroll_offset"));
    }

    #[test]
    fn test_extract_python_imports() {
        let content = r#"
import os
from app.services.worker import BackgroundWorker
from .models import User
"#;
        let imports = ArchParser::extract_imports("src/api/routes.py", content, "py");
        assert!(imports
            .iter()
            .any(|i| i.canonical_module == "src/app/services/worker"));
        assert!(imports
            .iter()
            .any(|i| i.canonical_module == "src/api/models"));
    }

    #[test]
    fn test_extract_js_ts_imports() {
        let content = r#"
import React from 'react';
import { Button } from './components/Button';
import * as utils from '../utils/strings';
const helper = require('./helper');
"#;
        let imports = ArchParser::extract_imports("src/ui/views/Main.tsx", content, "tsx");
        assert!(imports
            .iter()
            .any(|i| i.canonical_module == "src/ui/views/components/Button"));
        assert!(imports
            .iter()
            .any(|i| i.canonical_module == "src/ui/utils/strings"));
        assert!(imports
            .iter()
            .any(|i| i.canonical_module == "src/ui/views/helper"));
    }
}

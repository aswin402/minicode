//! Native Rust dead-code & duplicate-block detector (`DeadCodeKind::UnusedExport`,
//! `DeadCodeKind::UnusedFile`, `DeadCodeKind::DuplicateCode`) with zero external tool dependencies.

use crate::config::DoctorConfig;
use crate::detector::LoadedFile;
use crate::types::{DeadCodeKind, Diagnostic, DiagnosticCategory, Severity};
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};

const MAX_DEAD_EXPORTS_REPORTED: usize = 20;
const MAX_DUPLICATES_REPORTED: usize = 12;
const DUPLICATE_WINDOW_LINES: usize = 12;

/// Extracts exported JS/TS symbol names and 1-indexed declaration lines using Tree-sitter AST
/// (handles multiline signatures, generics, re-exports, and ignores strings/comments cleanly).
fn extract_js_ts_exports_ast(ext: &str, content: &str) -> Vec<(usize, String)> {
    let mut parser = tree_sitter::Parser::new();
    let lang = match ext {
        "tsx" | "jsx" => tree_sitter_typescript::LANGUAGE_TSX.into(),
        "ts" | "mts" => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        _ => tree_sitter_javascript::LANGUAGE.into(),
    };
    if parser.set_language(&lang).is_err() {
        return Vec::new();
    }
    let Some(tree) = parser.parse(content, None) else {
        return Vec::new();
    };
    let bytes = content.as_bytes();
    let mut out = Vec::new();
    let root = tree.root_node();
    let mut cursor = root.walk();

    for child in root.children(&mut cursor) {
        if child.kind() != "export_statement" {
            continue;
        }
        let Some(decl) = child.child_by_field_name("declaration") else {
            continue;
        };
        match decl.kind() {
            "function_declaration"
            | "generator_function_declaration"
            | "class_declaration"
            | "abstract_class_declaration"
            | "interface_declaration"
            | "type_alias_declaration"
            | "enum_declaration" => {
                if let Some(name_node) = decl.child_by_field_name("name") {
                    if let Ok(name) = name_node.utf8_text(bytes) {
                        if name.len() >= 4 {
                            out.push((name_node.start_position().row + 1, name.to_string()));
                        }
                    }
                }
            }
            "lexical_declaration" | "variable_declaration" => {
                let mut dcur = decl.walk();
                for sub in decl.children(&mut dcur) {
                    if sub.kind() == "variable_declarator" {
                        if let Some(name_node) = sub.child_by_field_name("name") {
                            if name_node.kind() == "identifier" {
                                if let Ok(name) = name_node.utf8_text(bytes) {
                                    if name.len() >= 4 {
                                        out.push((
                                            name_node.start_position().row + 1,
                                            name.to_string(),
                                        ));
                                    }
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// Extracts undecorated top-level Python `def`/`class` definitions using Tree-sitter AST
/// (automatically skipping `decorated_definition` routes/CLI handlers and private `_`/`test_` names).
fn extract_py_top_defs_ast(content: &str) -> Vec<(usize, String)> {
    let mut parser = tree_sitter::Parser::new();
    if parser
        .set_language(&tree_sitter_python::LANGUAGE.into())
        .is_err()
    {
        return Vec::new();
    }
    let Some(tree) = parser.parse(content, None) else {
        return Vec::new();
    };
    let bytes = content.as_bytes();
    let mut out = Vec::new();
    let root = tree.root_node();
    let mut cursor = root.walk();

    for child in root.children(&mut cursor) {
        // Decorated functions/classes appear as `decorated_definition`, so matching only
        // direct `function_definition` / `class_definition` skips FastAPI/Click decorators cleanly.
        if matches!(child.kind(), "function_definition" | "class_definition") {
            if let Some(name_node) = child.child_by_field_name("name") {
                if let Ok(name) = name_node.utf8_text(bytes) {
                    if name.len() >= 4 && !name.starts_with("test_") && !name.starts_with('_') {
                        out.push((name_node.start_position().row + 1, name.to_string()));
                    }
                }
            }
        }
    }
    out
}

fn is_entrypoint_file(rel_path: &str) -> bool {
    let lower = rel_path.to_ascii_lowercase();
    let filename = lower.rsplit('/').next().unwrap_or(&lower);
    let stem = filename.split('.').next().unwrap_or("");
    // Ambient TypeScript declarations and framework/build/linter/test configs
    // (e.g. `vite.config.ts`, `tailwind.config.js`, `eslint.config.mjs`, `vite-env.d.ts`)
    // are consumed by tooling rather than imported by application source files.
    if filename.ends_with(".d.ts")
        || filename.contains(".config.")
        || filename.starts_with("tsconfig")
        || filename.starts_with("jsconfig")
    {
        return true;
    }
    matches!(
        stem,
        "main"
            | "lib"
            | "mod"
            | "index"
            | "app"
            | "page"
            | "layout"
            | "route"
            | "loading"
            | "error"
            | "not-found"
            | "middleware"
            | "server"
            | "cli"
            | "__init__"
            | "__main__"
            | "conftest"
            | "setup"
            | "manage"
            | "wsgi"
            | "asgi"
            | "settings"
            | "build"
    ) || lower.contains("/routes/")
        || lower.contains("/pages/")
        || lower.contains("/app/")
        || lower.contains("/bin/")
        || lower.contains("/api/")
        || lower.contains("/migrations/")
}

/// Runs lightweight cross-file unused-export and duplicate-block detection over scanned workspace files.
#[must_use]
pub fn detect_dead_code_and_duplicates(
    files: &[LoadedFile],
    cfg: &DoctorConfig,
) -> Vec<Diagnostic> {
    if files.len() < 2 {
        return Vec::new();
    }

    let mut out = Vec::new();

    // 1. Build a fast identifier frequency table across all files (count how many distinct files mention each token)
    let mut token_file_count: HashMap<String, usize> = HashMap::new();
    let mut file_tokens_cache: Vec<HashSet<String>> = Vec::with_capacity(files.len());

    for f in files {
        let mut uniq = HashSet::new();
        for tok in f
            .content
            .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
            .filter(|t| t.len() >= 4)
        {
            uniq.insert(tok.to_string());
        }
        for t in &uniq {
            *token_file_count.entry(t.clone()).or_insert(0) += 1;
        }
        file_tokens_cache.push(uniq);
    }

    // 2. Unused exports (JS/TS `export ...` and Python module-level `def/class` via Tree-sitter AST)
    if !cfg.is_rule_ignored("dead/unused-export") {
        let mut reported = 0usize;

        for f in files {
            if reported >= MAX_DEAD_EXPORTS_REPORTED || f.is_test || is_entrypoint_file(&f.rel_path)
            {
                continue;
            }
            let candidates: Vec<(usize, String)> = match f.ext.as_str() {
                "ts" | "tsx" | "js" | "jsx" | "mts" | "mjs" => {
                    extract_js_ts_exports_ast(&f.ext, &f.content)
                }
                "py" => extract_py_top_defs_ast(&f.content),
                _ => Vec::new(),
            };

            for (line, sym) in candidates {
                if reported >= MAX_DEAD_EXPORTS_REPORTED {
                    break;
                }
                let file_mentions = token_file_count.get(&sym).copied().unwrap_or(0);
                if file_mentions <= 1 {
                    reported += 1;
                    out.push(Diagnostic {
                        rule: "dead/unused-export".to_string(),
                        severity: cfg.effective_severity("dead/unused-export", Severity::Warning),
                        category: DiagnosticCategory::DeadCode,
                        dead_code_kind: Some(DeadCodeKind::UnusedExport),
                        file: f.rel_path.clone(),
                        line,
                        column: 1,
                        message: format!(
                            "Exported symbol `{sym}` is never referenced outside `{}`",
                            f.rel_path
                        ),
                        suggestion: format!(
                            "Remove unused `{sym}` or make it module-private if only used internally."
                        ),
                        snippet: f
                            .content
                            .lines()
                            .nth(line.saturating_sub(1))
                            .map(|l| l.trim().chars().take(120).collect()),
                    });
                }
            }
        }
    }

    // 3. Unused non-entrypoint files (module stem never referenced in any other scanned file)
    if !cfg.is_rule_ignored("dead/unused-file") && files.len() >= 4 {
        let mut reported_files = 0usize;
        for (idx, f) in files.iter().enumerate() {
            if reported_files >= 10 || f.is_test || is_entrypoint_file(&f.rel_path) {
                continue;
            }
            if !matches!(f.ext.as_str(), "ts" | "tsx" | "js" | "jsx" | "py" | "dart") {
                continue;
            }
            let stem = f
                .rel_path
                .rsplit('/')
                .next()
                .unwrap_or("")
                .split('.')
                .next()
                .unwrap_or("");
            if stem.len() < 4 {
                continue;
            }
            let referenced_elsewhere = files.iter().enumerate().any(|(other_idx, other)| {
                if other_idx == idx {
                    return false;
                }
                file_tokens_cache[other_idx].contains(stem)
                    || other.content.contains(&format!("/{stem}"))
                    || other.content.contains(&format!(".{stem}"))
            });
            if !referenced_elsewhere {
                reported_files += 1;
                out.push(Diagnostic {
                    rule: "dead/unused-file".to_string(),
                    severity: cfg.effective_severity("dead/unused-file", Severity::Warning),
                    category: DiagnosticCategory::DeadCode,
                    dead_code_kind: Some(DeadCodeKind::UnusedFile),
                    file: f.rel_path.clone(),
                    line: 1,
                    column: 1,
                    message: format!(
                        "Source file `{}` appears unreferenced across the workspace",
                        f.rel_path
                    ),
                    suggestion:
                        "Remove this obsolete file or wire it into an active module/router."
                            .to_string(),
                    snippet: None,
                });
            }
        }
    }

    // 4. Duplicate code blocks (12+ normalized non-trivial lines repeated across 2+ distinct files)
    if !cfg.is_rule_ignored("dead/duplicate-block") {
        let mut block_origins: HashMap<u64, (String, usize)> = HashMap::new();
        let mut reported_pairs: HashSet<(String, String)> = HashSet::new();
        let mut dup_count = 0usize;

        for f in files {
            if dup_count >= MAX_DUPLICATES_REPORTED
                || f.is_test
                || f.line_count < DUPLICATE_WINDOW_LINES
            {
                continue;
            }
            let norm_lines: Vec<(usize, String)> = f
                .content
                .lines()
                .enumerate()
                .filter_map(|(i, l)| {
                    let t = l.trim();
                    if t.len() < 6
                        || t.starts_with("//")
                        || t.starts_with('#')
                        || t.starts_with("import ")
                        || t.starts_with("use ")
                        || t.starts_with("from ")
                    {
                        None
                    } else {
                        Some((i + 1, t.split_whitespace().collect::<Vec<_>>().join(" ")))
                    }
                })
                .collect();

            if norm_lines.len() < DUPLICATE_WINDOW_LINES {
                continue;
            }

            for window in norm_lines.windows(DUPLICATE_WINDOW_LINES).step_by(4) {
                if dup_count >= MAX_DUPLICATES_REPORTED {
                    break;
                }
                let mut hasher = std::collections::hash_map::DefaultHasher::new();
                for (_, text) in window {
                    text.hash(&mut hasher);
                }
                let h = hasher.finish();
                let start_line = window[0].0;

                if let Some((orig_file, orig_line)) = block_origins.get(&h) {
                    if orig_file != &f.rel_path {
                        let pair = if orig_file < &f.rel_path {
                            (orig_file.clone(), f.rel_path.clone())
                        } else {
                            (f.rel_path.clone(), orig_file.clone())
                        };
                        if reported_pairs.insert(pair) {
                            dup_count += 1;
                            out.push(Diagnostic {
                                rule: "dead/duplicate-block".to_string(),
                                severity: cfg.effective_severity("dead/duplicate-block", Severity::Warning),
                                category: DiagnosticCategory::DeadCode,
                                dead_code_kind: Some(DeadCodeKind::DuplicateCode),
                                file: f.rel_path.clone(),
                                line: start_line,
                                column: 1,
                                message: format!(
                                    "Duplicated {DUPLICATE_WINDOW_LINES}+ line block also appears in `{orig_file}:{orig_line}`"
                                ),
                                suggestion: format!(
                                    "Extract the duplicated block shared with `{orig_file}:{orig_line}` into a shared helper."
                                ),
                                snippet: Some(window[0].1.chars().take(120).collect()),
                            });
                        }
                    }
                } else {
                    block_origins.insert(h, (f.rel_path.clone(), start_line));
                }
            }
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{run_doctor, DoctorOptions, ScanScope};

    #[test]
    fn test_config_and_ambient_files_are_entrypoints_not_dead_code() {
        for p in [
            "web/vite.config.ts",
            "tailwind.config.js",
            "eslint.config.mjs",
            "src/vite-env.d.ts",
            "tsconfig.json",
            "app/manage.py",
        ] {
            assert!(
                is_entrypoint_file(p),
                "Expected tooling/config file `{p}` to be treated as an entrypoint"
            );
        }
        assert!(!is_entrypoint_file("src/utils/obsolete_helper.ts"));
    }

    #[test]
    fn test_python_shell_true_and_react_useeffect_missing_deps_rules() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("app")).unwrap();
        std::fs::create_dir_all(root.join("web/src")).unwrap();
        std::fs::write(root.join("requirements.txt"), "fastapi>=0.115.0\n").unwrap();
        std::fs::write(
            root.join("app/main.py"),
            "import subprocess\n\ndef run_shell(cmd: str):\n    return subprocess.run(cmd, shell=True)\n",
        )
        .unwrap();
        std::fs::write(
            root.join("web/package.json"),
            "{\"name\":\"t\",\"dependencies\":{\"react\":\"^19.0.0\"}}\n",
        )
        .unwrap();
        std::fs::write(
            root.join("web/vite.config.ts"),
            "export default { server: { port: 5173 } };\n",
        )
        .unwrap();
        std::fs::write(
            root.join("web/src/App.tsx"),
            "import { useEffect, useState } from 'react';\nexport default function App() {\n  const [items, setItems] = useState([]);\n  useEffect(() => { fetch('/api').then(r => r.json()).then(setItems); });\n  return <div>{items.length}</div>;\n}\n",
        )
        .unwrap();
        std::fs::write(root.join("web/src/extra1.ts"), "export const a = 1;\n").unwrap();
        std::fs::write(
            root.join("web/src/extra2.ts"),
            "import { a } from './extra1'; export const b = a + 1;\n",
        )
        .unwrap();

        let opts = DoctorOptions {
            scope: ScanScope::Full,
            write_workspace_artifacts: false,
            ..DoctorOptions::default()
        };
        let report = run_doctor(root, &opts).unwrap();
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.rule == "py/subprocess-shell-true"),
            "Expected py/subprocess-shell-true in {:?}",
            report.diagnostics
        );
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.rule == "react/useeffect-missing-deps"),
            "Expected react/useeffect-missing-deps in {:?}",
            report.diagnostics
        );
        assert!(
            !report
                .diagnostics
                .iter()
                .any(|d| d.rule == "dead/unused-file" && d.file.ends_with("vite.config.ts")),
            "vite.config.ts must never be flagged as dead/unused-file: {:?}",
            report.diagnostics
        );
    }
}

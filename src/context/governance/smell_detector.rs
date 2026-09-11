use crate::context::graph::CodeGraph;
use crate::context::repomap::{RepoMapExtractor, SymbolDef};
use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Classification of structural AST code smells and anti-patterns
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum SmellCategory {
    /// Function or method exceeds recommended length (>80 lines)
    GodFunction,
    /// Function accepts an excessive number of parameters (>=6 arguments)
    ExcessiveParameters,
    /// Control flow contains deeply nested blocks (>=5 nesting levels)
    DeepNesting,
    /// Public symbol is exported but has zero callers across the codebase graph
    DeadExport,
    /// Complex multi-clause boolean conditional (>=4 operators)
    ComplexBoolean,
}

impl SmellCategory {
    pub fn badge(&self) -> &'static str {
        match self {
            Self::GodFunction => "🚨 God Function",
            Self::ExcessiveParameters => "⚠️ Excessive Parameters",
            Self::DeepNesting => "⚠️ Deep Nesting",
            Self::DeadExport => "🔍 Dead Public Export",
            Self::ComplexBoolean => "⚠️ Complex Boolean Logic",
        }
    }
}

/// A specific detected code smell instance
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CodeSmell {
    pub id: String,
    pub category: SmellCategory,
    pub severity: String, // "ERROR" | "WARNING" | "INFO"
    pub symbol_name: Option<String>,
    pub file_path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub message: String,
    pub remediation: String,
}

/// Comprehensive codebase or file code health report
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SmellReport {
    pub files_scanned: usize,
    pub total_smells: usize,
    pub health_score: u32,
    pub smells: Vec<CodeSmell>,
}

pub struct AstSmellDetector;

impl AstSmellDetector {
    /// Scans workspace or a specific target file for AST smells and anti-patterns
    pub fn scan_workspace(
        workspace_root: &Path,
        graph: Option<&CodeGraph>,
        target_file: Option<&str>,
    ) -> Result<SmellReport> {
        let mut extractor = RepoMapExtractor::new();
        let mut all_smells = Vec::new();
        let mut files_scanned = 0;

        let files = if let Some(target) = target_file {
            let p = if Path::new(target).is_absolute() {
                PathBuf::from(target)
            } else {
                workspace_root.join(target)
            };
            if p.exists() {
                vec![p]
            } else {
                Vec::new()
            }
        } else {
            Self::discover_source_files(workspace_root)
        };

        for file_path in &files {
            if let Ok(content) = fs::read_to_string(file_path) {
                files_scanned += 1;
                let rel_path = file_path.strip_prefix(workspace_root).unwrap_or(file_path);

                let symbols = extractor
                    .extract_file_symbols(file_path)
                    .unwrap_or_default();

                let smells = Self::scan_code(rel_path, &content, &symbols, graph);
                all_smells.extend(smells);
            }
        }

        // Calculate health score: 100 base, -8 per ERROR, -3 per WARNING, -1 per INFO
        let mut penalty = 0_u32;
        for s in &all_smells {
            match s.severity.as_str() {
                "ERROR" => penalty = penalty.saturating_add(8),
                "WARNING" => penalty = penalty.saturating_add(3),
                _ => penalty = penalty.saturating_add(1),
            }
        }

        let health_score = 100_u32.saturating_sub(penalty);

        Ok(SmellReport {
            files_scanned,
            total_smells: all_smells.len(),
            health_score,
            smells: all_smells,
        })
    }

    /// Scans a single source file content and its extracted symbols for anti-patterns
    pub fn scan_code(
        file_path: &Path,
        content: &str,
        symbols: &[SymbolDef],
        graph: Option<&CodeGraph>,
    ) -> Vec<CodeSmell> {
        let mut smells = Vec::new();
        let path_display = file_path.display().to_string();
        let lines: Vec<&str> = content.lines().collect();

        // 1. Symbol-level AST inspections
        for sym in symbols {
            let line_count = sym.end_line.saturating_sub(sym.line_number) + 1;

            // (a) God Function / Method (> 80 lines)
            if sym.kind == "function" || sym.kind == "method" {
                if line_count > 80 {
                    smells.push(CodeSmell {
                        id: format!("god-fn-{}", uuid::Uuid::new_v4().simple()),
                        category: SmellCategory::GodFunction,
                        severity: if line_count > 150 { "ERROR" } else { "WARNING" }.to_string(),
                        symbol_name: Some(sym.name.clone()),
                        file_path: path_display.clone(),
                        start_line: sym.line_number,
                        end_line: sym.end_line,
                        message: format!(
                            "Function `{}` spans {} lines (threshold: 80 lines)",
                            sym.name, line_count
                        ),
                        remediation: "Decompose into smaller single-responsibility helper functions or state machine stages.".to_string(),
                    });
                }

                // (b) Excessive Parameter List (>= 6 arguments)
                let param_count = Self::count_parameters(&sym.signature);
                if param_count >= 6 {
                    smells.push(CodeSmell {
                        id: format!("params-{}", uuid::Uuid::new_v4().simple()),
                        category: SmellCategory::ExcessiveParameters,
                        severity: "WARNING".to_string(),
                        symbol_name: Some(sym.name.clone()),
                        file_path: path_display.clone(),
                        start_line: sym.line_number,
                        end_line: sym.line_number,
                        message: format!(
                            "Function `{}` takes {} parameters (threshold: 5)",
                            sym.name, param_count
                        ),
                        remediation: "Bundle related parameters into a dedicated Options or Config struct/context object.".to_string(),
                    });
                }
            }

            // (c) Dead Public Export Detection (via CodeGraph)
            if let Some(cg) = graph {
                if sym.signature.starts_with("pub ")
                    && sym.name != "main"
                    && sym.name != "new"
                    && sym.name != "default"
                    && !path_display.contains("test")
                {
                    if let Ok(blast) = cg.get_blast_radius(&sym.name, file_path) {
                        if blast.direct_caller_symbols.is_empty()
                            && blast.direct_dependents.is_empty()
                        {
                            smells.push(CodeSmell {
                                id: format!("dead-export-{}", uuid::Uuid::new_v4().simple()),
                                category: SmellCategory::DeadExport,
                                severity: "INFO".to_string(),
                                symbol_name: Some(sym.name.clone()),
                                file_path: path_display.clone(),
                                start_line: sym.line_number,
                                end_line: sym.end_line,
                                message: format!(
                                    "Exported public symbol `{}` has 0 callers across the repository graph",
                                    sym.name
                                ),
                                remediation: "Verify if this symbol is part of the external public crate API, or reduce visibility (e.g. `pub(crate)` / private).".to_string(),
                            });
                        }
                    }
                }
            }
        }

        // 2. Line-level inspections (Deep Nesting & Complex Boolean Logic)
        for (idx, line) in lines.iter().enumerate() {
            let line_no = idx + 1;
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with('#') {
                continue;
            }

            // (d) Deep Nesting (>= 5 levels: 20 spaces or 5 tabs)
            let indent_spaces = line.chars().take_while(|&c| c == ' ').count();
            let indent_tabs = line.chars().take_while(|&c| c == '\t').count();
            let nesting_level = indent_tabs + (indent_spaces / 4);

            if nesting_level >= 5 {
                smells.push(CodeSmell {
                    id: format!("nesting-{}", uuid::Uuid::new_v4().simple()),
                    category: SmellCategory::DeepNesting,
                    severity: "WARNING".to_string(),
                    symbol_name: None,
                    file_path: path_display.clone(),
                    start_line: line_no,
                    end_line: line_no,
                    message: format!(
                        "Deep nesting level {} detected at line {}",
                        nesting_level, line_no
                    ),
                    remediation: "Use early returns / guard clauses or extract nested inner loop/match into a helper method.".to_string(),
                });
            }

            // (e) Complex Boolean Expression (>= 4 logical operators)
            let and_count = trimmed.matches("&&").count();
            let or_count = trimmed.matches("||").count();
            let bool_ops = and_count + or_count;

            if bool_ops >= 4 && (trimmed.starts_with("if ") || trimmed.starts_with("while ")) {
                smells.push(CodeSmell {
                    id: format!("bool-{}", uuid::Uuid::new_v4().simple()),
                    category: SmellCategory::ComplexBoolean,
                    severity: "WARNING".to_string(),
                    symbol_name: None,
                    file_path: path_display.clone(),
                    start_line: line_no,
                    end_line: line_no,
                    message: format!(
                        "Complex boolean expression with {} logical operators at line {}",
                        bool_ops, line_no
                    ),
                    remediation: "Extract condition clauses into well-named boolean helper variables or methods.".to_string(),
                });
            }
        }

        smells
    }

    fn count_parameters(signature: &str) -> usize {
        let Some(start_paren) = signature.find('(') else {
            return 0;
        };
        let Some(end_paren) = signature.rfind(')') else {
            return 0;
        };
        if end_paren <= start_paren + 1 {
            return 0;
        }

        let inner = &signature[start_paren + 1..end_paren].trim();
        if inner.is_empty() {
            return 0;
        }

        inner
            .split(',')
            .filter(|p| {
                let t = p.trim();
                !t.is_empty() && t != "&self" && t != "&mut self" && t != "self"
            })
            .count()
    }

    fn discover_source_files(root: &Path) -> Vec<PathBuf> {
        let mut results = Vec::new();
        let src_dir = root.join("src");
        if src_dir.exists() {
            let walker = ignore::WalkBuilder::new(&src_dir).build();
            for entry in walker.flatten() {
                let p = entry.path();
                if p.is_file() {
                    if let Some(ext) = p.extension() {
                        if ext == "rs" || ext == "ts" || ext == "js" || ext == "py" {
                            results.push(p.to_path_buf());
                        }
                    }
                }
            }
        }
        results
    }

    /// Formats code smell report into clean, user-friendly markdown
    pub fn format_markdown(report: &SmellReport, target_hint: Option<&str>) -> String {
        let scope_str = target_hint
            .map(|t| format!(" for `{}`", t))
            .unwrap_or_default();

        let mut out = format!(
            "### 🧹 AST Code Smells & Architectural Health Scorecard{}\n\n\
            • **Code Health Score**: `{}/100`\n\
            • **Files Scanned**: {}\n\
            • **Total Smells Detected**: {}\n\n",
            scope_str, report.health_score, report.files_scanned, report.total_smells
        );

        if report.smells.is_empty() {
            out.push_str("✨ **No code smells detected!** The codebase adheres cleanly to structural and complexity best practices.\n");
            return out;
        }

        out.push_str("#### 🔍 Detected Anti-Patterns & Remediations\n\n");
        for smell in report.smells.iter().take(15) {
            let sym_hint = smell
                .symbol_name
                .as_deref()
                .map(|s| format!(" in `{}`", s))
                .unwrap_or_default();

            out.push_str(&format!(
                "- **{}** (`{}`): {}`{}` (lines {}-{}):\n  • *Issue:* {}\n  • *Fix:* {}\n\n",
                smell.category.badge(),
                smell.severity,
                path_or_filename(&smell.file_path),
                sym_hint,
                smell.start_line,
                smell.end_line,
                smell.message,
                smell.remediation
            ));
        }

        if report.smells.len() > 15 {
            out.push_str(&format!(
                "*... and {} more findings omitted for brevity.*\n",
                report.smells.len() - 15
            ));
        }

        out
    }
}

fn path_or_filename(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string())
}

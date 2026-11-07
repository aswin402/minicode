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
    /// Intentional YAGNI shortcut comment (`shortcut: <ceiling>, <upgrade trigger>`) harvested via Tree-sitter AST
    ShortcutDebt,
}

/// Structured entry in the Ponytail-inspired `shortcut:` technical debt ledger.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ShortcutDebtItem {
    pub file_path: String,
    pub line: usize,
    pub ceiling: String,
    pub upgrade_trigger: Option<String>,
    pub has_trigger: bool,
}

impl SmellCategory {
    pub fn badge(&self) -> &'static str {
        match self {
            Self::GodFunction => "🚨 God Function",
            Self::ExcessiveParameters => "⚠️ Excessive Parameters",
            Self::DeepNesting => "⚠️ Deep Nesting",
            Self::DeadExport => "🔍 Dead Public Export",
            Self::ComplexBoolean => "⚠️ Complex Boolean Logic",
            Self::ShortcutDebt => "📌 Shortcut Debt",
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

        // Calculate health score: 100 base, -8 per ERROR, -3 per WARNING, -1 per INFO (0 for tracked INFO shortcuts)
        let mut penalty = 0_u32;
        for s in &all_smells {
            match s.severity.as_str() {
                "ERROR" => penalty = penalty.saturating_add(8),
                "WARNING" => penalty = penalty.saturating_add(3),
                _ if s.category == SmellCategory::ShortcutDebt => {}
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

        let is_ui_component_file = matches!(
            file_path.extension().and_then(|e| e.to_str()),
            Some("tsx" | "jsx" | "vue" | "svelte")
        );
        let god_fn_threshold = if is_ui_component_file { 180 } else { 80 };
        let god_fn_error_threshold = if is_ui_component_file { 280 } else { 150 };

        // 1. Symbol-level AST inspections
        for sym in symbols {
            let line_count = sym.end_line.saturating_sub(sym.line_number) + 1;

            // (a) God Function / Method (> 80 lines for imperative code, > 180 lines for declarative UI components)
            if sym.kind == "function" || sym.kind == "method" {
                if line_count > god_fn_threshold {
                    smells.push(CodeSmell {
                        id: format!("god-fn-{}", uuid::Uuid::new_v4().simple()),
                        category: SmellCategory::GodFunction,
                        severity: if line_count > god_fn_error_threshold {
                            "ERROR"
                        } else {
                            "WARNING"
                        }
                        .to_string(),
                        symbol_name: Some(sym.name.clone()),
                        file_path: path_display.clone(),
                        start_line: sym.line_number,
                        end_line: sym.end_line,
                        message: format!(
                            "Function `{}` spans {} lines (threshold: {} lines)",
                            sym.name, line_count, god_fn_threshold
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
                    && !matches!(sym.name.as_str(), "main" | "new" | "default")
                    && !path_display.contains("test")
                {
                    if let Some(indices) = cg.name_to_nodes().get(&sym.name) {
                        let has_callers = indices.iter().any(|&idx| {
                            cg.graph()
                                .neighbors_directed(idx, petgraph::Direction::Incoming)
                                .any(|caller| {
                                    cg.graph().node_weight(caller).is_some_and(|w| {
                                        w.kind != crate::context::graph::SymbolKind::File
                                    })
                                })
                        });
                        if !has_callers {
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
        let mut last_nesting_line: usize = 0;
        let mut nesting_findings_in_file: usize = 0;

        for (idx, line) in lines.iter().enumerate() {
            let line_no = idx + 1;
            let trimmed = line.trim();
            if trimmed.is_empty()
                || trimmed.starts_with("//")
                || trimmed.starts_with('#')
                || trimmed.starts_with("/*")
                || trimmed.starts_with('*')
            {
                continue;
            }

            // (d) Deep Nesting (>= 5 levels: 20 spaces or 5 tabs) on control-flow branching statements
            // Ignore declarative JSX/HTML/SVG markup tags (`<div`, `</div`, `/>`) and coalesce contiguous blocks.
            let is_control_flow = trimmed.starts_with("if ")
                || trimmed.starts_with("if(")
                || trimmed.starts_with("else if ")
                || trimmed.starts_with("for ")
                || trimmed.starts_with("for(")
                || trimmed.starts_with("while ")
                || trimmed.starts_with("while(")
                || trimmed.starts_with("match ")
                || trimmed.starts_with("switch ")
                || trimmed.starts_with("switch(");

            if is_control_flow && nesting_findings_in_file < 3 {
                let indent_spaces = line.chars().take_while(|&c| c == ' ').count();
                let indent_tabs = line.chars().take_while(|&c| c == '\t').count();
                let nesting_level = indent_tabs + (indent_spaces / 4);

                if nesting_level >= 5
                    && (last_nesting_line == 0 || line_no > last_nesting_line + 10)
                {
                    last_nesting_line = line_no;
                    nesting_findings_in_file += 1;
                    smells.push(CodeSmell {
                        id: format!("nesting-{}", uuid::Uuid::new_v4().simple()),
                        category: SmellCategory::DeepNesting,
                        severity: "WARNING".to_string(),
                        symbol_name: None,
                        file_path: path_display.clone(),
                        start_line: line_no,
                        end_line: line_no,
                        message: format!(
                            "Deep control-flow nesting level {} detected at line {}",
                            nesting_level, line_no
                        ),
                        remediation: "Use early returns / guard clauses or extract nested inner loop/match into a helper method.".to_string(),
                    });
                }
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

        // (f) Tree-sitter AST `shortcut:` / `ponytail:` Technical Debt Ledger
        for shortcut in Self::harvest_shortcut_comments_ast(file_path, content) {
            if !shortcut.has_trigger {
                smells.push(CodeSmell {
                    id: format!("shortcut-{}", uuid::Uuid::new_v4().simple()),
                    category: SmellCategory::ShortcutDebt,
                    severity: "WARNING".to_string(),
                    symbol_name: None,
                    file_path: path_display.clone(),
                    start_line: shortcut.line,
                    end_line: shortcut.line,
                    message: format!(
                        "[no-trigger] Shortcut comment at line {} lacks an upgrade trigger (`{}`)",
                        shortcut.line, shortcut.ceiling
                    ),
                    remediation: "Format as `// shortcut: <ceiling>, <when to upgrade>` so the intentional shortcut has a concrete replacement condition instead of rotting.".to_string(),
                });
            } else {
                let trigger_desc = shortcut
                    .upgrade_trigger
                    .as_deref()
                    .unwrap_or("trigger condition specified inline");
                smells.push(CodeSmell {
                    id: format!("shortcut-{}", uuid::Uuid::new_v4().simple()),
                    category: SmellCategory::ShortcutDebt,
                    severity: "INFO".to_string(),
                    symbol_name: None,
                    file_path: path_display.clone(),
                    start_line: shortcut.line,
                    end_line: shortcut.line,
                    message: format!(
                        "Tracked shortcut ceiling `{}` (upgrade trigger: `{}`)",
                        shortcut.ceiling, trigger_desc
                    ),
                    remediation: "Intentional YAGNI shortcut tracked in ledger; upgrade only when the trigger condition is reached.".to_string(),
                });
            }
        }

        smells
    }

    /// Walks Tree-sitter AST comment nodes (`comment`, `line_comment`, `block_comment`) to harvest
    /// `shortcut: <ceiling>, <upgrade trigger>` (and `ponytail:`) debt markers with zero regexes.
    /// Ignores occurrences inside string literals or active code.
    #[must_use]
    pub fn harvest_shortcut_comments_ast(file_path: &Path, code: &str) -> Vec<ShortcutDebtItem> {
        let ext = file_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or_default();
        let path_display = file_path.to_string_lossy().to_string();
        let mut items = Vec::new();

        if let Some(lang) = crate::context::syntax_guard::SyntaxGuard::language_for_extension(ext) {
            let mut parser = tree_sitter::Parser::new();
            if parser.set_language(&lang).is_ok() {
                if let Some(tree) = parser.parse(code, None) {
                    collect_shortcut_comments_dfs(
                        tree.root_node(),
                        code,
                        &path_display,
                        &mut items,
                    );
                    return items;
                }
            }
        }

        items
    }

    fn count_parameters(signature: &str) -> usize {
        let Some(start_paren) = signature.find('(') else {
            return 0;
        };
        let rest = &signature[start_paren + 1..];
        let mut paren_depth = 0usize;
        let mut angle_depth = 0usize;
        let mut bracket_depth = 0usize;
        let mut params = Vec::new();
        let mut param_start = 0usize;
        let mut end_offset = None;

        for (i, ch) in rest.char_indices() {
            match ch {
                '(' => paren_depth += 1,
                ')' => {
                    if paren_depth == 0 {
                        end_offset = Some(i);
                        break;
                    }
                    paren_depth -= 1;
                }
                '<' => angle_depth += 1,
                '>' => angle_depth = angle_depth.saturating_sub(1),
                '[' => bracket_depth += 1,
                ']' => bracket_depth = bracket_depth.saturating_sub(1),
                ',' if paren_depth == 0 && angle_depth == 0 && bracket_depth == 0 => {
                    params.push(&rest[param_start..i]);
                    param_start = i + 1;
                }
                _ => {}
            }
        }

        let Some(end) = end_offset else {
            return 0;
        };
        params.push(&rest[param_start..end]);

        params
            .into_iter()
            .filter(|p| {
                let t = p.trim();
                !t.is_empty() && !matches!(t, "&self" | "&mut self" | "self" | "mut self")
            })
            .count()
    }

    fn discover_source_files(root: &Path) -> Vec<PathBuf> {
        crate::context::walker::WorkspaceWalker::new(root)
            .extensions(crate::constants::REFACTOR_SOURCE_EXTENSIONS)
            .collect_absolute_files()
    }

    /// Formats code smell report into clean, user-friendly markdown
    pub fn format_markdown(report: &SmellReport, target_hint: Option<&str>) -> String {
        let scope_str = target_hint
            .map(|t| format!(" for `{}`", t))
            .unwrap_or_default();

        let shortcut_total = report
            .smells
            .iter()
            .filter(|s| s.category == SmellCategory::ShortcutDebt)
            .count();
        let shortcut_untriggered = report
            .smells
            .iter()
            .filter(|s| s.category == SmellCategory::ShortcutDebt && s.severity != "INFO")
            .count();
        let shortcut_summary = if shortcut_total == 0 {
            "0 markers (clean ledger)".to_string()
        } else {
            format!(
                "{} marker(s), {} with [no-trigger] rot risk",
                shortcut_total, shortcut_untriggered
            )
        };

        let mut out = format!(
            "### 🧹 AST Code Smells & Architectural Health Scorecard{}\n\n\
            • **Code Health Score**: `{}/100`\n\
            • **Files Scanned**: {}\n\
            • **Total Smells Detected**: {}\n\
            • **Shortcut Debt Ledger (`shortcut:`)**: `{}`\n\n",
            scope_str,
            report.health_score,
            report.files_scanned,
            report.total_smells,
            shortcut_summary
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
                "- **{}** (`{}`): `{}`{} (lines {}-{}):\n  • *Issue:* {}\n  • *Fix:* {}\n\n",
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

fn collect_shortcut_comments_dfs(
    node: tree_sitter::Node<'_>,
    source: &str,
    file_path: &str,
    out: &mut Vec<ShortcutDebtItem>,
) {
    let kind = node.kind();
    if node.is_extra() || kind.contains("comment") {
        let Some(text) = source.get(node.start_byte()..node.end_byte()) else {
            return;
        };
        let base_line = node.start_position().row + 1;
        for (offset, raw_line) in text.lines().enumerate() {
            if let Some(item) = parse_shortcut_comment_line(file_path, base_line + offset, raw_line)
            {
                out.push(item);
            }
        }
        return;
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_shortcut_comments_dfs(child, source, file_path, out);
    }
}

fn parse_shortcut_comment_line(
    file_path: &str,
    line_no: usize,
    raw_line: &str,
) -> Option<ShortcutDebtItem> {
    let cleaned = raw_line
        .trim()
        .trim_start_matches("///")
        .trim_start_matches("//!")
        .trim_start_matches("//")
        .trim_start_matches("/*")
        .trim_end_matches("*/")
        .trim_start_matches('*')
        .trim_start_matches('#')
        .trim();
    if cleaned.is_empty() {
        return None;
    }

    let lower = cleaned.to_ascii_lowercase();
    let payload = crate::constants::SHORTCUT_DEBT_MARKERS
        .iter()
        .find_map(|marker| {
            lower
                .strip_prefix(marker)
                .map(|_| cleaned[marker.len()..].trim())
        })?;
    if payload.is_empty() {
        return None;
    }

    // Split on first comma or semicolon if present (`shortcut: <ceiling>, <when to upgrade>`)
    let (ceiling_raw, trigger_opt) = if let Some((left, right)) = payload.split_once([',', ';']) {
        let right_trimmed = right.trim();
        (
            left.trim(),
            if right_trimmed.is_empty() {
                None
            } else {
                Some(right_trimmed.to_string())
            },
        )
    } else {
        // Otherwise look for a trigger keyword boundary (`when`, `once`, `if`, `after`, `above`, `beyond`, `upgrade`, `until`)
        find_inline_trigger_split(payload)
    };

    let has_trigger = trigger_opt.is_some()
        || payload
            .split(|c: char| !c.is_ascii_alphanumeric())
            .any(|tok| {
                let t = tok.to_ascii_lowercase();
                crate::constants::SHORTCUT_TRIGGER_KEYWORDS.contains(&t.as_str())
            });

    Some(ShortcutDebtItem {
        file_path: file_path.to_string(),
        line: line_no,
        ceiling: ceiling_raw.to_string(),
        upgrade_trigger: trigger_opt,
        has_trigger,
    })
}

fn find_inline_trigger_split(payload: &str) -> (&str, Option<String>) {
    let lower = payload.to_ascii_lowercase();
    for kw in crate::constants::SHORTCUT_TRIGGER_KEYWORDS {
        let needle = format!(" {} ", kw);
        if let Some(idx) = lower.find(&needle) {
            let left = payload[..idx].trim();
            let right = payload[idx + 1..].trim();
            if !left.is_empty() && !right.is_empty() {
                return (left, Some(right.to_string()));
            }
        }
    }
    (payload.trim(), None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_harvest_shortcut_comments_ast_distinguishes_triggered_vs_untriggered_and_ignores_strings(
    ) {
        let rs_code = r#"
pub fn cache_lookup(key: &str) -> Option<String> {
    let fake_in_string = "// shortcut: string literal should not be harvested";
    // shortcut: linear scan over Vec, upgrade to HashMap when entries > 100
    // shortcut: hardcoded timeout
    let _ = fake_in_string;
    None
}
"#;
        let items = AstSmellDetector::harvest_shortcut_comments_ast(Path::new("cache.rs"), rs_code);
        assert_eq!(items.len(), 2);

        assert_eq!(items[0].line, 4);
        assert_eq!(items[0].ceiling, "linear scan over Vec");
        assert_eq!(
            items[0].upgrade_trigger.as_deref(),
            Some("upgrade to HashMap when entries > 100")
        );
        assert!(items[0].has_trigger);

        assert_eq!(items[1].line, 5);
        assert_eq!(items[1].ceiling, "hardcoded timeout");
        assert!(items[1].upgrade_trigger.is_none());
        assert!(!items[1].has_trigger);

        let smells = AstSmellDetector::scan_code(Path::new("cache.rs"), rs_code, &[], None);
        let shortcut_smells: Vec<_> = smells
            .iter()
            .filter(|s| s.category == SmellCategory::ShortcutDebt)
            .collect();
        assert_eq!(shortcut_smells.len(), 2);
        assert_eq!(shortcut_smells[0].severity, "INFO");
        assert_eq!(shortcut_smells[1].severity, "WARNING");
        assert!(shortcut_smells[1].message.contains("[no-trigger]"));
    }

    #[test]
    fn test_scan_code_ignores_jsx_markup_nesting_and_coalesces_control_flow() {
        let jsx_code = r#"
export function Hero() {
    return (
        <section>
            <div>
                <div>
                    <svg>
                        <path d="M0 0" />
                    </svg>
                </div>
            </div>
        </section>
    );
}
"#;
        let jsx_smells = AstSmellDetector::scan_code(Path::new("Hero.tsx"), jsx_code, &[], None);
        assert!(
            jsx_smells.is_empty(),
            "Declarative JSX markup must not trigger DeepNesting smells"
        );

        let deeply_nested_cf = "                    if condition_a {\n                        if condition_b {\n                            do_work();\n                        }\n                    }\n";
        let cf_smells =
            AstSmellDetector::scan_code(Path::new("logic.rs"), deeply_nested_cf, &[], None);
        assert_eq!(
            cf_smells.len(),
            1,
            "Contiguous deeply nested control flow should coalesce into 1 finding"
        );
        assert_eq!(cf_smells[0].category, SmellCategory::DeepNesting);
    }
}

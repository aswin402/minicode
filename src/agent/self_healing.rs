use crate::error::Result;
use crate::lsp::diagnostics::{DiagnosticItem, DiagnosticReport, FastCompilerChecker};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

/// High-level semantic classification of compiler/linter diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticCategory {
    /// Unresolved type, struct, module, or missing import (e.g. E0412, E0432, TS2304)
    MissingImport,
    /// Type mismatch, incompatible assignment, or return type difference (e.g. E0308, TS2322)
    TypeMismatch,
    /// Unimplemented trait methods or missing interface contracts (e.g. E0046, TS2420)
    MissingTraitImpl,
    /// Method or field not found on target struct/object (e.g. E0599, TS2339)
    UnresolvedMethodOrField,
    /// Syntax error, missing delimiter, or language version mismatch (e.g. E0658, TS1005)
    SyntaxError,
    /// Borrow checker, lifetime, or move semantics violations (e.g. E0382, E0502)
    BorrowOrLifetime,
    /// Other unclassified compiler diagnostics
    Other,
}

impl DiagnosticCategory {
    #[allow(dead_code)]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::MissingImport => "missing_import",
            Self::TypeMismatch => "type_mismatch",
            Self::MissingTraitImpl => "missing_trait_impl",
            Self::UnresolvedMethodOrField => "unresolved_method_or_field",
            Self::SyntaxError => "syntax_error",
            Self::BorrowOrLifetime => "borrow_or_lifetime",
            Self::Other => "other",
        }
    }
}

/// A cluster of related diagnostics sharing a root-cause file and symbol.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticCluster {
    pub file: PathBuf,
    pub category: DiagnosticCategory,
    pub root_symbol: Option<String>,
    pub primary_error: DiagnosticItem,
    pub cascading_errors: Vec<DiagnosticItem>,
    pub suggested_action: Option<String>,
}

/// Aggregated triage report grouping errors into primary root causes vs derivative cascading errors.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticTriageReport {
    pub total_errors: usize,
    pub total_warnings: usize,
    pub primary_clusters: Vec<DiagnosticCluster>,
    pub summary_text: String,
}

/// An individual repair action attempted by the self-healing engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelfHealingStep {
    pub attempt: usize,
    pub file: PathBuf,
    pub cluster_symbol: Option<String>,
    pub action_taken: String,
    pub errors_before: usize,
    pub errors_after: usize,
    pub success: bool,
}

/// Comprehensive outcome report of an autonomous self-healing pass.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelfHealingReport {
    pub initial_errors: usize,
    pub final_errors: usize,
    pub attempts_used: usize,
    pub max_attempts: usize,
    pub fully_resolved: bool,
    pub steps: Vec<SelfHealingStep>,
    pub remaining_diagnostics: Vec<DiagnosticItem>,
}

impl SelfHealingReport {
    /// Format a concise markdown report for the user or agent timeline.
    pub fn format_summary(&self, workspace_root: &Path) -> String {
        let mut out = String::new();
        out.push_str("### 🏥 Autonomous Diagnostic Self-Healing Report\n");

        if self.fully_resolved {
            out.push_str(&format!(
                "- **Status:** ✅ Clean (All {} initial error(s) resolved in {} attempt(s))\n",
                self.initial_errors, self.attempts_used
            ));
        } else {
            out.push_str(&format!(
                "- **Status:** ⚠️ {} error(s) remaining (reduced from {} in {} attempt(s))\n",
                self.final_errors, self.initial_errors, self.attempts_used
            ));
        }
        out.push_str(&format!(
            "- **Max Budget:** {} attempts allowed\n\n",
            self.max_attempts
        ));

        if !self.steps.is_empty() {
            out.push_str("#### Repair History\n");
            out.push_str("| # | File | Action | Errors Before → After | Outcome |\n");
            out.push_str("|---|------|--------|-----------------------|---------|\n");
            for step in &self.steps {
                let rel_file = step
                    .file
                    .strip_prefix(workspace_root)
                    .unwrap_or(&step.file)
                    .display();
                let outcome_icon = if step.success {
                    "✅ Success"
                } else {
                    "↩ Reverted"
                };
                out.push_str(&format!(
                    "| {} | `{}` | {} | {} → {} | {} |\n",
                    step.attempt,
                    rel_file,
                    step.action_taken,
                    step.errors_before,
                    step.errors_after,
                    outcome_icon
                ));
            }
            out.push('\n');
        }

        if !self.remaining_diagnostics.is_empty() {
            out.push_str("#### Remaining Diagnostics\n");
            for (idx, item) in self.remaining_diagnostics.iter().take(5).enumerate() {
                let rel = item
                    .file
                    .strip_prefix(workspace_root)
                    .unwrap_or(&item.file)
                    .display();
                let code_str = item
                    .code
                    .as_deref()
                    .map(|c| format!(" [{}]", c))
                    .unwrap_or_default();
                out.push_str(&format!(
                    "{}. `{}:{}:{}`{}: {}\n",
                    idx + 1,
                    rel,
                    item.line,
                    item.column,
                    code_str,
                    item.message
                ));
            }
            if self.remaining_diagnostics.len() > 5 {
                out.push_str(&format!(
                    "... and {} more diagnostic(s)\n",
                    self.remaining_diagnostics.len() - 5
                ));
            }
        }

        out
    }
}

/// Diagnostic Triage Engine analyzing, categorizing, and clustering compiler outputs.
pub struct DiagnosticTriageEngine;

impl DiagnosticTriageEngine {
    /// Categorizes a diagnostic item and attempts to extract the focal root symbol.
    pub fn categorize(item: &DiagnosticItem) -> (DiagnosticCategory, Option<String>) {
        let code = item.code.as_deref().unwrap_or("");
        let msg = &item.message;

        // 1. Rust E0412, E0432, E0433 (Missing type or import)
        if code == "E0412" || code == "E0433" || code == "E0425" {
            let symbol = Self::extract_quoted_symbol(msg);
            return (DiagnosticCategory::MissingImport, symbol);
        }

        if code == "E0432" {
            let symbol = Self::extract_unresolved_import(msg);
            return (DiagnosticCategory::MissingImport, symbol);
        }

        // 2. Rust E0599 (Unresolved method or field)
        if code == "E0599" {
            let symbol = Self::extract_method_name(msg);
            return (DiagnosticCategory::UnresolvedMethodOrField, symbol);
        }

        // 3. Rust E0308 (Type mismatch)
        if code == "E0308" {
            return (DiagnosticCategory::TypeMismatch, None);
        }

        // 4. Rust E0046 (Missing trait implementation)
        if code == "E0046" {
            let symbol = Self::extract_missing_trait_items(msg);
            return (DiagnosticCategory::MissingTraitImpl, symbol);
        }

        // 5. Rust Borrow / Lifetime errors
        if code == "E0382" || code == "E0502" || code == "E0505" || code == "E0507" {
            return (DiagnosticCategory::BorrowOrLifetime, None);
        }

        // TypeScript / General matches by message inspection
        if msg.contains("Cannot find name") || msg.contains("Cannot find module") {
            let symbol = Self::extract_single_quoted_symbol(msg);
            return (DiagnosticCategory::MissingImport, symbol);
        }

        if msg.contains("is not assignable to type") {
            return (DiagnosticCategory::TypeMismatch, None);
        }

        if msg.contains("does not exist on type") {
            let symbol = Self::extract_single_quoted_symbol(msg);
            return (DiagnosticCategory::UnresolvedMethodOrField, symbol);
        }

        if msg.contains("SyntaxError") || msg.contains("expected one of") {
            return (DiagnosticCategory::SyntaxError, None);
        }

        (DiagnosticCategory::Other, None)
    }

    fn extract_quoted_symbol(msg: &str) -> Option<String> {
        // e.g. "cannot find type `Foo` in this scope" -> "Foo"
        if let Some(start) = msg.find('`') {
            if let Some(end) = msg[start + 1..].find('`') {
                let sym = &msg[start + 1..start + 1 + end];
                if !sym.is_empty() && !sym.contains(' ') {
                    return Some(sym.to_string());
                }
            }
        }
        None
    }

    fn extract_single_quoted_symbol(msg: &str) -> Option<String> {
        // e.g. "Cannot find name 'Foo'" -> "Foo"
        if let Some(start) = msg.find('\'') {
            if let Some(end) = msg[start + 1..].find('\'') {
                let sym = &msg[start + 1..start + 1 + end];
                if !sym.is_empty() && !sym.contains(' ') {
                    return Some(sym.to_string());
                }
            }
        }
        None
    }

    fn extract_unresolved_import(msg: &str) -> Option<String> {
        // e.g. "unresolved import `crate::models::Foo`" -> "Foo"
        if let Some(sym) = Self::extract_quoted_symbol(msg) {
            if let Some(last) = sym.split("::").last() {
                return Some(last.to_string());
            }
            return Some(sym);
        }
        None
    }

    fn extract_method_name(msg: &str) -> Option<String> {
        // e.g. "no method named `format_summary` found for struct `DagExecutionReport` in the current scope"
        Self::extract_quoted_symbol(msg)
    }

    fn extract_missing_trait_items(msg: &str) -> Option<String> {
        // e.g. "not all trait items implemented, missing: `execute`"
        Self::extract_quoted_symbol(msg)
    }

    /// Triages a diagnostic report, clustering errors by root cause and separating primary from cascading.
    pub fn triage(report: &DiagnosticReport, workspace_root: &Path) -> DiagnosticTriageReport {
        let mut clusters: Vec<DiagnosticCluster> = Vec::new();
        let mut file_symbol_map: HashMap<(PathBuf, Option<String>), usize> = HashMap::new();

        for item in &report.errors {
            let (category, symbol) = Self::categorize(item);
            let key = (item.file.clone(), symbol.clone());

            if let Some(&cluster_idx) = file_symbol_map.get(&key) {
                // Add as cascading error to existing cluster
                if let Some(cluster) = clusters.get_mut(cluster_idx) {
                    cluster.cascading_errors.push(item.clone());
                }
            } else {
                // Create new primary cluster
                let mut suggested = None;
                if category == DiagnosticCategory::MissingImport {
                    if let Some(ref sym) = symbol {
                        if let Some(import_path) = Self::find_symbol_definition(workspace_root, sym)
                        {
                            suggested = Some(format!("Add import: `use {};`", import_path));
                        }
                    }
                } else if category == DiagnosticCategory::TypeMismatch {
                    if let Some(ref rend) = item.rendered {
                        if rend.contains("help: consider") || rend.contains("help: try") {
                            for line in rend.lines() {
                                if line.contains("help:") {
                                    suggested = Some(line.trim().to_string());
                                    break;
                                }
                            }
                        }
                    }
                }

                let cluster = DiagnosticCluster {
                    file: item.file.clone(),
                    category,
                    root_symbol: symbol.clone(),
                    primary_error: item.clone(),
                    cascading_errors: Vec::new(),
                    suggested_action: suggested,
                };

                let idx = clusters.len();
                file_symbol_map.insert(key, idx);
                clusters.push(cluster);
            }
        }

        // Prioritize clusters: Missing imports first, then Syntax, then others
        clusters.sort_by_key(|c| match c.category {
            DiagnosticCategory::MissingImport => 0,
            DiagnosticCategory::SyntaxError => 1,
            DiagnosticCategory::MissingTraitImpl => 2,
            DiagnosticCategory::UnresolvedMethodOrField => 3,
            DiagnosticCategory::TypeMismatch => 4,
            DiagnosticCategory::BorrowOrLifetime => 5,
            DiagnosticCategory::Other => 6,
        });

        let summary_text = format!(
            "Triaged {} compiler error(s) into {} primary root cause cluster(s) and {} warning(s).",
            report.errors.len(),
            clusters.len(),
            report.warnings.len()
        );

        DiagnosticTriageReport {
            total_errors: report.errors.len(),
            total_warnings: report.warnings.len(),
            primary_clusters: clusters,
            summary_text,
        }
    }

    /// Scans the workspace source files to discover candidate module export paths for a missing symbol.
    pub fn find_symbol_definition(workspace_root: &Path, symbol: &str) -> Option<String> {
        let src_dir = workspace_root.join("src");
        if !src_dir.exists() {
            return None;
        }

        let pattern = format!(
            r"\bpub\s+(struct|enum|trait|fn|type)\s+{}\b",
            regex::escape(symbol)
        );
        let re = match Regex::new(&pattern) {
            Ok(r) => r,
            Err(_) => return None,
        };

        let mut walker = ignore::WalkBuilder::new(&src_dir);
        walker.standard_filters(true);

        for entry in walker.build().flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("rs") {
                if let Ok(content) = std::fs::read_to_string(path) {
                    if re.is_match(&content) {
                        // Convert path to Rust module path: src/models/foo.rs -> crate::models::foo::Symbol
                        if let Ok(rel) = path.strip_prefix(&src_dir) {
                            let mut segments = vec!["crate".to_string()];
                            for comp in rel.iter() {
                                let comp_str = comp.to_string_lossy();
                                if comp_str.ends_with(".rs") {
                                    let stem = comp_str.trim_end_matches(".rs");
                                    if stem != "mod" && stem != "lib" && stem != "main" {
                                        segments.push(stem.to_string());
                                    }
                                } else {
                                    segments.push(comp_str.to_string());
                                }
                            }
                            segments.push(symbol.to_string());
                            return Some(segments.join("::"));
                        }
                    }
                }
            }
        }

        None
    }
}

/// Autonomous self-healing execution engine.
pub struct SelfHealingEngine;

impl SelfHealingEngine {
    /// Runs a bounded self-healing loop attempting deterministic repairs.
    pub async fn heal(
        workspace_root: &Path,
        max_attempts: usize,
        auto_apply_imports: bool,
        dry_run: bool,
    ) -> Result<SelfHealingReport> {
        let clamped_attempts = max_attempts.clamp(1, 5);

        // 1. Initial workspace diagnostics check
        let initial_report = FastCompilerChecker::check_workspace(workspace_root).await?;
        let initial_errors = initial_report.errors.len();

        if initial_report.is_clean() {
            return Ok(SelfHealingReport {
                initial_errors: 0,
                final_errors: 0,
                attempts_used: 0,
                max_attempts: clamped_attempts,
                fully_resolved: true,
                steps: Vec::new(),
                remaining_diagnostics: Vec::new(),
            });
        }

        if dry_run {
            let triage = DiagnosticTriageEngine::triage(&initial_report, workspace_root);
            let mut steps = Vec::new();
            for (idx, cluster) in triage.primary_clusters.iter().enumerate() {
                let action = cluster
                    .suggested_action
                    .clone()
                    .unwrap_or_else(|| format!("Manual review needed for {:?}", cluster.category));
                steps.push(SelfHealingStep {
                    attempt: idx + 1,
                    file: cluster.file.clone(),
                    cluster_symbol: cluster.root_symbol.clone(),
                    action_taken: format!("[Dry Run] {}", action),
                    errors_before: initial_errors,
                    errors_after: initial_errors,
                    success: false,
                });
            }

            return Ok(SelfHealingReport {
                initial_errors,
                final_errors: initial_errors,
                attempts_used: 0,
                max_attempts: clamped_attempts,
                fully_resolved: false,
                steps,
                remaining_diagnostics: initial_report.errors,
            });
        }

        let mut current_report = initial_report;
        let mut steps = Vec::new();
        let mut attempts = 0;

        while attempts < clamped_attempts && !current_report.is_clean() {
            attempts += 1;
            let triage = DiagnosticTriageEngine::triage(&current_report, workspace_root);

            // Find the first fixable candidate cluster
            let mut fix_applied = false;

            for cluster in &triage.primary_clusters {
                if cluster.category == DiagnosticCategory::MissingImport && auto_apply_imports {
                    if let Some(ref sym) = cluster.root_symbol {
                        if let Some(import_path) =
                            DiagnosticTriageEngine::find_symbol_definition(workspace_root, sym)
                        {
                            // Snapshot original file content for rollback safety
                            let orig_content = match std::fs::read_to_string(&cluster.file) {
                                Ok(c) => c,
                                Err(_) => continue,
                            };

                            let import_stmt = format!("use {};\n", import_path);

                            // Check if import already exists
                            if orig_content.contains(&format!("use {};", import_path)) {
                                continue;
                            }

                            // Insert import at top of file
                            let new_content = format!("{}{}", import_stmt, orig_content);
                            if std::fs::write(&cluster.file, new_content).is_err() {
                                continue;
                            }

                            // Re-evaluate compiler
                            let errors_before = current_report.errors.len();
                            let next_report =
                                FastCompilerChecker::check_workspace(workspace_root).await?;
                            let errors_after = next_report.errors.len();

                            if errors_after < errors_before {
                                info!(
                                    file = %cluster.file.display(),
                                    import = %import_path,
                                    "Self-healing step succeeded: errors reduced from {} to {}",
                                    errors_before, errors_after
                                );
                                steps.push(SelfHealingStep {
                                    attempt: attempts,
                                    file: cluster.file.clone(),
                                    cluster_symbol: Some(sym.clone()),
                                    action_taken: format!("Inserted `{}`", import_stmt.trim()),
                                    errors_before,
                                    errors_after,
                                    success: true,
                                });
                                current_report = next_report;
                                fix_applied = true;
                                break;
                            } else {
                                // Revert modification on regression or no progress
                                warn!(
                                    file = %cluster.file.display(),
                                    "Self-healing step reverted: error count did not improve ({} -> {})",
                                    errors_before, errors_after
                                );
                                let _ = std::fs::write(&cluster.file, orig_content);
                                steps.push(SelfHealingStep {
                                    attempt: attempts,
                                    file: cluster.file.clone(),
                                    cluster_symbol: Some(sym.clone()),
                                    action_taken: format!(
                                        "Attempted `{}` but reverted (errors {} → {})",
                                        import_stmt.trim(),
                                        errors_before,
                                        errors_after
                                    ),
                                    errors_before,
                                    errors_after,
                                    success: false,
                                });
                            }
                        }
                    }
                }
            }

            // If no deterministic fix could be applied, halt the loop
            if !fix_applied {
                debug!("No further deterministic automated fixes available; halting loop.");
                break;
            }
        }

        let final_errors = current_report.errors.len();
        let fully_resolved = current_report.is_clean();

        Ok(SelfHealingReport {
            initial_errors,
            final_errors,
            attempts_used: attempts,
            max_attempts: clamped_attempts,
            fully_resolved,
            steps,
            remaining_diagnostics: current_report.errors,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diagnostic_categorization_rust_missing_import() {
        let item = DiagnosticItem {
            file: PathBuf::from("src/main.rs"),
            line: 10,
            column: 15,
            severity: "error".to_string(),
            code: Some("E0412".to_string()),
            message: "cannot find type `UserSession` in this scope".to_string(),
            rendered: None,
        };

        let (category, symbol) = DiagnosticTriageEngine::categorize(&item);
        assert_eq!(category, DiagnosticCategory::MissingImport);
        assert_eq!(symbol.as_deref(), Some("UserSession"));
    }

    #[test]
    fn test_diagnostic_categorization_rust_method_not_found() {
        let item = DiagnosticItem {
            file: PathBuf::from("src/app.rs"),
            line: 25,
            column: 10,
            severity: "error".to_string(),
            code: Some("E0599".to_string()),
            message: "no method named `format_receipt` found for struct `TransactionReceipt` in the current scope".to_string(),
            rendered: None,
        };

        let (category, symbol) = DiagnosticTriageEngine::categorize(&item);
        assert_eq!(category, DiagnosticCategory::UnresolvedMethodOrField);
        assert_eq!(symbol.as_deref(), Some("format_receipt"));
    }

    #[test]
    fn test_diagnostic_clustering_primary_and_cascading() {
        let item1 = DiagnosticItem {
            file: PathBuf::from("src/service.rs"),
            line: 5,
            column: 8,
            severity: "error".to_string(),
            code: Some("E0412".to_string()),
            message: "cannot find type `PaymentClient` in this scope".to_string(),
            rendered: None,
        };

        let item2 = DiagnosticItem {
            file: PathBuf::from("src/service.rs"),
            line: 12,
            column: 14,
            severity: "error".to_string(),
            code: Some("E0412".to_string()),
            message: "cannot find type `PaymentClient` in this scope".to_string(),
            rendered: None,
        };

        let report = DiagnosticReport {
            errors: vec![item1, item2],
            warnings: vec![],
        };

        let triage = DiagnosticTriageEngine::triage(&report, Path::new("."));
        assert_eq!(triage.primary_clusters.len(), 1);
        assert_eq!(triage.primary_clusters[0].cascading_errors.len(), 1);
        assert_eq!(
            triage.primary_clusters[0].root_symbol.as_deref(),
            Some("PaymentClient")
        );
    }

    #[test]
    fn test_diagnostic_categorization_type_mismatch() {
        let item = DiagnosticItem {
            file: PathBuf::from("src/calc.rs"),
            line: 42,
            column: 12,
            severity: "error".to_string(),
            code: Some("E0308".to_string()),
            message: "mismatched types: expected `u64`, found `i32`".to_string(),
            rendered: Some("help: consider using `.try_into()`".to_string()),
        };

        let (category, symbol) = DiagnosticTriageEngine::categorize(&item);
        assert_eq!(category, DiagnosticCategory::TypeMismatch);
        assert!(symbol.is_none());
    }

    #[test]
    fn test_self_healing_report_formatting() {
        let report = SelfHealingReport {
            initial_errors: 3,
            final_errors: 0,
            attempts_used: 2,
            max_attempts: 3,
            fully_resolved: true,
            steps: vec![
                SelfHealingStep {
                    attempt: 1,
                    file: PathBuf::from("src/main.rs"),
                    cluster_symbol: Some("UserSession".to_string()),
                    action_taken: "Inserted `use crate::auth::UserSession;`".to_string(),
                    errors_before: 3,
                    errors_after: 1,
                    success: true,
                },
                SelfHealingStep {
                    attempt: 2,
                    file: PathBuf::from("src/main.rs"),
                    cluster_symbol: Some("AuthToken".to_string()),
                    action_taken: "Inserted `use crate::auth::AuthToken;`".to_string(),
                    errors_before: 1,
                    errors_after: 0,
                    success: true,
                },
            ],
            remaining_diagnostics: vec![],
        };

        let summary = report.format_summary(Path::new("/workspace"));
        assert!(summary.contains("Autonomous Diagnostic Self-Healing Report"));
        assert!(summary.contains("✅ Clean (All 3 initial error(s) resolved in 2 attempt(s))"));
        assert!(summary.contains("Inserted `use crate::auth::UserSession;`"));
        assert!(summary.contains("✅ Success"));
    }
}

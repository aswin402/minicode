//! Modular static analysis rules: Tree-sitter AST packs (Rust, Python/FastAPI, React/Next/Hono/TS/JS),
//! universal security/hygiene checks, and structural Flutter/Dart & Go scanners.

pub mod catalog;
pub mod flutter_go;
pub mod python;
pub mod react_js;
pub mod rust_lang;
pub mod universal;

use crate::config::{is_line_suppressed, DoctorConfig};
use crate::detector::LoadedFile;
use crate::types::{DetectedStack, Diagnostic, DiagnosticCategory, Severity};

/// Helper to construct a cleanly formatted [`Diagnostic`] with trimmed snippet line.
#[allow(clippy::too_many_arguments)]
#[must_use]
pub(crate) fn make_diag(
    rule: &str,
    severity: Severity,
    category: DiagnosticCategory,
    file: &str,
    line: usize,
    column: usize,
    message: impl Into<String>,
    suggestion: impl Into<String>,
    source: &str,
) -> Diagnostic {
    let snippet = if line > 0 {
        source
            .lines()
            .nth(line.saturating_sub(1))
            .map(|l| l.trim().chars().take(120).collect::<String>())
            .filter(|s| !s.is_empty())
    } else {
        None
    };
    Diagnostic {
        rule: rule.to_string(),
        severity,
        category,
        dead_code_kind: None,
        file: file.to_string(),
        line,
        column,
        message: message.into(),
        suggestion: suggestion.into(),
        snippet,
    }
}

/// Evaluates all applicable rules on a single loaded file, applying config & inline suppressions.
#[must_use]
pub fn analyze_file(
    file: &LoadedFile,
    stacks: &[DetectedStack],
    cfg: &DoctorConfig,
    run_security: bool,
) -> Vec<Diagnostic> {
    let mut raw = Vec::new();

    // 1. Universal hygiene & security rules
    universal::check_universal(file, run_security, &mut raw);

    // 2. Language / Stack-specific AST & structural rules
    match file.ext.as_str() {
        "ts" | "tsx" | "mts" | "cts" | "js" | "jsx" | "mjs" | "cjs" => {
            react_js::check_react_js(file, stacks, run_security, &mut raw);
        }
        "py" => {
            python::check_python(file, stacks, run_security, &mut raw);
        }
        "rs" => {
            rust_lang::check_rust(file, stacks, &mut raw);
        }
        "dart" => {
            flutter_go::check_flutter_dart(file, &mut raw);
        }
        "go" => {
            flutter_go::check_go(file, run_security, &mut raw);
        }
        _ => {}
    }

    // 3. Apply rule ignores, severity overrides, and inline `minidoctor-ignore` comments
    raw.into_iter()
        .filter(|d| !cfg.is_rule_ignored(&d.rule))
        .filter(|d| !is_line_suppressed(&file.content, d.line, &d.rule))
        .map(|mut d| {
            d.severity = cfg.effective_severity(&d.rule, d.severity);
            d
        })
        .collect()
}

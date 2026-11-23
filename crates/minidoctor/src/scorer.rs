//! Density-aware weighted 0–100 Health Scorer with category sub-scores and trend history.

use crate::types::{Diagnostic, DiagnosticCategory, HistoryEntry, ScoreBreakdown, Severity};
use std::fs;
use std::path::Path;

/// Calculates a density-normalized, risk-weighted 0–100 [`ScoreBreakdown`].
///
/// Combines `py_doctor`'s risk multipliers (Criticals and Security weigh far heavier than Warnings)
/// with square-root codebase size normalization so large repositories don't flatline at 0 from
/// minor warnings while Critical security/runtime bugs ALWAYS apply sharp ceiling caps.
#[must_use]
pub fn compute_score(
    diagnostics: &[Diagnostic],
    file_count: usize,
    threshold: u8,
    previous_history: &[HistoryEntry],
) -> ScoreBreakdown {
    let mut criticals = 0usize;
    let mut errors = 0usize;
    let mut warnings = 0usize;
    let mut infos = 0usize;
    let mut dead_code = 0usize;
    let mut security_issues = 0usize;

    let mut cat_penalties = [0.0f64; 5]; // Correctness, Security, Performance, Architecture/DeadCode, Maintainability

    for d in diagnostics {
        match d.severity {
            Severity::Critical => criticals += 1,
            Severity::Error => errors += 1,
            Severity::Warning => warnings += 1,
            Severity::Info => infos += 1,
        }
        if d.category == DiagnosticCategory::DeadCode {
            dead_code += 1;
        }
        if d.category == DiagnosticCategory::Security {
            security_issues += 1;
        }

        let base_weight = match d.severity {
            Severity::Critical => 12.0,
            Severity::Error => 4.0,
            Severity::Warning => 1.2,
            Severity::Info => 0.3,
        };
        let cat_idx = match d.category {
            DiagnosticCategory::Correctness => 0,
            DiagnosticCategory::Security => 1,
            DiagnosticCategory::Performance => 2,
            DiagnosticCategory::Architecture | DiagnosticCategory::DeadCode => 3,
            DiagnosticCategory::Maintainability => 4,
        };
        cat_penalties[cat_idx] += base_weight;
    }

    // Gentle sqrt normalization for medium/large repos (>= 4 files -> scale >= 1.0)
    let norm = ((file_count.max(1) as f64) / 4.0).sqrt().max(1.0);

    let sub_score = |raw_penalty: f64| -> u8 {
        let scaled = raw_penalty / norm;
        (100.0 - scaled).clamp(0.0, 100.0).round() as u8
    };

    let correctness = sub_score(cat_penalties[0] * 1.4);
    let security = sub_score(cat_penalties[1] * 1.8);
    let performance = sub_score(cat_penalties[2] * 1.3);
    let architecture = sub_score(cat_penalties[3] * 1.1);
    let maintainability = sub_score(cat_penalties[4] * 1.0);

    // Weighted composite penalty
    let raw_total = (criticals as f64) * 11.0
        + (errors as f64) * 3.5
        + (warnings as f64) * 1.0
        + (dead_code as f64) * 0.6
        + (security_issues as f64) * 3.0;

    // Criticals are NOT diluted by repo size (a hardcoded secret or SQLi is always serious)
    let fixed_crit_penalty = (criticals as f64) * 8.0;
    let scalable_penalty = (raw_total - (criticals as f64) * 11.0).max(0.0) / norm;
    let mut overall = (100.0 - fixed_crit_penalty - scalable_penalty)
        .clamp(0.0, 100.0)
        .round() as u8;

    // Hard ceiling caps for critical runtime/security defects
    if criticals >= 3 {
        overall = overall.min(49);
    } else if criticals >= 1 {
        overall = overall.min(74);
    }

    let (grade, label) = match overall {
        90..=100 => ("A", "Excellent"),
        75..=89 => ("B", "Great"),
        50..=74 => ("C", "Needs work"),
        25..=49 => ("D", "Critical"),
        _ => ("F", "Critical"),
    };

    let delta_from_previous = previous_history
        .last()
        .map(|prev| (overall as i16) - (prev.score as i16));

    let mut trend_history: Vec<u8> = previous_history
        .iter()
        .rev()
        .take(11)
        .map(|h| h.score)
        .collect();
    trend_history.reverse();
    trend_history.push(overall);

    ScoreBreakdown {
        overall,
        grade: grade.to_string(),
        label: label.to_string(),
        correctness,
        security,
        performance,
        architecture,
        maintainability,
        criticals,
        errors,
        warnings,
        infos,
        dead_code,
        security_issues,
        threshold,
        delta_from_previous,
        trend_history,
    }
}

/// Reads past score history from `.minicode/minidoctor/history.jsonl`.
#[must_use]
pub fn load_history(workspace: &Path) -> Vec<HistoryEntry> {
    let path = workspace
        .join(".minicode")
        .join("minidoctor")
        .join("history.jsonl");
    let Ok(text) = fs::read_to_string(path) else {
        return Vec::new();
    };
    text.lines()
        .filter_map(|line| serde_json::from_str::<HistoryEntry>(line).ok())
        .collect()
}

/// Renders an 8-level Unicode sparkline (`▁▂▃▄▅▆▇█`) for a sequence of 0–100 scores.
#[must_use]
pub fn render_sparkline(scores: &[u8]) -> String {
    const BARS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    scores
        .iter()
        .map(|&s| {
            let idx = ((s as usize) * 7) / 100;
            BARS[idx.min(7)]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_score_clean_and_critical_caps() {
        let clean = compute_score(&[], 20, 75, &[]);
        assert_eq!(clean.overall, 100);
        assert_eq!(clean.grade, "A");
        assert_eq!(clean.label, "Excellent");

        let crit = Diagnostic {
            rule: "sec/hardcoded-secret".to_string(),
            severity: Severity::Critical,
            category: DiagnosticCategory::Security,
            dead_code_kind: None,
            file: "src/config.py".to_string(),
            line: 10,
            column: 1,
            message: "Hardcoded secret".to_string(),
            suggestion: "Use env var".to_string(),
            snippet: None,
        };
        let one_crit = compute_score(&[crit.clone()], 100, 75, &[]);
        assert!(
            one_crit.overall <= 74,
            "1 critical must cap <= 74, got {}",
            one_crit.overall
        );

        let three_crits = compute_score(&[crit.clone(), crit.clone(), crit], 100, 75, &[]);
        assert!(
            three_crits.overall <= 49,
            "3 criticals must cap <= 49, got {}",
            three_crits.overall
        );
    }
}

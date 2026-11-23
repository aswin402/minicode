//! Native Rust dead-code & duplicate-block detector (`DeadCodeKind::UnusedExport`,
//! `DeadCodeKind::UnusedFile`, `DeadCodeKind::DuplicateCode`) with zero external tool dependencies.

use crate::config::DoctorConfig;
use crate::detector::LoadedFile;
use crate::types::{DeadCodeKind, Diagnostic, DiagnosticCategory, Severity};
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::OnceLock;

const MAX_DEAD_EXPORTS_REPORTED: usize = 20;
const MAX_DUPLICATES_REPORTED: usize = 12;
const DUPLICATE_WINDOW_LINES: usize = 12;

fn js_export_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?m)^\s*export\s+(?:async\s+)?(?:function|const|let|class|type|interface|enum)\s+([A-Za-z_][A-Za-z0-9_]{3,})"#)
            .unwrap_or_else(|_| Regex::new("a^").unwrap_or_else(|_| unreachable!()))
    })
}

fn py_top_def_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?m)^(?:async\s+)?(?:def|class)\s+([A-Za-z][A-Za-z0-9_]{3,})\s*[\(:]"#)
            .unwrap_or_else(|_| Regex::new("a^").unwrap_or_else(|_| unreachable!()))
    })
}

fn is_entrypoint_file(rel_path: &str) -> bool {
    let lower = rel_path.to_ascii_lowercase();
    let stem = lower
        .rsplit('/')
        .next()
        .unwrap_or(&lower)
        .split('.')
        .next()
        .unwrap_or("");
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
            | "build"
    ) || lower.contains("/routes/")
        || lower.contains("/pages/")
        || lower.contains("/app/")
        || lower.contains("/bin/")
        || lower.contains("/api/")
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

    // 2. Unused exports (JS/TS `export ...` and Python module-level `def/class` never referenced in any other file)
    if !cfg.is_rule_ignored("dead/unused-export") {
        let js_re = js_export_re();
        let py_re = py_top_def_re();
        let mut reported = 0usize;

        for f in files {
            if reported >= MAX_DEAD_EXPORTS_REPORTED || f.is_test || is_entrypoint_file(&f.rel_path)
            {
                continue;
            }
            let candidates: Vec<(usize, String)> = match f.ext.as_str() {
                "ts" | "tsx" | "js" | "jsx" | "mts" | "mjs" => js_re
                    .captures_iter(&f.content)
                    .filter_map(|cap| {
                        let m = cap.get(1)?;
                        let line = f.content[..m.start()]
                            .bytes()
                            .filter(|&b| b == b'\n')
                            .count()
                            + 1;
                        Some((line, m.as_str().to_string()))
                    })
                    .collect(),
                "py" => py_re
                    .captures_iter(&f.content)
                    .filter_map(|cap| {
                        let m = cap.get(1)?;
                        let name = m.as_str();
                        if name.starts_with("test_") || name.starts_with('_') {
                            return None;
                        }
                        let line = f.content[..m.start()]
                            .bytes()
                            .filter(|&b| b == b'\n')
                            .count()
                            + 1;
                        // Skip decorated FastAPI routes / CLI commands
                        let prev_line = if line > 1 {
                            f.content.lines().nth(line - 2).unwrap_or("").trim()
                        } else {
                            ""
                        };
                        if prev_line.starts_with('@') {
                            return None;
                        }
                        Some((line, name.to_string()))
                    })
                    .collect(),
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

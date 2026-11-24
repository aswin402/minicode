//! Structured line-anchored review comments, deterministic diff-hunk re-anchoring,
//! asymmetric fact-checker verification, audience formatters, and SARIF 2.1.0 export.
//!
//! Precision comment-anchoring and asymmetric fact-checking inspired by
//! `alibaba/open-code-review` (Apache-2.0).

use crate::git::diff_parser::{GitDiffFile, GitDiffViewer};
use crate::git::reviewer::{ReviewFinding, ReviewReport};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::Path;

/// Target audience for rendered review reports (`--audience human|agent`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReviewAudience {
    #[default]
    Human,
    Agent,
}

impl ReviewAudience {
    #[must_use]
    pub fn parse(raw: &str) -> Self {
        if raw.eq_ignore_ascii_case("agent") {
            Self::Agent
        } else {
            Self::Human
        }
    }
}

/// Output format for standalone `minicode review` CLI & CI pipelines.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReviewOutputFormat {
    #[default]
    Markdown,
    Json,
    Sarif,
}

impl ReviewOutputFormat {
    #[must_use]
    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "json" => Self::Json,
            "sarif" => Self::Sarif,
            _ => Self::Markdown,
        }
    }
}

/// Normalized severity levels aligned with OCR & SARIF 2.1.0.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CommentSeverity {
    Critical,
    High,
    Medium,
    Low,
}

impl CommentSeverity {
    #[must_use]
    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "critical" | "blocker" | "error" => Self::Critical,
            "high" => Self::High,
            "medium" | "important" | "warning" | "warn" => Self::Medium,
            _ => Self::Low,
        }
    }

    #[must_use]
    pub fn sarif_level(&self) -> &'static str {
        match self {
            Self::Critical | Self::High => "error",
            Self::Medium => "warning",
            Self::Low => "note",
        }
    }

    #[must_use]
    pub fn badge(&self) -> &'static str {
        match self {
            Self::Critical => "CRITICAL",
            Self::High => "HIGH",
            Self::Medium => "MEDIUM",
            Self::Low => "LOW",
        }
    }
}

/// A line-anchored structured code review comment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnchoredComment {
    pub path: String,
    pub line: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_line: Option<usize>,
    pub severity: CommentSeverity,
    pub category: String,
    pub title: String,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggested_diff: Option<String>,
    /// True once deterministic diff-hunk or live-file verification confirmed the line anchor.
    #[serde(default)]
    pub anchor_verified: bool,
    /// True if deterministic relocation snapped a drifted line coordinate onto the matching line.
    #[serde(default)]
    pub anchor_relocated: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor_snippet: Option<String>,
}

/// Result of deterministic line-anchoring + asymmetric fact-check filtering.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AnchoredReviewSummary {
    pub comments: Vec<AnchoredComment>,
    pub dropped_unverifiable: usize,
    pub relocated_count: usize,
}

/// Deterministic line-anchoring, JSON-comment repair, and SARIF 2.1.0 serializer.
pub struct CommentAnchorEngine;

impl CommentAnchorEngine {
    /// Maximum line drift radius when snapping a comment's snippet onto the real diff/file line.
    const RELOCATION_WINDOW: usize = 15;

    /// Converts static [`ReviewReport`] findings into [`AnchoredComment`] entries and verifies
    /// their coordinates against `raw_diff` and live files under `workspace_root`.
    #[must_use]
    pub fn anchor_static_report(
        workspace_root: &Path,
        report: &ReviewReport,
        raw_diff: &str,
    ) -> AnchoredReviewSummary {
        let mut raw_comments: Vec<AnchoredComment> = report
            .findings
            .iter()
            .filter(|f| !f.severity.eq_ignore_ascii_case("PASS"))
            .map(Self::from_review_finding)
            .collect();
        Self::verify_and_relocate(workspace_root, raw_diff, &mut raw_comments)
    }

    fn from_review_finding(f: &ReviewFinding) -> AnchoredComment {
        let (path, line, end_line, snippet) = f
            .line_hint
            .as_deref()
            .map(parse_path_and_line_hint)
            .unwrap_or_else(|| (String::new(), None, None, None));
        AnchoredComment {
            path,
            line,
            end_line,
            severity: CommentSeverity::parse(&f.severity),
            category: normalize_category(&f.category),
            title: f.title.clone(),
            content: f.details.clone(),
            suggested_diff: None,
            anchor_verified: false,
            anchor_relocated: false,
            anchor_snippet: snippet,
        }
    }

    /// Parses structured JSON comments from an LLM tool output or fenced JSON block, repairing
    /// stringified JSON arrays or trailing commas deterministically when needed.
    #[must_use]
    pub fn parse_structured_comments(
        raw: &str,
        default_path: Option<&str>,
    ) -> Vec<AnchoredComment> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Vec::new();
        }
        let candidate_json = extract_json_slice(trimmed).unwrap_or(trimmed);
        let parsed: Option<Value> =
            serde_json::from_str::<Value>(candidate_json)
                .ok()
                .or_else(|| {
                    // Deterministic trailing-comma repair before falling back
                    let repaired = candidate_json
                        .replace(",\n]", "\n]")
                        .replace(", ]", " ]")
                        .replace(",]", "]")
                        .replace(",\n}", "\n}")
                        .replace(",}", "}");
                    serde_json::from_str::<Value>(&repaired).ok()
                });

        let Some(val) = parsed else {
            return Vec::new();
        };

        let items: Vec<Value> = match val {
            Value::Array(arr) => arr,
            Value::Object(ref map) => {
                if let Some(Value::Array(arr)) = map.get("comments") {
                    arr.clone()
                } else if let Some(Value::String(s)) = map.get("comments") {
                    serde_json::from_str::<Vec<Value>>(s).unwrap_or_default()
                } else {
                    vec![val]
                }
            }
            _ => Vec::new(),
        };

        let mut out = Vec::new();
        for item in items {
            let Some(obj) = item.as_object() else {
                continue;
            };
            let content = obj
                .get("content")
                .or_else(|| obj.get("message"))
                .or_else(|| obj.get("details"))
                .and_then(Value::as_str)
                .map(str::trim)
                .unwrap_or("");
            if content.is_empty() {
                continue;
            }
            let raw_path = obj
                .get("path")
                .or_else(|| obj.get("file"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .or(default_path)
                .unwrap_or("");
            let (parsed_path, embedded_line, embedded_end_line, _) =
                parse_path_and_line_hint(raw_path);
            let line = obj
                .get("line")
                .or_else(|| obj.get("start_line"))
                .and_then(Value::as_u64)
                .map(|n| n as usize)
                .or(embedded_line);
            let end_line = obj
                .get("end_line")
                .and_then(Value::as_u64)
                .map(|n| n as usize)
                .or(embedded_end_line);
            let severity = obj
                .get("severity")
                .and_then(Value::as_str)
                .map(CommentSeverity::parse)
                .unwrap_or(CommentSeverity::Medium);
            let category = obj
                .get("category")
                .and_then(Value::as_str)
                .map(normalize_category)
                .unwrap_or_else(|| "bug".to_string());
            let title = obj
                .get("title")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| {
                    content
                        .lines()
                        .next()
                        .unwrap_or("Review finding")
                        .chars()
                        .take(80)
                        .collect()
                });
            let suggested_diff = obj
                .get("suggested_diff")
                .or_else(|| obj.get("suggestion"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string);
            let anchor_snippet = obj
                .get("snippet")
                .or_else(|| obj.get("code_snippet"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string);

            out.push(AnchoredComment {
                path: parsed_path,
                line,
                end_line,
                severity,
                category,
                title,
                content: content.to_string(),
                suggested_diff,
                anchor_verified: false,
                anchor_relocated: false,
                anchor_snippet,
            });
        }
        out
    }

    /// Deterministically verifies every comment's `(path, line)` against parsed unified diff hunks
    /// and live workspace files, snapping drifted lines within `±15` lines when an `anchor_snippet`
    /// matches, and dropping findings whose file path does not exist anywhere in diff or workspace.
    #[must_use]
    pub fn verify_and_relocate(
        workspace_root: &Path,
        raw_diff: &str,
        comments: &mut [AnchoredComment],
    ) -> AnchoredReviewSummary {
        let parsed_diff: Vec<GitDiffFile> = GitDiffViewer::parse_raw_diff(raw_diff, false);
        let diff_by_path: HashMap<String, &GitDiffFile> = parsed_diff
            .iter()
            .map(|df| (df.path.replace('\\', "/"), df))
            .collect();

        let mut kept = Vec::with_capacity(comments.len());
        let mut dropped_unverifiable = 0usize;
        let mut relocated_count = 0usize;

        for comment in comments.iter().cloned() {
            let mut c = comment;
            c.path = c.path.replace('\\', "/");

            // Repo-level findings with no specific file path remain valid
            if c.path.is_empty() {
                kept.push(c);
                continue;
            }

            let abs_path = workspace_root.join(&c.path);
            let in_diff = diff_by_path.get(&c.path).copied();
            let live_lines: Option<Vec<String>> = std::fs::read_to_string(&abs_path)
                .ok()
                .map(|s| s.lines().map(str::to_string).collect());

            // Asymmetric fact-check rule: only drop if the workspace + diff PROVE the target file
            // does not exist at all.
            if in_diff.is_none() && live_lines.is_none() {
                dropped_unverifiable += 1;
                continue;
            }

            if let Some(target_line) = c.line {
                // 1. Check exact match in diff hunk (new_lineno)
                let exact_in_diff = in_diff.and_then(|df| {
                    df.lines
                        .iter()
                        .find(|l| l.new_lineno == Some(target_line) && l.tag != '-')
                });

                if let Some(dl) = exact_in_diff {
                    c.anchor_verified = true;
                    if c.anchor_snippet.is_none() {
                        c.anchor_snippet = Some(dl.content.trim().to_string());
                    }
                    kept.push(c);
                    continue;
                }

                // 2. If a snippet was provided, search within ±RELOCATION_WINDOW in diff or live file
                if let Some(ref snip) = c.anchor_snippet {
                    let needle = snip.trim();
                    if !needle.is_empty() {
                        if let Some(snapped) = find_relocated_line(
                            in_diff,
                            live_lines.as_deref(),
                            target_line,
                            needle,
                            Self::RELOCATION_WINDOW,
                        ) {
                            if snapped != target_line {
                                c.line = Some(snapped);
                                c.anchor_relocated = true;
                                relocated_count += 1;
                            }
                            c.anchor_verified = true;
                            kept.push(c);
                            continue;
                        }
                    }
                }

                // 3. Fallback: verify line bounds against the live file on disk
                if let Some(ref lines) = live_lines {
                    if target_line >= 1 && target_line <= lines.len() {
                        c.anchor_verified = true;
                        if c.anchor_snippet.is_none() {
                            c.anchor_snippet = Some(lines[target_line - 1].trim().to_string());
                        }
                    } else if !lines.is_empty() {
                        // Out-of-bounds line number: clamp/clear instead of reporting a bogus line
                        c.line = None;
                        c.anchor_verified = false;
                    }
                }
            } else if in_diff.is_some() || live_lines.is_some() {
                c.anchor_verified = true;
            }

            kept.push(c);
        }

        // Stable sort: most severe first, then path + line
        kept.sort_by(|a, b| {
            a.severity
                .cmp(&b.severity)
                .then_with(|| a.path.cmp(&b.path))
                .then_with(|| a.line.cmp(&b.line))
        });

        AnchoredReviewSummary {
            comments: kept,
            dropped_unverifiable,
            relocated_count,
        }
    }

    /// Serializes review findings into a standards-compliant **SARIF 2.1.0** document for
    /// GitHub Actions Code Scanning (`github/codeql-action/upload-sarif`).
    #[must_use]
    pub fn to_sarif_json(summary: &AnchoredReviewSummary) -> Value {
        let results: Vec<Value> = summary
            .comments
            .iter()
            .map(|c| {
                let rule_id = format!("minicode-review/{}", c.category);
                let mut region = json!({});
                if let Some(line) = c.line {
                    region["startLine"] = json!(line);
                    if let Some(end) = c.end_line.filter(|&e| e >= line) {
                        region["endLine"] = json!(end);
                    }
                }
                let uri = if c.path.is_empty() {
                    ".".to_string()
                } else {
                    c.path.clone()
                };
                json!({
                    "ruleId": rule_id,
                    "level": c.severity.sarif_level(),
                    "message": {
                        "text": format!("{}: {}", c.title, c.content)
                    },
                    "locations": [{
                        "physicalLocation": {
                            "artifactLocation": { "uri": uri },
                            "region": region
                        }
                    }],
                    "properties": {
                        "severity": c.severity.badge(),
                        "anchorVerified": c.anchor_verified,
                        "anchorRelocated": c.anchor_relocated,
                        "suggestedDiff": c.suggested_diff,
                    }
                })
            })
            .collect();

        json!({
            "$schema": "https://json.schemastore.org/sarif-2.1.0.json",
            "version": "2.1.0",
            "runs": [{
                "tool": {
                    "driver": {
                        "name": "minicode-review",
                        "version": env!("CARGO_PKG_VERSION"),
                        "informationUri": "https://github.com/aswin402/minicode",
                    }
                },
                "results": results
            }]
        })
    }

    /// Renders findings for either `Human` (clean markdown scorecard) or `Agent` (compact,
    /// deterministic, zero-fluff coordinates + action contract).
    #[must_use]
    pub fn format_for_audience(
        report: &ReviewReport,
        summary: &AnchoredReviewSummary,
        audience: ReviewAudience,
    ) -> String {
        match audience {
            ReviewAudience::Human => {
                let mut out = crate::git::reviewer::GitReviewer::format_report(report);
                if summary.relocated_count > 0 || summary.dropped_unverifiable > 0 {
                    out.push_str(&format!(
                        "\n*Line-Anchor Verification: {} comment(s) verified, {} snapped to exact hunk lines, {} unverifiable dropped.*\n",
                        summary.comments.len(),
                        summary.relocated_count,
                        summary.dropped_unverifiable
                    ));
                }
                out
            }
            ReviewAudience::Agent => {
                let mut out = format!(
                    "MINICODE_REVIEW score={}/100 files_audited={} findings={} relocated={} dropped={}\n",
                    report.total_score,
                    report.files_audited,
                    summary.comments.len(),
                    summary.relocated_count,
                    summary.dropped_unverifiable
                );
                if summary.comments.is_empty() {
                    out.push_str("STATUS: CLEAN (0 blocking findings)\n");
                } else {
                    for (idx, c) in summary.comments.iter().enumerate() {
                        let loc = match (c.path.is_empty(), c.line) {
                            (false, Some(l)) => format!("{}:{}", c.path, l),
                            (false, None) => c.path.clone(),
                            (true, _) => "<workspace>".to_string(),
                        };
                        let verified = if c.anchor_verified {
                            "verified"
                        } else {
                            "unanchored"
                        };
                        out.push_str(&format!(
                            "{}. [{}] ({}, {}) {} — {}: {}\n",
                            idx + 1,
                            c.severity.badge(),
                            c.category,
                            verified,
                            loc,
                            c.title,
                            c.content
                        ));
                    }
                }
                out
            }
        }
    }
}

/// Asymmetric precision fact-checker contract & behavior/impact brief instructions inspired by
/// `alibaba/open-code-review`'s `review_filter_task` and `behaviortask`.
pub struct ReviewPrecisionPrompts;

impl ReviewPrecisionPrompts {
    /// Prompt block enforcing precision-over-recall self-fact-checking inside Reviewer subagents.
    pub const ASYMMETRIC_FACT_CHECK_CONTRACT: &'static str = "\
### Precision & Asymmetric Fact-Check Discipline\n\
- Favor precision over recall: report ONLY defects backed by direct file/diff evidence. Discard speculative nits silently.\n\
- Before reporting any cross-file claim, verify callers/types/invariants with `read_file` or `grep_search`.\n\
- Do NOT duplicate deterministic compiler/linter warnings (`rustc`, `clippy`, `tsc`, `eslint`, `rustfmt`).\n\
- Anchor every finding with exact `path:line` verified from the file/diff you actually read.";

    /// Prompt block for opt-in Behavior & Impact Analysis (`analysis=true` / `--analysis`).
    pub const BEHAVIOR_IMPACT_CONTRACT: &'static str = "\
### Behavior & Impact Brief (Required when `analysis=true`)\n\
- Include a concise `## Behavior & Impact` section **separate** from severity-rated defect findings.\n\
- Report any material behavior, API, CLI, config, or persistence change even when there are 0 defects.\n\
- For each material change, name: (1) affected workflow, (2) before vs. after behavior, and (3) exact `file:line` evidence.\n\
- Omit the section only if changes are purely internal refactors with zero observable behavioral shift.";
}

fn find_relocated_line(
    in_diff: Option<&GitDiffFile>,
    live_lines: Option<&[String]>,
    target_line: usize,
    needle: &str,
    window: usize,
) -> Option<usize> {
    let min_line = target_line.saturating_sub(window).max(1);
    let max_line = target_line.saturating_add(window);

    if let Some(df) = in_diff {
        let mut best: Option<(usize, usize)> = None;
        for dl in &df.lines {
            if dl.tag == '-' {
                continue;
            }
            let Some(ln) = dl.new_lineno else {
                continue;
            };
            if ln >= min_line && ln <= max_line && dl.content.contains(needle) {
                let dist = ln.abs_diff(target_line);
                if best.is_none_or(|(best_dist, _)| dist < best_dist) {
                    best = Some((dist, ln));
                }
            }
        }
        if let Some((_, ln)) = best {
            return Some(ln);
        }
    }

    if let Some(lines) = live_lines {
        let mut best: Option<(usize, usize)> = None;
        let end = max_line.min(lines.len());
        for ln in min_line..=end {
            if lines[ln - 1].contains(needle) {
                let dist = ln.abs_diff(target_line);
                if best.is_none_or(|(best_dist, _)| dist < best_dist) {
                    best = Some((dist, ln));
                }
            }
        }
        if let Some((_, ln)) = best {
            return Some(ln);
        }
    }

    None
}

/// Parses path/line hints in any format produced by `GitReviewer` or LLM reviewers:
/// - `src/db.rs:12`
/// - `src/App.tsx:L42` or `src/App.tsx:L42-L50`
/// - `src/foo.rs:lines 10-20`
/// - Raw diff code lines (`+foo.unwrap()`) or standalone `lines 10-20` (returns empty path + snippet
///   so static findings are never falsely dropped as nonexistent files).
fn parse_path_and_line_hint(hint: &str) -> (String, Option<usize>, Option<usize>, Option<String>) {
    let trimmed = hint.trim().trim_matches('`');
    if trimmed.is_empty() {
        return (String::new(), None, None, None);
    }

    // Raw diff line snippets (e.g. `+    x.unwrap()`) are not file paths
    if trimmed.starts_with('+') || trimmed.starts_with('-') || trimmed.starts_with("@@") {
        let code_snip = trimmed.trim_start_matches(['+', '-']).trim();
        let snip = (!code_snip.is_empty()).then(|| code_snip.to_string());
        return (String::new(), None, None, snip);
    }

    // Standalone `lines 10-20` without a file prefix
    if let Some(rest) = trimmed.strip_prefix("lines ") {
        let (start, end) = parse_line_range(rest);
        return (String::new(), start, end, None);
    }

    if let Some((left, right)) = trimmed.rsplit_once(':') {
        let right_clean = right.trim();
        let line_spec = right_clean
            .strip_prefix("lines ")
            .or_else(|| right_clean.strip_prefix("line "))
            .or_else(|| right_clean.strip_prefix('L'))
            .or_else(|| right_clean.strip_prefix('l'))
            .unwrap_or(right_clean);
        let (start, end) = parse_line_range(line_spec);
        if start.is_some() {
            let path = left.trim().replace('\\', "/");
            if looks_like_rel_path(&path) {
                return (path, start, end, None);
            }
        }
    }

    let candidate = trimmed.replace('\\', "/");
    if looks_like_rel_path(&candidate) {
        (candidate, None, None, None)
    } else {
        (String::new(), None, None, Some(trimmed.to_string()))
    }
}

fn parse_line_range(spec: &str) -> (Option<usize>, Option<usize>) {
    let clean = spec.trim();
    if let Some((a, b)) = clean.split_once('-') {
        let start = leading_usize(a.trim().trim_start_matches(['L', 'l']));
        let end = leading_usize(b.trim().trim_start_matches(['L', 'l']));
        return (start, end);
    }
    (leading_usize(clean), None)
}

fn leading_usize(s: &str) -> Option<usize> {
    let digits: String = s.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        None
    } else {
        digits.parse::<usize>().ok()
    }
}

fn looks_like_rel_path(s: &str) -> bool {
    !s.is_empty()
        && !s.contains('\n')
        && !s.contains(' ')
        && !s.contains(';')
        && !s.contains('(')
        && !s.contains('{')
        && (s.contains('/') || s.contains('.'))
}

fn normalize_category(raw: &str) -> String {
    let lower = raw.trim().to_ascii_lowercase();
    if lower.contains("sec") {
        "security".to_string()
    } else if lower.contains("perf") {
        "performance".to_string()
    } else if lower.contains("test") {
        "test".to_string()
    } else if lower.contains("doc") {
        "documentation".to_string()
    } else if lower.contains("maint") || lower.contains("arch") || lower.contains("hygiene") {
        "maintainability".to_string()
    } else if lower.contains("bug") || lower.contains("correct") || lower.contains("error") {
        "bug".to_string()
    } else if lower.is_empty() {
        "other".to_string()
    } else {
        lower.replace(' ', "-")
    }
}

fn extract_json_slice(raw: &str) -> Option<&str> {
    let first_arr = raw.find('[');
    let first_obj = raw.find('{');
    match (first_arr, first_obj) {
        (Some(a), Some(o)) if a < o => raw.rfind(']').filter(|&e| e > a).map(|e| &raw[a..=e]),
        (_, Some(o)) => raw.rfind('}').filter(|&e| e > o).map(|e| &raw[o..=e]),
        (Some(a), None) => raw.rfind(']').filter(|&e| e > a).map(|e| &raw[a..=e]),
        (None, None) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_and_relocate_drifted_comment_line() {
        let tmp = tempfile::tempdir().unwrap();
        let src_dir = tmp.path().join("src");
        std::fs::create_dir_all(&src_dir).unwrap();
        std::fs::write(
            src_dir.join("auth.rs"),
            "pub fn check(token: &str) -> bool {\n    let expected = \"abc\";\n    let _ok = token == expected;\n    token.unwrap_or_default(); // dummy\n    _ok\n}\n",
        )
        .unwrap();

        let raw_json = r#"```json
{
  "comments": [
    {
      "path": "src/auth.rs",
      "line": 1,
      "severity": "high",
      "category": "security",
      "title": "Non-constant-time token comparison",
      "content": "Direct == leaks timing side channel.",
      "snippet": "token == expected"
    },
    {
      "path": "src/nonexistent_hallucination.rs",
      "line": 42,
      "severity": "medium",
      "category": "bug",
      "content": "Hallucinated file comment."
    }
  ]
}
```"#;
        let mut parsed = CommentAnchorEngine::parse_structured_comments(raw_json, None);
        assert_eq!(parsed.len(), 2);

        let anchored = CommentAnchorEngine::verify_and_relocate(tmp.path(), "", &mut parsed);
        assert_eq!(anchored.dropped_unverifiable, 1);
        assert_eq!(anchored.relocated_count, 1);
        assert_eq!(anchored.comments.len(), 1);
        assert_eq!(anchored.comments[0].line, Some(3));
        assert!(anchored.comments[0].anchor_verified);
        assert!(anchored.comments[0].anchor_relocated);

        let sarif = CommentAnchorEngine::to_sarif_json(&anchored);
        assert_eq!(sarif["version"], "2.1.0");
        assert_eq!(
            sarif["runs"][0]["results"][0]["locations"][0]["physicalLocation"]["region"]
                ["startLine"],
            3
        );
        assert_eq!(sarif["runs"][0]["results"][0]["level"], "error");
    }

    #[test]
    fn test_agent_audience_format_is_compact_and_deterministic() {
        let report = ReviewReport {
            total_score: 85,
            files_audited: 4,
            findings: vec![ReviewFinding {
                category: "Security".to_string(),
                severity: "HIGH".to_string(),
                title: "SQL injection".to_string(),
                details: "Use parameterized bindings".to_string(),
                line_hint: Some("src/db.rs:12".to_string()),
            }],
            summary: String::new(),
        };
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("src")).unwrap();
        std::fs::write(tmp.path().join("src/db.rs"), "line\n".repeat(20)).unwrap();

        let anchored = CommentAnchorEngine::anchor_static_report(tmp.path(), &report, "");
        let rendered =
            CommentAnchorEngine::format_for_audience(&report, &anchored, ReviewAudience::Agent);
        assert!(rendered.contains("MINICODE_REVIEW score=85/100 files_audited=4 findings=1"));
        assert!(rendered.contains("[HIGH] (security, verified) src/db.rs:12"));
    }

    #[test]
    fn test_static_report_preserves_l_prefix_ranges_and_raw_diff_hints() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("src")).unwrap();
        std::fs::write(tmp.path().join("src/App.tsx"), "const a = 1;\n".repeat(50)).unwrap();
        std::fs::write(tmp.path().join("src/lib.rs"), "fn f() {}\n".repeat(30)).unwrap();

        let report = ReviewReport {
            total_score: 70,
            files_audited: 2,
            findings: vec![
                ReviewFinding {
                    category: "UI Taste & Anti-Slop".to_string(),
                    severity: "MEDIUM".to_string(),
                    title: "Banned pattern".to_string(),
                    details: "Avoid generic gradient".to_string(),
                    line_hint: Some("src/App.tsx:L42".to_string()),
                },
                ReviewFinding {
                    category: "AST Code Health".to_string(),
                    severity: "LOW".to_string(),
                    title: "God function".to_string(),
                    details: "Split helper".to_string(),
                    line_hint: Some("src/lib.rs:lines 10-20".to_string()),
                },
                ReviewFinding {
                    category: "Correctness".to_string(),
                    severity: "MEDIUM".to_string(),
                    title: "Unwrap pattern".to_string(),
                    details: "Avoid unwrap".to_string(),
                    line_hint: Some("+    val.unwrap();".to_string()),
                },
            ],
            summary: String::new(),
        };

        let anchored = CommentAnchorEngine::anchor_static_report(tmp.path(), &report, "");
        assert_eq!(anchored.dropped_unverifiable, 0);
        assert_eq!(anchored.comments.len(), 3);
        let ui = anchored
            .comments
            .iter()
            .find(|c| c.path == "src/App.tsx")
            .unwrap();
        assert_eq!(ui.line, Some(42));
        assert!(ui.anchor_verified);

        let ast = anchored
            .comments
            .iter()
            .find(|c| c.path == "src/lib.rs")
            .unwrap();
        assert_eq!(ast.line, Some(10));
        assert_eq!(ast.end_line, Some(20));
        assert!(ast.anchor_verified);

        let raw = anchored
            .comments
            .iter()
            .find(|c| c.path.is_empty())
            .unwrap();
        assert_eq!(raw.anchor_snippet.as_deref(), Some("val.unwrap();"));
    }
}

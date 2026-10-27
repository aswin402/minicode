//! Persistent numbered Decision Ledger (`.minicode/decisions.md`).
//!
//! Inspired by `XiangpengHao/bbtree`'s `design/03-decisions.md` (`D1..DN` living ledger):
//! - Every decision section is assigned a stable sequential ID (`[D1]`, `[D2]`, ...).
//! - Each entry states a decision as it holds now. When a decision with the same ID (`D<N>`)
//!   or canonical subject is recorded again, it is updated **in place** rather than appending
//!   contradictory history.
//! - Injected into the recency context as `<user_decisions>`. Secret answers are never written.

use std::path::{Path, PathBuf};

use crate::agent::inquiry::{InquiryInputType, InquiryRequest, InquiryResponse};

/// Workspace-relative location of the decision record.
pub const DECISIONS_FILE: &str = ".minicode/decisions.md";

/// Header line for `.minicode/decisions.md`.
const DECISIONS_HEADER: &str = "# Project Decisions (Living Decision Ledger — entries D1..DN state decisions as they hold now; updated in place)\n\n";

/// Maximum bytes of the decision record injected into the prompt (tail kept).
const MAX_PROMPT_BYTES: usize = 2_000;

/// Who settled the decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecisionSource {
    /// The user answered interactively.
    User,
    /// Headless mode auto-selected the recommended/default option.
    AutoDefault,
}

impl DecisionSource {
    fn label(self) -> &'static str {
        match self {
            Self::User => "answered by user",
            Self::AutoDefault => "auto-selected default (headless; not user-confirmed)",
        }
    }
}

fn decisions_path(workspace_root: &Path) -> PathBuf {
    workspace_root.join(DECISIONS_FILE)
}

/// Renders one inquiry/response pair as a markdown section. Returns `None` when
/// there is nothing worth recording (cancelled or only secret answers).
#[must_use]
pub fn render_entry(
    request: &InquiryRequest,
    response: &InquiryResponse,
    source: DecisionSource,
    timestamp: &str,
) -> Option<String> {
    if response.cancelled {
        return None;
    }
    let mut lines = Vec::new();
    for answer in &response.answers {
        let Some(question) = request
            .questions
            .iter()
            .find(|q| q.resolved_id() == answer.question_id)
        else {
            continue;
        };
        if answer.masked || question.input_type == InquiryInputType::Secret {
            continue;
        }
        // Map selected option ids back to human-readable labels.
        let value = if let Some(text) = &answer.custom_text {
            text.clone()
        } else {
            answer
                .selected_options
                .iter()
                .map(|id| {
                    question
                        .options
                        .iter()
                        .find(|o| o.resolved_id() == id)
                        .map_or_else(|| id.clone(), |o| o.label.clone())
                })
                .collect::<Vec<_>>()
                .join(", ")
        };
        if value.trim().is_empty() {
            continue;
        }
        let topic = question.header.as_deref().unwrap_or(&question.question);
        lines.push(format!("- {}: {}", topic.trim(), value.trim()));
    }
    if lines.is_empty() {
        return None;
    }
    Some(format!(
        "## {} ({}, {})\n{}\n\n",
        request.title.trim(),
        source.label(),
        timestamp,
        lines.join("\n")
    ))
}

/// Appends the decision entry to `.minicode/decisions.md`.
pub fn record(
    workspace_root: &Path,
    request: &InquiryRequest,
    response: &InquiryResponse,
    source: DecisionSource,
) -> std::io::Result<()> {
    let timestamp = chrono::Utc::now().format("%Y-%m-%d %H:%M UTC").to_string();
    let Some(entry) = render_entry(request, response, source, &timestamp) else {
        return Ok(());
    };
    append_entry(workspace_root, &entry)
}

/// Records that the foundation was settled by a verified quote from the user's
/// request (no question was asked).
#[allow(dead_code)]
pub fn record_basis(workspace_root: &Path, tool_name: &str, basis: &str) -> std::io::Result<()> {
    let timestamp = chrono::Utc::now().format("%Y-%m-%d %H:%M UTC").to_string();
    let entry = format!(
        "## Foundation (settled by the request, verified quote, {timestamp})\n- Basis: \"{}\" (via `{tool_name}`)\n\n",
        basis.trim()
    );
    append_entry(workspace_root, &entry)
}

/// Renders assumptions made without asking. Returns `None` when there are none.
#[must_use]
pub fn render_assumptions(
    context: &str,
    assumptions: &[String],
    timestamp: &str,
) -> Option<String> {
    let lines: Vec<String> = assumptions
        .iter()
        .map(|a| a.trim())
        .filter(|a| !a.is_empty())
        .map(|a| format!("- {a}"))
        .collect();
    if lines.is_empty() {
        return None;
    }
    Some(format!(
        "## Assumed for \"{}\" (not user-confirmed, {timestamp})\n{}\n\n",
        context.trim(),
        lines.join("\n")
    ))
}

/// Records assumptions the agent made without asking, for later review.
pub fn record_assumptions(
    workspace_root: &Path,
    context: &str,
    assumptions: &[String],
) -> std::io::Result<()> {
    let timestamp = chrono::Utc::now().format("%Y-%m-%d %H:%M UTC").to_string();
    match render_assumptions(context, assumptions, &timestamp) {
        Some(entry) => append_entry(workspace_root, &entry),
        None => Ok(()),
    }
}

/// Records or updates an explicit architectural/project decision in `.minicode/decisions.md` in place.
/// Supports addressing an existing decision by ID (`"D1"`, `"[D1]"`, `"D1: Title"`) or by title.
/// Returns the assigned or updated decision ID (e.g. `"D1"`).
pub fn upsert_decision(
    workspace_root: &Path,
    id_or_title: &str,
    decision: &str,
    rationale: Option<&str>,
) -> std::io::Result<String> {
    let timestamp = chrono::Utc::now().format("%Y-%m-%d %H:%M UTC").to_string();
    let trimmed = id_or_title.trim();
    let (explicit_num, raw_title) = parse_id_prefix(trimmed);

    let mut body_lines = vec![format!("- Decision: {}", decision.trim())];
    if let Some(rat) = rationale.map(str::trim).filter(|r| !r.is_empty()) {
        body_lines.push(format!("- Rationale: {rat}"));
    }

    let heading_prefix = match (explicit_num, raw_title.is_empty()) {
        (Some(n), true) => format!("[D{n}]"),
        (Some(n), false) => format!("[D{n}] {raw_title}"),
        (None, _) => trimmed.to_string(),
    };

    let entry = format!(
        "## {heading_prefix} (architectural decision, {timestamp})\n{}\n\n",
        body_lines.join("\n")
    );
    upsert_or_append_entry(workspace_root, &entry)
}

#[derive(Debug, Clone)]
struct ParsedDecisionSection {
    num: usize,
    subject: String,
    meta_suffix: String,
    body: String,
}

/// Parses an optional leading `[D<N>]` or `D<N>:` prefix from a heading string.
fn parse_id_prefix(s: &str) -> (Option<usize>, &str) {
    let t = s.trim();
    if let Some(rest) = t.strip_prefix("[D").or_else(|| t.strip_prefix("[d")) {
        if let Some(close) = rest.find(']') {
            if let Ok(n) = rest[..close].parse::<usize>() {
                return (Some(n), rest[close + 1..].trim());
            }
        }
    }
    if let Some(rest) = t.strip_prefix('D').or_else(|| t.strip_prefix('d')) {
        let digits_len = rest.chars().take_while(|c| c.is_ascii_digit()).count();
        if digits_len > 0 {
            let after_digits = &rest[digits_len..];
            if after_digits.is_empty()
                || after_digits.starts_with(':')
                || after_digits.starts_with(" (")
                || after_digits.starts_with(' ')
            {
                if let Ok(n) = rest[..digits_len].parse::<usize>() {
                    let clean_rest = after_digits.trim_start_matches(':').trim();
                    return (Some(n), clean_rest);
                }
            }
        }
    }
    (None, t)
}

/// Splits `<Subject> (<Meta>)` into `(<Subject>, "(<Meta>)")`, balancing nested parentheses in `<Meta>`.
fn split_subject_and_meta(s: &str) -> (&str, &str) {
    let t = s.trim();
    if t.ends_with(')') {
        let mut depth = 0usize;
        for (idx, ch) in t.char_indices().rev() {
            match ch {
                ')' => depth += 1,
                '(' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        if idx > 0 && t.as_bytes()[idx - 1] == b' ' {
                            return (t[..idx - 1].trim(), t[idx..].trim());
                        }
                        break;
                    }
                }
                _ => {}
            }
        }
    }
    (t, "")
}

fn parse_markdown_sections(content: &str) -> Vec<ParsedDecisionSection> {
    let mut sections = Vec::new();
    let mut current_heading: Option<String> = None;
    let mut current_body = Vec::new();
    let mut next_fallback_num = 1usize;

    let flush = |heading: String,
                 body: Vec<String>,
                 sections: &mut Vec<ParsedDecisionSection>,
                 fallback_num: &mut usize| {
        let (opt_num, after_id) = parse_id_prefix(&heading);
        let (subj, meta) = split_subject_and_meta(after_id);
        let num = opt_num.unwrap_or(*fallback_num);
        *fallback_num = (*fallback_num).max(num + 1);
        sections.push(ParsedDecisionSection {
            num,
            subject: subj.to_string(),
            meta_suffix: meta.to_string(),
            body: body.join("\n").trim().to_string(),
        });
    };

    for line in content.lines() {
        if let Some(h) = line.strip_prefix("## ") {
            if let Some(prev_h) = current_heading.take() {
                flush(
                    prev_h,
                    std::mem::take(&mut current_body),
                    &mut sections,
                    &mut next_fallback_num,
                );
            }
            current_heading = Some(h.trim().to_string());
        } else if current_heading.is_some() {
            current_body.push(line.to_string());
        }
    }
    if let Some(prev_h) = current_heading.take() {
        flush(prev_h, current_body, &mut sections, &mut next_fallback_num);
    }

    sections
}

fn render_markdown_sections(sections: &[ParsedDecisionSection]) -> String {
    let mut out = String::from(DECISIONS_HEADER);
    for sec in sections {
        if sec.meta_suffix.is_empty() {
            out.push_str(&format!(
                "## [D{}] {}\n{}\n\n",
                sec.num, sec.subject, sec.body
            ));
        } else {
            out.push_str(&format!(
                "## [D{}] {} {}\n{}\n\n",
                sec.num, sec.subject, sec.meta_suffix, sec.body
            ));
        }
    }
    out
}

fn append_entry(workspace_root: &Path, entry: &str) -> std::io::Result<()> {
    let _ = upsert_or_append_entry(workspace_root, entry)?;
    Ok(())
}

fn upsert_or_append_entry(workspace_root: &Path, entry: &str) -> std::io::Result<String> {
    let path = decisions_path(workspace_root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let existing_content = std::fs::read_to_string(&path).unwrap_or_default();
    let mut sections = parse_markdown_sections(&existing_content);

    let trimmed = entry.trim();
    let mut lines = trimmed.lines();
    let first_line = lines.next().unwrap_or("");
    let raw_heading = first_line.strip_prefix("## ").unwrap_or(first_line).trim();
    let body = lines.collect::<Vec<_>>().join("\n").trim().to_string();

    let (incoming_num, after_id) = parse_id_prefix(raw_heading);
    let (incoming_subj, incoming_meta) = split_subject_and_meta(after_id);

    // Match existing section by explicit D<N> ID first, then by case-insensitive canonical subject
    let existing_idx = if let Some(target_n) = incoming_num {
        sections.iter().position(|s| s.num == target_n)
    } else if !incoming_subj.is_empty() {
        sections
            .iter()
            .position(|s| s.subject.eq_ignore_ascii_case(incoming_subj))
    } else {
        None
    };

    let assigned_num = if let Some(idx) = existing_idx {
        let existing = &mut sections[idx];
        if !incoming_subj.is_empty() {
            existing.subject = incoming_subj.to_string();
        }
        if !incoming_meta.is_empty() {
            existing.meta_suffix = incoming_meta.to_string();
        }
        existing.body = body;
        existing.num
    } else {
        let next_num =
            incoming_num.unwrap_or_else(|| sections.iter().map(|s| s.num).max().unwrap_or(0) + 1);
        let subject = if incoming_subj.is_empty() {
            format!("Decision D{next_num}")
        } else {
            incoming_subj.to_string()
        };
        sections.push(ParsedDecisionSection {
            num: next_num,
            subject,
            meta_suffix: incoming_meta.to_string(),
            body,
        });
        next_num
    };

    let rendered = render_markdown_sections(&sections);
    std::fs::write(&path, rendered)?;
    Ok(format!("D{assigned_num}"))
}

/// Returns the (tail of the) decision record for prompt injection, if any.
#[must_use]
pub fn read_for_prompt(workspace_root: &Path) -> Option<String> {
    let content = std::fs::read_to_string(decisions_path(workspace_root)).ok()?;
    let body = content.trim();
    if body.is_empty() {
        return None;
    }
    if body.len() <= MAX_PROMPT_BYTES {
        return Some(body.to_string());
    }
    let mut start = body.len() - MAX_PROMPT_BYTES;
    while !body.is_char_boundary(start) {
        start += 1;
    }
    Some(format!("…(earlier decisions omitted)\n{}", &body[start..]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::inquiry::{InquiryAnswer, InquiryOption, InquiryQuestion};

    fn sample_request() -> InquiryRequest {
        let opt = |id: &str, label: &str| InquiryOption {
            id: id.into(),
            label: label.into(),
            description: None,
            recommended: false,
        };
        let q = |id: &str, header: &str, input_type, options| InquiryQuestion {
            id: id.into(),
            question: format!("Which {header}?"),
            header: Some(header.into()),
            input_type,
            is_multi_select: false,
            allow_custom: true,
            placeholder: None,
            default_value: None,
            options,
        };
        InquiryRequest {
            inquiry_id: "call_1".into(),
            title: "Project Foundation".into(),
            description: None,
            questions: vec![
                q(
                    "stack",
                    "Stack",
                    InquiryInputType::Choice,
                    vec![opt("rv", "React + Vite"), opt("html", "Static HTML")],
                ),
                q("key", "API Key", InquiryInputType::Secret, vec![]),
                q("theme", "Theme", InquiryInputType::Text, vec![]),
            ],
        }
    }

    fn sample_response() -> InquiryResponse {
        InquiryResponse {
            inquiry_id: "call_1".into(),
            answers: vec![
                InquiryAnswer {
                    question_id: "stack".into(),
                    selected_options: vec!["rv".into()],
                    custom_text: None,
                    masked: false,
                },
                InquiryAnswer {
                    question_id: "key".into(),
                    selected_options: vec![],
                    custom_text: Some("sk-secret".into()),
                    masked: true,
                },
                InquiryAnswer {
                    question_id: "theme".into(),
                    selected_options: vec![],
                    custom_text: Some("dark terminal".into()),
                    masked: false,
                },
            ],
            cancelled: false,
        }
    }

    #[test]
    fn test_render_maps_labels_and_skips_secrets() {
        let entry = render_entry(
            &sample_request(),
            &sample_response(),
            DecisionSource::User,
            "t",
        )
        .unwrap();
        assert!(entry.contains("- Stack: React + Vite"));
        assert!(entry.contains("- Theme: dark terminal"));
        assert!(!entry.contains("sk-secret"));
        assert!(!entry.contains("API Key"));
        assert!(entry.contains("answered by user"));
    }

    #[test]
    fn test_cancelled_is_not_recorded() {
        let mut resp = sample_response();
        resp.cancelled = true;
        assert!(render_entry(&sample_request(), &resp, DecisionSource::User, "t").is_none());
    }

    #[test]
    fn test_record_and_read_roundtrip() {
        let temp = tempfile::tempdir().unwrap();
        assert!(read_for_prompt(temp.path()).is_none());
        record(
            temp.path(),
            &sample_request(),
            &sample_response(),
            DecisionSource::AutoDefault,
        )
        .unwrap();
        let text = read_for_prompt(temp.path()).unwrap();
        assert!(text.contains("# Project Decisions"));
        assert!(text.contains("- Stack: React + Vite"));
        assert!(text.contains("not user-confirmed"));
    }

    #[test]
    fn test_render_assumptions_skips_blank_and_marks_unconfirmed() {
        assert!(render_assumptions("Plan", &["  ".to_string()], "t").is_none());
        let entry = render_assumptions("Notes persistence", &["Use a JSON file".to_string()], "t")
            .expect("entry");
        assert!(entry.contains("not user-confirmed"));
        assert!(entry.contains("- Use a JSON file"));
    }

    #[test]
    fn test_numbered_decision_ledger_in_place_update() {
        let temp = tempfile::tempdir().unwrap();

        // 1. First record an AutoDefault decision -> gets assigned [D1]
        record(
            temp.path(),
            &sample_request(),
            &sample_response(),
            DecisionSource::AutoDefault,
        )
        .unwrap();
        let text1 = read_for_prompt(temp.path()).unwrap();
        assert!(text1.contains("## [D1] Project Foundation"));
        assert!(text1.contains("not user-confirmed"));

        // 2. Record a second architectural decision -> gets assigned [D2]
        let id2 = upsert_decision(
            temp.path(),
            "Inline Key Prefix",
            "Store 8-byte big-endian u64 prefix on SymbolMatch",
            Some("Avoids heap string comparison on B-Tree postings scan"),
        )
        .unwrap();
        assert_eq!(id2, "D2");
        let text2 = read_for_prompt(temp.path()).unwrap();
        assert!(text2.contains("## [D2] Inline Key Prefix"));

        // 3. User now confirms/updates "Project Foundation" -> updates [D1] in place, removing "not user-confirmed"
        record(
            temp.path(),
            &sample_request(),
            &sample_response(),
            DecisionSource::User,
        )
        .unwrap();
        let text3 = read_for_prompt(temp.path()).unwrap();
        assert!(text3.contains("## [D1] Project Foundation (answered by user"));
        assert!(!text3.contains("not user-confirmed"));
        assert_eq!(text3.matches("## [D1]").count(), 1);

        // 4. Update D2 in place by ID reference ("D2")
        let id2_updated = upsert_decision(
            temp.path(),
            "D2",
            "Store 8-byte big-endian u64 prefix + disk snapshot fingerprint",
            None,
        )
        .unwrap();
        assert_eq!(id2_updated, "D2");
        let text4 = read_for_prompt(temp.path()).unwrap();
        assert_eq!(text4.matches("## [D2]").count(), 1);
        assert!(text4.contains("disk snapshot fingerprint"));
    }
}

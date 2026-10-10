//! Persistent record of user decisions captured through `ask_user`.
//!
//! Answers are appended to `.minicode/decisions.md` so they survive context
//! compaction and later turns/sessions, and are re-injected into the recency
//! context as `<user_decisions>`. Secret answers are never written.

use std::io::Write;
use std::path::{Path, PathBuf};

use crate::agent::inquiry::{InquiryInputType, InquiryRequest, InquiryResponse};

/// Workspace-relative location of the decision record.
pub const DECISIONS_FILE: &str = ".minicode/decisions.md";

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

fn append_entry(workspace_root: &Path, entry: &str) -> std::io::Result<()> {
    let path = decisions_path(workspace_root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let is_new = !path.exists();
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?;
    if is_new {
        file.write_all(b"# Project Decisions (answers from ask_user, verified request quotes, and unconfirmed assumptions)\n\n")?;
    }
    file.write_all(entry.as_bytes())?;
    Ok(())
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
}

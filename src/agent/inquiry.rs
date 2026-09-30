//! Domain models, protocol types, and asynchronous registry for user inquiries.
#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::oneshot;

/// Type of input requested from the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InquiryInputType {
    /// Select from a list of options (single or multi-choice).
    #[default]
    Choice,
    /// Freeform text write-in.
    Text,
    /// Masked secret entry (e.g. API keys, passwords, auth tokens).
    Secret,
}

/// An individual selectable option for an inquiry question.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InquiryOption {
    /// Identifier for option (defaults to label if omitted).
    pub id: String,
    /// Human-readable label for option.
    pub label: String,
    /// Optional explanatory note or detail.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Whether the agent recommends this option.
    #[serde(default)]
    pub recommended: bool,
}

/// A single question to ask the user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InquiryQuestion {
    /// Unique identifier for this question (e.g., "framework", "api_key").
    pub id: String,
    /// The prompt question text.
    pub question: String,
    /// Optional category header or breadcrumb (e.g. "Stack", "Credentials").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub header: Option<String>,
    /// The input format (Choice, Text, or Secret).
    #[serde(default)]
    pub input_type: InquiryInputType,
    /// Whether multiple options can be selected simultaneously.
    #[serde(default)]
    pub is_multi_select: bool,
    /// Whether the user can provide a custom write-in answer.
    #[serde(default)]
    pub allow_custom: bool,
    /// Optional placeholder hint for text/secret input.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
    /// Optional default answer value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_value: Option<String>,
    /// List of pre-defined options.
    #[serde(default)]
    pub options: Vec<InquiryOption>,
}

/// Structured inquiry request emitted by the agent loop or `ask_user` tool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InquiryRequest {
    /// Unique inquiry identifier (e.g. tool call ID).
    pub inquiry_id: String,
    /// Dialog title.
    pub title: String,
    /// Contextual explanation or instructions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Sequential or collective questions.
    pub questions: Vec<InquiryQuestion>,
}

impl InquiryRequest {
    /// Automatically resolves defaults when running in non-interactive mode (`-y`).
    pub fn auto_resolve_defaults(&self) -> InquiryResponse {
        let mut answers = Vec::with_capacity(self.questions.len());
        for q in &self.questions {
            let mut selected = Vec::new();
            let mut custom = None;

            if let Some(rec) = q.options.iter().find(|o| o.recommended) {
                selected.push(rec.id.clone());
            } else if let Some(first) = q.options.first() {
                selected.push(first.id.clone());
            } else if let Some(def) = &q.default_value {
                custom = Some(def.clone());
            } else {
                custom = Some("(default: not specified)".to_string());
            }

            answers.push(InquiryAnswer {
                question_id: q.id.clone(),
                selected_options: selected,
                custom_text: custom,
                masked: q.input_type == InquiryInputType::Secret,
            });
        }

        InquiryResponse {
            inquiry_id: self.inquiry_id.clone(),
            answers,
            cancelled: false,
        }
    }
}

/// Answer to a single question.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InquiryAnswer {
    /// The question ID this answer corresponds to.
    pub question_id: String,
    /// Selected option IDs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub selected_options: Vec<String>,
    /// Custom text entered by user.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_text: Option<String>,
    /// Whether the text should be masked in UI logs.
    #[serde(default)]
    pub masked: bool,
}

impl InquiryAnswer {
    /// Masked string representation for safe display in UI timelines/logs.
    pub fn display_value(&self) -> String {
        if self.masked {
            "••••••••".to_string()
        } else if let Some(text) = &self.custom_text {
            text.clone()
        } else {
            self.selected_options.join(", ")
        }
    }

    /// Raw unmasked value provided by the user.
    pub fn raw_value(&self) -> String {
        if let Some(text) = &self.custom_text {
            text.clone()
        } else {
            self.selected_options.join(", ")
        }
    }
}

/// Structured response returned by the user/TUI to the agent loop.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InquiryResponse {
    /// Unique inquiry ID matching the request.
    pub inquiry_id: String,
    /// Answers provided by the user.
    pub answers: Vec<InquiryAnswer>,
    /// True if the user dismissed or cancelled the dialog.
    pub cancelled: bool,
}

impl InquiryResponse {
    /// Formats the response as a JSON string for tool output.
    #[allow(clippy::wrong_self_convention)]
    pub fn into_tool_output(&self) -> String {
        if self.cancelled {
            return serde_json::json!({
                "status": "cancelled",
                "message": "User dismissed or cancelled the inquiry menu without selecting options."
            })
            .to_string();
        }

        let mut map = serde_json::Map::new();
        for ans in &self.answers {
            let val = if let Some(custom) = &ans.custom_text {
                serde_json::Value::String(custom.clone())
            } else if ans.selected_options.len() == 1 {
                serde_json::Value::String(ans.selected_options[0].clone())
            } else {
                serde_json::to_value(&ans.selected_options).unwrap_or(serde_json::Value::Null)
            };
            map.insert(ans.question_id.clone(), val);
        }

        serde_json::json!({
            "status": "answered",
            "answers": map
        })
        .to_string()
    }
}

/// Shared map of in-flight user inquiries: `inquiry_id` (tool_call.id) → responder.
pub type InquiryRegistry = Arc<Mutex<HashMap<String, oneshot::Sender<InquiryResponse>>>>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inquiry_models_serialization_and_defaults() {
        let question = InquiryQuestion {
            id: "framework".to_string(),
            question: "Which frontend framework would you like to use?".to_string(),
            header: Some("Stack".to_string()),
            input_type: InquiryInputType::Choice,
            is_multi_select: false,
            allow_custom: true,
            placeholder: None,
            default_value: Some("React 19".to_string()),
            options: vec![
                InquiryOption {
                    id: "react".to_string(),
                    label: "React 19 + Tailwind".to_string(),
                    description: Some("Modern React single page app".to_string()),
                    recommended: true,
                },
                InquiryOption {
                    id: "svelte".to_string(),
                    label: "Svelte 5 + Vite".to_string(),
                    description: Some("Lightweight reactive UI".to_string()),
                    recommended: false,
                },
            ],
        };

        let request = InquiryRequest {
            inquiry_id: "inq-123".to_string(),
            title: "Project Configuration".to_string(),
            description: Some("Please specify your application preferences.".to_string()),
            questions: vec![question],
        };

        // Auto-resolve non-interactive
        let auto_response = request.auto_resolve_defaults();
        assert_eq!(auto_response.inquiry_id, "inq-123");
        assert!(!auto_response.cancelled);
        assert_eq!(auto_response.answers.len(), 1);
        assert_eq!(auto_response.answers[0].question_id, "framework");
        assert_eq!(
            auto_response.answers[0].selected_options,
            vec!["react".to_string()]
        );

        // Output JSON serialization
        let output_json = auto_response.into_tool_output();
        assert!(output_json.contains("\"status\":\"answered\""));
        assert!(output_json.contains("\"framework\""));
    }

    #[test]
    fn test_inquiry_secret_masking() {
        let answer = InquiryAnswer {
            question_id: "api_key".to_string(),
            selected_options: vec![],
            custom_text: Some("sk-proj-supersecret123456789".to_string()),
            masked: true,
        };
        assert_eq!(answer.display_value(), "••••••••");
        assert_eq!(answer.raw_value(), "sk-proj-supersecret123456789");
    }
}

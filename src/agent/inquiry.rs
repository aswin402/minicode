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
    #[serde(default)]
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

impl InquiryOption {
    /// Returns the option identifier, falling back to `label` if `id` was omitted.
    pub fn resolved_id(&self) -> &str {
        if self.id.is_empty() {
            &self.label
        } else {
            &self.id
        }
    }
}

/// A single question to ask the user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InquiryQuestion {
    /// Unique identifier for this question (e.g., "framework", "api_key").
    #[serde(default)]
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

impl InquiryQuestion {
    /// Returns the question identifier, falling back to `header` or `question` if `id` was omitted.
    pub fn resolved_id(&self) -> &str {
        if !self.id.is_empty() {
            &self.id
        } else if let Some(h) = &self.header {
            h
        } else {
            &self.question
        }
    }
}

/// Structured inquiry request emitted by the agent loop or `ask_user` tool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InquiryRequest {
    /// Unique inquiry identifier (e.g. tool call ID).
    #[serde(default)]
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
    /// Resiliently parses an InquiryRequest from any LLM tool call payload, accommodating:
    /// - Serialized JSON string in `questions` (`"questions": "[{...}]"`)
    /// - Singular `question: "..."` shorthand
    /// - Options as simple strings `["Opt A", "Opt B"]` or objects `[{"label": "Opt A"}]`
    /// - Missing or empty title (defaulted to "User Inquiry")
    pub fn parse_from_value(val: &serde_json::Value) -> std::result::Result<Self, String> {
        let inquiry_id = val
            .get("inquiry_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let title = val
            .get("title")
            .and_then(|v| v.as_str())
            .filter(|s| !s.trim().is_empty())
            .unwrap_or("User Inquiry")
            .to_string();

        let description = val
            .get("description")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        // Parse questions flexibly: accommodate stringified JSON array
        let raw_questions_val: Option<serde_json::Value> =
            if let Some(q_str) = val.get("questions").and_then(|v| v.as_str()) {
                serde_json::from_str(q_str).ok()
            } else {
                val.get("questions").cloned()
            };

        let mut questions = Vec::new();

        if let Some(serde_json::Value::Array(arr)) = raw_questions_val {
            for item in arr {
                if let Ok(q) = Self::parse_question_item(&item) {
                    questions.push(q);
                }
            }
        } else if let Some(single_q) = val.get("question").and_then(|v| v.as_str()) {
            // Singular question shorthand
            questions.push(InquiryQuestion {
                id: "q1".to_string(),
                question: single_q.to_string(),
                header: None,
                input_type: InquiryInputType::Text,
                is_multi_select: false,
                allow_custom: true,
                placeholder: None,
                default_value: None,
                options: Vec::new(),
            });
        }

        if questions.is_empty() {
            return Err("ask_user requires at least one question in the 'questions' array or a 'question' prompt.".to_string());
        }

        Ok(Self {
            inquiry_id,
            title,
            description,
            questions,
        })
    }

    fn parse_question_item(
        item: &serde_json::Value,
    ) -> std::result::Result<InquiryQuestion, String> {
        if let Ok(mut q) = serde_json::from_value::<InquiryQuestion>(item.clone()) {
            // If options array in item was strings rather than objects, recover them
            if q.options.is_empty() {
                if let Some(opts_arr) = item.get("options").and_then(|v| v.as_array()) {
                    for opt in opts_arr {
                        if let Some(s) = opt.as_str() {
                            q.options.push(InquiryOption {
                                id: s.to_string(),
                                label: s.to_string(),
                                description: None,
                                recommended: false,
                            });
                        }
                    }
                }
            }
            return Ok(q);
        }

        // Fallback for minimal object {"question": "...", "options": [...]}
        let question_text = item
            .get("question")
            .or_else(|| item.get("title"))
            .or_else(|| item.get("prompt"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| "Question missing 'question' text".to_string())?;

        let id = item
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("q")
            .to_string();
        let header = item
            .get("header")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let input_type_str = item
            .get("input_type")
            .and_then(|v| v.as_str())
            .unwrap_or("choice");
        let input_type = match input_type_str {
            "text" => InquiryInputType::Text,
            "secret" => InquiryInputType::Secret,
            _ => InquiryInputType::Choice,
        };
        let is_multi_select = item
            .get("is_multi_select")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let allow_custom = item
            .get("allow_custom")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let placeholder = item
            .get("placeholder")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let default_value = item
            .get("default_value")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let mut options = Vec::new();
        if let Some(opts_arr) = item.get("options").and_then(|v| v.as_array()) {
            for opt in opts_arr {
                if let Some(s) = opt.as_str() {
                    options.push(InquiryOption {
                        id: s.to_string(),
                        label: s.to_string(),
                        description: None,
                        recommended: false,
                    });
                } else if let Ok(parsed_opt) = serde_json::from_value::<InquiryOption>(opt.clone())
                {
                    options.push(parsed_opt);
                }
            }
        }

        Ok(InquiryQuestion {
            id,
            question: question_text.to_string(),
            header,
            input_type,
            is_multi_select,
            allow_custom,
            placeholder,
            default_value,
            options,
        })
    }

    /// Automatically resolves defaults when running in non-interactive mode (`-y`).
    pub fn auto_resolve_defaults(&self) -> InquiryResponse {
        let mut answers = Vec::with_capacity(self.questions.len());
        for q in &self.questions {
            let mut selected = Vec::new();
            let mut custom = None;

            if q.is_multi_select {
                let recs: Vec<String> = q
                    .options
                    .iter()
                    .filter(|o| o.recommended)
                    .map(|o| o.resolved_id().to_string())
                    .collect();
                if !recs.is_empty() {
                    selected = recs;
                } else if let Some(def) = &q.default_value {
                    if q.options.iter().any(|o| o.resolved_id() == def) {
                        selected.push(def.clone());
                    } else {
                        custom = Some(def.clone());
                    }
                } else if let Some(first) = q.options.first() {
                    selected.push(first.resolved_id().to_string());
                } else {
                    custom = Some("(default: not specified)".to_string());
                }
            } else if let Some(rec) = q.options.iter().find(|o| o.recommended) {
                selected.push(rec.resolved_id().to_string());
            } else if let Some(def) = &q.default_value {
                if q.options.iter().any(|o| o.resolved_id() == def) {
                    selected.push(def.clone());
                } else {
                    custom = Some(def.clone());
                }
            } else if let Some(first) = q.options.first() {
                selected.push(first.resolved_id().to_string());
            } else {
                custom = Some("(default: not specified)".to_string());
            }

            answers.push(InquiryAnswer {
                question_id: q.resolved_id().to_string(),
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
    pub fn to_tool_output(&self) -> String {
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

    /// Backwards-compatible alias for `to_tool_output`.
    pub fn into_tool_output(self) -> String {
        self.to_tool_output()
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
        let output_json = auto_response.to_tool_output();
        assert!(output_json.contains("\"status\":\"answered\""));
        assert!(output_json.contains("\"framework\""));
    }

    #[test]
    fn test_inquiry_default_value_and_multiselect_resolution() {
        let q_default = InquiryQuestion {
            id: "db".to_string(),
            question: "Pick database".to_string(),
            header: None,
            input_type: InquiryInputType::Choice,
            is_multi_select: false,
            allow_custom: false,
            placeholder: None,
            default_value: Some("sqlite".to_string()),
            options: vec![
                InquiryOption {
                    id: "postgres".to_string(),
                    label: "PostgreSQL".to_string(),
                    description: None,
                    recommended: false,
                },
                InquiryOption {
                    id: "sqlite".to_string(),
                    label: "SQLite".to_string(),
                    description: None,
                    recommended: false,
                },
            ],
        };

        let q_multi = InquiryQuestion {
            id: "features".to_string(),
            question: "Pick features".to_string(),
            header: None,
            input_type: InquiryInputType::Choice,
            is_multi_select: true,
            allow_custom: false,
            placeholder: None,
            default_value: None,
            options: vec![
                InquiryOption {
                    id: "auth".to_string(),
                    label: "Auth".to_string(),
                    description: None,
                    recommended: true,
                },
                InquiryOption {
                    id: "logging".to_string(),
                    label: "Logging".to_string(),
                    description: None,
                    recommended: true,
                },
                InquiryOption {
                    id: "billing".to_string(),
                    label: "Billing".to_string(),
                    description: None,
                    recommended: false,
                },
            ],
        };

        let req = InquiryRequest {
            inquiry_id: "inq-priority".to_string(),
            title: "Setup".to_string(),
            description: None,
            questions: vec![q_default, q_multi],
        };

        let resp = req.auto_resolve_defaults();
        assert_eq!(resp.answers[0].selected_options, vec!["sqlite".to_string()]);
        assert_eq!(
            resp.answers[1].selected_options,
            vec!["auth".to_string(), "logging".to_string()]
        );
    }

    #[test]
    fn test_serde_roundtrip_for_inquiry_models() {
        let json_input = r#"{
            "inquiry_id": "req-99",
            "title": "API Key Prompt",
            "description": "Enter your key",
            "questions": [
                {
                    "id": "key",
                    "question": "Enter Anthropic API key",
                    "header": "Auth",
                    "input_type": "secret",
                    "is_multi_select": false,
                    "allow_custom": false,
                    "placeholder": "sk-ant-...",
                    "options": []
                }
            ]
        }"#;

        let parsed: InquiryRequest =
            serde_json::from_str(json_input).expect("Must parse valid JSON");
        assert_eq!(parsed.inquiry_id, "req-99");
        assert_eq!(parsed.questions[0].input_type, InquiryInputType::Secret);

        let serialized = serde_json::to_string(&parsed).expect("Must serialize");
        let parsed_again: InquiryRequest =
            serde_json::from_str(&serialized).expect("Must roundtrip");
        assert_eq!(parsed, parsed_again);
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

    #[test]
    fn test_parse_from_value_stringified_questions() {
        // Reproduce minitest31 exact failure payload where questions was passed as a string
        let payload = serde_json::json!({
            "title": "Landing Page Stack & Design Direction",
            "description": "minicode is a technical product",
            "questions": "[{\"header\": \"Tech Stack\", \"id\": \"stack\", \"input_type\": \"choice\", \"options\": [{\"description\": \"Static HTML\", \"id\": \"static\", \"label\": \"Modern Static HTML\", \"recommended\": true}, {\"description\": \"React Vite\", \"id\": \"react-vite\", \"label\": \"React Vite\"}], \"question\": \"Which tech stack?\"}]"
        });

        let req = InquiryRequest::parse_from_value(&payload)
            .expect("Must parse stringified questions successfully");
        assert_eq!(req.title, "Landing Page Stack & Design Direction");
        assert_eq!(req.questions.len(), 1);
        assert_eq!(req.questions[0].id, "stack");
        assert_eq!(req.questions[0].options.len(), 2);
        assert_eq!(req.questions[0].options[0].id, "static");
    }

    #[test]
    fn test_parse_from_value_single_question_and_string_options() {
        let payload = serde_json::json!({
            "question": "What database do you want?",
            "options": ["PostgreSQL", "SQLite", "MongoDB"]
        });

        let req = InquiryRequest::parse_from_value(&payload)
            .expect("Must parse single question shorthand");
        assert_eq!(req.title, "User Inquiry");
        assert_eq!(req.questions.len(), 1);
        assert_eq!(req.questions[0].question, "What database do you want?");
    }
}

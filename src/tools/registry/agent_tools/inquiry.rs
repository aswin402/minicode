//! Tool 186: `ask_user` - Human-in-the-loop interactive inquiry and clarification tool.

use crate::agent::inquiry::InquiryRequest;
use crate::agent::provider::ToolSchema;
use crate::error::Result;
use std::path::Path;

/// Tool name constant.
pub const ASK_USER_TOOL_NAME: &str = "ask_user";

/// Returns the schema for `ask_user`.
pub fn get_schemas() -> Vec<ToolSchema> {
    vec![ToolSchema {
        name: ASK_USER_TOOL_NAME.to_string(),
        description: "Your channel to whoever directs you (a human in the TUI, or the calling AI agent over --json-stream). Use it so you never silently decide what is theirs. Ask when something is not settled by the request, the workspace or recorded decisions AND guessing wrong is costly to undo: (1) clarify ambiguous/contradictory requirements or success criteria; (2) request data only they have (real content, business rules, accounts, credentials via input_type=secret); (3) user-owned decisions: stack, architecture, data model, API/spec contracts, design/style and UX behaviour, scope trade-offs, heavy dependencies, destructive or irreversible changes; (4) approve an approach or plan before large work (2-3 options, recommended first); (5) report a blocker with options after real attempts. Do NOT ask for facts you can find with tools or for cheap details you can change later; state an assumption instead. Batch the decisions you know now into ONE call (1-4 questions, 2-4 options each, recommended option first with recommended=true and a one-line reason); ask a new round only when answers open new costly decisions. The dialog always offers a free-text answer, so never add an 'Other' option. Answers are recorded in .minicode/decisions.md and shown in later turns; never re-ask them. Pauses until answered; in non-interactive mode (-y) the recommended option is selected and marked not user-confirmed.".to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "title": {
                    "type": "string",
                    "description": "Dialog header title describing the inquiry topic (e.g. 'Login feature', 'Dashboard redesign direction', 'Confirm removing /v1 API')."
                },
                "description": {
                    "type": "string",
                    "description": "Contextual explanation explaining why the agent is asking and how the choices will guide implementation."
                },
                "questions": {
                    "type": "array",
                    "description": "List of questions to present to the user.",
                    "items": {
                        "type": "object",
                        "properties": {
                            "id": {
                                "type": "string",
                                "description": "Unique machine-readable identifier for the question (e.g. 'framework', 'color_theme', 'api_key')."
                            },
                            "question": {
                                "type": "string",
                                "description": "The prompt question text to display."
                            },
                            "header": {
                                "type": "string",
                                "description": "Short category breadcrumb header (e.g. 'Architecture', 'Design', 'Credentials')."
                            },
                            "input_type": {
                                "type": "string",
                                "enum": ["choice", "text", "secret"],
                                "description": "Input format: 'choice' (default), 'text' for freeform string, or 'secret' for masked API key/token."
                            },
                            "is_multi_select": {
                                "type": "boolean",
                                "description": "Whether the user can select multiple options simultaneously (checkbox mode)."
                            },
                            "allow_custom": {
                                "type": "boolean",
                                "description": "Free-text answer is enabled by default; set false only to force a choice from the listed options."
                            },
                            "placeholder": {
                                "type": "string",
                                "description": "Placeholder hint text for text or secret inputs."
                            },
                            "default_value": {
                                "type": "string",
                                "description": "Default answer to use if skipped or running non-interactively."
                            },
                            "options": {
                                "type": "array",
                                "description": "List of choices for 'choice' questions.",
                                "items": {
                                    "type": "object",
                                    "properties": {
                                        "id": {
                                            "type": "string",
                                            "description": "Option identifier (defaults to label if omitted)."
                                        },
                                        "label": {
                                            "type": "string",
                                            "description": "Display label for the option."
                                        },
                                        "description": {
                                            "type": "string",
                                            "description": "Brief explanation of this option."
                                        },
                                        "recommended": {
                                            "type": "boolean",
                                            "description": "Set to true if this is the agent's recommended choice."
                                        }
                                    },
                                    "required": ["label"]
                                }
                            }
                        },
                        "required": ["question"]
                    }
                }
            },
            "required": ["title", "questions"]
        }),
    }]
}

/// Fallback dispatcher for `ask_user` if called outside the suspended agent loop.
pub async fn dispatch(
    tool_name: &str,
    args: &serde_json::Value,
    _workspace_root: &Path,
) -> Option<Result<String>> {
    if tool_name != ASK_USER_TOOL_NAME {
        return None;
    }

    let parsed_req = InquiryRequest::parse_from_value(args);
    match parsed_req {
        Ok(req) => {
            let auto_resp = req.auto_resolve_defaults();
            Some(Ok(auto_resp.into_tool_output()))
        }
        Err(err_msg) => Some(Err(crate::error::ToolError::InvalidArguments {
            name: ASK_USER_TOOL_NAME.to_string(),
            reason: err_msg,
        }
        .into())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ask_user_schema_invariants() {
        let schemas = get_schemas();
        assert_eq!(schemas.len(), 1);
        let s = &schemas[0];
        assert_eq!(s.name, "ask_user");
        assert!(s.description.contains("questions"));
        assert!(s.parameters["properties"]["questions"].is_object());
        assert!(s.parameters["properties"]["title"].is_object());
    }

    #[tokio::test]
    async fn test_ask_user_dispatch_fallback() {
        let args = serde_json::json!({
            "title": "Select Database",
            "questions": [{
                "id": "db",
                "question": "Which db?",
                "options": [{ "id": "sqlite", "label": "SQLite", "recommended": true }]
            }]
        });

        let res = dispatch("ask_user", &args, Path::new(".")).await;
        assert!(res.is_some());
        let output = res.unwrap().expect("Dispatch must succeed");
        assert!(output.contains("\"status\":\"answered\""));
        assert!(output.contains("\"sqlite\""));
    }

    #[tokio::test]
    async fn test_ask_user_dispatch_invalid_args() {
        let args = serde_json::json!({
            "title": "Missing questions"
        });

        let res = dispatch("ask_user", &args, Path::new(".")).await;
        assert!(res.is_some());
        assert!(res.unwrap().is_err());
    }

    #[tokio::test]
    async fn test_ask_user_dispatch_unrelated_tool() {
        let args = serde_json::json!({});
        let res = dispatch("unrelated_tool", &args, Path::new(".")).await;
        assert!(res.is_none());
    }
}

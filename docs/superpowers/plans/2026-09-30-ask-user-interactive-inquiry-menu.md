# Interactive Inquiry Menu & Question Tool (`ask_user`) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement Tool 186 (`ask_user`) and an interactive, minimal "Gum / GitHub CLI"-style progressive stepper TUI inquiry menu allowing minicode to query the user for requirements, architectural choices, opinions, custom write-ins, and masked API keys/secrets with zero hardcoding and zero round-trip guessing.

**Architecture:** Pure Rust Tokio async suspension model. When the LLM decides to ask clarifying questions or prompt for configuration/secrets, it calls `ask_user`. In non-interactive mode (`-y` / `--yes`), it auto-resolves to recommended or default values immediately. In interactive mode, `AgentLoop` suspends turn execution, registers a oneshot responder in `InquiryRegistry`, and emits `AgentEvent::UserInquiry`. The Ratatui TUI opens `ModalState::Inquiry`, providing a clean, progressive stepper with quick numeric selection (`1`-`9`), arrow navigation, and a self-contained in-card input box for custom write-ins and masked secrets (`••••••`). Upon user confirmation, the oneshot channel resumes turn execution with structured JSON answers.

**Tech Stack:** Rust 2021 Edition, Ratatui 0.29, Tokio async runtime (oneshot channels), Crossterm, Serde JSON, Tracing.

## Global Constraints

- **Language & Runtime:** Pure Rust 2021 Edition, Tokio multi-threaded async runtime.
- **Error Handling:** Use `thiserror` for crate errors and `Result<T, InquiryError>` / `anyhow::Result` at boundaries. Never use `.unwrap()` or `.expect()` in non-test library code (`src/`).
- **Tool Count Invariant:** `TOTAL_TOOL_COUNT` in `src/constants.rs` must increase from 185 to 186, strictly synchronized with `ToolRegistry::get_tool_schemas().len()`.
- **Concurrency Flags:** All cargo commands must use `-j 1` for check/test/clippy (`cargo check -j 1`, `cargo test -j 1`, `cargo clippy -j 1 -- -D warnings`), and `-j 2` for release builds (`cargo build --release -j 2`).
- **Zero Hardcoding:** All modal titles, descriptions, question headers, options, placeholders, and default values are 100% dynamically supplied by LLM tool arguments.
- **UI Aesthetic:** Option 1 ("The Gum / GitHub CLI" Progressive Stepper) with minimal borders, single-character cursor `❯`, numeric shortcuts `[1]`, `[2]`, collapsed summary checkmarks (`✔ Question: Answer`), and self-contained in-card input buffer with masked secrets (`••••••`).

---

### Task 1: Domain Models & Protocol Types (`InquiryRequest`, `InquiryQuestion`, `InquiryResponse`, `InquiryRegistry`, `AgentEvent::UserInquiry`)

**Files:**
- Create: `src/agent/inquiry.rs`
- Modify: `src/agent/types.rs:180-220`
- Modify: `src/agent/mod.rs:1-30`
- Modify: `src/logging/formatter.rs:210-230, 500-520`
- Test: `src/agent/inquiry.rs` (inline `#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: Serde, Tokio oneshot channel.
- Produces:
  - `pub enum InquiryInputType { Choice, Text, Secret }`
  - `pub struct InquiryOption { pub id: String, pub label: String, pub description: Option<String>, pub recommended: bool }`
  - `pub struct InquiryQuestion { pub id: String, pub question: String, pub header: Option<String>, pub input_type: InquiryInputType, pub is_multi_select: bool, pub allow_custom: bool, pub placeholder: Option<String>, pub default_value: Option<String>, pub options: Vec<InquiryOption> }`
  - `pub struct InquiryRequest { pub inquiry_id: String, pub title: String, pub description: Option<String>, pub questions: Vec<InquiryQuestion> }`
  - `pub struct InquiryAnswer { pub question_id: String, pub selected_options: Vec<String>, pub custom_text: Option<String>, pub masked: bool }`
  - `pub struct InquiryResponse { pub inquiry_id: String, pub answers: Vec<InquiryAnswer>, pub cancelled: bool }`
  - `pub type InquiryRegistry = std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, tokio::sync::oneshot::Sender<InquiryResponse>>>>;`
  - `AgentEvent::UserInquiry { turn_id: usize, tool_id: String, request: InquiryRequest }`

- [ ] **Step 1: Write the failing unit tests for domain models and serialization in `src/agent/inquiry.rs`**

```rust
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
        assert_eq!(auto_response.answers[0].selected_options, vec!["react".to_string()]);

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
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -j 1 --lib agent::inquiry::tests`
Expected: FAIL with module/type not found.

- [ ] **Step 3: Implement domain models in `src/agent/inquiry.rs` and wire into `src/agent/mod.rs`, `src/agent/types.rs`, and `src/logging/formatter.rs`**

Create `src/agent/inquiry.rs`:
```rust
//! Domain models, protocol types, and asynchronous registry for user inquiries.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::oneshot;

/// Type of input requested from the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InquiryInputType {
    /// Select from a list of options (single or multi-choice).
    Choice,
    /// Freeform text write-in.
    Text,
    /// Masked secret entry (e.g. API keys, passwords, auth tokens).
    Secret,
}

impl Default for InquiryInputType {
    fn default() -> Self {
        Self::Choice
    }
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
                serde_json::to_value(&ans.selected_options)
                    .unwrap_or(serde_json::Value::Null)
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
```

Update `src/agent/types.rs`:
Add variant to `AgentEvent`:
```rust
    #[serde(rename = "user_inquiry")]
    UserInquiry {
        turn_id: usize,
        tool_id: String,
        request: crate::agent::inquiry::InquiryRequest,
    },
```

Update `src/logging/formatter.rs`:
Add arms for `AgentEvent::UserInquiry` in formatted and plain log branches.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -j 1 --lib agent::inquiry::tests`
Expected: PASS with 2 passed tests.

- [ ] **Step 5: Commit**

```bash
git add src/agent/inquiry.rs src/agent/mod.rs src/agent/types.rs src/logging/formatter.rs
git commit -m "feat(inquiry): implement InquiryRequest domain models, protocol types, and AgentEvent::UserInquiry"
```

---

### Task 2: Tool 186 Schema & Registration (`ask_user`)

**Files:**
- Create: `src/tools/registry/agent_tools/inquiry.rs`
- Modify: `src/tools/registry/agent_tools/mod.rs:1-48`
- Modify: `src/constants.rs:953, 1100-1120`
- Modify: `src/tools/concurrency.rs:100-150`
- Test: `src/tools/registry/agent_tools/inquiry.rs` (inline test) & `cargo test -j 1 --lib constants::tool_count_validation::total_tool_count_matches_registry`

**Interfaces:**
- Consumes: `ToolSchema`, `ToolRegistry`, `InquiryRequest` from Task 1.
- Produces:
  - `pub fn get_schemas() -> Vec<ToolSchema>` in `inquiry.rs`
  - `pub async fn dispatch(...) -> Option<Result<String>>` in `inquiry.rs`
  - `TOTAL_TOOL_COUNT = 186` in `src/constants.rs`
  - `"ask_user" => ToolSafetyLevel::Mutating` in `src/tools/concurrency.rs`

- [ ] **Step 1: Write the failing test for `ask_user` tool schema and tool count**

In `src/tools/registry/agent_tools/inquiry.rs`:
```rust
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
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -j 1 --lib tools::registry::agent_tools::inquiry::tests`
Expected: FAIL with module not found.

- [ ] **Step 3: Implement Tool 186 (`ask_user`) in `src/tools/registry/agent_tools/inquiry.rs` and update registry exports, constants, and concurrency**

Create `src/tools/registry/agent_tools/inquiry.rs`:
```rust
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
        description: "Ask the user one or more questions to clarify requirements, select architecture/stack/theme choices, collect configuration or credentials, or solicit design opinions. Pauses turn execution until the user responds via the interactive inquiry menu or provides answers. In non-interactive mode (-y), returns recommended or default values immediately without hanging.".to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "title": {
                    "type": "string",
                    "description": "Dialog header title describing the inquiry topic (e.g. 'Project Stack & Theme Selection', 'Firebase Configuration')."
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
                                "description": "Whether to include an 'Other / Custom write-in' option."
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
                        "required": ["id", "question"]
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

    let parsed_req: std::result::Result<InquiryRequest, _> = serde_json::from_value(args.clone());
    match parsed_req {
        Ok(req) => {
            let auto_resp = req.auto_resolve_defaults();
            Some(Ok(auto_resp.into_tool_output()))
        }
        Err(e) => Some(Err(crate::error::ToolError::InvalidArguments(format!(
            "Invalid ask_user arguments: {}",
            e
        ))
        .into())),
    }
}
```

Update `src/tools/registry/agent_tools/mod.rs`:
- Add `pub mod inquiry;`
- Extend `get_schemas()` with `schemas.extend(inquiry::get_schemas());`
- Extend `dispatch()` with `if let Some(res) = inquiry::dispatch(tool_name, args, workspace_root).await { return Some(res); }`

Update `src/constants.rs`:
- Change `TOTAL_TOOL_COUNT` from 185 to 186.

Update `src/tools/concurrency.rs`:
- Add `"ask_user" => ToolSafetyLevel::Mutating,`

- [ ] **Step 4: Run tests to verify tool schema and tool count validation pass**

Run: `cargo test -j 1 --lib tools::registry::agent_tools::inquiry::tests`
Run: `cargo test -j 1 --lib constants::tool_count_validation::total_tool_count_matches_registry`
Expected: PASS on both tests.

- [ ] **Step 5: Commit**

```bash
git add src/tools/registry/agent_tools/inquiry.rs src/tools/registry/agent_tools/mod.rs src/constants.rs src/tools/concurrency.rs
git commit -m "feat(tools): register Tool 186 ask_user and update TOTAL_TOOL_COUNT to 186"
```

---

### Task 3: Agent Loop Suspension & Non-Interactive Resolution

**Files:**
- Modify: `src/agent/loop.rs:30-165, 1190-1250, 2190-2230`
- Modify: `src/main.rs:1530-1550`
- Test: `src/agent/loop.rs` (inline test for inquiry suspension and cancellation)

**Interfaces:**
- Consumes: `InquiryRegistry`, `InquiryRequest`, `InquiryResponse` from Task 1, `ask_user` tool from Task 2.
- Produces:
  - `pub fn inquiry_registry(&self) -> InquiryRegistry` on `AgentLoop`.
  - Suspension gate in `AgentLoop::execute_turn` intercepting `ask_user`.
  - Non-interactive auto-resolution when `!self.interactive_approvals` (e.g. `--yes`).

- [ ] **Step 1: Write the failing test for `AgentLoop` inquiry suspension and non-interactive auto-resolve**

In `src/agent/loop.rs`:
```rust
#[cfg(test)]
mod inquiry_tests {
    use super::*;
    use crate::agent::inquiry::*;

    #[tokio::test]
    async fn test_inquiry_registry_instantiation() {
        let registry: InquiryRegistry = Arc::new(Mutex::new(HashMap::new()));
        let (tx, rx) = tokio::sync::oneshot::channel::<InquiryResponse>();
        registry.lock().unwrap().insert("test-inq".to_string(), tx);

        let sender = registry.lock().unwrap().remove("test-inq");
        assert!(sender.is_some());
        let _ = sender.unwrap().send(InquiryResponse {
            inquiry_id: "test-inq".to_string(),
            answers: vec![],
            cancelled: false,
        });

        let resp = rx.await.unwrap();
        assert_eq!(resp.inquiry_id, "test-inq");
        assert!(!resp.cancelled);
    }
}
```

- [ ] **Step 2: Run test to verify compilation**

Run: `cargo test -j 1 --lib agent::r#loop::inquiry_tests`
Expected: PASS.

- [ ] **Step 3: Implement `pending_inquiries` suspension in `src/agent/loop.rs`**

1. In `src/agent/loop.rs`:
   Add field to `AgentLoop`:
   ```rust
   pending_inquiries: crate::agent::inquiry::InquiryRegistry,
   ```
   Initialize in `AgentLoop::new`:
   ```rust
   pending_inquiries: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
   ```
   Add accessor:
   ```rust
   /// Returns a handle to the in-flight inquiry registry for hosts (TUI/NDJSON).
   pub fn inquiry_registry(&self) -> crate::agent::inquiry::InquiryRegistry {
       self.pending_inquiries.clone()
   }
   ```
2. In `execute_turn`:
   When checking `tool_call.name == "ask_user"`:
   ```rust
   if tool_call.name == crate::tools::registry::agent_tools::inquiry::ASK_USER_TOOL_NAME {
       let mut request: crate::agent::inquiry::InquiryRequest =
           serde_json::from_value(tool_call.arguments.clone()).unwrap_or_else(|_| {
               crate::agent::inquiry::InquiryRequest {
                   inquiry_id: tool_call.id.clone(),
                   title: "User Inquiry".to_string(),
                   description: None,
                   questions: vec![],
               }
           });
       request.inquiry_id = tool_call.id.clone();

       if !self.interactive_approvals {
           // Non-interactive / headless auto-resolve
           let auto_resp = request.auto_resolve_defaults();
           let output = auto_resp.into_tool_output();
           let res_event = AgentEvent::ToolResult {
               turn_id,
               tool_id: tool_call.id.clone(),
               tool: tool_call.name.clone(),
               success: true,
               output: output.clone(),
               duration_ms: 0,
           };
           let _ = event_sender.send(res_event);
           turn_tool_results.push(crate::agent::types::ToolResult {
               tool_id: tool_call.id.clone(),
               tool_name: tool_call.name.clone(),
               success: true,
               output,
               display_output: String::new(),
               duration_ms: 0,
           });
           continue;
       }

       // Interactive suspension gate
       let (resp_tx, resp_rx) = tokio::sync::oneshot::channel::<crate::agent::inquiry::InquiryResponse>();
       self.pending_inquiries
           .lock()
           .unwrap_or_else(|p| p.into_inner())
           .insert(tool_call.id.clone(), resp_tx);

       let inq_event = AgentEvent::UserInquiry {
           turn_id,
           tool_id: tool_call.id.clone(),
           request: request.clone(),
       };
       let _ = event_sender.send(inq_event);

       let response = if let Some(cancel) = &cancel_token {
           tokio::select! {
               _ = cancel.cancelled() => None,
               r = resp_rx => r.ok(),
           }
       } else {
           resp_rx.await.ok()
       };

       self.pending_inquiries
           .lock()
           .unwrap_or_else(|p| p.into_inner())
           .remove(&tool_call.id);

       let output = match response {
           Some(resp) => resp.into_tool_output(),
           None => serde_json::json!({
               "status": "cancelled",
               "message": "Inquiry cancelled or timed out."
           }).to_string(),
       };

       let res_event = AgentEvent::ToolResult {
           turn_id,
           tool_id: tool_call.id.clone(),
           tool: tool_call.name.clone(),
           success: true,
           output: output.clone(),
           duration_ms: 0,
       };
       let _ = event_sender.send(res_event);
       turn_tool_results.push(crate::agent::types::ToolResult {
           tool_id: tool_call.id.clone(),
           tool_name: tool_call.name.clone(),
           success: true,
           output,
           display_output: String::new(),
           duration_ms: 0,
       });
       continue;
   }
   ```
3. In `src/main.rs`:
   Bind `inquiries = agent.inquiry_registry();` where appropriate.

- [ ] **Step 4: Run test to verify it compiles cleanly**

Run: `cargo test -j 1 --lib agent::r#loop::inquiry_tests`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/agent/loop.rs src/main.rs
git commit -m "feat(agent): implement inquiry suspension gate and non-interactive auto-resolve in AgentLoop"
```

---

### Task 4: Interactive TUI Stepper Modal & In-Card Input

**Files:**
- Create: `src/ui/modals/inquiry.rs`
- Modify: `src/ui/modals/mod.rs:1-170, 500-600`
- Modify: `src/app/mod.rs:250-290, 580-620, 1250-1280`
- Modify: `src/app/modals.rs:1300-1420`
- Test: `src/ui/modals/inquiry.rs` (inline navigation, selection, and masking tests)

**Interfaces:**
- Consumes: `InquiryRequest`, `InquiryResponse`, `InquiryAnswer`, `InquiryModalState`.
- Produces:
  - `InquiryModalState` in `src/ui/modals/inquiry.rs`
  - `ModalState::Inquiry(InquiryModalState)` in `src/ui/modals/mod.rs`
  - Render function: `pub fn render_inquiry_modal(...)`
  - Key handling in `src/app/modals.rs`

- [ ] **Step 1: Write the failing tests for `InquiryModalState` navigation, toggle, in-card input, and secret masking**

In `src/ui/modals/inquiry.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::inquiry::*;

    fn dummy_request() -> InquiryRequest {
        InquiryRequest {
            inquiry_id: "inq-1".to_string(),
            title: "Website Scaffolding".to_string(),
            description: Some("Choose your preferred stack.".to_string()),
            questions: vec![
                InquiryQuestion {
                    id: "framework".to_string(),
                    question: "Choose UI framework".to_string(),
                    header: Some("Stack".to_string()),
                    input_type: InquiryInputType::Choice,
                    is_multi_select: false,
                    allow_custom: true,
                    placeholder: None,
                    default_value: None,
                    options: vec![
                        InquiryOption {
                            id: "react".to_string(),
                            label: "React 19".to_string(),
                            description: Some("Vite + Tailwind".to_string()),
                            recommended: true,
                        },
                        InquiryOption {
                            id: "vue".to_string(),
                            label: "Vue 3".to_string(),
                            description: Some("Nuxt / Pinia".to_string()),
                            recommended: false,
                        },
                    ],
                },
                InquiryQuestion {
                    id: "api_key".to_string(),
                    question: "Enter Anthropic API Key".to_string(),
                    header: Some("Credentials".to_string()),
                    input_type: InquiryInputType::Secret,
                    is_multi_select: false,
                    allow_custom: false,
                    placeholder: Some("sk-ant-api03-...".to_string()),
                    default_value: None,
                    options: vec![],
                },
            ],
        }
    }

    #[test]
    fn test_modal_stepper_progression() {
        let req = dummy_request();
        let mut state = InquiryModalState::from_request(1, "tool-1", req);

        assert_eq!(state.current_question_idx, 0);
        assert_eq!(state.selected_option_idx, 0);

        // Select first option (React)
        let resp = state.confirm_selection();
        assert!(resp.is_none(), "Should advance to next question, not finish");
        assert_eq!(state.current_question_idx, 1);
        assert_eq!(state.answers.len(), 1);
        assert_eq!(state.answers[0].selected_options, vec!["react".to_string()]);

        // Second question is Secret: typing custom text
        state.handle_char('s');
        state.handle_char('k');
        state.handle_char('-');
        assert_eq!(state.custom_input_buffer, "sk-");

        let finish = state.confirm_selection();
        assert!(finish.is_some(), "Should complete all questions");
        let final_resp = finish.unwrap();
        assert_eq!(final_resp.answers.len(), 2);
        assert_eq!(final_resp.answers[1].custom_text, Some("sk-".to_string()));
        assert!(final_resp.answers[1].masked);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -j 1 --lib ui::modals::inquiry::tests`
Expected: FAIL with module not found.

- [ ] **Step 3: Implement `src/ui/modals/inquiry.rs` with Option 1 Gum/CLI Stepper aesthetic and in-card input**

Create `src/ui/modals/inquiry.rs`:
```rust
//! Minimalist "Gum / GitHub CLI" Progressive Stepper Modal for user inquiries.

use crate::agent::inquiry::{InquiryAnswer, InquiryInputType, InquiryOption, InquiryQuestion, InquiryRequest, InquiryResponse};
use crate::ui::theme::Theme;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

/// State for the in-TUI interactive inquiry modal.
#[derive(Debug, Clone)]
pub struct InquiryModalState {
    pub turn_id: usize,
    pub tool_id: String,
    pub request: InquiryRequest,
    pub current_question_idx: usize,
    pub selected_option_idx: usize,
    pub answers: Vec<InquiryAnswer>,
    pub selected_multi_indices: Vec<usize>,
    pub is_typing_custom: bool,
    pub custom_input_buffer: String,
    pub custom_cursor: usize,
}

impl InquiryModalState {
    pub fn from_request(turn_id: usize, tool_id: &str, request: InquiryRequest) -> Self {
        let is_text_or_secret = request.questions.first().map_or(false, |q| {
            q.input_type == InquiryInputType::Text || q.input_type == InquiryInputType::Secret || q.options.is_empty()
        });

        Self {
            turn_id,
            tool_id: tool_id.to_string(),
            request,
            current_question_idx: 0,
            selected_option_idx: 0,
            answers: Vec::new(),
            selected_multi_indices: Vec::new(),
            is_typing_custom: is_text_or_secret,
            custom_input_buffer: String::new(),
            custom_cursor: 0,
        }
    }

    pub fn current_question(&self) -> Option<&InquiryQuestion> {
        self.request.questions.get(self.current_question_idx)
    }

    pub fn total_options_count(&self) -> usize {
        if let Some(q) = self.current_question() {
            let mut count = q.options.len();
            if q.allow_custom && q.input_type == InquiryInputType::Choice {
                count += 1; // "Other / Custom write-in"
            }
            count
        } else {
            0
        }
    }

    pub fn next_option(&mut self) {
        let total = self.total_options_count();
        if total > 0 {
            self.selected_option_idx = (self.selected_option_idx + 1) % total;
        }
    }

    pub fn prev_option(&mut self) {
        let total = self.total_options_count();
        if total > 0 {
            if self.selected_option_idx == 0 {
                self.selected_option_idx = total.saturating_sub(1);
            } else {
                self.selected_option_idx -= 1;
            }
        }
    }

    pub fn toggle_multi(&mut self) {
        if let Some(q) = self.current_question() {
            if q.is_multi_select {
                if let Some(pos) = self.selected_multi_indices.iter().position(|&i| i == self.selected_option_idx) {
                    self.selected_multi_indices.remove(pos);
                } else {
                    self.selected_multi_indices.push(self.selected_option_idx);
                }
            }
        }
    }

    pub fn handle_char(&mut self, c: char) {
        self.custom_input_buffer.insert(self.custom_cursor, c);
        self.custom_cursor += 1;
    }

    pub fn handle_backspace(&mut self) {
        if self.custom_cursor > 0 && self.custom_cursor <= self.custom_input_buffer.len() {
            self.custom_input_buffer.remove(self.custom_cursor - 1);
            self.custom_cursor -= 1;
        }
    }

    pub fn confirm_selection(&mut self) -> Option<InquiryResponse> {
        let q = match self.current_question() {
            Some(q) => q.clone(),
            None => return Some(self.build_response(false)),
        };

        let is_custom_option = q.allow_custom && self.selected_option_idx == q.options.len();
        let is_text_or_secret = q.input_type == InquiryInputType::Text || q.input_type == InquiryInputType::Secret;

        if (is_custom_option || is_text_or_secret) && !self.is_typing_custom {
            self.is_typing_custom = true;
            return None;
        }

        // Record answer
        let answer = if self.is_typing_custom || is_text_or_secret {
            let val = self.custom_input_buffer.trim().to_string();
            let final_val = if val.is_empty() {
                q.default_value.clone().unwrap_or_else(|| "(none)".to_string())
            } else {
                val
            };
            InquiryAnswer {
                question_id: q.id.clone(),
                selected_options: vec![],
                custom_text: Some(final_val),
                masked: q.input_type == InquiryInputType::Secret,
            }
        } else if q.is_multi_select {
            let selected: Vec<String> = self.selected_multi_indices
                .iter()
                .filter_map(|&idx| q.options.get(idx).map(|o| o.id.clone()))
                .collect();
            InquiryAnswer {
                question_id: q.id.clone(),
                selected_options: selected,
                custom_text: None,
                masked: false,
            }
        } else {
            let selected = q.options.get(self.selected_option_idx)
                .map(|o| o.id.clone())
                .unwrap_or_else(|| "default".to_string());
            InquiryAnswer {
                question_id: q.id.clone(),
                selected_options: vec![selected],
                custom_text: None,
                masked: false,
            }
        };

        self.answers.push(answer);
        self.current_question_idx += 1;
        self.selected_option_idx = 0;
        self.selected_multi_indices.clear();
        self.custom_input_buffer.clear();
        self.custom_cursor = 0;

        if let Some(next_q) = self.current_question() {
            self.is_typing_custom = next_q.input_type == InquiryInputType::Text || next_q.input_type == InquiryInputType::Secret || next_q.options.is_empty();
            None
        } else {
            Some(self.build_response(false))
        }
    }

    pub fn cancel(&self) -> InquiryResponse {
        self.build_response(true)
    }

    fn build_response(&self, cancelled: bool) -> InquiryResponse {
        InquiryResponse {
            inquiry_id: self.tool_id.clone(),
            answers: self.answers.clone(),
            cancelled,
        }
    }
}

/// Renders the Option 1 Progressive Stepper modal in Ratatui.
pub fn render_inquiry_modal(
    frame: &mut Frame,
    state: &InquiryModalState,
    theme: &Theme,
    area: Rect,
) {
    let width = 74.min(area.width.saturating_sub(4));
    let height = 24.min(area.height.saturating_sub(4));
    let x = (area.width.saturating_sub(width)) / 2;
    let y = (area.height.saturating_sub(height)) / 2;
    let modal_area = Rect::new(x, y, width, height);

    frame.render_widget(Clear, modal_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.primary))
        .title(Span::styled(
            format!(" ❓ {} ", state.request.title),
            Style::default().fg(theme.primary).add_modifier(Modifier::BOLD),
        ));
    frame.render_widget(block, modal_area);

    let inner = Rect::new(
        modal_area.x + 2,
        modal_area.y + 1,
        modal_area.width.saturating_sub(4),
        modal_area.height.saturating_sub(2),
    );

    let mut lines = Vec::new();

    // Contextual description
    if let Some(desc) = &state.request.description {
        lines.push(Line::from(Span::styled(desc, Style::default().fg(theme.muted))));
        lines.push(Line::from(""));
    }

    // Collapsed summary of previously answered questions
    for (idx, ans) in state.answers.iter().enumerate() {
        if let Some(q) = state.request.questions.get(idx) {
            let label = q.header.as_deref().unwrap_or(&q.id);
            lines.push(Line::from(vec![
                Span::styled("✔ ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                Span::styled(format!("{}: ", label), Style::default().fg(theme.foreground).add_modifier(Modifier::BOLD)),
                Span::styled(ans.display_value(), Style::default().fg(theme.secondary)),
            ]));
        }
    }

    if !state.answers.is_empty() {
        lines.push(Line::from(""));
    }

    // Active question
    if let Some(q) = state.current_question() {
        let step_badge = format!("[{}/{}] ", state.current_question_idx + 1, state.request.questions.len());
        let category = q.header.as_deref().unwrap_or("Question");

        lines.push(Line::from(vec![
            Span::styled(step_badge, Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)),
            Span::styled(format!("{}: ", category), Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)),
            Span::styled(&q.question, Style::default().fg(theme.foreground).add_modifier(Modifier::BOLD)),
        ]));
        lines.push(Line::from(""));

        // Choice options
        if q.input_type == InquiryInputType::Choice && !q.options.is_empty() {
            for (idx, opt) in q.options.iter().enumerate() {
                let is_selected = idx == state.selected_option_idx;
                let cursor = if is_selected { "❯ " } else { "  " };
                let num_key = if idx < 9 { format!("[{}] ", idx + 1) } else { "    ".to_string() };

                let check = if q.is_multi_select {
                    if state.selected_multi_indices.contains(&idx) { "[✔] " } else { "[ ] " }
                } else {
                    ""
                };

                let mut spans = vec![
                    Span::styled(cursor, if is_selected { Style::default().fg(theme.primary).add_modifier(Modifier::BOLD) } else { Style::default() }),
                    Span::styled(num_key, Style::default().fg(theme.muted)),
                    Span::styled(check, Style::default().fg(if is_selected { theme.primary } else { theme.muted })),
                    Span::styled(&opt.label, if is_selected { Style::default().fg(theme.foreground).add_modifier(Modifier::BOLD) } else { Style::default().fg(theme.foreground) }),
                ];

                if opt.recommended {
                    spans.push(Span::styled(" (Recommended)", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)));
                }

                if let Some(desc) = &opt.description {
                    spans.push(Span::styled(format!(" — {}", desc), Style::default().fg(theme.muted)));
                }

                lines.push(Line::from(spans));
            }

            if q.allow_custom {
                let custom_idx = q.options.len();
                let is_selected = custom_idx == state.selected_option_idx;
                let cursor = if is_selected { "❯ " } else { "  " };
                lines.push(Line::from(vec![
                    Span::styled(cursor, if is_selected { Style::default().fg(theme.primary).add_modifier(Modifier::BOLD) } else { Style::default() }),
                    Span::styled("[o] ", Style::default().fg(theme.muted)),
                    Span::styled("Other / Custom write-in...", if is_selected { Style::default().fg(theme.primary).add_modifier(Modifier::BOLD) } else { Style::default().fg(theme.muted) }),
                ]));
            }
        }

        // In-card input box
        if state.is_typing_custom || q.input_type == InquiryInputType::Text || q.input_type == InquiryInputType::Secret {
            lines.push(Line::from(""));
            let input_title = if q.input_type == InquiryInputType::Secret { "Enter Secret / API Key (Masked)" } else { "Enter Custom Answer" };
            lines.push(Line::from(Span::styled(format!("┌─ {} ───────────────────────┐", input_title), Style::default().fg(theme.secondary))));

            let display_text = if q.input_type == InquiryInputType::Secret {
                "•".repeat(state.custom_input_buffer.len())
            } else if state.custom_input_buffer.is_empty() {
                q.placeholder.clone().unwrap_or_else(|| "Type answer here...".to_string())
            } else {
                state.custom_input_buffer.clone()
            };

            let text_style = if state.custom_input_buffer.is_empty() && q.input_type != InquiryInputType::Secret {
                Style::default().fg(theme.muted)
            } else {
                Style::default().fg(theme.foreground).add_modifier(Modifier::BOLD)
            };

            lines.push(Line::from(vec![
                Span::styled("│ ❯ ", Style::default().fg(theme.primary)),
                Span::styled(display_text, text_style),
                Span::styled(" █", Style::default().fg(theme.accent)),
            ]));
            lines.push(Line::from(Span::styled("└──────────────────────────────────────────────┘", Style::default().fg(theme.secondary))));
        }
    }

    // Footer instructions
    let footer_text = if state.is_typing_custom {
        "[Enter] Submit  [Esc] Back to options"
    } else {
        "[↑/↓] Navigate  [1-9] Fast pick  [Space] Toggle  [Enter] Confirm  [Esc] Cancel"
    };

    let p = Paragraph::new(lines);
    frame.render_widget(p, Rect::new(inner.x, inner.y, inner.width, inner.height.saturating_sub(1)));

    let footer = Paragraph::new(Line::from(Span::styled(footer_text, Style::default().fg(theme.muted))))
        .alignment(Alignment::Center);
    frame.render_widget(footer, Rect::new(inner.x, inner.y + inner.height.saturating_sub(1), inner.width, 1));
}
```

Update `src/ui/modals/mod.rs`:
- Add `pub mod inquiry;`
- Add `Inquiry(inquiry::InquiryModalState)` to `ModalState`.
- Wire `ModalState::Inquiry(ref state) => inquiry::render_inquiry_modal(frame, state, theme, area),` in `render_modal`.

Update `src/app/mod.rs`:
- Add `pub inquiries: crate::agent::inquiry::InquiryRegistry,` to `App`.
- In `App::new`, initialize `self.inquiries = agent.inquiry_registry()`.
- On `AgentEvent::UserInquiry`:
  ```rust
  let inq_state = crate::ui::modals::inquiry::InquiryModalState::from_request(turn_id, &tool_id, request);
  self.modal = crate::ui::modal::ModalState::Inquiry(inq_state);
  ```
- Add `pub fn resolve_inquiry(&self, tool_id: &str, resp: crate::agent::inquiry::InquiryResponse)`:
  ```rust
  if let Some(sender) = self.inquiries.lock().unwrap_or_else(|p| p.into_inner()).remove(tool_id) {
      let _ = sender.send(resp);
  }
  ```

Update `src/app/modals.rs`:
Handle `ModalState::Inquiry(ref mut inq_state)`:
- `KeyCode::Esc`:
  - If `inq_state.is_typing_custom && inq_state.total_options_count() > 0`: `inq_state.is_typing_custom = false;`
  - Else:
    `let resp = inq_state.cancel();`
    `self.resolve_inquiry(&inq_state.tool_id.clone(), resp);`
    `self.modal = ModalState::None;`
- `KeyCode::Up`: `if !inq_state.is_typing_custom { inq_state.prev_option(); }`
- `KeyCode::Down`: `if !inq_state.is_typing_custom { inq_state.next_option(); }`
- `KeyCode::Char(c)`:
  - If typing custom: `inq_state.handle_char(c);`
  - Else if `c >= '1' && c <= '9'`:
    let num = (c as u8 - b'1') as usize;
    if num < inq_state.total_options_count() {
        inq_state.selected_option_idx = num;
        if let Some(resp) = inq_state.confirm_selection() {
            let tid = inq_state.tool_id.clone();
            self.resolve_inquiry(&tid, resp);
            self.modal = ModalState::None;
        }
    }
  - Else if `c == ' '`: `inq_state.toggle_multi();`
- `KeyCode::Backspace`: `if inq_state.is_typing_custom { inq_state.handle_backspace(); }`
- `KeyCode::Enter`:
  `if let Some(resp) = inq_state.confirm_selection() { let tid = inq_state.tool_id.clone(); self.resolve_inquiry(&tid, resp); self.modal = ModalState::None; }`

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -j 1 --lib ui::modals::inquiry::tests`
Expected: PASS with 1 passed test.

- [ ] **Step 5: Commit**

```bash
git add src/ui/modals/inquiry.rs src/ui/modals/mod.rs src/app/mod.rs src/app/modals.rs
git commit -m "feat(ui): implement Gum/CLI progressive stepper inquiry modal and keyboard navigation"
```

---

### Task 5: Orchestrator Prompt Instructions, Integration Tests & Global Release

**Files:**
- Modify: `src/agent/prompt.rs:400-450`
- Create: `tests/integration_ask_user_inquiry.rs`
- Modify: `onpkg_docs/core/todo.md`
- Test: `cargo test -j 1 --test integration_ask_user_inquiry`
- Release: `cargo build --release -j 2 && cp target/release/minicode ~/.local/bin/minicode`

**Interfaces:**
- Consumes: All modules from Tasks 1-4.
- Produces:
  - Prompt contract `<human_in_the_loop_inquiry>` instructing LLM when to use `ask_user`.
  - 4 end-to-end integration tests verifying schemas, non-interactive -y auto-resolution, oneshot suspension, and secret masking.
  - Deployed release binary `~/.local/bin/minicode` (`v0.3.42`).

- [ ] **Step 1: Write integration tests in `tests/integration_ask_user_inquiry.rs`**

```rust
use minicode::agent::inquiry::*;
use minicode::constants::TOTAL_TOOL_COUNT;
use minicode::tools::ToolRegistry;

#[test]
fn test_ask_user_invariants_and_schema_sync() {
    let schemas = ToolRegistry::get_tool_schemas();
    assert_eq!(
        schemas.len(),
        TOTAL_TOOL_COUNT,
        "TOTAL_TOOL_COUNT must match registry schema count exactly"
    );

    let ask_schema = schemas.iter().find(|s| s.name == "ask_user");
    assert!(ask_schema.is_some(), "ask_user must be present in registry schemas");
}

#[test]
fn test_ask_user_auto_resolution_in_headless_mode() {
    let req = InquiryRequest {
        inquiry_id: "inq-head-1".to_string(),
        title: "Database Configuration".to_string(),
        description: Some("Pick your DB".to_string()),
        questions: vec![
            InquiryQuestion {
                id: "db_driver".to_string(),
                question: "Choose SQL engine".to_string(),
                header: Some("Database".to_string()),
                input_type: InquiryInputType::Choice,
                is_multi_select: false,
                allow_custom: false,
                placeholder: None,
                default_value: None,
                options: vec![
                    InquiryOption {
                        id: "pg".to_string(),
                        label: "PostgreSQL 16".to_string(),
                        description: None,
                        recommended: true,
                    },
                    InquiryOption {
                        id: "sqlite".to_string(),
                        label: "SQLite 3".to_string(),
                        description: None,
                        recommended: false,
                    },
                ],
            },
        ],
    };

    let resp = req.auto_resolve_defaults();
    assert_eq!(resp.inquiry_id, "inq-head-1");
    assert!(!resp.cancelled);
    assert_eq!(resp.answers[0].selected_options, vec!["pg".to_string()]);
    let output = resp.into_tool_output();
    assert!(output.contains("\"status\":\"answered\""));
    assert!(output.contains("\"db_driver\":\"pg\""));
}

#[tokio::test]
async fn test_inquiry_interactive_suspension_channel() {
    let registry: InquiryRegistry = std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new()));
    let (tx, rx) = tokio::sync::oneshot::channel::<InquiryResponse>();

    registry.lock().unwrap().insert("call_99".to_string(), tx);

    let sender = registry.lock().unwrap().remove("call_99").expect("Sender must exist");
    let send_res = sender.send(InquiryResponse {
        inquiry_id: "call_99".to_string(),
        answers: vec![InquiryAnswer {
            question_id: "auth".to_string(),
            selected_options: vec!["jwt".to_string()],
            custom_text: None,
            masked: false,
        }],
        cancelled: false,
    });
    assert!(send_res.is_ok());

    let received = rx.await.expect("Must receive from channel");
    assert_eq!(received.inquiry_id, "call_99");
    assert_eq!(received.answers[0].selected_options, vec!["jwt".to_string()]);
}

#[test]
fn test_inquiry_masked_secrets() {
    let ans = InquiryAnswer {
        question_id: "secret_token".to_string(),
        selected_options: vec![],
        custom_text: Some("ghp_1234567890abcdef".to_string()),
        masked: true,
    };
    assert_eq!(ans.display_value(), "••••••••");
    assert_eq!(ans.raw_value(), "ghp_1234567890abcdef");
}
```

- [ ] **Step 2: Run test to verify it passes**

Run: `cargo test -j 1 --test integration_ask_user_inquiry`
Expected: PASS with 4 passed tests.

- [ ] **Step 3: Update `src/agent/prompt.rs` with Orchestrator Prompt Instructions**

Add `<interactive_inquiry>` section:
```markdown
<interactive_inquiry>
When a user prompt or task is broad, underspecified, or has multiple viable architectures, stacks, frameworks, themes, or design patterns (e.g. "make a website", "add auth", "create a dashboard"), or when configuration credentials (API keys, connection strings, auth tokens) are needed:
NEVER GUESS, assume, or waste turn cycles hallucinating requirements!
Call the `ask_user` tool immediately. Provide a clear title, description, and structured questions with recommended choices and optional custom write-ins. Minicode will present an interactive inquiry dialog to the user and resume with their explicit choices.
</interactive_inquiry>
```

- [ ] **Step 4: Verify formatting, clippy, and build release binary**

```bash
cargo fmt
cargo clippy -j 1 --bin minicode -- -D warnings
cargo test -j 1 --test integration_ask_user_inquiry
cargo build --release -j 2
cp target/release/minicode ~/.local/bin/minicode
~/.local/bin/minicode --version
```
Expected: Clean compile, all tests pass, binary copied to `~/.local/bin/minicode`.

- [ ] **Step 5: Update `onpkg_docs/core/todo.md` and commit**

Update `onpkg_docs/core/todo.md` with Phase 148 entries marked complete.

```bash
git add src/agent/prompt.rs tests/integration_ask_user_inquiry.rs onpkg_docs/core/todo.md
git commit -m "feat(inquiry): wire prompt instructions, add integration tests, and bump to Phase 148"
```

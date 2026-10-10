//! Foundation checkpoint: a state-based gate before the first file mutation in
//! a workspace that has no established project foundation.
//!
//! The gate never classifies the user's prompt with keywords. It fires only when:
//! - the workspace has no project manifest and no source files (pure filesystem check), and
//! - the foundation has not been settled this session.
//!
//! The foundation is settled in exactly two ways:
//! 1. `ask_user` is called (the user, or the headless default, decides), or
//! 2. a gated call carries a `foundation_basis` argument: a verbatim quote from
//!    the user's request showing that the request itself settles the foundation
//!    (it names the stack, or the deliverable is a single file). The harness
//!    *verifies* the quote is an actual substring of a user request this
//!    session. This is evidence verification, not keyword matching: the model
//!    makes the judgement, the harness only refuses unverifiable claims.
//!
//! Until settled, every gated call is returned with guidance instead of being
//! executed. `ask_user` is always available, so this can never deadlock.

use std::path::Path;

use crate::agent::orchestrator::{RepoState, WorkflowRouter};

/// Tools that create or modify project files and therefore commit to a foundation.
/// Shell tools are included because they can write files (`cat > index.html`),
/// which otherwise bypasses the gate; before a foundation exists there is
/// nothing to build or run, and read-only inspection has dedicated tools.
pub const FOUNDATION_GATED_TOOLS: &[&str] = &[
    "kit_stack_add",
    "write_file",
    "patch_file",
    "replace_in_files",
    "block_insert",
    "block_scaffold",
    "exec_cmd",
    "execute_dag",
];

/// Tools whose invocation records a foundation decision for the session.
pub const FOUNDATION_DECISION_TOOLS: &[&str] =
    &[crate::tools::registry::agent_tools::inquiry::ASK_USER_TOOL_NAME];

/// Argument name carrying the verbatim quote that settles the foundation.
pub const FOUNDATION_BASIS_ARG: &str = "foundation_basis";

/// Minimum number of alphanumeric characters a basis quote must contain.
const MIN_BASIS_ALNUM: usize = 3;

/// Maximum words in a basis quote. Evidence that settles a foundation is a
/// short phrase ("React + Vite", "a single index.html"); long quotes are the
/// signature of rationalising from descriptive copy.
const MAX_BASIS_WORDS: usize = 10;

/// Per-session gate state.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct FoundationGate {
    /// True once the foundation is settled (decision tool, verified basis,
    /// established workspace, or swarm worker).
    resolved: bool,
    /// Raw user requests of this session (without injected context), used to
    /// verify `foundation_basis` quotes.
    user_requests: Vec<String>,
    /// Number of checkpoints returned so far (for escalating guidance).
    checkpoints: u32,
}

/// Outcome of evaluating a tool call against the gate.
#[derive(Debug, PartialEq, Eq)]
pub enum GateDecision {
    /// Execute the tool call normally.
    Allow,
    /// Execute the tool call; the foundation was settled by this verified quote.
    AllowVerified(String),
    /// Return the contained guidance as the tool result instead of executing.
    Checkpoint(String),
}

/// Lowercases, collapses whitespace and trims quoting/trailing punctuation.
fn normalize(text: &str) -> String {
    let collapsed = text
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    collapsed
        .trim_matches(|c: char| {
            matches!(
                c,
                '"' | '\'' | '`' | '“' | '”' | '‘' | '’' | '.' | ',' | ';' | ':' | '!' | '?'
            ) || c.is_whitespace()
        })
        .to_string()
}

impl FoundationGate {
    /// Records the raw text of a user request (call once per user turn, before
    /// any context is prepended).
    pub fn record_user_request(&mut self, request: &str) {
        let trimmed = request.trim();
        if !trimmed.is_empty() {
            self.user_requests.push(normalize(trimmed));
        }
    }

    /// Returns the normalized basis if it is a short verbatim quote from a user
    /// request, otherwise the reason it was refused.
    fn verify_basis(&self, basis: &str) -> Result<String, &'static str> {
        let needle = normalize(basis);
        if needle.chars().filter(|c| c.is_alphanumeric()).count() < MIN_BASIS_ALNUM {
            return Err("it is empty or too short to be evidence");
        }
        if needle.split_whitespace().count() > MAX_BASIS_WORDS {
            return Err(
                "it is too long: evidence is the short phrase that names the stack or the single-file deliverable, not a copied description",
            );
        }
        if self.user_requests.iter().any(|req| req.contains(&needle)) {
            Ok(needle)
        } else {
            Err("it is not a verbatim, contiguous quote from the user's request")
        }
    }

    /// Evaluates a tool call. Must be called for every dispatched tool call so
    /// decision tools are recorded.
    pub fn evaluate(
        &mut self,
        workspace_root: &Path,
        tool_name: &str,
        arguments: &serde_json::Value,
    ) -> GateDecision {
        if self.resolved {
            return GateDecision::Allow;
        }
        if FOUNDATION_DECISION_TOOLS.contains(&tool_name) {
            self.resolved = true;
            return GateDecision::Allow;
        }
        if !FOUNDATION_GATED_TOOLS.contains(&tool_name) {
            return GateDecision::Allow;
        }
        // Swarm workers execute a parent's already-decided plan.
        if std::env::var("MINICODE_SWARM_DIR")
            .map(|s| !s.trim().is_empty())
            .unwrap_or(false)
        {
            self.resolved = true;
            return GateDecision::Allow;
        }
        let (state, _, _) = WorkflowRouter::scan_repository_state(workspace_root);
        if state == RepoState::ExistingCodebase {
            self.resolved = true;
            return GateDecision::Allow;
        }

        let basis = arguments
            .get(FOUNDATION_BASIS_ARG)
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty());
        let rejection = match basis {
            Some(basis) => match self.verify_basis(basis) {
                Ok(verified) => {
                    tracing::info!(tool = tool_name, basis = %verified, "foundation basis verified");
                    self.resolved = true;
                    return GateDecision::AllowVerified(verified);
                }
                Err(reason) => {
                    tracing::info!(tool = tool_name, basis, reason, "foundation basis rejected");
                    Some((basis, reason))
                }
            },
            None => None,
        };

        self.checkpoints += 1;
        tracing::info!(
            tool = tool_name,
            ?state,
            n = self.checkpoints,
            "foundation checkpoint triggered"
        );
        GateDecision::Checkpoint(checkpoint_message(tool_name, rejection, self.checkpoints))
    }

    /// Whether the gate has been resolved for this session.
    #[cfg(test)]
    #[must_use]
    pub fn is_resolved(&self) -> bool {
        self.resolved
    }
}

/// Adds the optional `foundation_basis` parameter to gated tool schemas while
/// the workspace has no established foundation, so the model can see and use
/// it. Established workspaces get unchanged schemas (no token cost).
pub fn augment_schemas(tools: &mut [crate::agent::providers::ToolSchema], workspace_root: &Path) {
    let (state, _, _) = WorkflowRouter::scan_repository_state(workspace_root);
    if state == RepoState::ExistingCodebase {
        return;
    }
    for tool in tools
        .iter_mut()
        .filter(|t| FOUNDATION_GATED_TOOLS.contains(&t.name.as_str()))
    {
        if let Some(props) = tool
            .parameters
            .get_mut("properties")
            .and_then(serde_json::Value::as_object_mut)
        {
            props.insert(
                FOUNDATION_BASIS_ARG.to_string(),
                serde_json::json!({
                    "type": "string",
                    "description": "Only for the first file, scaffold or shell command in an empty workspace when you did not ask_user: a short phrase (max 10 words) copied verbatim from the user's request that names the stack or the single-file deliverable. Verified by the harness."
                }),
            );
        }
    }
}

/// Builds the tool-result guidance returned when the checkpoint fires.
#[must_use]
pub fn checkpoint_message(
    tool_name: &str,
    rejection: Option<(&str, &str)>,
    attempt: u32,
) -> String {
    let mut message = format!(
        "`{tool_name}` was NOT executed. This workspace has no project foundation yet \
         (no manifest such as package.json, Cargo.toml, pyproject.toml or index.html) and the \
         foundation has not been settled this session (shell commands are held too, since they can write files; use read-only tools to inspect). Apply the Reversibility Rule."
    );
    match rejection {
        Some((basis, reason)) => message.push_str(&format!(
            " The `{FOUNDATION_BASIS_ARG}` you gave (\"{basis}\") was refused because {reason}."
        )),
        None if attempt >= 2 => message.push_str(&format!(
            " Your call again had no `{FOUNDATION_BASIS_ARG}` argument; re-issuing unchanged will \
             always be refused. Either call `ask_user`, or add `{FOUNDATION_BASIS_ARG}`."
        )),
        None => {}
    }
    serde_json::json!({
        "status": "foundation_checkpoint",
        "executed": false,
        "message": message,
        "options": [
            "The request does not explicitly name the stack/framework (or, for UI work, the visual direction): these are costly to reverse and belong to the user. Call `ask_user` once with all open decisions, recommended option first. Detailed feature copy, page sections, styling or product descriptions are NOT a stack choice — your own preference is not evidence.",
            format!("The request itself settles it (it explicitly names the stack, or the deliverable is a single file such as one script or one standalone page): re-issue the same call with an extra argument `{FOUNDATION_BASIS_ARG}` = the short phrase (max {MAX_BASIS_WORDS} words) copied verbatim from the user's request that names the stack or the single file, e.g. \"React + Vite\" or \"a single index.html\". The harness verifies it; only then is the call executed. It is recorded in .minicode/decisions.md.")
        ]
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn gate_with(request: &str) -> FoundationGate {
        let mut gate = FoundationGate::default();
        gate.record_user_request(request);
        gate
    }

    #[test]
    fn test_checkpoint_repeats_until_settled() {
        let temp = tempfile::tempdir().unwrap();
        let mut gate = gate_with("Build a landing page for my bakery");
        let args = json!({"path": "index.html"});
        assert_eq!(
            gate.evaluate(temp.path(), "read_file", &args),
            GateDecision::Allow
        );
        assert!(!gate.is_resolved());
        match gate.evaluate(temp.path(), "write_file", &args) {
            GateDecision::Checkpoint(msg) => {
                assert!(msg.contains("foundation_checkpoint"));
                assert!(msg.contains("ask_user"));
                assert!(msg.contains(FOUNDATION_BASIS_ARG));
            }
            other => panic!("expected checkpoint, got {other:?}"),
        }
        // Plain re-issue without evidence is refused again.
        assert!(matches!(
            gate.evaluate(temp.path(), "write_file", &args),
            GateDecision::Checkpoint(_)
        ));
        assert!(!gate.is_resolved());
    }

    #[test]
    fn test_verified_basis_settles_foundation() {
        let temp = tempfile::tempdir().unwrap();
        let mut gate = gate_with("Create a  React + Vite dashboard\nwith charts");
        let args = json!({"stack": "react-vite", "foundation_basis": "\"react + vite dashboard\""});
        assert_eq!(
            gate.evaluate(temp.path(), "kit_stack_add", &args),
            GateDecision::AllowVerified("react + vite dashboard".to_string())
        );
        assert!(gate.is_resolved());
        assert_eq!(
            gate.evaluate(temp.path(), "write_file", &json!({})),
            GateDecision::Allow
        );
    }

    #[test]
    fn test_fabricated_or_trivial_basis_is_rejected() {
        let temp = tempfile::tempdir().unwrap();
        let mut gate = gate_with("Build a landing page for my bakery with a menu section");
        for basis in ["use react-vite", "a", "  ", "..."] {
            let args = json!({"stack": "react-vite", "foundation_basis": basis});
            match gate.evaluate(temp.path(), "kit_stack_add", &args) {
                GateDecision::Checkpoint(_) => {}
                other => panic!("basis {basis:?} should be rejected, got {other:?}"),
            }
        }
        assert!(!gate.is_resolved());
    }

    #[test]
    fn test_decision_tools_resolve_gate() {
        let temp = tempfile::tempdir().unwrap();
        for decision in FOUNDATION_DECISION_TOOLS {
            let mut gate = FoundationGate::default();
            assert_eq!(
                gate.evaluate(temp.path(), decision, &json!({})),
                GateDecision::Allow
            );
            assert_eq!(
                gate.evaluate(temp.path(), "write_file", &json!({})),
                GateDecision::Allow
            );
        }
    }

    #[test]
    fn test_existing_project_is_never_checkpointed() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("package.json"), "{}").unwrap();
        let mut gate = FoundationGate::default();
        assert_eq!(
            gate.evaluate(temp.path(), "patch_file", &json!({})),
            GateDecision::Allow
        );

        let temp_code = tempfile::tempdir().unwrap();
        std::fs::write(temp_code.path().join("main.py"), "print(1)").unwrap();
        let mut gate2 = FoundationGate::default();
        assert_eq!(
            gate2.evaluate(temp_code.path(), "write_file", &json!({})),
            GateDecision::Allow
        );
    }

    #[test]
    fn test_long_descriptive_quote_is_rejected() {
        // Regression from the v0.3.69 eval: the model quoted a design sentence verbatim.
        let temp = tempfile::tempdir().unwrap();
        let request = "Build a landing page. The page should have excellent responsive behavior across desktop, tablet, and mobile.";
        let mut gate = gate_with(request);
        let args = json!({
            "stack_name": "static-website",
            "foundation_basis": "The page should have excellent responsive behavior across desktop, tablet, and mobile."
        });
        match gate.evaluate(temp.path(), "kit_stack_add", &args) {
            GateDecision::Checkpoint(msg) => assert!(msg.contains("too long")),
            other => panic!("expected checkpoint, got {other:?}"),
        }
        assert!(!gate.is_resolved());
    }

    #[test]
    fn test_augment_schemas_only_on_fresh_workspace() {
        use crate::agent::providers::ToolSchema;
        let make = || {
            vec![
                ToolSchema {
                    name: "write_file".into(),
                    description: String::new(),
                    parameters: json!({"type": "object", "properties": {"path": {"type": "string"}}}),
                },
                ToolSchema {
                    name: "read_file".into(),
                    description: String::new(),
                    parameters: json!({"type": "object", "properties": {}}),
                },
            ]
        };
        let fresh = tempfile::tempdir().unwrap();
        let mut tools = make();
        augment_schemas(&mut tools, fresh.path());
        assert!(tools[0].parameters["properties"][FOUNDATION_BASIS_ARG].is_object());
        assert!(tools[1].parameters["properties"][FOUNDATION_BASIS_ARG].is_null());

        let existing = tempfile::tempdir().unwrap();
        std::fs::write(existing.path().join("Cargo.toml"), "[package]").unwrap();
        let mut tools = make();
        augment_schemas(&mut tools, existing.path());
        assert!(tools[0].parameters["properties"][FOUNDATION_BASIS_ARG].is_null());
    }

    #[test]
    fn test_shell_write_is_checkpointed_on_fresh_workspace() {
        // Regression from the v0.3.70 eval: `exec_cmd cat > index.html` bypassed the gate.
        let temp = tempfile::tempdir().unwrap();
        let mut gate = gate_with("Build a landing page for my bakery");
        let args = json!({"command": "cat > index.html <<'EOF'\n<html></html>\nEOF"});
        assert!(matches!(
            gate.evaluate(temp.path(), "exec_cmd", &args),
            GateDecision::Checkpoint(_)
        ));
    }

    #[test]
    fn test_docs_only_workspace_is_checkpointed() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("README.md"), "# idea").unwrap();
        let mut gate = FoundationGate::default();
        assert!(matches!(
            gate.evaluate(temp.path(), "write_file", &json!({})),
            GateDecision::Checkpoint(_)
        ));
    }
}

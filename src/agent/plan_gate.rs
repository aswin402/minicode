//! Plan approval checkpoint: the user (or calling agent) approves a large plan
//! before its first edit, mirroring Claude Code's plan mode / `ExitPlanMode`
//! and Cline's Plan→Act switch.
//!
//! Purely state-based: it reacts to a successful `create_plan` with at least
//! [`PLAN_APPROVAL_MIN_STEPS`] steps. It never inspects prompt text. The plan is
//! approved by the next `ask_user` round (the model presents the plan and asks
//! approve / revise). Small plans and swarm workers are never held, and
//! `agent.plan_approval = false` disables the checkpoint.

/// Plans with at least this many steps need approval before their first edit.
pub const PLAN_APPROVAL_MIN_STEPS: usize = 3;

/// Tools that start executing a plan by changing project files.
pub const PLAN_GATED_TOOLS: &[&str] = &[
    "kit_stack_add",
    "write_file",
    "patch_file",
    "replace_in_files",
    "block_insert",
    "block_scaffold",
];

/// Tools that represent verifiable implementation or runtime verification actions.
pub const VERIFIABLE_ACTION_TOOLS: &[&str] = &[
    "write_file",
    "patch_file",
    "replace_in_files",
    "exec_cmd",
    "kit_stack_add",
    "block_scaffold",
    "block_insert",
    "minitask",
    "git_commit",
];

/// Returns true if the tool call performs a verifiable implementation or verification action.
#[must_use]
pub fn is_verifiable_action_tool(name: &str) -> bool {
    VERIFIABLE_ACTION_TOOLS.contains(&name) || name.starts_with("browser_")
}

/// Per-session plan approval state.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct PlanGate {
    /// Step count of a plan created but not yet approved.
    pending_steps: Option<usize>,
    /// True if `ask_user` was already invoked prior to `create_plan` (e.g., greenfield consultation).
    pre_approved: bool,
}

impl PlanGate {
    /// Records a tool call that has just been executed successfully.
    pub fn observe_success(&mut self, tool_name: &str, arguments: &serde_json::Value) {
        if tool_name == crate::tools::registry::agent_tools::inquiry::ASK_USER_TOOL_NAME {
            if self.pending_steps.is_some() {
                self.pending_steps = None;
            } else {
                self.pre_approved = true;
            }
            return;
        }
        if tool_name == "create_plan" {
            let steps = ["steps", "tasks", "plan"]
                .iter()
                .find_map(|k| arguments.get(*k).and_then(serde_json::Value::as_array))
                .map_or(0, Vec::len);
            let is_swarm_worker = std::env::var("MINICODE_SWARM_DIR")
                .map(|s| !s.trim().is_empty())
                .unwrap_or(false);
            if self.pre_approved {
                self.pre_approved = false;
                self.pending_steps = None;
            } else {
                self.pending_steps =
                    (steps >= PLAN_APPROVAL_MIN_STEPS && !is_swarm_worker).then_some(steps);
            }
        }
    }

    /// Returns checkpoint guidance if `tool_name` would start an unapproved large plan.
    #[must_use]
    pub fn check(&self, tool_name: &str) -> Option<String> {
        let steps = self.pending_steps?;
        if !PLAN_GATED_TOOLS.contains(&tool_name) {
            return None;
        }
        tracing::info!(
            tool = tool_name,
            steps,
            "plan approval checkpoint triggered"
        );
        Some(
            serde_json::json!({
                "status": "plan_approval_checkpoint",
                "executed": false,
                "message": format!(
                    "`{tool_name}` was NOT executed. You created a {steps}-step plan that has not been \
                     approved. Large work starts only after whoever directs you approves the approach: \
                     call `ask_user` once, summarising the plan (goal, key steps, any decisions or \
                     assumptions it relies on). You MUST provide at least 2 distinct options: Option 1: 'Approve plan — proceed with implementation' (recommended=true), Option 2: 'Revise plan — adjust scope or milestones' (plus any specific alternative choices). Include any open architectural, visual, or structural decisions in the same round. If the user asks for changes, update the plan with `create_plan` before editing."
                ),
            })
            .to_string(),
        )
    }

    /// Whether a plan is waiting for approval.
    #[cfg(test)]
    #[must_use]
    pub fn is_pending(&self) -> bool {
        self.pending_steps.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn plan(n: usize) -> serde_json::Value {
        json!({"title": "t", "steps": (0..n).map(|i| format!("step {i}")).collect::<Vec<_>>()})
    }

    #[test]
    fn test_large_plan_requires_approval_until_ask_user() {
        let mut gate = PlanGate::default();
        gate.observe_success("create_plan", &plan(6));
        assert!(gate.is_pending());
        let msg = gate.check("write_file").expect("held");
        assert!(msg.contains("plan_approval_checkpoint"));
        assert!(msg.contains("ask_user"));
        // Read-only and shell tools are not held.
        assert!(gate.check("read_file").is_none());
        assert!(gate.check("exec_cmd").is_none());
        gate.observe_success("ask_user", &json!({}));
        assert!(gate.check("write_file").is_none());
    }

    #[test]
    fn test_small_plan_is_not_held() {
        let mut gate = PlanGate::default();
        gate.observe_success("create_plan", &plan(PLAN_APPROVAL_MIN_STEPS - 1));
        assert!(gate.check("patch_file").is_none());
    }

    #[test]
    fn test_new_large_plan_after_approval_needs_new_approval() {
        let mut gate = PlanGate::default();
        gate.observe_success("create_plan", &plan(PLAN_APPROVAL_MIN_STEPS));
        gate.observe_success("ask_user", &json!({}));
        gate.observe_success("create_plan", &plan(7));
        assert!(gate.check("write_file").is_some());
    }

    #[test]
    fn test_ask_user_before_create_plan_satisfies_approval() {
        let mut gate = PlanGate::default();
        gate.observe_success("ask_user", &json!({}));
        gate.observe_success("create_plan", &plan(18));
        assert!(!gate.is_pending());
        assert!(gate.check("write_file").is_none());
    }

    #[test]
    fn test_is_verifiable_action_tool() {
        assert!(is_verifiable_action_tool("write_file"));
        assert!(is_verifiable_action_tool("patch_file"));
        assert!(is_verifiable_action_tool("exec_cmd"));
        assert!(is_verifiable_action_tool("kit_stack_add"));
        assert!(is_verifiable_action_tool("browser_navigate"));
        assert!(is_verifiable_action_tool("browser_screenshot"));
        assert!(is_verifiable_action_tool("browser_debug_logs"));
        assert!(is_verifiable_action_tool("minitask"));
        assert!(is_verifiable_action_tool("git_commit"));

        assert!(!is_verifiable_action_tool("read_file"));
        assert!(!is_verifiable_action_tool("grep_search"));
        assert!(!is_verifiable_action_tool("ask_user"));
        assert!(!is_verifiable_action_tool("update_progress"));
    }
}

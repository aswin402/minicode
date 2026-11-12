//! Dynamic Swarm Planner — decomposes high-level user objectives into an acyclic task DAG.
//!
//! Uses repository context analysis and prompt engineering to dynamically synthesize
//! specialized worker roles, disjoint file boundaries, verification commands, and task dependencies
//! with zero hardcoded domain tables.

use std::path::Path;
use tracing::info;

use crate::agent::provider::{CompletionOptions, Provider};
use crate::agent::swarm::models::{SwarmError, SwarmPlan};
use crate::agent::types::Message;

fn short_swarm_id() -> String {
    format!("swarm_{}", &uuid::Uuid::new_v4().simple().to_string()[..8])
}

/// Autonomous planner that synthesizes contract-first task DAGs from natural language objectives.
pub struct SwarmPlanner;

impl SwarmPlanner {
    /// Generates the system prompt instructing the LLM on the decomposition protocol.
    pub fn build_planning_prompt(workspace_context: &str) -> String {
        format!(
            r#"<swarm_decomposition_protocol>
You are the Lead Swarm Architect. Your job is to decompose the user's software engineering objective into an optimal Directed Acyclic Graph (DAG) of parallelizable, testable subagent tasks.

{workspace_context}

### CORE INVARIANTS:
1. ZERO HARDCODING: Dynamically synthesize task IDs, specialized role titles, instructions, and file boundaries tailored to the specific language, stack, and domain.
2. CONCURRENCY MAXIMIZATION: Tasks that do NOT depend on each other MUST NOT list dependencies so they run in parallel waves.
3. DISJOINT FILE BOUNDARIES: Specify explicit relative file boundaries/globs (e.g. ["src/api/**"], ["src/ui/**"]) to prevent parallel workers from clobbering each other. Parallel workers in the same wave MUST NEVER share file boundaries or modify the same files (including shared index files like `__init__.py`, `index.ts`, `mod.rs`). Shared exports or integrations must be assigned to an integrator task in a subsequent wave.
4. VERIFICATION GATES: Define an automated test or check command for each task (e.g. `cargo test`, `pytest tests/...`, `npm test`, or script verification) so the worker self-validates before completion.
5. ARTIFACT HANDOFFS: Specify contract files (e.g. `openapi.json`, `types.ts`, `schema.sql`) that downstream tasks will consume.
6. NO CIRCULAR DEPENDENCIES: The graph must be strictly acyclic.

### OUTPUT JSON SCHEMA:
Return ONLY a valid JSON object matching this schema without extraneous prose:
{{
  "id": "swarm_<unique_short_id>",
  "title": "<High-level Swarm Title>",
  "objective": "<The original objective>",
  "tasks": [
    {{
      "id": "t1_core",
      "title": "<Short task name>",
      "role_title": "<Specialized Role Title, e.g. FastAPI Database Architect>",
      "instructions": "<Focused instructions, persona, and technical constraints for this worker>",
      "prompt": "<Specific implementation task and requirements>",
      "file_boundaries": ["<relative/path/or/glob/**>"],
      "dependencies": [],
      "check_command": "<automated verification command, e.g. pytest tests/test_core.py>",
      "expected_artifacts": ["<relative/path/to/contract_or_output>"]
    }}
  ]
}}
</swarm_decomposition_protocol>"#
        )
    }

    /// Extracts workspace context (frameworks, files, package manager) to inform the planner.
    pub fn gather_workspace_context(workspace_root: &Path) -> String {
        crate::context::workspace_profile::WorkspaceProfile::describe_for_planner(workspace_root)
    }

    /// Plans a swarm by prompting the LLM provider and validating the resulting DAG.
    pub async fn plan_with_provider(
        workspace_root: &Path,
        objective: &str,
        provider: &dyn Provider,
        model: &str,
    ) -> Result<SwarmPlan, SwarmError> {
        let ws_context = Self::gather_workspace_context(workspace_root);
        let system_prompt = Self::build_planning_prompt(&ws_context);

        let user_prompt = format!(
            "Please analyze this objective and synthesize an optimal parallel SwarmPlan DAG:\n\nObjective: {}",
            objective
        );

        let messages = vec![Message::system(system_prompt), Message::user(user_prompt)];

        info!(model = %model, "Decomposing objective into Swarm DAG via LLM provider");
        let options = CompletionOptions::new(model);
        let response_text = provider
            .completion(&messages, &[], &options)
            .await
            .map_err(|e| SwarmError::PlanningFailed(e.to_string()))?;

        Self::parse_plan_json(&response_text, objective)
    }

    /// Extracts and parses a SwarmPlan from raw LLM text (handling markdown code blocks).
    pub fn parse_plan_json(
        raw_text: &str,
        original_objective: &str,
    ) -> Result<SwarmPlan, SwarmError> {
        let trimmed = raw_text.trim();

        let fenced = if let Some(start) = trimmed.find("```json") {
            let rest = &trimmed[start + 7..];
            Some(rest.find("```").map_or(rest, |end| &rest[..end]).trim())
        } else if let Some(start) = trimmed.find("```") {
            let rest = &trimmed[start + 3..];
            Some(rest.find("```").map_or(rest, |end| &rest[..end]).trim())
        } else {
            None
        };

        let brace_slice = trimmed.find('{').and_then(|s| {
            trimmed
                .rfind('}')
                .filter(|&e| e >= s)
                .map(|e| &trimmed[s..=e])
        });

        let json_str = fenced.or(brace_slice).unwrap_or(trimmed);

        let mut plan: SwarmPlan = serde_json::from_str(json_str)
            .or_else(|first_err| {
                if let Some(fallback) = brace_slice.filter(|&b| b != json_str) {
                    serde_json::from_str(fallback).map_err(|_| first_err)
                } else {
                    Err(first_err)
                }
            })
            .map_err(|e| {
                SwarmError::PlanningFailed(format!(
                    "Failed to parse SwarmPlan JSON: {} (Input: {})",
                    e, json_str
                ))
            })?;

        // Fill empty fields if omitted by LLM
        if plan.id.is_empty() {
            plan.id = short_swarm_id();
        }
        if plan.objective.is_empty() {
            plan.objective = original_objective.to_string();
        }
        if plan.created_at.is_empty() {
            plan.created_at = chrono::Utc::now().to_rfc3339();
        }

        // Validate plan integrity
        plan.validate()?;

        info!(
            swarm_id = %plan.id,
            tasks_count = plan.tasks.len(),
            "Successfully validated dynamic SwarmPlan DAG"
        );

        Ok(plan)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_plan_json_with_markdown_fences() {
        let raw = r#"
Here is the synthesized DAG execution plan for your request:

```json
{
  "id": "swarm_abc123",
  "title": "Full Stack API & Web App",
  "objective": "Build modern CRUD app",
  "tasks": [
    {
      "id": "t1_api",
      "title": "REST API Server",
      "role_title": "FastAPI Architect",
      "instructions": "Build endpoints",
      "prompt": "Create REST API",
      "file_boundaries": ["backend/**"],
      "dependencies": [],
      "check_command": "pytest backend",
      "expected_artifacts": ["backend/openapi.json"]
    },
    {
      "id": "t2_frontend",
      "title": "React Dashboard",
      "role_title": "Frontend UI Specialist",
      "instructions": "Consume API",
      "prompt": "Create React dashboard",
      "file_boundaries": ["frontend/**"],
      "dependencies": ["t1_api"],
      "check_command": "npm test",
      "expected_artifacts": []
    }
  ]
}
```

Hope this helps!
"#;

        let plan =
            SwarmPlanner::parse_plan_json(raw, "Build modern CRUD app").expect("should parse json");
        assert_eq!(plan.id, "swarm_abc123");
        assert_eq!(plan.tasks.len(), 2);
        assert_eq!(plan.tasks[0].id, "t1_api");
        assert_eq!(plan.tasks[1].dependencies, vec!["t1_api".to_string()]);
    }
}

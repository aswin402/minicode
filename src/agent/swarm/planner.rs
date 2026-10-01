//! Dynamic Swarm Planner — decomposes high-level user objectives into an acyclic task DAG.
//!
//! Uses repository context analysis and prompt engineering to dynamically synthesize
//! specialized worker roles, disjoint file boundaries, verification commands, and task dependencies
//! with zero hardcoded domain tables.

use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;
use tracing::info;

use crate::agent::provider::{CompletionOptions, Provider};
use crate::agent::swarm::models::{SwarmError, SwarmPlan, SwarmTaskSpec};
use crate::agent::types::Message;

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
        let mut lines = Vec::new();
        lines.push("### WORKSPACE CONTEXT:".to_string());

        // Check for onpkg.json or minikit.json
        if let Ok(content) = std::fs::read_to_string(workspace_root.join("onpkg.json")) {
            if let Ok(json) = serde_json::from_str::<Value>(&content) {
                let name = json
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("project");
                let pm = json
                    .get("package_manager")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                let runtime = json
                    .get("runtime")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                lines.push(format!("- Project Name: {}", name));
                lines.push(format!("- Runtime / Ecosystem: {}", runtime));
                lines.push(format!("- Package Manager: {}", pm));
            }
        } else if let Ok(content) = std::fs::read_to_string(workspace_root.join("Cargo.toml")) {
            lines.push("- Primary Ecosystem: Rust (Cargo)".to_string());
            if content.contains("tokio") {
                lines.push("- Frameworks: Tokio Async Runtime".to_string());
            }
        } else if let Ok(content) = std::fs::read_to_string(workspace_root.join("package.json")) {
            lines.push("- Primary Ecosystem: JavaScript / TypeScript (Node/Bun/npm)".to_string());
            if content.contains("react") {
                lines.push("- Frameworks: React".to_string());
            }
            if content.contains("vite") {
                lines.push("- Build Tool: Vite".to_string());
            }
        } else if workspace_root.join("pyproject.toml").exists()
            || workspace_root.join("requirements.txt").exists()
        {
            lines.push("- Primary Ecosystem: Python (pytest, venv)".to_string());
        }

        // Top level directory layout
        if let Ok(entries) = std::fs::read_dir(workspace_root) {
            let mut top_dirs = Vec::new();
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if !name.starts_with('.')
                    && name != "target"
                    && name != "node_modules"
                    && name != "venv"
                {
                    top_dirs.push(name);
                }
            }
            if !top_dirs.is_empty() {
                lines.push(format!("- Existing Directories: {}", top_dirs.join(", ")));
            }
        }

        lines.join("\n")
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

        // 1. Try stripping markdown ```json ... ``` or ``` ... ```
        let json_str = if let Some(start) = trimmed.find("```json") {
            let rest = &trimmed[start + 7..];
            if let Some(end) = rest.find("```") {
                rest[..end].trim()
            } else {
                rest.trim()
            }
        } else if let Some(start) = trimmed.find("```") {
            let rest = &trimmed[start + 3..];
            if let Some(end) = rest.find("```") {
                rest[..end].trim()
            } else {
                rest.trim()
            }
        } else if let Some(start) = trimmed.find('{') {
            if let Some(end) = trimmed.rfind('}') {
                &trimmed[start..=end]
            } else {
                trimmed
            }
        } else {
            trimmed
        };

        let mut plan: SwarmPlan = serde_json::from_str(json_str).map_err(|e| {
            SwarmError::PlanningFailed(format!(
                "Failed to parse SwarmPlan JSON: {} (Input: {})",
                e, json_str
            ))
        })?;

        // Fill empty fields if omitted by LLM
        if plan.id.is_empty() {
            plan.id = format!(
                "swarm_{}",
                uuid::Uuid::new_v4()
                    .to_string()
                    .chars()
                    .take(8)
                    .collect::<String>()
            );
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

    /// Generates a sensible fallback DAG plan if provider calls are unavailable (e.g. mock/offline tests).
    pub fn generate_fallback_plan(objective: &str) -> SwarmPlan {
        let short_id = uuid::Uuid::new_v4()
            .to_string()
            .chars()
            .take(8)
            .collect::<String>();
        let swarm_id = format!("swarm_{}", short_id);

        let t1 = SwarmTaskSpec {
            id: "t1_architecture".to_string(),
            title: "Core Architecture & Data Contracts".to_string(),
            role_title: "Principal System Architect".to_string(),
            instructions: "Design core models, types, and foundation schemas for the objective."
                .to_string(),
            prompt: format!(
                "Implement core models and data structures for: {}",
                objective
            ),
            file_boundaries: vec!["src/core/**".to_string(), "src/models/**".to_string()],
            dependencies: vec![],
            check_command: None,
            expected_artifacts: vec!["src/models/mod.rs".to_string()],
            workspace_mode: None,
            max_iterations: None,
        };

        let t2 = SwarmTaskSpec {
            id: "t2_implementation".to_string(),
            title: "Feature Implementation & Integration".to_string(),
            role_title: "Full-Stack Implementation Specialist".to_string(),
            instructions: "Implement feature business logic consuming the core models from t1."
                .to_string(),
            prompt: format!("Implement feature logic and interfaces for: {}", objective),
            file_boundaries: vec!["src/**".to_string()],
            dependencies: vec!["t1_architecture".to_string()],
            check_command: None,
            expected_artifacts: vec![],
            workspace_mode: None,
            max_iterations: None,
        };

        let t3 = SwarmTaskSpec {
            id: "t3_verification".to_string(),
            title: "Automated Test Suite & Verification".to_string(),
            role_title: "Senior QA & Test Engineer".to_string(),
            instructions:
                "Write comprehensive unit and integration tests verifying all requirements."
                    .to_string(),
            prompt: format!("Write automated tests verifying: {}", objective),
            file_boundaries: vec!["tests/**".to_string()],
            dependencies: vec!["t2_implementation".to_string()],
            check_command: None,
            expected_artifacts: vec![],
            workspace_mode: None,
            max_iterations: None,
        };

        SwarmPlan {
            id: swarm_id,
            title: format!(
                "Swarm Plan: {}",
                objective.chars().take(40).collect::<String>()
            ),
            objective: objective.to_string(),
            tasks: vec![t1, t2, t3],
            created_at: chrono::Utc::now().to_rfc3339(),
            metadata: HashMap::new(),
        }
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

    #[test]
    fn test_fallback_plan_is_valid() {
        let plan = SwarmPlanner::generate_fallback_plan("Build an authentication microservice");
        assert!(plan.validate().is_ok());
        assert_eq!(plan.tasks.len(), 3);
        let waves = plan.calculate_waves().expect("waves should calculate");
        assert_eq!(waves.len(), 3);
    }
}

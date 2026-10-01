//! End-to-end integration tests for the native Multi-Agent Swarm Orchestrator (`minicode swarm`).
//!
//! Validates dynamic DAG planning, Tarjan wave scheduling, dependency unblocking,
//! failure propagation, and Mermaid executive report generation.

use minicode::agent::swarm::{
    SwarmExecutionState, SwarmPlan, SwarmPlanner, SwarmReporter, SwarmTaskOutcome, SwarmTaskSpec,
    SwarmTaskStatus,
};
use std::collections::HashMap;

fn make_test_task(id: &str, title: &str, role: &str, deps: Vec<&str>) -> SwarmTaskSpec {
    SwarmTaskSpec {
        id: id.to_string(),
        title: title.to_string(),
        role_title: role.to_string(),
        instructions: format!("Implement {}", title),
        prompt: format!("Build {}", title),
        file_boundaries: vec![format!("src/{}/**", id)],
        dependencies: deps.into_iter().map(String::from).collect(),
        check_command: Some("cargo test".to_string()),
        expected_artifacts: vec![format!("src/{}/artifact.json", id)],
        workspace_mode: None,
        max_iterations: Some(10),
    }
}

#[test]
fn test_swarm_plan_validation_and_tarjan_waves() {
    let plan = SwarmPlan {
        id: "swarm_fullstack_mvp".to_string(),
        title: "Full-Stack Web MVP".to_string(),
        objective: "Build SaaS with Rust backend, React frontend, and e2e Playwright tests"
            .to_string(),
        tasks: vec![
            make_test_task(
                "t1_db_schema",
                "Database Schema",
                "PostgreSQL Architect",
                vec![],
            ),
            make_test_task(
                "t2_design_tokens",
                "Design System",
                "UI/UX Designer",
                vec![],
            ),
            make_test_task(
                "t3_backend_api",
                "Actix REST API",
                "Rust Backend Engineer",
                vec!["t1_db_schema"],
            ),
            make_test_task(
                "t4_frontend_ui",
                "React Dashboard",
                "Frontend Specialist",
                vec!["t2_design_tokens", "t3_backend_api"],
            ),
            make_test_task(
                "t5_e2e_tests",
                "Playwright E2E",
                "QA Test Engineer",
                vec!["t4_frontend_ui"],
            ),
        ],
        created_at: chrono::Utc::now().to_rfc3339(),
        metadata: HashMap::new(),
    };

    // 1. Validation should succeed
    assert!(plan.validate().is_ok());

    // 2. Topological order must place t1 before t3, t2/t3 before t4, t4 before t5
    let topo = plan.topological_order().expect("toposort must succeed");
    let pos = |id: &str| topo.iter().position(|x| x == id).unwrap();

    assert!(pos("t1_db_schema") < pos("t3_backend_api"));
    assert!(pos("t3_backend_api") < pos("t4_frontend_ui"));
    assert!(pos("t2_design_tokens") < pos("t4_frontend_ui"));
    assert!(pos("t4_frontend_ui") < pos("t5_e2e_tests"));

    // 3. Parallel Waves calculation
    let waves = plan
        .calculate_waves()
        .expect("wave calculation must succeed");
    assert_eq!(waves.len(), 4);

    // Wave 1: t1 and t2 (0 dependencies, run concurrently)
    assert_eq!(waves[0].len(), 2);
    assert!(waves[0].contains(&"t1_db_schema".to_string()));
    assert!(waves[0].contains(&"t2_design_tokens".to_string()));

    // Wave 2: t3 (depends on t1)
    assert_eq!(waves[1], vec!["t3_backend_api".to_string()]);

    // Wave 3: t4 (depends on t2 and t3)
    assert_eq!(waves[2], vec!["t4_frontend_ui".to_string()]);

    // Wave 4: t5 (depends on t4)
    assert_eq!(waves[3], vec!["t5_e2e_tests".to_string()]);
}

#[test]
fn test_swarm_planner_fallback_and_json_parsing() {
    let raw_llm_output = r#"
Here is the optimal SwarmPlan DAG for your objective:

```json
{
  "id": "swarm_live_test",
  "title": "Search Engine Indexer",
  "objective": "Build BM25 search engine with inverted index",
  "tasks": [
    {
      "id": "t1_inverted_index",
      "title": "Inverted Index Core",
      "role_title": "Information Retrieval Specialist",
      "instructions": "Implement tokenizer, posting lists, and BM25 scorer",
      "prompt": "Build inverted index in Rust",
      "file_boundaries": ["src/index/**"],
      "dependencies": [],
      "check_command": "cargo test --lib index",
      "expected_artifacts": ["src/index/lib.rs"]
    },
    {
      "id": "t2_search_cli",
      "title": "Search CLI Interface",
      "role_title": "CLI Systems Engineer",
      "instructions": "Wire CLI search command",
      "prompt": "Implement search query CLI command",
      "file_boundaries": ["src/cli/**"],
      "dependencies": ["t1_inverted_index"],
      "check_command": "cargo test --test integration_search",
      "expected_artifacts": []
    }
  ]
}
```
"#;

    let plan = SwarmPlanner::parse_plan_json(raw_llm_output, "Build BM25 search engine")
        .expect("JSON parse must succeed");
    assert_eq!(plan.id, "swarm_live_test");
    assert_eq!(plan.tasks.len(), 2);
    assert_eq!(plan.tasks[0].role_title, "Information Retrieval Specialist");
    assert_eq!(
        plan.tasks[1].dependencies,
        vec!["t1_inverted_index".to_string()]
    );

    // Test fallback planner
    let fallback = SwarmPlanner::generate_fallback_plan("Refactor authentication session tokens");
    assert!(fallback.validate().is_ok());
    assert_eq!(fallback.tasks.len(), 3);
}

#[test]
fn test_swarm_dependency_unblocking_and_failure_propagation() {
    let plan = SwarmPlan {
        id: "swarm_unblock_test".to_string(),
        title: "Unblocking Test".to_string(),
        objective: "Test reactive DAG state transitions".to_string(),
        tasks: vec![
            make_test_task("t1", "Task 1", "Worker 1", vec![]),
            make_test_task("t2", "Task 2", "Worker 2", vec!["t1"]),
            make_test_task("t3", "Task 3", "Worker 3", vec!["t2"]),
        ],
        created_at: chrono::Utc::now().to_rfc3339(),
        metadata: HashMap::new(),
    };

    let mut state = SwarmExecutionState::new(&plan);

    // Initial state: t1 is Ready, t2 and t3 are Pending
    assert_eq!(state.task_statuses.get("t1"), Some(&SwarmTaskStatus::Ready));
    assert_eq!(
        state.task_statuses.get("t2"),
        Some(&SwarmTaskStatus::Pending)
    );
    assert_eq!(
        state.task_statuses.get("t3"),
        Some(&SwarmTaskStatus::Pending)
    );
    assert!(!state.is_complete());

    // Scenario A: t1 completes successfully -> unblocks t2
    state
        .task_statuses
        .insert("t1".to_string(), SwarmTaskStatus::Completed);
    let newly_ready = state.evaluate_ready_tasks(&plan);
    assert_eq!(newly_ready, vec!["t2".to_string()]);
    assert_eq!(state.task_statuses.get("t2"), Some(&SwarmTaskStatus::Ready));
    assert_eq!(
        state.task_statuses.get("t3"),
        Some(&SwarmTaskStatus::Pending)
    );

    // Scenario B: t2 fails verification -> propagates Blocked to t3
    state
        .task_statuses
        .insert("t2".to_string(), SwarmTaskStatus::Failed);
    let newly_ready_after_fail = state.evaluate_ready_tasks(&plan);
    assert!(newly_ready_after_fail.is_empty());
    assert_eq!(
        state.task_statuses.get("t3"),
        Some(&SwarmTaskStatus::Blocked)
    );
    assert!(state.is_complete());
}

#[test]
fn test_swarm_reporter_mermaid_and_scorecard() {
    let plan = SwarmPlan {
        id: "swarm_report_eval".to_string(),
        title: "Report Evaluation Swarm".to_string(),
        objective: "Synthesize executive report".to_string(),
        tasks: vec![
            make_test_task("t1_core", "Core Module", "Core Architect", vec![]),
            make_test_task("t2_web", "Web Frontend", "Frontend Lead", vec!["t1_core"]),
        ],
        created_at: chrono::Utc::now().to_rfc3339(),
        metadata: HashMap::new(),
    };

    let mut state = SwarmExecutionState::new(&plan);
    state
        .task_statuses
        .insert("t1_core".to_string(), SwarmTaskStatus::Completed);
    state
        .task_statuses
        .insert("t2_web".to_string(), SwarmTaskStatus::Completed);
    state.total_duration_ms = Some(4250);
    state.total_tokens = 15800;

    let mut artifacts = HashMap::new();
    artifacts.insert(
        "src/core/schema.rs".to_string(),
        "pub struct Schema;".to_string(),
    );

    state.outcomes.insert(
        "t1_core".to_string(),
        SwarmTaskOutcome {
            task_id: "t1_core".to_string(),
            role_title: "Core Architect".to_string(),
            status: SwarmTaskStatus::Completed,
            duration_ms: 2100,
            tokens_used: 7900,
            files_modified: vec!["src/core/schema.rs".to_string()],
            worktree_path: None,
            branch_name: Some("swarm/swarm_report_eval/t1_core".to_string()),
            verification_command: Some("cargo test".to_string()),
            verification_passed: true,
            summary: "Successfully implemented core schema".to_string(),
            error: None,
            generated_artifacts: artifacts,
        },
    );

    let report = SwarmReporter::format_report(
        &plan,
        &state,
        Some("✔ Successfully merged 2 branches with 0 conflicts."),
    );

    // Verify Mermaid diagram exists
    assert!(report.contains("```mermaid"));
    assert!(report.contains("graph TD"));
    assert!(report.contains("t1_core --> t2_web"));

    // Verify Task Scorecard Table
    assert!(report.contains(
        "| Task ID | Specialized Role | Status | Duration | Verification Check | Files Changed |"
    ));
    assert!(report.contains("`t1_core`"));
    assert!(report.contains("Core Architect"));
    assert!(report.contains("✔ Completed"));
    assert!(report.contains("✔ `cargo test`"));

    // Verify Merge summary
    assert!(report.contains("Successfully merged 2 branches with 0 conflicts."));
}

/// Integration tests for Phase 36: Task DAG & Dynamic Dependency Graph Engine
///
/// Tests parallel wave scheduling, dynamic task splitting, dependency propagation,
/// persistence, and tool schema registration.
use minicode::agent::task_dag::{TaskDag, TaskItem, TaskStatus};
use minicode::tools::registry::agent_tools;
use tempfile::tempdir;

#[test]
fn test_task_dag_waves_and_toposort() {
    let mut dag = TaskDag::new();

    dag.add_task(TaskItem {
        id: "task_a".to_string(),
        title: "Architecture Spec".to_string(),
        description: "Write PRD and design doc".to_string(),
        dependencies: vec![],
        complexity_score: 2,
        status: TaskStatus::Pending,
        assigned_role: Some("Researcher".to_string()),
        worktree_branch: None,
    });

    dag.add_task(TaskItem {
        id: "task_b1".to_string(),
        title: "Frontend Tree UI".to_string(),
        description: "Implement ratatui inline tree".to_string(),
        dependencies: vec!["task_a".to_string()],
        complexity_score: 4,
        status: TaskStatus::Pending,
        assigned_role: Some("TestEngineer".to_string()),
        worktree_branch: None,
    });

    dag.add_task(TaskItem {
        id: "task_b2".to_string(),
        title: "Backend Wave Scheduler".to_string(),
        description: "Implement petgraph topological waves".to_string(),
        dependencies: vec!["task_a".to_string()],
        complexity_score: 4,
        status: TaskStatus::Pending,
        assigned_role: Some("TestEngineer".to_string()),
        worktree_branch: None,
    });

    dag.add_task(TaskItem {
        id: "task_c".to_string(),
        title: "Critic Audit & Merge".to_string(),
        description: "Run actor-critic evaluation".to_string(),
        dependencies: vec!["task_b1".to_string(), "task_b2".to_string()],
        complexity_score: 3,
        status: TaskStatus::Pending,
        assigned_role: Some("CodeReviewer".to_string()),
        worktree_branch: None,
    });

    let waves = dag.calculate_execution_waves().unwrap();
    assert_eq!(waves.len(), 3);
    assert_eq!(waves[0], vec!["task_a"]);
    assert!(waves[1].contains(&"task_b1".to_string()));
    assert!(waves[1].contains(&"task_b2".to_string()));
    assert_eq!(waves[2], vec!["task_c"]);

    let report = dag.generate_report();
    assert!(report.contains("Parallel Execution Waves:"));
    assert!(report.contains("Wave 1: task_a"));
}

#[test]
fn test_task_dag_persistence() {
    let dir = tempdir().unwrap();
    let mut dag = TaskDag::new();

    dag.add_task(TaskItem {
        id: "t1".to_string(),
        title: "Persistent Task".to_string(),
        description: "Testing save/load".to_string(),
        dependencies: vec![],
        complexity_score: 3,
        status: TaskStatus::Pending,
        assigned_role: None,
        worktree_branch: None,
    });

    dag.save(dir.path()).unwrap();

    let loaded = TaskDag::load(dir.path()).unwrap();
    assert_eq!(loaded.tasks.len(), 1);
    assert_eq!(loaded.tasks["t1"].title, "Persistent Task");
}

#[test]
fn test_task_dag_tool_schemas_registered() {
    let schemas = agent_tools::get_schemas();
    let names: Vec<String> = schemas.into_iter().map(|s| s.name).collect();

    assert!(names.contains(&"schedule_task_waves".to_string()));
    assert!(names.contains(&"split_task".to_string()));
    assert!(names.contains(&"create_task_dag".to_string()));
    assert!(names.contains(&"get_next_task".to_string()));
    assert!(names.contains(&"complete_task".to_string()));
}

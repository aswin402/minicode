use minicode::agent::task_dag::{TaskDag, TaskItem, TaskStatus};
use minicode::tools::ToolRegistry;
use serde_json::json;
use tempfile::tempdir;

#[test]
fn test_task_dag_dependency_resolution_and_toposort() {
    let mut dag = TaskDag::new();
    dag.add_task(TaskItem {
        id: "step-1".to_string(),
        title: "Initialize Database".to_string(),
        description: "Run migrations".to_string(),
        dependencies: vec![],
        complexity_score: 3,
        status: TaskStatus::Pending,
        assigned_role: None,
        worktree_branch: None,
    });
    dag.add_task(TaskItem {
        id: "step-2".to_string(),
        title: "Seed User Data".to_string(),
        description: "Insert admin user".to_string(),
        dependencies: vec!["step-1".to_string()],
        complexity_score: 2,
        status: TaskStatus::Pending,
        assigned_role: None,
        worktree_branch: None,
    });
    dag.add_task(TaskItem {
        id: "step-3".to_string(),
        title: "Start Web Server".to_string(),
        description: "Axum router".to_string(),
        dependencies: vec!["step-2".to_string()],
        complexity_score: 4,
        status: TaskStatus::Pending,
        assigned_role: None,
        worktree_branch: None,
    });

    let order = dag.topological_order().unwrap();
    assert_eq!(order, vec!["step-1", "step-2", "step-3"]);

    let next = dag.next_executable_tasks();
    assert_eq!(next.len(), 1);
    assert_eq!(next[0].id, "step-1");

    dag.set_task_status("step-1", TaskStatus::Completed)
        .unwrap();
    let next2 = dag.next_executable_tasks();
    assert_eq!(next2.len(), 1);
    assert_eq!(next2[0].id, "step-2");

    dag.set_task_status("step-2", TaskStatus::Completed)
        .unwrap();
    let next3 = dag.next_executable_tasks();
    assert_eq!(next3.len(), 1);
    assert_eq!(next3[0].id, "step-3");
}

#[tokio::test]
async fn test_task_dag_tool_dispatch_lifecycle() {
    let dir = tempdir().unwrap();
    let ws_path = dir.path().to_path_buf();

    // 1. Create DAG
    let res = ToolRegistry::dispatch(
        &ws_path,
        "call_dag_create",
        "create_task_dag",
        &json!({
            "tasks": [
                {
                    "id": "t1",
                    "title": "Task 1",
                    "description": "Base setup",
                    "dependencies": []
                },
                {
                    "id": "t2",
                    "title": "Task 2",
                    "description": "Requires t1",
                    "dependencies": ["t1"]
                }
            ]
        }),
        None,
        1,
    )
    .await;

    assert!(res.success);
    assert!(res.output.contains("t1 ➔ t2"));

    // 2. Get Next Task
    let next_res = ToolRegistry::dispatch(
        &ws_path,
        "call_dag_next",
        "get_next_task",
        &json!({}),
        None,
        1,
    )
    .await;

    assert!(next_res.success);
    assert!(next_res.output.contains("`t1`"));
    assert!(!next_res.output.contains("`t2`"));

    // 3. Complete Task 1
    let comp_res = ToolRegistry::dispatch(
        &ws_path,
        "call_dag_comp",
        "complete_task",
        &json!({
            "task_id": "t1",
            "status": "completed"
        }),
        None,
        1,
    )
    .await;

    assert!(comp_res.success);
    assert!(comp_res.output.contains("`t2`"));

    // 4. Critic Review
    let critic_res = ToolRegistry::dispatch(
        &ws_path,
        "call_critic",
        "critic_review",
        &json!({}),
        None,
        1,
    )
    .await;

    assert!(critic_res.success);
    assert!(critic_res.output.contains("Critic"));
}

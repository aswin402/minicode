use minicode::agent::orchestrator::{FanoutWorkerOutcome, MultiAgentOrchestrator};
use minicode::agent::subagent::{SubAgentResult, SubagentRole, SubagentTaskSpec};
use minicode::git::worktree::WorktreeManager;
use minicode::tools::ToolRegistry;
use serde_json::json;
use tempfile::tempdir;
use tokio::process::Command;

#[test]
fn test_subagent_task_spec_serialization_and_loose_parsing() {
    // 1. Loose string role parsing
    assert_eq!(
        SubagentRole::from_str_loose("researcher"),
        SubagentRole::Researcher
    );
    assert_eq!(
        SubagentRole::from_str_loose("research"),
        SubagentRole::Researcher
    );
    assert_eq!(
        SubagentRole::from_str_loose("reviewer"),
        SubagentRole::CodeReviewer
    );
    assert_eq!(
        SubagentRole::from_str_loose("code_reviewer"),
        SubagentRole::CodeReviewer
    );
    assert_eq!(
        SubagentRole::from_str_loose("tester"),
        SubagentRole::TestEngineer
    );
    assert_eq!(
        SubagentRole::from_str_loose("test-engineer"),
        SubagentRole::TestEngineer
    );
    assert_eq!(
        SubagentRole::from_str_loose("security"),
        SubagentRole::SecurityAuditor
    );
    assert_eq!(
        SubagentRole::from_str_loose("security_auditor"),
        SubagentRole::SecurityAuditor
    );
    assert_eq!(
        SubagentRole::from_str_loose("architect"),
        SubagentRole::Custom("architect".to_string())
    );
    assert_eq!(
        SubagentRole::from_str_loose("coder"),
        SubagentRole::Custom("coder".to_string())
    );
    assert_eq!(
        SubagentRole::from_str_loose("custom_agent"),
        SubagentRole::Custom("custom_agent".to_string())
    );

    // 2. Deserialization from JSON
    let raw_json = json!({
        "role": "researcher",
        "prompt": "Find all usages of TokenBudget",
        "isolate_worktree": false,
        "timeout_secs": 90
    });
    let spec: SubagentTaskSpec = serde_json::from_value(raw_json).unwrap();
    assert_eq!(spec.role, SubagentRole::Researcher);
    assert_eq!(spec.prompt, "Find all usages of TokenBudget");
    assert_eq!(spec.isolate_worktree, Some(false));
    assert_eq!(spec.timeout_secs, Some(90));
}

#[test]
fn test_fanout_summary_formatting() {
    let results = vec![
        FanoutWorkerOutcome {
            id: "res-1".to_string(),
            role: "Researcher".to_string(),
            isolate_worktree: false,
            success: true,
            result: SubAgentResult {
                id: "res-1".to_string(),
                task_id: "res-1".to_string(),
                role: SubagentRole::Researcher,
                success: true,
                final_summary: "Found 4 usages of TokenBudget in prompt.rs and loop.rs."
                    .to_string(),
                tokens_used: 1250,
                turns_executed: 3,
                files_inspected: vec!["src/agent/prompt.rs".to_string()],
                files_modified: Vec::new(),
                worktree_branch: None,
            },
            merged: false,
            error: None,
        },
        FanoutWorkerOutcome {
            id: "test-2".to_string(),
            role: "TestEngineer".to_string(),
            isolate_worktree: true,
            success: true,
            result: SubAgentResult {
                id: "test-2".to_string(),
                task_id: "test-2".to_string(),
                role: SubagentRole::TestEngineer,
                success: true,
                final_summary: "Executed reproducer test successfully with exit code 0."
                    .to_string(),
                tokens_used: 2400,
                turns_executed: 4,
                files_inspected: vec!["tests/repro_foo.rs".to_string()],
                files_modified: vec!["tests/repro_foo.rs".to_string()],
                worktree_branch: Some("subagent/test-2".to_string()),
            },
            merged: true,
            error: None,
        },
        FanoutWorkerOutcome {
            id: "sec-3".to_string(),
            role: "SecurityAuditor".to_string(),
            isolate_worktree: false,
            success: false,
            result: SubAgentResult {
                id: "sec-3".to_string(),
                task_id: "sec-3".to_string(),
                role: SubagentRole::SecurityAuditor,
                success: false,
                final_summary: "Execution failed".to_string(),
                tokens_used: 0,
                turns_executed: 0,
                files_inspected: Vec::new(),
                files_modified: Vec::new(),
                worktree_branch: None,
            },
            merged: false,
            error: Some("Network error connecting to model provider".to_string()),
        },
    ];

    let summary = MultiAgentOrchestrator::format_fanout_summary(&results);
    assert!(summary.contains("Subagent Swarm Fan-Out Completed (3 worker(s) finished)"));
    assert!(
        summary.contains("| Worker ID | Role | Environment | Status | Tokens | Files Modified |")
    );
    assert!(summary.contains("`res-1`"));
    assert!(summary.contains("Shared (Read-Only)"));
    assert!(summary.contains("✔ Success"));
    assert!(summary.contains("`test-2`"));
    assert!(summary.contains("`subagent/test-2` *(merged)*"));
    assert!(summary.contains("`sec-3`"));
    assert!(summary.contains("✗ Failed"));
    assert!(summary.contains("Found 4 usages of TokenBudget"));
    assert!(summary.contains("Network error connecting to model provider"));
}

#[tokio::test]
async fn test_fanout_empty_tasks() {
    let temp = tempdir().unwrap();
    let res = MultiAgentOrchestrator::fanout_tasks(temp.path(), Vec::new(), true, false).await;
    assert!(res.is_ok());
    assert_eq!(res.unwrap(), "No subagent tasks specified for fan-out.");
}

#[tokio::test]
async fn test_tool_dispatch_for_fanout_and_merge() {
    let temp = tempdir().unwrap();
    let root = temp.path();

    // 1. Dispatch fanout_subagents in non-blocking mode (wait_for_completion: false)
    let fanout_args = json!({
        "tasks": [
            {
                "role": "researcher",
                "prompt": "Inspect codebase structure",
                "isolate_worktree": false
            },
            {
                "role": "code_reviewer",
                "prompt": "Audit recent commits",
                "isolate_worktree": false
            }
        ],
        "wait_for_completion": false
    });

    let res = ToolRegistry::dispatch(
        root,
        "call_fanout_1",
        "fanout_subagents",
        &fanout_args,
        None,
        1,
    )
    .await;

    assert!(res.success);
    assert!(res
        .output
        .contains("Subagent Swarm Fan-Out Launched (2 worker(s) in background)"));
    assert!(res.output.contains("Researcher"));
    assert!(res.output.contains("CodeReviewer"));

    // 2. Dispatch merge_subagent_worktree for non-existent ID (graceful error handling)
    let merge_args = json!({
        "subagent_id": "nonexistent_task_123"
    });

    let res_merge = ToolRegistry::dispatch(
        root,
        "call_merge_1",
        "merge_subagent_worktree",
        &merge_args,
        None,
        1,
    )
    .await;

    // Non-git repo or missing branch returns error gracefully without panicking
    assert!(!res_merge.success);
    assert!(res_merge
        .output
        .contains("Error executing merge_subagent_worktree"));
}

#[tokio::test]
async fn test_worktree_manager_isolation_lifecycle() {
    // Initialize a real temporary git repository
    let temp = tempdir().unwrap();
    let root = temp.path();

    // git init
    let init_status = Command::new("git")
        .args(["init"])
        .current_dir(root)
        .output()
        .await
        .unwrap();
    assert!(init_status.status.success());

    // git config user
    let _ = Command::new("git")
        .args(["config", "user.name", "TestUser"])
        .current_dir(root)
        .output()
        .await;
    let _ = Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(root)
        .output()
        .await;

    // Initial commit
    let base_file = root.join("README.md");
    tokio::fs::write(&base_file, "# Main Repo\n").await.unwrap();
    let _ = Command::new("git")
        .args(["add", "README.md"])
        .current_dir(root)
        .output()
        .await;
    let _ = Command::new("git")
        .args(["commit", "-m", "initial commit"])
        .current_dir(root)
        .output()
        .await;

    let mgr = WorktreeManager::new(root);
    let worker_id = "test-worker-1";

    // 1. Create worktree
    let worktree_path = mgr.create_worktree(worker_id).await.unwrap();
    assert!(worktree_path.exists());
    assert!(worktree_path.ends_with(".minicode/worktrees/test-worker-1"));

    // 2. Modify file inside worktree
    let worktree_new_file = worktree_path.join("subagent_feature.txt");
    tokio::fs::write(
        &worktree_new_file,
        "New feature created in isolated worktree\n",
    )
    .await
    .unwrap();

    // Verify parent workspace does NOT have this file (ISOLATION PROVEN!)
    let parent_file = root.join("subagent_feature.txt");
    assert!(!parent_file.exists());

    // Commit change in worktree branch
    let _ = Command::new("git")
        .args(["add", "subagent_feature.txt"])
        .current_dir(&worktree_path)
        .output()
        .await;
    let _ = Command::new("git")
        .args(["commit", "-m", "feature: subagent isolated commit"])
        .current_dir(&worktree_path)
        .output()
        .await;

    // 3. Merge worktree into parent branch
    let merge_res = mgr.merge_worktree(worker_id).await.unwrap();
    assert!(merge_res.contains("merge") || merge_res.contains("subagent"));

    // Verify file now exists in parent workspace!
    assert!(parent_file.exists());

    // 4. Remove worktree and clean up temporary branch
    mgr.remove_worktree(worker_id).await.unwrap();
    assert!(!worktree_path.exists());
}

#[test]
fn test_subagent_drawer_navigation_and_state() {
    use minicode::ui::SubagentDrawer;

    let mut drawer = SubagentDrawer::new();
    assert!(!drawer.is_open);
    assert_eq!(drawer.selected_index, 0);
    assert!(!drawer.inspect_mode);

    // Toggle open
    drawer.toggle();
    assert!(drawer.is_open);

    // Toggle inspect mode
    drawer.toggle_inspect();
    assert!(drawer.inspect_mode);
    drawer.toggle_inspect();
    assert!(!drawer.inspect_mode);

    // Navigation with 3 items
    drawer.next(3);
    assert_eq!(drawer.selected_index, 1);
    drawer.next(3);
    assert_eq!(drawer.selected_index, 2);
    drawer.next(3); // wrap around
    assert_eq!(drawer.selected_index, 0);

    drawer.previous(3); // wrap backward
    assert_eq!(drawer.selected_index, 2);
    drawer.previous(3);
    assert_eq!(drawer.selected_index, 1);

    // Empty list navigation
    drawer.next(0);
    assert_eq!(drawer.selected_index, 0);
    drawer.previous(0);
    assert_eq!(drawer.selected_index, 0);

    // Close
    drawer.close();
    assert!(!drawer.is_open);
    assert!(!drawer.inspect_mode);
}

#[test]
fn test_subagent_info_telemetry_fields() {
    use minicode::agent::subagent::SubagentInfo;

    let mut info = SubagentInfo::new(
        "worker-42".to_string(),
        SubagentRole::CodeReviewer,
        "Review git diff".to_string(),
    );
    assert_eq!(info.id, "worker-42");
    assert_eq!(info.role, SubagentRole::CodeReviewer);
    assert!(info.current_tool.is_none());
    assert_eq!(info.status_message.as_deref(), Some("Initialized"));
    assert!(!info.isolate_worktree);
    assert!(info.final_summary.is_none());

    // Update telemetry
    info.current_tool = Some("patch_file".to_string());
    info.status_message = Some("Executing `patch_file`".to_string());
    info.isolate_worktree = true;
    info.final_summary = Some("Patched successfully".to_string());

    // Serialize & deserialize roundtrip
    let json_val = serde_json::to_value(&info).unwrap();
    let deserialized: SubagentInfo = serde_json::from_value(json_val).unwrap();
    assert_eq!(deserialized.id, "worker-42");
    assert_eq!(deserialized.current_tool.as_deref(), Some("patch_file"));
    assert_eq!(
        deserialized.status_message.as_deref(),
        Some("Executing `patch_file`")
    );
    assert!(deserialized.isolate_worktree);
    assert_eq!(
        deserialized.final_summary.as_deref(),
        Some("Patched successfully")
    );
}

#[tokio::test]
async fn test_dispatch_subagent_tool_and_manage_await() {
    let temp = tempdir().unwrap();
    let root = temp.path();

    // 1. Dispatch a background researcher subagent
    let dispatch_args = json!({
        "role": "researcher",
        "prompt": "Find all usages of SubagentDrawer",
        "isolate_worktree": false,
        "token_budget": 5000,
        "max_turns": 2
    });

    let res = ToolRegistry::dispatch(
        root,
        "call_dispatch_1",
        "dispatch_subagent",
        &dispatch_args,
        None,
        1,
    )
    .await;

    assert!(res.success);
    assert!(res
        .output
        .contains("Background subagent spawned successfully"));
    assert!(res.output.contains("Worker ID"));
    assert!(res.output.contains("Researcher"));
    assert!(res.output.contains("Ctrl+S"));

    // Extract worker ID from output
    let worker_id_line = res
        .output
        .lines()
        .find(|l| l.contains("Worker ID"))
        .unwrap_or_default();
    let id_start = worker_id_line.find('`').unwrap_or(0) + 1;
    let id_end = worker_id_line.rfind('`').unwrap_or(worker_id_line.len());
    let worker_id = &worker_id_line[id_start..id_end];

    // 2. Query intermediate status via manage_subagents
    let status_args = json!({
        "action": "status",
        "subagent_id": worker_id
    });

    let status_res = ToolRegistry::dispatch(
        root,
        "call_status_1",
        "manage_subagents",
        &status_args,
        None,
        2,
    )
    .await;

    assert!(status_res.success);
    assert!(status_res.output.contains(worker_id));

    // 3. Await completion with short timeout
    let await_args = json!({
        "action": "await",
        "subagent_id": worker_id,
        "timeout_secs": 2
    });

    let await_res = ToolRegistry::dispatch(
        root,
        "call_await_1",
        "manage_subagents",
        &await_args,
        None,
        3,
    )
    .await;

    assert!(await_res.success);
    assert!(await_res.output.contains(worker_id));
}

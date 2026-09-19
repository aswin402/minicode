//! End-to-End Integration Test Suite for Parallel Subagent Swarm Fan-Out & Arbitration Engine.
//!
//! Validates the complete swarm fan-out and arbitration lifecycle:
//! 1. Tool dispatch and argument parsing integration (`fanout_subagents`).
//! 2. Sequential multi-worker clean arbitration and automatic worktree teardown.
//! 3. Competing concurrent modifications and safe conflict isolation without parent corruption.
//! 4. Pre-merge verification failure isolation and worktree retention.
//! 5. Executive map-reduce matrix reporting across all worker outcome variants.

use minicode::agent::subagent::fanout::{
    FanoutJoinMode, FanoutOrchestrator, FanoutTaskItem, MergeStatus, WorkerResult,
};
use minicode::agent::subagent::types::{AgentId, SubagentRole, WorkspaceMode};
use minicode::tools::registry::agent_tools::swarms::{dispatch, parse_fanout_args};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Initializes a temporary Git repository with local user identity and signing disabled.
fn setup_git_repo(root: &Path) {
    let run = |args: &[&str]| {
        let output = Command::new("git")
            .args(args)
            .current_dir(root)
            .output()
            .unwrap_or_else(|e| panic!("failed to run git {:?}: {}", args, e));
        assert!(
            output.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
    };

    run(&["init"]);
    run(&["config", "user.name", "test-user"]);
    run(&["config", "user.email", "test@example.com"]);
    run(&["config", "commit.gpgsign", "false"]);
    run(&["config", "init.defaultBranch", "main"]);

    std::fs::write(root.join(".gitignore"), ".minicode/\n").unwrap();
    run(&["add", ".gitignore"]);
    run(&["commit", "-m", "ignore minicode"]);
}

#[tokio::test]
async fn test_integration_fanout_dispatch_and_validation() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    // 1. Empty tasks test
    let empty_payload = serde_json::json!({
        "tasks": []
    });
    let empty_res = dispatch("fanout_subagents", &empty_payload, root)
        .await
        .expect("dispatcher handled fanout_subagents");
    assert!(empty_res.is_ok());
    assert!(empty_res
        .unwrap()
        .contains("No subagent tasks specified for fan-out."));

    // 2. Invalid task specification (missing task and prompt)
    let invalid_payload = serde_json::json!({
        "tasks": [
            { "role": "coder" }
        ]
    });
    let invalid_res = dispatch("fanout_subagents", &invalid_payload, root)
        .await
        .expect("dispatcher handled fanout_subagents");
    assert!(invalid_res.is_err());

    // 3. Positive argument parsing test with prompt alias, join_mode, auto_merge, concurrency
    let valid_payload = serde_json::json!({
        "tasks": [
            {
                "task": "Map module dependencies",
                "role": "scout",
                "workspace_mode": "shared"
            },
            {
                "prompt": "Implement AST node cache",
                "role": "coder",
                "workspace_mode": "worktree",
                "max_iterations": 20,
                "check_cmd": "cargo check -j 1"
            }
        ],
        "join_mode": "race",
        "auto_merge": true,
        "max_concurrency": 8
    });

    let (tasks, join_mode, auto_merge, max_concurrency) =
        parse_fanout_args(&valid_payload).expect("successful parse");

    assert_eq!(tasks.len(), 2);
    assert_eq!(tasks[0].task, "Map module dependencies");
    assert_eq!(tasks[0].role, SubagentRole::Scout);
    assert_eq!(tasks[0].workspace_mode, Some(WorkspaceMode::Shared));
    assert_eq!(tasks[0].check_cmd, None);

    assert_eq!(tasks[1].task, "Implement AST node cache");
    assert_eq!(tasks[1].role, SubagentRole::Coder);
    assert_eq!(tasks[1].workspace_mode, Some(WorkspaceMode::Worktree));
    assert_eq!(tasks[1].max_iterations, Some(20));
    assert_eq!(tasks[1].check_cmd, Some("cargo check -j 1".to_string()));

    assert_eq!(join_mode, FanoutJoinMode::Race);
    assert!(auto_merge);
    assert_eq!(max_concurrency, 8);
}

#[tokio::test]
async fn test_integration_fanout_sequential_multi_worker_merge() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    setup_git_repo(root);

    // Initial base commit
    std::fs::write(root.join("main.rs"), "fn main() {}\n").unwrap();
    Command::new("git")
        .args(["add", "."])
        .current_dir(root)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "initial base commit"])
        .current_dir(root)
        .output()
        .unwrap();

    // 1. Worker Alpha setup
    let branch_alpha = "minicode/subagent/worker-alpha";
    Command::new("git")
        .args(["branch", branch_alpha])
        .current_dir(root)
        .output()
        .unwrap();
    let wt_alpha = root.join("wt-alpha");
    Command::new("git")
        .args(["worktree", "add", wt_alpha.to_str().unwrap(), branch_alpha])
        .current_dir(root)
        .output()
        .unwrap();

    std::fs::write(
        wt_alpha.join("alpha.rs"),
        "pub fn alpha() -> &'static str { \"alpha\" }\n",
    )
    .unwrap();
    Command::new("git")
        .args(["add", "."])
        .current_dir(&wt_alpha)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "feat(alpha): add alpha module"])
        .current_dir(&wt_alpha)
        .output()
        .unwrap();

    // 2. Worker Beta setup
    let branch_beta = "minicode/subagent/worker-beta";
    Command::new("git")
        .args(["branch", branch_beta])
        .current_dir(root)
        .output()
        .unwrap();
    let wt_beta = root.join("wt-beta");
    Command::new("git")
        .args(["worktree", "add", wt_beta.to_str().unwrap(), branch_beta])
        .current_dir(root)
        .output()
        .unwrap();

    std::fs::write(
        wt_beta.join("beta.rs"),
        "pub fn beta() -> &'static str { \"beta\" }\n",
    )
    .unwrap();
    Command::new("git")
        .args(["add", "."])
        .current_dir(&wt_beta)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "feat(beta): add beta module"])
        .current_dir(&wt_beta)
        .output()
        .unwrap();

    // 3. Construct Swarm Results & Task Items
    let mut results = vec![
        WorkerResult {
            agent_id: AgentId("worker-alpha".to_string()),
            role: SubagentRole::Coder,
            task: "Add alpha module".to_string(),
            success: true,
            duration_ms: 180,
            tokens_used: 240,
            files_modified: vec!["alpha.rs".to_string()],
            worktree_path: Some(wt_alpha.clone()),
            branch_name: Some(branch_alpha.to_string()),
            merge_status: MergeStatus::RetainedUnmerged,
            summary: "Created alpha.rs".to_string(),
            error: None,
            check_cmd: Some("skip".to_string()),
        },
        WorkerResult {
            agent_id: AgentId("worker-beta".to_string()),
            role: SubagentRole::Coder,
            task: "Add beta module".to_string(),
            success: true,
            duration_ms: 220,
            tokens_used: 310,
            files_modified: vec!["beta.rs".to_string()],
            worktree_path: Some(wt_beta.clone()),
            branch_name: Some(branch_beta.to_string()),
            merge_status: MergeStatus::RetainedUnmerged,
            summary: "Created beta.rs".to_string(),
            error: None,
            check_cmd: Some("skip".to_string()),
        },
    ];

    let tasks = vec![
        FanoutTaskItem {
            task: "Add alpha module".to_string(),
            role: SubagentRole::Coder,
            workspace_mode: Some(WorkspaceMode::Worktree),
            max_iterations: None,
            check_cmd: Some("skip".to_string()),
        },
        FanoutTaskItem {
            task: "Add beta module".to_string(),
            role: SubagentRole::Coder,
            workspace_mode: Some(WorkspaceMode::Worktree),
            max_iterations: None,
            check_cmd: Some("skip".to_string()),
        },
    ];

    // 4. Run Sequential Merge Arbitration
    FanoutOrchestrator::arbitrate_mutating_workers(root, &mut results, &tasks).await;

    // Assert both workers merged cleanly
    assert!(matches!(
        results[0].merge_status,
        MergeStatus::Merged { .. }
    ));
    assert!(matches!(
        results[1].merge_status,
        MergeStatus::Merged { .. }
    ));

    // Assert both files landed in parent workspace
    assert!(root.join("alpha.rs").exists());
    assert!(root.join("beta.rs").exists());

    // Assert both worktrees were removed
    assert!(!wt_alpha.exists());
    assert!(!wt_beta.exists());
}

#[tokio::test]
async fn test_integration_fanout_conflict_isolation_retains_worktree() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    setup_git_repo(root);

    // Initial base commit with shared file
    std::fs::write(root.join("config.json"), "{\n  \"version\": 1\n}\n").unwrap();
    Command::new("git")
        .args(["add", "."])
        .current_dir(root)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "init config"])
        .current_dir(root)
        .output()
        .unwrap();

    // 1. Worker 1 branch & worktree
    let branch_1 = "minicode/subagent/worker-first";
    Command::new("git")
        .args(["branch", branch_1])
        .current_dir(root)
        .output()
        .unwrap();
    let wt_1 = root.join("wt-first");
    Command::new("git")
        .args(["worktree", "add", wt_1.to_str().unwrap(), branch_1])
        .current_dir(root)
        .output()
        .unwrap();

    std::fs::write(
        wt_1.join("config.json"),
        "{\n  \"version\": 2,\n  \"author\": \"first\"\n}\n",
    )
    .unwrap();
    Command::new("git")
        .args(["add", "."])
        .current_dir(&wt_1)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "update config by first"])
        .current_dir(&wt_1)
        .output()
        .unwrap();

    // 2. Worker 2 branch & worktree
    let branch_2 = "minicode/subagent/worker-second";
    Command::new("git")
        .args(["branch", branch_2])
        .current_dir(root)
        .output()
        .unwrap();
    let wt_2 = root.join("wt-second");
    Command::new("git")
        .args(["worktree", "add", wt_2.to_str().unwrap(), branch_2])
        .current_dir(root)
        .output()
        .unwrap();

    std::fs::write(
        wt_2.join("config.json"),
        "{\n  \"version\": 99,\n  \"author\": \"second\"\n}\n",
    )
    .unwrap();
    Command::new("git")
        .args(["add", "."])
        .current_dir(&wt_2)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "conflicting config by second"])
        .current_dir(&wt_2)
        .output()
        .unwrap();

    // 3. Construct Swarm Results & Task Items
    let mut results = vec![
        WorkerResult {
            agent_id: AgentId("worker-first".to_string()),
            role: SubagentRole::Coder,
            task: "Update config to version 2".to_string(),
            success: true,
            duration_ms: 150,
            tokens_used: 120,
            files_modified: vec!["config.json".to_string()],
            worktree_path: Some(wt_1.clone()),
            branch_name: Some(branch_1.to_string()),
            merge_status: MergeStatus::RetainedUnmerged,
            summary: "Updated version to 2".to_string(),
            error: None,
            check_cmd: Some("skip".to_string()),
        },
        WorkerResult {
            agent_id: AgentId("worker-second".to_string()),
            role: SubagentRole::Coder,
            task: "Update config to version 99".to_string(),
            success: true,
            duration_ms: 160,
            tokens_used: 140,
            files_modified: vec!["config.json".to_string()],
            worktree_path: Some(wt_2.clone()),
            branch_name: Some(branch_2.to_string()),
            merge_status: MergeStatus::RetainedUnmerged,
            summary: "Updated version to 99".to_string(),
            error: None,
            check_cmd: Some("skip".to_string()),
        },
    ];

    let tasks = vec![
        FanoutTaskItem {
            task: "Update config to version 2".to_string(),
            role: SubagentRole::Coder,
            workspace_mode: Some(WorkspaceMode::Worktree),
            max_iterations: None,
            check_cmd: Some("skip".to_string()),
        },
        FanoutTaskItem {
            task: "Update config to version 99".to_string(),
            role: SubagentRole::Coder,
            workspace_mode: Some(WorkspaceMode::Worktree),
            max_iterations: None,
            check_cmd: Some("skip".to_string()),
        },
    ];

    // 4. Run Sequential Merge Arbitration
    FanoutOrchestrator::arbitrate_mutating_workers(root, &mut results, &tasks).await;

    // Worker 1 should have merged cleanly and had its worktree cleaned up
    assert!(matches!(
        results[0].merge_status,
        MergeStatus::Merged { .. }
    ));
    assert!(!wt_1.exists());

    // Worker 2 should have encountered a conflict against the newly merged HEAD
    match &results[1].merge_status {
        MergeStatus::Conflict { conflicted_files } => {
            assert!(conflicted_files.contains(&"config.json".to_string()));
        }
        other => panic!("Expected Conflict status for worker 2, got: {:?}", other),
    }

    // Worker 2's worktree must be preserved on disk for developer remediation
    assert!(wt_2.exists());

    // Parent workspace must be clean and not corrupted by merge conflicts
    let main_config = std::fs::read_to_string(root.join("config.json")).unwrap();
    assert!(main_config.contains("\"author\": \"first\""));
    assert!(!main_config.contains("<<<<<<<"));
}

#[tokio::test]
async fn test_integration_fanout_verification_failure_isolation() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    setup_git_repo(root);

    std::fs::write(root.join("base.txt"), "base\n").unwrap();
    Command::new("git")
        .args(["add", "."])
        .current_dir(root)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "init"])
        .current_dir(root)
        .output()
        .unwrap();

    let branch = "minicode/subagent/worker-failing-check";
    Command::new("git")
        .args(["branch", branch])
        .current_dir(root)
        .output()
        .unwrap();
    let wt = root.join("wt-fail");
    Command::new("git")
        .args(["worktree", "add", wt.to_str().unwrap(), branch])
        .current_dir(root)
        .output()
        .unwrap();

    std::fs::write(wt.join("broken.rs"), "syntax error here").unwrap();
    Command::new("git")
        .args(["add", "."])
        .current_dir(&wt)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "broken commit"])
        .current_dir(&wt)
        .output()
        .unwrap();

    let mut results = vec![WorkerResult {
        agent_id: AgentId("worker-failing-check".to_string()),
        role: SubagentRole::Coder,
        task: "Implement broken feature".to_string(),
        success: true,
        duration_ms: 100,
        tokens_used: 80,
        files_modified: vec!["broken.rs".to_string()],
        worktree_path: Some(wt.clone()),
        branch_name: Some(branch.to_string()),
        merge_status: MergeStatus::RetainedUnmerged,
        summary: "Implemented with build failure".to_string(),
        error: None,
        check_cmd: Some("false".to_string()),
    }];

    let tasks = vec![FanoutTaskItem {
        task: "Implement broken feature".to_string(),
        role: SubagentRole::Coder,
        workspace_mode: Some(WorkspaceMode::Worktree),
        max_iterations: None,
        check_cmd: Some("false".to_string()),
    }];

    FanoutOrchestrator::arbitrate_mutating_workers(root, &mut results, &tasks).await;

    match &results[0].merge_status {
        MergeStatus::VerificationFailed {
            command, exit_code, ..
        } => {
            assert_eq!(command, "false");
            assert_ne!(*exit_code, 0);
        }
        other => panic!("Expected VerificationFailed, got: {:?}", other),
    }

    // Worktree preserved on disk
    assert!(wt.exists());
    // Parent workspace clean
    assert!(!root.join("broken.rs").exists());
}

#[test]
fn test_integration_fanout_matrix_reporting() {
    let results = vec![
        WorkerResult {
            agent_id: AgentId("scout-alpha".to_string()),
            role: SubagentRole::Scout,
            task: "Survey workspace".to_string(),
            success: true,
            duration_ms: 900,
            tokens_used: 400,
            files_modified: vec![],
            worktree_path: None,
            branch_name: None,
            merge_status: MergeStatus::NotApplicable,
            summary: "Read-only workspace survey complete.".to_string(),
            error: None,
            check_cmd: None,
        },
        WorkerResult {
            agent_id: AgentId("coder-clean".to_string()),
            role: SubagentRole::Coder,
            task: "Implement feature X".to_string(),
            success: true,
            duration_ms: 3200,
            tokens_used: 1100,
            files_modified: vec!["src/x.rs".to_string()],
            worktree_path: Some(PathBuf::from("/tmp/wt-clean")),
            branch_name: Some("minicode/subagent/coder-clean".to_string()),
            merge_status: MergeStatus::Merged {
                commit_hash: Some("9876abc".to_string()),
            },
            summary: "Feature X cleanly landed.".to_string(),
            error: None,
            check_cmd: Some("cargo test".to_string()),
        },
        WorkerResult {
            agent_id: AgentId("coder-conflict".to_string()),
            role: SubagentRole::Coder,
            task: "Implement feature Y".to_string(),
            success: true,
            duration_ms: 2800,
            tokens_used: 950,
            files_modified: vec!["src/x.rs".to_string()],
            worktree_path: Some(PathBuf::from("/tmp/wt-conflict")),
            branch_name: Some("minicode/subagent/coder-conflict".to_string()),
            merge_status: MergeStatus::Conflict {
                conflicted_files: vec!["src/x.rs".to_string()],
            },
            summary: "Feature Y conflicted with X.".to_string(),
            error: None,
            check_cmd: None,
        },
        WorkerResult {
            agent_id: AgentId("coder-failing".to_string()),
            role: SubagentRole::Coder,
            task: "Implement feature Z".to_string(),
            success: true,
            duration_ms: 1200,
            tokens_used: 350,
            files_modified: vec!["src/z.rs".to_string()],
            worktree_path: Some(PathBuf::from("/tmp/wt-failing")),
            branch_name: Some("minicode/subagent/coder-failing".to_string()),
            merge_status: MergeStatus::VerificationFailed {
                command: "cargo check".to_string(),
                exit_code: 101,
                stderr: "compile error".to_string(),
            },
            summary: "Compilation failed.".to_string(),
            error: None,
            check_cmd: Some("cargo check".to_string()),
        },
        WorkerResult {
            agent_id: AgentId("tester-cancelled".to_string()),
            role: SubagentRole::Tester,
            task: "Race candidate benchmark".to_string(),
            success: false,
            duration_ms: 400,
            tokens_used: 120,
            files_modified: vec![],
            worktree_path: None,
            branch_name: None,
            merge_status: MergeStatus::SkippedCancelled,
            summary: "Cancelled due to race winner.".to_string(),
            error: Some("Cancelled".to_string()),
            check_cmd: None,
        },
    ];

    let report =
        FanoutOrchestrator::format_fanout_report(&results, FanoutJoinMode::Race, true, 4500);

    // Verify header metrics
    assert!(report.contains("Subagent Swarm Fan-Out Completed"));
    assert!(report.contains("race (first success wins)"));
    assert!(report.contains("enabled (sequential arbitration)"));
    assert!(report.contains("2920 tokens used"));

    // Verify matrix table
    assert!(report
        .contains("| Worker ID | Role | Status | Duration | Tokens | Files | Merge Outcome |"));
    assert!(report.contains("✔ Merged (`9876abc`)"));
    assert!(report.contains("⚠️ Conflict (1 file(s))"));
    assert!(report.contains("❌ Verify Failed (`cargo check` exit 101)"));
    assert!(report.contains("— (read-only)"));
    assert!(report.contains("⏹ Cancelled"));

    // Verify diagnostics callout blocks
    assert!(report.contains("Arbitration Diagnostics & Conflicts:"));
    assert!(report.contains("Worker `coder-conflict` Merge Conflicts Detected"));
    assert!(report.contains("Worker `coder-failing` Pre-Merge Verification Failed"));
    assert!(report.contains("compile error"));

    // Verify summaries
    assert!(report.contains("Executive Summaries & Findings:"));
    assert!(report.contains("Feature X cleanly landed."));
    assert!(report.contains("Feature Y conflicted with X."));
}

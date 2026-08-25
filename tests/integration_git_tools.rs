mod common;

use common::{MockProvider, MockResponse};
use minicode::agent::AgentLoop;
use minicode::config::Config;
use minicode::git::GitService;
use tempfile::tempdir;
use tokio::process::Command;
use tokio::sync::mpsc;

async fn setup_git_workspace() -> (tempfile::TempDir, GitService) {
    let dir = tempdir().unwrap();
    let path = dir.path().to_path_buf();

    Command::new("git")
        .arg("init")
        .current_dir(&path)
        .output()
        .await
        .unwrap();

    Command::new("git")
        .args(["config", "user.name", "Minicode Test"])
        .current_dir(&path)
        .output()
        .await
        .unwrap();

    Command::new("git")
        .args(["config", "user.email", "test@minicode.ai"])
        .current_dir(&path)
        .output()
        .await
        .unwrap();

    let git = GitService::new(path);
    git.ensure_git_exclude();
    (dir, git)
}

#[tokio::test]
async fn test_auto_commit_after_agent_file_modification() {
    let (dir, git) = setup_git_workspace().await;
    let ws_path = dir.path().to_path_buf();

    // Initial commit so repository has a valid HEAD
    tokio::fs::write(ws_path.join("init.txt"), "init")
        .await
        .unwrap();
    let commit_svc = minicode::git::GitCommitService::new(&git);
    commit_svc
        .commit("feat: initial commit", None)
        .await
        .unwrap();

    // Scripted LLM creates a new file `src/service.rs`
    let responses = vec![
        MockResponse::with_tool_call(
            "call_write",
            "write_file",
            serde_json::json!({
                "path": "src/service.rs",
                "content": "pub fn start_service() -> bool { true }\n"
            }),
        ),
        MockResponse::text_only("Created service.rs"),
    ];

    let provider = Box::new(MockProvider::new(responses));
    let mut config = Config::default();
    config.git.auto_commit = true;
    config.agent.auto_approve = true; // scripted tools must run without the approval gate

    let mut agent = AgentLoop::new(&ws_path, config, provider);

    let (event_tx, mut event_rx) = mpsc::unbounded_channel();
    let collector = tokio::spawn(async move {
        let mut events = Vec::new();
        while let Some(evt) = event_rx.recv().await {
            events.push(evt);
        }
        events
    });

    let turn = agent
        .execute_turn("Create service.rs", event_tx, None)
        .await
        .unwrap();

    let events = collector.await.unwrap();

    // Turn modified src/service.rs
    assert!(turn.files_modified.contains(&"src/service.rs".to_string()));

    // Verify git event was emitted
    let git_commit_event = events
        .iter()
        .find(|e| matches!(e, minicode::agent::types::AgentEvent::GitCommit { .. }));
    assert!(
        git_commit_event.is_some(),
        "AgentEvent::GitCommit was not emitted!"
    );

    // Verify working tree is clean because it was auto-committed
    let status = git.get_status().await.unwrap();
    assert!(
        status.is_clean,
        "Working tree should be clean after auto-commit!"
    );

    // Verify commit in log
    let log = git.log(2).await.unwrap();
    assert!(log.contains("feat: update src/service.rs") || log.contains("feat: Create service.rs"));
}

#[tokio::test]
async fn test_git_tools_and_undo_rollback() {
    let (dir, git) = setup_git_workspace().await;
    let ws_path = dir.path().to_path_buf();

    // Initial commit
    let init_file = ws_path.join("config.txt");
    tokio::fs::write(&init_file, "version=1\n").await.unwrap();
    let commit_svc = minicode::git::GitCommitService::new(&git);
    commit_svc
        .commit("feat: initial config", None)
        .await
        .unwrap();

    // Scripted LLM checks git_status then patches config.txt
    let responses = vec![
        MockResponse::with_tool_call("call_status", "git_status", serde_json::json!({})),
        MockResponse::with_tool_call(
            "call_patch",
            "patch_file",
            serde_json::json!({
                "path": "config.txt",
                "search_block": "version=1",
                "replace_block": "version=2"
            }),
        ),
        MockResponse::text_only("Updated version to 2"),
    ];

    let provider = Box::new(MockProvider::new(responses));
    let mut config = Config::default();
    config.git.auto_commit = true;
    config.agent.auto_approve = true; // scripted tools must run without the approval gate

    let mut agent = AgentLoop::new(&ws_path, config, provider);
    let (event_tx, _event_rx) = mpsc::unbounded_channel();

    let turn = agent
        .execute_turn("Bump version to 2", event_tx, None)
        .await
        .unwrap();

    assert!(turn.files_modified.contains(&"config.txt".to_string()));

    // File on disk is now version=2
    assert_eq!(
        tokio::fs::read_to_string(&init_file).await.unwrap(),
        "version=2\n"
    );

    // Perform undo rollback
    let undo_res = minicode::session::undo::rollback_turn(&ws_path).unwrap();
    assert_eq!(undo_res.restored_count, 1);

    // Verify file is restored to version=1
    assert_eq!(
        tokio::fs::read_to_string(&init_file).await.unwrap(),
        "version=1\n"
    );

    // Verify commit was soft reset
    let current_log = git.log(5).await.unwrap();
    assert!(!current_log.contains("Bump version to 2"));
}

use minicode::git::{GitService, WorktreeManager};
use tempfile::tempdir;
use tokio::process::Command;

async fn setup_git_workspace() -> (tempfile::TempDir, std::path::PathBuf) {
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

    let git = GitService::new(path.clone());
    git.ensure_git_exclude();

    tokio::fs::write(path.join("README.md"), "# Main Project\n")
        .await
        .unwrap();

    Command::new("git")
        .args(["add", "-A"])
        .current_dir(&path)
        .output()
        .await
        .unwrap();

    Command::new("git")
        .args(["commit", "-m", "feat: initial commit"])
        .current_dir(&path)
        .output()
        .await
        .unwrap();

    (dir, path)
}

#[tokio::test]
async fn test_parallel_subagent_worktrees_and_merge() {
    let (_dir, ws_path) = setup_git_workspace().await;
    let wt_mgr = WorktreeManager::new(&ws_path);

    // 1. Create two parallel subagent worktrees concurrently
    let (wt1_res, wt2_res) = tokio::join!(
        wt_mgr.create_worktree("worker-1"),
        wt_mgr.create_worktree("worker-2")
    );

    let wt1_path = wt1_res.unwrap();
    let wt2_path = wt2_res.unwrap();

    assert!(wt1_path.exists());
    assert!(wt2_path.exists());

    // 2. Perform independent parallel modifications in both isolated worktrees
    tokio::fs::write(wt1_path.join("feature_1.rs"), "pub fn f1() {}\n")
        .await
        .unwrap();
    Command::new("git")
        .args(["add", "-A"])
        .current_dir(&wt1_path)
        .output()
        .await
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "feat: worker 1 changes"])
        .current_dir(&wt1_path)
        .output()
        .await
        .unwrap();

    tokio::fs::write(wt2_path.join("feature_2.rs"), "pub fn f2() {}\n")
        .await
        .unwrap();
    Command::new("git")
        .args(["add", "-A"])
        .current_dir(&wt2_path)
        .output()
        .await
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "feat: worker 2 changes"])
        .current_dir(&wt2_path)
        .output()
        .await
        .unwrap();

    // 3. Merge worker-1 into main
    let merge_res1 = wt_mgr.merge_worktree("worker-1").await;
    assert!(
        merge_res1.is_ok(),
        "Merge worker-1 failed: {:?}",
        merge_res1
    );
    assert!(ws_path.join("feature_1.rs").exists());

    // 4. Merge worker-2 into main
    let merge_res2 = wt_mgr.merge_worktree("worker-2").await;
    assert!(
        merge_res2.is_ok(),
        "Merge worker-2 failed: {:?}",
        merge_res2
    );
    assert!(ws_path.join("feature_2.rs").exists());

    // 5. Clean up both worktrees
    wt_mgr.remove_worktree("worker-1").await.unwrap();
    wt_mgr.remove_worktree("worker-2").await.unwrap();

    assert!(!wt1_path.exists());
    assert!(!wt2_path.exists());

    // 6. Verify main workspace contains both integrated features
    let git = GitService::new(ws_path);
    let log = git.log(5).await.unwrap();
    assert!(log.contains("worker 1 changes"));
    assert!(log.contains("worker 2 changes"));
}

pub mod commit;
pub mod diff_filter;
pub mod diff_projector;
pub mod diff_viewer;
pub mod reviewer;
pub mod service;
pub mod worktree;

pub use commit::GitCommitService;
pub use diff_filter::DiffFilter;
pub use diff_projector::{
    DiffProjectionReport, DiffProjector, ProjectedSymbolChange, SymbolMutationType,
};
pub use diff_viewer::{GitDiffFile, GitDiffLine, GitDiffViewer};
pub use reviewer::{GitReviewer, ReviewFinding, ReviewReport};
pub use service::{ConflictFile, GitService, GitStatus};
pub use worktree::WorktreeManager;

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use tokio::process::Command;

    async fn init_test_git_repo() -> (tempfile::TempDir, GitService) {
        let dir = tempdir().expect("Failed to create tempdir");
        let path = dir.path().to_path_buf();

        // Run git init and config user for testing
        Command::new("git")
            .arg("init")
            .current_dir(&path)
            .output()
            .await
            .expect("git init failed");

        Command::new("git")
            .args(["config", "user.name", "Minicode Test"])
            .current_dir(&path)
            .output()
            .await
            .expect("git config user.name failed");

        Command::new("git")
            .args(["config", "user.email", "test@minicode.ai"])
            .current_dir(&path)
            .output()
            .await
            .expect("git config user.email failed");

        let git = GitService::new(path);
        (dir, git)
    }

    #[tokio::test]
    async fn test_is_git_repo_true_for_init_repo() {
        let (_dir, git) = init_test_git_repo().await;
        assert!(git.is_git_repo().await);
    }

    #[tokio::test]
    async fn test_is_git_repo_false_for_empty_dir() {
        let dir = tempdir().expect("tempdir");
        let git = GitService::new(dir.path().to_path_buf());
        assert!(!git.is_git_repo().await);
    }

    #[tokio::test]
    async fn test_status_untracked_and_clean() {
        let (dir, git) = init_test_git_repo().await;
        let file_path = dir.path().join("hello.txt");
        tokio::fs::write(&file_path, "hello world").await.unwrap();

        let status = git.get_status().await.unwrap();
        assert!(!status.is_clean);
        assert!(status.untracked.contains(&"hello.txt".to_string()));
    }

    #[tokio::test]
    async fn test_commit_and_log_flow() {
        let (dir, git) = init_test_git_repo().await;
        let file_path = dir.path().join("main.rs");
        tokio::fs::write(&file_path, "fn main() {}").await.unwrap();

        let commit_svc = GitCommitService::new(&git);
        let commit_sha = commit_svc
            .commit("feat: initial commit", None)
            .await
            .unwrap();
        assert!(!commit_sha.is_empty());

        let status = git.get_status().await.unwrap();
        assert!(status.is_clean);

        let log = git.log(5).await.unwrap();
        assert!(log.contains("feat: initial commit"));
    }

    #[tokio::test]
    async fn test_undo_last_commit_soft_reset() {
        let (dir, git) = init_test_git_repo().await;
        let commit_svc = GitCommitService::new(&git);

        // Commit 1
        tokio::fs::write(dir.path().join("file1.txt"), "1")
            .await
            .unwrap();
        commit_svc.commit("feat: file 1", None).await.unwrap();

        // Commit 2
        tokio::fs::write(dir.path().join("file2.txt"), "2")
            .await
            .unwrap();
        commit_svc.commit("feat: file 2", None).await.unwrap();

        // Soft undo
        commit_svc.undo_last_commit().await.unwrap();

        let status = git.get_status().await.unwrap();
        // file2 is still present in working tree
        assert!(
            status.staged.contains(&"file2.txt".to_string())
                || status.unstaged.contains(&"file2.txt".to_string())
        );
    }

    #[tokio::test]
    async fn test_branch_creation_and_switch() {
        let (dir, git) = init_test_git_repo().await;
        let commit_svc = GitCommitService::new(&git);

        tokio::fs::write(dir.path().join("file1.txt"), "1")
            .await
            .unwrap();
        commit_svc.commit("feat: initial", None).await.unwrap();

        commit_svc
            .create_branch("feature/test-branch")
            .await
            .unwrap();
        let current_branch = git.get_current_branch().await.unwrap();
        assert_eq!(current_branch, "feature/test-branch");

        let branches = commit_svc.list_branches().await.unwrap();
        assert!(branches.contains(&"feature/test-branch".to_string()));
    }

    #[tokio::test]
    async fn commit_with_stage_all_includes_unstaged_files() {
        let (_dir, git) = init_test_git_repo().await;
        let tracked = _dir.path().join("tracked.txt");
        std::fs::write(&tracked, "v1").expect("write tracked");
        let svc = GitCommitService::new(&git);
        svc.commit("initial", Some(&["tracked.txt".to_string()]))
            .await
            .expect("first commit");

        // Untracked file must be swept in by with_stage_all(true).
        std::fs::write(_dir.path().join("stray.txt"), "surprise").expect("write stray");
        let svc_all = GitCommitService::new(&git).with_stage_all(true);
        svc_all
            .commit("second", Some(&[]))
            .await
            .expect("second commit");

        let out = git
            .run_git(&["show", "--name-only", "--format=", "HEAD"])
            .await
            .unwrap_or_default();
        assert!(
            out.contains("stray.txt"),
            "stage_all must include untracked files, got: {out}"
        );
    }
}

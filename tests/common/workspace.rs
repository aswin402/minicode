use minicode::git::service::GitService;
use std::path::{Path, PathBuf};
use tempfile::{tempdir, TempDir};
use tokio::process::Command;

/// Reusable fixture for managing isolated temporary workspace directories in integration tests.
#[allow(dead_code)]
pub struct TestWorkspace {
    pub temp_dir: TempDir,
}

impl Default for TestWorkspace {
    fn default() -> Self {
        Self::new()
    }
}

#[allow(dead_code)]
impl TestWorkspace {
    /// Creates a new empty temporary directory that is automatically cleaned up when dropped.
    pub fn new() -> Self {
        let temp_dir = tempdir().expect("Failed to create temporary test directory");
        Self { temp_dir }
    }

    /// Returns a reference to the temporary workspace Path.
    pub fn path(&self) -> &Path {
        self.temp_dir.path()
    }

    /// Returns a PathBuf clone of the temporary workspace path.
    pub fn path_buf(&self) -> PathBuf {
        self.temp_dir.path().to_path_buf()
    }

    /// Initializes a valid git repository inside this temporary directory with standard config and initial commit.
    pub async fn new_git_repo() -> (Self, GitService) {
        let ws = Self::new();
        let path = ws.path();

        Command::new("git")
            .arg("init")
            .current_dir(path)
            .output()
            .await
            .expect("git init failed");

        Command::new("git")
            .args(["config", "user.name", "Test Agent"])
            .current_dir(path)
            .output()
            .await
            .expect("git config user.name failed");

        Command::new("git")
            .args(["config", "user.email", "test@minicode.ai"])
            .current_dir(path)
            .output()
            .await
            .expect("git config user.email failed");

        let git = GitService::new(path.to_path_buf());
        git.ensure_git_exclude();

        let readme_path = path.join("README.md");
        tokio::fs::write(&readme_path, "# Test Workspace\n")
            .await
            .expect("write README failed");

        Command::new("git")
            .args(["add", "README.md"])
            .current_dir(path)
            .output()
            .await
            .expect("git add failed");

        Command::new("git")
            .args(["commit", "-m", "initial commit"])
            .current_dir(path)
            .output()
            .await
            .expect("git commit failed");

        (ws, git)
    }
}

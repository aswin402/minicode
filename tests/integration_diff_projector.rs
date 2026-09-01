use minicode::context::graph::CodeGraph;
use minicode::git::diff_projector::{DiffProjector, SymbolMutationType};
use minicode::git::reviewer::GitReviewer;
use minicode::git::service::GitService;
use tempfile::tempdir;
use tokio::process::Command;

async fn init_git_repo() -> (tempfile::TempDir, GitService) {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().to_path_buf();

    Command::new("git")
        .arg("init")
        .current_dir(&path)
        .output()
        .await
        .expect("git init failed");

    Command::new("git")
        .args(["config", "user.name", "Test Agent"])
        .current_dir(&path)
        .output()
        .await
        .expect("git config name failed");

    Command::new("git")
        .args(["config", "user.email", "test@minicode.ai"])
        .current_dir(&path)
        .output()
        .await
        .expect("git config email failed");

    let git = GitService::new(path);
    (dir, git)
}

#[tokio::test]
async fn test_diff_projector_empty_on_clean_repo() {
    let (dir, _git) = init_git_repo().await;
    let report = DiffProjector::project_workspace_diff(dir.path(), false, None)
        .await
        .expect("Should project clean repo");

    assert_eq!(report.total_files, 0);
    assert_eq!(report.total_symbols_modified, 0);
    assert!(report.is_empty());
}

#[tokio::test]
async fn test_diff_projector_detects_added_symbol() {
    let (dir, git) = init_git_repo().await;
    let file_path = dir.path().join("src");
    tokio::fs::create_dir_all(&file_path)
        .await
        .expect("create src dir");

    let rs_file = file_path.join("calc.rs");
    tokio::fs::write(&rs_file, "pub fn initial() -> i32 { 0 }\n")
        .await
        .expect("write calc.rs");

    git.stage_files(None).await.expect("stage files");
    Command::new("git")
        .args(["commit", "-m", "initial commit"])
        .current_dir(dir.path())
        .output()
        .await
        .expect("commit failed");

    // Add a new function
    tokio::fs::write(
        &rs_file,
        "pub fn initial() -> i32 { 0 }\n\npub fn calculate_sum(a: i32, b: i32) -> i32 { a + b }\n",
    )
    .await
    .expect("modify calc.rs");

    let mut graph = CodeGraph::new();
    let _ = graph.build_graph(dir.path());

    let report = DiffProjector::project_workspace_diff(dir.path(), false, Some(&graph))
        .await
        .expect("Should project diff");

    assert!(!report.is_empty());
    assert_eq!(report.total_symbols_modified, 1);
    let added = &report.changes[0];
    assert_eq!(added.symbol_name, "calculate_sum");
    assert_eq!(added.mutation_type, SymbolMutationType::Added);
}

#[tokio::test]
async fn test_diff_projector_detects_body_and_signature_modifications() {
    let (dir, git) = init_git_repo().await;
    let file_path = dir.path().join("src");
    tokio::fs::create_dir_all(&file_path)
        .await
        .expect("create src dir");

    let rs_file = file_path.join("math.rs");
    tokio::fs::write(
        &rs_file,
        "pub fn double(x: i32) -> i32 {\n    x * 2\n}\n\npub fn triple(x: i32) -> i32 {\n    x * 3\n}\n",
    )
    .await
    .expect("write math.rs");

    git.stage_files(None).await.expect("stage files");
    Command::new("git")
        .args(["commit", "-m", "initial math"])
        .current_dir(dir.path())
        .output()
        .await
        .expect("commit failed");

    // 1. Modify body of double, modify signature of triple
    tokio::fs::write(
        &rs_file,
        "pub fn double(x: i32) -> i32 {\n    let factor = 2;\n    x * factor\n}\n\npub fn triple(x: i64, extra: i64) -> i64 {\n    x * 3 + extra\n}\n",
    )
    .await
    .expect("modify math.rs");

    let mut graph = CodeGraph::new();
    let _ = graph.build_graph(dir.path());

    let report = DiffProjector::project_workspace_diff(dir.path(), false, Some(&graph))
        .await
        .expect("project diff");

    assert_eq!(report.total_symbols_modified, 2);

    let double_sym = report
        .changes
        .iter()
        .find(|c| c.symbol_name == "double")
        .expect("find double");
    assert_eq!(double_sym.mutation_type, SymbolMutationType::BodyModified);
    assert!(!double_sym.is_breaking);

    let triple_sym = report
        .changes
        .iter()
        .find(|c| c.symbol_name == "triple")
        .expect("find triple");
    assert_eq!(
        triple_sym.mutation_type,
        SymbolMutationType::SignatureChanged
    );
    assert!(triple_sym.is_breaking);

    let md = DiffProjector::format_markdown(&report);
    assert!(md.contains("Symbol-Level Diff Projection"));
    assert!(md.contains("Breaking API Mutations"));
}

#[tokio::test]
async fn test_git_reviewer_sixth_pillar_structural_impact() {
    let (dir, git) = init_git_repo().await;
    let file_path = dir.path().join("src");
    tokio::fs::create_dir_all(&file_path)
        .await
        .expect("create src dir");

    let rs_file = file_path.join("auth.rs");
    tokio::fs::write(
        &rs_file,
        "pub fn verify_token(token: &str) -> bool {\n    !token.is_empty()\n}\n",
    )
    .await
    .expect("write auth.rs");

    git.stage_files(None).await.expect("stage files");
    Command::new("git")
        .args(["commit", "-m", "init auth"])
        .current_dir(dir.path())
        .output()
        .await
        .expect("commit failed");

    // Change public signature
    tokio::fs::write(
        &rs_file,
        "pub fn verify_token(token: &str, secret: &str, expires: u64) -> Result<bool, ()> {\n    Ok(!token.is_empty())\n}\n",
    )
    .await
    .expect("modify auth.rs signature");

    let review = GitReviewer::review_workspace(dir.path(), false)
        .await
        .expect("review workspace");

    assert!(review.findings.iter().any(|f| {
        f.category == "Structural & Public API Impact" && f.title.contains("Breaking API Change")
    }));
}

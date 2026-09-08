use minicode::git::commit_synth::{CommitType, SemanticCommitSynthesizer};
use minicode::tools::registry::git_tools;
use serde_json::json;
use tempfile::tempdir;
use tokio::process::Command;

async fn setup_git_workspace() -> tempfile::TempDir {
    let dir = tempdir().expect("Failed to create tempdir");
    let path = dir.path();

    let _ = Command::new("git")
        .args(["init", "-b", "main"])
        .current_dir(path)
        .output()
        .await;

    let _ = Command::new("git")
        .args(["config", "user.name", "Minicode Test"])
        .current_dir(path)
        .output()
        .await;

    let _ = Command::new("git")
        .args(["config", "user.email", "test@minicode.ai"])
        .current_dir(path)
        .output()
        .await;

    // Initial file & commit
    tokio::fs::write(path.join("README.md"), "# Test Project\n")
        .await
        .expect("Failed to write README");

    let _ = Command::new("git")
        .args(["add", "README.md"])
        .current_dir(path)
        .output()
        .await;

    let _ = Command::new("git")
        .args(["commit", "-m", "chore: initial commit"])
        .current_dir(path)
        .output()
        .await;

    dir
}

#[test]
fn test_semantic_commit_synthesis_diff_analysis() {
    let sample_diff = r#"
diff --git a/src/git/commit_synth.rs b/src/git/commit_synth.rs
index 0000000..1111111 100644
--- /dev/null
+++ b/src/git/commit_synth.rs
@@ -0,0 +1,50 @@
+pub struct SemanticCommitSynthesizer;
+impl SemanticCommitSynthesizer {
+    pub fn synthesize() {}
+}
diff --git a/tests/integration_commit_synthesis.rs b/tests/integration_commit_synthesis.rs
--- a/tests/integration_commit_synthesis.rs
+++ b/tests/integration_commit_synthesis.rs
@@ -1,5 +1,10 @@
-old test
+new test line 1
+new test line 2
"#;

    let affected = vec![
        "src/git/commit_synth.rs".to_string(),
        "tests/integration_commit_synthesis.rs".to_string(),
    ];

    let report = SemanticCommitSynthesizer::analyze_diff(
        sample_diff,
        &affected,
        Some("Implement semantic commit synthesis tool"),
    );

    assert_eq!(report.affected_files.len(), 2);
    assert_eq!(report.unified_proposal.commit_type, CommitType::Feat);
    assert_eq!(report.unified_proposal.scope, Some("git".to_string()));
    assert!(report.insertions > 0);
    assert!(report
        .unified_proposal
        .format_title()
        .contains("feat(git):"));
    assert!(report
        .unified_proposal
        .format_full_message()
        .contains("feat(git):"));

    let md = report.format_markdown();
    assert!(md.contains("Synthesized Semantic Commits & Release Changelog"));
    assert!(md.contains("Unified Conventional Commit"));
    assert!(md.contains("Release Changelog Entry"));
}

#[test]
fn test_commit_type_heuristic_detection() {
    // 1. Fix detection
    let fix_type = SemanticCommitSynthesizer::detect_commit_type(
        &["src/session/store.rs".to_string()],
        "+ // fix panic when loading corrupted session\n",
        Some("Fix panic when loading corrupted session"),
    );
    assert_eq!(fix_type, CommitType::Fix);

    // 2. Refactor detection
    let refactor_type = SemanticCommitSynthesizer::detect_commit_type(
        &["src/tools/mod.rs".to_string()],
        "",
        Some("Refactor tool dispatcher to use enum matching"),
    );
    assert_eq!(refactor_type, CommitType::Refactor);

    // 3. Tests only detection
    let test_type =
        SemanticCommitSynthesizer::detect_commit_type(&["tests/test_foo.rs".to_string()], "", None);
    assert_eq!(test_type, CommitType::Test);

    // 4. Docs only detection
    let docs_type = SemanticCommitSynthesizer::detect_commit_type(
        &["docs/architecture.md".to_string()],
        "",
        None,
    );
    assert_eq!(docs_type, CommitType::Docs);

    // 5. Build/Deps only detection
    let build_type = SemanticCommitSynthesizer::detect_commit_type(
        &["Cargo.toml".to_string(), "Cargo.lock".to_string()],
        "",
        None,
    );
    assert_eq!(build_type, CommitType::Build);
}

#[test]
fn test_primary_scope_and_atomic_partitioning() {
    // Scope detection
    assert_eq!(
        SemanticCommitSynthesizer::detect_primary_scope(&["src/context/flaky.rs".to_string()]),
        Some("context/flaky".to_string())
    );
    assert_eq!(
        SemanticCommitSynthesizer::detect_primary_scope(&[
            "src/tools/registry/git_tools.rs".to_string()
        ]),
        Some("tools/registry".to_string())
    );
    assert_eq!(
        SemanticCommitSynthesizer::detect_primary_scope(&["src/ui/modal.rs".to_string()]),
        Some("ui".to_string())
    );
    assert_eq!(
        SemanticCommitSynthesizer::detect_primary_scope(&["tests/test_app.rs".to_string()]),
        Some("tests".to_string())
    );
    assert_eq!(
        SemanticCommitSynthesizer::detect_primary_scope(&["Cargo.toml".to_string()]),
        Some("deps".to_string())
    );

    // Multi-domain atomic partitioning
    let multi_files = vec![
        "src/git/commit_synth.rs".to_string(),
        "tests/integration_commit_synthesis.rs".to_string(),
        "onpkg_docs/todo.md".to_string(),
        "Cargo.toml".to_string(),
    ];

    let report = SemanticCommitSynthesizer::analyze_diff(
        "",
        &multi_files,
        Some("Phase 103 semantic commit synthesis"),
    );

    assert_eq!(report.atomic_proposals.len(), 4);
    assert_eq!(report.atomic_proposals[0].commit_type, CommitType::Feat);
    assert_eq!(report.atomic_proposals[1].commit_type, CommitType::Test);
    assert_eq!(report.atomic_proposals[2].commit_type, CommitType::Docs);
    assert_eq!(report.atomic_proposals[3].commit_type, CommitType::Build);

    let md = report.format_markdown();
    assert!(md.contains("Atomic Commit Sequence (Multi-Step Option)"));
}

#[test]
fn test_changelog_generation_keep_a_changelog() {
    let changelog_unreleased = SemanticCommitSynthesizer::generate_changelog_draft(
        CommitType::Feat,
        "automated semantic commit synthesis & conventional changelog generator",
        &["src/git/commit_synth.rs".to_string()],
        &Some("git".to_string()),
        None,
    );

    assert!(changelog_unreleased.contains("## [Unreleased] — "));
    assert!(changelog_unreleased.contains("### Added"));
    assert!(changelog_unreleased.contains(
        "- **git**: automated semantic commit synthesis & conventional changelog generator"
    ));
    assert!(changelog_unreleased.contains("Affected: src/git/commit_synth.rs"));

    let changelog_v033 = SemanticCommitSynthesizer::generate_changelog_draft(
        CommitType::Fix,
        "resolve race condition in session snapshot",
        &["src/session/store.rs".to_string()],
        &Some("session".to_string()),
        Some("0.3.3"),
    );

    assert!(changelog_v033.contains("## [0.3.3] — "));
    assert!(changelog_v033.contains("### Fixed"));
    assert!(changelog_v033.contains("- **session**: resolve race condition in session snapshot"));
}

#[tokio::test]
async fn test_tool_registry_synthesize_commits_dispatch() {
    let ws = setup_git_workspace().await;
    let ws_path = ws.path();

    // 1. Create changes in working tree
    tokio::fs::create_dir_all(ws_path.join("src"))
        .await
        .expect("Failed to create src");
    tokio::fs::write(
        ws_path.join("src/lib.rs"),
        "pub fn hello() -> &'static str { \"world\" }\n",
    )
    .await
    .expect("Failed to write lib.rs");

    // 2. Dispatch synthesize
    let synth_args = json!({
        "action": "synthesize",
        "task_hint": "Add hello world library entrypoint"
    });
    let result = git_tools::dispatch("synthesize_commits", &synth_args, ws_path)
        .await
        .expect("Dispatch should return Some")
        .expect("Execution should succeed");

    assert!(result.contains("Synthesized Semantic Commits & Release Changelog"));
    assert!(result.contains("feat:"));
    assert!(result.contains("hello world library entrypoint"));

    // 3. Dispatch changelog
    let changelog_args = json!({
        "action": "changelog",
        "version": "0.3.3",
        "task_hint": "Add hello world entrypoint"
    });
    let changelog_res = git_tools::dispatch("synthesize_commits", &changelog_args, ws_path)
        .await
        .expect("Dispatch should return Some")
        .expect("Execution should succeed");

    assert!(changelog_res.contains("## [0.3.3]"));
    assert!(changelog_res.contains("### Added"));

    // 4. Dispatch commit
    let commit_args = json!({
        "action": "commit",
        "task_hint": "Add hello world library entrypoint"
    });
    let commit_res = git_tools::dispatch("synthesize_commits", &commit_args, ws_path)
        .await
        .expect("Dispatch should return Some")
        .expect("Execution should succeed");

    assert!(commit_res.contains("Created commit"));
    assert!(commit_res.contains("feat:"));

    // Verify git log contains new commit
    let log_out = Command::new("git")
        .args(["log", "-n", "1", "--oneline"])
        .current_dir(ws_path)
        .output()
        .await
        .expect("git log failed");
    let log_str = String::from_utf8_lossy(&log_out.stdout);
    assert!(log_str.contains("feat: add hello world library entrypoint"));

    // 5. Dispatch invalid action returns error
    let invalid_args = json!({
        "action": "unknown_action"
    });
    let invalid_res = git_tools::dispatch("synthesize_commits", &invalid_args, ws_path)
        .await
        .expect("Dispatch should return Some");
    assert!(invalid_res.is_err());
}

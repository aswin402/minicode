/// Integration tests for Phase 48: Interactive Git Diff Viewer & Multi-Agent Code Review
use minicode::git::diff_viewer::GitDiffViewer;
use minicode::git::reviewer::GitReviewer;
use minicode::tools::registry::git_tools;
use minicode::tools::ToolRegistry;
use minicode::ui::input::PALETTE_COMMANDS;
use minicode::ui::modal::ModalState;

#[test]
fn test_git_review_schema_in_registry() {
    let schemas = git_tools::get_schemas();
    let names: Vec<String> = schemas.into_iter().map(|s| s.name).collect();
    assert!(names.contains(&"git_review".to_string()));
    assert!(names.contains(&"git_diff".to_string()));
    assert!(names.contains(&"git_status".to_string()));

    let global_schemas = ToolRegistry::get_tool_schemas();
    let global_names: Vec<String> = global_schemas.into_iter().map(|s| s.name).collect();
    assert!(global_names.contains(&"git_review".to_string()));
}

#[test]
fn test_slash_commands_contain_diff_and_review() {
    let cmd_names: Vec<&str> = PALETTE_COMMANDS.iter().map(|c| c.slash_name).collect();
    assert!(cmd_names.contains(&"/diff"));
    assert!(cmd_names.contains(&"/review"));
}

#[test]
fn test_git_diff_viewer_parse_raw_diff() {
    let raw_diff = r#"diff --git a/src/main.rs b/src/main.rs
index 1234567..89abcdef 100644
--- a/src/main.rs
+++ b/src/main.rs
@@ -10,4 +10,6 @@ fn main() {
     println!("hello");
+    let secret = "sk-live-12345";
+    println!("world");
-    panic!("error");
 }
"#;

    let files = GitDiffViewer::parse_raw_diff(raw_diff, false);
    assert_eq!(files.len(), 1);
    let f = &files[0];
    assert_eq!(f.path, "src/main.rs");
    assert_eq!(f.status_char, 'M');
    assert_eq!(f.additions, 2);
    assert_eq!(f.deletions, 1);
    assert!(!f.lines.is_empty());

    assert!(f
        .lines
        .iter()
        .any(|l| l.tag == '+' && l.content.contains("sk-live-12345")));
    assert!(f
        .lines
        .iter()
        .any(|l| l.tag == '-' && l.content.contains("panic!")));
}

#[test]
fn test_git_diff_modal_state_instantiation() {
    let raw_diff = r#"diff --git a/Cargo.toml b/Cargo.toml
index 1111111..2222222 100644
--- a/Cargo.toml
+++ b/Cargo.toml
@@ -1,3 +1,3 @@
 [package]
-version = "0.0.56"
+version = "0.0.57"
"#;

    let files = GitDiffViewer::parse_raw_diff(raw_diff, false);
    let modal = ModalState::new_git_diff(files, false);

    if let ModalState::GitDiff {
        diff_files,
        selected_file_index,
        scroll_offset,
        staged_view,
    } = modal
    {
        assert_eq!(diff_files.len(), 1);
        assert_eq!(selected_file_index, 0);
        assert_eq!(scroll_offset, 0);
        assert!(!staged_view);
    } else {
        panic!("Expected ModalState::GitDiff");
    }
}

#[tokio::test]
async fn test_git_reviewer_clean_repo_score() {
    let temp_dir = tempfile::tempdir().unwrap();
    let report = GitReviewer::review_workspace(temp_dir.path(), false)
        .await
        .unwrap();
    assert_eq!(report.total_score, 100);
}

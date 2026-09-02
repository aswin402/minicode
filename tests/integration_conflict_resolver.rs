use minicode::git::conflict_resolver::{ConflictResolver, MergeStrategy};
use serde_json::json;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_parse_and_resolve_import_conflict() {
    let conflicted_content = r#"
<<<<<<< HEAD
use std::collections::HashMap;
use std::sync::Arc;
=======
use std::collections::BTreeMap;
use std::sync::Arc;
>>>>>>> feature-branch

fn main() {}
"#;

    let blocks = ConflictResolver::parse_conflict_blocks(conflicted_content);
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].ours_label, "HEAD");
    assert_eq!(blocks[0].theirs_label, "feature-branch");

    let (resolved, count) =
        ConflictResolver::resolve_content(conflicted_content, MergeStrategy::Auto);
    assert_eq!(count, 1);
    assert!(!resolved.contains("<<<<<<<"));
    assert!(!resolved.contains("======="));
    assert!(!resolved.contains(">>>>>>>"));
    assert!(resolved.contains("use std::collections::HashMap;"));
    assert!(resolved.contains("use std::collections::BTreeMap;"));
    assert!(resolved.contains("use std::sync::Arc;"));
}

#[test]
fn test_resolve_non_overlapping_functions() {
    let conflicted_content = r#"
<<<<<<< HEAD
pub fn function_one() -> i32 {
    1
}
=======
pub fn function_two() -> i32 {
    2
}
>>>>>>> feature-branch
"#;

    let (resolved, count) =
        ConflictResolver::resolve_content(conflicted_content, MergeStrategy::Auto);
    assert_eq!(count, 1);
    assert!(resolved.contains("pub fn function_one()"));
    assert!(resolved.contains("pub fn function_two()"));
    assert!(!resolved.contains("<<<<<<<"));
}

#[tokio::test]
async fn test_resolve_git_conflicts_tool_dispatch() {
    let dir = tempdir().expect("tempdir");
    let file = dir.path().join("lib.rs");

    fs::write(
        &file,
        r#"
<<<<<<< HEAD
pub fn alpha() {}
=======
pub fn beta() {}
>>>>>>> incoming
"#,
    )
    .expect("write conflicted lib.rs");

    let args = json!({
        "strategy": "auto",
        "stage": false
    });

    let res =
        minicode::tools::registry::git_tools::dispatch("resolve_git_conflicts", &args, dir.path())
            .await;

    assert!(res.is_some());
    let output = res.unwrap().expect("tool execution success");
    assert!(output.contains("Git Conflict Resolution Report"));
    assert!(output.contains("lib.rs"));

    let disk_content = fs::read_to_string(&file).expect("read file");
    assert!(!disk_content.contains("<<<<<<<"));
    assert!(disk_content.contains("pub fn alpha()"));
    assert!(disk_content.contains("pub fn beta()"));
}

use minicode::context::graph_sync::GraphSynchronizer;
use serde_json::json;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_graph_sync_full_rebuild() {
    let dir = tempdir().expect("tempdir");
    let src_dir = dir.path().join("src");
    fs::create_dir_all(&src_dir).expect("create src");

    fs::write(
        src_dir.join("main.rs"),
        r#"
fn main() {
    helper();
}

fn helper() {}
"#,
    )
    .expect("write main.rs");

    let stats = GraphSynchronizer::sync(dir.path(), None, true).expect("sync graph full rebuild");

    assert!(stats.was_full_rebuild);
    assert!(stats.total_nodes >= 2);
    assert!(stats.files_scanned >= 1);
}

#[test]
fn test_graph_sync_incremental() {
    let dir = tempdir().expect("tempdir");
    let src_dir = dir.path().join("src");
    fs::create_dir_all(&src_dir).expect("create src");

    let file_a = src_dir.join("mod_a.rs");
    fs::write(&file_a, "pub fn alpha() {}").expect("write mod_a.rs");

    // Initial build & cache
    let stats1 = GraphSynchronizer::sync(dir.path(), None, true).expect("initial full sync");
    assert!(stats1.was_full_rebuild);

    // Modify file
    fs::write(&file_a, "pub fn alpha() {}\npub fn extra() {}").expect("update mod_a.rs");

    // Incremental sync
    let stats2 = GraphSynchronizer::sync(dir.path(), None, false).expect("incremental sync");
    assert!(!stats2.was_full_rebuild);
    assert!(stats2.total_nodes > 0);
}

#[tokio::test]
async fn test_sync_code_graph_tool_dispatch() {
    let dir = tempdir().expect("tempdir");
    let src_dir = dir.path().join("src");
    fs::create_dir_all(&src_dir).expect("create src");

    fs::write(src_dir.join("main.rs"), "fn main() {}").expect("write main.rs");

    let args = json!({
        "force_full": true
    });

    let res =
        minicode::tools::registry::context_tools::dispatch("sync_code_graph", &args, dir.path())
            .await;

    assert!(res.is_some());
    let output = res.unwrap().expect("tool execution success");
    assert!(output.contains("AST CodeGraph Synchronization Report"));
}

//! Integration tests for Multi-File Atomic Workspace Transactions & Semantic Rollback Journal (Phase 96, v0.2.7).

use minicode::session::transaction::{TransactionManager, TransactionStatus};
use minicode::tools::ToolRegistry;
use serde_json::json;
use tempfile::TempDir;

#[tokio::test]
async fn test_begin_and_get_transaction_status() {
    let temp = TempDir::new().expect("Failed to create tempdir");
    let ws = temp.path();

    // 1. Initial status when no transaction is open
    let res = ToolRegistry::dispatch(
        ws,
        "call_status_1",
        "get_transaction_status",
        &json!({}),
        None,
        1,
    )
    .await;
    assert!(res.success);
    assert!(res.output.contains("No active workspace transaction"));

    // 2. Begin transaction via tool registry
    let res = ToolRegistry::dispatch(
        ws,
        "call_begin_1",
        "begin_transaction",
        &json!({ "description": "Refactor database models" }),
        None,
        1,
    )
    .await;
    assert!(res.success);
    assert!(res.output.contains("Began atomic workspace transaction"));
    assert!(res.output.contains("Refactor database models"));

    // 3. Status now reports Active
    let res = ToolRegistry::dispatch(
        ws,
        "call_status_2",
        "get_transaction_status",
        &json!({}),
        None,
        1,
    )
    .await;
    assert!(res.success);
    assert!(res
        .output
        .contains("WORKSPACE TRANSACTION RECEIPT: [Active]"));
    assert!(res.output.contains("Refactor database models"));

    // Cleanup: commit
    let res = ToolRegistry::dispatch(
        ws,
        "call_commit_1",
        "commit_transaction",
        &json!({}),
        None,
        1,
    )
    .await;
    assert!(res.success);
    assert!(res.output.contains("[Committed]"));
}

#[tokio::test]
async fn test_multi_file_mutation_and_commit() {
    let temp = TempDir::new().expect("Failed to create tempdir");
    let ws = temp.path();

    // Pre-populate file_a.txt and src/lib.rs
    let file_a = ws.join("file_a.txt");
    std::fs::write(&file_a, "alpha beta gamma").expect("write file_a");

    let src_dir = ws.join("src");
    std::fs::create_dir_all(&src_dir).expect("create src dir");
    let file_rs = src_dir.join("lib.rs");
    std::fs::write(
        &file_rs,
        "pub fn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n",
    )
    .expect("write file_rs");

    // Begin transaction
    let res = ToolRegistry::dispatch(
        ws,
        "call_begin",
        "begin_transaction",
        &json!({ "description": "Cross-file refactoring" }),
        None,
        1,
    )
    .await;
    assert!(res.success);

    // Patch file_a
    let res = ToolRegistry::dispatch(
        ws,
        "call_patch",
        "patch_file",
        &json!({
            "path": "file_a.txt",
            "search_block": "beta",
            "replace_block": "BETA_MODIFIED"
        }),
        None,
        1,
    )
    .await;
    assert!(res.success);

    // AST Replace node in src/lib.rs
    let res = ToolRegistry::dispatch(
        ws,
        "call_ast",
        "ast_replace_node",
        &json!({
            "path": "src/lib.rs",
            "symbol": "add",
            "replacement_code": "pub fn add(a: i32, b: i32) -> i32 {\n    // Updated with tracing\n    a + b\n}"
        }),
        None,
        1,
    )
    .await;
    assert!(res.success);

    // Create a new file_b
    let res = ToolRegistry::dispatch(
        ws,
        "call_write_new",
        "write_file",
        &json!({
            "path": "file_b.txt",
            "content": "brand new file content"
        }),
        None,
        1,
    )
    .await;
    assert!(res.success);

    // Inspect status
    let active = TransactionManager::get_active(ws)
        .expect("get_active")
        .expect("should have active transaction");
    assert_eq!(active.files.len(), 3);

    // Commit transaction
    let res =
        ToolRegistry::dispatch(ws, "call_commit", "commit_transaction", &json!({}), None, 1).await;
    assert!(res.success);
    assert!(res.output.contains("Files Affected : 3"));

    // Verify disk content
    assert!(std::fs::read_to_string(&file_a)
        .expect("read file_a")
        .contains("BETA_MODIFIED"));
    assert!(std::fs::read_to_string(&file_rs)
        .expect("read file_rs")
        .contains("// Updated with tracing"));
    assert_eq!(
        std::fs::read_to_string(ws.join("file_b.txt")).expect("read file_b"),
        "brand new file content"
    );
}

#[tokio::test]
async fn test_atomic_rollback_restores_all_files() {
    let temp = TempDir::new().expect("Failed to create tempdir");
    let ws = temp.path();

    // Baseline existing files
    let file_1 = ws.join("config.txt");
    std::fs::write(&file_1, "PORT=8080\nDEBUG=false\n").expect("write config.txt");

    let src_dir = ws.join("src");
    std::fs::create_dir_all(&src_dir).expect("create src dir");
    let code_file = src_dir.join("calc.rs");
    std::fs::write(&code_file, "pub fn calculate() -> usize {\n    42\n}\n")
        .expect("write calc.rs");

    // Begin transaction
    ToolRegistry::dispatch(
        ws,
        "call_begin",
        "begin_transaction",
        &json!({ "description": "High stakes multi-file patch that will fail verification" }),
        None,
        1,
    )
    .await;

    // Mutate file 1
    ToolRegistry::dispatch(
        ws,
        "call_patch_1",
        "patch_file",
        &json!({
            "path": "config.txt",
            "search_block": "DEBUG=false",
            "replace_block": "DEBUG=true"
        }),
        None,
        1,
    )
    .await;

    // Mutate file 2 via ast_replace_node
    ToolRegistry::dispatch(
        ws,
        "call_ast_2",
        "ast_replace_node",
        &json!({
            "path": "src/calc.rs",
            "symbol": "calculate",
            "replacement_code": "pub fn calculate() -> usize {\n    // Broken edit\n    100\n}"
        }),
        None,
        1,
    )
    .await;

    // Create a new scratch file
    let scratch = ws.join("scratch_temp.rs");
    ToolRegistry::dispatch(
        ws,
        "call_write_scratch",
        "write_file",
        &json!({
            "path": "scratch_temp.rs",
            "content": "fn temporary_helper() {}"
        }),
        None,
        1,
    )
    .await;

    assert!(scratch.exists());
    assert!(std::fs::read_to_string(&file_1)
        .unwrap()
        .contains("DEBUG=true"));
    assert!(std::fs::read_to_string(&code_file)
        .unwrap()
        .contains("Broken edit"));

    // Trigger Rollback!
    let res = ToolRegistry::dispatch(
        ws,
        "call_rollback",
        "rollback_transaction",
        &json!({ "reason": "Pre-completion verification failed Gate 1" }),
        None,
        1,
    )
    .await;

    assert!(res.success);
    assert!(res
        .output
        .contains("WORKSPACE TRANSACTION ROLLED BACK (ATOMIC RESTORE)"));
    assert!(res
        .output
        .contains("Pre-completion verification failed Gate 1"));
    assert!(res.output.contains("restored: config.txt"));
    assert!(res.output.contains("restored: src/calc.rs"));
    assert!(res.output.contains("purged: scratch_temp.rs"));

    // Verify pristine baseline restoration
    assert_eq!(
        std::fs::read_to_string(&file_1).unwrap(),
        "PORT=8080\nDEBUG=false\n"
    );
    assert_eq!(
        std::fs::read_to_string(&code_file).unwrap(),
        "pub fn calculate() -> usize {\n    42\n}\n"
    );
    // Scratch file completely purged
    assert!(!scratch.exists());

    // Active transaction cleared
    assert!(TransactionManager::get_active(ws).unwrap().is_none());
}

#[tokio::test]
async fn test_duplicate_begin_transaction_rejected() {
    let temp = TempDir::new().expect("Failed to create tempdir");
    let ws = temp.path();

    let res1 = ToolRegistry::dispatch(
        ws,
        "call_1",
        "begin_transaction",
        &json!({ "description": "First transaction" }),
        None,
        1,
    )
    .await;
    assert!(res1.success);

    let res2 = ToolRegistry::dispatch(
        ws,
        "call_2",
        "begin_transaction",
        &json!({ "description": "Second transaction attempt" }),
        None,
        1,
    )
    .await;
    assert!(!res2.success);
    assert!(res2.output.contains("already active"));

    // Commit first
    let res3 =
        ToolRegistry::dispatch(ws, "call_3", "commit_transaction", &json!({}), None, 1).await;
    assert!(res3.success);

    // Now second begin succeeds
    let res4 = ToolRegistry::dispatch(
        ws,
        "call_4",
        "begin_transaction",
        &json!({ "description": "Second transaction valid" }),
        None,
        1,
    )
    .await;
    assert!(res4.success);
}

#[tokio::test]
async fn test_consecutive_edits_preserve_original_baseline() {
    let temp = TempDir::new().expect("Failed to create tempdir");
    let ws = temp.path();

    let target = ws.join("target.txt");
    std::fs::write(&target, "VERSION_0_ORIGINAL").expect("write target");

    TransactionManager::begin(ws, "Multiple sequential edits on same file").expect("begin");

    // Edit 1
    ToolRegistry::dispatch(
        ws,
        "edit_1",
        "write_file",
        &json!({ "path": "target.txt", "content": "VERSION_1_EDIT" }),
        None,
        1,
    )
    .await;

    // Edit 2
    ToolRegistry::dispatch(
        ws,
        "edit_2",
        "write_file",
        &json!({ "path": "target.txt", "content": "VERSION_2_EDIT" }),
        None,
        1,
    )
    .await;

    // Edit 3
    ToolRegistry::dispatch(
        ws,
        "edit_3",
        "write_file",
        &json!({ "path": "target.txt", "content": "VERSION_3_FINAL" }),
        None,
        1,
    )
    .await;

    assert_eq!(std::fs::read_to_string(&target).unwrap(), "VERSION_3_FINAL");

    // Rollback
    let rollback_res = TransactionManager::rollback(ws, None, Some("Reverting all 3 iterations"))
        .expect("rollback");
    assert_eq!(rollback_res.restored_files.len(), 1);

    // Must be VERSION_0_ORIGINAL, NOT VERSION_1 or VERSION_2!
    assert_eq!(
        std::fs::read_to_string(&target).unwrap(),
        "VERSION_0_ORIGINAL"
    );
}

#[tokio::test]
async fn test_transaction_manifest_listing() {
    let temp = TempDir::new().expect("Failed to create tempdir");
    let ws = temp.path();

    assert_eq!(TransactionManager::list(ws).unwrap().len(), 0);

    // Transaction 1 committed
    TransactionManager::begin(ws, "First tx").unwrap();
    TransactionManager::commit(ws, None).unwrap();

    // Transaction 2 rolled back
    TransactionManager::begin(ws, "Second tx").unwrap();
    TransactionManager::rollback(ws, None, None).unwrap();

    let list = TransactionManager::list(ws).unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].status, TransactionStatus::RolledBack);
    assert_eq!(list[1].status, TransactionStatus::Committed);
}

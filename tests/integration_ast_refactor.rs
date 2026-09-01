use minicode::context::ast_refactor::AstRefactorer;
use serde_json::json;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_extract_function() {
    let dir = tempdir().expect("tempdir");
    let src_dir = dir.path().join("src");
    fs::create_dir_all(&src_dir).expect("create src");

    let initial_code = r#"pub fn process_order(price: f64, tax: f64) -> f64 {
    let subtotal = price * 1.1;
    let total = subtotal + tax;
    total
}
"#;
    let file_path = src_dir.join("order.rs");
    fs::write(&file_path, initial_code).expect("write order.rs");

    let res = AstRefactorer::extract_function(
        dir.path(),
        "src/order.rs",
        2,
        3,
        "calculate_total",
        "price: f64, tax: f64",
        "price, tax",
        Some("f64"),
        false,
    )
    .expect("extract function");

    assert_eq!(res.files_modified.len(), 1);
    let updated = fs::read_to_string(&file_path).expect("read order.rs");
    assert!(updated.contains("calculate_total(price, tax);"));
    assert!(updated.contains("fn calculate_total(price: f64, tax: f64) -> f64"));
}

#[test]
fn test_rename_symbol() {
    let dir = tempdir().expect("tempdir");
    let src_dir = dir.path().join("src");
    fs::create_dir_all(&src_dir).expect("create src");

    let file_a = src_dir.join("user.rs");
    fs::write(
        &file_a,
        r#"
pub struct UserRecord {
    pub legacy_id: u64,
}
"#,
    )
    .expect("write user.rs");

    let res =
        AstRefactorer::rename_symbol(dir.path(), "legacy_id", "account_id", Some("src/user.rs"))
            .expect("rename symbol");

    assert_eq!(res.files_modified.len(), 1);
    let updated = fs::read_to_string(&file_a).expect("read user.rs");
    assert!(updated.contains("pub account_id: u64,"));
    assert!(!updated.contains("legacy_id"));
}

#[test]
fn test_inline_variable() {
    let dir = tempdir().expect("tempdir");
    let src_dir = dir.path().join("src");
    fs::create_dir_all(&src_dir).expect("create src");

    let initial = r#"pub fn calculate(base: i32) -> i32 {
    let multiplier = 10;
    base * multiplier
}
"#;
    let file_path = src_dir.join("math.rs");
    fs::write(&file_path, initial).expect("write math.rs");

    let res = AstRefactorer::inline_variable(dir.path(), "src/math.rs", "multiplier")
        .expect("inline variable");

    assert_eq!(res.files_modified.len(), 1);
    let updated = fs::read_to_string(&file_path).expect("read math.rs");
    assert!(!updated.contains("let multiplier = 10;"));
    assert!(updated.contains("base * 10"));
}

#[tokio::test]
async fn test_ast_refactor_tool_dispatch() {
    let dir = tempdir().expect("tempdir");
    let src_dir = dir.path().join("src");
    fs::create_dir_all(&src_dir).expect("create src");

    let file_path = src_dir.join("service.rs");
    fs::write(
        &file_path,
        r#"
pub fn run_job() {
    let old_metric = 42;
    let _res = old_metric + 1;
}
"#,
    )
    .expect("write service.rs");

    let args = json!({
        "action": "rename_symbol",
        "file_path": "src/service.rs",
        "target_symbol": "old_metric",
        "new_name": "current_metric"
    });

    let res =
        minicode::tools::registry::context_tools::dispatch("ast_refactor", &args, dir.path()).await;

    assert!(res.is_some());
    let output = res.unwrap().expect("tool execution success");
    assert!(output.contains("Refactored `rename_symbol `old_metric` → `current_metric``"));
    assert!(output.contains("```diff"));
}

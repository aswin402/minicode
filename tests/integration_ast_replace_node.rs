use minicode::session::backup::BackupManager;
use minicode::tools::ToolRegistry;
use serde_json::json;
use std::fs;
use tempfile::tempdir;

#[tokio::test]
async fn test_ast_replace_node_dispatch_rust_function() {
    let dir = tempdir().expect("create temp dir");
    let ws = dir.path();

    let rs_path = ws.join("src").join("calc.rs");
    fs::create_dir_all(rs_path.parent().unwrap()).unwrap();
    fs::write(
        &rs_path,
        r#"
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}

pub fn multiply(a: i32, b: i32) -> i32 {
    a * b
}
"#,
    )
    .unwrap();

    let new_code = r#"pub fn add(a: i32, b: i32) -> i32 {
    // Optimized addition with overflow protection
    a.saturating_add(b)
}"#;

    let res = ToolRegistry::dispatch(
        ws,
        "test_call_1",
        "ast_replace_node",
        &json!({
            "path": "src/calc.rs",
            "symbol": "add",
            "replacement_code": new_code,
        }),
        None,
        1,
    )
    .await;

    assert!(res.success, "Dispatch failed: {}", res.output);
    assert!(res.output.contains("Successfully replaced AST node `add`"));
    assert!(res.output.contains("VALID"));
    assert!(res.output.contains("Semantic AST Diff"));

    let updated_content = fs::read_to_string(&rs_path).unwrap();
    assert!(updated_content.contains("a.saturating_add(b)"));
    assert!(!updated_content.contains("a + b"));
    assert!(updated_content.contains("pub fn multiply(a: i32, b: i32) -> i32"));
}

#[tokio::test]
async fn test_ast_replace_node_dispatch_python_method_inside_class() {
    let dir = tempdir().expect("create temp dir");
    let ws = dir.path();

    let py_path = ws.join("service.py");
    fs::write(
        &py_path,
        r#"
class AuthService:
    def authenticate(self, user, secret):
        return False
"#,
    )
    .unwrap();

    // The replacement code has 0 leading indentation.
    // ast_replace_node must auto-align with the 4 spaces of the class!
    let new_code = r#"def authenticate(self, user, secret):
    if user == "admin" and secret == "supersecret":
        return True
    return False"#;

    let res = ToolRegistry::dispatch(
        ws,
        "test_call_2",
        "ast_replace_node",
        &json!({
            "path": "service.py",
            "symbol": "authenticate",
            "replacement_code": new_code,
        }),
        None,
        1,
    )
    .await;

    assert!(res.success, "Dispatch failed: {}", res.output);
    assert!(res
        .output
        .contains("Successfully replaced AST node `authenticate`"));

    let updated_content = fs::read_to_string(&py_path).unwrap();
    assert!(updated_content.contains("    def authenticate(self, user, secret):"));
    assert!(updated_content.contains("        if user == \"admin\" and secret == \"supersecret\":"));
    assert!(updated_content.contains("            return True"));
    assert!(updated_content.contains("        return False"));
}

#[tokio::test]
async fn test_ast_replace_node_dispatch_typescript_function() {
    let dir = tempdir().expect("create temp dir");
    let ws = dir.path();

    let ts_path = ws.join("utils.ts");
    fs::write(
        &ts_path,
        r#"
export function formatName(first: string, last: string): string {
    return first + " " + last;
}
"#,
    )
    .unwrap();

    let new_code = r#"export function formatName(first: string, last: string): string {
    return `${last.toUpperCase()}, ${first}`;
}"#;

    let res = ToolRegistry::dispatch(
        ws,
        "test_call_3",
        "ast_replace_node",
        &json!({
            "path": "utils.ts",
            "symbol": "formatName",
            "replacement_code": new_code,
        }),
        None,
        1,
    )
    .await;

    assert!(res.success, "Dispatch failed: {}", res.output);
    assert!(res
        .output
        .contains("Successfully replaced AST node `formatName`"));

    let updated_content = fs::read_to_string(&ts_path).unwrap();
    assert!(updated_content.contains("${last.toUpperCase()}, ${first}"));
}

#[tokio::test]
async fn test_ast_replace_node_syntax_error_prevention() {
    let dir = tempdir().expect("create temp dir");
    let ws = dir.path();

    let rs_path = ws.join("handler.rs");
    let initial_content = "pub fn handle_event() -> bool {\n    true\n}\n";
    fs::write(&rs_path, initial_content).unwrap();

    // Malformed code with broken syntax
    let broken_code = "pub fn handle_event( { invalid syntax here ;;;";

    let res = ToolRegistry::dispatch(
        ws,
        "test_call_4",
        "ast_replace_node",
        &json!({
            "path": "handler.rs",
            "symbol": "handle_event",
            "replacement_code": broken_code,
        }),
        None,
        1,
    )
    .await;

    assert!(!res.success, "Expected syntax validation to fail");
    assert!(res.output.contains("syntax validation failed") || res.output.contains("syntax error"));

    // Verify file on disk is 100% pristine and unaltered
    let disk_content = fs::read_to_string(&rs_path).unwrap();
    assert_eq!(disk_content, initial_content);
}

#[tokio::test]
async fn test_ast_replace_node_checkpoint_integration() {
    let dir = tempdir().expect("create temp dir");
    let ws = dir.path();
    let backup_mgr = BackupManager::new(ws);

    let rs_path = ws.join("state.rs");
    fs::write(&rs_path, "pub fn get_state() -> u32 {\n    0\n}\n").unwrap();

    let new_code = "pub fn get_state() -> u32 {\n    42\n}";

    let res = ToolRegistry::dispatch(
        ws,
        "test_call_5",
        "ast_replace_node",
        &json!({
            "path": "state.rs",
            "symbol": "get_state",
            "replacement_code": new_code,
        }),
        Some(&backup_mgr),
        1,
    )
    .await;

    assert!(res.success);
    let checkpoints = backup_mgr.list_checkpoints();
    assert!(
        !checkpoints.is_empty(),
        "Expected safety backup checkpoint to be created"
    );
}

#[tokio::test]
async fn test_ast_replace_node_symbol_not_found_lists_candidates() {
    let dir = tempdir().expect("create temp dir");
    let ws = dir.path();

    let rs_path = ws.join("module.rs");
    fs::write(
        &rs_path,
        r#"
pub fn actual_worker() {}
pub struct WorkerPool;
"#,
    )
    .unwrap();

    let res = ToolRegistry::dispatch(
        ws,
        "test_call_6",
        "ast_replace_node",
        &json!({
            "path": "module.rs",
            "symbol": "missing_fn",
            "replacement_code": "pub fn missing_fn() {}",
        }),
        None,
        1,
    )
    .await;

    assert!(!res.success);
    assert!(res.output.contains("missing_fn"));
    assert!(res.output.contains("actual_worker")); // Suggests available symbols
}

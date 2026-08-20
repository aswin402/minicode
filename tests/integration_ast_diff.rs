use minicode::context::ast_diff::AstDiffEngine;
use minicode::tools::ToolRegistry;
use serde_json::json;
use std::fs;
use tempfile::tempdir;

#[tokio::test]
async fn test_ast_diff_rust_structural_changes_and_breaking_rules() {
    let old_code = r#"
pub struct UserSession {
    pub user_id: u64,
    pub token: String,
}

pub fn validate_session(session: &UserSession) -> bool {
    !session.token.is_empty()
}

fn internal_helper() {
    println!("internal");
}
"#;

    let new_code = r#"
pub struct UserSession {
    pub user_id: u64,
    pub token: String,
    pub expires_at: u64,
}

pub fn validate_session(session: &UserSession, require_expiry: bool) -> bool {
    !session.token.is_empty()
}

pub fn logout_user(user_id: u64) {
    println!("logged out {}", user_id);
}
"#;

    let report = AstDiffEngine::diff_sources("session.rs", "rs", old_code, new_code).unwrap();

    // 1. Added
    assert_eq!(report.added.len(), 1);
    assert_eq!(report.added[0].name, "logout_user");
    assert!(report.added[0].is_public);

    // 2. Removed
    assert_eq!(report.removed.len(), 1);
    assert_eq!(report.removed[0].name, "internal_helper");
    assert!(!report.removed[0].is_public);

    // 3. Modified
    assert_eq!(report.modified.len(), 2); // UserSession and validate_session
    let val_sess = report
        .modified
        .iter()
        .find(|m| m.name == "validate_session")
        .unwrap();
    assert!(val_sess.signature_changed);

    // 4. Breaking changes (validate_session signature changed for pub fn)
    assert_eq!(report.breaking_changes.len(), 1);
    assert_eq!(report.breaking_changes[0].symbol_name, "validate_session");
    assert_eq!(report.breaking_changes[0].severity, "HIGH");

    // 5. Markdown formatting check
    let md = report.format_markdown();
    assert!(md.contains("logout_user"));
    assert!(md.contains("validate_session"));
    assert!(md.contains("Summary:"));
}

#[tokio::test]
async fn test_ast_diff_tool_dispatch_lifecycle() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("calculator.rs");

    fs::write(
        &file_path,
        r#"
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}
"#,
    )
    .unwrap();

    let new_content = r#"
pub fn add(a: i32, b: i32, c: i32) -> i32 {
    a + b + c
}

pub fn multiply(a: i32, b: i32) -> i32 {
    a * b
}
"#;

    let args = json!({
        "file_path": "calculator.rs",
        "new_content": new_content
    });

    let res =
        ToolRegistry::dispatch(dir.path(), "call_ast_diff_1", "ast_diff", &args, None, 1).await;
    assert!(res.success);
    assert!(res.output.contains("Semantic AST Diff"));
    assert!(res.output.contains("multiply"));
    assert!(res.output.contains("add"));
}

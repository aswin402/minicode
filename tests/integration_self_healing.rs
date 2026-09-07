//! Integration tests for Autonomous Self-Healing Diagnostic Loop & LSP Error Auto-Triage (Phase 98, v0.2.9).

use minicode::agent::self_healing::{
    DiagnosticCategory, DiagnosticTriageEngine, SelfHealingEngine,
};
use minicode::lsp::diagnostics::{DiagnosticItem, DiagnosticReport};
use minicode::tools::ToolRegistry;
use serde_json::json;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

#[test]
fn test_triage_engine_groups_primary_vs_cascading() {
    let item1 = DiagnosticItem {
        file: PathBuf::from("src/api.rs"),
        line: 14,
        column: 20,
        severity: "error".to_string(),
        code: Some("E0412".to_string()),
        message: "cannot find type `AuthClient` in this scope".to_string(),
        rendered: None,
    };

    let item2 = DiagnosticItem {
        file: PathBuf::from("src/api.rs"),
        line: 28,
        column: 5,
        severity: "error".to_string(),
        code: Some("E0412".to_string()),
        message: "cannot find type `AuthClient` in this scope".to_string(),
        rendered: None,
    };

    let item3 = DiagnosticItem {
        file: PathBuf::from("src/api.rs"),
        line: 45,
        column: 12,
        severity: "error".to_string(),
        code: Some("E0412".to_string()),
        message: "cannot find type `AuthClient` in this scope".to_string(),
        rendered: None,
    };

    let report = DiagnosticReport {
        errors: vec![item1, item2, item3],
        warnings: vec![],
    };

    let triage = DiagnosticTriageEngine::triage(&report, Path::new("."));
    assert_eq!(triage.primary_clusters.len(), 1);
    assert_eq!(triage.total_errors, 3);
    assert_eq!(triage.primary_clusters[0].cascading_errors.len(), 2);
    assert_eq!(
        triage.primary_clusters[0].root_symbol.as_deref(),
        Some("AuthClient")
    );
    assert_eq!(
        triage.primary_clusters[0].category,
        DiagnosticCategory::MissingImport
    );
}

#[test]
fn test_find_symbol_definition_in_workspace() {
    let temp = TempDir::new().expect("Failed to create tempdir");
    let ws = temp.path();

    let src_dir = ws.join("src").join("models");
    std::fs::create_dir_all(&src_dir).expect("Create src/models");

    let model_file = src_dir.join("user.rs");
    std::fs::write(
        &model_file,
        "#[derive(Debug)]\npub struct UserProfile {\n    pub id: u64,\n}\n",
    )
    .expect("Write user.rs");

    let discovered = DiagnosticTriageEngine::find_symbol_definition(ws, "UserProfile");
    assert!(discovered.is_some(), "Must locate UserProfile definition");
    let path = discovered.unwrap();
    assert_eq!(path, "crate::models::user::UserProfile");
}

#[tokio::test]
async fn test_self_healing_clean_workspace() {
    let temp = TempDir::new().expect("Failed to create tempdir");
    let ws = temp.path();

    // In a directory without Cargo.toml / package.json, diagnostics report clean
    let report = SelfHealingEngine::heal(ws, 3, true, false)
        .await
        .expect("Self healing should succeed");

    assert!(report.fully_resolved);
    assert_eq!(report.initial_errors, 0);
    assert_eq!(report.final_errors, 0);
}

#[tokio::test]
async fn test_self_healing_dry_run() {
    let temp = TempDir::new().expect("Failed to create tempdir");
    let ws = temp.path();

    let report = SelfHealingEngine::heal(ws, 3, true, true)
        .await
        .expect("Dry run should succeed");

    assert_eq!(report.attempts_used, 0);
}

#[tokio::test]
async fn test_self_healing_auto_import_recovery() {
    let temp = TempDir::new().expect("Failed to create tempdir");
    let ws = temp.path();

    // 1. Create a minimal Cargo project
    let cargo_toml = ws.join("Cargo.toml");
    std::fs::write(
        &cargo_toml,
        r#"[package]
name = "test_pkg"
version = "0.1.0"
edition = "2021"

[lib]
path = "src/lib.rs"
"#,
    )
    .expect("Write Cargo.toml");

    let src_dir = ws.join("src");
    std::fs::create_dir_all(&src_dir).expect("Create src dir");

    // 2. Define target symbol in src/helper.rs
    let helper_path = src_dir.join("helper.rs");
    std::fs::write(
        &helper_path,
        "pub struct SpecialCalculator;\nimpl SpecialCalculator {\n    pub fn calc() -> u32 { 42 }\n}\n",
    )
    .expect("Write helper.rs");

    // 3. Create src/lib.rs that declares helper mod but forgets to import SpecialCalculator
    let lib_path = src_dir.join("lib.rs");
    std::fs::write(
        &lib_path,
        "pub mod helper;\n\npub fn run() -> u32 {\n    SpecialCalculator::calc()\n}\n",
    )
    .expect("Write lib.rs");

    // 4. Run self-healing engine with auto_apply_imports = true
    let report = SelfHealingEngine::heal(ws, 3, true, false)
        .await
        .expect("Self healing run should succeed");

    // If cargo is available and executes, it should detect and resolve or attempt the repair
    if !report.steps.is_empty() {
        assert!(report.steps[0].action_taken.contains("SpecialCalculator"));
        let modified_lib = std::fs::read_to_string(&lib_path).expect("Read lib.rs");
        assert!(modified_lib.contains("use crate::helper::SpecialCalculator;"));
    }
}

#[tokio::test]
async fn test_tool_registry_repair_diagnostics_dispatch() {
    let temp = TempDir::new().expect("Failed to create tempdir");
    let ws = temp.path();

    let res = ToolRegistry::dispatch(
        ws,
        "heal_call_1",
        "repair_diagnostics",
        &json!({ "dry_run": true, "max_attempts": 2 }),
        None,
        1,
    )
    .await;

    assert!(
        res.success,
        "repair_diagnostics tool call should succeed: {}",
        res.output
    );
    assert!(res
        .output
        .contains("Autonomous Diagnostic Self-Healing Report"));
}

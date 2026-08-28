/// Integration tests for Phase 46: Full Native onpkg Integration in minicode
///
/// Tests embedded stack templates, scaffolder, sync engine, skills manager, and diagnostics.
use minicode::tools::onpkg::doctor::OnpkgDoctor;
use minicode::tools::onpkg::scaffolder::OnpkgScaffolder;
use minicode::tools::onpkg::sync::OnpkgSyncEngine;
use minicode::tools::registry::onpkg_tools;
use minicode::tools::ToolRegistry;
use tempfile::tempdir;

#[test]
fn test_onpkg_schemas_registered_in_registry() {
    let schemas = onpkg_tools::get_schemas();
    let names: Vec<String> = schemas.into_iter().map(|s| s.name).collect();

    assert!(names.contains(&"onpkg_stack_list".to_string()));
    assert!(names.contains(&"onpkg_stack_show".to_string()));
    assert!(names.contains(&"onpkg_stack_add".to_string()));
    assert!(names.contains(&"onpkg_skill_list".to_string()));
    assert!(names.contains(&"onpkg_skill_install".to_string()));
    assert!(names.contains(&"onpkg_sync".to_string()));
    assert!(names.contains(&"onpkg_doctor".to_string()));

    // Global ToolRegistry check
    let global_schemas = ToolRegistry::get_tool_schemas();
    let global_names: Vec<String> = global_schemas.into_iter().map(|s| s.name).collect();
    assert!(global_names.contains(&"onpkg_stack_add".to_string()));
}

#[test]
fn test_native_builtin_stacks_catalogue() {
    let stacks = OnpkgScaffolder::get_all_stacks();
    assert!(
        stacks.len() >= 10,
        "Expected at least 10 built-in stacks, found {}",
        stacks.len()
    );

    let names: Vec<String> = stacks.into_iter().map(|s| s.name).collect();
    assert!(names.contains(&"react-vite".to_string()));
    assert!(names.contains(&"react-vite-gsap".to_string()));
    assert!(names.contains(&"next-template".to_string()));
    assert!(names.contains(&"fastapi".to_string()));
    assert!(names.contains(&"hono-full".to_string()));
    assert!(names.contains(&"mern".to_string()));
    assert!(names.contains(&"pern".to_string()));
    assert!(names.contains(&"flutter-riverpod-my_app".to_string()));
}

#[tokio::test]
async fn test_native_scaffolding_end_to_end() {
    let temp_dir = tempdir().unwrap();
    let workspace = temp_dir.path();

    // Scaffold FastAPI stack natively without network install
    let res = OnpkgScaffolder::scaffold(workspace, "fastapi", Some("my_api"), true)
        .await
        .unwrap();

    assert!(res.contains("✔ Successfully scaffolded stack `fastapi`"));

    let api_dir = workspace.join("my_api");
    assert!(api_dir.join("app/main.py").exists());
    assert!(api_dir.join("alembic.ini").exists());
    assert!(api_dir.join("justfile").exists());
    assert!(api_dir.join("onpkg.json").exists());
    assert!(api_dir.join("AGENTS.md").exists());
    assert!(api_dir.join("onpkg_docs/prd.md").exists());
    assert!(api_dir.join("onpkg_docs/todo.md").exists());
}

#[test]
fn test_native_runtime_detection_and_sync() {
    let temp_dir = tempdir().unwrap();
    let workspace = temp_dir.path();

    // Simulate Rust project
    std::fs::write(workspace.join("Cargo.toml"), "[package]\nname=\"test\"\n").unwrap();
    let (runtime, pm) = OnpkgSyncEngine::detect_runtime(workspace);
    assert_eq!(runtime, "rust");
    assert_eq!(pm, "cargo");

    // Perform sync
    let sync_res = OnpkgSyncEngine::sync(workspace).unwrap();
    assert!(sync_res.contains("Synchronized `"));
    assert!(workspace.join("onpkg.json").exists());
    assert!(workspace.join("AGENTS.md").exists());
}

#[test]
fn test_native_doctor_diagnostics() {
    let report = OnpkgDoctor::diagnose();
    assert!(report.contains("Multi-Runtime Diagnostics"));
    assert!(report.contains("Rust / Cargo"));
    assert!(report.contains("Git"));
}

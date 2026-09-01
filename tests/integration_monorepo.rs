use minicode::context::monorepo::MonorepoOrchestrator;
use serde_json::json;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_cargo_workspace_monorepo_detection() {
    let dir = tempdir().expect("tempdir");
    let core_dir = dir.path().join("crates").join("core");
    let cli_dir = dir.path().join("crates").join("cli");
    fs::create_dir_all(&core_dir).expect("create core");
    fs::create_dir_all(&cli_dir).expect("create cli");

    // Root Cargo.toml
    fs::write(
        dir.path().join("Cargo.toml"),
        r#"
[workspace]
members = [
    "crates/core",
    "crates/cli",
]
"#,
    )
    .expect("write root Cargo.toml");

    // Core Cargo.toml
    fs::write(
        core_dir.join("Cargo.toml"),
        r#"
[package]
name = "mycore"
version = "0.1.0"
"#,
    )
    .expect("write core Cargo.toml");

    // CLI Cargo.toml depending on Core
    fs::write(
        cli_dir.join("Cargo.toml"),
        r#"
[package]
name = "mycli"
version = "0.1.0"

[dependencies]
mycore = { path = "../core" }
"#,
    )
    .expect("write cli Cargo.toml");

    let report = MonorepoOrchestrator::analyze_workspace(dir.path(), false, None)
        .expect("analyze cargo workspace");

    assert_eq!(report.workspace_type, "Cargo Multi-Crate Workspace");
    assert_eq!(report.packages.len(), 2);

    let cli_pkg = report.packages.iter().find(|p| p.name == "mycli");
    assert!(cli_pkg.is_some());
    assert!(cli_pkg
        .unwrap()
        .internal_dependencies
        .contains(&"mycore".to_string()));

    assert_eq!(report.topological_order, vec!["mycore", "mycli"]);
    assert!(report.cross_package_cycles.is_empty());
}

#[test]
fn test_npm_workspace_monorepo_detection() {
    let dir = tempdir().expect("tempdir");
    let web_dir = dir.path().join("packages").join("web");
    let api_dir = dir.path().join("packages").join("api");
    fs::create_dir_all(&web_dir).expect("create web");
    fs::create_dir_all(&api_dir).expect("create api");

    // Root package.json
    fs::write(
        dir.path().join("package.json"),
        r#"
{
  "name": "my-monorepo",
  "workspaces": [
    "packages/*"
  ]
}
"#,
    )
    .expect("write root package.json");

    // Web package.json
    fs::write(
        web_dir.join("package.json"),
        r#"
{
  "name": "@mono/web",
  "version": "1.0.0",
  "dependencies": {
    "@mono/api": "1.0.0"
  }
}
"#,
    )
    .expect("write web package.json");

    // Api package.json
    fs::write(
        api_dir.join("package.json"),
        r#"
{
  "name": "@mono/api",
  "version": "1.0.0"
}
"#,
    )
    .expect("write api package.json");

    let report = MonorepoOrchestrator::analyze_workspace(dir.path(), false, None)
        .expect("analyze npm workspace");

    assert_eq!(report.workspace_type, "npm/pnpm Monorepo");
    assert_eq!(report.packages.len(), 2);

    let web_pkg = report.packages.iter().find(|p| p.name == "@mono/web");
    assert!(web_pkg.is_some());
    assert!(web_pkg
        .unwrap()
        .internal_dependencies
        .contains(&"@mono/api".to_string()));
}

#[tokio::test]
async fn test_workspace_monorepo_map_tool_dispatch() {
    let dir = tempdir().expect("tempdir");

    let args = json!({
        "include_external": false
    });

    let res = minicode::tools::registry::context_tools::dispatch(
        "workspace_monorepo_map",
        &args,
        dir.path(),
    )
    .await;

    assert!(res.is_some());
    let output = res.unwrap().expect("tool execution success");
    assert!(output.contains("Workspace Monorepo & Multi-Package Architecture Report"));
}

/// Integration tests for Phase 46 & 47: Native MiniKit Engine, Interactive Stack Wizard, and Autonomous Goal/Plan Commands
use minicode::agent::prompt::DEFAULT_SYSTEM_PROMPT;
use minicode::tools::minikit::doctor::MiniKitDoctor;
use minicode::tools::minikit::scaffolder::MiniKitScaffolder;
use minicode::tools::minikit::sync::MiniKitSyncEngine;
use minicode::tools::registry::minikit_tools;
use minicode::tools::ToolRegistry;
use minicode::ui::input::PALETTE_COMMANDS;
use minicode::ui::modal::ModalState;
use serde_json::json;
use tempfile::tempdir;

#[test]
fn test_minikit_schemas_registered_in_registry() {
    let schemas = minikit_tools::get_schemas();
    let names: Vec<String> = schemas.into_iter().map(|s| s.name).collect();

    assert!(
        names.contains(&"kit_stack_list".to_string())
            || names.contains(&"onpkg_stack_list".to_string())
    );
    assert!(
        names.contains(&"kit_stack_show".to_string())
            || names.contains(&"onpkg_stack_show".to_string())
    );
    assert!(
        names.contains(&"kit_stack_add".to_string())
            || names.contains(&"onpkg_stack_add".to_string())
    );
    assert!(
        names.contains(&"kit_skill_list".to_string())
            || names.contains(&"onpkg_skill_list".to_string())
    );
    assert!(
        names.contains(&"kit_skill_install".to_string())
            || names.contains(&"onpkg_skill_install".to_string())
    );
    assert!(names.contains(&"kit_sync".to_string()) || names.contains(&"onpkg_sync".to_string()));
    assert!(
        names.contains(&"kit_doctor".to_string()) || names.contains(&"onpkg_doctor".to_string())
    );
    assert!(
        names.contains(&"kit_stack_snapshot".to_string())
            || names.contains(&"minikit_stack_snapshot".to_string())
            || names.contains(&"onpkg_stack_snapshot".to_string())
    );
    assert!(
        names.contains(&"kit_block_list".to_string())
            || names.contains(&"onpkg_block_list".to_string())
    );
    assert!(
        names.contains(&"kit_block_add".to_string())
            || names.contains(&"onpkg_block_add".to_string())
    );

    // Global ToolRegistry check
    let global_schemas = ToolRegistry::get_tool_schemas();
    let global_names: Vec<String> = global_schemas.into_iter().map(|s| s.name).collect();
    assert!(
        global_names.contains(&"kit_stack_add".to_string())
            || global_names.contains(&"onpkg_stack_add".to_string())
    );
    assert!(global_names.contains(&"kit_stack_snapshot".to_string()));
    assert!(global_names.contains(&"kit_block_list".to_string()));
    assert!(global_names.contains(&"kit_block_add".to_string()));
}

#[test]
fn test_native_builtin_stacks_catalogue() {
    let stacks = MiniKitScaffolder::get_all_stacks();
    assert!(
        stacks.len() >= 13,
        "Expected at least 13 built-in stacks, found {}",
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
    // Check 3 newly ported stacks from onpkg / minikit
    assert!(names.contains(&"rust-cli".to_string()));
    assert!(names.contains(&"static-website".to_string()));
    assert!(names.contains(&"express-api".to_string()));
}

#[tokio::test]
async fn test_native_scaffolding_end_to_end() {
    let temp_dir = tempdir().unwrap();
    let workspace = temp_dir.path();

    // Scaffold FastAPI stack natively without network install
    let res = MiniKitScaffolder::scaffold(workspace, "fastapi", Some("my_api"), true)
        .await
        .unwrap();

    assert!(res.contains("✔ Successfully scaffolded stack `fastapi`"));

    let api_dir = workspace.join("my_api");
    assert!(api_dir.join("app/main.py").exists());
    assert!(api_dir.join("alembic.ini").exists());
    assert!(api_dir.join("justfile").exists());
    assert!(api_dir.join("minikit.json").exists());
    assert!(api_dir.join("onpkg.json").exists());
    assert!(api_dir.join("AGENTS.md").exists());
    assert!(api_dir.join("minikit_docs/prd.md").exists());
    assert!(api_dir.join("minikit_docs/todo.md").exists());
}

#[test]
fn test_native_runtime_detection_and_sync() {
    let temp_dir = tempdir().unwrap();
    let workspace = temp_dir.path();

    // Simulate Rust project
    std::fs::write(workspace.join("Cargo.toml"), "[package]\nname=\"test\"\n").unwrap();
    let (runtime, pm) = MiniKitSyncEngine::detect_runtime(workspace);
    assert_eq!(runtime, "rust");
    assert_eq!(pm, "cargo");

    // Perform sync
    let sync_res = MiniKitSyncEngine::sync(workspace).unwrap();
    assert!(sync_res.contains("Synchronized `"));
    assert!(workspace.join("minikit.json").exists());
    assert!(workspace.join("onpkg.json").exists());
    assert!(workspace.join("AGENTS.md").exists());
}

#[test]
fn test_native_doctor_diagnostics() {
    let report = MiniKitDoctor::diagnose();
    assert!(report.contains("Multi-Runtime Diagnostics"));
    assert!(report.contains("Rust / Cargo"));
    assert!(report.contains("Git"));
}

#[test]
fn test_slash_commands_contain_stack_plan_goal() {
    let cmd_names: Vec<&str> = PALETTE_COMMANDS.iter().map(|c| c.slash_name).collect();
    assert!(cmd_names.contains(&"/stack"));
    assert!(cmd_names.contains(&"/plan"));
    assert!(cmd_names.contains(&"/goal"));
}

#[test]
fn test_stack_select_modal_state_and_filter() {
    let mut modal = ModalState::new_stack_select();
    match modal {
        ModalState::StackSelect {
            ref stacks,
            ref filtered_indices,
            ref mut filter,
            ..
        } => {
            assert!(!stacks.is_empty());
            assert_eq!(filtered_indices.len(), stacks.len());

            // Test filtering
            *filter = "fastapi".to_string();
        }
        _ => panic!("Expected ModalState::StackSelect"),
    }
    modal.update_filter();

    match modal {
        ModalState::StackSelect {
            ref filtered_indices,
            ..
        } => {
            assert_eq!(filtered_indices.len(), 1);
        }
        _ => panic!("Expected ModalState::StackSelect"),
    }
}

#[test]
fn test_system_prompt_autonomous_protocols() {
    assert!(
        DEFAULT_SYSTEM_PROMPT.contains("kit_stack_add")
            || DEFAULT_SYSTEM_PROMPT.contains("onpkg_stack_add")
    );
    assert!(DEFAULT_SYSTEM_PROMPT.contains("todo.md"));
    assert!(
        DEFAULT_SYSTEM_PROMPT.contains("locate_symbol")
            || DEFAULT_SYSTEM_PROMPT.contains("grep_search")
    );
}

#[test]
fn test_custom_stack_crud_lifecycle() {
    let temp_dir = tempdir().unwrap();
    let workspace = temp_dir.path();

    // 1. Create a custom stack
    let path =
        MiniKitScaffolder::create_custom_stack(workspace, "my-custom-stack", "bun", false).unwrap();
    assert!(path.exists());
    assert!(path.to_string_lossy().contains("my-custom-stack.json"));

    // 2. Verify JSON content is valid
    let content = std::fs::read_to_string(&path).unwrap();
    let json: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert_eq!(json["name"], "my-custom-stack");
    assert_eq!(json["runtime"], "bun");

    // 3. Delete the custom stack
    let del_msg =
        MiniKitScaffolder::delete_custom_stack(workspace, "my-custom-stack", false).unwrap();
    assert!(del_msg.contains("Successfully removed"));
    assert!(!path.exists());

    // 4. Deleting non-existent stack returns error
    let err = MiniKitScaffolder::delete_custom_stack(workspace, "non-existent-stack", false);
    assert!(err.is_err());
}

#[tokio::test]
async fn test_skill_crud_lifecycle() {
    let temp_dir = tempdir().unwrap();
    let workspace = temp_dir.path();

    // 1. Install a skill (e.g. react)
    let install_res =
        minicode::tools::minikit::skills::MiniKitSkillsManager::install_skill(workspace, "react")
            .unwrap();
    assert!(install_res.contains("Successfully installed built-in skill `react`"));
    assert!(workspace.join(".minicode/skills/react/SKILL.md").exists());

    // 2. Read the skill
    let show_res =
        minicode::tools::minikit::skills::MiniKitSkillsManager::show_skill(workspace, "react")
            .unwrap();
    assert!(show_res.contains("React"));

    // 3. Remove the skill
    let remove_res =
        minicode::tools::minikit::skills::MiniKitSkillsManager::remove_skill(workspace, "react")
            .unwrap();
    assert!(remove_res.contains("Successfully removed skill `react`"));
    assert!(!workspace.join(".minicode/skills/react").exists());
}

#[test]
fn test_package_removal_crud_lifecycle() {
    let temp_dir = tempdir().unwrap();
    let workspace = temp_dir.path();

    // 1. Test Node / package.json removal
    let pkg_json = r#"{
  "name": "test-app",
  "dependencies": {
    "express": "^4.18.2",
    "cors": "^2.8.5"
  },
  "devDependencies": {
    "typescript": "^5.0.0"
  }
}"#;
    std::fs::write(workspace.join("package.json"), pkg_json).unwrap();

    let registry = minicode::tools::minikit::pkg::PkgRegistry::new();
    let remove_res = registry
        .remove_from_project(workspace, "express", Some("bun"))
        .unwrap();
    assert!(remove_res.contains("Successfully removed package `express`"));

    let updated_json = std::fs::read_to_string(workspace.join("package.json")).unwrap();
    assert!(!updated_json.contains("express"));
    assert!(updated_json.contains("cors"));

    // 2. Test Rust / Cargo.toml removal
    let cargo_toml = r#"[package]
name = "test-crate"
version = "0.1.0"

[dependencies]
serde = "1.0"
tokio = { version = "1.0", features = ["full"] }
"#;
    std::fs::write(workspace.join("Cargo.toml"), cargo_toml).unwrap();

    let remove_cargo = registry
        .remove_from_project(workspace, "serde", Some("cargo"))
        .unwrap();
    assert!(remove_cargo.contains("Successfully removed package `serde`"));

    let updated_cargo = std::fs::read_to_string(workspace.join("Cargo.toml")).unwrap();
    assert!(!updated_cargo.contains("serde ="));
    assert!(updated_cargo.contains("tokio ="));
}

#[test]
fn test_all_new_kit_schemas_present() {
    let schemas = minicode::tools::registry::minikit_tools::get_schemas();
    let names: Vec<String> = schemas.into_iter().map(|s| s.name).collect();

    assert!(names.contains(&"kit_stack_new".to_string()));
    assert!(names.contains(&"kit_stack_remove".to_string()));
    assert!(names.contains(&"kit_skill_remove".to_string()));
    assert!(names.contains(&"kit_remove".to_string()));
}

#[tokio::test]
async fn test_scaffold_new_stacks_and_drift_healing() {
    let temp_dir = tempdir().unwrap();
    let workspace = temp_dir.path();

    // 1. Scaffold rust-cli stack
    let rust_res = MiniKitScaffolder::scaffold(workspace, "rust-cli", Some("my_cli"), true)
        .await
        .unwrap();
    assert!(rust_res.contains("Successfully scaffolded stack `rust-cli`"));
    let cli_dir = workspace.join("my_cli");
    assert!(cli_dir.join("Cargo.toml").exists());
    assert!(cli_dir.join("src/main.rs").exists());
    assert!(cli_dir.join("AGENTS.md").exists());
    assert!(cli_dir.join("minikit.json").exists());
    assert!(cli_dir.join("onpkg.json").exists());

    // 2. Check architecture drift: should be in sync (0 missing)
    let drift_clean = minicode::tools::minikit::diff::diff_stack(&cli_dir, None, false).unwrap();
    assert!(drift_clean.missing_files.is_empty());

    // 3. Simulate drift: delete src/main.rs
    std::fs::remove_file(cli_dir.join("src/main.rs")).unwrap();
    let drift_detected = minicode::tools::minikit::diff::diff_stack(&cli_dir, None, false).unwrap();
    assert_eq!(drift_detected.missing_files.len(), 1);
    assert_eq!(drift_detected.missing_files[0], "src/main.rs");

    // 4. Autonomous self-healing: apply repair
    let drift_healed = minicode::tools::minikit::diff::diff_stack(&cli_dir, None, true).unwrap();
    assert_eq!(drift_healed.restored_files.len(), 1);
    assert_eq!(drift_healed.restored_files[0], "src/main.rs");
    assert!(cli_dir.join("src/main.rs").exists());

    // 5. Scaffold static-website
    let static_res =
        MiniKitScaffolder::scaffold(workspace, "static-website", Some("web_app"), true)
            .await
            .unwrap();
    assert!(static_res.contains("Successfully scaffolded stack `static-website`"));
    let web_dir = workspace.join("web_app");
    assert!(web_dir.join("index.html").exists());
    assert!(web_dir.join("style.css").exists());
    assert!(web_dir.join("app.js").exists());

    // 6. Scaffold express-api
    let express_res =
        MiniKitScaffolder::scaffold(workspace, "express-api", Some("express_app"), true)
            .await
            .unwrap();
    assert!(express_res.contains("Successfully scaffolded stack `express-api`"));
    let exp_dir = workspace.join("express_app");
    assert!(exp_dir.join("package.json").exists());
    assert!(exp_dir.join("src/index.ts").exists());
}

#[test]
fn test_security_path_traversal_rejection() {
    let temp_dir = tempdir().unwrap();
    let workspace = temp_dir.path();

    // 1. Stack creation traversal rejection
    let bad_create =
        MiniKitScaffolder::create_custom_stack(workspace, "../../bad_stack", "bun", false);
    assert!(bad_create.is_err());

    // 2. Stack deletion traversal rejection
    let bad_delete =
        MiniKitScaffolder::delete_custom_stack(workspace, "../../../etc/passwd", false);
    assert!(bad_delete.is_err());

    // 3. Skill removal traversal rejection
    let bad_skill = minicode::tools::minikit::skills::MiniKitSkillsManager::remove_skill(
        workspace,
        "../../bad_skill",
    );
    assert!(bad_skill.is_err());
}

#[test]
fn test_python_requirements_collision_prevention() {
    let temp_dir = tempdir().unwrap();
    let workspace = temp_dir.path();

    let initial_reqs =
        "requests-oauthlib==1.3.0\nrequests==2.31.0\nurllib3>=1.26.0\n# comment with requests\n";
    std::fs::write(workspace.join("requirements.txt"), initial_reqs).unwrap();

    let registry = minicode::tools::minikit::pkg::PkgRegistry::new();

    // Remove requests - should NOT remove requests-oauthlib
    let res = registry
        .remove_from_project(workspace, "requests", Some("python"))
        .unwrap();
    assert!(res.contains("Successfully removed package `requests`"));

    let updated = std::fs::read_to_string(workspace.join("requirements.txt")).unwrap();
    assert!(
        updated.contains("requests-oauthlib==1.3.0"),
        "requests-oauthlib must be preserved!"
    );
    assert!(updated.contains("urllib3>=1.26.0"));
    assert!(
        !updated.contains("requests==2.31.0"),
        "requests must be removed!"
    );
}

#[test]
fn test_stack_alias_resolution() {
    assert_eq!(
        MiniKitScaffolder::find_stack("rust").map(|s| s.name),
        Some("rust-cli".to_string())
    );
    assert_eq!(
        MiniKitScaffolder::find_stack("express").map(|s| s.name),
        Some("express-api".to_string())
    );
    assert_eq!(
        MiniKitScaffolder::find_stack("static").map(|s| s.name),
        Some("static-website".to_string())
    );
    assert_eq!(
        MiniKitScaffolder::find_stack("flutter").map(|s| s.name),
        Some("flutter-riverpod-my_app".to_string())
    );
    assert_eq!(
        MiniKitScaffolder::find_stack("react").map(|s| s.name),
        Some("react-vite".to_string())
    );
    assert_eq!(
        MiniKitScaffolder::find_stack("hono").map(|s| s.name),
        Some("hono-full".to_string())
    );
    assert_eq!(
        MiniKitScaffolder::find_stack("python").map(|s| s.name),
        Some("fastapi".to_string())
    );
    assert_eq!(
        MiniKitScaffolder::find_stack("fastapi").map(|s| s.name),
        Some("fastapi".to_string())
    );
    assert_eq!(
        MiniKitScaffolder::find_stack("mern").map(|s| s.name),
        Some("mern".to_string())
    );
    assert_eq!(
        MiniKitScaffolder::find_stack("pern").map(|s| s.name),
        Some("pern".to_string())
    );
}

#[test]
fn test_go_runtime_detection_and_removal() {
    let temp_dir = tempdir().unwrap();
    let workspace = temp_dir.path();

    let go_mod_content = "module example.com/my-go-app\n\ngo 1.22\n\nrequire (\n\tgithub.com/gin-gonic/gin v1.9.1\n\tgithub.com/stretchr/testify v1.8.4\n)\n";
    std::fs::write(workspace.join("go.mod"), go_mod_content).unwrap();

    let (rt, pm) = minicode::tools::minikit::sync::MiniKitSyncEngine::detect_runtime(workspace);
    assert_eq!(rt, "go");
    assert_eq!(pm, "go");

    let registry = minicode::tools::minikit::pkg::PkgRegistry::new();
    assert_eq!(
        minicode::tools::minikit::pkg::PkgRegistry::detect_runtime(workspace),
        "go"
    );

    let res = registry
        .remove_from_project(workspace, "github.com/gin-gonic/gin", Some("go"))
        .unwrap();
    assert!(res.contains("Successfully removed package"));

    let updated = std::fs::read_to_string(workspace.join("go.mod")).unwrap();
    assert!(!updated.contains("github.com/gin-gonic/gin"));
    assert!(updated.contains("github.com/stretchr/testify"));
}

#[test]
fn test_workspace_custom_stack_scoping() {
    let temp_dir = tempdir().unwrap();
    let workspace = temp_dir.path();

    // Create custom stack in workspace
    let path =
        MiniKitScaffolder::create_custom_stack(workspace, "scoped-custom", "bun", false).unwrap();
    assert!(path.exists());

    // find_stack_in_workspace should find it
    let found = MiniKitScaffolder::find_stack_in_workspace(workspace, "scoped-custom");
    assert!(found.is_some());
    assert_eq!(found.unwrap().name, "scoped-custom");

    // All stacks for workspace should include it
    let all = MiniKitScaffolder::get_all_stacks_for(Some(workspace));
    assert!(all.iter().any(|s| s.name == "scoped-custom"));
}

#[tokio::test]
async fn test_snapshot_stack_tool_dispatch_and_scaffold_cycle() {
    let temp_dir = tempdir().unwrap();
    let workspace = temp_dir.path();

    // 1. Setup a realistic project in the workspace
    let package_json = json!({
        "name": "sample-microservice",
        "dependencies": {
            "hono": "^4.0.0",
            "zod": "^3.22.0"
        },
        "devDependencies": {
            "typescript": "^5.0.0"
        }
    });
    std::fs::write(
        workspace.join("package.json"),
        serde_json::to_string_pretty(&package_json).unwrap(),
    )
    .unwrap();

    let src_dir = workspace.join("src");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::write(
        src_dir.join("index.ts"),
        "import { Hono } from 'hono';\nconst app = new Hono();\nexport default app;\n",
    )
    .unwrap();
    std::fs::write(
        src_dir.join("routes.ts"),
        "export const routes = ['/health', '/api'];\n",
    )
    .unwrap();

    // 2. Dispatch snapshot tool via ToolRegistry
    let args = json!({
        "name": "sample-microservice-stack",
        "description": "Auto-snapshotted microservice template",
        "global": false
    });
    let res = ToolRegistry::dispatch(
        workspace,
        "call_snap_1",
        "kit_stack_snapshot",
        &args,
        None,
        1,
    )
    .await;
    assert!(res.success, "Tool dispatch failed: {}", res.output);
    assert!(res.output.contains("sample-microservice-stack"));
    assert!(res
        .output
        .contains("Successfully snapshotted workspace into custom stack"));

    // 3. Verify the stack file was created in .minicode/stacks/
    let stack_path = workspace
        .join(".minicode")
        .join("stacks")
        .join("sample-microservice-stack.json");
    assert!(stack_path.exists());

    // 4. Verify discovery via MiniKitScaffolder
    let found = MiniKitScaffolder::find_stack_in_workspace(workspace, "sample-microservice-stack");
    assert!(found.is_some());
    let stack = found.unwrap();
    assert_eq!(stack.name, "sample-microservice-stack");
    assert_eq!(stack.runtime, "node");
    assert!(stack.packages.contains(&"hono".to_string()));
    assert!(stack.packages.contains(&"zod".to_string()));
    assert!(stack.dev_packages.contains(&"typescript".to_string()));
    assert!(stack.files.iter().any(|f| f.path == "src/index.ts"));
    assert!(stack.files.iter().any(|f| f.path == "src/routes.ts"));

    // 5. Test alias dispatch (minikit_stack_snapshot or onpkg_stack_snapshot)
    let alias_args = json!({
        "name": "alias-stack",
        "global": false
    });
    let alias_res = ToolRegistry::dispatch(
        workspace,
        "call_snap_2",
        "onpkg_stack_snapshot",
        &alias_args,
        None,
        1,
    )
    .await;
    assert!(
        alias_res.success,
        "Alias dispatch failed: {}",
        alias_res.output
    );
    assert!(alias_res.output.contains("alias-stack"));

    // 6. Test scaffolding the snapshotted stack into a new target directory
    let target_dir = temp_dir.path().join("spawned-service");
    let scaffold_res = MiniKitScaffolder::scaffold(
        workspace,
        "sample-microservice-stack",
        Some(target_dir.to_str().unwrap()),
        true,
    )
    .await
    .unwrap();
    assert!(scaffold_res.contains("sample-microservice-stack"));
    assert!(target_dir.join("src/index.ts").exists());
    assert!(target_dir.join("src/routes.ts").exists());
    assert!(target_dir.join("package.json").exists());
}

#[tokio::test]
async fn test_architecture_blocks_tool_dispatch_and_conflict_handling() {
    let temp_dir = tempdir().unwrap();
    let workspace = temp_dir.path();

    // 1. List blocks via tool registry dispatch
    let list_res = ToolRegistry::dispatch(
        workspace,
        "call_block_1",
        "kit_block_list",
        &json!({}),
        None,
        1,
    )
    .await;
    assert!(list_res.success);
    assert!(list_res.output.contains("Available Architecture Blocks"));
    assert!(list_res.output.contains("docker"));
    assert!(list_res.output.contains("editorconfig"));

    // 2. Add editorconfig block
    let add_res = ToolRegistry::dispatch(
        workspace,
        "call_block_2",
        "kit_block_add",
        &json!({ "name": "editorconfig" }),
        None,
        1,
    )
    .await;
    assert!(add_res.success);
    assert!(workspace.join(".editorconfig").exists());

    // 3. Attempt to add again without force: conflict detected
    let conflict_res = ToolRegistry::dispatch(
        workspace,
        "call_block_3",
        "kit_block_add",
        &json!({ "name": "editorconfig" }),
        None,
        1,
    )
    .await;
    assert!(!conflict_res.success);
    assert!(conflict_res.output.to_lowercase().contains("conflict"));

    // 4. Overwrite with force = true
    let force_res = ToolRegistry::dispatch(
        workspace,
        "call_block_4",
        "kit_block_add",
        &json!({ "name": "editorconfig", "force": true }),
        None,
        1,
    )
    .await;
    assert!(force_res.success);
    assert!(force_res
        .output
        .contains("Successfully added architecture block"));
}

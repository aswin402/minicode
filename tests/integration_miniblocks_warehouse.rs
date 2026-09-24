use minicode::tools::minikit::sync::MiniKitSyncEngine;
use minicode::tools::ToolRegistry;
use serde_json::json;
use std::fs;
use tempfile::tempdir;

#[tokio::test]
async fn test_e2e_miniblocks_catalog_and_stats() {
    let temp_dir = tempdir().unwrap();
    let workspace = temp_dir.path();

    let res =
        ToolRegistry::dispatch(workspace, "call_stats", "block_stats", &json!({}), None, 1).await;

    assert!(res.success, "block_stats failed: {}", res.output);
    assert!(res.output.contains("# MiniBlocks UI Warehouse Statistics"));
    assert!(res.output.contains("Total Components:"));
    assert!(res.output.contains("- **Total Palettes:** 105"));
    assert!(res.output.contains("- **Total Gradients:** 212"));
    assert!(res.output.contains("- **Total Templates:** 3"));

    // Verify component count >= 1080
    let comp_line = res
        .output
        .lines()
        .find(|l| l.contains("Total Components:"))
        .expect("Total Components line exists");
    let count_str = comp_line
        .trim_start_matches("- **Total Components:** ")
        .trim();
    let count: usize = count_str.parse().expect("valid integer component count");
    assert!(
        count >= 1080,
        "Expected at least 1,080 components, found {}",
        count
    );
}

#[tokio::test]
async fn test_e2e_miniblocks_search_and_get() {
    let temp_dir = tempdir().unwrap();
    let workspace = temp_dir.path();

    // 1. Search for "navbar"
    let search_res = ToolRegistry::dispatch(
        workspace,
        "call_search",
        "block_search",
        &json!({
            "query": "navbar",
            "limit": 5
        }),
        None,
        1,
    )
    .await;

    assert!(
        search_res.success,
        "block_search failed: {}",
        search_res.output
    );
    assert!(search_res.output.contains("Found"));
    assert!(search_res.output.to_lowercase().contains("navbar"));

    // Extract component ID from markdown table
    // Format: | Name | Category | Framework | Version | Tags | Score | ID | Description |
    let id_line = search_res
        .output
        .lines()
        .find(|l| l.starts_with('|') && l.contains('`') && !l.contains("---"))
        .expect("component row with UUID in table");
    let comp_id = id_line
        .split('`')
        .nth(1)
        .expect("UUID between backticks in table row");

    // 2. Get full component source code and metadata by ID
    let get_res = ToolRegistry::dispatch(
        workspace,
        "call_get",
        "block_get",
        &json!({
            "id": comp_id
        }),
        None,
        1,
    )
    .await;

    assert!(get_res.success, "block_get failed: {}", get_res.output);
    assert!(get_res.output.contains("# Component:"));
    assert!(get_res.output.contains(&format!("- **ID:** `{}`", comp_id)));
    assert!(get_res.output.contains("```"));
    assert!(get_res.output.to_lowercase().contains("nav"));

    // 3. Multi-word Soft-OR query test ("navbar navigation header")
    let multi_res = ToolRegistry::dispatch(
        workspace,
        "call_search_multi",
        "block_search",
        &json!({
            "framework": "react",
            "query": "navbar navigation header",
            "limit": 5
        }),
        None,
        1,
    )
    .await;
    assert!(multi_res.success);
    assert!(
        multi_res.output.contains("Found"),
        "expected Soft-OR to find components: {}",
        multi_res.output
    );
}

#[tokio::test]
async fn test_e2e_miniblocks_palettes_and_gradients() {
    let temp_dir = tempdir().unwrap();
    let workspace = temp_dir.path();

    // 1. Query color palettes
    let pal_res = ToolRegistry::dispatch(
        workspace,
        "call_palettes",
        "block_palettes",
        &json!({
            "query": "dark",
            "limit": 5
        }),
        None,
        1,
    )
    .await;

    assert!(pal_res.success, "block_palettes failed: {}", pal_res.output);
    assert!(pal_res.output.contains("Found"));
    assert!(pal_res.output.contains("color palette(s)"));
    assert!(pal_res.output.contains("/* CSS Variables Export */"));
    assert!(pal_res.output.contains("--bg:"));
    assert!(pal_res.output.contains("--surface:"));
    assert!(pal_res.output.contains("--accent:"));
    assert!(pal_res.output.contains("--text:"));

    // 2. Query gradients
    let grad_res = ToolRegistry::dispatch(
        workspace,
        "call_gradients",
        "block_gradients",
        &json!({
            "query": "sunset",
            "limit": 5
        }),
        None,
        1,
    )
    .await;

    assert!(
        grad_res.success,
        "block_gradients failed: {}",
        grad_res.output
    );
    assert!(grad_res.output.contains("Found"));
    assert!(grad_res.output.contains("CSS gradient(s)"));
    assert!(grad_res.output.contains("background:"));
    assert!(
        grad_res.output.contains("linear-gradient")
            || grad_res.output.contains("radial-gradient")
            || grad_res.output.contains("conic-gradient")
    );
}

#[tokio::test]
async fn test_e2e_miniblocks_insert_and_path_sandbox() {
    let temp_dir = tempdir().unwrap();
    let workspace = temp_dir.path();

    let target_file = "src/components/Header.html";

    // 1. Mode: create
    let create_res = ToolRegistry::dispatch(
        workspace,
        "call_insert_create",
        "block_insert",
        &json!({
            "target_file": target_file,
            "code": "<header class=\"navbar\">Main Navigation</header>",
            "mode": "create"
        }),
        None,
        1,
    )
    .await;

    assert!(
        create_res.success,
        "block_insert create failed: {}",
        create_res.output
    );
    assert!(create_res.output.contains("Successfully inserted"));
    let created_path = workspace.join(target_file);
    assert!(created_path.exists());
    let initial_content = fs::read_to_string(&created_path).unwrap();
    assert_eq!(
        initial_content,
        "<header class=\"navbar\">Main Navigation</header>"
    );

    // 2. Mode: append
    let append_res = ToolRegistry::dispatch(
        workspace,
        "call_insert_append",
        "block_insert",
        &json!({
            "target_file": target_file,
            "code": "<nav class=\"links\"><a href=\"/\">Home</a></nav>",
            "mode": "append"
        }),
        None,
        1,
    )
    .await;

    assert!(
        append_res.success,
        "block_insert append failed: {}",
        append_res.output
    );
    let appended_content = fs::read_to_string(&created_path).unwrap();
    assert!(appended_content.contains("Main Navigation"));
    assert!(appended_content.contains("Home"));

    // 3. Path escaping security assertion: relative traversal outside workspace
    let escape_res = ToolRegistry::dispatch(
        workspace,
        "call_insert_escape",
        "block_insert",
        &json!({
            "target_file": "../outside.html",
            "code": "<script>alert('pwned')</script>",
            "mode": "create"
        }),
        None,
        1,
    )
    .await;

    assert!(!escape_res.success, "Path traversal should fail!");
    assert!(
        escape_res.output.to_lowercase().contains("outside")
            || escape_res.output.to_lowercase().contains("traversal")
            || escape_res.output.to_lowercase().contains("workspace")
            || escape_res.output.to_lowercase().contains("error"),
        "Expected error message on path traversal, got: {}",
        escape_res.output
    );
    assert!(!temp_dir
        .path()
        .parent()
        .unwrap()
        .join("outside.html")
        .exists());
}

#[tokio::test]
async fn test_e2e_miniblocks_save_update_delete_lifecycle() {
    let temp_dir = tempdir().unwrap();
    let workspace = temp_dir.path();

    let comp_name = "E2E Modern Test Card";

    // 1. Save new custom component
    let save_res = ToolRegistry::dispatch(
        workspace,
        "call_save",
        "block_save",
        &json!({
            "name": comp_name,
            "description": "Interactive test card component with hover elevation",
            "category": "card",
            "code": "<div class=\"p-6 rounded-xl bg-slate-900 border border-slate-800\"><h3>Card Title</h3><p>Body v1</p></div>",
            "framework": "tailwind",
            "dependencies": ["tailwindcss"],
            "tags": ["e2e", "card", "test", "tailwind"]
        }),
        None,
        1,
    )
    .await;

    assert!(save_res.success, "block_save failed: {}", save_res.output);
    assert!(save_res.output.contains(comp_name));
    assert!(save_res.output.contains("saved successfully"));

    // Extract component UUID
    let id_line = save_res
        .output
        .lines()
        .find(|l| l.contains("- ID: `"))
        .expect("ID line in save output");
    let comp_id = id_line.split('`').nth(1).expect("UUID between backticks");

    // 2. Search for saved component
    let search_res = ToolRegistry::dispatch(
        workspace,
        "call_search_saved",
        "block_search",
        &json!({
            "query": comp_name,
            "limit": 5
        }),
        None,
        1,
    )
    .await;

    assert!(
        search_res.success,
        "block_search failed: {}",
        search_res.output
    );
    assert!(search_res.output.contains(comp_name));

    // 3. Update component code and description (version 2)
    let update_res = ToolRegistry::dispatch(
        workspace,
        "call_update",
        "block_update",
        &json!({
            "id": comp_id,
            "description": "Updated interactive test card component with animated badge",
            "code": "<div class=\"p-6 rounded-xl bg-slate-900 border border-slate-800\"><h3>Card Title</h3><p>Body v2</p><span class=\"badge\">New</span></div>",
            "tags": ["e2e", "card", "v2"]
        }),
        None,
        1,
    )
    .await;

    assert!(
        update_res.success,
        "block_update failed: {}",
        update_res.output
    );
    assert!(update_res.output.contains("Version: 2"));
    assert!(update_res.output.contains("Updated interactive test card"));

    // Verify updated code via block_get
    let get_res = ToolRegistry::dispatch(
        workspace,
        "call_get_v2",
        "block_get",
        &json!({
            "id": comp_id
        }),
        None,
        1,
    )
    .await;

    assert!(get_res.success, "block_get failed: {}", get_res.output);
    assert!(get_res.output.contains("Version:** 2"));
    assert!(get_res.output.contains("Body v2"));

    // 4. Delete component
    let delete_res = ToolRegistry::dispatch(
        workspace,
        "call_delete",
        "block_delete",
        &json!({
            "id": comp_id
        }),
        None,
        1,
    )
    .await;

    assert!(
        delete_res.success,
        "block_delete failed: {}",
        delete_res.output
    );
    assert!(delete_res.output.contains("deleted successfully"));

    // 5. Confirm deletion
    let verify_res = ToolRegistry::dispatch(
        workspace,
        "call_get_deleted",
        "block_get",
        &json!({
            "id": comp_id
        }),
        None,
        1,
    )
    .await;

    assert!(
        !verify_res.success,
        "Deleted component should not be retrieved"
    );
}

#[tokio::test]
async fn test_e2e_miniblocks_scaffold() {
    let temp_dir = tempdir().unwrap();
    let workspace = temp_dir.path();

    let target_dir = "src/ui/landing";

    let scaffold_res = ToolRegistry::dispatch(
        workspace,
        "call_scaffold",
        "block_scaffold",
        &json!({
            "template_name": "landing",
            "target_dir": target_dir
        }),
        None,
        1,
    )
    .await;

    assert!(
        scaffold_res.success,
        "block_scaffold failed: {}",
        scaffold_res.output
    );
    assert!(scaffold_res.output.contains("Scaffolding Completed"));
    assert!(scaffold_res.output.contains("Files Written:"));

    let scaffolded_path = workspace.join(target_dir);
    assert!(scaffolded_path.exists());
    assert!(scaffolded_path.join("index.html").exists());

    // Verify index.html contains assembled layout
    let layout_content = fs::read_to_string(scaffolded_path.join("index.html")).unwrap();
    assert!(
        layout_content.contains("<!DOCTYPE html>")
            || layout_content.contains("<html")
            || layout_content.contains("<body")
    );
}

#[test]
fn test_e2e_minikit_sync_generates_miniblocks_skill() {
    let temp_dir = tempdir().unwrap();
    let docs_dir = temp_dir.path().join("minikit_docs");
    fs::create_dir_all(&docs_dir).unwrap();

    MiniKitSyncEngine::ensure_workflow_docs(&docs_dir, "e2e_project", "rust");

    let skill_file = docs_dir.join("skills").join("miniblocks.md");
    assert!(
        skill_file.exists(),
        "skills/miniblocks.md must be generated"
    );

    let content = fs::read_to_string(&skill_file).unwrap();
    assert!(content.contains("# MiniBlocks Native UI Component & Design Warehouse"));
    assert!(content.contains("1,082+"));
    assert!(content.contains("105 curated palettes") || content.contains("105"));
    assert!(content.contains("212 gradients") || content.contains("212"));
    assert!(content.contains("3 layout templates") || content.contains("3"));
    assert!(content.contains("block_search"));
    assert!(content.contains("block_get"));
    assert!(content.contains("block_insert"));
    assert!(content.contains("block_save"));
    assert!(content.contains("block_update"));
    assert!(content.contains("block_delete"));
    assert!(content.contains("block_palettes"));
    assert!(content.contains("block_gradients"));
    assert!(content.contains("block_scaffold"));
    assert!(content.contains("block_stats"));
    assert!(content.contains("F6"));
    assert!(content.contains("/blocks"));
}

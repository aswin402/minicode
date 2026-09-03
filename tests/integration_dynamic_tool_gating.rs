use minicode::config::ToolFilterMode;
use minicode::context::intent_filter::IntentClassifier;
use minicode::tools::category::{assemble_active_tools, get_core_schemas, ToolCategory};
use minicode::tools::ToolRegistry;
use serde_json::json;
use std::collections::HashSet;
use tempfile::tempdir;

#[tokio::test]
async fn test_core_schemas_minimal_and_efficient() {
    let core = get_core_schemas();
    assert!(
        core.len() >= 8 && core.len() <= 10,
        "Core schemas should be 8-10 tools, got {}",
        core.len()
    );

    let names: Vec<&str> = core.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"read_file"));
    assert!(names.contains(&"patch_file"));
    assert!(names.contains(&"write_file"));
    assert!(names.contains(&"exec_cmd"));
    assert!(names.contains(&"grep_search"));
    assert!(names.contains(&"locate_symbol"));
    assert!(names.contains(&"create_plan"));
    assert!(names.contains(&"update_progress"));
    assert!(names.contains(&"activate_tools"));
}

#[tokio::test]
async fn test_intent_classifier_domain_detection() {
    // 1. Plain greeting -> no domain intent
    assert!(IntentClassifier::detect("hello there minicode").is_empty());
    assert!(IntentClassifier::detect("hii").is_empty());

    // 2. Git intent
    let git_intent = IntentClassifier::detect("please commit these changes to git");
    assert!(git_intent.contains(&ToolCategory::Git));

    // 3. Web intent
    let web_intent = IntentClassifier::detect("search web for latest rust Tokio docs");
    assert!(web_intent.contains(&ToolCategory::Web));

    // 4. CodeGraph intent
    let graph_intent =
        IntentClassifier::detect("what is the blast radius and callers of this function?");
    assert!(graph_intent.contains(&ToolCategory::Codegraph));

    // 5. Onpkg intent
    let onpkg_intent = IntentClassifier::detect("scaffold a new stack with onpkg template");
    assert!(onpkg_intent.contains(&ToolCategory::Onpkg));

    // 6. Multi-agent intent
    let agent_intent = IntentClassifier::detect("delegate this subtask to a subagent swarm");
    assert!(agent_intent.contains(&ToolCategory::Agent));
}

#[tokio::test]
async fn test_assemble_active_tools_modes() {
    let dynamic_cats = HashSet::new();

    // 1. Dynamic mode with plain prompt -> Core tools only
    let tools_plain = assemble_active_tools(ToolFilterMode::Dynamic, "hii", &dynamic_cats);
    assert!(
        tools_plain.len() <= 10,
        "Expected minimal core tools for greeting, got {}",
        tools_plain.len()
    );

    // 2. Dynamic mode with Git prompt -> Core + Git tools
    let tools_git = assemble_active_tools(
        ToolFilterMode::Dynamic,
        "commit changes to git main branch",
        &dynamic_cats,
    );
    let names_git: Vec<&str> = tools_git.iter().map(|s| s.name.as_str()).collect();
    assert!(names_git.contains(&"git_commit"));
    assert!(names_git.contains(&"git_status"));
    assert!(names_git.contains(&"read_file"));

    // 3. CoreOnly mode ignores prompt intent
    let tools_core_only = assemble_active_tools(
        ToolFilterMode::CoreOnly,
        "commit changes to git main branch",
        &dynamic_cats,
    );
    let names_core: Vec<&str> = tools_core_only.iter().map(|s| s.name.as_str()).collect();
    assert!(!names_core.contains(&"git_commit"));
    assert!(names_core.contains(&"read_file"));
    assert_eq!(tools_core_only.len(), get_core_schemas().len());

    // 4. Full mode includes all tools
    let tools_full = assemble_active_tools(ToolFilterMode::Full, "hii", &dynamic_cats);
    assert!(tools_full.len() >= 100);
}

#[tokio::test]
async fn test_activate_tools_dispatch_meta_tool() {
    let temp = tempdir().unwrap();
    let workspace_root = temp.path();

    // 1. Activate git category
    let res = ToolRegistry::dispatch(
        workspace_root,
        "test_call_1",
        "activate_tools",
        &json!({ "category": "git", "reason": "Need to stage and commit code" }),
        None,
        1,
    )
    .await;
    assert!(res.success);
    assert!(res.output.contains("Successfully activated 'git' category"));

    // 2. Activate web category
    let res = ToolRegistry::dispatch(
        workspace_root,
        "test_call_2",
        "activate_tools",
        &json!({ "category": "web" }),
        None,
        1,
    )
    .await;
    assert!(res.success);
    assert!(res.output.contains("Successfully activated 'web' category"));

    // 3. Invalid category returns error
    let res = ToolRegistry::dispatch(
        workspace_root,
        "test_call_3",
        "activate_tools",
        &json!({ "category": "non_existent_category" }),
        None,
        1,
    )
    .await;
    assert!(!res.success);
    assert!(res.output.contains("Unknown tool category"));
}

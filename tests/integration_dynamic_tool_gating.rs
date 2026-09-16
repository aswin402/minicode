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

#[tokio::test]
async fn test_dynamic_mcp_tool_gating_and_isolation() {
    use minicode::agent::provider::ToolSchema;
    use minicode::tools::category::assemble_active_tools_with_mcp;
    use std::collections::{HashMap, HashSet};

    let mut mcp_by_server = HashMap::new();
    let figma_tools = vec![
        ToolSchema {
            name: "mcp__figma__create_frame".to_string(),
            description: "Creates an auto-layout frame".to_string(),
            parameters: json!({"type": "object"}),
        },
        ToolSchema {
            name: "mcp__figma__create_rectangle".to_string(),
            description: "Draws a rectangle on canvas".to_string(),
            parameters: json!({"type": "object"}),
        },
        ToolSchema {
            name: "mcp__figma__set_fill_color".to_string(),
            description: "Sets node fill color".to_string(),
            parameters: json!({"type": "object"}),
        },
        ToolSchema {
            name: "mcp__figma__scan_text_nodes".to_string(),
            description: "Scans text layers".to_string(),
            parameters: json!({"type": "object"}),
        },
        ToolSchema {
            name: "mcp__figma__export_node".to_string(),
            description: "Exports frame as png".to_string(),
            parameters: json!({"type": "object"}),
        },
    ];
    let github_tools = vec![
        ToolSchema {
            name: "mcp__github__create_issue".to_string(),
            description: "Opens an issue on GitHub".to_string(),
            parameters: json!({"type": "object"}),
        },
        ToolSchema {
            name: "mcp__github__create_pull_request".to_string(),
            description: "Opens a pull request".to_string(),
            parameters: json!({"type": "object"}),
        },
    ];

    mcp_by_server.insert("figma".to_string(), figma_tools);
    mcp_by_server.insert("github".to_string(), github_tools);

    let empty_cats = HashSet::new();
    let empty_mcp = HashSet::new();

    // 1. Plain coding prompt -> 0 MCP tools included (isolated!)
    let tools_plain = assemble_active_tools_with_mcp(
        ToolFilterMode::Dynamic,
        "fix bug in src/main.rs",
        &empty_cats,
        &mcp_by_server,
        &empty_mcp,
    );
    let plain_names: Vec<&str> = tools_plain.iter().map(|s| s.name.as_str()).collect();
    assert!(!plain_names.contains(&"mcp__figma__create_frame"));
    assert!(!plain_names.contains(&"mcp__github__create_issue"));
    assert!(plain_names.contains(&"read_file"));

    // 2. Figma prompt -> Figma tools included, GitHub tools excluded
    let tools_figma = assemble_active_tools_with_mcp(
        ToolFilterMode::Dynamic,
        "inspect the figma design frame for the login screen",
        &empty_cats,
        &mcp_by_server,
        &empty_mcp,
    );
    let figma_names: Vec<&str> = tools_figma.iter().map(|s| s.name.as_str()).collect();
    assert!(figma_names.contains(&"mcp__figma__create_frame"));
    assert!(figma_names.contains(&"mcp__figma__set_fill_color"));
    assert!(!figma_names.contains(&"mcp__github__create_issue"));

    // 3. GitHub prompt -> GitHub tools included, Figma tools excluded
    let tools_github = assemble_active_tools_with_mcp(
        ToolFilterMode::Dynamic,
        "open a pull request for this branch",
        &empty_cats,
        &mcp_by_server,
        &empty_mcp,
    );
    let github_names: Vec<&str> = tools_github.iter().map(|s| s.name.as_str()).collect();
    assert!(github_names.contains(&"mcp__github__create_pull_request"));
    assert!(!github_names.contains(&"mcp__figma__create_frame"));

    // 4. CoreOnly mode -> 0 MCP tools even if prompt mentions figma
    let tools_core = assemble_active_tools_with_mcp(
        ToolFilterMode::CoreOnly,
        "draw on figma",
        &empty_cats,
        &mcp_by_server,
        &empty_mcp,
    );
    let core_names: Vec<&str> = tools_core.iter().map(|s| s.name.as_str()).collect();
    assert!(!core_names.contains(&"mcp__figma__create_frame"));
    assert!(core_names.contains(&"read_file"));

    // 5. Full mode -> All MCP tools + native tools included
    let tools_full = assemble_active_tools_with_mcp(
        ToolFilterMode::Full,
        "hello",
        &empty_cats,
        &mcp_by_server,
        &empty_mcp,
    );
    let full_names: Vec<&str> = tools_full.iter().map(|s| s.name.as_str()).collect();
    assert!(full_names.contains(&"mcp__figma__create_frame"));
    assert!(full_names.contains(&"mcp__github__create_pull_request"));
}

#[test]
fn test_tool_schema_compactor_fidelity() {
    use minicode::agent::provider::ToolSchema;
    use minicode::tools::schema_compactor::ToolSchemaCompactor;

    let raw = ToolSchema {
        name: "mcp__postgres__execute_query".to_string(),
        description: "Executes an arbitrary SQL query against the active PostgreSQL database cluster. Use this tool when you need to inspect table schemas, retrieve records, insert data, or run migration statements. Do not use this tool for destructive DROP operations without approval.".to_string(),
        parameters: json!({
            "type": "object",
            "properties": {
                "sql": {
                    "type": "string",
                    "description": "The exact SQL statement to execute. Example: ```sql\nSELECT * FROM users WHERE active = true;\n```. Must be syntactically valid."
                },
                "read_only": {
                    "type": "boolean",
                    "description": "If true, enforces read-only query execution.",
                    "default": false
                }
            },
            "required": ["sql"]
        }),
    };

    let compacted = ToolSchemaCompactor::compact_schema(&raw, 120, 80);

    // Schema contract integrity
    assert_eq!(compacted.name, "mcp__postgres__execute_query");
    assert_eq!(compacted.parameters["required"], json!(["sql"]));
    assert_eq!(compacted.parameters["properties"]["sql"]["type"], "string");
    assert_eq!(
        compacted.parameters["properties"]["read_only"]["type"],
        "boolean"
    );

    // Token savings
    assert!(compacted.description.len() < raw.description.len());
    assert_eq!(
        compacted.description,
        "Executes an arbitrary SQL query against the active PostgreSQL database cluster."
    );
    let sql_desc = compacted.parameters["properties"]["sql"]["description"]
        .as_str()
        .unwrap();
    assert!(!sql_desc.contains("```"));
    assert!(sql_desc.len() <= 80);
}

#[tokio::test]
async fn test_activate_tools_mcp_server_dispatch() {
    let temp = tempdir().unwrap();
    let workspace_root = temp.path();

    // Explicit MCP server activation with mcp: prefix
    let res = ToolRegistry::dispatch(
        workspace_root,
        "test_call_mcp_1",
        "activate_tools",
        &json!({ "category": "mcp:figma", "reason": "Need Figma UI canvas inspection" }),
        None,
        1,
    )
    .await;
    assert!(res.success);
    assert!(res
        .output
        .contains("Successfully activated MCP server 'figma'"));

    // All MCP servers activation
    let res = ToolRegistry::dispatch(
        workspace_root,
        "test_call_mcp_2",
        "activate_tools",
        &json!({ "category": "mcp", "reason": "Need all connected MCP servers" }),
        None,
        1,
    )
    .await;
    assert!(res.success);
    assert!(res
        .output
        .contains("All connected MCP servers successfully activated"));
}

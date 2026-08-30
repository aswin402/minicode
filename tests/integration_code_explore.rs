use minicode::agent::intent::{match_intent, AgentIntent};
use minicode::context::explorer::CodeExploreEngine;
use minicode::context::graph::CodeGraph;
use minicode::context::layers::{ArchitecturalLayer, LayerClassifier};
use minicode::context::okf::{OkfDocument, OkfFrontmatter, OkfManager};
use minicode::tools::registry::explore_tools;
use serde_json::json;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_architectural_layer_classifier() {
    // 1. Path classification
    assert_eq!(
        LayerClassifier::classify_path(std::path::Path::new("src/ui/modal.rs")),
        ArchitecturalLayer::Ui
    );
    assert_eq!(
        LayerClassifier::classify_path(std::path::Path::new("src/components/Header.tsx")),
        ArchitecturalLayer::Ui
    );
    assert_eq!(
        LayerClassifier::classify_path(std::path::Path::new("src/api/routes.rs")),
        ArchitecturalLayer::Api
    );
    assert_eq!(
        LayerClassifier::classify_path(std::path::Path::new("src/services/auth_service.py")),
        ArchitecturalLayer::Service
    );
    assert_eq!(
        LayerClassifier::classify_path(std::path::Path::new("src/db/models.rs")),
        ArchitecturalLayer::Data
    );
    assert_eq!(
        LayerClassifier::classify_path(std::path::Path::new("src/utils/helpers.rs")),
        ArchitecturalLayer::Utility
    );

    // 2. Symbol classification fallback
    assert_eq!(
        LayerClassifier::classify_symbol(
            std::path::Path::new("src/misc.rs"),
            "render_view",
            "function"
        ),
        ArchitecturalLayer::Ui
    );
    assert_eq!(
        LayerClassifier::classify_symbol(
            std::path::Path::new("src/misc.rs"),
            "fetch_api_data",
            "function"
        ),
        ArchitecturalLayer::Api
    );
    assert_eq!(
        LayerClassifier::classify_symbol(
            std::path::Path::new("src/misc.rs"),
            "save_user_record",
            "function"
        ),
        ArchitecturalLayer::Data
    );
}

#[tokio::test]
async fn test_code_explore_engine_surgical_context() {
    let dir = tempdir().expect("create tempdir");
    let ws = dir.path();

    // Create a mock codebase with callers and callees
    let math_file = ws.join("math.rs");
    fs::write(
        &math_file,
        r#"
pub fn compute_sum(a: i32, b: i32) -> i32 {
    a + b
}
"#,
    )
    .expect("write math.rs");

    let app_file = ws.join("app.rs");
    fs::write(
        &app_file,
        r#"
mod math;

pub fn execute_workflow() {
    let result = math::compute_sum(10, 20);
    println!("{}", result);
}
"#,
    )
    .expect("write app.rs");

    let mut graph = CodeGraph::new();
    graph.build_graph(ws).expect("build graph");

    let explore_res = CodeExploreEngine::explore(ws, &graph, "compute_sum", None, 2, true)
        .expect("explore engine");

    assert_eq!(explore_res.matches.len(), 1);
    let matched = &explore_res.matches[0];
    assert_eq!(matched.symbol_name, "compute_sum");
    assert_eq!(matched.file_path, "math.rs");
    assert!(matched
        .source_code
        .as_ref()
        .unwrap()
        .contains("pub fn compute_sum"));
    // Caller detection across app.rs
    assert!(matched
        .callers
        .iter()
        .any(|c| c.name == "execute_workflow" && c.file_path == "app.rs"));
}

#[tokio::test]
async fn test_diff_impact_tool_dispatch() {
    let dir = tempdir().expect("create tempdir");
    let ws = dir.path();

    // Setup git repo with tracked files
    let lib_file = ws.join("lib.rs");
    fs::write(
        &lib_file,
        r#"
pub fn helper() -> bool {
    true
}
"#,
    )
    .expect("write lib.rs");

    let args = json!({
        "files": ["lib.rs"],
        "max_depth": 2
    });

    let result = explore_tools::dispatch("diff_impact", &args, ws).await;
    assert!(result.is_some());
    let output = result.unwrap().expect("tool execution");
    assert!(output.contains("Blast Radius"));
    assert!(output.contains("lib.rs"));
}

#[test]
fn test_okf_v02_document_and_manager() {
    let dir = tempdir().expect("create tempdir");
    let docs_dir = dir.path().join("onpkg_docs");
    fs::create_dir_all(&docs_dir).expect("create docs_dir");

    // 1. Create OKF document with frontmatter
    let frontmatter = OkfFrontmatter {
        concept_type: "prd".to_string(),
        title: Some("Auth Module PRD".to_string()),
        description: Some("Authentication and authorization specifications".to_string()),
        resource: None,
        tags: vec!["auth".to_string(), "security".to_string()],
        sources: Vec::new(),
        generated: None,
        verified: None,
        status: Some("active".to_string()),
        superseded_by: None,
    };

    let body = "# Authentication Requirements\n1. OAuth2 with PKCE.\n";
    let serialized = OkfDocument::serialize(&frontmatter, body);
    assert!(serialized.contains("---"));
    assert!(serialized.contains("type: prd"));
    assert!(serialized.contains("title: \"Auth Module PRD\""));

    // 2. Parse OKF document back
    let (parsed_fm, parsed_body) = OkfDocument::parse(&serialized).expect("parse markdown");
    let parsed = parsed_fm.expect("frontmatter exists");
    assert_eq!(parsed.title.as_deref(), Some("Auth Module PRD"));
    assert_eq!(parsed.concept_type, "prd");
    assert!(parsed_body.contains("Authentication Requirements"));

    // 3. Write document to docs_dir
    let prd_file = docs_dir.join("prd.md");
    fs::write(&prd_file, &serialized).expect("write prd.md");

    // 4. Test OkfManager index and log generation
    let index_msg = OkfManager::generate_index_md(&docs_dir).expect("generate index");
    assert!(index_msg.contains("Knowledge Catalog Index"));
    let index_content = fs::read_to_string(docs_dir.join("index.md")).expect("read index.md");
    assert!(index_content.contains("Knowledge Catalog Index"));
    assert!(index_content.contains("Auth Module PRD"));

    OkfManager::append_log_entry(
        &docs_dir,
        "minicode/v0.0.64",
        "CREATE",
        "prd.md",
        "Added OAuth2 specifications",
    )
    .expect("append log");

    let log_content = fs::read_to_string(docs_dir.join("log.md")).expect("read log.md");
    assert!(log_content.contains("Knowledge Evolution Ledger"));
    assert!(log_content.contains("minicode/v0.0.64"));
    assert!(log_content.contains("CREATE"));
    assert!(log_content.contains("prd.md"));
}

#[test]
fn test_code_explore_intent_routing() {
    // Direct slash command
    let res = match_intent("/explore auth").expect("slash match");
    assert_eq!(res.intent, AgentIntent::CodeExplore);
    assert_eq!(res.query, "auth");
    assert_eq!(res.confidence, 1.0);

    // Natural language exploration
    let res = match_intent("explore callers of execute_turn").expect("nl match");
    assert_eq!(res.intent, AgentIntent::CodeExplore);
    assert!(res.confidence >= 0.90);

    let res = match_intent("show call graph of parse_frontmatter").expect("nl match");
    assert_eq!(res.intent, AgentIntent::CodeExplore);

    let res = match_intent("what is the blast radius of error.rs").expect("nl match");
    assert_eq!(res.intent, AgentIntent::CodeExplore);
}

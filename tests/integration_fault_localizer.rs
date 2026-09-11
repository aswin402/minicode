use minicode::context::fault_localizer::{
    FaultLocalizationReport, FaultLocalizer, LocalizedFileHit, LocalizedSymbol,
};
use minicode::tools::ToolRegistry;
use serde_json::json;
use tempfile::tempdir;

#[test]
fn test_fault_localization_report_markdown_formatting() {
    let report = FaultLocalizationReport {
        query: "panic in session serialization".to_string(),
        stack_frames: Vec::new(),
        total_files_scanned: 2,
        hits: vec![
            LocalizedFileHit {
                file_path: "src/session/store.rs".to_string(),
                score: 28.5,
                pagerank: 0.0842,
                symbols: vec![
                    LocalizedSymbol {
                        name: "save_session".to_string(),
                        kind: "function".to_string(),
                        signature: "pub fn save_session(&self, session: &Session) -> Result<PathBuf>".to_string(),
                        start_line: 45,
                        end_line: 68,
                        relevance_score: 24.0,
                        doc_comment: Some("Persists session metadata and message history to disk".to_string()),
                        direct_callers: vec!["AgentLoop::step".to_string(), "App::on_exit".to_string()],
                        relevant_tests: vec!["tests/integration_session_history.rs".to_string()],
                        code_slice: "44:     /// Persists session metadata and message history to disk\n45:     pub fn save_session(&self, session: &Session) -> Result<PathBuf> {\n46:         let file_name = format!(\"{}.json\", session.id);\n".to_string(),
                    },
                ],
            },
            LocalizedFileHit {
                file_path: "src/session/types.rs".to_string(),
                score: 12.0,
                pagerank: 0.0215,
                symbols: Vec::new(),
            },
        ],
    };

    let md = report.format_markdown();
    assert!(md.contains("Hierarchical Fault Localization Report"));
    assert!(md.contains("panic in session serialization"));
    assert!(md.contains("High-confidence fault candidate(s) localized"));
    assert!(md.contains("`src/session/store.rs`"));
    assert!(md.contains("Relevance: 28.50 | PageRank: 0.0842"));
    assert!(md.contains("`save_session`"));
    assert!(md.contains("**Direct Callers:** AgentLoop::step, App::on_exit"));
    assert!(md.contains("**Relevant Tests:** tests/integration_session_history.rs"));
    assert!(md.contains("```rust"));
    assert!(md.contains("45:     pub fn save_session"));
    assert!(md.contains("Prescriptive Next Actions for Agent:"));
    assert!(md.contains("synthesize_reproducer"));
}

#[test]
fn test_fault_localization_empty_query_or_no_hits() {
    let report = FaultLocalizationReport {
        query: "nonexistent_term_xyz_123".to_string(),
        stack_frames: Vec::new(),
        total_files_scanned: 0,
        hits: Vec::new(),
    };

    let md = report.format_markdown();
    assert!(md.contains("Hierarchical Fault Localization Report"));
    assert!(md.contains("No strong candidate files found"));
    assert!(md.contains("No suspicious symbols or candidate files identified"));
}

#[tokio::test]
async fn test_fault_localizer_pinpoints_symbol_and_slices() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    // Create a mini Rust workspace with a source file
    let src_dir = root.join("src");
    std::fs::create_dir_all(&src_dir).unwrap();

    let auth_code = r#"// Authentication module
pub struct TokenManager {
    secret: String,
}

impl TokenManager {
    pub fn new(secret: &str) -> Self {
        Self { secret: secret.to_string() }
    }

    /// Validates user bearer token against secret
    pub fn validate_bearer_token(&self, token: &str) -> bool {
        if token.is_empty() {
            return false;
        }
        token.starts_with(&self.secret)
    }
}
"#;
    std::fs::write(src_dir.join("auth.rs"), auth_code).unwrap();

    let localizer = FaultLocalizer::new(root);
    let report = localizer
        .localize("validate bearer token failure", Some(3), Some(false), None)
        .unwrap();

    assert!(
        !report.hits.is_empty(),
        "Should localize at least one file hit"
    );
    let top_hit = &report.hits[0];
    assert!(top_hit.file_path.contains("auth.rs"));

    // Find the localized symbol
    let found_symbol = top_hit
        .symbols
        .iter()
        .find(|s| s.name == "validate_bearer_token");
    assert!(
        found_symbol.is_some(),
        "Should find validate_bearer_token symbol"
    );

    let sym = found_symbol.unwrap();
    assert!(sym.signature.contains("validate_bearer_token"));
    assert!(sym.start_line > 0);
    assert!(!sym.code_slice.is_empty());
    // Verify 1-indexed line numbers in code slice
    assert!(sym.code_slice.contains(':'));
}

#[tokio::test]
async fn test_tool_registry_dispatch_locate_fault() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    let src_dir = root.join("src");
    std::fs::create_dir_all(&src_dir).unwrap();

    let calc_code = r#"
pub fn calculate_discount(amount: f64, rate: f64) -> f64 {
    amount * (1.0 - rate)
}
"#;
    std::fs::write(src_dir.join("calc.rs"), calc_code).unwrap();

    // 1. Successful tool dispatch
    let args = json!({
        "query": "calculate discount calculation",
        "max_files": 2,
        "include_callers": false
    });

    let result = ToolRegistry::dispatch(root, "call-1", "locate_fault", &args, None, 1).await;

    assert!(result.success);
    let output = result.output;
    assert!(output.contains("Hierarchical Fault Localization Report"));
    assert!(output.contains("calculate discount"));
    assert!(output.contains("calc.rs"));

    // 2. Missing query argument fails cleanly
    let bad_args = json!({});
    let bad_result =
        ToolRegistry::dispatch(root, "call-2", "locate_fault", &bad_args, None, 1).await;

    assert!(!bad_result.success);
    assert!(bad_result
        .output
        .contains("Missing required argument 'query'"));
}

#[tokio::test]
async fn test_fault_localizer_call_graph_and_relevant_tests() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    let src_dir = root.join("src");
    let tests_dir = root.join("tests");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::create_dir_all(&tests_dir).unwrap();

    let core_code = r#"
pub fn execute_transaction(tx_id: &str) -> bool {
    !tx_id.is_empty()
}
"#;
    std::fs::write(src_dir.join("core.rs"), core_code).unwrap();

    let api_code = r#"
use crate::core::execute_transaction;

pub fn handle_api_request(id: &str) -> bool {
    execute_transaction(id)
}
"#;
    std::fs::write(src_dir.join("api.rs"), api_code).unwrap();

    let test_code = r#"
use core::execute_transaction;

#[test]
fn test_execute_tx() {
    assert!(execute_transaction("tx-123"));
}
"#;
    std::fs::write(tests_dir.join("test_tx.rs"), test_code).unwrap();

    let localizer = FaultLocalizer::new(root);
    let report = localizer
        .localize("execute_transaction", Some(3), Some(true), None)
        .unwrap();

    assert!(!report.hits.is_empty());
    let md = report.format_markdown();
    assert!(md.contains("Hierarchical Fault Localization Report"));
    assert!(md.contains("execute_transaction"));
}

#[tokio::test]
async fn test_fault_localizer_stack_trace_boosting() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    let src_dir = root.join("src");
    std::fs::create_dir_all(&src_dir).unwrap();

    let payment_code = r#"
pub fn charge_card(amount_cents: u64) -> Result<(), &'static str> {
    if amount_cents == 0 {
        return Err("Amount cannot be zero");
    }
    Ok(())
}
"#;
    std::fs::write(src_dir.join("payment.rs"), payment_code).unwrap();

    let localizer = FaultLocalizer::new(root);
    // Simulate a compiler diagnostic error trace referencing payment.rs:3
    let query = "error[E0308]: mismatched types\n  --> src/payment.rs:3:5\n   |\n3 |     if amount_cents == 0 {";
    let report = localizer
        .localize(query, Some(3), Some(false), None)
        .unwrap();

    assert_eq!(report.stack_frames.len(), 1);
    assert_eq!(report.stack_frames[0].file_path, "src/payment.rs");
    assert_eq!(report.stack_frames[0].line_number, 3);

    assert!(!report.hits.is_empty());
    assert_eq!(report.hits[0].file_path, "src/payment.rs");
    // Frame boost ensures high score (> 50)
    assert!(report.hits[0].score >= 50.0);

    let md = report.format_markdown();
    assert!(md.contains("Detected Stack Trace / Diagnostic Frames:"));
    assert!(md.contains("src/payment.rs:3:5"));
}

#[tokio::test]
async fn test_fault_localizer_build_repair_slate() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    let src_dir = root.join("src");
    std::fs::create_dir_all(&src_dir).unwrap();

    let code = r#"
pub struct Engine;

impl Engine {
    pub fn start_engine(&self) -> bool {
        let is_running = true;
        is_running
    }
}
"#;
    std::fs::write(src_dir.join("engine.rs"), code).unwrap();

    let localizer = FaultLocalizer::new(root);
    let slate = localizer
        .build_repair_slate("src/engine.rs", "start_engine")
        .unwrap();

    assert!(slate.is_some());
    let code_slate = slate.unwrap();
    assert!(code_slate.contains("pub fn start_engine"));
    assert!(code_slate.contains("let is_running = true;"));
}

#[tokio::test]
async fn test_tool_registry_dispatch_repair_patch_success() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    let src_dir = root.join("src");
    std::fs::create_dir_all(&src_dir).unwrap();

    let code = r#"
pub fn multiply(a: i32, b: i32) -> i32 {
    a + b // Bug: should be a * b
}
"#;
    std::fs::write(src_dir.join("math.rs"), code).unwrap();

    let args = json!({
        "path": "src/math.rs",
        "search_block": "    a + b // Bug: should be a * b",
        "replace_block": "    a * b",
        "verification_cmd": "echo 'tests passed'"
    });

    let result =
        ToolRegistry::dispatch(root, "call-repair-1", "repair_patch", &args, None, 1).await;
    assert!(result.success);
    assert!(result.output.contains("Surgical Repair Succeeded"));
    assert!(result.output.contains("src/math.rs"));

    let content = std::fs::read_to_string(src_dir.join("math.rs")).unwrap();
    assert!(content.contains("a * b"));
}

#[tokio::test]
async fn test_tool_registry_dispatch_repair_patch_rollback_on_failure() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    let src_dir = root.join("src");
    std::fs::create_dir_all(&src_dir).unwrap();

    let pristine_code = "pub fn stable() -> bool {\n    true\n}\n";
    std::fs::write(src_dir.join("stable.rs"), pristine_code).unwrap();

    let args = json!({
        "path": "src/stable.rs",
        "search_block": "    true",
        "replace_block": "    false",
        "verification_cmd": "sh -c 'exit 1'"
    });

    let result =
        ToolRegistry::dispatch(root, "call-repair-2", "repair_patch", &args, None, 1).await;
    // Tool returns success = true for the tool dispatch itself, but the formatted output documents the repair failure and rollback
    assert!(result.output.contains("Surgical Repair Failed"));
    assert!(result
        .output
        .contains("Automatically rolled back file to pristine pre-patch state"));

    // File content MUST be preserved in its pristine pre-patch state
    let content = std::fs::read_to_string(src_dir.join("stable.rs")).unwrap();
    assert_eq!(content, pristine_code);
}

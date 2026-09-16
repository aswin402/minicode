use minicode::agent::subagent::reducer::{SubagentSynthesisContext, SubagentSynthesisReducer};
use minicode::agent::subagent::transcript::{
    get_global_transcript_store, SubagentStepRecord, SubagentTranscript, SubagentTranscriptStore,
};
use minicode::agent::subagent::types::SubagentRole;
use minicode::context::budget::ccr_cache::CcrCache;
use minicode::tools::registry::agent_tools;
use serde_json::json;
use tempfile::tempdir;

#[test]
fn test_subagent_step_record_and_transcript() {
    let mut transcript = SubagentTranscript::new(
        "researcher-1",
        SubagentRole::Researcher,
        "Explore authentication implementation".to_string(),
    );

    assert_eq!(transcript.subagent_id, "researcher-1");
    assert_eq!(transcript.steps.len(), 0);

    // Add step 1: read_file
    transcript.add_step(SubagentStepRecord::new(
        1,
        1,
        "read_file".to_string(),
        json!({"path": "src/auth.rs"}),
        "pub fn verify_token(token: &str) -> bool { true }".to_string(),
        true,
        15,
    ));

    // Add step 2: exec_cmd failure
    transcript.add_step(SubagentStepRecord::new(
        2,
        2,
        "exec_cmd".to_string(),
        json!({"cmd": "cargo check"}),
        "error[E0425]: cannot find value `token` in this scope".to_string(),
        false,
        210,
    ));

    assert_eq!(transcript.steps.len(), 2);
    assert!(transcript.get_step(1).is_some());
    assert!(transcript.get_step(2).is_some());
    assert!(transcript.get_step(99).is_none());

    // Check step detail formatting
    let detail_1 = transcript.format_step_detail(1, Some(10)).unwrap();
    assert!(detail_1.contains("Step 1 Details"));
    assert!(detail_1.contains("`read_file`"));
    assert!(detail_1.contains("verify_token"));

    // Check errors formatting
    let errors = transcript.format_errors();
    assert!(errors.contains("Failed Tool Steps"));
    assert!(errors.contains("error[E0425]"));

    // Check overview table
    let overview = transcript.format_overview(Some(10));
    assert!(overview.contains("researcher-1"));
    assert!(overview.contains("| 1 | 1 | `read_file` | ✅ OK | 15ms |"));
    assert!(overview.contains("| 2 | 2 | `exec_cmd` | ❌ ERR | 210ms |"));
}

#[test]
fn test_subagent_transcript_store_persistence() {
    let dir = tempdir().unwrap();
    let workspace_root = dir.path();

    let store = SubagentTranscriptStore::new();
    let mut transcript = SubagentTranscript::new(
        "worker-42",
        SubagentRole::TestEngineer,
        "Run integration tests".to_string(),
    );
    transcript.add_step(SubagentStepRecord::new(
        1,
        1,
        "exec_cmd".to_string(),
        json!({"cmd": "cargo test"}),
        "test result: ok. 12 passed; 0 failed".to_string(),
        true,
        350,
    ));
    transcript.total_tokens = 1450;
    transcript.turns_executed = 2;

    // Store in memory & disk
    store.store(transcript.clone(), Some(workspace_root));

    // Verify disk path exists
    let disk_file = workspace_root
        .join(".minicode")
        .join("subagents")
        .join("worker-42")
        .join("transcript.json");
    assert!(disk_file.exists(), "Transcript must be persisted to disk");

    // Retrieve from store
    let retrieved = store.get("worker-42", Some(workspace_root)).unwrap();
    assert_eq!(retrieved.subagent_id, "worker-42");
    assert_eq!(retrieved.steps.len(), 1);
    assert_eq!(retrieved.steps[0].tool_name, "exec_cmd");

    // Simulate fresh memory cache and hydrate from disk
    let fresh_store = SubagentTranscriptStore::new();
    let loaded = fresh_store.get("worker-42", Some(workspace_root)).unwrap();
    assert_eq!(loaded.subagent_id, "worker-42");
    assert_eq!(loaded.total_tokens, 1450);
}

#[tokio::test]
async fn test_subagent_synthesis_reducer_ast_and_verifications() {
    let dir = tempdir().unwrap();
    let workspace_root = dir.path();

    // Create a rust source file in workspace
    let rs_path = workspace_root.join("math.rs");
    std::fs::write(
        &rs_path,
        "pub fn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n\npub fn multiply(a: i32, b: i32) -> i32 {\n    a * b\n}\n",
    )
    .unwrap();

    let mut transcript = SubagentTranscript::new(
        "tester-99",
        SubagentRole::TestEngineer,
        "Add multiply function and verify tests".to_string(),
    );

    transcript.add_step(SubagentStepRecord::new(
        1,
        1,
        "write_file".to_string(),
        json!({"path": "math.rs"}),
        "File written successfully".to_string(),
        true,
        10,
    ));

    transcript.add_step(SubagentStepRecord::new(
        2,
        2,
        "exec_cmd".to_string(),
        json!({"cmd": "cargo test --bin minicode"}),
        "running 5 tests\ntest test_add ... ok\ntest test_multiply ... ok\n\ntest result: ok. 5 passed; 0 failed; finished in 0.02s".to_string(),
        true,
        420,
    ));

    transcript.total_tokens = 2200;
    transcript.turns_executed = 2;

    let final_summary = "Implemented the requested math operations.\n- Added multiply function to math.rs\n- Verified all 5 tests pass successfully with 0 regressions\n- Code adheres to 2021 edition conventions";

    let report = SubagentSynthesisReducer::reduce(SubagentSynthesisContext {
        subagent_id: "tester-99",
        role: &SubagentRole::TestEngineer,
        final_summary,
        success: true,
        status: "completed",
        files_inspected: &["Cargo.toml".to_string()],
        files_modified: &["math.rs".to_string()],
        transcript: &transcript,
        workspace_root,
        worktree_branch: None,
    })
    .await;

    assert_eq!(report.subagent_id, "tester-99");
    assert!(report.success);
    assert_eq!(report.total_steps, 2);
    assert_eq!(report.verifications.len(), 1);
    assert_eq!(report.verifications[0].command, "cargo test --bin minicode");
    assert!(report.verifications[0].passed);
    assert!(report.verifications[0]
        .summary
        .contains("5 passed; 0 failed"));

    // Key findings should be extracted
    assert!(report.key_findings.len() >= 2);
    assert!(report.key_findings.iter().any(|f| f.contains("multiply")));

    // CCR Cache ID should be populated
    assert!(report.ccr_id.is_some());
    let ccr_id = report.ccr_id.as_ref().unwrap();
    let cached = CcrCache::retrieve(ccr_id, None, None);
    assert!(cached.is_some());

    // Markdown format validation
    let md = report.format_markdown();
    assert!(md.contains("✔ Subagent `[ID: tester-99 | Role: TestEngineer]` Completed Successfully"));
    assert!(md.contains("Executive Summary"));
    assert!(md.contains("math.rs"));
    assert!(md.contains("Verification & Command Execution"));
    assert!(md.contains("subagent_transcript_drilldown(subagent_id=\"tester-99\", step_index=N)"));
}

#[tokio::test]
async fn test_subagent_transcript_drilldown_tool_dispatch() {
    let dir = tempdir().unwrap();
    let workspace_root = dir.path();

    let mut transcript = SubagentTranscript::new(
        "reviewer-7",
        SubagentRole::CodeReviewer,
        "Review pull request changes".to_string(),
    );

    transcript.add_step(SubagentStepRecord::new(
        1,
        1,
        "read_file".to_string(),
        json!({"path": "src/main.rs"}),
        "fn main() { println!(\"hello\"); }".to_string(),
        true,
        8,
    ));

    transcript.add_step(SubagentStepRecord::new(
        2,
        2,
        "grep_search".to_string(),
        json!({"query": "TODO"}),
        "src/main.rs:12: // TODO: add graceful shutdown".to_string(),
        true,
        14,
    ));

    get_global_transcript_store().store(transcript, Some(workspace_root));

    // 1. Dispatch drilldown overview
    let overview_res = agent_tools::dispatch(
        "subagent_transcript_drilldown",
        &json!({"subagent_id": "reviewer-7"}),
        workspace_root,
    )
    .await
    .unwrap()
    .unwrap();

    assert!(overview_res.contains("reviewer-7"));
    assert!(overview_res.contains("`read_file`"));
    assert!(overview_res.contains("`grep_search`"));

    // 2. Dispatch drilldown specific step
    let step_res = agent_tools::dispatch(
        "subagent_transcript_drilldown",
        &json!({"subagent_id": "reviewer-7", "step_index": 2}),
        workspace_root,
    )
    .await
    .unwrap()
    .unwrap();

    assert!(step_res.contains("Step 2 Details (`grep_search`)"));
    assert!(step_res.contains("TODO: add graceful shutdown"));

    // 3. Dispatch via manage_subagents action="transcript"
    let manage_res = agent_tools::dispatch(
        "manage_subagents",
        &json!({"action": "transcript", "subagent_id": "reviewer-7"}),
        workspace_root,
    )
    .await
    .unwrap()
    .unwrap();

    assert!(manage_res.contains("reviewer-7"));
    assert!(manage_res.contains("`read_file`"));
}

#[tokio::test]
async fn test_ephemeral_isolation_token_reduction() {
    let dir = tempdir().unwrap();
    let workspace_root = dir.path();

    // Create a transcript simulating large tool responses (e.g. 500 lines of build logs)
    let mut large_log = String::new();
    for i in 0..100 {
        large_log.push_str(&format!(
            "Compiling crate-dep-{} v0.{}.0 (/path/to/target/debug/build/...)\n",
            i, i
        ));
    }
    large_log.push_str("test result: ok. 48 passed; 0 failed; finished in 1.4s\n");

    let mut transcript = SubagentTranscript::new(
        "worker-verbose",
        SubagentRole::Custom("CompilerRunner".to_string()),
        "Build and test entire workspace".to_string(),
    );

    transcript.add_step(SubagentStepRecord::new(
        1,
        1,
        "exec_cmd".to_string(),
        json!({"cmd": "cargo test --all"}),
        large_log.clone(),
        true,
        3400,
    ));

    let final_summary = "Build succeeded with 48 integration tests passing without warnings.";

    let role = SubagentRole::Custom("CompilerRunner".to_string());
    let report = SubagentSynthesisReducer::reduce(SubagentSynthesisContext {
        subagent_id: "worker-verbose",
        role: &role,
        final_summary,
        success: true,
        status: "completed",
        files_inspected: &[],
        files_modified: &[],
        transcript: &transcript,
        workspace_root,
        worktree_branch: None,
    })
    .await;

    let synthesized_md = report.format_markdown();

    // Raw trace size vs synthesized report size
    let raw_trace_len = large_log.len();
    let synthesized_len = synthesized_md.len();

    assert!(
        raw_trace_len > 4000,
        "Raw trace should be large ({} chars)",
        raw_trace_len
    );
    assert!(
        synthesized_len < 1000,
        "Synthesized executive report should be compact ({} chars)",
        synthesized_len
    );

    // Verify token reduction > 75%
    let reduction_pct = (1.0 - (synthesized_len as f64 / raw_trace_len as f64)) * 100.0;
    assert!(
        reduction_pct > 75.0,
        "Expected >75% reduction, got {:.1}%",
        reduction_pct
    );
}

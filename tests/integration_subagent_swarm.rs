/// Integration tests for Phase 33: Subagent Swarm Core Engine & Capability Sandboxing
///
/// Tests role presets, tool capability whitelisting, pool supervisor,
/// lifecycle cancellation, and tool schema registration.
use minicode::agent::subagent::{
    SubagentConfig, SubagentPool, SubagentRole, SubagentState, SubagentWorker,
};
use minicode::tools::registry::agent_tools;
use std::path::PathBuf;

#[test]
fn test_subagent_state_variants() {
    assert_eq!(SubagentState::Idle.as_str(), "idle");
    assert_eq!(SubagentState::Running.as_str(), "running");
    assert_eq!(SubagentState::Completed.as_str(), "completed");
    assert_eq!(SubagentState::Canceled.as_str(), "canceled");
    assert_eq!(SubagentState::Failed("err".to_string()).as_str(), "failed");
}

#[test]
fn test_subagent_role_presets_and_tool_whitelists() {
    let researcher = SubagentRole::Researcher;
    let reviewer = SubagentRole::CodeReviewer;
    let tester = SubagentRole::TestEngineer;
    let security = SubagentRole::SecurityAuditor;
    let custom = SubagentRole::Custom("specialist".to_string());

    // Researcher should have read-only tools, strictly NO write_file or patch_file
    let res_tools = researcher.default_tool_whitelist();
    assert!(res_tools.contains("read_file"));
    assert!(res_tools.contains("ripgrep_search"));
    assert!(res_tools.contains("fetch_or_browse"));
    assert!(!res_tools.contains("write_file"));
    assert!(!res_tools.contains("patch_file"));
    assert!(!res_tools.contains("exec_cmd"));

    // CodeReviewer has inspection and browser tools
    let rev_tools = reviewer.default_tool_whitelist();
    assert!(rev_tools.contains("read_file"));
    assert!(rev_tools.contains("ripgrep_search"));
    assert!(!rev_tools.contains("write_file"));

    // TestEngineer has exec_cmd for running test suites
    let test_tools = tester.default_tool_whitelist();
    assert!(test_tools.contains("read_file"));
    assert!(test_tools.contains("exec_cmd"));

    // SecurityAuditor has read & search tools
    let sec_tools = security.default_tool_whitelist();
    assert!(sec_tools.contains("read_file"));
    assert!(!sec_tools.contains("exec_cmd"));

    // Custom worker allows all standard tools
    let cust_tools = custom.default_tool_whitelist();
    assert!(cust_tools.contains("read_file"));
    assert!(cust_tools.contains("write_file"));
    assert!(cust_tools.contains("exec_cmd"));
}

#[test]
fn test_subagent_config_and_token_budgets() {
    let config_res = SubagentConfig::for_role(SubagentRole::Researcher);
    assert_eq!(config_res.token_budget, 24_000);
    assert_eq!(config_res.max_turns, 12);

    let config_rev = SubagentConfig::for_role(SubagentRole::CodeReviewer);
    assert_eq!(config_rev.token_budget, 16_000);
    assert_eq!(config_rev.max_turns, 6);

    let config_test = SubagentConfig::for_role(SubagentRole::TestEngineer);
    assert_eq!(config_test.token_budget, 32_000);
    assert_eq!(config_test.max_turns, 10);
}

#[test]
fn test_subagent_worker_system_prompt_builder() {
    let ws = PathBuf::from("/tmp/minicode_test_ws");
    let worker_res = SubagentWorker::new(
        "res-1".to_string(),
        "Find all auth endpoints".to_string(),
        SubagentConfig::for_role(SubagentRole::Researcher),
        &ws,
    );
    let prompt_res = worker_res.build_system_prompt();
    assert!(prompt_res.contains("Research Subagent"));
    assert!(prompt_res.contains("READ-ONLY"));

    let worker_sec = SubagentWorker::new(
        "sec-1".to_string(),
        "Audit JWT token handling".to_string(),
        SubagentConfig::for_role(SubagentRole::SecurityAuditor),
        &ws,
    );
    let prompt_sec = worker_sec.build_system_prompt();
    assert!(prompt_sec.contains("Security Auditor"));
}

#[tokio::test]
async fn test_subagent_pool_lifecycle_and_summary() {
    let ws = PathBuf::from("/tmp/minicode_test_pool");
    let pool = SubagentPool::new(&ws);

    let id1 = pool.next_id(&SubagentRole::Researcher).await;
    let id2 = pool.next_id(&SubagentRole::CodeReviewer).await;

    assert_eq!(id1, "researcher-1");
    assert_eq!(id2, "codereviewer-2");

    let summary = pool.format_swarm_summary().await;
    assert!(
        summary.contains("No subagent workers have been spawned")
            || summary.contains("Subagent Swarm Status")
    );
}

#[test]
fn test_subagent_tools_schema_registration() {
    let schemas = agent_tools::get_schemas();
    let names: Vec<String> = schemas.into_iter().map(|s| s.name).collect();

    assert!(names.contains(&"invoke_subagent".to_string()));
    assert!(names.contains(&"send_message".to_string()));
    assert!(names.contains(&"manage_subagents".to_string()));
}

//! Integration tests for Dev Server Port Lifecycle, Persistent GUI Browser Retention,
//! Pre-flight TCP Socket Readiness Barrier, and Anti-Batching Plan Progression Barrier.

use minicode::agent::plan_gate::is_verifiable_action_tool;
use minicode::context::memory::working_memory::{TaskItemStatus, WorkingMemory};
use minicode::dev::models::{DevProcessType, SpawnDevRequest};
use minicode::dev::registry::get_global_dev_registry;
use minicode::tools::browser::manager::BrowserManager;
use minicode::tools::browser::{reconcile_loopback_url, wait_for_port_ready};
use std::collections::HashMap;
use std::time::Duration;
use tempfile::tempdir;
use tokio::net::TcpListener;

#[tokio::test]
async fn test_reconcile_loopback_url_passthrough_for_external() {
    // External URLs must never be rewritten or modified
    let (url1, notice1) = reconcile_loopback_url("https://example.com:5173/dashboard").await;
    assert_eq!(url1, "https://example.com:5173/dashboard");
    assert!(notice1.is_none());

    let (url2, notice2) = reconcile_loopback_url("http://192.168.1.50:5173").await;
    assert_eq!(url2, "http://192.168.1.50:5173");
    assert!(notice2.is_none());
}

#[tokio::test]
async fn test_reconcile_loopback_url_without_active_dev_process() {
    // When no dev process exists on another port, loopback URL passes through normalized
    let (url, notice) = reconcile_loopback_url("http://localhost:5173").await;
    assert_eq!(url, "http://127.0.0.1:5173/");
    assert!(notice.is_none());

    let (url_ip, notice_ip) = reconcile_loopback_url("http://127.0.0.1:3000/app").await;
    assert_eq!(url_ip, "http://127.0.0.1:3000/app");
    assert!(notice_ip.is_none());
}

#[tokio::test]
async fn test_reconcile_loopback_url_with_active_dev_server() {
    let temp = tempdir().expect("tempdir");
    let registry = get_global_dev_registry();

    // Bind a real listener on an ephemeral port to simulate an active dev server
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind listener");
    let real_port = listener.local_addr().expect("local addr").port();

    // Register a dev process with port_hint / real_port and Ignore policy so port isn't shifted
    let req = SpawnDevRequest {
        command: "sh -c 'sleep 30'".to_string(),
        name: Some("test-vite-server".to_string()),
        process_type: DevProcessType::Frontend,
        working_dir: None,
        extra_env: HashMap::new(),
        port_hint: Some(real_port),
        max_memory_mb: None,
        port_policy: Some(minicode::dev::models::PortConflictPolicy::Ignore),
        restart_policy: None,
    };

    let summary = registry
        .spawn(temp.path(), req)
        .await
        .expect("spawn dev process");

    // Now test navigating to port 5173 when real dev server is on real_port
    // (Only test if real_port is distinct from 5173)
    match real_port != 5173 {
        true => {
            let (reconciled, notice) = reconcile_loopback_url("http://localhost:5173/demo").await;
            assert_eq!(reconciled, format!("http://127.0.0.1:{}/demo", real_port));
            assert!(notice.is_some());
            let notice_text = notice.expect("notice must exist");
            assert!(notice_text.contains(&real_port.to_string()));
        }
        false => {}
    }

    // Clean up
    registry.stop(&summary.id).await.expect("stop process");
}

#[tokio::test]
async fn test_tcp_socket_readiness_barrier_listener_and_timeout() {
    // 1. Test ready port detection: bind local TCP listener
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind local tcp");
    let port = listener.local_addr().expect("local addr").port();

    let ready = wait_for_port_ready(port, Duration::from_millis(1500)).await;
    assert!(ready, "Port with active TCP listener must report ready");

    // Drop listener so port closes
    drop(listener);

    // 2. Test timeout detection on an unused/closed port
    // Pick an unlikely high ephemeral port
    let timeout_detected = wait_for_port_ready(59999, Duration::from_millis(150)).await;
    assert!(
        !timeout_detected,
        "Unbound port must report false after timeout"
    );
}

#[tokio::test]
async fn test_persistent_gui_browser_retention_defaults() {
    // Initial state: no live engine running
    assert_eq!(BrowserManager::live_engine_mode().await, None);

    // Calling shutdown without force when no engine is running succeeds cleanly
    let res = BrowserManager::shutdown_live_engine_opt(false).await;
    assert!(res.is_ok());
    assert_eq!(res.unwrap(), false);

    // Calling shutdown with force when no engine is running also succeeds cleanly
    let res_force = BrowserManager::shutdown_live_engine_opt(true).await;
    assert!(res_force.is_ok());
    assert_eq!(res_force.unwrap(), false);
}

#[tokio::test]
async fn test_plan_progression_barrier_bulk_completion_rejection() {
    let temp = tempdir().expect("tempdir");
    let wm = WorkingMemory::new(temp.path());

    let steps = vec![
        "Step 1: Project Scaffolding".to_string(),
        "Step 2: Core Components".to_string(),
        "Step 3: Business Logic".to_string(),
        "Step 4: End-to-End Verification".to_string(),
    ];
    wm.init_plan("Test Anti-Batching Plan", &steps)
        .expect("init plan");

    let initial_tasks = wm.read_parsed_tasks();
    assert_eq!(initial_tasks.len(), 4);
    assert_eq!(initial_tasks[0].status, TaskItemStatus::InProgress);
    assert_eq!(initial_tasks[1].status, TaskItemStatus::Pending);

    // Attempting bulk completion with "all" MUST be rejected
    let res_all = wm.update_progress("all", "completed");
    assert!(
        res_all.is_err(),
        "update_progress('all', 'completed') must be rejected"
    );

    // Attempting bulk completion with "remaining" MUST be rejected
    let res_rem = wm.update_progress("remaining", "completed");
    assert!(
        res_rem.is_err(),
        "update_progress('remaining', 'completed') must be rejected"
    );

    // Verify tasks are still in their original state
    let tasks_after_cheats = wm.read_parsed_tasks();
    assert_eq!(tasks_after_cheats[0].status, TaskItemStatus::InProgress);
    assert_eq!(tasks_after_cheats[1].status, TaskItemStatus::Pending);

    // Atomic sequential completion must succeed step by step
    let step1 = wm.update_progress("1", "completed");
    assert!(step1.is_ok());
    let tasks_step1 = wm.read_parsed_tasks();
    assert_eq!(tasks_step1[0].status, TaskItemStatus::Completed);
    assert_eq!(tasks_step1[1].status, TaskItemStatus::InProgress);
    assert_eq!(tasks_step1[2].status, TaskItemStatus::Pending);

    let step2 = wm.update_progress("2", "completed");
    assert!(step2.is_ok());
    let tasks_step2 = wm.read_parsed_tasks();
    assert_eq!(tasks_step2[1].status, TaskItemStatus::Completed);
    assert_eq!(tasks_step2[2].status, TaskItemStatus::InProgress);
    assert_eq!(tasks_step2[3].status, TaskItemStatus::Pending);
}

#[test]
fn test_verifiable_action_tool_classification() {
    // Mutation tools
    assert!(is_verifiable_action_tool("write_file"));
    assert!(is_verifiable_action_tool("patch_file"));
    assert!(is_verifiable_action_tool("replace_in_files"));
    assert!(is_verifiable_action_tool("kit_stack_add"));
    assert!(is_verifiable_action_tool("block_scaffold"));
    assert!(is_verifiable_action_tool("block_insert"));

    // Execution tools
    assert!(is_verifiable_action_tool("exec_cmd"));
    assert!(is_verifiable_action_tool("minitask"));
    assert!(is_verifiable_action_tool("git_commit"));

    // Browser verification tools
    assert!(is_verifiable_action_tool("browser_navigate"));
    assert!(is_verifiable_action_tool("browser_screenshot"));
    assert!(is_verifiable_action_tool("browser_debug_logs"));
    assert!(is_verifiable_action_tool("browser_eval"));
    assert!(is_verifiable_action_tool("browser_click"));

    // Read-only or metadata tools MUST NOT qualify as verifiable actions
    assert!(!is_verifiable_action_tool("read_file"));
    assert!(!is_verifiable_action_tool("grep_search"));
    assert!(!is_verifiable_action_tool("locate_symbol"));
    assert!(!is_verifiable_action_tool("ask_user"));
    assert!(!is_verifiable_action_tool("update_progress"));
    assert!(!is_verifiable_action_tool("create_plan"));
}

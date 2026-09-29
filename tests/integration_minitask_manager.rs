//! Integration test suite validating MiniTask Manager & Process Supervisor features:
//! 1. Auto-detection & migration of python3 webservers and daemons in exec_cmd.
//! 2. Explicit is_daemon background task execution.
//! 3. Proactive Resource Watchdog terminating runaway / OOM memory spikes.
//! 4. Unified minitask_manager tool dispatch and alias execution.

use minicode::dev::models::{DevProcessStatus, DevProcessType, SpawnDevRequest};
use minicode::dev::ports::is_process_running;
use minicode::dev::registry::get_global_dev_registry;
use minicode::tools::ToolRegistry;
use serde_json::json;
use std::collections::HashMap;
use std::time::Duration;
use tempfile::tempdir;

#[tokio::test]
async fn test_exec_cmd_auto_detects_python_http_server() {
    let temp = tempdir().expect("tempdir");
    let registry = get_global_dev_registry();

    // Execute python3 -m http.server 9871 via exec_cmd
    let res = ToolRegistry::dispatch(
        temp.path(),
        "call_exec_cmd_webserver_1",
        "exec_cmd",
        &json!({
            "command": "python3 -m http.server 9871"
        }),
        None,
        1,
    )
    .await;

    println!(
        "exec_cmd result success: {}, output:\n{}",
        res.success, res.output
    );
    assert!(res.success, "exec_cmd should succeed for python webserver");
    assert!(
        res.output.contains("MiniTask Manager"),
        "Output should indicate migration to MiniTask Manager"
    );
    assert!(
        res.output.contains("http://localhost:"),
        "Output should report the server URL: got {}",
        res.output
    );

    // Verify it is registered and running in MiniDevRegistry
    let list = registry.list().await;
    let server_task = list.iter().find(|p| p.name.contains("python-webserver"));

    assert!(server_task.is_some(), "Server task must exist in registry");
    let task = server_task.unwrap();
    let pid = task.pid.expect("PID must exist");
    assert!(
        is_process_running(pid),
        "Python webserver PID {} must be running",
        pid
    );

    // Stop server cleanly
    let stop_res = registry.stop(&task.id).await;
    assert!(stop_res.is_ok(), "Stop should succeed");

    tokio::time::sleep(Duration::from_millis(150)).await;
    assert!(
        !is_process_running(pid),
        "Python server PID {} must be dead after stop",
        pid
    );
}

#[tokio::test]
async fn test_exec_cmd_is_daemon_background_flag() {
    let temp = tempdir().expect("tempdir");
    let registry = get_global_dev_registry();

    // Execute a non-server command with explicit is_daemon: true
    let res = ToolRegistry::dispatch(
        temp.path(),
        "call_exec_cmd_daemon_1",
        "exec_cmd",
        &json!({
            "command": "sleep 120",
            "is_daemon": true
        }),
        None,
        1,
    )
    .await;

    assert!(res.success, "exec_cmd with is_daemon: true should succeed");
    assert!(
        res.output.contains("MiniTask Manager"),
        "Output should report registration in MiniTask Manager"
    );

    let list = registry.list().await;
    let bg_task = list.iter().find(|p| p.name.contains("bg-task"));
    assert!(bg_task.is_some(), "Background task must be registered");

    let task = bg_task.unwrap();
    let pid = task.pid.expect("PID must exist");
    assert!(
        is_process_running(pid),
        "Background sleep PID {} must be running",
        pid
    );

    // Stop the task
    let _ = registry.stop(&task.id).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(
        !is_process_running(pid),
        "Sleep PID {} must be stopped",
        pid
    );
}

#[tokio::test]
async fn test_resource_watchdog_detects_oom_memory_spike() {
    let temp = tempdir().expect("tempdir");
    let registry = get_global_dev_registry();

    // Spawn a process with a strict 2 MB ceiling that allocates 25 MB
    let req = SpawnDevRequest {
        command: "python3 -c \"import time; x = 'A' * 25_000_000; time.sleep(10)\"".to_string(),
        name: Some("test-oom-spike".to_string()),
        process_type: DevProcessType::Script,
        working_dir: None,
        extra_env: HashMap::new(),
        port_hint: None,
        max_memory_mb: Some(2), // 2 MB limit!
        port_policy: None,
        restart_policy: None,
    };

    let summary = registry.spawn(temp.path(), req).await.expect("spawn");
    let pid = summary.pid.expect("PID must exist");

    // Poll up to 8 seconds for the 2-second Resource Watchdog tick to detect RAM spike and terminate
    let mut terminated = false;
    for _ in 0..16 {
        tokio::time::sleep(Duration::from_millis(500)).await;
        if !is_process_running(pid) {
            terminated = true;
            break;
        }
    }

    assert!(
        terminated,
        "Runaway process PID {} must have been terminated by watchdog",
        pid
    );

    // Check updated status: should have been caught and killed by Resource Watchdog
    let updated = registry.get(&summary.id).await;
    assert!(updated.is_some(), "Process summary must exist");

    let status = updated.unwrap().status;
    println!("Updated status after OOM watchdog check: {:?}", status);

    match status {
        DevProcessStatus::Degraded(msg) => {
            assert!(
                msg.contains("OOM") || msg.contains("exceeded"),
                "Degraded message should cite OOM: {}",
                msg
            );
        }
        DevProcessStatus::Stopped | DevProcessStatus::Killed | DevProcessStatus::Exited(_) => {
            // Terminated cleanly
        }
        _ => panic!(
            "Expected process to be terminated, got status: {:?}",
            status
        ),
    }
}

#[tokio::test]
async fn test_minitask_manager_tool_dispatch_alias() {
    let temp = tempdir().expect("tempdir");

    // Dispatching via alias 'minitask_manager'
    let res = ToolRegistry::dispatch(
        temp.path(),
        "call_minitask_mgr_list",
        "minitask_manager",
        &json!({
            "action": "list"
        }),
        None,
        1,
    )
    .await;

    assert!(
        res.success,
        "minitask_manager alias should dispatch successfully"
    );
    assert!(
        res.output.contains("Processes") || res.output.contains("processes"),
        "List output should mention processes: got {}",
        res.output
    );

    // Dispatching via alias 'minitask'
    let res_task = ToolRegistry::dispatch(
        temp.path(),
        "call_minitask_list",
        "minitask",
        &json!({
            "action": "list"
        }),
        None,
        1,
    )
    .await;

    assert!(
        res_task.success,
        "minitask alias should dispatch successfully"
    );
}

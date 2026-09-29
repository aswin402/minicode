//! End-to-end integration test suite verifying zero-orphan cleanup guarantees
//! across development servers, external apps, background processes, and browser sessions.

use minicode::constants::TOTAL_TOOL_COUNT;
use minicode::dev::models::{DevProcessType, SpawnDevRequest};
use minicode::dev::ports::{find_all_descendants, is_process_running};
use minicode::dev::registry::{get_global_dev_registry, kill_all_sync};
use minicode::tools::ToolRegistry;
use serde_json::json;
use std::collections::HashMap;
use std::time::Duration;
use tempfile::tempdir;

#[tokio::test]
async fn test_tool_registry_includes_browser_close() {
    let schemas = ToolRegistry::get_tool_schemas();
    assert_eq!(schemas.len(), TOTAL_TOOL_COUNT);
    let close_schema = schemas.iter().find(|s| s.name == "browser_close");
    assert!(
        close_schema.is_some(),
        "browser_close tool schema must be registered"
    );
}

#[tokio::test]
async fn test_process_tree_descendant_zero_orphan_termination() {
    let temp = tempdir().expect("tempdir");
    let registry = get_global_dev_registry();

    // Spawn a parent process that forks background sub-processes
    let req = SpawnDevRequest {
        command: "sh -c 'sleep 60 & sleep 60'".to_string(),
        name: Some("test-tree-spawn".to_string()),
        process_type: DevProcessType::Script,
        working_dir: None,
        extra_env: HashMap::new(),
        port_hint: None,
        max_memory_mb: None,
        port_policy: None,
        restart_policy: None,
    };

    let summary = registry.spawn(temp.path(), req).await.expect("spawn");
    let pid = summary.pid.expect("pid must exist");

    // Wait brief moment for processes to be running
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Discover descendants via procfs
    let descendants = find_all_descendants(pid);
    println!("Spawned root PID: {}, Descendants: {:?}", pid, descendants);

    // Terminate root process group
    registry.stop(&summary.id).await.expect("stop");

    tokio::time::sleep(Duration::from_millis(200)).await;

    // Assert root PID is dead
    #[cfg(unix)]
    unsafe {
        let res = libc::kill(pid as i32, 0);
        assert_eq!(res, -1, "Root process {} must be dead", pid);

        // Assert all discovered descendants are also dead (Zero Orphan Guarantee)
        for &desc_pid in &descendants {
            let desc_res = libc::kill(desc_pid as i32, 0);
            assert_eq!(desc_res, -1, "Descendant process {} must be dead", desc_pid);
        }
    }
}

#[tokio::test]
async fn test_browser_close_tool_dispatch() {
    let temp = tempdir().expect("tempdir");

    // Dispatching browser_close when no browser is running returns clean status
    let res = ToolRegistry::dispatch(
        temp.path(),
        "call_browser_close_1",
        "browser_close",
        &json!({}),
        None,
        1,
    )
    .await;

    assert!(res.success);
    assert!(
        res.output.contains("No active browser session")
            || res.output.contains("Browser closed successfully")
    );
}

#[tokio::test]
async fn test_minitask_browser_stop_dispatch() {
    let temp = tempdir().expect("tempdir");

    // Dispatching minitask stop with id="browser" cleanly stops browser without error
    let res = ToolRegistry::dispatch(
        temp.path(),
        "call_minitask_stop_browser",
        "minitask",
        &json!({
            "action": "stop",
            "id": "browser"
        }),
        None,
        1,
    )
    .await;

    // Should report either stopped or not found (if not running)
    assert!(res.success || res.output.contains("not found"));
}

#[tokio::test]
async fn test_kill_all_sync_zero_orphan_guarantee() {
    let registry = get_global_dev_registry();

    // Register a simulated background PID
    let _dummy_child = std::process::Command::new("sleep")
        .arg("60")
        .spawn()
        .expect("spawn sleep");
    let dummy_pid = _dummy_child.id();

    registry.register_external_pid(dummy_pid);

    // Call kill_all_sync
    kill_all_sync();

    tokio::time::sleep(Duration::from_millis(150)).await;

    // Assert process is no longer actively running
    assert!(
        !is_process_running(dummy_pid),
        "External PID {} must not be running after kill_all_sync",
        dummy_pid
    );

    // Child process was killed and reaped by kill_all_sync's waitpid sweep
    #[cfg(unix)]
    unsafe {
        let res = libc::kill(dummy_pid as i32, 0);
        assert_eq!(
            res, -1,
            "External PID {} must be completely reaped from OS after kill_all_sync",
            dummy_pid
        );
    }
}

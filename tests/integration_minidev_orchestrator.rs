//! End-to-end integration test suite for MiniDev Runtime Orchestrator (Tool 168).
//! Verifies process group isolation, dynamic port discovery, zero-orphan cleanup,
//! resource telemetry, and ToolRegistry dispatch.

use minicode::constants::TOTAL_TOOL_COUNT;
use minicode::dev::models::{DevProcessType, SpawnDevRequest};
use minicode::dev::registry::get_global_dev_registry;
use minicode::tools::ToolRegistry;
use serde_json::json;
use std::collections::HashMap;
use std::time::Duration;
use tempfile::tempdir;

#[tokio::test]
async fn test_tool_registry_includes_mini_dev() {
    let schemas = ToolRegistry::get_tool_schemas();
    assert_eq!(schemas.len(), TOTAL_TOOL_COUNT);
    let dev_schema = schemas.iter().find(|s| s.name == "mini_dev");
    assert!(
        dev_schema.is_some(),
        "mini_dev tool schema must be registered"
    );
}

#[tokio::test]
async fn test_minidev_process_lifecycle_and_zero_orphan() {
    let temp = tempdir().expect("tempdir");
    let registry = get_global_dev_registry();

    // 1. Spawn a background process tree
    let req = SpawnDevRequest {
        command: "echo 'Vite v5.4.0 ready in 120 ms'; echo '➜  Local:   http://localhost:5199/'; sleep 60".to_string(),
        name: Some("integration-vite".to_string()),
        process_type: DevProcessType::Frontend,
        working_dir: None,
        extra_env: HashMap::new(),
        port_hint: Some(5199),
        max_memory_mb: None,
    };

    let summary = registry
        .spawn(temp.path(), req)
        .await
        .expect("spawn failed");
    assert_eq!(summary.name, "integration-vite");
    assert_eq!(summary.process_type, DevProcessType::Frontend);

    let pid = summary.pid.expect("pid must exist");

    // Wait for stdout processing
    tokio::time::sleep(Duration::from_millis(300)).await;

    // 2. Assert port discovery
    let current = registry.get(&summary.id).await.expect("must exist");
    assert_eq!(current.ports, vec![5199]);
    assert_eq!(current.url, Some("http://localhost:5199".to_string()));

    // 3. Assert logs ring buffer
    let logs = registry.logs(&summary.id, 10, None).await.expect("logs");
    assert!(logs
        .iter()
        .any(|l| l.contains("Local:   http://localhost:5199/")));

    // 4. Assert telemetry
    let resources = registry.resources().await;
    assert!(resources.total_active_processes >= 1);

    // 5. Terminate and verify zero orphan
    registry.stop(&summary.id).await.expect("stop failed");

    tokio::time::sleep(Duration::from_millis(200)).await;

    // Verify process is dead via kill(pid, 0)
    #[cfg(unix)]
    unsafe {
        let res = libc::kill(pid as i32, 0);
        // res == -1 means process does not exist or ESRCH
        assert_eq!(res, -1, "Process {} should no longer be running", pid);
    }
}

#[tokio::test]
async fn test_tool_dispatch_mini_dev_crud() {
    let temp = tempdir().expect("tempdir");

    // 1. Dispatch mini_dev start
    let start_args = json!({
        "action": "start",
        "command": "echo 'API server listening on 127.0.0.1:9099'; sleep 45",
        "name": "integration-api",
        "process_type": "backend"
    });

    let res = ToolRegistry::dispatch(temp.path(), "call_1", "mini_dev", &start_args, None, 1).await;

    assert!(res.success);
    let output = &res.output;
    assert!(output.contains("Development process launched successfully"));

    tokio::time::sleep(Duration::from_millis(300)).await;

    // 2. Dispatch mini_dev list
    let list_args = json!({ "action": "list" });
    let list_res =
        ToolRegistry::dispatch(temp.path(), "call_2", "mini_dev", &list_args, None, 2).await;

    assert!(list_res.success);
    let list_out = &list_res.output;
    assert!(list_out.contains("integration-api"));

    // 3. Dispatch mini_dev resources
    let res_args = json!({ "action": "resources" });
    let res_eval =
        ToolRegistry::dispatch(temp.path(), "call_3", "mini_dev", &res_args, None, 3).await;

    assert!(res_eval.success);
    let res_out = &res_eval.output;
    assert!(res_out.contains("Runtime Resource Telemetry"));

    // 4. Dispatch mini_dev kill_all
    let kill_args = json!({ "action": "kill_all" });
    let kill_res =
        ToolRegistry::dispatch(temp.path(), "call_4", "mini_dev", &kill_args, None, 4).await;

    assert!(kill_res.success);
    let kill_out = &kill_res.output;
    assert!(kill_out.contains("Terminated"));
}

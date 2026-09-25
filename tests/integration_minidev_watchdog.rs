//! End-to-end integration tests for MiniDev Port Conflict Arbitrator and Auto-Restart Watchdog.

use minicode::dev::models::{
    DevProcessStatus, DevProcessType, PortConflictPolicy, PortResolution, RestartPolicy,
    SpawnDevRequest,
};
use minicode::dev::registry::get_global_dev_registry;
use minicode::tools::registry::dev_tools::dispatch;
use serde_json::json;
use std::collections::HashMap;
use std::net::TcpListener;
use std::time::Duration;
use tempfile::tempdir;

#[tokio::test]
async fn test_port_conflict_fallback_auto_shifting() {
    let temp = tempdir().expect("tempdir");
    let registry = get_global_dev_registry();

    // 1. Bind an artificial port to force conflict
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind listener");
    let occupied_port = listener.local_addr().expect("local_addr").port();

    // 2. Request a process that targets the occupied port with Fallback policy
    let req = SpawnDevRequest {
        command: "echo 'Server ready'; sleep 60".to_string(),
        name: Some("test-auto-shift".to_string()),
        process_type: DevProcessType::Backend,
        working_dir: None,
        extra_env: HashMap::new(),
        port_hint: Some(occupied_port),
        max_memory_mb: None,
        port_policy: Some(PortConflictPolicy::Fallback),
        restart_policy: None,
    };

    let summary = registry
        .spawn(temp.path(), req)
        .await
        .expect("spawn should succeed via auto-shift");

    // 3. Verify that the conflict was detected and shifted to an available port
    assert_eq!(summary.name, "test-auto-shift");
    match summary.port_resolution {
        Some(PortResolution::Shifted {
            requested,
            resolved,
            ref conflict,
        }) => {
            assert_eq!(requested, occupied_port);
            assert_ne!(resolved, occupied_port);
            assert_eq!(conflict.port, occupied_port);
            assert!(summary.ports.contains(&resolved));
        }
        other => panic!("Expected PortResolution::Shifted, got {:?}", other),
    }

    // 4. Verify ring buffer recorded the shift notice
    let logs = registry.logs(&summary.id, 20, None).await.expect("logs");
    assert!(logs.iter().any(|l| l.contains("[PORT_ARBITRATOR]")));

    // 5. Clean up
    let _ = registry.stop(&summary.id).await;
}

#[tokio::test]
async fn test_port_conflict_strict_error_policy() {
    let temp = tempdir().expect("tempdir");
    let registry = get_global_dev_registry();

    // 1. Bind an artificial port to force conflict
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind listener");
    let occupied_port = listener.local_addr().expect("local_addr").port();

    // 2. Request a process that targets the occupied port with Error policy
    let req = SpawnDevRequest {
        command: "echo 'Strict mode'; sleep 60".to_string(),
        name: Some("test-strict-err".to_string()),
        process_type: DevProcessType::Backend,
        working_dir: None,
        extra_env: HashMap::new(),
        port_hint: Some(occupied_port),
        max_memory_mb: None,
        port_policy: Some(PortConflictPolicy::Error),
        restart_policy: None,
    };

    let result = registry.spawn(temp.path(), req).await;
    assert!(result.is_err(), "Expected PortConflict error");
    let err_str = result.err().unwrap().to_string();
    assert!(
        err_str.contains("Port conflict on port") || err_str.contains(&occupied_port.to_string())
    );
}

#[tokio::test]
async fn test_watchdog_auto_restart_supervisor() {
    let temp = tempdir().expect("tempdir");
    let registry = get_global_dev_registry();

    // 1. Spawn a process configured with OnFailure restart policy
    let req = SpawnDevRequest {
        command: "echo 'Worker loop tick'; exit 42".to_string(),
        name: Some("test-watchdog-restart".to_string()),
        process_type: DevProcessType::Script,
        working_dir: None,
        extra_env: HashMap::new(),
        port_hint: None,
        max_memory_mb: None,
        port_policy: None,
        restart_policy: Some(RestartPolicy::OnFailure {
            max_retries: 2,
            backoff_ms: 60,
        }),
    };

    let summary = registry
        .spawn(temp.path(), req)
        .await
        .expect("spawn succeeded");

    // 2. Allow supervisor loop to observe crash and trigger auto-restart
    tokio::time::sleep(Duration::from_millis(500)).await;

    // 3. Inspect logs for watchdog records
    let logs = registry.logs(&summary.id, 50, None).await.expect("logs");
    assert!(logs.iter().any(|l| l.contains("Worker loop tick")));
    assert!(logs.iter().any(|l| l.contains("[WATCHDOG]")));

    // 4. Verify summary reflects restart policy
    let updated = registry.get(&summary.id).await.expect("process exists");
    assert_eq!(
        updated.restart_policy,
        RestartPolicy::OnFailure {
            max_retries: 2,
            backoff_ms: 60,
        }
    );

    // 5. Clean up
    let _ = registry.stop(&summary.id).await;
}

#[tokio::test]
async fn test_watchdog_crash_loop_protection() {
    let temp = tempdir().expect("tempdir");
    let registry = get_global_dev_registry();

    // Spawn an instantly crashing process
    let req = SpawnDevRequest {
        command: "exit 99".to_string(),
        name: Some("test-crash-loop".to_string()),
        process_type: DevProcessType::Script,
        working_dir: None,
        extra_env: HashMap::new(),
        port_hint: None,
        max_memory_mb: None,
        port_policy: None,
        restart_policy: Some(RestartPolicy::Always {
            max_retries: 10,
            backoff_ms: 20,
        }),
    };

    let summary = registry
        .spawn(temp.path(), req)
        .await
        .expect("spawn succeeded");

    // Wait for consecutive fast crashes to trigger crash loop barrier
    tokio::time::sleep(Duration::from_millis(600)).await;

    let updated = registry.get(&summary.id).await.expect("process exists");
    let logs = registry.logs(&summary.id, 30, None).await.expect("logs");

    // Must be either degraded with crash loop detected, or stopped/exited
    assert!(
        logs.iter()
            .any(|l| l.contains("[WATCHDOG] Crash loop detected"))
            || matches!(updated.status, DevProcessStatus::Degraded(_))
    );

    // Clean up
    let _ = registry.stop(&summary.id).await;
}

#[tokio::test]
async fn test_mini_dev_tool_probe_port_and_auto_restart() {
    let temp = tempdir().expect("tempdir");

    // 1. Probe a bound port via tool
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind listener");
    let bound_port = listener.local_addr().expect("addr").port();

    let probe_args = json!({
        "action": "probe_port",
        "port": bound_port
    });
    let probe_out = dispatch("mini_dev", &probe_args, temp.path())
        .await
        .expect("dispatch")
        .expect("probe result");
    assert!(probe_out.contains("OCCUPIED"));
    assert!(probe_out.contains("Next Available Port:"));

    // 2. Start process with auto_restart toggle
    let start_args = json!({
        "action": "start",
        "command": "echo 'Service online'; sleep 30",
        "name": "auto-restart-service",
        "auto_restart": true
    });
    let start_out = dispatch("mini_dev", &start_args, temp.path())
        .await
        .expect("dispatch")
        .expect("start result");
    assert!(start_out.contains("Development process launched successfully"));
    assert!(start_out.contains("Watchdog Supervision"));

    // 3. Kill all
    let kill_args = json!({ "action": "kill_all" });
    let kill_out = dispatch("mini_dev", &kill_args, temp.path())
        .await
        .expect("dispatch")
        .expect("kill result");
    assert!(kill_out.contains("Terminated"));
}

use minicode::sandbox::{run_sandboxed, SandboxPolicy};
use serde_json::json;
use std::collections::HashMap;

#[tokio::test]
async fn test_sandbox_exec_echo_basic() {
    let temp_dir = tempfile::tempdir().unwrap();
    let policy = SandboxPolicy::default();

    let res = run_sandboxed(
        temp_dir.path(),
        "echo 'minicode sandbox execution'",
        &policy,
    )
    .await
    .unwrap();

    assert!(res.success);
    assert!(res.combined_output.contains("minicode sandbox execution"));
    assert!(res.network_isolated);
    assert_eq!(res.exit_code, Some(0));
}

#[tokio::test]
async fn test_sandbox_network_isolation() {
    let temp_dir = tempfile::tempdir().unwrap();
    let mut policy = SandboxPolicy::default();
    policy.allow_network = false;

    // Test loopback vs external isolation
    // In bubblewrap/unshared network, only loopback exists or network operations fail immediately
    let res = run_sandboxed(
        temp_dir.path(),
        "ip route 2>&1 || echo 'no_routes'",
        &policy,
    )
    .await
    .unwrap();

    assert!(res.success);
    // In unshared net, ip route has 0 default gateways
    assert!(!res.combined_output.contains("default via"));
}

#[tokio::test]
async fn test_sandbox_read_only_protection() {
    let temp_dir = tempfile::tempdir().unwrap();
    let mut policy = SandboxPolicy::default();
    policy.read_only_workspace = true;

    let target_file = temp_dir.path().join("immutable_probe.txt");
    let cmd = format!("echo 'hacked' > {}", target_file.display());

    let res = run_sandboxed(temp_dir.path(), &cmd, &policy).await;

    // File should never exist in host filesystem
    assert!(!target_file.exists());
    if let Ok(r) = res {
        assert!(!r.success || !target_file.exists());
    }
}

#[tokio::test]
async fn test_sandbox_ephemeral_overlay() {
    let temp_dir = tempfile::tempdir().unwrap();
    let mut policy = SandboxPolicy::default();
    policy.ephemeral_overlay = true;

    let res = run_sandboxed(
        temp_dir.path(),
        "echo 'scratch data' > ephemeral_artifact.log",
        &policy,
    )
    .await
    .unwrap();

    assert!(res.success);
    assert!(res.ephemeral_writes_discarded);
    assert!(res
        .discarded_files
        .contains(&"ephemeral_artifact.log".to_string()));

    // Verify file did NOT persist to host workspace
    assert!(!temp_dir.path().join("ephemeral_artifact.log").exists());
}

#[tokio::test]
async fn test_sandbox_timeout_kill() {
    let temp_dir = tempfile::tempdir().unwrap();
    let mut policy = SandboxPolicy::default();
    policy.timeout_secs = 1;

    let res = run_sandboxed(temp_dir.path(), "sleep 5", &policy).await;
    assert!(
        res.is_err(),
        "Expected timeout error for long-running process"
    );
}

#[tokio::test]
async fn test_sandbox_custom_env() {
    let temp_dir = tempfile::tempdir().unwrap();
    let mut policy = SandboxPolicy::default();
    let mut extra = HashMap::new();
    extra.insert(
        "SECRET_CANARY".to_string(),
        "CANARY_VALUE_12345".to_string(),
    );
    policy.extra_env = extra;

    let res = run_sandboxed(temp_dir.path(), "echo canary: $SECRET_CANARY", &policy)
        .await
        .unwrap();

    assert!(res.success);
    assert!(res.combined_output.contains("canary: CANARY_VALUE_12345"));
}

#[tokio::test]
async fn test_tool_registry_sandbox_exec_dispatch() {
    let temp_dir = tempfile::tempdir().unwrap();
    let args = json!({
        "command": "echo 'registry dispatch test'",
        "allow_network": false,
        "read_only": true,
        "timeout_secs": 10
    });

    let opt_res =
        minicode::tools::registry::exec_tools::dispatch("sandbox_exec", &args, temp_dir.path())
            .await;
    assert!(opt_res.is_some());

    let res_str = opt_res.unwrap().unwrap();
    assert!(res_str.contains("Sandbox:"));
    assert!(res_str.contains("registry dispatch test"));
}

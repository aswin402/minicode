use minicode::dev::models::{DevProcessId, DevProcessStatus};
use minicode::dev::registry::get_global_dev_registry;
use minicode::tools::registry::dev_tools::dispatch;
use serde_json::json;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tempfile::tempdir;

#[tokio::test]
async fn test_subagent_worker_registration_and_mini_dev_crud() {
    let temp = tempdir().expect("tempdir created");
    let registry = get_global_dev_registry();

    let cancel_flag = Arc::new(AtomicBool::new(false));
    let cancel_clone = Arc::clone(&cancel_flag);

    let dev_id = DevProcessId::from("worker-subagent-coder-42");
    let handle = registry
        .register_worker(
            dev_id.clone(),
            "Subagent (Coder) - Implement Prometheus metrics".to_string(),
            "minicode run -d /tmp --tools coder 'metrics'".to_string(),
            temp.path().to_path_buf(),
            std::process::id(),
            0,
            Some(Arc::new(move || {
                cancel_clone.store(true, Ordering::SeqCst);
            })),
        )
        .await;

    // 1. Append step execution logs
    handle
        .append_log("Worker initializing workspace worktree...")
        .await;
    handle
        .append_log("🔧 Tool: file_search(\"Cargo.toml\")")
        .await;
    handle.append_log("📝 File modified: src/metrics.rs").await;

    // 2. Query via mini_dev action: "workers"
    let workers_args = json!({ "action": "workers" });
    let workers_res = dispatch("mini_dev", &workers_args, temp.path())
        .await
        .expect("dispatch handled")
        .expect("success response");
    assert!(workers_res.contains("Subagent (Coder) - Implement Prometheus metrics"));
    assert!(workers_res.contains("worker-subagent-coder-42"));

    // 3. Query via mini_dev action: "list", process_type: "worker"
    let list_args = json!({ "action": "list", "process_type": "worker" });
    let list_res = dispatch("mini_dev", &list_args, temp.path())
        .await
        .expect("dispatch handled")
        .expect("success response");
    assert!(list_res.contains("Subagent (Coder) - Implement Prometheus metrics"));

    // 4. Query logs via raw id (without worker- prefix to verify flexible resolution)
    let logs_args = json!({ "action": "logs", "id": "subagent-coder-42" });
    let logs_res = dispatch("mini_dev", &logs_args, temp.path())
        .await
        .expect("dispatch handled")
        .expect("success response");
    assert!(logs_res.contains("Worker initializing workspace worktree"));
    assert!(logs_res.contains("Tool: file_search"));
    assert!(logs_res.contains("File modified: src/metrics.rs"));

    // 5. Query status via raw id
    let status_args = json!({ "action": "status", "id": "subagent-coder-42" });
    let status_res = dispatch("mini_dev", &status_args, temp.path())
        .await
        .expect("dispatch handled")
        .expect("success response");
    assert!(status_res.contains("Subagent (Coder) - Implement Prometheus metrics"));
    assert!(status_res.contains("Running"));

    // 6. Inspect runtime resource telemetry
    let res_args = json!({ "action": "resources" });
    let res_output = dispatch("mini_dev", &res_args, temp.path())
        .await
        .expect("dispatch handled")
        .expect("success response");
    assert!(res_output.contains("Runtime Resource Telemetry"));

    // 7. Stop worker via mini_dev action: "stop"
    let stop_args = json!({ "action": "stop", "id": "subagent-coder-42" });
    let stop_res = dispatch("mini_dev", &stop_args, temp.path())
        .await
        .expect("dispatch handled")
        .expect("success response");
    assert!(stop_res.contains("stopped successfully"));
    assert!(cancel_flag.load(Ordering::SeqCst));

    // Verify worker status transitioned to Stopped
    let updated = registry.get(&dev_id).await.expect("worker found");
    assert_eq!(updated.status, DevProcessStatus::Stopped);
}

#[tokio::test]
async fn test_child_process_worker_group_isolation_and_zero_orphan_cleanup() {
    let temp = tempdir().expect("tempdir created");
    let registry = get_global_dev_registry();

    // Spawn a genuine long-running child process in an isolated process group
    let mut std_cmd = std::process::Command::new("sh");
    std_cmd.arg("-c").arg("sleep 60");
    std_cmd.current_dir(temp.path());

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        unsafe {
            std_cmd.pre_exec(|| {
                if libc::setpgid(0, 0) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                #[cfg(target_os = "linux")]
                {
                    if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM) != 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                }
                Ok(())
            });
        }
    }

    let mut child = std_cmd.spawn().expect("child spawned");
    let child_pid = child.id();
    let pgid = child_pid;

    let dev_id = DevProcessId::from("worker-child-process-1");
    let handle = registry
        .register_worker(
            dev_id.clone(),
            "Delegated Child Task".to_string(),
            "sh -c 'sleep 60'".to_string(),
            temp.path().to_path_buf(),
            child_pid,
            pgid,
            None,
        )
        .await;

    // Verify process is alive
    #[cfg(unix)]
    unsafe {
        assert_eq!(libc::kill(child_pid as i32, 0), 0, "Child should be alive");
    }

    // Verify registry sees it as alive
    let summary = registry.get(&dev_id).await.expect("found in registry");
    assert!(summary.status.is_alive());

    // Stop process via handle terminate
    handle.terminate().await.expect("terminate succeeded");

    // Wait a brief moment and reap child process
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    let exit = child.wait().expect("child exited");
    assert!(
        !exit.success(),
        "Killed child process should have non-zero exit or signal"
    );

    #[cfg(unix)]
    unsafe {
        let res = libc::kill(child_pid as i32, 0);
        assert_ne!(res, 0, "Child process group must be terminated cleanly");
    }
}

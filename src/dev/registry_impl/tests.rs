//! Unit tests for [`MiniDevRegistry`].

use super::*;
use crate::dev::models::ScheduleRequest;
use std::time::Duration;
use tempfile::tempdir;

#[tokio::test]
async fn test_registry_crud_lifecycle() {
    let registry = MiniDevRegistry::new();
    let temp = tempdir().unwrap();

    // 1. Spawn a test job
    let req = SpawnDevRequest {
        command: "echo 'API ready on http://localhost:8999'; sleep 30".to_string(),
        name: Some("test-api".to_string()),
        process_type: DevProcessType::Backend,
        working_dir: None,
        extra_env: HashMap::new(),
        port_hint: None,
        max_memory_mb: None,
        port_policy: None,
        restart_policy: None,
    };

    let summary = registry.spawn(temp.path(), req).await.unwrap();
    assert_eq!(summary.name, "test-api");
    assert_eq!(summary.process_type, DevProcessType::Backend);

    // Wait brief moment for stdout port discovery
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;

    // 2. List jobs
    let list = registry.list().await;
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].ports, vec![8999]);
    assert_eq!(list[0].url, Some("http://localhost:8999".to_string()));

    // 3. Query logs
    let logs = registry.logs(&summary.id, 10, None).await.unwrap();
    assert!(logs.iter().any(|l| l.contains("API ready")));

    // 4. Resources
    let res = registry.resources().await;
    assert_eq!(res.total_active_processes, 1);

    // 5. Kill all
    let killed = registry.kill_all().await.unwrap();
    assert_eq!(killed, 1);

    // Verify status after kill
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    let updated = registry.get(&summary.id).await.unwrap();
    assert!(!updated.status.is_alive());
}

#[tokio::test]
async fn test_register_worker_lifecycle() {
    let registry = MiniDevRegistry::new();
    let temp = tempdir().unwrap();

    let cancel_called = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let cancel_clone = Arc::clone(&cancel_called);

    let dev_id = DevProcessId::from("worker-subagent-coder-1");
    let handle = registry
        .register_worker(
            dev_id.clone(),
            "Subagent (Coder) - Task 1".to_string(),
            "minicode run -d /tmp".to_string(),
            temp.path().to_path_buf(),
            std::process::id(),
            0,
            Some(Arc::new(move || {
                cancel_clone.store(true, std::sync::atomic::Ordering::SeqCst);
            })),
        )
        .await;

    handle.append_log("Worker spawned in worktree").await;
    handle.append_log("Running tests...").await;

    // Verify list_workers includes it
    let workers = registry.list_workers().await;
    assert_eq!(workers.len(), 1);
    assert_eq!(workers[0].process_type, DevProcessType::Worker);
    assert_eq!(workers[0].name, "Subagent (Coder) - Task 1");

    // Verify flexible get without worker- prefix
    let raw_id = DevProcessId::from("subagent-coder-1");
    let summary = registry.get(&raw_id).await.expect("found by raw id");
    assert_eq!(summary.id, dev_id);

    // Verify logs
    let logs = registry.logs(&raw_id, 10, None).await.unwrap();
    assert_eq!(logs.len(), 2);
    assert!(logs[0].contains("Worker spawned"));

    // Verify stop invokes cancel hook
    registry.stop(&raw_id).await.unwrap();
    assert!(cancel_called.load(std::sync::atomic::Ordering::SeqCst));

    let updated = registry.get(&dev_id).await.unwrap();
    assert_eq!(updated.status, DevProcessStatus::Stopped);
}

#[tokio::test]
async fn test_restart_lifecycle_preserves_id_and_env() {
    use crate::dev::models::SpawnDevRequest;

    let registry = MiniDevRegistry::new();
    let temp = tempdir().unwrap();

    let mut env = HashMap::new();
    env.insert("CUSTOM_KEY".to_string(), "CUSTOM_VAL_123".to_string());

    let req = SpawnDevRequest {
        command: "echo 'Server started'; sleep 60".to_string(),
        name: Some("resilient-service".to_string()),
        process_type: DevProcessType::Backend,
        working_dir: None,
        extra_env: env,
        port_hint: None,
        max_memory_mb: None,
        port_policy: None,
        restart_policy: None,
    };

    let summary1 = registry.spawn(temp.path(), req).await.expect("spawn ok");
    let orig_id = summary1.id.clone();
    let orig_pid = summary1.pid.expect("pid ok");

    // Give process a moment to initialize
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Restart process
    let summary2 = registry
        .restart(temp.path(), &orig_id)
        .await
        .expect("restart ok");

    // Invariant 1: DevProcessId must be strictly preserved across restarts
    assert_eq!(
        summary2.id, orig_id,
        "DevProcessId must match canonical original ID"
    );
    assert_ne!(
        summary2.pid,
        Some(orig_pid),
        "New PID must be assigned on restart"
    );

    // Invariant 2: Subsequent lookup by original ID must succeed
    let queried = registry
        .get(&orig_id)
        .await
        .expect("must find restarted process by original ID");
    assert_eq!(queried.id, orig_id);

    // Invariant 3: Environment variables must be preserved in handle
    {
        let lock = registry.processes.read().await;
        let handle = lock.get(&orig_id).expect("handle exists");
        assert_eq!(
            handle.extra_env.get("CUSTOM_KEY").map(|s| s.as_str()),
            Some("CUSTOM_VAL_123"),
            "Environment variable must survive restart"
        );
    }

    // Invariant 4: Stop by original ID must cleanly terminate new instance
    registry.stop(&orig_id).await.expect("stop ok");
    let stopped = registry.get(&orig_id).await.expect("exists");
    assert_eq!(stopped.status, DevProcessStatus::Stopped);
}

#[tokio::test]
async fn test_schedule_interval_execution_and_cancellation() {
    let registry = MiniDevRegistry::new();
    let temp = tempdir().expect("tempdir");

    let req = ScheduleRequest {
        command: "echo 'tick_interval'".to_string(),
        name: Some("test-timer".to_string()),
        interval_seconds: Some(1),
        duration_seconds: None,
        cron_expression: None,
        max_iterations: Some(3),
    };

    let summary = registry.schedule(temp.path(), req).await.expect("schedule");
    assert_eq!(summary.name, "test-timer");
    assert!(summary.id.as_str().starts_with("sched-test-timer-"));
    assert_eq!(summary.process_type, DevProcessType::Cron);
    assert!(summary.schedule_info.is_some());
    let s_info = summary.schedule_info.unwrap();
    assert_eq!(s_info.interval_secs, 1);
    assert!(!s_info.is_one_shot);

    // Sleep to allow tick to fire
    tokio::time::sleep(Duration::from_millis(1300)).await;

    let logs = registry.logs(&summary.id, 10, None).await.expect("logs");
    assert!(
        logs.iter()
            .any(|l| l.contains("tick_interval") || l.contains("Scheduled execution")),
        "Logs must contain scheduled tick output: {:?}",
        logs
    );

    // Stop scheduled task early
    let stopped = registry.stop(&summary.id).await.expect("stop");
    assert!(stopped);
    let updated = registry.get(&summary.id).await.expect("exists");
    assert_eq!(updated.status, DevProcessStatus::Stopped);
}

#[tokio::test]
async fn test_schedule_one_shot_timer() {
    let registry = MiniDevRegistry::new();
    let temp = tempdir().expect("tempdir");

    let req = ScheduleRequest {
        command: "echo 'timer_done'".to_string(),
        name: Some("test-one-shot".to_string()),
        interval_seconds: None,
        duration_seconds: Some(1),
        cron_expression: None,
        max_iterations: None,
    };

    let summary = registry.schedule(temp.path(), req).await.expect("schedule");
    assert_eq!(summary.process_type, DevProcessType::Timer);
    let s_info = summary.schedule_info.expect("schedule_info");
    assert!(s_info.is_one_shot);

    // Sleep to allow one-shot timer to fire and complete
    tokio::time::sleep(Duration::from_millis(1400)).await;

    let logs = registry.logs(&summary.id, 10, None).await.expect("logs");
    assert!(
        logs.iter()
            .any(|l| l.contains("timer_done") || l.contains("One-shot timer")),
        "Logs must contain one-shot output: {:?}",
        logs
    );

    let updated = registry.get(&summary.id).await.expect("exists");
    assert_eq!(updated.status, DevProcessStatus::Stopped);
}

#[tokio::test]
async fn test_register_and_list_swarm_process() {
    let registry = MiniDevRegistry::new();
    let handle = registry
        .register_swarm_process(
            DevProcessId::from("swarm-test-123"),
            "Swarm Test DAG".to_string(),
            "minicode swarm run test".to_string(),
            PathBuf::from("/tmp"),
            std::process::id(),
            0,
            None,
        )
        .await;

    handle.append_log("Initializing swarm worker 1").await;
    handle.append_log("Worker 1 completed successfully").await;

    let swarms = registry.list_swarms().await;
    assert_eq!(swarms.len(), 1);
    assert_eq!(swarms[0].id.as_str(), "swarm-test-123");
    assert_eq!(swarms[0].process_type, DevProcessType::Swarm);

    let logs = registry
        .logs(&DevProcessId::from("swarm-test-123"), 10, None)
        .await
        .expect("logs");
    assert_eq!(logs.len(), 2);
    assert_eq!(logs[0], "Initializing swarm worker 1");

    handle.update_status(DevProcessStatus::Stopped).await;
    let summary = registry
        .get(&DevProcessId::from("swarm-test-123"))
        .await
        .expect("summary");
    assert_eq!(summary.status, DevProcessStatus::Stopped);
}

#[tokio::test]
async fn test_non_daemon_background_task_stdin_await_and_harvest() {
    let registry = MiniDevRegistry::new();
    let temp = tempdir().expect("tempdir");

    let req = SpawnDevRequest {
        command: "read line_in; echo \"built:$line_in\"".to_string(),
        name: Some("bun-build-task".to_string()),
        process_type: DevProcessType::Script,
        working_dir: None,
        extra_env: HashMap::new(),
        port_hint: None,
        max_memory_mb: None,
        port_policy: None,
        restart_policy: None,
    };

    let (summary, handle) = registry
        .spawn_with_options(temp.path(), req, false)
        .await
        .expect("spawn non-daemon task");
    assert!(!handle.is_daemon);
    assert!(handle.log_path.is_some());
    assert_eq!(registry.active_non_daemon_task_count().await, 1);

    // Send interactive input to the running background task
    registry
        .send_input(&summary.id, "bundle-v1")
        .await
        .expect("send_input");

    // Wait for task to settle
    registry
        .wait_until_active_tasks_settle(Duration::from_secs(3))
        .await;
    assert_eq!(registry.active_non_daemon_task_count().await, 0);

    // Harvest newly completed task notification block
    let (ids, block) = registry
        .harvest_newly_completed_tasks_block(temp.path())
        .await
        .expect("harvest completed task");
    assert_eq!(ids, vec![summary.id.as_str().to_string()]);
    assert!(block.contains("<completed_background_tasks>"));
    assert!(block.contains("built:bundle-v1"));

    // Second harvest should be None since it was already reported
    assert!(registry
        .harvest_newly_completed_tasks_block(temp.path())
        .await
        .is_none());

    // Disk log file should also exist and contain the output
    let log_path = handle.log_path.as_ref().expect("log path");
    let disk_log = std::fs::read_to_string(log_path).expect("read disk log");
    assert!(disk_log.contains("built:bundle-v1"));
}

#[tokio::test]
async fn test_live_steering_prompt_wakes_active_minitask_barrier_without_killing_task() {
    let registry = Arc::new(MiniDevRegistry::new());
    let temp = tempdir().expect("tempdir");

    let req = SpawnDevRequest {
        command: "sleep 0.6; echo \"bg-build-complete\"".to_string(),
        name: Some("cargo-build".to_string()),
        process_type: DevProcessType::Script,
        working_dir: None,
        extra_env: HashMap::new(),
        port_hint: None,
        max_memory_mb: None,
        port_policy: None,
        restart_policy: None,
    };

    let (_summary, _handle) = registry
        .spawn_with_options(temp.path(), req, false)
        .await
        .expect("spawn non-daemon task");
    assert_eq!(registry.active_non_daemon_task_count().await, 1);

    let reg_clone = Arc::clone(&registry);
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(80)).await;
        reg_clone.push_steering_prompt("what is the next step in todo.md?".to_string());
    });

    let wait_start = std::time::Instant::now();
    registry
        .wait_until_active_tasks_settle(Duration::from_secs(5))
        .await;
    // Must wake up early when the live steering prompt arrives (~80ms) while the 600ms task is still running
    assert!(wait_start.elapsed() < Duration::from_millis(450));
    assert!(registry.is_continuation_armed());
    assert_eq!(registry.active_non_daemon_task_count().await, 1);

    let active_ctx = registry.format_active_tasks_context().await;
    assert!(active_ctx.contains("<active_minitask_processes>"));
    assert!(active_ctx.contains("cargo-build"));

    let drained = registry.drain_steering_prompts();
    assert_eq!(
        drained,
        vec!["what is the next step in todo.md?".to_string()]
    );
    assert!(!registry.has_steering_prompts());

    // Now wait for the background task to actually finish and verify continuation harvest
    registry
        .wait_until_active_tasks_settle(Duration::from_secs(3))
        .await;
    assert_eq!(registry.active_non_daemon_task_count().await, 0);
    assert!(registry.has_unreported_completed_tasks().await);
    let (_ids, block) = registry
        .harvest_newly_completed_tasks_block(temp.path())
        .await
        .expect("harvest after completion");
    assert!(block.contains("bg-build-complete"));
}

#[tokio::test]
async fn test_exec_managed_background_task_detaches_on_steering_and_fast_kill_all_sync() {
    let temp = tempdir().expect("tempdir");
    let global_reg = get_global_dev_registry();
    let _ = global_reg.drain_steering_prompts();

    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(60)).await;
        get_global_dev_registry().push_steering_prompt("status of current bun build?".to_string());
    });

    let start = std::time::Instant::now();
    let out = crate::tools::exec::exec_managed_background_task(
        temp.path(),
        "sleep 0.5; echo 'bun-build-ok'",
        5_000,
        None,
    )
    .await
    .expect("exec_managed_background_task");

    assert!(
        start.elapsed() < Duration::from_millis(400),
        "Should detach immediately on live steering prompt without waiting 5s"
    );
    assert!(out.contains("Live User Prompt Received"));
    let _ = global_reg.drain_steering_prompts();

    // Wait for the background task to complete naturally
    global_reg
        .wait_until_active_tasks_settle(Duration::from_secs(3))
        .await;

    // Verify kill_all_sync() is instantaneous (<20ms) when no processes are running
    let shutdown_start = std::time::Instant::now();
    kill_all_sync();
    assert!(
        shutdown_start.elapsed() < Duration::from_millis(20),
        "kill_all_sync() must be instantaneous when no background processes remain alive"
    );
}

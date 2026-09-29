//! Integration test suite for MiniTask Dynamic Scheduling and Natural Intent Telemetry.

use minicode::agent::orchestrator::{WorkflowArchetype, WorkflowRouter};
use minicode::dev::get_global_dev_registry;
use minicode::dev::models::{DevProcessStatus, DevProcessType, ScheduleRequest};
use minicode::tools::registry::dev_tools::dispatch;
use serde_json::json;
use std::time::Duration;
use tempfile::tempdir;

static TEST_MUTEX: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[tokio::test]
async fn test_minitask_schedule_recurring_interval() {
    let _guard = TEST_MUTEX.lock().await;
    let temp = tempdir().expect("tempdir");
    let registry = get_global_dev_registry();
    let _ = registry.kill_all().await;

    // 1. Dispatch action: "schedule" with 1 second interval
    let schedule_args = json!({
        "action": "schedule",
        "command": "echo 'Periodic watcher run'",
        "name": "system-watcher",
        "interval_seconds": 1,
        "max_iterations": 2
    });

    let res = dispatch("minitask", &schedule_args, temp.path())
        .await
        .expect("dispatch")
        .expect("output");

    assert!(res.contains("Scheduled task registered successfully"));
    assert!(res.contains("system-watcher"));
    assert!(res.contains("Every 1s"));

    // 2. Wait 1.3s for first execution tick
    tokio::time::sleep(Duration::from_millis(1300)).await;

    // 3. Inspect via action: "list"
    let list_args = json!({ "action": "list" });
    let list_res = dispatch("minitask", &list_args, temp.path())
        .await
        .expect("dispatch")
        .expect("output");

    assert!(list_res.contains("system-watcher"));
    assert!(list_res.contains("Schedule: Every 1s"));

    // 4. Inspect via action: "status"
    let status_args = json!({ "action": "status", "name": "system-watcher" });
    let status_res = dispatch("minitask", &status_args, temp.path())
        .await
        .expect("dispatch")
        .expect("output");

    assert!(status_res.contains("system-watcher"));
    assert!(status_res.contains("Schedule: Every 1s"));

    // 5. Clean up
    let kill_args = json!({ "action": "kill_all" });
    let _ = dispatch("minitask", &kill_args, temp.path()).await;
}

#[tokio::test]
async fn test_minitask_schedule_one_shot_timer() {
    let _guard = TEST_MUTEX.lock().await;
    let temp = tempdir().expect("tempdir");
    let registry = get_global_dev_registry();
    let _ = registry.kill_all().await;

    // 1. Register one-shot timer with 1s duration
    let schedule_args = json!({
        "action": "schedule",
        "command": "echo 'Timer triggered'",
        "name": "wake-timer",
        "duration_seconds": 1
    });

    let res = dispatch("minitask", &schedule_args, temp.path())
        .await
        .expect("dispatch")
        .expect("output");

    assert!(res.contains("One-shot timer registered successfully"));
    assert!(res.contains("wake-timer"));
    assert!(res.contains("1s"));

    // 2. Wait 1.3s for timer to fire and auto-stop
    tokio::time::sleep(Duration::from_millis(1300)).await;

    // 3. Status should indicate Stopped
    let procs = registry.list().await;
    let timer_proc = procs.iter().find(|p| p.name == "wake-timer");
    if let Some(p) = timer_proc {
        assert_eq!(p.status, DevProcessStatus::Stopped);
        assert_eq!(p.process_type, DevProcessType::Timer);
    }

    // 4. Clean up
    let _ = registry.kill_all().await;
}

#[tokio::test]
async fn test_minitask_natural_intent_inspection_telemetry() {
    let _guard = TEST_MUTEX.lock().await;
    let temp = tempdir().expect("tempdir");
    let registry = get_global_dev_registry();
    let _ = registry.kill_all().await;

    // 1. Verify natural language prompt classifications
    assert_eq!(
        WorkflowRouter::classify("what's happening with the background tasks?"),
        WorkflowArchetype::RuntimeDev
    );
    assert_eq!(
        WorkflowRouter::classify("whats happening"),
        WorkflowArchetype::RuntimeDev
    );
    assert_eq!(
        WorkflowRouter::classify("what is running"),
        WorkflowArchetype::RuntimeDev
    );
    assert_eq!(
        WorkflowRouter::classify("status of tasks"),
        WorkflowArchetype::RuntimeDev
    );
    assert_eq!(
        WorkflowRouter::classify("is the server running?"),
        WorkflowArchetype::RuntimeDev
    );
    assert_eq!(
        WorkflowRouter::classify("how are background processes doing"),
        WorkflowArchetype::RuntimeDev
    );

    // 2. Start a mock server
    let start_args = json!({
        "action": "start",
        "command": "echo 'Telemetry service active'; sleep 30",
        "name": "telemetry-mock-service",
        "process_type": "backend"
    });
    let _ = dispatch("minitask", &start_args, temp.path()).await;

    // 3. Register a scheduled watcher
    let req = ScheduleRequest {
        command: "echo 'Health OK'".to_string(),
        name: Some("health-watch".to_string()),
        interval_seconds: Some(30),
        duration_seconds: None,
        cron_expression: None,
        max_iterations: None,
    };
    let _ = registry.schedule(temp.path(), req).await;

    // 4. Enrich context for "whats happening"
    let enrichment = WorkflowRouter::enrich_context(
        temp.path(),
        "what is happening with the background services?",
        WorkflowArchetype::RuntimeDev,
    )
    .await
    .expect("enrichment must be generated");

    assert!(enrichment.contains("<active_dev_services>"));
    assert!(enrichment.contains("telemetry-mock-service"));
    assert!(enrichment.contains("health-watch"));
    assert!(enrichment.contains("Schedule: Every 30s"));
    assert!(enrichment.contains("Status & Inspection Invariant:"));

    // 5. Clean up
    let kill_args = json!({ "action": "kill_all" });
    let _ = dispatch("minitask", &kill_args, temp.path()).await;

    // 6. Enrich context when 0 processes are active
    let empty_enrichment = WorkflowRouter::enrich_context(
        temp.path(),
        "whats happening",
        WorkflowArchetype::RuntimeDev,
    )
    .await
    .expect("empty enrichment must be generated");

    assert!(empty_enrichment.contains("None currently active"));
    assert!(empty_enrichment.contains("Directive: When the user asks what is happening"));
}

#[tokio::test]
async fn test_minitask_scheduled_zero_orphan_cancellation() {
    let _guard = TEST_MUTEX.lock().await;
    let temp = tempdir().expect("tempdir");
    let registry = get_global_dev_registry();
    let _ = registry.kill_all().await;

    // 1. Start scheduled task
    let schedule_args = json!({
        "action": "schedule",
        "command": "echo 'Orphan check'",
        "name": "orphan-test-task",
        "interval_seconds": 2
    });

    let _ = dispatch("minitask", &schedule_args, temp.path())
        .await
        .expect("dispatch");

    let procs_before = registry.list().await;
    assert_eq!(procs_before.len(), 1);

    // 2. Kill all
    let kill_args = json!({ "action": "kill_all" });
    let kill_res = dispatch("minitask", &kill_args, temp.path())
        .await
        .expect("dispatch")
        .expect("output");

    assert!(kill_res.contains("Terminated"));

    // 3. Verify zero orphans remain active
    let procs_after = registry.list().await;
    assert!(procs_after.is_empty(), "Zero orphan processes must remain");
}

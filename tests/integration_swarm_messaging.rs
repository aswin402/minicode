use minicode::agent::swarm::bus::{SwarmMessage, SwarmMessageBus, SwarmMessageIntent};
use minicode::agent::swarm::models::{SwarmError, SwarmExecutionState, SwarmPlan};
use minicode::agent::swarm::report::SwarmReporter;
use std::fs;
use tempfile::tempdir;

#[tokio::test]
async fn test_concurrent_cross_worker_messaging() {
    let dir = tempdir().unwrap();
    let swarm_dir = dir.path().to_path_buf();
    let bus = SwarmMessageBus::new(&swarm_dir).unwrap();

    let mut handles = Vec::new();
    for i in 0..3 {
        let bus_clone = bus.clone();
        let sender = format!("worker_{}", i);
        handles.push(tokio::spawn(async move {
            let msg = SwarmMessage::new(
                "swarm-conc",
                &sender,
                None,
                SwarmMessageIntent::PublishContract,
                format!("Topic {}", i),
                format!("Payload from {}", sender),
            );
            bus_clone.post_message(msg).unwrap();
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    let all = bus.all_messages().unwrap();
    assert_eq!(all.len(), 3);

    // Worker 0 should see messages from worker_1 and worker_2
    let unread_w0 = bus.read_unread("worker_0", None).unwrap();
    assert_eq!(unread_w0.len(), 2);
    assert!(!unread_w0.iter().any(|m| m.from_task == "worker_0"));
}

#[tokio::test]
async fn test_quota_and_payload_limits() {
    let dir = tempdir().unwrap();
    let bus = SwarmMessageBus::new(dir.path()).unwrap();

    // 1. Payload > 800 chars rejected
    let big = "X".repeat(801);
    let msg_big = SwarmMessage::new(
        "s",
        "w1",
        None,
        SwarmMessageIntent::CoordinationNote,
        "T",
        big,
    );
    assert!(matches!(
        bus.post_message(msg_big),
        Err(SwarmError::MessagePayloadTooLarge(_))
    ));

    // 2. Max 3 messages allowed
    for i in 0..3 {
        let msg = SwarmMessage::new(
            "s",
            "w1",
            None,
            SwarmMessageIntent::CoordinationNote,
            format!("T{}", i),
            "P",
        );
        assert!(bus.post_message(msg).is_ok());
    }

    let msg_overflow = SwarmMessage::new(
        "s",
        "w1",
        None,
        SwarmMessageIntent::CoordinationNote,
        "T_over",
        "P",
    );
    assert!(matches!(
        bus.post_message(msg_overflow),
        Err(SwarmError::MessageQuotaExceeded(_))
    ));
}

#[tokio::test]
async fn test_direct_addressing_privacy() {
    let dir = tempdir().unwrap();
    let bus = SwarmMessageBus::new(dir.path()).unwrap();

    let direct = SwarmMessage::new(
        "s",
        "w1",
        Some("w2"),
        SwarmMessageIntent::QueryInterface,
        "Secret Interface Query",
        "How to call fn bar()?",
    );
    bus.post_message(direct).unwrap();

    // w2 sees it
    let unread_w2 = bus.read_unread("w2", None).unwrap();
    assert_eq!(unread_w2.len(), 1);

    // w3 does NOT see it (addressed directly to w2)
    let unread_w3 = bus.read_unread("w3", None).unwrap();
    assert_eq!(unread_w3.len(), 0);
}

#[tokio::test]
async fn test_reporter_coordination_log_integration() {
    let dir = tempdir().unwrap();
    let workspace_root = dir.path();
    let swarm_id = "swarm-report-test";
    let swarm_dir = workspace_root
        .join(".minicode")
        .join("swarms")
        .join(swarm_id);
    let bus = SwarmMessageBus::new(&swarm_dir).unwrap();

    let msg = SwarmMessage::new(
        swarm_id,
        "worker_alpha",
        Some("worker_beta"),
        SwarmMessageIntent::PublishContract,
        "EngineInterface",
        "pub fn run_engine() -> Result<(), EngineError>",
    );
    bus.post_message(msg).unwrap();

    let plan = SwarmPlan {
        id: swarm_id.to_string(),
        title: "Test Swarm Integration".to_string(),
        objective: "Verify reporter coordination table".to_string(),
        created_at: "2026-10-01T00:00:00Z".to_string(),
        metadata: std::collections::HashMap::new(),
        tasks: vec![],
    };
    let state = SwarmExecutionState::new(&plan);

    let report_path =
        SwarmReporter::save_report(workspace_root, &plan, &state, Some("Merge clean")).unwrap();

    let report_content = fs::read_to_string(&report_path).unwrap();
    assert!(report_content.contains("## 💬 Inter-Worker Coordination Log"));
    assert!(report_content.contains("| Timestamp | From | To | Intent | Topic | Preview |"));
    assert!(report_content.contains("`worker_alpha`"));
    assert!(report_content.contains("`worker_beta`"));
    assert!(report_content.contains("📜 Contract"));
    assert!(report_content.contains("EngineInterface"));
    assert!(report_content.contains("pub fn run_engine() -> Result<(), EngineError>"));
}

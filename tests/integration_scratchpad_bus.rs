/// Integration tests for Phase 43: Subagent Shared Scratchpad & Inter-Worker Messaging Bus
///
/// Tests thread-safe blackboard CRUD, message bus routing, disk persistence, and tool dispatch.
use minicode::agent::subagent::scratchpad::{SharedScratchpad, WorkerMessageBus};
use minicode::tools::registry::agent_tools;
use tempfile::tempdir;

#[test]
fn test_scratchpad_crud_and_disk_persistence() {
    let dir = tempdir().unwrap();
    let ws = dir.path();

    let sp = SharedScratchpad::new();
    let entry = sp.write_entry(
        "auth_architecture",
        "JWT + Refresh Token flow",
        "Tokens expire in 15 mins. Refresh stored in Redis.",
        "researcher_1",
    );
    assert_eq!(entry.key, "auth_architecture");

    // Save and reload
    sp.save_to_disk(ws).unwrap();

    let reloaded_sp = SharedScratchpad::new();
    let loaded_count = reloaded_sp.load_from_disk(ws).unwrap();
    assert_eq!(loaded_count, 1);

    let fetched = reloaded_sp.read_entry("auth_architecture").unwrap();
    assert_eq!(fetched.title, "JWT + Refresh Token flow");
    assert_eq!(fetched.author, "researcher_1");
}

#[test]
fn test_worker_message_bus_swarm_routing() {
    let bus = WorkerMessageBus::new();

    // Worker 1 sends message to Worker 2
    bus.send_message(
        "researcher_agent",
        Some("test_engineer_agent"),
        "reproduction_steps",
        "Run `cargo test --test auth` with JWT_SECRET=test",
    );

    // Worker 3 broadcasts to all
    bus.send_message(
        "code_reviewer_agent",
        None,
        "critical_finding",
        "Memory leak found in connection pool buffer",
    );

    // Worker 2 should see both direct message and broadcast
    let inbox_w2 = bus.read_inbox("test_engineer_agent");
    assert_eq!(inbox_w2.len(), 2);
    assert!(inbox_w2.iter().any(|m| m.topic == "reproduction_steps"));
    assert!(inbox_w2.iter().any(|m| m.topic == "critical_finding"));

    // Worker 1 (sender) should only see Worker 3's broadcast, not its own direct message
    let inbox_w1 = bus.read_inbox("researcher_agent");
    assert_eq!(inbox_w1.len(), 1);
    assert_eq!(inbox_w1[0].topic, "critical_finding");
}

#[test]
fn test_scratchpad_and_bus_schemas_registered() {
    let schemas = agent_tools::get_schemas();
    let names: Vec<String> = schemas.into_iter().map(|s| s.name).collect();

    assert!(names.contains(&"scratchpad_write".to_string()));
    assert!(names.contains(&"scratchpad_read".to_string()));
    assert!(names.contains(&"scratchpad_list".to_string()));
    assert!(names.contains(&"send_worker_message".to_string()));
    assert!(names.contains(&"read_worker_messages".to_string()));
}

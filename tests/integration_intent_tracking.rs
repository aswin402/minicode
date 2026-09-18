mod common;

use common::{MockProvider, MockResponse};
use minicode::agent::AgentLoop;
use minicode::config::Config;
use minicode::context::memory::intent::{IntentLedger, RequirementStatus};
use tempfile::tempdir;
use tokio::sync::mpsc;

#[tokio::test]
async fn test_agent_loop_intent_tracking_auto_advance() {
    let dir = tempdir().unwrap();
    let ws_path = dir.path().to_path_buf();

    // 1. Initial Prompt with 2 requirements
    let prompt = "Build AgentBench\n#### 1. Dashboard\nCreate src/pages/Dashboard.tsx\n#### 2. Settings\nCreate src/pages/Settings.tsx";

    // Mock response invoking write_file for Dashboard.tsx
    let responses_turn_1 = vec![
        MockResponse::with_tool_call(
            "call_1",
            "write_file",
            serde_json::json!({
                "path": "src/pages/Dashboard.tsx",
                "content": "export const Dashboard = () => <div>Dashboard</div>;"
            }),
        ),
        MockResponse::text_only("Created Dashboard page component."),
    ];

    let provider = Box::new(MockProvider::new(responses_turn_1));
    let mut config = Config::default();
    config.git.auto_commit = false;
    config.agent.auto_approve = true;

    let mut agent = AgentLoop::new(&ws_path, config, provider);

    let (event_tx, mut event_rx) = mpsc::unbounded_channel();
    tokio::spawn(async move { while event_rx.recv().await.is_some() {} });

    let _turn_1 = agent.execute_turn(prompt, event_tx, None).await.unwrap();

    // Assert agent.intent_ledger().is_some()
    assert!(agent.intent_ledger().is_some());
    let ledger = agent.intent_ledger().unwrap();

    // Assert root objective
    assert_eq!(ledger.root_objective, "Build AgentBench");

    // Assert Dashboard requirement is marked Completed
    let dashboard_item = ledger
        .items
        .iter()
        .find(|item| item.title.contains("Dashboard"))
        .expect("Dashboard requirement item should exist");
    assert_eq!(dashboard_item.status, RequirementStatus::Completed);

    // Assert Settings requirement is still Pending
    let settings_item = ledger
        .items
        .iter()
        .find(|item| item.title.contains("Settings"))
        .expect("Settings requirement item should exist");
    assert_eq!(settings_item.status, RequirementStatus::Pending);

    // Assert .minicode/intent_anchor.json was written to disk and can be deserialized
    let anchor_path = ws_path.join(".minicode").join("intent_anchor.json");
    assert!(
        anchor_path.exists(),
        "Intent anchor file must exist on disk"
    );
    let disk_ledger =
        IntentLedger::load_from_disk(&anchor_path).expect("Must deserialize from disk");
    assert_eq!(disk_ledger.root_objective, "Build AgentBench");
    assert_eq!(disk_ledger.completed_count(), 1);
    assert_eq!(disk_ledger.total_count(), 2);
}

#[tokio::test]
async fn test_agent_loop_drift_increment_and_multi_turn_advance() {
    let dir = tempdir().unwrap();
    let ws_path = dir.path().to_path_buf();

    let prompt = "Build AgentBench\n#### 1. Dashboard\nCreate src/pages/Dashboard.tsx\n#### 2. Settings\nCreate src/pages/Settings.tsx";

    // Turn 1: Write Dashboard.tsx
    let responses_turn_1 = vec![
        MockResponse::with_tool_call(
            "call_1",
            "write_file",
            serde_json::json!({
                "path": "src/pages/Dashboard.tsx",
                "content": "export const Dashboard = () => <div>Dashboard</div>;"
            }),
        ),
        MockResponse::text_only("Created Dashboard page component."),
    ];

    let provider = Box::new(MockProvider::new(responses_turn_1));
    let mut config = Config::default();
    config.git.auto_commit = false;
    config.agent.auto_approve = true;

    let mut agent = AgentLoop::new(&ws_path, config.clone(), provider);

    let (event_tx1, mut event_rx1) = mpsc::unbounded_channel();
    tokio::spawn(async move { while event_rx1.recv().await.is_some() {} });

    let _ = agent.execute_turn(prompt, event_tx1, None).await.unwrap();

    assert_eq!(
        agent
            .intent_ledger()
            .unwrap()
            .consecutive_turns_without_progress,
        0
    );

    // Turn 2: Conversational reply without file modification -> drift increments
    let responses_turn_2 = vec![MockResponse::text_only("Thinking about next steps...")];
    agent.update_config(
        config.clone(),
        Box::new(MockProvider::new(responses_turn_2)),
    );

    let (event_tx2, mut event_rx2) = mpsc::unbounded_channel();
    tokio::spawn(async move { while event_rx2.recv().await.is_some() {} });

    let _ = agent
        .execute_turn("What's next?", event_tx2, None)
        .await
        .unwrap();

    assert_eq!(
        agent
            .intent_ledger()
            .unwrap()
            .consecutive_turns_without_progress,
        1
    );

    // Turn 3: Write Settings.tsx -> Settings completed, drift reset to 0
    let responses_turn_3 = vec![
        MockResponse::with_tool_call(
            "call_3",
            "write_file",
            serde_json::json!({
                "path": "src/pages/Settings.tsx",
                "content": "export const Settings = () => <div>Settings</div>;"
            }),
        ),
        MockResponse::text_only("Created Settings page component."),
    ];
    agent.update_config(
        config.clone(),
        Box::new(MockProvider::new(responses_turn_3)),
    );

    let (event_tx3, mut event_rx3) = mpsc::unbounded_channel();
    tokio::spawn(async move { while event_rx3.recv().await.is_some() {} });

    let _ = agent
        .execute_turn("Now build settings", event_tx3, None)
        .await
        .unwrap();

    let final_ledger = agent.intent_ledger().unwrap();
    assert_eq!(final_ledger.consecutive_turns_without_progress, 0);
    assert_eq!(final_ledger.completed_count(), 2);
    assert_eq!(final_ledger.total_count(), 2);
}

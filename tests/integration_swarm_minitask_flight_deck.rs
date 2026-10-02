//! Integration Test Suite: Swarm MiniTask Integration & Dynamic Multi-Style Flight Deck (Phase 153)
//!
//! Validates:
//! 1. Registration of swarms and worker child processes into MiniDevRegistry with DevProcessType::Swarm.
//! 2. Swarm dashboard style configuration, loose parsing, cycling (.next()), and persistence.
//! 3. Real-time SwarmDeckData ingestion of plan.json, state.json, and bus.jsonl messages.
//! 4. 100% theme-adaptive Ratatui rendering across all 5 styles with multiple color palettes.

use minicode::agent::swarm::bus::{SwarmMessage, SwarmMessageBus, SwarmMessageIntent};
use minicode::agent::swarm::models::{
    SwarmExecutionState, SwarmPlan, SwarmTaskSpec, SwarmTaskStatus,
};
use minicode::config::{Config, SwarmDashboardStyle, UiConfig};
use minicode::dev::models::{DevProcessId, DevProcessStatus, DevProcessType};
use minicode::dev::registry::MiniDevRegistry;
use minicode::ui::modals::processes::{render_processes_modal, ProcessesModalState, ProcessesTab};
use minicode::ui::modals::swarm_deck::{render_swarm_flight_deck, SwarmDeckData};
use minicode::ui::Theme;
use ratatui::backend::TestBackend;
use ratatui::Terminal;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use tempfile::tempdir;

#[tokio::test]
async fn test_swarm_minitask_registry_supervision() {
    let registry = MiniDevRegistry::new();

    // 1. Register parent swarm process (pid: 0 bypasses OS kill check in unit tests)
    let parent_id = DevProcessId::from("swarm-orchestrator-test");
    let _parent_handle = registry
        .register_swarm_process(
            parent_id.clone(),
            "Swarm Orchestrator: Backend Microservices".to_string(),
            "minicode swarm orchestrate".to_string(),
            PathBuf::from("/tmp"),
            0,
            0,
            None,
        )
        .await;

    // 2. Register worker sub-processes
    let worker1_id = DevProcessId::from("swarm-worker-t1-auth");
    let worker1_handle = registry
        .register_swarm_process(
            worker1_id.clone(),
            "Worker t1: Authentication Service".to_string(),
            "minicode worker run".to_string(),
            PathBuf::from("/tmp"),
            0,
            0,
            None,
        )
        .await;

    let worker2_id = DevProcessId::from("swarm-worker-t2-db");
    let _worker2_handle = registry
        .register_swarm_process(
            worker2_id.clone(),
            "Worker t2: Database Migrations".to_string(),
            "minicode worker run".to_string(),
            PathBuf::from("/tmp"),
            0,
            0,
            None,
        )
        .await;

    // 3. Verify list_swarms retrieves all 3 processes with DevProcessType::Swarm
    let swarms = registry.list_swarms().await;
    assert_eq!(swarms.len(), 3);
    for s in &swarms {
        assert_eq!(s.process_type, DevProcessType::Swarm);
        assert_eq!(s.status, DevProcessStatus::Running);
    }

    // 4. Append worker logs and read back
    worker1_handle
        .append_log("Compiling auth handlers...")
        .await;
    worker1_handle.append_log("JWT tokens verified").await;

    let logs = registry.logs(&worker1_id, 10, None).await.unwrap();
    assert_eq!(logs.len(), 2);
    assert_eq!(logs[0], "Compiling auth handlers...");
    assert_eq!(logs[1], "JWT tokens verified");

    // 5. Cleanly stop worker2
    registry.stop(&worker2_id).await.unwrap();
    let updated = registry.get(&worker2_id).await.unwrap();
    assert_eq!(updated.status, DevProcessStatus::Stopped);
}

#[test]
fn test_swarm_dashboard_style_cycling_and_config() {
    let ui_cfg = UiConfig::default();
    // Default swarm style must be "stylish" (Style 3: Minimal + Stylish)
    assert_eq!(ui_cfg.swarm_style, "stylish");
    assert_eq!(ui_cfg.swarm_style_enum(), SwarmDashboardStyle::Stylish);

    // Loose string parsing
    assert_eq!(
        SwarmDashboardStyle::from_str_loose("gitgraph"),
        SwarmDashboardStyle::GitGraph
    );
    assert_eq!(
        SwarmDashboardStyle::from_str_loose("dag"),
        SwarmDashboardStyle::GitGraph
    );
    assert_eq!(
        SwarmDashboardStyle::from_str_loose("modern"),
        SwarmDashboardStyle::Modern
    );
    assert_eq!(
        SwarmDashboardStyle::from_str_loose("cards"),
        SwarmDashboardStyle::Modern
    );
    assert_eq!(
        SwarmDashboardStyle::from_str_loose("minimal"),
        SwarmDashboardStyle::Minimal
    );
    assert_eq!(
        SwarmDashboardStyle::from_str_loose("tree"),
        SwarmDashboardStyle::Minimal
    );
    assert_eq!(
        SwarmDashboardStyle::from_str_loose("cockpit"),
        SwarmDashboardStyle::Cockpit
    );
    assert_eq!(
        SwarmDashboardStyle::from_str_loose("mission"),
        SwarmDashboardStyle::Cockpit
    );
    assert_eq!(
        SwarmDashboardStyle::from_str_loose("unknown"),
        SwarmDashboardStyle::Stylish
    );

    // Style cycling (.next())
    let mut style = SwarmDashboardStyle::Stylish;
    style = style.next();
    assert_eq!(style, SwarmDashboardStyle::GitGraph);
    style = style.next();
    assert_eq!(style, SwarmDashboardStyle::Modern);
    style = style.next();
    assert_eq!(style, SwarmDashboardStyle::Minimal);
    style = style.next();
    assert_eq!(style, SwarmDashboardStyle::Cockpit);
    style = style.next();
    assert_eq!(style, SwarmDashboardStyle::Stylish);

    // Config save and reload
    let temp = tempdir().unwrap();
    fs::create_dir_all(temp.path().join(".minicode")).unwrap();
    let mut config = Config::default();
    config.ui.swarm_style = "gitgraph".to_string();
    config.save(Some(temp.path())).unwrap();

    let reloaded = Config::load(Some(temp.path()), None).unwrap();
    assert_eq!(reloaded.ui.swarm_style, "gitgraph");
    assert_eq!(
        reloaded.ui.swarm_style_enum(),
        SwarmDashboardStyle::GitGraph
    );
}

#[tokio::test]
async fn test_flight_deck_data_loading_and_bus_tail() {
    let temp = tempdir().unwrap();
    let ws = temp.path();

    // Create swarm artifact directory
    let swarm_dir = ws.join(".minicode").join("swarms").join("integration-test");
    fs::create_dir_all(&swarm_dir).unwrap();

    // 1. Write mock plan.json
    let plan = SwarmPlan {
        id: "integration-test".to_string(),
        title: "Integration Swarm".to_string(),
        objective: "Run integration verification".to_string(),
        tasks: vec![
            SwarmTaskSpec {
                id: "t1".to_string(),
                title: "Frontend Build".to_string(),
                role_title: "Frontend Specialist".to_string(),
                instructions: "Build Vite UI".to_string(),
                prompt: "Build Vite UI".to_string(),
                file_boundaries: vec![],
                dependencies: vec![],
                check_command: None,
                expected_artifacts: vec![],
                workspace_mode: None,
                max_iterations: None,
            },
            SwarmTaskSpec {
                id: "t2".to_string(),
                title: "Backend API".to_string(),
                role_title: "Backend Architect".to_string(),
                instructions: "Compile Axum endpoints".to_string(),
                prompt: "Compile Axum endpoints".to_string(),
                file_boundaries: vec![],
                dependencies: vec!["t1".to_string()],
                check_command: None,
                expected_artifacts: vec![],
                workspace_mode: None,
                max_iterations: None,
            },
        ],
        created_at: chrono::Utc::now().to_rfc3339(),
        metadata: HashMap::new(),
    };
    fs::write(
        swarm_dir.join("plan.json"),
        serde_json::to_string_pretty(&plan).unwrap(),
    )
    .unwrap();

    // 2. Write mock state.json
    let mut state = SwarmExecutionState::new(&plan);
    state
        .task_statuses
        .insert("t1".to_string(), SwarmTaskStatus::Completed);
    state
        .task_statuses
        .insert("t2".to_string(), SwarmTaskStatus::Running);
    fs::write(
        swarm_dir.join("state.json"),
        serde_json::to_string_pretty(&state).unwrap(),
    )
    .unwrap();

    // 3. Post inter-worker messages to bus.jsonl
    let bus = SwarmMessageBus::new(&swarm_dir).unwrap();
    bus.post_message(SwarmMessage::new(
        "integration-test",
        "t1",
        Some("t2"),
        SwarmMessageIntent::PublishContract,
        "UserApiContract",
        "export interface User { id: string; name: string; }",
    ))
    .unwrap();

    bus.post_message(SwarmMessage::new(
        "integration-test",
        "t2",
        None,
        SwarmMessageIntent::CoordinationNote,
        "Progress",
        "Endpoints mounted, listening on port 3000",
    ))
    .unwrap();

    // 4. Register swarms in global dev registry (pid: 0 bypasses OS kill check)
    let reg = minicode::dev::registry::get_global_dev_registry();
    reg.register_swarm_process(
        DevProcessId::from("swarm-integration-test"),
        "Swarm: Integration Test".to_string(),
        "minicode swarm orchestrate".to_string(),
        PathBuf::from("/tmp"),
        0,
        0,
        None,
    )
    .await;
    reg.register_swarm_process(
        DevProcessId::from("swarm-worker-t1"),
        "Worker t1: Frontend Build".to_string(),
        "minicode worker run".to_string(),
        PathBuf::from("/tmp"),
        0,
        0,
        None,
    )
    .await;
    reg.register_swarm_process(
        DevProcessId::from("swarm-worker-t2"),
        "Worker t2: Backend API".to_string(),
        "minicode worker run".to_string(),
        PathBuf::from("/tmp"),
        0,
        0,
        None,
    )
    .await;

    // 5. Ingest data via SwarmDeckData
    let mut deck_data = SwarmDeckData::load(ws, 0, SwarmDashboardStyle::Stylish).await;
    assert_eq!(deck_data.style, SwarmDashboardStyle::Stylish);
    assert!(!deck_data.swarms.is_empty());
    assert_eq!(deck_data.workers.len(), 2);
    assert_eq!(deck_data.recent_messages.len(), 2);
    assert!(deck_data.plan.is_some());
    assert!(deck_data.state.is_some());

    // Test selection and cycle
    assert_eq!(
        deck_data.selected_worker().map(|w| w.id.as_str()),
        Some("swarm-worker-t1")
    );
    deck_data.select_next();
    assert_eq!(
        deck_data.selected_worker().map(|w| w.id.as_str()),
        Some("swarm-worker-t2")
    );
    deck_data.select_next();
    assert_eq!(
        deck_data.selected_worker().map(|w| w.id.as_str()),
        Some("swarm-worker-t1")
    );

    assert_eq!(deck_data.cycle_style(), SwarmDashboardStyle::GitGraph);
}

#[test]
fn test_render_all_five_styles_and_multi_theme_palettes() {
    let styles = [
        SwarmDashboardStyle::Stylish,
        SwarmDashboardStyle::GitGraph,
        SwarmDashboardStyle::Modern,
        SwarmDashboardStyle::Minimal,
        SwarmDashboardStyle::Cockpit,
    ];

    let themes = [
        Theme::default(),
        Theme::catppuccin_mocha(),
        Theme::nord_frost(),
        Theme::tokyo_night(),
        Theme::aura_soft_dark(),
    ];

    let backend = TestBackend::new(140, 45);
    let mut terminal = Terminal::new(backend).unwrap();

    let dummy_data = SwarmDeckData {
        swarms: vec![],
        workers: vec![],
        selected_index: 0,
        style: SwarmDashboardStyle::Stylish,
        recent_messages: vec![SwarmMessage::new(
            "swarm-theme-check",
            "t1",
            None,
            SwarmMessageIntent::CoordinationNote,
            "Live Bus Check",
            "Rendering with dynamic theme tokens",
        )],
        plan: None,
        state: None,
        selected_worker_logs: vec!["Compiling packages...".to_string()],
        status_message: Some("Ready".to_string()),
    };

    for theme in &themes {
        for style in styles {
            let mut data = dummy_data.clone();
            data.style = style;
            terminal
                .draw(|f| {
                    render_swarm_flight_deck(f, f.area(), theme, &data);
                })
                .unwrap();
        }
    }
}

#[tokio::test]
async fn test_processes_modal_swarm_tab_integration() {
    let temp = tempdir().unwrap();
    let mut modal_state = ProcessesModalState::new(temp.path());

    // Switch tabs until Swarm
    modal_state.set_tab(ProcessesTab::Swarm);
    assert_eq!(modal_state.active_tab, ProcessesTab::Swarm);

    let deck = SwarmDeckData::load(temp.path(), 0, SwarmDashboardStyle::Stylish).await;
    modal_state.swarm_deck_data = Some(Box::new(deck));

    let theme = Theme::default();
    let backend = TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|f| {
            render_processes_modal(f, &mut modal_state, f.area(), &theme);
        })
        .unwrap();
}

//! End-to-end integration test suite for MiniDev Process Monitor Modal & Interactive Dashboard.
//! Verifies modal state lifecycle, table navigation, search filtering, log streaming,
//! Ratatui rendering, and slash command catalog registrations.

use minicode::dev::models::{
    DevProcessId, DevProcessStatus, DevProcessSummary, DevProcessType, PortConflict,
    PortResolution, RestartPolicy, RuntimeResourceSummary,
};
use minicode::ui::input::PALETTE_COMMANDS;
use minicode::ui::modals::command_catalog::COMMAND_CATALOG_ITEMS;
use minicode::ui::modals::help::render_help;
use minicode::ui::modals::processes::{render_processes_modal, ProcessesModalState, ProcessesTab};
use minicode::ui::Theme;
use ratatui::backend::TestBackend;
use ratatui::Terminal;
use tempfile::tempdir;

fn create_test_processes() -> Vec<DevProcessSummary> {
    vec![
        DevProcessSummary {
            id: DevProcessId::from("vite-frontend"),
            name: "Vite Web Client".to_string(),
            process_type: DevProcessType::Frontend,
            status: DevProcessStatus::Running,
            pid: Some(10101),
            ports: vec![5173],
            url: Some("http://localhost:5173".to_string()),
            cpu_percent: 2.4,
            memory_rss_mb: 92.5,
            uptime_secs: 360,
            restart_count: 0,
            restart_policy: RestartPolicy::Never,
            port_resolution: Some(PortResolution::Unchanged { port: 5173 }),
        },
        DevProcessSummary {
            id: DevProcessId::from("axum-backend"),
            name: "Axum API Server".to_string(),
            process_type: DevProcessType::Backend,
            status: DevProcessStatus::Healthy,
            pid: Some(10102),
            ports: vec![8080],
            url: Some("http://localhost:8080".to_string()),
            cpu_percent: 1.1,
            memory_rss_mb: 48.0,
            uptime_secs: 720,
            restart_count: 1,
            restart_policy: RestartPolicy::on_failure_default(),
            port_resolution: Some(PortResolution::Shifted {
                requested: 8000,
                resolved: 8080,
                conflict: PortConflict {
                    port: 8000,
                    conflicting_pid: Some(9999),
                    process_name: Some("stale-service".to_string()),
                    command_line: Some("stale --port 8000".to_string()),
                    suggested_fallback: Some(8080),
                },
            }),
        },
        DevProcessSummary {
            id: DevProcessId::from("worker-codegen"),
            name: "Autonomous Codegen Subagent".to_string(),
            process_type: DevProcessType::Worker,
            status: DevProcessStatus::Running,
            pid: Some(10103),
            ports: vec![],
            url: None,
            cpu_percent: 4.8,
            memory_rss_mb: 135.0,
            uptime_secs: 90,
            restart_count: 0,
            restart_policy: RestartPolicy::Never,
            port_resolution: None,
        },
    ]
}

#[test]
fn test_process_monitor_modal_tab_filtering_and_navigation() {
    let temp = tempdir().expect("tempdir");
    let processes = create_test_processes();
    let res = RuntimeResourceSummary {
        total_active_processes: 3,
        total_cpu_percent: 8.3,
        total_memory_rss_mb: 275.5,
        active_ports: vec![5173, 8080],
    };
    let logs = vec![
        "2026-09-26T01:00:00 [info] System initialized".to_string(),
        "2026-09-26T01:00:01 [info] Ready at http://localhost:5173".to_string(),
    ];

    let mut state = ProcessesModalState::with_data(temp.path(), processes, Some(res), logs);

    // 1. Initial tab: All
    assert_eq!(state.active_tab, ProcessesTab::All);
    assert_eq!(state.filtered_indices.len(), 3);
    assert_eq!(
        state.selected_process().map(|p| p.name.as_str()),
        Some("Vite Web Client")
    );

    // 2. Tab: Servers (Filters out worker)
    state.next_tab();
    assert_eq!(state.active_tab, ProcessesTab::Servers);
    assert_eq!(state.filtered_indices.len(), 2);
    assert_eq!(
        state.selected_process().map(|p| p.name.as_str()),
        Some("Vite Web Client")
    );

    // 3. Tab: Workers (Only autonomous worker)
    state.next_tab();
    assert_eq!(state.active_tab, ProcessesTab::Workers);
    assert_eq!(state.filtered_indices.len(), 1);
    assert_eq!(
        state.selected_process().map(|p| p.name.as_str()),
        Some("Autonomous Codegen Subagent")
    );

    // 4. Tab: Logs
    state.next_tab();
    assert_eq!(state.active_tab, ProcessesTab::Logs);
    assert_eq!(state.selected_logs.len(), 2);

    // 5. Tab: Telemetry
    state.next_tab();
    assert_eq!(state.active_tab, ProcessesTab::Telemetry);
    assert!(state.resources.is_some());

    // 6. Loop back to All
    state.next_tab();
    assert_eq!(state.active_tab, ProcessesTab::All);
    assert_eq!(state.filtered_indices.len(), 3);
}

#[test]
fn test_process_monitor_search_query_filtering() {
    let temp = tempdir().expect("tempdir");
    let processes = create_test_processes();
    let mut state = ProcessesModalState::with_data(temp.path(), processes, None, Vec::new());

    // Search by port number
    state.search_query = "8080".to_string();
    state.refresh_filtered();
    assert_eq!(state.filtered_indices.len(), 1);
    assert_eq!(
        state.selected_process().map(|p| p.name.as_str()),
        Some("Axum API Server")
    );

    // Search by name substring
    state.search_query = "codegen".to_string();
    state.refresh_filtered();
    assert_eq!(state.filtered_indices.len(), 1);
    assert_eq!(
        state.selected_process().map(|p| p.name.as_str()),
        Some("Autonomous Codegen Subagent")
    );

    // Search with no matches
    state.search_query = "nonexistent-process".to_string();
    state.refresh_filtered();
    assert_eq!(state.filtered_indices.len(), 0);
    assert!(state.selected_process().is_none());

    // Clear search
    state.search_query.clear();
    state.refresh_filtered();
    assert_eq!(state.filtered_indices.len(), 3);
}

#[test]
fn test_process_monitor_log_scrolling_and_auto_scroll() {
    let temp = tempdir().expect("tempdir");
    let processes = create_test_processes();
    let logs: Vec<String> = (1..=100).map(|i| format!("Output line {}", i)).collect();

    let mut state = ProcessesModalState::with_data(temp.path(), processes, None, logs);
    state.set_tab(ProcessesTab::Logs);

    assert_eq!(state.selected_logs.len(), 100);
    assert!(state.auto_scroll_logs);
    assert_eq!(state.log_scroll_offset, 99);

    // Scroll up
    state.scroll_logs_up(20);
    assert!(!state.auto_scroll_logs);
    assert_eq!(state.log_scroll_offset, 79);

    // Scroll down to the end
    state.scroll_logs_down(50);
    assert_eq!(state.log_scroll_offset, 99);
    assert!(state.auto_scroll_logs);
}

#[test]
fn test_process_monitor_ratatui_render_all_tabs() {
    let temp = tempdir().expect("tempdir");
    let processes = create_test_processes();
    let res = RuntimeResourceSummary {
        total_active_processes: 3,
        total_cpu_percent: 8.3,
        total_memory_rss_mb: 275.5,
        active_ports: vec![5173, 8080],
    };
    let logs: Vec<String> = (1..=20)
        .map(|i| format!("Stream event [{:02}] OK", i))
        .collect();

    let state = ProcessesModalState::with_data(temp.path(), processes, Some(res), logs);
    let theme = Theme::aura_dark();

    let backend = TestBackend::new(140, 45);
    let mut terminal = Terminal::new(backend).expect("terminal initialization");

    for tab in ProcessesTab::all() {
        let mut tab_state = state.clone();
        tab_state.active_tab = *tab;
        terminal
            .draw(|f| {
                let area = f.area();
                render_processes_modal(f, &tab_state, area, &theme);
            })
            .expect("modal draw must succeed");
    }
}

#[test]
fn test_command_catalog_and_help_registrations() {
    // Assert /processes exists in COMMAND_CATALOG_ITEMS
    let catalog_item = COMMAND_CATALOG_ITEMS
        .iter()
        .find(|item| item.name == "/processes");
    assert!(
        catalog_item.is_some(),
        "/processes must be listed in COMMAND_CATALOG_ITEMS"
    );
    let item = catalog_item.expect("catalog_item");
    assert_eq!(item.shortcut, "F7");

    // Assert /processes and /dev exist in PALETTE_COMMANDS
    let has_processes_palette = PALETTE_COMMANDS
        .iter()
        .any(|cmd| cmd.slash_name == "/processes" && cmd.shortcut == Some("F7"));
    assert!(
        has_processes_palette,
        "/processes with shortcut F7 must be in PALETTE_COMMANDS"
    );

    let has_dev_palette = PALETTE_COMMANDS
        .iter()
        .any(|cmd| cmd.slash_name == "/dev" && cmd.shortcut == Some("F7"));
    assert!(
        has_dev_palette,
        "/dev with shortcut F7 must be in PALETTE_COMMANDS"
    );

    // Verify render_help renders without panic
    let theme = Theme::aura_dark();
    let backend = TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal
        .draw(|f| {
            let area = f.area();
            render_help(f, area, &theme);
        })
        .expect("render_help must succeed");
}

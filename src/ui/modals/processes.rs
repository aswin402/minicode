//! In-TUI MiniDev Process Monitor, Ring-Buffer Log Viewer & Watchdog Supervisor Modal.

use crate::dev::models::{
    DevProcessStatus, DevProcessSummary, DevProcessType, PortResolution, RestartPolicy,
    RuntimeResourceSummary,
};
use crate::ui::layout_utils::centered_rect;
use crate::ui::theme::Theme;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Margin, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Clear, Gauge, Paragraph, Row, Table};
use ratatui::Frame;
use std::path::{Path, PathBuf};

/// Active tab within the `/processes` (`/dev`) modal dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessesTab {
    All,
    Servers,
    Workers,
    Logs,
    Telemetry,
}

impl ProcessesTab {
    pub fn all() -> &'static [ProcessesTab] {
        &[
            ProcessesTab::All,
            ProcessesTab::Servers,
            ProcessesTab::Workers,
            ProcessesTab::Logs,
            ProcessesTab::Telemetry,
        ]
    }

    pub fn title(&self) -> &'static str {
        match self {
            ProcessesTab::All => "All Processes",
            ProcessesTab::Servers => "Dev Servers",
            ProcessesTab::Workers => "Workers",
            ProcessesTab::Logs => "Live Logs",
            ProcessesTab::Telemetry => "Telemetry",
        }
    }

    pub fn next(&self) -> Self {
        match self {
            ProcessesTab::All => ProcessesTab::Servers,
            ProcessesTab::Servers => ProcessesTab::Workers,
            ProcessesTab::Workers => ProcessesTab::Logs,
            ProcessesTab::Logs => ProcessesTab::Telemetry,
            ProcessesTab::Telemetry => ProcessesTab::All,
        }
    }

    pub fn prev(&self) -> Self {
        match self {
            ProcessesTab::All => ProcessesTab::Telemetry,
            ProcessesTab::Servers => ProcessesTab::All,
            ProcessesTab::Workers => ProcessesTab::Servers,
            ProcessesTab::Logs => ProcessesTab::Workers,
            ProcessesTab::Telemetry => ProcessesTab::Logs,
        }
    }
}

/// State for the interactive `/processes` (`/dev`, `F7`) modal dialog.
#[derive(Debug, Clone)]
pub struct ProcessesModalState {
    #[allow(dead_code)]
    pub workspace_root: PathBuf,
    pub active_tab: ProcessesTab,
    pub selected_index: usize,
    pub search_query: String,
    pub is_searching: bool,
    pub processes: Vec<DevProcessSummary>,
    pub filtered_indices: Vec<usize>,
    pub resources: Option<RuntimeResourceSummary>,
    pub selected_logs: Vec<String>,
    pub log_scroll_offset: usize,
    pub auto_scroll_logs: bool,
    pub status_message: Option<String>,
}

impl ProcessesModalState {
    #[allow(dead_code)]
    pub fn new(workspace: &Path) -> Self {
        let mut state = Self {
            workspace_root: workspace.to_path_buf(),
            active_tab: ProcessesTab::All,
            selected_index: 0,
            search_query: String::new(),
            is_searching: false,
            processes: Vec::new(),
            filtered_indices: Vec::new(),
            resources: None,
            selected_logs: Vec::new(),
            log_scroll_offset: 0,
            auto_scroll_logs: true,
            status_message: None,
        };
        state.refresh_filtered();
        state
    }

    pub fn with_data(
        workspace: &Path,
        processes: Vec<DevProcessSummary>,
        resources: Option<RuntimeResourceSummary>,
        logs: Vec<String>,
    ) -> Self {
        let initial_offset = if !logs.is_empty() {
            logs.len().saturating_sub(1)
        } else {
            0
        };
        let mut state = Self {
            workspace_root: workspace.to_path_buf(),
            active_tab: ProcessesTab::All,
            selected_index: 0,
            search_query: String::new(),
            is_searching: false,
            processes,
            filtered_indices: Vec::new(),
            resources,
            selected_logs: logs,
            log_scroll_offset: initial_offset,
            auto_scroll_logs: true,
            status_message: None,
        };
        state.refresh_filtered();
        state
    }

    pub fn update_data(
        &mut self,
        processes: Vec<DevProcessSummary>,
        resources: Option<RuntimeResourceSummary>,
    ) {
        self.processes = processes;
        self.resources = resources;
        self.refresh_filtered();
    }

    pub fn set_logs(&mut self, logs: Vec<String>) {
        self.selected_logs = logs;
        if self.auto_scroll_logs && !self.selected_logs.is_empty() {
            self.log_scroll_offset = self.selected_logs.len().saturating_sub(1);
        }
    }

    pub fn next_tab(&mut self) {
        self.active_tab = self.active_tab.next();
        self.selected_index = 0;
        if self.active_tab == ProcessesTab::Logs
            && self.auto_scroll_logs
            && !self.selected_logs.is_empty()
        {
            self.log_scroll_offset = self.selected_logs.len().saturating_sub(1);
        } else {
            self.log_scroll_offset = 0;
        }
        self.status_message = None;
        self.refresh_filtered();
    }

    pub fn prev_tab(&mut self) {
        self.active_tab = self.active_tab.prev();
        self.selected_index = 0;
        if self.active_tab == ProcessesTab::Logs
            && self.auto_scroll_logs
            && !self.selected_logs.is_empty()
        {
            self.log_scroll_offset = self.selected_logs.len().saturating_sub(1);
        } else {
            self.log_scroll_offset = 0;
        }
        self.status_message = None;
        self.refresh_filtered();
    }

    pub fn set_tab(&mut self, tab: ProcessesTab) {
        self.active_tab = tab;
        self.selected_index = 0;
        if self.active_tab == ProcessesTab::Logs
            && self.auto_scroll_logs
            && !self.selected_logs.is_empty()
        {
            self.log_scroll_offset = self.selected_logs.len().saturating_sub(1);
        } else {
            self.log_scroll_offset = 0;
        }
        self.status_message = None;
        self.refresh_filtered();
    }

    pub fn select_next(&mut self) {
        if self.filtered_indices.is_empty() {
            self.selected_index = 0;
            return;
        }
        if self.selected_index + 1 < self.filtered_indices.len() {
            self.selected_index += 1;
        } else {
            self.selected_index = 0;
        }
    }

    pub fn select_prev(&mut self) {
        if self.filtered_indices.is_empty() {
            self.selected_index = 0;
            return;
        }
        if self.selected_index > 0 {
            self.selected_index -= 1;
        } else {
            self.selected_index = self.filtered_indices.len().saturating_sub(1);
        }
    }

    pub fn scroll_logs_up(&mut self, delta: usize) {
        self.auto_scroll_logs = false;
        self.log_scroll_offset = self.log_scroll_offset.saturating_sub(delta);
    }

    pub fn scroll_logs_down(&mut self, delta: usize) {
        let max_offset = self.selected_logs.len().saturating_sub(1);
        self.log_scroll_offset = (self.log_scroll_offset + delta).min(max_offset);
        if self.log_scroll_offset == max_offset {
            self.auto_scroll_logs = true;
        }
    }

    pub fn selected_process(&self) -> Option<&DevProcessSummary> {
        if self.filtered_indices.is_empty() {
            return None;
        }
        let actual_idx = self.filtered_indices.get(self.selected_index).copied()?;
        self.processes.get(actual_idx)
    }

    pub fn handle_char(&mut self, c: char) {
        if self.is_searching {
            self.search_query.push(c);
            self.refresh_filtered();
        }
    }

    pub fn handle_backspace(&mut self) {
        if self.is_searching {
            self.search_query.pop();
            self.refresh_filtered();
        }
    }

    pub fn refresh_filtered(&mut self) {
        let query = self.search_query.trim().to_lowercase();
        let mut matched = Vec::new();

        for (idx, p) in self.processes.iter().enumerate() {
            // Tab filtering
            let tab_match = match self.active_tab {
                ProcessesTab::All | ProcessesTab::Logs | ProcessesTab::Telemetry => true,
                ProcessesTab::Servers => {
                    matches!(
                        p.process_type,
                        DevProcessType::Frontend | DevProcessType::Backend | DevProcessType::Docker
                    ) || !p.ports.is_empty()
                        || p.url.is_some()
                }
                ProcessesTab::Workers => matches!(p.process_type, DevProcessType::Worker),
            };

            if !tab_match {
                continue;
            }

            // Search query matching
            if query.is_empty() {
                matched.push(idx);
            } else {
                let id_match = p.id.as_str().to_lowercase().contains(&query);
                let name_match = p.name.to_lowercase().contains(&query);
                let port_match = p.ports.iter().any(|port| port.to_string().contains(&query));
                let url_match = p
                    .url
                    .as_deref()
                    .unwrap_or("")
                    .to_lowercase()
                    .contains(&query);
                let type_match = format!("{:?}", p.process_type)
                    .to_lowercase()
                    .contains(&query);

                if id_match || name_match || port_match || url_match || type_match {
                    matched.push(idx);
                }
            }
        }

        self.filtered_indices = matched;
        if self.selected_index >= self.filtered_indices.len() {
            self.selected_index = self.filtered_indices.len().saturating_sub(1);
        }
    }
}

/// Renders the complete MiniDev Process Monitor modal.
pub fn render_processes_modal(
    frame: &mut Frame,
    state: &ProcessesModalState,
    area: Rect,
    theme: &Theme,
) {
    let modal_area = centered_rect(92, 86, area);
    frame.render_widget(Clear, modal_area);

    let outer_block = Block::default()
        .borders(Borders::ALL)
        .border_style(
            Style::default()
                .fg(theme.brand_accent)
                .add_modifier(Modifier::BOLD),
        )
        .title(" ⚡ MiniDev Process Monitor & Watchdog Supervisor ");
    frame.render_widget(outer_block, modal_area);

    let inner = modal_area.inner(Margin {
        vertical: 1,
        horizontal: 1,
    });

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Top Tab Bar & Filter Box
            Constraint::Length(if state.status_message.is_some() { 1 } else { 0 }), // Flash notification banner
            Constraint::Min(6),    // Main Body Content
            Constraint::Length(1), // Footer Shortcuts
        ])
        .split(inner);

    render_top_tabs_and_search(frame, state, layout[0], theme);

    if let Some(ref msg) = state.status_message {
        let status_para = Paragraph::new(Line::from(vec![
            Span::styled(
                " ℹ ",
                Style::default()
                    .fg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(msg, Style::default().fg(theme.text_primary)),
        ]));
        frame.render_widget(status_para, layout[1]);
    }

    match state.active_tab {
        ProcessesTab::All | ProcessesTab::Servers | ProcessesTab::Workers => {
            render_process_table_and_details(frame, state, layout[2], theme);
        }
        ProcessesTab::Logs => {
            render_process_logs_tab(frame, state, layout[2], theme);
        }
        ProcessesTab::Telemetry => {
            render_system_telemetry_tab(frame, state, layout[2], theme);
        }
    }

    render_shortcuts_footer(frame, state, layout[3], theme);
}

/// Renders the top tabs row and the search/filter status.
fn render_top_tabs_and_search(
    frame: &mut Frame,
    state: &ProcessesModalState,
    area: Rect,
    theme: &Theme,
) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(70), // Tabs
            Constraint::Percentage(30), // Search bar
        ])
        .split(area);

    let mut tab_spans = Vec::new();
    let tabs = ProcessesTab::all();

    for (i, tab) in tabs.iter().enumerate() {
        let is_active = *tab == state.active_tab;
        let count_str = match tab {
            ProcessesTab::All => format!(" ({})", state.processes.len()),
            ProcessesTab::Servers => {
                let cnt = state
                    .processes
                    .iter()
                    .filter(|p| {
                        matches!(
                            p.process_type,
                            DevProcessType::Frontend
                                | DevProcessType::Backend
                                | DevProcessType::Docker
                        ) || !p.ports.is_empty()
                            || p.url.is_some()
                    })
                    .count();
                format!(" ({})", cnt)
            }
            ProcessesTab::Workers => {
                let cnt = state
                    .processes
                    .iter()
                    .filter(|p| matches!(p.process_type, DevProcessType::Worker))
                    .count();
                format!(" ({})", cnt)
            }
            ProcessesTab::Logs => {
                if let Some(p) = state.selected_process() {
                    format!(" ({})", p.name)
                } else {
                    String::new()
                }
            }
            ProcessesTab::Telemetry => String::new(),
        };

        let label = format!(" [{}] {}{} ", i + 1, tab.title(), count_str);

        if is_active {
            tab_spans.push(Span::styled(
                label,
                Style::default()
                    .fg(theme.bg_primary)
                    .bg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD),
            ));
        } else {
            tab_spans.push(Span::styled(
                label,
                Style::default()
                    .fg(theme.text_primary)
                    .add_modifier(Modifier::DIM),
            ));
        }
        tab_spans.push(Span::raw(" "));
    }

    let tabs_para = Paragraph::new(Line::from(tab_spans)).block(
        Block::default()
            .borders(Borders::BOTTOM)
            .border_style(Style::default().fg(theme.border)),
    );
    frame.render_widget(tabs_para, chunks[0]);

    // Right: Search Filter Input
    let search_line = if state.is_searching {
        Line::from(vec![
            Span::styled("🔍 ", Style::default().fg(theme.brand_accent)),
            Span::styled(
                &state.search_query,
                Style::default()
                    .fg(theme.text_primary)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("▎", Style::default().fg(theme.brand_accent)),
        ])
    } else if state.search_query.is_empty() {
        Line::from(vec![
            Span::styled(
                "[/] Filter",
                Style::default().fg(theme.muted).add_modifier(Modifier::DIM),
            ),
            Span::raw(" "),
        ])
    } else {
        Line::from(vec![
            Span::styled("🔍 Filter: ", Style::default().fg(theme.brand_accent)),
            Span::styled(&state.search_query, Style::default().fg(theme.text_primary)),
            Span::styled(
                " (Esc to clear)",
                Style::default().fg(theme.muted).add_modifier(Modifier::DIM),
            ),
        ])
    };

    let search_para = Paragraph::new(search_line)
        .alignment(Alignment::Right)
        .block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(Style::default().fg(theme.border)),
        );
    frame.render_widget(search_para, chunks[1]);
}

/// Renders split layout: Process Table on left, Process Details Card on right.
fn render_process_table_and_details(
    frame: &mut Frame,
    state: &ProcessesModalState,
    area: Rect,
    theme: &Theme,
) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(63), // Table view
            Constraint::Percentage(37), // Details panel
        ])
        .split(area);

    render_process_table(frame, state, chunks[0], theme);
    render_process_details_card(frame, state, chunks[1], theme);
}

/// Renders the rich table of managed processes.
fn render_process_table(frame: &mut Frame, state: &ProcessesModalState, area: Rect, theme: &Theme) {
    let table_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border))
        .title(format!(
            " Active Processes ({}) ",
            state.filtered_indices.len()
        ));

    if state.filtered_indices.is_empty() {
        let empty_msg = vec![
            Line::from(""),
            Line::from(vec![Span::styled(
                "  ℹ No active development processes match filter.",
                Style::default().fg(theme.muted),
            )]),
            Line::from(""),
            Line::from(vec![Span::styled(
                "  Use 'mini_dev start', /dev, or run npm/cargo to launch processes.",
                Style::default().fg(theme.muted).add_modifier(Modifier::DIM),
            )]),
        ];
        let p = Paragraph::new(empty_msg).block(table_block);
        frame.render_widget(p, area);
        return;
    }

    let header_cells = [
        Cell::from("STATUS").style(
            Style::default()
                .fg(theme.brand_accent)
                .add_modifier(Modifier::BOLD),
        ),
        Cell::from("ID").style(
            Style::default()
                .fg(theme.brand_accent)
                .add_modifier(Modifier::BOLD),
        ),
        Cell::from("NAME").style(
            Style::default()
                .fg(theme.brand_accent)
                .add_modifier(Modifier::BOLD),
        ),
        Cell::from("TYPE").style(
            Style::default()
                .fg(theme.brand_accent)
                .add_modifier(Modifier::BOLD),
        ),
        Cell::from("PID").style(
            Style::default()
                .fg(theme.brand_accent)
                .add_modifier(Modifier::BOLD),
        ),
        Cell::from("PORT / URL").style(
            Style::default()
                .fg(theme.brand_accent)
                .add_modifier(Modifier::BOLD),
        ),
        Cell::from("CPU").style(
            Style::default()
                .fg(theme.brand_accent)
                .add_modifier(Modifier::BOLD),
        ),
        Cell::from("RAM").style(
            Style::default()
                .fg(theme.brand_accent)
                .add_modifier(Modifier::BOLD),
        ),
        Cell::from("RESTARTS").style(
            Style::default()
                .fg(theme.brand_accent)
                .add_modifier(Modifier::BOLD),
        ),
    ];
    let header = Row::new(header_cells).height(1);

    let rows: Vec<Row> = state
        .filtered_indices
        .iter()
        .enumerate()
        .filter_map(|(pos, &actual_idx)| {
            let p = state.processes.get(actual_idx)?;
            let is_selected = pos == state.selected_index;

            let (status_text, status_color) = match &p.status {
                DevProcessStatus::Running => ("● Run", theme.success),
                DevProcessStatus::Healthy => ("✔ Ready", theme.success),
                DevProcessStatus::Degraded(_) => ("⚠ Degraded", theme.warning),
                DevProcessStatus::Exited(_) => ("○ Exited", theme.muted),
                DevProcessStatus::Stopped => ("○ Stop", theme.muted),
                DevProcessStatus::Killed => ("✖ Killed", theme.destructive),
            };

            let port_url_str = if let Some(ref res) = p.port_resolution {
                match res {
                    PortResolution::Shifted {
                        requested,
                        resolved,
                        ..
                    } => format!("{}➜{}", requested, resolved),
                    PortResolution::Reclaimed { port, .. } => format!("{}(rec)", port),
                    PortResolution::Unchanged { port } => port.to_string(),
                    PortResolution::Ignored { .. } => "-".to_string(),
                }
            } else if let Some(ref u) = p.url {
                u.clone()
            } else if !p.ports.is_empty() {
                p.ports
                    .iter()
                    .map(|x| x.to_string())
                    .collect::<Vec<_>>()
                    .join(",")
            } else {
                "-".to_string()
            };

            let pid_str = p.pid.map(|n| n.to_string()).unwrap_or_else(|| "-".into());
            let cpu_str = format!("{:.1}%", p.cpu_percent);
            let mem_str = format!("{:.1}M", p.memory_rss_mb);
            let restarts_str = if p.restart_count > 0 {
                format!("{}", p.restart_count)
            } else {
                "-".to_string()
            };

            let mut row = Row::new(vec![
                Cell::from(Span::styled(
                    status_text,
                    Style::default()
                        .fg(status_color)
                        .add_modifier(Modifier::BOLD),
                )),
                Cell::from(p.id.as_str()),
                Cell::from(p.name.as_str()),
                Cell::from(format!("{:?}", p.process_type)),
                Cell::from(pid_str),
                Cell::from(Span::styled(
                    port_url_str,
                    if p.port_resolution.is_some() {
                        Style::default().fg(theme.warning)
                    } else {
                        Style::default().fg(theme.text_primary)
                    },
                )),
                Cell::from(cpu_str),
                Cell::from(mem_str),
                Cell::from(restarts_str),
            ]);

            if is_selected {
                row = row.style(
                    Style::default()
                        .bg(theme.brand_accent)
                        .fg(theme.bg_primary)
                        .add_modifier(Modifier::BOLD),
                );
            }

            Some(row)
        })
        .collect();

    let widths = [
        Constraint::Length(8),  // STATUS
        Constraint::Length(14), // ID
        Constraint::Length(16), // NAME
        Constraint::Length(9),  // TYPE
        Constraint::Length(7),  // PID
        Constraint::Length(16), // PORT / URL
        Constraint::Length(7),  // CPU
        Constraint::Length(8),  // RAM
        Constraint::Length(8),  // RESTARTS
    ];

    let table = Table::new(rows, widths).header(header).block(table_block);

    frame.render_widget(table, area);
}

/// Renders the detailed inspection card for the currently selected process.
fn render_process_details_card(
    frame: &mut Frame,
    state: &ProcessesModalState,
    area: Rect,
    theme: &Theme,
) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border))
        .title(" Process Details & Telemetry ");

    let selected = match state.selected_process() {
        Some(p) => p,
        None => {
            let p = Paragraph::new(vec![
                Line::from(""),
                Line::from(vec![Span::styled(
                    "  No process selected.",
                    Style::default().fg(theme.muted),
                )]),
            ])
            .block(block);
            frame.render_widget(p, area);
            return;
        }
    };

    let inner_area = area.inner(Margin {
        vertical: 1,
        horizontal: 1,
    });

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(10), // Metadata & Watchdog info
            Constraint::Length(3),  // CPU & RAM Gauge meters
            Constraint::Min(4),     // Recent output peek
        ])
        .split(inner_area);

    let (status_str, status_color) = match &selected.status {
        DevProcessStatus::Running => ("RUNNING", theme.success),
        DevProcessStatus::Healthy => ("READY", theme.success),
        DevProcessStatus::Degraded(r) => (r.as_str(), theme.warning),
        DevProcessStatus::Exited(_) => ("EXITED", theme.muted),
        DevProcessStatus::Stopped => ("STOPPED", theme.muted),
        DevProcessStatus::Killed => ("KILLED", theme.destructive),
    };

    let port_res_str = match &selected.port_resolution {
        Some(PortResolution::Shifted {
            requested,
            resolved,
            ..
        }) => format!("Shifted: {} ➜ {} (Port Conflict)", requested, resolved),
        Some(PortResolution::Reclaimed { port, .. }) => {
            format!("Reclaimed port {} after conflict kill", port)
        }
        Some(PortResolution::Unchanged { port }) => format!("Port {} bound directly", port),
        Some(PortResolution::Ignored { .. }) => "Ignored port conflicts".to_string(),
        None => selected
            .url
            .as_deref()
            .unwrap_or(if selected.ports.is_empty() {
                "None"
            } else {
                "Listening"
            })
            .to_string(),
    };

    let policy_str = match selected.restart_policy {
        RestartPolicy::Never => "Never".to_string(),
        RestartPolicy::OnFailure {
            max_retries,
            backoff_ms,
        } => format!("OnFailure (max {}, backoff {}ms)", max_retries, backoff_ms),
        RestartPolicy::Always {
            max_retries,
            backoff_ms,
        } => format!("Always (max {}, backoff {}ms)", max_retries, backoff_ms),
    };

    let uptime_str = if selected.uptime_secs >= 3600 {
        format!(
            "{}h {}m",
            selected.uptime_secs / 3600,
            (selected.uptime_secs % 3600) / 60
        )
    } else if selected.uptime_secs >= 60 {
        format!(
            "{}m {}s",
            selected.uptime_secs / 60,
            selected.uptime_secs % 60
        )
    } else {
        format!("{}s", selected.uptime_secs)
    };

    let info_lines = vec![
        Line::from(vec![
            Span::styled(
                format!("  {} ", selected.name),
                Style::default()
                    .fg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(" [{}]", status_str),
                Style::default()
                    .fg(status_color)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("  ID:        ", Style::default().fg(theme.muted)),
            Span::styled(
                selected.id.as_str(),
                Style::default().fg(theme.text_primary),
            ),
        ]),
        Line::from(vec![
            Span::styled("  Type:      ", Style::default().fg(theme.muted)),
            Span::styled(
                format!("{:?}", selected.process_type),
                Style::default().fg(theme.text_primary),
            ),
            Span::styled("  PID: ", Style::default().fg(theme.muted)),
            Span::styled(
                selected
                    .pid
                    .map(|n| n.to_string())
                    .unwrap_or_else(|| "-".into()),
                Style::default().fg(theme.text_primary),
            ),
            Span::styled("  Uptime: ", Style::default().fg(theme.muted)),
            Span::styled(uptime_str, Style::default().fg(theme.text_primary)),
        ]),
        Line::from(vec![
            Span::styled("  Endpoint:  ", Style::default().fg(theme.muted)),
            Span::styled(
                selected.url.as_deref().unwrap_or("-"),
                Style::default().fg(theme.brand_accent),
            ),
        ]),
        Line::from(vec![
            Span::styled("  Port Res:  ", Style::default().fg(theme.muted)),
            Span::styled(
                port_res_str,
                if selected.port_resolution.is_some() {
                    Style::default().fg(theme.warning)
                } else {
                    Style::default().fg(theme.text_primary)
                },
            ),
        ]),
        Line::from(vec![
            Span::styled("  Watchdog:  ", Style::default().fg(theme.muted)),
            Span::styled(policy_str, Style::default().fg(theme.text_primary)),
            Span::styled(
                format!(" | Restarts: {}", selected.restart_count),
                Style::default().fg(theme.muted),
            ),
        ]),
    ];

    let info_para = Paragraph::new(info_lines);
    frame.render_widget(info_para, chunks[0]);

    // Metric Gauges (CPU & Memory)
    let gauge_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(50), // CPU
            Constraint::Percentage(50), // RAM
        ])
        .split(chunks[1]);

    let cpu_ratio = (selected.cpu_percent / 100.0).clamp(0.0, 1.0);
    let cpu_gauge = Gauge::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" CPU: {:.1}% ", selected.cpu_percent)),
        )
        .gauge_style(Style::default().fg(theme.brand_accent))
        .ratio(cpu_ratio as f64);
    frame.render_widget(cpu_gauge, gauge_layout[0]);

    let mem_ratio = (selected.memory_rss_mb / 512.0).clamp(0.0, 1.0);
    let mem_gauge = Gauge::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" RSS: {:.1} MB ", selected.memory_rss_mb)),
        )
        .gauge_style(Style::default().fg(theme.success))
        .ratio(mem_ratio as f64);
    frame.render_widget(mem_gauge, gauge_layout[1]);

    // Recent Log Peek (last few lines)
    let log_peek_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border))
        .title(" Recent Output Stream ([l] to expand) ");

    let log_lines: Vec<Line> = if state.selected_logs.is_empty() {
        vec![Line::from(Span::styled(
            "  (No log output recorded yet)",
            Style::default().fg(theme.muted),
        ))]
    } else {
        state
            .selected_logs
            .iter()
            .rev()
            .take(chunks[2].height.saturating_sub(2) as usize)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .map(|l| {
                let trimmed = if l.len() > 60 {
                    format!("{}…", &l[..59])
                } else {
                    l.clone()
                };
                Line::from(vec![
                    Span::styled(" › ", Style::default().fg(theme.brand_accent)),
                    Span::styled(trimmed, Style::default().fg(theme.text_primary)),
                ])
            })
            .collect()
    };

    let log_peek_para = Paragraph::new(log_lines).block(log_peek_block);
    frame.render_widget(log_peek_para, chunks[2]);

    frame.render_widget(block, area);
}

/// Renders the expanded, scrollable Live Logs tab.
fn render_process_logs_tab(
    frame: &mut Frame,
    state: &ProcessesModalState,
    area: Rect,
    theme: &Theme,
) {
    let proc_label = if let Some(p) = state.selected_process() {
        format!("{} ({})", p.name, p.id)
    } else {
        "All Processes".to_string()
    };

    let title = format!(
        " 📜 Live Log Viewer: {} | Total Lines: {} | Auto-scroll: {} ",
        proc_label,
        state.selected_logs.len(),
        if state.auto_scroll_logs { "ON" } else { "OFF" }
    );

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.brand_accent))
        .title(title);

    let inner = area.inner(Margin {
        vertical: 1,
        horizontal: 1,
    });

    if state.selected_logs.is_empty() {
        let empty = vec![
            Line::from(""),
            Line::from(vec![Span::styled(
                "  ℹ No logs available for this process.",
                Style::default().fg(theme.muted),
            )]),
            Line::from(vec![Span::styled(
                "  Logs stream into the 1,000-line ring-buffer as the process emits stdout/stderr.",
                Style::default().fg(theme.muted).add_modifier(Modifier::DIM),
            )]),
        ];
        let p = Paragraph::new(empty).block(block);
        frame.render_widget(p, area);
        return;
    }

    let visible_lines = inner.height as usize;
    let start_idx = state.log_scroll_offset.min(
        state
            .selected_logs
            .len()
            .saturating_sub(visible_lines.max(1)),
    );
    let end_idx = (start_idx + visible_lines).min(state.selected_logs.len());

    let lines: Vec<Line> = state.selected_logs[start_idx..end_idx]
        .iter()
        .enumerate()
        .map(|(offset, text)| {
            let line_num = start_idx + offset + 1;
            let is_err = text.to_lowercase().contains("error")
                || text.to_lowercase().contains("fatal")
                || text.to_lowercase().contains("panic");
            let is_warn = text.to_lowercase().contains("warn");

            let style = if is_err {
                Style::default().fg(theme.destructive)
            } else if is_warn {
                Style::default().fg(theme.warning)
            } else {
                Style::default().fg(theme.text_primary)
            };

            Line::from(vec![
                Span::styled(
                    format!("{:4} │ ", line_num),
                    Style::default().fg(theme.muted),
                ),
                Span::styled(text.clone(), style),
            ])
        })
        .collect();

    let p = Paragraph::new(lines).block(block);
    frame.render_widget(p, area);
}

/// Renders system runtime telemetry, metrics, and watchdog invariant guards.
fn render_system_telemetry_tab(
    frame: &mut Frame,
    state: &ProcessesModalState,
    area: Rect,
    theme: &Theme,
) {
    let vertical_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(8), // Top 4 metric cards
            Constraint::Min(6),    // Bottom 2 diagnostic panels
        ])
        .split(area);

    // Top: 4 Metric Cards
    let top_cards = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(25), // Total Processes
            Constraint::Percentage(25), // Total CPU
            Constraint::Percentage(25), // Total RAM
            Constraint::Percentage(25), // Active Ports
        ])
        .split(vertical_chunks[0]);

    let res = state.resources.clone().unwrap_or(RuntimeResourceSummary {
        total_active_processes: state.processes.len(),
        total_cpu_percent: state.processes.iter().map(|p| p.cpu_percent).sum(),
        total_memory_rss_mb: state.processes.iter().map(|p| p.memory_rss_mb).sum(),
        active_ports: {
            let mut ports: Vec<u16> = state
                .processes
                .iter()
                .flat_map(|p| p.ports.clone())
                .collect();
            ports.sort_unstable();
            ports.dedup();
            ports
        },
    });

    // Card 1: Active Processes
    let card1 = Paragraph::new(vec![
        Line::from(""),
        Line::from(vec![Span::styled(
            format!("   {}", res.total_active_processes),
            Style::default()
                .fg(theme.brand_accent)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from(vec![Span::styled(
            "   Supervised Processes",
            Style::default().fg(theme.muted),
        )]),
    ])
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.border))
            .title(" Active Processes "),
    );
    frame.render_widget(card1, top_cards[0]);

    // Card 2: CPU Utilization
    let card2 = Paragraph::new(vec![
        Line::from(""),
        Line::from(vec![Span::styled(
            format!("   {:.1}%", res.total_cpu_percent),
            Style::default()
                .fg(theme.warning)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from(vec![Span::styled(
            "   Aggregated CPU Load",
            Style::default().fg(theme.muted),
        )]),
    ])
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.border))
            .title(" Total CPU Usage "),
    );
    frame.render_widget(card2, top_cards[1]);

    // Card 3: Memory RSS
    let card3 = Paragraph::new(vec![
        Line::from(""),
        Line::from(vec![Span::styled(
            format!("   {:.1} MB", res.total_memory_rss_mb),
            Style::default()
                .fg(theme.success)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from(vec![Span::styled(
            "   Resident Set Size",
            Style::default().fg(theme.muted),
        )]),
    ])
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.border))
            .title(" Memory Footprint "),
    );
    frame.render_widget(card3, top_cards[2]);

    // Card 4: Active Listening Ports
    let ports_str = if res.active_ports.is_empty() {
        "None".to_string()
    } else {
        res.active_ports
            .iter()
            .map(|p| p.to_string())
            .collect::<Vec<_>>()
            .join(", ")
    };
    let card4 = Paragraph::new(vec![
        Line::from(""),
        Line::from(vec![Span::styled(
            format!("   {}", ports_str),
            Style::default()
                .fg(theme.brand_accent)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from(vec![Span::styled(
            "   Bound TCP Ports",
            Style::default().fg(theme.muted),
        )]),
    ])
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.border))
            .title(" Listening Ports "),
    );
    frame.render_widget(card4, top_cards[3]);

    // Bottom: 2 Diagnostic Panels
    let bottom_panels = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(50), // Watchdog Invariants
            Constraint::Percentage(50), // Port Conflict Arbitration
        ])
        .split(vertical_chunks[1]);

    let watchdog_lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled(
                "  ✔ Pure-Rust Linux Inode Discovery",
                Style::default()
                    .fg(theme.success)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                " (/proc/net/tcp + /proc/*/fd)",
                Style::default().fg(theme.muted),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                "  ✔ Zero-Orphan Isolation Guard",
                Style::default()
                    .fg(theme.success)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                " (PR_SET_PDEATHSIG + setpgid)",
                Style::default().fg(theme.muted),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                "  ✔ Two-Phase Signal Escalation",
                Style::default()
                    .fg(theme.success)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                " (SIGTERM ➜ 1000ms ➜ SIGKILL)",
                Style::default().fg(theme.muted),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                "  ✔ Crash-Loop Circuit Breaker",
                Style::default()
                    .fg(theme.success)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                " (≥3 fast crashes <3s trips Degraded)",
                Style::default().fg(theme.muted),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                "  ✔ Exponential Backoff Watchdog",
                Style::default()
                    .fg(theme.success)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                " (backoff_ms * 2^(retry - 1))",
                Style::default().fg(theme.muted),
            ),
        ]),
    ];
    let watchdog_panel = Paragraph::new(watchdog_lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.border))
            .title(" Watchdog Resilience & Process Invariants "),
    );
    frame.render_widget(watchdog_panel, bottom_panels[0]);

    let mut port_lines = vec![
        Line::from(""),
        Line::from(vec![Span::styled(
            "  Port Conflict Resolution Matrix:",
            Style::default()
                .fg(theme.brand_accent)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from(vec![
            Span::styled("  • Fallback: ", Style::default().fg(theme.warning)),
            Span::styled(
                "Auto-shifts to next open port and injects PORT=...",
                Style::default().fg(theme.text_primary),
            ),
        ]),
        Line::from(vec![
            Span::styled("  • Error:    ", Style::default().fg(theme.destructive)),
            Span::styled(
                "Fails immediately with PID & process command report",
                Style::default().fg(theme.text_primary),
            ),
        ]),
        Line::from(vec![
            Span::styled("  • Kill:     ", Style::default().fg(theme.brand_accent)),
            Span::styled(
                "Sends SIGTERM/SIGKILL to conflicting process",
                Style::default().fg(theme.text_primary),
            ),
        ]),
        Line::from(vec![
            Span::styled("  • Ignore:   ", Style::default().fg(theme.muted)),
            Span::styled(
                "Bypasses probe and attempts spawn unconditionally",
                Style::default().fg(theme.text_primary),
            ),
        ]),
    ];

    if !res.active_ports.is_empty() {
        port_lines.push(Line::from(""));
        port_lines.push(Line::from(vec![
            Span::styled(
                "  Active Port Bindings: ",
                Style::default()
                    .fg(theme.success)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(ports_str, Style::default().fg(theme.text_primary)),
        ]));
    }

    let port_panel = Paragraph::new(port_lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.border))
            .title(" Port Conflict Arbitration Matrix "),
    );
    frame.render_widget(port_panel, bottom_panels[1]);
}

/// Renders the bottom keyboard shortcuts bar.
fn render_shortcuts_footer(
    frame: &mut Frame,
    state: &ProcessesModalState,
    area: Rect,
    theme: &Theme,
) {
    let mut spans = vec![
        Span::styled(
            " [k] ",
            Style::default()
                .fg(theme.destructive)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("Stop  ", Style::default().fg(theme.muted)),
        Span::styled(
            "[r] ",
            Style::default()
                .fg(theme.warning)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("Restart  ", Style::default().fg(theme.muted)),
        Span::styled(
            "[l] ",
            Style::default()
                .fg(theme.brand_accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("Logs  ", Style::default().fg(theme.muted)),
        Span::styled(
            "[s] ",
            Style::default()
                .fg(theme.success)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("Screenshot  ", Style::default().fg(theme.muted)),
        Span::styled(
            "[Tab] ",
            Style::default()
                .fg(theme.text_primary)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("Switch Tab  ", Style::default().fg(theme.muted)),
        Span::styled(
            "[/] ",
            Style::default()
                .fg(theme.brand_accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("Filter  ", Style::default().fg(theme.muted)),
        Span::styled(
            "[Esc/F7] ",
            Style::default()
                .fg(theme.brand_accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("Close", Style::default().fg(theme.muted)),
    ];

    if state.active_tab == ProcessesTab::Logs {
        spans.insert(
            6,
            Span::styled(
                "[PgUp/PgDn] Scroll  [a] Auto-scroll  ",
                Style::default().fg(theme.brand_accent),
            ),
        );
    }

    let p = Paragraph::new(Line::from(spans)).alignment(Alignment::Center);
    frame.render_widget(p, area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dev::models::{DevProcessId, PortConflict};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use tempfile::tempdir;

    fn sample_processes() -> Vec<DevProcessSummary> {
        vec![
            DevProcessSummary {
                id: DevProcessId::from("vite-web"),
                name: "Vite Web Server".to_string(),
                process_type: DevProcessType::Frontend,
                status: DevProcessStatus::Running,
                pid: Some(12345),
                ports: vec![5173],
                url: Some("http://localhost:5173".to_string()),
                cpu_percent: 1.5,
                memory_rss_mb: 85.0,
                uptime_secs: 120,
                restart_count: 0,
                restart_policy: RestartPolicy::Never,
                port_resolution: Some(PortResolution::Unchanged { port: 5173 }),
            },
            DevProcessSummary {
                id: DevProcessId::from("backend-api"),
                name: "Axum API Backend".to_string(),
                process_type: DevProcessType::Backend,
                status: DevProcessStatus::Running,
                pid: Some(12346),
                ports: vec![8080],
                url: Some("http://localhost:8080".to_string()),
                cpu_percent: 0.8,
                memory_rss_mb: 42.0,
                uptime_secs: 240,
                restart_count: 1,
                restart_policy: RestartPolicy::on_failure_default(),
                port_resolution: Some(PortResolution::Shifted {
                    requested: 8000,
                    resolved: 8080,
                    conflict: PortConflict {
                        port: 8000,
                        conflicting_pid: Some(9999),
                        process_name: Some("old-api".to_string()),
                        command_line: Some("api --port 8000".to_string()),
                        suggested_fallback: Some(8080),
                    },
                }),
            },
            DevProcessSummary {
                id: DevProcessId::from("worker-task"),
                name: "Autonomous Subagent Worker".to_string(),
                process_type: DevProcessType::Worker,
                status: DevProcessStatus::Running,
                pid: Some(12347),
                ports: vec![],
                url: None,
                cpu_percent: 2.1,
                memory_rss_mb: 110.0,
                uptime_secs: 45,
                restart_count: 0,
                restart_policy: RestartPolicy::Never,
                port_resolution: None,
            },
        ]
    }

    #[test]
    fn test_processes_tab_cycling() {
        let tab = ProcessesTab::All;
        assert_eq!(tab.next(), ProcessesTab::Servers);
        assert_eq!(tab.next().next(), ProcessesTab::Workers);
        assert_eq!(tab.next().next().next(), ProcessesTab::Logs);
        assert_eq!(tab.next().next().next().next(), ProcessesTab::Telemetry);
        assert_eq!(tab.next().next().next().next().next(), ProcessesTab::All);

        assert_eq!(tab.prev(), ProcessesTab::Telemetry);
    }

    #[test]
    fn test_processes_filtering_by_tab() {
        let temp = tempdir().expect("tempdir");
        let mut state = ProcessesModalState::with_data(
            temp.path(),
            sample_processes(),
            None,
            vec!["Log line 1".to_string(), "Log line 2".to_string()],
        );

        // Tab: All -> 3 processes
        assert_eq!(state.filtered_indices.len(), 3);

        // Tab: Servers -> 2 processes (Vite and Axum)
        state.next_tab();
        assert_eq!(state.active_tab, ProcessesTab::Servers);
        assert_eq!(state.filtered_indices.len(), 2);

        // Tab: Workers -> 1 process
        state.next_tab();
        assert_eq!(state.active_tab, ProcessesTab::Workers);
        assert_eq!(state.filtered_indices.len(), 1);
        assert_eq!(
            state.selected_process().map(|p| p.name.as_str()),
            Some("Autonomous Subagent Worker")
        );
    }

    #[test]
    fn test_processes_search_query_filtering() {
        let temp = tempdir().expect("tempdir");
        let mut state = ProcessesModalState::with_data(
            temp.path(),
            sample_processes(),
            None,
            vec!["Server listening on port 5173".into()],
        );

        state.search_query = "axum".to_string();
        state.refresh_filtered();
        assert_eq!(state.filtered_indices.len(), 1);
        assert_eq!(
            state.selected_process().map(|p| p.name.as_str()),
            Some("Axum API Backend")
        );

        state.search_query = "5173".to_string();
        state.refresh_filtered();
        assert_eq!(state.filtered_indices.len(), 1);
        assert_eq!(
            state.selected_process().map(|p| p.name.as_str()),
            Some("Vite Web Server")
        );
    }

    #[test]
    fn test_processes_log_scrolling() {
        let temp = tempdir().expect("tempdir");
        let logs: Vec<String> = (0..50).map(|i| format!("Line {}", i)).collect();
        let mut state =
            ProcessesModalState::with_data(temp.path(), sample_processes(), None, logs.clone());

        state.set_tab(ProcessesTab::Logs);
        assert_eq!(state.selected_logs.len(), 50);

        state.scroll_logs_up(10);
        assert!(!state.auto_scroll_logs);
        assert!(state.log_scroll_offset < 49);

        state.scroll_logs_down(50);
        assert_eq!(state.log_scroll_offset, 49);
        assert!(state.auto_scroll_logs);
    }

    #[test]
    fn test_processes_modal_render_all_tabs() {
        let temp = tempdir().expect("tempdir");
        let logs = vec![
            "2026-09-26T01:00:00 [info] Server initialized".into(),
            "2026-09-26T01:00:01 [info] Ready on http://localhost:5173/".into(),
        ];
        let state = ProcessesModalState::with_data(temp.path(), sample_processes(), None, logs);
        let theme = Theme::aura_dark();

        let backend = TestBackend::new(140, 45);
        let mut terminal = Terminal::new(backend).expect("terminal");

        for tab in ProcessesTab::all() {
            let mut tab_state = state.clone();
            tab_state.active_tab = *tab;
            terminal
                .draw(|f| {
                    let area = f.area();
                    render_processes_modal(f, &tab_state, area, &theme);
                })
                .expect("draw must succeed");
        }
    }
}

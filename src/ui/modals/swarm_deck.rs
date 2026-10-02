//! Interactive Swarm Flight Deck TUI Modal.
//!
//! Provides real-time visibility and control over autonomous multi-agent swarms,
//! isolated Git worktree workers, and inter-worker message bus (`bus.jsonl`).
//!
//! Supports 5 selectable, 100% theme-adaptive visual layouts:
//! 1. `stylish`  - Minimal + Stylish (Default ⭐)
//! 2. `gitgraph` - Minimal + GitGraph Pipeline (Topological branch DAG)
//! 3. `modern`   - Clean Modernist (Floating card boxes with status pills)
//! 4. `minimal`  - Ultra-Minimalist (Zero-border pure whitespace tree)
//! 5. `cockpit`  - High-Density Cockpit (3-column split view: Fleet, Bus, Stdout)

#![allow(dead_code)]

use ratatui::layout::{Constraint, Direction, Layout, Margin, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use ratatui::Frame;
use std::path::Path;

use crate::agent::swarm::bus::{SwarmMessage, SwarmMessageBus, SwarmMessageIntent};
use crate::agent::swarm::models::{SwarmExecutionState, SwarmPlan};
use crate::config::SwarmDashboardStyle;
use crate::dev::models::{DevProcessStatus, DevProcessSummary};
use crate::ui::theme::Theme;

/// Aggregated data context for the Swarm Flight Deck.
#[derive(Debug, Clone)]
pub struct SwarmDeckData {
    pub swarms: Vec<DevProcessSummary>,
    pub workers: Vec<DevProcessSummary>,
    pub selected_index: usize,
    pub style: SwarmDashboardStyle,
    pub recent_messages: Vec<SwarmMessage>,
    pub plan: Option<SwarmPlan>,
    pub state: Option<SwarmExecutionState>,
    pub selected_worker_logs: Vec<String>,
    pub status_message: Option<String>,
}

impl SwarmDeckData {
    /// Loads the live swarm execution state, worker processes, and message bus from the registry and disk.
    pub async fn load(
        workspace_root: &Path,
        selected_index: usize,
        style: SwarmDashboardStyle,
    ) -> Self {
        let registry = crate::dev::registry::get_global_dev_registry();
        let all_swarms = registry.list_swarms().await;

        let mut swarms = Vec::new();
        let mut workers = Vec::new();

        for p in all_swarms {
            if p.id.as_str().starts_with("swarm-worker-") {
                workers.push(p);
            } else {
                swarms.push(p);
            }
        }

        workers.sort_by(|a, b| a.id.as_str().cmp(b.id.as_str()));

        // Locate active/most recent swarm directory in .minicode/swarms/
        let swarms_dir = workspace_root.join(".minicode").join("swarms");
        let mut recent_swarm_dir = None;

        if let Some(s) = swarms.first() {
            let id =
                s.id.as_str()
                    .strip_prefix("swarm-")
                    .unwrap_or(s.id.as_str());
            let dir = swarms_dir.join(id);
            if dir.exists() {
                recent_swarm_dir = Some(dir);
            }
        }

        if recent_swarm_dir.is_none() && swarms_dir.exists() {
            if let Ok(entries) = std::fs::read_dir(&swarms_dir) {
                let mut dirs: Vec<_> = entries
                    .flatten()
                    .filter(|e| e.path().is_dir())
                    .map(|e| e.path())
                    .collect();
                dirs.sort_by_key(|p| p.metadata().and_then(|m| m.modified()).ok());
                recent_swarm_dir = dirs.pop();
            }
        }

        let mut plan = None;
        let mut state = None;
        let mut recent_messages = Vec::new();

        if let Some(ref dir) = recent_swarm_dir {
            if let Ok(content) = std::fs::read_to_string(dir.join("plan.json")) {
                plan = serde_json::from_str(&content).ok();
            }
            if let Ok(content) = std::fs::read_to_string(dir.join("state.json")) {
                state = serde_json::from_str(&content).ok();
            }
            if let Ok(bus) = SwarmMessageBus::new(dir) {
                if let Ok(msgs) = bus.all_messages() {
                    recent_messages = msgs;
                }
            }
        }

        let clamped_index = if workers.is_empty() {
            0
        } else {
            selected_index.min(workers.len() - 1)
        };

        let mut selected_worker_logs = Vec::new();
        if let Some(w) = workers.get(clamped_index) {
            if let Ok(logs) = registry.logs(&w.id, 50, None).await {
                selected_worker_logs = logs;
            }
        } else if let Some(s) = swarms.first() {
            if let Ok(logs) = registry.logs(&s.id, 50, None).await {
                selected_worker_logs = logs;
            }
        }

        Self {
            swarms,
            workers,
            selected_index: clamped_index,
            style,
            recent_messages,
            plan,
            state,
            selected_worker_logs,
            status_message: None,
        }
    }

    pub fn select_prev(&mut self) {
        if self.workers.is_empty() {
            self.selected_index = 0;
            return;
        }
        if self.selected_index == 0 {
            self.selected_index = self.workers.len().saturating_sub(1);
        } else {
            self.selected_index -= 1;
        }
    }

    pub fn select_next(&mut self) {
        if self.workers.is_empty() {
            self.selected_index = 0;
            return;
        }
        if self.selected_index + 1 >= self.workers.len() {
            self.selected_index = 0;
        } else {
            self.selected_index += 1;
        }
    }

    pub fn selected_worker(&self) -> Option<&DevProcessSummary> {
        self.workers
            .get(self.selected_index)
            .or_else(|| self.swarms.first())
    }

    pub fn cycle_style(&mut self) -> SwarmDashboardStyle {
        self.style = self.style.next();
        self.style
    }
}

/// Dispatches rendering of the Swarm Flight Deck based on the active visual style.
pub fn render_swarm_flight_deck(
    frame: &mut Frame,
    area: Rect,
    theme: &Theme,
    data: &SwarmDeckData,
) {
    match data.style {
        SwarmDashboardStyle::Stylish => render_stylish(frame, area, theme, data),
        SwarmDashboardStyle::GitGraph => render_gitgraph(frame, area, theme, data),
        SwarmDashboardStyle::Modern => render_modern(frame, area, theme, data),
        SwarmDashboardStyle::Minimal => render_minimal(frame, area, theme, data),
        SwarmDashboardStyle::Cockpit => render_cockpit(frame, area, theme, data),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 1. STYLE: STYLISH (Minimal + Stylish — DEFAULT ⭐)
// ─────────────────────────────────────────────────────────────────────────────

pub fn render_stylish(frame: &mut Frame, area: Rect, theme: &Theme, data: &SwarmDeckData) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Plain)
        .border_style(Style::default().fg(theme.border))
        .style(Style::default().bg(theme.bg_primary));
    frame.render_widget(block, area);

    let inner = area.inner(Margin {
        vertical: 1,
        horizontal: 2,
    });
    if inner.height < 6 || inner.width < 20 {
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // Header
            Constraint::Min(6),    // Main Body (Fleet + Log preview)
            Constraint::Length(7), // Bus Tail
            Constraint::Length(1), // Footer
        ])
        .split(inner);

    // 1. Header Banner
    let active_workers = data.workers.iter().filter(|w| w.status.is_alive()).count();
    let swarm_title = data
        .plan
        .as_ref()
        .map(|p| p.title.as_str())
        .unwrap_or("Autonomous Multi-Agent Swarm");

    let header_line = Line::from(vec![
        Span::styled(
            "⚡ SWARM FLIGHT DECK ",
            Style::default()
                .fg(theme.brand_accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("• ", Style::default().fg(theme.muted)),
        Span::styled(
            format!("{} ", swarm_title),
            Style::default()
                .fg(theme.text_primary)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("• ", Style::default().fg(theme.muted)),
        Span::styled(
            format!("{} Active Workers", active_workers),
            Style::default().fg(theme.success),
        ),
    ]);
    frame.render_widget(Paragraph::new(header_line), chunks[0]);

    // 2. Main Body Split: Left (Fleet) & Right (Selected Worker Log)
    let body_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(chunks[1]);

    // Left: Worker Fleet
    let mut fleet_lines = Vec::new();
    if data.workers.is_empty() {
        fleet_lines.push(Line::from(vec![Span::styled(
            "  ℹ No active swarm workers registered.",
            Style::default().fg(theme.muted),
        )]));
    } else {
        for (i, w) in data.workers.iter().enumerate() {
            let is_selected = i == data.selected_index;
            let cursor = if is_selected { "❯ " } else { "  " };
            let cursor_style = if is_selected {
                Style::default()
                    .fg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.muted)
            };

            let (status_badge, status_color) = match &w.status {
                DevProcessStatus::Running | DevProcessStatus::Healthy => {
                    ("[RUNNING]", theme.success)
                }
                DevProcessStatus::Degraded(_) => ("[DEGRADED]", theme.warning),
                DevProcessStatus::Stopped => ("[COMPLETED]", theme.info),
                DevProcessStatus::Exited(Some(0)) => ("[DONE]", theme.info),
                DevProcessStatus::Exited(_) => ("[FAILED]", theme.destructive),
                DevProcessStatus::Killed => ("[KILLED]", theme.destructive),
            };

            let name_style = if is_selected {
                Style::default()
                    .fg(theme.text_primary)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.text_primary)
            };

            let short_id =
                w.id.as_str()
                    .strip_prefix("swarm-worker-")
                    .unwrap_or(w.id.as_str());

            fleet_lines.push(Line::from(vec![
                Span::styled(cursor, cursor_style),
                Span::styled(
                    format!("{:<10} ", short_id),
                    Style::default().fg(theme.brand_accent),
                ),
                Span::styled(
                    format!("{:<11} ", status_badge),
                    Style::default()
                        .fg(status_color)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!("{:<22}", w.name), name_style),
            ]));

            let metrics = format!(
                "    └─ CPU: {:>4.1}%  RSS: {:>4.1}MB  Uptime: {}s",
                w.cpu_percent, w.memory_rss_mb, w.uptime_secs
            );
            fleet_lines.push(Line::from(vec![Span::styled(
                metrics,
                Style::default().fg(theme.muted),
            )]));
        }
    }

    let fleet_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border))
        .title(Span::styled(
            " 🐝 Worker Fleet ",
            Style::default()
                .fg(theme.brand_accent)
                .add_modifier(Modifier::BOLD),
        ));
    frame.render_widget(
        Paragraph::new(fleet_lines).block(fleet_block),
        body_chunks[0],
    );

    // Right: Worker Output / Stdout preview
    let log_title = if let Some(w) = data.workers.get(data.selected_index) {
        format!(" Log Tail: {} ", w.id)
    } else {
        " Worker Output ".to_string()
    };
    let log_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border))
        .title(Span::styled(
            log_title,
            Style::default().fg(theme.info).add_modifier(Modifier::BOLD),
        ));

    let mut log_lines = Vec::new();
    if data.selected_worker_logs.is_empty() {
        log_lines.push(Line::from(Span::styled(
            "  No log output recorded yet for this worker.",
            Style::default().fg(theme.muted),
        )));
    } else {
        let tail_count = body_chunks[1].height.saturating_sub(2) as usize;
        let start = data.selected_worker_logs.len().saturating_sub(tail_count);
        for l in &data.selected_worker_logs[start..] {
            log_lines.push(Line::from(Span::styled(
                format!(" {}", l),
                Style::default().fg(theme.text_primary),
            )));
        }
    }
    frame.render_widget(Paragraph::new(log_lines).block(log_block), body_chunks[1]);

    // 3. Message Bus Tail
    let bus_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border))
        .title(Span::styled(
            " 💬 Inter-Worker Message Bus (bus.jsonl) ",
            Style::default()
                .fg(theme.warning)
                .add_modifier(Modifier::BOLD),
        ));

    let mut bus_lines = Vec::new();
    if data.recent_messages.is_empty() {
        bus_lines.push(Line::from(Span::styled(
            "  ℹ No messages on swarm bus yet. Workers coordinate contracts via send_worker_message.",
            Style::default().fg(theme.muted),
        )));
    } else {
        let max_msgs = chunks[2].height.saturating_sub(2) as usize;
        let start = data.recent_messages.len().saturating_sub(max_msgs);
        for m in &data.recent_messages[start..] {
            let (badge, color) = match m.intent {
                SwarmMessageIntent::PublishContract => ("📜 Contract", theme.brand_accent),
                SwarmMessageIntent::QueryInterface => ("❓ Query   ", theme.warning),
                SwarmMessageIntent::CoordinationNote => ("📢 Note    ", theme.info),
            };
            let target = m.to_task.as_deref().unwrap_or("broadcast");
            let line = Line::from(vec![
                Span::styled(
                    format!("  [{}] ", badge),
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("{} ➔ {}: ", m.from_task, target),
                    Style::default().fg(theme.text_primary),
                ),
                Span::styled(
                    format!("\"{}\" ", m.topic),
                    Style::default()
                        .fg(theme.highlight)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("- {}", m.payload.lines().next().unwrap_or("")),
                    Style::default().fg(theme.muted),
                ),
            ]);
            bus_lines.push(line);
        }
    }
    frame.render_widget(Paragraph::new(bus_lines).block(bus_block), chunks[2]);

    // 4. Footer
    let footer_line = Line::from(vec![
        Span::styled("[↑/↓] ", Style::default().fg(theme.brand_accent)),
        Span::styled("Select Worker  ", Style::default().fg(theme.text_primary)),
        Span::styled("[Space] ", Style::default().fg(theme.brand_accent)),
        Span::styled("Logs  ", Style::default().fg(theme.text_primary)),
        Span::styled("[m] ", Style::default().fg(theme.brand_accent)),
        Span::styled("Message Bus  ", Style::default().fg(theme.text_primary)),
        Span::styled("[k] ", Style::default().fg(theme.destructive)),
        Span::styled("Kill  ", Style::default().fg(theme.text_primary)),
        Span::styled("[s] ", Style::default().fg(theme.info)),
        Span::styled("Cycle Style  ", Style::default().fg(theme.text_primary)),
        Span::styled("[Esc] ", Style::default().fg(theme.muted)),
        Span::styled("Close", Style::default().fg(theme.muted)),
    ]);
    frame.render_widget(Paragraph::new(footer_line), chunks[3]);
}

// ─────────────────────────────────────────────────────────────────────────────
// 2. STYLE: GITGRAPH (Minimal + GitGraph Pipeline)
// ─────────────────────────────────────────────────────────────────────────────

pub fn render_gitgraph(frame: &mut Frame, area: Rect, theme: &Theme, data: &SwarmDeckData) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Plain)
        .border_style(Style::default().fg(theme.border))
        .style(Style::default().bg(theme.bg_primary));
    frame.render_widget(block, area);

    let inner = area.inner(Margin {
        vertical: 1,
        horizontal: 2,
    });
    if inner.height < 6 || inner.width < 20 {
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // Title
            Constraint::Min(8),    // DAG Pipeline
            Constraint::Length(6), // Node Inspector Card
            Constraint::Length(1), // Footer
        ])
        .split(inner);

    // Title
    let title_line = Line::from(vec![
        Span::styled(
            "🌿 SWARM PIPELINE (GITGRAPH) ",
            Style::default()
                .fg(theme.success)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("• ", Style::default().fg(theme.muted)),
        Span::styled(
            "Topological Worktree Branch DAG & Inter-Worker Contracts",
            Style::default().fg(theme.text_primary),
        ),
    ]);
    frame.render_widget(Paragraph::new(title_line), chunks[0]);

    // Topological Pipeline lines
    let mut dag_lines = Vec::new();
    dag_lines.push(Line::from(vec![
        Span::styled("  ● ", Style::default().fg(theme.brand_accent)),
        Span::styled(
            "[HEAD: main] Base Workspace Commit",
            Style::default()
                .fg(theme.text_primary)
                .add_modifier(Modifier::BOLD),
        ),
    ]));

    if data.workers.is_empty() {
        dag_lines.push(Line::from(vec![
            Span::styled("  │\n  └───○ ", Style::default().fg(theme.muted)),
            Span::styled(
                "No worker branches active",
                Style::default().fg(theme.muted),
            ),
        ]));
    } else {
        for (i, w) in data.workers.iter().enumerate() {
            let is_selected = i == data.selected_index;
            let marker = if is_selected { "● " } else { "○ " };
            let marker_style = if is_selected {
                Style::default()
                    .fg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.info)
            };

            let short_id =
                w.id.as_str()
                    .strip_prefix("swarm-worker-")
                    .unwrap_or(w.id.as_str());

            dag_lines.push(Line::from(vec![
                Span::styled("  │ ├───", Style::default().fg(theme.muted)),
                Span::styled(marker, marker_style),
                Span::styled(
                    format!("[{}] ", short_id),
                    Style::default()
                        .fg(theme.brand_accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("{} ", w.name),
                    if is_selected {
                        Style::default()
                            .fg(theme.text_primary)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(theme.text_primary)
                    },
                ),
                Span::styled(
                    format!("(branch: {})", w.id),
                    Style::default().fg(theme.muted),
                ),
            ]));

            // If there are messages from this worker, render comms arc
            let out_msgs: Vec<_> = data
                .recent_messages
                .iter()
                .filter(|m| m.from_task.contains(short_id) || short_id.contains(&m.from_task))
                .collect();
            for msg in out_msgs.iter().take(2) {
                let to_target = msg.to_task.as_deref().unwrap_or("broadcast");
                dag_lines.push(Line::from(vec![
                    Span::styled("  │ │   ├───💬 ", Style::default().fg(theme.warning)),
                    Span::styled(
                        format!("{} ──▶ {}: ", msg.from_task, to_target),
                        Style::default().fg(theme.warning),
                    ),
                    Span::styled(
                        format!("\"{}\"", msg.topic),
                        Style::default().fg(theme.text_primary),
                    ),
                ]));
            }
        }

        dag_lines.push(Line::from(vec![
            Span::styled("  ├───● ", Style::default().fg(theme.success)),
            Span::styled(
                "[auto-merge] Synchronization Barrier & Merge Arbitration",
                Style::default().fg(theme.success),
            ),
        ]));
    }

    let dag_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border))
        .title(Span::styled(
            " 🌿 Pipeline Tree ",
            Style::default()
                .fg(theme.success)
                .add_modifier(Modifier::BOLD),
        ));
    frame.render_widget(Paragraph::new(dag_lines).block(dag_block), chunks[1]);

    // Inspector Card
    let inspector_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border))
        .title(Span::styled(
            " 🔍 Focused Node Inspector ",
            Style::default().fg(theme.info).add_modifier(Modifier::BOLD),
        ));

    let mut inspector_lines = Vec::new();
    if let Some(w) = data.workers.get(data.selected_index) {
        inspector_lines.push(Line::from(vec![
            Span::styled("  Node ID: ", Style::default().fg(theme.muted)),
            Span::styled(
                format!("{:<20}", w.id),
                Style::default()
                    .fg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("  Role: ", Style::default().fg(theme.muted)),
            Span::styled(w.name.as_str(), Style::default().fg(theme.text_primary)),
        ]));
        inspector_lines.push(Line::from(vec![
            Span::styled("  Status:  ", Style::default().fg(theme.muted)),
            Span::styled(
                format!("{:?}", w.status),
                Style::default().fg(theme.success),
            ),
            Span::styled("  PID: ", Style::default().fg(theme.muted)),
            Span::styled(
                format!("{:<8}", w.pid.unwrap_or(0)),
                Style::default().fg(theme.text_primary),
            ),
            Span::styled("  CPU: ", Style::default().fg(theme.muted)),
            Span::styled(
                format!("{:>4.1}%", w.cpu_percent),
                Style::default().fg(theme.text_primary),
            ),
            Span::styled("  RSS: ", Style::default().fg(theme.muted)),
            Span::styled(
                format!("{:>4.1} MB", w.memory_rss_mb),
                Style::default().fg(theme.text_primary),
            ),
        ]));
        inspector_lines.push(Line::from(vec![
            Span::styled("  Working Dir: ", Style::default().fg(theme.muted)),
            Span::styled(
                w.url.as_deref().unwrap_or("-"),
                Style::default().fg(theme.muted),
            ),
        ]));
    } else {
        inspector_lines.push(Line::from(Span::styled(
            "  Select a pipeline node with [↑/↓] to inspect details.",
            Style::default().fg(theme.muted),
        )));
    }
    frame.render_widget(
        Paragraph::new(inspector_lines).block(inspector_block),
        chunks[2],
    );

    // Footer
    let footer_line = Line::from(vec![
        Span::styled("[↑/↓] ", Style::default().fg(theme.brand_accent)),
        Span::styled("Navigate DAG  ", Style::default().fg(theme.text_primary)),
        Span::styled("[Space] ", Style::default().fg(theme.brand_accent)),
        Span::styled("Inspect Logs  ", Style::default().fg(theme.text_primary)),
        Span::styled("[s] ", Style::default().fg(theme.info)),
        Span::styled("Cycle Style  ", Style::default().fg(theme.text_primary)),
        Span::styled("[Esc] ", Style::default().fg(theme.muted)),
        Span::styled("Close", Style::default().fg(theme.muted)),
    ]);
    frame.render_widget(Paragraph::new(footer_line), chunks[3]);
}

// ─────────────────────────────────────────────────────────────────────────────
// 3. STYLE: MODERN (Clean Modernist)
// ─────────────────────────────────────────────────────────────────────────────

pub fn render_modern(frame: &mut Frame, area: Rect, theme: &Theme, data: &SwarmDeckData) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border))
        .style(Style::default().bg(theme.bg_primary));
    frame.render_widget(block, area);

    let inner = area.inner(Margin {
        vertical: 1,
        horizontal: 2,
    });
    if inner.height < 6 || inner.width < 20 {
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // Modern Header
            Constraint::Min(7),    // Floating Cards
            Constraint::Length(6), // Modern Feed
            Constraint::Length(1), // Footer
        ])
        .split(inner);

    // Header
    let header_line = Line::from(vec![
        Span::styled(
            "◈ SWARM MISSION CONTROL (MODERN) ",
            Style::default().fg(theme.info).add_modifier(Modifier::BOLD),
        ),
        Span::styled("• ", Style::default().fg(theme.muted)),
        Span::styled("STATUS: OPERATIONAL", Style::default().fg(theme.success)),
    ]);
    frame.render_widget(Paragraph::new(header_line), chunks[0]);

    // Floating Worker Cards (Render 1 to 3 side-by-side or vertical list)
    let card_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(chunks[1]);

    for col in 0..2 {
        let mut card_lines = Vec::new();
        let worker_opt = data.workers.get(col);
        let is_selected = col == data.selected_index;

        let btype = if is_selected {
            BorderType::Double
        } else {
            BorderType::Rounded
        };
        let bcolor = if is_selected {
            theme.brand_accent
        } else {
            theme.border
        };

        let card_title = if let Some(w) = worker_opt {
            let short_id =
                w.id.as_str()
                    .strip_prefix("swarm-worker-")
                    .unwrap_or(w.id.as_str());
            format!(" Worker: {} ", short_id)
        } else {
            " Worker Slot (Idle) ".to_string()
        };

        if let Some(w) = worker_opt {
            card_lines.push(Line::from(vec![
                Span::styled(
                    "  [ RUNNING ] ",
                    Style::default()
                        .fg(theme.success)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    w.name.as_str(),
                    Style::default()
                        .fg(theme.text_primary)
                        .add_modifier(Modifier::BOLD),
                ),
            ]));
            card_lines.push(Line::from(""));
            card_lines.push(Line::from(vec![
                Span::styled("  ⚡ CPU: ", Style::default().fg(theme.muted)),
                Span::styled(
                    format!("{:.1}%   ", w.cpu_percent),
                    Style::default().fg(theme.text_primary),
                ),
                Span::styled("💾 RAM: ", Style::default().fg(theme.muted)),
                Span::styled(
                    format!("{:.1} MB   ", w.memory_rss_mb),
                    Style::default().fg(theme.text_primary),
                ),
                Span::styled("⏱ Time: ", Style::default().fg(theme.muted)),
                Span::styled(
                    format!("{}s", w.uptime_secs),
                    Style::default().fg(theme.text_primary),
                ),
            ]));
        } else {
            card_lines.push(Line::from(Span::styled(
                "  No active worker assigned in this slot.",
                Style::default().fg(theme.muted),
            )));
        }

        let card_block = Block::default()
            .borders(Borders::ALL)
            .border_type(btype)
            .border_style(Style::default().fg(bcolor))
            .title(Span::styled(
                card_title,
                Style::default().fg(bcolor).add_modifier(Modifier::BOLD),
            ));
        frame.render_widget(Paragraph::new(card_lines).block(card_block), card_cols[col]);
    }

    // Modern Feed
    let feed_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border))
        .title(Span::styled(
            " 💬 Live Message Feed ",
            Style::default()
                .fg(theme.brand_accent)
                .add_modifier(Modifier::BOLD),
        ));

    let mut feed_lines = Vec::new();
    if data.recent_messages.is_empty() {
        feed_lines.push(Line::from(Span::styled(
            "  No communications recorded.",
            Style::default().fg(theme.muted),
        )));
    } else {
        for m in data.recent_messages.iter().rev().take(3) {
            feed_lines.push(Line::from(vec![
                Span::styled(
                    format!("  [{}] ", m.intent.badge()),
                    Style::default().fg(theme.warning),
                ),
                Span::styled(
                    format!(
                        "{} ➔ {}: ",
                        m.from_task,
                        m.to_task.as_deref().unwrap_or("*")
                    ),
                    Style::default().fg(theme.text_primary),
                ),
                Span::styled(
                    m.payload.lines().next().unwrap_or(""),
                    Style::default().fg(theme.muted),
                ),
            ]));
        }
    }
    frame.render_widget(Paragraph::new(feed_lines).block(feed_block), chunks[2]);

    // Footer
    let footer_line = Line::from(vec![
        Span::styled("[1/2] ", Style::default().fg(theme.brand_accent)),
        Span::styled("Select Card  ", Style::default().fg(theme.text_primary)),
        Span::styled("[s] ", Style::default().fg(theme.info)),
        Span::styled("Cycle Style  ", Style::default().fg(theme.text_primary)),
        Span::styled("[Esc] ", Style::default().fg(theme.muted)),
        Span::styled("Close", Style::default().fg(theme.muted)),
    ]);
    frame.render_widget(Paragraph::new(footer_line), chunks[3]);
}

// ─────────────────────────────────────────────────────────────────────────────
// 4. STYLE: MINIMAL (Ultra-Minimalist)
// ─────────────────────────────────────────────────────────────────────────────

pub fn render_minimal(frame: &mut Frame, area: Rect, theme: &Theme, data: &SwarmDeckData) {
    // Pure whitespace - zero container borders
    let clear_block = Block::default().style(Style::default().bg(theme.bg_primary));
    frame.render_widget(clear_block, area);

    let inner = area.inner(Margin {
        vertical: 1,
        horizontal: 3,
    });
    if inner.height < 4 || inner.width < 20 {
        return;
    }

    let mut lines = Vec::new();

    // Minimal header
    lines.push(Line::from(vec![
        Span::styled(
            "swarm",
            Style::default()
                .fg(theme.brand_accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" / ", Style::default().fg(theme.muted)),
        Span::styled(
            data.plan
                .as_ref()
                .map(|p| p.id.as_str())
                .unwrap_or("active-run"),
            Style::default().fg(theme.text_primary),
        ),
    ]));
    lines.push(Line::from(Span::styled(
        "  │",
        Style::default().fg(theme.muted),
    )));

    // Workers tree
    if data.workers.is_empty() {
        lines.push(Line::from(vec![
            Span::styled("  ├─ ", Style::default().fg(theme.muted)),
            Span::styled("no active workers", Style::default().fg(theme.muted)),
        ]));
    } else {
        for (i, w) in data.workers.iter().enumerate() {
            let is_selected = i == data.selected_index;
            let marker = if is_selected { "▶ " } else { "├─ " };
            let marker_style = if is_selected {
                Style::default()
                    .fg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.muted)
            };

            let short_id =
                w.id.as_str()
                    .strip_prefix("swarm-worker-")
                    .unwrap_or(w.id.as_str());

            lines.push(Line::from(vec![
                Span::styled(format!("  {}", marker), marker_style),
                Span::styled(
                    format!("{:<14}", short_id),
                    Style::default().fg(theme.text_primary),
                ),
                Span::styled(
                    format!("{:<10}", format!("{:?}", w.status).to_lowercase()),
                    Style::default().fg(theme.success),
                ),
                Span::styled(
                    format!(
                        "· {:>3.1}% cpu · {:>4.1}mb · {}s",
                        w.cpu_percent, w.memory_rss_mb, w.uptime_secs
                    ),
                    Style::default().fg(theme.muted),
                ),
            ]));
        }
    }

    lines.push(Line::from(Span::styled(
        "  │",
        Style::default().fg(theme.muted),
    )));
    lines.push(Line::from(vec![
        Span::styled("  └─ ", Style::default().fg(theme.muted)),
        Span::styled("bus transactions", Style::default().fg(theme.info)),
    ]));

    if data.recent_messages.is_empty() {
        lines.push(Line::from(vec![
            Span::styled("     └─ ", Style::default().fg(theme.muted)),
            Span::styled("idle", Style::default().fg(theme.muted)),
        ]));
    } else {
        for (idx, m) in data.recent_messages.iter().rev().take(3).enumerate() {
            let prefix = if idx == 2 {
                "     └─ "
            } else {
                "     ├─ "
            };
            lines.push(Line::from(vec![
                Span::styled(prefix, Style::default().fg(theme.muted)),
                Span::styled(
                    format!(
                        "{} ──▶ {}: ",
                        m.from_task,
                        m.to_task.as_deref().unwrap_or("*")
                    ),
                    Style::default().fg(theme.text_primary),
                ),
                Span::styled(
                    format!("\"{}\"", m.topic),
                    Style::default().fg(theme.highlight),
                ),
            ]));
        }
    }

    frame.render_widget(Paragraph::new(lines), inner);
}

// ─────────────────────────────────────────────────────────────────────────────
// 5. STYLE: COCKPIT (High-Density Cockpit)
// ─────────────────────────────────────────────────────────────────────────────

pub fn render_cockpit(frame: &mut Frame, area: Rect, theme: &Theme, data: &SwarmDeckData) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Plain)
        .border_style(Style::default().fg(theme.border))
        .style(Style::default().bg(theme.bg_primary));
    frame.render_widget(block, area);

    let inner = area.inner(Margin {
        vertical: 1,
        horizontal: 1,
    });
    if inner.height < 6 || inner.width < 25 {
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Avionics Strip
            Constraint::Min(6),    // 3-Column Split
            Constraint::Length(1), // Footer
        ])
        .split(inner);

    // Avionics Strip
    let active_count = data.workers.iter().filter(|w| w.status.is_alive()).count();
    let total_cpu: f32 = data.workers.iter().map(|w| w.cpu_percent).sum();
    let total_rss: f32 = data.workers.iter().map(|w| w.memory_rss_mb).sum();

    let avionics_line = Line::from(vec![
        Span::styled(
            "[ COCKPIT V3 ] ",
            Style::default()
                .fg(theme.brand_accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("FLEET: {} AGENTS  ", active_count),
            Style::default().fg(theme.text_primary),
        ),
        Span::styled("•  ", Style::default().fg(theme.muted)),
        Span::styled(
            format!("TOTAL CPU: {:>4.1}%  ", total_cpu),
            Style::default().fg(theme.success),
        ),
        Span::styled("•  ", Style::default().fg(theme.muted)),
        Span::styled(
            format!("TOTAL RSS: {:>5.1} MB  ", total_rss),
            Style::default().fg(theme.info),
        ),
    ]);
    frame.render_widget(Paragraph::new(avionics_line), chunks[0]);

    // 3-Column Split
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(32), // Worker Pool
            Constraint::Percentage(34), // Message Bus
            Constraint::Percentage(34), // Live Logs
        ])
        .split(chunks[1]);

    // Col 1: Worker Pool
    let pool_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border))
        .title(Span::styled(
            " [ WORKER POOL ] ",
            Style::default()
                .fg(theme.brand_accent)
                .add_modifier(Modifier::BOLD),
        ));

    let mut pool_lines = Vec::new();
    for (i, w) in data.workers.iter().enumerate() {
        let is_selected = i == data.selected_index;
        let cursor = if is_selected { "▶ " } else { "  " };
        let short_id =
            w.id.as_str()
                .strip_prefix("swarm-worker-")
                .unwrap_or(w.id.as_str());

        pool_lines.push(Line::from(vec![
            Span::styled(cursor, Style::default().fg(theme.brand_accent)),
            Span::styled(
                format!("{:<8} ", short_id),
                Style::default()
                    .fg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{:<8}", format!("{:?}", w.status)),
                Style::default().fg(theme.success),
            ),
        ]));
        pool_lines.push(Line::from(vec![Span::styled(
            format!(
                "   {:>3.1}% CPU  {:>4.1}M  {}s",
                w.cpu_percent, w.memory_rss_mb, w.uptime_secs
            ),
            Style::default().fg(theme.muted),
        )]));
    }
    frame.render_widget(Paragraph::new(pool_lines).block(pool_block), cols[0]);

    // Col 2: Message Bus Tail
    let bus_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border))
        .title(Span::styled(
            " [ MESSAGE BUS ] ",
            Style::default()
                .fg(theme.warning)
                .add_modifier(Modifier::BOLD),
        ));

    let mut bus_lines = Vec::new();
    for m in data.recent_messages.iter().rev().take(10) {
        bus_lines.push(Line::from(vec![
            Span::styled(
                format!("{:<10} ", m.intent.badge()),
                Style::default().fg(theme.warning),
            ),
            Span::styled(
                format!("{}➔{}: ", m.from_task, m.to_task.as_deref().unwrap_or("*")),
                Style::default().fg(theme.text_primary),
            ),
            Span::styled(m.topic.as_str(), Style::default().fg(theme.highlight)),
        ]));
    }
    frame.render_widget(Paragraph::new(bus_lines).block(bus_block), cols[1]);

    // Col 3: Live Logs
    let log_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border))
        .title(Span::styled(
            " [ LIVE STDOUT ] ",
            Style::default().fg(theme.info).add_modifier(Modifier::BOLD),
        ));

    let mut log_lines = Vec::new();
    for l in data.selected_worker_logs.iter().rev().take(10) {
        log_lines.push(Line::from(Span::styled(
            l.as_str(),
            Style::default().fg(theme.text_primary),
        )));
    }
    frame.render_widget(Paragraph::new(log_lines).block(log_block), cols[2]);

    // Footer
    let footer_line = Line::from(vec![
        Span::styled("[↑/↓] ", Style::default().fg(theme.brand_accent)),
        Span::styled("Select  ", Style::default().fg(theme.text_primary)),
        Span::styled("[s] ", Style::default().fg(theme.info)),
        Span::styled("Style  ", Style::default().fg(theme.text_primary)),
        Span::styled("[Esc] ", Style::default().fg(theme.muted)),
        Span::styled("Back", Style::default().fg(theme.muted)),
    ]);
    frame.render_widget(Paragraph::new(footer_line), chunks[2]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use tempfile::tempdir;

    fn make_test_data(style: SwarmDashboardStyle) -> SwarmDeckData {
        SwarmDeckData {
            swarms: vec![DevProcessSummary {
                id: crate::dev::models::DevProcessId::from("swarm-test-dag"),
                name: "Swarm DAG: Math Engine".to_string(),
                process_type: crate::dev::models::DevProcessType::Swarm,
                status: DevProcessStatus::Running,
                pid: Some(100),
                ports: Vec::new(),
                url: None,
                cpu_percent: 1.5,
                memory_rss_mb: 50.0,
                uptime_secs: 20,
                restart_count: 0,
                restart_policy: crate::dev::models::RestartPolicy::Never,
                port_resolution: None,
                schedule_info: None,
            }],
            workers: vec![
                DevProcessSummary {
                    id: crate::dev::models::DevProcessId::from("swarm-worker-t1"),
                    name: "Worker t1: Calculator".to_string(),
                    process_type: crate::dev::models::DevProcessType::Swarm,
                    status: DevProcessStatus::Running,
                    pid: Some(101),
                    ports: Vec::new(),
                    url: None,
                    cpu_percent: 2.0,
                    memory_rss_mb: 45.0,
                    uptime_secs: 15,
                    restart_count: 0,
                    restart_policy: crate::dev::models::RestartPolicy::Never,
                    port_resolution: None,
                    schedule_info: None,
                },
                DevProcessSummary {
                    id: crate::dev::models::DevProcessId::from("swarm-worker-t2"),
                    name: "Worker t2: Formatter".to_string(),
                    process_type: crate::dev::models::DevProcessType::Swarm,
                    status: DevProcessStatus::Running,
                    pid: Some(102),
                    ports: Vec::new(),
                    url: None,
                    cpu_percent: 1.0,
                    memory_rss_mb: 40.0,
                    uptime_secs: 10,
                    restart_count: 0,
                    restart_policy: crate::dev::models::RestartPolicy::Never,
                    port_resolution: None,
                    schedule_info: None,
                },
            ],
            selected_index: 0,
            style,
            recent_messages: vec![SwarmMessage::new(
                "swarm-test-dag",
                "t1",
                Some("t2"),
                SwarmMessageIntent::PublishContract,
                "CalcResult",
                "export interface CalcResult { sum: number; }",
            )],
            plan: None,
            state: None,
            selected_worker_logs: vec![
                "Initializing worker environment".to_string(),
                "Running cargo check".to_string(),
            ],
            status_message: None,
        }
    }

    #[tokio::test]
    async fn test_swarm_deck_data_load_empty() {
        let temp = tempdir().unwrap();
        let data = SwarmDeckData::load(temp.path(), 0, SwarmDashboardStyle::Stylish).await;
        assert_eq!(data.style, SwarmDashboardStyle::Stylish);
        assert_eq!(data.selected_index, 0);
    }

    #[test]
    fn test_render_all_five_styles() {
        let styles = [
            SwarmDashboardStyle::Stylish,
            SwarmDashboardStyle::GitGraph,
            SwarmDashboardStyle::Modern,
            SwarmDashboardStyle::Minimal,
            SwarmDashboardStyle::Cockpit,
        ];

        let theme = Theme::default();
        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).unwrap();

        for s in styles {
            let data = make_test_data(s);
            terminal
                .draw(|f| {
                    render_swarm_flight_deck(f, f.area(), &theme, &data);
                })
                .unwrap();
        }
    }

    #[test]
    fn test_swarm_deck_navigation_and_cycle_style() {
        let mut data = make_test_data(SwarmDashboardStyle::Stylish);
        assert_eq!(data.selected_index, 0);
        assert_eq!(
            data.selected_worker().map(|w| w.id.as_str()),
            Some("swarm-worker-t1")
        );

        data.select_next();
        assert_eq!(data.selected_index, 1);
        assert_eq!(
            data.selected_worker().map(|w| w.id.as_str()),
            Some("swarm-worker-t2")
        );

        data.select_next(); // Wrap around
        assert_eq!(data.selected_index, 0);

        data.select_prev(); // Wrap backwards
        assert_eq!(data.selected_index, 1);

        // Cycle style
        assert_eq!(data.cycle_style(), SwarmDashboardStyle::GitGraph);
        assert_eq!(data.cycle_style(), SwarmDashboardStyle::Modern);
        assert_eq!(data.cycle_style(), SwarmDashboardStyle::Minimal);
        assert_eq!(data.cycle_style(), SwarmDashboardStyle::Cockpit);
        assert_eq!(data.cycle_style(), SwarmDashboardStyle::Stylish);
    }
}

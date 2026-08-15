use crate::agent::models::ModelFetcher;
use crate::agent::types::AgentEvent;
use crate::agent::AgentLoop;
use crate::config::Config;
use crate::error::Result;
use crate::session::undo::rollback_turn;
use crate::ui::{InputDock, ModalState, StatusWidgets, Theme, TimelineContext, TimelineView};
use crossterm::event::{Event, EventStream, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use futures::StreamExt;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::Style;
use ratatui::widgets::{Block, Borders};
use ratatui::Terminal;
use std::io::stdout;
use std::path::Path;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

pub struct App<'a> {
    workspace_root: std::path::PathBuf,
    config: Config,
    theme: Theme,
    timeline: TimelineView,
    input_dock: InputDock<'a>,
    modal: ModalState,
    model_fetcher: ModelFetcher,
    is_working: bool,
    work_start: Option<Instant>,
}

impl<'a> App<'a> {
    pub fn new(workspace_root: &Path, config: Config) -> Self {
        let theme = Theme::detect(&config.ui.theme);
        Self {
            workspace_root: workspace_root.to_path_buf(),
            config,
            theme,
            timeline: TimelineView::new(),
            input_dock: InputDock::new(),
            modal: ModalState::None,
            model_fetcher: ModelFetcher::new(),
            is_working: false,
            work_start: None,
        }
    }

    /// Hydrates past session events into the timeline view
    pub fn hydrate_session(&mut self, events: &[AgentEvent]) {
        for event in events {
            match event {
                AgentEvent::TurnStart { .. } => {}
                AgentEvent::StreamDelta { delta, .. } => {
                    self.timeline.append_assistant_delta(delta);
                }
                AgentEvent::ToolCall { tool, args, .. } => {
                    self.timeline.add_tool_call(tool.clone(), args.to_string());
                }
                AgentEvent::ToolResult {
                    tool,
                    success,
                    output,
                    duration_ms,
                    ..
                } => {
                    self.timeline
                        .finish_tool_call(tool, *success, output.clone(), *duration_ms);
                }
                AgentEvent::TurnEnd { .. } => {}
                AgentEvent::Error { message, .. } => {
                    self.timeline.add_status(format!("Error: {}", message));
                }
                _ => {}
            }
        }
    }

    /// Runs the full-screen interactive Ratatui TUI session in Aura Theme styling.
    pub async fn run(&mut self, mut agent: AgentLoop) -> Result<()> {
        enable_raw_mode()?;
        let mut stdout = stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        let mut event_stream = EventStream::new();
        let (prompt_tx, mut prompt_rx) = mpsc::unbounded_channel::<String>();
        let (event_tx, mut event_rx) = mpsc::unbounded_channel::<AgentEvent>();

        // Spawn background non-blocking Agent actor
        let agent_task = tokio::spawn(async move {
            while let Some(prompt) = prompt_rx.recv().await {
                if let Err(e) = agent.execute_turn(&prompt, event_tx.clone()).await {
                    let err_event = AgentEvent::Error {
                        turn_id: None,
                        code: "execution_error".to_string(),
                        message: e.to_string(),
                        retrying: false,
                        retry_after_ms: None,
                    };
                    event_tx.send(err_event).ok();
                }
            }
        });

        let mut ticker =
            tokio::time::interval(Duration::from_millis(crate::constants::TICK_RATE_MS));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            let working_secs = self.work_start.map(|s| s.elapsed().as_secs()).unwrap_or(0);

            // Render UI with Aura Theme aesthetic
            terminal.draw(|frame| {
                let background_block = Block::default()
                    .borders(Borders::NONE)
                    .style(Style::default().bg(self.theme.bg_primary));
                frame.render_widget(background_block, frame.area());

                // Reserve dynamic height for autocomplete suggestions if user is typing a slash command
                let matching_cmds = self.input_dock.matching_slash_commands();
                let has_slash_hint = !matching_cmds.is_empty();
                let hint_height = if has_slash_hint {
                    matching_cmds
                        .len()
                        .min(crate::constants::MAX_AUTOCOMPLETE_ROWS) as u16
                } else {
                    0
                };

                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Min(4),              // Streaming Timeline
                        Constraint::Length(hint_height), // Autocomplete hint rows
                        Constraint::Length(3),           // Input Dock
                        Constraint::Length(1),           // Minimal Bottom Status Line
                    ])
                    .split(frame.area());

                let timeline_ctx = TimelineContext {
                    theme: &self.theme,
                    is_working: self.is_working,
                    working_secs,
                    workspace: &self.workspace_root,
                    provider: &self.config.provider.default,
                    model: &self.config.provider.model,
                };
                self.timeline.render(frame, chunks[0], &timeline_ctx);

                if has_slash_hint {
                    self.input_dock
                        .render_autocomplete_hint(frame, chunks[1], &self.theme);
                }

                self.input_dock.render(frame, chunks[2], &self.theme);

                let active_mcp_count = self
                    .config
                    .mcp
                    .servers
                    .values()
                    .filter(|s| s.enabled)
                    .count();

                StatusWidgets::render_bottom_bar(
                    frame,
                    chunks[3],
                    &self.theme,
                    &self.workspace_root,
                    &self.config.provider.model,
                    active_mcp_count,
                );

                // Render Modal Overlay if active
                if self.modal.is_active() {
                    self.modal.render(frame, frame.area(), &self.theme);
                }
            })?;

            tokio::select! {
                // UI frame tick (smooth timer animation during execution)
                _ = ticker.tick() => {
                    // Triggers loop draw iteration
                }

                // Handle Agent streaming events from background actor
                Some(agent_event) = event_rx.recv() => {
                    match agent_event {
                        AgentEvent::StreamDelta { delta, .. } => {
                            self.timeline.append_assistant_delta(&delta);
                        }
                        AgentEvent::ToolCall { tool, args, .. } => {
                            self.timeline.add_tool_call(tool, args.to_string());
                        }
                        AgentEvent::ToolResult { tool, success, output, duration_ms, .. } => {
                            self.timeline.finish_tool_call(&tool, success, output, duration_ms);
                        }
                        AgentEvent::TurnEnd { .. } => {
                            self.is_working = false;
                            self.work_start = None;
                        }
                        AgentEvent::Error { message, retrying, .. } => {
                            if retrying {
                                self.timeline.add_status(message);
                            } else {
                                self.timeline.append_assistant_delta(&format!("\n✗ Error: {}\n", message));
                                self.is_working = false;
                                self.work_start = None;
                            }
                        }
                        _ => {}
                    }
                }

                // Handle user keyboard events from terminal
                Some(Ok(event)) = event_stream.next() => {
                    if let Event::Key(key_event) = event {
                        if key_event.kind == KeyEventKind::Release {
                            continue;
                        }

                        // Modal is active — intercept keyboard navigation
                        if self.modal.is_active() {
                            self.handle_modal_key(key_event).await;
                            continue;
                        }

                        // Check for Ctrl+C or Esc to interrupt or exit
                        if key_event.code == KeyCode::Esc || (key_event.code == KeyCode::Char('c') && key_event.modifiers.contains(KeyModifiers::CONTROL)) {
                            if self.is_working {
                                self.is_working = false;
                                self.work_start = None;
                                self.timeline.add_status("Interrupted".to_string());
                            } else {
                                break;
                            }
                        }

                        // Check PageUp / PageDown for timeline scrolling
                        if key_event.code == KeyCode::PageUp {
                            self.timeline.auto_scroll = false;
                            self.timeline.scroll_offset = self.timeline.scroll_offset.saturating_sub(crate::constants::PAGE_SCROLL_LINES);
                            continue;
                        }
                        if key_event.code == KeyCode::PageDown {
                            self.timeline.scroll_offset = self.timeline.scroll_offset.saturating_add(crate::constants::PAGE_SCROLL_LINES);
                            continue;
                        }

                        // Send input to input dock
                        if let Some(prompt) = self.input_dock.handle_key(key_event) {
                            if prompt == "/exit" || prompt == "/quit" {
                                break;
                            }

                            if prompt == "/clear" {
                                self.timeline.entries.clear();
                                continue;
                            }

                            if prompt == "/help" {
                                self.modal = ModalState::Help;
                                continue;
                            }

                            if prompt == "/model" || prompt == "/models" || prompt == "/provider" {
                                self.modal = ModalState::new_provider_select();
                                continue;
                            }

                            if prompt == "/undo" {
                                match rollback_turn(&self.workspace_root) {
                                    Ok(res) if res.restored_count > 0 || res.deleted_count > 0 => {
                                        self.timeline.add_status(format!(
                                            "✔ Reverted turn #{}: restored {} file(s), deleted {} file(s)",
                                            res.turn_id, res.restored_count, res.deleted_count
                                        ));
                                    }
                                    Ok(_) => {
                                        self.timeline.add_status("ℹ No changes found to undo in previous turn".to_string());
                                    }
                                    Err(e) => {
                                        self.timeline.add_status(format!("✗ Undo failed: {}", e));
                                    }
                                }
                                continue;
                            }

                            self.timeline.add_user_message(prompt.clone());
                            self.is_working = true;
                            self.work_start = Some(Instant::now());

                            // Dispatch asynchronously to agent background actor
                            prompt_tx.send(prompt).ok();
                        }
                    }
                }
            }
        }

        // Abort background task and cleanup terminal state cleanly
        agent_task.abort();
        disable_raw_mode()?;
        execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
        terminal.show_cursor()?;

        Ok(())
    }

    /// Handles keyboard interaction within in-TUI modal dialogs
    async fn handle_modal_key(&mut self, key: crossterm::event::KeyEvent) {
        match &mut self.modal {
            ModalState::None => {}
            ModalState::ProviderSelect {
                providers,
                selected_index,
            } => match key.code {
                KeyCode::Esc | KeyCode::Char('q') => {
                    self.modal = ModalState::None;
                }
                KeyCode::Up => {
                    *selected_index = selected_index.saturating_sub(1);
                }
                KeyCode::Down => {
                    if *selected_index + 1 < providers.len() {
                        *selected_index += 1;
                    }
                }
                KeyCode::Enter => {
                    let provider = providers[*selected_index].clone();
                    let api_key = self.config.get_api_key(&provider).unwrap_or_else(|e| {
                        tracing::warn!(error = %e, provider = %provider, "Failed to resolve API key for provider switch");
                        String::new()
                    });

                    // Switch modal to loading models state
                    let models_res = self
                        .model_fetcher
                        .fetch_models(&provider, &api_key, None)
                        .await;
                    let models = match models_res {
                        Ok(m) => m,
                        Err(e) => {
                            tracing::warn!(error = %e, provider = %provider, "Failed to fetch live model list");
                            Vec::new()
                        }
                    };

                    if models.is_empty() {
                        self.timeline.add_status(format!(
                            "ℹ No live models list found for {}. Keeping current model: {}",
                            provider, self.config.provider.model
                        ));
                        self.config.provider.default = provider;
                        self.modal = ModalState::None;
                    } else {
                        self.modal = ModalState::new_model_select(provider, models);
                    }
                }
                _ => {}
            },
            ModalState::ModelSelect {
                provider,
                models,
                filtered_indices,
                selected_index,
                filter,
                ..
            } => match key.code {
                KeyCode::Esc => {
                    self.modal = ModalState::new_provider_select();
                }
                KeyCode::Up => {
                    *selected_index = selected_index.saturating_sub(1);
                }
                KeyCode::Down => {
                    if *selected_index + 1 < filtered_indices.len() {
                        *selected_index += 1;
                    }
                }
                KeyCode::Backspace => {
                    filter.pop();
                    self.modal.update_filter();
                }
                KeyCode::Char(c) => {
                    filter.push(c);
                    self.modal.update_filter();
                }
                KeyCode::Enter => {
                    if !filtered_indices.is_empty() && *selected_index < filtered_indices.len() {
                        let real_idx = filtered_indices[*selected_index];
                        let selected_model = models[real_idx].id.clone();
                        self.config.provider.default = provider.clone();
                        self.config.provider.model = selected_model.clone();

                        self.timeline.add_status(format!(
                            "✔ Switched active provider to '{}' and model to '{}'",
                            provider, selected_model
                        ));
                    }
                    self.modal = ModalState::None;
                }
                _ => {}
            },
            ModalState::Help => {
                if key.code == KeyCode::Esc
                    || key.code == KeyCode::Enter
                    || key.code == KeyCode::Char('q')
                {
                    self.modal = ModalState::None;
                }
            }
        }
    }
}

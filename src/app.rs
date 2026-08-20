use crate::agent::models::ModelFetcher;
use crate::agent::types::AgentEvent;
use crate::agent::AgentLoop;
use crate::config::Config;
use crate::error::Result;
use crate::ui::{InputDock, ModalState, StatusWidgets, Theme, TimelineContext, TimelineView};
use crossterm::event::{
    DisableMouseCapture, EnableMouseCapture, Event, EventStream, KeyCode, KeyEventKind,
    KeyModifiers, MouseButton, MouseEventKind,
};
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

pub enum AgentCommand {
    Prompt(String, Option<tokio_util::sync::CancellationToken>),
    UpdateConfig {
        config: Box<Config>,
        provider: Box<dyn crate::agent::provider::Provider>,
    },
    Rollback {
        target_turn_id: usize,
        message_index: usize,
    },
}

pub struct App<'a> {
    workspace_root: std::path::PathBuf,
    config: Config,
    theme: Theme,
    timeline: TimelineView,
    input_dock: InputDock<'a>,
    pty_drawer: crate::ui::PtyDrawer,
    modal: ModalState,
    model_fetcher: ModelFetcher,
    is_working: bool,
    work_start: Option<Instant>,
    cancel_token: Option<tokio_util::sync::CancellationToken>,
    last_user_prompt: Option<String>,
    last_turn_tokens: usize,
    total_cost_usd: f64,
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
            pty_drawer: crate::ui::PtyDrawer::new(),
            modal: ModalState::None,
            model_fetcher: ModelFetcher::new(),
            is_working: false,
            work_start: None,
            cancel_token: None,
            last_user_prompt: None,
            last_turn_tokens: 0,
            total_cost_usd: 0.0,
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
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        let mut event_stream = EventStream::new();
        let (control_tx, mut control_rx) = mpsc::unbounded_channel::<AgentCommand>();
        let (event_tx, mut event_rx) = mpsc::unbounded_channel::<AgentEvent>();

        // Spawn background non-blocking Agent actor
        let agent_task = tokio::spawn(async move {
            while let Some(cmd) = control_rx.recv().await {
                match cmd {
                    AgentCommand::Prompt(prompt, cancel_token) => {
                        if let Err(e) = agent
                            .execute_turn(&prompt, event_tx.clone(), cancel_token)
                            .await
                        {
                            let err_event = AgentEvent::Error {
                                turn_id: None,
                                code: "execution_error".to_string(),
                                message: e.to_string(),
                                retrying: false,
                                retry_after_ms: None,
                            };
                            if let Err(send_err) = event_tx.send(err_event) {
                                tracing::error!(
                                    error = %send_err,
                                    "Failed to send agent error event to UI channel"
                                );
                            }
                        }
                    }
                    AgentCommand::UpdateConfig { config, provider } => {
                        agent.update_config(*config, provider);
                    }
                    AgentCommand::Rollback {
                        target_turn_id,
                        message_index,
                    } => {
                        agent.rollback_turn(target_turn_id, message_index);
                    }
                }
            }
        });

        let mut ticker =
            tokio::time::interval(Duration::from_millis(crate::constants::TICK_RATE_MS));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            let working_millis = self
                .work_start
                .map(|s| s.elapsed().as_millis() as u64)
                .unwrap_or(0);

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
                        Constraint::Min(4),              // 0: Streaming Timeline
                        Constraint::Length(1),           // 1: Top Spacer / Margin above input dock
                        Constraint::Length(hint_height), // 2: Autocomplete hint rows
                        Constraint::Length(3),           // 3: Input Dock
                        Constraint::Length(1), // 4: Bottom Spacer / Margin below input dock
                        Constraint::Length(1), // 5: Minimal Bottom Status Line
                    ])
                    .split(frame.area());

                let timeline_ctx = TimelineContext {
                    theme: &self.theme,
                    is_working: self.is_working,
                    working_millis,
                    workspace: &self.workspace_root,
                    provider: &self.config.provider.default,
                    model: &self.config.provider.model,
                };
                self.timeline.render(frame, chunks[0], &timeline_ctx);

                if has_slash_hint {
                    self.input_dock
                        .render_autocomplete_hint(frame, chunks[2], &self.theme);
                }

                self.input_dock.render(frame, chunks[3], &self.theme);

                let active_mcp_count = self
                    .config
                    .mcp
                    .servers
                    .values()
                    .filter(|s| s.enabled)
                    .count();

                let max_context =
                    crate::agent::models::get_model_context_limit(&self.config.provider.model);

                let status_ctx = crate::ui::StatusContext {
                    theme: &self.theme,
                    workspace: &self.workspace_root,
                    provider: &self.config.provider.default,
                    model: &self.config.provider.model,
                    mcp_count: active_mcp_count,
                    used_tokens: self.last_turn_tokens,
                    max_context,
                    show_cost: self.config.ui.show_cost,
                    session_cost_usd: self.total_cost_usd,
                };

                StatusWidgets::render_bottom_bar(frame, chunks[5], &status_ctx);

                // Render Modal Overlay if active
                if self.modal.is_active() {
                    self.modal.render(frame, frame.area(), &self.theme);
                }

                // Render Embedded PTY Terminal Drawer if active
                if self.pty_drawer.is_open {
                    self.pty_drawer.render(frame, frame.area());
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
                            AgentEvent::TurnEnd { total_tokens_used, .. } => {
                                self.last_turn_tokens = total_tokens_used;
                                let prompt_toks = (total_tokens_used * 3) / 4;
                                let comp_toks = total_tokens_used / 4;
                                let turn_cost = crate::agent::pricing::ModelPricing::calculate_cost(
                                    &self.config.provider.default,
                                    &self.config.provider.model,
                                    prompt_toks,
                                    comp_toks,
                                );
                                self.total_cost_usd += turn_cost;
                                let elapsed_secs = self.work_start.map(|s| s.elapsed().as_secs_f64());
                                self.timeline.finalize_pending_thoughts(elapsed_secs);
                                self.is_working = false;
                                self.work_start = None;
                                self.cancel_token = None;
                            }
                            AgentEvent::GitCommit { hash, message, .. } => {
                                self.timeline.add_status(format!("✔ Auto-committed {}: \"{}\"", hash, message));
                            }
                            AgentEvent::ApprovalRequest { turn_id, tool_id, tool, args, .. } => {
                                let approval_state = crate::ui::approval::ApprovalModalState::from_tool_call(
                                    turn_id,
                                    &tool_id,
                                    &tool,
                                    &args,
                                    &self.theme,
                                );
                                self.modal = crate::ui::modal::ModalState::Approval(approval_state);
                            }
                            AgentEvent::Error { message, retrying, .. } => {
                                if retrying {
                                    self.timeline.add_status(message);
                                } else {
                                    self.timeline.append_assistant_delta(&format!("\n✗ Error: {}\n", message));
                                    self.is_working = false;
                                    self.work_start = None;
                                    self.cancel_token = None;
                                }
                            }
                            _ => {}
                        }
                    }

                    // Handle user keyboard and mouse events from terminal
                    Some(Ok(event)) = event_stream.next() => {
                        match event {
                            Event::Mouse(mouse_event) => {
                                match mouse_event.kind {
                                    MouseEventKind::ScrollUp => {
                                        if self.pty_drawer.is_open {
                                            self.pty_drawer.scroll_offset = self.pty_drawer.scroll_offset.saturating_add(3);
                                        } else {
                                            self.timeline.scroll_up(3);
                                        }
                                    }
                                    MouseEventKind::ScrollDown => {
                                        if self.pty_drawer.is_open {
                                            self.pty_drawer.scroll_offset = self.pty_drawer.scroll_offset.saturating_sub(3);
                                        } else {
                                            self.timeline.scroll_down(3);
                                        }
                                    }
                                    MouseEventKind::Down(MouseButton::Left) => {
                                        if !self.pty_drawer.is_open && !self.modal.is_active() {
                                            self.timeline.handle_mouse_down(mouse_event.column, mouse_event.row);
                                        }
                                    }
                                    MouseEventKind::Drag(MouseButton::Left) => {
                                        if !self.pty_drawer.is_open && !self.modal.is_active() {
                                            self.timeline.handle_mouse_drag(mouse_event.column, mouse_event.row);
                                        }
                                    }
                                    MouseEventKind::Up(MouseButton::Left) => {
                                        if !self.pty_drawer.is_open && !self.modal.is_active() {
                                            if let Some(selected_text) = self.timeline.handle_mouse_up(mouse_event.column, mouse_event.row) {
                                                let trimmed = selected_text.trim();
                                                if !trimmed.is_empty() {
                                                    let preview = if trimmed.len() > 25 {
                                                        format!("{}...", &trimmed[..trimmed.char_indices().map(|(i, _)| i).take(25).last().unwrap_or(0)])
                                                    } else {
                                                        trimmed.to_string()
                                                    };
                                                    self.timeline.add_status(format!("✔ Copied to clipboard: \"{}\"", preview));
                                                }
                                            }
                                        }
                                    }
                                    _ => {}
                                }
                            }
                            Event::Key(key_event) => {
                                if key_event.kind == KeyEventKind::Release {
                                    continue;
                                }

                                // Modal is active — intercept keyboard navigation
                                if self.modal.is_active() {
                                    self.handle_modal_key(key_event, &control_tx).await;
                                    continue;
                                }

                                // Ctrl+T toggles embedded PTY terminal drawer
                                if key_event.code == KeyCode::Char('t') && key_event.modifiers.contains(KeyModifiers::CONTROL) {
                                    self.pty_drawer.toggle();
                                    continue;
                                }

                                // When PTY drawer is open, route keystrokes into drawer
                                if self.pty_drawer.is_open {
                                    match key_event.code {
                                        KeyCode::Esc => {
                                            self.pty_drawer.is_open = false;
                                        }
                                        KeyCode::PageUp => {
                                            self.pty_drawer.scroll_offset = self.pty_drawer.scroll_offset.saturating_add(5);
                                        }
                                        KeyCode::PageDown => {
                                            self.pty_drawer.scroll_offset = self.pty_drawer.scroll_offset.saturating_sub(5);
                                        }
                                        KeyCode::Up if key_event.modifiers.contains(KeyModifiers::SHIFT) || key_event.modifiers.contains(KeyModifiers::CONTROL) || key_event.modifiers.contains(KeyModifiers::ALT) => {
                                            self.pty_drawer.scroll_offset = self.pty_drawer.scroll_offset.saturating_add(2);
                                        }
                                        KeyCode::Down if key_event.modifiers.contains(KeyModifiers::SHIFT) || key_event.modifiers.contains(KeyModifiers::CONTROL) || key_event.modifiers.contains(KeyModifiers::ALT) => {
                                            self.pty_drawer.scroll_offset = self.pty_drawer.scroll_offset.saturating_sub(2);
                                        }
                                        KeyCode::Enter => {
                                            if let Some(cmd) = self.pty_drawer.submit_command() {
                                                let ws = self.workspace_root.clone();
                                                match crate::tools::exec::exec_cmd(&ws, &cmd, Some(60)).await {
                                                    Ok(out) => {
                                                        for line in out.lines() {
                                                            self.pty_drawer.append_output(line);
                                                        }
                                                    }
                                                    Err(e) => {
                                                        self.pty_drawer.append_output(format!("Error: {}", e));
                                                    }
                                                }
                                            }
                                        }
                                        KeyCode::Backspace => {
                                            self.pty_drawer.handle_backspace();
                                        }
                                        KeyCode::Char(c) => {
                                            self.pty_drawer.handle_char(c);
                                        }
                                        _ => {}
                                    }
                                    continue;
                                }

                                // Check for Ctrl+C or Esc to interrupt or exit
                                if key_event.code == KeyCode::Esc || (key_event.code == KeyCode::Char('c') && key_event.modifiers.contains(KeyModifiers::CONTROL)) {
                                    if self.is_working {
                                        if let Some(token) = self.cancel_token.take() {
                                            token.cancel();
                                        }
                                        self.is_working = false;
                                        self.work_start = None;
                                        self.timeline.add_status("⏹ Turn interrupted by user".to_string());
                                        continue;
                                    } else if key_event.code == KeyCode::Esc && self.timeline.has_selection() {
                                        self.timeline.clear_selection();
                                        continue;
                                    } else {
                                        break;
                                    }
                                }

                                // Dedicated timeline scroll keys: PageUp / PageDown / Home / End / Shift+Up / Shift+Down / Ctrl+Up / Ctrl+Down / Alt+Up / Alt+Down
                                if key_event.code == KeyCode::PageUp {
                                    self.timeline.scroll_page_up(crate::constants::PAGE_SCROLL_LINES * 5);
                                    continue;
                                }
                                if key_event.code == KeyCode::PageDown {
                                    self.timeline.scroll_page_down(crate::constants::PAGE_SCROLL_LINES * 5);
                                    continue;
                                }
                                if key_event.code == KeyCode::Home && (key_event.modifiers.contains(KeyModifiers::CONTROL) || key_event.modifiers.contains(KeyModifiers::SHIFT)) {
                                    self.timeline.scroll_to_top();
                                    continue;
                                }
                                if key_event.code == KeyCode::End && (key_event.modifiers.contains(KeyModifiers::CONTROL) || key_event.modifiers.contains(KeyModifiers::SHIFT)) {
                                    self.timeline.scroll_to_bottom();
                                    continue;
                                }
                                if (key_event.code == KeyCode::Up) && (key_event.modifiers.contains(KeyModifiers::SHIFT) || key_event.modifiers.contains(KeyModifiers::CONTROL) || key_event.modifiers.contains(KeyModifiers::ALT)) {
                                    self.timeline.scroll_up(3);
                                    continue;
                                }
                                if (key_event.code == KeyCode::Down) && (key_event.modifiers.contains(KeyModifiers::SHIFT) || key_event.modifiers.contains(KeyModifiers::CONTROL) || key_event.modifiers.contains(KeyModifiers::ALT)) {
                                    self.timeline.scroll_down(3);
                                    continue;
                                }

                                // If input dock has no matching slash suggestions and textarea is single-line empty, Up/Down scroll timeline
                                let is_input_empty = self.input_dock.textarea.lines().len() <= 1 && self.input_dock.textarea.lines().first().map(|l| l.is_empty()).unwrap_or(true);
                                let has_slash_matching = !self.input_dock.matching_slash_commands().is_empty();
                                if is_input_empty && !has_slash_matching {
                                    if key_event.code == KeyCode::Up {
                                        self.timeline.scroll_up(3);
                                        continue;
                                    }
                                    if key_event.code == KeyCode::Down {
                                        self.timeline.scroll_down(3);
                                        continue;
                                    }
                                }

                            // Send input to input dock
                            if let Some(raw_prompt) = self.input_dock.handle_key(key_event) {
                                let prompt = raw_prompt.trim().to_string();
                                if prompt.is_empty() {
                                    continue;
                                }

                                if prompt == "/exit" || prompt == "/quit" {
                                    break;
                                }

                                if prompt == "/terminal" {
                                    self.pty_drawer.toggle();
                                    continue;
                                }

                                if prompt == "/copy" || prompt.starts_with("/copy ") {
                                    let copy_all = prompt.contains("all");
                                    let text_to_copy = if copy_all {
                                        self.timeline.get_all_transcript_text()
                                    } else {
                                        self.timeline
                                            .get_last_assistant_response()
                                            .unwrap_or_default()
                                    };

                                    if text_to_copy.trim().is_empty() {
                                        self.timeline
                                            .add_status("ℹ Nothing to copy yet".to_string());
                                    } else {
                                        let ok =
                                            crate::ui::clipboard::copy_to_clipboard(&text_to_copy);
                                        if ok {
                                            let label = if copy_all {
                                                "entire conversation"
                                            } else {
                                                "latest assistant response"
                                            };
                                            self.timeline.add_status(format!(
                                                "✔ Copied {} to clipboard",
                                                label
                                            ));
                                        } else {
                                            self.timeline.add_status(
                                                "✗ Failed to copy to clipboard".to_string(),
                                            );
                                        }
                                    }
                                    continue;
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
                                    let backup_mgr = crate::session::backup::BackupManager::new(&self.workspace_root);
                                    let checkpoints = backup_mgr.list_checkpoints();
                                    if checkpoints.is_empty() {
                                        self.timeline.add_status("ℹ No recorded checkpoints available to undo".to_string());
                                    } else {
                                        self.modal = ModalState::new_undo_checkpoint(checkpoints);
                                    }
                                    continue;
                                }

                                if prompt == "/tokens" {
                                    let model_limit = crate::agent::models::get_model_context_limit(&self.config.provider.model);
                                    let card = format!(
                                        "📊 Token & Context Metrics:\n  • Provider: {}\n  • Active Model: {}\n  • Context Limit: {} tokens\n  • Last Turn Usage: {} tokens\n  • Compaction Threshold: {:.0}%",
                                        self.config.provider.default,
                                        self.config.provider.model,
                                        model_limit,
                                        self.last_turn_tokens,
                                        self.config.agent.warning_threshold * 100.0
                                    );
                                    self.timeline.add_status(card);
                                    continue;
                                }

                                if prompt == "/map" {
                                    let mut graph = crate::context::graph::CodeGraph::new();
                                    if let Err(e) = graph.build_graph(&self.workspace_root) {
                                        self.timeline.add_status(format!("✗ Failed to build repo map: {}", e));
                                    } else {
                                        let repomap = graph.format_repomap(&self.workspace_root, &[], self.config.agent.map_tokens);
                                        self.timeline.add_status(format!("🗺️ AST PageRank Repository Map:\n\n{}", repomap));
                                    }
                                    continue;
                                }

                                if prompt == "/compact" {
                                    self.timeline.add_status("✔ Context compaction requested".to_string());
                                    continue;
                                }

                                if prompt.starts_with("/save") {
                                    let target_path = prompt.trim_start_matches("/save").trim();
                                    let export_file = if target_path.is_empty() {
                                        let export_dir = self.workspace_root.join(".minicode").join("exports");
                                        let _ = std::fs::create_dir_all(&export_dir);
                                        export_dir.join(format!("session_{}.md", chrono::Utc::now().format("%Y%m%d_%H%M%S")))
                                    } else {
                                        self.workspace_root.join(target_path)
                                    };
                                    let mut md = format!("# minicode Session Export — {}\n\n", chrono::Utc::now().to_rfc3339());
                                    for entry in &self.timeline.entries {
                                        match entry {
                                            crate::ui::view::TimelineEntry::UserPrompt(text) => {
                                                md.push_str(&format!("### 👤 User\n{}\n\n", text));
                                            }
                                            crate::ui::view::TimelineEntry::AssistantMarkdown(text) => {
                                                md.push_str(&format!("### 🤖 Assistant\n{}\n\n", text));
                                            }
                                            crate::ui::view::TimelineEntry::ToolStart { name, command_or_path } => {
                                                md.push_str(&format!("*🔧 Tool Started:* `{}` (`{}`)\n\n", name, command_or_path));
                                            }
                                            crate::ui::view::TimelineEntry::ToolFinished { name, command_or_path, success, output, .. } => {
                                                md.push_str(&format!("*🔧 Tool Finished:* `{}` (`{}` - {})\n```\n{}\n```\n\n", name, command_or_path, if *success { "success" } else { "failure" }, output));
                                            }
                                            crate::ui::view::TimelineEntry::SystemStatus(text) => {
                                                md.push_str(&format!("*ℹ Status:* {}\n\n", text));
                                            }
                                            _ => {}
                                        }
                                    }
                                    if let Err(e) = std::fs::write(&export_file, md) {
                                        self.timeline.add_status(format!("✗ Failed to save session: {}", e));
                                    } else {
                                        self.timeline.add_status(format!("✔ Saved conversation export to {}", export_file.display()));
                                    }
                                    continue;
                                }

                                if prompt.starts_with("/load") {
                                    let target_id = prompt.trim_start_matches("/load").trim();
                                    let store = crate::session::store::SessionStore::new();
                                    if target_id.is_empty() {
                                        match store.list_sessions() {
                                            Ok(sessions) if !sessions.is_empty() => {
                                                let mut list_msg = format!("📂 Available Sessions ({}):\n", sessions.len());
                                                for s in sessions.iter().take(10) {
                                                    list_msg.push_str(&format!("  • {} ({})\n", s.id, s.created_at));
                                                }
                                                list_msg.push_str("\nUse `/load <session_id>` to view a session.");
                                                self.timeline.add_status(list_msg);
                                            }
                                            Ok(_) => {
                                                self.timeline.add_status("ℹ No past sessions found".to_string());
                                            }
                                            Err(e) => {
                                                self.timeline.add_status(format!("✗ Failed to list sessions: {}", e));
                                            }
                                        }
                                    } else {
                                        match store.load_session(target_id) {
                                            Ok(events) => {
                                                self.timeline.entries.clear();
                                                self.hydrate_session(&events);
                                                self.timeline.add_status(format!("✔ Loaded session '{}' with {} events", target_id, events.len()));
                                            }
                                            Err(e) => {
                                                self.timeline.add_status(format!("✗ Failed to load session '{}': {}", target_id, e));
                                            }
                                        }
                                    }
                                    continue;
                                }

                                let prompt_to_run = if prompt == "/retry" {
                                    if let Some(ref last) = self.last_user_prompt {
                                        self.timeline.add_status(format!("🔄 Retrying last prompt: \"{}\"", last));
                                        last.clone()
                                    } else {
                                        self.timeline.add_status("ℹ No previous user prompt to retry".to_string());
                                        continue;
                                    }
                                } else {
                                    self.last_user_prompt = Some(prompt.clone());
                                    prompt
                                };

                                self.timeline.add_user_message(prompt_to_run.clone());
                                self.is_working = true;
                                self.work_start = Some(Instant::now());

                                let cancel = tokio_util::sync::CancellationToken::new();
                                self.cancel_token = Some(cancel.clone());

                                // Dispatch asynchronously to agent background actor
                                if let Err(e) = control_tx.send(AgentCommand::Prompt(prompt_to_run, Some(cancel))) {
                                    tracing::error!(error = %e, "Failed to dispatch prompt to agent actor");
                                    self.is_working = false;
                                    self.work_start = None;
                                    self.cancel_token = None;
                                    self.timeline.add_status(format!("❌ Agent communication failure: {}", e));
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        // Abort background task and cleanup terminal state cleanly
        agent_task.abort();
        disable_raw_mode()?;
        execute!(
            terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        )?;
        terminal.show_cursor()?;

        Ok(())
    }

    /// Handles keyboard interaction within in-TUI modal dialogs
    async fn handle_modal_key(
        &mut self,
        key: crossterm::event::KeyEvent,
        control_tx: &mpsc::UnboundedSender<AgentCommand>,
    ) {
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

                        let custom_url = if self.config.provider.default == "ollama" {
                            Some(self.config.provider.ollama.host.as_str())
                        } else {
                            None
                        };

                        match self.config.get_api_key(&self.config.provider.default) {
                            Ok(key) => {
                                match crate::agent::provider::create_provider_with_base_url(
                                    &self.config.provider.default,
                                    &key,
                                    custom_url,
                                ) {
                                    Ok(new_prov) => {
                                        let _ = control_tx.send(AgentCommand::UpdateConfig {
                                            config: Box::new(self.config.clone()),
                                            provider: new_prov,
                                        });
                                        self.timeline.add_status(format!(
                                            "✔ Switched active provider to '{}' and model to '{}'",
                                            provider, selected_model
                                        ));
                                    }
                                    Err(e) => {
                                        self.timeline.add_status(format!(
                                            "✗ Failed to create provider '{}': {}",
                                            provider, e
                                        ));
                                    }
                                }
                            }
                            Err(e) => {
                                self.timeline.add_status(format!(
                                    "✗ Missing API key for provider '{}': {}",
                                    provider, e
                                ));
                            }
                        }
                    }
                    self.modal = ModalState::None;
                }
                _ => {}
            },
            ModalState::UndoCheckpoint {
                ref checkpoints,
                ref mut selected_index,
            } => match key.code {
                KeyCode::Esc | KeyCode::Char('q') => {
                    self.modal = ModalState::None;
                }
                KeyCode::Up => {
                    *selected_index = selected_index.saturating_sub(1);
                }
                KeyCode::Down => {
                    if *selected_index + 1 < checkpoints.len() {
                        *selected_index += 1;
                    }
                }
                KeyCode::Enter => {
                    if !checkpoints.is_empty() && *selected_index < checkpoints.len() {
                        let target_checkpoint = checkpoints[*selected_index].clone();
                        let target_turn_id = target_checkpoint.turn_id;
                        let target_prompt = target_checkpoint.prompt.clone();

                        match crate::session::undo::rollback_to_checkpoint(
                            &self.workspace_root,
                            target_turn_id,
                        ) {
                            Ok(res) => {
                                let backup_mgr = crate::session::backup::BackupManager::new(
                                    &self.workspace_root,
                                );
                                let message_index = backup_mgr
                                    .load_turn_manifest(target_turn_id)
                                    .map(|m| m.message_index)
                                    .unwrap_or(0);

                                let _ = control_tx.send(AgentCommand::Rollback {
                                    target_turn_id,
                                    message_index,
                                });

                                self.timeline.add_status(format!(
                                    "✔ Reverted workspace & conversation to checkpoint: \"{}\" (Turn #{}) [{} file(s) restored, {} deleted]",
                                    target_prompt, target_turn_id, res.restored_count, res.deleted_count
                                ));
                            }
                            Err(e) => {
                                self.timeline.add_status(format!("✗ Undo failed: {}", e));
                            }
                        }
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
            ModalState::Approval(ref mut approval_state) => match key.code {
                KeyCode::Esc => {
                    self.modal = ModalState::None;
                    self.timeline
                        .add_status("ℹ Action cancelled by user".to_string());
                }
                KeyCode::Up => {
                    approval_state.prev_option();
                }
                KeyCode::Down => {
                    approval_state.next_option();
                }
                KeyCode::Backspace => {
                    approval_state.handle_backspace();
                }
                KeyCode::Char(c) => {
                    approval_state.handle_char(c);
                }
                KeyCode::Enter => {
                    if let Some(resp) = approval_state.confirm_selection() {
                        match resp {
                            crate::ui::approval::ApprovalResponse::Accept => {
                                self.timeline
                                    .add_status("✔ Action approved & applied".to_string());
                                self.modal = ModalState::None;
                            }
                            crate::ui::approval::ApprovalResponse::Reject => {
                                self.timeline
                                    .add_status("✗ Action rejected by user".to_string());
                                self.modal = ModalState::None;
                            }
                            crate::ui::approval::ApprovalResponse::AllowSession => {
                                self.config.agent.auto_approve = true;
                                self.timeline.add_status(
                                    "✔ Auto-approve enabled for this session".to_string(),
                                );
                                self.modal = ModalState::None;
                            }
                            crate::ui::approval::ApprovalResponse::CustomFeedback(feedback) => {
                                self.timeline
                                    .add_status(format!("💬 User feedback sent: \"{}\"", feedback));
                                self.modal = ModalState::None;

                                let token = tokio_util::sync::CancellationToken::new();
                                self.cancel_token = Some(token.clone());
                                self.is_working = true;
                                self.work_start = Some(Instant::now());
                                let _ =
                                    control_tx.send(AgentCommand::Prompt(feedback, Some(token)));
                            }
                        }
                    }
                }
                _ => {}
            },
        }
    }
}

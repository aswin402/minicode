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

mod commands;
mod modals;

pub use commands::CommandAction;
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
    current_activity: Option<crate::ui::AgentActivity>,
    work_start: Option<Instant>,
    cancel_token: Option<tokio_util::sync::CancellationToken>,
    last_user_prompt: Option<String>,
    last_turn_tokens: usize,
    total_cost_usd: f64,
    /// Handle to the agent's in-flight approval requests.
    approvals: crate::agent::types::ApprovalRegistry,
    pub should_exit: bool,
    pub last_ctrl_c: Option<Instant>,
}

impl<'a> App<'a> {
    pub fn new(workspace_root: &Path, config: Config) -> Self {
        let theme = Theme::detect(&config.ui.theme);
        let graph_file = crate::context::graph_store::GraphStore::graph_file_path(workspace_root);
        let initial_modal = if !graph_file.exists() && !config.ui.plain {
            ModalState::new_workspace_analysis(workspace_root)
        } else {
            ModalState::None
        };

        Self {
            workspace_root: workspace_root.to_path_buf(),
            config,
            theme,
            timeline: TimelineView::new(),
            input_dock: InputDock::new(),
            pty_drawer: crate::ui::PtyDrawer::new(),
            modal: initial_modal,
            model_fetcher: ModelFetcher::new(),
            is_working: false,
            current_activity: None,
            work_start: None,
            cancel_token: None,
            last_user_prompt: None,
            last_turn_tokens: 0,
            total_cost_usd: 0.0,
            approvals: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            should_exit: false,
            last_ctrl_c: None,
        }
    }

    /// Adds a status banner notification directly to the timeline
    pub fn add_timeline_status(&mut self, message: impl Into<String>) {
        self.timeline.add_status(message.into());
    }

    /// Hydrates past session events into the timeline view
    pub fn hydrate_session(&mut self, events: &[AgentEvent]) {
        self.modal = ModalState::None;
        for event in events {
            match event {
                AgentEvent::UserPrompt { prompt, .. } => {
                    self.timeline.add_user_message(prompt.clone());
                }
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
                AgentEvent::ContextCompacted {
                    tier,
                    turns_summarized,
                    tokens_before,
                    tokens_after,
                    savings_percent,
                    ..
                } => {
                    self.timeline.add_context_compaction(
                        *tier,
                        *turns_summarized,
                        *tokens_before,
                        *tokens_after,
                        *savings_percent,
                    );
                }
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

        // Bind to the live agent's approval registry before it moves into the actor.
        self.approvals = agent.approval_registry();
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
            if self.should_exit {
                break;
            }

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

                if self.timeline.is_empty() {
                    let welcome_ctx = crate::ui::WelcomeContext {
                        workspace: &self.workspace_root,
                        provider: &self.config.provider.default,
                        model: &self.config.provider.model,
                    };
                    crate::ui::render_welcome_screen(
                        frame,
                        frame.area(),
                        &self.theme,
                        &welcome_ctx,
                        &self.input_dock,
                    );
                } else {
                    let input_height = self.input_dock.required_height();
                    let chunks = Layout::default()
                        .direction(Direction::Vertical)
                        .constraints([
                            Constraint::Min(4),               // 0: Streaming Timeline
                            Constraint::Length(1), // 1: Top Spacer / Margin above input dock
                            Constraint::Length(input_height), // 2: Dynamic Input Dock
                            Constraint::Length(1), // 3: Bottom Spacer / Margin below input dock
                            Constraint::Length(1), // 4: Minimal Bottom Status Line
                        ])
                        .split(frame.area());

                    let spinner_style =
                        crate::ui::animation::SpinnerStyle::from_id(&self.config.ui.animation);
                    let timeline_ctx = TimelineContext {
                        theme: &self.theme,
                        is_working: self.is_working,
                        working_millis,
                        current_activity: self.current_activity.as_ref(),
                        spinner_style,
                        workspace: &self.workspace_root,
                        provider: &self.config.provider.default,
                        model: &self.config.provider.model,
                    };
                    self.timeline.render(frame, chunks[0], &timeline_ctx);
                    self.input_dock.render(frame, chunks[2], &self.theme);

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

                    StatusWidgets::render_bottom_bar(frame, chunks[4], &status_ctx);
                }

                // Render Floating Spotlight Command Palette Overlay when typing '/'
                if self.input_dock.has_active_slash_query() {
                    self.input_dock
                        .render_slash_palette(frame, frame.area(), &self.theme);
                }

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
                        // If turn was cancelled or finished, ignore trailing stream/tool events
                        if !self.is_working {
                            if let AgentEvent::TurnEnd { total_tokens_used, .. } = agent_event {
                                self.last_turn_tokens = total_tokens_used;
                                self.cancel_token = None;
                            }
                            continue;
                        }

                        match agent_event {
                            AgentEvent::TurnStart { .. } => {
                                self.is_working = true;
                                if self.current_activity.is_none() {
                                    self.current_activity =
                                        Some(crate::ui::AgentActivity::Thinking);
                                }
                            }
                            AgentEvent::StreamDelta { delta, .. } => {
                                self.timeline.append_assistant_delta(&delta);
                                if self.timeline.in_thought_mode {
                                    self.current_activity =
                                        Some(crate::ui::AgentActivity::Thinking);
                                } else {
                                    self.current_activity =
                                        Some(crate::ui::AgentActivity::Responding);
                                }
                            }
                            AgentEvent::ToolCall { tool, args, .. } => {
                                self.current_activity = Some(crate::ui::AgentActivity::from_tool_call(
                                    &tool,
                                    &args.to_string(),
                                ));
                                self.timeline.add_tool_call(tool, args.to_string());
                            }
                            AgentEvent::ToolResult {
                                tool,
                                success,
                                output,
                                duration_ms,
                                ..
                            } => {
                                self.current_activity =
                                    Some(crate::ui::AgentActivity::Working);
                                self.timeline
                                    .finish_tool_call(&tool, success, output, duration_ms);
                            }
                            AgentEvent::TurnEnd {
                                total_tokens_used, ..
                            } => {
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
                                let elapsed_secs =
                                    self.work_start.map(|s| s.elapsed().as_secs_f64());
                                self.timeline.finalize_pending_thoughts(elapsed_secs);
                                self.is_working = false;
                                self.current_activity = None;
                                self.work_start = None;
                                self.cancel_token = None;
                            }
                            AgentEvent::GitCommit { hash, message, .. } => {
                                self.timeline
                                    .add_status(format!("✔ Auto-committed {}: \"{}\"", hash, message));
                            }
                            AgentEvent::ApprovalRequest {
                                turn_id,
                                tool_id,
                                tool,
                                args,
                                ..
                            } => {
                                if self.config.agent.auto_approve {
                                    self.resolve_approval(
                                        &tool_id,
                                        crate::agent::types::ApprovalDecision::Approve,
                                    );
                                    self.timeline.add_status(format!(
                                        "⚙ Auto-approved {} (session policy)",
                                        tool
                                    ));
                                } else {
                                    let approval_state =
                                        crate::ui::approval::ApprovalModalState::from_tool_call(
                                            turn_id,
                                            &tool_id,
                                            &tool,
                                            &args,
                                            &self.theme,
                                        );
                                    self.modal =
                                        crate::ui::modal::ModalState::Approval(approval_state);
                                }
                            }
                            AgentEvent::ContextCompacted {
                                tier,
                                turns_summarized,
                                tokens_before,
                                tokens_after,
                                savings_percent,
                                ..
                            } => {
                                self.current_activity =
                                    Some(crate::ui::AgentActivity::CompactingContext);
                                self.timeline.add_context_compaction(
                                    tier,
                                    turns_summarized,
                                    tokens_before,
                                    tokens_after,
                                    savings_percent,
                                );
                            }
                            AgentEvent::Error {
                                message, retrying, ..
                            } => {
                                if retrying {
                                    self.timeline.add_status(message);
                                } else {
                                    self.timeline
                                        .append_assistant_delta(&format!("\n✗ Error: {}\n", message));
                                    self.is_working = false;
                                    self.current_activity = None;
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
                                            self.pty_drawer.scroll_offset = self.pty_drawer.scroll_offset.saturating_add(crate::constants::SCROLL_LINES_NORMAL as usize);
                                        } else {
                                            self.timeline.scroll_up(crate::constants::SCROLL_LINES_NORMAL);
                                        }
                                    }
                                    MouseEventKind::ScrollDown => {
                                        if self.pty_drawer.is_open {
                                            self.pty_drawer.scroll_offset = self.pty_drawer.scroll_offset.saturating_sub(crate::constants::SCROLL_LINES_NORMAL as usize);
                                        } else {
                                            self.timeline.scroll_down(crate::constants::SCROLL_LINES_NORMAL);
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
                                        let selected_text =
                                            (!self.pty_drawer.is_open && !self.modal.is_active())
                                                .then(|| {
                                                    self.timeline.handle_mouse_up(mouse_event.column, mouse_event.row)
                                                })
                                                .flatten();
                                        if let Some(selected_text) = selected_text {
                                            let trimmed = selected_text.trim();
                                            if !trimmed.is_empty() {
                                                let preview = if trimmed.chars().count() > 25 {
                                                    format!("{}...", trimmed.chars().take(25).collect::<String>())
                                                } else {
                                                    trimmed.to_string()
                                                };
                                                self.timeline.add_status(format!("✔ Copied to clipboard: \"{}\"", preview));
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

                                // Ctrl+D toggles interactive Git Diff modal
                                if key_event.code == KeyCode::Char('d') && key_event.modifiers.contains(KeyModifiers::CONTROL) {
                                    let ws = self.workspace_root.clone();
                                    match crate::git::GitDiffViewer::load_diffs(&ws, false).await {
                                        Ok(diff_files) => {
                                            self.modal = ModalState::new_git_diff(diff_files, false);
                                        }
                                        Err(e) => {
                                            self.timeline.add_status(format!("✗ Failed to load git diff: {}", e));
                                        }
                                    }
                                    continue;
                                }

                                // Ctrl+E toggles interactive CodeGraph Surgical Explorer modal
                                if key_event.code == KeyCode::Char('e') && key_event.modifiers.contains(KeyModifiers::CONTROL) {
                                    self.modal = ModalState::new_code_explorer(&self.workspace_root);
                                    continue;
                                }

                                // Ctrl+H toggles interactive Session History & Time-Travel modal
                                if key_event.code == KeyCode::Char('h') && key_event.modifiers.contains(KeyModifiers::CONTROL) {
                                    let store = crate::session::store::SessionStore::with_workspace(&self.workspace_root);
                                    match store.list_sessions_rich() {
                                        Ok(sessions) => {
                                            let initial_summary = sessions.first().and_then(|s| store.get_session_summary(&s.id).ok());
                                            self.modal = ModalState::new_session_browser(sessions, initial_summary);
                                        }
                                        Err(e) => {
                                            self.timeline.add_status(format!("✗ Failed to load session history: {}", e));
                                        }
                                    }
                                    continue;
                                }

                                // Ctrl+N starts fresh conversation session
                                if key_event.code == KeyCode::Char('n') && key_event.modifiers.contains(KeyModifiers::CONTROL) {
                                    self.timeline = crate::ui::TimelineView::new();
                                    self.timeline.add_status("✨ Started a new session".to_string());
                                    continue;
                                }

                                // Ctrl+L opens Switch Model & Provider modal
                                if key_event.code == KeyCode::Char('l') && key_event.modifiers.contains(KeyModifiers::CONTROL) {
                                    self.modal = ModalState::new_provider_select();
                                    continue;
                                }

                                // Ctrl+R triggers code review
                                if key_event.code == KeyCode::Char('r') && key_event.modifiers.contains(KeyModifiers::CONTROL) {
                                    self.timeline.add_user_message("/review".to_string());
                                    self.timeline.add_status("🛡️ Running multi-agent adversarial code review...".to_string());
                                    match crate::git::GitReviewer::review_workspace(&self.workspace_root, false).await {
                                        Ok(report) => {
                                            let formatted = crate::git::GitReviewer::format_report(&report);
                                            self.timeline.entries.push(crate::ui::view::TimelineEntry::AssistantMarkdown(formatted));
                                        }
                                        Err(e) => {
                                            self.timeline.add_status(format!("✗ Code review error: {}", e));
                                        }
                                    }
                                    continue;
                                }

                                // F1 opens interactive Help modal
                                if key_event.code == KeyCode::F(1) {
                                    self.modal = ModalState::Help;
                                    continue;
                                }

                                // F2 opens interactive Workspace Analysis modal
                                if key_event.code == KeyCode::F(2) {
                                    self.modal = ModalState::new_provider_select();
                                    continue;
                                }

                                // F5 opens interactive Workspace Analysis modal
                                if key_event.code == KeyCode::F(5) {
                                    self.modal = ModalState::new_workspace_analysis(&self.workspace_root);
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

                                // Check for Ctrl+C to interrupt turn or confirm exit
                                if key_event.code == KeyCode::Char('c') && key_event.modifiers.contains(KeyModifiers::CONTROL) {
                                    if self.is_working {
                                        if let Some(token) = self.cancel_token.take() {
                                            token.cancel();
                                        }
                                        self.is_working = false;
                                        self.current_activity = None;
                                        let elapsed_secs = self.work_start.take().map(|s| s.elapsed().as_secs_f64());
                                        self.timeline.finalize_pending_thoughts(elapsed_secs);
                                        self.timeline.add_status("⏹ Turn interrupted by user (Ctrl+C)".to_string());
                                        self.timeline.auto_scroll.set(true);
                                        continue;
                                    } else {
                                        let now = Instant::now();
                                        if let Some(last) = self.last_ctrl_c {
                                            if now.duration_since(last) < Duration::from_millis(1500) {
                                                break;
                                            }
                                        }
                                        self.last_ctrl_c = Some(now);
                                        self.modal = ModalState::new_exit_confirm(&self.workspace_root);
                                        continue;
                                    }
                                }

                                if key_event.code == KeyCode::Esc {
                                    if self.is_working {
                                        if let Some(token) = self.cancel_token.take() {
                                            token.cancel();
                                        }
                                        self.is_working = false;
                                        self.current_activity = None;
                                        let elapsed_secs = self.work_start.take().map(|s| s.elapsed().as_secs_f64());
                                        self.timeline.finalize_pending_thoughts(elapsed_secs);
                                        self.timeline.add_status("⏹ Turn interrupted by user (Esc)".to_string());
                                        self.timeline.auto_scroll.set(true);
                                        continue;
                                    } else if self.timeline.has_selection() {
                                        self.timeline.clear_selection();
                                        continue;
                                    }
                                }

                                // Dedicated timeline scroll keys: PageUp / PageDown / Home / End / Shift+Up / Shift+Down / Ctrl+Up / Ctrl+Down / Alt+Up / Alt+Down
                                let viewport_h = self.timeline.timeline_viewport_height();
                                if key_event.code == KeyCode::PageUp {
                                    self.timeline.scroll_page_up(viewport_h);
                                    continue;
                                }
                                if key_event.code == KeyCode::PageDown {
                                    self.timeline.scroll_page_down(viewport_h);
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

                                // If input dock has no active slash palette and textarea is single-line empty, Up/Down scroll timeline
                                let is_input_empty = self.input_dock.textarea.lines().len() <= 1 && self.input_dock.textarea.lines().first().map(|l| l.is_empty()).unwrap_or(true);
                                let has_slash_matching = self.input_dock.has_active_slash_query();
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

                                match self.handle_command_or_prompt(&prompt, &control_tx).await {
                                    Ok(CommandAction::Continue) => continue,
                                    Ok(CommandAction::Exit) => return Ok(()),
                                    Err(e) => {
                                        self.timeline.add_status(format!("✗ Error: {}", e));
                                        continue;
                                    }
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

    /// Resolves a pending approval by tool_id, if one is registered.
    fn resolve_approval(&self, tool_id: &str, decision: crate::agent::types::ApprovalDecision) {
        if let Some(sender) = self
            .approvals
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(tool_id)
        {
            let _ = sender.send(decision);
        }
    }

    /// Pushes the current config (e.g. after AllowSession) into the agent actor
    /// so the approval gate observes the updated `auto_approve` policy.
    fn propagate_session_config(&self, control_tx: &mpsc::UnboundedSender<AgentCommand>) {
        let key_res = self.config.get_api_key(&self.config.provider.default);
        let custom_url = self
            .config
            .get_provider_base_url(&self.config.provider.default);
        let (new_prov, prov_err) = crate::agent::provider::create_provider_or_fallback(
            &self.config.provider.default,
            key_res,
            custom_url.as_deref(),
        );
        if let Some(e) = prov_err {
            tracing::warn!(error = %e, "AllowSession: provider fallback in use; auto_approve applies next turn");
        }
        let _ = control_tx.send(AgentCommand::UpdateConfig {
            config: Box::new(self.config.clone()),
            provider: new_prov,
        });
    }
}

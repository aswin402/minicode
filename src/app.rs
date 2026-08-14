use crate::agent::types::AgentEvent;
use crate::agent::AgentLoop;
use crate::config::Config;
use crate::error::Result;
use crate::ui::{InputDock, StatusWidgets, Theme, TimelineView};
use crossterm::event::{Event, EventStream, KeyCode, KeyModifiers};
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
            is_working: false,
            work_start: None,
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

        let mut ticker = tokio::time::interval(Duration::from_millis(50));

        loop {
            let working_secs = self.work_start.map(|s| s.elapsed().as_secs()).unwrap_or(0);

            // Render UI with Aura Theme aesthetic
            terminal.draw(|frame| {
                let background_block = Block::default()
                    .borders(Borders::NONE)
                    .style(Style::default().bg(self.theme.bg_primary));
                frame.render_widget(background_block, frame.area());

                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Min(4),    // Streaming Timeline
                        Constraint::Length(3), // Input Dock (Elevated bar)
                        Constraint::Length(1), // Minimal Bottom Status Line
                    ])
                    .split(frame.area());

                self.timeline
                    .render(frame, chunks[0], &self.theme, self.is_working, working_secs);

                self.input_dock.render(frame, chunks[1], &self.theme);

                StatusWidgets::render_bottom_bar(
                    frame,
                    chunks[2],
                    &self.theme,
                    &self.workspace_root,
                    &self.config.provider.model,
                );
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
                            self.timeline.scroll_offset = self.timeline.scroll_offset.saturating_sub(4);
                            continue;
                        }
                        if key_event.code == KeyCode::PageDown {
                            self.timeline.scroll_offset = self.timeline.scroll_offset.saturating_add(4);
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
}

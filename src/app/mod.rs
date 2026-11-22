use crate::agent::models::ModelFetcher;
use crate::agent::types::AgentEvent;
use crate::agent::AgentLoop;
use crate::config::Config;
use crate::error::Result;
use crate::ui::{InputDock, ModalState, StatusWidgets, Theme, TimelineContext, TimelineView};
use crossterm::event::{
    DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture, Event,
    EventStream, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use futures::StreamExt;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
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
    HydrateSession {
        session_id: String,
        events: Vec<crate::agent::types::AgentEvent>,
    },
    NewSession,
}

pub struct App<'a> {
    pub workspace_root: std::path::PathBuf,
    pub config: Config,
    pub theme: Theme,
    pub timeline: TimelineView,
    pub input_dock: InputDock<'a>,
    pub pty_drawer: crate::ui::PtyDrawer,
    pub subagent_drawer: crate::ui::SubagentDrawer,
    pub modal: ModalState,
    /// Navigation stack of parent modals (e.g. `/commands` -> `/theme`) so `[← Back]` / `Esc` returns cleanly.
    pub modal_history: Vec<ModalState>,
    /// Cell hitboxes of the `[← Back]` and `[× Close]` pill buttons rendered on the active modal.
    modal_nav_chrome: std::cell::RefCell<crate::ui::modals::ModalNavRegions>,
    model_fetcher: ModelFetcher,
    is_working: bool,
    current_activity: Option<crate::ui::AgentActivity>,
    work_start: Option<Instant>,
    cancel_token: Option<tokio_util::sync::CancellationToken>,
    last_user_prompt: Option<String>,
    last_turn_tokens: usize,
    last_turn_cached_tokens: usize,
    saw_turn_context_update: bool,
    cumulative_tokens: usize,
    total_cost_usd: f64,
    /// Handle to the agent's in-flight approval requests.
    approvals: crate::agent::types::ApprovalRegistry,
    /// Handle to the agent's in-flight inquiry requests.
    pub inquiries: crate::agent::inquiry::InquiryRegistry,
    pub should_exit: bool,
    pub last_ctrl_c: Option<Instant>,
    pub pending_submission: Option<PendingSubmission>,
    pub active_task: Option<String>,
    pub active_plan: Option<crate::ui::view::LivePlanBlock>,
    pub toasts: crate::ui::ToastManager,
    pub notifier: crate::ui::SystemNotifier,
    last_bg_task_count: usize,
    turn_tool_steps: usize,
    handling_live_steering: bool,
    last_pool_version: u64,
    last_minipub_states:
        std::collections::HashMap<String, crate::agent::subagent::types::SubagentState>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingSubmission {
    pub prompt: String,
    pub display: String,
}

fn map_working_memory_tasks(
    tasks: &[crate::context::memory::working_memory::TaskItem],
) -> Vec<crate::ui::view::LivePlanTaskItem> {
    tasks
        .iter()
        .map(|t| crate::ui::view::LivePlanTaskItem {
            title: t.title.clone(),
            status: match t.status {
                crate::context::memory::working_memory::TaskItemStatus::Completed => {
                    crate::ui::view::LivePlanTaskStatus::Completed
                }
                crate::context::memory::working_memory::TaskItemStatus::InProgress => {
                    crate::ui::view::LivePlanTaskStatus::InProgress
                }
                crate::context::memory::working_memory::TaskItemStatus::Pending => {
                    crate::ui::view::LivePlanTaskStatus::Pending
                }
            },
            sub_tasks: map_working_memory_tasks(&t.sub_tasks),
        })
        .collect()
}

impl<'a> App<'a> {
    pub fn new(workspace_root: &Path, config: Config) -> Self {
        let theme = Theme::detect(&config.ui.theme);
        let active_limit = config.provider.context_window.unwrap_or_else(|| {
            crate::agent::models::get_model_context_limit(&config.provider.model)
        });
        crate::context::budget::donut::set_active_context_limit(active_limit);

        // Always start with zero upfront modals for a clean welcome screen
        let initial_modal = ModalState::None;

        // Auto-hydrate active roadmap & milestone plan from WorkingMemory if present on disk
        let wm = crate::context::memory::working_memory::WorkingMemory::new(workspace_root);
        let (initial_active_task, initial_active_plan) = match wm.read_active_plan_summary() {
            Some(summary)
                if !summary.tasks.is_empty()
                    && (summary.total_tasks == 0
                        || summary.completed_tasks < summary.total_tasks
                        || summary.tasks.iter().any(|t| {
                            t.status
                                != crate::context::memory::working_memory::TaskItemStatus::Completed
                        })) =>
            {
                let live_tasks = map_working_memory_tasks(&summary.tasks);
                (
                    summary.active_task.clone(),
                    Some(crate::ui::view::LivePlanBlock {
                        title: summary.phase_label,
                        total_tasks: summary.total_tasks,
                        completed_tasks: summary.completed_tasks,
                        active_task: summary.active_task,
                        tasks: live_tasks,
                    }),
                )
            }
            _ => (None, None),
        };

        let (initial_bg_tasks, _, _) =
            crate::dev::registry::get_global_dev_registry().get_telemetry_snapshot();
        let (initial_pool_version, initial_minipub_states) = if workspace_root.exists() {
            let pool = crate::agent::subagent::get_global_subagent_pool(workspace_root);
            let ver = pool.state_version();
            let states = pool
                .snapshot_subagents()
                .into_iter()
                .map(|sub| (sub.id, sub.state))
                .collect();
            (ver, states)
        } else {
            (0, std::collections::HashMap::new())
        };

        let mut input_dock = InputDock::new_with_workspace(workspace_root);
        input_dock.menu_style = config.ui.menu_style_enum();
        input_dock.box_style = config.ui.input_box_style_enum();

        Self {
            workspace_root: workspace_root.to_path_buf(),
            config,
            theme,
            timeline: TimelineView::new(),
            input_dock,
            pty_drawer: crate::ui::PtyDrawer::new(),
            subagent_drawer: crate::ui::SubagentDrawer::new(),
            modal: initial_modal,
            modal_history: Vec::new(),
            modal_nav_chrome: std::cell::RefCell::new(crate::ui::modals::ModalNavRegions::default()),
            model_fetcher: ModelFetcher::new(),
            is_working: false,
            current_activity: None,
            work_start: None,
            cancel_token: None,
            last_user_prompt: None,
            last_turn_tokens: 0,
            last_turn_cached_tokens: 0,
            saw_turn_context_update: false,
            cumulative_tokens: 0,
            total_cost_usd: 0.0,
            approvals: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            inquiries: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            should_exit: false,
            last_ctrl_c: None,
            pending_submission: None,
            active_task: initial_active_task,
            active_plan: initial_active_plan,
            toasts: crate::ui::ToastManager::new(),
            notifier: crate::ui::SystemNotifier::new(),
            last_bg_task_count: initial_bg_tasks,
            turn_tool_steps: 0,
            handling_live_steering: false,
            last_pool_version: initial_pool_version,
            last_minipub_states: initial_minipub_states,
        }
    }

    /// Triggers a floating toast notification without polluting the conversation timeline.
    pub fn add_timeline_status(&mut self, message: impl Into<String>) {
        let msg = message.into();
        let trimmed = msg.trim();
        let kind = if trimmed.starts_with('✕') || trimmed.starts_with('✗') {
            crate::ui::ToastKind::Error
        } else if trimmed.starts_with('⚠') || trimmed.starts_with('⏸') {
            crate::ui::ToastKind::Warning
        } else if trimmed.starts_with('✔') || trimmed.starts_with('✓') || trimmed.starts_with('✨')
        {
            crate::ui::ToastKind::Success
        } else {
            crate::ui::ToastKind::Info
        };
        let clean_body = trimmed
            .trim_start_matches(['✔', '✓', '✕', '✗', '⚠', '⚙', '✨', 'ℹ', '⏸', '⏹', '⊘'])
            .trim();
        let title = match kind {
            crate::ui::ToastKind::Error => "Operation Failed",
            crate::ui::ToastKind::Warning => "System Warning",
            crate::ui::ToastKind::Success => "Action Completed",
            _ => "System Update",
        };
        self.toasts.push(
            kind,
            title,
            clean_body,
            None,
            Some("status_banner".to_string()),
        );
    }

    /// Hydrates past session events into the timeline view
    pub fn hydrate_session(&mut self, events: &[AgentEvent]) {
        self.modal = ModalState::None;
        self.modal_history.clear();
        for event in events {
            match event {
                AgentEvent::UserPrompt { prompt, .. } => {
                    self.timeline.add_user_message(prompt.clone());
                    if !crate::ui::prompt_corpus::is_excluded_from_prompt_history(prompt) {
                        self.input_dock.history.seed_prompt(prompt);
                    }
                }
                AgentEvent::TurnStart { context_tokens, .. } => {
                    self.saw_turn_context_update = false;
                    if *context_tokens > 0 {
                        self.last_turn_tokens = *context_tokens;
                    }
                }
                AgentEvent::ThinkingDelta { delta, .. } => {
                    self.timeline.append_thought_delta(delta);
                }
                AgentEvent::StreamDelta { delta, .. } => {
                    if self.timeline.has_open_thought() && !self.timeline.in_thought_mode {
                        self.timeline.finalize_pending_thoughts(None);
                    }
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
                AgentEvent::TurnEnd {
                    total_tokens_used,
                    cached_prompt_tokens,
                    ..
                } => {
                    self.timeline.finalize_open_entries_on_turn_end();
                    if *total_tokens_used > 0 {
                        if !self.saw_turn_context_update {
                            self.last_turn_tokens = *total_tokens_used;
                        }
                        self.cumulative_tokens =
                            self.cumulative_tokens.saturating_add(*total_tokens_used);
                    }
                    if *cached_prompt_tokens > 0 {
                        self.last_turn_cached_tokens = *cached_prompt_tokens;
                    }
                }
                AgentEvent::ContextTokensUpdated {
                    used_tokens,
                    cached_tokens,
                    ..
                } => {
                    if *used_tokens > 0 {
                        self.last_turn_tokens = *used_tokens;
                        self.saw_turn_context_update = true;
                    }
                    if *cached_tokens > 0 {
                        self.last_turn_cached_tokens = *cached_tokens;
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
                    self.timeline.add_context_compaction(
                        *tier,
                        *turns_summarized,
                        *tokens_before,
                        *tokens_after,
                        *savings_percent,
                    );
                }
                AgentEvent::GitCommit {
                    hash,
                    message,
                    files,
                    ..
                } => {
                    self.timeline
                        .add_git_commit(hash.clone(), message.clone(), files.clone());
                }
                AgentEvent::Error {
                    message, retrying, ..
                } => {
                    if *retrying {
                        self.timeline.add_status(message.clone());
                    } else {
                        self.timeline.add_status(format!("✕ Error: {}", message));
                    }
                }
                AgentEvent::AntiThrashTripped { intervention, .. } => {
                    let clean_msg = intervention
                        .lines()
                        .find(|l| l.contains("•") || l.contains("Notice:") || l.contains("Reason:"))
                        .unwrap_or("Repetitive operation detected; adaptive guidance applied.");
                    let line = clean_msg.trim().trim_start_matches('•').trim();
                    self.timeline.add_status(format!("⚠ Guidance: {}", line));
                }
                AgentEvent::PlanUpdated {
                    phase_label,
                    total_tasks,
                    completed_tasks,
                    active_task,
                    tasks,
                    ..
                } => {
                    self.active_task = active_task.clone();
                    if tasks.is_empty() {
                        self.active_plan = None;
                    } else {
                        let live_tasks = map_working_memory_tasks(tasks);
                        let next_plan = crate::ui::view::LivePlanBlock {
                            title: phase_label.clone().unwrap_or_else(|| "Plan".to_string()),
                            total_tasks: *total_tasks,
                            completed_tasks: *completed_tasks,
                            active_task: active_task.clone(),
                            tasks: live_tasks,
                        };
                        self.timeline
                            .record_plan_progress_update(self.active_plan.as_ref(), &next_plan);
                        self.active_plan = Some(next_plan);
                    }
                }
                _ => {}
            }
        }
        self.timeline.finalize_open_entries_on_turn_end();

        // Synchronize live plan state with WorkingMemory on disk when resuming a workspace session
        let wm = crate::context::memory::working_memory::WorkingMemory::new(&self.workspace_root);
        if let Some(summary) = wm.read_active_plan_summary() {
            self.active_task = summary.active_task.clone();
            if summary.tasks.is_empty() {
                self.active_plan = None;
            } else {
                let live_tasks = map_working_memory_tasks(&summary.tasks);
                self.active_plan = Some(crate::ui::view::LivePlanBlock {
                    title: summary.phase_label,
                    total_tasks: summary.total_tasks,
                    completed_tasks: summary.completed_tasks,
                    active_task: summary.active_task,
                    tasks: live_tasks,
                });
            }
        } else if !wm.task_plan_path().exists()
            && self.workspace_root.join(".minicode/plan/archive").exists()
        {
            self.active_task = None;
            self.active_plan = None;
        }

        if self.last_turn_tokens == 0 {
            self.last_turn_tokens = Self::estimate_context_tokens(0, &self.timeline);
        }
    }

    /// Returns the live or estimated current context token count from components in O(1) time.
    pub fn estimate_context_tokens(last_turn_tokens: usize, timeline: &TimelineView) -> usize {
        if last_turn_tokens > 0 {
            last_turn_tokens
        } else {
            timeline.estimated_content_chars() / 4
        }
    }

    /// Returns the live or estimated current context token count for UI and diagnostics
    pub fn current_context_tokens(&self) -> usize {
        Self::estimate_context_tokens(self.last_turn_tokens, &self.timeline)
    }

    /// Runs the full-screen interactive Ratatui TUI session in Aura Theme styling.
    pub async fn run(&mut self, mut agent: AgentLoop) -> Result<()> {
        enable_raw_mode()?;
        let mut stdout = stdout();
        execute!(
            stdout,
            EnterAlternateScreen,
            EnableMouseCapture,
            EnableBracketedPaste
        )?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        let mut event_stream = EventStream::new();
        let (control_tx, mut control_rx) = mpsc::unbounded_channel::<AgentCommand>();
        let (event_tx, mut event_rx) = mpsc::unbounded_channel::<AgentEvent>();

        // Bind to the live agent's approval and inquiry registries before it moves into the actor.
        self.approvals = agent.approval_registry();
        self.inquiries = agent.inquiry_registry();
        agent.set_async_idle_wakeup_enabled(true);
        crate::dev::registry::get_global_dev_registry().set_async_idle_wakeup_enabled(true);
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
                    AgentCommand::HydrateSession { session_id, events } => {
                        agent.hydrate_from_events(&session_id, &events);
                    }
                    AgentCommand::NewSession => {
                        agent.reset_session();
                    }
                }
            }
        });

        let mut scheduler = crate::ui::FrameScheduler::new();

        loop {
            if self.should_exit {
                break;
            }

            if scheduler.should_draw_now(Instant::now()) {
                let working_millis = self
                    .work_start
                    .map(|s| s.elapsed().as_millis() as u64)
                    .unwrap_or(0);

                // Render UI with Aura Theme aesthetic wrapped in DEC Mode 2026 atomic frame sync
                scheduler.draw_synchronized(&mut terminal, |frame| {
                    let background_block = Block::default()
                        .borders(Borders::NONE)
                        .style(Style::default().bg(self.theme.bg_primary));
                    frame.render_widget(background_block, frame.area());

                    if self.timeline.is_empty() {
                        self.input_dock.box_style = self.config.ui.input_box_style_enum();
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
                            &mut self.input_dock,
                        );
                    } else {
                        let input_height = self.input_dock.required_height();
                        // Reserve minimum 4 timeline rows + 2 activity/spacer rows + input dock + 2 status rows
                        let max_todo_height = frame
                            .area()
                            .height
                            .saturating_sub(input_height.saturating_add(8));
                        let todo_style = self.config.ui.todo_style_enum();
                        let show_bottom_todo = self.active_plan.as_ref().is_some_and(|p| {
                            !p.tasks.is_empty()
                                && (p.total_tasks == 0
                                    || p.completed_tasks < p.total_tasks
                                    || p.tasks.iter().any(|t| {
                                        t.status != crate::ui::view::LivePlanTaskStatus::Completed
                                    })
                                    || self.is_working)
                        });
                        let todo_height = if show_bottom_todo {
                            crate::ui::todo_widget::todo_widget_required_height_bounded(
                                &self.active_plan,
                                todo_style,
                                Some(max_todo_height),
                            )
                        } else {
                            0
                        };
                        let activity_spacer_height = todo_height + 2;
                        let chunks = Layout::default()
                            .direction(Direction::Vertical)
                            .constraints([
                                Constraint::Min(4),                         // 0: Streaming Timeline
                                Constraint::Length(activity_spacer_height), // 1: Live Todo Widget + Activity Bar + Spacer
                                Constraint::Length(input_height),           // 2: Dynamic Input Dock
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
                            show_thought_tokens: self.config.ui.show_thought_tokens,
                        };
                        self.timeline.render(frame, chunks[0], &timeline_ctx);

                        let mut activity_lines: Vec<Line> = Vec::new();

                        // 1. Render active todo widget if present and still in progress
                        if show_bottom_todo && todo_height > 0 {
                            if let Some(ref plan) = self.active_plan {
                                let todo_lines = crate::ui::todo_widget::render_todo_widget_bounded(
                                    plan,
                                    todo_style,
                                    chunks[1].width,
                                    &self.theme,
                                    Some(max_todo_height),
                                );
                                activity_lines.extend(todo_lines);
                            }
                        }

                        // 2. Render thinking animation line on left + mode badges strip on right
                        let mut left_spans: Vec<Span> = Vec::new();
                        if self.is_working {
                            let elapsed_secs = (working_millis as f64) / 1000.0;
                            let default_act = crate::ui::animation::AgentActivity::Thinking;
                            let act = self.current_activity.as_ref().unwrap_or(&default_act);
                            let activity_line =
                                crate::ui::animation::render_live_activity_line_full(
                                    act,
                                    spinner_style,
                                    self.config.ui.activity_style_enum(),
                                    self.config.ui.activity_hint_style_enum(),
                                    self.config.ui.show_interrupt_hint,
                                    working_millis,
                                    elapsed_secs,
                                    &self.theme,
                                );
                            left_spans.push(Span::raw(" "));
                            left_spans.extend(activity_line.spans);
                        }

                        let right_spans = StatusWidgets::render_mode_bar_spans(
                            &self.theme,
                            self.config.agent.ponytail_mode,
                            &self.config.agent.ponytail_level,
                            self.config.agent.caveman_mode,
                            &self.config.agent.caveman_level,
                            self.config.agent.streaming,
                            self.config.ui.mode_bar_style_enum(),
                            self.config.ui.show_ponytail_badge,
                            self.config.ui.show_caveman_badge,
                            self.config.ui.show_streaming_badge,
                        );

                        let left_w: usize = left_spans
                            .iter()
                            .map(|s| unicode_width::UnicodeWidthStr::width(s.content.as_ref()))
                            .sum();
                        let right_w: usize = right_spans
                            .iter()
                            .map(|s| unicode_width::UnicodeWidthStr::width(s.content.as_ref()))
                            .sum();
                        let total_w = chunks[1].width as usize;

                        let mut combined_spans = left_spans;
                        if right_w > 0 {
                            if left_w + right_w < total_w {
                                let pad = total_w.saturating_sub(left_w + right_w);
                                combined_spans.push(Span::raw(" ".repeat(pad)));
                                combined_spans.extend(right_spans);
                            } else if left_w == 0 {
                                combined_spans.extend(right_spans);
                            }
                        }
                        activity_lines.push(Line::from(combined_spans));
                        activity_lines.push(Line::from(String::new()));

                        if !activity_lines.is_empty() {
                            frame.render_widget(
                                Paragraph::new(activity_lines)
                                    .style(Style::default().bg(self.theme.bg_primary)),
                                chunks[1],
                            );
                        }

                        self.input_dock.box_style = self.config.ui.input_box_style_enum();
                        self.input_dock.render(frame, chunks[2], &self.theme);

                        let active_mcp_count = self
                            .config
                            .mcp
                            .servers
                            .values()
                            .filter(|s| s.enabled)
                            .count();

                        let max_context =
                            self.config.provider.context_window.unwrap_or_else(|| {
                                crate::agent::models::get_model_context_limit(
                                    &self.config.provider.model,
                                )
                            });
                        let effort_level = self.config.provider.effective_level_label();

                        let status_ctx = crate::ui::StatusContext {
                            theme: &self.theme,
                            workspace: &self.workspace_root,
                            provider: &self.config.provider.default,
                            model: &self.config.provider.model,
                            effort_level: &effort_level,
                            mcp_count: active_mcp_count,
                            used_tokens: self.current_context_tokens(),
                            max_context,
                            show_cost: self.config.ui.show_cost,
                            session_cost_usd: self.total_cost_usd,
                            cached_tokens: self.last_turn_cached_tokens,
                            tasks_summary: Some(
                                crate::dev::registry::get_global_dev_registry()
                                    .get_telemetry_snapshot(),
                            ),
                            display: crate::ui::StatusDisplayOptions::from_ui_config(
                                &self.config.ui,
                            ),
                        };

                        StatusWidgets::render_bottom_bar(frame, chunks[4], &status_ctx);
                    }

                    // Render Floating Spotlight Command Palette or Contextual Intellisense Popup Overlay
                    if self.input_dock.has_any_overlay_open() {
                        self.input_dock.menu_style = self.config.ui.menu_style_enum();
                        self.input_dock
                            .render_slash_palette(frame, frame.area(), &self.theme);
                    }

                    // Render Modal Overlay if active
                    if self.modal.is_active() {
                        let nav_info = self.current_modal_nav_info();
                        let regions =
                            self.modal
                                .render_with_nav(frame, frame.area(), &self.theme, &nav_info);
                        *self.modal_nav_chrome.borrow_mut() = regions;
                    } else {
                        *self.modal_nav_chrome.borrow_mut() =
                            crate::ui::modals::ModalNavRegions::default();
                    }

                    // Render Embedded PTY Terminal Drawer if active
                    if self.pty_drawer.is_open {
                        self.pty_drawer.render(frame, frame.area(), &self.theme);
                    }

                    // Render Subagent Swarm Activity Drawer if active
                    if self.subagent_drawer.is_open {
                        self.subagent_drawer
                            .render(frame, frame.area(), &self.theme);
                    }

                    // Render Floating TUI Toast Notifications at top-right of viewport
                    self.toasts.render_with_style_and_symbols(
                        frame,
                        frame.area(),
                        &self.theme,
                        self.config.ui.toast_style_enum(),
                        self.config.ui.toast_symbols_enum(),
                    );

                    // Post-process 2-cell emoji cells only when wide emoji or active overlays are present
                    if self.timeline.is_empty()
                        || self.timeline.last_frame_had_wide_emoji()
                        || self.is_working
                        || self.modal.is_active()
                        || self.toasts.has_active()
                        || self.input_dock.has_any_overlay_open()
                        || self.pty_drawer.is_open
                        || self.subagent_drawer.is_open
                    {
                        crate::ui::animation::sanitize_emoji_buffer_cells(frame.buffer_mut());
                    }
                })?;
            }

            let now = Instant::now();
            let pool_opt = crate::agent::subagent::try_get_global_subagent_pool();
            let is_animating = self.is_working
                || self.timeline.has_open_thought()
                || pool_opt.is_some_and(|p| p.active_worker_count() > 0)
                || self.modal.has_live_animation();
            let next_toast_expiry = self.toasts.next_expiry_instant(now);
            let next_modal_poll = match self.modal {
                crate::ui::modals::ModalState::Processes(ref state) => {
                    Some(state.last_refresh + Duration::from_secs(1))
                }
                _ => None,
            };
            let deadline_opt =
                scheduler.next_deadline(now, is_animating, next_toast_expiry, next_modal_poll);
            let sleep_fut = async {
                match deadline_opt {
                    Some(deadline) => {
                        tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)).await;
                    }
                    None => std::future::pending::<()>().await,
                }
            };
            let dev_registry = crate::dev::registry::get_global_dev_registry();
            // Zero-lost-wakeup guard: if a background MiniPub or minitask finished while
            // `event_rx.recv()` or a modal was active (so `Notify::notify_waiters()` fired before
            // we re-entered `tokio::select!`), dispatch the continuation wakeup immediately!
            if !self.is_working && !self.modal.is_active() && dev_registry.is_continuation_armed() {
                let bg_flags = self
                    .sync_background_telemetry_and_continuation(&control_tx)
                    .await;
                if !bg_flags.is_empty() {
                    scheduler.mark_dirty(bg_flags | crate::ui::DirtyFlags::STATUS);
                    continue;
                }
            }
            let pool_notified_fut = async {
                match pool_opt {
                    Some(pool) => pool.notified().await,
                    None => std::future::pending::<()>().await,
                }
            };

            tokio::select! {
                    // Demand-driven frame deadline, animation tick, toast expiry, or live modal poll
                    _ = sleep_fut => {
                        scheduler.mark_dirty(crate::ui::DirtyFlags::ANIMATION | crate::ui::DirtyFlags::OVERLAY);
                        let bg_flags = self.sync_background_telemetry_and_continuation(&control_tx).await;
                        scheduler.mark_dirty(bg_flags);

                        if let crate::ui::modals::ModalState::Processes(ref mut state) = self.modal {
                            if state.last_refresh.elapsed() >= std::time::Duration::from_millis(1000) {
                                state.refresh_live().await;
                                if state.active_tab == crate::ui::modals::processes::ProcessesTab::Swarm {
                                    let active_style = self.config.ui.swarm_style_enum();
                                    let selected = state
                                        .swarm_deck_data
                                        .as_ref()
                                        .map(|d| d.selected_index)
                                        .unwrap_or(0);
                                    state.swarm_deck_data = Some(Box::new(
                                        crate::ui::modals::swarm_deck::SwarmDeckData::load(
                                            &self.workspace_root,
                                            selected,
                                            active_style,
                                        )
                                        .await,
                                    ));
                                }
                                scheduler.mark_dirty(crate::ui::DirtyFlags::OVERLAY);
                            }
                        }
                    }

                    // Reactive background task lifecycle or live steering wakeup (works at 0% idle CPU!)
                    _ = dev_registry.notified_task_or_steering() => {
                        let bg_flags = self.sync_background_telemetry_and_continuation(&control_tx).await;
                        scheduler.mark_dirty(bg_flags | crate::ui::DirtyFlags::STATUS);
                    }

                    // Reactive MiniPub subagent pool state transition wakeup (works at 0% idle CPU!)
                    _ = pool_notified_fut => {
                        let bg_flags = self.sync_background_telemetry_and_continuation(&control_tx).await;
                        scheduler.mark_dirty(bg_flags | crate::ui::DirtyFlags::STATUS);
                    }

                    // Handle Agent streaming events from background actor (coalescing bursts up to 128 events per frame)
                    Some(first_event) = event_rx.recv() => {
                        let mut next_event = Some(first_event);
                        let mut processed = 0usize;
                        while let Some(agent_event) = next_event.take() {
                        match &agent_event {
                            AgentEvent::StreamDelta { .. } | AgentEvent::ThinkingDelta { .. } => {
                                scheduler.mark_dirty(
                                    crate::ui::DirtyFlags::TIMELINE_TAIL | crate::ui::DirtyFlags::STATUS,
                                );
                            }
                            AgentEvent::ContextTokensUpdated { .. } | AgentEvent::Heartbeat { .. } => {
                                scheduler.mark_dirty(crate::ui::DirtyFlags::STATUS);
                            }
                            _ => {
                                scheduler.mark_dirty(
                                    crate::ui::DirtyFlags::TIMELINE_LAYOUT
                                        | crate::ui::DirtyFlags::STATUS
                                        | crate::ui::DirtyFlags::OVERLAY,
                                );
                            }
                        }
                        // If turn was cancelled or finished, ignore trailing stream/tool events
                        // while still allowing background MiniPub subagent events to update the timeline!
                        if !self.is_working {
                            match agent_event {
                                AgentEvent::TurnEnd {
                                    total_tokens_used,
                                    cached_prompt_tokens,
                                    ..
                                } => {
                                    if total_tokens_used > 0 {
                                        if !self.saw_turn_context_update {
                                            self.last_turn_tokens = total_tokens_used;
                                        }
                                        self.cumulative_tokens =
                                            self.cumulative_tokens.saturating_add(total_tokens_used);
                                    }
                                    self.last_turn_cached_tokens = cached_prompt_tokens;
                                    self.cancel_token = None;
                                    self.timeline.finalize_open_entries_on_turn_end();
                                }
                                AgentEvent::ContextTokensUpdated {
                                    used_tokens,
                                    cached_tokens,
                                    ..
                                } => {
                                    if used_tokens > 0 {
                                        self.last_turn_tokens = used_tokens;
                                        self.saw_turn_context_update = true;
                                    }
                                    if cached_tokens > 0 {
                                        self.last_turn_cached_tokens = cached_tokens;
                                    }
                                }
                                AgentEvent::SubagentProgress {
                                    subagent_id,
                                    role,
                                    action,
                                    status,
                                    ..
                                } => {
                                    self.timeline.handle_subagent_progress(
                                        &subagent_id,
                                        &role,
                                        &action,
                                        &status,
                                    );
                                }
                                AgentEvent::SubagentCompleted {
                                    subagent_id,
                                    role,
                                    success,
                                    summary,
                                    ..
                                } => {
                                    self.timeline.handle_subagent_completed(
                                        &subagent_id,
                                        &role,
                                        success,
                                        &summary,
                                    );
                                }
                                _ => {}
                            }
                            processed += 1;
                            if processed >= 128 {
                                break;
                            }
                            next_event = event_rx.try_recv().ok();
                            continue;
                        }

                        let turn_elapsed = self.work_start.map(|s| s.elapsed());
                        self.toasts.ingest_agent_event(&agent_event, turn_elapsed);
                        self.notifier.ingest_agent_event(&agent_event, turn_elapsed);

                        match agent_event {
                            AgentEvent::TurnStart { context_tokens, .. } => {
                                self.is_working = true;
                                self.saw_turn_context_update = false;
                                self.turn_tool_steps = 0;
                                if self.active_plan.as_ref().is_some_and(|p| {
                                    p.total_tasks > 0 && p.completed_tasks >= p.total_tasks
                                }) && !crate::context::memory::working_memory::WorkingMemory::new(
                                    &self.workspace_root,
                                )
                                .has_active_plan()
                                {
                                    self.active_plan = None;
                                }
                                self.timeline.auto_scroll.set(true);
                                self.timeline.scroll_to_bottom();
                                if context_tokens > 0 {
                                    self.last_turn_tokens = context_tokens;
                                }
                                if self.current_activity.is_none() {
                                    self.current_activity =
                                        Some(crate::ui::AgentActivity::Thinking);
                                }
                            }
                            AgentEvent::ThinkingDelta { delta, .. } => {
                                self.timeline.append_thought_delta(&delta);
                                let delta_tokens = delta.len().div_ceil(4);
                                self.last_turn_tokens = self.last_turn_tokens.saturating_add(delta_tokens);
                                if !matches!(
                                    self.current_activity,
                                    Some(crate::ui::AgentActivity::ThinkingAfter { .. })
                                ) {
                                    self.current_activity =
                                        Some(crate::ui::AgentActivity::Thinking);
                                }
                            }
                            AgentEvent::StreamDelta { delta, .. } => {
                                if self.timeline.has_open_thought()
                                    && !self.timeline.in_thought_mode
                                {
                                    self.timeline.finalize_pending_thoughts(None);
                                }
                                self.timeline.append_assistant_delta(&delta);
                                // Real-time incremental context token tick during streaming
                                let delta_tokens = delta.len().div_ceil(4);
                                self.last_turn_tokens = self.last_turn_tokens.saturating_add(delta_tokens);
                                if self.timeline.in_thought_mode {
                                    if !matches!(
                                        self.current_activity,
                                        Some(crate::ui::AgentActivity::ThinkingAfter { .. })
                                    ) {
                                        self.current_activity =
                                            Some(crate::ui::AgentActivity::Thinking);
                                    }
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
                                self.turn_tool_steps = self.turn_tool_steps.saturating_add(1);
                                self.current_activity =
                                    Some(crate::ui::AgentActivity::ThinkingAfter {
                                        last_tool: tool.clone(),
                                        step: self.turn_tool_steps,
                                    });
                                let output_tokens = output.len() / 4;
                                self.last_turn_tokens = self.last_turn_tokens.saturating_add(output_tokens);
                                self.timeline
                                    .finish_tool_call(&tool, success, output, duration_ms);
                            }
                            AgentEvent::ContextTokensUpdated {
                                used_tokens,
                                cached_tokens,
                                ..
                            } => {
                                if used_tokens > 0 {
                                    self.last_turn_tokens = used_tokens;
                                    self.saw_turn_context_update = true;
                                }
                                if cached_tokens > 0 {
                                    self.last_turn_cached_tokens = cached_tokens;
                                }
                            }
                            AgentEvent::TurnEnd {
                                status,
                                total_tokens_used,
                                cached_prompt_tokens,
                                ..
                            } => {
                                if total_tokens_used > 0 {
                                    if !self.saw_turn_context_update {
                                        self.last_turn_tokens = total_tokens_used;
                                    }
                                    self.cumulative_tokens = self.cumulative_tokens.saturating_add(total_tokens_used);
                                }
                                self.last_turn_cached_tokens = cached_prompt_tokens;
                                let prompt_toks = (total_tokens_used * 3) / 4;
                                let comp_toks = total_tokens_used / 4;
                                let turn_cost =
                                    crate::agent::pricing::ModelPricing::calculate_cost_with_custom(
                                        &self.config.provider.default,
                                        &self.config.provider.model,
                                        prompt_toks,
                                        comp_toks,
                                        self.config.provider.prompt_cost_per_m,
                                        self.config.provider.completion_cost_per_m,
                                    );
                                self.total_cost_usd += turn_cost;
                                self.timeline.finalize_open_entries_on_turn_end();
                                if status == crate::constants::TURN_STATUS_ITERATION_LIMIT {
                                    self.timeline.add_status(
                                        "⏸ Turn paused: reached tool iteration limit. Type 'continue' to resume."
                                            .to_string(),
                                    );
                                }
                                self.is_working = false;
                                self.handling_live_steering = false;
                                self.current_activity = None;
                                self.work_start = None;
                                self.cancel_token = None;
                            }
                            AgentEvent::GitCommit {
                                hash,
                                message,
                                files,
                                ..
                            } => {
                                self.timeline.add_git_commit(hash, message, files);
                            }
                            AgentEvent::ApprovalRequest {
                                turn_id,
                                tool_id,
                                tool,
                                args,
                                ..
                            } => {
                                self.timeline.finalize_pending_thoughts(None);
                                let is_mandatory =
                                    crate::agent::r#loop::classify_mandatory_approval_with_workspace(
                                        &tool,
                                        &args,
                                        Some(&self.workspace_root),
                                    )
                                    .is_some();
                                if self.config.agent.auto_approve && !is_mandatory {
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
                            AgentEvent::UserInquiry {
                                turn_id,
                                tool_id,
                                request,
                            } => {
                                self.timeline.finalize_pending_thoughts(None);
                                let inq_state =
                                    crate::ui::modals::inquiry::InquiryModalState::from_request(
                                        turn_id,
                                        &tool_id,
                                        request,
                                    );
                                self.modal = ModalState::Inquiry(inq_state);
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
                                    self.timeline.finalize_open_entries_on_turn_end();
                                    self.timeline
                                        .add_status(format!("✕ Error: {}", message));
                                    self.is_working = false;
                                    self.handling_live_steering = false;
                                    self.current_activity = None;
                                    self.work_start = None;
                                    self.cancel_token = None;
                                }
                            }
                            AgentEvent::AntiThrashTripped { intervention, .. } => {
                                let clean_msg = intervention
                                    .lines()
                                    .find(|l| l.contains("•") || l.contains("Notice:") || l.contains("Reason:"))
                                    .unwrap_or("Repetitive operation detected; adaptive guidance applied.");
                                let line = clean_msg.trim().trim_start_matches('•').trim();
                                self.timeline.add_status(format!("⚠ Guidance: {}", line));
                            }
                            AgentEvent::SubagentProgress {
                                subagent_id,
                                role,
                                action,
                                status,
                                ..
                            } => {
                                if self.is_working
                                    && !self.handling_live_steering
                                    && !crate::dev::registry::get_global_dev_registry()
                                        .has_steering_prompts()
                                    && !matches!(
                                        self.current_activity,
                                        Some(crate::ui::AgentActivity::Responding)
                                    )
                                {
                                    self.current_activity =
                                        Some(crate::ui::AgentActivity::SubagentWorking {
                                            role: format!("{} ({})", role, subagent_id),
                                        });
                                }
                                self.timeline.handle_subagent_progress(
                                    &subagent_id,
                                    &role,
                                    &action,
                                    &status,
                                );
                            }
                            AgentEvent::SubagentCompleted {
                                subagent_id,
                                role,
                                success,
                                summary,
                                ..
                            } => {
                                self.timeline.handle_subagent_completed(
                                    &subagent_id,
                                    &role,
                                    success,
                                    &summary,
                                );
                                if self.is_working {
                                    let next_active =
                                        crate::agent::subagent::try_get_global_subagent_pool()
                                            .and_then(|pool| {
                                                pool.snapshot_subagents()
                                                    .into_iter()
                                                    .find(|w| w.state.is_active() && w.id != subagent_id)
                                            });
                                    if let Some(active_w) = next_active {
                                        if !self.handling_live_steering
                                            && !crate::dev::registry::get_global_dev_registry()
                                                .has_steering_prompts()
                                            && !matches!(
                                                self.current_activity,
                                                Some(crate::ui::AgentActivity::Responding)
                                            )
                                        {
                                            self.current_activity =
                                                Some(crate::ui::AgentActivity::SubagentWorking {
                                                    role: format!(
                                                        "{} ({})",
                                                        active_w.role.badge(),
                                                        active_w.id
                                                    ),
                                                });
                                        }
                                    } else {
                                        self.current_activity =
                                            Some(crate::ui::AgentActivity::Thinking);
                                    }
                                }
                            }
                            AgentEvent::PlanUpdated {
                                phase_label,
                                total_tasks,
                                completed_tasks,
                                active_task,
                                tasks,
                                ..
                            } => {
                                self.active_task = active_task.clone();
                                if tasks.is_empty() {
                                    let keep_completed_summary =
                                        self.active_plan.as_ref().is_some_and(|p| {
                                            p.total_tasks > 0 && p.completed_tasks >= p.total_tasks
                                        });
                                    if !keep_completed_summary {
                                        self.active_plan = None;
                                    }
                                } else {
                                    let live_tasks = map_working_memory_tasks(&tasks);
                                    let next_plan = crate::ui::view::LivePlanBlock {
                                        title: phase_label
                                            .clone()
                                            .unwrap_or_else(|| "Plan".to_string()),
                                        total_tasks,
                                        completed_tasks,
                                        active_task,
                                        tasks: live_tasks,
                                    };
                                    self.timeline.record_plan_progress_update(
                                        self.active_plan.as_ref(),
                                        &next_plan,
                                    );
                                    self.active_plan = Some(next_plan);
                                }
                                self.modal.refresh_todo_if_open(&self.workspace_root);
                            }
                            AgentEvent::Heartbeat { status, .. } => {
                                if let Some(rest) = status.strip_prefix("tool_stream:") {
                                    self.timeline.finalize_pending_thoughts(None);
                                    let (tool_name, args_preview) =
                                        rest.split_once('\t').unwrap_or((rest, ""));
                                    if !tool_name.trim().is_empty() {
                                        self.current_activity =
                                            Some(crate::ui::AgentActivity::from_tool_call(
                                                tool_name,
                                                args_preview,
                                            ));
                                    }
                                } else if status.starts_with("Live user prompt") {
                                    self.current_activity =
                                        Some(crate::ui::AgentActivity::Thinking);
                                } else if status.starts_with("Awaiting ")
                                    && !crate::dev::registry::get_global_dev_registry()
                                        .has_steering_prompts()
                                {
                                    self.current_activity =
                                        Some(crate::ui::AgentActivity::SubagentWorking {
                                            role: status,
                                        });
                                }
                            }
                            _ => {}
                        }
                        processed += 1;
                        if processed >= 128 {
                            break;
                        }
                        next_event = event_rx.try_recv().ok();
                        }
                        if !self.is_working && !self.modal.is_active() {
                            let bg_flags = self
                                .sync_background_telemetry_and_continuation(&control_tx)
                                .await;
                            scheduler.mark_dirty(bg_flags | crate::ui::DirtyFlags::STATUS);
                        }
                    }

                    // Handle user keyboard and mouse events from terminal
                    Some(Ok(event)) = event_stream.next() => {
                        // Filter zero-visual-effect hover/release events before marking urgent redraw
                        if matches!(event, Event::Key(ref k) if k.kind == KeyEventKind::Release)
                            || matches!(event, Event::Mouse(ref m) if matches!(m.kind, MouseEventKind::Moved))
                        {
                            continue;
                        }
                        scheduler.mark_urgent(crate::ui::DirtyFlags::ALL);
                        match event {
                            Event::Paste(pasted_text) => {
                                if self.modal.is_active() {
                                    if let ModalState::ApiKeyInput { ref mut input, ref mut cursor, .. } = self.modal {
                                        let sanitized = pasted_text.trim().replace(['\r', '\n'], "");
                                        input.insert_str(*cursor, &sanitized);
                                        *cursor += sanitized.len();
                                    } else if let ModalState::Inquiry(ref mut inq_state) = self.modal {
                                        if inq_state.is_typing_custom {
                                            let sanitized = pasted_text.trim().replace(['\r', '\n'], "");
                                            for c in sanitized.chars() {
                                                inq_state.handle_char(c);
                                            }
                                        }
                                    }
                                    continue;
                                }

                                if self.pty_drawer.is_open {
                                    self.pty_drawer.input_buffer.push_str(&pasted_text);
                                    continue;
                                }

                                let _ = self.input_dock.handle_paste_with_workspace(
                                    &pasted_text,
                                    Some(&self.workspace_root),
                                );
                            }
                            Event::Mouse(mouse_event) => {
                                match mouse_event.kind {
                                    MouseEventKind::ScrollUp => {
                                        if let ModalState::Todo(ref mut todo_state) = self.modal {
                                            todo_state.scroll_up(crate::constants::SCROLL_LINES_NORMAL as usize);
                                        } else if let ModalState::Inquiry(ref mut inq_state) = self.modal {
                                            if !inq_state.is_typing_custom {
                                                inq_state.prev_option();
                                            }
                                        } else if self.subagent_drawer.is_open {
                                            let count = crate::agent::subagent::try_get_global_subagent_pool()
                                                .map(|p| p.snapshot_subagents().len())
                                                .unwrap_or(0);
                                            self.subagent_drawer.previous(count);
                                        } else if self.pty_drawer.is_open {
                                            self.pty_drawer.scroll_offset = self.pty_drawer.scroll_offset.saturating_add(crate::constants::SCROLL_LINES_NORMAL as usize);
                                        } else if !self.modal.is_active() {
                                            self.timeline.scroll_up(crate::constants::SCROLL_LINES_NORMAL);
                                        }
                                    }
                                    MouseEventKind::ScrollDown => {
                                        if let ModalState::Todo(ref mut todo_state) = self.modal {
                                            todo_state.scroll_down(crate::constants::SCROLL_LINES_NORMAL as usize);
                                        } else if let ModalState::Inquiry(ref mut inq_state) = self.modal {
                                            if !inq_state.is_typing_custom {
                                                inq_state.next_option();
                                            }
                                        } else if self.subagent_drawer.is_open {
                                            let count = crate::agent::subagent::try_get_global_subagent_pool()
                                                .map(|p| p.snapshot_subagents().len())
                                                .unwrap_or(0);
                                            self.subagent_drawer.next(count);
                                        } else if self.pty_drawer.is_open {
                                            self.pty_drawer.scroll_offset = self.pty_drawer.scroll_offset.saturating_sub(crate::constants::SCROLL_LINES_NORMAL as usize);
                                        } else if !self.modal.is_active() {
                                            self.timeline.scroll_down(crate::constants::SCROLL_LINES_NORMAL);
                                        }
                                    }
                                    MouseEventKind::Down(MouseButton::Left) => {
                                        if self.modal.is_active() {
                                            let regions = *self.modal_nav_chrome.borrow();
                                            if regions.hit_back(mouse_event.column, mouse_event.row) {
                                                self.handle_modal_back(&control_tx).await;
                                            } else if regions.hit_close(mouse_event.column, mouse_event.row) {
                                                self.handle_modal_close_all(&control_tx).await;
                                            }
                                            if !self.modal.is_active() && !self.is_working {
                                                let bg_flags = self
                                                    .sync_background_telemetry_and_continuation(&control_tx)
                                                    .await;
                                                scheduler.mark_dirty(bg_flags | crate::ui::DirtyFlags::STATUS);
                                            }
                                        } else if !self.pty_drawer.is_open && !self.subagent_drawer.is_open {
                                            self.timeline.handle_mouse_down(mouse_event.column, mouse_event.row);
                                        }
                                    }
                                    MouseEventKind::Drag(MouseButton::Left) => {
                                        if !self.pty_drawer.is_open && !self.subagent_drawer.is_open && !self.modal.is_active() {
                                            self.timeline.handle_mouse_drag(mouse_event.column, mouse_event.row);
                                        }
                                    }
                                    MouseEventKind::Up(MouseButton::Left) => {
                                        let selected_text =
                                            (!self.pty_drawer.is_open && !self.subagent_drawer.is_open && !self.modal.is_active())
                                                .then(|| {
                                                    self.timeline.handle_mouse_up(mouse_event.column, mouse_event.row)
                                                })
                                                .flatten();
                                        if let Some(selected_text) = selected_text {
                                            let trimmed = selected_text.trim();
                                            if !trimmed.is_empty() {
                                                let preview = crate::utils::truncate_ellipsis(trimmed, 28);
                                                self.add_timeline_status(format!("✔ Copied to clipboard: \"{}\"", preview));
                                            }
                                        }
                                    }
                                    MouseEventKind::Up(MouseButton::Right) => {
                                        if !self.pty_drawer.is_open && !self.subagent_drawer.is_open && !self.modal.is_active() {
                                            let text = self
                                                .timeline
                                                .extract_selected_text()
                                                .filter(|t| !t.trim().is_empty())
                                                .or_else(|| self.timeline.get_last_assistant_response());

                                            if let Some(t) = text {
                                                let trimmed = t.trim();
                                                if !trimmed.is_empty()
                                                    && crate::ui::clipboard::copy_to_clipboard(trimmed)
                                                {
                                                    let preview = crate::utils::truncate_ellipsis(trimmed, 28);
                                                    self.add_timeline_status(format!("✔ Copied to clipboard: \"{}\"", preview));
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
                                    if !self.modal.is_active() && !self.is_working {
                                        let bg_flags = self
                                            .sync_background_telemetry_and_continuation(&control_tx)
                                            .await;
                                        scheduler.mark_dirty(bg_flags | crate::ui::DirtyFlags::STATUS);
                                    }
                                    continue;
                                }

                                // Ctrl+T toggles embedded PTY terminal drawer
                                if key_event.code == KeyCode::Char('t') && key_event.modifiers.contains(KeyModifiers::CONTROL) {
                                    self.pty_drawer.toggle();
                                    continue;
                                }

                                // Ctrl+S toggles Subagent Swarm Activity Drawer
                                if key_event.code == KeyCode::Char('s') && key_event.modifiers.contains(KeyModifiers::CONTROL) {
                                    self.subagent_drawer.toggle();
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
                                            self.add_timeline_status(format!("✗ Failed to load git diff: {}", e));
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
                                            self.add_timeline_status(format!("✗ Failed to load session history: {}", e));
                                        }
                                    }
                                    continue;
                                }

                                // Ctrl+N starts fresh conversation session
                                if key_event.code == KeyCode::Char('n') && key_event.modifiers.contains(KeyModifiers::CONTROL) {
                                    let dev_reg = crate::dev::registry::get_global_dev_registry();
                                    dev_reg.set_continuation_armed(false);
                                    let _ = dev_reg.drain_steering_prompts();
                                    self.timeline = crate::ui::TimelineView::new();
                                    let _ = control_tx.send(AgentCommand::NewSession);
                                    self.add_timeline_status("✨ Started a new session".to_string());
                                    continue;
                                }

                                // Ctrl+L opens Switch Model & Provider modal
                                if key_event.code == KeyCode::Char('l') && key_event.modifiers.contains(KeyModifiers::CONTROL) {
                                    self.modal = ModalState::new_provider_select_with_config(&self.config);
                                    continue;
                                }

                                // Ctrl+P displays interactive MiniPower methodology & verification modal
                                if key_event.code == KeyCode::Char('p') && key_event.modifiers.contains(KeyModifiers::CONTROL) {
                                    self.modal = ModalState::new_minipower(&self.workspace_root);
                                    continue;
                                }

                                // Ctrl+R opens interactive Code Review modal
                                if key_event.code == KeyCode::Char('r') && key_event.modifiers.contains(KeyModifiers::CONTROL) {
                                    match crate::git::GitReviewer::review_workspace(&self.workspace_root, false).await {
                                        Ok(report) => {
                                            self.modal = ModalState::new_code_review(report, false);
                                        }
                                        Err(e) => {
                                            self.add_timeline_status(format!("✗ Code review error: {}", e));
                                        }
                                    }
                                    continue;
                                }

                                // Ctrl+X opens interactive Context Visualizer & KV-Cache Diagnostics modal
                                if key_event.code == KeyCode::Char('x') && key_event.modifiers.contains(KeyModifiers::CONTROL) {
                                    let data = crate::ui::modals::context_diagnostics::ContextDiagnosticsData::gather(
                                        &self.workspace_root,
                                        &self.config,
                                        self.current_context_tokens(),
                                        self.cumulative_tokens,
                                        self.last_turn_cached_tokens,
                                        self.timeline.entries.len(),
                                    );
                                    self.modal = ModalState::new_context_diagnostics(data);
                                    continue;
                                }

                                // Ctrl+Y or Ctrl+Shift+C copies active text selection, input text, or latest AI response to clipboard
                                let is_ctrl_y = key_event.code == KeyCode::Char('y')
                                    && key_event.modifiers.contains(KeyModifiers::CONTROL);
                                let is_ctrl_shift_c = (key_event.code == KeyCode::Char('c') || key_event.code == KeyCode::Char('C'))
                                    && key_event.modifiers.contains(KeyModifiers::CONTROL)
                                    && key_event.modifiers.contains(KeyModifiers::SHIFT);
                                if is_ctrl_y || is_ctrl_shift_c {
                                    let text = self
                                        .timeline
                                        .extract_selected_text()
                                        .filter(|t| !t.trim().is_empty())
                                        .or_else(|| {
                                            if !self.input_dock.is_empty() {
                                                Some(self.input_dock.get_text())
                                            } else {
                                                None
                                            }
                                        })
                                        .or_else(|| self.timeline.get_last_assistant_response());

                                    if let Some(t) = text {
                                        let trimmed = t.trim();
                                        if !trimmed.is_empty() {
                                            let ok = crate::ui::clipboard::copy_to_clipboard(trimmed);
                                            if ok {
                                                let preview = crate::utils::truncate_ellipsis(trimmed, 28);
                                                self.add_timeline_status(format!("✔ Copied to clipboard: \"{}\"", preview));
                                            } else {
                                                self.add_timeline_status("✗ Failed to copy to clipboard".to_string());
                                            }
                                        } else {
                                            self.add_timeline_status("ℹ Nothing to copy yet".to_string());
                                        }
                                    } else {
                                        self.add_timeline_status("ℹ Nothing to copy yet".to_string());
                                    }
                                    continue;
                                }

                                // F1 opens interactive Help modal
                                if key_event.code == KeyCode::F(1) {
                                    self.modal = ModalState::Help;
                                    continue;
                                }

                                // F2 opens interactive Provider Select modal
                                if key_event.code == KeyCode::F(2) {
                                    self.modal = ModalState::new_provider_select_with_config(&self.config);
                                    continue;
                                }

                                // F3 opens interactive Settings modal
                                if key_event.code == KeyCode::F(3) {
                                    self.modal = ModalState::new_settings(&self.config, &self.workspace_root);
                                    continue;
                                }

                                // F5 opens interactive Workspace Analysis modal
                                if key_event.code == KeyCode::F(5) {
                                    self.modal = ModalState::new_workspace_analysis(&self.workspace_root);
                                    continue;
                                }

                                // F6 toggles interactive MiniBlocks Warehouse modal
                                if key_event.code == KeyCode::F(6) {
                                    if matches!(self.modal, ModalState::Blocks(_)) {
                                        self.modal = ModalState::None;
                                    } else {
                                        self.modal = ModalState::new_blocks(&self.workspace_root);
                                    }
                                    continue;
                                }

                                // F7 toggles interactive MiniDev Process Monitor modal
                                if key_event.code == KeyCode::F(7) {
                                    if matches!(self.modal, ModalState::Processes(_)) {
                                        self.modal = ModalState::None;
                                    } else {
                                        let mut state =
                                            crate::ui::modals::processes::ProcessesModalState::new(
                                                &self.workspace_root,
                                            );
                                        state.refresh_live().await;
                                        self.modal = ModalState::Processes(state);
                                    }
                                    continue;
                                }

                                // F8 toggles interactive Implementation Roadmap & Milestone DAG modal
                                if key_event.code == KeyCode::F(8) {
                                    if matches!(self.modal, ModalState::Todo(_)) {
                                        self.modal = ModalState::None;
                                    } else {
                                        self.modal = ModalState::new_todo(&self.workspace_root);
                                    }
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

                                // When Subagent Activity Drawer is open, route navigation and worker management
                                if self.subagent_drawer.is_open {
                                    let pool = crate::agent::subagent::try_get_global_subagent_pool();
                                    let worker_count = pool.map(|p| p.snapshot_subagents().len()).unwrap_or(0);

                                    match key_event.code {
                                        KeyCode::Esc => {
                                            self.subagent_drawer.close();
                                        }
                                        KeyCode::Up | KeyCode::Char('k') => {
                                            self.subagent_drawer.previous(worker_count);
                                        }
                                        KeyCode::Down | KeyCode::Char('j') => {
                                            self.subagent_drawer.next(worker_count);
                                        }
                                        KeyCode::Enter => {
                                            self.subagent_drawer.toggle_inspect();
                                        }
                                        KeyCode::Char('x') => {
                                            if let Some(p) = pool {
                                                let workers = p.snapshot_subagents();
                                                if let Some(target) = workers.get(self.subagent_drawer.selected_index) {
                                                    let _ = p.kill_subagent(&target.id).await;
                                                    self.add_timeline_status(format!("⊘ Terminated subagent worker `{}`", target.id));
                                                }
                                            }
                                        }
                                        KeyCode::Char('a') => {
                                            if let Some(p) = pool {
                                                p.kill_all().await;
                                                self.add_timeline_status("⊘ Terminated all running subagent workers".to_string());
                                            }
                                        }
                                        _ => {}
                                    }
                                    continue;
                                }

                                // Check for Ctrl+C to interrupt turn, copy selection/input, or confirm exit
                                if key_event.code == KeyCode::Char('c')
                                    && key_event.modifiers.contains(KeyModifiers::CONTROL)
                                    && !key_event.modifiers.contains(KeyModifiers::SHIFT)
                                {
                                    if self.is_working {
                                        let dev_reg = crate::dev::registry::get_global_dev_registry();
                                        dev_reg.set_continuation_armed(false);
                                        let _ = dev_reg.drain_steering_prompts();
                                        if let Some(token) = self.cancel_token.take() {
                                            token.cancel();
                                        }
                                        if let Some(pool) =
                                            crate::agent::subagent::try_get_global_subagent_pool()
                                        {
                                            pool.kill_all().await;
                                        }
                                        let _ = dev_reg.stop_active_non_daemon_tasks().await;
                                        let _ = crate::tools::browser::BrowserManager::shutdown_live_engine_opt(true).await;
                                        let _ = dev_reg.refresh_telemetry_cache().await;
                                        self.is_working = false;
                                        self.current_activity = None;
                                        self.work_start = None;
                                        self.timeline.finalize_open_entries_on_turn_end();
                                        self.add_timeline_status("⏹ Turn interrupted by user (Ctrl+C) — stopped active subagents, tasks & browser".to_string());
                                        self.timeline.auto_scroll.set(true);
                                        continue;
                                    } else {
                                        // 1. If text is visually selected in timeline, copy selection
                                        if let Some(selected) = self.timeline.extract_selected_text().filter(|t| !t.trim().is_empty()) {
                                            let ok = crate::ui::clipboard::copy_to_clipboard(&selected);
                                            if ok {
                                                let preview = crate::utils::truncate_ellipsis(selected.trim(), 28);
                                                self.add_timeline_status(format!("✔ Copied selection to clipboard: \"{}\"", preview));
                                            }
                                            continue;
                                        }

                                        // 2. If user has typed text in input dock, copy input text
                                        if !self.input_dock.is_empty() {
                                            let input_text = self.input_dock.get_text();
                                            let ok = crate::ui::clipboard::copy_to_clipboard(input_text.trim());
                                            if ok {
                                                let preview = crate::utils::truncate_ellipsis(input_text.trim(), 28);
                                                self.add_timeline_status(format!("✔ Copied input to clipboard: \"{}\"", preview));
                                            }
                                            continue;
                                        }

                                        // 3. Otherwise, confirm exit (double Ctrl+C exits immediately)
                                        let now = Instant::now();
                                        if let Some(last) = self.last_ctrl_c {
                                            if now.duration_since(last)
                                                < Duration::from_millis(
                                                    crate::constants::DOUBLE_ESC_EXIT_WINDOW_MS,
                                                )
                                            {
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
                                        let dev_reg = crate::dev::registry::get_global_dev_registry();
                                        dev_reg.set_continuation_armed(false);
                                        let _ = dev_reg.drain_steering_prompts();
                                        if let Some(token) = self.cancel_token.take() {
                                            token.cancel();
                                        }
                                        if let Some(pool) =
                                            crate::agent::subagent::try_get_global_subagent_pool()
                                        {
                                            pool.kill_all().await;
                                        }
                                        let _ = dev_reg.stop_active_non_daemon_tasks().await;
                                        let _ = crate::tools::browser::BrowserManager::shutdown_live_engine_opt(true).await;
                                        let _ = dev_reg.refresh_telemetry_cache().await;
                                        self.is_working = false;
                                        self.current_activity = None;
                                        self.work_start = None;
                                        self.timeline.finalize_open_entries_on_turn_end();
                                        self.timeline.add_status("⏹ Turn interrupted by user (Esc) — stopped active subagents, tasks & browser".to_string());
                                        self.timeline.auto_scroll.set(true);
                                        continue;
                                    } else if self.timeline.has_selection() {
                                        self.timeline.clear_selection();
                                        continue;
                                    } else if !self.timeline.auto_scroll.get() {
                                        self.timeline.scroll_to_bottom();
                                        continue;
                                    }
                                }

                                // Dedicated timeline scroll keys: PageUp / PageDown / Home / End / Shift+Up / Shift+Down (and Ctrl/Alt+Up/Down when no input overlay is open)
                                let viewport_h = self.timeline.timeline_viewport_height();
                                let is_input_empty = self.input_dock.textarea.lines().len() <= 1 && self.input_dock.textarea.lines().first().map(|l| l.is_empty()).unwrap_or(true);
                                let has_overlay_open = self.input_dock.has_any_overlay_open();
                                if key_event.code == KeyCode::PageUp {
                                    self.timeline.scroll_page_up(viewport_h);
                                    continue;
                                }
                                if key_event.code == KeyCode::PageDown {
                                    self.timeline.scroll_page_down(viewport_h);
                                    continue;
                                }
                                if key_event.code == KeyCode::Home && (is_input_empty || key_event.modifiers.contains(KeyModifiers::CONTROL) || key_event.modifiers.contains(KeyModifiers::SHIFT)) {
                                    self.timeline.scroll_to_top();
                                    continue;
                                }
                                if key_event.code == KeyCode::End && (!self.timeline.auto_scroll.get() || is_input_empty || key_event.modifiers.contains(KeyModifiers::CONTROL) || key_event.modifiers.contains(KeyModifiers::SHIFT)) {
                                    self.timeline.scroll_to_bottom();
                                    continue;
                                }
                                if (key_event.code == KeyCode::Up)
                                    && (key_event.modifiers.contains(KeyModifiers::SHIFT)
                                        || (is_input_empty
                                            && !has_overlay_open
                                            && (key_event.modifiers.contains(KeyModifiers::CONTROL)
                                                || key_event.modifiers.contains(KeyModifiers::ALT))))
                                {
                                    self.timeline.scroll_up(crate::constants::SCROLL_LINES_NORMAL);
                                    continue;
                                }
                                if (key_event.code == KeyCode::Down)
                                    && (key_event.modifiers.contains(KeyModifiers::SHIFT)
                                        || (is_input_empty
                                            && !has_overlay_open
                                            && (key_event.modifiers.contains(KeyModifiers::CONTROL)
                                                || key_event.modifiers.contains(KeyModifiers::ALT))))
                                {
                                    self.timeline.scroll_down(crate::constants::SCROLL_LINES_NORMAL);
                                    continue;
                                }

                                // If input dock has no active overlay, no prompt history to recall, and textarea is empty, Up/Down scroll timeline
                                let has_prompt_history = !self.input_dock.history.entries.is_empty();
                                if is_input_empty && !has_overlay_open && !has_prompt_history {
                                    if key_event.code == KeyCode::Up {
                                        self.timeline.scroll_up(crate::constants::SCROLL_LINES_NORMAL);
                                        continue;
                                    }
                                    if key_event.code == KeyCode::Down {
                                        self.timeline.scroll_down(crate::constants::SCROLL_LINES_NORMAL);
                                        continue;
                                    }
                                }

                                // Shift+Tab or Alt+T cycles model effort / temperature level (low -> medium -> high)
                                let is_cycle_effort = (!has_overlay_open && key_event.code == KeyCode::BackTab)
                                    || ((key_event.code == KeyCode::Char('t') || key_event.code == KeyCode::Char('T'))
                                        && key_event.modifiers.contains(KeyModifiers::ALT));
                                if is_cycle_effort {
                                    let next_level = self.config.provider.cycle_effort_level();
                                    let _ = self.config.save(Some(&self.workspace_root));
                                    let chain = self
                                        .config
                                        .resolve_orchestrator_chain(Some(&self.workspace_root));
                                    let (new_prov, _) =
                                        crate::agent::providers::factory::create_chain_provider_for_slots(
                                            &self.config,
                                            &chain,
                                        );
                                    let _ = control_tx.send(AgentCommand::UpdateConfig {
                                        config: Box::new(self.config.clone()),
                                        provider: new_prov,
                                    });
                                    self.toasts.push_status(format!(
                                        "✔ Model effort & temperature set to '{}'",
                                        next_level
                                    ));
                                    continue;
                                }

                            // Handle Ctrl+V / Alt+V for clipboard image or text paste (`[img 1]`, `[img 2]`, etc.)
                            if (key_event.code == KeyCode::Char('v') || key_event.code == KeyCode::Char('V'))
                                && (key_event.modifiers.contains(KeyModifiers::CONTROL)
                                    || key_event.modifiers.contains(KeyModifiers::ALT))
                            {
                                let _ = self
                                    .input_dock
                                    .handle_clipboard_paste(&self.workspace_root);
                                continue;
                            }

                            // Send input to input dock
                            if let Some(submission) = self.input_dock.handle_key(key_event) {
                                let prompt = submission.full.trim().to_string();
                                if prompt.is_empty() {
                                    continue;
                                }

                                let display = submission.display.trim().to_string();
                                match self.handle_command_or_prompt(&prompt, Some(&display), &control_tx).await {
                                    Ok(CommandAction::Continue) => continue,
                                    Ok(CommandAction::Exit) => break,
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

        // Abort background task, MiniPub worker pool, and cleanup terminal state cleanly
        crate::dev::registry::get_global_dev_registry().set_async_idle_wakeup_enabled(false);
        if let Some(token) = self.cancel_token.take() {
            token.cancel();
        }
        if let Some(pool) = crate::agent::subagent::try_get_global_subagent_pool() {
            pool.kill_all().await;
        }
        agent_task.abort();
        #[cfg(unix)]
        unsafe {
            // Flush any accumulated keycodes (e.g. ^C, Escape, F-keys) from the OS tty buffer
            // so they do not spill into the parent shell prompt.
            libc::tcflush(libc::STDIN_FILENO, libc::TCIFLUSH);
        }
        disable_raw_mode()?;
        execute!(
            terminal.backend_mut(),
            DisableBracketedPaste,
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

    /// Resolves a pending inquiry by tool_id, if one is registered.
    pub fn resolve_inquiry(&self, tool_id: &str, resp: crate::agent::inquiry::InquiryResponse) {
        if let Some(sender) = self
            .inquiries
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(tool_id)
        {
            let _ = sender.send(resp);
        }
    }

    /// Pushes the current config (e.g. after AllowSession) into the agent actor
    /// so the approval gate observes the updated `auto_approve` policy.
    fn propagate_session_config(&self, control_tx: &mpsc::UnboundedSender<AgentCommand>) {
        if let Some(e) = self.sync_single_provider_config(control_tx) {
            tracing::warn!(error = %e, "AllowSession: provider fallback in use; auto_approve applies next turn");
        }
    }

    /// Rebuilds the 4-slot orchestrator chain provider and sends `AgentCommand::UpdateConfig`.
    pub(crate) fn sync_orchestrator_config(
        &self,
        control_tx: &mpsc::UnboundedSender<AgentCommand>,
    ) -> Option<String> {
        let chain = self
            .config
            .resolve_orchestrator_chain(Some(&self.workspace_root));
        let (new_prov, prov_err) =
            crate::agent::providers::factory::create_chain_provider_for_slots(&self.config, &chain);
        let _ = control_tx.send(AgentCommand::UpdateConfig {
            config: Box::new(self.config.clone()),
            provider: new_prov,
        });
        prov_err
    }

    /// Rebuilds the active single provider and sends `AgentCommand::UpdateConfig`.
    pub(crate) fn sync_single_provider_config(
        &self,
        control_tx: &mpsc::UnboundedSender<AgentCommand>,
    ) -> Option<String> {
        let prov = &self.config.provider.default;
        let custom_url = self.config.get_provider_base_url(prov);
        let key_res = self.config.get_api_key(prov);
        let (new_prov, prov_err) = crate::agent::provider::create_provider_or_fallback(
            prov,
            key_res,
            custom_url.as_deref(),
        );
        let _ = control_tx.send(AgentCommand::UpdateConfig {
            config: Box::new(self.config.clone()),
            provider: new_prov,
        });
        prov_err
    }

    /// Synchronizes background `minitask` / `MiniPub` telemetry, triggers reactive idle-state
    /// Boomerang/task continuations, and inspects subagent lifecycle transitions only when
    /// `SubagentPool::state_version()` advances.
    async fn sync_background_telemetry_and_continuation(
        &mut self,
        control_tx: &mpsc::UnboundedSender<AgentCommand>,
    ) -> crate::ui::DirtyFlags {
        let mut dirty = crate::ui::DirtyFlags::NONE;
        let dev_registry = crate::dev::registry::get_global_dev_registry();
        let (active_bg, rss_mb, _) = dev_registry.get_telemetry_snapshot();
        if active_bg != self.last_bg_task_count {
            if active_bg > self.last_bg_task_count {
                let added = active_bg - self.last_bg_task_count;
                self.toasts.push(
                    crate::ui::ToastKind::Task,
                    "Background Task Active",
                    format!("{added} background process(es) spawned ({active_bg} running)."),
                    Some(format!("{rss_mb:.1}MB")),
                    Some("bg_task_lifecycle".to_string()),
                );
            } else {
                let finished = self.last_bg_task_count - active_bg;
                self.toasts.push(
                    crate::ui::ToastKind::Success,
                    "Background Task Completed",
                    format!("{finished} background task(s) finished ({active_bg} active)."),
                    Some(format!("{active_bg} active")),
                    Some("bg_task_lifecycle".to_string()),
                );
            }
            self.last_bg_task_count = active_bg;
            dirty |= crate::ui::DirtyFlags::STATUS | crate::ui::DirtyFlags::OVERLAY;
        }

        // Reactive async continuation when a background minitask or MiniPub finishes while TUI is idle
        if !self.is_working && !self.modal.is_active() && dev_registry.is_continuation_armed() {
            let mut dispatched_wakeup = false;
            if let Some(pool) = crate::agent::subagent::try_get_global_subagent_pool() {
                if pool.has_unreported_completions().await {
                    if let Some((completed_ids, minipub_block)) =
                        pool.harvest_newly_completed_boomerang_block().await
                    {
                        let resume_prompt = format!(
                            "[SYSTEM - ASYNC MINIPUB BOOMERANG COMPLETION WAKEUP ({})]\n\
                             Background `MiniPub` subagent worker(s) completed while the session was idle:\n\n{}\n\n\
                             Synthesize their findings, update your active `todo.md` / `task_plan.md` step if verified, and continue with the next planned step.",
                            completed_ids.join(", "),
                            minipub_block
                        );
                        self.dispatch_agent_prompt(
                            resume_prompt,
                            crate::ui::AgentActivity::Thinking,
                            control_tx,
                        );
                        dispatched_wakeup = true;
                        dirty |= crate::ui::DirtyFlags::TIMELINE_LAYOUT
                            | crate::ui::DirtyFlags::STATUS
                            | crate::ui::DirtyFlags::OVERLAY;
                    }
                }
            }

            if !dispatched_wakeup && dev_registry.has_unreported_completed_tasks().await {
                if let Some((completed_ids, tasks_block)) = dev_registry
                    .harvest_newly_completed_tasks_block(&self.workspace_root)
                    .await
                {
                    let resume_prompt = format!(
                        "[SYSTEM - ASYNC MINITASK COMPLETION WAKEUP ({})]\n\
                         Background command task(s) in `minitask` completed while the session was idle:\n\n{}\n\n\
                         Inspect the exit status and output above, update your active `todo.md` / `task_plan.md` step if verified, and continue with the next planned step.",
                        completed_ids.join(", "),
                        tasks_block
                    );
                    self.dispatch_agent_prompt(
                        resume_prompt,
                        crate::ui::AgentActivity::Thinking,
                        control_tx,
                    );
                    dispatched_wakeup = true;
                    dirty |= crate::ui::DirtyFlags::TIMELINE_LAYOUT
                        | crate::ui::DirtyFlags::STATUS
                        | crate::ui::DirtyFlags::OVERLAY;
                }
            }

            if !dispatched_wakeup {
                let active_workers = crate::agent::subagent::try_get_global_subagent_pool()
                    .map(|p| p.active_worker_count())
                    .unwrap_or(0);
                let active_tasks = dev_registry.active_non_daemon_task_count().await;
                if active_workers == 0 && active_tasks == 0 {
                    dev_registry.set_continuation_armed(false);
                }
            }
        }

        // Version-gated MiniPub subagent pool lifecycle transitions (zero lock/clone overhead when unchanged!)
        if let Some(pool) = crate::agent::subagent::try_get_global_subagent_pool() {
            let ver = pool.state_version();
            if ver != self.last_pool_version {
                self.last_pool_version = ver;
                let snapshots = pool.snapshot_subagents();
                for snap in snapshots {
                    let prev = self.last_minipub_states.get(&snap.id).cloned();
                    if prev.as_ref() != Some(&snap.state) {
                        use crate::agent::subagent::types::SubagentState;
                        let task_preview: String = snap.prompt.chars().take(48).collect();
                        let dedup_key = format!("minipub_{}", snap.id);
                        match &snap.state {
                            SubagentState::Starting
                            | SubagentState::Running
                            | SubagentState::WaitingForInput => {
                                if prev.is_none() {
                                    self.toasts.push(
                                        crate::ui::ToastKind::Subagent,
                                        format!("MiniPub Spawned ({})", snap.role.badge()),
                                        format!("{}: {}", snap.id, task_preview),
                                        Some(snap.isolation_badge().to_string()),
                                        Some(dedup_key),
                                    );
                                }
                            }
                            SubagentState::Completed => {
                                let done_todos = snap.todos.iter().filter(|t| t.completed).count();
                                let badge = if !snap.todos.is_empty() {
                                    format!("{}/{} todos", done_todos, snap.todos.len())
                                } else {
                                    format!("{}s", snap.elapsed_secs())
                                };
                                self.timeline.handle_subagent_completed(
                                    &snap.id,
                                    snap.role.badge(),
                                    true,
                                    snap.final_summary
                                        .as_deref()
                                        .and_then(|s| s.lines().next())
                                        .unwrap_or("Completed"),
                                );
                                self.toasts.push(
                                    crate::ui::ToastKind::Subagent,
                                    format!("MiniPub Completed ({})", snap.role.badge()),
                                    format!("✔ {}: {}", snap.id, task_preview),
                                    Some(badge),
                                    Some(dedup_key),
                                );
                            }
                            SubagentState::Failed(err) => {
                                let err_preview: String = err.chars().take(48).collect();
                                self.timeline.handle_subagent_completed(
                                    &snap.id,
                                    snap.role.badge(),
                                    false,
                                    err,
                                );
                                self.toasts.push(
                                    crate::ui::ToastKind::Error,
                                    format!("MiniPub Failed ({})", snap.role.badge()),
                                    format!("✗ {}: {}", snap.id, err_preview),
                                    Some(format!("{}s", snap.elapsed_secs())),
                                    Some(dedup_key),
                                );
                            }
                            SubagentState::Terminated => {
                                self.timeline.handle_subagent_completed(
                                    &snap.id,
                                    snap.role.badge(),
                                    false,
                                    "Terminated",
                                );
                                self.toasts.push(
                                    crate::ui::ToastKind::Warning,
                                    format!("MiniPub Terminated ({})", snap.role.badge()),
                                    format!("⊘ {}: {}", snap.id, task_preview),
                                    Some("killed".to_string()),
                                    Some(dedup_key),
                                );
                            }
                        }
                        self.last_minipub_states.insert(snap.id, snap.state);
                        dirty |= crate::ui::DirtyFlags::TIMELINE_LAYOUT
                            | crate::ui::DirtyFlags::STATUS
                            | crate::ui::DirtyFlags::OVERLAY;
                    }
                }
            }
        }

        dirty
    }

    /// Dispatches a prompt to the background agent actor with a fresh cancellation token,
    /// or routes it as a non-disruptive live steering prompt if a turn is already active.
    pub(crate) fn dispatch_agent_prompt(
        &mut self,
        prompt: String,
        activity: crate::ui::AgentActivity,
        control_tx: &mpsc::UnboundedSender<AgentCommand>,
    ) {
        let dev_reg = crate::dev::registry::get_global_dev_registry();
        if self.is_working {
            self.handling_live_steering = true;
            self.work_start = Some(Instant::now());
            dev_reg.push_steering_prompt(prompt);
            self.current_activity = Some(crate::ui::AgentActivity::Thinking);
            self.toasts.push(
                crate::ui::ToastKind::Info,
                "Live Prompt Sent",
                "Delivered to active orchestrator without interrupting background tasks."
                    .to_string(),
                Some("live".to_string()),
                Some("live_steering_prompt".to_string()),
            );
            return;
        }
        let bg_workers_active = crate::agent::subagent::try_get_global_subagent_pool()
            .is_some_and(|p| p.active_worker_count() > 0);
        self.handling_live_steering =
            bg_workers_active && !prompt.trim_start().starts_with("[SYSTEM");
        dev_reg.set_continuation_armed(true);
        self.is_working = true;
        self.current_activity = Some(activity);
        self.work_start = Some(Instant::now());
        let cancel = tokio_util::sync::CancellationToken::new();
        self.cancel_token = Some(cancel.clone());
        if let Err(e) = control_tx.send(AgentCommand::Prompt(prompt, Some(cancel))) {
            tracing::error!(error = %e, "Failed to dispatch prompt to agent actor");
        }
    }

    /// Pushes the currently active browsing modal onto `modal_history` and transitions to `next`.
    pub(crate) fn push_modal(&mut self, next: ModalState) {
        if self.modal.supports_nav_chrome() {
            let prev = std::mem::replace(&mut self.modal, next);
            self.modal_history.push(prev);
        } else {
            self.modal = next;
        }
    }

    /// Pops back to the immediate parent modal in `modal_history` (syncing live config/UI badges),
    /// or closes the modal overlay when at the root level.
    pub(crate) fn pop_modal(&mut self) {
        if let Some(mut prev) = self.modal_history.pop() {
            match &mut prev {
                ModalState::Settings(ref mut s) => {
                    s.sync_from_config(&self.config);
                }
                ModalState::StreamingSelect {
                    ref mut current_streaming,
                    ..
                } => {
                    *current_streaming = self.config.agent.streaming;
                }
                ModalState::ThemeSelect {
                    ref mut active_theme_id,
                    ref mut active_animation_id,
                    ref mut active_todo_style,
                    ref mut active_swarm_style,
                    ref mut active_toast_style,
                    ref mut active_toast_symbols,
                    ref mut active_activity_style,
                    ref mut active_menu_style,
                    ref mut ui_prefs,
                    ..
                } => {
                    *active_theme_id = self.config.ui.theme.clone();
                    *active_animation_id = self.config.ui.animation.clone();
                    *active_todo_style = self.config.ui.todo_style.clone();
                    *active_swarm_style = self.config.ui.swarm_style.clone();
                    *active_toast_style = self.config.ui.toast_style.clone();
                    *active_toast_symbols = self.config.ui.toast_symbols.clone();
                    *active_activity_style = self.config.ui.activity_style.clone();
                    *active_menu_style = self.config.ui.menu_style.clone();
                    *ui_prefs = self.config.ui.clone();
                }
                _ => {}
            }
            self.modal = prev;
        } else {
            self.modal = ModalState::None;
        }
    }

    /// Closes the active modal and clears the entire parent modal stack.
    pub(crate) fn close_all_modals(&mut self) {
        self.modal_history.clear();
        self.modal = ModalState::None;
    }

    /// Computes navigation chrome metadata (`[← Back: <Parent>]`, `[× Close]`, active provider/model)
    /// for the currently rendered modal.
    pub(crate) fn current_modal_nav_info(&self) -> crate::ui::modals::ModalNavInfo {
        let (parent_label, can_go_back) = match &self.modal {
            ModalState::Settings(s) if s.selecting_model_for_provider.is_some() => {
                (Some("Providers".to_string()), true)
            }
            ModalState::Settings(s) if s.selecting_provider_for_target => {
                (Some(s.active_tab.sidebar_label().to_string()), true)
            }
            ModalState::Processes(s)
                if matches!(
                    s.active_tab,
                    crate::ui::modals::processes::ProcessesTab::Logs
                        | crate::ui::modals::processes::ProcessesTab::Telemetry
                ) =>
            {
                (Some("Tasks".to_string()), true)
            }
            _ => match self.modal_history.last() {
                Some(parent) => (Some(parent.nav_label().to_string()), true),
                None => (None, false),
            },
        };

        crate::ui::modals::ModalNavInfo {
            parent_label,
            can_go_back,
            active_provider: Some(self.config.provider.default.clone()),
            active_model: Some(self.config.provider.model.clone()),
        }
    }

    /// Opens the Theme & Style selector modal, optionally pre-selecting a specific tab.
    pub(crate) fn open_theme_modal(&mut self, tab: Option<crate::ui::modals::ThemeModalTab>) {
        let mut m = ModalState::new_theme_select_from_ui(&self.config.ui);
        if let ModalState::ThemeSelect {
            ref mut active_tab, ..
        } = &mut m
        {
            if let Some(target_tab) = tab {
                *active_tab = target_tab;
            }
        }
        self.modal = m;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::types::AgentEvent;
    use crate::config::Config;
    use std::path::Path;

    #[test]
    fn test_hydrate_session_token_metrics() {
        let config = Config::default();
        let mut app = App::new(Path::new("."), config);
        assert_eq!(app.last_turn_tokens, 0);

        let events = vec![
            AgentEvent::UserPrompt {
                turn_id: 1,
                timestamp: "2026-09-09T12:00:00Z".to_string(),
                prompt: "Hello assistant".to_string(),
            },
            AgentEvent::TurnStart {
                turn_id: 1,
                timestamp: "2026-09-09T12:00:01Z".to_string(),
                model: "MiniMax-M2.7".to_string(),
                context_tokens: 1420,
            },
            AgentEvent::TurnEnd {
                turn_id: 1,
                status: "complete".to_string(),
                total_tokens_used: 1580,
                files_modified: vec![],
                cached_prompt_tokens: 0,
            },
        ];

        app.hydrate_session(&events);
        assert_eq!(app.last_turn_tokens, 1580);

        // minitest81 regression: when ContextTokensUpdated reports true active context occupancy
        // before TurnEnd, TurnEnd's cumulative turn spend must update `cumulative_tokens` without
        // overwriting `last_turn_tokens` (preserving accurate context bar & `kv (%)` cache hit rate).
        let multi_step_events = vec![
            AgentEvent::TurnStart {
                turn_id: 2,
                timestamp: "2026-11-21T17:13:54Z".to_string(),
                model: "MiniMax-M3.1-Flash".to_string(),
                context_tokens: 12153,
            },
            AgentEvent::ContextTokensUpdated {
                turn_id: 2,
                used_tokens: 175_000,
                cached_tokens: 157_184,
            },
            AgentEvent::TurnEnd {
                turn_id: 2,
                status: "complete".to_string(),
                total_tokens_used: 385_375,
                files_modified: vec![],
                cached_prompt_tokens: 157_184,
            },
        ];
        app.hydrate_session(&multi_step_events);
        assert_eq!(app.last_turn_tokens, 175_000);
        assert_eq!(app.last_turn_cached_tokens, 157_184);
        assert_eq!(app.cumulative_tokens, 1580 + 385_375);
    }

    #[test]
    fn test_silent_startup_unindexed_repo() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let config = Config::default();
        let app = App::new(temp_dir.path(), config);
        // Silent startup: unindexed repo does NOT show WorkspaceAnalysis modal
        assert!(matches!(app.modal, ModalState::None));
        assert!(app.pending_submission.is_none());
    }

    #[test]
    fn test_pending_submission_lifecycle() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let config = Config::default();
        let mut app = App::new(temp_dir.path(), config);

        let sub = PendingSubmission {
            prompt: "add login to auth.rs".to_string(),
            display: "add login to auth.rs".to_string(),
        };
        app.pending_submission = Some(sub);
        assert!(app.pending_submission.is_some());

        let popped = app.pending_submission.take().unwrap();
        assert_eq!(popped.prompt, "add login to auth.rs");
        assert!(app.pending_submission.is_none());
    }

    #[tokio::test]
    async fn test_commands_to_theme_enter_stays_and_back_returns_to_commands() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        let temp_dir = tempfile::tempdir().expect("tempdir");
        let config = Config::default();
        let mut app = App::new(temp_dir.path(), config);
        let (control_tx, _control_rx) = mpsc::unbounded_channel::<AgentCommand>();

        // 1. Open /commands catalog
        app.handle_command_or_prompt("/commands", None, &control_tx)
            .await
            .unwrap();
        assert!(matches!(app.modal, ModalState::CommandCatalog { .. }));

        // 2. Filter for "theme" and press Enter -> opens ThemeSelect while pushing CommandCatalog
        for ch in "theme".chars() {
            app.handle_modal_key(
                KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE),
                &control_tx,
            )
            .await;
        }
        app.handle_modal_key(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            &control_tx,
        )
        .await;

        assert!(
            matches!(app.modal, ModalState::ThemeSelect { .. }),
            "Selecting /theme from /commands must open ThemeSelect"
        );
        assert_eq!(app.modal_history.len(), 1);
        let nav = app.current_modal_nav_info();
        assert!(nav.can_go_back);
        assert_eq!(nav.parent_label.as_deref(), Some("Commands"));

        // 3. Render with nav chrome -> verify clickable [← Back: Commands] & [× Close] buttons
        let backend = TestBackend::new(110, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut regions = crate::ui::modals::ModalNavRegions::default();
        terminal
            .draw(|f| {
                regions = app.modal.render_with_nav(f, f.area(), &app.theme, &nav);
            })
            .unwrap();
        assert!(regions.back_button.is_some());
        assert!(regions.close_button.is_some());
        let buf = terminal.backend().buffer();
        let top_row: String = (0..110).map(|x| buf[(x, 0)].symbol()).collect();
        assert!(top_row.contains("[← Back: Commands]"), "top_row: {top_row}");
        assert!(top_row.contains("[× Close]"), "top_row: {top_row}");

        // 4. Move to next theme and press Enter -> applies theme AND STAYS inside ThemeSelect!
        let initial_theme = app.config.ui.theme.clone();
        app.handle_modal_key(
            KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
            &control_tx,
        )
        .await;
        app.handle_modal_key(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            &control_tx,
        )
        .await;
        assert!(
            matches!(app.modal, ModalState::ThemeSelect { .. }),
            "Pressing Enter in ThemeSelect must stay in ThemeSelect instead of quitting"
        );
        assert_ne!(app.config.ui.theme, initial_theme);

        // 5. Press Backspace -> returns to CommandCatalog with "theme" filter preserved!
        app.handle_modal_key(
            KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE),
            &control_tx,
        )
        .await;
        match &app.modal {
            ModalState::CommandCatalog { filter, .. } => {
                assert_eq!(filter, "theme");
            }
            other => panic!("Expected CommandCatalog on Back, got {:?}", other),
        }
        assert!(app.modal_history.is_empty());

        // 6. Re-open /theme and simulate clicking [← Back] via handle_modal_back -> back to /commands -> back to chat
        app.handle_modal_key(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            &control_tx,
        )
        .await;
        assert!(matches!(app.modal, ModalState::ThemeSelect { .. }));
        app.handle_modal_back(&control_tx).await;
        assert!(matches!(app.modal, ModalState::CommandCatalog { .. }));
        app.handle_modal_back(&control_tx).await;
        assert!(matches!(app.modal, ModalState::None));
    }

    #[tokio::test]
    async fn test_model_and_streaming_select_stay_on_enter_and_back_pops() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

        let temp_dir = tempfile::tempdir().expect("tempdir");
        let config = Config::default();
        let mut app = App::new(temp_dir.path(), config);
        let (control_tx, _control_rx) = mpsc::unbounded_channel::<AgentCommand>();

        // 1. ModelSelect stays open on Enter (when no pending submission) and returns to ProviderSelect on Esc
        app.modal = ModalState::new_provider_select_with_config(&app.config);
        app.push_modal(ModalState::new_model_select(
            "ollama".to_string(),
            vec![
                crate::agent::models::ModelInfo {
                    id: "llama3.2".to_string(),
                    name: "Llama 3.2".to_string(),
                    description: None,
                    context_length: Some(128_000),
                    is_free: false,
                },
                crate::agent::models::ModelInfo {
                    id: "qwen2.5-coder".to_string(),
                    name: "Qwen 2.5 Coder".to_string(),
                    description: None,
                    context_length: Some(128_000),
                    is_free: false,
                },
            ],
        ));
        app.handle_modal_key(
            KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
            &control_tx,
        )
        .await;
        app.handle_modal_key(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            &control_tx,
        )
        .await;
        assert_eq!(app.config.provider.default, "ollama");
        assert_eq!(app.config.provider.model, "qwen2.5-coder");
        assert!(
            matches!(app.modal, ModalState::ModelSelect { .. }),
            "ModelSelect must stay open after selecting a model"
        );
        app.handle_modal_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE), &control_tx)
            .await;
        assert!(matches!(app.modal, ModalState::ProviderSelect { .. }));

        // 2. StreamingSelect stays open on Enter/1/2 and pops back on '3' / Esc
        app.push_modal(ModalState::new_streaming_select(true));
        app.handle_modal_key(
            KeyEvent::new(KeyCode::Char('2'), KeyModifiers::NONE),
            &control_tx,
        )
        .await;
        assert!(!app.config.agent.streaming);
        match app.modal {
            ModalState::StreamingSelect {
                current_streaming, ..
            } => assert!(!current_streaming),
            ref other => panic!("Expected StreamingSelect to stay open, got {:?}", other),
        }
        app.handle_modal_key(
            KeyEvent::new(KeyCode::Char('3'), KeyModifiers::NONE),
            &control_tx,
        )
        .await;
        assert!(matches!(app.modal, ModalState::ProviderSelect { .. }));
    }

    #[tokio::test]
    async fn test_settings_drilldown_apikey_back_and_close_all_saves() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

        let temp_dir = tempfile::tempdir().expect("tempdir");
        let config = Config::default();
        let mut app = App::new(temp_dir.path(), config);
        let (control_tx, _control_rx) = mpsc::unbounded_channel::<AgentCommand>();

        // Open Settings from CommandCatalog
        app.modal = ModalState::new_command_catalog();
        app.push_modal(ModalState::new_settings(&app.config, temp_dir.path()));

        // Enter synthetic model drilldown on Providers tab and press Enter -> stays in drilldown!
        if let ModalState::Settings(ref mut s) = app.modal {
            s.selecting_model_for_provider = Some("ollama".to_string());
            s.provider_models = vec![crate::agent::models::ModelInfo {
                id: "deepseek-r1".to_string(),
                name: "DeepSeek R1".to_string(),
                description: None,
                context_length: Some(64_000),
                is_free: false,
            }];
            s.model_selected_index = 0;
        }
        assert_eq!(
            app.current_modal_nav_info().parent_label.as_deref(),
            Some("Providers")
        );
        app.handle_modal_key(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            &control_tx,
        )
        .await;
        if let ModalState::Settings(ref s) = app.modal {
            assert!(
                s.selecting_model_for_provider.is_some(),
                "Providers tab model drilldown must stay open on Enter"
            );
        } else {
            panic!("Expected Settings modal");
        }

        // Back steps out of model drilldown -> still in Settings
        app.handle_modal_back(&control_tx).await;
        if let ModalState::Settings(ref s) = app.modal {
            assert!(s.selecting_model_for_provider.is_none());
        } else {
            panic!("Expected Settings modal");
        }

        // Opening ApiKeyInput from Settings and pressing Esc returns to Settings (not ProviderSelect!)
        app.push_modal(ModalState::new_api_key_input(
            "anthropic".to_string(),
            "ANTHROPIC_API_KEY".to_string(),
            None,
        ));
        app.handle_modal_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE), &control_tx)
            .await;
        assert!(
            matches!(app.modal, ModalState::Settings(_)),
            "Esc from ApiKeyInput opened via Settings must return to Settings"
        );

        // Toggle streaming in Settings and click [× Close] -> persists setting & clears whole stack
        let prev_streaming = app.config.agent.streaming;
        if let ModalState::Settings(ref mut s) = app.modal {
            s.streaming = !prev_streaming;
        }
        app.handle_modal_close_all(&control_tx).await;
        assert_eq!(app.config.agent.streaming, !prev_streaming);
        assert!(matches!(app.modal, ModalState::None));
        assert!(app.modal_history.is_empty());
    }

    #[test]
    fn test_theme_selector_animation_tabs_keep_frame_scheduler_animating() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let config = Config::default();
        let mut app = App::new(temp_dir.path(), config);
        let mut scheduler = crate::ui::FrameScheduler::new();
        let now = Instant::now();
        scheduler.record_draw_completed(now, Duration::from_millis(2));

        // Idle with no modal -> 0% CPU (no animation deadline)
        assert!(!app.modal.has_live_animation());
        assert!(scheduler
            .next_deadline(now, app.modal.has_live_animation(), None, None)
            .is_none());

        // Static Themes tab -> still 0% CPU
        app.open_theme_modal(Some(crate::ui::modals::ThemeModalTab::Themes));
        assert!(!app.modal.has_live_animation());

        // Animations tab -> actively ticks FrameScheduler at spinner cadence
        app.open_theme_modal(Some(crate::ui::modals::ThemeModalTab::Animations));
        assert!(app.modal.has_live_animation());
        let deadline = scheduler
            .next_deadline(now, app.modal.has_live_animation(), None, None)
            .expect("Animations tab must schedule live animation ticks");
        assert!(deadline.saturating_duration_since(now) <= Duration::from_millis(125));

        // Activity Bar tab -> actively ticks FrameScheduler too
        app.open_theme_modal(Some(crate::ui::modals::ThemeModalTab::ActivityStyles));
        assert!(app.modal.has_live_animation());

        // Timer & Hint and Visibility tabs also animate live previews
        app.open_theme_modal(Some(crate::ui::modals::ThemeModalTab::ActivityHintStyles));
        assert!(app.modal.has_live_animation());
        app.open_theme_modal(Some(crate::ui::modals::ThemeModalTab::Visibility));
        assert!(app.modal.has_live_animation());
    }

    #[tokio::test]
    async fn test_theme_modal_new_style_tabs_visibility_toggles_and_persistence() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

        let temp_dir = tempfile::tempdir().expect("tempdir");
        let config = Config::default();
        let mut app = App::new(temp_dir.path(), config);
        let (control_tx, _control_rx) = mpsc::unbounded_channel::<AgentCommand>();

        // 1. Open Input Box Styles tab -> choose LeftBar -> stays open & updates InputDock live
        app.open_theme_modal(Some(crate::ui::modals::ThemeModalTab::InputBoxStyles));
        app.handle_modal_key(
            KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
            &control_tx,
        )
        .await;
        app.handle_modal_key(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            &control_tx,
        )
        .await;
        assert!(matches!(app.modal, ModalState::ThemeSelect { .. }));
        assert_eq!(app.config.ui.input_box_style, "left_bar");
        assert_eq!(
            app.input_dock.box_style,
            crate::config::InputBoxStyle::LeftBar
        );

        // 2. Context & KV tab -> select Circle style
        app.open_theme_modal(Some(crate::ui::modals::ThemeModalTab::ContextMeterStyles));
        app.handle_modal_key(
            KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
            &control_tx,
        )
        .await;
        app.handle_modal_key(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            &control_tx,
        )
        .await;
        assert_eq!(app.config.ui.context_meter_style, "circle");

        // 3. Visibility tab -> toggle Thought Token Usage (index 0: false -> true)
        //    and KV Cache Hit Rate (index 1: true -> false)
        app.open_theme_modal(Some(crate::ui::modals::ThemeModalTab::Visibility));
        assert!(!app.config.ui.show_thought_tokens);
        assert!(app.config.ui.show_kv_cache);
        app.handle_modal_key(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            &control_tx,
        )
        .await;
        assert!(app.config.ui.show_thought_tokens);
        app.handle_modal_key(
            KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
            &control_tx,
        )
        .await;
        app.handle_modal_key(
            KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE),
            &control_tx,
        )
        .await;
        assert!(!app.config.ui.show_kv_cache);
        assert!(matches!(app.modal, ModalState::ThemeSelect { .. }));

        // 4. Reload Config from workspace disk and verify every setting round-tripped cleanly!
        let reloaded = Config::load(Some(temp_dir.path()), None).expect("reload config");
        assert_eq!(reloaded.ui.input_box_style, "left_bar");
        assert_eq!(reloaded.ui.context_meter_style, "circle");
        assert!(reloaded.ui.show_thought_tokens);
        assert!(!reloaded.ui.show_kv_cache);

        // 5. Status Bar per-component override via 'c' key inside 10. Status Bar
        app.open_theme_modal(Some(crate::ui::modals::ThemeModalTab::StatusBarStyles));
        // Cycle target All -> Model ('c'), move down from Inherit(0) to Classic(1) -> PoshPure(2), press Enter
        app.handle_modal_key(
            KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE),
            &control_tx,
        )
        .await;
        app.handle_modal_key(
            KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
            &control_tx,
        )
        .await;
        app.handle_modal_key(
            KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
            &control_tx,
        )
        .await;
        app.handle_modal_key(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            &control_tx,
        )
        .await;
        assert_eq!(app.config.ui.status_model_style, "posh_pure");
        assert_eq!(app.config.ui.status_git_style, "inherit");

        let reloaded2 = Config::load(Some(temp_dir.path()), None).expect("reload config 2");
        assert_eq!(reloaded2.ui.status_model_style, "posh_pure");
        assert_eq!(reloaded2.ui.status_git_style, "inherit");
    }
}

//! In-TUI settings management modal and autonomous configuration permission cards.

use crate::config::Config;
use crate::constants::SUPPORTED_PROVIDERS;
use crate::tools::registry::agent_tools::config_tools::{
    model_supports_reasoning, ConfigChangeProposal, ConnectionTestResult,
};
use crate::ui::layout_utils::{centered_rect_exact, compute_scroll_offset};
use crate::ui::theme::Theme;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;
use std::path::Path;

/// Truncates a string in the middle with an ellipsis if it exceeds `max_len`.
pub fn truncate_middle(s: &str, max_len: usize) -> String {
    let char_count = s.chars().count();
    if char_count <= max_len || max_len < 8 {
        return s.to_string();
    }
    let keep = max_len.saturating_sub(3);
    let prefix_count = keep / 2;
    let suffix_count = keep - prefix_count;
    let prefix: String = s.chars().take(prefix_count).collect();
    let suffix: String = s
        .chars()
        .skip(char_count.saturating_sub(suffix_count))
        .collect();
    format!("{}...{}", prefix, suffix)
}

/// Formats token counts into a compact human-readable string (e.g. 128k, 2M).
pub fn format_context_tokens(tokens: Option<usize>) -> String {
    match tokens {
        Some(t) if t >= 1_000_000 => format!("{}M", t / 1_000_000),
        Some(t) if t >= 1_000 => format!("{}k", t / 1_000),
        Some(t) => format!("{}", t),
        None => "-".to_string(),
    }
}

/// Active tab within the `/settings` modal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    Providers,
    Workspace,
    Autonomy,
    Probes,
    Browser,
}

impl SettingsTab {
    /// Returns all tabs in order.
    pub fn all() -> &'static [SettingsTab] {
        &[
            SettingsTab::Providers,
            SettingsTab::Workspace,
            SettingsTab::Autonomy,
            SettingsTab::Probes,
            SettingsTab::Browser,
        ]
    }

    /// Tab display title.
    pub fn title(&self) -> &'static str {
        match self {
            SettingsTab::Providers => "1. Providers",
            SettingsTab::Workspace => "2. Workspace",
            SettingsTab::Autonomy => "3. Autonomy",
            SettingsTab::Probes => "4. Probes",
            SettingsTab::Browser => "5. Browser",
        }
    }

    /// Sidebar menu label with icon.
    pub fn sidebar_label(&self) -> &'static str {
        match self {
            SettingsTab::Providers => "🤖 1. Models & Routing",
            SettingsTab::Workspace => "📂 2. Workspace",
            SettingsTab::Autonomy => "🛡️ 3. Autonomy & Agents",
            SettingsTab::Probes => "📡 4. Endpoint Probes",
            SettingsTab::Browser => "🌐 5. Browser Engine",
        }
    }

    /// Right-pane section title and subtitle.
    pub fn section_header(&self) -> (&'static str, &'static str) {
        match self {
            SettingsTab::Providers => (
                "🤖 Models, 4-Slot Failover Chain & MiniPub Routing",
                "Configure 1 Primary + 2 Fallbacks + 1 Shared Global Last Fallback ★ across Orchestrator & 14 MiniPubs",
            ),
            SettingsTab::Workspace => (
                "📂 Workspace & Persistence Scope",
                "Manage active project root, assigned model, and config persistence (.minicode vs ~/.config)",
            ),
            SettingsTab::Autonomy => (
                "🛡️ Autonomy, Thinking Budget & MiniPub Concurrency",
                "Configure tool approval policy, extended reasoning tokens, worker limits, and stream timeouts",
            ),
            SettingsTab::Probes => (
                "📡 Live Provider Connectivity & Latency Diagnostics",
                "Verify API credentials, endpoint reachability, and round-trip latency across all providers",
            ),
            SettingsTab::Browser => (
                "🌐 Browser Automation Engine & Priority Chains",
                "Configure Obscura (pure-Rust headless V8), Firefox, and Chrome engine priorities",
            ),
        }
    }

    /// Cycles to the next tab.
    pub fn next(&self) -> Self {
        match self {
            SettingsTab::Providers => SettingsTab::Workspace,
            SettingsTab::Workspace => SettingsTab::Autonomy,
            SettingsTab::Autonomy => SettingsTab::Probes,
            SettingsTab::Probes => SettingsTab::Browser,
            SettingsTab::Browser => SettingsTab::Providers,
        }
    }

    /// Cycles to the previous tab.
    pub fn prev(&self) -> Self {
        match self {
            SettingsTab::Providers => SettingsTab::Browser,
            SettingsTab::Workspace => SettingsTab::Providers,
            SettingsTab::Autonomy => SettingsTab::Workspace,
            SettingsTab::Probes => SettingsTab::Autonomy,
            SettingsTab::Browser => SettingsTab::Probes,
        }
    }
}

/// Available target scopes for 4-slot model & fallback routing in `/settings`:
/// - `"global"` configures the Orchestrator + default MiniPub chain in one place.
/// - Individual MiniPub role slugs configure custom `(provider, model)` slots 1-3 for that specialist while sharing Slot 4 (`Global Last Fallback ★`).
pub const ROUTING_TARGETS: &[(&str, &str)] = &[
    ("global", "Global (Orchestrator + All MiniPubs)"),
    ("scout", "MiniPub: Scout (Codebase Explorer)"),
    ("architect", "MiniPub: Architect (System Design)"),
    ("planner", "MiniPub: Planner (Task Strategy)"),
    ("designer", "MiniPub: Designer (UI/UX Specialist)"),
    ("frontend", "MiniPub: Frontend (Web/UI Engineer)"),
    ("backend", "MiniPub: Backend (API/DB/Systems)"),
    ("coder", "MiniPub: Coder (Implementation)"),
    ("vision", "MiniPub: Vision (Multimodal Image/UI)"),
    ("content_writer", "MiniPub: Content Writer (Copy/Docs)"),
    ("debugger", "MiniPub: Debugger (Root-Cause Analysis)"),
    ("reviewer", "MiniPub: Reviewer (Quality & Correctness)"),
    ("security", "MiniPub: Security (Vulnerability Audit)"),
    ("tester", "MiniPub: Tester (QA & Verification)"),
    ("simplifier", "MiniPub: Simplifier (Clean Refactoring)"),
];

/// State tracking an interactive in-TUI `/settings` modal dialog.
#[derive(Debug, Clone)]
pub struct SettingsModalState {
    pub active_tab: SettingsTab,
    pub selected_index: usize,
    pub active_provider: String,
    pub active_model: String,
    pub workspace_path: String,
    pub save_to_workspace: bool,
    pub auto_approve: bool,
    pub thinking_budget: usize,
    pub approval_policy: String,
    pub minipub_max_parallel: usize,
    pub minipub_inactivity_timeout_secs: u64,
    pub minipub_worktree_isolation: String,
    pub stream_idle_timeout_secs: u64,
    pub probe_results: Option<Vec<ConnectionTestResult>>,
    pub probing: bool,

    // Browser settings for Browser tab:
    pub browser_default_engine: Option<String>,
    pub browser_headless_priority: Vec<String>,
    pub browser_gui_priority: Vec<String>,

    // Model selection drill-down sub-state for Providers tab:
    pub selecting_model_for_provider: Option<String>,
    pub provider_models: Vec<crate::agent::models::ModelInfo>,
    pub model_selected_index: usize,

    // 4-Slot Model & Fallback Routing state (1 Primary + 2 Fallbacks + 1 Shared Global Last Fallback):
    pub routing_target_idx: usize,
    pub fallback_1: Option<crate::config::ModelSlot>,
    pub fallback_2: Option<crate::config::ModelSlot>,
    pub global_last_fallback: Option<crate::config::ModelSlot>,
    pub minipub_sync_with_orchestrator: bool,
    pub minipub_default_chain: crate::config::MiniPubModelChain,
    pub minipub_role_models: std::collections::HashMap<String, crate::config::MiniPubModelChain>,
}

impl SettingsModalState {
    /// Constructs a new `SettingsModalState` from current runtime configuration and workspace root.
    pub fn from_config(config: &Config, workspace_root: &Path) -> Self {
        let workspace_path = if let Ok(home) = std::env::var("HOME") {
            let p_str = workspace_root.display().to_string();
            if let Some(rest) = p_str.strip_prefix(&home) {
                format!("~{}", rest)
            } else {
                p_str
            }
        } else {
            workspace_root.display().to_string()
        };

        let has_local_config = workspace_root
            .join(crate::constants::WORKSPACE_DIR_NAME)
            .join(crate::constants::CONFIG_FILE_NAME)
            .exists();
        let has_global_config = dirs::config_dir()
            .map(|d| {
                d.join(crate::constants::CONFIG_DIR_NAME)
                    .join(crate::constants::CONFIG_FILE_NAME)
                    .exists()
            })
            .unwrap_or(false);
        let save_to_workspace = has_local_config || !has_global_config;

        Self {
            active_tab: SettingsTab::Providers,
            selected_index: 0,
            active_provider: config.provider.default.clone(),
            active_model: config.provider.model.clone(),
            workspace_path,
            save_to_workspace,
            auto_approve: config.agent.auto_approve,
            thinking_budget: config.provider.thinking_budget.unwrap_or(0),
            approval_policy: config.agent.approval_policy.clone(),
            minipub_max_parallel: config.agent.minipub.max_parallel,
            minipub_inactivity_timeout_secs: config.agent.minipub.inactivity_timeout_secs,
            minipub_worktree_isolation: config.agent.minipub.worktree_isolation.clone(),
            stream_idle_timeout_secs: config.provider.effective_stream_idle_timeout_secs(),
            probe_results: None,
            probing: false,
            browser_default_engine: config.browser.default_engine.clone(),
            browser_headless_priority: config.browser.headless_priority.clone(),
            browser_gui_priority: config.browser.gui_priority.clone(),
            selecting_model_for_provider: None,
            provider_models: Vec::new(),
            model_selected_index: 0,
            routing_target_idx: 0,
            fallback_1: config.provider.fallback_1.clone(),
            fallback_2: config.provider.fallback_2.clone(),
            global_last_fallback: config.provider.global_last_fallback.clone(),
            minipub_sync_with_orchestrator: config.agent.minipub.sync_with_orchestrator,
            minipub_default_chain: config.agent.minipub.default_chain.clone(),
            minipub_role_models: config.agent.minipub.role_models.clone(),
        }
    }

    /// Synchronizes 4-slot routing fields from a mutated `Config`.
    pub fn sync_from_config(&mut self, config: &Config) {
        self.active_provider = config.provider.default.clone();
        self.active_model = config.provider.model.clone();
        self.fallback_1 = config.provider.fallback_1.clone();
        self.fallback_2 = config.provider.fallback_2.clone();
        self.global_last_fallback = config.provider.global_last_fallback.clone();
        self.minipub_sync_with_orchestrator = config.agent.minipub.sync_with_orchestrator;
        self.minipub_default_chain = config.agent.minipub.default_chain.clone();
        self.minipub_role_models = config.agent.minipub.role_models.clone();
    }

    /// Returns `(slug, display_label)` for the currently selected routing target scope.
    pub fn current_routing_target(&self) -> (&'static str, &'static str) {
        ROUTING_TARGETS
            .get(self.routing_target_idx)
            .copied()
            .unwrap_or(ROUTING_TARGETS[0])
    }

    /// Cycles forward through `Global` and the 14 `MiniPub` specialist targets.
    pub fn next_routing_target(&mut self) {
        if !ROUTING_TARGETS.is_empty() {
            self.routing_target_idx = (self.routing_target_idx + 1) % ROUTING_TARGETS.len();
        }
    }

    /// Cycles backward through `Global` and the 14 `MiniPub` specialist targets.
    pub fn prev_routing_target(&mut self) {
        if !ROUTING_TARGETS.is_empty() {
            self.routing_target_idx = if self.routing_target_idx == 0 {
                ROUTING_TARGETS.len() - 1
            } else {
                self.routing_target_idx - 1
            };
        }
    }

    /// Resolves the effective `[Primary, 1st Fallback, 2nd Fallback, Global Last Fallback]` slots
    /// for the currently active target in `SettingsModalState`, plus a boolean indicating whether
    /// the target has custom per-MiniPub overrides (`true`) or inherits from Global (`false`).
    pub fn effective_slots_for_current_target(
        &self,
    ) -> ([Option<crate::config::ModelSlot>; 4], bool) {
        let (target_slug, _) = self.current_routing_target();
        let orch_primary = if !self.active_provider.trim().is_empty() {
            let model = if !self.active_model.trim().is_empty() {
                self.active_model.clone()
            } else {
                Config::static_default_model_for_provider(&self.active_provider).to_string()
            };
            Some(crate::config::ModelSlot::new(
                &self.active_provider,
                model,
            ))
        } else {
            None
        };
        let orch_fb1 = self.fallback_1.clone().filter(|s| s.is_valid());
        let orch_fb2 = self.fallback_2.clone().filter(|s| s.is_valid());
        let global_last = self.global_last_fallback.clone().filter(|s| s.is_valid());

        if target_slug == "global" {
            return ([orch_primary, orch_fb1, orch_fb2, global_last], false);
        }

        if let Some(rc) = self.minipub_role_models.get(target_slug) {
            if !rc.is_empty() {
                return (
                    [
                        rc.primary
                            .clone()
                            .filter(|s| s.is_valid())
                            .or(orch_primary),
                        rc.fallback_1.clone().filter(|s| s.is_valid()).or(orch_fb1),
                        rc.fallback_2.clone().filter(|s| s.is_valid()).or(orch_fb2),
                        global_last,
                    ],
                    true,
                );
            }
        }

        if !self.minipub_sync_with_orchestrator && !self.minipub_default_chain.is_empty() {
            return (
                [
                    self.minipub_default_chain
                        .primary
                        .clone()
                        .filter(|s| s.is_valid())
                        .or(orch_primary),
                    self.minipub_default_chain
                        .fallback_1
                        .clone()
                        .filter(|s| s.is_valid())
                        .or(orch_fb1),
                    self.minipub_default_chain
                        .fallback_2
                        .clone()
                        .filter(|s| s.is_valid())
                        .or(orch_fb2),
                    global_last,
                ],
                true,
            );
        }

        ([orch_primary, orch_fb1, orch_fb2, global_last], false)
    }

    /// Switches to next tab and resets selection index and model drilldown.
    pub fn next_tab(&mut self) {
        self.selecting_model_for_provider = None;
        self.provider_models.clear();
        self.model_selected_index = 0;
        self.active_tab = self.active_tab.next();
        self.selected_index = 0;
    }

    /// Switches to previous tab and resets selection index and model drilldown.
    pub fn prev_tab(&mut self) {
        self.selecting_model_for_provider = None;
        self.provider_models.clear();
        self.model_selected_index = 0;
        self.active_tab = self.active_tab.prev();
        self.selected_index = 0;
    }

    /// Returns the maximum selectable items for the active tab (or model list).
    pub fn max_items(&self) -> usize {
        if self.selecting_model_for_provider.is_some() {
            return self.provider_models.len();
        }
        match self.active_tab {
            SettingsTab::Providers => SUPPORTED_PROVIDERS.len(),
            SettingsTab::Workspace => 3,
            SettingsTab::Autonomy => 7,
            SettingsTab::Probes => 1,
            SettingsTab::Browser => 4,
        }
    }

    /// Moves cursor down within the active tab or model drilldown.
    pub fn next_item(&mut self) {
        if self.selecting_model_for_provider.is_some() {
            if !self.provider_models.is_empty()
                && self.model_selected_index + 1 < self.provider_models.len()
            {
                self.model_selected_index += 1;
            }
            return;
        }
        let max = self.max_items();
        if max > 0 && self.selected_index + 1 < max {
            self.selected_index += 1;
        }
    }

    /// Moves cursor up within the active tab or model drilldown.
    pub fn prev_item(&mut self) {
        if self.selecting_model_for_provider.is_some() {
            self.model_selected_index = self.model_selected_index.saturating_sub(1);
            return;
        }
        self.selected_index = self.selected_index.saturating_sub(1);
    }
}

/// Renders the `/settings` configuration dialog as a Full-Screen Workbench with a Left Menu Bar
/// for selecting tabs and a Right Main Panel showing the full settings for the active tab.
pub fn render_settings(frame: &mut Frame, area: Rect, theme: &Theme, state: &SettingsModalState) {
    let popup_area = area;
    frame.render_widget(Clear, popup_area);

    let scope_badge = if state.save_to_workspace {
        "Workspace (.minicode)"
    } else {
        "Global (~/.config)"
    };

    let block = Block::default()
        .title(Line::from(vec![
            Span::styled(
                " ⚙️ MiniCode Settings ",
                Style::default()
                    .fg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(
                    "── {} · {} ── [{}] ",
                    state.active_provider,
                    truncate_middle(&state.active_model, 28),
                    scope_badge
                ),
                Style::default().fg(theme.muted),
            ),
        ]))
        .title_alignment(Alignment::Left)
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(
            Style::default()
                .fg(theme.brand_accent)
                .bg(theme.bg_elevated),
        )
        .style(Style::default().bg(theme.bg_elevated));

    let inner_area = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let outer_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(8),    // 0: Main Split Body (Left Menu Bar | Right Settings Pane)
            Constraint::Length(1), // 1: Bottom Divider
            Constraint::Length(1), // 2: Footer Key Hints
        ])
        .split(inner_area);

    let body_area = outer_chunks[0];
    let sidebar_w = if body_area.width >= 90 {
        28
    } else if body_area.width >= 68 {
        24
    } else {
        18.min(body_area.width / 3)
    };

    let main_split = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(sidebar_w), // 0: Left Menu Bar (Tab Selector + Live Summary)
            Constraint::Length(1),         // 1: Vertical Separator │
            Constraint::Min(20),           // 2: Right Full Settings Panel
        ])
        .split(body_area);

    let sidebar_area = main_split[0];
    let sep_area = main_split[1];
    let right_area = main_split[2];

    // 1. Render Left Menu Bar (Vertical Tab Navigation + Active 4-Slot Chain Summary)
    let mut sidebar_lines: Vec<Line> = Vec::new();
    sidebar_lines.push(Line::from(""));
    sidebar_lines.push(Line::from(vec![Span::styled(
        "  CATEGORIES",
        Style::default()
            .fg(theme.muted)
            .add_modifier(Modifier::BOLD),
    )]));
    sidebar_lines.push(Line::from(""));

    let sb_inner_w = (sidebar_area.width as usize).saturating_sub(2).max(10);
    for tab in SettingsTab::all() {
        let is_active = *tab == state.active_tab;
        let raw_label = if sidebar_area.width >= 24 {
            tab.sidebar_label()
        } else {
            tab.title()
        };
        let prefix = if is_active { " ▸ " } else { "   " };
        let full_text = format!("{}{}", prefix, raw_label);
        let char_len = full_text.chars().count();
        let padded = if char_len < sb_inner_w {
            format!("{}{}", full_text, " ".repeat(sb_inner_w - char_len))
        } else {
            truncate_middle(&full_text, sb_inner_w)
        };

        let style = if is_active {
            Style::default()
                .fg(theme.bg_primary)
                .bg(theme.brand_accent)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.text_primary)
        };

        sidebar_lines.push(Line::from(vec![
            Span::raw(" "),
            Span::styled(padded, style),
        ]));
        if body_area.height >= 22 {
            sidebar_lines.push(Line::from(""));
        }
    }

    // Add live 4-Slot Chain & Scope quick card in Left Sidebar if height permits
    if body_area.height >= 18 && sidebar_area.width >= 22 {
        let (slots, _) = state.effective_slots_for_current_target();
        let (_, target_label) = state.current_routing_target();
        let slot_max_w = (sidebar_area.width as usize).saturating_sub(10).max(8);
        let fmt_sb_slot = |slot_opt: &Option<crate::config::ModelSlot>| -> String {
            match slot_opt {
                Some(s) if s.is_valid() => truncate_middle(&s.display_short(), slot_max_w),
                _ => "—".to_string(),
            }
        };

        sidebar_lines.push(Line::from(vec![Span::styled(
            format!("  {}", "─".repeat(sb_inner_w.saturating_sub(2))),
            Style::default().fg(theme.border),
        )]));
        sidebar_lines.push(Line::from(vec![Span::styled(
            "  ACTIVE 4-SLOT CHAIN",
            Style::default()
                .fg(theme.muted)
                .add_modifier(Modifier::BOLD),
        )]));
        sidebar_lines.push(Line::from(vec![
            Span::styled("  Target: ", Style::default().fg(theme.muted)),
            Span::styled(
                truncate_middle(target_label, slot_max_w),
                Style::default()
                    .fg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
        sidebar_lines.push(Line::from(vec![
            Span::styled("  [1] Pri: ", Style::default().fg(theme.muted)),
            Span::styled(
                fmt_sb_slot(&slots[0]),
                Style::default()
                    .fg(theme.success)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
        sidebar_lines.push(Line::from(vec![
            Span::styled("  [2] FB1: ", Style::default().fg(theme.muted)),
            Span::styled(fmt_sb_slot(&slots[1]), Style::default().fg(theme.brand_accent)),
        ]));
        sidebar_lines.push(Line::from(vec![
            Span::styled("  [3] FB2: ", Style::default().fg(theme.muted)),
            Span::styled(fmt_sb_slot(&slots[2]), Style::default().fg(theme.info)),
        ]));
        sidebar_lines.push(Line::from(vec![
            Span::styled("  [4] Glb★:", Style::default().fg(theme.warning)),
            Span::styled(
                fmt_sb_slot(&slots[3]),
                Style::default()
                    .fg(theme.warning)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
    }

    frame.render_widget(Paragraph::new(sidebar_lines), sidebar_area);

    // 2. Render Vertical Separator │ between Left Menu Bar and Right Settings Panel
    let sep_lines: Vec<Line> = (0..sep_area.height)
        .map(|_| Line::from(vec![Span::styled("│", Style::default().fg(theme.border))]))
        .collect();
    frame.render_widget(Paragraph::new(sep_lines), sep_area);

    // 3. Render Right Main Settings Panel for the Active Tab
    let mut content_lines = Vec::new();
    let (sec_title, sec_subtitle) = state.active_tab.section_header();
    content_lines.push(Line::from(""));
    content_lines.push(Line::from(vec![
        Span::raw("  "),
        Span::styled(
            sec_title,
            Style::default()
                .fg(theme.brand_accent)
                .add_modifier(Modifier::BOLD),
        ),
    ]));
    content_lines.push(Line::from(vec![
        Span::raw("  "),
        Span::styled(
            truncate_middle(sec_subtitle, (right_area.width as usize).saturating_sub(4).max(20)),
            Style::default().fg(theme.muted),
        ),
    ]));
    content_lines.push(Line::from(vec![
        Span::raw("  "),
        Span::styled(
            "─".repeat((right_area.width as usize).saturating_sub(4).max(10)),
            Style::default().fg(theme.border),
        ),
    ]));
    content_lines.push(Line::from(""));

    let list_max_visible = (right_area.height as usize).saturating_sub(14).max(6);

    match state.active_tab {
        SettingsTab::Providers => {
            let (target_slug, target_label) = state.current_routing_target();
            let (slots, has_custom_override) = state.effective_slots_for_current_target();
            let slot_disp_w = ((right_area.width as usize).saturating_sub(44) / 2).clamp(20, 38);
            let fmt_slot = |slot_opt: &Option<crate::config::ModelSlot>| -> String {
                match slot_opt {
                    Some(s) if s.is_valid() => truncate_middle(&s.display_short(), slot_disp_w),
                    _ => "— (none)".to_string(),
                }
            };

            // 1. Target Scope Selector Header (Global vs 14 MiniPub Specialists)
            let mut target_spans = vec![
                Span::styled(
                    "  🎯 Target: ",
                    Style::default()
                        .fg(theme.brand_accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("◀ {} ▶", target_label),
                    Style::default()
                        .fg(theme.text_primary)
                        .add_modifier(Modifier::BOLD),
                ),
            ];
            if target_slug != "global" {
                target_spans.push(Span::raw(" "));
                if has_custom_override {
                    target_spans.push(Span::styled(
                        "[Custom Override]",
                        Style::default()
                            .fg(theme.warning)
                            .add_modifier(Modifier::BOLD),
                    ));
                } else {
                    target_spans.push(Span::styled(
                        "[Synced w/ Global]",
                        Style::default().fg(theme.success),
                    ));
                }
            } else {
                let custom_count = state
                    .minipub_role_models
                    .values()
                    .filter(|c| !c.is_empty())
                    .count();
                target_spans.push(Span::raw(" "));
                if custom_count == 0 && state.minipub_sync_with_orchestrator {
                    target_spans.push(Span::styled(
                        "[All 14 MiniPubs Synced ✔]",
                        Style::default().fg(theme.success),
                    ));
                } else {
                    target_spans.push(Span::styled(
                        format!("[{} MiniPub custom]", custom_count),
                        Style::default().fg(theme.warning),
                    ));
                }
            }
            target_spans.push(Span::styled(
                "  ([m/M] Target • [a] Sync All)",
                Style::default().fg(theme.muted),
            ));
            content_lines.push(Line::from(target_spans));

            // 2. 4-Slot Chain Summary (1 Primary + 2 Fallbacks + 1 Shared Global Last Fallback ★)
            content_lines.push(Line::from(vec![
                Span::styled("     [1] Primary:      ", Style::default().fg(theme.muted)),
                Span::styled(
                    format!("{:<width$}", fmt_slot(&slots[0]), width = slot_disp_w),
                    Style::default()
                        .fg(theme.success)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled("   [2] 1st Fallback:  ", Style::default().fg(theme.muted)),
                Span::styled(
                    fmt_slot(&slots[1]),
                    if slots[1].is_some() {
                        Style::default().fg(theme.brand_accent)
                    } else {
                        Style::default().fg(theme.muted)
                    },
                ),
            ]));
            content_lines.push(Line::from(vec![
                Span::styled("     [3] 2nd Fallback: ", Style::default().fg(theme.muted)),
                Span::styled(
                    format!("{:<width$}", fmt_slot(&slots[2]), width = slot_disp_w),
                    if slots[2].is_some() {
                        Style::default().fg(theme.info)
                    } else {
                        Style::default().fg(theme.muted)
                    },
                ),
                Span::styled(
                    "   [4] Global Last ★: ",
                    Style::default()
                        .fg(theme.warning)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    fmt_slot(&slots[3]),
                    if slots[3].is_some() {
                        Style::default()
                            .fg(theme.warning)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(theme.muted)
                    },
                ),
            ]));
            content_lines.push(Line::from(""));

            if let Some(ref prov) = state.selecting_model_for_provider {
                // MODEL DRILL-DOWN SUBVIEW
                content_lines.push(Line::from(vec![
                    Span::styled(
                        "  📋 Choose Model from ",
                        Style::default()
                            .fg(theme.brand_accent)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        prov,
                        Style::default()
                            .fg(theme.brand_accent)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!(" ({} models) — press [1/Enter] Primary, [2] 1st FB, [3] 2nd FB, [4] Global Last ★", state.provider_models.len()),
                        Style::default().fg(theme.muted),
                    ),
                ]));

                if state.provider_models.is_empty() {
                    content_lines.push(Line::from(vec![
                        Span::raw("    "),
                        Span::styled(
                            "⚠️ No models found or provider unconfigured. Press [Esc] to return to providers.",
                            Style::default().fg(theme.warning),
                        ),
                    ]));
                } else {
                    let max_visible = list_max_visible;
                    let scroll_offset =
                        compute_scroll_offset(state.model_selected_index, max_visible);
                    let visible_models = state
                        .provider_models
                        .iter()
                        .enumerate()
                        .skip(scroll_offset)
                        .take(max_visible);

                    let current_default = Config::static_default_model_for_provider(prov);
                    let model_col_w = ((right_area.width as usize).saturating_sub(36)).clamp(28, 48);

                    for (idx, model) in visible_models {
                        let is_selected = idx == state.model_selected_index;
                        let matches_slot = |s_opt: &Option<crate::config::ModelSlot>| -> bool {
                            s_opt
                                .as_ref()
                                .map(|s| {
                                    s.provider.eq_ignore_ascii_case(prov) && s.model == model.id
                                })
                                .unwrap_or(false)
                        };
                        let is_s1 = matches_slot(&slots[0]);
                        let is_s2 = matches_slot(&slots[1]);
                        let is_s3 = matches_slot(&slots[2]);
                        let is_s4 = matches_slot(&slots[3]);
                        let is_default = is_s1 || model.id == current_default;

                        let cursor = if is_selected { "  ❯ " } else { "    " };
                        let model_style = if is_selected {
                            Style::default()
                                .fg(theme.brand_accent)
                                .add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().fg(theme.text_primary)
                        };

                        let ctx_str = format_context_tokens(model.context_length);
                        let reasoning = model_supports_reasoning(&model.id, &model.name);

                        let mut spans = vec![
                            Span::styled(
                                cursor,
                                Style::default()
                                    .fg(theme.brand_accent)
                                    .add_modifier(Modifier::BOLD),
                            ),
                            Span::styled(
                                format!(
                                    "{:<width$}",
                                    truncate_middle(&model.id, model_col_w),
                                    width = model_col_w
                                ),
                                model_style,
                            ),
                            Span::styled(" ", Style::default()),
                            Span::styled(
                                format!("{:>6} ctx", ctx_str),
                                Style::default().fg(theme.muted),
                            ),
                        ];

                        if reasoning {
                            spans.push(Span::raw(" "));
                            spans.push(Span::styled(
                                "[🧠]",
                                Style::default().fg(theme.warning),
                            ));
                        }

                        if is_s1 {
                            spans.push(Span::raw(" "));
                            spans.push(Span::styled(
                                "[1·Primary ✔]",
                                Style::default()
                                    .fg(theme.success)
                                    .add_modifier(Modifier::BOLD),
                            ));
                        }
                        if is_s2 {
                            spans.push(Span::raw(" "));
                            spans.push(Span::styled(
                                "[2·1st FB]",
                                Style::default().fg(theme.brand_accent),
                            ));
                        }
                        if is_s3 {
                            spans.push(Span::raw(" "));
                            spans.push(Span::styled(
                                "[3·2nd FB]",
                                Style::default().fg(theme.info),
                            ));
                        }
                        if is_s4 {
                            spans.push(Span::raw(" "));
                            spans.push(Span::styled(
                                "[4·Global Last ★]",
                                Style::default()
                                    .fg(theme.warning)
                                    .add_modifier(Modifier::BOLD),
                            ));
                        } else if !is_s1 && !is_s2 && !is_s3 && is_default {
                            spans.push(Span::raw(" "));
                            spans.push(Span::styled(
                                "[Default]",
                                Style::default().fg(theme.muted),
                            ));
                        }

                        content_lines.push(Line::from(spans));
                    }
                }

                content_lines.push(Line::from(""));
                content_lines.push(Line::from(vec![
                    Span::raw("    "),
                    Span::styled(
                        "[Enter/1] Primary • [2] 1st Fallback • [3] 2nd Fallback • [4] Global Last ★ • [c] Clear • [Esc] Back",
                        Style::default().fg(theme.muted),
                    ),
                ]));
            } else {
                // TOP LEVEL PROVIDERS LIST
                let max_visible = list_max_visible;
                let scroll_offset = compute_scroll_offset(state.selected_index, max_visible);
                let visible_providers = SUPPORTED_PROVIDERS
                    .iter()
                    .enumerate()
                    .skip(scroll_offset)
                    .take(max_visible);
                let model_col_w = ((right_area.width as usize).saturating_sub(42)).clamp(24, 42);

                for (idx, provider) in visible_providers {
                    let is_selected = idx == state.selected_index;
                    let matches_prov = |s_opt: &Option<crate::config::ModelSlot>| -> bool {
                        s_opt
                            .as_ref()
                            .map(|s| s.provider.eq_ignore_ascii_case(provider))
                            .unwrap_or(false)
                    };
                    let is_s1 = matches_prov(&slots[0]);
                    let is_s2 = matches_prov(&slots[1]);
                    let is_s3 = matches_prov(&slots[2]);
                    let is_s4 = matches_prov(&slots[3]);

                    let default_model = if is_s1 {
                        slots[0]
                            .as_ref()
                            .map(|s| s.model.as_str())
                            .unwrap_or_else(|| Config::static_default_model_for_provider(provider))
                    } else if *provider == state.active_provider && !state.active_model.is_empty() {
                        state.active_model.as_str()
                    } else {
                        Config::static_default_model_for_provider(provider)
                    };

                    let cursor = if is_selected { "  ❯ " } else { "    " };
                    let prov_style = if is_selected {
                        Style::default()
                            .fg(theme.brand_accent)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(theme.text_primary)
                    };

                    let mut spans = vec![
                        Span::styled(
                            cursor,
                            Style::default()
                                .fg(theme.brand_accent)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(format!("{:<14}", provider), prov_style),
                        Span::styled(" model: ", Style::default().fg(theme.muted)),
                        Span::styled(
                            format!(
                                "{:<width$}",
                                truncate_middle(default_model, model_col_w),
                                width = model_col_w
                            ),
                            Style::default().fg(theme.text_primary),
                        ),
                    ];

                    if is_s1 {
                        spans.push(Span::styled(
                            " [1·Primary ✔]",
                            Style::default()
                                .fg(theme.success)
                                .add_modifier(Modifier::BOLD),
                        ));
                    }
                    if is_s2 {
                        spans.push(Span::styled(
                            " [2·1st FB]",
                            Style::default().fg(theme.brand_accent),
                        ));
                    }
                    if is_s3 {
                        spans.push(Span::styled(
                            " [3·2nd FB]",
                            Style::default().fg(theme.info),
                        ));
                    }
                    if is_s4 {
                        spans.push(Span::styled(
                            " [4·Global Last ★]",
                            Style::default()
                                .fg(theme.warning)
                                .add_modifier(Modifier::BOLD),
                        ));
                    }

                    content_lines.push(Line::from(spans));
                }
                content_lines.push(Line::from(""));
                content_lines.push(Line::from(vec![
                    Span::raw("    "),
                    Span::styled(
                        "[Enter] Browse Models • [1/Space] Primary • [2] 1st FB • [3] 2nd FB • [4] Global Last ★ • [c] Clear",
                        Style::default().fg(theme.muted),
                    ),
                ]));
            }
        }
        SettingsTab::Workspace => {
            // Path middle-truncation based on available width in right panel
            let path_avail_w = (right_area.width as usize).saturating_sub(26).max(20);
            let display_path = truncate_middle(&state.workspace_path, path_avail_w);

            // Item 0: Root path
            content_lines.push(Line::from(vec![
                Span::styled(
                    if state.selected_index == 0 {
                        "  ❯ "
                    } else {
                        "    "
                    },
                    Style::default()
                        .fg(theme.brand_accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    "📂 Workspace Root: ",
                    Style::default()
                        .fg(theme.brand_accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(display_path, Style::default().fg(theme.text_primary)),
            ]));
            content_lines.push(Line::from(""));

            // Item 1: Active provider and model
            content_lines.push(Line::from(vec![
                Span::styled(
                    if state.selected_index == 1 {
                        "  ❯ "
                    } else {
                        "    "
                    },
                    Style::default()
                        .fg(theme.brand_accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    "🤖 Assigned Provider & Model: ",
                    Style::default()
                        .fg(theme.brand_accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("{} → {}", state.active_provider, state.active_model),
                    Style::default().fg(theme.text_primary),
                ),
            ]));
            content_lines.push(Line::from(""));

            // Item 2: Scope preference (Vertical non-clipping layout)
            let is_scope_selected = state.selected_index == 2;
            content_lines.push(Line::from(vec![
                Span::styled(
                    if is_scope_selected { "  ❯ " } else { "    " },
                    Style::default()
                        .fg(theme.brand_accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    "🎯 Configuration Scope Preference:",
                    Style::default()
                        .fg(theme.brand_accent)
                        .add_modifier(Modifier::BOLD),
                ),
            ]));

            let is_ws = state.save_to_workspace;
            content_lines.push(Line::from(""));
            content_lines.push(Line::from(vec![
                Span::raw("      "),
                Span::styled(
                    if is_ws {
                        "[●] Workspace only (.minicode/config.toml)"
                    } else {
                        "[○] Workspace only (.minicode/config.toml)"
                    },
                    if is_ws {
                        Style::default()
                            .fg(theme.success)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(theme.text_primary)
                    },
                ),
                Span::styled(
                    "  — Project isolated; checked into git",
                    Style::default().fg(theme.muted),
                ),
            ]));
            content_lines.push(Line::from(vec![
                Span::raw("      "),
                Span::styled(
                    if !is_ws {
                        "[●] Global default (~/.config/minicode/config.toml)"
                    } else {
                        "[○] Global default (~/.config/minicode/config.toml)"
                    },
                    if !is_ws {
                        Style::default()
                            .fg(theme.brand_accent)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(theme.text_primary)
                    },
                ),
                Span::styled(
                    "  — Global user default across all repos",
                    Style::default().fg(theme.muted),
                ),
            ]));

            content_lines.push(Line::from(""));
            content_lines.push(Line::from(vec![
                Span::raw("    "),
                Span::styled(
                    "Press [Enter] or [Space] to toggle scope preference",
                    Style::default().fg(theme.muted),
                ),
            ]));
        }
        SettingsTab::Autonomy => {
            // Item 0: Auto-Approve
            let auto_appr_badge = if state.auto_approve {
                "[●] Enabled (Auto-execute)"
            } else {
                "[○] Disabled (Prompt confirmation)"
            };

            content_lines.push(Line::from(vec![
                Span::styled(
                    if state.selected_index == 0 {
                        "  ❯ "
                    } else {
                        "    "
                    },
                    Style::default()
                        .fg(theme.brand_accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    "⚡ Auto-Approve: ",
                    Style::default()
                        .fg(theme.brand_accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    auto_appr_badge,
                    if state.auto_approve {
                        Style::default()
                            .fg(theme.success)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(theme.text_primary)
                    },
                ),
                Span::styled(
                    if state.auto_approve {
                        "  — Auto-executes tool actions"
                    } else {
                        "  — Prompts before mutating actions"
                    },
                    Style::default().fg(theme.muted),
                ),
            ]));
            content_lines.push(Line::from(""));

            // Item 1: Thinking Budget
            let budget_label = if state.thinking_budget == 0 {
                "0 tokens (Disabled)".to_string()
            } else {
                format!("{} tokens", state.thinking_budget)
            };

            content_lines.push(Line::from(vec![
                Span::styled(
                    if state.selected_index == 1 {
                        "  ❯ "
                    } else {
                        "    "
                    },
                    Style::default()
                        .fg(theme.brand_accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    "🧠 Extended Thinking Budget: ",
                    Style::default()
                        .fg(theme.brand_accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    budget_label,
                    if state.selected_index == 1 {
                        Style::default()
                            .fg(theme.brand_accent)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(theme.text_primary)
                    },
                ),
                Span::styled(
                    "  — Allocated reasoning tokens (Claude 3.7 / Gemini 2.5)",
                    Style::default().fg(theme.muted),
                ),
            ]));
            content_lines.push(Line::from(""));

            // Item 2: Approval Policy
            content_lines.push(Line::from(vec![
                Span::styled(
                    if state.selected_index == 2 {
                        "  ❯ "
                    } else {
                        "    "
                    },
                    Style::default()
                        .fg(theme.brand_accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    "🛡️ Approval Policy: ",
                    Style::default()
                        .fg(theme.brand_accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("[ {} ]", state.approval_policy),
                    if state.selected_index == 2 {
                        Style::default()
                            .fg(theme.brand_accent)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(theme.text_primary)
                    },
                ),
                Span::styled(
                    "  — (strict: confirm all | prompt: mutate only | permissive: full autonomy)",
                    Style::default().fg(theme.muted),
                ),
            ]));
            content_lines.push(Line::from(""));

            // Item 3: MiniPub Max Parallel Workers
            content_lines.push(Line::from(vec![
                Span::styled(
                    if state.selected_index == 3 {
                        "  ❯ "
                    } else {
                        "    "
                    },
                    Style::default()
                        .fg(theme.brand_accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    "🤖 MiniPub Max Parallel: ",
                    Style::default()
                        .fg(theme.brand_accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("[ {} workers ]", state.minipub_max_parallel),
                    if state.selected_index == 3 {
                        Style::default()
                            .fg(theme.brand_accent)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(theme.text_primary)
                    },
                ),
                Span::styled(
                    "  — Max concurrent in-process MiniPub subagents (default: 4)",
                    Style::default().fg(theme.muted),
                ),
            ]));
            content_lines.push(Line::from(""));

            // Item 4: MiniPub Inactivity Timeout
            content_lines.push(Line::from(vec![
                Span::styled(
                    if state.selected_index == 4 {
                        "  ❯ "
                    } else {
                        "    "
                    },
                    Style::default()
                        .fg(theme.brand_accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    "⏱️ MiniPub Inactivity Timeout: ",
                    Style::default()
                        .fg(theme.brand_accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("[ {}s ]", state.minipub_inactivity_timeout_secs),
                    if state.selected_index == 4 {
                        Style::default()
                            .fg(theme.brand_accent)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(theme.text_primary)
                    },
                ),
                Span::styled(
                    "  — Auto-terminates stalled MiniPub worker if silent (resets on heartbeat)",
                    Style::default().fg(theme.muted),
                ),
            ]));
            content_lines.push(Line::from(""));

            // Item 5: MiniPub Worktree Isolation Mode
            content_lines.push(Line::from(vec![
                Span::styled(
                    if state.selected_index == 5 {
                        "  ❯ "
                    } else {
                        "    "
                    },
                    Style::default()
                        .fg(theme.brand_accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    "🌿 MiniPub Isolation Mode: ",
                    Style::default()
                        .fg(theme.brand_accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("[ {} ]", state.minipub_worktree_isolation),
                    if state.selected_index == 5 {
                        Style::default()
                            .fg(theme.brand_accent)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(theme.text_primary)
                    },
                ),
                Span::styled(
                    "  — (auto: leases/shared first | always: git worktree | never: shared only)",
                    Style::default().fg(theme.muted),
                ),
            ]));
            content_lines.push(Line::from(""));

            // Item 6: Model Stream Idle Timeout
            content_lines.push(Line::from(vec![
                Span::styled(
                    if state.selected_index == 6 {
                        "  ❯ "
                    } else {
                        "    "
                    },
                    Style::default()
                        .fg(theme.brand_accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    "📡 Model Stream Idle Timeout: ",
                    Style::default()
                        .fg(theme.brand_accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("[ {}s ]", state.stream_idle_timeout_secs),
                    if state.selected_index == 6 {
                        Style::default()
                            .fg(theme.brand_accent)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(theme.text_primary)
                    },
                ),
                Span::styled(
                    "  — Adaptive SSE inter-chunk idle timeout for LLM streaming",
                    Style::default().fg(theme.muted),
                ),
            ]));
            content_lines.push(Line::from(""));
            content_lines.push(Line::from(vec![
                Span::raw("    "),
                Span::styled(
                    "Press [Enter] or [Space] to toggle/cycle autonomy, MiniPub & timeout settings",
                    Style::default().fg(theme.muted),
                ),
            ]));
        }
        SettingsTab::Probes => {
            content_lines.push(Line::from(vec![
                Span::styled(
                    "  ❯ ",
                    Style::default()
                        .fg(theme.brand_accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    "[ ▶ Probe All Configured Provider Endpoints ]",
                    Style::default()
                        .fg(theme.brand_accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    "  — Verifies connectivity, latency & credentials",
                    Style::default().fg(theme.muted),
                ),
            ]));
            content_lines.push(Line::from(""));

            if state.probing {
                content_lines.push(Line::from(vec![
                    Span::raw("    "),
                    Span::styled(
                        "⏳ Testing connectivity, latency, and credentials across all providers...",
                        Style::default().fg(theme.warning),
                    ),
                ]));
            } else if let Some(ref results) = state.probe_results {
                for chunk in results.chunks(2) {
                    let mut line_spans = vec![Span::raw("    ")];
                    for (i, r) in chunk.iter().enumerate() {
                        if i > 0 {
                            line_spans.push(Span::raw("     "));
                        }
                        let status_badge = match r.status.as_str() {
                            "connected" => Span::styled(
                                "✔",
                                Style::default()
                                    .fg(theme.success)
                                    .add_modifier(Modifier::BOLD),
                            ),
                            "disconnected" => Span::styled(
                                "✗",
                                Style::default()
                                    .fg(theme.destructive)
                                    .add_modifier(Modifier::BOLD),
                            ),
                            _ => Span::styled("○", Style::default().fg(theme.warning)),
                        };
                        line_spans.push(status_badge);
                        line_spans.push(Span::raw(" "));
                        line_spans.push(Span::styled(
                            format!("{:<12}", r.provider),
                            Style::default()
                                .fg(theme.text_primary)
                                .add_modifier(Modifier::BOLD),
                        ));
                        line_spans.push(Span::styled(
                            format!("{:>4}ms", r.latency_ms),
                            Style::default().fg(theme.muted),
                        ));
                    }
                    content_lines.push(Line::from(line_spans));
                }
            } else {
                content_lines.push(Line::from(vec![
                    Span::raw("    "),
                    Span::styled(
                        "Press [Enter] or [Space] to test live connections, ping latencies, and credentials.",
                        Style::default().fg(theme.muted),
                    ),
                ]));
            }
        }
        SettingsTab::Browser => {
            // Item 0: Default Engine override
            let def_engine_str = match state.browser_default_engine.as_deref() {
                Some("obscura") => "Obscura (Pure-Rust V8, ~30MB)",
                Some("firefox") => "Firefox (Gecko)",
                Some("chrome") => "Chrome (Chromium Blink)",
                Some(other) => other,
                None => "Auto (Adaptive Priority)",
            };

            content_lines.push(Line::from(vec![
                Span::styled(
                    if state.selected_index == 0 {
                        "  ❯ "
                    } else {
                        "    "
                    },
                    Style::default()
                        .fg(theme.brand_accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    "🌐 Default Engine: ",
                    Style::default()
                        .fg(theme.brand_accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("[ {} ]", def_engine_str),
                    if state.selected_index == 0 {
                        Style::default()
                            .fg(theme.brand_accent)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(theme.text_primary)
                    },
                ),
                Span::styled(
                    "  — Auto adapts, or forces specific browser binary",
                    Style::default().fg(theme.muted),
                ),
            ]));
            content_lines.push(Line::from(""));

            // Item 1: Headless Mode Engine Priority
            let hl_chain = state.browser_headless_priority.join(" ➔ ");
            content_lines.push(Line::from(vec![
                Span::styled(
                    if state.selected_index == 1 {
                        "  ❯ "
                    } else {
                        "    "
                    },
                    Style::default()
                        .fg(theme.brand_accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    "⚡ Headless Mode Priority: ",
                    Style::default()
                        .fg(theme.brand_accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("[ {} ]", hl_chain),
                    if state.selected_index == 1 {
                        Style::default()
                            .fg(theme.success)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(theme.text_primary)
                    },
                ),
            ]));
            content_lines.push(Line::from(vec![
                Span::raw("      "),
                Span::styled(
                    "1st priority: Obscura (~30MB RAM footprint, instant start)",
                    Style::default().fg(theme.muted),
                ),
            ]));
            content_lines.push(Line::from(""));

            // Item 2: GUI Headed Mode Engine Priority
            let gui_chain = state.browser_gui_priority.join(" ➔ ");
            content_lines.push(Line::from(vec![
                Span::styled(
                    if state.selected_index == 2 {
                        "  ❯ "
                    } else {
                        "    "
                    },
                    Style::default()
                        .fg(theme.brand_accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    "🖥️ GUI Headed Mode Priority: ",
                    Style::default()
                        .fg(theme.brand_accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("[ {} ]", gui_chain),
                    if state.selected_index == 2 {
                        Style::default().fg(theme.info).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(theme.text_primary)
                    },
                ),
            ]));
            content_lines.push(Line::from(vec![
                Span::raw("      "),
                Span::styled(
                    "1st priority: Firefox for headed inspection (or Chrome)",
                    Style::default().fg(theme.muted),
                ),
            ]));
            content_lines.push(Line::from(""));

            // Item 3: Reset Preferences to Defaults
            content_lines.push(Line::from(vec![
                Span::styled(
                    if state.selected_index == 3 {
                        "  ❯ "
                    } else {
                        "    "
                    },
                    Style::default()
                        .fg(theme.brand_accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    "↺ [ Reset Browser Preferences to System Defaults ]",
                    if state.selected_index == 3 {
                        Style::default()
                            .fg(theme.warning)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(theme.text_primary)
                    },
                ),
            ]));
            content_lines.push(Line::from(""));

            content_lines.push(Line::from(vec![
                Span::raw("    "),
                Span::styled(
                    "Press [Enter] or [Space] to cycle engine / rotate priorities",
                    Style::default().fg(theme.muted),
                ),
            ]));
        }
    }

    let content_p = Paragraph::new(content_lines);
    frame.render_widget(content_p, right_area);

    // 4. Bottom Divider above Footer
    let bot_divider = Paragraph::new(Line::from(vec![Span::styled(
        "─".repeat(inner_area.width as usize),
        Style::default().fg(theme.border),
    )]));
    frame.render_widget(bot_divider, outer_chunks[1]);

    // 5. Footer Key Hints (Context sensitive)
    let footer_line = if state.selecting_model_for_provider.is_some() {
        Line::from(vec![
            Span::styled(
                "  [↑/↓] ",
                Style::default()
                    .fg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("Navigate Models  ", Style::default().fg(theme.text_primary)),
            Span::styled(
                "[1/Enter] ",
                Style::default()
                    .fg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("Primary  ", Style::default().fg(theme.text_primary)),
            Span::styled(
                "[2/3/4] ",
                Style::default()
                    .fg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("Fallbacks  ", Style::default().fg(theme.text_primary)),
            Span::styled(
                "[m/M] ",
                Style::default()
                    .fg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("Target Scope  ", Style::default().fg(theme.text_primary)),
            Span::styled(
                "[Esc/Backspace] ",
                Style::default()
                    .fg(theme.muted)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("Back to Providers", Style::default().fg(theme.muted)),
        ])
    } else {
        Line::from(vec![
            Span::styled(
                "  [Tab/◄►] ",
                Style::default()
                    .fg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("Left Menu Tab  ", Style::default().fg(theme.text_primary)),
            Span::styled(
                "[↑/↓] ",
                Style::default()
                    .fg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("Navigate  ", Style::default().fg(theme.text_primary)),
            Span::styled(
                "[Enter/Space] ",
                Style::default()
                    .fg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("Select/Toggle  ", Style::default().fg(theme.text_primary)),
            Span::styled(
                "[m/M] ",
                Style::default()
                    .fg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("Target Scope  ", Style::default().fg(theme.text_primary)),
            Span::styled("[Esc] ", Style::default().fg(theme.muted)),
            Span::styled("Save & Close", Style::default().fg(theme.muted)),
        ])
    };
    frame.render_widget(Paragraph::new(footer_line), outer_chunks[2]);
}

/// Renders the configuration modification confirmation card modal.
pub fn render_config_approval(
    frame: &mut Frame,
    area: Rect,
    theme: &Theme,
    proposal: &ConfigChangeProposal,
    selected_index: usize,
) {
    let popup_area = centered_rect_exact(68, 12, area);
    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(" Permission Required ")
        .title_alignment(Alignment::Left)
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(Style::default().fg(theme.brand_accent))
        .style(Style::default().bg(theme.bg_elevated));

    let inner_area = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Header title
            Constraint::Length(1), // Divider
            Constraint::Min(4),    // Details
            Constraint::Length(2), // Action buttons
        ])
        .split(inner_area);

    // Header title
    let header_p = Paragraph::new(Line::from(vec![Span::styled(
        "  minicode requests permission to modify configuration:",
        Style::default()
            .fg(theme.brand_accent)
            .add_modifier(Modifier::BOLD),
    )]));
    frame.render_widget(header_p, chunks[0]);

    // Divider
    let divider = Paragraph::new(Line::from(vec![Span::styled(
        "─".repeat(inner_area.width as usize),
        Style::default().fg(theme.border),
    )]));
    frame.render_widget(divider, chunks[1]);

    // Details
    let mut detail_lines = Vec::new();

    if let Some(ref p) = proposal.provider {
        detail_lines.push(Line::from(vec![
            Span::styled(
                "     • Active Provider : ",
                Style::default().fg(theme.text_primary),
            ),
            Span::styled(
                p,
                Style::default()
                    .fg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
    }
    if let Some(ref m) = proposal.model {
        detail_lines.push(Line::from(vec![
            Span::styled(
                "     • Active Model    : ",
                Style::default().fg(theme.text_primary),
            ),
            Span::styled(
                m,
                Style::default()
                    .fg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
    }
    if let Some(aa) = proposal.auto_approve {
        detail_lines.push(Line::from(vec![
            Span::styled(
                "     • Auto Approve    : ",
                Style::default().fg(theme.text_primary),
            ),
            Span::styled(
                if aa { "true" } else { "false" },
                Style::default()
                    .fg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
    }
    if let Some(tb) = proposal.thinking_budget {
        detail_lines.push(Line::from(vec![
            Span::styled(
                "     • Thinking Budget : ",
                Style::default().fg(theme.text_primary),
            ),
            Span::styled(
                format!("{} tokens", tb),
                Style::default()
                    .fg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
    }
    if let Some(ref th) = proposal.theme {
        detail_lines.push(Line::from(vec![
            Span::styled(
                "     • Theme           : ",
                Style::default().fg(theme.text_primary),
            ),
            Span::styled(
                th,
                Style::default()
                    .fg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
    }

    let scope_desc = if proposal.scope.eq_ignore_ascii_case("global") {
        "Global (~/.config/minicode/config.toml)"
    } else {
        "Workspace (.minicode/config.toml)"
    };
    detail_lines.push(Line::from(vec![
        Span::styled(
            "     • Scope           : ",
            Style::default().fg(theme.text_primary),
        ),
        Span::styled(scope_desc, Style::default().fg(theme.warning)),
    ]));

    let details_p = Paragraph::new(detail_lines);
    frame.render_widget(details_p, chunks[2]);

    // Action buttons
    let is_allow_selected = selected_index == 0;
    let is_deny_selected = selected_index == 1;

    let allow_style = if is_allow_selected {
        Style::default()
            .fg(theme.bg_primary)
            .bg(theme.brand_accent)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.text_primary)
    };

    let deny_style = if is_deny_selected {
        Style::default()
            .fg(theme.bg_primary)
            .bg(theme.destructive)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.muted)
    };

    let buttons_line = Line::from(vec![
        Span::raw("  "),
        Span::styled(" 1. Approve ", allow_style),
        Span::raw("    "),
        Span::styled(" 2. Reject ", deny_style),
    ]);
    frame.render_widget(Paragraph::new(buttons_line), chunks[3]);
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use tempfile::TempDir;

    #[test]
    fn test_settings_modal_state_initialization_and_tab_switching() {
        let temp_dir = TempDir::new().unwrap();
        let config = Config::default();
        let mut state = SettingsModalState::from_config(&config, temp_dir.path());

        assert_eq!(state.active_tab, SettingsTab::Providers);
        assert_eq!(state.selected_index, 0);

        // Next tab
        state.next_tab();
        assert_eq!(state.active_tab, SettingsTab::Workspace);
        assert_eq!(state.selected_index, 0);

        state.next_tab();
        assert_eq!(state.active_tab, SettingsTab::Autonomy);
        assert_eq!(state.selected_index, 0);

        state.next_tab();
        assert_eq!(state.active_tab, SettingsTab::Probes);
        assert_eq!(state.selected_index, 0);

        state.next_tab();
        assert_eq!(state.active_tab, SettingsTab::Browser);
        assert_eq!(state.selected_index, 0);

        // Wrap around
        state.next_tab();
        assert_eq!(state.active_tab, SettingsTab::Providers);

        // Prev tab
        state.prev_tab();
        assert_eq!(state.active_tab, SettingsTab::Browser);

        state.prev_tab();
        assert_eq!(state.active_tab, SettingsTab::Probes);
    }

    #[test]
    fn test_settings_modal_navigation_bounds() {
        let temp_dir = TempDir::new().unwrap();
        let config = Config::default();
        let mut state = SettingsModalState::from_config(&config, temp_dir.path());

        assert_eq!(state.selected_index, 0);
        state.prev_item();
        assert_eq!(state.selected_index, 0);

        let max = state.max_items();
        for _ in 0..max + 5 {
            state.next_item();
        }
        assert_eq!(state.selected_index, max.saturating_sub(1));
    }

    #[test]
    fn test_render_settings_all_tabs() {
        let temp_dir = TempDir::new().unwrap();
        let config = Config::default();
        let mut state = SettingsModalState::from_config(&config, temp_dir.path());
        let theme = Theme::default();
        let backend = TestBackend::new(90, 30);
        let mut terminal = Terminal::new(backend).unwrap();

        for tab in SettingsTab::all() {
            state.active_tab = *tab;
            state.selected_index = 0;
            terminal
                .draw(|f| {
                    let area = f.area();
                    render_settings(f, area, &theme, &state);
                })
                .unwrap();
        }
    }

    #[test]
    fn test_render_settings_probes_with_results() {
        let temp_dir = TempDir::new().unwrap();
        let config = Config::default();
        let mut state = SettingsModalState::from_config(&config, temp_dir.path());
        state.active_tab = SettingsTab::Probes;
        state.probe_results = Some(vec![
            ConnectionTestResult {
                provider: "anthropic".to_string(),
                status: "connected".to_string(),
                latency_ms: 120,
                http_status: Some(200),
                key_source: "environment (ANTHROPIC_API_KEY)".to_string(),
                key_masked: "sk-ant...xyz".to_string(),
                model_count: 5,
                error: None,
            },
            ConnectionTestResult {
                provider: "ollama".to_string(),
                status: "disconnected".to_string(),
                latency_ms: 45,
                http_status: None,
                key_source: "none (local endpoint)".to_string(),
                key_masked: String::new(),
                model_count: 0,
                error: Some("Connection refused (os error 111)".to_string()),
            },
        ]);

        let theme = Theme::default();
        let backend = TestBackend::new(90, 30);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|f| {
                let area = f.area();
                render_settings(f, area, &theme, &state);
            })
            .unwrap();
    }

    #[test]
    fn test_render_config_approval_dialog() {
        let proposal = ConfigChangeProposal {
            scope: "workspace".to_string(),
            provider: Some("anthropic".to_string()),
            model: Some("claude-3-7-sonnet-20250219".to_string()),
            auto_approve: Some(true),
            thinking_budget: Some(8192),
            theme: Some("catppuccin".to_string()),
        };

        let theme = Theme::default();
        let backend = TestBackend::new(90, 30);
        let mut terminal = Terminal::new(backend).unwrap();

        // Test with Allow selected (index 0)
        terminal
            .draw(|f| {
                let area = f.area();
                render_config_approval(f, area, &theme, &proposal, 0);
            })
            .unwrap();

        // Test with Deny selected (index 1)
        terminal
            .draw(|f| {
                let area = f.area();
                render_config_approval(f, area, &theme, &proposal, 1);
            })
            .unwrap();
    }

    #[test]
    fn test_settings_modal_model_drilldown_navigation() {
        let temp_dir = TempDir::new().unwrap();
        let config = Config::default();
        let mut state = SettingsModalState::from_config(&config, temp_dir.path());

        assert!(state.selecting_model_for_provider.is_none());
        assert_eq!(state.max_items(), SUPPORTED_PROVIDERS.len());

        // Enter model drilldown for anthropic
        state.selecting_model_for_provider = Some("anthropic".to_string());
        state.provider_models = vec![
            crate::agent::models::ModelInfo {
                id: "claude-3-7-sonnet-20250219".to_string(),
                name: "Claude 3.7 Sonnet".to_string(),
                description: None,
                context_length: Some(200_000),
                is_free: false,
            },
            crate::agent::models::ModelInfo {
                id: "claude-3-5-sonnet-20241022".to_string(),
                name: "Claude 3.5 Sonnet".to_string(),
                description: None,
                context_length: Some(200_000),
                is_free: false,
            },
        ];
        state.model_selected_index = 0;

        assert_eq!(state.max_items(), 2);
        state.next_item();
        assert_eq!(state.model_selected_index, 1);
        state.next_item();
        assert_eq!(state.model_selected_index, 1); // at boundary
        state.prev_item();
        assert_eq!(state.model_selected_index, 0);

        // Switching tab cancels drilldown
        state.next_tab();
        assert_eq!(state.active_tab, SettingsTab::Workspace);
        assert!(state.selecting_model_for_provider.is_none());
        assert!(state.provider_models.is_empty());
    }

    #[test]
    fn test_render_settings_model_drilldown_and_narrow_scroll() {
        let temp_dir = TempDir::new().unwrap();
        let config = Config::default();
        let mut state = SettingsModalState::from_config(&config, temp_dir.path());
        state.selecting_model_for_provider = Some("anthropic".to_string());
        state.provider_models = vec![
            crate::agent::models::ModelInfo {
                id: "claude-3-7-sonnet-20250219".to_string(),
                name: "Claude 3.7 Sonnet".to_string(),
                description: None,
                context_length: Some(200_000),
                is_free: false,
            },
            crate::agent::models::ModelInfo {
                id: "claude-3-5-haiku-20241022".to_string(),
                name: "Claude 3.5 Haiku".to_string(),
                description: None,
                context_length: Some(200_000),
                is_free: false,
            },
        ];
        state.model_selected_index = 0;

        let theme = Theme::default();
        // Test standard view
        let backend = TestBackend::new(95, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| {
                let area = f.area();
                render_settings(f, area, &theme, &state);
            })
            .unwrap();

        // Test narrow view (e.g. 55 cols) to exercise horizontal scrolling and indicator rendering
        let narrow_backend = TestBackend::new(55, 24);
        let mut narrow_terminal = Terminal::new(narrow_backend).unwrap();
        narrow_terminal
            .draw(|f| {
                let area = f.area();
                render_settings(f, area, &theme, &state);
            })
            .unwrap();
    }

    #[test]
    fn test_truncate_middle_and_format_tokens() {
        assert_eq!(truncate_middle("hello", 10), "hello");
        assert_eq!(
            truncate_middle("/home/user/super/long/workspace/path/to/project", 25),
            "/home/user/.../to/project"
        );
        assert_eq!(format_context_tokens(Some(2_000_000)), "2M");
        assert_eq!(format_context_tokens(Some(128_000)), "128k");
        assert_eq!(format_context_tokens(Some(512)), "512");
        assert_eq!(format_context_tokens(None), "-");
    }

    #[test]
    fn test_settings_modal_browser_tab() {
        let temp_dir = TempDir::new().unwrap();
        let config = Config::default();
        let mut state = SettingsModalState::from_config(&config, temp_dir.path());

        state.active_tab = SettingsTab::Browser;
        assert_eq!(state.max_items(), 4);

        let theme = Theme::default();
        let backend = TestBackend::new(95, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| {
                let area = f.area();
                render_settings(f, area, &theme, &state);
            })
            .unwrap();
    }
}

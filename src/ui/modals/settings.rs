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
    if s.len() <= max_len || max_len < 8 {
        return s.to_string();
    }
    let keep = max_len.saturating_sub(3);
    let prefix_len = keep / 2;
    let suffix_len = keep - prefix_len;
    format!("{}...{}", &s[..prefix_len], &s[s.len() - suffix_len..])
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
}

impl SettingsTab {
    /// Returns all tabs in order.
    pub fn all() -> &'static [SettingsTab] {
        &[
            SettingsTab::Providers,
            SettingsTab::Workspace,
            SettingsTab::Autonomy,
            SettingsTab::Probes,
        ]
    }

    /// Tab display title.
    pub fn title(&self) -> &'static str {
        match self {
            SettingsTab::Providers => "1. Providers",
            SettingsTab::Workspace => "2. Workspace",
            SettingsTab::Autonomy => "3. Autonomy",
            SettingsTab::Probes => "4. Probes",
        }
    }

    /// Cycles to the next tab.
    pub fn next(&self) -> Self {
        match self {
            SettingsTab::Providers => SettingsTab::Workspace,
            SettingsTab::Workspace => SettingsTab::Autonomy,
            SettingsTab::Autonomy => SettingsTab::Probes,
            SettingsTab::Probes => SettingsTab::Providers,
        }
    }

    /// Cycles to the previous tab.
    pub fn prev(&self) -> Self {
        match self {
            SettingsTab::Providers => SettingsTab::Probes,
            SettingsTab::Workspace => SettingsTab::Providers,
            SettingsTab::Autonomy => SettingsTab::Workspace,
            SettingsTab::Probes => SettingsTab::Autonomy,
        }
    }
}

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
    pub probe_results: Option<Vec<ConnectionTestResult>>,
    pub probing: bool,

    // Model selection drill-down sub-state for Providers tab:
    pub selecting_model_for_provider: Option<String>,
    pub provider_models: Vec<crate::agent::models::ModelInfo>,
    pub model_selected_index: usize,
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
            probe_results: None,
            probing: false,
            selecting_model_for_provider: None,
            provider_models: Vec::new(),
            model_selected_index: 0,
        }
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
            SettingsTab::Autonomy => 3,
            SettingsTab::Probes => 1,
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

/// Renders the `/settings` configuration modal dialog into the provided frame.
pub fn render_settings(frame: &mut Frame, area: Rect, theme: &Theme, state: &SettingsModalState) {
    // Dynamic responsive modal width (clamped 86..=110 cols) and height (clamped 24..=32 lines)
    let modal_width = (area.width * 88 / 100).clamp(86, 110).min(area.width);
    let modal_height = (area.height * 82 / 100).clamp(24, 32).min(area.height);
    let popup_area = centered_rect_exact(modal_width, modal_height, area);
    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(Line::from(vec![Span::styled(
            " ⚙️ minicode settings ",
            Style::default()
                .fg(theme.brand_accent)
                .add_modifier(Modifier::BOLD),
        )]))
        .title_alignment(Alignment::Center)
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

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Tab header bar
            Constraint::Length(1), // Divider
            Constraint::Min(8),    // Active tab content
            Constraint::Length(1), // Footer key hints
        ])
        .split(inner_area);

    // 1. Responsive Horizontal Scrollable Tab Header Bar
    let all_tabs = SettingsTab::all();
    let active_tab_idx = all_tabs
        .iter()
        .position(|t| *t == state.active_tab)
        .unwrap_or(0);
    let avail_width = inner_area.width as usize;

    let formatted_tabs: Vec<String> = all_tabs
        .iter()
        .map(|t| format!(" [ {} ] ", t.title()))
        .collect();
    let tab_widths: Vec<usize> = formatted_tabs.iter().map(|s| s.len()).collect();
    let total_tab_w: usize =
        tab_widths.iter().sum::<usize>() + (all_tabs.len().saturating_sub(1) * 2) + 2;

    if total_tab_w <= avail_width {
        // All tabs fit comfortably on screen
        let mut tab_spans = vec![Span::raw(" ")];
        for (idx, tab) in all_tabs.iter().enumerate() {
            let is_active = *tab == state.active_tab;
            let style = if is_active {
                Style::default()
                    .fg(theme.bg_primary)
                    .bg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.text_primary)
            };
            tab_spans.push(Span::styled(&formatted_tabs[idx], style));
            if idx + 1 < all_tabs.len() {
                tab_spans.push(Span::raw("  "));
            }
        }
        frame.render_widget(Paragraph::new(Line::from(tab_spans)), chunks[0]);
    } else {
        // Horizontal scroll mode: sliding window containing active_tab_idx with ◀ / ▶ indicators
        let ind_w = 3; // "◀  " or "  ▶"
        let mut start_idx = active_tab_idx;
        let mut end_idx = active_tab_idx;

        loop {
            let mut expanded = false;
            if end_idx + 1 < all_tabs.len() {
                let has_left = start_idx > 0;
                let has_right = end_idx + 2 < all_tabs.len();
                let next_w = tab_widths[start_idx..=end_idx + 1].iter().sum::<usize>()
                    + (end_idx + 1 - start_idx) * 2
                    + (if has_left { ind_w } else { 0 })
                    + (if has_right { ind_w } else { 0 });
                if next_w <= avail_width {
                    end_idx += 1;
                    expanded = true;
                }
            }
            if start_idx > 0 {
                let has_left = start_idx - 1 > 0;
                let has_right = end_idx + 1 < all_tabs.len();
                let next_w = tab_widths[start_idx - 1..=end_idx].iter().sum::<usize>()
                    + (end_idx - (start_idx - 1)) * 2
                    + (if has_left { ind_w } else { 0 })
                    + (if has_right { ind_w } else { 0 });
                if next_w <= avail_width {
                    start_idx -= 1;
                    expanded = true;
                }
            }
            if !expanded {
                break;
            }
        }

        let has_left = start_idx > 0;
        let has_right = end_idx + 1 < all_tabs.len();

        let mut tab_spans = Vec::new();
        if has_left {
            tab_spans.push(Span::styled(
                "◀ ",
                Style::default()
                    .fg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD),
            ));
        } else {
            tab_spans.push(Span::raw(" "));
        }

        for idx in start_idx..=end_idx {
            let tab = all_tabs[idx];
            let is_active = tab == state.active_tab;
            let style = if is_active {
                Style::default()
                    .fg(theme.bg_primary)
                    .bg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.text_primary)
            };
            tab_spans.push(Span::styled(&formatted_tabs[idx], style));
            if idx < end_idx {
                tab_spans.push(Span::raw("  "));
            }
        }

        if has_right {
            tab_spans.push(Span::styled(
                " ▶",
                Style::default()
                    .fg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD),
            ));
        }

        frame.render_widget(Paragraph::new(Line::from(tab_spans)), chunks[0]);
    }

    // Top Divider
    let divider = Paragraph::new(Line::from(vec![Span::styled(
        "─".repeat(inner_area.width as usize),
        Style::default().fg(theme.border),
    )]));
    frame.render_widget(divider, chunks[1]);

    // 2. Active Tab Content Area
    let mut content_lines = Vec::new();
    content_lines.push(Line::from(""));

    match state.active_tab {
        SettingsTab::Providers => {
            if let Some(ref prov) = state.selecting_model_for_provider {
                // MODEL DRILL-DOWN SUBVIEW
                content_lines.push(Line::from(vec![
                    Span::styled(
                        "  📋 Select Default Model for: ",
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
                        format!("  ({} models available)", state.provider_models.len()),
                        Style::default().fg(theme.muted),
                    ),
                ]));
                content_lines.push(Line::from(vec![
                    Span::raw("     "),
                    Span::styled(
                        "Choose a model to assign as default for this provider & switch active model.",
                        Style::default().fg(theme.muted),
                    ),
                ]));
                content_lines.push(Line::from(""));

                if state.provider_models.is_empty() {
                    content_lines.push(Line::from(vec![
                        Span::raw("    "),
                        Span::styled(
                            "⚠️ No models found or provider unconfigured. Press [Esc] to return to providers.",
                            Style::default().fg(theme.warning),
                        ),
                    ]));
                } else {
                    let max_visible = 8;
                    let scroll_offset =
                        compute_scroll_offset(state.model_selected_index, max_visible);
                    let visible_models = state
                        .provider_models
                        .iter()
                        .enumerate()
                        .skip(scroll_offset)
                        .take(max_visible);

                    let current_default = Config::static_default_model_for_provider(prov);

                    for (idx, model) in visible_models {
                        let is_selected = idx == state.model_selected_index;
                        let is_active_model =
                            model.id == state.active_model && *prov == state.active_provider;
                        let is_default = is_active_model || model.id == current_default;

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
                                format!("{:<34}", truncate_middle(&model.id, 34)),
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
                                "[🧠 Reasoning]",
                                Style::default().fg(theme.warning),
                            ));
                        }

                        if is_active_model {
                            spans.push(Span::raw(" "));
                            spans.push(Span::styled(
                                "[Active ✔]",
                                Style::default()
                                    .fg(theme.success)
                                    .add_modifier(Modifier::BOLD),
                            ));
                        } else if is_default {
                            spans.push(Span::raw(" "));
                            spans.push(Span::styled(
                                "[Default]",
                                Style::default().fg(theme.brand_accent),
                            ));
                        }

                        content_lines.push(Line::from(spans));
                    }
                }

                content_lines.push(Line::from(""));
                content_lines.push(Line::from(vec![
                    Span::raw("    "),
                    Span::styled(
                        "Press [Enter] to set as default  •  [Esc] or [Backspace] to return to providers",
                        Style::default().fg(theme.muted),
                    ),
                ]));
            } else {
                // TOP LEVEL PROVIDERS LIST
                let max_visible = 7;
                let scroll_offset = compute_scroll_offset(state.selected_index, max_visible);
                let visible_providers = SUPPORTED_PROVIDERS
                    .iter()
                    .enumerate()
                    .skip(scroll_offset)
                    .take(max_visible);

                for (idx, provider) in visible_providers {
                    let is_selected = idx == state.selected_index;
                    let is_active = *provider == state.active_provider;
                    let default_model = if is_active && !state.active_model.is_empty() {
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
                        Span::styled(" default: ", Style::default().fg(theme.muted)),
                        Span::styled(
                            format!("{:<30}", default_model),
                            Style::default().fg(theme.text_primary),
                        ),
                    ];

                    if is_active {
                        spans.push(Span::styled(
                            " [Active ✔]",
                            Style::default()
                                .fg(theme.success)
                                .add_modifier(Modifier::BOLD),
                        ));
                    }

                    content_lines.push(Line::from(spans));
                }
                content_lines.push(Line::from(""));
                content_lines.push(Line::from(vec![
                    Span::raw("    "),
                    Span::styled(
                        "Press [Enter] to browse & choose models  •  [Space] to activate provider immediately",
                        Style::default().fg(theme.muted),
                    ),
                ]));
            }
        }
        SettingsTab::Workspace => {
            // Path middle-truncation based on available width
            let path_avail_w = (inner_area.width as usize).saturating_sub(26).max(20);
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
            content_lines.push(Line::from(vec![
                Span::raw("    "),
                Span::styled(
                    "Press [Enter] or [Space] to toggle auto-approve or cycle budget/policy",
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
    }

    let content_p = Paragraph::new(content_lines);
    frame.render_widget(content_p, chunks[2]);

    // 3. Footer Key Hints (Context sensitive)
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
                "[Enter] ",
                Style::default()
                    .fg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "Set Default Model  ",
                Style::default().fg(theme.text_primary),
            ),
            Span::styled(
                "[Esc/Backspace] ",
                Style::default()
                    .fg(theme.muted)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("Back to Providers  ", Style::default().fg(theme.muted)),
            Span::styled("[Tab] ", Style::default().fg(theme.muted)),
            Span::styled("Next Tab", Style::default().fg(theme.muted)),
        ])
    } else {
        Line::from(vec![
            Span::styled(
                "  [Tab/◄►] ",
                Style::default()
                    .fg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("Switch Tab  ", Style::default().fg(theme.text_primary)),
            Span::styled(
                "[↑/↓] ",
                Style::default()
                    .fg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("Navigate  ", Style::default().fg(theme.text_primary)),
            Span::styled(
                "[Enter] ",
                Style::default()
                    .fg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("Choose/Toggle  ", Style::default().fg(theme.text_primary)),
            Span::styled(
                "[Space] ",
                Style::default()
                    .fg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("Activate/Toggle  ", Style::default().fg(theme.text_primary)),
            Span::styled("[Esc] ", Style::default().fg(theme.muted)),
            Span::styled("Save & Close", Style::default().fg(theme.muted)),
        ])
    };
    frame.render_widget(Paragraph::new(footer_line), chunks[3]);
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
        .title(" ⚙️ Permission Required ")
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
        "  ⚙️ minicode requests permission to modify configuration:",
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
        Span::styled(" [1] Allow Change ", allow_style),
        Span::raw("    "),
        Span::styled(" [2] Deny Change ", deny_style),
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

        // Wrap around
        state.next_tab();
        assert_eq!(state.active_tab, SettingsTab::Providers);

        // Prev tab
        state.prev_tab();
        assert_eq!(state.active_tab, SettingsTab::Probes);

        state.prev_tab();
        assert_eq!(state.active_tab, SettingsTab::Autonomy);
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
}

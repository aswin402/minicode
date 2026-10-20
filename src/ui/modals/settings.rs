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
    Fallbacks,
    MiniPubs,
    Autonomy,
    Workspace,
    Probes,
    Browser,
}

impl SettingsTab {
    /// Returns all tabs in order.
    pub fn all() -> &'static [SettingsTab] {
        &[
            SettingsTab::Providers,
            SettingsTab::Fallbacks,
            SettingsTab::MiniPubs,
            SettingsTab::Autonomy,
            SettingsTab::Workspace,
            SettingsTab::Probes,
            SettingsTab::Browser,
        ]
    }

    /// Tab display title.
    #[allow(dead_code)]
    pub fn title(&self) -> &'static str {
        match self {
            SettingsTab::Providers => "Providers",
            SettingsTab::Fallbacks => "Fallbacks",
            SettingsTab::MiniPubs => "MiniPubs",
            SettingsTab::Autonomy => "Autonomy",
            SettingsTab::Workspace => "Workspace",
            SettingsTab::Probes => "Probes",
            SettingsTab::Browser => "Browser",
        }
    }

    /// Sidebar menu label (minimal Go-style without emojis).
    pub fn sidebar_label(&self) -> &'static str {
        match self {
            SettingsTab::Providers => "Providers",
            SettingsTab::Fallbacks => "Fallbacks",
            SettingsTab::MiniPubs => "MiniPubs",
            SettingsTab::Autonomy => "Autonomy",
            SettingsTab::Workspace => "Workspace",
            SettingsTab::Probes => "Probes",
            SettingsTab::Browser => "Browser",
        }
    }

    /// Right-pane section title and subtitle.
    pub fn section_header(&self) -> (&'static str, &'static str) {
        match self {
            SettingsTab::Providers => (
                "Providers & Primary Model",
                "Select active provider and primary model across all 12 supported endpoints",
            ),
            SettingsTab::Fallbacks => (
                "Fallback Models & Failover Chain",
                "Configure 1 Primary + 2 Fallbacks + 1 Shared Global Last Fallback for automatic failover",
            ),
            SettingsTab::MiniPubs => (
                "MiniPub Subagents & Routing",
                "Configure runtime limits and per-specialist primary & fallback models across all 14 MiniPubs",
            ),
            SettingsTab::Autonomy => (
                "Autonomy & Execution Policy",
                "Configure tool approval policy, extended reasoning budget, and stream idle timeouts",
            ),
            SettingsTab::Workspace => (
                "Workspace & Persistence Scope",
                "Manage active project root and configuration persistence (.minicode vs ~/.config)",
            ),
            SettingsTab::Probes => (
                "Endpoint Connectivity & Latency",
                "Verify API credentials, endpoint reachability, and round-trip latency across providers",
            ),
            SettingsTab::Browser => (
                "Browser Automation Engine",
                "Configure Obscura (pure-Rust headless V8), Firefox, and Chrome engine priorities",
            ),
        }
    }

    /// Cycles to the next tab.
    pub fn next(&self) -> Self {
        match self {
            SettingsTab::Providers => SettingsTab::Fallbacks,
            SettingsTab::Fallbacks => SettingsTab::MiniPubs,
            SettingsTab::MiniPubs => SettingsTab::Autonomy,
            SettingsTab::Autonomy => SettingsTab::Workspace,
            SettingsTab::Workspace => SettingsTab::Probes,
            SettingsTab::Probes => SettingsTab::Browser,
            SettingsTab::Browser => SettingsTab::Providers,
        }
    }

    /// Cycles to the previous tab.
    pub fn prev(&self) -> Self {
        match self {
            SettingsTab::Providers => SettingsTab::Browser,
            SettingsTab::Fallbacks => SettingsTab::Providers,
            SettingsTab::MiniPubs => SettingsTab::Fallbacks,
            SettingsTab::Autonomy => SettingsTab::MiniPubs,
            SettingsTab::Workspace => SettingsTab::Autonomy,
            SettingsTab::Probes => SettingsTab::Workspace,
            SettingsTab::Browser => SettingsTab::Probes,
        }
    }
}

/// Available target scopes for 4-slot model & fallback routing in `/settings`:
/// - `"global"` configures the Orchestrator + default MiniPub chain in one place.
/// - Individual MiniPub role slugs configure custom `(provider, model)` slots 1-3 for that specialist while sharing Slot 4 (`Global Last Fallback`).
pub const ROUTING_TARGETS: &[(&str, &str)] = &[
    ("global", "Global (Orchestrator + All MiniPubs)"),
    ("scout", "scout (Codebase Explorer)"),
    ("architect", "architect (System Design)"),
    ("planner", "planner (Task Strategy)"),
    ("designer", "designer (UI/UX Specialist)"),
    ("frontend", "frontend (Web/UI Engineer)"),
    ("backend", "backend (API/DB/Systems)"),
    ("coder", "coder (Implementation)"),
    ("vision", "vision (Multimodal Image/UI)"),
    ("content_writer", "content_writer (Copy/Docs)"),
    ("debugger", "debugger (Root-Cause Analysis)"),
    ("reviewer", "reviewer (Quality & Correctness)"),
    ("security", "security (Vulnerability Audit)"),
    ("tester", "tester (QA & Verification)"),
    ("simplifier", "simplifier (Clean Refactoring)"),
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
    pub streaming: bool,
    pub caveman_mode: bool,
    pub caveman_level: String,
    pub effort_level: String,
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

    // Provider & Model selection drill-down sub-state:
    pub selecting_provider_for_target: bool,
    pub pending_slot: Option<u8>,
    pub return_selected_index: usize,
    pub selecting_model_for_provider: Option<String>,
    pub provider_models: Vec<crate::agent::models::ModelInfo>,
    pub model_selected_index: usize,

    // 4-Slot Model & Fallback Routing state (1 Primary + 2 Fallbacks + 1 Shared Global Last Fallback):
    pub routing_target_idx: usize,
    pub fallback_1: Option<crate::config::ModelSlot>,
    pub fallback_2: Option<crate::config::ModelSlot>,
    pub global_last_fallback: Option<crate::config::ModelSlot>,
    pub favorite_models: Vec<crate::config::ModelSlot>,
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
            streaming: config.agent.streaming,
            caveman_mode: config.agent.caveman_mode,
            caveman_level: config.agent.caveman_level.clone(),
            effort_level: config.provider.effective_level_label(),
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
            selecting_provider_for_target: false,
            pending_slot: None,
            return_selected_index: 0,
            selecting_model_for_provider: None,
            provider_models: Vec::new(),
            model_selected_index: 0,
            routing_target_idx: 0,
            fallback_1: config.provider.fallback_1.clone(),
            fallback_2: config.provider.fallback_2.clone(),
            global_last_fallback: config.provider.global_last_fallback.clone(),
            favorite_models: config.provider.favorite_models.clone(),
            minipub_sync_with_orchestrator: config.agent.minipub.sync_with_orchestrator,
            minipub_default_chain: config.agent.minipub.default_chain.clone(),
            minipub_role_models: config.agent.minipub.role_models.clone(),
        }
    }

    /// Synchronizes 4-slot routing fields from a mutated `Config`.
    pub fn sync_from_config(&mut self, config: &Config) {
        self.active_provider = config.provider.default.clone();
        self.active_model = config.provider.model.clone();
        self.streaming = config.agent.streaming;
        self.caveman_mode = config.agent.caveman_mode;
        self.caveman_level = config.agent.caveman_level.clone();
        self.effort_level = config.provider.effective_level_label();
        self.fallback_1 = config.provider.fallback_1.clone();
        self.fallback_2 = config.provider.fallback_2.clone();
        self.global_last_fallback = config.provider.global_last_fallback.clone();
        self.favorite_models = config.provider.favorite_models.clone();
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
    /// for a given `target_slug` (`"global"` or a MiniPub role slug), plus a boolean indicating
    /// whether the target has custom per-MiniPub overrides (`true`) or inherits from Global (`false`).
    pub fn effective_slots_for_slug(
        &self,
        target_slug: &str,
    ) -> ([Option<crate::config::ModelSlot>; 4], bool) {
        let orch_primary = if !self.active_provider.trim().is_empty() {
            let model = if !self.active_model.trim().is_empty() {
                self.active_model.clone()
            } else {
                Config::static_default_model_for_provider(&self.active_provider).to_string()
            };
            Some(crate::config::ModelSlot::new(&self.active_provider, model))
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
                        rc.primary.clone().filter(|s| s.is_valid()).or(orch_primary),
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

    /// Resolves the effective `[Primary, 1st Fallback, 2nd Fallback, Global Last Fallback]` slots
    /// for the currently active target in `SettingsModalState`.
    pub fn effective_slots_for_current_target(
        &self,
    ) -> ([Option<crate::config::ModelSlot>; 4], bool) {
        let (target_slug, _) = self.current_routing_target();
        self.effective_slots_for_slug(target_slug)
    }

    /// Resets any active provider/model drill-down state.
    pub fn clear_drilldown(&mut self) {
        self.selecting_model_for_provider = None;
        self.provider_models.clear();
        self.model_selected_index = 0;
        if self.selecting_provider_for_target {
            self.selecting_provider_for_target = false;
            self.pending_slot = None;
            self.selected_index = self.return_selected_index;
        }
    }

    /// Switches to next tab and resets selection index and model drilldown.
    pub fn next_tab(&mut self) {
        self.selecting_provider_for_target = false;
        self.pending_slot = None;
        self.selecting_model_for_provider = None;
        self.provider_models.clear();
        self.model_selected_index = 0;
        self.active_tab = self.active_tab.next();
        self.selected_index = 0;
        if matches!(
            self.active_tab,
            SettingsTab::Providers | SettingsTab::Fallbacks
        ) {
            self.routing_target_idx = 0;
        }
    }

    /// Switches to previous tab and resets selection index and model drilldown.
    pub fn prev_tab(&mut self) {
        self.selecting_provider_for_target = false;
        self.pending_slot = None;
        self.selecting_model_for_provider = None;
        self.provider_models.clear();
        self.model_selected_index = 0;
        self.active_tab = self.active_tab.prev();
        self.selected_index = 0;
        if matches!(
            self.active_tab,
            SettingsTab::Providers | SettingsTab::Fallbacks
        ) {
            self.routing_target_idx = 0;
        }
    }

    /// Returns the maximum selectable items for the active tab (or model/provider list).
    pub fn max_items(&self) -> usize {
        if self.selecting_model_for_provider.is_some() {
            return self.provider_models.len();
        }
        if self.selecting_provider_for_target {
            return SUPPORTED_PROVIDERS.len();
        }
        match self.active_tab {
            SettingsTab::Providers => SUPPORTED_PROVIDERS.len(),
            SettingsTab::Fallbacks => 5, // 4 slots + Sync All MiniPubs action
            SettingsTab::MiniPubs => 4 + (ROUTING_TARGETS.len().saturating_sub(1)), // 4 runtime settings + 14 MiniPubs = 18
            SettingsTab::Autonomy => 8, // Streaming, Caveman Mode, Caveman Level, Effort level, Auto-approve, Thinking budget, Approval policy, Stream idle timeout
            SettingsTab::Workspace => 3,
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

/// Helper to render the shared Provider & Model Picker sub-view when `Providers` is active
/// or when drilling into a slot/MiniPub from `Fallbacks` or `MiniPubs`.
fn render_provider_and_model_picker(
    content_lines: &mut Vec<Line<'static>>,
    right_area: Rect,
    theme: &Theme,
    state: &SettingsModalState,
    list_max_visible: usize,
) {
    let (_, target_label) = state.current_routing_target();
    let (slots, _) = state.effective_slots_for_current_target();

    if state.selecting_provider_for_target {
        let slot_desc = match state.pending_slot {
            Some(1) => "Slot 1 (Primary)",
            Some(2) => "Slot 2 (1st Fallback)",
            Some(3) => "Slot 3 (2nd Fallback)",
            Some(4) => "Slot 4 (Global Last Fallback)",
            _ => "Slots 1-4",
        };
        content_lines.push(Line::from(vec![
            Span::styled("  Target : ", Style::default().fg(theme.muted)),
            Span::styled(
                target_label.to_string(),
                Style::default()
                    .fg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("  ·  Assigning: ", Style::default().fg(theme.muted)),
            Span::styled(
                slot_desc.to_string(),
                Style::default()
                    .fg(theme.text_primary)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("  ([Esc] Back)", Style::default().fg(theme.muted)),
        ]));
        content_lines.push(Line::from(""));
    }

    if let Some(ref prov) = state.selecting_model_for_provider {
        // MODEL DRILL-DOWN SUBVIEW
        content_lines.push(Line::from(vec![
            Span::styled(
                "  Select Model from ",
                Style::default().fg(theme.text_primary),
            ),
            Span::styled(
                prov.clone(),
                Style::default()
                    .fg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(
                    " ({} available)  —  [Enter/1] Primary  [2] 1st FB  [3] 2nd FB  [4] Global Last  [f] Favorite",
                    state.provider_models.len()
                ),
                Style::default().fg(theme.muted),
            ),
        ]));
        content_lines.push(Line::from(""));

        if state.provider_models.is_empty() {
            content_lines.push(Line::from(vec![
                Span::raw("    "),
                Span::styled(
                    "No models found or provider unconfigured. Press [Esc] to return.",
                    Style::default().fg(theme.warning),
                ),
            ]));
        } else {
            let max_visible = list_max_visible;
            let scroll_offset = compute_scroll_offset(state.model_selected_index, max_visible);
            let visible_models = state
                .provider_models
                .iter()
                .enumerate()
                .skip(scroll_offset)
                .take(max_visible);

            let current_default = Config::static_default_model_for_provider(prov);
            let model_col_w = ((right_area.width as usize).saturating_sub(36)).clamp(28, 52);

            for (idx, model) in visible_models {
                let is_selected = idx == state.model_selected_index;
                let matches_slot = |s_opt: &Option<crate::config::ModelSlot>| -> bool {
                    s_opt
                        .as_ref()
                        .map(|s| s.provider.eq_ignore_ascii_case(prov) && s.model == model.id)
                        .unwrap_or(false)
                };
                let is_s1 = matches_slot(&slots[0]);
                let is_s2 = matches_slot(&slots[1]);
                let is_s3 = matches_slot(&slots[2]);
                let is_s4 = matches_slot(&slots[3]);
                let is_fav = state.favorite_models.iter().any(|s| {
                    s.provider.eq_ignore_ascii_case(prov) && s.model.eq_ignore_ascii_case(&model.id)
                });
                let is_default = is_s1 || model.id == current_default;

                let cursor = if is_selected { "  › " } else { "    " };
                let model_style = if is_selected {
                    Style::default()
                        .fg(theme.brand_accent)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(theme.text_primary)
                };

                let resolved_ctx = model
                    .context_length
                    .or_else(|| crate::agent::models::lookup_models_dev_context(&model.id))
                    .or_else(|| crate::agent::models::lookup_cached_model_context(&model.id))
                    .or_else(|| crate::agent::models::parse_context_window_from_name(&model.id));
                let ctx_str = format_context_tokens(resolved_ctx);
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
                    Span::raw(" "),
                    Span::styled(
                        format!("{:>6} ctx", ctx_str),
                        Style::default().fg(theme.muted),
                    ),
                ];

                if is_fav {
                    spans.push(Span::raw(" "));
                    spans.push(Span::styled(
                        "[fav]",
                        Style::default()
                            .fg(theme.warning)
                            .add_modifier(Modifier::BOLD),
                    ));
                }

                if model.is_free {
                    spans.push(Span::raw(" "));
                    spans.push(Span::styled("free", Style::default().fg(theme.success)));
                }

                if reasoning {
                    spans.push(Span::raw(" "));
                    spans.push(Span::styled("reasoning", Style::default().fg(theme.muted)));
                }

                if is_s1 {
                    spans.push(Span::raw("  "));
                    spans.push(Span::styled(
                        "[primary]",
                        Style::default()
                            .fg(theme.success)
                            .add_modifier(Modifier::BOLD),
                    ));
                }
                if is_s2 {
                    spans.push(Span::raw("  "));
                    spans.push(Span::styled(
                        "[1st-fb]",
                        Style::default().fg(theme.brand_accent),
                    ));
                }
                if is_s3 {
                    spans.push(Span::raw("  "));
                    spans.push(Span::styled("[2nd-fb]", Style::default().fg(theme.info)));
                }
                if is_s4 {
                    spans.push(Span::raw("  "));
                    spans.push(Span::styled(
                        "[global-last]",
                        Style::default()
                            .fg(theme.warning)
                            .add_modifier(Modifier::BOLD),
                    ));
                } else if !is_s1 && !is_s2 && !is_s3 && is_default {
                    spans.push(Span::raw("  "));
                    spans.push(Span::styled("default", Style::default().fg(theme.muted)));
                }

                content_lines.push(Line::from(spans));
            }
        }
    } else {
        // TOP LEVEL PROVIDERS LIST
        content_lines.push(Line::from(vec![Span::styled(
            format!(
                "    {:<15} {:<38} {}",
                "PROVIDER", "DEFAULT / ACTIVE MODEL", "ASSIGNED ROLE"
            ),
            Style::default()
                .fg(theme.muted)
                .add_modifier(Modifier::BOLD),
        )]));
        content_lines.push(Line::from(vec![
            Span::raw("    "),
            Span::styled(
                "─".repeat((right_area.width as usize).saturating_sub(8).max(20)),
                Style::default().fg(theme.border),
            ),
        ]));

        let max_visible = list_max_visible;
        let scroll_offset = compute_scroll_offset(state.selected_index, max_visible);
        let visible_providers = SUPPORTED_PROVIDERS
            .iter()
            .enumerate()
            .skip(scroll_offset)
            .take(max_visible);
        let model_col_w = 38usize;

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

            let cursor = if is_selected { "  › " } else { "    " };
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
                Span::styled(format!("{:<15} ", provider), prov_style),
                Span::styled(
                    format!(
                        "{:<width$} ",
                        truncate_middle(default_model, model_col_w),
                        width = model_col_w
                    ),
                    if is_selected {
                        Style::default().fg(theme.text_primary)
                    } else {
                        Style::default().fg(theme.muted)
                    },
                ),
            ];

            if is_s1 {
                spans.push(Span::styled(
                    "[primary] ",
                    Style::default()
                        .fg(theme.success)
                        .add_modifier(Modifier::BOLD),
                ));
            }
            if is_s2 {
                spans.push(Span::styled(
                    "[1st-fb] ",
                    Style::default().fg(theme.brand_accent),
                ));
            }
            if is_s3 {
                spans.push(Span::styled("[2nd-fb] ", Style::default().fg(theme.info)));
            }
            if is_s4 {
                spans.push(Span::styled(
                    "[global-last]",
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
                "[Enter] Browse Models  ·  [Space/1] Set Primary  ·  [2/3/4] Set Fallback Slot",
                Style::default().fg(theme.muted),
            ),
        ]));
    }
}

/// Renders the `/settings` configuration dialog as a minimal Go / Charmbracelet-style
/// Full-Screen Workbench with a Left Menu Bar and a Right Main Panel.
pub fn render_settings(frame: &mut Frame, area: Rect, theme: &Theme, state: &SettingsModalState) {
    let popup_area = area;
    frame.render_widget(Clear, popup_area);

    let scope_badge = if state.save_to_workspace {
        "workspace (.minicode)"
    } else {
        "global (~/.config)"
    };

    let block = Block::default()
        .title(Line::from(vec![
            Span::styled(
                " MiniCode Settings ",
                Style::default()
                    .fg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(
                    "── {} / {} ── [{}] ",
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
        .border_style(Style::default().fg(theme.border).bg(theme.bg_elevated))
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
        24
    } else if body_area.width >= 68 {
        20
    } else {
        16.min(body_area.width / 3)
    };

    let main_split = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(sidebar_w), // 0: Left Menu Bar (Minimal Go-style Navigation)
            Constraint::Length(1),         // 1: Vertical Separator │
            Constraint::Min(20),           // 2: Right Full Settings Panel
        ])
        .split(body_area);

    let sidebar_area = main_split[0];
    let sep_area = main_split[1];
    let right_area = main_split[2];

    // 1. Render Left Menu Bar (Minimal Go/Huh style: › active item, clean summary below)
    let mut sidebar_lines: Vec<Line> = Vec::new();
    sidebar_lines.push(Line::from(""));
    sidebar_lines.push(Line::from(vec![Span::styled(
        "  SETTINGS",
        Style::default()
            .fg(theme.muted)
            .add_modifier(Modifier::BOLD),
    )]));
    sidebar_lines.push(Line::from(""));

    let sb_inner_w = (sidebar_area.width as usize).saturating_sub(2).max(8);
    for tab in SettingsTab::all() {
        let is_active = *tab == state.active_tab;
        let raw_label = tab.sidebar_label();
        let prefix = if is_active { "  › " } else { "    " };
        let style = if is_active {
            Style::default()
                .fg(theme.brand_accent)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.muted)
        };
        sidebar_lines.push(Line::from(vec![
            Span::styled(prefix, style),
            Span::styled(raw_label, style),
        ]));
    }

    // Add minimal Active Chain summary card in Left Sidebar if height permits
    if body_area.height >= 19 && sidebar_area.width >= 18 {
        let (slots, _) = state.effective_slots_for_slug("global");
        let slot_max_w = (sidebar_area.width as usize).saturating_sub(9).max(7);
        let fmt_sb_slot = |slot_opt: &Option<crate::config::ModelSlot>| -> String {
            match slot_opt {
                Some(s) if s.is_valid() => truncate_middle(&s.model, slot_max_w),
                _ => "-".to_string(),
            }
        };

        sidebar_lines.push(Line::from(""));
        sidebar_lines.push(Line::from(vec![Span::styled(
            format!("  {}", "─".repeat(sb_inner_w.saturating_sub(2))),
            Style::default().fg(theme.border),
        )]));
        sidebar_lines.push(Line::from(vec![Span::styled(
            "  ACTIVE CHAIN",
            Style::default()
                .fg(theme.muted)
                .add_modifier(Modifier::BOLD),
        )]));
        sidebar_lines.push(Line::from(vec![
            Span::styled("  1. pri ", Style::default().fg(theme.muted)),
            Span::styled(
                fmt_sb_slot(&slots[0]),
                Style::default().fg(theme.text_primary),
            ),
        ]));
        sidebar_lines.push(Line::from(vec![
            Span::styled("  2. fb1 ", Style::default().fg(theme.muted)),
            Span::styled(fmt_sb_slot(&slots[1]), Style::default().fg(theme.muted)),
        ]));
        sidebar_lines.push(Line::from(vec![
            Span::styled("  3. fb2 ", Style::default().fg(theme.muted)),
            Span::styled(fmt_sb_slot(&slots[2]), Style::default().fg(theme.muted)),
        ]));
        sidebar_lines.push(Line::from(vec![
            Span::styled("  4. glb ", Style::default().fg(theme.muted)),
            Span::styled(fmt_sb_slot(&slots[3]), Style::default().fg(theme.warning)),
        ]));
    }

    frame.render_widget(Paragraph::new(sidebar_lines), sidebar_area);

    // 2. Render Vertical Separator │ between Left Menu Bar and Right Settings Panel
    let sep_lines: Vec<Line> = (0..sep_area.height)
        .map(|_| Line::from(vec![Span::styled("│", Style::default().fg(theme.border))]))
        .collect();
    frame.render_widget(Paragraph::new(sep_lines), sep_area);

    // 3. Render Right Main Settings Panel for the Active Tab
    let mut content_lines: Vec<Line<'static>> = Vec::new();
    let (sec_title, sec_subtitle) = state.active_tab.section_header();
    content_lines.push(Line::from(""));
    content_lines.push(Line::from(vec![
        Span::raw("  "),
        Span::styled(
            sec_title,
            Style::default()
                .fg(theme.text_primary)
                .add_modifier(Modifier::BOLD),
        ),
    ]));
    content_lines.push(Line::from(vec![
        Span::raw("  "),
        Span::styled(
            truncate_middle(
                sec_subtitle,
                (right_area.width as usize).saturating_sub(4).max(20),
            ),
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

    let list_max_visible = (right_area.height as usize).saturating_sub(11).max(6);

    // If user is currently in provider/model picker drill-down (from Providers, Fallbacks, or MiniPubs):
    if state.active_tab == SettingsTab::Providers
        || state.selecting_provider_for_target
        || state.selecting_model_for_provider.is_some()
    {
        render_provider_and_model_picker(
            &mut content_lines,
            right_area,
            theme,
            state,
            list_max_visible,
        );
    } else {
        match state.active_tab {
            SettingsTab::Providers => {}
            SettingsTab::Fallbacks => {
                let (slots, _) = state.effective_slots_for_slug("global");
                let custom_count = state
                    .minipub_role_models
                    .values()
                    .filter(|c| !c.is_empty())
                    .count();

                let slot_defs = [
                    (
                        0usize,
                        "1. Primary Model",
                        &slots[0],
                        "Default active orchestrator & subagent model",
                    ),
                    (
                        1usize,
                        "2. 1st Fallback",
                        &slots[1],
                        "Immediate failover on rate limit (429) or timeout",
                    ),
                    (
                        2usize,
                        "3. 2nd Fallback",
                        &slots[2],
                        "Secondary failover if 1st fallback is unavailable",
                    ),
                    (
                        3usize,
                        "4. Global Last Fallback",
                        &slots[3],
                        "Shared emergency fallback across Orchestrator & all 14 MiniPubs",
                    ),
                ];

                content_lines.push(Line::from(vec![Span::styled(
                    format!(
                        "    {:<26} {:<34} {}",
                        "FAILOVER SLOT", "PROVIDER / MODEL", "BEHAVIOR"
                    ),
                    Style::default()
                        .fg(theme.muted)
                        .add_modifier(Modifier::BOLD),
                )]));
                content_lines.push(Line::from(vec![
                    Span::raw("    "),
                    Span::styled(
                        "─".repeat((right_area.width as usize).saturating_sub(8).max(20)),
                        Style::default().fg(theme.border),
                    ),
                ]));

                for (idx, label, slot_opt, desc) in slot_defs {
                    let is_sel = state.selected_index == idx;
                    let cursor = if is_sel { "  › " } else { "    " };
                    let val_str = match slot_opt {
                        Some(s) if s.is_valid() => truncate_middle(&s.display_short(), 32),
                        _ => "- (not set)".to_string(),
                    };
                    let val_style = if slot_opt.as_ref().map(|s| s.is_valid()).unwrap_or(false) {
                        if idx == 0 {
                            Style::default()
                                .fg(theme.success)
                                .add_modifier(Modifier::BOLD)
                        } else if idx == 3 {
                            Style::default()
                                .fg(theme.warning)
                                .add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().fg(theme.brand_accent)
                        }
                    } else {
                        Style::default().fg(theme.muted)
                    };

                    content_lines.push(Line::from(vec![
                        Span::styled(
                            cursor,
                            Style::default()
                                .fg(theme.brand_accent)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            format!("{:<26} ", label),
                            if is_sel {
                                Style::default()
                                    .fg(theme.brand_accent)
                                    .add_modifier(Modifier::BOLD)
                            } else {
                                Style::default().fg(theme.text_primary)
                            },
                        ),
                        Span::styled(format!("{:<34} ", val_str), val_style),
                        Span::styled(desc, Style::default().fg(theme.muted)),
                    ]));
                    content_lines.push(Line::from(""));
                }

                // Item 4: Sync All MiniPubs to Global Chain
                let is_sync_sel = state.selected_index == 4;
                let sync_status = if custom_count == 0 && state.minipub_sync_with_orchestrator {
                    "[all 14 MiniPubs inheriting]"
                } else {
                    "[press Enter to reset custom MiniPub overrides]"
                };
                content_lines.push(Line::from(vec![
                    Span::styled(
                        if is_sync_sel { "  › " } else { "    " },
                        Style::default()
                            .fg(theme.brand_accent)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!("{:<26} ", "Sync All 14 MiniPubs"),
                        if is_sync_sel {
                            Style::default()
                                .fg(theme.brand_accent)
                                .add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().fg(theme.text_primary)
                        },
                    ),
                    Span::styled(
                        format!("{:<34} ", sync_status),
                        if custom_count == 0 {
                            Style::default().fg(theme.success)
                        } else {
                            Style::default().fg(theme.warning)
                        },
                    ),
                    Span::styled(
                        "Propagate this 4-slot chain to all subagents",
                        Style::default().fg(theme.muted),
                    ),
                ]));

                content_lines.push(Line::from(""));
                content_lines.push(Line::from(vec![
                    Span::raw("    "),
                    Span::styled(
                        "[Enter] Choose Provider & Model for Slot  ·  [c/Del] Clear Fallback Slot  ·  [a] Sync All MiniPubs",
                        Style::default().fg(theme.muted),
                    ),
                ]));
            }
            SettingsTab::MiniPubs => {
                // Top section: 4 MiniPub Runtime Settings (indices 0..4)
                let sync_badge = if state.minipub_sync_with_orchestrator {
                    "[on]  Inherit Orchestrator chain by default"
                } else {
                    "[off] Custom default subagent chain"
                };
                let runtime_rows: [(usize, &str, String, &str); 4] = [
                    (
                        0,
                        "Max Parallel Workers",
                        format!("{} workers", state.minipub_max_parallel),
                        "Max concurrent in-process MiniPub subagents",
                    ),
                    (
                        1,
                        "Inactivity Timeout",
                        format!("{}s", state.minipub_inactivity_timeout_secs),
                        "Auto-terminates stalled subagent if silent",
                    ),
                    (
                        2,
                        "Worktree Isolation",
                        state.minipub_worktree_isolation.clone(),
                        "auto (leases first) | always (worktree) | never",
                    ),
                    (
                        3,
                        "Sync All w/ Orchestrator",
                        sync_badge.to_string(),
                        "Press [a] or [Enter] to sync all 14 MiniPubs to Global chain",
                    ),
                ];

                for (idx, label, val, desc) in runtime_rows {
                    let is_sel = state.selected_index == idx;
                    content_lines.push(Line::from(vec![
                        Span::styled(
                            if is_sel { "  › " } else { "    " },
                            Style::default()
                                .fg(theme.brand_accent)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            format!("{:<24} ", label),
                            if is_sel {
                                Style::default()
                                    .fg(theme.brand_accent)
                                    .add_modifier(Modifier::BOLD)
                            } else {
                                Style::default().fg(theme.text_primary)
                            },
                        ),
                        Span::styled(
                            format!("{:<26} ", truncate_middle(&val, 25)),
                            Style::default()
                                .fg(theme.brand_accent)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(desc, Style::default().fg(theme.muted)),
                    ]));
                }

                content_lines.push(Line::from(""));
                content_lines.push(Line::from(vec![Span::styled(
                    format!(
                        "    {:<16} {:<10} {:<28} {}",
                        "MINIPUB AGENT",
                        "MODE",
                        "PRIMARY MODEL",
                        "FALLBACKS (1st -> 2nd -> Global)"
                    ),
                    Style::default()
                        .fg(theme.muted)
                        .add_modifier(Modifier::BOLD),
                )]));
                content_lines.push(Line::from(vec![
                    Span::raw("    "),
                    Span::styled(
                        "─".repeat((right_area.width as usize).saturating_sub(8).max(20)),
                        Style::default().fg(theme.border),
                    ),
                ]));

                let minipub_targets = &ROUTING_TARGETS[1..];
                let table_max_vis = (right_area.height as usize).saturating_sub(15).max(5);
                let mp_cursor = state.selected_index.saturating_sub(4);
                let scroll_offset = if state.selected_index >= 4 {
                    compute_scroll_offset(mp_cursor, table_max_vis)
                } else {
                    0
                };

                for (mp_idx, (slug, _label)) in minipub_targets
                    .iter()
                    .enumerate()
                    .skip(scroll_offset)
                    .take(table_max_vis)
                {
                    let row_idx = 4 + mp_idx;
                    let is_sel = state.selected_index == row_idx;
                    let (slots, is_custom) = state.effective_slots_for_slug(slug);

                    let pri_str = slots[0]
                        .as_ref()
                        .map(|s| truncate_middle(&s.display_short(), 26))
                        .unwrap_or_else(|| "-".to_string());
                    let fb1_str = slots[1]
                        .as_ref()
                        .map(|s| truncate_middle(&s.model, 14))
                        .unwrap_or_else(|| "-".to_string());
                    let fb2_str = slots[2]
                        .as_ref()
                        .map(|s| truncate_middle(&s.model, 14))
                        .unwrap_or_else(|| "-".to_string());
                    let glb_str = slots[3]
                        .as_ref()
                        .map(|s| truncate_middle(&s.model, 14))
                        .unwrap_or_else(|| "-".to_string());

                    let mode_str = if is_custom { "custom " } else { "inherit" };
                    let mode_style = if is_custom {
                        Style::default()
                            .fg(theme.warning)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(theme.muted)
                    };

                    content_lines.push(Line::from(vec![
                        Span::styled(
                            if is_sel { "  › " } else { "    " },
                            Style::default()
                                .fg(theme.brand_accent)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            format!("{:<16} ", slug),
                            if is_sel {
                                Style::default()
                                    .fg(theme.brand_accent)
                                    .add_modifier(Modifier::BOLD)
                            } else {
                                Style::default().fg(theme.text_primary)
                            },
                        ),
                        Span::styled(format!("{:<10} ", mode_str), mode_style),
                        Span::styled(
                            format!("{:<28} ", pri_str),
                            Style::default().fg(theme.text_primary),
                        ),
                        Span::styled(
                            format!("{} -> {} -> {}", fb1_str, fb2_str, glb_str),
                            Style::default().fg(theme.muted),
                        ),
                    ]));
                }

                content_lines.push(Line::from(""));
                content_lines.push(Line::from(vec![
                    Span::raw("    "),
                    Span::styled(
                        "[Enter] Configure MiniPub Model & Fallbacks  ·  [c/Del] Reset to Inherit  ·  [a] Sync All",
                        Style::default().fg(theme.muted),
                    ),
                ]));
            }
            SettingsTab::Autonomy => {
                let streaming_badge = if state.streaming {
                    "enabled"
                } else {
                    "disabled"
                };
                let caveman_badge = if state.caveman_mode {
                    "enabled"
                } else {
                    "disabled"
                };
                let auto_appr_badge = if state.auto_approve {
                    "enabled"
                } else {
                    "disabled"
                };
                let budget_label = if state.thinking_budget == 0 {
                    "disabled".to_string()
                } else {
                    format!("{} tokens", state.thinking_budget)
                };

                let rows: [(usize, &str, String, &str); 8] = [
                    (
                        0,
                        "Streaming",
                        streaming_badge.to_string(),
                        "Real-time token streaming (enabled | disabled)",
                    ),
                    (
                        1,
                        "Caveman Mode",
                        caveman_badge.to_string(),
                        "Autonomous AST & prose token compression (enabled | disabled)",
                    ),
                    (
                        2,
                        "Caveman Level",
                        state.caveman_level.clone(),
                        "Compression intensity level (lite | full | ultra)",
                    ),
                    (
                        3,
                        "Effort / Temp Level",
                        state.effort_level.clone(),
                        "Unified model temperature & reasoning effort (low | medium | high)",
                    ),
                    (
                        4,
                        "Auto-Approve Tools",
                        auto_appr_badge.to_string(),
                        "Bypass manual confirmation prompts for tool calls",
                    ),
                    (
                        5,
                        "Thinking Budget",
                        budget_label,
                        "Extended reasoning token budget (disabled or scaled by model limit)",
                    ),
                    (
                        6,
                        "Approval Policy",
                        state.approval_policy.clone(),
                        "strict (confirm all) | prompt (mutate only) | permissive",
                    ),
                    (
                        7,
                        "Stream Idle Timeout",
                        format!("{}s", state.stream_idle_timeout_secs),
                        "Adaptive SSE inter-chunk idle timeout for LLM streaming",
                    ),
                ];

                for (idx, label, val, desc) in rows {
                    let is_sel = state.selected_index == idx;
                    content_lines.push(Line::from(vec![
                        Span::styled(
                            if is_sel { "  › " } else { "    " },
                            Style::default()
                                .fg(theme.brand_accent)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            format!("{:<24} ", label),
                            if is_sel {
                                Style::default()
                                    .fg(theme.brand_accent)
                                    .add_modifier(Modifier::BOLD)
                            } else {
                                Style::default().fg(theme.text_primary)
                            },
                        ),
                        Span::styled(
                            format!("{:<34} ", val),
                            Style::default()
                                .fg(theme.brand_accent)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(desc, Style::default().fg(theme.muted)),
                    ]));
                    content_lines.push(Line::from(""));
                }

                content_lines.push(Line::from(vec![
                    Span::raw("    "),
                    Span::styled(
                        "Press [Enter] or [Space] to toggle or cycle the selected option",
                        Style::default().fg(theme.muted),
                    ),
                ]));
            }
            SettingsTab::Workspace => {
                let path_avail_w = (right_area.width as usize).saturating_sub(32).max(20);
                let display_path = truncate_middle(&state.workspace_path, path_avail_w);

                // Item 0: Root path
                content_lines.push(Line::from(vec![
                    Span::styled(
                        if state.selected_index == 0 {
                            "  › "
                        } else {
                            "    "
                        },
                        Style::default()
                            .fg(theme.brand_accent)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!("{:<24} ", "Workspace Root"),
                        Style::default()
                            .fg(theme.text_primary)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(display_path, Style::default().fg(theme.text_primary)),
                ]));
                content_lines.push(Line::from(""));

                // Item 1: Active provider and model
                content_lines.push(Line::from(vec![
                    Span::styled(
                        if state.selected_index == 1 {
                            "  › "
                        } else {
                            "    "
                        },
                        Style::default()
                            .fg(theme.brand_accent)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!("{:<24} ", "Active Model"),
                        Style::default()
                            .fg(theme.text_primary)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!("{} / {}", state.active_provider, state.active_model),
                        Style::default().fg(theme.brand_accent),
                    ),
                ]));
                content_lines.push(Line::from(""));

                // Item 2: Scope preference
                let is_scope_selected = state.selected_index == 2;
                content_lines.push(Line::from(vec![
                    Span::styled(
                        if is_scope_selected { "  › " } else { "    " },
                        Style::default()
                            .fg(theme.brand_accent)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        "Config Persistence Scope",
                        if is_scope_selected {
                            Style::default()
                                .fg(theme.brand_accent)
                                .add_modifier(Modifier::BOLD)
                        } else {
                            Style::default()
                                .fg(theme.text_primary)
                                .add_modifier(Modifier::BOLD)
                        },
                    ),
                ]));

                let is_ws = state.save_to_workspace;
                content_lines.push(Line::from(""));
                content_lines.push(Line::from(vec![
                    Span::raw("      "),
                    Span::styled(
                        if is_ws {
                            "(•) Workspace (.minicode/config.toml)"
                        } else {
                            "( ) Workspace (.minicode/config.toml)"
                        },
                        if is_ws {
                            Style::default()
                                .fg(theme.success)
                                .add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().fg(theme.muted)
                        },
                    ),
                    Span::styled("  — Project isolated", Style::default().fg(theme.muted)),
                ]));
                content_lines.push(Line::from(vec![
                    Span::raw("      "),
                    Span::styled(
                        if !is_ws {
                            "(•) Global (~/.config/minicode/config.toml)"
                        } else {
                            "( ) Global (~/.config/minicode/config.toml)"
                        },
                        if !is_ws {
                            Style::default()
                                .fg(theme.brand_accent)
                                .add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().fg(theme.muted)
                        },
                    ),
                    Span::styled(
                        "  — Shared user default across repositories",
                        Style::default().fg(theme.muted),
                    ),
                ]));

                content_lines.push(Line::from(""));
                content_lines.push(Line::from(vec![
                    Span::raw("    "),
                    Span::styled(
                        "Press [Enter] or [Space] to toggle persistence scope",
                        Style::default().fg(theme.muted),
                    ),
                ]));
            }
            SettingsTab::Probes => {
                content_lines.push(Line::from(vec![
                    Span::styled(
                        "  › ",
                        Style::default()
                            .fg(theme.brand_accent)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        "[ Run Endpoint Connectivity & Latency Probe ]",
                        Style::default()
                            .fg(theme.brand_accent)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        "  — Press [Enter] or [Space] to test all providers",
                        Style::default().fg(theme.muted),
                    ),
                ]));
                content_lines.push(Line::from(""));

                if state.probing {
                    content_lines.push(Line::from(vec![
                        Span::raw("    "),
                        Span::styled(
                            "Probing provider endpoints and verifying credentials...",
                            Style::default().fg(theme.warning),
                        ),
                    ]));
                } else if let Some(ref results) = state.probe_results {
                    for chunk in results.chunks(2) {
                        let mut line_spans = vec![Span::raw("    ")];
                        for (i, r) in chunk.iter().enumerate() {
                            if i > 0 {
                                line_spans.push(Span::raw("      "));
                            }
                            let status_badge = match r.status.as_str() {
                                "connected" => Span::styled(
                                    "[ok]  ",
                                    Style::default()
                                        .fg(theme.success)
                                        .add_modifier(Modifier::BOLD),
                                ),
                                "disconnected" => Span::styled(
                                    "[err] ",
                                    Style::default()
                                        .fg(theme.destructive)
                                        .add_modifier(Modifier::BOLD),
                                ),
                                _ => Span::styled("[--]  ", Style::default().fg(theme.warning)),
                            };
                            line_spans.push(status_badge);
                            line_spans.push(Span::styled(
                                format!("{:<14}", r.provider),
                                Style::default().fg(theme.text_primary),
                            ));
                            line_spans.push(Span::styled(
                                format!("{:>5}ms", r.latency_ms),
                                Style::default().fg(theme.muted),
                            ));
                        }
                        content_lines.push(Line::from(line_spans));
                    }
                } else {
                    content_lines.push(Line::from(vec![
                        Span::raw("    "),
                        Span::styled(
                            "No probe results yet. Press [Enter] to test configured endpoints.",
                            Style::default().fg(theme.muted),
                        ),
                    ]));
                }
            }
            SettingsTab::Browser => {
                let def_engine_str = match state.browser_default_engine.as_deref() {
                    Some("obscura") => "obscura (Pure-Rust V8, ~30MB)",
                    Some("firefox") => "firefox (Gecko)",
                    Some("chrome") => "chrome (Chromium Blink)",
                    Some(other) => other,
                    None => "auto (Adaptive Priority)",
                };
                let hl_chain = state.browser_headless_priority.join(" -> ");
                let gui_chain = state.browser_gui_priority.join(" -> ");

                let rows: [(usize, &str, String, &str); 4] = [
                    (
                        0,
                        "Default Engine",
                        def_engine_str.to_string(),
                        "Auto adapts, or forces a specific browser binary",
                    ),
                    (
                        1,
                        "Headless Priority",
                        hl_chain,
                        "1st priority: obscura (~30MB RAM, instant start)",
                    ),
                    (
                        2,
                        "GUI Headed Priority",
                        gui_chain,
                        "Order used when launching visible browser windows",
                    ),
                    (
                        3,
                        "Reset Browser Defaults",
                        "[reset]".to_string(),
                        "Restore default engine priority chains",
                    ),
                ];

                for (idx, label, val, desc) in rows {
                    let is_sel = state.selected_index == idx;
                    content_lines.push(Line::from(vec![
                        Span::styled(
                            if is_sel { "  › " } else { "    " },
                            Style::default()
                                .fg(theme.brand_accent)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            format!("{:<24} ", label),
                            if is_sel {
                                Style::default()
                                    .fg(theme.brand_accent)
                                    .add_modifier(Modifier::BOLD)
                            } else {
                                Style::default().fg(theme.text_primary)
                            },
                        ),
                        Span::styled(
                            format!("{:<32} ", val),
                            Style::default()
                                .fg(theme.brand_accent)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(desc, Style::default().fg(theme.muted)),
                    ]));
                    content_lines.push(Line::from(""));
                }

                content_lines.push(Line::from(vec![
                    Span::raw("    "),
                    Span::styled(
                        "Press [Enter] or [Space] to cycle engine or rotate priority chains",
                        Style::default().fg(theme.muted),
                    ),
                ]));
            }
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

    // 5. Minimal Go-style Footer Key Hints
    let footer_line =
        if state.selecting_model_for_provider.is_some() || state.selecting_provider_for_target {
            Line::from(vec![
                Span::styled("  ↑/↓ ", Style::default().fg(theme.brand_accent)),
                Span::styled("navigate   ", Style::default().fg(theme.muted)),
                Span::styled("enter/1 ", Style::default().fg(theme.brand_accent)),
                Span::styled("primary   ", Style::default().fg(theme.muted)),
                Span::styled("2/3/4 ", Style::default().fg(theme.brand_accent)),
                Span::styled("fallback slots   ", Style::default().fg(theme.muted)),
                Span::styled("esc ", Style::default().fg(theme.brand_accent)),
                Span::styled("back", Style::default().fg(theme.muted)),
            ])
        } else {
            Line::from(vec![
                Span::styled("  tab/←→ ", Style::default().fg(theme.brand_accent)),
                Span::styled("section   ", Style::default().fg(theme.muted)),
                Span::styled("↑/↓ ", Style::default().fg(theme.brand_accent)),
                Span::styled("navigate   ", Style::default().fg(theme.muted)),
                Span::styled("enter/space ", Style::default().fg(theme.brand_accent)),
                Span::styled("select/toggle   ", Style::default().fg(theme.muted)),
                Span::styled("c ", Style::default().fg(theme.brand_accent)),
                Span::styled("clear/reset   ", Style::default().fg(theme.muted)),
                Span::styled("a ", Style::default().fg(theme.brand_accent)),
                Span::styled("sync minipubs   ", Style::default().fg(theme.muted)),
                Span::styled("esc ", Style::default().fg(theme.brand_accent)),
                Span::styled("save & quit", Style::default().fg(theme.muted)),
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

        // Next tab order: Providers -> Fallbacks -> MiniPubs -> Autonomy -> Workspace -> Probes -> Browser
        state.next_tab();
        assert_eq!(state.active_tab, SettingsTab::Fallbacks);
        assert_eq!(state.selected_index, 0);

        state.next_tab();
        assert_eq!(state.active_tab, SettingsTab::MiniPubs);
        assert_eq!(state.selected_index, 0);

        state.next_tab();
        assert_eq!(state.active_tab, SettingsTab::Autonomy);
        assert_eq!(state.selected_index, 0);

        state.next_tab();
        assert_eq!(state.active_tab, SettingsTab::Workspace);
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
        assert_eq!(state.active_tab, SettingsTab::Fallbacks);
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

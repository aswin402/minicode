//! Modal dialog state management and rendering dispatch.

pub mod api_key;
pub mod architecture;
pub mod blocks;
pub mod code_explorer;
pub mod code_review;
pub mod command_catalog;
pub mod common;
pub mod context_diagnostics;
pub mod doctor;
pub mod exit_confirm;
pub mod git_diff;
pub mod help;
pub mod inquiry;
pub mod minipower;
pub mod model_select;
pub mod processes;
pub mod provider_select;
pub mod session_browser;
pub mod settings;
pub mod stack_select;
pub mod streaming_select;
pub mod swarm_deck;
pub mod theme_select;
pub mod todo;
pub mod undo_checkpoint;
pub mod vault;
pub mod workspace_analysis;

#[allow(unused_imports)]
pub use command_catalog::{CommandCatalogItem, COMMAND_CATALOG_ITEMS};
#[allow(unused_imports)]
pub use common::{
    compute_scroll_offset, format_time_ago, render_modal_nav_buttons, ModalNavInfo,
    ModalNavRegions, TurnCheckpointInfo,
};

use crate::agent::models::ModelInfo;
use crate::ui::theme::Theme;
use ratatui::layout::Rect;
use ratatui::Frame;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StatusBarTarget {
    #[default]
    All,
    Model,
    Effort,
    Path,
    Git,
    Badges,
}

impl StatusBarTarget {
    pub const fn all() -> &'static [StatusBarTarget] {
        &[
            StatusBarTarget::All,
            StatusBarTarget::Model,
            StatusBarTarget::Effort,
            StatusBarTarget::Path,
            StatusBarTarget::Git,
            StatusBarTarget::Badges,
        ]
    }

    pub const fn label(self) -> &'static str {
        match self {
            StatusBarTarget::All => "All (Global)",
            StatusBarTarget::Model => "Model",
            StatusBarTarget::Effort => "Effort",
            StatusBarTarget::Path => "Path",
            StatusBarTarget::Git => "Git",
            StatusBarTarget::Badges => "Badges",
        }
    }

    pub fn next(self) -> Self {
        let all = Self::all();
        let idx = all.iter().position(|t| *t == self).unwrap_or(0);
        all[(idx + 1) % all.len()]
    }

    pub fn prev(self) -> Self {
        let all = Self::all();
        let idx = all.iter().position(|t| *t == self).unwrap_or(0);
        all[(idx + all.len() - 1) % all.len()]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeModalTab {
    Themes,
    Animations,
    TodoStyles,
    SwarmStyles,
    ToastStyles,
    ToastSymbols,
    ActivityStyles,
    MenuStyles,
    InputBoxStyles,
    StatusBarStyles,
    ContextMeterStyles,
    ModeBarStyles,
    ActivityHintStyles,
    ToolCardStyles,
    ToolIconStyles,
    OutputFrameStyles,
    OutputStyles,
    OutputBadgeStyles,
    TurnSummaryStyles,
    DoctorStyles,
    Visibility,
}

impl ThemeModalTab {
    /// Returns all tabs in display order.
    pub const fn all() -> &'static [ThemeModalTab] {
        &[
            ThemeModalTab::Themes,
            ThemeModalTab::Animations,
            ThemeModalTab::TodoStyles,
            ThemeModalTab::SwarmStyles,
            ThemeModalTab::ToastStyles,
            ThemeModalTab::ToastSymbols,
            ThemeModalTab::ActivityStyles,
            ThemeModalTab::MenuStyles,
            ThemeModalTab::InputBoxStyles,
            ThemeModalTab::StatusBarStyles,
            ThemeModalTab::ContextMeterStyles,
            ThemeModalTab::ModeBarStyles,
            ThemeModalTab::ActivityHintStyles,
            ThemeModalTab::ToolCardStyles,
            ThemeModalTab::ToolIconStyles,
            ThemeModalTab::OutputFrameStyles,
            ThemeModalTab::OutputStyles,
            ThemeModalTab::OutputBadgeStyles,
            ThemeModalTab::TurnSummaryStyles,
            ThemeModalTab::DoctorStyles,
            ThemeModalTab::Visibility,
        ]
    }

    /// Sidebar navigation label.
    pub const fn sidebar_label(&self) -> &'static str {
        match self {
            ThemeModalTab::Themes => "1. Themes",
            ThemeModalTab::Animations => "2. Animations",
            ThemeModalTab::TodoStyles => "3. Todo Styles",
            ThemeModalTab::SwarmStyles => "4. Swarm Styles",
            ThemeModalTab::ToastStyles => "5. Toast Styles",
            ThemeModalTab::ToastSymbols => "6. Toast Symbols",
            ThemeModalTab::ActivityStyles => "7. Activity Bar",
            ThemeModalTab::MenuStyles => "8. Menu Styles",
            ThemeModalTab::InputBoxStyles => "9. Input Box",
            ThemeModalTab::StatusBarStyles => "10. Status Bar",
            ThemeModalTab::ContextMeterStyles => "11. Context & KV",
            ThemeModalTab::ModeBarStyles => "12. Mode Badges",
            ThemeModalTab::ActivityHintStyles => "13. Timer & Hint",
            ThemeModalTab::ToolCardStyles => "14. Tool Cards",
            ThemeModalTab::ToolIconStyles => "15. Tool Icons",
            ThemeModalTab::OutputFrameStyles => "16. Output Frame",
            ThemeModalTab::OutputStyles => "17. Output Style",
            ThemeModalTab::OutputBadgeStyles => "18. Output Badge",
            ThemeModalTab::TurnSummaryStyles => "19. Turn Summary",
            ThemeModalTab::DoctorStyles => "20. Doctor Styles",
            ThemeModalTab::Visibility => "21. Show / Hide",
        }
    }

    /// Right-pane section title and subtitle.
    pub const fn section_header(&self) -> (&'static str, &'static str) {
        match self {
            ThemeModalTab::Themes => (
                "Color Themes & Palettes",
                "Select active color theme across dark, light, neon, and high-contrast palettes",
            ),
            ThemeModalTab::Animations => (
                "Streaming Spinners & Animations",
                "Select live thinking and tool execution spinner animation style",
            ),
            ThemeModalTab::TodoStyles => (
                "Live Plan & Todo Widget Styles",
                "Configure how task plans and milestone progress render in the timeline",
            ),
            ThemeModalTab::SwarmStyles => (
                "MiniPub Swarm Flight Deck Styles",
                "Select multi-agent telemetry and message bus layout for parallel subagents",
            ),
            ThemeModalTab::ToastStyles => (
                "Toast Notification Borders & Frames",
                "Customize corner geometry, padding, and border framing for status toasts",
            ),
            ThemeModalTab::ToastSymbols => (
                "Toast Notification Symbol Sets",
                "Select status indicator glyphs (ASCII, Geometric, Emoji, Kaomoji, Cyber, Technical ⎇, Nerd Font)",
            ),
            ThemeModalTab::ActivityStyles => (
                "Activity Bar & Streaming Status",
                "Configure live thinking phase, tool execution pipe, and step trail format",
            ),
            ThemeModalTab::MenuStyles => (
                "Intellisense & Autocomplete Menu Styles",
                "Customize chatbox autocomplete popup border, badges, and icon pack (Minimal, Symbols ⎇, Emoji, Kaomoji, Nerd Font)",
            ),
            ThemeModalTab::InputBoxStyles => (
                "Input Chat Box Frame & Styles",
                "Choose prompt input container geometry (rounded, left rail, split bar, rules, minimal)",
            ),
            ThemeModalTab::StatusBarStyles => (
                "Bottom Status Bar Layout Styles",
                "Style provider/model, effort, workspace path, and git badges (press 'c' for per-component overrides)",
            ),
            ThemeModalTab::ContextMeterStyles => (
                "Context Occupancy & KV Cache Meters",
                "Select visual meter for context tokens and KV cache (circle, bar, blocks, braille, symbols, emoji)",
            ),
            ThemeModalTab::ModeBarStyles => (
                "Mode Strip Styles (Ponytail · Caveman · Streaming)",
                "Style or hide the right-side mode badge strip above the input box",
            ),
            ThemeModalTab::ActivityHintStyles => (
                "Activity Timer & Interrupt Hint Styles",
                "Style or hide the elapsed timer and '(esc to interrupt)' activity bar suffix",
            ),
            ThemeModalTab::ToolCardStyles => (
                "Agent Tool Call Cards & Timeline Containers",
                "Style inline Bash, Browser, Read/Edit tool blocks (classic tree, cards, rails, Oh My Posh pills, compact)",
            ),
            ThemeModalTab::ToolIconStyles => (
                "Agent Tool Call & Thought Icon Packs",
                "Choose icon set for inline tool calls & thoughts (Minimal, Unicode ⎇ ❯ ◎, ASCII, Emoji, Kaomoji, Nerd Font)",
            ),
            ThemeModalTab::OutputFrameStyles => (
                "Assistant Output Frame & Top Divider Styles",
                "Frame or separate minicode responses (open rounded ╭─ minicode ─╮, full/short/medium rules, Oh My Posh, Nerd Font, ASCII)",
            ),
            ThemeModalTab::OutputStyles => (
                "Assistant Output & Markdown Presentation Styles",
                "Style headings, callouts (> [!TIP]), checklists, code blocks & tables (Default, Linear/Vercel, Report-Skill, Notion, Posh, Nerd, Emoji, ASCII)",
            ),
            ThemeModalTab::OutputBadgeStyles => (
                "Output Header Badge & Icon Styles",
                "Customize the leading icon before 'minicode' (✦, Symbols, Nerd Font, Emoji, ASCII, Static & Animated Kaomoji, Pulse)",
            ),
            ThemeModalTab::TurnSummaryStyles => (
                "End-of-Turn Summary Footer (Tokens & Duration)",
                "Style total turn tokens & duration below responses (press 'p' to change placement; toggle fields in 21. Show / Hide)",
            ),
            ThemeModalTab::DoctorStyles => (
                "MiniDoctor Health Dashboard Styles (/doctor)",
                "Customize /doctor card chrome, gauge bars & badges (Aura, NerdFont, Kaomoji, Emoji, ASCII, Posh, Flutter, Minimal, Cyber, Compact)",
            ),
            ThemeModalTab::Visibility => (
                "Show / Hide TUI Elements & Doctor Options",
                "Toggle status elements, turn summary fields, MiniDoctor sparkline/sub-scores/drawer/animation, and thought tokens",
            ),
        }
    }

    /// Cycles to the next tab.
    pub fn next(&self) -> Self {
        let all = Self::all();
        let idx = all.iter().position(|t| t == self).unwrap_or(0);
        all[(idx + 1) % all.len()]
    }

    /// Cycles to the previous tab.
    pub fn prev(&self) -> Self {
        let all = Self::all();
        let idx = all.iter().position(|t| t == self).unwrap_or(0);
        all[(idx + all.len() - 1) % all.len()]
    }
}

#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum ModalState {
    None,
    ApiKeyInput {
        provider: String,
        env_var: String,
        input: String,
        cursor: usize,
    },
    ProviderSelect {
        providers: Vec<String>,
        selected_index: usize,
    },
    ModelSelect {
        provider: String,
        models: Vec<ModelInfo>,
        filtered_indices: Vec<usize>,
        selected_index: usize,
        filter: String,
        loading: bool,
    },
    UndoCheckpoint {
        checkpoints: Vec<TurnCheckpointInfo>,
        selected_index: usize,
    },
    ThemeSelect {
        themes: Vec<crate::ui::theme::ThemeInfo>,
        animations: Vec<crate::ui::animation::AnimationOption>,
        active_tab: ThemeModalTab,
        theme_selected_index: usize,
        animation_selected_index: usize,
        todo_style_selected_index: usize,
        swarm_style_selected_index: usize,
        toast_style_selected_index: usize,
        toast_symbol_selected_index: usize,
        activity_style_selected_index: usize,
        menu_style_selected_index: usize,
        input_box_style_selected_index: usize,
        status_bar_style_selected_index: usize,
        status_bar_target: StatusBarTarget,
        context_meter_style_selected_index: usize,
        mode_bar_style_selected_index: usize,
        activity_hint_style_selected_index: usize,
        tool_card_style_selected_index: usize,
        tool_icon_style_selected_index: usize,
        output_frame_style_selected_index: usize,
        output_style_selected_index: usize,
        output_badge_style_selected_index: usize,
        turn_summary_style_selected_index: usize,
        doctor_style_selected_index: usize,
        visibility_selected_index: usize,
        active_theme_id: String,
        active_animation_id: String,
        active_todo_style: String,
        active_swarm_style: String,
        active_toast_style: String,
        active_toast_symbols: String,
        active_activity_style: String,
        active_menu_style: String,
        ui_prefs: crate::config::UiConfig,
    },
    SessionBrowser {
        sessions: Vec<crate::session::store::SessionMetadata>,
        selected_index: usize,
        cached_summary: Option<crate::session::store::SessionSummary>,
    },
    StackSelect {
        stacks: Vec<crate::tools::minikit::stacks::Stack>,
        filtered_indices: Vec<usize>,
        selected_index: usize,
        filter: String,
    },
    CommandCatalog {
        filtered_indices: Vec<usize>,
        selected_index: usize,
        filter: String,
    },
    CodeExplorer {
        symbols: Vec<crate::context::explorer::CodeExploreMatch>,
        filtered_indices: Vec<usize>,
        selected_index: usize,
        filter: String,
        active_tab: usize,
    },
    GitDiff {
        diff_files: Vec<crate::git::GitDiffFile>,
        selected_file_index: usize,
        scroll_offset: usize,
        staged_view: bool,
    },
    Help,
    StreamingSelect {
        selected_index: usize,
        current_streaming: bool,
    },
    Approval(crate::ui::approval::ApprovalModalState),
    WorkspaceAnalysis {
        workspace_path: String,
        is_indexed: bool,
        cached_symbols_count: usize,
        cached_files_count: usize,
        selected_index: usize,
    },
    ProviderSetupRequired {
        provider_name: String,
        pending_prompt_preview: String,
        selected_index: usize,
    },
    ExitConfirm {
        workspace_name: String,
        selected_yes: bool,
    },
    ArchitectureAudit {
        report: Box<crate::context::governance::ArchitectureReport>,
        active_tab: usize,
        selected_index: usize,
        scroll_offset: usize,
    },
    ContextDiagnostics {
        data: Box<context_diagnostics::ContextDiagnosticsData>,
        active_tab: usize,
        scroll_offset: usize,
    },
    Settings(settings::SettingsModalState),
    #[allow(dead_code)]
    ConfigApproval {
        proposal: crate::tools::registry::agent_tools::config_tools::ConfigChangeProposal,
        selected_index: usize,
    },
    MiniPower(minipower::MiniPowerModalState),
    Blocks(blocks::BlocksModalState),
    Processes(processes::ProcessesModalState),
    Todo(todo::TodoModalState),
    Vault(vault::VaultModalState),
    Inquiry(inquiry::InquiryModalState),
    CodeReview(code_review::CodeReviewModalState),
    Doctor(doctor::DoctorModalState),
}

impl ModalState {
    pub fn is_active(&self) -> bool {
        !matches!(self, ModalState::None)
    }

    /// Returns `true` when the currently visible modal contains live animated spinners
    /// or progress indicators that require periodic `FrameScheduler` animation ticks
    /// even while the main agent turn is idle.
    pub fn has_live_animation(&self) -> bool {
        match self {
            ModalState::ThemeSelect { active_tab, .. } => matches!(
                active_tab,
                ThemeModalTab::Animations
                    | ThemeModalTab::ActivityStyles
                    | ThemeModalTab::ActivityHintStyles
                    | ThemeModalTab::OutputFrameStyles
                    | ThemeModalTab::OutputBadgeStyles
                    | ThemeModalTab::Visibility
            ),
            ModalState::ModelSelect { loading: true, .. } => true,
            ModalState::Settings(state) => state.probing,
            ModalState::MiniPower(state) => state.is_evaluating,
            ModalState::Doctor(state) => state.has_live_animation(),
            _ => false,
        }
    }

    pub fn new_doctor(
        workspace_root: &std::path::Path,
        scope: minidoctor::ScanScope,
        ui: &crate::config::UiConfig,
    ) -> Self {
        let mut st = doctor::DoctorModalState::new(workspace_root, scope, true);
        st.apply_ui_config(ui);
        Self::Doctor(st)
    }

    pub fn new_code_review(report: crate::git::ReviewReport, staged_only: bool) -> Self {
        Self::CodeReview(code_review::CodeReviewModalState::new(report, staged_only))
    }

    pub fn new_todo(workspace_root: &std::path::Path) -> Self {
        fn summarize_task_counts(
            tasks: &[crate::context::memory::working_memory::TaskItem],
        ) -> (usize, usize, usize, usize) {
            use crate::context::memory::working_memory::TaskItemStatus;
            let mut completed = 0;
            let mut in_progress = 0;
            let mut pending = 0;
            for t in tasks {
                match t.status {
                    TaskItemStatus::Completed => completed += 1,
                    TaskItemStatus::InProgress => in_progress += 1,
                    TaskItemStatus::Pending => pending += 1,
                }
            }
            (tasks.len(), completed, in_progress, pending)
        }

        let wm = crate::context::memory::working_memory::WorkingMemory::new(workspace_root);
        let mut milestones = wm.read_roadmap_milestones();
        let task_plan_tasks = wm.read_task_plan_tasks();
        let has_task_plan = !task_plan_tasks.is_empty();

        // 1. Reconcile Tier-2 `task_plan.md` against Tier-1 `todo.md` milestones FIRST so tactical
        //    sub-steps always attach to the active roadmap milestone rather than `Goal Anchor`.
        if has_task_plan {
            let active_pos = milestones.iter().position(|m| m.is_active);
            if let Some(pos) = active_pos {
                let active_m = &mut milestones[pos];
                if active_m.tasks.is_empty() {
                    active_m.tasks = task_plan_tasks;
                    let (total, completed, in_progress, pending) =
                        summarize_task_counts(&active_m.tasks);
                    active_m.total_tasks = total;
                    active_m.completed_tasks = completed;
                    active_m.in_progress_tasks = in_progress;
                    active_m.pending_tasks = pending;
                } else {
                    let is_same_task_set = !wm.is_tactical_subplan()
                        && active_m.tasks.len() == task_plan_tasks.len()
                        && active_m
                            .tasks
                            .iter()
                            .zip(task_plan_tasks.iter())
                            .all(|(a, b)| a.title.eq_ignore_ascii_case(&b.title));

                    if is_same_task_set {
                        let mut merged = task_plan_tasks;
                        for (dst, existing) in merged.iter_mut().zip(active_m.tasks.iter()) {
                            if dst.sub_tasks.is_empty() && !existing.sub_tasks.is_empty() {
                                dst.sub_tasks = existing.sub_tasks.clone();
                            }
                        }
                        active_m.tasks = merged;
                        let (total, completed, in_progress, pending) =
                            summarize_task_counts(&active_m.tasks);
                        active_m.total_tasks = total;
                        active_m.completed_tasks = completed;
                        active_m.in_progress_tasks = in_progress;
                        active_m.pending_tasks = pending;
                    } else {
                        let target_idx = active_m
                            .tasks
                            .iter()
                            .position(|t| {
                                t.status
                                    == crate::context::memory::working_memory::TaskItemStatus::InProgress
                            })
                            .or_else(|| {
                                active_m.tasks.iter().position(|t| {
                                    t.status
                                        == crate::context::memory::working_memory::TaskItemStatus::Pending
                                })
                            });
                        if let Some(idx) = target_idx {
                            active_m.tasks[idx].sub_tasks = task_plan_tasks;
                        } else if let Some(last) = active_m.tasks.last_mut() {
                            last.sub_tasks = task_plan_tasks;
                        }
                    }
                }
            } else {
                // When roadmap milestones are all completed or empty, synthesize active milestone from task plan
                let plan_title = wm
                    .read_task_plan_title()
                    .unwrap_or_else(|| "Active Milestone".to_string());
                let (total, completed, in_progress, pending) =
                    summarize_task_counts(&task_plan_tasks);
                let is_active = in_progress > 0 || pending > 0 || total > 0;

                milestones.push(crate::context::memory::working_memory::MilestonePhase {
                    id: plan_title.clone(),
                    title: plan_title,
                    total_tasks: total,
                    completed_tasks: completed,
                    in_progress_tasks: in_progress,
                    pending_tasks: pending,
                    tasks: task_plan_tasks,
                    is_active,
                });
            }
        }

        // 2. Include Active Goal Anchor (.minicode/intent_anchor.json or intent_ledger.json) ONLY
        //    when no structured roadmap/task plan exists yet (e.g. `/goal` / `/intent` before
        //    `create_plan`). Once a structured Two-Tier plan is active, `milestones` is canonical
        //    and showing raw prompt bullets above it creates duplicate clutter.
        let is_scaffold_stub_only = milestones.len() == 1
            && milestones[0].tasks.len() == 1
            && milestones[0].tasks[0].title == "Initial project setup and architecture sync";
        let has_structured_plan = !milestones.is_empty() && !is_scaffold_stub_only;

        let anchor_path = workspace_root.join(".minicode").join("intent_anchor.json");
        let ledger_path = if anchor_path.exists() {
            anchor_path
        } else {
            workspace_root.join(".minicode").join("intent_ledger.json")
        };
        if !has_structured_plan && ledger_path.exists() {
            if let Ok(ledger) =
                crate::context::memory::intent::IntentLedger::load_from_disk(&ledger_path)
            {
                if !ledger.root_objective.trim().is_empty() || !ledger.items.is_empty() {
                    let goal_tasks: Vec<crate::context::memory::working_memory::TaskItem> = ledger
                        .items
                        .iter()
                        .enumerate()
                        .map(|(idx, item)| {
                            let status = match item.status {
                                crate::context::memory::intent::RequirementStatus::Completed
                                | crate::context::memory::intent::RequirementStatus::Skipped => {
                                    crate::context::memory::working_memory::TaskItemStatus::Completed
                                }
                                crate::context::memory::intent::RequirementStatus::InProgress => {
                                    crate::context::memory::working_memory::TaskItemStatus::InProgress
                                }
                                _ => crate::context::memory::working_memory::TaskItemStatus::Pending,
                            };
                            let title = match &item.description {
                                Some(desc) if !desc.trim().is_empty() => {
                                    format!("{} ({})", item.title.trim(), desc.trim())
                                }
                                _ => item.title.trim().to_string(),
                            };
                            crate::context::memory::working_memory::TaskItem {
                                title,
                                status,
                                sub_tasks: Vec::new(),
                                line_index: idx,
                            }
                        })
                        .collect();

                    let (total, completed, in_progress, pending) =
                        summarize_task_counts(&goal_tasks);
                    let is_active = pending > 0 || in_progress > 0 || milestones.is_empty();
                    let obj_title = if ledger.root_objective.trim().is_empty() {
                        "Active Goal Anchor".to_string()
                    } else {
                        ledger.root_objective.trim().to_string()
                    };
                    milestones.insert(
                        0,
                        crate::context::memory::working_memory::MilestonePhase {
                            id: "🎯 Goal Anchor".to_string(),
                            title: obj_title,
                            total_tasks: total,
                            completed_tasks: completed,
                            in_progress_tasks: in_progress,
                            pending_tasks: pending,
                            tasks: goal_tasks,
                            is_active,
                        },
                    );
                }
            }
        }
        Self::Todo(todo::TodoModalState::new(milestones))
    }

    /// Refreshes the `/todo` modal in place when it is currently open so live `PlanUpdated`
    /// events immediately reflect completed steps and sub-steps without closing/reopening.
    pub fn refresh_todo_if_open(&mut self, workspace_root: &std::path::Path) {
        if matches!(self, Self::Todo(_)) {
            if let Self::Todo(fresh) = Self::new_todo(workspace_root) {
                if let Self::Todo(ref mut current) = self {
                    current.refresh_milestones(fresh.milestones);
                }
            }
        }
    }

    #[allow(dead_code)]
    pub fn new_processes(workspace_root: &std::path::Path) -> Self {
        Self::Processes(processes::ProcessesModalState::new(workspace_root))
    }

    pub fn new_blocks(workspace_root: &std::path::Path) -> Self {
        Self::Blocks(blocks::BlocksModalState::new(workspace_root))
    }

    pub fn new_vault(workspace_root: &std::path::Path) -> Self {
        Self::Vault(vault::VaultModalState::new(workspace_root))
    }

    pub fn new_minipower(workspace_root: &std::path::Path) -> Self {
        Self::MiniPower(minipower::MiniPowerModalState::new(workspace_root))
    }

    pub fn new_settings(config: &crate::config::Config, workspace_root: &std::path::Path) -> Self {
        Self::Settings(settings::SettingsModalState::from_config(
            config,
            workspace_root,
        ))
    }

    #[allow(dead_code)]
    pub fn new_config_approval(
        proposal: crate::tools::registry::agent_tools::config_tools::ConfigChangeProposal,
    ) -> Self {
        Self::ConfigApproval {
            proposal,
            selected_index: 0,
        }
    }

    pub fn new_context_diagnostics(data: context_diagnostics::ContextDiagnosticsData) -> Self {
        Self::ContextDiagnostics {
            data: Box::new(data),
            active_tab: 0,
            scroll_offset: 0,
        }
    }

    pub fn new_architecture_audit(report: crate::context::governance::ArchitectureReport) -> Self {
        Self::ArchitectureAudit {
            report: Box::new(report),
            active_tab: 0,
            selected_index: 0,
            scroll_offset: 0,
        }
    }

    pub fn new_exit_confirm(workspace_root: &std::path::Path) -> Self {
        let workspace_name = workspace_root
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("minicode")
            .to_string();
        Self::ExitConfirm {
            workspace_name,
            selected_yes: false,
        }
    }

    pub fn new_api_key_input(provider: String, env_var: String, initial: Option<String>) -> Self {
        let input = initial.unwrap_or_default();
        let cursor = input.len();
        ModalState::ApiKeyInput {
            provider,
            env_var,
            input,
            cursor,
        }
    }

    pub fn new_provider_select() -> Self {
        let mut providers = Vec::with_capacity(crate::constants::SUPPORTED_PROVIDERS.len() + 1);
        providers.push("favorites".to_string());
        providers.extend(
            crate::constants::SUPPORTED_PROVIDERS
                .iter()
                .map(|&p| p.to_string()),
        );
        ModalState::ProviderSelect {
            providers,
            selected_index: 0,
        }
    }

    /// Builds `ProviderSelect` including any user-defined custom providers from
    /// `config.provider.custom_endpoints` and pre-selecting the currently active provider.
    pub fn new_provider_select_with_config(config: &crate::config::Config) -> Self {
        let mut providers = Vec::with_capacity(
            crate::constants::SUPPORTED_PROVIDERS.len()
                + config.provider.custom_endpoints.len()
                + 1,
        );
        providers.push("favorites".to_string());
        providers.extend(
            crate::constants::SUPPORTED_PROVIDERS
                .iter()
                .map(|&p| p.to_string()),
        );

        let mut custom_names: Vec<String> = config
            .provider
            .custom_endpoints
            .keys()
            .filter(|name| {
                let lower = name.trim().to_ascii_lowercase();
                !lower.is_empty()
                    && !providers
                        .iter()
                        .any(|existing| existing.eq_ignore_ascii_case(&lower))
            })
            .cloned()
            .collect();
        custom_names.sort();
        providers.extend(custom_names);

        let active = config.provider.default.trim();
        let selected_index = if !active.is_empty() {
            providers
                .iter()
                .position(|p| p.eq_ignore_ascii_case(active))
                .unwrap_or(0)
        } else {
            0
        };

        ModalState::ProviderSelect {
            providers,
            selected_index,
        }
    }

    pub fn new_model_select(provider: String, models: Vec<ModelInfo>) -> Self {
        let count = models.len();
        ModalState::ModelSelect {
            provider,
            models,
            filtered_indices: (0..count).collect(),
            selected_index: 0,
            filter: String::new(),
            loading: false,
        }
    }

    pub fn new_undo_checkpoint(manifests: Vec<crate::session::backup::BackupManifest>) -> Self {
        let mut checkpoints = Vec::new();
        for (i, m) in manifests.into_iter().enumerate() {
            let time_ago = format_time_ago(&m.timestamp);
            let prompt = m
                .user_prompt
                .unwrap_or_else(|| format!("Turn #{}", m.turn_id));
            let files = m.files.into_iter().map(|f| f.original_path).collect();
            checkpoints.push(TurnCheckpointInfo {
                turn_id: m.turn_id,
                prompt,
                timestamp: m.timestamp,
                time_ago,
                files,
                is_latest: i == 0,
            });
        }
        ModalState::UndoCheckpoint {
            checkpoints,
            selected_index: 0,
        }
    }

    pub fn new_theme_select(
        active_theme_id: &str,
        active_animation_id: &str,
        active_todo_style: &str,
        active_swarm_style: &str,
        active_toast_style: &str,
        active_toast_symbols: &str,
        active_activity_style: &str,
    ) -> Self {
        let themes = crate::ui::theme::Theme::list_themes();
        let theme_selected_index = themes
            .iter()
            .position(|t| t.id == active_theme_id || active_theme_id.starts_with(&t.id))
            .unwrap_or(0);
        let animations = crate::ui::animation::ANIMATION_OPTIONS.to_vec();
        let animation_selected_index = animations
            .iter()
            .position(|a| a.id == active_animation_id)
            .unwrap_or(0);
        let todo_style_selected_index = theme_select::TODO_STYLE_OPTIONS
            .iter()
            .position(|s| s.id == active_todo_style)
            .unwrap_or(0);
        let swarm_style_selected_index = theme_select::SWARM_STYLE_OPTIONS
            .iter()
            .position(|s| s.id == active_swarm_style)
            .unwrap_or(0);
        let toast_style_selected_index = theme_select::TOAST_STYLE_OPTIONS
            .iter()
            .position(|s| s.id == active_toast_style)
            .unwrap_or(0);
        let toast_symbol_selected_index = theme_select::TOAST_SYMBOL_OPTIONS
            .iter()
            .position(|s| s.id == active_toast_symbols)
            .unwrap_or(0);
        let activity_style_selected_index = theme_select::ACTIVITY_STYLE_OPTIONS
            .iter()
            .position(|s| s.id == active_activity_style)
            .unwrap_or(0);
        ModalState::ThemeSelect {
            themes,
            animations,
            active_tab: ThemeModalTab::Themes,
            theme_selected_index,
            animation_selected_index,
            todo_style_selected_index,
            swarm_style_selected_index,
            toast_style_selected_index,
            toast_symbol_selected_index,
            activity_style_selected_index,
            menu_style_selected_index: 0,
            input_box_style_selected_index: 0,
            status_bar_style_selected_index: 0,
            status_bar_target: StatusBarTarget::All,
            context_meter_style_selected_index: 0,
            mode_bar_style_selected_index: 0,
            activity_hint_style_selected_index: 0,
            tool_card_style_selected_index: 0,
            tool_icon_style_selected_index: 0,
            output_frame_style_selected_index: 0,
            output_style_selected_index: 0,
            output_badge_style_selected_index: 0,
            turn_summary_style_selected_index: 0,
            doctor_style_selected_index: 0,
            visibility_selected_index: 0,
            active_theme_id: active_theme_id.to_string(),
            active_animation_id: active_animation_id.to_string(),
            active_todo_style: active_todo_style.to_string(),
            active_swarm_style: active_swarm_style.to_string(),
            active_toast_style: active_toast_style.to_string(),
            active_toast_symbols: active_toast_symbols.to_string(),
            active_activity_style: active_activity_style.to_string(),
            active_menu_style: crate::constants::DEFAULT_MENU_STYLE.to_string(),
            ui_prefs: crate::config::UiConfig {
                theme: active_theme_id.to_string(),
                animation: active_animation_id.to_string(),
                todo_style: active_todo_style.to_string(),
                swarm_style: active_swarm_style.to_string(),
                toast_style: active_toast_style.to_string(),
                toast_symbols: active_toast_symbols.to_string(),
                activity_style: active_activity_style.to_string(),
                ..Default::default()
            },
        }
    }

    pub fn new_theme_select_from_ui(ui: &crate::config::UiConfig) -> Self {
        let mut modal = Self::new_theme_select(
            &ui.theme,
            &ui.animation,
            &ui.todo_style,
            &ui.swarm_style,
            &ui.toast_style,
            &ui.toast_symbols,
            &ui.activity_style,
        );
        if let ModalState::ThemeSelect {
            ref mut menu_style_selected_index,
            ref mut input_box_style_selected_index,
            ref mut status_bar_style_selected_index,
            ref mut context_meter_style_selected_index,
            ref mut mode_bar_style_selected_index,
            ref mut activity_hint_style_selected_index,
            ref mut tool_card_style_selected_index,
            ref mut tool_icon_style_selected_index,
            ref mut output_frame_style_selected_index,
            ref mut output_style_selected_index,
            ref mut output_badge_style_selected_index,
            ref mut turn_summary_style_selected_index,
            ref mut doctor_style_selected_index,
            ref mut active_menu_style,
            ref mut ui_prefs,
            ..
        } = &mut modal
        {
            let active_menu = ui.menu_style_enum();
            *menu_style_selected_index = active_menu as usize;
            *active_menu_style = active_menu.as_str().to_string();
            let active_input = ui.input_box_style_enum().as_str();
            *input_box_style_selected_index = theme_select::INPUT_BOX_STYLE_OPTIONS
                .iter()
                .position(|s| s.id == active_input)
                .unwrap_or(0);
            let active_status = ui.status_bar_style_enum().as_str();
            *status_bar_style_selected_index = theme_select::STATUS_BAR_STYLE_OPTIONS
                .iter()
                .position(|s| s.id == active_status)
                .unwrap_or(0);
            let active_ctx = ui.context_meter_style_enum().as_str();
            *context_meter_style_selected_index = theme_select::CONTEXT_METER_STYLE_OPTIONS
                .iter()
                .position(|s| s.id == active_ctx)
                .unwrap_or(0);
            let active_mode = ui.mode_bar_style_enum().as_str();
            *mode_bar_style_selected_index = theme_select::MODE_BAR_STYLE_OPTIONS
                .iter()
                .position(|s| s.id == active_mode)
                .unwrap_or(0);
            let active_hint = ui.activity_hint_style_enum().as_str();
            *activity_hint_style_selected_index = theme_select::ACTIVITY_HINT_STYLE_OPTIONS
                .iter()
                .position(|s| s.id == active_hint)
                .unwrap_or(0);
            let active_tool_card = ui.tool_card_style_enum().as_str();
            *tool_card_style_selected_index = theme_select::TOOL_CARD_STYLE_OPTIONS
                .iter()
                .position(|s| s.id == active_tool_card)
                .unwrap_or(0);
            let active_tool_icon = ui.tool_icon_style_enum().as_str();
            *tool_icon_style_selected_index = theme_select::TOOL_ICON_STYLE_OPTIONS
                .iter()
                .position(|s| s.id == active_tool_icon)
                .unwrap_or(0);
            let active_output_frame = ui.output_frame_style_enum().as_str();
            *output_frame_style_selected_index = theme_select::OUTPUT_FRAME_STYLE_OPTIONS
                .iter()
                .position(|s| s.id == active_output_frame)
                .unwrap_or(0);
            let active_output_style = ui.output_style_enum().as_str();
            *output_style_selected_index = theme_select::OUTPUT_STYLE_OPTIONS
                .iter()
                .position(|s| s.id == active_output_style)
                .unwrap_or(0);
            let active_output_badge = ui.output_badge_style_enum().as_str();
            *output_badge_style_selected_index = theme_select::OUTPUT_BADGE_STYLE_OPTIONS
                .iter()
                .position(|s| s.id == active_output_badge)
                .unwrap_or(0);
            let active_turn_summary = ui.turn_summary_style_enum().as_str();
            *turn_summary_style_selected_index = theme_select::TURN_SUMMARY_STYLE_OPTIONS
                .iter()
                .position(|s| s.id == active_turn_summary)
                .unwrap_or(0);
            let active_doctor_style = ui.doctor_style_enum().as_str();
            *doctor_style_selected_index = theme_select::DOCTOR_STYLE_OPTIONS
                .iter()
                .position(|s| s.id == active_doctor_style)
                .unwrap_or(0);
            *ui_prefs = ui.clone();
        }
        modal
    }

    pub fn new_session_browser(
        sessions: Vec<crate::session::store::SessionMetadata>,
        cached_summary: Option<crate::session::store::SessionSummary>,
    ) -> Self {
        ModalState::SessionBrowser {
            sessions,
            selected_index: 0,
            cached_summary,
        }
    }

    pub fn new_stack_select() -> Self {
        let stacks = crate::tools::minikit::scaffolder::MiniKitScaffolder::get_all_stacks();
        let count = stacks.len();
        ModalState::StackSelect {
            stacks,
            filtered_indices: (0..count).collect(),
            selected_index: 0,
            filter: String::new(),
        }
    }

    pub fn new_command_catalog() -> Self {
        let count = COMMAND_CATALOG_ITEMS.len();
        ModalState::CommandCatalog {
            filtered_indices: (0..count).collect(),
            selected_index: 0,
            filter: String::new(),
        }
    }

    pub fn new_code_explorer(workspace_root: &std::path::Path) -> Self {
        let mut graph = crate::context::graph::CodeGraph::new();
        if let Err(e) = graph.build_graph(workspace_root) {
            tracing::warn!(error = %e, "Failed to build code graph for CodeExplorer modal");
        }
        let res = crate::context::explorer::CodeExploreEngine::explore(
            workspace_root,
            &graph,
            "",
            None,
            2,
            true,
        )
        .unwrap_or_else(|e| {
            tracing::warn!(error = %e, "Failed to run AST explore for CodeExplorer modal");
            crate::context::explorer::CodeExploreResult {
                query: String::new(),
                target_symbol: None,
                matches: Vec::new(),
                summary: String::new(),
            }
        });
        let count = res.matches.len();
        ModalState::CodeExplorer {
            symbols: res.matches,
            filtered_indices: (0..count).collect(),
            selected_index: 0,
            filter: String::new(),
            active_tab: 0,
        }
    }

    pub fn new_git_diff(diff_files: Vec<crate::git::GitDiffFile>, staged_view: bool) -> Self {
        ModalState::GitDiff {
            diff_files,
            selected_file_index: 0,
            scroll_offset: 0,
            staged_view,
        }
    }

    pub fn update_filter(&mut self) {
        if let ModalState::ModelSelect {
            models,
            filtered_indices,
            selected_index,
            filter,
            ..
        } = self
        {
            let f = filter.trim().to_lowercase();
            *filtered_indices = models
                .iter()
                .enumerate()
                .filter(|(_, m)| {
                    if f.is_empty() {
                        true
                    } else {
                        Self::contains_ci(&m.id, &f) || Self::contains_ci(&m.name, &f)
                    }
                })
                .map(|(i, _)| i)
                .collect();

            if *selected_index >= filtered_indices.len() {
                *selected_index = filtered_indices.len().saturating_sub(1);
            }
        } else if let ModalState::StackSelect {
            stacks,
            filtered_indices,
            selected_index,
            filter,
        } = self
        {
            let f = filter.trim().to_lowercase();
            *filtered_indices = stacks
                .iter()
                .enumerate()
                .filter(|(_, s)| {
                    if f.is_empty() {
                        true
                    } else {
                        Self::contains_ci(&s.name, &f)
                            || Self::contains_ci(&s.runtime, &f)
                            || Self::contains_ci(&s.description, &f)
                            || s.packages.iter().any(|p| Self::contains_ci(p, &f))
                    }
                })
                .map(|(i, _)| i)
                .collect();

            if *selected_index >= filtered_indices.len() {
                *selected_index = filtered_indices.len().saturating_sub(1);
            }
        } else if let ModalState::CommandCatalog {
            filtered_indices,
            selected_index,
            filter,
        } = self
        {
            let f = filter.trim().to_lowercase();
            *filtered_indices = COMMAND_CATALOG_ITEMS
                .iter()
                .enumerate()
                .filter(|(_, c)| {
                    if f.is_empty() {
                        true
                    } else {
                        Self::contains_ci(c.name, &f)
                            || Self::contains_ci(c.category, &f)
                            || Self::contains_ci(c.description, &f)
                            || Self::contains_ci(c.shortcut, &f)
                    }
                })
                .map(|(i, _)| i)
                .collect();

            if *selected_index >= filtered_indices.len() {
                *selected_index = filtered_indices.len().saturating_sub(1);
            }
        } else if let ModalState::CodeExplorer {
            symbols,
            filtered_indices,
            selected_index,
            filter,
            ..
        } = self
        {
            let f = filter.trim().to_lowercase();
            *filtered_indices = symbols
                .iter()
                .enumerate()
                .filter(|(_, s)| {
                    if f.is_empty() {
                        true
                    } else {
                        Self::contains_ci(&s.symbol_name, &f)
                            || Self::contains_ci(&s.kind, &f)
                            || Self::contains_ci(&s.file_path, &f)
                            || Self::contains_ci(&s.signature, &f)
                            || Self::contains_ci(s.layer.display_name(), &f)
                    }
                })
                .map(|(i, _)| i)
                .collect();

            if *selected_index >= filtered_indices.len() {
                *selected_index = filtered_indices.len().saturating_sub(1);
            }
        }
    }

    pub fn new_streaming_select(current_streaming: bool) -> Self {
        let selected_index = if current_streaming { 0 } else { 1 };
        ModalState::StreamingSelect {
            selected_index,
            current_streaming,
        }
    }

    pub fn new_workspace_analysis(workspace_root: &std::path::Path) -> Self {
        let graph_file = crate::context::graph_store::GraphStore::graph_file_path(workspace_root);
        let is_indexed = graph_file.exists();
        let (cached_symbols_count, cached_files_count) = if is_indexed {
            if let Ok(Some(snap)) =
                crate::context::graph_store::GraphStore::load_snapshot(workspace_root)
            {
                let sym_count = snap
                    .nodes
                    .iter()
                    .filter(|n| n.kind != crate::context::graph::SymbolKind::File)
                    .count();
                let file_count = snap.file_hashes.len();
                (sym_count, file_count)
            } else {
                (0, 0)
            }
        } else {
            (0, 0)
        };

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

        ModalState::WorkspaceAnalysis {
            workspace_path,
            is_indexed,
            cached_symbols_count,
            cached_files_count,
            selected_index: 0,
        }
    }

    pub fn new_provider_setup_required(provider_name: &str, prompt: &str) -> Self {
        let preview = if prompt.lines().count() > 1 {
            let first_line = prompt.lines().next().unwrap_or("");
            if first_line.chars().count() > 45 {
                format!("{}...", first_line.chars().take(45).collect::<String>())
            } else {
                format!("{}...", first_line)
            }
        } else if prompt.chars().count() > 45 {
            format!("{}...", prompt.chars().take(45).collect::<String>())
        } else {
            prompt.to_string()
        };

        ModalState::ProviderSetupRequired {
            provider_name: provider_name.to_string(),
            pending_prompt_preview: preview,
            selected_index: 0,
        }
    }

    /// Efficient zero-allocation case-insensitive substring search for ASCII text.
    fn contains_ci(haystack: &str, needle_lower: &str) -> bool {
        if needle_lower.is_empty() {
            return true;
        }
        if haystack.len() < needle_lower.len() {
            return false;
        }
        if haystack.is_ascii() && needle_lower.is_ascii() {
            let needle_bytes = needle_lower.as_bytes();
            haystack.as_bytes().windows(needle_bytes.len()).any(|w| {
                w.iter()
                    .zip(needle_bytes.iter())
                    .all(|(a, b)| a.to_ascii_lowercase() == *b)
            })
        } else {
            haystack.to_lowercase().contains(needle_lower)
        }
    }

    /// Returns whether this modal uses the unified browsing navigation chrome (`[← Back]` / `[× Close]`).
    /// Confirmation gates (`Approval`, `Inquiry`, `ExitConfirm`, `ConfigApproval`, `ProviderSetupRequired`)
    /// have dedicated accept/reject buttons and opt out of navigation stack chrome.
    pub fn supports_nav_chrome(&self) -> bool {
        !matches!(
            self,
            ModalState::None
                | ModalState::Approval(_)
                | ModalState::Inquiry(_)
                | ModalState::ExitConfirm { .. }
                | ModalState::ConfigApproval { .. }
                | ModalState::ProviderSetupRequired { .. }
        )
    }

    /// Short human-readable label for breadcrumbs and `[← Back: <Parent>]` buttons.
    pub fn nav_label(&self) -> &'static str {
        match self {
            ModalState::None => "Chat",
            ModalState::ApiKeyInput { .. } => "API Key",
            ModalState::ProviderSelect { .. } => "Providers",
            ModalState::ModelSelect { .. } => "Models",
            ModalState::UndoCheckpoint { .. } => "Undo",
            ModalState::ThemeSelect { .. } => "Themes",
            ModalState::SessionBrowser { .. } => "Sessions",
            ModalState::StackSelect { .. } => "Stacks",
            ModalState::CommandCatalog { .. } => "Commands",
            ModalState::CodeExplorer { .. } => "Explorer",
            ModalState::GitDiff { .. } => "Git Diff",
            ModalState::Help => "Help",
            ModalState::StreamingSelect { .. } => "Streaming",
            ModalState::WorkspaceAnalysis { .. } => "Workspace",
            ModalState::ArchitectureAudit { .. } => "Architecture",
            ModalState::ContextDiagnostics { .. } => "Context",
            ModalState::Settings(_) => "Settings",
            ModalState::MiniPower(_) => "MiniPower",
            ModalState::Blocks(_) => "MiniBlocks",
            ModalState::Processes(_) => "Tasks",
            ModalState::Todo(_) => "Roadmap",
            ModalState::Vault(_) => "MiniVault",
            ModalState::CodeReview(_) => "Review",
            ModalState::Doctor(_) => "MiniDoctor",
            ModalState::Approval(_)
            | ModalState::Inquiry(_)
            | ModalState::ExitConfirm { .. }
            | ModalState::ConfigApproval { .. }
            | ModalState::ProviderSetupRequired { .. } => "Dialog",
        }
    }

    /// Returns the outer bounding rectangle drawn by this modal, used to anchor the top-right
    /// clickable `[← Back]` / `[× Close]` chrome cleanly on the modal border.
    pub fn outer_rect(&self, area: Rect) -> Option<Rect> {
        use crate::constants::*;
        use crate::ui::layout_utils::{centered_rect, centered_rect_exact};
        match self {
            ModalState::ThemeSelect { .. } | ModalState::Settings(_) | ModalState::Todo(_) => {
                Some(area)
            }
            ModalState::ApiKeyInput { .. } => Some(centered_rect(
                API_KEY_MODAL_WIDTH_PCT,
                API_KEY_MODAL_HEIGHT_PCT,
                area,
            )),
            ModalState::ProviderSelect { .. } => Some(centered_rect(
                PROVIDER_SELECT_WIDTH_PCT,
                PROVIDER_SELECT_HEIGHT_PCT,
                area,
            )),
            ModalState::ModelSelect { .. } => Some(centered_rect(
                MODEL_SELECT_WIDTH_PCT,
                MODEL_SELECT_HEIGHT_PCT,
                area,
            )),
            ModalState::UndoCheckpoint { .. } => Some(centered_rect(
                UNDO_CHECKPOINT_WIDTH_PCT,
                UNDO_CHECKPOINT_HEIGHT_PCT,
                area,
            )),
            ModalState::SessionBrowser { .. } => {
                let width = (area.width.saturating_sub(4))
                    .clamp(SESSION_BROWSER_MIN_WIDTH, SESSION_BROWSER_MAX_WIDTH);
                let height = (area.height.saturating_sub(4))
                    .clamp(SESSION_BROWSER_MIN_HEIGHT, SESSION_BROWSER_MAX_HEIGHT);
                Some(centered_rect_exact(width, height, area))
            }
            ModalState::StackSelect { .. } => Some(centered_rect(
                STACK_SELECT_WIDTH_PCT,
                STACK_SELECT_HEIGHT_PCT,
                area,
            )),
            ModalState::CommandCatalog { .. } => Some(centered_rect(
                COMMAND_CATALOG_WIDTH_PCT,
                COMMAND_CATALOG_HEIGHT_PCT,
                area,
            )),
            ModalState::CodeExplorer { .. } => Some(centered_rect(
                CODE_EXPLORER_WIDTH_PCT,
                CODE_EXPLORER_HEIGHT_PCT,
                area,
            )),
            ModalState::GitDiff { .. } => {
                Some(centered_rect(GIT_DIFF_WIDTH_PCT, GIT_DIFF_HEIGHT_PCT, area))
            }
            ModalState::Help => Some(centered_rect(HELP_WIDTH_PCT, HELP_HEIGHT_PCT, area)),
            ModalState::StreamingSelect { .. } => Some(centered_rect(
                PROVIDER_SELECT_WIDTH_PCT.max(52),
                STREAMING_SELECT_HEIGHT_PCT,
                area,
            )),
            ModalState::WorkspaceAnalysis { .. } => Some(centered_rect_exact(
                WORKSPACE_ANALYSIS_WIDTH,
                WORKSPACE_ANALYSIS_HEIGHT,
                area,
            )),
            ModalState::ArchitectureAudit { .. } => Some(centered_rect(
                ARCHITECTURE_MODAL_WIDTH_PCT,
                ARCHITECTURE_MODAL_HEIGHT_PCT,
                area,
            )),
            ModalState::ContextDiagnostics { .. } => Some(centered_rect(
                CONTEXT_MODAL_WIDTH_PCT,
                CONTEXT_MODAL_HEIGHT_PCT,
                area,
            )),
            ModalState::MiniPower(_) => Some(centered_rect(82, 80, area)),
            ModalState::Blocks(_) => Some(centered_rect(90, 85, area)),
            ModalState::Processes(_) | ModalState::Vault(_) => Some(centered_rect(92, 86, area)),
            ModalState::CodeReview(_) => Some(centered_rect(82, 78, area)),
            ModalState::Doctor(_) => Some(centered_rect(92, 88, area)),
            ModalState::None
            | ModalState::Approval(_)
            | ModalState::Inquiry(_)
            | ModalState::ExitConfirm { .. }
            | ModalState::ConfigApproval { .. }
            | ModalState::ProviderSetupRequired { .. } => None,
        }
    }

    pub fn render(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let _ = self.render_with_nav(frame, area, theme, &ModalNavInfo::default());
    }

    pub fn render_with_nav(
        &self,
        frame: &mut Frame,
        area: Rect,
        theme: &Theme,
        nav: &ModalNavInfo,
    ) -> ModalNavRegions {
        match self {
            ModalState::None => return ModalNavRegions::default(),
            ModalState::ApiKeyInput {
                provider,
                env_var,
                input,
                ..
            } => {
                api_key::render_api_key_input(frame, area, theme, provider, env_var, input);
            }
            ModalState::ProviderSelect {
                providers,
                selected_index,
            } => {
                provider_select::render_provider_select_with_active(
                    frame,
                    area,
                    theme,
                    providers,
                    *selected_index,
                    nav.active_provider.as_deref(),
                    nav.parent_label.as_deref(),
                );
            }
            ModalState::ModelSelect {
                provider,
                models,
                filtered_indices,
                selected_index,
                filter,
                loading,
            } => {
                model_select::render_model_select_with_active(
                    frame,
                    area,
                    theme,
                    provider,
                    models,
                    filtered_indices,
                    *selected_index,
                    filter,
                    *loading,
                    nav.active_provider.as_deref(),
                    nav.active_model.as_deref(),
                    nav.parent_label.as_deref(),
                );
            }
            ModalState::UndoCheckpoint {
                checkpoints,
                selected_index,
            } => {
                undo_checkpoint::render_undo_checkpoint(
                    frame,
                    area,
                    theme,
                    checkpoints,
                    *selected_index,
                );
            }
            ModalState::ThemeSelect {
                themes,
                animations,
                active_tab,
                theme_selected_index,
                animation_selected_index,
                todo_style_selected_index,
                swarm_style_selected_index,
                toast_style_selected_index,
                toast_symbol_selected_index,
                activity_style_selected_index,
                menu_style_selected_index,
                input_box_style_selected_index,
                status_bar_style_selected_index,
                status_bar_target,
                context_meter_style_selected_index,
                mode_bar_style_selected_index,
                activity_hint_style_selected_index,
                tool_card_style_selected_index,
                tool_icon_style_selected_index,
                output_frame_style_selected_index,
                output_style_selected_index,
                output_badge_style_selected_index,
                turn_summary_style_selected_index,
                doctor_style_selected_index,
                visibility_selected_index,
                active_theme_id,
                active_animation_id,
                active_todo_style,
                active_swarm_style,
                active_toast_style,
                active_toast_symbols,
                active_activity_style,
                active_menu_style,
                ui_prefs,
            } => {
                let ctx = theme_select::ThemeSelectContext {
                    themes,
                    animations,
                    todo_styles: theme_select::TODO_STYLE_OPTIONS,
                    swarm_styles: theme_select::SWARM_STYLE_OPTIONS,
                    toast_styles: theme_select::TOAST_STYLE_OPTIONS,
                    toast_symbols: theme_select::TOAST_SYMBOL_OPTIONS,
                    activity_styles: theme_select::ACTIVITY_STYLE_OPTIONS,
                    menu_styles: theme_select::MENU_STYLE_OPTIONS,
                    input_box_styles: theme_select::INPUT_BOX_STYLE_OPTIONS,
                    status_bar_styles: theme_select::STATUS_BAR_STYLE_OPTIONS,
                    context_meter_styles: theme_select::CONTEXT_METER_STYLE_OPTIONS,
                    mode_bar_styles: theme_select::MODE_BAR_STYLE_OPTIONS,
                    activity_hint_styles: theme_select::ACTIVITY_HINT_STYLE_OPTIONS,
                    tool_card_styles: theme_select::TOOL_CARD_STYLE_OPTIONS,
                    tool_icon_styles: theme_select::TOOL_ICON_STYLE_OPTIONS,
                    output_frame_styles: theme_select::OUTPUT_FRAME_STYLE_OPTIONS,
                    output_styles: theme_select::OUTPUT_STYLE_OPTIONS,
                    output_badge_styles: theme_select::OUTPUT_BADGE_STYLE_OPTIONS,
                    turn_summary_styles: theme_select::TURN_SUMMARY_STYLE_OPTIONS,
                    doctor_styles: theme_select::DOCTOR_STYLE_OPTIONS,
                    active_tab: *active_tab,
                    theme_selected_index: *theme_selected_index,
                    animation_selected_index: *animation_selected_index,
                    todo_style_selected_index: *todo_style_selected_index,
                    swarm_style_selected_index: *swarm_style_selected_index,
                    toast_style_selected_index: *toast_style_selected_index,
                    toast_symbol_selected_index: *toast_symbol_selected_index,
                    activity_style_selected_index: *activity_style_selected_index,
                    menu_style_selected_index: *menu_style_selected_index,
                    input_box_style_selected_index: *input_box_style_selected_index,
                    status_bar_style_selected_index: *status_bar_style_selected_index,
                    status_bar_target: *status_bar_target,
                    context_meter_style_selected_index: *context_meter_style_selected_index,
                    mode_bar_style_selected_index: *mode_bar_style_selected_index,
                    activity_hint_style_selected_index: *activity_hint_style_selected_index,
                    tool_card_style_selected_index: *tool_card_style_selected_index,
                    tool_icon_style_selected_index: *tool_icon_style_selected_index,
                    output_frame_style_selected_index: *output_frame_style_selected_index,
                    output_style_selected_index: *output_style_selected_index,
                    output_badge_style_selected_index: *output_badge_style_selected_index,
                    turn_summary_style_selected_index: *turn_summary_style_selected_index,
                    doctor_style_selected_index: *doctor_style_selected_index,
                    visibility_selected_index: *visibility_selected_index,
                    active_theme_id,
                    active_animation_id,
                    active_todo_style,
                    active_swarm_style,
                    active_toast_style,
                    active_toast_symbols,
                    active_activity_style,
                    active_menu_style,
                    ui_prefs,
                    parent_label: nav.parent_label.as_deref(),
                };
                theme_select::render_theme_select(frame, area, theme, &ctx);
            }
            ModalState::SessionBrowser {
                sessions,
                selected_index,
                cached_summary,
            } => {
                session_browser::render_session_browser(
                    frame,
                    area,
                    theme,
                    sessions,
                    *selected_index,
                    cached_summary.as_ref(),
                );
            }
            ModalState::StreamingSelect {
                selected_index,
                current_streaming,
            } => {
                streaming_select::render_streaming_select(
                    frame,
                    area,
                    theme,
                    *selected_index,
                    *current_streaming,
                );
            }
            ModalState::Help => {
                help::render_help(frame, area, theme);
            }
            ModalState::StackSelect {
                stacks,
                filtered_indices,
                selected_index,
                filter,
            } => {
                stack_select::render_stack_select(
                    frame,
                    area,
                    theme,
                    stacks,
                    filtered_indices,
                    *selected_index,
                    filter,
                );
            }
            ModalState::CommandCatalog {
                filtered_indices,
                selected_index,
                filter,
            } => {
                command_catalog::render_command_catalog(
                    frame,
                    area,
                    theme,
                    filtered_indices,
                    *selected_index,
                    filter,
                );
            }
            ModalState::GitDiff {
                diff_files,
                selected_file_index,
                scroll_offset,
                staged_view,
            } => {
                git_diff::render_git_diff(
                    frame,
                    area,
                    theme,
                    diff_files,
                    *selected_file_index,
                    *scroll_offset,
                    *staged_view,
                );
            }
            ModalState::CodeExplorer {
                symbols,
                filtered_indices,
                selected_index,
                filter,
                active_tab,
            } => {
                code_explorer::render_code_explorer(
                    frame,
                    area,
                    theme,
                    symbols,
                    filtered_indices,
                    *selected_index,
                    filter,
                    *active_tab,
                );
            }
            ModalState::Approval(approval_state) => {
                approval_state.render(frame, area, theme);
            }
            ModalState::WorkspaceAnalysis {
                workspace_path,
                is_indexed,
                cached_symbols_count,
                cached_files_count,
                selected_index,
            } => {
                workspace_analysis::render_workspace_analysis(
                    frame,
                    area,
                    theme,
                    workspace_path,
                    *is_indexed,
                    *cached_symbols_count,
                    *cached_files_count,
                    *selected_index,
                );
            }
            ModalState::ProviderSetupRequired {
                provider_name,
                pending_prompt_preview,
                selected_index,
            } => {
                workspace_analysis::render_provider_setup_required(
                    frame,
                    area,
                    theme,
                    provider_name,
                    pending_prompt_preview,
                    *selected_index,
                );
            }
            ModalState::ExitConfirm {
                workspace_name,
                selected_yes,
            } => {
                exit_confirm::render_exit_confirm(
                    frame,
                    area,
                    theme,
                    workspace_name,
                    *selected_yes,
                );
            }
            ModalState::ArchitectureAudit {
                report,
                active_tab,
                selected_index,
                scroll_offset,
            } => {
                architecture::render_architecture_audit(
                    frame,
                    area,
                    theme,
                    report,
                    *active_tab,
                    *selected_index,
                    *scroll_offset,
                );
            }
            ModalState::ContextDiagnostics {
                data,
                active_tab,
                scroll_offset,
            } => {
                context_diagnostics::render_context_diagnostics(
                    frame,
                    area,
                    theme,
                    data,
                    *active_tab,
                    *scroll_offset,
                );
            }
            ModalState::Settings(state) => {
                settings::render_settings_with_nav(
                    frame,
                    area,
                    theme,
                    state,
                    nav.parent_label.as_deref(),
                );
            }
            ModalState::ConfigApproval {
                proposal,
                selected_index,
            } => {
                settings::render_config_approval(frame, area, theme, proposal, *selected_index);
            }
            ModalState::MiniPower(state) => {
                minipower::render_minipower(frame, state, area, theme);
            }
            ModalState::Blocks(state) => {
                blocks::render_blocks_modal(frame, state, area, theme);
            }
            ModalState::Processes(state) => {
                processes::render_processes_modal(frame, state, area, theme);
            }
            ModalState::Todo(state) => {
                todo::render_todo_modal(frame, area, state, theme);
            }
            ModalState::Vault(state) => {
                vault::render_vault_modal(frame, state, theme);
            }
            ModalState::Inquiry(ref state) => {
                inquiry::render_inquiry_modal(frame, state, theme, area);
            }
            ModalState::CodeReview(ref state) => {
                code_review::render_code_review(frame, area, state, theme);
            }
            ModalState::Doctor(ref state) => {
                doctor::render_doctor_modal(frame, area, state, theme, state.spinner_style);
            }
        }

        if let Some(modal_rect) = self.outer_rect(area) {
            render_modal_nav_buttons(frame, modal_rect, theme, nav)
        } else {
            ModalNavRegions::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use tempfile::TempDir;

    #[test]
    fn test_new_workspace_analysis_unindexed() {
        let temp_dir = TempDir::new().unwrap();
        let modal = ModalState::new_workspace_analysis(temp_dir.path());
        match modal {
            ModalState::WorkspaceAnalysis {
                is_indexed,
                selected_index,
                ..
            } => {
                assert!(!is_indexed);
                assert_eq!(selected_index, 0);
            }
            _ => panic!("Expected WorkspaceAnalysis variant"),
        }
    }

    #[test]
    fn test_workspace_analysis_render_without_panic() {
        let temp_dir = TempDir::new().unwrap();
        let modal = ModalState::new_workspace_analysis(temp_dir.path());
        let theme = Theme::default();
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|f| {
                let area = f.area();
                modal.render(f, area, &theme);
            })
            .unwrap();
    }

    #[test]
    fn test_exit_confirm_initial_state_and_render() {
        let temp_dir = TempDir::new().unwrap();
        let modal = ModalState::new_exit_confirm(temp_dir.path());
        match modal {
            ModalState::ExitConfirm {
                selected_yes,
                ref workspace_name,
            } => {
                assert!(!selected_yes, "Default should be Nope for safety");
                assert!(!workspace_name.is_empty());
            }
            _ => panic!("Expected ExitConfirm variant"),
        }

        let theme = Theme::default();
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|f| {
                let area = f.area();
                modal.render(f, area, &theme);
            })
            .unwrap();

        let modal_yes = ModalState::ExitConfirm {
            workspace_name: "test_ws".to_string(),
            selected_yes: true,
        };
        terminal
            .draw(|f| {
                let area = f.area();
                modal_yes.render(f, area, &theme);
            })
            .unwrap();
    }

    #[test]
    fn test_session_browser_render() {
        let theme = Theme::default();
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        // Test empty
        let empty_modal = ModalState::SessionBrowser {
            sessions: vec![],
            selected_index: 0,
            cached_summary: None,
        };
        terminal
            .draw(|f| {
                let area = f.area();
                empty_modal.render(f, area, &theme);
            })
            .unwrap();

        // Test with sessions
        let meta1 = crate::session::store::SessionMetadata {
            id: "20260831T142903Z-10f7a7e0".to_string(),
            created_at: "2026-08-31T14:29:03Z".to_string(),
            workspace: "minicode".to_string(),
            path: "/tmp/s1.jsonl".to_string(),
            event_count: 42,
            preview: "Fix permission asking modal responsive layout".to_string(),
        };
        let meta2 = crate::session::store::SessionMetadata {
            id: "20260831T142845Z-20a8b9c1".to_string(),
            created_at: "2026-08-31T14:28:45Z".to_string(),
            workspace: "minicode".to_string(),
            path: "/tmp/s2.jsonl".to_string(),
            event_count: 160,
            preview: "Implement floating spotlight command palette".to_string(),
        };

        let populated_modal = ModalState::SessionBrowser {
            sessions: vec![meta1, meta2],
            selected_index: 0,
            cached_summary: None,
        };
        terminal
            .draw(|f| {
                let area = f.area();
                populated_modal.render(f, area, &theme);
            })
            .unwrap();
    }

    #[test]
    fn test_architecture_audit_render() {
        let theme = Theme::default();
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();

        let report = crate::context::governance::ArchitectureReport {
            health_score: 95,
            total_files: 12,
            total_loc: 1500,
            circular_cycles: vec![vec!["mod_a".into(), "mod_b".into()]],
            layer_violations: vec![],
            coupling_metrics: vec![],
            god_files: vec![],
            fan_out_spikes: vec![],
        };

        let modal = ModalState::new_architecture_audit(report);
        terminal
            .draw(|f| {
                let area = f.area();
                modal.render(f, area, &theme);
            })
            .unwrap();
    }

    #[test]
    fn test_provider_select_render() {
        let theme = Theme::default();
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();

        let modal = ModalState::new_provider_select();
        terminal
            .draw(|f| {
                let area = f.area();
                modal.render(f, area, &theme);
            })
            .unwrap();
    }

    #[test]
    fn test_model_select_render() {
        let theme = Theme::default();
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();

        let models = vec![
            crate::agent::models::ModelInfo {
                id: "gemini-2.5-pro".to_string(),
                name: "Gemini 2.5 Pro".to_string(),
                description: None,
                context_length: Some(1_000_000),
                is_free: false,
            },
            crate::agent::models::ModelInfo {
                id: "gemini-2.5-flash".to_string(),
                name: "Gemini 2.5 Flash".to_string(),
                description: None,
                context_length: Some(1_000_000),
                is_free: true,
            },
        ];

        let modal = ModalState::new_model_select("gemini".to_string(), models);
        terminal
            .draw(|f| {
                let area = f.area();
                modal.render(f, area, &theme);
            })
            .unwrap();
    }

    #[test]
    fn test_streaming_select_render() {
        let theme = Theme::default();
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();

        let modal = ModalState::StreamingSelect {
            selected_index: 0,
            current_streaming: true,
        };
        terminal
            .draw(|f| {
                let area = f.area();
                modal.render(f, area, &theme);
            })
            .unwrap();
    }

    #[test]
    fn test_provider_setup_required_initial_state_and_render() {
        let modal = ModalState::new_provider_setup_required(
            "minimax",
            "Please fix the bug in src/auth.rs and add login test",
        );
        match modal {
            ModalState::ProviderSetupRequired {
                ref provider_name,
                ref pending_prompt_preview,
                selected_index,
            } => {
                assert_eq!(provider_name, "minimax");
                assert!(pending_prompt_preview.contains("Please fix the bug"));
                assert_eq!(selected_index, 0);
            }
            _ => panic!("Expected ProviderSetupRequired variant"),
        }

        let theme = Theme::default();
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| {
                let area = f.area();
                modal.render(f, area, &theme);
            })
            .unwrap();
    }

    #[test]
    fn test_new_settings_initial_state_and_render() {
        let temp_dir = TempDir::new().unwrap();
        let config = crate::config::Config::default();
        let modal = ModalState::new_settings(&config, temp_dir.path());
        match modal {
            ModalState::Settings(ref s) => {
                assert_eq!(s.active_tab, settings::SettingsTab::Providers);
                assert_eq!(s.selected_index, 0);
            }
            _ => panic!("Expected Settings variant"),
        }

        let theme = Theme::default();
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| {
                let area = f.area();
                modal.render(f, area, &theme);
            })
            .unwrap();
    }

    #[test]
    fn test_new_config_approval_initial_state_and_render() {
        let proposal = crate::tools::registry::agent_tools::config_tools::ConfigChangeProposal {
            scope: "workspace".to_string(),
            provider: Some("anthropic".to_string()),
            model: Some("claude-3-7-sonnet".to_string()),
            auto_approve: Some(true),
            thinking_budget: Some(4096),
            theme: None,
        };
        let modal = ModalState::new_config_approval(proposal);
        match modal {
            ModalState::ConfigApproval {
                ref proposal,
                selected_index,
            } => {
                assert_eq!(selected_index, 0);
                assert_eq!(proposal.provider.as_deref(), Some("anthropic"));
            }
            _ => panic!("Expected ConfigApproval variant"),
        }

        let theme = Theme::default();
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| {
                let area = f.area();
                modal.render(f, area, &theme);
            })
            .unwrap();
    }

    #[test]
    fn test_new_processes_initial_state_and_render() {
        let temp = tempfile::tempdir().unwrap();
        let modal = ModalState::new_processes(temp.path());
        assert!(modal.is_active());
        assert!(matches!(modal, ModalState::Processes(_)));

        let theme = Theme::default();
        let backend = TestBackend::new(120, 35);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| {
                let area = f.area();
                modal.render(f, area, &theme);
            })
            .unwrap();
    }

    #[test]
    fn test_new_code_review_and_empty_undo_render() {
        let theme = Theme::default();
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();

        let report = crate::git::ReviewReport {
            total_score: 92,
            files_audited: 4,
            summary: "1 medium finding across 4 audited files.".to_string(),
            findings: vec![crate::git::ReviewFinding {
                severity: "MEDIUM".to_string(),
                category: "Error Handling".to_string(),
                title: "Unchecked unwrap() in src/main.rs".to_string(),
                details: "Avoid .unwrap() in production paths.".to_string(),
                line_hint: Some("src/main.rs:18".to_string()),
            }],
        };
        let review_modal = ModalState::new_code_review(report, false);
        terminal
            .draw(|f| {
                let area = f.area();
                review_modal.render(f, area, &theme);
            })
            .unwrap();

        let empty_undo_modal = ModalState::new_undo_checkpoint(vec![]);
        terminal
            .draw(|f| {
                let area = f.area();
                empty_undo_modal.render(f, area, &theme);
            })
            .unwrap();
    }
}

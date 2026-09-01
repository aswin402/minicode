use crate::agent::models::ModelInfo;
use crate::constants::{
    CHECKPOINT_FILES_PREVIEW, CHECKPOINT_PROMPT_MAX_CHARS, CHECKPOINT_PROMPT_PREVIEW_CHARS,
    CODE_EXPLORER_HEIGHT_PCT, CODE_EXPLORER_WIDTH_PCT, COMMAND_CATALOG_HEIGHT_PCT,
    COMMAND_CATALOG_WIDTH_PCT, COMMAND_NAME_DISPLAY_COLS, EXIT_CONFIRM_MODAL_HEIGHT,
    EXIT_CONFIRM_MODAL_WIDTH, EXPLORER_BADGE_DISPLAY_COLS, GIT_DIFF_HEIGHT_PCT, GIT_DIFF_WIDTH_PCT,
    HELP_HEIGHT_PCT, HELP_WIDTH_PCT, MODEL_SELECT_HEIGHT_PCT, MODEL_SELECT_WIDTH_PCT,
    PROVIDER_SELECT_HEIGHT_PCT, PROVIDER_SELECT_WIDTH_PCT, SESSION_BROWSER_MAX_HEIGHT,
    SESSION_BROWSER_MAX_WIDTH, SESSION_BROWSER_MIN_HEIGHT, SESSION_BROWSER_MIN_WIDTH,
    SESSION_ID_DISPLAY_COLS, SESSION_LIST_ITEM_HEIGHT, SESSION_TIME_AGO_COLS,
    STACK_PREVIEW_MAX_FILES, STACK_SELECT_HEIGHT_PCT, STACK_SELECT_WIDTH_PCT,
    THEME_MODAL_MAX_VISIBLE, THEME_NAME_DISPLAY_COLS, THEME_SELECT_HEIGHT_PCT,
    THEME_SELECT_WIDTH_PCT, UNDO_CHECKPOINT_HEIGHT_PCT, UNDO_CHECKPOINT_MAX_VISIBLE,
    UNDO_CHECKPOINT_WIDTH_PCT, WORKSPACE_ANALYSIS_HEIGHT, WORKSPACE_ANALYSIS_WIDTH,
};
use crate::session::store::truncate_display;
use crate::ui::theme::Theme;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Margin, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph};
use ratatui::Frame;
use unicode_width::UnicodeWidthStr;

#[derive(Debug, Clone)]
pub struct TurnCheckpointInfo {
    pub turn_id: usize,
    pub prompt: String,
    #[allow(dead_code)]
    pub timestamp: String,
    pub time_ago: String,
    pub files: Vec<String>,
    pub is_latest: bool,
}

pub fn format_time_ago(rfc3339_ts: &str) -> String {
    if let Ok(ts) = chrono::DateTime::parse_from_rfc3339(rfc3339_ts) {
        let now = chrono::Utc::now();
        let diff = now.signed_duration_since(ts.with_timezone(&chrono::Utc));
        let secs = diff.num_seconds();
        if secs < 60 {
            format!("{}s ago", secs.max(1))
        } else if secs < 3600 {
            format!("{}m ago", secs / 60)
        } else if secs < 86400 {
            format!("{}h ago", secs / 3600)
        } else {
            format!("{}d ago", secs / 86400)
        }
    } else {
        "recently".to_string()
    }
}

#[derive(Debug, Clone)]
pub struct CommandCatalogItem {
    pub name: &'static str,
    pub category: &'static str,
    pub shortcut: &'static str,
    pub description: &'static str,
    pub example: &'static str,
}

pub const COMMAND_CATALOG_ITEMS: &[CommandCatalogItem] = &[
    CommandCatalogItem {
        name: "/commands",
        category: "Discovery & Help",
        shortcut: "",
        description: "Interactive catalog of all available slash commands & keybindings",
        example: "/commands",
    },
    CommandCatalogItem {
        name: "/new",
        category: "History & Sessions",
        shortcut: "Ctrl+N",
        description: "Start fresh session & reset conversation timeline",
        example: "/new",
    },
    CommandCatalogItem {
        name: "/configure",
        category: "Config & Runtime",
        shortcut: "F2",
        description: "Interactive API key & endpoint manager for providers",
        example: "/configure | /init",
    },
    CommandCatalogItem {
        name: "/index",
        category: "Code & Inspection",
        shortcut: "F5",
        description: "Scan AST symbols & build PageRank code graph",
        example: "/index | /analyze",
    },
    CommandCatalogItem {
        name: "/stack",
        category: "Workflows & Scaffolding",
        shortcut: "",
        description: "Interactive multi-runtime stack wizard & template scaffolder (onpkg)",
        example: "/stack nextjs | /stack react-vite",
    },
    CommandCatalogItem {
        name: "/plan",
        category: "Workflows & Scaffolding",
        shortcut: "",
        description: "Break complex feature into verifiable milestones in todo.md & plan.md",
        example: "/plan implement OAuth2 auth flow",
    },
    CommandCatalogItem {
        name: "/goal",
        category: "Workflows & Scaffolding",
        shortcut: "",
        description: "Autonomous goal execution loop until all checklist tasks pass",
        example: "/goal refactor database models and pass tests",
    },
    CommandCatalogItem {
        name: "/diff",
        category: "Code & Inspection",
        shortcut: "Ctrl+D",
        description: "Interactive split/unified git diff viewer & staging explorer",
        example: "/diff",
    },
    CommandCatalogItem {
        name: "/explore",
        category: "Code & Inspection",
        shortcut: "Ctrl+E",
        description: "Surgically explore codebase AST symbols, call graph & blast radius",
        example: "/explore | /explore auth",
    },
    CommandCatalogItem {
        name: "/review",
        category: "Code & Inspection",
        shortcut: "Ctrl+R",
        description: "Multi-dimensional adversarial code review on git changes",
        example: "/review | /review --staged",
    },
    CommandCatalogItem {
        name: "/map",
        category: "Code & Inspection",
        shortcut: "",
        description: "Render AST PageRank repository hierarchy & dependency graph",
        example: "/map",
    },
    CommandCatalogItem {
        name: "/sessions",
        category: "History & Sessions",
        shortcut: "Ctrl+H",
        description: "Interactive session history & time-travel explorer",
        example: "/sessions | /history",
    },
    CommandCatalogItem {
        name: "/undo",
        category: "History & Sessions",
        shortcut: "Ctrl+U",
        description: "Interactive checkpoint browser to revert file modifications",
        example: "/undo",
    },
    CommandCatalogItem {
        name: "/export",
        category: "History & Sessions",
        shortcut: "",
        description: "Export current session trajectory to a markdown report",
        example: "/export report.md",
    },
    CommandCatalogItem {
        name: "/save",
        category: "History & Sessions",
        shortcut: "",
        description: "Export full conversation history transcript to a target file",
        example: "/save session_backup.md",
    },
    CommandCatalogItem {
        name: "/load",
        category: "History & Sessions",
        shortcut: "",
        description: "Load and replay past session events into active timeline",
        example: "/load <session_id>",
    },
    CommandCatalogItem {
        name: "/model",
        category: "Config & Runtime",
        shortcut: "Ctrl+L",
        description: "Choose active LLM model & provider interactively",
        example: "/model",
    },
    CommandCatalogItem {
        name: "/theme",
        category: "Config & Runtime",
        shortcut: "",
        description: "Switch TUI color theme (Aura, Tokyo Night, Nord, Gruvbox, etc.)",
        example: "/theme",
    },
    CommandCatalogItem {
        name: "/tokens",
        category: "Config & Runtime",
        shortcut: "",
        description: "Display detailed token usage breakdown and compaction limits",
        example: "/tokens",
    },
    CommandCatalogItem {
        name: "/compact",
        category: "Config & Runtime",
        shortcut: "",
        description: "Manually compact and prune context tokens",
        example: "/compact",
    },
    CommandCatalogItem {
        name: "/terminal",
        category: "Utilities",
        shortcut: "Ctrl+T",
        description: "Toggle interactive embedded PTY terminal drawer",
        example: "/terminal",
    },
    CommandCatalogItem {
        name: "/copy",
        category: "Utilities",
        shortcut: "",
        description: "Copy latest assistant response or full transcript to clipboard",
        example: "/copy | /copy all",
    },
    CommandCatalogItem {
        name: "/retry",
        category: "Utilities",
        shortcut: "",
        description: "Re-submit the previous user prompt to the agent",
        example: "/retry",
    },
    CommandCatalogItem {
        name: "/clear",
        category: "Utilities",
        shortcut: "",
        description: "Clear active conversation timeline display",
        example: "/clear",
    },
    CommandCatalogItem {
        name: "/help",
        category: "Discovery & Help",
        shortcut: "F1",
        description: "Display quick help and keyboard shortcuts",
        example: "/help",
    },
    CommandCatalogItem {
        name: "/exit",
        category: "Discovery & Help",
        shortcut: "Ctrl+C",
        description: "Quit minicode interactive session cleanly",
        example: "/exit",
    },
];

#[derive(Debug, Clone)]
pub enum ModalState {
    None,
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
        selected_index: usize,
    },
    SessionBrowser {
        sessions: Vec<crate::session::store::SessionMetadata>,
        selected_index: usize,
        cached_summary: Option<crate::session::store::SessionSummary>,
    },
    StackSelect {
        stacks: Vec<crate::tools::onpkg::stacks::Stack>,
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
    Approval(crate::ui::approval::ApprovalModalState),
    WorkspaceAnalysis {
        workspace_path: String,
        is_indexed: bool,
        cached_symbols_count: usize,
        cached_files_count: usize,
        selected_index: usize,
    },
    ExitConfirm {
        workspace_name: String,
        selected_yes: bool,
    },
}

impl ModalState {
    pub fn is_active(&self) -> bool {
        !matches!(self, ModalState::None)
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

    pub fn new_provider_select() -> Self {
        let providers = vec![
            "openrouter".to_string(),
            "gemini".to_string(),
            "openai".to_string(),
            "deepseek".to_string(),
            "groq".to_string(),
            "minimax".to_string(),
            "z.ai".to_string(),
            "together".to_string(),
            "mistral".to_string(),
            "ollama".to_string(),
        ];
        ModalState::ProviderSelect {
            providers,
            selected_index: 0,
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

    pub fn new_theme_select(active_theme_id: &str) -> Self {
        let themes = crate::ui::theme::Theme::list_themes();
        let selected_index = themes
            .iter()
            .position(|t| t.id == active_theme_id || active_theme_id.starts_with(&t.id))
            .unwrap_or(0);
        ModalState::ThemeSelect {
            themes,
            selected_index,
        }
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
        let stacks = crate::tools::onpkg::scaffolder::OnpkgScaffolder::get_all_stacks();
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

    pub fn render(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        match self {
            ModalState::None => {}
            ModalState::ProviderSelect {
                providers,
                selected_index,
            } => {
                let popup_area =
                    centered_rect(PROVIDER_SELECT_WIDTH_PCT, PROVIDER_SELECT_HEIGHT_PCT, area);
                frame.render_widget(Clear, popup_area);

                let block = Block::default()
                    .title(" Select Provider ")
                    .title_alignment(Alignment::Center)
                    .borders(Borders::ALL)
                    .border_style(
                        Style::default()
                            .fg(theme.brand_accent)
                            .bg(theme.bg_elevated),
                    )
                    .style(Style::default().bg(theme.bg_elevated));

                let inner_area = block.inner(popup_area);
                let max_visible = (inner_area.height as usize).max(1);
                let scroll_offset = if max_visible > 0 && *selected_index >= max_visible {
                    selected_index.saturating_sub(max_visible.saturating_sub(1))
                } else {
                    0
                };

                let items: Vec<ListItem> = providers
                    .iter()
                    .enumerate()
                    .skip(scroll_offset)
                    .take(max_visible)
                    .map(|(i, p)| {
                        let is_selected = i == *selected_index;
                        let prefix = if is_selected { " › " } else { "   " };
                        let style = if is_selected {
                            Style::default()
                                .fg(theme.bg_primary)
                                .bg(theme.brand_accent)
                                .add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().fg(theme.text_primary)
                        };
                        ListItem::new(format!("{}{}", prefix, p)).style(style)
                    })
                    .collect();

                let list = List::new(items)
                    .block(block)
                    .highlight_style(Style::default().bg(theme.brand_accent));

                frame.render_widget(list, popup_area);
            }
            ModalState::ModelSelect {
                provider,
                models,
                filtered_indices,
                selected_index,
                filter,
                loading,
            } => {
                let popup_area =
                    centered_rect(MODEL_SELECT_WIDTH_PCT, MODEL_SELECT_HEIGHT_PCT, area);
                frame.render_widget(Clear, popup_area);

                let outer_block = Block::default()
                    .title(format!(" Select Model ({}) ", provider))
                    .title_alignment(Alignment::Center)
                    .borders(Borders::ALL)
                    .border_style(
                        Style::default()
                            .fg(theme.brand_accent)
                            .bg(theme.bg_elevated),
                    )
                    .style(Style::default().bg(theme.bg_elevated));

                let inner_area = outer_block.inner(popup_area);
                frame.render_widget(outer_block, popup_area);

                if *loading {
                    let loading_p =
                        Paragraph::new(format!("Fetching live models from {} API...", provider))
                            .style(Style::default().fg(theme.warning))
                            .alignment(Alignment::Center);
                    frame.render_widget(loading_p, inner_area);
                    return;
                }

                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Length(3), // Search / filter input
                        Constraint::Min(5),    // Models list
                    ])
                    .split(inner_area);

                // Search box
                let search_text = format!(" Search: {}█", filter);
                let search_box = Paragraph::new(search_text)
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .border_style(Style::default().fg(theme.border))
                            .title(" Filter Models "),
                    )
                    .style(Style::default().fg(theme.text_primary));
                frame.render_widget(search_box, chunks[0]);

                // Models list with viewport scrolling
                let max_visible = chunks[1].height as usize;
                let scroll_offset = if max_visible > 0 && *selected_index >= max_visible {
                    selected_index.saturating_sub(max_visible.saturating_sub(1))
                } else {
                    0
                };

                let items: Vec<ListItem> = filtered_indices
                    .iter()
                    .enumerate()
                    .skip(scroll_offset)
                    .take(max_visible)
                    .map(|(visual_idx, &real_idx)| {
                        let m = &models[real_idx];
                        let is_selected = visual_idx == *selected_index;
                        let prefix = if is_selected { " › " } else { "   " };

                        let mut spans = vec![
                            Span::raw(prefix),
                            Span::styled(
                                &m.id,
                                if is_selected {
                                    Style::default()
                                        .fg(theme.bg_primary)
                                        .bg(theme.brand_accent)
                                        .add_modifier(Modifier::BOLD)
                                } else {
                                    Style::default().fg(theme.text_primary)
                                },
                            ),
                        ];

                        if m.is_free {
                            spans.push(Span::styled(
                                " [FREE]",
                                Style::default()
                                    .fg(theme.success)
                                    .add_modifier(Modifier::BOLD),
                            ));
                        }

                        if let Some(ctx) = m.context_length {
                            spans.push(Span::styled(
                                format!(" ({}k ctx)", ctx / 1000),
                                Style::default().fg(theme.muted),
                            ));
                        }

                        let item_style = if is_selected {
                            Style::default().bg(theme.brand_accent)
                        } else {
                            Style::default()
                        };

                        ListItem::new(Line::from(spans)).style(item_style)
                    })
                    .collect();

                let list = List::new(items);
                frame.render_widget(list, chunks[1]);
            }
            ModalState::UndoCheckpoint {
                checkpoints,
                selected_index,
            } => {
                let popup_area =
                    centered_rect(UNDO_CHECKPOINT_WIDTH_PCT, UNDO_CHECKPOINT_HEIGHT_PCT, area);
                frame.render_widget(Clear, popup_area);

                let mut lines = Vec::new();
                lines.push(Line::from(""));

                let total = checkpoints.len();
                let max_visible = UNDO_CHECKPOINT_MAX_VISIBLE;
                let scroll_offset = if max_visible > 0 && *selected_index >= max_visible {
                    selected_index.saturating_sub(max_visible.saturating_sub(1))
                } else {
                    0
                };
                let visible_checkpoints = checkpoints.iter().skip(scroll_offset).take(max_visible);

                for (idx_rel, cp) in visible_checkpoints.enumerate() {
                    let idx = scroll_offset + idx_rel;
                    let is_selected = idx == *selected_index;
                    let is_last = idx == total - 1;

                    let node_sym = if is_selected {
                        "  ◉─ "
                    } else {
                        "  ○─ "
                    };
                    let node_style = if is_selected {
                        Style::default()
                            .fg(theme.brand_accent)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(theme.muted)
                    };

                    let turn_badge = if cp.is_latest {
                        format!("[Turn {}] (Latest)", cp.turn_id)
                    } else {
                        format!("[Turn {}]", cp.turn_id)
                    };

                    let badge_style = if is_selected {
                        Style::default()
                            .fg(theme.success)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(theme.muted)
                    };

                    lines.push(Line::from(vec![
                        Span::styled(node_sym, node_style),
                        Span::styled(turn_badge, badge_style),
                    ]));

                    let prompt_style = if is_selected {
                        Style::default()
                            .fg(theme.text_primary)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(theme.muted)
                    };
                    let prompt_display = if cp.prompt.chars().count() > CHECKPOINT_PROMPT_MAX_CHARS
                    {
                        format!(
                            "{}...",
                            cp.prompt
                                .chars()
                                .take(CHECKPOINT_PROMPT_PREVIEW_CHARS)
                                .collect::<String>()
                        )
                    } else {
                        cp.prompt.clone()
                    };

                    lines.push(Line::from(vec![
                        Span::styled(
                            "  │   ",
                            if is_last {
                                Style::default().fg(theme.bg_elevated)
                            } else {
                                Style::default().fg(theme.muted)
                            },
                        ),
                        Span::styled(format!("\"{}\"", prompt_display), prompt_style),
                    ]));

                    let file_summary = if cp.files.is_empty() {
                        "0 files modified".to_string()
                    } else {
                        let files_str: Vec<&str> = cp
                            .files
                            .iter()
                            .map(|p| {
                                std::path::Path::new(p)
                                    .file_name()
                                    .and_then(|n| n.to_str())
                                    .unwrap_or(p)
                            })
                            .take(CHECKPOINT_FILES_PREVIEW)
                            .collect();
                        let more = if cp.files.len() > CHECKPOINT_FILES_PREVIEW {
                            format!(" +{} more", cp.files.len() - CHECKPOINT_FILES_PREVIEW)
                        } else {
                            String::new()
                        };
                        format!(
                            "{} file(s) ({}{})",
                            cp.files.len(),
                            files_str.join(", "),
                            more
                        )
                    };

                    let meta_text = format!("└─ {} • {}", cp.time_ago, file_summary);
                    lines.push(Line::from(vec![
                        Span::styled(
                            "  │   ",
                            if is_last {
                                Style::default().fg(theme.bg_elevated)
                            } else {
                                Style::default().fg(theme.muted)
                            },
                        ),
                        Span::styled(
                            meta_text,
                            Style::default().fg(if is_selected {
                                theme.brand_accent
                            } else {
                                theme.muted
                            }),
                        ),
                    ]));

                    if !is_last {
                        lines.push(Line::from(vec![Span::styled(
                            "  │",
                            Style::default().fg(theme.muted),
                        )]));
                    }
                }

                let block = Block::default()
                    .title(" Undo to Checkpoint ")
                    .title_alignment(Alignment::Center)
                    .borders(Borders::ALL)
                    .border_style(
                        Style::default()
                            .fg(theme.brand_accent)
                            .bg(theme.bg_elevated),
                    )
                    .style(Style::default().bg(theme.bg_elevated));

                let p = Paragraph::new(lines).block(block);
                frame.render_widget(p, popup_area);
            }
            ModalState::ThemeSelect {
                themes,
                selected_index,
            } => {
                let popup_area =
                    centered_rect(THEME_SELECT_WIDTH_PCT, THEME_SELECT_HEIGHT_PCT, area);
                frame.render_widget(Clear, popup_area);

                let mut lines = Vec::new();
                lines.push(Line::from(""));

                let max_visible = THEME_MODAL_MAX_VISIBLE;
                let scroll_offset = if max_visible > 0 && *selected_index >= max_visible {
                    selected_index.saturating_sub(max_visible.saturating_sub(1))
                } else {
                    0
                };
                let visible_themes = themes.iter().skip(scroll_offset).take(max_visible);

                for (idx_rel, t) in visible_themes.enumerate() {
                    let idx = scroll_offset + idx_rel;
                    let is_selected = idx == *selected_index;

                    let cursor = if is_selected { "  ❯ " } else { "    " };
                    let title_style = if is_selected {
                        Style::default()
                            .fg(theme.brand_accent)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(theme.text_primary)
                    };

                    let mut header_spans = vec![
                        Span::styled(
                            cursor,
                            Style::default()
                                .fg(theme.brand_accent)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(format!("[{}] ", idx + 1), Style::default().fg(theme.muted)),
                        Span::styled(
                            format!("{:<width$}", t.name, width = THEME_NAME_DISPLAY_COLS),
                            title_style,
                        ),
                    ];

                    for c in &t.swatches {
                        header_spans.push(Span::styled(" ■", Style::default().fg(*c)));
                    }

                    lines.push(Line::from(header_spans));

                    let desc_style = if is_selected {
                        Style::default().fg(theme.text_primary)
                    } else {
                        Style::default().fg(theme.muted)
                    };
                    lines.push(Line::from(vec![
                        Span::styled("        ", Style::default()),
                        Span::styled(&t.description, desc_style),
                    ]));

                    lines.push(Line::from(""));
                }

                let block = Block::default()
                    .title(" Theme Switcher ")
                    .title_alignment(Alignment::Center)
                    .borders(Borders::ALL)
                    .border_style(
                        Style::default()
                            .fg(theme.brand_accent)
                            .bg(theme.bg_elevated),
                    )
                    .style(Style::default().bg(theme.bg_elevated));

                let p = Paragraph::new(lines).block(block);
                frame.render_widget(p, popup_area);
            }
            ModalState::SessionBrowser {
                sessions,
                selected_index,
                cached_summary: _,
            } => {
                let width = (area.width.saturating_sub(4))
                    .clamp(SESSION_BROWSER_MIN_WIDTH, SESSION_BROWSER_MAX_WIDTH);
                let height = (area.height.saturating_sub(4))
                    .clamp(SESSION_BROWSER_MIN_HEIGHT, SESSION_BROWSER_MAX_HEIGHT);
                let popup_area = centered_rect_exact(width, height, area);
                frame.render_widget(Clear, popup_area);

                let total_sessions = sessions.len();
                let index_badge = if total_sessions > 0 {
                    format!(" [{}/{}] ", *selected_index + 1, total_sessions)
                } else {
                    " [0/0] ".to_string()
                };

                let outer_block = Block::default()
                    .title(Line::from(vec![
                        Span::styled(
                            " 📜 Session History ",
                            Style::default()
                                .fg(theme.brand_accent)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(index_badge, Style::default().fg(theme.muted)),
                    ]))
                    .title_alignment(Alignment::Left)
                    .borders(Borders::ALL)
                    .border_type(ratatui::widgets::BorderType::Rounded)
                    .border_style(Style::default().fg(theme.brand_accent))
                    .style(Style::default().bg(theme.bg_elevated));

                let inner_area = outer_block.inner(popup_area);
                frame.render_widget(outer_block, popup_area);

                if sessions.is_empty() {
                    let empty = Paragraph::new(Line::from(vec![Span::styled(
                        "No past sessions found in this workspace.",
                        Style::default().fg(theme.muted),
                    )]))
                    .alignment(Alignment::Center);
                    frame.render_widget(empty, inner_area);
                } else {
                    let available_height = inner_area.height as usize;
                    // Each item takes SESSION_LIST_ITEM_HEIGHT lines
                    let max_visible = (available_height / SESSION_LIST_ITEM_HEIGHT).max(1);
                    let scroll_offset = if max_visible > 0 && *selected_index >= max_visible {
                        selected_index.saturating_sub(max_visible.saturating_sub(1))
                    } else {
                        0
                    };
                    let visible_sessions = sessions
                        .iter()
                        .enumerate()
                        .skip(scroll_offset)
                        .take(max_visible);

                    let items: Vec<ListItem> = visible_sessions
                        .map(|(i, s)| {
                            let is_selected = i == *selected_index;
                            let time_ago = format_time_ago(&s.created_at);

                            // Determine preview title (prompt or clean fallback)
                            let preview_title = if !s.preview.is_empty() {
                                s.preview.replace('\n', " ")
                            } else {
                                format!("Session {}", s.id)
                            };

                            let event_text = if s.event_count == 0 {
                                "0 events".to_string()
                            } else if s.event_count == 1 {
                                "1 event".to_string()
                            } else {
                                format!("{} events", s.event_count)
                            };

                            let id_short = truncate_display(&s.id, SESSION_ID_DISPLAY_COLS, "…");

                            let (line1, line2) = if is_selected {
                                let time_style = Style::default()
                                    .fg(theme.brand_accent)
                                    .add_modifier(Modifier::BOLD);
                                let title_style = Style::default()
                                    .fg(theme.text_primary)
                                    .add_modifier(Modifier::BOLD);
                                let l1 = Line::from(vec![
                                    Span::styled(
                                        " › ",
                                        Style::default()
                                            .fg(theme.brand_accent)
                                            .add_modifier(Modifier::BOLD),
                                    ),
                                    Span::styled(
                                        format!(
                                            "{:<width$} ",
                                            time_ago,
                                            width = SESSION_TIME_AGO_COLS
                                        ),
                                        time_style,
                                    ),
                                    Span::styled(preview_title, title_style),
                                ]);

                                let l2 = Line::from(vec![
                                    Span::raw("     "),
                                    Span::styled("● ", Style::default().fg(theme.success)),
                                    Span::styled(
                                        format!("{}  •  id: {}", event_text, id_short),
                                        Style::default().fg(theme.muted),
                                    ),
                                ]);
                                (l1, l2)
                            } else {
                                let time_style = Style::default().fg(theme.muted);
                                let title_style = Style::default().fg(theme.text_primary);
                                let l1 = Line::from(vec![
                                    Span::raw("   "),
                                    Span::styled(
                                        format!(
                                            "{:<width$} ",
                                            time_ago,
                                            width = SESSION_TIME_AGO_COLS
                                        ),
                                        time_style,
                                    ),
                                    Span::styled(preview_title, title_style),
                                ]);

                                let l2 = Line::from(vec![
                                    Span::raw("     "),
                                    Span::styled("● ", Style::default().fg(theme.muted)),
                                    Span::styled(
                                        format!("{}  •  id: {}", event_text, id_short),
                                        Style::default().fg(theme.muted),
                                    ),
                                ]);
                                (l1, l2)
                            };

                            ListItem::new(vec![line1, line2])
                        })
                        .collect();

                    let list = List::new(items);
                    frame.render_widget(list, inner_area);
                }
            }
            ModalState::Help => {
                let popup_area = centered_rect(HELP_WIDTH_PCT, HELP_HEIGHT_PCT, area);
                frame.render_widget(Clear, popup_area);

                let help_text = vec![
                    Line::from(vec![Span::styled(
                        "⚡ minicode Help & Keyboard Shortcuts",
                        Style::default()
                            .fg(theme.brand_accent)
                            .add_modifier(Modifier::BOLD),
                    )]),
                    Line::from(""),
                    Line::from(vec![
                        Span::styled("  /model     ", Style::default().fg(theme.success)),
                        Span::styled(
                            "Choose LLM model & provider interactively",
                            Style::default().fg(theme.text_primary),
                        ),
                    ]),
                    Line::from(vec![
                        Span::styled("  /theme     ", Style::default().fg(theme.success)),
                        Span::styled(
                            "Switch TUI color theme palette interactively",
                            Style::default().fg(theme.text_primary),
                        ),
                    ]),
                    Line::from(vec![
                        Span::styled("  /undo      ", Style::default().fg(theme.success)),
                        Span::styled(
                            "Revert all file modifications from previous turn",
                            Style::default().fg(theme.text_primary),
                        ),
                    ]),
                    Line::from(vec![
                        Span::styled("  /sessions  ", Style::default().fg(theme.success)),
                        Span::styled(
                            "Browse & reload past workspace session history",
                            Style::default().fg(theme.text_primary),
                        ),
                    ]),
                    Line::from(vec![
                        Span::styled("  /copy      ", Style::default().fg(theme.success)),
                        Span::styled(
                            "Copy latest AI response to clipboard (/copy all for whole chat)",
                            Style::default().fg(theme.text_primary),
                        ),
                    ]),
                    Line::from(vec![
                        Span::styled("  /clear     ", Style::default().fg(theme.success)),
                        Span::styled(
                            "Clear conversation timeline display",
                            Style::default().fg(theme.text_primary),
                        ),
                    ]),
                    Line::from(vec![
                        Span::styled("  /exit      ", Style::default().fg(theme.success)),
                        Span::styled(
                            "Quit minicode interactive session",
                            Style::default().fg(theme.text_primary),
                        ),
                    ]),
                    Line::from(""),
                    Line::from(vec![
                        Span::styled("  Shift+Drag ", Style::default().fg(theme.warning)),
                        Span::styled(
                            "Select and copy text in terminal directly",
                            Style::default().fg(theme.text_primary),
                        ),
                    ]),
                    Line::from(vec![
                        Span::styled("  Enter      ", Style::default().fg(theme.warning)),
                        Span::styled(
                            "Submit prompt or confirm action",
                            Style::default().fg(theme.text_primary),
                        ),
                    ]),
                    Line::from(vec![
                        Span::styled("  Ctrl+J     ", Style::default().fg(theme.warning)),
                        Span::styled(
                            "Insert newline (multi-line prompt)",
                            Style::default().fg(theme.text_primary),
                        ),
                    ]),
                    Line::from(vec![
                        Span::styled("  PgUp / PgDn", Style::default().fg(theme.warning)),
                        Span::styled(
                            "Scroll timeline (or Shift+↑/↓, Mouse wheel)",
                            Style::default().fg(theme.text_primary),
                        ),
                    ]),
                    Line::from(vec![
                        Span::styled("  Home / End ", Style::default().fg(theme.warning)),
                        Span::styled(
                            "Scroll directly to top / bottom of timeline",
                            Style::default().fg(theme.text_primary),
                        ),
                    ]),
                    Line::from(vec![
                        Span::styled("  Ctrl+T     ", Style::default().fg(theme.warning)),
                        Span::styled(
                            "Toggle embedded PTY terminal drawer",
                            Style::default().fg(theme.text_primary),
                        ),
                    ]),
                    Line::from(vec![
                        Span::styled("  Esc        ", Style::default().fg(theme.warning)),
                        Span::styled(
                            "Interrupt running execution / close modal",
                            Style::default().fg(theme.text_primary),
                        ),
                    ]),
                    Line::from(""),
                    Line::from(vec![Span::styled(
                        "Press Esc or Enter to close",
                        Style::default().fg(theme.muted),
                    )]),
                ];

                let block = Block::default()
                    .title(" minicode Help ")
                    .title_alignment(Alignment::Center)
                    .borders(Borders::ALL)
                    .border_style(
                        Style::default()
                            .fg(theme.brand_accent)
                            .bg(theme.bg_elevated),
                    )
                    .style(Style::default().bg(theme.bg_elevated));

                let p = Paragraph::new(help_text).block(block);
                frame.render_widget(p, popup_area);
            }
            ModalState::StackSelect {
                stacks,
                filtered_indices,
                selected_index,
                filter,
            } => {
                let popup_area =
                    centered_rect(STACK_SELECT_WIDTH_PCT, STACK_SELECT_HEIGHT_PCT, area);
                frame.render_widget(Clear, popup_area);

                let outer_block = Block::default()
                    .title(" 📦 Native onpkg Stack Wizard ")
                    .title_alignment(Alignment::Center)
                    .borders(Borders::ALL)
                    .border_style(
                        Style::default()
                            .fg(theme.brand_accent)
                            .bg(theme.bg_elevated),
                    )
                    .style(Style::default().bg(theme.bg_elevated));

                let inner_area = outer_block.inner(popup_area);
                frame.render_widget(outer_block, popup_area);

                let v_chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Length(3), // Filter search input
                        Constraint::Min(6),    // 2-column main area
                    ])
                    .split(inner_area);

                // Search box
                let search_text = format!(" Filter: {}█", filter);
                let search_box = Paragraph::new(search_text)
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .border_style(Style::default().fg(theme.border))
                            .title(" Search Stacks by name / tech / runtime "),
                    )
                    .style(Style::default().fg(theme.text_primary));
                frame.render_widget(search_box, v_chunks[0]);

                // Split middle area horizontally (List vs Preview)
                let h_chunks = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([Constraint::Percentage(42), Constraint::Percentage(58)])
                    .split(v_chunks[1]);

                let max_visible = (h_chunks[0].height.saturating_sub(2) as usize).max(1);
                let scroll_offset = if max_visible > 0 && *selected_index >= max_visible {
                    selected_index.saturating_sub(max_visible.saturating_sub(1))
                } else {
                    0
                };

                // Left: Stacks List with viewport scrolling
                let items: Vec<ListItem> = filtered_indices
                    .iter()
                    .enumerate()
                    .skip(scroll_offset)
                    .take(max_visible)
                    .map(|(visual_idx, &real_idx)| {
                        let s = &stacks[real_idx];
                        let is_selected = visual_idx == *selected_index;
                        let prefix = if is_selected { " › " } else { "   " };

                        let runtime_badge = format!(" [{}]", s.runtime);
                        let file_info = format!(" ({} files)", s.files.len());

                        let spans = vec![
                            Span::raw(prefix),
                            Span::styled(
                                &s.name,
                                if is_selected {
                                    Style::default()
                                        .fg(theme.bg_primary)
                                        .bg(theme.brand_accent)
                                        .add_modifier(Modifier::BOLD)
                                } else {
                                    Style::default().fg(theme.text_primary)
                                },
                            ),
                            Span::styled(
                                runtime_badge,
                                if is_selected {
                                    Style::default().fg(theme.bg_primary).bg(theme.brand_accent)
                                } else {
                                    Style::default().fg(theme.brand_accent)
                                },
                            ),
                            Span::styled(
                                file_info,
                                if is_selected {
                                    Style::default().fg(theme.bg_primary).bg(theme.brand_accent)
                                } else {
                                    Style::default().fg(theme.muted)
                                },
                            ),
                        ];

                        let item_style = if is_selected {
                            Style::default().bg(theme.brand_accent)
                        } else {
                            Style::default()
                        };

                        ListItem::new(Line::from(spans)).style(item_style)
                    })
                    .collect();

                let list_block = Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme.border))
                    .title(format!(" Stacks ({}) ", filtered_indices.len()));
                let list = List::new(items).block(list_block);
                frame.render_widget(list, h_chunks[0]);

                // Right: Preview of Selected Stack
                let preview_block = Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme.border))
                    .title(" Stack Preview ");

                let mut preview_lines = Vec::new();
                if !filtered_indices.is_empty() && *selected_index < filtered_indices.len() {
                    let s = &stacks[filtered_indices[*selected_index]];

                    preview_lines.push(Line::from(vec![
                        Span::styled("📦 ", Style::default()),
                        Span::styled(
                            &s.name,
                            Style::default()
                                .fg(theme.brand_accent)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            format!("  (Runtime: {})", s.runtime),
                            Style::default().fg(theme.success),
                        ),
                    ]));
                    preview_lines.push(Line::from(""));
                    preview_lines.push(Line::from(vec![
                        Span::styled("📝 ", Style::default()),
                        Span::styled(&s.description, Style::default().fg(theme.text_primary)),
                    ]));
                    preview_lines.push(Line::from(""));

                    let pkgs_str = if s.packages.is_empty() {
                        "none".to_string()
                    } else {
                        s.packages.join(", ")
                    };
                    preview_lines.push(Line::from(vec![
                        Span::styled(
                            "⚡ Packages: ",
                            Style::default()
                                .fg(theme.warning)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(pkgs_str, Style::default().fg(theme.text_primary)),
                    ]));
                    preview_lines.push(Line::from(""));

                    preview_lines.push(Line::from(vec![Span::styled(
                        format!("📁 File Structure ({} files):", s.files.len()),
                        Style::default().fg(theme.brand_accent),
                    )]));

                    for f in s.files.iter().take(STACK_PREVIEW_MAX_FILES) {
                        preview_lines.push(Line::from(vec![
                            Span::styled("  ├── ", Style::default().fg(theme.muted)),
                            Span::styled(&f.path, Style::default().fg(theme.text_primary)),
                        ]));
                    }
                    if s.files.len() > STACK_PREVIEW_MAX_FILES {
                        preview_lines.push(Line::from(vec![Span::styled(
                            format!(
                                "  ╰── ... and {} more files",
                                s.files.len() - STACK_PREVIEW_MAX_FILES
                            ),
                            Style::default().fg(theme.muted),
                        )]));
                    }
                } else {
                    preview_lines.push(Line::from(Span::styled(
                        "No stack selected",
                        Style::default().fg(theme.muted),
                    )));
                }

                let preview_p = Paragraph::new(preview_lines).block(preview_block);
                frame.render_widget(preview_p, h_chunks[1]);
            }
            ModalState::CommandCatalog {
                ref filtered_indices,
                selected_index,
                ref filter,
            } => {
                let popup_area =
                    centered_rect(COMMAND_CATALOG_WIDTH_PCT, COMMAND_CATALOG_HEIGHT_PCT, area);
                frame.render_widget(Clear, popup_area);

                let root_block = Block::default()
                    .title(" 🧭 minicode Slash Commands & Autonomous Intent Catalog ")
                    .title_alignment(Alignment::Center)
                    .borders(Borders::ALL)
                    .border_style(
                        Style::default()
                            .fg(theme.brand_accent)
                            .bg(theme.bg_elevated),
                    )
                    .style(Style::default().bg(theme.bg_elevated));

                let inner_area = root_block.inner(popup_area);
                frame.render_widget(root_block, popup_area);

                let v_chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Length(3), Constraint::Min(8)])
                    .split(inner_area);

                // Search Bar
                let count_badge = format!(
                    " [{}/{}] ",
                    if filtered_indices.is_empty() {
                        0
                    } else {
                        *selected_index + 1
                    },
                    filtered_indices.len()
                );
                let search_block = Block::default()
                    .title(" 🔍 Filter Commands ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme.brand_accent))
                    .style(Style::default().bg(theme.bg_elevated));

                let search_p = Paragraph::new(Line::from(vec![
                    Span::styled(
                        "❯ ",
                        Style::default()
                            .fg(theme.brand_accent)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        filter,
                        Style::default()
                            .fg(theme.text_primary)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled("█", Style::default().fg(theme.brand_accent)),
                    Span::styled(
                        format!(
                            "{:>width$}",
                            count_badge,
                            width = (v_chunks[0].width as usize)
                                .saturating_sub(UnicodeWidthStr::width(filter.as_str()) + 8)
                        ),
                        Style::default().fg(theme.muted),
                    ),
                ]))
                .block(search_block);
                frame.render_widget(search_p, v_chunks[0]);

                // Split into [Left List (42%), Right Details (58%)]
                let h_chunks = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([Constraint::Percentage(42), Constraint::Percentage(58)])
                    .split(v_chunks[1]);

                let list_block = Block::default()
                    .title(" Commands ")
                    .borders(Borders::RIGHT)
                    .border_style(Style::default().fg(theme.bg_elevated));

                let mut list_lines = Vec::new();
                let max_visible = (h_chunks[0].height as usize).saturating_sub(2).max(1);
                let scroll_offset = if max_visible > 0 && *selected_index >= max_visible {
                    selected_index.saturating_sub(max_visible.saturating_sub(1))
                } else {
                    0
                };

                for (idx_rel, &item_idx) in filtered_indices
                    .iter()
                    .skip(scroll_offset)
                    .take(max_visible)
                    .enumerate()
                {
                    let real_idx = scroll_offset + idx_rel;
                    let item = &COMMAND_CATALOG_ITEMS[item_idx];
                    let is_selected = real_idx == *selected_index;

                    let cursor = if is_selected { "▶ " } else { "  " };
                    let name_style = if is_selected {
                        Style::default()
                            .fg(theme.brand_accent)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(theme.text_primary)
                    };

                    let shortcut_style = Style::default().fg(theme.warning);
                    let shortcut_str = if item.shortcut.is_empty() {
                        String::new()
                    } else {
                        format!(" [{}]", item.shortcut)
                    };

                    list_lines.push(Line::from(vec![
                        Span::styled(
                            cursor,
                            Style::default()
                                .fg(theme.brand_accent)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            format!("{:<width$}", item.name, width = COMMAND_NAME_DISPLAY_COLS),
                            name_style,
                        ),
                        Span::styled(shortcut_str, shortcut_style),
                    ]));
                }

                if filtered_indices.is_empty() {
                    list_lines.push(Line::from(Span::styled(
                        "  No matching commands found",
                        Style::default().fg(theme.muted),
                    )));
                }

                let left_p = Paragraph::new(list_lines).block(list_block);
                frame.render_widget(left_p, h_chunks[0]);

                // Right Details Pane
                let mut detail_lines = Vec::new();
                if let Some(&selected_item_idx) = filtered_indices.get(*selected_index) {
                    let cmd = &COMMAND_CATALOG_ITEMS[selected_item_idx];

                    detail_lines.push(Line::from(vec![
                        Span::styled(
                            cmd.name,
                            Style::default()
                                .fg(theme.brand_accent)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            format!("  •  {}", cmd.category),
                            Style::default().fg(theme.muted),
                        ),
                    ]));
                    detail_lines.push(Line::from(""));

                    if !cmd.shortcut.is_empty() {
                        detail_lines.push(Line::from(vec![
                            Span::styled(
                                "⚡ Shortcut: ",
                                Style::default()
                                    .fg(theme.warning)
                                    .add_modifier(Modifier::BOLD),
                            ),
                            Span::styled(cmd.shortcut, Style::default().fg(theme.text_primary)),
                        ]));
                        detail_lines.push(Line::from(""));
                    }

                    detail_lines.push(Line::from(vec![Span::styled(
                        "📖 Description:",
                        Style::default()
                            .fg(theme.success)
                            .add_modifier(Modifier::BOLD),
                    )]));
                    detail_lines.push(Line::from(vec![Span::styled(
                        format!("  {}", cmd.description),
                        Style::default().fg(theme.text_primary),
                    )]));
                    detail_lines.push(Line::from(""));

                    detail_lines.push(Line::from(vec![Span::styled(
                        "💡 Usage Example:",
                        Style::default()
                            .fg(theme.brand_accent)
                            .add_modifier(Modifier::BOLD),
                    )]));
                    detail_lines.push(Line::from(vec![Span::styled(
                        format!("  {}", cmd.example),
                        Style::default()
                            .fg(theme.muted)
                            .add_modifier(Modifier::ITALIC),
                    )]));
                    detail_lines.push(Line::from(""));

                    detail_lines.push(Line::from(vec![Span::styled(
                        "🤖 Autonomous Intent Routing:",
                        Style::default()
                            .fg(theme.brand_accent)
                            .add_modifier(Modifier::BOLD),
                    )]));
                    detail_lines.push(Line::from(vec![Span::styled(
                        "  You can type natural language in the prompt dock. minicode's",
                        Style::default().fg(theme.muted),
                    )]));
                    detail_lines.push(Line::from(vec![Span::styled(
                        "  intent router will recognize and trigger this workflow automatically.",
                        Style::default().fg(theme.muted),
                    )]));
                }

                let right_p = Paragraph::new(detail_lines);
                frame.render_widget(right_p, h_chunks[1]);
            }
            ModalState::GitDiff {
                ref diff_files,
                selected_file_index,
                scroll_offset,
                staged_view,
            } => {
                let modal_area = centered_rect(GIT_DIFF_WIDTH_PCT, GIT_DIFF_HEIGHT_PCT, area);
                frame.render_widget(Clear, modal_area);

                let title = if *staged_view {
                    " 🔍 Staged Git Changes (--cached) "
                } else {
                    " 🔍 Working Tree Git Changes (unstaged) "
                };

                let block = Block::default()
                    .borders(Borders::ALL)
                    .border_style(
                        Style::default()
                            .fg(theme.brand_accent)
                            .add_modifier(Modifier::BOLD),
                    )
                    .title(title);
                frame.render_widget(block, modal_area);

                let inner = modal_area.inner(Margin {
                    vertical: 1,
                    horizontal: 1,
                });

                let h_chunks = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([
                        Constraint::Percentage(32), // Files list
                        Constraint::Percentage(68), // Diff viewer
                    ])
                    .split(inner);

                let max_visible = (h_chunks[0].height.saturating_sub(2) as usize).max(1);
                let file_scroll_offset = if max_visible > 0 && *selected_file_index >= max_visible {
                    selected_file_index.saturating_sub(max_visible.saturating_sub(1))
                } else {
                    0
                };

                // Left: Files list with viewport scrolling
                let items: Vec<ListItem> = diff_files
                    .iter()
                    .enumerate()
                    .skip(file_scroll_offset)
                    .take(max_visible)
                    .map(|(idx, f)| {
                        let is_selected = idx == *selected_file_index;
                        let prefix = if is_selected { " › " } else { "   " };
                        let status_badge = format!("[{}] ", f.status_char);
                        let stats = format!(" +{} -{}", f.additions, f.deletions);

                        let badge_style = match f.status_char {
                            'A' => Style::default().fg(theme.success),
                            'D' => Style::default().fg(theme.destructive),
                            'M' => Style::default().fg(theme.warning),
                            _ => Style::default().fg(theme.muted),
                        };

                        let spans = vec![
                            Span::raw(prefix),
                            Span::styled(status_badge, badge_style),
                            Span::styled(
                                &f.path,
                                if is_selected {
                                    Style::default()
                                        .fg(theme.bg_primary)
                                        .bg(theme.brand_accent)
                                        .add_modifier(Modifier::BOLD)
                                } else {
                                    Style::default().fg(theme.text_primary)
                                },
                            ),
                            Span::styled(stats, Style::default().fg(theme.muted)),
                        ];

                        let item_style = if is_selected {
                            Style::default().bg(theme.brand_accent).fg(theme.bg_primary)
                        } else {
                            Style::default()
                        };

                        ListItem::new(Line::from(spans)).style(item_style)
                    })
                    .collect();

                let files_title = format!(" Modified Files ({}) ", diff_files.len());
                let list_block = Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme.border))
                    .title(files_title);
                let list = List::new(items).block(list_block);
                frame.render_widget(list, h_chunks[0]);

                // Right: Diff content
                let diff_title =
                    if !diff_files.is_empty() && *selected_file_index < diff_files.len() {
                        format!(" Diff: {} ", diff_files[*selected_file_index].path)
                    } else {
                        " Diff Preview ".to_string()
                    };

                let preview_block = Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme.border))
                    .title(diff_title);

                let mut diff_lines = Vec::new();
                if !diff_files.is_empty() && *selected_file_index < diff_files.len() {
                    let cur_f = &diff_files[*selected_file_index];
                    if cur_f.lines.is_empty() {
                        diff_lines.push(Line::from(Span::styled(
                            " (No line diffs recorded) ",
                            Style::default().fg(theme.muted),
                        )));
                    } else {
                        for l in cur_f.lines.iter().skip(*scroll_offset) {
                            let (style, prefix_char) = match l.tag {
                                '+' => (Style::default().fg(theme.success), "+ "),
                                '-' => (Style::default().fg(theme.destructive), "- "),
                                '@' => (
                                    Style::default()
                                        .fg(theme.brand_accent)
                                        .add_modifier(Modifier::BOLD),
                                    "  ",
                                ),
                                _ => (Style::default().fg(theme.text_primary), "  "),
                            };

                            let lineno_str = match (l.old_lineno, l.new_lineno) {
                                (Some(o), Some(n)) => format!("{:>4} {:>4} │ ", o, n),
                                (Some(o), None) => format!("{:>4}      │ ", o),
                                (None, Some(n)) => format!("     {:>4} │ ", n),
                                (None, None) => "          │ ".to_string(),
                            };

                            diff_lines.push(Line::from(vec![
                                Span::styled(lineno_str, Style::default().fg(theme.muted)),
                                Span::styled(prefix_char, style),
                                Span::styled(&l.content, style),
                            ]));
                        }
                    }
                } else {
                    diff_lines.push(Line::from(Span::styled(
                        " Working tree is clean — no diffs found. ",
                        Style::default().fg(theme.muted),
                    )));
                }

                let diff_p = Paragraph::new(diff_lines).block(preview_block);
                frame.render_widget(diff_p, h_chunks[1]);
            }
            ModalState::CodeExplorer {
                symbols,
                filtered_indices,
                selected_index,
                filter,
                active_tab,
            } => {
                let popup_area =
                    centered_rect(CODE_EXPLORER_WIDTH_PCT, CODE_EXPLORER_HEIGHT_PCT, area);
                frame.render_widget(Clear, popup_area);

                let outer_block = Block::default()
                    .title(" 🧭 CodeGraph Surgical Explorer (Ctrl+E) ")
                    .title_alignment(Alignment::Center)
                    .borders(Borders::ALL)
                    .border_style(
                        Style::default()
                            .fg(theme.brand_accent)
                            .bg(theme.bg_elevated),
                    )
                    .style(Style::default().bg(theme.bg_elevated));

                let inner_area = outer_block.inner(popup_area);
                frame.render_widget(outer_block, popup_area);

                let v_chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Length(3), // Search box
                        Constraint::Min(5),    // Main content (2 cols)
                    ])
                    .split(inner_area);

                // Search input
                let search_text = format!(" Filter symbols/layers: {}█", filter);
                let search_box = Paragraph::new(search_text)
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .border_style(Style::default().fg(theme.border)),
                    )
                    .style(Style::default().fg(theme.text_primary));
                frame.render_widget(search_box, v_chunks[0]);

                let h_chunks = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([
                        Constraint::Percentage(35), // Left: Symbols list
                        Constraint::Percentage(65), // Right: Details & Call graph
                    ])
                    .split(v_chunks[1]);

                let max_visible = (h_chunks[0].height.saturating_sub(2) as usize).max(1);
                let scroll_offset = if max_visible > 0 && *selected_index >= max_visible {
                    selected_index.saturating_sub(max_visible.saturating_sub(1))
                } else {
                    0
                };

                // Left: Symbols list with viewport scrolling
                let items: Vec<ListItem> = filtered_indices
                    .iter()
                    .enumerate()
                    .skip(scroll_offset)
                    .take(max_visible)
                    .map(|(display_idx, &real_idx)| {
                        let sym = &symbols[real_idx];
                        let is_selected = display_idx == *selected_index;

                        let spans = vec![
                            Span::styled(
                                format!(
                                    "{:<width$} ",
                                    sym.layer.badge(),
                                    width = EXPLORER_BADGE_DISPLAY_COLS
                                ),
                                Style::default().fg(theme.brand_accent),
                            ),
                            Span::styled(
                                &sym.symbol_name,
                                if is_selected {
                                    Style::default()
                                        .fg(theme.bg_primary)
                                        .add_modifier(Modifier::BOLD)
                                } else {
                                    Style::default()
                                        .fg(theme.text_primary)
                                        .add_modifier(Modifier::BOLD)
                                },
                            ),
                            Span::styled(
                                format!(" [{}]", sym.kind),
                                Style::default().fg(theme.muted),
                            ),
                        ];

                        let item_style = if is_selected {
                            Style::default().bg(theme.brand_accent).fg(theme.bg_primary)
                        } else {
                            Style::default()
                        };

                        ListItem::new(Line::from(spans)).style(item_style)
                    })
                    .collect();

                let list_title = format!(" Symbols ({}) ", filtered_indices.len());
                let list_block = Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme.border))
                    .title(list_title);
                let list = List::new(items).block(list_block);
                frame.render_widget(list, h_chunks[0]);

                // Right: Detail view with tabs
                let detail_block = Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme.border))
                    .title(" Surgical AST Inspection ");

                let mut detail_lines = Vec::new();

                if let Some(&real_idx) = filtered_indices.get(*selected_index) {
                    let sym = &symbols[real_idx];

                    // Header line
                    detail_lines.push(Line::from(vec![
                        Span::styled(
                            format!("{} ", sym.layer.badge()),
                            Style::default()
                                .fg(theme.brand_accent)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            &sym.symbol_name,
                            Style::default()
                                .fg(theme.brand_accent)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            format!(
                                " ({}) in {}:{}-{}",
                                sym.kind, sym.file_path, sym.line_range.0, sym.line_range.1
                            ),
                            Style::default().fg(theme.muted),
                        ),
                    ]));

                    // Signature
                    detail_lines.push(Line::from(vec![
                        Span::styled("Signature: ", Style::default().fg(theme.warning)),
                        Span::styled(&sym.signature, Style::default().fg(theme.text_primary)),
                    ]));

                    if let Some(doc) = &sym.doc_comment {
                        detail_lines.push(Line::from(vec![
                            Span::styled("Doc: ", Style::default().fg(theme.muted)),
                            Span::styled(doc.trim(), Style::default().fg(theme.muted)),
                        ]));
                    }
                    detail_lines.push(Line::from(""));

                    // Tab selector bar
                    let tab_titles = [
                        "[ 1. Source Definition ]".to_string(),
                        format!("[ 2. Callers ({}) ]", sym.callers.len()),
                        format!("[ 3. Callees ({}) ]", sym.callees.len()),
                        format!("[ 4. Blast Radius ({}) ]", sym.blast_radius_files.len()),
                    ];

                    let mut tab_spans = Vec::new();
                    for (t_idx, t_title) in tab_titles.iter().enumerate() {
                        let is_active_tab = t_idx == *active_tab;
                        tab_spans.push(Span::styled(
                            format!("{} ", t_title),
                            if is_active_tab {
                                Style::default()
                                    .fg(theme.brand_accent)
                                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
                            } else {
                                Style::default().fg(theme.muted)
                            },
                        ));
                    }
                    detail_lines.push(Line::from(tab_spans));
                    detail_lines.push(Line::from(""));

                    // Tab contents
                    match *active_tab {
                        0 => {
                            if let Some(src) = &sym.source_code {
                                let start_line = sym.line_range.0;
                                for (i, src_line) in src.lines().enumerate() {
                                    detail_lines.push(Line::from(vec![
                                        Span::styled(
                                            format!("{:>4} │ ", start_line + i),
                                            Style::default().fg(theme.muted),
                                        ),
                                        Span::styled(
                                            src_line,
                                            Style::default().fg(theme.text_primary),
                                        ),
                                    ]));
                                }
                            } else {
                                detail_lines.push(Line::from(Span::styled(
                                    " (Source code not loaded) ",
                                    Style::default().fg(theme.muted),
                                )));
                            }
                        }
                        1 => {
                            if sym.callers.is_empty() {
                                detail_lines.push(Line::from(Span::styled(
                                    " No incoming callers found (Root entrypoint / public symbol).",
                                    Style::default().fg(theme.muted),
                                )));
                            } else {
                                for c in &sym.callers {
                                    detail_lines.push(Line::from(vec![
                                        Span::styled("  ← ", Style::default().fg(theme.success)),
                                        Span::styled(
                                            &c.name,
                                            Style::default()
                                                .fg(theme.text_primary)
                                                .add_modifier(Modifier::BOLD),
                                        ),
                                        Span::styled(
                                            format!(" in {}:{}", c.file_path, c.line),
                                            Style::default().fg(theme.muted),
                                        ),
                                    ]));
                                }
                            }
                        }
                        2 => {
                            if sym.callees.is_empty() {
                                detail_lines.push(Line::from(Span::styled(
                                    " No outgoing calls found (Leaf function).",
                                    Style::default().fg(theme.muted),
                                )));
                            } else {
                                for c in &sym.callees {
                                    detail_lines.push(Line::from(vec![
                                        Span::styled(
                                            "  → ",
                                            Style::default().fg(theme.brand_accent),
                                        ),
                                        Span::styled(
                                            &c.name,
                                            Style::default()
                                                .fg(theme.text_primary)
                                                .add_modifier(Modifier::BOLD),
                                        ),
                                        Span::styled(
                                            format!(" in {}:{}", c.file_path, c.line),
                                            Style::default().fg(theme.muted),
                                        ),
                                    ]));
                                }
                            }
                        }
                        _ => {
                            if sym.blast_radius_files.is_empty() {
                                detail_lines.push(Line::from(Span::styled(
                                    " Isolated module (No direct downstream dependents).",
                                    Style::default().fg(theme.success),
                                )));
                            } else {
                                detail_lines.push(Line::from(Span::styled(
                                    "Files directly dependent on this symbol:",
                                    Style::default().fg(theme.warning),
                                )));
                                for file in &sym.blast_radius_files {
                                    detail_lines.push(Line::from(vec![
                                        Span::styled(
                                            "  • ",
                                            Style::default().fg(theme.destructive),
                                        ),
                                        Span::styled(file, Style::default().fg(theme.text_primary)),
                                    ]));
                                }
                            }
                        }
                    }
                } else {
                    detail_lines.push(Line::from(Span::styled(
                        " No matching AST symbols found. ",
                        Style::default().fg(theme.muted),
                    )));
                }

                let detail_p = Paragraph::new(detail_lines).block(detail_block);
                frame.render_widget(detail_p, h_chunks[1]);
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
                let popup_area =
                    centered_rect_exact(WORKSPACE_ANALYSIS_WIDTH, WORKSPACE_ANALYSIS_HEIGHT, area);
                frame.render_widget(Clear, popup_area);

                let title = format!(" ✨ minicode v{} ", env!("CARGO_PKG_VERSION"));
                let block = Block::default()
                    .title(title)
                    .title_alignment(Alignment::Left)
                    .borders(Borders::ALL)
                    .border_style(
                        Style::default()
                            .fg(theme.brand_accent)
                            .bg(theme.bg_elevated),
                    )
                    .style(Style::default().bg(theme.bg_elevated));

                let inner = block.inner(popup_area);
                frame.render_widget(block, popup_area);

                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Length(1), // Header info
                        Constraint::Length(1), // Spacer
                        Constraint::Min(3),    // Options
                    ])
                    .split(inner);

                let cache_status_span = if *is_indexed {
                    Span::styled(
                        format!(
                            "Indexed ({} syms, {} files)",
                            cached_symbols_count, cached_files_count
                        ),
                        Style::default()
                            .fg(theme.success)
                            .add_modifier(Modifier::BOLD),
                    )
                } else {
                    Span::styled(
                        "Unindexed",
                        Style::default()
                            .fg(theme.warning)
                            .add_modifier(Modifier::BOLD),
                    )
                };

                let header_line = Line::from(vec![
                    Span::raw(" 📁 "),
                    Span::styled(
                        workspace_path.as_str(),
                        Style::default().fg(theme.text_primary),
                    ),
                    Span::styled("  │  🔍 Cache: ", Style::default().fg(theme.muted)),
                    cache_status_span,
                ]);
                frame.render_widget(Paragraph::new(header_line), chunks[0]);

                let options: Vec<(&str, &str, &str)> = if !*is_indexed {
                    vec![
                        (
                            "[1] ⚡ Quick Index",
                            "AST symbols & PageRank graph",
                            "[ENTER]",
                        ),
                        (
                            "[2] 🧠 Deep Scan",
                            "Symbol graph & architecture scan",
                            "[2]",
                        ),
                        ("[3] ⏩ Skip", "Instant lightweight chat", "[ESC]"),
                    ]
                } else {
                    vec![
                        (
                            "[1] ⚡ Incremental Sync",
                            "Scan modified files only (~5ms)",
                            "[ENTER]",
                        ),
                        ("[2] 🔄 Full Rebuild", "Cold re-index from scratch", "[2]"),
                        (
                            "[3] 🗺️ View Repo Map",
                            "Display AST centrality hierarchy",
                            "[3]",
                        ),
                        ("[4] ⏩ Close", "Resume chat without re-indexing", "[ESC]"),
                    ]
                };

                let items: Vec<ListItem> = options
                    .iter()
                    .enumerate()
                    .map(|(i, (tag, desc, key))| {
                        let is_selected = i == *selected_index;
                        let prefix = if is_selected { "  ❯ " } else { "    " };

                        let line = if is_selected {
                            Line::from(vec![
                                Span::styled(
                                    prefix,
                                    Style::default()
                                        .fg(theme.brand_accent)
                                        .add_modifier(Modifier::BOLD),
                                ),
                                Span::styled(
                                    *tag,
                                    Style::default()
                                        .fg(theme.brand_accent)
                                        .add_modifier(Modifier::BOLD),
                                ),
                                Span::styled(
                                    format!("  ({})", desc),
                                    Style::default().fg(theme.text_primary),
                                ),
                                Span::styled(
                                    format!("  {:>8}", key),
                                    Style::default()
                                        .fg(theme.brand_accent)
                                        .add_modifier(Modifier::BOLD),
                                ),
                            ])
                        } else {
                            Line::from(vec![
                                Span::raw(prefix),
                                Span::styled(*tag, Style::default().fg(theme.text_primary)),
                                Span::styled(
                                    format!("  ({})", desc),
                                    Style::default().fg(theme.muted),
                                ),
                                Span::styled(
                                    format!("  {:>8}", key),
                                    Style::default().fg(theme.muted),
                                ),
                            ])
                        };

                        ListItem::new(line)
                    })
                    .collect();

                let list = List::new(items);
                frame.render_widget(list, chunks[2]);
            }
            ModalState::ExitConfirm {
                workspace_name,
                selected_yes,
            } => {
                let width = EXIT_CONFIRM_MODAL_WIDTH.min(area.width.saturating_sub(2));
                let height = EXIT_CONFIRM_MODAL_HEIGHT.min(area.height.saturating_sub(2));
                let popup_area = centered_rect_exact(width, height, area);
                frame.render_widget(Clear, popup_area);

                let block = Block::default()
                    .title(Line::from(vec![Span::styled(
                        " ⏻ Exit minicode ",
                        Style::default()
                            .fg(theme.brand_accent)
                            .add_modifier(Modifier::BOLD),
                    )]))
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
                        Constraint::Length(1), // 0: ● Workspace: name (vX.X.X)
                        Constraint::Length(1), // 1: ● Session: auto-saved to .minicode/history
                        Constraint::Length(1), // 2: Question "Are you sure you want to quit?"
                        Constraint::Length(1), // 3: Spacer
                        Constraint::Length(1), // 4: Buttons row "[ ✖ Yep, Quit ]   [ ✔ Stay in Session ]"
                    ])
                    .split(inner_area);

                // 0: Workspace info
                let ws_line = Line::from(vec![
                    Span::styled(" ● ", Style::default().fg(theme.success)),
                    Span::styled("Workspace: ", Style::default().fg(theme.muted)),
                    Span::styled(
                        workspace_name.clone(),
                        Style::default()
                            .fg(theme.text_primary)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!(" (v{})", env!("CARGO_PKG_VERSION")),
                        Style::default().fg(theme.muted),
                    ),
                ]);
                frame.render_widget(Paragraph::new(ws_line), chunks[0]);

                // 1: Session persistence info
                let sess_line = Line::from(vec![
                    Span::styled(" ● ", Style::default().fg(theme.info)),
                    Span::styled("Session: ", Style::default().fg(theme.muted)),
                    Span::styled(
                        "auto-saved to .minicode/history",
                        Style::default().fg(theme.muted),
                    ),
                ]);
                frame.render_widget(Paragraph::new(sess_line), chunks[1]);

                // 2: Question
                let question_p = Paragraph::new(Line::from(vec![Span::styled(
                    "Are you sure you want to quit?",
                    Style::default()
                        .fg(theme.text_primary)
                        .add_modifier(Modifier::BOLD),
                )]))
                .alignment(Alignment::Center);
                frame.render_widget(question_p, chunks[2]);

                // 4: Buttons
                let yep_style = if *selected_yes {
                    Style::default()
                        .bg(theme.destructive)
                        .fg(theme.bg_primary)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().bg(theme.bg_primary).fg(theme.text_primary)
                };

                let nope_style = if !*selected_yes {
                    Style::default()
                        .bg(theme.brand_accent)
                        .fg(theme.bg_primary)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().bg(theme.bg_primary).fg(theme.text_primary)
                };

                let yep_spans = if *selected_yes {
                    vec![Span::styled("  ✖ Yep, Quit (Y)  ", yep_style)]
                } else {
                    vec![
                        Span::styled("  ✖ ", yep_style),
                        Span::styled("Y", yep_style.add_modifier(Modifier::UNDERLINED)),
                        Span::styled("ep, Quit  ", yep_style),
                    ]
                };

                let nope_spans = if !*selected_yes {
                    vec![Span::styled("  ✔ Stay in Session (N)  ", nope_style)]
                } else {
                    vec![
                        Span::styled("  ✔ Stay in Session (", nope_style),
                        Span::styled("N", nope_style.add_modifier(Modifier::UNDERLINED)),
                        Span::styled(")  ", nope_style),
                    ]
                };

                let mut buttons_line = Vec::new();
                buttons_line.extend(yep_spans);
                buttons_line.push(Span::raw("   "));
                buttons_line.extend(nope_spans);

                let buttons_p =
                    Paragraph::new(Line::from(buttons_line)).alignment(Alignment::Center);
                frame.render_widget(buttons_p, chunks[4]);
            }
        }
    }
}

/// Helper function to create an exact sized centered rect
fn centered_rect_exact(width: u16, height: u16, r: Rect) -> Rect {
    let w = width.min(r.width.saturating_sub(2));
    let h = height.min(r.height.saturating_sub(2));
    let x = r.x + (r.width.saturating_sub(w)) / 2;
    let y = r.y + (r.height.saturating_sub(h)) / 2;
    Rect {
        x,
        y,
        width: w,
        height: h,
    }
}

/// Helper function to create a centered rect rectangle
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let margin_y = 100_u16.saturating_sub(percent_y) / 2;
    let margin_x = 100_u16.saturating_sub(percent_x) / 2;

    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(margin_y),
            Constraint::Percentage(percent_y.min(100)),
            Constraint::Percentage(margin_y),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(margin_x),
            Constraint::Percentage(percent_x.min(100)),
            Constraint::Percentage(margin_x),
        ])
        .split(popup_layout[1])[1]
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
}

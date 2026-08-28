use crate::agent::models::ModelInfo;
use crate::ui::theme::Theme;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph};
use ratatui::Frame;

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
    },
    StackSelect {
        stacks: Vec<crate::tools::onpkg::stacks::Stack>,
        filtered_indices: Vec<usize>,
        selected_index: usize,
        filter: String,
    },
    Help,
    Approval(crate::ui::approval::ApprovalModalState),
}

impl ModalState {
    pub fn is_active(&self) -> bool {
        !matches!(self, ModalState::None)
    }

    pub fn new_provider_select() -> Self {
        let providers = vec![
            "openrouter".to_string(),
            "gemini".to_string(),
            "openai".to_string(),
            "deepseek".to_string(),
            "groq".to_string(),
            "together".to_string(),
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

    pub fn new_session_browser(sessions: Vec<crate::session::store::SessionMetadata>) -> Self {
        ModalState::SessionBrowser {
            sessions,
            selected_index: 0,
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

    pub fn update_filter(&mut self) {
        if let ModalState::ModelSelect {
            models,
            filtered_indices,
            selected_index,
            filter,
            ..
        } = self
        {
            let f = filter.to_lowercase();
            *filtered_indices = models
                .iter()
                .enumerate()
                .filter(|(_, m)| {
                    if f.is_empty() {
                        true
                    } else {
                        m.id.to_lowercase().contains(&f) || m.name.to_lowercase().contains(&f)
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
            let f = filter.to_lowercase();
            *filtered_indices = stacks
                .iter()
                .enumerate()
                .filter(|(_, s)| {
                    if f.is_empty() {
                        true
                    } else {
                        s.name.to_lowercase().contains(&f)
                            || s.runtime.to_lowercase().contains(&f)
                            || s.description.to_lowercase().contains(&f)
                            || s.packages.iter().any(|p| p.to_lowercase().contains(&f))
                    }
                })
                .map(|(i, _)| i)
                .collect();

            if *selected_index >= filtered_indices.len() {
                *selected_index = filtered_indices.len().saturating_sub(1);
            }
        }
    }

    pub fn render(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        match self {
            ModalState::None => {}
            ModalState::ProviderSelect {
                providers,
                selected_index,
            } => {
                let popup_area = centered_rect(50, 45, area);
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

                let items: Vec<ListItem> = providers
                    .iter()
                    .enumerate()
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
                let popup_area = centered_rect(75, 70, area);
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
                        Constraint::Length(1), // Keybind hints
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

                // Models list
                let items: Vec<ListItem> = filtered_indices
                    .iter()
                    .enumerate()
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

                // Footer hints
                let footer = Paragraph::new(
                    " [↑/↓] Navigate  [Enter] Select Model  [Esc] Back to Providers ",
                )
                .style(Style::default().fg(theme.muted))
                .alignment(Alignment::Center);
                frame.render_widget(footer, chunks[2]);
            }
            ModalState::UndoCheckpoint {
                checkpoints,
                selected_index,
            } => {
                let popup_area = centered_rect(72, 65, area);
                frame.render_widget(Clear, popup_area);

                let mut lines = Vec::new();
                lines.push(Line::from(""));

                let total = checkpoints.len();
                let max_visible = 5;
                let scroll_offset = if *selected_index >= max_visible {
                    *selected_index - max_visible + 1
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
                    let prompt_display = if cp.prompt.len() > 55 {
                        format!("{}...", &cp.prompt[..52])
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
                            .take(2)
                            .collect();
                        let more = if cp.files.len() > 2 {
                            format!(" +{} more", cp.files.len() - 2)
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

                let footer_text = vec![
                    Span::styled(
                        "[↑/↓] ",
                        Style::default()
                            .fg(theme.brand_accent)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled("Select   ", Style::default().fg(theme.text_primary)),
                    Span::styled(
                        "[Enter] ",
                        Style::default()
                            .fg(theme.success)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled("Revert   ", Style::default().fg(theme.text_primary)),
                    Span::styled(
                        "[Esc] ",
                        Style::default()
                            .fg(theme.warning)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled("Cancel", Style::default().fg(theme.text_primary)),
                ];
                let footer_area = Rect {
                    x: popup_area.x + 2,
                    y: popup_area.y + popup_area.height.saturating_sub(2),
                    width: popup_area.width.saturating_sub(4),
                    height: 1,
                };
                frame.render_widget(
                    Paragraph::new(Line::from(footer_text)).alignment(Alignment::Center),
                    footer_area,
                );
            }
            ModalState::ThemeSelect {
                themes,
                selected_index,
            } => {
                let popup_area = centered_rect(74, 68, area);
                frame.render_widget(Clear, popup_area);

                let mut lines = Vec::new();
                lines.push(Line::from(""));

                let max_visible = 5;
                let scroll_offset = if *selected_index >= max_visible {
                    *selected_index - max_visible + 1
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
                        Span::styled(format!("{:<22}", t.name), title_style),
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

                let footer_text = vec![
                    Span::styled(
                        "[↑/↓] ",
                        Style::default()
                            .fg(theme.brand_accent)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled("Select   ", Style::default().fg(theme.text_primary)),
                    Span::styled(
                        "[Enter] ",
                        Style::default()
                            .fg(theme.success)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled("Apply & Save   ", Style::default().fg(theme.text_primary)),
                    Span::styled(
                        "[Esc] ",
                        Style::default()
                            .fg(theme.warning)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled("Cancel", Style::default().fg(theme.text_primary)),
                ];
                let footer_area = Rect {
                    x: popup_area.x + 2,
                    y: popup_area.y + popup_area.height.saturating_sub(2),
                    width: popup_area.width.saturating_sub(4),
                    height: 1,
                };
                frame.render_widget(
                    Paragraph::new(Line::from(footer_text)).alignment(Alignment::Center),
                    footer_area,
                );
            }
            ModalState::SessionBrowser {
                sessions,
                selected_index,
            } => {
                let popup_area = centered_rect(78, 70, area);
                frame.render_widget(Clear, popup_area);

                let outer_block = Block::default()
                    .title(" Session History ")
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

                // Split inner: list | footer
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Min(3), Constraint::Length(1)])
                    .split(inner_area);

                if sessions.is_empty() {
                    let empty = Paragraph::new(Line::from(vec![Span::styled(
                        "  No past sessions found in this workspace.",
                        Style::default().fg(theme.muted),
                    )]))
                    .alignment(Alignment::Left);
                    frame.render_widget(empty, chunks[0]);
                } else {
                    let items: Vec<ListItem> = sessions
                        .iter()
                        .enumerate()
                        .map(|(i, s)| {
                            let is_selected = i == *selected_index;
                            let time_ago = format_time_ago(&s.created_at);

                            // Shorten workspace to last 2 path segments
                            let ws_short = {
                                let parts: Vec<&str> =
                                    s.workspace.split('/').filter(|p| !p.is_empty()).collect();
                                if parts.len() >= 2 {
                                    format!(
                                        "…/{}/{}",
                                        parts[parts.len() - 2],
                                        parts[parts.len() - 1]
                                    )
                                } else {
                                    s.workspace.clone()
                                }
                            };

                            let event_label = if s.event_count == 0 {
                                String::new()
                            } else {
                                format!(" • {} events", s.event_count)
                            };

                            let id_short = if s.id.len() > 20 {
                                format!("{}…", &s.id[..20])
                            } else {
                                s.id.clone()
                            };

                            let line1 = if is_selected {
                                Line::from(vec![
                                    Span::styled(
                                        format!(" › {} ", time_ago),
                                        Style::default()
                                            .fg(theme.bg_primary)
                                            .bg(theme.brand_accent)
                                            .add_modifier(Modifier::BOLD),
                                    ),
                                    Span::styled(
                                        format!(" {}{}", ws_short, event_label),
                                        Style::default()
                                            .fg(theme.bg_primary)
                                            .bg(theme.brand_accent),
                                    ),
                                ])
                            } else {
                                Line::from(vec![
                                    Span::styled(
                                        format!("   {} ", time_ago),
                                        Style::default()
                                            .fg(theme.warning)
                                            .add_modifier(Modifier::BOLD),
                                    ),
                                    Span::styled(
                                        format!(" {}{}", ws_short, event_label),
                                        Style::default().fg(theme.text_primary),
                                    ),
                                ])
                            };

                            let id_style = if is_selected {
                                Style::default()
                                    .fg(theme.bg_elevated)
                                    .bg(theme.brand_accent)
                            } else {
                                Style::default().fg(theme.muted)
                            };
                            let line2 = Line::from(vec![Span::styled(
                                format!("     {}", id_short),
                                id_style,
                            )]);

                            ListItem::new(vec![line1, line2])
                        })
                        .collect();

                    let list = List::new(items);
                    frame.render_widget(list, chunks[0]);
                }

                // Footer hints
                let footer_text = vec![
                    Span::styled(
                        "[↑↓] ",
                        Style::default()
                            .fg(theme.success)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled("Navigate   ", Style::default().fg(theme.text_primary)),
                    Span::styled(
                        "[Enter] ",
                        Style::default()
                            .fg(theme.success)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled("Load Session   ", Style::default().fg(theme.text_primary)),
                    Span::styled(
                        "[Esc] ",
                        Style::default()
                            .fg(theme.warning)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled("Cancel", Style::default().fg(theme.text_primary)),
                ];
                frame.render_widget(
                    Paragraph::new(Line::from(footer_text)).alignment(Alignment::Center),
                    chunks[1],
                );
            }
            ModalState::Help => {
                let popup_area = centered_rect(60, 50, area);
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
                let popup_area = centered_rect(84, 76, area);
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
                        Constraint::Length(1), // Footer
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

                // Left: Stacks List
                let items: Vec<ListItem> = filtered_indices
                    .iter()
                    .enumerate()
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

                    for f in s.files.iter().take(12) {
                        preview_lines.push(Line::from(vec![
                            Span::styled("  ├── ", Style::default().fg(theme.muted)),
                            Span::styled(&f.path, Style::default().fg(theme.text_primary)),
                        ]));
                    }
                    if s.files.len() > 12 {
                        preview_lines.push(Line::from(vec![Span::styled(
                            format!("  ╰── ... and {} more files", s.files.len() - 12),
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

                // Footer hints
                let footer =
                    Paragraph::new(" [↑/↓] Navigate  [Enter] Scaffold Stack  [Esc] Close ")
                        .style(Style::default().fg(theme.muted))
                        .alignment(Alignment::Center);
                frame.render_widget(footer, v_chunks[2]);
            }
            ModalState::Approval(approval_state) => {
                approval_state.render(frame, area, theme);
            }
        }
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

use crate::agent::models::ModelInfo;
use crate::ui::theme::Theme;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph};
use ratatui::Frame;

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
    Help,
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
                        Span::styled("  /undo      ", Style::default().fg(theme.success)),
                        Span::styled(
                            "Revert all file modifications from previous turn",
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

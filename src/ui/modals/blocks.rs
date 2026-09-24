//! In-TUI MiniBlocks Native UI Component & Design Token Warehouse Modal.

use crate::blocks::store::get_global_block_store;
use crate::ui::layout_utils::centered_rect;
use crate::ui::modals::common::compute_scroll_offset;
use crate::ui::theme::Theme;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// Active tab within the `/blocks` MiniBlocks warehouse modal dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlocksTab {
    Components,
    Palettes,
    Gradients,
    Templates,
}

#[allow(dead_code)]
impl BlocksTab {
    pub fn all() -> &'static [BlocksTab] {
        &[
            BlocksTab::Components,
            BlocksTab::Palettes,
            BlocksTab::Gradients,
            BlocksTab::Templates,
        ]
    }

    pub fn title(&self) -> &'static str {
        match self {
            BlocksTab::Components => "Components",
            BlocksTab::Palettes => "Palettes",
            BlocksTab::Gradients => "Gradients",
            BlocksTab::Templates => "Templates",
        }
    }

    pub fn next(&self) -> Self {
        match self {
            BlocksTab::Components => BlocksTab::Palettes,
            BlocksTab::Palettes => BlocksTab::Gradients,
            BlocksTab::Gradients => BlocksTab::Templates,
            BlocksTab::Templates => BlocksTab::Components,
        }
    }

    pub fn prev(&self) -> Self {
        match self {
            BlocksTab::Components => BlocksTab::Templates,
            BlocksTab::Palettes => BlocksTab::Components,
            BlocksTab::Gradients => BlocksTab::Palettes,
            BlocksTab::Templates => BlocksTab::Gradients,
        }
    }
}

/// State for the interactive `/blocks` modal dialog.
#[derive(Debug, Clone)]
pub struct BlocksModalState {
    pub workspace_root: PathBuf,
    pub active_tab: BlocksTab,
    pub search_query: String,
    pub selected_index: usize,
    pub preview_scroll_offset: usize,
    pub filtered_component_ids: Vec<Uuid>,
    pub filtered_palette_ids: Vec<Uuid>,
    pub filtered_gradient_ids: Vec<Uuid>,
    pub filtered_template_ids: Vec<Uuid>,
    pub status_message: Option<String>,
}

impl BlocksModalState {
    pub fn new(workspace: &Path) -> Self {
        let mut state = Self {
            workspace_root: workspace.to_path_buf(),
            active_tab: BlocksTab::Components,
            search_query: String::new(),
            selected_index: 0,
            preview_scroll_offset: 0,
            filtered_component_ids: Vec::new(),
            filtered_palette_ids: Vec::new(),
            filtered_gradient_ids: Vec::new(),
            filtered_template_ids: Vec::new(),
            status_message: None,
        };
        state.refresh_filtered();
        state
    }

    pub fn current_tab_len(&self) -> usize {
        match self.active_tab {
            BlocksTab::Components => self.filtered_component_ids.len(),
            BlocksTab::Palettes => self.filtered_palette_ids.len(),
            BlocksTab::Gradients => self.filtered_gradient_ids.len(),
            BlocksTab::Templates => self.filtered_template_ids.len(),
        }
    }

    pub fn next_tab(&mut self) {
        self.active_tab = self.active_tab.next();
        self.search_query.clear();
        self.selected_index = 0;
        self.preview_scroll_offset = 0;
        self.status_message = None;
        self.refresh_filtered();
    }

    pub fn prev_tab(&mut self) {
        self.active_tab = self.active_tab.prev();
        self.search_query.clear();
        self.selected_index = 0;
        self.preview_scroll_offset = 0;
        self.status_message = None;
        self.refresh_filtered();
    }

    pub fn select_next(&mut self) {
        let len = self.current_tab_len();
        if len > 0 && self.selected_index + 1 < len {
            self.selected_index += 1;
            self.preview_scroll_offset = 0;
        }
    }

    pub fn select_prev(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
            self.preview_scroll_offset = 0;
        }
    }

    pub fn scroll_preview_up(&mut self) {
        self.preview_scroll_offset = self.preview_scroll_offset.saturating_sub(3);
    }

    pub fn scroll_preview_down(&mut self) {
        self.preview_scroll_offset = self.preview_scroll_offset.saturating_add(3);
    }

    pub fn handle_char(&mut self, c: char) {
        self.search_query.push(c);
        self.selected_index = 0;
        self.preview_scroll_offset = 0;
        self.refresh_filtered();
    }

    pub fn handle_backspace(&mut self) {
        if self.search_query.pop().is_some() {
            self.selected_index = 0;
            self.preview_scroll_offset = 0;
            self.refresh_filtered();
        }
    }

    pub fn refresh_filtered(&mut self) {
        let store = match get_global_block_store().read() {
            Ok(s) => s,
            Err(poisoned) => poisoned.into_inner(),
        };

        // 1. Components
        let filter = crate::blocks::store::BlockSearchFilter {
            query: if self.search_query.trim().is_empty() {
                None
            } else {
                Some(self.search_query.clone())
            },
            limit: 5000,
            ..Default::default()
        };
        let mut comp_results = store.search_components(&filter);
        if self.search_query.trim().is_empty() {
            comp_results.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        }
        self.filtered_component_ids = comp_results.into_iter().map(|r| r.id).collect();

        // 2. Palettes (only filter by search_query when active_tab is Palettes)
        let pal_query = if self.active_tab == BlocksTab::Palettes {
            self.search_query.trim()
        } else {
            ""
        };
        let mut pals = store.search_palettes(pal_query);
        pals.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        self.filtered_palette_ids = pals.into_iter().map(|p| p.id).collect();

        // 3. Gradients (only filter by search_query when active_tab is Gradients)
        let grad_query = if self.active_tab == BlocksTab::Gradients {
            self.search_query.trim()
        } else {
            ""
        };
        let mut grads = store.search_gradients(grad_query);
        grads.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        self.filtered_gradient_ids = grads.into_iter().map(|g| g.id).collect();

        // 4. Templates (only filter by search_query when active_tab is Templates)
        let tmpl_query = if self.active_tab == BlocksTab::Templates {
            self.search_query.trim().to_lowercase()
        } else {
            String::new()
        };
        let mut tmpls: Vec<_> = store
            .list_templates()
            .into_iter()
            .filter(|t| {
                tmpl_query.is_empty()
                    || t.name.to_lowercase().contains(&tmpl_query)
                    || t.description.to_lowercase().contains(&tmpl_query)
                    || t.base_layout.to_lowercase().contains(&tmpl_query)
            })
            .collect();
        tmpls.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        self.filtered_template_ids = tmpls.into_iter().map(|t| t.id).collect();

        // Clamp selected index to boundary
        let len = self.current_tab_len();
        if len == 0 {
            self.selected_index = 0;
        } else if self.selected_index >= len {
            self.selected_index = len.saturating_sub(1);
        }
    }

    pub fn get_selected_code(&self) -> Option<String> {
        let store = match get_global_block_store().read() {
            Ok(s) => s,
            Err(poisoned) => poisoned.into_inner(),
        };

        match self.active_tab {
            BlocksTab::Components => {
                let id = self.filtered_component_ids.get(self.selected_index)?;
                store.get_component(id).map(|c| c.code.clone())
            }
            BlocksTab::Palettes => {
                let id = self.filtered_palette_ids.get(self.selected_index)?;
                store.get_palette(id).map(|p| p.to_css_variables())
            }
            BlocksTab::Gradients => {
                let id = self.filtered_gradient_ids.get(self.selected_index)?;
                store.get_gradient(id).map(|g| g.css.clone())
            }
            BlocksTab::Templates => {
                let id = self.filtered_template_ids.get(self.selected_index)?;
                store.get_template(id).map(|t| t.base_layout.clone())
            }
        }
    }
}

/// Renders the complete `/blocks` warehouse modal dialog in Ratatui.
pub fn render_blocks_modal(frame: &mut Frame, state: &BlocksModalState, area: Rect, theme: &Theme) {
    let popup_area = centered_rect(90, 85, area);
    frame.render_widget(Clear, popup_area);

    let store = match get_global_block_store().read() {
        Ok(s) => s,
        Err(poisoned) => poisoned.into_inner(),
    };
    let stats = store.stats();

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.brand_accent))
        .title(" 🧱 MiniBlocks Native UI Component & Design Warehouse ")
        .title_alignment(Alignment::Center)
        .style(Style::default().bg(theme.bg_elevated));
    let inner_area = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Tab navigation
            Constraint::Min(5),    // Content area
            Constraint::Length(1), // Hotkey / status footer
        ])
        .split(inner_area);

    // 1. Tab Bar
    render_tab_bar(frame, chunks[0], state, &stats, theme);

    // 2. Tab Content Pane
    match state.active_tab {
        BlocksTab::Components => {
            render_components_tab(frame, chunks[1], state, &store, theme);
        }
        BlocksTab::Palettes => {
            render_palettes_tab(frame, chunks[1], state, &store, theme);
        }
        BlocksTab::Gradients => {
            render_gradients_tab(frame, chunks[1], state, &store, theme);
        }
        BlocksTab::Templates => {
            render_templates_tab(frame, chunks[1], state, &store, theme);
        }
    }

    // 3. Hotkey & Status Bar
    render_footer(frame, chunks[2], state, theme);
}

fn render_tab_bar(
    frame: &mut Frame,
    area: Rect,
    state: &BlocksModalState,
    stats: &crate::blocks::models::BlockStats,
    theme: &Theme,
) {
    let mut tab_spans = Vec::new();
    for (i, tab) in BlocksTab::all().iter().enumerate() {
        let count = match tab {
            BlocksTab::Components => stats.total_components,
            BlocksTab::Palettes => stats.total_palettes,
            BlocksTab::Gradients => stats.total_gradients,
            BlocksTab::Templates => stats.total_templates,
        };
        let is_active = *tab == state.active_tab;
        let style = if is_active {
            Style::default()
                .fg(Color::Black)
                .bg(theme.brand_accent)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.muted)
        };
        tab_spans.push(Span::styled(
            format!(" [{}] {} ({}) ", i + 1, tab.title(), count),
            style,
        ));
        tab_spans.push(Span::raw(" "));
    }

    let tab_block = Block::default()
        .borders(Borders::BOTTOM)
        .border_style(Style::default().fg(theme.border));
    let tab_p = Paragraph::new(Line::from(tab_spans)).block(tab_block);
    frame.render_widget(tab_p, area);
}

fn render_components_tab(
    frame: &mut Frame,
    area: Rect,
    state: &BlocksModalState,
    store: &crate::blocks::store::BlockStore,
    theme: &Theme,
) {
    let content_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
        .split(area);

    // Left column: Search input + list
    let left_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(4)])
        .split(content_cols[0]);

    // Search input box
    let search_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.brand_accent))
        .title(" Search Components ")
        .style(Style::default().bg(theme.bg_elevated));
    let query_display = if state.search_query.is_empty() {
        Span::styled(" Type to filter...", Style::default().fg(theme.muted))
    } else {
        Span::styled(
            format!(" {}▌", state.search_query),
            Style::default()
                .fg(theme.text_primary)
                .add_modifier(Modifier::BOLD),
        )
    };
    let search_p = Paragraph::new(Line::from(vec![
        Span::styled(" 🔍", Style::default().fg(theme.brand_accent)),
        query_display,
    ]))
    .block(search_block);
    frame.render_widget(search_p, left_chunks[0]);

    // Component List
    let list_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border))
        .title(format!(
            " Components ({}) ",
            state.filtered_component_ids.len()
        ));
    let list_inner = list_block.inner(left_chunks[1]);
    frame.render_widget(list_block, left_chunks[1]);

    let visible_rows = list_inner.height as usize;
    let scroll_offset = compute_scroll_offset(state.selected_index, visible_rows);

    let mut list_lines = Vec::new();
    if state.filtered_component_ids.is_empty() {
        list_lines.push(Line::from(vec![Span::styled(
            "  (No components found)",
            Style::default().fg(theme.muted),
        )]));
    } else {
        let end_idx = (scroll_offset + visible_rows).min(state.filtered_component_ids.len());
        for (i, id) in state.filtered_component_ids[scroll_offset..end_idx]
            .iter()
            .enumerate()
        {
            let actual_idx = scroll_offset + i;
            let is_selected = actual_idx == state.selected_index;

            if let Some(comp) = store.get_component(id) {
                let marker = if is_selected { "▸ " } else { "  " };
                let cat_badge = format!("[{}]", comp.category);
                let fw_badge = format!("[{}]", comp.framework);

                let line_style = if is_selected {
                    Style::default()
                        .fg(theme.highlight)
                        .bg(theme.border)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(theme.text_primary)
                };

                let name_truncated = if comp.name.len() > 18 {
                    format!("{}…", &comp.name[..17])
                } else {
                    comp.name.clone()
                };

                let spans = vec![
                    Span::styled(marker, line_style),
                    Span::styled(format!("{:<19} ", name_truncated), line_style),
                    Span::styled(
                        format!("{:<9} ", cat_badge),
                        Style::default().fg(theme.info),
                    ),
                    Span::styled(fw_badge, Style::default().fg(theme.success)),
                ];
                list_lines.push(Line::from(spans));
            }
        }
    }
    let list_p = Paragraph::new(list_lines);
    frame.render_widget(list_p, list_inner);

    // Right column: Component Details & Code Preview
    let right_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border))
        .title(" Component Details & Preview ");
    let right_inner = right_block.inner(content_cols[1]);
    frame.render_widget(right_block, content_cols[1]);

    if let Some(id) = state.filtered_component_ids.get(state.selected_index) {
        if let Some(comp) = store.get_component(id) {
            let right_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(5), Constraint::Min(4)])
                .split(right_inner);

            // Metadata Header
            let deps_str = if comp.dependencies.is_empty() {
                "none".to_string()
            } else {
                comp.dependencies.join(", ")
            };
            let tags_str = if comp.tags.is_empty() {
                "none".to_string()
            } else {
                comp.tags.join(", ")
            };

            let meta_lines = vec![
                Line::from(vec![
                    Span::styled(
                        format!("{} ", comp.name),
                        Style::default()
                            .fg(theme.brand_accent)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!("(v{}) ", comp.version),
                        Style::default().fg(theme.muted),
                    ),
                    Span::styled(
                        format!("• Category: {} ", comp.category),
                        Style::default().fg(theme.info),
                    ),
                    Span::styled(
                        format!("• Framework: {}", comp.framework),
                        Style::default().fg(theme.success),
                    ),
                ]),
                Line::from(vec![
                    Span::styled("Description: ", Style::default().fg(theme.muted)),
                    Span::styled(&comp.description, Style::default().fg(theme.text_primary)),
                ]),
                Line::from(vec![
                    Span::styled("Dependencies: ", Style::default().fg(theme.muted)),
                    Span::styled(deps_str, Style::default().fg(theme.warning)),
                    Span::styled("   Tags: ", Style::default().fg(theme.muted)),
                    Span::styled(tags_str, Style::default().fg(theme.highlight)),
                ]),
            ];
            let meta_p = Paragraph::new(meta_lines);
            frame.render_widget(meta_p, right_chunks[0]);

            // Code Preview Block
            let preview_box = Block::default()
                .borders(Borders::TOP)
                .border_style(Style::default().fg(theme.border))
                .title(format!(
                    " Code Preview (lines {}+) ",
                    state.preview_scroll_offset + 1
                ));
            let preview_inner = preview_box.inner(right_chunks[1]);
            frame.render_widget(preview_box, right_chunks[1]);

            let code_lines: Vec<&str> = comp.code.lines().collect();
            let visible_preview_height = preview_inner.height as usize;
            let offset = state
                .preview_scroll_offset
                .min(code_lines.len().saturating_sub(1));

            let mut styled_code_lines = Vec::new();
            for (line_idx, line) in code_lines[offset..].iter().enumerate() {
                if line_idx >= visible_preview_height {
                    break;
                }
                let abs_line_no = offset + line_idx + 1;
                let mut spans = vec![Span::styled(
                    format!("{:3} │ ", abs_line_no),
                    Style::default().fg(theme.muted),
                )];
                spans.extend(style_code_line(line, theme));
                styled_code_lines.push(Line::from(spans));
            }
            let code_p = Paragraph::new(styled_code_lines);
            frame.render_widget(code_p, preview_inner);
        }
    } else {
        let empty_p =
            Paragraph::new(" No component selected.").style(Style::default().fg(theme.muted));
        frame.render_widget(empty_p, right_inner);
    }
}

fn render_palettes_tab(
    frame: &mut Frame,
    area: Rect,
    state: &BlocksModalState,
    store: &crate::blocks::store::BlockStore,
    theme: &Theme,
) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border))
        .title(format!(
            " Design Token Palettes ({}) ",
            state.filtered_palette_ids.len()
        ));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if state.filtered_palette_ids.is_empty() {
        let empty_p =
            Paragraph::new(" (No color palettes found)").style(Style::default().fg(theme.muted));
        frame.render_widget(empty_p, inner);
        return;
    }

    let card_height = 4;
    let visible_cards = (inner.height as usize / card_height).max(1);
    let scroll_offset = compute_scroll_offset(state.selected_index, visible_cards);

    let end_idx = (scroll_offset + visible_cards).min(state.filtered_palette_ids.len());
    let mut card_lines = Vec::new();

    for (i, id) in state.filtered_palette_ids[scroll_offset..end_idx]
        .iter()
        .enumerate()
    {
        let actual_idx = scroll_offset + i;
        let is_selected = actual_idx == state.selected_index;

        if let Some(pal) = store.get_palette(id) {
            let cursor = if is_selected { "▸ " } else { "  " };
            let title_style = if is_selected {
                Style::default()
                    .fg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.text_primary)
            };

            let tags_str = if pal.tags.is_empty() {
                String::new()
            } else {
                format!(" [{}]", pal.tags.join(", "))
            };

            // Header line
            card_lines.push(Line::from(vec![
                Span::styled(cursor, title_style),
                Span::styled(&pal.name, title_style),
                Span::styled(tags_str, Style::default().fg(theme.muted)),
            ]));

            // Swatches line with colors
            let labels = ["BG", "Surface", "Accent", "Text"];
            let mut swatch_spans = vec![Span::raw("    ")];
            for (idx, color_hex) in pal.colors.iter().enumerate() {
                let rgb = hex_to_rgb(color_hex).unwrap_or(theme.border);
                let lbl = labels.get(idx).copied().unwrap_or("Color");
                swatch_spans.push(Span::styled(
                    format!("{}: ", lbl),
                    Style::default().fg(theme.muted),
                ));
                swatch_spans.push(Span::styled("████ ", Style::default().fg(rgb)));
                swatch_spans.push(Span::styled(
                    format!("{}   ", color_hex),
                    Style::default().fg(theme.text_primary),
                ));
            }
            card_lines.push(Line::from(swatch_spans));

            // Blank separator line
            card_lines.push(Line::from(""));
        }
    }

    let p = Paragraph::new(card_lines);
    frame.render_widget(p, inner);
}

fn render_gradients_tab(
    frame: &mut Frame,
    area: Rect,
    state: &BlocksModalState,
    store: &crate::blocks::store::BlockStore,
    theme: &Theme,
) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border))
        .title(format!(
            " CSS Gradients ({}) ",
            state.filtered_gradient_ids.len()
        ));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if state.filtered_gradient_ids.is_empty() {
        let empty_p =
            Paragraph::new(" (No gradients found)").style(Style::default().fg(theme.muted));
        frame.render_widget(empty_p, inner);
        return;
    }

    let card_height = 4;
    let visible_cards = (inner.height as usize / card_height).max(1);
    let scroll_offset = compute_scroll_offset(state.selected_index, visible_cards);

    let end_idx = (scroll_offset + visible_cards).min(state.filtered_gradient_ids.len());
    let mut card_lines = Vec::new();

    for (i, id) in state.filtered_gradient_ids[scroll_offset..end_idx]
        .iter()
        .enumerate()
    {
        let actual_idx = scroll_offset + i;
        let is_selected = actual_idx == state.selected_index;

        if let Some(grad) = store.get_gradient(id) {
            let cursor = if is_selected { "▸ " } else { "  " };
            let title_style = if is_selected {
                Style::default()
                    .fg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.text_primary)
            };

            let tags_str = if grad.tags.is_empty() {
                String::new()
            } else {
                format!(" [{}]", grad.tags.join(", "))
            };

            // Title & Swatches header
            let mut top_spans = vec![
                Span::styled(cursor, title_style),
                Span::styled(&grad.name, title_style),
                Span::styled(tags_str, Style::default().fg(theme.muted)),
                Span::raw("    "),
            ];
            for (c_idx, col) in grad.colors.iter().enumerate() {
                let rgb = hex_to_rgb(col).unwrap_or(theme.border);
                if c_idx > 0 {
                    top_spans.push(Span::styled(" ─> ", Style::default().fg(theme.muted)));
                }
                top_spans.push(Span::styled("████ ", Style::default().fg(rgb)));
                top_spans.push(Span::styled(col.clone(), Style::default().fg(theme.info)));
            }
            card_lines.push(Line::from(top_spans));

            // CSS Definition
            card_lines.push(Line::from(vec![
                Span::raw("    "),
                Span::styled("CSS: ", Style::default().fg(theme.muted)),
                Span::styled(&grad.css, Style::default().fg(theme.success)),
            ]));

            // Separator
            card_lines.push(Line::from(""));
        }
    }

    let p = Paragraph::new(card_lines);
    frame.render_widget(p, inner);
}

fn render_templates_tab(
    frame: &mut Frame,
    area: Rect,
    state: &BlocksModalState,
    store: &crate::blocks::store::BlockStore,
    theme: &Theme,
) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border))
        .title(format!(
            " Page Layout Templates ({}) ",
            state.filtered_template_ids.len()
        ));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if state.filtered_template_ids.is_empty() {
        let empty_p =
            Paragraph::new(" (No templates found)").style(Style::default().fg(theme.muted));
        frame.render_widget(empty_p, inner);
        return;
    }

    let card_height = 5;
    let visible_cards = (inner.height as usize / card_height).max(1);
    let scroll_offset = compute_scroll_offset(state.selected_index, visible_cards);

    let end_idx = (scroll_offset + visible_cards).min(state.filtered_template_ids.len());
    let mut card_lines = Vec::new();

    for (i, id) in state.filtered_template_ids[scroll_offset..end_idx]
        .iter()
        .enumerate()
    {
        let actual_idx = scroll_offset + i;
        let is_selected = actual_idx == state.selected_index;

        if let Some(tmpl) = store.get_template(id) {
            let cursor = if is_selected { "▸ " } else { "  " };
            let title_style = if is_selected {
                Style::default()
                    .fg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.text_primary)
            };

            // Title line
            card_lines.push(Line::from(vec![
                Span::styled(cursor, title_style),
                Span::styled(&tmpl.name, title_style),
                Span::styled(
                    format!(" ({} linked components)", tmpl.component_ids.len()),
                    Style::default().fg(theme.info),
                ),
            ]));

            // Description
            card_lines.push(Line::from(vec![
                Span::raw("    "),
                Span::styled(&tmpl.description, Style::default().fg(theme.muted)),
            ]));

            // Base Layout
            let layout_preview = tmpl.base_layout.lines().next().unwrap_or("");
            card_lines.push(Line::from(vec![
                Span::raw("    "),
                Span::styled("Layout: ", Style::default().fg(theme.muted)),
                Span::styled(layout_preview, Style::default().fg(theme.success)),
            ]));

            // Separator
            card_lines.push(Line::from(""));
        }
    }

    let p = Paragraph::new(card_lines);
    frame.render_widget(p, inner);
}

fn render_footer(frame: &mut Frame, area: Rect, state: &BlocksModalState, theme: &Theme) {
    let mut footer_spans = vec![
        Span::styled(
            " [Tab] ",
            Style::default()
                .fg(theme.brand_accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("Next Tab  ", Style::default().fg(theme.text_primary)),
        Span::styled(
            " [↑/↓] ",
            Style::default()
                .fg(theme.brand_accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("Select  ", Style::default().fg(theme.text_primary)),
        Span::styled(
            " [PgUp/PgDn] ",
            Style::default()
                .fg(theme.brand_accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("Scroll  ", Style::default().fg(theme.text_primary)),
        Span::styled(
            " [Enter] ",
            Style::default()
                .fg(theme.brand_accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("Insert  ", Style::default().fg(theme.text_primary)),
        Span::styled(
            " [c] ",
            Style::default()
                .fg(theme.brand_accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("Copy  ", Style::default().fg(theme.text_primary)),
        Span::styled(
            " [Esc] ",
            Style::default()
                .fg(theme.brand_accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("Close", Style::default().fg(theme.text_primary)),
    ];

    if let Some(ref msg) = state.status_message {
        footer_spans.push(Span::raw("   "));
        footer_spans.push(Span::styled(
            msg.clone(),
            Style::default()
                .fg(theme.success)
                .add_modifier(Modifier::BOLD),
        ));
    }

    let p = Paragraph::new(Line::from(footer_spans));
    frame.render_widget(p, area);
}

/// Helper that parses a code line into syntax-highlighted Ratatui Spans.
fn style_code_line<'a>(line: &'a str, theme: &'a Theme) -> Vec<Span<'a>> {
    let trimmed = line.trim_start();
    if trimmed.starts_with("//") || trimmed.starts_with("/*") || trimmed.starts_with('*') {
        return vec![Span::styled(line, Style::default().fg(theme.muted))];
    }

    let mut spans = Vec::new();
    let indent_len = line.len() - trimmed.len();
    if indent_len > 0 {
        spans.push(Span::raw(&line[..indent_len]));
    }

    let mut remaining = trimmed;
    while !remaining.is_empty() {
        if remaining.starts_with("//") {
            spans.push(Span::styled(remaining, Style::default().fg(theme.muted)));
            break;
        }

        // String literals
        if remaining.starts_with('"') || remaining.starts_with('\'') || remaining.starts_with('`') {
            let quote = remaining.chars().next().unwrap_or('"');
            if let Some(end_idx) = remaining[1..].find(quote) {
                let str_token = &remaining[..=end_idx + 1];
                spans.push(Span::styled(str_token, Style::default().fg(theme.success)));
                remaining = &remaining[end_idx + 2..];
            } else {
                spans.push(Span::styled(remaining, Style::default().fg(theme.success)));
                break;
            }
            continue;
        }

        // HTML/JSX tag delimiters
        if remaining.starts_with("</")
            || remaining.starts_with("/>")
            || remaining.starts_with('<')
            || remaining.starts_with('>')
        {
            let tag_len = if remaining.starts_with("</") || remaining.starts_with("/>") {
                2
            } else {
                1
            };
            spans.push(Span::styled(
                &remaining[..tag_len],
                Style::default().fg(theme.info),
            ));
            remaining = &remaining[tag_len..];
            continue;
        }

        // Identifiers and keywords
        let word_end = remaining
            .find(|c: char| !c.is_alphanumeric() && c != '_' && c != '$')
            .unwrap_or(remaining.len());
        if word_end > 0 {
            let word = &remaining[..word_end];
            let is_keyword = matches!(
                word,
                "import"
                    | "export"
                    | "from"
                    | "default"
                    | "function"
                    | "const"
                    | "let"
                    | "var"
                    | "return"
                    | "class"
                    | "interface"
                    | "type"
                    | "extends"
                    | "implements"
                    | "async"
                    | "await"
                    | "if"
                    | "else"
                    | "match"
                    | "switch"
                    | "case"
                    | "break"
                    | "continue"
                    | "for"
                    | "while"
                    | "new"
                    | "this"
                    | "true"
                    | "false"
                    | "null"
                    | "undefined"
                    | "pub"
                    | "fn"
                    | "struct"
                    | "enum"
                    | "impl"
                    | "mut"
                    | "use"
                    | "mod"
            );

            if is_keyword {
                spans.push(Span::styled(
                    word,
                    Style::default()
                        .fg(theme.brand_accent)
                        .add_modifier(Modifier::BOLD),
                ));
            } else if word.starts_with(|c: char| c.is_ascii_digit()) {
                spans.push(Span::styled(word, Style::default().fg(theme.warning)));
            } else if word == "className"
                || word == "class"
                || word == "style"
                || word == "id"
                || word == "onClick"
                || word == "onChange"
            {
                spans.push(Span::styled(word, Style::default().fg(theme.highlight)));
            } else {
                spans.push(Span::styled(word, Style::default().fg(theme.text_primary)));
            }
            remaining = &remaining[word_end..];
        } else {
            let char_len = remaining.chars().next().map(|c| c.len_utf8()).unwrap_or(1);
            spans.push(Span::styled(
                &remaining[..char_len],
                Style::default().fg(theme.muted),
            ));
            remaining = &remaining[char_len..];
        }
    }

    spans
}

/// Parses a hex color string into a Ratatui RGB Color.
fn hex_to_rgb(hex: &str) -> Option<Color> {
    let s = hex.trim().strip_prefix('#')?;
    if !s.is_ascii() {
        return None;
    }
    if s.len() == 6 {
        let r = u8::from_str_radix(&s[0..2], 16).ok()?;
        let g = u8::from_str_radix(&s[2..4], 16).ok()?;
        let b = u8::from_str_radix(&s[4..6], 16).ok()?;
        Some(Color::Rgb(r, g, b))
    } else if s.len() == 3 {
        let r = u8::from_str_radix(&s[0..1], 16).ok()? * 17;
        let g = u8::from_str_radix(&s[1..2], 16).ok()? * 17;
        let b = u8::from_str_radix(&s[2..3], 16).ok()? * 17;
        Some(Color::Rgb(r, g, b))
    } else {
        None
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use tempfile::tempdir;

    #[test]
    fn test_blocks_modal_navigation_and_tabs() {
        let temp = tempdir().unwrap();
        let mut state = BlocksModalState::new(temp.path());

        // Initial state
        assert_eq!(state.active_tab, BlocksTab::Components);
        assert_eq!(state.active_tab.title(), "Components");

        // Tab cycling forwards
        state.next_tab();
        assert_eq!(state.active_tab, BlocksTab::Palettes);
        assert_eq!(state.active_tab.title(), "Palettes");

        state.next_tab();
        assert_eq!(state.active_tab, BlocksTab::Gradients);
        assert_eq!(state.active_tab.title(), "Gradients");

        state.next_tab();
        assert_eq!(state.active_tab, BlocksTab::Templates);
        assert_eq!(state.active_tab.title(), "Templates");

        state.next_tab();
        assert_eq!(state.active_tab, BlocksTab::Components);

        // Tab cycling backwards
        state.prev_tab();
        assert_eq!(state.active_tab, BlocksTab::Templates);

        state.prev_tab();
        assert_eq!(state.active_tab, BlocksTab::Gradients);

        state.prev_tab();
        assert_eq!(state.active_tab, BlocksTab::Palettes);

        state.prev_tab();
        assert_eq!(state.active_tab, BlocksTab::Components);

        // Boundary clamping on item selection
        assert_eq!(state.selected_index, 0);
        state.select_prev();
        assert_eq!(state.selected_index, 0);

        let count = state.current_tab_len();
        if count > 1 {
            state.select_next();
            assert_eq!(state.selected_index, 1);
            state.select_prev();
            assert_eq!(state.selected_index, 0);
        }
    }

    #[test]
    fn test_blocks_modal_filtering() {
        let temp = tempdir().unwrap();
        let mut state = BlocksModalState::new(temp.path());

        let initial_comp_count = state.filtered_component_ids.len();

        // Type query 'navbar'
        for c in "navbar".chars() {
            state.handle_char(c);
        }
        assert_eq!(state.search_query, "navbar");
        // Filtered count should be non-empty and at most total count
        assert!(state.filtered_component_ids.len() <= initial_comp_count);

        // Backspace
        state.handle_backspace();
        assert_eq!(state.search_query, "navba");

        // Non-existent search query
        for c in "xyz999nonexistentfilter".chars() {
            state.handle_char(c);
        }
        assert_eq!(state.filtered_component_ids.len(), 0);
        assert_eq!(state.selected_index, 0);
    }

    #[test]
    fn test_blocks_modal_preview_scrolling() {
        let temp = tempdir().unwrap();
        let mut state = BlocksModalState::new(temp.path());

        assert_eq!(state.preview_scroll_offset, 0);

        state.scroll_preview_down();
        assert_eq!(state.preview_scroll_offset, 3);

        state.scroll_preview_down();
        assert_eq!(state.preview_scroll_offset, 6);

        state.scroll_preview_up();
        assert_eq!(state.preview_scroll_offset, 3);

        state.scroll_preview_up();
        assert_eq!(state.preview_scroll_offset, 0);

        // Saturating at 0
        state.scroll_preview_up();
        assert_eq!(state.preview_scroll_offset, 0);
    }

    #[test]
    fn test_blocks_tab_methods() {
        let tabs = BlocksTab::all();
        assert_eq!(tabs.len(), 4);
        assert_eq!(BlocksTab::Components.next(), BlocksTab::Palettes);
        assert_eq!(BlocksTab::Palettes.next(), BlocksTab::Gradients);
        assert_eq!(BlocksTab::Gradients.next(), BlocksTab::Templates);
        assert_eq!(BlocksTab::Templates.next(), BlocksTab::Components);

        assert_eq!(BlocksTab::Components.prev(), BlocksTab::Templates);
        assert_eq!(BlocksTab::Palettes.prev(), BlocksTab::Components);
        assert_eq!(BlocksTab::Gradients.prev(), BlocksTab::Palettes);
        assert_eq!(BlocksTab::Templates.prev(), BlocksTab::Gradients);
    }

    #[test]
    fn test_blocks_modal_render() {
        let temp = tempdir().unwrap();
        let state = BlocksModalState::new(temp.path());
        let theme = Theme::aura_dark();

        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|f| {
                let area = f.area();
                render_blocks_modal(f, &state, area, &theme);
            })
            .unwrap();

        // Also test rendering for the other tabs
        for tab in [
            BlocksTab::Palettes,
            BlocksTab::Gradients,
            BlocksTab::Templates,
        ] {
            let mut tab_state = state.clone();
            tab_state.active_tab = tab;
            terminal
                .draw(|f| {
                    let area = f.area();
                    render_blocks_modal(f, &tab_state, area, &theme);
                })
                .unwrap();
        }
    }

    #[test]
    fn test_get_selected_code() {
        let temp = tempdir().unwrap();
        let mut state = BlocksModalState::new(temp.path());

        // In Components tab
        let code = state.get_selected_code();
        if !state.filtered_component_ids.is_empty() {
            assert!(code.is_some());
        }

        // In Palettes tab
        state.next_tab();
        let pal_code = state.get_selected_code();
        if !state.filtered_palette_ids.is_empty() {
            assert!(pal_code.is_some());
            assert!(pal_code.unwrap().contains(":root"));
        }

        // In Gradients tab
        state.next_tab();
        let grad_code = state.get_selected_code();
        if !state.filtered_gradient_ids.is_empty() {
            assert!(grad_code.is_some());
        }

        // In Templates tab
        state.next_tab();
        let tmpl_code = state.get_selected_code();
        if !state.filtered_template_ids.is_empty() {
            assert!(tmpl_code.is_some());
        }
    }
}

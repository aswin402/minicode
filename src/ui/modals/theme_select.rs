//! Theme switcher and streaming configuration modals rendering.

use crate::constants::{
    THEME_MODAL_MAX_VISIBLE, THEME_NAME_DISPLAY_COLS, THEME_SELECT_HEIGHT_PCT,
    THEME_SELECT_WIDTH_PCT,
};
use crate::ui::layout_utils::centered_rect;
use crate::ui::modals::common::compute_scroll_offset;
use crate::ui::theme::{Theme, ThemeInfo};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

#[derive(Debug, Clone)]
pub struct TodoStyleOption {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub preview: &'static [&'static str],
}

pub static TODO_STYLE_OPTIONS: &[TodoStyleOption] = &[
    TodoStyleOption {
        id: "tree",
        name: "Tree (Oh My Pi)",
        description: "Branch connectors with status icons (Default)",
        preview: &[
            "TODO",
            "└── Plan · 1/3",
            "    ├── ✔ T1: Scaffolding and project structure setup",
            "    ├── ▶ T2: Form validation logic and interactive UI",
            "    └── ○ T3: End-to-end verification and documentation",
        ],
    },
    TodoStyleOption {
        id: "card",
        name: "Card Container",
        description: "Rounded boxed card container with milestone badge",
        preview: &[
            "╭── 📋 TODO · 1/3 Tasks Completed ───────────────── [Phase 142] ──╮",
            "│ └── Tasks                                                      │",
            "│     ├── ✔ T1: Scaffolding and project structure setup          │",
            "│     ├── ▶ T2: Form validation logic and interactive UI         │",
            "│     └── ○ T3: End-to-end verification and documentation        │",
            "╰────────────────────────────────────────────────────────────────╯",
        ],
    },
    TodoStyleOption {
        id: "rail",
        name: "Rail Accent",
        description: "Left accent rail bar with clean compact task list",
        preview: &[
            "▎ 📋 TODO · 1/3 Completed · [Phase 142]",
            "▎   ✔ T1: Scaffolding and project structure setup",
            "▎   ▶ T2: Form validation logic and interactive UI",
            "▎   ○ T3: End-to-end verification and documentation",
        ],
    },
    TodoStyleOption {
        id: "minimal",
        name: "Minimalist Rule",
        description: "Minimalist rule header with indented step list",
        preview: &[
            "📋 TODO · 1/3 Completed ───────────────────────────── [Phase 142]",
            "   ✔ T1: Scaffolding and project structure setup",
            "   ▶ T2: Form validation logic and interactive UI",
            "   ○ T3: End-to-end verification and documentation",
        ],
    },
];

pub struct ThemeSelectContext<'a> {
    pub themes: &'a [ThemeInfo],
    pub animations: &'a [crate::ui::animation::AnimationOption],
    pub todo_styles: &'a [TodoStyleOption],
    pub active_tab: crate::ui::modals::ThemeModalTab,
    pub theme_selected_index: usize,
    pub animation_selected_index: usize,
    pub todo_style_selected_index: usize,
    pub active_theme_id: &'a str,
    pub active_animation_id: &'a str,
    pub active_todo_style: &'a str,
}

pub fn render_theme_select(
    frame: &mut Frame,
    area: Rect,
    theme: &Theme,
    ctx: &ThemeSelectContext<'_>,
) {
    let ThemeSelectContext {
        themes,
        animations,
        todo_styles,
        active_tab,
        theme_selected_index,
        animation_selected_index,
        todo_style_selected_index,
        active_theme_id,
        active_animation_id,
        active_todo_style,
    } = *ctx;

    let popup_area = centered_rect(THEME_SELECT_WIDTH_PCT, THEME_SELECT_HEIGHT_PCT, area);
    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(Line::from(vec![Span::styled(
            " 🎨 Theme & Animation Customization ",
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
            Constraint::Min(4),    // Items list
            Constraint::Length(1), // Footer key hints
        ])
        .split(inner_area);

    // 1. Tab header bar: [ 1. Themes ]   •   [ 2. Animations ]   •   [ 3. Todo Styles ]
    let is_themes_tab = active_tab == crate::ui::modals::ThemeModalTab::Themes;
    let is_anim_tab = active_tab == crate::ui::modals::ThemeModalTab::Animations;
    let is_todo_tab = active_tab == crate::ui::modals::ThemeModalTab::TodoStyles;

    let tab1_style = if is_themes_tab {
        Style::default()
            .fg(theme.bg_primary)
            .bg(theme.brand_accent)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.text_primary)
    };

    let tab2_style = if is_anim_tab {
        Style::default()
            .fg(theme.bg_primary)
            .bg(theme.brand_accent)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.text_primary)
    };

    let tab3_style = if is_todo_tab {
        Style::default()
            .fg(theme.bg_primary)
            .bg(theme.brand_accent)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.text_primary)
    };

    let tab_line = Line::from(vec![
        Span::raw("  "),
        Span::styled(
            if is_themes_tab {
                " [ 1. Themes (Tab) ] "
            } else {
                " [ 1. Themes ] "
            },
            tab1_style,
        ),
        Span::raw("   "),
        Span::styled(
            if is_anim_tab {
                " [ 2. Animations (Tab) ] "
            } else {
                " [ 2. Animations ] "
            },
            tab2_style,
        ),
        Span::raw("   "),
        Span::styled(
            if is_todo_tab {
                " [ 3. Todo Styles (Tab) ] "
            } else {
                " [ 3. Todo Styles ] "
            },
            tab3_style,
        ),
        Span::raw("    "),
        Span::styled("Tab/1/2/3 to switch", Style::default().fg(theme.muted)),
    ]);
    frame.render_widget(Paragraph::new(tab_line), chunks[0]);

    // Top Divider
    let divider = Paragraph::new(Line::from(vec![Span::styled(
        "─".repeat(inner_area.width as usize),
        Style::default().fg(theme.border),
    )]));
    frame.render_widget(divider, chunks[1]);

    // 2. Items List based on active tab
    let mut lines = Vec::new();
    lines.push(Line::from(""));

    let max_visible = THEME_MODAL_MAX_VISIBLE;

    match active_tab {
        crate::ui::modals::ThemeModalTab::Themes => {
            let scroll_offset = compute_scroll_offset(theme_selected_index, max_visible);
            let visible_themes = themes.iter().skip(scroll_offset).take(max_visible);

            for (idx_rel, t) in visible_themes.enumerate() {
                let idx = scroll_offset + idx_rel;
                let is_selected = idx == theme_selected_index;
                let is_active = t.id == active_theme_id || active_theme_id.starts_with(&t.id);

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

                if is_active {
                    header_spans.push(Span::styled(
                        "  [Active ✔]",
                        Style::default()
                            .fg(theme.success)
                            .add_modifier(Modifier::BOLD),
                    ));
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
        }
        crate::ui::modals::ThemeModalTab::Animations => {
            let scroll_offset = compute_scroll_offset(animation_selected_index, max_visible);
            let visible_anims = animations.iter().skip(scroll_offset).take(max_visible);

            let millis = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;

            for (idx_rel, anim) in visible_anims.enumerate() {
                let idx = scroll_offset + idx_rel;
                let is_selected = idx == animation_selected_index;
                let is_active = anim.id == active_animation_id;

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
                    Span::styled(format!("{:<26}", anim.name), title_style),
                ];

                // Render live animated preview spinner
                let preview_spans =
                    crate::ui::animation::render_preview_spinner(anim.style, millis, theme);
                header_spans.extend(preview_spans);

                if is_active {
                    header_spans.push(Span::styled(
                        " [Active ✔]",
                        Style::default()
                            .fg(theme.success)
                            .add_modifier(Modifier::BOLD),
                    ));
                }

                lines.push(Line::from(header_spans));

                let desc_style = if is_selected {
                    Style::default().fg(theme.text_primary)
                } else {
                    Style::default().fg(theme.muted)
                };
                lines.push(Line::from(vec![
                    Span::styled("        ", Style::default()),
                    Span::styled(anim.description, desc_style),
                ]));

                lines.push(Line::from(""));
            }
        }
        crate::ui::modals::ThemeModalTab::TodoStyles => {
            for (idx, opt) in todo_styles.iter().enumerate() {
                let is_selected = idx == todo_style_selected_index;
                let is_active = opt.id == active_todo_style;

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
                    Span::styled(format!("{:<22}", opt.name), title_style),
                ];

                if is_active {
                    header_spans.push(Span::styled(
                        " [Active ✔]",
                        Style::default()
                            .fg(theme.success)
                            .add_modifier(Modifier::BOLD),
                    ));
                }

                lines.push(Line::from(header_spans));

                let desc_style = if is_selected {
                    Style::default().fg(theme.text_primary)
                } else {
                    Style::default().fg(theme.muted)
                };
                lines.push(Line::from(vec![
                    Span::styled("        ", Style::default()),
                    Span::styled(opt.description, desc_style),
                ]));

                // Live preview of the selected todo style
                if is_selected {
                    lines.push(Line::from(""));
                    for preview_line in opt.preview {
                        lines.push(Line::from(vec![
                            Span::raw("        "),
                            Span::styled(*preview_line, Style::default().fg(theme.info)),
                        ]));
                    }
                }

                lines.push(Line::from(""));
            }
        }
    }

    let items_p = Paragraph::new(lines);
    frame.render_widget(items_p, chunks[2]);

    // 3. Footer Key Hints
    let footer_line = Line::from(vec![
        Span::styled(
            "  [Tab/1/2/3] ",
            Style::default()
                .fg(theme.brand_accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("Tabs   ", Style::default().fg(theme.text_primary)),
        Span::styled(
            "[↑/↓] ",
            Style::default()
                .fg(theme.brand_accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("Move   ", Style::default().fg(theme.text_primary)),
        Span::styled(
            "[Enter] ",
            Style::default()
                .fg(theme.brand_accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("Apply   ", Style::default().fg(theme.text_primary)),
        Span::styled("[Esc] ", Style::default().fg(theme.muted)),
        Span::styled("Cancel", Style::default().fg(theme.muted)),
    ]);
    frame.render_widget(Paragraph::new(footer_line), chunks[3]);
}

// Re-export for backward compatibility
#[allow(unused_imports)]
pub use super::streaming_select::render_streaming_select;

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

#[derive(Debug, Clone)]
pub struct SwarmStyleOption {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub preview: &'static [&'static str],
}

pub static SWARM_STYLE_OPTIONS: &[SwarmStyleOption] = &[
    SwarmStyleOption {
        id: "stylish",
        name: "Minimal + Stylish (Neo-Nordic)",
        description: "Thin borders, stylish accent badges, micro-meters (Default)",
        preview: &[
            "┌── ⚡ SWARM FLIGHT DECK ───────── swarm_finance ── [01:24] ──┐",
            "│ ❯ t1_calc    [RUNNING] ━╾────── 65%  turn 3  14.2k tokens │",
            "│ ❯ t2_form    [RUNNING] ━╾──── 50%    turn 2  11.8k tokens │",
            "├── 💬 MESSAGE BUS ──────────────────────────────── 3 msgs ─┤",
            "│ 20:38:49 t2 ──▶ t1 [QUERY]    \"What return types?\"       │",
            "│ 20:39:04 t1 ──▶ t2 [CONTRACT] \"ROISummary, Dataclass\"     │",
            "└─ [Tab] Switch   [Space] Logs   [k] Kill   [q] Close ──────┘",
        ],
    },
    SwarmStyleOption {
        id: "gitgraph",
        name: "Minimal + GitGraph Pipeline",
        description: "Topological branch DAG nodes with inline comms",
        preview: &[
            "● [HEAD: main] Swarm Orchestrator (Wave 1: Active)",
            "├─┬─● [t1_calc] ⚡ Running (42s) · patch_file(\"calc.py\")",
            "│ │ │  └──💬 [query_interface] t2 ──▶ t1: \"Return types?\"",
            "│ │ │  └──💬 [publish_contract] t1 ──▶ t2: \"ROISummary\"",
            "│ └─● [t2_form] ⚡ Running (38s) · exec_cmd(\"pytest\")",
            "├───● [auto-merge] Wave 1 Synchronization Barrier",
            "└───○ [t3_tests] ⏳ Pending (Wave 2) · QA Integration",
        ],
    },
    SwarmStyleOption {
        id: "modern",
        name: "Clean Modernist (Floating Cards)",
        description: "Rounded boxed cards with pill badges and status dots",
        preview: &[
            "╭─ [Worker 1] t1_calc ────────╮ ╭─ [Worker 2] t2_form ────────╮",
            "│ ⚡ RUNNING  Turn 3/5         │ │ ⚡ RUNNING  Turn 2/5         │",
            "│ patch_file(calculator.py)   │ │ exec_cmd(pytest formatter)   │",
            "│ [████████████░░░░] 65%      │ │ [██████████░░░░░░] 50%       │",
            "╰─────────────────────────────╯ ╰──────────────────────────────╯",
        ],
    },
    SwarmStyleOption {
        id: "minimal",
        name: "Ultra-Minimalist (Whitespace Tree)",
        description: "Zero-border clean whitespace with muted guide lines",
        preview: &[
            "WAVE 1  PARALLEL CONCURRENCY (2 WORKERS)",
            "│",
            "├─ ⚡ t1_calculator_core  (Financial Calculator)",
            "│    tool › patch_file(\"calculator.py\")",
            "├─ ⚡ t2_formatter_core   (Financial Formatter)",
            "│    tool › exec_cmd(\"pytest formatter.py\")",
            "└─ ○ t3_tests (QA) — waiting on Wave 1",
        ],
    },
    SwarmStyleOption {
        id: "cockpit",
        name: "High-Density Cockpit (3-Column Split)",
        description: "3-column flight deck: Workers, Message Bus, Live Logs",
        preview: &[
            "┌── SWARM COCKPIT ────────────────────────────── 2 Workers Active ┐",
            "│ ▶ [t1] Calc Engine  │ 20:38:49 t2 ──? t1 │ 12 | @dataclass     │",
            "│   Tool: patch_file  │ \"Need schema\"      │ 13 | class Amort:   │",
            "│   Dur: 42s · 14.2k  │ 20:39:04 t1 ──! t2 │ 14 |   principal    │",
            "│ ▶ [t2] Formatter    │ \"ROISummary\"       │ [test] passed       │",
            "└─────────────────────┴────────────────────┴─────────────────────┘",
        ],
    },
];

#[derive(Debug, Clone)]
pub struct ToastStyleOption {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub preview: &'static [&'static str],
}

pub const TOAST_STYLE_OPTIONS: &[ToastStyleOption] = &[
    ToastStyleOption {
        id: "opencode",
        name: "OpenCode Split-Bar (Default)",
        description: "Thick left & right vertical accent bars (┃…┃) with breathing room and zero horizontal borders",
        preview: &[
            "┃                                          ┃",
            "┃  ✔ Subagent Completed                27s ┃",
            "┃    \"Security Auditor\" finished           ┃",
            "┃                                          ┃",
        ],
    },
    ToastStyleOption {
        id: "rounded",
        name: "Clean Rounded Card",
        description: "Minimalist rounded border (╭─╮…╰─╯) with crisp typography and zero progress bar clutter",
        preview: &[
            "╭──────────────────────────────────────────╮",
            "│  ✔ Subagent Completed                27s │",
            "│    \"Security Auditor\" finished           │",
            "╰──────────────────────────────────────────╯",
        ],
    },
    ToastStyleOption {
        id: "rounded_padded",
        name: "Spacious Rounded Card",
        description: "Rounded card (╭─╮…╰─╯) with top and bottom breathing rows matching OpenCode's vertical rhythm",
        preview: &[
            "╭──────────────────────────────────────────╮",
            "│                                          │",
            "│  ✔ Subagent Completed                27s │",
            "│    \"Security Auditor\" finished           │",
            "│                                          │",
            "╰──────────────────────────────────────────╯",
        ],
    },
    ToastStyleOption {
        id: "bracket",
        name: "Square Corner-Bracket Frame",
        description: "Architectural corner brackets (┌─ … ─┐ / └─ … ─┘) with vertical side rails",
        preview: &[
            "┌─                                        ─┐",
            "│  ✔ Subagent Completed                27s │",
            "│    \"Security Auditor\" finished           │",
            "└─                                        ─┘",
        ],
    },
    ToastStyleOption {
        id: "left_bar",
        name: "Minimal Left Accent Bar",
        description: "Ultra-clean single left spine (┃) on an elevated card with open top, right, and bottom",
        preview: &[
            "┃                                           ",
            "┃  ✔ Subagent Completed                27s  ",
            "┃    \"Security Auditor\" finished            ",
            "┃                                           ",
        ],
    },
    ToastStyleOption {
        id: "cradle_round",
        name: "Asymmetric Half-Top Cradle (Rounded)",
        description: "Partial top overhang (╭────), full left spine (│), and full bottom baseline (╰────────)",
        preview: &[
            "╭──────────────────                         ",
            "│  ✔ Subagent Completed                27s  ",
            "│    \"Security Auditor\" finished            ",
            "╰───────────────────────────────────────────",
        ],
    },
    ToastStyleOption {
        id: "cradle_square",
        name: "Asymmetric Half-Top Cradle (Square)",
        description: "Sharp square partial top overhang (┌────), left spine (│), and full bottom baseline (└────────)",
        preview: &[
            "┌──────────────────                         ",
            "│  ✔ Subagent Completed                27s  ",
            "│    \"Security Auditor\" finished            ",
            "└───────────────────────────────────────────",
        ],
    },
    ToastStyleOption {
        id: "l_round",
        name: "Asymmetric L-Frame (Rounded)",
        description: "Open top and right — vertical left spine meeting a full bottom baseline at a rounded corner (╰────)",
        preview: &[
            "│  ✔ Subagent Completed                27s  ",
            "│    \"Security Auditor\" finished            ",
            "╰───────────────────────────────────────────",
        ],
    },
    ToastStyleOption {
        id: "l_square",
        name: "Asymmetric L-Frame (Square)",
        description: "Open top and right — vertical left spine meeting a full bottom baseline at a sharp corner (└────)",
        preview: &[
            "│  ✔ Subagent Completed                27s  ",
            "│    \"Security Auditor\" finished            ",
            "└───────────────────────────────────────────",
        ],
    },
];

pub struct ThemeSelectContext<'a> {
    pub themes: &'a [ThemeInfo],
    pub animations: &'a [crate::ui::animation::AnimationOption],
    pub todo_styles: &'a [TodoStyleOption],
    pub swarm_styles: &'a [SwarmStyleOption],
    pub toast_styles: &'a [ToastStyleOption],
    pub active_tab: crate::ui::modals::ThemeModalTab,
    pub theme_selected_index: usize,
    pub animation_selected_index: usize,
    pub todo_style_selected_index: usize,
    pub swarm_style_selected_index: usize,
    pub toast_style_selected_index: usize,
    pub active_theme_id: &'a str,
    pub active_animation_id: &'a str,
    pub active_todo_style: &'a str,
    pub active_swarm_style: &'a str,
    pub active_toast_style: &'a str,
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
        swarm_styles,
        toast_styles,
        active_tab,
        theme_selected_index,
        animation_selected_index,
        todo_style_selected_index,
        swarm_style_selected_index,
        toast_style_selected_index,
        active_theme_id,
        active_animation_id,
        active_todo_style,
        active_swarm_style,
        active_toast_style,
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

    // 1. Tab header bar: [ 1. Themes ] • [ 2. Animations ] • [ 3. Todo Styles ] • [ 4. Swarm Styles ] • [ 5. Toast Styles ]
    let is_themes_tab = active_tab == crate::ui::modals::ThemeModalTab::Themes;
    let is_anim_tab = active_tab == crate::ui::modals::ThemeModalTab::Animations;
    let is_todo_tab = active_tab == crate::ui::modals::ThemeModalTab::TodoStyles;
    let is_swarm_tab = active_tab == crate::ui::modals::ThemeModalTab::SwarmStyles;
    let is_toast_tab = active_tab == crate::ui::modals::ThemeModalTab::ToastStyles;

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

    let tab4_style = if is_swarm_tab {
        Style::default()
            .fg(theme.bg_primary)
            .bg(theme.brand_accent)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.text_primary)
    };

    let tab5_style = if is_toast_tab {
        Style::default()
            .fg(theme.bg_primary)
            .bg(theme.brand_accent)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.text_primary)
    };

    let tab_line = Line::from(vec![
        Span::raw(" "),
        Span::styled(" [ 1. Themes ] ", tab1_style),
        Span::raw("  "),
        Span::styled(" [ 2. Animations ] ", tab2_style),
        Span::raw("  "),
        Span::styled(" [ 3. Todo Styles ] ", tab3_style),
        Span::raw("  "),
        Span::styled(" [ 4. Swarm Styles ] ", tab4_style),
        Span::raw("  "),
        Span::styled(" [ 5. Toast Styles ] ", tab5_style),
        Span::raw("   "),
        Span::styled("Tab/1-5", Style::default().fg(theme.muted)),
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
        crate::ui::modals::ThemeModalTab::SwarmStyles => {
            for (idx, opt) in swarm_styles.iter().enumerate() {
                let is_selected = idx == swarm_style_selected_index;
                let is_active = opt.id == active_swarm_style;

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
                    Span::styled(format!("{:<32}", opt.name), title_style),
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

                // Live preview of the selected swarm style
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
        crate::ui::modals::ThemeModalTab::ToastStyles => {
            let toast_max_visible = 5;
            let scroll_offset =
                compute_scroll_offset(toast_style_selected_index, toast_max_visible);
            let visible_toasts = toast_styles
                .iter()
                .skip(scroll_offset)
                .take(toast_max_visible);

            for (idx_rel, opt) in visible_toasts.enumerate() {
                let idx = scroll_offset + idx_rel;
                let is_selected = idx == toast_style_selected_index;
                let is_active = opt.id == active_toast_style;

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
                    Span::styled(format!("{:<40}", opt.name), title_style),
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

                // Live preview of the selected toast style
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
            "  [Tab/1-5] ",
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

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    #[test]
    fn test_swarm_style_options_completeness() {
        assert_eq!(SWARM_STYLE_OPTIONS.len(), 5);
        let ids: Vec<&str> = SWARM_STYLE_OPTIONS.iter().map(|s| s.id).collect();
        assert!(ids.contains(&"stylish"));
        assert!(ids.contains(&"gitgraph"));
        assert!(ids.contains(&"modern"));
        assert!(ids.contains(&"minimal"));
        assert!(ids.contains(&"cockpit"));
    }

    #[test]
    fn test_toast_style_options_completeness() {
        assert_eq!(TOAST_STYLE_OPTIONS.len(), 9);
        let ids: Vec<&str> = TOAST_STYLE_OPTIONS.iter().map(|s| s.id).collect();
        assert!(ids.contains(&"opencode"));
        assert!(ids.contains(&"rounded"));
        assert!(ids.contains(&"rounded_padded"));
        assert!(ids.contains(&"bracket"));
        assert!(ids.contains(&"left_bar"));
        assert!(ids.contains(&"cradle_round"));
        assert!(ids.contains(&"cradle_square"));
        assert!(ids.contains(&"l_round"));
        assert!(ids.contains(&"l_square"));
    }

    #[test]
    fn test_render_theme_select_swarm_and_toast_styles_tabs() {
        let backend = TestBackend::new(110, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        let theme = Theme::default();
        let themes = Theme::list_themes();
        let animations = crate::ui::animation::ANIMATION_OPTIONS.to_vec();

        for tab in [
            crate::ui::modals::ThemeModalTab::SwarmStyles,
            crate::ui::modals::ThemeModalTab::ToastStyles,
        ] {
            let ctx = ThemeSelectContext {
                themes: &themes,
                animations: &animations,
                todo_styles: TODO_STYLE_OPTIONS,
                swarm_styles: SWARM_STYLE_OPTIONS,
                toast_styles: TOAST_STYLE_OPTIONS,
                active_tab: tab,
                theme_selected_index: 0,
                animation_selected_index: 0,
                todo_style_selected_index: 0,
                swarm_style_selected_index: 0,
                toast_style_selected_index: 0,
                active_theme_id: "auto",
                active_animation_id: "dual_pillars",
                active_todo_style: "tree",
                active_swarm_style: "stylish",
                active_toast_style: "opencode",
            };

            terminal
                .draw(|f| {
                    render_theme_select(f, f.area(), &theme, &ctx);
                })
                .unwrap();
        }
    }
}

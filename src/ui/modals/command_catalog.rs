//! Command palette & slash commands catalog modal rendering.

use crate::constants::{
    COMMAND_CATALOG_HEIGHT_PCT, COMMAND_CATALOG_WIDTH_PCT, COMMAND_NAME_DISPLAY_COLS,
};
use crate::ui::layout_utils::centered_rect;
use crate::ui::modals::common::compute_scroll_offset;
use crate::ui::theme::Theme;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;
use unicode_width::UnicodeWidthStr;

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
        name: "/provider",
        category: "Config & Runtime",
        shortcut: "",
        description: "Select active LLM provider (OpenRouter, Groq, Ollama, etc.)",
        example: "/provider",
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
        description: "Generate structured implementation plan without modifying files",
        example: "/plan add auth middleware",
    },
    CommandCatalogItem {
        name: "/explore",
        category: "Code & Inspection",
        shortcut: "Ctrl+E",
        description: "Interactive PageRank AST CodeGraph symbol explorer & blast radius",
        example: "/explore | /explore SymbolName",
    },
    CommandCatalogItem {
        name: "/review",
        category: "Code & Inspection",
        shortcut: "",
        description: "Comprehensive code quality, security & architectural review",
        example: "/review",
    },
    CommandCatalogItem {
        name: "/arch",
        category: "Code & Inspection",
        shortcut: "",
        description: "Lint layered software architecture boundaries, instability & circular cycles",
        example: "/arch",
    },
    CommandCatalogItem {
        name: "/subagent",
        category: "Workflows & Scaffolding",
        shortcut: "",
        description: "Spawn parallel specialized subagent worker swarm",
        example: "/subagent researcher \"find all memory leaks\"",
    },
    CommandCatalogItem {
        name: "/stream",
        category: "Config & Runtime",
        shortcut: "",
        description: "Toggle token-by-token live assistant streaming mode",
        example: "/stream on | /stream off",
    },
    CommandCatalogItem {
        name: "/model",
        category: "Config & Runtime",
        shortcut: "",
        description: "Switch active LLM model interactively from live provider list",
        example: "/model",
    },
    CommandCatalogItem {
        name: "/theme",
        category: "Display & Aesthetics",
        shortcut: "",
        description: "Switch TUI color theme palette interactively",
        example: "/theme | /theme catppuccin",
    },
    CommandCatalogItem {
        name: "/diff",
        category: "Code & Inspection",
        shortcut: "",
        description: "View git changes (unstaged working tree or --cached staged)",
        example: "/diff | /diff --cached",
    },
    CommandCatalogItem {
        name: "/undo",
        category: "History & Sessions",
        shortcut: "",
        description: "Revert all file modifications from previous turn",
        example: "/undo",
    },
    CommandCatalogItem {
        name: "/sessions",
        category: "History & Sessions",
        shortcut: "",
        description: "Browse & reload past workspace session history",
        example: "/sessions",
    },
    CommandCatalogItem {
        name: "/logs",
        category: "History & Sessions",
        shortcut: "",
        description: "Stream live server-style agent logs (tail -f, -n 100, session IDs)",
        example: "/logs | minicode logs -f",
    },
    CommandCatalogItem {
        name: "/copy",
        category: "Display & Aesthetics",
        shortcut: "",
        description: "Copy latest AI response or entire chat to system clipboard",
        example: "/copy | /copy all",
    },
    CommandCatalogItem {
        name: "/clear",
        category: "Display & Aesthetics",
        shortcut: "",
        description: "Clear conversation timeline display",
        example: "/clear",
    },
    CommandCatalogItem {
        name: "/help",
        category: "Discovery & Help",
        shortcut: "F1",
        description: "Display help cheatsheet & keyboard shortcuts modal",
        example: "/help",
    },
    CommandCatalogItem {
        name: "/terminal",
        category: "Config & Runtime",
        shortcut: "Ctrl+T",
        description: "Toggle embedded persistent PTY shell terminal drawer",
        example: "/terminal",
    },
    CommandCatalogItem {
        name: "/exit",
        category: "Discovery & Help",
        shortcut: "",
        description: "Quit minicode interactive session cleanly",
        example: "/exit",
    },
];

pub fn render_command_catalog(
    frame: &mut Frame,
    area: Rect,
    theme: &Theme,
    filtered_indices: &[usize],
    selected_index: usize,
    filter: &str,
) {
    let popup_area = centered_rect(COMMAND_CATALOG_WIDTH_PCT, COMMAND_CATALOG_HEIGHT_PCT, area);
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
        .constraints([
            Constraint::Length(crate::constants::MODAL_SEARCH_INPUT_HEIGHT),
            Constraint::Min(8),
        ])
        .split(inner_area);

    // Search Bar
    let count_badge = format!(
        " [{}/{}] ",
        if filtered_indices.is_empty() {
            0
        } else {
            selected_index + 1
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
                width =
                    (v_chunks[0].width as usize).saturating_sub(UnicodeWidthStr::width(filter) + 8)
            ),
            Style::default().fg(theme.muted),
        ),
    ]))
    .block(search_block);
    frame.render_widget(search_p, v_chunks[0]);

    // Split into [Left List (42%), Right Details (58%)]
    let h_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(crate::constants::MODAL_SPLIT_PRIMARY_PERCENT),
            Constraint::Percentage(crate::constants::MODAL_SPLIT_SECONDARY_PERCENT),
        ])
        .split(v_chunks[1]);

    let list_block = Block::default()
        .title(" Commands ")
        .borders(Borders::RIGHT)
        .border_style(Style::default().fg(theme.bg_elevated));

    let mut list_lines = Vec::new();
    let max_visible = (h_chunks[0].height as usize).saturating_sub(2).max(1);
    let scroll_offset = compute_scroll_offset(selected_index, max_visible);

    for (idx_rel, &item_idx) in filtered_indices
        .iter()
        .skip(scroll_offset)
        .take(max_visible)
        .enumerate()
    {
        let real_idx = scroll_offset + idx_rel;
        let item = &COMMAND_CATALOG_ITEMS[item_idx];
        let is_selected = real_idx == selected_index;

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
    if let Some(&selected_item_idx) = filtered_indices.get(selected_index) {
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

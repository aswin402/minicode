//! In-TUI MiniVault Agent Skills Warehouse & Lifecycle Modal.

use crate::ui::layout_utils::centered_rect;
use crate::ui::modals::common::compute_scroll_offset;
use crate::ui::theme::Theme;
use crate::vault::models::{SkillScope, VaultSkill};
use crate::vault::store::VaultStore;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;
use std::path::{Path, PathBuf};

/// Active tab within the `/vault` MiniVault modal dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VaultTab {
    All,
    Project,
    Global,
    Builtin,
}

impl VaultTab {
    #[allow(dead_code)]
    pub fn all() -> &'static [VaultTab] {
        &[
            VaultTab::All,
            VaultTab::Project,
            VaultTab::Global,
            VaultTab::Builtin,
        ]
    }

    #[allow(dead_code)]
    pub fn title(&self) -> &'static str {
        match self {
            VaultTab::All => "All Skills",
            VaultTab::Project => "Project Active",
            VaultTab::Global => "Global User",
            VaultTab::Builtin => "Built-in Core",
        }
    }

    pub fn next(&self) -> Self {
        match self {
            VaultTab::All => VaultTab::Project,
            VaultTab::Project => VaultTab::Global,
            VaultTab::Global => VaultTab::Builtin,
            VaultTab::Builtin => VaultTab::All,
        }
    }

    pub fn prev(&self) -> Self {
        match self {
            VaultTab::All => VaultTab::Builtin,
            VaultTab::Project => VaultTab::All,
            VaultTab::Global => VaultTab::Project,
            VaultTab::Builtin => VaultTab::Global,
        }
    }
}

/// State for the interactive `/vault` modal dialog.
#[derive(Debug, Clone)]
pub struct VaultModalState {
    pub workspace_root: PathBuf,
    pub active_tab: VaultTab,
    pub search_query: String,
    pub selected_index: usize,
    pub preview_scroll_offset: usize,
    pub filtered_skills: Vec<VaultSkill>,
    pub status_message: Option<String>,
}

impl VaultModalState {
    pub fn new(workspace: &Path) -> Self {
        let mut state = Self {
            workspace_root: workspace.to_path_buf(),
            active_tab: VaultTab::All,
            search_query: String::new(),
            selected_index: 0,
            preview_scroll_offset: 0,
            filtered_skills: Vec::new(),
            status_message: None,
        };
        state.refresh_filtered();
        state
    }

    pub fn current_tab_len(&self) -> usize {
        self.filtered_skills.len()
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

    #[allow(dead_code)]
    pub fn clear_search(&mut self) {
        self.search_query.clear();
        self.selected_index = 0;
        self.preview_scroll_offset = 0;
        self.refresh_filtered();
    }

    /// Toggles loading/unloading of the currently selected skill into the active project.
    pub fn toggle_project_load(&mut self) {
        let selected = match self.filtered_skills.get(self.selected_index) {
            Some(s) => s.clone(),
            None => return,
        };

        let store = VaultStore::new(&self.workspace_root);

        if selected.is_active_in_project {
            match store.unload_from_project(&selected.name) {
                Ok(_) => {
                    self.status_message = Some(format!(
                        "✔ Unloaded skill `{}` from project workspace.",
                        selected.name
                    ));
                }
                Err(e) => {
                    self.status_message = Some(format!("❌ Failed to unload: {}", e));
                }
            }
        } else {
            match store.load_to_project(&selected.name) {
                Ok(_) => {
                    self.status_message = Some(format!(
                        "✔ Successfully loaded skill `{}` into active project!",
                        selected.name
                    ));
                }
                Err(e) => {
                    self.status_message = Some(format!("❌ Failed to load skill: {}", e));
                }
            }
        }

        self.refresh_filtered();
    }

    /// Deletes the currently selected custom skill from project or global vault.
    pub fn delete_selected(&mut self) {
        let selected = match self.filtered_skills.get(self.selected_index) {
            Some(s) => s.clone(),
            None => return,
        };

        if selected.scope == SkillScope::Builtin {
            self.status_message =
                Some("ℹ Built-in skills are immutable core standards.".to_string());
            return;
        }

        let store = VaultStore::new(&self.workspace_root);
        match store.delete_skill(selected.scope, &selected.name) {
            Ok(_) => {
                self.status_message = Some(format!(
                    "✔ Deleted skill `{}` from {:?} vault.",
                    selected.name, selected.scope
                ));
                self.refresh_filtered();
            }
            Err(e) => {
                self.status_message = Some(format!("❌ Delete failed: {}", e));
            }
        }
    }

    /// Refreshes the filtered list according to active tab and search query.
    pub fn refresh_filtered(&mut self) {
        let store = VaultStore::new(&self.workspace_root);
        let mut list = if self.search_query.trim().is_empty() {
            store.list_all_skills()
        } else {
            store.search_skills(&self.search_query)
        };

        match self.active_tab {
            VaultTab::All => {}
            VaultTab::Project => {
                list.retain(|s| s.is_active_in_project);
            }
            VaultTab::Global => {
                list.retain(|s| s.scope == SkillScope::Global);
            }
            VaultTab::Builtin => {
                list.retain(|s| s.scope == SkillScope::Builtin);
            }
        }

        self.filtered_skills = list;
        if self.selected_index >= self.filtered_skills.len() {
            self.selected_index = self.filtered_skills.len().saturating_sub(1);
        }
    }
}

/// Renders the `/vault` MiniVault interactive modal dialog.
pub fn render_vault_modal(frame: &mut Frame, state: &VaultModalState, theme: &Theme) {
    let area = centered_rect(92, 86, frame.area());
    frame.render_widget(Clear, area);

    // Compute tab counts
    let store = VaultStore::new(&state.workspace_root);
    let all = store.list_all_skills();
    let total_count = all.len();
    let project_count = all.iter().filter(|s| s.is_active_in_project).count();
    let global_count = all.iter().filter(|s| s.scope == SkillScope::Global).count();
    let builtin_count = all
        .iter()
        .filter(|s| s.scope == SkillScope::Builtin)
        .count();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header & Tabs
            Constraint::Length(3), // Search bar
            Constraint::Min(10),   // Main content: List + Preview
            Constraint::Length(3), // Footer & hotkeys
        ])
        .split(area);

    // 1. Header & Tabs
    let tabs = [
        (VaultTab::All, format!("1. All ({})", total_count)),
        (
            VaultTab::Project,
            format!("2. Project Active ({})", project_count),
        ),
        (VaultTab::Global, format!("3. Global ({})", global_count)),
        (
            VaultTab::Builtin,
            format!("4. Built-in ({})", builtin_count),
        ),
    ];

    let mut tab_spans = Vec::new();
    for (tab, title) in tabs {
        let is_active = state.active_tab == tab;
        let style = if is_active {
            Style::default()
                .fg(theme.brand_accent)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
        } else {
            Style::default().fg(theme.muted)
        };
        tab_spans.push(Span::styled(format!(" [{}] ", title), style));
        tab_spans.push(Span::styled(" ", Style::default()));
    }

    let header_widget = Paragraph::new(Line::from(tab_spans))
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.border))
                .title(Span::styled(
                    " 📦 MiniVault Agent Skills Warehouse ",
                    Style::default()
                        .fg(theme.brand_accent)
                        .add_modifier(Modifier::BOLD),
                )),
        );
    frame.render_widget(header_widget, chunks[0]);

    // 2. Search Bar
    let search_text = if state.search_query.is_empty() {
        Span::styled(
            "Type to search skills (name, triggers, description)...",
            Style::default()
                .fg(theme.muted)
                .add_modifier(Modifier::ITALIC),
        )
    } else {
        Span::styled(&state.search_query, Style::default().fg(theme.text_primary))
    };

    let search_widget = Paragraph::new(Line::from(vec![
        Span::styled(
            " Search: ",
            Style::default()
                .fg(theme.brand_accent)
                .add_modifier(Modifier::BOLD),
        ),
        search_text,
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.border)),
    );
    frame.render_widget(search_widget, chunks[1]);

    // 3. Main Body Split: List (40%) and Preview (60%)
    let body_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(42), Constraint::Percentage(58)])
        .split(chunks[2]);

    // 3A. Skills List
    render_skills_list(frame, body_chunks[0], state, theme);

    // 3B. Skill Preview Pane
    render_skill_preview(frame, body_chunks[1], state, theme);

    // 4. Footer & Hotkeys
    let status_text = if let Some(msg) = &state.status_message {
        Span::styled(
            msg,
            Style::default()
                .fg(theme.success)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        Span::styled(
            "[Space/Enter] Toggle Project Load   [d] Delete Custom   [Tab] Switch Tab   [Esc] Close",
            Style::default().fg(theme.muted),
        )
    };

    let footer_widget = Paragraph::new(Line::from(status_text))
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.border)),
        );
    frame.render_widget(footer_widget, chunks[3]);
}

fn render_skills_list(frame: &mut Frame, area: Rect, state: &VaultModalState, theme: &Theme) {
    let items_area_height = area.height.saturating_sub(2) as usize;
    let scroll_offset = compute_scroll_offset(state.selected_index, items_area_height);

    let mut lines = Vec::new();

    if state.filtered_skills.is_empty() {
        lines.push(Line::from(Span::styled(
            " No skills found matching filter.",
            Style::default()
                .fg(theme.muted)
                .add_modifier(Modifier::ITALIC),
        )));
    } else {
        let visible_items = state
            .filtered_skills
            .iter()
            .enumerate()
            .skip(scroll_offset)
            .take(items_area_height);

        for (idx, skill) in visible_items {
            let is_selected = idx == state.selected_index;

            let pointer = if is_selected { "▶ " } else { "  " };

            let active_badge = if skill.is_active_in_project {
                Span::styled(
                    "[✔] ",
                    Style::default()
                        .fg(theme.success)
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                Span::styled("[ ] ", Style::default().fg(theme.muted))
            };

            let scope_badge = match skill.scope {
                SkillScope::Project => Span::styled(
                    "[Proj] ",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                SkillScope::Global => Span::styled("[User] ", Style::default().fg(Color::Yellow)),
                SkillScope::Builtin => {
                    Span::styled("[Core] ", Style::default().fg(Color::DarkGray))
                }
            };

            let name_style = if is_selected {
                Style::default()
                    .fg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.text_primary)
            };

            lines.push(Line::from(vec![
                Span::styled(pointer, Style::default().fg(theme.brand_accent)),
                active_badge,
                scope_badge,
                Span::styled(&skill.name, name_style),
            ]));
        }
    }

    let list_widget = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.border))
            .title(Span::styled(
                format!(" Skills ({}) ", state.filtered_skills.len()),
                Style::default().fg(theme.muted),
            )),
    );
    frame.render_widget(list_widget, area);
}

fn render_skill_preview(frame: &mut Frame, area: Rect, state: &VaultModalState, theme: &Theme) {
    let mut lines = Vec::new();

    if let Some(skill) = state.filtered_skills.get(state.selected_index) {
        let status_str = if skill.is_active_in_project {
            "Active in Project (Loaded)"
        } else {
            "Available in Vault (Not Active)"
        };
        let status_color = if skill.is_active_in_project {
            theme.success
        } else {
            theme.muted
        };

        lines.push(Line::from(vec![
            Span::styled(
                "Name: ",
                Style::default()
                    .fg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                &skill.name,
                Style::default()
                    .fg(theme.text_primary)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("  •  Scope: ", Style::default().fg(theme.muted)),
            Span::styled(skill.scope.as_str(), Style::default().fg(Color::Cyan)),
        ]));

        lines.push(Line::from(vec![
            Span::styled(
                "Status: ",
                Style::default()
                    .fg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                status_str,
                Style::default()
                    .fg(status_color)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));

        lines.push(Line::from(vec![
            Span::styled("Description: ", Style::default().fg(theme.brand_accent)),
            Span::styled(&skill.description, Style::default().fg(theme.text_primary)),
        ]));

        if !skill.frontmatter.triggers.is_empty() {
            lines.push(Line::from(vec![
                Span::styled("Triggers: ", Style::default().fg(theme.muted)),
                Span::styled(
                    skill.frontmatter.triggers.join(", "),
                    Style::default().fg(Color::Yellow),
                ),
            ]));
        }

        if !skill.frontmatter.globs.is_empty() {
            lines.push(Line::from(vec![
                Span::styled("Globs: ", Style::default().fg(theme.muted)),
                Span::styled(
                    skill.frontmatter.globs.join(", "),
                    Style::default().fg(Color::LightBlue),
                ),
            ]));
        }

        if let Some(path) = &skill.path {
            lines.push(Line::from(vec![
                Span::styled("Path: ", Style::default().fg(theme.muted)),
                Span::styled(path.display().to_string(), Style::default().fg(theme.muted)),
            ]));
        }

        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "────────────────── Instructions & Guidelines ──────────────────",
            Style::default().fg(theme.border),
        )));
        lines.push(Line::from(""));

        for raw_line in skill.instructions.lines() {
            let styled_line = if raw_line.starts_with("# ") {
                Span::styled(
                    raw_line,
                    Style::default()
                        .fg(theme.brand_accent)
                        .add_modifier(Modifier::BOLD),
                )
            } else if raw_line.starts_with("## ") {
                Span::styled(
                    raw_line,
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                )
            } else if raw_line.starts_with("### ") {
                Span::styled(raw_line, Style::default().fg(Color::Yellow))
            } else if raw_line.starts_with('-') || raw_line.starts_with('*') {
                Span::styled(raw_line, Style::default().fg(theme.text_primary))
            } else {
                Span::styled(raw_line, Style::default().fg(theme.muted))
            };
            lines.push(Line::from(styled_line));
        }
    } else {
        lines.push(Line::from(Span::styled(
            "Select a skill to inspect its full guidelines and rules.",
            Style::default()
                .fg(theme.muted)
                .add_modifier(Modifier::ITALIC),
        )));
    }

    let preview_area_height = area.height.saturating_sub(2) as usize;
    let max_scroll = lines.len().saturating_sub(preview_area_height);
    let clamped_offset = state.preview_scroll_offset.min(max_scroll);

    let visible_lines: Vec<Line> = lines.into_iter().skip(clamped_offset).collect();

    let preview_widget = Paragraph::new(visible_lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.border))
            .title(Span::styled(
                " Skill Preview ",
                Style::default().fg(theme.brand_accent),
            )),
    );
    frame.render_widget(preview_widget, area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_vault_modal_state_navigation() {
        let dir = tempdir().unwrap();
        let ws = dir.path();

        let mut state = VaultModalState::new(ws);
        assert_eq!(state.active_tab, VaultTab::All);
        assert!(!state.filtered_skills.is_empty());

        // Tab cycling
        state.next_tab();
        assert_eq!(state.active_tab, VaultTab::Project);
        state.next_tab();
        assert_eq!(state.active_tab, VaultTab::Global);
        state.next_tab();
        assert_eq!(state.active_tab, VaultTab::Builtin);
        state.next_tab();
        assert_eq!(state.active_tab, VaultTab::All);

        // Search filtering
        state.handle_char('t');
        state.handle_char('a');
        state.handle_char('i');
        state.handle_char('l');
        assert_eq!(state.search_query, "tail");
        assert!(state
            .filtered_skills
            .iter()
            .any(|s| s.name.contains("tail")));

        // Toggle load into project
        state.toggle_project_load();
        assert!(state.status_message.is_some());
    }
}

use crate::constants::{
    COMMAND_PALETTE_HEIGHT, COMMAND_PALETTE_MAX_WIDTH, COMMAND_PALETTE_MIN_WIDTH,
    COMMAND_PALETTE_WIDTH_PCT, DEFAULT_INPUT_PLACEHOLDER,
};
use crate::ui::theme::Theme;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;
use tui_textarea::TextArea;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// Categories for organizing palette commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandCategory {
    All,
    System,
    Intelligence,
    Tools,
}

impl CommandCategory {
    pub fn all() -> &'static [CommandCategory] {
        &[
            CommandCategory::All,
            CommandCategory::System,
            CommandCategory::Intelligence,
            CommandCategory::Tools,
        ]
    }

    pub fn label(&self) -> &'static str {
        match self {
            CommandCategory::All => "All",
            CommandCategory::System => "System",
            CommandCategory::Intelligence => "Intelligence",
            CommandCategory::Tools => "Tools",
        }
    }
}

/// A command item displayed in the floating spotlight palette.
#[derive(Debug, Clone)]
pub struct PaletteCommand {
    pub slash_name: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub category: CommandCategory,
    pub shortcut: Option<&'static str>,
}

pub const PALETTE_COMMANDS: &[PaletteCommand] = &[
    PaletteCommand {
        slash_name: "/commands",
        title: "Command Catalog",
        description: "Interactive catalog of all slash commands & shortcuts",
        category: CommandCategory::System,
        shortcut: None,
    },
    PaletteCommand {
        slash_name: "/new",
        title: "New Session",
        description: "Start fresh session & reset conversation",
        category: CommandCategory::System,
        shortcut: Some("ctrl+n"),
    },
    PaletteCommand {
        slash_name: "/model",
        title: "Switch Model",
        description: "Choose AI model or provider",
        category: CommandCategory::System,
        shortcut: Some("ctrl+l"),
    },
    PaletteCommand {
        slash_name: "/thinking",
        title: "Extended Thinking",
        description: "Configure reasoning budget for Claude 3.7 / o1 / o3 / DeepSeek R1",
        category: CommandCategory::System,
        shortcut: None,
    },
    PaletteCommand {
        slash_name: "/configure",
        title: "Configure Providers",
        description: "Interactive API key & endpoint manager",
        category: CommandCategory::System,
        shortcut: Some("F2"),
    },
    PaletteCommand {
        slash_name: "/keys",
        title: "Manage API Keys",
        description: "Configure or update provider API keys",
        category: CommandCategory::System,
        shortcut: None,
    },
    PaletteCommand {
        slash_name: "/provider",
        title: "Switch Provider",
        description: "Select active LLM provider (OpenRouter, Groq, Ollama, etc.)",
        category: CommandCategory::System,
        shortcut: None,
    },
    PaletteCommand {
        slash_name: "/theme",
        title: "Theme Selector",
        description: "Switch TUI color theme palette",
        category: CommandCategory::System,
        shortcut: None,
    },
    PaletteCommand {
        slash_name: "/sessions",
        title: "Session History",
        description: "Browse & reload past workspace sessions",
        category: CommandCategory::System,
        shortcut: Some("ctrl+h"),
    },
    PaletteCommand {
        slash_name: "/tokens",
        title: "Token Breakdown",
        description: "Display detailed token usage & context stats",
        category: CommandCategory::System,
        shortcut: None,
    },
    PaletteCommand {
        slash_name: "/streaming",
        title: "Streaming Mode",
        description: "Enable or disable LLM response streaming",
        category: CommandCategory::System,
        shortcut: None,
    },
    PaletteCommand {
        slash_name: "/parallel",
        title: "Parallel Tools",
        description: "Configure parallel & speculative read-only tool execution",
        category: CommandCategory::System,
        shortcut: None,
    },
    PaletteCommand {
        slash_name: "/tx",
        title: "Workspace Transaction",
        description: "Inspect, commit, or rollback active multi-file transaction",
        category: CommandCategory::System,
        shortcut: None,
    },
    PaletteCommand {
        slash_name: "/dag",
        title: "Execution DAG",
        description: "View DAG pipeline execution syntax and active workflow templates",
        category: CommandCategory::System,
        shortcut: None,
    },
    PaletteCommand {
        slash_name: "/heal",
        title: "Self-Healing Diagnostics",
        description: "Triage compiler diagnostics and run autonomous error repair pass",
        category: CommandCategory::System,
        shortcut: None,
    },
    PaletteCommand {
        slash_name: "/sandbox",
        title: "Execute Sandboxed Command",
        description: "Run command in isolated sandbox (network/fs/overlay protection)",
        category: CommandCategory::System,
        shortcut: None,
    },
    PaletteCommand {
        slash_name: "/retrieve",
        title: "Multi-Modal Knowledge Retrieval",
        description: "Search across CodeGraph, semantic vectors, wiki, and memory",
        category: CommandCategory::System,
        shortcut: None,
    },
    PaletteCommand {
        slash_name: "/route",
        title: "Adaptive Model Router",
        description: "Adaptive tier routing (Fast/Standard/Deep) and provider health",
        category: CommandCategory::System,
        shortcut: None,
    },
    PaletteCommand {
        slash_name: "/quarantine",
        title: "Flaky Test Quarantine",
        description: "Quarantine intermittent tests and analyze statistical failure variance",
        category: CommandCategory::Tools,
        shortcut: None,
    },
    PaletteCommand {
        slash_name: "/commit",
        title: "Synthesize Semantic Commits",
        description: "Synthesize atomic Conventional Commits and release changelog from git diff",
        category: CommandCategory::Tools,
        shortcut: None,
    },
    PaletteCommand {
        slash_name: "/clear",
        title: "Clear Timeline",
        description: "Clear active conversation timeline messages",
        category: CommandCategory::System,
        shortcut: None,
    },
    PaletteCommand {
        slash_name: "/help",
        title: "Help & Shortcuts",
        description: "Interactive keyboard shortcuts cheatsheet",
        category: CommandCategory::System,
        shortcut: Some("F1"),
    },
    PaletteCommand {
        slash_name: "/exit",
        title: "Exit minicode",
        description: "Quit minicode interactive session cleanly",
        category: CommandCategory::System,
        shortcut: Some("ctrl+c"),
    },
    // Intelligence / Code Graph Commands
    PaletteCommand {
        slash_name: "/index",
        title: "Repository Index",
        description: "Scan AST symbols & build PageRank code graph",
        category: CommandCategory::Intelligence,
        shortcut: Some("F5"),
    },
    PaletteCommand {
        slash_name: "/review",
        title: "Code Review",
        description: "Run git diff impact analysis & security audit",
        category: CommandCategory::Intelligence,
        shortcut: Some("ctrl+r"),
    },
    PaletteCommand {
        slash_name: "/explore",
        title: "Code Explorer",
        description: "Surgically explore AST symbols, call graph & blast radius",
        category: CommandCategory::Intelligence,
        shortcut: Some("ctrl+e"),
    },
    PaletteCommand {
        slash_name: "/map",
        title: "Codebase Map",
        description: "Render AST PageRank repository hierarchy & dependency graph",
        category: CommandCategory::Intelligence,
        shortcut: None,
    },
    PaletteCommand {
        slash_name: "/arch",
        title: "Architecture Governor",
        description: "Lint layered boundaries, instability metrics & circular cycles",
        category: CommandCategory::Intelligence,
        shortcut: None,
    },
    PaletteCommand {
        slash_name: "/compact",
        title: "Compact Context",
        description: "Manually compact conversation context tokens",
        category: CommandCategory::Intelligence,
        shortcut: None,
    },
    PaletteCommand {
        slash_name: "/plan",
        title: "Plan Feature",
        description: "Break complex task into verifiable milestones",
        category: CommandCategory::Intelligence,
        shortcut: None,
    },
    PaletteCommand {
        slash_name: "/goal",
        title: "Autonomous Goal",
        description: "Run self-directed loop until complete",
        category: CommandCategory::Intelligence,
        shortcut: None,
    },
    // Tools Commands
    PaletteCommand {
        slash_name: "/diff",
        title: "Git Diff Viewer",
        description: "Interactive split/unified git diff viewer & staging",
        category: CommandCategory::Tools,
        shortcut: Some("ctrl+d"),
    },
    PaletteCommand {
        slash_name: "/terminal",
        title: "Toggle Terminal",
        description: "Toggle embedded interactive terminal drawer",
        category: CommandCategory::Tools,
        shortcut: Some("ctrl+t"),
    },
    PaletteCommand {
        slash_name: "/swarm",
        title: "Subagent Swarm Drawer",
        description: "Toggle interactive subagent swarm activity telemetry drawer",
        category: CommandCategory::Intelligence,
        shortcut: Some("ctrl+s"),
    },
    PaletteCommand {
        slash_name: "/stack",
        title: "Scaffold Stack",
        description: "Interactive onpkg multi-runtime stack wizard",
        category: CommandCategory::Tools,
        shortcut: None,
    },
    PaletteCommand {
        slash_name: "/undo",
        title: "Undo Changes",
        description: "Revert file modifications from previous turn",
        category: CommandCategory::Tools,
        shortcut: Some("ctrl+u"),
    },
    PaletteCommand {
        slash_name: "/retry",
        title: "Retry Prompt",
        description: "Re-submit previous prompt to agent",
        category: CommandCategory::Tools,
        shortcut: None,
    },
    PaletteCommand {
        slash_name: "/export",
        title: "Export Session",
        description: "Export conversation trajectory to markdown",
        category: CommandCategory::Tools,
        shortcut: None,
    },
    PaletteCommand {
        slash_name: "/save",
        title: "Save Session Transcript",
        description: "Export full conversation history transcript to a target file",
        category: CommandCategory::Tools,
        shortcut: None,
    },
    PaletteCommand {
        slash_name: "/load",
        title: "Load Session",
        description: "Load and replay past session events into active timeline",
        category: CommandCategory::Tools,
        shortcut: None,
    },
    PaletteCommand {
        slash_name: "/copy",
        title: "Copy Transcript",
        description: "Copy latest response or whole transcript to clipboard",
        category: CommandCategory::Tools,
        shortcut: None,
    },
];

/// A collapsed block representing a multiline paste (> 5 lines).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PastedBlock {
    pub id: usize,
    pub placeholder: String,
    pub full_text: String,
    pub line_count: usize,
}

/// Prompt submission containing both the visual display version (with placeholders)
/// and the full version (with placeholders expanded back to the original text).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptSubmission {
    pub display: String,
    pub full: String,
}

pub struct InputDock<'a> {
    pub textarea: TextArea<'a>,
    pub slash_selected_index: usize,
    pub category_index: usize,
    pub last_width: std::cell::Cell<u16>,
    pub pasted_blocks: Vec<PastedBlock>,
    pub next_block_id: usize,
}

impl<'a> Default for InputDock<'a> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> InputDock<'a> {
    pub fn new() -> Self {
        let mut textarea = TextArea::default();
        textarea.set_placeholder_text(DEFAULT_INPUT_PLACEHOLDER);
        textarea.set_cursor_line_style(Style::default());
        Self {
            textarea,
            slash_selected_index: 0,
            category_index: 0,
            last_width: std::cell::Cell::new(80),
            pasted_blocks: Vec::new(),
            next_block_id: 1,
        }
    }

    /// Returns the required dynamic height for the input dock (border top + lines + border bottom)
    pub fn required_height(&self) -> u16 {
        let num_lines = self.textarea.lines().len().clamp(1, 5);
        (num_lines as u16) + 2
    }

    /// Automatically wraps lines that exceed the visible input dock width
    pub fn auto_wrap(&mut self) {
        if self.has_active_slash_query() {
            return;
        }

        let width = self.last_width.get();
        // Leave 2 cols for cursor buffer and safety
        let max_width = (width.saturating_sub(2) as usize).max(20);

        let (mut cursor_row, mut cursor_col) = self.textarea.cursor();
        let mut lines = self.textarea.lines().to_vec();
        if lines.is_empty() {
            return;
        }

        let mut changed = false;
        let mut r = 0;
        while r < lines.len() {
            let line_width = UnicodeWidthStr::width(lines[r].as_str());
            if line_width > max_width {
                let current = &lines[r];

                // Find the last space at or before max_width chars, ignoring spaces inside bracketed blocks '[...]'
                let mut current_w = 0;
                let mut last_space_char_idx = None;
                let mut in_bracket = false;

                for (char_idx, ch) in current.chars().enumerate() {
                    if ch == '[' {
                        in_bracket = true;
                    } else if ch == ']' {
                        in_bracket = false;
                    }

                    current_w += UnicodeWidthChar::width(ch).unwrap_or(1);
                    if ch == ' ' && !in_bracket && current_w <= max_width {
                        last_space_char_idx = Some(char_idx);
                    }
                    if current_w > max_width {
                        break;
                    }
                }

                let char_break = match last_space_char_idx {
                    Some(idx) => idx,
                    None => {
                        // If no space outside brackets, check if a bracket started before max_width to break before it
                        let mut bracket_start_idx = None;
                        for (char_idx, ch) in current.chars().enumerate() {
                            if ch == '[' && char_idx > 0 {
                                bracket_start_idx = Some(char_idx);
                            }
                        }
                        if let Some(b_idx) = bracket_start_idx {
                            b_idx
                        } else {
                            // Hard break at max_width
                            let mut w = 0;
                            let mut hard_idx = 0;
                            for (char_idx, ch) in current.chars().enumerate() {
                                let cw = UnicodeWidthChar::width(ch).unwrap_or(1);
                                if w + cw > max_width {
                                    break;
                                }
                                w += cw;
                                hard_idx = char_idx + 1;
                            }
                            hard_idx.max(1)
                        }
                    }
                };

                let char_vec: Vec<char> = current.chars().collect();
                let first_part: String = char_vec[..char_break].iter().collect();
                let rest_start = if char_break < char_vec.len() && char_vec[char_break] == ' ' {
                    char_break + 1
                } else {
                    char_break
                };
                let second_part: String = char_vec[rest_start..].iter().collect();

                lines[r] = first_part;
                if r + 1 < lines.len() {
                    lines[r + 1] = if second_part.is_empty() {
                        lines[r + 1].clone()
                    } else {
                        format!("{} {}", second_part, lines[r + 1])
                    };
                } else {
                    lines.push(second_part);
                }

                // Adjust cursor if cursor was on this row
                if cursor_row == r {
                    if cursor_col > char_break {
                        cursor_row = r + 1;
                        cursor_col = cursor_col.saturating_sub(rest_start);
                    }
                } else if cursor_row > r {
                    cursor_row += 1;
                }

                changed = true;
            }
            r += 1;
        }

        if changed {
            let mut new_ta = TextArea::new(lines);
            new_ta.set_placeholder_text(DEFAULT_INPUT_PLACEHOLDER);
            new_ta.set_cursor_line_style(Style::default());
            new_ta.move_cursor(tui_textarea::CursorMove::Jump(
                cursor_row as u16,
                cursor_col as u16,
            ));
            self.textarea = new_ta;
        }
    }

    /// Resets the input dock textarea to an empty prompt state
    pub fn reset(&mut self) {
        let mut ta = TextArea::default();
        ta.set_placeholder_text(DEFAULT_INPUT_PLACEHOLDER);
        ta.set_cursor_line_style(Style::default());
        self.textarea = ta;
        self.slash_selected_index = 0;
        self.pasted_blocks.clear();
        self.next_block_id = 1;
    }

    /// Returns the full text in the input dock (joining lines with newline)
    pub fn get_text(&self) -> String {
        self.textarea.lines().join("\n")
    }

    /// Checks whether the input dock has no user text typed into it
    pub fn is_empty(&self) -> bool {
        self.textarea.lines().iter().all(|l| l.trim().is_empty())
    }

    /// Handles bracketed paste events.
    /// - If <= 5 lines: pastes full content directly into the textarea (dock height adjusts dynamically up to 5 lines).
    /// - If > 5 lines: collapses into a clean preview placeholder: `[<preview words> ..... +<line_count> lines]`,
    ///   storing the raw full text in `pasted_blocks` to be expanded on submission.
    pub fn handle_paste(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }

        let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
        let line_count = normalized.lines().count();

        if line_count <= 5 {
            // Paste directly into the textarea
            let parts: Vec<&str> = normalized.split('\n').collect();
            for (i, part) in parts.iter().enumerate() {
                if i > 0 {
                    self.textarea.insert_newline();
                }
                self.textarea.insert_str(part);
            }
            self.auto_wrap();
        } else {
            // Collapse into a preview placeholder
            let first_non_empty = normalized
                .lines()
                .find(|line| !line.trim().is_empty())
                .unwrap_or("snippet");
            let trimmed = first_non_empty.trim();
            let preview = if trimmed.chars().count() > 24 {
                let s: String = trimmed.chars().take(24).collect();
                format!("{}…", s.trim_end())
            } else {
                trimmed.to_string()
            };

            let block_id = self.next_block_id;
            self.next_block_id += 1;

            let placeholder = format!("[{} ..... +{} lines]", preview, line_count);

            self.pasted_blocks.push(PastedBlock {
                id: block_id,
                placeholder: placeholder.clone(),
                full_text: normalized,
                line_count,
            });

            self.textarea.insert_str(&placeholder);
            self.textarea.insert_str(" ");
            self.auto_wrap();
        }
    }

    /// Resolves the typed text against any collapsed paste blocks.
    /// Returns `PromptSubmission { display, full }`.
    pub fn resolve_submission(&self, text: &str) -> PromptSubmission {
        let display = text.trim().to_string();
        let mut full = display.clone();

        for block in &self.pasted_blocks {
            if let Some(pos) = full.find(&block.placeholder) {
                full.replace_range(pos..pos + block.placeholder.len(), &block.full_text);
            }
        }

        PromptSubmission { display, full }
    }

    /// Checks if the input starts with '/' and is actively triggering the command palette
    pub fn has_active_slash_query(&self) -> bool {
        let lines = self.textarea.lines();
        if let Some(first_line) = lines.first() {
            let trimmed = first_line.trim();
            trimmed.starts_with('/') && !trimmed.contains(' ')
        } else {
            false
        }
    }

    /// Returns matching slash command candidates filtered by search query and active category
    pub fn matching_palette_commands(&self) -> Vec<&'static PaletteCommand> {
        let lines = self.textarea.lines();
        let query = if let Some(first_line) = lines.first() {
            let trimmed = first_line.trim();
            if let Some(stripped) = trimmed.strip_prefix('/') {
                stripped.to_lowercase()
            } else {
                return Vec::new();
            }
        } else {
            return Vec::new();
        };

        let selected_category =
            CommandCategory::all()[self.category_index % CommandCategory::all().len()];

        PALETTE_COMMANDS
            .iter()
            .filter(|cmd| {
                // Category filter
                let matches_category = match selected_category {
                    CommandCategory::All => true,
                    cat => cmd.category == cat,
                };

                if !matches_category {
                    return false;
                }

                // Query filter
                if query.is_empty() {
                    return true;
                }

                cmd.title.to_lowercase().contains(&query)
                    || cmd
                        .slash_name
                        .trim_start_matches('/')
                        .to_lowercase()
                        .contains(&query)
                    || cmd.description.to_lowercase().contains(&query)
                    || cmd
                        .shortcut
                        .is_some_and(|s| s.to_lowercase().contains(&query))
            })
            .collect()
    }

    /// Returns the currently selected palette command candidate
    pub fn selected_palette_command(&self) -> Option<&'static PaletteCommand> {
        let matches = self.matching_palette_commands();
        if matches.is_empty() {
            None
        } else {
            let idx = self
                .slash_selected_index
                .min(matches.len().saturating_sub(1));
            Some(matches[idx])
        }
    }

    /// Cycles the active category (Tab / BackTab)
    pub fn cycle_category(&mut self, forward: bool) {
        let total = CommandCategory::all().len();
        if total == 0 {
            return;
        }
        if forward {
            self.category_index = (self.category_index + 1) % total;
        } else if self.category_index == 0 {
            self.category_index = total.saturating_sub(1);
        } else {
            self.category_index = self.category_index.saturating_sub(1);
        }
        self.slash_selected_index = 0;
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Option<PromptSubmission> {
        // Only process KeyPress / KeyRepeat events (ignore KeyRelease)
        if key.kind == KeyEventKind::Release {
            return None;
        }

        let is_slash_open = self.has_active_slash_query();
        let matching = self.matching_palette_commands();

        // Handle Escape to dismiss command palette immediately
        if is_slash_open && key.code == KeyCode::Esc {
            self.reset();
            return None;
        }

        // Handle Up/Down arrow navigation across palette commands
        if is_slash_open && !matching.is_empty() {
            if key.code == KeyCode::Up {
                self.slash_selected_index = self.slash_selected_index.saturating_sub(1);
                return None;
            }
            if key.code == KeyCode::Down {
                if self.slash_selected_index + 1 < matching.len() {
                    self.slash_selected_index += 1;
                }
                return None;
            }
        }

        // Handle Tab to cycle categories or autocomplete
        if is_slash_open && key.code == KeyCode::Tab {
            self.cycle_category(true);
            return None;
        }
        if is_slash_open && key.code == KeyCode::BackTab {
            self.cycle_category(false);
            return None;
        }

        match (key.code, key.modifiers) {
            // Shift+Enter, Alt+Enter, Ctrl+Enter insert a newline
            (KeyCode::Enter, m)
                if m.contains(KeyModifiers::SHIFT)
                    || m.contains(KeyModifiers::ALT)
                    || m.contains(KeyModifiers::CONTROL) =>
            {
                self.textarea.insert_newline();
                self.auto_wrap();
                None
            }
            // Insert newline on Ctrl+J
            (KeyCode::Char('j'), KeyModifiers::CONTROL) => {
                self.textarea.insert_newline();
                self.auto_wrap();
                None
            }
            // Submit prompt on plain Enter
            (KeyCode::Enter, _) => {
                let text = self.textarea.lines().join("\n");
                let trimmed = text.trim().to_string();

                // If user typed an exact or prefix slash command with palette open,
                // resolve to the highlighted slash command
                let final_prompt = if is_slash_open && !matching.is_empty() {
                    if let Some(cmd) = self.selected_palette_command() {
                        cmd.slash_name.to_string()
                    } else {
                        trimmed
                    }
                } else {
                    trimmed
                };

                if !final_prompt.is_empty() {
                    let submission = if is_slash_open {
                        PromptSubmission {
                            display: final_prompt.clone(),
                            full: final_prompt,
                        }
                    } else {
                        self.resolve_submission(&final_prompt)
                    };
                    self.reset();
                    Some(submission)
                } else {
                    None
                }
            }
            _ => {
                self.textarea.input(key);
                let new_matches = self.matching_palette_commands();
                if self.slash_selected_index >= new_matches.len() {
                    self.slash_selected_index = new_matches.len().saturating_sub(1);
                }
                self.auto_wrap();
                None
            }
        }
    }

    pub fn render(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(ratatui::widgets::BorderType::Rounded)
            .border_style(Style::default().fg(theme.border).bg(theme.bg_input))
            .style(Style::default().bg(theme.bg_input));

        let inner_area = block.inner(area);
        frame.render_widget(block, area);

        // Subdivide inner area to render prefix ("› " / "- ") and text editor inline
        let input_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(2), // "› " on line 1, "- " on continuation lines
                Constraint::Min(1),    // TextArea input
            ])
            .split(inner_area);

        self.last_width.set(input_chunks[1].width);

        let view_height = inner_area.height as usize;
        let cursor_row = self.textarea.cursor().0;
        let scroll_row = crate::ui::layout_utils::compute_scroll_offset(cursor_row, view_height);

        let mut prefix_lines = Vec::with_capacity(view_height.max(1));
        for i in 0..view_height.max(1) {
            let actual_line_idx = scroll_row + i;
            if actual_line_idx == 0 {
                prefix_lines.push(Line::from(vec![Span::styled(
                    "› ",
                    Style::default()
                        .fg(theme.brand_accent)
                        .bg(theme.bg_input)
                        .add_modifier(Modifier::BOLD),
                )]));
            } else {
                prefix_lines.push(Line::from(vec![Span::styled(
                    "- ",
                    Style::default()
                        .fg(theme.muted)
                        .bg(theme.bg_input)
                        .add_modifier(Modifier::BOLD),
                )]));
            }
        }
        let prompt_widget = Paragraph::new(prefix_lines).style(Style::default().bg(theme.bg_input));
        frame.render_widget(prompt_widget, input_chunks[0]);

        let mut cloned = self.textarea.clone();
        cloned.set_style(Style::default().fg(theme.text_primary).bg(theme.bg_input));
        cloned.set_cursor_style(Style::default().fg(theme.bg_primary).bg(theme.brand_accent));
        // Explicitly disable underline on cursor line
        cloned.set_cursor_line_style(Style::default().bg(theme.bg_input));
        cloned.set_placeholder_style(Style::default().fg(theme.muted).bg(theme.bg_input));
        cloned.set_placeholder_text(DEFAULT_INPUT_PLACEHOLDER);
        cloned.set_block(
            Block::default()
                .borders(Borders::NONE)
                .style(Style::default().bg(theme.bg_input)),
        );

        frame.render_widget(&cloned, input_chunks[1]);
    }

    /// Renders the Clean Floating Spotlight Command Palette
    pub fn render_slash_palette(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        if !self.has_active_slash_query() {
            return;
        }

        let matches = self.matching_palette_commands();

        // Modal dimensions (responsive spotlight centered on screen)
        let width = ((area.width as u32 * COMMAND_PALETTE_WIDTH_PCT as u32 / 100) as u16)
            .clamp(COMMAND_PALETTE_MIN_WIDTH, COMMAND_PALETTE_MAX_WIDTH);
        let height = COMMAND_PALETTE_HEIGHT;

        let x = area.x + (area.width.saturating_sub(width)) / 2;
        let y = area.y + (area.height.saturating_sub(height)) / 3; // Position in upper-middle
        let popup_area = Rect::new(x, y, width.min(area.width), height.min(area.height));

        frame.render_widget(Clear, popup_area);

        // Build Title Bar with Category Radio Tabs on right
        let current_cat_idx = self.category_index % CommandCategory::all().len();
        let mut title_spans = vec![
            Span::styled(
                " ⌘ Commands ",
                Style::default()
                    .fg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("[Tab] ", Style::default().fg(theme.muted)),
        ];

        for (idx, cat) in CommandCategory::all().iter().enumerate() {
            let is_active = idx == current_cat_idx;
            if is_active {
                title_spans.push(Span::styled(
                    format!("◉ {} ", cat.label()),
                    Style::default()
                        .fg(theme.brand_accent)
                        .add_modifier(Modifier::BOLD),
                ));
            } else {
                title_spans.push(Span::styled(
                    format!("○ {} ", cat.label()),
                    Style::default().fg(theme.muted),
                ));
            }
        }
        title_spans.push(Span::raw(" "));

        let outer_block = Block::default()
            .title(Line::from(title_spans))
            .borders(Borders::ALL)
            .border_type(ratatui::widgets::BorderType::Rounded)
            .border_style(Style::default().fg(theme.brand_accent))
            .style(Style::default().bg(theme.bg_elevated));

        let inner_area = outer_block.inner(popup_area);
        frame.render_widget(outer_block, popup_area);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // Search row: › /search█
                Constraint::Length(1), // Divider
                Constraint::Min(4),    // Commands list
            ])
            .split(inner_area);

        // 1. Search Query Row
        let typed_text = self.textarea.lines().first().cloned().unwrap_or_default();
        let search_line = Line::from(vec![
            Span::styled(
                "  › ",
                Style::default()
                    .fg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                if typed_text.is_empty() {
                    "/"
                } else {
                    &typed_text
                },
                Style::default()
                    .fg(theme.text_primary)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("█", Style::default().fg(theme.brand_accent)),
        ]);
        frame.render_widget(Paragraph::new(search_line), chunks[0]);

        // Top Divider
        let divider = Paragraph::new(Line::from(vec![Span::styled(
            "─".repeat(inner_area.width as usize),
            Style::default().fg(theme.border),
        )]));
        frame.render_widget(divider, chunks[1]);

        // 2. Command Items List
        let list_height = chunks[2].height as usize;
        let selected_idx = self
            .slash_selected_index
            .min(matches.len().saturating_sub(1));

        // Viewport windowing calculation for smooth scrolling
        let scroll_offset =
            crate::ui::layout_utils::compute_scroll_offset(selected_idx, list_height);

        let mut item_lines = Vec::new();
        let inner_width = inner_area.width as usize;

        if matches.is_empty() {
            item_lines.push(Line::from(vec![Span::styled(
                "   No matching commands found",
                Style::default().fg(theme.muted),
            )]));
        } else {
            for (i, cmd) in matches
                .iter()
                .skip(scroll_offset)
                .take(list_height)
                .enumerate()
            {
                let actual_idx = scroll_offset + i;
                let is_selected = actual_idx == selected_idx;
                let shortcut_str = cmd.shortcut.unwrap_or("");

                let prefix = if is_selected { " ❯ " } else { "   " };
                let left_content = format!("{}{}", prefix, cmd.title);
                let left_width = UnicodeWidthStr::width(left_content.as_str());
                let shortcut_width = UnicodeWidthStr::width(shortcut_str);

                // Right-aligned shortcut badge using display width
                let avail_space = inner_width.saturating_sub(left_width + shortcut_width + 2);
                let padding = " ".repeat(avail_space);

                if is_selected {
                    let line_str = format!("{}{}{}", left_content, padding, shortcut_str);
                    let current_width = UnicodeWidthStr::width(line_str.as_str());
                    let trailing_spaces = " ".repeat(inner_width.saturating_sub(current_width));
                    let full_padded = format!("{}{}", line_str, trailing_spaces);
                    item_lines.push(Line::from(vec![Span::styled(
                        full_padded,
                        Style::default()
                            .bg(theme.brand_accent)
                            .fg(theme.bg_primary)
                            .add_modifier(Modifier::BOLD),
                    )]));
                } else {
                    let mut spans = vec![
                        Span::styled(left_content, Style::default().fg(theme.text_primary)),
                        Span::raw(padding),
                    ];
                    if !shortcut_str.is_empty() {
                        spans.push(Span::styled(shortcut_str, Style::default().fg(theme.muted)));
                    }
                    spans.push(Span::raw(" "));
                    item_lines.push(Line::from(spans));
                }
            }
        }

        let list_p = Paragraph::new(item_lines);
        frame.render_widget(list_p, chunks[2]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_matching_palette_commands_filtering() {
        let mut dock = InputDock::new();
        dock.textarea.insert_str("/mod");
        let matches = dock.matching_palette_commands();
        assert!(matches.iter().any(|c| c.title == "Switch Model"));

        let mut dock2 = InputDock::new();
        dock2.textarea.insert_str("/rev");
        let matches2 = dock2.matching_palette_commands();
        assert!(matches2.iter().any(|c| c.title == "Code Review"));
    }

    #[test]
    fn test_category_cycling() {
        let mut dock = InputDock::new();
        assert_eq!(dock.category_index, 0);
        dock.cycle_category(true);
        assert_eq!(dock.category_index, 1);
        dock.cycle_category(false);
        assert_eq!(dock.category_index, 0);
    }

    #[test]
    fn test_required_height() {
        let mut dock = InputDock::new();
        assert_eq!(dock.required_height(), 3); // 1 line + 2 borders

        dock.textarea.insert_newline();
        assert_eq!(dock.required_height(), 4); // 2 lines + 2 borders

        dock.textarea.insert_newline();
        assert_eq!(dock.required_height(), 5); // 3 lines + 2 borders

        dock.reset();
        assert_eq!(dock.required_height(), 3);
    }

    #[test]
    fn test_auto_wrap() {
        let mut dock = InputDock::new();
        // Set visible width to 24 (max_width will be (24-2) = 22)
        dock.last_width.set(24);
        dock.textarea
            .insert_str("This is a long sentence that wraps");
        dock.auto_wrap();

        let lines = dock.textarea.lines();
        assert!(lines.len() >= 2);
        assert_eq!(lines[0], "This is a long");
        assert_eq!(lines[1], "sentence that wraps");
        assert_eq!(dock.required_height(), 4);
    }

    #[test]
    fn test_short_paste_lte_5_lines() {
        let mut dock = InputDock::new();
        let short_paste = "line 1\nline 2\nline 3";
        dock.handle_paste(short_paste);

        // Should not create collapsed block for <= 5 lines
        assert!(dock.pasted_blocks.is_empty());
        assert_eq!(dock.textarea.lines().len(), 3);
        assert_eq!(dock.required_height(), 5); // 3 lines + 2 borders

        let submission = dock.resolve_submission(&dock.textarea.lines().join("\n"));
        assert_eq!(submission.display, "line 1\nline 2\nline 3");
        assert_eq!(submission.full, "line 1\nline 2\nline 3");
    }

    #[test]
    fn test_long_paste_gt_5_lines_collapses_to_preview() {
        let mut dock = InputDock::new();
        let long_code = "\
fn calculate_hash(data: &[u8]) -> u64 {
    let mut hash: u64 = 0x12345678;
    for &byte in data {
        hash = hash.rotate_left(5) ^ (byte as u64);
        hash = hash.wrapping_mul(0x5bd1e995);
    }
    hash ^= hash >> 15;
    hash = hash.wrapping_mul(0x1b873593);
    hash ^= hash >> 13;
    hash
}";
        let lines_count = long_code.lines().count();
        assert_eq!(lines_count, 11);

        dock.handle_paste(long_code);

        // Must create a single collapsed block
        assert_eq!(dock.pasted_blocks.len(), 1);
        let block = &dock.pasted_blocks[0];
        assert_eq!(block.line_count, 11);
        assert!(block.placeholder.contains("..... +11 lines]"));
        assert!(block.placeholder.starts_with("[fn calculate_hash"));

        // Textarea should contain placeholder, not raw 11 lines
        let ta_text = dock.textarea.lines().join("\n");
        assert!(ta_text.contains(&block.placeholder));

        // Now user types instructions after the placeholder
        dock.textarea.insert_str("please optimize this function");

        let current_text = dock.textarea.lines().join("\n");
        let submission = dock.resolve_submission(&current_text);

        // Display keeps the clean preview
        assert!(submission.display.contains(&block.placeholder));
        assert!(submission.display.contains("please optimize this function"));

        // Full submission contains the actual uncompressed 11 lines of code!
        assert!(submission
            .full
            .contains("fn calculate_hash(data: &[u8]) -> u64 {"));
        assert!(submission.full.contains("please optimize this function"));
        assert!(!submission.full.contains(&block.placeholder));
    }

    #[test]
    fn test_shift_enter_inserts_newline_and_plain_enter_submits() {
        let mut dock = InputDock::new();
        dock.textarea.insert_str("line 1");

        // Shift+Enter should insert newline, not submit
        let shift_enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT);
        let res = dock.handle_key(shift_enter);
        assert!(res.is_none());
        assert_eq!(dock.textarea.lines().len(), 2);

        dock.textarea.insert_str("line 2");

        // Plain Enter should submit both lines
        let plain_enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        let res = dock.handle_key(plain_enter);
        assert!(res.is_some());
        let submission = res.unwrap();
        assert_eq!(submission.display, "line 1\nline 2");
        assert_eq!(submission.full, "line 1\nline 2");

        // Dock should be cleanly reset
        assert_eq!(dock.textarea.lines().len(), 1);
        assert_eq!(dock.textarea.lines()[0], "");
    }

    #[test]
    fn test_height_capped_at_5_lines() {
        let mut dock = InputDock::new();
        for i in 1..=20 {
            if i > 1 {
                dock.textarea.insert_newline();
            }
            dock.textarea.insert_str(&format!("Line {}", i));
        }
        assert_eq!(dock.textarea.lines().len(), 20);
        // Required height is capped at 5 lines + 2 borders = 7
        assert_eq!(dock.required_height(), 7);
    }
}

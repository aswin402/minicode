use crate::ui::theme::Theme;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

/// Quadrant loading square spinner (rotating clockwise)
pub const SQUARE_SPINNER_FRAMES: &[&str] = &["▖", "▘", "▝", "▗"];

/// Standard Braille spinner
#[allow(dead_code)]
pub const BRAILLE_SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// Concentric stepped squares spinner
#[allow(dead_code)]
pub const CONCENTRIC_SPINNER_FRAMES: &[&str] = &["▫", "◽", "◻", "⬜", "◻", "◽"];

/// Pulsating block spinner
#[allow(dead_code)]
pub const PULSE_SPINNER_FRAMES: &[&str] = &["░", "▒", "▓", "█", "▓", "▒"];

/// MiniCode Brand Dual-Pillars spinner
#[allow(dead_code)]
pub const BRAND_SPINNER_FRAMES: &[&str] = &["▰▱", "▰▰", "▱▰", "▱▱"];

/// Represents the specific activity the agent is executing
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentActivity {
    Thinking,
    InternetResearch { query: String },
    RepoResearch { target: String },
    EditingFile { path: String },
    Debugging { step: String },
    ExecutingCommand { command: String },
    SubagentWorking { role: String },
    CompactingContext,
}

impl AgentActivity {
    /// Classifies a tool call and its arguments into an activity
    pub fn from_tool_call(tool: &str, args_str: &str) -> Self {
        let tool_lower = tool.to_ascii_lowercase();

        // 1. Internet / Web Research
        if tool_lower == "search_web"
            || tool_lower == "fetch_or_browse"
            || tool_lower == "browser_navigate"
            || tool_lower.contains("web")
            || tool_lower.contains("fetch")
            || tool_lower.contains("browse")
        {
            let query = Self::extract_arg_or_default(args_str, &["query", "url"], "web query");
            return AgentActivity::InternetResearch { query };
        }

        // 2. Code Edits & File Writing
        if tool_lower == "patch_file"
            || tool_lower == "write_file"
            || tool_lower == "create_file"
            || tool_lower == "replace_file_content"
            || tool_lower == "edit_file"
            || tool_lower.contains("patch")
        {
            let path =
                Self::extract_arg_or_default(args_str, &["path", "file_path", "target"], "file");
            return AgentActivity::EditingFile { path };
        }

        // 3. Command Execution & Debugging / Verification
        if tool_lower == "exec_cmd"
            || tool_lower == "sandbox_exec"
            || tool_lower == "run_command"
            || tool_lower == "execute_command"
        {
            let cmd = Self::extract_arg_or_default(args_str, &["command", "cmd"], "command");
            let cmd_lower = cmd.to_ascii_lowercase();
            if cmd_lower.contains("check")
                || cmd_lower.contains("test")
                || cmd_lower.contains("lint")
                || cmd_lower.contains("clippy")
                || cmd_lower.contains("tsc")
                || cmd_lower.contains("pytest")
                || cmd_lower.contains("vitest")
                || cmd_lower.contains("diff")
            {
                return AgentActivity::Debugging { step: cmd };
            } else {
                return AgentActivity::ExecutingCommand { command: cmd };
            }
        }

        // 4. Subagents
        if tool_lower.contains("subagent") || tool_lower.contains("swarm") {
            let role =
                Self::extract_arg_or_default(args_str, &["role", "name", "task"], "agent task");
            return AgentActivity::SubagentWorking { role };
        }

        // 5. Codebase & Repo Research
        if tool_lower == "read_file"
            || tool_lower == "find_files"
            || tool_lower == "grep_search"
            || tool_lower == "code_graph"
            || tool_lower == "semantic_search"
            || tool_lower == "repomap"
            || tool_lower == "recall_query"
            || tool_lower == "explore_codebase"
            || tool_lower.contains("graph")
            || tool_lower.contains("search")
            || tool_lower.contains("explore")
        {
            let target = Self::extract_arg_or_default(
                args_str,
                &["query", "path", "symbol", "name"],
                "codebase",
            );
            return AgentActivity::RepoResearch { target };
        }

        // Default to executing command or generic tool
        AgentActivity::ExecutingCommand {
            command: tool.to_string(),
        }
    }

    fn extract_arg_or_default(args_str: &str, keys: &[&str], default: &str) -> String {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(args_str) {
            for key in keys {
                if let Some(val) = v.get(*key).and_then(|val| val.as_str()) {
                    let s = val.trim();
                    if !s.is_empty() {
                        return if s.chars().count() > 36 {
                            format!("{}...", s.chars().take(33).collect::<String>())
                        } else {
                            s.to_string()
                        };
                    }
                }
            }
        }
        default.to_string()
    }
}

/// Extracts RGB components from any Ratatui Color safely
pub fn color_to_rgb(color: Color) -> (u8, u8, u8) {
    match color {
        Color::Rgb(r, g, b) => (r, g, b),
        Color::Reset => (200, 200, 200),
        Color::Black => (18, 18, 24),
        Color::Red => (255, 100, 100),
        Color::Green => (100, 255, 160),
        Color::Yellow => (255, 215, 100),
        Color::Blue => (100, 180, 255),
        Color::Magenta => (240, 120, 255),
        Color::Cyan => (100, 240, 255),
        Color::Gray => (140, 140, 150),
        Color::DarkGray => (80, 80, 90),
        Color::LightRed => (255, 140, 140),
        Color::LightGreen => (140, 255, 180),
        Color::LightYellow => (255, 235, 140),
        Color::LightBlue => (140, 200, 255),
        Color::LightMagenta => (255, 160, 255),
        Color::LightCyan => (140, 250, 255),
        Color::White => (245, 245, 245),
        Color::Indexed(idx) => (idx, idx, idx),
    }
}

/// Computes linear interpolation between two colors
pub fn lerp_color(c1: Color, c2: Color, factor: f32) -> Color {
    let t = factor.clamp(0.0, 1.0);
    let (r1, g1, b1) = color_to_rgb(c1);
    let (r2, g2, b2) = color_to_rgb(c2);

    let r = (r1 as f32 + (r2 as f32 - r1 as f32) * t).round() as u8;
    let g = (g1 as f32 + (g2 as f32 - g1 as f32) * t).round() as u8;
    let b = (b1 as f32 + (b2 as f32 - b1 as f32) * t).round() as u8;

    Color::Rgb(r, g, b)
}

/// Renders a string where each character is styled along a dynamic sine wave gradient
pub fn render_shimmer_spans(
    text: &str,
    c1: Color,
    c2: Color,
    millis: u64,
    speed: f32,
    frequency: f32,
) -> Vec<Span<'static>> {
    let mut spans = Vec::with_capacity(text.chars().count());
    let base_phase = (millis as f32) / speed;

    for (i, ch) in text.chars().enumerate() {
        let phase = base_phase - (i as f32 * frequency);
        let factor = (phase.sin() * 0.5 + 0.5).clamp(0.0, 1.0);
        let color = lerp_color(c1, c2, factor);
        spans.push(Span::styled(
            ch.to_string(),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ));
    }
    spans
}

/// Renders the complete live activity line with loading square spinner,
/// dynamic shimmering gradient text ("t to g..."), and elapsed timer.
pub fn render_live_activity_line(
    activity: &AgentActivity,
    millis: u64,
    elapsed_secs: f64,
    theme: &Theme,
) -> Line<'static> {
    // 1. Calculate spinner frame from SQUARE_SPINNER_FRAMES (rotating quadrant square)
    let spinner_idx =
        ((millis / crate::constants::SPINNER_FRAME_MS) as usize) % SQUARE_SPINNER_FRAMES.len();
    let spinner = SQUARE_SPINNER_FRAMES[spinner_idx];

    // 2. Determine activity title and dynamic color pair from the active Theme
    let (label, c1, c2) = match activity {
        AgentActivity::Thinking => (
            "Thinking...".to_string(),
            theme.brand_accent,
            theme.highlight,
        ),
        AgentActivity::InternetResearch { query } => (
            format!("Searching web: \"{}\"...", query),
            theme.info,
            theme.success,
        ),
        AgentActivity::RepoResearch { target } => (
            format!("Researching repo: {}...", target),
            theme.brand_accent,
            theme.info,
        ),
        AgentActivity::EditingFile { path } => (
            format!("Applying edits to {}...", path),
            theme.warning,
            theme.brand_accent,
        ),
        AgentActivity::Debugging { step } => (
            format!("Debugging & verifying: {}...", step),
            theme.success,
            theme.warning,
        ),
        AgentActivity::ExecutingCommand { command } => (
            format!("Executing: {}...", command),
            theme.info,
            theme.text_primary,
        ),
        AgentActivity::SubagentWorking { role } => (
            format!("Subagent active: {}...", role),
            theme.highlight,
            theme.brand_accent,
        ),
        AgentActivity::CompactingContext => (
            "Compacting conversation memory...".to_string(),
            theme.muted,
            theme.text_primary,
        ),
    };

    // 3. Assemble Spans:
    //    [Spinner] in c1
    //    [Shimmering Text]
    //    [Elapsed Time + Cancel Hint] in theme.muted
    let mut spans = Vec::new();

    // Spinner glyph in primary activity color with bold
    spans.push(Span::styled(
        format!("{}  ", spinner),
        Style::default().fg(c1).add_modifier(Modifier::BOLD),
    ));

    // Dynamic wave shimmer across the text: speed 160.0, frequency 0.32
    let shimmer_spans = render_shimmer_spans(&label, c1, c2, millis, 160.0, 0.32);
    spans.extend(shimmer_spans);

    // Elapsed timer & interrupt prompt
    spans.push(Span::styled(
        format!(" ({:.1}s • esc to interrupt)", elapsed_secs),
        Style::default().fg(theme.muted),
    ));

    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_color_to_rgb() {
        let (r, g, b) = color_to_rgb(Color::Rgb(10, 20, 30));
        assert_eq!((r, g, b), (10, 20, 30));

        let (r, g, b) = color_to_rgb(Color::White);
        assert_eq!((r, g, b), (245, 245, 245));
    }

    #[test]
    fn test_lerp_color() {
        let c1 = Color::Rgb(0, 0, 0);
        let c2 = Color::Rgb(100, 200, 50);

        let start = lerp_color(c1, c2, 0.0);
        assert_eq!(color_to_rgb(start), (0, 0, 0));

        let end = lerp_color(c1, c2, 1.0);
        assert_eq!(color_to_rgb(end), (100, 200, 50));

        let mid = lerp_color(c1, c2, 0.5);
        assert_eq!(color_to_rgb(mid), (50, 100, 25));
    }

    #[test]
    fn test_render_shimmer_spans() {
        let text = "Thinking...";
        let spans = render_shimmer_spans(
            text,
            Color::Rgb(100, 50, 200),
            Color::Rgb(200, 150, 255),
            120,
            160.0,
            0.32,
        );
        assert_eq!(spans.len(), text.chars().count());
        let joined: String = spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(joined, text);
    }

    #[test]
    fn test_activity_classification() {
        let act_web = AgentActivity::from_tool_call("search_web", r#"{"query": "rust ratatui"}"#);
        assert_eq!(
            act_web,
            AgentActivity::InternetResearch {
                query: "rust ratatui".to_string()
            }
        );

        let act_edit = AgentActivity::from_tool_call(
            "patch_file",
            r#"{"path": "src/main.rs", "content": ""}"#,
        );
        assert_eq!(
            act_edit,
            AgentActivity::EditingFile {
                path: "src/main.rs".to_string()
            }
        );

        let act_debug = AgentActivity::from_tool_call(
            "exec_cmd",
            r#"{"command": "cargo test -j 3 --lib ui::view"}"#,
        );
        assert_eq!(
            act_debug,
            AgentActivity::Debugging {
                step: "cargo test -j 3 --lib ui::view".to_string()
            }
        );

        let act_cmd =
            AgentActivity::from_tool_call("exec_cmd", r#"{"command": "echo hello world"}"#);
        assert_eq!(
            act_cmd,
            AgentActivity::ExecutingCommand {
                command: "echo hello world".to_string()
            }
        );
    }

    #[test]
    fn test_render_live_activity_line() {
        let theme = Theme::aura_dark();
        let activity = AgentActivity::Thinking;
        let line = render_live_activity_line(&activity, 250, 1.5, &theme);
        assert!(!line.spans.is_empty());
    }
}

use crate::ui::theme::Theme;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

/// Quadrant loading square spinner (rotating clockwise)
#[allow(dead_code)]
pub const SQUARE_SPINNER_FRAMES: &[&str] = &["▖", "▘", "▝", "▗"];

/// Standard Braille spinner
#[allow(dead_code)]
pub const BRAILLE_SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// Concentric stepped squares spinner (strictly 1-cell `Emoji=No` geometric squares)
#[allow(dead_code)]
pub const CONCENTRIC_SPINNER_FRAMES: &[&str] = &["▫", "▪", "◻", "◼", "◻", "▪"];

/// Pulsating block spinner
#[allow(dead_code)]
pub const PULSE_SPINNER_FRAMES: &[&str] = &["░", "▒", "▓", "█", "▓", "▒"];

/// MiniCode Brand Dual-Pillars spinner (alternating left and right pillar states)
pub const BRAND_SPINNER_FRAMES: &[(&str, &str)] = &[("▰", "▱"), ("▰", "▰"), ("▱", "▰"), ("▱", "▱")];

/// Rotating corner frame quadrants
#[allow(dead_code)]
pub const CORNER_ANGLES_FRAMES: &[&str] = &["◰", "◳", "◲", "◱"];

/// Horizontal scanning progress beam
#[allow(dead_code)]
pub const SCANNER_BAR_FRAMES: &[&str] = &[
    "▏", "▎", "▍", "▌", "▋", "▊", "▉", "█", "▉", "▊", "▋", "▌", "▍", "▎",
];

/// Dual orbital particle dots
#[allow(dead_code)]
pub const PARTICLE_ORBIT_FRAMES: &[&str] = &["⠋", "⠙", "⠚", "⠞", "⠦", "⠴", "⠲", "⠳"];

/// Hermes-style Kaomoji breathing & focus pulse spinner (strictly 1-cell EAW=N)
pub const KAOMOJI_PULSE_FRAMES: &[&str] =
    &["( -_-)", "( ._.)", "( °_°)", "( ◉_◉)", "( °_°)", "( ._.)"];

/// Expressive Kaomoji action cycle spinner (strictly 1-cell EAW=N)
pub const KAOMOJI_ACTION_FRAMES: &[&str] = &[
    "( ▸_▸)",
    "( ▸_▸)◈",
    "(⌐▪_▪)",
    "(ง°_°)ง",
    "( ◉_◉)⊃",
    "( ^_-)",
];

/// Cheerful dancing Kaomoji wave spinner (strictly 1-cell EAW=N)
pub const KAOMOJI_DANCE_FRAMES: &[&str] = &["~(˘▾˘~)", "(~˘▾˘)~", "\\(˘▾˘)/", "(~˘▾˘)~"];

/// Context-aware Activity Emojis preview cycle (Thinking 💭, Writing 📝, Editing 🔧, Reading 📖, Searching 🔭, Running 🚀)
pub const ACTIVITY_EMOJI_PREVIEW_FRAMES: &[&str] = &["💭", "📝", "🔧", "📖", "🔭", "🚀"];

/// Cyber geometric hexagon crystal pulse spinner (strictly 1-cell EAW=N)
pub const HEX_PULSE_FRAMES: &[&str] = &["⎔", "⏣", "◈", "◆", "◈", "⏣"];

/// Supported loading animation styles selectable by the user in /theme
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SpinnerStyle {
    #[default]
    DualPillars,
    Quadrants,
    PulseMatrix,
    Concentric,
    BrailleWave,
    CornerAngles,
    ScannerBar,
    ParticleOrbit,
    EmojiActivity,
    KaomojiPulse,
    KaomojiAction,
    KaomojiDance,
    HexPulse,
}

impl SpinnerStyle {
    pub fn from_id(id: &str) -> Self {
        match id {
            "quadrants" => SpinnerStyle::Quadrants,
            "pulse_matrix" => SpinnerStyle::PulseMatrix,
            "concentric" => SpinnerStyle::Concentric,
            "braille_wave" => SpinnerStyle::BrailleWave,
            "corner_angles" => SpinnerStyle::CornerAngles,
            "scanner_bar" => SpinnerStyle::ScannerBar,
            "particle_orbit" => SpinnerStyle::ParticleOrbit,
            "emoji_activity" | "emoji" | "emoji_orbit" | "emoji_moon" => {
                SpinnerStyle::EmojiActivity
            }
            "kaomoji_pulse" | "kaomoji" => SpinnerStyle::KaomojiPulse,
            "kaomoji_action" => SpinnerStyle::KaomojiAction,
            "kaomoji_dance" => SpinnerStyle::KaomojiDance,
            "hex_pulse" | "hex" => SpinnerStyle::HexPulse,
            _ => SpinnerStyle::DualPillars,
        }
    }

    #[allow(dead_code)]
    pub fn to_id(self) -> &'static str {
        match self {
            SpinnerStyle::DualPillars => "dual_pillars",
            SpinnerStyle::Quadrants => "quadrants",
            SpinnerStyle::PulseMatrix => "pulse_matrix",
            SpinnerStyle::Concentric => "concentric",
            SpinnerStyle::BrailleWave => "braille_wave",
            SpinnerStyle::CornerAngles => "corner_angles",
            SpinnerStyle::ScannerBar => "scanner_bar",
            SpinnerStyle::ParticleOrbit => "particle_orbit",
            SpinnerStyle::EmojiActivity => "emoji_activity",
            SpinnerStyle::KaomojiPulse => "kaomoji_pulse",
            SpinnerStyle::KaomojiAction => "kaomoji_action",
            SpinnerStyle::KaomojiDance => "kaomoji_dance",
            SpinnerStyle::HexPulse => "hex_pulse",
        }
    }
}

/// Metadata for selectable animations in the /theme modal
#[derive(Debug, Clone)]
pub struct AnimationOption {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub style: SpinnerStyle,
}

pub const ANIMATION_OPTIONS: &[AnimationOption] = &[
    AnimationOption {
        id: "dual_pillars",
        name: "MiniCode Dual-Pillars",
        description: "Stepped brand logo pillars with dynamic dual-color lighting",
        style: SpinnerStyle::DualPillars,
    },
    AnimationOption {
        id: "emoji_activity",
        name: "Activity Emoji (Gradient)",
        description: "Context-aware Emoji per action (💭 Thinking, 📝 Writing, 🔧 Editing, 🚀 Running) with theme gradient",
        style: SpinnerStyle::EmojiActivity,
    },
    AnimationOption {
        id: "kaomoji_action",
        name: "Activity Kaomoji (Gradient)",
        description: "Context-aware Kaomoji per action (( -_-) Thinking, ( ▸_▸)◈ Writing, (⌐▪_▪) Editing) with theme gradient",
        style: SpinnerStyle::KaomojiAction,
    },
    AnimationOption {
        id: "kaomoji_pulse",
        name: "Kaomoji Mind Pulse",
        description: "Expressive Hermes-style Kaomoji face cycle: ( -_-) -> ( °_°) -> ( ◉_◉) with theme gradient",
        style: SpinnerStyle::KaomojiPulse,
    },
    AnimationOption {
        id: "kaomoji_dance",
        name: "Kaomoji Vibe Wave",
        description: "Rhythmic animated Kaomoji wave: ~(˘▾˘~) -> \\(˘▾˘)/ -> (~˘▾˘)~ with theme gradient",
        style: SpinnerStyle::KaomojiDance,
    },
    AnimationOption {
        id: "hex_pulse",
        name: "Cyber Hex Crystal",
        description: "Resonant geometric hexagon & diamond core pulse: ⎔ -> ⏣ -> ◈ -> ◆",
        style: SpinnerStyle::HexPulse,
    },
    AnimationOption {
        id: "quadrants",
        name: "Rotating Quadrants",
        description: "Smooth 2x2 corner-orbit rotating square blocks",
        style: SpinnerStyle::Quadrants,
    },
    AnimationOption {
        id: "pulse_matrix",
        name: "Cyber Pulse Block",
        description: "High-tech breathing block density matrix",
        style: SpinnerStyle::PulseMatrix,
    },
    AnimationOption {
        id: "concentric",
        name: "Concentric Squares",
        description: "Nested geometric expanding and contracting boxes",
        style: SpinnerStyle::Concentric,
    },
    AnimationOption {
        id: "braille_wave",
        name: "Orbital Dots Wave",
        description: "Classic high-speed smooth Braille orbital spinner",
        style: SpinnerStyle::BrailleWave,
    },
    AnimationOption {
        id: "corner_angles",
        name: "Rotating Corner Frames",
        description: "Precision geometric corner frame quadrants",
        style: SpinnerStyle::CornerAngles,
    },
    AnimationOption {
        id: "scanner_bar",
        name: "Horizontal Pulse Bar",
        description: "Progressive horizontal block scanning beam",
        style: SpinnerStyle::ScannerBar,
    },
    AnimationOption {
        id: "particle_orbit",
        name: "Quantum Satellite",
        description: "Dual revolving particle dot satellites",
        style: SpinnerStyle::ParticleOrbit,
    },
];

/// Represents the specific activity the agent is executing
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentActivity {
    Thinking,
    ThinkingAfter {
        last_tool: String,
        step: usize,
    },
    Working,
    Responding,
    Planning {
        tool: String,
        target: String,
    },
    WritingFile {
        tool: String,
        path: String,
    },
    ReadingFile {
        tool: String,
        path: String,
    },
    InternetResearch {
        query: String,
    },
    RepoResearch {
        target: String,
    },
    EditingFile {
        path: String,
    },
    Debugging {
        step: String,
    },
    ExecutingCommand {
        command: String,
    },
    SubagentWorking {
        role: String,
    },
    UsingSkill {
        tool: String,
        skill: String,
    },
    CustomTool {
        verb: String,
        tool: String,
        target: String,
    },
    CompactingContext,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ToolActivityKind {
    Planning,
    Skill,
    InternetResearch,
    WritingFile,
    EditingFile,
    CommandExec,
    Subagent,
    ReadingFile,
    RepoResearch,
}

struct ActivityClassificationRule {
    kind: ToolActivityKind,
    exact_names: &'static [&'static str],
    substrings: &'static [&'static str],
    arg_keys: &'static [&'static str],
}

const ACTIVITY_CLASSIFICATION_RULES: &[ActivityClassificationRule] = &[
    ActivityClassificationRule {
        kind: ToolActivityKind::Planning,
        exact_names: &["create_plan", "update_plan", "todo_write", "update_todo"],
        substrings: &["plan", "todo"],
        arg_keys: &["title", "phase_label", "goal", "milestone", "task"],
    },
    ActivityClassificationRule {
        kind: ToolActivityKind::Skill,
        exact_names: &[
            "vault_show",
            "vault_load",
            "vault_create",
            "vault_update",
            "vault_patch",
            "vault_bundle_load",
            "vault_gotcha_add",
            "kit_skill_show",
            "kit_skill_install",
            "onpkg_skill_show",
            "onpkg_skill_install",
        ],
        substrings: &["vault_", "skill_"],
        arg_keys: &["name", "skill_name", "bundle", "trigger", "query"],
    },
    ActivityClassificationRule {
        kind: ToolActivityKind::InternetResearch,
        exact_names: &[
            "search_web",
            "webfetch",
            "fetch_or_browse",
            "browser_navigate",
        ],
        substrings: &["web", "fetch", "browse"],
        arg_keys: &["query", "url"],
    },
    ActivityClassificationRule {
        kind: ToolActivityKind::WritingFile,
        exact_names: &["write_file", "create_file"],
        substrings: &[],
        arg_keys: &["path", "file_path", "target"],
    },
    ActivityClassificationRule {
        kind: ToolActivityKind::EditingFile,
        exact_names: &[
            "patch_file",
            "replace_file_content",
            "edit_file",
            "ast_replace_node",
        ],
        substrings: &["patch", "edit", "write", "replace"],
        arg_keys: &["path", "file_path", "target"],
    },
    ActivityClassificationRule {
        kind: ToolActivityKind::CommandExec,
        exact_names: &["exec_cmd", "sandbox_exec", "run_command", "execute_command"],
        substrings: &["exec", "cmd", "command", "bash", "terminal"],
        arg_keys: &["command", "cmd"],
    },
    ActivityClassificationRule {
        kind: ToolActivityKind::Subagent,
        exact_names: &[],
        substrings: &["subagent", "swarm"],
        arg_keys: &[
            "role",
            "name",
            "subagent_id",
            "subagent_ids",
            "id",
            "ids",
            "action",
            "task",
        ],
    },
    ActivityClassificationRule {
        kind: ToolActivityKind::ReadingFile,
        exact_names: &["read_file", "view_file"],
        substrings: &[],
        arg_keys: &["path", "file_path", "target"],
    },
    ActivityClassificationRule {
        kind: ToolActivityKind::RepoResearch,
        exact_names: &[
            "find_files",
            "grep_search",
            "code_graph",
            "semantic_search",
            "repomap",
            "recall_query",
            "explore_codebase",
        ],
        substrings: &["graph", "search", "explore", "read", "find"],
        arg_keys: &["query", "path", "symbol", "name", "pattern"],
    },
];

const VERIFICATION_COMMAND_TOKENS: &[&str] = &[
    "check", "test", "lint", "clippy", "tsc", "pytest", "vitest", "diff",
];

impl AgentActivity {
    /// Classifies a tool call and its arguments into an activity via declarative rule table
    pub fn from_tool_call(tool: &str, args_str: &str) -> Self {
        let tool_lower = tool.trim().to_ascii_lowercase();
        if tool_lower.is_empty() {
            return AgentActivity::Working;
        }

        let matched_rule = ACTIVITY_CLASSIFICATION_RULES.iter().find(|rule| {
            rule.exact_names.contains(&tool_lower.as_str())
                || rule.substrings.iter().any(|sub| tool_lower.contains(sub))
        });

        match matched_rule {
            Some(rule) => {
                let raw_target = Self::extract_arg(args_str, rule.arg_keys).unwrap_or_default();
                match rule.kind {
                    ToolActivityKind::Planning => AgentActivity::Planning {
                        tool: tool_lower,
                        target: raw_target,
                    },
                    ToolActivityKind::Skill => AgentActivity::UsingSkill {
                        tool: tool_lower,
                        skill: raw_target,
                    },
                    ToolActivityKind::InternetResearch => {
                        AgentActivity::InternetResearch { query: raw_target }
                    }
                    ToolActivityKind::WritingFile => AgentActivity::WritingFile {
                        tool: tool_lower,
                        path: Self::short_display_path(&raw_target),
                    },
                    ToolActivityKind::EditingFile => AgentActivity::EditingFile {
                        path: Self::short_display_path(&raw_target),
                    },
                    ToolActivityKind::CommandExec => {
                        let cmd_lower = raw_target.to_ascii_lowercase();
                        let is_verify = VERIFICATION_COMMAND_TOKENS
                            .iter()
                            .any(|tok| cmd_lower.contains(tok));
                        match is_verify {
                            true => AgentActivity::Debugging { step: raw_target },
                            false => AgentActivity::ExecutingCommand {
                                command: raw_target,
                            },
                        }
                    }
                    ToolActivityKind::Subagent => {
                        AgentActivity::SubagentWorking { role: raw_target }
                    }
                    ToolActivityKind::ReadingFile => AgentActivity::ReadingFile {
                        tool: tool_lower,
                        path: Self::short_display_path(&raw_target),
                    },
                    ToolActivityKind::RepoResearch => {
                        AgentActivity::RepoResearch { target: raw_target }
                    }
                }
            }
            None => {
                let target = Self::extract_arg(
                    args_str,
                    &["path", "command", "query", "name", "target", "package"],
                )
                .unwrap_or_default();
                AgentActivity::CustomTool {
                    verb: "Working...".to_string(),
                    tool: tool_lower,
                    target,
                }
            }
        }
    }

    fn short_display_path(raw: &str) -> String {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return String::new();
        }
        // Keep last 2 path components if path has many segments
        let parts: Vec<&str> = trimmed.split('/').filter(|p| !p.is_empty()).collect();
        match parts.len() > 2 {
            true => {
                let tail = format!("{}/{}", parts[parts.len() - 2], parts[parts.len() - 1]);
                crate::utils::truncate_ellipsis(&tail, 36)
            }
            false => crate::utils::truncate_ellipsis(trimmed, 36),
        }
    }

    fn extract_arg(args_str: &str, keys: &[&str]) -> Option<String> {
        // 1. Try complete JSON parse
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(args_str) {
            for key in keys {
                if let Some(field) = v.get(*key) {
                    if let Some(val) = field.as_str() {
                        let clean = crate::ui::markdown::MarkdownRenderer::normalize_status_glyphs(
                            val.trim(),
                        );
                        let s = clean.trim();
                        if !s.is_empty() {
                            return Some(crate::utils::truncate_ellipsis(s, 36));
                        }
                    } else if let Some(arr) = field.as_array() {
                        let joined: Vec<String> = arr
                            .iter()
                            .filter_map(|item| item.as_str().map(|s| s.trim().to_string()))
                            .filter(|s| !s.is_empty())
                            .collect();
                        if !joined.is_empty() {
                            let clean =
                                crate::ui::markdown::MarkdownRenderer::normalize_status_glyphs(
                                    &joined.join(", "),
                                );
                            return Some(crate::utils::truncate_ellipsis(clean.trim(), 36));
                        }
                    }
                }
            }
        }
        // 2. Fallback for streaming partial JSON (`"path": "src/App.tsx"`) or XML (`<parameter name="path">src/App.tsx`)
        for key in keys {
            let json_pat = format!("\"{}\"", key);
            if let Some(pos) = args_str.find(&json_pat) {
                let after_key = &args_str[pos + json_pat.len()..];
                if let Some(colon_pos) = after_key.find(':') {
                    let after_colon = after_key[colon_pos + 1..].trim_start();
                    if let Some(rest) = after_colon.strip_prefix('"') {
                        let val: String = rest
                            .chars()
                            .take_while(|&c| c != '"' && c != '\n' && c != '\r')
                            .collect();
                        let clean = crate::ui::markdown::MarkdownRenderer::normalize_status_glyphs(
                            val.trim(),
                        );
                        let s = clean.trim();
                        if !s.is_empty() {
                            return Some(crate::utils::truncate_ellipsis(s, 36));
                        }
                    }
                }
            }
            let xml_pat = format!("name=\"{}\">", key);
            if let Some(pos) = args_str.find(&xml_pat) {
                let after_tag = &args_str[pos + xml_pat.len()..];
                let val: String = after_tag
                    .chars()
                    .take_while(|&c| c != '<' && c != '\n' && c != '\r')
                    .collect();
                let clean =
                    crate::ui::markdown::MarkdownRenderer::normalize_status_glyphs(val.trim());
                let s = clean.trim();
                if !s.is_empty() {
                    return Some(crate::utils::truncate_ellipsis(s, 36));
                }
            }
        }
        None
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

/// Renders the spinner glyph spans according to the selected SpinnerStyle
pub fn render_spinner_spans(
    style: SpinnerStyle,
    millis: u64,
    c1: Color,
    c2: Color,
    theme: &Theme,
) -> Vec<Span<'static>> {
    render_spinner_spans_with_activity(style, millis, c1, c2, theme, None, None)
}

/// Renders the spinner glyph spans with optional context-aware activity Emoji / Kaomoji
/// styled using the current theme's dynamic sine-wave gradient animation.
pub fn render_spinner_spans_with_activity(
    style: SpinnerStyle,
    millis: u64,
    c1: Color,
    c2: Color,
    theme: &Theme,
    activity_emoji: Option<&str>,
    activity_kaomoji: Option<&str>,
) -> Vec<Span<'static>> {
    match style {
        SpinnerStyle::DualPillars => {
            let spinner_idx = ((millis / 120) as usize) % BRAND_SPINNER_FRAMES.len();
            let (left_glyph, right_glyph) = BRAND_SPINNER_FRAMES[spinner_idx];
            let left_color = if left_glyph == "▰" {
                theme.brand_accent
            } else {
                theme.muted
            };
            let right_color = if right_glyph == "▰" {
                theme.info
            } else {
                theme.muted
            };
            vec![
                Span::styled(
                    left_glyph,
                    Style::default().fg(left_color).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("{}  ", right_glyph),
                    Style::default()
                        .fg(right_color)
                        .add_modifier(Modifier::BOLD),
                ),
            ]
        }
        SpinnerStyle::EmojiActivity => {
            let emoji = activity_emoji.unwrap_or_else(|| {
                let idx = ((millis / 500) as usize) % ACTIVITY_EMOJI_PREVIEW_FRAMES.len();
                ACTIVITY_EMOJI_PREVIEW_FRAMES[idx]
            });
            let phase = (millis as f32) / 160.0;
            let factor = (phase.sin() * 0.5 + 0.5).clamp(0.0, 1.0);
            let grad_color = lerp_color(c1, c2, factor);
            vec![Span::styled(
                format!("{}  ", emoji),
                Style::default().fg(grad_color).add_modifier(Modifier::BOLD),
            )]
        }
        SpinnerStyle::KaomojiAction => {
            let face = activity_kaomoji.unwrap_or_else(|| {
                let idx = ((millis / 420) as usize) % KAOMOJI_ACTION_FRAMES.len();
                KAOMOJI_ACTION_FRAMES[idx]
            });
            render_shimmer_spans(&format!("{}  ", face), c1, c2, millis, 160.0, 0.32)
        }
        SpinnerStyle::KaomojiPulse => {
            let idx = ((millis / 380) as usize) % KAOMOJI_PULSE_FRAMES.len();
            render_shimmer_spans(
                &format!("{}  ", KAOMOJI_PULSE_FRAMES[idx]),
                c1,
                c2,
                millis,
                160.0,
                0.32,
            )
        }
        SpinnerStyle::KaomojiDance => {
            let idx = ((millis / 260) as usize) % KAOMOJI_DANCE_FRAMES.len();
            render_shimmer_spans(
                &format!("{}  ", KAOMOJI_DANCE_FRAMES[idx]),
                c1,
                c2,
                millis,
                160.0,
                0.32,
            )
        }
        other => {
            let (frames, interval_ms) = match other {
                SpinnerStyle::Quadrants => {
                    (SQUARE_SPINNER_FRAMES, crate::constants::SPINNER_FRAME_MS)
                }
                SpinnerStyle::PulseMatrix => (PULSE_SPINNER_FRAMES, 100),
                SpinnerStyle::Concentric => (CONCENTRIC_SPINNER_FRAMES, 120),
                SpinnerStyle::BrailleWave => {
                    (BRAILLE_SPINNER_FRAMES, crate::constants::SPINNER_FRAME_MS)
                }
                SpinnerStyle::CornerAngles => (CORNER_ANGLES_FRAMES, 100),
                SpinnerStyle::ScannerBar => (SCANNER_BAR_FRAMES, 70),
                SpinnerStyle::HexPulse => (HEX_PULSE_FRAMES, 140),
                _ => (PARTICLE_ORBIT_FRAMES, crate::constants::SPINNER_FRAME_MS),
            };
            let idx = ((millis / interval_ms) as usize) % frames.len();
            let phase = (millis as f32) / 160.0;
            let factor = (phase.sin() * 0.5 + 0.5).clamp(0.0, 1.0);
            let grad_color = lerp_color(c1, c2, factor);
            vec![Span::styled(
                format!("{}  ", frames[idx]),
                Style::default().fg(grad_color).add_modifier(Modifier::BOLD),
            )]
        }
    }
}

/// Renders a single preview of a spinner for the /theme modal
pub fn render_preview_spinner(
    style: SpinnerStyle,
    millis: u64,
    theme: &Theme,
) -> Vec<Span<'static>> {
    render_spinner_spans(style, millis, theme.brand_accent, theme.info, theme)
}

/// Renders the complete live activity line in Classic mode for backward compatibility.
#[allow(dead_code)]
pub fn render_live_activity_line(
    activity: &AgentActivity,
    spinner_style: SpinnerStyle,
    millis: u64,
    elapsed_secs: f64,
    theme: &Theme,
) -> Line<'static> {
    render_live_activity_line_with_style(
        activity,
        spinner_style,
        crate::config::ActivityBarStyle::Classic,
        millis,
        elapsed_secs,
        theme,
    )
}

/// Renders the live activity status bar according to the user's selected `ActivityBarStyle`.
pub fn render_live_activity_line_with_style(
    activity: &AgentActivity,
    spinner_style: SpinnerStyle,
    activity_style: crate::config::ActivityBarStyle,
    millis: u64,
    elapsed_secs: f64,
    theme: &Theme,
) -> Line<'static> {
    use crate::config::ActivityBarStyle;
    use std::borrow::Cow;

    let thinking_kaomojis = ["( -_-)", "( ._.)", "( °_°)"];
    let think_k_idx = ((millis / 800) as usize) % thinking_kaomojis.len();

    // Extract structured descriptors for the active state/action:
    // (verb_label, trail_gerund, Option<(tool_name, target)>, emoji_glyph, kaomoji_face, c1, c2)
    type ActivityDescriptor<'a> = (
        &'a str,
        &'a str,
        Option<(&'a str, &'a str)>,
        &'a str,
        &'a str,
        Color,
        Color,
    );
    let (verb_label, trail_gerund, action_info, emoji_glyph, kaomoji_face, c1, c2): ActivityDescriptor<'_> =
        match activity {
            AgentActivity::Thinking => (
                "Thinking...",
                "",
                None,
                "💭",
                thinking_kaomojis[think_k_idx],
                theme.brand_accent,
                theme.highlight,
            ),
            AgentActivity::ThinkingAfter { last_tool, .. } => (
                "Thinking...",
                "after",
                Some((last_tool.as_str(), "")),
                "💭",
                thinking_kaomojis[think_k_idx],
                theme.brand_accent,
                theme.highlight,
            ),
            AgentActivity::Working => (
                "Working...",
                "processing",
                None,
                "🌟",
                "( •_•)",
                theme.brand_accent,
                theme.info,
            ),
            AgentActivity::Responding => (
                "Generating...",
                "",
                None,
                "💬",
                "( ^_-)",
                theme.info,
                theme.brand_accent,
            ),
            AgentActivity::Planning { tool, target } => (
                "Planning...",
                "creating plan",
                Some((tool.as_str(), target.as_str())),
                "🧭",
                "( °_°)◈",
                theme.brand_accent,
                theme.highlight,
            ),
            AgentActivity::WritingFile { tool, path } => (
                "Writing...",
                "writing",
                Some((tool.as_str(), path.as_str())),
                "📝",
                "( ▸_▸)◈",
                theme.info,
                theme.brand_accent,
            ),
            AgentActivity::EditingFile { path } => (
                "Editing...",
                "editing",
                Some(("patch_file", path.as_str())),
                "🔧",
                "(⌐▪_▪)",
                theme.info,
                theme.brand_accent,
            ),
            AgentActivity::ReadingFile { tool, path } => (
                "Reading...",
                "reading",
                Some((tool.as_str(), path.as_str())),
                "📖",
                "( ◉_◉)",
                theme.info,
                theme.brand_accent,
            ),
            AgentActivity::RepoResearch { target } => (
                "Searching...",
                "searching",
                Some(("grep_search", target.as_str())),
                "🔭",
                "( ◉_◉)",
                theme.info,
                theme.brand_accent,
            ),
            AgentActivity::InternetResearch { query } => (
                "Browsing...",
                "browsing",
                Some(("search_web", query.as_str())),
                "📡",
                "( °o°)",
                theme.info,
                theme.success,
            ),
            AgentActivity::Debugging { step } => (
                "Verifying...",
                "verifying",
                Some(("exec_cmd", step.as_str())),
                "🚀",
                "(ง°_°)ง",
                theme.info,
                theme.brand_accent,
            ),
            AgentActivity::ExecutingCommand { command } => (
                "Running...",
                "running",
                Some(("exec_cmd", command.as_str())),
                "🚀",
                "(ง°_°)ง",
                theme.info,
                theme.brand_accent,
            ),
            AgentActivity::SubagentWorking { role } => (
                "Subagent...",
                "subagent",
                Some(("spawn_subagent", role.as_str())),
                "🐝",
                "( ◉_◉)⊃",
                theme.highlight,
                theme.brand_accent,
            ),
            AgentActivity::UsingSkill { tool, skill } => (
                "Using skill...",
                "skill",
                Some((tool.as_str(), skill.as_str())),
                "🧠",
                "( ✧_✧)",
                theme.brand_accent,
                theme.highlight,
            ),
            AgentActivity::CustomTool { verb, tool, target } => (
                verb.as_str(),
                "executing",
                Some((tool.as_str(), target.as_str())),
                "🌟",
                "( •_•)",
                theme.info,
                theme.brand_accent,
            ),
            AgentActivity::CompactingContext => (
                "Compacting...",
                "compacting",
                Some(("context_memory", "")),
                "💾",
                "( ~_~)",
                theme.muted,
                theme.text_primary,
            ),
        };

    // 1. Classic mode preserves the original single-label rendering
    if activity_style == ActivityBarStyle::Classic {
        let label: Cow<'static, str> = match activity {
            AgentActivity::Thinking | AgentActivity::ThinkingAfter { .. } => {
                Cow::Borrowed("Thinking...")
            }
            AgentActivity::Working => Cow::Borrowed("Working..."),
            AgentActivity::Responding => Cow::Borrowed("Generating..."),
            AgentActivity::Planning { .. } => Cow::Borrowed("Planning..."),
            AgentActivity::WritingFile { .. } => Cow::Borrowed("Writing files..."),
            AgentActivity::ReadingFile { .. } | AgentActivity::RepoResearch { .. } => {
                Cow::Borrowed("Searching files...")
            }
            AgentActivity::InternetResearch { .. } => Cow::Borrowed("Searching web..."),
            AgentActivity::EditingFile { .. } => Cow::Borrowed("Editing files..."),
            AgentActivity::Debugging { .. } | AgentActivity::ExecutingCommand { .. } => {
                Cow::Borrowed("Running command...")
            }
            AgentActivity::SubagentWorking { role } => {
                Cow::Owned(format!("Subagent active: {}...", role))
            }
            AgentActivity::UsingSkill { skill, .. } => {
                if skill.is_empty() {
                    Cow::Borrowed("Using skill...")
                } else {
                    Cow::Owned(format!("Using skill: {}...", skill))
                }
            }
            AgentActivity::CustomTool { verb, .. } => Cow::Owned(verb.clone()),
            AgentActivity::CompactingContext => Cow::Borrowed("Compacting memory..."),
        };

        let mut spans = render_spinner_spans_with_activity(
            spinner_style,
            millis,
            c1,
            c2,
            theme,
            Some(emoji_glyph),
            Some(kaomoji_face),
        );
        spans.extend(render_shimmer_spans(&label, c1, c2, millis, 160.0, 0.32));
        spans.push(Span::styled(
            format!(" ({:.1}s • esc to interrupt)", elapsed_secs),
            Style::default().fg(theme.muted),
        ));
        return Line::from(spans);
    }

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum PostPipeBadgeKind {
        None,
        Emoji,
        Kaomoji,
    }

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum PipeFormatKind {
        Detail,
        ToolOnly,
        Trail,
        CompactBadge,
        CyberArrow,
    }

    let badge_kind = match activity_style {
        ActivityBarStyle::EmojiDetail
        | ActivityBarStyle::EmojiTool
        | ActivityBarStyle::EmojiTrail => PostPipeBadgeKind::Emoji,
        ActivityBarStyle::KaomojiDetail
        | ActivityBarStyle::KaomojiTool
        | ActivityBarStyle::KaomojiTrail => PostPipeBadgeKind::Kaomoji,
        ActivityBarStyle::ActionDetail
        | ActivityBarStyle::ActionTool
        | ActivityBarStyle::HermesTrail
        | ActivityBarStyle::CompactBadge
        | ActivityBarStyle::CyberArrow
        | ActivityBarStyle::Classic => PostPipeBadgeKind::None,
    };

    let pipe_format = match activity_style {
        ActivityBarStyle::ActionTool
        | ActivityBarStyle::EmojiTool
        | ActivityBarStyle::KaomojiTool => PipeFormatKind::ToolOnly,
        ActivityBarStyle::HermesTrail
        | ActivityBarStyle::EmojiTrail
        | ActivityBarStyle::KaomojiTrail => PipeFormatKind::Trail,
        ActivityBarStyle::CompactBadge => PipeFormatKind::CompactBadge,
        ActivityBarStyle::CyberArrow => PipeFormatKind::CyberArrow,
        ActivityBarStyle::ActionDetail
        | ActivityBarStyle::EmojiDetail
        | ActivityBarStyle::KaomojiDetail
        | ActivityBarStyle::Classic => PipeFormatKind::Detail,
    };

    let mut spans = render_spinner_spans_with_activity(
        spinner_style,
        millis,
        c1,
        c2,
        theme,
        Some(emoji_glyph),
        Some(kaomoji_face),
    );

    // Helper closure: renders the Emoji or Kaomoji badge ONLY after the `|` pipe
    // with the current theme's dynamic sine-wave gradient animation!
    let push_post_pipe_badge = |spans: &mut Vec<Span<'static>>| match badge_kind {
        PostPipeBadgeKind::None => {}
        PostPipeBadgeKind::Emoji => {
            let emoji_phase = (millis as f32) / 160.0;
            let emoji_factor = (emoji_phase.sin() * 0.5 + 0.5).clamp(0.0, 1.0);
            let emoji_color = lerp_color(c1, c2, emoji_factor);
            spans.push(Span::styled(
                format!("{}  ", emoji_glyph),
                Style::default()
                    .fg(emoji_color)
                    .add_modifier(Modifier::BOLD),
            ));
        }
        PostPipeBadgeKind::Kaomoji => {
            spans.extend(render_shimmer_spans(
                &format!("{}  ", kaomoji_face),
                c1,
                c2,
                millis,
                160.0,
                0.32,
            ));
        }
    };

    // Left shimmering phase/action label:
    // In Trail mode (`HermesTrail`, `EmojiTrail`, `KaomojiTrail`), active tools display `"Working..."` on the left of `|`
    let left_shimmer_label = match (
        pipe_format,
        matches!(
            activity,
            AgentActivity::Thinking
                | AgentActivity::ThinkingAfter { .. }
                | AgentActivity::Responding
                | AgentActivity::CompactingContext
        ),
    ) {
        (PipeFormatKind::Trail, false) => "Working...",
        _ => verb_label,
    };
    spans.extend(render_shimmer_spans(
        left_shimmer_label,
        c1,
        c2,
        millis,
        160.0,
        0.32,
    ));

    // Right side of `|` pipe: ONLY shown for actual actions (or in Trail mode when ThinkingAfter a tool)
    match activity {
        AgentActivity::Thinking | AgentActivity::Working | AgentActivity::Responding => {
            // No `|` pipe during pure Thinking / Responding / generic Working
        }
        AgentActivity::ThinkingAfter { last_tool, step } => {
            if pipe_format == PipeFormatKind::Trail {
                spans.push(Span::styled(" | ", Style::default().fg(theme.muted)));
                push_post_pipe_badge(&mut spans);
                spans.push(Span::styled(
                    format!("after {}", last_tool),
                    Style::default().fg(theme.info),
                ));
                if *step > 0 {
                    spans.push(Span::styled(
                        format!(" · step {}", step),
                        Style::default().fg(theme.muted),
                    ));
                }
            }
        }
        _ => {
            if let Some((tool_name, target)) = action_info {
                spans.push(Span::styled(" | ", Style::default().fg(theme.muted)));
                push_post_pipe_badge(&mut spans);
                let target_clean = target.trim();
                let has_meaningful_target = !target_clean.is_empty();

                match pipe_format {
                    PipeFormatKind::Trail => match has_meaningful_target {
                        false => {
                            spans.push(Span::styled(
                                format!("{} {}", trail_gerund, tool_name),
                                Style::default().fg(theme.info).add_modifier(Modifier::BOLD),
                            ));
                        }
                        true => {
                            spans.push(Span::styled(
                                format!("{} ", trail_gerund),
                                Style::default().fg(theme.info).add_modifier(Modifier::BOLD),
                            ));
                            spans.push(Span::styled(
                                target_clean.to_string(),
                                Style::default().fg(theme.text_primary),
                            ));
                        }
                    },
                    PipeFormatKind::ToolOnly => {
                        spans.push(Span::styled(
                            tool_name.to_string(),
                            Style::default().fg(theme.info).add_modifier(Modifier::BOLD),
                        ));
                    }
                    PipeFormatKind::CompactBadge => {
                        spans.push(Span::styled(
                            format!("[{}]", tool_name),
                            Style::default()
                                .fg(theme.brand_accent)
                                .add_modifier(Modifier::BOLD),
                        ));
                        if has_meaningful_target {
                            spans.push(Span::styled(
                                format!(" {}", target_clean),
                                Style::default().fg(theme.text_primary),
                            ));
                        }
                    }
                    PipeFormatKind::CyberArrow => {
                        spans.push(Span::styled(
                            "▸ ",
                            Style::default()
                                .fg(theme.brand_accent)
                                .add_modifier(Modifier::BOLD),
                        ));
                        spans.push(Span::styled(
                            tool_name.to_string(),
                            Style::default().fg(theme.info).add_modifier(Modifier::BOLD),
                        ));
                        if has_meaningful_target {
                            spans.push(Span::styled(" ──▸ ", Style::default().fg(theme.muted)));
                            spans.push(Span::styled(
                                target_clean.to_string(),
                                Style::default().fg(theme.text_primary),
                            ));
                        }
                    }
                    PipeFormatKind::Detail => {
                        spans.push(Span::styled(
                            tool_name.to_string(),
                            Style::default().fg(theme.info).add_modifier(Modifier::BOLD),
                        ));
                        if has_meaningful_target {
                            spans.push(Span::styled(" · ", Style::default().fg(theme.muted)));
                            spans.push(Span::styled(
                                target_clean.to_string(),
                                Style::default().fg(theme.text_primary),
                            ));
                        }
                    }
                }
            }
        }
    }

    // Elapsed timer & cancel hint
    spans.push(Span::styled(
        format!(" ({:.1}s · esc to cancel)", elapsed_secs),
        Style::default().fg(theme.muted),
    ));

    Line::from(spans)
}

/// Post-processes the Ratatui frame buffer right before differential terminal rendering
/// to prevent 2-cell emojis from retaining stale ASCII letters in their right-half cell (`x + 1`)
/// or drifting the terminal cursor on `wcwidth=1` vs `wcwidth=2` terminals.
///
/// Specifically, for any 2-cell decorative emoji at `(x, y)`:
/// 1. Wraps the symbol as `"  \x08\x08<emoji> "` so the terminal first overwrites both cells
///    `x` and `x + 1` with spaces, backspaces twice to `x`, draws `<emoji>`, and writes a
///    trailing space (clearing `x + 1` on `wcwidth=1` terminals or `x + 2` on `wcwidth=2`).
/// 2. Because the wrapped symbol has `UnicodeWidthStr::width >= 3`, Ratatui's `Buffer::diff`
///    skips `(x + 1, y)` (`to_skip > 0`) and marks `(x + 2, y)` as `invalidated > 0`, which
///    forces `CrosstermBackend::draw` to emit an explicit `MoveTo(x + 2, y)` cursor jump
///    before printing any subsequent text on the line.
pub fn sanitize_emoji_buffer_cells(buf: &mut ratatui::buffer::Buffer) {
    use unicode_width::UnicodeWidthStr;
    let area = buf.area;
    let right = area.right();
    for y in area.top()..area.bottom() {
        let mut x = area.left();
        while x < right {
            let sym = buf[(x, y)].symbol();
            let w = sym.width();
            if w == 2
                && sym.chars().next().is_some_and(
                    crate::ui::markdown::MarkdownRenderer::is_decorative_emoji_or_symbol,
                )
            {
                let guarded = format!("  \x08\x08{} ", sym);
                buf[(x, y)].set_symbol(&guarded);
                if x + 1 < right {
                    buf[(x + 1, y)].set_symbol(" ");
                }
                x = x.saturating_add(2);
            } else {
                x = x.saturating_add(1);
            }
        }
    }
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
        let activities = vec![
            (AgentActivity::Thinking, "Thinking..."),
            (AgentActivity::Working, "Working..."),
            (AgentActivity::Responding, "Generating..."),
            (
                AgentActivity::EditingFile {
                    path: "src/main.rs".to_string(),
                },
                "Editing files...",
            ),
            (
                AgentActivity::ExecutingCommand {
                    command: "cargo test".to_string(),
                },
                "Running command...",
            ),
        ];
        for (activity, expected_label) in activities {
            for opt in ANIMATION_OPTIONS {
                let line = render_live_activity_line(&activity, opt.style, 250, 1.5, &theme);
                assert!(!line.spans.is_empty());
                let full_text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
                assert!(
                    full_text.contains(expected_label),
                    "Expected line '{}' to contain '{}'",
                    full_text,
                    expected_label
                );
            }
        }
    }

    #[test]
    fn test_render_live_activity_all_10_styles() {
        use crate::config::ActivityBarStyle;
        let theme = Theme::aura_dark();
        let all_styles = [
            ActivityBarStyle::Classic,
            ActivityBarStyle::ActionDetail,
            ActivityBarStyle::ActionTool,
            ActivityBarStyle::HermesTrail,
            ActivityBarStyle::EmojiDetail,
            ActivityBarStyle::EmojiTool,
            ActivityBarStyle::EmojiTrail,
            ActivityBarStyle::KaomojiDetail,
            ActivityBarStyle::KaomojiTool,
            ActivityBarStyle::KaomojiTrail,
            ActivityBarStyle::CompactBadge,
            ActivityBarStyle::CyberArrow,
        ];

        // 1. Pure Thinking has NO pipe `|` and NO emoji/kaomoji prefix in any style
        for style in all_styles {
            let line = render_live_activity_line_with_style(
                &AgentActivity::Thinking,
                SpinnerStyle::DualPillars,
                style,
                400,
                4.2,
                &theme,
            );
            let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
            assert!(text.contains("Thinking..."), "style {:?}: {}", style, text);
            assert!(
                !text.contains(" | "),
                "Pure Thinking must not contain pipe in style {:?}: {}",
                style,
                text
            );
            assert!(
                !text.contains("💭") && !text.contains("( -_-)"),
                "Pure Thinking must not show emoji/kaomoji before pipe in style {:?}: {}",
                style,
                text
            );
        }

        // 2. WritingFile via partial JSON / XML extraction
        let write_act = AgentActivity::from_tool_call(
            "write_file",
            r#"<parameter name="path">src/components/Hero.tsx</parameter><parameter name="content">export"#,
        );
        assert_eq!(
            write_act,
            AgentActivity::WritingFile {
                tool: "write_file".to_string(),
                path: "components/Hero.tsx".to_string(),
            }
        );

        let detail_line = render_live_activity_line_with_style(
            &write_act,
            SpinnerStyle::DualPillars,
            ActivityBarStyle::ActionDetail,
            400,
            18.4,
            &theme,
        );
        let detail_text: String = detail_line
            .spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect();
        assert!(detail_text.contains("Writing... | write_file · components/Hero.tsx"));
        assert!(detail_text.contains("(18.4s · esc to cancel)"));

        let tool_line = render_live_activity_line_with_style(
            &write_act,
            SpinnerStyle::DualPillars,
            ActivityBarStyle::ActionTool,
            400,
            18.4,
            &theme,
        );
        let tool_text: String = tool_line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(tool_text.contains("Writing... | write_file (18.4s · esc to cancel)"));

        let trail_line = render_live_activity_line_with_style(
            &write_act,
            SpinnerStyle::DualPillars,
            ActivityBarStyle::HermesTrail,
            400,
            18.4,
            &theme,
        );
        let trail_text: String = trail_line
            .spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect();
        assert!(trail_text.contains("Working... | writing components/Hero.tsx"));

        let emoji_line = render_live_activity_line_with_style(
            &write_act,
            SpinnerStyle::DualPillars,
            ActivityBarStyle::EmojiDetail,
            400,
            18.4,
            &theme,
        );
        let emoji_text: String = emoji_line
            .spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect();
        assert!(emoji_text.contains("Writing... | 📝  write_file · components/Hero.tsx"));

        let kaomoji_line = render_live_activity_line_with_style(
            &write_act,
            SpinnerStyle::DualPillars,
            ActivityBarStyle::KaomojiDetail,
            400,
            18.4,
            &theme,
        );
        let kaomoji_text: String = kaomoji_line
            .spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect();
        assert!(kaomoji_text.contains("Writing... | ( ▸_▸)◈  write_file · components/Hero.tsx"));

        let badge_line = render_live_activity_line_with_style(
            &write_act,
            SpinnerStyle::DualPillars,
            ActivityBarStyle::CompactBadge,
            400,
            18.4,
            &theme,
        );
        let badge_text: String = badge_line
            .spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect();
        assert!(badge_text.contains("Writing... | [write_file] components/Hero.tsx"));

        let arrow_line = render_live_activity_line_with_style(
            &write_act,
            SpinnerStyle::DualPillars,
            ActivityBarStyle::CyberArrow,
            400,
            18.4,
            &theme,
        );
        let arrow_text: String = arrow_line
            .spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect();
        assert!(arrow_text.contains("Writing... | ▸ write_file ──▸ components/Hero.tsx"));

        // 3. Activity Emoji spinner (`SpinnerStyle::EmojiActivity`) shows context-aware emoji per action
        let emoji_spin_think = render_live_activity_line_with_style(
            &AgentActivity::Thinking,
            SpinnerStyle::EmojiActivity,
            ActivityBarStyle::ActionDetail,
            400,
            4.2,
            &theme,
        );
        let emoji_spin_think_text: String = emoji_spin_think
            .spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect();
        assert!(emoji_spin_think_text.starts_with("💭  Thinking..."));

        let emoji_spin_write = render_live_activity_line_with_style(
            &write_act,
            SpinnerStyle::EmojiActivity,
            ActivityBarStyle::ActionDetail,
            400,
            18.4,
            &theme,
        );
        let emoji_spin_write_text: String = emoji_spin_write
            .spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect();
        assert!(
            emoji_spin_write_text.starts_with("📝  Writing... | write_file · components/Hero.tsx")
        );
    }

    #[test]
    fn test_sanitize_emoji_buffer_cells_clears_right_half_and_realigns_diff() {
        use ratatui::buffer::Buffer;
        use ratatui::layout::Rect;
        use ratatui::widgets::{Paragraph, Widget};

        let area = Rect::new(0, 0, 40, 1);
        let mut prev_buf = Buffer::empty(area);
        Paragraph::new("▰▱ Thinking...").render(area, &mut prev_buf);
        // Column 3 had 'T', column 4 had 'h'
        assert_eq!(prev_buf[(3, 0)].symbol(), "T");
        assert_eq!(prev_buf[(4, 0)].symbol(), "h");

        let mut next_buf = Buffer::empty(area);
        Paragraph::new("▰▱ 💭  Thinking...").render(area, &mut next_buf);
        sanitize_emoji_buffer_cells(&mut next_buf);

        assert_eq!(next_buf[(3, 0)].symbol(), "  \x08\x08💭 ");
        assert_eq!(next_buf[(4, 0)].symbol(), " ");

        let diff = prev_buf.diff(&next_buf);
        // (3, 0) must be emitted with the space+backspace+emoji guard
        assert!(diff
            .iter()
            .any(|(x, y, c)| *x == 3 && *y == 0 && c.symbol() == "  \x08\x08💭 "));
        // (4, 0) must be skipped by to_skip so it doesn't overwrite the emoji after drawing
        assert!(!diff.iter().any(|(x, y, _)| *x == 4 && *y == 0));
        // (5, 0) must be emitted so CrosstermBackend issues MoveTo(5, 0) after skipping (4, 0)
        assert!(diff.iter().any(|(x, y, _)| *x == 5 && *y == 0));
    }
}

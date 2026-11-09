//! Theme switcher and streaming configuration modals rendering.

use crate::constants::THEME_NAME_DISPLAY_COLS;
use crate::ui::modals::common::compute_scroll_offset;
use crate::ui::theme::{Theme, ThemeInfo};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

fn truncate_str(s: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let count = s.chars().count();
    if count <= max_chars {
        s.to_string()
    } else if max_chars <= 3 {
        s.chars().take(max_chars).collect()
    } else {
        let mut out: String = s.chars().take(max_chars - 1).collect();
        out.push('…');
        out
    }
}

#[derive(Debug, Clone)]
pub struct StylePreviewOption {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub preview: &'static [&'static str],
}

pub type TodoStyleOption = StylePreviewOption;
pub type SwarmStyleOption = StylePreviewOption;
pub type ToastStyleOption = StylePreviewOption;
pub type ToastSymbolOption = StylePreviewOption;
pub type ActivityStyleOption = StylePreviewOption;
pub type MenuStyleOption = StylePreviewOption;

pub static TODO_STYLE_OPTIONS: &[TodoStyleOption] = &[
    TodoStyleOption {
        id: "tree",
        name: "Tree (Oh My Pi)",
        description: "Branch connectors with status icons (Default)",
        preview: &[
            "TODO",
            "└── Plan · 1/3",
            "    ├── ✓ T1: Scaffolding and project structure setup",
            "    ├── > T2: Form validation logic and interactive UI",
            "    └── ○ T3: End-to-end verification and documentation",
        ],
    },
    TodoStyleOption {
        id: "card",
        name: "Card Container",
        description: "Rounded boxed card container with milestone badge",
        preview: &[
            "╭── ◆ TODO · 1/3 Tasks Completed ───────────────── [Phase 142] ──╮",
            "│ └── Tasks                                                      │",
            "│     ├── ✓ T1: Scaffolding and project structure setup          │",
            "│     ├── > T2: Form validation logic and interactive UI         │",
            "│     └── ○ T3: End-to-end verification and documentation        │",
            "╰────────────────────────────────────────────────────────────────╯",
        ],
    },
    TodoStyleOption {
        id: "rail",
        name: "Rail Accent",
        description: "Left accent rail bar with clean compact task list",
        preview: &[
            "▎ ◆ TODO · 1/3 Completed · [Phase 142]",
            "▎   ✓ T1: Scaffolding and project structure setup",
            "▎   > T2: Form validation logic and interactive UI",
            "▎   ○ T3: End-to-end verification and documentation",
        ],
    },
    TodoStyleOption {
        id: "minimal",
        name: "Minimalist Rule",
        description: "Minimalist rule header with indented step list",
        preview: &[
            "◆ TODO · 1/3 Completed ───────────────────────────── [Phase 142]",
            "   ✓ T1: Scaffolding and project structure setup",
            "   > T2: Form validation logic and interactive UI",
            "   ○ T3: End-to-end verification and documentation",
        ],
    },
    TodoStyleOption {
        id: "checklist",
        name: "Markdown Checklist",
        description: "Developer-native [x] / [>] / [ ] checkbox list",
        preview: &[
            "◆ CHECKLIST · 1/3 Done · [Phase 142]",
            "  [x] T1: Scaffolding and project structure setup",
            "  [>] T2: Form validation logic and interactive UI",
            "  [ ] T3: End-to-end verification and documentation",
        ],
    },
    TodoStyleOption {
        id: "pipeline",
        name: "Stepper Pipeline",
        description: "Connected ●── / ◈── / ○── stage nodes with vertical rail",
        preview: &[
            "◈ PIPELINE · Step 1/3 · Phase 142",
            "  ●── T1: Scaffolding and project structure setup",
            "  ◈── T2: Form validation logic and interactive UI",
            "  ○── T3: End-to-end verification and documentation",
        ],
    },
];

pub static SWARM_STYLE_OPTIONS: &[SwarmStyleOption] = &[
    SwarmStyleOption {
        id: "stylish",
        name: "Minimal + Stylish (Neo-Nordic)",
        description: "Thin borders, stylish accent badges, micro-meters (Default)",
        preview: &[
            "┌── ◆ SWARM FLIGHT DECK ───────── swarm_finance ── [01:24] ──┐",
            "│ ❯ t1_calc    [RUNNING] ━╾────── 65%  turn 3  14.2k tokens │",
            "│ ❯ t2_form    [RUNNING] ━╾──── 50%    turn 2  11.8k tokens │",
            "├── ◈ MESSAGE BUS ──────────────────────────────── 3 msgs ─┤",
            "│ 20:38:49 t2 ──▸ t1 [QUERY]    \"What return types?\"       │",
            "│ 20:39:04 t1 ──▸ t2 [CONTRACT] \"ROISummary, Dataclass\"     │",
            "└─ [Tab] Switch   [Space] Logs   [k] Kill   [q] Close ──────┘",
        ],
    },
    SwarmStyleOption {
        id: "gitgraph",
        name: "Minimal + GitGraph Pipeline",
        description: "Topological branch DAG nodes with inline comms",
        preview: &[
            "● [HEAD: main] Swarm Orchestrator (Wave 1: Active)",
            "├─┬─● [t1_calc] ◆ Running (42s) · patch_file(\"calc.py\")",
            "│ │ │  └──◈ [query_interface] t2 ──▸ t1: \"Return types?\"",
            "│ │ │  └──◈ [publish_contract] t1 ──▸ t2: \"ROISummary\"",
            "│ └─● [t2_form] ◆ Running (38s) · exec_cmd(\"pytest\")",
            "├───● [auto-merge] Wave 1 Synchronization Barrier",
            "└───○ [t3_tests] ◷ Pending (Wave 2) · QA Integration",
        ],
    },
    SwarmStyleOption {
        id: "modern",
        name: "Clean Modernist (Floating Cards)",
        description: "Rounded boxed cards with pill badges and status dots",
        preview: &[
            "╭─ [Worker 1] t1_calc ────────╮ ╭─ [Worker 2] t2_form ────────╮",
            "│ ◆ RUNNING  Turn 3/5         │ │ ◆ RUNNING  Turn 2/5         │",
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
            "├─ ◆ t1_calculator_core  (Financial Calculator)",
            "│    tool › patch_file(\"calculator.py\")",
            "├─ ◆ t2_formatter_core   (Financial Formatter)",
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
            "│ ▸ [t1] Calc Engine  │ 20:38:49 t2 ──? t1 │ 12 | @dataclass     │",
            "│   Tool: patch_file  │ \"Need schema\"      │ 13 | class Amort:   │",
            "│   Dur: 42s · 14.2k  │ 20:39:04 t1 ──! t2 │ 14 |   principal    │",
            "│ ▸ [t2] Formatter    │ \"ROISummary\"       │ [test] passed       │",
            "└─────────────────────┴────────────────────┴─────────────────────┘",
        ],
    },
];

pub const TOAST_STYLE_OPTIONS: &[ToastStyleOption] = &[
    ToastStyleOption {
        id: "opencode",
        name: "OpenCode Split-Bar (Default)",
        description: "Thick left & right vertical accent bars (┃…┃) with breathing room and zero horizontal borders",
        preview: &[
            "┃                                          ┃",
            "┃  ✓ Subagent Completed                27s ┃",
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
            "│  ✓ Subagent Completed                27s │",
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
            "│  ✓ Subagent Completed                27s │",
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
            "│  ✓ Subagent Completed                27s │",
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
            "┃  ✓ Subagent Completed                27s  ",
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
            "│  ✓ Subagent Completed                27s  ",
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
            "│  ✓ Subagent Completed                27s  ",
            "│    \"Security Auditor\" finished            ",
            "└───────────────────────────────────────────",
        ],
    },
    ToastStyleOption {
        id: "l_round",
        name: "Asymmetric L-Frame (Rounded)",
        description: "Open top and right — vertical left spine meeting a full bottom baseline at a rounded corner (╰────)",
        preview: &[
            "│  ✓ Subagent Completed                27s  ",
            "│    \"Security Auditor\" finished            ",
            "╰───────────────────────────────────────────",
        ],
    },
    ToastStyleOption {
        id: "l_square",
        name: "Asymmetric L-Frame (Square)",
        description: "Open top and right — vertical left spine meeting a full bottom baseline at a sharp corner (└────)",
        preview: &[
            "│  ✓ Subagent Completed                27s  ",
            "│    \"Security Auditor\" finished            ",
            "└───────────────────────────────────────────",
        ],
    },
];

pub static ACTIVITY_STYLE_OPTIONS: &[ActivityStyleOption] = &[
    ActivityStyleOption {
        id: "action_detail",
        name: "Style 1: Action | Tool · Target (Default)",
        description: "Clean Thinking..., then Action... | tool_name · target during work",
        preview: &[
            "▰▱ Thinking... (4.2s · esc to cancel)",
            "▰▱ Writing... | write_file · src/components/Hero.tsx (18.4s · esc to cancel)",
            "▰▱ Editing... | patch_file · src/ui/view.rs (9.2s · esc to cancel)",
            "▰▱ Running... | exec_cmd · cargo check -j 1 (24.0s · esc to cancel)",
        ],
    },
    ActivityStyleOption {
        id: "action_tool",
        name: "Style 2: Action | Tool Name (Minimal)",
        description: "Ultra-minimal: Action... | tool_name without file or command paths",
        preview: &[
            "▰▱ Thinking... (4.2s · esc to cancel)",
            "▰▱ Writing... | write_file (18.4s · esc to cancel)",
            "▰▱ Editing... | patch_file (9.2s · esc to cancel)",
            "▰▱ Running... | exec_cmd (24.0s · esc to cancel)",
        ],
    },
    ActivityStyleOption {
        id: "hermes_trail",
        name: "Style 3: Hermes State + Action Trail",
        description: "Working... | <action target> + Thinking... | after <tool> · step N",
        preview: &[
            "▰▱ Thinking... (4.2s · esc to cancel)",
            "▰▱ Working... | writing src/components/Hero.tsx (18.4s · esc to cancel)",
            "▰▱ Working... | running cargo check -j 1 (24.0s · esc to cancel)",
            "▰▱ Thinking... | after write_file · step 2 (26.1s · esc to cancel)",
        ],
    },
    ActivityStyleOption {
        id: "classic",
        name: "Classic (Original UI)",
        description: "Single shimmering phase label without tool pipe (Generating...)",
        preview: &[
            "▰▱ Thinking... (4.2s • esc to interrupt)",
            "▰▱ Editing files... (18.4s • esc to interrupt)",
            "▰▱ Running command... (24.0s • esc to interrupt)",
        ],
    },
    ActivityStyleOption {
        id: "emoji_detail",
        name: "Emoji + Style 1 (Action | Emoji Tool · Target)",
        description: "Clean Thinking..., then Action... | <emoji> tool_name · target after pipe",
        preview: &[
            "▰▱ Thinking... (4.2s · esc to cancel)",
            "▰▱ Writing... | 📝  write_file · src/components/Hero.tsx (18.4s · esc to cancel)",
            "▰▱ Editing... | 🔧  patch_file · src/ui/view.rs (9.2s · esc to cancel)",
            "▰▱ Running... | 🚀  exec_cmd · cargo check -j 1 (24.0s · esc to cancel)",
        ],
    },
    ActivityStyleOption {
        id: "emoji_tool",
        name: "Emoji + Style 2 (Action | Emoji Tool Name)",
        description: "Clean Thinking..., then Action... | <emoji> tool_name after pipe",
        preview: &[
            "▰▱ Thinking... (4.2s · esc to cancel)",
            "▰▱ Writing... | 📝  write_file (18.4s · esc to cancel)",
            "▰▱ Editing... | 🔧  patch_file (9.2s · esc to cancel)",
            "▰▱ Running... | 🚀  exec_cmd (24.0s · esc to cancel)",
        ],
    },
    ActivityStyleOption {
        id: "emoji_trail",
        name: "Emoji + Style 3 (Hermes Trail after |)",
        description: "Clean Thinking..., then Working... | <emoji> <action target> after pipe",
        preview: &[
            "▰▱ Thinking... (4.2s · esc to cancel)",
            "▰▱ Working... | 📝  writing src/components/Hero.tsx (18.4s · esc to cancel)",
            "▰▱ Working... | 🚀  running cargo check -j 1 (24.0s · esc to cancel)",
            "▰▱ Thinking... | 💭  after write_file · step 2 (26.1s · esc to cancel)",
        ],
    },
    ActivityStyleOption {
        id: "kaomoji_detail",
        name: "Kaomoji + Style 1 (Action | Kaomoji Tool · Target)",
        description: "Clean Thinking..., then Action... | <kaomoji> tool_name · target after pipe",
        preview: &[
            "▰▱ Thinking... (4.2s · esc to cancel)",
            "▰▱ Writing... | ( ▸_▸)◈  write_file · src/components/Hero.tsx (18.4s · esc to cancel)",
            "▰▱ Editing... | (⌐▪_▪)  patch_file · src/ui/view.rs (9.2s · esc to cancel)",
            "▰▱ Running... | (ง°_°)ง  exec_cmd · cargo check -j 1 (24.0s · esc to cancel)",
        ],
    },
    ActivityStyleOption {
        id: "kaomoji_tool",
        name: "Kaomoji + Style 2 (Action | Kaomoji Tool Name)",
        description: "Clean Thinking..., then Action... | <kaomoji> tool_name after pipe",
        preview: &[
            "▰▱ Thinking... (4.2s · esc to cancel)",
            "▰▱ Writing... | ( ▸_▸)◈  write_file (18.4s · esc to cancel)",
            "▰▱ Editing... | (⌐▪_▪)  patch_file (9.2s · esc to cancel)",
            "▰▱ Running... | (ง°_°)ง  exec_cmd (24.0s · esc to cancel)",
        ],
    },
    ActivityStyleOption {
        id: "kaomoji_trail",
        name: "Kaomoji + Style 3 (Hermes Trail after |)",
        description: "Clean Thinking..., then Working... | <kaomoji> <action target> after pipe",
        preview: &[
            "▰▱ Thinking... (4.2s · esc to cancel)",
            "▰▱ Working... | ( ▸_▸)◈  writing src/components/Hero.tsx (18.4s · esc to cancel)",
            "▰▱ Working... | (ง°_°)ง  running cargo check -j 1 (24.0s · esc to cancel)",
            "▰▱ Thinking... | ( ._.)  after write_file · step 2 (26.1s · esc to cancel)",
        ],
    },
    ActivityStyleOption {
        id: "compact_badge",
        name: "Style 4: Compact Pill Badge ([tool] target)",
        description: "Bracketed tool pill badge after pipe: Action... | [tool_name] target",
        preview: &[
            "▰▱ Thinking... (4.2s · esc to cancel)",
            "▰▱ Writing... | [write_file] src/components/Hero.tsx (18.4s · esc to cancel)",
            "▰▱ Editing... | [patch_file] src/ui/view.rs (9.2s · esc to cancel)",
            "▰▱ Running... | [exec_cmd] cargo check -j 1 (24.0s · esc to cancel)",
        ],
    },
    ActivityStyleOption {
        id: "cyber_arrow",
        name: "Style 5: Cyber Telemetry Arrow (▸ tool ──▸ target)",
        description:
            "Directional pipeline telemetry after pipe: Action... | ▸ tool_name ──▸ target",
        preview: &[
            "▰▱ Thinking... (4.2s · esc to cancel)",
            "▰▱ Writing... | ▸ write_file ──▸ src/components/Hero.tsx (18.4s · esc to cancel)",
            "▰▱ Editing... | ▸ patch_file ──▸ src/ui/view.rs (9.2s · esc to cancel)",
            "▰▱ Running... | ▸ exec_cmd ──▸ cargo check -j 1 (24.0s · esc to cancel)",
        ],
    },
];

pub static TOAST_SYMBOL_OPTIONS: &[ToastSymbolOption] = &[
    ToastSymbolOption {
        id: "ascii",
        name: "1. Clean ASCII (`@` `#` `*` `✓`)",
        description:
            "Strict 1-cell ASCII / minimal operator glyphs for zero cell-width drift (Default)",
        preview: &[
            "* Skill Loaded       · `frontend-design`                  Loaded",
            "@ MiniPub Spawned    · worker_1: Audit auth flow          Branch",
            "# Plan Step Complete · Next: \"HeroSection.tsx\"            2/8 · 25%",
            "✓ Turn Completed     · Finished in 4.2s (3 tools)         4.2s",
        ],
    },
    ToastSymbolOption {
        id: "geometric",
        name: "2. Geometric Unicode (`◉` `◆` `✦` `◈`)",
        description: "Crisp geometric Unicode indicators for modern nerd/monospace fonts",
        preview: &[
            "✦ Skill Loaded       · `frontend-design`                  Loaded",
            "◉ MiniPub Spawned    · worker_1: Audit auth flow          Branch",
            "◆ Plan Step Complete · Next: \"HeroSection.tsx\"            2/8 · 25%",
            "✓ Turn Completed     · Finished in 4.2s (3 tools)         4.2s",
        ],
    },
    ToastSymbolOption {
        id: "emoji",
        name: "3. Expressive Emoji (`🧠` `🤖` `📋` `✅`)",
        description: "Full-color status emojis for skills, subagents, tasks, and alerts",
        preview: &[
            "🧠 Skill Loaded       · `frontend-design`                  Loaded",
            "🤖 MiniPub Spawned    · worker_1: Audit auth flow          Branch",
            "📋 Plan Step Complete · Next: \"HeroSection.tsx\"            2/8 · 25%",
            "✅ Turn Completed     · Finished in 4.2s (3 tools)         4.2s",
        ],
    },
    ToastSymbolOption {
        id: "kaomoji",
        name: "4. Kaomoji Badges (`(✧_✧)` `(◉_◉)` `(ง•_•)ง`)",
        description: "Expressive mini Japanese kaomoji faces for each toast category",
        preview: &[
            "(✧_✧) Skill Loaded       · `frontend-design`              Loaded",
            "(◉_◉) MiniPub Spawned    · worker_1: Audit auth flow      Branch",
            "(ง•_•)ง Plan Step Complete · Next: \"HeroSection.tsx\"      2/8 · 25%",
            "(•‿•) Turn Completed     · Finished in 4.2s (3 tools)     4.2s",
        ],
    },
    ToastSymbolOption {
        id: "cyber",
        name: "5. Cyber / Nerd (`✧` `❖` `⚙` `⚡`)",
        description: "High-contrast technical & directional operator symbols",
        preview: &[
            "✧ Skill Loaded       · `frontend-design`                  Loaded",
            "❖ MiniPub Spawned    · worker_1: Audit auth flow          Branch",
            "⚙ Plan Step Complete · Next: \"HeroSection.tsx\"            2/8 · 25%",
            "✔ Turn Completed     · Finished in 4.2s (3 tools)         4.2s",
        ],
    },
    ToastSymbolOption {
        id: "bracket",
        name: "6. Bracketed Tags (`[SKL]` `[BOT]` `[TSK]` `[OK]`)",
        description: "Structured CLI badge tags for ultra-clear semantic categorization",
        preview: &[
            "[SKL] Skill Loaded       · `frontend-design`              Loaded",
            "[BOT] MiniPub Spawned    · worker_1: Audit auth flow      Branch",
            "[TSK] Plan Step Complete · Next: \"HeroSection.tsx\"        2/8 · 25%",
            "[OK]  Turn Completed     · Finished in 4.2s (3 tools)     4.2s",
        ],
    },
];

pub static MENU_STYLE_OPTIONS: &[MenuStyleOption] = &[
    MenuStyleOption {
        id: crate::ui::intellisense::MENU_STYLE_SPECS[0].id,
        name: crate::ui::intellisense::MENU_STYLE_SPECS[0].name,
        description: crate::ui::intellisense::MENU_STYLE_SPECS[0].description,
        preview: crate::ui::intellisense::MENU_STYLE_SPECS[0].preview,
    },
    MenuStyleOption {
        id: crate::ui::intellisense::MENU_STYLE_SPECS[1].id,
        name: crate::ui::intellisense::MENU_STYLE_SPECS[1].name,
        description: crate::ui::intellisense::MENU_STYLE_SPECS[1].description,
        preview: crate::ui::intellisense::MENU_STYLE_SPECS[1].preview,
    },
    MenuStyleOption {
        id: crate::ui::intellisense::MENU_STYLE_SPECS[2].id,
        name: crate::ui::intellisense::MENU_STYLE_SPECS[2].name,
        description: crate::ui::intellisense::MENU_STYLE_SPECS[2].description,
        preview: crate::ui::intellisense::MENU_STYLE_SPECS[2].preview,
    },
    MenuStyleOption {
        id: crate::ui::intellisense::MENU_STYLE_SPECS[3].id,
        name: crate::ui::intellisense::MENU_STYLE_SPECS[3].name,
        description: crate::ui::intellisense::MENU_STYLE_SPECS[3].description,
        preview: crate::ui::intellisense::MENU_STYLE_SPECS[3].preview,
    },
    MenuStyleOption {
        id: crate::ui::intellisense::MENU_STYLE_SPECS[4].id,
        name: crate::ui::intellisense::MENU_STYLE_SPECS[4].name,
        description: crate::ui::intellisense::MENU_STYLE_SPECS[4].description,
        preview: crate::ui::intellisense::MENU_STYLE_SPECS[4].preview,
    },
    MenuStyleOption {
        id: crate::ui::intellisense::MENU_STYLE_SPECS[5].id,
        name: crate::ui::intellisense::MENU_STYLE_SPECS[5].name,
        description: crate::ui::intellisense::MENU_STYLE_SPECS[5].description,
        preview: crate::ui::intellisense::MENU_STYLE_SPECS[5].preview,
    },
];

pub struct ThemeSelectContext<'a> {
    pub themes: &'a [ThemeInfo],
    pub animations: &'a [crate::ui::animation::AnimationOption],
    pub todo_styles: &'a [TodoStyleOption],
    pub swarm_styles: &'a [SwarmStyleOption],
    pub toast_styles: &'a [ToastStyleOption],
    pub toast_symbols: &'a [ToastSymbolOption],
    pub activity_styles: &'a [ActivityStyleOption],
    pub menu_styles: &'a [MenuStyleOption],
    pub active_tab: crate::ui::modals::ThemeModalTab,
    pub theme_selected_index: usize,
    pub animation_selected_index: usize,
    pub todo_style_selected_index: usize,
    pub swarm_style_selected_index: usize,
    pub toast_style_selected_index: usize,
    pub toast_symbol_selected_index: usize,
    pub activity_style_selected_index: usize,
    pub menu_style_selected_index: usize,
    pub active_theme_id: &'a str,
    pub active_animation_id: &'a str,
    pub active_todo_style: &'a str,
    pub active_swarm_style: &'a str,
    pub active_toast_style: &'a str,
    pub active_toast_symbols: &'a str,
    pub active_activity_style: &'a str,
    pub active_menu_style: &'a str,
}

/// Renders the `/theme` customization workbench using the same Left Sidebar + Right Main Panel
/// architecture as `/settings` (`render_settings`), with a compact scrollable options table
/// and a dedicated bottom Live Preview panel so options are never clipped horizontally or vertically.
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
        toast_symbols,
        activity_styles,
        menu_styles,
        active_tab,
        theme_selected_index,
        animation_selected_index,
        todo_style_selected_index,
        swarm_style_selected_index,
        toast_style_selected_index,
        toast_symbol_selected_index,
        activity_style_selected_index,
        menu_style_selected_index,
        active_theme_id,
        active_animation_id,
        active_todo_style,
        active_swarm_style,
        active_toast_style,
        active_toast_symbols,
        active_activity_style,
        active_menu_style,
    } = *ctx;

    let popup_area = area;
    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(Line::from(vec![
            Span::styled(
                " MiniCode Appearance & Themes ",
                Style::default()
                    .fg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(
                    "── {} / {} ── [menu: {}] ",
                    active_theme_id, active_animation_id, active_menu_style
                ),
                Style::default().fg(theme.muted),
            ),
        ]))
        .title_alignment(Alignment::Left)
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(Style::default().fg(theme.border).bg(theme.bg_elevated))
        .style(Style::default().bg(theme.bg_elevated));

    let inner_area = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let outer_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(8), // 0: Main Split Body (Left Menu Bar | Right Customization Pane)
            Constraint::Length(1), // 1: Bottom Divider
            Constraint::Length(1), // 2: Footer Key Hints
        ])
        .split(inner_area);

    let body_area = outer_chunks[0];
    let sidebar_w = if body_area.width >= 90 {
        24
    } else if body_area.width >= 68 {
        20
    } else {
        16.min(body_area.width / 3)
    };

    let main_split = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(sidebar_w), // 0: Left Menu Bar (Minimal Go-style Navigation)
            Constraint::Length(1),         // 1: Vertical Separator │
            Constraint::Min(20),           // 2: Right Main Panel
        ])
        .split(body_area);

    let sidebar_area = main_split[0];
    let sep_area = main_split[1];
    let right_area = main_split[2];

    // 1. Render Left Menu Bar (Minimal Go/Huh style: › active item, clean ACTIVE PROFILE summary below)
    let mut sidebar_lines: Vec<Line> = Vec::new();
    sidebar_lines.push(Line::from(""));
    sidebar_lines.push(Line::from(vec![Span::styled(
        "  APPEARANCE",
        Style::default()
            .fg(theme.muted)
            .add_modifier(Modifier::BOLD),
    )]));
    sidebar_lines.push(Line::from(""));

    let sb_inner_w = (sidebar_area.width as usize).saturating_sub(2).max(8);
    for tab in crate::ui::modals::ThemeModalTab::all() {
        let is_active = *tab == active_tab;
        let raw_label = tab.sidebar_label();
        let prefix = if is_active { "  › " } else { "    " };
        let style = if is_active {
            Style::default()
                .fg(theme.brand_accent)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.muted)
        };
        sidebar_lines.push(Line::from(vec![
            Span::styled(prefix, style),
            Span::styled(raw_label, style),
        ]));
    }

    // Add minimal Active Profile summary card in Left Sidebar if height permits
    if body_area.height >= 20 && sidebar_area.width >= 18 {
        let val_max_w = (sidebar_area.width as usize).saturating_sub(10).max(6);
        sidebar_lines.push(Line::from(""));
        sidebar_lines.push(Line::from(vec![Span::styled(
            format!("  {}", "─".repeat(sb_inner_w.saturating_sub(2))),
            Style::default().fg(theme.border),
        )]));
        sidebar_lines.push(Line::from(vec![Span::styled(
            "  ACTIVE PROFILE",
            Style::default()
                .fg(theme.muted)
                .add_modifier(Modifier::BOLD),
        )]));
        for (lbl, val) in [
            ("  theme ", active_theme_id),
            ("  anim  ", active_animation_id),
            ("  todo  ", active_todo_style),
            ("  swarm ", active_swarm_style),
            ("  toast ", active_toast_style),
            ("  sym   ", active_toast_symbols),
            ("  act   ", active_activity_style),
            ("  menu  ", active_menu_style),
        ] {
            sidebar_lines.push(Line::from(vec![
                Span::styled(lbl, Style::default().fg(theme.muted)),
                Span::styled(
                    truncate_str(val, val_max_w),
                    Style::default().fg(theme.text_primary),
                ),
            ]));
        }
    }

    frame.render_widget(Paragraph::new(sidebar_lines), sidebar_area);

    // 2. Render Vertical Separator │ between Left Menu Bar and Right Panel
    let sep_lines: Vec<Line> = (0..sep_area.height)
        .map(|_| Line::from(vec![Span::styled("│", Style::default().fg(theme.border))]))
        .collect();
    frame.render_widget(Paragraph::new(sep_lines), sep_area);

    // 3. Render Right Main Panel for the Active Tab
    let (total_items, selected_idx) = match active_tab {
        crate::ui::modals::ThemeModalTab::Themes => (themes.len(), theme_selected_index),
        crate::ui::modals::ThemeModalTab::Animations => {
            (animations.len(), animation_selected_index)
        }
        crate::ui::modals::ThemeModalTab::TodoStyles => {
            (todo_styles.len(), todo_style_selected_index)
        }
        crate::ui::modals::ThemeModalTab::SwarmStyles => {
            (swarm_styles.len(), swarm_style_selected_index)
        }
        crate::ui::modals::ThemeModalTab::ToastStyles => {
            (toast_styles.len(), toast_style_selected_index)
        }
        crate::ui::modals::ThemeModalTab::ToastSymbols => {
            (toast_symbols.len(), toast_symbol_selected_index)
        }
        crate::ui::modals::ThemeModalTab::ActivityStyles => {
            (activity_styles.len(), activity_style_selected_index)
        }
        crate::ui::modals::ThemeModalTab::MenuStyles => {
            (menu_styles.len(), menu_style_selected_index)
        }
    };

    let mut content_lines: Vec<Line<'static>> = Vec::new();
    let (sec_title, sec_subtitle) = active_tab.section_header();
    let right_w = right_area.width as usize;

    content_lines.push(Line::from(""));
    content_lines.push(Line::from(vec![
        Span::raw("  "),
        Span::styled(
            sec_title.to_string(),
            Style::default()
                .fg(theme.text_primary)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(
                "  ({}/{})",
                if total_items == 0 {
                    0
                } else {
                    selected_idx.min(total_items - 1) + 1
                },
                total_items
            ),
            Style::default().fg(theme.brand_accent),
        ),
    ]));
    content_lines.push(Line::from(vec![
        Span::raw("  "),
        Span::styled(
            truncate_str(sec_subtitle, right_w.saturating_sub(4).max(20)),
            Style::default().fg(theme.muted),
        ),
    ]));
    content_lines.push(Line::from(vec![
        Span::raw("  "),
        Span::styled(
            "─".repeat(right_w.saturating_sub(4).max(10)),
            Style::default().fg(theme.border),
        ),
    ]));

    // Reserve 9 lines at the bottom of the right pane for the dedicated Live Preview box,
    // and 5 lines at the top for the section header, giving a roomy scrollable options table in the middle.
    let list_max_visible = (right_area.height as usize).saturating_sub(14).clamp(5, 14);
    let mut preview_title = String::new();
    let mut preview_lines: Vec<Line<'static>> = Vec::new();

    match active_tab {
        crate::ui::modals::ThemeModalTab::Themes => {
            let scroll_offset = compute_scroll_offset(theme_selected_index, list_max_visible);
            let visible_themes = themes.iter().skip(scroll_offset).take(list_max_visible);
            let desc_max_w = right_w.saturating_sub(THEME_NAME_DISPLAY_COLS + 32).max(16);

            for (idx_rel, t) in visible_themes.enumerate() {
                let idx = scroll_offset + idx_rel;
                let is_selected = idx == theme_selected_index;
                let is_active = t.id == active_theme_id
                    || (t.id == "aura" && matches!(active_theme_id, "auto" | "default" | "dark"));

                let cursor = if is_selected { "  › " } else { "    " };
                let title_style = if is_selected {
                    Style::default()
                        .fg(theme.brand_accent)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(theme.text_primary)
                };

                let mut row_spans = vec![
                    Span::styled(
                        cursor,
                        Style::default()
                            .fg(theme.brand_accent)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!("[{:>2}] ", idx + 1),
                        Style::default().fg(theme.muted),
                    ),
                    Span::styled(
                        format!(
                            "{:<width$}",
                            truncate_str(&t.name, THEME_NAME_DISPLAY_COLS),
                            width = THEME_NAME_DISPLAY_COLS
                        ),
                        title_style,
                    ),
                ];

                for c in &t.swatches {
                    row_spans.push(Span::styled(" ■", Style::default().fg(*c)));
                }

                if is_active {
                    row_spans.push(Span::styled(
                        "  [active] ",
                        Style::default()
                            .fg(theme.success)
                            .add_modifier(Modifier::BOLD),
                    ));
                } else {
                    row_spans.push(Span::raw("           "));
                }

                row_spans.push(Span::styled(
                    truncate_str(&t.description, desc_max_w),
                    if is_selected {
                        Style::default().fg(theme.text_primary)
                    } else {
                        Style::default().fg(theme.muted)
                    },
                ));

                content_lines.push(Line::from(row_spans));
            }

            if let Some(sel_theme) = themes.get(theme_selected_index) {
                preview_title = format!("{} ({})", sel_theme.name, sel_theme.id);
                let mut swatch_spans = vec![Span::styled(
                    "    Palette Swatches : ",
                    Style::default().fg(theme.muted),
                )];
                for c in &sel_theme.swatches {
                    swatch_spans.push(Span::styled("■■■ ", Style::default().fg(*c)));
                }
                preview_lines.push(Line::from(swatch_spans));
                preview_lines.push(Line::from(vec![
                    Span::styled("    Description      : ", Style::default().fg(theme.muted)),
                    Span::styled(
                        sel_theme.description.clone(),
                        Style::default().fg(theme.text_primary),
                    ),
                ]));
                preview_lines.push(Line::from(vec![
                    Span::styled("    Sample Prompt    : ", Style::default().fg(theme.muted)),
                    Span::styled(
                        "› ",
                        Style::default()
                            .fg(theme.brand_accent)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled("refactor ", Style::default().fg(theme.text_primary)),
                    Span::styled(
                        "@src/ui/view.rs ",
                        Style::default().fg(theme.info).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled("✔ Ready", Style::default().fg(theme.success)),
                ]));
            }
        }
        crate::ui::modals::ThemeModalTab::Animations => {
            let scroll_offset = compute_scroll_offset(animation_selected_index, list_max_visible);
            let visible_anims = animations.iter().skip(scroll_offset).take(list_max_visible);

            let millis = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
            let desc_max_w = right_w.saturating_sub(58).max(16);

            for (idx_rel, anim) in visible_anims.enumerate() {
                let idx = scroll_offset + idx_rel;
                let is_selected = idx == animation_selected_index;
                let is_active = anim.id == active_animation_id;

                let cursor = if is_selected { "  › " } else { "    " };
                let title_style = if is_selected {
                    Style::default()
                        .fg(theme.brand_accent)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(theme.text_primary)
                };

                let mut row_spans = vec![
                    Span::styled(
                        cursor,
                        Style::default()
                            .fg(theme.brand_accent)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!("[{:>2}] ", idx + 1),
                        Style::default().fg(theme.muted),
                    ),
                    Span::styled(format!("{:<24}", truncate_str(anim.name, 24)), title_style),
                ];

                let preview_spans =
                    crate::ui::animation::render_preview_spinner(anim.style, millis, theme);
                row_spans.extend(preview_spans);

                if is_active {
                    row_spans.push(Span::styled(
                        "  [active] ",
                        Style::default()
                            .fg(theme.success)
                            .add_modifier(Modifier::BOLD),
                    ));
                } else {
                    row_spans.push(Span::raw("           "));
                }

                row_spans.push(Span::styled(
                    truncate_str(anim.description, desc_max_w),
                    if is_selected {
                        Style::default().fg(theme.text_primary)
                    } else {
                        Style::default().fg(theme.muted)
                    },
                ));

                content_lines.push(Line::from(row_spans));
            }

            if let Some(sel_anim) = animations.get(animation_selected_index) {
                preview_title = format!("{} ({})", sel_anim.name, sel_anim.id);
                let act_style =
                    crate::config::ActivityBarStyle::from_str_loose(active_activity_style);
                for (act, elapsed) in [
                    (crate::ui::animation::AgentActivity::Thinking, 4.2),
                    (
                        crate::ui::animation::AgentActivity::WritingFile {
                            tool: "write_file".to_string(),
                            path: "src/ui/modals/theme_select.rs".to_string(),
                        },
                        12.8,
                    ),
                    (
                        crate::ui::animation::AgentActivity::Debugging {
                            step: "cargo test -j 1".to_string(),
                        },
                        19.5,
                    ),
                ] {
                    let line = crate::ui::animation::render_live_activity_line_with_style(
                        &act,
                        sel_anim.style,
                        act_style,
                        millis,
                        elapsed,
                        theme,
                    );
                    let mut spans = vec![Span::raw("    ")];
                    spans.extend(line.spans);
                    preview_lines.push(Line::from(spans));
                }
            }
        }
        crate::ui::modals::ThemeModalTab::TodoStyles => {
            let demo_plan = crate::ui::view::LivePlanBlock {
                title: "Phase 142".to_string(),
                total_tasks: 3,
                completed_tasks: 1,
                active_task: Some("T2: Form validation logic and interactive UI".to_string()),
                tasks: vec![
                    crate::ui::view::LivePlanTaskItem {
                        title: "T1: Scaffolding and project structure setup".to_string(),
                        status: crate::ui::view::LivePlanTaskStatus::Completed,
                        sub_tasks: Vec::new(),
                    },
                    crate::ui::view::LivePlanTaskItem {
                        title: "T2: Form validation logic and interactive UI".to_string(),
                        status: crate::ui::view::LivePlanTaskStatus::InProgress,
                        sub_tasks: Vec::new(),
                    },
                    crate::ui::view::LivePlanTaskItem {
                        title: "T3: End-to-end verification and documentation".to_string(),
                        status: crate::ui::view::LivePlanTaskStatus::Pending,
                        sub_tasks: Vec::new(),
                    },
                ],
            };
            let preview_width = right_area.width.saturating_sub(8).max(44);
            let live_todo_preview = |style_id: &str| -> Vec<Line<'static>> {
                let mut cfg = crate::config::UiConfig::default();
                cfg.todo_style = style_id.to_string();
                let style_enum = cfg.todo_style_enum();
                crate::ui::todo_widget::render_todo_widget(
                    &demo_plan,
                    style_enum,
                    preview_width,
                    theme,
                )
                .into_iter()
                .skip(1)
                .map(|line| {
                    Line::from(
                        line.spans
                            .into_iter()
                            .map(|s| Span::styled(s.content.into_owned(), s.style))
                            .collect::<Vec<_>>(),
                    )
                })
                .collect()
            };
            render_style_preview_list(
                &mut content_lines,
                &mut preview_title,
                &mut preview_lines,
                todo_styles,
                todo_style_selected_index,
                active_todo_style,
                24,
                list_max_visible,
                right_w,
                theme,
                Some(&live_todo_preview),
            );
        }
        crate::ui::modals::ThemeModalTab::SwarmStyles => {
            render_style_preview_list(
                &mut content_lines,
                &mut preview_title,
                &mut preview_lines,
                swarm_styles,
                swarm_style_selected_index,
                active_swarm_style,
                30,
                list_max_visible,
                right_w,
                theme,
                None,
            );
        }
        crate::ui::modals::ThemeModalTab::ToastStyles => {
            render_style_preview_list(
                &mut content_lines,
                &mut preview_title,
                &mut preview_lines,
                toast_styles,
                toast_style_selected_index,
                active_toast_style,
                34,
                list_max_visible,
                right_w,
                theme,
                None,
            );
        }
        crate::ui::modals::ThemeModalTab::ToastSymbols => {
            let demo_rows = [
                (
                    crate::ui::ToastKind::Skill,
                    "Skill Loaded      ",
                    "`frontend-design` — UI layout guidelines",
                    "Loaded",
                ),
                (
                    crate::ui::ToastKind::Subagent,
                    "MiniPub Spawned   ",
                    "worker_1: Audit authentication flow",
                    "Branch",
                ),
                (
                    crate::ui::ToastKind::Task,
                    "Plan Step Complete",
                    "Next: \"HeroSection.tsx\"",
                    "2/8 · 25%",
                ),
                (
                    crate::ui::ToastKind::Success,
                    "Turn Completed    ",
                    "Finished in 4.2s (3 tools invoked)",
                    "4.2s",
                ),
            ];
            let live_symbol_preview = |symbol_id: &str| -> Vec<Line<'static>> {
                let sym_theme = crate::config::ToastSymbolTheme::from_str_loose(symbol_id);
                demo_rows
                    .iter()
                    .map(|(kind, title, msg, badge)| {
                        let accent = kind.accent_color(theme);
                        let glyph = kind.glyph_for(sym_theme);
                        Line::from(vec![
                            Span::styled("┃ ", Style::default().fg(accent)),
                            Span::styled(
                                format!("{glyph} "),
                                Style::default().fg(accent).add_modifier(Modifier::BOLD),
                            ),
                            Span::styled(
                                (*title).to_string(),
                                Style::default()
                                    .fg(theme.text_primary)
                                    .add_modifier(Modifier::BOLD),
                            ),
                            Span::styled(
                                format!(" · {:<40} ", msg),
                                Style::default().fg(theme.muted),
                            ),
                            Span::styled((*badge).to_string(), Style::default().fg(accent)),
                        ])
                    })
                    .collect()
            };
            render_style_preview_list(
                &mut content_lines,
                &mut preview_title,
                &mut preview_lines,
                toast_symbols,
                toast_symbol_selected_index,
                active_toast_symbols,
                36,
                list_max_visible,
                right_w,
                theme,
                Some(&live_symbol_preview),
            );
        }
        crate::ui::modals::ThemeModalTab::ActivityStyles => {
            let millis = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
            let active_spinner = crate::ui::animation::SpinnerStyle::from_id(active_animation_id);
            let demo_activities = [
                (crate::ui::animation::AgentActivity::Thinking, 4.2),
                (
                    crate::ui::animation::AgentActivity::WritingFile {
                        tool: "write_file".to_string(),
                        path: "src/components/Hero.tsx".to_string(),
                    },
                    18.4,
                ),
                (
                    crate::ui::animation::AgentActivity::EditingFile {
                        path: "src/ui/view.rs".to_string(),
                    },
                    9.2,
                ),
                (
                    crate::ui::animation::AgentActivity::Debugging {
                        step: "cargo check -j 1".to_string(),
                    },
                    24.0,
                ),
            ];
            let live_activity_preview = |style_id: &str| -> Vec<Line<'static>> {
                let style_enum = crate::config::ActivityBarStyle::from_str_loose(style_id);
                demo_activities
                    .iter()
                    .map(|(act, elapsed)| {
                        crate::ui::animation::render_live_activity_line_with_style(
                            act,
                            active_spinner,
                            style_enum,
                            millis,
                            *elapsed,
                            theme,
                        )
                    })
                    .collect()
            };
            render_style_preview_list(
                &mut content_lines,
                &mut preview_title,
                &mut preview_lines,
                activity_styles,
                activity_style_selected_index,
                active_activity_style,
                36,
                list_max_visible,
                right_w,
                theme,
                Some(&live_activity_preview),
            );
        }
        crate::ui::modals::ThemeModalTab::MenuStyles => {
            render_style_preview_list(
                &mut content_lines,
                &mut preview_title,
                &mut preview_lines,
                menu_styles,
                menu_style_selected_index,
                active_menu_style,
                36,
                list_max_visible,
                right_w,
                theme,
                None,
            );
        }
    }

    // Dedicated Live Preview section below the compact options table
    if !preview_lines.is_empty() {
        content_lines.push(Line::from(""));
        let header_prefix = format!("  ── Live Preview: {} ", preview_title);
        let rule_rem = right_w
            .saturating_sub(header_prefix.chars().count() + 2)
            .max(4);
        content_lines.push(Line::from(vec![
            Span::styled(
                header_prefix,
                Style::default()
                    .fg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("─".repeat(rule_rem), Style::default().fg(theme.border)),
        ]));
        content_lines.extend(preview_lines);
    }

    frame.render_widget(Paragraph::new(content_lines), right_area);

    // 4. Bottom Divider above Footer
    let bot_divider = Paragraph::new(Line::from(vec![Span::styled(
        "─".repeat(inner_area.width as usize),
        Style::default().fg(theme.border),
    )]));
    frame.render_widget(bot_divider, outer_chunks[1]);

    // 5. Minimal Go-style Footer Key Hints (matching /settings)
    let footer_line = Line::from(vec![
        Span::styled("  tab/←→/1-8 ", Style::default().fg(theme.brand_accent)),
        Span::styled("section   ", Style::default().fg(theme.muted)),
        Span::styled("↑/↓/j/k ", Style::default().fg(theme.brand_accent)),
        Span::styled("navigate   ", Style::default().fg(theme.muted)),
        Span::styled("enter/space ", Style::default().fg(theme.brand_accent)),
        Span::styled("apply style   ", Style::default().fg(theme.muted)),
        Span::styled("esc ", Style::default().fg(theme.brand_accent)),
        Span::styled("save & close", Style::default().fg(theme.muted)),
    ]);
    frame.render_widget(Paragraph::new(footer_line), outer_chunks[2]);
}

#[allow(clippy::too_many_arguments)]
fn render_style_preview_list<'a>(
    lines: &mut Vec<Line<'static>>,
    preview_title: &mut String,
    preview_lines: &mut Vec<Line<'static>>,
    options: &'a [StylePreviewOption],
    selected_index: usize,
    active_id: &str,
    name_width: usize,
    max_visible: usize,
    right_width: usize,
    theme: &Theme,
    live_renderer: Option<&dyn Fn(&str) -> Vec<Line<'static>>>,
) {
    let scroll_offset = compute_scroll_offset(selected_index, max_visible);
    let desc_max_w = right_width.saturating_sub(name_width + 24).max(16);

    for (idx_rel, opt) in options
        .iter()
        .skip(scroll_offset)
        .take(max_visible)
        .enumerate()
    {
        let idx = scroll_offset + idx_rel;
        let is_selected = idx == selected_index;
        let is_active = opt.id == active_id;

        let cursor = if is_selected { "  › " } else { "    " };
        let title_style = if is_selected {
            Style::default()
                .fg(theme.brand_accent)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.text_primary)
        };

        let mut row_spans = vec![
            Span::styled(
                cursor,
                Style::default()
                    .fg(theme.brand_accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("[{:>2}] ", idx + 1),
                Style::default().fg(theme.muted),
            ),
            Span::styled(
                format!(
                    "{:<width$}",
                    truncate_str(opt.name, name_width),
                    width = name_width
                ),
                title_style,
            ),
        ];

        if is_active {
            row_spans.push(Span::styled(
                " [active]  ",
                Style::default()
                    .fg(theme.success)
                    .add_modifier(Modifier::BOLD),
            ));
        } else {
            row_spans.push(Span::raw("           "));
        }

        row_spans.push(Span::styled(
            truncate_str(opt.description, desc_max_w),
            if is_selected {
                Style::default().fg(theme.text_primary)
            } else {
                Style::default().fg(theme.muted)
            },
        ));

        lines.push(Line::from(row_spans));
    }

    if let Some(sel_opt) = options.get(selected_index) {
        *preview_title = format!("{} ({})", sel_opt.name, sel_opt.id);
        match live_renderer {
            Some(render_fn) => {
                for rendered_line in render_fn(sel_opt.id) {
                    let mut spans = vec![Span::raw("    ")];
                    spans.extend(rendered_line.spans);
                    preview_lines.push(Line::from(spans));
                }
            }
            None => {
                for preview_line in sel_opt.preview {
                    preview_lines.push(Line::from(vec![
                        Span::raw("    "),
                        Span::styled((*preview_line).to_string(), Style::default().fg(theme.info)),
                    ]));
                }
            }
        }
    }
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
    fn test_toast_symbol_options_completeness() {
        assert_eq!(TOAST_SYMBOL_OPTIONS.len(), 6);
        let ids: Vec<&str> = TOAST_SYMBOL_OPTIONS.iter().map(|s| s.id).collect();
        assert!(ids.contains(&"ascii"));
        assert!(ids.contains(&"geometric"));
        assert!(ids.contains(&"emoji"));
        assert!(ids.contains(&"kaomoji"));
        assert!(ids.contains(&"cyber"));
        assert!(ids.contains(&"bracket"));
    }

    #[test]
    fn test_render_theme_select_swarm_and_toast_styles_tabs() {
        let backend = TestBackend::new(110, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        let theme = Theme::default();
        let themes = Theme::list_themes();
        let animations = crate::ui::animation::ANIMATION_OPTIONS.to_vec();

        for &tab in crate::ui::modals::ThemeModalTab::all() {
            let ctx = ThemeSelectContext {
                themes: &themes,
                animations: &animations,
                todo_styles: TODO_STYLE_OPTIONS,
                swarm_styles: SWARM_STYLE_OPTIONS,
                toast_styles: TOAST_STYLE_OPTIONS,
                toast_symbols: TOAST_SYMBOL_OPTIONS,
                activity_styles: ACTIVITY_STYLE_OPTIONS,
                menu_styles: MENU_STYLE_OPTIONS,
                active_tab: tab,
                theme_selected_index: 0,
                animation_selected_index: 0,
                todo_style_selected_index: 0,
                swarm_style_selected_index: 0,
                toast_style_selected_index: 0,
                toast_symbol_selected_index: 0,
                activity_style_selected_index: 0,
                menu_style_selected_index: 0,
                active_theme_id: "auto",
                active_animation_id: "dual_pillars",
                active_todo_style: "tree",
                active_swarm_style: "stylish",
                active_toast_style: "opencode",
                active_toast_symbols: "ascii",
                active_activity_style: "action_detail",
                active_menu_style: "minimal",
            };

            terminal
                .draw(|f| {
                    render_theme_select(f, f.area(), &theme, &ctx);
                })
                .unwrap();

            let buf = terminal.backend().buffer();
            let mut rendered = String::new();
            for y in 0..buf.area.height {
                for x in 0..buf.area.width {
                    rendered.push_str(buf[(x, y)].symbol());
                }
                rendered.push('\n');
            }
            assert!(rendered.contains("APPEARANCE"));
            assert!(rendered.contains("1. Themes"));
            assert!(rendered.contains("7. Activity Bar"));
            assert!(rendered.contains("8. Menu Styles"));
            assert!(rendered.contains("Live Preview:"));
        }
    }
}

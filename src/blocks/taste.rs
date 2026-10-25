//! Native UI Taste, Combinatorial Art Direction Dealer & Anti-Slop Linter.
//!
//! Distills `leonxlnx/taste-skill` v2.1 into zero-dependency native Rust:
//! 1. **Surface Classification (`UiSurfaceKind`)**: Automatically distinguishes scrolled
//!    **Pages** (landing, marketing, portfolio, editorial) from worked-in **App Screens**
//!    (dashboards, analytics, admin consoles, workbenches, tools).
//! 2. **Combinatorial Art Direction Dealer (`deal_art_direction`)**: Deals concrete,
//!    non-generic art directions with paired typography scales, ground/ink/accent rules,
//!    section composition blueprints, and the "One Live Interactive Thing" mandate,
//!    persisted per workspace in `.minicode/taste_direction.json` with automatic rotation.
//! 3. **Static Source UI Anti-Slop Linter (`lint_ui_source`)**: Scans `.html`, `.css`,
//!    `.scss`, `.jsx`, `.tsx`, `.vue`, `.svelte`, and `.astro` files for banned and warned
//!    AI-slop tells (`lorem`, `placeholder-data`, `gradient-text`, `ai-purple`,
//!    `claude-palette`, `decorative-numbering`, `eyebrow-pill`, `stock-ai-copy`,
//!    `generic-cta`, `transition-all`).

use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

static DIRECTION_ROTATION_COUNTER: AtomicUsize = AtomicUsize::new(0);

/// Distinguishes scrolled editorial/marketing Pages from worked-in product App Screens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UiSurfaceKind {
    /// Scrolled marketing, landing, portfolio, documentation, or showcase page.
    Page,
    /// Worked-in product interface: dashboard, admin console, workspace, analytics, or tool.
    AppScreen,
}

impl UiSurfaceKind {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Page => "Scrolled Page (Landing / Editorial / Showcase)",
            Self::AppScreen => "Worked-In App Screen (Dashboard / Workbench / Tool)",
        }
    }
}

/// A concrete, non-generic art direction brief dealt automatically for UI work.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtDirectionBrief {
    pub id: String,
    pub name: String,
    pub surface_kind: UiSurfaceKind,
    pub thesis: String,
    pub typography_scale: String,
    pub surface_and_color: String,
    pub layout_blueprint: String,
    pub live_interactive_signature: String,
}

/// Static template for curated art directions.
struct DirectionTemplate {
    id: &'static str,
    name: &'static str,
    surface_kind: UiSurfaceKind,
    thesis: &'static str,
    typography_scale: &'static str,
    surface_and_color: &'static str,
    layout_blueprint: &'static str,
    live_interactive_signature: &'static str,
}

const PAGE_DIRECTIONS: &[DirectionTemplate] = &[
    DirectionTemplate {
        id: "live_interface_hero",
        name: "Live Interface Crop & Technical Ledger",
        surface_kind: UiSurfaceKind::Page,
        thesis: "Lead with a crisp 2-line statement of what the product does, anchored immediately by a live, interactive `data-ui` product fragment with real domain data.",
        typography_scale: "Hero H1: `clamp(2.4rem, 4.8vw, 4.2rem)`, line-height `1.04`, tracking `-0.03em`, max 2 lines (<= 10 words). Section H2: `clamp(1.6rem, 2.8vw, 2.4rem)`. Technical readouts in `ui-monospace` with `tabular-nums`.",
        surface_and_color: "85% quiet primary ground + 10% elevated surface (`1px solid` hairline border at 8-12% ink opacity) + <= 5% single high-signal accent hue on primary CTA and active states.",
        layout_blueprint: "1) Top bar (`height: 60px`, brand + 3 nav links + 1 CTA) -> 2) Split or stacked Hero (2-line H1 + 1-sentence subcopy + primary action + live `data-ui` console/preview) -> 3) Asymmetric 2-column capability ledger -> 4) Interactive specification/comparison table -> 5) Focused closing action.",
        live_interactive_signature: "Include ONE live interactive product fragment (`data-ui=\"true\"`) in the hero or primary section (e.g., interactive command/pipeline runner, live latency/pricing calculator, or switchable real-data inspector) with numbers that genuinely add up.",
    },
    DirectionTemplate {
        id: "swiss_spec_sheet",
        name: "Swiss Technical Spec Sheet & Hairline Grid",
        surface_kind: UiSurfaceKind::Page,
        thesis: "Treat the page like a precision engineering datasheet: exposed `1px` structural grid hairlines, monospace coordinates, zero decorative pills, and tangible interactive benchmarks.",
        typography_scale: "Hero H1: `clamp(2.5rem, 5.2vw, 4.5rem)`, line-height `1.0`, tracking `-0.035em`, max 2 lines. Labels & metrics: `0.78rem` uppercase monospace (`letter-spacing: 0.06em`, `font-variant-numeric: tabular-nums`).",
        surface_and_color: "High-contrast architectural ground (either deep carbon `#0B0C10` with `#F3F4F6` ink, or archival paper `#F6F5F0` with `#121316` ink) and a single international-orange, emerald, or cobalt technical accent.",
        layout_blueprint: "1) Exposed hairline header grid -> 2) Full-frame asymmetric Hero cell (60% headline + action, 40% live telemetry/spec readout) -> 3) 3-column architectural matrix with shared borders (no floating card shadows) -> 4) Live interactive benchmark/configurator -> 5) Minimal colophon footer.",
        live_interactive_signature: "Embed a live interactive configurator, benchmark slider, or toggleable architecture matrix where switching modes updates real computed metrics in `tabular-nums`.",
    },
    DirectionTemplate {
        id: "monumental_editorial",
        name: "Name as Monument & Editorial Split",
        surface_kind: UiSurfaceKind::Page,
        thesis: "Contrast monumental display typography with quiet, generous negative space and one tactile interactive showcase.",
        typography_scale: "Display H1: `clamp(3.0rem, 6.8vw, 5.8rem)`, line-height `0.96`, tracking `-0.04em`, max 2 lines. Body copy constrained to `55ch-65ch` measure at `1.05rem` (`line-height: 1.65`).",
        surface_and_color: "Warm stone/alabaster or deep obsidian ground, zero gradient text, solid high-contrast ink, and one restrained editorial accent.",
        layout_blueprint: "1) Minimal header -> 2) Monumental 2-line title + concise lead paragraph + primary CTA on left, live interactive artifact on right -> 3) Editorial feature rows (alternating 5/7 and 7/5 column splits, never 3 identical icon-in-circle cards) -> 4) Social proof / real case ledger -> 5) Closing CTA.",
        live_interactive_signature: "Provide an interactive live preview switcher, interactive workflow stepper, or live filterable showcase with authentic domain content.",
    },
    DirectionTemplate {
        id: "kinetic_bento_workbench",
        name: "Asymmetric Bento Workbench & Live Micro-Tool",
        surface_kind: UiSurfaceKind::Page,
        thesis: "Organize capabilities into an asymmetric multi-span bento grid where every cell shows a real interface state or interactive control rather than stock marketing prose.",
        typography_scale: "Hero H1: `clamp(2.3rem, 4.5vw, 3.8rem)`, line-height `1.06`, tracking `-0.03em`, max 2 lines. Card titles `1.125rem` (`font-weight: 600`), body `0.925rem`.",
        surface_and_color: "Layered surface hierarchy (`--bg`, `--surface-1`, `--surface-2`) with crisp `1px` borders, equal outer viewport margins, and equal row heights across sibling cells.",
        layout_blueprint: "1) Compact top nav -> 2) Focused 2-line Hero + immediate action bar -> 3) Asymmetric 12-column Bento Grid (8-col live interactive workbench cell + 4-col telemetry cell; followed by 4-col + 8-col cells with matched bottom edges) -> 4) Interactive FAQ / spec accordion -> 5) Footer.",
        live_interactive_signature: "Make the primary 8-column bento cell a live interactive micro-tool (live search/filter, interactive code/state sandbox, or real-time estimator).",
    },
];

const APP_DIRECTIONS: &[DirectionTemplate] = &[
    DirectionTemplate {
        id: "quiet_linear_panels",
        name: "Quiet Linear Panels & Tabular Precision",
        surface_kind: UiSurfaceKind::AppScreen,
        thesis: "A worked-in workspace prioritizes information density, scannable alignment, keyboard speed, and quiet chrome over marketing decoration.",
        typography_scale: "Page/View title: `1.125rem-1.25rem` (`font-weight: 600`). Table & panel body: `0.8125rem-0.875rem` (`13px-14px`). All numeric columns, timestamps, deltas, and IDs MUST use `font-variant-numeric: tabular-nums`.",
        surface_and_color: "Whisper-quiet `1px` structural borders, maximum 3 data points per summary card, status dots (`6px` circle) instead of oversized glowing badges, and semantic status colors reserved strictly for real alerts.",
        layout_blueprint: "1) Compact navigation rail (`200px-240px` desktop, collapsible drawer on `<768px`) -> 2) Utility header bar (breadcrumb/view title + live search/filter input + primary action) -> 3) 3-4 compact KPI strip cells (equal height, `tabular-nums`) -> 4) Primary interactive data table or split master-detail view with right-aligned numeric columns.",
        live_interactive_signature: "Wire real interactive state: working search/status filter, sortable table columns, selectable rows with a live detail/inspector pane, and functional create/edit action state.",
    },
    DirectionTemplate {
        id: "dense_telemetry_console",
        name: "High-Density Telemetry & Split Inspector Console",
        surface_kind: UiSurfaceKind::AppScreen,
        thesis: "Engineered for operational clarity: zero marketing hero banners, instant scannability, monospace metric precision, and live filtering.",
        typography_scale: "Header title `1.15rem`, panel headers `0.75rem` uppercase tracked monospace, data rows `13px` sans + `12.5px` monospace with `tabular-nums`.",
        surface_and_color: "Deep slate/obsidian or crisp daylight neutral surface, hairline dividers, high-contrast primary readouts, and restrained single-hue active selection state.",
        layout_blueprint: "1) Top command & environment bar -> 2) Time-range & status filter toolbar -> 3) Primary 2-pane split (live chart/timeline + structured event/resource table with right-aligned numbers) -> 4) Contextual slide-over or side inspector for the selected item.",
        live_interactive_signature: "Include interactive time-window/status filtering, row selection that updates the live inspector panel, and consistent totals that match the visible rows.",
    },
    DirectionTemplate {
        id: "warm_paper_ledger",
        name: "Daylight Ledger & Structured Operations Table",
        surface_kind: UiSurfaceKind::AppScreen,
        thesis: "Calm daylight-first workspace with crisp ink hierarchy, scannable ledger tables, right-aligned figures, and zero visual clutter.",
        typography_scale: "View heading `1.25rem` (`font-weight: 600`), table cells `0.875rem`, numeric figures right-aligned with `tabular-nums`.",
        surface_and_color: "Warm paper ground (`#FAF8F5` or `#F8FAFC`), pure surface cards (`#FFFFFF`), `1px solid` slate borders (`#E4E4E7`), carbon ink (`#18181B`), and one decisive accent hue.",
        layout_blueprint: "1) Left sidebar or clean top module switcher -> 2) Summary ledger bar (3-4 metrics max, equal height) -> 3) Filterable & searchable ledger table with sticky header and right-aligned numeric columns -> 4) Detail modal or inline row expansion.",
        live_interactive_signature: "Provide live category/status tabs, instant search filtering across rows, and dynamic summary totals computed directly from the active filtered dataset.",
    },
];

/// Classifies whether the current workspace/task targets a scrolled **Page** or a worked-in **App Screen**.
pub fn detect_ui_surface_kind(workspace_root: &Path, prompt: &str) -> UiSurfaceKind {
    let lower = prompt.to_ascii_lowercase();

    let app_cues = [
        "dashboard",
        "admin",
        "analytics",
        "console",
        "workbench",
        "kanban",
        "crm",
        "erp",
        "backoffice",
        "telemetry",
        "data table",
        "settings panel",
        "control panel",
        "billing portal",
        "inventory system",
        "issue tracker",
    ];
    let page_cues = [
        "landing page",
        "marketing page",
        "marketing site",
        "portfolio",
        "home page",
        "homepage",
        "waitlist",
        "product page",
        "agency site",
        "blog",
        "editorial",
        "showcase",
        "hero section",
    ];

    let has_page_cue = page_cues.iter().any(|k| lower.contains(k));
    let has_app_cue = app_cues.iter().any(|k| lower.contains(k));

    if has_app_cue && !has_page_cue {
        return UiSurfaceKind::AppScreen;
    }
    if has_page_cue {
        return UiSurfaceKind::Page;
    }

    // Check workspace files for dashboard/admin route conventions if prompt didn't specify
    if workspace_root.join("src/pages/Dashboard.tsx").is_file()
        || workspace_root
            .join("src/components/Dashboard.tsx")
            .is_file()
        || workspace_root.join("src/app/dashboard").is_dir()
    {
        return UiSurfaceKind::AppScreen;
    }

    UiSurfaceKind::Page
}

/// Deals (or loads the persisted workspace) `ArtDirectionBrief` for the given workspace and prompt.
pub fn deal_art_direction(workspace_root: &Path, prompt: &str) -> ArtDirectionBrief {
    let surface_kind = detect_ui_surface_kind(workspace_root, prompt);
    let persist_path = workspace_root.join(".minicode/taste_direction.json");

    // Reuse persisted art direction for this workspace if surface kind matches
    if let Ok(raw) = std::fs::read_to_string(&persist_path) {
        if let Ok(existing) = serde_json::from_str::<ArtDirectionBrief>(&raw) {
            if existing.surface_kind == surface_kind {
                return existing;
            }
        }
    }

    let pool = match surface_kind {
        UiSurfaceKind::Page => PAGE_DIRECTIONS,
        UiSurfaceKind::AppScreen => APP_DIRECTIONS,
    };

    // Deterministic hash of workspace path + rotation counter so consecutive workspaces get fresh directions
    let ws_hash = workspace_root
        .to_string_lossy()
        .bytes()
        .fold(0usize, |acc, b| {
            acc.wrapping_mul(31).wrapping_add(b as usize)
        });
    let seq = DIRECTION_ROTATION_COUNTER.fetch_add(1, Ordering::Relaxed);
    let idx = (ws_hash.wrapping_add(seq)) % pool.len();
    let tpl = &pool[idx];

    let brief = ArtDirectionBrief {
        id: tpl.id.to_string(),
        name: tpl.name.to_string(),
        surface_kind: tpl.surface_kind,
        thesis: tpl.thesis.to_string(),
        typography_scale: tpl.typography_scale.to_string(),
        surface_and_color: tpl.surface_and_color.to_string(),
        layout_blueprint: tpl.layout_blueprint.to_string(),
        live_interactive_signature: tpl.live_interactive_signature.to_string(),
    };

    if workspace_root.exists() {
        let minicode_dir = workspace_root.join(".minicode");
        if std::fs::create_dir_all(&minicode_dir).is_ok() {
            if let Ok(json_str) = serde_json::to_string_pretty(&brief) {
                let _ = std::fs::write(&persist_path, json_str);
            }
        }
    }

    brief
}

impl ArtDirectionBrief {
    /// Formats the dealt art direction and `taste-skill` v2.1 house rules for prompt injection.
    pub fn format_prompt_block(&self) -> String {
        format!(
            "<taste_art_direction id=\"{id}\" surface=\"{surface}\">\n\
             \x20 Dealt Direction: **{name}** ({surface_label})\n\
             \x20 Thesis: {thesis}\n\
             \x20 Typography & Scale: {typo}\n\
             \x20 Surface & Color Proportions: {color}\n\
             \x20 Layout Blueprint: {layout}\n\
             \x20 Mandatory Live Interactive Thing: {live}\n\
             \x20 Core UI Taste & Anti-Slop Invariants (Enforced by Linter & Browser Probe):\n\
             \x20   1. **Page vs. App Screen**: Never put a marketing hero banner inside a worked-in App Screen; never build a Landing Page without one live interactive product fragment (`data-ui=\"true\"`).\n\
             \x20   2. **Headline Discipline**: Hero `<h1>` MUST be <= 2 rendered lines (<= 10 words) and visible in the first viewport (`top < innerHeight`). Section `<h2>` <= 2 lines.\n\
             \x20   3. **Frame & Section Symmetry**: Maintain equal left/right outer viewport margins. Never leave a tall section with content huddled on one side and >= 42% of the horizontal width empty (`lopsided`), and keep sibling cards in the same row at equal height (`h-full` / grid stretch).\n\
             \x20   4. **Banned AI-Slop Tells**: Zero `bg-clip-text text-transparent` gradient headings, zero unrequested indigo/violet AI-purple (`#6366f1`/`#8b5cf6`/`bg-indigo-*`/`text-purple-*`) or cloned Claude terracotta (`#d97757`), zero decorative `01 / 02 / 03` badges or `.eyebrow` pills above every heading, zero `Lorem ipsum`/`Acme Corp`/`John Doe`, zero stock AI buzzwords (`Unleash`, `Supercharge`, `Seamlessly`, `Revolutionize`, `Game-changer`), and zero `transition-all`.\n\
             \x20   5. **3-Viewport Visual Verification**: When verifying in browser, inspect layout geometry across desktop (`1440px`), tablet (`768px`), and mobile (`390px`) so text never touches screen edges (`>= 16px` padding).\n\
             </taste_art_direction>",
            id = self.id,
            surface = match self.surface_kind {
                UiSurfaceKind::Page => "page",
                UiSurfaceKind::AppScreen => "app_screen",
            },
            name = self.name,
            surface_label = self.surface_kind.label(),
            thesis = self.thesis,
            typo = self.typography_scale,
            color = self.surface_and_color,
            layout = self.layout_blueprint,
            live = self.live_interactive_signature,
        )
    }
}

/// Severity of a static UI anti-slop lint finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TasteLintSeverity {
    /// Hard anti-slop rule violation (`BANNED` in `taste-skill` v2.1).
    Banned,
    /// High-signal taste/layout warning (`WARN` in `taste-skill` v2.1).
    Warn,
}

impl TasteLintSeverity {
    pub fn badge(&self) -> &'static str {
        match self {
            Self::Banned => "BANNED",
            Self::Warn => "WARN",
        }
    }
}

/// A single finding produced by `lint_ui_source`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TasteLintFinding {
    pub rule_id: &'static str,
    pub severity: TasteLintSeverity,
    pub line: usize,
    pub message: String,
    pub fix_hint: &'static str,
}

/// Returns true if `path` is a UI/frontend source file subject to `taste-skill` linting.
pub fn is_ui_source_file(path: &str) -> bool {
    let norm = path.replace('\\', "/");
    if norm.contains("node_modules/")
        || norm.contains("dist/")
        || norm.contains("build/")
        || norm.contains(".minicode/")
        || norm.contains("test")
        || norm.contains("spec.")
    {
        return false;
    }
    let ext = Path::new(&norm)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    matches!(
        ext.as_str(),
        "html" | "htm" | "css" | "scss" | "jsx" | "tsx" | "vue" | "svelte" | "astro"
    )
}

/// Scans a UI source file (`.html`, `.css`, `.jsx`, `.tsx`, `.vue`, `.svelte`, `.astro`)
/// for `leonxlnx/taste-skill` v2.1 anti-slop tells.
pub fn lint_ui_source(path: &str, content: &str) -> Vec<TasteLintFinding> {
    if !is_ui_source_file(path) {
        return Vec::new();
    }

    let mut findings = Vec::new();
    let mut in_code_or_pre = false;
    let mut in_block_comment = false;
    let mut seen_rules: std::collections::HashSet<&'static str> = std::collections::HashSet::new();

    for (idx, raw_line) in content.lines().enumerate() {
        let line_no = idx + 1;
        let trimmed = raw_line.trim();
        if trimmed.is_empty() || trimmed.contains("taste-skill-ignore") {
            continue;
        }

        let lower = trimmed.to_ascii_lowercase();

        // Track multi-line block comments
        if in_block_comment {
            if lower.contains("*/") || lower.contains("-->") {
                in_block_comment = false;
            }
            continue;
        }
        if (lower.starts_with("/*") && !lower.contains("*/"))
            || (lower.starts_with("<!--") && !lower.contains("-->"))
        {
            in_block_comment = true;
            continue;
        }
        if trimmed.starts_with("//")
            || trimmed.starts_with("/*")
            || trimmed.starts_with('*')
            || trimmed.starts_with("{/*")
            || trimmed.starts_with("<!--")
        {
            continue;
        }

        // Track <pre> / <code> blocks so code samples aren't flagged as UI copy
        if lower.contains("<pre") || lower.contains("<code") {
            in_code_or_pre = true;
        }
        if in_code_or_pre {
            if lower.contains("</pre>") || lower.contains("</code>") {
                in_code_or_pre = false;
            }
            continue;
        }

        // 1. BANNED: Lorem ipsum & fake placeholder data
        if !seen_rules.contains("placeholder-data")
            && (lower.contains("lorem ipsum")
                || lower.contains("dolor sit amet")
                || lower.contains("consectetur adipiscing")
                || lower.contains("john doe")
                || lower.contains("jane doe")
                || lower.contains("acme corp")
                || lower.contains("user@example.com"))
        {
            seen_rules.insert("placeholder-data");
            findings.push(TasteLintFinding {
                rule_id: "placeholder-data",
                severity: TasteLintSeverity::Banned,
                line: line_no,
                message: "Placeholder text (`Lorem ipsum`, `John Doe`, `Acme Corp`, `example.com`) detected.".to_string(),
                fix_hint: "Write specific, believable domain copy and realistic product names/numbers that add up.",
            });
        }

        // 2. BANNED: Gradient text (`bg-clip-text` + `text-transparent` or `background-clip: text`)
        if !seen_rules.contains("gradient-text")
            && ((lower.contains("bg-clip-text") && lower.contains("text-transparent"))
                || lower.contains("background-clip: text")
                || lower.contains("-webkit-background-clip: text"))
        {
            seen_rules.insert("gradient-text");
            findings.push(TasteLintFinding {
                rule_id: "gradient-text",
                severity: TasteLintSeverity::Banned,
                line: line_no,
                message: "Gradient-clipped text (`bg-clip-text` / `background-clip: text`) is an overused AI-slop tell.".to_string(),
                fix_hint: "Use solid high-contrast ink for headings; if emphasis is needed, color a single keyword with a solid accent token.",
            });
        }

        // 3. BANNED: Default AI-Purple / Indigo gradient palette & Cloned Claude palette
        if !seen_rules.contains("ai-purple-palette")
            && (lower.contains("from-indigo-")
                || lower.contains("via-purple-")
                || lower.contains("to-purple-")
                || lower.contains("from-purple-")
                || lower.contains("to-indigo-")
                || lower.contains("#6366f1")
                || lower.contains("#8b5cf6")
                || lower.contains("#7c3aed")
                || lower.contains("#a855f7")
                || lower.contains("#d97757")
                || lower.contains("#da7756"))
        {
            seen_rules.insert("ai-purple-palette");
            findings.push(TasteLintFinding {
                rule_id: "ai-purple-palette",
                severity: TasteLintSeverity::Banned,
                line: line_no,
                message: "Default AI indigo/purple gradient (`#6366f1`/`#8b5cf6`/`from-indigo-*`) or cloned terracotta (`#d97757`) detected.".to_string(),
                fix_hint: "Use the workspace's curated `block_palettes` tokens or the dealt `<taste_art_direction>` ground/ink/accent palette.",
            });
        }

        // 4. WARN: Decorative 01 / 02 / 03 index badges in markup
        if !seen_rules.contains("decorative-numbering") && is_decorative_number_badge(trimmed) {
            seen_rules.insert("decorative-numbering");
            findings.push(TasteLintFinding {
                rule_id: "decorative-numbering",
                severity: TasteLintSeverity::Warn,
                line: line_no,
                message: "Decorative `01 / 02 / 03` number badge detected.".to_string(),
                fix_hint: "Remove decorative `01/02/03` badges unless the section is a genuine ordered specification or index table.",
            });
        }

        // 5. WARN: Eyebrow / kicker pill classes above headings
        if !seen_rules.contains("eyebrow-pill")
            && (lower.contains("class=\"eyebrow")
                || lower.contains("classname=\"eyebrow")
                || lower.contains("class=\"kicker")
                || lower.contains("classname=\"kicker")
                || lower.contains("section-badge")
                || lower.contains("hero-badge"))
        {
            seen_rules.insert("eyebrow-pill");
            findings.push(TasteLintFinding {
                rule_id: "eyebrow-pill",
                severity: TasteLintSeverity::Warn,
                line: line_no,
                message: "Eyebrow/kicker pill badge class (`.eyebrow`, `.kicker`, `.section-badge`) detected.".to_string(),
                fix_hint: "Let the heading hierarchy speak directly without stacking rounded pill badges above every section title.",
            });
        }

        // 6. WARN: Stock AI marketing buzzwords & generic CTAs
        if !seen_rules.contains("stock-ai-copy") && contains_stock_ai_copy(&lower) {
            seen_rules.insert("stock-ai-copy");
            findings.push(TasteLintFinding {
                rule_id: "stock-ai-copy",
                severity: TasteLintSeverity::Warn,
                line: line_no,
                message: "Stock AI buzzword copy (`Unleash`, `Supercharge`, `Seamlessly`, `Game-changer`, `Revolutionize`, `Elevate your`) detected.".to_string(),
                fix_hint: "State concretely what the product does and who it is for in plain, authoritative domain language.",
            });
        }

        if !seen_rules.contains("generic-cta")
            && (lower.contains(">learn more<")
                || lower.contains(">click here<")
                || lower.contains(">get started for free<"))
        {
            seen_rules.insert("generic-cta");
            findings.push(TasteLintFinding {
                rule_id: "generic-cta",
                severity: TasteLintSeverity::Warn,
                line: line_no,
                message: "Generic CTA button label (`Learn More` / `Click Here`) detected.".to_string(),
                fix_hint: "Name the concrete action or destination (e.g. `Run Live Benchmark`, `Inspect AST Graph`, `Read Protocol Spec`).",
            });
        }

        // 7. WARN: CSS & motion footguns (`transition-all` / `transition: all`)
        if !seen_rules.contains("transition-all")
            && (contains_class_token(&lower, "transition-all")
                || lower.contains("transition: all")
                || lower.contains("transition:all"))
        {
            seen_rules.insert("transition-all");
            findings.push(TasteLintFinding {
                rule_id: "transition-all",
                severity: TasteLintSeverity::Warn,
                line: line_no,
                message: "`transition-all` / `transition: all` animates layout properties (`width`, `height`, `padding`) and causes jank.".to_string(),
                fix_hint: "Specify exact properties (`transition-colors`, `transition-transform`, `transition-opacity`, or `transition: transform 200ms ease, opacity 200ms ease`).",
            });
        }
    }

    findings
}

/// Formats UI anti-slop lint findings as a compact post-write feedback block for `write_file` / `patch_file`.
pub fn format_post_write_taste_notice(path: &str, content: &str) -> Option<String> {
    let findings = lint_ui_source(path, content);
    if findings.is_empty() {
        return None;
    }

    let mut lines = Vec::new();
    for f in findings.iter().take(4) {
        lines.push(format!(
            "- [{} `{}` at L{}]: {} Fix: {}",
            f.severity.badge(),
            f.rule_id,
            f.line,
            f.message,
            f.fix_hint
        ));
    }

    Some(format!(
        "\n\n<ui-taste-lint file=\"{}\">\n{}\n</ui-taste-lint>\nRefine these UI taste / anti-slop tells so the interface looks human-crafted and intentional.",
        path,
        lines.join("\n")
    ))
}

fn is_decorative_number_badge(line: &str) -> bool {
    // Matches e.g. `>01<`, `>02<`, `>03<`, `>[01]<`, `>(01)<` inside span/div tags
    for badge in [
        ">01<", ">02<", ">03<", ">04<", ">[01]<", ">[02]<", ">[03]<", ">(01)<", ">(02)<",
    ] {
        if line.contains(badge) {
            return true;
        }
    }
    false
}

fn contains_stock_ai_copy(lower_line: &str) -> bool {
    const BUZZWORDS: &[&str] = &[
        "unleash the power",
        "supercharge your",
        "revolutionize your",
        "revolutionizing the way",
        "game-changing",
        "game changer",
        "seamlessly integrate",
        "elevate your workflow",
        "take your workflow to the next level",
        "in today's fast-paced",
        "delve into",
    ];
    BUZZWORDS.iter().any(|b| lower_line.contains(b))
}

fn contains_class_token(lower_line: &str, token: &str) -> bool {
    lower_line
        .split(|c: char| !c.is_alphanumeric() && c != '-' && c != '_')
        .any(|t| t == token)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_ui_surface_kind_distinguishes_page_and_app() {
        let tmp = std::env::temp_dir();
        assert_eq!(
            detect_ui_surface_kind(
                &tmp,
                "Build a SaaS analytics dashboard with telemetry tables"
            ),
            UiSurfaceKind::AppScreen
        );
        assert_eq!(
            detect_ui_surface_kind(&tmp, "Create a modern landing page for our developer CLI"),
            UiSurfaceKind::Page
        );
    }

    #[test]
    fn test_deal_art_direction_formats_prompt_block() {
        let tmp =
            std::env::temp_dir().join(format!("minicode_taste_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&tmp).unwrap();

        let brief = deal_art_direction(&tmp, "Create a landing page for minicode");
        assert_eq!(brief.surface_kind, UiSurfaceKind::Page);
        let block = brief.format_prompt_block();
        assert!(block.contains("<taste_art_direction"));
        assert!(block.contains("Mandatory Live Interactive Thing:"));
        assert!(block.contains("Headline Discipline"));

        // Verify persistence in .minicode/taste_direction.json
        let brief_again = deal_art_direction(&tmp, "Update hero section on the landing page");
        assert_eq!(brief.id, brief_again.id);

        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn test_lint_ui_source_detects_banned_and_warn_tells() {
        let bad_tsx = r#"
export function HeroSection() {
  return (
    <section className="bg-slate-950">
      <span className="eyebrow">01</span>
      <h1 className="bg-gradient-to-r from-indigo-500 via-purple-500 to-pink-500 bg-clip-text text-transparent">
        Supercharge your workflow with Acme Corp
      </h1>
      <p>Lorem ipsum dolor sit amet, consectetur adipiscing elit.</p>
      <button className="transition-all">Learn More</button>
    </section>
  );
}
"#;
        let findings = lint_ui_source("src/components/HeroSection.tsx", bad_tsx);
        let ids: Vec<&str> = findings.iter().map(|f| f.rule_id).collect();
        assert!(ids.contains(&"gradient-text"));
        assert!(ids.contains(&"ai-purple-palette"));
        assert!(ids.contains(&"placeholder-data"));
        assert!(ids.contains(&"decorative-numbering"));
        assert!(ids.contains(&"eyebrow-pill"));
        assert!(ids.contains(&"stock-ai-copy"));
        assert!(ids.contains(&"generic-cta"));
        assert!(ids.contains(&"transition-all"));

        let notice = format_post_write_taste_notice("src/components/HeroSection.tsx", bad_tsx);
        assert!(notice.is_some());
        assert!(notice
            .unwrap()
            .contains("<ui-taste-lint file=\"src/components/HeroSection.tsx\">"));
    }

    #[test]
    fn test_lint_ui_source_ignores_clean_crafted_ui_and_non_ui_files() {
        let clean_tsx = r#"
export function TelemetryPanel() {
  return (
    <section className="border border-zinc-800 bg-zinc-950 p-6">
      <h1 className="text-3xl font-semibold tracking-tight text-zinc-100">
        Deterministic AST Code Graph
      </h1>
      <button className="transition-colors hover:bg-emerald-500">
        Inspect Blast Radius
      </button>
    </section>
  );
}
"#;
        assert!(lint_ui_source("src/components/TelemetryPanel.tsx", clean_tsx).is_empty());
        assert!(lint_ui_source("src/main.rs", "fn main() { /* lorem ipsum */ }").is_empty());
    }
}

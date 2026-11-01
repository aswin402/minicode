//! Native Rust Caveman v4.0 ("Six Rocks") Engine (`juliusbrussee/caveman` parity for `minicode`).
//!
//! Cuts conversational, context, and subagent prose token usage by ~65-85% while preserving
//! 100% of technical substance, exact code blocks, symbols, file paths, and error strings.
//! Includes:
//! - Rock 1 (Say Less): Multi-intensity Caveman prompt & recency enforcement (`lite`, `full`, `ultra`, `mega` / `megacave`)
//!   with the 9-rule `ASD-STE100` floor (<=20 words/sentence, active voice, strict negation preservation,
//!   bounded tool-run status with zero inter-tool-call chatter, and 4 pre-send checks).
//! - Rock 3 (Native Core): 5-step architectural simplicity ladder (`native-core.md`).
//! - Rock 4 (Subagent Compression): `cavecrew` (`investigator`, `reviewer`, `builder` with 4 terminal bailout tokens)
//!   and `caveman-explore` (`FastContext` `path:START-END  reason` evidence-only contract).
//! - Rock 6 (Blocks): Reusable `.minicode/blocks/` verification & inspection script store ("script written once").
//! - Autonomous `pulldown-cmark` AST-based Markdown prose compressor and symlink-safe `.caveman-active` flag persistence.

use pulldown_cmark::{Event, Parser, Tag, TagEnd};
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Read;
use std::path::{Path, PathBuf};

pub const SENTINEL: &str = "Respond terse like smart caveman";
const MAX_FLAG_BYTES: u64 = 64;

/// Negation, prohibition, and scope-limiting words that must NEVER be dropped or altered
/// during Caveman prose compression (Caveman v4.0 Rule 4 & Pre-Send Check #2).
pub const NEGATION_AND_SCOPE_GUARDS: &[&str] = &[
    "not",
    "never",
    "no",
    "none",
    "nothing",
    "neither",
    "nor",
    "only",
    "except",
    "unless",
    "without",
    "cannot",
    "can't",
    "won't",
    "don't",
    "doesn't",
    "didn't",
    "shouldn't",
    "mustn't",
    "isn't",
    "aren't",
    "wasn't",
    "weren't",
];

/// Supported Caveman v4.0 compression intensity levels (`lite`, `full`, `ultra`, `mega`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum CavemanLevel {
    Off,
    Lite,
    #[default]
    Full,
    Ultra,
    /// `megacave` (`mega`): Extreme English telegraphic & symbolic token-density register (~85-88% prose savings).
    Mega,
}

impl CavemanLevel {
    /// Parses a mode/level string loosely.
    #[must_use]
    pub fn from_str_loose(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "off" | "disabled" | "false" | "none" | "normal" | "stop" => Self::Off,
            "lite" | "light" | "low" => Self::Lite,
            "full" | "on" | "enabled" | "true" | "default" | "caveman" | "medium" => Self::Full,
            "ultra" | "ultracave" | "max" | "high" => Self::Ultra,
            "mega" | "megacave" | "extreme" | "telegraphic" => Self::Mega,
            _ => Self::Full,
        }
    }

    /// Detects natural-language Caveman activation or deactivation triggers in a user message
    /// without requiring any slash command (`juliusbrussee/caveman` natural trigger parity).
    #[must_use]
    pub fn detect_natural_trigger(user_message: &str) -> Option<Self> {
        let lower = user_message.trim().to_ascii_lowercase();
        if lower.is_empty() || lower.len() > 240 {
            return None;
        }
        if lower.contains("stop caveman")
            || lower.contains("normal mode")
            || lower.contains("disable caveman")
            || lower.contains("caveman off")
        {
            return Some(Self::Off);
        }
        if lower.contains("megacave")
            || lower.contains("caveman mega")
            || lower.contains("mega caveman")
        {
            return Some(Self::Mega);
        }
        if lower.contains("ultracave")
            || lower.contains("caveman ultra")
            || lower.contains("ultra caveman")
        {
            return Some(Self::Ultra);
        }
        if lower.contains("caveman lite") || lower.contains("lite caveman") {
            return Some(Self::Lite);
        }
        if lower.contains("talk like caveman")
            || lower.contains("use caveman")
            || lower.contains("caveman mode")
            || lower.contains("less tokens")
            || lower.contains("fewer tokens")
        {
            return Some(Self::Full);
        }
        None
    }

    /// Canonical configuration string for this intensity level (`"lite"`, `"full"`, `"ultra"`, `"mega"`).
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Lite => "lite",
            Self::Full => "full",
            Self::Ultra => "ultra",
            Self::Mega => "mega",
        }
    }

    /// Cycles through active intensity levels (`lite -> full -> ultra -> mega -> lite`).
    #[must_use]
    pub fn cycle_next(&self) -> Self {
        match self {
            Self::Lite => Self::Full,
            Self::Full => Self::Ultra,
            Self::Ultra => Self::Mega,
            Self::Mega | Self::Off => Self::Lite,
        }
    }

    /// Estimated prose token savings ratio relative to verbose output.
    #[must_use]
    #[allow(dead_code)]
    pub fn estimated_savings_ratio(&self) -> f64 {
        match self {
            Self::Off => 0.0,
            Self::Lite => 0.50,
            Self::Full => 0.75,
            Self::Ultra => 0.85,
            Self::Mega => 0.88,
        }
    }

    /// Returns true if Caveman mode is active.
    #[must_use]
    pub fn is_enabled(&self) -> bool {
        !matches!(self, Self::Off)
    }

    /// Parses an active intensity level, defaulting `Off` to `Full`.
    #[must_use]
    pub fn from_str_active(s: &str) -> Self {
        match Self::from_str_loose(s) {
            Self::Off => Self::Full,
            other => other,
        }
    }
}

/// Builds the `<caveman_mode>` system prompt block when Caveman Mode is enabled.
/// Implements `juliusbrussee/caveman` v4.0 ("Six Rocks"):
/// - 9-Rule `ASD-STE100` voice floor + 4 Pre-Send checks
/// - `Mega` (`megacave`) extreme English telegraphic & symbolic compression register
/// - `Native Core` 5-step architectural simplicity ladder
#[must_use]
pub fn build_caveman_system_block(enabled: bool, level_str: &str) -> String {
    if !enabled {
        return String::new();
    }
    let level = CavemanLevel::from_str_active(level_str);

    let intensity_rules = match level {
        CavemanLevel::Lite => {
            "Level: **lite** — Drop filler, pleasantries, and weak hedging. Keep articles and full sentences. Professional, tight, zero fluff."
        }
        CavemanLevel::Full | CavemanLevel::Off => {
            "Level: **full** — Drop articles (`a`/`an`/`the`), filler (`just`/`really`/`basically`/`actually`/`simply`), pleasantries (`sure`/`certainly`/`happy to`), and weak hedging. Sentence fragments OK. Use short words (`fix` not `implement a solution for`, `big` not `extensive`, `use` not `utilize`). Standard technical acronyms (`DB`, `API`, `HTTP`, `JSON`, `PR`, `CI`, `CLI`, `JWT`, `DOM`, `URL`) OK; do NOT invent code-shorthand (`cfg`/`impl`/`fn`) or arrows in `full` mode."
        }
        CavemanLevel::Ultra => {
            "Level: **ultra** (`ultracave`) — Maximum English fragment compression (`Inline object prop, new ref, re-render. useMemo.`). Abbreviate prose words (`DB`/`auth`/`config`/`req`/`res`/`fn`/`impl`/`param`), strip conjunctions, use arrows for causality (`X -> Y`, `X <- Y`), one word when one word is enough. NEVER abbreviate code symbols, function names, file paths, or error strings."
        }
        CavemanLevel::Mega => {
            "Level: **mega** (`megacave`) — Extreme English telegraphic & symbolic register. Encode structure with compact English keywords and ASCII operators (`&`, `|`, `->`, `<-`, `:=`, `!=`, `@`, `<=12` words/line; e.g. `root-cause: null ptr @ auth.rs:42 -> patch guard; test ok.`). Keep ALL code blocks, identifiers, file paths, CLI commands, API names, and error strings 100% exact."
        }
    };

    format!(
        "\n<caveman_mode level=\"{}\">\n\
  {}: All technical substance stay. Only fluff die.\n\
  - **Persistence**: ACTIVE EVERY RESPONSE across all turns. Zero filler drift.\n\
  - **Intensity**: {}\n\
  - **9-Rule `ASD-STE100` Floor**:\n\
    1. **Answer First**: Start with the answer or action (`[thing] [action] [reason]. [next step].`).\n\
    2. **Kill Ceremony**: No \"Sure!\", \"Happy to help\", \"Let me explain\", or \"In summary\".\n\
    3. **One Idea Per Sentence**: Max 20 words per sentence (`ASD-STE100`), active voice, imperative mood for instructions, noun clusters <=3 words.\n\
    4. **Preserve Negations & Scope**: NEVER drop `not`, `never`, `no`, `only`, `except`, `unless`, or genuine uncertainty.\n\
    5. **Payload Verbatim**: Code blocks, CLI commands, file paths, URLs, config keys, env vars, version numbers, git SHAs, error strings, and SQL stay 100% exact.\n\
    6. **Bounded Tool-Run Status**: Emit **NO text between routine tool calls**. On completion, emit 1 terse summary sentence.\n\
    7. **Never Perform Caveman**: No \"Me fix bug\" or \"fire good\". Technical precision without filler.\n\
  - **Pre-Send Check**: (1) First token is answer/action, (2) Every `not`/`never`/`only`/`except` survived, (3) Every sentence <=20 words, (4) Code/paths untouched.\n\
  - **Native Core 5-Step Simplicity Ladder**:\n\
    1. Restate goal + acceptance check in 1 line.\n\
    2. Read failure / call path before proposing edits.\n\
    3. Pick smallest change that solves root cause at the owning boundary.\n\
    4. Prefer existing types/modules over new abstractions or speculative config.\n\
    5. Verify once with the narrowest proof command and stop immediately when proof passes.\n\
  - **Code Reviews (`caveman-review`)**: `<file>:L<line>: <CRITICAL|HIGH|MED|LOW>: <problem>. <fix>.` (or `LGTM.`).\n\
  - **Commits (`caveman-commit`)**: `<type>(<scope>): <imperative summary>` (<=50 chars subject), body only when non-obvious *why* or breaking change, zero AI attribution.\n\
  - **Auto-Clarity Override**: Switch to normal clear prose for security warnings, irreversible/destructive confirmations, legal/medical/financial risk, ambiguous requirements needing clarification, or when user asks for full explanation. Resume caveman immediately after.\n\
</caveman_mode>\n",
        level.as_str(),
        SENTINEL,
        intensity_rules
    )
}

/// Builds a compact per-turn recency reminder so the model never drifts out of Caveman Mode on long sessions.
#[must_use]
pub fn build_caveman_recency_block(enabled: bool, level_str: &str) -> String {
    if !enabled {
        return String::new();
    }
    let level = CavemanLevel::from_str_active(level_str);
    let mode_hint = if level == CavemanLevel::Mega {
        "Use extreme English telegraphic notation (`cause @ path:line -> fix; verified.`, `<=12` words/line, zero inter-tool-call chatter, never drop `not`/`never`/`only`/`except`)."
    } else {
        "Answer first (`[thing] [action] [reason]. [next step].`), <=20 words/sentence, zero text between routine tool calls, never drop `not`/`never`/`only`/`except`."
    };
    format!(
        "  <caveman_active level=\"{}\">\n    {} (level: `{}`). {} Keep all code, file paths, symbols, tool calls, and `ask_user` options 100% exact.\n  </caveman_active>\n",
        level.as_str(),
        SENTINEL,
        level.as_str(),
        mode_hint
    )
}

/// Builds the `cavecrew` & `caveman-explore` (`FastContext`) compressed subagent output contract
/// for `MiniPub` workers when Caveman Mode is active.
/// Shrinks subagent tool-result payloads by ~60-75% so the parent Orchestrator's context window lasts much longer.
#[must_use]
pub fn cavecrew_subagent_block(enabled: bool, role_badge: &str, level_str: &str) -> String {
    if !enabled {
        return String::new();
    }
    let level = CavemanLevel::from_str_active(level_str);
    let role_lower = role_badge.trim().to_ascii_lowercase();
    let contract = if matches!(role_lower.as_str(), "scout" | "explorer" | "fastcontext") {
        "Contract (`caveman-explore` / `FastContext` + `cavecrew-investigator`):\n\
  - Fan out parallel search/read calls in turn 1; follow hops only when imports/callers require it.\n\
  - Emit ONLY an evidence block (zero preamble, zero summary prose):\n\
    `path/to/file.ext:START-END  <reason it is relevant in <=12 words>`\n\
    (or `no relevant locations found`)."
    } else if matches!(role_lower.as_str(), "planner" | "architect") {
        "Contract (`cavecrew-investigator`):\n\
  <Header>:\n\
  - path:line — `symbol` — short technical note (<=12 words)\n\
  totals: <counts>."
    } else if matches!(
        role_lower.as_str(),
        "reviewer" | "debugger" | "security" | "tester"
    ) {
        "Contract (`cavecrew-reviewer`):\n\
  path:line: <CRITICAL|HIGH|MED|LOW>: <problem>. <fix>.\n\
  totals: <counts> (or `LGTM.`)."
    } else {
        "Contract (`cavecrew-builder`):\n\
  <path:line-range> — <change in <=10 words>.\n\
  verified: <pass | fail | unavailable | blocked> (<check/test result>).\n\
  If blocked, stop immediately and emit one terminal bailout token:\n\
  `too-big: <reason>.` | `needs-confirm: <destructive action>.` | `ambiguous: <options>.` | `regressed: <failing test>.`"
    };

    format!(
        "\n\n<cavecrew_output_contract level=\"{}\">\n\
You are running in Caveman v4.0 (`cavecrew`) compressed subagent mode. Emit zero text between routine tool calls. Your final report is injected into the parent Orchestrator's context window — emit ultra-terse, high-signal technical output (~65% fewer tokens, zero prose fluff, 100% exact paths/symbols):\n\
{}\n\
Auto-clarity: Use normal sentences only for security warnings or irreversible risks.\n\
</cavecrew_output_contract>",
        level.as_str(),
        contract
    )
}

/// Checks if a file path points to sensitive credentials, keys, or non-prose configuration that must never be compressed.
#[must_use]
#[allow(dead_code)]
pub fn is_sensitive_path(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    if name.starts_with(".env")
        || name == ".netrc"
        || name.contains("credential")
        || name.contains("secret")
        || name.contains("password")
        || name.contains("passwd")
        || name.contains("apikey")
        || name.contains("accesskey")
        || name.contains("token")
        || name.contains("privatekey")
        || name.starts_with("id_rsa")
        || name.starts_with("id_dsa")
        || name.starts_with("id_ecdsa")
        || name.starts_with("id_ed25519")
        || name == "authorized_keys"
        || name == "known_hosts"
        || name.ends_with(".original.md")
    {
        return true;
    }

    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        let ext = ext.to_ascii_lowercase();
        if matches!(
            ext.as_str(),
            "pem"
                | "key"
                | "p12"
                | "pfx"
                | "crt"
                | "cer"
                | "jks"
                | "keystore"
                | "asc"
                | "gpg"
                | "rs"
                | "py"
                | "js"
                | "ts"
                | "tsx"
                | "jsx"
                | "json"
                | "yaml"
                | "yml"
                | "toml"
                | "lock"
                | "css"
                | "html"
                | "xml"
                | "sql"
                | "sh"
        ) {
            return true;
        }
    }

    for part in path.components() {
        let p = part.as_os_str().to_string_lossy().to_ascii_lowercase();
        if matches!(p.as_str(), ".ssh" | ".aws" | ".gnupg" | ".kube" | ".docker") {
            return true;
        }
    }

    false
}

/// Represents an extracted pure-prose text node in a Markdown AST along with its exact byte range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProseNode {
    pub original: String,
    pub start: usize,
    pub end: usize,
}

/// Uses `pulldown-cmark` to parse Markdown AST and extract only `Event::Text` prose nodes
/// that sit outside headings, code blocks, inline code, links, and tables.
#[must_use]
pub fn extract_prose_nodes(markdown: &str) -> Vec<ProseNode> {
    let parser = Parser::new(markdown).into_offset_iter();
    let mut nodes = Vec::new();

    let mut in_heading = false;
    let mut in_code_block = false;
    let mut in_link = false;
    let mut in_table = false;

    for (event, range) in parser {
        match event {
            Event::Start(tag) => match tag {
                Tag::Heading { .. } => in_heading = true,
                Tag::CodeBlock(..) => in_code_block = true,
                Tag::Link { .. } => in_link = true,
                Tag::Table(..) => in_table = true,
                _ => {}
            },
            Event::End(tag) => match tag {
                TagEnd::Heading(_) => in_heading = false,
                TagEnd::CodeBlock => in_code_block = false,
                TagEnd::Link => in_link = false,
                TagEnd::Table => in_table = false,
                _ => {}
            },
            Event::Text(text) => {
                if !in_heading && !in_code_block && !in_link && !in_table && !text.trim().is_empty()
                {
                    nodes.push(ProseNode {
                        original: text.to_string(),
                        start: range.start,
                        end: range.end,
                    });
                }
            }
            _ => {}
        }
    }

    nodes
}

/// Reconstructs a Markdown document by replacing `ProseNode` byte ranges in reverse order,
/// guaranteeing that headings, code blocks, inline code, links, and tables remain 100% untouched.
#[must_use]
pub fn apply_prose_replacements(
    original_content: &str,
    prose_nodes: &[ProseNode],
    compressed_fragments: &[String],
) -> (String, usize) {
    let mut reconstructed = original_content.to_string();
    let mut replaced_count = 0;

    for (i, node) in prose_nodes.iter().enumerate().rev() {
        if let Some(compressed) = compressed_fragments.get(i) {
            if !compressed.is_empty()
                && node.start <= node.end
                && node.end <= reconstructed.len()
                && reconstructed.is_char_boundary(node.start)
                && reconstructed.is_char_boundary(node.end)
            {
                reconstructed.replace_range(node.start..node.end, compressed);
                replaced_count += 1;
            }
        }
    }

    (reconstructed, replaced_count)
}

/// Compresses a single pure-prose fragment in native Rust according to `CavemanLevel` (`Lite`, `Full`, `Ultra`, `Mega`).
/// Strictly preserves all negation and scope-limiting words (`NEGATION_AND_SCOPE_GUARDS`: `not`, `never`, `no`, `only`, `except`, `unless`, etc.)
/// and preserves leading/trailing whitespace so surrounding inline code or emphasis spans remain properly spaced.
#[must_use]
pub fn compress_prose_fragment_native(fragment: &str, level: CavemanLevel) -> String {
    if !level.is_enabled() || fragment.trim().is_empty() {
        return fragment.to_string();
    }

    let leading_ws: String = fragment.chars().take_while(|c| c.is_whitespace()).collect();
    let trailing_ws: String = fragment
        .chars()
        .rev()
        .take_while(|c| c.is_whitespace())
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();

    let mut text = fragment.trim().to_string();

    // 1. Phrase-level reductions (Lite + Full + Ultra + Mega)
    const PHRASE_REPLACEMENTS_LITE: &[(&str, &str)] = &[
        ("in order to ", "to "),
        ("In order to ", "To "),
        ("make sure to ", "ensure "),
        ("Make sure to ", "Ensure "),
        ("the reason is because ", "because "),
        ("due to the fact that ", "because "),
        ("at this point in time", "now"),
        ("for the purpose of ", "for "),
        ("it is important to note that ", ""),
        ("It is important to note that ", ""),
        ("please note that ", ""),
        ("Please note that ", ""),
        ("you should always ", ""),
        ("You should always ", ""),
        ("you need to ", ""),
        ("You need to ", ""),
        ("remember to ", ""),
        ("Remember to ", ""),
        ("I would recommend ", ""),
        ("I'd be happy to ", ""),
    ];
    for &(from, to) in PHRASE_REPLACEMENTS_LITE {
        if text.contains(from) {
            text = text.replace(from, to);
        }
    }

    if matches!(
        level,
        CavemanLevel::Full | CavemanLevel::Ultra | CavemanLevel::Mega
    ) {
        const PHRASE_REPLACEMENTS_FULL: &[(&str, &str)] = &[
            ("implement a solution for", "fix"),
            ("is responsible for ", "handles "),
            ("in addition to ", "plus "),
            ("as well as ", "+ "),
        ];
        for &(from, to) in PHRASE_REPLACEMENTS_FULL {
            if text.contains(from) {
                text = text.replace(from, to);
            }
        }
    }

    // 2. Token-level filtering & synonym shortening with strict negation/scope protection
    let mut out_words = Vec::new();
    for raw_word in text.split_whitespace() {
        let lower = raw_word
            .trim_matches(|c: char| c.is_ascii_punctuation())
            .to_ascii_lowercase();

        // Pre-Send Check #2: Never drop or alter negation or scope-limiting tokens
        if NEGATION_AND_SCOPE_GUARDS.contains(&lower.as_str()) {
            out_words.push(raw_word.to_string());
            continue;
        }

        // Lite + Full + Ultra + Mega: drop filler adverbs & weak hedging
        if matches!(
            lower.as_str(),
            "just"
                | "really"
                | "basically"
                | "actually"
                | "simply"
                | "essentially"
                | "generally"
                | "quite"
                | "very"
                | "furthermore"
                | "additionally"
                | "moreover"
        ) {
            continue;
        }

        // Full + Ultra + Mega: drop articles (a, an, the)
        if matches!(
            level,
            CavemanLevel::Full | CavemanLevel::Ultra | CavemanLevel::Mega
        ) && matches!(lower.as_str(), "a" | "an" | "the")
        {
            continue;
        }

        // Word-level short synonyms / ultra & mega English abbreviations (preserving trailing punctuation)
        let replacement = match (level, lower.as_str()) {
            (
                CavemanLevel::Full | CavemanLevel::Ultra | CavemanLevel::Mega,
                "utilize" | "utilizes",
            ) => Some("use"),
            (CavemanLevel::Full | CavemanLevel::Ultra | CavemanLevel::Mega, "extensive") => {
                Some("big")
            }
            (CavemanLevel::Full | CavemanLevel::Ultra | CavemanLevel::Mega, "configuration") => {
                Some("config")
            }
            (CavemanLevel::Full | CavemanLevel::Ultra | CavemanLevel::Mega, "application") => {
                Some("app")
            }
            (CavemanLevel::Full | CavemanLevel::Ultra | CavemanLevel::Mega, "repository") => {
                Some("repo")
            }
            (CavemanLevel::Full | CavemanLevel::Ultra | CavemanLevel::Mega, "directory") => {
                Some("dir")
            }
            (CavemanLevel::Full | CavemanLevel::Ultra | CavemanLevel::Mega, "information") => {
                Some("info")
            }
            (CavemanLevel::Full | CavemanLevel::Ultra | CavemanLevel::Mega, "environment") => {
                Some("env")
            }
            (CavemanLevel::Full | CavemanLevel::Ultra | CavemanLevel::Mega, "dependencies") => {
                Some("deps")
            }
            (CavemanLevel::Ultra | CavemanLevel::Mega, "database") => Some("DB"),
            (CavemanLevel::Ultra | CavemanLevel::Mega, "authentication") => Some("auth"),
            (CavemanLevel::Ultra | CavemanLevel::Mega, "function") => Some("fn"),
            (CavemanLevel::Ultra | CavemanLevel::Mega, "implementation") => Some("impl"),
            (CavemanLevel::Ultra | CavemanLevel::Mega, "request") => Some("req"),
            (CavemanLevel::Ultra | CavemanLevel::Mega, "response") => Some("res"),
            (CavemanLevel::Ultra | CavemanLevel::Mega, "parameters") => Some("params"),
            (CavemanLevel::Ultra | CavemanLevel::Mega, "parameter") => Some("param"),
            (CavemanLevel::Ultra | CavemanLevel::Mega, "because") => Some("<-"),
            (CavemanLevel::Ultra | CavemanLevel::Mega, "therefore" | "consequently") => Some("->"),
            (CavemanLevel::Mega, "and") => Some("&"),
            (CavemanLevel::Mega, "or") => Some("|"),
            (CavemanLevel::Mega, "approximately" | "around") => Some("~"),
            _ => None,
        };

        if let Some(short) = replacement {
            let trimmed_len = raw_word
                .trim_end_matches(|c: char| c.is_ascii_punctuation())
                .len();
            let trailing_punct = &raw_word[trimmed_len..];
            out_words.push(format!("{}{}", short, trailing_punct));
        } else {
            out_words.push(raw_word.to_string());
        }
    }

    if out_words.is_empty() {
        return fragment.to_string();
    }

    let compressed = out_words.join(" ");
    format!("{}{}{}", leading_ws, compressed, trailing_ws)
}

/// Autonomously compresses Markdown prose in-memory using `pulldown-cmark` AST parsing.
/// Leaves all headings, code blocks, inline code, URLs/links, tables, and custom workflow XML tags
/// (`TagProtector` from Headroom) 100% untouched.
/// Used automatically by the `minicode` Orchestrator and `PromptBuilder` when Caveman Mode is enabled.
#[must_use]
pub fn compress_markdown_ast_auto(markdown: &str, level: CavemanLevel) -> String {
    if !level.is_enabled() || markdown.trim().is_empty() {
        return markdown.to_string();
    }

    let protected = crate::context::budget::headroom::TagProtector::protect_tags(markdown);

    let prose_nodes = extract_prose_nodes(&protected.text_with_placeholders);
    if prose_nodes.is_empty() {
        return markdown.to_string();
    }

    let compressed_fragments: Vec<String> = prose_nodes
        .iter()
        .map(|node| compress_prose_fragment_native(&node.original, level))
        .collect();

    let (reconstructed, _) = apply_prose_replacements(
        &protected.text_with_placeholders,
        &prose_nodes,
        &compressed_fragments,
    );
    crate::context::budget::headroom::TagProtector::restore_tags(&reconstructed, &protected)
}

/// Compresses a Markdown file on disk via `compress_markdown_ast_auto(..., CavemanLevel::Full)`
/// after creating a `<stem>.original.md` backup (and rejecting any path flagged by `is_sensitive_path`).
pub fn compress_markdown_file_with_backup(
    path: &Path,
) -> std::result::Result<(usize, usize, PathBuf), String> {
    if is_sensitive_path(path) {
        return Err(format!(
            "Refusing to compress sensitive or non-prose file `{}`",
            path.display()
        ));
    }
    let orig = fs::read_to_string(path)
        .map_err(|e| format!("Failed to read `{}`: {}", path.display(), e))?;
    let orig_bytes = orig.len();
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("document");
    let backup_path = path.with_file_name(format!("{}.original.md", stem));
    if !backup_path.exists() {
        fs::write(&backup_path, &orig)
            .map_err(|e| format!("Failed to write backup `{}`: {}", backup_path.display(), e))?;
    }
    let compressed = compress_markdown_ast_auto(&orig, CavemanLevel::Full);
    let comp_bytes = compressed.len();
    fs::write(path, &compressed)
        .map_err(|e| format!("Failed to write compressed `{}`: {}", path.display(), e))?;
    Ok((orig_bytes, comp_bytes, backup_path))
}

/// Rock 6 (`juliusbrussee/caveman` "Code Mode: Script Written Once"):
/// Manages reusable verification and inspection command blocks under `.minicode/blocks/<name>.sh`
/// so repeatable multi-command workflows can be written once and invoked in 1 line (~80% fewer tokens per run).
pub struct CavemanBlockStore;

impl CavemanBlockStore {
    /// Returns the workspace directory for reusable Caveman execution blocks (`.minicode/blocks`).
    #[must_use]
    pub fn blocks_dir(workspace_root: &Path) -> PathBuf {
        workspace_root
            .join(crate::constants::WORKSPACE_DIR_NAME)
            .join("blocks")
    }

    /// Sanitizes a block name into a safe filename slug (`[a-z0-9_-]`).
    #[must_use]
    pub fn sanitize_block_name(name: &str) -> String {
        name.trim()
            .trim_end_matches(".sh")
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                    c.to_ascii_lowercase()
                } else {
                    '-'
                }
            })
            .collect::<String>()
            .trim_matches('-')
            .to_string()
    }

    /// Persists a reusable verification/inspection script to `.minicode/blocks/<slug>.sh`.
    pub fn save_block(
        workspace_root: &Path,
        name: &str,
        script_body: &str,
    ) -> std::io::Result<PathBuf> {
        let slug = Self::sanitize_block_name(name);
        if slug.is_empty() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Block name cannot be empty",
            ));
        }
        let dir = Self::blocks_dir(workspace_root);
        fs::create_dir_all(&dir)?;
        let path = dir.join(format!("{}.sh", slug));
        let mut content = String::new();
        if !script_body.trim_start().starts_with("#!") {
            content.push_str("#!/usr/bin/env bash\nset -euo pipefail\n");
        }
        content.push_str(script_body.trim());
        content.push('\n');
        fs::write(&path, content)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&path)?.permissions();
            perms.set_mode(0o755);
            let _ = fs::set_permissions(&path, perms);
        }
        Ok(path)
    }

    /// Lists all saved reusable execution blocks (`(slug, relative_path)`) in `.minicode/blocks/`.
    #[must_use]
    pub fn list_blocks(workspace_root: &Path) -> Vec<(String, String)> {
        let dir = Self::blocks_dir(workspace_root);
        let Ok(entries) = fs::read_dir(&dir) else {
            return Vec::new();
        };
        let mut list = Vec::new();
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_file() && p.extension().and_then(|e| e.to_str()) == Some("sh") {
                if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                    list.push((
                        stem.to_string(),
                        format!(
                            "{}/blocks/{}.sh",
                            crate::constants::WORKSPACE_DIR_NAME,
                            stem
                        ),
                    ));
                }
            }
        }
        list.sort_by(|a, b| a.0.cmp(&b.0));
        list
    }
}

/// Symlink-safe, size-capped read of `.caveman-active` flag file.
#[must_use]
pub fn read_flag_safe(flag_path: &Path) -> Option<CavemanLevel> {
    let st = fs::symlink_metadata(flag_path).ok()?;
    if st.file_type().is_symlink() || !st.is_file() || st.len() > MAX_FLAG_BYTES {
        return None;
    }

    let mut file = OpenOptions::new().read(true).open(flag_path).ok()?;
    let mut buf = [0_u8; MAX_FLAG_BYTES as usize];
    let n = file.read(&mut buf).ok()?;
    let content = std::str::from_utf8(&buf[..n]).ok()?;
    Some(CavemanLevel::from_str_loose(content))
}

/// Symlink-safe atomic write of `.caveman-active` flag file.
pub fn write_flag_safe(flag_path: &Path, level: CavemanLevel) -> std::io::Result<()> {
    if let Ok(st) = fs::symlink_metadata(flag_path) {
        if st.file_type().is_symlink() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "Refusing to write to symlinked .caveman-active file",
            ));
        }
    }
    if let Some(parent) = flag_path.parent() {
        fs::create_dir_all(parent)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let meta = fs::metadata(parent)?;
            let current_uid = unsafe { libc::getuid() };
            if meta.uid() != current_uid {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "Parent directory UID mismatch",
                ));
            }
        }
    }
    let tmp_path = flag_path.with_extension(format!("tmp.{}", std::process::id()));
    fs::write(&tmp_path, level.as_str())?;
    fs::rename(&tmp_path, flag_path)?;
    Ok(())
}

pub fn default_flag_path(workspace_root: &Path) -> PathBuf {
    workspace_root
        .join(crate::constants::WORKSPACE_DIR_NAME)
        .join(".caveman-active")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_caveman_level_parsing_and_cycling() {
        assert_eq!(CavemanLevel::from_str_loose("off"), CavemanLevel::Off);
        assert_eq!(CavemanLevel::from_str_loose("lite"), CavemanLevel::Lite);
        assert_eq!(CavemanLevel::from_str_loose("full"), CavemanLevel::Full);
        assert_eq!(CavemanLevel::from_str_loose("ultra"), CavemanLevel::Ultra);
        assert_eq!(CavemanLevel::from_str_loose("mega"), CavemanLevel::Mega);
        assert_eq!(CavemanLevel::from_str_loose("megacave"), CavemanLevel::Mega);
        assert_eq!(CavemanLevel::Lite.cycle_next(), CavemanLevel::Full);
        assert_eq!(CavemanLevel::Full.cycle_next(), CavemanLevel::Ultra);
        assert_eq!(CavemanLevel::Ultra.cycle_next(), CavemanLevel::Mega);
        assert_eq!(CavemanLevel::Mega.cycle_next(), CavemanLevel::Lite);
        assert!(!CavemanLevel::Off.is_enabled());
        assert!(CavemanLevel::Full.is_enabled());
        assert!(CavemanLevel::Mega.is_enabled());
    }

    #[test]
    fn test_natural_trigger_detection() {
        assert_eq!(
            CavemanLevel::detect_natural_trigger("please talk like caveman"),
            Some(CavemanLevel::Full)
        );
        assert_eq!(
            CavemanLevel::detect_natural_trigger("enable ultracave"),
            Some(CavemanLevel::Ultra)
        );
        assert_eq!(
            CavemanLevel::detect_natural_trigger("switch to megacave"),
            Some(CavemanLevel::Mega)
        );
        assert_eq!(
            CavemanLevel::detect_natural_trigger("stop caveman"),
            Some(CavemanLevel::Off)
        );
        assert_eq!(
            CavemanLevel::detect_natural_trigger("fix the bug in src/main.rs"),
            None
        );
    }

    #[test]
    fn test_autonomous_ast_markdown_compression_preserves_code_structure_and_negations() {
        let md = "# Architecture Heading\n\nYou should always make sure to run the test suite in order to verify the application configuration, and never drop not or only or except.\n\n```rust\nfn main() {\n    println!(\"The code inside a block must never be modified!\");\n}\n```\n\nCheck [the documentation](https://example.com) for more information.\n";
        let compressed = compress_markdown_ast_auto(md, CavemanLevel::Full);

        // Heading, code block, and link text/url must remain 100% untouched
        assert!(compressed.contains("# Architecture Heading"));
        assert!(
            compressed.contains("println!(\"The code inside a block must never be modified!\");")
        );
        assert!(compressed.contains("[the documentation](https://example.com)"));

        // Prose should be compressed AND all negation/scope guards (never, not, only, except) must survive
        assert!(compressed.contains(
            "ensure run test suite to verify app config, and never drop not or only or except."
        ));
        assert!(compressed.len() < md.len());
    }

    #[test]
    fn test_is_sensitive_path_guard() {
        assert!(is_sensitive_path(Path::new(".env.local")));
        assert!(is_sensitive_path(Path::new("/home/user/.ssh/id_rsa")));
        assert!(is_sensitive_path(Path::new("src/main.rs")));
        assert!(is_sensitive_path(Path::new("NOTES.original.md")));
        assert!(!is_sensitive_path(Path::new("README.md")));
        assert!(!is_sensitive_path(Path::new("docs/architecture.md")));
    }

    #[test]
    fn test_system_mega_and_cavecrew_blocks() {
        assert!(build_caveman_system_block(false, "full").is_empty());
        let sys = build_caveman_system_block(true, "ultra");
        assert!(sys.contains("<caveman_mode level=\"ultra\">"));
        assert!(sys.contains("ASD-STE100"));
        assert!(sys.contains("Native Core 5-Step Simplicity Ladder"));
        assert!(sys.contains(SENTINEL));

        let mega_sys = build_caveman_system_block(true, "mega");
        assert!(mega_sys.contains("<caveman_mode level=\"mega\">"));
        assert!(mega_sys.contains("telegraphic"));

        let cc = cavecrew_subagent_block(true, "Scout", "full");
        assert!(cc.contains("caveman-explore"));
        assert!(cc.contains("FastContext"));
        let cc_rev = cavecrew_subagent_block(true, "Reviewer", "full");
        assert!(cc_rev.contains("cavecrew-reviewer"));
        let cc_build = cavecrew_subagent_block(true, "Coder", "full");
        assert!(cc_build.contains("cavecrew-builder"));
        assert!(cc_build.contains("too-big:"));
        assert!(cc_build.contains("regressed:"));
    }
}

//! Native Rust Caveman Mode Engine (`caveman-rs` integration for `minicode`).
//!
//! Cuts conversational, context, and subagent prose token usage by ~65-75% while preserving
//! 100% of technical substance, exact code blocks, symbols, file paths, and error strings.
//! Includes:
//! - Multi-intensity Caveman prompt & recency enforcement (`lite`, `full`, `ultra`)
//! - `cavecrew` compressed subagent output contracts for `MiniPub` workers (`~60%` smaller handoffs)
//! - Autonomous `pulldown-cmark` AST-based Markdown prose compressor (leaves headings, code blocks, tables, and links untouched)
//! - Symlink-safe `.caveman-active` flag persistence

use pulldown_cmark::{Event, Parser, Tag, TagEnd};
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Read;
use std::path::{Path, PathBuf};

pub const SENTINEL: &str = "Respond terse like smart caveman";
const MAX_FLAG_BYTES: u64 = 64;

/// Supported Caveman compression intensity levels (`lite`, `full`, `ultra`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum CavemanLevel {
    Off,
    Lite,
    #[default]
    Full,
    Ultra,
}

impl CavemanLevel {
    /// Parses a mode/level string loosely.
    #[must_use]
    pub fn from_str_loose(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "off" | "disabled" | "false" | "none" | "normal" | "stop" => Self::Off,
            "lite" | "light" | "low" => Self::Lite,
            "full" | "on" | "enabled" | "true" | "default" | "caveman" | "medium" => Self::Full,
            "ultra" | "max" | "high" => Self::Ultra,
            _ => Self::Full,
        }
    }

    /// Canonical configuration string for this intensity level (`"lite"`, `"full"`, `"ultra"`).
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Lite => "lite",
            Self::Full => "full",
            Self::Ultra => "ultra",
        }
    }

    /// Cycles through active intensity levels (`lite -> full -> ultra -> lite`).
    #[must_use]
    pub fn cycle_next(&self) -> Self {
        match self {
            Self::Lite => Self::Full,
            Self::Full => Self::Ultra,
            Self::Ultra | Self::Off => Self::Lite,
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
        }
    }

    /// Returns true if Caveman mode is active.
    #[must_use]
    pub fn is_enabled(&self) -> bool {
        !matches!(self, Self::Off)
    }
}

/// Builds the `<caveman_mode>` system prompt block when Caveman Mode is enabled.
#[must_use]
pub fn build_caveman_system_block(enabled: bool, level_str: &str) -> String {
    if !enabled {
        return String::new();
    }
    let level = {
        let parsed = CavemanLevel::from_str_loose(level_str);
        if parsed == CavemanLevel::Off {
            CavemanLevel::Full
        } else {
            parsed
        }
    };

    let intensity_rules = match level {
        CavemanLevel::Lite => {
            "Level: **lite** — Drop filler, pleasantries, and hedging. Keep articles and full sentences. Professional, tight, zero fluff."
        }
        CavemanLevel::Full | CavemanLevel::Off => {
            "Level: **full** — Drop articles (a/an/the), filler (just/really/basically/actually/simply), pleasantries (sure/certainly/happy to), and hedging. Sentence fragments OK. Short synonyms (`fix` not `implement a solution for`, `big` not `extensive`). Pattern: `[thing] [action] [reason]. [next step].`"
        }
        CavemanLevel::Ultra => {
            "Level: **ultra** — Maximum compression. Abbreviate prose words (DB/auth/config/req/res/fn/impl), strip conjunctions, use arrows for causality (`X -> Y`), one word when one word is enough. NEVER abbreviate code symbols, function names, file paths, or error strings."
        }
    };

    format!(
        "\n<caveman_mode level=\"{}\">\n\
  {}: All technical substance stay. Only fluff die.\n\
  - **Persistence**: ACTIVE EVERY RESPONSE across all turns. No filler drift.\n\
  - **Intensity**: {}\n\
  - **Code Reviews (`caveman-review`)**: One line per finding: `<file>:L<line>: <severity>: <problem>. <fix>.` No throat-clearing.\n\
  - **Commits (`caveman-commit`)**: Conventional Commits `<type>(<scope>): <imperative summary>` (<=50 chars subject), body only when non-obvious *why* or breaking change.\n\
  - **Auto-Clarity Override**: Automatically switch to normal clear English for security warnings, destructive/irreversible confirmations, or multi-step sequences where fragment order could be misread. Resume caveman immediately after.\n\
  - **Strict Boundaries**: NEVER compress or alter application code written via `write_file`/`patch_file`, code blocks, inline code (`backticks`), file paths, URLs, error messages, or user-facing UI copy inside the user's project.\n\
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
    let level = {
        let parsed = CavemanLevel::from_str_loose(level_str);
        if parsed == CavemanLevel::Off {
            CavemanLevel::Full
        } else {
            parsed
        }
    };
    format!(
        "  <caveman_active level=\"{}\">\n    {} (level: `{}`). Drop articles/filler/pleasantries in conversational prose (`[thing] [action] [reason]. [next step].`). Keep all code, file paths, symbols, tool calls, and `ask_user` options 100% exact.\n  </caveman_active>\n",
        level.as_str(),
        SENTINEL,
        level.as_str()
    )
}

/// Builds the `cavecrew` compressed subagent output contract for `MiniPub` workers when Caveman Mode is active.
/// Shrinks subagent tool-result payloads by ~60% so the parent Orchestrator's context window lasts much longer.
#[must_use]
pub fn cavecrew_subagent_block(enabled: bool, role_badge: &str, level_str: &str) -> String {
    if !enabled {
        return String::new();
    }
    let level = {
        let parsed = CavemanLevel::from_str_loose(level_str);
        if parsed == CavemanLevel::Off {
            CavemanLevel::Full
        } else {
            parsed
        }
    };
    let role_lower = role_badge.trim().to_ascii_lowercase();
    let contract = if matches!(role_lower.as_str(), "scout" | "planner" | "architect") {
        "Contract (`cavecrew-investigator`):\n\
  <Header>:\n\
  - path:line — `symbol` — short technical note\n\
  totals: <counts>."
    } else if matches!(
        role_lower.as_str(),
        "reviewer" | "debugger" | "security" | "tester"
    ) {
        "Contract (`cavecrew-reviewer`):\n\
  path:line: <HIGH|MED|LOW>: <problem>. <fix>.\n\
  totals: <counts> (or `No issues.`)."
    } else {
        "Contract (`cavecrew-builder`):\n\
  <path:line-range> — <change in <=10 words>.\n\
  verified: <check/test result>."
    };

    format!(
        "\n\n<cavecrew_output_contract level=\"{}\">\n\
You are running in Caveman (`cavecrew`) compressed subagent mode. Your final report is injected into the parent Orchestrator's context window — emit ultra-terse, high-signal technical output (~60% fewer tokens, zero prose fluff, 100% exact paths/symbols):\n\
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

/// Compresses a single pure-prose fragment in native Rust according to `CavemanLevel` (`Lite`, `Full`, `Ultra`).
/// Preserves leading/trailing whitespace so surrounding inline code or emphasis spans remain properly spaced.
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

    // 1. Phrase-level reductions (Lite + Full + Ultra)
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

    if matches!(level, CavemanLevel::Full | CavemanLevel::Ultra) {
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

    // 2. Token-level filtering & synonym shortening
    let mut out_words = Vec::new();
    for raw_word in text.split_whitespace() {
        let lower = raw_word
            .trim_matches(|c: char| c.is_ascii_punctuation())
            .to_ascii_lowercase();

        // Lite + Full + Ultra: drop filler adverbs & pleasantries
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

        // Full + Ultra: drop articles (a, an, the) and verbose auxiliaries
        if matches!(level, CavemanLevel::Full | CavemanLevel::Ultra)
            && matches!(lower.as_str(), "a" | "an" | "the")
        {
            continue;
        }

        // Word-level short synonyms / ultra abbreviations (preserving trailing punctuation)
        let trailing_punct: String = raw_word
            .chars()
            .rev()
            .take_while(|c| c.is_ascii_punctuation())
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();

        let replacement = match (level, lower.as_str()) {
            (CavemanLevel::Full | CavemanLevel::Ultra, "utilize" | "utilizes") => Some("use"),
            (CavemanLevel::Full | CavemanLevel::Ultra, "extensive") => Some("big"),
            (CavemanLevel::Full | CavemanLevel::Ultra, "configuration") => Some("config"),
            (CavemanLevel::Full | CavemanLevel::Ultra, "application") => Some("app"),
            (CavemanLevel::Full | CavemanLevel::Ultra, "repository") => Some("repo"),
            (CavemanLevel::Full | CavemanLevel::Ultra, "directory") => Some("dir"),
            (CavemanLevel::Full | CavemanLevel::Ultra, "information") => Some("info"),
            (CavemanLevel::Full | CavemanLevel::Ultra, "environment") => Some("env"),
            (CavemanLevel::Full | CavemanLevel::Ultra, "dependencies") => Some("deps"),
            (CavemanLevel::Ultra, "database") => Some("DB"),
            (CavemanLevel::Ultra, "authentication") => Some("auth"),
            (CavemanLevel::Ultra, "function") => Some("fn"),
            (CavemanLevel::Ultra, "implementation") => Some("impl"),
            (CavemanLevel::Ultra, "request") => Some("req"),
            (CavemanLevel::Ultra, "response") => Some("res"),
            (CavemanLevel::Ultra, "parameters") => Some("params"),
            (CavemanLevel::Ultra, "parameter") => Some("param"),
            (CavemanLevel::Ultra, "because") => Some("<-"),
            (CavemanLevel::Ultra, "therefore" | "consequently") => Some("->"),
            _ => None,
        };

        if let Some(short) = replacement {
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
/// Leaves all headings, code blocks, inline code, URLs/links, and tables 100% untouched.
/// Used automatically by the `minicode` Orchestrator and `PromptBuilder` when Caveman Mode is enabled.
#[must_use]
pub fn compress_markdown_ast_auto(markdown: &str, level: CavemanLevel) -> String {
    if !level.is_enabled() || markdown.trim().is_empty() {
        return markdown.to_string();
    }

    let prose_nodes = extract_prose_nodes(markdown);
    if prose_nodes.is_empty() {
        return markdown.to_string();
    }

    let compressed_fragments: Vec<String> = prose_nodes
        .iter()
        .map(|node| compress_prose_fragment_native(&node.original, level))
        .collect();

    let (reconstructed, _) = apply_prose_replacements(markdown, &prose_nodes, &compressed_fragments);
    reconstructed
}

/// Symlink-safe, size-capped read of `.caveman-active` flag file.
#[must_use]
pub fn read_flag_safe(flag_path: &Path) -> Option<CavemanLevel> {
    let st = fs::symlink_metadata(flag_path).ok()?;
    if st.file_type().is_symlink() || !st.is_file() || st.len() > MAX_FLAG_BYTES {
        return None;
    }

    let mut file = OpenOptions::new().read(true).open(flag_path).ok()?;
    let mut buf = vec![0_u8; MAX_FLAG_BYTES as usize];
    let n = file.read(&mut buf).ok()?;
    let content = String::from_utf8(buf[..n].to_vec()).ok()?;
    Some(CavemanLevel::from_str_loose(&content))
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
        assert_eq!(CavemanLevel::Lite.cycle_next(), CavemanLevel::Full);
        assert_eq!(CavemanLevel::Full.cycle_next(), CavemanLevel::Ultra);
        assert_eq!(CavemanLevel::Ultra.cycle_next(), CavemanLevel::Lite);
        assert!(!CavemanLevel::Off.is_enabled());
        assert!(CavemanLevel::Full.is_enabled());
    }

    #[test]
    fn test_autonomous_ast_markdown_compression_preserves_code_and_structure() {
        let md = "# Architecture Heading\n\nYou should always make sure to run the test suite in order to verify the application configuration.\n\n```rust\nfn main() {\n    println!(\"The code inside a block must never be modified!\");\n}\n```\n\nCheck [the documentation](https://example.com) for more information.\n";
        let compressed = compress_markdown_ast_auto(md, CavemanLevel::Full);

        // Heading, code block, and link text/url must remain 100% untouched
        assert!(compressed.contains("# Architecture Heading"));
        assert!(compressed.contains("println!(\"The code inside a block must never be modified!\");"));
        assert!(compressed.contains("[the documentation](https://example.com)"));

        // Prose should be compressed ("You should always make sure to" -> "ensure", "in order to" -> "to", "application configuration" -> "app config")
        assert!(compressed.contains("ensure run test suite to verify app config."));
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
    fn test_system_and_cavecrew_blocks() {
        assert!(build_caveman_system_block(false, "full").is_empty());
        let sys = build_caveman_system_block(true, "ultra");
        assert!(sys.contains("<caveman_mode level=\"ultra\">"));
        assert!(sys.contains(SENTINEL));

        let cc = cavecrew_subagent_block(true, "Scout", "full");
        assert!(cc.contains("cavecrew-investigator"));
        let cc_rev = cavecrew_subagent_block(true, "Reviewer", "full");
        assert!(cc_rev.contains("cavecrew-reviewer"));
    }
}

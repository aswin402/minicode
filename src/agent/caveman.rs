//! Native Rust Caveman Mode Engine (`caveman-rs` integration for `minicode`).
//!
//! Cuts conversational and subagent prose token usage by ~65-75% while preserving
//! 100% of technical substance, exact code blocks, symbols, file paths, and error strings.
//! Includes:
//! - Multi-intensity Caveman prompt & recency enforcement (`lite`, `full`, `ultra`, `wenyan-*`)
//! - `cavecrew` compressed subagent output contracts for `MiniPub` workers (`~60%` smaller handoffs)
//! - `pulldown-cmark` AST-based Markdown prose compressor (leaves headings, code blocks, tables, and links untouched)
//! - Symlink-safe `.caveman-active` flag & `.caveman-history.jsonl` token savings telemetry

use crate::agent::provider::{CompletionOptions, Provider};
use crate::agent::types::Message;
use futures::StreamExt;
use pulldown_cmark::{Event, Parser, Tag, TagEnd};
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub const SENTINEL: &str = "Respond terse like smart caveman";
const MAX_FLAG_BYTES: u64 = 64;

/// Supported Caveman compression intensity levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum CavemanLevel {
    Off,
    Lite,
    #[default]
    Full,
    Ultra,
    WenyanLite,
    WenyanFull,
    WenyanUltra,
}

impl CavemanLevel {
    /// Parses a mode/level string loosely.
    #[must_use]
    pub fn from_str_loose(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "off" | "disabled" | "false" | "none" | "normal" | "stop" => Self::Off,
            "lite" | "light" => Self::Lite,
            "full" | "on" | "enabled" | "true" | "default" | "caveman" => Self::Full,
            "ultra" | "max" => Self::Ultra,
            "wenyan-lite" | "wenyan_lite" => Self::WenyanLite,
            "wenyan" | "wenyan-full" | "wenyan_full" => Self::WenyanFull,
            "wenyan-ultra" | "wenyan_ultra" => Self::WenyanUltra,
            _ => Self::Full,
        }
    }

    /// Canonical configuration string for this intensity level.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Lite => "lite",
            Self::Full => "full",
            Self::Ultra => "ultra",
            Self::WenyanLite => "wenyan-lite",
            Self::WenyanFull => "wenyan-full",
            Self::WenyanUltra => "wenyan-ultra",
        }
    }

    /// Concise status bar / settings label.
    #[must_use]
    pub fn badge_label(&self) -> &'static str {
        match self {
            Self::Off => "disabled",
            Self::Lite => "caveman:lite",
            Self::Full => "caveman",
            Self::Ultra => "caveman:ultra",
            Self::WenyanLite => "caveman:wenyan-lite",
            Self::WenyanFull => "caveman:wenyan",
            Self::WenyanUltra => "caveman:wenyan-ultra",
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
            Self::WenyanLite => 0.65,
            Self::WenyanFull => 0.85,
            Self::WenyanUltra => 0.90,
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
        CavemanLevel::WenyanLite => {
            "Level: **wenyan-lite** — Semi-classical terse register (文言). Drop filler and hedging while keeping clear structure and exact code/symbol identifiers."
        }
        CavemanLevel::WenyanFull => {
            "Level: **wenyan-full** — Classical Chinese terseness (文言文, ~80-90% character reduction) for prose explanations while keeping all code blocks, file paths, CLI commands, and identifiers in exact original form."
        }
        CavemanLevel::WenyanUltra => {
            "Level: **wenyan-ultra** — Extreme classical abbreviation with causal arrows (`->`) for prose while preserving all code, symbols, paths, and commands verbatim."
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
/// that sit outside headings, code blocks, links, and tables.
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

/// Builds the structured prompt to compress numbered prose fragments into Caveman style.
#[must_use]
pub fn build_compress_prompt(fragments: &[String]) -> String {
    let mut prompt = String::from(
        "Compress these text fragments into caveman style.\n\n\
        STRICT RULES:\n\
        - Return ONLY the compressed fragments, in the exact same order.\n\
        - Output format must be: `[index]: [compressed text]` (e.g. `0: New ref each render.`)\n\
        - Keep all code variables, function names, error strings, exact numbers, and filenames unchanged.\n\
        - Only drop articles (a/an/the), pleasantries, hedging, and filler words.\n\
        - Keep the output extremely brief.\n\
        - DO NOT wrap the output in code blocks.\n\n\
        FRAGMENTS TO COMPRESS:\n",
    );

    for (i, frag) in fragments.iter().enumerate() {
        prompt.push_str(&format!("{}: {}\n", i, frag));
    }

    prompt
}

/// Parses `[index]: [compressed text]` lines returned by the LLM.
#[must_use]
pub fn parse_compressed_fragments(response: &str, count: usize) -> Vec<String> {
    let mut results = vec![String::new(); count];

    for line in response.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(colon_idx) = line.find(':') {
            if let Ok(idx) = line[..colon_idx].trim().parse::<usize>() {
                if idx < count {
                    results[idx] = line[colon_idx + 1..].trim().to_string();
                }
            }
        }
    }

    results
}

/// Reconstructs a Markdown document by replacing `ProseNode` byte ranges in reverse order,
/// guaranteeing that headings, code blocks, links, and tables remain 100% untouched.
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

/// Compresses a Markdown file in-place using `minicode`'s active `Provider` and AST prose extraction.
/// Saves a backup to `<filename>.original.md` before writing.
pub async fn compress_markdown_file_with_provider(
    file_path: &Path,
    provider: &Arc<dyn Provider>,
    model: &str,
    dry_run: bool,
) -> std::result::Result<String, String> {
    if is_sensitive_path(file_path) {
        return Err(
            "Refusing to compress: file path is a code/config file or contains sensitive names/extensions."
                .to_string(),
        );
    }

    let original_content =
        fs::read_to_string(file_path).map_err(|e| format!("Could not read file: {}", e))?;

    let prose_nodes = extract_prose_nodes(&original_content);
    if prose_nodes.is_empty() {
        return Ok("ℹ No compressible prose blocks found in file.".to_string());
    }

    let original_fragments: Vec<String> = prose_nodes.iter().map(|n| n.original.clone()).collect();

    if dry_run {
        return Ok(format!(
            "✔ Dry run: would compress {} prose block(s) in `{}` while keeping all headings, code blocks, links, and tables untouched.",
            original_fragments.len(),
            file_path.display()
        ));
    }

    let prompt = build_compress_prompt(&original_fragments);
    let messages = vec![Message::user(prompt)];
    let options = CompletionOptions {
        model: model.to_string(),
        temperature: 0.1,
        max_tokens: 4096,
        system_instruction: Some(
            "You are an AST prose compression engine. Compress numbered text fragments into terse caveman style without altering technical terms, identifiers, or numbers."
                .to_string(),
        ),
        thinking_budget: None,
        reasoning_effort: None,
    };

    let mut stream = provider
        .stream_completion(&messages, &[], &options)
        .await
        .map_err(|e| format!("Provider request failed: {}", e))?;

    let mut response_text = String::new();
    while let Some(chunk_res) = stream.next().await {
        if let Ok(crate::agent::provider::StreamChunk::Delta(delta)) = chunk_res {
            response_text.push_str(&delta);
        }
    }

    let compressed_fragments =
        parse_compressed_fragments(&response_text, original_fragments.len());
    let (reconstructed, replaced_count) =
        apply_prose_replacements(&original_content, &prose_nodes, &compressed_fragments);

    if replaced_count == 0 {
        return Err("Model did not return valid indexed fragments; original file left untouched.".to_string());
    }

    let backup_path = file_path.with_extension("original.md");
    if !backup_path.exists() {
        fs::write(&backup_path, &original_content)
            .map_err(|e| format!("Could not create backup file: {}", e))?;
    }

    fs::write(file_path, &reconstructed)
        .map_err(|e| format!("Could not write compressed markdown: {}", e))?;

    let orig_bytes = original_content.len();
    let new_bytes = reconstructed.len();
    let saved_pct = if orig_bytes > 0 && new_bytes < orig_bytes {
        ((orig_bytes - new_bytes) as f64 / orig_bytes as f64) * 100.0
    } else {
        0.0
    };

    Ok(format!(
        "✔ Compressed {}/{} prose blocks in `{}` ({:.1}% byte reduction; backup at `{}`).",
        replaced_count,
        prose_nodes.len(),
        file_path.display(),
        saved_pct,
        backup_path.display()
    ))
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

/// Formats the `/caveman help` quick-reference card.
#[must_use]
pub fn format_caveman_help(active_enabled: bool, active_level: &str) -> String {
    let status = if active_enabled {
        format!("enabled ({})", active_level)
    } else {
        "disabled".to_string()
    };
    format!(
        "🪨 **Caveman Mode — Native Rust Token Compression Engine** (Status: `{}`)\n\n\
        • `/caveman` — Toggle Caveman Mode `enabled` / `disabled`\n\
        • `/caveman lite | full | ultra` — Switch compression intensity (`full` = ~75% fewer prose tokens)\n\
        • `/caveman wenyan-lite | wenyan | wenyan-ultra` — Classical terse 文言文 register\n\
        • `/caveman compress <file.md>` — AST-safe Markdown prose compression (`pulldown-cmark`, saves `<file>.original.md`)\n\
        • `/caveman off` — Return to standard verbose prose\n\
        • Also configurable in `/settings` → `Autonomy` → `Caveman Mode`",
        status
    )
}

#[allow(dead_code)]
pub fn default_flag_path(workspace_root: &Path) -> PathBuf {
    workspace_root
        .join(crate::constants::WORKSPACE_DIR_NAME)
        .join(".caveman-active")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_caveman_level_parsing_and_badges() {
        assert_eq!(CavemanLevel::from_str_loose("off"), CavemanLevel::Off);
        assert_eq!(CavemanLevel::from_str_loose("lite"), CavemanLevel::Lite);
        assert_eq!(CavemanLevel::from_str_loose("full"), CavemanLevel::Full);
        assert_eq!(CavemanLevel::from_str_loose("ultra"), CavemanLevel::Ultra);
        assert_eq!(
            CavemanLevel::from_str_loose("wenyan"),
            CavemanLevel::WenyanFull
        );
        assert_eq!(CavemanLevel::Full.badge_label(), "caveman");
        assert_eq!(CavemanLevel::Ultra.badge_label(), "caveman:ultra");
        assert!(!CavemanLevel::Off.is_enabled());
        assert!(CavemanLevel::Full.is_enabled());
    }

    #[test]
    fn test_extract_prose_nodes_preserves_code_headings_links_tables() {
        let md = "# Architecture Heading\n\nHere is a verbose paragraph of prose.\n\n```rust\nfn main() {\n    println!(\"Never touch code!\");\n}\n```\n\nCheck [the documentation](https://example.com) for details.\n";
        let nodes = extract_prose_nodes(md);
        // Should extract "Here is a verbose paragraph of prose.", "Check ", " for details."
        // Must NOT extract "Architecture Heading", "fn main()...", or "the documentation"
        assert!(!nodes.is_empty());
        for node in &nodes {
            assert!(!node.original.contains("Architecture Heading"));
            assert!(!node.original.contains("Never touch code!"));
            assert!(!node.original.contains("the documentation"));
        }

        let replacements: Vec<String> = nodes
            .iter()
            .map(|n| {
                if n.original.contains("verbose paragraph") {
                    "Terse prose.".to_string()
                } else {
                    n.original.clone()
                }
            })
            .collect();

        let (reconstructed, count) = apply_prose_replacements(md, &nodes, &replacements);
        assert_eq!(count, nodes.len());
        assert!(reconstructed.contains("# Architecture Heading"));
        assert!(reconstructed.contains("Terse prose."));
        assert!(reconstructed.contains("println!(\"Never touch code!\");"));
        assert!(reconstructed.contains("[the documentation](https://example.com)"));
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

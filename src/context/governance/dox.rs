use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

/// Canonical rule files read by `minicode` at runtime.
/// Foreign agent rule files (`AGENTS.md`, `CLAUDE.md`, `.cursorrules`, etc.) are NEVER
/// injected directly; instead, `sync_and_harvest_rules` distills their useful project
/// rules into `.minicode/minicode.md` (or scoped `minicode.md`) to prevent cross-agent confusion.
const CANONICAL_RULE_FILES: &[&str] = &[
    ".minicode/minicode.md",
    ".minicode/MINICODE.md",
    "minicode.md",
    "MINICODE.md",
    ".minicode/RULES.md",
    ".minicode/rules.md",
    "RULES.md",
    ".rules.md",
    "minikit_docs/core/spec.md",
    "onpkg_docs/core/spec.md",
    ".dox",
    ".dox.md",
    ".minicode/dox.md",
];

/// Foreign / legacy agent instruction files whose useful project rules are harvested
/// into `.minicode/minicode.md` while filtering out foreign agent personas & tool names.
const FOREIGN_AGENT_RULE_FILES: &[&str] = &[
    "AGENTS.md",
    ".agents.md",
    "CLAUDE.md",
    ".claude/CLAUDE.md",
    ".cursorrules",
    ".windsurfrules",
    ".clinerules",
    ".github/copilot-instructions.md",
];

pub const DEFAULT_MAX_DOX_CHARS: usize = 4500;
const SUBDIR_INJECT_MAX_CHARS: usize = 2500;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct RulesSyncManifest {
    /// Maps relative foreign source path -> (mtime_secs, byte_len)
    #[serde(default)]
    harvested_sources: BTreeMap<String, (u64, u64)>,
}

pub struct DoxEngine;

static DOX_SYNC_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
static INJECTED_SUBDIR_RULES: std::sync::OnceLock<std::sync::Mutex<HashSet<PathBuf>>> =
    std::sync::OnceLock::new();

impl DoxEngine {
    /// Resolves the canonical repository root even when `workspace_root` points inside
    /// an isolated subagent worktree (`.minicode/worktrees/<id>`).
    pub fn canonical_workspace_root(workspace_root: &Path) -> PathBuf {
        if let Some(wt_dir) = workspace_root.parent() {
            if wt_dir.file_name().and_then(|n| n.to_str())
                == Some(crate::constants::WORKTREES_DIR_NAME)
            {
                if let Some(minicode_dir) = wt_dir.parent() {
                    if minicode_dir.file_name().and_then(|n| n.to_str())
                        == Some(crate::constants::WORKSPACE_DIR_NAME)
                    {
                        if let Some(repo_root) = minicode_dir.parent() {
                            return repo_root.to_path_buf();
                        }
                    }
                }
            }
        }
        workspace_root.to_path_buf()
    }

    /// Automatically scans workspace root (and active file ancestor directories) for foreign
    /// agent rule files (`AGENTS.md`, `CLAUDE.md`, `.cursorrules`, etc.), strips out foreign
    /// agent persona/tool noise, and saves useful project engineering rules into `.minicode/minicode.md`.
    pub fn sync_and_harvest_rules(workspace_root: &Path, active_files: &[impl AsRef<Path>]) {
        let root_buf = Self::canonical_workspace_root(workspace_root);
        let root = root_buf.as_path();
        if !root.exists() {
            return;
        }

        let _guard = DOX_SYNC_LOCK.lock().unwrap_or_else(|e| e.into_inner());

        let minicode_dir = root.join(crate::constants::WORKSPACE_DIR_NAME);
        let sync_path = minicode_dir.join(crate::constants::RULES_SYNC_STATE_FILE);
        let canonical_root_rules = root.join(crate::constants::RULES_MD_FILE);
        let legacy_root_rules = root.join(crate::constants::LEGACY_RULES_MD_FILE);

        // Migrate legacy .minicode/RULES.md -> .minicode/minicode.md if minicode.md does not exist yet
        if !canonical_root_rules.is_file() && legacy_root_rules.is_file() {
            if let Ok(legacy_content) = fs::read_to_string(&legacy_root_rules) {
                let _ = fs::create_dir_all(&minicode_dir);
                let _ = fs::write(&canonical_root_rules, legacy_content);
            }
        }

        let mut manifest: RulesSyncManifest = fs::read_to_string(&sync_path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();

        let canonical_exists = canonical_root_rules.is_file();
        let mut dirty = false;

        // Collect directories to inspect: canonical root (+ worktree root if distinct) + ancestor dirs of active_files
        let mut dirs_to_check: Vec<PathBuf> = vec![root.to_path_buf()];
        let mut seen_dirs: HashSet<PathBuf> = HashSet::new();
        seen_dirs.insert(root.to_path_buf());
        if workspace_root != root
            && workspace_root.exists()
            && seen_dirs.insert(workspace_root.to_path_buf())
        {
            dirs_to_check.push(workspace_root.to_path_buf());
        }

        for base_root in [root, workspace_root] {
            for file in active_files {
                let file_ref = file.as_ref();
                let full_path = if file_ref.is_absolute() {
                    file_ref.to_path_buf()
                } else {
                    base_root.join(file_ref)
                };
                let mut cur = if full_path.is_dir() {
                    Some(full_path.as_path())
                } else {
                    full_path.parent()
                };
                while let Some(dir) = cur {
                    if dir == base_root || !dir.starts_with(base_root) {
                        break;
                    }
                    if seen_dirs.insert(dir.to_path_buf()) {
                        dirs_to_check.push(dir.to_path_buf());
                    }
                    cur = dir.parent();
                }
            }
            if workspace_root == root {
                break;
            }
        }

        let mut harvested_blocks: Vec<(String, String)> = Vec::new();
        let mut seen_rel_keys: HashSet<String> = HashSet::new();

        for dir in &dirs_to_check {
            for foreign_rel in FOREIGN_AGENT_RULE_FILES {
                let candidate = dir.join(foreign_rel);
                if !candidate.is_file() {
                    continue;
                }
                let Ok(meta) = fs::metadata(&candidate) else {
                    continue;
                };
                let mtime = meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                let byte_len = meta.len();

                let rel_key = candidate
                    .strip_prefix(workspace_root)
                    .or_else(|_| candidate.strip_prefix(root))
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|_| foreign_rel.to_string());
                if !seen_rel_keys.insert(rel_key.clone()) {
                    continue;
                }

                let already_synced = canonical_exists
                    && manifest
                        .harvested_sources
                        .get(&rel_key)
                        .map(|&(prev_mtime, prev_len)| prev_mtime == mtime && prev_len == byte_len)
                        .unwrap_or(false);

                if already_synced {
                    continue;
                }

                if let Ok(raw) = fs::read_to_string(&candidate) {
                    let distilled = Self::distill_useful_rules(&rel_key, &raw);
                    manifest
                        .harvested_sources
                        .insert(rel_key.clone(), (mtime, byte_len));
                    dirty = true;
                    if !distilled.trim().is_empty() {
                        harvested_blocks.push((rel_key, distilled));
                    }
                }
            }
        }

        if !dirty && canonical_exists {
            return;
        }

        if !harvested_blocks.is_empty() {
            let _ = fs::create_dir_all(&minicode_dir);
            let mut existing_rules = fs::read_to_string(&canonical_root_rules)
                .or_else(|_| fs::read_to_string(&legacy_root_rules))
                .unwrap_or_else(|_| {
                    "# minicode — Canonical Workspace Rules (`.minicode/minicode.md`)\n\n\
                     > Single canonical source of project rules for `minicode`. Useful project rules from repository files are consolidated here automatically.\n"
                        .to_string()
                });

            for (source_key, distilled_body) in harvested_blocks {
                let begin_marker = format!("<!-- BEGIN HARVESTED RULES: {} -->", source_key);
                let end_marker = format!("<!-- END HARVESTED RULES: {} -->", source_key);
                let block = format!(
                    "{}\n## Project Rules (Distilled from `{}`)\n{}\n{}",
                    begin_marker,
                    source_key,
                    distilled_body.trim(),
                    end_marker
                );

                if let (Some(start_idx), Some(end_idx)) = (
                    existing_rules.find(&begin_marker),
                    existing_rules.find(&end_marker),
                ) {
                    if end_idx >= start_idx {
                        let replace_end = end_idx + end_marker.len();
                        existing_rules.replace_range(start_idx..replace_end, &block);
                        continue;
                    }
                }

                if !existing_rules.ends_with('\n') {
                    existing_rules.push('\n');
                }
                existing_rules.push('\n');
                existing_rules.push_str(&block);
                existing_rules.push('\n');
            }

            let _ = fs::write(&canonical_root_rules, existing_rules);
        }

        if dirty {
            let _ = fs::create_dir_all(&minicode_dir);
            if let Ok(serialized) = serde_json::to_string_pretty(&manifest) {
                let _ = fs::write(&sync_path, serialized);
            }
        }
    }

    /// Post-Tool Subdirectory Rules Injector:
    /// When a file tool (`read_file`, `write_file`, `patch_file`) touches a file inside a nested
    /// subdirectory, walks from `workspace_root` down to the file's parent directory and checks
    /// for scoped `.minicode/minicode.md`, `minicode.md`, `.minicode/RULES.md`, `RULES.md`,
    /// `AGENTS.md`, or `CLAUDE.md` files. Each discovered subdirectory rule file is injected at
    /// most once per session (deduplicated by canonical path).
    pub fn inject_unseen_subdirectory_rules(
        workspace_root: &Path,
        relative_path: &str,
    ) -> Option<String> {
        let root_buf = Self::canonical_workspace_root(workspace_root);
        let root = root_buf.as_path();
        let full_path = if Path::new(relative_path).is_absolute() {
            PathBuf::from(relative_path)
        } else {
            root.join(relative_path)
        };

        let mut cur = if full_path.is_dir() {
            Some(full_path.as_path())
        } else {
            full_path.parent()
        };

        let mut chain = Vec::new();
        while let Some(dir) = cur {
            if dir == root || !dir.starts_with(root) {
                break;
            }
            // Skip internal .minicode directory itself
            if dir.file_name().and_then(|n| n.to_str())
                == Some(crate::constants::WORKSPACE_DIR_NAME)
            {
                cur = dir.parent();
                continue;
            }
            chain.push(dir.to_path_buf());
            cur = dir.parent();
        }

        if chain.is_empty() {
            return None;
        }
        chain.reverse();

        let cache = INJECTED_SUBDIR_RULES.get_or_init(|| std::sync::Mutex::new(HashSet::new()));
        let mut seen = cache.lock().unwrap_or_else(|e| e.into_inner());

        let mut injected_sections = Vec::new();

        for dir in chain {
            // 1. Check canonical minicode subdirectory rules first
            let mut found_for_dir = false;
            for candidate in &[
                ".minicode/minicode.md",
                ".minicode/MINICODE.md",
                "minicode.md",
                "MINICODE.md",
                ".minicode/RULES.md",
                "RULES.md",
            ] {
                let candidate_path = dir.join(candidate);
                if candidate_path.is_file() {
                    let canon = candidate_path
                        .canonicalize()
                        .unwrap_or_else(|_| candidate_path.clone());
                    if seen.insert(canon) {
                        if let Ok(raw) = fs::read_to_string(&candidate_path) {
                            let trimmed = raw.trim();
                            if !trimmed.is_empty() {
                                let rel_label = candidate_path
                                    .strip_prefix(root)
                                    .map(|p| p.display().to_string())
                                    .unwrap_or_else(|_| candidate.to_string());
                                let capped: String =
                                    trimmed.chars().take(SUBDIR_INJECT_MAX_CHARS).collect();
                                injected_sections.push(format!(
                                    "### `{}`\n{}",
                                    rel_label,
                                    capped.trim()
                                ));
                            }
                        }
                    }
                    found_for_dir = true;
                    break;
                }
            }

            if found_for_dir {
                continue;
            }

            // 2. Otherwise check foreign agent rule files in that subdirectory (distilled cleanly)
            for foreign_rel in FOREIGN_AGENT_RULE_FILES {
                let candidate_path = dir.join(foreign_rel);
                if candidate_path.is_file() {
                    let canon = candidate_path
                        .canonicalize()
                        .unwrap_or_else(|_| candidate_path.clone());
                    if seen.insert(canon) {
                        if let Ok(raw) = fs::read_to_string(&candidate_path) {
                            let rel_label = candidate_path
                                .strip_prefix(root)
                                .map(|p| p.display().to_string())
                                .unwrap_or_else(|_| foreign_rel.to_string());
                            let distilled = Self::distill_useful_rules(&rel_label, &raw);
                            if !distilled.trim().is_empty() {
                                let capped: String =
                                    distilled.chars().take(SUBDIR_INJECT_MAX_CHARS).collect();
                                injected_sections.push(format!(
                                    "### `{}` (distilled into `.minicode/minicode.md`)\n{}",
                                    rel_label,
                                    capped.trim()
                                ));
                            }
                        }
                    }
                    break;
                }
            }
        }

        if injected_sections.is_empty() {
            None
        } else {
            Some(format!(
                "\n\n<subdirectory_rules>\n{}\n</subdirectory_rules>",
                injected_sections.join("\n\n")
            ))
        }
    }

    /// Reads the canonical root `.minicode/minicode.md` (or fallback `minicode.md` / `.minicode/RULES.md`)
    /// after ensuring any foreign rule files in the workspace root have been harvested into `.minicode/minicode.md`.
    pub fn read_canonical_root_rules(workspace_root: &Path) -> Option<String> {
        let root = Self::canonical_workspace_root(workspace_root);
        let empty_files: &[&str] = &[];
        Self::sync_and_harvest_rules(&root, empty_files);

        for candidate in &[
            crate::constants::MINICODE_MD_FILE,
            ".minicode/MINICODE.md",
            crate::constants::SCOPED_RULES_FILE,
            "MINICODE.md",
            crate::constants::LEGACY_RULES_MD_FILE,
            ".minicode/rules.md",
            "RULES.md",
            ".rules.md",
        ] {
            let path = root.join(candidate);
            if path.is_file() {
                if let Ok(content) = fs::read_to_string(&path) {
                    let trimmed = content.trim();
                    if !trimmed.is_empty() {
                        return Some(trimmed.to_string());
                    }
                }
            }
        }
        None
    }

    /// Distills useful project rules (build/test/lint commands, architecture, coding style,
    /// directory structure, invariants) from a foreign agent file while stripping out
    /// foreign agent personas, foreign tool names (`TodoWrite`, `MultiEdit`, etc.), and noise.
    pub fn distill_useful_rules(_source_name: &str, raw_content: &str) -> String {
        let mut kept_lines = Vec::new();
        let mut skip_foreign_section = false;

        for line in raw_content.lines() {
            let trimmed = line.trim();
            let lower = trimmed.to_lowercase();

            // Check if a markdown heading starts a foreign-agent-only persona/slash-command section
            if trimmed.starts_with('#') {
                if lower.contains("claude code")
                    || lower.contains("cursor agent")
                    || lower.contains("windsurf")
                    || lower.contains("copilot instructions")
                    || lower.contains("slash commands")
                {
                    skip_foreign_section = true;
                    continue;
                } else {
                    skip_foreign_section = false;
                }
            }

            if skip_foreign_section {
                continue;
            }

            // Filter out foreign agent persona declarations and foreign tool instructions
            if Self::is_foreign_agent_noise(&lower) {
                continue;
            }

            kept_lines.push(line);
        }

        // Collapse excessive blank lines
        let mut cleaned = String::new();
        let mut prev_blank = false;
        for line in kept_lines {
            let is_blank = line.trim().is_empty();
            if is_blank && prev_blank {
                continue;
            }
            cleaned.push_str(line);
            cleaned.push('\n');
            prev_blank = is_blank;
        }

        cleaned.trim().to_string()
    }

    fn is_foreign_agent_noise(lower_line: &str) -> bool {
        let noise_patterns = [
            "you are claude",
            "you are cursor",
            "you are github copilot",
            "you are windsurf",
            "you are cline",
            "you are opencode",
            "use todowrite",
            "call todowrite",
            "todowrite tool",
            "todoread tool",
            "multiedit tool",
            "notebookedit tool",
            "str_replace_editor",
            "claude.md",
            ".cursorrules",
        ];
        noise_patterns.iter().any(|pat| lower_line.contains(pat))
    }

    /// Resolves hierarchical developer rules by inspecting `.minicode/RULES.md` (and scoped `RULES.md`)
    /// in the workspace root and all parent directories leading to the currently active files.
    /// Automatically harvests useful rules from any foreign rule files into `.minicode/RULES.md` first.
    pub fn resolve_scoped_rules(
        workspace_root: &Path,
        active_files: &[impl AsRef<Path>],
    ) -> String {
        Self::resolve_scoped_rules_with_budget(workspace_root, active_files, DEFAULT_MAX_DOX_CHARS)
    }

    /// Resolves scoped rules with dynamic character budget derived from real-time context budget headroom.
    pub fn resolve_scoped_rules_with_headroom(
        workspace_root: &Path,
        active_files: &[impl AsRef<Path>],
        budget: Option<&crate::context::budget::ContextBudget>,
    ) -> String {
        let max_chars = match budget {
            Some(b) => {
                let pct = b.percentage();
                if pct >= crate::constants::BUDGET_PRESSURE_HIGH_THRESHOLD {
                    2_000
                } else if pct >= crate::constants::BUDGET_PRESSURE_MODERATE_THRESHOLD {
                    6_000
                } else {
                    let h = b.headroom_tokens();
                    (h * 4 / 10).clamp(DEFAULT_MAX_DOX_CHARS, 16_000)
                }
            }
            None => DEFAULT_MAX_DOX_CHARS,
        };
        Self::resolve_scoped_rules_with_budget(workspace_root, active_files, max_chars)
    }

    /// Resolves hierarchical developer rules with a maximum character budget and smart
    /// language-aware markdown section filtering. Only reads canonical `minicode` rule files.
    pub fn resolve_scoped_rules_with_budget(
        workspace_root: &Path,
        active_files: &[impl AsRef<Path>],
        max_chars: usize,
    ) -> String {
        // First ensure any useful rules from foreign agent files are harvested into .minicode/RULES.md
        Self::sync_and_harvest_rules(workspace_root, active_files);

        let root_buf = Self::canonical_workspace_root(workspace_root);
        let root = root_buf.as_path();

        let mut discovered_files: Vec<PathBuf> = Vec::new();
        let mut visited_paths: HashSet<PathBuf> = HashSet::new();

        // Detect file extensions in the active working set
        let mut active_exts: HashSet<String> = HashSet::new();
        for file in active_files {
            let p = file.as_ref();
            if let Some(ext) = p.extension().and_then(|e| e.to_str()) {
                active_exts.insert(ext.to_lowercase());
            }
        }

        // 1. Root canonical rules (.minicode/RULES.md or RULES.md)
        for candidate in CANONICAL_RULE_FILES {
            let root_file = root.join(candidate);
            if root_file.is_file() && visited_paths.insert(root_file.clone()) {
                discovered_files.push(root_file);
                break; // One primary canonical rule file per directory
            }
        }

        // 2. Traversal down active file paths for scoped directory RULES.md
        for file in active_files {
            let file_ref = file.as_ref();
            let full_path = if file_ref.is_absolute() {
                file_ref.to_path_buf()
            } else {
                root.join(file_ref)
            };

            // Collect directory chain from root to file's parent (or directory itself)
            let mut current = if full_path.is_dir() {
                Some(full_path.as_path())
            } else {
                full_path.parent()
            };
            let mut chain = Vec::new();
            while let Some(dir) = current {
                if dir == root || !dir.starts_with(root) {
                    break;
                }
                chain.push(dir.to_path_buf());
                current = dir.parent();
            }

            // Reverse to process top-down (e.g. crates/ -> crates/core/)
            chain.reverse();

            for dir in chain {
                for candidate in CANONICAL_RULE_FILES {
                    let rule_file = dir.join(candidate);
                    if rule_file.is_file() && visited_paths.insert(rule_file.clone()) {
                        discovered_files.push(rule_file);
                        break;
                    }
                }
            }
        }

        if discovered_files.is_empty() {
            return String::new();
        }

        let mut output = String::new();
        for file in discovered_files {
            if let Ok(content) = fs::read_to_string(&file) {
                let filtered = Self::filter_markdown_sections(&content, &active_exts);
                let trimmed = filtered.trim();
                if trimmed.is_empty() {
                    continue;
                }

                let rel = file
                    .strip_prefix(root)
                    .or_else(|_| file.strip_prefix(workspace_root))
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|_| file.display().to_string());

                let header = format!("<!-- Scoped Rules: {} -->\n", rel);
                let needed = header.len() + trimmed.len() + 2;

                if output.len() + needed > max_chars {
                    // Check if we can fit at least a partial block
                    let remaining = max_chars.saturating_sub(output.len() + header.len() + 50);
                    if remaining > 150 {
                        if !output.is_empty() {
                            output.push_str("\n\n");
                        }
                        output.push_str(&header);
                        let truncated: String = trimmed.chars().take(remaining).collect();
                        output.push_str(&truncated);
                        output.push_str(
                            "\n<!-- [... rule file truncated to preserve context budget ...] -->",
                        );
                    }
                    break;
                }

                if !output.is_empty() {
                    output.push_str("\n\n");
                }
                output.push_str(&header);
                output.push_str(trimmed);
            }
        }

        output
    }

    /// Filters markdown content by section headers when the active working set has
    /// specific languages, omitting sections clearly dedicated to disjoint languages.
    pub fn filter_markdown_sections(content: &str, active_exts: &HashSet<String>) -> String {
        if active_exts.is_empty() {
            return content.to_string();
        }

        let is_rust = active_exts.contains("rs");
        let is_python = active_exts.contains("py");
        let is_frontend = active_exts.iter().any(|ext| {
            matches!(
                ext.as_str(),
                "ts" | "tsx" | "js" | "jsx" | "html" | "css" | "scss" | "vue" | "svelte"
            )
        });
        let is_go = active_exts.contains("go");

        let mut result = Vec::new();
        let mut current_section_header = String::new();
        let mut current_section_lines = Vec::new();

        for line in content.lines() {
            if line.starts_with('#') {
                // Process previous section
                if !current_section_lines.is_empty() || !current_section_header.is_empty() {
                    if Self::should_keep_section(
                        &current_section_header,
                        is_rust,
                        is_python,
                        is_frontend,
                        is_go,
                    ) {
                        if !current_section_header.is_empty() {
                            result.push(current_section_header.clone());
                        }
                        result.append(&mut current_section_lines);
                    } else {
                        current_section_lines.clear();
                    }
                }
                current_section_header = line.to_string();
            } else {
                current_section_lines.push(line.to_string());
            }
        }

        // Flush last section
        if (!current_section_lines.is_empty() || !current_section_header.is_empty())
            && Self::should_keep_section(
                &current_section_header,
                is_rust,
                is_python,
                is_frontend,
                is_go,
            )
        {
            if !current_section_header.is_empty() {
                result.push(current_section_header);
            }
            result.append(&mut current_section_lines);
        }

        result.join("\n")
    }

    fn should_keep_section(
        header: &str,
        is_rust: bool,
        is_python: bool,
        is_frontend: bool,
        is_go: bool,
    ) -> bool {
        if header.is_empty() {
            return true;
        }

        let lower = header.to_lowercase();

        // Check if the section targets a specific technology
        let targets_rust = lower.contains("rust") || lower.contains("cargo");
        let targets_python =
            lower.contains("python") || lower.contains("pytest") || lower.contains("pip");
        let targets_frontend = lower.contains("frontend")
            || lower.contains("react")
            || lower.contains("vue")
            || lower.contains("svelte")
            || lower.contains("css")
            || lower.contains("tailwind")
            || lower.contains("javascript")
            || lower.contains("typescript");
        let targets_go = lower.contains("golang") || lower.contains("go modules");

        // If it exclusively targets another language not present in active working set, omit it
        if targets_python && !is_python && (is_rust || is_frontend || is_go) {
            return false;
        }
        if targets_rust && !is_rust && (is_python || is_frontend || is_go) {
            return false;
        }
        if targets_frontend && !is_frontend && (is_rust || is_python || is_go) {
            return false;
        }
        if targets_go && !is_go && (is_rust || is_python || is_frontend) {
            return false;
        }

        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_dox_candidate_discovery() {
        let dir = tempdir().unwrap();
        let ws = dir.path();

        // Test with .dox file
        fs::write(ws.join(".dox"), "# Dox Global Invariants\n- Always test").unwrap();

        let rules = DoxEngine::resolve_scoped_rules(ws, &["src/main.rs"]);
        assert!(rules.contains("Dox Global Invariants"));
    }

    #[test]
    fn test_dox_section_filtering() {
        let content = r#"# Project Rules
General rule for all files.

## Rust Guidelines
Use thiserror and tracing.

## Python Guidelines
Use poetry and black.

## Testing Standards
Write comprehensive tests.
"#;

        let mut rust_exts = HashSet::new();
        rust_exts.insert("rs".to_string());

        let filtered_rust = DoxEngine::filter_markdown_sections(content, &rust_exts);
        assert!(filtered_rust.contains("Rust Guidelines"));
        assert!(filtered_rust.contains("General rule"));
        assert!(filtered_rust.contains("Testing Standards"));
        assert!(!filtered_rust.contains("Python Guidelines"));
        assert!(!filtered_rust.contains("Use poetry and black"));
    }

    #[test]
    fn test_dox_budget_capping() {
        let dir = tempdir().unwrap();
        let ws = dir.path();

        let huge_rules = "A".repeat(5000);
        fs::write(ws.join("AGENTS.md"), format!("# Big Rules\n{}", huge_rules)).unwrap();

        let capped = DoxEngine::resolve_scoped_rules_with_budget(ws, &["src/main.rs"], 500);
        assert!(capped.len() <= 600);
        assert!(capped.contains("truncated to preserve context budget"));
    }

    #[test]
    fn test_dox_headroom_scaling() {
        let dir = tempdir().unwrap();
        let ws = dir.path();

        fs::write(ws.join("AGENTS.md"), "# Rules\n- Rule 1").unwrap();

        let budget_healthy = crate::context::budget::ContextBudget::new(10_000, 100_000, 10_000);
        let rules_healthy = DoxEngine::resolve_scoped_rules_with_headroom(
            ws,
            &["src/main.rs"],
            Some(&budget_healthy),
        );
        assert!(rules_healthy.contains("Rule 1"));

        let budget_critical = crate::context::budget::ContextBudget::new(95_000, 100_000, 95_000);
        let rules_critical = DoxEngine::resolve_scoped_rules_with_headroom(
            ws,
            &["src/main.rs"],
            Some(&budget_critical),
        );
        assert!(rules_critical.contains("Rule 1"));
    }

    #[test]
    fn test_canonical_rules_harvest_from_foreign_claude_and_agents_md() {
        let dir = tempdir().unwrap();
        let ws = dir.path();

        fs::write(
            ws.join("CLAUDE.md"),
            "# Project Setup\nYou are Claude Code, Anthropic's official CLI.\nUse TodoWrite tool to track tasks.\n- Always run `cargo check -j 1` before finishing.\n- Never use `.unwrap()` in library code.\n",
        )
        .unwrap();

        let resolved = DoxEngine::resolve_scoped_rules(ws, &["src/main.rs"]);
        // Useful engineering rules must be harvested into .minicode/minicode.md
        assert!(ws.join(".minicode/minicode.md").is_file());
        assert!(resolved.contains(".minicode/minicode.md"));
        assert!(resolved.contains("Always run `cargo check -j 1`"));
        assert!(resolved.contains("Never use `.unwrap()`"));
        // Foreign persona & foreign tool lines must be stripped out
        assert!(!resolved.contains("You are Claude"));
        assert!(!resolved.contains("TodoWrite"));
    }

    #[test]
    fn test_inject_unseen_subdirectory_rules_once_per_session() {
        let dir = tempdir().unwrap();
        let ws = dir.path();
        let sub_dir = ws.join("packages/web/.minicode");
        fs::create_dir_all(&sub_dir).unwrap();
        fs::write(
            sub_dir.join("minicode.md"),
            "# Web Package Rules\n- Use Bun for frontend scripts.\n",
        )
        .unwrap();

        // First touch inside packages/web injects the subdirectory rules
        let first = DoxEngine::inject_unseen_subdirectory_rules(ws, "packages/web/src/App.tsx");
        assert!(first.is_some());
        let text = first.unwrap();
        assert!(text.contains("<subdirectory_rules>"));
        assert!(text.contains("Use Bun for frontend scripts."));

        // Second touch inside the same subdirectory is deduplicated
        let second = DoxEngine::inject_unseen_subdirectory_rules(ws, "packages/web/src/main.tsx");
        assert!(second.is_none());
    }
}

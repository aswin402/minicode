//! Native Rust TUI Chatbox Intellisense, Inline Ghost-Text Hinter, Boundary-Bonus Fuzzy Matcher,
//! and Frecency/Prefix-Locked Prompt History Engine.
//!
//! Designed for sub-millisecond keystroke latency (`<0.05ms`) and minimal memory footprint (`<80 KB`)
//! with zero external autocomplete crate dependencies.

use crate::constants::{
    FUZZY_BASENAME_MATCH_BONUS, FUZZY_BASE_CHAR_SCORE, FUZZY_BOUNDARY_SEPARATOR_BONUS,
    FUZZY_CAMEL_CASE_BONUS, FUZZY_CONSECUTIVE_CHAR_BONUS, FUZZY_EXACT_PREFIX_BONUS,
    FUZZY_GAP_PENALTY_PER_CHAR, INTELLISENSE_MAX_FILE_INDEX, INTELLISENSE_MAX_POPUP_ITEMS,
    INTELLISENSE_MAX_SCAN_DEPTH, INTELLISENSE_MAX_SYMBOL_INDEX, PROMPT_HISTORY_FILE,
    PROMPT_HISTORY_MAX_ENTRIES,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashSet, VecDeque};
use std::path::{Path, PathBuf};

/// Single-pass boundary-bonus subsequence fuzzy scorer (modeled after Helix `nucleo` / `fzf`).
///
/// Returns `Some(score)` when every character of `query` appears in order in `candidate`
/// (case-insensitive), awarding bonuses for exact prefixes, filename basename matches,
/// path/word boundaries (`/`, `_`, `-`, `.`, `:`), `camelCase` transitions, and consecutive runs.
#[must_use]
pub fn fuzzy_score(candidate: &str, query: &str) -> Option<i32> {
    if query.is_empty() {
        return Some(0);
    }
    let cand_chars: Vec<char> = candidate.chars().collect();
    let query_chars: Vec<char> = query.chars().collect();
    if query_chars.len() > cand_chars.len() {
        return None;
    }

    let basename_start = cand_chars
        .iter()
        .rposition(|&c| c == '/' || c == '\\')
        .map_or(0, |idx| idx + 1);

    let mut score: i32 = 0;
    let mut q_idx = 0usize;
    let mut prev_match_idx: Option<usize> = None;

    for (c_idx, &c_ch) in cand_chars.iter().enumerate() {
        let q_ch = query_chars[q_idx];
        if c_ch.eq_ignore_ascii_case(&q_ch) {
            score += FUZZY_BASE_CHAR_SCORE;

            if c_idx >= basename_start {
                score += FUZZY_BASENAME_MATCH_BONUS / (query_chars.len().max(1) as i32);
            }

            let is_boundary = c_idx == 0
                || c_idx == basename_start
                || matches!(
                    cand_chars[c_idx - 1],
                    '/' | '\\' | '_' | '-' | '.' | ':' | ' '
                );
            let is_camel = c_idx > 0
                && cand_chars[c_idx - 1].is_ascii_lowercase()
                && c_ch.is_ascii_uppercase();

            if is_boundary {
                score += FUZZY_BOUNDARY_SEPARATOR_BONUS;
            } else if is_camel {
                score += FUZZY_CAMEL_CASE_BONUS;
            }

            if let Some(prev) = prev_match_idx {
                if c_idx == prev + 1 {
                    score += FUZZY_CONSECUTIVE_CHAR_BONUS;
                } else {
                    let gap = (c_idx - prev - 1) as i32;
                    score -= gap * FUZZY_GAP_PENALTY_PER_CHAR;
                }
            }

            prev_match_idx = Some(c_idx);
            q_idx += 1;
            if q_idx == query_chars.len() {
                break;
            }
        }
    }

    if q_idx < query_chars.len() {
        return None;
    }

    // Exact prefix bonus on full string or basename
    let basename = &candidate[candidate
        .rfind(['/', '\\'])
        .map_or(0, |byte_idx| byte_idx + 1)..];
    if starts_with_ignore_ascii_case(candidate, query)
        || starts_with_ignore_ascii_case(basename, query)
    {
        score += FUZZY_EXACT_PREFIX_BONUS;
    }

    // Prefer shorter, tighter matches on ties
    score -= (cand_chars.len() as i32) / 4;
    Some(score)
}

#[inline]
fn starts_with_ignore_ascii_case(haystack: &str, prefix: &str) -> bool {
    haystack
        .get(..prefix.len())
        .is_some_and(|sub| sub.eq_ignore_ascii_case(prefix))
}

/// A persisted prompt history entry with usage frequency and timestamp for frecency ranking.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HistoryEntry {
    pub text: String,
    #[serde(default = "default_one")]
    pub count: u32,
    #[serde(default)]
    pub timestamp: u64,
}

const fn default_one() -> u32 {
    1
}

/// Frecency-weighted, prefix-aware prompt history ring buffer (`Atuin` + `Fish` style).
///
/// Entries are ordered newest-first (`entries[0]` is the most recently submitted prompt).
#[derive(Debug, Clone, Default)]
pub struct PromptHistory {
    /// Newest-first deque of unique prompt entries.
    pub entries: VecDeque<HistoryEntry>,
    /// Current navigation index into `entries` (`None` when editing live draft).
    pub cursor: Option<usize>,
    /// Unsubmitted user draft stashed when first pressing `Up`.
    pub draft_stash: String,
    /// Prefix filter locked when starting `Up` navigation from a non-empty single-line draft.
    pub prefix_filter: String,
}

impl PromptHistory {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Loads workspace prompt history from `<workspace>/.minicode/prompt_history.jsonl`.
    pub fn load_from_workspace(&mut self, workspace_root: &Path) {
        let path = workspace_root.join(PROMPT_HISTORY_FILE);
        let Ok(content) = std::fs::read_to_string(&path) else {
            return;
        };
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if let Ok(entry) = serde_json::from_str::<HistoryEntry>(trimmed) {
                self.insert_or_promote_memory_only(entry.text, entry.count, entry.timestamp);
            }
        }
    }

    /// Seeds a prompt into the in-memory history (e.g. during session hydration) without disk I/O.
    pub fn seed_prompt(&mut self, prompt: &str) {
        let clean = prompt.trim();
        if clean.is_empty() {
            return;
        }
        self.insert_or_promote_memory_only(clean.to_string(), 1, current_unix_secs());
    }

    /// Records a newly submitted prompt into the newest slot (`index 0`) and persists to disk.
    pub fn record(&mut self, prompt: &str, workspace_root: Option<&Path>) {
        let clean = prompt.trim();
        if clean.is_empty() {
            return;
        }
        let now = current_unix_secs();
        self.insert_or_promote_memory_only(clean.to_string(), 1, now);
        self.reset_navigation();

        if let Some(root) = workspace_root {
            self.persist_to_workspace(root);
        }
    }

    fn insert_or_promote_memory_only(&mut self, text: String, add_count: u32, timestamp: u64) {
        let prev_count = if let Some(pos) = self.entries.iter().position(|e| e.text == text) {
            self.entries.remove(pos).map_or(0, |removed| removed.count)
        } else {
            0
        };

        self.entries.push_front(HistoryEntry {
            text,
            count: prev_count.saturating_add(add_count).max(1),
            timestamp,
        });

        while self.entries.len() > PROMPT_HISTORY_MAX_ENTRIES {
            self.entries.pop_back();
        }
    }

    fn persist_to_workspace(&self, workspace_root: &Path) {
        let path = workspace_root.join(PROMPT_HISTORY_FILE);
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        // Write oldest-to-newest so sequential loading reconstructs newest-first order
        let mut out = String::new();
        for entry in self.entries.iter().rev() {
            if let Ok(json) = serde_json::to_string(entry) {
                out.push_str(&json);
                out.push('\n');
            }
        }
        let _ = std::fs::write(path, out);
    }

    /// Resets `Up`/`Down` navigation cursor when the user types or mutates the buffer.
    pub fn reset_navigation(&mut self) {
        self.cursor = None;
        self.prefix_filter.clear();
    }

    /// Navigates to the next older prompt matching the active prefix filter (or all entries if none match).
    pub fn navigate_up(&mut self, current_text: &str) -> Option<String> {
        if self.entries.is_empty() {
            return None;
        }

        let start_search_from = match self.cursor {
            None => {
                self.draft_stash = current_text.to_string();
                let single_line = !current_text.contains('\n');
                self.prefix_filter = if single_line {
                    current_text.trim().to_string()
                } else {
                    String::new()
                };
                0
            }
            Some(cur) => cur.saturating_add(1),
        };

        if start_search_from >= self.entries.len() {
            return None;
        }

        // 1. Try prefix-filtered search if a non-empty prefix filter is locked
        if !self.prefix_filter.is_empty() {
            if let Some(found_idx) = (start_search_from..self.entries.len()).find(|&idx| {
                starts_with_ignore_ascii_case(&self.entries[idx].text, &self.prefix_filter)
                    && self.entries[idx].text != current_text
            }) {
                self.cursor = Some(found_idx);
                return Some(self.entries[found_idx].text.clone());
            }
            // If first Up had no prefix matches at all, fall back to unfiltered chronological history
            if self.cursor.is_none() {
                self.prefix_filter.clear();
            } else {
                return None;
            }
        }

        // 2. Unfiltered chronological step to older entry
        self.cursor = Some(start_search_from);
        Some(self.entries[start_search_from].text.clone())
    }

    /// Navigates toward newer prompts; restores `draft_stash` when stepping past index `0`.
    pub fn navigate_down(&mut self) -> Option<String> {
        let cur = self.cursor?;
        if cur == 0 {
            self.cursor = None;
            self.prefix_filter.clear();
            return Some(std::mem::take(&mut self.draft_stash));
        }

        if !self.prefix_filter.is_empty() {
            if let Some(found_idx) = (0..cur).rev().find(|&idx| {
                starts_with_ignore_ascii_case(&self.entries[idx].text, &self.prefix_filter)
            }) {
                self.cursor = Some(found_idx);
                return Some(self.entries[found_idx].text.clone());
            }
            self.cursor = None;
            self.prefix_filter.clear();
            return Some(std::mem::take(&mut self.draft_stash));
        }

        let next_idx = cur - 1;
        self.cursor = Some(next_idx);
        Some(self.entries[next_idx].text.clone())
    }

    /// Computes a Fish/Reedline-style inline ghost suffix from history frecency.
    #[must_use]
    pub fn suggest_ghost_suffix(&self, line_prefix: &str) -> Option<String> {
        if line_prefix.is_empty() || line_prefix.starts_with('/') {
            return None;
        }
        self.entries
            .iter()
            .filter_map(|entry| {
                let first_line = entry.text.lines().next().unwrap_or("");
                (first_line.len() > line_prefix.len()
                    && starts_with_ignore_ascii_case(first_line, line_prefix))
                .then(|| &first_line[line_prefix.len()..])
            })
            .next()
            .map(str::to_string)
    }
}

/// Kind of contextual intellisense trigger detected at the cursor position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TriggerKind {
    /// `@<path_query>` workspace file mention
    FileMention,
    /// `#<symbol_query>` AST symbol mention
    SymbolMention,
    /// `$<skill_query>` Vault skill mention
    SkillMention,
    /// `/<cmd> <arg_query>` slash command argument completion
    SlashArg { command: String },
}

impl TriggerKind {
    #[must_use]
    pub const fn badge(&self) -> &'static str {
        match self {
            Self::FileMention => " @ Files ",
            Self::SymbolMention => " # Symbols ",
            Self::SkillMention => " $ Skills ",
            Self::SlashArg { .. } => " ⌘ Args ",
        }
    }
}

/// Active trigger token span on the current cursor row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TriggerToken {
    pub kind: TriggerKind,
    pub query: String,
    /// Character index on the current line where the replacement begins (including `@`, `#`, `$`, or arg start).
    pub replace_start_col: usize,
    /// Character index of the cursor on the current line.
    pub replace_end_col: usize,
}

/// A single ranked candidate shown in the floating intellisense popup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionCandidate {
    /// Text inserted into `[replace_start_col..replace_end_col]` when accepted.
    pub insert_text: String,
    /// Primary label displayed on the left of the popup row.
    pub display_label: String,
    /// Secondary category/detail badge displayed on the right of the popup row.
    pub detail: String,
    pub score: i32,
}

/// Declarative sub-argument completion schema for slash commands (`/cmd <arg>`).
struct SlashArgSpec {
    command: &'static str,
    arg: &'static str,
    detail: &'static str,
}

const SLASH_ARG_TABLE: &[SlashArgSpec] = &[
    // /caveman
    SlashArgSpec {
        command: "/caveman",
        arg: "lite",
        detail: "Concise output, full grammar preserved",
    },
    SlashArgSpec {
        command: "/caveman",
        arg: "full",
        detail: "Balanced token compression (default)",
    },
    SlashArgSpec {
        command: "/caveman",
        arg: "ultra",
        detail: "Maximum terse telegraphic compression",
    },
    SlashArgSpec {
        command: "/caveman",
        arg: "status",
        detail: "Inspect active Caveman token savings mode",
    },
    SlashArgSpec {
        command: "/caveman",
        arg: "off",
        detail: "Disable Caveman response compression",
    },
    // /temp
    SlashArgSpec {
        command: "/temp",
        arg: "low",
        detail: "Deterministic precision (temp 0.1)",
    },
    SlashArgSpec {
        command: "/temp",
        arg: "medium",
        detail: "Balanced reasoning & creativity (temp 0.4)",
    },
    SlashArgSpec {
        command: "/temp",
        arg: "high",
        detail: "Deep exploration & high effort (temp 0.7)",
    },
    // /theme
    SlashArgSpec {
        command: "/theme",
        arg: "tokyo-night",
        detail: "Tokyo Night dark palette",
    },
    SlashArgSpec {
        command: "/theme",
        arg: "catppuccin",
        detail: "Catppuccin Mocha warm dark palette",
    },
    SlashArgSpec {
        command: "/theme",
        arg: "dracula",
        detail: "Dracula high-contrast neon palette",
    },
    SlashArgSpec {
        command: "/theme",
        arg: "gruvbox",
        detail: "Gruvbox retro warm earth palette",
    },
    SlashArgSpec {
        command: "/theme",
        arg: "nord",
        detail: "Nord arctic blue palette",
    },
    SlashArgSpec {
        command: "/theme",
        arg: "kanagawa",
        detail: "Kanagawa ink-wash palette",
    },
    SlashArgSpec {
        command: "/theme",
        arg: "rose-pine",
        detail: "Rosé Pine muted dark palette",
    },
    SlashArgSpec {
        command: "/theme",
        arg: "everforest",
        detail: "Everforest organic green palette",
    },
    SlashArgSpec {
        command: "/theme",
        arg: "one-dark",
        detail: "Atom One Dark classic palette",
    },
    // /fav
    SlashArgSpec {
        command: "/fav",
        arg: "add",
        detail: "Save current provider/model to favorites",
    },
    SlashArgSpec {
        command: "/fav",
        arg: "rm",
        detail: "Remove a model from favorites",
    },
    SlashArgSpec {
        command: "/fav",
        arg: "list",
        detail: "List all saved favorite models",
    },
    // /copy
    SlashArgSpec {
        command: "/copy",
        arg: "last",
        detail: "Copy latest assistant response to clipboard",
    },
    SlashArgSpec {
        command: "/copy",
        arg: "all",
        detail: "Copy entire session transcript to clipboard",
    },
    // /goal
    SlashArgSpec {
        command: "/goal",
        arg: "status",
        detail: "Inspect active autonomous goal & ledger",
    },
    SlashArgSpec {
        command: "/goal",
        arg: "clear",
        detail: "Clear active autonomous goal anchor",
    },
];

/// Lazy-indexed workspace completion engine for `@file`, `#symbol`, `$skill`, and `/cmd <arg>`.
#[derive(Debug, Clone, Default)]
pub struct IntellisenseEngine {
    indexed_root: Option<PathBuf>,
    pub files: Vec<String>,
    pub symbols: Vec<(String, String)>,
    pub skills: Vec<(String, String)>,
    pub selected_index: usize,
    pub dismissed_for_query: Option<String>,
}

impl IntellisenseEngine {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Ensures the workspace file, symbol, and skill index is populated for `workspace_root`.
    pub fn ensure_indexed(&mut self, workspace_root: Option<&Path>) {
        if self.skills.is_empty() {
            self.index_builtin_skills();
        }
        let Some(root) = workspace_root else {
            return;
        };
        if self.indexed_root.as_deref() == Some(root) {
            return;
        }
        self.indexed_root = Some(root.to_path_buf());
        self.files.clear();
        self.symbols.clear();

        let mut seen_symbols: HashSet<String> = HashSet::new();
        let walker = ignore::WalkBuilder::new(root)
            .hidden(false)
            .git_ignore(true)
            .max_depth(Some(INTELLISENSE_MAX_SCAN_DEPTH))
            .filter_entry(|entry| {
                let name = entry.file_name().to_str().unwrap_or("");
                !matches!(
                    name,
                    ".git" | "target" | "node_modules" | "dist" | "build" | ".next" | ".minicode"
                )
            })
            .build();

        for entry in walker.flatten() {
            if self.files.len() >= INTELLISENSE_MAX_FILE_INDEX {
                break;
            }
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Ok(rel) = path.strip_prefix(root) else {
                continue;
            };
            let rel_str = rel.to_string_lossy().replace('\\', "/");
            if rel_str.is_empty() {
                continue;
            }

            if self.symbols.len() < INTELLISENSE_MAX_SYMBOL_INDEX && is_indexable_source_file(path)
            {
                if let Ok(content) = std::fs::read_to_string(path) {
                    extract_lightweight_symbols(
                        &content,
                        &rel_str,
                        &mut self.symbols,
                        &mut seen_symbols,
                    );
                }
            }

            self.files.push(rel_str);
        }

        // Sort files by path depth first, then alphabetically so top-level files appear first on `@`
        self.files.sort_by(|a, b| {
            let depth_a = a.bytes().filter(|&ch| ch == b'/').count();
            let depth_b = b.bytes().filter(|&ch| ch == b'/').count();
            depth_a.cmp(&depth_b).then_with(|| a.cmp(b))
        });
    }

    fn index_builtin_skills(&mut self) {
        self.skills = crate::vault::builtin::BUILTIN_SKILLS
            .iter()
            .map(|s| {
                let kind = s
                    .content
                    .lines()
                    .take(10)
                    .find_map(|l| {
                        l.strip_prefix("type:")
                            .or_else(|| l.strip_prefix("category:"))
                    })
                    .map_or("skill", |v| v.trim().trim_matches('"'));
                (s.name.to_string(), kind.to_string())
            })
            .collect();
    }

    /// Detects if the cursor is currently positioned inside an intellisense trigger token
    /// (`@<file>`, `#<symbol>`, `$<skill>`, or `/<cmd> <arg>`).
    #[must_use]
    pub fn extract_trigger(
        line: &str,
        cursor_col: usize,
        cursor_row: usize,
    ) -> Option<TriggerToken> {
        let chars: Vec<char> = line.chars().collect();
        let col = cursor_col.min(chars.len());
        let prefix: String = chars[..col].iter().collect();

        // 1. Check `/command <arg>` on row 0 when there is a single space after a known command
        if cursor_row == 0 {
            let trimmed_start = prefix.trim_start();
            if trimmed_start.starts_with('/') {
                if let Some(space_char_idx) = chars[..col].iter().position(|&c| c == ' ') {
                    let cmd_part: String = chars[..space_char_idx].iter().collect();
                    let cmd_clean = cmd_part.trim().to_ascii_lowercase();
                    let arg_part: String = chars[space_char_idx + 1..col].iter().collect();
                    if !arg_part.contains(' ')
                        && SLASH_ARG_TABLE.iter().any(|s| s.command == cmd_clean)
                    {
                        return Some(TriggerToken {
                            kind: TriggerKind::SlashArg { command: cmd_clean },
                            query: arg_part,
                            replace_start_col: space_char_idx + 1,
                            replace_end_col: col,
                        });
                    }
                }
            }
        }

        // 2. Scan backward from `col` to the start of the current whitespace-delimited token
        let token_start = chars[..col]
            .iter()
            .rposition(|&c| c.is_whitespace() || matches!(c, '(' | '[' | '{' | '"' | '\'' | ','))
            .map_or(0, |idx| idx + 1);

        if token_start >= col {
            return None;
        }

        let marker = chars[token_start];
        let kind = match marker {
            '@' => TriggerKind::FileMention,
            '#' => TriggerKind::SymbolMention,
            '$' => TriggerKind::SkillMention,
            _ => return None,
        };

        let query: String = chars[token_start + 1..col].iter().collect();
        Some(TriggerToken {
            kind,
            query,
            replace_start_col: token_start,
            replace_end_col: col,
        })
    }

    /// Returns up to `INTELLISENSE_MAX_POPUP_ITEMS` ranked completion candidates for `trigger`.
    pub fn query_candidates(
        &mut self,
        trigger: &TriggerToken,
        workspace_root: Option<&Path>,
    ) -> Vec<CompletionCandidate> {
        let sig = format!("{:?}:{}", trigger.kind, trigger.query);
        if self.dismissed_for_query.as_deref() == Some(sig.as_str()) {
            return Vec::new();
        }

        self.ensure_indexed(workspace_root);

        let mut ranked: Vec<CompletionCandidate> = match &trigger.kind {
            TriggerKind::SlashArg { command } => SLASH_ARG_TABLE
                .iter()
                .filter(|spec| spec.command == command)
                .filter_map(|spec| {
                    let score = fuzzy_score(spec.arg, &trigger.query)?;
                    Some(CompletionCandidate {
                        insert_text: spec.arg.to_string(),
                        display_label: format!("{} {}", spec.command, spec.arg),
                        detail: spec.detail.to_string(),
                        score,
                    })
                })
                .collect(),
            TriggerKind::FileMention => self
                .files
                .iter()
                .filter_map(|path| {
                    let score = fuzzy_score(path, &trigger.query)?;
                    let ext = Path::new(path)
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("file");
                    Some(CompletionCandidate {
                        insert_text: format!("@{path} "),
                        display_label: format!("@{path}"),
                        detail: ext.to_string(),
                        score,
                    })
                })
                .collect(),
            TriggerKind::SymbolMention => self
                .symbols
                .iter()
                .filter_map(|(sym, file_hint)| {
                    let score = fuzzy_score(sym, &trigger.query)?;
                    Some(CompletionCandidate {
                        insert_text: format!("#{sym} "),
                        display_label: format!("#{sym}"),
                        detail: file_hint.clone(),
                        score,
                    })
                })
                .collect(),
            TriggerKind::SkillMention => self
                .skills
                .iter()
                .filter_map(|(name, category)| {
                    let score = fuzzy_score(name, &trigger.query)?;
                    Some(CompletionCandidate {
                        insert_text: format!("${name} "),
                        display_label: format!("${name}"),
                        detail: category.clone(),
                        score,
                    })
                })
                .collect(),
        };

        ranked.sort_by(|a, b| {
            b.score
                .cmp(&a.score)
                .then_with(|| a.display_label.len().cmp(&b.display_label.len()))
                .then_with(|| a.display_label.cmp(&b.display_label))
        });
        ranked.truncate(INTELLISENSE_MAX_POPUP_ITEMS);
        ranked
    }

    /// Computes an inline dimmed ghost-text completion suffix when the cursor is at the end of a line.
    pub fn compute_ghost_hint(
        &mut self,
        line: &str,
        cursor_col: usize,
        cursor_row: usize,
        history: &PromptHistory,
        workspace_root: Option<&Path>,
        top_slash_cmd: Option<&str>,
    ) -> Option<String> {
        let line_char_len = line.chars().count();
        if cursor_col < line_char_len || line.is_empty() {
            return None;
        }

        // 1. Column-0 `/command` prefix completion
        if cursor_row == 0 && line.starts_with('/') && !line.contains(' ') {
            if let Some(cmd) = top_slash_cmd {
                if cmd.len() > line.len() && starts_with_ignore_ascii_case(cmd, line) {
                    return Some(cmd[line.len()..].to_string());
                }
            }
        }

        // 2. Active trigger (`/<cmd> <arg>`, `@file`, `#symbol`, `$skill`) prefix completion
        if let Some(trigger) = Self::extract_trigger(line, cursor_col, cursor_row) {
            if !trigger.query.is_empty() {
                let candidates = self.query_candidates(&trigger, workspace_root);
                if let Some(best) = candidates.first() {
                    let raw_target = best
                        .insert_text
                        .trim_end()
                        .trim_start_matches(['@', '#', '$']);
                    if raw_target.len() > trigger.query.len()
                        && starts_with_ignore_ascii_case(raw_target, &trigger.query)
                    {
                        return Some(raw_target[trigger.query.len()..].to_string());
                    }
                }
            }
        }

        // 3. Frecency-weighted prompt history prefix completion
        history.suggest_ghost_suffix(line)
    }
}

/// Extracts the first word segment from `ghost` (including leading spaces or trailing path separator)
/// for Fish/Zsh-style single-word acceptance (`Ctrl+Right` / `Alt+Right` / `Alt+F`).
#[must_use]
pub fn extract_first_ghost_word(ghost: &str) -> &str {
    if ghost.is_empty() {
        return "";
    }
    let bytes = ghost.as_bytes();
    let mut i = 0;
    // Consume any leading whitespace
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    // Consume word/identifier characters up to next whitespace or path separator
    while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
        if bytes[i] == b'/' || bytes[i] == b'\\' {
            i += 1;
            break;
        }
        i += 1;
    }
    // Advance to valid UTF-8 char boundary if needed
    while i < ghost.len() && !ghost.is_char_boundary(i) {
        i += 1;
    }
    &ghost[..i]
}

fn is_indexable_source_file(path: &Path) -> bool {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    matches!(
        ext,
        "rs" | "ts" | "tsx" | "js" | "jsx" | "py" | "go" | "dart"
    )
}

fn extract_lightweight_symbols(
    content: &str,
    rel_file: &str,
    out: &mut Vec<(String, String)>,
    seen: &mut HashSet<String>,
) {
    const PREFIXES: &[&str] = &[
        "pub async fn ",
        "pub fn ",
        "async fn ",
        "fn ",
        "pub struct ",
        "struct ",
        "pub enum ",
        "enum ",
        "pub trait ",
        "trait ",
        "export async function ",
        "export function ",
        "export default function ",
        "export class ",
        "export interface ",
        "export type ",
        "def ",
        "class ",
    ];

    for line in content.lines().take(600) {
        if out.len() >= INTELLISENSE_MAX_SYMBOL_INDEX {
            break;
        }
        let trimmed = line.trim_start();
        for &prefix in PREFIXES {
            if let Some(rest) = trimmed.strip_prefix(prefix) {
                let ident: String = rest
                    .chars()
                    .take_while(|&c| c.is_ascii_alphanumeric() || c == '_')
                    .collect();
                if ident.len() >= 3 && seen.insert(ident.clone()) {
                    out.push((ident, rel_file.to_string()));
                }
                break;
            }
        }
    }
}

fn current_unix_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fuzzy_score_boundary_and_camel_bonuses() {
        let exact = fuzzy_score("src/ui/input.rs", "input").unwrap();
        let scattered = fuzzy_score("src/utils/internal_parser_unit.rs", "input").unwrap();
        assert!(
            exact > scattered,
            "Basename consecutive match ({exact}) must outscore scattered subsequence ({scattered})"
        );

        let camel = fuzzy_score("PromptSignalVector", "psv").unwrap();
        assert!(camel > 0);
        assert!(fuzzy_score("src/main.rs", "xyz").is_none());
    }

    #[test]
    fn test_prompt_history_up_down_and_prefix_lock() {
        let mut hist = PromptHistory::new();
        hist.seed_prompt("cargo test --lib");
        hist.seed_prompt("fix the login button alignment");
        hist.seed_prompt("fix the navbar z-index");

        // 1. Unfiltered Up from empty draft recalls newest ("fix the navbar z-index")
        assert_eq!(
            hist.navigate_up("").as_deref(),
            Some("fix the navbar z-index")
        );
        assert_eq!(
            hist.navigate_up("fix the navbar z-index").as_deref(),
            Some("fix the login button alignment")
        );
        // Down steps forward and then restores empty draft
        assert_eq!(
            hist.navigate_down().as_deref(),
            Some("fix the navbar z-index")
        );
        assert_eq!(hist.navigate_down().as_deref(), Some(""));

        // 2. Prefix-locked Up when user typed "cargo" jumps directly to "cargo test --lib"
        assert_eq!(
            hist.navigate_up("cargo").as_deref(),
            Some("cargo test --lib")
        );
        // Down restores the in-progress draft "cargo"
        assert_eq!(hist.navigate_down().as_deref(), Some("cargo"));
    }

    #[test]
    fn test_extract_trigger_and_slash_arg_completion() {
        let t_file = IntellisenseEngine::extract_trigger("inspect @src/ui/in", 18, 0).unwrap();
        assert_eq!(t_file.kind, TriggerKind::FileMention);
        assert_eq!(t_file.query, "src/ui/in");
        assert_eq!(t_file.replace_start_col, 8);

        let t_sym = IntellisenseEngine::extract_trigger("check #InputD", 13, 0).unwrap();
        assert_eq!(t_sym.kind, TriggerKind::SymbolMention);
        assert_eq!(t_sym.query, "InputD");

        let t_arg = IntellisenseEngine::extract_trigger("/caveman ul", 11, 0).unwrap();
        assert_eq!(
            t_arg.kind,
            TriggerKind::SlashArg {
                command: "/caveman".to_string()
            }
        );
        assert_eq!(t_arg.query, "ul");

        let mut engine = IntellisenseEngine::new();
        let cands = engine.query_candidates(&t_arg, None);
        assert!(!cands.is_empty());
        assert_eq!(cands[0].insert_text, "ultra");
    }

    #[test]
    fn test_extract_first_ghost_word() {
        assert_eq!(extract_first_ghost_word(" the login button"), " the");
        assert_eq!(extract_first_ghost_word("ui/input.rs "), "ui/");
    }
}

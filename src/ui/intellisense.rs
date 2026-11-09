//! Native Rust TUI Chatbox Intellisense, Inline Ghost-Text Hinter, Boundary-Bonus Fuzzy Matcher,
//! and Frecency/Prefix-Locked Prompt History Engine.
//!
//! Designed for sub-millisecond keystroke latency (`<0.05ms`) and minimal memory footprint (`<80 KB`)
//! with zero external autocomplete crate dependencies.

use crate::constants::{
    FUZZY_BASENAME_MATCH_BONUS, FUZZY_BASE_CHAR_SCORE, FUZZY_BOUNDARY_SEPARATOR_BONUS,
    FUZZY_CAMEL_CASE_BONUS, FUZZY_CONSECUTIVE_CHAR_BONUS, FUZZY_EXACT_PREFIX_BONUS,
    FUZZY_GAP_PENALTY_PER_CHAR, INTELLISENSE_EXACT_TOKEN_PREFIX_BONUS,
    INTELLISENSE_HISTORY_FRECENCY_BONUS, INTELLISENSE_MAX_FILE_INDEX, INTELLISENSE_MAX_POPUP_ITEMS,
    INTELLISENSE_MAX_SCAN_DEPTH, INTELLISENSE_MAX_SYMBOL_INDEX, INTELLISENSE_MAX_TASK_INDEX,
    INTELLISENSE_MIN_WORD_CHARS, PROMPT_HISTORY_FILE, PROMPT_HISTORY_MAX_ENTRIES,
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

#[inline]
fn char_slice(s: &str, start_char: usize) -> &str {
    if start_char == 0 {
        return s;
    }
    s.char_indices()
        .nth(start_char)
        .map_or("", |(byte_idx, _)| &s[byte_idx..])
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

/// Frecency-weighted, chronological + prefix-aware prompt history ring buffer (`Atuin` + `Fish` style).
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
    /// Prefix filter locked when using `Alt+Up` (`navigate_up_prefix`).
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
        self.draft_stash.clear();

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

    /// Navigates chronologically to the next older prompt (`None -> 0 -> 1 -> ... -> len - 1`).
    ///
    /// When already at the oldest entry (`len - 1`), clamps `cursor` at `Some(len - 1)` and returns
    /// that oldest entry so pressing `Up` multiple times at the top boundary never resets `cursor`
    /// and subsequent `Down` presses immediately step back toward newer prompts and `draft_stash`.
    pub fn navigate_up(&mut self, current_text: &str) -> Option<String> {
        if self.entries.is_empty() {
            return None;
        }

        let next_idx = match self.cursor {
            None => {
                self.draft_stash = current_text.to_string();
                self.prefix_filter.clear();
                0
            }
            Some(cur) => {
                if cur + 1 < self.entries.len() {
                    cur + 1
                } else {
                    self.cursor = Some(cur);
                    return Some(self.entries[cur].text.clone());
                }
            }
        };

        self.cursor = Some(next_idx);
        Some(self.entries[next_idx].text.clone())
    }

    /// Navigates to the next older prompt matching the current single-line prefix (`Alt+Up`).
    pub fn navigate_up_prefix(&mut self, current_text: &str) -> Option<String> {
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

        if !self.prefix_filter.is_empty() {
            if let Some(found_idx) = (start_search_from..self.entries.len()).find(|&idx| {
                starts_with_ignore_ascii_case(&self.entries[idx].text, &self.prefix_filter)
                    && self.entries[idx].text != current_text
            }) {
                self.cursor = Some(found_idx);
                return Some(self.entries[found_idx].text.clone());
            }
            if let Some(cur) = self.cursor {
                return Some(self.entries[cur].text.clone());
            }
            self.prefix_filter.clear();
        }

        self.navigate_up(current_text)
    }

    /// Navigates toward newer prompts; restores `draft_stash` when stepping past index `0`.
    pub fn navigate_down(&mut self) -> Option<String> {
        let cur = self.cursor?;
        if cur == 0 {
            self.cursor = None;
            self.prefix_filter.clear();
            return Some(self.draft_stash.clone());
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
            return Some(self.draft_stash.clone());
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

    /// Predicts full-phrase and Markov word-ngram continuations from user history.
    ///
    /// Returns `(insert_text_from_token_start, display_label, score)`.
    #[must_use]
    pub fn predict_continuations(
        &self,
        line_prefix: &str,
        replace_start_col: usize,
        token_query: &str,
        prev_word: &str,
    ) -> Vec<(String, String, i32)> {
        let mut results: Vec<(String, String, i32)> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();

        for (recency_rank, entry) in self.entries.iter().enumerate() {
            let first_line = entry.text.lines().next().unwrap_or("").trim();
            if first_line.is_empty() {
                continue;
            }
            let recency_bonus = (20usize.saturating_sub(recency_rank)) as i32;
            let freq_bonus = (entry.count.min(10) as i32) * 4;

            // 1. Full-line prefix match
            if !line_prefix.is_empty()
                && first_line.len() > line_prefix.len()
                && starts_with_ignore_ascii_case(first_line, line_prefix)
            {
                let insert_part = char_slice(first_line, replace_start_col).to_string();
                if !insert_part.is_empty() && seen.insert(insert_part.to_ascii_lowercase()) {
                    results.push((
                        insert_part,
                        first_line.to_string(),
                        INTELLISENSE_HISTORY_FRECENCY_BONUS + 45 + recency_bonus + freq_bonus,
                    ));
                }
            }

            // 2. Markov bigram continuation (`prev_word -> next_words`)
            if !prev_word.is_empty() {
                let words: Vec<&str> = first_line.split_whitespace().collect();
                for win_idx in 0..words.len().saturating_sub(1) {
                    let w_prev = words[win_idx].trim_matches(|c: char| !c.is_ascii_alphanumeric());
                    if w_prev.eq_ignore_ascii_case(prev_word) {
                        let tail_end = (win_idx + 4).min(words.len());
                        let continuation = words[win_idx + 1..tail_end].join(" ");
                        if (token_query.is_empty()
                            || starts_with_ignore_ascii_case(&continuation, token_query))
                            && continuation.len() > token_query.len()
                            && seen.insert(continuation.to_ascii_lowercase())
                        {
                            let display = format!("{prev_word} {continuation}");
                            results.push((
                                continuation,
                                display,
                                INTELLISENSE_HISTORY_FRECENCY_BONUS
                                    + 25
                                    + recency_bonus
                                    + freq_bonus,
                            ));
                        }
                    }
                }
            }
        }

        results
    }
}

/// Kind of contextual or code-like intellisense trigger detected at the cursor position.
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
    /// IDE-style AI Code & Prompt Intellisense (mid-word token or next-word context prediction)
    CodePrompt {
        prev_word: String,
        prev_two_words: String,
        line_prefix: String,
    },
}

impl TriggerKind {
    #[must_use]
    pub const fn badge(&self) -> &'static str {
        match self {
            Self::FileMention => " Files ",
            Self::SymbolMention => " Symbols ",
            Self::SkillMention => " Skills ",
            Self::SlashArg { .. } => " Arguments ",
            Self::CodePrompt { .. } => " Intellisense ",
        }
    }

    /// Returns `true` for explicit sigil triggers (`@`, `#`, `$`, `/cmd <arg>`),
    /// and `false` for ambient `CodePrompt` intellisense.
    #[must_use]
    pub const fn is_explicit(&self) -> bool {
        !matches!(self, Self::CodePrompt { .. })
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

/// Curated AI coding & conversational prompt completions (`TriggerKind::CodePrompt`).
struct PromptPhraseSpec {
    phrase: &'static str,
    detail: &'static str,
}

const AI_PROMPT_PHRASES: &[PromptPhraseSpec] = &[
    PromptPhraseSpec {
        phrase: "so whats next",
        detail: "prompt",
    },
    PromptPhraseSpec {
        phrase: "so whats happening",
        detail: "prompt",
    },
    PromptPhraseSpec {
        phrase: "so whats the status",
        detail: "prompt",
    },
    PromptPhraseSpec {
        phrase: "ok go on",
        detail: "prompt",
    },
    PromptPhraseSpec {
        phrase: "yes go on",
        detail: "prompt",
    },
    PromptPhraseSpec {
        phrase: "continue with the next step",
        detail: "prompt",
    },
    PromptPhraseSpec {
        phrase: "fix the bug in ",
        detail: "prompt",
    },
    PromptPhraseSpec {
        phrase: "fix all compiler warnings and errors",
        detail: "prompt",
    },
    PromptPhraseSpec {
        phrase: "refactor this module for clean architecture",
        detail: "prompt",
    },
    PromptPhraseSpec {
        phrase: "implement the feature for ",
        detail: "prompt",
    },
    PromptPhraseSpec {
        phrase: "optimize performance and memory usage",
        detail: "prompt",
    },
    PromptPhraseSpec {
        phrase: "add unit tests for ",
        detail: "prompt",
    },
    PromptPhraseSpec {
        phrase: "run cargo test and fix any failures",
        detail: "prompt",
    },
    PromptPhraseSpec {
        phrase: "run cargo clippy and fix warnings",
        detail: "prompt",
    },
    PromptPhraseSpec {
        phrase: "check compilation and run tests",
        detail: "prompt",
    },
    PromptPhraseSpec {
        phrase: "review the recent git diff",
        detail: "prompt",
    },
    PromptPhraseSpec {
        phrase: "debug why this is failing",
        detail: "prompt",
    },
    PromptPhraseSpec {
        phrase: "explain how this works",
        detail: "prompt",
    },
    PromptPhraseSpec {
        phrase: "update documentation to match the codebase",
        detail: "prompt",
    },
    PromptPhraseSpec {
        phrase: "create a modern responsive UI for ",
        detail: "prompt",
    },
];

/// Context-triggered next-word / clause predictions after a specific preceding word or 2-word phrase.
struct ContextNextSpec {
    prev_context: &'static str,
    continuation: &'static str,
    detail: &'static str,
}

const CONTEXT_NEXT_SPECS: &[ContextNextSpec] = &[
    // "so ..." / "so whats ..."
    ContextNextSpec {
        prev_context: "so",
        continuation: "whats next",
        detail: "prompt",
    },
    ContextNextSpec {
        prev_context: "so",
        continuation: "whats happening",
        detail: "prompt",
    },
    ContextNextSpec {
        prev_context: "so whats",
        continuation: "next",
        detail: "prompt",
    },
    ContextNextSpec {
        prev_context: "so whats",
        continuation: "happening",
        detail: "prompt",
    },
    ContextNextSpec {
        prev_context: "so whats",
        continuation: "the status",
        detail: "prompt",
    },
    ContextNextSpec {
        prev_context: "whats",
        continuation: "next",
        detail: "prompt",
    },
    ContextNextSpec {
        prev_context: "whats",
        continuation: "happening",
        detail: "prompt",
    },
    // "ok ..." / "ok go ..."
    ContextNextSpec {
        prev_context: "ok",
        continuation: "go on",
        detail: "prompt",
    },
    ContextNextSpec {
        prev_context: "ok go",
        continuation: "on",
        detail: "prompt",
    },
    ContextNextSpec {
        prev_context: "yes",
        continuation: "go on",
        detail: "prompt",
    },
    // "fix ..."
    ContextNextSpec {
        prev_context: "fix",
        continuation: "the bug in ",
        detail: "prompt",
    },
    ContextNextSpec {
        prev_context: "fix",
        continuation: "all compiler warnings and errors",
        detail: "prompt",
    },
    ContextNextSpec {
        prev_context: "fix",
        continuation: "the failing tests",
        detail: "prompt",
    },
    // "run ..." / "cargo ..." / "git ..."
    ContextNextSpec {
        prev_context: "run",
        continuation: "cargo test",
        detail: "command",
    },
    ContextNextSpec {
        prev_context: "run",
        continuation: "cargo check",
        detail: "command",
    },
    ContextNextSpec {
        prev_context: "run",
        continuation: "cargo clippy -- -D warnings",
        detail: "command",
    },
    ContextNextSpec {
        prev_context: "cargo",
        continuation: "test",
        detail: "rust",
    },
    ContextNextSpec {
        prev_context: "cargo",
        continuation: "check",
        detail: "rust",
    },
    ContextNextSpec {
        prev_context: "cargo",
        continuation: "clippy -- -D warnings",
        detail: "rust",
    },
    ContextNextSpec {
        prev_context: "cargo",
        continuation: "build --release",
        detail: "rust",
    },
    ContextNextSpec {
        prev_context: "git",
        continuation: "status",
        detail: "git",
    },
    ContextNextSpec {
        prev_context: "git",
        continuation: "diff",
        detail: "git",
    },
    ContextNextSpec {
        prev_context: "git",
        continuation: "log --oneline -n 10",
        detail: "git",
    },
    // Code construct continuations ("pub ...", "async ...", "export ...")
    ContextNextSpec {
        prev_context: "pub",
        continuation: "fn ",
        detail: "rust",
    },
    ContextNextSpec {
        prev_context: "pub",
        continuation: "struct ",
        detail: "rust",
    },
    ContextNextSpec {
        prev_context: "pub",
        continuation: "enum ",
        detail: "rust",
    },
    ContextNextSpec {
        prev_context: "pub",
        continuation: "async fn ",
        detail: "rust",
    },
    ContextNextSpec {
        prev_context: "async",
        continuation: "fn ",
        detail: "rust",
    },
    ContextNextSpec {
        prev_context: "export",
        continuation: "function ",
        detail: "typescript",
    },
    ContextNextSpec {
        prev_context: "export",
        continuation: "interface ",
        detail: "typescript",
    },
];

/// Code keyword & developer CLI completions for IDE-like token intellisense.
struct CodeKeywordSpec {
    token: &'static str,
    detail: &'static str,
}

const CODE_KEYWORD_SPECS: &[CodeKeywordSpec] = &[
    CodeKeywordSpec {
        token: "async fn ",
        detail: "rust",
    },
    CodeKeywordSpec {
        token: "pub fn ",
        detail: "rust",
    },
    CodeKeywordSpec {
        token: "pub struct ",
        detail: "rust",
    },
    CodeKeywordSpec {
        token: "pub enum ",
        detail: "rust",
    },
    CodeKeywordSpec {
        token: "pub trait ",
        detail: "rust",
    },
    CodeKeywordSpec {
        token: "anyhow::Result",
        detail: "rust",
    },
    CodeKeywordSpec {
        token: "tokio::spawn",
        detail: "rust",
    },
    CodeKeywordSpec {
        token: "serde::Serialize",
        detail: "rust",
    },
    CodeKeywordSpec {
        token: "cargo test",
        detail: "command",
    },
    CodeKeywordSpec {
        token: "cargo check",
        detail: "command",
    },
    CodeKeywordSpec {
        token: "cargo clippy",
        detail: "command",
    },
    CodeKeywordSpec {
        token: "export default function ",
        detail: "typescript",
    },
    CodeKeywordSpec {
        token: "export interface ",
        detail: "typescript",
    },
];

/// Lazy-indexed workspace completion & AI code/prompt intellisense engine.
#[derive(Debug, Clone, Default)]
pub struct IntellisenseEngine {
    indexed_root: Option<PathBuf>,
    pub files: Vec<String>,
    pub symbols: Vec<(String, String)>,
    pub skills: Vec<(String, String)>,
    pub tasks: Vec<String>,
    pub selected_index: usize,
    pub dismissed_for_query: Option<String>,
    /// True when the user explicitly focused the ambient popup via `Alt+Down`/`Alt+Up` or `Ctrl+N`/`Ctrl+P`.
    pub popup_explicitly_focused: bool,
}

impl IntellisenseEngine {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Ensures the workspace file, symbol, skill, and active task index is populated for `workspace_root`.
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
        self.tasks.clear();

        self.index_workspace_tasks(root);

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

    fn index_workspace_tasks(&mut self, root: &Path) {
        let candidates = [
            root.join("minikit_docs/core/todo.md"),
            root.join(".minicode/plan/task_plan.md"),
            root.join("TODO.md"),
        ];
        let mut seen = HashSet::new();
        for path in candidates {
            let Ok(content) = std::fs::read_to_string(&path) else {
                continue;
            };
            for line in content.lines() {
                if self.tasks.len() >= INTELLISENSE_MAX_TASK_INDEX {
                    return;
                }
                let trimmed = line.trim_start();
                let task_body = trimmed
                    .strip_prefix("- [>] ")
                    .or_else(|| trimmed.strip_prefix("- [ ] "))
                    .or_else(|| trimmed.strip_prefix("* [ ] "));
                if let Some(raw) = task_body {
                    let clean = raw
                        .trim()
                        .trim_start_matches("**")
                        .replace("**", "")
                        .replace('`', "");
                    let short: String = clean.chars().take(72).collect();
                    if short.len() >= 6 && seen.insert(short.to_ascii_lowercase()) {
                        self.tasks.push(short);
                    }
                }
            }
        }
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

    /// Detects if the cursor is currently positioned inside an explicit trigger token
    /// (`@<file>`, `#<symbol>`, `$<skill>`, `/<cmd> <arg>`) OR an ambient AI Code & Prompt
    /// Intellisense token (`TriggerKind::CodePrompt`).
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
                return None;
            }
        }

        // 2. Scan backward from `col` to the start of the current whitespace/delimiter token
        let token_start = chars[..col]
            .iter()
            .rposition(|&c| c.is_whitespace() || matches!(c, '(' | '[' | '{' | '"' | '\'' | ','))
            .map_or(0, |idx| idx + 1);

        if token_start < col {
            let marker = chars[token_start];
            match marker {
                '@' => {
                    let query: String = chars[token_start + 1..col].iter().collect();
                    return Some(TriggerToken {
                        kind: TriggerKind::FileMention,
                        query,
                        replace_start_col: token_start,
                        replace_end_col: col,
                    });
                }
                '#' => {
                    let query: String = chars[token_start + 1..col].iter().collect();
                    return Some(TriggerToken {
                        kind: TriggerKind::SymbolMention,
                        query,
                        replace_start_col: token_start,
                        replace_end_col: col,
                    });
                }
                '$' => {
                    let query: String = chars[token_start + 1..col].iter().collect();
                    return Some(TriggerToken {
                        kind: TriggerKind::SkillMention,
                        query,
                        replace_start_col: token_start,
                        replace_end_col: col,
                    });
                }
                _ => {}
            }
        }

        // 3. Ambient IDE-Style Code & Prompt Intellisense (`TriggerKind::CodePrompt`)
        let before_token: String = chars[..token_start].iter().collect();
        let prev_words: Vec<&str> = before_token
            .split_whitespace()
            .map(|w| w.trim_matches(|c: char| !c.is_ascii_alphanumeric()))
            .filter(|w| !w.is_empty())
            .collect();
        let prev_word = prev_words
            .last()
            .map(|w| w.to_ascii_lowercase())
            .unwrap_or_default();
        let prev_two_words = if prev_words.len() >= 2 {
            format!(
                "{} {}",
                prev_words[prev_words.len() - 2].to_ascii_lowercase(),
                prev_word
            )
        } else {
            prev_word.clone()
        };

        let query: String = chars[token_start..col].iter().collect();
        let query_char_len = query.chars().count();

        // Activate when typing a word token >= INTELLISENSE_MIN_WORD_CHARS (2 chars)
        // OR right after a space when the preceding word(s) have context continuations
        let has_space_continuation = query_char_len == 0
            && col == chars.len()
            && !prev_word.is_empty()
            && (CONTEXT_NEXT_SPECS
                .iter()
                .any(|s| s.prev_context == prev_word || s.prev_context == prev_two_words)
                || matches!(
                    prev_word.as_str(),
                    "in" | "inspect" | "refactor" | "update" | "check" | "implement"
                ));

        if query_char_len >= INTELLISENSE_MIN_WORD_CHARS || has_space_continuation {
            return Some(TriggerToken {
                kind: TriggerKind::CodePrompt {
                    prev_word,
                    prev_two_words,
                    line_prefix: prefix,
                },
                query,
                replace_start_col: token_start,
                replace_end_col: col,
            });
        }

        None
    }

    /// Returns up to `INTELLISENSE_MAX_POPUP_ITEMS` ranked completion candidates for `trigger`.
    pub fn query_candidates(
        &mut self,
        trigger: &TriggerToken,
        workspace_root: Option<&Path>,
    ) -> Vec<CompletionCandidate> {
        self.query_candidates_with_history(trigger, workspace_root, None)
    }

    /// Returns up to `INTELLISENSE_MAX_POPUP_ITEMS` ranked completion candidates for `trigger`,
    /// incorporating learned user prompt history when `history` is provided.
    pub fn query_candidates_with_history(
        &mut self,
        trigger: &TriggerToken,
        workspace_root: Option<&Path>,
        history: Option<&PromptHistory>,
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
            TriggerKind::CodePrompt {
                prev_word,
                prev_two_words,
                line_prefix,
            } => self.rank_code_prompt_candidates(
                trigger,
                prev_word,
                prev_two_words,
                line_prefix,
                history,
            ),
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

    fn rank_code_prompt_candidates(
        &self,
        trigger: &TriggerToken,
        prev_word: &str,
        prev_two_words: &str,
        line_prefix: &str,
        history: Option<&PromptHistory>,
    ) -> Vec<CompletionCandidate> {
        let mut out: Vec<CompletionCandidate> = Vec::new();
        let mut seen_labels: HashSet<String> = HashSet::new();
        let q = trigger.query.as_str();

        // 1. Learned User History Continuations (Full-line prefix + Markov bigrams)
        if let Some(hist) = history {
            for (insert_text, display, score) in
                hist.predict_continuations(line_prefix, trigger.replace_start_col, q, prev_word)
            {
                if seen_labels.insert(display.to_ascii_lowercase()) {
                    out.push(CompletionCandidate {
                        insert_text,
                        display_label: display,
                        detail: "history".to_string(),
                        score,
                    });
                }
            }
        }

        // 2. Context-Triggered Next-Word / Clause Predictions (`so whats -> next`, `cargo -> test`, etc.)
        if !prev_word.is_empty() {
            for (spec_idx, spec) in CONTEXT_NEXT_SPECS.iter().enumerate() {
                let matched_two = spec.prev_context == prev_two_words;
                let matched_one = spec.prev_context == prev_word;
                if !matched_two && !matched_one {
                    continue;
                }
                if !q.is_empty() && !starts_with_ignore_ascii_case(spec.continuation, q) {
                    continue;
                }
                if spec.continuation.eq_ignore_ascii_case(q) {
                    continue;
                }
                let display = format!("{} {}", spec.prev_context, spec.continuation);
                if seen_labels.insert(display.to_ascii_lowercase()) {
                    let context_bonus = if matched_two { 110 } else { 98 };
                    out.push(CompletionCandidate {
                        insert_text: spec.continuation.to_string(),
                        display_label: display,
                        detail: spec.detail.to_string(),
                        score: context_bonus + (q.len() as i32 * 6) - (spec_idx as i32),
                    });
                }
            }
        }

        // 3. Full-Line AI Prompt Phrase Templates (`so whats next`, `fix the bug in `, etc.)
        let trimmed_line = line_prefix.trim_start();
        if !trimmed_line.is_empty() {
            for (spec_idx, spec) in AI_PROMPT_PHRASES.iter().enumerate() {
                if spec.phrase.len() > trimmed_line.len()
                    && starts_with_ignore_ascii_case(spec.phrase, trimmed_line)
                {
                    let insert_part = char_slice(spec.phrase, trigger.replace_start_col);
                    if !insert_part.is_empty()
                        && seen_labels.insert(spec.phrase.to_ascii_lowercase())
                    {
                        out.push(CompletionCandidate {
                            insert_text: insert_part.to_string(),
                            display_label: spec.phrase.to_string(),
                            detail: spec.detail.to_string(),
                            score: 92 + (trimmed_line.len() as i32 * 2) - (spec_idx as i32),
                        });
                    }
                }
            }
        }

        // 4. Context-Triggered Active Tasks from `todo.md` (`implement ...`)
        if matches!(
            prev_word,
            "implement" | "complete" | "finish" | "continue" | "task"
        ) {
            for task in self.tasks.iter().take(4) {
                if q.is_empty() || contains_ignore_ascii_case(task, q) {
                    if seen_labels.insert(task.to_ascii_lowercase()) {
                        out.push(CompletionCandidate {
                            insert_text: task.clone(),
                            display_label: task.clone(),
                            detail: "task".to_string(),
                            score: 84,
                        });
                    }
                }
            }
        }

        // 5. Context-Triggered Workspace Files right after `in `, `inspect `, `refactor `, `check `, `update `
        if q.is_empty()
            && matches!(
                prev_word,
                "in" | "inspect" | "refactor" | "check" | "update"
            )
        {
            for path in self.files.iter().take(4) {
                if seen_labels.insert(path.to_ascii_lowercase()) {
                    out.push(CompletionCandidate {
                        insert_text: format!("{path} "),
                        display_label: path.clone(),
                        detail: "file".to_string(),
                        score: 76,
                    });
                }
            }
        }

        if q.len() < INTELLISENSE_MIN_WORD_CHARS {
            return out;
        }

        // 6. Code Keywords & CLI Idioms (`async fn`, `pub struct`, `cargo test`, `anyhow::Result`)
        for kw in CODE_KEYWORD_SPECS {
            if kw.token.trim().len() > q.len() && starts_with_ignore_ascii_case(kw.token, q) {
                let label = kw.token.trim().to_string();
                if seen_labels.insert(label.to_ascii_lowercase()) {
                    out.push(CompletionCandidate {
                        insert_text: kw.token.to_string(),
                        display_label: label,
                        detail: kw.detail.to_string(),
                        score: 78 + (q.len() as i32 * 5),
                    });
                }
            }
        }

        // 7. Workspace AST Code Symbols (`InputDock`, `IntellisenseEngine`, `fuzzy_score`, etc.)
        for (sym, file_hint) in &self.symbols {
            if sym.eq_ignore_ascii_case(q) {
                continue;
            }
            let is_exact_prefix = starts_with_ignore_ascii_case(sym, q);
            if !is_exact_prefix && q.len() < 3 {
                continue;
            }
            if let Some(mut score) = fuzzy_score(sym, q) {
                if is_exact_prefix {
                    score += INTELLISENSE_EXACT_TOKEN_PREFIX_BONUS;
                }
                if score >= 36 && seen_labels.insert(sym.to_ascii_lowercase()) {
                    out.push(CompletionCandidate {
                        insert_text: format!("{sym} "),
                        display_label: sym.clone(),
                        detail: file_hint.clone(),
                        score,
                    });
                }
            }
        }

        // 8. Workspace Filenames & Paths (`src/ui/intellisense.rs`, `Cargo.toml`, etc.)
        for path in &self.files {
            let basename = &path[path.rfind('/').map_or(0, |idx| idx + 1)..];
            if basename.eq_ignore_ascii_case(q) || path.eq_ignore_ascii_case(q) {
                continue;
            }
            let is_basename_prefix = starts_with_ignore_ascii_case(basename, q)
                || starts_with_ignore_ascii_case(path, q);
            if !is_basename_prefix && q.len() < 3 {
                continue;
            }
            if let Some(mut score) = fuzzy_score(path, q) {
                if is_basename_prefix {
                    score += INTELLISENSE_EXACT_TOKEN_PREFIX_BONUS;
                }
                if score >= 38 && seen_labels.insert(path.to_ascii_lowercase()) {
                    out.push(CompletionCandidate {
                        insert_text: format!("{path} "),
                        display_label: path.clone(),
                        detail: "file".to_string(),
                        score,
                    });
                }
            }
        }

        out
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

        // 2. Full-line frecency-weighted prompt history prefix completion (highest priority for full lines)
        if let Some(hist_ghost) = history.suggest_ghost_suffix(line) {
            return Some(hist_ghost);
        }

        // 3. Active trigger (`/<cmd> <arg>`, `@file`, `#symbol`, `$skill`, or `CodePrompt`) completion
        if let Some(trigger) = Self::extract_trigger(line, cursor_col, cursor_row) {
            let candidates =
                self.query_candidates_with_history(&trigger, workspace_root, Some(history));
            let idx = self.selected_index.min(candidates.len().saturating_sub(1));
            if let Some(best) = candidates.get(idx).or_else(|| candidates.first()) {
                let raw_target = match trigger.kind {
                    TriggerKind::FileMention
                    | TriggerKind::SymbolMention
                    | TriggerKind::SkillMention => best
                        .insert_text
                        .trim_end()
                        .trim_start_matches(['@', '#', '$']),
                    TriggerKind::SlashArg { .. } | TriggerKind::CodePrompt { .. } => {
                        best.insert_text.trim_end()
                    }
                };
                if trigger.query.is_empty() && !raw_target.is_empty() {
                    return Some(raw_target.to_string());
                }
                if raw_target.len() > trigger.query.len()
                    && starts_with_ignore_ascii_case(raw_target, &trigger.query)
                {
                    return Some(raw_target[trigger.query.len()..].to_string());
                }
            }
        }

        None
    }
}

#[inline]
fn contains_ignore_ascii_case(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    if haystack.len() < needle.len() {
        return false;
    }
    haystack
        .as_bytes()
        .windows(needle.len())
        .any(|w| w.eq_ignore_ascii_case(needle.as_bytes()))
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
        "pub type ",
        "type ",
        "pub const ",
        "const ",
        "pub mod ",
        "mod ",
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
    fn test_prompt_history_up_up_down_boundary_and_prefix_lock() {
        let mut hist = PromptHistory::new();
        hist.seed_prompt("hii");
        hist.seed_prompt("so whats");

        // 1. Exact user scenario: submit "hii", then "so whats".
        // Up -> "so whats", Up -> "hii", Up again at boundary -> stays "hii", Down -> "so whats", Down -> ""
        assert_eq!(hist.navigate_up("").as_deref(), Some("so whats"));
        assert_eq!(hist.navigate_up("so whats").as_deref(), Some("hii"));
        assert_eq!(
            hist.navigate_up("hii").as_deref(),
            Some("hii"),
            "Pressing Up at the oldest entry must clamp at 'hii' without resetting cursor"
        );
        assert_eq!(
            hist.navigate_down().as_deref(),
            Some("so whats"),
            "Pressing Down after clamping at oldest entry must step back to newer prompt 'so whats'"
        );
        assert_eq!(hist.navigate_down().as_deref(), Some(""));

        // 2. Unsubmitted draft scenario: user typed "so whats" without pressing Enter, pressed Up to get "hii",
        // pressed Up again, then pressed Down -> gets back unsubmitted draft "so whats"
        let mut hist2 = PromptHistory::new();
        hist2.seed_prompt("hii");
        assert_eq!(hist2.navigate_up("so whats").as_deref(), Some("hii"));
        assert_eq!(hist2.navigate_up("hii").as_deref(), Some("hii"));
        assert_eq!(hist2.navigate_down().as_deref(), Some("so whats"));

        // 3. Explicit prefix-locked navigation (`Alt+Up` / `navigate_up_prefix`)
        hist.seed_prompt("cargo test --lib");
        hist.seed_prompt("fix the navbar z-index");
        assert_eq!(
            hist.navigate_up_prefix("cargo").as_deref(),
            Some("cargo test --lib")
        );
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
    fn test_code_and_prompt_ai_intellisense_predictions() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("src/ui")).unwrap();
        std::fs::write(
            dir.path().join("src/ui/input.rs"),
            "pub struct InputDock {}\npub fn handle_key() {}\n",
        )
        .unwrap();

        let mut engine = IntellisenseEngine::new();
        let hist = PromptHistory::new();

        // 1. Conversational prompt prediction: "so wh" -> "whats next" (full line: "so whats next")
        let trig_prompt = IntellisenseEngine::extract_trigger("so wh", 5, 0).unwrap();
        assert!(!trig_prompt.kind.is_explicit());
        let cands =
            engine.query_candidates_with_history(&trig_prompt, Some(dir.path()), Some(&hist));
        assert!(!cands.is_empty());
        assert_eq!(cands[0].insert_text, "whats next");
        let ghost = engine.compute_ghost_hint("so wh", 5, 0, &hist, Some(dir.path()), None);
        assert_eq!(ghost.as_deref(), Some("ats next"));

        // 2. Next-word prediction after space: "so whats " -> "next"
        let ghost_space =
            engine.compute_ghost_hint("so whats ", 9, 0, &hist, Some(dir.path()), None);
        assert_eq!(ghost_space.as_deref(), Some("next"));

        // 3. Ambient code symbol intellisense without `#`: "refactor InputD" -> "InputDock "
        let trig_code = IntellisenseEngine::extract_trigger("refactor InputD", 15, 0).unwrap();
        let code_cands =
            engine.query_candidates_with_history(&trig_code, Some(dir.path()), Some(&hist));
        assert!(code_cands.iter().any(|c| c.insert_text == "InputDock "));
        let code_ghost =
            engine.compute_ghost_hint("refactor InputD", 15, 0, &hist, Some(dir.path()), None);
        assert_eq!(code_ghost.as_deref(), Some("ock"));
    }

    #[test]
    fn test_extract_first_ghost_word() {
        assert_eq!(extract_first_ghost_word(" the login button"), " the");
        assert_eq!(extract_first_ghost_word("ui/input.rs "), "ui/");
    }
}

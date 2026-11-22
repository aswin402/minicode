//! Native Rust TUI Chatbox Intellisense, Inline Ghost-Text Hinter, Boundary-Bonus Fuzzy Matcher,
//! Unified `@` Multimodal Entity Graph (`Files + Functions + Types/Classes + Images`),
//! and Global Omnibox/Gboard Kneser-Ney Prompt Recommender.
//!
//! Architectural Guarantees:
//! - Sub-millisecond keystroke latency (`< 0.05ms`) with zero regexes and zero hardcoded code-keyword lists.
//! - `/` commands are strictly isolated from `PromptHistory` and `PromptCorpus`.
//! - `@` mentions unify workspace files, Tree-sitter AST functions/methods, classes/structs/types
//!   (backed by `.minicode/graph_shards.bin` + `RepoMapExtractor`), and project/screenshot images.

use crate::constants::{
    CANDIDATE_KIND_COUNT, FUZZY_BASENAME_MATCH_BONUS, FUZZY_BASE_CHAR_SCORE,
    FUZZY_BOUNDARY_SEPARATOR_BONUS, FUZZY_CAMEL_CASE_BONUS, FUZZY_CONSECUTIVE_CHAR_BONUS,
    FUZZY_EXACT_PREFIX_BONUS, FUZZY_GAP_PENALTY_PER_CHAR, INTELLISENSE_EXACT_TOKEN_PREFIX_BONUS,
    INTELLISENSE_MAX_FILE_INDEX, INTELLISENSE_MAX_IMAGE_INDEX, INTELLISENSE_MAX_POPUP_ITEMS,
    INTELLISENSE_MAX_SCAN_DEPTH, INTELLISENSE_MAX_SYMBOL_INDEX, INTELLISENSE_MAX_TASK_INDEX,
    INTELLISENSE_MIN_WORD_CHARS, MENU_STYLE_COUNT, PROMPT_HISTORY_FILE, PROMPT_HISTORY_MAX_ENTRIES,
    TRIGGER_KIND_COUNT,
};
use crate::context::graph_store::GraphStore;
use crate::context::repomap::{RepoMapExtractor, SymbolDef};
use crate::ui::prompt_corpus::{
    compute_workspace_hash, is_slash_command_input, ordered_token_prefix_match, PromptCorpus,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::io::Read;
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
        .rposition(|&c| c == '/' || c == '\\' || c == ':')
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

    let basename = &candidate[candidate
        .rfind(['/', '\\', ':'])
        .map_or(0, |byte_idx| byte_idx + 1)..];
    if starts_with_ignore_ascii_case(candidate, query)
        || starts_with_ignore_ascii_case(basename, query)
    {
        score += FUZZY_EXACT_PREFIX_BONUS;
    }

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

/// Pure-Prompt History & Global Omnibox/Gboard Corpus Wrapper.
///
/// Slash commands (`/...`) are structurally excluded from `entries`, `.minicode/prompt_history.jsonl`,
/// and the global `PromptCorpus` so `Up`/`Down` strictly recalls conversational prompts.
#[derive(Debug, Clone, Default)]
pub struct PromptHistory {
    /// Newest-first deque of unique conversational prompt entries (workspace prioritized, then global).
    pub entries: VecDeque<HistoryEntry>,
    /// Global + workspace persistent corpus and Order-4 Kneser-Ney language model.
    pub corpus: PromptCorpus,
    /// Current navigation index into `entries` (`None` when editing live draft).
    pub cursor: Option<usize>,
    /// Unsubmitted user draft stashed when first pressing `Up`.
    pub draft_stash: String,
    /// Prefix/token filter locked when using `Alt+Up` (`navigate_up_prefix`).
    pub prefix_filter: String,
}

impl PromptHistory {
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: VecDeque::new(),
            corpus: PromptCorpus::new_in_memory(),
            cursor: None,
            draft_stash: String::new(),
            prefix_filter: String::new(),
        }
    }

    /// Loads workspace + global prompt history, purging any legacy `/command` lines.
    pub fn load_from_workspace(&mut self, workspace_root: &Path) {
        self.load_with_global_dir(workspace_root, None);
    }

    /// Loads workspace prompt history merged with a specific global corpus directory.
    pub fn load_with_global_dir(&mut self, workspace_root: &Path, global_dir: Option<PathBuf>) {
        self.corpus.bind_workspace(workspace_root, global_dir);
        self.sync_entries_from_corpus(Some(workspace_root));
    }

    fn sync_entries_from_corpus(&mut self, workspace_root: Option<&Path>) {
        let ws_hash = compute_workspace_hash(workspace_root);
        let mut local_recs = Vec::new();
        let mut global_recs = Vec::new();

        for rec in self.corpus.records() {
            if crate::ui::prompt_corpus::is_excluded_from_prompt_history(&rec.text) {
                continue;
            }
            if ws_hash != 0 && rec.workspace_hashes.contains(&ws_hash) {
                local_recs.push(rec);
            } else {
                global_recs.push(rec);
            }
        }

        local_recs.sort_by(|a, b| b.latest_timestamp().cmp(&a.latest_timestamp()));
        global_recs.sort_by(|a, b| b.latest_timestamp().cmp(&a.latest_timestamp()));

        self.entries.clear();
        for rec in local_recs.into_iter().chain(global_recs) {
            if self.entries.len() >= PROMPT_HISTORY_MAX_ENTRIES {
                break;
            }
            self.entries.push_back(HistoryEntry {
                text: rec.text.clone(),
                count: rec.total_count.max(1),
                timestamp: rec.latest_timestamp(),
            });
        }
    }

    /// Seeds a conversational prompt into in-memory history & LM during session hydration (without disk I/O).
    /// Slash commands (`/...`), internal `[SYSTEM ...]` wakeups, and collapsed paste chips are excluded.
    pub fn seed_prompt(&mut self, prompt: &str) {
        let clean = prompt.trim();
        if crate::ui::prompt_corpus::is_excluded_from_prompt_history(clean) {
            return;
        }
        let redacted = crate::security::redactor::redact_secrets(clean);
        if !redacted.warnings.is_empty() {
            return;
        }
        self.insert_or_promote_memory_only(clean.to_string(), 1, current_unix_secs());
        let _ = self.corpus.record_submission(clean);
    }

    /// Records a newly submitted prompt into slot `0` and persists to global + workspace stores.
    /// Slash commands (`/...`), internal `[SYSTEM ...]` wakeups, and orphaned paste chips are excluded.
    pub fn record(&mut self, prompt: &str, workspace_root: Option<&Path>) {
        let clean = prompt.trim();
        self.reset_navigation();
        self.draft_stash.clear();

        if crate::ui::prompt_corpus::is_excluded_from_prompt_history(clean) {
            return;
        }

        let redacted = crate::security::redactor::redact_secrets(clean);
        if !redacted.warnings.is_empty() {
            // Keep sanitized version in session memory for immediate Up-key editing, never on disk
            self.insert_or_promote_memory_only(redacted.content, 1, current_unix_secs());
            return;
        }

        let now = current_unix_secs();
        self.insert_or_promote_memory_only(clean.to_string(), 1, now);
        let _ = self.corpus.record_submission(clean);

        if let Some(root) = workspace_root {
            self.persist_to_workspace(root);
        }
    }

    fn insert_or_promote_memory_only(&mut self, text: String, add_count: u32, timestamp: u64) {
        if crate::ui::prompt_corpus::is_excluded_from_prompt_history(&text) {
            return;
        }
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
        let mut out = String::new();
        for entry in self.entries.iter().rev() {
            if crate::ui::prompt_corpus::is_excluded_from_prompt_history(&entry.text) {
                continue;
            }
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

    /// Navigates chronologically to the next older conversational prompt (`None -> 0 -> 1 -> ... -> len - 1`).
    pub fn navigate_up(&mut self, current_text: &str) -> Option<String> {
        // Defensive filter in case an external caller mutated `entries`
        while self
            .entries
            .front()
            .is_some_and(|e| is_slash_command_input(&e.text))
        {
            self.entries.pop_front();
        }
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
                let mut candidate = cur + 1;
                while candidate < self.entries.len()
                    && is_slash_command_input(&self.entries[candidate].text)
                {
                    candidate += 1;
                }
                if candidate < self.entries.len() {
                    candidate
                } else {
                    self.cursor = Some(cur);
                    return Some(self.entries[cur].text.clone());
                }
            }
        };

        self.cursor = Some(next_idx);
        Some(self.entries[next_idx].text.clone())
    }

    /// Navigates to the next older prompt matching the current prefix or Omnibox token sequence (`Alt+Up`).
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
            let filter_tokens: Vec<String> = self
                .prefix_filter
                .split_whitespace()
                .map(|t| t.to_lowercase())
                .collect();
            if let Some(found_idx) = (start_search_from..self.entries.len()).find(|&idx| {
                let cand = &self.entries[idx].text;
                !is_slash_command_input(cand)
                    && cand != current_text
                    && (starts_with_ignore_ascii_case(cand, &self.prefix_filter)
                        || (filter_tokens.len() >= 2
                            && ordered_token_prefix_match(cand, &filter_tokens)))
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
            let filter_tokens: Vec<String> = self
                .prefix_filter
                .split_whitespace()
                .map(|t| t.to_lowercase())
                .collect();
            if let Some(found_idx) = (0..cur).rev().find(|&idx| {
                let cand = &self.entries[idx].text;
                !is_slash_command_input(cand)
                    && (starts_with_ignore_ascii_case(cand, &self.prefix_filter)
                        || (filter_tokens.len() >= 2
                            && ordered_token_prefix_match(cand, &filter_tokens)))
            }) {
                self.cursor = Some(found_idx);
                return Some(self.entries[found_idx].text.clone());
            }
            self.cursor = None;
            self.prefix_filter.clear();
            return Some(self.draft_stash.clone());
        }

        let mut prev_idx = cur - 1;
        while prev_idx > 0 && is_slash_command_input(&self.entries[prev_idx].text) {
            prev_idx -= 1;
        }
        if is_slash_command_input(&self.entries[prev_idx].text) {
            self.cursor = None;
            return Some(self.draft_stash.clone());
        }
        self.cursor = Some(prev_idx);
        Some(self.entries[prev_idx].text.clone())
    }

    /// Computes inline dimmed ghost text from Omnibox Frecency + Order-4 Kneser-Ney Gboard rollout.
    #[must_use]
    pub fn suggest_ghost_suffix(&self, line_prefix: &str) -> Option<String> {
        self.corpus.suggest_ghost_suffix(line_prefix)
    }
}

/// Interactive facet filter inside the unified `@` mention popup (`Shift+Tab` cycles facets).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MentionFilter {
    #[default]
    All = 0,
    Files = 1,
    Functions = 2,
    Types = 3,
    Images = 4,
}

impl MentionFilter {
    #[must_use]
    pub const fn all() -> &'static [MentionFilter] {
        &[
            MentionFilter::All,
            MentionFilter::Files,
            MentionFilter::Functions,
            MentionFilter::Types,
            MentionFilter::Images,
        ]
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Files => "Files",
            Self::Functions => "Functions",
            Self::Types => "Types",
            Self::Images => "Images",
        }
    }

    #[must_use]
    pub const fn cycle(self, forward: bool) -> Self {
        let all = Self::all();
        let idx = self as usize;
        if forward {
            all[(idx + 1) % all.len()]
        } else if idx == 0 {
            all[all.len() - 1]
        } else {
            all[idx - 1]
        }
    }
}

struct ScopePrefixSpec {
    prefix: &'static str,
    filter: MentionFilter,
}

const MENTION_SCOPE_PREFIXES: &[ScopePrefixSpec] = &[
    ScopePrefixSpec {
        prefix: "fn:",
        filter: MentionFilter::Functions,
    },
    ScopePrefixSpec {
        prefix: "func:",
        filter: MentionFilter::Functions,
    },
    ScopePrefixSpec {
        prefix: "type:",
        filter: MentionFilter::Types,
    },
    ScopePrefixSpec {
        prefix: "cls:",
        filter: MentionFilter::Types,
    },
    ScopePrefixSpec {
        prefix: "class:",
        filter: MentionFilter::Types,
    },
    ScopePrefixSpec {
        prefix: "struct:",
        filter: MentionFilter::Types,
    },
    ScopePrefixSpec {
        prefix: "trait:",
        filter: MentionFilter::Types,
    },
    ScopePrefixSpec {
        prefix: "interface:",
        filter: MentionFilter::Types,
    },
    ScopePrefixSpec {
        prefix: "enum:",
        filter: MentionFilter::Types,
    },
    ScopePrefixSpec {
        prefix: "img:",
        filter: MentionFilter::Images,
    },
    ScopePrefixSpec {
        prefix: "image:",
        filter: MentionFilter::Images,
    },
    ScopePrefixSpec {
        prefix: "pic:",
        filter: MentionFilter::Images,
    },
    ScopePrefixSpec {
        prefix: "file:",
        filter: MentionFilter::Files,
    },
    ScopePrefixSpec {
        prefix: "path:",
        filter: MentionFilter::Files,
    },
];

/// Parses an `@` query for an optional scope prefix (`@fn:foo`, `@img:hero`, `@cls:Input`),
/// returning `(scoped_filter_override, stripped_query)`.
#[must_use]
pub fn parse_at_mention_query(raw_query: &str) -> (Option<MentionFilter>, &str) {
    for spec in MENTION_SCOPE_PREFIXES {
        if starts_with_ignore_ascii_case(raw_query, spec.prefix) {
            return (
                Some(spec.filter),
                raw_query.get(spec.prefix.len()..).unwrap_or(""),
            );
        }
    }
    (None, raw_query)
}

/// Kind of contextual or ambient intellisense trigger detected at the cursor position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TriggerKind {
    /// `@<query>` unified multimodal mention (`Files + Functions + Classes/Types + Images`)
    AtMention,
    /// `#<symbol_query>` explicit AST symbol alias
    SymbolMention,
    /// `$<skill_query>` Vault skill mention
    SkillMention,
    /// `/<cmd> <arg_query>` slash command argument completion
    SlashArg { command: String },
    /// Ambient conversational prompt continuation (`Omnibox + Gboard Kneser-Ney`, non-blocking strip)
    PromptFlow { line_prefix: String },
}

impl TriggerKind {
    /// Returns the zero-based lookup index into `MenuStyleSpec::trigger_badges`.
    #[must_use]
    pub const fn kind_index(&self) -> usize {
        match self {
            Self::AtMention => 0,
            Self::SymbolMention => 1,
            Self::SkillMention => 2,
            Self::SlashArg { .. } => 3,
            Self::PromptFlow { .. } => 4,
        }
    }

    #[must_use]
    pub const fn badge_for(&self, style: MenuStyle) -> &'static str {
        style.spec().trigger_badges[self.kind_index()]
    }

    #[must_use]
    pub const fn badge(&self) -> &'static str {
        self.badge_for(MenuStyle::Minimal)
    }

    /// Returns `true` for explicit sigil triggers (`@`, `#`, `$`, `/cmd <arg>`),
    /// and `false` for ambient `PromptFlow` suggestions.
    #[must_use]
    pub const fn is_explicit(&self) -> bool {
        !matches!(self, Self::PromptFlow { .. })
    }
}

/// Semantic category of a completion item, used as an `O(1)` index into `MenuStyleSpec::kind_prefixes`.
#[repr(usize)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CandidateKind {
    #[default]
    Prompt = 0,
    History = 1,
    Symbol = 2,
    File = 3,
    Command = 4,
    Task = 5,
    Skill = 6,
    Arg = 7,
    Image = 8,
}

/// Frame border rendering mode for the intellisense and command menu popup.
#[repr(usize)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuBorderMode {
    Rounded = 0,
    PlainAscii = 1,
    DoubleFrame = 2,
    BorderlessCompact = 3,
}

/// User-selectable visual theme for the floating Intellisense & Command Menu (`ui.menu_style`).
#[repr(usize)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum MenuStyle {
    #[default]
    Minimal = 0,
    Ascii = 1,
    Icons = 2,
    Emoji = 3,
    Kaomoji = 4,
    Compact = 5,
    Symbols = 6,
    NerdFont = 7,
    PoshCard = 8,
}

/// Declarative specification for a single `MenuStyle` variant (zero branching in render loops).
pub struct MenuStyleSpec {
    pub style: MenuStyle,
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub aliases: &'static [&'static str],
    pub border_mode: MenuBorderMode,
    /// Header badges indexed by `TriggerKind::kind_index()`:
    /// `[0: AtMention, 1: SymbolMention, 2: SkillMention, 3: SlashArg, 4: PromptFlow]`
    pub trigger_badges: [&'static str; TRIGGER_KIND_COUNT],
    /// Key hints indexed by `usize::from(trigger.kind.is_explicit())`:
    /// `[0: Ambient PromptFlow, 1: Explicit Sigil (@, #, $, /arg)]`
    pub key_hints: [&'static str; 2],
    /// Row selection prefix indexed by `usize::from(is_selected)`:
    /// `[0: Unselected, 1: Selected]`
    pub row_prefixes: [&'static str; 2],
    /// Candidate kind prefix indexed by `CandidateKind as usize`:
    /// `[0: Prompt, 1: History, 2: Symbol, 3: File, 4: Command, 5: Task, 6: Skill, 7: Arg, 8: Image]`
    pub kind_prefixes: [&'static str; CANDIDATE_KIND_COUNT],
    pub detail_wrap: (&'static str, &'static str),
    pub palette_chrome: (&'static str, &'static str, &'static str),
    pub preview: &'static [&'static str],
}

pub static MENU_STYLE_SPECS: [MenuStyleSpec; MENU_STYLE_COUNT] = [
    MenuStyleSpec {
        style: MenuStyle::Minimal,
        id: "minimal",
        name: "1. Minimal Clean (Default)",
        description: "Zero icons, emojis, or tags — pure typography with full-row accent bar",
        aliases: &["clean", "default", "none"],
        border_mode: MenuBorderMode::Rounded,
        trigger_badges: [
            " @ Mentions ",
            " Symbols ",
            " Skills ",
            " Arguments ",
            " Suggestions ",
        ],
        key_hints: [
            "[Tab/Right] Complete  [Alt+1..3] Pick  [Enter] Send  [Esc] Dismiss ",
            "[Tab/Enter] Insert  [Up/Down] Select  [Shift+Tab] Filter  [Esc] Dismiss ",
        ],
        row_prefixes: ["  ", "  "],
        kind_prefixes: ["", "", "", "", "", "", "", "", ""],
        detail_wrap: ("", ""),
        palette_chrome: (" Commands ", "   ", " ❯ "),
        preview: &[
            "╭ @ Mentions [Tab/Enter] Insert  [Shift+Tab] Filter ─────────╮",
            "│  @InputDock                    struct · src/ui/input.rs:L1 │",
            "│  @src/ui/intellisense.rs                         rs · file │",
            "│  @assets/hero.png                    png · 1440x900 · 48KB │",
            "╰────────────────────────────────────────────────────────────╯",
        ],
    },
    MenuStyleSpec {
        style: MenuStyle::Ascii,
        id: "ascii",
        name: "2. Pure ASCII Bracketed ([SYM] / +---+)",
        description:
            "Strict 7-bit ASCII box frame (+---+) with > pointer and [PRMPT]/[SYM]/[FILE]/[IMG] tags",
        aliases: &["retro", "plain", "bracket", "terminal"],
        border_mode: MenuBorderMode::PlainAscii,
        trigger_badges: [
            " [@MENTIONS] ",
            " [SYMBOLS] ",
            " [SKILLS] ",
            " [ARGS] ",
            " [SUGGESTIONS] ",
        ],
        key_hints: [
            "[Tab/Right] Complete  [Alt+1..3] Pick  [Esc] Close ",
            "[Tab/Enter] Insert  [Up/Dn] Select  [S-Tab] Filter ",
        ],
        row_prefixes: ["   ", " > "],
        kind_prefixes: [
            "[PRMPT] ", "[HIST]  ", "[SYM]   ", "[FILE]  ", "[CMD]   ", "[TASK]  ", "[SKL]   ",
            "[ARG]   ", "[IMG]   ",
        ],
        detail_wrap: ("(", ")"),
        palette_chrome: (" [COMMANDS] ", "   ", " > "),
        preview: &[
            "+ [@MENTIONS] [Tab/Enter] Insert  [S-Tab] Filter ------------+",
            "| > [SYM]   @InputDock                 (struct · input.rs:1) |",
            "|   [FILE]  @src/ui/intellisense.rs              (rs · file) |",
            "|   [IMG]   @assets/hero.png                  (png · 1440x900)|",
            "+------------------------------------------------------------+",
        ],
    },
    MenuStyleSpec {
        style: MenuStyle::Icons,
        id: "icons",
        name: "3. Modern IDE Icons (❯ / ƒ / § / ⌖ / ◩)",
        description: "Crisp 1-cell developer glyphs (ƒ symbol, § prompt, ⌖ file, ◩ image, ↺ history)",
        aliases: &["icon", "ide", "glyph", "glyphs"],
        border_mode: MenuBorderMode::Rounded,
        trigger_badges: [
            " @ Mentions ",
            " ƒ Symbols ",
            " ✦ Skills ",
            " ▸ Arguments ",
            " ✦ Suggestions ",
        ],
        key_hints: [
            "[Tab/→] Complete  [Alt+1..3] Pick  [Enter] Send  [Esc] Dismiss ",
            "[Tab/Enter] Insert  [↑/↓] Select  [⇧Tab] Filter  [Esc] Dismiss ",
        ],
        row_prefixes: ["   ", " ❯ "],
        kind_prefixes: ["§ ", "↺ ", "ƒ ", "⌖ ", "$ ", "✓ ", "✦ ", "▸ ", "◩ "],
        detail_wrap: ("", ""),
        palette_chrome: (" ⌘ Commands ", "   ", " ❯ "),
        preview: &[
            "╭ @ Mentions [Tab/Enter] Insert  [⇧Tab] Filter ──────────────╮",
            "│ ❯ ƒ @InputDock                 struct · src/ui/input.rs:L1 │",
            "│   ⌖ @src/ui/intellisense.rs                      rs · file │",
            "│   ◩ @assets/hero.png                 png · 1440x900 · 48KB │",
            "╰────────────────────────────────────────────────────────────╯",
        ],
    },
    MenuStyleSpec {
        style: MenuStyle::Emoji,
        id: "emoji",
        name: "4. Expressive Emoji (👉 / ⚡ / 🧩 / 📄 / 🖼️)",
        description: "Visual category emojis for prompts, AST symbols, files, images, and tasks",
        aliases: &["emojis", "expressive"],
        border_mode: MenuBorderMode::Rounded,
        trigger_badges: [
            " 🎯 Mentions ",
            " 🧩 Symbols ",
            " 🧠 Skills ",
            " 🔧 Arguments ",
            " ✨ Suggestions ",
        ],
        key_hints: [
            "[Tab/→] Complete  [Alt+1..3] Pick  [Enter] Send  [Esc] Dismiss ",
            "[Tab/Enter] Insert  [↑/↓] Select  [⇧Tab] Filter  [Esc] Dismiss ",
        ],
        row_prefixes: ["    ", " 👉 "],
        kind_prefixes: ["⚡ ", "🕒 ", "🧩 ", "📄 ", "⚙️ ", "✅ ", "🧠 ", "🔧 ", "🖼️ "],
        detail_wrap: ("", ""),
        palette_chrome: (" ✨ Commands ", "    ", " 👉 "),
        preview: &[
            "╭ 🎯 Mentions [Tab/Enter] Insert  [⇧Tab] Filter ─────────────╮",
            "│ 👉 🧩 @InputDock               struct · src/ui/input.rs:L1 │",
            "│    📄 @src/ui/intellisense.rs                    rs · file │",
            "│    🖼️ @assets/hero.png               png · 1440x900 · 48KB │",
            "╰────────────────────────────────────────────────────────────╯",
        ],
    },
    MenuStyleSpec {
        style: MenuStyle::Kaomoji,
        id: "kaomoji",
        name: "5. Kaomoji Expressions ((☞ﾟヮﾟ)☞ / (・ω・))",
        description: "Playful Japanese ASCII kaomoji pointers and double-line frame",
        aliases: &["kamoji", "kaomojis", "face", "japanese"],
        border_mode: MenuBorderMode::DoubleFrame,
        trigger_badges: [
            " (・ω・) @Mentions ",
            " (⌐■_■) Symbols ",
            " (✧ω✧) Skills ",
            " (•̀ᴗ•́)و Args ",
            " (◕‿◕) Suggestions ",
        ],
        key_hints: [
            "[Tab/→] Complete  [Alt+1..3] Pick  [Esc] Bye ",
            "[Tab/Enter] Insert  [↑/↓] Pick  [⇧Tab] Filter ",
        ],
        row_prefixes: ["  (•_•)  ", " (☞ﾟヮﾟ)☞ "],
        kind_prefixes: ["", "", "", "", "", "", "", "", ""],
        detail_wrap: ("[", "]"),
        palette_chrome: (" (◕‿◕) Commands ", "  (•_•)  ", " (☞ﾟヮﾟ)☞ "),
        preview: &[
            "╔ (・ω・) @Mentions [Tab/Enter] Insert  [⇧Tab] Filter ════════╗",
            "║ (☞ﾟヮﾟ)☞ @InputDock                [struct · input.rs:L1] ║",
            "║  (•_•)  @src/ui/intellisense.rs               [rs · file] ║",
            "║  (•_•)  @assets/hero.png                   [png · 1440x900]║",
            "╚════════════════════════════════════════════════════════════╝",
        ],
    },
    MenuStyleSpec {
        style: MenuStyle::Compact,
        id: "compact",
        name: "6. Compact Borderless Strip (▎ inline)",
        description: "Ultra-slim borderless 4-row floating ribbon with left accent bar (▎)",
        aliases: &["strip", "inline", "borderless", "slim"],
        border_mode: MenuBorderMode::BorderlessCompact,
        trigger_badges: [
            " @Mentions ",
            " Symbols ",
            " Skills ",
            " Args ",
            " Suggestions ",
        ],
        key_hints: ["[Tab] Complete", "[Tab] Insert  [⇧Tab] Filter"],
        row_prefixes: ["  ", "▎ "],
        kind_prefixes: ["", "", "", "", "", "", "", "", ""],
        detail_wrap: ("· ", ""),
        palette_chrome: (" Commands ", "  ", "▎ "),
        preview: &[
            "▎ @InputDock                    · struct · src/ui/input.rs:L1 ",
            "  @src/ui/intellisense.rs                         · rs · file ",
            "  @assets/hero.png                    · png · 1440x900 · 48KB ",
        ],
    },
    MenuStyleSpec {
        style: MenuStyle::Symbols,
        id: "symbols",
        name: "7. Unicode Technical Symbols (⎇ / ⌘ / ◈ / ✦ / ⚙)",
        description: "Universal 1-cell Unicode symbols (⎇ ⌘ ◈ ✦ ⚙ ◎) — crisp in every terminal font",
        aliases: &["symbol", "technical", "alt", "unicode"],
        border_mode: MenuBorderMode::Rounded,
        trigger_badges: [
            " ⎇ Mentions ",
            " ◈ Symbols ",
            " ✦ Skills ",
            " ⚙ Arguments ",
            " ⌘ Suggestions ",
        ],
        key_hints: [
            "[Tab/→] Complete  [Alt+1..3] Pick  [Enter] Send  [⎋] Close ",
            "[Tab/Enter] Insert  [↑/↓] Select  [⇧Tab] Filter  [⎋] Close ",
        ],
        row_prefixes: ["   ", " ❯ "],
        kind_prefixes: ["⌘ ", "◷ ", "◈ ", "⌂ ", "❯ ", "⎇ ", "✦ ", "⚙ ", "◎ "],
        detail_wrap: ("· ", ""),
        palette_chrome: (" ⌘ Commands ", "   ", " ❯ "),
        preview: &[
            "╭ ⎇ Mentions [Tab/Enter] Insert  [⇧Tab] Filter  [⎋] Close ───╮",
            "│ ❯ ◈ @InputDock               · struct · src/ui/input.rs:L1 │",
            "│   ⌂ @src/ui/intellisense.rs                    · rs · file │",
            "│   ◎ @assets/hero.png               · png · 1440x900 · 48KB │",
            "╰────────────────────────────────────────────────────────────╯",
        ],
    },
    MenuStyleSpec {
        style: MenuStyle::NerdFont,
        id: "nerdfont",
        name: "8. Nerd Font Codicons (Nerd Font)",
        description: "Collision-free Nerd Font Codicon icons per item category (requires Nerd Font)",
        aliases: &["nerd", "nf", "codicons", "nerd_font"],
        border_mode: MenuBorderMode::Rounded,
        trigger_badges: [
            " \u{ea68} Mentions ",
            " \u{ea8b} Symbols ",
            " \u{ea61} Skills ",
            " \u{eaf8} Arguments ",
            " \u{eb65} Suggestions ",
        ],
        key_hints: [
            "[Tab/→] Complete  [Alt+1..3] Pick  [Enter] Send  [Esc] Close ",
            "[Tab/Enter] Insert  [↑/↓] Select  [⇧Tab] Filter  [Esc] Close ",
        ],
        row_prefixes: ["   ", " ❯ "],
        kind_prefixes: [
            "\u{eb65} ",
            "\u{ea82} ",
            "\u{ea8b} ",
            "\u{ea7b} ",
            "\u{ea85} ",
            "\u{eaa6} ",
            "\u{ea61} ",
            "\u{eaf8} ",
            "\u{eb3f} ",
        ],
        detail_wrap: ("· ", ""),
        palette_chrome: (" \u{ea85} Commands ", "   ", " ❯ "),
        preview: &[
            "╭ \u{ea68} Mentions [Tab/Enter] Insert  [⇧Tab] Filter ──────────────╮",
            "│ ❯ \u{ea8b} @InputDock               · struct · src/ui/input.rs:L1 │",
            "│   \u{ea7b} @src/ui/intellisense.rs                    · rs · file │",
            "│   \u{eb3f} @assets/hero.png               · png · 1440x900 · 48KB │",
            "╰────────────────────────────────────────────────────────────╯",
        ],
    },
    MenuStyleSpec {
        style: MenuStyle::PoshCard,
        id: "posh_card",
        name: "9. Oh My Posh Minimal Rail (▎ + ⎇ / ◈)",
        description: "Airy Oh My Posh borderless strip with left rail ▎ and universal ⎇/◈ symbols",
        aliases: &["posh", "ohmyposh", "velvet", "rail"],
        border_mode: MenuBorderMode::BorderlessCompact,
        trigger_badges: [
            " ⎇ Mentions ",
            " ◈ Symbols ",
            " ✦ Skills ",
            " ⚙ Args ",
            " ⌘ Suggest ",
        ],
        key_hints: ["[Tab] Complete", "[Tab] Insert  [⇧Tab] Filter"],
        row_prefixes: ["  ", "▎ "],
        kind_prefixes: ["⌘ ", "◷ ", "◈ ", "⌂ ", "❯ ", "⎇ ", "✦ ", "⚙ ", "◎ "],
        detail_wrap: ("/ ", ""),
        palette_chrome: (" ⌘ Commands ", "  ", "▎ "),
        preview: &[
            "▎ ◈ @InputDock                  / struct · src/ui/input.rs:L1 ",
            "  ⌂ @src/ui/intellisense.rs                       / rs · file ",
            "  ◎ @assets/hero.png                  / png · 1440x900 · 48KB ",
        ],
    },
];

impl MenuStyle {
    #[must_use]
    pub fn from_str_loose(raw: &str) -> Self {
        let norm = raw.trim().to_ascii_lowercase().replace('-', "_");
        MENU_STYLE_SPECS
            .iter()
            .find_map(|spec| {
                (spec.id == norm || spec.aliases.contains(&norm.as_str())).then_some(spec.style)
            })
            .unwrap_or_default()
    }

    #[must_use]
    pub const fn spec(self) -> &'static MenuStyleSpec {
        &MENU_STYLE_SPECS[self as usize]
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        self.spec().id
    }
}

/// Active trigger token span on the current cursor row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TriggerToken {
    pub kind: TriggerKind,
    pub query: String,
    pub replace_start_col: usize,
    pub replace_end_col: usize,
}

/// A single ranked candidate shown in the floating intellisense popup or Gboard chip strip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionCandidate {
    pub insert_text: String,
    pub display_label: String,
    pub detail: String,
    pub kind: CandidateKind,
    pub score: i32,
}

impl CompletionCandidate {
    #[must_use]
    pub fn format_left(&self, style: MenuStyle, is_selected: bool) -> String {
        let spec = style.spec();
        let row_prefix = spec.row_prefixes[usize::from(is_selected)];
        let kind_prefix = spec.kind_prefixes[self.kind as usize];
        format!("{row_prefix}{kind_prefix}{}", self.display_label)
    }

    #[must_use]
    pub fn format_right(&self, style: MenuStyle) -> String {
        let (left, right) = style.spec().detail_wrap;
        format!("{left}{}{right}", self.detail)
    }
}

/// Indexed AST symbol definition (function, method, struct, class, trait, interface, enum, type).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AstSymbolEntry {
    pub name: String,
    /// Unambiguous `@` token inserted into prompt (`@Symbol` when unique, `@path:Symbol` when overloaded).
    pub insert_token: String,
    pub kind_badge: String,
    pub is_type: bool,
    pub rel_file: String,
    pub start_line: usize,
    pub end_line: usize,
    pub signature: String,
    pub centrality_bonus: i32,
}

/// Indexed project or screenshot image asset available for `@` multimodal mention.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageAssetEntry {
    pub rel_path: String,
    pub ext: String,
    pub media_type: String,
    pub dimensions: Option<(u32, u32)>,
    pub size_bytes: usize,
    pub is_raster_vision: bool,
}

impl ImageAssetEntry {
    #[must_use]
    pub fn detail_badge(&self) -> String {
        let kb = (self.size_bytes.max(1024) / 1024).max(1);
        match self.dimensions {
            Some((w, h)) => format!("{} · {}x{} · {}KB", self.ext, w, h, kb),
            None => format!("{} · {}KB", self.ext, kb),
        }
    }
}

/// Resolved `@` entities extracted from a submitted prompt for automatic vision & AST span grounding.
#[derive(Debug, Clone, Default)]
pub struct ResolvedPromptMentions {
    pub images: Vec<ImageAssetEntry>,
    pub symbols: Vec<AstSymbolEntry>,
}

/// Declarative sub-argument completion schema for slash commands (`/cmd <arg>`).
struct SlashArgSpec {
    command: &'static str,
    arg: &'static str,
    detail: &'static str,
}

const SLASH_ARG_TABLE: &[SlashArgSpec] = &[
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

/// Unified `@` Multimodal Entity Graph & Pure-Prompt Intellisense Engine.
#[derive(Debug, Clone, Default)]
pub struct IntellisenseEngine {
    indexed_root: Option<PathBuf>,
    pub files: Vec<(String, bool)>,
    pub ast_symbols: Vec<AstSymbolEntry>,
    pub images: Vec<ImageAssetEntry>,
    pub skills: Vec<(String, String)>,
    pub tasks: Vec<String>,
    pub mention_filter: MentionFilter,
    pub selected_index: usize,
    pub dismissed_for_query: Option<String>,
    /// True when the user explicitly focused the popup via `Alt+Down`/`Alt+Up` or `Ctrl+Space`.
    pub popup_explicitly_focused: bool,
}

impl IntellisenseEngine {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cycle_mention_filter(&mut self, forward: bool) {
        self.mention_filter = self.mention_filter.cycle(forward);
        self.selected_index = 0;
    }

    /// Forces a fresh workspace re-index on next `@` query (e.g. after new files/screenshots appear).
    pub fn invalidate_index(&mut self) {
        self.indexed_root = None;
    }

    /// Populates the unified workspace entity graph (`Files + AST Functions/Types + Images + Tasks + Skills`).
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
        self.ast_symbols.clear();
        self.images.clear();
        self.tasks.clear();

        self.index_workspace_tasks(root);

        // 1. Load cached per-file Tree-sitter AST shards (`.minicode/graph_shards.bin`) and inbound centrality
        let shard_by_rel: HashMap<String, Vec<SymbolDef>> = GraphStore::load_shards(root)
            .unwrap_or_default()
            .into_iter()
            .map(|(p, shard)| {
                let rel = p
                    .strip_prefix(root)
                    .unwrap_or(&p)
                    .to_string_lossy()
                    .replace('\\', "/");
                (rel, shard.all_symbols)
            })
            .collect();

        let mut inbound_degrees: HashMap<String, usize> = HashMap::new();
        if let Ok(Some(snap)) = GraphStore::load_snapshot(root) {
            for &(_, target_idx, _) in &snap.edges {
                if let Some(node) = snap.nodes.get(target_idx) {
                    *inbound_degrees.entry(node.name.clone()).or_insert(0) += 1;
                }
            }
        }

        let now_secs = current_unix_secs();
        let mut fallback_extractor: Option<RepoMapExtractor> = None;
        let mut fallback_parses = 0usize;
        let mut raw_symbols: Vec<AstSymbolEntry> = Vec::new();
        let mut seen_symbol_keys: HashSet<(String, String)> = HashSet::new();

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

            let meta = entry.metadata().ok();
            let recently_modified = meta
                .as_ref()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .is_some_and(|d| now_secs.saturating_sub(d.as_secs()) <= 3_600);

            if self.images.len() < INTELLISENSE_MAX_IMAGE_INDEX {
                if let Some(img_entry) =
                    inspect_workspace_image_asset(path, &rel_str, meta.as_ref())
                {
                    self.images.push(img_entry);
                }
            }

            if raw_symbols.len() < INTELLISENSE_MAX_SYMBOL_INDEX && is_ast_source_file(path) {
                let defs: Option<Vec<SymbolDef>> = if let Some(cached) = shard_by_rel.get(&rel_str)
                {
                    Some(cached.clone())
                } else if fallback_parses < 120 {
                    fallback_parses += 1;
                    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                    std::fs::read_to_string(path).ok().and_then(|code| {
                        let ex = fallback_extractor.get_or_insert_with(RepoMapExtractor::new);
                        ex.extract_symbols_from_str(ext, &code).ok()
                    })
                } else {
                    None
                };

                if let Some(sym_defs) = defs {
                    for sym in sym_defs {
                        if raw_symbols.len() >= INTELLISENSE_MAX_SYMBOL_INDEX {
                            break;
                        }
                        let Some((badge, is_type)) = classify_symbol_kind(&sym.kind) else {
                            continue;
                        };
                        let clean_name = sym.name.trim();
                        if clean_name.len() < 2
                            || !seen_symbol_keys.insert((rel_str.clone(), clean_name.to_string()))
                        {
                            continue;
                        }
                        let deg = inbound_degrees.get(clean_name).copied().unwrap_or(0);
                        let centrality_bonus = ((1.0 + deg as f64).ln() * 6.0) as i32
                            + if recently_modified { 8 } else { 0 };
                        raw_symbols.push(AstSymbolEntry {
                            name: clean_name.to_string(),
                            insert_token: String::new(),
                            kind_badge: badge.to_string(),
                            is_type,
                            rel_file: rel_str.clone(),
                            start_line: sym.line_number.max(1),
                            end_line: sym.end_line.max(sym.line_number.max(1)),
                            signature: sym.signature,
                            centrality_bonus,
                        });
                    }
                }
            }

            self.files.push((rel_str, recently_modified));
        }

        // Also index `.minicode/screenshots` so users can `@`-mention MiniBrowse QA screenshots directly
        let screenshots_dir = root.join(".minicode").join("screenshots");
        if screenshots_dir.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&screenshots_dir) {
                for entry in entries.flatten() {
                    if self.images.len() >= INTELLISENSE_MAX_IMAGE_INDEX {
                        break;
                    }
                    let path = entry.path();
                    if path.is_file() {
                        if let Ok(rel) = path.strip_prefix(root) {
                            let rel_str = rel.to_string_lossy().replace('\\', "/");
                            let meta = entry.metadata().ok();
                            if let Some(img) =
                                inspect_workspace_image_asset(&path, &rel_str, meta.as_ref())
                            {
                                self.images.push(img);
                            }
                        }
                    }
                }
            }
        }

        // Disambiguate symbol insert tokens (`@Name` when globally unique, `@file:Name` when overloaded)
        let mut name_counts: HashMap<String, usize> = HashMap::new();
        for sym in &raw_symbols {
            *name_counts.entry(sym.name.to_lowercase()).or_insert(0) += 1;
        }
        for mut sym in raw_symbols {
            let count = name_counts
                .get(&sym.name.to_lowercase())
                .copied()
                .unwrap_or(1);
            sym.insert_token = if count == 1 {
                format!("@{}", sym.name)
            } else {
                format!("@{}:{}", sym.rel_file, sym.name)
            };
            self.ast_symbols.push(sym);
        }

        self.files.sort_by(|(a, mod_a), (b, mod_b)| {
            mod_b
                .cmp(mod_a)
                .then_with(|| {
                    let depth_a = a.bytes().filter(|&ch| ch == b'/').count();
                    let depth_b = b.bytes().filter(|&ch| ch == b'/').count();
                    depth_a.cmp(&depth_b)
                })
                .then_with(|| a.cmp(b))
        });
    }

    fn index_workspace_tasks(&mut self, root: &Path) {
        let candidates = [
            root.join(".minicode/plan/todo.md"),
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

    /// Resolves `@...` mentions inside a submitted prompt into raster `ImageAssetEntry`s
    /// (for automatic `<attached_images>` vision grounding) and `AstSymbolEntry`s
    /// (for exact `<mentioned_symbols>` file & line-span grounding).
    pub fn resolve_prompt_mentions(
        &mut self,
        prompt_text: &str,
        workspace_root: Option<&Path>,
    ) -> ResolvedPromptMentions {
        if !prompt_text.contains('@') {
            return ResolvedPromptMentions::default();
        }
        self.ensure_indexed(workspace_root);
        self.resolve_prompt_mentions_readonly(prompt_text, workspace_root)
    }

    /// Read-only resolution of `@...` mentions against the current index and filesystem.
    #[must_use]
    pub fn resolve_prompt_mentions_readonly(
        &self,
        prompt_text: &str,
        workspace_root: Option<&Path>,
    ) -> ResolvedPromptMentions {
        let mut out = ResolvedPromptMentions::default();
        if !prompt_text.contains('@') {
            return out;
        }

        let mut seen_imgs = HashSet::new();
        let mut seen_syms = HashSet::new();
        let mut in_fence = false;

        for line in prompt_text.lines() {
            if line.trim_start().starts_with("```") {
                in_fence = !in_fence;
                continue;
            }
            if in_fence {
                continue;
            }
            for raw_tok in line.split_whitespace() {
                let clean = raw_tok.trim_matches(|c: char| {
                    matches!(
                        c,
                        ',' | ';' | '.' | '(' | ')' | '[' | ']' | '"' | '\'' | '`'
                    )
                });
                let Some(mention) = clean.strip_prefix('@') else {
                    continue;
                };
                if mention.is_empty() {
                    continue;
                }

                // 1. Check indexed raster images (or direct workspace image path)
                if let Some(img) = self
                    .images
                    .iter()
                    .find(|i| i.is_raster_vision && i.rel_path.eq_ignore_ascii_case(mention))
                {
                    if seen_imgs.insert(img.rel_path.to_lowercase()) {
                        out.images.push(img.clone());
                    }
                    continue;
                } else if let Some(root) = workspace_root {
                    let candidate = root.join(mention);
                    if candidate.is_file() {
                        if let Some(img) = inspect_workspace_image_asset(
                            &candidate,
                            mention,
                            std::fs::metadata(&candidate).ok().as_ref(),
                        ) {
                            if img.is_raster_vision && seen_imgs.insert(img.rel_path.to_lowercase())
                            {
                                out.images.push(img);
                                continue;
                            }
                        }
                    }
                }

                // 2. Check AST symbols (`@SymbolName` or `@file/path.rs:SymbolName`)
                let full_at = format!("@{mention}");
                if let Some(sym) = self
                    .ast_symbols
                    .iter()
                    .find(|s| s.insert_token.eq_ignore_ascii_case(&full_at))
                    .or_else(|| {
                        self.ast_symbols
                            .iter()
                            .find(|s| s.name.eq_ignore_ascii_case(mention))
                    })
                {
                    let key = format!("{}:{}", sym.rel_file, sym.name).to_lowercase();
                    if seen_syms.insert(key) {
                        out.symbols.push(sym.clone());
                    }
                }
            }
        }

        out
    }

    /// Detects if the cursor is currently positioned inside an explicit sigil trigger
    /// (`@<entity>`, `#<symbol>`, `$<skill>`, `/<cmd> <arg>`) OR an ambient conversational
    /// prompt continuation (`TriggerKind::PromptFlow`).
    #[must_use]
    pub fn extract_trigger(
        line: &str,
        cursor_col: usize,
        cursor_row: usize,
    ) -> Option<TriggerToken> {
        let chars: Vec<char> = line.chars().collect();
        let col = cursor_col.min(chars.len());
        let prefix: String = chars[..col].iter().collect();

        // 1. `/command <arg>` on row 0
        if cursor_row == 0 {
            let trimmed_start = prefix.trim_start();
            if trimmed_start.starts_with('/') {
                if let Some(space_char_idx) = chars[..col].iter().position(|&c| c == ' ') {
                    let cmd_part: String = chars[..space_char_idx].iter().collect();
                    let cmd_clean = cmd_part.trim().to_ascii_lowercase();
                    let arg_part: String = chars[space_char_idx + 1..col].iter().collect();
                    if !arg_part.contains(' ')
                        && (SLASH_ARG_TABLE.iter().any(|s| s.command == cmd_clean)
                            || matches!(cmd_clean.as_str(), "/menu" | "/menu-style"))
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
                        kind: TriggerKind::AtMention,
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

        // 3. Ambient Conversational Prompt Flow (`Omnibox + Gboard Kneser-Ney`, zero raw code noise)
        let query: String = chars[token_start..col].iter().collect();
        let query_char_len = query.chars().count();
        let has_preceding_context = !chars[..token_start].iter().all(|c| c.is_whitespace());

        if query_char_len >= INTELLISENSE_MIN_WORD_CHARS
            || (query_char_len == 0 && col == chars.len() && has_preceding_context)
        {
            return Some(TriggerToken {
                kind: TriggerKind::PromptFlow {
                    line_prefix: prefix,
                },
                query,
                replace_start_col: token_start,
                replace_end_col: col,
            });
        }

        None
    }

    pub fn query_candidates(
        &mut self,
        trigger: &TriggerToken,
        workspace_root: Option<&Path>,
    ) -> Vec<CompletionCandidate> {
        self.query_candidates_with_history(trigger, workspace_root, None)
    }

    pub fn query_candidates_with_history(
        &mut self,
        trigger: &TriggerToken,
        workspace_root: Option<&Path>,
        history: Option<&PromptHistory>,
    ) -> Vec<CompletionCandidate> {
        let sig = format!(
            "{:?}:{}:{:?}",
            trigger.kind, trigger.query, self.mention_filter
        );
        if self.dismissed_for_query.as_deref() == Some(sig.as_str()) {
            return Vec::new();
        }

        self.ensure_indexed(workspace_root);

        let mut ranked: Vec<CompletionCandidate> = match &trigger.kind {
            TriggerKind::SlashArg { command } => SLASH_ARG_TABLE
                .iter()
                .filter(|spec| spec.command == command)
                .map(|spec| (spec.arg, spec.detail))
                .chain(
                    matches!(command.as_str(), "/menu" | "/menu-style")
                        .then_some(MENU_STYLE_SPECS.iter())
                        .into_iter()
                        .flatten()
                        .map(|spec| (spec.id, spec.description)),
                )
                .filter_map(|(arg, detail)| {
                    let score = fuzzy_score(arg, &trigger.query)?;
                    Some(CompletionCandidate {
                        insert_text: arg.to_string(),
                        display_label: format!("{command} {arg}"),
                        detail: detail.to_string(),
                        kind: CandidateKind::Arg,
                        score,
                    })
                })
                .collect(),
            TriggerKind::AtMention => self.rank_at_mention_candidates(&trigger.query),
            TriggerKind::SymbolMention => self
                .ast_symbols
                .iter()
                .filter_map(|sym| {
                    let score = fuzzy_score(&sym.name, &trigger.query)?;
                    Some(CompletionCandidate {
                        insert_text: format!("#{} ", sym.name),
                        display_label: format!("#{}", sym.name),
                        detail: format!(
                            "{} · {}:L{}",
                            sym.kind_badge, sym.rel_file, sym.start_line
                        ),
                        kind: CandidateKind::Symbol,
                        score: score + sym.centrality_bonus,
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
                        kind: CandidateKind::Skill,
                        score,
                    })
                })
                .collect(),
            TriggerKind::PromptFlow { line_prefix } => {
                self.rank_prompt_flow_candidates(trigger, line_prefix, history)
            }
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

    fn rank_at_mention_candidates(&self, raw_query: &str) -> Vec<CompletionCandidate> {
        let (prefix_override, q) = parse_at_mention_query(raw_query);
        let active_filter = prefix_override.unwrap_or(self.mention_filter);
        let mut out = Vec::new();
        let image_paths: HashSet<&str> = self.images.iter().map(|i| i.rel_path.as_str()).collect();

        // 1. Project & Screenshot Images (`@assets/hero.png`, `@.minicode/screenshots/...`)
        if matches!(active_filter, MentionFilter::All | MentionFilter::Images) {
            for img in &self.images {
                if let Some(mut score) = fuzzy_score(&img.rel_path, q) {
                    let basename = &img.rel_path[img.rel_path.rfind('/').map_or(0, |i| i + 1)..];
                    if starts_with_ignore_ascii_case(basename, q) && !q.is_empty() {
                        score += INTELLISENSE_EXACT_TOKEN_PREFIX_BONUS / 2;
                    }
                    out.push(CompletionCandidate {
                        insert_text: format!("@{} ", img.rel_path),
                        display_label: format!("@{}", img.rel_path),
                        detail: img.detail_badge(),
                        kind: CandidateKind::Image,
                        score,
                    });
                }
            }
        }

        // 2. Workspace Files (excluding image paths already surfaced as `CandidateKind::Image`)
        if matches!(active_filter, MentionFilter::All | MentionFilter::Files) {
            for (path, recently_mod) in &self.files {
                if image_paths.contains(path.as_str()) {
                    continue;
                }
                if let Some(mut score) = fuzzy_score(path, q) {
                    if *recently_mod {
                        score += 10;
                    }
                    let ext = Path::new(path)
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("file");
                    let detail = if *recently_mod {
                        format!("{ext} · recent")
                    } else {
                        format!("{ext} · file")
                    };
                    out.push(CompletionCandidate {
                        insert_text: format!("@{path} "),
                        display_label: format!("@{path}"),
                        detail,
                        kind: CandidateKind::File,
                        score,
                    });
                }
            }
        }

        // 3. Tree-sitter AST Functions / Methods (`MentionFilter::Functions` | `All`)
        // 4. Tree-sitter AST Classes / Structs / Traits / Interfaces / Enums (`MentionFilter::Types` | `All`)
        let include_fns = matches!(active_filter, MentionFilter::All | MentionFilter::Functions);
        let include_types = matches!(active_filter, MentionFilter::All | MentionFilter::Types);
        if include_fns || include_types {
            for sym in &self.ast_symbols {
                if sym.is_type && !include_types {
                    continue;
                }
                if !sym.is_type && !include_fns {
                    continue;
                }
                // Score against both symbol name and `file:symbol` qualified token
                let best_match = fuzzy_score(&sym.name, q)
                    .into_iter()
                    .chain(fuzzy_score(sym.insert_token.trim_start_matches('@'), q))
                    .max();
                if let Some(mut score) = best_match {
                    if !q.is_empty() && starts_with_ignore_ascii_case(&sym.name, q) {
                        score += INTELLISENSE_EXACT_TOKEN_PREFIX_BONUS / 2;
                    }
                    score += sym.centrality_bonus;
                    out.push(CompletionCandidate {
                        insert_text: format!("{} ", sym.insert_token),
                        display_label: sym.insert_token.clone(),
                        detail: format!(
                            "{} · {}:L{}",
                            sym.kind_badge, sym.rel_file, sym.start_line
                        ),
                        kind: CandidateKind::Symbol,
                        score,
                    });
                }
            }
        }

        out
    }

    fn rank_prompt_flow_candidates(
        &self,
        trigger: &TriggerToken,
        line_prefix: &str,
        history: Option<&PromptHistory>,
    ) -> Vec<CompletionCandidate> {
        let mut out = Vec::new();
        let mut seen_labels = HashSet::new();
        let default_corpus;
        let corpus = match history {
            Some(h) => &h.corpus,
            None => {
                default_corpus = PromptCorpus::new_in_memory();
                &default_corpus
            }
        };

        for sug in corpus.suggest_completions(
            line_prefix,
            trigger.replace_start_col,
            &trigger.query,
            INTELLISENSE_MAX_POPUP_ITEMS,
        ) {
            if seen_labels.insert(sug.display_label.to_lowercase()) {
                let kind = if sug.source_badge == "phrase" {
                    CandidateKind::Prompt
                } else {
                    CandidateKind::History
                };
                out.push(CompletionCandidate {
                    insert_text: sug.insert_text,
                    display_label: sug.display_label,
                    detail: sug.source_badge.to_string(),
                    kind,
                    score: sug.score,
                });
            }
        }

        // Surface active `todo.md` step descriptions when query matches an open task
        if !trigger.query.is_empty() && trigger.query.len() >= 3 {
            for task in self.tasks.iter().take(6) {
                if let Some(score) = fuzzy_score(task, &trigger.query) {
                    if score >= 32 && seen_labels.insert(task.to_lowercase()) {
                        out.push(CompletionCandidate {
                            insert_text: task.clone(),
                            display_label: task.clone(),
                            detail: "task".to_string(),
                            kind: CandidateKind::Task,
                            score: 70 + score / 2,
                        });
                    }
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
            return None;
        }

        // 2. Explicit sigil trigger (`@entity`, `#symbol`, `$skill`, `/cmd <arg>`) takes priority at cursor
        if let Some(trigger) = Self::extract_trigger(line, cursor_col, cursor_row) {
            if trigger.kind.is_explicit() {
                let candidates =
                    self.query_candidates_with_history(&trigger, workspace_root, Some(history));
                let idx = self.selected_index.min(candidates.len().saturating_sub(1));
                if let Some(best) = candidates.get(idx).or_else(|| candidates.first()) {
                    let raw_target = match trigger.kind {
                        TriggerKind::AtMention
                        | TriggerKind::SymbolMention
                        | TriggerKind::SkillMention => best
                            .insert_text
                            .trim_end()
                            .trim_start_matches(['@', '#', '$']),
                        TriggerKind::SlashArg { .. } | TriggerKind::PromptFlow { .. } => {
                            best.insert_text.trim_end()
                        }
                    };
                    let (_, stripped_q) = if matches!(trigger.kind, TriggerKind::AtMention) {
                        parse_at_mention_query(&trigger.query)
                    } else {
                        (None, trigger.query.as_str())
                    };
                    if stripped_q.is_empty() && trigger.query.is_empty() && !raw_target.is_empty() {
                        return Some(raw_target.to_string());
                    }
                    if raw_target.len() > trigger.query.len()
                        && starts_with_ignore_ascii_case(raw_target, &trigger.query)
                    {
                        return Some(raw_target[trigger.query.len()..].to_string());
                    }
                }
                return None;
            }
        }

        // 3. Omnibox Frecency + Order-4 Kneser-Ney Gboard continuous phrase suffix
        history.suggest_ghost_suffix(line)
    }
}

/// Extracts the first word segment from `ghost` (including leading spaces or trailing path separator)
/// for Fish/Gboard-style single-word acceptance (`Ctrl+Right` / `Alt+Right` / `Alt+F`).
#[must_use]
pub fn extract_first_ghost_word(ghost: &str) -> &str {
    if ghost.is_empty() {
        return "";
    }
    let bytes = ghost.as_bytes();
    let mut i = 0;
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
        if bytes[i] == b'/' || bytes[i] == b'\\' {
            i += 1;
            break;
        }
        i += 1;
    }
    while i < ghost.len() && !ghost.is_char_boundary(i) {
        i += 1;
    }
    &ghost[..i]
}

fn is_ast_source_file(path: &Path) -> bool {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    matches!(ext, "rs" | "ts" | "tsx" | "js" | "jsx" | "py")
}

fn classify_symbol_kind(raw_kind: &str) -> Option<(&'static str, bool)> {
    match raw_kind {
        "function" => Some(("fn", false)),
        "method" => Some(("method", false)),
        "struct" => Some(("struct", true)),
        "class" => Some(("class", true)),
        "trait" => Some(("trait", true)),
        "interface" => Some(("interface", true)),
        "enum" => Some(("enum", true)),
        "type_alias" => Some(("type", true)),
        _ => None,
    }
}

fn inspect_workspace_image_asset(
    path: &Path,
    rel_str: &str,
    meta: Option<&std::fs::Metadata>,
) -> Option<ImageAssetEntry> {
    let ext_lower = path
        .extension()
        .and_then(|e| e.to_str())?
        .to_ascii_lowercase();
    if !matches!(
        ext_lower.as_str(),
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg"
    ) {
        return None;
    }
    let size_bytes = meta
        .map(|m| m.len() as usize)
        .or_else(|| std::fs::metadata(path).ok().map(|m| m.len() as usize))
        .unwrap_or(0);

    if ext_lower == "svg" {
        return Some(ImageAssetEntry {
            rel_path: rel_str.to_string(),
            ext: "svg".to_string(),
            media_type: "image/svg+xml".to_string(),
            dimensions: None,
            size_bytes,
            is_raster_vision: false,
        });
    }

    let mut header = Vec::with_capacity(1024);
    if let Ok(file) = std::fs::File::open(path) {
        let _ = file.take(32_768).read_to_end(&mut header);
    }
    if let Some((media_type, canon_ext, dims)) = crate::ui::clipboard::inspect_image_bytes(&header)
    {
        return Some(ImageAssetEntry {
            rel_path: rel_str.to_string(),
            ext: canon_ext.to_string(),
            media_type: media_type.to_string(),
            dimensions: dims,
            size_bytes,
            is_raster_vision: true,
        });
    }

    let media_type = match ext_lower.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        _ => return None,
    };
    Some(ImageAssetEntry {
        rel_path: rel_str.to_string(),
        ext: ext_lower,
        media_type: media_type.to_string(),
        dimensions: None,
        size_bytes,
        is_raster_vision: true,
    })
}

fn current_unix_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(1, |d| d.as_secs())
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
    fn test_prompt_history_up_up_down_boundary_and_slash_command_exclusion() {
        let mut hist = PromptHistory::new();
        hist.seed_prompt("hii");
        hist.seed_prompt("/settings");
        hist.seed_prompt("so whats");
        hist.record("/model", None);
        hist.record("/caveman ultra", None);

        // Slash commands are completely excluded from prompt history!
        assert_eq!(hist.entries.len(), 2);
        assert_eq!(hist.navigate_up("").as_deref(), Some("so whats"));
        assert_eq!(hist.navigate_up("so whats").as_deref(), Some("hii"));
        assert_eq!(hist.navigate_up("hii").as_deref(), Some("hii"));
        assert_eq!(hist.navigate_down().as_deref(), Some("so whats"));
        assert_eq!(hist.navigate_down().as_deref(), Some(""));

        let mut hist2 = PromptHistory::new();
        hist2.seed_prompt("hii");
        assert_eq!(hist2.navigate_up("so whats").as_deref(), Some("hii"));
        assert_eq!(hist2.navigate_up("hii").as_deref(), Some("hii"));
        assert_eq!(hist2.navigate_down().as_deref(), Some("so whats"));

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
        assert_eq!(t_file.kind, TriggerKind::AtMention);
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
    fn test_pure_prompt_gboard_continuity_and_unified_at_mentions_without_code_pollution() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("src/ui")).unwrap();
        std::fs::create_dir_all(dir.path().join("assets")).unwrap();
        std::fs::write(
            dir.path().join("src/ui/input.rs"),
            "pub struct InputDock {}\npub fn handle_key_event() {}\n",
        )
        .unwrap();
        let png_bytes: [u8; 24] = [
            0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, b'I', b'H',
            b'D', b'R', 0x00, 0x00, 0x05, 0x00, 0x00, 0x00, 0x02, 0xD0,
        ];
        std::fs::write(dir.path().join("assets/mockup.png"), png_bytes).unwrap();

        let mut engine = IntellisenseEngine::new();
        let hist = PromptHistory::new();

        // 1. Conversational prompt prediction works cleanly: "so wh" -> "ats next"
        let ghost = engine.compute_ghost_hint("so wh", 5, 0, &hist, Some(dir.path()), None);
        assert_eq!(ghost.as_deref(), Some("ats next"));

        // 2. Next-word continuity after space: "so whats " -> "next"
        let ghost_space =
            engine.compute_ghost_hint("so whats ", 9, 0, &hist, Some(dir.path()), None);
        assert_eq!(ghost_space.as_deref(), Some("next"));

        // 3. Bare prose typing WITHOUT `@` NEVER injects raw code symbols (`InputDock`) or Rust keywords (`pub struct`)
        let trig_prose = IntellisenseEngine::extract_trigger("refactor InputD", 15, 0).unwrap();
        let prose_cands =
            engine.query_candidates_with_history(&trig_prose, Some(dir.path()), Some(&hist));
        assert!(
            !prose_cands
                .iter()
                .any(|c| c.display_label.contains("InputDock")),
            "Bare prose typing must never pollute prompt flow with unprompted AST code symbols"
        );

        // 4. Unified `@` mention discovers files, AST structs/classes, AST functions, AND project images!
        let trig_at_sym = IntellisenseEngine::extract_trigger("refactor @InputD", 16, 0).unwrap();
        let sym_cands =
            engine.query_candidates_with_history(&trig_at_sym, Some(dir.path()), Some(&hist));
        assert!(sym_cands.iter().any(|c| c.insert_text == "@InputDock "));

        let trig_at_fn =
            IntellisenseEngine::extract_trigger("audit @fn:handle_key", 20, 0).unwrap();
        let fn_cands =
            engine.query_candidates_with_history(&trig_at_fn, Some(dir.path()), Some(&hist));
        assert!(fn_cands
            .iter()
            .any(|c| c.insert_text == "@handle_key_event "));

        let trig_at_img = IntellisenseEngine::extract_trigger("match @mockup", 13, 0).unwrap();
        let img_cands =
            engine.query_candidates_with_history(&trig_at_img, Some(dir.path()), Some(&hist));
        assert!(img_cands
            .iter()
            .any(|c| c.insert_text == "@assets/mockup.png " && c.kind == CandidateKind::Image));
    }

    #[test]
    fn test_extract_first_ghost_word() {
        assert_eq!(extract_first_ghost_word(" the login button"), " the");
        assert_eq!(extract_first_ghost_word("ui/input.rs "), "ui/");
    }

    #[test]
    fn test_menu_style_lookup_and_formatting_table() {
        assert_eq!(MenuStyle::from_str_loose("minimal"), MenuStyle::Minimal);
        assert_eq!(MenuStyle::from_str_loose("ascii"), MenuStyle::Ascii);
        assert_eq!(MenuStyle::from_str_loose("icons"), MenuStyle::Icons);
        assert_eq!(MenuStyle::from_str_loose("emoji"), MenuStyle::Emoji);
        assert_eq!(MenuStyle::from_str_loose("kaomoji"), MenuStyle::Kaomoji);
        assert_eq!(MenuStyle::from_str_loose("kamoji"), MenuStyle::Kaomoji);
        assert_eq!(MenuStyle::from_str_loose("compact"), MenuStyle::Compact);
        assert_eq!(MenuStyle::from_str_loose("symbols"), MenuStyle::Symbols);
        assert_eq!(MenuStyle::from_str_loose("nerdfont"), MenuStyle::NerdFont);
        assert_eq!(MenuStyle::from_str_loose("nerd"), MenuStyle::NerdFont);
        assert_eq!(MenuStyle::from_str_loose("posh_card"), MenuStyle::PoshCard);

        let sample = CompletionCandidate {
            insert_text: "whats next".to_string(),
            display_label: "so whats next".to_string(),
            detail: "prompt".to_string(),
            kind: CandidateKind::Prompt,
            score: 100,
        };

        assert_eq!(
            sample.format_left(MenuStyle::Minimal, true),
            "  so whats next"
        );
        assert_eq!(sample.format_right(MenuStyle::Minimal), "prompt");
        assert_eq!(
            sample.format_left(MenuStyle::Ascii, true),
            " > [PRMPT] so whats next"
        );
        assert_eq!(sample.format_right(MenuStyle::Ascii), "(prompt)");
        assert_eq!(
            sample.format_left(MenuStyle::Icons, true),
            " ❯ § so whats next"
        );
        assert_eq!(
            sample.format_left(MenuStyle::Emoji, true),
            " 👉 ⚡ so whats next"
        );
        assert_eq!(
            sample.format_left(MenuStyle::Kaomoji, true),
            " (☞ﾟヮﾟ)☞ so whats next"
        );
        assert_eq!(sample.format_right(MenuStyle::Kaomoji), "[prompt]");
        assert_eq!(
            sample.format_left(MenuStyle::Compact, true),
            "▎ so whats next"
        );
        assert_eq!(sample.format_right(MenuStyle::Compact), "· prompt");

        let trig_menu = IntellisenseEngine::extract_trigger("/menu ka", 8, 0).unwrap();
        let mut engine = IntellisenseEngine::new();
        let menu_cands = engine.query_candidates(&trig_menu, None);
        assert!(!menu_cands.is_empty());
        assert_eq!(menu_cands[0].insert_text, "kaomoji");
    }

    #[test]
    fn test_prompt_history_excludes_system_wakeups_and_collapsed_paste_chips() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path();

        // Pre-seed legacy workspace prompt_history.jsonl with poisoned entries from minitest72
        let legacy_path = ws.join(PROMPT_HISTORY_FILE);
        std::fs::create_dir_all(legacy_path.parent().unwrap()).unwrap();
        std::fs::write(
            &legacy_path,
            concat!(
                "{\"text\":\"[Create a premium, modern… ..... +84 lines]\",\"count\":2,\"timestamp\":100}\n",
                "{\"text\":\"[SYSTEM - ASYNC MINIPUB BOOMERANG COMPLETION WAKEUP (reviewer-2)]\\nDone\",\"count\":1,\"timestamp\":101}\n",
                "{\"text\":\"continue\",\"count\":3,\"timestamp\":102}\n",
            ),
        )
        .unwrap();

        let mut hist = PromptHistory::new();
        hist.load_from_workspace(ws);
        assert_eq!(hist.entries.len(), 1);
        assert_eq!(hist.entries[0].text, "continue");

        // Attempting to seed or record internal wakeup prompts or paste chips must be ignored
        hist.seed_prompt(
            "[SYSTEM - ASYNC MINIPUB BOOMERANG COMPLETION WAKEUP (reviewer-2)]\nReport",
        );
        hist.seed_prompt("[SYSTEM - ASYNC MINITASK COMPLETION WAKEUP (build)]\nDone");
        hist.record("[Create a premium, modern… ..... +84 lines]", Some(ws));
        hist.record("[img 1]", Some(ws));
        hist.record("build the landing page hero section", Some(ws));

        let texts: Vec<&str> = hist.entries.iter().map(|e| e.text.as_str()).collect();
        assert_eq!(
            texts,
            vec!["build the landing page hero section", "continue"]
        );

        let persisted = std::fs::read_to_string(&legacy_path).unwrap();
        assert!(!persisted.contains("[SYSTEM"));
        assert!(!persisted.contains("+84 lines"));
        assert!(persisted.contains("build the landing page hero section"));
    }
}

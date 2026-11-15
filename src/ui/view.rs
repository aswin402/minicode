use crate::ui::sum_tree::{Dimension, SumTree, Summarizable, Summary};
use crate::ui::theme::Theme;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;
use std::cell::RefCell;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use unicode_width::UnicodeWidthStr;

#[path = "view_impl/entry_renderers.rs"]
mod entry_renderers;
#[path = "view_impl/subagents_and_selection.rs"]
mod subagents_and_selection;
#[path = "view_impl/wrapping_and_format.rs"]
mod wrapping_and_format;

#[cfg(test)]
#[path = "view_impl/tests.rs"]
mod tests;

/// Execution status of a subagent tool action item
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub enum SubagentItemStatus {
    Running,
    Success,
    Failed(String),
}

/// An individual tool execution branch in a subagent tree
#[derive(Debug, Clone)]
pub struct SubagentTreeItem {
    pub name: String,
    pub detail: String,
    pub status: SubagentItemStatus,
}

/// An inline Crush/OpenCode style subagent tree block
#[derive(Debug, Clone)]
pub struct SubagentTreeBlock {
    pub id: String,
    pub role_name: String,
    pub task_prompt: String,
    pub items: Vec<SubagentTreeItem>,
    pub is_running: bool,
    pub is_success: bool,
    pub outcome: Option<String>,
    pub error_message: Option<String>,
    pub tokens_used: usize,
    pub duration_ms: Option<u64>,
}

/// An adaptive matrix card for large multi-agent swarms (4+ parallel workers)
#[derive(Debug, Clone)]
pub struct SwarmMatrixBlock {
    pub title: String,
    pub workers: Vec<SubagentTreeBlock>,
    pub is_expanded: bool,
    pub total_tokens: usize,
    pub is_running: bool,
    pub duration_ms: Option<u64>,
}

/// Execution status of a discrete task in a live plan block
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LivePlanTaskStatus {
    Completed,
    InProgress,
    Pending,
}

/// A parsed task item in a visual Live Plan Card
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LivePlanTaskItem {
    pub title: String,
    pub status: LivePlanTaskStatus,
    pub sub_tasks: Vec<LivePlanTaskItem>,
}

/// An inline Oh-My-Pi / Pi style Live Plan & Execution Progress card
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LivePlanBlock {
    pub title: String,
    pub total_tasks: usize,
    pub completed_tasks: usize,
    pub active_task: Option<String>,
    pub tasks: Vec<LivePlanTaskItem>,
}

#[derive(Debug, Clone)]
pub enum TimelineEntry {
    UserPrompt(String),
    ThoughtBlock {
        text: String,
        started_at: Option<std::time::Instant>,
        duration_secs: Option<f64>,
    },
    AssistantMarkdown(String),
    ToolStart {
        name: String,
        command_or_path: String,
    },
    #[allow(dead_code)]
    ToolApproved {
        name: String,
        command_or_path: String,
    },
    ToolFinished {
        #[allow(dead_code)]
        name: String,
        command_or_path: String,
        success: bool,
        output: String,
        #[allow(dead_code)]
        duration_ms: Option<u64>,
    },
    SubagentTree(SubagentTreeBlock),
    SubagentSwarm(SwarmMatrixBlock),
    #[allow(dead_code)]
    LivePlan(LivePlanBlock),
    ContextCompaction {
        tier: usize,
        turns_summarized: usize,
        tokens_before: usize,
        tokens_after: usize,
        savings_percent: usize,
    },
    GitCommit {
        hash: String,
        message: String,
        files: Vec<String>,
    },
    SystemStatus(String),
    TurnSeparator,
}

/// Sentinel marker inserted into pre-compacted `ToolFinished` diff blocks so UI RAM never retains unbounded diffs.
pub(crate) const FOLDED_DIFF_SENTINEL: &str = "\x00MINICODE_FOLDED_DIFF:";
/// Sentinel marker inserted into pre-compacted `ToolFinished` outputs so UI RAM never retains multi-MB raw dumps.
pub(crate) const FOLDED_OUT_SENTINEL: &str = "\x00MINICODE_FOLDED_OUT:";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct CellSummary {
    pub line_count: usize,
    pub prompt_count: usize,
    pub has_wide_emoji: bool,
}

impl Summary for CellSummary {
    #[inline]
    fn combine(left: &Self, right: &Self) -> Self {
        Self {
            line_count: left.line_count + right.line_count,
            prompt_count: left.prompt_count + right.prompt_count,
            has_wide_emoji: left.has_wide_emoji || right.has_wide_emoji,
        }
    }
}

pub(crate) struct CellLineDim;

impl Dimension<CellSummary> for CellLineDim {
    #[inline]
    fn measure(summary: &CellSummary) -> usize {
        summary.line_count
    }
}

/// Immutable compiled representation of a single `TimelineEntry` inside the virtualized `SumTree`.
#[derive(Debug, Clone)]
pub(crate) struct RenderedCell {
    pub lines: Arc<[Line<'static>]>,
    pub prompt_anchor: Option<Arc<str>>,
    pub has_wide_emoji: bool,
}

impl Summarizable for RenderedCell {
    type Summary = CellSummary;

    #[inline]
    fn summary(&self) -> Self::Summary {
        CellSummary {
            line_count: self.lines.len(),
            prompt_count: usize::from(self.prompt_anchor.is_some()),
            has_wide_emoji: self.has_wide_emoji,
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct TimelineRenderCache {
    pub layout_key: u64,
    pub header_key: u64,
    pub header_lines: Vec<Line<'static>>,
    pub header_has_emoji: bool,
    pub entry_keys: Vec<u64>,
    pub cells: SumTree<RenderedCell>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DiagnosticTone {
    Destructive,
    DestructiveBold,
    Success,
    SuccessBold,
    Warning,
    Info,
    Muted,
}

impl DiagnosticTone {
    #[inline]
    fn style(self, theme: &Theme) -> Style {
        match self {
            Self::Destructive => Style::default().fg(theme.destructive),
            Self::DestructiveBold => Style::default()
                .fg(theme.destructive)
                .add_modifier(Modifier::BOLD),
            Self::Success => Style::default().fg(theme.success),
            Self::SuccessBold => Style::default()
                .fg(theme.success)
                .add_modifier(Modifier::BOLD),
            Self::Warning => Style::default().fg(theme.warning),
            Self::Info => Style::default().fg(theme.info),
            Self::Muted => Style::default().fg(theme.muted),
        }
    }
}

struct DiagnosticPrefixRule {
    prefix: &'static str,
    match_trimmed: bool,
    exclude_prefix: Option<&'static str>,
    tone: DiagnosticTone,
}

const DIAGNOSTIC_PREFIX_RULES: &[DiagnosticPrefixRule] = &[
    DiagnosticPrefixRule {
        prefix: "left:",
        match_trimmed: true,
        exclude_prefix: None,
        tone: DiagnosticTone::Destructive,
    },
    DiagnosticPrefixRule {
        prefix: "-",
        match_trimmed: false,
        exclude_prefix: Some("---"),
        tone: DiagnosticTone::Destructive,
    },
    DiagnosticPrefixRule {
        prefix: "right:",
        match_trimmed: true,
        exclude_prefix: None,
        tone: DiagnosticTone::Success,
    },
    DiagnosticPrefixRule {
        prefix: "+",
        match_trimmed: false,
        exclude_prefix: Some("+++"),
        tone: DiagnosticTone::Success,
    },
    DiagnosticPrefixRule {
        prefix: "assertion ",
        match_trimmed: true,
        exclude_prefix: None,
        tone: DiagnosticTone::DestructiveBold,
    },
    DiagnosticPrefixRule {
        prefix: "error[",
        match_trimmed: true,
        exclude_prefix: None,
        tone: DiagnosticTone::DestructiveBold,
    },
    DiagnosticPrefixRule {
        prefix: "error:",
        match_trimmed: true,
        exclude_prefix: None,
        tone: DiagnosticTone::DestructiveBold,
    },
    DiagnosticPrefixRule {
        prefix: "✗",
        match_trimmed: true,
        exclude_prefix: None,
        tone: DiagnosticTone::DestructiveBold,
    },
    DiagnosticPrefixRule {
        prefix: "warning:",
        match_trimmed: true,
        exclude_prefix: None,
        tone: DiagnosticTone::Warning,
    },
    DiagnosticPrefixRule {
        prefix: "⚠",
        match_trimmed: true,
        exclude_prefix: None,
        tone: DiagnosticTone::Warning,
    },
    DiagnosticPrefixRule {
        prefix: "✔",
        match_trimmed: true,
        exclude_prefix: None,
        tone: DiagnosticTone::SuccessBold,
    },
    DiagnosticPrefixRule {
        prefix: "✓",
        match_trimmed: true,
        exclude_prefix: None,
        tone: DiagnosticTone::SuccessBold,
    },
    DiagnosticPrefixRule {
        prefix: "libs compiled",
        match_trimmed: true,
        exclude_prefix: None,
        tone: DiagnosticTone::Info,
    },
    DiagnosticPrefixRule {
        prefix: "disk used",
        match_trimmed: true,
        exclude_prefix: None,
        tone: DiagnosticTone::Info,
    },
    DiagnosticPrefixRule {
        prefix: "binary size",
        match_trimmed: true,
        exclude_prefix: None,
        tone: DiagnosticTone::Info,
    },
    DiagnosticPrefixRule {
        prefix: "##",
        match_trimmed: false,
        exclude_prefix: None,
        tone: DiagnosticTone::Info,
    },
    DiagnosticPrefixRule {
        prefix: "test result:",
        match_trimmed: true,
        exclude_prefix: None,
        tone: DiagnosticTone::Info,
    },
];

#[inline]
fn classify_diagnostic_line(
    out_line: &str,
    trimmed_line: &str,
    is_panic_or_after: bool,
    success: bool,
) -> DiagnosticTone {
    // Check diff/assertion polarity rules first (`left:`/`-` and `right:`/`+` precede panic fallback)
    for rule in &DIAGNOSTIC_PREFIX_RULES[..4] {
        let target = if rule.match_trimmed {
            trimmed_line
        } else {
            out_line
        };
        if target.starts_with(rule.prefix)
            && rule.exclude_prefix.is_none_or(|ex| !target.starts_with(ex))
        {
            return rule.tone;
        }
    }
    if is_panic_or_after {
        return DiagnosticTone::DestructiveBold;
    }
    for rule in &DIAGNOSTIC_PREFIX_RULES[4..] {
        let target = if rule.match_trimmed {
            trimmed_line
        } else {
            out_line
        };
        if target.starts_with(rule.prefix)
            && rule.exclude_prefix.is_none_or(|ex| !target.starts_with(ex))
        {
            return rule.tone;
        }
    }
    if success {
        DiagnosticTone::Muted
    } else {
        DiagnosticTone::Destructive
    }
}

pub struct TimelineView {
    pub entries: Vec<TimelineEntry>,
    pub scroll_offset: std::cell::Cell<u16>,
    pub auto_scroll: std::cell::Cell<bool>,
    pub max_scroll: std::cell::Cell<u16>,
    pub selection: crate::ui::selection::TimelineSelection,
    pub in_thought_mode: bool,
    pub thought_start: Option<std::time::Instant>,
    pub turn_start: Option<std::time::Instant>,
    pub thought_tag_buffer: String,
    pub(crate) estimated_chars: usize,
    pub(crate) last_frame_had_wide_emoji: std::cell::Cell<bool>,
    pub(crate) render_cache: RefCell<TimelineRenderCache>,
}

#[allow(dead_code)]
pub struct TimelineContext<'a> {
    pub theme: &'a Theme,
    pub is_working: bool,
    pub working_millis: u64,
    pub current_activity: Option<&'a crate::ui::animation::AgentActivity>,
    pub spinner_style: crate::ui::animation::SpinnerStyle,
    pub workspace: &'a std::path::Path,
    pub provider: &'a str,
    pub model: &'a str,
}

impl Default for TimelineView {
    fn default() -> Self {
        Self::new()
    }
}

impl TimelineView {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            scroll_offset: std::cell::Cell::new(0),
            auto_scroll: std::cell::Cell::new(true),
            max_scroll: std::cell::Cell::new(0),
            selection: crate::ui::selection::TimelineSelection::new(),
            in_thought_mode: false,
            thought_start: None,
            turn_start: None,
            thought_tag_buffer: String::new(),
            estimated_chars: 0,
            last_frame_had_wide_emoji: std::cell::Cell::new(false),
            render_cache: RefCell::new(TimelineRenderCache::default()),
        }
    }

    /// Resets all timeline entries, selection state, and cached `SumTree` cells in $O(1)$ time.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.estimated_chars = 0;
        self.in_thought_mode = false;
        self.thought_start = None;
        self.turn_start = None;
        self.thought_tag_buffer.clear();
        self.scroll_offset.set(0);
        self.max_scroll.set(0);
        self.auto_scroll.set(true);
        self.selection.clear();
        self.selection.cached_plain_lines.borrow_mut().clear();
        let mut cache = self.render_cache.borrow_mut();
        cache.entry_keys.clear();
        cache.cells.clear();
        cache.header_lines.clear();
        cache.header_key = 0;
    }

    /// Returns an $O(1)$ estimate of accumulated timeline character count for context token fallback.
    #[must_use]
    pub fn estimated_content_chars(&self) -> usize {
        if self.entries.is_empty() {
            return 0;
        }
        if self.estimated_chars > 0 {
            return self.estimated_chars;
        }
        self.entries
            .iter()
            .map(|e| match e {
                TimelineEntry::UserPrompt(s) | TimelineEntry::AssistantMarkdown(s) => s.len(),
                TimelineEntry::ThoughtBlock { text, .. } => text.len(),
                TimelineEntry::ToolFinished { output, .. } => output.len(),
                _ => 0,
            })
            .sum()
    }

    /// Returns `true` if the most recently rendered timeline viewport contained any 2-cell decorative emoji.
    #[inline]
    #[must_use]
    pub fn last_frame_had_wide_emoji(&self) -> bool {
        self.last_frame_had_wide_emoji.get()
    }

    /// Last rendered timeline viewport height (0 before first render).
    pub fn timeline_viewport_height(&self) -> u16 {
        self.selection.timeline_area.get().height
    }

    /// Scrolls timeline up by a number of lines
    pub fn scroll_up(&self, lines: u16) {
        if self.auto_scroll.get() {
            self.scroll_offset.set(self.max_scroll.get());
            self.auto_scroll.set(false);
        }
        let current = self.scroll_offset.get();
        self.scroll_offset.set(current.saturating_sub(lines));
    }

    /// Scrolls timeline down by a number of lines
    pub fn scroll_down(&self, lines: u16) {
        if self.auto_scroll.get() {
            return;
        }
        let current = self.scroll_offset.get();
        let next = current.saturating_add(lines);
        let max = self.max_scroll.get();
        if next >= max {
            self.scroll_offset.set(max);
            self.auto_scroll.set(true);
        } else {
            self.scroll_offset.set(next);
        }
    }

    #[inline]
    fn page_scroll_step(viewport_height: u16) -> u16 {
        if viewport_height > 2 {
            viewport_height.saturating_sub(2)
        } else {
            5
        }
    }

    /// Scrolls timeline up by a page
    pub fn scroll_page_up(&self, viewport_height: u16) {
        self.scroll_up(Self::page_scroll_step(viewport_height));
    }

    /// Scrolls timeline down by a page
    pub fn scroll_page_down(&self, viewport_height: u16) {
        self.scroll_down(Self::page_scroll_step(viewport_height));
    }

    /// Jumps straight to the top of conversation history
    pub fn scroll_to_top(&self) {
        self.auto_scroll.set(false);
        self.scroll_offset.set(0);
    }

    /// Jumps straight to the bottom and resumes auto-scroll
    pub fn scroll_to_bottom(&self) {
        self.auto_scroll.set(true);
        self.scroll_offset.set(self.max_scroll.get());
    }

    /// Returns true if there are no messages or entries in the timeline.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn add_user_message(&mut self, prompt: String) {
        self.finalize_pending_thoughts(None);
        self.estimated_chars = self.estimated_chars.saturating_add(prompt.len());
        if !self.entries.is_empty() {
            self.entries.push(TimelineEntry::TurnSeparator);
        }
        self.entries.push(TimelineEntry::UserPrompt(prompt));
        self.turn_start = Some(std::time::Instant::now());
        self.thought_start = None;
        self.in_thought_mode = false;
        self.thought_tag_buffer.clear();
        self.auto_scroll.set(true);
    }

    /// Returns `true` if there is any unfinalized thought block or pending `<think>` tag timer.
    #[must_use]
    pub fn has_open_thought(&self) -> bool {
        self.thought_start.is_some()
            || self.entries.iter().rev().any(|e| {
                matches!(
                    e,
                    TimelineEntry::ThoughtBlock {
                        started_at: Some(_),
                        ..
                    }
                )
            })
    }

    /// Closes all currently open `ThoughtBlock` entries in the timeline, freezing each block's
    /// elapsed duration from its own `started_at` timestamp so completed thoughts never keep ticking.
    pub fn close_open_thought_blocks(&mut self, explicit_elapsed: Option<f64>) {
        let mut closed_any = false;
        for entry in &mut self.entries {
            if let TimelineEntry::ThoughtBlock {
                text,
                started_at,
                duration_secs,
            } = entry
            {
                if let Some(st) = started_at.take() {
                    *text = Self::sanitize_thought_text(text);
                    let measured = st.elapsed().as_secs_f64();
                    let resolved = match explicit_elapsed {
                        Some(exp) if measured < 0.05 => exp,
                        _ => measured,
                    };
                    *duration_secs = Some(resolved);
                    closed_any = true;
                }
            }
        }
        if !closed_any {
            if let Some(exp) = explicit_elapsed {
                if let Some(TimelineEntry::ThoughtBlock { duration_secs, .. }) = self
                    .entries
                    .iter_mut()
                    .rev()
                    .find(|e| matches!(e, TimelineEntry::ThoughtBlock { .. }))
                {
                    if duration_secs.is_none_or(|d| d < 0.05) {
                        *duration_secs = Some(exp);
                    }
                }
            }
        }
    }

    pub fn append_thought_delta(&mut self, delta: &str) {
        let cleaned = Self::sanitize_thought_text(delta);
        if cleaned.is_empty() {
            return;
        }
        self.estimated_chars = self.estimated_chars.saturating_add(cleaned.len());
        if let Some(TimelineEntry::ThoughtBlock {
            text,
            started_at: Some(_),
            duration_secs: None,
        }) = self.entries.last_mut()
        {
            text.push_str(&cleaned);
            return;
        }

        let seed_start = self.thought_start.take();
        self.close_open_thought_blocks(None);
        let now = seed_start.unwrap_or_else(std::time::Instant::now);
        self.thought_start = Some(now);
        self.entries.push(TimelineEntry::ThoughtBlock {
            text: cleaned,
            started_at: Some(now),
            duration_secs: None,
        });
    }

    #[allow(dead_code)]
    pub fn add_thought_block(&mut self, text: String, duration_secs: Option<f64>) {
        let cleaned = Self::sanitize_thought_text(&text);
        if !cleaned.is_empty() {
            self.estimated_chars = self.estimated_chars.saturating_add(cleaned.len());
            self.close_open_thought_blocks(None);
            self.entries.push(TimelineEntry::ThoughtBlock {
                text: cleaned,
                started_at: None,
                duration_secs,
            });
        }
    }

    pub fn finalize_pending_thoughts(&mut self, elapsed_secs: Option<f64>) {
        if !self.thought_tag_buffer.is_empty() {
            let pending = std::mem::take(&mut self.thought_tag_buffer);
            if self.in_thought_mode {
                self.append_thought_delta(&pending);
            } else {
                self.append_assistant_text(&pending);
            }
        }
        self.close_open_thought_blocks(elapsed_secs);
        self.thought_start = None;
        self.in_thought_mode = false;
    }

    pub const OPEN_THOUGHT_TAGS: &'static [&'static str] = &[
        "<think>",
        "<thought>",
        "<thinking>",
        "<reasoning>",
        "<antThinking>",
        "<Think>",
        "<Thought>",
        "<Thinking>",
        "<Reasoning>",
        "<THINK>",
    ];

    pub const CLOSE_THOUGHT_TAGS: &'static [&'static str] = &[
        "</think>",
        "</thought>",
        "</thinking>",
        "</reasoning>",
        "</antThinking>",
        "</Think>",
        "</Thought>",
        "</Thinking>",
        "</Reasoning>",
        "</THINK>",
    ];

    pub const ALL_THOUGHT_TAGS: &'static [&'static str] = &[
        "<think>",
        "</think>",
        "<thought>",
        "</thought>",
        "<thinking>",
        "</thinking>",
        "<reasoning>",
        "</reasoning>",
        "<antThinking>",
        "</antThinking>",
        "<Think>",
        "</Think>",
        "<Thought>",
        "</Thought>",
        "<Thinking>",
        "</Thinking>",
        "<Reasoning>",
        "</Reasoning>",
        "<THINK>",
        "</THINK>",
    ];

    /// Strips any raw thinking XML tags (<think>, </think>, <thought>, </thought>, etc.)
    pub fn sanitize_thought_text(raw: &str) -> String {
        if !raw.contains('<') {
            return raw.to_string();
        }
        let mut s = raw.to_string();
        for tag in Self::ALL_THOUGHT_TAGS {
            if s.contains(tag) {
                s = s.replace(tag, "");
            }
        }
        s
    }

    /// Strips any stray thinking XML tags that might leak into assistant markdown
    #[inline]
    pub fn sanitize_assistant_text(raw: &str) -> String {
        Self::sanitize_thought_text(raw)
    }

    pub fn append_assistant_delta(&mut self, delta: &str) {
        const OPEN_TAGS: &[&str] = TimelineView::OPEN_THOUGHT_TAGS;
        const CLOSE_TAGS: &[&str] = TimelineView::CLOSE_THOUGHT_TAGS;

        let mut working_text = if self.thought_tag_buffer.is_empty() {
            delta.to_string()
        } else {
            let mut s = std::mem::take(&mut self.thought_tag_buffer);
            s.push_str(delta);
            s
        };

        while !working_text.is_empty() {
            if self.in_thought_mode {
                let earliest_close = CLOSE_TAGS
                    .iter()
                    .filter_map(|&tag| working_text.find(tag).map(|idx| (idx, tag.len())))
                    .min_by_key(|&(idx, _)| idx);

                if let Some((end_idx, tag_len)) = earliest_close {
                    let thought_chunk = &working_text[..end_idx];
                    if !thought_chunk.is_empty() {
                        self.append_thought_delta(thought_chunk);
                    }
                    self.in_thought_mode = false;
                    let dur = self.thought_start.take().map(|s| s.elapsed().as_secs_f64());
                    self.close_open_thought_blocks(dur);
                    let mut next_start = end_idx + tag_len;
                    if working_text[next_start..].starts_with("\r\n") {
                        next_start += 2;
                    } else if working_text[next_start..].starts_with('\n') {
                        next_start += 1;
                    }
                    working_text = working_text[next_start..].to_string();
                } else {
                    if let Some(pos) = working_text.rfind('<') {
                        let tail = &working_text[pos..];
                        if CLOSE_TAGS.iter().any(|tag| tag.starts_with(tail)) {
                            let thought_chunk = &working_text[..pos];
                            if !thought_chunk.is_empty() {
                                self.append_thought_delta(thought_chunk);
                            }
                            self.thought_tag_buffer = tail.to_string();
                            break;
                        }
                    }
                    self.append_thought_delta(&working_text);
                    break;
                }
            } else {
                let earliest_open = OPEN_TAGS
                    .iter()
                    .filter_map(|&tag| working_text.find(tag).map(|idx| (idx, tag.len())))
                    .min_by_key(|&(idx, _)| idx);

                if let Some((start_idx, tag_len)) = earliest_open {
                    let prefix = &working_text[..start_idx];
                    if !prefix.is_empty() {
                        self.append_assistant_text(prefix);
                    }
                    self.close_open_thought_blocks(None);
                    self.in_thought_mode = true;
                    self.thought_start = Some(std::time::Instant::now());
                    let mut next_start = start_idx + tag_len;
                    if working_text[next_start..].starts_with("\r\n") {
                        next_start += 2;
                    } else if working_text[next_start..].starts_with('\n') {
                        next_start += 1;
                    }
                    working_text = working_text[next_start..].to_string();
                } else {
                    if let Some(pos) = working_text.rfind('<') {
                        let tail = &working_text[pos..];
                        if OPEN_TAGS.iter().any(|tag| tag.starts_with(tail)) {
                            let prefix = &working_text[..pos];
                            if !prefix.is_empty() {
                                self.append_assistant_text(prefix);
                            }
                            self.thought_tag_buffer = tail.to_string();
                            break;
                        }
                    }
                    self.append_assistant_text(&working_text);
                    break;
                }
            }
        }
    }

    pub(super) fn append_assistant_text(&mut self, text: &str) {
        let cleaned = Self::sanitize_assistant_text(text);
        if cleaned.is_empty() {
            return;
        }
        self.estimated_chars = self.estimated_chars.saturating_add(cleaned.len());
        self.close_open_thought_blocks(None);
        self.thought_start = None;
        if let Some(TimelineEntry::AssistantMarkdown(ref mut existing)) = self.entries.last_mut() {
            existing.push_str(&cleaned);
        } else {
            self.entries.push(TimelineEntry::AssistantMarkdown(cleaned));
        }
    }

    pub fn add_tool_call(&mut self, name: String, args: String) {
        self.finalize_pending_thoughts(None);
        let display_cmd = Self::extract_cmd_display(&name, &args);
        if let Some(TimelineEntry::ToolStart {
            name: n,
            command_or_path: c,
        }) = self.entries.last()
        {
            if n == &name && c == &display_cmd {
                return;
            }
        }
        self.entries.push(TimelineEntry::ToolStart {
            name,
            command_or_path: display_cmd,
        });
    }

    /// Pre-folds large `ToolFinished` outputs at UI insertion time so `TimelineView` never retains
    /// multi-megabyte raw stdout/file dumps in heap RAM after the visible preview lines are known.
    pub(crate) fn compact_tool_output_for_ui(output: String, success: bool) -> String {
        if output.len() <= 2048 {
            return output;
        }
        let diff_marker = crate::tools::middleware::DIFF_MARKER;
        let max_out_lines = if success {
            crate::constants::UI_MAX_TOOL_OUTPUT_LINES
        } else {
            12
        };

        let (has_diff, diff_raw, regular_raw) = if let Some(rest) = output.strip_prefix(diff_marker)
        {
            if let Some(split_pos) = rest.find("\n\n") {
                (true, &rest[..split_pos + 1], rest[split_pos + 2..].trim())
            } else {
                (true, rest, "")
            }
        } else {
            (false, "", output.trim())
        };

        let mut buf = String::with_capacity(1024);
        if has_diff {
            buf.push_str(diff_marker);
            let mut plus_minus_seen = 0usize;
            let mut total_plus_minus = 0usize;
            for l in diff_raw.lines() {
                if l.starts_with("+ ") || l.starts_with("- ") {
                    total_plus_minus += 1;
                }
            }
            for l in diff_raw.lines() {
                let is_pm = l.starts_with("+ ") || l.starts_with("- ");
                if is_pm {
                    plus_minus_seen += 1;
                }
                buf.push_str(l);
                buf.push('\n');
                if plus_minus_seen > crate::constants::UI_MAX_TOOL_OUTPUT_LINES {
                    let rem = total_plus_minus.saturating_sub(plus_minus_seen);
                    if rem > 0 {
                        buf.push_str(FOLDED_DIFF_SENTINEL);
                        buf.push_str(&rem.to_string());
                        buf.push('\n');
                    }
                    break;
                }
            }
            if !regular_raw.is_empty() {
                buf.push('\n');
            }
        }

        if !regular_raw.is_empty() {
            let total_lines = regular_raw.lines().count();
            for (idx, line) in regular_raw.lines().take(max_out_lines).enumerate() {
                if idx > 0 {
                    buf.push('\n');
                }
                if line.len() > 1024 {
                    let capped: String = line.chars().take(512).collect();
                    buf.push_str(&capped);
                    buf.push('…');
                } else {
                    buf.push_str(line);
                }
            }
            if total_lines > max_out_lines {
                buf.push('\n');
                buf.push_str(FOLDED_OUT_SENTINEL);
                buf.push_str(&(total_lines - max_out_lines).to_string());
            }
        }

        buf
    }

    pub fn finish_tool_call(
        &mut self,
        name: &str,
        success: bool,
        output: String,
        duration_ms: u64,
    ) {
        self.finalize_pending_thoughts(None);
        self.estimated_chars = self.estimated_chars.saturating_add(output.len());
        // Find corresponding tool start from the back to update in-place
        let mut found_idx = None;
        let mut display_cmd = String::new();
        for (idx, entry) in self.entries.iter().enumerate().rev() {
            if let TimelineEntry::ToolStart {
                command_or_path,
                name: n,
            } = entry
            {
                if n == name {
                    display_cmd = command_or_path.clone();
                    found_idx = Some(idx);
                    break;
                }
            }
        }

        if success {
            if let Some(resolved) = Self::resolve_plan_tool_display(name, &output) {
                display_cmd = resolved;
            }
        }

        let compacted_output = Self::compact_tool_output_for_ui(output, success);

        let finished = TimelineEntry::ToolFinished {
            name: name.to_string(),
            command_or_path: display_cmd,
            success,
            output: compacted_output,
            duration_ms: Some(duration_ms),
        };

        if let Some(idx) = found_idx {
            self.entries[idx] = finished;
        } else {
            self.entries.push(finished);
        }
    }

    /// Upgrades opaque plan-tool argument badges (such as `[completed] active` on `update_progress`
    /// or empty arguments on `archive_plan`) into resolved step titles and `(X/Y)` progress counts.
    pub(crate) fn resolve_plan_tool_display(name: &str, output: &str) -> Option<String> {
        let first_line = output.lines().next()?.trim();
        match name {
            "update_progress" => {
                let (raw_title, raw_status) =
                    if let Some(rest) = first_line.strip_prefix("✔ Updated step '") {
                        if let Some((title, tail)) = rest.rsplit_once("' to '") {
                            let status = tail.split('\'').next().unwrap_or("completed");
                            (title.trim(), status.trim())
                        } else if let Some((title, tail)) = rest.rsplit_once("' status to '") {
                            let status = tail.split('\'').next().unwrap_or("completed");
                            (title.trim(), status.trim())
                        } else {
                            return None;
                        }
                    } else if let Some(rest) = first_line.strip_prefix("✔ Step '") {
                        let (title, tail) = rest.rsplit_once("' is already '")?;
                        let status = tail.split('\'').next().unwrap_or("completed");
                        (title.trim(), status.trim())
                    } else if let Some(rest) = first_line.strip_prefix("🎉 All ") {
                        let total = rest.split_whitespace().next()?;
                        return Some(format!("[{total}/{total} completed] All plan tasks"));
                    } else {
                        return None;
                    };

                let count_prefix = if let Some(plan_part) =
                    first_line.split_once("Current plan: ").map(|(_, r)| r)
                {
                    if let Some((macro_part, sub_part)) =
                        plan_part.split_once(" macro tasks completed (active step sub-plan: ")
                    {
                        let macro_frac = macro_part.trim();
                        let sub_frac = sub_part.split_whitespace().next().unwrap_or("");
                        if !macro_frac.is_empty() && !sub_frac.is_empty() {
                            Some(format!("{macro_frac} · sub {sub_frac}"))
                        } else {
                            None
                        }
                    } else {
                        plan_part
                            .split_whitespace()
                            .next()
                            .filter(|tok| tok.contains('/'))
                            .map(str::to_string)
                    }
                } else if let Some(all_part) = first_line.split_once("🎉 All ").map(|(_, r)| r) {
                    all_part
                        .split_whitespace()
                        .next()
                        .filter(|tok| tok.chars().all(|c| c.is_ascii_digit()))
                        .map(|n| format!("{n}/{n}"))
                } else {
                    None
                };

                let max_title_chars = 68usize;
                let short_title: String = if raw_title.chars().count() > max_title_chars {
                    let truncated: String = raw_title.chars().take(max_title_chars).collect();
                    format!("{}…", truncated.trim_end())
                } else {
                    raw_title.to_string()
                };

                if let Some(counts) = count_prefix {
                    Some(format!("[{counts} {raw_status}] {short_title}"))
                } else {
                    Some(format!("[{raw_status}] {short_title}"))
                }
            }
            "archive_plan" => {
                let inner = first_line
                    .strip_prefix("✔ Archived completed task plan (")?
                    .split(')')
                    .next()?;
                let frac = inner.split_whitespace().next()?;
                if frac.contains('/') {
                    Some(format!("[{frac} completed]"))
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    pub fn add_status(&mut self, status: String) {
        self.finalize_pending_thoughts(None);
        self.entries.push(TimelineEntry::SystemStatus(status));
    }

    /// Emits chronological `update_progress` milestone entries in the timeline when plan steps or
    /// sub-steps transition to `Completed` via automatic step-transition / verification catch-up,
    /// while skipping duplicates whenever an explicit `update_progress` tool call already ran.
    pub fn record_plan_progress_update(
        &mut self,
        prev_plan: Option<&LivePlanBlock>,
        next_plan: &LivePlanBlock,
    ) {
        let Some(prev) = prev_plan else {
            return;
        };
        if prev.title != next_plan.title {
            return;
        }

        // If the immediately preceding tool action in this turn was an explicit `update_progress`
        // call (`duration_ms.is_some()`), `finish_tool` already rendered its milestone badge.
        for entry in self.entries.iter().rev() {
            match entry {
                TimelineEntry::UserPrompt(_) => break,
                TimelineEntry::ToolStart { name, .. }
                | TimelineEntry::ToolApproved { name, .. } => {
                    if name == "update_progress" {
                        return;
                    }
                    break;
                }
                TimelineEntry::ToolFinished {
                    name, duration_ms, ..
                } => {
                    if duration_ms.is_some() {
                        if name == "update_progress" {
                            return;
                        }
                        break;
                    }
                }
                _ => {}
            }
        }

        let truncate_title = |raw: &str| -> String {
            let max_chars = 68usize;
            if raw.chars().count() > max_chars {
                let truncated: String = raw.chars().take(max_chars).collect();
                format!("{}…", truncated.trim_end())
            } else {
                raw.to_string()
            }
        };

        let mut pending_milestones: Vec<(String, String, String)> = Vec::new();
        for next_macro in &next_plan.tasks {
            let prev_macro = prev.tasks.iter().find(|t| t.title == next_macro.title);
            if next_macro.status == LivePlanTaskStatus::Completed
                && prev_macro.is_some_and(|pm| pm.status != LivePlanTaskStatus::Completed)
            {
                let short_title = truncate_title(&next_macro.title);
                let badge = format!(
                    "[{}/{} completed] {}",
                    next_plan.completed_tasks, next_plan.total_tasks, short_title
                );
                let out = format!(
                    "✔ Updated step '{}' to 'completed'. Current plan: {}/{} tasks completed.",
                    next_macro.title, next_plan.completed_tasks, next_plan.total_tasks
                );
                pending_milestones.push((next_macro.title.clone(), badge, out));
            } else if next_macro.status != LivePlanTaskStatus::Completed
                && !next_macro.sub_tasks.is_empty()
            {
                let sub_total = next_macro.sub_tasks.len();
                let sub_done = next_macro
                    .sub_tasks
                    .iter()
                    .filter(|s| s.status == LivePlanTaskStatus::Completed)
                    .count();
                for next_sub in &next_macro.sub_tasks {
                    let was_uncompleted = prev_macro
                        .and_then(|pm| pm.sub_tasks.iter().find(|ps| ps.title == next_sub.title))
                        .is_some_and(|ps| ps.status != LivePlanTaskStatus::Completed);
                    if next_sub.status == LivePlanTaskStatus::Completed && was_uncompleted {
                        let short_title = truncate_title(&next_sub.title);
                        let badge = format!(
                            "[{}/{} · sub {}/{} completed] {}",
                            next_plan.completed_tasks,
                            next_plan.total_tasks,
                            sub_done,
                            sub_total,
                            short_title
                        );
                        let out = format!(
                            "✔ Updated step '{}' to 'completed'. Current plan: {}/{} macro tasks completed (active step sub-plan: {}/{} sub-steps).",
                            next_sub.title,
                            next_plan.completed_tasks,
                            next_plan.total_tasks,
                            sub_done,
                            sub_total
                        );
                        pending_milestones.push((next_sub.title.clone(), badge, out));
                    }
                }
            }
        }

        for (raw_title, badge, out) in pending_milestones {
            let quoted = format!("'{raw_title}'");
            let already_recorded = self
                .entries
                .iter()
                .rev()
                .take_while(|e| !matches!(e, TimelineEntry::UserPrompt(_)))
                .any(|e| {
                    matches!(
                        e,
                        TimelineEntry::ToolFinished { name, output, .. }
                            if name == "update_progress" && output.contains(&quoted)
                    )
                });
            if !already_recorded {
                self.entries.push(TimelineEntry::ToolFinished {
                    name: "update_progress".to_string(),
                    command_or_path: badge,
                    success: true,
                    output: out,
                    duration_ms: None,
                });
            }
        }
    }

    /// Adds or updates in-place the Live Plan & Execution Progress card in the timeline
    #[allow(dead_code)]
    pub fn update_live_plan(&mut self, plan: LivePlanBlock) {
        self.finalize_pending_thoughts(None);
        if let Some(TimelineEntry::LivePlan(ref mut existing)) = self.entries.last_mut() {
            *existing = plan;
        } else {
            self.entries.push(TimelineEntry::LivePlan(plan));
        }
        self.auto_scroll.set(true);
    }

    pub fn add_context_compaction(
        &mut self,
        tier: usize,
        turns_summarized: usize,
        tokens_before: usize,
        tokens_after: usize,
        savings_percent: usize,
    ) {
        self.finalize_pending_thoughts(None);
        self.entries.push(TimelineEntry::ContextCompaction {
            tier,
            turns_summarized,
            tokens_before,
            tokens_after,
            savings_percent,
        });
    }

    pub fn add_git_commit(&mut self, hash: String, message: String, files: Vec<String>) {
        self.finalize_pending_thoughts(None);
        self.entries.push(TimelineEntry::GitCommit {
            hash,
            message,
            files,
        });
    }

    #[inline]
    fn hash_str_edges(h: &mut std::collections::hash_map::DefaultHasher, s: &str) {
        s.len().hash(h);
        let bytes = s.as_bytes();
        let head = &bytes[..bytes.len().min(64)];
        head.hash(h);
        if bytes.len() > 64 {
            let tail = &bytes[bytes.len() - 64..];
            tail.hash(h);
        }
    }

    #[inline]
    fn hash_color(h: &mut std::collections::hash_map::DefaultHasher, c: Color) {
        std::mem::discriminant(&c).hash(h);
        if let Color::Rgb(r, g, b) = c {
            (r, g, b).hash(h);
        } else if let Color::Indexed(i) = c {
            i.hash(h);
        }
    }

    fn theme_fingerprint(theme: &Theme) -> u64 {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        Self::hash_color(&mut h, theme.bg_primary);
        Self::hash_color(&mut h, theme.bg_elevated);
        Self::hash_color(&mut h, theme.text_primary);
        Self::hash_color(&mut h, theme.muted);
        Self::hash_color(&mut h, theme.brand_accent);
        Self::hash_color(&mut h, theme.info);
        Self::hash_color(&mut h, theme.success);
        Self::hash_color(&mut h, theme.warning);
        Self::hash_color(&mut h, theme.destructive);
        Self::hash_color(&mut h, theme.border);
        h.finish()
    }

    fn entry_content_key(entry: &TimelineEntry, is_working: bool, spinner_tick: u64) -> u64 {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        std::mem::discriminant(entry).hash(&mut h);
        match entry {
            TimelineEntry::TurnSeparator => {}
            TimelineEntry::UserPrompt(p) | TimelineEntry::SystemStatus(p) => {
                Self::hash_str_edges(&mut h, p);
            }
            TimelineEntry::AssistantMarkdown(text) => {
                Self::hash_str_edges(&mut h, text);
            }
            TimelineEntry::ThoughtBlock {
                text,
                started_at,
                duration_secs,
            } => {
                Self::hash_str_edges(&mut h, text);
                let dur_decisecs = duration_secs.map(|d| (d * 10.0).round() as u64);
                dur_decisecs.hash(&mut h);
                if duration_secs.is_none() {
                    if let Some(st) = started_at {
                        is_working.hash(&mut h);
                        if is_working {
                            let live_decisecs = (st.elapsed().as_millis() / 100) as u64;
                            live_decisecs.hash(&mut h);
                        }
                    }
                }
            }
            TimelineEntry::ToolStart {
                name,
                command_or_path,
            }
            | TimelineEntry::ToolApproved {
                name,
                command_or_path,
            } => {
                name.hash(&mut h);
                command_or_path.hash(&mut h);
            }
            TimelineEntry::ToolFinished {
                name,
                command_or_path,
                success,
                output,
                duration_ms,
            } => {
                name.hash(&mut h);
                command_or_path.hash(&mut h);
                success.hash(&mut h);
                duration_ms.hash(&mut h);
                Self::hash_str_edges(&mut h, output);
            }
            TimelineEntry::SubagentTree(block) => {
                block.id.hash(&mut h);
                block.role_name.hash(&mut h);
                block.items.len().hash(&mut h);
                if let Some(last) = block.items.last() {
                    last.name.hash(&mut h);
                    last.detail.hash(&mut h);
                    std::mem::discriminant(&last.status).hash(&mut h);
                }
                block.is_running.hash(&mut h);
                block.is_success.hash(&mut h);
                block.tokens_used.hash(&mut h);
                block.duration_ms.hash(&mut h);
                block.outcome.as_deref().map(str::len).hash(&mut h);
                block.error_message.as_deref().map(str::len).hash(&mut h);
                if block.is_running {
                    spinner_tick.hash(&mut h);
                }
            }
            TimelineEntry::SubagentSwarm(swarm) => {
                swarm.title.hash(&mut h);
                swarm.workers.len().hash(&mut h);
                swarm.is_expanded.hash(&mut h);
                swarm.total_tokens.hash(&mut h);
                swarm.is_running.hash(&mut h);
                swarm.duration_ms.hash(&mut h);
                for w in &swarm.workers {
                    w.is_running.hash(&mut h);
                    w.is_success.hash(&mut h);
                    w.tokens_used.hash(&mut h);
                    w.items.len().hash(&mut h);
                }
                if swarm.is_running {
                    spinner_tick.hash(&mut h);
                }
            }
            TimelineEntry::LivePlan(plan) => {
                plan.title.hash(&mut h);
                plan.total_tasks.hash(&mut h);
                plan.completed_tasks.hash(&mut h);
                plan.active_task.hash(&mut h);
                plan.tasks.len().hash(&mut h);
                for t in &plan.tasks {
                    t.status.hash(&mut h);
                    for st in &t.sub_tasks {
                        st.status.hash(&mut h);
                    }
                }
            }
            TimelineEntry::ContextCompaction {
                tier,
                turns_summarized,
                tokens_before,
                tokens_after,
                savings_percent,
            } => {
                (
                    *tier,
                    *turns_summarized,
                    *tokens_before,
                    *tokens_after,
                    *savings_percent,
                )
                    .hash(&mut h);
            }
            TimelineEntry::GitCommit {
                hash,
                message,
                files,
            } => {
                hash.hash(&mut h);
                Self::hash_str_edges(&mut h, message);
                files.len().hash(&mut h);
            }
        }
        h.finish()
    }

    #[inline]
    fn line_contains_wide_emoji(line: &Line<'_>) -> bool {
        line.spans.iter().any(|s| {
            !s.content.is_ascii()
                && s.content.chars().any(|ch| {
                    (ch as u32) >= 0x2600
                        && crate::ui::markdown::MarkdownRenderer::is_decorative_emoji_or_symbol(ch)
                })
        })
    }

    #[inline]
    fn into_static_line(line: Line<'_>) -> Line<'static> {
        let spans: Vec<Span<'static>> = line
            .spans
            .into_iter()
            .map(|s| Span::styled(s.content.into_owned(), s.style))
            .collect();
        Line {
            spans,
            style: line.style,
            alignment: line.alignment,
        }
    }

    fn compile_header_lines(area: Rect, ctx: &TimelineContext) -> (Vec<Line<'static>>, bool) {
        let theme = ctx.theme;
        let usable_width = (area.width.saturating_sub(1) as usize).max(20);
        let mut lines: Vec<Line<'static>> = Vec::with_capacity(10);

        static HOME_DIR: std::sync::OnceLock<Option<std::path::PathBuf>> =
            std::sync::OnceLock::new();
        let cached_home = HOME_DIR.get_or_init(dirs::home_dir);
        let display_path = if let Some(ref home) = cached_home {
            if let Ok(rel) = ctx.workspace.strip_prefix(home) {
                format!("~/{}", rel.display())
            } else {
                ctx.workspace.display().to_string()
            }
        } else {
            ctx.workspace.display().to_string()
        };

        let prov_display = if ctx.provider.is_empty() {
            String::new()
        } else {
            let mut chars = ctx.provider.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        };
        let model_summary = if prov_display.is_empty() {
            ctx.model.to_string()
        } else {
            format!("{} · {}", prov_display, ctx.model)
        };

        lines.push(Line::from(String::new()));
        lines.push(Line::from(vec![
            Span::raw(" "),
            Span::styled("████", Style::default().fg(theme.brand_accent)),
            Span::raw("    "),
            Span::styled("████", Style::default().fg(theme.info)),
            Span::raw("   "),
            Span::styled(
                "MiniCode",
                Style::default()
                    .fg(theme.text_primary)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(" v{}", env!("CARGO_PKG_VERSION")),
                Style::default().fg(theme.muted),
            ),
        ]));
        lines.push(Line::from(vec![
            Span::raw(" "),
            Span::styled("████", Style::default().fg(theme.brand_accent)),
            Span::raw("    "),
            Span::styled("████", Style::default().fg(theme.info)),
            Span::raw("   "),
            Span::styled("Build better, with AI", Style::default().fg(theme.muted)),
        ]));
        lines.push(Line::from(vec![
            Span::raw(" "),
            Span::styled("██  ", Style::default().fg(theme.brand_accent)),
            Span::raw("    "),
            Span::styled("  ██", Style::default().fg(theme.info)),
            Span::raw("   "),
            Span::styled(model_summary, Style::default().fg(theme.brand_accent)),
        ]));
        lines.push(Line::from(vec![
            Span::raw("                "),
            Span::styled(display_path, Style::default().fg(theme.muted)),
        ]));
        lines.push(Line::from(String::new()));
        let divider_w = 60usize.min(area.width.saturating_sub(2) as usize);
        lines.push(Line::from(vec![
            Span::raw(" "),
            Span::styled("─".repeat(divider_w), Style::default().fg(theme.border)),
        ]));
        lines.push(Line::from(String::new()));

        let wrapped: Vec<Line<'static>> = Self::wrap_lines_to_width(lines, usable_width)
            .into_iter()
            .map(Self::into_static_line)
            .collect();
        let has_emoji = wrapped.iter().any(Self::line_contains_wide_emoji);
        (wrapped, has_emoji)
    }

    fn compile_entry_cell(
        entry: &TimelineEntry,
        area_width: u16,
        usable_width: usize,
        ctx: &TimelineContext,
    ) -> RenderedCell {
        let theme = ctx.theme;
        let mut lines: Vec<Line<'_>> = Vec::new();
        let mut prompt_anchor: Option<Arc<str>> = None;

        match entry {
            TimelineEntry::TurnSeparator => {
                let divider_w = 60usize.min(area_width.saturating_sub(2) as usize);
                lines.push(Line::from(String::new()));
                lines.push(Line::from(vec![
                    Span::raw(" "),
                    Span::styled("─".repeat(divider_w), Style::default().fg(theme.border)),
                ]));
                lines.push(Line::from(String::new()));
            }
            TimelineEntry::UserPrompt(prompt) => {
                prompt_anchor = Some(Arc::from(prompt.as_str()));
                let prompt_lines: Vec<&str> = prompt.lines().collect();

                if prompt_lines.is_empty() {
                    lines.push(Line::from(vec![Span::styled(
                        "› ",
                        Style::default()
                            .fg(theme.brand_accent)
                            .add_modifier(Modifier::BOLD),
                    )]));
                } else {
                    for (idx, pline) in prompt_lines.iter().enumerate() {
                        let prefix = if idx == 0 { "› " } else { "  " };
                        let prefix_style = if idx == 0 {
                            Style::default()
                                .fg(theme.brand_accent)
                                .add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().fg(theme.muted)
                        };
                        let clean_pline =
                            crate::ui::markdown::MarkdownRenderer::normalize_status_glyphs(pline);
                        let row_spans = vec![
                            Span::styled(prefix, prefix_style),
                            Span::styled(
                                clean_pline,
                                Style::default()
                                    .fg(theme.text_primary)
                                    .add_modifier(if idx == 0 {
                                        Modifier::BOLD
                                    } else {
                                        Modifier::empty()
                                    }),
                            ),
                        ];
                        lines.push(Line::from(row_spans));
                    }
                }
                lines.push(Line::from(String::new()));
            }
            TimelineEntry::ThoughtBlock {
                text,
                started_at,
                duration_secs,
            } => {
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    let header = match (duration_secs, started_at) {
                        (Some(secs), _) if *secs >= 0.1 => format!("• Thought for {:.1}s", secs),
                        (Some(_), _) => "• Thought".to_string(),
                        (None, Some(st)) if ctx.is_working => {
                            format!("• Thinking ({:.1}s)", st.elapsed().as_secs_f64().max(0.1))
                        }
                        (None, _) => "• Thought".to_string(),
                    };

                    lines.push(Line::from(vec![Span::styled(
                        header,
                        Style::default()
                            .fg(theme.muted)
                            .add_modifier(Modifier::BOLD),
                    )]));
                    let max_t_width = (area_width.saturating_sub(4) as usize).max(20);
                    for t_line in trimmed.lines() {
                        let chunks = Self::split_line_into_chunks(t_line, max_t_width);
                        for chunk in chunks {
                            lines.push(Line::from(vec![
                                Span::styled("  ", Style::default()),
                                Span::styled(
                                    chunk,
                                    Style::default()
                                        .fg(theme.muted)
                                        .add_modifier(Modifier::ITALIC),
                                ),
                            ]));
                        }
                    }
                    lines.push(Line::from(String::new()));
                }
            }
            TimelineEntry::AssistantMarkdown(text) => {
                let parsed_lines = Self::render_markdown_with_width(text, theme, usable_width);
                lines.extend(parsed_lines);
            }
            TimelineEntry::ToolStart {
                name,
                command_or_path,
            } => {
                let title = Self::format_tool_title(name);
                let mut spans = vec![
                    Span::styled("• ", Style::default().fg(theme.brand_accent)),
                    Span::styled(
                        format!("{} ", title),
                        Style::default()
                            .fg(theme.brand_accent)
                            .add_modifier(Modifier::BOLD),
                    ),
                ];
                if !command_or_path.is_empty() {
                    let clean_cmd = crate::ui::markdown::MarkdownRenderer::normalize_status_glyphs(
                        command_or_path,
                    );
                    spans.push(Span::styled(
                        clean_cmd,
                        Style::default().fg(theme.text_primary),
                    ));
                }
                lines.push(Line::from(spans));
                lines.push(Line::from(vec![
                    Span::styled("  └ ", Style::default().fg(theme.muted)),
                    Span::styled(
                        "running...",
                        Style::default()
                            .fg(theme.muted)
                            .add_modifier(Modifier::ITALIC),
                    ),
                ]));
                lines.push(Line::from(String::new()));
            }
            TimelineEntry::ToolApproved {
                name,
                command_or_path,
                ..
            } => {
                let title = Self::format_tool_title(name);
                let clean_cmd =
                    crate::ui::markdown::MarkdownRenderer::normalize_status_glyphs(command_or_path);
                lines.push(Line::from(vec![
                    Span::styled(
                        "✓ ",
                        Style::default()
                            .fg(theme.success)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled("You approved ", Style::default().fg(theme.muted)),
                    Span::styled(
                        format!("{} ", title),
                        Style::default()
                            .fg(theme.brand_accent)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(clean_cmd, Style::default().fg(theme.text_primary)),
                    Span::styled(" this time", Style::default().fg(theme.muted)),
                ]));
                lines.push(Line::from(String::new()));
            }
            TimelineEntry::ToolFinished {
                name,
                command_or_path,
                output,
                success,
                duration_ms,
                ..
            } => {
                let title = Self::format_tool_title(name);
                let status_color = if *success {
                    theme.brand_accent
                } else {
                    theme.destructive
                };

                let bullet = if *success { "• " } else { "✗ " };
                let mut spans = vec![
                    Span::styled(
                        bullet,
                        Style::default()
                            .fg(status_color)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!("{} ", title),
                        Style::default()
                            .fg(status_color)
                            .add_modifier(Modifier::BOLD),
                    ),
                ];
                if !command_or_path.is_empty() {
                    let clean_cmd = crate::ui::markdown::MarkdownRenderer::normalize_status_glyphs(
                        command_or_path,
                    );
                    spans.push(Span::styled(
                        clean_cmd,
                        Style::default().fg(theme.text_primary),
                    ));
                }
                lines.push(Line::from(spans));

                let diff_marker = crate::tools::middleware::DIFF_MARKER;
                let (diff_section, regular_output) =
                    if let Some(rest) = output.strip_prefix(diff_marker) {
                        if let Some(split_pos) = rest.find("\n\n") {
                            (&rest[..split_pos + 1], rest[split_pos + 2..].trim())
                        } else {
                            (rest, "")
                        }
                    } else {
                        ("", output.trim())
                    };

                if !diff_section.is_empty() {
                    let diff_usable_width = area_width.saturating_sub(2) as usize;
                    let max_content_width = diff_usable_width.saturating_sub(6).max(20);
                    let mut diff_line_count = 0;
                    for diff_line in diff_section.lines() {
                        if let Some(rem_str) = diff_line.strip_prefix(FOLDED_DIFF_SENTINEL) {
                            lines.push(Line::from(vec![
                                Span::styled("  │ ", Style::default().fg(theme.muted)),
                                Span::styled(
                                    format!("... +{} diff lines (folded)", rem_str.trim()),
                                    Style::default().fg(theme.border),
                                ),
                            ]));
                            break;
                        }
                        if diff_line.starts_with("---") || diff_line.starts_with("+++") {
                            let chunks = Self::split_line_into_chunks(diff_line, max_content_width);
                            for (c_idx, chunk) in chunks.into_iter().enumerate() {
                                let pfx = if c_idx == 0 { "  │ " } else { "  │   " };
                                lines.push(Line::from(vec![
                                    Span::styled(pfx, Style::default().fg(theme.muted)),
                                    Span::styled(
                                        chunk,
                                        Style::default()
                                            .fg(theme.muted)
                                            .add_modifier(Modifier::ITALIC),
                                    ),
                                ]));
                            }
                        } else if let Some((marker, color, rest)) = diff_line
                            .strip_prefix("+ ")
                            .map(|r| ("+", theme.success, r))
                            .or_else(|| {
                                diff_line
                                    .strip_prefix("- ")
                                    .map(|r| ("-", theme.destructive, r))
                            })
                        {
                            let chunks = Self::split_line_into_chunks(rest, max_content_width);
                            for (c_idx, chunk) in chunks.into_iter().enumerate() {
                                if c_idx == 0 {
                                    lines.push(Line::from(vec![
                                        Span::styled("  │ ", Style::default().fg(theme.muted)),
                                        Span::styled(
                                            marker,
                                            Style::default().fg(color).add_modifier(Modifier::BOLD),
                                        ),
                                        Span::styled(
                                            format!(" {}", chunk),
                                            Style::default().fg(color),
                                        ),
                                    ]));
                                } else {
                                    lines.push(Line::from(vec![
                                        Span::styled("  │   ", Style::default().fg(theme.muted)),
                                        Span::styled(chunk, Style::default().fg(color)),
                                    ]));
                                }
                            }
                            diff_line_count += 1;
                        } else if let Some(rest) = diff_line.strip_prefix("  ") {
                            let chunks = Self::split_line_into_chunks(rest, max_content_width);
                            for chunk in chunks {
                                lines.push(Line::from(vec![
                                    Span::styled("  │   ", Style::default().fg(theme.muted)),
                                    Span::styled(chunk, Style::default().fg(theme.muted)),
                                ]));
                            }
                        }
                        if diff_line_count > crate::constants::UI_MAX_TOOL_OUTPUT_LINES {
                            let remaining = diff_section
                                .lines()
                                .filter(|l| l.starts_with("+ ") || l.starts_with("- "))
                                .count()
                                .saturating_sub(diff_line_count);
                            if remaining > 0 {
                                lines.push(Line::from(vec![
                                    Span::styled("  │ ", Style::default().fg(theme.muted)),
                                    Span::styled(
                                        format!("... +{} diff lines (folded)", remaining),
                                        Style::default().fg(theme.border),
                                    ),
                                ]));
                            }
                            break;
                        }
                    }
                }

                let trimmed = regular_output;
                if !trimmed.is_empty() {
                    let out_usable_width = area_width.saturating_sub(2) as usize;
                    let max_content_width = out_usable_width.saturating_sub(6).max(20);
                    let max_lines = if *success {
                        crate::constants::UI_MAX_TOOL_OUTPUT_LINES
                    } else {
                        12
                    };
                    let border_color = if *success {
                        theme.muted
                    } else {
                        theme.destructive
                    };
                    let mut after_panic = false;
                    let mut pre_folded_rem: Option<usize> = None;

                    for (shown_count, raw_out_line) in trimmed.lines().enumerate() {
                        if let Some(rem_str) = raw_out_line.strip_prefix(FOLDED_OUT_SENTINEL) {
                            pre_folded_rem = rem_str.trim().parse::<usize>().ok();
                            break;
                        }
                        if shown_count >= max_lines {
                            break;
                        }
                        let out_line = if !*success && raw_out_line.contains("Raw arguments") {
                            "[Arguments payload truncated: model exceeded single-call capacity]"
                        } else {
                            raw_out_line
                        };
                        let trimmed_line = out_line.trim();
                        let is_panic_header = trimmed_line.contains("panicked at");
                        let line_style = classify_diagnostic_line(
                            out_line,
                            trimmed_line,
                            is_panic_header || after_panic,
                            *success,
                        )
                        .style(theme);
                        after_panic = is_panic_header;

                        let chunks = Self::split_line_into_chunks(out_line, max_content_width);
                        for (c_idx, chunk) in chunks.into_iter().enumerate() {
                            let pfx = if c_idx == 0 { "  │ " } else { "  │   " };
                            lines.push(Line::from(vec![
                                Span::styled(pfx, Style::default().fg(border_color)),
                                Span::styled(chunk, line_style),
                            ]));
                        }
                    }

                    let folded_remaining = pre_folded_rem.or_else(|| {
                        let total = trimmed.lines().count();
                        (total > max_lines).then(|| total.saturating_sub(max_lines))
                    });
                    if let Some(remaining) = folded_remaining.filter(|&r| r > 0) {
                        lines.push(Line::from(vec![
                            Span::styled("  │ ", Style::default().fg(border_color)),
                            Span::styled(
                                format!("... +{} lines (output folded)", remaining),
                                Style::default().fg(theme.border),
                            ),
                        ]));
                    }
                }

                let dur_str = duration_ms
                    .filter(|d| *d > 0)
                    .map(|d| format!(" ({}ms)", d))
                    .unwrap_or_default();
                let (status_glyph, glyph_color, status_text) = if *success {
                    ("✓", theme.success, "completed")
                } else {
                    ("✗", theme.destructive, "failed")
                };

                let foot_border = if *success {
                    theme.muted
                } else {
                    theme.destructive
                };
                lines.push(Line::from(vec![
                    Span::styled("  └ ", Style::default().fg(foot_border)),
                    Span::styled(
                        format!("{} ", status_glyph),
                        Style::default()
                            .fg(glyph_color)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!("{}{}", status_text, dur_str),
                        Style::default().fg(if *success {
                            theme.muted
                        } else {
                            theme.destructive
                        }),
                    ),
                ]));
                lines.push(Line::from(String::new()));
            }
            TimelineEntry::SubagentTree(block) => {
                Self::render_subagent_tree_entry(block, ctx, theme, &mut lines);
            }
            TimelineEntry::SubagentSwarm(swarm) => {
                Self::render_subagent_swarm_entry(swarm, ctx, theme, area_width, &mut lines);
            }
            TimelineEntry::ContextCompaction {
                tier,
                turns_summarized,
                tokens_before,
                tokens_after,
                savings_percent,
            } => {
                Self::render_context_compaction_entry(
                    *tier,
                    *turns_summarized,
                    *tokens_before,
                    *tokens_after,
                    *savings_percent,
                    theme,
                    &mut lines,
                );
            }
            TimelineEntry::GitCommit {
                hash,
                message,
                files,
            } => {
                Self::render_git_commit_entry(
                    hash,
                    message,
                    files,
                    usable_width,
                    theme,
                    &mut lines,
                );
            }
            TimelineEntry::LivePlan(plan) => {
                Self::render_live_plan_entry(plan, theme, area_width, &mut lines);
            }
            TimelineEntry::SystemStatus(status) => {
                Self::render_system_status_entry(status, theme, &mut lines);
            }
        }

        let static_lines: Vec<Line<'static>> = Self::wrap_lines_to_width(lines, usable_width)
            .into_iter()
            .map(Self::into_static_line)
            .collect();
        let has_wide_emoji = static_lines.iter().any(Self::line_contains_wide_emoji);
        RenderedCell {
            lines: Arc::from(static_lines),
            prompt_anchor,
            has_wide_emoji,
        }
    }

    /// Lazily resolves a single rendered timeline row into plain text in $O(\log N)$ time for
    /// mouse drag / clipboard selection without allocating strings for off-screen rows.
    pub(crate) fn resolve_plain_line(&self, row: usize) -> Option<String> {
        let cache = self.render_cache.borrow();
        if cache.header_lines.is_empty() && cache.cells.is_empty() {
            return None;
        }
        let line_to_plain = |l: &Line<'_>| -> String {
            let mut s = String::new();
            for sp in &l.spans {
                s.push_str(sp.content.as_ref());
            }
            s
        };
        if row < cache.header_lines.len() {
            return Some(line_to_plain(&cache.header_lines[row]));
        }
        let tree_row = row - cache.header_lines.len();
        if tree_row >= cache.cells.summary().line_count {
            return None;
        }
        let mut out: Option<String> = None;
        cache.cells.visit_dimension_range::<CellLineDim, _>(
            tree_row,
            tree_row + 1,
            |_, cell_start, cell| {
                let local_idx = tree_row.saturating_sub(cell_start);
                if let Some(line) = cell.lines.get(local_idx) {
                    out = Some(line_to_plain(line));
                }
            },
        );
        out
    }

    pub fn render(&self, frame: &mut Frame, area: Rect, ctx: &TimelineContext) {
        let theme = ctx.theme;
        let usable_width = (area.width.saturating_sub(1) as usize).max(20);

        if self.entries.is_empty() {
            self.last_frame_had_wide_emoji.set(false);
            return;
        }

        let theme_key = Self::theme_fingerprint(theme);
        let mut layout_hasher = std::collections::hash_map::DefaultHasher::new();
        (area.width, theme_key).hash(&mut layout_hasher);
        let layout_key = layout_hasher.finish();

        let spinner_tick = ctx.working_millis / crate::constants::SPINNER_FRAME_MS;

        let mut cache = self.render_cache.borrow_mut();
        if cache.layout_key != layout_key {
            cache.layout_key = layout_key;
            cache.header_key = 0;
            cache.header_lines.clear();
            cache.entry_keys.clear();
            cache.cells.clear();
        } else if self.entries.len() < cache.entry_keys.len() {
            cache.entry_keys.truncate(self.entries.len());
            cache.cells.truncate(self.entries.len());
        }

        // Update existing entries only when their $O(1)$ content/animation key changed
        for idx in 0..cache.entry_keys.len() {
            let key = Self::entry_content_key(&self.entries[idx], ctx.is_working, spinner_tick);
            if cache.entry_keys[idx] != key {
                let cell =
                    Self::compile_entry_cell(&self.entries[idx], area.width, usable_width, ctx);
                cache.cells.set(idx, cell);
                cache.entry_keys[idx] = key;
            }
        }

        // Append newly added entries into the `SumTree` in $O(\log N)$ time
        for idx in cache.entry_keys.len()..self.entries.len() {
            let key = Self::entry_content_key(&self.entries[idx], ctx.is_working, spinner_tick);
            let cell = Self::compile_entry_cell(&self.entries[idx], area.width, usable_width, ctx);
            cache.cells.push(cell);
            cache.entry_keys.push(key);
        }

        // 1. Top Session Header Banner (`prompt_count > 0` in `SumTree` root summary is $O(1)$!)
        let has_user_prompt = cache.cells.summary().prompt_count > 0;
        let header_key = {
            let mut hh = std::collections::hash_map::DefaultHasher::new();
            (
                has_user_prompt,
                area.width,
                theme_key,
                ctx.provider,
                ctx.model,
                ctx.workspace,
            )
                .hash(&mut hh);
            hh.finish()
        };
        if cache.header_key != header_key {
            cache.header_key = header_key;
            if has_user_prompt {
                let (h_lines, h_emoji) = Self::compile_header_lines(area, ctx);
                cache.header_lines = h_lines;
                cache.header_has_emoji = h_emoji;
            } else {
                cache.header_lines.clear();
                cache.header_has_emoji = false;
            }
        }

        let header_len = cache.header_lines.len();
        let total_lines_usize = header_len + cache.cells.summary().line_count;
        let total_lines = total_lines_usize.min(u16::MAX as usize) as u16;
        let viewport_height = area.height;
        let max_scroll = total_lines.saturating_sub(viewport_height);
        self.max_scroll.set(max_scroll);
        let scroll = if self.auto_scroll.get() {
            self.scroll_offset.set(max_scroll);
            max_scroll
        } else {
            let s = self.scroll_offset.get().min(max_scroll);
            self.scroll_offset.set(s);
            s
        };

        self.selection.timeline_area.set(area);
        let selection_bounds = self.selection.ordered_bounds();

        // Slice ONLY the `[start_idx .. end_idx]` visible viewport rows in $O(\log N + H)$ time!
        let start_idx = (scroll as usize).min(total_lines_usize);
        let end_idx = (start_idx + viewport_height as usize).min(total_lines_usize);
        let full_w = area.width as usize;
        let clear_style = Style::default().bg(theme.bg_primary).fg(theme.text_primary);
        let mut visible_lines: Vec<Line<'static>> = Vec::with_capacity(viewport_height as usize);
        let mut viewport_has_emoji = false;

        let mut push_formatted_visible_row = |global_row: usize, src_line: &Line<'static>| {
            let mut line = src_line.clone();
            if let Some(((c1, r1), (c2, r2))) = selection_bounds {
                let r1_u = r1 as usize;
                let r2_u = r2 as usize;
                if global_row >= r1_u && global_row <= r2_u {
                    let (sel_start, sel_end) = if r1_u == r2_u {
                        (c1 as usize, c2 as usize)
                    } else if global_row == r1_u {
                        (c1 as usize, usize::MAX)
                    } else if global_row == r2_u {
                        (0, c2 as usize)
                    } else {
                        (0, usize::MAX)
                    };
                    line = Self::into_static_line(
                        crate::ui::selection::TimelineSelection::apply_selection_to_line(
                            line, sel_start, sel_end, theme,
                        ),
                    );
                }
            }
            let line_w: usize = line
                .spans
                .iter()
                .map(|s| UnicodeWidthStr::width(s.content.as_ref()))
                .sum();
            if line_w < full_w {
                line.spans
                    .push(Span::styled(" ".repeat(full_w - line_w), clear_style));
            }
            visible_lines.push(line);
        };

        if start_idx < header_len {
            let h_end = end_idx.min(header_len);
            if cache.header_has_emoji {
                viewport_has_emoji = true;
            }
            for (r, h_line) in cache.header_lines[start_idx..h_end].iter().enumerate() {
                push_formatted_visible_row(start_idx + r, h_line);
            }
        }

        if end_idx > header_len {
            let tree_start = start_idx.saturating_sub(header_len);
            let tree_end = end_idx - header_len;
            cache.cells.visit_dimension_range::<CellLineDim, _>(
                tree_start,
                tree_end,
                |_item_idx, cell_start_dim, cell| {
                    if cell.has_wide_emoji {
                        viewport_has_emoji = true;
                    }
                    let local_from = tree_start.saturating_sub(cell_start_dim);
                    let local_to = (tree_end - cell_start_dim).min(cell.lines.len());
                    for local_i in local_from..local_to {
                        let global_row = header_len + cell_start_dim + local_i;
                        push_formatted_visible_row(global_row, &cell.lines[local_i]);
                    }
                },
            );
        }

        while visible_lines.len() < viewport_height as usize {
            visible_lines.push(Line::from(vec![Span::styled(
                " ".repeat(full_w),
                clear_style,
            )]));
        }

        // ── Scroll-Aware Stacked Sticky Top Prompt Header ($O(\log N)$ SumTree Lookup) ──
        let scroll_row = scroll as usize;
        let active_prompt_info: Option<(usize, Arc<str>, Option<usize>)> = if scroll_row
            > header_len
            && cache.cells.summary().prompt_count > 0
        {
            let target_tree_row = scroll_row - header_len;
            cache
                .cells
                .rfind::<CellLineDim, _, _>(
                    |sub_start, sum| sum.prompt_count > 0 && sub_start < target_tree_row,
                    |item_start, cell| cell.prompt_anchor.is_some() && item_start < target_tree_row,
                )
                .and_then(|(anchor_item_idx, anchor_tree_row, anchor_cell)| {
                    let prompt_arc = anchor_cell.prompt_anchor.as_ref()?.clone();
                    let next_global_row = cache
                        .cells
                        .find_first::<CellLineDim, _, _>(
                            |sub_start, sum| {
                                sum.prompt_count > 0 && sub_start + sum.line_count > anchor_tree_row
                            },
                            |idx, _start, cell| {
                                idx > anchor_item_idx && cell.prompt_anchor.is_some()
                            },
                        )
                        .map(|(_, next_tree_row, _)| header_len + next_tree_row);
                    Some((anchor_item_idx, prompt_arc, next_global_row))
                })
        } else {
            None
        };
        drop(cache);

        if let Some((_, prompt_text, next_prompt_row)) = active_prompt_info {
            if viewport_height >= 12 && full_w >= 24 {
                let raw_lines: Vec<&str> = prompt_text.lines().collect();
                let mut wrapped_prompt_lines: Vec<String> = Vec::new();
                let usable_w = full_w.saturating_sub(4).max(12);
                for raw in &raw_lines {
                    if raw.is_empty() {
                        wrapped_prompt_lines.push(String::new());
                        continue;
                    }
                    for chunk in Self::split_line_into_chunks(raw, usable_w) {
                        wrapped_prompt_lines.push(chunk);
                    }
                }
                let max_sticky_lines = 2usize;
                let total_p_lines = wrapped_prompt_lines.len();
                let shown_p_lines = total_p_lines.min(max_sticky_lines);
                let hidden_p_lines = total_p_lines.saturating_sub(shown_p_lines);

                let sticky_base = Style::default().bg(theme.bg_primary).fg(theme.text_primary);
                let mut sticky_rows: Vec<Line<'static>> = Vec::with_capacity(4);

                for (idx, pline) in wrapped_prompt_lines.iter().take(shown_p_lines).enumerate() {
                    let prefix_str = if idx == 0 { "› " } else { "  " };
                    let prefix_style = if idx == 0 {
                        Style::default()
                            .bg(theme.bg_primary)
                            .fg(theme.brand_accent)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        sticky_base
                    };
                    let mut row_spans = vec![
                        Span::styled(prefix_str, prefix_style),
                        Span::styled(
                            pline.clone(),
                            sticky_base.add_modifier(if idx == 0 {
                                Modifier::BOLD
                            } else {
                                Modifier::empty()
                            }),
                        ),
                    ];
                    let is_last_shown = idx + 1 == shown_p_lines;
                    if is_last_shown && hidden_p_lines > 0 {
                        let suffix = format!(" ... +{} lines", hidden_p_lines);
                        let cur_len = 2
                            + UnicodeWidthStr::width(pline.as_str())
                            + UnicodeWidthStr::width(suffix.as_str());
                        if cur_len <= full_w {
                            row_spans.push(Span::styled(
                                suffix,
                                Style::default()
                                    .bg(theme.bg_primary)
                                    .fg(theme.muted)
                                    .add_modifier(Modifier::ITALIC),
                            ));
                        }
                    }
                    let used_w: usize = row_spans
                        .iter()
                        .map(|s| UnicodeWidthStr::width(s.content.as_ref()))
                        .sum();
                    if used_w < full_w {
                        row_spans.push(Span::styled(" ".repeat(full_w - used_w), sticky_base));
                    }
                    let s_line = Line::from(row_spans);
                    if Self::line_contains_wide_emoji(&s_line) {
                        viewport_has_emoji = true;
                    }
                    sticky_rows.push(s_line);
                }

                let div_w = 60usize.min(full_w.saturating_sub(1));
                let rem_w = full_w.saturating_sub(1 + div_w);
                sticky_rows.push(Line::from(vec![
                    Span::styled(" ", sticky_base),
                    Span::styled(
                        "─".repeat(div_w),
                        Style::default().bg(theme.bg_primary).fg(theme.border),
                    ),
                    Span::styled(" ".repeat(rem_w), sticky_base),
                ]));

                let max_allowed_rows = next_prompt_row
                    .map(|nr| nr.saturating_sub(scroll_row))
                    .unwrap_or(sticky_rows.len());

                for (r_idx, s_line) in sticky_rows.into_iter().take(max_allowed_rows).enumerate() {
                    if r_idx < visible_lines.len() {
                        visible_lines[r_idx] = s_line;
                    }
                }
            }
        }

        // ── Floating `↓ Back to bottom · End` Pill When Scrolled Up ─────────
        if !self.auto_scroll.get()
            && scroll < max_scroll
            && max_scroll > 0
            && !self.selection.is_selecting.get()
            && !self.has_selection()
            && viewport_height >= 14
            && full_w >= 32
            && !visible_lines.is_empty()
        {
            let pill_text = " ↓ Back to bottom · End ";
            let pill_style = Style::default()
                .bg(theme.bg_elevated)
                .fg(theme.brand_accent)
                .add_modifier(Modifier::BOLD);
            let last_row_idx = visible_lines.len() - 1;
            let overlaid = Self::overlay_centered_pill(
                &visible_lines[last_row_idx],
                pill_text,
                pill_style,
                clear_style,
                full_w,
            );
            visible_lines[last_row_idx] = overlaid;
        }

        self.last_frame_had_wide_emoji.set(viewport_has_emoji);

        let block = Block::default()
            .borders(Borders::NONE)
            .style(Style::default().bg(theme.bg_primary));

        let paragraph = Paragraph::new(visible_lines).block(block);

        frame.render_widget(paragraph, area);
    }
}

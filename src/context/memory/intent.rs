//! Intent Anchoring & Execution Tracking (Phase 128).
//!
//! Provides data structures (`RequirementStatus`, `RequirementItem`, `IntentLedger`)
//! and persistence methods for goal anchoring and execution tracking.
#![allow(dead_code)]

use serde::{Deserialize, Serialize};

/// Execution status of an individual requirement or goal item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RequirementStatus {
    #[default]
    Pending,
    InProgress,
    Completed,
    Blocked,
    Skipped,
}

/// An individual granular requirement or sub-task tracked within an `IntentLedger`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RequirementItem {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub status: RequirementStatus,
    #[serde(default)]
    pub related_files: Vec<String>,
    #[serde(default)]
    pub created_turn: usize,
    #[serde(default)]
    pub updated_turn: usize,
}

/// Living execution ledger that maintains the root goal anchor, active items, and drift telemetry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct IntentLedger {
    pub root_objective: String,
    pub items: Vec<RequirementItem>,
    pub active_item_id: Option<String>,
    pub consecutive_turns_without_progress: usize,
}

impl IntentLedger {
    /// Creates a new `IntentLedger` initialized with the given root objective.
    pub fn new(root_objective: &str) -> Self {
        Self {
            root_objective: root_objective.to_string(),
            items: Vec::new(),
            active_item_id: None,
            consecutive_turns_without_progress: 0,
        }
    }

    /// Appends a new requirement item to the ledger and returns its generated ID.
    pub fn add_item(
        &mut self,
        title: &str,
        description: Option<&str>,
        related_files: Vec<String>,
    ) -> String {
        let id = format!("req-{}", uuid::Uuid::new_v4().simple());
        let item = RequirementItem {
            id: id.clone(),
            title: title.to_string(),
            description: description.map(|d| d.to_string()),
            status: RequirementStatus::Pending,
            related_files,
            created_turn: 0,
            updated_turn: 0,
        };
        self.items.push(item);
        id
    }

    /// Updates the status of an existing requirement item by ID.
    ///
    /// - If moved to `InProgress`, sets `active_item_id` to this item and resets drift counter
    ///   if transitioning from `Pending`.
    /// - If moved to `Completed`, clears `active_item_id` (if matching) and resets drift counter
    ///   if transitioning from a non-`Completed` state.
    /// - If moved to any other status, clears `active_item_id` if it was pointing to this item.
    ///
    /// Returns `true` if the item was found and updated, or `false` otherwise.
    pub fn set_status(&mut self, id: &str, status: RequirementStatus) -> bool {
        let Some(item) = self.items.iter_mut().find(|item| item.id == id) else {
            return false;
        };
        let old_status = item.status;
        item.status = status;
        match status {
            RequirementStatus::InProgress => {
                self.active_item_id = Some(id.to_string());
                if old_status == RequirementStatus::Pending {
                    self.consecutive_turns_without_progress = 0;
                }
            }
            RequirementStatus::Completed => {
                if self.active_item_id.as_deref() == Some(id) {
                    self.active_item_id = None;
                }
                if old_status != RequirementStatus::Completed {
                    self.consecutive_turns_without_progress = 0;
                }
            }
            RequirementStatus::Pending
            | RequirementStatus::Blocked
            | RequirementStatus::Skipped => {
                if self.active_item_id.as_deref() == Some(id) {
                    self.active_item_id = None;
                }
            }
        }
        true
    }

    /// Retrieves an immutable reference to a requirement item by ID.
    pub fn get_item(&self, id: &str) -> Option<&RequirementItem> {
        self.items.iter().find(|item| item.id == id)
    }

    /// Retrieves a mutable reference to a requirement item by ID.
    pub fn get_item_mut(&mut self, id: &str) -> Option<&mut RequirementItem> {
        self.items.iter_mut().find(|item| item.id == id)
    }

    /// Returns the number of completed requirement items.
    pub fn completed_count(&self) -> usize {
        self.items
            .iter()
            .filter(|item| item.status == RequirementStatus::Completed)
            .count()
    }

    /// Returns the total number of tracked requirement items.
    pub fn total_count(&self) -> usize {
        self.items.len()
    }

    /// Checks if consecutive turns without progress exceed the drift threshold.
    /// If so, returns a high-priority course-correction reminder string.
    pub fn check_drift(&self, threshold: usize) -> Option<String> {
        if self.consecutive_turns_without_progress >= threshold
            && self.total_count() > self.completed_count()
        {
            let active_title = self
                .active_item_id
                .as_deref()
                .and_then(|id| self.get_item(id))
                .map(|item| item.title.as_str())
                .or_else(|| {
                    self.items
                        .iter()
                        .find(|item| item.status == RequirementStatus::InProgress)
                        .map(|item| item.title.as_str())
                })
                .or_else(|| {
                    self.items
                        .iter()
                        .find(|item| item.status == RequirementStatus::Pending)
                        .map(|item| item.title.as_str())
                })
                .unwrap_or("None (select next requirement)");

            Some(format!(
                "⚠️ Task Drift Warning: {} turns have passed without requirement progress (completed {}/{}). Active task: {}. Do not get distracted by tangential edits; focus on completing remaining requirements.",
                self.consecutive_turns_without_progress,
                self.completed_count(),
                self.total_count(),
                active_title
            ))
        } else {
            None
        }
    }

    /// Renders the immutable root goal anchor and the living execution ledger
    /// into a compact, token-efficient XML prompt block for LLM context injection.
    pub fn to_prompt_block(&self) -> String {
        if self.root_objective.trim().is_empty() && self.items.is_empty() {
            return String::new();
        }

        let mut block = String::with_capacity(512);
        if !self.root_objective.trim().is_empty() {
            block.push_str("  <goal_anchor>\n");
            block.push_str(&format!(
                "    <root_objective>{}</root_objective>\n",
                self.root_objective.trim()
            ));
            block.push_str("  </goal_anchor>\n");
        }

        block.push_str(&format!(
            "  <execution_ledger progress=\"{}/{} completed\">\n",
            self.completed_count(),
            self.total_count()
        ));
        for item in &self.items {
            let marker = match item.status {
                RequirementStatus::Completed => "[x]",
                RequirementStatus::InProgress => "[-]",
                RequirementStatus::Blocked => "[!]",
                RequirementStatus::Skipped => "[s]",
                RequirementStatus::Pending => "[ ]",
            };
            block.push_str(&format!("    {} {}\n", marker, item.title.trim()));
        }
        block.push_str("  </execution_ledger>\n");

        block
    }

    /// Atomically persists the `IntentLedger` to disk via a temporary file and rename.
    pub fn save_to_disk(&self, path: &std::path::Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        let tmp_path = match path.file_name() {
            Some(fname) => {
                let mut fname_os = fname.to_os_string();
                fname_os.push(format!(
                    ".tmp.{}.{}",
                    std::process::id(),
                    uuid::Uuid::new_v4().simple()
                ));
                path.with_file_name(fname_os)
            }
            None => path.with_extension(format!(
                "tmp.{}.{}",
                std::process::id(),
                uuid::Uuid::new_v4().simple()
            )),
        };

        std::fs::write(&tmp_path, json)?;
        if let Err(rename_err) = std::fs::rename(&tmp_path, path) {
            if let Err(e) = std::fs::remove_file(&tmp_path) {
                tracing::warn!(
                    error = %e,
                    path = %tmp_path.display(),
                    "Failed to clean up temporary intent ledger file"
                );
            }
            return Err(rename_err);
        }
        Ok(())
    }

    /// Loads and deserializes an `IntentLedger` from disk.
    pub fn load_from_disk(path: &std::path::Path) -> std::io::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let ledger = serde_json::from_str::<Self>(&content)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        Ok(ledger)
    }

    /// Dynamically extracts the root objective and actionable requirement items
    /// from a freeform user prompt across diverse formatting styles (headers, checklists, numbered lists, bullet points).
    pub fn from_prompt(prompt: &str, max_items: usize) -> Self {
        let cap = if max_items == 0 { 32 } else { max_items };
        let raw_lines: Vec<&str> = prompt.lines().collect();
        let first_non_empty_idx = raw_lines.iter().position(|l| !l.trim().is_empty());

        let (root_objective, start_idx) = match first_non_empty_idx {
            Some(idx) => {
                let first_line_trimmed = raw_lines[idx].trim();
                if strip_checkbox_prefix(first_line_trimmed).is_some() {
                    ("Complete prompt checklist".to_string(), idx)
                } else if parse_numbered_item(first_line_trimmed).is_some()
                    || strip_bullet_prefix(first_line_trimmed).is_some()
                {
                    ("Complete prompt tasks".to_string(), idx)
                } else {
                    let cleaned = clean_root_objective(raw_lines[idx]);
                    let obj = if cleaned.is_empty() {
                        "General Assistance".to_string()
                    } else {
                        cleaned
                    };
                    (obj, idx + 1)
                }
            }
            None => ("General Assistance".to_string(), 0),
        };

        let mut ledger = Self::new(&root_objective);
        let mut draft: Option<DraftItem> = None;
        let mut collected_items: Vec<RequirementItem> = Vec::new();

        for line in &raw_lines[start_idx..] {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            // 1. Checkbox item
            if let Some((rest, is_checked)) = strip_checkbox_prefix(trimmed) {
                commit_draft(draft.take(), &mut collected_items, cap);
                let status = if is_checked {
                    RequirementStatus::Completed
                } else {
                    RequirementStatus::Pending
                };
                let (title, desc_line) = parse_title_and_desc(rest);
                let mut desc_lines = Vec::new();
                if let Some(d) = desc_line {
                    desc_lines.push(d);
                }
                draft = Some(DraftItem {
                    title: title.to_string(),
                    description_lines: desc_lines,
                    status,
                    from_header: false,
                });
                continue;
            }

            // 2. Header line
            if let Some((_level, header_text)) = parse_header(trimmed) {
                commit_draft(draft.take(), &mut collected_items, cap);
                if is_container_header(header_text) {
                    draft = None;
                } else {
                    let cleaned_title = strip_leading_enumeration(header_text)
                        .trim_matches(|c| c == '*' || c == '_' || c == '`')
                        .trim();
                    draft = Some(DraftItem {
                        title: cleaned_title.to_string(),
                        description_lines: Vec::new(),
                        status: RequirementStatus::Pending,
                        from_header: true,
                    });
                }
                continue;
            }

            // 3. Numbered list item
            if let Some(rest) = parse_numbered_item(trimmed) {
                let is_indented = line.starts_with("  ") || line.starts_with('\t');
                if is_indented && draft.is_some() {
                    if let Some(ref mut d) = draft {
                        d.description_lines.push(trimmed.to_string());
                    }
                    continue;
                }

                commit_draft(draft.take(), &mut collected_items, cap);
                let (title, desc_line) = parse_title_and_desc(rest);
                let mut desc_lines = Vec::new();
                if let Some(d) = desc_line {
                    desc_lines.push(d);
                }
                draft = Some(DraftItem {
                    title: title.to_string(),
                    description_lines: desc_lines,
                    status: RequirementStatus::Pending,
                    from_header: false,
                });
                continue;
            }

            // 4. Bullet list item
            if let Some(rest) = strip_bullet_prefix(trimmed) {
                let is_indented = line.starts_with("  ") || line.starts_with('\t');
                if (is_indented || draft.as_ref().is_some_and(|d| d.from_header)) && draft.is_some()
                {
                    if let Some(ref mut d) = draft {
                        d.description_lines.push(trimmed.to_string());
                    }
                    continue;
                }

                commit_draft(draft.take(), &mut collected_items, cap);
                let (title, desc_line) = parse_title_and_desc(rest);
                let mut desc_lines = Vec::new();
                if let Some(d) = desc_line {
                    desc_lines.push(d);
                }
                draft = Some(DraftItem {
                    title: title.to_string(),
                    description_lines: desc_lines,
                    status: RequirementStatus::Pending,
                    from_header: false,
                });
                continue;
            }

            // 5. Plain / description line
            if let Some(ref mut d) = draft {
                d.description_lines.push(trimmed.to_string());
            }
        }

        commit_draft(draft.take(), &mut collected_items, cap);

        if collected_items.is_empty() {
            let title = if root_objective == "General Assistance" {
                "General Assistance".to_string()
            } else {
                format!("Complete objective: {}", root_objective)
            };
            let related_files = extract_related_files(&root_objective);
            ledger.items.push(RequirementItem {
                id: format!("req-{}", uuid::Uuid::new_v4().simple()),
                title,
                description: None,
                status: RequirementStatus::Pending,
                related_files,
                created_turn: 0,
                updated_turn: 0,
            });
        } else {
            ledger.items = collected_items;
        }

        ledger
    }
}

struct DraftItem {
    title: String,
    description_lines: Vec<String>,
    status: RequirementStatus,
    from_header: bool,
}

fn commit_draft(draft: Option<DraftItem>, collected: &mut Vec<RequirementItem>, max_items: usize) {
    if let Some(item) = draft {
        let title = item.title.trim().to_string();
        if title.is_empty() {
            return;
        }

        let desc_text = if item.description_lines.is_empty() {
            None
        } else {
            let joined = item.description_lines.join("\n").trim().to_string();
            if joined.is_empty() {
                None
            } else {
                Some(joined)
            }
        };

        let mut related_files = extract_related_files(&title);
        if let Some(ref desc) = desc_text {
            for f in extract_related_files(desc) {
                if !related_files.contains(&f) {
                    related_files.push(f);
                }
            }
        }

        let mut is_dup = false;
        for existing in collected.iter_mut() {
            if is_duplicate_title(&existing.title, &title) {
                is_dup = true;
                if existing.description.is_none() && desc_text.is_some() {
                    existing.description = desc_text.clone();
                }
                for f in &related_files {
                    if !existing.related_files.contains(f) {
                        existing.related_files.push(f.clone());
                    }
                }
                break;
            }
        }

        if !is_dup && collected.len() < max_items {
            let req = RequirementItem {
                id: format!("req-{}", uuid::Uuid::new_v4().simple()),
                title,
                description: desc_text,
                status: item.status,
                related_files,
                created_turn: 0,
                updated_turn: 0,
            };
            collected.push(req);
        }
    }
}

fn clean_root_objective(line: &str) -> String {
    let trimmed = line.trim();
    let without_hash = trimmed.trim_start_matches('#').trim();
    let mut cleaned = without_hash;
    for prefix in &["objective:", "goal:", "task:", "project:", "title:"] {
        if cleaned.len() >= prefix.len()
            && cleaned.is_char_boundary(prefix.len())
            && cleaned[..prefix.len()].eq_ignore_ascii_case(prefix)
        {
            cleaned = cleaned[prefix.len()..].trim();
            break;
        }
    }
    let cleaned = cleaned
        .trim_matches(|c| c == '*' || c == '_' || c == '`')
        .trim();
    cleaned.to_string()
}

fn strip_leading_enumeration(s: &str) -> &str {
    let trimmed = s.trim_start();
    let chars = trimmed.char_indices();
    let mut seen_digit = false;
    for (i, c) in chars {
        if c.is_ascii_digit() {
            seen_digit = true;
        } else if seen_digit && (c == '.' || c == ')' || c == ':' || c == '-') {
            let rest = &trimmed[i + c.len_utf8()..];
            return rest.trim_start();
        } else {
            break;
        }
    }
    trimmed
}

fn strip_checkbox_prefix(s: &str) -> Option<(&str, bool)> {
    let trimmed = s.trim_start();
    for prefix in &["- [ ]", "* [ ]", "+ [ ]", "- []", "* []", "+ []"] {
        if let Some(rest) = trimmed.strip_prefix(prefix) {
            return Some((rest.trim_start(), false));
        }
    }
    for prefix in &["- [x]", "* [x]", "+ [x]", "- [X]", "* [X]", "+ [X]"] {
        if let Some(rest) = trimmed.strip_prefix(prefix) {
            return Some((rest.trim_start(), true));
        }
    }
    None
}

fn strip_bullet_prefix(s: &str) -> Option<&str> {
    let trimmed = s.trim_start();
    if trimmed.starts_with("---") || trimmed.starts_with("***") || trimmed.starts_with("___") {
        return None;
    }
    for prefix in &["- ", "* ", "+ "] {
        if let Some(rest) = trimmed.strip_prefix(prefix) {
            return Some(rest.trim_start());
        }
    }
    None
}

fn parse_header(s: &str) -> Option<(usize, &str)> {
    let trimmed = s.trim_start();
    if !trimmed.starts_with('#') {
        return None;
    }
    let level = trimmed.chars().take_while(|c| *c == '#').count();
    let rest = &trimmed[level..];
    if rest.starts_with(' ') || rest.starts_with('\t') {
        Some((level, rest.trim()))
    } else {
        None
    }
}

fn parse_numbered_item(s: &str) -> Option<&str> {
    let trimmed = s.trim_start();
    let chars = trimmed.char_indices();
    let mut seen_digit = false;
    for (i, c) in chars {
        if c.is_ascii_digit() {
            seen_digit = true;
        } else if seen_digit && (c == '.' || c == ')') {
            let rest = &trimmed[i + c.len_utf8()..];
            if rest.starts_with(' ') || rest.starts_with('\t') {
                return Some(rest.trim_start());
            } else {
                return None;
            }
        } else {
            return None;
        }
    }
    None
}

fn is_container_header(title: &str) -> bool {
    let norm = title.trim().trim_end_matches(':').to_lowercase();
    let stripped = strip_leading_enumeration(&norm).to_lowercase();
    matches!(
        stripped.as_str(),
        "pages"
            | "page"
            | "requirements"
            | "requirement"
            | "acceptance criteria"
            | "acceptance test criteria"
            | "criteria"
            | "tasks"
            | "task list"
            | "features"
            | "key features"
            | "deliverables"
            | "specifications"
            | "specs"
            | "overview"
            | "scope"
            | "goals"
            | "milestones"
            | "checklist"
            | "todo"
            | "to-do"
            | "instructions"
            | "details"
            | "notes"
            | "subtasks"
            | "sub-tasks"
    )
}

fn parse_title_and_desc(s: &str) -> (&str, Option<String>) {
    let clean = strip_leading_enumeration(s)
        .trim_matches(|c| c == '*' || c == '_' || c == '`')
        .trim();

    if let Some((left, right)) = clean.split_once(": ") {
        let left_trim = left.trim();
        let right_trim = right.trim();
        if left_trim.len() >= 3
            && left_trim.len() <= 40
            && !left_trim.contains('/')
            && !left_trim.ends_with(".rs")
            && !left_trim.ends_with(".ts")
        {
            return (left_trim, Some(right_trim.to_string()));
        }
    }
    (clean, None)
}

fn extract_related_files(text: &str) -> Vec<String> {
    let mut files = Vec::new();
    let known_extensions = [
        ".rs", ".ts", ".tsx", ".js", ".jsx", ".py", ".json", ".toml", ".css", ".html", ".md",
        ".yaml", ".yml", ".sql", ".sh", ".go", ".c", ".cpp", ".h",
    ];

    let is_bracket_or_quote = |c: char| {
        matches!(
            c,
            '\'' | '"' | '`' | '(' | ')' | '[' | ']' | '{' | '}' | '<' | '>'
        )
    };

    for word in text.split_whitespace() {
        let mut cleaned = word;

        loop {
            let prev = cleaned;
            cleaned = cleaned.trim_matches(is_bracket_or_quote);

            // Strip trailing sentence punctuation while preserving directory paths like "." or ".."
            for punct in ['!', '?', ',', ';', ':'] {
                if cleaned.len() > 1 {
                    if let Some(stripped) = cleaned.strip_suffix(punct) {
                        cleaned = stripped;
                    }
                }
            }

            if cleaned.len() > 1 && cleaned != ".." {
                if let Some(stripped) = cleaned.strip_suffix('.') {
                    cleaned = stripped;
                }
            }

            cleaned = cleaned.trim_matches(is_bracket_or_quote);

            if cleaned == prev {
                break;
            }
        }

        if cleaned.is_empty() || cleaned.starts_with("http://") || cleaned.starts_with("https://") {
            continue;
        }

        let has_known_ext = known_extensions.iter().any(|ext| {
            if let Some(pos) = cleaned.rfind(ext) {
                pos + ext.len() == cleaned.len() && pos > 0
            } else {
                false
            }
        });

        let has_path_prefix = cleaned == "."
            || cleaned == ".."
            || cleaned.starts_with("src/")
            || cleaned.starts_with("tests/")
            || cleaned.starts_with("docs/")
            || cleaned.starts_with("./")
            || cleaned.starts_with("../")
            || (cleaned.starts_with('/') && cleaned.contains('.') && cleaned.len() > 2);

        if (has_known_ext || has_path_prefix) && is_valid_path_candidate(cleaned) {
            let path_str = cleaned.to_string();
            if !files.contains(&path_str) {
                files.push(path_str);
            }
        }
    }
    files
}

fn is_valid_path_candidate(s: &str) -> bool {
    if s == "." || s == ".." {
        return true;
    }
    if s.chars().all(|c| c.is_ascii_digit() || c == '.') {
        return false;
    }
    s.chars()
        .all(|c| c.is_alphanumeric() || c == '/' || c == '.' || c == '_' || c == '-')
}

fn is_duplicate_title(a: &str, b: &str) -> bool {
    let clean_a = strip_leading_enumeration(a.trim())
        .trim_matches(|c| c == '*' || c == '_' || c == '`')
        .trim();
    let clean_b = strip_leading_enumeration(b.trim())
        .trim_matches(|c| c == '*' || c == '_' || c == '`')
        .trim();
    clean_a.eq_ignore_ascii_case(clean_b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_intent_ledger_lifecycle() {
        let mut ledger = IntentLedger::new("Build AgentBench sandbox website");
        assert_eq!(ledger.root_objective, "Build AgentBench sandbox website");
        assert_eq!(ledger.items.len(), 0);

        let id = ledger.add_item(
            "Dashboard with metrics cards",
            Some("Render revenue and activity"),
            vec!["src/pages/Dashboard.tsx".to_string()],
        );
        assert_eq!(ledger.items.len(), 1);
        assert_eq!(ledger.items[0].status, RequirementStatus::Pending);

        assert!(ledger.get_item(&id).is_some());
        assert_eq!(
            ledger.get_item(&id).unwrap().title,
            "Dashboard with metrics cards"
        );

        ledger.set_status(&id, RequirementStatus::InProgress);
        assert_eq!(ledger.items[0].status, RequirementStatus::InProgress);
        assert_eq!(ledger.active_item_id, Some(id.clone()));

        ledger.set_status(&id, RequirementStatus::Completed);
        assert_eq!(ledger.items[0].status, RequirementStatus::Completed);
        assert_eq!(ledger.active_item_id, None);
    }

    #[test]
    fn test_intent_ledger_status_and_drift_reset() {
        let mut ledger = IntentLedger::new("Test objective");
        let id1 = ledger.add_item("Task 1", None, vec![]);
        let id2 = ledger.add_item("Task 2", None, vec![]);

        ledger.consecutive_turns_without_progress = 5;

        // InProgress resets drift counter
        assert!(ledger.set_status(&id1, RequirementStatus::InProgress));
        assert_eq!(ledger.consecutive_turns_without_progress, 0);
        assert_eq!(ledger.active_item_id, Some(id1.clone()));

        // Blocked clears active_item_id without resetting drift counter
        ledger.consecutive_turns_without_progress = 3;
        assert!(ledger.set_status(&id1, RequirementStatus::Blocked));
        assert_eq!(ledger.consecutive_turns_without_progress, 3);
        assert_eq!(ledger.active_item_id, None);

        // Setting id2 to InProgress sets active_item_id to id2
        assert!(ledger.set_status(&id2, RequirementStatus::InProgress));
        assert_eq!(ledger.active_item_id, Some(id2.clone()));
        assert_eq!(ledger.consecutive_turns_without_progress, 0);

        // Non-existent id returns false
        assert!(!ledger.set_status("non-existent-id", RequirementStatus::Completed));
        assert!(ledger.get_item("non-existent-id").is_none());
    }

    #[test]
    fn test_intent_ledger_persistence_roundtrip() {
        let temp_dir = std::env::temp_dir().join(format!("intent_test_{}", uuid::Uuid::new_v4()));
        let file_path = temp_dir.join(".minicode").join("intent_anchor.json");

        let mut ledger = IntentLedger::new("Persistent Root Objective");
        let id1 = ledger.add_item(
            "Feature A",
            Some("Description A"),
            vec!["src/a.rs".to_string()],
        );
        ledger.set_status(&id1, RequirementStatus::InProgress);

        ledger
            .save_to_disk(&file_path)
            .expect("Failed to save ledger to disk");
        assert!(file_path.exists());

        let loaded =
            IntentLedger::load_from_disk(&file_path).expect("Failed to load ledger from disk");
        assert_eq!(loaded, ledger);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_intent_config_defaults() {
        let config = crate::config::IntentConfig::default();
        assert!(config.enabled);
        assert_eq!(
            config.persistence_file,
            crate::constants::DEFAULT_INTENT_PERSISTENCE_FILE
        );
        assert_eq!(
            config.drift_warning_turns,
            crate::constants::DEFAULT_INTENT_DRIFT_WARNING_TURNS
        );
        assert!(config.auto_extract);
        assert_eq!(
            config.max_ledger_items,
            crate::constants::DEFAULT_INTENT_MAX_ITEMS
        );
    }

    #[test]
    fn test_requirement_status_serde() {
        let statuses = vec![
            (RequirementStatus::Pending, "\"pending\""),
            (RequirementStatus::InProgress, "\"in_progress\""),
            (RequirementStatus::Completed, "\"completed\""),
            (RequirementStatus::Blocked, "\"blocked\""),
            (RequirementStatus::Skipped, "\"skipped\""),
        ];

        for (status, json_str) in statuses {
            let serialized = serde_json::to_string(&status).expect("Serialization failed");
            assert_eq!(serialized, json_str);
            let deserialized: RequirementStatus =
                serde_json::from_str(json_str).expect("Deserialization failed");
            assert_eq!(deserialized, status);
        }
    }

    #[test]
    fn test_dynamic_prompt_requirement_extraction() {
        let complex_prompt = r#"
Build a modern, realistic web application called AgentBench.
### Pages
#### 1. Dashboard
Show total customers, open tickets, monthly revenue.
#### 2. Customers
Customer list with search and filter.
#### 3. Tickets
Support ticket queue.
"#;
        let ledger = IntentLedger::from_prompt(complex_prompt, 16);
        assert_eq!(
            ledger.root_objective,
            "Build a modern, realistic web application called AgentBench."
        );
        assert!(ledger.items.len() >= 3);
        assert!(ledger.items.iter().any(|i| i.title.contains("Dashboard")));
        assert!(ledger.items.iter().any(|i| i.title.contains("Customers")));
        assert!(ledger.items.iter().any(|i| i.title.contains("Tickets")));

        let dash = ledger
            .items
            .iter()
            .find(|i| i.title.contains("Dashboard"))
            .unwrap();
        assert!(dash.description.is_some());
        assert!(dash
            .description
            .as_ref()
            .unwrap()
            .contains("total customers"));
    }

    #[test]
    fn test_prompt_checkbox_parsing_and_status() {
        let prompt = r#"
# Database & API Setup
- [ ] Setup database schema in src/db/schema.rs
- [x] Configure auth routes in src/api/auth.ts
- [ ] Add rate limiting
"#;
        let ledger = IntentLedger::from_prompt(prompt, 16);
        assert_eq!(ledger.root_objective, "Database & API Setup");
        assert_eq!(ledger.items.len(), 3);

        let db_item = &ledger.items[0];
        assert_eq!(db_item.status, RequirementStatus::Pending);
        assert!(db_item
            .related_files
            .contains(&"src/db/schema.rs".to_string()));

        let auth_item = &ledger.items[1];
        assert_eq!(auth_item.status, RequirementStatus::Completed);
        assert!(auth_item
            .related_files
            .contains(&"src/api/auth.ts".to_string()));

        let rate_item = &ledger.items[2];
        assert_eq!(rate_item.status, RequirementStatus::Pending);
    }

    #[test]
    fn test_prompt_numbered_and_bullet_lists() {
        let prompt = r#"
Refactor core architecture
1. Create models in src/models.rs
2. Write tests in tests/model_test.rs
* Build UI in src/ui/dashboard.tsx
"#;
        let ledger = IntentLedger::from_prompt(prompt, 16);
        assert_eq!(ledger.root_objective, "Refactor core architecture");
        assert_eq!(ledger.items.len(), 3);

        assert!(ledger.items[0].title.contains("Create models"));
        assert!(ledger.items[0]
            .related_files
            .contains(&"src/models.rs".to_string()));

        assert!(ledger.items[1].title.contains("Write tests"));
        assert!(ledger.items[1]
            .related_files
            .contains(&"tests/model_test.rs".to_string()));

        assert!(ledger.items[2].title.contains("Build UI"));
        assert!(ledger.items[2]
            .related_files
            .contains(&"src/ui/dashboard.tsx".to_string()));
    }

    #[test]
    fn test_single_sentence_and_empty_prompts() {
        // Single sentence
        let prompt = "Refactor error handling to use thiserror in src/error.rs";
        let ledger = IntentLedger::from_prompt(prompt, 16);
        assert_eq!(
            ledger.root_objective,
            "Refactor error handling to use thiserror in src/error.rs"
        );
        assert_eq!(ledger.items.len(), 1);
        assert_eq!(
            ledger.items[0].title,
            "Complete objective: Refactor error handling to use thiserror in src/error.rs"
        );
        assert!(ledger.items[0]
            .related_files
            .contains(&"src/error.rs".to_string()));

        // Empty prompt
        let empty_ledger = IntentLedger::from_prompt("   \n\t  ", 16);
        assert_eq!(empty_ledger.root_objective, "General Assistance");
        assert_eq!(empty_ledger.items.len(), 1);
        assert_eq!(empty_ledger.items[0].title, "General Assistance");
    }

    #[test]
    fn test_deduplication_and_capping() {
        let prompt = r#"
# System Enhancements
- Dashboard
- 1. Dashboard
- dashboard
- Tasks
- Users
- Billing
- Settings
"#;
        let ledger = IntentLedger::from_prompt(prompt, 3);
        // Dashboard, 1. Dashboard, dashboard should deduplicate into 1 Dashboard item
        assert!(ledger.items.len() <= 3);
        assert_eq!(
            ledger
                .items
                .iter()
                .filter(|i| i.title.to_lowercase().contains("dashboard"))
                .count(),
            1
        );
    }

    #[test]
    fn test_utf8_char_boundary_emojis_and_international() {
        let prompt = "💡 Build website with 🚀\n- [ ] 🦀 Rust backend in src/main.rs\n- [ ] 🎨 Frontend in src/ui.rs";
        let ledger = IntentLedger::from_prompt(prompt, 16);
        assert_eq!(ledger.root_objective, "💡 Build website with 🚀");
        assert_eq!(ledger.items.len(), 2);
        assert!(ledger.items[0].title.contains("🦀 Rust backend"));
        assert!(ledger.items[0]
            .related_files
            .contains(&"src/main.rs".to_string()));
        assert!(ledger.items[1].title.contains("🎨 Frontend"));

        // Direct UTF-8 multi-byte slicing tests
        assert_eq!(
            clean_root_objective("💡 Build website with 🚀"),
            "💡 Build website with 🚀"
        );
        assert_eq!(
            clean_root_objective("Objective: 💡 Build website with 🚀"),
            "💡 Build website with 🚀"
        );
        assert_eq!(clean_root_objective("🦀🦀🦀"), "🦀🦀🦀");
        assert_eq!(
            clean_root_objective("Task: 🚀 Ship Phase 2"),
            "🚀 Ship Phase 2"
        );
        assert_eq!(clean_root_objective("你好世界"), "你好世界");
    }

    #[test]
    fn test_checklist_first_prompt_preserves_first_item() {
        let prompt = "- [ ] First task\n- [ ] Second task";
        let ledger = IntentLedger::from_prompt(prompt, 16);
        assert_eq!(ledger.root_objective, "Complete prompt checklist");
        assert_eq!(ledger.items.len(), 2);
        assert_eq!(ledger.items[0].title, "First task");
        assert_eq!(ledger.items[0].status, RequirementStatus::Pending);
        assert_eq!(ledger.items[1].title, "Second task");
        assert_eq!(ledger.items[1].status, RequirementStatus::Pending);
    }

    #[test]
    fn test_extract_related_files_trailing_punctuation() {
        let text = "Check src/main.rs. Also see tests/app_test.rs! Did you update docs/spec.md? Or ../ and .";
        let files = extract_related_files(text);
        assert!(files.contains(&"src/main.rs".to_string()));
        assert!(files.contains(&"tests/app_test.rs".to_string()));
        assert!(files.contains(&"docs/spec.md".to_string()));
        assert!(files.contains(&"../".to_string()));
        assert!(files.contains(&".".to_string()));

        let single = "Check src/main.rs.";
        let single_files = extract_related_files(single);
        assert_eq!(single_files, vec!["src/main.rs".to_string()]);
    }

    #[test]
    fn test_no_aggressive_substring_deduplication() {
        let prompt = "1. User Service\n2. User Service Tests";
        let ledger = IntentLedger::from_prompt(prompt, 16);
        assert_eq!(ledger.items.len(), 2);
        assert_eq!(ledger.items[0].title, "User Service");
        assert_eq!(ledger.items[1].title, "User Service Tests");
    }

    #[test]
    fn test_get_item_mut_and_drift_reset_semantics() {
        let mut ledger = IntentLedger::new("Test drift semantics");
        let id1 = ledger.add_item("Task 1", None, vec![]);
        let id2 = ledger.add_item("Task 2", None, vec![]);

        // get_item_mut
        let item_mut = ledger.get_item_mut(&id1).expect("Item should exist");
        item_mut.title = "Task 1 Modified".to_string();
        assert_eq!(ledger.get_item(&id1).unwrap().title, "Task 1 Modified");

        ledger.consecutive_turns_without_progress = 5;

        // Pending -> InProgress: resets drift
        assert!(ledger.set_status(&id1, RequirementStatus::InProgress));
        assert_eq!(ledger.consecutive_turns_without_progress, 0);

        // InProgress -> InProgress: should NOT reset drift
        ledger.consecutive_turns_without_progress = 3;
        assert!(ledger.set_status(&id1, RequirementStatus::InProgress));
        assert_eq!(ledger.consecutive_turns_without_progress, 3);

        // InProgress -> Completed: resets drift
        assert!(ledger.set_status(&id1, RequirementStatus::Completed));
        assert_eq!(ledger.consecutive_turns_without_progress, 0);

        // Completed -> Completed: should NOT reset drift
        ledger.consecutive_turns_without_progress = 4;
        assert!(ledger.set_status(&id1, RequirementStatus::Completed));
        assert_eq!(ledger.consecutive_turns_without_progress, 4);

        // id2: Pending -> Completed: resets drift
        ledger.consecutive_turns_without_progress = 2;
        assert!(ledger.set_status(&id2, RequirementStatus::Completed));
        assert_eq!(ledger.consecutive_turns_without_progress, 0);
    }

    #[test]
    fn test_intent_ledger_prompt_block_and_drift() {
        let mut ledger = IntentLedger::new("Implement authentication system");
        assert_eq!(ledger.completed_count(), 0);
        assert_eq!(ledger.total_count(), 0);

        let id1 = ledger.add_item(
            "JWT token generation",
            None,
            vec!["src/auth/jwt.rs".to_string()],
        );
        let id2 = ledger.add_item(
            "Login endpoint",
            None,
            vec!["src/routes/login.rs".to_string()],
        );
        assert_eq!(ledger.completed_count(), 0);
        assert_eq!(ledger.total_count(), 2);

        let block = ledger.to_prompt_block();
        assert!(block.contains("<goal_anchor>"));
        assert!(block.contains("<root_objective>Implement authentication system</root_objective>"));
        assert!(block.contains("<execution_ledger progress=\"0/2 completed\">"));
        assert!(block.contains("[ ] JWT token generation"));
        assert!(block.contains("[ ] Login endpoint"));

        ledger.set_status(&id1, RequirementStatus::InProgress);
        let block_in_progress = ledger.to_prompt_block();
        assert!(block_in_progress.contains("[-] JWT token generation"));

        ledger.set_status(&id1, RequirementStatus::Completed);
        assert_eq!(ledger.completed_count(), 1);
        let block_completed = ledger.to_prompt_block();
        assert!(block_completed.contains("<execution_ledger progress=\"1/2 completed\">"));
        assert!(block_completed.contains("[x] JWT token generation"));

        // Drift test: 5 turns with 1 completed item (1 remaining)
        ledger.consecutive_turns_without_progress = 5;
        let warning = ledger.check_drift(4);
        assert!(warning.is_some());
        let warning_msg = warning.unwrap();
        assert!(warning_msg.contains(
            "⚠️ Task Drift Warning: 5 turns have passed without requirement progress (completed 1/2)."
        ));
        assert!(warning_msg.contains("Active task: Login endpoint"));
        assert!(warning_msg.contains(
            "Do not get distracted by tangential edits; focus on completing remaining requirements."
        ));

        // When turns < threshold, no drift
        assert!(ledger.check_drift(6).is_none());

        // When all items completed, no drift warning even if turns exceed threshold
        ledger.set_status(&id2, RequirementStatus::Completed);
        assert_eq!(ledger.completed_count(), 2);
        ledger.consecutive_turns_without_progress = 10;
        assert!(ledger.check_drift(4).is_none());
    }

    #[test]
    fn test_intent_ledger_status_markers() {
        let mut ledger = IntentLedger::new("Test status markers");
        let _id_pend = ledger.add_item("Pending item", None, vec![]);
        let id_prog = ledger.add_item("InProgress item", None, vec![]);
        let id_comp = ledger.add_item("Completed item", None, vec![]);
        let id_block = ledger.add_item("Blocked item", None, vec![]);
        let id_skip = ledger.add_item("Skipped item", None, vec![]);

        ledger.set_status(&id_prog, RequirementStatus::InProgress);
        ledger.set_status(&id_comp, RequirementStatus::Completed);
        ledger.set_status(&id_block, RequirementStatus::Blocked);
        ledger.set_status(&id_skip, RequirementStatus::Skipped);

        let block = ledger.to_prompt_block();
        assert!(block.contains("[ ] Pending item"));
        assert!(block.contains("[-] InProgress item"));
        assert!(block.contains("[x] Completed item"));
        assert!(block.contains("[!] Blocked item"));
        assert!(block.contains("[s] Skipped item"));
    }
}

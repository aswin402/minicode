use crate::constants::{
    ARCHIVE_DIR, FINDINGS_FILE, MAX_PLAN_LINES_IN_PROMPT, PLAN_DIR, PROGRESS_FILE,
    PROGRESS_TRUNCATE_THRESHOLD, TASK_PLAN_FILE, TIMESTAMP_FORMAT, WORKSPACE_DIR_NAME,
};
use crate::error::{ContextError, Result};
use chrono::Utc;
use std::fs;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Status of a discrete task item in the execution plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskItemStatus {
    Pending,
    InProgress,
    Completed,
}

/// A parsed atomic task item from `todo.md` or `task_plan.md`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskItem {
    pub line_index: usize,
    pub status: TaskItemStatus,
    pub title: String,
    #[serde(default)]
    pub sub_tasks: Vec<TaskItem>,
}

/// A milestone or phase section containing a collection of atomic tasks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MilestonePhase {
    pub id: String,
    pub title: String,
    pub total_tasks: usize,
    pub completed_tasks: usize,
    pub in_progress_tasks: usize,
    pub pending_tasks: usize,
    pub tasks: Vec<TaskItem>,
    pub is_active: bool,
}

/// Detailed summary of the active plan and its tasks for UI synchronization.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivePlanSummary {
    pub phase_label: String,
    pub total_tasks: usize,
    pub completed_tasks: usize,
    pub in_progress_tasks: usize,
    pub pending_tasks: usize,
    pub active_task: Option<String>,
    pub tasks: Vec<TaskItem>,
}

/// Quadrants of the senior engineer scratchpad (`.minicode/NOTES.md`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum NoteSection {
    Invariants,
    Hypotheses,
    NegativeKnowledge,
    WorkingSetAnchors,
}

impl NoteSection {
    pub fn heading(&self) -> &'static str {
        match self {
            Self::Invariants => "## 1. Architectural Invariants & Constraints",
            Self::Hypotheses => "## 2. Active Hypotheses & Evidence",
            Self::NegativeKnowledge => "## 3. Negative Knowledge (DO NOT ATTEMPT)",
            Self::WorkingSetAnchors => "## 4. Working Set Anchors & Offsets",
        }
    }
}

/// Filesystem Working Memory & Task Plan of Record for complex multi-step coding tasks.
/// Synchronizes bidirectionally with `minikit_docs/core/todo.md` (or `.minicode/plan/`)
/// and maintains `.minicode/NOTES.md` as the senior engineer cognitive scratchpad.
#[derive(Debug, Clone)]
pub struct WorkingMemory {
    workspace_root: PathBuf,
}

impl WorkingMemory {
    pub const NOTES_FILE_NAME: &'static str = "NOTES.md";

    pub fn new(workspace_root: &Path) -> Self {
        Self {
            workspace_root: workspace_root.to_path_buf(),
        }
    }

    pub fn plan_dir(&self) -> PathBuf {
        self.workspace_root.join(WORKSPACE_DIR_NAME).join(PLAN_DIR)
    }

    pub fn task_plan_path(&self) -> PathBuf {
        self.plan_dir().join(TASK_PLAN_FILE)
    }

    pub fn findings_path(&self) -> PathBuf {
        self.plan_dir().join(FINDINGS_FILE)
    }

    pub fn progress_path(&self) -> PathBuf {
        self.plan_dir().join(PROGRESS_FILE)
    }

    pub fn archive_dir(&self) -> PathBuf {
        self.plan_dir().join(ARCHIVE_DIR)
    }

    /// Resolves canonical project `todo.md` via MiniKit (`minikit_docs/core/todo.md` or `todo.md`).
    pub fn canonical_todo_path(&self) -> PathBuf {
        let minikit_todo = crate::tools::minikit::resolve_doc_path(&self.workspace_root, "todo.md");
        if minikit_todo.exists() {
            return minikit_todo;
        }
        let root_todo = self.workspace_root.join("todo.md");
        if root_todo.exists() {
            return root_todo;
        }
        let legacy_todo = self.workspace_root.join("minikit_docs").join("todo.md");
        if legacy_todo.exists() {
            return legacy_todo;
        }
        let onpkg_todo = self.workspace_root.join("onpkg_docs").join("todo.md");
        if onpkg_todo.exists() {
            return onpkg_todo;
        }
        minikit_todo
    }

    /// Resolves canonical project `implementation.md` via MiniKit.
    pub fn canonical_implementation_path(&self) -> PathBuf {
        let minikit_impl =
            crate::tools::minikit::resolve_doc_path(&self.workspace_root, "implementation.md");
        if minikit_impl.exists() {
            return minikit_impl;
        }
        let root_impl = self.workspace_root.join("implementation.md");
        if root_impl.exists() {
            return root_impl;
        }
        minikit_impl
    }

    /// Resolves path to the senior engineer scratchpad (`.minicode/NOTES.md`).
    pub fn notes_path(&self) -> PathBuf {
        self.workspace_root
            .join(WORKSPACE_DIR_NAME)
            .join(Self::NOTES_FILE_NAME)
    }

    /// Checks if there is an active task plan in progress.
    pub fn has_active_plan(&self) -> bool {
        let plan_path = self.task_plan_path();
        match fs::read_to_string(&plan_path) {
            Ok(s) => !s.trim().is_empty(),
            Err(e) => {
                if e.kind() != ErrorKind::NotFound {
                    tracing::warn!(path = %plan_path.display(), error = %e, "Failed to inspect task plan");
                }
                false
            }
        }
    }

    /// Strips leading numbers, markdown bullets, and checkbox brackets from a task step string.
    pub fn sanitize_step_title(raw: &str) -> String {
        let mut s = raw.trim();
        // Strip leading list numbering like "1. ", "2) ", "10. "
        if let Some(pos) = s.find(['.', ')']) {
            if s[..pos].chars().all(|c| c.is_ascii_digit()) && pos + 1 < s.len() {
                s = s[pos + 1..].trim();
            }
        }
        // Strip markdown bullets
        s = s.trim_start_matches(['-', '*', '+', ' ']);
        // Strip brackets like "[ ] ", "[>] ", "[x] ", "[X] ", "[/] "
        for marker in &["[ ]", "[>]", "[/]", "[x]", "[X]"] {
            if s.starts_with(marker) {
                s = s[marker.len()..].trim();
                break;
            }
        }
        // Secondary pass for nested redundant numbers e.g. "1. [ ] 1. Task"
        if let Some(pos) = s.find(['.', ')']) {
            if s[..pos].chars().all(|c| c.is_ascii_digit()) && pos + 1 < s.len() {
                s = s[pos + 1..].trim();
            }
        }
        s.trim().to_string()
    }

    /// Initializes a new task plan with structured steps (backward-compatible delegate to `init_two_tier_plan`).
    pub fn init_plan(&self, title: &str, steps: &[String]) -> Result<()> {
        self.init_two_tier_plan(title, steps, None, None)
    }

    /// Initializes a comprehensive two-tier task plan:
    /// - Tier 1 Architecture Blueprint: Written to `minikit_docs/core/implementation.md`.
    /// - Tier 1 Roadmap Milestones: Written to `minikit_docs/core/todo.md`.
    /// - Tier 2 Tactical Steps: Written to `.minicode/plan/task_plan.md` and `.minicode/plan/progress.md`.
    pub fn init_two_tier_plan(
        &self,
        title: &str,
        steps: &[String],
        architecture: Option<&str>,
        milestones: Option<&[String]>,
    ) -> Result<()> {
        let dir = self.plan_dir();
        fs::create_dir_all(&dir).map_err(|e| ContextError::Memory(e.to_string()))?;

        let timestamp = Utc::now().format(TIMESTAMP_FORMAT).to_string();

        // 1. Tier 1 Architecture Blueprint -> canonical implementation.md
        let impl_path = self.canonical_implementation_path();
        let is_placeholder = fs::read_to_string(&impl_path)
            .map(|c| c.contains("Code layout, public APIs") || c.len() < 300)
            .unwrap_or(true);
        let should_write_impl = architecture.is_some() || !impl_path.exists() || is_placeholder;
        if should_write_impl {
            if let Some(parent) = impl_path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let impl_content = if let Some(arch) = architecture {
                arch.to_string()
            } else {
                let mut blueprint = format!(
                    "# Technical Implementation Architecture — {}\n\n> Created: {}\n> Generated by minicode Two-Tier Planning Engine\n\n## 1. Executive Summary & Goals\nBlueprint and technical architecture for {}.\n\n## 2. Key Modules & Planned Deliverables\n",
                    title, timestamp, title
                );
                for (idx, step) in steps.iter().enumerate() {
                    blueprint.push_str(&format!(
                        "{}. {}\n",
                        idx + 1,
                        Self::sanitize_step_title(step)
                    ));
                }
                blueprint.push_str("\n## 3. Verification & Acceptance Criteria\n- Compiles and lints cleanly with zero warnings\n- Core functionality verified against specifications\n");
                blueprint
            };
            let _ = fs::write(&impl_path, impl_content);
        }

        // 2. Tier 1 Milestone Roadmap -> canonical todo.md
        let todo_path = self.canonical_todo_path();
        if let Some(parent) = todo_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let (has_uncompleted, is_boilerplate) = if let Ok(existing) = fs::read_to_string(&todo_path)
        {
            let uncompleted = existing.lines().any(|l| {
                let t = l.trim();
                t.starts_with("- [ ]")
                    || t.starts_with("* [ ]")
                    || t.starts_with("- [>]")
                    || t.starts_with("* [>]")
            });
            let boilerplate = existing.contains("Initial project setup and architecture sync")
                || existing.contains("Scaffolding complete. Decompose features into milestones");
            (uncompleted, boilerplate)
        } else {
            (false, false)
        };

        let should_write_todo = milestones.is_some_and(|ms| !ms.is_empty())
            || !todo_path.exists()
            || is_boilerplate
            || !has_uncompleted;

        if should_write_todo && !steps.is_empty() {
            let mut todo_content = format!(
                "# Tasks & Milestone Roadmap: {}\n\n> Initialized: {}\n",
                title, timestamp
            );
            if let Some(ms) = milestones.filter(|ms| !ms.is_empty()) {
                let cleaned_ms: Vec<String> =
                    ms.iter().map(|m| Self::sanitize_step_title(m)).collect();
                todo_content.push_str(&format!("> Milestones: {}\n", cleaned_ms.join(" | ")));
            }
            todo_content.push('\n');
            todo_content.push_str(&format!("## Active Milestone: {}\n", title));
            for (idx, step) in steps.iter().enumerate() {
                let clean = Self::sanitize_step_title(step);
                let marker = if idx == 0 { "[>]" } else { "[ ]" };
                todo_content.push_str(&format!("- {} {}\n", marker, clean));
            }
            let _ = fs::write(&todo_path, todo_content);
        }

        // 3. Tier 2 Tactical Steps -> .minicode/plan/task_plan.md
        let mut plan_content = format!(
            "# Task Plan: {}\n\n> Created: {}\n> Mode: direct\n\n## Objectives & Steps:\n",
            title, timestamp
        );
        for (idx, step) in steps.iter().enumerate() {
            let clean = Self::sanitize_step_title(step);
            let marker = if idx == 0 { "[>]" } else { "[ ]" };
            plan_content.push_str(&format!("{}. {} {}\n", idx + 1, marker, clean));
        }

        fs::write(self.task_plan_path(), &plan_content)
            .map_err(|e| ContextError::Memory(e.to_string()))?;

        // 4. Progress tracker & findings
        let first_step_clean = steps
            .first()
            .map(|s| Self::sanitize_step_title(s))
            .unwrap_or_default();
        let initial_progress = format!(
            "# Progress Tracker\n\n> Initialized: {}\n\n- Active Goal: {}\n- Status: In Progress\n\n- [{}] **In Progress**: {}\n",
            timestamp, title, timestamp, first_step_clean
        );
        fs::write(self.progress_path(), initial_progress)
            .map_err(|e| ContextError::Memory(e.to_string()))?;

        let initial_findings = format!(
            "# Discoveries & Code Findings\n\n> Workspace: {}\n\n",
            self.workspace_root.display()
        );
        fs::write(self.findings_path(), initial_findings)
            .map_err(|e| ContextError::Memory(e.to_string()))?;

        // Synchronize living IntentLedger with the new active plan
        let intent_path = self
            .workspace_root
            .join(crate::constants::DEFAULT_INTENT_PERSISTENCE_FILE);
        let mut new_ledger = crate::context::memory::intent::IntentLedger::new(title);
        for step in steps {
            new_ledger.add_item(&Self::sanitize_step_title(step), None, Vec::new());
        }
        let _ = new_ledger.save_to_disk(&intent_path);

        // Initialize NOTES.md template if missing
        let _ = self.ensure_notes_template();

        Ok(())
    }

    /// Initializes tactical sub-steps in `.minicode/plan/task_plan.md` for the currently active `[>]`
    /// macro task in `minikit_docs/core/todo.md` (used by `power_plan` to decompose an active task).
    pub fn init_tactical_subplan(&self, topic: &str, steps: &[String]) -> Result<()> {
        if steps.is_empty() {
            return Ok(());
        }
        let dir = self.plan_dir();
        fs::create_dir_all(&dir).map_err(|e| ContextError::Memory(e.to_string()))?;
        let timestamp = Utc::now().format(TIMESTAMP_FORMAT).to_string();

        // Ensure canonical todo.md exists; if not, initialize it with `topic` as the macro task.
        let todo_path = self.canonical_todo_path();
        if !todo_path.exists() {
            if let Some(parent) = todo_path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let clean_topic = Self::sanitize_step_title(topic);
            let todo_content = format!(
                "# Tasks & Milestone Roadmap: {}\n\n> Initialized: {}\n\n## Active Milestone: {}\n- [>] {}\n",
                clean_topic, timestamp, clean_topic, clean_topic
            );
            let _ = fs::write(&todo_path, todo_content);
        }

        let mut plan_content = format!(
            "# Task Plan: {}\n\n> Created: {}\n> Mode: subplan\n\n## Objectives & Steps:\n",
            topic, timestamp
        );
        for (idx, step) in steps.iter().enumerate() {
            let clean = Self::sanitize_step_title(step);
            let marker = if idx == 0 { "[>]" } else { "[ ]" };
            plan_content.push_str(&format!("{}. {} {}\n", idx + 1, marker, clean));
        }

        fs::write(self.task_plan_path(), &plan_content)
            .map_err(|e| ContextError::Memory(e.to_string()))?;

        let first_step_clean = steps
            .first()
            .map(|s| Self::sanitize_step_title(s))
            .unwrap_or_default();
        let entry = format!(
            "\n- [{}] **Sub-Plan ({})**: [>] {}\n",
            timestamp, topic, first_step_clean
        );
        let _ = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.progress_path())
            .and_then(|mut f| f.write_all(entry.as_bytes()));

        Ok(())
    }

    /// Reads the active execution plan from `.minicode/plan/task_plan.md`.
    pub fn read_task_plan(&self) -> Result<Option<String>> {
        let path = self.task_plan_path();
        match fs::read_to_string(&path) {
            Ok(content) if !content.trim().is_empty() => Ok(Some(content)),
            Ok(_) => Ok(None),
            Err(e) => {
                if e.kind() == ErrorKind::NotFound {
                    Ok(None)
                } else {
                    tracing::warn!(path = %path.display(), error = %e, "Failed to read task plan");
                    Err(ContextError::Memory(e.to_string()).into())
                }
            }
        }
    }

    /// Reads the canonical project roadmap from `canonical_todo_path()`.
    pub fn read_roadmap_todo(&self) -> Result<Option<String>> {
        let todo_path = self.canonical_todo_path();
        if todo_path.exists() {
            if let Ok(content) = fs::read_to_string(&todo_path) {
                if !content.trim().is_empty() {
                    return Ok(Some(content));
                }
            }
        }
        Ok(None)
    }

    /// Reads the active plan if available (prefers active task plan, falls back to roadmap).
    pub fn read_plan(&self) -> Result<Option<String>> {
        if let Ok(Some(plan)) = self.read_task_plan() {
            return Ok(Some(plan));
        }
        self.read_roadmap_todo()
    }

    /// Parses discrete task items specifically from active `.minicode/plan/task_plan.md`.
    pub fn read_task_plan_tasks(&self) -> Vec<TaskItem> {
        let content = match self.read_task_plan() {
            Ok(Some(c)) => c,
            _ => return Vec::new(),
        };

        let mut tasks = Vec::new();
        for (idx, line) in content.lines().enumerate() {
            if let Some(task) = Self::parse_task_line(idx, line.trim()) {
                tasks.push(task);
            }
        }
        tasks
    }

    /// Extracts title header from `task_plan.md`.
    pub fn read_task_plan_title(&self) -> Option<String> {
        let content = self.read_task_plan().ok().flatten()?;
        for line in content.lines() {
            let trimmed = line.trim();
            if let Some(t) = trimmed.strip_prefix("# Task Plan:") {
                let clean = t.trim();
                if !clean.is_empty() {
                    return Some(clean.to_string());
                }
            } else if let Some(t) = trimmed.strip_prefix("# ") {
                let clean = t.trim();
                if !clean.is_empty() {
                    return Some(clean.to_string());
                }
            }
        }
        None
    }

    /// Parses a single task line into a TaskItem if it contains a task marker.
    pub fn parse_task_line(idx: usize, trimmed: &str) -> Option<TaskItem> {
        let markers = [
            ("[x]", TaskItemStatus::Completed),
            ("[X]", TaskItemStatus::Completed),
            ("[>]", TaskItemStatus::InProgress),
            ("[/]", TaskItemStatus::InProgress),
            ("[ ]", TaskItemStatus::Pending),
        ];

        for (marker, status) in markers {
            if let Some(pos) = trimmed.find(marker) {
                let prefix = &trimmed[..pos];
                // Prefix must be a list marker: e.g. "", "-", "*", "+", "1.", "1)", or whitespace
                let is_valid_prefix = prefix.chars().all(|c| {
                    c.is_ascii_whitespace()
                        || c.is_ascii_digit()
                        || c == '-'
                        || c == '*'
                        || c == '+'
                        || c == '.'
                        || c == ')'
                });

                if is_valid_prefix {
                    let raw_title = &trimmed[pos + marker.len()..];
                    let clean_title = Self::sanitize_step_title(raw_title);
                    if !clean_title.is_empty() {
                        return Some(TaskItem {
                            line_index: idx,
                            status,
                            title: clean_title,
                            sub_tasks: Vec::new(),
                        });
                    }
                }
            }
        }
        None
    }

    /// Parses all discrete task items from the active plan.
    pub fn read_parsed_tasks(&self) -> Vec<TaskItem> {
        let content = match self.read_plan() {
            Ok(Some(c)) => c,
            _ => return Vec::new(),
        };

        let mut tasks = Vec::new();
        for (idx, line) in content.lines().enumerate() {
            if let Some(task) = Self::parse_task_line(idx, line.trim()) {
                tasks.push(task);
            }
        }
        tasks
    }

    /// Parses roadmap milestones and their tasks from canonical `todo.md`.
    pub fn read_roadmap_milestones(&self) -> Vec<MilestonePhase> {
        let content = match self
            .read_roadmap_todo()
            .ok()
            .flatten()
            .or_else(|| self.read_task_plan().ok().flatten())
        {
            Some(c) => c,
            None => return Vec::new(),
        };

        let mut milestones: Vec<MilestonePhase> = Vec::new();
        let mut current_id = "General".to_string();
        let mut current_title = "Active Tasks".to_string();
        let mut current_tasks: Vec<TaskItem> = Vec::new();

        for (idx, line) in content.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("### ")
                || trimmed.starts_with("## ")
                || trimmed.starts_with("# ")
            {
                let header_text = trimmed.trim_start_matches('#').trim();
                if !current_tasks.is_empty() {
                    let total = current_tasks.len();
                    let completed = current_tasks
                        .iter()
                        .filter(|t| t.status == TaskItemStatus::Completed)
                        .count();
                    let in_progress = current_tasks
                        .iter()
                        .filter(|t| t.status == TaskItemStatus::InProgress)
                        .count();
                    let pending = current_tasks
                        .iter()
                        .filter(|t| t.status == TaskItemStatus::Pending)
                        .count();
                    milestones.push(MilestonePhase {
                        id: current_id.clone(),
                        title: current_title.clone(),
                        total_tasks: total,
                        completed_tasks: completed,
                        in_progress_tasks: in_progress,
                        pending_tasks: pending,
                        tasks: std::mem::take(&mut current_tasks),
                        is_active: false,
                    });
                }
                if let Some((id_part, title_part)) = header_text.split_once(':') {
                    current_id = id_part.trim().to_string();
                    current_title = title_part.trim().to_string();
                } else {
                    current_id = header_text.to_string();
                    current_title = header_text.to_string();
                }
                continue;
            }

            if let Some(task) = Self::parse_task_line(idx, trimmed) {
                current_tasks.push(task);
            }
        }

        if !current_tasks.is_empty() {
            let total = current_tasks.len();
            let completed = current_tasks
                .iter()
                .filter(|t| t.status == TaskItemStatus::Completed)
                .count();
            let in_progress = current_tasks
                .iter()
                .filter(|t| t.status == TaskItemStatus::InProgress)
                .count();
            let pending = current_tasks
                .iter()
                .filter(|t| t.status == TaskItemStatus::Pending)
                .count();
            milestones.push(MilestonePhase {
                id: current_id,
                title: current_title,
                total_tasks: total,
                completed_tasks: completed,
                in_progress_tasks: in_progress,
                pending_tasks: pending,
                tasks: current_tasks,
                is_active: false,
            });
        }

        // Determine active milestone dynamically:
        // 1. First milestone with any InProgress task
        // 2. Otherwise, first milestone with any Pending task
        // 3. If all tasks across all milestones are completed, no milestone is active!
        if let Some(active_idx) = milestones.iter().position(|m| m.in_progress_tasks > 0) {
            milestones[active_idx].is_active = true;
        } else if let Some(pending_idx) = milestones.iter().position(|m| m.pending_tasks > 0) {
            milestones[pending_idx].is_active = true;
        }

        milestones
    }

    /// Reads only the active phase's tasks, windowed to recent completed + active + next pending.
    /// Returns `(phase_title, windowed_tasks)`. If all tasks are completed or no tasks exist,
    /// returns `(None, Vec::new())`.
    #[allow(dead_code)]
    pub fn read_active_phase_tasks(&self) -> (Option<String>, Vec<TaskItem>) {
        let milestones = self.read_roadmap_milestones();
        if let Some(active_milestone) = milestones.iter().find(|m| m.is_active) {
            let phase_label = if active_milestone.id == active_milestone.title {
                active_milestone.id.clone()
            } else {
                format!("{}: {}", active_milestone.id, active_milestone.title)
            };
            let tasks = &active_milestone.tasks;

            let completed: Vec<&TaskItem> = tasks
                .iter()
                .filter(|t| t.status == TaskItemStatus::Completed)
                .collect();
            let in_progress: Vec<&TaskItem> = tasks
                .iter()
                .filter(|t| t.status == TaskItemStatus::InProgress)
                .collect();
            let pending: Vec<&TaskItem> = tasks
                .iter()
                .filter(|t| t.status == TaskItemStatus::Pending)
                .collect();

            let mut windowed = Vec::new();
            // Take up to 2 most recent completed tasks for context
            let start = completed.len().saturating_sub(2);
            for t in &completed[start..] {
                windowed.push((*t).clone());
            }
            // Take all in-progress tasks
            for t in in_progress {
                windowed.push(t.clone());
            }
            // Take up to 3 next pending tasks
            for t in pending.into_iter().take(3) {
                windowed.push(t.clone());
            }

            (Some(phase_label), windowed)
        } else {
            (None, Vec::new())
        }
    }

    /// Helper to construct an ActivePlanSummary from a slice of tasks and phase label.
    pub fn build_plan_summary(
        phase_label: String,
        tasks: &[TaskItem],
    ) -> Option<ActivePlanSummary> {
        if tasks.is_empty() {
            return None;
        }

        let stats_source = tasks
            .iter()
            .find(|t| t.status == TaskItemStatus::InProgress && !t.sub_tasks.is_empty())
            .map(|t| t.sub_tasks.as_slice())
            .unwrap_or(tasks);

        let total_tasks = stats_source.len();
        let completed_tasks = stats_source
            .iter()
            .filter(|t| t.status == TaskItemStatus::Completed)
            .count();
        let in_progress_tasks = stats_source
            .iter()
            .filter(|t| t.status == TaskItemStatus::InProgress)
            .count();
        let pending_tasks = stats_source
            .iter()
            .filter(|t| t.status == TaskItemStatus::Pending)
            .count();

        let active_task = stats_source
            .iter()
            .find(|t| t.status == TaskItemStatus::InProgress)
            .map(|t| t.title.clone())
            .or_else(|| {
                stats_source
                    .iter()
                    .find(|t| t.status == TaskItemStatus::Pending)
                    .map(|t| t.title.clone())
            });

        let mut windowed = Vec::new();
        if pending_tasks == 0 && in_progress_tasks == 0 {
            // All tasks completed: display all completed tasks (up to 6)
            let start = tasks.len().saturating_sub(6);
            for t in &tasks[start..] {
                windowed.push((*t).clone());
            }
        } else {
            let completed: Vec<&TaskItem> = tasks
                .iter()
                .filter(|t| t.status == TaskItemStatus::Completed)
                .collect();
            let in_progress: Vec<&TaskItem> = tasks
                .iter()
                .filter(|t| t.status == TaskItemStatus::InProgress)
                .collect();
            let pending: Vec<&TaskItem> = tasks
                .iter()
                .filter(|t| t.status == TaskItemStatus::Pending)
                .collect();

            let start = completed.len().saturating_sub(2);
            for t in &completed[start..] {
                windowed.push((*t).clone());
            }
            for t in in_progress {
                windowed.push(t.clone());
            }
            for t in pending.into_iter().take(3) {
                windowed.push(t.clone());
            }
        }

        Some(ActivePlanSummary {
            phase_label,
            total_tasks,
            completed_tasks,
            in_progress_tasks,
            pending_tasks,
            active_task,
            tasks: windowed,
        })
    }

    /// Returns whether `.minicode/plan/task_plan.md` is a `Direct` 1-to-1 project plan
    /// (`create_plan`) or a `SubPlan` decomposing the active `[>]` macro task (`power_plan`).
    pub fn is_tactical_subplan(&self) -> bool {
        if let Ok(Some(content)) = self.read_task_plan() {
            for line in content.lines().take(10) {
                let trimmed = line.trim();
                if let Some(mode) = trimmed
                    .strip_prefix("> Mode:")
                    .or_else(|| trimmed.strip_prefix("> mode:"))
                {
                    return mode.trim().eq_ignore_ascii_case("subplan");
                }
            }
        }
        false
    }

    /// Reconciles any uncompleted plan step titles in `task_plan.md` and `todo.md` if they
    /// mention a different MiniKit stack identifier than the `chosen_stack` actually scaffolded.
    pub fn reconcile_stack_choice(&self, chosen_stack: &str) {
        let chosen = chosen_stack.trim();
        if chosen.is_empty() {
            return;
        }
        let all_stacks: Vec<String> =
            crate::tools::minikit::scaffolder::MiniKitScaffolder::get_all_stacks_for(None)
                .into_iter()
                .map(|s| s.name)
                .filter(|name| !name.eq_ignore_ascii_case(chosen))
                .collect();
        if all_stacks.is_empty() {
            return;
        }

        for path in [self.task_plan_path(), self.canonical_todo_path()] {
            let Ok(content) = fs::read_to_string(&path) else {
                continue;
            };
            let mut changed = false;
            let mut new_lines = Vec::with_capacity(content.lines().count());
            for line in content.lines() {
                let mut updated = line.to_string();
                if line.contains("[ ]") || line.contains("[>]") {
                    for other_stack in &all_stacks {
                        if updated.contains(other_stack.as_str()) {
                            updated = updated.replace(other_stack.as_str(), chosen);
                            changed = true;
                        }
                    }
                }
                new_lines.push(updated);
            }
            if changed {
                let _ = fs::write(&path, new_lines.join("\n"));
            }
        }
    }

    /// Returns the active plan summary for UI synchronization.
    /// Prioritizes active MiniPower execution steps from `.minicode/plan/task_plan.md`
    /// when tactical tasks are in-flight, and smoothly switches to canonical `todo.md`
    /// roadmap milestones when a milestone's tactical steps are completed.
    pub fn read_active_plan_summary(&self) -> Option<ActivePlanSummary> {
        let task_plan_tasks = self.read_task_plan_tasks();
        let has_active_or_pending = task_plan_tasks
            .iter()
            .any(|t| t.status == TaskItemStatus::InProgress || t.status == TaskItemStatus::Pending);

        let milestones = self.read_roadmap_milestones();
        let active_milestone = milestones
            .iter()
            .find(|m| m.in_progress_tasks > 0)
            .or_else(|| milestones.iter().find(|m| m.pending_tasks > 0));

        // 1. If task_plan has active or pending tactical tasks, display the tactical plan!
        if !task_plan_tasks.is_empty() && has_active_or_pending {
            let label = if let Some(m) = active_milestone {
                if m.id == m.title {
                    m.id.clone()
                } else {
                    format!("{}: {}", m.id, m.title)
                }
            } else {
                self.read_task_plan_title()
                    .unwrap_or_else(|| "Active Milestone Tasks".to_string())
            };

            // Check if task_plan.md is a `SubPlan` (`power_plan`) or a `Direct` 1-to-1 plan (`create_plan`)
            let is_same_task_set = if !self.is_tactical_subplan() {
                true
            } else if let Some(m) = active_milestone {
                if m.tasks.len() == task_plan_tasks.len() {
                    m.tasks
                        .iter()
                        .zip(task_plan_tasks.iter())
                        .all(|(a, b)| a.title.eq_ignore_ascii_case(&b.title))
                } else {
                    false
                }
            } else {
                false
            };

            if is_same_task_set {
                return Self::build_plan_summary(label, &task_plan_tasks);
            }

            // Otherwise, task_plan_tasks are distinct tactical sub-steps for the active in-progress task.
            if let Some(m) = active_milestone {
                let is_single_placeholder = m.tasks.len() == 1
                    && (m.tasks[0].title.eq_ignore_ascii_case("active execution")
                        || m.tasks[0].title.eq_ignore_ascii_case("execution"));
                if m.tasks.len() > 1 && !is_single_placeholder {
                    let mut milestone_tasks = m.tasks.clone();
                    let active_idx = milestone_tasks
                        .iter()
                        .position(|t| t.status == TaskItemStatus::InProgress)
                        .or_else(|| {
                            milestone_tasks
                                .iter()
                                .position(|t| t.status == TaskItemStatus::Pending)
                        });
                    if let Some(idx) = active_idx {
                        milestone_tasks[idx].sub_tasks = task_plan_tasks.clone();
                    }
                    return Self::build_plan_summary(label, &milestone_tasks);
                }
            }

            return Self::build_plan_summary(label, &task_plan_tasks);
        }

        // 2. If task_plan is finished or empty, check roadmap milestones from canonical todo.md!
        if let Some(milestone) = active_milestone {
            let phase_label = if milestone.id == milestone.title {
                milestone.id.clone()
            } else {
                format!("{}: {}", milestone.id, milestone.title)
            };
            return Self::build_plan_summary(phase_label, &milestone.tasks);
        }

        // 3. Fallback to task_plan_tasks if all are completed
        if !task_plan_tasks.is_empty() {
            let title = self
                .read_task_plan_title()
                .unwrap_or_else(|| "Plan".to_string());
            return Self::build_plan_summary(title, &task_plan_tasks);
        }

        // 4. Fallback to last milestone if all milestones are completed
        if let Some(milestone) = milestones.last() {
            let phase_label = if milestone.id == milestone.title {
                milestone.id.clone()
            } else {
                format!("{}: {}", milestone.id, milestone.title)
            };
            return Self::build_plan_summary(phase_label, &milestone.tasks);
        }

        None
    }

    /// Appends a new architectural finding or observation.
    pub fn append_finding(&self, finding: &str) -> Result<()> {
        let dir = self.plan_dir();
        fs::create_dir_all(&dir).map_err(|e| ContextError::Memory(e.to_string()))?;

        let path = self.findings_path();
        let timestamp = Utc::now().format(TIMESTAMP_FORMAT).to_string();
        let entry = format!("\n### [{}] Observation\n{}\n", timestamp, finding);

        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|e| ContextError::Memory(e.to_string()))?;

        file.write_all(entry.as_bytes())
            .map_err(|e| ContextError::Memory(e.to_string()))?;

        Ok(())
    }

    /// Updates the progress status of a task step in both `task_plan.md` and `todo.md`.
    /// Supports step selection by 1-based index (e.g. "1", "step 1"), special keywords ("active", "current", "next"),
    /// or title substring. Automatically promotes the next pending task to InProgress when a task is completed.
    pub fn update_progress(&self, step: &str, status: &str) -> Result<()> {
        let dir = self.plan_dir();
        fs::create_dir_all(&dir).map_err(|e| ContextError::Memory(e.to_string()))?;

        let status_clean = status
            .trim()
            .to_ascii_lowercase()
            .replace([' ', '-', '_'], "");
        let (is_done, is_active, is_pending) = match status_clean.as_str() {
            "completed" | "complete" | "done" | "closed" | "finished" | "resolved" | "success"
            | "pass" | "passed" | "x" | "check" => (true, false, false),
            "inprogress" | "active" | "started" | "running" | "wip" | "doing" | "current" | ">"
            | "/" => (false, true, false),
            "pending" | "todo" | "open" | "queued" | "reset" | "unstarted" | "backlog" | " " => {
                (false, false, true)
            }
            other => {
                if other.starts_with("complete")
                    || other.starts_with("done")
                    || other.starts_with("finish")
                {
                    (true, false, false)
                } else if other.starts_with("inprog")
                    || other.starts_with("active")
                    || other.starts_with("start")
                    || other.starts_with("work")
                {
                    (false, true, false)
                } else if other.starts_with("pend")
                    || other.starts_with("todo")
                    || other.starts_with("queue")
                {
                    (false, false, true)
                } else {
                    return Err(ContextError::Memory(format!(
                        "Invalid task status '{}'. Supported: completed, in_progress, pending",
                        status
                    ))
                    .into());
                }
            }
        };

        let step_trimmed = step.trim();
        // Check for numeric index: "1", "2", or "step 1", "task 1", "step #1", "10.", "10. Test and verify...", "10: ..."
        let target_index: Option<usize> = if let Ok(n) = step_trimmed.parse::<usize>() {
            Some(n)
        } else {
            let lower = step_trimmed.to_ascii_lowercase();
            if let Some(rest) = lower
                .strip_prefix("step")
                .or_else(|| lower.strip_prefix("task"))
            {
                let num_str = rest.trim().trim_start_matches('#').trim();
                let digits: String = num_str.chars().take_while(|c| c.is_ascii_digit()).collect();
                digits.parse::<usize>().ok()
            } else {
                let digits: String = step_trimmed
                    .chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect();
                if !digits.is_empty() {
                    let remainder = &step_trimmed[digits.len()..];
                    if remainder.starts_with('.')
                        || remainder.starts_with(')')
                        || remainder.starts_with(':')
                        || remainder.starts_with('-')
                        || remainder.starts_with(' ')
                    {
                        digits.parse::<usize>().ok()
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
        };

        let is_target_active = step_trimmed.eq_ignore_ascii_case("active")
            || step_trimmed.eq_ignore_ascii_case("current");
        let is_target_next = step_trimmed.eq_ignore_ascii_case("next");
        let is_target_all_remaining = step_trimmed.eq_ignore_ascii_case("all")
            || step_trimmed.eq_ignore_ascii_case("remaining")
            || step_trimmed.eq_ignore_ascii_case("rest")
            || step_trimmed.starts_with("remaining");
        if is_target_all_remaining {
            return Err(ContextError::Memory(
                "Bulk completion ('all' / 'remaining') is rejected. Each task step must be verified and updated individually.".to_string()
            ).into());
        }
        let clean_needle = Self::sanitize_step_title(step_trimmed).to_ascii_lowercase();

        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        enum UpdateFileOutcome {
            Modified,
            AlreadyInStatus,
            NotFound,
        }

        // Helper to update a target markdown plan file
        let update_file = |path: &Path| -> UpdateFileOutcome {
            if !path.exists() {
                return UpdateFileOutcome::NotFound;
            }
            let Ok(content) = fs::read_to_string(path) else {
                return UpdateFileOutcome::NotFound;
            };

            // First pass: identify all parsed task items with their line indices
            let mut parsed_items = Vec::new();
            for (line_idx, line) in content.lines().enumerate() {
                if let Some(item) = Self::parse_task_line(line_idx, line.trim()) {
                    parsed_items.push((line_idx, item));
                }
            }

            if parsed_items.is_empty() {
                return UpdateFileOutcome::NotFound;
            }

            // Determine matching item index in parsed_items
            let matched_idx = if let Some(idx) = target_index {
                if idx >= 1 && idx <= parsed_items.len() {
                    Some(idx - 1)
                } else {
                    None
                }
            } else if is_target_active {
                parsed_items
                    .iter()
                    .position(|(_, item)| item.status == TaskItemStatus::InProgress)
                    .or_else(|| {
                        parsed_items
                            .iter()
                            .position(|(_, item)| item.status == TaskItemStatus::Pending)
                    })
            } else if is_target_next {
                parsed_items
                    .iter()
                    .position(|(_, item)| item.status == TaskItemStatus::Pending)
            } else {
                // Match by title substring or line substring
                parsed_items
                    .iter()
                    .position(|(_, item)| {
                        let item_title = item.title.to_ascii_lowercase();
                        item_title.contains(&clean_needle) || clean_needle.contains(&item_title)
                    })
                    .or_else(|| {
                        // Word-overlap / token similarity match if substring didn't match directly
                        let needle_words: std::collections::HashSet<&str> =
                            clean_needle.split_whitespace().collect();
                        if needle_words.is_empty() {
                            return None;
                        }
                        parsed_items.iter().position(|(_, item)| {
                            let title_lower = item.title.to_ascii_lowercase();
                            let item_words: std::collections::HashSet<&str> =
                                title_lower.split_whitespace().collect();
                            let common = needle_words.intersection(&item_words).count();
                            common * 2 >= needle_words.len()
                        })
                    })
                    .or_else(|| {
                        parsed_items.iter().position(|(l_idx, _)| {
                            let line = content.lines().nth(*l_idx).unwrap_or("");
                            line.contains(step_trimmed)
                        })
                    })
            };

            let Some(target_pos) = matched_idx else {
                return UpdateFileOutcome::NotFound;
            };

            let current_status = parsed_items[target_pos].1.status;
            if (is_done && current_status == TaskItemStatus::Completed)
                || (is_active && current_status == TaskItemStatus::InProgress)
                || (is_pending && current_status == TaskItemStatus::Pending)
            {
                return UpdateFileOutcome::AlreadyInStatus;
            }

            let target_line_idx = parsed_items[target_pos].0;
            let mut updated_lines: Vec<String> = Vec::new();
            let mut modified = false;

            // Check if any other item in the file is already InProgress
            let has_other_active = parsed_items.iter().enumerate().any(|(pos, (_, item))| {
                pos != target_pos && item.status == TaskItemStatus::InProgress
            });

            let mut auto_advanced = false;

            for (line_idx, line) in content.lines().enumerate() {
                if line_idx == target_line_idx {
                    if is_done {
                        if line.contains("[ ]") {
                            updated_lines.push(line.replacen("[ ]", "[x]", 1));
                            modified = true;
                            continue;
                        } else if line.contains("[>]") {
                            updated_lines.push(line.replacen("[>]", "[x]", 1));
                            modified = true;
                            continue;
                        } else if line.contains("[/]") {
                            updated_lines.push(line.replacen("[/]", "[x]", 1));
                            modified = true;
                            continue;
                        }
                    } else if is_active {
                        if line.contains("[ ]") {
                            updated_lines.push(line.replacen("[ ]", "[>]", 1));
                            modified = true;
                            continue;
                        } else if line.contains("[x]") {
                            updated_lines.push(line.replacen("[x]", "[>]", 1));
                            modified = true;
                            continue;
                        }
                    } else if is_pending {
                        if line.contains("[x]") {
                            updated_lines.push(line.replacen("[x]", "[ ]", 1));
                            modified = true;
                            continue;
                        } else if line.contains("[>]") {
                            updated_lines.push(line.replacen("[>]", "[ ]", 1));
                            modified = true;
                            continue;
                        }
                    }
                }

                // Dynamic Auto-Advancement Invariant:
                // If a task was marked completed, and no other task is active,
                // automatically advance the very next pending task to InProgress ([>])!
                if is_done
                    && modified
                    && !has_other_active
                    && !auto_advanced
                    && line_idx > target_line_idx
                    && line.contains("[ ]")
                {
                    updated_lines.push(line.replacen("[ ]", "[>]", 1));
                    auto_advanced = true;
                    continue;
                }

                updated_lines.push(line.to_string());
            }

            if modified {
                let updated = updated_lines.join("\n");
                let _ = fs::write(path, updated);
                UpdateFileOutcome::Modified
            } else {
                UpdateFileOutcome::NotFound
            }
        };

        let pre_plan_tasks = self.read_task_plan_tasks();
        let pre_todo_tasks = self
            .read_roadmap_milestones()
            .into_iter()
            .flat_map(|m| m.tasks)
            .collect::<Vec<_>>();
        let plan_has_uncompleted = pre_plan_tasks
            .iter()
            .any(|t| t.status == TaskItemStatus::InProgress || t.status == TaskItemStatus::Pending);
        let todo_has_uncompleted = pre_todo_tasks
            .iter()
            .any(|t| t.status == TaskItemStatus::InProgress || t.status == TaskItemStatus::Pending);
        let has_active_subplan = self.is_tactical_subplan()
            && !pre_plan_tasks.is_empty()
            && !pre_todo_tasks.is_empty()
            && plan_has_uncompleted;

        let (outcome_plan, outcome_todo) = if has_active_subplan {
            let op = update_file(&self.task_plan_path());
            if op != UpdateFileOutcome::NotFound {
                (op, UpdateFileOutcome::NotFound)
            } else {
                (op, update_file(&self.canonical_todo_path()))
            }
        } else if !pre_plan_tasks.is_empty() && !plan_has_uncompleted && todo_has_uncompleted {
            (
                UpdateFileOutcome::NotFound,
                update_file(&self.canonical_todo_path()),
            )
        } else {
            let op = update_file(&self.task_plan_path());
            let ot = update_file(&self.canonical_todo_path());
            (op, ot)
        };

        // Also synchronize IntentLedger if active on disk
        let mut intent_outcome = UpdateFileOutcome::NotFound;
        let intent_path = self
            .workspace_root
            .join(crate::constants::DEFAULT_INTENT_PERSISTENCE_FILE);
        if intent_path.exists() {
            if let Ok(mut ledger) =
                crate::context::memory::intent::IntentLedger::load_from_disk(&intent_path)
            {
                let clean_step = Self::sanitize_step_title(step).to_ascii_lowercase();
                for (item_idx, item) in ledger.items.iter_mut().enumerate() {
                    let title_lower = item.title.to_ascii_lowercase();
                    if title_lower.contains(&clean_step)
                        || clean_step.contains(&title_lower)
                        || target_index.map(|i| i - 1) == Some(item_idx)
                    {
                        let target_intent_status = if is_done {
                            crate::context::memory::intent::RequirementStatus::Completed
                        } else if is_active {
                            crate::context::memory::intent::RequirementStatus::InProgress
                        } else {
                            crate::context::memory::intent::RequirementStatus::Pending
                        };
                        if item.status == target_intent_status {
                            if intent_outcome != UpdateFileOutcome::Modified {
                                intent_outcome = UpdateFileOutcome::AlreadyInStatus;
                            }
                        } else {
                            item.status = target_intent_status;
                            intent_outcome = UpdateFileOutcome::Modified;
                        }
                    }
                }
                if intent_outcome == UpdateFileOutcome::Modified {
                    let _ = ledger.save_to_disk(&intent_path);
                }
            }
        }

        let is_any_modified = outcome_plan == UpdateFileOutcome::Modified
            || outcome_todo == UpdateFileOutcome::Modified
            || intent_outcome == UpdateFileOutcome::Modified;

        let is_any_already = outcome_plan == UpdateFileOutcome::AlreadyInStatus
            || outcome_todo == UpdateFileOutcome::AlreadyInStatus
            || intent_outcome == UpdateFileOutcome::AlreadyInStatus;

        if !is_any_modified && !is_any_already {
            let available_tasks = self.read_parsed_tasks();
            if available_tasks.is_empty()
                && !self.task_plan_path().exists()
                && !self.canonical_todo_path().exists()
            {
                // Auto-bootstrap active plan with this initial step so the agent workflow continues seamlessly
                let clean_title = Self::sanitize_step_title(step);
                let _ = self.init_plan("Development Plan", &[clean_title]);
                let _ = update_file(&self.task_plan_path());
                let _ = update_file(&self.canonical_todo_path());
                let timestamp = Utc::now().format(TIMESTAMP_FORMAT).to_string();
                let entry = format!("\n- [{}] **{}**: {}\n", timestamp, status, step);
                let _ = fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(self.progress_path())
                    .and_then(|mut f| f.write_all(entry.as_bytes()));
            } else {
                let pending_list = available_tasks
                    .iter()
                    .enumerate()
                    .map(|(i, t)| format!("  {}. [{:?}] {}", i + 1, t.status, t.title))
                    .collect::<Vec<_>>()
                    .join("\n");
                return Err(ContextError::Memory(format!(
                    "No matching task step found for '{}'. Available tasks:\n{}",
                    step, pending_list
                ))
                .into());
            }
        } else if is_any_modified {
            // Only append to progress.md if an actual status modification took place (idempotent progress tracking)
            let resolved_title = {
                let tasks = self.read_parsed_tasks();
                if let Some(idx) = target_index {
                    if idx >= 1 && idx <= tasks.len() {
                        tasks[idx - 1].title.clone()
                    } else {
                        Self::sanitize_step_title(step)
                    }
                } else if is_target_active {
                    tasks
                        .iter()
                        .find(|t| t.status == TaskItemStatus::InProgress)
                        .map(|t| t.title.clone())
                        .unwrap_or_else(|| Self::sanitize_step_title(step))
                } else if is_target_next {
                    tasks
                        .iter()
                        .find(|t| t.status == TaskItemStatus::Pending)
                        .map(|t| t.title.clone())
                        .unwrap_or_else(|| Self::sanitize_step_title(step))
                } else {
                    tasks
                        .iter()
                        .find(|t| {
                            let t_lower = t.title.to_ascii_lowercase();
                            t_lower.contains(&clean_needle) || clean_needle.contains(&t_lower)
                        })
                        .map(|t| t.title.clone())
                        .unwrap_or_else(|| Self::sanitize_step_title(step))
                }
            };
            let timestamp = Utc::now().format(TIMESTAMP_FORMAT).to_string();
            let entry = format!("\n- [{}] **{}**: {}\n", timestamp, status, resolved_title);
            let _ = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(self.progress_path())
                .and_then(|mut f| f.write_all(entry.as_bytes()));
        }

        // When a tactical sub-plan in task_plan.md finishes all its steps, advance the active task in canonical todo.md!
        if has_active_subplan {
            let task_plan_tasks = self.read_task_plan_tasks();
            if !task_plan_tasks.is_empty()
                && task_plan_tasks
                    .iter()
                    .all(|t| t.status == TaskItemStatus::Completed)
            {
                let todo_path = self.canonical_todo_path();
                if todo_path.exists() {
                    if let Ok(content) = fs::read_to_string(&todo_path) {
                        let mut updated_lines = Vec::new();
                        let mut advanced_completed = false;
                        let mut advanced_next = false;
                        for line in content.lines() {
                            if !advanced_completed && (line.contains("[>]") || line.contains("[/]"))
                            {
                                let replaced =
                                    line.replacen("[>]", "[x]", 1).replacen("[/]", "[x]", 1);
                                updated_lines.push(replaced);
                                advanced_completed = true;
                            } else if advanced_completed && !advanced_next && line.contains("[ ]") {
                                updated_lines.push(line.replacen("[ ]", "[>]", 1));
                                advanced_next = true;
                            } else {
                                updated_lines.push(line.to_string());
                            }
                        }
                        if advanced_completed {
                            let _ = fs::write(&todo_path, updated_lines.join("\n"));
                        }
                    }
                }
            }
        }

        // When all tasks in the active plan are completed, ensure progress.md reflects Completed (100%)
        let refreshed = self.read_parsed_tasks();
        if !refreshed.is_empty()
            && refreshed
                .iter()
                .all(|t| t.status == TaskItemStatus::Completed)
        {
            let p_path = self.progress_path();
            if let Ok(content) = fs::read_to_string(&p_path) {
                if content.contains("- Status: In Progress") {
                    let updated =
                        content.replacen("- Status: In Progress", "- Status: Completed (100%)", 1);
                    let _ = fs::write(&p_path, updated);
                }
            }
        }

        Ok(())
    }

    /// Extracts potential deliverable file names or relative paths from a task title.
    /// E.g. "Create main.js — terminal preview simulation..." -> ["main.js"]
    /// E.g. "Implement user auth in src/auth.rs" -> ["src/auth.rs"]
    pub fn extract_candidate_files(title: &str) -> Vec<String> {
        let mut candidates = Vec::new();
        for word in title.split_whitespace() {
            let clean = word.trim_matches(|c: char| {
                !c.is_alphanumeric() && c != '.' && c != '/' && c != '_' && c != '-'
            });
            if clean.is_empty() {
                continue;
            }
            if clean.eq_ignore_ascii_case("dockerfile") || clean.eq_ignore_ascii_case("makefile") {
                candidates.push(clean.to_string());
                continue;
            }
            if let Some(pos) = clean.rfind('.') {
                let ext = &clean[pos + 1..];
                // Ignore purely numeric extensions (e.g. "0.1.0" or "v1.0") and known non-file tech tokens
                if !ext.is_empty()
                    && ext.len() <= 6
                    && !ext.chars().all(|c| c.is_ascii_digit())
                    && ext.chars().all(|c| c.is_ascii_alphanumeric())
                {
                    let clean_lower = clean.to_ascii_lowercase();
                    if clean_lower != "node.js"
                        && clean_lower != "socket.io"
                        && !clean_lower.ends_with(".ai")
                        && !clean_lower.ends_with(".com")
                        && !clean_lower.ends_with(".org")
                        && !clean_lower.ends_with(".io")
                    {
                        candidates.push(clean.to_string());
                        continue;
                    }
                }
            }
        }
        candidates
    }

    /// Splits a file path's stem on CamelCase, PascalCase, snake_case, and kebab-case boundaries
    /// into lowercase tokens of length >= 3 (e.g. `src/components/FeaturesBento.tsx` -> `["features", "bento"]`).
    fn split_identifier_stem_tokens(file_path: &str) -> Vec<String> {
        let Some(stem) = Path::new(file_path).file_stem().and_then(|s| s.to_str()) else {
            return Vec::new();
        };
        let mut tokens = Vec::new();
        let mut current = String::new();
        let mut prev_lower_or_digit = false;

        for ch in stem.chars() {
            if !ch.is_ascii_alphanumeric() {
                if current.len() >= 3 {
                    tokens.push(current.to_ascii_lowercase());
                }
                current.clear();
                prev_lower_or_digit = false;
            } else {
                if prev_lower_or_digit && ch.is_ascii_uppercase() && current.len() >= 3 {
                    tokens.push(current.to_ascii_lowercase());
                    current.clear();
                }
                current.push(ch);
                prev_lower_or_digit = ch.is_ascii_lowercase() || ch.is_ascii_digit();
            }
        }
        if current.len() >= 3 {
            tokens.push(current.to_ascii_lowercase());
        }
        if tokens.len() > 1 {
            let filtered: Vec<String> = tokens
                .iter()
                .filter(|t| {
                    !matches!(
                        t.as_str(),
                        "section" | "sections" | "component" | "components"
                    )
                })
                .cloned()
                .collect();
            if !filtered.is_empty() {
                return filtered;
            }
        }
        tokens
    }

    /// Returns whether a stem token `s` matches any alphanumeric word `w` in `task_lower`.
    fn stem_token_matches_task(s: &str, task_lower: &str) -> bool {
        task_lower
            .split(|c: char| !c.is_ascii_alphanumeric())
            .filter(|w| w.len() >= 3)
            .any(|w| {
                if (w == "section" || w == "sections") && s != "section" && s != "sections" {
                    return false;
                }
                w == s
                    || (w.len() >= 4 && s.len() >= 4 && (w.starts_with(s) || s.starts_with(w)))
                    || (w.len() >= 6 && s.len() >= 6 && w[..3] == s[..3])
            })
    }

    /// Extracts completed `<section id="...">`, `<header id="...">`, `<nav>`, and `<footer>`
    /// identifiers from an HTML document (requiring at least 25 lines so starter templates do not trigger).
    fn extract_html_section_ids(html_content: &str) -> std::collections::HashSet<String> {
        let mut ids = std::collections::HashSet::new();
        if html_content.lines().count() < 25 {
            return ids;
        }
        let lower = html_content.to_ascii_lowercase();
        if lower.contains("<nav") && lower.contains("</nav>") {
            ids.insert("nav".to_string());
            ids.insert("navbar".to_string());
        }
        if lower.contains("<footer") && lower.contains("</footer>") {
            ids.insert("footer".to_string());
        }

        let mut search_pos = 0;
        while let Some(rel_lt) = lower[search_pos..].find('<') {
            let tag_start = search_pos + rel_lt;
            let rest = &lower[tag_start..];
            let is_section_like = rest.starts_with("<section")
                || rest.starts_with("<header")
                || rest.starts_with("<footer")
                || rest.starts_with("<nav")
                || rest.starts_with("<main");
            if is_section_like {
                if let Some(rel_gt) = rest.find('>') {
                    let open_tag = &rest[..rel_gt];
                    // If this is a <section>, verify that a closing </section> exists after it
                    let has_closing =
                        !rest.starts_with("<section") || rest[rel_gt..].contains("</section>");
                    if has_closing {
                        for attr in ["id=\"", "id='"] {
                            if let Some(id_start_rel) = open_tag.find(attr) {
                                let quote = attr.chars().last().unwrap_or('"');
                                let val_start = id_start_rel + attr.len();
                                if let Some(val_end_rel) = open_tag[val_start..].find(quote) {
                                    let raw_id = &open_tag[val_start..val_start + val_end_rel];
                                    for tok in raw_id
                                        .split(|c: char| !c.is_ascii_alphanumeric())
                                        .filter(|t| t.len() >= 3)
                                    {
                                        ids.insert(tok.to_string());
                                    }
                                }
                            }
                        }
                    }
                    search_pos = tag_start + rel_gt + 1;
                    continue;
                }
            }
            search_pos = tag_start + 1;
        }
        ids
    }

    /// Dynamically reconciles uncompleted tasks against files modified during the turn
    /// or verified on disk in the workspace.
    ///
    /// Returns the number of tasks updated to `Completed`.
    pub fn reconcile_workspace_tasks(&self, modified_files: &[String]) -> usize {
        let tasks = self.read_parsed_tasks();
        if tasks.is_empty() {
            return 0;
        }

        let mut html_section_ids = std::collections::HashSet::new();
        for m_path in modified_files {
            if m_path.ends_with(".html") || m_path.ends_with(".htm") {
                let full_path = self.workspace_root.join(m_path.trim_start_matches("./"));
                if let Ok(content) = fs::read_to_string(&full_path) {
                    html_section_ids.extend(Self::extract_html_section_ids(&content));
                }
            }
        }

        let mut reconciled = 0;
        let mut consumed_modified: std::collections::HashSet<usize> =
            std::collections::HashSet::new();

        for (task_idx, task) in tasks.iter().enumerate() {
            if task.status == TaskItemStatus::Completed {
                continue;
            }

            let task_lower = task.title.to_ascii_lowercase();
            let primary_head = task_lower
                .split([':', '—'])
                .next()
                .unwrap_or(&task_lower)
                .trim();
            let candidates = Self::extract_candidate_files(&task.title);
            let mut file_satisfied = false;

            for candidate in &candidates {
                // 1. Check if candidate matches any explicitly modified file
                let matched_mod_idx = modified_files.iter().position(|m| {
                    m.eq_ignore_ascii_case(candidate)
                        || m.ends_with(&format!("/{}", candidate))
                        || candidate.ends_with(&format!("/{}", m))
                });

                // 2. Check if candidate file exists on disk and is non-empty
                let on_disk = {
                    let direct = self.workspace_root.join(candidate);
                    if direct.is_file() {
                        fs::metadata(&direct).map(|m| m.len() > 0).unwrap_or(false)
                    } else if !candidate.contains('/') {
                        let in_src = self.workspace_root.join("src").join(candidate);
                        if in_src.is_file() {
                            fs::metadata(&in_src).map(|m| m.len() > 0).unwrap_or(false)
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                };

                if !modified_files.is_empty() {
                    if let Some(m_idx) = matched_mod_idx {
                        file_satisfied = true;
                        consumed_modified.insert(m_idx);
                        break;
                    }
                } else if on_disk && task.status == TaskItemStatus::InProgress {
                    file_satisfied = true;
                    break;
                }
            }

            // Initial foundation task reconciliation: if the initial active step (index 0) does not name
            // a specific sub-component file and a root project manifest was created/modified, mark it satisfied.
            if !file_satisfied
                && candidates.is_empty()
                && task_idx == 0
                && task.status == TaskItemStatus::InProgress
            {
                if !modified_files.is_empty() {
                    if let Some(m_idx) = modified_files.iter().position(|m| {
                        let base = Path::new(m)
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or(m);
                        matches!(
                            base,
                            "package.json"
                                | "Cargo.toml"
                                | "pyproject.toml"
                                | "go.mod"
                                | "minikit.json"
                                | "index.html"
                        )
                    }) {
                        file_satisfied = true;
                        consumed_modified.insert(m_idx);
                    }
                } else if crate::agent::orchestrator::WorkflowRouter::project_manifest(
                    &self.workspace_root,
                )
                .is_some()
                {
                    file_satisfied = true;
                }
            }

            // In-file HTML `<section id="...">` / `<nav>` / `<footer>` DOM ID reconciliation:
            if !file_satisfied && !html_section_ids.is_empty() {
                let matched_ids: Vec<&String> = html_section_ids
                    .iter()
                    .filter(|sec_id| Self::stem_token_matches_task(sec_id, primary_head))
                    .collect();
                if !matched_ids.is_empty() {
                    file_satisfied = true;
                }
            }

            // Identifier-Stem Token Alignment: if no literal filename matched,
            // check if an unconsumed modified file's CamelCase/identifier stem tokens match
            // this task with high specificity across the plan.
            if !file_satisfied && !modified_files.is_empty() {
                for (m_idx, m_path) in modified_files.iter().enumerate() {
                    if consumed_modified.contains(&m_idx) {
                        continue;
                    }
                    let stem_tokens = Self::split_identifier_stem_tokens(m_path);
                    if stem_tokens.is_empty() {
                        continue;
                    }
                    // Generic entry/stylesheet stems ("style", "index", "main", "app") should only
                    // match the currently active (InProgress) task or a task whose primary heading
                    // explicitly names them, never jumping ahead to a Pending composite task.
                    let is_generic_stem = stem_tokens.len() == 1
                        && matches!(
                            stem_tokens[0].as_str(),
                            "style" | "styles" | "index" | "main" | "app" | "global" | "globals"
                        );
                    let target_text =
                        if is_generic_stem && task.status != TaskItemStatus::InProgress {
                            primary_head
                        } else {
                            task_lower.as_str()
                        };
                    let matched_count = stem_tokens
                        .iter()
                        .filter(|tok| Self::stem_token_matches_task(tok, target_text))
                        .count();
                    if matched_count == 0 {
                        continue;
                    }
                    // Specificity (IDF) check: count how many uncompleted tasks have >= matched_count
                    let competing_tasks = tasks
                        .iter()
                        .filter(|other| {
                            other.status != TaskItemStatus::Completed && {
                                let other_lower = other.title.to_ascii_lowercase();
                                let other_head = other_lower
                                    .split([':', '—'])
                                    .next()
                                    .unwrap_or(&other_lower)
                                    .trim();
                                let other_target = if is_generic_stem
                                    && other.status != TaskItemStatus::InProgress
                                {
                                    other_head
                                } else {
                                    other_lower.as_str()
                                };
                                let c = stem_tokens
                                    .iter()
                                    .filter(|tok| Self::stem_token_matches_task(tok, other_target))
                                    .count();
                                c >= matched_count
                            }
                        })
                        .count();
                    let is_specific_match = competing_tasks == 1
                        || (competing_tasks <= 2 && task.status == TaskItemStatus::InProgress);
                    if is_specific_match {
                        file_satisfied = true;
                        consumed_modified.insert(m_idx);
                        break;
                    }
                }
            }

            if file_satisfied {
                if let Ok(()) = self.update_progress(&task.title, "completed") {
                    reconciled += 1;
                }
            }
        }

        let final_tasks = self.read_parsed_tasks();
        if !final_tasks.is_empty()
            && final_tasks
                .iter()
                .all(|t| t.status == TaskItemStatus::Completed)
        {
            let p_path = self.progress_path();
            if let Ok(content) = fs::read_to_string(&p_path) {
                if content.contains("- Status: In Progress") {
                    let updated =
                        content.replacen("- Status: In Progress", "- Status: Completed (100%)", 1);
                    let _ = fs::write(&p_path, updated);
                }
            }
        }

        reconciled
    }

    /// Ensures `.minicode/NOTES.md` exists with the 4 senior engineer quadrants.
    pub fn ensure_notes_template(&self) -> Result<()> {
        let path = self.notes_path();
        if path.exists() {
            return Ok(());
        }

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| ContextError::Memory(e.to_string()))?;
        }

        let template = r#"# 📝 ACTIVE ENGINEERING SCRATCHPAD (.minicode/NOTES.md)

## 1. Architectural Invariants & Constraints

## 2. Active Hypotheses & Evidence

## 3. Negative Knowledge (DO NOT ATTEMPT)

## 4. Working Set Anchors & Offsets
"#;

        fs::write(&path, template).map_err(|e| ContextError::Memory(e.to_string()))?;
        Ok(())
    }

    /// Reads the senior engineer scratchpad (`.minicode/NOTES.md`).
    pub fn read_notes(&self) -> Option<String> {
        let path = self.notes_path();
        match fs::read_to_string(&path) {
            Ok(content) => {
                let trimmed = content.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            }
            Err(_) => None,
        }
    }

    /// Appends a note into a specific quadrant of `.minicode/NOTES.md`.
    pub fn append_note(&self, section: NoteSection, note: &str) -> Result<()> {
        self.ensure_notes_template()?;
        let path = self.notes_path();
        let content = fs::read_to_string(&path).map_err(|e| ContextError::Memory(e.to_string()))?;

        let heading = section.heading();
        let note_line = format!("- {}\n", note.trim());

        let updated = if let Some(pos) = content.find(heading) {
            let insert_pos = pos + heading.len();
            let mut out = String::with_capacity(content.len() + note_line.len() + 2);
            out.push_str(&content[..insert_pos]);
            out.push('\n');
            out.push_str(&note_line);
            out.push_str(&content[insert_pos..]);
            out
        } else {
            format!("{}\n\n{}\n{}\n", content.trim_end(), heading, note_line)
        };

        fs::write(&path, updated).map_err(|e| ContextError::Memory(e.to_string()))?;
        Ok(())
    }

    /// Archives the active plan once all steps are completed.
    pub fn archive_plan(&self) -> Result<Option<PathBuf>> {
        if !self.has_active_plan() {
            return Ok(None);
        }

        let archive_dir = self.archive_dir();
        fs::create_dir_all(&archive_dir).map_err(|e| ContextError::Memory(e.to_string()))?;

        let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
        let archive_file = archive_dir.join(format!("{}_archived_plan.md", timestamp));

        let plan_raw = fs::read_to_string(self.task_plan_path()).unwrap_or_default();
        let plan = plan_raw
            .lines()
            .map(|l| {
                if l.contains("[>]") || l.contains("[/]") || l.contains("[ ]") {
                    l.replace("[>]", "[x]")
                        .replace("[/]", "[x]")
                        .replace("[ ]", "[x]")
                } else {
                    l.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        let progress_raw = fs::read_to_string(self.progress_path()).unwrap_or_default();
        let progress =
            progress_raw.replacen("- Status: In Progress", "- Status: Completed (100%)", 1);
        let findings = fs::read_to_string(self.findings_path()).unwrap_or_default();

        let combined = format!(
            "# Archived Plan ({})\n\n{}\n\n---\n\n{}\n\n---\n\n{}",
            timestamp, plan, progress, findings
        );

        fs::write(&archive_file, combined).map_err(|e| ContextError::Memory(e.to_string()))?;

        let _ = fs::remove_file(self.task_plan_path());
        let _ = fs::remove_file(self.progress_path());
        let _ = fs::remove_file(self.findings_path());
        let _ = fs::remove_file(
            self.workspace_root
                .join(crate::constants::DEFAULT_INTENT_PERSISTENCE_FILE),
        );

        // Synchronize canonical todo.md: mark all remaining tasks as completed [x]
        let todo_path = self.canonical_todo_path();
        if todo_path.exists() {
            if let Ok(content) = fs::read_to_string(&todo_path) {
                let mut updated_lines = Vec::new();
                for line in content.lines() {
                    if line.contains("[>]") || line.contains("[/]") || line.contains("[ ]") {
                        updated_lines.push(
                            line.replace("[>]", "[x]")
                                .replace("[/]", "[x]")
                                .replace("[ ]", "[x]"),
                        );
                    } else {
                        updated_lines.push(line.to_string());
                    }
                }
                let _ = fs::write(&todo_path, updated_lines.join("\n"));
            }
        }

        Ok(Some(archive_file))
    }

    /// Renders active working memory as a unified, senior-engineer prompt block.
    pub fn to_prompt_block(&self) -> String {
        let has_plan = self.has_active_plan()
            || self
                .read_roadmap_todo()
                .map(|o| o.is_some())
                .unwrap_or(false);
        if !has_plan && self.read_notes().is_none() {
            return String::new();
        }

        let mut block = String::with_capacity(1024);
        block.push_str("<working_memory>\n");

        let impl_path = self.canonical_implementation_path();
        if impl_path.exists() {
            let rel = impl_path
                .strip_prefix(&self.workspace_root)
                .unwrap_or(&impl_path);
            block.push_str(&format!(
                "  <!-- Implementation Plan Blueprint: {} -->\n",
                rel.display()
            ));
        }

        let tasks = self.read_parsed_tasks();
        if !tasks.is_empty() {
            let completed: Vec<&TaskItem> = tasks
                .iter()
                .filter(|t| t.status == TaskItemStatus::Completed)
                .collect();
            let in_progress: Vec<&TaskItem> = tasks
                .iter()
                .filter(|t| t.status == TaskItemStatus::InProgress)
                .collect();
            let pending: Vec<&TaskItem> = tasks
                .iter()
                .filter(|t| t.status == TaskItemStatus::Pending)
                .collect();

            block.push_str("  <live_execution_plan>\n");
            if !completed.is_empty() {
                block.push_str(&format!(
                    "    <completed_milestones count=\"{}\">\n",
                    completed.len()
                ));
                // Show up to last 3 completed
                let start = completed.len().saturating_sub(3);
                for task in &completed[start..] {
                    block.push_str(&format!("      ✔ {}\n", task.title));
                }
                block.push_str("    </completed_milestones>\n");
            }

            if let Some(active) = in_progress.first() {
                block.push_str(&format!(
                    "    <active_task status=\"IN_PROGRESS\">\n      ▶ {}\n    </active_task>\n",
                    active.title
                ));
            } else if let Some(next) = pending.first() {
                block.push_str(&format!(
                    "    <active_task status=\"NEXT_UP\">\n      ▶ {}\n    </active_task>\n",
                    next.title
                ));
            }

            if !pending.is_empty() {
                let skip = if in_progress.is_empty() { 1 } else { 0 };
                let remaining: Vec<&TaskItem> = pending.into_iter().skip(skip).take(4).collect();
                if !remaining.is_empty() {
                    block.push_str("    <next_pending_tasks>\n");
                    for task in remaining {
                        block.push_str(&format!("      ○ {}\n", task.title));
                    }
                    block.push_str("    </next_pending_tasks>\n");
                }
            }
            block.push_str("  </live_execution_plan>\n");
        } else if let Ok(Some(plan)) = self.read_plan() {
            block.push_str("# Active Task Plan:\n");
            for line in plan.lines().take(MAX_PLAN_LINES_IN_PROMPT) {
                block.push_str(line);
                block.push('\n');
            }
        }

        if let Some(notes) = self.read_notes() {
            block.push_str("  <senior_engineer_scratchpad>\n");
            for line in notes.lines().take(30) {
                block.push_str("    ");
                block.push_str(line);
                block.push('\n');
            }
            block.push_str("  </senior_engineer_scratchpad>\n");
        }

        if let Ok(progress) = fs::read_to_string(self.progress_path()) {
            let lines: Vec<&str> = progress.lines().collect();
            if lines.len() > 2 {
                block.push_str("\n# Recent Progress:\n");
                let recent = if lines.len() > PROGRESS_TRUNCATE_THRESHOLD {
                    &lines[lines.len() - PROGRESS_TRUNCATE_THRESHOLD..]
                } else {
                    &lines[..]
                };
                for line in recent {
                    block.push_str(line);
                    block.push('\n');
                }
            }
        }

        block.push_str("</working_memory>");
        block
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_working_memory_lifecycle() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_test_wm_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&temp_dir).unwrap();

        let wm = WorkingMemory::new(&temp_dir);
        assert!(!wm.has_active_plan());

        // Initialize plan
        let steps = vec![
            "Scaffold API endpoints".to_string(),
            "Write integration tests".to_string(),
            "Run cargo clippy".to_string(),
        ];
        wm.init_plan("Build v0.1.0 API", &steps).unwrap();
        assert!(wm.has_active_plan());

        // Read plan
        let plan = wm.read_plan().unwrap().unwrap();
        assert!(plan.contains("Build v0.1.0 API"));
        assert!(plan.contains("1. [>] Scaffold API endpoints"));

        // Append finding
        wm.append_finding("Discovered existing axum router in src/routes.rs")
            .unwrap();
        let findings = fs::read_to_string(wm.findings_path()).unwrap();
        assert!(findings.contains("axum router in src/routes.rs"));

        // Update progress & check box
        wm.update_progress("Scaffold API endpoints", "Completed")
            .unwrap();
        let updated_plan = wm.read_plan().unwrap().unwrap();
        assert!(updated_plan.contains("[x] Scaffold API endpoints"));

        // Prompt block formatting
        let block = wm.to_prompt_block();
        assert!(block.contains("<working_memory>"));

        // Archive plan
        let archived = wm.archive_plan().unwrap().unwrap();
        assert!(archived.exists());
        assert!(!wm.has_active_plan());

        fs::remove_dir_all(temp_dir).ok();
    }

    #[test]
    fn test_senior_engineer_scratchpad_and_task_parsing() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_test_notes_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&temp_dir).unwrap();

        let wm = WorkingMemory::new(&temp_dir);

        // Scratchpad tests
        assert!(wm.read_notes().is_none());
        wm.append_note(
            NoteSection::Invariants,
            "Must use rustls-tls-webpki-roots for pure-Rust portability",
        )
        .unwrap();
        wm.append_note(
            NoteSection::NegativeKnowledge,
            "Do not use walkdir crate due to repository policy",
        )
        .unwrap();

        let notes = wm.read_notes().unwrap();
        assert!(notes.contains("Must use rustls-tls-webpki-roots"));
        assert!(notes.contains("Do not use walkdir crate"));

        // Test todo.md parsing
        let todo_path = temp_dir.join("todo.md");
        fs::write(
            &todo_path,
            "- [x] Task 1: Initialize AST Graph\n- [>] Task 2: Implement 4D Scorer\n- [ ] Task 3: Run Benchmarks\n",
        )
        .unwrap();

        let tasks = wm.read_parsed_tasks();
        assert_eq!(tasks.len(), 3);
        assert_eq!(tasks[0].status, TaskItemStatus::Completed);
        assert_eq!(tasks[1].status, TaskItemStatus::InProgress);
        assert_eq!(tasks[2].status, TaskItemStatus::Pending);

        let block = wm.to_prompt_block();
        assert!(block.contains("<live_execution_plan>"));
        assert!(block.contains("✔ Task 1: Initialize AST Graph"));
        assert!(block.contains("▶ Task 2: Implement 4D Scorer"));
        assert!(block.contains("○ Task 3: Run Benchmarks"));
        assert!(block.contains("<senior_engineer_scratchpad>"));

        fs::remove_dir_all(temp_dir).ok();
    }

    #[test]
    fn test_roadmap_milestones_and_active_scoping() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_test_roadmap_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&temp_dir).unwrap();

        let wm = WorkingMemory::new(&temp_dir);
        let todo_path = temp_dir.join("todo.md");

        // 1. Multi-phase file with Phase 1 done, Phase 2 in progress, Phase 3 pending
        let multi_phase_content = r#"
### Phase 140: MiniDev Runtime
- [x] 140.1: Process isolation
- [x] 140.2: Port scanner

### Phase 141: In-TUI Process Monitor
- [x] 141.1: Interactive modal
- [>] 141.2: Status indicators
- [ ] 141.3: Ring-buffer logs
- [ ] 141.4: Telemetry

### Phase 142: Two-Tier Plan
- [ ] 142.1: Scoped WorkingMemory
"#;
        fs::write(&todo_path, multi_phase_content).unwrap();

        let milestones = wm.read_roadmap_milestones();
        assert_eq!(milestones.len(), 3);

        // Phase 140
        assert_eq!(milestones[0].id, "Phase 140");
        assert_eq!(milestones[0].title, "MiniDev Runtime");
        assert_eq!(milestones[0].total_tasks, 2);
        assert_eq!(milestones[0].completed_tasks, 2);
        assert!(!milestones[0].is_active);

        // Phase 141 should be marked active because it has an InProgress task
        assert_eq!(milestones[1].id, "Phase 141");
        assert_eq!(milestones[1].total_tasks, 4);
        assert_eq!(milestones[1].completed_tasks, 1);
        assert_eq!(milestones[1].in_progress_tasks, 1);
        assert_eq!(milestones[1].pending_tasks, 2);
        assert!(milestones[1].is_active);

        // Phase 142
        assert!(!milestones[2].is_active);

        // Scoped active tasks should return Phase 141 only
        let (phase_label, active_tasks) = wm.read_active_phase_tasks();
        assert_eq!(
            phase_label,
            Some("Phase 141: In-TUI Process Monitor".to_string())
        );
        assert_eq!(active_tasks.len(), 4);
        assert_eq!(active_tasks[0].status, TaskItemStatus::Completed);
        assert_eq!(active_tasks[1].status, TaskItemStatus::InProgress);

        // 2. Mark all tasks completed: active tasks should return None
        let all_done_content = r#"
### Phase 140: MiniDev Runtime
- [x] 140.1: Process isolation
- [x] 140.2: Port scanner
"#;
        fs::write(&todo_path, all_done_content).unwrap();
        let (phase_label_done, active_tasks_done) = wm.read_active_phase_tasks();
        assert_eq!(phase_label_done, None);
        assert!(active_tasks_done.is_empty());

        fs::remove_dir_all(temp_dir).ok();
    }

    #[test]
    fn test_update_progress_index_matching_and_auto_advancement() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_test_adv_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&temp_dir).unwrap();

        let wm = WorkingMemory::new(&temp_dir);
        let steps = vec![
            "1. Setup environment".to_string(),
            "2. Implement backend service".to_string(),
            "3. Verify endpoints".to_string(),
        ];
        wm.init_plan("Test Service Pipeline", &steps).unwrap();

        let tasks = wm.read_parsed_tasks();
        assert_eq!(tasks.len(), 3);
        assert_eq!(tasks[0].title, "Setup environment");
        assert_eq!(tasks[0].status, TaskItemStatus::InProgress);
        assert_eq!(tasks[1].status, TaskItemStatus::Pending);

        // 1. Update by 1-based index "1" to completed -> should auto-advance step 2 to InProgress!
        wm.update_progress("1", "completed").unwrap();
        let tasks_after_1 = wm.read_parsed_tasks();
        assert_eq!(tasks_after_1[0].status, TaskItemStatus::Completed);
        assert_eq!(tasks_after_1[1].status, TaskItemStatus::InProgress);
        assert_eq!(tasks_after_1[2].status, TaskItemStatus::Pending);

        // 2. Update by keyword "active" to completed -> should auto-advance step 3 to InProgress!
        wm.update_progress("active", "completed").unwrap();
        let tasks_after_2 = wm.read_parsed_tasks();
        assert_eq!(tasks_after_2[1].status, TaskItemStatus::Completed);
        assert_eq!(tasks_after_2[2].status, TaskItemStatus::InProgress);

        // 3. Update by title substring to completed
        wm.update_progress("Verify endpoints", "completed").unwrap();
        let tasks_after_3 = wm.read_parsed_tasks();
        assert_eq!(tasks_after_3[2].status, TaskItemStatus::Completed);

        // 4. Update non-existent step -> must return helpful error
        let err = wm.update_progress("non_existent_step_xyz", "completed");
        assert!(err.is_err());
        let err_msg = err.unwrap_err().to_string();
        assert!(err_msg.contains("No matching task step found"));

        fs::remove_dir_all(temp_dir).ok();
    }

    #[test]
    fn test_update_progress_space_normalization_and_leading_index() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_test_norm_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&temp_dir).unwrap();

        let wm = WorkingMemory::new(&temp_dir);
        let steps = vec![
            "Research components".to_string(),
            "Set up structure".to_string(),
            "Build navbar".to_string(),
            "Build hero".to_string(),
            "Build features".to_string(),
            "Build pricing".to_string(),
            "Build testimonials".to_string(),
            "Build contact".to_string(),
            "Add footer".to_string(),
            "Test and verify all components render correctly".to_string(),
        ];
        wm.init_plan("NovaCode Landing", &steps).unwrap();

        // Verify canonical todo.md was created in minikit_docs/core/todo.md
        let canonical_todo = temp_dir.join("minikit_docs").join("core").join("todo.md");
        assert!(canonical_todo.exists());

        // 1. Update with status "In Progress" (with space) on "10. Test and verify all components render correctly"
        wm.update_progress(
            "10. Test and verify all components render correctly",
            "In Progress",
        )
        .unwrap();

        let tasks = wm.read_parsed_tasks();
        assert_eq!(tasks.len(), 10);
        assert_eq!(tasks[9].status, TaskItemStatus::InProgress);

        // 2. Update with status "Completed"
        wm.update_progress(
            "10. Test and verify all components render correctly",
            "Completed",
        )
        .unwrap();
        let tasks2 = wm.read_parsed_tasks();
        assert_eq!(tasks2[9].status, TaskItemStatus::Completed);

        // 3. Test invalid status produces clear error
        let err = wm.update_progress("1", "unknown_bogus_status");
        assert!(err.is_err());
        let err_msg = err.unwrap_err().to_string();
        assert!(err_msg.contains("Invalid task status"));

        fs::remove_dir_all(temp_dir).ok();
    }

    #[test]
    fn test_read_active_plan_summary_all_completed() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_test_summary_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&temp_dir).unwrap();

        let wm = WorkingMemory::new(&temp_dir);
        let steps = vec![
            "Initial setup".to_string(),
            "Core implementation".to_string(),
            "Unit tests".to_string(),
            "Verification and docs".to_string(),
        ];
        wm.init_plan("Test Plan", &steps).unwrap();

        // Mark all 4 steps completed
        for step in &steps {
            wm.update_progress(step, "completed").unwrap();
        }

        // Active plan summary must report 4/4 completed, active_task: None, and all tasks completed
        let summary = wm
            .read_active_plan_summary()
            .expect("Summary must exist when milestones are present");
        assert_eq!(summary.total_tasks, 4);
        assert_eq!(summary.completed_tasks, 4);
        assert_eq!(summary.in_progress_tasks, 0);
        assert_eq!(summary.pending_tasks, 0);
        assert_eq!(summary.active_task, None);
        assert_eq!(summary.tasks.len(), 4);
        for t in &summary.tasks {
            assert_eq!(t.status, TaskItemStatus::Completed);
        }

        fs::remove_dir_all(temp_dir).ok();
    }

    #[test]
    fn test_reconcile_workspace_tasks_dynamic_deliverables() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_test_reconcile_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&temp_dir).unwrap();

        let wm = WorkingMemory::new(&temp_dir);
        let steps = vec![
            "Create index.html — semantic HTML structure".to_string(),
            "Create styles.css — dark cyber theme and layout".to_string(),
            "Create main.js — terminal preview simulation and animations".to_string(),
            "Verify responsive layout in browser".to_string(),
        ];
        wm.init_plan("Landing Page Plan", &steps).unwrap();

        // Initially no files exist -> 0 reconciled
        let count0 = wm.reconcile_workspace_tasks(&[]);
        assert_eq!(count0, 0);

        // Write index.html to disk
        fs::write(temp_dir.join("index.html"), "<!DOCTYPE html><html></html>").unwrap();
        let count1 = wm.reconcile_workspace_tasks(&["index.html".to_string()]);
        assert_eq!(count1, 1);

        let tasks1 = wm.read_parsed_tasks();
        assert_eq!(tasks1[0].status, TaskItemStatus::Completed);
        assert_eq!(tasks1[1].status, TaskItemStatus::InProgress);

        // Now write styles.css and main.js to disk
        fs::write(temp_dir.join("styles.css"), "body { margin: 0; }").unwrap();
        fs::write(temp_dir.join("main.js"), "console.log('ready');").unwrap();

        // Turn ends with main.js modified -> reconciles tasks 2 and 3!
        let count2 =
            wm.reconcile_workspace_tasks(&["styles.css".to_string(), "main.js".to_string()]);
        assert_eq!(count2, 2);

        let tasks2 = wm.read_parsed_tasks();
        assert_eq!(tasks2[0].status, TaskItemStatus::Completed);
        assert_eq!(tasks2[1].status, TaskItemStatus::Completed);
        assert_eq!(tasks2[2].status, TaskItemStatus::Completed);
        assert_eq!(tasks2[3].status, TaskItemStatus::InProgress);

        // Explicit verification step requires update_progress (no silent background auto-completion)
        wm.update_progress("Verify responsive layout in browser", "completed")
            .unwrap();

        let final_tasks = wm.read_parsed_tasks();
        assert_eq!(final_tasks.len(), 4);
        assert_eq!(final_tasks[0].status, TaskItemStatus::Completed);
        assert_eq!(final_tasks[1].status, TaskItemStatus::Completed);
        assert_eq!(final_tasks[2].status, TaskItemStatus::Completed);
        assert_eq!(final_tasks[3].status, TaskItemStatus::Completed);

        let summary = wm.read_active_plan_summary().unwrap();
        assert_eq!(summary.total_tasks, 4);
        assert_eq!(summary.completed_tasks, 4);
        assert_eq!(summary.in_progress_tasks, 0);
        assert_eq!(summary.pending_tasks, 0);

        fs::remove_dir_all(temp_dir).ok();
    }

    #[test]
    fn test_reconcile_workspace_tasks_semantic_and_single_step() {
        let temp_dir = std::env::temp_dir().join(format!(
            "minicode_test_reconcile_semantic_{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&temp_dir).unwrap();

        let wm = WorkingMemory::new(&temp_dir);
        let steps = vec![
            "Scaffold project structure".to_string(),
            "Implement modern hero component in src/Hero.tsx".to_string(),
            "Verify all features".to_string(),
        ];
        wm.init_plan("Semantic Plan", &steps).unwrap();

        // 1. Scaffolding task matched by package.json creation
        fs::write(temp_dir.join("package.json"), "{}").unwrap();
        let c1 = wm.reconcile_workspace_tasks(&["package.json".to_string()]);
        assert_eq!(c1, 1);
        let tasks = wm.read_parsed_tasks();
        assert_eq!(tasks[0].status, TaskItemStatus::Completed);
        assert_eq!(tasks[1].status, TaskItemStatus::InProgress);

        // 2. Candidate file src/Hero.tsx created -> reconciles hero component
        let src_dir = temp_dir.join("src");
        fs::create_dir_all(&src_dir).unwrap();
        fs::write(src_dir.join("Hero.tsx"), "export const Hero = () => null;").unwrap();
        let c2 = wm.reconcile_workspace_tasks(&["src/Hero.tsx".to_string()]);
        assert_eq!(c2, 1);
        let tasks_after_hero = wm.read_parsed_tasks();
        assert_eq!(tasks_after_hero[0].status, TaskItemStatus::Completed);
        assert_eq!(tasks_after_hero[1].status, TaskItemStatus::Completed);
        assert_eq!(tasks_after_hero[2].status, TaskItemStatus::InProgress);

        // 3. Verification task completed explicitly
        wm.update_progress("Verify all features", "completed")
            .unwrap();

        let final_tasks = wm.read_parsed_tasks();
        assert_eq!(final_tasks[0].status, TaskItemStatus::Completed);
        assert_eq!(final_tasks[1].status, TaskItemStatus::Completed);
        assert_eq!(final_tasks[2].status, TaskItemStatus::Completed);

        let summary = wm.read_active_plan_summary().unwrap();
        assert_eq!(summary.total_tasks, 3);
        assert_eq!(summary.completed_tasks, 3);
        assert_eq!(summary.in_progress_tasks, 0);
        assert_eq!(summary.pending_tasks, 0);

        let progress_content = fs::read_to_string(wm.progress_path()).unwrap();
        assert!(progress_content.contains("- Status: Completed (100%)"));

        fs::remove_dir_all(temp_dir).ok();
    }

    #[test]
    fn test_two_tier_planning_architecture_and_switching() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_test_two_tier_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&temp_dir).unwrap();

        let wm = WorkingMemory::new(&temp_dir);
        let steps = vec![
            "Setup Tailwind and icons in src/index.css".to_string(),
            "Build Navbar in src/components/Navbar.tsx".to_string(),
        ];
        let milestones = vec![
            "Core UI Components".to_string(),
            "Interactive States & Polish".to_string(),
        ];
        let architecture = "# Blueprint\nTop-to-bottom architecture.";

        wm.init_two_tier_plan(
            "Two-Tier Web App",
            &steps,
            Some(architecture),
            Some(&milestones),
        )
        .unwrap();

        // 1. Verify implementation.md was written
        let impl_file = wm.canonical_implementation_path();
        assert!(impl_file.exists());
        let impl_content = fs::read_to_string(&impl_file).unwrap();
        assert!(impl_content.contains("# Blueprint"));

        // 2. Verify todo.md was written with roadmap milestones
        let todo_file = wm.canonical_todo_path();
        assert!(todo_file.exists());
        let todo_content = fs::read_to_string(&todo_file).unwrap();
        assert!(todo_content.contains("Core UI Components"));
        assert!(todo_content.contains("Interactive States & Polish"));

        // 3. While tactical tasks are pending in task_plan.md, read_active_plan_summary returns the tactical steps
        let summary1 = wm.read_active_plan_summary().unwrap();
        assert_eq!(summary1.total_tasks, 2);
        assert_eq!(summary1.completed_tasks, 0);

        // 4. Complete first tactical step
        wm.update_progress("Setup Tailwind and icons in src/index.css", "completed")
            .unwrap();
        let summary2 = wm.read_active_plan_summary().unwrap();
        assert_eq!(summary2.completed_tasks, 1);

        // 5. Complete second tactical step -> all tactical steps in task_plan.md are now done!
        wm.update_progress("Build Navbar in src/components/Navbar.tsx", "completed")
            .unwrap();

        // 6. After tactical tasks complete, milestone 1 in todo.md was marked completed [x],
        // and summary switches to displaying the roadmap milestones (showing Phase 2 next)!
        let summary3 = wm.read_active_plan_summary().unwrap();
        assert!(
            summary3.phase_label.contains("Phase 2")
                || summary3.phase_label.contains("Interactive States")
                || summary3.phase_label.contains("Two-Tier")
        );

        fs::remove_dir_all(temp_dir).ok();
    }

    #[test]
    fn test_update_progress_auto_bootstrap_when_no_plan() {
        let temp_dir = std::env::temp_dir().join(format!(
            "minicode_test_auto_bootstrap_{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&temp_dir).unwrap();

        let wm = WorkingMemory::new(&temp_dir);
        // Initially no plan exists
        assert!(wm.read_parsed_tasks().is_empty());

        // Calling update_progress directly should auto-bootstrap the plan and mark step completed
        let res = wm.update_progress("1. Initial scaffold of project", "completed");
        assert!(res.is_ok());

        let tasks = wm.read_parsed_tasks();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].status, TaskItemStatus::Completed);

        let progress_content = fs::read_to_string(wm.progress_path()).unwrap();
        assert!(progress_content.contains("- Status: Completed (100%)"));

        fs::remove_dir_all(temp_dir).ok();
    }

    #[test]
    fn test_reconcile_in_file_html_sections_and_bulk_remaining() {
        let temp_dir = std::env::temp_dir().join(format!(
            "minicode_test_html_sections_{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&temp_dir).unwrap();

        let wm = WorkingMemory::new(&temp_dir);
        let steps = vec![
            "Scaffold project and core files".to_string(),
            "Build Hero section with terminal preview".to_string(),
            "Build Philosophy section".to_string(),
            "Build Autonomous Swarm section".to_string(),
            "Final testing and verification".to_string(),
        ];
        wm.init_plan("Landing Page Plan", &steps).unwrap();

        // Write index.html containing semantic section IDs/comments
        let html_content = r#"<!DOCTYPE html>
<html>
<head><title>minicode</title></head>
<body>
  <!-- HERO -->
  <section id="hero" class="hero"><h1>Hero</h1></section>
  <!-- PHILOSOPHY -->
  <section id="philosophy"><h2>Philosophy</h2></section>
  <!-- SWARM -->
  <section id="swarm"><h2>Swarm</h2></section>
</body>
</html>"#;
        fs::write(temp_dir.join("index.html"), html_content).unwrap();

        // Reconcile workspace tasks against index.html (verifies active deliverable without silent skips)
        let reconciled = wm.reconcile_workspace_tasks(&["index.html".to_string()]);
        assert!(
            reconciled >= 1,
            "Expected at least 1 task reconciled for active deliverable, got {}",
            reconciled
        );

        let tasks = wm.read_parsed_tasks();
        assert_eq!(tasks[0].status, TaskItemStatus::Completed);

        // Verify bulk completion via "remaining" is strictly rejected
        let res = wm.update_progress("remaining", "completed");
        assert!(res.is_err(), "Bulk completion must be rejected");

        // Complete remaining tasks individually
        for idx in 2..=5 {
            let res = wm.update_progress(&idx.to_string(), "completed");
            assert!(res.is_ok());
        }

        let final_tasks = wm.read_parsed_tasks();
        assert_eq!(final_tasks[4].status, TaskItemStatus::Completed);
        let summary = wm.read_active_plan_summary().unwrap();
        assert_eq!(summary.completed_tasks, 5);
        assert_eq!(summary.total_tasks, 5);

        fs::remove_dir_all(temp_dir).ok();
    }

    #[test]
    fn test_init_tactical_subplan_and_archive_completion() {
        let temp_dir = std::env::temp_dir().join(format!(
            "minicode_test_tactical_subplan_{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&temp_dir).unwrap();

        let wm = WorkingMemory::new(&temp_dir);
        let macro_tasks = vec![
            "Create landing page hero section".to_string(),
            "Build interactive bento features".to_string(),
        ];
        wm.init_plan("Landing Page", &macro_tasks).unwrap();

        // Decompose active task 1 into tactical sub-steps via init_tactical_subplan
        let sub_steps = vec![
            "Check theme tokens in index.css".to_string(),
            "Implement HeroSection.tsx with terminal preview".to_string(),
        ];
        wm.init_tactical_subplan("Create landing page hero section", &sub_steps)
            .unwrap();

        let summary = wm.read_active_plan_summary().unwrap();
        assert_eq!(summary.total_tasks, 2);
        assert_eq!(summary.completed_tasks, 0);
        assert_eq!(
            summary.active_task.as_deref(),
            Some("Check theme tokens in index.css")
        );

        // Complete sub-step 1 -> macro task 1 in todo.md must remain [>]
        wm.update_progress("1", "completed").unwrap();
        let todo_mid = fs::read_to_string(wm.canonical_todo_path()).unwrap();
        assert!(todo_mid.contains("- [>] Create landing page hero section"));

        // Complete sub-step 2 -> macro task 1 in todo.md becomes [x] and macro task 2 becomes [>]
        wm.update_progress("2", "completed").unwrap();
        let todo_after = fs::read_to_string(wm.canonical_todo_path()).unwrap();
        assert!(todo_after.contains("- [x] Create landing page hero section"));
        assert!(todo_after.contains("- [>] Build interactive bento features"));

        // Archive plan -> all remaining tasks in todo.md become [x]
        wm.archive_plan().unwrap();
        let todo_final = fs::read_to_string(wm.canonical_todo_path()).unwrap();
        assert!(todo_final.contains("- [x] Create landing page hero section"));
        assert!(todo_final.contains("- [x] Build interactive bento features"));
        assert!(!todo_final.contains("[>]"));
        assert!(!todo_final.contains("[ ]"));

        fs::remove_dir_all(temp_dir).ok();
    }

    #[test]
    fn test_minitest54_direct_plan_sync_and_stem_reconciliation() {
        let temp_dir = std::env::temp_dir().join(format!(
            "minicode_test_minitest54_sync_{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&temp_dir).unwrap();

        let wm = WorkingMemory::new(&temp_dir);
        let steps = vec![
            "Scaffold static-website foundation".to_string(),
            "Build navigation bar with links and CTA".to_string(),
            "Build hero section with terminal preview animation".to_string(),
            "Build 6-feature bento grid".to_string(),
            "Verify and build production bundle".to_string(),
        ];
        let milestones = vec![
            "Foundation & Structure".to_string(),
            "Hero Section".to_string(),
            "Bento Features".to_string(),
            "Verification".to_string(),
        ];
        wm.init_two_tier_plan("WarpFlow Landing", &steps, None, Some(&milestones))
            .unwrap();

        // 1. Reconcile stack choice when user selects react-vite
        wm.reconcile_stack_choice("react-vite");
        let plan_text = wm.read_task_plan().unwrap().unwrap();
        assert!(plan_text.contains("Scaffold react-vite foundation"));
        assert!(!plan_text.contains("static-website"));

        // 2. Reconcile step 1 via package.json
        fs::write(temp_dir.join("package.json"), "{}").unwrap();
        assert_eq!(
            wm.reconcile_workspace_tasks(&["package.json".to_string()]),
            1
        );

        // 3. Reconcile steps 2, 3, 4 via CamelCase component stems (no literal filenames in step titles)
        assert_eq!(
            wm.reconcile_workspace_tasks(&["src/components/Navbar.tsx".to_string()]),
            1
        );
        assert_eq!(
            wm.reconcile_workspace_tasks(&["src/components/Hero.tsx".to_string()]),
            1
        );
        assert_eq!(
            wm.reconcile_workspace_tasks(&["src/components/FeaturesBento.tsx".to_string()]),
            1
        );

        // 4. Complete final step 5 -> both task_plan.md and todo.md must be 5/5 (100%), never jumping backward to 1/4!
        wm.update_progress("5", "completed").unwrap();
        let summary = wm.read_active_plan_summary().unwrap();
        assert_eq!(summary.total_tasks, 5);
        assert_eq!(summary.completed_tasks, 5);
        assert_eq!(summary.in_progress_tasks, 0);
        assert_eq!(summary.pending_tasks, 0);

        let todo_final = fs::read_to_string(wm.canonical_todo_path()).unwrap();
        assert!(!todo_final.contains("[>]"));
        assert!(!todo_final.contains("[ ]"));

        fs::remove_dir_all(temp_dir).ok();
    }
}

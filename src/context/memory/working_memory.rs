use crate::constants::{
    ARCHIVE_DIR, FINDINGS_FILE, MAX_PLAN_LINES_IN_PROMPT, PLAN_DIR, PROGRESS_FILE,
    PROGRESS_TRUNCATE_THRESHOLD, TASK_PLAN_FILE, TIMESTAMP_FORMAT, WORKSPACE_DIR_NAME,
};
use crate::error::{ContextError, Result};
use chrono::Utc;
use std::fs;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

/// Status of a discrete task item in the execution plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskItemStatus {
    Pending,
    InProgress,
    Completed,
}

/// A parsed atomic task item from `todo.md` or `task_plan.md`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskItem {
    pub line_index: usize,
    pub status: TaskItemStatus,
    pub title: String,
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
        let todo_path = self.canonical_todo_path();
        if todo_path.exists() {
            if let Ok(content) = fs::read_to_string(&todo_path) {
                if content.lines().any(|l| {
                    let t = l.trim();
                    t.starts_with("- [ ]")
                        || t.starts_with("* [ ]")
                        || t.starts_with("- [>]")
                        || t.starts_with("* [>]")
                        || t.starts_with("- [x]")
                        || t.starts_with("* [x]")
                }) {
                    return true;
                }
            }
        }

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

    /// Initializes a new task plan with structured steps.
    pub fn init_plan(&self, title: &str, steps: &[String]) -> Result<()> {
        let dir = self.plan_dir();
        fs::create_dir_all(&dir).map_err(|e| ContextError::Memory(e.to_string()))?;

        let timestamp = Utc::now().format(TIMESTAMP_FORMAT).to_string();

        let mut plan_content = format!(
            "# Task Plan: {}\n\n> Created: {}\n\n## Objectives & Steps:\n",
            title, timestamp
        );
        for (idx, step) in steps.iter().enumerate() {
            plan_content.push_str(&format!("{}. [ ] {}\n", idx + 1, step));
        }

        fs::write(self.task_plan_path(), &plan_content)
            .map_err(|e| ContextError::Memory(e.to_string()))?;

        // Also synchronize with canonical `todo.md` if minikit docs are active
        let todo_path = self.canonical_todo_path();
        if let Some(parent) = todo_path.parent() {
            if parent.exists() {
                let mut todo_content = format!(
                    "# Tasks & Todo: {}\n\n> Initialized: {}\n\n",
                    title, timestamp
                );
                for step in steps {
                    todo_content.push_str(&format!("- [ ] {}\n", step));
                }
                let _ = fs::write(&todo_path, todo_content);
            }
        }

        let initial_progress = format!(
            "# Progress Tracker\n\n> Initialized: {}\n\n- Active Goal: {}\n- Status: In Progress\n",
            timestamp, title
        );
        fs::write(self.progress_path(), initial_progress)
            .map_err(|e| ContextError::Memory(e.to_string()))?;

        let initial_findings = format!(
            "# Discoveries & Code Findings\n\n> Workspace: {}\n\n",
            self.workspace_root.display()
        );
        fs::write(self.findings_path(), initial_findings)
            .map_err(|e| ContextError::Memory(e.to_string()))?;

        // Initialize NOTES.md template if missing
        let _ = self.ensure_notes_template();

        Ok(())
    }

    /// Reads the current active plan if available.
    pub fn read_plan(&self) -> Result<Option<String>> {
        let todo_path = self.canonical_todo_path();
        if todo_path.exists() {
            if let Ok(content) = fs::read_to_string(&todo_path) {
                if !content.trim().is_empty() {
                    return Ok(Some(content));
                }
            }
        }

        let path = self.task_plan_path();
        match fs::read_to_string(&path) {
            Ok(content) => Ok(Some(content)),
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

    /// Parses all discrete task items from the active plan.
    pub fn read_parsed_tasks(&self) -> Vec<TaskItem> {
        let content = match self.read_plan() {
            Ok(Some(c)) => c,
            _ => return Vec::new(),
        };

        let mut tasks = Vec::new();
        for (idx, line) in content.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("- [x]") || trimmed.starts_with("* [x]") {
                let title = trimmed[5..].trim().to_string();
                tasks.push(TaskItem {
                    line_index: idx,
                    status: TaskItemStatus::Completed,
                    title,
                });
            } else if trimmed.starts_with("- [>]") || trimmed.starts_with("* [>]") {
                let title = trimmed[5..].trim().to_string();
                tasks.push(TaskItem {
                    line_index: idx,
                    status: TaskItemStatus::InProgress,
                    title,
                });
            } else if trimmed.starts_with("- [ ]") || trimmed.starts_with("* [ ]") {
                let title = trimmed[5..].trim().to_string();
                tasks.push(TaskItem {
                    line_index: idx,
                    status: TaskItemStatus::Pending,
                    title,
                });
            } else if let Some(bracket) = trimmed.find("[ ]") {
                let title = trimmed[bracket + 3..].trim().to_string();
                tasks.push(TaskItem {
                    line_index: idx,
                    status: TaskItemStatus::Pending,
                    title,
                });
            } else if let Some(bracket) = trimmed.find("[x]") {
                let title = trimmed[bracket + 3..].trim().to_string();
                tasks.push(TaskItem {
                    line_index: idx,
                    status: TaskItemStatus::Completed,
                    title,
                });
            }
        }
        tasks
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
    pub fn update_progress(&self, step: &str, status: &str) -> Result<()> {
        let dir = self.plan_dir();
        fs::create_dir_all(&dir).map_err(|e| ContextError::Memory(e.to_string()))?;

        let timestamp = Utc::now().format(TIMESTAMP_FORMAT).to_string();
        let entry = format!("\n- [{}] **{}**: {}\n", timestamp, status, step);

        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.progress_path())
            .map_err(|e| ContextError::Memory(e.to_string()))?;

        file.write_all(entry.as_bytes())
            .map_err(|e| ContextError::Memory(e.to_string()))?;

        let is_done =
            status.eq_ignore_ascii_case("completed") || status.eq_ignore_ascii_case("done");
        let is_active =
            status.eq_ignore_ascii_case("in_progress") || status.eq_ignore_ascii_case("active");

        let step_trimmed = step.trim();

        // Helper to update a target markdown plan file
        let update_file = |path: &Path| {
            if !path.exists() {
                return;
            }
            if let Ok(content) = fs::read_to_string(path) {
                let mut updated_lines = Vec::new();
                let mut modified = false;
                for line in content.lines() {
                    let trimmed = line.trim();
                    if !modified && trimmed.contains(step_trimmed) {
                        if is_done {
                            if trimmed.contains("[ ]") {
                                updated_lines.push(line.replacen("[ ]", "[x]", 1));
                                modified = true;
                                continue;
                            } else if trimmed.contains("[>]") {
                                updated_lines.push(line.replacen("[>]", "[x]", 1));
                                modified = true;
                                continue;
                            }
                        } else if is_active && trimmed.contains("[ ]") {
                            updated_lines.push(line.replacen("[ ]", "[>]", 1));
                            modified = true;
                            continue;
                        }
                    }
                    updated_lines.push(line.to_string());
                }
                if modified {
                    let updated = updated_lines.join("\n");
                    let _ = fs::write(path, updated);
                }
            }
        };

        update_file(&self.task_plan_path());
        update_file(&self.canonical_todo_path());

        Ok(())
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

        let plan = fs::read_to_string(self.task_plan_path()).unwrap_or_default();
        let progress = fs::read_to_string(self.progress_path()).unwrap_or_default();
        let findings = fs::read_to_string(self.findings_path()).unwrap_or_default();

        let combined = format!(
            "# Archived Plan ({})\n\n{}\n\n---\n\n{}\n\n---\n\n{}",
            timestamp, plan, progress, findings
        );

        fs::write(&archive_file, combined).map_err(|e| ContextError::Memory(e.to_string()))?;

        let _ = fs::remove_file(self.task_plan_path());
        let _ = fs::remove_file(self.progress_path());
        let _ = fs::remove_file(self.findings_path());

        Ok(Some(archive_file))
    }

    /// Renders active working memory as a unified, senior-engineer prompt block.
    pub fn to_prompt_block(&self) -> String {
        if !self.has_active_plan() && self.read_notes().is_none() {
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
        assert!(plan.contains("1. [ ] Scaffold API endpoints"));

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
}

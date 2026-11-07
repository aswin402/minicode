use super::{NoteSection, TaskItem, TaskItemStatus, WorkingMemory};
use crate::constants::{MAX_PLAN_LINES_IN_PROMPT, PROGRESS_TRUNCATE_THRESHOLD};
use crate::error::{ContextError, Result};
use chrono::Utc;
use std::fs;
use std::path::PathBuf;

impl WorkingMemory {
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
        let tasks = self.read_parsed_tasks();
        let has_incomplete_tasks = tasks.iter().any(|t| t.status != TaskItemStatus::Completed);
        let has_plan = if !tasks.is_empty() {
            has_incomplete_tasks || self.has_active_plan()
        } else {
            self.has_active_plan()
        };
        let notes = self.read_notes();
        if !has_plan && notes.is_none() {
            return String::new();
        }

        let mut block = String::with_capacity(1024);
        block.push_str("<working_memory>\n");

        let impl_path = self.canonical_implementation_path();
        if has_plan && impl_path.exists() {
            let rel = impl_path
                .strip_prefix(&self.workspace_root)
                .unwrap_or(&impl_path);
            block.push_str(&format!(
                "  <!-- Implementation Plan Blueprint: {} -->\n",
                rel.display()
            ));
        }

        if has_plan && !tasks.is_empty() {
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

        if let Some(notes) = notes {
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

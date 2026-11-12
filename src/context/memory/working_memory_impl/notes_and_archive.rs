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

        if self.is_tactical_subplan() {
            self.sync_direct_task_plan_from_todo();
        }
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

        // Fold and clean up any active SDD ledger directory (`.minicode/sdd/<slug>/`) associated
        // with this plan so verification artifacts are preserved in the archive and never orphaned.
        let sdd_root = self.workspace_root.join(".minicode").join("sdd");
        let mut sdd_sections = String::new();
        if sdd_root.is_dir() {
            if let Ok(entries) = fs::read_dir(&sdd_root) {
                for entry in entries.flatten() {
                    let sub = entry.path();
                    if sub.is_dir() {
                        if let Ok(files) = fs::read_dir(&sub) {
                            for f in files.flatten() {
                                let fp = f.path();
                                if fp.is_file() {
                                    if let Ok(body) = fs::read_to_string(&fp) {
                                        if !body.trim().is_empty() {
                                            sdd_sections.push_str("\n\n---\n\n");
                                            sdd_sections.push_str(body.trim());
                                        }
                                    }
                                }
                            }
                        }
                        let _ = fs::remove_dir_all(&sub);
                    }
                }
            }
            if fs::read_dir(&sdd_root)
                .map(|mut it| it.next().is_none())
                .unwrap_or(false)
            {
                let _ = fs::remove_dir(&sdd_root);
            }
        }

        let combined = format!(
            "# Archived Plan ({})\n\n{}\n\n---\n\n{}\n\n---\n\n{}{}",
            timestamp, plan, progress, findings, sdd_sections
        );

        fs::write(&archive_file, combined).map_err(|e| ContextError::Memory(e.to_string()))?;

        let _ = fs::remove_file(self.task_plan_path());
        let _ = fs::remove_file(self.progress_path());
        let _ = fs::remove_file(self.findings_path());
        self.clear_verification_evidence();
        let _ = fs::remove_file(
            self.workspace_root
                .join(crate::constants::DEFAULT_INTENT_PERSISTENCE_FILE),
        );

        self.sync_completed_core_docs();

        Ok(Some(archive_file))
    }

    /// Extracts the concise plan goal title from the active `task_plan.md` if present,
    /// or from a plan archived during the current turn (`mtime >= min_archived_mtime`).
    pub fn recent_or_active_plan_goal(
        &self,
        min_archived_mtime: Option<std::time::SystemTime>,
    ) -> Option<String> {
        let extract_goal = |content: &str| -> Option<String> {
            let mut task_plan_title: Option<String> = None;
            for line in content.lines().take(16) {
                let trimmed = line.trim();
                if let Some(root) = trimmed.strip_prefix("> Root Goal:") {
                    let g = root.trim();
                    if !g.is_empty() {
                        return Some(g.to_string());
                    }
                }
                if task_plan_title.is_none() {
                    if let Some(rest) = trimmed.strip_prefix("# Task Plan:") {
                        let g = rest.trim();
                        if !g.is_empty() {
                            task_plan_title = Some(g.to_string());
                        }
                    }
                }
            }
            task_plan_title
        };

        if let Ok(active) = fs::read_to_string(self.task_plan_path()) {
            if let Some(goal) = extract_goal(&active) {
                return Some(goal);
            }
        }

        let arch_dir = self.archive_dir();
        if arch_dir.is_dir() {
            let mut latest_path: Option<(std::time::SystemTime, PathBuf)> = None;
            if let Ok(entries) = fs::read_dir(&arch_dir) {
                for entry in entries.flatten() {
                    let p = entry.path();
                    if p.is_file() {
                        if let Ok(meta) = fs::metadata(&p) {
                            let mtime = meta.modified().unwrap_or(std::time::UNIX_EPOCH);
                            let meets_cutoff = min_archived_mtime.is_none_or(|cutoff| {
                                // Allow 1s filesystem mtime granularity slack
                                mtime
                                    .checked_add(std::time::Duration::from_secs(1))
                                    .unwrap_or(mtime)
                                    >= cutoff
                            });
                            if meets_cutoff
                                && latest_path.as_ref().is_none_or(|(best, _)| mtime > *best)
                            {
                                latest_path = Some((mtime, p));
                            }
                        }
                    }
                }
            }
            if let Some((_, p)) = latest_path {
                if let Ok(archived) = fs::read_to_string(p) {
                    return extract_goal(&archived);
                }
            }
        }
        None
    }

    /// Returns true if all tasks in the active plan AND all roadmap milestone tasks in `todo.md`
    /// (when present) have reached `TaskItemStatus::Completed`.
    pub fn is_all_project_tasks_completed(&self) -> bool {
        let tasks = self.read_parsed_tasks();
        if tasks.is_empty() || !tasks.iter().all(|t| t.status == TaskItemStatus::Completed) {
            return false;
        }
        self.read_roadmap_milestones()
            .iter()
            .all(|m| m.in_progress_tasks == 0 && m.pending_tasks == 0)
    }

    /// Synchronizes canonical `todo.md`, `prd.md`, and `implementation.md` when all tasks in the
    /// active plan reach 100% completion.
    pub fn sync_completed_core_docs(&self) {
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

        let prd_path = crate::tools::minikit::resolve_doc_path(&self.workspace_root, "prd.md");
        if prd_path.exists() {
            if let Ok(content) = fs::read_to_string(&prd_path) {
                let updated: Vec<String> = content
                    .lines()
                    .map(|line| {
                        if line.trim_start().starts_with("- [ ]") {
                            line.replacen("- [ ]", "- [x]", 1)
                        } else {
                            line.to_string()
                        }
                    })
                    .collect();
                let _ = fs::write(&prd_path, updated.join("\n"));
            }
        }

        let impl_path = self.canonical_implementation_path();
        if impl_path.exists() {
            if let Ok(content) = fs::read_to_string(&impl_path) {
                if content.contains("status: active") {
                    let updated = content.replacen("status: active", "status: completed", 1);
                    let _ = fs::write(&impl_path, updated);
                }
            }
        }

        crate::tools::minikit::sync::MiniKitSyncEngine::sync_decisions_into_core_docs(
            &self.workspace_root,
        );
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

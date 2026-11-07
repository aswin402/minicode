use super::{TaskItemStatus, WorkingMemory};
use crate::constants::TIMESTAMP_FORMAT;
use crate::error::{ContextError, Result};
use chrono::Utc;
use std::fs;
use std::io::Write;
use std::path::Path;

impl WorkingMemory {
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
            let raw_lines: Vec<&str> = content.lines().collect();
            let mut parsed_items = Vec::new();
            for (line_idx, &line) in raw_lines.iter().enumerate() {
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
                            raw_lines
                                .get(*l_idx)
                                .copied()
                                .unwrap_or("")
                                .contains(step_trimmed)
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

            // Guard against double-stepping when `step="active"` is called right after
            // `reconcile_workspace_tasks` already auto-completed the previous step (`target_pos - 1`)
            // and auto-promoted `target_pos` to `[>]`. If the previous step is Completed with its
            // deliverable files present on disk, and the newly promoted active step explicitly names
            // deliverable files that have NOT all been created/completed on disk yet, treat this
            // `update_progress("active", "completed")` call as idempotently acknowledging the
            // just-completed step rather than prematurely completing the newly promoted step.
            if is_target_active
                && is_done
                && target_pos > 0
                && parsed_items[target_pos - 1].1.status == TaskItemStatus::Completed
            {
                let curr_candidates =
                    Self::extract_candidate_files(&parsed_items[target_pos].1.title);
                let prev_candidates =
                    Self::extract_candidate_files(&parsed_items[target_pos - 1].1.title);
                if !curr_candidates.is_empty()
                    && !prev_candidates.is_empty()
                    && prev_candidates
                        .iter()
                        .any(|c| self.resolve_candidate_on_disk(c).is_some())
                    && !curr_candidates
                        .iter()
                        .all(|c| self.is_candidate_completed_on_disk(c))
                {
                    return UpdateFileOutcome::AlreadyInStatus;
                }
            }

            let target_line_idx = parsed_items[target_pos].0;
            let mut updated_lines: Vec<String> = Vec::new();
            let mut modified = false;

            // Identify any earlier `[>]` (`InProgress`) steps whose deliverable files are already
            // completed on disk so out-of-order `update_progress` calls don't leave Step 1 stuck at `[>]`.
            let mut auto_completed_earlier_lines: std::collections::HashSet<usize> =
                std::collections::HashSet::new();
            if is_done && target_pos > 0 {
                for (l_idx, item) in parsed_items.iter().take(target_pos) {
                    if item.status == TaskItemStatus::InProgress {
                        let candidates = Self::extract_candidate_files(&item.title);
                        if !candidates.is_empty()
                            && candidates
                                .iter()
                                .all(|c| self.is_candidate_completed_on_disk(c))
                        {
                            auto_completed_earlier_lines.insert(*l_idx);
                        }
                    }
                }
            }

            // Check if any other item in the file is already InProgress (excluding earlier steps auto-completed above)
            let has_other_active = parsed_items.iter().enumerate().any(|(pos, (l_idx, item))| {
                pos != target_pos
                    && item.status == TaskItemStatus::InProgress
                    && !auto_completed_earlier_lines.contains(l_idx)
            });

            let mut auto_advanced = false;

            for (line_idx, line) in content.lines().enumerate() {
                if auto_completed_earlier_lines.contains(&line_idx) {
                    if line.contains("[>]") {
                        updated_lines.push(line.replacen("[>]", "[x]", 1));
                        modified = true;
                        continue;
                    } else if line.contains("[/]") {
                        updated_lines.push(line.replacen("[/]", "[x]", 1));
                        modified = true;
                        continue;
                    }
                }
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

        let is_plan_or_todo_modified = outcome_plan == UpdateFileOutcome::Modified
            || outcome_todo == UpdateFileOutcome::Modified;
        let is_any_modified =
            is_plan_or_todo_modified || intent_outcome == UpdateFileOutcome::Modified;

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
        } else if is_plan_or_todo_modified {
            // Only append to progress.md if a task plan or roadmap file was modified (idempotent progress tracking)
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
            let dedup_suffix = format!("**{}**: {}", status, resolved_title);
            let already_logged = fs::read_to_string(self.progress_path())
                .map(|existing| existing.lines().any(|line| line.ends_with(&dedup_suffix)))
                .unwrap_or(false);
            if !already_logged {
                let timestamp = Utc::now().format(TIMESTAMP_FORMAT).to_string();
                let entry = format!("\n- [{}] {}\n", timestamp, dedup_suffix);
                let _ = fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(self.progress_path())
                    .and_then(|mut f| f.write_all(entry.as_bytes()));
            }
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
}

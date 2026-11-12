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
        self.update_progress_detailed(step, status).map(|_| ())
    }

    /// Updates progress status and returns `(was_modified, matched_step_title)`.
    pub fn update_progress_detailed(&self, step: &str, status: &str) -> Result<(bool, String)> {
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

        // Helper to update a target markdown plan file and capture the exact matched step title
        let update_file = |path: &Path| -> (UpdateFileOutcome, Option<String>) {
            if !path.exists() {
                return (UpdateFileOutcome::NotFound, None);
            }
            let Ok(content) = fs::read_to_string(path) else {
                return (UpdateFileOutcome::NotFound, None);
            };

            // First pass: identify all top-level parsed task items with their line indices
            let raw_lines: Vec<&str> = content.lines().collect();
            let mut parsed_items = Vec::new();
            for (line_idx, &line) in raw_lines.iter().enumerate() {
                if Self::is_indented_subtask_line(line) {
                    continue;
                }
                if let Some(item) = Self::parse_task_line(line_idx, line.trim()) {
                    parsed_items.push((line_idx, item));
                }
            }

            if parsed_items.is_empty() {
                return (UpdateFileOutcome::NotFound, None);
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
                let find_preferring_uncompleted =
                    |pred: &dyn Fn(&(usize, super::TaskItem)) -> bool| -> Option<usize> {
                        if is_done {
                            parsed_items
                                .iter()
                                .position(|entry| {
                                    entry.1.status != TaskItemStatus::Completed && pred(entry)
                                })
                                .or_else(|| parsed_items.iter().position(pred))
                        } else {
                            parsed_items.iter().position(pred)
                        }
                    };

                // Match by title substring or line substring (preferring uncompleted items when completing)
                find_preferring_uncompleted(&|(_, item)| {
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
                    find_preferring_uncompleted(&|(_, item)| {
                        let title_lower = item.title.to_ascii_lowercase();
                        let item_words: std::collections::HashSet<&str> =
                            title_lower.split_whitespace().collect();
                        let common = needle_words.intersection(&item_words).count();
                        common * 2 >= needle_words.len()
                    })
                })
                .or_else(|| {
                    find_preferring_uncompleted(&|(l_idx, _)| {
                        raw_lines
                            .get(*l_idx)
                            .copied()
                            .unwrap_or("")
                            .contains(step_trimmed)
                    })
                })
            };

            let Some(target_pos) = matched_idx else {
                return (UpdateFileOutcome::NotFound, None);
            };

            let matched_item_title = parsed_items[target_pos].1.title.clone();
            let current_status = parsed_items[target_pos].1.status;
            if (is_done && current_status == TaskItemStatus::Completed)
                || (is_active && current_status == TaskItemStatus::InProgress)
                || (is_pending && current_status == TaskItemStatus::Pending)
            {
                return (UpdateFileOutcome::AlreadyInStatus, Some(matched_item_title));
            }

            let mut all_items: Vec<super::TaskItem> =
                parsed_items.iter().map(|(_, item)| item.clone()).collect();

            // Guard against double-stepping when `step="active"` is called right after
            // background reconciliation (`reconcile_workspace_tasks`,
            // `reconcile_preceding_tasks_on_step_transition`, or
            // `reconcile_verification_substep_on_tool_transition`) already auto-completed the
            // previous step (`target_pos - 1`) and auto-promoted `target_pos` to `[>]`.
            if is_target_active
                && is_done
                && target_pos > 0
                && parsed_items[target_pos - 1].1.status == TaskItemStatus::Completed
            {
                let prev_title = &parsed_items[target_pos - 1].1.title;
                let curr_title = &parsed_items[target_pos].1.title;
                let mut ev = self.load_verification_evidence();
                if ev.auto_promoted_step.as_deref() == Some(curr_title.as_str()) {
                    let deliverables_unsatisfied =
                        Self::task_declares_verifiable_deliverables(curr_title)
                            && !self.are_task_deliverables_verified_on_disk(target_pos, &all_items);
                    let verification_unsatisfied =
                        !self.is_verification_step_evidence_satisfied(curr_title, &ev);
                    if deliverables_unsatisfied || verification_unsatisfied {
                        ev.auto_promoted_step = None;
                        self.save_verification_evidence(&ev);
                        return (UpdateFileOutcome::AlreadyInStatus, Some(prev_title.clone()));
                    }
                }
            }

            let target_line_idx = parsed_items[target_pos].0;
            let mut updated_lines: Vec<String> = Vec::new();
            let mut modified = false;

            // Identify any earlier uncompleted (`InProgress` or `Pending`) steps whose declared
            // file or symbol deliverables are already verified on disk so out-of-order `update_progress`
            // calls (e.g. marking final `Verify` completed) automatically reconcile completed steps.
            let mut auto_completed_earlier_lines: std::collections::HashSet<usize> =
                std::collections::HashSet::new();
            if is_done && target_pos > 0 {
                for (pos, (l_idx, item)) in parsed_items.iter().enumerate().take(target_pos) {
                    if (item.status == TaskItemStatus::InProgress
                        || item.status == TaskItemStatus::Pending)
                        && self.are_task_deliverables_verified_on_disk(pos, &all_items)
                    {
                        auto_completed_earlier_lines.insert(*l_idx);
                        all_items[pos].status = TaskItemStatus::Completed;
                    }
                }
            }

            // Check if any other item in the file is already InProgress (excluding earlier steps auto-completed above)
            let has_other_active = parsed_items.iter().enumerate().any(|(pos, (l_idx, item))| {
                pos != target_pos
                    && item.status == TaskItemStatus::InProgress
                    && !auto_completed_earlier_lines.contains(l_idx)
            });

            // Dynamic Auto-Advancement Invariant:
            // When completing a task and no other task remains active `[>]`, select the next
            // pending top-level task after `target_pos`, or wrap around to the earliest remaining
            // pending top-level task before `target_pos` (for out-of-order completions).
            let next_active_line_idx: Option<usize> = if is_done && !has_other_active {
                parsed_items
                    .iter()
                    .skip(target_pos + 1)
                    .find(|(_, item)| item.status == TaskItemStatus::Pending)
                    .or_else(|| {
                        parsed_items.iter().take(target_pos).find(|(l_idx, item)| {
                            item.status == TaskItemStatus::Pending
                                && !auto_completed_earlier_lines.contains(l_idx)
                        })
                    })
                    .map(|(l_idx, _)| *l_idx)
            } else {
                None
            };

            let mut completing_parent_subtasks = false;

            for (line_idx, line) in content.lines().enumerate() {
                if completing_parent_subtasks {
                    if Self::is_indented_subtask_line(line) {
                        let replaced = line
                            .replacen("[ ]", "[x]", 1)
                            .replacen("[>]", "[x]", 1)
                            .replacen("[/]", "[x]", 1);
                        updated_lines.push(replaced);
                        continue;
                    } else {
                        completing_parent_subtasks = false;
                    }
                }
                if auto_completed_earlier_lines.contains(&line_idx) {
                    if line.contains("[>]") {
                        updated_lines.push(line.replacen("[>]", "[x]", 1));
                        modified = true;
                        continue;
                    } else if line.contains("[/]") {
                        updated_lines.push(line.replacen("[/]", "[x]", 1));
                        modified = true;
                        continue;
                    } else if line.contains("[ ]") {
                        updated_lines.push(line.replacen("[ ]", "[x]", 1));
                        modified = true;
                        continue;
                    }
                }
                if line_idx == target_line_idx {
                    if is_done {
                        completing_parent_subtasks = true;
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

                if Some(line_idx) == next_active_line_idx
                    && !Self::is_indented_subtask_line(line)
                    && line.contains("[ ]")
                {
                    updated_lines.push(line.replacen("[ ]", "[>]", 1));
                    continue;
                }

                updated_lines.push(line.to_string());
            }

            if modified {
                let updated = updated_lines.join("\n");
                let _ = fs::write(path, updated);
                (UpdateFileOutcome::Modified, Some(matched_item_title))
            } else {
                (UpdateFileOutcome::NotFound, None)
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

        let active_macro = pre_todo_tasks
            .iter()
            .enumerate()
            .find(|(_, t)| t.status == TaskItemStatus::InProgress);
        let targets_active_parent_macro = has_active_subplan
            && is_done
            && !is_target_active
            && !is_target_next
            && active_macro.is_some_and(|(m_idx, m_task)| {
                let m_lower = Self::sanitize_step_title(&m_task.title).to_ascii_lowercase();
                let matches_substep_title = !clean_needle.is_empty()
                    && pre_plan_tasks.iter().any(|s| {
                        let sl = s.title.to_ascii_lowercase();
                        sl.contains(&clean_needle) || clean_needle.contains(&sl)
                    });
                let title_matches_macro = !clean_needle.is_empty()
                    && clean_needle != (m_idx + 1).to_string()
                    && !matches_substep_title
                    && (m_lower == clean_needle
                        || (clean_needle.len() >= 10 && m_lower.contains(&clean_needle)));
                let out_of_subplan_index_matches_macro =
                    target_index == Some(m_idx + 1) && m_idx + 1 > pre_plan_tasks.len();
                title_matches_macro || out_of_subplan_index_matches_macro
            });

        let (outcome_plan, outcome_todo, matched_title) = if targets_active_parent_macro {
            let macro_title = active_macro.map(|(_, t)| t.title.clone());
            let plan_path = self.task_plan_path();
            let op = if let Ok(content) = fs::read_to_string(&plan_path) {
                let mut changed = false;
                let updated: Vec<String> = content
                    .lines()
                    .enumerate()
                    .map(|(line_idx, line)| {
                        if !Self::is_indented_subtask_line(line) {
                            if let Some(item) = Self::parse_task_line(line_idx, line.trim()) {
                                if item.status != TaskItemStatus::Completed {
                                    changed = true;
                                    return line
                                        .replacen("[ ]", "[x]", 1)
                                        .replacen("[>]", "[x]", 1)
                                        .replacen("[/]", "[x]", 1);
                                }
                            }
                        }
                        line.to_string()
                    })
                    .collect();
                if changed {
                    let _ = fs::write(&plan_path, updated.join("\n"));
                    UpdateFileOutcome::Modified
                } else {
                    UpdateFileOutcome::AlreadyInStatus
                }
            } else {
                UpdateFileOutcome::NotFound
            };
            (op, UpdateFileOutcome::NotFound, macro_title)
        } else if has_active_subplan {
            let (op, title_p) = update_file(&self.task_plan_path());
            if op != UpdateFileOutcome::NotFound {
                (op, UpdateFileOutcome::NotFound, title_p)
            } else {
                let (ot, title_t) = update_file(&self.canonical_todo_path());
                (op, ot, title_t)
            }
        } else if !pre_plan_tasks.is_empty() && !plan_has_uncompleted && todo_has_uncompleted {
            let (ot, title_t) = update_file(&self.canonical_todo_path());
            (UpdateFileOutcome::NotFound, ot, title_t)
        } else {
            let (op, title_p) = update_file(&self.task_plan_path());
            let (ot, title_t) = update_file(&self.canonical_todo_path());
            (op, ot, title_p.or(title_t))
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
        let mut is_any_modified =
            is_plan_or_todo_modified || intent_outcome == UpdateFileOutcome::Modified;

        let is_any_already = outcome_plan == UpdateFileOutcome::AlreadyInStatus
            || outcome_todo == UpdateFileOutcome::AlreadyInStatus
            || intent_outcome == UpdateFileOutcome::AlreadyInStatus;

        let mut resolved_title = matched_title
            .clone()
            .unwrap_or_else(|| Self::sanitize_step_title(step));

        if !is_any_modified && !is_any_already {
            let available_tasks = self.read_parsed_tasks();
            if available_tasks.is_empty()
                && !self.task_plan_path().exists()
                && !self.canonical_todo_path().exists()
            {
                // Auto-bootstrap active plan with this initial step so the agent workflow continues seamlessly
                let clean_title = Self::sanitize_step_title(step);
                let _ = self.init_plan("Development Plan", std::slice::from_ref(&clean_title));
                let _ = update_file(&self.task_plan_path());
                let _ = update_file(&self.canonical_todo_path());
                let timestamp = Utc::now().format(TIMESTAMP_FORMAT).to_string();
                let entry = format!("\n- [{}] **{}**: {}\n", timestamp, status, clean_title);
                let _ = fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(self.progress_path())
                    .and_then(|mut f| f.write_all(entry.as_bytes()));
                is_any_modified = true;
                resolved_title = clean_title;
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
            let mut ev = self.load_verification_evidence();
            if ev.auto_promoted_step.is_some() {
                ev.auto_promoted_step = None;
                self.save_verification_evidence(&ev);
            }
            // Only append to progress.md if a task plan or roadmap file was modified (idempotent progress tracking)
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

        // Synchronize Tier-2 tactical sub-plan progress with canonical todo.md and auto-promote
        // any pre-planned nested sub-tasks when advancing to the next `[>]` macro task.
        if has_active_subplan && outcome_plan == UpdateFileOutcome::Modified {
            let task_plan_tasks = self.read_task_plan_tasks();
            let all_sub_done = !task_plan_tasks.is_empty()
                && task_plan_tasks
                    .iter()
                    .all(|t| t.status == TaskItemStatus::Completed);
            let todo_path = self.canonical_todo_path();
            if todo_path.exists() {
                if let Ok(content) = fs::read_to_string(&todo_path) {
                    if all_sub_done {
                        let mut updated_lines = Vec::new();
                        let mut advanced_completed = false;
                        let mut completing_subblock = false;
                        let mut advanced_next = false;
                        for line in content.lines() {
                            let is_sub = Self::is_indented_subtask_line(line);
                            if completing_subblock {
                                if is_sub {
                                    let replaced = line
                                        .replacen("[ ]", "[x]", 1)
                                        .replacen("[>]", "[x]", 1)
                                        .replacen("[/]", "[x]", 1);
                                    updated_lines.push(replaced);
                                    continue;
                                } else {
                                    completing_subblock = false;
                                }
                            }
                            if !advanced_completed
                                && !is_sub
                                && (line.contains("[>]") || line.contains("[/]"))
                            {
                                let replaced =
                                    line.replacen("[>]", "[x]", 1).replacen("[/]", "[x]", 1);
                                updated_lines.push(replaced);
                                advanced_completed = true;
                                completing_subblock = true;
                            } else if advanced_completed
                                && !advanced_next
                                && !is_sub
                                && line.contains("[ ]")
                            {
                                updated_lines.push(line.replacen("[ ]", "[>]", 1));
                                advanced_next = true;
                            } else {
                                updated_lines.push(line.to_string());
                            }
                        }
                        if advanced_completed {
                            let _ = fs::write(&todo_path, updated_lines.join("\n"));
                        }
                    } else if !task_plan_tasks.is_empty() {
                        // Mirror in-flight substep statuses onto indented sub-items under active `[>]` in todo.md
                        let raw_lines: Vec<&str> = content.lines().collect();
                        if let Some(active_idx) = raw_lines.iter().position(|l| {
                            !Self::is_indented_subtask_line(l)
                                && (l.contains("[>]") || l.contains("[/]"))
                        }) {
                            let mut updated = Vec::with_capacity(raw_lines.len());
                            updated
                                .extend(raw_lines[..=active_idx].iter().map(|s| (*s).to_string()));
                            for t in &task_plan_tasks {
                                let marker = match t.status {
                                    TaskItemStatus::Completed => "[x]",
                                    TaskItemStatus::InProgress => "[>]",
                                    TaskItemStatus::Pending => "[ ]",
                                };
                                updated.push(format!("  - {} {}", marker, t.title));
                            }
                            let mut rest_idx = active_idx + 1;
                            while rest_idx < raw_lines.len()
                                && Self::is_indented_subtask_line(raw_lines[rest_idx])
                            {
                                rest_idx += 1;
                            }
                            updated.extend(raw_lines[rest_idx..].iter().map(|s| (*s).to_string()));
                            let _ = fs::write(&todo_path, updated.join("\n"));
                        }
                    }
                }
            }
        }

        // If the currently active `[>]` macro task in `todo.md` has pre-planned uncompleted
        // indented `sub_tasks` and no active tactical sub-plan is running yet, auto-activate Tier 2;
        // otherwise, if a `subplan` just finished, synchronize `task_plan.md` back to `Mode: direct`
        // from canonical `todo.md` so macro steps 2..N are visible to reconciliation, TUI, and archival.
        let was_subplan = self.is_tactical_subplan();
        let current_plan_tasks = self.read_task_plan_tasks();
        let subplan_still_running = was_subplan
            && current_plan_tasks.iter().any(|t| {
                t.status == TaskItemStatus::InProgress || t.status == TaskItemStatus::Pending
            });
        if !subplan_still_running {
            let next_subplan: Option<(String, Vec<String>)> = self
                .read_roadmap_milestones()
                .into_iter()
                .find(|m| m.is_active)
                .and_then(|m| {
                    m.tasks
                        .into_iter()
                        .find(|t| t.status == TaskItemStatus::InProgress)
                })
                .and_then(|t| {
                    let has_uncompleted_subs = t
                        .sub_tasks
                        .iter()
                        .any(|s| s.status != TaskItemStatus::Completed);
                    if has_uncompleted_subs {
                        let titles = t.sub_tasks.into_iter().map(|s| s.title).collect();
                        Some((t.title, titles))
                    } else {
                        None
                    }
                });
            if let Some((parent_title, sub_titles)) = next_subplan {
                let _ = self.init_tactical_subplan(&parent_title, &sub_titles);
            } else if was_subplan {
                self.sync_direct_task_plan_from_todo();
            }
        }

        // When all tasks in the active plan and roadmap are completed, ensure progress.md and core docs reflect 100% completion
        if self.is_all_project_tasks_completed() {
            let p_path = self.progress_path();
            if let Ok(content) = fs::read_to_string(&p_path) {
                if content.contains("- Status: In Progress") {
                    let updated =
                        content.replacen("- Status: In Progress", "- Status: Completed (100%)", 1);
                    let _ = fs::write(&p_path, updated);
                }
            }
            self.sync_completed_core_docs();
        }

        Ok((is_any_modified, resolved_title))
    }
}

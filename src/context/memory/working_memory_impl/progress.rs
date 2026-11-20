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
        // Resolve `task_dag.json` node IDs (e.g. `t1_tokens`, `t2_shell`) to their canonical step title
        // so `update_progress(step="t1_tokens", status="completed")` works seamlessly.
        let dag_resolved_title: Option<String> = if !step_trimmed.is_empty()
            && !step_trimmed.eq_ignore_ascii_case("active")
            && !step_trimmed.eq_ignore_ascii_case("current")
            && !step_trimmed.eq_ignore_ascii_case("next")
        {
            crate::context::memory::task_dag::PlanDag::load(&self.workspace_root).and_then(|dag| {
                dag.nodes
                    .into_iter()
                    .find(|n| n.id.eq_ignore_ascii_case(step_trimmed))
                    .map(|n| n.title)
            })
        } else {
            None
        };
        let effective_step = dag_resolved_title.as_deref().unwrap_or(step_trimmed);
        let clean_needle = Self::sanitize_step_title(effective_step).to_ascii_lowercase();

        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        enum UpdateFileOutcome {
            Modified,
            AlreadyInStatus,
            NotFound,
        }

        // Helper to update a target markdown plan file and capture the exact matched step title.
        // When `override_title` is provided (e.g. syncing `todo.md` with the exact step updated in
        // `task_plan.md`), match that exact title instead of re-evaluating `"active"` / index blindly.
        let update_file = |path: &Path,
                           override_title: Option<&str>|
         -> (UpdateFileOutcome, Option<String>) {
            if !path.exists() {
                return (UpdateFileOutcome::NotFound, None);
            }
            let Ok(content) = fs::read_to_string(path) else {
                return (UpdateFileOutcome::NotFound, None);
            };

            let local_override_needle = override_title
                .map(|t| Self::sanitize_step_title(t).to_ascii_lowercase())
                .filter(|s| !s.is_empty());
            let local_is_active = local_override_needle.is_none() && is_target_active;
            let local_is_next = local_override_needle.is_none() && is_target_next;
            let local_target_index = if local_override_needle.is_some() {
                None
            } else {
                target_index
            };
            let local_needle: &str = local_override_needle
                .as_deref()
                .unwrap_or(clean_needle.as_str());
            let local_raw_step: &str = override_title.unwrap_or(effective_step);

            // First pass: identify all top-level parsed task items with their line indices,
            // tracking whether each item lives under a non-active/deferred roadmap heading.
            let raw_lines: Vec<&str> = content.lines().collect();
            let mut parsed_items: Vec<(usize, super::TaskItem, bool)> = Vec::new();
            let mut cur_heading_non_active = false;
            for (line_idx, &line) in raw_lines.iter().enumerate() {
                let trimmed = line.trim();
                if trimmed.starts_with("### ")
                    || trimmed.starts_with("## ")
                    || trimmed.starts_with("# ")
                {
                    let header_text = trimmed.trim_start_matches('#').trim();
                    let (hid, htitle) = if let Some((a, b)) = header_text.split_once(':') {
                        (a.trim(), b.trim())
                    } else {
                        (header_text, header_text)
                    };
                    cur_heading_non_active = Self::is_non_active_roadmap_heading(hid, htitle);
                    continue;
                }
                if Self::is_indented_subtask_line(line) {
                    continue;
                }
                if let Some(item) = Self::parse_task_line(line_idx, trimmed) {
                    parsed_items.push((line_idx, item, cur_heading_non_active));
                }
            }

            if parsed_items.is_empty() {
                return (UpdateFileOutcome::NotFound, None);
            }

            // Determine matching item index in parsed_items
            let matched_idx = if let Some(idx) = local_target_index {
                let eligible: Vec<usize> = parsed_items
                    .iter()
                    .enumerate()
                    .filter(|(_, (_, _, non_act))| !*non_act)
                    .map(|(p, _)| p)
                    .collect();
                if !eligible.is_empty() && idx >= 1 && idx <= eligible.len() {
                    Some(eligible[idx - 1])
                } else if idx >= 1 && idx <= parsed_items.len() {
                    Some(idx - 1)
                } else {
                    None
                }
            } else if local_is_active {
                parsed_items
                    .iter()
                    .position(|(_, item, non_act)| {
                        !*non_act && item.status == TaskItemStatus::InProgress
                    })
                    .or_else(|| {
                        parsed_items.iter().position(|(_, item, non_act)| {
                            !*non_act && item.status == TaskItemStatus::Pending
                        })
                    })
            } else if local_is_next {
                parsed_items.iter().position(|(_, item, non_act)| {
                    !*non_act && item.status == TaskItemStatus::Pending
                })
            } else {
                let find_preferring_uncompleted =
                    |pred: &dyn Fn(&(usize, super::TaskItem, bool)) -> bool| -> Option<usize> {
                        if is_done {
                            parsed_items
                                .iter()
                                .position(|entry| {
                                    !entry.2
                                        && entry.1.status != TaskItemStatus::Completed
                                        && pred(entry)
                                })
                                .or_else(|| {
                                    parsed_items
                                        .iter()
                                        .position(|entry| !entry.2 && pred(entry))
                                })
                                .or_else(|| {
                                    parsed_items.iter().position(|entry| {
                                        entry.1.status != TaskItemStatus::Completed && pred(entry)
                                    })
                                })
                                .or_else(|| parsed_items.iter().position(pred))
                        } else {
                            parsed_items
                                .iter()
                                .position(|entry| !entry.2 && pred(entry))
                                .or_else(|| parsed_items.iter().position(pred))
                        }
                    };

                // Match by title substring or line substring (preferring active-section uncompleted items when completing)
                find_preferring_uncompleted(&|(_, item, _)| {
                    let item_title = item.title.to_ascii_lowercase();
                    item_title.contains(local_needle) || local_needle.contains(&item_title)
                })
                .or_else(|| {
                    // Word-overlap / token similarity match if substring didn't match directly
                    let needle_words: std::collections::HashSet<&str> =
                        local_needle.split_whitespace().collect();
                    if needle_words.is_empty() {
                        return None;
                    }
                    find_preferring_uncompleted(&|(_, item, _)| {
                        let title_lower = item.title.to_ascii_lowercase();
                        let item_words: std::collections::HashSet<&str> =
                            title_lower.split_whitespace().collect();
                        let common = needle_words.intersection(&item_words).count();
                        common * 2 >= needle_words.len()
                    })
                })
                .or_else(|| {
                    find_preferring_uncompleted(&|(l_idx, _, _)| {
                        raw_lines
                            .get(*l_idx)
                            .copied()
                            .unwrap_or("")
                            .contains(local_raw_step)
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

            let mut all_items: Vec<super::TaskItem> = parsed_items
                .iter()
                .map(|(_, item, _)| item.clone())
                .collect();

            // Guard against double-stepping when `step="active"` is called right after
            // background reconciliation (`reconcile_workspace_tasks`,
            // `reconcile_preceding_tasks_on_step_transition`, or
            // `reconcile_verification_substep_on_tool_transition`) already auto-completed the
            // previous step (`target_pos - 1`) and auto-promoted `target_pos` to `[>]`.
            if local_is_active
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
                for (pos, (l_idx, item, non_act)) in
                    parsed_items.iter().enumerate().take(target_pos)
                {
                    if !*non_act
                        && (item.status == TaskItemStatus::InProgress
                            || item.status == TaskItemStatus::Pending)
                        && self.are_task_deliverables_verified_on_disk(pos, &all_items)
                    {
                        auto_completed_earlier_lines.insert(*l_idx);
                        all_items[pos].status = TaskItemStatus::Completed;
                    }
                }
            }

            // Check if any other eligible item in the file is already InProgress (excluding earlier steps auto-completed above)
            let has_other_active =
                parsed_items
                    .iter()
                    .enumerate()
                    .any(|(pos, (l_idx, item, non_act))| {
                        pos != target_pos
                            && !*non_act
                            && item.status == TaskItemStatus::InProgress
                            && !auto_completed_earlier_lines.contains(l_idx)
                    });

            // Dynamic Auto-Advancement Invariant:
            // When completing a task and no other task remains active `[>]`, select the next
            // pending top-level task in an active/eligible section after `target_pos`, or wrap around
            // to the earliest remaining eligible pending top-level task before `target_pos`.
            let next_active_line_idx: Option<usize> = if is_done && !has_other_active {
                parsed_items
                    .iter()
                    .skip(target_pos + 1)
                    .find(|(_, item, non_act)| !*non_act && item.status == TaskItemStatus::Pending)
                    .or_else(|| {
                        parsed_items
                            .iter()
                            .take(target_pos)
                            .find(|(l_idx, item, non_act)| {
                                !*non_act
                                    && item.status == TaskItemStatus::Pending
                                    && !auto_completed_earlier_lines.contains(l_idx)
                            })
                    })
                    .map(|(l_idx, _, _)| *l_idx)
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
            .filter(|m| !Self::is_non_active_roadmap_heading(&m.id, &m.title))
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
                let exact_macro_title = !clean_needle.is_empty()
                    && m_lower == clean_needle
                    && !pre_plan_tasks.iter().any(|s| {
                        Self::sanitize_step_title(&s.title).to_ascii_lowercase() == clean_needle
                    });
                let matches_substep_title = !exact_macro_title
                    && !clean_needle.is_empty()
                    && pre_plan_tasks.iter().any(|s| {
                        let sl = s.title.to_ascii_lowercase();
                        sl.contains(&clean_needle) || clean_needle.contains(&sl)
                    });
                let title_matches_macro = exact_macro_title
                    || (!clean_needle.is_empty()
                        && clean_needle != (m_idx + 1).to_string()
                        && !matches_substep_title
                        && clean_needle.len() >= 10
                        && m_lower.contains(&clean_needle));
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
            let (op, title_p) = update_file(&self.task_plan_path(), None);
            let matched_done_substep = if op == UpdateFileOutcome::NotFound
                && is_done
                && !clean_needle.is_empty()
                && self.canonical_todo_path().exists()
            {
                // Before fuzzy-matching macro tasks (which can falsely collide on short words like
                // "gate" inside an unrelated completed macro description), check if `clean_needle`
                // directly matches an already-completed indented substep in `todo.md` (e.g. a
                // substep of the previous macro step that was auto-reconciled by `power_verify`).
                fs::read_to_string(self.canonical_todo_path())
                    .ok()
                    .and_then(|todo_text| {
                        todo_text.lines().find_map(|line| {
                            let is_indented = line.starts_with("  ") || line.starts_with('\t');
                            if !is_indented {
                                return None;
                            }
                            let item = Self::parse_task_line(0, line.trim())?;
                            if item.status != TaskItemStatus::Completed {
                                return None;
                            }
                            let sub_l = Self::sanitize_step_title(&item.title).to_ascii_lowercase();
                            if sub_l == clean_needle
                                || sub_l.contains(&clean_needle)
                                || clean_needle.contains(&sub_l)
                            {
                                Some(item.title)
                            } else {
                                None
                            }
                        })
                    })
            } else {
                None
            };
            if op != UpdateFileOutcome::NotFound {
                (op, UpdateFileOutcome::NotFound, title_p)
            } else if let Some(done_sub_title) = matched_done_substep {
                (
                    UpdateFileOutcome::NotFound,
                    UpdateFileOutcome::AlreadyInStatus,
                    Some(done_sub_title),
                )
            } else if let Some((active_m_idx, active_m_task)) = active_macro {
                let matched_macro_idx = if let Some(idx) = target_index {
                    if idx > pre_plan_tasks.len() && idx <= pre_todo_tasks.len() {
                        Some(idx - 1)
                    } else {
                        None
                    }
                } else if !clean_needle.is_empty() {
                    let meaningful_words = |s: &str| -> std::collections::HashSet<String> {
                        s.split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                            .filter(|w| {
                                w.len() >= 2
                                    && !matches!(
                                        *w,
                                        "and" | "the" | "for" | "with" | "from" | "into" | "to"
                                    )
                            })
                            .map(str::to_string)
                            .collect()
                    };
                    let needle_words = meaningful_words(&clean_needle);
                    let score_macro = |t: &super::TaskItem| -> Option<usize> {
                        let tl = Self::sanitize_step_title(&t.title).to_ascii_lowercase();
                        if tl == clean_needle {
                            return Some(10_000);
                        }
                        if tl.contains(&clean_needle) || clean_needle.contains(&tl) {
                            return Some(5_000);
                        }
                        if needle_words.is_empty() {
                            return None;
                        }
                        let item_words = meaningful_words(&tl);
                        let common = needle_words.intersection(&item_words).count();
                        if common * 2 >= needle_words.len()
                            && (common >= 2 || needle_words.len() == 1)
                        {
                            Some(common * 100)
                        } else {
                            None
                        }
                    };
                    if is_done {
                        pre_todo_tasks
                            .iter()
                            .enumerate()
                            .filter(|(_, t)| t.status != TaskItemStatus::Completed)
                            .filter_map(|(i, t)| score_macro(t).map(|sc| (i, sc)))
                            .max_by_key(|(_, sc)| *sc)
                            .map(|(i, _)| i)
                            .or_else(|| {
                                pre_todo_tasks
                                    .iter()
                                    .enumerate()
                                    .filter_map(|(i, t)| score_macro(t).map(|sc| (i, sc)))
                                    .max_by_key(|(_, sc)| *sc)
                                    .map(|(i, _)| i)
                            })
                    } else {
                        pre_todo_tasks
                            .iter()
                            .enumerate()
                            .filter_map(|(i, t)| score_macro(t).map(|sc| (i, sc)))
                            .max_by_key(|(_, sc)| *sc)
                            .map(|(i, _)| i)
                    }
                } else {
                    None
                };

                if let Some(m_idx) = matched_macro_idx {
                    if is_done && pre_todo_tasks[m_idx].status == TaskItemStatus::Completed {
                        (
                            UpdateFileOutcome::NotFound,
                            UpdateFileOutcome::AlreadyInStatus,
                            Some(pre_todo_tasks[m_idx].title.clone()),
                        )
                    } else if is_done && m_idx > active_m_idx {
                        let later_has_verify =
                            Self::step_requires_runtime_verification(&pre_todo_tasks[m_idx].title)
                                || pre_todo_tasks[(m_idx + 1)..].iter().any(|t| {
                                    t.status != TaskItemStatus::Completed
                                        && Self::step_requires_runtime_verification(&t.title)
                                });
                        let can_catch_up_preceding = (active_m_idx..m_idx).all(|k| {
                            if pre_todo_tasks[k].status == TaskItemStatus::Completed {
                                return true;
                            }
                            if Self::step_requires_browser_verification(&pre_todo_tasks[k].title)
                                || (Self::step_requires_runtime_verification(
                                    &pre_todo_tasks[k].title,
                                ) && !later_has_verify)
                                || !self.is_step_deliverables_satisfied_on_disk(k, &pre_todo_tasks)
                            {
                                return false;
                            }
                            if k == active_m_idx {
                                pre_plan_tasks
                                    .iter()
                                    .enumerate()
                                    .filter(|(_, s)| s.status != TaskItemStatus::Completed)
                                    .all(|(s_idx, s)| {
                                        self.is_step_deliverables_satisfied_on_disk(
                                            s_idx,
                                            &pre_plan_tasks,
                                        ) || (later_has_verify
                                            && Self::is_deferrable_intermediate_smoke_substep(
                                                &s.title,
                                            ))
                                    })
                            } else {
                                true
                            }
                        });
                        if can_catch_up_preceding {
                            for task in pre_todo_tasks.iter().take(m_idx).skip(active_m_idx) {
                                if task.status != TaskItemStatus::Completed {
                                    let _ =
                                        self.update_progress_detailed(&task.title, "completed")?;
                                }
                            }
                            return self
                                .update_progress_detailed(&pre_todo_tasks[m_idx].title, status);
                        }
                        return Err(ContextError::Memory(format!(
                            "Cannot mark later roadmap step '{}' Completed while active Step #{} ('{}') still has uncompleted sub-tasks in task_plan.md. Finish and mark the active sub-plan steps first.",
                            pre_todo_tasks[m_idx].title,
                            active_m_idx + 1,
                            active_m_task.title
                        ))
                        .into());
                    } else {
                        (
                            UpdateFileOutcome::NotFound,
                            UpdateFileOutcome::NotFound,
                            None,
                        )
                    }
                } else {
                    (
                        UpdateFileOutcome::NotFound,
                        UpdateFileOutcome::NotFound,
                        None,
                    )
                }
            } else {
                (
                    UpdateFileOutcome::NotFound,
                    UpdateFileOutcome::NotFound,
                    None,
                )
            }
        } else if !pre_plan_tasks.is_empty() && !plan_has_uncompleted && todo_has_uncompleted {
            let (ot, title_t) = update_file(&self.canonical_todo_path(), None);
            if ot == UpdateFileOutcome::Modified {
                self.sync_direct_task_plan_from_todo();
            }
            (UpdateFileOutcome::NotFound, ot, title_t)
        } else {
            let (op, title_p) = update_file(&self.task_plan_path(), None);
            let todo_sync_override =
                if (is_target_active || is_target_next || target_index.is_some())
                    && op != UpdateFileOutcome::NotFound
                {
                    title_p.as_deref()
                } else {
                    None
                };
            let (ot, title_t) = update_file(&self.canonical_todo_path(), todo_sync_override);
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
        let is_plan_or_todo_already = outcome_plan == UpdateFileOutcome::AlreadyInStatus
            || outcome_todo == UpdateFileOutcome::AlreadyInStatus;
        let mut is_any_modified = is_plan_or_todo_modified
            || (!is_plan_or_todo_already && intent_outcome == UpdateFileOutcome::Modified);

        let is_any_already =
            is_plan_or_todo_already || intent_outcome == UpdateFileOutcome::AlreadyInStatus;

        let mut resolved_title = matched_title
            .clone()
            .unwrap_or_else(|| Self::sanitize_step_title(step));

        if !is_any_modified && !is_any_already {
            if !has_active_subplan && is_done && !clean_needle.is_empty() {
                if let Some(done_sub) =
                    pre_todo_tasks
                        .iter()
                        .flat_map(|t| t.sub_tasks.iter())
                        .find(|sub| {
                            sub.status == TaskItemStatus::Completed && {
                                let sl = Self::sanitize_step_title(&sub.title).to_ascii_lowercase();
                                sl.contains(&clean_needle) || clean_needle.contains(&sl)
                            }
                        })
                {
                    return Ok((false, done_sub.title.clone()));
                }
            }
            let available_tasks = self.read_parsed_tasks();
            if available_tasks.is_empty()
                && !self.task_plan_path().exists()
                && !self.canonical_todo_path().exists()
            {
                // Auto-bootstrap active plan with this initial step so the agent workflow continues seamlessly
                let clean_title = Self::sanitize_step_title(step);
                let _ = self.init_plan("Development Plan", std::slice::from_ref(&clean_title));
                let _ = update_file(&self.task_plan_path(), None);
                let _ = update_file(&self.canonical_todo_path(), None);
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
            // Only append to progress.md if a task plan or roadmap file was modified (idempotent progress tracking),
            // checking only the most recent entry for `resolved_title` so re-completing after a reopen logs cleanly.
            let dedup_suffix = format!("**{}**: {}", status, resolved_title);
            let dedup_suffix_lower = dedup_suffix.to_ascii_lowercase();
            let title_lower = resolved_title.to_ascii_lowercase();
            let already_logged = fs::read_to_string(self.progress_path())
                .map(|existing| {
                    existing
                        .lines()
                        .rev()
                        .find(|line| line.to_ascii_lowercase().contains(&title_lower))
                        .is_some_and(|last_for_step| {
                            last_for_step
                                .to_ascii_lowercase()
                                .ends_with(&dedup_suffix_lower)
                        })
                })
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

        // Synchronize `.minicode/plan/task_dag.json` node statuses with canonical tasks
        self.sync_task_dag_statuses();

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

    /// Synchronizes `.minicode/plan/task_dag.json` node statuses from canonical `todo.md` / `task_plan.md`.
    pub fn sync_task_dag_statuses(&self) {
        let Some(mut dag) = crate::context::memory::task_dag::PlanDag::load(&self.workspace_root)
        else {
            return;
        };
        let macro_tasks: Vec<super::TaskItem> = self
            .read_roadmap_milestones()
            .into_iter()
            .find(|m| m.is_active)
            .map(|m| m.tasks)
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| self.read_task_plan_tasks());
        if !macro_tasks.is_empty() && dag.sync_statuses_from_tasks(&macro_tasks) {
            let _ = dag.save(&self.workspace_root);
        }
    }

    fn resolve_macro_task_index(tasks: &[super::TaskItem], selector: &str) -> Option<usize> {
        let trimmed = selector.trim();
        if trimmed.is_empty() {
            return None;
        }
        if trimmed.eq_ignore_ascii_case("active") || trimmed.eq_ignore_ascii_case("current") {
            return tasks
                .iter()
                .position(|t| t.status == TaskItemStatus::InProgress)
                .or_else(|| {
                    tasks
                        .iter()
                        .position(|t| t.status == TaskItemStatus::Pending)
                });
        }
        if let Ok(one_based) = trimmed.parse::<usize>() {
            if one_based >= 1 && one_based <= tasks.len() {
                return Some(one_based - 1);
            }
        }
        let needle = Self::sanitize_step_title(trimmed).to_ascii_lowercase();
        tasks.iter().position(|t| {
            let tl = Self::sanitize_step_title(&t.title).to_ascii_lowercase();
            tl == needle || tl.contains(&needle) || needle.contains(&tl)
        })
    }

    fn rewrite_canonical_macro_tasks(&self, title: &str, tasks: &[super::TaskItem]) -> Result<()> {
        let dir = self.plan_dir();
        fs::create_dir_all(&dir).map_err(|e| ContextError::Memory(e.to_string()))?;
        let timestamp = Utc::now().format(TIMESTAMP_FORMAT).to_string();

        let format_task_lines = |items: &[super::TaskItem]| -> Vec<String> {
            let mut out = Vec::new();
            for task in items {
                let marker = match task.status {
                    TaskItemStatus::Completed => "[x]",
                    TaskItemStatus::InProgress => "[>]",
                    TaskItemStatus::Pending => "[ ]",
                };
                out.push(format!("- {} {}", marker, task.title));
                for sub in &task.sub_tasks {
                    let sub_marker = match sub.status {
                        TaskItemStatus::Completed => "[x]",
                        TaskItemStatus::InProgress => "[>]",
                        TaskItemStatus::Pending => "[ ]",
                    };
                    out.push(format!("  - {} {}", sub_marker, sub.title));
                }
            }
            out
        };

        let todo_path = self.canonical_todo_path();
        if let Some(parent) = todo_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let mut patched_in_place = false;
        if let Ok(existing) = fs::read_to_string(&todo_path) {
            let raw_lines: Vec<&str> = existing.lines().collect();
            // Locate markdown sections (`##` / `###`) so we preserve all other sections
            // (e.g. `## Shipped`, `## Deferred — not started`, `## Explicitly not done`).
            struct SecSpan {
                heading_idx: usize,
                end_idx: usize,
                non_active: bool,
                in_progress: usize,
                pending: usize,
                total_tasks: usize,
                is_active_milestone_hdr: bool,
            }
            let mut sections: Vec<SecSpan> = Vec::new();
            for (idx, line) in raw_lines.iter().enumerate() {
                let trimmed = line.trim();
                if trimmed.starts_with("## ") || trimmed.starts_with("### ") {
                    if let Some(prev) = sections.last_mut() {
                        prev.end_idx = idx;
                    }
                    let hdr = trimmed.trim_start_matches('#').trim();
                    let (hid, htitle) = hdr
                        .split_once(':')
                        .map(|(a, b)| (a.trim(), b.trim()))
                        .unwrap_or((hdr, hdr));
                    sections.push(SecSpan {
                        heading_idx: idx,
                        end_idx: raw_lines.len(),
                        non_active: Self::is_non_active_roadmap_heading(hid, htitle),
                        in_progress: 0,
                        pending: 0,
                        total_tasks: 0,
                        is_active_milestone_hdr: hdr
                            .to_ascii_lowercase()
                            .starts_with("active milestone"),
                    });
                } else if let Some(cur) = sections.last_mut() {
                    if !Self::is_indented_subtask_line(line) {
                        if let Some(item) = Self::parse_task_line(idx, trimmed) {
                            cur.total_tasks += 1;
                            match item.status {
                                TaskItemStatus::InProgress => cur.in_progress += 1,
                                TaskItemStatus::Pending => cur.pending += 1,
                                TaskItemStatus::Completed => {}
                            }
                        }
                    }
                }
            }
            let target_sec = sections
                .iter()
                .find(|s| !s.non_active && s.in_progress > 0)
                .or_else(|| sections.iter().find(|s| !s.non_active && s.pending > 0))
                .or_else(|| sections.iter().find(|s| s.is_active_milestone_hdr))
                .or_else(|| sections.iter().find(|s| !s.non_active && s.total_tasks > 0));
            if let Some(sec) = target_sec {
                if sections.len() > 1 {
                    let mut out_lines: Vec<String> = Vec::with_capacity(raw_lines.len() + 8);
                    for line in &raw_lines[..=sec.heading_idx] {
                        if line.trim_start().starts_with("> Updated:") {
                            out_lines.push(format!("> Updated: {}", timestamp));
                        } else {
                            out_lines.push((*line).to_string());
                        }
                    }
                    out_lines.extend(format_task_lines(tasks));
                    if sec.end_idx < raw_lines.len() {
                        out_lines.push(String::new());
                        out_lines.extend(raw_lines[sec.end_idx..].iter().map(|s| (*s).to_string()));
                    }
                    let mut final_text = out_lines.join("\n");
                    if !final_text.ends_with('\n') {
                        final_text.push('\n');
                    }
                    fs::write(&todo_path, final_text)
                        .map_err(|e| ContextError::Memory(e.to_string()))?;
                    patched_in_place = true;
                }
            }
        }
        if !patched_in_place {
            let mut todo_out = format!(
                "# Tasks & Milestone Roadmap: {}\n\n> Updated: {}\n\n## Active Milestone: {}\n",
                title, timestamp, title
            );
            for line in format_task_lines(tasks) {
                todo_out.push_str(&line);
                todo_out.push('\n');
            }
            fs::write(&todo_path, todo_out).map_err(|e| ContextError::Memory(e.to_string()))?;
        }

        if !self.is_tactical_subplan() {
            let mut plan_out = format!(
                "# Task Plan: {}\n\n> Created: {}\n> Mode: direct\n\n## Objectives & Steps:\n",
                title, timestamp
            );
            for (idx, task) in tasks.iter().enumerate() {
                let marker = match task.status {
                    TaskItemStatus::Completed => "[x]",
                    TaskItemStatus::InProgress => "[>]",
                    TaskItemStatus::Pending => "[ ]",
                };
                plan_out.push_str(&format!("{}. {} {}\n", idx + 1, marker, task.title));
            }
            fs::write(self.task_plan_path(), plan_out)
                .map_err(|e| ContextError::Memory(e.to_string()))?;
        }
        Ok(())
    }

    /// Mid-flight adaptive mutation: inserts a new step into the active plan without losing
    /// completed `[x]` progress, updating `todo.md`, `task_plan.md`, and `task_dag.json`.
    #[allow(clippy::too_many_arguments)]
    pub fn add_step_in_place(
        &self,
        new_step: &str,
        after_step: Option<&str>,
        substeps: &[String],
        target_files: &[String],
        depends_on: &[String],
        role: Option<&str>,
        verify_cmd: Option<&str>,
    ) -> Result<String> {
        let clean_title = Self::sanitize_step_title(new_step);
        if clean_title.is_empty() {
            return Err(ContextError::Memory("Cannot add an empty plan step.".to_string()).into());
        }
        let mut tasks = self
            .read_roadmap_milestones()
            .into_iter()
            .find(|m| m.is_active)
            .map(|m| m.tasks)
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| self.read_task_plan_tasks());
        if tasks.is_empty() {
            return Err(ContextError::Memory(
                "No active plan exists yet; call `create_plan` first.".to_string(),
            )
            .into());
        }

        let has_in_progress = tasks.iter().any(|t| t.status == TaskItemStatus::InProgress);
        let has_pending = tasks.iter().any(|t| t.status == TaskItemStatus::Pending);
        let new_status = if !has_in_progress && !has_pending {
            TaskItemStatus::InProgress
        } else {
            TaskItemStatus::Pending
        };

        let parsed_subs: Vec<super::TaskItem> = substeps
            .iter()
            .enumerate()
            .map(|(i, s)| super::TaskItem {
                line_index: i,
                status: if i == 0 && new_status == TaskItemStatus::InProgress {
                    TaskItemStatus::InProgress
                } else {
                    TaskItemStatus::Pending
                },
                title: Self::sanitize_step_title(s),
                sub_tasks: Vec::new(),
            })
            .filter(|st| !st.title.is_empty())
            .collect();

        let insert_pos = after_step
            .and_then(|sel| Self::resolve_macro_task_index(&tasks, sel))
            .map(|idx| idx + 1)
            .unwrap_or(tasks.len());

        tasks.insert(
            insert_pos,
            super::TaskItem {
                line_index: insert_pos,
                status: new_status,
                title: clean_title.clone(),
                sub_tasks: parsed_subs,
            },
        );

        let plan_title = self
            .read_roadmap_root_title()
            .or_else(|| self.read_task_plan_title())
            .unwrap_or_else(|| "Active Plan".to_string());

        if let Some(mut dag) = crate::context::memory::task_dag::PlanDag::load(&self.workspace_root)
        {
            let next_id = format!("t{}", dag.nodes.len() + 1);
            let unique_id = if dag.nodes.iter().any(|n| n.id == next_id) {
                format!("t{}_{}", dag.nodes.len() + 1, insert_pos + 1)
            } else {
                next_id
            };
            let resolved_deps = if !depends_on.is_empty() {
                dag.explicit_dependencies = true;
                depends_on.to_vec()
            } else if insert_pos > 0 && insert_pos <= dag.nodes.len() {
                vec![dag.nodes[insert_pos - 1].id.clone()]
            } else {
                Vec::new()
            };
            let normalized_files: Vec<String> = if target_files.is_empty() {
                Self::extract_candidate_files(&clean_title)
            } else {
                target_files
                    .iter()
                    .map(|p| crate::context::memory::task_dag::normalize_boundary_path(p))
                    .filter(|p| !p.is_empty())
                    .collect()
            };
            dag.nodes.insert(
                insert_pos.min(dag.nodes.len()),
                crate::context::memory::task_dag::PlanDagNode {
                    id: unique_id,
                    title: clean_title.clone(),
                    description: clean_title.clone(),
                    substeps: substeps
                        .iter()
                        .map(|s| Self::sanitize_step_title(s))
                        .filter(|s| !s.is_empty())
                        .collect(),
                    target_files: normalized_files,
                    depends_on: resolved_deps,
                    role: role
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(String::from),
                    verify_cmd: verify_cmd
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(String::from),
                    status: new_status,
                },
            );
            dag.updated_at = Utc::now().to_rfc3339();
            let _ = dag.compute_execution_topology()?;
            dag.save(&self.workspace_root)?;
        }

        self.rewrite_canonical_macro_tasks(&plan_title, &tasks)?;
        let timestamp = Utc::now().format(TIMESTAMP_FORMAT).to_string();
        let entry = format!(
            "\n- [{}] **Adapted Plan (+Step #{})**: {}\n",
            timestamp,
            insert_pos + 1,
            clean_title
        );
        let _ = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.progress_path())
            .and_then(|mut f| f.write_all(entry.as_bytes()));

        Ok(clean_title)
    }

    /// Mid-flight adaptive mutation: splits a macro step into nested tactical `substeps`.
    pub fn split_step_in_place(&self, target_step: &str, substeps: &[String]) -> Result<String> {
        let clean_subs: Vec<String> = substeps
            .iter()
            .map(|s| Self::sanitize_step_title(s))
            .filter(|s| !s.is_empty())
            .collect();
        if clean_subs.is_empty() {
            return Err(ContextError::Memory(
                "`split` requires a non-empty `substeps` array.".to_string(),
            )
            .into());
        }
        let mut tasks = self
            .read_roadmap_milestones()
            .into_iter()
            .find(|m| m.is_active)
            .map(|m| m.tasks)
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| self.read_task_plan_tasks());
        let Some(idx) = Self::resolve_macro_task_index(&tasks, target_step) else {
            return Err(ContextError::Memory(format!(
                "Could not find plan step matching `{}` to split.",
                target_step
            ))
            .into());
        };
        let matched_title = tasks[idx].title.clone();
        let is_active = tasks[idx].status == TaskItemStatus::InProgress;

        if let Some(mut dag) = crate::context::memory::task_dag::PlanDag::load(&self.workspace_root)
        {
            if let Some(node) = dag
                .nodes
                .iter_mut()
                .find(|n| n.title.eq_ignore_ascii_case(&matched_title))
            {
                node.substeps.clone_from(&clean_subs);
                dag.updated_at = Utc::now().to_rfc3339();
                let _ = dag.save(&self.workspace_root);
            }
        }

        if is_active {
            self.init_tactical_subplan(&matched_title, &clean_subs)?;
        } else {
            tasks[idx].sub_tasks = clean_subs
                .iter()
                .enumerate()
                .map(|(i, s)| super::TaskItem {
                    line_index: i,
                    status: TaskItemStatus::Pending,
                    title: s.clone(),
                    sub_tasks: Vec::new(),
                })
                .collect();
            let plan_title = self
                .read_roadmap_root_title()
                .or_else(|| self.read_task_plan_title())
                .unwrap_or_else(|| "Active Plan".to_string());
            self.rewrite_canonical_macro_tasks(&plan_title, &tasks)?;
        }
        Ok(matched_title)
    }

    /// Mid-flight adaptive mutation: drops an obsolete uncompleted step from the active plan
    /// while preserving completed `[x]` tasks and rewiring downstream sequential dependencies.
    pub fn drop_step_in_place(&self, target_step: &str, reason: Option<&str>) -> Result<String> {
        let mut tasks = self
            .read_roadmap_milestones()
            .into_iter()
            .find(|m| m.is_active)
            .map(|m| m.tasks)
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| self.read_task_plan_tasks());
        let Some(idx) = Self::resolve_macro_task_index(&tasks, target_step) else {
            return Err(ContextError::Memory(format!(
                "Could not find plan step matching `{}` to drop.",
                target_step
            ))
            .into());
        };
        if tasks[idx].status == TaskItemStatus::Completed {
            return Err(ContextError::Memory(format!(
                "Step `{}` is already completed (`[x]`) and cannot be dropped.",
                tasks[idx].title
            ))
            .into());
        }
        if tasks.len() <= 1 {
            return Err(ContextError::Memory(
                "Cannot drop the sole remaining step in the plan; call `archive_plan` or `create_plan` instead.".to_string(),
            )
            .into());
        }

        let removed = tasks.remove(idx);
        if removed.status == TaskItemStatus::InProgress
            && !tasks.iter().any(|t| t.status == TaskItemStatus::InProgress)
        {
            if let Some(next_pending) = tasks
                .iter_mut()
                .skip(idx)
                .find(|t| t.status == TaskItemStatus::Pending)
            {
                next_pending.status = TaskItemStatus::InProgress;
            } else if let Some(any_pending) = tasks
                .iter_mut()
                .find(|t| t.status == TaskItemStatus::Pending)
            {
                any_pending.status = TaskItemStatus::InProgress;
            }
            if self.is_tactical_subplan() {
                let _ = fs::remove_file(self.task_plan_path());
            }
        }

        let plan_title = self
            .read_roadmap_root_title()
            .or_else(|| self.read_task_plan_title())
            .unwrap_or_else(|| "Active Plan".to_string());
        self.rewrite_canonical_macro_tasks(&plan_title, &tasks)?;

        if let Some(mut dag) = crate::context::memory::task_dag::PlanDag::load(&self.workspace_root)
        {
            if let Some(d_idx) = dag
                .nodes
                .iter()
                .position(|n| n.title.eq_ignore_ascii_case(&removed.title))
            {
                let dropped_node = dag.nodes.remove(d_idx);
                let inherited_deps = dropped_node.depends_on;
                for node in &mut dag.nodes {
                    if let Some(pos) = node.depends_on.iter().position(|d| d == &dropped_node.id) {
                        node.depends_on.remove(pos);
                        for dep in &inherited_deps {
                            if dep != &node.id && !node.depends_on.contains(dep) {
                                node.depends_on.push(dep.clone());
                            }
                        }
                    }
                }
                dag.sync_statuses_from_tasks(&tasks);
                dag.updated_at = Utc::now().to_rfc3339();
                let _ = dag.save(&self.workspace_root);
            }
        }

        let timestamp = Utc::now().format(TIMESTAMP_FORMAT).to_string();
        let why = reason.unwrap_or("superseded during implementation");
        let entry = format!(
            "\n- [{}] **Adapted Plan (-Dropped)**: {} _(reason: {})_\n",
            timestamp, removed.title, why
        );
        let _ = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.progress_path())
            .and_then(|mut f| f.write_all(entry.as_bytes()));

        Ok(removed.title)
    }
}

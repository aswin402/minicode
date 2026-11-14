use crate::constants::{
    ARCHIVE_DIR, FINDINGS_FILE, PLAN_DIR, PROGRESS_FILE, TASK_PLAN_FILE, TIMESTAMP_FORMAT,
    WORKSPACE_DIR_NAME,
};
use crate::error::{ContextError, Result};
use chrono::Utc;
use std::fs;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[path = "working_memory_impl/notes_and_archive.rs"]
mod notes_and_archive;
#[path = "working_memory_impl/progress.rs"]
mod progress;
#[path = "working_memory_impl/reconcile.rs"]
mod reconcile;
pub use reconcile::ActiveStepReadiness;
#[cfg(test)]
#[path = "working_memory_impl/tests.rs"]
mod tests;

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

/// Persistent runtime/browser verification evidence for the active plan or sub-plan
/// (`.minicode/plan/verification_evidence.json`), preventing premature double-stepping
/// right after automatic stage promotion before actual stage verification has occurred.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerificationEvidenceState {
    #[serde(default)]
    pub auto_promoted_step: Option<String>,
    #[serde(default)]
    pub static_build_verified: bool,
    #[serde(default)]
    pub dev_server_boot_verified: bool,
    #[serde(default)]
    pub browser_desktop_verified: bool,
    #[serde(default)]
    pub browser_responsive_verified: bool,
    #[serde(default)]
    pub responsive_mode_active: bool,
    #[serde(default)]
    pub dirty_since_browser_qa: bool,
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

    pub fn last_verify_path(&self) -> PathBuf {
        self.plan_dir().join(crate::constants::LAST_VERIFY_FILENAME)
    }

    pub fn verification_evidence_path(&self) -> PathBuf {
        self.plan_dir().join("verification_evidence.json")
    }

    pub fn load_verification_evidence(&self) -> VerificationEvidenceState {
        fs::read_to_string(self.verification_evidence_path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save_verification_evidence(&self, state: &VerificationEvidenceState) {
        let dir = self.plan_dir();
        if fs::create_dir_all(&dir).is_ok() {
            if let Ok(serialized) = serde_json::to_string_pretty(state) {
                let _ = fs::write(self.verification_evidence_path(), serialized);
            }
        }
    }

    pub fn clear_verification_evidence(&self) {
        let _ = fs::remove_file(self.verification_evidence_path());
    }

    /// Returns true if `line` is an indented markdown task line (`>= 2` leading spaces or a tab)
    /// representing a nested sub-task beneath a top-level milestone task.
    pub fn is_indented_subtask_line(line: &str) -> bool {
        let indent_cols: usize = line
            .chars()
            .take_while(|c| *c == ' ' || *c == '\t')
            .map(|c| if c == '\t' { 2 } else { 1 })
            .sum();
        indent_cols >= 2 && Self::parse_task_line(0, line.trim()).is_some()
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
        self.init_two_tier_plan_with_substeps(title, steps, &[], architecture, milestones)
    }

    /// Initializes a comprehensive two-tier task plan with optional per-step nested `step_substeps`.
    /// Any macro step `i` with non-empty `step_substeps[i]` persists its nested sub-tasks indented
    /// in `todo.md`; if Step 1 (`index 0`) has sub-steps, `.minicode/plan/task_plan.md` immediately
    /// activates in `> Mode: subplan` for Step 1 (and subsequent steps with sub-steps auto-activate
    /// when promoted to `[>]`).
    pub fn init_two_tier_plan_with_substeps(
        &self,
        title: &str,
        steps: &[String],
        step_substeps: &[Vec<String>],
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
                if arch.trim_start().starts_with("---") {
                    arch.to_string()
                } else {
                    format!(
                        "---\ntype: Implementation Plan\ntitle: \"Technical Implementation Architecture — {}\"\ndescription: \"Blueprint, key modules, and verification criteria\"\nstatus: active\n---\n\n{}",
                        title.replace('"', "'"),
                        arch.trim_start()
                    )
                }
            } else {
                let mut blueprint = format!(
                    "---\ntype: Implementation Plan\ntitle: \"Technical Implementation Architecture — {}\"\ndescription: \"Blueprint, key modules, and verification criteria\"\nstatus: active\n---\n\n# Technical Implementation Architecture — {}\n\n> Created: {}\n> Generated by minicode Two-Tier Planning Engine\n\n## 1. Executive Summary & Goals\nBlueprint and technical architecture for {}.\n\n## 2. Key Modules & Planned Deliverables\n",
                    title.replace('"', "'"),
                    title,
                    timestamp,
                    title
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

        // 1b. Synchronize skeleton prd.md objectives with the active plan steps
        let prd_path = crate::tools::minikit::resolve_doc_path(&self.workspace_root, "prd.md");
        if let Ok(existing_prd) = fs::read_to_string(&prd_path) {
            let skeleton_objectives = "- [ ] **Objective 1**: Core system functionality.\n- [ ] **Objective 2**: Fast, intuitive developer and user experience.";
            if existing_prd.contains(skeleton_objectives) && !steps.is_empty() {
                let rendered_objectives = steps
                    .iter()
                    .enumerate()
                    .map(|(idx, step)| {
                        format!(
                            "- [ ] **Objective {}**: {}",
                            idx + 1,
                            Self::sanitize_step_title(step)
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                let updated_prd =
                    existing_prd.replacen(skeleton_objectives, &rendered_objectives, 1);
                let _ = fs::write(&prd_path, updated_prd);
            }
        }

        // 2. Tier 1 Milestone Roadmap -> canonical todo.md
        let todo_path = self.canonical_todo_path();
        if let Some(parent) = todo_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let has_any_substeps = step_substeps.iter().any(|s| !s.is_empty());
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
            || has_any_substeps
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
                if let Some(subs) = step_substeps.get(idx).filter(|s| !s.is_empty()) {
                    for (s_idx, sub) in subs.iter().enumerate() {
                        let sub_clean = Self::sanitize_step_title(sub);
                        let sub_marker = if idx == 0 && s_idx == 0 { "[>]" } else { "[ ]" };
                        todo_content.push_str(&format!("  - {} {}\n", sub_marker, sub_clean));
                    }
                }
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

        // Synchronize living IntentLedger with the new active plan while preserving any
        // immutable root user objective already anchored on disk.
        let intent_path = self
            .workspace_root
            .join(crate::constants::DEFAULT_INTENT_PERSISTENCE_FILE);
        let preserved_root =
            crate::context::memory::intent::IntentLedger::load_from_disk(&intent_path)
                .ok()
                .map(|l| l.root_objective)
                .filter(|r| {
                    let rt = r.trim();
                    !rt.is_empty()
                        && rt != "General Assistance"
                        && rt != "Complete prompt tasks"
                        && rt != "Complete prompt checklist"
                });
        let ledger_root = preserved_root.as_deref().unwrap_or(title);
        let mut new_ledger = crate::context::memory::intent::IntentLedger::new(ledger_root);
        let mut first_id: Option<String> = None;
        for (idx, step) in steps.iter().enumerate() {
            let clean_step = Self::sanitize_step_title(step);
            let related = Self::extract_candidate_files(&clean_step);
            let item_id = new_ledger.add_item(&clean_step, None, related);
            if idx == 0 {
                first_id = Some(item_id);
            }
        }
        if let Some(ref fid) = first_id {
            new_ledger.set_status(
                fid,
                crate::context::memory::intent::RequirementStatus::InProgress,
            );
        }
        let _ = new_ledger.save_to_disk(&intent_path);

        // Initialize NOTES.md template if missing
        let _ = self.ensure_notes_template();
        self.clear_verification_evidence();

        // If Step 1 already carries nested tactical sub-steps, activate Tier-2 `subplan` mode now
        if let Some(first_subs) = step_substeps.first().filter(|s| !s.is_empty()) {
            let parent_topic = if first_step_clean.is_empty() {
                title
            } else {
                first_step_clean.as_str()
            };
            self.init_tactical_subplan(parent_topic, first_subs)?;
        }

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
        self.clear_verification_evidence();
        let timestamp = Utc::now().format(TIMESTAMP_FORMAT).to_string();

        // Ensure canonical todo.md exists; if it exists and the active `[>]` macro task does not
        // yet list indented sub-items, persist `steps` indented underneath it so `todo.md` reflects
        // the full two-tier hierarchy on disk.
        let todo_path = self.canonical_todo_path();
        if !todo_path.exists() {
            if let Some(parent) = todo_path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let clean_topic = Self::sanitize_step_title(topic);
            let mut todo_content = format!(
                "# Tasks & Milestone Roadmap: {}\n\n> Initialized: {}\n\n## Active Milestone: {}\n- [>] {}\n",
                clean_topic, timestamp, clean_topic, clean_topic
            );
            for (idx, step) in steps.iter().enumerate() {
                let sub_clean = Self::sanitize_step_title(step);
                let sub_marker = if idx == 0 { "[>]" } else { "[ ]" };
                todo_content.push_str(&format!("  - {} {}\n", sub_marker, sub_clean));
            }
            let _ = fs::write(&todo_path, todo_content);
        } else if let Ok(existing_todo) = fs::read_to_string(&todo_path) {
            let raw_lines: Vec<&str> = existing_todo.lines().collect();
            if let Some(active_line_idx) = raw_lines.iter().position(|l| {
                !Self::is_indented_subtask_line(l) && (l.contains("[>]") || l.contains("[/]"))
            }) {
                let mut updated = Vec::with_capacity(raw_lines.len() + steps.len());
                updated.extend(
                    raw_lines[..=active_line_idx]
                        .iter()
                        .map(|s| (*s).to_string()),
                );
                for (idx, step) in steps.iter().enumerate() {
                    let sub_clean = Self::sanitize_step_title(step);
                    let sub_marker = if idx == 0 { "[>]" } else { "[ ]" };
                    updated.push(format!("  - {} {}", sub_marker, sub_clean));
                }
                // Skip any old indented sub-lines under the active macro task before appending remaining lines
                let mut rest_idx = active_line_idx + 1;
                while rest_idx < raw_lines.len()
                    && Self::is_indented_subtask_line(raw_lines[rest_idx])
                {
                    rest_idx += 1;
                }
                updated.extend(raw_lines[rest_idx..].iter().map(|s| (*s).to_string()));
                let _ = fs::write(&todo_path, format!("{}\n", updated.join("\n")));
            }
        }

        let root_goal_line = self
            .read_roadmap_root_title()
            .filter(|root| !root.is_empty())
            .map(|root| format!("> Root Goal: {}\n", root))
            .unwrap_or_default();

        let mut plan_content = format!(
            "# Task Plan: {}\n\n> Created: {}\n> Mode: subplan\n{}\n## Objectives & Steps:\n",
            topic, timestamp, root_goal_line
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

    /// Extracts the root project/roadmap title from canonical `todo.md` (or `progress.md` fallback).
    pub fn read_roadmap_root_title(&self) -> Option<String> {
        if let Ok(Some(todo)) = self.read_roadmap_todo() {
            for line in todo.lines().take(12) {
                let trimmed = line.trim();
                for prefix in ["# Tasks & Milestone Roadmap:", "## Active Milestone:", "# "] {
                    if let Some(rest) = trimmed.strip_prefix(prefix) {
                        let clean = rest.trim();
                        if !clean.is_empty() {
                            return Some(clean.to_string());
                        }
                    }
                }
            }
        }
        if let Ok(prog) = fs::read_to_string(self.progress_path()) {
            for line in prog.lines().take(10) {
                if let Some(rest) = line.trim().strip_prefix("- Active Goal:") {
                    let clean = rest.trim();
                    if !clean.is_empty() {
                        return Some(clean.to_string());
                    }
                }
            }
        }
        None
    }

    /// Synchronizes `.minicode/plan/task_plan.md` back to `> Mode: direct` from the canonical
    /// `todo.md` milestone tasks once a tactical `Mode: subplan` finishes and the next active
    /// macro step has no nested sub-steps (or all macro steps have completed).
    pub(crate) fn sync_direct_task_plan_from_todo(&self) {
        let milestones = self.read_roadmap_milestones();
        let target_milestone = milestones
            .iter()
            .find(|m| m.is_active)
            .or_else(|| milestones.iter().rev().find(|m| !m.tasks.is_empty()));
        let Some(m) = target_milestone else {
            return;
        };
        if m.tasks.is_empty() {
            return;
        }
        let root_title = self
            .read_roadmap_root_title()
            .unwrap_or_else(|| m.title.clone());
        let timestamp = self
            .read_roadmap_todo()
            .ok()
            .flatten()
            .or_else(|| fs::read_to_string(self.progress_path()).ok())
            .and_then(|doc| {
                doc.lines().take(8).find_map(|line| {
                    line.trim()
                        .strip_prefix("> Initialized:")
                        .map(|ts| ts.trim().to_string())
                        .filter(|ts| !ts.is_empty())
                })
            })
            .unwrap_or_else(|| Utc::now().format(TIMESTAMP_FORMAT).to_string());
        let mut plan_content = format!(
            "# Task Plan: {}\n\n> Created: {}\n> Mode: direct\n\n## Objectives & Steps:\n",
            root_title, timestamp
        );
        for (idx, t) in m.tasks.iter().enumerate() {
            let marker = match t.status {
                TaskItemStatus::Completed => "[x]",
                TaskItemStatus::InProgress => "[>]",
                TaskItemStatus::Pending => "[ ]",
            };
            plan_content.push_str(&format!("{}. {} {}\n", idx + 1, marker, t.title));
        }
        let _ = fs::write(self.task_plan_path(), plan_content);
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

    /// Checks whether `.minicode/plan/task_plan.md` exists and was modified within `max_age_secs`.
    pub fn has_fresh_task_plan(&self, max_age_secs: u64) -> bool {
        let path = self.task_plan_path();
        fs::metadata(&path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|mtime| mtime.elapsed().ok())
            .is_some_and(|elapsed| elapsed.as_secs() <= max_age_secs)
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
            if self.is_tactical_subplan()
                && !plan.contains("[>]")
                && !plan.contains("[/]")
                && !plan.contains("[ ]")
            {
                if let Ok(Some(todo)) = self.read_roadmap_todo() {
                    if todo.contains("[>]") || todo.contains("[/]") || todo.contains("[ ]") {
                        return Ok(Some(todo));
                    }
                }
            }
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

    /// Parses all discrete top-level task items from the active plan.
    pub fn read_parsed_tasks(&self) -> Vec<TaskItem> {
        let content = match self.read_plan() {
            Ok(Some(c)) => c,
            _ => return Vec::new(),
        };

        let mut tasks = Vec::new();
        for (idx, line) in content.lines().enumerate() {
            if Self::is_indented_subtask_line(line) {
                continue;
            }
            if let Some(task) = Self::parse_task_line(idx, line.trim()) {
                tasks.push(task);
            }
        }
        tasks
    }

    /// Parses roadmap milestones and their tasks from canonical `todo.md`
    /// (falling back to `task_plan.md` or the latest archived plan in `.minicode/plan/archive/`).
    pub fn read_roadmap_milestones(&self) -> Vec<MilestonePhase> {
        let content = match self
            .read_roadmap_todo()
            .ok()
            .flatten()
            .or_else(|| self.read_task_plan().ok().flatten())
            .or_else(|| self.read_latest_archived_plan())
        {
            Some(c) => c,
            None => return Vec::new(),
        };

        let mut milestones: Vec<MilestonePhase> = Vec::new();
        let mut current_id = "General".to_string();
        let mut current_title = "Active Tasks".to_string();
        let mut current_tasks: Vec<TaskItem> = Vec::new();
        let make_milestone = |id: String, title: String, tasks: Vec<TaskItem>| -> MilestonePhase {
            let total_tasks = tasks.len();
            let completed_tasks = tasks
                .iter()
                .filter(|t| t.status == TaskItemStatus::Completed)
                .count();
            let in_progress_tasks = tasks
                .iter()
                .filter(|t| t.status == TaskItemStatus::InProgress)
                .count();
            let pending_tasks = tasks
                .iter()
                .filter(|t| t.status == TaskItemStatus::Pending)
                .count();
            MilestonePhase {
                id,
                title,
                total_tasks,
                completed_tasks,
                in_progress_tasks,
                pending_tasks,
                tasks,
                is_active: false,
            }
        };

        for (idx, line) in content.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("### ")
                || trimmed.starts_with("## ")
                || trimmed.starts_with("# ")
            {
                let header_text = trimmed.trim_start_matches('#').trim();
                if !current_tasks.is_empty() {
                    milestones.push(make_milestone(
                        current_id.clone(),
                        current_title.clone(),
                        std::mem::take(&mut current_tasks),
                    ));
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

            if Self::is_indented_subtask_line(line) {
                if let Some(sub_task) = Self::parse_task_line(idx, trimmed) {
                    if let Some(parent) = current_tasks.last_mut() {
                        parent.sub_tasks.push(sub_task);
                        continue;
                    }
                }
            }

            if let Some(task) = Self::parse_task_line(idx, trimmed) {
                current_tasks.push(task);
            }
        }

        if !current_tasks.is_empty() {
            milestones.push(make_milestone(current_id, current_title, current_tasks));
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

        // Attach active Tier-2 tactical sub-plan tasks to the active milestone's [>] task so /todo modal shows the nested hierarchy
        if self.is_tactical_subplan() {
            let sub_tasks = self.read_task_plan_tasks();
            let has_active_sub = sub_tasks.iter().any(|t| {
                t.status == TaskItemStatus::InProgress || t.status == TaskItemStatus::Pending
            });
            if has_active_sub {
                if let Some(m) = milestones.iter_mut().find(|m| m.is_active) {
                    let is_same = m.tasks.len() == sub_tasks.len()
                        && m.tasks
                            .iter()
                            .zip(sub_tasks.iter())
                            .all(|(a, b)| a.title.eq_ignore_ascii_case(&b.title));
                    if !is_same {
                        if let Some(idx) = m
                            .tasks
                            .iter()
                            .position(|t| t.status == TaskItemStatus::InProgress)
                            .or_else(|| {
                                m.tasks
                                    .iter()
                                    .position(|t| t.status == TaskItemStatus::Pending)
                            })
                        {
                            m.tasks[idx].sub_tasks = sub_tasks;
                        }
                    }
                }
            }
        }

        milestones
    }

    fn format_milestone_label(m: &MilestonePhase) -> String {
        if m.id == m.title {
            m.id.clone()
        } else {
            format!("{}: {}", m.id, m.title)
        }
    }

    fn window_active_tasks(tasks: &[TaskItem]) -> Vec<TaskItem> {
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
        windowed
    }

    /// Reads only the active phase's tasks, windowed to recent completed + active + next pending.
    /// Returns `(phase_title, windowed_tasks)`. If all tasks are completed or no tasks exist,
    /// returns `(None, Vec::new())`.
    #[allow(dead_code)]
    pub fn read_active_phase_tasks(&self) -> (Option<String>, Vec<TaskItem>) {
        let milestones = self.read_roadmap_milestones();
        if let Some(active_milestone) = milestones.iter().find(|m| m.is_active) {
            (
                Some(Self::format_milestone_label(active_milestone)),
                Self::window_active_tasks(&active_milestone.tasks),
            )
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

        let windowed = if tasks.len() <= 24 {
            tasks.to_vec()
        } else if pending_tasks == 0 && in_progress_tasks == 0 {
            let start = tasks.len().saturating_sub(6);
            tasks[start..].to_vec()
        } else {
            Self::window_active_tasks(tasks)
        };

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

    /// Reads the task plan section of the most recent archived plan in `.minicode/plan/archive/` if any exists.
    pub fn read_latest_archived_plan(&self) -> Option<String> {
        let dir = self.archive_dir();
        let entries = fs::read_dir(&dir).ok()?;
        let mut files: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_file() && p.extension().is_some_and(|ext| ext == "md"))
            .collect();
        files.sort();
        let latest = files.last()?;
        let raw = fs::read_to_string(latest).ok()?;
        let plan_section = raw.split("\n---\n").next().unwrap_or(&raw).trim();
        if plan_section.is_empty() {
            None
        } else {
            Some(plan_section.to_string())
        }
    }

    /// Reconciles any uncompleted plan step titles in `task_plan.md` and `todo.md` if they
    /// mention a different MiniKit stack identifier than the `chosen_stack` actually scaffolded.
    pub fn reconcile_stack_choice(&self, chosen_stack: &str) {
        let chosen = chosen_stack.trim();
        if chosen.is_empty() {
            return;
        }
        let chosen_lower = chosen.to_ascii_lowercase();
        let mut all_stacks: Vec<String> =
            crate::tools::minikit::scaffolder::MiniKitScaffolder::get_all_stacks_for(None)
                .into_iter()
                .map(|s| s.name)
                .filter(|name| !name.eq_ignore_ascii_case(chosen))
                .collect();
        if all_stacks.is_empty() {
            return;
        }
        // Sort longest stack names first so "react-vite" never matches inside "react-vite-gsap"
        all_stacks.sort_by(|a, b| b.len().cmp(&a.len()));

        for path in [self.task_plan_path(), self.canonical_todo_path()] {
            let Ok(content) = fs::read_to_string(&path) else {
                continue;
            };
            let mut changed = false;
            let mut new_lines = Vec::with_capacity(content.lines().count());
            for line in content.lines() {
                let mut updated = line.to_string();
                if (line.contains("[ ]") || line.contains("[>]"))
                    && !updated.to_ascii_lowercase().contains(&chosen_lower)
                {
                    for other_stack in &all_stacks {
                        if let Some(pos) = updated.find(other_stack.as_str()) {
                            let before_ok = pos == 0
                                || !updated[..pos].chars().next_back().is_some_and(|c| {
                                    c.is_ascii_alphanumeric() || c == '-' || c == '_'
                                });
                            let end = pos + other_stack.len();
                            let after_ok = end >= updated.len()
                                || !updated[end..].chars().next().is_some_and(|c| {
                                    c.is_ascii_alphanumeric() || c == '-' || c == '_'
                                });
                            if before_ok && after_ok {
                                updated =
                                    format!("{}{}{}", &updated[..pos], chosen, &updated[end..]);
                                changed = true;
                                break;
                            }
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
                if !m.tasks.is_empty() {
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
                        milestone_tasks[idx].sub_tasks = if task_plan_tasks.len() <= 8 {
                            task_plan_tasks.clone()
                        } else {
                            Self::window_active_tasks(&task_plan_tasks)
                        };
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

        // 3. Fallback to task_plan_tasks if task_plan.md is still active on disk and all steps just finished
        if !task_plan_tasks.is_empty() {
            let title = self
                .read_task_plan_title()
                .unwrap_or_else(|| "Plan".to_string());
            return Self::build_plan_summary(title, &task_plan_tasks);
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
}

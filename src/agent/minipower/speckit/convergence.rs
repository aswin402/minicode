//! Append-Only Implementation Convergence Engine (`speckit.converge`).
//!
//! Audits the live workspace filesystem against `prd.md`, `spec.md`, `architecture.md`,
//! and `todo.md` / `.minicode/plan/task_plan.md`.
//!
//! Strictly resolves to one of two deterministic outcomes:
//! 1. [`ConvergenceVerdict::Converged`] (`✅ CONVERGED`): All spec deliverables exist on disk,
//!    all requirements map to tasks, and all tasks are complete. Leaves `todo.md` byte-for-byte untouched.
//! 2. [`ConvergenceVerdict::TasksAppended`]: Computes the set difference
//!    $\Delta = \text{RequiredDeliverables} \setminus \text{VerifiedWorkspaceState}$
//!    and **append-only** writes missing delta tasks under `## Convergence` in `todo.md`
//!    without modifying or deleting any existing task lines.

use crate::agent::minipower::speckit::traceability::{
    analyze_workspace_artifacts, extract_normalized_stems,
};
use crate::constants::{
    SPECKIT_CONVERGENCE_HEADING, SPECKIT_DUPLICATE_TASK_STEM_OVERLAP, SPECKIT_SOURCE_FALLBACK_DIRS,
    SPECKIT_TASK_PLAN_REL_PATH, SPECKIT_TODO_REL_PATH,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Two-outcome verdict of the Append-Only Implementation Convergence Engine (`speckit.converge`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConvergenceVerdict {
    /// Zero missing deliverables, zero unmapped requirements, 100% task completion (`✅ CONVERGED`).
    Converged,
    /// Missing deliverables or unmapped requirements were appended under `## Convergence` in `todo.md`.
    TasksAppended,
}

/// Detailed convergence audit report produced by [`converge_workspace`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConvergenceReport {
    pub verdict: ConvergenceVerdict,
    pub missing_deliverables: Vec<String>,
    pub unmapped_requirement_ids: Vec<String>,
    pub incomplete_task_ids: Vec<String>,
    pub appended_tasks: Vec<String>,
    pub todo_path: Option<PathBuf>,
}

impl ConvergenceReport {
    /// Renders the convergence report in Markdown.
    pub fn format_markdown(&self) -> String {
        let mut out = String::new();
        match self.verdict {
            ConvergenceVerdict::Converged => {
                out.push_str("### ✅ CONVERGED — SpecKit Implementation Convergence Audit\n\n");
                out.push_str(
                    "- **Status:** All specification requirements, deliverable files, and tasks are verified in the workspace.\n",
                );
                out.push_str(
                    "- **Mutation Guarantee:** `todo.md` was left byte-for-byte unchanged.\n",
                );
            }
            ConvergenceVerdict::TasksAppended => {
                out.push_str("### 🔄 SpecKit Convergence Delta — Append-Only Tasks Generated\n\n");
                out.push_str(&format!(
                    "- **Missing Deliverable Files:** `{}`\n- **Unmapped Requirements:** `{}`\n- **Incomplete Existing Tasks:** `{}`\n- **Appended Delta Tasks:** `{}`\n",
                    self.missing_deliverables.len(),
                    self.unmapped_requirement_ids.len(),
                    self.incomplete_task_ids.len(),
                    self.appended_tasks.len()
                ));
                if !self.appended_tasks.is_empty() {
                    out.push_str("\n#### Appended under `## Convergence`\n");
                    for task in &self.appended_tasks {
                        out.push_str(&format!("- [ ] {}\n", task));
                    }
                }
                if let Some(p) = &self.todo_path {
                    out.push_str(&format!(
                        "\n*Appended to `{}` (existing tasks preserved untouched).*\n",
                        p.display()
                    ));
                }
            }
        }
        out
    }
}

/// Checks whether a candidate relative file path exists on disk inside `workspace_root`.
fn deliverable_exists_on_disk(workspace_root: &Path, candidate: &str) -> bool {
    if workspace_root.join(candidate).exists() {
        return true;
    }
    !candidate.contains('/')
        && SPECKIT_SOURCE_FALLBACK_DIRS
            .iter()
            .any(|dir| workspace_root.join(dir).join(candidate).exists())
}

/// Runs the Append-Only Implementation Convergence Engine (`speckit.converge`).
///
/// When `apply_append` is `true` and gaps are found, appends missing delta tasks under
/// `## Convergence` in `<docs_dir>/core/todo.md` (and `.minicode/plan/task_plan.md` if present).
/// When zero gaps exist, returns [`ConvergenceVerdict::Converged`] without touching `todo.md`.
pub fn converge_workspace(workspace_root: &Path, apply_append: bool) -> ConvergenceReport {
    let analysis = analyze_workspace_artifacts(workspace_root);
    let docs_dir = crate::tools::minikit::resolve_docs_dir(workspace_root);
    let todo_path = docs_dir.join(SPECKIT_TODO_REL_PATH);

    // 1. Collect all required deliverable file paths across Requirements and PlanComponents
    let mut required_files = BTreeSet::new();
    for node in analysis
        .requirements
        .iter()
        .chain(analysis.plan_components.iter())
    {
        for f in &node.candidate_files {
            required_files.insert(f.clone());
        }
    }

    let missing_deliverables: Vec<String> = required_files
        .into_iter()
        .filter(|f| !deliverable_exists_on_disk(workspace_root, f))
        .collect();

    let unmapped_requirement_ids: Vec<String> = analysis
        .unmapped_requirements
        .iter()
        .map(|r| r.id.clone())
        .collect();

    let incomplete_task_ids: Vec<String> = analysis
        .tasks
        .iter()
        .filter(|t| t.completed == Some(false))
        .map(|t| t.id.clone())
        .collect();

    // Existing task stem signatures to avoid appending duplicate delta tasks
    let existing_task_signatures: Vec<BTreeSet<String>> =
        analysis.tasks.iter().map(|t| t.stems.clone()).collect();

    let mut delta_tasks = Vec::new();

    for missing_file in &missing_deliverables {
        let candidate = format!(
            "Implement missing specification deliverable `{}` and verify compilation/tests",
            missing_file
        );
        let stems = extract_normalized_stems(&candidate);
        let already_present = analysis
            .tasks
            .iter()
            .any(|t| t.candidate_files.contains(missing_file) && t.completed == Some(false));
        if !already_present && !existing_task_signatures.contains(&stems) {
            delta_tasks.push(candidate);
        }
    }

    for req in &analysis.unmapped_requirements {
        let candidate = format!("Implement `{}` ({}): {}", req.id, req.section, req.text);
        let stems = extract_normalized_stems(&candidate);
        let is_dup = existing_task_signatures.iter().any(|existing| {
            let inter = existing.intersection(&stems).count();
            let min_len = existing.len().min(stems.len()).max(1);
            (inter as f64 / min_len as f64) >= SPECKIT_DUPLICATE_TASK_STEM_OVERLAP
        });
        if !is_dup {
            delta_tasks.push(candidate);
        }
    }

    // Converged condition: 0 missing deliverables, 0 unmapped requirements, 0 incomplete tasks, and at least 1 requirement or task
    let has_any_spec_or_task = !analysis.requirements.is_empty() || !analysis.tasks.is_empty();
    if has_any_spec_or_task
        && missing_deliverables.is_empty()
        && unmapped_requirement_ids.is_empty()
        && incomplete_task_ids.is_empty()
        && delta_tasks.is_empty()
    {
        return ConvergenceReport {
            verdict: ConvergenceVerdict::Converged,
            missing_deliverables,
            unmapped_requirement_ids,
            incomplete_task_ids,
            appended_tasks: Vec::new(),
            todo_path: todo_path.exists().then_some(todo_path),
        };
    }

    // If no new delta tasks were synthesized (e.g. only existing incomplete tasks remain),
    // do not append redundant lines, or synthesize a single verification convergence step if specs were empty.
    if !has_any_spec_or_task && delta_tasks.is_empty() {
        delta_tasks.push(
            "Author baseline specification in `minikit_docs/core/spec.md` before convergence check"
                .to_string(),
        );
    }

    let mut written_todo_path = None;
    if apply_append && !delta_tasks.is_empty() {
        if let Ok(path) = append_convergence_tasks(&todo_path, &delta_tasks) {
            written_todo_path = Some(path);
        }
        let task_plan_path = workspace_root.join(SPECKIT_TASK_PLAN_REL_PATH);
        if task_plan_path.exists() {
            let _ = append_convergence_tasks(&task_plan_path, &delta_tasks);
        }
    }

    ConvergenceReport {
        verdict: ConvergenceVerdict::TasksAppended,
        missing_deliverables,
        unmapped_requirement_ids,
        incomplete_task_ids,
        appended_tasks: delta_tasks,
        todo_path: written_todo_path.or_else(|| todo_path.exists().then_some(todo_path)),
    }
}

/// Append-only writer that appends `delta_tasks` to the end of `target_path` under `## Convergence`,
/// preserving every existing byte in `target_path`.
fn append_convergence_tasks(
    target_path: &Path,
    delta_tasks: &[String],
) -> std::io::Result<PathBuf> {
    if let Some(parent) = target_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let existing = std::fs::read_to_string(target_path).unwrap_or_default();
    let has_convergence_heading = existing.lines().any(|line| {
        line.trim()
            .eq_ignore_ascii_case(SPECKIT_CONVERGENCE_HEADING)
    });

    let mut block = String::new();
    if !existing.is_empty() && !existing.ends_with('\n') {
        block.push('\n');
    }
    if !has_convergence_heading {
        if !existing.is_empty() {
            block.push('\n');
        }
        block.push_str(SPECKIT_CONVERGENCE_HEADING);
        block.push('\n');
    }
    for task in delta_tasks {
        block.push_str(&format!("- [ ] {}\n", task));
    }

    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(target_path)?;
    file.write_all(block.as_bytes())?;
    Ok(target_path.to_path_buf())
}

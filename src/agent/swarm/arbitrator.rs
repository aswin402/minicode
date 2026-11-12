//! Swarm Worktree Merge Arbitrator.
//!
//! Task branches are merged into the swarm integration branch as soon as they complete
//! (see [`crate::agent::swarm::integration`]). This module performs the single final merge
//! of the integration branch into the user's checked-out branch and renders the summary.

use std::path::Path;
use tracing::{info, warn};

use crate::agent::swarm::integration::IntegrationBranch;
use crate::agent::swarm::models::{SwarmError, SwarmExecutionState, SwarmPlan, SwarmTaskStatus};
use crate::sandbox::arbitration::MergeArbitrator;

pub struct SwarmArbitrator;

impl SwarmArbitrator {
    /// Merges the integration branch (all completed tasks, already merged in dependency order)
    /// into the workspace's checked-out branch, and returns a markdown merge summary.
    pub fn merge_integration(
        workspace_root: &Path,
        plan: &SwarmPlan,
        state: &mut SwarmExecutionState,
        integration: &IntegrationBranch,
    ) -> Result<String, SwarmError> {
        let mut summary = String::from("#### Automated Worktree Merge Outcomes\n\n");
        let topo = plan.topological_order()?;
        let mut integrated = 0usize;
        for task_id in &topo {
            match state.outcomes.get(task_id) {
                Some(o) if o.status == SwarmTaskStatus::Completed => {
                    integrated += 1;
                    summary.push_str(&format!(
                        "- `{}`: ✔ Integrated into `{}`\n",
                        task_id, integration.branch
                    ));
                }
                Some(o) => summary.push_str(&format!(
                    "- `{}`: Skipped (Status: {}{})\n",
                    task_id,
                    o.status.badge(),
                    o.error
                        .as_deref()
                        .map(|e| format!(" — {}", e))
                        .unwrap_or_default()
                )),
                None => summary.push_str(&format!("- `{}`: Skipped (not executed)\n", task_id)),
            }
        }

        if integrated == 0 || !integration.has_changes() {
            summary.push_str("\n**Summary:** no integrated changes to merge.\n");
            return Ok(summary);
        }

        let mergeability = MergeArbitrator::check_mergeability(workspace_root, &integration.branch)
            .map_err(|e| SwarmError::ArbitrationFailed(e.to_string()))?;
        if !mergeability.can_merge_cleanly {
            warn!(
                conflicts = ?mergeability.conflicted_files,
                "Integration branch conflicts with workspace; retaining branch unmerged"
            );
            summary.push_str(&format!(
                "\n⚠ Integration branch `{}` conflicts with the workspace (`{}`); left unmerged for manual review.\n",
                integration.branch,
                mergeability.conflicted_files.join("`, `")
            ));
            return Ok(summary);
        }

        let msg = format!("feat(swarm): merge swarm `{}` ({})", plan.id, plan.title);
        let merged =
            MergeArbitrator::apply_merge(workspace_root, &integration.branch, true, Some(&msg))
                .map_err(|e| SwarmError::ArbitrationFailed(e.to_string()))?;

        let hash = merged.commit_hash.as_deref().unwrap_or("merged");
        info!(hash = %hash, "Merged swarm integration branch into workspace");
        let modified: Vec<String> = state
            .outcomes
            .values()
            .filter(|o| o.status == SwarmTaskStatus::Completed)
            .flat_map(|o| o.files_modified.iter().cloned())
            .collect();
        if !modified.is_empty() {
            crate::agent::subagent::relay::get_global_relay_store()
                .record_modified_files(workspace_root, &modified);
        }
        state.auto_merged = true;
        summary.push_str(&format!(
            "\n**Summary:** {} task(s) merged into the workspace via `{}` ({}).\n",
            integrated, integration.branch, hash
        ));
        Ok(summary)
    }
}

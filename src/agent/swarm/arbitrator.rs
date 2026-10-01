//! Swarm Worktree Merge Arbitrator.
//!
//! Sequentially arbitrates, verifies, and merges completed worker branches
//! into the target workspace branch with zero-clobbering guarantees.

use std::path::Path;
use tracing::{error, info, warn};

use crate::agent::swarm::models::{SwarmError, SwarmExecutionState, SwarmPlan, SwarmTaskStatus};
use crate::sandbox::arbitration::MergeArbitrator;

pub struct SwarmArbitrator;

impl SwarmArbitrator {
    /// Arbitrates and merges passing worker branches into the target repository in topological order.
    pub async fn arbitrate_and_merge(
        workspace_root: &Path,
        plan: &SwarmPlan,
        state: &mut SwarmExecutionState,
    ) -> Result<String, SwarmError> {
        let mut merge_summary = String::new();
        merge_summary.push_str("#### Automated Worktree Merge Outcomes\n\n");

        // 1. Get topological order so we merge dependencies before dependents
        let topo = plan.topological_order()?;
        let mut successfully_merged = 0;
        let mut skipped_or_failed = 0;

        for task_id in &topo {
            let task = match plan.get_task(task_id) {
                Some(t) => t,
                None => continue,
            };

            let outcome = match state.outcomes.get(task_id) {
                Some(o) => o,
                None => continue,
            };

            // Only attempt merging for Completed tasks with a valid branch
            if outcome.status != SwarmTaskStatus::Completed {
                merge_summary.push_str(&format!(
                    "- `{}`: Skipped (Status: {})\n",
                    task.id,
                    outcome.status.badge()
                ));
                skipped_or_failed += 1;
                continue;
            }

            let branch_name = match &outcome.branch_name {
                Some(b) => b,
                None => {
                    // Task did not create an isolated branch (e.g. shared mode)
                    continue;
                }
            };

            let worktree_path = match &outcome.worktree_path {
                Some(p) => p.as_path(),
                None => workspace_root,
            };

            info!(
                task_id = %task.id,
                branch = %branch_name,
                "Arbitrating merge for completed worker branch"
            );

            // Verify worktree if check command specified
            if let Some(ref check_cmd) = task.check_command {
                let report = MergeArbitrator::verify_worktree(worktree_path, Some(check_cmd));
                match report {
                    Ok(val) if !val.success => {
                        warn!(task_id = %task.id, exit_code = val.exit_code, "Verification failed prior to merge");
                        merge_summary.push_str(&format!(
                            "- `{}`: ✖ Pre-merge check failed (`{}` exited with code {})\n",
                            task.id, check_cmd, val.exit_code
                        ));
                        skipped_or_failed += 1;
                        continue;
                    }
                    Err(e) => {
                        warn!(task_id = %task.id, error = %e, "Pre-merge check execution error");
                        merge_summary.push_str(&format!(
                            "- `{}`: ✖ Pre-merge check error: {}\n",
                            task.id, e
                        ));
                        skipped_or_failed += 1;
                        continue;
                    }
                    _ => {}
                }
            }

            // Check mergeability against current HEAD
            match MergeArbitrator::check_mergeability(workspace_root, branch_name) {
                Ok(mergeability) => {
                    if !mergeability.can_merge_cleanly {
                        warn!(
                            task_id = %task.id,
                            conflicts = ?mergeability.conflicted_files,
                            "Merge conflicts detected; retaining branch unmerged"
                        );
                        merge_summary.push_str(&format!(
                            "- `{}`: ⚠ Conflict detected on `{}` (Conflicted: `{}`)\n",
                            task.id,
                            branch_name,
                            mergeability.conflicted_files.join("`, `")
                        ));
                        skipped_or_failed += 1;
                        continue;
                    }

                    // Merge branch into main workspace
                    let commit_msg =
                        format!("feat(swarm): merge worker `{}` ({})", task.id, task.title);
                    match MergeArbitrator::apply_merge(
                        workspace_root,
                        branch_name,
                        true,
                        Some(&commit_msg),
                    ) {
                        Ok(success) => {
                            let hash_str = success.commit_hash.as_deref().unwrap_or("merged");
                            info!(task_id = %task.id, hash = %hash_str, "Successfully merged worker branch");
                            merge_summary.push_str(&format!(
                                "- `{}`: ✔ Successfully merged branch `{}` ({})\n",
                                task.id, branch_name, hash_str
                            ));
                            successfully_merged += 1;
                        }
                        Err(e) => {
                            error!(task_id = %task.id, error = %e, "Git merge execution error");
                            merge_summary.push_str(&format!(
                                "- `{}`: ✖ Merge execution error: {}\n",
                                task.id, e
                            ));
                            skipped_or_failed += 1;
                        }
                    }
                }
                Err(e) => {
                    warn!(task_id = %task.id, error = %e, "Mergeability probe failed");
                    merge_summary.push_str(&format!(
                        "- `{}`: ✖ Mergeability probe failed: {}\n",
                        task.id, e
                    ));
                    skipped_or_failed += 1;
                }
            }
        }

        state.auto_merged = successfully_merged > 0;
        merge_summary.push_str(&format!(
            "\n**Summary:** {} branch(es) merged cleanly, {} skipped/failed.\n",
            successfully_merged, skipped_or_failed
        ));

        Ok(merge_summary)
    }
}

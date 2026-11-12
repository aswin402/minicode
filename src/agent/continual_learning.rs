//! Autonomous Continual Skill & Gotcha Harvester (Hermes-style Self-Improvement, Zero `/commands`).
//!
//! Runs automatically inside `AgentLoop` whenever a task passes verification after one or more
//! self-healing / verification-barrier recovery iterations (`heal_attempts > 0`):
//! 1. Resolves technology scope dynamically via [`WorkspaceProfile::language_label_for_path`].
//! 2. Redacts any credentials via [`SecretRedactor::global`] before persistence.
//! 3. Upserts the distilled [`GlobalGotcha`] into [`VaultStore`].
//! 4. When `heal_attempts >= 2` or `occurrence_count >= 2`, autonomously synthesizes/updates a
//!    project-scoped `SKILL.md` (`.minicode/skills/learned-<scope>-invariants/SKILL.md`), gated
//!    through `SkillSpectorEngine` so recurring verified fixes become first-class progressive skills.

use crate::context::workspace_profile::WorkspaceProfile;
use crate::sandbox::redact::SecretRedactor;
use crate::vault::models::{sanitize_skill_name, GlobalGotcha, SkillScope};
use crate::vault::store::VaultStore;
use std::collections::BTreeSet;
use std::path::Path;

/// Autonomous post-verification continual learning engine.
pub struct ContinualSkillHarvester;

impl ContinualSkillHarvester {
    /// Distills a verified self-healing recovery into `GlobalGotcha` and (for multi-iteration or
    /// recurring pitfalls) synthesizes/updates a project `SKILL.md` without any manual `/commands`.
    pub fn harvest_verified_recovery(
        workspace_root: &Path,
        turn_files_modified: &[String],
        heal_attempts: usize,
        last_verification_failure: Option<&(String, String)>,
    ) {
        if heal_attempts == 0 {
            return;
        }
        let store = VaultStore::new(workspace_root);
        Self::harvest_with_store(
            &store,
            turn_files_modified,
            heal_attempts,
            last_verification_failure,
        );
    }

    /// Testable entry point accepting an explicit [`VaultStore`] (e.g. with isolated test vault dir).
    pub fn harvest_with_store(
        store: &VaultStore,
        turn_files_modified: &[String],
        heal_attempts: usize,
        last_verification_failure: Option<&(String, String)>,
    ) {
        if heal_attempts == 0 {
            return;
        }

        let tech_scope = turn_files_modified
            .iter()
            .find_map(|f| {
                let label = WorkspaceProfile::language_label_for_path(Path::new(f));
                (label != "General").then_some(label)
            })
            .or_else(|| {
                turn_files_modified
                    .first()
                    .map(|f| WorkspaceProfile::language_label_for_path(Path::new(f)))
            })
            .unwrap_or_else(|| "General".to_string());

        let files_summary = if turn_files_modified.is_empty() {
            "workspace".to_string()
        } else {
            turn_files_modified
                .iter()
                .take(4)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        };

        let redactor = SecretRedactor::global();
        let (trigger, failed_attempt, verified_fix) =
            if let Some((fail_reason, remediation)) = last_verification_failure {
                let clean_fail = redactor.redact(fail_reason);
                let clean_rem = redactor.redact(remediation);
                (
                    clean_fail.clone(),
                    format!(
                        "Edit on {} failed verification: {}",
                        files_summary, clean_fail
                    ),
                    format!(
                        "Applied remediation ({}) and verified all gates pass on {}.",
                        clean_rem, files_summary
                    ),
                )
            } else {
                (
                    format!(
                        "Modifications to {} required {} verification recovery iteration(s)",
                        files_summary, heal_attempts
                    ),
                    format!(
                        "Initial edit did not pass verification gates on {}",
                        files_summary
                    ),
                    format!(
                        "Verified all tests, lints, and compiler diagnostics pass on {}.",
                        files_summary
                    ),
                )
            };

        let gotcha = GlobalGotcha {
            id: format!("gotcha-{}", uuid::Uuid::new_v4().simple()),
            trigger,
            context_scope: tech_scope.clone(),
            failed_attempt,
            verified_fix,
            created_at: chrono::Utc::now().to_rfc3339(),
            occurrence_count: 1,
        };

        let persisted = match store.upsert_global_gotcha(gotcha) {
            Ok(g) => {
                tracing::info!(
                    scope = %tech_scope,
                    occurrences = g.occurrence_count,
                    "Autonomous ContinualSkillHarvester: persisted verified recovery gotcha"
                );
                g
            }
            Err(e) => {
                tracing::debug!("Failed to persist auto-learned gotcha: {}", e);
                return;
            }
        };

        // Synthesize or update a project-local procedural SKILL.md when the recovery required >= 2
        // iterations or the pitfall has recurred >= 2 times across sessions.
        if heal_attempts >= 2 || persisted.occurrence_count >= 2 {
            Self::synthesize_or_update_project_skill(store, &persisted, turn_files_modified);
        }
    }

    fn synthesize_or_update_project_skill(
        store: &VaultStore,
        gotcha: &GlobalGotcha,
        turn_files_modified: &[String],
    ) {
        let scope_slug = sanitize_skill_name(&gotcha.context_scope);
        let skill_name = if scope_slug.is_empty() {
            "learned-workspace-invariants".to_string()
        } else {
            format!("learned-{}-invariants", scope_slug)
        };

        let mut globs: BTreeSet<String> = BTreeSet::new();
        for f in turn_files_modified {
            if let Some(ext) = Path::new(f).extension().and_then(|e| e.to_str()) {
                if !ext.is_empty()
                    && ext.len() <= 10
                    && ext.chars().all(|c| c.is_ascii_alphanumeric())
                {
                    globs.insert(format!("*.{}", ext.to_ascii_lowercase()));
                }
            }
        }

        let mut triggers: BTreeSet<String> = BTreeSet::new();
        triggers.insert(gotcha.context_scope.to_ascii_lowercase());
        for tok in gotcha
            .trigger
            .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
            .filter(|t| t.len() >= 4)
            .take(4)
        {
            triggers.insert(tok.to_ascii_lowercase());
        }

        let entry_bullet = format!(
            "- **Trigger (`{}`)**: {}\n  - **Avoid**: {}\n  - **Verified Pattern**: {}",
            gotcha.context_scope,
            gotcha
                .trigger
                .lines()
                .next()
                .unwrap_or(&gotcha.trigger)
                .trim(),
            gotcha.failed_attempt.trim(),
            gotcha.verified_fix.trim()
        );

        let (instructions, merged_globs, merged_triggers) = match store.get_skill(&skill_name) {
            Ok(existing) if existing.scope == SkillScope::Project => {
                for g in existing.frontmatter.globs {
                    globs.insert(g);
                }
                for t in existing.frontmatter.triggers {
                    triggers.insert(t);
                }
                let trigger_head = gotcha
                    .trigger
                    .lines()
                    .next()
                    .unwrap_or(&gotcha.trigger)
                    .trim();
                let updated_body = if existing.instructions.contains(trigger_head) {
                    existing.instructions
                } else {
                    format!("{}\n\n{}", existing.instructions.trim(), entry_bullet)
                };
                // Bound skill body well below 20KB compaction limit
                let capped = if updated_body.len() > 12_000 {
                    let start = updated_body.ceil_char_boundary(updated_body.len() - 12_000);
                    format!(
                        "# Learned {} Verification Invariants\n\n{}",
                        gotcha.context_scope,
                        &updated_body[start..]
                    )
                } else {
                    updated_body
                };
                (
                    capped,
                    globs.into_iter().collect(),
                    triggers.into_iter().take(12).collect(),
                )
            }
            _ => (
                format!(
                    "# Learned {} Verification Invariants\n\n\
                     Automatically harvested from verified multi-step self-healing recoveries in this workspace:\n\n\
                     {}",
                    gotcha.context_scope, entry_bullet
                ),
                globs.into_iter().collect(),
                triggers.into_iter().take(12).collect(),
            ),
        };

        let description = format!(
            "Use when editing {} files in this workspace to prevent recurring verification failures and compiler/test regressions.",
            gotcha.context_scope
        );

        if let Err(e) = store.create_skill_with_tools(
            SkillScope::Project,
            &skill_name,
            &description,
            Some("quality"),
            Some("workflow"),
            &instructions,
            merged_triggers,
            merged_globs,
            vec![
                "read_file".to_string(),
                "patch_file".to_string(),
                "power_verify".to_string(),
            ],
        ) {
            tracing::debug!(
                skill = %skill_name,
                error = %e,
                "Skipped autonomous skill synthesis"
            );
        } else {
            tracing::info!(
                skill = %skill_name,
                "Autonomous ContinualSkillHarvester: synthesized/updated project SKILL.md"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_continual_skill_harvester_synthesizes_project_skill_on_recurring_gotcha() {
        let ws = tempdir().unwrap();
        let global_vault = tempdir().unwrap();
        let store =
            VaultStore::new(ws.path()).with_custom_global_vault(global_vault.path().to_path_buf());

        let modified = vec!["src/auth/middleware.rs".to_string()];
        let failure = (
            "error[E0382]: borrow of moved value `token`".to_string(),
            "clone token slice or borrow `&str` before passing to validator".to_string(),
        );

        // First single-attempt heal records gotcha (occurrence_count = 1)
        ContinualSkillHarvester::harvest_with_store(&store, &modified, 1, Some(&failure));
        let recalled = store.find_relevant_gotchas("E0382 borrow moved value token");
        assert_eq!(recalled.len(), 1);
        assert_eq!(recalled[0].context_scope, "Rust");

        // Second occurrence promotes it into `.minicode/skills/learned-rust-invariants/SKILL.md`
        ContinualSkillHarvester::harvest_with_store(&store, &modified, 1, Some(&failure));
        let skill = store.get_skill("learned-rust-invariants").unwrap();
        assert_eq!(skill.scope, SkillScope::Project);
        assert!(skill.instructions.contains("E0382"));
        assert!(skill.frontmatter.globs.contains(&"*.rs".to_string()));
    }
}

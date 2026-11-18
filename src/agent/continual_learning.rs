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
use std::collections::{BTreeMap, BTreeSet};
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

    /// Computes the dominant technology scope across `turn_files_modified` by plurality vote,
    /// weighting primary source languages above auxiliary config/documentation files.
    fn resolve_dominant_tech_scope(turn_files_modified: &[String]) -> String {
        let mut freq: BTreeMap<String, usize> = BTreeMap::new();
        for f in turn_files_modified {
            let label = WorkspaceProfile::language_label_for_path(Path::new(f));
            *freq.entry(label).or_insert(0) += 1;
        }
        freq.into_iter()
            .max_by_key(|(label, count)| {
                let tier: u8 = match label.as_str() {
                    "General" => 0,
                    "Config" | "Markdown" => 1,
                    _ => 2,
                };
                (tier, *count)
            })
            .map(|(label, _)| label)
            .unwrap_or_else(|| "General".to_string())
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

        // Skip transient filesystem/binary-stream read diagnostics and single-attempt unattributed retries
        if let Some((fail_reason, _)) = last_verification_failure {
            if fail_reason.contains("Failed to read modified file")
                || fail_reason.contains("stream did not contain valid UTF-8")
            {
                return;
            }
        } else if heal_attempts < 2 {
            return;
        }

        let tech_scope = Self::resolve_dominant_tech_scope(turn_files_modified);

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
                if heal_attempts >= 2 || g.occurrence_count >= 2 {
                    tracing::info!(
                        scope = %tech_scope,
                        occurrences = g.occurrence_count,
                        "Autonomous ContinualSkillHarvester: persisted recurring verified recovery gotcha"
                    );
                } else {
                    tracing::debug!(
                        scope = %tech_scope,
                        occurrences = g.occurrence_count,
                        "Autonomous ContinualSkillHarvester: recorded initial recovery candidate"
                    );
                }
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
                let updated_body = merge_learned_invariant_bullet(
                    &gotcha.context_scope,
                    &existing.instructions,
                    &entry_bullet,
                );
                (
                    updated_body,
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

/// Deduplicates and bounds learned invariant bullets using Jaccard stem-set similarity
/// ($J(A, B) = |A \cap B| / |A \cup B| \ge 0.60$) so semantically equivalent compiler/test
/// diagnostics replace stale entries in place rather than accumulating unbounded duplicates.
fn merge_learned_invariant_bullet(
    context_scope: &str,
    existing_instructions: &str,
    new_bullet: &str,
) -> String {
    const MAX_BULLETS: usize = 12;

    let stem_set = |text: &str| -> BTreeSet<u64> {
        crate::vault::ranking::tokenize_with_stems(text)
            .into_iter()
            .filter(|(raw, _)| !raw.chars().all(|c| c.is_ascii_digit()))
            .map(|(_, h)| h)
            .collect()
    };

    let mut bullets: Vec<String> = existing_instructions
        .split("\n- **Trigger")
        .enumerate()
        .filter_map(|(i, chunk)| {
            if i == 0 {
                chunk
                    .find("- **Trigger")
                    .map(|pos| chunk[pos..].trim().to_string())
            } else {
                Some(format!("- **Trigger{}", chunk.trim_end()))
            }
        })
        .collect();

    let new_stems = stem_set(new_bullet);
    let mut replaced = false;
    for existing_b in &mut bullets {
        let ex_stems = stem_set(existing_b);
        let inter = new_stems.intersection(&ex_stems).count();
        let union = new_stems.union(&ex_stems).count().max(1);
        if (inter as f64 / union as f64) >= 0.60 {
            *existing_b = new_bullet.to_string();
            replaced = true;
            break;
        }
    }

    if !replaced {
        bullets.push(new_bullet.to_string());
    }
    if bullets.len() > MAX_BULLETS {
        let drain_count = bullets.len() - MAX_BULLETS;
        bullets.drain(0..drain_count);
    }

    format!(
        "# Learned {} Verification Invariants\n\n\
         Automatically harvested from verified multi-step self-healing recoveries in this workspace:\n\n\
         {}",
        context_scope,
        bullets.join("\n\n")
    )
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

    #[test]
    fn test_minitest70_harvester_plurality_scope_and_skips_binary_utf8_read_noise() {
        let ws = tempdir().unwrap();
        let global_vault = tempdir().unwrap();
        let store =
            VaultStore::new(ws.path()).with_custom_global_vault(global_vault.path().to_path_buf());

        // Leading `.gitignore` + `package.json` (`Config`) must not overshadow plurality `TypeScript` files
        let modified = vec![
            ".gitignore".to_string(),
            "package.json".to_string(),
            "src/Hero.tsx".to_string(),
            "src/Swarm.tsx".to_string(),
            "src/Tools.tsx".to_string(),
        ];

        // Binary UTF-8 read failure must be ignored completely
        let utf8_noise = (
            "[Gate 1: AST Syntax & Asset Integrity] Failed to read modified file `src/assets/hero.png`: stream did not contain valid UTF-8".to_string(),
            "Ensure `src/assets/hero.png` is accessible on disk.".to_string(),
        );
        ContinualSkillHarvester::harvest_with_store(&store, &modified, 1, Some(&utf8_noise));
        assert!(store.find_relevant_gotchas("UTF-8 hero.png").is_empty());

        // Real TypeScript diagnostic attributes to dominant `TypeScript` scope (not `Config`)
        let ts_failure = (
            "src/Hero.tsx:14: error TS6133: 'Sparkles' is declared but its value is never read."
                .to_string(),
            "Remove unused import from src/Hero.tsx".to_string(),
        );
        ContinualSkillHarvester::harvest_with_store(&store, &modified, 1, Some(&ts_failure));
        let recalled = store.find_relevant_gotchas("TS6133 unused import Sparkles");
        assert_eq!(recalled.len(), 1);
        assert_eq!(recalled[0].context_scope, "TypeScript/React");
    }
}

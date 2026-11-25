//! Cross-session repository micro-skills: deterministic extraction of recurring environment
//! & command facts, multi-session corroboration gating, and automatic contradiction eviction.
//!
//! Inspired by `microsoft/TokenEconomizer` (Apache-2.0, arXiv:2610.22732), implemented natively
//! in pure Rust at 0 extra LLM tokens.

use crate::sandbox::redact::SecretRedactor;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Minimum independent corroborations (sessions or verified fail→recover pairs) before a
/// micro-skill is promoted to active prompt injection.
pub const MICRO_SKILL_CONFIRM_THRESHOLD: u32 = 2;

/// Maximum active micro-skills injected at session start to keep Zone 1 compact and KV-friendly.
pub const MAX_ACTIVE_MICRO_SKILLS: usize = 6;

/// Maximum stored micro-skills (active + candidates) per repository.
const MAX_STORED_MICRO_SKILLS: usize = 48;

/// Relative path under workspace root for persisted repository micro-skills.
pub const MICRO_SKILLS_REL_PATH: &str = ".minicode/economy/micro_skills.json";

/// A concise, single-line repository environment or command fact learned from experience.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MicroSkill {
    pub id: String,
    /// Concise, actionable 1-line operational fact injected when `active == true`.
    pub fact: String,
    /// Command/tool prefix that previously failed (e.g. `pytest`, `npm test`).
    pub failed_command: String,
    /// Verified working command prefix that succeeded (e.g. `python3 -m pytest`, `bun run test`).
    pub preferred_command: String,
    /// Distinct session IDs that corroborated this micro-skill.
    #[serde(default)]
    pub confirmed_sessions: BTreeSet<String>,
    /// Total verified confirmations observed.
    pub confirmations: u32,
    /// Contradictions observed when `preferred_command` later failed.
    #[serde(default)]
    pub contradictions: u32,
    /// True once corroborated (`confirmed_sessions.len() >= 2` or `confirmations >= 2`) and not contradicted.
    pub active: bool,
    pub updated_at: String,
}

/// Persistent per-repository micro-skill registry.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MicroSkillStore {
    #[serde(default)]
    pub skills: Vec<MicroSkill>,
}

/// In-turn tracker observing `exec_cmd` failures and recoveries to extract verified micro-skills
/// and detect contradictions against already-active micro-skills.
#[derive(Debug, Clone, Default)]
pub struct TurnCommandTracker {
    /// Recent failed command `(command_prefix, normalized_error)` observed in this turn.
    recent_failures: Vec<(String, String)>,
    /// Verified `(failed_prefix, working_prefix, error_hint)` transitions discovered this turn.
    pub discovered_recoveries: Vec<(String, String, String)>,
    /// Command prefixes that succeeded cleanly in this turn (for cross-session corroboration).
    pub succeeded_commands: Vec<String>,
    /// Command prefixes that failed with environment/missing-binary errors (for contradiction eviction).
    pub env_failed_commands: Vec<String>,
}

impl TurnCommandTracker {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        self.recent_failures.clear();
        self.discovered_recoveries.clear();
        self.succeeded_commands.clear();
        self.env_failed_commands.clear();
    }

    /// Observes a single `exec_cmd` invocation and result.
    pub fn observe_exec_cmd(&mut self, command: &str, success: bool, output: &str) {
        let Some(prefix) = normalize_command_prefix(command) else {
            return;
        };

        if !success {
            if let Some(env_err) = detect_environment_failure_hint(output) {
                self.env_failed_commands.push(prefix.clone());
                if self.recent_failures.len() >= 8 {
                    self.recent_failures.remove(0);
                }
                self.recent_failures.push((prefix, env_err.to_string()));
            }
            return;
        }

        // Command succeeded: record for runner corroboration and check if it recovers a recent env failure.
        if !self.succeeded_commands.contains(&prefix) {
            self.succeeded_commands.push(prefix.clone());
        }

        if let Some(pos) = self.recent_failures.iter().rposition(|(failed_prefix, _)| {
            failed_prefix != &prefix && are_related_commands(failed_prefix, &prefix)
        }) {
            let (failed_prefix, err_hint) = self.recent_failures.remove(pos);
            self.discovered_recoveries
                .push((failed_prefix, prefix, err_hint));
        }
    }
}

impl MicroSkillStore {
    fn store_path(workspace_root: &Path) -> PathBuf {
        workspace_root.join(MICRO_SKILLS_REL_PATH)
    }

    /// Loads the workspace micro-skill store (returns empty default if absent or malformed).
    #[must_use]
    pub fn load(workspace_root: &Path) -> Self {
        let path = Self::store_path(workspace_root);
        std::fs::read_to_string(path)
            .ok()
            .and_then(|raw| serde_json::from_str::<Self>(&raw).ok())
            .unwrap_or_default()
    }

    /// Persists the workspace micro-skill store cleanly under `.minicode/economy/`.
    pub fn save(&self, workspace_root: &Path) {
        let path = Self::store_path(workspace_root);
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, json);
        }
    }

    /// Applies turn-end observations from [`TurnCommandTracker`]:
    /// 1. Evicts any active micro-skill whose `preferred_command` failed with an environment error.
    /// 2. Corroborates or records newly discovered fail→recover micro-skills (redacted).
    ///
    /// Returns `(newly_activated_count, evicted_count)`.
    pub fn apply_turn_observations(
        &mut self,
        session_id: &str,
        tracker: &TurnCommandTracker,
    ) -> (usize, usize) {
        let now = chrono::Utc::now().to_rfc3339();
        let redactor = SecretRedactor::global();
        let mut newly_activated = 0usize;
        let mut evicted = 0usize;

        // 1. Automatic contradiction eviction
        for failed_prefix in &tracker.env_failed_commands {
            for skill in &mut self.skills {
                if skill.preferred_command == *failed_prefix {
                    skill.contradictions = skill.contradictions.saturating_add(1);
                    skill.updated_at = now.clone();
                    if skill.active {
                        skill.active = false;
                        evicted += 1;
                    }
                }
            }
        }

        // 2. Corroborate existing micro-skills when their preferred_command succeeded in a new session
        for ok_prefix in &tracker.succeeded_commands {
            for skill in &mut self.skills {
                if skill.preferred_command == *ok_prefix && skill.contradictions == 0 {
                    let was_active = skill.active;
                    if skill.confirmed_sessions.insert(session_id.to_string()) {
                        skill.confirmations = skill.confirmations.saturating_add(1);
                        skill.updated_at = now.clone();
                        if !was_active
                            && (skill.confirmed_sessions.len() as u32
                                >= MICRO_SKILL_CONFIRM_THRESHOLD
                                || skill.confirmations >= MICRO_SKILL_CONFIRM_THRESHOLD)
                        {
                            skill.active = true;
                            newly_activated += 1;
                        }
                    }
                }
            }
        }

        // 3. Upsert newly discovered fail -> recover pairs
        for (failed_cmd, working_cmd, err_hint) in &tracker.discovered_recoveries {
            let clean_failed = redactor.redact(failed_cmd);
            let clean_working = redactor.redact(working_cmd);
            let clean_hint = redactor.redact(err_hint);
            let id = format!("ms:{clean_failed}->{clean_working}");

            if let Some(existing) = self.skills.iter_mut().find(|s| s.id == id) {
                let was_active = existing.active;
                existing.confirmed_sessions.insert(session_id.to_string());
                existing.confirmations = existing.confirmations.saturating_add(1);
                existing.contradictions = 0;
                existing.updated_at = now.clone();
                if existing.confirmations >= MICRO_SKILL_CONFIRM_THRESHOLD
                    || existing.confirmed_sessions.len() as u32 >= MICRO_SKILL_CONFIRM_THRESHOLD
                {
                    existing.active = true;
                    if !was_active {
                        newly_activated += 1;
                    }
                }
            } else {
                let mut sessions = BTreeSet::new();
                sessions.insert(session_id.to_string());
                let fact = format!(
                    "Prefer `{clean_working}` over `{clean_failed}` in this repo (`{clean_failed}` failed: {clean_hint})."
                );
                self.skills.push(MicroSkill {
                    id,
                    fact,
                    failed_command: clean_failed,
                    preferred_command: clean_working,
                    confirmed_sessions: sessions,
                    confirmations: 1,
                    contradictions: 0,
                    active: false,
                    updated_at: now.clone(),
                });
            }
        }

        // Cap stored entries, prioritizing active and highest-confirmation entries
        if self.skills.len() > MAX_STORED_MICRO_SKILLS {
            self.skills.sort_by(|a, b| {
                b.active
                    .cmp(&a.active)
                    .then_with(|| b.confirmations.cmp(&a.confirmations))
                    .then_with(|| b.updated_at.cmp(&a.updated_at))
            });
            self.skills.truncate(MAX_STORED_MICRO_SKILLS);
        }

        (newly_activated, evicted)
    }

    /// Returns currently active (corroborated & non-contradicted) micro-skills, capped cleanly.
    #[must_use]
    pub fn active_skills(&self) -> Vec<&MicroSkill> {
        let mut active: Vec<&MicroSkill> = self
            .skills
            .iter()
            .filter(|s| s.active && s.contradictions == 0)
            .collect();
        active.sort_by(|a, b| {
            b.confirmations
                .cmp(&a.confirmations)
                .then_with(|| a.id.cmp(&b.id))
        });
        active.truncate(MAX_ACTIVE_MICRO_SKILLS);
        active
    }

    /// Formats active micro-skills as a compact, KV-stable prompt block for session startup.
    #[must_use]
    pub fn format_prompt_block(&self) -> Option<String> {
        let active = self.active_skills();
        if active.is_empty() {
            return None;
        }
        let mut out = String::from(
            "  <workspace_micro_skills source=\".minicode/economy/micro_skills.json\">\n",
        );
        for s in active {
            out.push_str(&format!(
                "    • {} (verified {}x)\n",
                s.fact, s.confirmations
            ));
        }
        out.push_str("  </workspace_micro_skills>\n");
        Some(out)
    }
}

/// Extracts a normalized 1–3 token command prefix (e.g. `pytest`, `python3 -m pytest`,
/// `bun run test`, `cargo test`) while ignoring chained shell glue or env assignments.
fn normalize_command_prefix(raw_cmd: &str) -> Option<String> {
    let trimmed = raw_cmd.trim();
    if trimmed.is_empty() || trimmed.len() > 240 {
        return None;
    }
    // Skip complex multi-command pipelines or subshells
    if trimmed.contains('\n')
        || trimmed.contains("&&")
        || trimmed.contains(';')
        || trimmed.contains('|')
    {
        return None;
    }
    let tokens: Vec<&str> = trimmed
        .split_whitespace()
        .skip_while(|t| t.contains('=') && !t.starts_with('-'))
        .collect();
    if tokens.is_empty() {
        return None;
    }
    let first = tokens[0];
    // Strip harmless `nice -n <N>` prefix so `nice -n 19 cargo test` normalizes cleanly
    let effective = if first == "nice" && tokens.len() >= 4 && tokens[1] == "-n" {
        &tokens[3..]
    } else {
        &tokens[..]
    };
    if effective.is_empty() {
        return None;
    }

    let bin = effective[0];
    // Only learn facts for well-known developer toolchains/runners, never arbitrary data/echo cmds
    const TRACKED_BINS: &[&str] = &[
        "pytest", "python", "python3", "uv", "pip", "pip3", "poetry", "npm", "npx", "pnpm", "yarn",
        "bun", "bunx", "node", "vitest", "jest", "tsc", "eslint", "cargo", "rustc", "go", "make",
        "just", "mvn", "gradle", "dotnet", "flutter", "dart", "mix", "bundle", "rspec", "rake",
    ];
    if !TRACKED_BINS.contains(&bin) {
        return None;
    }

    // Keep up to 3 semantic tokens (skip file paths / flags after the subcommand)
    let mut parts = vec![bin];
    for tok in effective.iter().skip(1).take(3) {
        if *tok == "-m" && (bin == "python" || bin == "python3") {
            parts.push(*tok);
            continue;
        }
        if tok.starts_with('-') || tok.contains('/') || tok.contains('.') {
            break;
        }
        parts.push(*tok);
        if parts.len() >= 3 {
            break;
        }
    }
    Some(parts.join(" "))
}

fn detect_environment_failure_hint(output: &str) -> Option<&'static str> {
    let lower = output.to_ascii_lowercase();
    if lower.contains("command not found") || lower.contains("not recognized as an internal") {
        Some("command not found")
    } else if lower.contains("no such file or directory") && lower.contains("exec") {
        Some("executable missing")
    } else if lower.contains("missing script:") || lower.contains("script not found") {
        Some("missing package script")
    } else if lower.contains("no module named") {
        Some("missing python module in default interpreter")
    } else if lower.contains("cannot find module") && lower.contains("node_modules") {
        Some("missing local node dependency")
    } else {
        None
    }
}

fn are_related_commands(failed: &str, working: &str) -> bool {
    let f_tokens: Vec<&str> = failed.split_whitespace().collect();
    let w_tokens: Vec<&str> = working.split_whitespace().collect();
    if f_tokens.is_empty() || w_tokens.is_empty() {
        return false;
    }
    // Same task word shared (e.g. `pytest` <-> `python3 -m pytest`, `npm test` <-> `bun run test`)
    for ft in &f_tokens {
        if ft.len() >= 3 && w_tokens.contains(ft) {
            return true;
        }
    }
    // Same ecosystem swap (e.g. `npm` <-> `bun`/`pnpm`/`yarn`, `python` <-> `python3`/`uv`)
    const JS_RUNNERS: &[&str] = &["npm", "npx", "pnpm", "yarn", "bun", "bunx", "node"];
    const PY_RUNNERS: &[&str] = &["python", "python3", "pytest", "uv", "pip", "pip3", "poetry"];
    let both_js = JS_RUNNERS.contains(&f_tokens[0]) && JS_RUNNERS.contains(&w_tokens[0]);
    let both_py = PY_RUNNERS.contains(&f_tokens[0]) && PY_RUNNERS.contains(&w_tokens[0]);
    both_js || both_py
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_micro_skill_requires_corroboration_and_evicts_on_contradiction() {
        let mut store = MicroSkillStore::default();

        // Session 1: `pytest` fails -> `python3 -m pytest` succeeds (candidate only, not active yet)
        let mut t1 = TurnCommandTracker::new();
        t1.observe_exec_cmd("pytest", false, "bash: pytest: command not found");
        t1.observe_exec_cmd("python3 -m pytest", true, "3 passed in 0.12s");
        let (act1, ev1) = store.apply_turn_observations("sess-1", &t1);
        assert_eq!((act1, ev1), (0, 0));
        assert!(
            store.active_skills().is_empty(),
            "single session must remain candidate until corroborated"
        );

        // Session 2: `python3 -m pytest` succeeds again -> corroborated across 2 sessions -> active!
        let mut t2 = TurnCommandTracker::new();
        t2.observe_exec_cmd("python3 -m pytest", true, "5 passed in 0.18s");
        let (act2, ev2) = store.apply_turn_observations("sess-2", &t2);
        assert_eq!((act2, ev2), (1, 0));
        let block = store.format_prompt_block().expect("prompt block");
        assert!(block.contains("python3 -m pytest"));
        assert!(block.contains("pytest"));

        // Session 3: environment changes and `python3 -m pytest` breaks -> automatically evicted!
        let mut t3 = TurnCommandTracker::new();
        t3.observe_exec_cmd(
            "python3 -m pytest",
            false,
            "/usr/bin/python3: No module named pytest",
        );
        let (act3, ev3) = store.apply_turn_observations("sess-3", &t3);
        assert_eq!((act3, ev3), (0, 1));
        assert!(store.active_skills().is_empty());
        assert!(store.format_prompt_block().is_none());
    }
}

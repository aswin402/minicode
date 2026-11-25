//! Native Rust **MiniEconomy** Engine (`src/context/budget/economy/`).
//!
//! Experience-driven, zero-LLM-overhead cross-session cost control inspired by
//! `microsoft/TokenEconomizer` (Apache-2.0, arXiv:2610.22732):
//! - **Micro-Skills (`micro_skills.rs`):** Learns corroborated repository command/env facts across
//!   sessions and auto-evicts them on contradiction.
//! - **Tool Pruning (`tool_pruner.rs`):** Omits cold non-essential tool schemas past dual
//!   prompt/session thresholds (`p=10, m=2` or `s=5`) while protecting core tools & allowing instant restore.
//! - **Edit→Test Loop Guard (`loop_guard.rs`):** Catches repeated identical compiler/test error
//!   signatures across differing file edits and emits constructive redirects.
//! - **Economy Ledger (`ledger.rs`):** Persists cumulative cross-session token spend, cache hit
//!   efficiency, and savings under `.minicode/economy/`.

pub mod ledger;
pub mod loop_guard;
pub mod micro_skills;
pub mod tool_pruner;

pub use ledger::{EconomyLedger, TurnEconomyRecord, TurnLedgerInput, ECONOMY_LEDGER_REL_PATH};
pub use loop_guard::{EditTestLoopGuard, LoopIntervention};
pub use micro_skills::{
    MicroSkill, MicroSkillStore, TurnCommandTracker, MICRO_SKILLS_REL_PATH,
    MICRO_SKILL_CONFIRM_THRESHOLD,
};
pub use tool_pruner::{
    ToolUsageEntry, ToolUsagePruner, ABSOLUTE_SESSIONS_FOR_PRUNE, MIN_PROMPTS_FOR_PRUNE,
    MIN_SESSIONS_FOR_PRUNE, TOOL_USAGE_REL_PATH,
};

use crate::agent::pricing::ModelPricing;
use crate::agent::provider::ToolSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;
use std::sync::{Mutex, OnceLock, RwLock};

static ECONOMY_IO_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
static ACTIVE_SESSION_ID: OnceLock<RwLock<Option<String>>> = OnceLock::new();

fn economy_io_lock() -> &'static Mutex<()> {
    ECONOMY_IO_LOCK.get_or_init(|| Mutex::new(()))
}

fn active_session_cell() -> &'static RwLock<Option<String>> {
    ACTIVE_SESSION_ID.get_or_init(|| RwLock::new(None))
}

/// Snapshot returned by `minicode economy` CLI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EconomyStatusSnapshot {
    pub sessions: usize,
    pub total_turns: u64,
    pub total_tokens: u64,
    pub cached_tokens: u64,
    pub cache_hit_pct: f64,
    pub total_cost_usd: f64,
    pub estimated_tokens_saved: u64,
    pub loop_interventions: u64,
    pub active_micro_skills: Vec<MicroSkill>,
    pub candidate_micro_skills: usize,
    pub pruned_tools: Vec<String>,
}

/// Parameters for recording turn-end economy state across all MiniEconomy stores.
pub struct TurnEconomyFinish<'a> {
    pub session_id: &'a str,
    pub source: &'a str,
    pub turn_id: usize,
    pub provider: &'a str,
    pub model: &'a str,
    pub total_tokens: usize,
    pub cached_tokens: usize,
    pub duration_ms: u64,
    pub exposed_tools: &'a [String],
    pub invoked_tools: &'a [String],
    pub pruned_tools_count: usize,
    pub est_tokens_saved: usize,
    pub loop_interventions: usize,
    pub command_tracker: &'a TurnCommandTracker,
}

/// High-level facade for the native Rust MiniEconomy subsystem.
pub struct MiniEconomy;

impl MiniEconomy {
    /// Registers the active parent orchestrator session ID so parallel subagent workers share
    /// the parent's session identity when recording ledger spend and micro-skill corroboration.
    pub fn set_active_session_id(session_id: &str) {
        if let Ok(mut guard) = active_session_cell().write() {
            *guard = Some(session_id.to_string());
        }
    }

    /// Returns the currently active parent orchestrator session ID, if any.
    #[must_use]
    pub fn active_session_id() -> Option<String> {
        active_session_cell().read().ok().and_then(|g| g.clone())
    }

    /// Returns the KV-cache-friendly `<workspace_micro_skills>` prompt block for active
    /// corroborated facts, or `None` if no corroborated micro-skills exist yet.
    #[must_use]
    pub fn format_prompt_micro_skills(workspace_root: &Path) -> Option<String> {
        let store = MicroSkillStore::load(workspace_root);
        store.format_prompt_block()
    }

    /// Prunes cold non-essential tool schemas from `schemas` based on persisted repository usage,
    /// preserving all protected core tools and `explicitly_requested` tools.
    /// Returns `(pruned_tool_names, estimated_tokens_saved)`.
    pub fn prune_inactive_schemas(
        workspace_root: &Path,
        schemas: &mut Vec<ToolSchema>,
        explicitly_requested: &HashSet<String>,
    ) -> (Vec<String>, usize) {
        let pruner = ToolUsagePruner::load(workspace_root);
        if pruner.tools.is_empty() {
            return (Vec::new(), 0);
        }
        pruner.filter_schemas(schemas, explicitly_requested)
    }

    /// Updates tool usage counters, repository micro-skills, and the cumulative economy ledger
    /// at the end of a completed turn (serialized across parallel subagent workers).
    pub fn record_turn_complete(workspace_root: &Path, finish: &TurnEconomyFinish<'_>) {
        let _guard = economy_io_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        // 1. Update tool utilization & dual-threshold pruning state
        let mut pruner = ToolUsagePruner::load(workspace_root);
        pruner.record_turn(
            finish.session_id,
            finish.exposed_tools,
            finish.invoked_tools,
        );
        pruner.save(workspace_root);

        // 2. Apply command fail->recover observations & contradiction checks to micro-skills
        let mut ms_store = MicroSkillStore::load(workspace_root);
        let (activated, evicted) =
            ms_store.apply_turn_observations(finish.session_id, finish.command_tracker);
        if !ms_store.skills.is_empty() {
            ms_store.save(workspace_root);
        }

        // 3. Record turn metrics into the unified cost & savings ledger
        let mut ledger = EconomyLedger::load(workspace_root);
        ledger.record_turn(&TurnLedgerInput {
            session_id: finish.session_id,
            source: finish.source,
            turn_id: finish.turn_id,
            provider: finish.provider,
            model: finish.model,
            total_tokens: finish.total_tokens,
            cached_tokens: finish.cached_tokens,
            duration_ms: finish.duration_ms,
            pruned_tools_count: finish.pruned_tools_count,
            est_tokens_saved: finish.est_tokens_saved,
            loop_interventions: finish.loop_interventions,
            micro_skills_activated: activated,
            micro_skills_evicted: evicted,
        });
        ledger.save(workspace_root);
    }

    /// Builds a structured status snapshot across ledger, micro-skills, and pruned tools.
    #[must_use]
    pub fn status_snapshot(workspace_root: &Path) -> EconomyStatusSnapshot {
        let ledger = EconomyLedger::load(workspace_root);
        let ms_store = MicroSkillStore::load(workspace_root);
        let pruner = ToolUsagePruner::load(workspace_root);

        let active_micro_skills: Vec<MicroSkill> =
            ms_store.active_skills().into_iter().cloned().collect();
        let candidate_micro_skills = ms_store
            .skills
            .iter()
            .filter(|s| !s.active && s.contradictions == 0)
            .count();

        EconomyStatusSnapshot {
            sessions: ledger.seen_sessions.len(),
            total_turns: ledger.total_turns,
            total_tokens: ledger.total_tokens,
            cached_tokens: ledger.total_cached_tokens,
            cache_hit_pct: ledger.cache_hit_pct(),
            total_cost_usd: ledger.total_cost_usd,
            estimated_tokens_saved: ledger.estimated_tokens_saved,
            loop_interventions: ledger.total_loop_interventions,
            active_micro_skills,
            candidate_micro_skills,
            pruned_tools: pruner.pruned_tools(),
        }
    }

    /// Formats a concise human-readable MiniEconomy dashboard for `minicode economy`.
    #[must_use]
    pub fn format_status_report(snapshot: &EconomyStatusSnapshot) -> String {
        let mut out = String::from("### 💰 MiniEconomy — Repository Token Cost Control\n\n");
        out.push_str(&format!(
            "• **Sessions / Turns Tracked:** {} session(s), {} turn(s)\n",
            snapshot.sessions, snapshot.total_turns
        ));
        out.push_str(&format!(
            "• **Cumulative Tokens:** {} ({} cached • {:.1}% cache hit)\n",
            snapshot.total_tokens, snapshot.cached_tokens, snapshot.cache_hit_pct
        ));
        out.push_str(&format!(
            "• **Estimated Spend:** {}\n",
            ModelPricing::format_cost(snapshot.total_cost_usd)
        ));
        out.push_str(&format!(
            "• **Schema Pruning Savings:** ~{} tokens saved | **Loop Guards Triggered:** {}\n",
            snapshot.estimated_tokens_saved, snapshot.loop_interventions
        ));

        out.push_str(&format!(
            "\n#### Learned Micro-Skills ({} active, {} candidate)\n",
            snapshot.active_micro_skills.len(),
            snapshot.candidate_micro_skills
        ));
        if snapshot.active_micro_skills.is_empty() {
            out.push_str("- _(None active yet — facts promote automatically after 2 corroborated sessions.)_\n");
        } else {
            for s in &snapshot.active_micro_skills {
                out.push_str(&format!("- {} _(verified {}x)_\n", s.fact, s.confirmations));
            }
        }

        out.push_str(&format!(
            "\n#### Pruned Unused Tools ({})\n",
            snapshot.pruned_tools.len()
        ));
        if snapshot.pruned_tools.is_empty() {
            out.push_str("- _(None pruned yet — cold non-essential tools prune after 10 prompts across 2+ sessions or 5 unused sessions.)_\n");
        } else {
            out.push_str(&format!(
                "- {}\n  _(Restore anytime with `minicode economy restore [tool]` or `activate_tools`.)_\n",
                snapshot
                    .pruned_tools
                    .iter()
                    .map(|t| format!("`{t}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        out
    }

    /// Restores one or all pruned tools and persists the updated state.
    pub fn restore_tools(workspace_root: &Path, tool_name: Option<&str>) -> usize {
        let _guard = economy_io_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut pruner = ToolUsagePruner::load(workspace_root);
        let restored = pruner.restore(tool_name);
        pruner.save(workspace_root);
        restored
    }

    /// Clears all persisted MiniEconomy state for this workspace (`.minicode/economy/`).
    pub fn reset(workspace_root: &Path) {
        let _guard = economy_io_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let dir = workspace_root.join(".minicode/economy");
        if dir.is_dir() {
            let _ = std::fs::remove_dir_all(dir);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mini_economy_end_to_end_lifecycle_and_reset() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = tmp.path();

        let mut t1 = TurnCommandTracker::new();
        t1.observe_exec_cmd("pytest", false, "sh: pytest: command not found");
        t1.observe_exec_cmd("python3 -m pytest", true, "4 passed");
        MiniEconomy::record_turn_complete(
            ws,
            &TurnEconomyFinish {
                session_id: "sess-a",
                source: "orchestrator",
                turn_id: 1,
                provider: "openai",
                model: "gpt-4o-mini",
                total_tokens: 12_000,
                cached_tokens: 6_000,
                duration_ms: 2500,
                exposed_tools: &["read_file".to_string(), "vault_bundle_show".to_string()],
                invoked_tools: &["read_file".to_string()],
                pruned_tools_count: 0,
                est_tokens_saved: 0,
                loop_interventions: 0,
                command_tracker: &t1,
            },
        );

        let mut t2 = TurnCommandTracker::new();
        t2.observe_exec_cmd("python3 -m pytest", true, "4 passed");
        MiniEconomy::record_turn_complete(
            ws,
            &TurnEconomyFinish {
                session_id: "sess-b",
                source: "orchestrator",
                turn_id: 1,
                provider: "openai",
                model: "gpt-4o-mini",
                total_tokens: 8_000,
                cached_tokens: 5_000,
                duration_ms: 1800,
                exposed_tools: &["read_file".to_string(), "vault_bundle_show".to_string()],
                invoked_tools: &["read_file".to_string()],
                pruned_tools_count: 1,
                est_tokens_saved: 120,
                loop_interventions: 1,
                command_tracker: &t2,
            },
        );

        let snap = MiniEconomy::status_snapshot(ws);
        assert_eq!(snap.sessions, 2);
        assert_eq!(snap.total_turns, 2);
        assert_eq!(snap.active_micro_skills.len(), 1);
        assert!(MiniEconomy::format_prompt_micro_skills(ws)
            .unwrap()
            .contains("python3 -m pytest"));
        let report = MiniEconomy::format_status_report(&snap);
        assert!(report.contains("MiniEconomy"));
        assert!(report.contains("python3 -m pytest"));

        MiniEconomy::reset(ws);
        assert_eq!(MiniEconomy::status_snapshot(ws).total_turns, 0);
    }
}

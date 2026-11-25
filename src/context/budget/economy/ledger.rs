//! Unified cross-session cost, cache-efficiency, and savings ledger (`.minicode/economy/ledger.json`).
//!
//! Inspired by `microsoft/TokenEconomizer`'s `tokeneconomizer.ledger` & `pricing` modules,
//! leveraging `minicode`'s native [`crate::agent::pricing::ModelPricing`].

use crate::agent::pricing::ModelPricing;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Relative path under workspace root for the persistent token economy ledger.
pub const ECONOMY_LEDGER_REL_PATH: &str = ".minicode/economy/ledger.json";

/// Maximum recent turn records retained in the ledger ring buffer.
const MAX_RECENT_ENTRIES: usize = 25;

fn default_turn_source() -> String {
    "orchestrator".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnEconomyRecord {
    pub timestamp: String,
    pub session_id: String,
    #[serde(default = "default_turn_source")]
    pub source: String,
    pub turn_id: usize,
    pub provider: String,
    pub model: String,
    pub total_tokens: usize,
    pub cached_tokens: usize,
    pub cost_usd: f64,
    pub duration_ms: u64,
    pub pruned_tools_count: usize,
    pub est_tokens_saved: usize,
    pub loop_interventions: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EconomyLedger {
    #[serde(default)]
    pub seen_sessions: BTreeSet<String>,
    pub total_turns: u64,
    pub total_tokens: u64,
    pub total_cached_tokens: u64,
    pub total_cost_usd: f64,
    pub estimated_tokens_saved: u64,
    pub total_loop_interventions: u64,
    pub total_micro_skills_activated: u64,
    pub total_micro_skills_evicted: u64,
    #[serde(default)]
    pub recent_turns: Vec<TurnEconomyRecord>,
}

/// Inputs for recording one completed turn in [`EconomyLedger`].
pub struct TurnLedgerInput<'a> {
    pub session_id: &'a str,
    pub source: &'a str,
    pub turn_id: usize,
    pub provider: &'a str,
    pub model: &'a str,
    pub total_tokens: usize,
    pub cached_tokens: usize,
    pub duration_ms: u64,
    pub pruned_tools_count: usize,
    pub est_tokens_saved: usize,
    pub loop_interventions: usize,
    pub micro_skills_activated: usize,
    pub micro_skills_evicted: usize,
}

impl EconomyLedger {
    fn store_path(workspace_root: &Path) -> PathBuf {
        workspace_root.join(ECONOMY_LEDGER_REL_PATH)
    }

    #[must_use]
    pub fn load(workspace_root: &Path) -> Self {
        let path = Self::store_path(workspace_root);
        std::fs::read_to_string(path)
            .ok()
            .and_then(|raw| serde_json::from_str::<Self>(&raw).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, workspace_root: &Path) {
        let path = Self::store_path(workspace_root);
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, json);
        }
    }

    /// Appends a completed turn to the persistent repository economy ledger.
    pub fn record_turn(&mut self, input: &TurnLedgerInput<'_>) {
        // Approximate prompt vs completion split (85/15) when only aggregate total is available
        let prompt_est = (input.total_tokens.saturating_mul(85)) / 100;
        let comp_est = input.total_tokens.saturating_sub(prompt_est);
        let cost_usd =
            ModelPricing::calculate_cost(input.provider, input.model, prompt_est, comp_est);

        self.seen_sessions.insert(input.session_id.to_string());
        self.total_turns = self.total_turns.saturating_add(1);
        self.total_tokens = self.total_tokens.saturating_add(input.total_tokens as u64);
        self.total_cached_tokens = self
            .total_cached_tokens
            .saturating_add(input.cached_tokens as u64);
        self.total_cost_usd += cost_usd;
        self.estimated_tokens_saved = self
            .estimated_tokens_saved
            .saturating_add(input.est_tokens_saved as u64);
        self.total_loop_interventions = self
            .total_loop_interventions
            .saturating_add(input.loop_interventions as u64);
        self.total_micro_skills_activated = self
            .total_micro_skills_activated
            .saturating_add(input.micro_skills_activated as u64);
        self.total_micro_skills_evicted = self
            .total_micro_skills_evicted
            .saturating_add(input.micro_skills_evicted as u64);

        self.recent_turns.push(TurnEconomyRecord {
            timestamp: chrono::Utc::now().to_rfc3339(),
            session_id: input.session_id.to_string(),
            source: input.source.to_string(),
            turn_id: input.turn_id,
            provider: input.provider.to_string(),
            model: input.model.to_string(),
            total_tokens: input.total_tokens,
            cached_tokens: input.cached_tokens,
            cost_usd,
            duration_ms: input.duration_ms,
            pruned_tools_count: input.pruned_tools_count,
            est_tokens_saved: input.est_tokens_saved,
            loop_interventions: input.loop_interventions,
        });
        if self.recent_turns.len() > MAX_RECENT_ENTRIES {
            let drop_count = self.recent_turns.len() - MAX_RECENT_ENTRIES;
            self.recent_turns.drain(0..drop_count);
        }
    }

    #[must_use]
    pub fn cache_hit_pct(&self) -> f64 {
        if self.total_tokens == 0 {
            0.0
        } else {
            ((self.total_cached_tokens as f64 / self.total_tokens as f64) * 100.0).min(100.0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_economy_ledger_accumulates_cost_cache_and_savings() {
        let mut ledger = EconomyLedger::default();
        ledger.record_turn(&TurnLedgerInput {
            session_id: "s1",
            source: "orchestrator",
            turn_id: 1,
            provider: "gemini",
            model: "gemini-2.5-flash",
            total_tokens: 10_000,
            cached_tokens: 7_500,
            duration_ms: 4200,
            pruned_tools_count: 3,
            est_tokens_saved: 450,
            loop_interventions: 1,
            micro_skills_activated: 1,
            micro_skills_evicted: 0,
        });
        assert_eq!(ledger.seen_sessions.len(), 1);
        assert_eq!(ledger.total_turns, 1);
        assert_eq!(ledger.estimated_tokens_saved, 450);
        assert_eq!(ledger.total_loop_interventions, 1);
        assert!((ledger.cache_hit_pct() - 75.0).abs() < 0.01);
        assert!(ledger.total_cost_usd > 0.0);
    }
}

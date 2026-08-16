use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Fact category determining temporal decay resistance.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryScope {
    /// Zero decay — permanent developer preferences, repo architectural rules, build commands.
    Permanent,
    /// Gradual decay — active milestones, design decisions, component patterns (half-life: ~7 days).
    Milestone,
    /// Fast decay — temporary debugging findings, unverified hypotheses (half-life: ~60 mins).
    Transient,
}

/// A timestamped memory fact with decay tracking and reinforcement counter.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScopedMemoryFact {
    pub id: String,
    pub scope: MemoryScope,
    pub content: String,
    pub created_at: u64, // Unix timestamp in seconds
    pub last_accessed_at: u64,
    pub reinforcement_count: u32,
}

#[allow(dead_code)]
impl ScopedMemoryFact {
    /// Creates a new scoped memory fact with current timestamp.
    pub fn new(id: impl Into<String>, scope: MemoryScope, content: impl Into<String>) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::from_secs(0))
            .as_secs();

        Self {
            id: id.into(),
            scope,
            content: content.into(),
            created_at: now,
            last_accessed_at: now,
            reinforcement_count: 1,
        }
    }

    /// Reinforces the memory fact, resetting its decay clock and increasing stability.
    pub fn reinforce(&mut self) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::from_secs(0))
            .as_secs();
        self.last_accessed_at = now;
        self.reinforcement_count = self.reinforcement_count.saturating_add(1);
    }

    /// Calculates current retention strength between 0.0 and 1.0 using biological decay math.
    pub fn calculate_retention(&self, current_time_secs: u64) -> f32 {
        if self.scope == MemoryScope::Permanent {
            return 1.0;
        }

        let elapsed_secs = current_time_secs.saturating_sub(self.last_accessed_at) as f32;
        let stability_factor = 1.0 + (self.reinforcement_count as f32 * 0.5);

        let half_life_secs = match self.scope {
            MemoryScope::Permanent => f32::MAX,
            MemoryScope::Milestone => 7.0 * 24.0 * 3600.0, // 7 days
            MemoryScope::Transient => 3600.0,              // 60 minutes
        };

        // Exponential decay: R(t) = exp(-ln(2) * t / (half_life * stability))
        let effective_half_life = half_life_secs * stability_factor;
        let exponent = -std::f32::consts::LN_2 * (elapsed_secs / effective_half_life);
        exponent.exp().clamp(0.0, 1.0)
    }

    /// Checks if the fact is still active above the retention threshold.
    pub fn is_active(&self, current_time_secs: u64, threshold: f32) -> bool {
        self.calculate_retention(current_time_secs) >= threshold
    }
}

/// In-memory cognitive memory manager with temporal decay filtering.
#[allow(dead_code)]
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct CognitiveMemoryManager {
    pub facts: Vec<ScopedMemoryFact>,
}

#[allow(dead_code)]
impl CognitiveMemoryManager {
    pub fn new() -> Self {
        Self { facts: Vec::new() }
    }

    pub fn insert_or_reinforce(
        &mut self,
        id: impl Into<String>,
        scope: MemoryScope,
        content: impl Into<String>,
    ) {
        let id_str = id.into();
        let content_str = content.into();

        if let Some(existing) = self.facts.iter_mut().find(|f| f.id == id_str) {
            existing.content = content_str;
            existing.reinforce();
        } else {
            self.facts
                .push(ScopedMemoryFact::new(id_str, scope, content_str));
        }
    }

    /// Prunes facts whose retention has decayed below threshold (excluding Permanent).
    pub fn prune_decayed(&mut self, current_time_secs: u64, threshold: f32) {
        self.facts
            .retain(|f| f.is_active(current_time_secs, threshold));
    }

    /// Formats active facts sorted by retention score into a prompt context block.
    pub fn format_prompt_block(&self, current_time_secs: u64, threshold: f32) -> String {
        let mut active_facts: Vec<(&ScopedMemoryFact, f32)> = self
            .facts
            .iter()
            .map(|f| (f, f.calculate_retention(current_time_secs)))
            .filter(|(_, r)| *r >= threshold)
            .collect();

        if active_facts.is_empty() {
            return String::new();
        }

        // Sort descending by retention score
        active_facts.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let mut out = "🧠 Active Cognitive Memory & Grounded Rules:\n".to_string();
        for (fact, score) in active_facts {
            let scope_badge = match fact.scope {
                MemoryScope::Permanent => "📌 [RULE]",
                MemoryScope::Milestone => "🎯 [MILESTONE]",
                MemoryScope::Transient => "⏳ [EPISODIC]",
            };
            out.push_str(&format!(
                "  {} `{}` (strength: {:.0}%): {}\n",
                scope_badge,
                fact.id,
                score * 100.0,
                fact.content
            ));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_permanent_fact_never_decays() {
        let fact = ScopedMemoryFact::new("rule-1", MemoryScope::Permanent, "Use cargo test -j 2");
        let future_time = fact.created_at + 1_000_000_000;
        assert_eq!(fact.calculate_retention(future_time), 1.0);
        assert!(fact.is_active(future_time, 0.5));
    }

    #[test]
    fn test_transient_fact_decays_and_reinforcement_boosts_stability() {
        let mut fact = ScopedMemoryFact::new("tmp-1", MemoryScope::Transient, "Check socket 8080");
        let one_hour_later = fact.created_at + 3600;

        let ret1 = fact.calculate_retention(one_hour_later);
        // After 1 half-life with stability 1.5, retention should be around exp(-ln2 * 1 / 1.5) = 0.63
        assert!(ret1 > 0.5 && ret1 < 0.7);

        // Reinforce twice
        fact.reinforce();
        fact.reinforce();
        let ret2 = fact.calculate_retention(one_hour_later);
        assert!(ret2 > ret1);
    }
}

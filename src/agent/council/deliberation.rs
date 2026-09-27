//! Debate deliberation engine for structured multi-agent discussion

use crate::agent::council::types::{AgentPerspective, DebateTopic, MemberId};
use std::collections::HashMap;

/// A single round of debate
#[derive(Debug, Clone)]
pub struct DebateRound {
    pub round_number: u32,
    pub topic_id: String,
    pub perspectives: Vec<AgentPerspective>,
    pub is_final: bool,
}

impl DebateRound {
    pub fn new(round_number: u32, topic_id: &str) -> Self {
        Self {
            round_number,
            topic_id: topic_id.to_string(),
            perspectives: Vec::new(),
            is_final: false,
        }
    }

    pub fn add_perspective(&mut self, perspective: AgentPerspective) {
        self.perspectives.push(perspective);
    }

    pub fn mark_final(&mut self) {
        self.is_final = true;
    }
}

/// Synthesized view combining multiple perspectives
#[derive(Debug, Clone)]
pub struct SynthesizedView {
    pub topic_id: String,
    pub consensus_points: Vec<String>,
    pub disputed_points: Vec<String>,
    pub recommended_action: String,
    pub confidence: f64,
    pub reasoning: String,
}

impl SynthesizedView {
    pub fn new(topic_id: &str) -> Self {
        Self {
            topic_id: topic_id.to_string(),
            consensus_points: Vec::new(),
            disputed_points: Vec::new(),
            recommended_action: String::new(),
            confidence: 0.0,
            reasoning: String::new(),
        }
    }
}

/// Structured deliberation engine
pub struct DeliberationEngine {
    max_rounds: u32,
    convergence_threshold: f64,
}

impl Default for DeliberationEngine {
    fn default() -> Self {
        Self {
            max_rounds: 3,
            convergence_threshold: 0.7,
        }
    }
}

impl DeliberationEngine {
    pub fn new(max_rounds: u32, convergence_threshold: f64) -> Self {
        Self {
            max_rounds,
            convergence_threshold,
        }
    }

    /// Analyze consensus and divergence between perspectives
    pub fn analyze_perspectives(
        &self,
        perspectives: &[AgentPerspective],
    ) -> (Vec<String>, Vec<String>) {
        let mut all_arguments: HashMap<String, Vec<MemberId>> = HashMap::new();

        for perspective in perspectives {
            for arg in &perspective.arguments {
                all_arguments
                    .entry(arg.clone())
                    .or_default()
                    .push(perspective.member_id.clone());
            }
        }

        let total_members = perspectives.len();
        let mut consensus = Vec::new();
        let mut disputed = Vec::new();

        for (arg, supporters) in &all_arguments {
            let ratio = supporters.len() as f64 / total_members as f64;
            if ratio >= self.convergence_threshold {
                consensus.push(format!(
                    "{} (supported by {}/{} members)",
                    arg,
                    supporters.len(),
                    total_members
                ));
            } else if ratio >= 0.3 {
                disputed.push(format!(
                    "{} (supported by {}/{} members)",
                    arg,
                    supporters.len(),
                    total_members
                ));
            }
        }

        (consensus, disputed)
    }

    /// Synthesize perspectives into a coherent recommendation
    pub fn synthesize(
        &self,
        topic: &DebateTopic,
        perspectives: &[AgentPerspective],
        _prior_rounds: &[DebateRound],
    ) -> SynthesizedView {
        let mut view = SynthesizedView::new(&topic.id);

        // Analyze convergence
        let (consensus_points, disputed_points) = self.analyze_perspectives(perspectives);
        view.consensus_points = consensus_points;
        view.disputed_points = disputed_points;

        // Calculate overall confidence
        let total_confidence: f64 = perspectives.iter().map(|p| p.confidence).sum();
        view.confidence = if perspectives.is_empty() {
            0.0
        } else {
            total_confidence / perspectives.len() as f64
        };

        // Build recommendation
        let mut reasoning = String::new();

        if !view.consensus_points.is_empty() {
            reasoning.push_str("## Points of Agreement\n\n");
            for point in &view.consensus_points {
                reasoning.push_str(&format!("- {}\n", point));
            }
            reasoning.push('\n');
        }

        if !view.disputed_points.is_empty() {
            reasoning.push_str("## Disputed Points\n\n");
            for point in &view.disputed_points {
                reasoning.push_str(&format!("- {}\n", point));
            }
            reasoning.push('\n');
        }

        // Determine recommendation based on stances
        let stance_counts = self::count_stances(perspectives);
        if let Some(winner) = stance_counts
            .iter()
            .max_by_key(|(_, count)| *count)
            .filter(|(_, count)| **count > perspectives.len() / 2)
        {
            view.recommended_action = winner.0.clone();
            reasoning.push_str(&format!(
                "## Recommendation\n\nThe council recommends **{}** based on {} of {} members agreeing.\n",
                winner.0,
                winner.1,
                perspectives.len()
            ));
        }

        view.reasoning = reasoning;
        view
    }

    /// Check if deliberation has converged
    pub fn has_converged(&self, perspectives: &[AgentPerspective]) -> bool {
        if perspectives.len() < 2 {
            return true;
        }

        // Check stance convergence
        let stance_counts = count_stances(perspectives);
        let total = perspectives.len();

        for count in stance_counts.values() {
            let ratio = *count as f64 / total as f64;
            if ratio >= self.convergence_threshold {
                return true;
            }
        }

        false
    }

    /// Check if max rounds reached
    pub fn should_end(&self, round_number: u32) -> bool {
        round_number >= self.max_rounds
    }
}

fn count_stances(perspectives: &[AgentPerspective]) -> HashMap<String, usize> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for p in perspectives {
        if let Some(ref stance) = p.stance {
            *counts.entry(stance.clone()).or_insert(0) += 1;
        }
    }
    counts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_convergence_detection() {
        let engine = DeliberationEngine::default();
        let mut perspectives = vec![
            AgentPerspective::new(MemberId::new("a"), "t1").with_stance("A", 0.9),
            AgentPerspective::new(MemberId::new("b"), "t1").with_stance("A", 0.8),
            AgentPerspective::new(MemberId::new("c"), "t1").with_stance("A", 0.85),
        ];

        assert!(engine.has_converged(&perspectives));

        perspectives.push(AgentPerspective::new(MemberId::new("d"), "t1").with_stance("B", 0.7));
        perspectives.push(AgentPerspective::new(MemberId::new("e"), "t1").with_stance("B", 0.7));
        assert!(!engine.has_converged(&perspectives));
    }

    #[test]
    fn test_perspective_analysis() {
        let engine = DeliberationEngine::new(3, 0.5);
        let perspectives = vec![
            AgentPerspective::new(MemberId::new("a"), "t1")
                .with_arguments(vec!["Safe approach", "Tested pattern"]),
            AgentPerspective::new(MemberId::new("b"), "t1")
                .with_arguments(vec!["Safe approach", "Better performance"]),
        ];

        let (consensus, disputed) = engine.analyze_perspectives(&perspectives);
        assert!(consensus.iter().any(|s| s.contains("Safe approach")));
        assert!(disputed.is_empty());
    }

    #[test]
    fn test_synthesis_recommendation() {
        let engine = DeliberationEngine::default();
        let topic = DebateTopic::new("t1", "Test", "Test topic", vec!["A", "B", "C"]);
        let perspectives = vec![
            AgentPerspective::new(MemberId::new("a"), "t1").with_stance("A", 0.9),
            AgentPerspective::new(MemberId::new("b"), "t1").with_stance("A", 0.8),
            AgentPerspective::new(MemberId::new("c"), "t1").with_stance("B", 0.7),
        ];

        let view = engine.synthesize(&topic, &perspectives, &[]);
        assert_eq!(view.recommended_action, "A");
        assert!(view.confidence > 0.0);
    }
}

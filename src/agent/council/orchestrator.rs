//! Council orchestrator - coordinates multi-agent debate and voting

use crate::agent::council::deliberation::{DebateRound, DeliberationEngine, SynthesizedView};
use crate::agent::council::divergence::{DivergenceAnalyzer, DivergenceReport};
use crate::agent::council::types::{
    AgentPerspective, CouncilMember, CouncilOutcome, DebateTopic, MemberId, Vote, VoteBallot,
    VoteProtocol, VotingRound,
};
use crate::agent::council::voting::tally_votes;
use std::collections::HashMap;

/// Configuration for the council orchestration
#[derive(Debug, Clone)]
pub struct CouncilConfig {
    pub max_deliberation_rounds: u32,
    pub max_voting_rounds: u32,
    pub convergence_threshold: f64,
    pub default_protocol: VoteProtocol,
    pub allow_abstention: bool,
    pub escalation_on_stalemate: bool,
}

impl Default for CouncilConfig {
    fn default() -> Self {
        Self {
            max_deliberation_rounds: 3,
            max_voting_rounds: 2,
            convergence_threshold: 0.7,
            default_protocol: VoteProtocol::SimpleMajority,
            allow_abstention: true,
            escalation_on_stalemate: true,
        }
    }
}

impl CouncilConfig {
    pub fn strict() -> Self {
        Self {
            max_deliberation_rounds: 5,
            max_voting_rounds: 3,
            convergence_threshold: 0.8,
            default_protocol: VoteProtocol::ThreeFifthsSupermajority,
            allow_abstention: false,
            escalation_on_stalemate: true,
        }
    }

    pub fn permissive() -> Self {
        Self {
            max_deliberation_rounds: 2,
            max_voting_rounds: 1,
            convergence_threshold: 0.5,
            default_protocol: VoteProtocol::SimpleMajority,
            allow_abstention: true,
            escalation_on_stalemate: false,
        }
    }
}

/// The main council orchestrator
pub struct CouncilOrchestrator {
    config: CouncilConfig,
    deliberation_engine: DeliberationEngine,
    divergence_analyzer: DivergenceAnalyzer,
    members: HashMap<MemberId, CouncilMember>,
    debate_history: Vec<DebateRound>,
    voting_history: Vec<VotingRound>,
}

impl CouncilOrchestrator {
    pub fn new(config: CouncilConfig) -> Self {
        let max_rounds = config.max_deliberation_rounds;
        let convergence_threshold = config.convergence_threshold;
        Self {
            config,
            deliberation_engine: DeliberationEngine::new(max_rounds, convergence_threshold),
            divergence_analyzer: DivergenceAnalyzer::default(),
            members: HashMap::new(),
            debate_history: Vec::new(),
            voting_history: Vec::new(),
        }
    }

    pub fn with_members(mut self, members: Vec<CouncilMember>) -> Self {
        for m in members {
            self.members.insert(m.id.clone(), m);
        }
        self
    }

    /// Add a member to the council
    pub fn add_member(&mut self, member: CouncilMember) {
        self.members.insert(member.id.clone(), member);
    }

    /// Get all council members
    pub fn members(&self) -> Vec<&CouncilMember> {
        self.members.values().collect()
    }

    /// Get member by ID
    pub fn get_member(&self, id: &MemberId) -> Option<&CouncilMember> {
        self.members.get(id)
    }

    /// Run a complete deliberation cycle on a topic
    pub fn deliberate(&mut self, topic: &DebateTopic) -> Result<CouncilResult, CouncilError> {
        tracing::info!(
            topic_id = %topic.id,
            member_count = self.members.len(),
            "Starting council deliberation"
        );

        // Phase 1: Initial perspective gathering
        let mut current_round = DebateRound::new(1, &topic.id);

        // Phase 2: Deliberation rounds
        for round_num in 1..=self.config.max_deliberation_rounds {
            current_round.round_number = round_num;

            // Check for convergence
            if self
                .deliberation_engine
                .has_converged(&current_round.perspectives)
            {
                tracing::info!(round = round_num, "Deliberation converged early");
                break;
            }

            current_round.mark_final();
            self.debate_history.push(current_round.clone());
            current_round = DebateRound::new(round_num + 1, &topic.id);
        }

        // Phase 3: Divergence analysis
        let divergence_report = self
            .divergence_analyzer
            .analyze(&current_round.perspectives, &topic.id);

        tracing::info!(
            conflict_count = divergence_report.conflicts.len(),
            divergence_score = divergence_report.overall_divergence_score,
            "Divergence analysis complete"
        );

        // Phase 4: Voting
        let outcome = self.run_voting(topic, &current_round.perspectives, &divergence_report)?;

        // Phase 5: Final synthesis
        let synthesized = self.deliberation_engine.synthesize(
            topic,
            &current_round.perspectives,
            &self.debate_history,
        );

        tracing::info!(
            decision = %outcome.decision,
            confidence = outcome.confidence,
            rounds = outcome.rounds,
            "Council deliberation complete"
        );

        Ok(CouncilResult {
            outcome,
            divergence_report,
            synthesized_view: synthesized,
            debate_rounds: self.debate_history.len() as u32,
        })
    }

    /// Run voting process
    fn run_voting(
        &mut self,
        topic: &DebateTopic,
        perspectives: &[AgentPerspective],
        divergence: &DivergenceReport,
    ) -> Result<CouncilOutcome, CouncilError> {
        // Select protocol based on divergence
        let protocol = if divergence.is_high_conflict() {
            tracing::info!("High conflict detected - escalating to stricter protocol");
            VoteProtocol::ThreeFifthsSupermajority
        } else {
            self.config.default_protocol
        };

        let mut current_round = VotingRound::new(1, protocol);
        let mut voting_outcome = CouncilOutcome::new(&topic.id, "", protocol);

        // Collect votes from perspectives
        for perspective in perspectives {
            let member = self.members.get(&perspective.member_id);
            let weight = member.map(|m| m.weight).unwrap_or(1.0);

            let ballot = VoteBallot {
                voter_id: perspective.member_id.clone(),
                topic_id: topic.id.clone(),
                option: perspective.stance.clone().unwrap_or_default(),
                weight,
                rationale: perspective.rationale.clone(),
                confidence: perspective.confidence,
                round: 1,
            };

            let vote = if ballot.option.is_empty() && self.config.allow_abstention {
                Vote::abstain(perspective.member_id.clone(), &topic.id, 1)
            } else {
                Vote::standard(ballot)
            };

            current_round.add_vote(vote);
        }

        // Tally votes
        let result = tally_votes(&current_round, self.members.len());

        if result.passed {
            voting_outcome.decision = result.winner.clone().unwrap_or_default();
            voting_outcome.vote_tally = result.scores.clone();
            voting_outcome.confidence = result
                .winner
                .as_ref()
                .and_then(|w| result.scores.get(w))
                .copied()
                .unwrap_or(0.5);
        }

        self.voting_history.push(current_round);

        // Handle failure - attempt re-vote with different protocol
        if !result.passed && self.config.escalation_on_stalemate {
            tracing::info!("Initial vote failed, attempting escalation");
            return self.escalate_voting(topic, perspectives, &voting_outcome);
        }

        voting_outcome.rounds = 1;
        Ok(voting_outcome)
    }

    /// Escalate voting with stricter protocol or fallback
    fn escalate_voting(
        &mut self,
        topic: &DebateTopic,
        perspectives: &[AgentPerspective],
        prior_outcome: &CouncilOutcome,
    ) -> Result<CouncilOutcome, CouncilError> {
        let mut round_num = 2;
        let mut current_protocol = prior_outcome.protocol;
        let mut outcome = prior_outcome.clone();

        while round_num <= self.config.max_voting_rounds {
            // Upgrade protocol
            current_protocol = match current_protocol {
                VoteProtocol::SimpleMajority => VoteProtocol::ThreeFifthsSupermajority,
                VoteProtocol::ThreeFifthsSupermajority => VoteProtocol::TwoThirdsSupermajority,
                VoteProtocol::TwoThirdsSupermajority => VoteProtocol::Unanimous,
                _ => current_protocol,
            };

            tracing::info!(
                round = round_num,
                protocol = ?current_protocol,
                "Escalating to stricter voting protocol"
            );

            let mut round = VotingRound::new(round_num, current_protocol);

            for perspective in perspectives {
                let member = self.members.get(&perspective.member_id);
                let weight = member.map(|m| m.weight).unwrap_or(1.0);

                let ballot = VoteBallot {
                    voter_id: perspective.member_id.clone(),
                    topic_id: topic.id.clone(),
                    option: perspective.stance.clone().unwrap_or_default(),
                    weight,
                    rationale: perspective.rationale.clone(),
                    confidence: perspective.confidence,
                    round: round_num,
                };

                let vote = Vote::standard(ballot);
                round.add_vote(vote);
            }

            let result = tally_votes(&round, self.members.len());
            self.voting_history.push(round.clone());

            if result.passed {
                outcome.decision = result.winner.unwrap_or_default();
                outcome.protocol = current_protocol;
                outcome.rounds = round_num;
                outcome.confidence = result.scores.get(&outcome.decision).copied().unwrap_or(0.5);
                return Ok(outcome);
            }

            round_num += 1;
        }

        // Final fallback - pick highest scoring option even without threshold
        let last_round = self.voting_history.last();
        if let Some(round) = last_round {
            let mut tally = HashMap::new();
            for vote in &round.votes {
                if !vote.is_abstain {
                    *tally.entry(vote.ballot.option.clone()).or_insert(0.0) += 1.0;
                }
            }
            if let Some((winner, _)) = tally.iter().max_by_key(|(_, v)| **v as i64) {
                tracing::warn!("Council stalemate - using fallback winner: {}", winner);
                outcome.decision = winner.clone();
                outcome.summary = "Stalemate resolved via fallback".to_string();
                return Ok(outcome);
            }
        }

        Err(CouncilError::Stalemate {
            topic: topic.id.clone(),
            rounds_attempted: round_num,
        })
    }

    /// Get debate history
    pub fn debate_history(&self) -> &[DebateRound] {
        &self.debate_history
    }

    /// Get voting history
    pub fn voting_history(&self) -> &[VotingRound] {
        &self.voting_history
    }
}

/// Result of a complete council deliberation
#[derive(Debug)]
pub struct CouncilResult {
    pub outcome: CouncilOutcome,
    pub divergence_report: DivergenceReport,
    pub synthesized_view: SynthesizedView,
    pub debate_rounds: u32,
}

impl CouncilResult {
    /// Format the result as a markdown report
    pub fn format_report(&self) -> String {
        let mut report = String::new();

        report.push_str("# Council Decision Report\n\n");
        report.push_str(&format!("**Topic**: {}\n\n", self.outcome.topic_id));

        // Decision
        report.push_str("## Decision\n\n");
        report.push_str(&format!(
            "**Outcome**: {} (confidence: {:.0}%)\n",
            self.outcome.decision,
            self.outcome.confidence * 100.0
        ));
        report.push_str(&format!(
            "**Protocol**: {}\n",
            self.outcome.protocol.description()
        ));
        report.push_str(&format!("**Rounds**: {}\n\n", self.outcome.rounds));

        // Divergence
        report.push_str("## Divergence Analysis\n\n");
        report.push_str(&format!(
            "**Score**: {:.0}% ({})\n\n",
            self.divergence_report.overall_divergence_score * 100.0,
            if self.divergence_report.can_proceed {
                "can proceed"
            } else {
                "high conflict"
            }
        ));

        if !self.divergence_report.conflicts.is_empty() {
            report.push_str("**Conflicts Detected**:\n");
            for conflict in &self.divergence_report.conflicts {
                report.push_str(&format!(
                    "- [{}] {}\n",
                    conflict.kind.label(),
                    conflict.description
                ));
            }
            report.push('\n');
        }

        // Synthesis
        report.push_str("## Synthesis\n\n");
        if !self.synthesized_view.consensus_points.is_empty() {
            report.push_str("**Consensus Points**:\n");
            for point in &self.synthesized_view.consensus_points {
                report.push_str(&format!("- {}\n", point));
            }
            report.push('\n');
        }

        if !self.synthesized_view.disputed_points.is_empty() {
            report.push_str("**Disputed Points**:\n");
            for point in &self.synthesized_view.disputed_points {
                report.push_str(&format!("- {}\n", point));
            }
            report.push('\n');
        }

        report.push_str(&self.synthesized_view.reasoning);

        report
    }
}

/// Errors that can occur during council orchestration
#[derive(Debug, thiserror::Error)]
pub enum CouncilError {
    #[error("Stalemate reached after {rounds_attempted} rounds for topic {topic}")]
    Stalemate {
        topic: String,
        rounds_attempted: u32,
    },

    #[error("Not enough council members: {have} < {need}")]
    InsufficientMembers { have: usize, need: usize },

    #[error("Member not found: {0}")]
    MemberNotFound(MemberId),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::council::types::MemberRole;

    fn create_test_council() -> CouncilOrchestrator {
        let members = vec![
            CouncilMember::new("alice", MemberRole::Generalist, "Alice"),
            CouncilMember::new("bob", MemberRole::SecuritySpecialist, "Bob"),
            CouncilMember::new("carol", MemberRole::ArchitectureSpecialist, "Carol"),
        ];

        CouncilOrchestrator::new(CouncilConfig::permissive()).with_members(members)
    }

    #[test]
    fn test_council_members() {
        let council = create_test_council();
        assert_eq!(council.members().len(), 3);

        let alice = council.get_member(&MemberId::new("alice"));
        assert!(alice.is_some());
        assert_eq!(alice.unwrap().name, "Alice");
    }

    #[test]
    fn test_council_config_presets() {
        let strict = CouncilConfig::strict();
        assert_eq!(
            strict.default_protocol,
            VoteProtocol::ThreeFifthsSupermajority
        );
        assert_eq!(strict.max_deliberation_rounds, 5);

        let permissive = CouncilConfig::permissive();
        assert_eq!(permissive.default_protocol, VoteProtocol::SimpleMajority);
        assert_eq!(permissive.max_deliberation_rounds, 2);
    }
}

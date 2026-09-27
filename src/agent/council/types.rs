//! Core types for the multi-agent council system

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Unique identifier for a council member (agent)
#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemberId(pub String);

impl MemberId {
    pub fn new(id: &str) -> Self {
        Self(id.to_lowercase())
    }
}

impl std::fmt::Display for MemberId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Role or expertise domain of a council member
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemberRole {
    /// General-purpose reasoning agent
    Generalist,
    /// Security and safety specialist
    SecuritySpecialist,
    /// Performance and optimization specialist
    PerformanceSpecialist,
    /// Code quality and architecture specialist
    ArchitectureSpecialist,
    /// Testing and verification specialist
    TestingSpecialist,
    /// Documentation and clarity specialist
    DocumentationSpecialist,
    /// Operations and DevOps specialist
    DevOpsSpecialist,
}

impl MemberRole {
    pub fn label(&self) -> &'static str {
        match self {
            MemberRole::Generalist => "Generalist",
            MemberRole::SecuritySpecialist => "Security Specialist",
            MemberRole::PerformanceSpecialist => "Performance Specialist",
            MemberRole::ArchitectureSpecialist => "Architecture Specialist",
            MemberRole::TestingSpecialist => "Testing Specialist",
            MemberRole::DocumentationSpecialist => "Documentation Specialist",
            MemberRole::DevOpsSpecialist => "DevOps Specialist",
        }
    }
}

/// A member of the agent council
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CouncilMember {
    pub id: MemberId,
    pub role: MemberRole,
    pub name: String,
    pub expertise: Vec<String>,
    pub weight: f64,
}

impl CouncilMember {
    pub fn new(id: &str, role: MemberRole, name: &str) -> Self {
        Self {
            id: MemberId::new(id),
            role,
            name: name.to_string(),
            expertise: Vec::new(),
            weight: 1.0,
        }
    }

    pub fn with_expertise(mut self, expertise: Vec<&str>) -> Self {
        self.expertise = expertise.into_iter().map(|s| s.to_string()).collect();
        self
    }

    pub fn with_weight(mut self, weight: f64) -> Self {
        self.weight = weight.clamp(0.0, 10.0);
        self
    }
}

/// Topic under deliberation in the council
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebateTopic {
    pub id: String,
    pub title: String,
    pub description: String,
    pub options: Vec<String>,
    pub criteria: Vec<String>,
    pub context: String,
}

impl DebateTopic {
    pub fn new(id: &str, title: &str, description: &str, options: Vec<&str>) -> Self {
        Self {
            id: id.to_string(),
            title: title.to_string(),
            description: description.to_string(),
            options: options.into_iter().map(|s| s.to_string()).collect(),
            criteria: Vec::new(),
            context: String::new(),
        }
    }

    pub fn with_criteria(mut self, criteria: Vec<&str>) -> Self {
        self.criteria = criteria.into_iter().map(|s| s.to_string()).collect();
        self
    }

    pub fn with_context(mut self, context: &str) -> Self {
        self.context = context.to_string();
        self
    }
}

/// An agent's perspective on a debate topic
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentPerspective {
    pub member_id: MemberId,
    pub topic_id: String,
    pub stance: Option<String>,
    pub confidence: f64,
    pub arguments: Vec<String>,
    pub concerns: Vec<String>,
    pub supporting_evidence: Vec<String>,
    pub counter_arguments: Vec<String>,
    pub rationale: String,
}

impl AgentPerspective {
    pub fn new(member_id: MemberId, topic_id: &str) -> Self {
        Self {
            member_id,
            topic_id: topic_id.to_string(),
            stance: None,
            confidence: 0.5,
            arguments: Vec::new(),
            concerns: Vec::new(),
            supporting_evidence: Vec::new(),
            counter_arguments: Vec::new(),
            rationale: String::new(),
        }
    }

    pub fn with_stance(mut self, stance: &str, confidence: f64) -> Self {
        self.stance = Some(stance.to_string());
        self.confidence = confidence.clamp(0.0, 1.0);
        self
    }

    pub fn with_confidence(mut self, confidence: f64) -> Self {
        self.confidence = confidence.clamp(0.0, 1.0);
        self
    }

    pub fn with_arguments(mut self, args: Vec<&str>) -> Self {
        self.arguments = args.into_iter().map(|s| s.to_string()).collect();
        self
    }

    pub fn with_concerns(mut self, concerns: Vec<&str>) -> Self {
        self.concerns = concerns.into_iter().map(|s| s.to_string()).collect();
        self
    }
}

/// A single vote ballot from a council member
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoteBallot {
    pub voter_id: MemberId,
    pub topic_id: String,
    pub option: String,
    pub weight: f64,
    pub rationale: String,
    pub confidence: f64,
    pub round: u32,
}

impl VoteBallot {
    pub fn new(voter_id: MemberId, topic_id: &str, option: &str, round: u32) -> Self {
        Self {
            voter_id,
            topic_id: topic_id.to_string(),
            option: option.to_string(),
            weight: 1.0,
            rationale: String::new(),
            confidence: 0.5,
            round,
        }
    }

    pub fn with_weight(mut self, weight: f64) -> Self {
        self.weight = weight.max(0.0);
        self
    }

    pub fn with_confidence(mut self, confidence: f64) -> Self {
        self.confidence = confidence.clamp(0.0, 1.0);
        self
    }
}

/// A vote cast by a council member
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vote {
    pub ballot: VoteBallot,
    pub is_abstain: bool,
    pub is_qualified: bool,
}

impl Vote {
    pub fn standard(ballot: VoteBallot) -> Self {
        Self {
            ballot,
            is_abstain: false,
            is_qualified: true,
        }
    }

    pub fn abstain(voter_id: MemberId, topic_id: &str, round: u32) -> Self {
        Self {
            ballot: VoteBallot {
                voter_id,
                topic_id: topic_id.to_string(),
                option: String::new(),
                weight: 0.0,
                rationale: String::new(),
                confidence: 0.0,
                round,
            },
            is_abstain: true,
            is_qualified: true,
        }
    }
}

/// Voting protocol types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VoteProtocol {
    /// All agents must agree (highest bar)
    Unanimous,
    /// 2/3 (66.7%) supermajority required
    TwoThirdsSupermajority,
    /// 3/5 (60%) supermajority required
    ThreeFifthsSupermajority,
    /// Simple >50% majority
    SimpleMajority,
    /// Ranked preference voting with Borda count
    BordaCount,
    /// Quorum threshold + majority
    QuorumMajority,
}

impl VoteProtocol {
    /// Minimum vote fraction required for approval
    pub fn threshold(&self) -> f64 {
        match self {
            VoteProtocol::Unanimous => 1.0,
            VoteProtocol::TwoThirdsSupermajority => 2.0 / 3.0,
            VoteProtocol::ThreeFifthsSupermajority => 3.0 / 5.0,
            VoteProtocol::SimpleMajority => 0.5,
            VoteProtocol::BordaCount => 0.0, // Special handling
            VoteProtocol::QuorumMajority => 0.5,
        }
    }

    /// Human-readable description
    pub fn description(&self) -> &'static str {
        match self {
            VoteProtocol::Unanimous => "All members must agree",
            VoteProtocol::TwoThirdsSupermajority => "2/3 supermajority required",
            VoteProtocol::ThreeFifthsSupermajority => "3/5 supermajority required",
            VoteProtocol::SimpleMajority => ">50% majority",
            VoteProtocol::BordaCount => "Borda count (ranked preferences)",
            VoteProtocol::QuorumMajority => "Quorum + majority required",
        }
    }
}

/// A round of voting in the deliberation process
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VotingRound {
    pub round_number: u32,
    pub protocol: VoteProtocol,
    pub votes: Vec<Vote>,
    pub results: Option<super::voting::VotingResult>,
    pub quorum_met: bool,
    pub timestamp: std::time::SystemTime,
}

impl VotingRound {
    pub fn new(round_number: u32, protocol: VoteProtocol) -> Self {
        Self {
            round_number,
            protocol,
            votes: Vec::new(),
            results: None,
            quorum_met: false,
            timestamp: std::time::SystemTime::now(),
        }
    }

    pub fn add_vote(&mut self, vote: Vote) {
        self.votes.push(vote);
    }

    pub fn participant_count(&self) -> usize {
        self.votes.iter().filter(|v| !v.is_abstain).count()
    }

    pub fn total_votes(&self) -> usize {
        self.votes.len()
    }
}

/// Outcome of the council deliberation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CouncilOutcome {
    pub topic_id: String,
    pub decision: String,
    pub confidence: f64,
    pub protocol: VoteProtocol,
    pub vote_tally: HashMap<String, f64>,
    pub dissenting_members: Vec<MemberId>,
    pub summary: String,
    pub rounds: u32,
}

impl CouncilOutcome {
    pub fn new(topic_id: &str, decision: &str, protocol: VoteProtocol) -> Self {
        Self {
            topic_id: topic_id.to_string(),
            decision: decision.to_string(),
            confidence: 0.5,
            protocol,
            vote_tally: HashMap::new(),
            dissenting_members: Vec::new(),
            summary: String::new(),
            rounds: 0,
        }
    }

    pub fn with_tally(mut self, tally: HashMap<String, f64>) -> Self {
        self.vote_tally = tally;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_member_id_case_normalization() {
        let id1 = MemberId::new("TestAgent");
        let id2 = MemberId::new("testagent");
        assert_eq!(id1, id2);
    }

    #[test]
    fn test_vote_protocol_thresholds() {
        assert_eq!(VoteProtocol::Unanimous.threshold(), 1.0);
        assert!((VoteProtocol::TwoThirdsSupermajority.threshold() - 0.667).abs() < 0.01);
        assert!((VoteProtocol::SimpleMajority.threshold() - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_voting_round_counts() {
        let mut round = VotingRound::new(1, VoteProtocol::SimpleMajority);
        let m1 = MemberId::new("alice");
        let m2 = MemberId::new("bob");

        round.add_vote(Vote::standard(VoteBallot::new(m1, "t1", "Option A", 1)));
        round.add_vote(Vote::abstain(m2, "t1", 1));

        assert_eq!(round.total_votes(), 2);
        assert_eq!(round.participant_count(), 1);
    }

    #[test]
    fn test_confidence_clamping() {
        let perspective =
            AgentPerspective::new(MemberId::new("test"), "topic").with_stance("option", 1.5);
        assert_eq!(perspective.confidence, 1.0);

        let perspective = perspective.with_stance("option", -0.5);
        assert_eq!(perspective.confidence, 0.0);
    }
}

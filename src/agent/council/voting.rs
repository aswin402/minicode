//! Voting protocols and vote tallying logic

use crate::agent::council::types::{MemberId, VoteProtocol, VotingRound};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Result of a vote tally operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VotingResult {
    pub winner: Option<String>,
    pub scores: HashMap<String, f64>,
    pub vote_counts: HashMap<String, usize>,
    pub weighted_scores: HashMap<String, f64>,
    pub passed: bool,
    pub threshold: f64,
    pub protocol: VoteProtocol,
    pub details: String,
}

impl VotingResult {
    pub fn new(protocol: VoteProtocol) -> Self {
        Self {
            winner: None,
            scores: HashMap::new(),
            vote_counts: HashMap::new(),
            weighted_scores: HashMap::new(),
            passed: false,
            threshold: protocol.threshold(),
            protocol,
            details: String::new(),
        }
    }

    pub fn with_winner(mut self, winner: Option<String>) -> Self {
        self.winner = winner;
        self
    }
}

/// Tallies votes according to the specified protocol
pub fn tally_votes(round: &VotingRound, total_members: usize) -> VotingResult {
    let mut result = VotingResult::new(round.protocol);

    let votes: Vec<_> = round.votes.iter().filter(|v| !v.is_abstain).collect();
    let participant_count = votes.len();

    // Initialize tally maps
    let options: std::collections::HashSet<_> = votes
        .iter()
        .filter_map(|v| {
            if v.is_abstain {
                None
            } else {
                Some(v.ballot.option.as_str())
            }
        })
        .collect();

    for option in &options {
        result.scores.insert((*option).to_string(), 0.0);
        result.vote_counts.insert((*option).to_string(), 0);
        result.weighted_scores.insert((*option).to_string(), 0.0);
    }

    // Accumulate votes
    for vote in &votes {
        let option = &vote.ballot.option;
        let weight = vote.ballot.weight;
        let confidence = vote.ballot.confidence;

        // Raw vote count
        *result.vote_counts.entry(option.clone()).or_insert(0) += 1;

        // Weighted score (weight * confidence)
        let score = weight * confidence;
        *result.scores.entry(option.clone()).or_insert(0.0) += score;

        // Weighted score contribution
        *result.weighted_scores.entry(option.clone()).or_insert(0.0) += score * weight;
    }

    let n = participant_count as f64;
    let threshold = round.protocol.threshold();

    match round.protocol {
        VoteProtocol::Unanimous => {
            // All must vote for same option
            if let Some(first) = votes.first() {
                let all_same = votes.iter().all(|v| v.ballot.option == first.ballot.option);
                if all_same && participant_count == total_members {
                    result.passed = true;
                    result.winner = Some(first.ballot.option.clone());
                    result.details = "Unanimous consent achieved".to_string();
                } else {
                    result.passed = false;
                    result.details = format!(
                        "No unanimous agreement: {} of {} agreed",
                        participant_count, total_members
                    );
                }
            }
        }
        VoteProtocol::TwoThirdsSupermajority | VoteProtocol::ThreeFifthsSupermajority => {
            // Find winner by simple majority, then check threshold
            if let Some(winner) = result.vote_counts.iter().max_by_key(|(_, c)| *c) {
                let fraction = *winner.1 as f64 / n.max(1.0);
                if fraction >= threshold {
                    result.passed = true;
                    result.winner = Some(winner.0.clone());
                    result.details = format!(
                        "{:.1}% approved (threshold: {:.1}%)",
                        fraction * 100.0,
                        threshold * 100.0
                    );
                } else {
                    result.passed = false;
                    result.details = format!(
                        "{:.1}% falls short of {:.1}% threshold",
                        fraction * 100.0,
                        threshold * 100.0
                    );
                }
            }
        }
        VoteProtocol::SimpleMajority => {
            // >50% wins
            if let Some(winner) = result.vote_counts.iter().max_by_key(|(_, c)| *c) {
                let fraction = *winner.1 as f64 / n.max(1.0);
                if fraction > 0.5 {
                    result.passed = true;
                    result.winner = Some(winner.0.clone());
                    result.details = format!("{:.1}% majority achieved", fraction * 100.0);
                } else if fraction == 0.5 && n > 1.0 {
                    // Tie case - no clear winner
                    result.passed = false;
                    result.details = "Tie result - no clear majority".to_string();
                } else {
                    result.passed = false;
                    result.details = format!(
                        "Leading option has only {:.1}% (needs >50%)",
                        fraction * 100.0
                    );
                }
            }
        }
        VoteProtocol::BordaCount => {
            // Winner is highest weighted score
            if !result.scores.is_empty() {
                if let Some((winner, score)) = result
                    .scores
                    .iter()
                    .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
                {
                    result.passed = true;
                    result.winner = Some(winner.clone());
                    result.details = format!("Borda winner '{}' with score {:.2}", winner, score);
                }
            } else {
                result.passed = false;
                result.details = "No votes cast for Borda count".to_string();
            }
        }
        VoteProtocol::QuorumMajority => {
            // Quorum = 60% participation + majority
            let quorum_threshold = 0.6;
            if (n / total_members as f64) < quorum_threshold {
                result.passed = false;
                result.details = format!(
                    "Quorum not met: {:.1}% < {:.1}% required",
                    (n / total_members as f64) * 100.0,
                    quorum_threshold * 100.0
                );
            } else if let Some(winner) = result.vote_counts.iter().max_by_key(|(_, c)| *c) {
                let fraction = *winner.1 as f64 / n.max(1.0);
                if fraction > 0.5 {
                    result.passed = true;
                    result.winner = Some(winner.0.clone());
                    result.details = format!(
                        "Quorum met ({:.1}%), {:.1}% majority for '{}'",
                        (n / total_members as f64) * 100.0,
                        fraction * 100.0,
                        winner.0
                    );
                } else {
                    result.passed = false;
                    result.details = "Quorum met but no majority".to_string();
                }
            }
        }
    }

    result
}

/// Compute Borda scores for ranked preference voting
/// Points: 1st choice gets (n-1), 2nd gets (n-2), etc.
pub fn compute_borda_scores(rankings: &HashMap<MemberId, Vec<String>>) -> HashMap<String, f64> {
    let n = rankings.values().next().map(|r| r.len()).unwrap_or(0);
    if n == 0 {
        return HashMap::new();
    }

    let mut scores: HashMap<String, f64> = HashMap::new();

    for ranking in rankings.values() {
        for (position, option) in ranking.iter().enumerate() {
            let points = (n - position - 1) as f64;
            *scores.entry(option.clone()).or_insert(0.0) += points;
        }
    }

    scores
}

/// Determine if a decision has reached consensus
#[allow(dead_code)]
pub fn has_consensus(result: &VotingResult, dissenters: &[MemberId], threshold: f64) -> bool {
    if !result.passed {
        return false;
    }

    // Check if dissent is within acceptable threshold
    let dissent_ratio = dissenters.len() as f64 / (dissenters.len() as f64 + 1.0);
    dissent_ratio <= (1.0 - threshold)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::council::types::{Vote, Vote as CouncilVote, VoteBallot};

    fn create_test_votes(options: Vec<&str>, weights: Vec<f64>) -> VotingRound {
        let mut round = VotingRound::new(1, VoteProtocol::SimpleMajority);
        let members = ["alice", "bob", "charlie", "diana"];

        for (i, option) in options.iter().enumerate() {
            let voter = MemberId::new(members[i % members.len()]);
            let ballot = VoteBallot::new(voter, "test_topic", option, 1)
                .with_weight(weights.get(i).copied().unwrap_or(1.0))
                .with_confidence(0.9);
            round.add_vote(CouncilVote::standard(ballot));
        }

        round
    }

    #[test]
    fn test_simple_majority_win() {
        let round = create_test_votes(vec!["A", "A", "B"], vec![]);
        let result = tally_votes(&round, 3);
        assert!(result.passed);
        assert_eq!(result.winner, Some("A".to_string()));
    }

    #[test]
    fn test_no_majority() {
        let round = create_test_votes(vec!["A", "B", "C"], vec![]);
        let result = tally_votes(&round, 3);
        assert!(!result.passed);
    }

    #[test]
    fn test_borda_count() {
        let mut rankings = HashMap::new();
        rankings.insert(
            MemberId::new("voter1"),
            vec!["A".to_string(), "B".to_string(), "C".to_string()],
        );
        rankings.insert(
            MemberId::new("voter2"),
            vec!["B".to_string(), "A".to_string(), "C".to_string()],
        );
        rankings.insert(
            MemberId::new("voter3"),
            vec!["A".to_string(), "C".to_string(), "B".to_string()],
        );

        let scores = compute_borda_scores(&rankings);
        assert!(*scores.get("A").unwrap() > *scores.get("B").unwrap());
        assert!(*scores.get("B").unwrap() > *scores.get("C").unwrap());
    }

    #[test]
    fn test_supermajority_threshold() {
        let mut round = VotingRound::new(1, VoteProtocol::TwoThirdsSupermajority);
        let members = ["a", "b", "c"];

        for (i, opt) in ["A", "A", "B"].iter().enumerate() {
            let voter = MemberId::new(members[i]);
            let ballot = VoteBallot::new(voter, "t", opt, 1);
            round.add_vote(CouncilVote::standard(ballot));
        }

        let result = tally_votes(&round, 3);
        // 2/3 = 66.7%, we have 2/3 = 66.7%
        assert!(result.passed, "2/3 should pass: {}", result.details);
    }
}

//! Multi-Perspective Voting Protocols, Debate Deliberation & Divergence Resolution
//!
//! This module implements a structured multi-agent council system for collective
//! decision-making. Agents contribute perspectives, debate arguments, and resolve
//! divergent viewpoints through voting protocols.
//!
//! ## Architecture
//!
//! - **Council**: The coordinating orchestrator that manages the debate lifecycle
//! - **Perspective**: An agent's viewpoint, arguments, and confidence on a topic
//! - **Vote**: A structured ballot with rationale and confidence weighting
//! - **Deliberation**: The process of synthesizing perspectives into a final recommendation
//! - **Divergence**: Detected conflict between agent viewpoints requiring resolution
//!
//! ## Voting Protocols
//!
//! - **Unanimous**: All agents must agree (highest bar)
//! - **Supermajority**: 2/3 or 3/5 majority
//! - **Simple Majority**: >50% of votes
//! - **Borda Count**: Ranked preference voting with point assignment
//! - **Quorum**: Minimum participation threshold + majority

mod deliberation;
mod divergence;
mod orchestrator;
mod types;
mod voting;

pub use deliberation::{DebateRound, DeliberationEngine, SynthesizedView};
pub use divergence::{ConflictKind, DetectedConflict, DivergenceAnalyzer, DivergenceReport};
pub use orchestrator::{CouncilConfig, CouncilOrchestrator, CouncilResult};
pub use types::{
    AgentPerspective, CouncilMember, CouncilOutcome, DebateTopic, MemberId, MemberRole, Vote,
    VoteBallot, VoteProtocol, VotingRound,
};
pub use voting::{compute_borda_scores, tally_votes, VotingResult};

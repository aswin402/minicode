//! Native Autonomous Multi-Agent Swarm Orchestrator.
//!
//! Provides dynamic task decomposition, dependency-aware DAG scheduling,
//! isolated git worktree execution, artifact passing, and map-reduce merge arbitration.

pub mod arbitrator;
pub mod bus;
pub mod models;
pub mod planner;
pub mod report;
pub mod scheduler;

#[allow(unused_imports)]
pub use arbitrator::SwarmArbitrator;
#[allow(unused_imports)]
pub use bus::{SwarmMessage, SwarmMessageBus, SwarmMessageIntent};
#[allow(unused_imports)]
pub use models::{
    SwarmError, SwarmExecutionState, SwarmPlan, SwarmTaskOutcome, SwarmTaskSpec, SwarmTaskStatus,
};
pub use planner::SwarmPlanner;
#[allow(unused_imports)]
pub use report::SwarmReporter;
pub use scheduler::{SwarmRunOptions, SwarmScheduler};

//! MiniDev runtime orchestration engine for managing long-running dev servers,
//! backend processes, Docker stacks, Chrome instances, and background subagents
//! with zero-orphan process guarantees and resource monitoring.

pub mod lifecycle;
pub mod metrics;
pub mod models;
pub mod ports;
pub mod process;
pub mod registry;

pub use lifecycle::*;
pub use metrics::*;
pub use models::*;
pub use ports::*;
pub use process::*;
pub use registry::*;

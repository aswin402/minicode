//! Domain models and types for the MiniDev runtime orchestrator.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;
use uuid::Uuid;

/// Unique identifier for a managed dev process, server, container, or browser.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DevProcessId(String);

impl DevProcessId {
    /// Generates a new random DevProcessId with an optional prefix.
    pub fn new(prefix: Option<&str>) -> Self {
        let uuid = Uuid::new_v4();
        let short_uuid = &uuid.to_string()[..8];
        match prefix {
            Some(p) if !p.trim().is_empty() => {
                let clean_p = p.trim().to_lowercase().replace(' ', "-");
                Self(format!("dev-{}-{}", clean_p, short_uuid))
            }
            _ => Self(format!("dev-{}", short_uuid)),
        }
    }

    /// Creates a DevProcessId from an existing string.
    pub fn from_string(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for DevProcessId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl From<String> for DevProcessId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl fmt::Display for DevProcessId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Category of running process managed by mini_dev.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DevProcessType {
    /// Frontend development server (e.g. Vite, Next.js, SvelteKit, Webpack)
    Frontend,
    /// Backend API or application server (e.g. Axum, Node/Express, FastAPI, Go, Django)
    Backend,
    /// Persistent script or build watcher (e.g. cargo watch, tsc --watch, python script)
    Script,
    /// Managed Docker container or compose stack
    Docker,
    /// Managed Chrome browser session (headless or headful for visual inspection)
    Chrome,
    /// Autonomous background subagent or delegated task worker
    Worker,
}

impl DevProcessType {
    pub fn from_str_loose(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "frontend" | "web" | "ui" | "client" | "vite" | "next" => Self::Frontend,
            "backend" | "api" | "server" | "express" | "axum" | "fastapi" => Self::Backend,
            "docker" | "container" | "compose" => Self::Docker,
            "chrome" | "browser" | "headless" | "headful" => Self::Chrome,
            "worker" | "subagent" | "task" => Self::Worker,
            _ => Self::Script,
        }
    }
}

impl std::str::FromStr for DevProcessType {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Ok(Self::from_str_loose(s))
    }
}

impl fmt::Display for DevProcessType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Frontend => write!(f, "Frontend"),
            Self::Backend => write!(f, "Backend"),
            Self::Script => write!(f, "Script"),
            Self::Docker => write!(f, "Docker"),
            Self::Chrome => write!(f, "Chrome"),
            Self::Worker => write!(f, "Worker"),
        }
    }
}

/// Current lifecycle status of a managed process.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", content = "details")]
pub enum DevProcessStatus {
    /// Process has been spawned and is actively running
    Running,
    /// Process has confirmed listening on a network port or passed health check
    Healthy,
    /// Process is alive but has logged error spikes or missed health checks
    Degraded(String),
    /// Process exited with exit code
    Exited(Option<i32>),
    /// Process was gracefully stopped
    Stopped,
    /// Process was forcefully killed
    Killed,
}

impl DevProcessStatus {
    pub fn is_alive(&self) -> bool {
        matches!(self, Self::Running | Self::Healthy | Self::Degraded(_))
    }
}

impl fmt::Display for DevProcessStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Running => write!(f, "Running"),
            Self::Healthy => write!(f, "Healthy"),
            Self::Degraded(reason) => write!(f, "Degraded ({})", reason),
            Self::Exited(Some(code)) => write!(f, "Exited (code {})", code),
            Self::Exited(None) => write!(f, "Exited"),
            Self::Stopped => write!(f, "Stopped"),
            Self::Killed => write!(f, "Killed"),
        }
    }
}

/// Resource telemetry for a process or aggregate tree.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DevProcessResourceUsage {
    /// Operating system process ID
    pub pid: Option<u32>,
    /// CPU usage percentage (e.g. 4.5%)
    pub cpu_percent: f32,
    /// Resident Set Size memory consumption in Megabytes
    pub memory_rss_mb: f32,
    /// Open TCP listening ports discovered for this process tree
    pub open_ports: Vec<u16>,
}

impl Default for DevProcessResourceUsage {
    fn default() -> Self {
        Self {
            pid: None,
            cpu_percent: 0.0,
            memory_rss_mb: 0.0,
            open_ports: Vec::new(),
        }
    }
}

/// Request to spawn a new managed development process.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpawnDevRequest {
    /// Shell command or script to execute
    pub command: String,
    /// Human-readable label or service name (e.g. "vite-app", "axum-backend")
    pub name: Option<String>,
    /// Category of process
    pub process_type: DevProcessType,
    /// Working directory relative to workspace root (defaults to workspace root)
    pub working_dir: Option<PathBuf>,
    /// Custom environment variables to inject
    pub extra_env: HashMap<String, String>,
    /// Expected network port if known
    pub port_hint: Option<u16>,
    /// Optional virtual memory ceiling in MB
    pub max_memory_mb: Option<u64>,
    /// Port collision handling strategy (default: Fallback)
    pub port_policy: Option<PortConflictPolicy>,
    /// Auto-restart watchdog policy (default: Never)
    pub restart_policy: Option<RestartPolicy>,
}

/// Policy for handling port collisions when spawning dev processes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PortConflictPolicy {
    /// Automatically find the next available port and inject PORT=... into environment
    #[default]
    Fallback,
    /// Abort with PortConflict error if target port is occupied
    Error,
    /// Terminate conflicting process if possible (SIGTERM/SIGKILL)
    Kill,
    /// Ignore the conflict and proceed with spawn
    Ignore,
}

impl PortConflictPolicy {
    pub fn from_str_loose(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "error" | "strict" | "fail" => Self::Error,
            "kill" | "force" | "override" => Self::Kill,
            "ignore" | "skip" => Self::Ignore,
            _ => Self::Fallback,
        }
    }
}

/// Details of a port conflict detected during socket probing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortConflict {
    pub port: u16,
    pub conflicting_pid: Option<u32>,
    pub process_name: Option<String>,
    pub command_line: Option<String>,
    pub suggested_fallback: Option<u16>,
}

/// Result of arbitrating a port conflict.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PortResolution {
    /// Port was available and assigned without conflict
    Unchanged { port: u16 },
    /// Port conflict detected and resolved to a fallback port
    Shifted {
        requested: u16,
        resolved: u16,
        conflict: PortConflict,
    },
    /// Conflicting process was killed and original port claimed
    Reclaimed { port: u16, killed_pid: Option<u32> },
    /// Conflict was ignored as requested
    Ignored { port: u16 },
}

impl PortResolution {
    pub fn resolved_port(&self) -> u16 {
        match self {
            Self::Unchanged { port } => *port,
            Self::Shifted { resolved, .. } => *resolved,
            Self::Reclaimed { port, .. } => *port,
            Self::Ignored { port } => *port,
        }
    }
}

/// Restart policy for managed processes supervised by the auto-restart watchdog.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "policy", rename_all = "snake_case")]
pub enum RestartPolicy {
    /// Never restart automatically
    #[default]
    Never,
    /// Automatically restart if process exits with non-zero exit code
    OnFailure { max_retries: u32, backoff_ms: u64 },
    /// Always restart when process exits, unless explicitly stopped
    Always { max_retries: u32, backoff_ms: u64 },
}

impl RestartPolicy {
    pub fn on_failure_default() -> Self {
        Self::OnFailure {
            max_retries: 3,
            backoff_ms: 1000,
        }
    }

    pub fn should_restart(&self, exit_code: Option<i32>) -> bool {
        match self {
            Self::Never => false,
            Self::OnFailure { .. } => !matches!(exit_code, Some(0)),
            Self::Always { .. } => true,
        }
    }

    pub fn max_retries(&self) -> u32 {
        match self {
            Self::Never => 0,
            Self::OnFailure { max_retries, .. } | Self::Always { max_retries, .. } => *max_retries,
        }
    }

    pub fn backoff_ms(&self) -> u64 {
        match self {
            Self::Never => 0,
            Self::OnFailure { backoff_ms, .. } | Self::Always { backoff_ms, .. } => *backoff_ms,
        }
    }
}

/// Dynamic metrics tracked by the auto-restart watchdog supervisor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct RestartStats {
    pub restart_count: u32,
    pub consecutive_fast_crashes: u32,
    pub last_restart_at_secs: Option<u64>,
    pub last_crash_reason: Option<String>,
}

/// High-level summary of a managed process for tables and status views.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DevProcessSummary {
    pub id: DevProcessId,
    pub name: String,
    pub process_type: DevProcessType,
    pub status: DevProcessStatus,
    pub pid: Option<u32>,
    pub ports: Vec<u16>,
    pub url: Option<String>,
    pub cpu_percent: f32,
    pub memory_rss_mb: f32,
    pub uptime_secs: u64,
    pub restart_count: u32,
    pub restart_policy: RestartPolicy,
    pub port_resolution: Option<PortResolution>,
}

/// Cumulative resource metrics for all processes managed by mini_dev.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuntimeResourceSummary {
    /// Total count of active, running processes
    pub total_active_processes: usize,
    /// Cumulative CPU utilization percentage across all managed processes
    pub total_cpu_percent: f32,
    /// Cumulative RSS memory consumption in MB
    pub total_memory_rss_mb: f32,
    /// All active listening ports
    pub active_ports: Vec<u16>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dev_process_id_generation() {
        let id1 = DevProcessId::new(Some("vite"));
        assert!(id1.as_str().starts_with("dev-vite-"));

        let id2 = DevProcessId::new(None);
        assert!(id2.as_str().starts_with("dev-"));
    }

    #[test]
    fn test_dev_process_type_loose_parsing() {
        assert_eq!(
            DevProcessType::from_str_loose("vite"),
            DevProcessType::Frontend
        );
        assert_eq!(
            DevProcessType::from_str_loose("express"),
            DevProcessType::Backend
        );
        assert_eq!(
            DevProcessType::from_str_loose("docker"),
            DevProcessType::Docker
        );
        assert_eq!(
            DevProcessType::from_str_loose("headless"),
            DevProcessType::Chrome
        );
        assert_eq!(
            DevProcessType::from_str_loose("worker"),
            DevProcessType::Worker
        );
        assert_eq!(
            DevProcessType::from_str_loose("anything_else"),
            DevProcessType::Script
        );
    }

    #[test]
    fn test_dev_process_status_alive() {
        assert!(DevProcessStatus::Running.is_alive());
        assert!(DevProcessStatus::Healthy.is_alive());
        assert!(DevProcessStatus::Degraded("slow".into()).is_alive());
        assert!(!DevProcessStatus::Stopped.is_alive());
        assert!(!DevProcessStatus::Killed.is_alive());
        assert!(!DevProcessStatus::Exited(Some(0)).is_alive());
    }

    #[test]
    fn test_dev_summary_serialization() {
        let summary = DevProcessSummary {
            id: DevProcessId::from_string("dev-vite-123"),
            name: "Vite App".to_string(),
            process_type: DevProcessType::Frontend,
            status: DevProcessStatus::Healthy,
            pid: Some(12345),
            ports: vec![5173],
            url: Some("http://localhost:5173".to_string()),
            cpu_percent: 2.5,
            memory_rss_mb: 85.4,
            uptime_secs: 42,
            restart_count: 0,
            restart_policy: RestartPolicy::Never,
            port_resolution: Some(PortResolution::Unchanged { port: 5173 }),
        };

        let json = serde_json::to_string(&summary).unwrap();
        let decoded: DevProcessSummary = serde_json::from_str(&json).unwrap();
        assert_eq!(summary, decoded);
    }

    #[test]
    fn test_restart_policy_logic() {
        let never = RestartPolicy::Never;
        assert!(!never.should_restart(Some(0)));
        assert!(!never.should_restart(Some(1)));
        assert!(!never.should_restart(None));

        let on_fail = RestartPolicy::on_failure_default();
        assert!(!on_fail.should_restart(Some(0)));
        assert!(on_fail.should_restart(Some(1)));
        assert!(on_fail.should_restart(None));
        assert_eq!(on_fail.max_retries(), 3);
        assert_eq!(on_fail.backoff_ms(), 1000);

        let always = RestartPolicy::Always {
            max_retries: 5,
            backoff_ms: 200,
        };
        assert!(always.should_restart(Some(0)));
        assert!(always.should_restart(Some(1)));
        assert_eq!(always.max_retries(), 5);
        assert_eq!(always.backoff_ms(), 200);
    }

    #[test]
    fn test_port_conflict_policy_parsing() {
        assert_eq!(
            PortConflictPolicy::from_str_loose("error"),
            PortConflictPolicy::Error
        );
        assert_eq!(
            PortConflictPolicy::from_str_loose("fail"),
            PortConflictPolicy::Error
        );
        assert_eq!(
            PortConflictPolicy::from_str_loose("kill"),
            PortConflictPolicy::Kill
        );
        assert_eq!(
            PortConflictPolicy::from_str_loose("ignore"),
            PortConflictPolicy::Ignore
        );
        assert_eq!(
            PortConflictPolicy::from_str_loose("fallback"),
            PortConflictPolicy::Fallback
        );
        assert_eq!(
            PortConflictPolicy::from_str_loose("unknown"),
            PortConflictPolicy::Fallback
        );
    }
}

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
        };

        let json = serde_json::to_string(&summary).unwrap();
        let decoded: DevProcessSummary = serde_json::from_str(&json).unwrap();
        assert_eq!(summary, decoded);
    }
}

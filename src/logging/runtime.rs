use crate::constants::{CONFIG_DIR_NAME, RUNTIME_DIR_NAME, SESSIONS_DIR_NAME, WORKSPACE_DIR_NAME};
use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Descriptor of an actively running minicode agent process.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveSessionRecord {
    pub session_id: String,
    pub pid: u32,
    pub workspace: String,
    pub provider: String,
    pub model: String,
    pub started_at: String,
    pub session_file: String,
}

/// RAII Guard that unregisters the active process lockfile on termination.
pub struct ActiveSessionGuard {
    pub session_id: String,
    pub runtime_file: PathBuf,
}

impl Drop for ActiveSessionGuard {
    fn drop(&mut self) {
        if let Err(e) = std::fs::remove_file(&self.runtime_file) {
            tracing::debug!(
                file = %self.runtime_file.display(),
                error = %e,
                "Active session runtime lockfile already removed or unavailable"
            );
        } else {
            tracing::debug!(
                session_id = %self.session_id,
                "Unregistered active session from runtime directory"
            );
        }
    }
}

/// Returns the global runtime registry directory (`~/.config/minicode/runtime`).
pub fn runtime_dir() -> PathBuf {
    if let Some(config_dir) = dirs::config_dir() {
        config_dir.join(CONFIG_DIR_NAME).join(RUNTIME_DIR_NAME)
    } else {
        PathBuf::from(WORKSPACE_DIR_NAME).join(RUNTIME_DIR_NAME)
    }
}

/// Registers the current minicode process in the global runtime registry.
pub fn register_active_session(
    session_id: &str,
    workspace: &Path,
    provider: &str,
    model: &str,
    session_file: &Path,
) -> anyhow::Result<ActiveSessionGuard> {
    let dir = runtime_dir();
    std::fs::create_dir_all(&dir)?;

    let runtime_file = dir.join(format!("{}.json", session_id));
    let record = ActiveSessionRecord {
        session_id: session_id.to_string(),
        pid: std::process::id(),
        workspace: workspace.display().to_string(),
        provider: provider.to_string(),
        model: model.to_string(),
        started_at: chrono::Utc::now().to_rfc3339(),
        session_file: session_file.display().to_string(),
    };

    let serialized = serde_json::to_string_pretty(&record)?;
    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&runtime_file)?;
    file.write_all(serialized.as_bytes())?;
    file.flush()?;

    tracing::debug!(
        session_id = %session_id,
        pid = record.pid,
        runtime_file = %runtime_file.display(),
        "Registered active minicode session in runtime registry"
    );

    Ok(ActiveSessionGuard {
        session_id: session_id.to_string(),
        runtime_file,
    })
}

/// Checks whether a process with the given PID is currently alive on the host.
pub fn is_pid_alive(pid: u32) -> bool {
    #[cfg(target_os = "linux")]
    {
        Path::new(&format!("/proc/{}", pid)).exists()
    }
    #[cfg(not(target_os = "linux"))]
    {
        std::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .output()
            .map(|out| out.status.success())
            .unwrap_or(false)
    }
}

/// Lists all active minicode agent sessions across all repositories on the machine.
/// Automatically cleans up stale records for terminated or crashed PIDs.
pub fn list_active_sessions() -> Vec<ActiveSessionRecord> {
    let dir = runtime_dir();
    let entries = match std::fs::read_dir(&dir) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };

    let mut active = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("json") {
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(record) = serde_json::from_str::<ActiveSessionRecord>(&content) {
                    if is_pid_alive(record.pid) {
                        active.push(record);
                        continue;
                    } else {
                        // Process is dead: purge stale lockfile
                        let _ = std::fs::remove_file(&path);
                    }
                }
            }
        }
    }

    active.sort_by(|a, b| b.started_at.cmp(&a.started_at));
    active
}

/// Finds a session file path and optional active record by ID or partial prefix match.
pub fn find_session_by_id_or_prefix(
    query: &str,
    workspace: Option<&Path>,
) -> Option<(PathBuf, Option<ActiveSessionRecord>)> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return None;
    }

    // 1. Search active sessions first
    let active_sessions = list_active_sessions();
    for active in &active_sessions {
        if active.session_id == trimmed
            || active.session_id.starts_with(trimmed)
            || active.session_id.ends_with(trimmed)
        {
            let path = PathBuf::from(&active.session_file);
            if path.exists() {
                return Some((path, Some(active.clone())));
            }
        }
    }

    // 2. Search local workspace sessions if provided
    if let Some(ws) = workspace {
        let local_dir = ws.join(WORKSPACE_DIR_NAME).join(SESSIONS_DIR_NAME);
        if let Some(found) = find_jsonl_in_dir(&local_dir, trimmed) {
            return Some((found, None));
        }
    }

    // 3. Search global sessions directory
    if let Some(config_dir) = dirs::config_dir() {
        let global_dir = config_dir.join(CONFIG_DIR_NAME).join(SESSIONS_DIR_NAME);
        if let Some(found) = find_jsonl_in_dir(&global_dir, trimmed) {
            return Some((found, None));
        }
    }

    None
}

/// Scans a directory for a JSONL file whose stem matches or contains the query prefix.
fn find_jsonl_in_dir(dir: &Path, query: &str) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    let mut candidates = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("jsonl") {
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                if stem == query {
                    return Some(path); // Exact match
                }
                if stem.starts_with(query) || stem.ends_with(query) || stem.contains(query) {
                    candidates.push(path);
                }
            }
        }
    }

    candidates.into_iter().next()
}

/// Resolves the default session to view or tail.
/// Checks:
/// 1. Active session in the current workspace
/// 2. Single global active session
/// 3. Most recent session in the workspace or global store
pub fn resolve_default_session(workspace: &Path) -> Option<(PathBuf, Option<ActiveSessionRecord>)> {
    let active_sessions = list_active_sessions();
    let ws_canonical = std::fs::canonicalize(workspace).unwrap_or_else(|_| workspace.to_path_buf());
    let ws_str = ws_canonical.display().to_string();

    // Check if an active agent is running in the current workspace
    for active in &active_sessions {
        if active.workspace == ws_str {
            let path = PathBuf::from(&active.session_file);
            if path.exists() {
                return Some((path, Some(active.clone())));
            }
        }
    }

    // If there's exactly 1 active agent running on the entire system, pick it!
    if active_sessions.len() == 1 {
        let active = &active_sessions[0];
        let path = PathBuf::from(&active.session_file);
        if path.exists() {
            return Some((path, Some(active.clone())));
        }
    }

    // Fall back to most recent historical session in workspace
    let store = crate::session::store::SessionStore::with_workspace(workspace);
    if let Ok(sessions) = store.list_sessions() {
        if let Some(first) = sessions.first() {
            let path = PathBuf::from(&first.path);
            if path.exists() {
                return Some((path, None));
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_current_pid_is_alive() {
        let my_pid = std::process::id();
        assert!(is_pid_alive(my_pid));
        assert!(!is_pid_alive(9_999_999));
    }

    #[test]
    fn test_active_session_guard_creation_and_cleanup() {
        let temp = tempfile::tempdir().unwrap();
        let session_id = "test-session-guard-123";
        let session_file = temp.path().join("session.jsonl");
        std::fs::write(&session_file, "{\"session_meta\":{}}\n").unwrap();

        let runtime_file = runtime_dir().join(format!("{}.json", session_id));
        let guard = register_active_session(
            session_id,
            temp.path(),
            "test_provider",
            "test_model",
            &session_file,
        )
        .unwrap();

        assert!(runtime_file.exists());
        drop(guard);
        assert!(!runtime_file.exists());
    }
}

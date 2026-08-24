use crate::error::{Result, ToolError};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

/// An entry on the shared subagent scratchpad blackboard.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScratchpadEntry {
    pub key: String,
    pub title: String,
    pub content: String,
    pub author: String,
    pub updated_at_secs: u64,
}

/// Thread-safe shared scratchpad blackboard for subagents and orchestrators.
#[derive(Debug, Clone)]
pub struct SharedScratchpad {
    entries: Arc<RwLock<HashMap<String, ScratchpadEntry>>>,
}

impl Default for SharedScratchpad {
    fn default() -> Self {
        Self::new()
    }
}

#[allow(dead_code)]
impl SharedScratchpad {
    pub fn new() -> Self {
        Self {
            entries: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Writes or updates a key-value entry on the shared scratchpad.
    pub fn write_entry(
        &self,
        key: &str,
        title: &str,
        content: &str,
        author: &str,
    ) -> ScratchpadEntry {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let entry = ScratchpadEntry {
            key: key.to_string(),
            title: title.to_string(),
            content: content.to_string(),
            author: author.to_string(),
            updated_at_secs: now,
        };

        let mut lock = self.entries.write().unwrap_or_else(|e| e.into_inner());
        lock.insert(key.to_string(), entry.clone());
        entry
    }

    /// Reads a specific entry by key.
    pub fn read_entry(&self, key: &str) -> Option<ScratchpadEntry> {
        let lock = self.entries.read().unwrap_or_else(|e| e.into_inner());
        lock.get(key).cloned()
    }

    /// Lists all entries currently on the scratchpad.
    pub fn list_entries(&self) -> Vec<ScratchpadEntry> {
        let lock = self.entries.read().unwrap_or_else(|e| e.into_inner());
        let mut list: Vec<ScratchpadEntry> = lock.values().cloned().collect();
        list.sort_by(|a, b| a.key.cmp(&b.key));
        list
    }

    /// Deletes an entry by key.
    pub fn delete_entry(&self, key: &str) -> bool {
        let mut lock = self.entries.write().unwrap_or_else(|e| e.into_inner());
        lock.remove(key).is_some()
    }

    /// Clears all entries from the scratchpad.
    #[allow(dead_code)]
    pub fn clear(&self) {
        let mut lock = self.entries.write().unwrap_or_else(|e| e.into_inner());
        lock.clear();
    }

    /// Saves scratchpad entries to disk.
    pub fn save_to_disk(&self, workspace_root: &Path) -> Result<()> {
        let path = Self::storage_path(workspace_root);
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let list = self.list_entries();
        let raw = serde_json::to_string_pretty(&list)
            .map_err(|e| ToolError::CommandExec(e.to_string()))?;
        fs::write(&path, raw).map_err(|e| ToolError::FileOp {
            path: path.display().to_string(),
            source: e,
        })?;
        Ok(())
    }

    /// Loads scratchpad entries from disk.
    pub fn load_from_disk(&self, workspace_root: &Path) -> Result<usize> {
        let path = Self::storage_path(workspace_root);
        if !path.exists() {
            return Ok(0);
        }
        let raw = fs::read_to_string(&path).map_err(|e| ToolError::FileOp {
            path: path.display().to_string(),
            source: e,
        })?;
        let list: Vec<ScratchpadEntry> = serde_json::from_str(&raw).unwrap_or_default();
        let mut lock = self.entries.write().unwrap_or_else(|e| e.into_inner());
        let count = list.len();
        for entry in list {
            lock.insert(entry.key.clone(), entry);
        }
        Ok(count)
    }

    fn storage_path(workspace_root: &Path) -> PathBuf {
        workspace_root.join(".minicode").join("scratchpad.json")
    }
}

/// An asynchronous inter-worker message for subagent swarms.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkerMessage {
    pub id: String,
    pub from_worker_id: String,
    pub to_worker_id: Option<String>, // None indicates a broadcast to all workers
    pub topic: String,
    pub payload: String,
    pub timestamp_secs: u64,
}

/// Inter-worker messaging bus for coordinating multi-agent swarms.
#[derive(Debug, Clone)]
pub struct WorkerMessageBus {
    messages: Arc<RwLock<Vec<WorkerMessage>>>,
}

impl Default for WorkerMessageBus {
    fn default() -> Self {
        Self::new()
    }
}

#[allow(dead_code)]
impl WorkerMessageBus {
    pub fn new() -> Self {
        Self {
            messages: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Posts a message to a specific recipient worker or broadcasts to all.
    pub fn send_message(
        &self,
        from_worker_id: &str,
        to_worker_id: Option<&str>,
        topic: &str,
        payload: &str,
    ) -> WorkerMessage {
        static MSG_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1000);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let msg_seq = MSG_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let msg = WorkerMessage {
            id: format!("msg_{}_{}", now, msg_seq),
            from_worker_id: from_worker_id.to_string(),
            to_worker_id: to_worker_id.map(|s| s.to_string()),
            topic: topic.to_string(),
            payload: payload.to_string(),
            timestamp_secs: now,
        };

        let mut lock = self.messages.write().unwrap_or_else(|e| e.into_inner());
        lock.push(msg.clone());
        msg
    }

    /// Retrieves all messages directed to `worker_id` or broadcasted.
    pub fn read_inbox(&self, worker_id: &str) -> Vec<WorkerMessage> {
        let lock = self.messages.read().unwrap_or_else(|e| e.into_inner());
        lock.iter()
            .filter(|m| match &m.to_worker_id {
                Some(to) => to == worker_id,
                None => m.from_worker_id != worker_id, // Broadcast from other workers
            })
            .cloned()
            .collect()
    }

    /// Lists all historical messages across the swarm.
    #[allow(dead_code)]
    pub fn all_messages(&self) -> Vec<WorkerMessage> {
        let lock = self.messages.read().unwrap_or_else(|e| e.into_inner());
        lock.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shared_scratchpad_crud() {
        let sp = SharedScratchpad::new();
        sp.write_entry(
            "api_endpoints",
            "Discovered APIs",
            "/auth, /user",
            "researcher_1",
        );

        let entry = sp.read_entry("api_endpoints").unwrap();
        assert_eq!(entry.title, "Discovered APIs");
        assert_eq!(entry.content, "/auth, /user");

        let all = sp.list_entries();
        assert_eq!(all.len(), 1);

        assert!(sp.delete_entry("api_endpoints"));
        assert!(sp.read_entry("api_endpoints").is_none());
    }

    #[test]
    fn test_worker_message_bus_direct_and_broadcast() {
        let bus = WorkerMessageBus::new();

        // Direct message
        bus.send_message("worker_1", Some("worker_2"), "task_ready", "payload_data");
        // Broadcast
        bus.send_message("worker_1", None, "swarm_alert", "all workers sync");

        // Worker 2 should see both direct and broadcast
        let inbox_2 = bus.read_inbox("worker_2");
        assert_eq!(inbox_2.len(), 2);

        // Worker 1 should see 0 (should not receive own broadcast)
        let inbox_1 = bus.read_inbox("worker_1");
        assert_eq!(inbox_1.len(), 0);
    }
}

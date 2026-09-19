use crate::agent::subagent::message::AgentMessage;
use crate::agent::subagent::types::AgentId;
use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Thread-safe and process-safe reactive FIFO mailbox backed by a disk-persisted JSONL file
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct AgentMailbox {
    agent_id: AgentId,
    mailbox_path: PathBuf,
}

#[allow(dead_code)]
impl AgentMailbox {
    /// Initializes an AgentMailbox for the specified agent within its directory
    pub fn new(agent_id: AgentId, agent_dir: &Path) -> std::io::Result<Self> {
        std::fs::create_dir_all(agent_dir)?;
        let mailbox_path = agent_dir.join("mailbox.jsonl");
        Ok(Self {
            agent_id,
            mailbox_path,
        })
    }

    /// Returns the owner AgentId of this mailbox
    pub fn agent_id(&self) -> &AgentId {
        &self.agent_id
    }

    /// Returns the file path of this mailbox
    pub fn mailbox_path(&self) -> &Path {
        &self.mailbox_path
    }

    /// Appends a message to the agent's mailbox JSONL file with sync_all durability
    pub fn post(&self, msg: AgentMessage) -> std::io::Result<()> {
        let mut line = serde_json::to_string(&msg)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        line.push('\n');

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.mailbox_path)?;

        file.write_all(line.as_bytes())?;
        file.sync_all()?;
        Ok(())
    }

    /// Atomically drains all unread messages from the mailbox
    ///
    /// Renames the active `mailbox.jsonl` to an ephemeral processing file to prevent
    /// race conditions against concurrent posts, reads and parses each line, and deletes
    /// the processing file upon completion.
    pub fn drain_unread(&self) -> std::io::Result<Vec<AgentMessage>> {
        if !self.mailbox_path.exists() {
            return Ok(Vec::new());
        }

        let parent = self.mailbox_path.parent().unwrap_or_else(|| Path::new("."));

        let now_nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();

        let processing_file = format!(
            "mailbox.processing.{}.{}.jsonl",
            now_nanos,
            uuid::Uuid::new_v4()
        );
        let processing_path = parent.join(processing_file);

        if let Err(e) = std::fs::rename(&self.mailbox_path, &processing_path) {
            if e.kind() == std::io::ErrorKind::NotFound {
                return Ok(Vec::new());
            }
            return Err(e);
        }

        let file = match std::fs::File::open(&processing_path) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e),
        };

        let reader = BufReader::new(file);
        let mut messages = Vec::new();

        for line_res in reader.lines() {
            let line = line_res?;
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            match serde_json::from_str::<AgentMessage>(trimmed) {
                Ok(msg) => messages.push(msg),
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        line = %trimmed,
                        "Failed to deserialize AgentMessage from mailbox"
                    );
                }
            }
        }

        let _ = std::fs::remove_file(&processing_path);
        Ok(messages)
    }

    /// Returns the number of unread messages currently in the mailbox
    pub fn unread_count(&self) -> std::io::Result<usize> {
        if !self.mailbox_path.exists() {
            return Ok(0);
        }

        let file = match std::fs::File::open(&self.mailbox_path) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
            Err(e) => return Err(e),
        };

        let reader = BufReader::new(file);
        let mut count = 0;
        for line_res in reader.lines() {
            let line = line_res?;
            if !line.trim().is_empty() {
                count += 1;
            }
        }

        Ok(count)
    }
}

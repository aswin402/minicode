//! Durable, process-safe Swarm Message Bus for cross-worker inter-agent messaging.
#![allow(dead_code)]

use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use crate::agent::swarm::models::SwarmError;

/// Maximum payload length allowed in a single peer message.
pub const MAX_SWARM_MESSAGE_PAYLOAD_LEN: usize = 800;

/// Maximum outgoing messages a single worker may send during a wave.
pub const MAX_SWARM_WORKER_MESSAGES_PER_WAVE: usize = 3;

/// Typed classification of inter-agent messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SwarmMessageIntent {
    /// Exposing exported function signatures, structs, types, or API endpoints.
    PublishContract,
    /// Asking a peer worker for the signature or contract of an uncommitted symbol.
    QueryInterface,
    /// Sharing a critical environmental or architectural discovery with peers.
    CoordinationNote,
}

impl SwarmMessageIntent {
    pub fn badge(&self) -> &'static str {
        match self {
            Self::PublishContract => "📜 Contract",
            Self::QueryInterface => "❓ Query",
            Self::CoordinationNote => "📢 Note",
        }
    }
}

/// A structured peer message exchanged between workers in a swarm wave.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SwarmMessage {
    pub id: String,
    pub swarm_id: String,
    pub from_task: String,
    pub to_task: Option<String>,
    pub intent: SwarmMessageIntent,
    pub topic: String,
    pub payload: String,
    pub timestamp: String,
}

impl SwarmMessage {
    pub fn new(
        swarm_id: impl Into<String>,
        from_task: impl Into<String>,
        to_task: Option<&str>,
        intent: SwarmMessageIntent,
        topic: impl Into<String>,
        payload: impl Into<String>,
    ) -> Self {
        let from_task_str = from_task.into();
        let now_millis = Utc::now().timestamp_millis();
        let unique_suffix = uuid::Uuid::new_v4().to_string();
        let short_suffix = &unique_suffix[..6];
        let id = format!("msg-{}-{}-{}", from_task_str, now_millis, short_suffix);

        Self {
            id,
            swarm_id: swarm_id.into(),
            from_task: from_task_str,
            to_task: to_task.map(|t| t.to_string()),
            intent,
            topic: topic.into(),
            payload: payload.into(),
            timestamp: Utc::now().to_rfc3339(),
        }
    }

    /// Formats the message into an XML context block for turn-start ingestion.
    pub fn format_for_prompt(&self) -> String {
        let recipient_str = self.to_task.as_deref().unwrap_or("all_peers (broadcast)");
        format!(
            "<peer_message id=\"{}\" from=\"{}\" to=\"{}\" intent=\"{}\" topic=\"{}\" time=\"{}\">\n{}\n</peer_message>",
            self.id,
            self.from_task,
            recipient_str,
            serde_json::to_string(&self.intent).unwrap_or_default().trim_matches('"'),
            self.topic,
            self.timestamp,
            self.payload.trim()
        )
    }
}

/// Durable, process-safe Swarm Message Bus.
#[derive(Debug, Clone)]
pub struct SwarmMessageBus {
    bus_path: PathBuf,
}

impl SwarmMessageBus {
    pub fn new(swarm_dir: &Path) -> Result<Self, SwarmError> {
        fs::create_dir_all(swarm_dir)?;
        let bus_path = swarm_dir.join("bus.jsonl");
        Ok(Self { bus_path })
    }

    pub fn bus_path(&self) -> &Path {
        &self.bus_path
    }

    /// Appends a new message atomically to the bus with strict quota and size validation.
    pub fn post_message(&self, msg: SwarmMessage) -> Result<(), SwarmError> {
        if msg.payload.len() > MAX_SWARM_MESSAGE_PAYLOAD_LEN {
            return Err(SwarmError::MessagePayloadTooLarge(msg.payload.len()));
        }

        if let Some(ref recipient) = msg.to_task {
            if recipient == &msg.from_task {
                return Err(SwarmError::SelfMessageNotAllowed);
            }
        }

        let all = self.all_messages()?;
        let sender_count = all.iter().filter(|m| m.from_task == msg.from_task).count();

        if sender_count >= MAX_SWARM_WORKER_MESSAGES_PER_WAVE {
            return Err(SwarmError::MessageQuotaExceeded(msg.from_task));
        }

        let mut line = serde_json::to_string(&msg)?;
        line.push('\n');

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.bus_path)?;

        file.write_all(line.as_bytes())?;
        file.sync_data()?;

        Ok(())
    }

    /// Reads all messages posted after `last_seen_id` targeted to `task_id` or broadcasted.
    pub fn read_unread(
        &self,
        task_id: &str,
        last_seen_id: Option<&str>,
    ) -> Result<Vec<SwarmMessage>, SwarmError> {
        let all = self.all_messages()?;
        let mut past_cursor = last_seen_id.is_none();
        let mut unread = Vec::new();

        for msg in all {
            if !past_cursor {
                if Some(msg.id.as_str()) == last_seen_id {
                    past_cursor = true;
                }
                continue;
            }

            // Exclude messages authored by the caller
            if msg.from_task == task_id {
                continue;
            }

            // Include if addressed to task_id or broadcast (None)
            let is_recipient = match &msg.to_task {
                Some(recipient) => recipient == task_id,
                None => true,
            };

            if is_recipient {
                unread.push(msg);
            }
        }

        Ok(unread)
    }

    /// Reads all historical messages from `bus.jsonl`.
    pub fn all_messages(&self) -> Result<Vec<SwarmMessage>, SwarmError> {
        if !self.bus_path.exists() {
            return Ok(Vec::new());
        }

        let file = fs::File::open(&self.bus_path)?;
        let reader = BufReader::new(file);
        let mut messages = Vec::new();

        for line_res in reader.lines() {
            let line = line_res?;
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if let Ok(msg) = serde_json::from_str::<SwarmMessage>(trimmed) {
                messages.push(msg);
            }
        }

        Ok(messages)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_bus_post_and_read_unread() {
        let dir = tempdir().unwrap();
        let bus = SwarmMessageBus::new(dir.path()).unwrap();

        let msg1 = SwarmMessage::new(
            "swarm-123",
            "t1_backend",
            Some("t2_frontend"),
            SwarmMessageIntent::PublishContract,
            "Auth API",
            "export interface AuthToken { token: string; }",
        );
        bus.post_message(msg1.clone()).unwrap();

        // Reading unread for t2_frontend should return msg1
        let unread = bus.read_unread("t2_frontend", None).unwrap();
        assert_eq!(unread.len(), 1);
        assert_eq!(unread[0].topic, "Auth API");

        // Reading unread with cursor msg1.id should return 0
        let unread_next = bus.read_unread("t2_frontend", Some(&msg1.id)).unwrap();
        assert_eq!(unread_next.len(), 0);

        // Sender t1_backend should not receive its own message
        let unread_sender = bus.read_unread("t1_backend", None).unwrap();
        assert_eq!(unread_sender.len(), 0);
    }

    #[test]
    fn test_bus_quota_enforcement() {
        let dir = tempdir().unwrap();
        let bus = SwarmMessageBus::new(dir.path()).unwrap();

        for i in 1..=3 {
            let msg = SwarmMessage::new(
                "swarm-123",
                "t1_worker",
                None,
                SwarmMessageIntent::CoordinationNote,
                format!("Note {}", i),
                format!("Payload {}", i),
            );
            assert!(bus.post_message(msg).is_ok());
        }

        // 4th message should fail quota
        let msg4 = SwarmMessage::new(
            "swarm-123",
            "t1_worker",
            None,
            SwarmMessageIntent::CoordinationNote,
            "Note 4",
            "Payload 4",
        );
        let res = bus.post_message(msg4);
        assert!(matches!(res, Err(SwarmError::MessageQuotaExceeded(_))));
    }

    #[test]
    fn test_bus_payload_limit() {
        let dir = tempdir().unwrap();
        let bus = SwarmMessageBus::new(dir.path()).unwrap();

        let big_payload = "A".repeat(801);
        let msg = SwarmMessage::new(
            "swarm-123",
            "t1_worker",
            None,
            SwarmMessageIntent::CoordinationNote,
            "Big",
            big_payload,
        );
        let res = bus.post_message(msg);
        assert!(matches!(res, Err(SwarmError::MessagePayloadTooLarge(_))));
    }

    #[test]
    fn test_bus_self_message_rejected() {
        let dir = tempdir().unwrap();
        let bus = SwarmMessageBus::new(dir.path()).unwrap();

        let msg = SwarmMessage::new(
            "swarm-123",
            "t1_worker",
            Some("t1_worker"),
            SwarmMessageIntent::CoordinationNote,
            "Self",
            "Hello me",
        );
        let res = bus.post_message(msg);
        assert!(matches!(res, Err(SwarmError::SelfMessageNotAllowed)));
    }
}

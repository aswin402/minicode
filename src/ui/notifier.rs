//! Cross-platform System Attention Notifier (`oh-my-opencode` parity).
//!
//! Emits terminal attention sequences (OSC 9 / OSC 777 + `\x07` bell) and non-blocking
//! OS desktop notifications (`notify-send` on Linux, `osascript` on macOS) when the agent
//! suspends for human input (`ask_user` / `ApprovalRequest`) or completes a long-running
//! background turn.

use crate::agent::types::AgentEvent;
use std::io::Write;
use std::time::{Duration, Instant};

/// Semantic attention categories triggered by agent lifecycle events.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttentionKind {
    /// Agent suspended on `ask_user` (`AgentEvent::UserInquiry`) waiting for human input.
    Inquiry,
    /// Agent suspended on `ApprovalRequest` waiting for tool confirmation.
    Approval,
    /// Long-running autonomous turn completed (`AgentEvent::TurnEnd`).
    TurnCompleted,
    /// Unrecoverable turn error (`AgentEvent::Error` with `retrying == false`).
    TurnFailed,
}

/// Declarative metadata rule for each `AttentionKind`.
#[derive(Debug, Clone, Copy)]
pub struct AttentionDescriptor {
    pub kind: AttentionKind,
    pub default_title: &'static str,
    pub urgency: &'static str,
}

const ATTENTION_DESCRIPTORS: &[AttentionDescriptor] = &[
    AttentionDescriptor {
        kind: AttentionKind::Inquiry,
        default_title: "minicode · Input Requested",
        urgency: "critical",
    },
    AttentionDescriptor {
        kind: AttentionKind::Approval,
        default_title: "minicode · Approval Required",
        urgency: "critical",
    },
    AttentionDescriptor {
        kind: AttentionKind::TurnCompleted,
        default_title: "minicode · Turn Completed",
        urgency: "normal",
    },
    AttentionDescriptor {
        kind: AttentionKind::TurnFailed,
        default_title: "minicode · Execution Error",
        urgency: "critical",
    },
];

impl AttentionKind {
    #[must_use]
    pub fn descriptor(self) -> &'static AttentionDescriptor {
        ATTENTION_DESCRIPTORS
            .iter()
            .find(|d| d.kind == self)
            .unwrap_or(&ATTENTION_DESCRIPTORS[2])
    }
}

/// Resolved attention payload ready for terminal OSC / bell and desktop dispatch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttentionAlert {
    pub kind: AttentionKind,
    pub title: String,
    pub body: String,
    pub urgency: &'static str,
}

/// Manages rate-limited terminal bell / OSC sequences and OS desktop notifications.
#[derive(Debug)]
pub struct SystemNotifier {
    pub bell_enabled: bool,
    pub desktop_enabled: bool,
    pub min_turn_secs_for_notify: u64,
    pub cooldown: Duration,
    last_emitted_at: Option<Instant>,
}

impl Default for SystemNotifier {
    fn default() -> Self {
        Self::new()
    }
}

impl SystemNotifier {
    #[must_use]
    pub fn new() -> Self {
        Self {
            bell_enabled: true,
            desktop_enabled: true,
            min_turn_secs_for_notify: 12,
            cooldown: Duration::from_secs(3),
            last_emitted_at: None,
        }
    }

    /// Classifies an `AgentEvent` into a structured `AttentionAlert` if it warrants user attention.
    #[must_use]
    pub fn classify_event(
        &self,
        event: &AgentEvent,
        turn_elapsed: Option<Duration>,
    ) -> Option<AttentionAlert> {
        match event {
            AgentEvent::UserInquiry { request, .. } => {
                let desc = AttentionKind::Inquiry.descriptor();
                let q_count = request.questions.len();
                let body = if request.title.trim().is_empty() {
                    format!("{q_count} question(s) waiting for your response.")
                } else {
                    format!("{} ({q_count}Q)", Self::sanitize_text(&request.title))
                };
                Some(AttentionAlert {
                    kind: AttentionKind::Inquiry,
                    title: desc.default_title.to_string(),
                    body,
                    urgency: desc.urgency,
                })
            }
            AgentEvent::ApprovalRequest { tool, reason, .. } => {
                let desc = AttentionKind::Approval.descriptor();
                let clean_reason = Self::sanitize_text(reason);
                let body = if clean_reason.is_empty() {
                    format!("Tool `{tool}` requires confirmation.")
                } else {
                    format!("`{tool}` — {clean_reason}")
                };
                Some(AttentionAlert {
                    kind: AttentionKind::Approval,
                    title: desc.default_title.to_string(),
                    body,
                    urgency: desc.urgency,
                })
            }
            AgentEvent::TurnEnd {
                status,
                total_tokens_used,
                ..
            } => {
                let elapsed = turn_elapsed?;
                if elapsed.as_secs() < self.min_turn_secs_for_notify {
                    return None;
                }
                let desc = AttentionKind::TurnCompleted.descriptor();
                let body = format!(
                    "Finished ({}) in {}s ({:.1}k tokens).",
                    status,
                    elapsed.as_secs(),
                    *total_tokens_used as f64 / 1000.0
                );
                Some(AttentionAlert {
                    kind: AttentionKind::TurnCompleted,
                    title: desc.default_title.to_string(),
                    body,
                    urgency: desc.urgency,
                })
            }
            AgentEvent::Error {
                message,
                retrying: false,
                ..
            } => {
                let desc = AttentionKind::TurnFailed.descriptor();
                Some(AttentionAlert {
                    kind: AttentionKind::TurnFailed,
                    title: desc.default_title.to_string(),
                    body: Self::sanitize_text(message),
                    urgency: desc.urgency,
                })
            }
            _ => None,
        }
    }

    /// Formats the combined OSC 777 + OSC 9 + BEL (`\x07`) terminal sequence for an alert.
    #[must_use]
    pub fn format_terminal_escape(alert: &AttentionAlert) -> String {
        let title = Self::sanitize_osc_field(&alert.title);
        let body = Self::sanitize_osc_field(&alert.body);
        format!("\x1b]777;notify;{title};{body}\x07\x1b]9;{title}: {body}\x07\x07")
    }

    /// Evaluates `event` and dispatches rate-limited terminal/OS notifications if applicable.
    pub fn ingest_agent_event(&mut self, event: &AgentEvent, turn_elapsed: Option<Duration>) {
        let Some(alert) = self.classify_event(event, turn_elapsed) else {
            return;
        };
        let now = Instant::now();
        if let Some(prev) = self.last_emitted_at {
            if now.saturating_duration_since(prev) < self.cooldown {
                return;
            }
        }
        self.last_emitted_at = Some(now);

        if self.bell_enabled {
            let seq = Self::format_terminal_escape(&alert);
            let _ = std::io::stderr().write_all(seq.as_bytes());
            let _ = std::io::stderr().flush();
        }

        if self.desktop_enabled {
            Self::spawn_desktop_notification(&alert);
        }
    }

    fn spawn_desktop_notification(alert: &AttentionAlert) {
        #[cfg(target_os = "linux")]
        {
            let has_display = std::env::var_os("DISPLAY").is_some()
                || std::env::var_os("WAYLAND_DISPLAY").is_some();
            if !has_display {
                return;
            }
            let _ = std::process::Command::new("notify-send")
                .arg("-a")
                .arg("minicode")
                .arg("-u")
                .arg(alert.urgency)
                .arg("-t")
                .arg("5000")
                .arg(&alert.title)
                .arg(&alert.body)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn();
        }
        #[cfg(target_os = "macos")]
        {
            let script = format!(
                "display notification \"{}\" with title \"{}\"",
                alert.body.replace('\\', "\\\\").replace('"', "\\\""),
                alert.title.replace('\\', "\\\\").replace('"', "\\\"")
            );
            let _ = std::process::Command::new("osascript")
                .arg("-e")
                .arg(script)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn();
        }
    }

    fn sanitize_text(s: &str) -> String {
        s.lines()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("")
            .trim()
            .chars()
            .take(96)
            .collect()
    }

    fn sanitize_osc_field(s: &str) -> String {
        s.chars()
            .filter(|c| !c.is_control() && *c != ';')
            .take(80)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::inquiry::{InquiryInputType, InquiryQuestion, InquiryRequest};

    #[test]
    fn test_system_notifier_classifies_inquiry_approval_and_long_turn() {
        let notifier = SystemNotifier::new();

        let inq_event = AgentEvent::UserInquiry {
            turn_id: 1,
            tool_id: "call_1".to_string(),
            request: InquiryRequest {
                inquiry_id: "inq_1".to_string(),
                title: "Database Engine Selection".to_string(),
                description: None,
                questions: vec![InquiryQuestion {
                    id: "db".to_string(),
                    question: "Which DB?".to_string(),
                    header: None,
                    options: Vec::new(),
                    is_multi_select: false,
                    allow_custom: true,
                    placeholder: None,
                    default_value: None,
                    min_value: None,
                    max_value: None,
                    step: None,
                    input_type: InquiryInputType::Choice,
                }],
            },
        };
        let alert = notifier
            .classify_event(&inq_event, None)
            .expect("inquiry alert");
        assert_eq!(alert.kind, AttentionKind::Inquiry);
        assert_eq!(alert.urgency, "critical");
        assert!(alert.body.contains("Database Engine Selection"));
        let osc = SystemNotifier::format_terminal_escape(&alert);
        assert!(osc.contains("\x1b]777;notify;"));
        assert!(osc.contains("\x1b]9;"));
        assert!(osc.ends_with('\x07'));

        let approval_event = AgentEvent::ApprovalRequest {
            turn_id: 1,
            tool_id: "call_2".to_string(),
            tool: "exec_cmd".to_string(),
            args: serde_json::json!({}),
            reason: "Destructive command".to_string(),
        };
        let app_alert = notifier
            .classify_event(&approval_event, None)
            .expect("approval alert");
        assert_eq!(app_alert.kind, AttentionKind::Approval);
        assert!(app_alert.body.contains("exec_cmd"));

        let short_turn = AgentEvent::TurnEnd {
            turn_id: 1,
            status: "complete".to_string(),
            total_tokens_used: 1200,
            files_modified: Vec::new(),
            cached_prompt_tokens: 0,
        };
        assert!(notifier
            .classify_event(&short_turn, Some(Duration::from_secs(3)))
            .is_none());
        let long_alert = notifier
            .classify_event(&short_turn, Some(Duration::from_secs(18)))
            .expect("long turn alert");
        assert_eq!(long_alert.kind, AttentionKind::TurnCompleted);
        assert!(long_alert.body.contains("18s"));
    }
}

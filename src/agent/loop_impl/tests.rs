//! Unit tests for `AgentLoop`, mandatory approvals, session hydration, and interactive inquiries.

use super::*;
use crate::agent::inquiry::*;
use crate::agent::provider::{ChunkStream, CompletionOptions, Provider, StreamChunk, ToolSchema};
use crate::constants::CONTEXT_MIN_PRESERVED_MESSAGES;
use async_trait::async_trait;
use std::sync::{Arc, Mutex};

struct DummyProvider;

#[async_trait]
impl Provider for DummyProvider {
    fn name(&self) -> &str {
        "dummy"
    }
    fn default_model(&self) -> &str {
        "dummy-model"
    }
    async fn stream_completion(
        &self,
        _messages: &[Message],
        _tools: &[ToolSchema],
        _options: &CompletionOptions,
    ) -> Result<ChunkStream> {
        let stream = tokio_stream::empty();
        Ok(Box::pin(stream))
    }
}

/// Streams exactly one scripted tool call, then Done on every later call
/// (models see the rejection and stop calling tools).
struct ToolCallProvider {
    call: ToolCall,
    spent: std::sync::atomic::AtomicBool,
}

#[async_trait]
impl Provider for ToolCallProvider {
    fn name(&self) -> &str {
        "dummy-toolcall"
    }
    fn default_model(&self) -> &str {
        "dummy-model"
    }
    async fn stream_completion(
        &self,
        _messages: &[Message],
        _tools: &[ToolSchema],
        _options: &CompletionOptions,
    ) -> Result<ChunkStream> {
        if self.spent.swap(true, std::sync::atomic::Ordering::SeqCst) {
            let stream = tokio_stream::iter(vec![Ok(StreamChunk::Done)]);
            return Ok(Box::pin(stream));
        }
        let stream = tokio_stream::iter(vec![
            Ok(StreamChunk::ToolCallChunk(self.call.clone())),
            Ok(StreamChunk::Done),
        ]);
        Ok(Box::pin(stream))
    }
}

#[tokio::test]
async fn rejected_approval_produces_failed_tool_result_without_hang() {
    let temp_dir =
        std::env::temp_dir().join(format!("minicode_loop_test_{}", uuid::Uuid::new_v4()));
    let _ = std::fs::create_dir_all(&temp_dir);
    // Established project so the one-time foundation checkpoint does not apply.
    let _ = std::fs::write(temp_dir.join("package.json"), "{}");
    let config = Config::default(); // agent.auto_approve == false
    let provider = Box::new(ToolCallProvider {
        call: ToolCall {
            id: "call_gate_1".into(),
            name: "write_file".into(),
            arguments: serde_json::json!({"path": "gate.txt", "content": "hi"}),
        },
        spent: std::sync::atomic::AtomicBool::new(false),
    });
    let mut agent_loop = AgentLoop::new(&temp_dir, config, provider);
    agent_loop.set_interactive_approvals(true);
    let reg = agent_loop.approval_registry();

    let (tx, mut rx) = mpsc::unbounded_channel();
    let turn_task =
        tokio::spawn(async move { agent_loop.execute_turn("make a file", tx, None).await });

    // Gate must block: wait for the ApprovalRequest event.
    let mut requested_tool_id = None;
    for _ in 0..50 {
        match rx.recv().await {
            Some(AgentEvent::ApprovalRequest { tool_id, .. }) => {
                requested_tool_id = Some(tool_id);
                break;
            }
            Some(_) => {}
            None => panic!("event stream ended before ApprovalRequest"),
        }
    }
    let tool_id = requested_tool_id.expect("ApprovalRequest must be emitted");
    assert!(
        !turn_task.is_finished(),
        "turn must block awaiting decision"
    );

    // Reject through the registry by key.
    let sender = reg
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .remove(&tool_id)
        .expect("pending approval must be registered under its tool_id");
    assert!(sender.send(ApprovalDecision::Reject).is_ok());

    let turn = turn_task.await.expect("join ok").expect("turn completes");
    assert_eq!(turn.tool_results.len(), 1);
    assert!(!turn.tool_results[0].success);
    assert!(turn.tool_results[0].output.contains("rejected by user"));
    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[tokio::test]
async fn cancelled_while_awaiting_approval_cleans_registry_and_reports_cancelled() {
    let temp_dir =
        std::env::temp_dir().join(format!("minicode_loop_test_{}", uuid::Uuid::new_v4()));
    let _ = std::fs::create_dir_all(&temp_dir);
    // Established project so the foundation gate does not hold exec_cmd.
    let _ = std::fs::write(temp_dir.join("package.json"), "{}");
    let config = Config::default();
    let provider = Box::new(ToolCallProvider {
        call: ToolCall {
            id: "call_cancel_1".into(),
            name: "exec_cmd".into(),
            arguments: serde_json::json!({"command": "echo hi"}),
        },
        spent: std::sync::atomic::AtomicBool::new(false),
    });
    let mut agent_loop = AgentLoop::new(&temp_dir, config, provider);
    agent_loop.set_interactive_approvals(true); // exec_cmd gates on approval
    let reg = agent_loop.approval_registry();

    let cancel = tokio_util::sync::CancellationToken::new();
    let (tx, mut rx) = mpsc::unbounded_channel();
    let cancel_for_task = cancel.clone();
    let turn_task = tokio::spawn(async move {
        agent_loop
            .execute_turn("run echo", tx, Some(cancel_for_task))
            .await
    });

    // Wait until the gate registers the pending approval.
    loop {
        match rx.recv().await {
            Some(AgentEvent::ApprovalRequest { .. }) => break,
            Some(_) => {}
            None => panic!("stream ended before ApprovalRequest"),
        }
    }
    cancel.cancel();

    let turn = turn_task.await.expect("join ok").expect("turn completes");
    assert!(
        reg.lock().unwrap_or_else(|p| p.into_inner()).is_empty(),
        "registry must not leak entries after cancellation"
    );
    assert_eq!(turn.tool_results.len(), 1);
    assert!(!turn.tool_results[0].success);
    assert!(turn.tool_results[0].output.contains("cancelled"));

    // TurnEnd must report cancelled, not complete.
    let saw_cancelled = std::iter::from_fn(|| rx.try_recv().ok()).any(|e| {
        matches!(
            e,
            AgentEvent::TurnEnd { ref status, .. } if status == "cancelled"
        )
    });
    assert!(saw_cancelled, "expected TurnEnd status=cancelled");
    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[tokio::test]
async fn precancelled_turn_emits_cancelled_turnend() {
    let temp_dir =
        std::env::temp_dir().join(format!("minicode_loop_test_{}", uuid::Uuid::new_v4()));
    let _ = std::fs::create_dir_all(&temp_dir);
    let config = Config::default();
    let mut agent_loop = AgentLoop::new(&temp_dir, config, Box::new(DummyProvider));
    let cancel = tokio_util::sync::CancellationToken::new();
    cancel.cancel();
    let (tx, mut rx) = mpsc::unbounded_channel();
    let _ = agent_loop.execute_turn("hi", tx, Some(cancel)).await;
    let saw_cancelled = std::iter::from_fn(|| rx.try_recv().ok()).any(|e| {
        matches!(
            e,
            AgentEvent::TurnEnd { ref status, .. } if status == "cancelled"
        )
    });
    assert!(saw_cancelled, "expected TurnEnd status=cancelled");
    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_prune_context_preserves_minimum_messages() {
    let temp_dir =
        std::env::temp_dir().join(format!("minicode_loop_test_{}", uuid::Uuid::new_v4()));
    let _ = std::fs::create_dir_all(&temp_dir);
    let config = Config::default();
    let provider = Box::new(DummyProvider);
    let mut agent_loop = AgentLoop::new(&temp_dir, config, provider);

    // Add small number of messages <= CONTEXT_MIN_PRESERVED_MESSAGES
    agent_loop.messages.push(Message::user("Hello 1"));
    agent_loop.messages.push(Message::assistant("Hi 1"));
    agent_loop.prune_context();
    assert_eq!(agent_loop.messages.len(), 2);
    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_prune_context_compacts_and_prunes_large_history() {
    let temp_dir =
        std::env::temp_dir().join(format!("minicode_loop_test_{}", uuid::Uuid::new_v4()));
    let _ = std::fs::create_dir_all(&temp_dir);
    let mut config = Config::default();
    config.provider.model = "dummy".to_string();
    let provider = Box::new(DummyProvider);
    let mut agent_loop = AgentLoop::new(&temp_dir, config, provider);

    // Add 10 messages with huge content
    let huge_text = "word ".repeat(15_000);
    for i in 0..10 {
        agent_loop
            .messages
            .push(Message::user(format!("User {}", i)));
        agent_loop
            .messages
            .push(Message::assistant(huge_text.clone()));
    }
    assert_eq!(agent_loop.messages.len(), 20);

    agent_loop.prune_context();
    // Should have pruned oldest messages down while preserving at least min preserved
    assert!(agent_loop.messages.len() < 20);
    assert!(agent_loop.messages.len() >= CONTEXT_MIN_PRESERVED_MESSAGES);
    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[tokio::test]
async fn test_lazy_session_creation_deferred_until_prompt() {
    let temp_dir =
        std::env::temp_dir().join(format!("minicode_lazy_test_{}", uuid::Uuid::new_v4()));
    let _ = std::fs::create_dir_all(&temp_dir);
    let mut config = Config::default();
    config.provider.model = "dummy".to_string();
    let provider = Box::new(DummyProvider);

    // 1. Initializing AgentLoop must NOT create any session file on disk!
    let agent = AgentLoop::new(&temp_dir, config, provider);
    let session_file = agent.session_store.session_file_path(agent.session_id());
    assert!(
        !session_file.exists(),
        "Session file must not exist before first prompt"
    );
    assert!(!agent.session_persisted);

    // 2. Dropping agent without prompt must leave zero files on disk
    drop(agent);
    assert!(
        !session_file.exists(),
        "Session file must not exist after drop without prompt"
    );

    let _ = std::fs::remove_dir_all(&temp_dir);
}

struct TestToolCallProvider {
    call: ToolCall,
    spent: std::sync::atomic::AtomicBool,
}

#[async_trait]
impl Provider for TestToolCallProvider {
    fn name(&self) -> &str {
        "test-inquiry-provider"
    }
    fn default_model(&self) -> &str {
        "test-model"
    }
    async fn stream_completion(
        &self,
        _messages: &[Message],
        _tools: &[ToolSchema],
        _options: &CompletionOptions,
    ) -> Result<ChunkStream> {
        if self.spent.swap(true, std::sync::atomic::Ordering::SeqCst) {
            let stream = tokio_stream::iter(vec![Ok(StreamChunk::Done)]);
            return Ok(Box::pin(stream));
        }
        let stream = tokio_stream::iter(vec![
            Ok(StreamChunk::ToolCallChunk(self.call.clone())),
            Ok(StreamChunk::Done),
        ]);
        Ok(Box::pin(stream))
    }
}

#[tokio::test]
async fn test_inquiry_registry_instantiation() {
    let registry: InquiryRegistry = Arc::new(Mutex::new(HashMap::new()));
    let (tx, rx) = tokio::sync::oneshot::channel::<InquiryResponse>();
    registry.lock().unwrap().insert("test-inq".to_string(), tx);

    let sender = registry.lock().unwrap().remove("test-inq");
    assert!(sender.is_some());
    let _ = sender.unwrap().send(InquiryResponse {
        inquiry_id: "test-inq".to_string(),
        answers: vec![],
        cancelled: false,
    });

    let resp = rx.await.unwrap();
    assert_eq!(resp.inquiry_id, "test-inq");
    assert!(!resp.cancelled);
}

#[tokio::test]
async fn test_inquiry_suspension_interactive_resolution() {
    let temp_dir = std::env::temp_dir().join(format!("minicode_inq_test_{}", uuid::Uuid::new_v4()));
    let _ = std::fs::create_dir_all(&temp_dir);
    let config = Config::default();
    let provider = Box::new(TestToolCallProvider {
        call: ToolCall {
            id: "call_inq_1".into(),
            name: "ask_user".into(),
            arguments: serde_json::json!({
                "title": "Choose database",
                "questions": [
                    {
                        "id": "db",
                        "question": "Which database?",
                        "options": [
                            { "id": "pg", "label": "PostgreSQL", "recommended": true },
                            { "id": "sqlite", "label": "SQLite" }
                        ]
                    }
                ]
            }),
        },
        spent: std::sync::atomic::AtomicBool::new(false),
    });

    let mut agent_loop = AgentLoop::new(&temp_dir, config, provider);
    agent_loop.set_interactive_approvals(true);
    let inq_reg = agent_loop.inquiry_registry();

    let (tx, mut rx) = mpsc::unbounded_channel();
    let turn_task =
        tokio::spawn(async move { agent_loop.execute_turn("setup db", tx, None).await });

    // Wait for AgentEvent::UserInquiry
    let mut captured_inquiry_id = None;
    for _ in 0..50 {
        match rx.recv().await {
            Some(AgentEvent::UserInquiry {
                tool_id, request, ..
            }) => {
                assert_eq!(tool_id, "call_inq_1");
                assert_eq!(request.title, "Choose database");
                captured_inquiry_id = Some(tool_id);
                break;
            }
            Some(_) => {}
            None => panic!("event stream ended before UserInquiry"),
        }
    }
    let inquiry_id = captured_inquiry_id.expect("UserInquiry event must be emitted");

    // Verify turn is suspended
    assert!(
        !turn_task.is_finished(),
        "turn must suspend awaiting inquiry response"
    );

    // Respond via inquiry registry
    let sender = inq_reg
        .lock()
        .unwrap()
        .remove(&inquiry_id)
        .expect("inquiry must be in registry");

    let response = InquiryResponse {
        inquiry_id: inquiry_id.clone(),
        answers: vec![InquiryAnswer {
            question_id: "db".to_string(),
            selected_options: vec!["pg".to_string()],
            custom_text: None,
            masked: false,
        }],
        cancelled: false,
    };
    assert!(sender.send(response).is_ok());

    let turn = turn_task.await.expect("join ok").expect("turn completes");
    assert_eq!(turn.tool_results.len(), 1);
    assert!(turn.tool_results[0].success);
    assert!(turn.tool_results[0].output.contains("answered"));
    assert!(turn.tool_results[0].output.contains("pg"));

    // Registry should now be empty for this inquiry
    assert!(inq_reg.lock().unwrap().get(&inquiry_id).is_none());

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[tokio::test]
async fn test_inquiry_suspension_cancellation() {
    let temp_dir =
        std::env::temp_dir().join(format!("minicode_inq_cancel_{}", uuid::Uuid::new_v4()));
    let _ = std::fs::create_dir_all(&temp_dir);
    let config = Config::default();
    let provider = Box::new(TestToolCallProvider {
        call: ToolCall {
            id: "call_inq_cancel".into(),
            name: "ask_user".into(),
            arguments: serde_json::json!({
                "title": "Choose stack",
                "questions": [
                    {
                        "id": "stack",
                        "question": "Which stack?",
                        "options": [
                            { "id": "react", "label": "React" }
                        ]
                    }
                ]
            }),
        },
        spent: std::sync::atomic::AtomicBool::new(false),
    });

    let mut agent_loop = AgentLoop::new(&temp_dir, config, provider);
    agent_loop.set_interactive_approvals(true);
    let inq_reg = agent_loop.inquiry_registry();

    let cancel_token = tokio_util::sync::CancellationToken::new();
    let cancel_clone = cancel_token.clone();

    let (tx, mut rx) = mpsc::unbounded_channel();
    let turn_task = tokio::spawn(async move {
        agent_loop
            .execute_turn("setup stack", tx, Some(cancel_clone))
            .await
    });

    // Wait for AgentEvent::UserInquiry
    for _ in 0..50 {
        match rx.recv().await {
            Some(AgentEvent::UserInquiry { tool_id, .. }) => {
                assert_eq!(tool_id, "call_inq_cancel");
                break;
            }
            Some(_) => {}
            None => panic!("event stream ended before UserInquiry"),
        }
    }

    // Cancel while suspended
    cancel_token.cancel();

    let turn = turn_task.await.expect("join ok").expect("turn completes");
    assert_eq!(turn.tool_results.len(), 1);
    assert!(turn.tool_results[0].output.contains("cancelled"));

    // Registry must be cleaned up
    assert!(inq_reg.lock().unwrap().get("call_inq_cancel").is_none());

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[tokio::test]
async fn test_inquiry_non_interactive_auto_resolve() {
    let temp_dir =
        std::env::temp_dir().join(format!("minicode_inq_noninter_{}", uuid::Uuid::new_v4()));
    let _ = std::fs::create_dir_all(&temp_dir);
    let config = Config::default();
    let provider = Box::new(TestToolCallProvider {
        call: ToolCall {
            id: "call_inq_auto".into(),
            name: "ask_user".into(),
            arguments: serde_json::json!({
                "title": "Pick template",
                "questions": [
                    {
                        "id": "tmpl",
                        "question": "Which template?",
                        "options": [
                            { "id": "default_tmpl", "label": "Default", "recommended": true },
                            { "id": "custom_tmpl", "label": "Custom" }
                        ]
                    }
                ]
            }),
        },
        spent: std::sync::atomic::AtomicBool::new(false),
    });

    let mut agent_loop = AgentLoop::new(&temp_dir, config, provider);
    // Non-interactive / headless mode (e.g. -y)
    agent_loop.set_interactive_approvals(false);
    let inq_reg = agent_loop.inquiry_registry();

    let (tx, mut rx) = mpsc::unbounded_channel();
    // Should complete without hanging or suspending
    let turn = agent_loop
        .execute_turn("auto template", tx, None)
        .await
        .expect("turn completes immediately");

    assert_eq!(turn.tool_results.len(), 1);
    assert!(turn.tool_results[0].success);
    assert!(turn.tool_results[0].output.contains("answered"));
    assert!(turn.tool_results[0].output.contains("default_tmpl"));

    // No UserInquiry event should have been emitted in non-interactive mode
    let mut saw_user_inquiry = false;
    while let Ok(event) = rx.try_recv() {
        if matches!(event, AgentEvent::UserInquiry { .. }) {
            saw_user_inquiry = true;
        }
    }
    assert!(
        !saw_user_inquiry,
        "UserInquiry event must NOT be emitted in non-interactive mode"
    );

    // Registry should be empty
    assert!(inq_reg.lock().unwrap().is_empty());

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[tokio::test]
async fn test_inquiry_empty_questions_fails_fast() {
    let temp_dir =
        std::env::temp_dir().join(format!("minicode_inq_empty_{}", uuid::Uuid::new_v4()));
    let _ = std::fs::create_dir_all(&temp_dir);
    let config = Config::default();
    let provider = Box::new(TestToolCallProvider {
        call: ToolCall {
            id: "call_inq_empty".into(),
            name: "ask_user".into(),
            arguments: serde_json::json!({
                "title": "Empty Questions Inquiry",
                "questions": []
            }),
        },
        spent: std::sync::atomic::AtomicBool::new(false),
    });

    let mut agent_loop = AgentLoop::new(&temp_dir, config, provider);
    agent_loop.set_interactive_approvals(true);

    let (tx, _rx) = mpsc::unbounded_channel();
    let turn = agent_loop
        .execute_turn("test empty inquiry", tx, None)
        .await
        .expect("turn completes");

    assert_eq!(turn.tool_results.len(), 1);
    assert!(!turn.tool_results[0].success);
    assert!(turn.tool_results[0]
        .output
        .contains("requires at least one question"));

    let _ = std::fs::remove_dir_all(&temp_dir);
}

struct TrailingErrorToolCallProvider {
    spent: std::sync::atomic::AtomicBool,
}

#[async_trait]
impl Provider for TrailingErrorToolCallProvider {
    fn name(&self) -> &str {
        "trailing-error-provider"
    }
    fn default_model(&self) -> &str {
        "test-model"
    }
    async fn stream_completion(
        &self,
        _messages: &[Message],
        _tools: &[ToolSchema],
        _options: &CompletionOptions,
    ) -> Result<ChunkStream> {
        let was_spent = self.spent.swap(true, std::sync::atomic::Ordering::SeqCst);
        if was_spent {
            let s = async_stream::stream! {
                yield Ok(StreamChunk::Done);
            };
            return Ok(Box::pin(s));
        }

        let s = async_stream::stream! {
            yield Ok(StreamChunk::Delta("I will find files.\n".to_string()));
            yield Ok(StreamChunk::ToolCallChunk(ToolCall {
                id: "call_trailing_test".to_string(),
                name: "file_search".to_string(),
                arguments: serde_json::json!({ "pattern": "*" }),
            }));
            // Trailing network closure after tool call was emitted
            yield Err(crate::error::ProviderError::StreamDecode(
                "SSE stream error: error decoding response body".to_string(),
            ).into());
        };
        Ok(Box::pin(s))
    }
}

#[tokio::test]
async fn test_trailing_stream_closure_with_tool_calls_tolerated() {
    let temp_dir =
        std::env::temp_dir().join(format!("minicode_trailing_err_{}", uuid::Uuid::new_v4()));
    let _ = std::fs::create_dir_all(&temp_dir);
    let config = Config::default();
    let provider = Box::new(TrailingErrorToolCallProvider {
        spent: std::sync::atomic::AtomicBool::new(false),
    });

    let mut agent_loop = AgentLoop::new(&temp_dir, config, provider);
    let (tx, _rx) = mpsc::unbounded_channel();
    let turn = agent_loop
        .execute_turn("list files", tx, None)
        .await
        .expect("turn should succeed by tolerating trailing stream closure");

    assert_eq!(turn.tool_results.len(), 1);
    assert_eq!(turn.tool_results[0].tool_name, "file_search");
    assert!(turn.tool_results[0].success);

    let _ = std::fs::remove_dir_all(&temp_dir);
}

struct ThinkOnlyDropThenSuccessProvider {
    calls: std::sync::atomic::AtomicUsize,
}

#[async_trait]
impl Provider for ThinkOnlyDropThenSuccessProvider {
    fn name(&self) -> &str {
        "think-only-drop-provider"
    }
    fn default_model(&self) -> &str {
        "test-model"
    }
    async fn stream_completion(
        &self,
        _messages: &[Message],
        _tools: &[ToolSchema],
        _options: &CompletionOptions,
    ) -> Result<ChunkStream> {
        let call_idx = self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if call_idx == 0 {
            // Attempt 0: Stream drops right after <think> block before tool call arrives
            let s = async_stream::stream! {
                yield Ok(StreamChunk::Delta("<think>Planning next component...</think>".to_string()));
                yield Err(crate::error::ProviderError::StreamDecode(
                    "SSE stream transport error: error decoding response body".to_string(),
                ).into());
            };
            return Ok(Box::pin(s));
        }
        if call_idx == 1 {
            // Attempt 1 (retry): Emits the tool call cleanly
            let s = async_stream::stream! {
                yield Ok(StreamChunk::Delta("Searching workspace files.\n".to_string()));
                yield Ok(StreamChunk::ToolCallChunk(ToolCall {
                    id: "call_retry_ok".to_string(),
                    name: "file_search".to_string(),
                    arguments: serde_json::json!({ "pattern": "*" }),
                }));
                yield Ok(StreamChunk::Done);
            };
            return Ok(Box::pin(s));
        }
        let s = async_stream::stream! {
            yield Ok(StreamChunk::Delta("Done.".to_string()));
            yield Ok(StreamChunk::Done);
        };
        Ok(Box::pin(s))
    }
}

#[tokio::test]
async fn test_stream_closure_after_think_only_retries_instead_of_exiting() {
    let temp_dir =
        std::env::temp_dir().join(format!("minicode_think_drop_{}", uuid::Uuid::new_v4()));
    let _ = std::fs::create_dir_all(&temp_dir);
    let config = Config::default();
    let provider = Box::new(ThinkOnlyDropThenSuccessProvider {
        calls: std::sync::atomic::AtomicUsize::new(0),
    });

    let mut agent_loop = AgentLoop::new(&temp_dir, config, provider);
    let (tx, _rx) = mpsc::unbounded_channel();
    let turn = agent_loop
        .execute_turn("continue", tx, None)
        .await
        .expect("turn should retry after think-only stream drop and succeed");

    assert_eq!(
        turn.tool_results.len(),
        1,
        "Expected retry to execute the tool call instead of prematurely ending turn"
    );
    assert_eq!(turn.tool_results[0].tool_name, "file_search");
    assert!(turn.tool_results[0].success);

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_classify_mandatory_approval_enforces_git_push_pull_and_critical_ops() {
    // Dedicated tools
    assert!(classify_mandatory_approval("git_push", &serde_json::json!({})).is_some());
    assert!(classify_mandatory_approval("git_pull", &serde_json::json!({})).is_some());
    assert!(
        classify_mandatory_approval("create_pr", &serde_json::json!({"title": "feat: x"}))
            .is_some()
    );

    // Shell commands inside exec_cmd
    assert!(classify_mandatory_approval(
        "exec_cmd",
        &serde_json::json!({"command": "git push origin main"})
    )
    .is_some());
    assert!(classify_mandatory_approval(
        "exec_cmd",
        &serde_json::json!({"command": "git status && git pull --rebase origin main"})
    )
    .is_some());
    assert!(classify_mandatory_approval(
        "exec_cmd",
        &serde_json::json!({"command": "git reset --hard HEAD~1"})
    )
    .is_some());
    assert!(classify_mandatory_approval(
        "exec_cmd",
        &serde_json::json!({"command": "git clean -fd"})
    )
    .is_some());
    assert!(classify_mandatory_approval(
        "exec_cmd",
        &serde_json::json!({"command": "gh pr merge 42 --squash"})
    )
    .is_some());
    assert!(classify_mandatory_approval(
        "exec_cmd",
        &serde_json::json!({"command": "npm publish"})
    )
    .is_some());

    // Safe local commands must NOT trigger mandatory approval when auto_approve is true
    assert!(classify_mandatory_approval(
        "exec_cmd",
        &serde_json::json!({"command": "git status && git diff"})
    )
    .is_none());
    assert!(classify_mandatory_approval(
        "exec_cmd",
        &serde_json::json!({"command": "bun run build && bun run lint"})
    )
    .is_none());
    assert!(classify_mandatory_approval(
        "exec_cmd",
        &serde_json::json!({"command": "rm -rf dist node_modules/.vite"})
    )
    .is_none());
}

#[test]
fn test_update_config_refreshes_compactor_and_context_limit() {
    let temp_dir =
        std::env::temp_dir().join(format!("minicode_test_update_cfg_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&temp_dir);

    let mut cfg = Config::default();
    cfg.provider.model = "claude-3-5-sonnet".to_string();
    cfg.provider.context_window = Some(64_000);

    let mut agent = AgentLoop::new(
        &temp_dir,
        cfg.clone(),
        Box::new(crate::agent::mock_provider::MockProvider::new(
            "mock",
            "claude-3-5-sonnet",
        )),
    );
    assert_eq!(agent.compactor.model_token_limit(), 64_000);

    let mut updated_cfg = cfg;
    updated_cfg.provider.model = "gemini-2.5-pro".to_string();
    updated_cfg.provider.context_window = Some(512_000);

    agent.update_config(
        updated_cfg,
        Box::new(crate::agent::mock_provider::MockProvider::new(
            "mock",
            "gemini-2.5-pro",
        )),
    );
    assert_eq!(agent.compactor.model_token_limit(), 512_000);
    assert_eq!(
        crate::context::budget::donut::get_active_context_limit(),
        512_000
    );

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_hydrate_from_events_synthesizes_results_for_interrupted_tool_calls() {
    let temp_dir = std::env::temp_dir().join(format!(
        "minicode_test_hydrate_interrupted_{}",
        std::process::id()
    ));
    let _ = std::fs::create_dir_all(&temp_dir);

    let mut agent = AgentLoop::new(
        &temp_dir,
        Config::default(),
        Box::new(crate::agent::mock_provider::MockProvider::new(
            "mock",
            "mock-model",
        )),
    );

    let events = vec![
        AgentEvent::UserPrompt {
            turn_id: 1,
            prompt: "read two files".to_string(),
            timestamp: String::new(),
        },
        AgentEvent::ToolCall {
            turn_id: 1,
            tool_id: "call_1".to_string(),
            tool: "read_file".to_string(),
            args: serde_json::json!({"path": "a.rs"}),
        },
        AgentEvent::ToolCall {
            turn_id: 1,
            tool_id: "call_2".to_string(),
            tool: "read_file".to_string(),
            args: serde_json::json!({"path": "b.rs"}),
        },
        AgentEvent::ToolResult {
            turn_id: 1,
            tool_id: "call_1".to_string(),
            tool: "read_file".to_string(),
            success: true,
            output: "fn a() {}".to_string(),
            duration_ms: 5,
        },
        // Session crashed before call_2 received a ToolResult, then resumed with Turn 2:
        AgentEvent::UserPrompt {
            turn_id: 2,
            prompt: "continue".to_string(),
            timestamp: String::new(),
        },
        AgentEvent::ToolCall {
            turn_id: 2,
            tool_id: "call_3".to_string(),
            tool: "read_file".to_string(),
            args: serde_json::json!({"path": "c.rs"}),
        },
        // Crash again before call_3 received any ToolResult
    ];

    agent.hydrate_from_events("sess_1", &events);
    let msgs = agent.messages();

    assert_eq!(msgs.len(), 7);
    assert_eq!(msgs[2].tool_call_id.as_deref(), Some("call_1"));
    assert_eq!(msgs[3].tool_call_id.as_deref(), Some("call_2"));
    assert!(msgs[3].content.contains("interrupted"));
    assert_eq!(msgs[6].tool_call_id.as_deref(), Some("call_3"));
    assert!(msgs[6].content.contains("interrupted"));

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_workspace_fingerprint_sync_captures_scaffolded_files_and_respects_gitignore() {
    let temp_dir = std::env::temp_dir().join(format!(
        "minicode_test_ws_fingerprints_{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&temp_dir).unwrap();
    std::fs::write(
        temp_dir.join(".gitignore"),
        "node_modules/\nminikit_docs/\n.minicode/\n",
    )
    .unwrap();

    let mut agent = AgentLoop::new(
        &temp_dir,
        Config::default(),
        Box::new(crate::agent::mock_provider::MockProvider::new(
            "mock",
            "mock-model",
        )),
    );

    // 1. Baseline snapshot before scaffold / external tool execution
    agent.turn_workspace_fingerprints = agent.snapshot_workspace_fingerprints();

    // 2. Simulate `kit_stack_add` / `exec_cmd` scaffolding project files AND `.gitignore`-excluded files
    std::fs::create_dir_all(temp_dir.join("src")).unwrap();
    std::fs::create_dir_all(temp_dir.join("minikit_docs/core")).unwrap();
    std::fs::create_dir_all(temp_dir.join(".minicode/logs")).unwrap();
    std::fs::write(temp_dir.join("package.json"), "{\"name\":\"minitest50\"}").unwrap();
    std::fs::write(temp_dir.join("vite.config.ts"), "export default {};").unwrap();
    std::fs::write(
        temp_dir.join("src/App.tsx"),
        "export const App = () => null;",
    )
    .unwrap();
    std::fs::write(temp_dir.join("minikit_docs/core/prd.md"), "# PRD").unwrap();
    std::fs::write(temp_dir.join(".minicode/logs/run.log"), "ok").unwrap();

    let mut turn_files_modified = Vec::new();
    let changed = agent.sync_workspace_mutations(&mut turn_files_modified);

    assert!(changed.contains(&"package.json".to_string()));
    assert!(changed.contains(&"vite.config.ts".to_string()));
    assert!(changed.contains(&"src/App.tsx".to_string()));
    assert!(turn_files_modified.contains(&"package.json".to_string()));
    assert!(turn_files_modified.contains(&"vite.config.ts".to_string()));
    assert!(turn_files_modified.contains(&"src/App.tsx".to_string()));
    assert!(
        !turn_files_modified
            .iter()
            .any(|p| p.starts_with("minikit_docs")),
        "Gitignored `minikit_docs/` files must not pollute turn_files_modified"
    );
    assert!(
        !turn_files_modified
            .iter()
            .any(|p| p.starts_with(".minicode")),
        "Internal `.minicode/` files must not pollute turn_files_modified"
    );

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[tokio::test]
async fn test_activate_tools_query_narrows_selection_and_active_plan_promotes_closeout_tools() {
    struct CapturingActivateProvider {
        call: ToolCall,
        spent: std::sync::atomic::AtomicBool,
        second_iter_tools: Arc<Mutex<Vec<String>>>,
    }

    #[async_trait]
    impl Provider for CapturingActivateProvider {
        fn name(&self) -> &str {
            "capturing-activate-provider"
        }
        fn default_model(&self) -> &str {
            "test-model"
        }
        async fn stream_completion(
            &self,
            _messages: &[Message],
            tools: &[ToolSchema],
            _options: &CompletionOptions,
        ) -> Result<ChunkStream> {
            if self.spent.swap(true, std::sync::atomic::Ordering::SeqCst) {
                *self.second_iter_tools.lock().unwrap() =
                    tools.iter().map(|t| t.name.clone()).collect();
                let stream = tokio_stream::iter(vec![Ok(StreamChunk::Done)]);
                return Ok(Box::pin(stream));
            }
            let stream = tokio_stream::iter(vec![
                Ok(StreamChunk::ToolCallChunk(self.call.clone())),
                Ok(StreamChunk::Done),
            ]);
            Ok(Box::pin(stream))
        }
    }

    let temp_dir = std::env::temp_dir().join(format!(
        "minicode_test_activate_narrow_{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&temp_dir).unwrap();

    let captured_tools = Arc::new(Mutex::new(Vec::new()));
    let provider = Box::new(CapturingActivateProvider {
        call: ToolCall {
            id: "call_act_1".into(),
            name: "activate_tools".into(),
            arguments: serde_json::json!({
                "category": "memory",
                "query": "archive_plan",
                "reason": "close completed plan"
            }),
        },
        spent: std::sync::atomic::AtomicBool::new(false),
        second_iter_tools: Arc::clone(&captured_tools),
    });

    let mut agent = AgentLoop::new(&temp_dir, Config::default(), provider);
    let (tx, _rx) = mpsc::unbounded_channel();
    let turn = agent
        .execute_turn("continue", tx, None)
        .await
        .expect("turn succeeds");
    assert!(turn.tool_results[0].success);

    // `query: "archive_plan"` on a `"continue"` turn MUST activate individual `archive_plan` (~11 tools),
    // NOT flood all 43 `Memory` category schemas (~53 tools)!
    let post_activate_names = captured_tools.lock().unwrap().clone();
    assert!(post_activate_names.iter().any(|n| n == "archive_plan"));
    assert!(
        post_activate_names.iter().any(|n| n == "kit_stack_add"),
        "Empty greenfield workspace should promote kit_stack_add without requiring activate_tools(minikit)"
    );
    assert!(
        !post_activate_names.iter().any(|n| n == "wiki_write"),
        "Passing `query` to `activate_tools` must not flood the prompt with unrelated Memory category schemas: {:?}",
        post_activate_names
    );
    assert!(
        post_activate_names.len() <= 16,
        "Expected lean core + narrowed query tool count (<= 16), got {} tools: {:?}",
        post_activate_names.len(),
        post_activate_names
    );

    // Initializing an active plan on disk automatically promotes closeout & verification tools (`archive_plan`, `power_verify`, `power_review`)
    let wm = crate::context::memory::working_memory::WorkingMemory::new(&temp_dir);
    wm.init_plan("Ship Hero", &["Build Hero.tsx".to_string()])
        .unwrap();

    let empty_cats = std::collections::HashSet::new();
    let empty_mcp = std::collections::HashMap::new();
    let empty_servers = std::collections::HashSet::new();
    let empty_names = std::collections::HashSet::new();
    let rebuilt = agent.rebuild_active_tools(
        "continue building Hero.tsx",
        &empty_cats,
        &empty_mcp,
        &empty_servers,
        &empty_names,
    );
    let rebuilt_names: Vec<&str> = rebuilt.iter().map(|t| t.name.as_str()).collect();
    assert!(rebuilt_names.contains(&"archive_plan"));
    assert!(rebuilt_names.contains(&"power_verify"));
    assert!(rebuilt_names.contains(&"power_review"));

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[tokio::test]
async fn test_session_activated_categories_persist_across_continue_turns() {
    use async_trait::async_trait;
    use std::sync::{Arc, Mutex};

    struct TwoTurnToolCaptureProvider {
        turn_counter: std::sync::atomic::AtomicUsize,
        turn2_initial_tools: Arc<Mutex<Vec<String>>>,
    }

    #[async_trait]
    impl Provider for TwoTurnToolCaptureProvider {
        fn name(&self) -> &str {
            "two-turn-tool-capture"
        }
        fn default_model(&self) -> &str {
            "test-model"
        }
        async fn stream_completion(
            &self,
            _messages: &[Message],
            tools: &[ToolSchema],
            _options: &CompletionOptions,
        ) -> Result<ChunkStream> {
            let call_idx = self
                .turn_counter
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            match call_idx {
                0 => {
                    // Turn 1 iter 1: activate `files` and `web`
                    let tc = ToolCall {
                        id: "act_1".into(),
                        name: "activate_tools".into(),
                        arguments: serde_json::json!({
                            "category": "files,web",
                            "reason": "multi-file read and browser QA"
                        }),
                    };
                    Ok(Box::pin(tokio_stream::iter(vec![
                        Ok(StreamChunk::ToolCallChunk(tc)),
                        Ok(StreamChunk::Done),
                    ])))
                }
                1 => {
                    // Turn 1 iter 2: finish Turn 1
                    Ok(Box::pin(tokio_stream::iter(vec![Ok(StreamChunk::Done)])))
                }
                _ => {
                    // Turn 2 iter 1 (`"continue"`): inspect active tool schemas immediately at turn start
                    *self.turn2_initial_tools.lock().unwrap() =
                        tools.iter().map(|t| t.name.clone()).collect();
                    Ok(Box::pin(tokio_stream::iter(vec![Ok(StreamChunk::Done)])))
                }
            }
        }
    }

    let temp_dir = std::env::temp_dir().join(format!(
        "minicode_test_session_tools_{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&temp_dir).unwrap();
    let wm = crate::context::memory::working_memory::WorkingMemory::new(&temp_dir);
    wm.init_plan("Landing Page", &["Build HomePage.tsx".to_string()])
        .unwrap();

    let turn2_tools = Arc::new(Mutex::new(Vec::new()));
    let provider = Box::new(TwoTurnToolCaptureProvider {
        turn_counter: std::sync::atomic::AtomicUsize::new(0),
        turn2_initial_tools: Arc::clone(&turn2_tools),
    });

    let mut agent = AgentLoop::new(&temp_dir, Config::default(), provider);
    let (tx1, _rx1) = mpsc::unbounded_channel();
    agent
        .execute_turn("build landing page", tx1, None)
        .await
        .expect("turn 1 succeeds");

    let (tx2, _rx2) = mpsc::unbounded_channel();
    agent
        .execute_turn("continue", tx2, None)
        .await
        .expect("turn 2 succeeds");

    let names = turn2_tools.lock().unwrap().clone();
    assert!(
        names
            .iter()
            .any(|n| n == "batch_read" && names.iter().any(|m| m == "replace_in_files")),
        "Turn 2 (`continue`) must retain `files` category tools activated in Turn 1: {:?}",
        names
    );
    assert!(
        names.iter().any(|n| n == "browser_qa_audit"),
        "Turn 2 (`continue`) must retain `web` category tools activated in Turn 1: {:?}",
        names
    );

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[tokio::test]
async fn test_minitest62_async_background_yield_and_kill_guard_protection() {
    use async_trait::async_trait;

    // 1. Verify deterministic kill guard on `MiniDevRegistry`
    let reg = crate::dev::registry::MiniDevRegistry::new();
    reg.record_user_prompt("so whats happening", true);
    assert!(
        reg.check_background_kill_guard("worker-reviewer-1", false)
            .is_some(),
        "Casual status question MUST block killing active MiniPub/minitask worker"
    );
    assert!(
        reg.check_background_kill_guard("frontend-dev", true)
            .is_some(),
        "Casual status question while daemon is running MUST block stopping the daemon"
    );

    // Agent-initiated daemon teardown during normal build turn (`had_active_background = false`) is allowed
    reg.record_user_prompt("build a landing page", false);
    assert!(
        reg.check_background_kill_guard("frontend-dev", true)
            .is_none(),
        "Agent-owned daemon teardown during normal build turn should be permitted"
    );
    assert!(
        reg.check_background_kill_guard("cargo-build-1", false)
            .is_some(),
        "Finite background build task still protected unless user asked to stop/cancel"
    );

    // Explicit termination intent from user permits killing immediately
    reg.record_user_prompt("please stop the running build", true);
    assert!(reg
        .check_background_kill_guard("cargo-build-1", false)
        .is_none());
    reg.clear_recorded_user_prompt();

    // 2. Verify `AgentLoop` with `set_async_idle_wakeup_enabled(true)` yields promptly without
    //    blocking while a background `minitask` job runs, and answering a status prompt keeps it alive!
    struct QuickTextProvider;
    #[async_trait]
    impl Provider for QuickTextProvider {
        fn name(&self) -> &str {
            "quick-text"
        }
        fn default_model(&self) -> &str {
            "test-model"
        }
        async fn stream_completion(
            &self,
            _messages: &[Message],
            _tools: &[ToolSchema],
            _options: &CompletionOptions,
        ) -> Result<ChunkStream> {
            Ok(Box::pin(tokio_stream::iter(vec![
                Ok(StreamChunk::Delta(
                    "Background build is running in minitask.".into(),
                )),
                Ok(StreamChunk::Done),
            ])))
        }
    }

    let temp_dir = std::env::temp_dir().join(format!(
        "minicode_test_async_yield_{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&temp_dir).unwrap();

    let global_reg = crate::dev::registry::get_global_dev_registry();
    let req = crate::dev::models::SpawnDevRequest {
        command: "sleep 3 && echo async_done".to_string(),
        name: Some("test-async-cargo-build".to_string()),
        process_type: crate::dev::models::DevProcessType::Script,
        working_dir: None,
        extra_env: std::collections::HashMap::new(),
        port_hint: None,
        max_memory_mb: None,
        port_policy: None,
        restart_policy: Some(crate::dev::models::RestartPolicy::Never),
    };
    let (task_summary, handle) = global_reg
        .spawn_with_options(&temp_dir, req, false)
        .await
        .expect("spawn_with_options");

    let mut agent = AgentLoop::new(&temp_dir, Config::default(), Box::new(QuickTextProvider));
    agent.set_async_idle_wakeup_enabled(true);

    let start = std::time::Instant::now();
    let (tx, _rx) = mpsc::unbounded_channel();
    agent
        .execute_turn("so whats happening", tx, None)
        .await
        .expect("async status turn succeeds without blocking");
    let elapsed = start.elapsed();

    assert!(
        elapsed < std::time::Duration::from_millis(1800),
        "Async TUI turn must yield in <1.8s instead of blocking on 3s background task (took {:?})",
        elapsed
    );
    assert!(
        global_reg.is_continuation_armed(),
        "Continuation wakeup must remain armed after async yield"
    );
    assert!(
        handle.status.read().await.is_alive(),
        "Answering user status prompt MUST NOT stop or kill the running background task"
    );

    // Also verify TurnPromptGuard cleared recorded prompt automatically upon execute_turn exit
    assert!(
        global_reg
            .check_background_kill_guard("test-async-cargo-build", false)
            .is_none(),
        "TurnPromptGuard must clear recorded user prompt once execute_turn finishes"
    );

    // Clean up background test task & global flag
    global_reg.set_continuation_armed(false);
    let _ = global_reg.stop(&task_summary.id).await;
    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[tokio::test]
async fn test_algorithmic_polarity_epoch_universal_async_and_pagerank_reviewer() {
    // 1. Clause-scoped negation, interrogative, and contrastive target polarity analysis
    assert!(
        crate::dev::registry::MiniDevRegistry::analyze_termination_polarity(
            "don't stop the build, tell me what's happening",
            Some("cargo-build-1")
        ) <= 0,
        "Negated termination must NOT authorize killing a background task"
    );
    assert_eq!(
        crate::dev::registry::MiniDevRegistry::analyze_termination_polarity(
            "why did the build stop earlier?",
            Some("cargo-build-1")
        ),
        0,
        "Interrogative question must NOT authorize killing a background task"
    );
    assert!(
        crate::dev::registry::MiniDevRegistry::analyze_termination_polarity(
            "leave vite running, but kill reviewer-1",
            Some("vite-dev (dev-1)")
        ) <= 0,
        "Contrastive preservation clause must protect `vite`"
    );
    assert!(
        crate::dev::registry::MiniDevRegistry::analyze_termination_polarity(
            "leave vite running, but kill reviewer-1",
            Some("reviewer (reviewer-1)")
        ) > 0,
        "Contrastive termination clause must authorize stopping `reviewer-1`"
    );

    // 2. Monotonic TurnEpoch ownership state machine
    let reg = crate::dev::registry::MiniDevRegistry::new();
    reg.set_orchestrator_turn_active(true);
    let e1 = reg.advance_turn_epoch();
    reg.record_user_prompt("build the landing page and verify it", false);
    let epoch_proc_id = crate::dev::models::DevProcessId::new(Some("vite-epoch-test"));
    reg.record_spawn_epoch(&epoch_proc_id);

    // In the SAME turn epoch (`e1`) where `vite-epoch-test` was spawned, normal QA cleanup is allowed:
    assert!(reg
        .check_background_kill_guard_for_id(Some(&epoch_proc_id), "vite-dev", true)
        .is_none());

    // Once a NEW user turn advances the epoch (`e2 = e1 + 1`), killing the earlier-epoch daemon
    // without user authorization is strictly BLOCKED by `TurnEpoch` ownership even when `had_active_background=false`:
    let e2 = reg.advance_turn_epoch();
    assert_eq!(e2, e1 + 1);
    reg.record_user_prompt("add a pricing section", false);
    assert!(reg
        .check_background_kill_guard_for_id(Some(&epoch_proc_id), "vite-dev", true)
        .is_some());
    reg.clear_recorded_user_prompt();
    reg.set_orchestrator_turn_active(false);

    // 3. Universal deadline-based async auto-promotion + ephemeral fast-command pruning
    assert_eq!(
        crate::tools::exec::derive_dynamic_command_slug("./scripts/generate_assets.sh --prod"),
        "generate-assets"
    );
    assert_eq!(
        crate::tools::exec::derive_dynamic_command_slug("wasm-pack build --target web"),
        "wasm-pack-build"
    );

    let ws = std::env::temp_dir().join(format!(
        "minicode_test_algo_upgrades_{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(ws.join("src/core")).unwrap();
    let global_reg = crate::dev::registry::get_global_dev_registry();
    global_reg.set_async_idle_wakeup_enabled(true);

    // 3a. Fast unrecognized command (`echo`) in async TUI mode runs synchronously and prunes itself
    let before_ids: std::collections::HashSet<crate::dev::models::DevProcessId> =
        global_reg.list().await.into_iter().map(|p| p.id).collect();
    let fast_out = crate::tools::exec::exec_cmd(&ws, "echo universal_ephemeral_ok", Some(2)).await;
    assert!(fast_out.is_ok(), "fast_out: {:?}", fast_out);
    assert!(fast_out.unwrap().contains("universal_ephemeral_ok"));
    let after_ids: std::collections::HashSet<crate::dev::models::DevProcessId> =
        global_reg.list().await.into_iter().map(|p| p.id).collect();
    let leaked: Vec<_> = after_ids.difference(&before_ids).collect();
    assert!(
        leaked.is_empty(),
        "Fast ephemeral commands must prune cleanly from MiniDevRegistry, leaked: {:?}",
        leaked
    );

    // 3b. Slow unrecognized command (`sleep 0.6`) exceeding `wait_ms = 100` auto-promotes to `minitask`
    //     and sets structured `is_command_currently_detached` until exit!
    let slow_cmd = "sleep 0.6";
    let slow_out = crate::tools::exec::exec_managed_background_task(&ws, slow_cmd, 100, None)
        .await
        .expect("slow_out");
    assert!(
        global_reg.is_command_currently_detached(slow_cmd),
        "Detached command must be tracked structurally in MiniDevRegistry, out:\n{}",
        slow_out
    );
    tokio::time::sleep(std::time::Duration::from_millis(850)).await;
    assert!(
        !global_reg.is_command_currently_detached(slow_cmd),
        "Structured detach state must clear automatically once background process exits"
    );
    global_reg.set_async_idle_wakeup_enabled(false);
    global_reg.set_continuation_armed(false);
    let _ = global_reg.harvest_newly_completed_tasks_block(&ws).await;

    // 4. `CodeGraph` Personalized PageRank + `WorkingMemory` scaffold-baseline diff for Reviewer
    std::fs::create_dir_all(ws.join("src/components")).unwrap();
    let starter_navbar = crate::tools::minikit::scaffolder::MiniKitScaffolder::get_all_stacks()
        .into_iter()
        .find(|s| s.name == "react-vite")
        .and_then(|s| {
            s.files
                .into_iter()
                .find(|f| f.path == "src/components/Navbar.tsx")
        })
        .expect("react-vite Navbar.tsx template");
    std::fs::write(
        ws.join("src/components/Navbar.tsx"),
        &starter_navbar.content,
    )
    .unwrap();

    // Create authored AST modules with an import/symbol edge + a 70 KiB generated lock artifact
    std::fs::write(
        ws.join("src/core/Engine.ts"),
        "export function runCoreEngine(): string { return 'ok'; }\n",
    )
    .unwrap();
    std::fs::write(
        ws.join("src/App.tsx"),
        "import { runCoreEngine } from './core/Engine';\nexport function App() { return runCoreEngine(); }\n",
    )
    .unwrap();
    std::fs::write(ws.join("generated.blob"), "x".repeat(70_000)).unwrap();

    let ranked = crate::tools::registry::minipower_tools::rank_reviewer_workspace_files(&ws, 10);
    assert!(
        ranked.contains(&"src/core/Engine.ts".to_string())
            && ranked.contains(&"src/App.tsx".to_string()),
        "Expected authored AST files in top reviewer candidates, got: {:?}",
        ranked
    );
    assert!(
        !ranked.contains(&"generated.blob".to_string()),
        "Oversized generated blobs/lockfiles (>64 KiB) must not crowd reviewer candidates: {:?}",
        ranked
    );
    // Authored files (`App.tsx`, `Engine.ts`) must rank BEFORE unmodified scaffold baseline (`src/components/Navbar.tsx`)
    let authored_pos = ranked
        .iter()
        .position(|p| p == "src/core/Engine.ts")
        .unwrap();
    if let Some(scaf_pos) = ranked.iter().position(|p| p == "src/components/Navbar.tsx") {
        assert!(
            authored_pos < scaf_pos,
            "Authored files must outrank unmodified scaffold baseline files: {:?}",
            ranked
        );
    }

    let _ = std::fs::remove_dir_all(&ws);
}

use crate::agent::provider::{ChunkStream, CompletionOptions, Provider, StreamChunk, ToolSchema};
use crate::agent::types::{Message, ToolCall};
use crate::error::{ProviderError, Result};
use async_stream::stream;
use async_trait::async_trait;
use std::sync::{Arc, Mutex};

/// A single scripted turn response for the MockProvider
#[derive(Debug, Clone)]
pub struct MockTurn {
    #[allow(dead_code)]
    pub chunks: Vec<StreamChunk>,
    #[allow(dead_code)]
    pub should_fail: Option<String>,
}

/// In-memory deterministic LLM provider for offline testing and replay simulations
#[allow(dead_code)]
pub struct MockProvider {
    pub name: String,
    pub default_model: String,
    turns: Arc<Mutex<Vec<MockTurn>>>,
    received_requests: Arc<Mutex<Vec<Vec<Message>>>>,
}

#[allow(dead_code)]
impl MockProvider {
    pub fn new(name: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            default_model: model.into(),
            turns: Arc::new(Mutex::new(Vec::new())),
            received_requests: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Appends a scripted turn with text deltas and tool calls
    pub fn push_response(&self, text_deltas: &[&str], tool_calls: Vec<ToolCall>) {
        let mut chunks = Vec::new();
        for delta in text_deltas {
            chunks.push(StreamChunk::Delta(delta.to_string()));
        }
        for tool_call in tool_calls {
            chunks.push(StreamChunk::ToolCallChunk(tool_call));
        }
        chunks.push(StreamChunk::Usage {
            prompt_tokens: 150,
            completion_tokens: 50,
        });
        chunks.push(StreamChunk::Done);

        let mut turns = self.turns.lock().unwrap_or_else(|e| e.into_inner());
        turns.push(MockTurn {
            chunks,
            should_fail: None,
        });
    }

    /// Appends a simulated failure turn
    pub fn push_error(&self, error_msg: impl Into<String>) {
        let mut turns = self.turns.lock().unwrap_or_else(|e| e.into_inner());
        turns.push(MockTurn {
            chunks: Vec::new(),
            should_fail: Some(error_msg.into()),
        });
    }

    /// Retrieves all conversational request message histories sent to this provider
    pub fn get_received_requests(&self) -> Vec<Vec<Message>> {
        self.received_requests
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Clears any remaining queued turns
    pub fn clear_turns(&self) {
        let mut turns = self.turns.lock().unwrap_or_else(|e| e.into_inner());
        turns.clear();
    }
}

#[async_trait]
impl Provider for MockProvider {
    fn name(&self) -> &str {
        &self.name
    }

    fn default_model(&self) -> &str {
        &self.default_model
    }

    async fn stream_completion(
        &self,
        messages: &[Message],
        _tools: &[ToolSchema],
        _options: &CompletionOptions,
    ) -> Result<ChunkStream> {
        // Record incoming messages
        {
            let mut reqs = self
                .received_requests
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            reqs.push(messages.to_vec());
        }

        // Pop next scripted turn
        let next_turn = {
            let mut turns = self.turns.lock().unwrap_or_else(|e| e.into_inner());
            if turns.is_empty() {
                return Err(ProviderError::StreamDecode(
                    "MockProvider exhausted: no more scripted turns available".to_string(),
                )
                .into());
            }
            turns.remove(0)
        };

        if let Some(err_msg) = next_turn.should_fail {
            return Err(ProviderError::Api {
                status: 500,
                message: err_msg,
            }
            .into());
        }

        let chunks = next_turn.chunks;
        let s = stream! {
            for chunk in chunks {
                yield Ok(chunk);
            }
        };

        Ok(Box::pin(s))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;

    #[tokio::test]
    async fn test_mock_provider_streaming() {
        let provider = MockProvider::new("mock", "mock-model-v1");
        provider.push_response(&["Hello ", "world!"], vec![]);

        let messages = vec![Message::user("Hi")];
        let options = CompletionOptions {
            model: "mock-model-v1".to_string(),
            temperature: 0.2,
            max_tokens: 1000,
            system_instruction: None,
        };

        let mut stream = provider
            .stream_completion(&messages, &[], &options)
            .await
            .unwrap();

        let mut deltas = Vec::new();
        while let Some(chunk_res) = stream.next().await {
            let chunk = chunk_res.unwrap();
            if let StreamChunk::Delta(d) = chunk {
                deltas.push(d);
            }
        }

        assert_eq!(deltas, vec!["Hello ", "world!"]);
        assert_eq!(provider.get_received_requests().len(), 1);
        assert_eq!(provider.get_received_requests()[0][0].content, "Hi");
    }

    #[tokio::test]
    async fn test_mock_provider_error_injection() {
        let provider = MockProvider::new("mock", "mock-model-v1");
        provider.push_error("Rate limit exceeded");

        let messages = vec![Message::user("Hi")];
        let options = CompletionOptions {
            model: "mock-model-v1".to_string(),
            temperature: 0.2,
            max_tokens: 1000,
            system_instruction: None,
        };

        let res = provider.stream_completion(&messages, &[], &options).await;
        assert!(res.is_err());
    }
}

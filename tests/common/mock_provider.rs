use async_trait::async_trait;
use minicode::agent::provider::{
    ChunkStream, CompletionOptions, Provider, StreamChunk, ToolSchema,
};
use minicode::agent::types::{Message, ToolCall};
use minicode::error::Result;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

/// A scripted response to yield from MockProvider during a turn
#[derive(Debug, Clone)]
pub struct MockResponse {
    pub text: String,
    pub tool_calls: Vec<ToolCall>,
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
}

impl MockResponse {
    pub fn text_only(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            tool_calls: Vec::new(),
            prompt_tokens: 100,
            completion_tokens: 50,
        }
    }

    pub fn with_tool_call(tool_id: &str, tool_name: &str, args: serde_json::Value) -> Self {
        Self {
            text: String::new(),
            tool_calls: vec![ToolCall {
                id: tool_id.to_string(),
                name: tool_name.to_string(),
                arguments: args,
            }],
            prompt_tokens: 120,
            completion_tokens: 30,
        }
    }
}

/// In-memory scripted MockProvider for deterministic agent loop testing
#[derive(Clone)]
pub struct MockProvider {
    responses: Arc<Mutex<VecDeque<MockResponse>>>,
}

impl MockProvider {
    pub fn new(responses: Vec<MockResponse>) -> Self {
        Self {
            responses: Arc::new(Mutex::new(responses.into())),
        }
    }
}

#[async_trait]
impl Provider for MockProvider {
    fn name(&self) -> &str {
        "mock"
    }

    fn default_model(&self) -> &str {
        "mock-model"
    }

    async fn stream_completion(
        &self,
        _messages: &[Message],
        _tools: &[ToolSchema],
        _options: &CompletionOptions,
    ) -> Result<ChunkStream> {
        let next_resp = {
            let mut guard = self.responses.lock().unwrap();
            guard.pop_front()
        };

        let response = match next_resp {
            Some(r) => r,
            None => {
                return Err(minicode::error::ProviderError::Api {
                    status: 500,
                    message: "MockProvider: No scripted response left in queue".to_string(),
                }
                .into());
            }
        };

        let mut chunks: Vec<Result<StreamChunk>> = Vec::new();

        if !response.text.is_empty() {
            chunks.push(Ok(StreamChunk::Delta(response.text)));
        }

        for tc in response.tool_calls {
            chunks.push(Ok(StreamChunk::ToolCallChunk(tc)));
        }

        chunks.push(Ok(StreamChunk::Usage {
            prompt_tokens: response.prompt_tokens,
            completion_tokens: response.completion_tokens,
        }));
        chunks.push(Ok(StreamChunk::Done));

        let stream = futures::stream::iter(chunks);
        Ok(Box::pin(stream))
    }
}

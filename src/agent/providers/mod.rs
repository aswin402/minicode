//! Universal Provider trait and LLM implementations modularized into submodules.

pub mod anthropic;
pub mod factory;
pub mod gemini;
pub mod openai;
pub mod resilient;
pub mod unconfigured;

#[allow(unused_imports)]
pub use anthropic::AnthropicProvider;
#[allow(unused_imports)]
pub use factory::{create_provider, create_provider_or_fallback, create_provider_with_base_url};
#[allow(unused_imports)]
pub use gemini::GeminiProvider;
#[allow(unused_imports)]
pub use openai::OpenAiCompatibleProvider;
#[allow(unused_imports)]
pub use resilient::ResilientProvider;
#[allow(unused_imports)]
pub use unconfigured::UnconfiguredProvider;

use crate::agent::types::{Message, ToolCall};
use crate::error::Result;
use async_trait::async_trait;
use futures::Stream;
use serde::{Deserialize, Serialize};
use std::pin::Pin;
use tokio_stream::StreamExt;

/// Standard JSON schema tool definition, provider-agnostic
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolSchema {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct CompletionOptions {
    pub model: String,
    pub temperature: f32,
    pub max_tokens: usize,
    pub system_instruction: Option<String>,
    pub thinking_budget: Option<usize>,
    pub reasoning_effort: Option<String>,
}

impl Default for CompletionOptions {
    fn default() -> Self {
        Self {
            model: String::new(),
            temperature: 0.2,
            max_tokens: 8192,
            system_instruction: None,
            thinking_budget: None,
            reasoning_effort: None,
        }
    }
}

#[allow(dead_code)]
impl CompletionOptions {
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            temperature: 0.2,
            max_tokens: 8192,
            system_instruction: None,
            thinking_budget: None,
            reasoning_effort: None,
        }
    }

    pub fn with_thinking(mut self, budget: usize) -> Self {
        self.thinking_budget = Some(budget);
        self
    }

    pub fn with_reasoning_effort(mut self, effort: impl Into<String>) -> Self {
        self.reasoning_effort = Some(effort.into());
        self
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum StreamChunk {
    Delta(String),
    ToolCallChunk(ToolCall),
    Usage {
        prompt_tokens: usize,
        completion_tokens: usize,
    },
    Done,
}

pub type ChunkStream = Pin<Box<dyn Stream<Item = Result<StreamChunk>> + Send>>;

/// Universal trait for LLM inference providers (Gemini, Claude, OpenAI, OpenRouter, Ollama)
#[async_trait]
pub trait Provider: Send + Sync {
    #[allow(dead_code)]
    fn name(&self) -> &str;
    fn default_model(&self) -> &str;

    async fn stream_completion(
        &self,
        messages: &[Message],
        tools: &[ToolSchema],
        options: &CompletionOptions,
    ) -> Result<ChunkStream>;

    /// Non-streaming completion: returns the full response text as a single string.
    /// Default implementation wraps `stream_completion` and collects all deltas.
    #[allow(dead_code)]
    async fn completion(
        &self,
        messages: &[Message],
        tools: &[ToolSchema],
        options: &CompletionOptions,
    ) -> Result<String> {
        let mut stream = self.stream_completion(messages, tools, options).await?;
        let mut text = String::new();
        while let Some(chunk) = stream.next().await {
            match chunk {
                Ok(StreamChunk::Delta(delta)) => text.push_str(&delta),
                Ok(
                    StreamChunk::ToolCallChunk(_) | StreamChunk::Usage { .. } | StreamChunk::Done,
                ) => {}
                Err(e) => {
                    let err_str = e.to_string().to_lowercase();
                    if !text.is_empty() && err_str.contains("stream ended") {
                        break;
                    }
                    return Err(e);
                }
            }
        }
        Ok(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::types::{Message, Role};

    #[test]
    fn test_gemini_format_merges_consecutive_tool_messages() {
        let messages = vec![
            Message::user("Please read two files"),
            Message {
                role: Role::Assistant,
                content: "".to_string(),
                tool_calls: Some(vec![
                    crate::agent::types::ToolCall {
                        id: "call_1".to_string(),
                        name: "read_file".to_string(),
                        arguments: serde_json::json!({"path": "src/main.rs"}),
                    },
                    crate::agent::types::ToolCall {
                        id: "call_2".to_string(),
                        name: "read_file".to_string(),
                        arguments: serde_json::json!({"path": "src/lib.rs"}),
                    },
                ]),
                tool_call_id: None,
                tool_name: None,
            },
            Message {
                role: Role::Tool,
                content: "fn main() {}".to_string(),
                tool_calls: None,
                tool_call_id: Some("call_1".to_string()),
                tool_name: Some("read_file".to_string()),
            },
            Message {
                role: Role::Tool,
                content: "pub mod agent;".to_string(),
                tool_calls: None,
                tool_call_id: Some("call_2".to_string()),
                tool_name: Some("read_file".to_string()),
            },
        ];

        let formatted = GeminiProvider::format_contents(&messages);
        // Expect exactly 3 content items: 1 user prompt, 1 model with 2 functionCalls, 1 user with 2 functionResponses merged
        assert_eq!(formatted.len(), 3);
        assert_eq!(formatted[0]["role"], "user");
        assert_eq!(formatted[1]["role"], "model");
        assert_eq!(formatted[1]["parts"].as_array().unwrap().len(), 2);
        assert_eq!(formatted[2]["role"], "user");
        let tool_parts = formatted[2]["parts"].as_array().unwrap();
        assert_eq!(tool_parts.len(), 2);
        assert_eq!(tool_parts[0]["functionResponse"]["name"], "read_file");
        assert_eq!(tool_parts[0]["functionResponse"]["id"], "call_1");
        assert_eq!(tool_parts[1]["functionResponse"]["name"], "read_file");
        assert_eq!(tool_parts[1]["functionResponse"]["id"], "call_2");
    }
}

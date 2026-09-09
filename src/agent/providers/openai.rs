use crate::agent::providers::{ChunkStream, CompletionOptions, Provider, StreamChunk, ToolSchema};
use crate::agent::types::{Message, Role, ToolCall};
use crate::error::{ProviderError, Result};
use async_trait::async_trait;
use reqwest_eventsource::{Event, EventSource};
use tokio_stream::StreamExt;

/// OpenAI & OpenRouter compatible Provider (DeepSeek, Groq, Together, Local vLLM, OpenAI, OpenRouter)
pub struct OpenAiCompatibleProvider {
    provider_name: String,
    api_key: String,
    base_url: String,
    default_model_name: String,
    client: reqwest::Client,
}

impl OpenAiCompatibleProvider {
    pub fn new(
        provider_name: impl Into<String>,
        api_key: impl Into<String>,
        base_url: impl Into<String>,
        default_model: impl Into<String>,
    ) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(
                crate::constants::PROVIDER_STREAM_TIMEOUT_SECS,
            ))
            .build()
            .unwrap_or_default();

        Self {
            provider_name: provider_name.into(),
            api_key: api_key.into(),
            base_url: base_url.into(),
            default_model_name: default_model.into(),
            client,
        }
    }

    pub fn openrouter(api_key: impl Into<String>) -> Self {
        Self::new(
            "openrouter",
            api_key,
            crate::constants::OPENROUTER_BASE_URL,
            crate::constants::OPENROUTER_DEFAULT_MODEL,
        )
    }

    pub fn openai(api_key: impl Into<String>) -> Self {
        let base_url = std::env::var("OPENAI_BASE_URL")
            .unwrap_or_else(|_| crate::constants::OPENAI_DEFAULT_BASE_URL.to_string());
        Self::new(
            "openai",
            api_key,
            base_url,
            crate::constants::OPENAI_DEFAULT_MODEL,
        )
    }

    fn format_messages(
        messages: &[Message],
        system_instruction: Option<&str>,
    ) -> Vec<serde_json::Value> {
        let mut formatted = Vec::new();

        if let Some(sys) = system_instruction {
            formatted.push(serde_json::json!({
                "role": "system",
                "content": sys
            }));
        }

        for msg in messages {
            match msg.role {
                Role::System => {
                    formatted.push(serde_json::json!({
                        "role": "system",
                        "content": msg.content
                    }));
                }
                Role::User => {
                    formatted.push(serde_json::json!({
                        "role": "user",
                        "content": msg.content
                    }));
                }
                Role::Assistant => {
                    if let Some(ref tool_calls) = msg.tool_calls {
                        let calls: Vec<serde_json::Value> = tool_calls
                            .iter()
                            .map(|tc| {
                                serde_json::json!({
                                    "id": tc.id,
                                    "type": "function",
                                    "function": {
                                        "name": tc.name,
                                        "arguments": tc.arguments.to_string()
                                    }
                                })
                            })
                            .collect();

                        formatted.push(serde_json::json!({
                            "role": "assistant",
                            "content": if msg.content.is_empty() { serde_json::Value::Null } else { serde_json::Value::String(msg.content.clone()) },
                            "tool_calls": calls
                        }));
                    } else {
                        formatted.push(serde_json::json!({
                            "role": "assistant",
                            "content": msg.content
                        }));
                    }
                }
                Role::Tool => {
                    formatted.push(serde_json::json!({
                        "role": "tool",
                        "tool_call_id": msg.tool_call_id.as_deref().unwrap_or("call_1"),
                        "content": msg.content
                    }));
                }
            }
        }

        formatted
    }

    fn format_tools(tools: &[ToolSchema]) -> Option<Vec<serde_json::Value>> {
        if tools.is_empty() {
            return None;
        }

        let formatted = tools
            .iter()
            .map(|t| {
                serde_json::json!({
                    "type": "function",
                    "function": {
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.parameters
                    }
                })
            })
            .collect();

        Some(formatted)
    }

    /// Builds request JSON body for OpenAI-compatible endpoints with test-time reasoning support
    pub fn build_request_body(
        provider_name: &str,
        messages: &[Message],
        tools: &[ToolSchema],
        options: &CompletionOptions,
        default_model: &str,
    ) -> serde_json::Value {
        let model = if options.model.is_empty() {
            default_model
        } else {
            &options.model
        };

        let is_reasoning_model = model.starts_with("o1")
            || model.starts_with("o3")
            || model.contains("reasoning")
            || model.contains("reasoner")
            || model.contains("r1");

        let formatted_messages =
            Self::format_messages(messages, options.system_instruction.as_deref());

        let mut request_body = serde_json::json!({
            "model": model,
            "messages": formatted_messages,
            "stream": true,
        });

        if !is_reasoning_model {
            request_body["temperature"] = serde_json::json!(options.temperature);
        }

        if model.starts_with("o1") || model.starts_with("o3") {
            request_body["max_completion_tokens"] = serde_json::json!(options.max_tokens);
        } else {
            request_body["max_tokens"] = serde_json::json!(options.max_tokens);
        }

        if let Some(ref effort) = options.reasoning_effort {
            if provider_name == "openrouter" {
                request_body["reasoning"] = serde_json::json!({ "effort": effort });
            } else {
                request_body["reasoning_effort"] = serde_json::json!(effort);
            }
        } else if let Some(budget) = options.thinking_budget {
            if provider_name == "openrouter" {
                if model.contains("claude-3.7") || model.contains("claude-3-7") {
                    request_body["thinking"] = serde_json::json!({
                        "type": "enabled",
                        "budget_tokens": budget
                    });
                    request_body["temperature"] = serde_json::json!(1.0);
                } else {
                    request_body["reasoning"] = serde_json::json!({
                        "max_tokens": budget
                    });
                }
            } else if model.starts_with("o1") || model.starts_with("o3") {
                let effort = if budget <= 4096 {
                    "low"
                } else if budget <= 16000 {
                    "medium"
                } else {
                    "high"
                };
                request_body["reasoning_effort"] = serde_json::json!(effort);
            }
        }

        if let Some(tools_payload) = Self::format_tools(tools) {
            request_body["tools"] = serde_json::Value::Array(tools_payload);
        }

        request_body
    }
}

#[async_trait]
impl Provider for OpenAiCompatibleProvider {
    fn name(&self) -> &str {
        &self.provider_name
    }

    fn default_model(&self) -> &str {
        &self.default_model_name
    }

    async fn stream_completion(
        &self,
        messages: &[Message],
        tools: &[ToolSchema],
        options: &CompletionOptions,
    ) -> Result<ChunkStream> {
        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
        let request_body = Self::build_request_body(
            &self.provider_name,
            messages,
            tools,
            options,
            self.default_model(),
        );

        let mut req_builder = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("Authorization", format!("Bearer {}", self.api_key));

        // OpenRouter custom attribution headers
        if self.provider_name == "openrouter" {
            req_builder = req_builder
                .header("HTTP-Referer", crate::constants::PROJECT_REPO_URL)
                .header("X-Title", "minicode");
        }

        let request = req_builder.json(&request_body);

        // Clone now — both the EventSource closure and the usage-missing log closure need owned data
        let provider_name = self.provider_name.clone();

        let event_source = EventSource::new(request).map_err(|e| {
            ProviderError::StreamDecode(format!("Failed to connect to {}: {}", provider_name, e))
        })?;

        let stream = async_stream::stream! {
            let mut event_source = event_source;
            let mut tool_calls_accumulator: std::collections::BTreeMap<usize, (String, String, String)> =
                std::collections::BTreeMap::new(); // (id, name, args_json)
            let mut in_reasoning_mode = false;

            while let Some(event_res) = event_source.next().await {
                match event_res {
                    Ok(Event::Open) => {
                        tracing::debug!("OpenAI-compatible SSE stream open");
                    }
                    Ok(Event::Message(message)) => {
                        if message.data.trim() == "[DONE]" {
                            if in_reasoning_mode {
                                in_reasoning_mode = false;
                                yield Ok(StreamChunk::Delta("</thought>".to_string()));
                            }
                            // Emit any accumulated tool calls before concluding
                            for (_, (id, name, args_str)) in std::mem::take(&mut tool_calls_accumulator) {
                                let parsed_args = match serde_json::from_str::<serde_json::Value>(&args_str) {
                                    Ok(val) => val,
                                    Err(e) => {
                                        tracing::warn!(raw_arguments = %args_str, error = %e, "Malformed JSON tool call arguments from model");
                                        serde_json::json!({
                                            "__json_parse_error": format!("Invalid JSON syntax generated by model: {}", e),
                                            "__raw": args_str
                                        })
                                    }
                                };
                                yield Ok(StreamChunk::ToolCallChunk(ToolCall {
                                    id,
                                    name,
                                    arguments: parsed_args,
                                }));
                            }
                            yield Ok(StreamChunk::Done);
                            break;
                        }

                        let parsed: Result<serde_json::Value> = serde_json::from_str(&message.data)
                            .map_err(|e| ProviderError::StreamDecode(format!("JSON parse error: {}", e)).into());

                        match parsed {
                            Ok(val) => {
                                if let Some(choices) = val.get("choices").and_then(|c| c.as_array()) {
                                    for choice in choices {
                                        if let Some(delta) = choice.get("delta") {
                                            // 1. Reasoning / Thought Delta (DeepSeek R1, OpenAI o1/o3, MiniMax, etc.)
                                            if let Some(reasoning) = delta.get("reasoning_content").or_else(|| delta.get("reasoning")).and_then(|r| r.as_str()) {
                                                if !reasoning.is_empty() {
                                                    if !in_reasoning_mode {
                                                        in_reasoning_mode = true;
                                                        yield Ok(StreamChunk::Delta("<thought>".to_string()));
                                                    }
                                                    yield Ok(StreamChunk::Delta(reasoning.to_string()));
                                                }
                                            }

                                            // 2. Text Delta
                                            if let Some(content) = delta.get("content").and_then(|c| c.as_str()) {
                                                if !content.is_empty() {
                                                    if in_reasoning_mode {
                                                        in_reasoning_mode = false;
                                                        yield Ok(StreamChunk::Delta("</thought>".to_string()));
                                                    }
                                                    yield Ok(StreamChunk::Delta(content.to_string()));
                                                }
                                            }

                                            // 3. Tool Calls Delta
                                            if let Some(tool_calls) = delta.get("tool_calls").and_then(|tc| tc.as_array()) {
                                                if !tool_calls.is_empty() && in_reasoning_mode {
                                                    in_reasoning_mode = false;
                                                    yield Ok(StreamChunk::Delta("</thought>".to_string()));
                                                }
                                                for tc in tool_calls {
                                                    let index = tc.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
                                                    let entry = tool_calls_accumulator.entry(index).or_insert_with(|| (
                                                        uuid::Uuid::new_v4().to_string(),
                                                        String::new(),
                                                        String::new(),
                                                    ));

                                                    if let Some(id) = tc.get("id").and_then(|id| id.as_str()) {
                                                        entry.0 = id.to_string();
                                                    }
                                                    if let Some(func) = tc.get("function") {
                                                        if let Some(name) = func.get("name").and_then(|n| n.as_str()) {
                                                            if entry.1.is_empty() {
                                                                entry.1 = name.to_string();
                                                            }
                                                        }
                                                        if let Some(args) = func.get("arguments").and_then(|a| a.as_str()) {
                                                            entry.2.push_str(args);
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }

                                if let Some(usage) = val.get("usage") {
                                    let prompt_tokens = usage.get("prompt_tokens").and_then(|t| t.as_u64()).unwrap_or(0) as usize;
                                    let completion_tokens = usage.get("completion_tokens").and_then(|t| t.as_u64()).unwrap_or(0) as usize;
                                    if prompt_tokens > 0 || completion_tokens > 0 {
                                        yield Ok(StreamChunk::Usage { prompt_tokens, completion_tokens });
                                    } else {
                                        tracing::debug!(
                                            provider = provider_name.clone(),
                                            "Usage metadata absent or zero in response (prompt_tokens=0, completion_tokens=0)"
                                        );
                                    }
                                }
                            }
                            Err(e) => yield Err(e),
                        }
                    }
                    Err(reqwest_eventsource::Error::StreamEnded) => {
                        // The server cleanly ended the SSE stream — this is normal EOF
                        break;
                    }
                    Err(reqwest_eventsource::Error::InvalidStatusCode(status, resp)) => {
                        let status_code = status.as_u16();
                        if status_code == 429 {
                            let retry_after = resp
                                .headers()
                                .get("retry-after")
                                .and_then(|v| v.to_str().ok())
                                .and_then(|s| s.trim().parse::<u64>().ok())
                                .or(Some(5));
                            yield Err(ProviderError::RateLimited {
                                retry_after_secs: retry_after,
                            }
                            .into());
                        } else {
                            let text = resp.text().await.unwrap_or_default();
                            yield Err(ProviderError::Api {
                                status: status_code,
                                message: format!("API error ({}): {}", status_code, text),
                            }
                            .into());
                        }
                        break;
                    }
                    Err(e) => {
                        let err_str = e.to_string();
                        if err_str.to_lowercase().contains("stream ended") {
                            break;
                        }
                        yield Err(ProviderError::StreamDecode(format!("SSE stream error: {}", err_str)).into());
                        break;
                    }
                }
            }

            if in_reasoning_mode {
                yield Ok(StreamChunk::Delta("</thought>".to_string()));
            }

            // Drain any remaining tool calls if stream completed without explicit [DONE]
            for (_, (id, name, args_str)) in std::mem::take(&mut tool_calls_accumulator) {
                let parsed_args = match serde_json::from_str::<serde_json::Value>(&args_str) {
                    Ok(val) => val,
                    Err(e) => {
                        tracing::warn!(
                            raw_arguments = %args_str,
                            error = %e,
                            "Malformed JSON tool call arguments from model on stream close"
                        );
                        serde_json::json!({
                            "__json_parse_error": format!("Invalid JSON syntax generated by model: {}", e),
                            "__raw": args_str
                        })
                    }
                };
                yield Ok(StreamChunk::ToolCallChunk(ToolCall {
                    id,
                    name,
                    arguments: parsed_args,
                }));
            }
        };

        Ok(Box::pin(stream))
    }
}

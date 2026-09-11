use crate::agent::provider::{ChunkStream, CompletionOptions, Provider, StreamChunk, ToolSchema};
use crate::agent::types::{Message, Role, ToolCall};
use crate::error::{ProviderError, Result};
use async_trait::async_trait;
use reqwest_eventsource::{Event, EventSource};
use tokio_stream::StreamExt;

/// Anthropic Messages API Provider implementation (native support for Claude 3.7 Sonnet & extended thinking)
pub struct AnthropicProvider {
    api_key: String,
    client: reqwest::Client,
    base_url: String,
    default_model_name: String,
}

impl AnthropicProvider {
    pub fn new(api_key: impl Into<String>) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(
                crate::constants::PROVIDER_STREAM_TIMEOUT_SECS,
            ))
            .build()
            .unwrap_or_default();

        Self {
            api_key: api_key.into(),
            client,
            base_url: crate::constants::ANTHROPIC_BASE_URL.to_string(),
            default_model_name: crate::constants::ANTHROPIC_DEFAULT_MODEL.to_string(),
        }
    }

    pub fn with_base_url(
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
            api_key: api_key.into(),
            client,
            base_url: base_url.into(),
            default_model_name: default_model.into(),
        }
    }

    /// Extracts all system prompts (from system instruction + any Role::System messages)
    pub fn extract_system_prompt(
        messages: &[Message],
        system_instruction: Option<&str>,
    ) -> Option<String> {
        let mut parts = Vec::new();
        if let Some(sys) = system_instruction {
            let trimmed = sys.trim();
            if !trimmed.is_empty() {
                parts.push(trimmed.to_string());
            }
        }
        for msg in messages {
            if msg.role == Role::System {
                let trimmed = msg.content.trim();
                if !trimmed.is_empty() {
                    parts.push(trimmed.to_string());
                }
            }
        }
        if parts.is_empty() {
            None
        } else {
            Some(parts.join("\n\n"))
        }
    }

    /// Translates unified Message slice to Anthropic API messages format.
    /// Alternates user and assistant roles and groups tool results under user messages.
    pub fn format_messages(messages: &[Message]) -> Vec<serde_json::Value> {
        let mut anthropic_messages: Vec<serde_json::Value> = Vec::new();

        let mut i = 0;
        while i < messages.len() {
            let msg = &messages[i];
            match msg.role {
                Role::System => {
                    i += 1;
                }
                Role::Assistant => {
                    let mut content_blocks = Vec::new();
                    if !msg.content.is_empty() {
                        content_blocks.push(serde_json::json!({
                            "type": "text",
                            "text": msg.content
                        }));
                    }
                    if let Some(ref tool_calls) = msg.tool_calls {
                        for tc in tool_calls {
                            content_blocks.push(serde_json::json!({
                                "type": "tool_use",
                                "id": tc.id,
                                "name": tc.name,
                                "input": tc.arguments
                            }));
                        }
                    }
                    if content_blocks.is_empty() {
                        content_blocks.push(serde_json::json!({
                            "type": "text",
                            "text": "..."
                        }));
                    }

                    anthropic_messages.push(serde_json::json!({
                        "role": "assistant",
                        "content": content_blocks
                    }));
                    i += 1;
                }
                Role::Tool => {
                    let mut tool_results = Vec::new();
                    while i < messages.len() && messages[i].role == Role::Tool {
                        let tool_msg = &messages[i];
                        let tool_use_id = tool_msg.tool_call_id.as_deref().unwrap_or("call_1");
                        tool_results.push(serde_json::json!({
                            "type": "tool_result",
                            "tool_use_id": tool_use_id,
                            "content": tool_msg.content
                        }));
                        i += 1;
                    }

                    if let Some(last) = anthropic_messages.last_mut() {
                        if last.get("role").and_then(|r| r.as_str()) == Some("user") {
                            if let Some(arr) =
                                last.get_mut("content").and_then(|c| c.as_array_mut())
                            {
                                arr.extend(tool_results);
                                continue;
                            }
                        }
                    }

                    anthropic_messages.push(serde_json::json!({
                        "role": "user",
                        "content": tool_results
                    }));
                }
                Role::User => {
                    if let Some(last) = anthropic_messages.last_mut() {
                        if last.get("role").and_then(|r| r.as_str()) == Some("user") {
                            if let Some(prev_text) = last.get("content").and_then(|c| c.as_str()) {
                                let merged = format!("{}\n\n{}", prev_text, msg.content);
                                last["content"] = serde_json::Value::String(merged);
                                i += 1;
                                continue;
                            } else if let Some(arr) =
                                last.get_mut("content").and_then(|c| c.as_array_mut())
                            {
                                arr.push(serde_json::json!({
                                    "type": "text",
                                    "text": msg.content
                                }));
                                i += 1;
                                continue;
                            }
                        }
                    }

                    anthropic_messages.push(serde_json::json!({
                        "role": "user",
                        "content": if msg.content.is_empty() { "..." } else { &msg.content }
                    }));
                    i += 1;
                }
            }
        }

        if let Some(first) = anthropic_messages.first() {
            if first.get("role").and_then(|r| r.as_str()) == Some("assistant") {
                anthropic_messages.insert(
                    0,
                    serde_json::json!({
                        "role": "user",
                        "content": crate::constants::ANTHROPIC_INIT_USER_PROMPT
                    }),
                );
            }
        }

        anthropic_messages
    }

    /// Formats tools into Anthropic tool specification
    pub fn format_tools(tools: &[ToolSchema]) -> Option<Vec<serde_json::Value>> {
        if tools.is_empty() {
            return None;
        }

        let formatted: Vec<serde_json::Value> = tools
            .iter()
            .map(|t| {
                serde_json::json!({
                    "name": t.name,
                    "description": t.description,
                    "input_schema": t.parameters
                })
            })
            .collect();

        Some(formatted)
    }

    /// Builds the Anthropic request JSON body
    pub fn build_request_body(
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

        let system_prompt =
            Self::extract_system_prompt(messages, options.system_instruction.as_deref());
        let formatted_messages = Self::format_messages(messages);

        let thinking_budget = options
            .thinking_budget
            .filter(|&b| b >= crate::constants::MIN_THINKING_BUDGET_TOKENS);

        // Anthropic requires max_tokens > thinking.budget_tokens
        let max_tokens = if let Some(budget) = thinking_budget {
            if options.max_tokens <= budget {
                budget + crate::constants::ANTHROPIC_THINKING_HEADROOM_TOKENS
            } else {
                options.max_tokens
            }
        } else {
            options.max_tokens
        };

        let mut body = serde_json::json!({
            "model": model,
            "messages": formatted_messages,
            "max_tokens": max_tokens,
            "stream": true,
        });

        if let Some(sys) = system_prompt {
            body["system"] = serde_json::Value::String(sys);
        }

        if let Some(tools_payload) = Self::format_tools(tools) {
            body["tools"] = serde_json::Value::Array(tools_payload);
        }

        if let Some(budget) = thinking_budget {
            body["thinking"] = serde_json::json!({
                "type": "enabled",
                "budget_tokens": budget
            });
            // Anthropic rejects temperature != 1.0 when thinking is enabled
            body["temperature"] = serde_json::json!(1.0);
        } else {
            body["temperature"] = serde_json::json!(options.temperature);
        }

        body
    }

    /// Parses an Anthropic SSE event into StreamChunk items
    pub fn parse_anthropic_event(
        event_type: &str,
        val: &serde_json::Value,
        tool_accumulator: &mut std::collections::BTreeMap<usize, (String, String, String)>,
        prompt_tokens: &mut usize,
        completion_tokens: &mut usize,
        in_thinking_mode: &mut bool,
    ) -> Vec<StreamChunk> {
        let mut chunks = Vec::new();
        let eff_type = val
            .get("type")
            .and_then(|t| t.as_str())
            .unwrap_or(event_type);

        match eff_type {
            "message_start" => {
                if let Some(usage) = val.get("message").and_then(|m| m.get("usage")) {
                    *prompt_tokens = usage
                        .get("input_tokens")
                        .and_then(|u| u.as_u64())
                        .unwrap_or(0) as usize;
                }
            }
            "content_block_start" => {
                let index = val.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
                if let Some(block) = val.get("content_block") {
                    let block_type = block.get("type").and_then(|t| t.as_str()).unwrap_or("");
                    if block_type == "tool_use" {
                        let id = block
                            .get("id")
                            .and_then(|s| s.as_str())
                            .unwrap_or("")
                            .to_string();
                        let name = block
                            .get("name")
                            .and_then(|s| s.as_str())
                            .unwrap_or("")
                            .to_string();
                        tool_accumulator.insert(index, (id, name, String::new()));
                    } else if block_type == "thinking" && !*in_thinking_mode {
                        *in_thinking_mode = true;
                        chunks.push(StreamChunk::Delta("<thought>".to_string()));
                    }
                }
            }
            "content_block_delta" => {
                let index = val.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
                if let Some(delta) = val.get("delta") {
                    let delta_type = delta.get("type").and_then(|t| t.as_str()).unwrap_or("");
                    match delta_type {
                        "text_delta" => {
                            if let Some(text) = delta.get("text").and_then(|t| t.as_str()) {
                                if !text.is_empty() {
                                    if *in_thinking_mode {
                                        *in_thinking_mode = false;
                                        chunks.push(StreamChunk::Delta("</thought>".to_string()));
                                    }
                                    chunks.push(StreamChunk::Delta(text.to_string()));
                                }
                            }
                        }
                        "thinking_delta" => {
                            if let Some(thinking) = delta.get("thinking").and_then(|t| t.as_str()) {
                                if !thinking.is_empty() {
                                    if !*in_thinking_mode {
                                        *in_thinking_mode = true;
                                        chunks.push(StreamChunk::Delta("<thought>".to_string()));
                                    }
                                    chunks.push(StreamChunk::Delta(thinking.to_string()));
                                }
                            }
                        }
                        "input_json_delta" => {
                            if let Some(partial_json) =
                                delta.get("partial_json").and_then(|s| s.as_str())
                            {
                                if let Some(entry) = tool_accumulator.get_mut(&index) {
                                    entry.2.push_str(partial_json);
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            "content_block_stop" => {
                let index = val.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
                if *in_thinking_mode {
                    *in_thinking_mode = false;
                    chunks.push(StreamChunk::Delta("</thought>".to_string()));
                }
                if let Some((id, name, args_str)) = tool_accumulator.remove(&index) {
                    let parsed_args = match serde_json::from_str::<serde_json::Value>(&args_str) {
                        Ok(parsed) => parsed,
                        Err(e) => {
                            tracing::warn!(raw = %args_str, error = %e, "Malformed JSON in Anthropic tool call arguments");
                            serde_json::json!({
                                "__json_parse_error": format!("Invalid JSON: {}", e),
                                "__raw": args_str
                            })
                        }
                    };
                    chunks.push(StreamChunk::ToolCallChunk(ToolCall {
                        id,
                        name,
                        arguments: parsed_args,
                    }));
                }
            }
            "message_delta" => {
                if let Some(usage) = val.get("usage") {
                    if let Some(out) = usage.get("output_tokens").and_then(|o| o.as_u64()) {
                        *completion_tokens = out as usize;
                    }
                }
            }
            "message_stop" => {
                if *in_thinking_mode {
                    *in_thinking_mode = false;
                    chunks.push(StreamChunk::Delta("</thought>".to_string()));
                }
                if *prompt_tokens > 0 || *completion_tokens > 0 {
                    chunks.push(StreamChunk::Usage {
                        prompt_tokens: *prompt_tokens,
                        completion_tokens: *completion_tokens,
                    });
                }
                chunks.push(StreamChunk::Done);
            }
            _ => {}
        }

        chunks
    }
}

#[async_trait]
impl Provider for AnthropicProvider {
    fn name(&self) -> &str {
        "anthropic"
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
        let url = format!("{}/messages", self.base_url.trim_end_matches('/'));
        let request_body = Self::build_request_body(messages, tools, options, self.default_model());

        let req_builder = self
            .client
            .post(&url)
            .header("content-type", "application/json")
            .header("x-api-key", &self.api_key)
            .header(
                "anthropic-version",
                crate::constants::ANTHROPIC_VERSION_HEADER,
            )
            .json(&request_body);

        let event_source = EventSource::new(req_builder).map_err(|e| {
            ProviderError::StreamDecode(format!("Failed to connect to Anthropic API: {}", e))
        })?;

        let stream = async_stream::stream! {
            let mut event_source = event_source;
            let mut prompt_tokens = 0usize;
            let mut completion_tokens = 0usize;
            let mut in_thinking_mode = false;
            let mut tool_calls_accumulator: std::collections::BTreeMap<usize, (String, String, String)> =
                std::collections::BTreeMap::new();

            while let Some(event_res) = event_source.next().await {
                match event_res {
                    Ok(Event::Open) => {
                        tracing::debug!("Anthropic SSE stream open");
                    }
                    Ok(Event::Message(message)) => {
                        let parsed: std::result::Result<serde_json::Value, _> = serde_json::from_str(&message.data);
                        let val = match parsed {
                            Ok(v) => v,
                            Err(e) => {
                                tracing::debug!("Non-JSON SSE chunk from Anthropic: {}", e);
                                continue;
                            }
                        };

                        if let Some(err_obj) = val.get("error") {
                            let err_msg = err_obj
                                .get("message")
                                .and_then(|m| m.as_str())
                                .unwrap_or("Unknown Anthropic API error");
                            yield Err(ProviderError::Api {
                                status: 400,
                                message: format!("Anthropic API error: {}", err_msg),
                            }.into());
                            break;
                        }

                        let chunks = Self::parse_anthropic_event(
                            message.event.as_str(),
                            &val,
                            &mut tool_calls_accumulator,
                            &mut prompt_tokens,
                            &mut completion_tokens,
                            &mut in_thinking_mode,
                        );

                        let mut is_done = false;
                        for chunk in chunks {
                            if chunk == StreamChunk::Done {
                                is_done = true;
                            }
                            yield Ok(chunk);
                        }
                        if is_done {
                            break;
                        }
                    }
                    Err(reqwest_eventsource::Error::StreamEnded) => {
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
                                .or(Some(crate::constants::PROVIDER_RATE_LIMIT_RETRY_DELAY_SECS));
                            yield Err(ProviderError::RateLimited {
                                retry_after_secs: retry_after,
                            }.into());
                        } else {
                            let text = resp.text().await.unwrap_or_default();
                            yield Err(ProviderError::Api {
                                status: status_code,
                                message: format!("Anthropic API error ({}): {}", status_code, text),
                            }.into());
                        }
                        break;
                    }
                    Err(e) => {
                        let err_str = e.to_string();
                        if err_str.to_lowercase().contains("stream ended") {
                            break;
                        }
                        yield Err(ProviderError::StreamDecode(format!("Anthropic SSE stream error: {}", err_str)).into());
                        break;
                    }
                }
            }

            if in_thinking_mode {
                yield Ok(StreamChunk::Delta("</thought>".to_string()));
            }

            // Drain any remaining tool calls
            for (_, (id, name, args_str)) in std::mem::take(&mut tool_calls_accumulator) {
                let parsed_args = match serde_json::from_str::<serde_json::Value>(&args_str) {
                    Ok(parsed) => parsed,
                    Err(e) => {
                        serde_json::json!({
                            "__json_parse_error": format!("Invalid JSON: {}", e),
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

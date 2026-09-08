use crate::agent::provider::{ChunkStream, CompletionOptions, Provider, StreamChunk, ToolSchema};
use crate::agent::types::{Message, Role, ToolCall};
use crate::constants::DEFAULT_MODEL_GEMINI;
use crate::error::{ProviderError, Result};
use async_trait::async_trait;
use reqwest_eventsource::{Event, EventSource};
use tokio_stream::StreamExt;

/// Google Gemini API Provider implementation
pub struct GeminiProvider {
    api_key: String,
    client: reqwest::Client,
    base_url: String,
}

impl GeminiProvider {
    pub fn new(api_key: impl Into<String>) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(
                crate::constants::PROVIDER_REQUEST_TIMEOUT_SECS,
            ))
            .build()
            .unwrap_or_default();

        Self {
            api_key: api_key.into(),
            client,
            base_url: crate::constants::GEMINI_BASE_URL.to_string(),
        }
    }

    /// Translates our unified Message slice to Gemini API `contents` format
    pub(crate) fn format_contents(messages: &[Message]) -> Vec<serde_json::Value> {
        let mut contents = Vec::new();

        for msg in messages {
            match msg.role {
                Role::System => {
                    // System instructions in Gemini are handled in a dedicated top-level field
                    continue;
                }
                Role::User => {
                    contents.push(serde_json::json!({
                        "role": "user",
                        "parts": [{ "text": msg.content }]
                    }));
                }
                Role::Assistant => {
                    if let Some(ref tool_calls) = msg.tool_calls {
                        let mut parts = Vec::new();
                        if !msg.content.is_empty() {
                            parts.push(serde_json::json!({ "text": msg.content }));
                        }
                        for tc in tool_calls {
                            let mut func_call = serde_json::json!({
                                "name": tc.name,
                                "args": tc.arguments
                            });
                            if !tc.id.is_empty() {
                                func_call["id"] = serde_json::json!(tc.id);
                            }
                            parts.push(serde_json::json!({
                                "functionCall": func_call
                            }));
                        }
                        contents.push(serde_json::json!({
                            "role": "model",
                            "parts": parts
                        }));
                    } else {
                        contents.push(serde_json::json!({
                            "role": "model",
                            "parts": [{ "text": msg.content }]
                        }));
                    }
                }
                Role::Tool => {
                    let tool_name = msg
                        .tool_name
                        .as_deref()
                        .or(msg.tool_call_id.as_deref())
                        .unwrap_or("unknown_tool");
                    let mut func_resp = serde_json::json!({
                        "name": tool_name,
                        "response": {
                            "output": msg.content
                        }
                    });
                    if let Some(ref call_id) = msg.tool_call_id {
                        if !call_id.is_empty() {
                            func_resp["id"] = serde_json::json!(call_id);
                        }
                    }
                    let part = serde_json::json!({
                        "functionResponse": func_resp
                    });

                    // Merge consecutive tool responses into single user content block
                    let mut merged = false;
                    if let Some(last) = contents.last_mut() {
                        if last.get("role").and_then(|r| r.as_str()) == Some("user") {
                            if let Some(parts) =
                                last.get_mut("parts").and_then(|p| p.as_array_mut())
                            {
                                if parts.iter().any(|p| p.get("functionResponse").is_some()) {
                                    parts.push(part.clone());
                                    merged = true;
                                }
                            }
                        }
                    }
                    if !merged {
                        contents.push(serde_json::json!({
                            "role": "user",
                            "parts": [part]
                        }));
                    }
                }
            }
        }

        contents
    }

    /// Formats our internal ToolSchemas to Gemini `functionDeclarations`
    fn format_tools(tools: &[ToolSchema]) -> Option<serde_json::Value> {
        if tools.is_empty() {
            return None;
        }

        let declarations: Vec<serde_json::Value> = tools
            .iter()
            .map(|t| {
                serde_json::json!({
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.parameters
                })
            })
            .collect();

        Some(serde_json::json!([{
            "functionDeclarations": declarations
        }]))
    }
}

#[async_trait]
impl Provider for GeminiProvider {
    fn name(&self) -> &str {
        "gemini"
    }

    fn default_model(&self) -> &str {
        DEFAULT_MODEL_GEMINI
    }

    async fn stream_completion(
        &self,
        messages: &[Message],
        tools: &[ToolSchema],
        options: &CompletionOptions,
    ) -> Result<ChunkStream> {
        let model = if options.model.is_empty() {
            self.default_model()
        } else {
            &options.model
        };

        let url = format!(
            "{}/models/{}:streamGenerateContent?alt=sse",
            self.base_url, model
        );

        let contents = Self::format_contents(messages);
        let mut gen_config = serde_json::json!({
            "temperature": options.temperature,
            "maxOutputTokens": options.max_tokens,
        });

        if let Some(budget) = options.thinking_budget {
            gen_config["thinkingConfig"] = serde_json::json!({
                "thinkingBudget": budget
            });
        }

        let mut request_body = serde_json::json!({
            "contents": contents,
            "generationConfig": gen_config,
        });

        if let Some(ref sys) = options.system_instruction {
            request_body["systemInstruction"] = serde_json::json!({
                "parts": [{ "text": sys }]
            });
        }

        if let Some(tools_payload) = Self::format_tools(tools) {
            request_body["tools"] = tools_payload;
        }

        let request = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("x-goog-api-key", &self.api_key)
            .json(&request_body);

        let event_source = EventSource::new(request).map_err(|e| {
            ProviderError::StreamDecode(format!("Failed to establish SSE connection: {}", e))
        })?;

        let stream = async_stream::stream! {
            let mut event_source = event_source;

            while let Some(event_res) = event_source.next().await {
                match event_res {
                    Ok(Event::Open) => {
                        tracing::debug!("Gemini SSE connection established");
                    }
                    Ok(Event::Message(message)) => {
                        if message.data.trim() == "[DONE]" {
                            yield Ok(StreamChunk::Done);
                            break;
                        }

                        let parsed: Result<serde_json::Value> = serde_json::from_str(&message.data)
                            .map_err(|e| ProviderError::StreamDecode(format!("JSON parse error: {}", e)).into());

                        match parsed {
                            Ok(val) => {
                                // 1. Check API-level error inside stream payload
                                if let Some(err) = val.get("error") {
                                    let code = err.get("code").and_then(|c| c.as_u64()).unwrap_or(500) as u16;
                                    let message = err
                                        .get("message")
                                        .and_then(|m| m.as_str())
                                        .unwrap_or("Unknown Gemini API error")
                                        .to_string();
                                    yield Err(ProviderError::Api {
                                        status: code,
                                        message: format!("Gemini stream API error: {}", message),
                                    }
                                    .into());
                                    break;
                                }

                                // 2. Check prompt-level policy blocks
                                if let Some(feedback) = val.get("promptFeedback") {
                                    if let Some(reason) = feedback.get("blockReason").and_then(|r| r.as_str()) {
                                        yield Err(ProviderError::Api {
                                            status: 400,
                                            message: format!("Gemini prompt blocked: blockReason={}", reason),
                                        }
                                        .into());
                                        break;
                                    }
                                }

                                // 3. Check candidates and candidate finishReason
                                if let Some(candidates) = val.get("candidates").and_then(|c| c.as_array()) {
                                    for candidate in candidates {
                                        if let Some(reason) = candidate.get("finishReason").and_then(|r| r.as_str()) {
                                            if reason == "SAFETY" || reason == "RECITATION" || reason == "BLOCKLIST" || reason == "PROHIBITED_CONTENT" {
                                                yield Err(ProviderError::Api {
                                                    status: 400,
                                                    message: format!("Gemini candidate generation halted: finishReason={}", reason),
                                                }
                                                .into());
                                                return;
                                            }
                                        }
                                        if let Some(parts) = candidate.get("content").and_then(|c| c.get("parts")).and_then(|p| p.as_array()) {
                                            for part in parts {
                                                // Handle text delta
                                                if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                                                    if !text.is_empty() {
                                                        if part.get("thought").and_then(|t| t.as_bool()) == Some(true) {
                                                            yield Ok(StreamChunk::Delta(format!("<thought>{}</thought>", text)));
                                                        } else {
                                                            yield Ok(StreamChunk::Delta(text.to_string()));
                                                        }
                                                    }
                                                }

                                                // Handle function / tool call
                                                if let Some(func_call) = part.get("functionCall") {
                                                    if let Some(name) = func_call.get("name").and_then(|n| n.as_str()) {
                                                        let args = func_call.get("args").cloned().unwrap_or(serde_json::json!({}));
                                                        let call_id = func_call
                                                            .get("id")
                                                            .and_then(|i| i.as_str())
                                                            .map(|s| s.to_string())
                                                            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
                                                        let tool_call = ToolCall {
                                                            id: call_id,
                                                            name: name.to_string(),
                                                            arguments: args,
                                                        };
                                                        yield Ok(StreamChunk::ToolCallChunk(tool_call));
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }

                                // Check usage metadata if provided
                                if let Some(usage) = val.get("usageMetadata") {
                                    let prompt_tokens = usage.get("promptTokenCount").and_then(|t| t.as_u64()).unwrap_or(0) as usize;
                                    let completion_tokens = usage.get("candidatesTokenCount").and_then(|t| t.as_u64()).unwrap_or(0) as usize;
                                    if prompt_tokens > 0 || completion_tokens > 0 {
                                        yield Ok(StreamChunk::Usage { prompt_tokens, completion_tokens });
                                    } else {
                                        tracing::debug!(
                                            provider = "gemini",
                                            "Usage metadata absent or zero in Gemini response (prompt_tokens=0, completion_tokens=0)"
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
                                message: format!("Gemini API error: {}", text),
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
                        yield Err(ProviderError::StreamDecode(format!("EventSource stream error: {}", err_str)).into());
                        break;
                    }
                }
            }
        };

        Ok(Box::pin(stream))
    }
}

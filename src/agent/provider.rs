use crate::agent::types::{Message, Role, ToolCall};
use crate::error::{ProviderError, Result};
use async_trait::async_trait;
use futures::Stream;
use reqwest_eventsource::{Event, EventSource};
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
}

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
    fn format_contents(messages: &[Message]) -> Vec<serde_json::Value> {
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
        "gemini-2.5-pro"
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
        let mut request_body = serde_json::json!({
            "contents": contents,
            "generationConfig": {
                "temperature": options.temperature,
                "maxOutputTokens": options.max_tokens,
            }
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
                                    }
                                }
                            }
                            Err(e) => yield Err(e),
                        }
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
                        yield Err(ProviderError::StreamDecode(format!("EventSource stream error: {}", e)).into());
                        break;
                    }
                }
            }
        };

        Ok(Box::pin(stream))
    }
}

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
        let model = if options.model.is_empty() {
            self.default_model()
        } else {
            &options.model
        };

        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
        let formatted_messages =
            Self::format_messages(messages, options.system_instruction.as_deref());

        let mut request_body = serde_json::json!({
            "model": model,
            "messages": formatted_messages,
            "temperature": options.temperature,
            "max_tokens": options.max_tokens,
            "stream": true,
        });

        if let Some(tools_payload) = Self::format_tools(tools) {
            request_body["tools"] = serde_json::Value::Array(tools_payload);
        }

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

        let event_source = EventSource::new(request).map_err(|e| {
            ProviderError::StreamDecode(format!(
                "Failed to connect to {}: {}",
                self.provider_name, e
            ))
        })?;

        let stream = async_stream::stream! {
            let mut event_source = event_source;
            let mut tool_calls_accumulator: std::collections::BTreeMap<usize, (String, String, String)> =
                std::collections::BTreeMap::new(); // (id, name, args_json)

            while let Some(event_res) = event_source.next().await {
                match event_res {
                    Ok(Event::Open) => {
                        tracing::debug!("OpenAI-compatible SSE stream open");
                    }
                    Ok(Event::Message(message)) => {
                        if message.data.trim() == "[DONE]" {
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
                                            // 1. Reasoning / Thought Delta (DeepSeek R1, OpenAI o1/o3, etc.)
                                            if let Some(reasoning) = delta.get("reasoning_content").or_else(|| delta.get("reasoning")).and_then(|r| r.as_str()) {
                                                if !reasoning.is_empty() {
                                                    yield Ok(StreamChunk::Delta(format!("<thought>{}</thought>", reasoning)));
                                                }
                                            }

                                            // 2. Text Delta
                                            if let Some(content) = delta.get("content").and_then(|c| c.as_str()) {
                                                if !content.is_empty() {
                                                    yield Ok(StreamChunk::Delta(content.to_string()));
                                                }
                                            }

                                            // 2. Tool Calls Delta
                                            if let Some(tool_calls) = delta.get("tool_calls").and_then(|tc| tc.as_array()) {
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
                                    }
                                }
                            }
                            Err(e) => yield Err(e),
                        }
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
                        yield Err(ProviderError::StreamDecode(format!("SSE stream error: {}", e)).into());
                        break;
                    }
                }
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

/// Provider factory that initializes the appropriate provider based on name and config
pub fn create_provider(provider_name: &str, api_key: &str) -> Result<Box<dyn Provider>> {
    create_provider_with_base_url(provider_name, api_key, None)
}

/// Provider factory that supports custom base URLs for OpenAI-compatible endpoints
pub fn create_provider_with_base_url(
    provider_name: &str,
    api_key: &str,
    custom_base_url: Option<&str>,
) -> Result<Box<dyn Provider>> {
    match provider_name.to_lowercase().as_str() {
        "gemini" | "google" => Ok(Box::new(GeminiProvider::new(api_key))),
        "openrouter" => Ok(Box::new(OpenAiCompatibleProvider::openrouter(api_key))),
        "openai" => {
            if let Some(url) = custom_base_url {
                Ok(Box::new(OpenAiCompatibleProvider::new(
                    "openai",
                    api_key,
                    url,
                    crate::constants::OPENAI_DEFAULT_MODEL,
                )))
            } else {
                Ok(Box::new(OpenAiCompatibleProvider::openai(api_key)))
            }
        }
        "deepseek" => Ok(Box::new(OpenAiCompatibleProvider::new(
            "deepseek",
            api_key,
            custom_base_url.unwrap_or(crate::constants::DEEPSEEK_BASE_URL),
            crate::constants::DEEPSEEK_DEFAULT_MODEL,
        ))),
        "groq" => Ok(Box::new(OpenAiCompatibleProvider::new(
            "groq",
            api_key,
            custom_base_url.unwrap_or(crate::constants::GROQ_BASE_URL),
            "llama-3.3-70b-versatile",
        ))),
        "together" => Ok(Box::new(OpenAiCompatibleProvider::new(
            "together",
            api_key,
            custom_base_url.unwrap_or(crate::constants::TOGETHER_BASE_URL),
            "meta-llama/Llama-3.3-70B-Instruct-Turbo",
        ))),
        "ollama" => Ok(Box::new(OpenAiCompatibleProvider::new(
            "ollama",
            "",
            custom_base_url.unwrap_or(crate::constants::OLLAMA_DEFAULT_BASE_URL),
            "qwen2.5-coder",
        ))),
        custom_name => {
            if let Some(url) = custom_base_url {
                Ok(Box::new(OpenAiCompatibleProvider::new(
                    custom_name,
                    api_key,
                    url,
                    "default-model",
                )))
            } else {
                Err(ProviderError::UnsupportedModel {
                    model: custom_name.to_string(),
                    provider: provider_name.to_string(),
                }
                .into())
            }
        }
    }
}

use crate::agent::circuit_breaker::{CircuitBreaker, CircuitBreakerConfig, RetryPolicy};

/// A resilient provider decorator that wraps any LLM Provider with
/// an automated circuit breaker and exponential backoff retry policy.
#[allow(dead_code)]
pub struct ResilientProvider<P: Provider> {
    inner: P,
    circuit_breaker: CircuitBreaker,
    retry_policy: RetryPolicy,
}

#[allow(dead_code)]
impl<P: Provider> ResilientProvider<P> {
    pub fn new(inner: P) -> Self {
        Self {
            inner,
            circuit_breaker: CircuitBreaker::new(CircuitBreakerConfig::default()),
            retry_policy: RetryPolicy::default(),
        }
    }

    pub fn with_config(
        inner: P,
        cb_config: CircuitBreakerConfig,
        retry_policy: RetryPolicy,
    ) -> Self {
        Self {
            inner,
            circuit_breaker: CircuitBreaker::new(cb_config),
            retry_policy,
        }
    }

    pub fn circuit_breaker(&self) -> &CircuitBreaker {
        &self.circuit_breaker
    }
}

#[async_trait]
impl<P: Provider> Provider for ResilientProvider<P> {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn default_model(&self) -> &str {
        self.inner.default_model()
    }

    async fn stream_completion(
        &self,
        messages: &[Message],
        tools: &[ToolSchema],
        options: &CompletionOptions,
    ) -> Result<ChunkStream> {
        self.circuit_breaker.can_execute()?;

        let mut last_error = None;
        for attempt in 0..=self.retry_policy.max_retries {
            if attempt > 0 {
                let delay = self.retry_policy.delay_for_attempt(attempt);
                tracing::warn!(
                    attempt = attempt,
                    delay_ms = delay.as_millis(),
                    "Retrying LLM stream completion with exponential backoff..."
                );
                tokio::time::sleep(delay).await;
            }

            match self.inner.stream_completion(messages, tools, options).await {
                Ok(stream) => {
                    self.circuit_breaker.record_success();
                    return Ok(stream);
                }
                Err(err) => {
                    self.circuit_breaker.record_failure();
                    if !RetryPolicy::is_retryable(&err) {
                        return Err(err);
                    }
                    last_error = Some(err);
                }
            }
        }

        Err(last_error.unwrap_or_else(|| {
            ProviderError::Network("All stream completion retry attempts failed".to_string()).into()
        }))
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

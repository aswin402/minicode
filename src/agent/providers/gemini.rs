use crate::agent::provider::{
    classify_reqwest_stream_error, send_request_with_ipv4_fallback, shared_llm_http_client_ipv4,
    ChunkStream, CompletionOptions, Provider, SseLineDecoder, StreamChunk, ToolSchema,
};
use crate::agent::types::{ImageAttachment, Message, Role, ToolCall};
use crate::error::{ProviderError, Result};
use async_trait::async_trait;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};
use tokio_stream::StreamExt;

static GEMINI_THOUGHT_SIGNATURES: OnceLock<RwLock<HashMap<String, String>>> = OnceLock::new();

fn thought_signature_cache() -> &'static RwLock<HashMap<String, String>> {
    GEMINI_THOUGHT_SIGNATURES.get_or_init(|| RwLock::new(HashMap::new()))
}

fn record_thought_signature(call_id: &str, signature: &str) {
    if call_id.is_empty() || signature.is_empty() {
        return;
    }
    if let Ok(mut guard) = thought_signature_cache().write() {
        if guard.len() >= 256 {
            guard.clear();
        }
        guard.insert(call_id.to_string(), signature.to_string());
    }
}

fn lookup_thought_signature(call_id: &str) -> Option<String> {
    if call_id.is_empty() {
        return None;
    }
    thought_signature_cache()
        .read()
        .ok()
        .and_then(|g| g.get(call_id).cloned())
}

/// Returns true when a Gemini model supports `generationConfig.thinkingConfig`.
#[must_use]
fn model_supports_gemini_thinking(model: &str) -> bool {
    let lower = model.trim().to_ascii_lowercase();
    let stripped = lower.strip_prefix("models/").unwrap_or(&lower);
    stripped.starts_with("gemini-2.5")
        || stripped.starts_with("gemini-3")
        || stripped.contains("thinking")
}

/// Clamps `maxOutputTokens` for Gemini 1.5 / 2.0 / Gemma models capped at 8192 output tokens.
#[must_use]
fn clamp_gemini_max_output_tokens(model: &str, requested: usize) -> usize {
    if let Some(limit) = crate::agent::models::lookup_models_dev_output_limit(model) {
        if limit >= 256 {
            return requested.min(limit);
        }
    }
    let lower = model.trim().to_ascii_lowercase();
    let stripped = lower.strip_prefix("models/").unwrap_or(&lower);
    if stripped.starts_with("gemini-2.0")
        || stripped.starts_with("gemini-1.5")
        || stripped.starts_with("gemma")
    {
        requested.min(8192)
    } else {
        requested
    }
}

#[derive(Serialize)]
struct GeminiInlineDataWire<'a> {
    #[serde(rename = "mimeType")]
    mime_type: &'a str,
    data: &'a str,
}

#[derive(Serialize)]
struct GeminiFunctionCallWire<'a> {
    name: &'a str,
    args: &'a serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<&'a str>,
}

#[derive(Serialize)]
struct GeminiToolOutputWire<'a> {
    output: &'a str,
}

#[derive(Serialize)]
struct GeminiFunctionResponseWire<'a> {
    name: &'a str,
    response: GeminiToolOutputWire<'a>,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<&'a str>,
}

#[derive(Serialize)]
#[serde(untagged)]
enum GeminiPartWire<'a> {
    Text {
        text: &'a str,
    },
    InlineData {
        #[serde(rename = "inlineData")]
        inline_data: GeminiInlineDataWire<'a>,
    },
    FunctionCall {
        #[serde(rename = "functionCall")]
        function_call: GeminiFunctionCallWire<'a>,
        #[serde(rename = "thoughtSignature", skip_serializing_if = "Option::is_none")]
        thought_signature: Option<String>,
    },
    FunctionResponse {
        #[serde(rename = "functionResponse")]
        function_response: GeminiFunctionResponseWire<'a>,
    },
}

#[derive(Serialize)]
struct GeminiContentWire<'a> {
    role: &'static str,
    parts: Vec<GeminiPartWire<'a>>,
}

#[derive(Serialize)]
struct GeminiThinkingConfigWire {
    #[serde(rename = "thinkingBudget")]
    thinking_budget: usize,
}

#[derive(Serialize)]
struct GeminiGenerationConfigWire {
    temperature: f32,
    #[serde(rename = "maxOutputTokens")]
    max_output_tokens: usize,
    #[serde(rename = "thinkingConfig", skip_serializing_if = "Option::is_none")]
    thinking_config: Option<GeminiThinkingConfigWire>,
}

#[derive(Serialize)]
struct GeminiSystemInstructionWire<'a> {
    parts: [GeminiPartWire<'a>; 1],
}

#[derive(Serialize)]
struct GeminiFunctionDeclWire<'a> {
    name: &'a str,
    description: &'a str,
    parameters: &'a serde_json::Value,
}

#[derive(Serialize)]
struct GeminiToolDeclsWire<'a> {
    #[serde(rename = "functionDeclarations")]
    function_declarations: Vec<GeminiFunctionDeclWire<'a>>,
}

#[derive(Serialize)]
struct GeminiRequestWire<'a> {
    contents: Vec<GeminiContentWire<'a>>,
    #[serde(rename = "generationConfig")]
    generation_config: GeminiGenerationConfigWire,
    #[serde(rename = "systemInstruction", skip_serializing_if = "Option::is_none")]
    system_instruction: Option<GeminiSystemInstructionWire<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<[GeminiToolDeclsWire<'a>; 1]>,
}

/// Google Gemini API Provider implementation
pub struct GeminiProvider {
    api_key: String,
    client: reqwest::Client,
    base_url: String,
}

impl GeminiProvider {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self::with_base_url(api_key, crate::constants::GEMINI_BASE_URL)
    }

    pub fn with_base_url(api_key: impl Into<String>, base_url: impl Into<String>) -> Self {
        let client = crate::agent::providers::shared_llm_http_client();

        Self {
            api_key: api_key.into(),
            client,
            base_url: base_url.into(),
        }
    }

    #[allow(dead_code)]
    fn append_gemini_images(
        parts: &mut Vec<serde_json::Value>,
        imgs: Option<&Vec<crate::agent::types::ImageAttachment>>,
    ) {
        if let Some(imgs) = imgs {
            for img in imgs {
                parts.push(serde_json::json!({
                    "inlineData": {
                        "mimeType": img.media_type,
                        "data": img.data_base64
                    }
                }));
            }
        }
    }

    /// Translates our unified Message slice to Gemini API `contents` format
    #[allow(dead_code)]
    pub(crate) fn format_contents(messages: &[Message]) -> Vec<serde_json::Value> {
        let mut contents = Vec::new();

        for msg in messages {
            match msg.role {
                Role::System => {
                    // System instructions in Gemini are handled in a dedicated top-level field
                    continue;
                }
                Role::User => {
                    let mut parts = vec![serde_json::json!({ "text": msg.content })];
                    Self::append_gemini_images(&mut parts, msg.images.as_ref());
                    contents.push(serde_json::json!({
                        "role": "user",
                        "parts": parts
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
                                    Self::append_gemini_images(parts, msg.images.as_ref());
                                    merged = true;
                                }
                            }
                        }
                    }
                    if !merged {
                        let mut parts = vec![part];
                        Self::append_gemini_images(&mut parts, msg.images.as_ref());
                        contents.push(serde_json::json!({
                            "role": "user",
                            "parts": parts
                        }));
                    }
                }
            }
        }

        contents
    }

    /// Formats our internal ToolSchemas to Gemini `functionDeclarations`
    #[allow(dead_code)]
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

    fn append_wire_images<'a>(
        parts: &mut Vec<GeminiPartWire<'a>>,
        imgs: Option<&'a Vec<ImageAttachment>>,
    ) {
        if let Some(imgs) = imgs {
            for img in imgs {
                parts.push(GeminiPartWire::InlineData {
                    inline_data: GeminiInlineDataWire {
                        mime_type: &img.media_type,
                        data: &img.data_base64,
                    },
                });
            }
        }
    }

    /// Serializes the Gemini API request directly into a pre-sized `Vec<u8>` using borrowed
    /// wire structs — zero `serde_json::Value` DOM trees and zero base64/message cloning.
    pub fn serialize_request_bytes(
        messages: &[Message],
        tools: &[ToolSchema],
        options: &CompletionOptions,
    ) -> Result<Vec<u8>> {
        let mut contents: Vec<GeminiContentWire<'_>> = Vec::with_capacity(messages.len());

        for msg in messages {
            match msg.role {
                Role::System => continue,
                Role::User => {
                    let mut parts = Vec::with_capacity(1 + msg.images.as_ref().map_or(0, Vec::len));
                    parts.push(GeminiPartWire::Text { text: &msg.content });
                    Self::append_wire_images(&mut parts, msg.images.as_ref());
                    contents.push(GeminiContentWire {
                        role: "user",
                        parts,
                    });
                }
                Role::Assistant => {
                    if let Some(ref tool_calls) = msg.tool_calls {
                        let mut parts = Vec::with_capacity(tool_calls.len() + 1);
                        if !msg.content.is_empty() {
                            parts.push(GeminiPartWire::Text { text: &msg.content });
                        }
                        for tc in tool_calls {
                            parts.push(GeminiPartWire::FunctionCall {
                                function_call: GeminiFunctionCallWire {
                                    name: &tc.name,
                                    args: &tc.arguments,
                                    id: (!tc.id.is_empty()).then_some(tc.id.as_str()),
                                },
                                thought_signature: lookup_thought_signature(&tc.id),
                            });
                        }
                        contents.push(GeminiContentWire {
                            role: "model",
                            parts,
                        });
                    } else {
                        contents.push(GeminiContentWire {
                            role: "model",
                            parts: vec![GeminiPartWire::Text { text: &msg.content }],
                        });
                    }
                }
                Role::Tool => {
                    let tool_name = msg
                        .tool_name
                        .as_deref()
                        .or(msg.tool_call_id.as_deref())
                        .unwrap_or("unknown_tool");
                    let part = GeminiPartWire::FunctionResponse {
                        function_response: GeminiFunctionResponseWire {
                            name: tool_name,
                            response: GeminiToolOutputWire {
                                output: &msg.content,
                            },
                            id: msg.tool_call_id.as_deref().filter(|id| !id.is_empty()),
                        },
                    };

                    let can_merge = contents.last().is_some_and(|last| {
                        last.role == "user"
                            && last
                                .parts
                                .iter()
                                .any(|p| matches!(p, GeminiPartWire::FunctionResponse { .. }))
                    });
                    if can_merge {
                        if let Some(last) = contents.last_mut() {
                            last.parts.push(part);
                            Self::append_wire_images(&mut last.parts, msg.images.as_ref());
                        }
                    } else {
                        let mut parts =
                            Vec::with_capacity(1 + msg.images.as_ref().map_or(0, Vec::len));
                        parts.push(part);
                        Self::append_wire_images(&mut parts, msg.images.as_ref());
                        contents.push(GeminiContentWire {
                            role: "user",
                            parts,
                        });
                    }
                }
            }
        }

        let thinking_config = model_supports_gemini_thinking(&options.model)
            .then_some(options.thinking_budget)
            .flatten()
            .map(|b| GeminiThinkingConfigWire { thinking_budget: b });

        let generation_config = GeminiGenerationConfigWire {
            temperature: options.temperature,
            max_output_tokens: clamp_gemini_max_output_tokens(&options.model, options.max_tokens),
            thinking_config,
        };

        let system_instruction =
            options
                .system_instruction
                .as_deref()
                .map(|sys| GeminiSystemInstructionWire {
                    parts: [GeminiPartWire::Text { text: sys }],
                });

        let wire_tools = (!tools.is_empty()).then(|| {
            [GeminiToolDeclsWire {
                function_declarations: tools
                    .iter()
                    .map(|t| GeminiFunctionDeclWire {
                        name: &t.name,
                        description: &t.description,
                        parameters: &t.parameters,
                    })
                    .collect(),
            }]
        });

        let estimated_bytes = 512
            + options.system_instruction.as_ref().map_or(0, String::len)
            + messages
                .iter()
                .map(|m| {
                    m.content.len()
                        + 96
                        + m.images.as_ref().map_or(0, |imgs| {
                            imgs.iter().map(|i| i.data_base64.len() + 64).sum::<usize>()
                        })
                })
                .sum::<usize>()
            + tools.len() * 384;

        let wire_req = GeminiRequestWire {
            contents,
            generation_config,
            system_instruction,
            tools: wire_tools,
        };

        let mut buf = Vec::with_capacity(estimated_bytes);
        serde_json::to_writer(&mut buf, &wire_req)?;
        Ok(buf)
    }
    fn process_gemini_payload_value(
        val: &serde_json::Value,
        in_reasoning_mode: &mut bool,
    ) -> Result<Vec<StreamChunk>> {
        let mut chunks = Vec::new();

        // 1. Check API-level error inside payload
        if let Some(err) = val.get("error") {
            let code = err.get("code").and_then(|c| c.as_u64()).unwrap_or(500) as u16;
            let message = err
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("Unknown Gemini API error")
                .to_string();
            return Err(ProviderError::Api {
                status: code,
                message: format!("Gemini stream API error: {}", message),
            }
            .into());
        }

        // 2. Check prompt-level policy blocks
        if let Some(feedback) = val.get("promptFeedback") {
            if let Some(reason) = feedback.get("blockReason").and_then(|r| r.as_str()) {
                return Err(ProviderError::Api {
                    status: 400,
                    message: format!("Gemini prompt blocked: blockReason={}", reason),
                }
                .into());
            }
        }

        // 3. Check candidates and candidate finishReason
        if let Some(candidates) = val.get("candidates").and_then(|c| c.as_array()) {
            for candidate in candidates {
                if let Some(reason) = candidate.get("finishReason").and_then(|r| r.as_str()) {
                    if reason == "SAFETY"
                        || reason == "RECITATION"
                        || reason == "BLOCKLIST"
                        || reason == "PROHIBITED_CONTENT"
                    {
                        return Err(ProviderError::Api {
                            status: 400,
                            message: format!(
                                "Gemini candidate generation halted: finishReason={}",
                                reason
                            ),
                        }
                        .into());
                    }
                }
                if let Some(parts) = candidate
                    .get("content")
                    .and_then(|c| c.get("parts"))
                    .and_then(|p| p.as_array())
                {
                    for part in parts {
                        // Handle text delta
                        if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                            if !text.is_empty() {
                                if part.get("thought").and_then(|t| t.as_bool()) == Some(true) {
                                    if !*in_reasoning_mode {
                                        *in_reasoning_mode = true;
                                        chunks.push(StreamChunk::Delta("<thought>".to_string()));
                                    }
                                    chunks.push(StreamChunk::Delta(text.to_string()));
                                } else {
                                    if *in_reasoning_mode {
                                        *in_reasoning_mode = false;
                                        chunks.push(StreamChunk::Delta("</thought>".to_string()));
                                    }
                                    chunks.push(StreamChunk::Delta(text.to_string()));
                                }
                            }
                        }

                        // Handle function / tool call (+ preserve thoughtSignature for Gemini 2.5/3)
                        if let Some(func_call) = part.get("functionCall") {
                            if *in_reasoning_mode {
                                *in_reasoning_mode = false;
                                chunks.push(StreamChunk::Delta("</thought>".to_string()));
                            }
                            if let Some(name) = func_call.get("name").and_then(|n| n.as_str()) {
                                let args = func_call
                                    .get("args")
                                    .cloned()
                                    .unwrap_or_else(|| serde_json::json!({}));
                                let call_id = func_call
                                    .get("id")
                                    .and_then(|i| i.as_str())
                                    .map(|s| s.to_string())
                                    .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
                                if let Some(sig) = part
                                    .get("thoughtSignature")
                                    .or_else(|| part.get("thought_signature"))
                                    .and_then(|s| s.as_str())
                                {
                                    record_thought_signature(&call_id, sig);
                                }
                                let tool_call = ToolCall {
                                    id: call_id,
                                    name: name.to_string(),
                                    arguments: args,
                                };
                                chunks.push(StreamChunk::ToolCallChunk(tool_call));
                            }
                        }
                    }
                }
            }
        }

        // Check usage metadata if provided
        if let Some(usage) = val.get("usageMetadata") {
            let prompt_tokens = usage
                .get("promptTokenCount")
                .and_then(|t| t.as_u64())
                .unwrap_or(0) as usize;
            let completion_tokens = usage
                .get("candidatesTokenCount")
                .and_then(|t| t.as_u64())
                .unwrap_or(0) as usize;
            let cached_prompt_tokens = usage
                .get("cachedContentTokenCount")
                .and_then(|t| t.as_u64())
                .unwrap_or(0) as usize;
            if prompt_tokens > 0 || completion_tokens > 0 || cached_prompt_tokens > 0 {
                chunks.push(StreamChunk::Usage {
                    prompt_tokens,
                    completion_tokens,
                    cached_prompt_tokens,
                });
            } else {
                tracing::debug!(
                    provider = "gemini",
                    "Usage metadata absent or zero in Gemini response (prompt_tokens=0, completion_tokens=0)"
                );
            }
        }

        Ok(chunks)
    }
}

#[async_trait]
impl Provider for GeminiProvider {
    fn name(&self) -> &str {
        "gemini"
    }

    fn default_model(&self) -> &str {
        ":auto"
    }

    async fn stream_completion(
        &self,
        messages: &[Message],
        tools: &[ToolSchema],
        options: &CompletionOptions,
    ) -> Result<ChunkStream> {
        let model = if options.model.is_empty()
            || options.model == ":auto"
            || options.model.starts_with(':')
        {
            crate::agent::models::DynamicModelResolver::resolve_model_sync(
                "gemini",
                Some(&options.model),
            )
        } else {
            options.model.clone()
        };

        let mut resolved_options = options.clone();
        resolved_options.model = model.clone();

        let base = self.base_url.trim_end_matches('/');
        let url = if options.stream {
            format!("{}/models/{}:streamGenerateContent?alt=sse", base, model)
        } else {
            format!("{}/models/{}:generateContent", base, model)
        };

        let request_bytes = Self::serialize_request_bytes(messages, tools, &resolved_options)?;
        let ipv4_client = shared_llm_http_client_ipv4();
        let accept = if options.stream {
            "text/event-stream, application/json"
        } else {
            "application/json"
        };

        let resp = send_request_with_ipv4_fallback(&url, &self.client, &ipv4_client, |c| {
            c.post(&url)
                .header("Content-Type", "application/json")
                .header("Accept", accept)
                .header("x-goog-api-key", &self.api_key)
                .body(request_bytes.clone())
        })
        .await
        .map_err(|e| classify_reqwest_stream_error("Gemini", &e, false))?;

        if !resp.status().is_success() {
            let status = resp.status();
            return Err(
                crate::agent::providers::handle_sse_status_error("Gemini", status, resp).await,
            );
        }

        let content_type = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_ascii_lowercase();

        if !options.stream || content_type.contains("application/json") {
            let raw_text = resp
                .text()
                .await
                .map_err(|e| classify_reqwest_stream_error("Gemini", &e, false))?;
            let val: serde_json::Value = serde_json::from_str(&raw_text).map_err(|e| {
                ProviderError::StreamDecode(format!("Gemini JSON response parse error: {}", e))
            })?;
            let mut in_reasoning = false;
            let mut chunks = if let Some(arr) = val.as_array() {
                let mut all = Vec::new();
                for item in arr {
                    all.extend(Self::process_gemini_payload_value(item, &mut in_reasoning)?);
                }
                all
            } else {
                Self::process_gemini_payload_value(&val, &mut in_reasoning)?
            };
            if in_reasoning {
                chunks.push(StreamChunk::Delta("</thought>".to_string()));
            }
            chunks.push(StreamChunk::Done);
            let stream = async_stream::stream! {
                for chunk in chunks {
                    yield Ok(chunk);
                }
            };
            return Ok(Box::pin(stream));
        }

        let stream = async_stream::stream! {
            let mut byte_stream = resp.bytes_stream();
            let mut decoder = SseLineDecoder::new();
            let mut in_reasoning_mode = false;

            while let Some(chunk_res) = byte_stream.next().await {
                match chunk_res {
                    Ok(bytes) => {
                        for frame in decoder.push_chunk(&bytes) {
                            let data_trimmed = frame.data.trim();
                            if data_trimmed.is_empty() {
                                continue;
                            }
                            if data_trimmed == "[DONE]" {
                                if in_reasoning_mode {
                                    yield Ok(StreamChunk::Delta("</thought>".to_string()));
                                }
                                yield Ok(StreamChunk::Done);
                                return;
                            }

                            let parsed: Result<serde_json::Value> = serde_json::from_str(data_trimmed)
                                .map_err(|e| ProviderError::StreamDecode(format!("JSON parse error: {}", e)).into());

                            match parsed {
                                Ok(val) => {
                                    match Self::process_gemini_payload_value(&val, &mut in_reasoning_mode) {
                                        Ok(chunks) => {
                                            for chunk in chunks {
                                                yield Ok(chunk);
                                            }
                                        }
                                        Err(err) => {
                                            if in_reasoning_mode {
                                                yield Ok(StreamChunk::Delta("</thought>".to_string()));
                                            }
                                            yield Err(err);
                                            return;
                                        }
                                    }
                                }
                                Err(e) => yield Err(e),
                            }
                        }
                    }
                    Err(e) => {
                        yield Err(classify_reqwest_stream_error("Gemini", &e, false));
                        return;
                    }
                }
            }

            for frame in decoder.finish() {
                let data_trimmed = frame.data.trim();
                if data_trimmed.is_empty() || data_trimmed == "[DONE]" {
                    continue;
                }
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(data_trimmed) {
                    match Self::process_gemini_payload_value(&val, &mut in_reasoning_mode) {
                        Ok(chunks) => {
                            for chunk in chunks {
                                yield Ok(chunk);
                            }
                        }
                        Err(err) => {
                            yield Err(err);
                            return;
                        }
                    }
                }
            }

            if in_reasoning_mode {
                yield Ok(StreamChunk::Delta("</thought>".to_string()));
            }
        };

        Ok(Box::pin(stream))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zero_copy_serialize_request_bytes_matches_gemini_wire_shape() {
        let messages = vec![
            Message::user("Inspect repo"),
            Message::assistant_with_tools(
                "",
                vec![ToolCall {
                    id: "call_1".to_string(),
                    name: "read_file".to_string(),
                    arguments: serde_json::json!({"path": "src/main.rs"}),
                }],
            ),
            Message::tool_result("call_1", "read_file", "fn main() {}"),
        ];
        let tools = vec![ToolSchema {
            name: "read_file".to_string(),
            description: "Read file".to_string(),
            parameters: serde_json::json!({"type": "object"}),
        }];
        let options = CompletionOptions {
            model: "gemini-2.5-pro".to_string(),
            temperature: 0.1,
            max_tokens: 8192,
            system_instruction: Some("System prompt".to_string()),
            thinking_budget: Some(1024),
            reasoning_effort: None,
            ..Default::default()
        };

        let wire_bytes =
            GeminiProvider::serialize_request_bytes(&messages, &tools, &options).unwrap();
        let wire_val: serde_json::Value = serde_json::from_slice(&wire_bytes).unwrap();
        assert_eq!(
            wire_val["contents"],
            serde_json::Value::Array(GeminiProvider::format_contents(&messages))
        );
        assert_eq!(
            wire_val["tools"],
            GeminiProvider::format_tools(&tools).unwrap()
        );
        assert_eq!(
            wire_val["generationConfig"]["thinkingConfig"]["thinkingBudget"],
            1024
        );

        let req = reqwest::Client::new()
            .post("http://127.0.0.1:1/models/gemini-2.5-pro:streamGenerateContent?alt=sse")
            .header("Content-Type", "application/json")
            .body(wire_bytes);
        assert!(reqwest_eventsource::EventSource::new(req).is_ok());
    }

    #[test]
    fn test_gemini_thinking_gate_and_thought_signature_roundtrip() {
        // Non-thinking gemini-2.0-flash must omit thinkingConfig and clamp maxOutputTokens to 8192
        let opts_20 = CompletionOptions {
            model: "gemini-2.0-flash".to_string(),
            max_tokens: 16384,
            thinking_budget: Some(2048),
            ..Default::default()
        };
        let bytes_20 =
            GeminiProvider::serialize_request_bytes(&[Message::user("hi")], &[], &opts_20).unwrap();
        let val_20: serde_json::Value = serde_json::from_slice(&bytes_20).unwrap();
        assert!(val_20["generationConfig"].get("thinkingConfig").is_none());
        assert_eq!(val_20["generationConfig"]["maxOutputTokens"], 8192);

        // ThoughtSignature extracted from a functionCall part must be echoed back on the next turn
        let stream_payload = serde_json::json!({
            "candidates": [{
                "content": {
                    "parts": [{
                        "functionCall": {
                            "id": "gem_call_sig_1",
                            "name": "read_file",
                            "args": {"path": "Cargo.toml"}
                        },
                        "thoughtSignature": "ENC_SIG_ABC123"
                    }]
                }
            }]
        });
        let mut in_reasoning = false;
        let chunks =
            GeminiProvider::process_gemini_payload_value(&stream_payload, &mut in_reasoning)
                .unwrap();
        assert!(chunks
            .iter()
            .any(|c| matches!(c, StreamChunk::ToolCallChunk(tc) if tc.id == "gem_call_sig_1")));

        let next_messages = vec![
            Message::user("check cargo"),
            Message::assistant_with_tools(
                "",
                vec![ToolCall {
                    id: "gem_call_sig_1".to_string(),
                    name: "read_file".to_string(),
                    arguments: serde_json::json!({"path": "Cargo.toml"}),
                }],
            ),
            Message::tool_result("gem_call_sig_1", "read_file", "[package]"),
        ];
        let opts_25 = CompletionOptions::new("gemini-2.5-pro");
        let next_bytes =
            GeminiProvider::serialize_request_bytes(&next_messages, &[], &opts_25).unwrap();
        let next_val: serde_json::Value = serde_json::from_slice(&next_bytes).unwrap();
        assert_eq!(
            next_val["contents"][1]["parts"][0]["thoughtSignature"],
            "ENC_SIG_ABC123"
        );
    }
}

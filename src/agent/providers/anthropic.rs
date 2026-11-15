use crate::agent::provider::{ChunkStream, CompletionOptions, Provider, StreamChunk, ToolSchema};
use crate::agent::types::{ImageAttachment, Message, Role};
use crate::error::{ProviderError, Result};
use async_trait::async_trait;
use reqwest_eventsource::{Event, EventSource};
use serde::Serialize;
use std::borrow::Cow;
use tokio_stream::StreamExt;

#[derive(Serialize, Clone, Copy)]
struct CacheControlWire {
    #[serde(rename = "type")]
    kind: &'static str,
}

impl CacheControlWire {
    const fn ephemeral() -> Self {
        Self { kind: "ephemeral" }
    }
}

#[derive(Serialize)]
struct AnthropicImageSourceWire<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    media_type: &'a str,
    data: &'a str,
}

#[derive(Serialize)]
#[serde(untagged)]
enum AnthropicToolResultContentWire<'a> {
    Text(&'a str),
    Blocks(Vec<AnthropicBlockWire<'a>>),
}

#[derive(Serialize)]
#[serde(tag = "type")]
enum AnthropicBlockWire<'a> {
    #[serde(rename = "text")]
    Text {
        text: Cow<'a, str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControlWire>,
    },
    #[serde(rename = "image")]
    Image {
        source: AnthropicImageSourceWire<'a>,
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControlWire>,
    },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: &'a str,
        name: &'a str,
        input: &'a serde_json::Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControlWire>,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: &'a str,
        content: AnthropicToolResultContentWire<'a>,
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControlWire>,
    },
}

impl AnthropicBlockWire<'_> {
    fn set_ephemeral_cache(&mut self) {
        let cc = Some(CacheControlWire::ephemeral());
        match self {
            Self::Text { cache_control, .. }
            | Self::Image { cache_control, .. }
            | Self::ToolUse { cache_control, .. }
            | Self::ToolResult { cache_control, .. } => *cache_control = cc,
        }
    }
}

#[derive(Serialize)]
#[serde(untagged)]
enum AnthropicContentWire<'a> {
    Text(Cow<'a, str>),
    Blocks(Vec<AnthropicBlockWire<'a>>),
}

#[derive(Serialize)]
struct AnthropicMessageWire<'a> {
    role: &'static str,
    content: AnthropicContentWire<'a>,
}

#[derive(Serialize)]
struct AnthropicSystemBlockWire<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    text: Cow<'a, str>,
    cache_control: CacheControlWire,
}

#[derive(Serialize)]
struct AnthropicToolWire<'a> {
    name: &'a str,
    description: &'a str,
    input_schema: &'a serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    cache_control: Option<CacheControlWire>,
}

#[derive(Serialize)]
struct AnthropicThinkingWire {
    #[serde(rename = "type")]
    kind: &'static str,
    budget_tokens: usize,
}

#[derive(Serialize)]
struct AnthropicRequestWire<'a> {
    model: &'a str,
    messages: Vec<AnthropicMessageWire<'a>>,
    max_tokens: usize,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<[AnthropicSystemBlockWire<'a>; 1]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<AnthropicToolWire<'a>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    thinking: Option<AnthropicThinkingWire>,
    temperature: f32,
}

/// Anthropic Messages API Provider implementation (native support for Claude 3.7 Sonnet & extended thinking)
pub struct AnthropicProvider {
    api_key: String,
    client: reqwest::Client,
    base_url: String,
    default_model_name: String,
}

impl AnthropicProvider {
    pub fn new(api_key: impl Into<String>) -> Self {
        let client = crate::agent::providers::shared_llm_http_client();

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
        let client = crate::agent::providers::shared_llm_http_client();

        Self {
            api_key: api_key.into(),
            client,
            base_url: base_url.into(),
            default_model_name: default_model.into(),
        }
    }

    fn build_image_blocks(
        text: &str,
        imgs: &[crate::agent::types::ImageAttachment],
    ) -> Vec<serde_json::Value> {
        let mut blocks = Vec::with_capacity(1 + imgs.len());
        blocks.push(serde_json::json!({
            "type": "text",
            "text": text
        }));
        for img in imgs {
            blocks.push(serde_json::json!({
                "type": "image",
                "source": {
                    "type": "base64",
                    "media_type": img.media_type,
                    "data": img.data_base64
                }
            }));
        }
        blocks
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
                parts.push(trimmed);
            }
        }
        for msg in messages {
            if msg.role == Role::System {
                let trimmed = msg.content.trim();
                if !trimmed.is_empty() {
                    parts.push(trimmed);
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
                        let content = match tool_msg.images {
                            Some(ref imgs) if !imgs.is_empty() => serde_json::Value::Array(
                                Self::build_image_blocks(&tool_msg.content, imgs),
                            ),
                            _ => serde_json::Value::String(tool_msg.content.clone()),
                        };
                        tool_results.push(serde_json::json!({
                            "type": "tool_result",
                            "tool_use_id": tool_use_id,
                            "content": content
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
                    if let Some(ref imgs) = msg.images {
                        if !imgs.is_empty() {
                            let text = if msg.content.is_empty() {
                                "..."
                            } else {
                                &msg.content
                            };
                            anthropic_messages.push(serde_json::json!({
                                "role": "user",
                                "content": Self::build_image_blocks(text, imgs)
                            }));
                            i += 1;
                            continue;
                        }
                    }

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

        // Inject ephemeral cache breakpoint on penultimate user turn if multiple turns exist
        if anthropic_messages.len() >= 3 {
            for msg in anthropic_messages.iter_mut().rev().skip(1) {
                if msg.get("role").and_then(|r| r.as_str()) == Some("user") {
                    if let Some(text) = msg
                        .get("content")
                        .and_then(|c| c.as_str())
                        .map(str::to_string)
                    {
                        msg["content"] = serde_json::json!([
                            {
                                "type": "text",
                                "text": text,
                                "cache_control": {
                                    "type": "ephemeral"
                                }
                            }
                        ]);
                    } else if let Some(arr) = msg.get_mut("content").and_then(|c| c.as_array_mut())
                    {
                        if let Some(last_block) = arr.last_mut() {
                            last_block["cache_control"] = serde_json::json!({
                                "type": "ephemeral"
                            });
                        }
                    }
                    break;
                }
            }
        }

        anthropic_messages
    }

    /// Formats tools into Anthropic tool specification with prompt caching on the last tool
    pub fn format_tools(tools: &[ToolSchema]) -> Option<Vec<serde_json::Value>> {
        if tools.is_empty() {
            return None;
        }

        let mut formatted: Vec<serde_json::Value> = tools
            .iter()
            .map(|t| {
                serde_json::json!({
                    "name": t.name,
                    "description": t.description,
                    "input_schema": t.parameters
                })
            })
            .collect();

        // Inject cache_control on the final tool schema for prompt caching
        if let Some(last_tool) = formatted.last_mut() {
            last_tool["cache_control"] = serde_json::json!({
                "type": "ephemeral"
            });
        }

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
            body["system"] = serde_json::json!([
                {
                    "type": "text",
                    "text": sys,
                    "cache_control": {
                        "type": "ephemeral"
                    }
                }
            ]);
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

    fn build_wire_image_blocks<'a>(
        text: &'a str,
        imgs: &'a [ImageAttachment],
    ) -> Vec<AnthropicBlockWire<'a>> {
        let mut blocks = Vec::with_capacity(1 + imgs.len());
        blocks.push(AnthropicBlockWire::Text {
            text: Cow::Borrowed(text),
            cache_control: None,
        });
        for img in imgs {
            blocks.push(AnthropicBlockWire::Image {
                source: AnthropicImageSourceWire {
                    kind: "base64",
                    media_type: &img.media_type,
                    data: &img.data_base64,
                },
                cache_control: None,
            });
        }
        blocks
    }

    /// Serializes the Anthropic Messages API request directly into a pre-sized `Vec<u8>` using
    /// borrowed wire structs — zero `serde_json::Value` DOM trees and zero base64/message cloning.
    pub fn serialize_request_bytes(
        messages: &[Message],
        tools: &[ToolSchema],
        options: &CompletionOptions,
        default_model: &str,
    ) -> Result<Vec<u8>> {
        let model = if options.model.is_empty() {
            default_model
        } else {
            &options.model
        };

        let mut sys_parts: Vec<&str> = Vec::new();
        if let Some(ref sys) = options.system_instruction {
            let trimmed = sys.trim();
            if !trimmed.is_empty() {
                sys_parts.push(trimmed);
            }
        }
        for msg in messages {
            if msg.role == Role::System {
                let trimmed = msg.content.trim();
                if !trimmed.is_empty() {
                    sys_parts.push(trimmed);
                }
            }
        }
        let system_prompt: Option<Cow<'_, str>> = match sys_parts.as_slice() {
            [] => None,
            [single] => Some(Cow::Borrowed(*single)),
            many => Some(Cow::Owned(many.join("\n\n"))),
        };

        let mut wire_messages: Vec<AnthropicMessageWire<'_>> =
            Vec::with_capacity(messages.len() + 1);
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
                        content_blocks.push(AnthropicBlockWire::Text {
                            text: Cow::Borrowed(&msg.content),
                            cache_control: None,
                        });
                    }
                    if let Some(ref tool_calls) = msg.tool_calls {
                        for tc in tool_calls {
                            content_blocks.push(AnthropicBlockWire::ToolUse {
                                id: &tc.id,
                                name: &tc.name,
                                input: &tc.arguments,
                                cache_control: None,
                            });
                        }
                    }
                    if content_blocks.is_empty() {
                        content_blocks.push(AnthropicBlockWire::Text {
                            text: Cow::Borrowed("..."),
                            cache_control: None,
                        });
                    }
                    wire_messages.push(AnthropicMessageWire {
                        role: "assistant",
                        content: AnthropicContentWire::Blocks(content_blocks),
                    });
                    i += 1;
                }
                Role::Tool => {
                    let mut tool_results = Vec::new();
                    while i < messages.len() && messages[i].role == Role::Tool {
                        let tool_msg = &messages[i];
                        let tool_use_id = tool_msg.tool_call_id.as_deref().unwrap_or("call_1");
                        let content = match tool_msg.images {
                            Some(ref imgs) if !imgs.is_empty() => {
                                AnthropicToolResultContentWire::Blocks(
                                    Self::build_wire_image_blocks(&tool_msg.content, imgs),
                                )
                            }
                            _ => AnthropicToolResultContentWire::Text(&tool_msg.content),
                        };
                        tool_results.push(AnthropicBlockWire::ToolResult {
                            tool_use_id,
                            content,
                            cache_control: None,
                        });
                        i += 1;
                    }

                    if let Some(last) = wire_messages.last_mut() {
                        if last.role == "user" {
                            if let AnthropicContentWire::Blocks(ref mut arr) = last.content {
                                arr.extend(tool_results);
                                continue;
                            }
                        }
                    }

                    wire_messages.push(AnthropicMessageWire {
                        role: "user",
                        content: AnthropicContentWire::Blocks(tool_results),
                    });
                }
                Role::User => {
                    if let Some(ref imgs) = msg.images {
                        if !imgs.is_empty() {
                            let text = if msg.content.is_empty() {
                                "..."
                            } else {
                                &msg.content
                            };
                            wire_messages.push(AnthropicMessageWire {
                                role: "user",
                                content: AnthropicContentWire::Blocks(
                                    Self::build_wire_image_blocks(text, imgs),
                                ),
                            });
                            i += 1;
                            continue;
                        }
                    }

                    if let Some(last) = wire_messages.last_mut() {
                        if last.role == "user" {
                            match last.content {
                                AnthropicContentWire::Text(ref mut prev_text) => {
                                    let merged = format!("{}\n\n{}", prev_text, msg.content);
                                    *prev_text = Cow::Owned(merged);
                                    i += 1;
                                    continue;
                                }
                                AnthropicContentWire::Blocks(ref mut arr) => {
                                    arr.push(AnthropicBlockWire::Text {
                                        text: Cow::Borrowed(&msg.content),
                                        cache_control: None,
                                    });
                                    i += 1;
                                    continue;
                                }
                            }
                        }
                    }

                    wire_messages.push(AnthropicMessageWire {
                        role: "user",
                        content: AnthropicContentWire::Text(Cow::Borrowed(
                            if msg.content.is_empty() {
                                "..."
                            } else {
                                &msg.content
                            },
                        )),
                    });
                    i += 1;
                }
            }
        }

        if wire_messages.first().is_some_and(|m| m.role == "assistant") {
            wire_messages.insert(
                0,
                AnthropicMessageWire {
                    role: "user",
                    content: AnthropicContentWire::Text(Cow::Borrowed(
                        crate::constants::ANTHROPIC_INIT_USER_PROMPT,
                    )),
                },
            );
        }

        if wire_messages.len() >= 3 {
            for msg in wire_messages.iter_mut().rev().skip(1) {
                if msg.role == "user" {
                    match msg.content {
                        AnthropicContentWire::Text(ref mut text_cow) => {
                            let taken = std::mem::replace(text_cow, Cow::Borrowed(""));
                            msg.content =
                                AnthropicContentWire::Blocks(vec![AnthropicBlockWire::Text {
                                    text: taken,
                                    cache_control: Some(CacheControlWire::ephemeral()),
                                }]);
                        }
                        AnthropicContentWire::Blocks(ref mut arr) => {
                            if let Some(last_block) = arr.last_mut() {
                                last_block.set_ephemeral_cache();
                            }
                        }
                    }
                    break;
                }
            }
        }

        let thinking_budget = options
            .thinking_budget
            .filter(|&b| b >= crate::constants::MIN_THINKING_BUDGET_TOKENS);

        let max_tokens = if let Some(budget) = thinking_budget {
            if options.max_tokens <= budget {
                budget + crate::constants::ANTHROPIC_THINKING_HEADROOM_TOKENS
            } else {
                options.max_tokens
            }
        } else {
            options.max_tokens
        };

        let system = system_prompt.map(|sys| {
            [AnthropicSystemBlockWire {
                kind: "text",
                text: sys,
                cache_control: CacheControlWire::ephemeral(),
            }]
        });

        let wire_tools = (!tools.is_empty()).then(|| {
            let last_idx = tools.len().saturating_sub(1);
            tools
                .iter()
                .enumerate()
                .map(|(idx, t)| AnthropicToolWire {
                    name: &t.name,
                    description: &t.description,
                    input_schema: &t.parameters,
                    cache_control: (idx == last_idx).then(CacheControlWire::ephemeral),
                })
                .collect()
        });

        let (thinking, temperature) = match thinking_budget {
            Some(budget) => (
                Some(AnthropicThinkingWire {
                    kind: "enabled",
                    budget_tokens: budget,
                }),
                1.0,
            ),
            None => (None, options.temperature),
        };

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

        let wire_req = AnthropicRequestWire {
            model,
            messages: wire_messages,
            max_tokens,
            stream: true,
            system,
            tools: wire_tools,
            thinking,
            temperature,
        };

        let mut buf = Vec::with_capacity(estimated_bytes);
        serde_json::to_writer(&mut buf, &wire_req)?;
        Ok(buf)
    }

    /// Parses an Anthropic SSE event into StreamChunk items
    pub fn parse_anthropic_event(
        event_type: &str,
        val: &serde_json::Value,
        tool_accumulator: &mut std::collections::BTreeMap<usize, (String, String, String)>,
        prompt_tokens: &mut usize,
        completion_tokens: &mut usize,
        cached_prompt_tokens: &mut usize,
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
                    *cached_prompt_tokens = usage
                        .get("cache_read_input_tokens")
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
                    chunks.push(crate::agent::providers::parse_tool_call_chunk(
                        id, name, args_str,
                    ));
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
                if *prompt_tokens > 0 || *completion_tokens > 0 || *cached_prompt_tokens > 0 {
                    chunks.push(StreamChunk::Usage {
                        prompt_tokens: *prompt_tokens,
                        completion_tokens: *completion_tokens,
                        cached_prompt_tokens: *cached_prompt_tokens,
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
        let request_bytes =
            Self::serialize_request_bytes(messages, tools, options, self.default_model())?;

        let req_builder = self
            .client
            .post(&url)
            .header("content-type", "application/json")
            .header("x-api-key", &self.api_key)
            .header(
                "anthropic-version",
                crate::constants::ANTHROPIC_VERSION_HEADER,
            )
            .body(request_bytes);

        let event_source = EventSource::new(req_builder).map_err(|e| {
            ProviderError::StreamDecode(format!("Failed to connect to Anthropic API: {}", e))
        })?;

        let stream = async_stream::stream! {
            let mut event_source = event_source;
            let mut prompt_tokens = 0usize;
            let mut completion_tokens = 0usize;
            let mut cached_prompt_tokens = 0usize;
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
                            &mut cached_prompt_tokens,
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
                        yield Err(crate::agent::providers::handle_sse_status_error("Anthropic", status, resp).await);
                        return;
                    }
                    Err(e) => {
                        let (completed, had_incomplete) =
                            crate::agent::providers::drain_completed_tool_calls(&mut tool_calls_accumulator);
                        if !completed.is_empty() {
                            for chunk in completed {
                                yield Ok(chunk);
                            }
                            tracing::warn!(
                                error = %e,
                                "Anthropic SSE stream terminated with trailing error after emitting valid tool calls"
                            );
                            break;
                        }
                        if let Some(classified_err) =
                            crate::agent::providers::classify_sse_stream_error("Anthropic", &e, had_incomplete)
                        {
                            yield Err(classified_err);
                            return;
                        }
                        break;
                    }
                }
            }

            if in_thinking_mode {
                yield Ok(StreamChunk::Delta("</thought>".to_string()));
            }

            // Drain any remaining tool calls
            for (_, (id, name, args_str)) in std::mem::take(&mut tool_calls_accumulator) {
                yield Ok(crate::agent::providers::parse_tool_call_chunk(id, name, args_str));
            }
        };

        Ok(Box::pin(stream))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::types::ToolCall;

    #[test]
    fn test_zero_copy_serialize_request_bytes_matches_build_request_body() {
        let messages = vec![
            Message::user("First question"),
            Message::assistant_with_tools(
                "Checking file",
                vec![ToolCall {
                    id: "toolu_1".to_string(),
                    name: "read_file".to_string(),
                    arguments: serde_json::json!({"path": "Cargo.toml"}),
                }],
            ),
            Message::tool_result("toolu_1", "read_file", "[package]\nname = \"minicode\""),
            Message::user("Follow-up with image").with_images(vec![ImageAttachment {
                media_type: "image/png".to_string(),
                data_base64: "iVBORw0KGgoAAAANSUhEUg==".to_string(),
                path: "ui.png".to_string(),
            }]),
        ];
        let tools = vec![
            ToolSchema {
                name: "read_file".to_string(),
                description: "Read file".to_string(),
                parameters: serde_json::json!({"type": "object"}),
            },
            ToolSchema {
                name: "write_file".to_string(),
                description: "Write file".to_string(),
                parameters: serde_json::json!({"type": "object"}),
            },
        ];
        let options = CompletionOptions {
            model: "claude-3-7-sonnet-20250219".to_string(),
            temperature: 0.2,
            max_tokens: 4096,
            system_instruction: Some("System instructions".to_string()),
            thinking_budget: Some(2048),
            reasoning_effort: None,
            ..Default::default()
        };

        let dom_val = AnthropicProvider::build_request_body(
            &messages,
            &tools,
            &options,
            "claude-3-7-sonnet-latest",
        );
        let wire_bytes = AnthropicProvider::serialize_request_bytes(
            &messages,
            &tools,
            &options,
            "claude-3-7-sonnet-latest",
        )
        .unwrap();
        let wire_val: serde_json::Value = serde_json::from_slice(&wire_bytes).unwrap();
        assert_eq!(wire_val, dom_val);

        let req = reqwest::Client::new()
            .post("http://127.0.0.1:1/v1/messages")
            .header("content-type", "application/json")
            .body(wire_bytes);
        assert!(EventSource::new(req).is_ok());
    }
}

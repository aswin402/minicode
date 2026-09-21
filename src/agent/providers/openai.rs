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
        let base_url = std::env::var(crate::constants::ENV_OPENAI_BASE_URL)
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
            "stream_options": {
                "include_usage": true
            }
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
                let effort = if budget <= crate::constants::OPENAI_REASONING_LOW_MAX_TOKENS {
                    "low"
                } else if budget <= crate::constants::OPENAI_REASONING_MEDIUM_MAX_TOKENS {
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
                .header("X-Title", crate::constants::APP_NAME);
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
            let mut accumulated_content = String::new();

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

                            // Fallback: check for Hermes / Ollama inline XML <tool_call> tags, markdown code blocks, or raw JSON
                            if tool_calls_accumulator.is_empty() {
                                let (_, inline_calls) = extract_inline_tool_calls(&accumulated_content);
                                for tc in inline_calls {
                                    yield Ok(StreamChunk::ToolCallChunk(tc));
                                }
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
                                                    accumulated_content.push_str(content);
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
                                    let cached_prompt_tokens = usage
                                        .get("prompt_tokens_details")
                                        .and_then(|d| d.get("cached_tokens"))
                                        .and_then(|t| t.as_u64())
                                        .unwrap_or(0) as usize;
                                    if prompt_tokens > 0 || completion_tokens > 0 || cached_prompt_tokens > 0 {
                                        yield Ok(StreamChunk::Usage { prompt_tokens, completion_tokens, cached_prompt_tokens });
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
                                .or(Some(crate::constants::PROVIDER_RATE_LIMIT_RETRY_DELAY_SECS));
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

            // Fallback: check for Hermes / Ollama inline XML <tool_call> tags if stream ended without explicit [DONE]
            if accumulated_content.contains("<tool_call>") {
                let (_, inline_calls) = extract_inline_tool_calls(&accumulated_content);
                for tc in inline_calls {
                    yield Ok(StreamChunk::ToolCallChunk(tc));
                }
            }
        };

        Ok(Box::pin(stream))
    }
}

fn parse_single_tool_call_value(val: &serde_json::Value) -> Option<ToolCall> {
    let name = val
        .get("name")
        .or_else(|| val.get("tool"))
        .or_else(|| val.get("function"))
        .and_then(|n| n.as_str())?;

    let arguments = if let Some(args) = val
        .get("arguments")
        .or_else(|| val.get("parameters"))
        .or_else(|| val.get("args"))
    {
        if let Some(s) = args.as_str() {
            serde_json::from_str::<serde_json::Value>(s).unwrap_or_else(|_| serde_json::json!({}))
        } else {
            args.clone()
        }
    } else {
        let mut map = serde_json::Map::new();
        if let Some(obj) = val.as_object() {
            for (k, v) in obj {
                if k != "name" && k != "tool" && k != "function" {
                    map.insert(k.clone(), v.clone());
                }
            }
        }
        serde_json::Value::Object(map)
    };

    if !name.trim().is_empty() {
        Some(ToolCall {
            id: format!("call_{}", uuid::Uuid::new_v4().simple()),
            name: name.trim().to_string(),
            arguments,
        })
    } else {
        None
    }
}

/// Extracts inline tool calls from model output:
/// 1. `<tool_call>{"name": ..., "arguments": ...}</tool_call>` tags
/// 2. Markdown codeblocks ```json ... ``` or ```tool_call ... ``` containing tool objects
/// 3. Raw JSON tool objects or arrays emitted directly by local models (e.g. Qwen / Ollama)
///
/// Used for local Hermes, Qwen, and Ollama models that omit SSE delta.tool_calls.
pub fn extract_inline_tool_calls(text: &str) -> (String, Vec<ToolCall>) {
    let mut cleaned = text.to_string();
    let mut tool_calls = Vec::new();

    // 1. Check for <tool_call>...</tool_call> tags
    while let Some(start) = cleaned.find("<tool_call>") {
        let tag_len = "<tool_call>".len();
        if let Some(end) = cleaned[start + tag_len..].find("</tool_call>") {
            let json_str = cleaned[start + tag_len..start + tag_len + end].trim();
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(json_str) {
                if let Some(arr) = val.as_array() {
                    for item in arr {
                        if let Some(tc) = parse_single_tool_call_value(item) {
                            tool_calls.push(tc);
                        }
                    }
                } else if let Some(tc) = parse_single_tool_call_value(&val) {
                    tool_calls.push(tc);
                }
            }
            cleaned.replace_range(start..start + tag_len + end + "</tool_call>".len(), "");
        } else {
            break;
        }
    }

    // 2. Check for markdown codeblocks: ```json ... ``` or ```tool_call ... ```
    let mut search_pos = 0;
    while let Some(open_idx) = cleaned[search_pos..].find("```") {
        let abs_open = search_pos + open_idx;
        let after_open = abs_open + 3;
        if let Some(newline_idx) = cleaned[after_open..].find('\n') {
            let content_start = after_open + newline_idx + 1;
            if let Some(close_idx) = cleaned[content_start..].find("```") {
                let abs_close = content_start + close_idx;
                let block_content = cleaned[content_start..abs_close].trim();

                let mut found_call = false;
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(block_content) {
                    if let Some(arr) = val.as_array() {
                        for item in arr {
                            if let Some(tc) = parse_single_tool_call_value(item) {
                                tool_calls.push(tc);
                                found_call = true;
                            }
                        }
                    } else if let Some(tc) = parse_single_tool_call_value(&val) {
                        tool_calls.push(tc);
                        found_call = true;
                    }
                }

                if found_call {
                    cleaned.replace_range(abs_open..abs_close + 3, "");
                    search_pos = abs_open;
                } else {
                    search_pos = abs_close + 3;
                }
            } else {
                break;
            }
        } else {
            break;
        }
    }

    // 3. Check for raw JSON object or array when text starts with { or [
    if tool_calls.is_empty() {
        let trimmed = cleaned.trim();
        if trimmed.starts_with('{') || trimmed.starts_with('[') {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(trimmed) {
                if let Some(arr) = val.as_array() {
                    for item in arr {
                        if let Some(tc) = parse_single_tool_call_value(item) {
                            tool_calls.push(tc);
                        }
                    }
                    if !tool_calls.is_empty() {
                        cleaned.clear();
                    }
                } else if let Some(tc) = parse_single_tool_call_value(&val) {
                    tool_calls.push(tc);
                    cleaned.clear();
                }
            }
        }
    }

    // 4. Substring raw JSON search if model included text preamble around JSON
    if tool_calls.is_empty() {
        let mut search_pos = 0;
        while let Some(start_offset) = cleaned[search_pos..]
            .find("{\"name\"")
            .or_else(|| cleaned[search_pos..].find("{\"tool\""))
            .or_else(|| cleaned[search_pos..].find("{\"function\""))
        {
            let abs_start = search_pos + start_offset;
            let mut depth = 0;
            let mut in_str = false;
            let mut escape = false;
            let mut end_pos = None;
            for (i, ch) in cleaned[abs_start..].char_indices() {
                if escape {
                    escape = false;
                    continue;
                }
                match ch {
                    '\\' if in_str => escape = true,
                    '"' => in_str = !in_str,
                    '{' if !in_str => depth += 1,
                    '}' if !in_str => {
                        depth -= 1;
                        if depth == 0 {
                            end_pos = Some(abs_start + i + 1);
                            break;
                        }
                    }
                    _ => {}
                }
            }

            if let Some(abs_end) = end_pos {
                let candidate = &cleaned[abs_start..abs_end];
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(candidate) {
                    if let Some(tc) = parse_single_tool_call_value(&val) {
                        tool_calls.push(tc);
                        cleaned.replace_range(abs_start..abs_end, "");
                        search_pos = abs_start;
                        continue;
                    }
                }
                search_pos = abs_end;
            } else {
                break;
            }
        }
    }

    (cleaned, tool_calls)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_inline_tool_calls_tag() {
        let input = "Here is the call: <tool_call>{\"name\": \"write_file\", \"arguments\": {\"path\": \"foo.txt\"}}</tool_call> Done.";
        let (cleaned, calls) = extract_inline_tool_calls(input);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "write_file");
        assert_eq!(calls[0].arguments["path"], "foo.txt");
        assert_eq!(cleaned.trim(), "Here is the call:  Done.");
    }

    #[test]
    fn test_extract_inline_tool_calls_markdown() {
        let input = "I will write the file:\n```json\n{\n  \"name\": \"write_file\",\n  \"arguments\": {\"path\": \"src/server.ts\", \"content\": \"hello\"}\n}\n```\nFinished.";
        let (cleaned, calls) = extract_inline_tool_calls(input);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "write_file");
        assert_eq!(calls[0].arguments["path"], "src/server.ts");
        assert_eq!(calls[0].arguments["content"], "hello");
        assert!(!cleaned.contains("write_file"));
    }

    #[test]
    fn test_extract_inline_tool_calls_raw_json() {
        let input = "{\"name\": \"write_file\", \"arguments\": {\"path\": \"src/routes/health.ts\", \"content\": \"healthy\"}}";
        let (cleaned, calls) = extract_inline_tool_calls(input);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "write_file");
        assert_eq!(calls[0].arguments["path"], "src/routes/health.ts");
        assert_eq!(cleaned.trim(), "");
    }

    #[test]
    fn test_extract_inline_tool_calls_raw_json_with_preamble() {
        let input = "Sure! Here is the tool call:\n{\"name\": \"patch_file\", \"arguments\": {\"path\": \"src/index.ts\"}}\nHope that helps!";
        let (cleaned, calls) = extract_inline_tool_calls(input);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "patch_file");
        assert_eq!(calls[0].arguments["path"], "src/index.ts");
        assert!(!cleaned.contains("patch_file"));
    }
}

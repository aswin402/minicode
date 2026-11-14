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
pub use factory::{
    create_chain_provider_for_slots, create_provider, create_provider_or_fallback,
    create_provider_with_base_url,
};
#[allow(unused_imports)]
pub use gemini::GeminiProvider;
#[allow(unused_imports)]
pub use openai::OpenAiCompatibleProvider;
#[allow(unused_imports)]
pub use resilient::{FallbackChainProvider, FallbackTarget, ResilientProvider};
#[allow(unused_imports)]
pub use unconfigured::UnconfiguredProvider;

use crate::agent::types::{Message, ToolCall};
use crate::error::{MinicodeError, ProviderError, Result};
use async_trait::async_trait;
use futures::Stream;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::pin::Pin;
use std::sync::{OnceLock, RwLock};
use tokio_stream::StreamExt;

/// Losslessly parses a JSON tool-argument string while preserving duplicate object keys
/// (accumulating repeated keys in the same JSON object into a `Value::Array` in wire order)
/// and normalizing XML-to-JSON gateway `<item>` wrapper / spilled-sibling artifacts.
pub(crate) fn parse_tool_args_preserving_duplicates(
    input: &str,
) -> std::result::Result<serde_json::Value, serde_json::Error> {
    struct LosslessJsonSeed;

    impl<'de> serde::de::DeserializeSeed<'de> for LosslessJsonSeed {
        type Value = serde_json::Value;

        fn deserialize<D>(self, deserializer: D) -> std::result::Result<Self::Value, D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            struct LosslessVisitor;

            impl<'de> serde::de::Visitor<'de> for LosslessVisitor {
                type Value = serde_json::Value;

                fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                    formatter.write_str("any valid JSON value")
                }

                fn visit_bool<E>(self, v: bool) -> std::result::Result<Self::Value, E> {
                    Ok(serde_json::Value::Bool(v))
                }

                fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E> {
                    Ok(serde_json::Value::Number(v.into()))
                }

                fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E> {
                    Ok(serde_json::Value::Number(v.into()))
                }

                fn visit_f64<E>(self, v: f64) -> std::result::Result<Self::Value, E> {
                    Ok(serde_json::Number::from_f64(v)
                        .map(serde_json::Value::Number)
                        .unwrap_or(serde_json::Value::Null))
                }

                fn visit_str<E>(self, v: &str) -> std::result::Result<Self::Value, E>
                where
                    E: serde::de::Error,
                {
                    Ok(serde_json::Value::String(v.to_owned()))
                }

                fn visit_string<E>(self, v: String) -> std::result::Result<Self::Value, E> {
                    Ok(serde_json::Value::String(v))
                }

                fn visit_none<E>(self) -> std::result::Result<Self::Value, E> {
                    Ok(serde_json::Value::Null)
                }

                fn visit_unit<E>(self) -> std::result::Result<Self::Value, E> {
                    Ok(serde_json::Value::Null)
                }

                fn visit_seq<A>(self, mut seq: A) -> std::result::Result<Self::Value, A::Error>
                where
                    A: serde::de::SeqAccess<'de>,
                {
                    let mut values = Vec::new();
                    while let Some(elem) = seq.next_element_seed(LosslessJsonSeed)? {
                        values.push(elem);
                    }
                    Ok(serde_json::Value::Array(values))
                }

                fn visit_map<A>(self, mut access: A) -> std::result::Result<Self::Value, A::Error>
                where
                    A: serde::de::MapAccess<'de>,
                {
                    let mut map = serde_json::Map::new();
                    let mut repeated_keys = std::collections::HashSet::new();
                    while let Some(key) = access.next_key::<String>()? {
                        let val = access.next_value_seed(LosslessJsonSeed)?;
                        if let Some(existing) = map.get_mut(&key) {
                            if repeated_keys.insert(key) {
                                let first = std::mem::replace(existing, serde_json::Value::Null);
                                *existing = serde_json::Value::Array(vec![first, val]);
                            } else if let serde_json::Value::Array(arr) = existing {
                                arr.push(val);
                            }
                        } else {
                            map.insert(key, val);
                        }
                    }
                    Ok(serde_json::Value::Object(map))
                }
            }

            deserializer.deserialize_any(LosslessVisitor)
        }
    }

    let mut de = serde_json::Deserializer::from_str(input);
    let raw_val = serde::de::DeserializeSeed::deserialize(LosslessJsonSeed, &mut de)?;
    de.end()?;
    Ok(normalize_xml_gateway_wrappers(raw_val))
}

/// Normalizes XML-to-JSON gateway serialization artifacts structurally:
/// 1. Unwraps nested single-key `{"item": [...]}` list wrappers on object properties.
/// 2. Reattaches spilled sibling `"item"` entries into the schema-matching sibling `Value::Array`
///    and restores any child scalar property missing on a single existing element when present
///    on all spilled items.
pub(crate) fn normalize_xml_gateway_wrappers(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Array(arr) => serde_json::Value::Array(
            arr.into_iter()
                .map(normalize_xml_gateway_wrappers)
                .collect(),
        ),
        serde_json::Value::Object(mut map) => {
            for (_, v) in map.iter_mut() {
                let normalized =
                    normalize_xml_gateway_wrappers(std::mem::replace(v, serde_json::Value::Null));
                *v = normalized;
            }

            // 1. Unwrap nested single-key `{"item": inner}` wrappers on non-`item` fields.
            let keys: Vec<String> = map.keys().cloned().collect();
            for key in keys {
                if key == "item" {
                    continue;
                }
                let unwrap_replacement = match map.get(&key) {
                    Some(serde_json::Value::Object(sub))
                        if sub.len() == 1 && sub.contains_key("item") =>
                    {
                        sub.get("item").cloned().and_then(|inner| match inner {
                            serde_json::Value::Array(_) => Some(inner),
                            other if key.ends_with('s') => {
                                Some(serde_json::Value::Array(vec![other]))
                            }
                            _ => None,
                        })
                    }
                    _ => None,
                };
                if let Some(replaced) = unwrap_replacement {
                    map.insert(key, replaced);
                }
            }

            // 2. Reattach spilled `"item"` entries into the schema-matching sibling array field.
            if map.contains_key("item") && map.len() > 1 {
                let spilled_items: Vec<serde_json::Value> = match map.get("item") {
                    Some(serde_json::Value::Array(items)) => items.clone(),
                    Some(single) => vec![single.clone()],
                    None => Vec::new(),
                };

                if !spilled_items.is_empty() {
                    let spilled_obj_keys: std::collections::HashSet<&str> = spilled_items
                        .iter()
                        .filter_map(|v| v.as_object())
                        .flat_map(|o| o.keys().map(String::as_str))
                        .collect();
                    let all_spilled_are_objs =
                        spilled_items.iter().all(|v| v.is_object()) && !spilled_obj_keys.is_empty();
                    let all_spilled_are_strs = spilled_items.iter().all(|v| v.is_string());

                    let array_sibling_count = map
                        .iter()
                        .filter(|(k, v)| k.as_str() != "item" && v.is_array())
                        .count();

                    let mut best_target: Option<(String, usize)> = None;
                    for (k, v) in &map {
                        if k == "item" {
                            continue;
                        }
                        let Some(existing_arr) = v.as_array() else {
                            continue;
                        };
                        let score = if all_spilled_are_objs {
                            if existing_arr.is_empty() {
                                usize::from(array_sibling_count == 1) * 5
                            } else if existing_arr.iter().all(|e| e.is_object()) {
                                let existing_keys: std::collections::HashSet<&str> = existing_arr
                                    .iter()
                                    .filter_map(|e| e.as_object())
                                    .flat_map(|o| o.keys().map(String::as_str))
                                    .collect();
                                let overlap = existing_keys.intersection(&spilled_obj_keys).count();
                                if overlap > 0 {
                                    10 + overlap
                                } else if array_sibling_count == 1 {
                                    5
                                } else {
                                    0
                                }
                            } else {
                                0
                            }
                        } else if all_spilled_are_strs && array_sibling_count == 1 {
                            if existing_arr.iter().all(|e| e.is_string()) {
                                5
                            } else {
                                0
                            }
                        } else {
                            0
                        };

                        if score > 0
                            && best_target
                                .as_ref()
                                .is_none_or(|(_, best_score)| score > *best_score)
                        {
                            best_target = Some((k.clone(), score));
                        }
                    }

                    if let Some((target_key, _)) = best_target {
                        map.remove("item");
                        if all_spilled_are_objs {
                            // Scalar keys shared by every spilled item object
                            let shared_spilled_keys: Vec<String> = spilled_items
                                .first()
                                .and_then(|v| v.as_object())
                                .map(|first_spilled| {
                                    first_spilled
                                        .keys()
                                        .filter(|sk| {
                                            spilled_items.iter().all(|it| {
                                                it.as_object().is_some_and(|o| {
                                                    o.get(*sk).is_some_and(|sv| {
                                                        sv.is_string()
                                                            || sv.is_number()
                                                            || sv.is_boolean()
                                                    })
                                                })
                                            })
                                        })
                                        .cloned()
                                        .collect()
                                })
                                .unwrap_or_default();

                            for sk in shared_spilled_keys {
                                if sk == target_key {
                                    continue;
                                }
                                let missing_indices: Vec<usize> = map
                                    .get(&target_key)
                                    .and_then(|v| v.as_array())
                                    .map(|arr| {
                                        arr.iter()
                                            .enumerate()
                                            .filter_map(|(idx, elem)| {
                                                elem.as_object()
                                                    .filter(|o| !o.contains_key(&sk))
                                                    .map(|_| idx)
                                            })
                                            .collect()
                                    })
                                    .unwrap_or_default();

                                if missing_indices.is_empty() {
                                    continue;
                                }

                                enum RootScalarRecovery {
                                    DuplicateList {
                                        keep_root: serde_json::Value,
                                        assign: Vec<(usize, serde_json::Value)>,
                                    },
                                    SingleSpilled(usize, serde_json::Value),
                                }

                                let recovery = match map.get(&sk) {
                                    Some(serde_json::Value::Array(dup_vals))
                                        if dup_vals.len() >= 2
                                            && dup_vals
                                                .iter()
                                                .all(|dv| !dv.is_object() && !dv.is_array()) =>
                                    {
                                        let keep_root = dup_vals[0].clone();
                                        let assign = missing_indices
                                            .iter()
                                            .copied()
                                            .zip(dup_vals.iter().skip(1).cloned())
                                            .collect();
                                        Some(RootScalarRecovery::DuplicateList {
                                            keep_root,
                                            assign,
                                        })
                                    }
                                    Some(scalar_val)
                                        if missing_indices.len() == 1
                                            && (scalar_val.is_string()
                                                || scalar_val.is_number()
                                                || scalar_val.is_boolean()) =>
                                    {
                                        Some(RootScalarRecovery::SingleSpilled(
                                            missing_indices[0],
                                            scalar_val.clone(),
                                        ))
                                    }
                                    _ => None,
                                };

                                match recovery {
                                    Some(RootScalarRecovery::DuplicateList {
                                        keep_root,
                                        assign,
                                    }) => {
                                        map.insert(sk.clone(), keep_root);
                                        if let Some(serde_json::Value::Array(arr)) =
                                            map.get_mut(&target_key)
                                        {
                                            for (idx, val) in assign {
                                                if let Some(serde_json::Value::Object(obj)) =
                                                    arr.get_mut(idx)
                                                {
                                                    obj.insert(sk.clone(), val);
                                                }
                                            }
                                        }
                                    }
                                    Some(RootScalarRecovery::SingleSpilled(idx, val)) => {
                                        if let Some(serde_json::Value::Array(arr)) =
                                            map.get_mut(&target_key)
                                        {
                                            if let Some(serde_json::Value::Object(obj)) =
                                                arr.get_mut(idx)
                                            {
                                                obj.insert(sk.clone(), val);
                                            }
                                        }
                                    }
                                    None => {}
                                }
                            }
                        }

                        if let Some(serde_json::Value::Array(arr)) = map.get_mut(&target_key) {
                            arr.extend(spilled_items);
                        }
                    }
                }
            }

            serde_json::Value::Object(map)
        }
        other => other,
    }
}

/// Parses a completed tool call `(id, name, args_str)` into a `StreamChunk::ToolCallChunk`,
/// gracefully wrapping malformed JSON arguments so the agent loop can self-correct.
pub(crate) fn parse_tool_call_chunk(id: String, name: String, args_str: String) -> StreamChunk {
    let parsed_args = match parse_tool_args_preserving_duplicates(&args_str) {
        Ok(val) => val,
        Err(e) => {
            tracing::warn!(
                raw_arguments = %args_str,
                error = %e,
                "Malformed JSON tool call arguments from model"
            );
            serde_json::json!({
                "__json_parse_error": format!("Invalid JSON syntax generated by model: {}", e),
                "__raw": args_str
            })
        }
    };
    StreamChunk::ToolCallChunk(ToolCall {
        id,
        name,
        arguments: parsed_args,
    })
}

/// Drains any valid JSON tool calls accumulated before a mid-stream error.
/// Returns `(completed_chunks, had_incomplete_tool_calls)`.
pub(crate) fn drain_completed_tool_calls(
    acc: &mut BTreeMap<usize, (String, String, String)>,
) -> (Vec<StreamChunk>, bool) {
    let mut chunks = Vec::new();
    let mut had_incomplete = false;
    for (_, (id, name, args_str)) in std::mem::take(acc) {
        if let Ok(val) = parse_tool_args_preserving_duplicates(&args_str) {
            chunks.push(StreamChunk::ToolCallChunk(ToolCall {
                id,
                name,
                arguments: val,
            }));
        } else {
            had_incomplete = true;
        }
    }
    (chunks, had_incomplete)
}

/// Extracts a clean, single-line human-readable error message from an upstream HTTP error body,
/// stripping raw HTML gateway error pages (e.g. Nginx/ALB 502/503/504 HTML documents).
pub(crate) fn summarize_http_error_body(status: reqwest::StatusCode, raw_body: &str) -> String {
    let trimmed = raw_body.trim();
    if trimmed.is_empty() {
        return status
            .canonical_reason()
            .unwrap_or("Upstream HTTP error")
            .to_string();
    }
    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("<!doctype html")
        || lower.starts_with("<html")
        || lower.contains("<title>")
    {
        for (open_tag, close_tag) in [("<title>", "</title>"), ("<h1>", "</h1>")] {
            if let Some(start_idx) = lower.find(open_tag) {
                let content_start = start_idx + open_tag.len();
                if let Some(rel_end) = lower[content_start..].find(close_tag) {
                    let extracted = trimmed[content_start..content_start + rel_end]
                        .split_whitespace()
                        .collect::<Vec<_>>()
                        .join(" ");
                    if !extracted.is_empty() {
                        return extracted;
                    }
                }
            }
        }
        return status
            .canonical_reason()
            .unwrap_or("Upstream gateway error")
            .to_string();
    }
    trimmed.to_string()
}

/// Shared SSE HTTP status code error handler (429 rate-limit + generic status errors).
pub(crate) async fn handle_sse_status_error(
    provider_label: &str,
    status: reqwest::StatusCode,
    resp: reqwest::Response,
) -> MinicodeError {
    let status_code = status.as_u16();
    if status_code == 429 {
        let retry_after = resp
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.trim().parse::<u64>().ok())
            .or(Some(crate::constants::PROVIDER_RATE_LIMIT_RETRY_DELAY_SECS));
        ProviderError::RateLimited {
            retry_after_secs: retry_after,
        }
        .into()
    } else {
        let text = resp.text().await.unwrap_or_default();
        let summary = summarize_http_error_body(status, &text);
        let message = if provider_label.is_empty() {
            summary
        } else {
            format!("{}: {}", provider_label, summary)
        };
        ProviderError::Api {
            status: status_code,
            message,
        }
        .into()
    }
}

/// Shared SSE transport/decode error classifier. Returns `None` on normal `"stream ended"` EOF.
pub(crate) fn classify_sse_stream_error(
    provider_label: &str,
    err: &reqwest_eventsource::Error,
    had_incomplete_tools: bool,
) -> Option<MinicodeError> {
    let err_str = err.to_string();
    if err_str.to_lowercase().contains("stream ended") {
        return None;
    }

    let prefix = if provider_label.is_empty() {
        String::new()
    } else {
        format!("{} ", provider_label)
    };
    let suffix = if had_incomplete_tools {
        " (incomplete tool call truncated mid-stream)"
    } else {
        ""
    };

    let classified = if let reqwest_eventsource::Error::Transport(ref te) = err {
        if te.is_timeout() {
            ProviderError::Network(format!(
                "{}SSE stream read timeout (no data received for {}s){}",
                prefix,
                crate::config::get_active_stream_idle_timeout_secs(),
                suffix
            ))
        } else if te.is_connect() {
            ProviderError::Network(format!(
                "{}SSE stream connection error: {}{}",
                prefix, te, suffix
            ))
        } else {
            ProviderError::StreamDecode(format!(
                "{}SSE stream transport error: {}{}",
                prefix, te, suffix
            ))
        }
    } else {
        ProviderError::StreamDecode(format!("{}SSE stream error: {}{}", prefix, err_str, suffix))
    };

    Some(classified.into())
}

type CachedHttpClient = (u64, u64, reqwest::Client);

static SHARED_LLM_HTTP_CLIENT: OnceLock<RwLock<Option<CachedHttpClient>>> = OnceLock::new();

/// Returns a process-wide shared `reqwest::Client` configured with the active connect and stream-idle
/// timeouts. Shares the `rustls` `webpki-roots` TLS certificate store and HTTP/2 connection pool across
/// the Orchestrator and all `MiniPub` fallback-chain provider instances, rebuilding only if timeout settings change.
pub fn shared_llm_http_client() -> reqwest::Client {
    let connect_secs = crate::config::get_active_connect_timeout_secs();
    let idle_secs = crate::config::get_active_stream_idle_timeout_secs();
    let lock = SHARED_LLM_HTTP_CLIENT.get_or_init(|| RwLock::new(None));

    if let Ok(guard) = lock.read() {
        if let Some((c, i, ref client)) = *guard {
            if c == connect_secs && i == idle_secs {
                return client.clone();
            }
        }
    }

    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(connect_secs))
        .read_timeout(std::time::Duration::from_secs(idle_secs))
        .tcp_keepalive(Some(std::time::Duration::from_secs(30)))
        .build()
        .unwrap_or_default();

    if let Ok(mut guard) = lock.write() {
        *guard = Some((connect_secs, idle_secs, client.clone()));
    }

    client
}

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
            max_tokens: 16384,
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
            max_tokens: 16384,
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
    ToolCallProgress {
        name: String,
        args_preview: String,
    },
    ToolCallChunk(ToolCall),
    Usage {
        prompt_tokens: usize,
        completion_tokens: usize,
        cached_prompt_tokens: usize,
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
                    StreamChunk::ToolCallProgress { .. }
                    | StreamChunk::ToolCallChunk(_)
                    | StreamChunk::Usage { .. }
                    | StreamChunk::Done,
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
                reasoning_content: None,
                images: None,
            },
            Message {
                role: Role::Tool,
                content: "fn main() {}".to_string(),
                tool_calls: None,
                tool_call_id: Some("call_1".to_string()),
                tool_name: Some("read_file".to_string()),
                reasoning_content: None,
                images: None,
            },
            Message {
                role: Role::Tool,
                content: "pub mod agent;".to_string(),
                tool_calls: None,
                tool_call_id: Some("call_2".to_string()),
                tool_name: Some("read_file".to_string()),
                reasoning_content: None,
                images: None,
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

    #[test]
    fn test_summarize_http_error_body_strips_html_and_prevents_duplicate_api_prefix() {
        let html_502 = "<html>\r\n<head><title>502 Bad Gateway</title></head>\r\n<body>\r\n<center><h1>502 Bad Gateway</h1></center>\r\n</body>\r\n</html>";
        let summary = summarize_http_error_body(reqwest::StatusCode::BAD_GATEWAY, html_502);
        assert_eq!(summary, "502 Bad Gateway");

        let err = ProviderError::Api {
            status: 502,
            message: summary,
        };
        assert_eq!(err.to_string(), "API error (502): 502 Bad Gateway");
    }

    #[test]
    fn test_minitest63_lossless_duplicate_keys_and_xml_gateway_wrapper_normalization() {
        let raw_json = r#"{
            "title": "minicode Autonomous AI Coding Agent",
            "steps": [
                {
                    "description": "Scaffold react-vite stack via kit_stack_add",
                    "substeps": ["Run kit_stack_add", "Wire fonts"]
                }
            ],
            "title": "Scaffold React+Vite+Tailwind foundation with design tokens",
            "item": {
                "title": "Build nav + hero with interactive terminal preview",
                "description": "Sticky top nav and live terminal preview",
                "substeps": {
                    "item": ["Nav.tsx", "Hero.tsx", "TerminalPreview.tsx"]
                }
            },
            "item": {
                "title": "Responsive verification at 1440/768/390 + build verification",
                "description": "Run build and verify viewports",
                "substeps": {
                    "item": "Run bun run build and verify 1440/768/390"
                }
            }
        }"#;

        let parsed = parse_tool_args_preserving_duplicates(raw_json).expect("valid lossless JSON");
        assert_eq!(parsed["title"], "minicode Autonomous AI Coding Agent");
        let steps = parsed["steps"].as_array().expect("steps array");
        assert_eq!(steps.len(), 3);
        assert_eq!(
            steps[0]["title"],
            "Scaffold React+Vite+Tailwind foundation with design tokens"
        );
        assert_eq!(
            steps[1]["title"],
            "Build nav + hero with interactive terminal preview"
        );
        assert_eq!(
            steps[1]["substeps"].as_array().unwrap().len(),
            3,
            "nested `substeps: {{item: [...]}}` must unwrap cleanly"
        );
        assert_eq!(
            steps[2]["title"],
            "Responsive verification at 1440/768/390 + build verification"
        );
        assert_eq!(steps[2]["substeps"].as_array().unwrap().len(), 1);
        assert!(parsed.get("item").is_none());
    }
}

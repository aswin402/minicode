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
#[allow(dead_code)]
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

/// Classifies a `reqwest::Error` encountered while reading an HTTP/SSE response stream.
pub(crate) fn classify_reqwest_stream_error(
    provider_label: &str,
    err: &reqwest::Error,
    had_incomplete_tools: bool,
) -> MinicodeError {
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

    if err.is_timeout() {
        ProviderError::Network(format!(
            "{}SSE stream read timeout (no data received for {}s){}",
            prefix,
            crate::config::get_active_stream_idle_timeout_secs(),
            suffix
        ))
        .into()
    } else if err.is_connect() {
        ProviderError::Network(format!(
            "{}SSE stream connection error: {}{}",
            prefix, err, suffix
        ))
        .into()
    } else {
        ProviderError::StreamDecode(format!(
            "{}SSE stream transport error: {}{}",
            prefix, err, suffix
        ))
        .into()
    }
}

/// Single Server-Sent Events (SSE) frame parsed from a raw HTTP byte stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SseFrame {
    pub event: String,
    pub data: String,
}

/// Tolerant, pure-Rust incremental SSE decoder operating directly on chunked HTTP bytes.
/// Handles `\r\n` and `\n`, multi-line `data:` blocks, UTF-8 characters split across TCP frames,
/// SSE comment heartbeats (`: keep-alive`), and missing trailing blank lines at EOF — without
/// failing when upstream proxies strip or alter `Content-Type` headers.
#[derive(Debug, Default)]
pub(crate) struct SseLineDecoder {
    buf: Vec<u8>,
    current_event: String,
    data_lines: Vec<String>,
}

impl SseLineDecoder {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Appends a raw TCP/HTTP byte chunk and returns any complete SSE frames decoded so far.
    pub(crate) fn push_chunk(&mut self, chunk: &[u8]) -> Vec<SseFrame> {
        self.buf.extend_from_slice(chunk);
        let mut frames = Vec::new();

        let mut start = 0usize;
        while let Some(rel_nl) = self.buf[start..].iter().position(|&b| b == b'\n') {
            let line_end = start + rel_nl;
            let mut line_slice = &self.buf[start..line_end];
            if line_slice.ends_with(b"\r") {
                line_slice = &line_slice[..line_slice.len() - 1];
            }
            let line = String::from_utf8_lossy(line_slice).into_owned();
            if let Some(frame) = self.process_line(&line) {
                frames.push(frame);
            }
            start = line_end + 1;
        }

        if start > 0 {
            self.buf.drain(..start);
        }

        frames
    }

    /// Flushes any trailing unterminated line or pending `data:` frame at EOF.
    pub(crate) fn finish(&mut self) -> Vec<SseFrame> {
        let mut frames = Vec::new();
        if !self.buf.is_empty() {
            let mut slice = self.buf.as_slice();
            if slice.ends_with(b"\r") {
                slice = &slice[..slice.len() - 1];
            }
            let line = String::from_utf8_lossy(slice).into_owned();
            self.buf.clear();
            if let Some(frame) = self.process_line(&line) {
                frames.push(frame);
            }
        }
        if let Some(frame) = self.flush_frame() {
            frames.push(frame);
        }
        frames
    }

    fn process_line(&mut self, line: &str) -> Option<SseFrame> {
        // Strip UTF-8 BOM if present on the first line of the stream
        let line = line.strip_prefix('\u{feff}').unwrap_or(line);
        if line.is_empty() {
            return self.flush_frame();
        }
        if line.starts_with(':') {
            // SSE comment / keep-alive heartbeat
            return None;
        }
        let (field, raw_val) = match line.split_once(':') {
            Some((f, v)) => (f, v.strip_prefix(' ').unwrap_or(v)),
            None => (line, ""),
        };
        match field {
            "event" => {
                self.current_event.clear();
                self.current_event.push_str(raw_val);
            }
            "data" => {
                self.data_lines.push(raw_val.to_string());
            }
            _ => {}
        }
        None
    }

    fn flush_frame(&mut self) -> Option<SseFrame> {
        if self.data_lines.is_empty() {
            self.current_event.clear();
            return None;
        }
        let data = self.data_lines.join("\n");
        self.data_lines.clear();
        let event = std::mem::take(&mut self.current_event);
        Some(SseFrame { event, data })
    }
}

/// Canonicalizes provider aliases into a stable lowercase key for endpoint/model caching.
#[must_use]
pub fn canonical_provider_key(provider: &str) -> String {
    match provider.trim().to_ascii_lowercase().as_str() {
        "claude" | "anthropic" => "anthropic".to_string(),
        "google" | "gemini" => "gemini".to_string(),
        "z.ai" | "z_ai" => "z.ai".to_string(),
        "zhipu" | "glm" | "bigmodel" => "zhipu".to_string(),
        "kimi" | "moonshot" | "moonshotai" => "kimi".to_string(),
        "hf" | "huggingface" => "huggingface".to_string(),
        "nim" | "nvidia-nim" | "nvidia" => "nvidia".to_string(),
        "opencode-zen" | "zen" | "opencode" => "opencode".to_string(),
        "agnes-ai" | "agnes" => "agnes".to_string(),
        "nararouter" | "bynara" | "nara" => "nara".to_string(),
        "ofoxai" | "ofox" => "ofox".to_string(),
        "lm-studio" | "lmstudio" => "lmstudio".to_string(),
        other => other.to_string(),
    }
}

static RESOLVED_PROVIDER_BASE_URLS: OnceLock<RwLock<std::collections::HashMap<String, String>>> =
    OnceLock::new();

fn resolved_base_url_map() -> &'static RwLock<std::collections::HashMap<String, String>> {
    RESOLVED_PROVIDER_BASE_URLS.get_or_init(|| RwLock::new(std::collections::HashMap::new()))
}

/// Remembers a verified working regional base URL for `provider` so subsequent `/models`
/// and chat completion calls hit the exact working region without extra latency.
pub fn record_working_provider_base_url(provider: &str, base_url: &str) {
    let trimmed = base_url.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return;
    }
    let key = canonical_provider_key(provider);
    if let Ok(mut guard) = resolved_base_url_map().write() {
        guard.insert(key, trimmed.to_string());
    }
}

/// Returns known regional base URLs for multi-region cloud providers (`minimax`, `z.ai`, `zhipu`, `kimi`).
#[must_use]
pub(crate) fn known_regional_urls_for_provider(provider: &str) -> &'static [&'static str] {
    match provider.trim().to_ascii_lowercase().as_str() {
        "minimax" => &[
            crate::constants::MINIMAX_BASE_URL,
            crate::constants::MINIMAX_CN_BASE_URL,
            crate::constants::MINIMAX_LEGACY_BASE_URL,
        ],
        "z.ai" | "z_ai" => &[
            crate::constants::ZAI_GLOBAL_BASE_URL,
            crate::constants::ZAI_CODING_BASE_URL,
            crate::constants::ZHIPU_BASE_URL,
            crate::constants::ZHIPU_CODING_BASE_URL,
        ],
        "zhipu" | "glm" | "bigmodel" => &[
            crate::constants::ZHIPU_BASE_URL,
            crate::constants::ZHIPU_CODING_BASE_URL,
            crate::constants::ZAI_GLOBAL_BASE_URL,
            crate::constants::ZAI_CODING_BASE_URL,
        ],
        "kimi" | "moonshot" | "moonshotai" => &[
            crate::constants::KIMI_BASE_URL,
            crate::constants::KIMI_CN_BASE_URL,
        ],
        _ => &[],
    }
}

/// Computes the ordered list of candidate base URLs for a provider call.
/// If `configured_base_url` is a user-defined custom URL (not one of the provider's built-in
/// regional endpoints), returns only `[configured_base_url]` so explicit proxies are never bypassed.
#[must_use]
pub(crate) fn candidate_base_urls_for_provider(
    provider: &str,
    configured_base_url: &str,
) -> Vec<String> {
    let trimmed = configured_base_url.trim().trim_end_matches('/');
    let regional = known_regional_urls_for_provider(provider);
    if regional.is_empty() {
        return vec![trimmed.to_string()];
    }

    let is_known_builtin = regional
        .iter()
        .any(|u| u.trim_end_matches('/').eq_ignore_ascii_case(trimmed));
    if !is_known_builtin {
        return vec![trimmed.to_string()];
    }

    let mut out = Vec::with_capacity(regional.len() + 1);
    let key = canonical_provider_key(provider);
    if let Ok(guard) = resolved_base_url_map().read() {
        if let Some(cached) = guard.get(&key) {
            if regional
                .iter()
                .any(|u| u.trim_end_matches('/').eq_ignore_ascii_case(cached))
            {
                out.push(cached.clone());
            }
        }
    }
    if !out.iter().any(|u| u.eq_ignore_ascii_case(trimmed)) {
        out.push(trimmed.to_string());
    }
    for &alt in regional {
        let alt_clean = alt.trim_end_matches('/');
        if !out.iter().any(|u| u.eq_ignore_ascii_case(alt_clean)) {
            out.push(alt_clean.to_string());
        }
    }
    out
}

type CachedHttpClient = (u64, u64, reqwest::Client);

static SHARED_LLM_HTTP_CLIENT: OnceLock<RwLock<Option<CachedHttpClient>>> = OnceLock::new();
static SHARED_LLM_HTTP_CLIENT_IPV4: OnceLock<RwLock<Option<CachedHttpClient>>> = OnceLock::new();
static SHARED_MODEL_FETCH_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
static SHARED_MODEL_FETCH_CLIENT_IPV4: OnceLock<reqwest::Client> = OnceLock::new();
static PREFER_IPV4_NETWORK: OnceLock<std::sync::atomic::AtomicBool> = OnceLock::new();

fn prefer_ipv4_flag() -> &'static std::sync::atomic::AtomicBool {
    PREFER_IPV4_NETWORK.get_or_init(|| {
        let env_force = std::env::var("MINICODE_FORCE_IPV4")
            .ok()
            .map(|v| {
                matches!(
                    v.trim().to_ascii_lowercase().as_str(),
                    "1" | "true" | "yes" | "on"
                )
            })
            .unwrap_or(false);
        std::sync::atomic::AtomicBool::new(env_force)
    })
}

/// Returns true when the current process should route outbound HTTP requests over IPv4 first
/// (either via `MINICODE_FORCE_IPV4=1` or after an automatic dual-stack -> IPv4 fallback succeeded).
#[must_use]
pub fn prefer_ipv4() -> bool {
    prefer_ipv4_flag().load(std::sync::atomic::Ordering::Relaxed)
}

/// Records that the active network path works cleanly over IPv4 after dual-stack failed.
pub fn mark_prefer_ipv4() {
    prefer_ipv4_flag().store(true, std::sync::atomic::Ordering::Relaxed);
}

fn build_llm_client(connect_secs: u64, idle_secs: u64, force_ipv4: bool) -> reqwest::Client {
    let mut builder = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(connect_secs))
        .read_timeout(std::time::Duration::from_secs(idle_secs))
        .tcp_keepalive(Some(std::time::Duration::from_secs(30)))
        .tcp_nodelay(true)
        .pool_idle_timeout(Some(std::time::Duration::from_secs(25)))
        .pool_max_idle_per_host(4);

    if force_ipv4 {
        builder = builder.local_address(std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED));
    }

    builder.build().unwrap_or_default()
}

/// Returns a process-wide shared `reqwest::Client` configured with the active connect and stream-idle
/// timeouts, `tcp_nodelay`, and CGNAT-safe 25s pool idle expiry. Shares the `rustls` `webpki-roots`
/// TLS certificate store across the Orchestrator and all `MiniPub` instances.
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

    let client = build_llm_client(connect_secs, idle_secs, false);

    if let Ok(mut guard) = lock.write() {
        *guard = Some((connect_secs, idle_secs, client.clone()));
    }

    client
}

/// Returns a process-wide shared IPv4-bound `reqwest::Client` for automatic recovery on networks
/// with broken IPv6 routes, PMTU blackholes, or slow dual-stack TLS handshakes.
pub fn shared_llm_http_client_ipv4() -> reqwest::Client {
    let connect_secs = crate::config::get_active_connect_timeout_secs();
    let idle_secs = crate::config::get_active_stream_idle_timeout_secs();
    let lock = SHARED_LLM_HTTP_CLIENT_IPV4.get_or_init(|| RwLock::new(None));

    if let Ok(guard) = lock.read() {
        if let Some((c, i, ref client)) = *guard {
            if c == connect_secs && i == idle_secs {
                return client.clone();
            }
        }
    }

    let client = build_llm_client(connect_secs, idle_secs, true);

    if let Ok(mut guard) = lock.write() {
        *guard = Some((connect_secs, idle_secs, client.clone()));
    }

    client
}

/// Returns a shared `reqwest::Client` tuned for `/models` and `models.dev` catalog fetches.
pub(crate) fn shared_model_fetch_http_client(force_ipv4: bool) -> reqwest::Client {
    let build = |v4: bool| {
        let mut builder = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(
                crate::constants::MODEL_FETCH_CONNECT_TIMEOUT_SECS,
            ))
            .timeout(std::time::Duration::from_secs(
                crate::constants::MODEL_FETCH_TIMEOUT_SECS,
            ))
            .tcp_keepalive(Some(std::time::Duration::from_secs(20)))
            .tcp_nodelay(true)
            .pool_idle_timeout(Some(std::time::Duration::from_secs(20)))
            .pool_max_idle_per_host(4);
        if v4 {
            builder = builder.local_address(std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED));
        }
        builder.build().unwrap_or_default()
    };

    if force_ipv4 {
        SHARED_MODEL_FETCH_CLIENT_IPV4
            .get_or_init(|| build(true))
            .clone()
    } else {
        SHARED_MODEL_FETCH_CLIENT
            .get_or_init(|| build(false))
            .clone()
    }
}

fn url_targets_explicit_ipv6(url: &str) -> bool {
    url.contains("://[")
}

/// Returns true when a `reqwest::Error` indicates a network/connect/TLS/timeout failure that
/// frequently resolves when retrying over an IPv4-only socket on broken dual-stack networks.
#[must_use]
pub(crate) fn is_ipv4_fallback_candidate_error(err: &reqwest::Error) -> bool {
    if err.is_connect() || err.is_timeout() {
        return true;
    }
    let msg = err.to_string().to_ascii_lowercase();
    msg.contains("dns")
        || msg.contains("lookup")
        || msg.contains("network unreachable")
        || msg.contains("host unreachable")
        || msg.contains("connection reset")
        || msg.contains("connection refused")
        || msg.contains("broken pipe")
        || msg.contains("unexpected eof")
        || msg.contains("handshake")
        || msg.contains("timed out")
        || msg.contains("error sending request")
}

/// Executes an HTTP request with transparent dual-stack -> IPv4 fallback when a network has
/// broken IPv6 routing or stale pooled sockets.
pub(crate) async fn send_request_with_ipv4_fallback<F>(
    url: &str,
    primary_client: &reqwest::Client,
    ipv4_client: &reqwest::Client,
    build_req: F,
) -> std::result::Result<reqwest::Response, reqwest::Error>
where
    F: Fn(&reqwest::Client) -> reqwest::RequestBuilder,
{
    let allow_ipv4 = !url_targets_explicit_ipv6(url);
    if allow_ipv4 && prefer_ipv4() {
        return build_req(ipv4_client).send().await;
    }

    match build_req(primary_client).send().await {
        Ok(resp) => Ok(resp),
        Err(first_err) if allow_ipv4 && is_ipv4_fallback_candidate_error(&first_err) => {
            tracing::warn!(
                url = %url,
                error = %first_err,
                "Primary network request failed; retrying via IPv4-only socket"
            );
            match build_req(ipv4_client).send().await {
                Ok(resp) => {
                    mark_prefer_ipv4();
                    tracing::info!(
                        url = %url,
                        "IPv4 fallback succeeded; preferring IPv4 for this session"
                    );
                    Ok(resp)
                }
                Err(_) => Err(first_err),
            }
        }
        Err(err) => Err(err),
    }
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
    /// Deterministic KV-cache slot / prefix-router affinity key (`LMCache` / `vLLM` / `Ollama` / `OpenAI`).
    pub prompt_cache_key: Option<String>,
    /// Whether to request SSE streaming (`true`) or a single JSON response (`false`, safer on SSE-hostile proxies).
    pub stream: bool,
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
            prompt_cache_key: None,
            stream: true,
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
            prompt_cache_key: None,
            stream: true,
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

    pub fn with_prompt_cache_key(mut self, key: impl Into<String>) -> Self {
        self.prompt_cache_key = Some(key.into());
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
    use crate::agent::types::Message;

    #[test]
    fn test_gemini_format_merges_consecutive_tool_messages() {
        let messages = vec![
            Message::user("Please read two files"),
            Message::assistant_with_tools(
                "",
                vec![
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
                ],
            ),
            Message::tool_result("call_1", "read_file", "fn main() {}"),
            Message::tool_result("call_2", "read_file", "pub mod agent;"),
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

    #[test]
    fn test_sse_line_decoder_handles_crlf_chunked_utf8_comments_and_eof() {
        let mut decoder = SseLineDecoder::new();
        // Split a multi-byte UTF-8 emoji across two TCP chunks with CRLF endings and comments
        let raw = b": keep-alive\r\nevent: content_block_delta\r\ndata: {\"text\":\"hi \xF0\x9F\x9A\x80\"}\r\n\r\ndata: {\"done\":true}";
        let mut frames = decoder.push_chunk(&raw[..50]);
        frames.extend(decoder.push_chunk(&raw[50..]));
        frames.extend(decoder.finish());

        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].event, "content_block_delta");
        assert_eq!(frames[0].data, "{\"text\":\"hi 🚀\"}");
        assert_eq!(frames[1].event, "");
        assert_eq!(frames[1].data, "{\"done\":true}");
    }

    #[test]
    fn test_candidate_base_urls_regional_failover_and_custom_proxy_preservation() {
        let zai = candidate_base_urls_for_provider("z.ai", crate::constants::ZAI_GLOBAL_BASE_URL);
        assert!(zai.len() >= 3);
        assert_eq!(zai[0], crate::constants::ZAI_GLOBAL_BASE_URL);
        assert!(zai.iter().any(|u| u == crate::constants::ZHIPU_BASE_URL));

        let minimax =
            candidate_base_urls_for_provider("minimax", crate::constants::MINIMAX_BASE_URL);
        assert!(minimax.len() >= 3);
        assert!(minimax
            .iter()
            .any(|u| u == crate::constants::MINIMAX_CN_BASE_URL));
        assert!(minimax
            .iter()
            .any(|u| u == crate::constants::MINIMAX_LEGACY_BASE_URL));

        // Explicit custom proxy URLs must NEVER be silently rerouted to built-in regional URLs
        let custom = candidate_base_urls_for_provider("z.ai", "https://my-corp-proxy.internal/v1");
        assert_eq!(
            custom,
            vec!["https://my-corp-proxy.internal/v1".to_string()]
        );
    }
}

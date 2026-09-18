#![allow(dead_code)]

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::agent::types::{Message, Role, ToolCall};
use crate::context::budget::ccr_cache::CcrCache;

/// Metrics tracking semantic micro-compaction savings and operations performed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MicroCompactMetrics {
    pub superseded_reads_compacted: usize,
    pub duplicate_reads_compacted: usize,
    pub mutation_echoes_compacted: usize,
    pub search_results_compacted: usize,
    pub tokens_saved_estimate: usize,
}

impl MicroCompactMetrics {
    /// Total number of tool results compacted across all micro-compaction rules.
    #[must_use]
    pub fn total_compacted(&self) -> usize {
        self.superseded_reads_compacted
            + self.duplicate_reads_compacted
            + self.mutation_echoes_compacted
            + self.search_results_compacted
    }
}

/// Dynamic key names for target file path extraction from tool call arguments.
const PATH_KEYS: &[&str] = &[
    "path",
    "target_file",
    "file_path",
    "file",
    "TargetFile",
    "AbsolutePath",
    "target",
];

/// Dynamic key names for search query extraction from tool call arguments.
const QUERY_KEYS: &[&str] = &["query", "pattern", "term", "regex", "Query", "Pattern"];

/// Returns true if the tool name indicates a file mutation operation.
fn is_mutation_tool(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    matches!(
        n.as_str(),
        "write_file"
            | "patch_file"
            | "replace_file_content"
            | "edit_file"
            | "repair_patch"
            | "write_to_file"
    ) || n.starts_with("write_")
        || n.starts_with("patch_")
        || n.starts_with("edit_")
}

/// Returns true if the tool name indicates a file read operation.
fn is_read_tool(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    matches!(n.as_str(), "read_file" | "view_file" | "cat")
        || n.starts_with("read_")
        || n.starts_with("view_")
}

/// Returns true if the tool name indicates a search or grep operation.
fn is_search_tool(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    matches!(
        n.as_str(),
        "grep_search" | "find_by_name" | "file_search" | "glob" | "grep" | "search"
    ) || n.contains("grep")
        || n.contains("search")
}

/// Normalizes a file path for cross-platform comparison by replacing `\\` with `/`,
/// stripping leading `./`, and trimming leading `/`.
pub fn normalize_path_for_compare(path: &str) -> String {
    let s = path.trim().replace('\\', "/");
    let mut s = s.as_str();
    loop {
        if let Some(stripped) = s.strip_prefix("./") {
            s = stripped;
        } else if let Some(stripped) = s.strip_prefix('/') {
            s = stripped;
        } else {
            break;
        }
    }
    s.to_string()
}

#[derive(Debug, Clone)]
struct CompactToolMeta {
    name: String,
    target_path: Option<String>,
    query: Option<String>,
}

fn is_compacted_receipt(content: &str) -> bool {
    content.starts_with('[')
        && (content.contains("Use retrieve_observation(id=\"")
            || content.contains("superseded by")
            || content.contains("successfully applied,"))
}

fn is_already_receipt(content: &str, tool_name: &str) -> bool {
    content
        .strip_prefix('[')
        .and_then(|s| s.strip_prefix(tool_name))
        .map(|s| s.starts_with(':'))
        .unwrap_or(false)
}

fn resolve_tool_call_meta(
    messages: &[Message],
    idx: usize,
    tool_meta_by_id: &HashMap<String, CompactToolMeta>,
) -> Option<CompactToolMeta> {
    if idx >= messages.len() {
        return None;
    }

    let msg = &messages[idx];

    if let Some(meta) = msg
        .tool_call_id
        .as_deref()
        .and_then(|id| tool_meta_by_id.get(id))
    {
        return Some(meta.clone());
    }

    let expected_name = msg.tool_name.as_deref();

    for prev_idx in (0..idx).rev() {
        let prev = &messages[prev_idx];
        if prev.role == Role::Assistant {
            if let Some(ref calls) = prev.tool_calls {
                let matched_call = if let Some(tname) = expected_name {
                    calls.iter().find(|c| {
                        c.name == tname
                            || (is_read_tool(&c.name) && is_read_tool(tname))
                            || (is_mutation_tool(&c.name) && is_mutation_tool(tname))
                            || (is_search_tool(&c.name) && is_search_tool(tname))
                    })
                } else {
                    calls.first()
                };

                if let Some(c) = matched_call {
                    return Some(CompactToolMeta {
                        name: expected_name.unwrap_or(&c.name).to_string(),
                        target_path: MicroCompactor::extract_target_path(c),
                        query: MicroCompactor::extract_search_query(c),
                    });
                }
            }
            break;
        }
    }

    expected_name.map(|tname| CompactToolMeta {
        name: tname.to_string(),
        target_path: None,
        query: None,
    })
}

/// Semantic micro-compactor that performs granular, lossless condensation of stale or redundant
/// tool observations in conversation history, backed by lossless CCR recovery cache.
pub struct MicroCompactor;

impl MicroCompactor {
    /// Compacts messages by identifying superseded file reads, duplicate consecutive reads,
    /// and large historical mutation echoes outside the preserved recent turn window.
    ///
    /// Preserves the last `preserve_recent_turns` turns untouched. If `preserve_recent_turns == 0`,
    /// all messages across the conversation are eligible for compaction.
    pub fn compact_messages(
        messages: &mut [Message],
        preserve_recent_turns: usize,
    ) -> MicroCompactMetrics {
        let mut metrics = MicroCompactMetrics::default();
        if messages.is_empty() {
            return metrics;
        }

        let cutoff = Self::calculate_cutoff(messages, preserve_recent_turns);
        if cutoff == 0 {
            return metrics;
        }

        Self::compact_stale_observations(messages, cutoff, &mut metrics);

        metrics
    }

    /// Dynamically extracts target file path from a tool call's arguments.
    #[must_use]
    pub fn extract_target_path(tool_call: &ToolCall) -> Option<String> {
        let check_obj = |obj: &serde_json::Map<String, serde_json::Value>| -> Option<String> {
            for key in PATH_KEYS {
                if let Some(val) = obj.get(*key) {
                    if let Some(s) = val.as_str() {
                        let trimmed = s.trim();
                        if !trimmed.is_empty() {
                            return Some(trimmed.to_string());
                        }
                    }
                }
            }
            None
        };

        if let Some(obj) = tool_call.arguments.as_object() {
            if let Some(path) = check_obj(obj) {
                return Some(path);
            }
        } else if let Some(s) = tool_call.arguments.as_str() {
            if let Ok(serde_json::Value::Object(obj)) = serde_json::from_str::<serde_json::Value>(s)
            {
                if let Some(path) = check_obj(&obj) {
                    return Some(path);
                }
            }
        }

        None
    }

    /// Dynamically extracts search query or pattern from a tool call's arguments.
    #[must_use]
    pub fn extract_search_query(tool_call: &ToolCall) -> Option<String> {
        let check_obj = |obj: &serde_json::Map<String, serde_json::Value>| -> Option<String> {
            for key in QUERY_KEYS {
                if let Some(val) = obj.get(*key) {
                    if let Some(s) = val.as_str() {
                        let trimmed = s.trim();
                        if !trimmed.is_empty() {
                            return Some(trimmed.to_string());
                        }
                    }
                }
            }
            None
        };

        if let Some(obj) = tool_call.arguments.as_object() {
            if let Some(query) = check_obj(obj) {
                return Some(query);
            }
        } else if let Some(s) = tool_call.arguments.as_str() {
            if let Ok(serde_json::Value::Object(obj)) = serde_json::from_str::<serde_json::Value>(s)
            {
                if let Some(query) = check_obj(&obj) {
                    return Some(query);
                }
            }
        }

        None
    }

    /// Calculates the message index cutoff before which messages are eligible for compaction.
    /// Messages at or after `cutoff` belong to the preserved recent turns and are left untouched.
    fn calculate_cutoff(messages: &[Message], preserve_recent_turns: usize) -> usize {
        if preserve_recent_turns == 0 {
            return messages.len();
        }

        let user_indices: Vec<usize> = messages
            .iter()
            .enumerate()
            .filter_map(|(i, m)| if m.role == Role::User { Some(i) } else { None })
            .collect();

        if !user_indices.is_empty() {
            if preserve_recent_turns >= user_indices.len() {
                0
            } else {
                user_indices[user_indices.len() - preserve_recent_turns]
            }
        } else {
            let assistant_indices: Vec<usize> = messages
                .iter()
                .enumerate()
                .filter_map(|(i, m)| {
                    if m.role == Role::Assistant {
                        Some(i)
                    } else {
                        None
                    }
                })
                .collect();
            if preserve_recent_turns >= assistant_indices.len() {
                0
            } else {
                assistant_indices[assistant_indices.len() - preserve_recent_turns]
            }
        }
    }

    /// Identifies and condenses stale tool observations outside the preserved window:
    /// 1. Duplicate consecutive reads (superseded by subsequent read without modifications in between).
    /// 2. Superseded reads (stale read observations superseded by subsequent file modifications).
    /// 3. Historical mutation echoes (verbose diffs or write echoes > 256 bytes).
    fn compact_stale_observations(
        messages: &mut [Message],
        cutoff: usize,
        metrics: &mut MicroCompactMetrics,
    ) {
        // Pass 1a: Build map of lightweight tool metadata and indexed mutations across the conversation
        let mut tool_calls_by_id: HashMap<String, CompactToolMeta> = HashMap::new();
        let mut mutations: HashMap<String, Vec<usize>> = HashMap::new();
        let mut reads: HashMap<String, Vec<usize>> = HashMap::new();

        for (idx, msg) in messages.iter().enumerate() {
            if let Some(ref calls) = msg.tool_calls {
                for call in calls {
                    let target_path = Self::extract_target_path(call);
                    let query = Self::extract_search_query(call);
                    let meta = CompactToolMeta {
                        name: call.name.clone(),
                        target_path,
                        query,
                    };
                    if is_mutation_tool(&meta.name) {
                        if let Some(ref tp) = meta.target_path {
                            let norm = normalize_path_for_compare(tp);
                            mutations.entry(norm).or_default().push(idx);
                        }
                    }
                    tool_calls_by_id.insert(call.id.clone(), meta);
                }
            }
        }

        // Pass 1b: Index tool result message operations (reads and mutations)
        for (idx, msg) in messages.iter().enumerate() {
            if msg.role != Role::Tool {
                continue;
            }

            let Some(meta) = resolve_tool_call_meta(messages, idx, &tool_calls_by_id) else {
                continue;
            };

            let tname = &meta.name;

            if let Some(path) = meta.target_path {
                let norm = normalize_path_for_compare(&path);
                if is_read_tool(tname) {
                    reads.entry(norm.clone()).or_default().push(idx);
                }
                if is_mutation_tool(tname) {
                    mutations.entry(norm).or_default().push(idx);
                }
            }
        }

        // Pass 2: Condense observations outside the preserved recent window
        for i in 0..cutoff {
            if messages[i].role != Role::Tool {
                continue;
            }

            let Some(meta) = resolve_tool_call_meta(messages, i, &tool_calls_by_id) else {
                continue;
            };

            let tname = &meta.name;

            if is_read_tool(tname) {
                if is_compacted_receipt(&messages[i].content)
                    || is_already_receipt(&messages[i].content, tname)
                    || is_already_receipt(&messages[i].content, "read_file")
                {
                    continue;
                }

                // Prevent negative compression: do not replace if raw output <= 128 bytes
                if messages[i].content.len() <= 128 {
                    continue;
                }

                let Some(path) = meta.target_path else {
                    continue;
                };

                let norm = normalize_path_for_compare(&path);

                // Check duplicate read: subsequent read of same path without any mutation between
                let next_read_idx = reads
                    .get(&norm)
                    .and_then(|indices| indices.iter().copied().find(|&read_idx| read_idx > i));

                let has_mutation_before_next_read = if let Some(next_r) = next_read_idx {
                    mutations
                        .get(&norm)
                        .map(|indices| indices.iter().any(|&m_idx| m_idx > i && m_idx < next_r))
                        .unwrap_or(false)
                } else {
                    false
                };

                if next_read_idx.is_some() && !has_mutation_before_next_read {
                    let raw_len = messages[i].content.len();
                    let ccr_id = CcrCache::store(&messages[i].content);
                    let receipt = format!(
                        "[read_file: {} (superseded by subsequent read. Use retrieve_observation(id=\"{}\") for raw content)]",
                        path, ccr_id
                    );
                    let chars_saved = raw_len.saturating_sub(receipt.len());
                    metrics.tokens_saved_estimate += chars_saved / 4;
                    messages[i].content = receipt;
                    metrics.duplicate_reads_compacted += 1;
                    continue;
                }

                // Check superseded read: subsequent mutation to this file exists
                let is_superseded = mutations
                    .get(&norm)
                    .map(|indices| indices.iter().any(|&m_idx| m_idx > i))
                    .unwrap_or(false);

                if is_superseded {
                    let raw_len = messages[i].content.len();
                    let line_count = messages[i].content.lines().count();
                    let ccr_id = CcrCache::store(&messages[i].content);
                    let receipt = format!(
                        "[read_file: {} ({} lines read, superseded by modification. Use retrieve_observation(id=\"{}\") for raw content)]",
                        path, line_count, ccr_id
                    );
                    let chars_saved = raw_len.saturating_sub(receipt.len());
                    metrics.tokens_saved_estimate += chars_saved / 4;
                    messages[i].content = receipt;
                    metrics.superseded_reads_compacted += 1;
                    continue;
                }
            } else if is_mutation_tool(tname) {
                if is_compacted_receipt(&messages[i].content)
                    || is_already_receipt(&messages[i].content, tname)
                {
                    continue;
                }

                // Historical mutation echo: condense outputs larger than 256 bytes
                if messages[i].content.len() > 256 {
                    let path = meta.target_path.unwrap_or_else(|| "file".to_string());
                    let raw_len = messages[i].content.len();
                    let ccr_id = CcrCache::store(&messages[i].content);
                    let receipt = format!(
                        "[{}: {} (successfully applied, {} bytes. Use retrieve_observation(id=\"{}\") for details)]",
                        tname, path, raw_len, ccr_id
                    );
                    let chars_saved = raw_len.saturating_sub(receipt.len());
                    metrics.tokens_saved_estimate += chars_saved / 4;
                    messages[i].content = receipt;
                    metrics.mutation_echoes_compacted += 1;
                    continue;
                }
            } else if is_search_tool(tname) {
                if is_compacted_receipt(&messages[i].content)
                    || is_already_receipt(&messages[i].content, tname)
                {
                    continue;
                }

                let line_count = messages[i].content.lines().count();
                let raw_len = messages[i].content.len();

                // Condense historical bulky search and grep observations (>25 lines or >300 bytes)
                if line_count > 25 || raw_len > 300 {
                    let query = meta.query.as_deref().unwrap_or("...");
                    let ccr_id = CcrCache::store(&messages[i].content);
                    let receipt = format!(
                        "[{}: query \"{}\" returned {} lines. Use retrieve_observation(id=\"{}\") for full matches]",
                        tname, query, line_count, ccr_id
                    );
                    let chars_saved = raw_len.saturating_sub(receipt.len());
                    metrics.tokens_saved_estimate += chars_saved / 4;
                    messages[i].content = receipt;
                    metrics.search_results_compacted += 1;
                    continue;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::types::ToolCall;
    use serde_json::json;

    #[test]
    fn test_extract_target_path_various_schemas() {
        let tc1 = ToolCall {
            id: "1".into(),
            name: "read_file".into(),
            arguments: json!({"path": "foo.rs"}),
        };
        assert_eq!(
            MicroCompactor::extract_target_path(&tc1),
            Some("foo.rs".to_string())
        );

        let tc2 = ToolCall {
            id: "2".into(),
            name: "patch_file".into(),
            arguments: json!({"target_file": "bar.rs"}),
        };
        assert_eq!(
            MicroCompactor::extract_target_path(&tc2),
            Some("bar.rs".to_string())
        );

        let tc3 = ToolCall {
            id: "3".into(),
            name: "view_file".into(),
            arguments: json!({"file_path": "baz.rs"}),
        };
        assert_eq!(
            MicroCompactor::extract_target_path(&tc3),
            Some("baz.rs".to_string())
        );

        let tc4 = ToolCall {
            id: "4".into(),
            name: "replace_file_content".into(),
            arguments: json!({"TargetFile": "qux.rs"}),
        };
        assert_eq!(
            MicroCompactor::extract_target_path(&tc4),
            Some("qux.rs".to_string())
        );

        let tc5 = ToolCall {
            id: "5".into(),
            name: "read_file".into(),
            arguments: json!({"AbsolutePath": "/project/src/main.rs"}),
        };
        assert_eq!(
            MicroCompactor::extract_target_path(&tc5),
            Some("/project/src/main.rs".to_string())
        );

        let tc6 = ToolCall {
            id: "6".into(),
            name: "read_file".into(),
            arguments: json!({"file": "data.json"}),
        };
        assert_eq!(
            MicroCompactor::extract_target_path(&tc6),
            Some("data.json".to_string())
        );

        let tc7 = ToolCall {
            id: "7".into(),
            name: "read_file".into(),
            arguments: json!({"target": "target.txt"}),
        };
        assert_eq!(
            MicroCompactor::extract_target_path(&tc7),
            Some("target.txt".to_string())
        );

        let tc_empty = ToolCall {
            id: "8".into(),
            name: "read_file".into(),
            arguments: json!({}),
        };
        assert_eq!(MicroCompactor::extract_target_path(&tc_empty), None);

        let tc_blank = ToolCall {
            id: "9".into(),
            name: "read_file".into(),
            arguments: json!({"path": "   "}),
        };
        assert_eq!(MicroCompactor::extract_target_path(&tc_blank), None);

        // Stringified JSON arguments
        let tc_str = ToolCall {
            id: "10".into(),
            name: "read_file".into(),
            arguments: json!("{\"path\": \"nested.rs\"}"),
        };
        assert_eq!(
            MicroCompactor::extract_target_path(&tc_str),
            Some("nested.rs".to_string())
        );
    }

    #[test]
    fn test_extract_search_query() {
        let tc1 = ToolCall {
            id: "1".into(),
            name: "grep_search".into(),
            arguments: json!({"query": "fn main"}),
        };
        assert_eq!(
            MicroCompactor::extract_search_query(&tc1),
            Some("fn main".to_string())
        );

        let tc2 = ToolCall {
            id: "2".into(),
            name: "find_by_name".into(),
            arguments: json!({"pattern": "*.rs"}),
        };
        assert_eq!(
            MicroCompactor::extract_search_query(&tc2),
            Some("*.rs".to_string())
        );

        let tc3 = ToolCall {
            id: "3".into(),
            name: "search".into(),
            arguments: json!({"term": "micro_compact"}),
        };
        assert_eq!(
            MicroCompactor::extract_search_query(&tc3),
            Some("micro_compact".to_string())
        );

        let tc4 = ToolCall {
            id: "4".into(),
            name: "search".into(),
            arguments: json!({"regex": "^pub fn"}),
        };
        assert_eq!(
            MicroCompactor::extract_search_query(&tc4),
            Some("^pub fn".to_string())
        );

        let tc5 = ToolCall {
            id: "5".into(),
            name: "search".into(),
            arguments: json!({"Query": "UpperQuery"}),
        };
        assert_eq!(
            MicroCompactor::extract_search_query(&tc5),
            Some("UpperQuery".to_string())
        );

        let tc6 = ToolCall {
            id: "6".into(),
            name: "search".into(),
            arguments: json!({"Pattern": "UpperPattern"}),
        };
        assert_eq!(
            MicroCompactor::extract_search_query(&tc6),
            Some("UpperPattern".to_string())
        );

        let tc_none = ToolCall {
            id: "7".into(),
            name: "search".into(),
            arguments: json!({}),
        };
        assert_eq!(MicroCompactor::extract_search_query(&tc_none), None);
    }

    #[test]
    fn test_superseded_file_read_compaction() {
        let original_50_lines = (1..=50)
            .map(|i| format!("pub fn line_{i}() -> usize {{ {i} }}"))
            .collect::<Vec<_>>()
            .join("\n");

        let mut messages = vec![
            // Message 0: User "Edit main.rs"
            Message::user("Edit main.rs"),
            // Message 1: Assistant calls read_file(path="src/main.rs")
            Message::assistant_with_tools(
                "",
                vec![ToolCall {
                    id: "call_read_1".into(),
                    name: "read_file".into(),
                    arguments: json!({"path": "src/main.rs"}),
                }],
            ),
            // Message 2: Tool result with 50 lines of code
            Message::tool_result("call_read_1", "read_file", &original_50_lines),
            // Message 3: Assistant calls write_file(path="src/main.rs")
            Message::assistant_with_tools(
                "",
                vec![ToolCall {
                    id: "call_write_1".into(),
                    name: "write_file".into(),
                    arguments: json!({"path": "src/main.rs"}),
                }],
            ),
            // Message 4: Tool result "ok"
            Message::tool_result("call_write_1", "write_file", "ok"),
            // Message 5: User "Now test it"
            Message::user("Now test it"),
            // Message 6: Assistant "Testing"
            Message::assistant("Testing"),
        ];

        let metrics = MicroCompactor::compact_messages(&mut messages, 1);

        assert_eq!(metrics.superseded_reads_compacted, 1);
        assert!(metrics.tokens_saved_estimate > 0);

        // Assert Message 2 was replaced with receipt containing [read_file: src/main.rs and ccr_.
        let compacted_content = &messages[2].content;
        assert!(
            compacted_content.starts_with("[read_file: src/main.rs (50 lines read, superseded by modification. Use retrieve_observation(id=\"ccr_"),
            "Expected receipt format, got: {compacted_content}"
        );

        // Extract ccr_id from receipt
        let id_start = compacted_content
            .find("id=\"")
            .expect("id=\" should be in receipt")
            + 4;
        let id_end = compacted_content[id_start..]
            .find('"')
            .expect("closing quote should be in receipt")
            + id_start;
        let ccr_id = &compacted_content[id_start..id_end];

        // Assert CcrCache::retrieve(&ccr_id, None, None) returns the original 50 lines.
        let retrieved = CcrCache::retrieve(ccr_id, None, None);
        assert_eq!(
            retrieved,
            Some(original_50_lines),
            "Original content must be retrieved losslessly from CCR cache"
        );
    }

    #[test]
    fn test_recent_turn_preserved_uncompacted() {
        let code = (1..=30)
            .map(|i| format!("let x_{i} = {i};"))
            .collect::<Vec<_>>()
            .join("\n");

        let mut messages = vec![
            // Turn 1
            Message::user("Modify foo.rs"),
            Message::assistant_with_tools(
                "",
                vec![ToolCall {
                    id: "call_w1".into(),
                    name: "write_file".into(),
                    arguments: json!({"path": "foo.rs"}),
                }],
            ),
            Message::tool_result("call_w1", "write_file", "ok"),
            // Turn 2 (Most recent turn)
            Message::user("Check foo.rs"),
            Message::assistant_with_tools(
                "",
                vec![ToolCall {
                    id: "call_r1".into(),
                    name: "read_file".into(),
                    arguments: json!({"path": "foo.rs"}),
                }],
            ),
            Message::tool_result("call_r1", "read_file", &code),
            Message::assistant_with_tools(
                "",
                vec![ToolCall {
                    id: "call_w2".into(),
                    name: "write_file".into(),
                    arguments: json!({"path": "foo.rs"}),
                }],
            ),
            Message::tool_result("call_w2", "write_file", "ok"),
        ];

        // preserve_recent_turns = 1 should preserve Turn 2 completely
        let metrics = MicroCompactor::compact_messages(&mut messages, 1);

        assert_eq!(metrics.superseded_reads_compacted, 0);
        assert_eq!(
            messages[5].content, code,
            "Tool result in recent turn must remain 100% untouched"
        );
    }

    #[test]
    fn test_non_superseded_read_uncompacted() {
        let code = "fn read_only() {}\n";
        let mut messages = vec![
            Message::user("Read bar.rs"),
            Message::assistant_with_tools(
                "",
                vec![ToolCall {
                    id: "call_r1".into(),
                    name: "read_file".into(),
                    arguments: json!({"path": "bar.rs"}),
                }],
            ),
            Message::tool_result("call_r1", "read_file", code),
            Message::user("Next turn"),
            Message::assistant("Done"),
        ];

        let metrics = MicroCompactor::compact_messages(&mut messages, 1);
        assert_eq!(metrics.superseded_reads_compacted, 0);
        assert_eq!(messages[2].content, code);
    }

    #[test]
    fn test_already_compacted_read_skipped() {
        let receipt = "[read_file: src/main.rs (10 lines read, superseded by modification. Use retrieve_observation(id=\"ccr_dummy\") for raw content)]";
        let mut messages = vec![
            Message::user("Edit main.rs"),
            Message::assistant_with_tools(
                "",
                vec![ToolCall {
                    id: "call_r1".into(),
                    name: "read_file".into(),
                    arguments: json!({"path": "src/main.rs"}),
                }],
            ),
            Message::tool_result("call_r1", "read_file", receipt),
            Message::assistant_with_tools(
                "",
                vec![ToolCall {
                    id: "call_w1".into(),
                    name: "write_file".into(),
                    arguments: json!({"path": "src/main.rs"}),
                }],
            ),
            Message::tool_result("call_w1", "write_file", "ok"),
            Message::user("Next turn"),
            Message::assistant("Done"),
        ];

        let metrics = MicroCompactor::compact_messages(&mut messages, 1);
        assert_eq!(metrics.superseded_reads_compacted, 0);
        assert_eq!(messages[2].content, receipt);
    }

    #[test]
    fn test_duplicate_consecutive_reads() {
        let lines_50 = (1..=50)
            .map(|i| format!("pub fn line_{i}() -> usize {{ {i} }}"))
            .collect::<Vec<_>>()
            .join("\n");

        let mut messages = vec![
            // Turn 1: read_file("src/main.rs") (50 lines)
            Message::user("Read main.rs"),
            Message::assistant_with_tools(
                "",
                vec![ToolCall {
                    id: "call_r1".into(),
                    name: "read_file".into(),
                    arguments: json!({"path": "src/main.rs"}),
                }],
            ),
            Message::tool_result("call_r1", "read_file", &lines_50),
            // Turn 2: User asks question, assistant answers
            Message::user("What does it do?"),
            Message::assistant("It defines 50 functions."),
            // Turn 3: read_file("src/main.rs") (50 lines)
            Message::user("Read main.rs again"),
            Message::assistant_with_tools(
                "",
                vec![ToolCall {
                    id: "call_r2".into(),
                    name: "read_file".into(),
                    arguments: json!({"path": "src/main.rs"}),
                }],
            ),
            Message::tool_result("call_r2", "read_file", &lines_50),
            // Turn 4: Recent turn
            Message::user("Recent question"),
            Message::assistant("Recent answer"),
        ];

        let metrics = MicroCompactor::compact_messages(&mut messages, 1);

        assert_eq!(metrics.duplicate_reads_compacted, 1);
        assert_eq!(metrics.superseded_reads_compacted, 0);
        assert!(metrics.tokens_saved_estimate > 0);

        // Assert Turn 1 read is condensed with "superseded by subsequent read"
        assert!(
            messages[2]
                .content
                .contains("superseded by subsequent read"),
            "Turn 1 read should be condensed with 'superseded by subsequent read', got: {}",
            messages[2].content
        );
        assert!(
            messages[2].content.starts_with("[read_file: src/main.rs (superseded by subsequent read. Use retrieve_observation(id=\"ccr_"),
            "Unexpected receipt format: {}",
            messages[2].content
        );

        // Assert Turn 3 read is preserved
        assert_eq!(
            messages[7].content, lines_50,
            "Turn 3 read must be preserved"
        );
    }

    #[test]
    fn test_historical_mutation_echo_condensation() {
        // Turn 1: write_file("src/main.rs") returning 500 bytes of unified diff / echo
        let echo_500 = "x".repeat(500);

        let mut messages = vec![
            // Turn 1
            Message::user("Update main.rs"),
            Message::assistant_with_tools(
                "",
                vec![ToolCall {
                    id: "call_w1".into(),
                    name: "write_file".into(),
                    arguments: json!({"path": "src/main.rs"}),
                }],
            ),
            Message::tool_result("call_w1", "write_file", &echo_500),
            // Turn 2: User prompt + assistant action
            Message::user("Next step"),
            Message::assistant("Done"),
            // Turn 3: Recent turn
            Message::user("Recent check"),
            Message::assistant("All good"),
        ];

        let metrics = MicroCompactor::compact_messages(&mut messages, 1);

        assert_eq!(metrics.mutation_echoes_compacted, 1);
        assert!(metrics.tokens_saved_estimate > 0);

        // Assert Turn 1 mutation output is condensed to [write_file: src/main.rs (successfully applied, 500 bytes...)]
        let compacted = &messages[2].content;
        assert!(
            compacted.starts_with("[write_file: src/main.rs (successfully applied, 500 bytes. Use retrieve_observation(id=\"ccr_"),
            "Expected mutation echo receipt, got: {compacted}"
        );

        // Assert raw 500 bytes is retrievable via CcrCache::retrieve
        let id_start = compacted.find("id=\"").expect("id=\" should be in receipt") + 4;
        let id_end = compacted[id_start..]
            .find('"')
            .expect("closing quote should be in receipt")
            + id_start;
        let ccr_id = &compacted[id_start..id_end];

        let retrieved = CcrCache::retrieve(ccr_id, None, None);
        assert_eq!(
            retrieved,
            Some(echo_500),
            "Raw 500 bytes must be losslessly retrievable from CCR cache"
        );
    }

    #[test]
    fn test_no_negative_compression_on_tiny_output() {
        let tiny_read = "let a = 1;"; // 10 bytes <= 128
        let tiny_write = "ok"; // 2 bytes <= 256

        let mut messages = vec![
            Message::user("Read small"),
            Message::assistant_with_tools(
                "",
                vec![ToolCall {
                    id: "call_r1".into(),
                    name: "read_file".into(),
                    arguments: json!({"path": "tiny.rs"}),
                }],
            ),
            Message::tool_result("call_r1", "read_file", tiny_read),
            Message::user("Write small"),
            Message::assistant_with_tools(
                "",
                vec![ToolCall {
                    id: "call_w1".into(),
                    name: "write_file".into(),
                    arguments: json!({"path": "tiny.rs"}),
                }],
            ),
            Message::tool_result("call_w1", "write_file", tiny_write),
            Message::user("Recent"),
            Message::assistant("Done"),
        ];

        let metrics = MicroCompactor::compact_messages(&mut messages, 1);

        assert_eq!(metrics.total_compacted(), 0);
        assert_eq!(
            messages[2].content, tiny_read,
            "Tiny read must not be replaced"
        );
        assert_eq!(
            messages[5].content, tiny_write,
            "Tiny write must not be replaced"
        );
    }

    #[test]
    fn test_cross_platform_path_normalization() {
        let lines_50 = (1..=50)
            .map(|i| format!("fn f_{i}() {{}}"))
            .collect::<Vec<_>>()
            .join("\n");

        let mut messages = vec![
            // Turn 1: Windows-style path with backslashes
            Message::user("Read file"),
            Message::assistant_with_tools(
                "",
                vec![ToolCall {
                    id: "call_r1".into(),
                    name: "read_file".into(),
                    arguments: json!({"path": "src\\main.rs"}),
                }],
            ),
            Message::tool_result("call_r1", "read_file", &lines_50),
            // Turn 2: Unix-style path modifying same file
            Message::user("Modify file"),
            Message::assistant_with_tools(
                "",
                vec![ToolCall {
                    id: "call_w1".into(),
                    name: "write_file".into(),
                    arguments: json!({"path": "src/main.rs"}),
                }],
            ),
            Message::tool_result("call_w1", "write_file", "ok"),
            // Turn 3: Recent turn
            Message::user("Recent"),
            Message::assistant("Done"),
        ];

        let metrics = MicroCompactor::compact_messages(&mut messages, 1);

        assert_eq!(metrics.superseded_reads_compacted, 1);
        assert!(messages[2].content.contains("superseded by modification"));
    }

    #[test]
    fn test_normalize_path_for_compare_variants() {
        assert_eq!(normalize_path_for_compare("src\\main.rs"), "src/main.rs");
        assert_eq!(normalize_path_for_compare("./src/main.rs"), "src/main.rs");
        assert_eq!(normalize_path_for_compare("/src/main.rs"), "src/main.rs");
        assert_eq!(normalize_path_for_compare(".\\src\\main.rs"), "src/main.rs");
        assert_eq!(normalize_path_for_compare("\\src\\main.rs"), "src/main.rs");
        assert_eq!(normalize_path_for_compare("src/main.rs"), "src/main.rs");
        assert_eq!(
            normalize_path_for_compare("  ./src\\main.rs  "),
            "src/main.rs"
        );
    }

    #[test]
    fn test_duplicate_read_with_normalized_paths() {
        let lines_50 = (1..=50)
            .map(|i| format!("fn norm_{i}() {{}}"))
            .collect::<Vec<_>>()
            .join("\n");

        let mut messages = vec![
            Message::user("Read 1"),
            Message::assistant_with_tools(
                "",
                vec![ToolCall {
                    id: "call_r1".into(),
                    name: "read_file".into(),
                    arguments: json!({"path": ".\\src\\main.rs"}),
                }],
            ),
            Message::tool_result("call_r1", "read_file", &lines_50),
            Message::user("Question"),
            Message::assistant("Answer"),
            Message::user("Read 2"),
            Message::assistant_with_tools(
                "",
                vec![ToolCall {
                    id: "call_r2".into(),
                    name: "read_file".into(),
                    arguments: json!({"path": "/src/main.rs"}),
                }],
            ),
            Message::tool_result("call_r2", "read_file", &lines_50),
            Message::user("Recent"),
            Message::assistant("Done"),
        ];

        let metrics = MicroCompactor::compact_messages(&mut messages, 1);
        assert_eq!(metrics.duplicate_reads_compacted, 1);
        assert!(messages[2]
            .content
            .contains("superseded by subsequent read"));
    }

    #[test]
    fn test_duplicate_read_with_mutation_between_not_compacted_as_duplicate() {
        let lines_50 = (1..=50)
            .map(|i| format!("fn inter_{i}() {{}}"))
            .collect::<Vec<_>>()
            .join("\n");

        let mut messages = vec![
            Message::user("Read 1"),
            Message::assistant_with_tools(
                "",
                vec![ToolCall {
                    id: "call_r1".into(),
                    name: "read_file".into(),
                    arguments: json!({"path": "src/main.rs"}),
                }],
            ),
            Message::tool_result("call_r1", "read_file", &lines_50),
            Message::user("Write"),
            Message::assistant_with_tools(
                "",
                vec![ToolCall {
                    id: "call_w1".into(),
                    name: "write_file".into(),
                    arguments: json!({"path": "src/main.rs"}),
                }],
            ),
            Message::tool_result("call_w1", "write_file", "ok"),
            Message::user("Read 2"),
            Message::assistant_with_tools(
                "",
                vec![ToolCall {
                    id: "call_r2".into(),
                    name: "read_file".into(),
                    arguments: json!({"path": "src/main.rs"}),
                }],
            ),
            Message::tool_result("call_r2", "read_file", &lines_50),
            Message::user("Recent"),
            Message::assistant("Done"),
        ];

        let metrics = MicroCompactor::compact_messages(&mut messages, 1);
        assert_eq!(metrics.duplicate_reads_compacted, 0);
        assert_eq!(metrics.superseded_reads_compacted, 1);
        assert!(messages[2].content.contains("superseded by modification"));
    }

    #[test]
    fn test_mutation_echo_recent_turn_preserved() {
        let echo_500 = "y".repeat(500);

        let mut messages = vec![
            Message::user("Recent write"),
            Message::assistant_with_tools(
                "",
                vec![ToolCall {
                    id: "call_w1".into(),
                    name: "write_file".into(),
                    arguments: json!({"path": "src/main.rs"}),
                }],
            ),
            Message::tool_result("call_w1", "write_file", &echo_500),
        ];

        let metrics = MicroCompactor::compact_messages(&mut messages, 1);
        assert_eq!(metrics.mutation_echoes_compacted, 0);
        assert_eq!(messages[2].content, echo_500);
    }

    #[test]
    fn test_historical_search_result_condensation() {
        // Turn 1: grep_search(query="AuthService") returning 80 lines of matches (>1000 bytes)
        let search_output_80 = (1..=80)
            .map(|i| format!("src/auth/service.rs:{i}:    let user_{i} = AuthService::find({i});"))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(search_output_80.len() > 1000);
        assert_eq!(search_output_80.lines().count(), 80);

        let mut messages = vec![
            // Turn 1: grep_search(query="AuthService")
            Message::user("Find references to AuthService"),
            Message::assistant_with_tools(
                "",
                vec![ToolCall {
                    id: "call_grep_1".into(),
                    name: "grep_search".into(),
                    arguments: json!({"query": "AuthService"}),
                }],
            ),
            Message::tool_result("call_grep_1", "grep_search", &search_output_80),
            // Turn 2: User prompt + assistant action
            Message::user("Now refactor AuthService"),
            Message::assistant("I will refactor it now."),
            // Turn 3: Recent turn
            Message::user("Recent check"),
            Message::assistant("All set."),
        ];

        let metrics = MicroCompactor::compact_messages(&mut messages, 1);

        assert_eq!(metrics.search_results_compacted, 1);
        assert!(metrics.tokens_saved_estimate > 0);

        // Assert Turn 1 is condensed to [grep_search: query "AuthService" returned 80 lines. Use retrieve_observation(id="ccr_...")]
        let compacted = &messages[2].content;
        assert!(
            compacted.starts_with("[grep_search: query \"AuthService\" returned 80 lines. Use retrieve_observation(id=\"ccr_"),
            "Expected search receipt format, got: {compacted}"
        );
        assert!(
            compacted.ends_with("for full matches]"),
            "Expected receipt to end with 'for full matches]', got: {compacted}"
        );

        // Extract ccr_id from receipt
        let id_start = compacted.find("id=\"").expect("id=\" should be in receipt") + 4;
        let id_end = compacted[id_start..]
            .find('"')
            .expect("closing quote should be in receipt")
            + id_start;
        let ccr_id = &compacted[id_start..id_end];

        // Assert raw 80 lines is losslessly retrievable via CcrCache::retrieve
        let retrieved = CcrCache::retrieve(ccr_id, None, None);
        assert_eq!(
            retrieved,
            Some(search_output_80),
            "Original 80 lines must be losslessly retrievable from CCR cache"
        );
    }

    #[test]
    fn test_recent_search_result_uncompacted() {
        let search_output_80 = (1..=80)
            .map(|i| format!("src/search.rs:{i}: match_{i}"))
            .collect::<Vec<_>>()
            .join("\n");

        let mut messages = vec![
            // Turn 1
            Message::user("Hello"),
            Message::assistant("Hi there"),
            // Turn 2 (Recent turn)
            Message::user("Search for matches"),
            Message::assistant_with_tools(
                "",
                vec![ToolCall {
                    id: "call_grep_recent".into(),
                    name: "grep_search".into(),
                    arguments: json!({"query": "match"}),
                }],
            ),
            Message::tool_result("call_grep_recent", "grep_search", &search_output_80),
        ];

        // preserve_recent_turns = 1 should preserve Turn 2 completely
        let metrics = MicroCompactor::compact_messages(&mut messages, 1);

        assert_eq!(metrics.search_results_compacted, 0);
        assert_eq!(
            messages[4].content, search_output_80,
            "Search result in recent turn must remain 100% untouched"
        );
    }

    #[test]
    fn test_small_search_result_uncompacted() {
        // A search result with only 3 lines / 100 bytes is not condensed (negative compression prevention)
        let small_search = "file1.rs:1: match\nfile2.rs:2: match\nfile3.rs:3: match";
        assert_eq!(small_search.lines().count(), 3);
        assert!(small_search.len() < 300);

        let mut messages = vec![
            // Turn 1: Small search
            Message::user("Search for match"),
            Message::assistant_with_tools(
                "",
                vec![ToolCall {
                    id: "call_grep_small".into(),
                    name: "grep_search".into(),
                    arguments: json!({"query": "match"}),
                }],
            ),
            Message::tool_result("call_grep_small", "grep_search", small_search),
            // Turn 2: User prompt + assistant action
            Message::user("Continue"),
            Message::assistant("Okay"),
            // Turn 3: Recent turn
            Message::user("Recent"),
            Message::assistant("Done"),
        ];

        let metrics = MicroCompactor::compact_messages(&mut messages, 1);

        assert_eq!(metrics.search_results_compacted, 0);
        assert_eq!(
            messages[2].content, small_search,
            "Small search result (<=25 lines and <=300 bytes) must not be condensed"
        );
    }

    #[test]
    fn test_find_by_name_pattern_search_result_condensation() {
        let file_list_40 = (1..=40)
            .map(|i| format!("src/models/model_{i}.rs"))
            .collect::<Vec<_>>()
            .join("\n");

        let mut messages = vec![
            // Turn 1: find_by_name(pattern="*.rs")
            Message::user("Find all rust files"),
            Message::assistant_with_tools(
                "",
                vec![ToolCall {
                    id: "call_find_1".into(),
                    name: "find_by_name".into(),
                    arguments: json!({"pattern": "*.rs"}),
                }],
            ),
            Message::tool_result("call_find_1", "find_by_name", &file_list_40),
            // Turn 2
            Message::user("Next"),
            Message::assistant("Done"),
            // Turn 3: Recent turn
            Message::user("Recent"),
            Message::assistant("Finished"),
        ];

        let metrics = MicroCompactor::compact_messages(&mut messages, 1);

        assert_eq!(metrics.search_results_compacted, 1);
        let compacted = &messages[2].content;
        assert!(
            compacted.starts_with("[find_by_name: query \"*.rs\" returned 40 lines. Use retrieve_observation(id=\"ccr_"),
            "Expected find_by_name receipt format, got: {compacted}"
        );
    }

    #[test]
    fn test_search_missing_query_fallback() {
        let raw_30_lines = (1..=30)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");

        let mut messages = vec![
            Message::user("Search"),
            Message::assistant_with_tools(
                "",
                vec![ToolCall {
                    id: "call_search_no_query".into(),
                    name: "grep".into(),
                    arguments: json!({}),
                }],
            ),
            Message::tool_result("call_search_no_query", "grep", &raw_30_lines),
            Message::user("Next"),
            Message::assistant("Done"),
            Message::user("Recent"),
            Message::assistant("Finished"),
        ];

        let metrics = MicroCompactor::compact_messages(&mut messages, 1);
        assert_eq!(metrics.search_results_compacted, 1);
        assert!(messages[2]
            .content
            .contains("query \"...\" returned 30 lines"));
    }

    #[test]
    fn test_is_already_receipt() {
        assert!(is_already_receipt("[read_file: src/main.rs]", "read_file"));
        assert!(is_already_receipt(
            "[write_file: src/main.rs (successfully applied)]",
            "write_file"
        ));
        assert!(is_already_receipt(
            "[grep_search: query \"AuthService\" returned 80 lines]",
            "grep_search"
        ));
        assert!(!is_already_receipt(
            "[read_file: src/main.rs]",
            "write_file"
        ));
        assert!(!is_already_receipt(
            "read_file: not in brackets",
            "read_file"
        ));
        assert!(!is_already_receipt(
            "[read_file_extra: not colon]",
            "read_file"
        ));
        assert!(!is_already_receipt("", "read_file"));
    }
}

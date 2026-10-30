use crate::agent::types::{Message, Role};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Statistics on tokens and characters saved through observation deduplication.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeduplicationStats {
    pub redundant_reads_collapsed: usize,
    pub identical_diagnostics_collapsed: usize,
    pub characters_saved: usize,
}

pub struct ObservationDeduplicator;

impl ObservationDeduplicator {
    /// Inspects conversation history and deduplicates redundant file reads and identical diagnostic outputs.
    pub fn deduplicate_messages(messages: &mut [Message]) -> DeduplicationStats {
        let mut stats = DeduplicationStats::default();
        let mut seen_file_reads: HashMap<String, (usize, String)> = HashMap::new();
        let mut last_diagnostic_hash: Option<(usize, u64, String)> = None;

        for (idx, msg) in messages.iter_mut().enumerate() {
            if msg.role != Role::Tool {
                continue;
            }

            // 1. Deduplicate redundant read_file outputs
            if msg.content.contains("File Content (")
                || (msg.content.starts_with("```") && msg.content.contains("\n"))
            {
                let lines_count = msg.content.lines().count();
                if lines_count > crate::constants::MIN_LINES_FOR_DEDUPLICATION {
                    let hash = hash_str(&msg.content);
                    let key = format!("hash:{}", hash);

                    if let Some((prev_turn, _prev_summary)) = seen_file_reads.get(&key) {
                        let original_len = msg.content.len();
                        let replacement = format!(
                            "ℹ [Observation Deduplicated: Identical to output in turn #{}, {} lines preserved]",
                            prev_turn, lines_count
                        );
                        if original_len > replacement.len() {
                            stats.characters_saved += original_len - replacement.len();
                            stats.redundant_reads_collapsed += 1;
                            msg.content = replacement;
                        }
                    } else {
                        seen_file_reads.insert(key, (idx + 1, format!("{} lines", lines_count)));
                    }
                }
            }

            // 2. Deduplicate repeating identical compiler/diagnostic outputs
            if msg.content.contains("Finished `dev` profile")
                || msg.content.contains("Finished `test` profile")
                || msg.content.contains("Checking ")
                || msg.content.contains("Compiling ")
            {
                let hash = hash_str(&msg.content);
                if let Some((prev_turn, prev_hash, prev_status)) = &last_diagnostic_hash {
                    if *prev_hash == hash {
                        let original_len = msg.content.len();
                        let replacement = format!(
                            "ℹ [Diagnostic Deduplicated: Output identical to turn #{}: {}]",
                            prev_turn, prev_status
                        );
                        if original_len > replacement.len() {
                            stats.characters_saved += original_len - replacement.len();
                            stats.identical_diagnostics_collapsed += 1;
                            msg.content = replacement;
                        }
                    } else {
                        let summary = if msg.content.contains("error:") {
                            "Compiler errors reported"
                        } else if msg.content.contains("warning:") {
                            "Clean with warnings"
                        } else {
                            "Build clean"
                        };
                        last_diagnostic_hash = Some((idx + 1, hash, summary.to_string()));
                    }
                } else {
                    let summary = if msg.content.contains("error:") {
                        "Compiler errors reported"
                    } else if msg.content.contains("warning:") {
                        "Clean with warnings"
                    } else {
                        "Build clean"
                    };
                    last_diagnostic_hash = Some((idx + 1, hash, summary.to_string()));
                }
            }
        }

        stats
    }
}

fn hash_str(s: &str) -> u64 {
    let mut h: u64 = crate::constants::FNV_OFFSET_BASIS;
    for b in s.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(crate::constants::FNV_PRIME);
    }
    h
}

/// Per-session tracker that detects and blocks redundant identical fetch, search, and read calls
/// (inspired by `micode`'s Fetch Tracker hook).
///
/// - On the 2nd identical call (`count == 2`), appends a `<fetch-warning>` note reminding the agent
///   that the result is already in context.
/// - On the 3rd+ identical call (`count >= 3`), replaces bulky output with a compact `<fetch-blocked>`
///   notice so the agent cannot loop infinitely on the same query.
/// - When any mutating tool runs, `invalidate_local_reads()` clears local workspace read/search
///   counts while keeping external web fetch/search counts intact.
#[derive(Debug, Clone, Default)]
pub struct FetchCallTracker {
    web_calls: HashMap<String, usize>,
    local_calls: HashMap<String, usize>,
}

impl FetchCallTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Clears local file/search call counts after a workspace mutation or shell command execution.
    pub fn invalidate_local_reads(&mut self) {
        self.local_calls.clear();
    }

    /// Resets all tracked web and local call counts.
    pub fn clear(&mut self) {
        self.web_calls.clear();
        self.local_calls.clear();
    }

    fn is_web_tool(tool_name: &str) -> bool {
        matches!(
            tool_name,
            "search_web" | "fetch_or_browse" | "crawl_documentation" | "crawl_sitemap"
        )
    }

    fn is_local_query_tool(tool_name: &str) -> bool {
        matches!(
            tool_name,
            "read_file"
                | "look_at"
                | "batch_read"
                | "grep_search"
                | "file_search"
                | "locate_symbol"
                | "view_outline"
        )
    }

    /// Returns true if `tool_name` can mutate workspace source files on disk (requiring
    /// cached local file read counts to be invalidated). Browser inspection tools (`browser_*`),
    /// planning tools (`update_progress`, `create_plan`, `record_decision`), and verification
    /// tools (`power_review`, `power_verify`) do not mutate source files and must NOT wipe
    /// the local read deduplication cache.
    pub fn should_invalidate_local_reads(tool_name: &str) -> bool {
        if Self::is_local_query_tool(tool_name) || Self::is_web_tool(tool_name) {
            return false;
        }
        if tool_name.starts_with("browser_")
            || matches!(
                tool_name,
                "update_progress"
                    | "create_plan"
                    | "power_plan"
                    | "archive_plan"
                    | "record_decision"
                    | "ask_user"
                    | "power_review"
                    | "power_verify"
                    | "block_search"
                    | "block_get"
                    | "block_palettes"
                    | "activate_tools"
            )
        {
            return false;
        }
        !crate::tools::concurrency::classify_tool(tool_name).is_read_only()
    }

    /// Canonicalizes tool arguments by normalizing relative path components (`.` segments)
    /// so equivalent paths like `"./a/./b.png"` and `"a/b.png"` map to the same dedup key.
    fn canonicalize_args(args: &serde_json::Value) -> String {
        let mut normalized = args.clone();
        if let Some(obj) = normalized.as_object_mut() {
            for key in ["path", "file_path"] {
                if let Some(raw_path) = obj.get(key).and_then(|v| v.as_str()) {
                    let clean: Vec<String> = std::path::Path::new(raw_path)
                        .components()
                        .filter(|c| !matches!(c, std::path::Component::CurDir))
                        .map(|c| c.as_os_str().to_string_lossy().to_string())
                        .collect();
                    if !clean.is_empty() {
                        obj.insert(key.to_string(), serde_json::Value::String(clean.join("/")));
                    }
                }
            }
        }
        serde_json::to_string(&normalized).unwrap_or_default()
    }

    /// Pre-registers screenshot paths captured by `browser_screenshot` or `browser_batch`
    /// as already loaded in the multimodal context so subsequent `read_file` calls on the
    /// same screenshot file are immediately recognized as redundant.
    pub fn record_preloaded_screenshot(&mut self, tool_name: &str, args: &serde_json::Value) {
        let mut raw_paths = Vec::new();
        match tool_name {
            "browser_screenshot" => {
                if let Some(p) = args
                    .get("path")
                    .or_else(|| args.get("file_path"))
                    .and_then(|v| v.as_str())
                {
                    raw_paths.push(p.to_string());
                }
            }
            "browser_batch" => {
                if let Some(actions) = args.get("actions").and_then(|v| v.as_array()) {
                    for step in actions {
                        let is_screenshot = step
                            .get("action")
                            .or_else(|| step.get("type"))
                            .or_else(|| step.get("step"))
                            .and_then(|v| v.as_str())
                            .is_some_and(|a| a.eq_ignore_ascii_case("screenshot"));
                        if is_screenshot {
                            if let Some(p) = step
                                .get("path")
                                .or_else(|| step.get("file_path"))
                                .and_then(|v| v.as_str())
                            {
                                raw_paths.push(p.to_string());
                            }
                        }
                    }
                }
            }
            _ => return,
        }

        for raw in raw_paths {
            let trimmed = raw.trim().trim_start_matches("./");
            if trimmed.is_empty() {
                continue;
            }
            let candidates = if trimmed.starts_with(".minicode/screenshots/") {
                vec![
                    trimmed.to_string(),
                    trimmed
                        .trim_start_matches(".minicode/screenshots/")
                        .to_string(),
                ]
            } else {
                vec![
                    trimmed.to_string(),
                    format!(".minicode/screenshots/{}", trimmed),
                ]
            };
            for candidate in candidates {
                for key_name in ["path", "file_path"] {
                    let mut map = serde_json::Map::new();
                    map.insert(
                        key_name.to_string(),
                        serde_json::Value::String(candidate.clone()),
                    );
                    let canonical = Self::canonicalize_args(&serde_json::Value::Object(map));
                    let dedup_key = format!("read_file:{}", canonical);
                    let entry = self.local_calls.entry(dedup_key).or_insert(0);
                    if *entry == 0 {
                        *entry = 1;
                    }
                }
            }
        }
    }

    /// Records a completed tool call and annotates or collapses redundant output when repeated.
    pub fn record_and_annotate(
        &mut self,
        tool_name: &str,
        args: &serde_json::Value,
        output: String,
    ) -> String {
        let is_web = Self::is_web_tool(tool_name);
        let is_local = Self::is_local_query_tool(tool_name);
        if !is_web && !is_local {
            return output;
        }

        let canonical_args = Self::canonicalize_args(args);
        let key = format!("{}:{}", tool_name, canonical_args);

        let map = if is_web {
            &mut self.web_calls
        } else {
            &mut self.local_calls
        };

        let count = map.entry(key).or_insert(0);
        *count += 1;
        let current_count = *count;

        let is_image_asset = output.contains("[Visual Image Asset Loaded:");
        if current_count >= 2 && is_image_asset {
            format!(
                "<fetch-blocked tool=\"{}\" count=\"{}\">This visual image asset is already attached to your multimodal vision context in this session. Inspect the image already in your context instead of calling `{}` on it again.</fetch-blocked>",
                tool_name, current_count, tool_name
            )
        } else if current_count == 2 {
            format!(
                "{}\n\n<fetch-warning tool=\"{}\" count=\"2\">You already executed this exact `{}` call earlier in this session. Reuse the result already in your conversation context instead of repeating identical queries.</fetch-warning>",
                output, tool_name, tool_name
            )
        } else if current_count >= 3 && output.len() > 240 {
            format!(
                "<fetch-blocked tool=\"{}\" count=\"{}\">Redundant identical `{}` call blocked ({}x in this session). Refer to the output from the earlier call in your conversation history instead of repeating the same request.</fetch-blocked>",
                tool_name, current_count, tool_name, current_count
            )
        } else {
            output
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deduplicate_identical_tool_observations() {
        let long_code =
            "fn main() {\n".to_string() + &"    println!(\"hello\");\n".repeat(20) + "}\n";

        let mut msgs = vec![
            Message::user("Please read main.rs"),
            Message::tool_result(
                "call_1",
                "read_file",
                format!("File Content (src/main.rs):\n{}", long_code),
            ),
            Message::user("Please read main.rs again"),
            Message::tool_result(
                "call_2",
                "read_file",
                format!("File Content (src/main.rs):\n{}", long_code),
            ),
        ];

        let stats = ObservationDeduplicator::deduplicate_messages(&mut msgs);
        assert_eq!(stats.redundant_reads_collapsed, 1);
        assert!(stats.characters_saved > 100);
        assert!(msgs[3].content.contains("Observation Deduplicated"));
    }

    #[test]
    fn test_fetch_call_tracker_warns_and_blocks_until_invalidated() {
        let mut tracker = FetchCallTracker::new();
        let args = serde_json::json!({ "path": "src/lib.rs" });
        let sample_output = "x".repeat(400);

        let out1 = tracker.record_and_annotate("read_file", &args, sample_output.clone());
        assert_eq!(out1, sample_output);

        let out2 = tracker.record_and_annotate("read_file", &args, sample_output.clone());
        assert!(out2.contains("<fetch-warning"));

        let out3 = tracker.record_and_annotate("read_file", &args, sample_output.clone());
        assert!(out3.contains("<fetch-blocked"));

        // After a file mutation, local read counts reset while web counts persist
        let web_args = serde_json::json!({ "query": "rust tokio" });
        let _ = tracker.record_and_annotate("search_web", &web_args, sample_output.clone());
        tracker.invalidate_local_reads();

        let out_after_mutation =
            tracker.record_and_annotate("read_file", &args, sample_output.clone());
        assert_eq!(out_after_mutation, sample_output);

        let web2 = tracker.record_and_annotate("search_web", &web_args, sample_output);
        assert!(web2.contains("<fetch-warning"));

        // Preloaded screenshot immediately blocks redundant `read_file` on the same screenshot
        tracker.record_preloaded_screenshot(
            "browser_screenshot",
            &serde_json::json!({ "path": "d-hero3.png" }),
        );
        let img_out =
            "✔ [Visual Image Asset Loaded: `.minicode/screenshots/d-hero3.png`]\n• **Format**: `image/png`".to_string();
        let blocked_img = tracker.record_and_annotate(
            "read_file",
            &serde_json::json!({ "path": ".minicode/screenshots/d-hero3.png" }),
            img_out,
        );
        assert!(blocked_img.contains("<fetch-blocked"));
    }
}

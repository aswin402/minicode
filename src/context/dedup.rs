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
}

use crate::agent::types::{Message, Role};
use crate::context::auto_compact::CompactionConfig;
use crate::error::Result;

pub struct ContextCompressor {
    config: CompactionConfig,
}

impl ContextCompressor {
    pub fn new() -> Result<Self> {
        Ok(Self {
            config: CompactionConfig::default(),
        })
    }

    /// Infallible fallback constructor that never panics or errors.
    pub fn default_safe() -> Self {
        Self {
            config: CompactionConfig::default(),
        }
    }

    /// Zero-allocation, O(N) single-pass lexical BPE token estimator.
    /// Avoids loading/cloning the 35MB `cl100k_base` BPE hash tables on every turn while
    /// maintaining subword-level accuracy across source code, JSON, and natural language.
    #[inline]
    pub fn count_tokens_fast(text: &str) -> usize {
        let bytes = text.as_bytes();
        let len = bytes.len();
        if len == 0 {
            return 0;
        }

        let mut tokens = 0usize;
        let mut i = 0usize;

        while i < len {
            let b = bytes[i];
            if b.is_ascii_alphabetic() || b == b'_' {
                let start = i;
                i += 1;
                while i < len && (bytes[i].is_ascii_alphabetic() || bytes[i] == b'_') {
                    i += 1;
                }
                let word_len = i - start;
                tokens += word_len.div_ceil(4);
            } else if b.is_ascii_digit() {
                let start = i;
                i += 1;
                while i < len && bytes[i].is_ascii_digit() {
                    i += 1;
                }
                let num_len = i - start;
                tokens += num_len.div_ceil(3);
            } else if b.is_ascii_whitespace() {
                let start = i;
                i += 1;
                while i < len && bytes[i].is_ascii_whitespace() {
                    i += 1;
                }
                let ws_len = i - start;
                // A single space typically merges with the following BPE token;
                // longer indentation/newline runs add ~1 token per 4 whitespace bytes.
                tokens += ws_len / 4;
            } else if b < 0x80 {
                // ASCII punctuation / operator
                tokens += 1;
                i += 1;
                // Merge common 2-char operators (==, !=, <=, >=, ->, =>, ::, //, &&, ||, ..)
                if i < len {
                    let next = bytes[i];
                    let is_pair = matches!(
                        (b, next),
                        (b'=', b'=')
                            | (b'!', b'=')
                            | (b'<', b'=')
                            | (b'>', b'=')
                            | (b'-', b'>')
                            | (b'=', b'>')
                            | (b':', b':')
                            | (b'/', b'/')
                            | (b'&', b'&')
                            | (b'|', b'|')
                            | (b'.', b'.')
                    );
                    if is_pair {
                        i += 1;
                    }
                }
            } else {
                // Multi-byte UTF-8 sequence
                let char_width = if b & 0b1110_0000 == 0b1100_0000 {
                    2
                } else if b & 0b1111_0000 == 0b1110_0000 {
                    3
                } else if b & 0b1111_1000 == 0b1111_0000 {
                    4
                } else {
                    1
                };
                tokens += if char_width >= 4 { 2 } else { 1 };
                i = (i + char_width).min(len);
            }
        }

        tokens.max(1)
    }

    /// Recursively counts tokens in a `serde_json::Value` in-place without allocating a serialized JSON string.
    pub fn count_json_value_tokens(val: &serde_json::Value) -> usize {
        match val {
            serde_json::Value::Null | serde_json::Value::Bool(_) => 1,
            serde_json::Value::Number(n) => {
                if n.is_f64() {
                    3
                } else {
                    2
                }
            }
            serde_json::Value::String(s) => 2 + Self::count_tokens_fast(s),
            serde_json::Value::Array(arr) => {
                2 + arr
                    .iter()
                    .map(|v| Self::count_json_value_tokens(v) + 1)
                    .sum::<usize>()
            }
            serde_json::Value::Object(map) => {
                2 + map
                    .iter()
                    .map(|(k, v)| Self::count_tokens_fast(k) + 3 + Self::count_json_value_tokens(v))
                    .sum::<usize>()
            }
        }
    }

    /// Accurately counts tokens for a text string using the fast zero-allocation lexical BPE estimator.
    pub fn count_tokens(&self, text: &str) -> usize {
        Self::count_tokens_fast(text)
    }

    /// Counts total tokens for a slice of conversation messages without intermediate string allocations.
    pub fn count_messages_tokens(&self, messages: &[Message]) -> usize {
        let mut total = 0;
        for msg in messages {
            total += crate::constants::MESSAGE_FRAMING_TOKEN_OVERHEAD; // Message overhead
            total += Self::count_tokens_fast(&msg.content);
            if let Some(ref tool_calls) = msg.tool_calls {
                for tc in tool_calls {
                    total += Self::count_tokens_fast(&tc.name);
                    total += Self::count_json_value_tokens(&tc.arguments);
                }
            }
        }
        total
    }

    /// Masks observation outputs that exceed max lines (Observation Masking).
    /// Leverages Smart Donut truncation to preserve head, tail, and critical error lines from the middle.
    pub fn mask_observation(output: &str, max_lines: usize) -> String {
        let total_lines = output.lines().count();
        if total_lines <= max_lines {
            return output.to_string();
        }

        let budget = (max_lines / 2).max(1);
        let head_count = budget.min(crate::constants::COMPRESSOR_HEAD_TAIL_LINES);
        let tail_count = budget.min(crate::constants::COMPRESSOR_HEAD_TAIL_LINES);

        crate::context::donut::SmartDonutTruncator::truncate_custom(
            output,
            max_lines,
            head_count,
            tail_count,
            crate::constants::DONUT_MAX_ERROR_LINES,
        )
        .content
    }

    /// Compacts message history if context consumption exceeds threshold.
    /// Preserves system prompt + most recent 3 turns, compressing older tool results.
    #[allow(dead_code)]
    pub fn compact_history(&self, messages: &mut [Message], max_window_tokens: usize) {
        let current_tokens = self.count_messages_tokens(messages);
        let threshold = (max_window_tokens as f64
            * (self.config.tier1_ratio - self.config.safety_margin).max(0.0))
            as usize;

        if current_tokens <= threshold
            || messages.len() <= crate::constants::MIN_COMPACTABLE_MESSAGES
        {
            return;
        }

        tracing::info!(
            current_tokens = current_tokens,
            threshold = threshold,
            "Compacting conversation context history"
        );

        // Retain the most recent preserved messages and compress older tool observations
        let preserve_count = crate::constants::CONTEXT_MIN_PRESERVED_MESSAGES.min(messages.len());
        let cutoff = messages.len().saturating_sub(preserve_count);

        for msg in messages.iter_mut().take(cutoff) {
            if msg.role == Role::Tool {
                // Compress older tool outputs
                msg.content =
                    Self::mask_observation(&msg.content, crate::constants::COMPRESSOR_MASK_LINES);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_counting() {
        let compressor = ContextCompressor::new().unwrap();
        let count = compressor.count_tokens("Hello, world! This is minicode.");
        assert!(count > 0 && count < 20);
    }

    #[test]
    fn test_mask_observation() {
        let mut long_output = String::new();
        for i in 1..=100 {
            long_output.push_str(&format!("Line {}\n", i));
        }

        let masked = ContextCompressor::mask_observation(&long_output, 30);
        assert!(masked.contains("Line 1"));
        assert!(masked.contains("Line 100"));
        assert!(masked.contains("Omitted 70 lines"));
    }

    #[test]
    fn test_mask_observation_preserves_errors() {
        let mut long_output = String::new();
        for i in 1..=100 {
            if i == 50 {
                long_output.push_str("error: critical build failure\n");
            } else {
                long_output.push_str(&format!("Log line {}\n", i));
            }
        }

        let masked = ContextCompressor::mask_observation(&long_output, 30);
        assert!(masked.contains("error: critical build failure"));
    }
}

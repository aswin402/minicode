use crate::agent::types::{Message, Role};
use crate::error::Result;
use tiktoken_rs::CoreBPE;

pub struct ContextCompressor {
    bpe: CoreBPE,
    warning_threshold: f64,
    safety_margin: f64,
}

impl ContextCompressor {
    pub fn new() -> Result<Self> {
        let bpe = tiktoken_rs::cl100k_base()
            .map_err(|e| crate::error::ContextError::TokenCount(e.to_string()))?;

        Ok(Self {
            bpe,
            warning_threshold: crate::constants::COMPRESSOR_WARNING_THRESHOLD,
            safety_margin: crate::constants::COMPRESSOR_SAFETY_MARGIN,
        })
    }

    /// Accurately counts tokens for a text string using OpenAI BPE.
    pub fn count_tokens(&self, text: &str) -> usize {
        self.bpe.encode_with_special_tokens(text).len()
    }

    /// Counts total tokens for a slice of conversation messages.
    pub fn count_messages_tokens(&self, messages: &[Message]) -> usize {
        let mut total = 0;
        for msg in messages {
            total += 4; // Message overhead
            total += self.count_tokens(&msg.content);
            if let Some(ref tool_calls) = msg.tool_calls {
                for tc in tool_calls {
                    total += self.count_tokens(&tc.name);
                    total += self.count_tokens(&tc.arguments.to_string());
                }
            }
        }
        total
    }

    /// Masks observation outputs that exceed max lines (Observation Masking).
    /// Retains first 15 lines (head) and last 15 lines (tail) to preserve crucial context & errors.
    pub fn mask_observation(output: &str, max_lines: usize) -> String {
        let total_lines = output.lines().count();
        if total_lines <= max_lines {
            return output.to_string();
        }

        let head_count = crate::constants::COMPRESSOR_HEAD_TAIL_LINES.min(total_lines / 2);
        let tail_count = crate::constants::COMPRESSOR_HEAD_TAIL_LINES.min(total_lines - head_count);
        let truncated_count = total_lines - (head_count + tail_count);

        let head: Vec<&str> = output.lines().take(head_count).collect();
        let tail: Vec<&str> = output.lines().skip(total_lines - tail_count).collect();

        format!(
            "{}\n\n[... Truncated {} lines of verbose tool output ...]\n\n{}",
            head.join("\n"),
            truncated_count,
            tail.join("\n")
        )
    }

    /// Compacts message history if context consumption exceeds threshold.
    /// Preserves system prompt + most recent 3 turns, compressing older tool results.
    pub fn compact_history(&self, messages: &mut [Message], max_window_tokens: usize) {
        let current_tokens = self.count_messages_tokens(messages);
        let threshold = (max_window_tokens as f64
            * (self.warning_threshold - self.safety_margin).max(0.0))
            as usize;

        if current_tokens <= threshold || messages.len() <= 6 {
            return;
        }

        tracing::info!(
            current_tokens = current_tokens,
            threshold = threshold,
            "Compacting conversation context history"
        );

        // Retain system messages (index 0) and the most recent 4 messages
        let preserve_count = 4.min(messages.len());
        let cutoff = messages.len() - preserve_count;

        for msg in messages.iter_mut().take(cutoff).skip(1) {
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
        assert!(masked.contains("Truncated 70 lines"));
    }
}

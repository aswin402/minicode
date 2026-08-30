use crate::agent::types::{Message, Role};
use crate::error::Result;
use tiktoken_rs::CoreBPE;

pub struct ContextCompressor {
    bpe: Option<CoreBPE>,
    #[allow(dead_code)]
    warning_threshold: f64,
    #[allow(dead_code)]
    safety_margin: f64,
}

impl ContextCompressor {
    pub fn new() -> Result<Self> {
        let bpe = tiktoken_rs::cl100k_base()
            .map_err(|e| crate::error::ContextError::TokenCount(e.to_string()))?;

        Ok(Self {
            bpe: Some(bpe),
            warning_threshold: crate::constants::COMPRESSOR_WARNING_THRESHOLD,
            safety_margin: crate::constants::COMPRESSOR_SAFETY_MARGIN,
        })
    }

    /// Infallible fallback constructor that never panics or errors.
    pub fn default_safe() -> Self {
        Self {
            bpe: tiktoken_rs::cl100k_base().ok(),
            warning_threshold: crate::constants::COMPRESSOR_WARNING_THRESHOLD,
            safety_margin: crate::constants::COMPRESSOR_SAFETY_MARGIN,
        }
    }

    /// Accurately counts tokens for a text string using OpenAI BPE or safe heuristic fallback.
    pub fn count_tokens(&self, text: &str) -> usize {
        if let Some(ref bpe) = self.bpe {
            bpe.encode_with_special_tokens(text).len()
        } else {
            (text.len() / 4).max(1)
        }
    }

    /// Counts total tokens for a slice of conversation messages.
    pub fn count_messages_tokens(&self, messages: &[Message]) -> usize {
        let mut total = 0;
        for msg in messages {
            total += crate::constants::MESSAGE_FRAMING_TOKEN_OVERHEAD; // Message overhead
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

        let budget = (max_lines / 2).max(1);
        let head_count = budget.min(crate::constants::COMPRESSOR_HEAD_TAIL_LINES);
        let tail_count = budget.min(crate::constants::COMPRESSOR_HEAD_TAIL_LINES);

        if head_count + tail_count >= total_lines {
            return output.to_string();
        }

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
    #[allow(dead_code)]
    pub fn compact_history(&self, messages: &mut [Message], max_window_tokens: usize) {
        let current_tokens = self.count_messages_tokens(messages);
        let threshold = (max_window_tokens as f64
            * (self.warning_threshold - self.safety_margin).max(0.0))
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
        assert!(masked.contains("Truncated 70 lines"));
    }
}

use crate::agent::types::{Message, Role};
use crate::context::auto_compact::CompactionConfig;
use crate::error::Result;
use tiktoken_rs::CoreBPE;

pub struct ContextCompressor {
    bpe: Option<CoreBPE>,
    config: CompactionConfig,
}

impl ContextCompressor {
    pub fn new() -> Result<Self> {
        let bpe = tiktoken_rs::cl100k_base()
            .map_err(|e| crate::error::ContextError::TokenCount(e.to_string()))?;

        Ok(Self {
            bpe: Some(bpe),
            config: CompactionConfig::default(),
        })
    }

    /// Infallible fallback constructor that never panics or errors.
    pub fn default_safe() -> Self {
        Self {
            bpe: tiktoken_rs::cl100k_base().ok(),
            config: CompactionConfig::default(),
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

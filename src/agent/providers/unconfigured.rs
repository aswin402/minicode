use crate::agent::provider::{ChunkStream, CompletionOptions, Provider, ToolSchema};
use crate::agent::types::Message;
use crate::error::Result;
use async_trait::async_trait;

/// A fallback placeholder provider used when a requested provider is not configured
/// (e.g. missing API key or invalid configuration) so that the TUI can always launch
/// safely and allow the user to switch models interactively.
pub struct UnconfiguredProvider {
    provider_name: String,
    reason: String,
}

impl UnconfiguredProvider {
    pub fn new(provider_name: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            provider_name: provider_name.into(),
            reason: reason.into(),
        }
    }
}

#[async_trait]
impl Provider for UnconfiguredProvider {
    fn name(&self) -> &str {
        &self.provider_name
    }

    fn default_model(&self) -> &str {
        "unconfigured"
    }

    async fn stream_completion(
        &self,
        _messages: &[Message],
        _tools: &[ToolSchema],
        _options: &CompletionOptions,
    ) -> Result<ChunkStream> {
        Err(crate::error::ProviderError::Api {
            status: 400,
            message: format!(
                "Provider '{}' is not configured: {}. Press F2 or run /model to switch providers or enter your API key.",
                self.provider_name, self.reason
            ),
        }
        .into())
    }
}

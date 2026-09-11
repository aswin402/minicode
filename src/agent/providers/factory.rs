use crate::agent::provider::Provider;
use crate::agent::providers::anthropic::AnthropicProvider;
use crate::agent::providers::gemini::GeminiProvider;
use crate::agent::providers::openai::OpenAiCompatibleProvider;
use crate::agent::providers::unconfigured::UnconfiguredProvider;
use crate::error::{ProviderError, Result};

/// Provider factory that initializes the appropriate provider based on name and config
#[allow(dead_code)]
pub fn create_provider(provider_name: &str, api_key: &str) -> Result<Box<dyn Provider>> {
    create_provider_with_base_url(provider_name, api_key, None)
}

/// Provider factory that supports custom base URLs for OpenAI-compatible endpoints
pub fn create_provider_with_base_url(
    provider_name: &str,
    api_key: &str,
    custom_base_url: Option<&str>,
) -> Result<Box<dyn Provider>> {
    match provider_name.to_lowercase().as_str() {
        "anthropic" | "claude" => {
            if let Some(url) = custom_base_url {
                Ok(Box::new(AnthropicProvider::with_base_url(
                    api_key,
                    url,
                    crate::constants::ANTHROPIC_DEFAULT_MODEL,
                )))
            } else {
                Ok(Box::new(AnthropicProvider::new(api_key)))
            }
        }
        "gemini" | "google" => Ok(Box::new(GeminiProvider::new(api_key))),
        "openrouter" => Ok(Box::new(OpenAiCompatibleProvider::openrouter(api_key))),
        "openai" => {
            if let Some(url) = custom_base_url {
                Ok(Box::new(OpenAiCompatibleProvider::new(
                    "openai",
                    api_key,
                    url,
                    crate::constants::OPENAI_DEFAULT_MODEL,
                )))
            } else {
                Ok(Box::new(OpenAiCompatibleProvider::openai(api_key)))
            }
        }
        "deepseek" => Ok(Box::new(OpenAiCompatibleProvider::new(
            "deepseek",
            api_key,
            custom_base_url.unwrap_or(crate::constants::DEEPSEEK_BASE_URL),
            crate::constants::DEEPSEEK_DEFAULT_MODEL,
        ))),
        "groq" => Ok(Box::new(OpenAiCompatibleProvider::new(
            "groq",
            api_key,
            custom_base_url.unwrap_or(crate::constants::GROQ_BASE_URL),
            crate::constants::GROQ_DEFAULT_MODEL,
        ))),
        "together" => Ok(Box::new(OpenAiCompatibleProvider::new(
            "together",
            api_key,
            custom_base_url.unwrap_or(crate::constants::TOGETHER_BASE_URL),
            crate::constants::TOGETHER_DEFAULT_MODEL,
        ))),
        "minimax" => Ok(Box::new(OpenAiCompatibleProvider::new(
            "minimax",
            api_key,
            custom_base_url.unwrap_or(crate::constants::MINIMAX_BASE_URL),
            crate::constants::MINIMAX_DEFAULT_MODEL,
        ))),
        "z.ai" | "z_ai" | "zhipu" | "glm" | "bigmodel" => {
            Ok(Box::new(OpenAiCompatibleProvider::new(
                "z.ai",
                api_key,
                custom_base_url.unwrap_or(crate::constants::ZHIPU_BASE_URL),
                crate::constants::ZHIPU_DEFAULT_MODEL,
            )))
        }
        "mistral" => Ok(Box::new(OpenAiCompatibleProvider::new(
            "mistral",
            api_key,
            custom_base_url.unwrap_or(crate::constants::MISTRAL_BASE_URL),
            crate::constants::MISTRAL_DEFAULT_MODEL,
        ))),
        "ollama" => Ok(Box::new(OpenAiCompatibleProvider::new(
            "ollama",
            if api_key.is_empty() {
                "ollama"
            } else {
                api_key
            },
            custom_base_url.unwrap_or(crate::constants::OLLAMA_DEFAULT_BASE_URL),
            crate::constants::OLLAMA_DEFAULT_MODEL,
        ))),
        "lmstudio" | "lm-studio" => Ok(Box::new(OpenAiCompatibleProvider::new(
            "lmstudio",
            if api_key.is_empty() {
                "lm-studio"
            } else {
                api_key
            },
            custom_base_url.unwrap_or(crate::constants::LMSTUDIO_DEFAULT_BASE_URL),
            crate::constants::DEFAULT_LOCAL_MODEL_NAME,
        ))),
        "vllm" => Ok(Box::new(OpenAiCompatibleProvider::new(
            "vllm",
            if api_key.is_empty() { "none" } else { api_key },
            custom_base_url.unwrap_or(crate::constants::VLLM_DEFAULT_BASE_URL),
            crate::constants::VLLM_DEFAULT_MODEL,
        ))),
        "local" | "localhost" | "localai" | "llama.cpp" | "llamacpp" | "jan" => {
            Ok(Box::new(OpenAiCompatibleProvider::new(
                provider_name,
                if api_key.is_empty() { "none" } else { api_key },
                custom_base_url.unwrap_or(crate::constants::LOCALAI_DEFAULT_BASE_URL),
                crate::constants::DEFAULT_LOCAL_MODEL_NAME,
            )))
        }
        custom_name => {
            if let Some(url) = custom_base_url {
                let key = if api_key.is_empty() { "none" } else { api_key };
                Ok(Box::new(OpenAiCompatibleProvider::new(
                    custom_name,
                    key,
                    url,
                    crate::constants::DEFAULT_FALLBACK_MODEL_NAME,
                )))
            } else if custom_name.contains("local") || custom_name.contains("127.0.0.1") {
                let key = if api_key.is_empty() { "none" } else { api_key };
                Ok(Box::new(OpenAiCompatibleProvider::new(
                    custom_name,
                    key,
                    crate::constants::LOCALAI_DEFAULT_BASE_URL,
                    crate::constants::DEFAULT_LOCAL_MODEL_NAME,
                )))
            } else {
                Err(ProviderError::UnsupportedModel {
                    model: custom_name.to_string(),
                    provider: provider_name.to_string(),
                }
                .into())
            }
        }
    }
}

/// Creates a provider, or returns a safe UnconfiguredProvider fallback so that
/// the application / TUI never crashes at startup due to missing keys or invalid configurations.
pub fn create_provider_or_fallback(
    provider_name: &str,
    api_key: Result<String>,
    custom_base_url: Option<&str>,
) -> (Box<dyn Provider>, Option<String>) {
    match api_key {
        Ok(key) => match create_provider_with_base_url(provider_name, &key, custom_base_url) {
            Ok(p) => (p, None),
            Err(e) => {
                let reason = e.to_string();
                (
                    Box::new(UnconfiguredProvider::new(provider_name, &reason)),
                    Some(reason),
                )
            }
        },
        Err(e) => {
            let reason = e.to_string();
            (
                Box::new(UnconfiguredProvider::new(provider_name, &reason)),
                Some(reason),
            )
        }
    }
}

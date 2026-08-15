#![allow(dead_code)]

use crate::error::{ProviderError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub context_length: Option<usize>,
    pub is_free: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct ModelsCache {
    providers: HashMap<String, Vec<ModelInfo>>,
    last_updated: HashMap<String, String>,
}

pub struct ModelFetcher {
    client: reqwest::Client,
    cache_path: PathBuf,
}

impl ModelFetcher {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(
                crate::constants::MODEL_FETCH_TIMEOUT_SECS,
            ))
            .build()
            .unwrap_or_default();

        let cache_path = if let Some(config_dir) = dirs::config_dir() {
            config_dir.join("minicode").join("models_cache.json")
        } else {
            PathBuf::from(".minicode").join("models_cache.json")
        };

        Self { client, cache_path }
    }

    /// Loads cached models from disk if available
    fn load_cache(&self) -> ModelsCache {
        if self.cache_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&self.cache_path) {
                if let Ok(cache) = serde_json::from_str::<ModelsCache>(&content) {
                    return cache;
                }
            }
        }
        ModelsCache::default()
    }

    /// Saves updated model cache to disk
    fn save_cache(&self, cache: &ModelsCache) {
        if let Some(parent) = self.cache_path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        if let Ok(json) = serde_json::to_string_pretty(cache) {
            std::fs::write(&self.cache_path, json).ok();
        }
    }

    /// Fetches all available models dynamically from the provider API (no hardcoding)
    pub async fn fetch_models(
        &self,
        provider: &str,
        api_key: &str,
        custom_base_url: Option<&str>,
    ) -> Result<Vec<ModelInfo>> {
        let provider_lower = provider.to_lowercase();

        // 1. Fetch live models
        let models_res = match provider_lower.as_str() {
            "openrouter" => self.fetch_openrouter_models(api_key).await,
            "gemini" | "google" => self.fetch_gemini_models(api_key).await,
            "openai" => {
                let url = custom_base_url.unwrap_or(crate::constants::OPENAI_DEFAULT_BASE_URL);
                self.fetch_openai_compatible_models(url, api_key).await
            }
            "deepseek" => {
                self.fetch_openai_compatible_models(crate::constants::DEEPSEEK_BASE_URL, api_key)
                    .await
            }
            "groq" => {
                self.fetch_openai_compatible_models(crate::constants::GROQ_BASE_URL, api_key)
                    .await
            }
            "together" => {
                self.fetch_openai_compatible_models(crate::constants::TOGETHER_BASE_URL, api_key)
                    .await
            }
            "ollama" => {
                let url = custom_base_url.unwrap_or(crate::constants::OLLAMA_DEFAULT_BASE_URL);
                self.fetch_openai_compatible_models(url, api_key).await
            }
            _ => {
                // Custom provider with provided base URL
                if let Some(base_url) = custom_base_url {
                    self.fetch_openai_compatible_models(base_url, api_key).await
                } else {
                    Err(ProviderError::UnsupportedModel {
                        model: "unknown".to_string(),
                        provider: provider.to_string(),
                    }
                    .into())
                }
            }
        };

        match models_res {
            Ok(models) => {
                // Update cache
                let mut cache = self.load_cache();
                cache
                    .providers
                    .insert(provider_lower.clone(), models.clone());
                cache
                    .last_updated
                    .insert(provider_lower, chrono::Utc::now().to_rfc3339());
                self.save_cache(&cache);
                Ok(models)
            }
            Err(e) => {
                // Fallback to cache if available
                let cache = self.load_cache();
                if let Some(cached_models) = cache.providers.get(&provider_lower) {
                    if !cached_models.is_empty() {
                        tracing::warn!(
                            "Failed to fetch live models from {}, using {} cached models",
                            provider,
                            cached_models.len()
                        );
                        return Ok(cached_models.clone());
                    }
                }
                Err(e)
            }
        }
    }

    /// Fetches live models from OpenRouter API
    async fn fetch_openrouter_models(&self, api_key: &str) -> Result<Vec<ModelInfo>> {
        let mut req = self
            .client
            .get(crate::constants::OPENROUTER_MODELS_URL)
            .header("HTTP-Referer", "https://github.com/aswin402/minicode")
            .header("X-Title", "minicode");

        if !api_key.is_empty() {
            req = req.header("Authorization", format!("Bearer {}", api_key));
        }

        let resp = req.send().await?;
        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let text = resp.text().await.unwrap_or_default();
            return Err(ProviderError::Api {
                status,
                message: format!("OpenRouter models error: {}", text),
            }
            .into());
        }

        let body: serde_json::Value = resp.json().await?;
        let mut models = Vec::new();

        if let Some(data) = body.get("data").and_then(|d| d.as_array()) {
            for item in data {
                if let Some(id) = item.get("id").and_then(|i| i.as_str()) {
                    let name = item
                        .get("name")
                        .and_then(|n| n.as_str())
                        .unwrap_or(id)
                        .to_string();
                    let description = item
                        .get("description")
                        .and_then(|d| d.as_str())
                        .map(|s| s.to_string());
                    let context_length = item
                        .get("context_length")
                        .and_then(|c| c.as_u64())
                        .map(|c| c as usize);

                    let is_free = id.ends_with(":free")
                        || item
                            .get("pricing")
                            .and_then(|p| p.get("prompt"))
                            .and_then(|pr| pr.as_str())
                            == Some("0");

                    models.push(ModelInfo {
                        id: id.to_string(),
                        name,
                        description,
                        context_length,
                        is_free,
                    });
                }
            }
        }

        // Sort: Free models first, then alphabetically by ID
        models.sort_by(|a, b| {
            if a.is_free != b.is_free {
                b.is_free.cmp(&a.is_free)
            } else {
                a.id.cmp(&b.id)
            }
        });

        Ok(models)
    }

    /// Fetches live models from Google Gemini API
    async fn fetch_gemini_models(&self, api_key: &str) -> Result<Vec<ModelInfo>> {
        let url = format!(
            "{}{}",
            crate::constants::GEMINI_MODELS_URL_TEMPLATE,
            api_key
        );

        let resp = self.client.get(&url).send().await?;
        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let text = resp.text().await.unwrap_or_default();
            return Err(ProviderError::Api {
                status,
                message: format!("Gemini models error: {}", text),
            }
            .into());
        }

        let body: serde_json::Value = resp.json().await?;
        let mut models = Vec::new();

        if let Some(data) = body.get("models").and_then(|m| m.as_array()) {
            for item in data {
                if let Some(full_name) = item.get("name").and_then(|n| n.as_str()) {
                    let id = full_name.trim_start_matches("models/").to_string();
                    // Filter models that support content generation
                    if let Some(methods) = item
                        .get("supportedGenerationMethods")
                        .and_then(|m| m.as_array())
                    {
                        let can_generate = methods
                            .iter()
                            .any(|m| m.as_str() == Some("generateContent"));
                        if !can_generate {
                            continue;
                        }
                    }

                    let name = item
                        .get("displayName")
                        .and_then(|d| d.as_str())
                        .unwrap_or(&id)
                        .to_string();
                    let description = item
                        .get("description")
                        .and_then(|d| d.as_str())
                        .map(|s| s.to_string());
                    let context_length = item
                        .get("inputTokenLimit")
                        .and_then(|c| c.as_u64())
                        .map(|c| c as usize);

                    models.push(ModelInfo {
                        id,
                        name,
                        description,
                        context_length,
                        is_free: false,
                    });
                }
            }
        }

        models.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(models)
    }

    /// Fetches live models from standard OpenAI-compatible `/models` endpoint
    async fn fetch_openai_compatible_models(
        &self,
        base_url: &str,
        api_key: &str,
    ) -> Result<Vec<ModelInfo>> {
        let url = format!("{}/models", base_url.trim_end_matches('/'));
        let mut req = self.client.get(&url);

        if !api_key.is_empty() {
            req = req.header("Authorization", format!("Bearer {}", api_key));
        }

        let resp = req.send().await?;
        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let text = resp.text().await.unwrap_or_default();
            return Err(ProviderError::Api {
                status,
                message: format!("Models endpoint error ({}): {}", status, text),
            }
            .into());
        }

        let body: serde_json::Value = resp.json().await?;
        let mut models = Vec::new();

        // Support both {"data": [...]} and {"models": [...]}
        let list = body
            .get("data")
            .and_then(|d| d.as_array())
            .or_else(|| body.get("models").and_then(|m| m.as_array()));

        if let Some(items) = list {
            for item in items {
                if let Some(id) = item
                    .get("id")
                    .and_then(|i| i.as_str())
                    .or_else(|| item.get("name").and_then(|n| n.as_str()))
                {
                    models.push(ModelInfo {
                        id: id.to_string(),
                        name: id.to_string(),
                        description: None,
                        context_length: None,
                        is_free: id.contains("free"),
                    });
                }
            }
        }

        models.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(models)
    }
}

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

/// Universal semantic capability tiers decoupling agents and configurations
/// from specific provider model identifiers and revisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SemanticTier {
    /// Newest, most capable, or recommended model from the active provider
    Auto,
    /// Highest capability, architectural, or complex reasoning model
    Smart,
    /// Lowest latency, cost-effective, high-throughput model (e.g. Flash, Haiku, Mini)
    Fast,
    /// Extended thinking or dedicated reasoning model (e.g. thinking-exp, o1, o3, R1)
    Reasoning,
}

impl SemanticTier {
    /// Parses a semantic tier loosely from a string (e.g. ":fast", "fast", ":smart", ":auto")
    pub fn from_str_loose(s: &str) -> Option<Self> {
        let trimmed = s.trim().trim_start_matches(':').to_lowercase();
        match trimmed.as_str() {
            "auto" | "default" | "" => Some(SemanticTier::Auto),
            "smart" | "pro" | "best" | "architect" => Some(SemanticTier::Smart),
            "fast" | "flash" | "mini" | "haiku" | "small" | "quick" => Some(SemanticTier::Fast),
            "reasoning" | "thinking" | "reasoner" | "deep" => Some(SemanticTier::Reasoning),
            _ => None,
        }
    }
}

/// Extracts a semantic numerical version score (major, minor, patch/date) from a model ID string.
/// Higher numbers indicate newer/higher capability models.
/// Examples:
/// - "gemini-3.7-flash" -> (3, 7, 0)
/// - "gemini-2.5-flash" -> (2, 5, 0)
/// - "gemini-2.0-flash" -> (2, 0, 0)
/// - "gemini-1.5-pro"   -> (1, 5, 0)
/// - "claude-3-7-sonnet-20250219" -> (3, 7, 20250219)
/// - "claude-3-5-sonnet-20241022" -> (3, 5, 20241022)
/// - "gpt-4.5-preview"  -> (4, 5, 0)
/// - "gpt-4o"           -> (4, 0, 0)
/// - "o3-mini"          -> (3, 0, 0)
/// - "o1"               -> (1, 0, 0)
pub fn extract_version_score(id: &str) -> (u32, u32, u32) {
    let lower = id.to_lowercase();

    // 1. Check for trailing date stamp like -20250219 or 20241022
    let mut date_stamp = 0u32;
    for part in lower.split(|c: char| !c.is_alphanumeric()) {
        if part.len() == 8 && (part.starts_with("202") || part.starts_with("203")) {
            if let Ok(d) = part.parse::<u32>() {
                date_stamp = d;
            }
        }
    }

    // 2. Scan for major.minor patterns e.g. "3.7", "2.5", "2.0", "1.5", "3.5", "4.5", "3.3"
    let mut major = 0u32;
    let mut minor = 0u32;

    let parts: Vec<&str> = lower.split(['-', '/', '_', ':']).collect();
    for (i, part) in parts.iter().enumerate() {
        if let Some((maj_str, min_str)) = part.split_once('.') {
            let maj_digits: String = maj_str.chars().filter(|c| c.is_ascii_digit()).collect();
            let min_digits: String = min_str.chars().take_while(|c| c.is_ascii_digit()).collect();
            if let (Ok(maj), Ok(min)) = (maj_digits.parse::<u32>(), min_digits.parse::<u32>()) {
                if maj < 100 && min < 100 && (maj, min) > (major, minor) {
                    major = maj;
                    minor = min;
                }
            }
        } else if let Some(stripped) = part.strip_prefix('o') {
            if let Ok(num) = stripped
                .chars()
                .take_while(|c| c.is_ascii_digit())
                .collect::<String>()
                .parse::<u32>()
            {
                if num < 100 && (num, 0) > (major, minor) {
                    major = num;
                }
            }
        } else if let Some(stripped) = part.strip_prefix('v') {
            if let Ok(num) = stripped
                .chars()
                .take_while(|c| c.is_ascii_digit())
                .collect::<String>()
                .parse::<u32>()
            {
                if num < 100 && (num, 0) > (major, minor) {
                    major = num;
                }
            }
        } else if i + 1 < parts.len() {
            // Check consecutive parts like "3" and "7" in "claude-3-7-sonnet"
            if let (Ok(maj), Ok(min)) = (part.parse::<u32>(), parts[i + 1].parse::<u32>()) {
                if maj < 10 && min < 100 && (maj, min) > (major, minor) {
                    major = maj;
                    minor = min;
                }
            }
        }
    }

    // 3. Fallback to any standalone single digit if major is still 0
    if major == 0 {
        for part in lower.split(|c: char| !c.is_alphanumeric()) {
            if let Ok(num) = part.parse::<u32>() {
                if num < 100 {
                    major = num;
                    break;
                }
            }
        }
    }

    (major, minor, date_stamp)
}

/// Dynamically filters and selects the most optimal model from an active list for a semantic tier.
pub fn select_best_model_for_tier(models: &[ModelInfo], tier: SemanticTier) -> Option<String> {
    if models.is_empty() {
        return None;
    }

    // Filter out non-chat / utility models
    let chat_models: Vec<&ModelInfo> = models
        .iter()
        .filter(|m| {
            let id = m.id.to_lowercase();
            !id.contains("embed")
                && !id.contains("whisper")
                && !id.contains("tts")
                && !id.contains("audio")
                && !id.contains("moderation")
                && !id.contains("dall-e")
                && !id.contains("image")
                && !id.contains("babbage")
                && !id.contains("davinci")
        })
        .collect();

    let pool = if chat_models.is_empty() {
        models.iter().collect::<Vec<_>>()
    } else {
        chat_models
    };

    match tier {
        SemanticTier::Fast => {
            // Prefer flash, haiku, mini, small, 8b, fast
            let fast_matches: Vec<&ModelInfo> = pool
                .iter()
                .filter(|m| {
                    let id = m.id.to_lowercase();
                    id.contains("flash")
                        || id.contains("haiku")
                        || id.contains("mini")
                        || id.contains("small")
                        || id.contains("8b")
                        || id.contains("fast")
                })
                .copied()
                .collect();

            let candidates = if !fast_matches.is_empty() {
                fast_matches
            } else {
                pool
            };

            candidates
                .iter()
                .max_by_key(|m| extract_version_score(&m.id))
                .map(|m| m.id.clone())
        }
        SemanticTier::Reasoning => {
            // Prefer thinking, reasoning, reasoner, o1, o3, r1
            let reasoning_matches: Vec<&ModelInfo> = pool
                .iter()
                .filter(|m| {
                    let id = m.id.to_lowercase();
                    id.contains("thinking")
                        || id.contains("reason")
                        || id.contains("r1")
                        || id.starts_with("o1")
                        || id.starts_with("o3")
                        || id.contains("-o1")
                        || id.contains("-o3")
                })
                .copied()
                .collect();

            if !reasoning_matches.is_empty() {
                reasoning_matches
                    .iter()
                    .max_by_key(|m| extract_version_score(&m.id))
                    .map(|m| m.id.clone())
            } else {
                // Fallback to Smart if no reasoning model found
                select_best_model_for_tier(models, SemanticTier::Smart)
            }
        }
        SemanticTier::Smart | SemanticTier::Auto => {
            // For Smart / Auto, rank by version score.
            // If equal version, prefer pro / sonnet / plus over flash / mini for smart tier
            pool.iter()
                .max_by(|a, b| {
                    let score_a = extract_version_score(&a.id);
                    let score_b = extract_version_score(&b.id);
                    if score_a != score_b {
                        score_a.cmp(&score_b)
                    } else {
                        let a_pref = a.id.contains("pro")
                            || a.id.contains("sonnet")
                            || a.id.contains("opus")
                            || a.id.contains("plus");
                        let b_pref = b.id.contains("pro")
                            || b.id.contains("sonnet")
                            || b.id.contains("opus")
                            || b.id.contains("plus");
                        a_pref.cmp(&b_pref)
                    }
                })
                .map(|m| m.id.clone())
        }
    }
}

/// Dynamic Model Resolver that resolves active and fallback models for all providers
/// without requiring static hardcoded strings.
pub struct DynamicModelResolver;

impl DynamicModelResolver {
    /// Synchronously resolves the active model for a provider from local cache or dynamic heuristics.
    /// Never blocks on network requests; safe for immediate UI and CLI startup.
    pub fn resolve_model_sync(provider: &str, requested: Option<&str>) -> String {
        let norm_prov = provider.to_lowercase();
        let req_trim = requested.unwrap_or("").trim();

        // If explicit model name that is not a semantic tier like :auto, :smart, :fast, :reasoning
        if !req_trim.is_empty() && !req_trim.starts_with(':') {
            return req_trim.to_string();
        }

        let tier = SemanticTier::from_str_loose(req_trim).unwrap_or(SemanticTier::Auto);

        let fetcher = ModelFetcher::new();
        if let Some(models) = fetcher.get_cached_models(&norm_prov) {
            if let Some(best) = select_best_model_for_tier(&models, tier) {
                return best;
            }
        }

        Self::canonical_fallback(&norm_prov, tier)
    }

    /// Asynchronously resolves the model for a provider, querying the live /models API
    /// if the local cache is empty or stale.
    #[allow(dead_code)]
    pub async fn resolve_model_async(
        provider: &str,
        requested: Option<&str>,
        api_key: &str,
        custom_base_url: Option<&str>,
    ) -> String {
        let norm_prov = provider.to_lowercase();
        let req_trim = requested.unwrap_or("").trim();

        if !req_trim.is_empty() && !req_trim.starts_with(':') {
            return req_trim.to_string();
        }

        let tier = SemanticTier::from_str_loose(req_trim).unwrap_or(SemanticTier::Auto);

        let fetcher = ModelFetcher::new();
        if !api_key.is_empty()
            || norm_prov == "ollama"
            || norm_prov == "lmstudio"
            || custom_base_url.is_some()
        {
            if let Ok(models) = fetcher
                .fetch_models(&norm_prov, api_key, custom_base_url)
                .await
            {
                if let Some(best) = select_best_model_for_tier(&models, tier) {
                    return best;
                }
            }
        }

        Self::resolve_model_sync(provider, requested)
    }

    /// Returns the next fallback model for a provider when a specific model encounters a 404 or deprecation.
    pub fn resolve_next_fallback(
        provider: &str,
        failed_model: &str,
        tier: SemanticTier,
    ) -> Option<String> {
        let fetcher = ModelFetcher::new();
        fetcher.invalidate_model(provider, failed_model);

        if let Some(models) = fetcher.get_cached_models(provider) {
            let remaining: Vec<ModelInfo> = models
                .into_iter()
                .filter(|m| m.id != failed_model)
                .collect();
            if let Some(next) = select_best_model_for_tier(&remaining, tier) {
                return Some(next);
            }
        }

        let fallback = Self::canonical_fallback(provider, tier);
        if fallback != failed_model {
            Some(fallback)
        } else {
            None
        }
    }

    /// Safe canonical fallback when offline and cache is empty
    pub fn canonical_fallback(provider: &str, tier: SemanticTier) -> String {
        let norm = provider.to_lowercase();
        match norm.as_str() {
            "gemini" | "google" => match tier {
                SemanticTier::Fast => "gemini-2.0-flash".to_string(),
                SemanticTier::Reasoning => "gemini-2.0-flash-thinking-exp".to_string(),
                SemanticTier::Smart | SemanticTier::Auto => "gemini-2.0-flash".to_string(),
            },
            "anthropic" | "claude" => match tier {
                SemanticTier::Fast => "claude-3-5-haiku-20241022".to_string(),
                SemanticTier::Reasoning | SemanticTier::Smart | SemanticTier::Auto => {
                    "claude-3-7-sonnet-20250219".to_string()
                }
            },
            "openai" => match tier {
                SemanticTier::Fast => "gpt-4o-mini".to_string(),
                SemanticTier::Reasoning => "o3-mini".to_string(),
                SemanticTier::Smart | SemanticTier::Auto => "gpt-4o".to_string(),
            },
            "deepseek" => match tier {
                SemanticTier::Reasoning => "deepseek-reasoner".to_string(),
                _ => "deepseek-chat".to_string(),
            },
            "groq" => "llama-3.3-70b-versatile".to_string(),
            "together" => "meta-llama/Llama-3.3-70B-Instruct-Turbo".to_string(),
            "minimax" => "MiniMax-Text-01".to_string(),
            "z.ai" | "z_ai" | "zhipu" | "glm" | "bigmodel" => "glm-4-flash".to_string(),
            "mistral" => "codestral-latest".to_string(),
            "ollama" => "qwen2.5-coder".to_string(),
            "openrouter" => "anthropic/claude-3.7-sonnet".to_string(),
            "lmstudio" | "lm-studio" | "vllm" | "local" | "localhost" | "localai" => {
                "local-model".to_string()
            }
            _ => "default-model".to_string(),
        }
    }
}

/// Extracts context window size encoded in the model name (e.g. `llama-3.1-8b-instruct-128k`, `qwen2.5-32k`, `gemini-1.5-pro-2m`)
pub fn parse_context_window_from_name(model: &str) -> Option<usize> {
    let lower = model.to_lowercase();
    for part in lower.split(|c: char| !c.is_alphanumeric()) {
        if let Some(num_str) = part.strip_suffix('m') {
            if let Ok(n) = num_str.parse::<usize>() {
                if n > 0 && n <= 100 {
                    return Some(n * 1_000_000);
                }
            }
        } else if let Some(num_str) = part.strip_suffix('k') {
            if let Ok(n) = num_str.parse::<usize>() {
                if n > 0 && n <= 10_000 {
                    let tokens = match n {
                        32 => 32_768,
                        64 => 65_536,
                        other => other * 1_000,
                    };
                    return Some(tokens);
                }
            }
        }
    }
    None
}

/// Looks up dynamic context length reported by live provider API from local disk cache
pub fn lookup_cached_model_context(model: &str) -> Option<usize> {
    let fetcher = ModelFetcher::new();
    let cache = fetcher.load_cache();
    let lower = model.to_lowercase();
    for models in cache.providers.values() {
        for m in models {
            let m_id = m.id.to_lowercase();
            if m_id == lower || lower.ends_with(&m_id) || m_id.ends_with(&lower) {
                if let Some(ctx) = m.context_length {
                    if ctx > 0 {
                        return Some(ctx);
                    }
                }
            }
        }
    }
    None
}

/// Resolves estimated max context window length using a 4-tier cascade:
/// 1. Local disk cache populated by dynamic provider API calls (Gemini, OpenRouter, Ollama)
/// 2. Automatic pattern extractor from model identifier (e.g. `-32k`, `-128k`, `-1m`, `-2m`)
/// 3. Well-known model family heuristics
/// 4. Safe standard default: 128,000 tokens
pub fn get_model_context_limit(model: &str) -> usize {
    if let Ok(val) = std::env::var("MINICODE_CONTEXT_WINDOW") {
        if let Ok(limit) = val.parse::<usize>() {
            if limit >= 1024 {
                return limit;
            }
        }
    }

    if let Some(cached) = lookup_cached_model_context(model) {
        return cached;
    }

    if let Some(parsed) = parse_context_window_from_name(model) {
        return parsed;
    }

    let lower = model.to_lowercase();
    if lower.contains("gemini-1.5-pro") || lower.contains("2m") {
        crate::constants::CONTEXT_WINDOW_2M
    } else if lower.contains("gemini-2")
        || lower.contains("gemini-1.5")
        || lower.contains("1m")
        || lower.contains("minimax-text-01")
    {
        crate::constants::CONTEXT_WINDOW_1M
    } else if lower.contains("claude-3-7")
        || lower.contains("claude-3-5")
        || lower.contains("claude-3")
        || lower.contains("sonnet")
        || lower.contains("opus")
        || lower.contains("codestral")
        || lower.contains("mistral-large")
    {
        crate::constants::CONTEXT_WINDOW_200K
    } else if lower.contains("ollama")
        || lower.contains("localhost")
        || lower.contains("localai")
        || lower.contains("lmstudio")
        || lower.contains("jan")
        || lower.contains(":0.5b")
        || lower.contains(":1.5b")
        || lower.contains(":3b")
        || lower.contains(":7b")
        || lower.contains(":8b")
        || lower.contains(":14b")
        || lower.contains(":32b")
        || lower.contains(":34b")
        || lower.contains(":70b")
        || lower.contains(":72b")
        || lower.contains(":405b")
    {
        if lower.contains(":70b") || lower.contains(":72b") || lower.contains(":405b") {
            crate::constants::CONTEXT_WINDOW_64K
        } else if lower.contains(":32b") || lower.contains(":34b") {
            crate::constants::CONTEXT_WINDOW_32K
        } else if lower.contains(":7b") || lower.contains(":8b") || lower.contains(":14b") {
            crate::constants::CONTEXT_WINDOW_16K
        } else if lower.contains(":0.5b") || lower.contains(":1.5b") || lower.contains(":3b") {
            crate::constants::CONTEXT_WINDOW_8K
        } else if lower.contains("qwen-2.5")
            || lower.contains("qwen2.5")
            || lower.contains("llama-3.3")
            || lower.contains("llama-3.1")
        {
            crate::constants::CONTEXT_WINDOW_32K
        } else {
            crate::constants::CONTEXT_WINDOW_8K
        }
    } else if lower.contains("gpt-4o")
        || lower.contains("gpt-4.1")
        || lower.contains("o1")
        || lower.contains("o3")
        || lower.contains("o4")
        || lower.contains("glm")
        || lower.contains("z.ai")
        || lower.contains("zhipu")
        || lower.contains("deepseek")
        || lower.contains("qwen-2.5")
        || lower.contains("qwen2.5")
        || lower.contains("llama-3.1")
        || lower.contains("llama-3.2")
        || lower.contains("llama-3.3")
    {
        crate::constants::CONTEXT_WINDOW_128K
    } else if lower.contains("qwen")
        || lower.contains("liquid")
        || lower.contains("lfm")
        || lower.contains("north")
    {
        crate::constants::CONTEXT_WINDOW_64K
    } else if lower.contains("gemma") || lower.contains("llama-3") {
        crate::constants::CONTEXT_WINDOW_8K
    } else {
        crate::constants::CONTEXT_WINDOW_128K
    }
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

impl Default for ModelFetcher {
    fn default() -> Self {
        Self::new()
    }
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
            config_dir
                .join(crate::constants::CONFIG_DIR_NAME)
                .join(crate::constants::MODELS_CACHE_FILE)
        } else {
            PathBuf::from(crate::constants::WORKSPACE_DIR_NAME)
                .join(crate::constants::MODELS_CACHE_FILE)
        };

        Self { client, cache_path }
    }

    /// Returns cached models for a provider if available
    pub fn get_cached_models(&self, provider: &str) -> Option<Vec<ModelInfo>> {
        let cache = self.load_cache();
        cache.providers.get(&provider.to_lowercase()).cloned()
    }

    /// Removes a model from disk cache when an API returns 404 (model not found / deprecated)
    pub fn invalidate_model(&self, provider: &str, model_id: &str) {
        let mut cache = self.load_cache();
        let norm = provider.to_lowercase();
        if let Some(list) = cache.providers.get_mut(&norm) {
            list.retain(|m| m.id != model_id && !m.id.ends_with(model_id));
        }
        self.save_cache(&cache);
    }

    /// Loads cached models from disk if available
    fn load_cache(&self) -> ModelsCache {
        match std::fs::read_to_string(&self.cache_path) {
            Ok(content) => match serde_json::from_str::<ModelsCache>(&content) {
                Ok(cache) => cache,
                Err(e) => {
                    tracing::warn!(
                        path = %self.cache_path.display(),
                        error = %e,
                        "Corrupted model cache file, falling back to defaults"
                    );
                    ModelsCache::default()
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => ModelsCache::default(),
            Err(e) => {
                tracing::warn!(
                    path = %self.cache_path.display(),
                    error = %e,
                    "Failed to read model cache file"
                );
                ModelsCache::default()
            }
        }
    }

    /// Saves updated model cache to disk
    fn save_cache(&self, cache: &ModelsCache) {
        if let Some(parent) = self.cache_path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                tracing::warn!(
                    path = %parent.display(),
                    error = %e,
                    "Failed to create model cache parent directory"
                );
                return;
            }
        }
        match serde_json::to_string_pretty(cache) {
            Ok(json) => {
                if let Err(e) = std::fs::write(&self.cache_path, json) {
                    tracing::warn!(
                        path = %self.cache_path.display(),
                        error = %e,
                        "Failed to write model cache file"
                    );
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "Failed to serialize model cache JSON");
            }
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
            "anthropic" | "claude" => self.fetch_anthropic_models(api_key).await,
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
            "minimax" => {
                let url = custom_base_url.unwrap_or(crate::constants::MINIMAX_BASE_URL);
                self.fetch_openai_compatible_models(url, api_key).await
            }
            "z.ai" | "z_ai" | "zhipu" | "glm" | "bigmodel" => {
                let url = custom_base_url.unwrap_or(crate::constants::ZHIPU_BASE_URL);
                self.fetch_openai_compatible_models(url, api_key).await
            }
            "mistral" => {
                let url = custom_base_url.unwrap_or(crate::constants::MISTRAL_BASE_URL);
                self.fetch_openai_compatible_models(url, api_key).await
            }
            "ollama" => {
                let url = custom_base_url.unwrap_or(crate::constants::OLLAMA_DEFAULT_BASE_URL);
                self.fetch_openai_compatible_models(url, api_key).await
            }
            "lmstudio" | "lm-studio" => {
                let url = custom_base_url.unwrap_or(crate::constants::LMSTUDIO_DEFAULT_BASE_URL);
                self.fetch_openai_compatible_models(url, api_key).await
            }
            "vllm" => {
                let url = custom_base_url.unwrap_or(crate::constants::VLLM_DEFAULT_BASE_URL);
                self.fetch_openai_compatible_models(url, api_key).await
            }
            "local" | "localhost" | "localai" | "llama.cpp" | "llamacpp" | "jan" => {
                let url = custom_base_url.unwrap_or(crate::constants::LOCALAI_DEFAULT_BASE_URL);
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

                // Fallback to static defaults for local providers if offline
                let defaults = match provider_lower.as_str() {
                    "anthropic" | "claude" => vec![
                        ModelInfo {
                            id: "claude-3-7-sonnet-20250219".to_string(),
                            name: "Claude 3.7 Sonnet (Hybrid Reasoning)".to_string(),
                            description: Some(
                                "Anthropic flagship hybrid reasoning & coding model".to_string(),
                            ),
                            context_length: Some(200_000),
                            is_free: false,
                        },
                        ModelInfo {
                            id: "claude-3-5-sonnet-20241022".to_string(),
                            name: "Claude 3.5 Sonnet v2".to_string(),
                            description: Some(
                                "High-speed intelligence and coding benchmark leader".to_string(),
                            ),
                            context_length: Some(200_000),
                            is_free: false,
                        },
                        ModelInfo {
                            id: "claude-3-5-haiku-20241022".to_string(),
                            name: "Claude 3.5 Haiku".to_string(),
                            description: Some("Ultra-fast compact reasoning model".to_string()),
                            context_length: Some(200_000),
                            is_free: false,
                        },
                    ],
                    "ollama" => vec![
                        ModelInfo {
                            id: "qwen2.5-coder".to_string(),
                            name: "qwen2.5-coder".to_string(),
                            description: Some("Ollama default code model".to_string()),
                            context_length: Some(32_768),
                            is_free: true,
                        },
                        ModelInfo {
                            id: "llama3.2".to_string(),
                            name: "llama3.2".to_string(),
                            description: Some("Meta LLaMA 3.2 local model".to_string()),
                            context_length: Some(128_000),
                            is_free: true,
                        },
                    ],
                    "lmstudio" | "lm-studio" => vec![ModelInfo {
                        id: "local-model".to_string(),
                        name: "Loaded LM Studio Model".to_string(),
                        description: Some("Currently active model loaded in LM Studio".to_string()),
                        context_length: Some(128_000),
                        is_free: true,
                    }],
                    "vllm" => vec![ModelInfo {
                        id: "default".to_string(),
                        name: "vLLM Served Model".to_string(),
                        description: Some("Model served by vLLM local instance".to_string()),
                        context_length: Some(128_000),
                        is_free: true,
                    }],
                    "local" | "localhost" | "localai" | "llama.cpp" | "llamacpp" | "jan" => {
                        vec![ModelInfo {
                            id: "local-model".to_string(),
                            name: "Localhost AI Model".to_string(),
                            description: Some("Default local model on localhost:8080".to_string()),
                            context_length: Some(128_000),
                            is_free: true,
                        }]
                    }
                    _ => Vec::new(),
                };

                if !defaults.is_empty() {
                    return Ok(defaults);
                }

                Err(e)
            }
        }
    }

    /// Fetches live models from Anthropic API
    async fn fetch_anthropic_models(&self, api_key: &str) -> Result<Vec<ModelInfo>> {
        let mut req = self
            .client
            .get(crate::constants::ANTHROPIC_MODELS_URL)
            .header(
                "anthropic-version",
                crate::constants::ANTHROPIC_VERSION_HEADER,
            );

        if !api_key.is_empty() {
            req = req.header("x-api-key", api_key);
        }

        let resp = req.send().await?;
        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let text = resp.text().await.unwrap_or_default();
            return Err(ProviderError::Api {
                status,
                message: format!("Anthropic models error: {}", text),
            }
            .into());
        }

        let body: serde_json::Value = resp.json().await?;
        let mut models = Vec::new();
        if let Some(data) = body.get("data").and_then(|d| d.as_array()) {
            for item in data {
                if let Some(id) = item.get("id").and_then(|i| i.as_str()) {
                    let name = item
                        .get("display_name")
                        .and_then(|n| n.as_str())
                        .unwrap_or(id)
                        .to_string();
                    let context_length = Some(get_model_context_limit(id));
                    models.push(ModelInfo {
                        id: id.to_string(),
                        name,
                        description: Some("Anthropic Claude model".to_string()),
                        context_length,
                        is_free: false,
                    });
                }
            }
        }

        if models.is_empty() {
            models = vec![
                ModelInfo {
                    id: "claude-3-7-sonnet-20250219".to_string(),
                    name: "Claude 3.7 Sonnet (Hybrid Reasoning)".to_string(),
                    description: Some(
                        "Anthropic flagship hybrid reasoning & coding model".to_string(),
                    ),
                    context_length: Some(200_000),
                    is_free: false,
                },
                ModelInfo {
                    id: "claude-3-5-sonnet-20241022".to_string(),
                    name: "Claude 3.5 Sonnet v2".to_string(),
                    description: Some(
                        "High-speed intelligence and coding benchmark leader".to_string(),
                    ),
                    context_length: Some(200_000),
                    is_free: false,
                },
                ModelInfo {
                    id: "claude-3-5-haiku-20241022".to_string(),
                    name: "Claude 3.5 Haiku".to_string(),
                    description: Some("Ultra-fast compact reasoning model".to_string()),
                    context_length: Some(200_000),
                    is_free: false,
                },
            ];
        }

        Ok(models)
    }

    /// Fetches live models from OpenRouter API
    async fn fetch_openrouter_models(&self, api_key: &str) -> Result<Vec<ModelInfo>> {
        let mut req = self
            .client
            .get(crate::constants::OPENROUTER_MODELS_URL)
            .header("HTTP-Referer", crate::constants::PROJECT_REPO_URL)
            .header("X-Title", crate::constants::APP_NAME);

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
                            .is_some_and(|pr| match pr {
                                serde_json::Value::String(s) => s.parse::<f64>() == Ok(0.0),
                                serde_json::Value::Number(n) => n.as_f64() == Some(0.0),
                                _ => false,
                            });

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
        let url = crate::constants::GEMINI_MODELS_URL;

        let resp = self
            .client
            .get(url)
            .header("x-goog-api-key", api_key)
            .send()
            .await?;
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
                    let context_length = item
                        .get("context_length")
                        .or_else(|| item.get("max_model_len"))
                        .and_then(|c| c.as_u64())
                        .map(|c| c as usize)
                        .or_else(|| parse_context_window_from_name(id));

                    models.push(ModelInfo {
                        id: id.to_string(),
                        name: id.to_string(),
                        description: None,
                        context_length,
                        is_free: id.contains("free"),
                    });
                }
            }
        }

        models.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(models)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_context_window_from_name() {
        assert_eq!(
            parse_context_window_from_name("llama-3.1-8b-instruct-128k"),
            Some(128_000)
        );
        assert_eq!(
            parse_context_window_from_name("qwen2.5-coder-32k"),
            Some(32_768)
        );
        assert_eq!(
            parse_context_window_from_name("mistral-small-24b-64k"),
            Some(65_536)
        );
        assert_eq!(
            parse_context_window_from_name("custom-model-1m"),
            Some(1_000_000)
        );
        assert_eq!(
            parse_context_window_from_name("gemini-1.5-pro-2m"),
            Some(2_000_000)
        );
        assert_eq!(
            parse_context_window_from_name("deepseek-v3:64k"),
            Some(65_536)
        );
        // 32b parameter size should NOT match as a context window
        assert_eq!(parse_context_window_from_name("qwen2.5-coder:32b"), None);
    }

    #[test]
    fn test_get_model_context_limit_heuristics() {
        assert_eq!(get_model_context_limit("gemini-1.5-pro"), 2_000_000);
        assert_eq!(get_model_context_limit("gemini-2.0-flash"), 1_000_000);
        assert_eq!(get_model_context_limit("claude-3-5-sonnet"), 200_000);
        assert_eq!(get_model_context_limit("gpt-4o"), 128_000);
        // Live API cache reports 163,840; fallback heuristic is 128,000
        assert!(get_model_context_limit("deepseek-chat") >= 128_000);
        assert_eq!(get_model_context_limit("unknown-fine-tune-32k"), 32_768);
        assert_eq!(
            get_model_context_limit("completely-unknown-custom-model"),
            128_000
        );
        assert_eq!(
            get_model_context_limit("ollama/qwen2.5-coder:32b"),
            crate::constants::CONTEXT_WINDOW_32K
        );
        assert_eq!(
            get_model_context_limit("ollama/llama3.3:70b"),
            crate::constants::CONTEXT_WINDOW_64K
        );
    }

    #[test]
    fn test_extract_version_score() {
        assert!(
            extract_version_score("gemini-3.7-flash") > extract_version_score("gemini-2.5-flash")
        );
        assert!(
            extract_version_score("gemini-2.5-flash") > extract_version_score("gemini-2.0-flash")
        );
        assert!(
            extract_version_score("gemini-2.0-flash") > extract_version_score("gemini-1.5-pro")
        );
        assert!(
            extract_version_score("claude-3-7-sonnet-20250219")
                > extract_version_score("claude-3-5-sonnet-20241022")
        );
        assert!(extract_version_score("gpt-4.5-preview") > extract_version_score("gpt-4o"));
        assert_eq!(extract_version_score("gemini-3.7-flash").0, 3);
        assert_eq!(extract_version_score("gemini-3.7-flash").1, 7);
    }

    #[test]
    fn test_select_best_model_for_tier_gemini() {
        let models = vec![
            ModelInfo {
                id: "gemini-1.5-pro".to_string(),
                name: "Gemini 1.5 Pro".to_string(),
                description: None,
                context_length: Some(2_000_000),
                is_free: false,
            },
            ModelInfo {
                id: "gemini-2.0-flash".to_string(),
                name: "Gemini 2.0 Flash".to_string(),
                description: None,
                context_length: Some(1_000_000),
                is_free: false,
            },
            ModelInfo {
                id: "gemini-2.5-flash".to_string(),
                name: "Gemini 2.5 Flash".to_string(),
                description: None,
                context_length: Some(1_000_000),
                is_free: false,
            },
            ModelInfo {
                id: "gemini-3.7-flash".to_string(),
                name: "Gemini 3.7 Flash".to_string(),
                description: None,
                context_length: Some(1_000_000),
                is_free: false,
            },
            ModelInfo {
                id: "text-embedding-004".to_string(),
                name: "Text Embedding 004".to_string(),
                description: None,
                context_length: Some(2_048),
                is_free: false,
            },
        ];

        // Fast tier selects newest flash model automatically without hardcoding
        assert_eq!(
            select_best_model_for_tier(&models, SemanticTier::Fast),
            Some("gemini-3.7-flash".to_string())
        );

        // Auto tier also resolves to the highest capability model
        assert_eq!(
            select_best_model_for_tier(&models, SemanticTier::Auto),
            Some("gemini-3.7-flash".to_string())
        );
    }

    #[test]
    fn test_dynamic_model_resolver_sync_fallback() {
        // Explicit model name passes through verbatim
        assert_eq!(
            DynamicModelResolver::resolve_model_sync("gemini", Some("custom-model-x")),
            "custom-model-x"
        );

        // Semantic tiers resolve cleanly
        assert!(!DynamicModelResolver::resolve_model_sync("gemini", Some(":auto")).is_empty());
        assert!(!DynamicModelResolver::resolve_model_sync("anthropic", Some(":fast")).is_empty());
        assert!(!DynamicModelResolver::resolve_model_sync("openai", Some(":reasoning")).is_empty());
    }
}

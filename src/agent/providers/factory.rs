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

type OpenAiCompatSpec = (
    &'static [&'static str],
    &'static str,
    &'static str,
    &'static str,
    Option<&'static str>,
);

const OPENAI_COMPAT_SPECS: &[OpenAiCompatSpec] = &[
    (
        &["deepseek"],
        "deepseek",
        crate::constants::DEEPSEEK_BASE_URL,
        crate::constants::DEEPSEEK_DEFAULT_MODEL,
        None,
    ),
    (
        &["groq"],
        "groq",
        crate::constants::GROQ_BASE_URL,
        crate::constants::GROQ_DEFAULT_MODEL,
        None,
    ),
    (
        &["together"],
        "together",
        crate::constants::TOGETHER_BASE_URL,
        crate::constants::TOGETHER_DEFAULT_MODEL,
        None,
    ),
    (
        &["minimax"],
        "minimax",
        crate::constants::MINIMAX_BASE_URL,
        crate::constants::MINIMAX_DEFAULT_MODEL,
        None,
    ),
    (
        &["z.ai", "z_ai", "zhipu", "glm", "bigmodel"],
        "z.ai",
        crate::constants::ZHIPU_BASE_URL,
        crate::constants::ZHIPU_DEFAULT_MODEL,
        None,
    ),
    (
        &["mistral"],
        "mistral",
        crate::constants::MISTRAL_BASE_URL,
        crate::constants::MISTRAL_DEFAULT_MODEL,
        None,
    ),
    (
        &["huggingface", "hf"],
        "huggingface",
        crate::constants::HUGGINGFACE_BASE_URL,
        crate::constants::HUGGINGFACE_DEFAULT_MODEL,
        None,
    ),
    (
        &["nvidia", "nim", "nvidia-nim"],
        "nvidia",
        crate::constants::NVIDIA_NIM_BASE_URL,
        crate::constants::NVIDIA_NIM_DEFAULT_MODEL,
        None,
    ),
    (
        &["opencode", "opencode-zen", "zen"],
        "opencode",
        crate::constants::OPENCODE_ZEN_BASE_URL,
        crate::constants::OPENCODE_ZEN_DEFAULT_MODEL,
        None,
    ),
    (
        &["cerebras"],
        "cerebras",
        crate::constants::CEREBRAS_BASE_URL,
        crate::constants::CEREBRAS_DEFAULT_MODEL,
        None,
    ),
    (
        &["agnes", "agnes-ai"],
        "agnes",
        crate::constants::AGNES_BASE_URL,
        crate::constants::AGNES_DEFAULT_MODEL,
        None,
    ),
    (
        &["nara", "nararouter", "bynara"],
        "nara",
        crate::constants::NARA_BASE_URL,
        crate::constants::NARA_DEFAULT_MODEL,
        None,
    ),
    (
        &["ofox", "ofoxai"],
        "ofox",
        crate::constants::OFOX_BASE_URL,
        crate::constants::OFOX_DEFAULT_MODEL,
        None,
    ),
    (
        &["aihubmix"],
        "aihubmix",
        crate::constants::AIHUBMIX_BASE_URL,
        crate::constants::AIHUBMIX_DEFAULT_MODEL,
        None,
    ),
    (
        &["kimi", "moonshot", "moonshotai"],
        "kimi",
        crate::constants::KIMI_BASE_URL,
        crate::constants::KIMI_DEFAULT_MODEL,
        None,
    ),
    (
        &["ollama"],
        "ollama",
        crate::constants::OLLAMA_DEFAULT_BASE_URL,
        crate::constants::OLLAMA_DEFAULT_MODEL,
        Some("ollama"),
    ),
    (
        &["lmstudio", "lm-studio"],
        "lmstudio",
        crate::constants::LMSTUDIO_DEFAULT_BASE_URL,
        crate::constants::DEFAULT_LOCAL_MODEL_NAME,
        Some("lm-studio"),
    ),
    (
        &["vllm"],
        "vllm",
        crate::constants::VLLM_DEFAULT_BASE_URL,
        crate::constants::VLLM_DEFAULT_MODEL,
        Some("none"),
    ),
];

/// Provider factory that supports custom base URLs for OpenAI-compatible endpoints
pub fn create_provider_with_base_url(
    provider_name: &str,
    api_key: &str,
    custom_base_url: Option<&str>,
) -> Result<Box<dyn Provider>> {
    let lower = provider_name.to_lowercase();
    match lower.as_str() {
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
        "local" | "localhost" | "localai" | "llama.cpp" | "llamacpp" | "jan" => {
            Ok(Box::new(OpenAiCompatibleProvider::new(
                provider_name,
                if api_key.is_empty() { "none" } else { api_key },
                custom_base_url.unwrap_or(crate::constants::LOCALAI_DEFAULT_BASE_URL),
                crate::constants::DEFAULT_LOCAL_MODEL_NAME,
            )))
        }
        custom_name => {
            if let Some(&(_, canonical, default_url, default_model, empty_key)) =
                OPENAI_COMPAT_SPECS
                    .iter()
                    .find(|(aliases, _, _, _, _)| aliases.contains(&custom_name))
            {
                let key = match empty_key {
                    Some(fallback) if api_key.is_empty() => fallback,
                    _ => api_key,
                };
                return Ok(Box::new(OpenAiCompatibleProvider::new(
                    canonical,
                    key,
                    custom_base_url.unwrap_or(default_url),
                    default_model,
                )));
            }

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

/// Creates a single provider without wrapping in a fallback chain.
pub fn create_single_provider_or_fallback(
    provider_name: &str,
    api_key: Result<String>,
    custom_base_url: Option<&str>,
) -> (Box<dyn Provider>, Option<String>) {
    match api_key
        .and_then(|key| create_provider_with_base_url(provider_name, &key, custom_base_url))
    {
        Ok(p) => (p, None),
        Err(e) => {
            let reason = e.to_string();
            (
                Box::new(UnconfiguredProvider::new(provider_name, &reason)),
                Some(reason),
            )
        }
    }
}

fn append_unique_fallback_slot(
    cfg: &crate::config::Config,
    label: &str,
    slot: &crate::config::ModelSlot,
    targets: &mut Vec<crate::agent::providers::resilient::FallbackTarget>,
    first_err: &mut Option<String>,
    any_ok: &mut bool,
) {
    if !slot.is_valid()
        || targets
            .iter()
            .any(|t| t.provider_name.eq_ignore_ascii_case(&slot.provider) && t.model == slot.model)
    {
        return;
    }
    let fb_key = cfg.get_api_key(&slot.provider);
    let fb_url = cfg.get_provider_base_url(&slot.provider);
    let (fb_prov, fb_err) =
        create_single_provider_or_fallback(&slot.provider, fb_key, fb_url.as_deref());
    if let Some(err) = fb_err {
        if first_err.is_none() {
            *first_err = Some(err);
        }
        tracing::debug!(
            slot = %label,
            provider = %slot.provider,
            "Skipping fallback slot because provider credentials are not configured"
        );
        return;
    }
    *any_ok = true;
    targets.push(crate::agent::providers::resilient::FallbackTarget {
        slot_label: label.to_string(),
        provider_name: slot.provider.clone(),
        model: slot.model.clone(),
        provider: fb_prov,
    });
}

fn create_provider_or_fallback_with_config(
    cfg: &crate::config::Config,
    provider_name: &str,
    api_key: Result<String>,
    custom_base_url: Option<&str>,
) -> (Box<dyn Provider>, Option<String>) {
    let (primary_prov, primary_err) =
        create_single_provider_or_fallback(provider_name, api_key, custom_base_url);

    let fallback_slots: [(&str, Option<&crate::config::ModelSlot>); 3] = [
        ("1st Fallback", cfg.provider.fallback_1.as_ref()),
        ("2nd Fallback", cfg.provider.fallback_2.as_ref()),
        (
            "Global Last Fallback ★",
            cfg.provider.global_last_fallback.as_ref(),
        ),
    ];

    if !fallback_slots
        .iter()
        .any(|(_, s)| s.is_some_and(|slot| slot.is_valid()))
    {
        return (primary_prov, primary_err);
    }

    let primary_model = if !cfg.provider.model.trim().is_empty()
        && cfg.provider.default.eq_ignore_ascii_case(provider_name)
    {
        cfg.provider.model.trim().to_string()
    } else {
        cfg.get_default_model_for_provider(provider_name)
    };

    let mut targets = Vec::with_capacity(4);
    targets.push(crate::agent::providers::resilient::FallbackTarget {
        slot_label: "Primary".to_string(),
        provider_name: provider_name.to_lowercase(),
        model: primary_model,
        provider: primary_prov,
    });

    let mut any_fallback_ok = primary_err.is_none();
    let mut unused_first_err = None;

    for (label, slot_opt) in fallback_slots {
        if let Some(slot) = slot_opt {
            append_unique_fallback_slot(
                cfg,
                label,
                slot,
                &mut targets,
                &mut unused_first_err,
                &mut any_fallback_ok,
            );
        }
    }

    if targets.len() == 1 {
        let only = targets.remove(0);
        return (only.provider, primary_err);
    }

    let chain_err = if any_fallback_ok { None } else { primary_err };
    (
        Box::new(crate::agent::providers::resilient::FallbackChainProvider::new(targets)),
        chain_err,
    )
}

/// Creates a provider (automatically wrapped in a 4-slot `FallbackChainProvider` if fallback slots
/// `[2] 1st Fallback`, `[3] 2nd Fallback`, or `[4] Global Last Fallback ★` are configured in `config.toml`),
/// or returns a safe `UnconfiguredProvider` fallback so that the application / TUI never crashes at startup.
pub fn create_provider_or_fallback(
    provider_name: &str,
    api_key: Result<String>,
    custom_base_url: Option<&str>,
) -> (Box<dyn Provider>, Option<String>) {
    let Ok(cfg) = crate::config::Config::load(None, None) else {
        return create_single_provider_or_fallback(provider_name, api_key, custom_base_url);
    };
    create_provider_or_fallback_with_config(&cfg, provider_name, api_key, custom_base_url)
}

/// Builds a 4-slot `FallbackChainProvider` from an explicit ordered list of `ModelSlot`s
/// (`[1] Primary -> [2] 1st Fallback -> [3] 2nd Fallback -> [4] Global Last Fallback ★`).
pub fn create_chain_provider_for_slots(
    config: &crate::config::Config,
    slots: &[crate::config::ModelSlot],
) -> (Box<dyn Provider>, Option<String>) {
    let mut targets = Vec::with_capacity(slots.len().max(1));
    let mut first_err: Option<String> = None;
    let mut any_ok = false;

    for (idx, slot) in slots.iter().enumerate() {
        let label = match idx {
            0 => "Primary",
            1 => "1st Fallback",
            2 => "2nd Fallback",
            _ => "Global Last Fallback ★",
        };
        append_unique_fallback_slot(
            config,
            label,
            slot,
            &mut targets,
            &mut first_err,
            &mut any_ok,
        );
    }

    if targets.is_empty() {
        return create_provider_or_fallback_with_config(
            config,
            &config.provider.default,
            config.get_api_key(&config.provider.default),
            config
                .get_provider_base_url(&config.provider.default)
                .as_deref(),
        );
    }

    let final_err = if any_ok { None } else { first_err };
    (
        Box::new(crate::agent::providers::resilient::FallbackChainProvider::new(targets)),
        final_err,
    )
}

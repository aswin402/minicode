use crate::agent::models::{get_model_context_limit, ModelFetcher};
use crate::agent::provider::ToolSchema;
use crate::config::{mask_api_key, Config};
use crate::error::{Result, ToolError};
use crate::tools::param;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::path::Path;

/// Return schemas for the 4 agent configuration management tools (Tools 136 - 139).
pub fn get_schemas() -> Vec<ToolSchema> {
    vec![
        ToolSchema {
            name: "get_agent_config".to_string(),
            description: "Inspect current agent configuration, active provider/model, workspace scope, execution preferences, and masked provider API keys.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolSchema {
            name: "update_agent_config".to_string(),
            description: "Request an update to the agent configuration (active provider, model, auto_approve, thinking budget, theme). Proposes structured modifications requiring explicit user approval.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "provider": {
                        "type": "string",
                        "description": "AI provider identifier (e.g. 'anthropic', 'openai', 'gemini', 'ollama')"
                    },
                    "model": {
                        "type": "string",
                        "description": "Model identifier (e.g. 'claude-3-7-sonnet-20250219', 'gpt-4o', 'qwen2.5-coder')"
                    },
                    "auto_approve": {
                        "type": "boolean",
                        "description": "Whether tool actions should be automatically approved without user confirmation"
                    },
                    "thinking_budget": {
                        "type": "integer",
                        "description": "Thinking / reasoning token budget in tokens (0 to disable)"
                    },
                    "theme": {
                        "type": "string",
                        "description": "TUI visual theme name (e.g. 'dark', 'light', 'nord', 'monokai', 'cyberpunk')"
                    },
                    "scope": {
                        "type": "string",
                        "enum": ["workspace", "global"],
                        "description": "Configuration scope to update ('workspace' or 'global', default: 'workspace')"
                    }
                }
            }),
        },
        ToolSchema {
            name: "test_provider_connection".to_string(),
            description: "Test connectivity, latency, and credential validity for an AI model provider endpoint without exposing raw API keys.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "provider": {
                        "type": "string",
                        "description": "Provider identifier (e.g. 'anthropic', 'openai', 'gemini', 'openrouter', 'deepseek', 'groq', 'mistral', 'ollama', 'lmstudio', or 'all')"
                    }
                },
                "required": ["provider"]
            }),
        },
        ToolSchema {
            name: "list_available_models".to_string(),
            description: "List available AI models from a provider with context window lengths and capabilities.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "provider": {
                        "type": "string",
                        "description": "Provider identifier (e.g. 'anthropic', 'openai', 'gemini', 'openrouter', 'deepseek', 'groq', 'mistral', 'ollama', 'lmstudio')"
                    }
                },
                "required": ["provider"]
            }),
        },
    ]
}

/// Structured proposal for configuration changes requiring user confirmation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConfigChangeProposal {
    pub scope: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub auto_approve: Option<bool>,
    pub thinking_budget: Option<usize>,
    pub theme: Option<String>,
}

/// Report summarizing provider connection status, latency, and zero-leak credential info.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderConnectionReport {
    pub provider: String,
    pub status: String,
    pub latency_ms: u64,
    pub http_status: Option<u16>,
    pub key_source: String,
    pub key_masked: String,
    pub model_count: usize,
    pub error: Option<String>,
}

/// Convenience alias for connection test reports used by modals and diagnostic tools.
pub type ConnectionTestResult = ProviderConnectionReport;

/// Information about an available model from a provider.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AvailableModelItem {
    pub id: String,
    pub name: String,
    pub context_length: usize,
    pub supports_reasoning: bool,
}

/// Known cloud providers tested during connection scans.
const CLOUD_PROVIDERS: [&str; 10] = [
    "anthropic",
    "gemini",
    "openai",
    "openrouter",
    "deepseek",
    "groq",
    "mistral",
    "together",
    "minimax",
    "z.ai",
];

/// Known local providers tested during connection scans.
const LOCAL_PROVIDERS: [&str; 2] = ["ollama", "lmstudio"];

/// Persist an approved `ConfigChangeProposal` to disk.
#[allow(dead_code)]
pub fn apply_proposal(proposal: &ConfigChangeProposal, workspace_root: &Path) -> Result<()> {
    match proposal.scope.as_str() {
        "global" => {
            let mut config = Config::load(None, None).unwrap_or_default();
            if let Some(ref p) = proposal.provider {
                config.provider.default = p.clone();
            }
            if let Some(ref m) = proposal.model {
                config.provider.model = m.clone();
            }
            if let Some(aa) = proposal.auto_approve {
                config.agent.auto_approve = aa;
            }
            if let Some(tb) = proposal.thinking_budget {
                config.provider.thinking_budget = if tb == 0 { None } else { Some(tb) };
            }
            if let Some(ref th) = proposal.theme {
                config.ui.theme = th.clone();
            }
            config.save(None)?;
        }
        _ => {
            // Workspace scope
            let minicode_dir = workspace_root.join(crate::constants::WORKSPACE_DIR_NAME);
            std::fs::create_dir_all(&minicode_dir).map_err(|e| ToolError::FileOp {
                path: minicode_dir.display().to_string(),
                source: e,
            })?;

            let mut config = Config::load(Some(workspace_root), None).unwrap_or_default();
            if let Some(ref p) = proposal.provider {
                config.provider.default = p.clone();
            }
            if let Some(ref m) = proposal.model {
                config.provider.model = m.clone();
            }
            if let Some(aa) = proposal.auto_approve {
                config.agent.auto_approve = aa;
            }
            if let Some(tb) = proposal.thinking_budget {
                config.provider.thinking_budget = if tb == 0 { None } else { Some(tb) };
            }
            if let Some(ref th) = proposal.theme {
                config.ui.theme = th.clone();
            }
            config.save(Some(workspace_root))?;

            if proposal.provider.is_some() || proposal.model.is_some() {
                let _ = Config::save_workspace_preference(
                    workspace_root,
                    &config.provider.default,
                    &config.provider.model,
                );
            }
        }
    }
    Ok(())
}

/// Identifies the configuration source of a provider's credential without revealing it.
fn determine_key_source(provider: &str, config: &Config) -> String {
    let norm = provider.to_lowercase();
    let env_var: Option<&str> = match norm.as_str() {
        "gemini" | "google" => {
            if std::env::var("GEMINI_API_KEY").is_ok() {
                Some("GEMINI_API_KEY")
            } else if std::env::var("GOOGLE_API_KEY").is_ok() {
                Some("GOOGLE_API_KEY")
            } else {
                None
            }
        }
        "anthropic" | "claude" => {
            if std::env::var("ANTHROPIC_API_KEY").is_ok() {
                Some("ANTHROPIC_API_KEY")
            } else {
                None
            }
        }
        "openrouter" => {
            if std::env::var("OPENROUTER_API_KEY").is_ok() {
                Some("OPENROUTER_API_KEY")
            } else if std::env::var("OPENROUTER_KEY").is_ok() {
                Some("OPENROUTER_KEY")
            } else {
                None
            }
        }
        "openai" => {
            if std::env::var("OPENAI_API_KEY").is_ok() {
                Some("OPENAI_API_KEY")
            } else {
                None
            }
        }
        "deepseek" => {
            if std::env::var("DEEPSEEK_API_KEY").is_ok() {
                Some("DEEPSEEK_API_KEY")
            } else {
                None
            }
        }
        "groq" => {
            if std::env::var("GROQ_API_KEY").is_ok() {
                Some("GROQ_API_KEY")
            } else {
                None
            }
        }
        "together" => {
            if std::env::var("TOGETHER_API_KEY").is_ok() {
                Some("TOGETHER_API_KEY")
            } else {
                None
            }
        }
        "minimax" => {
            if std::env::var("MINIMAX_API_KEY").is_ok() {
                Some("MINIMAX_API_KEY")
            } else {
                None
            }
        }
        "z.ai" | "z_ai" | "zhipu" | "glm" | "bigmodel" => {
            if std::env::var("ZHIPU_API_KEY").is_ok() {
                Some("ZHIPU_API_KEY")
            } else if std::env::var("Z_AI_API_KEY").is_ok() {
                Some("Z_AI_API_KEY")
            } else if std::env::var("GLM_API_KEY").is_ok() {
                Some("GLM_API_KEY")
            } else if std::env::var("BIGMODEL_API_KEY").is_ok() {
                Some("BIGMODEL_API_KEY")
            } else {
                None
            }
        }
        "mistral" => {
            if std::env::var("MISTRAL_API_KEY").is_ok() {
                Some("MISTRAL_API_KEY")
            } else {
                None
            }
        }
        _ => None,
    };

    if let Some(var) = env_var {
        return format!("environment ({})", var);
    }

    let custom_env = format!("{}_API_KEY", norm.to_uppercase().replace(['-', '.'], "_"));
    if std::env::var(&custom_env).is_ok() {
        return format!("environment ({})", custom_env);
    }

    if config.provider.api_keys.contains_key(&norm)
        || config.provider.api_keys.contains_key(provider)
    {
        return "config.toml ([provider.api_keys])".to_string();
    }

    if config.is_local_provider(provider) {
        return "none (local endpoint)".to_string();
    }

    "none (unconfigured)".to_string()
}

/// Evaluates whether a given model identifier or display name supports extended thinking / reasoning.
pub fn model_supports_reasoning(id: &str, name: &str) -> bool {
    let id_lower = id.to_lowercase();
    let name_lower = name.to_lowercase();
    id_lower.contains("reasoning")
        || id_lower.contains("thinking")
        || id_lower.contains("deepseek-r1")
        || id_lower.contains("r1")
        || id_lower.contains("o1")
        || id_lower.contains("o3")
        || id_lower.contains("o4")
        || id_lower.contains("qwq")
        || id_lower.contains("claude-3-7")
        || name_lower.contains("reasoning")
        || name_lower.contains("thinking")
}

/// Fallback static model catalog for when remote provider model endpoints are unreachable or unconfigured.
pub fn fallback_models_for_provider(provider: &str) -> Vec<AvailableModelItem> {
    let norm = provider.to_lowercase();
    match norm.as_str() {
        "anthropic" | "claude" => vec![
            AvailableModelItem {
                id: "claude-3-7-sonnet-20250219".to_string(),
                name: "Claude 3.7 Sonnet (Hybrid Reasoning)".to_string(),
                context_length: 200_000,
                supports_reasoning: true,
            },
            AvailableModelItem {
                id: "claude-3-5-sonnet-20241022".to_string(),
                name: "Claude 3.5 Sonnet v2".to_string(),
                context_length: 200_000,
                supports_reasoning: false,
            },
            AvailableModelItem {
                id: "claude-3-5-haiku-20241022".to_string(),
                name: "Claude 3.5 Haiku".to_string(),
                context_length: 200_000,
                supports_reasoning: false,
            },
        ],
        "openai" => vec![
            AvailableModelItem {
                id: "o3-mini".to_string(),
                name: "o3-mini (Reasoning)".to_string(),
                context_length: 200_000,
                supports_reasoning: true,
            },
            AvailableModelItem {
                id: "o1".to_string(),
                name: "o1 (Reasoning)".to_string(),
                context_length: 200_000,
                supports_reasoning: true,
            },
            AvailableModelItem {
                id: "gpt-4o".to_string(),
                name: "GPT-4o".to_string(),
                context_length: 128_000,
                supports_reasoning: false,
            },
            AvailableModelItem {
                id: "gpt-4o-mini".to_string(),
                name: "GPT-4o Mini".to_string(),
                context_length: 128_000,
                supports_reasoning: false,
            },
        ],
        "gemini" | "google" => vec![
            AvailableModelItem {
                id: "gemini-2.5-pro".to_string(),
                name: "Gemini 2.5 Pro (Thinking)".to_string(),
                context_length: 2_000_000,
                supports_reasoning: true,
            },
            AvailableModelItem {
                id: "gemini-2.0-flash".to_string(),
                name: "Gemini 2.0 Flash".to_string(),
                context_length: 1_000_000,
                supports_reasoning: false,
            },
            AvailableModelItem {
                id: "gemini-1.5-pro".to_string(),
                name: "Gemini 1.5 Pro".to_string(),
                context_length: 2_000_000,
                supports_reasoning: false,
            },
        ],
        "deepseek" => vec![
            AvailableModelItem {
                id: "deepseek-reasoner".to_string(),
                name: "DeepSeek R1 (Reasoning)".to_string(),
                context_length: 128_000,
                supports_reasoning: true,
            },
            AvailableModelItem {
                id: "deepseek-chat".to_string(),
                name: "DeepSeek V3".to_string(),
                context_length: 128_000,
                supports_reasoning: false,
            },
        ],
        "openrouter" => vec![
            AvailableModelItem {
                id: "anthropic/claude-3.7-sonnet".to_string(),
                name: "Claude 3.7 Sonnet".to_string(),
                context_length: 200_000,
                supports_reasoning: true,
            },
            AvailableModelItem {
                id: "openai/gpt-4o".to_string(),
                name: "GPT-4o".to_string(),
                context_length: 128_000,
                supports_reasoning: false,
            },
            AvailableModelItem {
                id: "deepseek/deepseek-r1".to_string(),
                name: "DeepSeek R1".to_string(),
                context_length: 128_000,
                supports_reasoning: true,
            },
        ],
        "groq" => vec![
            AvailableModelItem {
                id: "llama-3.3-70b-versatile".to_string(),
                name: "Llama 3.3 70B Versatile".to_string(),
                context_length: 128_000,
                supports_reasoning: false,
            },
            AvailableModelItem {
                id: "deepseek-r1-distill-llama-70b".to_string(),
                name: "DeepSeek R1 Distill Llama 70B".to_string(),
                context_length: 128_000,
                supports_reasoning: true,
            },
        ],
        "mistral" => vec![
            AvailableModelItem {
                id: "codestral-latest".to_string(),
                name: "Codestral Latest".to_string(),
                context_length: 256_000,
                supports_reasoning: false,
            },
            AvailableModelItem {
                id: "mistral-large-latest".to_string(),
                name: "Mistral Large Latest".to_string(),
                context_length: 128_000,
                supports_reasoning: false,
            },
        ],
        "together" => vec![AvailableModelItem {
            id: "meta-llama/Llama-3.3-70B-Instruct-Turbo".to_string(),
            name: "Llama 3.3 70B Turbo".to_string(),
            context_length: 128_000,
            supports_reasoning: false,
        }],
        "minimax" => vec![AvailableModelItem {
            id: "MiniMax-Text-01".to_string(),
            name: "MiniMax Text 01".to_string(),
            context_length: 1_000_000,
            supports_reasoning: false,
        }],
        "z.ai" | "z_ai" | "zhipu" | "glm" | "bigmodel" => vec![AvailableModelItem {
            id: "glm-4-plus".to_string(),
            name: "GLM-4 Plus".to_string(),
            context_length: 128_000,
            supports_reasoning: false,
        }],
        "ollama" => vec![
            AvailableModelItem {
                id: "qwen2.5-coder".to_string(),
                name: "Qwen 2.5 Coder".to_string(),
                context_length: 32_768,
                supports_reasoning: false,
            },
            AvailableModelItem {
                id: "deepseek-r1".to_string(),
                name: "DeepSeek R1".to_string(),
                context_length: 32_768,
                supports_reasoning: true,
            },
        ],
        "lmstudio" | "lm-studio" => vec![AvailableModelItem {
            id: "local-model".to_string(),
            name: "LM Studio Local Model".to_string(),
            context_length: 32_768,
            supports_reasoning: false,
        }],
        _ => {
            let default_model = Config::static_default_model_for_provider(provider);
            let ctx = get_model_context_limit(default_model);
            vec![AvailableModelItem {
                id: default_model.to_string(),
                name: format!("{} Default Model", provider),
                context_length: ctx,
                supports_reasoning: model_supports_reasoning(default_model, ""),
            }]
        }
    }
}

/// Tests connectivity for an individual provider endpoint while enforcing zero plaintext key leakage.
pub async fn test_single_provider(provider: &str, config: &Config) -> ProviderConnectionReport {
    let norm = provider.to_lowercase();
    let key_res = config.get_api_key(&norm);
    let is_local = config.is_local_provider(&norm);
    let key_source = determine_key_source(&norm, config);

    let key = match &key_res {
        Ok(k) => k.clone(),
        Err(_) if is_local => String::new(),
        Err(_) => {
            return ProviderConnectionReport {
                provider: provider.to_string(),
                status: "unconfigured".to_string(),
                latency_ms: 0,
                http_status: None,
                key_source,
                key_masked: String::new(),
                model_count: 0,
                error: Some("Provider API key is not configured".to_string()),
            };
        }
    };

    let key_masked = mask_api_key(&key);
    let custom_url = config
        .provider
        .custom_endpoints
        .get(&norm)
        .or_else(|| config.provider.custom_endpoints.get(provider))
        .map(|s| s.as_str());

    let start = std::time::Instant::now();
    let fetcher = ModelFetcher::new();
    let res = fetcher.fetch_models(&norm, &key, custom_url).await;
    let latency_ms = start.elapsed().as_millis() as u64;

    match res {
        Ok(models) => ProviderConnectionReport {
            provider: provider.to_string(),
            status: "connected".to_string(),
            latency_ms,
            http_status: Some(200),
            key_source,
            key_masked,
            model_count: models.len(),
            error: None,
        },
        Err(e) => {
            let raw_err = e.to_string();
            // Invariant: sanitize error string so plaintext keys never leak into logs, reports or TUI
            let sanitized_err = if !key.is_empty() {
                raw_err.replace(&key, &key_masked)
            } else {
                raw_err
            };

            let http_status = match e {
                crate::error::MinicodeError::Provider(crate::error::ProviderError::Api {
                    status,
                    ..
                }) => Some(status),
                _ => None,
            };

            ProviderConnectionReport {
                provider: provider.to_string(),
                status: "disconnected".to_string(),
                latency_ms,
                http_status,
                key_source,
                key_masked,
                model_count: 0,
                error: Some(sanitized_err),
            }
        }
    }
}

/// Tests connectivity for an individual provider endpoint (convenience wrapper).
#[allow(dead_code)]
pub async fn test_provider_connection(provider: &str, config: &Config) -> ConnectionTestResult {
    test_single_provider(provider, config).await
}

/// Tests connectivity for all supported cloud and local provider endpoints concurrently.
pub async fn test_all_provider_connections(config: &Config) -> Vec<ConnectionTestResult> {
    let mut all_providers = Vec::new();
    all_providers.extend_from_slice(&CLOUD_PROVIDERS);
    all_providers.extend_from_slice(&LOCAL_PROVIDERS);

    let futures = all_providers
        .into_iter()
        .map(|p| test_single_provider(p, config));
    futures::future::join_all(futures).await
}

/// Unified dispatcher routing calls for the 4 agent configuration management tools.
pub async fn dispatch(
    tool_name: &str,
    args: &serde_json::Value,
    workspace_root: &Path,
) -> Option<Result<String>> {
    match tool_name {
        "get_agent_config" => Some(
            async {
                let config = Config::load(Some(workspace_root), None).unwrap_or_default();

                let mut configured_providers = Vec::new();
                for &p in &CLOUD_PROVIDERS {
                    if let Ok(k) = config.get_api_key(p) {
                        if !k.trim().is_empty() {
                            configured_providers.push(p.to_string());
                        }
                    }
                }
                for &p in &LOCAL_PROVIDERS {
                    if config.is_local_provider_configured(p) {
                        configured_providers.push(p.to_string());
                    }
                }
                for k in config.provider.custom_endpoints.keys() {
                    if !configured_providers.contains(k) {
                        configured_providers.push(k.clone());
                    }
                }
                configured_providers.sort();

                let mut masked_keys = HashMap::new();
                for (p, key) in &config.provider.api_keys {
                    let trimmed = key.trim();
                    if !trimmed.is_empty() {
                        masked_keys.insert(p.clone(), mask_api_key(trimmed));
                    }
                }
                for &p in &CLOUD_PROVIDERS {
                    if let Ok(key) = config.get_api_key(p) {
                        let trimmed = key.trim();
                        if !trimmed.is_empty() {
                            masked_keys.insert(p.to_string(), mask_api_key(trimmed));
                        }
                    }
                }

                let response = json!({
                    "active_provider": config.provider.default,
                    "active_model": config.provider.model,
                    "workspace_scope": workspace_root.display().to_string(),
                    "auto_approve": config.agent.auto_approve,
                    "approval_policy": config.agent.approval_policy,
                    "thinking_budget": config.provider.thinking_budget,
                    "theme": config.ui.theme,
                    "configured_providers": configured_providers,
                    "masked_keys": masked_keys,
                });

                serde_json::to_string_pretty(&response)
                    .map_err(|e| ToolError::CommandExec(format!("Failed to format JSON: {}", e)).into())
            }
            .await,
        ),
        "update_agent_config" => Some(
            async {
                let provider = param::opt_str(args, "provider").map(|s| s.trim().to_string());
                let model = param::opt_str(args, "model").map(|s| s.trim().to_string());
                let auto_approve = param::get_bool(args, "auto_approve");
                let thinking_budget = param::get_usize(args, "thinking_budget");
                let theme = param::opt_str(args, "theme").map(|s| s.trim().to_string());
                let scope = param::opt_str(args, "scope")
                    .unwrap_or("workspace")
                    .trim()
                    .to_lowercase();

                if scope != "workspace" && scope != "global" {
                    return Err(ToolError::InvalidArguments {
                        name: "update_agent_config".to_string(),
                        reason: format!("Invalid scope '{}'. Must be 'workspace' or 'global'", scope),
                    }
                    .into());
                }

                if provider.is_none()
                    && model.is_none()
                    && auto_approve.is_none()
                    && thinking_budget.is_none()
                    && theme.is_none()
                {
                    return Err(ToolError::InvalidArguments {
                        name: "update_agent_config".to_string(),
                        reason: "At least one configuration field (provider, model, auto_approve, thinking_budget, theme) must be specified".to_string(),
                    }
                    .into());
                }

                let proposal = ConfigChangeProposal {
                    scope: scope.clone(),
                    provider,
                    model,
                    auto_approve,
                    thinking_budget,
                    theme,
                };

                let response = json!({
                    "status": "proposal_generated",
                    "requires_confirmation": true,
                    "scope": scope,
                    "proposal": proposal,
                    "message": "Configuration change proposal generated. User confirmation required to persist changes.",
                });

                serde_json::to_string_pretty(&response)
                    .map_err(|e| ToolError::CommandExec(format!("Failed to format JSON: {}", e)).into())
            }
            .await,
        ),
        "test_provider_connection" => Some(
            async {
                let provider = param::require_str(args, "provider", "test_provider_connection")?;
                let config = Config::load(Some(workspace_root), None).unwrap_or_default();

                if provider.eq_ignore_ascii_case("all") {
                    let reports = test_all_provider_connections(&config).await;
                    serde_json::to_string_pretty(&reports).map_err(|e| {
                        ToolError::CommandExec(format!("Failed to format JSON: {}", e)).into()
                    })
                } else {
                    let report = test_single_provider(provider, &config).await;
                    serde_json::to_string_pretty(&report).map_err(|e| {
                        ToolError::CommandExec(format!("Failed to format JSON: {}", e)).into()
                    })
                }
            }
            .await,
        ),
        "list_available_models" => Some(
            async {
                let provider = param::require_str(args, "provider", "list_available_models")?;
                let config = Config::load(Some(workspace_root), None).unwrap_or_default();
                let key = config.get_api_key(provider).unwrap_or_default();
                let custom_url = config
                    .provider
                    .custom_endpoints
                    .get(&provider.to_lowercase())
                    .or_else(|| config.provider.custom_endpoints.get(provider))
                    .map(|s| s.as_str());

                let fetcher = ModelFetcher::new();
                let live_res = fetcher.fetch_models(provider, &key, custom_url).await;

                let models: Vec<AvailableModelItem> = match live_res {
                    Ok(items) if !items.is_empty() => items
                        .into_iter()
                        .map(|m| {
                            let context_length = m.context_length.unwrap_or_else(|| {
                                get_model_context_limit(&m.id)
                            });
                            let supports_reasoning = model_supports_reasoning(&m.id, &m.name);
                            AvailableModelItem {
                                id: m.id,
                                name: m.name,
                                context_length,
                                supports_reasoning,
                            }
                        })
                        .collect(),
                    _ => fallback_models_for_provider(provider),
                };

                serde_json::to_string_pretty(&models)
                    .map_err(|e| ToolError::CommandExec(format!("Failed to format JSON: {}", e)).into())
            }
            .await,
        ),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_config_tools_schemas() {
        let schemas = get_schemas();
        assert_eq!(schemas.len(), 4);
        let names: Vec<&str> = schemas.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"get_agent_config"));
        assert!(names.contains(&"update_agent_config"));
        assert!(names.contains(&"test_provider_connection"));
        assert!(names.contains(&"list_available_models"));

        for s in &schemas {
            if s.name == "test_provider_connection" || s.name == "list_available_models" {
                let req = s.parameters.get("required").and_then(|r| r.as_array());
                assert!(
                    req.is_some(),
                    "Tool {} must have required parameters",
                    s.name
                );
                let req_arr = req.unwrap();
                assert!(req_arr.iter().any(|v| v == "provider"));
            }
        }
    }

    #[tokio::test]
    async fn test_get_agent_config_masked_keys() {
        let temp = TempDir::new().unwrap();
        let raw_key = "sk-ant-test-super-secret-1234567890abcdef";
        std::env::set_var("ANTHROPIC_API_KEY", raw_key);

        let res = dispatch("get_agent_config", &json!({}), temp.path())
            .await
            .expect("Tool must be handled")
            .expect("Execution must succeed");

        // Zero plaintext credential leakage
        assert!(
            !res.contains(raw_key),
            "Raw secret key leaked in get_agent_config output!"
        );
        let masked = mask_api_key(raw_key);
        assert!(
            res.contains(&masked),
            "Masked key '{}' not found in: {}",
            masked,
            res
        );

        std::env::remove_var("ANTHROPIC_API_KEY");
    }

    #[tokio::test]
    async fn test_update_agent_config_proposal() {
        let temp = TempDir::new().unwrap();

        // 1. Updating with empty arguments must fail validation
        let err_res = dispatch("update_agent_config", &json!({}), temp.path())
            .await
            .expect("Tool must be handled");
        assert!(err_res.is_err(), "Empty update must fail validation");

        // 2. Updating with fields generates structured proposal
        let valid_args = json!({
            "provider": "anthropic",
            "model": "claude-3-7-sonnet-20250219",
            "auto_approve": true,
            "thinking_budget": 8192,
            "theme": "nord",
            "scope": "workspace"
        });
        let res = dispatch("update_agent_config", &valid_args, temp.path())
            .await
            .expect("Tool must be handled")
            .expect("Execution must succeed");

        assert!(res.contains("proposal_generated"));
        assert!(res.contains("\"requires_confirmation\": true"));
        assert!(res.contains("claude-3-7-sonnet-20250219"));

        // 3. Test apply_proposal
        let parsed_val: serde_json::Value = serde_json::from_str(&res).unwrap();
        let proposal: ConfigChangeProposal =
            serde_json::from_value(parsed_val["proposal"].clone()).unwrap();
        apply_proposal(&proposal, temp.path()).unwrap();

        let cfg_path = temp
            .path()
            .join(crate::constants::WORKSPACE_DIR_NAME)
            .join(crate::constants::CONFIG_FILE_NAME);
        assert!(cfg_path.exists(), "Workspace config.toml must be created");
        let content = std::fs::read_to_string(cfg_path).unwrap();
        assert!(content.contains("claude-3-7-sonnet-20250219"));
    }

    #[tokio::test]
    async fn test_test_provider_connection_zero_leak() {
        let temp = TempDir::new().unwrap();
        let raw_key = "sk-fake-openai-secret-key-1234567890";
        std::env::set_var("OPENAI_API_KEY", raw_key);

        let res = dispatch(
            "test_provider_connection",
            &json!({ "provider": "openai" }),
            temp.path(),
        )
        .await
        .expect("Tool must be handled")
        .expect("Execution must return report");

        assert!(res.contains("\"provider\": \"openai\""));
        assert!(res.contains("\"key_masked\""));
        // Zero-leak check: raw key must NEVER appear in output
        assert!(
            !res.contains(raw_key),
            "Plaintext key leaked in test_provider_connection!"
        );

        std::env::remove_var("OPENAI_API_KEY");
    }

    #[tokio::test]
    async fn test_list_available_models_fallback() {
        let temp = TempDir::new().unwrap();
        let res = dispatch(
            "list_available_models",
            &json!({ "provider": "anthropic" }),
            temp.path(),
        )
        .await
        .expect("Tool must be handled")
        .expect("Execution must succeed");

        assert!(res.contains("claude-3-7-sonnet"));
        assert!(res.contains("context_length"));
        assert!(res.contains("supports_reasoning"));
    }
}

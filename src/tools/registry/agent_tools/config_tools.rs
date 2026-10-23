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
            description: "Request an update to the agent configuration (active provider, model, 4-slot fallback chain [fallback_1, fallback_2, global_last_fallback], per-MiniPub model overrides, auto_approve, thinking budget, theme, timeouts, or minipub subagent settings).".to_string(),
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
                    "fallback_1": {
                        "type": "string",
                        "description": "Optional [2] 1st Fallback in 'provider:model' or 'provider/model' format (can be any provider and model)"
                    },
                    "fallback_2": {
                        "type": "string",
                        "description": "Optional [3] 2nd Fallback in 'provider:model' or 'provider/model' format (can be any provider and model)"
                    },
                    "global_last_fallback": {
                        "type": "string",
                        "description": "Optional [4] Global Last Fallback ★ in 'provider:model' or 'provider/model' format (shared across Orchestrator and all 14 MiniPubs)"
                    },
                    "target_role": {
                        "type": "string",
                        "description": "Optional target scope for model/fallback assignment: 'global' (default: Orchestrator + all default MiniPubs) or a specific MiniPub role slug (e.g. 'coder', 'vision', 'frontend', 'reviewer')"
                    },
                    "sync_all_minipubs": {
                        "type": "boolean",
                        "description": "If true, syncs all 14 MiniPubs to inherit the Global (Orchestrator) 4-slot model & fallback chain"
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
                    "timeout": {
                        "type": "integer",
                        "description": "Base command execution timeout in seconds (e.g. 30, 60, 120)"
                    },
                    "connect_timeout_secs": {
                        "type": "integer",
                        "description": "Provider TCP/TLS connect timeout in seconds (default: 30)"
                    },
                    "stream_idle_timeout_secs": {
                        "type": "integer",
                        "description": "Provider inter-token stream idle timeout in seconds (default: 300)"
                    },
                    "max_parallel_minipubs": {
                        "type": "integer",
                        "description": "Maximum parallel minipub subagents spawned concurrently by the Orchestrator (1-16, default: 4)"
                    },
                    "minipub_timeout_secs": {
                        "type": "integer",
                        "description": "Activity-heartbeat inactivity timeout in seconds per minipub worker (default: 90)"
                    },
                    "minipub_token_budget": {
                        "type": "integer",
                        "description": "Default token budget per minipub worker (default: 50000)"
                    },
                    "minipub_max_turns": {
                        "type": "integer",
                        "description": "Default maximum tool-calling turns per minipub worker (default: 15)"
                    },
                    "minipub_worktree_isolation": {
                        "type": "string",
                        "enum": ["auto", "always", "file_leases", "never"],
                        "description": "Workspace isolation policy for mutating minipub workers ('auto', 'always', 'file_leases', 'never')"
                    },
                    "minipub_extra_tools": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Additional tool names granted to minipub workers beyond their role whitelist"
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
const CLOUD_PROVIDERS: [&str; 19] = [
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
    "huggingface",
    "nvidia",
    "opencode",
    "cerebras",
    "agnes",
    "nara",
    "ofox",
    "aihubmix",
    "kimi",
];

/// Known local providers tested during connection scans.
const LOCAL_PROVIDERS: [&str; 2] = ["ollama", "lmstudio"];

/// Persist an approved `ConfigChangeProposal` to disk.
#[allow(dead_code)]
pub fn apply_proposal(proposal: &ConfigChangeProposal, workspace_root: &Path) -> Result<()> {
    let ws_opt = if proposal.scope == "global" {
        None
    } else {
        let minicode_dir = workspace_root.join(crate::constants::WORKSPACE_DIR_NAME);
        std::fs::create_dir_all(&minicode_dir).map_err(|e| ToolError::FileOp {
            path: minicode_dir.display().to_string(),
            source: e,
        })?;
        Some(workspace_root)
    };

    let mut config = Config::load(ws_opt, None).unwrap_or_default();
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
    config.save(ws_opt)?;

    if ws_opt.is_some() && (proposal.provider.is_some() || proposal.model.is_some()) {
        let _ = Config::save_workspace_preference(
            workspace_root,
            &config.provider.default,
            &config.provider.model,
        );
    }
    Ok(())
}

/// Identifies the configuration source of a provider's credential without revealing it.
fn determine_key_source(provider: &str, config: &Config) -> String {
    const PROVIDER_ENV_VARS: &[(&[&str], &[&str])] = &[
        (&["gemini", "google"], &["GEMINI_API_KEY", "GOOGLE_API_KEY"]),
        (&["anthropic", "claude"], &["ANTHROPIC_API_KEY"]),
        (&["openrouter"], &["OPENROUTER_API_KEY", "OPENROUTER_KEY"]),
        (&["openai"], &["OPENAI_API_KEY"]),
        (&["deepseek"], &["DEEPSEEK_API_KEY"]),
        (&["groq"], &["GROQ_API_KEY"]),
        (&["together"], &["TOGETHER_API_KEY"]),
        (&["minimax"], &["MINIMAX_API_KEY"]),
        (
            &["z.ai", "z_ai", "zhipu", "glm", "bigmodel"],
            &[
                "ZHIPU_API_KEY",
                "Z_AI_API_KEY",
                "GLM_API_KEY",
                "BIGMODEL_API_KEY",
            ],
        ),
        (&["mistral"], &["MISTRAL_API_KEY"]),
        (
            &["huggingface", "hf"],
            &["HUGGINGFACE_API_KEY", "HF_TOKEN", "HUGGINGFACE_TOKEN"],
        ),
        (
            &["nvidia", "nim", "nvidia-nim"],
            &["NVIDIA_API_KEY", "NGC_API_KEY"],
        ),
        (
            &["opencode", "opencode-zen", "zen"],
            &["OPENCODE_API_KEY", "OPENCODE_ZEN_API_KEY"],
        ),
        (&["cerebras"], &["CEREBRAS_API_KEY"]),
        (&["agnes", "agnes-ai"], &["AGNES_API_KEY"]),
        (
            &["nara", "nararouter", "bynara"],
            &["NARA_API_KEY", "NARAROUTER_API_KEY"],
        ),
        (&["ofox", "ofoxai"], &["OFOX_API_KEY", "OFOXAI_API_KEY"]),
        (&["aihubmix"], &["AIHUBMIX_API_KEY"]),
        (
            &["kimi", "moonshot", "moonshotai"],
            &["KIMI_API_KEY", "MOONSHOT_API_KEY"],
        ),
    ];

    let norm = provider.to_ascii_lowercase();
    let env_var = PROVIDER_ENV_VARS
        .iter()
        .find(|(aliases, _)| aliases.contains(&norm.as_str()))
        .and_then(|(_, vars)| vars.iter().copied().find(|v| std::env::var(v).is_ok()));

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

/// Fallback model resolver for when remote provider model endpoints and `models.dev` catalog are unreachable (offline).
/// Checks the local disk cache (`models_cache.json`) first before falling back to minimal offline seeds.
pub fn fallback_models_for_provider(provider: &str) -> Vec<AvailableModelItem> {
    let norm = provider.to_lowercase();
    if let Some(cached) = ModelFetcher::new().get_cached_models(&norm) {
        if !cached.is_empty() {
            return cached
                .into_iter()
                .map(|m| {
                    let context_length = m
                        .context_length
                        .unwrap_or_else(|| get_model_context_limit(&m.id));
                    let supports_reasoning = model_supports_reasoning(&m.id, &m.name);
                    AvailableModelItem {
                        id: m.id,
                        name: m.name,
                        context_length,
                        supports_reasoning,
                    }
                })
                .collect();
        }
    }
    type FallbackSeed = (&'static str, &'static str, usize, bool);
    const FALLBACK_SEEDS: &[(&[&str], &[FallbackSeed])] = &[
        (
            &["anthropic", "claude"],
            &[
                (
                    "claude-3-7-sonnet-20250219",
                    "Claude 3.7 Sonnet (Hybrid Reasoning)",
                    200_000,
                    true,
                ),
                (
                    "claude-3-5-sonnet-20241022",
                    "Claude 3.5 Sonnet v2",
                    200_000,
                    false,
                ),
                (
                    "claude-3-5-haiku-20241022",
                    "Claude 3.5 Haiku",
                    200_000,
                    false,
                ),
            ],
        ),
        (
            &["openai"],
            &[
                ("o3-mini", "o3-mini (Reasoning)", 200_000, true),
                ("o1", "o1 (Reasoning)", 200_000, true),
                ("gpt-4o", "GPT-4o", 128_000, false),
                ("gpt-4o-mini", "GPT-4o Mini", 128_000, false),
            ],
        ),
        (
            &["gemini", "google"],
            &[
                (
                    "gemini-2.5-pro",
                    "Gemini 2.5 Pro (Thinking)",
                    2_000_000,
                    true,
                ),
                ("gemini-2.0-flash", "Gemini 2.0 Flash", 1_000_000, false),
                ("gemini-1.5-pro", "Gemini 1.5 Pro", 2_000_000, false),
            ],
        ),
        (
            &["deepseek"],
            &[
                (
                    "deepseek-reasoner",
                    "DeepSeek R1 (Reasoning)",
                    128_000,
                    true,
                ),
                ("deepseek-chat", "DeepSeek V3", 128_000, false),
            ],
        ),
        (
            &["openrouter"],
            &[
                (
                    "anthropic/claude-3.7-sonnet",
                    "Claude 3.7 Sonnet",
                    200_000,
                    true,
                ),
                ("openai/gpt-4o", "GPT-4o", 128_000, false),
                ("deepseek/deepseek-r1", "DeepSeek R1", 128_000, true),
            ],
        ),
        (
            &["groq"],
            &[
                (
                    "llama-3.3-70b-versatile",
                    "Llama 3.3 70B Versatile",
                    128_000,
                    false,
                ),
                (
                    "deepseek-r1-distill-llama-70b",
                    "DeepSeek R1 Distill Llama 70B",
                    128_000,
                    true,
                ),
            ],
        ),
        (
            &["mistral"],
            &[
                ("codestral-latest", "Codestral Latest", 256_000, false),
                (
                    "mistral-large-latest",
                    "Mistral Large Latest",
                    128_000,
                    false,
                ),
            ],
        ),
        (
            &["together"],
            &[(
                "meta-llama/Llama-3.3-70B-Instruct-Turbo",
                "Llama 3.3 70B Turbo",
                128_000,
                false,
            )],
        ),
        (
            &["minimax"],
            &[("MiniMax-Text-01", "MiniMax Text 01", 1_000_000, false)],
        ),
        (
            &["z.ai", "z_ai", "zhipu", "glm", "bigmodel"],
            &[("glm-4-plus", "GLM-4 Plus", 128_000, false)],
        ),
        (
            &["ollama"],
            &[
                ("qwen2.5-coder", "Qwen 2.5 Coder", 32_768, false),
                ("deepseek-r1", "DeepSeek R1", 32_768, true),
            ],
        ),
        (
            &["lmstudio", "lm-studio"],
            &[("local-model", "LM Studio Local Model", 32_768, false)],
        ),
    ];

    if let Some((_, items)) = FALLBACK_SEEDS
        .iter()
        .find(|(aliases, _)| aliases.contains(&norm.as_str()))
    {
        return items
            .iter()
            .map(
                |(id, name, context_length, supports_reasoning)| AvailableModelItem {
                    id: (*id).to_string(),
                    name: (*name).to_string(),
                    context_length: *context_length,
                    supports_reasoning: *supports_reasoning,
                },
            )
            .collect();
    }

    let default_model = Config::static_default_model_for_provider(provider);
    let ctx = get_model_context_limit(default_model);
    vec![AvailableModelItem {
        id: default_model.to_string(),
        name: format!("{} Default Model", provider),
        context_length: ctx,
        supports_reasoning: model_supports_reasoning(default_model, ""),
    }]
}

/// Tests connectivity for an individual provider endpoint while enforcing zero plaintext key leakage.
pub async fn test_single_provider(provider: &str, config: &Config) -> ProviderConnectionReport {
    let fetcher = ModelFetcher::new();
    test_single_provider_with_fetcher(provider, config, &fetcher).await
}

async fn test_single_provider_with_fetcher(
    provider: &str,
    config: &Config,
    fetcher: &ModelFetcher,
) -> ProviderConnectionReport {
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
    let res = fetcher
        .fetch_live_models_strict(&norm, &key, custom_url)
        .await;
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
    let fetcher = ModelFetcher::new();
    let futures = CLOUD_PROVIDERS
        .iter()
        .chain(LOCAL_PROVIDERS.iter())
        .map(|&p| test_single_provider_with_fetcher(p, config, &fetcher));
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
                    "fallback_1": config.provider.fallback_1.as_ref().map(|s| s.display_short()),
                    "fallback_2": config.provider.fallback_2.as_ref().map(|s| s.display_short()),
                    "global_last_fallback": config.provider.global_last_fallback.as_ref().map(|s| s.display_short()),
                    "orchestrator_4_slot_chain": config
                        .resolve_orchestrator_chain(Some(workspace_root))
                        .iter()
                        .map(|s| s.display_short())
                        .collect::<Vec<_>>(),
                    "workspace_scope": workspace_root.display().to_string(),
                    "auto_approve": config.agent.auto_approve,
                    "approval_policy": config.agent.approval_policy,
                    "thinking_budget": config.provider.thinking_budget,
                    "theme": config.ui.theme,
                    "timeouts": {
                        "exec_timeout_secs": config.agent.timeout,
                        "connect_timeout_secs": config.provider.effective_connect_timeout_secs(),
                        "stream_idle_timeout_secs": config.provider.effective_stream_idle_timeout_secs(),
                        "adaptive_timeout": config.provider.adaptive_timeout,
                    },
                    "minipub": {
                        "max_parallel": config.agent.minipub.max_parallel,
                        "default_token_budget": config.agent.minipub.default_token_budget,
                        "default_max_turns": config.agent.minipub.default_max_turns,
                        "inactivity_timeout_secs": config.agent.minipub.inactivity_timeout_secs,
                        "worktree_isolation": config.agent.minipub.worktree_isolation,
                        "extra_allowed_tools": config.agent.minipub.extra_allowed_tools,
                        "sync_with_orchestrator": config.agent.minipub.sync_with_orchestrator,
                        "role_models": config.agent.minipub.role_models,
                    },
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
                let fallback_1 = param::opt_str(args, "fallback_1").map(|s| s.trim().to_string());
                let fallback_2 = param::opt_str(args, "fallback_2").map(|s| s.trim().to_string());
                let global_last_fallback =
                    param::opt_str(args, "global_last_fallback").map(|s| s.trim().to_string());
                let target_role = param::opt_str(args, "target_role")
                    .map(|s| s.trim().to_lowercase())
                    .filter(|s| !s.is_empty());
                let sync_all_minipubs = param::get_bool(args, "sync_all_minipubs");
                let auto_approve = param::get_bool(args, "auto_approve");
                let thinking_budget = param::get_usize(args, "thinking_budget");
                let theme = param::opt_str(args, "theme").map(|s| s.trim().to_string());
                let timeout = param::get_u64(args, "timeout");
                let connect_timeout_secs = param::get_u64(args, "connect_timeout_secs");
                let stream_idle_timeout_secs = param::get_u64(args, "stream_idle_timeout_secs");
                let max_parallel_minipubs = param::get_usize(args, "max_parallel_minipubs");
                let minipub_timeout_secs = param::get_u64(args, "minipub_timeout_secs");
                let minipub_token_budget = param::get_usize(args, "minipub_token_budget");
                let minipub_max_turns = param::get_usize(args, "minipub_max_turns");
                let minipub_worktree_isolation = param::opt_str(args, "minipub_worktree_isolation")
                    .map(|s| s.trim().to_string());
                let minipub_extra_tools: Option<Vec<String>> = args
                    .get("minipub_extra_tools")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|x| x.as_str().map(|s| s.trim().to_string()))
                            .filter(|s| !s.is_empty())
                            .collect()
                    });
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

                let is_role_targeted = target_role
                    .as_deref()
                    .is_some_and(|r| r != "global" && r != "orchestrator" && r != "all");

                let has_runtime_tuning = timeout.is_some()
                    || connect_timeout_secs.is_some()
                    || stream_idle_timeout_secs.is_some()
                    || max_parallel_minipubs.is_some()
                    || minipub_timeout_secs.is_some()
                    || minipub_token_budget.is_some()
                    || minipub_max_turns.is_some()
                    || minipub_worktree_isolation.is_some()
                    || minipub_extra_tools.is_some()
                    || fallback_1.is_some()
                    || fallback_2.is_some()
                    || global_last_fallback.is_some()
                    || sync_all_minipubs.is_some()
                    || is_role_targeted;

                let has_proposal_fields = (!is_role_targeted && (provider.is_some() || model.is_some()))
                    || auto_approve.is_some()
                    || thinking_budget.is_some()
                    || theme.is_some();

                if !has_proposal_fields && !has_runtime_tuning {
                    return Err(ToolError::InvalidArguments {
                        name: "update_agent_config".to_string(),
                        reason: "At least one configuration field (provider, model, fallback_1, fallback_2, global_last_fallback, target_role, sync_all_minipubs, auto_approve, thinking_budget, theme, timeout, or minipub settings) must be specified".to_string(),
                    }
                    .into());
                }

                if has_runtime_tuning {
                    let target_ws = if scope == "global" {
                        None
                    } else {
                        let minicode_dir = workspace_root.join(crate::constants::WORKSPACE_DIR_NAME);
                        let _ = std::fs::create_dir_all(&minicode_dir);
                        Some(workspace_root)
                    };
                    let mut cfg = Config::load(target_ws, None).unwrap_or_default();
                    let target_slug = target_role.as_deref().unwrap_or("global");
                    let def_prov = provider
                        .as_deref()
                        .unwrap_or(&cfg.provider.default)
                        .to_string();

                    if is_role_targeted && (provider.is_some() || model.is_some()) {
                        let m = model
                            .clone()
                            .unwrap_or_else(|| cfg.get_default_model_for_provider(&def_prov));
                        cfg.set_slot_for_target(
                            target_slug,
                            1,
                            Some(crate::config::ModelSlot::new(&def_prov, m)),
                        );
                    }
                    for (fb_opt, slot_target, slot_num) in [
                        (fallback_1.as_deref(), target_slug, 2usize),
                        (fallback_2.as_deref(), target_slug, 3usize),
                        (global_last_fallback.as_deref(), "global", 4usize),
                    ] {
                        if let Some(fb) = fb_opt {
                            let slot = if fb.is_empty() || fb.eq_ignore_ascii_case("none") {
                                None
                            } else {
                                crate::config::ModelSlot::parse_loose(fb, &def_prov)
                            };
                            cfg.set_slot_for_target(slot_target, slot_num, slot);
                        }
                    }
                    if sync_all_minipubs == Some(true) {
                        cfg.sync_all_minipubs_to_orchestrator();
                    }
                    if let Some(t) = timeout {
                        cfg.agent.timeout = t.max(5);
                    }
                    if let Some(ct) = connect_timeout_secs {
                        cfg.provider.connect_timeout_secs = Some(ct.max(5));
                    }
                    if let Some(st) = stream_idle_timeout_secs {
                        cfg.provider.stream_idle_timeout_secs = Some(st.max(15));
                    }
                    if let Some(mp) = max_parallel_minipubs {
                        cfg.agent.minipub.max_parallel = mp.clamp(
                            crate::constants::MIN_PARALLEL_MINIPUBS,
                            crate::constants::MAX_PARALLEL_MINIPUBS_CAP,
                        );
                    }
                    if let Some(mto) = minipub_timeout_secs {
                        cfg.agent.minipub.inactivity_timeout_secs = mto.max(10);
                    }
                    if let Some(mbud) = minipub_token_budget {
                        cfg.agent.minipub.default_token_budget = mbud.max(4_000);
                    }
                    if let Some(mturns) = minipub_max_turns {
                        cfg.agent.minipub.default_max_turns = mturns.clamp(1, 100);
                    }
                    if let Some(ref miso) = minipub_worktree_isolation {
                        cfg.agent.minipub.worktree_isolation = miso.clone();
                    }
                    if let Some(ref mtools) = minipub_extra_tools {
                        cfg.agent.minipub.extra_allowed_tools = mtools.clone();
                    }
                    cfg.sync_runtime_timeouts();
                    cfg.save(target_ws)?;
                }

                let proposal = ConfigChangeProposal {
                    scope: scope.clone(),
                    provider: if is_role_targeted { None } else { provider },
                    model: if is_role_targeted { None } else { model },
                    auto_approve,
                    thinking_budget,
                    theme,
                };

                let response = json!({
                    "status": if has_proposal_fields { "proposal_generated" } else { "applied" },
                    "requires_confirmation": has_proposal_fields,
                    "scope": scope,
                    "proposal": proposal,
                    "runtime_tuning_applied": has_runtime_tuning,
                    "message": if has_proposal_fields {
                        "Configuration change proposal generated. User confirmation required to persist provider/model/policy changes."
                    } else {
                        "4-slot model/fallback routing, MiniPub, and timeout configuration updated and synchronized."
                    },
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

        assert!(res.contains("claude"));
        assert!(res.contains("context_length"));
        assert!(res.contains("supports_reasoning"));
    }

    #[test]
    fn test_all_supported_providers_create_provider_cleanly() {
        for &prov in crate::constants::SUPPORTED_PROVIDERS {
            let created = crate::agent::providers::factory::create_provider_with_base_url(
                prov,
                "test-api-key",
                None,
            );
            assert!(
                created.is_ok(),
                "Provider '{}' must be supported by factory",
                prov
            );
        }
    }
}

use serde::{Deserialize, Serialize};

#[path = "config_impl/load_and_merge.rs"]
mod load_and_merge;
#[path = "config_impl/provider_resolution.rs"]
mod provider_resolution;
#[cfg(test)]
#[path = "config_impl/tests.rs"]
mod tests;

pub use load_and_merge::{
    RawAgentConfig, RawConfig, RawGitConfig, RawLoggingConfig, RawMiniPubConfig, RawProviderConfig,
    RawUiConfig,
};
pub use provider_resolution::{
    load_workspace_preference_from_file, mask_api_key, save_workspace_preference_to_file,
    WorkspacePreference, WorkspaceRegistry,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Config {
    #[serde(default)]
    pub provider: ProviderConfig,

    #[serde(default)]
    pub agent: AgentConfig,

    #[serde(default)]
    pub ui: UiConfig,

    #[serde(default)]
    pub logging: LoggingConfig,

    #[serde(default)]
    pub mcp: McpConfig,

    #[serde(default)]
    pub git: GitConfig,

    #[serde(default)]
    pub browser: BrowserSettingsConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProviderConfig {
    #[serde(default)]
    pub default: String,

    #[serde(default)]
    pub model: String,

    #[serde(default)]
    pub ollama: OllamaConfig,

    #[serde(default = "default_temperature")]
    pub temperature: f32,

    #[serde(default = "default_max_tokens")]
    pub max_tokens: usize,

    #[serde(default)]
    pub api_keys: std::collections::HashMap<String, String>,

    #[serde(default)]
    pub custom_endpoints: std::collections::HashMap<String, String>,

    #[serde(default)]
    pub thinking_budget: Option<usize>,

    #[serde(default)]
    pub reasoning_effort: Option<String>,

    /// Explicit user override for context window size in tokens
    #[serde(default)]
    pub context_window: Option<usize>,

    /// Custom prompt rate per 1,000,000 tokens in USD
    #[serde(default)]
    pub prompt_cost_per_m: Option<f64>,

    /// Custom completion rate per 1,000,000 tokens in USD
    #[serde(default)]
    pub completion_cost_per_m: Option<f64>,

    #[serde(default)]
    pub default_models: std::collections::HashMap<String, String>,

    /// Slot 2 (1st Fallback): `(provider, model)` used when Primary fails or hits rate limits
    #[serde(default)]
    pub fallback_1: Option<ModelSlot>,

    /// Slot 3 (2nd Fallback): `(provider, model)` used when 1st Fallback fails or hits rate limits
    #[serde(default)]
    pub fallback_2: Option<ModelSlot>,

    /// Slot 4 (Global Last Fallback ★): shared `(provider, model)` used as the final fallback across the Orchestrator and ALL MiniPubs
    #[serde(default)]
    pub global_last_fallback: Option<ModelSlot>,

    /// User-saved favorite `(provider, model)` pairs for quick cross-provider switching in `/model` (`/switch`) and `/settings`
    #[serde(default)]
    pub favorite_models: Vec<ModelSlot>,

    /// Explicit TCP/TLS connect timeout in seconds (overrides default 30s)
    #[serde(default)]
    pub connect_timeout_secs: Option<u64>,

    /// Explicit inter-token stream idle timeout in seconds (overrides default 300s)
    #[serde(default)]
    pub stream_idle_timeout_secs: Option<u64>,

    /// Enable adaptive TTFT and stream idle timeout scaling based on prompt size, thinking budget, and local vs cloud provider
    #[serde(default = "default_true")]
    pub adaptive_timeout: bool,
}

/// A `(provider, model)` routing slot used for Primary, 1st Fallback, 2nd Fallback, and Global Last Fallback ★.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ModelSlot {
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub model: String,
}

impl ModelSlot {
    #[must_use]
    pub fn new(provider: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            provider: provider.into().trim().to_lowercase(),
            model: model.into().trim().to_string(),
        }
    }

    #[must_use]
    pub fn is_valid(&self) -> bool {
        !self.provider.trim().is_empty() && !self.model.trim().is_empty()
    }

    #[must_use]
    pub fn display_short(&self) -> String {
        if !self.is_valid() {
            "—".to_string()
        } else {
            format!("{}:{}", self.provider.trim(), self.model.trim())
        }
    }

    /// Parses `"provider:model"` or `"provider/model"` (or bare `"model"` using `default_provider`).
    #[must_use]
    pub fn parse_loose(spec: &str, default_provider: &str) -> Option<Self> {
        let trimmed = spec.trim();
        if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("none") || trimmed == "-" {
            return None;
        }
        if let Some((prov, mdl)) = trimmed.split_once(':') {
            let slot = Self::new(prov, mdl);
            return slot.is_valid().then_some(slot);
        }
        if let Some((prefix, rest)) = trimmed.split_once('/') {
            let prefix_lower = prefix.trim().to_lowercase();
            let def_lower = default_provider.trim().to_lowercase();
            if (crate::constants::SUPPORTED_PROVIDERS.contains(&prefix_lower.as_str())
                || prefix_lower == "google")
                && (def_lower != "openrouter" || prefix_lower == "openrouter" || rest.contains('/'))
            {
                let slot = Self::new(prefix_lower, rest);
                return slot.is_valid().then_some(slot);
            }
        }
        let fallback_prov = if default_provider.trim().is_empty() {
            "openrouter"
        } else {
            default_provider.trim()
        };
        let slot = Self::new(fallback_prov, trimmed);
        slot.is_valid().then_some(slot)
    }
}

/// 3-Slot override chain (`Primary -> 1st Fallback -> 2nd Fallback`) for a MiniPub role or the MiniPub default chain.
/// Note: The 4th slot (`global_last_fallback`) is always shared globally from `config.provider.global_last_fallback`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct MiniPubModelChain {
    #[serde(default)]
    pub primary: Option<ModelSlot>,
    #[serde(default)]
    pub fallback_1: Option<ModelSlot>,
    #[serde(default)]
    pub fallback_2: Option<ModelSlot>,
}

impl MiniPubModelChain {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.primary.as_ref().is_none_or(|s| !s.is_valid())
            && self.fallback_1.as_ref().is_none_or(|s| !s.is_valid())
            && self.fallback_2.as_ref().is_none_or(|s| !s.is_valid())
    }
}

static RUNTIME_CONNECT_TIMEOUT_SECS: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(crate::constants::PROVIDER_CONNECT_TIMEOUT_SECS);
static RUNTIME_STREAM_IDLE_TIMEOUT_SECS: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(crate::constants::PROVIDER_STREAM_IDLE_TIMEOUT_SECS);
pub(super) static RUNTIME_TIMEOUTS_OVERRIDDEN: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Returns the active provider TCP/TLS connect timeout in seconds (configurable via config.toml / env / runtime).
pub fn get_active_connect_timeout_secs() -> u64 {
    RUNTIME_CONNECT_TIMEOUT_SECS
        .load(std::sync::atomic::Ordering::Relaxed)
        .max(5)
}

/// Returns the active provider stream idle timeout in seconds (configurable via config.toml / env / runtime).
pub fn get_active_stream_idle_timeout_secs() -> u64 {
    RUNTIME_STREAM_IDLE_TIMEOUT_SECS
        .load(std::sync::atomic::Ordering::Relaxed)
        .max(15)
}

/// Updates the active provider timeouts at runtime.
pub fn set_active_provider_timeouts(connect_secs: u64, stream_idle_secs: u64) {
    RUNTIME_CONNECT_TIMEOUT_SECS.store(connect_secs.max(5), std::sync::atomic::Ordering::Relaxed);
    RUNTIME_STREAM_IDLE_TIMEOUT_SECS.store(
        stream_idle_secs.max(15),
        std::sync::atomic::Ordering::Relaxed,
    );
}

/// Explicitly overrides the active provider timeouts at runtime (e.g. by the Orchestrator or Settings modal)
/// so subsequent `Config::load_for_workspace` calls preserve the live dynamic override.
pub fn set_active_provider_timeouts_override(connect_secs: u64, stream_idle_secs: u64) {
    RUNTIME_TIMEOUTS_OVERRIDDEN.store(true, std::sync::atomic::Ordering::Relaxed);
    set_active_provider_timeouts(connect_secs, stream_idle_secs);
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            default: String::new(),
            model: String::new(),
            default_models: std::collections::HashMap::new(),
            fallback_1: None,
            fallback_2: None,
            global_last_fallback: None,
            favorite_models: Vec::new(),
            ollama: OllamaConfig::default(),
            temperature: default_temperature(),
            max_tokens: default_max_tokens(),
            api_keys: std::collections::HashMap::new(),
            custom_endpoints: std::collections::HashMap::new(),
            thinking_budget: None,
            reasoning_effort: None,
            context_window: None,
            prompt_cost_per_m: None,
            completion_cost_per_m: None,
            connect_timeout_secs: None,
            stream_idle_timeout_secs: None,
            adaptive_timeout: true,
        }
    }
}

#[allow(dead_code)]
impl ProviderConfig {
    /// Returns the concise dynamic effort/temperature level badge for the bottom status bar (`"low"`, `"medium"`, `"high"`, or custom float).
    pub fn effective_level_label(&self) -> String {
        if let Some(ref effort) = self.reasoning_effort {
            let trimmed = effort.trim().to_lowercase();
            if matches!(trimmed.as_str(), "low" | "medium" | "high") {
                return trimmed;
            }
        }
        if (self.temperature - 0.1).abs() < 0.05 {
            "low".to_string()
        } else if (self.temperature - 0.2).abs() < 0.06 || (self.temperature - 0.3).abs() < 0.06 {
            "medium".to_string()
        } else if (self.temperature - 0.6).abs() < 0.06 {
            "high".to_string()
        } else {
            let formatted = format!("{:.2}", self.temperature);
            formatted
                .trim_end_matches('0')
                .trim_end_matches('.')
                .to_string()
        }
    }

    /// Applies a unified `low`, `medium`, or `high` temperature & reasoning effort profile,
    /// scaling thinking budget dynamically from the active model's `models.dev` output limit if thinking is active.
    pub fn apply_effort_level(&mut self, level: &str) {
        let norm = level.trim().to_lowercase();
        let max_out = crate::agent::models::get_model_output_limit(&self.model);
        let min_tb = crate::constants::MIN_THINKING_BUDGET_TOKENS;
        let max_tb = crate::constants::MAX_THINKING_BUDGET_TOKENS;

        match norm.as_str() {
            "low" => {
                self.temperature = 0.1;
                self.reasoning_effort = Some("low".to_string());
                if self.thinking_budget.is_some() {
                    self.thinking_budget = Some((max_out / 8).clamp(min_tb, max_tb));
                }
            }
            "high" => {
                self.temperature = 0.6;
                self.reasoning_effort = Some("high".to_string());
                if self.thinking_budget.is_some() {
                    self.thinking_budget = Some((max_out / 2).clamp(min_tb, max_tb));
                }
            }
            _ => {
                self.temperature = 0.3;
                self.reasoning_effort = Some("medium".to_string());
                if self.thinking_budget.is_some() {
                    self.thinking_budget = Some((max_out / 4).clamp(min_tb, max_tb));
                }
            }
        }
    }

    /// Cycles the active effort/temperature preset (`low -> medium -> high -> low`) and returns the new level.
    pub fn cycle_effort_level(&mut self) -> &'static str {
        let next = match self.effective_level_label().as_str() {
            "low" => "medium",
            "medium" => "high",
            _ => "low",
        };
        self.apply_effort_level(next);
        next
    }

    /// Returns whether extended thinking / reasoning is currently enabled
    pub fn is_thinking_enabled(&self) -> bool {
        self.thinking_budget
            .map(|b| b >= crate::constants::MIN_THINKING_BUDGET_TOKENS)
            .unwrap_or(false)
    }

    /// Returns the effective thinking budget in tokens
    pub fn effective_thinking_budget(&self) -> Option<usize> {
        self.thinking_budget
            .filter(|&b| b >= crate::constants::MIN_THINKING_BUDGET_TOKENS)
    }

    /// Effective TCP/TLS connect timeout in seconds
    pub fn effective_connect_timeout_secs(&self) -> u64 {
        self.connect_timeout_secs
            .unwrap_or(crate::constants::PROVIDER_CONNECT_TIMEOUT_SECS)
            .max(5)
    }

    /// Effective inter-token stream idle timeout in seconds, scaled adaptively when thinking/reasoning is enabled
    pub fn effective_stream_idle_timeout_secs(&self) -> u64 {
        if let Some(explicit) = self.stream_idle_timeout_secs {
            return explicit.max(15);
        }
        let base = crate::constants::PROVIDER_STREAM_IDLE_TIMEOUT_SECS;
        if !self.adaptive_timeout {
            return base;
        }
        let thinking_bonus = self
            .effective_thinking_budget()
            .map(|b| (b as u64 / 256).min(300))
            .unwrap_or(0);
        base.saturating_add(thinking_bonus)
    }

    /// Computes an adaptive Time-To-First-Token (TTFT) deadline in seconds based on:
    /// `base_connect + (prompt_tokens / prefill_speed) + thinking_headroom`
    pub fn compute_adaptive_ttft_secs(&self, prompt_tokens: usize, is_local: bool) -> u64 {
        let base = self.effective_connect_timeout_secs();
        if !self.adaptive_timeout {
            return base.max(60);
        }
        let prefill_tps: u64 = if is_local { 150 } else { 1500 };
        let prefill_allowance = (prompt_tokens as u64 / prefill_tps).min(300);
        let thinking_allowance = if self.is_thinking_enabled()
            || self
                .reasoning_effort
                .as_deref()
                .map(|r| !r.is_empty() && r != "none")
                .unwrap_or(false)
        {
            120
        } else {
            30
        };
        base.saturating_add(prefill_allowance)
            .saturating_add(thinking_allowance)
    }
}

fn default_temperature() -> f32 {
    0.2
}

fn default_max_tokens() -> usize {
    16384
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OllamaConfig {
    #[serde(default = "default_ollama_host")]
    pub host: String,
}

impl Default for OllamaConfig {
    fn default() -> Self {
        Self {
            host: default_ollama_host(),
        }
    }
}

fn default_ollama_host() -> String {
    crate::constants::DEFAULT_OLLAMA_HOST.to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentConfig {
    #[serde(default)]
    pub auto_approve: bool,

    #[serde(default = "default_approval_policy")]
    pub approval_policy: String,

    #[serde(default = "default_timeout_secs")]
    pub timeout: u64,

    #[serde(default = "default_map_tokens")]
    pub map_tokens: usize,

    #[serde(default = "default_warning_threshold")]
    pub warning_threshold: f32,

    #[serde(default = "default_true")]
    pub auto_heal: bool,

    #[serde(default = "default_true")]
    pub streaming: bool,

    /// Native Caveman Mode (`caveman-rs`): cuts conversational & subagent prose tokens by ~75% while preserving 100% technical accuracy
    #[serde(default)]
    pub caveman_mode: bool,

    /// Caveman compression intensity (`"lite"`, `"full"`, `"ultra"`, `"wenyan-lite"`, `"wenyan-full"`, `"wenyan-ultra"`)
    #[serde(default = "default_caveman_level")]
    pub caveman_level: String,

    #[serde(default = "default_tool_mode")]
    pub tool_mode: ToolFilterMode,

    #[serde(default = "default_true")]
    pub syntax_barrier: bool,

    #[serde(default = "default_true")]
    pub auto_lint: bool,

    #[serde(default = "default_true")]
    pub parallel_tools: bool,

    #[serde(default = "default_true")]
    pub speculative_execution: bool,

    #[serde(default = "default_max_parallel_tools")]
    pub max_parallel_tools: usize,

    #[serde(default = "default_true")]
    pub compact_tool_schemas: bool,

    /// Maximum tool calling iterations per turn (0 = unbounded continuous autonomous execution)
    #[serde(default = "default_max_tool_iterations")]
    pub max_tool_iterations: usize,

    #[serde(default = "default_true")]
    pub auto_continue: bool,

    #[serde(default = "default_max_auto_continues")]
    pub max_auto_continues: usize,

    #[serde(default)]
    pub intent: IntentConfig,

    /// Require an `ask_user` approval round before the first edit of a large
    /// plan (see `agent::plan_gate`).
    #[serde(default = "default_true")]
    pub plan_approval: bool,

    /// Plan review mode for `create_plan` / `power_plan`: `"auto"` (structural checks always,
    /// plus an advisory LLM review for project-sized work), `"llm"` (LLM review on every plan),
    /// or `"structural"` (no LLM call). Structural failures block; LLM findings are advisory.
    #[serde(default = "default_plan_review")]
    pub plan_review: String,

    /// MiniPub subagent orchestration, concurrency, budget, timeout, and isolation settings
    #[serde(default, alias = "subagent", alias = "subagents")]
    pub minipub: MiniPubConfig,
}

/// Configuration for `minipub` (MiniPub) subagent workers managed by the `minicode` Orchestrator.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MiniPubConfig {
    /// Maximum parallel `minipub` workers allowed concurrently (default: 4)
    #[serde(default = "default_max_parallel_minipubs")]
    pub max_parallel: usize,

    /// Default token budget per `minipub` worker (default: 50,000)
    #[serde(default = "default_minipub_token_budget")]
    pub default_token_budget: usize,

    /// Default maximum tool-calling turns per `minipub` worker (default: 15)
    #[serde(default = "default_minipub_max_turns")]
    pub default_max_turns: usize,

    /// Activity-heartbeat stall timeout in seconds (resets on every token or tool output; default: 90s)
    #[serde(default = "default_minipub_inactivity_timeout_secs")]
    pub inactivity_timeout_secs: u64,

    /// Workspace isolation strategy for mutating `minipub` workers:
    /// "auto" (file leases for disjoint boundaries, git worktree for overlapping/unbounded),
    /// "always" (always create `.minicode/worktrees/`),
    /// "file_leases" (in-place cooperative file leases),
    /// "never" (shared workspace without worktree).
    #[serde(default = "default_minipub_worktree_isolation")]
    pub worktree_isolation: String,

    /// Additional tool names granted to all `minipub` workers beyond their role whitelist
    #[serde(default)]
    pub extra_allowed_tools: Vec<String>,

    /// When true (default), all MiniPubs inherit the Orchestrator's Primary, 1st Fallback, and 2nd Fallback unless overridden
    #[serde(default = "default_true")]
    pub sync_with_orchestrator: bool,

    /// Optional shared default 3-slot chain (`primary`, `fallback_1`, `fallback_2`) for all MiniPubs
    #[serde(default)]
    pub default_chain: MiniPubModelChain,

    /// Per-MiniPub 3-slot chain overrides keyed by canonical role slug (e.g. `"coder"`, `"vision"`, `"architect"`)
    #[serde(default)]
    pub role_models: std::collections::HashMap<String, MiniPubModelChain>,

    /// Per-category 3-slot chain overrides keyed by `delegate_task` category (e.g. `"quick"`,
    /// `"ultrabrain"`, `"visual_engineering"`). Takes precedence over `role_models` when a task
    /// is delegated with that category; explicit per-call model overrides still win.
    #[serde(default)]
    pub category_models: std::collections::HashMap<String, MiniPubModelChain>,

    /// Per-MiniPub custom instruction fragments keyed by role slug (e.g. `"coder"`, `"reviewer"`, `"all"`).
    /// Merged with project-local `.minicode/fragments.json` and injected into `<user_role_instructions>`.
    #[serde(default)]
    pub fragments: std::collections::HashMap<String, Vec<String>>,
}

impl Default for MiniPubConfig {
    fn default() -> Self {
        Self {
            max_parallel: default_max_parallel_minipubs(),
            default_token_budget: default_minipub_token_budget(),
            default_max_turns: default_minipub_max_turns(),
            inactivity_timeout_secs: default_minipub_inactivity_timeout_secs(),
            worktree_isolation: default_minipub_worktree_isolation(),
            extra_allowed_tools: Vec::new(),
            sync_with_orchestrator: true,
            default_chain: MiniPubModelChain::default(),
            role_models: std::collections::HashMap::new(),
            category_models: std::collections::HashMap::new(),
            fragments: std::collections::HashMap::new(),
        }
    }
}

impl MiniPubConfig {
    /// Normalizes a delegation category key the same way `SubagentConfig::with_category` does
    /// (lowercase, `-`/space → `_`).
    #[must_use]
    pub fn normalize_category(raw: &str) -> String {
        raw.trim().to_lowercase().replace(['-', ' '], "_")
    }

    /// Returns the configured model chain for a delegation category, if any.
    #[must_use]
    pub fn category_chain(&self, category: &str) -> Option<&MiniPubModelChain> {
        self.category_models
            .get(&Self::normalize_category(category))
            .filter(|c| !c.is_empty())
    }
    /// Resolves hierarchical prompt fragments for a given `SubagentRole`, merging:
    /// 1. Global `[agent.minipub.fragments]` from `config.toml` (keys `"all"` / `"*"` + role slug)
    /// 2. Project-local `.minicode/fragments.json` (`{ "coder": ["..."], "all": ["..."] }`)
    pub fn resolve_role_fragments(
        &self,
        role: crate::agent::subagent::types::SubagentRole,
        workspace_root: &std::path::Path,
    ) -> Vec<String> {
        let mut out = Vec::new();
        let target_slug = role.as_str().to_lowercase();

        let mut collect_from_map = |map: &std::collections::HashMap<String, Vec<String>>| {
            for (raw_key, items) in map {
                let key_trimmed = raw_key.trim().to_lowercase();
                let matches = if key_trimmed == "all" || key_trimmed == "*" {
                    true
                } else {
                    crate::agent::subagent::types::SubagentRole::from_slug_or_alias(&key_trimmed)
                        == Some(role)
                };
                if matches {
                    for item in items {
                        let trimmed = item.trim();
                        if !trimmed.is_empty() && !out.iter().any(|existing| existing == trimmed) {
                            out.push(trimmed.to_string());
                        }
                    }
                }
            }
        };

        collect_from_map(&self.fragments);

        let local_path = workspace_root.join(".minicode/fragments.json");
        if let Ok(raw_json) = std::fs::read_to_string(&local_path) {
            if let Ok(parsed_val) = serde_json::from_str::<serde_json::Value>(&raw_json) {
                let mut local_map: std::collections::HashMap<String, Vec<String>> =
                    std::collections::HashMap::new();
                if let Some(obj) = parsed_val.as_object() {
                    let source_obj = obj
                        .get("fragments")
                        .and_then(|v| v.as_object())
                        .unwrap_or(obj);
                    for (k, v) in source_obj {
                        if let Some(arr) = v.as_array() {
                            let strings: Vec<String> = arr
                                .iter()
                                .filter_map(|x| x.as_str().map(String::from))
                                .collect();
                            if !strings.is_empty() {
                                local_map.insert(k.clone(), strings);
                            }
                        } else if let Some(single) = v.as_str() {
                            local_map.insert(k.clone(), vec![single.to_string()]);
                        }
                    }
                }
                collect_from_map(&local_map);
            }
        }

        let _ = target_slug;
        out
    }
}

fn default_max_parallel_minipubs() -> usize {
    crate::constants::DEFAULT_MAX_PARALLEL_MINIPUBS
}

fn default_minipub_token_budget() -> usize {
    crate::constants::DEFAULT_MINIPUB_TOKEN_BUDGET
}

fn default_minipub_max_turns() -> usize {
    crate::constants::DEFAULT_MINIPUB_MAX_TURNS
}

fn default_minipub_inactivity_timeout_secs() -> u64 {
    crate::constants::DEFAULT_MINIPUB_INACTIVITY_TIMEOUT_SECS
}

fn default_minipub_worktree_isolation() -> String {
    crate::constants::DEFAULT_MINIPUB_WORKTREE_ISOLATION.to_string()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ToolFilterMode {
    #[default]
    Dynamic,
    CoreOnly,
    Full,
    ReadOnly,
    Standard,
}

impl std::fmt::Display for ToolFilterMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Dynamic => write!(f, "dynamic"),
            Self::CoreOnly => write!(f, "core_only"),
            Self::Full => write!(f, "full"),
            Self::ReadOnly => write!(f, "read_only"),
            Self::Standard => write!(f, "standard"),
        }
    }
}

impl std::str::FromStr for ToolFilterMode {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().trim() {
            "dynamic" => Ok(Self::Dynamic),
            "core_only" | "core" => Ok(Self::CoreOnly),
            "full" | "all" => Ok(Self::Full),
            "read_only" | "readonly" => Ok(Self::ReadOnly),
            "standard" => Ok(Self::Standard),
            other => Err(format!(
                "Invalid tool_mode '{}'. Must be 'dynamic', 'core_only', 'full', 'read_only', or 'standard'",
                other
            )),
        }
    }
}

fn default_tool_mode() -> ToolFilterMode {
    ToolFilterMode::Dynamic
}

fn default_max_parallel_tools() -> usize {
    crate::constants::DEFAULT_MAX_PARALLEL_TOOLS
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            auto_approve: false,
            approval_policy: default_approval_policy(),
            timeout: default_timeout_secs(),
            map_tokens: default_map_tokens(),
            warning_threshold: default_warning_threshold(),
            auto_heal: true,
            streaming: true,
            caveman_mode: false,
            caveman_level: default_caveman_level(),
            tool_mode: default_tool_mode(),
            syntax_barrier: true,
            auto_lint: true,
            parallel_tools: true,
            speculative_execution: true,
            max_parallel_tools: crate::constants::DEFAULT_MAX_PARALLEL_TOOLS,
            compact_tool_schemas: true,
            max_tool_iterations: default_max_tool_iterations(),
            auto_continue: true,
            max_auto_continues: default_max_auto_continues(),
            intent: IntentConfig::default(),
            plan_approval: true,
            plan_review: default_plan_review(),
            minipub: MiniPubConfig::default(),
        }
    }
}

fn default_plan_review() -> String {
    "auto".to_string()
}

fn default_max_tool_iterations() -> usize {
    crate::constants::DEFAULT_MAX_TOOL_ITERATIONS
}

fn default_max_auto_continues() -> usize {
    crate::constants::DEFAULT_MAX_AUTO_CONTINUES
}

fn default_approval_policy() -> String {
    "strict".to_string()
}

fn default_caveman_level() -> String {
    "full".to_string()
}

fn default_timeout_secs() -> u64 {
    30
}

fn default_map_tokens() -> usize {
    1024
}

fn default_warning_threshold() -> f32 {
    0.70
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IntentConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_intent_persistence_file")]
    pub persistence_file: String,
    #[serde(default = "default_intent_drift_warning_turns")]
    pub drift_warning_turns: usize,
    #[serde(default = "default_true")]
    pub auto_extract: bool,
    #[serde(default = "default_intent_max_items")]
    pub max_ledger_items: usize,
}

impl Default for IntentConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            persistence_file: default_intent_persistence_file(),
            drift_warning_turns: default_intent_drift_warning_turns(),
            auto_extract: true,
            max_ledger_items: default_intent_max_items(),
        }
    }
}

fn default_intent_persistence_file() -> String {
    crate::constants::DEFAULT_INTENT_PERSISTENCE_FILE.to_string()
}

fn default_intent_drift_warning_turns() -> usize {
    crate::constants::DEFAULT_INTENT_DRIFT_WARNING_TURNS
}

fn default_intent_max_items() -> usize {
    crate::constants::DEFAULT_INTENT_MAX_ITEMS
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TodoWidgetStyle {
    #[default]
    Tree,
    Card,
    Rail,
    Minimal,
    Checklist,
    Pipeline,
}

impl TodoWidgetStyle {
    #[allow(dead_code)]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Tree => "tree",
            Self::Card => "card",
            Self::Rail => "rail",
            Self::Minimal => "minimal",
            Self::Checklist => "checklist",
            Self::Pipeline => "pipeline",
        }
    }
}

/// Visual layout aesthetic for the Swarm Flight Deck in MiniTask.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SwarmDashboardStyle {
    /// Minimal + Stylish (Nordic neo-TUI with sleek meters and accent badges) - Default
    #[default]
    Stylish,
    /// Minimal + GitGraph Pipeline (Topological branch DAG nodes with inline comms)
    GitGraph,
    /// Clean Modernist (Floating card boxes with status pills)
    Modern,
    /// Ultra-Minimalist (Zero-border clean whitespace with muted guide lines)
    Minimal,
    /// High-Density Cockpit (3-column split view: Workers, Message Bus, Live Logs)
    Cockpit,
}

#[allow(dead_code)]
impl SwarmDashboardStyle {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Stylish => "stylish",
            Self::GitGraph => "gitgraph",
            Self::Modern => "modern",
            Self::Minimal => "minimal",
            Self::Cockpit => "cockpit",
        }
    }

    pub fn from_str_loose(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "gitgraph" | "git" | "dag" | "pipeline" => Self::GitGraph,
            "modern" | "cards" | "floating" => Self::Modern,
            "minimal" | "clean" | "tree" => Self::Minimal,
            "cockpit" | "dense" | "mission" => Self::Cockpit,
            _ => Self::Stylish,
        }
    }

    pub fn next(&self) -> Self {
        match self {
            Self::Stylish => Self::GitGraph,
            Self::GitGraph => Self::Modern,
            Self::Modern => Self::Minimal,
            Self::Minimal => Self::Cockpit,
            Self::Cockpit => Self::Stylish,
        }
    }
}

/// Visual layout aesthetic for floating TUI Toast notifications.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ToastStyle {
    /// OpenCode Split-Bar: vertical `┃` left/right accent bars with breathing room, zero horizontal borders (Default)
    #[default]
    OpenCode,
    /// Clean Rounded Card: `╭───╮` / `│ … │` / `╰───╯`
    Rounded,
    /// Spacious Rounded Card: `╭───╮` with top/bottom breathing rows inside `│ … │` and `╰───╯`
    RoundedPadded,
    /// Square Corner-Bracket Frame: `┌─ … ─┐` / `│ … │` / `└─ … ─┘`
    Bracket,
    /// Minimal Left Accent Bar: single thick `┃` left spine with open top, right, and bottom
    LeftBar,
    /// Asymmetric Half-Top + Left + Full-Bottom Cradle (Rounded corners `╭` / `╰`)
    CradleRound,
    /// Asymmetric Half-Top + Left + Full-Bottom Cradle (Square corners `┌` / `└`)
    CradleSquare,
    /// Asymmetric Left + Full-Bottom L-Frame (Rounded corner `╰`)
    LRound,
    /// Asymmetric Left + Full-Bottom L-Frame (Square corner `└`)
    LSquare,
}

#[allow(dead_code)]
impl ToastStyle {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::OpenCode => "opencode",
            Self::Rounded => "rounded",
            Self::RoundedPadded => "rounded_padded",
            Self::Bracket => "bracket",
            Self::LeftBar => "left_bar",
            Self::CradleRound => "cradle_round",
            Self::CradleSquare => "cradle_square",
            Self::LRound => "l_round",
            Self::LSquare => "l_square",
        }
    }

    #[must_use]
    pub fn from_str_loose(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "rounded" | "card" | "nordic" => Self::Rounded,
            "rounded_padded" | "spacious_rounded" => Self::RoundedPadded,
            "bracket" | "square" | "corner_bracket" => Self::Bracket,
            "left_bar" | "left" | "minimal" => Self::LeftBar,
            "cradle_round" | "cradle" => Self::CradleRound,
            "cradle_square" => Self::CradleSquare,
            "l_round" | "l_frame" | "lframe" => Self::LRound,
            "l_square" => Self::LSquare,
            _ => Self::OpenCode,
        }
    }
}

/// Configurable icon/glyph theme for floating Toast Notifications (`Success`, `Info`, `Warning`, `Error`, `Subagent`, `Task`, `Skill`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ToastSymbolTheme {
    /// Clean 1-cell ASCII / minimal operator glyphs (`✓`, `i`, `!`, `x`, `@`, `#`, `*`)
    #[default]
    Ascii,
    /// Crisp geometric Unicode glyphs (`✓`, `◈`, `△`, `✗`, `◉`, `◆`, `✦`)
    Geometric,
    /// Expressive status emojis (`✅`, `💡`, `⚠️`, `❌`, `🤖`, `📋`, `🧠`)
    Emoji,
    /// Expressive mini kaomoji badges (`(•‿•)`, `(・_・)`, `(°△°)`, `(×_×)`, `(◉_◉)`, `(ง•_•)ง`, `(✧_✧)`)
    Kaomoji,
    /// Technical & directional operator glyphs (`✔`, `▸`, `⚡`, `✖`, `❖`, `⚙`, `✧`)
    Cyber,
    /// Compact bracketed status tags (`[OK]`, `[i]`, `[!]`, `[ERR]`, `[BOT]`, `[TSK]`, `[SKL]`)
    Bracket,
}

const TOAST_SYMBOL_THEME_TABLE: &[(ToastSymbolTheme, &str, &[&str])] = &[
    (
        ToastSymbolTheme::Ascii,
        "ascii",
        &["ascii", "minimal", "default", "plain"],
    ),
    (
        ToastSymbolTheme::Geometric,
        "geometric",
        &["geometric", "unicode", "shapes", "symbols", "modern"],
    ),
    (
        ToastSymbolTheme::Emoji,
        "emoji",
        &["emoji", "emojis", "icons"],
    ),
    (
        ToastSymbolTheme::Kaomoji,
        "kaomoji",
        &["kaomoji", "kamoji", "faces", "anime"],
    ),
    (
        ToastSymbolTheme::Cyber,
        "cyber",
        &["cyber", "nerd", "tech", "operator"],
    ),
    (
        ToastSymbolTheme::Bracket,
        "bracket",
        &["bracket", "brackets", "tags", "badge"],
    ),
];

#[allow(dead_code)]
impl ToastSymbolTheme {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        TOAST_SYMBOL_THEME_TABLE
            .iter()
            .find_map(|(theme, id, _)| (*theme == self).then_some(*id))
            .unwrap_or("ascii")
    }

    #[must_use]
    pub fn from_str_loose(s: &str) -> Self {
        let needle = s.trim();
        TOAST_SYMBOL_THEME_TABLE
            .iter()
            .find_map(|(theme, _, aliases)| {
                aliases
                    .iter()
                    .any(|alias| alias.eq_ignore_ascii_case(needle))
                    .then_some(*theme)
            })
            .unwrap_or_default()
    }
}

/// Visual layout aesthetic for the live bottom Activity Bar (`Thinking...` / `Writing... | ...`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ActivityBarStyle {
    /// Style 1 (Default): Clean `Thinking...` (no pipe), then `Action... | tool_name · target` during actions
    #[default]
    ActionDetail,
    /// Style 2 (Ultra-Minimal): Clean `Thinking...`, then `Action... | tool_name` during actions
    ActionTool,
    /// Style 3 (Hermes Trail): `Thinking...`, then `Working... | <action + target>` and `Thinking... | after <tool> · step N`
    HermesTrail,
    /// Classic: Original minimal label only (`Generating... (12.4s • esc to interrupt)`)
    Classic,
    /// Emoji + Style 1: `Action... | <emoji> tool_name · target` (emoji after `|` only)
    EmojiDetail,
    /// Emoji + Style 2: `Action... | <emoji> tool_name` (emoji after `|` only)
    EmojiTool,
    /// Emoji + Style 3: `Working... | <emoji> <action + target>` (emoji after `|` only)
    EmojiTrail,
    /// Kaomoji + Style 1: `Action... | <kaomoji> tool_name · target` (kaomoji after `|` only)
    KaomojiDetail,
    /// Kaomoji + Style 2: `Action... | <kaomoji> tool_name` (kaomoji after `|` only)
    KaomojiTool,
    /// Kaomoji + Style 3: `Working... | <kaomoji> <action + target>` (kaomoji after `|` only)
    KaomojiTrail,
    /// Compact Pill Badge: `Action... | [tool_name] target`
    CompactBadge,
    /// Cyber Telemetry Arrow: `Action... | ▸ tool_name ──▸ target`
    CyberArrow,
}

#[allow(dead_code)]
impl ActivityBarStyle {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ActionDetail => "action_detail",
            Self::ActionTool => "action_tool",
            Self::HermesTrail => "hermes_trail",
            Self::Classic => "classic",
            Self::EmojiDetail => "emoji_detail",
            Self::EmojiTool => "emoji_tool",
            Self::EmojiTrail => "emoji_trail",
            Self::KaomojiDetail => "kaomoji_detail",
            Self::KaomojiTool => "kaomoji_tool",
            Self::KaomojiTrail => "kaomoji_trail",
            Self::CompactBadge => "compact_badge",
            Self::CyberArrow => "cyber_arrow",
        }
    }

    #[must_use]
    pub fn from_str_loose(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "action_detail" | "detail" | "style1" | "default" => Self::ActionDetail,
            "action_tool" | "tool" | "style2" | "minimal" => Self::ActionTool,
            "hermes_trail" | "hermes" | "trail" | "style3" => Self::HermesTrail,
            "classic" | "legacy" | "original" => Self::Classic,
            "emoji_detail" | "emoji" | "emoji1" => Self::EmojiDetail,
            "emoji_tool" | "emoji2" => Self::EmojiTool,
            "emoji_trail" | "emoji_hermes" | "emoji3" => Self::EmojiTrail,
            "kaomoji_detail" | "kaomoji" | "kaomoji1" => Self::KaomojiDetail,
            "kaomoji_tool" | "kaomoji2" => Self::KaomojiTool,
            "kaomoji_trail" | "kaomoji_hermes" | "kaomoji3" => Self::KaomojiTrail,
            "compact_badge" | "badge" | "pill" => Self::CompactBadge,
            "cyber_arrow" | "arrow" | "cyber" => Self::CyberArrow,
            _ => Self::ActionDetail,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UiConfig {
    #[serde(default = "default_theme")]
    pub theme: String,

    #[serde(default = "default_animation")]
    pub animation: String,

    #[serde(default = "default_todo_style")]
    pub todo_style: String,

    #[serde(default = "default_swarm_style")]
    pub swarm_style: String,

    #[serde(default = "default_toast_style")]
    pub toast_style: String,

    #[serde(default = "default_toast_symbols")]
    pub toast_symbols: String,

    #[serde(default = "default_activity_style")]
    pub activity_style: String,

    #[serde(default = "default_menu_style")]
    pub menu_style: String,

    #[serde(default)]
    pub plain: bool,

    #[serde(default = "default_max_width")]
    pub max_width: usize,

    #[serde(default)]
    pub show_cost: bool,
}

impl UiConfig {
    pub fn todo_style_enum(&self) -> TodoWidgetStyle {
        match self.todo_style.to_lowercase().as_str() {
            "card" | "box" => TodoWidgetStyle::Card,
            "rail" | "gutter" => TodoWidgetStyle::Rail,
            "minimal" | "open" => TodoWidgetStyle::Minimal,
            "checklist" | "markdown" | "check" => TodoWidgetStyle::Checklist,
            "pipeline" | "steps" | "stepper" => TodoWidgetStyle::Pipeline,
            _ => TodoWidgetStyle::Tree,
        }
    }

    #[allow(dead_code)]
    pub fn swarm_style_enum(&self) -> SwarmDashboardStyle {
        SwarmDashboardStyle::from_str_loose(&self.swarm_style)
    }

    #[must_use]
    pub fn toast_style_enum(&self) -> ToastStyle {
        ToastStyle::from_str_loose(&self.toast_style)
    }

    #[must_use]
    pub fn toast_symbols_enum(&self) -> ToastSymbolTheme {
        ToastSymbolTheme::from_str_loose(&self.toast_symbols)
    }

    #[must_use]
    pub fn activity_style_enum(&self) -> ActivityBarStyle {
        ActivityBarStyle::from_str_loose(&self.activity_style)
    }

    #[must_use]
    pub fn menu_style_enum(&self) -> crate::ui::intellisense::MenuStyle {
        crate::ui::intellisense::MenuStyle::from_str_loose(&self.menu_style)
    }
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            theme: default_theme(),
            animation: default_animation(),
            todo_style: default_todo_style(),
            swarm_style: default_swarm_style(),
            toast_style: default_toast_style(),
            toast_symbols: default_toast_symbols(),
            activity_style: default_activity_style(),
            menu_style: default_menu_style(),
            plain: false,
            max_width: default_max_width(),
            show_cost: false,
        }
    }
}

fn default_theme() -> String {
    "auto".to_string()
}

fn default_animation() -> String {
    "dual_pillars".to_string()
}

fn default_swarm_style() -> String {
    "stylish".to_string()
}

fn default_todo_style() -> String {
    "tree".to_string()
}

fn default_toast_style() -> String {
    "opencode".to_string()
}

fn default_toast_symbols() -> String {
    "ascii".to_string()
}

fn default_activity_style() -> String {
    "action_detail".to_string()
}

fn default_menu_style() -> String {
    crate::constants::DEFAULT_MENU_STYLE.to_string()
}

fn default_max_width() -> usize {
    100
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LoggingConfig {
    #[serde(default = "default_log_level")]
    pub level: String,

    #[serde(default = "default_log_file")]
    pub file: bool,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: default_log_level(),
            file: default_log_file(),
        }
    }
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_log_file() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct McpConfig {
    #[serde(default)]
    pub servers: std::collections::HashMap<String, McpServerConfig>,

    /// Policy for dangerous tools (exec_cmd, write_file, patch_file) when
    /// minicode runs as an MCP server: Some("deny") blocks them; None/Some
    /// ("allow") permits everything.
    #[serde(default)]
    pub approval_policy: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McpServerConfig {
    #[serde(default = "default_mcp_transport")]
    pub transport: McpTransport,

    #[serde(default)]
    pub command: Option<String>,

    #[serde(default)]
    pub args: Option<Vec<String>>,

    #[serde(default)]
    pub url: Option<String>,

    #[serde(default)]
    pub env: Option<std::collections::HashMap<String, String>>,

    #[serde(default = "default_true")]
    pub enabled: bool,

    #[serde(default)]
    pub timeout_secs: Option<u64>,
}

impl McpServerConfig {
    pub fn validate(&self, server_name: &str) -> std::result::Result<(), String> {
        match self.transport {
            McpTransport::Stdio => {
                if self.command.as_deref().unwrap_or("").trim().is_empty() {
                    return Err(format!(
                        "MCP server '{}' with stdio transport missing 'command'",
                        server_name
                    ));
                }
            }
            McpTransport::Sse | McpTransport::Http => {
                if self.url.as_deref().unwrap_or("").trim().is_empty() {
                    return Err(format!(
                        "MCP server '{}' with http/sse transport missing 'url'",
                        server_name
                    ));
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum McpTransport {
    Stdio,
    Sse,
    Http,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GitConfig {
    #[serde(default = "default_true")]
    pub auto_commit: bool,

    #[serde(default)]
    pub dirty_commit: bool,

    #[serde(default = "default_true")]
    pub ai_commit_messages: bool,
}

impl Default for GitConfig {
    fn default() -> Self {
        Self {
            auto_commit: true,
            dirty_commit: false,
            ai_commit_messages: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BrowserSettingsConfig {
    /// Ordered engine priority list for headless automation (default: ["chrome", "obscura", "firefox"])
    #[serde(default = "default_headless_priority")]
    pub headless_priority: Vec<String>,

    /// Ordered engine priority list for GUI windowed automation (default: ["chrome", "firefox", "obscura"])
    #[serde(default = "default_gui_priority")]
    pub gui_priority: Vec<String>,

    /// Explicit override engine (obscura | chrome | firefox)
    #[serde(default)]
    pub default_engine: Option<String>,
}

fn default_headless_priority() -> Vec<String> {
    vec![
        "chrome".to_string(),
        "obscura".to_string(),
        "firefox".to_string(),
    ]
}

fn default_gui_priority() -> Vec<String> {
    vec![
        "chrome".to_string(),
        "obscura".to_string(),
        "firefox".to_string(),
    ]
}

impl Default for BrowserSettingsConfig {
    fn default() -> Self {
        Self {
            headless_priority: default_headless_priority(),
            gui_priority: default_gui_priority(),
            default_engine: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RawBrowserConfig {
    pub headless_priority: Option<Vec<String>>,
    pub gui_priority: Option<Vec<String>>,
    pub default_engine: Option<String>,
}

fn default_mcp_transport() -> McpTransport {
    McpTransport::Stdio
}

impl McpConfig {
    /// Resolved approval policy for dangerous tools in MCP serve mode.
    pub fn effective_approval_policy(&self) -> &str {
        self.approval_policy.as_deref().unwrap_or("allow")
    }
}

fn default_true() -> bool {
    true
}

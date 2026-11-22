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

    /// Caveman compression intensity (`"lite"`, `"full"`, `"ultra"`, `"mega"`)
    #[serde(default = "default_caveman_level")]
    pub caveman_level: String,

    /// Native Ponytail Minimalist Engineering Mode (`dietrichgebert/ponytail`): 7-Rung YAGNI Ladder, AST symbol reuse & shortcut debt discipline
    #[serde(default = "default_true")]
    pub ponytail_mode: bool,

    /// Ponytail minimalist intensity (`"lite"`, `"full"`, `"ultra"`)
    #[serde(default = "default_ponytail_level")]
    pub ponytail_level: String,

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
            ponytail_mode: true,
            ponytail_level: default_ponytail_level(),
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

fn default_ponytail_level() -> String {
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
    /// Universal Unicode Technical Symbols (`✓`, `◈`, `△`, `✗`, `❖`, `⎇`, `⚡`) — zero boxes in any font
    Technical,
    /// Nerd Font Codicons vector icons (`\u{eab2}`, `\u{ea74}`, `\u{ea6c}`, `\u{ea87}`, `\u{eb08}`, `\u{eaa6}`, `\u{ea61}`)
    NerdFont,
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
        &["cyber", "tech", "operator"],
    ),
    (
        ToastSymbolTheme::Bracket,
        "bracket",
        &["bracket", "brackets", "tags", "badge"],
    ),
    (
        ToastSymbolTheme::Technical,
        "technical",
        &["technical", "alt", "branch", "universal", "utf8"],
    ),
    (
        ToastSymbolTheme::NerdFont,
        "nerdfont",
        &["nerdfont", "nerd", "nf", "fontawesome", "fa", "codicons"],
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
    /// Nerd Font + Style 1: `Action... | <nf-icon> tool_name · target` (themed vector icon after `|`)
    NfDetail,
    /// Nerd Font + Style 2: `Action... | <nf-icon> tool_name` (themed vector icon after `|`)
    NfTool,
    /// Nerd Font + Style 3: `Working... | <nf-icon> <action target>` (Hermes trail with Nerd Font icon)
    NfTrail,
    /// Unicode Symbols + Style 1: `Action... | ✎ tool_name · target` (zero-box standard Unicode icons)
    SymDetail,
    /// Unicode Symbols + Style 2: `Action... | ✎ tool_name` (zero-box standard Unicode icons)
    SymTool,
    /// Unicode Symbols + Style 3: `Working... | ✎ <action target>` (Hermes trail with Unicode icons)
    SymTrail,
    /// Pure 7-bit ASCII + Style 1: `Action... | [>] tool_name - target`
    AsciiDetail,
    /// Pure 7-bit ASCII + Style 2: `Action... | [>] tool_name`
    AsciiTool,
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
            Self::NfDetail => "nf_detail",
            Self::NfTool => "nf_tool",
            Self::NfTrail => "nf_trail",
            Self::SymDetail => "sym_detail",
            Self::SymTool => "sym_tool",
            Self::SymTrail => "sym_trail",
            Self::AsciiDetail => "ascii_detail",
            Self::AsciiTool => "ascii_tool",
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
            "nf_detail" | "nerdfont" | "nerd_detail" | "nf1" => Self::NfDetail,
            "nf_tool" | "nerd_tool" | "nf2" => Self::NfTool,
            "nf_trail" | "nerd_trail" | "nf3" => Self::NfTrail,
            "sym_detail" | "symbols" | "symbol_detail" | "unicode" => Self::SymDetail,
            "sym_tool" | "symbol_tool" => Self::SymTool,
            "sym_trail" | "symbol_trail" => Self::SymTrail,
            "ascii_detail" | "ascii" => Self::AsciiDetail,
            "ascii_tool" => Self::AsciiTool,
            _ => Self::ActionDetail,
        }
    }
}

/// Visual border & container style for the bottom chat Input Box.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum InputBoxStyle {
    /// Clean rounded card frame `╭──╮ / │ › … │ / ╰──╯` (Default)
    #[default]
    Rounded,
    /// Sleek thick left accent rail `┃` on an elevated surface, open top/right/bottom
    LeftBar,
    /// OpenCode-style dual vertical accent bars `┃ … ┃` with zero horizontal borders
    SplitBar,
    /// Crisp thin horizontal top and bottom rules `───` with open sides
    TopBottom,
    /// Architectural sharp square frame `┌──┐ / │ … │ / └──┘`
    Square,
    /// Bold double-line frame `╔══╗ / ║ … ║ / ╚══╝`
    Double,
    /// Borderless elevated surface with clean vertical breathing room
    Minimal,
    /// Clean open surface anchored by a single accent bottom underline `───`
    Underline,
}

#[allow(dead_code)]
impl InputBoxStyle {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Rounded => "rounded",
            Self::LeftBar => "left_bar",
            Self::SplitBar => "split_bar",
            Self::TopBottom => "top_bottom",
            Self::Square => "square",
            Self::Double => "double",
            Self::Minimal => "minimal",
            Self::Underline => "underline",
        }
    }

    #[must_use]
    pub fn from_str_loose(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "left_bar" | "left" | "rail" | "bar" => Self::LeftBar,
            "split_bar" | "split" | "opencode" => Self::SplitBar,
            "top_bottom" | "rules" | "horizontal" | "lines" => Self::TopBottom,
            "square" | "box" | "sharp" => Self::Square,
            "double" | "heavy" => Self::Double,
            "minimal" | "clean" | "borderless" | "flat" => Self::Minimal,
            "underline" | "bottom" => Self::Underline,
            _ => Self::Rounded,
        }
    }
}

/// Visual layout style for the bottom Status Bar left telemetry (`provider/model · effort | path · git`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum StatusBarStyle {
    /// Classic dot/pipe layout `provider/model · effort | path · git:branch` (Default)
    #[default]
    Classic,
    /// Oh My Posh Pure minimal typography (`model  effort  path on ⎇ branch`)
    PoshPure,
    /// Oh My Posh clean slash rhythm (`model / effort / path / ⎇ branch`)
    PoshSlash,
    /// Oh My Posh flat velvet tinted chips with thin left accent rail (`▎ model `)
    PoshVelvet,
    /// Oh My Posh soft theme-tinted pill chips per status segment
    Pills,
    /// Oh My Posh rounded capsule bubbles (`model effort path branch`)
    PoshBubbles,
    /// Oh My Posh seamless Powerline ribbon with color-blended `` transitions
    Powerline,
    /// Oh My Posh seamless diagonal slant ribbon with color-blended `` transitions
    PoshSlant,
    /// Oh My Posh pointed diamond badges (` model   effort `)
    PoshDiamond,
    /// Ultra-compact terse readout (short model name, short effort, repo basename)
    Compact,
    /// Airy minimalist spacing without separator glyphs
    Minimal,
    /// Structured bracketed CLI badges `[model] [effort] [path] [git:branch]`
    Brackets,
    /// Crisp geometric operator indicators (`◈ model · ⚡effort │ ⌂ path · ⎇ branch`)
    Icons,
    /// Oh My Posh rounded capsules with universal Unicode symbols (`◈ model ⚡ effort ⌂ path ⎇ branch`)
    SymbolBubbles,
    /// Oh My Posh Powerline ribbon with universal Unicode symbols (` ◈ model  ⚡ effort  ⌂ path  ⎇ branch `)
    SymbolPowerline,
    /// Expressive Emoji indicators (`🧠 model · ⚡ effort │ 📁 path · 🌿 branch`)
    Emoji,
    /// Expressive Kaomoji indicators (`(◈_◈) model · (ง°_°)ง effort │ (◉_◉) path · (•‿•) branch`)
    Kaomoji,
    /// Strict 7-bit ASCII status bar (`[model] | effort | path | git:branch`)
    Ascii,
    /// Nerd Font clean icon bar (Codicons/Devicons)
    NerdIcons,
    /// Oh My Posh rounded capsules with Nerd Font Codicons
    NerdBubbles,
    /// Oh My Posh seamless Powerline ribbon with Nerd Font Codicons
    NerdPowerline,
}

#[allow(dead_code)]
impl StatusBarStyle {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Classic => "classic",
            Self::PoshPure => "posh_pure",
            Self::PoshSlash => "posh_slash",
            Self::PoshVelvet => "posh_velvet",
            Self::Pills => "pills",
            Self::PoshBubbles => "posh_bubbles",
            Self::Powerline => "powerline",
            Self::PoshSlant => "posh_slant",
            Self::PoshDiamond => "posh_diamond",
            Self::Compact => "compact",
            Self::Minimal => "minimal",
            Self::Brackets => "brackets",
            Self::Icons => "icons",
            Self::SymbolBubbles => "symbol_bubbles",
            Self::SymbolPowerline => "symbol_powerline",
            Self::Emoji => "emoji",
            Self::Kaomoji => "kaomoji",
            Self::Ascii => "ascii",
            Self::NerdIcons => "nerd_icons",
            Self::NerdBubbles => "nerd_bubbles",
            Self::NerdPowerline => "nerd_powerline",
        }
    }

    #[must_use]
    pub fn from_str_loose(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "posh_pure" | "pure" | "starship" => Self::PoshPure,
            "posh_slash" | "slash" | "clean_detailed" => Self::PoshSlash,
            "posh_velvet" | "velvet" | "rail" | "chips" => Self::PoshVelvet,
            "pills" | "pill" | "soft_pills" => Self::Pills,
            "posh_bubbles" | "bubbles" | "capsule" | "capsules" | "catppuccin" => Self::PoshBubbles,
            "powerline" | "posh_powerline" | "omp" | "chevron" | "arrows" => Self::Powerline,
            "posh_slant" | "slant" | "diagonal" => Self::PoshSlant,
            "posh_diamond" | "diamond" | "atomic" => Self::PoshDiamond,
            "compact" | "dense" | "short" => Self::Compact,
            "minimal" | "clean" | "plain" => Self::Minimal,
            "brackets" | "bracket" | "tags" => Self::Brackets,
            "icons" | "icon" | "glyphs" | "symbols" => Self::Icons,
            "symbol_bubbles" | "sym_bubbles" | "unicode_bubbles" => Self::SymbolBubbles,
            "symbol_powerline" | "sym_powerline" | "unicode_powerline" => Self::SymbolPowerline,
            "emoji" | "emojis" => Self::Emoji,
            "kaomoji" | "kamoji" => Self::Kaomoji,
            "ascii" | "retro" => Self::Ascii,
            "nerd_icons" | "nerdfont" | "nerd" | "nf" => Self::NerdIcons,
            "nerd_bubbles" | "nf_bubbles" => Self::NerdBubbles,
            "nerd_powerline" | "nf_powerline" => Self::NerdPowerline,
            _ => Self::Classic,
        }
    }
}

/// Visual meter style for the bottom-right Context Occupancy & KV Cache indicator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ContextMeterStyle {
    /// Compact numeric readout `kv (99%) • 212.7k / 1M` (Default)
    #[default]
    Compact,
    /// Progressive pie circle ring `○◔◑◕●` + percentage (`● kv 98% · ◔ 21%`)
    Circle,
    /// Crisp horizontal block progress bar (`kv 98% • [██░░░░░░░░] 21%`)
    Bar,
    /// Shaded density blocks (`kv 98% • █▒░░░░░░ 21%`)
    Blocks,
    /// High-resolution braille fill meter (`kv 98% • ⣿⣦⣀⣀⣀⣀ 21%`)
    Braille,
    /// Clean percentage-only readout (`kv 98% • ctx 21%`)
    Percent,
    /// Elevated pill chips (` kv 98% ` ` ◔ 21% `)
    Pills,
    /// Ultra-minimal compact readout `99% kv · 212.7k/1M`
    Minimal,
    /// Sleek thin horizontal line gauge `━━──────── 21%`
    Gauge,
    /// Universal Unicode Technical Symbols + pie ring (`◈ 98% • ⌘ ◔ 21%`)
    SymbolCircle,
    /// Universal Unicode Technical Symbols + block bar (`◈ 98% • ⌘ [██░░░░░░] 21%`)
    SymbolBar,
    /// Expressive Emoji + block bar (`⚡ 98% • 🧠 [██░░░░░░] 21%`)
    EmojiBar,
    /// Expressive Kaomoji telemetry (`(•‿•) 98% · (◉_◉) 21%`)
    Kaomoji,
    /// Pure 7-bit ASCII progress bar (`kv 98% | [##......] 21%`)
    AsciiBar,
    /// Nerd Font Codicons + pie gauge
    NerdIcons,
    /// Nerd Font Codicons + sleek block progress bar
    NerdBar,
}

#[allow(dead_code)]
impl ContextMeterStyle {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Compact => "compact",
            Self::Circle => "circle",
            Self::Bar => "bar",
            Self::Blocks => "blocks",
            Self::Braille => "braille",
            Self::Percent => "percent",
            Self::Pills => "pills",
            Self::Minimal => "minimal",
            Self::Gauge => "gauge",
            Self::SymbolCircle => "symbol_circle",
            Self::SymbolBar => "symbol_bar",
            Self::EmojiBar => "emoji_bar",
            Self::Kaomoji => "kaomoji",
            Self::AsciiBar => "ascii_bar",
            Self::NerdIcons => "nerd_icons",
            Self::NerdBar => "nerd_bar",
        }
    }

    #[must_use]
    pub fn from_str_loose(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "circle" | "pie" | "ring" | "donut" => Self::Circle,
            "bar" | "progress" | "meter" => Self::Bar,
            "blocks" | "shade" | "shaded" => Self::Blocks,
            "braille" | "dots" => Self::Braille,
            "percent" | "pct" | "percentage" => Self::Percent,
            "pills" | "pill" | "chips" => Self::Pills,
            "minimal" | "clean" => Self::Minimal,
            "gauge" | "line" | "thin" => Self::Gauge,
            "symbol_circle" | "symbols" | "sym_circle" => Self::SymbolCircle,
            "symbol_bar" | "sym_bar" => Self::SymbolBar,
            "emoji_bar" | "emoji" => Self::EmojiBar,
            "kaomoji" | "kamoji" => Self::Kaomoji,
            "ascii_bar" | "ascii" => Self::AsciiBar,
            "nerd_icons" | "nerd" | "nerdfont" | "nf" => Self::NerdIcons,
            "nerd_bar" | "nf_bar" => Self::NerdBar,
            _ => Self::Compact,
        }
    }
}

/// Visual style for the Mode Badges strip (`· ponytail:full | caveman:full | streaming`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ModeBarStyle {
    /// Classic piped mode strip `· ponytail:full | caveman:full | streaming` (Default)
    #[default]
    Classic,
    /// Oh My Posh Pure minimal quiet badges (`• ponytail·full  • caveman·full  • stream`)
    PoshPure,
    /// Oh My Posh clean minimal slash rhythm (`ponytail·full / caveman·full / stream`)
    PoshSlash,
    /// Oh My Posh velvet tinted chips with thin left accent rail (`▎ ponytail·full `)
    PoshVelvet,
    /// Oh My Posh soft theme-tinted pill chips per mode
    Pills,
    /// Oh My Posh rounded capsule bubbles (`ponytail·full caveman·full stream`)
    PoshBubbles,
    /// Oh My Posh right-aligned Powerline ribbon pointing left (` ponytail·full  …`)
    PoshPowerline,
    /// Oh My Posh right-aligned diagonal slant ribbon (` ponytail·full  …`)
    PoshSlant,
    /// Status dot indicators (`● ponytail:full  ● caveman:full  ● streaming`)
    Dots,
    /// Ultra-compact abbreviations (`pt:full · cm:full · strm`)
    Compact,
    /// Minimal clean spaced labels without pipes or leading dot
    Minimal,
    /// Structured CLI bracket badges (`[ponytail:full] [caveman:full] [streaming]`)
    Brackets,
    /// Universal Unicode Technical Symbols (`✦ ponytail·full · ◈ caveman·full · ▸ stream`)
    Icons,
    /// Oh My Posh rounded capsules with universal Unicode symbols (`✦ ponytail·full ◈ caveman·full ▸ stream`)
    SymbolBubbles,
    /// Oh My Posh right-aligned Powerline with universal Unicode symbols (` ✦ ponytail·full  …`)
    SymbolPowerline,
    /// Expressive Emoji badges (`✨ ponytail·full · 🔥 caveman·full · ⚡ stream`)
    Emoji,
    /// Expressive Kaomoji badges (`(✧_✧) ponytail · (ง•_•)ง caveman · (•‿•) stream`)
    Kaomoji,
    /// Pure 7-bit ASCII mode strip (`* ponytail:full | # caveman:full | > stream`)
    Ascii,
    /// Nerd Font clean Codicon icons
    NerdIcons,
    /// Oh My Posh rounded capsules with Nerd Font Codicon icons
    NerdBubbles,
    /// Hide the entire mode strip
    Hidden,
}

#[allow(dead_code)]
impl ModeBarStyle {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Classic => "classic",
            Self::PoshPure => "posh_pure",
            Self::PoshSlash => "posh_slash",
            Self::PoshVelvet => "posh_velvet",
            Self::Pills => "pills",
            Self::PoshBubbles => "posh_bubbles",
            Self::PoshPowerline => "posh_powerline",
            Self::PoshSlant => "posh_slant",
            Self::Dots => "dots",
            Self::Compact => "compact",
            Self::Minimal => "minimal",
            Self::Brackets => "brackets",
            Self::Icons => "icons",
            Self::SymbolBubbles => "symbol_bubbles",
            Self::SymbolPowerline => "symbol_powerline",
            Self::Emoji => "emoji",
            Self::Kaomoji => "kaomoji",
            Self::Ascii => "ascii",
            Self::NerdIcons => "nerd_icons",
            Self::NerdBubbles => "nerd_bubbles",
            Self::Hidden => "hidden",
        }
    }

    #[must_use]
    pub fn from_str_loose(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "posh_pure" | "pure" | "starship" => Self::PoshPure,
            "posh_slash" | "slash" => Self::PoshSlash,
            "posh_velvet" | "velvet" | "rail" | "chips" => Self::PoshVelvet,
            "pills" | "pill" | "soft_pills" => Self::Pills,
            "posh_bubbles" | "bubbles" | "capsule" | "capsules" | "catppuccin" => Self::PoshBubbles,
            "posh_powerline" | "powerline" | "omp" => Self::PoshPowerline,
            "posh_slant" | "slant" | "diagonal" => Self::PoshSlant,
            "dots" | "dot" | "led" => Self::Dots,
            "compact" | "short" | "mini" => Self::Compact,
            "minimal" | "clean" | "plain" => Self::Minimal,
            "brackets" | "bracket" | "tags" => Self::Brackets,
            "icons" | "icon" | "glyphs" | "symbols" => Self::Icons,
            "symbol_bubbles" | "sym_bubbles" | "unicode_bubbles" => Self::SymbolBubbles,
            "symbol_powerline" | "sym_powerline" | "unicode_powerline" => Self::SymbolPowerline,
            "emoji" | "emojis" => Self::Emoji,
            "kaomoji" | "kamoji" => Self::Kaomoji,
            "ascii" | "retro" => Self::Ascii,
            "nerd_icons" | "nerd" | "nerdfont" | "nf" => Self::NerdIcons,
            "nerd_bubbles" | "nf_bubbles" => Self::NerdBubbles,
            "hidden" | "hide" | "off" | "none" => Self::Hidden,
            _ => Self::Classic,
        }
    }
}

/// Visual style for the Activity Bar elapsed timer and interrupt hint (`(6.3s • esc to interrupt)`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ActivityHintStyle {
    /// Classic parenthesized timer + cancel hint `(6.3s · esc to cancel)` (Default)
    #[default]
    Classic,
    /// Elapsed timer only `(6.3s)` without the interrupt hint text
    TimerOnly,
    /// Minimal inline readout `6.3s · esc`
    Minimal,
    /// Structured pill tag `[6.3s | esc]`
    Pill,
    /// Sleek angle-bracket badge `⟨6.3s · ⎋ interrupt⟩`
    Bracket,
    /// Subtle dot-prefixed hint `· 6.3s · esc to stop`
    Dot,
    /// Bare compact seconds `6.3s`
    Compact,
    /// Universal Unicode Technical Symbols (`◷ 6.3s · ⎋ cancel`)
    Symbols,
    /// Expressive Emoji timer (`⏱ 6.3s · 🛑 esc`)
    Emoji,
    /// Expressive Kaomoji timer (`(•_•) 6.3s · esc`)
    Kaomoji,
    /// Pure 7-bit ASCII timer (`(6.3s | esc)`)
    Ascii,
    /// Nerd Font Codicon clock (`\u{ea82} 6.3s · esc`)
    NerdFont,
    /// Hide both elapsed timer and interrupt hint completely
    Hidden,
}

#[allow(dead_code)]
impl ActivityHintStyle {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Classic => "classic",
            Self::TimerOnly => "timer_only",
            Self::Minimal => "minimal",
            Self::Pill => "pill",
            Self::Bracket => "bracket",
            Self::Dot => "dot",
            Self::Compact => "compact",
            Self::Symbols => "symbols",
            Self::Emoji => "emoji",
            Self::Kaomoji => "kaomoji",
            Self::Ascii => "ascii",
            Self::NerdFont => "nerdfont",
            Self::Hidden => "hidden",
        }
    }

    #[must_use]
    pub fn from_str_loose(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "timer_only" | "timer" | "time" => Self::TimerOnly,
            "minimal" | "short" | "clean" => Self::Minimal,
            "pill" | "pills" | "badge" => Self::Pill,
            "bracket" | "brackets" | "angle" => Self::Bracket,
            "dot" | "dots" | "subtle" => Self::Dot,
            "compact" | "bare" | "seconds" => Self::Compact,
            "symbols" | "unicode" | "technical" => Self::Symbols,
            "emoji" | "emojis" => Self::Emoji,
            "kaomoji" | "kamoji" => Self::Kaomoji,
            "ascii" | "plain" => Self::Ascii,
            "nerdfont" | "nerd" | "nf" => Self::NerdFont,
            "hidden" | "hide" | "off" | "none" => Self::Hidden,
            _ => Self::Classic,
        }
    }
}

/// Visual card / container layout style for inline Agent Tool Execution blocks in the timeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ToolCardStyle {
    /// Classic minimal tree (`• Bash cmd` / `  │ out` / `  └ ✓ completed (48ms)`) (Default)
    #[default]
    Classic,
    /// Sleek rounded card frame (`╭─ Bash cmd ─── ✓ 48ms ─╮` / `│ out │` / `╰──────╯`)
    RoundedCard,
    /// Sharp architectural square box (`┌─ Bash cmd ─── ✓ 48ms ─┐` / `│ out │` / `└──────┘`)
    SquareCard,
    /// Elevated left accent spine card (`┃ Bash cmd` on tinted surface with quiet status pill)
    LeftRail,
    /// OpenCode-style split vertical accent bars (`┃ … ┃`) on elevated surface
    SplitBar,
    /// Oh My Posh tinted header pill (` Bash ` + command + crisp `▎` body gutter + status pill)
    PoshPill,
    /// Oh My Posh rounded capsule header (`Bash` + command + clean `│` body + `✓ 48ms`)
    PoshBubbles,
    /// Ultra-dense compact inline block (header + inline `✓ 48ms` badge + clean indented output)
    Compact,
    /// Minimalist hairline rule (`── Bash cmd ───────── ✓ 48ms` + borderless indented output)
    Minimal,
    /// Pure CLI bracket telemetry (`[BASH] cmd` / `  > out` / `  [OK · 48ms]`)
    Brackets,
}

#[allow(dead_code)]
impl ToolCardStyle {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Classic => "classic",
            Self::RoundedCard => "rounded_card",
            Self::SquareCard => "square_card",
            Self::LeftRail => "left_rail",
            Self::SplitBar => "split_bar",
            Self::PoshPill => "posh_pill",
            Self::PoshBubbles => "posh_bubbles",
            Self::Compact => "compact",
            Self::Minimal => "minimal",
            Self::Brackets => "brackets",
        }
    }

    #[must_use]
    pub fn from_str_loose(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "rounded_card" | "rounded" | "card" => Self::RoundedCard,
            "square_card" | "square" | "box" => Self::SquareCard,
            "left_rail" | "rail" | "left_bar" => Self::LeftRail,
            "split_bar" | "split" | "opencode" => Self::SplitBar,
            "posh_pill" | "pill" | "pills" | "posh" => Self::PoshPill,
            "posh_bubbles" | "bubbles" | "capsule" => Self::PoshBubbles,
            "compact" | "dense" | "inline" => Self::Compact,
            "minimal" | "hairline" | "rule" => Self::Minimal,
            "brackets" | "bracket" | "cli" | "ascii" => Self::Brackets,
            _ => Self::Classic,
        }
    }
}

/// Icon & indicator pack for inline Agent Tool Execution blocks, Thought headers, and Approvals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ToolIconStyle {
    /// Clean minimal dot (`•` / `✓` / `✗`) (Default)
    #[default]
    Minimal,
    /// Universal Unicode Technical Symbols (`❯`, `◎`, `✎`, `⚙`, `⌖`, `⎇`, `❖`, `✦`, `◈`)
    Symbols,
    /// Strict 7-bit ASCII operators (`$`, `@`, `+`, `~`, `?`, `#`, `&`, `*`, `[OK]`, `[ERR]`)
    Ascii,
    /// Expressive Emoji (`⚡`, `🌐`, `📝`, `🔧`, `📖`, `🔍`, `🧭`, `🤖`, `🧠`, `✅`, `❌`)
    Emoji,
    /// Expressive Kaomoji (`(ง°_°)ง`, `( °o°)`, `( ▸_▸)◈`, `(⌐▪_▪)`, `( ◉_◉)`, `(•‿•)`, `(×_×)`)
    Kaomoji,
    /// Collision-free Nerd Font Codicons (`\u{ea85}`, `\u{eb01}`, `\u{ea73}`, `\u{ea6d}`, `\u{ea68}`)
    NerdFont,
    /// Pure typography with zero leading bullet or icon
    None,
}

#[allow(dead_code)]
impl ToolIconStyle {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Minimal => "minimal",
            Self::Symbols => "symbols",
            Self::Ascii => "ascii",
            Self::Emoji => "emoji",
            Self::Kaomoji => "kaomoji",
            Self::NerdFont => "nerdfont",
            Self::None => "none",
        }
    }

    #[must_use]
    pub fn from_str_loose(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "symbols" | "symbol" | "unicode" | "technical" | "alt" | "icons" => Self::Symbols,
            "ascii" | "plain" | "retro" => Self::Ascii,
            "emoji" | "emojis" => Self::Emoji,
            "kaomoji" | "kamoji" | "faces" => Self::Kaomoji,
            "nerdfont" | "nerd" | "nf" | "codicons" => Self::NerdFont,
            "none" | "off" | "text" | "hidden" => Self::None,
            _ => Self::Minimal,
        }
    }
}

/// Top divider & open/enclosed frame container style for Assistant Markdown output blocks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum OutputFrameStyle {
    /// Clean borderless stream without a top divider or frame (Default)
    #[default]
    None,
    /// Open-sided rounded bracket frame (`╭─ ✦ minicode ───╮` / `  body` / `╰───╯`) — zero side walls for clean copy-paste
    OpenRounded,
    /// Label-free open-sided rounded bracket (`╭──────╮` / `  body` / `╰──────╯`)
    OpenRoundedMinimal,
    /// Open-sided rounded frame with auto-detecting Nerd Font icon (`╭─ <nf> minicode ───╮` / `╰───╯`)
    OpenRoundedNerd,
    /// Open-sided rounded frame with expressive emoji badge (`╭─ ✨ minicode ───╮` / `╰───╯`)
    OpenRoundedEmoji,
    /// Sharp 90-degree open architectural bracket (`┌─ minicode ───┐` / `  body` / `└──────┘`)
    OpenSquare,
    /// Bold double-line open bracket (`╔═ minicode ═══╗` / `  body` / `╚══════╝`)
    OpenDouble,
    /// Open rounded top header cap only (`╭─ ✦ minicode ───╮` / `  body`, no bottom rule)
    OpenTopOnly,
    /// Full enclosed rounded card (`╭─ ✦ minicode ───╮` / `│ body │` / `╰───╯`)
    FullCard,
    /// Continuous left accent spine (`┃ ✦ minicode` / `┃ body`)
    LeftRail,
    /// Full-width hairline top divider (`────────────────────────────────────────`)
    FullRule,
    /// Balanced half-width hairline top divider (`────────────────────────`)
    MediumRule,
    /// Minimal compact 14-column top hairline (`──────────────`)
    ShortRule,
    /// Labeled top hairline (`── minicode ────────────────────────────────`)
    LabeledRule,
    /// Smooth theme-colored gradient fade top rule (`━━━━━━━━───────┈┈┈┈┈┈`)
    GradientRule,
    /// Oh My Posh rounded capsule pill + trailing hairline (`✦ minicode ────`)
    PoshPill,
    /// Oh My Posh angled powerline ribbon + trailing hairline (` ✦ minicode  ───`)
    PoshPowerline,
    /// Full-width dashed hairline top divider (`╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌`)
    DashedRule,
    /// Editorial full-width dotted hairline top divider (`┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈`)
    DottedRule,
    /// Pure 7-bit ASCII open frame (`+-- minicode ------+` / `  body` / `+------------------+`)
    AsciiFrame,
}

#[allow(dead_code)]
impl OutputFrameStyle {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::OpenRounded => "open_rounded",
            Self::OpenRoundedMinimal => "open_rounded_minimal",
            Self::OpenRoundedNerd => "open_rounded_nerd",
            Self::OpenRoundedEmoji => "open_rounded_emoji",
            Self::OpenSquare => "open_square",
            Self::OpenDouble => "open_double",
            Self::OpenTopOnly => "open_top_only",
            Self::FullCard => "full_card",
            Self::LeftRail => "left_rail",
            Self::FullRule => "full_rule",
            Self::MediumRule => "medium_rule",
            Self::ShortRule => "short_rule",
            Self::LabeledRule => "labeled_rule",
            Self::GradientRule => "gradient_rule",
            Self::PoshPill => "posh_pill",
            Self::PoshPowerline => "posh_powerline",
            Self::DashedRule => "dashed_rule",
            Self::DottedRule => "dotted_rule",
            Self::AsciiFrame => "ascii_frame",
        }
    }

    #[must_use]
    pub fn from_str_loose(s: &str) -> Self {
        match s.trim().to_lowercase().replace('-', "_").as_str() {
            "open_rounded" | "open_bracket" | "bracket" | "rounded" | "frame" | "catppuccin" => {
                Self::OpenRounded
            }
            "open_rounded_minimal" | "open_minimal" | "rounded_minimal" => Self::OpenRoundedMinimal,
            "open_rounded_nerd" | "open_nerd" | "nerd_frame" | "nerdfont" | "nerd" => {
                Self::OpenRoundedNerd
            }
            "open_rounded_emoji" | "open_emoji" | "emoji_frame" | "emoji" => Self::OpenRoundedEmoji,
            "open_square" | "square" => Self::OpenSquare,
            "open_double" | "double" => Self::OpenDouble,
            "open_top_only" | "top_only" | "open_top" | "cap" => Self::OpenTopOnly,
            "full_card" | "card" | "box" => Self::FullCard,
            "left_rail" | "rail" | "spine" => Self::LeftRail,
            "full_rule" | "full" | "line" | "hairline" => Self::FullRule,
            "medium_rule" | "medium" | "half" => Self::MediumRule,
            "short_rule" | "short" | "minimal" | "compact" => Self::ShortRule,
            "labeled_rule" | "labeled" | "label" => Self::LabeledRule,
            "gradient_rule" | "gradient" | "fade" => Self::GradientRule,
            "posh_pill" | "pill" | "posh" => Self::PoshPill,
            "posh_powerline" | "powerline" | "ribbon" => Self::PoshPowerline,
            "dashed_rule" | "dashed" => Self::DashedRule,
            "dotted_rule" | "dotted" => Self::DottedRule,
            "ascii_frame" | "ascii" | "plain" => Self::AsciiFrame,
            _ => Self::None,
        }
    }
}

/// Markdown presentation preset for Assistant prose, headings, callouts, checklists, code & tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum OutputStyle {
    /// Current minicode Aura Markdown presentation (with callouts & checklists) (Default)
    #[default]
    Default,
    /// Minimal Linear / Vercel editorial style: quiet hairlines, borderless code rail, booktabs tables
    Minimal,
    /// Publication report (`Report-Skill` inspired): double-underlined H1, thick callout strips, booktabs numeric tables
    Editorial,
    /// Workspace document (Notion / Best-README inspired): rounded code cards, rounded tables, roadmap checklists
    Workspace,
    /// Oh My Posh: rounded pill H1, powerline H2, capsule code tags, tinted callout badges
    Posh,
    /// Nerd Font Codicons on headings, code headers, callouts, checklists & bullets (auto zero-box fallback)
    NerdFont,
    /// Universal 1-cell Unicode technical symbols (`◈`, `▸`, `✦`, `⌘`, `✓`, `○`) — zero boxes on any font
    Symbols,
    /// Expressive Emoji markers (`📌`, `💻`, `💡`, `⚠️`, `✅`, `⬜`)
    Emoji,
    /// Playful Kaomoji accents on headings & callouts with clean code/tables
    Kaomoji,
    /// Strict 7-bit ASCII (`===`, `---`, `+---+`, `|`, `[x]/[ ]`, `> [NOTE]`)
    Ascii,
}

#[allow(dead_code)]
impl OutputStyle {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Minimal => "minimal",
            Self::Editorial => "editorial",
            Self::Workspace => "workspace",
            Self::Posh => "posh",
            Self::NerdFont => "nerdfont",
            Self::Symbols => "symbols",
            Self::Emoji => "emoji",
            Self::Kaomoji => "kaomoji",
            Self::Ascii => "ascii",
        }
    }

    #[must_use]
    pub fn from_str_loose(s: &str) -> Self {
        match s.trim().to_lowercase().replace('-', "_").as_str() {
            "minimal" | "linear" | "vercel" | "clean" => Self::Minimal,
            "editorial" | "report" | "report_skill" | "publication" => Self::Editorial,
            "workspace" | "notion" | "readme" | "doc" | "docs" => Self::Workspace,
            "posh" | "oh_my_posh" | "ohmyposh" | "powerline" | "pill" => Self::Posh,
            "nerdfont" | "nerd" | "nf" | "codicons" => Self::NerdFont,
            "symbols" | "unicode" | "technical" => Self::Symbols,
            "emoji" | "emojis" => Self::Emoji,
            "kaomoji" | "kamoji" => Self::Kaomoji,
            "ascii" | "plain" | "retro" => Self::Ascii,
            _ => Self::Default,
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

    #[serde(default = "default_input_box_style")]
    pub input_box_style: String,

    #[serde(default = "default_status_bar_style")]
    pub status_bar_style: String,

    #[serde(default = "default_inherit_style")]
    pub status_model_style: String,

    #[serde(default = "default_inherit_style")]
    pub status_effort_style: String,

    #[serde(default = "default_inherit_style")]
    pub status_path_style: String,

    #[serde(default = "default_inherit_style")]
    pub status_git_style: String,

    #[serde(default = "default_inherit_style")]
    pub status_badges_style: String,

    #[serde(default = "default_context_meter_style")]
    pub context_meter_style: String,

    #[serde(default = "default_mode_bar_style")]
    pub mode_bar_style: String,

    #[serde(default = "default_activity_hint_style")]
    pub activity_hint_style: String,

    #[serde(default = "default_tool_card_style")]
    pub tool_card_style: String,

    #[serde(default = "default_tool_icon_style")]
    pub tool_icon_style: String,

    #[serde(default = "default_output_frame_style")]
    pub output_frame_style: String,

    #[serde(default = "default_output_style")]
    pub output_style: String,

    #[serde(default)]
    pub plain: bool,

    #[serde(default = "default_max_width")]
    pub max_width: usize,

    #[serde(default)]
    pub show_cost: bool,

    #[serde(default = "default_true")]
    pub show_ponytail_badge: bool,

    #[serde(default = "default_true")]
    pub show_caveman_badge: bool,

    #[serde(default = "default_true")]
    pub show_streaming_badge: bool,

    #[serde(default = "default_true")]
    pub show_kv_cache: bool,

    #[serde(default = "default_true")]
    pub show_context_meter: bool,

    #[serde(default = "default_true")]
    pub show_status_model: bool,

    #[serde(default = "default_true")]
    pub show_status_effort: bool,

    #[serde(default = "default_true")]
    pub show_status_path: bool,

    #[serde(default = "default_true")]
    pub show_status_git: bool,

    #[serde(default = "default_true")]
    pub show_status_badges: bool,

    #[serde(default = "default_true")]
    pub show_interrupt_hint: bool,

    #[serde(default)]
    pub show_thought_tokens: bool,

    /// Force raw Nerd Font Private Use Area (PUA) Codicon glyphs (`true`) instead of auto-detecting
    /// terminal Nerd Font support with zero-box standard Unicode fallback (`false`, default).
    #[serde(default)]
    pub nerd_font_pua: bool,
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

    #[must_use]
    pub fn input_box_style_enum(&self) -> InputBoxStyle {
        InputBoxStyle::from_str_loose(&self.input_box_style)
    }

    #[must_use]
    pub fn status_bar_style_enum(&self) -> StatusBarStyle {
        StatusBarStyle::from_str_loose(&self.status_bar_style)
    }

    #[must_use]
    pub fn resolve_status_component_style(raw: &str) -> Option<StatusBarStyle> {
        let trimmed = raw.trim();
        if trimmed.is_empty()
            || trimmed.eq_ignore_ascii_case("inherit")
            || trimmed.eq_ignore_ascii_case("default")
        {
            None
        } else {
            Some(StatusBarStyle::from_str_loose(trimmed))
        }
    }

    #[must_use]
    pub fn status_model_style_override(&self) -> Option<StatusBarStyle> {
        Self::resolve_status_component_style(&self.status_model_style)
    }

    #[must_use]
    pub fn status_effort_style_override(&self) -> Option<StatusBarStyle> {
        Self::resolve_status_component_style(&self.status_effort_style)
    }

    #[must_use]
    pub fn status_path_style_override(&self) -> Option<StatusBarStyle> {
        Self::resolve_status_component_style(&self.status_path_style)
    }

    #[must_use]
    pub fn status_git_style_override(&self) -> Option<StatusBarStyle> {
        Self::resolve_status_component_style(&self.status_git_style)
    }

    #[must_use]
    pub fn status_badges_style_override(&self) -> Option<StatusBarStyle> {
        Self::resolve_status_component_style(&self.status_badges_style)
    }

    #[must_use]
    pub fn context_meter_style_enum(&self) -> ContextMeterStyle {
        ContextMeterStyle::from_str_loose(&self.context_meter_style)
    }

    #[must_use]
    pub fn mode_bar_style_enum(&self) -> ModeBarStyle {
        ModeBarStyle::from_str_loose(&self.mode_bar_style)
    }

    #[must_use]
    pub fn activity_hint_style_enum(&self) -> ActivityHintStyle {
        ActivityHintStyle::from_str_loose(&self.activity_hint_style)
    }

    #[must_use]
    pub fn tool_card_style_enum(&self) -> ToolCardStyle {
        ToolCardStyle::from_str_loose(&self.tool_card_style)
    }

    #[must_use]
    pub fn tool_icon_style_enum(&self) -> ToolIconStyle {
        ToolIconStyle::from_str_loose(&self.tool_icon_style)
    }

    #[must_use]
    pub fn output_frame_style_enum(&self) -> OutputFrameStyle {
        OutputFrameStyle::from_str_loose(&self.output_frame_style)
    }

    #[must_use]
    pub fn output_style_enum(&self) -> OutputStyle {
        OutputStyle::from_str_loose(&self.output_style)
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
            input_box_style: default_input_box_style(),
            status_bar_style: default_status_bar_style(),
            status_model_style: default_inherit_style(),
            status_effort_style: default_inherit_style(),
            status_path_style: default_inherit_style(),
            status_git_style: default_inherit_style(),
            status_badges_style: default_inherit_style(),
            context_meter_style: default_context_meter_style(),
            mode_bar_style: default_mode_bar_style(),
            activity_hint_style: default_activity_hint_style(),
            tool_card_style: default_tool_card_style(),
            tool_icon_style: default_tool_icon_style(),
            output_frame_style: default_output_frame_style(),
            output_style: default_output_style(),
            plain: false,
            max_width: default_max_width(),
            show_cost: false,
            show_ponytail_badge: true,
            show_caveman_badge: true,
            show_streaming_badge: true,
            show_kv_cache: true,
            show_context_meter: true,
            show_status_model: true,
            show_status_effort: true,
            show_status_path: true,
            show_status_git: true,
            show_status_badges: true,
            show_interrupt_hint: true,
            show_thought_tokens: false,
            nerd_font_pua: false,
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

fn default_input_box_style() -> String {
    "rounded".to_string()
}

fn default_status_bar_style() -> String {
    "classic".to_string()
}

fn default_inherit_style() -> String {
    "inherit".to_string()
}

fn default_context_meter_style() -> String {
    "compact".to_string()
}

fn default_mode_bar_style() -> String {
    "classic".to_string()
}

fn default_activity_hint_style() -> String {
    "classic".to_string()
}

fn default_tool_card_style() -> String {
    "classic".to_string()
}

fn default_tool_icon_style() -> String {
    "minimal".to_string()
}

fn default_output_frame_style() -> String {
    "none".to_string()
}

fn default_output_style() -> String {
    "default".to_string()
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
    /// Ordered engine priority list for headless automation (default: ["minibrowse", "chrome", "firefox", "safari", "obscura"])
    #[serde(default = "default_headless_priority")]
    pub headless_priority: Vec<String>,

    /// Ordered engine priority list for GUI windowed automation (default: ["minibrowse", "chrome", "firefox", "safari", "obscura"])
    #[serde(default = "default_gui_priority")]
    pub gui_priority: Vec<String>,

    /// Explicit override engine for general browsing (minibrowse | chrome | firefox | safari | obscura).
    /// When `None`, `minibrowse` is used as primary by default.
    #[serde(default)]
    pub default_engine: Option<String>,

    /// Dedicated engine used for Web, CSS/DOM, QA, and Flutter/Dart VM debugging (default: "minibrowse").
    #[serde(default = "default_debug_engine")]
    pub debug_engine: String,
}

fn default_debug_engine() -> String {
    "minibrowse".to_string()
}

fn default_headless_priority() -> Vec<String> {
    vec![
        "minibrowse".to_string(),
        "chrome".to_string(),
        "firefox".to_string(),
        "safari".to_string(),
        "obscura".to_string(),
    ]
}

fn default_gui_priority() -> Vec<String> {
    vec![
        "minibrowse".to_string(),
        "chrome".to_string(),
        "firefox".to_string(),
        "safari".to_string(),
        "obscura".to_string(),
    ]
}

impl Default for BrowserSettingsConfig {
    fn default() -> Self {
        Self {
            headless_priority: default_headless_priority(),
            gui_priority: default_gui_priority(),
            default_engine: None,
            debug_engine: default_debug_engine(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RawBrowserConfig {
    pub headless_priority: Option<Vec<String>>,
    pub gui_priority: Option<Vec<String>>,
    pub default_engine: Option<String>,
    pub debug_engine: Option<String>,
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

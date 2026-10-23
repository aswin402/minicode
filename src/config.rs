use crate::constants::env_vars;
use crate::error::{ConfigError, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
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
static RUNTIME_TIMEOUTS_OVERRIDDEN: std::sync::atomic::AtomicBool =
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
        }
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
            minipub: MiniPubConfig::default(),
        }
    }
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
}

impl TodoWidgetStyle {
    #[allow(dead_code)]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Tree => "tree",
            Self::Card => "card",
            Self::Rail => "rail",
            Self::Minimal => "minimal",
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
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            theme: default_theme(),
            animation: default_animation(),
            todo_style: default_todo_style(),
            swarm_style: default_swarm_style(),
            toast_style: default_toast_style(),
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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct McpJsonFile {
    #[serde(default, rename = "mcpServers")]
    pub mcp_servers: std::collections::HashMap<String, McpServerConfig>,
    #[serde(default)]
    pub servers: std::collections::HashMap<String, McpServerConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RawGitConfig {
    pub auto_commit: Option<bool>,
    pub dirty_commit: Option<bool>,
    pub ai_commit_messages: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RawProviderConfig {
    pub default: Option<String>,
    pub model: Option<String>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<usize>,
    pub ollama: Option<OllamaConfig>,
    pub api_keys: Option<std::collections::HashMap<String, String>>,
    pub custom_endpoints: Option<std::collections::HashMap<String, String>>,
    pub context_window: Option<usize>,
    pub prompt_cost_per_m: Option<f64>,
    pub completion_cost_per_m: Option<f64>,
    pub thinking_budget: Option<usize>,
    pub reasoning_effort: Option<String>,
    pub default_models: Option<std::collections::HashMap<String, String>>,
    pub fallback_1: Option<ModelSlot>,
    pub fallback_2: Option<ModelSlot>,
    pub global_last_fallback: Option<ModelSlot>,
    pub favorite_models: Option<Vec<ModelSlot>>,
    pub connect_timeout_secs: Option<u64>,
    pub stream_idle_timeout_secs: Option<u64>,
    pub adaptive_timeout: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RawMiniPubConfig {
    pub max_parallel: Option<usize>,
    pub default_token_budget: Option<usize>,
    pub default_max_turns: Option<usize>,
    pub inactivity_timeout_secs: Option<u64>,
    pub worktree_isolation: Option<String>,
    pub extra_allowed_tools: Option<Vec<String>>,
    pub sync_with_orchestrator: Option<bool>,
    pub default_chain: Option<MiniPubModelChain>,
    pub role_models: Option<std::collections::HashMap<String, MiniPubModelChain>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RawAgentConfig {
    pub auto_approve: Option<bool>,
    pub approval_policy: Option<String>,
    pub timeout: Option<u64>,
    pub map_tokens: Option<usize>,
    pub warning_threshold: Option<f32>,
    pub auto_heal: Option<bool>,
    pub streaming: Option<bool>,
    pub caveman_mode: Option<bool>,
    pub caveman_level: Option<String>,
    pub tool_mode: Option<ToolFilterMode>,
    pub syntax_barrier: Option<bool>,
    pub auto_lint: Option<bool>,
    pub parallel_tools: Option<bool>,
    pub speculative_execution: Option<bool>,
    pub max_parallel_tools: Option<usize>,
    pub compact_tool_schemas: Option<bool>,
    pub max_tool_iterations: Option<usize>,
    pub auto_continue: Option<bool>,
    pub max_auto_continues: Option<usize>,
    pub intent: Option<IntentConfig>,
    pub plan_approval: Option<bool>,
    #[serde(alias = "subagent", alias = "subagents")]
    pub minipub: Option<RawMiniPubConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RawUiConfig {
    pub plain: Option<bool>,
    pub theme: Option<String>,
    pub animation: Option<String>,
    pub todo_style: Option<String>,
    pub swarm_style: Option<String>,
    pub toast_style: Option<String>,
    pub max_width: Option<usize>,
    pub show_cost: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RawLoggingConfig {
    pub level: Option<String>,
    pub file: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RawConfig {
    #[serde(default)]
    pub provider: RawProviderConfig,
    #[serde(default)]
    pub agent: RawAgentConfig,
    #[serde(default)]
    pub ui: RawUiConfig,
    #[serde(default)]
    pub logging: RawLoggingConfig,
    #[serde(default)]
    pub mcp: McpConfig,
    #[serde(default)]
    pub git: RawGitConfig,
    #[serde(default)]
    pub browser: RawBrowserConfig,
}

impl Config {
    /// Synchronizes configured provider and command execution timeouts into the process-wide runtime atomics.
    pub fn sync_runtime_timeouts(&self) {
        if !RUNTIME_TIMEOUTS_OVERRIDDEN.load(std::sync::atomic::Ordering::Relaxed) {
            set_active_provider_timeouts(
                self.provider.effective_connect_timeout_secs(),
                self.provider.effective_stream_idle_timeout_secs(),
            );
        }
        crate::tools::exec::set_configured_exec_timeout(self.agent.timeout);
    }

    /// Convenience helper that loads configuration for `workspace_dir` or falls back to defaults.
    pub fn load_for_workspace(workspace_dir: &Path) -> Self {
        Self::load(Some(workspace_dir), None).unwrap_or_default()
    }

    /// Loads configuration respecting the hierarchy:
    /// 1. Project-local `.minicode/config.toml` (if present)
    /// 2. Global `~/.config/minicode/config.toml` (if present)
    /// 3. Project-local `mcp.json` or `.minicode/mcp.json` (if present)
    /// 4. Built-in defaults
    /// 5. Environment variable overrides (MINICODE_*)
    pub fn load(workspace_dir: Option<&Path>, custom_config_path: Option<&Path>) -> Result<Self> {
        // 0. Load global ~/.config/minicode/.env if present
        if let Some(global_dir) = dirs::config_dir() {
            let global_env = global_dir
                .join(crate::constants::CONFIG_DIR_NAME)
                .join(crate::constants::ENV_FILE_NAME);
            match dotenvy::from_path(&global_env) {
                Ok(_) => {}
                Err(dotenvy::Error::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => {
                    tracing::warn!(path = %global_env.display(), error = %e, "Failed to parse global .env file");
                }
            }
        }

        // 1. Load workspace .env if present
        if let Some(dir) = workspace_dir {
            let env_file = dir.join(crate::constants::ENV_FILE_NAME);
            match dotenvy::from_path(&env_file) {
                Ok(_) => {}
                Err(dotenvy::Error::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => {
                    tracing::warn!(path = %env_file.display(), error = %e, "Failed to parse workspace .env file");
                }
            }
        }
        if workspace_dir.is_none() {
            if let Err(e) = dotenvy::dotenv() {
                if !matches!(e, dotenvy::Error::Io(ref io_err) if io_err.kind() == std::io::ErrorKind::NotFound)
                {
                    tracing::warn!(error = %e, "Failed to parse default .env file");
                }
            }
        }

        let mut config = Config::default();

        // 1. Try custom config path if explicitly specified
        if let Some(path) = custom_config_path {
            if let Some(raw) = Self::load_raw_from_file(path)? {
                config.merge_raw(raw);
            }
        } else {
            // 2. Global config: ~/.config/minicode/config.toml
            if let Some(global_dir) = dirs::config_dir() {
                let global_config = global_dir
                    .join(crate::constants::CONFIG_DIR_NAME)
                    .join(crate::constants::CONFIG_FILE_NAME);
                if let Some(raw) = Self::load_raw_from_file(&global_config)? {
                    config.merge_raw(raw);
                }
            }

            // 3. Project-local config: <workspace>/.minicode/config.toml
            if let Some(dir) = workspace_dir {
                let local_config = dir
                    .join(crate::constants::WORKSPACE_DIR_NAME)
                    .join(crate::constants::CONFIG_FILE_NAME);
                if let Some(raw) = Self::load_raw_from_file(&local_config)? {
                    config.merge_raw(raw);
                }
            }
        }

        // 4. Always load global and workspace MCP configs
        if let Some(global_dir) = dirs::config_dir() {
            config.load_mcp_json_if_exists(
                &global_dir
                    .join(crate::constants::CONFIG_DIR_NAME)
                    .join(crate::constants::MCP_CONFIG_FILE),
            );
        }

        if let Some(dir) = workspace_dir {
            config.load_mcp_json_if_exists(
                &dir.join(crate::constants::WORKSPACE_DIR_NAME)
                    .join(crate::constants::MCP_CONFIG_FILE),
            );
            config.load_mcp_json_if_exists(&dir.join(crate::constants::MCP_CONFIG_FILE));
        }

        // 5. Apply environment variable overrides
        config.apply_env_overrides();

        // 6. Dynamic 6-tier provider resolution
        let (resolved_provider, resolved_model) =
            config.resolve_active_provider_and_model(workspace_dir);
        config.provider.default = resolved_provider;
        config.provider.model = resolved_model;

        // 7. Sync runtime timeouts for providers and command execution
        config.sync_runtime_timeouts();

        Ok(config)
    }

    /// Saves the current configuration to disk:
    /// If project-local `.minicode/config.toml` exists, updates it.
    /// Otherwise writes to global `~/.config/minicode/config.toml`.
    pub fn save(&self, workspace_root: Option<&Path>) -> Result<()> {
        let path = if let Some(ws) = workspace_root {
            let ws_minicode_dir = ws.join(crate::constants::WORKSPACE_DIR_NAME);
            let local_path = ws_minicode_dir.join(crate::constants::CONFIG_FILE_NAME);
            if local_path.exists() || ws_minicode_dir.exists() {
                local_path
            } else if let Some(global_dir) = dirs::config_dir() {
                global_dir
                    .join(crate::constants::CONFIG_DIR_NAME)
                    .join(crate::constants::CONFIG_FILE_NAME)
            } else {
                local_path
            }
        } else if let Some(global_dir) = dirs::config_dir() {
            global_dir
                .join(crate::constants::CONFIG_DIR_NAME)
                .join(crate::constants::CONFIG_FILE_NAME)
        } else {
            return Ok(());
        };

        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let toml_str = toml::to_string_pretty(self).map_err(ConfigError::TomlSerialize)?;
        std::fs::write(&path, toml_str)?;
        tracing::info!(path = %path.display(), "Configuration saved to disk");
        Ok(())
    }

    fn load_mcp_json_if_exists(&mut self, path: &Path) {
        match std::fs::read_to_string(path) {
            Ok(content) => match serde_json::from_str::<McpJsonFile>(&content) {
                Ok(parsed) => {
                    for (name, server) in parsed.mcp_servers {
                        if let Err(err) = server.validate(&name) {
                            tracing::warn!(path = %path.display(), error = %err, "Invalid MCP server config; skipping");
                            continue;
                        }
                        self.mcp.servers.insert(name, server);
                    }
                    for (name, server) in parsed.servers {
                        if let Err(err) = server.validate(&name) {
                            tracing::warn!(path = %path.display(), error = %err, "Invalid MCP server config; skipping");
                            continue;
                        }
                        self.mcp.servers.insert(name, server);
                    }
                }
                Err(e) => {
                    tracing::warn!(path = %path.display(), error = %e, "Failed to parse mcp.json");
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                tracing::warn!(path = %path.display(), error = %e, "Failed to read mcp.json");
            }
        }
    }

    fn load_raw_from_file(path: &Path) -> Result<Option<RawConfig>> {
        match std::fs::read_to_string(path) {
            Ok(content) => {
                let parsed: RawConfig = toml::from_str(&content).map_err(ConfigError::TomlParse)?;
                tracing::debug!(path = %path.display(), "Loaded configuration from file");
                Ok(Some(parsed))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(ConfigError::FileRead {
                path: path.display().to_string(),
                source: e,
            }
            .into()),
        }
    }

    pub fn merge_raw(&mut self, other: RawConfig) {
        if let Some(default) = other.provider.default {
            self.provider.default = default;
        }
        if let Some(model) = other.provider.model {
            self.provider.model = model;
        }
        if let Some(temperature) = other.provider.temperature {
            self.provider.temperature = temperature;
        }
        if let Some(max_tokens) = other.provider.max_tokens {
            self.provider.max_tokens = max_tokens;
        }
        if let Some(ollama) = other.provider.ollama {
            self.provider.ollama = ollama;
        }
        if let Some(context_window) = other.provider.context_window {
            self.provider.context_window = Some(context_window);
        }
        if let Some(prompt_cost) = other.provider.prompt_cost_per_m {
            self.provider.prompt_cost_per_m = Some(prompt_cost);
        }
        if let Some(comp_cost) = other.provider.completion_cost_per_m {
            self.provider.completion_cost_per_m = Some(comp_cost);
        }
        if let Some(thinking_budget) = other.provider.thinking_budget {
            self.provider.thinking_budget = Some(thinking_budget);
        }
        if let Some(reasoning_effort) = other.provider.reasoning_effort {
            let trimmed = reasoning_effort.trim().to_string();
            self.provider.reasoning_effort = (!trimmed.is_empty()).then_some(trimmed);
        }
        if let Some(f1) = other.provider.fallback_1 {
            self.provider.fallback_1 = f1.is_valid().then_some(f1);
        }
        if let Some(f2) = other.provider.fallback_2 {
            self.provider.fallback_2 = f2.is_valid().then_some(f2);
        }
        if let Some(glf) = other.provider.global_last_fallback {
            self.provider.global_last_fallback = glf.is_valid().then_some(glf);
        }
        if let Some(favs) = other.provider.favorite_models {
            for fav in favs {
                if fav.is_valid()
                    && !self.provider.favorite_models.iter().any(|existing| {
                        existing.provider.eq_ignore_ascii_case(&fav.provider)
                            && existing.model.eq_ignore_ascii_case(&fav.model)
                    })
                {
                    self.provider.favorite_models.push(fav);
                }
            }
        }
        if let Some(connect_timeout) = other.provider.connect_timeout_secs {
            self.provider.connect_timeout_secs = Some(connect_timeout.max(5));
        }
        if let Some(stream_idle) = other.provider.stream_idle_timeout_secs {
            self.provider.stream_idle_timeout_secs = Some(stream_idle.max(15));
        }
        if let Some(adaptive) = other.provider.adaptive_timeout {
            self.provider.adaptive_timeout = adaptive;
        }
        if let Some(auto_approve) = other.agent.auto_approve {
            self.agent.auto_approve = auto_approve;
        }
        if let Some(approval_policy) = other.agent.approval_policy {
            self.agent.approval_policy = approval_policy;
        }
        if let Some(timeout) = other.agent.timeout {
            self.agent.timeout = timeout;
        }
        if let Some(map_tokens) = other.agent.map_tokens {
            self.agent.map_tokens = map_tokens;
        }
        if let Some(warning_threshold) = other.agent.warning_threshold {
            self.agent.warning_threshold = warning_threshold;
        }
        if let Some(auto_heal) = other.agent.auto_heal {
            self.agent.auto_heal = auto_heal;
        }
        if let Some(streaming) = other.agent.streaming {
            self.agent.streaming = streaming;
        }
        if let Some(caveman_mode) = other.agent.caveman_mode {
            self.agent.caveman_mode = caveman_mode;
        }
        if let Some(caveman_level) = other.agent.caveman_level {
            if !caveman_level.trim().is_empty() {
                self.agent.caveman_level = caveman_level.trim().to_lowercase();
            }
        }
        if let Some(tool_mode) = other.agent.tool_mode {
            self.agent.tool_mode = tool_mode;
        }
        if let Some(syntax_barrier) = other.agent.syntax_barrier {
            self.agent.syntax_barrier = syntax_barrier;
        }
        if let Some(auto_lint) = other.agent.auto_lint {
            self.agent.auto_lint = auto_lint;
        }
        if let Some(parallel_tools) = other.agent.parallel_tools {
            self.agent.parallel_tools = parallel_tools;
        }
        if let Some(speculative_execution) = other.agent.speculative_execution {
            self.agent.speculative_execution = speculative_execution;
        }
        if let Some(max_parallel_tools) = other.agent.max_parallel_tools {
            self.agent.max_parallel_tools = max_parallel_tools.clamp(
                crate::constants::MIN_PARALLEL_TOOLS,
                crate::constants::MAX_PARALLEL_TOOLS_CAP,
            );
        }
        if let Some(compact_tool_schemas) = other.agent.compact_tool_schemas {
            self.agent.compact_tool_schemas = compact_tool_schemas;
        }
        if let Some(max_iter) = other.agent.max_tool_iterations {
            self.agent.max_tool_iterations = max_iter;
        }
        if let Some(ac) = other.agent.auto_continue {
            self.agent.auto_continue = ac;
        }
        if let Some(mac) = other.agent.max_auto_continues {
            self.agent.max_auto_continues = mac;
        }
        if let Some(intent) = other.agent.intent {
            self.agent.intent = intent;
        }
        if let Some(plan_approval) = other.agent.plan_approval {
            self.agent.plan_approval = plan_approval;
        }
        if let Some(mp) = other.agent.minipub {
            if let Some(max_parallel) = mp.max_parallel {
                self.agent.minipub.max_parallel = max_parallel.clamp(
                    crate::constants::MIN_PARALLEL_MINIPUBS,
                    crate::constants::MAX_PARALLEL_MINIPUBS_CAP,
                );
            }
            if let Some(budget) = mp.default_token_budget {
                self.agent.minipub.default_token_budget = budget.max(4_000);
            }
            if let Some(turns) = mp.default_max_turns {
                self.agent.minipub.default_max_turns = turns.clamp(1, 100);
            }
            if let Some(inact) = mp.inactivity_timeout_secs {
                self.agent.minipub.inactivity_timeout_secs = inact.max(10);
            }
            if let Some(iso) = mp.worktree_isolation {
                self.agent.minipub.worktree_isolation = iso;
            }
            if let Some(extra) = mp.extra_allowed_tools {
                self.agent.minipub.extra_allowed_tools = extra;
            }
            if let Some(sync) = mp.sync_with_orchestrator {
                self.agent.minipub.sync_with_orchestrator = sync;
            }
            if let Some(dc) = mp.default_chain {
                if dc.primary.is_some() {
                    self.agent.minipub.default_chain.primary = dc.primary.filter(|s| s.is_valid());
                }
                if dc.fallback_1.is_some() {
                    self.agent.minipub.default_chain.fallback_1 =
                        dc.fallback_1.filter(|s| s.is_valid());
                }
                if dc.fallback_2.is_some() {
                    self.agent.minipub.default_chain.fallback_2 =
                        dc.fallback_2.filter(|s| s.is_valid());
                }
            }
            if let Some(roles) = mp.role_models {
                for (role_key, chain) in roles {
                    let norm_role = role_key.trim().to_lowercase().replace('-', "_");
                    if chain.is_empty() {
                        self.agent.minipub.role_models.remove(&norm_role);
                    } else {
                        self.agent.minipub.role_models.insert(norm_role, chain);
                    }
                }
            }
        }
        if let Some(plain) = other.ui.plain {
            self.ui.plain = plain;
        }
        if let Some(theme) = other.ui.theme {
            self.ui.theme = theme;
        }
        if let Some(animation) = other.ui.animation {
            self.ui.animation = animation;
        }
        if let Some(todo_style) = other.ui.todo_style {
            self.ui.todo_style = todo_style;
        }
        if let Some(swarm_style) = other.ui.swarm_style {
            self.ui.swarm_style = swarm_style;
        }
        if let Some(toast_style) = other.ui.toast_style {
            self.ui.toast_style = toast_style;
        }
        if let Some(max_width) = other.ui.max_width {
            self.ui.max_width = max_width;
        }
        if let Some(show_cost) = other.ui.show_cost {
            self.ui.show_cost = show_cost;
        }
        if let Some(level) = other.logging.level {
            self.logging.level = level;
        }
        if let Some(file) = other.logging.file {
            self.logging.file = file;
        }
        if let Some(auto_commit) = other.git.auto_commit {
            self.git.auto_commit = auto_commit;
        }
        if let Some(dirty_commit) = other.git.dirty_commit {
            self.git.dirty_commit = dirty_commit;
        }
        if let Some(ai_commit_messages) = other.git.ai_commit_messages {
            self.git.ai_commit_messages = ai_commit_messages;
        }
        if let Some(keys) = other.provider.api_keys {
            for (k, v) in keys {
                if !v.trim().is_empty() {
                    self.provider.api_keys.insert(k.to_lowercase(), v);
                }
            }
        }
        if let Some(endpoints) = other.provider.custom_endpoints {
            for (k, v) in endpoints {
                if !v.trim().is_empty() {
                    self.provider.custom_endpoints.insert(k.to_lowercase(), v);
                }
            }
        }
        if let Some(models) = other.provider.default_models {
            for (k, v) in models {
                if !v.trim().is_empty() {
                    self.provider.default_models.insert(k.to_lowercase(), v);
                }
            }
        }
        if let Some(policy) = other.mcp.approval_policy {
            self.mcp.approval_policy = Some(policy);
        }
        for (name, srv) in other.mcp.servers {
            self.mcp.servers.insert(name, srv);
        }
        if let Some(hl) = other.browser.headless_priority {
            self.browser.headless_priority = hl;
        }
        if let Some(gp) = other.browser.gui_priority {
            self.browser.gui_priority = gp;
        }
        if let Some(de) = other.browser.default_engine {
            self.browser.default_engine = Some(de);
        }
    }

    fn env_bool(key: &str) -> Option<bool> {
        std::env::var(key)
            .ok()
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
    }

    fn env_parse<T: std::str::FromStr>(key: &str) -> Option<T> {
        std::env::var(key).ok().and_then(|v| v.parse::<T>().ok())
    }

    fn apply_env_overrides(&mut self) {
        if let Ok(model) = std::env::var(env_vars::MINICODE_MODEL) {
            self.provider.model = model;
        }
        if let Ok(provider) = std::env::var(env_vars::MINICODE_PROVIDER) {
            self.provider.default = provider;
        }
        if let Some(auto_approve) = Self::env_bool(env_vars::MINICODE_AUTO_APPROVE) {
            self.agent.auto_approve = auto_approve;
        }
        if let Ok(policy) = std::env::var(env_vars::MINICODE_APPROVAL_POLICY) {
            self.agent.approval_policy = policy;
        }
        if let Some(temp) = Self::env_parse::<f32>(env_vars::MINICODE_TEMPERATURE) {
            self.provider.temperature = temp;
        }
        if let Some(tokens) = Self::env_parse::<usize>(env_vars::MINICODE_MAX_TOKENS) {
            self.provider.max_tokens = tokens;
        }
        if let Some(timeout) = Self::env_parse::<u64>(env_vars::MINICODE_TIMEOUT) {
            self.agent.timeout = timeout;
        }
        if let Some(conn) = Self::env_parse::<u64>(env_vars::MINICODE_CONNECT_TIMEOUT) {
            self.provider.connect_timeout_secs = Some(conn.max(5));
        }
        if let Some(idle) = Self::env_parse::<u64>(env_vars::MINICODE_STREAM_IDLE_TIMEOUT) {
            self.provider.stream_idle_timeout_secs = Some(idle.max(15));
        }
        if let Some(plain) = Self::env_bool(env_vars::MINICODE_PLAIN) {
            self.ui.plain = plain;
        }
        if let Ok(theme) = std::env::var(env_vars::MINICODE_THEME) {
            self.ui.theme = theme;
        }
        if let Ok(animation) = std::env::var(env_vars::MINICODE_ANIMATION) {
            self.ui.animation = animation;
        }
        if let Ok(todo_style) = std::env::var(env_vars::MINICODE_TODO_STYLE) {
            self.ui.todo_style = todo_style;
        }
        if let Ok(level) = std::env::var(env_vars::MINICODE_LOG_LEVEL) {
            self.logging.level = level;
        }
        if let Some(parallel) = Self::env_bool(env_vars::MINICODE_PARALLEL_TOOLS) {
            self.agent.parallel_tools = parallel;
        }
        if let Some(spec) = Self::env_bool(env_vars::MINICODE_SPECULATIVE_EXECUTION) {
            self.agent.speculative_execution = spec;
        }
        if let Some(val) = Self::env_parse::<usize>(env_vars::MINICODE_MAX_PARALLEL_TOOLS) {
            self.agent.max_parallel_tools = val.clamp(
                crate::constants::MIN_PARALLEL_TOOLS,
                crate::constants::MAX_PARALLEL_TOOLS_CAP,
            );
        }
        if let Some(val) = Self::env_parse::<usize>(env_vars::MINICODE_MAX_MINIPUBS) {
            self.agent.minipub.max_parallel = val.clamp(
                crate::constants::MIN_PARALLEL_MINIPUBS,
                crate::constants::MAX_PARALLEL_MINIPUBS_CAP,
            );
        }
        if let Some(val) = Self::env_parse::<u64>(env_vars::MINICODE_MINIPUB_TIMEOUT) {
            self.agent.minipub.inactivity_timeout_secs = val.max(10);
        }
        if let Some(val) = Self::env_parse::<usize>(env_vars::MINICODE_MINIPUB_TOKEN_BUDGET) {
            self.agent.minipub.default_token_budget = val.max(4_000);
        }
        if let Some(val) = Self::env_parse::<usize>(env_vars::MINICODE_MINIPUB_MAX_TURNS) {
            self.agent.minipub.default_max_turns = val.clamp(1, 100);
        }
        if let Ok(mp_iso) = std::env::var(env_vars::MINICODE_MINIPUB_ISOLATION) {
            self.agent.minipub.worktree_isolation = mp_iso;
        }
        if let Some(compact) = Self::env_bool(env_vars::MINICODE_COMPACT_TOOL_SCHEMAS) {
            self.agent.compact_tool_schemas = compact;
        }
        if let Some(val) = Self::env_parse::<usize>(env_vars::MINICODE_MAX_TOOL_ITERATIONS) {
            self.agent.max_tool_iterations = val;
        }
        if let Some(ac) = Self::env_bool(env_vars::MINICODE_AUTO_CONTINUE) {
            self.agent.auto_continue = ac;
        }
    }

    /// Checks whether a `(provider, model)` pair is saved in `config.provider.favorite_models`.
    #[must_use]
    pub fn is_favorite_model(&self, provider: &str, model: &str) -> bool {
        let p = provider.trim();
        let m = model.trim();
        self.provider
            .favorite_models
            .iter()
            .any(|s| s.provider.eq_ignore_ascii_case(p) && s.model.eq_ignore_ascii_case(m))
    }

    /// Toggles a `(provider, model)` pair in `config.provider.favorite_models`.
    /// Returns `true` if added to favorites, or `false` if removed.
    pub fn toggle_favorite_model(&mut self, provider: &str, model: &str) -> bool {
        let slot = ModelSlot::new(provider, model);
        if !slot.is_valid() {
            return false;
        }
        if let Some(pos) = self.provider.favorite_models.iter().position(|s| {
            s.provider.eq_ignore_ascii_case(&slot.provider)
                && s.model.eq_ignore_ascii_case(&slot.model)
        }) {
            self.provider.favorite_models.remove(pos);
            false
        } else {
            self.provider.favorite_models.push(slot);
            true
        }
    }

    /// Returns true if the provider runs locally (e.g. Ollama, LM Studio, vLLM, LocalAI, or localhost endpoint)
    /// where API keys are strictly optional.
    pub fn is_local_provider(&self, provider_name: &str) -> bool {
        let norm = provider_name.to_lowercase();
        if matches!(
            norm.as_str(),
            "ollama"
                | "localhost"
                | "local"
                | "localai"
                | "lmstudio"
                | "lm-studio"
                | "vllm"
                | "llama.cpp"
                | "llamacpp"
                | "jan"
                | "text-generation-webui"
                | "oobabooga"
        ) {
            return true;
        }

        // Also check if custom_endpoints for this provider points to localhost or a loopback address
        if let Some(endpoint) = self.provider.custom_endpoints.get(&norm) {
            let lower = endpoint.to_lowercase();
            if lower.contains("localhost")
                || lower.contains("127.0.0.1")
                || lower.contains("0.0.0.0")
                || lower.contains("::1")
                || lower.contains("[::1]")
            {
                return true;
            }
        }

        false
    }

    /// Resolves the base URL for a provider, taking into account custom_endpoints,
    /// provider-specific defaults (Ollama, LM Studio, vLLM, etc.), or None.
    pub fn get_provider_base_url(&self, provider_name: &str) -> Option<String> {
        let norm = provider_name.to_lowercase();
        if let Some(url) = self.provider.custom_endpoints.get(&norm) {
            return Some(url.clone());
        }

        match norm.as_str() {
            "ollama" => Some(self.provider.ollama.host.clone()),
            "lmstudio" | "lm-studio" => {
                Some(crate::constants::LMSTUDIO_DEFAULT_BASE_URL.to_string())
            }
            "vllm" => Some(crate::constants::VLLM_DEFAULT_BASE_URL.to_string()),
            "local" | "localhost" | "localai" | "llama.cpp" | "llamacpp" | "jan" => {
                Some(crate::constants::LOCALAI_DEFAULT_BASE_URL.to_string())
            }
            _ => None,
        }
    }

    /// Returns default fallback model for a provider from the static catalog
    #[allow(dead_code)]
    pub fn static_default_model_for_provider(provider_name: &str) -> &'static str {
        match provider_name.to_lowercase().as_str() {
            "gemini" | "google" => "gemini-2.0-flash",
            "anthropic" | "claude" => crate::constants::ANTHROPIC_DEFAULT_MODEL,
            "openrouter" => crate::constants::OPENROUTER_DEFAULT_MODEL,
            "openai" => crate::constants::OPENAI_DEFAULT_MODEL,
            "deepseek" => crate::constants::DEEPSEEK_DEFAULT_MODEL,
            "groq" => crate::constants::GROQ_DEFAULT_MODEL,
            "together" => crate::constants::TOGETHER_DEFAULT_MODEL,
            "minimax" => crate::constants::MINIMAX_DEFAULT_MODEL,
            "z.ai" | "z_ai" | "zhipu" | "glm" | "bigmodel" => crate::constants::ZHIPU_DEFAULT_MODEL,
            "mistral" => crate::constants::MISTRAL_DEFAULT_MODEL,
            "huggingface" | "hf" => crate::constants::HUGGINGFACE_DEFAULT_MODEL,
            "nvidia" | "nim" | "nvidia-nim" => crate::constants::NVIDIA_NIM_DEFAULT_MODEL,
            "opencode" | "opencode-zen" | "zen" => crate::constants::OPENCODE_ZEN_DEFAULT_MODEL,
            "cerebras" => crate::constants::CEREBRAS_DEFAULT_MODEL,
            "agnes" | "agnes-ai" => crate::constants::AGNES_DEFAULT_MODEL,
            "nara" | "nararouter" | "bynara" => crate::constants::NARA_DEFAULT_MODEL,
            "ofox" | "ofoxai" => crate::constants::OFOX_DEFAULT_MODEL,
            "aihubmix" => crate::constants::AIHUBMIX_DEFAULT_MODEL,
            "kimi" | "moonshot" | "moonshotai" => crate::constants::KIMI_DEFAULT_MODEL,
            "ollama" => crate::constants::OLLAMA_DEFAULT_MODEL,
            "lmstudio" | "lm-studio" | "vllm" | "local" | "localhost" | "localai" => "local-model",
            _ => "default-model",
        }
    }

    /// Associated function returning default fallback model from the static catalog
    #[allow(dead_code)]
    pub fn default_model_for_provider(provider_name: &str) -> &'static str {
        Self::static_default_model_for_provider(provider_name)
    }

    /// Returns the effective default model for a provider:
    /// First checks `self.provider.default_models.get(provider_name)`;
    /// if present and non-empty, returns it. Otherwise dynamically resolves via DynamicModelResolver.
    pub fn get_default_model_for_provider(&self, provider_name: &str) -> String {
        let norm = provider_name.to_lowercase();
        if let Some(model) = self
            .provider
            .default_models
            .get(&norm)
            .or_else(|| self.provider.default_models.get(provider_name))
        {
            let trimmed = model.trim();
            if !trimmed.is_empty() {
                return trimmed.to_string();
            }
        }
        crate::agent::models::DynamicModelResolver::resolve_model_sync(provider_name, None)
    }

    /// Returns true if a local provider has been explicitly configured by the user
    /// (via custom_endpoints, api_keys, default_models, host override, or env vars).
    pub fn is_local_provider_configured(&self, provider_name: &str) -> bool {
        let norm = provider_name.to_lowercase();
        if self.provider.custom_endpoints.contains_key(&norm)
            || self.provider.api_keys.contains_key(&norm)
            || self.provider.default_models.contains_key(&norm)
        {
            return true;
        }

        match norm.as_str() {
            "ollama" => {
                std::env::var("OLLAMA_HOST").is_ok()
                    || std::env::var("OLLAMA_API_KEY").is_ok()
                    || self.provider.ollama.host != crate::constants::DEFAULT_OLLAMA_HOST
            }
            "lmstudio" | "lm-studio" => {
                std::env::var("LMSTUDIO_BASE_URL").is_ok()
                    || std::env::var("LMSTUDIO_API_KEY").is_ok()
            }
            _ => false,
        }
    }

    /// Scans known providers in deterministic priority order:
    /// `anthropic`, `gemini`, `openai`, `openrouter`, `deepseek`, `groq`, `mistral`, `together`, `minimax`, `z.ai`,
    /// `huggingface`, `nvidia`, `opencode`, `cerebras`, `agnes`, `nara`, `ofox`, `aihubmix`, `kimi`,
    /// followed by local providers (`ollama`, `lmstudio`).
    /// If any has a valid key (or is local and configured), returns `(provider_name, default_model)`.
    pub fn find_first_configured_provider(&self) -> Option<(&str, &str)> {
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

        for &provider in &CLOUD_PROVIDERS {
            if let Ok(key) = self.get_api_key(provider) {
                if !key.trim().is_empty() {
                    return Some((provider, self.resolve_configured_default_model(provider)));
                }
            }
        }

        const LOCAL_PROVIDERS: [&str; 2] = ["ollama", "lmstudio"];
        for &provider in &LOCAL_PROVIDERS {
            if self.is_local_provider_configured(provider) {
                return Some((provider, self.resolve_configured_default_model(provider)));
            }
        }

        // Check custom endpoints configured by the user
        if let Some(custom_name) = self.provider.custom_endpoints.keys().next() {
            let model = self
                .provider
                .default_models
                .get(custom_name)
                .map(|m| m.as_str())
                .unwrap_or("default-model");
            return Some((custom_name.as_str(), model));
        }

        // Check any custom API keys configured by the user
        for (custom_name, key) in &self.provider.api_keys {
            if !key.trim().is_empty() {
                let model = self
                    .provider
                    .default_models
                    .get(custom_name)
                    .map(|m| m.as_str())
                    .unwrap_or("default-model");
                return Some((custom_name.as_str(), model));
            }
        }

        None
    }

    /// Resolves active provider and model using the 6-tier resolution hierarchy:
    /// 1. CLI flag / env var override (`provider.default` and `provider.model` already set)
    /// 2. Workspace preference from `workspaces.toml` (or `.minicode/config.toml` merged earlier)
    /// 3. Auto-discovery of first configured provider
    /// 4. Empty fallback `("", "")` for deferred onboarding Gate 1 setup
    pub fn resolve_active_provider_and_model(
        &self,
        workspace_root: Option<&Path>,
    ) -> (String, String) {
        self.resolve_active_provider_and_model_with_registry(
            workspace_root,
            Self::get_workspace_registry_path().as_deref(),
        )
    }

    /// Version of `resolve_active_provider_and_model` with customizable registry file path for deterministic testing.
    pub fn resolve_active_provider_and_model_with_registry(
        &self,
        workspace_root: Option<&Path>,
        registry_path: Option<&Path>,
    ) -> (String, String) {
        // 1. Explicit provider override (e.g. from CLI flag or env var override):
        if !self.provider.default.is_empty() {
            let model = if !self.provider.model.is_empty() {
                self.provider.model.clone()
            } else {
                self.get_default_model_for_provider(&self.provider.default)
            };
            return (self.provider.default.clone(), model);
        }

        // 2. Check workspace preference in workspaces.toml
        if let Some(ws) = workspace_root {
            let pref = if let Some(reg) = registry_path {
                load_workspace_preference_from_file(ws, reg)
            } else {
                Self::load_workspace_preference(ws)
            };

            if let Some(pref) = pref {
                if !pref.provider.trim().is_empty() && !pref.model.trim().is_empty() {
                    let model = if !self.provider.model.is_empty() {
                        self.provider.model.clone()
                    } else {
                        pref.model
                    };
                    return (pref.provider, model);
                }
            }
        }

        // 3. Check find_first_configured_provider()
        if let Some((prov, default_m)) = self.find_first_configured_provider() {
            let model = if !self.provider.model.is_empty() {
                self.provider.model.clone()
            } else {
                default_m.to_string()
            };
            return (prov.to_string(), model);
        }

        // 4. Empty fallback (triggering Gate 1 deferred setup when prompt arrives)
        (String::new(), String::new())
    }

    /// Returns the Orchestrator's Primary `ModelSlot` (`[1] Primary`).
    #[must_use]
    pub fn orchestrator_primary_slot(&self, workspace_root: Option<&Path>) -> Option<ModelSlot> {
        let (prov, mdl) = if !self.provider.default.trim().is_empty() {
            let p = self.provider.default.trim().to_lowercase();
            let m = if !self.provider.model.trim().is_empty() {
                self.provider.model.trim().to_string()
            } else {
                self.get_default_model_for_provider(&p)
            };
            (p, m)
        } else {
            self.resolve_active_provider_and_model(workspace_root)
        };
        let slot = ModelSlot::new(prov, mdl);
        slot.is_valid().then_some(slot)
    }

    /// Returns the 4-slot configuration `[Primary, 1st Fallback, 2nd Fallback, Global Last Fallback ★]`
    /// for `"global"` (Orchestrator + Default MiniPubs) or a specific MiniPub role slug (e.g. `"coder"`, `"vision"`).
    #[must_use]
    pub fn get_four_slots_for_target(
        &self,
        target: &str,
        workspace_root: Option<&Path>,
    ) -> [Option<ModelSlot>; 4] {
        let norm_target = target.trim().to_lowercase().replace('-', "_");
        let orch_primary = self.orchestrator_primary_slot(workspace_root);
        let orch_f1 = self.provider.fallback_1.clone().filter(|s| s.is_valid());
        let orch_f2 = self.provider.fallback_2.clone().filter(|s| s.is_valid());
        let global_last = self
            .provider
            .global_last_fallback
            .clone()
            .filter(|s| s.is_valid());

        if norm_target.is_empty() || norm_target == "global" || norm_target == "orchestrator" {
            return [orch_primary, orch_f1, orch_f2, global_last];
        }

        let default_chain = &self.agent.minipub.default_chain;
        let role_chain = self.agent.minipub.role_models.get(&norm_target);

        let p = role_chain
            .and_then(|c| c.primary.clone())
            .filter(|s| s.is_valid())
            .or_else(|| default_chain.primary.clone().filter(|s| s.is_valid()))
            .or(orch_primary);
        let f1 = role_chain
            .and_then(|c| c.fallback_1.clone())
            .filter(|s| s.is_valid())
            .or_else(|| default_chain.fallback_1.clone().filter(|s| s.is_valid()))
            .or(orch_f1);
        let f2 = role_chain
            .and_then(|c| c.fallback_2.clone())
            .filter(|s| s.is_valid())
            .or_else(|| default_chain.fallback_2.clone().filter(|s| s.is_valid()))
            .or(orch_f2);

        [p, f1, f2, global_last]
    }

    /// Resolves the deduplicated ordered failover chain (`Primary -> 1st Fallback -> 2nd Fallback -> Global Last Fallback ★`)
    /// for the Orchestrator.
    #[must_use]
    pub fn resolve_orchestrator_chain(&self, workspace_root: Option<&Path>) -> Vec<ModelSlot> {
        Self::dedup_slots(self.get_four_slots_for_target("global", workspace_root))
    }

    /// Resolves the deduplicated ordered failover chain (`Primary -> 1st Fallback -> 2nd Fallback -> Global Last Fallback ★`)
    /// for a specific MiniPub role slug (falling back to Orchestrator slots by default, with Slot 4 always shared globally).
    #[must_use]
    #[allow(dead_code)]
    pub fn resolve_minipub_chain(
        &self,
        role_slug: &str,
        workspace_root: Option<&Path>,
    ) -> Vec<ModelSlot> {
        Self::dedup_slots(self.get_four_slots_for_target(role_slug, workspace_root))
    }

    fn resolve_configured_default_model<'a>(&'a self, provider: &str) -> &'a str {
        let norm = provider.to_lowercase();
        self.provider
            .default_models
            .get(&norm)
            .or_else(|| self.provider.default_models.get(provider))
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| Self::static_default_model_for_provider(provider))
    }

    pub(crate) fn dedup_slots(slots: [Option<ModelSlot>; 4]) -> Vec<ModelSlot> {
        let mut out = Vec::with_capacity(4);
        for slot in slots.into_iter().flatten() {
            if slot.is_valid()
                && !out.iter().any(|existing: &ModelSlot| {
                    existing.provider.eq_ignore_ascii_case(&slot.provider)
                        && existing.model == slot.model
                })
            {
                out.push(slot);
            }
        }
        out
    }

    /// Returns true if the given MiniPub role slug has a custom override in `role_models`.
    #[must_use]
    pub fn has_custom_role_chain(&self, role_slug: &str) -> bool {
        let norm = role_slug.trim().to_lowercase().replace('-', "_");
        self.agent
            .minipub
            .role_models
            .get(&norm)
            .is_some_and(|c| !c.is_empty())
    }

    /// Assigns (or clears when `slot` is `None`) one of the 4 slots (`1` = Primary, `2` = 1st Fallback,
    /// `3` = 2nd Fallback, `4` = Global Last Fallback ★) for `"global"` (Orchestrator + Default MiniPubs)
    /// or a specific MiniPub role slug.
    /// Note: Slot 4 (`Global Last Fallback ★`) is always common/shared globally across the Orchestrator and all MiniPubs.
    pub fn set_slot_for_target(&mut self, target: &str, slot_num: usize, slot: Option<ModelSlot>) {
        let clean_slot = slot.filter(|s| s.is_valid());
        if slot_num == 4 {
            self.provider.global_last_fallback = clean_slot;
            return;
        }
        let norm_target = target.trim().to_lowercase().replace('-', "_");
        if norm_target.is_empty() || norm_target == "global" || norm_target == "orchestrator" {
            match slot_num {
                1 => {
                    if let Some(s) = clean_slot {
                        self.provider.default = s.provider.clone();
                        self.provider.model = s.model.clone();
                        self.provider.default_models.insert(s.provider, s.model);
                    }
                }
                2 => self.provider.fallback_1 = clean_slot,
                3 => self.provider.fallback_2 = clean_slot,
                _ => {}
            }
            return;
        }
        if norm_target == "all_minipubs" {
            match slot_num {
                1 => self.agent.minipub.default_chain.primary = clean_slot,
                2 => self.agent.minipub.default_chain.fallback_1 = clean_slot,
                3 => self.agent.minipub.default_chain.fallback_2 = clean_slot,
                _ => {}
            }
            return;
        }

        let entry = self
            .agent
            .minipub
            .role_models
            .entry(norm_target.clone())
            .or_default();
        match slot_num {
            1 => entry.primary = clean_slot,
            2 => entry.fallback_1 = clean_slot,
            3 => entry.fallback_2 = clean_slot,
            _ => {}
        }
        if entry.is_empty() {
            self.agent.minipub.role_models.remove(&norm_target);
        }
    }

    /// Synchronizes all MiniPubs to inherit the Global Orchestrator chain (`Primary -> 1st Fallback -> 2nd Fallback -> Global Last Fallback ★`),
    /// clearing any divergent per-role overrides.
    pub fn sync_all_minipubs_to_orchestrator(&mut self) {
        self.agent.minipub.sync_with_orchestrator = true;
        self.agent.minipub.default_chain = MiniPubModelChain::default();
        self.agent.minipub.role_models.clear();
    }

    /// Clears custom model overrides for a single MiniPub role so it inherits from the Global Orchestrator chain.
    pub fn clear_minipub_role_override(&mut self, role_slug: &str) {
        let norm = role_slug.trim().to_lowercase().replace('-', "_");
        self.agent.minipub.role_models.remove(&norm);
    }

    /// Resolves the API key for a specific provider.
    /// Checks environment variables first, then persistent `[provider.api_keys]` in config.toml.
    /// For local models (Ollama, LM Studio, vLLM, localhost endpoints), API keys are strictly optional.
    pub fn get_api_key(&self, provider_name: &str) -> Result<String> {
        let env_trimmed = |key: &str| std::env::var(key).map(|v| v.trim().to_string());
        let norm = provider_name.to_lowercase();

        // 1. Check environment variables
        let env_val = match norm.as_str() {
            "gemini" | "google" => {
                env_trimmed("GEMINI_API_KEY").or_else(|_| env_trimmed("GOOGLE_API_KEY"))
            }
            "anthropic" | "claude" => env_trimmed("ANTHROPIC_API_KEY"),
            "openrouter" => {
                env_trimmed("OPENROUTER_API_KEY").or_else(|_| env_trimmed("OPENROUTER_KEY"))
            }
            "openai" => env_trimmed("OPENAI_API_KEY"),
            "deepseek" => env_trimmed("DEEPSEEK_API_KEY"),
            "groq" => env_trimmed("GROQ_API_KEY"),
            "together" => env_trimmed("TOGETHER_API_KEY"),
            "minimax" => env_trimmed("MINIMAX_API_KEY"),
            "z.ai" | "z_ai" | "zhipu" | "glm" | "bigmodel" => env_trimmed("ZHIPU_API_KEY")
                .or_else(|_| env_trimmed("Z_AI_API_KEY"))
                .or_else(|_| env_trimmed("GLM_API_KEY"))
                .or_else(|_| env_trimmed("BIGMODEL_API_KEY")),
            "mistral" => env_trimmed("MISTRAL_API_KEY"),
            "huggingface" | "hf" => env_trimmed("HUGGINGFACE_API_KEY")
                .or_else(|_| env_trimmed("HF_TOKEN"))
                .or_else(|_| env_trimmed("HUGGINGFACE_TOKEN")),
            "nvidia" | "nim" | "nvidia-nim" => {
                env_trimmed("NVIDIA_API_KEY").or_else(|_| env_trimmed("NGC_API_KEY"))
            }
            "opencode" | "opencode-zen" | "zen" => {
                env_trimmed("OPENCODE_API_KEY").or_else(|_| env_trimmed("OPENCODE_ZEN_API_KEY"))
            }
            "cerebras" => env_trimmed("CEREBRAS_API_KEY"),
            "agnes" | "agnes-ai" => env_trimmed("AGNES_API_KEY"),
            "nara" | "nararouter" | "bynara" => {
                env_trimmed("NARA_API_KEY").or_else(|_| env_trimmed("NARAROUTER_API_KEY"))
            }
            "ofox" | "ofoxai" => {
                env_trimmed("OFOX_API_KEY").or_else(|_| env_trimmed("OFOXAI_API_KEY"))
            }
            "aihubmix" => env_trimmed("AIHUBMIX_API_KEY"),
            "kimi" | "moonshot" | "moonshotai" => {
                env_trimmed("KIMI_API_KEY").or_else(|_| env_trimmed("MOONSHOT_API_KEY"))
            }
            "ollama" => env_trimmed("OLLAMA_API_KEY"),
            custom => {
                let sanitized_custom = custom.to_uppercase().replace(['-', '.'], "_");
                let env_var = format!("{}_API_KEY", sanitized_custom);
                env_trimmed(&env_var)
            }
        };

        if let Ok(key) = env_val {
            if !key.is_empty() {
                return Ok(key);
            }
        }

        // 2. Check persistent api_keys table in config.toml
        if let Some(key) = self
            .provider
            .api_keys
            .get(&norm)
            .or_else(|| match norm.as_str() {
                "gemini" | "google" => self
                    .provider
                    .api_keys
                    .get("gemini")
                    .or_else(|| self.provider.api_keys.get("google")),
                "z.ai" | "z_ai" | "zhipu" | "glm" | "bigmodel" => self
                    .provider
                    .api_keys
                    .get("z.ai")
                    .or_else(|| self.provider.api_keys.get("z_ai"))
                    .or_else(|| self.provider.api_keys.get("zhipu"))
                    .or_else(|| self.provider.api_keys.get("glm")),
                "huggingface" | "hf" => self
                    .provider
                    .api_keys
                    .get("huggingface")
                    .or_else(|| self.provider.api_keys.get("hf")),
                "nvidia" | "nim" | "nvidia-nim" => self
                    .provider
                    .api_keys
                    .get("nvidia")
                    .or_else(|| self.provider.api_keys.get("nim")),
                "opencode" | "opencode-zen" | "zen" => self
                    .provider
                    .api_keys
                    .get("opencode")
                    .or_else(|| self.provider.api_keys.get("opencode-zen")),
                "kimi" | "moonshot" | "moonshotai" => self
                    .provider
                    .api_keys
                    .get("kimi")
                    .or_else(|| self.provider.api_keys.get("moonshot")),
                "openrouter" => self.provider.api_keys.get("openrouter"),
                _ => None,
            })
        {
            if !key.trim().is_empty() {
                return Ok(key.trim().to_string());
            }
        }

        // 3. Localhost and local models run without requiring an API key (it is optional)
        if self.is_local_provider(provider_name) {
            return Ok(String::new());
        }

        let env_var = match norm.as_str() {
            "gemini" | "google" => "GEMINI_API_KEY",
            "anthropic" | "claude" => "ANTHROPIC_API_KEY",
            "openrouter" => "OPENROUTER_API_KEY",
            "openai" => "OPENAI_API_KEY",
            "deepseek" => "DEEPSEEK_API_KEY",
            "groq" => "GROQ_API_KEY",
            "together" => "TOGETHER_API_KEY",
            "minimax" => "MINIMAX_API_KEY",
            "z.ai" | "z_ai" | "zhipu" | "glm" | "bigmodel" => "ZHIPU_API_KEY",
            "mistral" => "MISTRAL_API_KEY",
            "huggingface" | "hf" => "HF_TOKEN",
            "nvidia" | "nim" | "nvidia-nim" => "NVIDIA_API_KEY",
            "opencode" | "opencode-zen" | "zen" => "OPENCODE_API_KEY",
            "cerebras" => "CEREBRAS_API_KEY",
            "agnes" | "agnes-ai" => "AGNES_API_KEY",
            "nara" | "nararouter" | "bynara" => "NARA_API_KEY",
            "ofox" | "ofoxai" => "OFOX_API_KEY",
            "aihubmix" => "AIHUBMIX_API_KEY",
            "kimi" | "moonshot" | "moonshotai" => "KIMI_API_KEY",
            _ => "",
        };

        let env_var_string = if env_var.is_empty() {
            let sanitized_custom = norm.to_uppercase().replace(['-', '.'], "_");
            format!("{}_API_KEY", sanitized_custom)
        } else {
            env_var.to_string()
        };

        Err(ConfigError::MissingApiKey {
            provider: provider_name.to_string(),
            env_var: env_var_string,
        }
        .into())
    }

    /// Returns the path to the global workspaces registry file (~/.config/minicode/workspaces.toml)
    #[allow(dead_code)]
    pub fn get_workspace_registry_path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| {
            d.join(crate::constants::CONFIG_DIR_NAME)
                .join(crate::constants::WORKSPACES_FILE_NAME)
        })
    }

    /// Loads the stored preference for the given workspace root from the global registry.
    #[allow(dead_code)]
    pub fn load_workspace_preference(workspace_root: &Path) -> Option<WorkspacePreference> {
        let registry_path = Self::get_workspace_registry_path()?;
        let canonical = match workspace_root.canonicalize() {
            Ok(c) => c,
            Err(_) => workspace_root.to_path_buf(),
        };
        load_workspace_preference_from_file(&canonical, &registry_path)
            .or_else(|| load_workspace_preference_from_file(workspace_root, &registry_path))
    }

    /// Saves the preferred provider and model for the given workspace root into the global registry.
    #[allow(dead_code)]
    pub fn save_workspace_preference(
        workspace_root: &Path,
        provider: &str,
        model: &str,
    ) -> anyhow::Result<()> {
        let registry_path = match Self::get_workspace_registry_path() {
            Some(p) => p,
            None => anyhow::bail!("Unable to determine config directory"),
        };
        let canonical = match workspace_root.canonicalize() {
            Ok(c) => c,
            Err(_) => workspace_root.to_path_buf(),
        };
        save_workspace_preference_to_file(&canonical, provider, model, &registry_path)
    }
}

/// Universally masks sensitive credentials for safe display and tool returns.
/// Empty or whitespace-only keys return empty string.
/// Keys <= 8 chars return 8 bullet points "••••••••".
/// Keys > 8 chars return first 4 chars + "..." + last 4 chars (e.g. "sk-a...cdef").
#[allow(dead_code)]
pub fn mask_api_key(key: &str) -> String {
    let trimmed = key.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let char_count = trimmed.chars().count();
    if char_count <= 8 {
        return "••••••••".to_string();
    }
    let prefix: String = trimmed.chars().take(4).collect();
    let suffix: String = trimmed.chars().skip(char_count.saturating_sub(4)).collect();
    format!("{}...{}", prefix, suffix)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[allow(dead_code)]
pub struct WorkspacePreference {
    pub provider: String,
    pub model: String,
    #[serde(default)]
    pub last_used: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[allow(dead_code)]
pub struct WorkspaceRegistry {
    #[serde(default)]
    pub workspaces: std::collections::HashMap<String, WorkspacePreference>,
}

/// Loads a workspace preference from a specific registry file path.
#[allow(dead_code)]
pub fn load_workspace_preference_from_file(
    workspace_root: &Path,
    registry_path: &Path,
) -> Option<WorkspacePreference> {
    if !registry_path.exists() {
        return None;
    }
    let content = match std::fs::read_to_string(registry_path) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(path = %registry_path.display(), error = %e, "Failed to read workspaces registry");
            return None;
        }
    };
    let registry: WorkspaceRegistry = match toml::from_str(&content) {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(path = %registry_path.display(), error = %e, "Failed to parse workspaces registry TOML");
            return None;
        }
    };
    let key = workspace_root.to_string_lossy().to_string();
    if let Some(pref) = registry.workspaces.get(&key) {
        return Some(pref.clone());
    }
    if let Ok(canon) = workspace_root.canonicalize() {
        let canon_key = canon.to_string_lossy().to_string();
        if let Some(pref) = registry.workspaces.get(&canon_key) {
            return Some(pref.clone());
        }
    }
    None
}

/// Saves or updates a workspace preference into a specific registry file path.
#[allow(dead_code)]
pub fn save_workspace_preference_to_file(
    workspace_root: &Path,
    provider: &str,
    model: &str,
    registry_path: &Path,
) -> anyhow::Result<()> {
    let mut registry = if registry_path.exists() {
        let content = std::fs::read_to_string(registry_path)?;
        toml::from_str::<WorkspaceRegistry>(&content).map_err(|e| {
            anyhow::anyhow!(
                "Failed to parse workspaces registry at {}: {}",
                registry_path.display(),
                e
            )
        })?
    } else {
        WorkspaceRegistry::default()
    };

    if let Some(parent) = registry_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let key = workspace_root.to_string_lossy().to_string();
    registry.workspaces.insert(
        key,
        WorkspacePreference {
            provider: provider.to_string(),
            model: model.to_string(),
            last_used: chrono::Utc::now().to_rfc3339(),
        },
    );

    let content = toml::to_string_pretty(&registry)?;
    std::fs::write(registry_path, content)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.provider.default, "");
        assert_eq!(config.provider.model, "");
        assert_eq!(config.agent.timeout, 30);
        assert_eq!(config.agent.map_tokens, 1024);
        assert_eq!(config.agent.max_tool_iterations, 0);
        assert!(config.agent.auto_continue);
        assert_eq!(config.agent.max_auto_continues, 5);
        assert!(!config.ui.plain);
    }

    #[test]
    fn test_agent_iteration_config_merge() {
        let toml_str = r#"
            [agent]
            max_tool_iterations = 25
            auto_continue = false
            max_auto_continues = 2
        "#;
        let raw: RawConfig = toml::from_str(toml_str).unwrap();
        let mut config = Config::default();
        config.merge_raw(raw);
        assert_eq!(config.agent.max_tool_iterations, 25);
        assert!(!config.agent.auto_continue);
        assert_eq!(config.agent.max_auto_continues, 2);

        // Test explicit 0 (unbounded) merge
        let unbounded_toml = r#"
            [agent]
            max_tool_iterations = 0
        "#;
        let raw_unbounded: RawConfig = toml::from_str(unbounded_toml).unwrap();
        config.merge_raw(raw_unbounded);
        assert_eq!(config.agent.max_tool_iterations, 0);
    }

    #[test]
    fn test_toml_parsing() {
        let toml_content = r#"
            [provider]
            default = "anthropic"
            model = "claude-3-7-sonnet"
            temperature = 0.5
            max_tokens = 4096

            [agent]
            auto_approve = true
            timeout = 60
            map_tokens = 2048

            [ui]
            theme = "dark"
            plain = true
        "#;

        let config: Config = toml::from_str(toml_content).unwrap();
        assert_eq!(config.provider.default, "anthropic");
        assert_eq!(config.provider.model, "claude-3-7-sonnet");
        assert!(config.agent.auto_approve);
        assert_eq!(config.agent.timeout, 60);
        assert_eq!(config.agent.map_tokens, 2048);
        assert!(config.ui.plain);
        assert_eq!(config.ui.theme, "dark");
    }

    #[test]
    fn test_raw_config_merge_animation_and_ui_preferences() {
        let mut config = Config::default();
        assert_eq!(config.ui.animation, "dual_pillars");

        let override_toml = r#"
            [ui]
            animation = "braille_wave"
            theme = "monokai"
            show_cost = true
            todo_style = "rail"

            [agent]
            tool_mode = "full"
            syntax_barrier = false
            auto_lint = false
        "#;
        let raw: RawConfig = toml::from_str(override_toml).unwrap();
        config.merge_raw(raw);
        assert_eq!(config.ui.animation, "braille_wave");
        assert_eq!(config.ui.theme, "monokai");
        assert_eq!(config.ui.todo_style, "rail");
        assert_eq!(config.ui.todo_style_enum(), TodoWidgetStyle::Rail);
        assert!(config.ui.show_cost);
        assert_eq!(config.agent.tool_mode, ToolFilterMode::Full);
        assert!(!config.agent.syntax_barrier);
        assert!(!config.agent.auto_lint);
    }

    #[test]
    fn test_todo_widget_style_enum_mapping() {
        let mut ui = UiConfig::default();
        assert_eq!(ui.todo_style, "tree");
        assert_eq!(ui.todo_style_enum(), TodoWidgetStyle::Tree);

        ui.todo_style = "card".to_string();
        assert_eq!(ui.todo_style_enum(), TodoWidgetStyle::Card);

        ui.todo_style = "box".to_string();
        assert_eq!(ui.todo_style_enum(), TodoWidgetStyle::Card);

        ui.todo_style = "rail".to_string();
        assert_eq!(ui.todo_style_enum(), TodoWidgetStyle::Rail);

        ui.todo_style = "gutter".to_string();
        assert_eq!(ui.todo_style_enum(), TodoWidgetStyle::Rail);

        ui.todo_style = "minimal".to_string();
        assert_eq!(ui.todo_style_enum(), TodoWidgetStyle::Minimal);

        ui.todo_style = "open".to_string();
        assert_eq!(ui.todo_style_enum(), TodoWidgetStyle::Minimal);

        ui.todo_style = "unknown".to_string();
        assert_eq!(ui.todo_style_enum(), TodoWidgetStyle::Tree);
    }

    #[test]
    fn test_raw_config_merge_override_false() {
        let mut config = Config::default();
        config.agent.auto_approve = true;

        let override_toml = r#"
            [agent]
            auto_approve = false
        "#;
        let raw: RawConfig = toml::from_str(override_toml).unwrap();
        config.merge_raw(raw);
        assert!(!config.agent.auto_approve);
    }

    static ENV_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn test_get_api_key_trims_whitespace() {
        let _guard = ENV_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let config = Config::default();
        std::env::set_var("GEMINI_API_KEY", "  sk-test-gemini-key-12345\n\t ");
        let key = config.get_api_key("gemini").unwrap();
        assert_eq!(key, "sk-test-gemini-key-12345");
        std::env::remove_var("GEMINI_API_KEY");
    }

    #[test]
    fn test_config_merges_ollama_from_toml() {
        let mut config = Config::default();
        let override_toml = r#"
            [provider.ollama]
            host = "http://192.168.1.100:11434"
        "#;
        let raw: RawConfig = toml::from_str(override_toml).unwrap();
        config.merge_raw(raw);
        assert_eq!(config.provider.ollama.host, "http://192.168.1.100:11434");
    }

    #[test]
    fn test_config_merges_git_from_toml() {
        let mut config = Config::default();
        assert!(config.git.auto_commit);
        assert!(!config.git.dirty_commit);

        let override_toml = r#"
            [git]
            auto_commit = false
            dirty_commit = true
            ai_commit_messages = false
        "#;
        let raw: RawConfig = toml::from_str(override_toml).unwrap();
        config.merge_raw(raw);
        assert!(!config.git.auto_commit);
        assert!(config.git.dirty_commit);
        assert!(!config.git.ai_commit_messages);
    }

    const TEST_PROVIDER_ENV_KEYS: &[&str] = &[
        "ANTHROPIC_API_KEY",
        "GEMINI_API_KEY",
        "GOOGLE_API_KEY",
        "OPENAI_API_KEY",
        "OPENROUTER_API_KEY",
        "DEEPSEEK_API_KEY",
        "GROQ_API_KEY",
        "MISTRAL_API_KEY",
        "TOGETHER_API_KEY",
        "MINIMAX_API_KEY",
        "ZAI_API_KEY",
        "ZHIPU_API_KEY",
        "GLM_API_KEY",
        "HF_TOKEN",
        "HUGGINGFACE_API_KEY",
        "NVIDIA_API_KEY",
        "OPENCODE_API_KEY",
        "CEREBRAS_API_KEY",
        "AGNES_API_KEY",
        "NARA_API_KEY",
        "OFOX_API_KEY",
        "AIHUBMIX_API_KEY",
        "KIMI_API_KEY",
        "MOONSHOT_API_KEY",
    ];

    fn with_clean_provider_env<R>(f: impl FnOnce() -> R) -> R {
        let _guard = ENV_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let saved: Vec<(&str, Option<String>)> = TEST_PROVIDER_ENV_KEYS
            .iter()
            .map(|&k| {
                let v = std::env::var(k).ok();
                std::env::remove_var(k);
                (k, v)
            })
            .collect();
        let res = f();
        for (k, v) in saved {
            if let Some(val) = v {
                std::env::set_var(k, val);
            } else {
                std::env::remove_var(k);
            }
        }
        res
    }

    #[test]
    fn test_get_api_key_from_persistent_map() {
        with_clean_provider_env(|| {
            let mut config = Config::default();
            config.provider.api_keys.insert(
                "openrouter".to_string(),
                "sk-or-v1-persist-test".to_string(),
            );
            config
                .provider
                .api_keys
                .insert("minimax".to_string(), "mm-test-key".to_string());
            config
                .provider
                .api_keys
                .insert("z.ai".to_string(), "glm-test-key".to_string());

            assert_eq!(
                config.get_api_key("openrouter").unwrap(),
                "sk-or-v1-persist-test"
            );
            assert_eq!(config.get_api_key("minimax").unwrap(), "mm-test-key");
            assert_eq!(config.get_api_key("z.ai").unwrap(), "glm-test-key");
            assert_eq!(config.get_api_key("zhipu").unwrap(), "glm-test-key");
            assert_eq!(config.get_api_key("ollama").unwrap(), "");
        });
    }

    #[test]
    fn test_local_providers_key_optional() {
        let mut config = Config::default();
        // Ollama, LM Studio, vLLM, localhost all return Ok("") without requiring API keys
        assert_eq!(config.get_api_key("ollama").unwrap(), "");
        assert_eq!(config.get_api_key("localhost").unwrap(), "");
        assert_eq!(config.get_api_key("local").unwrap(), "");
        assert_eq!(config.get_api_key("lmstudio").unwrap(), "");
        assert_eq!(config.get_api_key("vllm").unwrap(), "");
        assert_eq!(config.get_api_key("localai").unwrap(), "");
        assert_eq!(config.get_api_key("llama.cpp").unwrap(), "");

        // Custom endpoint on localhost is also detected as local
        config.provider.custom_endpoints.insert(
            "my-local-server".to_string(),
            "http://127.0.0.1:9090/v1".to_string(),
        );
        assert!(config.is_local_provider("my-local-server"));
        assert_eq!(config.get_api_key("my-local-server").unwrap(), "");

        // Base URL resolution
        assert_eq!(
            config.get_provider_base_url("lmstudio").unwrap(),
            "http://localhost:1234/v1"
        );
        assert_eq!(
            config.get_provider_base_url("vllm").unwrap(),
            "http://localhost:8000/v1"
        );
        assert_eq!(
            config.get_provider_base_url("my-local-server").unwrap(),
            "http://127.0.0.1:9090/v1"
        );

        // Default models
        assert!(!config.get_default_model_for_provider("ollama").is_empty());
        assert_eq!(
            config.get_default_model_for_provider("lmstudio"),
            "local-model"
        );
    }

    #[test]
    fn test_mask_api_key_variations() {
        assert_eq!(mask_api_key(""), "");
        assert_eq!(mask_api_key("   "), "");
        assert_eq!(mask_api_key("short"), "••••••••");
        assert_eq!(mask_api_key("12345678"), "••••••••");
        assert_eq!(mask_api_key("123456789"), "1234...6789");
        assert_eq!(mask_api_key("🔑1234567890🦀"), "🔑123...890🦀");
        assert_eq!(mask_api_key("sk-ant-1234567890abcdef"), "sk-a...cdef");
        assert_eq!(mask_api_key("AIzaSyD-1234567890XYZ"), "AIza...0XYZ");
    }

    #[test]
    fn test_workspace_preference_roundtrip() {
        let temp_dir = tempfile::tempdir().unwrap();
        let ws_path = temp_dir.path();

        // Initially none
        let pref = load_workspace_preference_from_file(ws_path, &ws_path.join("workspaces.toml"));
        assert!(pref.is_none());

        // Save preference
        save_workspace_preference_to_file(
            ws_path,
            "anthropic",
            "claude-3-7-sonnet-20250219",
            &ws_path.join("workspaces.toml"),
        )
        .unwrap();

        let pref =
            load_workspace_preference_from_file(ws_path, &ws_path.join("workspaces.toml")).unwrap();
        assert_eq!(pref.provider, "anthropic");
        assert_eq!(pref.model, "claude-3-7-sonnet-20250219");
    }

    #[test]
    fn test_dynamic_provider_resolution_hierarchy() {
        with_clean_provider_env(|| {
            let temp_dir = tempfile::tempdir().unwrap();
            let ws_path = temp_dir.path();
            let reg_path = ws_path.join("workspaces.toml");

            let mut config = Config::default();
            config.provider.default = String::new();
            config.provider.model = String::new();

            // 0. Empty fallback when nothing is set and no workspace pref
            let empty_reg_dir = tempfile::tempdir().unwrap();
            let empty_ws = empty_reg_dir.path().join("empty_ws");
            let empty_reg = empty_reg_dir.path().join("workspaces.toml");
            let (empty_p, empty_m) = config
                .resolve_active_provider_and_model_with_registry(Some(&empty_ws), Some(&empty_reg));
            assert_eq!(empty_p, "");
            assert_eq!(empty_m, "");

            // 1. Auto-discovery from configured API keys when no workspace pref exists
            config
                .provider
                .api_keys
                .insert("openai".to_string(), "sk-test-key".to_string());
            let (disc_p, disc_m) = config
                .resolve_active_provider_and_model_with_registry(Some(&empty_ws), Some(&empty_reg));
            assert_eq!(disc_p, "openai");
            assert_eq!(disc_m, "gpt-4o");
            config.provider.api_keys.clear();

            // 2. With workspace preference saved
            save_workspace_preference_to_file(
                ws_path,
                "anthropic",
                "claude-3-7-sonnet-20250219",
                &reg_path,
            )
            .unwrap();

            let (prov, model) = config
                .resolve_active_provider_and_model_with_registry(Some(ws_path), Some(&reg_path));
            assert_eq!(prov, "anthropic");
            assert_eq!(model, "claude-3-7-sonnet-20250219");

            // 3. Explicit provider without model overrides workspace preference and gets default model
            config.provider.default = "mistral".to_string();
            config.provider.model = String::new();
            let (prov, model) = config
                .resolve_active_provider_and_model_with_registry(Some(ws_path), Some(&reg_path));
            assert_eq!(prov, "mistral");
            assert_eq!(model, config.get_default_model_for_provider("mistral"));

            // 4. CLI / explicit override with both takes highest precedence over workspace preference
            config.provider.default = "deepseek".to_string();
            config.provider.model = "deepseek-chat".to_string();
            let (prov, model) = config
                .resolve_active_provider_and_model_with_registry(Some(ws_path), Some(&reg_path));
            assert_eq!(prov, "deepseek");
            assert_eq!(model, "deepseek-chat");
        });
    }

    #[test]
    fn test_default_models_override() {
        let mut config = Config::default();
        assert!(!config
            .get_default_model_for_provider("anthropic")
            .is_empty());
        assert!(!config.get_default_model_for_provider("openai").is_empty());

        config.provider.default_models.insert(
            "anthropic".to_string(),
            "claude-3-5-haiku-20241022".to_string(),
        );
        config
            .provider
            .default_models
            .insert("openai".to_string(), "o3-mini".to_string());

        assert_eq!(
            config.get_default_model_for_provider("anthropic"),
            "claude-3-5-haiku-20241022"
        );
        assert_eq!(config.get_default_model_for_provider("openai"), "o3-mini");
        assert!(!config.get_default_model_for_provider("gemini").is_empty());
    }

    #[test]
    fn test_favorite_models_and_effort_level_cycling() {
        let mut config = Config::default();
        assert_eq!(config.provider.effective_level_label(), "medium");
        assert_eq!(config.provider.cycle_effort_level(), "high");
        assert_eq!(config.provider.effective_level_label(), "high");
        assert_eq!(config.provider.cycle_effort_level(), "low");
        assert_eq!(config.provider.effective_level_label(), "low");
        assert_eq!(config.provider.cycle_effort_level(), "medium");
        assert_eq!(config.provider.effective_level_label(), "medium");

        assert!(!config.is_favorite_model("openai", "gpt-5.4-mini"));
        assert!(config.toggle_favorite_model("openai", "gpt-5.4-mini"));
        assert!(config.is_favorite_model("openai", "gpt-5.4-mini"));
        assert_eq!(config.provider.favorite_models.len(), 1);
        assert!(!config.toggle_favorite_model("openai", "gpt-5.4-mini"));
        assert!(!config.is_favorite_model("openai", "gpt-5.4-mini"));
        assert!(config.provider.favorite_models.is_empty());
    }

    #[test]
    fn test_four_slot_orchestrator_and_minipub_routing() {
        let mut config = Config::default();
        config.provider.default = "anthropic".to_string();
        config.provider.model = "claude-3-7-sonnet-20250219".to_string();
        config.provider.fallback_1 = Some(ModelSlot::new("openai", "gpt-4o"));
        config.provider.fallback_2 = Some(ModelSlot::new("gemini", "gemini-2.5-pro"));
        config.provider.global_last_fallback = Some(ModelSlot::new("ollama", "qwen2.5-coder:7b"));

        // 1. Orchestrator 4-slot chain resolves in order [1] Primary -> [2] 1st FB -> [3] 2nd FB -> [4] Global Last ★
        let orch_chain = config.resolve_orchestrator_chain(None);
        assert_eq!(orch_chain.len(), 4);
        assert_eq!(orch_chain[0].provider, "anthropic");
        assert_eq!(orch_chain[0].model, "claude-3-7-sonnet-20250219");
        assert_eq!(orch_chain[1].provider, "openai");
        assert_eq!(orch_chain[1].model, "gpt-4o");
        assert_eq!(orch_chain[2].provider, "gemini");
        assert_eq!(orch_chain[2].model, "gemini-2.5-pro");
        assert_eq!(orch_chain[3].provider, "ollama");
        assert_eq!(orch_chain[3].model, "qwen2.5-coder:7b");

        // 2. By default, all 14 MiniPubs inherit the Orchestrator's 4-slot chain
        for role in &[
            "scout",
            "architect",
            "planner",
            "designer",
            "frontend",
            "backend",
            "coder",
            "vision",
            "content_writer",
            "debugger",
            "reviewer",
            "security",
            "tester",
            "simplifier",
        ] {
            assert_eq!(config.resolve_minipub_chain(role, None), orch_chain);
            assert!(!config.has_custom_role_chain(role));
        }

        // 3. Customizing a specific MiniPub role (e.g. 'vision' or 'coder') overrides slots 1-3
        // while keeping Slot 4 (Global Last Fallback ★) shared across all agents!
        config.set_slot_for_target(
            "vision",
            1,
            Some(ModelSlot::new("gemini", "gemini-2.5-pro")),
        );
        config.set_slot_for_target("vision", 2, Some(ModelSlot::new("openai", "gpt-4o")));
        assert!(config.has_custom_role_chain("vision"));

        let vision_chain = config.resolve_minipub_chain("vision", None);
        assert_eq!(vision_chain[0].provider, "gemini");
        assert_eq!(vision_chain[0].model, "gemini-2.5-pro");
        assert_eq!(vision_chain[1].provider, "openai");
        assert_eq!(vision_chain[1].model, "gpt-4o");
        // Shared Slot 4 is always at the tail
        assert_eq!(vision_chain.last().unwrap().provider, "ollama");
        assert_eq!(vision_chain.last().unwrap().model, "qwen2.5-coder:7b");

        // 4. Setting Slot 4 from ANY target updates the shared Global Last Fallback ★ for everyone
        config.set_slot_for_target(
            "coder",
            4,
            Some(ModelSlot::new("openrouter", "deepseek/deepseek-chat")),
        );
        assert_eq!(
            config
                .resolve_orchestrator_chain(None)
                .last()
                .unwrap()
                .display_short(),
            "openrouter:deepseek/deepseek-chat"
        );
        assert_eq!(
            config
                .resolve_minipub_chain("vision", None)
                .last()
                .unwrap()
                .display_short(),
            "openrouter:deepseek/deepseek-chat"
        );

        // 5. One-click sync_all_minipubs_to_orchestrator resets all MiniPubs back to the Global chain
        config.sync_all_minipubs_to_orchestrator();
        assert!(!config.has_custom_role_chain("vision"));
        assert_eq!(
            config.resolve_minipub_chain("vision", None),
            config.resolve_orchestrator_chain(None)
        );
    }
}

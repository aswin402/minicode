use super::{Config, MiniPubModelChain, ModelSlot};
use crate::error::{ConfigError, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

impl Config {
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

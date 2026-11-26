/// Dynamic pricing and dollar spend calculator for AI model token consumption.
pub struct ModelPricing;

/// Local/free provider prefixes whose token spend is always `$0.00`.
const FREE_PROVIDER_MARKERS: &[&str] = &["ollama", "local", "lmstudio", "vllm", "llama.cpp"];

/// Offline fallback `(input_usd_per_1m, output_usd_per_1m)` table consulted only when live
/// `models.dev` catalog pricing is not yet populated in memory.
const FALLBACK_PRICING_RULES: &[(&[&str], (f64, f64))] = &[
    (&["claude-3-5-haiku", "haiku"], (0.80, 4.00)),
    (&["claude"], (3.00, 15.00)),
    (&["gpt-4o-mini", "-mini"], (0.15, 0.60)),
    (&["gpt-4o"], (2.50, 10.00)),
    (&["o3-mini", "o1"], (1.10, 4.40)),
    (
        &["gemini-2.5-flash", "gemini-2.0-flash", "flash"],
        (0.10, 0.40),
    ),
    (&["gemini"], (1.25, 5.00)),
    (&["deepseek"], (0.14, 0.28)),
];

const DEFAULT_FALLBACK_RATE: (f64, f64) = (1.00, 3.00);

impl ModelPricing {
    /// Returns estimated `(input_usd_per_1m, output_usd_per_1m)` token rates for a provider/model pair.
    /// Queries live `models.dev` catalog pricing first, falling back to `FALLBACK_PRICING_RULES` offline.
    pub fn input_output_rates(provider: &str, model: &str) -> (f64, f64) {
        let prov_lower = provider.trim().to_lowercase();
        let model_lower = model.trim().to_lowercase();

        if FREE_PROVIDER_MARKERS.iter().any(|p| prov_lower.contains(p))
            || model_lower.ends_with(":free")
        {
            return (0.0, 0.0);
        }

        if !prov_lower.is_empty() {
            let qualified = format!("{prov_lower}/{model_lower}");
            if let Some(rates) = crate::agent::models::lookup_models_dev_pricing(&qualified) {
                return rates;
            }
        }
        if let Some(rates) = crate::agent::models::lookup_models_dev_pricing(&model_lower) {
            return rates;
        }

        FALLBACK_PRICING_RULES
            .iter()
            .find(|(patterns, _)| patterns.iter().any(|p| model_lower.contains(p)))
            .map_or(DEFAULT_FALLBACK_RATE, |(_, rates)| *rates)
    }

    /// Computes estimated dollar cost given provider, model, and token metrics
    pub fn calculate_cost(
        provider: &str,
        model: &str,
        prompt_tokens: usize,
        completion_tokens: usize,
    ) -> f64 {
        let (input_rate, output_rate) = Self::input_output_rates(provider, model);

        let prompt_cost = (prompt_tokens as f64 / 1_000_000.0) * input_rate;
        let completion_cost = (completion_tokens as f64 / 1_000_000.0) * output_rate;
        prompt_cost + completion_cost
    }

    /// Computes estimated dollar cost, honoring optional user custom rates (per 1M tokens) if set
    pub fn calculate_cost_with_custom(
        provider: &str,
        model: &str,
        prompt_tokens: usize,
        completion_tokens: usize,
        custom_prompt_rate: Option<f64>,
        custom_comp_rate: Option<f64>,
    ) -> f64 {
        if let (Some(p_rate), Some(c_rate)) = (custom_prompt_rate, custom_comp_rate) {
            let prompt_cost = (prompt_tokens as f64 / 1_000_000.0) * p_rate;
            let completion_cost = (completion_tokens as f64 / 1_000_000.0) * c_rate;
            return prompt_cost + completion_cost;
        }
        Self::calculate_cost(provider, model, prompt_tokens, completion_tokens)
    }

    /// Formats cost into human readable currency string (e.g. "$0.0042" or "$0.18")
    pub fn format_cost(usd: f64) -> String {
        if usd <= 0.00001 {
            "$0.00".to_string()
        } else if usd < 0.01 {
            format!("${:.4}", usd)
        } else {
            format!("${:.2}", usd)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ollama_local_is_free() {
        let cost = ModelPricing::calculate_cost("ollama", "qwen2.5-coder", 100_000, 50_000);
        assert_eq!(cost, 0.0);
        assert_eq!(ModelPricing::format_cost(cost), "$0.00");
    }

    #[test]
    fn test_gemini_flash_pricing() {
        let cost = ModelPricing::calculate_cost("gemini", "gemini-2.5-flash", 10_000, 2_000);
        // (10_000 / 1M) * 0.10 + (2_000 / 1M) * 0.40 = 0.001 + 0.0008 = 0.0018
        assert!((cost - 0.0018).abs() < 0.0001);
        assert_eq!(ModelPricing::format_cost(cost), "$0.0018");
    }

    #[test]
    fn test_live_models_dev_pricing_overrides_static_fallback() {
        crate::agent::models::cache_models_dev_pricing("custom-vendor/future-model-9", 0.42, 1.68);
        let rates = ModelPricing::input_output_rates("custom-vendor", "future-model-9");
        assert!((rates.0 - 0.42).abs() < 1e-6);
        assert!((rates.1 - 1.68).abs() < 1e-6);
    }
}

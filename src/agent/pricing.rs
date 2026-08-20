/// Dynamic pricing and dollar spend calculator for AI model token consumption
pub struct ModelPricing;

impl ModelPricing {
    /// Computes estimated dollar cost given provider, model, and token metrics
    pub fn calculate_cost(
        provider: &str,
        model: &str,
        prompt_tokens: usize,
        completion_tokens: usize,
    ) -> f64 {
        let prov_lower = provider.to_lowercase();
        let model_lower = model.to_lowercase();

        // Rates per 1,000,000 tokens (Input USD, Output USD)
        let (input_rate, output_rate) =
            if prov_lower.contains("ollama") || prov_lower.contains("local") {
                (0.0, 0.0)
            } else if model_lower.contains("claude-3-5-haiku") || model_lower.contains("haiku") {
                (0.80, 4.00)
            } else if model_lower.contains("claude") {
                (3.00, 15.00)
            } else if model_lower.contains("gpt-4o-mini") {
                (0.15, 0.60)
            } else if model_lower.contains("gpt-4o") {
                (2.50, 10.00)
            } else if model_lower.contains("o3-mini") || model_lower.contains("o1") {
                (1.10, 4.40)
            } else if model_lower.contains("gemini-2.5-flash")
                || model_lower.contains("gemini-2.0-flash")
                || model_lower.contains("flash")
            {
                (0.10, 0.40)
            } else if model_lower.contains("gemini") {
                (1.25, 5.00)
            } else if model_lower.contains("deepseek") {
                (0.14, 0.28)
            } else {
                // Default conservative fallback rate
                (1.00, 3.00)
            };

        let prompt_cost = (prompt_tokens as f64 / 1_000_000.0) * input_rate;
        let completion_cost = (completion_tokens as f64 / 1_000_000.0) * output_rate;
        prompt_cost + completion_cost
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
}

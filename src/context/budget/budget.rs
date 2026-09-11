use crate::constants::{
    BUDGET_PRESSURE_HIGH_THRESHOLD, BUDGET_PRESSURE_MODERATE_THRESHOLD, BUDGET_PROGRESS_BAR_WIDTH,
};
use serde::{Deserialize, Serialize};

/// Dynamic token context budget and headroom tracker for active model sessions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextBudget {
    /// Active tokens residing in current conversation history
    pub used_tokens: usize,
    /// Maximum effective token context window for the active model
    pub max_tokens: usize,
    /// Cumulative tokens expended across all turns in the session
    pub cumulative_session_tokens: usize,
}

impl ContextBudget {
    /// Creates a new ContextBudget tracker instance.
    #[must_use]
    pub fn new(used_tokens: usize, max_tokens: usize, cumulative_session_tokens: usize) -> Self {
        Self {
            used_tokens,
            max_tokens: max_tokens.max(1),
            cumulative_session_tokens,
        }
    }

    /// Computes context window utilization percentage (0.0% to 100.0%+).
    #[must_use]
    pub fn percentage(&self) -> f64 {
        (self.used_tokens as f64 / self.max_tokens as f64) * 100.0
    }

    /// Computes remaining token headroom before reaching model limit.
    #[must_use]
    pub fn headroom_tokens(&self) -> usize {
        self.max_tokens.saturating_sub(self.used_tokens)
    }

    /// Renders an intuitive visual progress bar using block characters (e.g. `[████░░░░░░░░░░░░░░░░]`).
    #[must_use]
    pub fn render_progress_bar(&self, width: usize) -> String {
        if width == 0 {
            return "[]".to_string();
        }
        let pct = (self.percentage() / 100.0).clamp(0.0, 1.0);
        let filled = ((width as f64) * pct).round() as usize;
        let filled = filled.min(width);
        let empty = width.saturating_sub(filled);

        let mut bar = String::with_capacity(width + 2);
        bar.push('[');
        for _ in 0..filled {
            bar.push('█');
        }
        for _ in 0..empty {
            bar.push('░');
        }
        bar.push(']');
        bar
    }

    /// Generates contextual advice based on current token pressure.
    #[must_use]
    pub fn advice(&self) -> &'static str {
        let pct = self.percentage();
        if pct >= BUDGET_PRESSURE_HIGH_THRESHOLD {
            "CRITICAL: Context limit approaching (>80%). Keep answers concise, avoid large file dumps, wrap up pending objectives."
        } else if pct >= BUDGET_PRESSURE_MODERATE_THRESHOLD {
            "WARNING: Moderate context pressure (>60%). Prefer targeted reads and concise tool invocations."
        } else {
            "HEALTHY: Ample context headroom available."
        }
    }

    /// Generates the XML prompt block for injection into `<workspace_context>`.
    #[must_use]
    pub fn to_prompt_block(&self) -> String {
        let pct = self.percentage();
        let bar = self.render_progress_bar(BUDGET_PROGRESS_BAR_WIDTH);
        let headroom = self.headroom_tokens();
        let advice_msg = self.advice();

        format!(
            "  <context_budget used=\"{}\" limit=\"{}\" pct=\"{:.1}%\" headroom=\"{}\" cumulative=\"{}\">\n    {} {:.1}% ({}/{} tokens) | Headroom: {} tokens | Session Total: {}\n    Note: {}\n  </context_budget>\n",
            self.used_tokens,
            self.max_tokens,
            pct,
            headroom,
            self.cumulative_session_tokens,
            bar,
            pct,
            self.used_tokens,
            self.max_tokens,
            headroom,
            self.cumulative_session_tokens,
            advice_msg
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_budget_percentage_and_headroom() {
        let budget = ContextBudget::new(32_000, 128_000, 50_000);
        assert_eq!(budget.headroom_tokens(), 96_000);
        assert!((budget.percentage() - 25.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_progress_bar_rendering() {
        let budget_empty = ContextBudget::new(0, 100_000, 0);
        assert_eq!(budget_empty.render_progress_bar(10), "[░░░░░░░░░░]");

        let budget_half = ContextBudget::new(50_000, 100_000, 50_000);
        assert_eq!(budget_half.render_progress_bar(10), "[█████░░░░░]");

        let budget_full = ContextBudget::new(100_000, 100_000, 120_000);
        assert_eq!(budget_full.render_progress_bar(10), "[██████████]");
    }

    #[test]
    fn test_advice_thresholds() {
        let budget_low = ContextBudget::new(10_000, 100_000, 10_000);
        assert!(budget_low.advice().contains("HEALTHY"));

        let budget_mid = ContextBudget::new(65_000, 100_000, 65_000);
        assert!(budget_mid.advice().contains("WARNING"));

        let budget_high = ContextBudget::new(85_000, 100_000, 85_000);
        assert!(budget_high.advice().contains("CRITICAL"));
    }

    #[test]
    fn test_prompt_block_formatting() {
        let budget = ContextBudget::new(20_000, 100_000, 40_000);
        let block = budget.to_prompt_block();
        assert!(block.contains("<context_budget used=\"20000\" limit=\"100000\""));
        assert!(block.contains("pct=\"20.0%\""));
        assert!(block.contains("headroom=\"80000\""));
        assert!(block.contains("cumulative=\"40000\""));
        assert!(block.contains("HEALTHY: Ample context headroom"));
    }
}

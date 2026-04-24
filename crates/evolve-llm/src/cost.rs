//! In-memory token cost tracker. Purely observational — no enforcement.

use crate::TokenUsage;
use std::sync::atomic::{AtomicU64, Ordering};

/// Price table entry. Units: **micro-cents per token**, where 1 cent = 10 000
/// micro-cents. Stored this way so per-token values are integers even for
/// cheap models.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Price {
    /// Micro-cents per input token.
    pub input_per_token: u64,
    /// Micro-cents per output token.
    pub output_per_token: u64,
}

impl Price {
    /// Claude Haiku 4.5 public pricing: $0.25/M input, $1.25/M output.
    ///
    /// $0.25/M tokens = 25 cents/M = 250 000 micro-cents/M = 25 micro-cents per 100 tokens
    /// = 0.25 micro-cents per token. We scale the *whole* per-M price up by 1e6 so the
    /// per-token figure is integer: **2.5 micro-cents/token** for input → stored as 2.
    /// For accuracy we use **$0.25/M = 2 500 micro-cents per 10 000 tokens**, i.e. we
    /// charge in chunks. Simpler: bill on 1 000 tokens at a time by keeping the per-
    /// 1M ratio. See `record_with_price` below.
    pub const HAIKU: Self = Self {
        // Using micro-cents per million tokens for precision, then dividing.
        // $0.25/M = 25 cents/M = 250_000 micro-cents/M.
        input_per_token: 250_000,
        // $1.25/M = 125 cents/M = 1_250_000 micro-cents/M.
        output_per_token: 1_250_000,
    };

    /// Ollama runs locally — free.
    pub const OLLAMA: Self = Self {
        input_per_token: 0,
        output_per_token: 0,
    };
}

/// Accumulates token usage and cost across calls. Thread-safe.
///
/// Cost is stored as **micro-cents** (1 cent = 10 000 micro-cents). The scaling
/// factor is chosen so a single Haiku session (~1 000 tokens) costs on the
/// order of a few hundred micro-cents — integer math without losing precision.
#[derive(Debug, Default)]
pub struct CostTracker {
    input_tokens: AtomicU64,
    output_tokens: AtomicU64,
    /// Accumulated cost, scaled: `tokens * price.input_per_token / 1_000_000`.
    /// We keep the un-divided product here and divide on read.
    scaled_cost_micro_cents: AtomicU64,
}

impl CostTracker {
    /// Fresh tracker at zero.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record one call's usage under the given price table.
    pub fn record(&self, usage: TokenUsage, price: Price) {
        self.input_tokens
            .fetch_add(usage.input as u64, Ordering::Relaxed);
        self.output_tokens
            .fetch_add(usage.output as u64, Ordering::Relaxed);
        // price fields are micro-cents per million tokens.
        // product = tokens * (mc/M) -> mc * tokens / M.
        let product = (usage.input as u64).saturating_mul(price.input_per_token)
            + (usage.output as u64).saturating_mul(price.output_per_token);
        self.scaled_cost_micro_cents
            .fetch_add(product, Ordering::Relaxed);
    }

    /// Total cost so far, in **micro-cents** (integer). Divide by 10 000 for cents.
    pub fn spent_micro_cents(&self) -> u64 {
        // Divide off the per-million scaling.
        self.scaled_cost_micro_cents.load(Ordering::Relaxed) / 1_000_000
    }

    /// Total input tokens accumulated.
    pub fn input_tokens(&self) -> u64 {
        self.input_tokens.load(Ordering::Relaxed)
    }

    /// Total output tokens accumulated.
    pub fn output_tokens(&self) -> u64 {
        self.output_tokens.load(Ordering::Relaxed)
    }

    /// Emit a `tracing::info!` line summarizing accumulated cost.
    pub fn log_session(&self) {
        tracing::info!(
            target: "evolve::cost",
            input_tokens = self.input_tokens(),
            output_tokens = self.output_tokens(),
            micro_cents = self.spent_micro_cents(),
            "evolve llm usage"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_twice_accumulates() {
        let tracker = CostTracker::new();
        tracker.record(
            TokenUsage {
                input: 100,
                output: 50,
            },
            Price::HAIKU,
        );
        tracker.record(
            TokenUsage {
                input: 200,
                output: 75,
            },
            Price::HAIKU,
        );
        assert_eq!(tracker.input_tokens(), 300);
        assert_eq!(tracker.output_tokens(), 125);
    }

    #[test]
    fn haiku_cost_math_matches_hand_calc() {
        let tracker = CostTracker::new();
        // 1_000_000 input tokens at $0.25/M = 25 cents = 250_000 micro-cents.
        tracker.record(
            TokenUsage {
                input: 1_000_000,
                output: 0,
            },
            Price::HAIKU,
        );
        assert_eq!(tracker.spent_micro_cents(), 250_000);

        let tracker2 = CostTracker::new();
        // 1_000_000 output tokens at $1.25/M = 125 cents = 1_250_000 micro-cents.
        tracker2.record(
            TokenUsage {
                input: 0,
                output: 1_000_000,
            },
            Price::HAIKU,
        );
        assert_eq!(tracker2.spent_micro_cents(), 1_250_000);
    }

    #[test]
    fn typical_challenger_call_cost_is_under_one_cent() {
        // Realistic: ~500 input + 200 output tokens per challenger generation.
        let tracker = CostTracker::new();
        tracker.record(
            TokenUsage {
                input: 500,
                output: 200,
            },
            Price::HAIKU,
        );
        // 500 * 250_000 / 1_000_000 = 125 mc input
        // 200 * 1_250_000 / 1_000_000 = 250 mc output
        // total = 375 micro-cents = 0.0375 cents = less than a penny.
        assert_eq!(tracker.spent_micro_cents(), 375);
    }

    #[test]
    fn ollama_records_zero_cost() {
        let tracker = CostTracker::new();
        tracker.record(
            TokenUsage {
                input: 10_000,
                output: 10_000,
            },
            Price::OLLAMA,
        );
        assert_eq!(tracker.spent_micro_cents(), 0);
        assert_eq!(tracker.input_tokens(), 10_000);
        assert_eq!(tracker.output_tokens(), 10_000);
    }
}

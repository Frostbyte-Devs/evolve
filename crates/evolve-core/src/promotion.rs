//! Signal aggregation + Bayesian champion-vs-challenger promotion math.
//!
//! Pure functions, no I/O. Callers (CLI, adapters) translate
//! `evolve_storage::signals::Signal` rows into [`SignalInput`] before calling
//! into this module.

use rand::Rng;
use rand_distr::{Beta, Distribution};

/// Whether a signal was contributed explicitly by the user
/// (`evolve good`/`bad`/`thumbs`) or inferred implicitly by an adapter
/// from the session log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalKind {
    /// User explicitly graded the session.
    Explicit,
    /// Inferred from adapter session log.
    Implicit,
}

/// One normalized fitness signal feeding into aggregation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SignalInput {
    /// Source category -- controls weighting.
    pub kind: SignalKind,
    /// Score in `[0.0, 1.0]`. Out-of-range values are clamped before weighting.
    pub value: f64,
}

/// Per-kind weights used by the aggregator.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AggregationConfig {
    /// Weight applied to explicit user signals. Default 5.0.
    pub explicit_weight: f64,
    /// Weight applied to adapter-inferred implicit signals. Default 1.0.
    pub implicit_weight: f64,
}

impl Default for AggregationConfig {
    fn default() -> Self {
        Self {
            explicit_weight: 5.0,
            implicit_weight: 1.0,
        }
    }
}

impl SignalInput {
    /// Weight this signal carries under the given aggregation config.
    pub fn weight(&self, config: &AggregationConfig) -> f64 {
        match self.kind {
            SignalKind::Explicit => config.explicit_weight,
            SignalKind::Implicit => config.implicit_weight,
        }
    }
}

/// Collapse a session's signals into a single fitness score in `[0.0, 1.0]`.
///
/// Uses the weighted arithmetic mean. Values are clamped to `[0.0, 1.0]`
/// before weighting. Empty input returns `0.5` (neutral prior).
pub fn aggregate(signals: &[SignalInput], config: &AggregationConfig) -> f64 {
    if signals.is_empty() {
        return 0.5;
    }
    let mut numerator = 0.0;
    let mut denominator = 0.0;
    for s in signals {
        let w = s.weight(config);
        numerator += w * s.value.clamp(0.0, 1.0);
        denominator += w;
    }
    (numerator / denominator).clamp(0.0, 1.0)
}

/// Count scores >= 0.5 as wins, the rest as losses.
fn wins_losses(scores: &[f64]) -> (u32, u32) {
    let wins: u32 = scores.iter().filter(|&&s| s >= 0.5).count() as u32;
    let losses = scores.len() as u32 - wins;
    (wins, losses)
}

/// Monte Carlo estimate of `P(challenger > champion)` under beta-binomial
/// posteriors: `Beta(1 + wins, 1 + losses)` per arm (uniform Jeffreys-like prior).
///
/// Each session score is binarized at 0.5 before counting.
pub fn posterior_probability<R: Rng>(
    champion_scores: &[f64],
    challenger_scores: &[f64],
    samples: u32,
    rng: &mut R,
) -> f64 {
    let (cw, cl) = wins_losses(champion_scores);
    let (hw, hl) = wins_losses(challenger_scores);
    let champ = Beta::new(1.0 + cw as f64, 1.0 + cl as f64).expect("valid Beta params");
    let chall = Beta::new(1.0 + hw as f64, 1.0 + hl as f64).expect("valid Beta params");
    let mut hits: u32 = 0;
    for _ in 0..samples {
        let a: f64 = champ.sample(rng);
        let b: f64 = chall.sample(rng);
        if b > a {
            hits += 1;
        }
    }
    hits as f64 / samples as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_weights_are_five_to_one() {
        let cfg = AggregationConfig::default();
        assert_eq!(cfg.explicit_weight, 5.0);
        assert_eq!(cfg.implicit_weight, 1.0);
    }

    #[test]
    fn explicit_signal_weighs_five_times_implicit() {
        let cfg = AggregationConfig::default();
        let e = SignalInput {
            kind: SignalKind::Explicit,
            value: 1.0,
        };
        let i = SignalInput {
            kind: SignalKind::Implicit,
            value: 1.0,
        };
        assert_eq!(e.weight(&cfg) / i.weight(&cfg), 5.0);
    }

    #[test]
    fn aggregate_empty_returns_neutral_half() {
        assert_eq!(aggregate(&[], &AggregationConfig::default()), 0.5);
    }

    #[test]
    fn aggregate_single_explicit_1_is_1() {
        let signals = [SignalInput {
            kind: SignalKind::Explicit,
            value: 1.0,
        }];
        assert_eq!(aggregate(&signals, &AggregationConfig::default()), 1.0);
    }

    #[test]
    fn aggregate_single_implicit_0_is_0() {
        let signals = [SignalInput {
            kind: SignalKind::Implicit,
            value: 0.0,
        }];
        assert_eq!(aggregate(&signals, &AggregationConfig::default()), 0.0);
    }

    #[test]
    fn aggregate_clips_out_of_range_values() {
        let signals = [SignalInput {
            kind: SignalKind::Implicit,
            value: 2.0,
        }];
        assert_eq!(aggregate(&signals, &AggregationConfig::default()), 1.0);
    }

    #[test]
    fn aggregate_weighted_mean_matches_hand_calculation() {
        // 1 explicit at 0.0 (weight 5) + 2 implicit at 1.0 (weight 1 each)
        // weighted mean = (5*0 + 1*1 + 1*1) / (5 + 1 + 1) = 2/7
        let signals = [
            SignalInput {
                kind: SignalKind::Explicit,
                value: 0.0,
            },
            SignalInput {
                kind: SignalKind::Implicit,
                value: 1.0,
            },
            SignalInput {
                kind: SignalKind::Implicit,
                value: 1.0,
            },
        ];
        let got = aggregate(&signals, &AggregationConfig::default());
        assert!((got - 2.0 / 7.0).abs() < 1e-9, "got {got}");
    }

    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;

    fn seeded_rng() -> ChaCha8Rng {
        ChaCha8Rng::seed_from_u64(42)
    }

    #[test]
    fn posterior_obvious_challenger_win_exceeds_threshold() {
        let champion: Vec<f64> = (0..20).map(|i| if i < 5 { 1.0 } else { 0.0 }).collect();
        let challenger: Vec<f64> = (0..20).map(|i| if i < 18 { 1.0 } else { 0.0 }).collect();
        let p = posterior_probability(&champion, &challenger, 10_000, &mut seeded_rng());
        assert!(p > 0.95, "expected P(chall > champ) > 0.95, got {p}");
    }

    #[test]
    fn posterior_obvious_champion_win_stays_below_threshold() {
        let champion: Vec<f64> = (0..20).map(|i| if i < 18 { 1.0 } else { 0.0 }).collect();
        let challenger: Vec<f64> = (0..20).map(|i| if i < 5 { 1.0 } else { 0.0 }).collect();
        let p = posterior_probability(&champion, &challenger, 10_000, &mut seeded_rng());
        assert!(p < 0.05, "expected P(chall > champ) < 0.05, got {p}");
    }

    #[test]
    fn posterior_tied_evidence_stays_near_half() {
        let champion: Vec<f64> = (0..40).map(|i| if i < 20 { 1.0 } else { 0.0 }).collect();
        let challenger: Vec<f64> = (0..40).map(|i| if i < 20 { 1.0 } else { 0.0 }).collect();
        let p = posterior_probability(&champion, &challenger, 10_000, &mut seeded_rng());
        assert!((p - 0.5).abs() < 0.05, "expected P near 0.5, got {p}");
    }

    #[test]
    fn posterior_is_deterministic_under_same_seed() {
        let champion: Vec<f64> = (0..10).map(|_| 0.6).collect();
        let challenger: Vec<f64> = (0..10).map(|_| 0.7).collect();
        let p1 = posterior_probability(&champion, &challenger, 5_000, &mut seeded_rng());
        let p2 = posterior_probability(&champion, &challenger, 5_000, &mut seeded_rng());
        assert_eq!(p1, p2);
    }

    #[test]
    fn aggregate_single_explicit_dominates_many_implicit() {
        let signals = [
            SignalInput {
                kind: SignalKind::Explicit,
                value: 0.0,
            },
            SignalInput {
                kind: SignalKind::Implicit,
                value: 1.0,
            },
            SignalInput {
                kind: SignalKind::Implicit,
                value: 1.0,
            },
            SignalInput {
                kind: SignalKind::Implicit,
                value: 1.0,
            },
        ];
        let got = aggregate(&signals, &AggregationConfig::default());
        // (0*5 + 1*1 + 1*1 + 1*1) / (5+1+1+1) = 3/8 = 0.375, below 0.5 threshold
        assert!(
            got < 0.5,
            "explicit 0.0 should pull aggregate below 0.5, got {got}",
        );
    }
}

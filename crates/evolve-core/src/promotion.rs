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

/// Configuration for [`promotion_decision`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PromotionConfig {
    /// Minimum sessions required in each arm before any decision is made.
    pub min_sessions_per_arm: usize,
    /// Posterior threshold above which the challenger is promoted.
    pub promote_threshold: f64,
    /// Monte Carlo sample count for [`posterior_probability`].
    pub mc_samples: u32,
}

impl Default for PromotionConfig {
    fn default() -> Self {
        Self {
            min_sessions_per_arm: 20,
            promote_threshold: 0.95,
            mc_samples: 10_000,
        }
    }
}

/// Outcome of a promotion evaluation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Decision {
    /// At least one arm has too few sessions to decide yet.
    NeedMoreData {
        /// Sessions in the thinner arm.
        sessions_each: usize,
        /// Minimum required per arm.
        required: usize,
    },
    /// Enough data, but posterior below threshold. Keep running.
    Hold {
        /// Current estimated `P(challenger > champion)`.
        posterior: f64,
    },
    /// Promote: posterior crossed threshold.
    Promote {
        /// Current estimated `P(challenger > champion)`.
        posterior: f64,
    },
}

/// Evaluate whether the challenger should be promoted, held, or needs more data.
pub fn promotion_decision<R: Rng>(
    champion_scores: &[f64],
    challenger_scores: &[f64],
    config: &PromotionConfig,
    rng: &mut R,
) -> Decision {
    let champ_n = champion_scores.len();
    let chall_n = challenger_scores.len();
    if champ_n < config.min_sessions_per_arm || chall_n < config.min_sessions_per_arm {
        return Decision::NeedMoreData {
            sessions_each: champ_n.min(chall_n),
            required: config.min_sessions_per_arm,
        };
    }
    let posterior =
        posterior_probability(champion_scores, challenger_scores, config.mc_samples, rng);
    if posterior >= config.promote_threshold {
        Decision::Promote { posterior }
    } else {
        Decision::Hold { posterior }
    }
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
    fn decision_needs_more_data_when_either_arm_is_thin() {
        let champion: Vec<f64> = vec![1.0; 5];
        let challenger: Vec<f64> = vec![0.0; 20];
        let cfg = PromotionConfig::default();
        let d = promotion_decision(&champion, &challenger, &cfg, &mut seeded_rng());
        assert!(matches!(d, Decision::NeedMoreData { .. }));
    }

    #[test]
    fn decision_promotes_obvious_winner() {
        let champion: Vec<f64> = (0..25).map(|i| if i < 5 { 1.0 } else { 0.0 }).collect();
        let challenger: Vec<f64> = (0..25).map(|i| if i < 23 { 1.0 } else { 0.0 }).collect();
        let cfg = PromotionConfig::default();
        let d = promotion_decision(&champion, &challenger, &cfg, &mut seeded_rng());
        match d {
            Decision::Promote { posterior } => {
                assert!(
                    posterior >= cfg.promote_threshold,
                    "posterior {posterior} below threshold {}",
                    cfg.promote_threshold,
                );
            }
            other => panic!("expected Promote, got {other:?}"),
        }
    }

    #[test]
    fn decision_holds_when_evidence_is_tied() {
        let champion: Vec<f64> = (0..30).map(|i| if i < 15 { 1.0 } else { 0.0 }).collect();
        let challenger: Vec<f64> = (0..30).map(|i| if i < 15 { 1.0 } else { 0.0 }).collect();
        let cfg = PromotionConfig::default();
        let d = promotion_decision(&champion, &challenger, &cfg, &mut seeded_rng());
        assert!(matches!(d, Decision::Hold { .. }));
    }

    #[test]
    fn decision_finishes_in_reasonable_time_for_realistic_input() {
        let champion: Vec<f64> = (0..100).map(|i| if i < 60 { 1.0 } else { 0.0 }).collect();
        let challenger: Vec<f64> = (0..100).map(|i| if i < 70 { 1.0 } else { 0.0 }).collect();
        let cfg = PromotionConfig::default();
        let mut r = seeded_rng();

        let start = std::time::Instant::now();
        let _ = promotion_decision(&champion, &challenger, &cfg, &mut r);
        let elapsed = start.elapsed();

        // Generous cap: budget is ~1ms in release, debug builds on slow CI
        // can swell this 5-10x. Failing here is a real red flag, not a flake.
        assert!(
            elapsed.as_millis() < 50,
            "promotion_decision took {elapsed:?}; expected < 50ms",
        );
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

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;

    fn arb_scores(max_n: usize) -> impl Strategy<Value = Vec<f64>> {
        prop::collection::vec(prop_oneof![Just(0.0_f64), Just(1.0_f64)], 0..max_n)
    }

    fn arb_signal() -> impl Strategy<Value = SignalInput> {
        (prop::bool::ANY, -10.0_f64..10.0_f64).prop_map(|(is_explicit, v)| SignalInput {
            kind: if is_explicit {
                SignalKind::Explicit
            } else {
                SignalKind::Implicit
            },
            value: v,
        })
    }

    proptest! {
        /// `aggregate` always returns a value in `[0.0, 1.0]` for any inputs.
        #[test]
        fn aggregate_is_in_unit_interval(
            signals in prop::collection::vec(arb_signal(), 0..50),
        ) {
            let out = aggregate(&signals, &AggregationConfig::default());
            prop_assert!((0.0..=1.0).contains(&out), "got {out}");
        }

        /// `promotion_decision` never returns `Promote` with a posterior below threshold.
        #[test]
        fn decision_never_promotes_below_threshold(
            champion in arb_scores(60),
            challenger in arb_scores(60),
        ) {
            let cfg = PromotionConfig::default();
            let mut r = ChaCha8Rng::seed_from_u64(1);
            let d = promotion_decision(&champion, &challenger, &cfg, &mut r);
            if let Decision::Promote { posterior } = d {
                prop_assert!(posterior >= cfg.promote_threshold);
            }
        }

        /// Posterior probability is always in `[0.0, 1.0]`.
        #[test]
        fn posterior_is_in_unit_interval(
            champion in arb_scores(50),
            challenger in arb_scores(50),
        ) {
            let mut r = ChaCha8Rng::seed_from_u64(7);
            let p = posterior_probability(&champion, &challenger, 1_000, &mut r);
            prop_assert!((0.0..=1.0).contains(&p), "got {p}");
        }
    }
}

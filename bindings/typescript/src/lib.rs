//! TypeScript bindings for Evolve. Builds via `@napi-rs/cli`.

use evolve_core::promotion::{
    AggregationConfig, Decision, PromotionConfig, SignalInput, SignalKind, aggregate,
    posterior_probability, promotion_decision,
};
use napi::bindgen_prelude::*;
use napi_derive::napi;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

/// Weighted-mean aggregate over `[kind, value]` signal pairs.
#[napi]
pub fn aggregate_signals(signals: Vec<(String, f64)>) -> Result<f64> {
    let parsed: Result<Vec<SignalInput>> = signals
        .into_iter()
        .map(|(kind, value)| {
            let kind = match kind.as_str() {
                "explicit" => SignalKind::Explicit,
                "implicit" => SignalKind::Implicit,
                other => return Err(Error::from_reason(format!("unknown kind: {other}"))),
            };
            Ok(SignalInput { kind, value })
        })
        .collect();
    let parsed = parsed?;
    Ok(aggregate(&parsed, &AggregationConfig::default()))
}

/// Monte Carlo estimate of P(challenger > champion).
#[napi]
pub fn posterior(
    champion: Vec<f64>,
    challenger: Vec<f64>,
    samples: u32,
    seed: BigInt,
) -> f64 {
    let seed_u64 = seed.get_u64().1;
    let mut rng = ChaCha8Rng::seed_from_u64(seed_u64);
    posterior_probability(&champion, &challenger, samples, &mut rng)
}

/// Promotion decision; returns `{ outcome, posterior, sessionsEach }`.
#[napi(object)]
pub struct DecisionResult {
    pub outcome: String,
    pub posterior: f64,
    pub sessions_each: u32,
}

#[napi]
pub fn promote(champion: Vec<f64>, challenger: Vec<f64>, seed: BigInt) -> DecisionResult {
    let seed_u64 = seed.get_u64().1;
    let mut rng = ChaCha8Rng::seed_from_u64(seed_u64);
    let cfg = PromotionConfig::default();
    let d = promotion_decision(&champion, &challenger, &cfg, &mut rng);
    match d {
        Decision::NeedMoreData { sessions_each, .. } => DecisionResult {
            outcome: "need_more_data".into(),
            posterior: 0.0,
            sessions_each: sessions_each as u32,
        },
        Decision::Hold { posterior } => DecisionResult {
            outcome: "hold".into(),
            posterior,
            sessions_each: champion.len().min(challenger.len()) as u32,
        },
        Decision::Promote { posterior } => DecisionResult {
            outcome: "promote".into(),
            posterior,
            sessions_each: champion.len().min(challenger.len()) as u32,
        },
    }
}

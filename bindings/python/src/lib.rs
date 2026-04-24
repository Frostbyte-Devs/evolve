//! Python bindings for Evolve. Builds via `maturin build`.

use evolve_core::promotion::{
    AggregationConfig, Decision, PromotionConfig, SignalInput, SignalKind, aggregate,
    posterior_probability, promotion_decision,
};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

/// Weighted-mean aggregate over a list of `(kind, value)` signal tuples.
///
/// `kind` is the string `"explicit"` or `"implicit"`.
#[pyfunction]
fn py_aggregate(signals: Vec<(String, f64)>) -> PyResult<f64> {
    let parsed: Result<Vec<SignalInput>, _> = signals
        .into_iter()
        .map(|(kind, value)| {
            let kind = match kind.as_str() {
                "explicit" => SignalKind::Explicit,
                "implicit" => SignalKind::Implicit,
                other => {
                    return Err(PyValueError::new_err(format!("unknown kind: {other}")));
                }
            };
            Ok(SignalInput { kind, value })
        })
        .collect();
    let parsed = parsed?;
    Ok(aggregate(&parsed, &AggregationConfig::default()))
}

/// Monte Carlo estimate of `P(challenger > champion)`.
#[pyfunction]
fn py_posterior_probability(
    champion: Vec<f64>,
    challenger: Vec<f64>,
    samples: u32,
    seed: u64,
) -> f64 {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    posterior_probability(&champion, &challenger, samples, &mut rng)
}

/// Full promotion decision. Returns a 3-tuple `(outcome, posterior, sessions_each)`.
/// `outcome` is one of `"need_more_data"`, `"hold"`, `"promote"`.
#[pyfunction]
fn py_promotion_decision(
    champion: Vec<f64>,
    challenger: Vec<f64>,
    seed: u64,
) -> (String, f64, usize) {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let cfg = PromotionConfig::default();
    match promotion_decision(&champion, &challenger, &cfg, &mut rng) {
        Decision::NeedMoreData {
            sessions_each,
            required: _,
        } => ("need_more_data".into(), 0.0, sessions_each),
        Decision::Hold { posterior } => ("hold".into(), posterior, champion.len().min(challenger.len())),
        Decision::Promote { posterior } => ("promote".into(), posterior, champion.len().min(challenger.len())),
    }
}

/// Python module registered as `evolveai`.
#[pymodule]
fn evolveai(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(py_aggregate, m)?)?;
    m.add_function(wrap_pyfunction!(py_posterior_probability, m)?)?;
    m.add_function(wrap_pyfunction!(py_promotion_decision, m)?)?;
    Ok(())
}

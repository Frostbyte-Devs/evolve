//! Signal aggregation + Bayesian champion-vs-challenger promotion math.
//!
//! Pure functions, no I/O. Callers (CLI, adapters) translate
//! `evolve_storage::signals::Signal` rows into [`SignalInput`] before calling
//! into this module.

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
}

//! Genome schema DSL: declarative description of a genome's structure.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// One field in a Genome's schema. Tagged enum so it serializes as JSON
/// like `{"type": "float", "range": [0.0, 2.0], "sigma": 0.2}`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FieldSchema {
    /// A free-form string with a designated mutator strategy.
    String {
        /// The strategy for mutating this string field.
        mutator: StringMutatorKind,
    },
    /// A floating-point value bounded by `range`, mutated with Gaussian noise of std-dev `sigma`.
    Float {
        /// Inclusive lower and upper bounds.
        range: (f64, f64),
        /// Standard deviation of the Gaussian mutation kernel.
        sigma: f64,
    },
    /// An integer value bounded by `range`, mutated with rounded Gaussian noise.
    Integer {
        /// Inclusive lower and upper bounds.
        range: (i64, i64),
        /// Standard deviation of the mutation kernel before rounding.
        sigma: f64,
    },
    /// One-of value, chosen from `choices`.
    Categorical {
        /// The set of allowed values.
        choices: Vec<String>,
    },
    /// A subset of values drawn from `pool`, optionally capped at `max` elements.
    Set {
        /// The universe of allowed values.
        pool: Vec<String>,
        /// Optional cap on subset size.
        max: Option<usize>,
    },
}

/// How a `String`-typed field is mutated.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum StringMutatorKind {
    /// Use a cheap LLM to rewrite the prompt with small variations.
    LlmRewrite,
    /// Treat the prompt as a template with named slots; mutation swaps a slot's value
    /// for another from the slot's pool.
    TemplateSlot {
        /// Map of slot-name → list of candidate values for that slot.
        slots: BTreeMap<String, Vec<String>>,
    },
}

/// A complete declarative description of a Genome's structure.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GenomeSchema {
    /// All fields in the genome, keyed by field name.
    pub fields: BTreeMap<String, FieldSchema>,
    /// Per-field mutation rates (probability that this field gets mutated per generation).
    /// Each value must be in `[0.0, 1.0]` and reference a field that exists in `fields`.
    pub mutation_rates: BTreeMap<String, f64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_roundtrips_through_json() {
        let schema = GenomeSchema {
            fields: BTreeMap::from([(
                "temperature".to_string(),
                FieldSchema::Float {
                    range: (0.0, 2.0),
                    sigma: 0.2,
                },
            )]),
            mutation_rates: BTreeMap::from([("temperature".to_string(), 0.1)]),
        };
        let json = serde_json::to_string(&schema).unwrap();
        let back: GenomeSchema = serde_json::from_str(&json).unwrap();
        assert_eq!(schema, back);
    }
}

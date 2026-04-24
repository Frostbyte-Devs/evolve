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

/// Errors returned by [`GenomeSchema::validate`] when a schema is malformed.
#[derive(thiserror::Error, Debug, PartialEq)]
pub enum SchemaError {
    /// A `Float` field has a `range` where lo > hi.
    #[error("field {0}: float range inverted ({1} > {2})")]
    InvertedFloatRange(String, f64, f64),
    /// An `Integer` field has a `range` where lo > hi.
    #[error("field {0}: integer range inverted ({1} > {2})")]
    InvertedIntegerRange(String, i64, i64),
    /// A `Categorical` field has no choices to pick from.
    #[error("field {0}: categorical has no choices")]
    EmptyCategorical(String),
    /// A `Set` field has an empty pool.
    #[error("field {0}: set pool is empty")]
    EmptySetPool(String),
    /// `mutation_rates` references a field name not present in `fields`.
    #[error("mutation rate references unknown field {0}")]
    UnknownMutationRateField(String),
    /// A mutation rate is outside the `[0.0, 1.0]` interval.
    #[error("field {0}: mutation rate {1} not in [0.0, 1.0]")]
    MutationRateOutOfRange(String, f64),
}

impl GenomeSchema {
    /// Validate the schema's internal consistency.
    ///
    /// Checks:
    /// - `Float`/`Integer` ranges are not inverted (lo <= hi)
    /// - `Categorical` choices and `Set` pools are non-empty
    /// - Every key in `mutation_rates` references a field that exists in `fields`
    /// - Every value in `mutation_rates` is in `[0.0, 1.0]`
    pub fn validate(&self) -> Result<(), SchemaError> {
        for (name, field) in &self.fields {
            match field {
                FieldSchema::Float {
                    range: (lo, hi), ..
                } if lo > hi => {
                    return Err(SchemaError::InvertedFloatRange(name.clone(), *lo, *hi));
                }
                FieldSchema::Integer {
                    range: (lo, hi), ..
                } if lo > hi => {
                    return Err(SchemaError::InvertedIntegerRange(name.clone(), *lo, *hi));
                }
                FieldSchema::Categorical { choices } if choices.is_empty() => {
                    return Err(SchemaError::EmptyCategorical(name.clone()));
                }
                FieldSchema::Set { pool, .. } if pool.is_empty() => {
                    return Err(SchemaError::EmptySetPool(name.clone()));
                }
                _ => {}
            }
        }
        for (name, rate) in &self.mutation_rates {
            if !self.fields.contains_key(name) {
                return Err(SchemaError::UnknownMutationRateField(name.clone()));
            }
            if !(0.0..=1.0).contains(rate) {
                return Err(SchemaError::MutationRateOutOfRange(name.clone(), *rate));
            }
        }
        Ok(())
    }
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

    #[test]
    fn validate_rejects_inverted_float_range() {
        let bad = GenomeSchema {
            fields: BTreeMap::from([(
                "t".to_string(),
                FieldSchema::Float {
                    range: (2.0, 0.0),
                    sigma: 0.1,
                },
            )]),
            mutation_rates: BTreeMap::new(),
        };
        assert!(bad.validate().is_err());
    }

    #[test]
    fn validate_rejects_empty_categorical() {
        let bad = GenomeSchema {
            fields: BTreeMap::from([(
                "model".to_string(),
                FieldSchema::Categorical { choices: vec![] },
            )]),
            mutation_rates: BTreeMap::new(),
        };
        assert!(bad.validate().is_err());
    }

    #[test]
    fn validate_rejects_mutation_rate_for_unknown_field() {
        let bad = GenomeSchema {
            fields: BTreeMap::new(),
            mutation_rates: BTreeMap::from([("nope".to_string(), 0.1)]),
        };
        assert!(bad.validate().is_err());
    }

    #[test]
    fn validate_rejects_mutation_rate_out_of_range() {
        let bad = GenomeSchema {
            fields: BTreeMap::from([(
                "t".to_string(),
                FieldSchema::Float {
                    range: (0.0, 1.0),
                    sigma: 0.1,
                },
            )]),
            mutation_rates: BTreeMap::from([("t".to_string(), 1.5)]),
        };
        assert!(bad.validate().is_err());
    }

    #[test]
    fn validate_accepts_well_formed_schema() {
        let good = GenomeSchema {
            fields: BTreeMap::from([(
                "t".to_string(),
                FieldSchema::Float {
                    range: (0.0, 2.0),
                    sigma: 0.2,
                },
            )]),
            mutation_rates: BTreeMap::from([("t".to_string(), 0.1)]),
        };
        assert!(good.validate().is_ok());
    }
}

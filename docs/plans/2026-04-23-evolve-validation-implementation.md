# Evolve Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build Evolve v1.0 — an evolutionary computation framework for LLM agents, shipping Rust core + Python (PyO3) bindings + TypeScript (napi-rs) bindings + HTTP/gRPC service mode + bundled web dashboard + 3 framework adapters (LangChain, DSPy, Vercel AI).

**Architecture:** Cargo workspace with 7 crates. `evolve-core` is dependency-free engine (genome trait, operators, generation cycle, cost meter, fitness cache, diversity). LLM providers, storage, service, runtime, dashboard, CLI live in separate crates. Bindings wrap `evolve-core` + in-process service. All four integration surfaces share one engine.

**Tech Stack:** Rust 2024 + Tokio, async-openai, candle (sentence-transformers), sqlx (SQLite/Postgres), axum + tonic, leptos OR Vite+React (decided in Phase 14), rust-embed, PyO3 + maturin, napi-rs. See design doc `2026-04-23-evolve-validation-design.md` for full rationale.

**Reference:** All design decisions, trade-offs, and locked choices are in `2026-04-23-evolve-validation-design.md`. This plan implements that design.

**Total realistic timeline:** 35-50 working days for one full-time engineer. Plan is phased so each phase is shippable independently and the project has visible progress at every checkpoint.

---

## Phase Index

| Phase | Scope | Est. Days | Status |
|---|---|---|---|
| 0 | Workspace foundation, CI scaffold | 1-2 | Pending |
| 1 | Genome trait + DefaultGenome + Schema DSL | 2-3 | Pending |
| 2 | Operators (per-axis Mutator/Crosser) | 2-3 | Pending |
| 3 | Cost meter + Fitness cache | 1 | Pending |
| 4 | Provider trait + Ollama backend | 2 | Pending |
| 5 | OpenAI + Anthropic providers | 2 | Pending |
| 6 | Benchmark trait + built-in benchmarks | 1 | Pending |
| 7 | Evaluator + Failure classifier | 1 | Pending |
| 8 | Diversity engine (candle + crowding) | 1-2 | Pending |
| 9 | Population engine (the inner loop) | 2-3 | Pending |
| 10 | Storage layer (sqlx) | 2 | Pending |
| 11 | CLI (evolve run/serve/export) | 1-2 | Pending |
| 12 | HTTP/gRPC service | 2-3 | Pending |
| 13 | OpenAI-compatible runtime | 2 | Pending |
| 14 | Dashboard (SPA + rust-embed) | 3-5 | Pending |
| 15 | Python bindings (PyO3) | 2-3 | Pending |
| 16 | TypeScript bindings (napi-rs) | 2-3 | Pending |
| 17 | Framework adapters | 3 | Pending |
| 18 | Examples (synthetic + GSM8K) | 1-2 | Pending |
| 19 | Documentation + Release | 2-3 | Pending |
| 20 | Public roadmap + community | 1 | Pending |

**Bite-sized expansion policy:** Phases 0, 1, 2 are fully bite-sized below. Phases 3-20 are structured task lists with file paths, test signatures, and key notes. Before starting any phase 3+, re-invoke `superpowers:writing-plans` against that phase's task list to expand it to bite-sized form, OR hand the task list directly to `superpowers:subagent-driven-development` (it can drive fresh subagents per task with the existing detail).

---

# PHASE 0 — Workspace Foundation

**Goal:** Cargo workspace skeleton, license, README, CI scaffold. Anyone can clone and `cargo test` (which passes trivially) on any platform.

## Task 0.1 — Initialize Cargo workspace

**Files:**
- Create: `~/projects/active/evolve/Cargo.toml`
- Create: `~/projects/active/evolve/.gitignore`
- Create: `~/projects/active/evolve/rust-toolchain.toml`

**Step 1: Write `Cargo.toml`**

```toml
[workspace]
resolver = "2"
members = [
    "crates/evolve-core",
]
exclude = []

[workspace.package]
version = "0.1.0"
edition = "2024"
rust-version = "1.83"
authors = ["Kristian Baer <kristianb43r@gmail.com>"]
license = "Apache-2.0"
repository = "https://github.com/NORTHTEKDevs/evolve"
homepage = "https://github.com/NORTHTEKDevs/evolve"
description = "Evolutionary computation for LLM agents"
keywords = ["llm", "agents", "evolution", "genetic-algorithms", "ai"]
categories = ["science", "algorithms"]

[workspace.dependencies]
tokio = { version = "1.42", features = ["full"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
anyhow = "1.0"
thiserror = "2.0"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["json", "env-filter"] }
rand = "0.8"
rand_chacha = "0.3"
async-trait = "0.1"
futures = "0.3"
dashmap = "6.0"
proptest = "1.5"

[profile.release]
lto = "thin"
codegen-units = 1
strip = true
```

**Step 2: Write `.gitignore`**

```gitignore
/target
**/*.rs.bk
*.pdb
.env
.env.local
*.log
/runs/
.DS_Store
node_modules/
__pycache__/
*.egg-info/
.pytest_cache/
dist/
```

**Step 3: Write `rust-toolchain.toml`**

```toml
[toolchain]
channel = "stable"
components = ["rustfmt", "clippy"]
profile = "default"
```

**Step 4: Verify workspace parses**

Run: `cd ~/projects/active/evolve && cargo metadata --format-version=1 > /dev/null`
Expected: exits 0, no error output

**Step 5: Commit**

```bash
cd ~/projects/active/evolve
git add Cargo.toml .gitignore rust-toolchain.toml
git commit -m "chore: initialize Cargo workspace skeleton"
```

## Task 0.2 — Create `evolve-core` crate skeleton

**Files:**
- Create: `crates/evolve-core/Cargo.toml`
- Create: `crates/evolve-core/src/lib.rs`

**Step 1: Write `crates/evolve-core/Cargo.toml`**

```toml
[package]
name = "evolve-core"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
authors.workspace = true
license.workspace = true
repository.workspace = true
description = "Core engine for Evolve: genome trait, operators, generation cycle"

[dependencies]
serde.workspace = true
serde_json.workspace = true
anyhow.workspace = true
thiserror.workspace = true
tracing.workspace = true
rand.workspace = true
rand_chacha.workspace = true
async-trait.workspace = true
futures.workspace = true
dashmap.workspace = true

[dev-dependencies]
proptest.workspace = true
tokio = { workspace = true, features = ["macros", "rt-multi-thread"] }
```

**Step 2: Write `crates/evolve-core/src/lib.rs`**

```rust
//! evolve-core: engine for evolutionary computation on LLM agents.
//!
//! See the workspace README and `docs/plans/2026-04-23-evolve-validation-design.md`
//! for the full architecture.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

#[cfg(test)]
mod tests {
    #[test]
    fn smoke() {
        assert_eq!(2 + 2, 4);
    }
}
```

**Step 3: Run tests**

Run: `cd ~/projects/active/evolve && cargo test -p evolve-core`
Expected: `test tests::smoke ... ok` and `1 passed`

**Step 4: Commit**

```bash
git add crates/evolve-core/
git commit -m "feat(evolve-core): scaffold crate"
```

## Task 0.3 — License + README + CONTRIBUTING

**Files:**
- Create: `LICENSE` (Apache-2.0 full text)
- Create: `README.md`
- Create: `CONTRIBUTING.md`

**Step 1: Write `LICENSE`**

Run: `curl -s https://www.apache.org/licenses/LICENSE-2.0.txt -o LICENSE`

Replace `[yyyy]` and `[name of copyright owner]` placeholders in the resulting boilerplate with `2026` and `Kristian Baer / Northtek`.

Verify: `head -2 LICENSE` shows the Apache header, `wc -l LICENSE` is 202.

**Step 2: Write `README.md`**

```markdown
# Evolve

> Evolutionary computation for LLM agents. Bring your own agent and benchmark — Evolve evolves it.

## Status

Pre-alpha. v1.0 in active development.

## Design

See [`docs/plans/2026-04-23-evolve-validation-design.md`](docs/plans/2026-04-23-evolve-validation-design.md) for the full architecture.

## Quickstart

(coming once Phase 11 ships)

## License

Apache-2.0
```

**Step 3: Write `CONTRIBUTING.md`**

```markdown
# Contributing to Evolve

Thanks for your interest. Evolve is a young project; expect breaking changes
until v1.0 GA.

## Development setup

- Rust stable (see `rust-toolchain.toml` for the pinned version)
- `cargo test --workspace` must pass on your branch

## TDD is mandatory

Every PR adding a public function in `evolve-core` MUST add a corresponding
test in the same diff. CI enforces this.

## Coverage thresholds

- `evolve-core`: ≥80%
- `evolve-providers`: ≥75%
- `evolve-storage`: ≥70%

Use `cargo llvm-cov` locally to check before pushing.
```

**Step 4: Commit**

```bash
git add LICENSE README.md CONTRIBUTING.md
git commit -m "docs: add license, README, and contributing guide"
```

## Task 0.4 — CI scaffold (Rust matrix)

**Files:**
- Create: `.github/workflows/ci.yml`

**Step 1: Write the workflow**

```yaml
name: CI

on:
  push:
    branches: [main]
  pull_request:

jobs:
  rust:
    name: Rust ${{ matrix.toolchain }} on ${{ matrix.os }}
    runs-on: ${{ matrix.os }}
    strategy:
      fail-fast: false
      matrix:
        os: [ubuntu-latest, macos-latest, windows-latest]
        toolchain: [stable]
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy
      - uses: Swatinem/rust-cache@v2
      - run: cargo fmt --all -- --check
      - run: cargo clippy --workspace --all-targets -- -D warnings
      - run: cargo test --workspace --all-features

  coverage:
    name: Coverage
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - uses: taiki-e/install-action@cargo-llvm-cov
      - run: cargo llvm-cov --workspace --lcov --output-path lcov.info
      - uses: codecov/codecov-action@v5
        with:
          files: lcov.info
```

**Step 2: Verify the workflow file is valid YAML**

Run: `python -c "import yaml; yaml.safe_load(open('.github/workflows/ci.yml'))"`
Expected: no error, exits 0.

**Step 3: Commit**

```bash
git add .github/
git commit -m "ci: add Rust workflow with fmt/clippy/test/coverage"
```

## Task 0.5 — Verify foundation works end-to-end

**Step 1:** `cargo fmt --all -- --check` → exits 0
**Step 2:** `cargo clippy --workspace --all-targets -- -D warnings` → exits 0
**Step 3:** `cargo test --workspace` → 1 passed, 0 failed
**Step 4:** `git log --oneline` → shows 4 commits

If all pass, **PHASE 0 COMPLETE.** If anything fails, fix before proceeding.

---

# PHASE 1 — Genome Trait + Default Implementation + Schema DSL

**Goal:** A working `Genome` trait, the `DefaultGenome` 8-axis impl, the `GenomeSchema` DSL types, and serialization for both. All bit-stable across runs given the same seed. Property tests prove it.

## Task 1.1 — Define `GenomeSchema` types (the DSL)

**Files:**
- Create: `crates/evolve-core/src/schema.rs`
- Modify: `crates/evolve-core/src/lib.rs` (add `pub mod schema;`)

**Step 1: Write the failing test**

In `crates/evolve-core/src/schema.rs`:

```rust
//! Genome schema DSL: declarative description of a genome's structure.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FieldSchema {
    String { mutator: StringMutatorKind },
    Float { range: (f64, f64), sigma: f64 },
    Integer { range: (i64, i64), sigma: f64 },
    Categorical { choices: Vec<String> },
    Set { pool: Vec<String>, max: Option<usize> },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum StringMutatorKind {
    LlmRewrite,
    TemplateSlot { slots: BTreeMap<String, Vec<String>> },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GenomeSchema {
    pub fields: BTreeMap<String, FieldSchema>,
    pub mutation_rates: BTreeMap<String, f64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_roundtrips_through_json() {
        let schema = GenomeSchema {
            fields: BTreeMap::from([
                ("temperature".to_string(), FieldSchema::Float { range: (0.0, 2.0), sigma: 0.2 }),
            ]),
            mutation_rates: BTreeMap::from([("temperature".to_string(), 0.1)]),
        };
        let json = serde_json::to_string(&schema).unwrap();
        let back: GenomeSchema = serde_json::from_str(&json).unwrap();
        assert_eq!(schema, back);
    }
}
```

In `crates/evolve-core/src/lib.rs`, add: `pub mod schema;`

**Step 2: Run the test**

Run: `cargo test -p evolve-core schema_roundtrips_through_json`
Expected: PASS

**Step 3: Commit**

```bash
git add crates/evolve-core/src/{lib.rs,schema.rs}
git commit -m "feat(schema): define GenomeSchema DSL with serde roundtrip"
```

## Task 1.2 — `GenomeSchema::validate`

**Files:**
- Modify: `crates/evolve-core/src/schema.rs`

**Step 1: Write the failing tests**

Append to the `tests` module in `schema.rs`:

```rust
#[test]
fn validate_rejects_inverted_float_range() {
    let bad = GenomeSchema {
        fields: BTreeMap::from([
            ("t".to_string(), FieldSchema::Float { range: (2.0, 0.0), sigma: 0.1 }),
        ]),
        mutation_rates: BTreeMap::new(),
    };
    assert!(bad.validate().is_err());
}

#[test]
fn validate_rejects_empty_categorical() {
    let bad = GenomeSchema {
        fields: BTreeMap::from([
            ("model".to_string(), FieldSchema::Categorical { choices: vec![] }),
        ]),
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
        fields: BTreeMap::from([
            ("t".to_string(), FieldSchema::Float { range: (0.0, 1.0), sigma: 0.1 }),
        ]),
        mutation_rates: BTreeMap::from([("t".to_string(), 1.5)]),
    };
    assert!(bad.validate().is_err());
}

#[test]
fn validate_accepts_well_formed_schema() {
    let good = GenomeSchema {
        fields: BTreeMap::from([
            ("t".to_string(), FieldSchema::Float { range: (0.0, 2.0), sigma: 0.2 }),
        ]),
        mutation_rates: BTreeMap::from([("t".to_string(), 0.1)]),
    };
    assert!(good.validate().is_ok());
}
```

**Step 2: Run tests — should fail to compile (no `validate` method)**

Run: `cargo test -p evolve-core schema::`
Expected: compile error: no method `validate` on `GenomeSchema`

**Step 3: Implement `validate`**

Add to `schema.rs`:

```rust
#[derive(thiserror::Error, Debug, PartialEq)]
pub enum SchemaError {
    #[error("field {0}: float range inverted ({1} > {2})")]
    InvertedFloatRange(String, f64, f64),
    #[error("field {0}: integer range inverted ({1} > {2})")]
    InvertedIntegerRange(String, i64, i64),
    #[error("field {0}: categorical has no choices")]
    EmptyCategorical(String),
    #[error("field {0}: set pool is empty")]
    EmptySetPool(String),
    #[error("mutation rate references unknown field {0}")]
    UnknownMutationRateField(String),
    #[error("field {0}: mutation rate {1} not in [0.0, 1.0]")]
    MutationRateOutOfRange(String, f64),
}

impl GenomeSchema {
    pub fn validate(&self) -> Result<(), SchemaError> {
        for (name, field) in &self.fields {
            match field {
                FieldSchema::Float { range: (lo, hi), .. } if lo > hi => {
                    return Err(SchemaError::InvertedFloatRange(name.clone(), *lo, *hi));
                }
                FieldSchema::Integer { range: (lo, hi), .. } if lo > hi => {
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
```

**Step 4: Run all schema tests**

Run: `cargo test -p evolve-core schema::`
Expected: 6 passed

**Step 5: Commit**

```bash
git add crates/evolve-core/src/schema.rs
git commit -m "feat(schema): add GenomeSchema::validate with explicit SchemaError"
```

## Task 1.3 — Define `Genome` trait

**Files:**
- Create: `crates/evolve-core/src/genome.rs`
- Modify: `crates/evolve-core/src/lib.rs` (add `pub mod genome;`)

**Step 1: Write the trait**

```rust
//! The Genome trait — central abstraction for evolvable agents.

use crate::schema::GenomeSchema;
use rand::Rng;
use serde::{de::DeserializeOwned, Serialize};

#[derive(Debug, Clone, Serialize, serde::Deserialize, PartialEq)]
pub struct ProviderConfig {
    pub provider: String,
    pub model: String,
    pub temperature: f64,
    pub top_p: f64,
    pub max_tokens: u32,
}

pub trait Genome: Serialize + DeserializeOwned + Send + Sync + Clone + 'static {
    fn schema() -> GenomeSchema;
    fn random<R: Rng + ?Sized>(rng: &mut R, schema: &GenomeSchema) -> Self;
    fn render_prompt(&self) -> String;
    fn provider_config(&self) -> ProviderConfig;
    fn fingerprint(&self) -> u64;
}
```

In `lib.rs`: add `pub mod genome;`

**Step 2: Verify compiles**

Run: `cargo check -p evolve-core`
Expected: clean.

**Step 3: Commit**

```bash
git add crates/evolve-core/src/{lib.rs,genome.rs}
git commit -m "feat(genome): define Genome trait + ProviderConfig"
```

## Task 1.4 — Implement `DefaultGenome` (8 axes)

**Files:**
- Create: `crates/evolve-core/src/default_genome.rs`
- Modify: `crates/evolve-core/src/lib.rs`

**Step 1: Write failing tests**

```rust
//! Built-in default genome with the 8 standard axes.

use crate::genome::{Genome, ProviderConfig};
use crate::schema::{FieldSchema, GenomeSchema, StringMutatorKind};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DefaultGenome {
    pub system_prompt: String,
    pub model: String,
    pub temperature: f64,
    pub top_p: f64,
    pub max_tokens: u32,
    pub reasoning_style: String,
    pub tools: BTreeSet<String>,
    pub few_shot_ids: BTreeSet<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;

    #[test]
    fn schema_has_eight_fields() {
        let s = DefaultGenome::schema();
        assert_eq!(s.fields.len(), 8);
    }

    #[test]
    fn random_with_same_seed_is_deterministic() {
        let s = DefaultGenome::schema();
        let mut r1 = ChaCha8Rng::seed_from_u64(42);
        let mut r2 = ChaCha8Rng::seed_from_u64(42);
        let g1 = DefaultGenome::random(&mut r1, &s);
        let g2 = DefaultGenome::random(&mut r2, &s);
        assert_eq!(g1, g2);
    }

    #[test]
    fn random_with_different_seeds_differs() {
        let s = DefaultGenome::schema();
        let mut r1 = ChaCha8Rng::seed_from_u64(1);
        let mut r2 = ChaCha8Rng::seed_from_u64(2);
        let g1 = DefaultGenome::random(&mut r1, &s);
        let g2 = DefaultGenome::random(&mut r2, &s);
        assert_ne!(g1, g2);
    }

    #[test]
    fn fingerprint_is_stable_across_clones() {
        let s = DefaultGenome::schema();
        let mut r = ChaCha8Rng::seed_from_u64(7);
        let g = DefaultGenome::random(&mut r, &s);
        let h1 = g.fingerprint();
        let h2 = g.clone().fingerprint();
        assert_eq!(h1, h2);
    }

    #[test]
    fn fingerprint_differs_when_field_changes() {
        let s = DefaultGenome::schema();
        let mut r = ChaCha8Rng::seed_from_u64(7);
        let mut g = DefaultGenome::random(&mut r, &s);
        let h_before = g.fingerprint();
        g.temperature += 0.001;
        let h_after = g.fingerprint();
        assert_ne!(h_before, h_after);
    }

    #[test]
    fn provider_config_reflects_genome_fields() {
        let s = DefaultGenome::schema();
        let mut r = ChaCha8Rng::seed_from_u64(3);
        let g = DefaultGenome::random(&mut r, &s);
        let pc = g.provider_config();
        assert_eq!(pc.model, g.model);
        assert!((pc.temperature - g.temperature).abs() < 1e-9);
        assert!((pc.top_p - g.top_p).abs() < 1e-9);
        assert_eq!(pc.max_tokens, g.max_tokens);
    }
}
```

**Step 2: Run tests — should fail to compile**

Run: `cargo test -p evolve-core default_genome::`
Expected: compile error: no `Genome` impl for `DefaultGenome`.

**Step 3: Implement `Genome` for `DefaultGenome`**

Add to `default_genome.rs`:

```rust
impl Genome for DefaultGenome {
    fn schema() -> GenomeSchema {
        use std::collections::BTreeMap;
        let mut slots = BTreeMap::new();
        slots.insert("role".to_string(), vec![
            "an expert mathematician".to_string(),
            "a careful step-by-step reasoner".to_string(),
            "a concise problem-solver".to_string(),
        ]);
        slots.insert("strategy".to_string(), vec![
            "decompose the problem into smaller parts".to_string(),
            "verify each step before continuing".to_string(),
            "consider multiple approaches".to_string(),
        ]);
        let fields = BTreeMap::from([
            ("system_prompt".to_string(), FieldSchema::String {
                mutator: StringMutatorKind::TemplateSlot { slots },
            }),
            ("model".to_string(), FieldSchema::Categorical {
                choices: vec!["claude-haiku-4-5".to_string(), "gpt-4o-mini".to_string(), "qwen2.5:7b".to_string()],
            }),
            ("temperature".to_string(), FieldSchema::Float { range: (0.0, 2.0), sigma: 0.2 }),
            ("top_p".to_string(), FieldSchema::Float { range: (0.1, 1.0), sigma: 0.1 }),
            ("max_tokens".to_string(), FieldSchema::Integer { range: (128, 4096), sigma: 256.0 }),
            ("reasoning_style".to_string(), FieldSchema::Categorical {
                choices: vec!["direct".to_string(), "cot".to_string(), "scratchpad".to_string(), "decomposition".to_string()],
            }),
            ("tools".to_string(), FieldSchema::Set {
                pool: vec!["calculator".to_string()],
                max: Some(1),
            }),
            ("few_shot_ids".to_string(), FieldSchema::Set {
                pool: (0..10).map(|i| format!("ex_{i}")).collect(),
                max: Some(3),
            }),
        ]);
        let mutation_rates = BTreeMap::from([
            ("system_prompt".to_string(), 0.20),
            ("model".to_string(), 0.05),
            ("temperature".to_string(), 0.15),
            ("top_p".to_string(), 0.10),
            ("max_tokens".to_string(), 0.05),
            ("reasoning_style".to_string(), 0.10),
            ("tools".to_string(), 0.10),
            ("few_shot_ids".to_string(), 0.10),
        ]);
        GenomeSchema { fields, mutation_rates }
    }

    fn random<R: Rng + ?Sized>(rng: &mut R, _schema: &GenomeSchema) -> Self {
        use rand::seq::SliceRandom;
        let roles = ["an expert mathematician", "a careful step-by-step reasoner", "a concise problem-solver"];
        let strategies = ["decompose the problem", "verify each step", "consider multiple approaches"];
        let role = roles.choose(rng).unwrap();
        let strategy = strategies.choose(rng).unwrap();
        let models = ["claude-haiku-4-5", "gpt-4o-mini", "qwen2.5:7b"];
        let styles = ["direct", "cot", "scratchpad", "decomposition"];
        DefaultGenome {
            system_prompt: format!("You are {role}. Approach: {strategy}."),
            model: models.choose(rng).unwrap().to_string(),
            temperature: rng.gen_range(0.0..2.0),
            top_p: rng.gen_range(0.1..1.0),
            max_tokens: rng.gen_range(128..4096),
            reasoning_style: styles.choose(rng).unwrap().to_string(),
            tools: BTreeSet::new(),
            few_shot_ids: BTreeSet::new(),
        }
    }

    fn render_prompt(&self) -> String {
        self.system_prompt.clone()
    }

    fn provider_config(&self) -> ProviderConfig {
        let provider = if self.model.starts_with("claude") { "anthropic" }
                       else if self.model.starts_with("gpt") { "openai" }
                       else { "ollama" };
        ProviderConfig {
            provider: provider.to_string(),
            model: self.model.clone(),
            temperature: self.temperature,
            top_p: self.top_p,
            max_tokens: self.max_tokens,
        }
    }

    fn fingerprint(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        let json = serde_json::to_string(self).expect("genome serializes");
        json.hash(&mut h);
        h.finish()
    }
}
```

In `lib.rs`: add `pub mod default_genome;`

**Step 4: Run tests**

Run: `cargo test -p evolve-core default_genome::`
Expected: 6 passed

**Step 5: Commit**

```bash
git add crates/evolve-core/src/{lib.rs,default_genome.rs}
git commit -m "feat(genome): implement DefaultGenome with 8 axes"
```

## Task 1.5 — Property tests for genome invariants

**Files:**
- Modify: `crates/evolve-core/src/default_genome.rs` (add proptest module)

**Step 1: Add proptest module**

```rust
#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;

    proptest! {
        #[test]
        fn random_is_in_schema_bounds(seed in any::<u64>()) {
            let s = DefaultGenome::schema();
            let mut r = ChaCha8Rng::seed_from_u64(seed);
            let g = DefaultGenome::random(&mut r, &s);
            prop_assert!((0.0..=2.0).contains(&g.temperature));
            prop_assert!((0.1..=1.0).contains(&g.top_p));
            prop_assert!((128..=4096).contains(&g.max_tokens));
            prop_assert!(["claude-haiku-4-5", "gpt-4o-mini", "qwen2.5:7b"].contains(&g.model.as_str()));
            prop_assert!(["direct", "cot", "scratchpad", "decomposition"].contains(&g.reasoning_style.as_str()));
        }

        #[test]
        fn fingerprint_collides_only_when_genomes_equal(seed_a in any::<u64>(), seed_b in any::<u64>()) {
            let s = DefaultGenome::schema();
            let mut ra = ChaCha8Rng::seed_from_u64(seed_a);
            let mut rb = ChaCha8Rng::seed_from_u64(seed_b);
            let ga = DefaultGenome::random(&mut ra, &s);
            let gb = DefaultGenome::random(&mut rb, &s);
            if ga == gb {
                prop_assert_eq!(ga.fingerprint(), gb.fingerprint());
            }
        }
    }
}
```

**Step 2: Run all tests**

Run: `cargo test -p evolve-core`
Expected: all pass (proptest generates 256 cases per test by default)

**Step 3: Commit**

```bash
git add crates/evolve-core/src/default_genome.rs
git commit -m "test(genome): add proptest invariants for DefaultGenome"
```

## Task 1.6 — `SchemaGenome` (generic, schema-driven)

**Files:**
- Create: `crates/evolve-core/src/schema_genome.rs`
- Modify: `crates/evolve-core/src/lib.rs`

**Step 1: Write failing tests**

```rust
//! SchemaGenome: a generic Genome whose fields are determined entirely by a GenomeSchema.
//! Used for the declarative DSL on-ramp and for HTTP/gRPC clients.

use crate::genome::{Genome, ProviderConfig};
use crate::schema::{FieldSchema, GenomeSchema, StringMutatorKind};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FieldValue {
    String(String),
    Float(f64),
    Integer(i64),
    Categorical(String),
    Set(Vec<String>),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SchemaGenome {
    pub schema_id: String,        // identifies which schema this conforms to
    pub values: BTreeMap<String, FieldValue>,
}

#[cfg(test)]
mod tests {
    // tests TBD in expansion: roundtrip, random-from-schema, fingerprint stability,
    // provider-config extraction (from a "model" categorical field if present)
}
```

**Step 2: Implement `Genome` for `SchemaGenome`**

(Implementation: `random` reads `schema.fields` and picks a value for each field type.
 `provider_config` looks for conventional field names: `model`, `temperature`, `top_p`, `max_tokens`.
 `fingerprint` hashes JSON of `(schema_id, values)`.
 `render_prompt` joins all `String`/`Categorical` field values with newlines, OR returns the value of a field named `system_prompt` if present.)

**Step 3: Add and run tests covering:**
- random produces value for every schema field
- fingerprint stable across clone, differs when value changes
- provider_config extracts conventional fields
- proptest: random_is_in_schema_bounds for any valid schema

**Step 4: Commit**

```bash
git add crates/evolve-core/src/{lib.rs,schema_genome.rs}
git commit -m "feat(genome): SchemaGenome — generic schema-driven Genome impl"
```

## Task 1.7 — Phase 1 verification

Run:
- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `cargo llvm-cov --package evolve-core --summary-only` → expect ≥80% line coverage

If all pass: **PHASE 1 COMPLETE**.

---

# PHASE 2 — Operators (Mutator + Crosser per axis type)

**Goal:** Per-axis-type `Mutator<T>` and `Crosser<T>` traits with built-in implementations for every `FieldSchema` variant. Property tests prove all operators preserve schema bounds.

## Task 2.1 — Define `Mutator<T>` and `Crosser<T>` traits

**Files:** Create `crates/evolve-core/src/operators/mod.rs`; add `pub mod operators;` to `lib.rs`.

```rust
use rand::Rng;

pub trait Mutator<T>: Send + Sync {
    fn mutate<R: Rng + ?Sized>(&self, value: &mut T, rng: &mut R);
}

pub trait Crosser<T>: Send + Sync {
    fn cross<R: Rng + ?Sized>(&self, a: &T, b: &T, rng: &mut R) -> T;
}
```

Test: trait objects compile (`Box<dyn Mutator<f64>>`).

Commit: `feat(operators): define Mutator and Crosser traits`

## Task 2.2 — `GaussianMutator` for floats with bounds

**Files:** `crates/evolve-core/src/operators/float.rs`

Tests (TDD):
- mutated value stays within `(lo, hi)` after any mutation (proptest)
- mutated value differs from input with probability ~1 (statistical, seed-stable)
- σ=0 produces identity mutation
- mutating at boundary (lo or hi) clamps correctly

Implementation: `GaussianMutator { lo, hi, sigma }`. `mutate` adds `rng.sample(Normal::new(0, sigma)) * sigma` and clamps to `[lo, hi]`.

Commit: `feat(operators): GaussianMutator for bounded floats with proptest`

## Task 2.3 — `BlendCrosser` for floats

Tests: `cross(a, b)` returns value in `[min(a,b), max(a,b)]` (proptest); `cross(a, a) == a`.

Implementation: weighted average with random weight in `[0, 1]`.

Commit: `feat(operators): BlendCrosser for floats`

## Task 2.4 — `GaussianRoundedMutator` + `BlendRoundedCrosser` for integers

Same as 2.2/2.3 but rounds to nearest integer and clamps to integer range. Tests cover the same invariants.

Commit: `feat(operators): integer mutator and crosser`

## Task 2.5 — `UniformCategoricalMutator` + `RandomChoiceCrosser`

Tests: mutated value is in `choices`; crossed value is one of `a` or `b`.

Implementation:
- Mutator picks any value from `choices` uniformly
- Crosser picks `a` or `b` 50/50

Commit: `feat(operators): categorical mutator and crosser`

## Task 2.6 — `BitflipMutator` + `UniformElementCrosser` for sets

Tests:
- Mutator: each element of pool is independently flipped with rate p (default 1/|pool|); resulting set respects max size
- Crosser: each element of (a ∪ b) is included with 50% probability from each parent; respects max size

Commit: `feat(operators): set-valued mutator and crosser`

## Task 2.7 — `TemplateSlotMutator` for string-with-slots

Tests:
- Mutator picks one slot at random and swaps its current value for another from the slot's pool
- Mutated string still parses as the template (slot markers preserved)

Implementation: parse string against `slots` map, pick a slot, replace its value, re-render.

Commit: `feat(operators): TemplateSlotMutator for slot-based prompt mutation`

## Task 2.8 — `LlmRewriteMutator` (skeleton, depends on Provider trait from Phase 4)

**Defer concrete impl until Phase 4 lands; in Phase 2, define the trait shape:**

```rust
#[async_trait::async_trait]
pub trait LlmMutator: Send + Sync {
    async fn rewrite(&self, prompt: &str, hint: &str) -> Result<String, anyhow::Error>;
}
```

Test: a `MockLlmMutator` that returns a deterministic rewrite passes a contract test.

Commit: `feat(operators): LlmMutator trait + MockLlmMutator for testing`

## Task 2.9 — `BlockCrosser` for prompts (paragraph-boundary crossover)

Tests:
- Splits both prompts on `\n\n`, picks crossover point, concatenates first half of A with second half of B
- Result is non-empty if both inputs non-empty

Commit: `feat(operators): BlockCrosser for paragraph-boundary prompt crossover`

## Task 2.10 — Operator dispatcher (axis-typed → trait-object lookup)

**Files:** `crates/evolve-core/src/operators/dispatch.rs`

For a given `FieldSchema`, return the appropriate `Mutator` and `Crosser` trait objects. Used by the population engine.

Tests: every `FieldSchema` variant returns non-null operators.

Commit: `feat(operators): dispatcher mapping FieldSchema to operator trait objects`

## Task 2.11 — Phase 2 verification

- `cargo test --workspace` → all pass (~50+ new tests including proptests)
- Coverage on `operators/` ≥85%
- `cargo bench --bench operators` (criterion bench, scaffold for now) shows sub-ms per individual on every operator

**PHASE 2 COMPLETE.**

---

# PHASE 3 — Cost Meter + Fitness Cache

**Goal:** `CostMeter` (atomic counters, trip flag) and `FitnessCache` trait + `DashMapCache` impl. Both used pervasively in later phases.

**Tasks:**

- 3.1 — `CostMeter` struct: `AtomicU64` tokens_in, tokens_out, micro_dollars; `AtomicBool` tripped. Methods: `record(tokens_in, tokens_out, micro_dollars)`, `check_or_trip(ceiling_micro_dollars) -> bool`, `is_tripped() -> bool`. **Tests with `loom`** for concurrency.
- 3.2 — `PricingTable` per provider: input/output per-token rates. Built-in tables for OpenAI, Anthropic, Ollama (zero-cost).
- 3.3 — `FitnessCache` trait + key shape (`(genome_fingerprint, input_id, model_id, sample_index)`).
- 3.4 — `DashMapCache` impl. Tests: `get` after `put` returns same value; concurrent `put`s are safe; size cap eviction (LRU via `lru` crate).
- 3.5 — Phase 3 verification: coverage, no clippy warnings, commit.

**PHASE 3 COMPLETE.**

---

# PHASE 4 — Provider Trait + Ollama Backend

**Goal:** `LlmProvider` trait + `CostMetered<P>` wrapper + working Ollama backend via `async-openai`.

**New crate:** `crates/evolve-providers/`

**Tasks:**

- 4.1 — Crate scaffold; add `evolve-providers` to workspace.
- 4.2 — `LlmProvider` trait: `async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse, ProviderError>`.
- 4.3 — `ProviderError` enum (Transient, Semantic, BudgetExhausted, Misconfigured).
- 4.4 — `CompletionRequest` and `CompletionResponse` types (Serde-derivable for future remote impl).
- 4.5 — `CostMetered<P>` wrapper: every `complete()` checks the meter flag, then records tokens after.
- 4.6 — `OllamaProvider` via `async-openai` (set `base_url` to `http://localhost:11434/v1`).
- 4.7 — Cassette tests using `wiremock`: record a real Ollama interaction once, replay in CI without network.
- 4.8 — `--ignored` smoke test against real Ollama (gated by `OLLAMA_URL` env var).
- 4.9 — Error mapping: 5xx/timeout → Transient, 4xx → Semantic, 401 → Misconfigured, 429 → Transient with retry-after header parsed.
- 4.10 — Phase 4 verification: coverage, manual smoke test against local Ollama instance.

**PHASE 4 COMPLETE.**

---

# PHASE 5 — OpenAI + Anthropic Providers

**Tasks:**

- 5.1 — `OpenAiProvider` via `async-openai` (default `base_url`, requires `OPENAI_API_KEY`).
- 5.2 — `AnthropicProvider` via custom `reqwest` client (Anthropic uses different request shape than OpenAI; `messages` API with `system` separate).
- 5.3 — Tool-calling shape parity: both providers expose tools via `CompletionRequest::tools` and parse tool calls into `CompletionResponse::tool_calls`.
- 5.4 — Cassette tests for both. Provider error mapping coverage.
- 5.5 — `--ignored` smoke tests (gated by `OPENAI_API_KEY` / `ANTHROPIC_API_KEY`).
- 5.6 — Pricing tables exact (cross-check against current public pricing; flag for periodic update).
- 5.7 — Phase 5 verification.

**PHASE 5 COMPLETE.**

---

# PHASE 6 — Benchmark Trait + Built-in Benchmarks

**Tasks:**

- 6.1 — `Benchmark` trait in `evolve-core`.
- 6.2 — `SyntheticClassification` impl: generates N short strings, each labeled A/B/C by a hidden rule (e.g., starts with vowel). Score = correct/total. Used for $0 loop validation.
- 6.3 — `Gsm8kSubset` impl: downloads first 200 problems from public GSM8K dataset (cached to `~/.evolve/datasets/`), exact-match scoring on the numeric answer (regex `#### (\d+)` extraction).
- 6.4 — Tests: both benchmarks score known good/bad outputs correctly.
- 6.5 — Phase 6 verification.

**PHASE 6 COMPLETE.**

---

# PHASE 7 — Evaluator Trait + InProcess + Failure Classifier

**Tasks:**

- 7.1 — `Evaluator` trait + `EvalRequest` + `EvalResult` types (all Serde).
- 7.2 — `InProcessEvaluator` using `tokio::task::JoinSet`; streams results as they complete.
- 7.3 — `FailureClassifier` middleware: wraps Evaluator output, retries on Transient (1 retry, exp backoff 200→800ms), classifies leftover errors.
- 7.4 — Tests: with `MockProvider` that fails on schedule, classifier produces correct outcomes.
- 7.5 — Tests: budget exhaustion mid-stream halts cleanly.
- 7.6 — Phase 7 verification.

**PHASE 7 COMPLETE.**

---

# PHASE 8 — Diversity Engine (candle + crowding)

**Tasks:**

- 8.1 — Add `candle-core`, `candle-nn`, `candle-transformers`, `tokenizers` deps to `evolve-core` (or split into `evolve-diversity` crate to keep core dep-light — decide before starting).
- 8.2 — Download MiniLM-L6-v2 weights at build time OR lazy-load on first use; cache to `~/.evolve/models/`.
- 8.3 — `Embedder` struct wrapping the model; batched embedding API.
- 8.4 — `DistanceMatrix` builder; pairwise cosine sim → distance matrix.
- 8.5 — `DeterministicCrowding` selector: each offspring competes against nearest parent by distance.
- 8.6 — Tests: batched embedding throughput <500μs/genome at batch=50; crowding correctness on hand-crafted distance matrices.
- 8.7 — Bench: `bench_embedding_throughput` in `benches/`.
- 8.8 — Phase 8 verification.

**PHASE 8 COMPLETE.**

---

# PHASE 9 — Population Engine (the Inner Loop)

**Tasks:**

- 9.1 — `Population<G: Genome>` struct: pre-allocated `Vec<G>`, `Vec<f64>` fitness, `Vec<u64>` fingerprints.
- 9.2 — `TournamentSelector { size: usize }`: picks parent pairs.
- 9.3 — `Engine<G, P, B, E>` (Genome, Provider, Benchmark, Evaluator) — the orchestrator.
- 9.4 — `Engine::run_generation()`: implements Section 5.2 of the design doc step by step.
- 9.5 — K=2 contested re-evaluation: identify top 30% (top 15 + ties within 0.02), re-eval at sample_index=1.
- 9.6 — Elitism: top 5 carried forward unchanged with cached fitness.
- 9.7 — Random injection: 5 fresh randoms replace bottom 5.
- 9.8 — **THE reproducibility test**: `Engine` with seed=42 and `MockProvider` produces bit-identical fitness curves across re-runs.
- 9.9 — Budget gating test: ceiling=$0.10, run halts mid-generation cleanly.
- 9.10 — Resilience test: all-failures generation N doesn't kill the run; gen N+1 succeeds.
- 9.11 — Bench: `bench_generation_step_no_llm` <50ms wall-clock for pop=50 with MockProvider.
- 9.12 — Phase 9 verification — this is the load-bearing milestone for the entire project.

**PHASE 9 COMPLETE.**

---

# PHASE 10 — Storage Layer (sqlx)

**New crate:** `crates/evolve-storage/`

**Tasks:**

- 10.1 — Crate scaffold + `sqlx` deps with both `sqlite` and `postgres` features behind cargo features.
- 10.2 — Schema migration files (`migrations/`): tables `runs`, `generations`, `genomes`, `evaluations`, `cost_events`. Use `sqlx migrate add`.
- 10.3 — `Storage` trait (async): `start_run`, `persist_generation`, `load_run`, `list_runs`, `best_genome`.
- 10.4 — `SqliteStorage` impl. Tests against `sqlite::memory:`.
- 10.5 — `PostgresStorage` impl. Tests against testcontainers Postgres.
- 10.6 — Wire storage into `Engine` (per-generation transactional write).
- 10.7 — Test: state survives process restart (write run, drop process, reopen, load run, verify identical).
- 10.8 — Phase 10 verification.

**PHASE 10 COMPLETE.**

---

# PHASE 11 — CLI

**New crate:** `crates/evolve-cli/`

**Tasks:**

- 11.1 — `clap` setup with subcommands `run`, `serve`, `export`, `worker` (worker is stub returning "not yet implemented in v1").
- 11.2 — `evolve run <config.yaml>`: load YAML config (provider, benchmark, schema, hyperparameters, ceiling), construct Engine, run.
- 11.3 — Config schema with `serde_yaml`. Validation errors with line/column.
- 11.4 — `evolve export <run-id>`: load best genome from storage, write JSON + snippets + runtime-spec + adapter stubs to `runs/<id>/export/`.
- 11.5 — Auto-generated `usage.py`, `usage.ts`, `usage.rs` snippets templated against the runtime endpoint.
- 11.6 — Integration test: end-to-end `evolve run examples/synthetic-classification/config.yaml` produces a non-empty fitness curve and an exported best genome.
- 11.7 — Phase 11 verification.

**PHASE 11 COMPLETE.**

---

# PHASE 12 — HTTP/gRPC Service

**New crate:** `crates/evolve-service/`

**Tasks:**

- 12.1 — Crate scaffold, deps: `axum`, `tonic`, `tower`, `tonic-build`.
- 12.2 — Define gRPC schema in `proto/evolve.proto` (`StartRun`, `StreamGenerations`, `GetBest`).
- 12.3 — Shared service implementation as a tower service.
- 12.4 — Axum HTTP routes: `POST /runs`, `GET /runs/{id}/generations` (SSE), `GET /runs/{id}/best`.
- 12.5 — Tonic gRPC service mounted on the same tower.
- 12.6 — Tests: HTTP and gRPC endpoints return identical data for identical inputs.
- 12.7 — SSE streaming via `axum::response::sse`.
- 12.8 — `evolve serve` CLI subcommand spins up the service.
- 12.9 — Phase 12 verification.

**PHASE 12 COMPLETE.**

---

# PHASE 13 — OpenAI-Compatible Runtime

**New crate:** `crates/evolve-runtime/`

**Tasks:**

- 13.1 — Crate scaffold + axum.
- 13.2 — `POST /v1/chat/completions` matching OpenAI's request/response shape.
- 13.3 — Load a genome from JSON; render its prompt; call its provider; return response in OpenAI shape.
- 13.4 — Tool-calling support: pass through OpenAI's `tools` param, invoke matching native tools (calculator), include results in completion.
- 13.5 — `evolve runtime serve --spec runtime-spec.json` CLI subcommand.
- 13.6 — Test: `curl -X POST localhost:7777/v1/chat/completions -d '{...}'` returns OpenAI-shaped response.
- 13.7 — Test: official `openai` Python client successfully calls the runtime (integration test using PyO3 bindings or subprocess).
- 13.8 — Phase 13 verification.

**PHASE 13 COMPLETE.**

---

# PHASE 14 — Dashboard

**New crate:** `crates/evolve-dashboard/`

**Decision needed at start of phase:** leptos (Rust-only, full-stack, harder to find frontend contributors) vs. Vite+React (industry-standard, larger contributor pool, separate build step). Defer to a brainstorming session at start of this phase.

**Tasks (Vite+React assumed below; revise if leptos chosen):**

- 14.1 — Decision: framework. Document in `docs/decisions/2026-XX-dashboard-framework.md`.
- 14.2 — Vite+React+TS scaffold under `crates/evolve-dashboard/web/`. shadcn/ui or similar component library.
- 14.3 — Dashboard pages: Runs list, Run detail (live fitness curve, cost burn-down, best genome inspector), Settings.
- 14.4 — Live update via `EventSource` on `GET /runs/{id}/generations`.
- 14.5 — Fitness curve via `recharts` or similar.
- 14.6 — Diversity heatmap via D3 or canvas.
- 14.7 — Build step: `npm run build` produces `web/dist/`.
- 14.8 — `rust-embed` macro embeds `web/dist/` into the binary.
- 14.9 — Axum routes serve the SPA from embedded files.
- 14.10 — Test: `cargo run -p evolve-cli -- serve --dashboard` starts; `curl localhost:8080/` returns the SPA HTML.
- 14.11 — Phase 14 verification (manual UI test, no automated UI tests in v1).

**PHASE 14 COMPLETE.**

---

# PHASE 15 — Python Bindings (PyO3)

**Directory:** `bindings/python/`

**Tasks:**

- 15.1 — `pyproject.toml` with maturin backend; `Cargo.toml` for the PyO3 extension.
- 15.2 — Wrap `Engine`, `DefaultGenome`, `SchemaGenome`, `Benchmark` trait, `Provider` trait via PyO3.
- 15.3 — Async via `pyo3-async-runtimes` (tokio bridge).
- 15.4 — Python-side `Benchmark` ABC: users subclass in Python, framework calls back into Python.
- 15.5 — Python-side `Genome` advanced trait: same pattern.
- 15.6 — Wheel build via `maturin build --release` for Linux/macOS/Windows × CPython 3.10/3.11/3.12.
- 15.7 — Pytest test suite under `bindings/python/tests/`.
- 15.8 — **Bindings parity test #1**: 5-generation `MockProvider` run from Python produces same fitness curve as same run from Rust.
- 15.9 — GitHub Actions workflow: build wheels on every push, publish to PyPI on tag.
- 15.10 — Phase 15 verification.

**PHASE 15 COMPLETE.**

---

# PHASE 16 — TypeScript Bindings (napi-rs)

**Directory:** `bindings/typescript/`

**Tasks:**

- 16.1 — napi-rs scaffold; `package.json` with `@napi-rs/cli`.
- 16.2 — Wrap same surface as Python: Engine, DefaultGenome, SchemaGenome, Benchmark interface, Provider interface.
- 16.3 — Async via napi-rs's tokio bridge (returns native JS Promises).
- 16.4 — TypeScript .d.ts declarations generated by napi-rs.
- 16.5 — Prebuilt binaries for Linux/macOS/Windows × x64/arm64 via napi-rs's CI templates.
- 16.6 — `node --test` test suite.
- 16.7 — **Bindings parity test #2**: same canonical scenario from TS produces same fitness curve.
- 16.8 — GitHub Actions: build prebuilt binaries, publish to npm on tag.
- 16.9 — Phase 16 verification.

**PHASE 16 COMPLETE.**

---

# PHASE 17 — Framework Adapters

**Directory:** `adapters/`

**Tasks:**

- 17.1 — `adapters/langchain/`: Python package, `EvolveRunnable(Runnable)` subclass that calls the OpenAI-compat runtime.
- 17.2 — `adapters/dspy/`: Python package, `EvolveModule(dspy.Module)` subclass.
- 17.3 — `adapters/vercel-ai/`: TS package, `evolveProvider()` factory returning a `LanguageModelV1` impl.
- 17.4 — Per-adapter test: end-to-end "spin up runtime → call adapter → get response" for each.
- 17.5 — Per-adapter README with install + usage examples.
- 17.6 — Each adapter publishes independently (own CI, own version).
- 17.7 — Phase 17 verification.

**PHASE 17 COMPLETE.**

---

# PHASE 18 — Examples

**Directory:** `examples/`

**Tasks:**

- 18.1 — `examples/synthetic-classification/`: `config.yaml`, `README.md`, `run.sh`. CI runs this on every PR (Ollama via testcontainers, ~2 min, $0). Acceptance: best fitness > random baseline by 2σ across 5 seeds.
- 18.2 — `examples/gsm8k-headline/`: `config.yaml`, `README.md`, `run.sh`. Nightly CI on main (Anthropic Haiku, ~5 min, ~$5). Acceptance: best accuracy > baseline-prompt accuracy by 5pp.
- 18.3 — Result archives: `examples/*/results/<date>/` committed for historical tracking.
- 18.4 — Phase 18 verification.

**PHASE 18 COMPLETE.**

---

# PHASE 19 — Documentation + Release

**Tasks:**

- 19.1 — mdBook scaffold under `docs/book/`.
- 19.2 — Chapters: Introduction, Quickstart (Python, TS, Rust, HTTP), Architecture, Genome DSL Reference, Custom Benchmarks, Custom Providers, Adapters, FAQ.
- 19.3 — GitHub Pages workflow to build and deploy the book on every main push.
- 19.4 — Per-crate README with "what this crate does" + "see workspace docs" link.
- 19.5 — `cargo doc --workspace --no-deps` published to docs.rs.
- 19.6 — `cargo-dist` setup: `cargo dist init` + workflow for prebuilt binaries on release tags.
- 19.7 — Multi-arch Docker image (`ghcr.io/<user>/evolve:<version>`) via GitHub Actions.
- 19.8 — `CHANGELOG.md` following Keep a Changelog format.
- 19.9 — v1.0.0 release process: tag, push, verify all artifacts published (crates.io, PyPI, npm, ghcr.io, GitHub Releases).
- 19.10 — Phase 19 verification.

**PHASE 19 COMPLETE.**

---

# PHASE 20 — Public Roadmap + Community

**Tasks:**

- 20.1 — `ROADMAP.md` listing v1.1 (distributed workers, 8-12 weeks post v1 GA), v1.2 (multi-task, more benchmarks), v2.0 (hosted).
- 20.2 — GitHub issue templates: bug, feature request, performance regression, documentation, contributed adapter.
- 20.3 — `CODE_OF_CONDUCT.md`.
- 20.4 — `SECURITY.md` with disclosure policy.
- 20.5 — `.github/PULL_REQUEST_TEMPLATE.md` with TDD checklist.
- 20.6 — Public roadmap board on GitHub Projects.
- 20.7 — First Show HN / Reddit / Twitter post drafted (NOT posted; user decides timing).
- 20.8 — Phase 20 verification.

**PHASE 20 COMPLETE — v1.0 GA READY.**

---

## Cross-Cutting Concerns

### Reproducibility
Every operator, selector, and engine method takes `&mut R: Rng + ?Sized`. The CLI seeds with `--seed <u64>` (default: random). Bindings parity tests use the same seed across surfaces.

### Determinism caveats
LLM responses are non-deterministic at the provider level. The fitness cache makes re-runs of the same run with the same seed deterministic AFTER the first complete run (cached responses replay). Document this clearly in the docs.

### Observability
`tracing` everywhere. `EVOLVE_OTEL_ENDPOINT` enables OTEL export. `EVOLVE_LOG_LEVEL` sets the level. JSON logs by default for service mode, human-readable for CLI mode.

### Performance budgets
Per-individual non-LLM hot path: <1ms. Per-generation overhead (excluding LLM time): <50ms at pop=50. Embedding throughput: <500μs/genome at batch=50. Cache lookup: <100ns.

### TDD enforcement
CI gate: PRs adding a public function in `evolve-core` without a corresponding test in the same diff fail CI. Implementation: a `scripts/check-tdd.sh` script run by CI.

### Memory layout for hot paths
Pre-allocate `Vec<G>`, `Vec<f64>`, `Vec<u64>` buffers reused across generations. No per-individual allocation in the inner loop. `SmallVec` for short slot arrays. RNG passed by `&mut R` everywhere.

---

## Acceptance Criteria for v1.0 GA

(Copied from design doc Section 11 for direct verification.)

1. `cargo test --workspace` passes on Linux/macOS/Windows × stable Rust
2. `pip install evolveai` then a 5-line Python script runs a 5-generation `MockProvider` evolution and prints fitness curve
3. `npm install @evolveai/sdk` then a 5-line TS script does the same
4. `evolve serve --dashboard` exposes the dashboard at localhost; running a real GSM8K evolution updates the UI live
5. `examples/synthetic-classification/` produces best-genome fitness > random-baseline fitness by ≥2σ across 5 seeds
6. `examples/gsm8k-headline/` produces best-genome accuracy > baseline-prompt accuracy by ≥5 percentage points, total cost <$10
7. Bindings parity test: identical seed produces identical fitness curve from Rust, Python, TS, and HTTP
8. `evolve export <run-id>` produces a runnable runtime + working snippets + working LangChain/DSPy/Vercel adapters
9. Coverage thresholds met: `evolve-core` ≥80%, `evolve-providers` ≥75%, `evolve-storage` ≥70%
10. Cost meter abort test: a run with $0.10 ceiling halts cleanly with `budget_exhausted` and persisted partial state

---

*End of implementation plan. Phases 0-2 are bite-sized. Phases 3-20 are structured task lists with file paths, test signatures, and key notes — re-invoke `superpowers:writing-plans` against any phase to expand it to bite-sized form before execution, or hand the task list directly to `superpowers:subagent-driven-development`.*

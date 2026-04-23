# Evolve — Design Document

**Date:** 2026-04-23
**Author:** Kristian Baer (Northtek / FrostByte Digital)
**Status:** Approved, ready for implementation planning
**Companion document:** `2026-04-23-evolve-validation-implementation.md` (to be produced by writing-plans skill)

---

## 1. Context & Goal

**Evolve** is a research bet on applying evolutionary computation to LLM agents.
Each "agent" is a *genome* (prompt template + tool set + model config + hyperparameters).
A population of N agents is evaluated against a task benchmark; high-fitness agents
reproduce via crossover and mutation, low-fitness agents are replaced. Over generations,
the population self-specializes for the task — without any fine-tuning, prompt engineering,
or RLHF.

### Why novel

- **GENOME** (sister project, Python) applies genetic operators to *memory* (embeddings).
  Evolve applies them to *agents* (prompts + tools + configs). Different substrate,
  same theoretical foundation.
- **Voyager** (Nvidia) did skill-library evolution for Minecraft. Nobody has generalized
  evolutionary optimization to arbitrary LLM agent workloads as a reusable framework.
- **PromptBreeder / EvoPrompt** evolve prompts only. Evolve evolves the whole agent
  configuration (prompt + tools + routing + hyperparameters) as a unit.

### Scope evolution during brainstorming

The original brief framed Evolve as a narrow v0 validation experiment producing a
single benchmark result. During brainstorming, the user pivoted explicitly:
**"build what should actually be built in the final version. It needs to be compatible
and the obvious switch or helpful product for people running agents that people will
want to adopt."**

The shipped scope is therefore the *final product*, not a validation experiment.
The original v0 hypothesis ("a population of LLM agents with genetic operators can
outperform a fixed baseline on GSM8K") is preserved as the *headline example run*
(`examples/gsm8k-headline/`) — proof-of-correctness for the framework, not the framework's
purpose.

---

## 2. Locked Decisions Summary

| # | Decision Area | Choice | Notes |
|---|---|---|---|
| 1 | Latency budget | Sub-ms harness for entire non-LLM hot path | Forces candle for embeddings, allocation-conscious operators |
| 2 | Headline benchmark | Synthetic classification → GSM8K (in-order) | One example, not the product. Synthetic = $0 loop validation; GSM8K = headline result |
| 3 | LLM providers | All three (Ollama + OpenAI + Anthropic), extensible trait | `async-openai` covers OpenAI + Ollama (different `base_url`); custom reqwest Anthropic client |
| 4 | Genome shape | 8 axes (prompt, model, temp, top-p, max_tokens, reasoning_style, tools, few-shot) + per-axis mutation rates + calculator tool implemented for v0 | Default genome shipped; user-extensible via DSL or trait |
| 5 | Population sizing (default example) | pop=50, gens=30, $80 ceiling | All configurable per run |
| 6 | Diversity preservation | Elitism (top 5) + random injection (bottom 5) + deterministic crowding | Belt-and-suspenders against population collapse |
| 7 | Fitness variance | Response cache + k=2 contested re-evaluation on top 30% | Cache makes runs deterministically reproducible |
| 8 | Prompt mutation | Hybrid: template slots + LLM-as-mutator on slot contents | Modern (PromptBreeder-style) + interpretable |
| 9 | Integration surface | Rust core + Python (PyO3) + TypeScript (napi-rs) + HTTP/gRPC service mode | All four built day one |
| 10 | Genome extensibility | 3 on-ramps: turnkey default + declarative DSL + advanced trait | DSL is JSON-Schema-superset; trait works in Rust/Python/TS, not over the wire |
| 11 | Best-genome export | JSON + auto-generated snippets + OpenAI-compatible runtime + 3 framework adapters (LangChain, DSPy, Vercel AI) | OpenAI-compat runtime is the highest-leverage feature |
| 12 | Persistence + observability | SQLite default, Postgres opt-in, bundled web dashboard via rust-embed SPA | Dashboard is the single biggest adoption multiplier |
| 13 | Distributed evaluation | Single-node v1, `Evaluator` trait shaped for distribution, v1.1 worker mode on public roadmap (8-12 weeks post-v1 GA) | Polish over breadth; published roadmap makes promise credible |
| 14 | Failure handling | Retry once + classify (transient → mean fitness; semantic → 0; budget exhaustion → fail-fast) | Distinguishes provider flakiness from broken genomes |

---

## 3. Architecture & Repository Shape

### 3.1 Naming & licensing

- **Project name:** `evolve` (Cargo crate, GitHub repo)
- **Package name on PyPI/npm:** `evolveai` (PyPI `evolve` is taken; user can override before publish)
- **License:** Apache 2.0 (matches GENOME, fits open-core if a hosted version ever ships)
- **Project location:** `~/projects/active/evolve`

### 3.2 Workspace layout

```
evolve/
├── Cargo.toml              # workspace root
├── crates/
│   ├── evolve-core/        # engine: genome trait, operators, eval loop, cost meter, cache, diversity
│   ├── evolve-providers/   # LlmProvider trait + Ollama/OpenAI/Anthropic backends
│   ├── evolve-storage/     # SQLite + Postgres via sqlx, schema migrations
│   ├── evolve-runtime/     # OpenAI-compatible HTTP server for evolved genomes
│   ├── evolve-service/     # axum + tonic: HTTP + gRPC service mode
│   ├── evolve-dashboard/   # axum + rust-embed bundled SPA
│   └── evolve-cli/         # clap-based CLI: evolve run, serve, export, worker (stub)
├── bindings/
│   ├── python/             # PyO3 + maturin → pip install evolveai
│   └── typescript/         # napi-rs → npm install @evolveai/sdk
├── adapters/
│   ├── langchain/          # Python: LangChain Runnable adapter
│   ├── dspy/               # Python: dspy.Module adapter
│   └── vercel-ai/          # TS: LanguageModelV1 provider
├── examples/
│   ├── synthetic-classification/   # validation run, $0, runs in CI
│   └── gsm8k-headline/             # headline result, ~$5, runs nightly on main
├── benches/                # criterion benchmarks
├── docs/                   # mdBook site
└── docker/                 # Dockerfile for evolve serve, compose for Postgres dev
```

### 3.3 Build & release

- **Rust:** `cargo publish` per crate; GitHub Releases for binaries via `cargo-dist`
- **Python:** `maturin build --release` → wheels for Linux/macOS/Windows × CPython 3.10/3.11/3.12 → PyPI via GitHub Actions
- **TypeScript:** `napi build --release` → prebuilt binaries for the same matrix → npm
- **Docker:** multi-arch `ghcr.io/<user>/evolve:<version>` with the service + dashboard
- **Toolchain:** Rust 2024 edition, latest stable (MSRV pinned to current stable - 2)

---

## 4. Components

### 4.1 Core abstractions (`evolve-core`)

#### Genome trait

Three on-ramps:
- `DefaultGenome` (8-axis turnkey impl shipped in the crate)
- `SchemaGenome` (generic struct driven by a `GenomeSchema` value, DSL-defined, works over the wire)
- User-defined `impl Genome for MyGenome` for advanced users

```rust
pub trait Genome: Serialize + DeserializeOwned + Send + Sync + Clone + 'static {
    fn schema() -> GenomeSchema;
    fn random(rng: &mut impl Rng, schema: &GenomeSchema) -> Self;
    fn render_prompt(&self) -> String;
    fn provider_config(&self) -> ProviderConfig;
    fn fingerprint(&self) -> u64;  // for cache + crowding tie-break
}
```

#### Default genome axes

| Axis | Type | Mutator | Crosser |
|---|---|---|---|
| `system_prompt` | template-with-slots (string) | `TemplateSlotMutator` + `LlmRewriteMutator` | `BlockCrosser` (paragraph boundary) |
| `model` | categorical | `UniformCategoricalMutator` | `RandomChoiceCrosser` |
| `temperature` | float [0, 2] | `GaussianMutator` (σ=0.2, clamped) | `BlendCrosser` |
| `top_p` | float [0, 1] | `GaussianMutator` (σ=0.1, clamped) | `BlendCrosser` |
| `max_tokens` | integer [128, 4096] | `GaussianRoundedMutator` | `BlendRoundedCrosser` |
| `reasoning_style` | categorical {direct, cot, scratchpad, decomposition} | `UniformCategoricalMutator` | `RandomChoiceCrosser` |
| `tools` | set of `ToolId` (calculator implemented; user-extensible) | `BitflipMutator` | `UniformElementCrosser` |
| `few_shot_ids` | set of example IDs from a curated pool | `BitflipMutator` | `UniformElementCrosser` |

Per-axis mutation rates configurable; user can set rate=0 for axes irrelevant to their task.

#### LlmProvider trait + cost-metered wrapper

```rust
pub trait LlmProvider: Send + Sync {
    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse, ProviderError>;
    fn pricing(&self) -> &PricingTable;
}

pub struct CostMetered<P: LlmProvider> {
    inner: P,
    meter: Arc<CostMeter>,  // AtomicU64 tokens + AtomicU64 micro-dollars + AtomicBool tripped
}
```

Backends:
- `OllamaProvider` and `OpenAiProvider` via `async-openai` (shared client, different `base_url`)
- `AnthropicProvider` via thin reqwest client

Type system enforces no call path bypasses metering (all providers wrapped at construction).

#### Evaluator trait (shaped for v1.1 distribution)

```rust
pub trait Evaluator: Send + Sync {
    async fn evaluate(&self, requests: Vec<EvalRequest>)
        -> impl Stream<Item = EvalResult>;
}
```

`EvalRequest` is `Serialize` for future remote impl. v1 ships `InProcessEvaluator` (Tokio `JoinSet`).

#### Benchmark trait (user-defined task)

```rust
pub trait Benchmark: Send + Sync {
    type Input: Serialize + Send + Clone;
    type Output: DeserializeOwned + Send;
    fn samples(&self) -> &[Self::Input];
    fn score(&self, input: &Self::Input, output: &Self::Output) -> f64;  // 0.0..=1.0
    fn parse_output(&self, raw: &str) -> Result<Self::Output, ParseError>;
}
```

Built-in: `SyntheticClassification`, `Gsm8kSubset`. Users implement for their task.

#### Operators (per-axis-type)

```rust
trait Mutator<T> { fn mutate(&self, value: &mut T, rng: &mut impl Rng); }
trait Crosser<T>  { fn cross(&self, a: &T, b: &T, rng: &mut impl Rng) -> T; }
```

All operators must stay in-bounds for their axis schema. `debug_assert!` in dev builds.
All operators allocation-free in hot path.

### 4.2 Systems

| System | Description |
|---|---|
| Population engine | `evaluate → crowd-replace → tournament-select (size 3) → crossover → mutate → elitism → inject randoms → next gen`. All steps in pre-allocated buffers. Target <50ms wall-clock per gen excluding LLM time. |
| Diversity engine | candle-backed `MiniLM-L6-v2` (~80MB, ~400μs/embed). Pairwise cosine sim → distance matrix → deterministic crowding. Heatmap data exposed via SSE. |
| Cost meter | `Arc<CostMeter>` with `AtomicU64` token/dollar counters and `AtomicBool` tripped flag. Every `.complete()` checks the flag first. Hard ceiling configurable; soft warning at 75%. |
| Fitness cache | `FitnessCache` trait. v1 default `DashMapCache`. Key: `(genome_fingerprint, benchmark_input_id, model_id, sample_index)`. Cache hit short-circuits LLM call. |
| Failure classifier | Wraps `Evaluator` output. Transient (rate-limit, 5xx, timeout) → assign mean population fitness. Semantic (parse error, validation fail) → 0 fitness. One retry with exponential backoff. |

### 4.3 Surfaces

| Crate | Role |
|---|---|
| `evolve-storage` | sqlx against SQLite (default) or Postgres. Tables: `runs`, `generations`, `genomes`, `evaluations`, `cost_events`. Migrations via sqlx migrate. |
| `evolve-service` | axum (HTTP) + tonic (gRPC) sharing a single tower service. `POST /runs`, `GET /runs/{id}/generations` (SSE), `GET /runs/{id}/best`. |
| `evolve-runtime` | Standalone axum server exposing `/v1/chat/completions` (+ tool-calling) wrapping a frozen evolved genome. OpenAI-compat surface. |
| `evolve-dashboard` | SPA (leptos OR Vite+React, decision in plan) embedded via `rust-embed`. Live SSE-driven fitness curve, diversity heatmap, best-genome inspector, cost burn-down. |
| `evolve-cli` | clap-based. Commands: `evolve run <config.yaml>`, `evolve serve [--dashboard]`, `evolve export <run-id>`, `evolve worker` (stub for v1). |
| `bindings/python` | PyO3 wrapping `evolve-core` + in-process `evolve-service`. Async via `pyo3-async-runtimes`. |
| `bindings/typescript` | napi-rs wrapping the same. Async via napi-rs's tokio bridge. |
| `adapters/*` | Thin shims: Python `Runnable` (LangChain), Python `dspy.Module` (DSPy), TS `LanguageModelV1` (Vercel AI). Each calls `evolve-runtime`'s OpenAI-compat endpoint. |

---

## 5. Data Flow

### 5.1 Run lifecycle (outer loop)

```
config.yaml → Run init → seed RNG → load Benchmark → load Provider(s) (cost-metered)
                  ↓                                              ↓
              persist Run row in storage ◄────────────────────────┘
                  ↓
        generate Generation 0 (random, schema-driven) → Generation cycle ┐
                  ↓                                                       │
         while gen_n < max && !budget_tripped ◄─────────────────────────┘
                  ↓
          select best genome by fitness
                  ↓
   Export pipeline: JSON + snippet + runtime-spec + adapter-stubs
```

### 5.2 Generation cycle (inner loop)

1. Evaluate population[N] via Evaluator stream (cache lookups short-circuit)
2. Failed evaluations classified (transient → mean fitness; semantic → 0)
3. Identify contested top-30% (top 15 + ties within 0.02)
4. Re-evaluate contested at k=2 → fitness as mean
5. Compute prompt embeddings (candle, batched) → pairwise distance matrix
6. Tournament selection (size 3, with-replacement) → parent pairs
7. Crossover (axis-typed) → offspring
8. Mutation (per-axis rate from config) → mutated offspring
9. Deterministic crowding: each offspring competes against nearest parent
10. Elitism: top 5 carried forward unchanged with cached fitness
11. Random injection: 5 fresh randoms replace bottom 5
12. Persist Generation + Genome + Evaluation rows in one transaction
13. Emit `GenerationComplete` event to dashboard SSE
14. → next generation

### 5.3 Cost gating

Every `Provider.complete()` wrapped by `CostMetered<P>`:
- Atomic load of `tripped` flag → if true, return `BudgetExhausted` immediately
- Otherwise call inner, record token/dollar usage atomically, check ceiling, trip if over
- Trip mid-generation aborts cleanly: stream ends early, partial state persisted, run marked `budget_exhausted`

### 5.4 Cache + k=2 interaction

Cache key: `hash(genome.fingerprint(), benchmark_input.id, provider_config.model_id, sample_index)`.

- **Elites**: same key as last generation → 100% cache hit → zero cost
- **Contested re-eval (k=2)**: `sample_index=1` → cache miss → genuine new sample (preserves variance reduction)
- **Crossover/mutation children**: new fingerprint → cache miss → new call

### 5.5 Dashboard live update

Engine broadcasts `EvalResult`, `GenerationComplete`, `CostEvent` via `tokio::sync::broadcast`
(bounded, lossy on slow consumers — acceptable for UI). Axum SSE handler bridges to browser
`EventSource`. Dashboard renders live fitness curve, diversity heatmap, cost burn-down,
best-genome inspector.

### 5.6 Export pipeline (after final generation)

1. Write `best.json` (genome serialized)
2. Generate `runtime-spec.json` (provider + model + prompt + tools + hyperparams)
3. Generate code snippets (`usage.py`, `usage.ts`, `usage.rs`) targeting the OpenAI-compat runtime
4. Generate adapter stubs (`langchain_adapter.py`, `dspy_adapter.py`, `vercel_ai_provider.ts`)

---

## 6. Error Handling

### 6.1 Error taxonomy

`evolve-core` and `evolve-providers` define library errors with `thiserror`.
Application crates use `anyhow` for context-rich top-level handling.

```rust
#[derive(thiserror::Error, Debug)]
pub enum ProviderError {
    #[error("transient: {0}")]   Transient(String),
    #[error("semantic: {0}")]    Semantic(String),
    #[error("budget exhausted")] BudgetExhausted,
    #[error("misconfigured: {0}")] Misconfigured(String),
}

#[derive(thiserror::Error, Debug)]
pub enum EvolveError {
    #[error(transparent)] Provider(#[from] ProviderError),
    #[error(transparent)] Storage(#[from] StorageError),
    #[error(transparent)] Schema(#[from] SchemaError),
    #[error(transparent)] Operator(#[from] OperatorError),
    #[error(transparent)] Cache(#[from] CacheError),
}
```

### 6.2 Per-failure-mode handling

| Failure | Detection | Action | User-visible |
|---|---|---|---|
| LLM transient (429, 5xx, timeout) | HTTP status / tokio timeout | 1 retry with exp backoff (200ms→800ms); on second fail, mean-population fitness | Tracing warn, dashboard "transient retry" badge |
| LLM semantic (4xx, parse fail) | HTTP 4xx OR `parse_output` Err | No retry. 0 fitness | Tracing info, "parse fail" badge |
| Budget exhausted | `CostMetered.complete()` flag | Returns `BudgetExhausted` immediately. Engine stops cleanly, persists partial state | Dashboard red banner; CLI exits 2 |
| Provider misconfigured | First call returns 401/404 | Fail-fast at run startup. Validate at `evolve run` invocation, not lazily | CLI exits 1 with concrete fix message |
| Storage failure | sqlx error | 1 retry. Persistent fail: write to filesystem fallback, log, continue. Run marked `degraded` | Dashboard yellow banner |
| Schema validation | At `Run` init via `GenomeSchema::validate()` | Fail-fast before generation 0 | CLI exits 1 with line/column |
| Operator failure (out-of-bounds) | Invariant check | `debug_assert!` in dev; in release, log+skip, fill from random injection pool | "operator anomaly" badge |
| Embedding failure | candle error | Model load fail → fail-fast. Runtime fail → skip crowding for THIS gen, continue with elitism+injection | "diversity skipped gen N" |
| HTTP/gRPC client disconnect mid-SSE | tokio channel close | Engine continues; reconnect resumes | Silent recovery |
| Cache hash collision (theoretical) | Value validation hash | Treat as miss, recompute, log warning | Tracing warn |

### 6.3 Propagation rules

1. `evolve-core` never panics in release. All invariants checked with `Result`.
2. Errors carry context via `anyhow::Context` at every layer boundary.
3. The Engine's outer loop catches and persists, never dies on one bad generation.
   Three consecutive total-failure generations triggers shutdown with `degraded_terminated` status.
4. Cost meter is the only error that's allowed to short-circuit. Budget exhaustion is a
   deliberate state, not a failure.

### 6.4 Tracing

`tracing` crate, JSON output by default, span-per-generation + span-per-evaluation.
OTEL exporter wired but disabled unless `EVOLVE_OTEL_ENDPOINT` env var is set.

---

## 7. Testing Strategy

### 7.1 Test layers

| Layer | Target | Description |
|---|---|---|
| Unit (`evolve-core`) | ~150 tests, ≥80% line cov | Every operator/mutator/crosser. proptest for invariants. loom for cost-meter concurrency. |
| Integration (`evolve-core` + MockProvider) | ~30 tests | Full generation cycles, no LLM. Reproducibility test (bit-identical re-runs), budget gating, resilience. |
| Provider conformance (`evolve-providers`) | ~60 tests | Per-provider request/response shape, error mapping, cost calculation. Wiremock cassettes committed under `tests/cassettes/`. Plus `--ignored` smoke tests against real APIs. |
| Service tests (`evolve-service`) | ~25 tests | axum + tonic via in-memory transports. HTTP/gRPC parity. State survives restart. |
| Bindings parity | 4 tests | Same canonical scenario runs from Rust, Python, TS, HTTP. All four produce bit-identical results given the same seed. |
| Adapter tests | 3 tests | One per adapter (LangChain, DSPy, Vercel AI). Verifies adapter calls runtime correctly. |
| End-to-end validation | 2 runs | `synthetic-classification` in CI per PR (Ollama via testcontainers, $0). `gsm8k-headline` nightly on main (Anthropic Haiku, ~$5). |

### 7.2 Benchmarks (`benches/` via criterion)

- `bench_generation_step_no_llm` — full gen cycle against MockProvider, target <50ms wall-clock for pop=50
- `bench_operator_throughput` — sub-ms per individual on every operator
- `bench_embedding_throughput` — candle MiniLM batch embedding, target <500μs/genome at batch=50
- `bench_cache_lookup` — DashMap fitness cache, target <100ns per lookup

Regressions tracked via `criterion-compare-action` in CI.

### 7.3 TDD discipline

Per the brief, TDD is mandatory. Codified as a CI gate: PRs adding a public function in
`evolve-core` without a corresponding test in the same diff fail CI.

Coverage thresholds enforced via `cargo-llvm-cov`:
- `evolve-core`: ≥80%
- `evolve-providers`: ≥75%
- `evolve-storage`: ≥70%
- Surfaces: presence of integration tests is sufficient

### 7.4 CI matrix

GitHub Actions:
- Rust: `cargo test --workspace` on Linux/macOS/Windows × stable + MSRV
- Python: pytest on CPython 3.10/3.11/3.12 × Linux/macOS/Windows
- TypeScript: `node --test` on Node 20/22 × Linux/macOS/Windows
- Adapters: per-adapter test in their own job
- Nightly: GSM8K headline run on Linux/Anthropic, posts result to commit status

---

## 8. Out of Scope (v1)

- Multi-task agents (one task per run; multi-task in v1.2+)
- Cross-population learning (separate research)
- Integration with GENOME (separate future experiment)
- Distributed worker mode (trait shaped, impl in v1.1)
- Cloud-hosted version (v2+, post-adoption)
- Fine-tuning or gradient-based updates (out of scope by design)
- Hand-rolled LLM client (use `async-openai` + thin reqwest Anthropic)
- LlamaIndex / Mastra / Instructor adapters (community contributions in v1.1+)

---

## 9. Public Roadmap

| Version | Scope | Target |
|---|---|---|
| v1.0 GA | Everything in this design doc | TBD (sized in implementation plan) |
| v1.1 | Distributed worker mode (`evolve worker` full impl, gRPC coordinator), cloud-portable storage layer | 8-12 weeks post v1 GA |
| v1.2 | Multi-task agents, additional benchmark library, contributed adapters | TBD |
| v2.0 | Hosted evolve.cloud (managed runs, team workspaces, usage-based billing) | Post-adoption |

---

## 10. Decision Log

Each decision below was reached through one-question-at-a-time brainstorming on 2026-04-23.
"Pivot" rows mark moments where the user override changed the design direction.

1. **Sub-ms harness budget (Q1 → D)**: User clarified mid-conversation that sub-ms latency
   matters across the entire non-LLM hot path. Forces candle for embeddings,
   allocation-conscious operators. LLM calls remain the only wall-clock-dominant work.
2. **Synthetic-then-GSM8K (Q2 → D)**: Validate the loop on $0 synthetic, then run GSM8K
   for the headline. Cost and risk increase monotonically.
3. **All three providers (Q3 → C)**: User chose maximum portability over the recommended
   2-backend approach. Refinement: `async-openai` covers OpenAI + Ollama via base_url swap.
4. **8-axis genome with riders (Q4 → C + R1 + R2)**: User overrode minimal-axis recommendation;
   pushed for final-product shape day one. Riders ensure honesty: real calculator tool
   implementation prevents tools axis being dead code; per-axis mutation rates allow
   focusing evolution per-task.
5. **Pop=50, gens=30 (Q5 → C)**: Publication-quality population/generation count. $80 ceiling
   with safety margin.
6. **Belt-and-suspenders diversity (Q6 → E)**: Elitism + random injection + deterministic
   crowding. Crowding enables the "evolution discovered N distinct strategies" story.
7. **Response cache + k=2 contested (Q7 → E)**: Cache makes runs deterministically reproducible
   and cuts cost ~30-40%. K=2 on contested top reduces tie-break noise.
8. **Hybrid prompt mutation (Q8 → E, after pivot)**: Originally recommended D (LLM-as-mutator
   only). After pivot to "build the final product," upgraded to E (template slots +
   LLM-as-mutator) — interpretable structure plus high-quality wordsmithing.
9. **PIVOT — final product, not v0 validation**: User explicitly redirected scope from
   research experiment to adoption-grade framework. All earlier answers reframed.
10. **Rust core + Python + TS + HTTP/gRPC (Q9 → C+D)**: All four integration surfaces day one.
    Honest scope flag: this is "Polars for agent evolution" — weeks-to-months timeline.
11. **3-on-ramp genome extensibility (Q10 → C)**: Turnkey default + declarative DSL +
    advanced trait. DSL is the wire-compatible path.
12. **Export = JSON + runtime + adapters (Q11 → E)**: OpenAI-compatible runtime is the
    single highest-leverage feature. 3 framework adapters in v1; community for the rest.
13. **SQLite + Postgres + dashboard (Q12 → C)**: Dashboard via rust-embed bundled SPA is
    the biggest adoption multiplier. SSE-driven live updates.
14. **Single-node v1 with shaped trait (Q13 → B)**: Distribution shipped in v1.1 with
    public roadmap commitment (8-12 weeks post v1 GA). Polish over breadth wins for
    adoption.
15. **Retry + classify failure handling (Q14 → E)**: Distinguishes transient (assign mean
    fitness) from semantic (0 fitness). Production-grade rather than naive.

---

## 11. Acceptance Criteria

This design is considered correctly implemented when:

1. `cargo test --workspace` passes on Linux/macOS/Windows × stable Rust
2. `pip install evolveai` then a 5-line Python script runs a 5-generation
   `MockProvider` evolution and prints fitness curve
3. `npm install @evolveai/sdk` then a 5-line TS script does the same
4. `evolve serve --dashboard` exposes the dashboard at localhost; running a real
   GSM8K evolution updates the UI live
5. `examples/synthetic-classification/` produces best-genome fitness > random-baseline
   fitness by ≥2σ across 5 seeds
6. `examples/gsm8k-headline/` produces best-genome accuracy > baseline-prompt accuracy
   by ≥5 percentage points, total cost <$10
7. Bindings parity test: identical seed produces identical fitness curve from Rust,
   Python, TS, and HTTP
8. `evolve export <run-id>` produces a runnable runtime + working snippets + working
   LangChain/DSPy/Vercel adapters
9. Coverage thresholds met: `evolve-core` ≥80%, `evolve-providers` ≥75%, `evolve-storage` ≥70%
10. Cost meter abort test: a run with $0.10 ceiling halts cleanly with `budget_exhausted`
    and persisted partial state

---

*End of design document. Implementation plan to be produced by the writing-plans skill,
saved alongside as `2026-04-23-evolve-validation-implementation.md`.*

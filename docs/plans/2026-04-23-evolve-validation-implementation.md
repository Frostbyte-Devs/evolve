# Evolve Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build Evolve v1.0 — a drop-in passive evolution framework for existing AI coding agents (Claude Code, Cursor, Aider). The user installs Evolve, keeps using their tool normally, and Evolve learns what configurations work best for their specific projects via champion/challenger A/B testing with Bayesian promotion. ~$0 idle cost; ~$0.01-$0.10/mo per active project for occasional challenger generation. Local-only, no telemetry.

**Architecture:** Cargo workspace with 7 small crates. `evolve-core` holds AgentConfig types, the schema DSL, mutation operators, Bayesian promotion logic, and signal aggregation. `evolve-storage` is SQLite-only persistence. `evolve-llm` is one minimal client used for occasional challenger generation. `evolve-adapters` ships Claude Code + Cursor + Aider adapters. `evolve-proxy` is the OpenAI-compat fallback for Cursor. `evolve-dashboard` is a bundled SPA. `evolve-cli` is the user-facing surface.

**Tech Stack:** Rust 2024, sqlx (SQLite), reqwest, async-openai (for Ollama only), axum, rust-embed, PyO3 + maturin, napi-rs. See `2026-04-23-evolve-validation-design.md` for full design rationale.

**Total realistic timeline:** 25-35 working days for one full-time engineer. (Smaller than the pre-pivot design because passive evolution is architecturally simpler than population-based optimization — no diversity engine, no population sizing, no batch evaluation, no benchmark runners, no provider matrix, no service mode.)

---

## Status & Pivot Note

This plan replaces the pre-pivot implementation plan (preserved in git at commit `4c1eab2`)
which targeted active-evolution-on-benchmark architecture. After Phase 0 + Task 1.1 of the
old plan, the user redirected the product to passive drop-in evolution. The work
already completed is *preserved and reused*:

| Phase / Task | Status | Notes |
|---|---|---|
| Old Phase 0 (workspace foundation) | ✅ DONE — commits `2318595`..`2fd0009` | Reused as-is |
| Old Task 1.1 (`GenomeSchema` DSL) | ✅ DONE — commit `9b2cf3c` | Reused as-is; the DSL describes the shape of `AgentConfig` |
| Everything from old Task 1.2 onward | Replaced by this plan | Old plan invalidated by pivot |

---

## Phase Index (new architecture)

| Phase | Scope | Est. Days | Status |
|---|---|---|---|
| 0 | Workspace foundation | 1-2 | ✅ DONE |
| 1 | Schema DSL + validation (reuses old Task 1.1; adds validate, AgentConfig type, basic operators) | 3-4 | 🟡 IN PROGRESS (1.1 done) |
| 2 | SQLite storage (`evolve-storage` crate, schema, migrations, repositories) | 2-3 | Pending |
| 3 | Bayesian promotion + signal aggregation in `evolve-core` | 2 | Pending |
| 4 | `evolve-llm` minimal client (Anthropic Haiku + Ollama) | 1 | Pending |
| 5 | Mutation operators (LLM-as-mutator + 4 rule-based mutators) | 2 | Pending |
| 6 | `evolve-adapters` trait + ClaudeCodeAdapter (config, hook install, transcript parser, signal extraction) | 4 | Pending |
| 7 | CursorAdapter (`.cursorrules` rewriter; falls back to proxy for live signals) | 2 | Pending |
| 8 | AiderAdapter (`aider.conf.yml` rewriter + post-commit git hook) | 2 | Pending |
| 9 | `evolve-proxy` (OpenAI-compat HTTP fallback for Cursor and friends) | 2 | Pending |
| 10 | `evolve-cli` (init, record-*, status, good/bad, roll, list, forget) | 2-3 | Pending |
| 11 | `evolve-dashboard` (bundled SPA, axum + rust-embed, champion-vs-challenger UI) | 4-5 | Pending |
| 12 | Python bindings (PyO3 + maturin) | 2 | Pending |
| 13 | TypeScript bindings (napi-rs) | 2 | Pending |
| 14 | Examples + documentation (mdBook, walkthrough per adapter) | 2-3 | Pending |
| 15 | Release pipeline (crates.io + PyPI + npm + GitHub Releases + Docker) | 1-2 | Pending |

**Bite-sized expansion policy:** Phase 1 (the only phase with partial completion) is bite-sized below. Phases 2-15 are structured task lists with file paths, key types, and test signatures — re-invoke `superpowers:writing-plans` to expand any phase to bite-sized form before execution.

---

# PHASE 1 — Schema DSL, Validation, AgentConfig

**Goal:** The `GenomeSchema` DSL types (already built), schema validation, the universal `AgentConfig` struct + per-tool extension hook, and the foundational types (`ProjectId`, `ConfigId`, `ExperimentId`, `SessionId`) used everywhere downstream.

## Task 1.1 — `GenomeSchema` DSL types

✅ **DONE** — commit `9b2cf3c`. `crates/evolve-core/src/schema.rs` defines `FieldSchema` (5 variants), `StringMutatorKind` (LlmRewrite + TemplateSlot), `GenomeSchema { fields, mutation_rates }`, and a serde-roundtrip test.

## Task 1.2 — `GenomeSchema::validate`

**Files:** Modify `crates/evolve-core/src/schema.rs`

**Step 1: Append 5 failing tests to the existing tests module:**
- `validate_rejects_inverted_float_range`
- `validate_rejects_empty_categorical`
- `validate_rejects_mutation_rate_for_unknown_field`
- `validate_rejects_mutation_rate_out_of_range`
- `validate_accepts_well_formed_schema`

**Step 2:** Run tests, confirm compile error (no `validate` method).

**Step 3:** Add `SchemaError` enum (6 variants) and `impl GenomeSchema { pub fn validate() -> Result<(), SchemaError> }` covering: float range inverted, integer range inverted, empty categorical, empty set pool, unknown mutation-rate field, mutation rate out of [0.0, 1.0].

**Step 4:** Run tests — 6 pass.

**Step 5:** `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all -- --check` — both clean.

**Step 6:** Commit `feat(schema): add GenomeSchema::validate with explicit SchemaError`.

## Task 1.3 — Foundational ID types

**Files:** Create `crates/evolve-core/src/ids.rs`; add `pub mod ids;` to `lib.rs`.

Define newtype-wrapped `Uuid` for: `ProjectId`, `ConfigId`, `ExperimentId`, `SessionId`, `SignalId`, `AdapterId` (this last is `String`-backed for stability across machines: `"claude-code"`, `"cursor"`, `"aider"`).

Add `uuid` to workspace deps (`uuid = { version = "1", features = ["v4", "serde"] }`).

Tests: each ID newtype roundtrips through serde JSON; Display implements as the inner UUID; PartialEq/Eq/Hash all derived correctly.

## Task 1.4 — `AgentConfig` universal type

**Files:** Create `crates/evolve-core/src/agent_config.rs`; add `pub mod agent_config;` to `lib.rs`.

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentConfig {
    pub system_prompt_prefix: String,
    pub model_pref: ModelPref,
    pub behavioral_rules: BTreeSet<String>,
    pub tool_permissions: BTreeSet<String>,
    pub response_style: ResponseStyle,
    pub extensions: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ModelPref {
    ClaudeOpus, ClaudeSonnet, ClaudeHaiku,
    Gpt4o, Gpt4oMini,
    Ollama(String),  // model name
    AnyCheap,        // adapter picks
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ResponseStyle { Terse, Normal, Verbose }
```

Methods:
- `AgentConfig::default_for(adapter_id: &str) -> Self` — returns a sane starting point per adapter
- `AgentConfig::fingerprint(&self) -> u64` — stable hash for cache/dedup
- `AgentConfig::extension<T: DeserializeOwned>(&self, key: &str) -> Option<T>` — typed extension reader

Tests: roundtrip serde, fingerprint stable across clones, fingerprint differs when any field changes, default_for returns non-empty values for all 3 v1 adapter ids.

## Task 1.5 — Property tests for AgentConfig

`proptest` invariants in the same file:
- "fingerprint changes whenever ANY field changes" (proptest with arbitrary AgentConfig pairs)
- "serde roundtrip is identity for any AgentConfig"

## Task 1.6 — Phase 1 verification

`cargo test --workspace`, `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo llvm-cov --package evolve-core --summary-only` ≥80%.

**PHASE 1 COMPLETE when all gates pass.**

---

# PHASE 2 — SQLite Storage (`evolve-storage`)

**Goal:** Embedded SQLite at `~/.evolve/evolve.db` with `projects`, `agent_configs`, `experiments`, `sessions`, `signals` tables. Repository pattern over sqlx. Migrations via `sqlx migrate`.

**Tasks:**

- 2.1 — Create `crates/evolve-storage/` (Cargo.toml + src/lib.rs). Add to workspace.
- 2.2 — Write SQL migration `0001_init.sql` defining all 5 tables with PK/FK/indexes. Include explicit UNIQUE constraints (e.g., one running experiment per project).
- 2.3 — `Storage` struct wrapping `SqlitePool`. `Storage::open(path)` and `Storage::migrate()`.
- 2.4 — `ProjectRepo` (insert, get_by_id, get_by_root_path, list, delete).
- 2.5 — `AgentConfigRepo` (insert, get_by_id, latest_for_project_role).
- 2.6 — `ExperimentRepo` (insert, get_running_for_project, list_completed, update_status).
- 2.7 — `SessionRepo` (insert, list_recent, list_for_experiment).
- 2.8 — `SignalRepo` (insert, list_for_session, list_for_config).
- 2.9 — Integration tests against `sqlite::memory:` covering each repo.
- 2.10 — Schema-survives-restart test: open→write→drop→reopen→read same data.

**PHASE 2 COMPLETE.**

---

# PHASE 3 — Bayesian Promotion + Signal Aggregation

**Goal:** The math engine. Pure functions, no I/O.

**Tasks:**

- 3.1 — `Signal::weight()` — explicit signals weighted 5x implicit by default; configurable.
- 3.2 — `aggregate(session_signals: &[Signal]) -> f64` — collapses N signals from one session into a single 0..=1 fitness score (weighted mean, explicit overrides win on conflict).
- 3.3 — `posterior_probability(champion: &[f64], challenger: &[f64], samples: u32 = 10_000) -> f64` — Monte Carlo estimate of P(challenger > champion) under beta-binomial posteriors with α=1+wins, β=1+losses.
- 3.4 — `promotion_decision(...) -> Decision` (Promote | Hold | NeedMoreData) using thresholds.
- 3.5 — Property tests: at extreme inputs (all challenger wins / all losses) decision is correct with probability ≥ threshold.
- 3.6 — Bench (`benches/promotion.rs`) — promotion_decision should run in <1ms for typical session counts.

**PHASE 3 COMPLETE.**

---

# PHASE 4 — `evolve-llm` Minimal Client

**Goal:** ONE LLM client for ONE purpose: occasionally generating challenger configs.

**Tasks:**

- 4.1 — Create `crates/evolve-llm/` (Cargo.toml + src/lib.rs).
- 4.2 — `LlmClient` trait: `async fn complete(prompt: &str, max_tokens: u32) -> Result<String>`.
- 4.3 — `AnthropicHaikuClient` via reqwest. Reads `ANTHROPIC_API_KEY` env. Retries 1x on transient.
- 4.4 — `OllamaClient` via async-openai (`base_url` = `http://localhost:11434/v1`). No auth.
- 4.5 — `pick_default_client()` — tries Ollama first; falls back to Anthropic if key set; returns `Err(NoLlmAvailable)` if neither.
- 4.6 — Cassette tests for both via `wiremock`. `--ignored` smoke tests against real APIs.
- 4.7 — Token cost tracker (passive — we just want to log how many cents we've spent, no enforcement at this size).

**PHASE 4 COMPLETE.**

---

# PHASE 5 — Mutation Operators

**Goal:** Five mutators that take an `AgentConfig` and produce a varied "challenger" config. Each generation, ONE is applied (chosen probabilistically).

**Tasks:**

- 5.1 — `Mutator` trait: `async fn mutate(&self, parent: &AgentConfig, llm: &dyn LlmClient, rng: &mut dyn RngCore) -> Result<AgentConfig>`.
- 5.2 — `LlmRewriteMutator` — calls `llm.complete()` with prompt template asking for a small variation of the system_prompt_prefix.
- 5.3 — `BehavioralRulesMutator` — adds, removes, or rephrases ONE rule from a curated pool of 30 candidate rules.
- 5.4 — `ResponseStyleMutator` — cycles through `{Terse, Normal, Verbose}`.
- 5.5 — `ModelPrefMutator` — swaps to a neighboring model from a per-adapter pool.
- 5.6 — `ToolPermissionsMutator` — toggles ONE permission from the adapter's allow-list.
- 5.7 — `MutatorPicker` — weighted random selection (50/15/15/10/10).
- 5.8 — Tests: each mutator returns a valid AgentConfig that differs from input in exactly one axis.
- 5.9 — Test: with seeded RNG, sequence of mutations is deterministic.

**PHASE 5 COMPLETE.**

---

# PHASE 6 — Adapter Trait + Claude Code Adapter

**Goal:** The Adapter trait + the most important adapter (Claude Code). This is THE load-bearing milestone — if Claude Code adapter doesn't work end-to-end, nothing else matters.

**Tasks:**

- 6.1 — Create `crates/evolve-adapters/`.
- 6.2 — Define `Adapter` trait (see design doc 4.4).
- 6.3 — `AdapterRegistry` — runtime lookup `&str → Box<dyn Adapter>`.
- 6.4 — `ClaudeCodeAdapter::detect()` — checks for `.claude/`, `CLAUDE.md`, etc. in repo or `~/.claude/`.
- 6.5 — `ClaudeCodeAdapter::install()` — modifies `~/.claude/settings.json` to add a `Stop` hook calling `evolve record-claude-code <transcript-path>`. Idempotent. Detects user's existing hook config and merges safely.
- 6.6 — `ClaudeCodeAdapter::apply_config()` — writes managed section into `~/.claude/CLAUDE.md` (or per-project `CLAUDE.md` for project-specific configs) bracketed by `<!-- evolve:start -->` / `<!-- evolve:end -->` markers. Never touches content outside markers.
- 6.7 — `ClaudeCodeAdapter::parse_session()` — reads transcript JSONL, extracts:
  - Session duration (very short = abandoned)
  - User /clear command (negative)
  - Bash exit codes for test/lint commands (positive if 0)
  - User feedback patterns ("redo", "no, that's wrong", "thanks!", etc. — regex-matched)
  - Subagent invocations and their outcomes
  Returns `Vec<Signal>`.
- 6.8 — Test fixtures: 5 hand-crafted Claude Code session transcripts in `tests/fixtures/claude-code/`. Test `parse_session` extracts expected signals from each.
- 6.9 — Idempotency test: install → install again → settings.json unchanged the second time.
- 6.10 — Restoration test: `forget` removes the managed section and restores original settings.json.

**PHASE 6 COMPLETE.**

---

# PHASE 7 — Cursor Adapter

**Tasks:**

- 7.1 — `CursorAdapter::detect()` — checks for `.vscode/`, `.cursorrules`, or `cursor` in PATH.
- 7.2 — `CursorAdapter::install()` — writes initial `.cursorrules` (managed-section-bracketed) AND prints instructions for the user to point Cursor's "Custom OpenAI base URL" at `http://localhost:7777` (the Evolve proxy) for live signal capture.
- 7.3 — `CursorAdapter::apply_config()` — updates `.cursorrules` managed section.
- 7.4 — `CursorAdapter::parse_session()` — for v1, primary signal source is the proxy (Phase 9), not Cursor logs. Adapter consumes proxy-emitted records.
- 7.5 — Test fixtures + parse tests for proxy-emitted records.

**PHASE 7 COMPLETE.**

---

# PHASE 8 — Aider Adapter

**Tasks:**

- 8.1 — `AiderAdapter::detect()` — checks for `.aider.conf.yml` or `.aider.tags.cache.v3` or `aider` in PATH.
- 8.2 — `AiderAdapter::install()` — writes managed section in `aider.conf.yml`; installs git `post-commit` hook calling `evolve record-aider HEAD`.
- 8.3 — `AiderAdapter::apply_config()` — updates managed section.
- 8.4 — `AiderAdapter::parse_session()` — for a given commit SHA: runs configured lint+test commands, checks exit codes, looks at whether the commit was reverted within 24h. Emits Signals.
- 8.5 — Tests with fixture git repo state.

**PHASE 8 COMPLETE.**

---

# PHASE 9 — `evolve-proxy`

**Goal:** OpenAI-compat HTTP server that injects active config's `system_prompt_prefix` into requests, forwards to upstream, and records suggestion accept/reject as signals.

**Tasks:**

- 9.1 — Create `crates/evolve-proxy/`. axum-based.
- 9.2 — `POST /v1/chat/completions` endpoint. Reads active config for the calling project (project identified by request header or by directory of running process — TBD per adapter).
- 9.3 — Inject `system_prompt_prefix` into the request's system message.
- 9.4 — Forward to upstream (configurable: real OpenAI, real Anthropic, real Ollama). Stream response back.
- 9.5 — Track signal candidates: which assistant message, which session, which variant. Persist via `SessionRepo` and `SignalRepo`.
- 9.6 — Auth: shared secret env var, OR localhost-only binding (default).
- 9.7 — `evolve proxy --for cursor` CLI subcommand (Phase 10) starts it.
- 9.8 — Cassette tests; integration test: real openai-python client → proxy → mock upstream → signal recorded.

**PHASE 9 COMPLETE.**

---

# PHASE 10 — `evolve-cli`

**Goal:** Single user-facing binary. Every command in the design doc 4.7.

**Tasks:**

- 10.1 — Create `crates/evolve-cli/`. clap-based.
- 10.2 — `evolve init <adapter>` — detect, install, write initial config.
- 10.3 — `evolve record-claude-code <transcript-path>` — parse, store, check promotion.
- 10.4 — `evolve record-aider <commit-sha>` — same.
- 10.5 — `evolve record-cursor-event <event-json>` — same (used by proxy).
- 10.6 — `evolve good` / `evolve bad` — explicit override on most recent session.
- 10.7 — `evolve thumbs <session-id> <±>` — explicit on a specific session.
- 10.8 — `evolve roll` — force challenger generation now.
- 10.9 — `evolve status` — one-line per-project summary.
- 10.10 — `evolve dashboard [--port N]` — start dashboard.
- 10.11 — `evolve proxy --for <adapter>` — start proxy.
- 10.12 — `evolve list` — list known projects.
- 10.13 — `evolve forget <project-id> | --all` — remove project + restore original configs.
- 10.14 — Integration test: `evolve init claude-code` in a tmpdir; assert hook installed; record fake session; assert signal stored.

**PHASE 10 COMPLETE.**

---

# PHASE 11 — `evolve-dashboard`

**Goal:** Local-only web UI that makes evolution visible.

**Tasks:**

- 11.1 — Create `crates/evolve-dashboard/`. axum + rust-embed.
- 11.2 — Decide SPA framework: Vite+React (recommended for contributor pool) OR leptos (Rust-only). Document decision.
- 11.3 — Web build under `crates/evolve-dashboard/web/`. Components: ProjectList, ProjectDetail, ExperimentDetail, SignalTimeline, PromotionLog, Settings.
- 11.4 — REST API endpoints: `GET /projects`, `GET /projects/:id`, `GET /projects/:id/experiments`, `GET /projects/:id/sessions?limit=N`, `GET /projects/:id/promotion-log`, `POST /signals/:id/override`, `POST /experiments/:id/abort`.
- 11.5 — SSE endpoint `GET /projects/:id/live` pushing new sessions/signals as they arrive.
- 11.6 — `npm run build` produces `web/dist/`; `rust-embed` bundles it.
- 11.7 — `evolve dashboard` opens browser to localhost.
- 11.8 — Manual UI test against real SQLite data.

**PHASE 11 COMPLETE.**

---

# PHASE 12 — Python Bindings (PyO3)

**Goal:** `pip install evolveai`; scriptable evolution control from Python.

**Tasks:**

- 12.1 — `bindings/python/` with maturin. Wrap `Storage`, `AdapterRegistry`, `Mutator`, `promotion_decision`.
- 12.2 — Async via `pyo3-async-runtimes`.
- 12.3 — Pytest suite. Parity test: 5 simulated sessions → expected promotion → identical to Rust result.
- 12.4 — Wheel build CI for Linux/macOS/Windows × CPython 3.10/3.11/3.12.

**PHASE 12 COMPLETE.**

---

# PHASE 13 — TypeScript Bindings (napi-rs)

**Tasks:**

- 13.1 — `bindings/typescript/` with napi-rs. Wrap same surface as Python.
- 13.2 — Async via napi-rs's tokio bridge.
- 13.3 — `node --test` suite. Parity test #2.
- 13.4 — Prebuilt binary CI.

**PHASE 13 COMPLETE.**

---

# PHASE 14 — Examples + Documentation

**Tasks:**

- 14.1 — mdBook scaffold under `docs/book/`. Chapters: Introduction, Quickstart (per adapter), How Evolution Works, Architecture, Cost & Privacy, FAQ, Contributing.
- 14.2 — `examples/claude-code-rust-project/` — a tiny Rust project with `evolve init claude-code` already configured; README walks the user through "use it for a week, watch it evolve."
- 14.3 — Same for Cursor (`examples/cursor-nextjs-project/`).
- 14.4 — Same for Aider (`examples/aider-python-project/`).
- 14.5 — GitHub Pages workflow deploys mdBook on every push to main.

**PHASE 14 COMPLETE.**

---

# PHASE 15 — Release Pipeline

**Tasks:**

- 15.1 — `cargo-dist` for prebuilt CLI binaries.
- 15.2 — `maturin-action` for PyPI publishing.
- 15.3 — `napi publish` for npm publishing.
- 15.4 — Multi-arch Docker image (`ghcr.io/<user>/evolve:<version>`).
- 15.5 — `CHANGELOG.md` (Keep a Changelog format).
- 15.6 — v1.0.0 release: tag, push, verify all artifacts published.

**PHASE 15 COMPLETE — v1.0 GA.**

---

## Cross-Cutting Concerns

### Privacy invariants (enforced by tests)

- No source code in `signals` table — tests assert `signals.payload` columns never contain code-like content
- No prompts logged unless `--include-prompts` opt-in flag set on `evolve init`
- No outbound HTTP except to user-configured LLM provider (mutation only)

### Idempotency (enforced by tests)

- Every `evolve init` is idempotent (re-run = no change)
- Every adapter's `install()` is idempotent
- Every adapter's `apply_config()` is idempotent in the sense that reapplying the same config = no change to the file

### Observability

- `tracing` everywhere with structured fields. JSON logs for service mode (proxy/dashboard), pretty for CLI.
- `EVOLVE_LOG_LEVEL` env var controls verbosity.
- No OTEL by default (privacy posture). Opt-in via env var if user wants to wire it up themselves.

---

## Acceptance Criteria for v1.0 GA

(From design doc Section 11.)

1. `cargo test --workspace` passes on Linux/macOS/Windows × stable
2. `pip install evolveai` then `evolve init claude-code` in a Rust project: hooks installed, baseline `CLAUDE.md` snippet written
3. Same for Cursor and Aider
4. After 100 simulated sessions where challenger always succeeds and champion always fails: challenger promoted
5. Same scenario but with random tied-50/50 outcomes: champion holds, no false promotion
6. `evolve dashboard` serves a working SPA on localhost; shows current project's champion + experiment + signals
7. `evolve good` / `evolve bad` records explicit signals that override conflicting implicit signals
8. `evolve forget <project-id>` removes all SQLite rows AND restores the user's pre-evolve config files
9. Coverage thresholds met (`evolve-core` ≥80%, `evolve-storage` ≥75%)
10. Real-world soak test on 3 of my own active projects (one per adapter), 2 weeks, document what evolved

---

*End of implementation plan.*

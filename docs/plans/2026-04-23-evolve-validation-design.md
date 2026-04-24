# Evolve — Design Document

**Date:** 2026-04-23
**Author:** Kristian Baer (Northtek / FrostByte Digital)
**Status:** Approved, in implementation
**Companion document:** `2026-04-23-evolve-validation-implementation.md`

> **Note:** An earlier version of this document (commit `aac080a`) described an
> active-evolution-on-benchmark architecture. After Phase 0 + Task 1.1, the user pivoted
> the product framing to *passive drop-in evolution for existing agent tools*. This
> document reflects the new design. The earlier version is preserved in git history.

---

## 1. Context & Goal

**Evolve** is a drop-in framework that makes existing AI coding agents (Claude Code,
Cursor, Aider) measurably better at the user's specific projects, over time, without
the user changing how they work and without the framework adding meaningful LLM cost.

### Mechanism (one paragraph)

Evolve writes config files for the user's existing agent (`CLAUDE.md` snippets,
`.cursorrules`, `aider.conf.yml`). The user keeps using their tool normally. A hook
installed in the tool reports each session's outcome to Evolve via `evolve record-*`.
Periodically (about once a week, ~$0.01 in LLM cost on Haiku), Evolve generates a
"challenger" config that varies the active "champion." About 5% of sessions are routed
to the challenger; outcomes are tracked. When the Bayesian posterior P(challenger beats
champion) exceeds 0.95, the challenger is promoted to champion. The cycle repeats.
Result: the user's agent configuration evolves continuously toward what works on their
actual codebases. No benchmarks, no batch optimization, no model training.

### Why novel

- **PromptBreeder, EvoPrompt, DSPy** all do *active* prompt optimization against a
  user-provided benchmark, in batch. Evolve does *passive* optimization against the
  user's real day-to-day usage outcomes — no benchmark required.
- **GitHub Copilot, Cursor's built-in tuning** improve via aggregate vendor-side data
  collection across all users. Evolve is local-only, per-project, owned by the user.
- **Adapters first.** No existing tool ships drop-in adapters that work *with* Claude
  Code + Cursor + Aider rather than competing with them. The integration *is* the
  product.

### What survives from the pre-pivot work (already built)

- Cargo workspace at `~/projects/active/evolve` (`master` + `dev` branches)
- `evolve-core` crate scaffold
- `GenomeSchema` DSL (the field-type system is exactly what we need to declare the
  shape of an `AgentConfig`)
- Apache 2.0 LICENSE, README, CONTRIBUTING, GitHub Actions CI workflow

---

## 2. Locked Decisions Summary

| # | Area | Choice |
|---|---|---|
| 1 | Product framing | Drop-in passive evolution for existing agent tools |
| 2 | v1 adapters | Claude Code + Cursor + Aider |
| 3 | Drop-in mechanism | Config rewriter primary; OpenAI-compat proxy fallback for tools/scenarios where config is too rigid |
| 4 | Fitness signal | Implicit (parse session logs, watch test/lint exit codes, observe git diff acceptance) + explicit override commands (`evolve good` / `evolve bad`) |
| 5 | Champion/challenger algorithm | Bayesian posterior (beta-binomial); promote when P(challenger > champion) > 0.95 |
| 6 | Genome shape | Universal `AgentConfig` core + per-tool extension fields |
| 7 | Process model | Tool-hook driven; **no background daemon** |
| 8 | Privacy | Local-only by default; no telemetry; opt-in cloud sync deferred to v1.1+ |
| 9 | Storage | SQLite (always local) under `~/.evolve/`; tables: `projects`, `agent_configs`, `experiments`, `sessions`, `signals` |
| 10 | LLM usage by framework | Only for challenger generation, only when an experiment concludes; cheap model (Haiku via Anthropic, or Ollama if available); ~$0.01-0.10/mo per active project |
| 11 | Mutation | LLM-as-mutator for `system_prompt_prefix`; rule-based for `model_pref`, `tool_permissions`, `behavioral_rules`, `response_style` |
| 12 | Integration surfaces | Rust core + CLI + Python (PyO3) + TypeScript (napi-rs) + local web dashboard. **No HTTP/gRPC service mode** in v1 (no daemon = nothing to serve over network) |
| 13 | Dashboard | Bundled SPA via rust-embed; champion-vs-challenger view, per-project session history, model/config performance curves; served by `evolve dashboard` CLI on demand |
| 14 | Failure handling | Implicit-signal failures skip the session silently; explicit override always wins; LLM-mutation failure → reuse champion as challenger this cycle |
| 15 | Naming | `evolve` (Cargo crate, GitHub repo); `evolveai` (PyPI/npm packages); Apache 2.0 |

---

## 3. Architecture & Repository Shape

### 3.1 Workspace layout

```
evolve/
├── Cargo.toml              # workspace root (virtual workspace)
├── crates/
│   ├── evolve-core/        # AgentConfig types, schema DSL, mutation operators, Bayesian promotion, signal aggregation
│   ├── evolve-storage/     # SQLite via sqlx; tables for projects, configs, experiments, sessions, signals
│   ├── evolve-llm/         # one cheap LLM client (Haiku via reqwest; Ollama via async-openai with custom base_url); used solely for challenger generation
│   ├── evolve-adapters/    # adapter trait + ClaudeCodeAdapter + CursorAdapter + AiderAdapter
│   ├── evolve-proxy/       # OpenAI-compatible HTTP proxy fallback for non-config-driven scenarios
│   ├── evolve-dashboard/   # axum + rust-embed bundled SPA (champion-vs-challenger UI)
│   └── evolve-cli/         # clap CLI: evolve init, record-*, status, dashboard, good/bad, roll
├── bindings/
│   ├── python/             # PyO3 + maturin → pip install evolveai
│   └── typescript/         # napi-rs → npm install @evolveai/sdk
├── examples/
│   └── claude-code-rust-project/   # walkthrough: install, run normally, watch evolution
├── docs/                   # mdBook
└── docker/                 # optional Dockerfile for users who want it isolated
```

Note: the previous design's `evolve-providers`, `evolve-runtime`, `evolve-service`,
`evolve-bench` crates are gone. There's no service mode, no benchmark runner, no
multi-provider trait (we only need ONE provider for the rare challenger-generation call).

### 3.2 Build & release

- **Rust:** `cargo publish` per crate; GitHub Releases for prebuilt CLI binaries via `cargo-dist`
- **Python:** `maturin build --release` → wheels for Linux/macOS/Windows × CPython 3.10/3.11/3.12 → PyPI via GitHub Actions
- **TypeScript:** `napi build --release` → prebuilt binaries → npm
- **Docker:** optional multi-arch image (`ghcr.io/<user>/evolve:<version>`)
- **Toolchain:** Rust 2024 edition, latest stable

---

## 4. Components

### 4.1 Core types (`evolve-core`)

#### AgentConfig (universal core + per-tool extensions)

```rust
/// Universal agent configuration that all tools can consume some subset of.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentConfig {
    /// Free-form prefix prepended to the agent's system prompt
    /// (e.g., "You are working on a Rust compiler. Always run `cargo check` after edits.")
    pub system_prompt_prefix: String,
    /// Preferred model (interpreted per-tool)
    pub model_pref: ModelPref,
    /// Behavioral rules (e.g., "always run tests after edits", "ask before deleting files")
    pub behavioral_rules: BTreeSet<String>,
    /// Tools/permissions the agent is allowed to use (interpretation per-tool)
    pub tool_permissions: BTreeSet<String>,
    /// "terse" | "normal" | "verbose"
    pub response_style: ResponseStyle,
    /// Per-tool extensions (opaque blob each adapter understands)
    pub extensions: BTreeMap<String, serde_json::Value>,
}
```

The `AgentConfig` is the *evolved unit*. Each project has a current champion config and
optionally an active challenger config. Both are stored in SQLite.

#### Schema DSL (already built in Task 1.1)

`GenomeSchema`, `FieldSchema`, `StringMutatorKind` are reused as-is. They describe what
fields an `AgentConfig` has, what their types are, and how each is mutated. Validation
(Task 1.2) is preserved.

#### Champion/Challenger types

```rust
pub struct Project {
    pub id: ProjectId,
    pub root_path: PathBuf,
    pub adapter_id: AdapterId,        // "claude-code" | "cursor" | "aider"
    pub created_at: DateTime<Utc>,
}

pub struct Experiment {
    pub id: ExperimentId,
    pub project_id: ProjectId,
    pub champion_config_id: ConfigId,
    pub challenger_config_id: ConfigId,
    pub started_at: DateTime<Utc>,
    pub status: ExperimentStatus,     // Running | ChallengerWon | ChampionHeld | Aborted
    pub challenger_traffic_share: f64, // default 0.05
}

pub struct Session {
    pub id: SessionId,
    pub project_id: ProjectId,
    pub experiment_id: Option<ExperimentId>,
    pub config_id: ConfigId,           // which config was active for this session
    pub variant: Variant,              // Champion | Challenger
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub adapter_metadata: serde_json::Value,  // tool-specific context
}

pub struct Signal {
    pub session_id: SessionId,
    pub kind: SignalKind,              // Implicit { source, ... } | Explicit { user_label }
    pub outcome: f64,                  // 0.0..=1.0; the fitness contribution
    pub recorded_at: DateTime<Utc>,
}
```

#### Bayesian promotion

```rust
pub struct PromotionThreshold {
    pub probability: f64,     // default 0.95
    pub min_sessions_per_arm: u32,  // default 30 — guards against early-noise promotion
}

/// Beta-binomial: each Variant has α=1+wins, β=1+losses; sample posterior, count wins.
pub fn promotion_decision(
    champion_signals: &[Signal],
    challenger_signals: &[Signal],
    threshold: &PromotionThreshold,
) -> Decision {
    // Decision::ChallengerPromote | ChampionHold | NeedMoreData
}
```

#### Mutation operators

- `LlmRewriteMutator` calls `evolve-llm` with prompt: "rewrite the following system prompt
  prefix with one small variation that might improve coding-agent performance" → new
  `system_prompt_prefix` for challenger. One LLM call per challenger generation.
- `ModelPrefMutator` swaps to a neighboring model from a small per-adapter pool.
- `BehavioralRulesMutator` adds, removes, or rephrases one rule from a curated pool.
- `ToolPermissionsMutator` toggles one permission (`+ allow_web_fetch`, `- bash_unrestricted`).
- `ResponseStyleMutator` cycles through `{terse, normal, verbose}`.

Each generation, ONE mutation is applied (single-axis at a time → cleaner attribution).

### 4.2 LLM client (`evolve-llm`)

Minimal: one `LlmClient` trait with `async fn complete(prompt: String) -> Result<String>`.
Two impls: `AnthropicHaikuClient` (reqwest, ~$0.001/call) and `OllamaClient` (async-openai
with custom `base_url`). User picks one in config; default tries Ollama first, falls back
to Anthropic if unavailable AND a key is configured. **If neither is available, mutation
is skipped that cycle** and the next generation reuses the champion.

This crate is intentionally tiny — we only call an LLM ~once per project per week. No
streaming, no tool-calling, no provider matrix.

### 4.3 Storage (`evolve-storage`)

SQLite via `sqlx`. Single file at `~/.evolve/evolve.db`. Schema migrations via `sqlx migrate`.

Tables: `projects`, `agent_configs`, `experiments`, `sessions`, `signals`. All rows
include timestamps. No PII, no source code, no prompts logged by default — only
metadata and fitness scores.

### 4.4 Adapters (`evolve-adapters`)

```rust
#[async_trait]
pub trait Adapter: Send + Sync {
    fn id(&self) -> AdapterId;

    /// Detect whether a directory looks like a project of this kind.
    fn detect(&self, root: &Path) -> AdapterDetection;

    /// Install whatever hooks/configs are needed so the tool reports back to evolve.
    async fn install(&self, root: &Path, config: &AgentConfig) -> Result<()>;

    /// Write a new AgentConfig into the tool's config files.
    async fn apply_config(&self, root: &Path, config: &AgentConfig) -> Result<()>;

    /// Parse a session log (provided by the hook) into Signals.
    async fn parse_session(&self, log: SessionLog) -> Result<Vec<Signal>>;

    /// Optional: serve as an OpenAI-compat proxy for this tool (fallback for tools
    /// whose config doesn't expose enough surface).
    fn proxy_endpoint(&self) -> Option<ProxyEndpoint>;
}
```

#### ClaudeCodeAdapter

- **Config target:** `~/.claude/CLAUDE.md` (sections marked with `<!-- evolve:start -->` /
  `<!-- evolve:end -->` so we never overwrite user content)
- **Hook installation:** writes a `Stop` hook in `~/.claude/settings.json` calling
  `evolve record-claude-code <session-transcript-path>`
- **Implicit signals from session log:**
  - User typed `/clear` early (session abandoned) → 0.0
  - User accepted the work without rerolls → 1.0
  - Tests/lint passed in the post-edit `Bash` calls → 1.0
  - User typed something like "no", "wrong", "redo" mid-session → 0.3
- **Tool extensions:** map of hook configs to apply, list of subagents to enable

#### CursorAdapter

- **Config target:** `.cursorrules` in the project root (markers as above)
- **Hook installation:** Cursor doesn't have a Stop hook. Fallback: a small VS Code
  extension shipped alongside (or `cursor-cli` integration if/when Cursor exposes one).
  For v1, we use the proxy fallback: user points Cursor's "Use custom OpenAI key" feature
  at `localhost:7777` (where `evolve-proxy` runs on demand)
- **Implicit signals:** suggestion accept/reject from the proxy (we see whether subsequent
  user edits keep our generated text or rewrite it within N seconds)

#### AiderAdapter

- **Config target:** `aider.conf.yml` in the project root + `.aider.conf.yml` in `~/`
- **Hook installation:** Aider supports `--commit-prompt` / `--lint-cmd`; we add a
  post-commit git hook that calls `evolve record-aider HEAD`
- **Implicit signals:** lint exit code, test exit code (configured per-project), commit
  reverted within 24h (negative signal)

### 4.5 Proxy (`evolve-proxy`)

Standalone axum server exposing `/v1/chat/completions`. Started on demand by `evolve proxy
--for cursor`. Injects the active AgentConfig's `system_prompt_prefix` into every request,
forwards to the real upstream provider (configured via env vars), and records the
outcome (accepted vs rewritten by user) as a Signal.

### 4.6 Dashboard (`evolve-dashboard`)

axum server bundling a Vite+React SPA via `rust-embed`. Started by `evolve dashboard`.
Default port 8787. Local-only by default (binds 127.0.0.1). Pages:

- **Projects** — list with current champion/challenger
- **Project detail** — champion config (rendered), active challenger (rendered, diffed),
  session timeline, signal history with rolling success-rate
- **Promotion log** — every champion change, why, posterior probabilities at decision time
- **Settings** — LLM provider for mutation, promotion threshold, traffic share

No charts of fitness curves over generations — that framing is gone. The chart that
matters is "this project's success rate over time," which monotonically rises (in
expectation) as champions get promoted.

### 4.7 CLI (`evolve-cli`)

```
evolve init <adapter>           # detect project, install hooks, create initial AgentConfig
evolve record-claude-code <log> # called by Claude Code Stop hook
evolve record-aider <commit>    # called by Aider post-commit hook
evolve record-cursor-event <e>  # called by VS Code extension or proxy
evolve good                     # mark the most recent session as success (override)
evolve bad                      # mark as failure
evolve thumbs <session-id> <±>  # explicit signal for a specific past session
evolve roll                     # force-generate a new challenger now (skip the schedule)
evolve status                   # one-line summary: current champion, active experiment, win-rate
evolve dashboard [--port N]     # start local dashboard
evolve proxy --for <adapter>    # start the OpenAI-compat proxy (Cursor, etc.)
evolve list                     # list known projects
evolve forget <project-id>      # remove a project (delete configs + signals)
```

---

## 5. Data Flow

### 5.1 Install

```
evolve init claude-code
   │
   ▼
detect Claude Code install + project root
   │
   ▼
write initial AgentConfig (defaults from a minimal "good starting point" template)
   │
   ▼
adapter.apply_config() → updates ~/.claude/CLAUDE.md inside <!-- evolve:start --> markers
   │
   ▼
adapter.install() → adds Stop hook to ~/.claude/settings.json
   │
   ▼
SQLite: insert Project, insert AgentConfig, set as champion, no active experiment yet
```

### 5.2 Normal session (champion only, no experiment)

```
User uses Claude Code as normal
   │
   ▼
Session ends → Stop hook fires → calls `evolve record-claude-code <transcript-path>`
   │
   ▼
Adapter parses transcript, emits Signals
   │
   ▼
SQLite: insert Session, insert Signals
   │
   ▼
Check experiment scheduler: enough sessions accumulated? (default: 100 since last champion change OR 7 days)
   │
   ▼ (if yes)
Generate challenger (one LLM call), start new Experiment with traffic_share=0.05
```

### 5.3 Session during active experiment

```
User uses Claude Code as normal
   │
   ▼
Stop hook fires
   │
   ▼
Adapter checks: which variant was active when session started?
   (NOTE: variant is decided at session START, before user's first prompt — adapter
   either applied champion or challenger config based on Bernoulli(traffic_share))
   │
   ▼
Adapter parses transcript, emits Signals tagged with the variant
   │
   ▼
SQLite: insert Session (with variant), insert Signals
   │
   ▼
Bayesian promotion check:
   posterior = beta_binomial_posterior(champion_signals, challenger_signals)
   if P(challenger > champion) > 0.95 AND min_sessions met → promote challenger
   if P(champion > challenger) > 0.95 AND min_sessions met → end experiment, champion holds
   else → continue
```

### 5.4 Variant selection at session start

Tools that support per-session config swapping:
- **Claude Code:** the Stop hook from the previous session also writes which variant to use
  next. Before each session, on `SessionStart` hook, we either keep the current config or
  swap it to challenger (Bernoulli(traffic_share)).
- **Cursor:** via proxy mode, we rotate per-request.
- **Aider:** the post-commit hook decides for the next invocation; we toggle config files.

If a tool can't reliably swap mid-session, the variant choice happens at install time and
runs for a fixed window (e.g., a day).

### 5.5 Explicit override

```
User runs `evolve good` after a session
   │
   ▼
CLI looks up the most recent Session
   │
   ▼
Insert Signal { kind: Explicit { user_label: Good }, outcome: 1.0 }
   │
   ▼
This signal trumps any implicit signals for the same session in fitness calculations
(weighted higher; framework treats explicit signals as ground truth)
```

### 5.6 Mutation (challenger generation)

```
Time to generate a challenger
   │
   ▼
Pick mutation operator:
   - 50% → LlmRewriteMutator (system_prompt_prefix)
   - 15% → BehavioralRulesMutator
   - 15% → ResponseStyleMutator
   - 10% → ModelPrefMutator
   - 10% → ToolPermissionsMutator
   │
   ▼
Apply operator to current champion → challenger AgentConfig
   │
   ▼
SQLite: insert AgentConfig (as challenger), insert Experiment with status=Running
   │
   ▼
Notify user via dashboard ("new challenger active") if dashboard is running
```

---

## 6. Error Handling

### 6.1 Failure modes

| Failure | Action |
|---|---|
| Implicit signal parsing fails (malformed log) | Skip the session, log warn, do NOT block the user's tool |
| Explicit override called with no recent session | Print friendly error: "No recent session — use `evolve thumbs <id>` instead" |
| LLM mutation call fails (provider down, no key) | Skip mutation cycle; reuse champion as challenger; log warn; surface in dashboard |
| Hook installation conflicts with user's existing settings.json | Detect, refuse to overwrite, print one-line fix instructions |
| SQLite db corrupted | Backup to `~/.evolve/evolve.db.bak.<ts>`, recreate empty, log loudly |
| Adapter cannot detect project type | `evolve init <adapter>` requires explicit adapter; suggest options |
| User runs `evolve init` twice in same project | Idempotent: re-validate config, re-install hook only if missing |

### 6.2 Privacy & data hygiene

- No source code is logged. Adapters extract only structured metadata from session
  transcripts (file names mentioned: optional + redactable; outcomes: yes; user prompts:
  optional, default OFF).
- The `~/.evolve/` directory is the universe. Removing it removes everything.
- `evolve forget --all` is a documented one-line nuke.

---

## 7. Testing Strategy

### 7.1 Test layers

| Layer | Description |
|---|---|
| Unit | Operators, Bayesian decision function (proptest invariants), schema validation, signal aggregation |
| Storage | SQLite roundtrips for every table; migration up/down |
| LLM client | Cassette tests via wiremock; smoke test `--ignored` against real Anthropic |
| Adapter conformance | Each adapter: detect, install, apply_config, parse_session against fixture session logs committed in `tests/fixtures/` |
| Promotion logic | Property test: given enough sessions favoring challenger, posterior decision = ChallengerPromote with correct probability bound |
| End-to-end | Spin up a fake "agent" subprocess that emits a known transcript pattern; run `evolve init` + N simulated sessions; assert champion is promoted by session N+M |
| Bindings parity | Same canonical "5 simulated sessions, expect promotion" scenario from Rust + Python + TS, all produce identical promotion log |

### 7.2 Coverage thresholds

- `evolve-core`: ≥80%
- `evolve-storage`: ≥75%
- `evolve-adapters`: ≥70% (per adapter)
- Bindings: presence of parity test sufficient

### 7.3 CI matrix

GitHub Actions:
- Rust: `cargo test --workspace` on Linux/macOS/Windows × stable
- Python: pytest on CPython 3.10/3.11/3.12 × OS matrix
- TypeScript: `node --test` on Node 20/22 × OS matrix
- Adapter fixtures: per-adapter test in own job
- End-to-end: Linux only, runs against a stubbed LLM (no real API in CI)

---

## 8. Cost Model

The single most-asked question this product will face is "what does it cost to run?"
The honest answer:

| Item | Cost |
|---|---|
| Framework idle | $0.00 |
| Per-session signal recording | $0.00 (no LLM calls) |
| Challenger generation (one per project per ~week) | ~$0.001 if Ollama local; ~$0.001-$0.01 on Anthropic Haiku |
| Active experiment (5% traffic share, no extra LLM) | $0.00 |
| Dashboard hosting | $0.00 (local-only) |
| Cloud sync | n/a in v1 (deferred) |
| **Estimated monthly per active project** | **$0.01 - $0.10** |

For users with no API keys, Evolve runs entirely free against local Ollama. For users
without Ollama, Anthropic Haiku is the cheapest sane default.

---

## 9. Out of Scope (v1)

- Multi-machine sync (cloud, team workspaces) — v1.1+
- Active benchmark optimization (the original design) — explicitly removed
- Multi-arm bandit with N>2 challengers — Bayesian binary champion-vs-challenger only
- HTTP/gRPC service mode — no daemon, nothing to serve
- Distributed evaluation, worker mode — passive evolution doesn't need it
- Embedding-based diversity — single-arm at a time, no diversity engine needed
- LangChain/DSPy/Vercel AI adapters — those are framework-as-product, not tool-as-product
- Multi-task agents — each project is its own arena
- Fine-tuning, gradient methods — out of scope by design
- Codex / Continue / Cline / Roo / Copilot Workspace adapters — community contributions
  in v1.1+

---

## 10. Public Roadmap

| Version | Scope |
|---|---|
| v1.0 GA | Everything in this document |
| v1.1 | Codex + Continue + Cline adapters; opt-in cloud sync; multi-project portfolio view |
| v1.2 | Multi-arm bandit (N challengers concurrently); team mode (shared champion across users) |
| v2.0 | Hosted evolve.cloud (managed across-machine sync, billing, SSO) — only if community asks |

---

## 11. Acceptance Criteria for v1.0 GA

1. `cargo test --workspace` passes on Linux/macOS/Windows × stable Rust
2. `pip install evolveai` then `evolve init claude-code` in a Rust project: hooks installed, baseline `CLAUDE.md` snippet written
3. Same for Cursor (.cursorrules) and Aider (aider.conf.yml)
4. After 100 simulated sessions where challenger always succeeds and champion always fails, challenger is promoted
5. Same scenario but with random tied-50/50 outcomes: champion holds, no false promotion
6. `evolve dashboard` serves a working SPA on localhost; shows current project's champion + experiment + signals
7. `evolve good` / `evolve bad` records explicit signals that override conflicting implicit signals
8. `evolve forget <project-id>` removes all SQLite rows AND restores the user's pre-evolve config files
9. Coverage thresholds met: `evolve-core` ≥80%, `evolve-storage` ≥75%
10. Real-world soak test: install on three of my own active projects (one per adapter), let run for two weeks, document what evolved (this is the human acceptance test)

---

## 12. Decision Log (post-pivot brainstorm 2026-04-23)

| # | Question | Choice | Rationale |
|---|---|---|---|
| Pivot | Active vs passive | **Passive drop-in** | User redirective: "drops into [tools] and makes agents evolve over time...product not research...not costing a ridiculous amount" |
| Q1 | Adapters v1 | **Claude Code + Cursor + Aider** | Three loud markets; manageable test surface; Codex/Continue/Cline deferred |
| Q2 | Drop-in mechanism | **Config rewriter primary + proxy fallback** | Cleanest UX where possible; proxy fills the gaps |
| Q3 | Fitness signal | **Implicit + explicit override** | Auto by default; user can correct |
| Q4 | Champion/challenger algorithm | **Bayesian posterior beta-binomial** | Standard industry A/B; faster than fixed-N; robust to variance |
| Q5 | Genome shape | **Universal AgentConfig + per-tool extensions** | Cross-tool insight transfer where possible; honest per-tool quirks |
| Q6 | Process model | **Tool-hook driven, no daemon** | Zero idle cost; runs only when something happens |
| Q7 | Privacy | **Local-only, no telemetry** | Trust signal that converts; pairs naturally with "doesn't cost anything" |

---

*End of design document.*

# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **Phase 0:** Cargo workspace skeleton and `evolve-core` stub crate.
- **Phase 1:** `GenomeSchema` DSL with validation; typed newtype IDs
  (`ProjectId`, `ConfigId`, `ExperimentId`, `SessionId`, `SignalId`, `AdapterId`);
  universal `AgentConfig` with adapter defaults, fingerprint, typed extensions;
  proptest coverage.
- **Phase 2:** `evolve-storage` crate with 5-table SQLite schema (projects,
  agent_configs, experiments, sessions, signals), partial-unique index enforcing
  "one running experiment per project", full repository layer, privacy-guard on
  `signals.payload_json`, restart + cascade integration tests. Coverage 93.76%.
- **Phase 3:** Bayesian promotion math (`aggregate`, `posterior_probability`,
  `promotion_decision`) in `evolve-core::promotion`. Beta(1+wins, 1+losses)
  posteriors + Monte Carlo; default thresholds 20 sessions/arm, 0.95 promote
  threshold, 10 000 MC samples. 3 proptest invariants. Coverage 99.05%.
- **Phase 4:** `evolve-llm` with `AnthropicHaikuClient` (claude-haiku-4-5-20251001)
  and `OllamaClient` (native `/api/chat`), both with 1x retry on 5xx/429.
  `pick_default_client` factory, `CostTracker` with Haiku/Ollama price tables.
- **Phase 5:** `evolve-mutators` with 5 operators (LLM rewrite, behavioral
  rules, response style, model pref, tool permissions) + weighted picker
  (default 50/15/15/10/10).
- **Phase 6:** `evolve-adapters` with `Adapter` trait + registry +
  `ClaudeCodeAdapter` (detect, idempotent install of Stop hook, managed-section
  edit of CLAUDE.md, transcript JSONL parsing with /clear + feedback +
  test-exit-code signals, full forget).
- **Phase 7:** `CursorAdapter` with `.cursorrules` rewriter and proxy-event
  signal parser.
- **Phase 8:** `AiderAdapter` with `aider.conf.yml` rewriter and `post-commit`
  git hook installation.
- **Phase 9:** `evolve-proxy` — axum-based OpenAI-compat proxy that injects
  system prefix and records forwarded events.
- **Phase 10:** `evolve-cli` binary with subcommands: init, record-claude-code,
  record-aider, record-cursor-event, good, bad, status, list, forget.
- **Phase 11:** `evolve-dashboard` — axum + rust-embed single-page HTML
  dashboard with REST endpoints `/api/projects`, `/api/projects/:id`,
  `/api/projects/:id/sessions`.
- **Phase 12:** PyO3 bindings (`bindings/python/`) wrapping `aggregate`,
  `posterior_probability`, `promotion_decision`.
- **Phase 13:** napi-rs bindings (`bindings/typescript/`) with equivalent
  surface.
- **Phase 14:** mdBook at `docs/book/` with 8 chapters (intro, quickstart +
  per-adapter, how-it-works, architecture, cost/privacy, FAQ, contributing)
  and example project READMEs.
- **Phase 15:** GitHub Actions release workflow for prebuilt CLI binaries,
  PyPI wheel build/publish, and a docs.yml for GitHub Pages deployment of
  the mdBook.

## [0.1.0] - TBD

First public release.

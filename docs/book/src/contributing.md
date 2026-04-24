# Contributing

The repo is a standard Cargo workspace. Clone, then:

```bash
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo llvm-cov --workspace --summary-only    # coverage gate
```

## Layout

```
crates/
  evolve-core/         # math, IDs, AgentConfig
  evolve-storage/      # SQLite
  evolve-llm/          # LLM clients (Anthropic + Ollama)
  evolve-mutators/     # mutation operators
  evolve-adapters/     # per-tool integration
  evolve-proxy/        # OpenAI-compat proxy
  evolve-dashboard/    # local web UI
  evolve-cli/          # evolve binary
bindings/
  python/              # PyO3 + maturin
  typescript/          # napi-rs
```

## Plan

The full implementation plan and design decisions live at
`docs/plans/2026-04-23-evolve-validation-implementation.md`.

## Commit message style

Conventional-ish: `feat(scope): short subject` / `fix(...)` / `test(...)`.
See `git log` for examples.

# Architecture

Evolve is a Cargo workspace of seven small crates:

| Crate | Responsibility |
|-------|----------------|
| `evolve-core` | `AgentConfig` types, schema DSL, mutation operators' trait, Bayesian promotion math. Zero I/O. |
| `evolve-storage` | SQLite via sqlx. Migrations, repositories for projects/configs/experiments/sessions/signals. |
| `evolve-llm` | Minimal LLM client (Haiku + Ollama) for occasional challenger generation. |
| `evolve-mutators` | Five mutation operators + weighted picker. |
| `evolve-adapters` | Adapter trait + ClaudeCodeAdapter / CursorAdapter / AiderAdapter. |
| `evolve-proxy` | OpenAI-compat HTTP proxy for Cursor-like tools. |
| `evolve-dashboard` | Local-only axum server serving a tiny HTML SPA + REST API. |
| `evolve-cli` | User-facing binary (`evolve`). |

Python and TypeScript bindings (under `bindings/`) re-export the math engine
via PyO3 and napi-rs.

## Where data lives

- `~/.evolve/evolve.db` — SQLite with everything. Single file, easy to back up
  or delete.
- Adapter-managed files (`CLAUDE.md`, `.cursorrules`, `aider.conf.yml`) —
  only the managed section between markers is ever touched.
- Hooks — `.claude/settings.json` (Stop hook), `.git/hooks/post-commit`.

## Layering

```
 evolve-cli
     |
     +-- evolve-adapters -- evolve-core (AgentConfig)
     |
     +-- evolve-storage -- evolve-core (ids)
     |
     +-- evolve-mutators -- evolve-core + evolve-llm
     |
     +-- evolve-proxy
     |
     +-- evolve-dashboard -- evolve-storage
```

`evolve-core` is the foundation: zero runtime dependencies, safe for
bindings to call synchronously. Everything else builds upward.

# Cost & Privacy

## Cost

Evolve is local-only. The only outbound calls are the occasional mutation
prompts to your configured LLM. At the default cadence (roughly one
challenger per project per week), using Haiku:

- **Per call:** ~500 input + 200 output tokens ≈ $0.000375
- **Per project / month:** ~$0.002

Use Ollama instead of Haiku and the cost is $0. The proxy (Cursor fallback)
forwards to *your* existing OpenAI/Anthropic key — it adds no additional cost.

## Privacy

- **Storage:** a single local SQLite file. No cloud, no telemetry.
- **Signals DB:** only metadata + numeric scores are persisted. No source code
  in the `signals.payload_json` column; the storage layer rejects payloads
  that look code-like (`fn `, `def `, `class `, SQL statements, etc.).
- **Prompts:** not logged unless you pass `--include-prompts` on `evolve init`.
- **Outbound HTTP:** only to the LLM provider you configure (for mutation
  generation). No analytics, no pings.

## Opting out

- `evolve forget <project-id>` removes all Evolve rows *and* restores the
  unmanaged versions of your config files.
- `evolve forget --all` wipes everything.
- Deleting `~/.evolve/` is always safe.

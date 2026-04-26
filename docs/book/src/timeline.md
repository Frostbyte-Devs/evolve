# What to expect after install

The honest timeline. Days assume ~5 Claude Code sessions per day on the same project.

## Day 0 — install

```bash
cargo install evolve-cli
cd ~/projects/my-app
evolve init claude-code
evolve doctor
```

`evolve doctor` should show `[OK]` for everything except `experiment running` (none yet) and `sessions recorded` (0 of 20).

## Days 1-4 — accumulating

Use Claude Code normally. After every session, the Stop hook fires `evolve record-claude-code` automatically. You won't notice it.

`evolve doctor` after a few days shows:

```
[INFO] sessions recorded                12 (8 more before challenger generation)
[INFO] experiment running               none yet
```

This is expected. Evolve refuses to make decisions on too little data — that's a feature, not a bug.

## Day 5 — first challenger

When session count hits 20, the next `evolve record-claude-code` call generates a challenger config and starts an experiment:

```
Recorded session 8e72...
Generated challenger 4f1c... in experiment a93b...
```

From this point forward, the SessionStart hook on Claude Code re-rolls the deployed config per session at 50/50 traffic share. About half your sessions run on the champion, half on the challenger.

## Days 5-12 — A/B testing

`evolve doctor` now shows:

```
[OK]   experiment running               started 2026-04-29T10:14:22Z
```

Open the dashboard:

```bash
evolve dashboard
```

The "Success rate over time" panel shows two stacked bars per day: champion vs. challenger. The longer the bars, the more sessions on that day; the brighter the green, the better the average aggregate score.

After ~40 sessions split between the arms, every new session-end runs the Bayesian decision check. It will say one of:

- `experiment needs more data` (default debug log) — keep going
- `experiment holding at posterior 0.62` — challenger isn't winning yet
- `Promoted challenger 4f1c... (posterior 0.96)` — done, swap

## Day ~12 — first promotion

When the posterior crosses 0.95, the challenger gets promoted to champion. The dashboard's "Promotion log" shows it. Your `CLAUDE.md` managed section is updated to the new champion's prompt prefix.

If the challenger never crosses 0.95, the experiment runs indefinitely until you manually `evolve roll` to try a different mutation, or the success rate diverges enough to fire on its own.

## Cadence going forward

After the first promotion, the cycle repeats: 20 more sessions → another challenger → another A/B → another decision.

In practice: 1-3 promotions per month is realistic for a single active project. The compounding effect is small per generation but adds up.

## When `evolve doctor` shows things wrong

Common patterns:

| What you see | What it means |
|--------------|---------------|
| `[MISS] Stop hook installed` | `evolve init` didn't finish or was reverted. Re-run `evolve init claude-code`. |
| `[WARN] LLM available — no Anthropic key + no Ollama` | Mutator runs without LLM rewrite. Set `ANTHROPIC_API_KEY` or run Ollama for richer mutations. |
| `[INFO] sessions recorded — 0` after using Claude Code | Hook isn't firing. Check `.claude/settings.json` for the Stop hook entry. |
| `[INFO] experiment running — none yet` after 20+ sessions | Either the LLM is unreachable AND the rule-based mutators happened to fail, or `should_evolve` threshold logic differs. Run `evolve roll` to force generation. |

`evolve doctor` is supposed to give you these answers without you needing to ask anyone. If it doesn't, file a bug — that's the tool's job and a doctor that misses things is a defect.

# FAQ

## How long before I see my first promotion?

Default thresholds require at least 20 sessions per arm before any decision,
and generally ~100 combined sessions before a clear winner emerges. If you
use your AI tool ~5 sessions per day, expect your first promotion around day
10-20.

## What if the challenger is obviously worse?

The posterior calculation rejects it. Experiments that go below `P = 0.05`
for a long time are candidates for manual abort (`evolve abort <experiment-id>`).

## Can I force a challenger right now?

Yes: `evolve roll`. This skips the normal schedule and immediately generates
and deploys a challenger with 5% traffic share.

## Does Evolve modify files I didn't ask it to?

No. Every file Evolve writes into is bracketed by `evolve:start` / `evolve:end`
markers and only the content between those markers is owned by Evolve.

## What if I hate the challenger?

`evolve bad` on the most recent session immediately pushes the challenger's
posterior toward zero. After a few `bad` signals the challenger gets held
indefinitely; the experiment can be aborted from the dashboard.

## Why does it take a Haiku API call to generate challengers?

Because prompt variations that sound natural need an LLM. The other four
mutators (rules, response style, model pref, permissions) run locally with
no LLM calls. By default `LlmRewriteMutator` is picked with 50% weight, so
half of all challengers cost zero.

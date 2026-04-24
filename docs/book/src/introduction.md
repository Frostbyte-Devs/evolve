# Introduction

Evolve is a drop-in tool that learns which AI-assistant configuration works
best for **your** codebase, automatically, via champion-vs-challenger A/B tests.

You keep using Claude Code, Cursor, or Aider normally. Every session, Evolve
collects implicit signals (tests pass/fail, you typed `/clear`, you accepted
the diff) and occasionally generates a slightly different configuration to try
against your current champion. When the Bayesian posterior that the challenger
outperforms the champion crosses 95%, Evolve promotes the challenger. Otherwise
nothing changes.

## Why this exists

You already tweak your `CLAUDE.md`, `.cursorrules`, and `aider.conf.yml` by
hand, guessing what works. Evolve turns that guessing into an empirical test
anchored to **your** sessions.

## What it is not

- Not a rewrite of the tool you use. Your Claude Code / Cursor / Aider setup
  keeps running exactly as before.
- Not a cloud service. Everything runs locally in a single SQLite file at
  `~/.evolve/evolve.db`. No telemetry, no outbound calls except the occasional
  mutation prompt (to your configured LLM provider).
- Not an opinionated "best prompt" catalog. Evolve only knows what *your
  sessions* say works.

## Install

```bash
cargo install evolve-cli       # or: pip install evolveai / npm install evolveai
evolve init claude-code         # in your project root
```

That's it. Go back to coding.

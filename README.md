# Evolve

**Drop-in passive A/B evolution for AI coding assistants.** Install Evolve in any repo, keep using Claude Code / Cursor / Aider exactly the way you already do, and over the next few weeks Evolve quietly learns which prompt prefix, which behavioral rules, which model, and which response style work best for that specific codebase - then promotes the winning configuration automatically.

- No workflow change. Your AI tool keeps running exactly as before.
- No cloud. Everything lives in one SQLite file at `~/.evolve/evolve.db`.
- No telemetry. The only outbound HTTP is the occasional challenger-generation prompt to a provider you choose.
- About $0.002 per active project per month (or $0 if you point it at a local Ollama).

**Status:** pre-alpha. All 15 planned implementation phases are complete and 125 tests pass, but not a single session from a real user has been observed yet. Expect breakage.

## Table of contents

1. [The problem this solves](#the-problem-this-solves)
2. [How it works](#how-it-works)
3. [Installation](#installation)
4. [Quickstart](#quickstart)
5. [What gets installed where](#what-gets-installed-where)
6. [Architecture](#architecture)
7. [Cost](#cost)
8. [Privacy](#privacy)
9. [Implications - the honest version](#implications---the-honest-version)
10. [What Evolve is not](#what-evolve-is-not)
11. [FAQ](#faq)
12. [Roadmap](#roadmap)
13. [Contributing](#contributing)
14. [License](#license)

## The problem this solves

Anyone who has used Claude Code, Cursor, or Aider seriously for more than a few weeks ends up doing the same thing: fiddling with the config file. Adding a rule to CLAUDE.md because the agent kept making the same mistake. Changing the response style because the narration got noisy. Swapping the model because Sonnet felt too cautious on this codebase. Removing a rule because it seemed to have stopped helping.

The problem is you are flying blind. You have no way to tell whether any of these tweaks actually helped. The agent felt better? Or the last two sessions were just easier tasks? Or you happened to be in a good mood? You cannot A/B test against yourself, and you cannot remember last month well enough to say whether the new config is outperforming the old one.

Evolve turns that guessing into an empirical test anchored to the sessions you are already having, on the codebase you actually care about. It does not replace your tool, it does not need cloud infrastructure, and it does not require you to change a single thing about how you work. It just watches, and occasionally offers a variation, and keeps the one that demonstrably wins.

## How it works

```
   (session N occurs in your editor as normal)
                  |
                  v
       session hook fires -> Evolve records signals:
         - tests passed / failed (bash exit codes)
         - you typed /clear early (counted negative)
         - "redo" / "thanks" / "wrong" / "perfect" in messages
         - optional: you ran `evolve good` / `evolve bad`
                  |
                  v
       enough sessions accumulated?    (default: 20/arm, 100 total)
         /                  \
       no                   yes
        |                    |
        |           generate ONE challenger config (one cheap LLM call)
        |           start A/B test at 5% traffic share
        |                    |
        v                    v
   keep champion     run champion vs challenger
                              |
                              v
                  every session updates Beta posteriors:
                    champion   ~ Beta(1 + wins, 1 + losses)
                    challenger ~ Beta(1 + wins, 1 + losses)
                              |
                              v
               Monte Carlo estimate P(challenger > champion)
                     /                            \
                   >= 0.95                    < 0.95
                    |                            |
                    v                            v
            PROMOTE challenger              HOLD (keep running)
```

The five mutation operators that can change a champion into a challenger:

| Operator | What it does | Weight |
|----------|--------------|--------|
| `LlmRewrite` | Asks Haiku or Ollama to propose a small variation of the system prompt prefix | 50% |
| `BehavioralRules` | Adds, removes, or swaps one rule from a curated 30-rule pool | 15% |
| `ResponseStyle` | Cycles terse / normal / verbose | 15% |
| `ModelPref` | Swaps to a neighboring model (Sonnet <-> Opus <-> Haiku, etc.) | 10% |
| `ToolPermissions` | Toggles one tool permission on or off | 10% |

Explicit signals (`evolve good` / `evolve bad`) are weighted 5x implicit signals. One explicit grade typically dominates the noise of regex-matched chat feedback.

## Installation

Option 1 - Rust CLI (recommended):

```bash
cargo install evolve-cli
```

Option 2 - Python (matches the same math engine, useful if you want to script evolution from a notebook):

```bash
pip install evolveai
```

Option 3 - Node / TypeScript:

```bash
npm install evolveai
```

All three share the same SQLite database at `~/.evolve/evolve.db`. You can mix and match freely - for example, run the CLI daily but script reports from Python.

## Quickstart

### Claude Code

```bash
cd ~/projects/my-app
evolve init claude-code
```

That is the entire setup. Use Claude Code as normal from here. Every time you finish a session, a `Stop` hook fires and Evolve extracts signals from the transcript.

To manually grade a session:

```bash
evolve good     # marks the most recent session as a win
evolve bad      # ... or a loss
```

### Cursor

```bash
cd ~/projects/my-nextjs-app
evolve init cursor
evolve proxy --for cursor &
```

Then in Cursor's settings, point "Custom OpenAI Base URL" at `http://localhost:7777`. The proxy records `suggestion_accepted` / `suggestion_rejected` based on whether you keep the generated text.

If you do not want to run a proxy, just use `evolve good` / `evolve bad` after each session. Explicit signals alone are enough to drive evolution, just slower.

### Aider

```bash
cd ~/projects/my-python-app
evolve init aider
```

This writes `aider.conf.yml` and installs a git `post-commit` hook that calls `evolve record-aider HEAD` after every commit.

### Watching progress

```bash
evolve status         # one-liner per project
evolve list           # all registered projects
evolve dashboard      # local web UI at http://127.0.0.1:8787
```

## What gets installed where

Evolve is obsessive about not touching anything you did not ask it to touch. Every file it writes into uses bracketed markers so the rest of the file is yours.

**Claude Code:**
- `CLAUDE.md` - managed section bracketed by `<!-- evolve:start -->` / `<!-- evolve:end -->`. Content outside the markers is never touched.
- `.claude/settings.json` - one `Stop` hook entry added to `hooks.Stop[]`. Existing hooks, theme, permissions, anything else in the file is preserved.

**Cursor:**
- `.cursorrules` - managed section bracketed by the same markers.

**Aider:**
- `aider.conf.yml` - managed section bracketed by `# evolve:start` / `# evolve:end` (YAML comments).
- `.git/hooks/post-commit` - a three-line block wrapped in `# evolve:hook-start` / `# evolve:hook-end`. If a post-commit hook already exists, Evolve appends; it never overwrites.

`evolve forget <project-id>` removes every trace - strips the managed sections, removes the hook lines, deletes the SQLite rows for that project. The file ends up byte-identical to what you had before `evolve init` (modulo any edits you made in between).

## Architecture

Cargo workspace of eight crates, all under Apache-2.0:

| Crate | Responsibility |
|-------|----------------|
| `evolve-core` | Math engine: `AgentConfig`, IDs, schema DSL, promotion math. Zero I/O - safe to call synchronously from Python/Node bindings. |
| `evolve-storage` | SQLite via sqlx. Five tables: projects, agent_configs, experiments, sessions, signals. |
| `evolve-llm` | Minimal LLM client for occasional challenger generation. Supports Anthropic Haiku + local Ollama. |
| `evolve-mutators` | Five mutation operators + weighted picker. |
| `evolve-adapters` | Adapter trait + per-tool implementations (Claude Code, Cursor, Aider). |
| `evolve-proxy` | OpenAI-compat HTTP proxy for Cursor-like tools that lack a session hook. |
| `evolve-dashboard` | Local-only axum server serving a static HTML SPA + REST API. |
| `evolve-cli` | The `evolve` binary. |

Bindings (under `bindings/`):

- `bindings/python/` - PyO3 + maturin. Wraps the math engine for Python.
- `bindings/typescript/` - napi-rs. Wraps the math engine for Node.

Full architecture notes live in [`docs/plans/2026-04-23-evolve-validation-design.md`](docs/plans/2026-04-23-evolve-validation-design.md).

## Cost

A challenger generation calls an LLM exactly once. Using Anthropic Haiku at current public pricing ($0.25/M input, $1.25/M output):

- Per call: roughly 500 input + 200 output tokens = $0.000375
- Default cadence: one challenger per project per week
- Monthly cost per active project: about $0.002

Using Ollama instead, the cost is $0.

The proxy (Cursor fallback) forwards to whatever LLM provider you already use - it adds no additional cost beyond what your IDE is already paying for.

Worst case scenario: someone spams `evolve roll` to manually trigger challenger generation a thousand times in a day. That would cost under $1.

## Privacy

- **All data is local.** One SQLite file at `~/.evolve/evolve.db`. Delete it any time.
- **No telemetry.** The only outbound HTTP is the occasional mutation prompt to your configured LLM provider.
- **No source code in the signal database.** The storage layer rejects payloads that look code-like (`fn `, `def `, `class `, SQL statements, etc.) on insert. This is a heuristic, not a proof - size limits will follow.
- **No prompts logged** unless you explicitly opt in with `--include-prompts` on `evolve init`.
- **`evolve forget`** removes all data for a project *and* restores the pre-Evolve versions of your config files.

## Implications - the honest version

Because this is a public project shipped by one person, it is worth being direct about what Evolve does and does not change.

**What will actually happen if this works:**

- People who already tweak their AI tool's config files will stop guessing. They will have an answer to "is this new rule helping." That answer will often be "no, the old config was fine" - which is useful information too.
- Teams that want a shared `CLAUDE.md` can let Evolve pick it empirically instead of arguing in a PR.
- The local SQLite database becomes a real dataset: what prompt patterns correlate with sessions where tests pass, on YOUR codebase. Useful for research, blog posts, internal talks.

**What almost certainly will not happen:**

- Evolve will not discover some magic prompt that makes Claude Code into a senior engineer. The search space is small (five axes) and the signal is noisy. What you will see is small but real improvements - a few percentage points of session success rate over weeks, compounding.
- Evolve will not replace careful prompt engineering for production agents. If you are building a customer-facing LLM product, you want a proper experimentation framework with thousands of evaluations per day. Evolve is for your personal coding setup, where you have 5-20 sessions a day.
- Evolve will not help casual users. If you use Claude Code twice a week, you will never accumulate the 20+ sessions-per-arm that the Bayesian engine needs.

**Risks worth knowing about:**

- Implicit signals are regex-matched. If you say "no, wait, redo the other part" and the agent did exactly what you wanted, Evolve counts that as a negative signal. Use `evolve good` when the regex is wrong.
- The privacy-guard on signal payloads is a string heuristic. Adversarial input (base64-encoded code, for instance) would bypass it. The threat model is accidental leakage, not a determined attacker.
- Cursor's fallback (the proxy) adds latency to every request. If your flow is latency-sensitive, skip the proxy and use `evolve good` / `bad` manually.
- The session hook runs after every Claude Code session. If you run 100 sessions in a day, you will see 100 invocations of `evolve record-claude-code` in your process list. It is fast (under 50ms typically) but it is there.

**If this project dies, what happens:**

Evolve is designed to be forgettable. `evolve forget --all` removes every trace from your machine. The managed sections in your config files strip cleanly. You are not locked in.

## What Evolve is not

- Not a rewrite of Claude Code / Cursor / Aider. Your existing setup keeps working.
- Not a SaaS. There is no hosted version and there will never be one.
- Not an opinionated "best prompt" catalog. Evolve only knows what your sessions say works.
- Not a general-purpose experimentation framework. If you need to run 10,000 parallel trials against GSM8K, use Weights & Biases or similar.
- Not a benchmark runner. The input is your actual daily work, not a test suite.

## FAQ

**How long before I see my first promotion?**
Default thresholds need 20 sessions per arm. At 5 sessions/day, expect day 10-20.

**What if the challenger is obviously worse?**
The Bayesian posterior pushes down. Experiments that sit below P = 0.05 for a long time become candidates for `evolve abort`.

**Can I force a challenger right now?**
Yes: `evolve roll`. Skips the normal schedule.

**Does Evolve see my source code?**
No. Adapters read transcript metadata (exit codes, regex matches on chat messages), never file contents. The storage layer explicitly rejects code-like payloads.

**Why does it need a Haiku API call at all?**
Because prompt variations that sound natural need an LLM. The other four mutators (rules, style, model, permissions) run locally. `LlmRewrite` is picked with 50% weight, so half of all challengers cost zero.

**What if I hate the challenger?**
`evolve bad` on the most recent session pushes the challenger's posterior toward zero. A few `bad` signals and the challenger gets held indefinitely.

**Can I use it with a model other than Claude / GPT / Ollama?**
For the CLI's direct usage, yes - anything the LLM client can reach. For the proxy, it forwards to any OpenAI-compat endpoint. The mutation prompt is provider-agnostic.

## Roadmap

**v0.1.0-rc.1 - name-claim release:** locks `evolve-*` names on crates.io and `evolveai` on PyPI/npm. Same code as current `dev` branch. ETA: when we have `CARGO_REGISTRY_TOKEN` wired up in CI.

**v0.1.0 - first GA:** same surface as RC, but marked stable. Requires two weeks of soak testing on three of my own projects (one per adapter).

**v0.2.0 - first real feature addition:**
- Richer Aider signals: run configured `test-cmd` and `lint-cmd` per commit, emit real signals instead of the current placeholder.
- Subagent signal extraction for Claude Code.
- Dashboard gets champion-vs-challenger diff view and a promotion log.

**v0.3.0 - team features:**
- Share a `.evolve/` directory in git for team-wide experiments.
- Aggregate signals across teammates.

**Unscheduled:**
- VS Code extension (because Cursor's proxy fallback is annoying).
- Neovim integration.
- Windows-native hooks (currently tested but not polished).

## Contributing

```bash
git clone https://github.com/Frostbyte-Devs/evolve
cd evolve
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo llvm-cov --workspace --summary-only   # coverage gate
```

The implementation plan is bite-sized and available at [`docs/plans/2026-04-23-evolve-validation-implementation.md`](docs/plans/2026-04-23-evolve-validation-implementation.md) - 2500+ lines covering every task with exact file paths, TDD order, and commit messages. Good place to start if you want to add a new adapter or mutation operator.

Commit message style: `feat(scope): subject`, `fix(...)`, `test(...)`, etc. See `git log` for examples.

Issues and PRs welcome. The things I most want help with:
- Testing on real projects (any of the three adapters).
- Reports of false-positive signals (regex matching chat messages wrong).
- Additional mutation operators for specific languages or frameworks.

## License

Apache-2.0 - see [LICENSE](LICENSE).

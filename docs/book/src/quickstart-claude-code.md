# Quickstart: Claude Code

```bash
cd ~/projects/my-app
evolve init claude-code
```

That's the whole setup. Confirmation:

```bash
cat .claude/settings.json | jq '.hooks.Stop'
cat CLAUDE.md | grep -A 20 "evolve:start"
```

You should see:

- A `Stop` hook entry calling `evolve record-claude-code <transcript-path>`.
- A managed section in `CLAUDE.md` bracketed by `evolve:start`/`evolve:end`.

Now just use Claude Code. Every time you finish a session, the hook fires and
Evolve captures:

- Whether you typed `/clear` early (counted as negative).
- Whether you hit `cargo test` / `pytest` / `npm test` and the exit code.
- Whether you said "redo"/"wrong"/"perfect"/"thanks" in any message.
- (Optional) Your explicit grade via `evolve good` / `evolve bad`.

## Marking a session as good/bad

If Evolve guessed wrong about a session, override it:

```bash
evolve good    # marks the most recent session as a win
evolve bad     # ... or a loss
```

Explicit signals are weighted **5× implicit** by default. One explicit grade
typically overrides the noise of regex-matched "feedback."

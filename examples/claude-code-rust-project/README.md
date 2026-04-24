# Example: Claude Code on a tiny Rust project

A minimal Rust project pre-configured for `evolve init claude-code`.

## Setup

```bash
cd examples/claude-code-rust-project
evolve init claude-code
```

Now use Claude Code normally for a week. Run `evolve status` to watch
sessions accumulate.

## What you'll see

After ~20 sessions, `evolve status` will show a champion + challenger pair.
After ~100, one will win.

## Expected files written

- `CLAUDE.md` — managed section appended between evolve markers.
- `.claude/settings.json` — Stop hook added (or merged if it existed).

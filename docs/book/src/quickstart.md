# Quickstart

Pick your tool:

- [Claude Code](./quickstart-claude-code.md)
- [Cursor](./quickstart-cursor.md)
- [Aider](./quickstart-aider.md)

## What happens when you run `evolve init`

1. Evolve detects the tool's config files in your project.
2. Installs a session hook so sessions report back on completion.
3. Writes an initial managed section inside your existing config files
   (bracketed by `<!-- evolve:start -->` / `<!-- evolve:end -->` markers —
   nothing outside those markers is ever touched).
4. Records the project + its starting champion config in `~/.evolve/evolve.db`.

Use your tool normally from here. After ~100 sessions (or 7 days, whichever
comes first), Evolve will propose its first challenger.

## Inspecting progress

```bash
evolve status      # one-line per project: last session, current champion
evolve list        # all known projects
evolve dashboard   # opens http://localhost:8787 with a live UI
```

# Quickstart: Aider

```bash
cd ~/projects/my-python-app
evolve init aider
```

This:

1. Writes a managed section into `aider.conf.yml` (created if missing).
2. Installs a git `post-commit` hook that calls `evolve record-aider HEAD`.

Now every `aider` commit fires the hook and records an
`aider_commit_observed` signal. Explicit `evolve good` / `evolve bad` still
applies.

## Per-project test commands

If you want richer implicit signals (test pass/fail per commit), set:

```yaml
# aider.conf.yml
test-cmd: pytest -q
lint-cmd: ruff check .
```

Aider runs these itself; Evolve reads the exit codes from the git hook
context in a future version.

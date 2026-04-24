# Example: Aider on a tiny Python project

## Setup

```bash
cd examples/aider-python-project
evolve init aider
```

This writes `aider.conf.yml` and installs a `.git/hooks/post-commit` hook.

## Expected files written

- `aider.conf.yml` — managed section between evolve markers.
- `.git/hooks/post-commit` — wrapper that calls `evolve record-aider HEAD`.

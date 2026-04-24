# Quickstart: Cursor

```bash
cd ~/projects/my-nextjs-app
evolve init cursor
```

This writes a managed section into `.cursorrules`. Cursor doesn't expose a
session-end hook, so signal capture requires Evolve's proxy:

```bash
evolve proxy --for cursor &    # binds http://localhost:7777
```

Point Cursor's **Settings → Models → Custom OpenAI Base URL** at
`http://localhost:7777`. Evolve now sees every request/response pair and
records a `suggestion_accepted` or `suggestion_rejected` signal based on
whether you keep the generated text.

## Alternative

If you don't want to route through a proxy, just use `evolve good` / `evolve
bad` after each meaningful session. The explicit signals alone are enough to
drive evolution (just slower).

# Example: Cursor on a tiny Next.js project

## Setup

```bash
cd examples/cursor-nextjs-project
evolve init cursor
evolve proxy --for cursor &
```

Then in Cursor's settings, point "Custom OpenAI Base URL" to
`http://localhost:7777`.

## Expected files written

- `.cursorrules` — managed section between evolve markers.

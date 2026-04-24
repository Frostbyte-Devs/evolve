# evolveai (TypeScript bindings)

napi-rs bindings for the Evolve math engine.

## Build

Requires Node 18+ and `@napi-rs/cli`:

```bash
npm install
npm run build         # release build
npm run build:debug   # debug build
```

## Usage

```ts
import { aggregateSignals, promote } from "evolveai";

const score = aggregateSignals([["explicit", 1.0], ["implicit", 0.0]]);
const { outcome, posterior } = promote(
    [0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1],
    [1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1],
    42n,
);
```

## Release

Prebuilt binaries for Linux / macOS / Windows are built in CI (see Phase 15
release workflow). Published to npm as `evolveai`.

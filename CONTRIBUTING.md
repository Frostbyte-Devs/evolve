# Contributing to Evolve

Thanks for your interest. Evolve is a young project; expect breaking changes
until v1.0 GA.

## Development setup

- Rust stable (see `rust-toolchain.toml` for the pinned version)
- `cargo test --workspace` must pass on your branch

## TDD is mandatory

Every PR adding a public function in `evolve-core` MUST add a corresponding
test in the same diff. CI enforces this.

## Coverage thresholds

- `evolve-core`: ≥80%
- `evolve-providers`: ≥75%
- `evolve-storage`: ≥70%

Use `cargo llvm-cov` locally to check before pushing.

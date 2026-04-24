# evolveai (Python bindings)

PyO3 + maturin bindings for the Evolve math engine.

## Build

Requires Python 3.10+ and [maturin](https://github.com/PyO3/maturin):

```bash
pip install maturin
cd bindings/python
maturin develop      # builds + installs into active virtualenv
maturin build        # builds a wheel in target/wheels/
```

## Usage

```python
from evolveai import py_aggregate, py_promotion_decision

score = py_aggregate([("explicit", 1.0), ("implicit", 0.0)])
outcome, posterior, sessions_each = py_promotion_decision(
    champion=[0.0] * 15 + [1.0] * 5,
    challenger=[1.0] * 18 + [0.0] * 2,
    seed=42,
)
```

## Release

Wheels for Linux / macOS / Windows × CPython 3.10/3.11/3.12 are built in CI
(see Phase 15 release workflow). Published to PyPI as `evolveai`.

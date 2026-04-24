# How Evolution Works

## The loop

```
          +-- (session N occurs) --+
          |                        v
          |                   signals recorded
          |                        |
          |                        v
          |              enough sessions/time?
          |              /                  \
          |            no                  yes
          |            |                    |
          |            |               generate challenger
          |            v                    |
          +---- keep champion            A/B test starts
                                             |
                                             v
                                      P(challenger > champion) >= 0.95?
                                        /                          \
                                      yes                           no
                                       |                             |
                                       v                             v
                                 promote challenger              hold / keep running
```

## Sessions → fitness

Each session's signals are collapsed into one fitness score in `[0, 1]`:

- Implicit signals (tests passed, user accepted, no `/clear`) each contribute
  with weight **1**.
- Explicit signals (`evolve good` / `evolve bad`) each contribute with weight
  **5**.
- The weighted mean is clamped to `[0, 1]`.

## Fitness → promotion

Scores are binarized at `0.5` (win / loss) and fed into a Beta-binomial
posterior per arm:

- `Champion ~ Beta(1 + champ_wins, 1 + champ_losses)`
- `Challenger ~ Beta(1 + chall_wins, 1 + chall_losses)`

A 10,000-sample Monte Carlo estimates `P(challenger > champion)`. When that
exceeds `0.95` and both arms have at least 20 sessions, the challenger is
promoted.

Why 0.95 and not a straight A/B frequentist test? Bayesian posteriors let us
peek — we can check after every session without inflating false positives.

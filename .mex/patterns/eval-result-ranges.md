---
name: eval-result-ranges
description: Documenting or updating eval probe score ranges, metric names, and how to read lm-eval or EvalPlus results
triggers:
  - "eval result ranges"
  - "benchmark interpretation"
  - "lm-eval"
  - "evalplus"
  - "pass@1"
edges:
  - target: "context/conventions.md"
    condition: when writing the documentation copy, table wording, or file naming
last_updated: 2026-06-01
---

# Eval Result Ranges Docs

## Context

- Read `src/eval.rs` and `contrib/evals/README.md` first.
- Use saved probe artifacts under `contrib/evals/artifacts/` if you need the exact current metric names or values.
- Treat `evalplus.codegen` output as generation-only; the scored result comes from `evalplus.evaluate`.

## Steps

1. Identify the exact shipped probes and metric names.
2. Confirm the score unit for each probe.
3. Write one doc that explains the shared `0.0` to `1.0` scale and the suite-specific meaning.
4. Call out any probe that is generation-only until a later evaluation step.
5. Keep the doc short and comparison-safe.

## Gotchas

- Do not invent percentage cutoffs; the repo only defines the normalized scale.
- `leaderboard_instruction_following` is reported as `leaderboard_ifeval` inside lm-eval output.
- Math scores are grouped means across subtasks, not a single answer key.
- EvalPlus `pass@k` is a fraction, not a raw count.

## Verify

- Every named probe has an explicit range.
- The doc explains what `0.0` and `1.0` mean.
- The doc distinguishes generation artifacts from scored results.
- The doc warns against cross-suite comparisons.

## Update Scaffold

- [ ] Update `.mex/ROUTER.md` "Current Project State" if the eval docs changed materially
- [ ] Update any `.mex/context/` files that are now out of date
- [ ] Add this pattern to `.mex/patterns/INDEX.md` if it is new
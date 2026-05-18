---
name: ozone-launch-planner-parity
description: Aligning base Ozone fast-launch planning with profiling without accidentally changing profiling-only behavior.
triggers:
  - "planner parity"
  - "fast launch heuristic"
  - "default launch planner"
  - "GGUF topology"
  - "metadata layers"
edges:
  - target: "../context/architecture.md"
    condition: when tracing how planner output flows into the launcher and backend start path
  - target: "../context/conventions.md"
    condition: before changing planner behavior or adding planner regression tests
  - target: "tui-profiling-workflow.md"
    condition: when the parity change affects profiling advisory, confirm, or report screens
last_updated: 2026-05-12
---

# Ozone Launch Planner Parity

## Context

- `src/planner.rs` owns both `plan_launch()` for the normal launcher path and `plan_profiling_launch()` for profiling.
- Parity fixes should usually share topology sourcing and estimation helpers, but should not silently collapse the intentional `profiling_mode` differences unless the task explicitly calls for that.
- The most concrete known gap was fast launch still seeding `total_layers` from the coarse size heuristic while profiling already used `gguf::inspect_model_topology()` when available.

## Steps

1. Start at `src/planner.rs` and locate the first explicit divergence between `plan_launch()` and `plan_profiling_launch()`.
2. If the gap is about topology discovery, extract a shared helper that returns:
   - `total_layers`
   - `layer_source_label`
   - `layer_source_note`
3. Reuse that shared helper in both fast launch and profiling, but keep the existing `profiling_mode` flag unchanged unless you are intentionally changing non-profiling adaptation rules too.
4. Add a regression test next to the existing profiling planner tests for the fast-launch side of the same behavior.
5. Widen validation from the root crate to the full workspace, then finish with the canonical release gate.

## Gotchas

- `src/main.rs` only compiles `gguf.rs` when `profiling-ui` or `sweep` is enabled, so planner parity helpers that call into GGUF inspection need cfg-aware fallback logic to keep reduced-feature builds compiling.
- Do not assume “parity” means identical rationale text or identical hardware adaptation. Profiling still intentionally passes `profiling_mode = true`; fast launch may continue using `false` even after sharing topology inspection.
- `make release-gates` can fail after a real code change because the installed local binaries are stale; if parity is the only failure, `./contrib/sync-local-install.sh --no-build` is the right repair before rerunning the gate.

## Verify

- `cargo test -p ozone --features full metadata_layers --quiet`
- `cargo test -p ozone --features full --quiet`
- `cargo clippy -p ozone --features full --all-targets -- -D warnings`
- `cargo test --workspace --quiet`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `make release-gates`

## Debug

- If a parity change compiles locally but fails in `profiling.rs`, look for stale references to planner-local helper types or labels before assuming the planner math is wrong.
- If only reduced-feature builds fail, inspect cfg boundaries around `gguf` imports and fallback helpers.
- If release gates fail only on install parity, resync installed binaries first and rerun the gate before reopening the planner code.

## Update Scaffold

- [ ] Update `.mex/ROUTER.md` "Current Project State" when fast-launch/planner behavior changes materially
- [ ] Update any stale `.mex/` notes or known-issue bullets that still describe the old planner gap
- [ ] Add this pattern to `.mex/patterns/INDEX.md` if it is new
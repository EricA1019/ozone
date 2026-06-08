---
name: eval-report-viewer
description: Converting eval JSON or JSONL artifacts into markdown reports and surfacing them inside the Bench + Eval TUI
triggers:
  - "eval report"
  - "markdown report"
  - "view report"
  - "JSON to MD"
  - "evalplus"
edges:
  - target: "context/conventions.md"
    condition: when writing Rust report-conversion or TUI viewer code so file paths, screen state, and command output stay consistent
last_updated: 2026-06-01
---

# Eval Report Viewer

## Context

- Start with `src/eval.rs`, `src/eval_report.rs`, and the Bench + Eval UI flow in `src/ui/`.
- Treat lm-eval result JSON and EvalPlus HumanEval codegen JSONL as separate artifact shapes.
- Keep markdown generation pure enough to test without the TUI, then wire the resulting text into a dedicated report screen.

## Steps

1. Detect the artifact type and locate the latest source file under `contrib/evals/artifacts/`.
2. Convert the artifact into markdown with clear headings, source paths, and metric or sample summaries.
3. Write the markdown file next to the source artifact so it can be reopened later.
4. Store the markdown in `App` state and open the Bench + Eval report screen when a run finishes.
5. Add a menu action for reopening the last report from within Ozone.

## Gotchas

- `lm-eval` output is normalized metrics on a `0.0` to `1.0` scale; do not invent thresholds.
- EvalPlus `codegen` output is generation only; the score comes from a later `evalplus.evaluate` pass.
- The report screen should preserve the last generated report even after the user returns to the menu.
- Keep the subprocess workflow from parsing stdout for structure when the source artifact already exists on disk.

## Verify

- `cargo test -p ozone lm_eval_markdown_includes_metric_values --quiet`
- `cargo test -p ozone evalplus_markdown_renders_code_sample --quiet`
- `cargo check -p ozone --features full --quiet`
- Open a report from the Bench + Eval screen and confirm `Esc`/`q` returns to the menu.

## Update Scaffold

- [ ] Update `.mex/ROUTER.md` "Current Project State" if report-viewing behavior changes materially
- [ ] Update any `.mex/context/` files that are now out of date
- [ ] Add this pattern to `.mex/patterns/INDEX.md` if it is new
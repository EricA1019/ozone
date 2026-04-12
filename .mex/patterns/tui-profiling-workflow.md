---
name: tui-profiling-workflow
description: Add or debug the Ozone TUI profiling path that recommends actions, runs bench/sweep/analyze work, and returns review-first report screens.
triggers:
  - "profiling flow"
  - "tui profiling"
  - "profile model"
  - "advisory screen"
  - "failure report"
edges:
  - target: "../context/architecture.md"
    condition: when you need to understand how the TUI, profiling layer, and CLI helpers connect
  - target: "../context/conventions.md"
    condition: before editing Rust modules so new process calls and UI code stay in the right files
last_updated: 2026-04-12
---

# TUI Profiling Workflow

## Context

The profiling path is intentionally separate from the fast launch path.

- `src/profiling.rs` owns recommendation logic, workflow orchestration, path validation, and failure classification.
- `src/ui/mod.rs` owns screen state, keyboard handling, and background task message handling.
- `src/ui/launcher.rs` owns advisory/confirm/running/success/failure renderers.
- `src/bench.rs`, `src/sweep.rs`, and `src/analyze.rs` expose quiet/structured helpers so the TUI does not parse stdout.

## Steps

1. Start in `src/profiling.rs` and decide whether the change belongs in:
   - advisory generation,
   - workflow execution,
   - success/failure report building,
   - or follow-up action selection.
2. If the workflow needs new long-running behavior, add message types there first, then wire them into `src/ui/mod.rs`.
3. Keep launch behavior separate:
   - advisory can recommend launch,
   - success screens can offer launch,
   - but raw launch still flows through `current_plan -> Screen::Confirm`.
4. When adding new profiling actions, update all three places:
   - `ProfilingAction` in `src/profiling.rs`
   - action handling in `src/ui/mod.rs`
   - renderer/menu presentation in `src/ui/launcher.rs`
5. If a profiling helper currently prints directly to stdout/stderr, refactor it behind a quiet helper before using it in the TUI.

## Gotchas

- `bench` and `sweep` clear GPU backends before they run; the TUI must warn before destructive profiling steps.
- Broken `.gguf` symlinks may still appear in the catalog. Validate selected models before starting work and route bad selections into the failure-report screen.
- Do not let the TUI parse printed CLI text. Add structured return/progress types instead.
- Keep the UX review-first: success screens may offer generate/export/launch, but they should not apply those automatically.
- `tokio::sync::mpsc::UnboundedReceiver` lives in `App`, so event draining must happen before drawing/handling keypresses each loop.

## Verify

- [ ] `cargo check`
- [ ] `cargo test`
- [ ] `cargo build --release`
- [ ] Run a smoke pass through `./target/release/ozone`
- [ ] Exercise at least one TUI profiling action that reaches a success or failure report screen
- [ ] Confirm follow-up actions still require explicit confirmation/review

## Debug

- If the TUI screen does not update during profiling, check the `profiling_event_rx` drain loop in `src/ui/mod.rs`.
- If output tears the terminal, look for direct `println!` / `eprintln!` calls in `bench`, `sweep`, or `analyze` paths being used by the TUI.
- If a model reaches the wrong screen, verify `ModelPickerMode` and the `Screen::ModelPicker` Enter branch.
- If retry or export actions do nothing, verify `ProfilingSuccessReport::available_actions()` / `ProfilingFailureReport::available_actions()` and the matching key handler branch.

## Update Scaffold

- [ ] Update `.mex/ROUTER.md` "Current Project State" if what's working/not built has changed
- [ ] Update any `.mex/context/` files that are now out of date
- [ ] If this is a new task type without a pattern, create one in `.mex/patterns/` and add to `INDEX.md`

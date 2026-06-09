# BUG-B7: Two parallel diverging eval code paths

- **Severity:** 🔴 Bug
- **File(s):** `src/eval.rs`
- **Found:** 2026-06-08
- **Status:** Open

## What's Wrong
`run_eval_task()` (registry-based, uses `EVAL_TASKS`) and `run_eval()` (enum-based, uses `EvalPreset`) are separate implementations of the same dispatch. The registry path has correct task names but is `#[allow(dead_code)]` — never called. The enum path runs in production but has wrong task names for TruthfulQA and BBH.

## Evidence
- `run_eval_task()` — `#[allow(dead_code)]` (line 149) — no caller outside its own dead-code dependencies
- `run_eval()` — called by CLI `oz eval --preset <X>` (main.rs:481)
- TUI spawns subprocess that calls CLI which dispatches to `run_eval()`

## Impact
Fixing `EVAL_TASKS` has zero effect on actual execution. Any new task added to the registry can't be run without also touching `EvalPreset`, `run_eval()`, `eval_report.rs`, and the TUI mapping — defeating the purpose of the registry.

## Suggested Fix
Decide on one path: either (a) delete the registry and stick with the enum (simpler, 4 files change per new task), or (b) switch CLI/TUI to dispatch through `run_eval_task()` and retire the enum (registry becomes single source of truth).

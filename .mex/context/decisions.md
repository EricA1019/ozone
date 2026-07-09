---
name: decisions
description: Key architectural and technical decisions with reasoning. Load when making design choices or understanding why something is built a certain way.
triggers:
  - "why do we"
  - "why is it"
  - "decision"
  - "alternative"
  - "we chose"
edges:
  - target: context/architecture.md
    condition: when a decision relates to system structure
  - target: context/stack.md
    condition: when a decision relates to technology choice
last_updated: 2026-07-09
---

# Decisions

## Decision Log

### Eval feature flag for conditional compilation
**Date:** 2026-07-07
**Status:** Active
**Decision:** Eval-related modules (eval.rs, runner.rs, suites.rs, etc.) are gated behind `#[cfg(feature = "eval")]` and excluded from `--no-default-features` (lite) builds.
**Reasoning:** The eval subsystem depends on Python/lm-eval harness, GPU-accelerated calibration, and external task definitions. Lite builds (for distro packages, minimal deployments) don't need eval. Gating with a feature flag keeps the dependency tree small for lite users while keeping eval fully available in default builds.
**Alternatives considered:** Runtime detection (rejected — adds complexity, can't reduce binary size), unconditional compilation (rejected — lite builds would bloat with uneeded deps).
**Consequences:** Adding a new eval module requires adding `#[cfg(feature = "eval")]` to the module declaration and any call sites. The feature is in `default` and `full`. The `database` feature is a dependency of `eval`.

### Use cfg block pattern for conditional code, not inline cfg on match arms
**Date:** 2026-07-08
**Status:** Active
**Decision:** Wrap eval/profiling-specific match arms and `matches!` patterns in `#[cfg(feature = "...")]` blocks with `#[cfg(not(feature = "..."))] { false/unreachable }` fallbacks, rather than placing `#[cfg]` inside macro invocations.
**Reasoning:** The `matches!` macro does not support `#[cfg]` attributes on individual patterns. Inline cfg on match arms works but creates non-exhaustive pattern errors when variants are gated away. The block pattern is explicit, compiles to zero overhead, and avoids clippy warnings.
**Alternatives considered:** Gating enum variants themselves (rejected — breaks match exhaustiveness across all call sites), catch-all `_ => {}` arms (rejected — masks missing arms for legitimate variants).
**Consequences:** Module files that reference gated types need `#[cfg(feature = "...")]` on function definitions, not just on the call site inside the function body.

### App struct decomposition: 73 fields → 36 fields
**Date:** 2026-07-08
**Status:** Active
**Decision:** Consolidated ProfilingState (12 fields) and BenchEvalState (28 fields) into single fields holding their respective structs, reduced App struct from 73 to 36 pub fields.
**Reasoning:** The App struct in ui/mod.rs was a god-struct with deeply nested state for profiling, bench, and eval workflows. Each workflow's state is now encapsulated in its own struct. The remaining 36 fields are directly accessed by render and dispatch code without intermediate indirection.
**Alternatives considered:** Further splitting into smaller state structs (rejected — would add indirection to every render path). Extracting screens entirely (rejected — too large a refactor for current phase).
**Consequences:** Adding new workflow state means adding a field to the relevant sub-struct, not to App itself. Sub-struct fields must be `pub(crate)` for screen/dispatch access.

### CLI handler extraction to commands/ module
**Date:** 2026-07-08
**Status:** Active
**Decision:** Each `Commands::*` dispatch arm now lives in its own file under `src/commands/`, with `cmd_` prefix, and is called via `commands::cmd_*()` from `lib.rs::run()`.
**Reasoning:** The `run()` function's match block had 6 inline handlers ranging from 30 to 120 lines each. Extracting to separate files gives each handler a clear home, makes them unit-testable independently, and keeps `run()` as pure dispatch (~80 lines).
**Alternatives considered:** Keeping handlers inline (rejected — function too long, poor discoverability). Macro-based dispatch (rejected — complexity not justified).
**Consequences:** Adding a new CLI command means: (1) add enum variant, (2) create `cmd_*.rs`, (3) add re-export in `commands/mod.rs`, (4) add dispatch arm in `run()`.

### processes.rs → llamacpp.rs rename
**Date:** 2026-07-09
**Status:** Active
**Decision:** Renamed `src/processes.rs` to `src/llamacpp.rs` to reflect that the module contains exclusively llama.cpp server launch/management code.
**Reasoning:** The original name `processes` was too generic — the module had nothing to do with general process management. A specific name improves discoverability and prevents scope drift.
**Alternatives considered:** Module alias (rejected — adds complexity without clarity improvement). Keeping the old name (rejected — actively misleading for new contributors).
**Consequences:** All `crate::processes::*` references updated to `crate::llamacpp::*`. 22 files touched.

### Eliminate BenchEvalAction eval/registry duplication
**Date:** 2026-07-09
**Status:** Active
**Decision:** Replaced 18 individual `BenchEvalAction::Eval*` enum variants with a single `Eval(&'static str)` variant that carries the task name from the `EVAL_TASKS` registry. Render and dispatch match arms collapse from 18 arms to 1.
**Reasoning:** Each eval variant was a 1:1 duplicate of a `EVAL_TASKS` registry entry. Adding a new eval task required touching 4 files (enum variant, eval_action_for_cli_name match arm, render match arm, dispatch match arm). Now the UI is data-driven from the registry — adding a task only requires the registry entry.
**Alternatives considered:** Generating the enum with a macro (rejected — complexity not justified for what's now a single variant). Keeping the duplication (rejected — violates DRY, slows down development).
**Consequences:** The `Eval` variant's `&'static str` must be a valid key in `EVAL_TASKS`. The `eval_action_for_cli_name` function validates this at runtime by calling `eval::find_task()`.

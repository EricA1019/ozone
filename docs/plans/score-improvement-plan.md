# Score Improvement Plan — Ozone Structural Audit Follow-Up

> Based on audit `docs/audit-2026-07-08.md`. Target: move all metrics
> from current range (4-7) to 7-9 range across the board.
>
> **Principle**: Do the hard structural work first. Cosmetics and docs
> fill the gaps between structural waves.

---

## Current Scores vs Targets

| Metric | Current | Target | Gap |
|---|---|---|---|
| Architecture | 6 | 8 | +2 |
| Code Quality | 7 | 9 | +2 |
| Maintainability | 6 | 8 | +2 |
| Test Coverage | 7 | 8 | +1 |
| Test-First Discipline | 4 | 6 | +2 |
| Documentation | 5 | 7 | +2 |
| UX and UI | 5 | 7 | +2 |
| Developer Experience | 6 | 8 | +2 |

---

## Wave 1 — Monolith Splitting (Architecture +2, Maintainability +1)

**Goal**: Eliminate the remaining god-files. This is the highest-impact
structural work remaining.

**Detailed plan:** `docs/plans/wave1-monolith-splitting.md`

### 1.1 Split `crates/ozone-mcp/src/lib.rs` (2,287L → ~400L)

The existing monolith-refactor-plan.md is stale (referenced pre-split line
numbers). A fresh analysis shows 6 extraction targets:

| New Module | Current Lines in lib.rs | What Moves |
|---|---|---|
| `src/server.rs` | 148-697 | `OzoneMcpServer` struct + impl (tool/journey/sandbox methods) |
| `src/tool_defs.rs` | 1377-1844 | `ToolDefinition` struct + all 19 tool schema entries |
| `src/types.rs` | 1839-1913 | `ToolReply`, `CommandOutput`, `EnvOverrideGuard` |
| `src/arg_helpers.rs` | 1914-1981 | `required_*`, `optional_*`, `command_output_data` |
| `src/persist_helpers.rs` | 1982-2285 | `parse_*_id`, `merge_json_objects`, session/branch/message JSON converters |
| Expand `testing/screen.rs` | 1198-1376 | `PtyVteCaptureConfig`, screenshot capture helpers |
| Expand `testing/journey.rs` | 698-1197 | `capturable_screen_journey_builders()` |

After extraction, `lib.rs` is ~400L (imports, module decls, re-exports,
`run_stdio_server()`, `OzoneMcpServer::new()`, `handle_request()`).

**Score impact**: Architecture +1, Maintainability +1

### 1.2 Extract `src/ui/plan_builder.rs` + Shrink `src/ui/mod.rs` (1,672L → ~700L)

Analysis shows mod.rs is actually 293L imports/module-decls + 378L App
struct + 117L free functions + 880L tests. The tests inflate the line
count but are correctly placed.

**What moves:**
- `src/ui/plan_builder.rs` — `next_screen_after_splash()`, `queue_launch()`,
  `LauncherActionOutcome` enum, `selected_record()` (~20L total)
- Move `run_monitor()` to existing `monitor_flow.rs` (~90L) — it's a
  standalone TUI event loop, not a mod.rs concern

**What stays:**
- Imports + module decls + re-exports (~293L)
- App struct + impl (~378L) — correct home for these
- Tests (~880L) — tests `super::*` so must stay

Result: mod.rs ~700L logic + 880L tests = ~1,580L total but split
responsibility. The App struct is the only real responsibility remaining.

**Score impact**: Architecture +1, Maintainability +1

### 1.3 Extract Inline CLI Handlers from `src/lib.rs` (948L → ~550L)

6 inline dispatch arms remain in `run()`:

| Handler | Lines | Target File | Notes |
|---|---|---|---|
| `Commands::Bench` | 101 | `commands/cmd_bench.rs` | Straightforward extract |
| `Commands::Sweep` | 114 | `commands/cmd_sweep.rs` | Straightforward extract |
| `Commands::ThreadSweep` | 60 | `commands/cmd_thread_sweep.rs` | Straightforward extract |
| `Commands::EvalRun` | 122 | `commands/cmd_eval_run.rs` | **Includes DRY fix**: extract shared `build_eval_run_config()` helper to eliminate duplicated managed/unmanaged branches |
| `Commands::CreativeWrite` | 32 | `commands/cmd_creative_write.rs` | Straightforward extract |
| `Commands::Model` | 8 | `commands/cmd_model.rs` | Trivial delegate |

After extraction: `lib.rs::run()` shrinks from ~250L to ~80L (pure
dispatch). `lib.rs` total drops from 948L to ~550L.

**Score impact**: Maintainability +1

---

## Wave 2 — Code Quality & DRY (Code Quality +1, Maintainability +1)

### 2.1 Eliminate Eval Action / Registry Duplication

**Problem**: 18 `BenchEvalAction` variants in `src/ui/bench_eval.rs`
duplicate the `EVAL_TASKS` registry in `src/eval.rs`. Adding a new task
requires touching 4 files.

**Solution**: Generate `BenchEvalAction` from `EVAL_TASKS` using a macro
or a `lazy_static!` map. Or simpler: replace the enum with a `&'static str`
and look up from the registry. The render and dispatch code already use
`cli_name()` — the enum adds no value.

**Steps**:
1. Replace `BenchEvalAction` enum with a simple `&'static str` or
   `EvalTaskKey` newtype
2. Replace match arms with registry lookups
3. Remove dead rendering branches that have no registry entry
4. Add a test that all `EVAL_TASKS` entries have a rendering path

**Score impact**: Code Quality +1

### 2.2 Remove All `#[allow(dead_code)]` Suppressions (16 total)

Current suppressions by file:

| File | Count | Status |
|---|---|---|
| `src/eval_result.rs` | 1 | `EvalMarkdownReport` fields reserved for Phase 3.3 |
| `src/theme.rs` | 2 | Unused theme colors |
| `src/scorers.rs` | 2 | Unused scorer functions |
| `src/catalog.rs` | 2 | Fields dead in lite builds without profiling-ui |
| `src/gate.rs` | 3 | `GateKeeper` config variants reserved for future |
| `src/suites.rs` | 2 | Reserved suite definitions |
| `src/hardware.rs` | 1 | Unused imports on non-unix |

**Action per suppression**:
1. If used within 1-2 phases → keep with a review-date comment
2. If never going to be wired → delete the dead code
3. If blocked on a feature flag → wrap in `#[cfg(feature = "...")]`
   instead of allowing dead_code

**Score impact**: Code Quality +1

### 2.3 Fix `processes.rs` Name/Scope Drift

The module is named "processes" but contains exclusively llama.cpp
launch/server/management code. Two options:
- Rename to `llamacpp.rs` (accurate, but large git diff)
- Add `mod llama_server` as an alias and re-export

**Recommended**: Rename. Update all `use crate::processes::*` references.
This is mechanical but touches ~15 files.

**Score impact**: Code Quality (naming) +0.5

### 2.4 Extract Shared `resolve_cache_type` Helper

The `resolve_cache_type` closure in `src/lib.rs` (EvalRun dispatch)
and `kv_cache_args()` in `processes.rs` both handle env-var fallback
for KV cache quantization. Extract to `ozone-core` paths module or
a shared `cache.rs` helper.

**Score impact**: Code Quality (DRY) +0.5

---

## Wave 3 — Documentation & Process (Documentation +2, Test-First +1)

### 3.1 Fix `decisions.md` Placeholder Date

The `last_updated: [YYYY-MM-DD]` in `.mex/context/decisions.md` has
never been filled in. Set it to the current date and review whether
recent decisions (eval unification, MCP legacy-tools flag, lite build
fix) are documented.

Also add entries for:
- Eval feature flag and why eval is not in `default`
- App struct decomposition (73→36 fields, why remaining fields stay)
- Lite build fix approach (cfg block pattern vs inline cfg)

**Score impact**: Documentation +0.5

### 2.2 Mark `structural-debt-remediation-plan.md` Complete

The plan's task items (0.2, 1.1, 1.2, etc.) were executed in Waves 1-4
but the plan document was never updated. Add completion status, dates,
and commit hashes to each task line.

**Score impact**: Documentation +0.5

### 3.3 Prune Archive Docs

`docs/archive/ozone-plus/` contains ~600KB of documentation from the
deprecated ozone+ chat era. Options:
- Delete entirely (it's in git history)
- Move to a `docs/archive-index.md` with one-line summaries and git
  commit references
- Archive to an `orphan` branch

**Recommended**: Create `docs/archive/INDEX.md` with summaries and
commit hashes, then delete the individual files.

**Score impact**: Documentation +0.5

### 3.4 Add TUI Snapshot Tests

Use `insta` or manual `TestBackend` snapshot comparisons for each
screen variant. Currently only `tui_characterization.rs` (74 lines)
exists, testing state transitions but not rendering.

Add these screens:
- `Splash` render snapshot
- `Launcher` with one model + without models
- `Monitor` live view
- `Settings` screen
- `BenchEval` action list
- `ProfileAdvisory`/`ProfileConfirm`/`ProfileRunning`
- `Results` viewer with sample data

**Score impact**: Test Coverage +1, Test-First +0.5

### 3.5 Add MCP Crate Integration Tests

Current MCP tests are unit tests only (571 lines in `crates/ozone-mcp/src/tests.rs`).
Add integration tests that:
1. Start the MCP server in-process
2. Send JSON-RPC requests
3. Verify responses for each tool

Requires a `JsonRpcClient` test helper and a `TestContext` that provides
a temp XDG sandbox.

**Score impact**: Test Coverage +0.5

---

## Wave 4 — UX & Developer Experience (UX +2, DevEx +1)

### 4.1 Keyboard Shortcut Hints in Launcher

Currently the launcher shows no key hints. The user must know that
`?` opens the command overlay, `m` opens model picker, etc.

Add a "key hints" footer bar (ratatui `Paragraph` at bottom) that
shows available keys for the current screen. This can live in
`launcher_screens.rs`'s `render_hint_bar()` function (which exists
but may not show all relevant keys).

**Score impact**: UX +1

### 4.2 Eval Action Organization

18 eval actions in a flat list is overwhelming. Group them:
- **Standard Benchmarks**: GSM8K, MATH, HumanEval, MBPP
- **Knowledge**: MMLU, MMLU-Pro, TruthfulQA
- **Reasoning**: BBH, ARC-Challenge, HellaSwag
- **Safety/Ethics**: MMLU-Philosophy, Hendrycks-Ethics, BBH-FormalFallacies

Use ratatui `List` sections with headers.

**Score impact**: UX +0.5

### 4.3 Theme Customization at Runtime

Currently `src/theme.rs` defines colors at compile time. Add a
`theme.toml` config file that overrides colors at runtime, falling
back to the compiled defaults. Parse in `Preferences::load()`.

**Score impact**: UX +0.5

### 4.4 Feature Gate Documentation in Dev Experience

Add a `make feature-matrix` command that prints the current feature
gate status for all modules:
```
ozone crate — feature gates:
  eval:  enabled (62 gates)
  bench: enabled (12 gates)
  profiling-ui: enabled (28 gates)
  ...
```

This helps developers understand which code compiles under which
feature set without guessing.

**Score impact**: Developer Experience +0.5

### 4.5 Add `justfile` or Improve Makefile Targets

Current Makefile has common commands but lacks:
- `make check-all` — check all feature permutations
- `make outdated` — check for outdated dependencies
- `make doc` — build docs with `--document-private-items`

**Score impact**: Developer Experience +0.5

---

## Effort Summary

| Wave | Items | Est. Effort | Score Impact |
|---|---|---|---|
| Wave 1 — Monolith Splitting | 1.1, 1.2, 1.3 | ~25h | Arch +2, Maint +2 |
| Wave 2 — Code Quality & DRY | 2.1, 2.2, 2.3, 2.4 | ~12h | CodeQual +2, Maint +1 |
| Wave 3 — Documentation & Tests | 3.1, 3.2, 3.3, 3.4, 3.5 | ~20h | Doc +2, TestFirst +1, TestCov +1.5 |
| Wave 4 — UX & DevEx | 4.1, 4.2, 4.3, 4.4, 4.5 | ~15h | UX +2, DevEx +1.5 |
| **Total** | **18 items** | **~72h** | **All metrics +2-3** |

---

## Phasing Recommendation

**Do Wave 1 first** — it's the hardest, highest-impact, and unblocks
everything else. Splitting the monoliths makes Waves 2-4 easier because
code is in the right files.

**Do Wave 2 second** — code quality improvements compound with the
new module structure from Wave 1.

**Do Wave 3 third** — tests and docs are easiest after the code has
stopped moving.

**Do Wave 4 last** — UX polish is wasted if the underlying structure
is still being rewritten.

---

## Success Criteria

After all 4 waves:

```
make preflight         # passes — clippy clean, all tests pass
cargo build --no-default-features -p ozone  # zero errors
cargo test --workspace  # 330+ tests (was 304)
grep -r "#\[allow(dead_code)\]" src/ | wc -l  # 0 (was 16)
# All screens have snapshot tests
# decisions.md has all recent decisions documented
# MCP crate has integration tests
# Launcher shows key hints
# Eval actions are grouped by category
```

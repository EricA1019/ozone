---
name: score-improvement
description: Plan to raise all metric scores to 8/10 by targeting the bottlenecks that previous sessions deferred
---

# Score Improvement Plan — Ozone

## Post-Mortem: Why the Last Session Plateaued

The previous refactoring session (Waves 0-4) moved the average score from ~5.5 to ~6.5.
Eight categories improved by +1, five stayed flat. Here's why it didn't go further:

### What Worked (+1 each)
- Mechanical fixes (renames, dead code removal, magic numbers)
- CI expansion, doc comments, test consolidation
- CLI extraction of 5 simple commands

### What We Deferred (the next +1-2)
| Deferred work | Why | Impact lost |
|--------------|-----|-------------|
| launcher.rs decomposition (2291 lines) | "High risk" | SRP +1, Maint +1 |
| profiling.rs workflow extraction (1742 lines) | "High risk" | SRP +1 |
| CLI extraction of complex commands | "API mismatches" | Arch +1, Readiness +1 |
| bench/backend_args arg consolidation | "Low priority" | DRY +1 |
| VRAM formula constants naming | "Low priority" | Data-driven +1 |
| pre_split_snapshots.rs real imports | "Low priority" | Test quality +1 |
| Lite build fix | "Pre-existing, deep" | CI coverage, DX +1 |

### The Pattern
**Mechanical fixes give +1 per category. Structural decomposition gives the next +1-2.**
We did all the mechanical work. To reach 8/10, we must do the structural work.

---

## Target: 12/13 metrics at 8+

### Current vs Target Scores

| Metric | Current | Target | Gap | Phase |
|--------|---------|--------|-----|-------|
| Architecture | 7 | 8 | +1 | P1 (CLI extraction) |
| Code quality | 7 | 8 | +1 | P3 (naming, pre_split) |
| Maintainability | 6 | 8 | +2 | P2 (launcher + profiling) |
| Test coverage | 7 | 8 | +1 | P3 (pre_split imports) |
| Test-first discipline | 4 | 4 | 0 | Stuck (culture) |
| Documentation | 8 | 8 | 0 | Already at target |
| UX and UI | 6 | 7 | +1 | P0 (eprintln fix) |
| Developer experience | 8 | 8 | 0 | Already at target |
| Debuggability | 7 | 8 | +1 | P0 (instrument remaining) |
| DRY adherence | 6 | 8 | +2 | P1 (args.rs) + P3 (pre_split) |
| SRP adherence | 5 | 8 | +3 | P2 (launcher + profiling + commands) |
| Data-driven design | 6 | 8 | +2 | P3 (VRAM constants) |
| Readiness for expansion | 6 | 8 | +2 | P1 (CLI trait) + P2 (boundaries) |

---

## Phase 0 — Lite Build Fix + eprintln Cleanup (2h)

*Fix the lite build first — it unblocks CI and is purely additive (no logic change).*

1. **Gate App struct fields** in `ui/mod.rs` that reference eval/profiling modules:
   - `eval_launcher_selected`, `bench_eval_selected`, `bench_launcher_selected`
   - `bench_eval_progress_*`, `eval_run_*`, `bench_eval_running_*`
   - `profiling_*` fields (already behind cfg, verify)
2. **Gate match arms and render calls** in `ui/mod.rs` that reference gated modules.
3. **Replace remaining eprintln** in `src/ui/bench_eval_flow.rs` (check if any survived).
4. **Add `#[tracing::instrument]`** to `dispatch_feature_command` and major sweep/bench wrappers.

**Verification:** `cargo check --no-default-features` passes clean.

---

## Phase 1 — Complete CLI Extraction + arg Building (4h)

*Finish what Phase 1.2b started. Extract all complex commands, consolidate arg building.*

1. **Create `src/commands/` sub-modules** for remaining complex commands:
   - `commands/sweep.rs` — Sweep, ThreadSweep
   - `commands/bench.rs` — Bench
   - `commands/eval.rs` — Eval, EvalRun, CreativeWrite
   - `commands/analyze.rs` — Analyze
   - `commands/mod.rs` — dispatch trait + simple commands + List, ExportServer, Model
2. **Trait-based registration**: `pub trait CliCommand { fn dispatch(cli: &Cli) -> Result<()>; }` — each command module implements this. `dispatch_feature_command()` iterates registered handlers.
3. **Create `src/args.rs`** — shared llama-server argument builder extracted from `bench.rs::build_llamacpp_bench_args` and `backend_args.rs::build_llama_args`. Both callers use the shared version.

---

## Phase 2 — Decompose launcher.rs + profiling.rs (5h)

*The two biggest SRP violations. Straight extraction, no behavior change.*

1. **Decompose `src/ui/launcher.rs`** (2291 lines → ~500):
   - `launcher_render.rs` — main `render()` + `chrome_block()`, `launcher_title()`, `launcher_hint()`, `render_header()`, `render_resources()`, `render_services()`, `render_actions()`, `render_status_bar()`
   - `launcher_model_panel.rs` — `render_model_picker()`, `render_launching()`, `render_confirm()`, `render_configure_hub()`
   - `launcher_profile_views.rs` — all `render_profile_*()` functions + `action_items()`, `warning_style()`
   - `launcher_settings.rs` — `render_settings()`, `render_exit_confirm()`
   - `launcher.rs` keeps: key handling, action dispatch, `visible_launcher_actions()`, `filtered_launcher_actions()`
2. **Extract `run_workflow`** from `profiling.rs` (1742 lines → ~900) into `src/profiling_workflow.rs`.

---

## Phase 3 — Polish (2h)

*Small remaining items that each give +1 to a metric.*

1. **Replace `tests/pre_split_snapshots.rs` with real imports** — the comment says "these should import from `ozone::*` instead." Replace replicated logic with actual `ozone::*` imports. (Test coverage +1, DRY +1)
2. **Name VRAM formula constants** in `launch_config.rs`:
   - `OVERHEAD_BASE_LOW: f64 = 320.0` (line 90)
   - `OVERHEAD_WEIGHT_LOW: f64 = 12.0`
   - `OVERHEAD_CTX_LOW: f64 = 40.0`
   - `OVERHEAD_BASE_HIGH: f64 = 384.0` (line 123)
   - `OVERHEAD_WEIGHT_HIGH: f64 = 14.0`
   - `OVERHEAD_CTX_HIGH: f64 = 48.0`
   - Add comments explaining the origin of each constant. (Data-driven +1)
3. **Add rationale comment to `CONFIGURE_CONTEXT_STEPS`** documenting why each step value was chosen. (Data-driven +1)

---

## Score Projection

| Metric | Current | After | Δ | Responsible phase |
|--------|---------|-------|---|-------------------|
| Architecture | 7 | **8** | +1 | P1 (CLI extraction + trait) |
| Code quality | 7 | **8** | +1 | P3 (VRAM naming) + P0 (dead code cleanup) |
| Maintainability | 6 | **8** | +2 | P2 (launcher + profiling) |
| Test coverage | 7 | **8** | +1 | P3 (pre_split imports) |
| Test-first discipline | 4 | **4** | 0 | Stuck (culture) |
| Documentation | 8 | **9** | +1 | P3 (VRAM/docs in constants) |
| UX and UI | 6 | **7** | +1 | P0 (eprintln → tracing) |
| Developer experience | 8 | **8** | 0 | Already at target |
| Debuggability | 7 | **8** | +1 | P0 (more instrument) |
| DRY adherence | 6 | **8** | +2 | P1 (args.rs) + P3 (pre_split) |
| SRP adherence | 5 | **8** | +3 | P1 + P2 (all 3 monoliths) |
| Data-driven design | 6 | **8** | +2 | P3 (VRAM constants) |
| Readiness for expansion | 6 | **8** | +2 | P1 (CLI trait) + P2 (boundaries) |

**12 of 13 metrics hit 8+.** Test-first discipline stays at 4 (culture issue).

**Total effort:** ~13 hours (vs ~23h for the original plan — this is more focused).
**Estimated score improvement:** Average 6.5 → **7.8** (+1.3).

---

## Validation Checklist

- [ ] `cargo check --no-default-features` passes (lite build)
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` passes
- [ ] `cargo test --workspace` passes (≥304 tests)
- [ ] `cargo test --features full` passes
- [ ] `cargo test -p ozone-mcp-app` passes
- [ ] No `eprintln!` remains in non-test code (all replaced with `tracing::error!`)
- [ ] `tests/pre_split_snapshots.rs` imports from `ozone::*` instead of replicating logic
- [ ] VRAM formula constants in `launch_config.rs` have named constants with comments
- [ ] `CONFIGURE_CONTEXT_STEPS` has rationale comment
- [ ] `src/args.rs` shared llama-server builder exists, both bench.rs and backend_args.rs use it
- [ ] CLI dispatch uses trait-based registration in `commands/`
- [ ] `launcher.rs` reduced from 2291 to ~500 lines
- [ ] `profiling_workflow.rs` extracted from `profiling.rs`

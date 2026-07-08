# Structural Debt Remediation Plan

> Generated 2026-07-07 from full-project audit. See `docs/plans/project-audit.md`
> for the audit findings this plan addresses.

## Goal

Eliminate the top structural risks in the active RC codebase — dead code, lint
gaps, test drift, and monolith boundaries — without changing user-visible
behavior or breaking the green build.

## Prerequisites

- [ ] `make preflight` passes (green build — confirmed 2026-07-07: 296 tests, 0
      clippy warnings)
- [ ] You have read `.mex/patterns/audit-triage-planning.md` before starting this
      plan

## Scope

### In

- Dead code removal: eliminate `#[allow(dead_code)]` and `#![allow(dead_code)]`
  annotations in the active `ozone` crate (src/)
- Lint hardening: make `deny(clippy::expect_used)` actually catch runtime
  `.expect()` calls across the entire crate
- Test drift: remove the duplicated `ManagedLaunchState` struct from
  `tests/managed_launch_state.rs` by importing the production one
- Feature gating: add an `eval` feature flag so the eval module is not always
  compiled
- `lib.rs` extraction: split `src/main.rs` into `src/lib.rs` + thin `main.rs`
  shim so internal modules become importable by integration tests
- SRP break-up of the top-3 oversized source files (by LOC)
- Workspace: remove `ozone-persist` dependency from `ozone-mcp` and excise
  archived crates from the build tree

### Out

- `ozone-mcp/src/lib.rs` decomposition (existing plan in
  `docs/plans/monolith-refactor-plan.md` covers this separately)
- Archived ozone+ crate decomposition (`crates/ozone-tui`, `crates/ozone-engine`,
  `crates/ozone-inference`, `crates/ozone-memory` — these are slated for
  removal, not refactoring)
- Renaming or redesigning public APIs (pure structural changes only)
- Adding new tests (beyond fixing existing test drift)
- Performance optimization
- UX improvements
- Documentation restructuring (except ROUTER.md state updates)

### Deferred

- Property-based or fuzz testing for the eval pipeline
- Integration test expansion
- Extracting `ozone-core` further into smaller sub-crates
- Converting the Screen enum match-arm pattern to a trait-based dispatch
- Full `ozone-mcp` lib.rs split (has its own existing plan)
- Retiring the archived ozone+ crates entirely (must wait for
  `ozone-persist` dependency removal in ozone-mcp — this plan does that)

---

## Waves

### Wave 0 — Safety Hardening

**Objective**: Eliminate silent correctness risks before touching structure.
Hidden `.expect()` panics in runtime paths and dead code that can mask bugs are
the highest-risk items.

| # | Task | File(s) | Verification |
|---|------|---------|-------------|
| 0.1 | Fix crate-level `deny(clippy::expect_used)` so it catches runtime `.expect()` calls in all modules, not just `main.rs`. Currently `main.rs` has `#![cfg_attr(not(test), deny(clippy::expect_used))]` but calls in `catalog.rs`, `export_server.rs`, `db.rs`, `artifacts.rs`, `gguf.rs`, `eval_report.rs`, and `ui/mod.rs` are not flagged. Investigate whether feature gating shields them or the deny is scoped incorrectly. | `src/main.rs`, all modules with runtime `.expect()` | `cargo clippy --all-targets -- -D warnings` fails if any runtime `.expect()` is present |
| 0.2 | Remove every `#[allow(dead_code)]` and `#![allow(dead_code)]` from `src/`. Either wire up the dead symbols, mark them `#[deprecated]` with a timeline, or delete them. | `src/eval_result.rs`, `src/csv_export.rs`, `src/theme.rs`, `src/eval.rs`, `src/scorers.rs`, `src/catalog.rs`, `src/gate.rs`, `src/suites.rs` | After cleanup, `cargo clippy` reports zero dead-code warnings across `src/` |
| 0.3 | Remove the duplicated `ManagedLaunchState` struct from `tests/managed_launch_state.rs`. Import `processes::ManagedLlamaCppLaunchState` instead. Update any field accesses that differ (e.g., `config_fingerprint`). | `tests/managed_launch_state.rs`, `src/processes.rs` | All launch-state tests pass and use the production struct |
| 0.4 | Remove duplicated Screen-variant string arrays from `tests/tui_characterization.rs`. Extract one canonical array from `src/ui/mod.rs` (or test the real `Screen` enum with a `strum`-style count). | `tests/tui_characterization.rs`, `src/ui/mod.rs` | Tests reference a single source of truth for screen variant names |

**Exit gate**: `make preflight` passes. Zero active `#[allow(dead_code)]` or
`#![allow(dead_code)]` in `src/`. No duplicated production types in test files.

---

### Wave 1 — Contract Alignment

**Objective**: Fix inconsistencies between declared scope and actual build
behavior. The eval module being always-on while `ozonelite` is supposed to be a
minimal build is a contract violation.

| # | Task | File(s) | Verification |
|---|------|---------|-------------|
| 1.1 | Add `eval` feature flag to `Cargo.toml`. Gate all eval modules (`src/eval.rs`, `src/eval_types.rs`, `src/eval_result.rs`, `src/eval_report.rs`, `src/runner.rs`, `src/suites.rs`, `src/gate.rs`, `src/scorers.rs`, `src/artifacts.rs`, `src/calibration.rs`, `src/warmup.rs`, `src/hash.rs`, `src/timeout.rs`, `src/policy.rs`, `src/preflight.rs`, `src/creative_writing.rs`, `src/db.rs`) behind `eval`. Make `full` imply `eval`. Make `default` NOT imply `eval`. | `Cargo.toml`, all eval-supporting modules | `cargo build` (default features) does not compile eval or db code. `cargo build --features full` includes eval. All tests pass in both modes. |
| 1.2 | Update `CONTRIBUTING.md` and `README.md` to document the new `eval` feature flag alongside existing ones. | `CONTRIBUTING.md`, `README.md` | Both docs mention `--features eval` |
| 1.3 | Clean up overlapping RC-scope descriptions. `docs/RC_SCOPE.md` and `README.md` and `CONTRIBUTING.md` all describe in-scope/out-of-scope. Pick one canonical source (recommend `docs/RC_SCOPE.md`) and have the others reference it by URL. | `README.md`, `CONTRIBUTING.md`, `docs/RC_SCOPE.md` | Only `docs/RC_SCOPE.md` has the full scope definition; README and CONTRIBUTING link to it |

**Exit gate**: `cargo build` (default features) skips all eval code. `cargo build
--features full` includes it. Scope descriptions are not duplicated.

---

### Wave 2 — Structural Refactors

**Objective**: Create module boundaries where none exist. The main crate has no
`lib.rs`, which prevents integration tests from importing internal types and
forces hacks like struct duplication in test files.

| # | Task | File(s) | Verification |
|---|------|---------|-------------|
| 2.1 | Extract `src/lib.rs` from `src/main.rs`. Move all `mod` declarations, imports, and non-`main()` code into `lib.rs`. Leave `src/main.rs` as a thin `fn main() -> Result<()>` that calls into `lib.rs`. The `main.rs` keep `#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]` — `lib.rs` gets the same deny if it doesn't already inherit it. | `src/main.rs`, new `src/lib.rs` | `cargo build --workspace` compiles. `cargo test --workspace` passes. Integration tests can `use ozone::Something`. |
| 2.2 | Break up `src/ui/mod.rs` (2,155 lines). Extract the Screen enum + its navigation logic into `src/ui/screen.rs`. Extract the top-level event loop and terminal setup into `src/ui/event_loop.rs`. Leave `mod.rs` as an index that re-exports. | `src/ui/mod.rs`, new `src/ui/screen.rs`, new `src/ui/event_loop.rs` | All UI tests pass. No behavioral change. |
| 2.3 | Break up `src/processes.rs` (963 lines). Extract KV-cache argument helpers into `src/kv_cache.rs`. Extract URL readiness and health-check helpers into `src/health.rs`. Leave managed lifecycle in `src/processes.rs`. | `src/processes.rs`, new `src/kv_cache.rs`, new `src/health.rs` | All process-related tests pass. No behavioral change. |
| 2.4 | Break up `src/eval.rs` (949 lines, post-feature-gating). Separate the enum/mapping surface (`EvalPreset`, `EVAL_TASKS`) from the dispatch/runtime logic that runs eval tasks. | `src/eval.rs` → `src/eval/mod.rs` + `src/eval/tasks.rs` + `src/eval/dispatch.rs` | All eval tests pass. No behavioral change. |
| 2.5 | Break up `src/profiling.rs` (1,828 lines). Separate the action enum + labels from the orchestration logic that starts benchmarks and sweeps. | `src/profiling.rs` → `src/profiling/mod.rs` + `src/profiling/actions.rs` + `src/profiling/orchestrator.rs` | All profiling tests pass. No behavioral change. |

**Exit gate**: `make preflight` passes. No single `src/` file is over 1,000 lines.
Integration tests can import from `ozone::*`.

---

### Wave 3 — Workspace Cleanup

**Objective**: Remove archived crates that still pollute the build. This is
gated on `ozone-mcp` no longer depending on `ozone-persist`.

| # | Task | File(s) | Verification |
|---|------|---------|-------------|
| 3.1 | Audit ozone-mcp's use of `ozone_persist` types. These are needed only by the legacy-archived MCP tools (`session_tool`, `message_tool`, `memory_tool`, `search_tool`, `branch_tool`, `swipe_tool`, `export_tool`, `import_card`). Move the consumed types (likely `SqliteRepository`, `BranchRecord`, `PinnedMemoryView`, `PersistError`) into `ozone-core` or define local equivalents in `ozone-mcp`. | `crates/ozone-mcp/Cargo.toml`, `crates/ozone-mcp/src/lib.rs` | `ozone-mcp` compiles without `ozone-persist` in its dependencies. Legacy MCP tools still work. |
| 3.2 | Remove `crates/ozone-persist` from workspace members and source tree. | `Cargo.toml`, remove `crates/ozone-persist` directory | `cargo build --workspace` does not compile `ozone-persist`. |
| 3.3 | Remove `crates/ozone-engine`, `crates/ozone-inference`, `crates/ozone-memory` from the workspace exclude list and source tree. They are archived and should not exist in the active project. | `Cargo.toml`, remove the 3 crate directories | `cargo build --workspace` compiles only the active members (ozone, ozone-core, ozone-mcp). |
| 3.4 | Update `docs/RC_SCOPE.md`, `CONTRIBUTING.md`, and `.mex/ROUTER.md` to reflect the leaner workspace. | `docs/RC_SCOPE.md`, `CONTRIBUTING.md`, `.mex/ROUTER.md` | All docs describe the current workspace. |

**Exit gate**: `cargo build --workspace` compiles exactly 3 crates (`ozone`,
`ozone-core`, `ozone-mcp`). `make preflight` passes. `cargo test --workspace`
runs tests only for those 3 crates.

---

## Validation Cadence

- After every task in every wave: run `make preflight`. Fix any failures before
  moving to the next task.
- After Wave 0: confirm the crate-level `deny(clippy::expect_used)` actually
  fires on a deliberately introduced `.expect("test")` in production code.
- After Wave 1: build with `--features eval` and without, and verify the binary
  size difference is measurable (should be 1-2 MB without eval).
- After Wave 2: run the full test suite and verify no test file still contains a
  duplicated production struct.
- After Wave 3: `cargo tree` should show only the 3 active crates.

## Done When

1. `make preflight` passes with zero warnings.
2. `cargo clippy` reports zero dead-code warnings across `src/`.
3. No test file in `tests/` defines a struct that exists in `src/` or `crates/`.
4. `cargo build` (default features) excludes all eval/db code.
5. No single `src/` file exceeds 1,000 lines (excluding generated code).
6. `cargo build --workspace` compiles only 3 crates (ozone, ozone-core,
   ozone-mcp).
7. `crate::deny(clippy::expect_used)` catches a deliberately planted test
   `.expect()` in non- gated production code.

---

## Open Questions (Deferred to Spikes)

- **Q**: Does the current `#![cfg_attr(not(test), deny(...))]` in `main.rs`
  actually apply to modules in `src/` that are behind non-default feature gates?
  **Spike**: Create a minimal reproduction and confirm lint scoping rules for
  cfg-gated modules.
- **Q**: Does `ozone-mcp` use `ozone_persist` types in ways that are simple to
  inline, or does it depend on complex persistence logic?
  **Spike**: Trace every `use ozone_persist::*` in `ozone-mcp` and categorize
  each as "trivial to inline" or "needs core extraction".

---

## References

- [Full audit findings](../project-audit.md) — detailed evidence for every item
  above
- [Monolith refactor plan](./monolith-refactor-plan.md) — existing plan for
  `ozone-mcp/src/lib.rs` split (separate from this plan)
- [Audit triage pattern](../../.mex/patterns/audit-triage-planning.md) — the
  pattern this plan follows

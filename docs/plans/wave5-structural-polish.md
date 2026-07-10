# Wave 5 — Structural Polish

**Target**: Move average audit score from 6.75 → 7.5+ by fixing SRP, DRY, and documentation gaps.

**Estimated effort**: 5-6 hours across 6 items, ordered for maximum safety.

---

## 5.1 — Extract inline tests from `ui/mod.rs` (1h)

Move the 880-line `#[cfg(test)] mod tests { ... }` block from `src/ui/mod.rs` to `src/ui/tests.rs`.

**Why this first**: Immediately cuts `ui/mod.rs` from 1,559L → ~680L, hitting the plan's target. Pure move — zero behavioral risk because tests are already passing. Unlocks extracting `App` into its own file.

**Steps**:
1. Create `src/ui/tests.rs`
2. Move the entire `#[cfg(test)] mod tests { ... }` block, adjusting imports as needed
3. Verify: `cargo build --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`

**Verification**: All 315+ tests pass. `ui/mod.rs` ≤ 700 lines.

---

## 5.2 — Extract `App` struct to `src/ui/app.rs` (1.5h)

Move `App` struct, `BenchEvalState`, `ServiceStatus`, and their `impl` blocks to a new `src/ui/app.rs` module. Leave event dispatch and flow glue in `mod.rs`.

**Why**: Without this step, deduplicating `App::new()` in 5.3 would fix duplication inside a god-module without fixing the SRP violation. This is the structural root cause.

**Steps**:
1. Create `src/ui/app.rs` with `App`, `BenchEvalState`, `ServiceStatus`
2. Move all `impl App { ... }` methods that are pure state operations (not dispatch/event handling)
3. Re-export `App` from `src/ui/mod.rs`
4. Update all `use super::App` references across ui submodules
5. Verify: build + clippy + tests pass

**Verification**: `ui/mod.rs` ≤ 400 lines (pure dispatch + module declarations). `App` lives in `app.rs` with clear state-management methods.

---

## 5.3 — Deduplicate `App::new()` (1h)

Merge the two near-identical `App::new()` constructors (lite mode vs full mode, ~110 lines each) into a single constructor with `#[cfg(feature = "profiling-ui")]` on the differing fields.

**Steps**:
1. Identify the exact fields that differ between lite and full constructors (profile-related)
2. Write a test that verifies both feature sets produce correct `App` state
3. Merge into a single `App::new()` with `#[cfg(feature = "profiling-ui")]` guards
4. Verify: build + clippy + tests pass (both default and `--no-default-features`)

**Verification**: One `App::new()` implementation. Zero code duplication.

---

## 5.4 — Quick cleanup batch (1h)

Four small, safe, high-value items batched together:

### 5.4a — README refresh (30m)
- Add documentation for: `theme.toml` runtime customization, keyboard shortcut hints, eval action grouping by category, `make feature-matrix` target, `make doc`, `make check-all`, `make outdated`
- Update archive reference (pruned to 8KB)
- Ensure installation instructions remain accurate

### 5.4b — Remove `database` phantom feature (15m)
- `database = ["dep:rusqlite"]` currently has zero direct `#[cfg(feature = "database")]` gates
- Inline `dep:rusqlite` into `bench` and `eval` features (the only features that actually need rusqlite)
- Remove the `database` feature definition
- Update any references in `Cargo.toml` comments or docs

### 5.4c — Fix `CacheMode` naming inconsistency (10m)
- `CacheMode::Hybrid` → `CacheMode::HybridCache`
- `CacheMode::NoCache` stays (parallel naming)

### 5.4d — Review 13 `dead_code` suppressions (20m)
- Check each `#[allow(dead_code)]` in `src/` — delete if the code is now used, add review/expiry date if still needed
- Target: ≤ 10 remaining suppressions

**Verification**: README is current. `database` feature removed. `CacheMode` naming is consistent. Dead code suppressions reduced.

---

## 5.5 — Centralize hardcoded path strings (1h)

`"results"` and `"contrib/evals"` appear as hardcoded relative path strings in 10+ files across `src/`. Extract them into named constants in `ozone-core::paths`.

**Steps**:
1. Add `pub const RESULTS_DIR: &str = "results";` and `pub const CONTRIB_EVALS_DIR: &str = "contrib/evals";` to `ozone-core::paths`
2. Find all call sites with `\grep -rn '"results"' src/` and `\grep -rn '"contrib/evals"' src/`
3. Replace each literal string with the named constant
4. Verify: build + clippy + tests pass

**Verification**: Zero hardcoded `"results"` or `"contrib/evals"` strings in business logic. All reference named constants.

---

## 5.6 — Fix `make check-all` (2h, *diagnose first*)

`cargo check --workspace --all-features` currently fails with 5 compilation errors in `ozone-mcp` due to feature conflicts (`Uuid`, `SqliteRepository` types).

**Steps**:
1. Investigate the root cause — is it `legacy-tools` pulling `ozone-persist` which conflicts with the crate's own type usage?
2. If it's a quick dependency fix (e.g., conditional import), apply it
3. If it requires restructuring feature flags or dependency graph changes, document the limitation in the Makefile target and defer to a future wave
4. Verify: `make check-all` either passes or the failure is documented with a known-workaround note

**Verification**: `make check-all` either succeeds, or the Makefile target includes a comment explaining the pre-existing limitation.

---

## Deferred (next wave)

These items were evaluated but deferred to keep Wave 5 scoped to easy/medium effort with maximum impact:

| Item | Effort | Reason for deferral |
|---|---|---|
| Replace 7 `too_many_arguments` with config structs | ~4h | Pure refactor, no user-facing impact, can be done independently |
| Break 3 longest functions (397L, 361L, 353L) | ~4h | Higher risk of behavioral regressions; needs test coverage first |
| Archive/purge unused crates | ~1h | Low impact while `legacy-tools` feature still exists; revisit after `legacy-tools` deprecation |
| Unify outcome enums | ~2h | Cosmetic; no user-facing impact |

---

## Effort Summary

| Item | Hours | Risk | Score Impact |
|---|---|---|---|
| 5.1 — Extract inline tests | 1h | None (pure move) | SRP +0.5, Maintainability +0.5 |
| 5.2 — Extract `App` struct | 1.5h | Low | SRP +1.0, Maintainability +0.5 |
| 5.3 — Deduplicate `App::new()` | 1h | Low (with test) | DRY +0.5, Code Quality +0.5 |
| 5.4 — Quick cleanup batch | 1h | Very low | Documentation +0.5, Architecture +0.5 |
| 5.5 — Centralize paths | 1h | Low | Data-Driven +0.5, DRY +0.5 |
| 5.6 — Fix `check-all` | 2h | Medium (diagnose first) | DevEx +0.5 |
| **Total** | **5-6h** | | **~+0.75 avg** |

**Validation gate after each item**: `cargo build --workspace` + `cargo clippy --workspace --all-targets -- -D warnings` + `cargo test --workspace`.

---

## Relevance

- **clippy-instructions.md**: All new code must be clippy-clean (`-D warnings`). No new `#[allow(...)]` suppressions without review date and documented reason.
- **AGENTS.md**: After completing the wave, update `.mex/ROUTER.md` project state and any `.mex/` files that are now out of date. Create patterns for any novel work.

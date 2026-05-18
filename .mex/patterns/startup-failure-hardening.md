---
name: startup-failure-hardening
description: Surface hidden startup and bootstrap failures in base ozone or ozone+ without widening into structural refactors.
triggers:
  - "startup hardening"
  - "hidden failure"
  - "surface loader error"
  - "wave 0"
edges:
  - target: env-isolated-tests.md
    condition: when tests depend on XDG or HOME-resolved paths
  - target: tui-terminal-session-guard.md
    condition: when startup hardening touches base ozone TUI entrypoints or splash/monitor setup
  - target: local-install-sync.md
    condition: when install parity or install-source markers are involved
last_updated: 2026-05-17
---

# Startup Failure Hardening

## Context

- Base ozone startup spans prefs loading, catalog bootstrap, terminal setup, hardware probe, and install-update checks.
- Hidden-failure fixes should preserve first-run defaults while surfacing real corruption, read failures, and trust-boundary violations.
- This pattern is for targeted hardening slices, not monolith decomposition.

## Steps

1. Start from the exact loader or trust boundary that is swallowing failure.
2. Change the low-level helper to return a real `Result` with contextual messages.
3. Preserve silent fallback only for expected first-run states like a missing optional file.
4. Move user-facing fallback decisions to the caller:
   - UI startup should show status or error text when it falls back.
   - CLI paths should print sidecar issues to stderr or return a real error.
5. For partial-success loaders, use a report struct with both successful data and surfaced issues instead of flattening everything to `Ok(default)`.
6. For install-update logic, trust only recorded install metadata; do not discover executable sync roots from ambient cwd.
7. Add focused tests for the new failure behavior before running wider validation.

## Gotchas

- Do not treat a missing first-run prefs file the same as unreadable or invalid JSON.
- Do not let startup hang on splash just because the catalog loaded zero models or returned issues.
- Do not replace a panic with a different hidden fallback; propagate or surface the error.
- Any test that mutates `HOME`, `XDG_DATA_HOME`, or current directory must serialize access.

## Verify

- Corrupt prefs now return an error and startup surfaces it while falling back explicitly.
- Missing or malformed catalog side files generate visible issues instead of a fake clean catalog state.
- Runtime `expect` paths in the touched slice are removed.
- Install update checks require the recorded install-source marker.
- Run focused crate tests first, then `make preflight`, then `cargo check --workspace --all-targets --release`.

## Debug

- If startup still looks clean when a file is broken, search for `unwrap_or_default()` or caller-side ignored `Result`s above the loader.
- If XDG-based tests are flaky, compare the fixture path against `ozone_core::paths::*` resolution inside the sandboxed env.
- If splash never advances after a catalog change, verify readiness tracks catalog completion, not catalog non-emptiness.

## Update Scaffold

- [ ] Update `.mex/ROUTER.md` "Current Project State" if startup behavior or validation status changed
- [ ] Update any `.mex/context/` files that are now out of date
- [ ] Add this pattern to `.mex/patterns/INDEX.md` if it is new

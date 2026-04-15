---
name: tui-launcher-smoke-test
description: Run a repeatable live smoke test of the base Ozone ratatui launcher, especially launch, monitor, settings, profiling, and clear-GPU paths.
triggers:
  - "launcher smoke test"
  - "live ozone session"
  - "test the launcher"
  - "smoke test ozone"
edges:
  - target: "context/architecture.md"
    condition: when you need to remember how launch, profiling, monitor, and preferences connect
  - target: "context/conventions.md"
    condition: before changing code in response to smoke-test findings
last_updated: 2026-04-14
---

# TUI Launcher Smoke Test

## Context

- The root `ozone` TUI lives in `src/ui/mod.rs` and `src/ui/launcher.rs`.
- `Launch` and `Monitor` are the fast-path base-Ozone surfaces.
- Profiling routes through `src/profiling.rs` and should turn invalid models into failure/report screens instead of crashing or starting work.
- PTY captures in automation are noisy; judge behavior by reached screens and state transitions, not by raw ANSI cleanliness.

## Steps

1. Launch the installed binary, not a potentially stale workspace artifact. Prefer `~/.local/bin/ozone --mode base --no-browser`.
2. For base-Ozone smoke tests, prefer `--frontend sillyTavern` so `Launch` stays in the root app instead of handing off into `ozone-plus`.
3. Wait for the launcher header and confirm the saved/overridden backend/frontend line is what you intend to test.
4. Exercise the fast path first:
   - `Launch` with Ollama + SillyTavern should land in `Monitor` when `--no-browser` is active.
   - `r` from `Monitor` should return to the launcher.
5. Exercise `Profile` next:
   - pick one known-good model to confirm advisory/render flow,
   - and, when useful, pick a known-bad/broken-symlink model to verify the issue-report path.
6. Leave `Clear GPU` for last because it is destructive and can change backend availability for the rest of the smoke test.
7. After `Clear GPU`, verify actual service state externally (`ss`, process list, or both) instead of trusting the status line alone.

## Gotchas

- PTY automation may not reliably send every control chord or render every frame cleanly; use arrow/enter-driven paths where possible.
- Broken `.gguf` symlinks still appear in the picker. The correct behavior is an advisory/failure report, not a crash or a blind benchmark attempt.
- `Clear GPU` may not make `:11434` disappear if Ollama is supervised externally or auto-restarts.
- `--no-browser` is important for automated smoke tests so the `Launch` path stays observable in-terminal.

## Verify

- [ ] Launcher header appears with expected backend/frontend
- [ ] `Launch` reaches `Monitor`
- [ ] `Monitor -> r` returns to launcher
- [ ] `Profile` reaches an advisory, success, or failure report screen
- [ ] Broken-model selection produces a reviewable issue path
- [ ] `Clear GPU` reports status and the real listener/process state is checked separately

## Debug

- If `Launch` exits into `ozone-plus` unexpectedly, check saved frontend preference or force `--frontend sillyTavern`.
- If the monitor appears corrupted in capture logs but works interactively, treat ANSI/PTy noise as a tooling limitation first.
- If profiling never leaves the picker, verify the filter text and selected model index.
- If `Clear GPU` claims success but ports remain open, inspect how Ollama is being supervised outside the launcher.

## Update Scaffold

- [ ] Update `.mex/ROUTER.md` "Current Project State" if what's working/not built has changed
- [ ] Update any `.mex/context/` files that are now out of date
- [ ] If this is a new task type without a pattern, create one in `.mex/patterns/` and add to `.mex/patterns/INDEX.md`

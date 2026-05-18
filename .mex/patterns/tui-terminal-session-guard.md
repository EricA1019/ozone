---
name: tui-terminal-session-guard
description: Hardening base Ozone ratatui entrypoints that enter raw mode or the alternate screen, especially monitor-style loops that can leave the terminal looking crashed.
triggers:
  - "alt screen"
  - "raw mode"
  - "terminal crash"
  - "monitor crash"
  - "run_monitor"
edges:
  - target: context/architecture.md
    condition: when you need to confirm which base Ozone entrypoint owns the terminal loop
  - target: context/conventions.md
    condition: when editing `src/ui/mod.rs` or any base Ozone TUI control flow
last_updated: 2026-05-07
---

# TUI Terminal Session Guard

## Context

Start in `src/ui/mod.rs` and identify the exact base Ozone entrypoint that calls `enable_raw_mode()` and `EnterAlternateScreen`.

If the issue is in the monitor path, also inspect `src/hardware.rs` to see whether a fast refresh loop is calling a heavy probe (`nvidia-smi`, `rocm-smi`, `sysinfo::refresh_all`) too often.

## Steps

1. Read the owning entrypoint before editing.
   Usually this is `run_launcher()` or `run_monitor()` in `src/ui/mod.rs`.
2. If cleanup only happens at the bottom of the function, add a small RAII-style guard.
   The guard should restore raw mode, leave the alternate screen, and show the cursor from `Drop` so early `?` returns do not strand the terminal.
3. Arm that guard before the next fallible setup step after `enable_raw_mode()`.
   In practice, mark raw mode immediately after `enable_raw_mode()?` succeeds, then mark alternate-screen entry immediately after `EnterAlternateScreen` succeeds, so failures in `Terminal::new()`, `hide_cursor()`, or similar setup still unwind correctly.
4. Keep an explicit `restore()` method on that guard for normal exits and any `exec()` handoff path.
   Successful `exec()` replaces the process, so cleanup cannot rely on `Drop` alone there.
5. Separate UI tick speed from hardware probe cost.
   If the monitor redraws every 500ms, do not run full hardware discovery on every fast tick. Use a short live cache or a slower hardware-specific interval.
6. Keep the cached startup/planning path and the live monitor path distinct.
   Startup can tolerate a longer cache; monitor mode should be fresher, but not uncached on every frame.

## Gotchas

- Any `?` after `enable_raw_mode()` can leave the terminal in raw mode or the alternate buffer if cleanup only exists on the happy path.
- A guard created after `EnterAlternateScreen`, `Terminal::new()`, or `hide_cursor()` is too late; those setup calls can fail after terminal state has already changed.
- Redirecting monitor output is not enough to make PTY automation safe. The app can still flip terminal state and make the session look crashed.
- Polling vendor GPU CLIs every 500ms is usually unnecessary and can make monitor regressions look like crashes or hangs.
- A shared cache is acceptable if the live path uses a much shorter TTL than the startup/planning path.
- PTY captures can still include unrelated shell noise even when the TUI path is healthy; a successful live render followed by `q` returning exit code `0` is stronger evidence than a perfectly clean capture transcript.

## Verify

- [ ] `src/ui/mod.rs` entrypoints restore terminal state on early error via a guard, not just on normal return
- [ ] Any `exec()` handoff restores the terminal explicitly before process replacement
- [ ] Monitor hardware refreshes no longer run the heaviest probes on every fast tick
- [ ] A real terminal smoke can open `cargo run --quiet -- monitor`, render the live monitor, and return to a normal shell prompt on `q`
- [ ] Touched files are clean in `get_errors`
- [ ] If PTY smoke still looks broken, confirm whether the app actually failed or the harness only got stuck in the alternate screen

## Debug

- If the terminal still looks crashed, search for every `enable_raw_mode()` / `EnterAlternateScreen` pair and verify each path has a guard.
- If monitor data is fresh but performance is poor, inspect `load_hardware_live()` and the monitor fast-refresh branch together.
- If automated smoke enters the alternate buffer again, prefer non-interactive validation (`get_errors`, compile checks, or redirected probes) before assuming the app logic is broken.

## Update Scaffold

- [ ] Update `.mex/ROUTER.md` "Current Project State" if what's working/not built has changed
- [ ] Update any `.mex/context/` files that are now out of date
- [ ] If this is a new task type without a pattern, create one in `.mex/patterns/` and add to `INDEX.md`

---
name: ozone-launcher-normalization
description: Normalize the base Ozone launcher UI around typed actions, shared chrome, and ozone+-aligned settings/navigation behavior before adding quick-command features.
triggers:
  - "launcher normalization"
  - "typed launcher actions"
  - "base ozone settings polish"
  - "launcher chrome"
  - "quick command groundwork"
  - "launcher status copy"
edges:
  - target: "../context/conventions.md"
    condition: before changing launcher state, screen routing, or render helpers in base `ozone`
  - target: "../patterns/launcher-configure-hub.md"
    condition: when the same launcher work also changes Configure Hub tuning, saved profiles, or launch warnings
  - target: "../patterns/tui-launcher-smoke-test.md"
    condition: when finishing the pass and doing live launcher verification
last_updated: 2026-04-21
---

# Ozone Launcher Normalization

## Context

- Keep this phase in base `ozone`: `src/ui/mod.rs` owns launcher state/routing, `src/ui/launcher.rs` owns launcher-facing rendering, `src/ui/monitor.rs` owns the monitor surface, and `src/theme.rs` owns shared style helpers.
- Prefer **typed launcher actions** over numeric slots. `LauncherActionId` plus `LauncherAction` should be the single source of truth for labels, descriptions, and any future `/command` aliasing.
- The goal is a shared visual and interaction baseline, not a shell rewrite. Reuse the existing ratatui launcher architecture and existing ticker/pulse state.

## Steps

1. Replace tuple/slot launcher action rows with typed metadata in `src/ui/mod.rs` and `src/ui/launcher.rs`.
2. Add or reuse launcher helpers such as `visible_launcher_actions()` / `launcher_actions()` so selection, rendering, and routing all derive from the same filtered list.
3. Centralize launcher-facing chrome in `src/ui/launcher.rs`:
   - title helper
   - hint/footer helper
   - shared block/panel styling
   - status-bar fallback copy based on the currently selected action
4. Extend `src/theme.rs` only as much as the shared chrome needs (`muted`, key-hint, panel border/title styles). Do not fork a second theme system for this pass.
5. Refresh `render_settings()` into a clearer menu surface with:
   - focused section treatment
   - selected value visibility
   - an `Active Defaults` summary
   - explicit save/discard navigation copy
6. Normalize model picker, confirm/review, Configure Hub, launching, and monitor surfaces onto the same chrome language where that improves consistency without erasing purposeful screen differences.
7. Reuse existing ticker-driven motion for selected rows or status accents; do not add a new animation subsystem just for this phase.

## Gotchas

- Do not reintroduce hard-coded slot routing in `Screen::Launcher`; match on `LauncherActionId` instead.
- The visible launcher action list changes by tier. Tests should derive positions from action metadata instead of assuming fixed indexes.
- Keep Settings scope stable: this pass should polish the existing backend/frontend/launcher preferences, not add new persisted fields.
- Base `ozone` is bin-only for tests. Use `cargo test -p ozone --tests --quiet`, not `cargo test -p ozone --lib --tests`.
- PTY captures of the launcher may include ANSI noise; use them as a smoke signal, not as a substitute for render tests.

## Verify

- `cargo fmt`
- `cargo test -p ozone --tests --quiet`
- `make preflight`
- live launcher smoke covering:
  - splash -> launcher
  - launcher -> settings
  - launcher -> model picker / review flow when relevant
  - monitor/footer copy consistency

## Debug

- If launcher copy or selection seems inconsistent, inspect `visible_launcher_actions()` and `render_status_bar()` before changing individual screens.
- If Settings feels right visually but saves the wrong values, inspect `open_settings()`, `sync_settings_from_prefs()`, and the `Screen::Settings` key handler before touching render code.
- If a launcher row disappears or routing breaks only in one tier, inspect the visibility filter before changing the action matcher.

## Update Scaffold

- [ ] Update `.mex/ROUTER.md` when the launcher UX baseline changes materially
- [ ] Add or update this pattern when launcher normalization introduces a reusable structure or caution
- [ ] Keep `.mex/patterns/INDEX.md` sorted when adding launcher-related patterns

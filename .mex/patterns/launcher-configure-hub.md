---
name: launcher-configure-hub
description: Add or extend the base Ozone Configure Hub, saved per-model launch profiles, manual overrides, and planner-backed profiling/report surfaces.
triggers:
  - "configure hub"
  - "manual launch tuning"
  - "saved launch profiles"
  - "per-model launch overrides"
  - "context slider"
  - "gpu layers"
  - "cpu offload"
  - "launch warnings"
  - "tokens/sec report"
  - "benchmark saved profile"
edges:
  - target: "../context/conventions.md"
    condition: before changing launcher state, render flows, or preferences wiring
  - target: "../context/architecture.md"
    condition: when deciding whether logic belongs in prefs, planner, or launcher UI state
  - target: "../patterns/llamacpp-backend-integration.md"
    condition: when the Configure Hub change affects llama.cpp launch wiring or saved backend args
  - target: "../patterns/ozoneplus-tui-shell.md"
    condition: when the same sprint also touches ozone+ shell input, render, or runtime adapter behavior
last_updated: 2026-04-21
---

# Launcher Configure Hub

## Context

- The base `ozone` launcher should own manual launch tuning before the confirm/launch step.
- Planner math stays in `src/planner.rs`; persistent override storage stays in `src/prefs.rs`; screen flow and editing state stay in `src/ui/mod.rs`; rendering stays in `src/ui/launcher.rs`.
- Manual tuning should complement profiling, not replace it.

## Steps

1. Add a per-model override structure in `Preferences` and preserve legacy global llama.cpp values as a migration fallback when reasonable.
2. Reuse planner helpers to apply overrides onto a recommended `LaunchPlan`, then recompute derived fields (`cpu_layers`, VRAM/RAM estimates, mode, rationale, warnings).
3. Add a dedicated `Screen::ConfigureHub` between model selection and confirm for the normal launch path only.
4. Keep the Configure Hub editing model discrete and stable in TUI form first:
   - context-size stepping (`4096`, `8192`, `16384`, `24576`, `32768`)
   - GPU-layer adjustment with explicit CPU-resident-layer feedback
5. If the feature grows into reusable configs, keep **saved launch profiles** separate from the transient per-model override:
   - profiles live in prefs as explicit reusable configs
   - transient overrides still cover unsaved Configure Hub tweaks
   - default saved profile should take precedence on model re-entry
6. Profile/report integration should tag benchmark rows back to the selected saved profile instead of inventing a second report store; surface latest/best tok/s, TTFT, VRAM, and RAM next to the selected profile in Configure Hub.
7. Save overrides per model on confirmation, and make launch execution consume the effective customized plan instead of bypassing it with a separate backend-specific override path.
8. If the same work includes ozone+ transcript polish, route message editing through `SessionRuntime` instead of mutating persisted transcript state directly in `ozone-tui`.

## Gotchas

- `LaunchPlan.gpu_layers == -1` means “all GPU” in some older flows; normalize that before comparing or saving per-model overrides so the Configure Hub does not accidentally turn “all GPU” into CPU-only.
- Saved profiles and profiler-generated `profiles` are different things in this repo: saved launch profiles are user-owned reusable configs, while the SQLite `profiles` table is still the auto-generated benchmark frontier/export path.
- Keep profiling/report flows separate: back-navigation from confirm should still return to profiling screens when Configure Hub was not the entry point.
- If Configure Hub can launch profiling for a selected saved profile, make Esc from the confirm/success/failure screens return to Configure Hub and refresh the selected profile's attached report.
- A character-edit stale-transcript bug is usually a layout-routing problem, not just a missing `Clear`: check `is_menu_screen()` before adding render-only workarounds.
- `Ctrl+I` may collide with terminal Tab behavior in some PTYs; keep regression tests around shell-state transitions so the behavior is still provable.

## Verify

- `cargo test -p ozone -p ozone-tui -p ozone-plus --lib --tests`
- `make preflight`
- manual launcher check covering:
  - model picker -> Configure Hub -> confirm -> launch
  - reopen same model and confirm saved override rehydrates
  - save/update/delete/default a saved launch profile and verify it rehydrates on next model-open
  - benchmark a saved profile and verify tok/s report lines appear in Configure Hub
  - warning text changes when pushing context or CPU offload higher

## Debug

- If the launcher shows the right plan but starts the wrong runtime args, inspect the launch execution branch in `src/ui/mod.rs` before changing planner math.
- If saved overrides are ignored on reopen, inspect `Preferences::launch_override_for()` and the Configure Hub entry path from `ModelPicker`.
- If character-edit screens still show transcript content, inspect `crates/ozone-tui/src/layout.rs` and the menu/full-screen routing before touching the render clear path.

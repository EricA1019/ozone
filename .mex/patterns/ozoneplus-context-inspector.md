---
name: ozoneplus-context-inspector
description: Implement or extend the Phase 1E context assembler surface, inspector preview, or dry-run trigger for ozone+.
triggers:
  - "phase 1e"
  - "context plan"
  - "context inspector"
  - "dry-run"
  - "token budget"
edges:
  - target: "context/architecture.md"
    condition: when deciding what belongs in ozone-engine versus apps/ozone-plus versus ozone-tui
  - target: "context/conventions.md"
    condition: before changing shell/runtime boundaries or structured TUI state
  - target: "../ozone+/ozone_v0.4_design.md"
    condition: when checking ContextPlan fields, budget rules, or inspector UX expectations
last_updated: 2026-04-13
---

# Ozone+ Context Inspector

## Context

- Phase 1E is the first explicit, inspectable context pipeline.
- `crates/ozone-engine` owns the context contract and assembler logic.
- `apps/ozone-plus` owns the app/runtime bridge that turns engine context output
  into prompt text plus UI-friendly preview data.
- `crates/ozone-tui` owns the generic inspector/status presentation and key
  routing, but it must not import engine or inference logic directly.

## Steps

1. Add or extend the engine-side `ContextPlan` / `ContextAssembler` types first.
   Keep the first version explicit, data-heavy, and inspectable.
2. Bridge that engine output through `apps/ozone-plus`:
   - preview summary text
   - included / omitted counts
   - token-budget summary
   - dry-run output where available
3. Extend `ozone-tui` with generic context preview models rather than app- or
   engine-specific types.
4. Replace placeholder inspector lines with real preview data from the app
   bridge.
5. Add a user-facing dry-run action through the generic shell/runtime boundary
   (for example `Ctrl+D`) so the dry-run path is actually reachable from the
   live shell.

## Gotchas

- Do not push context-assembly logic into `ozone-tui`; the shell should only
  render structured preview data.
- A status-line string alone is not enough. The inspector needs structured
  included/omitted/budget data to stay useful as the context plan grows.
- PTY automation may not be able to send every control chord reliably. If the
  live channel cannot express something like `Ctrl+D`, cover it with focused
  cargo tests and record that limitation instead of faking a live pass.

## Verify

- `cargo test -p ozone-engine --quiet`
- `cargo test -p ozone-tui --quiet`
- `cargo check -p ozone-plus --quiet`
- `cargo test -p ozone-plus --quiet`
- `cargo clippy -p ozone-plus --all-targets --quiet -- -D warnings`
- temp-XDG live shell pass covering:
  - inline context preview / token-budget text visible during a generation
  - inspector data present when the pane is visible
  - dry-run command path covered either live or by focused tests if PTY control
    chords are unavailable

## Debug

- If the preview string updates but inspector details do not, inspect the TUI
  structured state/hydration path before touching engine code.
- If the app still renders context from a transcript fallback after the engine
  plan exists, inspect the app bridge rather than the shell renderer.
- If `Ctrl+D` or another dry-run command seems inert, inspect key mapping,
  runtime-command routing, and the `SessionRuntime` boundary in that order.

## Update Scaffold

- [ ] Update `.mex/ROUTER.md` "Current Project State" when context preview or dry-run behavior changes materially
- [ ] Update any `.mex/context/` files that still describe ozone+ as lacking a context inspector
- [ ] Add this pattern to `.mex/patterns/INDEX.md` if it is new

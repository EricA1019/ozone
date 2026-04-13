---
name: ozoneplus-streaming-backend-runtime
description: Implement or extend the Phase 1D ozone+ inference crate, app-side adapter, and streamed backend runtime path.
triggers:
  - "phase 1d"
  - "ozone-inference"
  - "koboldcpp"
  - "streaming backend"
  - "inference adapter"
  - "real backend"
edges:
  - target: "context/architecture.md"
    condition: when deciding what belongs in ozone-inference versus apps/ozone-plus versus ozone-tui
  - target: "context/conventions.md"
    condition: before changing runtime wiring, background workers, or app-level integration code
  - target: "../ozone+/ozone_v0.4_design.md"
    condition: when checking Phase 1D acceptance criteria or config/backend expectations
last_updated: 2026-04-13
---

# Ozone+ Streaming Backend Runtime

## Context

- Phase 1D is where ozone+ stops faking assistant turns and starts driving a
  real backend protocol.
- `crates/ozone-tui` must stay backend-agnostic. It owns shell state, polling
  hooks, and render behavior, but not HTTP clients or app-specific persistence.
- `crates/ozone-inference` owns layered config loading, prompt-template
  rendering, backend descriptors, streaming decode, and gateway behavior.
- `apps/ozone-plus` owns the app-side adapter and the runtime bridge that turns
  engine transcripts into backend requests and backend stream events into TUI
  polling results.

## Steps

1. Keep backend-neutral shell behavior in `crates/ozone-tui`; if Phase 1D work
   needs new UI states, expose them as generic runtime progress/failure types
   instead of importing inference logic into the shell crate.
2. Put config/template/backend protocol work in `crates/ozone-inference`:
   - layered TOML config
   - template registry / selection
   - backend descriptors
   - streaming decoder
   - gateway / cancellation / retry behavior
3. Create or extend an app-side adapter in `apps/ozone-plus` so the runtime can
   ask for:
   - resolved config
   - rendered prompt text
   - prepared inference requests
   - backend health / identity information
4. In the app runtime:
   - preserve session advisory locks and draft persistence
   - commit user turns through the existing engine path first
   - build the prompt from the persisted active transcript
   - launch inference work off the TUI thread
   - feed partials back through `poll_generation()`
   - commit the final assistant turn through the engine on completion
   - surface failures and cancellations explicitly
5. Validate both the negative and positive live paths:
   - backend unreachable should surface a clear runtime failure
   - a reachable backend should stream partial tokens and persist the final turn

## Gotchas

- A build-only pass is not enough; the Phase 1D boundary is specifically about
  live streamed runtime behavior.
- The app runtime should never block the TUI event loop waiting for network I/O;
  use a worker boundary and pollable channels.
- Keep config path handling aligned with the project layout:
  - global config via XDG `~/.config/ozone/config.toml`
  - session overrides via `<session_dir>/config.toml`
- If no real KoboldCpp instance is available during development, a
  Kobold-compatible mock server is good enough to verify the streaming control
  flow — but record that distinction in the plan/router instead of pretending it
  was a real-model smoke.

## Verify

- `cargo test -p ozone-tui --quiet`
- `cargo test -p ozone-inference --quiet`
- `cargo check -p ozone-plus --quiet`
- `cargo test -p ozone-plus --quiet`
- `cargo clippy -p ozone-plus --all-targets --quiet -- -D warnings`
- `cargo build -p ozone-plus --quiet`
- temp-XDG live shell pass covering:
  - backend-unreachable failure surfacing
  - streamed partial token updates in the live TUI
  - persisted final assistant turn in the session transcript

## Debug

- If the TUI opens but no partials appear, inspect the app runtime worker and
  poll loop before touching `ozone-tui`.
- If prompt rendering fails before the HTTP request starts, inspect the adapter
  transcript-role mapping and template selection path.
- If the final assistant turn appears in the UI but not the transcript, inspect
  the engine commit path and generation-state transitions separately.
- If live smoke fails only under automation, verify the backend protocol first;
  terminal automation can hide keystroke issues, but it will not fake streamed
  HTTP responses.

## Update Scaffold

- [ ] Update `.mex/ROUTER.md` "Current Project State" when the backend/runtime boundary changes materially
- [ ] Update any `.mex/context/` files that still describe ozone+ as mock-only
- [ ] Add this pattern to `.mex/patterns/INDEX.md` if it is new

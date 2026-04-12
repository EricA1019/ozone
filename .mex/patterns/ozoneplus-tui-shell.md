---
name: ozoneplus-tui-shell
description: Implement or extend the Phase 1C ozone+ TUI shell, `open` integration, draft persistence, and mock-runtime chat loop.
triggers:
  - "phase 1c"
  - "ozone-tui"
  - "chat shell"
  - "draft persistence"
  - "open session"
  - "mock backend"
edges:
  - target: "context/architecture.md"
    condition: when deciding what belongs in ozone-tui versus apps/ozone-plus
  - target: "context/conventions.md"
    condition: before changing shell state, event-loop wiring, or app-level runtime adapters
  - target: "../ozone+/ozone_v0.4_design.md"
    condition: when checking layout thresholds, keybindings, or Phase 1C acceptance criteria
last_updated: 2026-04-12
---

# Ozone+ TUI Shell

## Context

- Phase 1C is the chat-first shell, not the real backend adapter.
- `crates/ozone-tui` should own shell state, key handling, responsive layout,
  render models, and the generic terminal event loop.
- `apps/ozone-plus` should own the app-specific `SessionRuntime` adapter that
  bridges the shell to real persistence and engine writes.
- User turns should already persist through the real engine/persistence path in
  Phase 1C; only assistant generation stays mocked until Phase 1D.

## Steps

1. Extend `crates/ozone-tui` with the shell state, keybindings, and layout/render
   models needed for the current Phase 1C acceptance criteria.
2. Keep the shell backend-agnostic by expressing runtime interactions through the
   `SessionRuntime` boundary instead of importing `ozone-persist` or
   `ozone-engine` into `ozone-tui`.
3. Wire `apps/ozone-plus open <session-id>` into the TUI shell with a local
   runtime adapter that:
   - loads the persisted transcript and active branch
   - holds the session advisory lock for the duration of the live shell
   - persists drafts to `<session_dir>/draft.txt`
   - sends user turns through the real engine
   - returns mock assistant completion/cancel events until Phase 1D
4. Preserve the old Phase 1B session-summary output behind an explicit metadata
   path rather than silently deleting it.
5. Validate with both cargo commands and live temp-XDG TUI runs that exercise
   80x24, 120x40, draft restore, and mock send/cancel behavior.

## Gotchas

- Terminal input over automation channels may not distinguish `Ctrl+I` from plain
  Tab; keep layout and render tests honest so the shell behavior is still proven
  even when live PTY tooling cannot express every chord cleanly.
- If a live TUI process is killed instead of exiting cleanly, advisory session
  locks can remain until the stale-lock timeout. That is expected lock behavior,
  not a shell rendering bug.
- Keep the mock assistant path local to the app runtime adapter. Do not leak
  mock-generation assumptions into `ozone-engine` or `ozone-persist`.

## Verify

- `cargo test --workspace --all-targets`
- `cargo check --workspace --all-targets --release`
- `cargo clippy --workspace --all-targets --release -- -D warnings`
- `cargo build --workspace --release`
- temp-XDG live shell pass covering:
  - `ozone-plus open <session-id>` at 80x24
  - draft text persisted to `draft.txt` and restored on reopen
  - a real user turn committed through the engine
  - mock assistant cancel path exercised with `Ctrl+C`
  - `ozone-plus open <session-id>` at 120x40

## Debug

- If the TUI opens but transcript changes do not persist, inspect the
  `SessionRuntime` adapter in `apps/ozone-plus` before changing `ozone-tui`.
- If cancel/send behavior looks right in UI state but not in storage, verify the
  engine commit path and generation-state updates separately.
- If reopen does not restore drafts, inspect both the app-level draft file path
  and the shell bootstrap path before changing input handling.

## Update Scaffold

- [ ] Update `.mex/ROUTER.md` "Current Project State" if the shell/app boundary changed materially
- [ ] Update any `.mex/context/` files that are now out of date
- [ ] Add this pattern to `.mex/patterns/INDEX.md` if it is new

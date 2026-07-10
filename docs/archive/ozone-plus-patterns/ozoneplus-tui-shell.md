---
name: ozoneplus-tui-shell
description: Implement or extend the ozone+ TUI shell, `open` integration, draft persistence, and runtime-backed chat loop.
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
last_updated: 2026-05-08
---

# Ozone+ TUI Shell

## Context

- This pattern now covers the shipped chat-first shell plus runtime-backed chat-loop polish, not just the original Phase 1C bootstrap.
- `crates/ozone-tui` should own shell state, key handling, responsive layout,
  render models, and the generic terminal event loop.
- `apps/ozone-plus` should own the app-specific `SessionRuntime` adapter that
  bridges the shell to real persistence and engine writes.
- User turns and runtime events should already flow through the real
  app-runtime adapter. Do not reintroduce mock-only assumptions into current
  chat-loop work.

## Steps

1. Extend `crates/ozone-tui` with the shell state, keybindings, and
  layout/render models needed for the current chat-loop acceptance criteria.
2. Keep the shell backend-agnostic by expressing runtime interactions through the
   `SessionRuntime` boundary instead of importing `ozone-persist` or
   `ozone-engine` into `ozone-tui`.
3. Wire `apps/ozone-plus open <session-id>` into the TUI shell with a local
   runtime adapter that loads the persisted transcript and active branch, holds
   the session advisory lock for the duration of the live shell, persists
   drafts to `<session_dir>/draft.txt`, sends user turns through the real
   engine, and preserves runtime refresh/failure/cancel semantics in the app
   adapter without duplicating chat-loop state logic inside `ozone-tui`.
4. Preserve the old Phase 1B session-summary output behind an explicit metadata
   path rather than silently deleting it.
5. Validate with both cargo commands and live temp-XDG TUI runs that exercise
  80x24, 120x40, draft restore, and the current runtime-backed chat loop.

## Gotchas

- Terminal input over automation channels may not distinguish `Ctrl+I` from plain
  Tab; keep layout and render tests honest so the shell behavior is still proven
  even when live PTY tooling cannot express every chord cleanly.
- If the runtime returns `RuntimeCancellation.partial_assistant_message`, push it
  into the transcript as a non-persisted assistant row; do not treat cancel as a
  pure state flip or the user loses visible partial output.
- If `RuntimeFailure` includes the submitted prompt, restore that prompt into the
  composer and return focus to the draft so retry is immediate instead of
  forcing a retype.
- `RuntimeContextRefresh` is a real state delta, not just a status string:
  apply the provided title/transcript/metadata/context/recall fields and keep
  transcript selection stable by matching the previously selected `message_id`
  when possible.
- `RuntimePhase::Generating` with no streamed text yet still needs a transient
  assistant placeholder in the render model so the chat does not look frozen
  before the first token arrives.
- `AppBootstrap.screen` must be explicit when a flow should land in the live
  chat shell. Leaving it as `None` can fall back to the default main-menu
  screen even when `ozone-plus open <session-id>` resolved the right session.
- `TranscriptItem.author` is presentation text, not behavior-safe role data.
  Assistant-only actions like reroll must key off `author_kind`, or backend
  display labels like `koboldcpp backend` will be rejected as non-assistant
  messages.
- If a chat-surface polish only needs to reveal which character/persona is
  active, prefer `session_metadata.character_name` in the render model before
  adding a new runtime command or overlay fetch. That metadata is already part
  of the normal session bootstrap/refresh path.
- `:memories`, `/memories`, and `/memory list` already come back through
  `RuntimeContextRefresh.recall_browser`; open `ScreenState::MemoriesOverlay`
  locally in `ozone-tui` and render from `state.recall_browser` rather than
  waiting for a separate `session_metadata.memory_metadata` path that may not be
  populated.
- `?` should now toggle Help only in normal mode; if it stops inserting in
  insert/command mode again, start in `crates/ozone-tui/src/input.rs`.
- If a live TUI process is killed instead of exiting cleanly, advisory session
  locks can remain until the stale-lock timeout. That is expected lock behavior,
  not a shell rendering bug.
- Keep the mock assistant path local to the app runtime adapter. Do not leak
  mock-generation assumptions into `ozone-engine` or `ozone-persist`.
- PTY smoke in this environment can accept navigation/control keys while still
  dropping plain text entry. If live text injection looks flaky, seed
  `<session_dir>/draft.txt` and submit from the restored draft before assuming
  the shell's send/cancel path is broken.

## Verify

- `cargo test --workspace --all-targets`
- `cargo check --workspace --all-targets --release`
- `cargo clippy --workspace --all-targets --release -- -D warnings`
- `cargo build --workspace --release`
- temp-XDG live shell pass covering:
  - `ozone-plus open <session-id>` at 80x24
  - draft text persisted to `draft.txt` and restored on reopen
  - a real user turn committed through the engine
  - real send/fail/retry/cancel/reroll behavior against the current runtime path
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

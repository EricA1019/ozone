---
name: ozoneplus-message-editor
description: Tighten ozone+ transcript-message editing, textarea synchronization, and composer/transcript UX without changing the engine persistence contract.
triggers:
  - "message editing"
  - "tui-textarea"
  - "edit selected message"
  - "composer clash"
  - "draft sync"
  - "transcript edit"
  - "composer scrollbar"
edges:
  - target: "../context/conventions.md"
    condition: before changing shell state, key routing, or render behavior in `crates/ozone-tui`
  - target: "../patterns/ozoneplus-tui-shell.md"
    condition: when the work also touches broader shell/runtime boundaries or `open` session behavior
  - target: "../ozone+/ozone_v0.4_design.md"
    condition: when checking conversation-screen keybindings or edit/composer UX expectations
last_updated: 2026-04-21
---

# Ozone+ Message Editor

## Context

- Persisted transcript-message editing is a shell concern first: `crates/ozone-tui` owns the editor state, key routing, and render polish; `apps/ozone-plus` should keep owning the actual edit persistence call through `SessionRuntime`.
- The safest mental model is **dedicated edit surface**: once a transcript row enters edit mode, normal draft-only affordances like history browsing and slash autocomplete stop applying until save or cancel.
- `DraftState` is still the persistable snapshot, but the live typing surface should be driven by `tui-textarea`; do not let a second insert path drift independently.

## Steps

1. Start in `crates/ozone-tui/src/app.rs` and identify every path that mutates `draft.text` or `draft.cursor`.
2. Add or reuse explicit helpers so textarea → draft sync and draft → textarea sync each happen in one place, including multiline cursor offsets and trailing newline handling.
3. Route persisted-message edit mode through its own key dispatcher in `crates/ozone-tui/src/input.rs`:
   - keep save/cancel explicit
   - keep global non-editor shortcuts only if they still make sense
   - suppress history/slash/focus shortcuts that would fight the editor
4. Preserve the pre-edit working draft separately from the temporary edit buffer so session draft persistence does not get overwritten while editing a transcript row.
5. Update `crates/ozone-tui/src/render.rs` so edit mode has distinct labels/hints and long composer content exposes scrollbar feedback instead of silently truncating.
6. Leave the real runtime edit seam alone unless shell behavior truly requires a contract change; `apps/ozone-plus/src/runtime.rs` should keep mapping edit saves to `EngineCommand::EditMessage`.

## Gotchas

- `tui-textarea::TextArea::cursor()` is row/column state, not a flat draft char offset. Always convert it back to a global cursor before persisting `DraftState`.
- `str::lines()` drops a trailing empty line. If the editor must preserve a final newline, split on `'\n'` instead.
- Slash popup behavior should usually be derived from draft text, but edit mode is the exception: hide it entirely while a transcript message is being edited.
- If edit mode seems to “pollute” the normal composer after cancel/save, inspect `persistable_draft()` and the stored pre-edit state before changing runtime code.
- Automation channels may treat `Ctrl+I` as Tab. Keep focused tests around edit-mode routing rather than relying only on live PTY behavior.

## Verify

- `cargo test -p ozone-tui --lib --tests --quiet`
- `make preflight`
- manual/editor smoke if live PTY automation is available:
  - enter `Ctrl+I` edit on a persisted transcript row
  - move across multiline content with arrows/tab and confirm history/slash behavior stays suppressed
  - save and cancel once each, confirming the prior working draft is restored
  - confirm long edit content shows stable composer scrollbar feedback

## Debug

- If text appears right but the caret jumps across lines, inspect the textarea cursor-to-offset conversion first.
- If slash/history behavior reappears during edit mode, inspect `handle_key_event()` before the renderer.
- If save/cancel restores the wrong composer text, inspect `MessageEditState` and `persistable_draft()` before touching `apps/ozone-plus`.

## Update Scaffold

- [ ] Update `.mex/ROUTER.md` "Current Project State" when edit-mode behavior changes materially
- [ ] Update any `.mex/context/` files that still describe transcript editing as a shared draft/history surface
- [ ] Add this pattern to `.mex/patterns/INDEX.md` if it is new

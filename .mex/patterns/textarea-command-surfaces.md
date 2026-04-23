---
name: textarea-command-surfaces
description: Extend `tui-textarea`-backed command and editor surfaces across ozone+ and base Ozone without inventing parallel input models.
triggers:
  - "tui-textarea polish"
  - "command palette textarea"
  - "quick command overlay"
  - "launcher slash commands"
  - "composer textarea"
  - "textarea command surface"
edges:
  - target: "../context/conventions.md"
    condition: before changing shell state, key routing, or shared render helpers
  - target: "../patterns/ozoneplus-message-editor.md"
    condition: when the work changes ozone+ transcript edit behavior or draft/edit synchronization
  - target: "../patterns/ozone-launcher-normalization.md"
    condition: when the same pass changes base Ozone launcher routing, chrome, or typed action metadata
  - target: "../patterns/tui-launcher-smoke-test.md"
    condition: when finishing the base Ozone overlay pass and doing a real live smoke
last_updated: 2026-04-22
---

# Textarea Command Surfaces

## Context

- Keep one source of truth per product: ozone+ should let `TextArea<'static>` own composer/edit/palette input state, and base `ozone` should derive `/` overlay choices from `LauncherAction` metadata instead of adding a second command tree.
- This pattern is for **interaction polish**, not a shell rewrite. Reuse the existing app state, render pipeline, and ticker/pulse infrastructure.
- Favor single-line normalization for command-entry surfaces; multiline editing belongs in the composer or explicit transcript-edit surface, not the base launcher overlay.

## Steps

1. Identify every input surface in scope and decide whether it is:
   - multiline editor (`composer`, transcript edit)
   - single-line command entry (`palette`, launcher overlay)
2. Add surface-specific textarea setup instead of one shared default:
   - placeholder text
   - cursor/selection styling
   - cursor-line styling where multiline matters
   - undo history depth
   - tab behavior
3. Centralize text sync:
   - textarea -> persisted draft/edit state
   - persisted state -> textarea restore
   - single-line command normalization after paste/newline input
4. For ozone+, route command-palette editing through raw `KeyEvent` -> `TextArea` input so undo/redo, selection, and word movement come from the textarea layer instead of bespoke char/backspace logic.
5. When a slash popup or command-palette selection fills command text, write it through the same draft<->textarea sync path the composer already uses. Do not mutate `draft.text` alone while submit still reads from the textarea surface.
6. Distinguish command-palette outcomes explicitly:
   - UI/navigation commands act immediately
   - zero-argument shell commands can queue `RunCommand` immediately
   - argument-taking shell commands should prefill runnable slash text into the composer and leave focus in insert mode
7. For base `ozone`, open the overlay with `/`, filter via existing `LauncherAction.command`/label/description metadata, and execute the selected `LauncherActionId` through the same action handler the launcher already uses.
8. Render overlays/editors as first-class surfaces:
   - real caret
   - placeholder copy
   - visible selection state
   - explicit match or empty-state copy
   - hint text that matches the actual keys

## Gotchas

- `TextArea::cursor()` is row/column-based. When syncing multiline draft state, convert back to flat offsets deliberately.
- Rebuilding a single-line textarea to normalize pasted newlines is acceptable, but do it in one helper so cursor/history behavior stays predictable.
- If palette/slash selections appear to "show" a command but Enter does nothing, check whether the selection path updated the live textarea as well as the mirrored draft state.
- Raw PTY smoke for ratatui screens can misreport what happened after `/` or `Esc`; treat render tests plus clean MCP/screenshot smoke as the reliable verification path.
- Base `ozone` overlay commands should stay launcher-scoped. Do not let the convenience bar grow into a free-form settings editor.

## Verify

- `cargo fmt`
- `cargo test -p ozone-tui --lib --tests --quiet`
- `cargo test -p ozone --tests --quiet`
- `make preflight`
- live smoke when available:
  - ozone+ command palette / transcript edit
  - base Ozone `/settings`, `/monitor`, `/exit`

## Debug

- If the overlay shows the wrong commands, inspect the action filter before touching render code.
- If Enter from the overlay and Enter from the launcher diverge, move both through the same launcher-action executor before changing screen-specific handlers.
- If PTY smoke lands on the wrong screen but render tests pass, suspect terminal capture/input timing before assuming the routing is broken.

## Update Scaffold

- [ ] Update `.mex/ROUTER.md` when textarea-driven command/editor behavior changes materially
- [ ] Keep `.mex/patterns/INDEX.md` sorted when adding related patterns
- [ ] Link follow-up smoke learnings here once a clean launcher/ozone+ capture path is available

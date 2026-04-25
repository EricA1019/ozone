---
name: ozoneplus-reroll-swipes
description: Add or debug ozone+ assistant-reply rerolls that reuse the parent user prompt while preserving branch and swipe invariants.
triggers:
  - "session reroll"
  - "reroll selected reply"
  - "assistant reroll"
  - "swipe ordinal"
  - "reroll branch"
  - "reroll current branch"
edges:
  - target: "../patterns/ozoneplus-tui-shell.md"
    condition: when the work changes key routing, command palette behavior, or transcript/render copy in `crates/ozone-tui`
  - target: "../patterns/ozoneplus-conversation-engine.md"
    condition: when the work changes branch creation, swipe activation, or persistence rules beyond the reroll flow itself
  - target: "../context/conventions.md"
    condition: before changing runtime/TUI contracts or adding new persistence-side helper seams
last_updated: 2026-04-24
---

# Ozone+ Reroll + Swipes

## Context

- Reroll is a **selected-assistant** action. The TUI owns which transcript row is targeted; the runtime owns how that target turns into branch/swipe persistence.
- `/session reroll` is a local TUI command, not a pure runtime shell command, because the runtime shell-command path does not know the current selected transcript row.
- Keep the existing user prompt visible in state, not duplicated. If a reroll receipt references an already-present persisted user message, the TUI should reuse it instead of pushing a second copy.
- Preserve the swipe invariant: a reroll group is keyed to the parent user message, ordinal `0` remains the original assistant reply when the group is first introduced, and the new reroll result appends at the next ordinal before activation.

## Steps

1. In `crates/ozone-tui/src/app.rs`, keep reroll entry points local to the conversation shell:
   - plain `r` in conversation normal mode
   - command-palette `session reroll`
   - exact `/session reroll` draft submission interception
2. In `apps/ozone-plus/src/runtime.rs`, resolve reroll targets from the **active branch transcript**:
   - selected message must still exist
   - selected message must be an assistant turn
   - its parent must be a user message
   - capture the prior context message id (if any) for swipe-group metadata
3. Build the generation prompt from the transcript **through the parent user turn**, not through the assistant reply being rerolled.
4. Avoid mutating durable branch state before generation starts unless you also have a rollback story for cancel/failure.
5. On completion:
   - current-tip reroll: retip the current branch to the parent user, commit the new assistant reply, record/activate the new swipe
   - historical reroll: create and activate a new branch rooted at the parent user, commit there, then record/activate the new swipe
6. Return a runtime completion refresh so transcript, branches, stats, and status copy all land in one coherent TUI update.

## Gotchas

- `commit_message()` requires `message.parent_id == branch.tip_message_id`. If reroll completion fails this invariant, fix branch selection/retipping first.
- Do not let the assistant reply being rerolled remain in the prompt transcript you send to the backend.
- If you add a reroll completion refresh, make `apply_runtime_completion()` prefer the refresh transcript over blindly pushing another assistant row.
- If a reroll receipt references an existing persisted user message, the shell should not append a duplicate row just to enter `Generating`.

## Verify

- `cargo test -q -p ozone-plus -p ozone-tui`
- Exercise:
  - tip reroll on the current branch
  - historical reroll that creates a new branch
  - command-palette reroll
  - exact `/session reroll` submission from the composer

## Update Scaffold

- [ ] Update `.mex/ROUTER.md` when reroll behavior, swipe semantics, or TUI command entry points change materially
- [ ] Keep `.mex/patterns/INDEX.md` sorted after adding or renaming this pattern

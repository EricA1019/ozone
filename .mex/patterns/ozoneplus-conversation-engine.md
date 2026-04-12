---
name: ozoneplus-conversation-engine
description: Implement or extend the Phase 1B ozone+ conversation engine, engine-backed CLI flows, and durable branch/swipe behavior.
triggers:
  - "phase 1b"
  - "ozone-engine"
  - "send message"
  - "branch create"
  - "swipe activate"
  - "transcript"
edges:
  - target: "context/architecture.md"
    condition: when deciding what belongs in ozone-core, ozone-engine, ozone-persist, or apps/ozone-plus
  - target: "../ozone+/ozone_v0.4_design.md"
    condition: when mapping engine, branch, or swipe behavior to the baseline design
  - target: "context/conventions.md"
    condition: before adding engine code, repository mutations, or ozone-plus CLI commands
last_updated: 2026-04-12
---

# Ozone+ Conversation Engine

## Context

- Phase 1B is the conversation-engine slice, not the final TUI or backend layer.
- `ozone-core` should hold only the identifiers and domain enums shared across
  engine and persistence.
- `ozone-engine` should own single-writer sequencing, event fanout, snapshots,
  and the store trait used by app-specific adapters.
- `ozone-persist` should own durable message, edit-history, branch, swipe, and
  transcript-path storage rules.
- `apps/ozone-plus` should expose a thin engine-backed CLI and keep any direct
  repository access inside a local adapter/newtype, not spread through command
  handlers.

## Steps

1. Extend `ozone-core` only with engine types that are clearly shared across the
   engine and persistence layers.
2. Keep `crates/ozone-engine` trait-first: single-writer command processing,
   broadcast events, snapshots, and a testable in-memory store.
3. Add or extend `ozone-persist` repository APIs for the durable transcript,
   branch, and swipe operations the engine actually needs.
4. Wire `apps/ozone-plus` through an engine-facing facade or local store adapter
   so CLI commands drive the engine rather than mutating SQLite from scattered
   code paths.
5. Validate with workspace-wide cargo commands plus a temp-XDG smoke flow that
   covers create -> send -> edit -> branch -> swipe -> transcript -> open.

## Gotchas

- The first transcript message needs bootstrap handling; keep that special case
  inside the engine-facing adapter, not in random CLI commands.
- Swipe activation must change the visible branch tip or transcript path, not
  just update the swipe-group metadata.
- Do not couple `ozone-engine` directly to `ozone-persist`; use a local adapter
  or newtype in the app layer when both crates need to meet.
- Keep manual swipe seeding clearly temporary until Phase 1D adds real backend
  generation.

## Verify

- `cargo test --workspace --all-targets`
- `cargo check --workspace --all-targets --release`
- `cargo clippy --workspace --all-targets --release -- -D warnings`
- `cargo build --workspace --release`
- temp-XDG smoke test for `ozone-plus create`, `send`, `edit`, `branch`, `swipe`,
  `transcript`, and `open`

## Debug

- If the first `send` fails, inspect the bootstrap path that creates the initial
  branch and root message before touching later branch logic.
- If swipe activation appears to do nothing, verify the active branch tip is
  updated after the selected candidate is activated.
- If branch transcripts look wrong, inspect the closure-table ancestry rows and
  the branch `tip_message_id` before changing CLI rendering.

## Update Scaffold

- [ ] Update `.mex/ROUTER.md` "Current Project State" if engine/CLI behavior changed materially
- [ ] Update any `.mex/context/` files that are now out of date
- [ ] Add this pattern to `.mex/patterns/INDEX.md` if it is new

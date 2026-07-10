---
name: ozoneplus-persistence-bootstrap
description: Build or extend the Phase 1A ozone+ persistence bootstrap using ozone-core session types, ozone-persist, and the ozone-plus session CLI.
triggers:
  - "ozone-persist"
  - "phase 1a"
  - "create session"
  - "global.db"
  - "session.db"
edges:
  - target: "context/architecture.md"
    condition: when deciding what belongs in ozone-core, ozone-persist, or apps/ozone-plus
  - target: "../ozone+/ozone_v0.4_design.md"
    condition: when mapping schema or repository behavior to the baseline design
  - target: "context/conventions.md"
    condition: before adding persistence code or ozone-plus CLI commands
last_updated: 2026-04-12
---

# Ozone+ Persistence Bootstrap

## Context

- Phase 1A is the persistence bootstrap, not the conversation engine.
- `ozone-core` should own shared identifiers, session metadata, and persistence
  path helpers.
- `ozone-persist` should own schema setup, migrations, repository APIs, and
  advisory session-lock behavior.
- `apps/ozone-plus` should only expose a thin bootstrap CLI over those
  persistence APIs in this phase.

## Steps

1. Extend `ozone-core` only with types and paths that are clearly shared across
   persistence and future engine/UI layers.
2. Add `ozone-persist` as a workspace member and implement the Phase 1A schema
   plus migration/bootstrap logic.
3. Keep repository APIs focused on create/list/get session plus the minimum
   helper operations needed to prove the schema and locks work.
4. Wire `apps/ozone-plus` to the repository with create/list/open commands.
5. Validate with workspace-wide cargo commands plus a temporary XDG smoke test
   that creates, lists, and opens a session end to end.

## Gotchas

- Do not let the Phase 1A CLI mutate SQLite directly; go through ozone-persist.
- Do not pull conversation-engine rules into ozone-persist just because tables
  exist for later phases.
- Use UTC integer timestamps everywhere to match the design and avoid later
  migrations for timestamp cleanup.

## Verify

- `cargo test --workspace --all-targets`
- `cargo check --workspace --all-targets --release`
- `cargo clippy --workspace --all-targets --release -- -D warnings`
- `cargo build --workspace --release`
- temp-XDG smoke test for `ozone-plus create`, `list`, and `open`

## Debug

- If session creation works but listing does not, inspect the global index DB
  update path before touching the CLI.
- If FTS search tests fail, verify the content-sync triggers exist and target
  the correct `rowid`.
- If lock behavior is flaky, test stale takeover with a fake clock or explicit
  timestamps instead of sleeping in tests.

## Update Scaffold

- [ ] Update `.mex/ROUTER.md` "Current Project State" if persistence/bootstrap behavior changed materially
- [ ] Update any `.mex/context/` files that are now out of date
- [ ] Add this pattern to `.mex/patterns/INDEX.md` if it is new

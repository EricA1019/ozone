---
name: ozoneplus-workspace-bootstrap
description: Implement or extend the Phase 0 workspace bootstrap that preserves the current ozone app while adding shared crates and the ozone-plus stub target.
triggers:
  - "phase 0 workspace"
  - "ozone-core"
  - "ozone-plus stub"
  - "workspace bootstrap"
edges:
  - target: "context/architecture.md"
    condition: when deciding what belongs in the root ozone app versus shared crates or apps/ozone-plus
  - target: "../ozone+/ozone_v0.4_design.md"
    condition: when checking the workspace layout against the ozone+ baseline design
  - target: "context/conventions.md"
    condition: before extracting shared helpers or adding new workspace members
last_updated: 2026-04-12
---

# Ozone+ Workspace Bootstrap

## Context

- Phase 0 keeps the current root `ozone` package intact; do not move `src/`
  unless a later phase truly requires it.
- `crates/ozone-core` is for small shared foundations such as product metadata
  and filesystem path helpers.
- `apps/ozone-plus` is a bootstrap target in Phase 0, not the real ozone+
  product yet.
- CI and docs must become workspace-aware as soon as new members land.

## Steps

1. Preserve the root package and convert `Cargo.toml` into a combined
   `[package]` + `[workspace]` manifest.
2. Add only the minimum Phase 0 members needed for the current slice.
3. Extract shared helpers that are already duplicated across current ozone
   modules before inventing new abstractions.
4. Keep the ozone-plus app honest: product identity, docs, and path visibility
   are fine; fake frontend behavior is not.
5. Update workspace-aware cargo commands in docs and CI once the new members
   exist.
6. Validate with workspace-wide test/check/clippy/build commands before marking
   Phase 0 done.

## Gotchas

- Do not trample unrelated local edits in `Cargo.toml` or `README.md`.
- Avoid extracting logic that is still ozone-specific; Phase 0 should move only
  clearly shared foundations.
- A stub app that compiles but pretends to implement ozone+ is worse than a
  smaller, truthful bootstrap target.

## Verify

- `cargo test --workspace --all-targets`
- `cargo check --workspace --all-targets --release`
- `cargo clippy --workspace --all-targets --release -- -D warnings`
- `cargo build --workspace --release`
- `cargo run -p ozone-plus -- paths`

## Debug

- If workspace commands fail, check that every member path in `Cargo.toml`
  exists and has a valid manifest.
- If a shared path helper breaks ozone behavior, compare the extracted helper
  against the old inline path construction before changing app logic.
- If CI breaks after the split, make sure it uses workspace-aware cargo commands
  instead of root-package-only commands.

## Update Scaffold

- [ ] Update `.mex/ROUTER.md` "Current Project State" if workspace/bootstrap behavior changed materially
- [ ] Update any `.mex/context/` files that are now out of date
- [ ] Add this pattern to `.mex/patterns/INDEX.md` if it is new

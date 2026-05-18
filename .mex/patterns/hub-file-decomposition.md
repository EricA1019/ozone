---
name: hub-file-decomposition
description: Decomposing Ozone hub files like src/ui/mod.rs, crates/ozone-mcp/src/lib.rs, and crates/ozone-persist/src/repository/mod.rs in small behavior-preserving slices.
triggers:
  - "wave 2"
  - "hub-file decomposition"
  - "split src/ui/mod.rs"
  - "extract helper module"
  - "change-magnet file"
edges:
  - target: "../context/architecture.md"
    condition: when deciding whether logic should stay in a same-directory child module or move across a broader boundary
  - target: "../context/conventions.md"
    condition: before moving code across module boundaries or tightening validation scope
  - target: "ozoneplus-runtime-decomposition.md"
    condition: when the hotspot is apps/ozone-plus/src/runtime.rs and the runtime-specific seam/test guidance is more precise
last_updated: 2026-05-18
---

# Hub File Decomposition

## Context

- Large Ozone hub files accumulate unrelated edits and should be decomposed in small, behavior-preserving slices.
- Prefer same-directory child modules first; prove a stable seam before moving code into shared crates.
- Good first slices are pure builders, stateless helpers, small contiguous command clusters, or inline tests that can move without changing ownership.
- Bad first slices mix state mutation, async control flow, IO, and UI transitions with no clear seam.

## Steps

1. Pick one cohesive seam and name the smallest concrete boundary.
   - Prefer pure builders, stateless helpers, or one contiguous cluster with one caller group.
   - Confirm the call sites and owning types before editing.
2. Create a child module beside the hub file and move only that seam.
   - Keep visibility minimal with `pub(super)` or a private re-export.
   - Leave stateful orchestration in the hub file until a larger seam is proven.
3. Add focused regression coverage when the moved seam is pure or already has an obvious input/output contract.
4. Validate immediately with the narrowest compile or test command that covers the moved seam.
5. Widen validation to the owning package before moving on.

## Gotchas

- Imports used only by moved code become unused in the parent hub file; trim them immediately.
- Do not rename user-facing commands, status text, or config keys while extracting a seam.
- Avoid cross-crate moves on the first slice unless the boundary is already stable.
- If the seam lacks a cheap executable check, add a small unit test before relying on package-wide validation.

## Verify

- Narrow seam validation first, for example:
  - `cargo test -p ozone build_ --quiet`
  - `cargo check -p ozone`
  - `cargo test -p ozone-plus <seam-test> --quiet`
- Then widen to the owning package or crate suite.

## Debug

- If a move causes visibility or import errors, reduce the public surface instead of pulling more parent logic into the child module.
- If the seam starts touching unrelated state, stop and choose a smaller boundary.

## Update Scaffold

- [ ] Update `.mex/ROUTER.md` when a new decomposition slice lands
- [ ] Update the active plan when Wave 2 status changes
- [ ] Add this pattern to `.mex/patterns/INDEX.md`

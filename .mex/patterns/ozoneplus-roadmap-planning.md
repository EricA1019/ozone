---
name: ozoneplus-roadmap-planning
description: Build a phased ozone+ execution plan from the family docs, v0.4 design, and the current ozone codebase.
triggers:
  - "ozone+ action plan"
  - "build ozone+"
  - "ozone+ roadmap"
  - "plan ozone+"
edges:
  - target: context/architecture.md
    condition: when mapping current modules to future crates or app boundaries
  - target: ../ozone+/README.md
    condition: when confirming product-family boundaries and build positioning
  - target: ../ozone+/ozone_v0.4_design.md
    condition: when converting the v0.4 architecture and phase order into implementation work
last_updated: 2026-04-12
---

# Ozone+ Roadmap Planning

## Context
- The current repository implements the middle-tier `ozone` product, not the
  full ozone+ stack.
- `ozone+/README.md` defines the family split and should be treated as the build
  boundary reference.
- `ozone+/ozone_v0.4_design.md` is the ozone+ baseline architecture and roadmap,
  especially `§5.4`, `§13`, `§29`, and `§31`.
- The current codebase is a single-binary Rust TUI with reusable launch,
  hardware, process, theme, and terminal foundations.

## Steps
1. Read `ozone+/README.md`, `ozone+/ozone_plus_documentation_stack.md`, the
   relevant sections of `ozone+/ozone_v0.4_design.md`, and the current codebase
   entry points (`Cargo.toml`, `src/main.rs`, `src/db.rs`, `src/ui/mod.rs`, and
   any backend/process modules relevant to reuse).
2. Treat the current app as `ozone`, not as a partially finished ozone+ build.
3. Produce a reuse-vs-rewrite map:
   - what can move into shared crates
   - what must stay ozone-only
   - what must be built net-new for ozone+
4. Add a repo-structure pre-phase if the current repository layout cannot
   directly support the v0.4 crate plan. In this repo, that means a workspace
   extraction phase before `1A`.
5. Preserve the v0.4 execution order after that pre-phase:
   `1A` through `1F`, then `2A` through `2C`, then later phases.
6. End the plan with a concrete "next slice" so implementation can begin
   without re-planning.

## Gotchas
- Do not blur product boundaries by turning ozone+ into "the current ozone app
  with a chat screen bolted on."
- Do not make `bench`, `sweep`, `analyze`, or the current profiling database a
  Phase 1 dependency for ozone+.
- The current SQLite setup is reusable as a pattern, but the ozone+ schema is
  fundamentally different and should be rebuilt from the design doc.
- If you need to adapt the v0.4 crate layout to the family repo, say so
  explicitly. The adaptation is intentional, not a mistake.

## Verify
- The plan names the repo strategy, not just the feature roadmap.
- The plan distinguishes shared reusable code from ozone-only code.
- The plan covers all of `1A` through `1F`, `2A` through `2C`, and later phases.
- The plan ends with the recommended first execution slice.

## Debug
- If the plan keeps collapsing back to the current single-binary layout, reload
  the family docs and restate the ozone/ozone+ split first.
- If the plan is too abstract, read the current `src/` modules again and map
  them directly to future crates or keep/extract decisions.
- If the phase order drifts, reload `§29` and `§31` and rebuild the sequence.

## Update Scaffold
- [ ] Update `.mex/ROUTER.md` "Current Project State" if what's working/not built has changed
- [ ] Update any `.mex/context/` files that are now out of date
- [ ] If this is a new task type without a pattern, create one in `.mex/patterns/` and add to `.mex/patterns/INDEX.md`

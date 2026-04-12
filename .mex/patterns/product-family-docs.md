---
name: product-family-docs
description: Updating or extending the Ozone family documentation split, scope docs, and compatibility notes
triggers:
  - "product family"
  - "documentation split"
  - "ozonelite"
  - "ozone+ docs"
edges:
  - target: "context/decisions.md"
    condition: when the task changes product boundaries or scope ownership between builds
  - target: "context/conventions.md"
    condition: when writing or editing docs and you need naming/style guidance
last_updated: 2026-04-12
---

# Product Family Documentation

## Context

- Start with `ozone+/README.md` because it is the onboarding and family-routing document.
- Treat `ozone_v0.4_design.md` as the ozone+ baseline design, not the umbrella family spec.
- Keep product-level docs and family-level docs separate on purpose.

## Steps

1. Audit the current doc surfaces before editing:
   - `README.md` for onboarding and family boundaries
   - build-specific scope docs
   - family-wide compatibility notes
   - ozone+ baseline design / roadmap docs
2. Decide which layer the new content belongs to:
   - family onboarding
   - ozonelite scope
   - ozone scope
   - ozone+ architecture / build prep
   - shared compatibility / migration
3. Add or update docs so each build has explicit "in scope" and "out of scope" boundaries.
4. Tighten cross-links so new readers can find the right doc without reading historical documents first.
5. Preserve historical design docs as history; do not silently rewrite them into the new primary routing layer.

## Gotchas

- Do not let `ozone` drift into full conversation/frontend scope.
- Do not let ozone+ docs redefine the entire family when the content belongs in `README.md`.
- Do not present shared compatibility notes as already-implemented guarantees if the schemas are still planned.
- Do not delete or flatten historical docs just because the family split is clearer now.

## Verify

- `README.md` points to the current product-specific and shared docs.
- `ozonelite`, `ozone`, and `ozone+` each have explicit boundaries.
- Shared compatibility and upgrade-path guidance lives in a shared doc, not scattered across multiple product docs.
- `ozone_v0.4_design.md` is clearly positioned as the ozone+ baseline.

## Debug

- If a doc feels mixed or confusing, identify whether the content is:
  - onboarding
  - product scope
  - shared compatibility
  - ozone+ architecture / roadmap
- Then move the content to the correct layer instead of adding more explanation in the wrong file.

## Update Scaffold

- [ ] Update `.mex/ROUTER.md` "Current Project State" if the documentation stack changed materially
- [ ] Update any `.mex/context/` files that are now out of date
- [ ] Add this pattern to `.mex/patterns/INDEX.md` if it is new

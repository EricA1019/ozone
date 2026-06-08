---
name: graphify-scoped-analysis
description: Rebuild a narrower Graphify graph for one Ozone slice when the repo-root graph is too noisy, generic symbol collisions need verification, or you want a cleaner architecture map.
triggers:
  - "graphify scope"
  - "narrower graph"
  - "scoped graphify"
  - "false positive"
  - "generic symbol collision"
edges:
  - target: "patterns/graphify-integration.md"
    condition: when Graphify itself needs install or refresh work before running the scoped analysis
  - target: "context/architecture.md"
    condition: when choosing the highest-signal Ozone slice to graph
  - target: "context/conventions.md"
    condition: when you need the verify checklist or are updating .mex docs afterward
last_updated: 2026-05-12
---

# Graphify Scoped Analysis

## Context

- The repo-root graph is useful for broad discovery, but Ozone is large enough that generic labels like `main()` and `.new()` can create misleading inferred edges across crates.
- For architecture questions, prefer a narrower code-only slice such as `src`, `crates/ozone-tui/src`, or `crates/ozone-memory/src`.
- Keep scoped outputs out of the main repo graph by building them under `tmp/graphify-scopes/<scope>/graphify-out/`.
- The repo now ships `./contrib/graphify-scope.sh` and `make graphify-scope SCOPE=<scope>` so common scoped builds can be rebuilt without retyping the extraction/build pipeline.

## Steps

1. Choose the smallest slice that still contains the behavior you care about.
   - Use `src` for the base launcher/planner/process graph.
   - Use `crates/ozone-tui/src` for layout/render/event-loop questions.
  - Use `ozone-tui-core` for a production-only TUI core graph around `lib.rs`, `layout.rs`, and `render/coordinator.rs` when the full crate is too test-heavy.
   - Use `crates/ozone-memory/src` for embedding, retrieval, and memory-type questions.
2. Prefer the helper first.
  - Run `./contrib/graphify-scope.sh --list` to see supported scopes.
  - Run `make graphify-scope SCOPE=<scope>` for the standard isolated build path.
3. Create an isolated working directory under `tmp/graphify-scopes/<scope>/` and write a local `graphify-out/.graphify_python` there.
   - Do not reuse or overwrite the repo-root `graphify-out/` for this analysis.
4. Run detect on the target slice first.
   - If the slice is code-only, skip semantic extraction and use AST extraction only.
   - If docs/images are present, either narrow the path further or accept that semantic extraction will add time and cost.
5. Build the scoped graph and report in the isolated work directory.
   - Save at least `graph.json`, `GRAPH_REPORT.md`, and `.graphify_analysis.json`.
   - Generate `graph.html` only if the graph is still small enough to be useful.
6. Compare the scoped report against the repo-root report.
   - Check whether cross-crate surprises disappear.
   - Compare node, edge, and community counts.
   - Review the scoped god nodes and surprising connections before trusting any inferred edge.
7. Validate the top inferred edges in source.
   - Treat Graphify edges involving generic labels like `main()`, `.new()`, `open()`, or path-helper functions as hypotheses until code confirms them.

## Gotchas

- `tmp/` is typically ignored by workspace search, so search tools may need `includeIgnoredFiles=true` when inspecting scoped outputs.
- A narrower graph removes some false positives, but it does not eliminate all local inference noise.
  - In base `src`, helper/path names like `presets_path()` can still attract inferred edges that need code verification.
- `crates/ozone-tui/src` is useful for render-loop questions, but embedded tests and mock runtime helpers can dominate god-node rankings.
  - `MockRuntime` and `seeded_state()` are expected hotspots in that scope; they are not architectural roots.
- `ozone-tui-core` exists specifically to avoid that bias.
  - It strips embedded `#[cfg(test)] mod tests` blocks from the three core files before extraction and currently validates as a much cleaner graph (`39` nodes, `59` edges) centered on `run_event_loop()`, `build_layout_for_area()`, and `build_render_model()`.
- `crates/ozone-memory/src` is currently the highest-signal scoped graph because it is smaller and mostly structural, with very few inferred edges.

## Verify

- Scoped outputs live under `tmp/graphify-scopes/<scope>/graphify-out/`, not the repo-root `graphify-out/`
- The scoped report mentions only files inside the chosen slice
- Cross-crate false positives from the repo-root graph are gone or sharply reduced
- The main architecture question for that slice is answered by scoped god nodes plus validated call edges
- Any remaining generic-name inferred edges were checked against real source before being trusted

## Debug

- If the scoped graph still looks noisy, narrow again to a smaller subtree or a specific module cluster.
- If test helpers dominate the report, choose a more specific slice centered on production modules instead of the whole crate `src` tree.
- If a scoped report is hard to inspect from tools, remember that `tmp/` may be ignored and rerun searches with ignored files included.
- If the graph is empty, confirm detect found code files and that AST extraction wrote non-empty `nodes` and `edges`.

## Update Scaffold
- [ ] Update `.mex/ROUTER.md` "Current Project State" if what's working/not built has changed
- [ ] Update any `.mex/context/` files that are now out of date
- [ ] If this is a new task type without a pattern, create one in `.mex/patterns/` and add to `INDEX.md`
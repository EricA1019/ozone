# Pattern Index

Lookup table for all pattern files in this directory. Check here before starting any task — if a pattern exists, follow it.

<!-- This file is populated during setup (Pass 2) and updated whenever patterns are added.
     Each row maps a pattern file (or section) to its trigger — when should the agent load it?

     Format — simple (one task per file):
     | [filename.md](filename.md) | One-line description of when to use this pattern |

     Format — anchored (multi-section file, one row per task):
     | [filename.md#task-first-task](filename.md#task-first-task) | When doing the first task |
     | [filename.md#task-second-task](filename.md#task-second-task) | When doing the second task |

     Example (from a Flask API project):
     | [add-api-client.md](add-api-client.md) | Adding a new external service integration |
     | [debug-pipeline.md](debug-pipeline.md) | Diagnosing failures in the request pipeline |
     | [crud-operations.md#task-add-endpoint](crud-operations.md#task-add-endpoint) | Adding a new API route with validation |
     | [crud-operations.md#task-add-model](crud-operations.md#task-add-model) | Adding a new database model |

     Keep this table sorted alphabetically. One row per task (not per file).
     If you create a new pattern, add it here. If you delete one, remove it. -->

| Pattern | Use when |
| ------- | -------- |
| [artifact-hygiene.md](artifact-hygiene.md) | Cleaning up oversized build outputs, preventing stale artifact buildup, or adding repo hygiene around transient generated files |
| [audit-triage-planning.md](audit-triage-planning.md) | Turning a full-project audit into an ordered remediation plan with explicit waves, scope boundaries, and validation gates |
| [copilot-skill-customization.md](copilot-skill-customization.md) | Creating or updating reusable local Copilot skills in the user-level Copilot skill library |
| [env-isolated-tests.md](env-isolated-tests.md) | Writing or debugging tests that depend on config defaults, `OZONE__...` env overrides, or XDG-resolved repo/config paths |
| [eval-report-viewer.md](eval-report-viewer.md) | Converting eval JSON or JSONL artifacts into markdown reports and surfacing them inside the Bench + Eval TUI |
| [eval-result-ranges.md](eval-result-ranges.md) | Documenting eval probe score ranges, metric names, and how to read lm-eval or EvalPlus results |
| [graphify-integration.md](graphify-integration.md) | Installing or refreshing Graphify for Ozone, aligning the user-level Copilot skill, and wiring repo-managed graph refreshes |
| [graphify-scoped-analysis.md](graphify-scoped-analysis.md) | Rebuilding a narrower Graphify view for one Ozone slice when the repo-root graph is too noisy or generic symbol collisions need verification |
| [github-actions-release.md](github-actions-release.md) | Debugging or updating this repo's GitHub Actions CI or release automation |
| [hub-file-decomposition.md](hub-file-decomposition.md) | Starting Wave 2-style behavior-preserving extractions from central hub files such as `src/ui/mod.rs`, `crates/ozone-mcp/src/lib.rs`, or `crates/ozone-persist/src/repository/mod.rs` |
| [koboldcpp-launch-diagnostics.md](koboldcpp-launch-diagnostics.md) | Diagnosing or hardening the KoboldCpp launcher path, startup failures, or override-wrapper behavior in base Ozone |
| [launcher-configure-hub.md](launcher-configure-hub.md) | Adding or extending the base launcher Configure Hub, saved per-model launch profiles, manual context/GPU tuning, or attached profiling/report UI |
| [local-install-sync.md](local-install-sync.md) | Updating `~/.cargo/bin` / `~/.local/bin` safely from current release artifacts without overwriting matching binaries |
| [llamacpp-backend-integration.md](llamacpp-backend-integration.md) | Adding or extending llama.cpp-backed HF imports, ozone+ runtime support, or base-launcher llama.cpp wiring |
| [mex-scaffold-sync.md](mex-scaffold-sync.md) | Detecting or fixing drift in the .mex scaffold, paths, or helper scripts |
| [ozone-mcp-automation.md](ozone-mcp-automation.md) | Building or extending the developer-facing ozone MCP server, its stdio tool contract, sandbox helpers, or launcher smoke orchestration |
| [ozone-launch-planner-parity.md](ozone-launch-planner-parity.md) | Aligning base Ozone fast-launch planning with profiling, especially GGUF topology/layer-count sourcing |
| [ozone-launcher-normalization.md](ozone-launcher-normalization.md) | Normalizing the base Ozone launcher chrome, typed action metadata, settings UX, or future `/` quick-command groundwork |
| _Archived ozone+ patterns_ — see `docs/archive/ozone-plus-patterns/` | The ozone+ chat shell is deprecated. Patterns moved 2026-07-10. |
| [product-family-docs.md](product-family-docs.md) | Updating or extending the Ozone family documentation split, scope docs, or compatibility notes |
| [release-smoke-gates.md](release-smoke-gates.md) | Adding or debugging shipped-artifact release smoke for fresh temp-XDG and existing-user paths, especially when `make release-gates` or `make release-smoke` changes |
| [safe-hygiene-pass.md](safe-hygiene-pass.md) | Running a behavior-preserving cleanup pass for dead tracked residue, stale comments/text, ignore-rule gaps, or unused plumbing while a dirty worktree is in play |
| [startup-failure-hardening.md](startup-failure-hardening.md) | Surfacing hidden startup, loader, catalog, or install-update failures without widening into structural refactors |
| [textarea-command-surfaces.md](textarea-command-surfaces.md) | Extending cross-product `tui-textarea` command/composer surfaces, including ozone+ palette/editor polish and base Ozone `/` quick-command overlays |
| [tui-launcher-smoke-test.md](tui-launcher-smoke-test.md) | Running a live smoke test of the base Ozone launcher, monitor, profiling, and clear-GPU flows |
| [tui-terminal-session-guard.md](tui-terminal-session-guard.md) | Adding or debugging base Ozone TUI entrypoints that use raw mode, alternate screen, or monitor-style live refresh loops that can leave the terminal looking crashed |
| [tui-profiling-workflow.md](tui-profiling-workflow.md) | Adding, reviewing, or debugging the Ozone TUI profiling/advisory/report flow |

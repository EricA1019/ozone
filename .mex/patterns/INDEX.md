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
|---------|----------|
| [github-actions-release.md](github-actions-release.md) | Debugging or updating this repo's GitHub Actions CI or release automation |
| [mex-scaffold-sync.md](mex-scaffold-sync.md) | Detecting or fixing drift in the .mex scaffold, paths, or helper scripts |
| [ozoneplus-conversation-engine.md](ozoneplus-conversation-engine.md) | Building or extending the Phase 1B ozone+ engine, engine-backed CLI, branches, or swipe flows |
| [ozoneplus-context-inspector.md](ozoneplus-context-inspector.md) | Building or extending the Phase 1E context assembler surface, inspector preview, or dry-run trigger |
| [ozoneplus-phase1f-import-export.md](ozoneplus-phase1f-import-export.md) | Building or extending Phase 1F import/export (character cards, session/transcript export), bookmarks, slash commands, or stats |
| [ozoneplus-phase1g-launcher-onramp.md](ozoneplus-phase1g-launcher-onramp.md) | Building or extending the Phase 1G frontend-choice screen, FrontendMode, --frontend flag, or exec handoff to ozone-plus |
| [ozoneplus-phase2a-memory-foundations.md](ozoneplus-phase2a-memory-foundations.md) | Building or extending Phase 2A manual retrieval: ozone-memory types, pinned memories, FTS recall, `memory`/`search` commands, `Ctrl+K`, or `:memories` |
| [ozoneplus-phase2b-hybrid-retrieval.md](ozoneplus-phase2b-hybrid-retrieval.md) | Building or extending Phase 2B embeddings, vector-index rebuilds, hybrid recall/search, stale-embedding handling, or `RetrievedMemory` context injection |
| [ozoneplus-persistence-bootstrap.md](ozoneplus-persistence-bootstrap.md) | Building or extending the Phase 1A ozone+ persistence bootstrap, schema, or session CLI |
| [ozoneplus-roadmap-planning.md](ozoneplus-roadmap-planning.md) | Turning the ozone+ docs and current codebase into a phased execution plan |
| [ozoneplus-streaming-backend-runtime.md](ozoneplus-streaming-backend-runtime.md) | Building or extending the Phase 1D ozone+ inference crate, app-side adapter, or streamed backend runtime path |
| [ozoneplus-tui-shell.md](ozoneplus-tui-shell.md) | Building or extending the Phase 1C ozone+ TUI shell, `open` integration, draft persistence, or mock-runtime chat flow |
| [ozoneplus-workspace-bootstrap.md](ozoneplus-workspace-bootstrap.md) | Implementing or extending the Phase 0 workspace split, shared ozone-core crate, or ozone-plus bootstrap app |
| [product-family-docs.md](product-family-docs.md) | Updating or extending the Ozone family documentation split, scope docs, or compatibility notes |
| [tui-profiling-workflow.md](tui-profiling-workflow.md) | Adding, reviewing, or debugging the Ozone TUI profiling/advisory/report flow |

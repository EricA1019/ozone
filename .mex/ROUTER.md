---
name: router
description: Session bootstrap and navigation hub. Read at the start of every session before any task. Contains project state, routing table, and behavioural contract.
edges:
  - target: context/architecture.md
    condition: when working on system design, integrations, or understanding how components connect
  - target: context/stack.md
    condition: when working with specific technologies, libraries, or making tech decisions
  - target: context/conventions.md
    condition: when writing new code, reviewing code, or unsure about project patterns
  - target: context/decisions.md
    condition: when making architectural choices or understanding why something is built a certain way
  - target: context/setup.md
    condition: when setting up the dev environment or running the project for the first time
  - target: patterns/INDEX.md
    condition: when starting a task — check the pattern index for a matching pattern file
last_updated: 2026-04-14
---

# Session Bootstrap

If you haven't already read `AGENTS.md`, read it now — it contains the project identity, non-negotiables, and commands.

Then read this file fully before doing anything else in this session.

## Current Project State

**Working:**
- Launcher dashboard, model picker, launch confirm flow, and live monitor all work in the ratatui TUI
- CLI profiling commands work: `bench`, `sweep`, `analyze`, profile generation, and preset export
- New TUI profiling flow works: advisory -> confirm -> running -> success/failure report
- Profiling now gives actionable warnings for risky plans and issue reports for invalid model paths / launcher failures
- Success reports keep the UX review-first: generate/export/launch are explicit follow-up actions, not automatic side effects
- Product-family docs now include a top-level family guide plus dedicated `ozonelite`, `ozone`, `ozone+`, and shared compatibility documents under `ozone+/`
- GitHub Actions CI is clippy-clean again, and release automation is wired so pushes to `main` can create a version tag while the actual packaged release is built from the resulting tag run
- ozone+ now has a concrete phased execution plan: preserve the current `ozone` app, add a workspace pre-phase, then build `1A` through `1G` before Phase 2 memory and later assistive/group-chat layers
- The `.mex` scaffold passes `mex check` again, and `.mex/setup.sh` / `.mex/sync.sh` now work as repo-root wrappers for `mex setup` and `mex sync`
- The user Copilot skill library now includes a validated `response-consolidation` skill, and `.mex` now has a reusable pattern for future local skill customization work
- Phase 0 is now implemented: the repo is a Cargo workspace, `crates/ozone-core` owns shared product/path helpers, and `apps/ozone-plus` exists as a bootstrap CLI target
- Phase 1A is now implemented: `crates/ozone-persist` owns the ozone+ persistence bootstrap, and `apps/ozone-plus` can create, list, and open persisted sessions
- Phase 1B is now implemented: `crates/ozone-engine` exists, `crates/ozone-persist` owns durable branch/swipe/message operations, and `apps/ozone-plus` can drive send/edit/transcript/branch/swipe flows through an engine-backed CLI
- Phase 1C is now implemented: `crates/ozone-tui` owns the ozone+ shell state/input/layout/render loop, `apps/ozone-plus open <session-id>` launches the TUI by default, `--metadata` preserves the old summary path, and drafts now persist across close/reopen
- Phase 1D is now implemented: `crates/ozone-inference` owns layered config, prompt templates, streaming decode, and the first KoboldCpp gateway, while `apps/ozone-plus` now drives a real async streaming runtime with failure/cancel handling and persisted assistant turns
- Phase 1E is now implemented: `crates/ozone-engine` owns the first `ContextPlan` / `ContextAssembler` core, `apps/ozone-plus` now bridges context preview + dry-run data, and `crates/ozone-tui` now renders a real context inspector surface with a `Ctrl+D` dry-run trigger
- Phase 1F is now implemented: `crates/ozone-persist` owns character-card import + session/transcript export, `apps/ozone-plus` has `import card` / `export session` / `export transcript` CLI commands, `crates/ozone-tui` now supports bookmark toggle (`b`), session stats, and `/session` slash commands (rename/character/tags/show)
- Phase 1G is now implemented: `src/ui/mod.rs` adds `Screen::FrontendChoice` and `FrontendMode` enum; `ozone --frontend ozonePlus` bypasses the choice screen and exec-hands-off to `ozone-plus list`; the choice screen sits between Confirm and Launch with ↑↓/Enter navigation
- Phase 2A is now implemented: `crates/ozone-memory` owns typed pinned-memory / provenance / recall models, `crates/ozone-persist` reuses `memory_artifacts` + `session_search` for pinned memory and cross-session FTS recall, `apps/ozone-plus` now has `memory` / `search` CLI commands, and `crates/ozone-tui` supports `Ctrl+K`, `/memory ...`, `/search ...`, and `:memories` with recall output in the existing inspector/status surfaces
- Phase 2B is now implemented: `crates/ozone-memory` now owns optional embedding providers, retrieval scoring, and the disk-backed `usearch` vector index manager; `crates/ozone-persist` now persists embedding artifacts in `memory_artifacts`; and `apps/ozone-plus` now supports `index rebuild`, hybrid session/global search, stale-embedding filtering, and `RetrievedMemory` context injection with explicit FTS-only fallback
- Phase 2C is now implemented (alpha → gamma): `apps/ozone-plus` now has `/summarize`, memory-recall lifecycle labels, `lifecycle inspect`, GC planning and execution commands, `events compact`, and `lifecycle disk-status`; `crates/ozone-memory` owns `StorageTierPolicy`, `ArtifactStaleness`, the disk-monitor policy/status/check result types, and `VersionCompatibilityResult`; `crates/ozone-persist` owns derived-artifact GC planning/application and events compaction; `crates/ozone-inference` has full `MemoryLifecycleConfig` under `[memory.lifecycle]`
- Phase 5 is now implemented: the launcher ships the lime-green brand refresh, the tier picker, and `--mode` / `--pick` family selection for ozonelite / ozone / ozone+
- Phase 5.5 is now implemented: ozone now has `model list|add|remove|info`, ozone+ copy and metadata reflect the shipped MVP state, timestamps are human-readable, and release docs/changelog now cover `v0.4.0-alpha`

**Phase 2 is COMPLETE. All of Phase 2A, 2B, and 2C (alpha/beta/gamma) have shipped.**

**Phase 3 is COMPLETE (2026-04-14).** All cleanup tasks a–e shipped in commits db33494 and af96d0c:
- `ThinkingBlockDecoder` wired into streaming token loop; `/thinking [status|hidden|assisted|debug]`
- `ImportanceScorer` + `KeywordExtractor` wired post-generation when Tier B enabled; `/tierb [status|toggle]`
- `HooksConfig` fires pre/post generation; `/hooks [status|list]`
- Safe mode (`/safemode [status|on|off|toggle]`) gates all Tier B features
- 8 new integration tests; workspace total 218 → 226

**Not yet built:**
- TUI cancellation for in-flight profiling runs
- A dedicated in-TUI browser for existing benchmark history and Pareto data
- Rich per-substep sweep visualizations beyond streamed text progress
- Phase 3 (group chat, assistive layer, original ozone integration) — **COMPLETE, see above**
- Phase 4 (scene foundation: multi-actor scenes, speaker routing, group-chat model)

**Known issues:**
- Broken `.gguf` symlinks can still appear in the catalog/list and are only surfaced as issues when selected
- Hardware guidance is still NVIDIA-centric because GPU memory detection depends on `nvidia-smi`
- Focused ozone+ tests now cover core, engine, persistence, TUI, and app-level draft restore; the older root `ozone` app still has intentionally light test coverage outside profiling helpers
- Manual swipe seeding in `ozone-plus` is still a temporary CLI helper for transcript experiments; automatic alternate-generation/swipe creation is not yet wired into the runtime
- The Phase 1D live smoke in this session used a temporary KoboldCpp-compatible mock backend because no real local KoboldCpp server was running; a real-model smoke pass is still recommended when one is available
- The Phase 1E live smoke verified inline context preview with a mock backend, but the `Ctrl+D` path itself was verified through cargo tests because this PTY automation channel cannot reliably send every control chord
- The Phase 2A live smoke verified `:memories` and `/search ...` in the TUI, but `Ctrl+K` itself was validated through tests because this PTY automation channel cannot reliably send every control chord
- The Phase 2B live smoke verified CLI fallback -> `index rebuild` -> hybrid search with the mock embedding provider; the full-screen TUI hybrid-recall path is still better covered by cargo tests than PTY-driven visual automation
- `cargo build --release` at the repo root is not sufficient when you want a fresh `target/release/ozone-plus` artifact; explicitly building `-p ozone-plus` avoids stale smoke-test binaries

## Routing Table

Load the relevant file based on the current task. Always load `context/architecture.md` first if not already in context this session.

| Task type | Load |
|-----------|------|
| Understanding how the system works | `context/architecture.md` |
| Working with a specific technology | `context/stack.md` |
| Writing or reviewing code | `context/conventions.md` |
| Making a design decision | `context/decisions.md` |
| Setting up or running the project | `context/setup.md` |
| Any specific task | Check `patterns/INDEX.md` for a matching pattern |

## Behavioural Contract

For every task, follow this loop:

1. **CONTEXT** — Load the relevant context file(s) from the routing table above. Check `patterns/INDEX.md` for a matching pattern. If one exists, follow it. Narrate what you load: "Loading architecture context..."
2. **BUILD** — Do the work. If a pattern exists, follow its Steps. If you are about to deviate from an established pattern, say so before writing any code — state the deviation and why.
3. **VERIFY** — Load `context/conventions.md` and run the Verify Checklist item by item. State each item and whether the output passes. Do not summarise — enumerate explicitly.
4. **DEBUG** — If verification fails or something breaks, check `patterns/INDEX.md` for a debug pattern. Follow it. Fix the issue and re-run VERIFY.
5. **GROW** — After completing the task:
   - If no pattern exists for this task type, create one in `patterns/` using the format in `patterns/README.md`. Add it to `patterns/INDEX.md`. Flag it: "Created `patterns/<name>.md` from this session."
   - If a pattern exists but you deviated from it or discovered a new gotcha, update it with what you learned.
   - If any `context/` file is now out of date because of this work, update it surgically — do not rewrite entire files.
   - Update the "Current Project State" section above if the work was significant.

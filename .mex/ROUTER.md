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
last_updated: 2026-04-19
---

# Session Bootstrap

If you haven't already read `AGENTS.md`, read it now — it contains the project identity, non-negotiables, and commands.

Then read this file fully before doing anything else in this session.

## Current Project State

**Working:**
- **v0.4.5-alpha shipped**: Settings crash fixes (usize underflow, out-of-bounds category index, `"Context"` → Model mapping, missing `Session` variant), side-by-side launch preference persists and drives launcher label, theme preset system (`DarkMint`/`OzoneDark`/`HighContrast`, default `#2DAF82`), and fully interactive editable settings (Toggle + Cycle entries for Appearance, Launch, Display categories) — 18 new tests; ozone-tui total now 143
- **Current version**: `0.4.5-alpha`
- TUI QOL upgrade shipped: scrollable lists with `PgUp`/`PgDn` (ratatui `ListState` + `Scrollbar`), slash autocomplete popup, settings drill-down, 1-row footer, braille spinner during generation, message separators, colored INS/CMD mode badges, Model Intelligence screen, and side-by-side monitor mode
- llama.cpp native profiler support shipped: full QuickSweep/FullSweep/SingleBenchmark/GenerateProfiles parity with KoboldCpp, token rate from `/completion` timings, profile export to `~/.local/share/ozone/`, auto backend detection, structured launcher args saved to prefs, and classified startup diagnostics (`GgmlAbort`, `CudaOom`, `CudaError`, `ModelLoadFailed`, `MissingSharedLibrary`, `RuntimeCrash`, `Timeout`)
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
- Phase 5 is now implemented: the launcher ships the teal brand refresh, the tier picker, and `--mode` / `--pick` family selection for ozonelite / ozone / ozone+
- Phase 5.5 is now implemented: ozone now has `model list|add|remove|info`, ozone+ copy and metadata reflect the shipped MVP state, timestamps are human-readable, and the current development version now targets `v0.4.2-alpha` while the changelog still records `v0.4.0-alpha` as the last tagged release
- MVP launcher polish landed after live install testing: saved frontend preference is now honored when launching, and the selected `ozone+` row in the tier picker uses a higher-contrast accent so it remains readable on the dark theme
- Launcher/settings UX follow-up landed after the theme lock-in: the current teal minimalist palette is preserved, `Esc` now backs out to the previous menu instead of quitting from intermediate launcher screens, the launcher itself asks for exit confirmation, and Settings now clearly exposes backend/frontend section focus with real cancel-on-Esc behavior
- Backend visibility follow-up landed after monitor feedback: service polling now tracks Ollama explicitly, the launcher and monitor service panels show all three relevant services (`KoboldCpp`, `Ollama`, `SillyTavern`), and monitor hints now match the actual back/exit keys so backend selection is easier to confirm in the installed binary
- ozone -> ozone+ launcher handoff now opens a real ozone+ session shell instead of execing `ozone-plus list`; the hidden `ozone-plus handoff` entrypoint reuses the freshest session when possible, creates a fallback launcher session when needed, and the installed binaries under both `~/.local/bin` and `~/.cargo/bin` are now synced at `0.4.1-alpha`
- Launcher smoke-fix pass landed after live testing: the splash version label no longer duplicates `alpha`, `Launch` with saved `Ollama + ozone+` now hands off into `ozone-plus`, and the TUI now clears the frame before redraws to reduce stale-screen overlap during launcher, monitor, and profiling transitions
- Base launcher live smoke re-validated on the installed binary: `Launch` with `--no-browser --frontend sillyTavern` and Ollama reaches the Monitor screen, the profiling advisor correctly routes broken `.gguf` symlinks into a reviewable failure path, and the destructive `Clear GPU` action is best verified separately because external Ollama management may keep `:11434` listening after the UI reports success
- ozone+ MVP smoke surfaces were validated on a fresh imported character session: `open` loaded the real TUI shell, a generated assistant turn persisted, `memory pin`, `memory note`, `memory list`, and `search session` worked, and pinned/note memories reached generation context in the live shell
- Post-smoke hardening landed for the remaining ozone+ blockers: `start_kobold()` now fails fast with classified launcher/install diagnostics and supports `OZONE_KOBOLDCPP_LAUNCHER` for repaired wrapper overrides, ozone+ insert/command modes now treat `?` as literal text instead of hijacking Help, and FTS-only search now includes note-memory hits in both session and global recall flows
- Fresh-user launcher/runtime follow-up landed after the latest seam audit: splash auto-advance now honors the tier picker for unset preferences, the base launcher live-refreshes its model catalog while ozone stays open, `ozone-plus send` now uses the real Phase 1D runtime path for normal user prompts, and launcher exec handoff now targets `ozone-plus handoff --launcher-session` so the launcher reuses a dedicated `Launcher Session` instead of implicitly jumping into the freshest chat
- The launcher now rejects `Ollama + ozone+` from the guided launch path with an explicit error because the current ozone+ runtime supports KoboldCpp and llama.cpp, but not Ollama; direct ozone+ shell entry remains available, and the launcher no longer pretends that the unsupported combo is wired end-to-end
- Deep live-test follow-up landed for ozone+ swipe behavior: activating a swipe candidate after branching no longer tries to retip an unrelated active branch just because the swipe parent appears earlier in that transcript, so the old ancestry-consistency failure is gone and the CLI help now points users at `swipe list` before `swipe activate`
- Fresh temp-XDG smoke on the current release artifacts still works end-to-end for the main paths: `Launch` with saved `Ollama + SillyTavern` reaches `Monitor`, `Open ozone+` execs into `ozone-plus list`, profiling a missing-model symlink lands on the issue report instead of crashing, and a real CPU-only KoboldCpp run (`mn-12b-mag-mell-r1.gguf`) can serve ozone+ with persisted assistant turns plus memory/search flows
- Base Ozone and ozone+ now have first-class llama.cpp support: `ozone model add --hf` uses llama.cpp's built-in HF downloader and then links the resolved GGUF from the HF cache into `~/models/`; `crates/ozone-inference` now supports both `koboldcpp` and `llamacpp`; and the base launcher can now launch `LlamaCpp` directly and hand that runtime off into ozone+
- Base Ozone profiling is now layer-aware inside the profiling workflow: GGUF metadata feeds GPU/CPU split recommendations, estimated RAM, and sweep seeding, while a live Pantheon 22B profiling smoke completed advisory -> confirm -> running -> saved benchmark with a 42 GPU / 14 CPU split at ctx 2048 (`8.08 tok/s`, `10498 MiB` VRAM peak)
- User-facing docs now describe the base `Profile` workflow as **autoprofiling**: a strong benchmark-backed starting point for manual GPU/CPU layer tweaking rather than a promise of final automatic tuning
- Documentation overhauled (philosophy-first pass): README.md fully restructured with philosophy/family story leading, KoboldCpp install guide, model setup guide, first-run walkthrough, autoprofiling walkthrough, ozone+ TUI reference, CPU-only mode, env vars, troubleshooting; `ozone+/README.md` Section 10 updated to reflect shipped MVP state and TUI slash command reference added; `CONTRIBUTING.md` updated for workspace structure and ozone+ crates; `ozone_plus_documentation_stack.md` Section 6 updated to reflect what is actually still pending
- Phase 7 live-test fixes are now shipped in the base `ozone` binary: model commands use consistent `✗` errors, empty/path-like model names are rejected, broken symlinks surface as `⚠ broken` in list output, `ozone model list` supports `--json`, release builds strip symbols by default, and an `oz+` symlink now selects the ozone+ tier like `ozone+`
- First-cut Ozone MCP automation now exists as `apps/ozone-mcp` + `crates/ozone-mcp`: a stdio MCP server with repo-dev tools, ozone+ session/memory/search/branch/swipe/import/export tools, temp-XDG sandbox + mock KoboldCpp helpers, and a higher-level `launcher_smoke` tool; real protocol smokes now cover `initialize`, `tools/list`, sandboxed runtime-backed send/search, and launcher invocation flow
- Ozone MCP now also has a front-door `mock_user_tool` for terminal-only scripted user journeys: it launches the real `ozone` binary in a PTY, drives named launcher/ozone+ flows with keys/text, and reports success from recent-screen terminal markers instead of repo/API back doors
- Ozone MCP now has a centralized capturable-screen navigation catalog plus target-name-driven mock-user reuse for base Ozone and ozone+ entry screens, so later screenshot tooling can navigate from cold start without duplicating flow definitions
- Ozone MCP now has a standalone `screenshot_tool` that reuses those centralized screen targets to drive front-door PTY captures and write PNG + JSON sidecars for capturable launcher / ozone+ screens
- Ozone MCP now has `screen_check_tool`, which reads those screenshot JSON sidecars and runs structured screen-grid checks (text, color, border, layout, overlap) without OCR
- Installed release binaries were resynced after the MCP/front-door testing work: `ozone`, `ozone-plus`, and `ozone-mcp` under both `~/.local/bin` and `~/.cargo/bin` now match the current `target/release` artifacts, and the installed app binaries now report `0.4.2-alpha`
- Local install sync is now checksum-aware via `./contrib/sync-local-install.sh`: it explicitly builds `ozone`, `ozone-plus`, and `ozone-mcp`, then refreshes both `~/.cargo/bin` and `~/.local/bin` only when the built SHA-256 differs from the installed copy
- Installed `ozone` and `ozone-plus` now also detect stale local installs at startup: if the synced repo's `target/release` artifact no longer matches the installed checksum, the app prompts `Update installed binaries now? [Y/n]` and can refresh/relaunch in place
- MCP screenshot artifacts were re-validated in a temp-XDG live-test pass: direct ozone+ targets still write usable PNG + JSON sidecars and `screen_check_tool` can assert against them reliably, even though the current marker-based runner can still report false failures for successful captures

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
- ~~Settings crashes in ozone+ (usize underflow, out-of-bounds index, missing Session variant)~~ — **resolved in v0.4.5-alpha**
- Theme preset system is now live (`ThemePreset` enum, `ACTIVE_PRESET` OnceLock); runtime hot-swap via `Settings > Appearance` cycle entry
- Broken `.gguf` symlinks can still appear in the catalog/list, but base CLI list views now mark them as `⚠ broken` and the picker still routes them into issue reports when selected
- Hardware guidance is still NVIDIA-centric because GPU memory detection depends on `nvidia-smi`
- Focused ozone+ tests now cover core, engine, persistence, TUI, and app-level draft restore; the older root `ozone` app still has intentionally light test coverage outside profiling helpers
- Manual swipe seeding in `ozone-plus` is still a temporary CLI helper for transcript experiments; automatic alternate-generation/swipe creation is not yet wired into the runtime
- Several `~/models/*.gguf` symlinks still point at missing Ollama blob files (`Qwen2.5-0.5B.gguf`, `Qwen2.5-Coder-7B-Q5_K_M.gguf`, and others), so the launcher's profile/catalog paths can only treat them as broken-model issue reports until the model files are repaired or removed
- The new layer-aware split currently applies to the profiling workflow only; the normal fast-launch planner still uses the older coarse heuristic until that path is deliberately upgraded
- The Phase 1E live smoke verified inline context preview with a mock backend, but the `Ctrl+D` path itself was verified through cargo tests because this PTY automation channel cannot reliably send every control chord
- The Phase 2A live smoke verified `:memories` and `/search ...` in the TUI, but `Ctrl+K` itself was validated through tests because this PTY automation channel cannot reliably send every control chord
- The Phase 2B live smoke verified CLI fallback -> `index rebuild` -> hybrid search with the mock embedding provider; the full-screen TUI hybrid-recall path is still better covered by cargo tests than PTY-driven visual automation
- `cargo build --release` at the repo root is not sufficient when you want a fresh `target/release/ozone-plus` artifact; explicitly building `-p ozone-plus` avoids stale smoke-test binaries
- PTY/script transcript captures can still show raw ANSI/control-sequence noise even after the redraw hardening pass; treat that as a capture limitation unless it is also reproducible in a normal interactive terminal
- `Clear GPU` can report success while `:11434` is still listening if Ollama is supervised outside ozone or auto-restarts; validate the port/process state instead of assuming the listener is gone
- The local `~/koboldcpp/koboldcpp` install is now a source-built wrapper and works in both CUDA and CPU modes; remaining live-smoke blockers are model-file availability/validity and PTY automation limits, not the launcher binary itself.
- llama.cpp support assumes `llama-server` and `llama-cli` are installed or exposed through `OZONE_LLAMACPP_SERVER` / `OZONE_LLAMACPP_CLI`; this environment currently does not have either binary on `PATH`, so live launch/import smokes need explicit overrides or a local install first
- First-cut `ozone-mcp` still shells out through `ozone-plus` for runtime-backed `send`, `search`, and `index rebuild`, and its launcher smoke tool relies on PTY automation; no external editor/client integration test has been run yet.
- `mock_user_tool` is intentionally terminal-only and marker-based: it can prove front-door launcher/ozone+ flows reached expected recent-screen text, but PTY captures are still noisy and not suitable for pixel-perfect assertions.
- Temp-XDG screenshot runs can lose user-site Python modules like `pyte` because the capture helper inherits the sandbox HOME; export a real-site `PYTHONPATH` or install the dependency system-wide before assuming the Rust MCP layer is broken.
- Repeated `ozone-plus handoff --launcher-session` invocations inside the same temp-XDG sandbox currently create duplicate `Launcher Session` rows instead of reusing one dedicated launcher session.
- The sandboxed MCP `base_ozone_plus_shell` / `launcher_to_ozone_plus` path still fails to hand off from base Ozone into ozone+; captures stay on the base launcher with `Backend: —  Frontend: —`, and no ozone+ session is created in that sandbox.

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

## Versioning

See [`.mex/conventions/versioning.md`](.mex/conventions/versioning.md) for full rules.

**TL;DR:** Bump PATCH at named feature-set sprints. Small changes use git hash only. `0.5.x` = first beta. PATCH can exceed 9.

## Branch Workflow

| Branch | Purpose |
|--------|---------|
| `main` | Stable releases — tagged, installable, always green |
| `dev` | Integration — all feature branches merge here first |
| `feature/*` | Individual feature work — branch off `dev`, PR back to `dev` |
| `hotfix/*` | Urgent fixes — branch off `main`, merge to both `main` and `dev` |

**Rule:** `main` only receives merges from `dev` (via PR at release time) or hotfix branches. Feature work always goes through `dev`.

## Commands

- **Dev build**: `cargo build`
- **Release build + install both binaries**: `make install` (always run after any code change)
- **Tests**: `make test` or `cargo test`

> **Rule**: After any code change, always run `make install` to update both `ozone` and `ozone-plus` locally. Never leave the installed binaries out of sync with the codebase.

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

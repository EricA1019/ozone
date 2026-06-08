# Project Audit Triage Plan

## Goal

Bring the repo to a state where startup and runtime failures are explicit, docs and automation match shipped behavior, and the remaining hub files are small enough to change safely without regressing the current green build.

## Status

- Created on 2026-05-17 from the full-project audit findings.
- Wave 0 completed on 2026-05-17.
- Wave 1 completed on 2026-05-18: README, CONTRIBUTING, ozone+ docs, CI, the release workflow, CLI help, Cargo feature comments, `.mex/ROUTER.md`, and `.mex/patterns/INDEX.md` now agree on dev-first integration, separate local family binaries, base-only GitHub Releases, and the `--features full` requirement for the installable base artifact; current diagnostics are clean for those touched docs/workflows.
- Wave 2 started on 2026-05-18.
- Wave 2 completed on 2026-05-18.
- Wave 2 progress (2026-05-18): completed small low-risk extractions to continue hub-file decomposition: moved `handle_command_overlay_key` into `src/ui/command_overlay_flow.rs`, moved `run_launcher_action` into `src/ui/launch_execution_flow.rs`, and moved terminal helper functions `find_in_path` / `spawn_in_terminal` into `src/ui/launcher.rs`. Each slice was validated with per-crate `cargo test` and `cargo clippy` and then with workspace-wide `cargo test --workspace` and `cargo clippy --workspace --all-targets -- -D warnings` — all gates passed.
- The first Wave 2 slice landed on 2026-05-18: the pure backend launch-argument builders moved out of `src/ui/mod.rs` into `src/ui/backend_args.rs`, reducing one small but real coupling point in the base launcher hub file without changing launch behavior; focused validation passed with `cargo test -p ozone build_ --quiet` and `cargo check -p ozone`.
- The second Wave 2 slice landed on 2026-05-18: the JSON-RPC framing/request-response helpers moved out of `crates/ozone-mcp/src/lib.rs` into `crates/ozone-mcp/src/jsonrpc.rs`, reducing one cohesive protocol seam in the MCP hub file without changing tool behavior; focused validation passed with `cargo test -p ozone-mcp jsonrpc_ --quiet`, and widened validation passed with `cargo clippy -p ozone-mcp --all-targets -- -D warnings`.
- The third Wave 2 slice landed on 2026-05-18: `PersistencePaths` and the default session file-content helpers moved out of `crates/ozone-persist/src/repository/mod.rs` into `crates/ozone-persist/src/repository/paths.rs`, reducing one pure path/layout seam in the repository hub file without changing persistence behavior; focused validation passed with `cargo test -p ozone-persist persistence_paths_ --quiet`, and widened validation passed with `cargo clippy -p ozone-persist --all-targets -- -D warnings`.
- The fourth Wave 2 slice landed on 2026-05-18: the repository filesystem/bootstrap helpers `open_connection`, `ensure_file_with_contents`, and `secure_path` moved out of `crates/ozone-persist/src/repository/mod.rs` into `crates/ozone-persist/src/repository/fs_helpers.rs`, reducing another pure bootstrap seam in the repository hub file without changing persistence behavior; focused validation passed with `cargo test -p ozone-persist fs_helpers_ --quiet`, and widened validation passed with `cargo clippy -p ozone-persist --all-targets -- -D warnings`.
- The fifth Wave 2 slice landed on 2026-05-18 as an autonomous `ozone-persist` helper pass: `plain_text_fts_query` moved out of `crates/ozone-persist/src/repository/mod.rs` into `crates/ozone-persist/src/repository/search_helpers.rs`, and `current_timestamp_ms` plus `generate_uuid_like` moved into `crates/ozone-persist/src/repository/generated_values.rs`; both seams stayed behavior-preserving, followed a test-first workflow, and passed the full crate gate with `cargo test -p ozone-persist --quiet` plus `cargo clippy -p ozone-persist --all-targets -- -D warnings`.
- The sixth Wave 2 slice landed on 2026-05-18: the SQLite row parsing helpers `read_conversation_message`, `read_branch_record`, `parse_sqlite_text`, `parse_i64_as_u16`, `parse_i64_as_u64`, and error formatters moved out of `crates/ozone-persist/src/repository/mod.rs` into `crates/ozone-persist/src/repository/row_parsers.rs`, reducing another pure data-conversion seam in the repository hub file without changing persistence behavior; 6 new focused unit tests were added for type conversions, behavior stayed preserve-preserved, and the full crate gate passed with `cargo test -p ozone-persist` (49 total tests) plus `cargo clippy -p ozone-persist --all-targets -- -D warnings`.
- The seventh Wave 2 slice landed on 2026-05-18: the central MCP `tools/call` dispatch in `crates/ozone-mcp/src/lib.rs` now routes through a registry-style handler table in new module `crates/ozone-mcp/src/tool_dispatch.rs`, replacing the large inline match with table-driven lookup while preserving tool behavior and unknown-tool error semantics; validation passed with `cargo test -p ozone-mcp --quiet` (21 passed, 2 ignored) and `cargo clippy -p ozone-mcp --all-targets -- -D warnings`.
- The eighth Wave 2 slice landed on 2026-05-18: launcher settings and confirm-navigation state helpers (`sync_settings_from_prefs`, `open_settings`, `open_exit_confirm`, `back_from_confirm`) moved out of `src/ui/mod.rs` into new module `src/ui/settings_flow.rs`, reducing another cohesive state-transition seam in the base UI hub without changing behavior; validation passed with `cargo test -p ozone settings_screen_syncs_from_saved_preferences --quiet`, `cargo check -p ozone`, and `cargo clippy -p ozone --all-targets -- -D warnings`.
- The ninth Wave 2 slice landed on 2026-05-18: the large inline repository test harness moved out of `crates/ozone-persist/src/repository/mod.rs` into `crates/ozone-persist/src/repository/tests.rs`, leaving the hub with only `#[cfg(test)] mod tests;` and removing another major change-collision hotspot while preserving all persistence behavior; validation passed with `cargo test -p ozone-persist --quiet` (49 passed) and `cargo clippy -p ozone-persist --all-targets -- -D warnings`.
- The tenth Wave 2 slice landed on 2026-05-18: base launcher catalog refresh/report helpers (`apply_catalog_refresh`, `apply_catalog_report`, and supporting catalog-selection/issue summarization helpers) moved out of `src/ui/mod.rs` into new module `src/ui/catalog_flow.rs`, reducing another cohesive catalog-state seam in the base UI hub while preserving behavior; validation passed with `cargo check -p ozone`, `cargo test -p ozone settings_screen_syncs_from_saved_preferences --quiet`, and `cargo clippy -p ozone --all-targets -- -D warnings`.
- The eleventh Wave 2 slice landed on 2026-05-18: base launcher command-overlay helpers (`new_command_overlay`, `overlay_supported`, `open_command_overlay`, `close_command_overlay`, selection syncing, and normalization/input flow) moved out of `src/ui/mod.rs` into new module `src/ui/command_overlay_flow.rs`, reducing another cohesive command-surface seam in the base UI hub while preserving behavior; validation passed with `cargo check -p ozone`, `cargo test -p ozone settings_screen_syncs_from_saved_preferences --quiet`, and `cargo clippy -p ozone --all-targets -- -D warnings`.
- The twelfth Wave 2 slice landed on 2026-05-18: the large inline ozone+ runtime test harness moved out of `apps/ozone-plus/src/runtime.rs` into `apps/ozone-plus/src/runtime/tests.rs`, leaving runtime orchestration in the hub while removing a major change-collision test block; validation passed with `cargo test -p ozone-plus --quiet` (`46` unit + `14` integration tests passing) and `cargo clippy -p ozone-plus --all-targets -- -D warnings`.
- The thirteenth Wave 2 slice landed on 2026-05-18: pending frontend launch execution moved out of `src/ui/mod.rs` into `src/ui/launch_execution_flow.rs`, replacing the large inline backend launch branch inside `run_launcher()` with an explicit outcome-driven helper (`Continue`/`SkipTick`/`ExitLauncher`) while preserving launch behavior and control flow semantics; validation passed with `cargo check -p ozone`, focused launcher regression `cargo test -p ozone settings_screen_syncs_from_saved_preferences --quiet`, and `cargo clippy -p ozone --all-targets -- -D warnings`.
- The fourteenth Wave 2 slice landed on 2026-05-18: configure-profile and effective-plan helpers (`build_effective_plan`, profile selection/cycle/apply/save/update/delete, and configure profile refresh/report plumbing) moved out of `src/ui/mod.rs` into `src/ui/configure_profile_flow.rs`, reducing another cohesive configure-hub seam in the base launcher hub while preserving behavior; validation passed with `cargo check -p ozone` and `cargo clippy -p ozone --all-targets -- -D warnings`.
- The fifteenth Wave 2 slice landed on 2026-05-18: configure-plan mutation/reset helpers (`adjust_configure_plan`, `reset_configure_plan`) moved out of `src/ui/mod.rs` into `src/ui/configure_plan_flow.rs`, reducing another focused configure-hub seam while preserving behavior; tests-first characterization coverage was added for context stepping, GPU-layer clamping, and reset-to-recommended behavior, and validation passed with `cargo test --workspace --quiet` plus `cargo clippy --workspace --all-targets -- -D warnings`.
- The sixteenth Wave 2 slice landed on 2026-05-18: Configure Hub key-input branching moved out of `src/ui/mod.rs` into `src/ui/configure_hub_flow.rs` via `handle_configure_hub_key`, extracting configure-screen keyboard transitions (escape/reset, profile save/load/update/delete/default, manual adjust/reset, and confirm override persistence) behind a dedicated flow module while preserving behavior; tests-first characterization coverage was added for `Esc` state reset and `Enter` override persistence paths, and validation passed with `cargo test --workspace` plus `cargo clippy --workspace --all-targets -- -D warnings`.
- The seventeenth Wave 2 slice landed on 2026-05-18: FrontendChoice key-input branching moved out of `src/ui/mod.rs` into `src/ui/frontend_choice_flow.rs` via `handle_frontend_choice_key`, extracting frontend selection transitions (`Esc` back-to-confirm, bounded up/down selection, and enter-to-pending-launch) behind a focused flow boundary while preserving behavior; tests-first characterization coverage was added for escape routing, enter pending-launch assignment, and down-key clamping, and validation passed with `cargo test --workspace --quiet` plus `cargo clippy --workspace --all-targets -- -D warnings`.
- The eighteenth Wave 2 slice landed on 2026-05-18: ModelPicker key-input branching moved out of `src/ui/mod.rs` into `src/ui/model_picker_flow.rs` via `handle_model_picker_key`, extracting filter/navigation/input selection plus launch/configure/profile routing transitions behind a dedicated flow module while preserving behavior; tests-first characterization coverage was added for filter-first escape handling, empty-filter escape return-to-launcher, and enter-to-configure-hub launch-mode behavior.
- The nineteenth Wave 2 slice landed on 2026-05-18: Confirm-screen key handling moved out of `src/ui/mod.rs` into `src/ui/confirm_flow.rs` via `handle_confirm_key`, extracting confirm back-navigation and launch-queue transitions behind a focused boundary while preserving behavior; tests-first characterization coverage was added for escape back-target routing and preferred-frontend launch queueing, and the batched validation gate for slices 18-19 passed with `cargo test --workspace --quiet` plus `cargo clippy --workspace --all-targets -- -D warnings`.
- The twentieth Wave 2 slice landed on 2026-05-18: Exit-confirm key handling moved out of `src/ui/mod.rs` into `src/ui/exit_confirm_flow.rs` via `handle_exit_confirm_key`, extracting confirm-exit navigation and exit outcome routing behind a focused boundary while preserving behavior; tests-first characterization coverage was added for escape return-to-launcher and enter-on-yes exit outcome paths.
- The twenty-first Wave 2 slice landed on 2026-05-18: Settings-screen key handling moved out of `src/ui/mod.rs` into `src/ui/settings_screen_flow.rs` via `handle_settings_key`, extracting section navigation, backend/frontend selection updates, save/discard transitions, and preferred-frontend runtime update behavior behind a dedicated flow module while preserving behavior; tests-first characterization coverage was added for enter-save and escape-discard paths, and the batched validation gate for slices 20-21 passed with `cargo test --workspace --quiet` plus `cargo clippy --workspace --all-targets -- -D warnings`.
- The twenty-second Wave 2 slice landed on 2026-05-18: TierPicker key/phase handling moved out of `src/ui/mod.rs` into `src/ui/tier_picker_flow.rs` via `handle_tier_picker_key` and `TierPickerOutcome`, extracting picking/download-confirm/install-done/install-error transitions behind a dedicated flow boundary while preserving behavior; tests-first characterization coverage was added for picking-phase quit and lite-tier enter paths.
- The twenty-third Wave 2 slice landed on 2026-05-18: Launcher-screen key handling moved out of `src/ui/mod.rs` into `src/ui/launcher_screen_flow.rs` via `handle_launcher_screen_key`, extracting launcher quit/escape/navigation/enter action transitions behind a focused flow module while preserving behavior; tests-first characterization coverage was added for launcher `q` exit and escape-to-exit-confirm behavior, and the batched validation gate for slices 22-23 passed with `cargo test --workspace --quiet` plus `cargo clippy --workspace --all-targets -- -D warnings`.
- The twenty-fourth Wave 2 slice landed on 2026-05-18: Profiling result-state key handling moved out of `src/ui/mod.rs` into `src/ui/profiling_result_flow.rs` via `handle_profile_running_key`, `handle_profile_success_key`, and `handle_profile_failure_key` with `ProfilingResultOutcome`, extracting running/success/failure transitions (including configure-hub return and advisory rebuild fallback) behind a focused flow module while preserving behavior and reducing duplication through a shared advisory-return helper; tests-first characterization coverage was added for running-phase cancellation and saved-profile success escape paths (`profile_running_q_cancels_active_workflow`, `profile_success_escape_from_saved_profile_returns_configure_hub`), default batched gates stayed green with `cargo test --workspace --quiet` plus `cargo clippy --workspace --all-targets -- -D warnings`, and profiling-ui validation passed with targeted tests plus a serial full run (`cargo test -p ozone --features profiling-ui -- --test-threads=1`).
- The twenty-fifth Wave 2 slice landed on 2026-05-18: Profiling entry-state key handling moved out of `src/ui/mod.rs` into `src/ui/profiling_entry_flow.rs` via `handle_profile_advisory_key` and `handle_profile_confirm_key`, extracting advisory selection and confirm-to-workflow transitions behind a focused flow module while preserving behavior; tests-first characterization coverage was added for advisory escape and saved-profile confirm escape (`profile_advisory_escape_returns_to_model_picker`, `profile_confirm_escape_returns_to_configure_hub_for_saved_profile_benchmark`), default batched gates passed with `cargo test --workspace --quiet` plus `cargo clippy --workspace --all-targets -- -D warnings`, and profiling-ui validation passed via focused tests plus a serial full pass (`cargo test -p ozone --features profiling-ui -- --test-threads=1`).
- The twenty-sixth Wave 2 slice landed on 2026-05-18: repeated profiling state-transition clusters in `src/ui/mod.rs` were encapsulated behind new `App` methods (`start_profile_workflow`, `reset_profile_and_open_launcher`, `open_profile_advisory`, `open_profile_failure`, `open_confirm_with_plan`, `clear_profile_success_and_open_configure_hub`, `clear_profile_failure_and_open_configure_hub`) and then applied in `src/ui/profiling_entry_flow.rs` plus `src/ui/profiling_result_flow.rs`, reducing open-coded multi-field mutations while preserving behavior; tests-first coverage was added for the new encapsulation methods (`start_profile_workflow_sets_running_state_cluster`, `reset_profile_and_open_launcher_resets_cluster_and_screen`), default batched gates passed with `cargo test --workspace --quiet` plus `cargo clippy --workspace --all-targets -- -D warnings`, and profiling-ui validation passed including serial full-suite execution (`cargo test -p ozone --features profiling-ui -- --test-threads=1`).
- The twenty-seventh Wave 2 slice landed on 2026-05-18: generation worker state types (`WorkerEvent`, `PendingGeneration`, `PendingCompletion`, `PendingReroll`, `RerollBranchMode`, `RerollSource`) were moved from `apps/ozone-plus/src/runtime.rs` into `apps/ozone-plus/src/runtime/generation.rs` to colocate generation orchestration; two tests-first characterization tests were added to `apps/ozone-plus/src/runtime/tests.rs` (`poll_generation_streams_tokens_and_sets_streaming_state`, `mark_generation_failure_sets_failed_state_midstream`); focused, crate, and workspace gates were run and passed, and `cargo clippy -p ozone-plus -- -D warnings` completed clean.
- The twenty-eighth Wave 2 slice landed on 2026-05-18: small command and helper types (`SessionCommand`, `MemoryCommand`, `SearchCommand`, `ShellCommand`, `SummarizeShellCommand`, `ThinkingCommand`, `TierBCommand`, `HooksCommand`, `SafeModeCommand`, `RecentSearchSection`) were moved out of `apps/ozone-plus/src/runtime.rs` into `apps/ozone-plus/src/runtime/types.rs` to further shrink the runtime hub file; `apps/ozone-plus/src/runtime.rs` now re-exports the moved symbols for existing child modules. Focused tests and full crate/workspace gates were executed and passed, and `cargo clippy -p ozone-plus -- -D warnings` remained clean.
- Wave 2 was reassessed against the live codebase on 2026-05-18: `apps/ozone-plus/src/runtime.rs` is now down to `435` lines and mostly acts as struct/bootstrap/delegation glue, so it is no longer the primary blocker for strict Wave 2 closure; remaining closeout work is concentrated in the last inline `Screen::Splash` / `Screen::Monitor` handling in `src/ui/mod.rs`, one more cohesive decomposition pass in `crates/ozone-mcp/src/lib.rs`, and stale `Phase1dRuntime` naming cleanup when the ozone+ runtime surface is next touched.
- The twenty-ninth Wave 2 slice landed on 2026-05-18: the remaining inline `Screen::Splash` and `Screen::Monitor` handling moved out of `src/ui/mod.rs` into `src/ui/splash_flow.rs` and `src/ui/monitor_flow.rs`, leaving the base launcher hub with orchestration while focused seam tests covered splash routing and monitor exit/back behavior; validation passed with targeted `cargo test -p ozone splash_key_ --quiet` / `cargo test -p ozone monitor_ --quiet`, plus `cargo check -p ozone` and `cargo clippy -p ozone --all-targets -- -D warnings`.
- The thirtieth Wave 2 slice landed on 2026-05-18: the large inline `ozone-mcp` test harness moved out of `crates/ozone-mcp/src/lib.rs` into `crates/ozone-mcp/src/tests.rs`, reducing another change-collision hotspot while preserving tool behavior; validation passed with `cargo test -p ozone-mcp --quiet` and `cargo clippy -p ozone-mcp --all-targets -- -D warnings`.
- The thirty-first Wave 2 slice landed on 2026-05-18: stale roadmap-era ozone+ runtime naming was cleaned up by renaming `Phase1dRuntime` to `OzonePlusRuntime` across `apps/ozone-plus/src/runtime.rs` and its callers, bringing the reduced runtime hub in line with the shipped product surface; validation passed with `cargo test -p ozone-plus --quiet`, `cargo check -p ozone-plus`, and `cargo clippy -p ozone-plus --all-targets -- -D warnings`.
- The thirty-second Wave 2 slice landed on 2026-05-18: `crates/ozone-mcp/src/lib.rs` shed sandbox creation plus mock-backend lifecycle methods into `crates/ozone-mcp/src/sandbox.rs`, and the MCP sandbox preference normalizer plus front-door journey fixtures were tightened to canonical kebab-case enum spellings so shipped-artifact release smoke stayed aligned with the base launcher contract; validation passed with `cargo test -p ozone-mcp --quiet`, `cargo clippy -p ozone-mcp --all-targets -- -D warnings`, `make preflight`, `cargo check --workspace --all-targets --release`, `cargo build --release -p ozone --features full -p ozone-plus -p ozone-mcp-app`, and `cargo test -p ozone-mcp release_smoke_gate_ -- --ignored --test-threads=1`.
- Wave 2 closeout is complete: the reassessed end-state exceptions were eliminated rather than documented, the remaining runtime naming drift is gone, and the final workspace plus shipped-artifact validation gates are green.
- Execution order is mandatory: Wave 0 before contract cleanup, contract cleanup before major refactors.

## Scope

**In**: hidden-failure paths, documented-contract drift, release and CI drift, hub-file decomposition, direct protocol debt that increases change risk.

**Out**: new product features, new backends, platform expansion, visual redesign, schema redesign beyond what is required to remove confirmed failure paths.

**Deferred**: shipping all family binaries in GitHub Releases, deep observability work beyond a coherent logging baseline, and broad ozone+ runtime cleanup outside the audited hotspots.

## Prerequisites

- Workspace validation baseline exists and is green via `make preflight`.
- Release-style compile validation exists and is green via `cargo check --workspace --all-targets --release`.
- Focused regression surfaces already exist for install update flow, ozone+ CLI, and release smoke.
- No blocker remains from the audit; the remaining work is sequencing, not discovery.

## Principles

- Fix hidden failures before refactoring structure.
- Align docs, CI, and release behavior before widening scope.
- Prefer bounded extractions over broad rewrites.
- Keep the current green build green after every wave.
- Separate confirmed defects from desirable follow-up cleanup.

## Wave 0: Diagnostics And Safety Hardening

### Wave 0 Objective

Stop swallowing real failures in startup, preferences, catalog side files, database setup, and install-update trust boundaries.

### Wave 0 Tasks

1. Replace silent defaulting in base preferences loading.
   - Touch [src/prefs.rs](src/prefs.rs) and [src/ui/mod.rs](src/ui/mod.rs).
   - Parse and load failures must surface actionable diagnostics instead of collapsing to `Preferences::default()`.
2. Replace silent defaulting in launcher catalog bootstrap.
   - Touch [src/catalog.rs](src/catalog.rs) and [src/ui/mod.rs](src/ui/mod.rs).
   - Missing or invalid preset and benchmark side files must not look like a successful empty catalog state.
3. Remove confirmed production panic paths.
   - Touch [src/db.rs](src/db.rs) and [apps/ozone-plus/src/store.rs](apps/ozone-plus/src/store.rs).
   - Replace `expect`-backed runtime assumptions with propagated `Result` handling.
4. Tighten the install-update trust boundary.
   - Touch [crates/ozone-core/src/install.rs](crates/ozone-core/src/install.rs).
   - Refuse cwd-discovered repos for executable sync; trust only the recorded install source root.
5. Add focused regressions for the new failure behavior.
   - Touch [src/ui/mod.rs](src/ui/mod.rs), [apps/ozone-plus/tests/cli_tests.rs](apps/ozone-plus/tests/cli_tests.rs), and [crates/ozone-core/src/install.rs](crates/ozone-core/src/install.rs).

### Wave 0 Exit Gate

- Invalid prefs produce an explicit user-visible error.
- Invalid catalog side files produce an explicit diagnostic path.
- The audited production `expect` sites are gone from the touched files.
- `make preflight` and `cargo check --workspace --all-targets --release` stay green.

## Wave 1: Contract Alignment

### Wave 1 Objective

Make the code, docs, CI, and release automation tell the same story.

### Wave 1 Tasks

1. Fix backend-support contradictions.
   - Align [README.md](README.md), [CONTRIBUTING.md](CONTRIBUTING.md), and [ozone+/README.md](ozone+/README.md).
   - llama.cpp support must be described consistently.
2. Lock one release contract.
   - Align [README.md](README.md), [contrib/sync-local-install.sh](contrib/sync-local-install.sh), and [.github/workflows/release.yml](.github/workflows/release.yml).
   - Explicitly document that GitHub Releases package base ozone only, while local install sync provides the family binaries.
3. Align CI with the documented branch workflow.
   - Align [.github/workflows/ci.yml](.github/workflows/ci.yml) with [.mex/ROUTER.md](.mex/ROUTER.md).
   - The repo should validate the documented `dev` integration path instead of describing a flow automation does not enforce.
4. Clean product-doc markdown debt.
   - Fix malformed tables and markdown-lint drift in [README.md](README.md), [ozone+/README.md](ozone+/README.md), [.mex/ROUTER.md](.mex/ROUTER.md), and [.mex/patterns/INDEX.md](.mex/patterns/INDEX.md).
5. Make build-surface expectations explicit.
   - Align [Cargo.toml](Cargo.toml), [src/main.rs](src/main.rs), and [README.md](README.md).
   - The default source build must not be presented as equivalent to the installed full build if feature gating keeps the command surface smaller.

### Wave 1 Exit Gate

- Help text, README, contributor guidance, CI behavior, and release packaging all agree.
- Markdown diagnostics are cleared for the touched docs.

## Wave 2: Hub-File Decomposition

### Wave 2 Objective

Reduce the files that currently act as change magnets and force unrelated edits to collide.

### Wave 2 Concrete End Goal

Wave 2 is complete when hub-file risk is materially reduced and new logic no longer defaults to the current change magnets.

Concrete end state:

1. `src/ui/mod.rs` is primarily orchestration and delegates screen-specific key handling plus cohesive helper clusters to focused child modules.
2. `src/ui/mod.rs` mutable state transitions are narrowed behind `App` methods for repeated multi-field updates.
3. `apps/ozone-plus/src/runtime.rs` is reduced to bootstrap/delegation scale, with stale roadmap-era naming cleanup applied where it is still touched.
4. Wave 2 touched surfaces are fully green at workspace level: `cargo test --workspace` and `cargo clippy --workspace --all-targets -- -D warnings`.
5. Wave 2 progress and closure are reflected in this plan and `.mex/ROUTER.md`.

### Wave 2 Tasks

1. Complete remaining screen-input extraction from `src/ui/mod.rs` into focused flow modules.
2. Extract `Screen::Launcher` key handling into a dedicated flow module.
3. Extract `Screen::Settings` key handling into a dedicated flow module.
4. Extract `Screen::ExitConfirm` key handling into a dedicated flow module.
5. Extract `Screen::TierPicker` phase key handling into a dedicated flow module.
6. Split any remaining oversized profiling key-handling branches (`ProfileAdvisory`, `ProfileConfirm`, `ProfileRunning`, `ProfileSuccess`, `ProfileFailure`) if still acting as inline hotspots.
7. Encapsulate repeated multi-field state transitions in `src/ui/mod.rs` behind `App` methods.
8. Replace direct field-mutation clusters in event handling with those `App` methods where practical.
9. Continue runtime hotspot decomposition in `apps/ozone-plus/src/runtime.rs` using cohesive seam extraction.
10. Apply stale roadmap-era naming cleanup in `apps/ozone-plus/src/runtime.rs` when touched by decomposition work.
11. Keep tests-first seam coverage for each new extraction, then validate with focused tests before batched workspace gates.
12. Update plan/router tracking after each landed Wave 2 batch.

### Wave 2 Remaining Slices (Closed 2026-05-18)

1. None. The reassessed closeout items were completed: `Screen::Splash` / `Screen::Monitor` delegation, one more cohesive `ozone-mcp` decomposition pass, runtime naming cleanup, tracking-doc refresh, and final workspace/release validation.

### Wave 2 Exit Gate

- New behavior no longer defaults to landing in `src/ui/mod.rs`, `crates/ozone-mcp/src/lib.rs`, or `crates/ozone-persist/src/repository/mod.rs`.
- Remaining `src/ui/mod.rs` screen branches are delegated to focused child modules, with orchestration retained in the hub.
- Repeated `App` state-transition clusters in `src/ui/mod.rs` are method-driven instead of open-coded in many branches.
- Runtime hotspot reduction for `apps/ozone-plus/src/runtime.rs` has advanced far enough that the parent hub is mostly struct/bootstrap/delegation glue, and stale roadmap-era naming cleanup is complete.
- Workspace validation is green at end-of-wave: `cargo test --workspace` and `cargo clippy --workspace --all-targets -- -D warnings`.

## Wave 3: Protocol Debt Cleanup

### Wave 3 Objective

Remove the remaining debt that will keep reintroducing drift after the big issues are fixed.

### Wave 3 Tasks

1. Consolidate duplicated preference-loading policy between base ozone and ozone+.
2. Consolidate duplicated hardware probing logic between base ozone and ozone-tui.
3. Replace broad Clippy suppressions with better-shaped request and state types in the audited hotspots.
4. Establish one coherent structured-logging path for startup, load, inference, and install-update failures.

### Wave 3 Exit Gate

- Duplicated prefs and hardware logic are gone or intentionally centralized.
- The touched suppression sites are either removed or reduced to narrow justified cases.
- Important runtime failures produce durable diagnostic context instead of temporary UI status text.

## Validation Cadence

- After each task slice: run the narrowest relevant tests first.
- After each wave: run `make preflight` and `cargo check --workspace --all-targets --release`.
- After Waves 1 and 2: rerun the ignored release smoke tests in [crates/ozone-mcp/src/lib.rs](crates/ozone-mcp/src/lib.rs) when touched surfaces overlap release behavior.

## Done When

- Bad prefs, bad catalog side files, DB path failures, and install-update trust violations are explicit and tested.
- Docs, CI, and release automation describe the same actual product behavior.
- The main hub files are no longer the default place to add new work.
- The repo remains green at the workspace level after each wave, not just at the end.

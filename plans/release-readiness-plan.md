# Release Readiness Plan

## Goal
A fresh Linux user can install Ozone, add or list a model, launch base ozone or ozone+, send the first prompt, create and retrieve a memory, and follow the docs without hitting contradictory commands, missing features, or release-only regressions.

## Status
- P0 validated complete on 2026-05-12.
- P1 validated complete on 2026-05-12 after ozone+ compact-footer/copy/command-surface polish, the `ozone-plus open` conversation-bootstrap fix, `cargo test --workspace --quiet`, `cargo clippy --workspace --all-targets -- -D warnings`, and a live temp-XDG `ozone-plus open/help/memories` smoke.
- Latest canonical validation: `make release-gates` passed, including workspace clippy/test, install parity, fresh temp-XDG release smoke with sidecar-backed launcher, base Settings, base confirm-launch, and base frontend-choice screen checks, and existing-user release smoke.
- P2 validated complete on 2026-05-12 after the final ozone+ generation-control extraction, the focused `cancel_generation_strips_hidden_thinking_blocks_from_partial_message` regression, `cargo test -p ozone-plus --quiet`, `cargo clippy -p ozone-plus --all-targets -- -D warnings`, and a rerun `make release-gates` after the expected local install-parity resync.
- Release-readiness hardening is now complete for this plan; any deeper structural cleanup is deferred follow-up rather than active release work.
- P2 runtime decomposition completed on 2026-05-12 with the final ozone+ generation-control extraction: worker startup plus `send_draft` / `poll_generation` / `cancel_generation` delegation now live in `apps/ozone-plus/src/runtime/generation.rs` beside completion/session-title helpers; `runtime.rs` dropped from `3209` to `1379` lines, `generation.rs` is `569` lines, the new cancellation regression is green, and `cargo test -p ozone-plus --quiet`, `cargo clippy -p ozone-plus --all-targets -- -D warnings`, and the rerun `make release-gates` all passed.
- P2 planner parity also advanced on 2026-05-12: base fast launch now reuses GGUF topology inspection for layer counts when metadata is available, closing the old size-heuristic gap between default launch and profiling without changing profiling-only adaptation rules; `fast_launch_uses_metadata_layers` was added, `cargo test -p ozone --features full --quiet`, `cargo clippy -p ozone --features full --all-targets -- -D warnings`, and the rerun `make release-gates` all passed after the expected local install resync.
- P2 accessibility/layout also advanced on 2026-05-12: ozone+ help-overlay and toast surfaces now use preset-aware theme helpers instead of hard-coded low-contrast colors, so `HighContrast` and the other presets finally affect those overlays; `cargo test -p ozone-tui --quiet`, `cargo clippy -p ozone-tui --all-targets -- -D warnings`, and the rerun `make release-gates` all passed after the expected install-parity resync.
- P2 front-door automation also advanced on 2026-05-12: the fresh temp-XDG release smoke now captures final sidecars for the shipped base launcher, the shipped base Settings path, the shipped base confirm-launch path, and the shipped base frontend-choice path, with the settings journey routed through the launcher quick-command overlay instead of brittle arrow navigation; `screen_check_tool` now validates all four screens, the focused ignored test passed, and the rerun `make release-gates` also passed.

## Scope
**In**: shipped feature-matrix alignment, install and release artifact correctness, first-run and first-session UX, targeted base ozone and ozone+ UX hardening, release-smoke automation, documentation truthfulness, and low-risk stabilization only.

**Out**: group chat, new backend support, Windows or macOS support, full monolith decomposition, full planner redesign, and a whole-product visual redesign.

**Deferred**: deep runtime decomposition, full compact-layout redesign, full command-surface rewrite, pixel-perfect UI automation, and moving the normal fast-launch path to the full layer-aware planner.

## Principles
- Truth over aspiration: shipped binaries, docs, help text, and errors must tell the same story.
- Degrade over delay: hide, narrow, or reword incomplete features instead of shipping half-finished affordances.
- Installed artifacts are the source of truth: release validation must use installed or release binaries, not only debug builds.
- Freeze structural work early: broad refactors stop as soon as the shipping contract is locked.

## P0 Ship Blockers
1. Lock the shipping product matrix.
   - Decide exactly what `ozone`, `ozonelite`, and `ozone+` install and release artifacts expose.
   - Align `Cargo.toml`, `Makefile`, install scripts, and release automation.
2. Make installed and release artifacts match the documented command surface.
   - Validate `ozone` install path exposes the commands the README promises.
   - Add artifact-parity checks so this cannot drift again.
3. Fix the first-model path.
   - Empty-state model flows must never point to nonexistent commands.
   - If model management is unavailable in a given tier, say so explicitly and route the user to the supported install path.
4. Fix first-chat backend guidance.
   - `ozone+` first-send failure messages, CLI help, and docs must consistently describe supported backends and next steps.
5. Fix first-memory vocabulary.
   - Distinguish pinned memories, note memories, and searchable memories in help text, command output, and docs.
6. Add release gates.
   - Fresh temp-XDG user path.
   - Existing-user data path.
   - Installed vs release artifact parity.
   - Workspace test and clippy gates.

## P1 Release Polish
1. Reduce compact-layout information loss in ozone+.
2. Simplify hint bars and high-density first-use copy.
3. Unify overlay exit language and empty-state next actions.
4. Finish or hide unfinished editing affordances such as clipboard actions.
5. Add command-palette and slash-surface ranking and copy polish.

## P2 Post-Release Hardening
1. Decompose runtime and other hotspot files in behavior-preserving slices.
   - 2026-05-12 progress: ozone+ settings/session/character management helpers moved into `runtime/management.rs`, shrinking `runtime.rs` further to `2034` lines without changing the `SessionRuntime` surface.
   - 2026-05-12 progress: ozone+ shell-command execution moved into `runtime/commands.rs`, shrinking `runtime.rs` further to `1731` lines while keeping the `SessionRuntime` impl as a delegating surface.
   - 2026-05-12 progress: ozone+ bookmark/pin/edit helpers moved into `runtime/message_actions.rs`, shrinking `runtime.rs` further to `1669` lines while preserving the same TUI action/status behavior.
   - 2026-05-12 progress: ozone+ generation worker startup plus `send_draft` / `poll_generation` / `cancel_generation` control flow moved into `runtime/generation.rs`, shrinking `runtime.rs` further to `1379` lines; `cancel_generation_strips_hidden_thinking_blocks_from_partial_message` now guards the cancellation path against leaking hidden thinking text.
2. Improve planner parity between profiling and default launch.
3. Expand accessibility and layout work beyond current theme presets.
   - 2026-05-12 progress: ozone+ help-overlay and toast surfaces now honor the active theme preset instead of hard-coded low-contrast colors.
4. Upgrade front-door automation from marker-heavy smoke toward stronger visual validation.
   - 2026-05-12 progress: fresh-user release smoke now captures the shipped base launcher, base Settings, base confirm-launch, and base frontend-choice surfaces and runs `screen_check_tool` assertions against all four final screen sidecars.

## Release Gates
- Installed and release binaries expose the same documented commands.
- Fresh-user and existing-user paths both succeed on core flows.
- No user-facing help or docs reference missing or unsupported flows.
- No visible TODO behavior remains in user-facing interactions.
- `cargo test --workspace` and `cargo clippy --workspace --all-targets -- -D warnings` pass.
- Front-door release smoke passes on the actual shipped artifacts.

## Done When
- A new user can follow the README without hitting a nonexistent command or contradictory path.
- Base ozone and ozone+ first-run flows are coherent in success and failure cases.
- Release validation uses the same artifacts users actually run.
- Remaining structural cleanup is explicitly deferred, not mixed into the release candidate.

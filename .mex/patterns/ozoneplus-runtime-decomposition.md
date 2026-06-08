---
name: ozoneplus-runtime-decomposition
description: Decomposing large ozone+ runtime hotspots into child modules in behavior-preserving slices.
triggers:
  - "runtime decomposition"
  - "phase 2"
  - "refactor runtime"
  - "split runtime.rs"
  - "behavior-preserving extraction"
edges:
  - target: "../context/architecture.md"
    condition: when deciding whether logic belongs in apps/ozone-plus runtime, ozone-tui, or a shared crate
  - target: "../context/conventions.md"
    condition: before moving code across module boundaries or tightening verification scope
  - target: "ozoneplus-streaming-backend-runtime.md"
    condition: when the extraction touches live backend workers, adapters, or streaming control flow
last_updated: 2026-05-12
---

# Ozone+ Runtime Decomposition

## Context

- `apps/ozone-plus/src/runtime.rs` is a hotspot file and should be decomposed in small, behavior-preserving slices rather than broad rewrites.
- Prefer child modules under `apps/ozone-plus/src/runtime/` that keep `impl Phase1dRuntime` methods close to the owning app runtime instead of prematurely pushing logic into new crates.
- Existing seams already extracted this way include `shell_commands.rs`, `recall_helpers.rs`, `bootstrap.rs`, `reroll.rs`, `context.rs`, `generation.rs`, `management.rs`, `commands.rs`, and `message_actions.rs`; `generation.rs` now owns worker startup, `send_draft` / `poll_generation` / `cancel_generation` delegation, completion/failure handling, and session-title helpers.
- The safest first-class validation anchors are the narrow seam tests when they exist (`load_session_into_tui...`, reroll-focused tests), then the `ozone-plus` crate suite before widening to the workspace.

## Steps

1. Find one cohesive seam that is already clustered in `runtime.rs`.
  - Good candidates: bootstrap/session-loading helpers, shell-command parsing, recall formatting helpers, the settings/session/character management cluster, large `SessionRuntime` command/action handlers, and small contiguous message-action helpers like bookmark/pin/edit that share the same session-refresh contract.
  - Bad first candidates: mixed worker/thread/control-flow code that crosses multiple responsibilities before a real seam exists.
  - Once a generation seam already exists, keep worker startup, token draining, `send_draft` / `poll_generation` / `cancel_generation`, and completion/failure helpers together in that module instead of splitting token handling across parent and child files.
2. Create a child module in `apps/ozone-plus/src/runtime/` and move only that seam.
   - Keep methods on `impl Phase1dRuntime` when they still need runtime fields.
   - Move helper structs with the seam when they are not broadly shared.
  - If the moved code currently lives inside the `SessionRuntime` trait impl, keep the trait impl in `runtime.rs` and delegate into `pub(super)` helpers in the child module instead of splitting the trait impl itself across files.
3. Keep the extraction behavior-preserving.
   - Do not rename user-facing commands, change runtime messages, or mix feature work into the refactor slice.
   - If the parent module still reads fields from a moved helper struct, expose only the minimum `pub(super)` surface.
4. Validate immediately with the narrowest existing tests for that seam.
   - For bootstrap/session-loading work, start with the direct runtime bootstrap test and persisted-draft restore coverage.
5. If the narrow tests pass, widen validation to the crate and then the workspace.

## Gotchas

- Child modules need the relevant traits in scope explicitly. Moving methods that call `engine.process(...)` or `store().list_branches(...)` requires importing `ConversationEngine` and/or `ConversationStore` in the child module.
- Imports that were only used by the moved seam can become unused in `runtime.rs`; trim them immediately so clippy stays clean.
- Management-style slices usually need explicit `ozone_tui` entry/detail types and `ozone_persist` request/id types in the child module, even when those were previously only imported once at the parent level.
- Runtime tests that use `use super::*;` can accidentally depend on parent-module helper imports. When a moved seam takes `parse_*` or recall helpers with it, switch the tests to explicit module paths instead of keeping unused production imports alive for the test module.
- When moving context-building helpers, check whether `runtime.rs` still uses `HybridSearchService` in command handlers before dropping that import; the parent runtime still owns search-command execution even after the context builders move.
- If both `poll_generation` and `cancel_generation` drain worker tokens, route both through the same `ThinkingBlockDecoder` helper; otherwise hidden or assisted thinking can leak into partial assistant text on cancellation.
- Keep extractions local to `apps/ozone-plus` unless a boundary is already proven stable. Moving app runtime code into shared crates too early increases blast radius.
- Preserve the cheapest regression tests for the seam before widening validation; they are the fastest falsifier when the move breaks visibility or trait scope.

## Verify

- Narrow seam tests first, for example:
  - `cargo test -p ozone-plus cancel_generation_strips_hidden_thinking_blocks_from_partial_message --quiet`
  - `cargo test -p ozone-plus load_session_into_tui_boots_directly_into_conversation --quiet`
  - `cargo test -p ozone-plus phase1d_runtime_restores_persisted_draft_on_bootstrap --quiet`
  - `cargo test -p ozone-plus run_command_session_rename_updates_session_metadata --quiet`
  - `cargo test -p ozone-plus toggle_bookmark_updates_bookmark_state_and_status --quiet`
- Then widen to:
  - `cargo test -p ozone-plus --quiet`
  - `cargo clippy -p ozone-plus --all-targets -- -D warnings`
  - `cargo test --workspace --quiet`
  - `cargo clippy --workspace --all-targets -- -D warnings`

## Debug

- If the extraction compiles in the parent module but fails in the child module, check missing trait imports before suspecting logic changes.
- If moved helper structs cause visibility errors, tighten them with `pub(super)` fields rather than moving unrelated call sites.
- If runtime behavior regresses after a move, step back to the smallest seam-specific test before running the whole workspace again.

## Update Scaffold

- [ ] Update `.mex/ROUTER.md` when a new runtime decomposition slice lands or a hotspot meaningfully shrinks
- [ ] Update `plans/release-readiness-plan.md` when P2 progress changes materially
- [ ] Add this pattern to `.mex/patterns/INDEX.md` if it is new
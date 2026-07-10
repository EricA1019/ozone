---
name: ozoneplus-phase2a-memory-foundations
description: Pattern for Phase 2A manual retrieval foundations — ozone-memory types, pinned-memory persistence, FTS recall, context inclusion, and the `memory` / `search` / `:memories` surfaces.
applies_to: ["crates/ozone-memory/src/*", "crates/ozone-persist/src/repository.rs", "apps/ozone-plus/src/main.rs", "apps/ozone-plus/src/runtime.rs", "apps/ozone-plus/src/context_bridge.rs", "crates/ozone-tui/src/*"]
---

# Phase 2A: Manual Retrieval Foundations

## Storage Reuse First

Do **not** add new Phase 2A tables unless the existing schema is truly insufficient.

Use the foundations that already exist:
- `memory_artifacts` for pinned memories and freeform note memories
- `session_search` + `session_search_fts` for global cross-session keyword recall
- `search_messages(session_id, query)` for session-local FTS5 recall

For pinned memories, store `kind = 'pinned_memory'` and serialize `ozone_memory::MemoryContent`.

## Expiry Handling

Use `snapshot_version` as the message-count snapshot at creation time.

Compute expiry from:
- `current_message_count - snapshot_version`
- `expires_after_turns`

Phase 2A behavior:
- **active** pinned memories participate in context
- **expired** pinned memories remain inspectable/listable
- unpin removes the artifact

## Query Normalization

SQLite FTS5 treats hyphens and some punctuation as operators or column syntax.

Before calling FTS-backed search APIs with raw user text:
- split to plain terms
- quote/escape them as literal text terms
- join back into a safe FTS query

This avoids bugs like:
- `database error: no such column: phase2a`

## CLI / Shell Surfaces

Phase 2A surfaces should remain explicit and inspectable:

CLI:
- `ozone-plus memory pin <session-id> <message-id> [--expires-after-turns N]`
- `ozone-plus memory note <session-id> <text> [--expires-after-turns N]`
- `ozone-plus memory list <session-id>`
- `ozone-plus memory unpin <session-id> <artifact-id>`
- `ozone-plus search session <session-id> <query>`
- `ozone-plus search global <query>`

TUI / runtime:
- `Ctrl+K` toggles pin/unpin for the selected persisted transcript message
- `/memory ...`
- `/search ...`
- `:memories` shortcut for the recall browser

Vocabulary rules:
- pinned-message memories and note memories are both **saved memories** for browse/list/removal surfaces
- pinning an existing message is separate from creating a freeform note memory
- `/search session|global` should describe messages plus saved-memory text, not just transcript text

## Presentation Rule

Reuse the existing inspector/status surfaces instead of inventing a new pane for Phase 2A.

The recall browser should show:
- active saved memories (pins + notes)
- expired saved memories (if any)
- session/global search hits
- enough metadata to preserve provenance and session visibility

## Context Integration

Phase 2A is not just storage — pinned memories must affect generation.

Minimum acceptable behavior:
- active pinned-message memories appear in context preview lines/summary
- active pinned-message memories are injected into the generation prompt
- note memories remain searchable/retrievable on demand instead of being injected every turn
- expired pinned-message memories are excluded from generation

Use `ContextLayerKind::PinnedMemory` where possible instead of inventing a parallel concept.

## Validation

Required validation for this pattern:
- `cargo test -p ozone-memory --quiet`
- `cargo test -p ozone-persist --quiet`
- `cargo test -p ozone-tui --quiet`
- `cargo test -p ozone-plus --quiet`
- `cargo check -p ozone-plus --quiet`
- `cargo clippy -p ozone-memory --all-targets -- -D warnings`
- `cargo clippy -p ozone-persist --all-targets -- -D warnings`
- `cargo clippy -p ozone-tui --all-targets -- -D warnings`
- `cargo clippy -p ozone-plus --all-targets -- -D warnings`

Smoke expectations:
- temp-XDG CLI flow for pin/list/unpin + local/global search
- best-effort TUI smoke for `:memories` and `/search ...`

## PTY Limitation

This automation channel cannot reliably send every control chord.

In practice:
- `Ctrl+K` should be verified through tests
- `:memories` and `/search ...` are better live-smoke targets for PTY validation

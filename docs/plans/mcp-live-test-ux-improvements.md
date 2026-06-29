# Plan: MCP Live Test UX Improvements

**Started**: 2026-04-27
**Status**: Proposed
**Branch**: `dev` (17 commits ahead of main)
**Philosophy**: Local-first · Efficiency · Transparency · User agency
**Scope**: ozone-mcp (developer stdio tool), ozone-persist (data layer), ozone-tui (TUI surface)

---

## Context

After a live end-to-end test of the ozone system via MCP — model loaded via KoboldCpp, full conversation turn, multi-turn context, search, branch creation, character import, memory ops — the following friction points were identified.

The MCP server is a **developer-facing stdio bridge** (`crates/ozone-mcp`) that proxies to `ozone-plus` CLI for runtime-backed operations and uses direct crate APIs for persistence-heavy session work. It is not part of the end-user TUI flow.

ozone+'s core UX principles are:
- **Explicit and transparent memory** — nothing is retrieved without the user seeing it
- **Sessions are isolated** — character A in session 1 never leaks into session 2
- **User agency** — no hidden magic; every decision is overridable
- **Local-first** — no cloud dependency

All fixes must honor these principles. Changes that silently suppress, hide, or auto-correct user data betray the transparency contract.

---

## Non-Negotiables (per CLAUDE.md behavioral contract)

1. Never write database queries outside the repository layer
2. Never commit with clippy warnings or failing tests — run `make preflight` before every commit
3. ozone-persist tests must remain fully passing (34/34)
4. ozone-tui tests must remain fully passing (175/175)
5. All new patterns added to `.mex/patterns/`
6. After every task: update `.mex/ROUTER.md` project state if anything changed

---

## P0 — Breaks Real Workflows

### P0-1: `<|im_end|>` Token Leaks into Stored Messages

**Severity**: Data corruption
**Files**: `crates/ozone-persist/src/repository/message_ops.rs`

**Root cause**: The inference layer writes raw model output directly to storage without stripping the end-of-generation token (`<|im_end|>`). This means every assistant message in the database contains a trailing `<|im_end|>` that gets included in context on the next turn and in exports.

**What it means in practice**:
- Context pollution: each turn that includes a prior assistant message is bloated with the token
- Exports (JSON/markdown) contain the raw token
- Search FTS indexes the token as content
- Reroll context rebuilding could double the token

**Fix location**: `insert_message` in `message_ops.rs` or the caller in the inference layer. The assistant content should be stripped before storage:
```rust
pub fn insert_message(...) -> Result<MessageId> {
    let content = content
        .replace("<|im_end|>", "")
        .replace("<|im_eos_id|>", "");
    // ... rest of insertion
}
```

**Verification**:
```bash
# After fix, exports should not contain the token
ozone-plus export <session-id> --format text | grep -c "<|im_end|>"  # must be 0
# Or via MCP:
cargo test --workspace  # must pass
```

**Tasks**: 1
**Pattern**: `fix-*.md` (patch-based fix in data layer)

---

### P0-2: SillyTavern Character Cards Fail to Import (example_dialogue Array)

**Severity**: Blocks real workflow — users can't import their existing character collections
**Files**: `crates/ozone-persist/src/import_export.rs`, `crates/ozone-mcp/src/lib.rs`

**Root cause**: ozone's `CharacterCard::from_json_str` requires `example_dialogue` to be a string, but SillyTavern V2 format exports it as an array of strings. The import fails with `"character card field 'example_dialogue' must be a string"` before any data is stored.

**What it means in practice**:
- Users with SillyTavern character libraries (the primary source of character cards) cannot import any of them
- The only way to get a character into ozone is via JSON hand-written in ozone's format
- This is a major blocker for adoption

**Fix**: Extend the `string_field` helper to also accept `Vec<String>` and collapse/join the array. In `import_export.rs`, add a new field parser:

```rust
// In string_field or a new example_dialogue_field function:
// Accept both single-string and array formats
match value {
    Some(Value::String(text)) => string_from_text(text),
    Some(Value::Array(arr)) => {
        // Join non-empty strings with "\n" to approximate the SillyTavern UX
        let joined: Vec<&str> = arr.iter()
            .filter_map(|v| v.as_str())
            .filter(|s| !s.trim().is_empty())
            .collect();
        if joined.is_empty() {
            Ok(None)
        } else {
            Ok(Some(joined.join("\n")))
        }
    }
    Some(_) => Err(...)
    None => Ok(None),
}
```

Alternative (simpler): Use `serde_json::from_str` with a field that accepts both:
```rust
#[serde(deserialize_with = "example_dialogue_from_value")]
pub example_dialogue: Option<String>,
```

**Verification**:
```bash
# Find a SillyTavern character card and verify it imports
ozone-plus import-character ~/path/to/SillyTavern/card.json
# Via MCP:
# The Daemon Sand card in SillyTaven data should import successfully
```

**Tasks**: 1
**Pattern**: `data-layer-extension.md`

---

### P0-3: Greeting Not Injected When Loading Existing Session (T2/T3 gap)

**Severity**: Partial feature — T2/T3 (greeting seeding) works for `import_card` but not for session reload
**Files**: `apps/ozone-plus/src/runtime.rs`, `crates/ozone-mcp/src/lib.rs`

**Root cause**: The T2/T3 fix (`seed_greeting_if_present`) is wired into `load_session_into_tui()` in `runtime.rs`. The MCP `session_tool` does not call `load_session_into_tui()` — it uses direct repository APIs. So when MCP loads a session, the greeting is not seeded. The Aisha Alfahim session (created 2026-04-19) still has "Hello" as its first message instead of Aisha's greeting.

**What it means in practice**:
- Character sessions created via MCP have no greeting
- Re-opening an old session in the TUI (which calls `load_session_into_tui`) works fine
- MCP users can't test or use the greeting feature

**Fix**: The MCP server should NOT be in the business of reimplementing session loading. The `seed_greeting_if_present` function should be moved to a shareable helper that both the TUI runtime and MCP can call. Two approaches:

**Approach A** (recommended): Make `seed_greeting_if_present` a method on `RuntimeAccess` or a free function in `ozone_engine`, then call it from both `runtime.rs` and `ozone-mcp/src/lib.rs`.

**Approach B**: MCP session_tool's `load` action calls `load_session_into_tui` semantics without starting the TUI.

The core insight: the greeting seeding is a business rule. It belongs in the engine/persist layer, not in the TUI layer.

**Verification**:
```bash
# Create a new session via MCP with a greeting character
# First message should be the character greeting, not from user
# Check via export_tool — first message should be assistant with greeting text
```

**Tasks**: 2 (1 for refactor, 1 for MCP wiring)
**Pattern**: `business-logic-extraction.md`

---

## P1 — Causes Confusion

### P1-4: `memory_tool list` Only Returns Pinned Memories (not note memories)

**Severity**: Users can't see their note memories via MCP
**Files**: `crates/ozone-mcp/src/lib.rs`

**Root cause**: QW9 added `list_note_memories()` to `ozone-persist` repository, but the MCP `memory_tool` list action only calls `list_pinned_memories()`. Note memories are completely invisible to MCP callers.

**Fix**: Add `memory_tool list` action that accepts a `kind` filter:
```
memory_tool.list(kind: "note" | "pinned" | "all")
```
The MCP layer calls both `list_pinned_memories()` and `list_note_memories()` and merges results with kind tagging.

**Verification**: Note a memory via `memory_tool note`, then list it via `memory_tool list(kind: "note")` — should appear.

**Tasks**: 1
**Pattern**: `mcp-tool-extension.md`

---

### P1-5: `last_message_preview` Not in MCP Session List

**Severity**: MCP users can't see the last message preview without exporting
**Files**: `crates/ozone-mcp/src/lib.rs`

**Root cause**: QW5 added `last_message_preview` to the TUI `SessionListEntry` struct but the MCP `session_tool list` returns a narrower struct.

**Fix**: Add `last_message_preview: Option<String>` to the MCP `session_list_entry` response struct. Populated from the latest assistant message in the transcript.

**Verification**: `session_tool list` returns `last_message_preview` with actual content.

**Tasks**: 1
**Pattern**: `mcp-tool-extension.md`

---

### P1-6: `search_tool` Error Message "embedding.provider is disabled" Is Cryptic

**Severity**: Users see an error without understanding what it means or what to do
**Files**: `crates/ozone-mcp/src/lib.rs`

**Root cause**: When vector search is unavailable, the code falls back to FTS but logs (or returns) the embedding provider disabled message, which is a debug/internal message not meant for users.

**Fix**: Replace the error message with a user-facing message:
```
"Vector search unavailable — no embedding provider configured. Falling back to keyword text search."
```
This matches the transparency principle: the user knows what's happening and why, and that the feature is still working (just with FTS instead of vector).

**Verification**: `search_tool` with vector provider disabled returns the new message and FTS results.

**Tasks**: 1
**Pattern**: `user-facing-error.md`

---

### P1-7: `swipe_tool` Has No `send` Action — Misleading Name

**Severity**: Naming misleading; users expect swipe to generate alternatives
**Files**: `crates/ozone-mcp/src/lib.rs`, README.md

**Root cause**: `swipe_tool` manages swipe alternatives (create candidates, activate, list) but the naming implies it generates alternatives. The actual generation of alternatives is done via `message_tool send` with `parentMessageId`.

**Fix**:
1. Rename `swipe_tool` to `swipe_alternative_tool` in MCP to set correct expectations
2. Add a `/swipe generate` CLI command to ozone-plus that generates N alternatives for a given message
3. Add documentation in MCP tool descriptions that alternatives are generated via `message_tool send` with `parentMessageId` to branch from

**Verification**: Tool name change is reflected in `tools/list`, and the description accurately describes the capability.

**Tasks**: 2
**Pattern**: `renaming-and-deprecation.md`

---

## P1 — Additional CRUD Gaps (Not in Original Plan)

### P1-8: `session_tool` Has No `rename` or `delete` Action

**Severity**: Basic CRUD incomplete — users cannot rename or delete sessions via MCP. Test sessions accumulate with no cleanup path.
**Files**: `crates/ozone-mcp/src/lib.rs`

**Root cause**: `session_tool` has `create`, `list`, `metadata`, `transcript` — but no `rename` or `delete`. This is the most basic CRUD gap.

**Fix**: Add two match arms to `session_tool`:
- `"rename"` — calls `repo.rename_session(&session_id, &new_name)`
- `"delete"` — calls `repo.delete_session(&session_id)` (soft delete with `deleted_at` timestamp)

Both are <10 lines each. The rename action should be wired to the existing `rename_session` repository method if it exists, or implemented as a small patch.

**Verification**:
```bash
# Via MCP — rename a session
# Via MCP — delete a session, then verify it no longer appears in list
```

**Tasks**: 2 (rename + delete, each ~1 file changed)
**Pattern**: `mcp-crud.md`

---

### P1-9: `branch_tool` Has No `delete` Action

**Severity**: No way to clean up test branches via MCP.
**Files**: `crates/ozone-mcp/src/lib.rs`

**Fix**: Add `"delete"` match arm calling `repo.delete_branch(&session_id, &branch_id)`.

**Tasks**: 1
**Pattern**: `mcp-crud.md`

---

### P1-10: `message_tool` Returns Fragile stdout-Parsed Message IDs, No Structured Metadata

**Severity**: `message_tool send` parses `stdout` text to extract message IDs via `line.strip_prefix("  message id      ")`. This breaks if the CLI output format changes. No structured message content is returned.
**Files**: `crates/ozone-mcp/src/lib.rs`

**Root cause**: The MCP server shells out to `ozone-plus send` CLI and parses its stdout. The CLI outputs human-readable text, not machine-readable data.

**Fix**: Two options:
- **Option A (recommended)**: Enhance `message_tool send` to also return the full parsed message metadata from the CLI stdout, including author kind for each message ID. Parse the "author          X" line from stdout to label which ID is user vs assistant.
- **Option B**: Switch `message_tool send` to use direct crate API calls instead of shelling out, which returns structured `MessageId` objects directly.

Option A is a quick improvement. Option B is a larger refactor but removes the fragile CLI dependency entirely for message operations.

**Verification**: `message_tool send` response includes `userMessageId` and `assistantMessageId` as distinct labeled fields.

**Tasks**: 1 (Option A) or 2 (Option B)
**Pattern**: `mcp-structural-response.md`

---

### P1-11: `session_tool list` Has No `found` Count

**Severity**: Returns `sessions: [...]` but no `found: N` count. MCP clients must count the array length themselves.
**Files**: `crates/ozone-mcp/src/lib.rs`

**Fix**: In the `"list"` match arm, add `"found": sessions.len()` to the response JSON.

Same for `branch_tool list` and `memory_tool list`.

**Verification**: `session_tool list` response contains `found: <number>`.

**Tasks**: 1
**Pattern**: `mcp-list-response-enrichment.md`

---

### P1-12: `search_tool` Hits Parsed from stdout Text Is Fragile

**Severity**: `hits` is parsed via `parse_prefixed_field(&output.stdout, "  hits            ")` which silently returns `None` if the regex doesn't match. Users get `hits: null` instead of a count.
**Files**: `crates/ozone-mcp/src/lib.rs`

**Root cause**: Search output format is human-readable, not machine-readable. The parsing is brittle.

**Fix**: Either (a) improve the stdout parsing regex to be more robust, or (b) switch to structured output from the search CLI. For Option B, the search command could be enhanced to output JSON when `--json` is passed.

**Tasks**: 1
**Pattern**: `mcp-brittle-parsing.md`

---

## P2 — Minor UX Polish

### P2-8: `export_tool` Action Field Inconsistency

**Severity**: API design inconsistency; `action` required but other tools use first-field positional args
**Files**: `crates/ozone-mcp/src/lib.rs`

**Fix**: Normalize `export_tool` to match the pattern of other tools. Use `format` as an optional field with a default, rather than `action`:
```
export_tool(sessionId: "...", format: "json" | "text")
```
Keep `action` as a deprecated alias for backward compatibility but document the new format.

**Verification**: Both `format` and `action` work for `export_tool`.

**Tasks**: 1
**Pattern**: `api-stabilization.md`

---

### P2-9: `import_card` Documents `example_dialogue` as String

**Severity**: Documentation gap — no indication SillyTavern arrays are not supported
**Files**: `crates/ozone-mcp/src/lib.rs`, `crates/ozone-persist/src/import_export.rs`

**Fix**: Add doc comments on `import_card` noting the expected format for `example_dialogue` and that SillyTavern V2 arrays are the goal for future support (P0-2 is the fix).

**Tasks**: 1
**Pattern**: `documentation.md`

---

### P2-10: MCP `Content-Length` Framing Error Handling

**Severity**: When a malformed request arrives, the MCP server logs "failed to fill whole buffer" to stderr rather than returning a JSON-RPC error response
**Files**: `crates/ozone-mcp/src/lib.rs`

**Fix**: The framing loop should catch `io::ErrorKind::UnexpectedEof` and return a proper `ParseError` response over stdio, matching the JSON-RPC spec. Currently it exits with a non-zero code and a Go-style error to stderr.

**Verification**: Send a truncated JSON-RPC body and verify the response is a valid JSON-RPC error (not a process exit).

**Tasks**: 1
**Pattern**: `error-handling.md`

---

## Quick Wins (1-2 hours each — no backend refactor needed)

These are high-impact UX improvements that require only MCP layer changes (mostly adding match arms and response fields). Each one makes the API feel like it was designed, not generated.

| # | Quick Win | Impact | Effort |
|---|-----------|--------|--------|
| **Q-1** | `lastMessageId` in `session_tool create` response | Users can branch from the seeded greeting without a second export call | ~2 lines in `session_summary_json` |
| **Q-2** | `parentMessageId` echoed in `message_tool send` response | Clear which message each send was in response to; enables branching without separate lookup | ~3 lines parsing stdout |
| **Q-3** | `found: N` in all list responses (`sessions`, `branches`, `memories`) | MCP clients don't need to count arrays; consistent with search `hits` | ~1 line per list action |
| **Q-4** | `activeBranchName` in `session_tool metadata` response | Users can see the current branch name without a separate `branch_tool list` call | ~1 field in metadata response |
| **Q-5** | `import_card` result tells you whether SillyTavern format was detected | User knows if their card was imported correctly and if array conversion happened | ~5 lines in `import_card` response |
| **Q-6** | `message_tool send` response labels which ID is user vs assistant | Eliminates ambiguity in multi-turn flows; no stdout parsing needed | ~5 lines in message_tool |
| **Q-7** | `search_tool` global fallback message says "FTS mode — configure embedding for vector search" | User understands why vector search isn't working and what to do about it | ~1 line |

**Verification for all quick wins**: Build, send one MCP call per win, verify response structure.

---

## Implementation Phases

### Phase A: Data Integrity (P0-1, P0-2) — Critical

These break real data workflows. Do first.

| # | Task | Files | Commands |
|---|------|-------|----------|
| A-1 | Strip `<\|im_end\|>` from assistant message content before storage | `ozone-persist/src/repository/message_ops.rs` | `make preflight` |
| A-2 | Support SillyTavern `example_dialogue` array format | `ozone-persist/src/import_export.rs` | `make preflight`, verify Daemon Sand card imports |

### Phase B: Feature Completeness (P0-3, P1-4, P1-5, P1-8, P1-9) — Core

These make the system complete and consistent.

| # | Task | Files | Commands |
|---|------|-------|----------|
| B-1 | Move `seed_greeting_if_present` to shareable location | `ozone-engine`, `ozone-persist` | `make preflight`, verify greeting on session reload |
| B-2 | Wire greeting seeding into MCP session load | `ozone-mcp/src/lib.rs` | Live test via MCP |
| B-3 | Expose note memories via `memory_tool list` with kind filter | `ozone-mcp/src/lib.rs` | Live test: note → list(kind: note) |
| B-4 | Add `last_message_preview` to MCP session list | `ozone-mcp/src/lib.rs` | Live test: verify field populated |
| B-5 | Add `session_tool rename` and `delete` actions | `ozone-mcp/src/lib.rs` | Live test: rename → list shows new name |
| B-6 | Add `branch_tool delete` action | `ozone-mcp/src/lib.rs` | Live test: delete → branch_tool list returns empty |

### Phase C: UX Polish (P1-6, P1-7, P1-10, P1-11, P1-12, P2-8, P2-9, P2-10) — Refinement

These improve the experience without changing data semantics.

| # | Task | Files | Commands |
|---|------|-------|----------|
| C-1 | Replace embedding disabled error with user-facing FTS fallback message | `ozone-mcp/src/lib.rs` | Live test: verify message |
| C-2 | Rename `swipe_tool` to `swipe_alternative_tool` + add description | `ozone-mcp/src/lib.rs` | `make preflight` |
| C-3 | Normalize `export_tool` format field | `ozone-mcp/src/lib.rs` | `make preflight`, both formats work |
| C-4 | Fix MCP framing error to return JSON-RPC error | `ozone-mcp/src/lib.rs` | Live test: send truncated body |
| C-5 | Add doc comments on expected `example_dialogue` format | `ozone-persist/src/import_export.rs` | `cargo doc --no-deps` |
| C-6 | Structured message metadata in `message_tool send` (Option A) | `ozone-mcp/src/lib.rs` | Live test: verify userMessageId/assistantMessageId labels |
| C-7 | `found: N` in all list responses | `ozone-mcp/src/lib.rs` | Live test: verify `found` field present |
| C-8 | Robust `search_tool` hits parsing | `ozone-mcp/src/lib.rs` | Live test: search returns non-null hits |

### Phase Q: Quick Wins — Small But Significant

Run in parallel, do any time.

| # | Task | Files | Commands |
|---|------|-------|----------|
| Q-1 | `lastMessageId` in session create response | `crates/ozone-mcp/src/lib.rs` | Build + MCP call |
| Q-2 | `parentMessageId` echoed in send response | `crates/ozone-mcp/src/lib.rs` | Build + MCP call |
| Q-3 | `found: N` in list responses | `crates/ozone-mcp/src/lib.rs` | Build + MCP call |
| Q-4 | `activeBranchName` in metadata response | `crates/ozone-mcp/src/lib.rs` | Build + MCP call |
| Q-5 | `import_card` format detection notice | `crates/ozone-mcp/src/lib.rs` | Build + import SillyTavern card |
| Q-6 | Label user vs assistant in send response | `crates/ozone-mcp/src/lib.rs` | Build + MCP call |
| Q-7 | FTS fallback message in search | `crates/ozone-mcp/src/lib.rs` | Build + MCP call |

---

## Verification Checklist (run after every phase)

```bash
make preflight          # clippy clean + all tests pass
cargo test --workspace  # must be green before and after

# Live smoke tests
cargo build -p ozone-mcp-app
# MCP initialize
echo '{"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}' | timeout 5 ./target/debug/ozone-mcp | grep -c "protocolVersion"  # must be 1
# Tools list
payload='{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}'
printf "Content-Length: ${#payload}\r\n\r\n$payload" | timeout 5 ./target/debug/ozone-mcp | grep -c "name"  # must be 19
# Import character with greeting
# Verify export: first message is assistant greeting (not user)
```

---

## Rollback Plan

If a change breaks tests or introduces regression:
1. `git stash` — save uncommitted work
2. `git log --oneline dev~5..dev` — identify last known-good commit
3. `git diff dev~5..dev --stat` — see what changed
4. `make preflight` before and after each rollback
5. `git stash pop` to restore if rollback was wrong direction

Each task is small enough to isolate. The P0 fixes are in the data layer (persist) and verified by the existing test suite — rollback risk is low.

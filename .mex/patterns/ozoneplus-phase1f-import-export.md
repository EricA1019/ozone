---
name: ozoneplus-phase1f-import-export
description: Pattern for ozone+ Phase 1F import/export surfaces — character card ingestion, session export, transcript export, product polish (bookmarks, stats, slash commands).
applies_to: ["apps/ozone-plus/src/main.rs", "apps/ozone-plus/src/runtime.rs", "crates/ozone-persist/src/import_export.rs", "crates/ozone-persist/src/repository.rs", "crates/ozone-tui/src/*"]
---

# Phase 1F: Import/Export and Product Polish

## Character Card Import

`crates/ozone-persist/src/import_export.rs` owns `CharacterCard` and its parser.

Three input formats are handled transparently in `CharacterCard::from_json_str`:
- **ozone-plus native** (`"format": "ozone-plus.character-card.v1"`)
- **chara_card_v2** (has `"spec"` field with nested `"data"` object; `first_mes` → `greeting`, `mes_example` → `example_dialogue`)
- **flat legacy JSON** (name + description + optional greeting/example fields)

`import_character_card` in `repository.rs` inserts a StoredCharacterCard row and creates a new session seeded with the greeting (if present).

CLI surface:
```
ozone-plus import card <path-to-json>
```
Output includes the new session id so callers can immediately `open` it.

## Session + Transcript Export

`export_session` produces `ozone-plus.session-export.v1` JSON — full dump of:
- session summary
- stored character card (if attached)
- all branches with transcript_message_id arrays
- all messages
- all bookmarks
- all swipe groups

`export_transcript` produces either:
- `ozone-plus.transcript-export.v1` JSON (structured)
- Plain text (markdown-style with `# ozone+ transcript export` header and labeled message blocks)

CLI surface:
```
ozone-plus export session <session-id>            # stdout JSON
ozone-plus export transcript <session-id>         # stdout plain text
ozone-plus export transcript <session-id> --json  # stdout JSON
```

## Bookmark Toggle

- Runtime command: `RuntimeCommand::ToggleBookmark { message_id }`
- TUI key: `b` on selected transcript entry
- Runtime does read-before-write: calls `list_bookmarks`, checks if already bookmarked, flips it
- `TuiTranscriptItem::is_bookmarked` drives the `★` indicator in the render layer

## Slash Commands (`/session`)

Composer text starting with `/` is dispatched as a `RuntimeCommand::RunCommand { text }`.

Runtime routes through `parse_session_command`:
| Command | Effect |
|---|---|
| `/session show` | Display session metadata (name, character, tags, stats) |
| `/session rename <new-name>` | Rename the current session |
| `/session character <name>` | Update the attached character name |
| `/session tags <tag1,tag2,...>` | Replace the tag list |

Unknown `/` commands return a user-facing error notification.

## Stats Display

`SessionStats` is computed during `load_session_snapshot`:
- `message_count`, `user_count`, `assistant_count`
- `bookmark_count` (from `list_bookmarks`)

`SessionMetadata` holds:
- `name`, `character_name`, `tags`, `is_bookmarked`

Both are passed to `TuiBootstrap` and can be refreshed via `build_session_refresh`.

## Tests

- `import_and_export_commands_use_xdg_paths` integration test in `apps/ozone-plus/src/main.rs` covers import card → export session roundtrip against a temp XDG dir.
- `cargo test -p ozone-persist` and `cargo test -p ozone-plus` must pass green.

## UUID Extraction

When automating around the CLI, extract the session UUID from `import card` output with:
```
grep -Eo '[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}'
```
(Do not rely on `awk '/session id/'` — field positions can vary.)

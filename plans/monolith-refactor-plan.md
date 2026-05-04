# Monolith Refactoring Plan — ozone-rs

> Generated after full audit of 5 largest source files (24,139 lines total).

---

## FILE 1: `crates/ozone-mcp/src/lib.rs` — 6,232 lines

### Current Structure

| Line Range | Section | Lines |
|---|---|---|
| 1-43 | Imports, constants | 43 |
| 44-2488 | `OzoneMcpServer` impl with all tool handlers | 2,445 |
| 2489-2496 | `impl Drop for OzoneMcpServer` | 8 |
| 2497-2594 | `Sandbox` struct + impl | 98 |
| 2595-2615 | `ManagedBackend` struct + impl | 21 |
| 2616-3204 | `PreparedSandbox`, `CapturableScreenJourneyDefinition`, journey builder methods (23 `build_base_*` / `build_ozone_plus_*` functions) | 589 |
| 3205-3221 | `MockUserJourneySpec`, `MockUserJourneyStep`, `MockUserAction` | 17 |
| 3271-3416 | `LauncherSmokeRunnerSpec`, `MockUserRunnerSpec`, PTY capture structs (`PtyVteCaptureConfig`, `PtyVteCursor`, etc.) | 146 |
| 3417-3530 | Screen check structs (`ScreenRegion`, `ScreenCheckOutcome`, `ScreenColorMatch`, `ComparableScreenCell`, `BaselineCompareDiff`) + PTY capture impl | 114 |
| 3531-3637 | VTE helper functions (VTE parsing, cursor tracking, cell extraction) | 107 |
| 3638-4067 | `ToolDefinition` struct + 19 `tool_definitions()` entries | 430 |
| 4068-4108 | `ToolReply` struct + impl | 41 |
| 4109-4130 | `CommandOutput` struct, `EnvOverrideGuard` | 22 |
| 4131-5204 | `JsonRpcRequest` struct + JSON-RPC protocol helpers, VTE screen check implementation | 1,074 |
| 5205-5661 | Horizontal edge analysis, screen diff helpers, `render_transcript_text` | 457 |
| 5662-6232 | `#[cfg(test)]` module | 571 |

### Split Plan (7 files)

| Target File | Line Range | Description |
|---|---|---|
| **`lib.rs`** (thin) | 1-67, 80-176 | Imports, constants, `OzoneMcpServer` type, `handle_request`, `run_stdio_server` — module glue |
| **`tools/session.rs`** | 177-208, 574-792 | `workspace_status_tool`, `session_tool`, `message_tool`, `memory_tool`, `search_tool` — data layer tools |
| **`tools/conversation.rs`** | 951-1037, 1038-1201 | `branch_tool`, `swipe_tool`, `export_tool`, `import_card_tool` |
| **`tools/workspace.rs`** | 210-335 | `cargo_tool`, `catalog_list_tool`, `preferences_get_tool` — local workspace tools |
| **`sandbox.rs`** | 337-557, 2497-2615 | `sandbox_tool`, `create_sandbox`, `destroy_sandbox`, `mock_backend_tool`/`start/stop`, `Sandbox`/`ManagedBackend` structs |
| **`testing/journey.rs`** | 1558-1557, 1600-2223, 2241-2488 | Mock user journey builders (all `build_base_*` and `build_ozone_plus_*` functions), `MockUserJourneySpec`, `run_mock_user_journey`, `build_mock_user_journey`, `prepare_mock_user_sandbox` |
| **`testing/screen.rs`** | 1453-1557, 3638-3700, 5205-5661 | Screenshot, screen capture, VTE parsing, screen check logic (`screen_check_tool`, `screenshot_tool`, `screen_nav_targets_tool`) |
| **`testing/types.rs`** | 2616-3637 | `PreparedSandbox`, `CapturableScreenJourneyDefinition`, PTY capture structs, `ToolDefinition`, `ToolReply`, `JsonRpcRequest` |
| **`lib.rs`** (test) | 5662-6232 | Tests module — stays at bottom of lib.rs or moves to `tests/mod.rs` |

### Public API to Preserve
- `pub fn run_stdio_server() -> Result<()>` (line 44)
- `OzoneMcpServer` struct (line 62) — currently private struct, keep as is
- All tool definitions via `tool_definitions()` function

### Dependencies
- Depends on: `ozone_core`, `ozone_persist`, `serde`, `serde_json`, `uuid`, `anyhow`
- Internal: JSON-RPC types, tool infrastructure

---

## FILE 2: `crates/ozone-tui/src/app.rs` — 6,110 lines

### Current Structure

| Line Range | Section | Lines |
|---|---|---|
| 1-17 | Imports | 17 |
| 18-73 | `TextAreaSurface` enum, textarea theming functions | 56 |
| 88-106 | `ScreenState` enum | 19 |
| 107-131 | `SettingsCategory` enum + impl | 25 |
| 132-176 | `EntryKind` enum + impl | 45 |
| 177-402 | `SettingsState` struct + impl (category navigation, entries) | 226 |
| 403-416 | `SettingsEntry` struct | 14 |
| 417-433 | `FocusTarget`, `InspectorFocus` enums | 17 |
| 434-448 | `InspectorState` struct + Default | 15 |
| 449-535 | `MenuItem`, `MenuState` structs + impl (navigation) | 87 |
| 536-711 | `SessionListEntry`, `VisibleSessionItem`, `SessionListState` + impl | 177 |
| 712-770 | `FolderPickerState` struct + impl | 59 |
| 771-940 | `CharacterEntry`, `CharacterDetail`, `CharacterListState`, `CharacterFormField`, `CharacterCreateState` | 170 |
| 941-964 | `CharacterImportState`, `SessionContext` | 24 |
| 965-1072 | `DraftCheckpoint`, `DraftState` + impl (cursor movement, dirty tracking) | 108 |
| 1073-1133 | `InputHistoryState` + impl | 61 |
| 1134-1258 | `TranscriptItem`, `BranchItem`, `ContextTokenBudget`, `ContextPreview`, `ContextDryRunPreview` | 125 |
| 1259-1286 | `RecallBrowser`, `TuiSessionMemoryMetadata`, `TuiMemoryView`, `SessionMetadata`, `SessionStats` | 28 |
| 1287-1402 | `RuntimePhase` enum + impl | 116 |
| 1403-1487 | Runtime event types: `ContextCompressionEvent`, `RuntimeSendReceipt`, `RuntimeContextRefresh`, `RuntimeSessionLoad`, `RuntimeCompletion`, `RuntimeCancellation`, `RuntimeProgress`, `RuntimeFailure` | 85 |
| 1488-1520 | `GenerationPoll` enum, `SessionState` + impl | 33 |
| 1521-1661 | `AppBootstrap`, `CommandEntry` + impl, `CommandPaletteState` | 141 |
| 1662-1754 | `CommandPaletteState` impl (open/close, filtering, selection) | 93 |
| 1755-1764 | `MessageEditState` struct | 10 |
| 1765-1814 | `ShellState` struct | 50 |
| 1815-3592 | `impl ShellState` — event handling, input, key dispatch, mouse, runtime commands, state transitions | **1,778** |
| 3593-6110 | `#[cfg(test)]` module | 2,518 |

### Split Plan (8 files)

| Target File | Line Range | Description |
|---|---|---|
| **`app/mod.rs`** | 1815-3592 | `ShellState` struct and impl — the core event loop integration, state machine |
| **`app/state.rs`** | 88-770 | All screen/state enums and structs: `ScreenState`, `MenuState`, `SessionListState`, `FolderPickerState`, `SettingsState`, `InspectorState` + all their impls |
| **`app/character.rs`** | 771-940 | Character management: `CharacterEntry`, `CharacterDetail`, `CharacterListState`, `CharacterFormField`, `CharacterCreateState`, `CharacterImportState` |
| **`app/draft.rs`** | 965-1133 | Text input infrastructure: `DraftState`, `DraftCheckpoint`, `InputHistoryState`, textarea theming (18-73) |
| **`app/protocol.rs`** | 1134-1520 | Data-transfer types: `TranscriptItem`, `BranchItem`, `ContextPreview`, `SessionMetadata`, `SessionStats`, `RuntimePhase`, all runtime event types, `GenerationPoll`, `SessionState` |
| **`app/commands.rs`** | 1329-1402, 1521-1754 | `RuntimeCommand` enum, `CommandEntry`, `CommandPaletteState` + impl |
| **`app/types.rs`** | 1-87, 107-448 | Core enums and basic types: `TextAreaSurface`, `ScreenState`, `SettingsCategory`, `EntryKind`, `FocusTarget`, `InspectorFocus` |
| **`app/tests.rs`** | 3593-6110 | All tests (currently 41% of the file) |

### Public API to Preserve
**All types re-exported from `ozone-tui/src/lib.rs` (lines 24-33):**
- `AppBootstrap`, `BranchItem`, `CharacterDetail`, `CharacterEntry`, `CharacterListState`, `CommandEntry`, `CommandPaletteState`, `ContextDryRunPreview`, `ContextPreview`, `ContextTokenBudget`, `DraftState`, `EntryKind`, `FocusTarget`, `FolderPickerState`, `GenerationPoll`, `MenuItem`, `MenuState`, `RecallBrowser`, `RuntimeCancellation`, `RuntimeCompletion`, `RuntimeContextRefresh`, `RuntimeFailure`, `RuntimePhase`, `RuntimeProgress`, `RuntimeSendReceipt`, `RuntimeSessionLoad`, `ScreenState`, `SessionContext`, `SessionListEntry`, `SessionListState`, `SessionMetadata`, `SessionState`, `SessionStats`, `SettingsCategory`, `SettingsEntry`, `SettingsState`, `ShellState`, `TranscriptItem`, `TuiMemoryView`, `TuiSessionMemoryMetadata`, `VisibleSessionItem`

These all need `pub use` re-exports from `app/mod.rs` in `lib.rs`.

### Dependencies
- Depends on: `ratatui`, `crossterm`, `ozone_core`, `tui_textarea`, `arboard`, `crate::input`, `crate::theme`
- Heavy dependency from `render.rs` (imports 16 types from this file)
- No circular deps within ozone-tui (render depends on app, not vice versa)

---

## FILE 3: `crates/ozone-tui/src/render.rs` — 4,319 lines

### Current Structure

| Line Range | Section | Lines |
|---|---|---|
| 1-22 | Imports | 22 |
| 23-322 | All render model structs (19 structs, 2 enums) | 300 |
| 323-358 | `RenderModel` struct + `build_folder_picker_model` | 36 |
| 359-908 | `build_render_model` — THE GOD FUNCTION | **550** |
| 909-1125 | `build_hints` | 217 |
| 1126-1151 | `build_breadcrumb` | 26 |
| 1152-1274 | `render_shell` — top-level shell render dispatcher | 123 |
| 1275-1391 | `render_hints`, `render_command_palette` | 117 |
| 1392-1456 | `render_slash_popup`, `render_breadcrumb` | 65 |
| 1457-1769 | Conversation rendering: `ConversationContent` struct, `render_conversation`, `conversation_viewport`, `build_conversation_content`, line wrapping | 313 |
| 1770-1946 | Composer rendering: `render_composer`, scroll metrics, visual line counting | 177 |
| 1947-2129 | Status & inspector rendering: `render_status`, `render_inspector`, `format_mb`, `render_overlay` | 183 |
| 2130-2233 | Overlays: `render_help_overlay`, `render_toast` | 104 |
| 2234-2401 | Main menu: `render_main_menu`, `render_menu_placeholder` | 167 |
| 2402-2647 | Session list: `render_session_list` (includes folder picker inline) | 246 |
| 2648-2722 | `centered_rect`, `render_folder_picker` | 75 |
| 2723-2937 | Character list: `render_character_list` | 215 |
| 2938-3038 | Character form: `render_character_form` | 101 |
| 3039-3166 | Settings: `render_settings` | 128 |
| 3167-3300 | Model intelligence: `render_model_intelligence`, `textwrap_simple`, `truncate_str` | 134 |
| 3301-3694 | Helper functions: `pane_block`, `overlay_block`, labels for input mode/screen/focus/selection/branch/runtime, context preview lines, slash suggestions, overlay model | 394 |
| 3695-4319 | `#[cfg(test)]` module | 625 |

### Split Plan (7 files)

| Target File | Line Range | Description |
|---|---|---|
| **`render/mod.rs`** | 1152-1274, 15-22, 323-358 | Module glue: imports, `RenderModel`, `build_render_model`, `render_shell` entry point |
| **`render/model.rs`** | 23-322 | All 19 render model structs and 2 enums (ConversationEntryModel, ConversationPaneModel, ComposerPaneModel, StatusPaneModel, InspectorPaneModel, ShellIndicators, etc.) |
| **`render/conversation.rs`** | 1457-1769 | Conversation pane rendering + viewport calculation + line wrapping |
| **`render/composer.rs`** | 1770-1946 | Composer rendering, scrollbars, visual line/cursor position calculation |
| **`render/status_inspector.rs`** | 1947-2129 | Status bar + inspector pane rendering |
| **`render/screens.rs`** | 2130-3038 | Screen-specific renders: main menu, session list, character list/form, settings |
| **`render/helpers.rs`** | 3301-3694, 909-1151 | Utility functions: blocks, label formatters, hints, breadcrumb, text wrapping, slash suggestions |
| **`render/tests.rs`** | 3695-4319 | Test module |

### Public API to Preserve
- `pub fn build_render_model(state: &ShellState, layout: &LayoutModel) -> RenderModel` (line 359)
- `pub fn render_shell(...)` (line 1152)
- `pub(crate) fn conversation_viewport(...)` (line 1492)
- All `pub struct` model types
- Re-exported from `lib.rs` line 42: `build_render_model`, `render_shell`, `FolderPickerRenderModel`, `RenderModel`

### Dependencies
- Heavy dependency ON `app.rs` (imports `ShellState`, `CommandEntry`, `ContextPreview`, `EntryKind`, `FocusTarget`, `RuntimePhase`, `ScreenState`, `VisibleSessionItem`)
- Depends on: `crate::layout`, `crate::theme`, `crate::input`
- No circular dependency — clean DAG

---

## FILE 4: `apps/ozone-plus/src/main.rs` — 3,780 lines

### Current Structure

| Line Range | Section | Lines |
|---|---|---|
| 1-50 | Imports, module declarations, constants | 50 |
| 51-538 | CLI types: `Cli`, `Command` enum (13 subcommands), ~25 arg structs | 488 |
| 540-548 | `main()` function | 9 |
| 550-558 | `run() -> Result` | 9 |
| 560-598 | `run_cli()` — command dispatch match (13 arms) | 39 |
| 599-705 | `RepoConversationStore` struct + `ConversationStore` impl | 107 |
| 706-871 | `Phase1bCliEngine` struct + impl | 166 |
| 1135-1157 | `print_bootstrap_summary`, `print_identity`, `print_docs`, `print_paths` | 23 |
| 1214-1267 | Session management: `create_session`, `list_sessions`, `handoff_session`, `open_session`, `handoff_candidates`, `create_handoff_session`, `open_session_record`, `run_session_shell` | 54 |
| 1394-1489 | `OzonePlusPrefs` struct + Default, theme loading | 96 |
| 1490-1553 | `open_session_metadata` | 64 |
| 1554-1644 | Messaging: `send_message`, `send_message_legacy`, `show_transcript`, `edit_message` | 91 |
| 1645-1687 | Branch operations: `handle_branch_command`, `list_branches`, `create_branch`, `activate_branch` | 43 |
| 1688-1745 | Swipe operations: `handle_swipe_command`, `list_swipes`, `add_swipe_candidate`, `activate_swipe` | 58 |
| 1746-1795 | Import: `handle_import_command`, `import_character_card` | 50 |
| 1796-1850 | Export: `handle_export_command`, `export_session`, `export_transcript` | 55 |
| 1851-1962 | Memory: `handle_memory_command`, `pin_memory`, `create_note_memory`, `list_memories`, `unpin_memory` | 112 |
| 1963-2022 | Search/index: `handle_search_command`, `search_session`, `search_global`, `handle_index_command`, `rebuild_vector_index` | 60 |
| 2023-2140 | Summarization: `handle_summarize_command`, `summarize_session`, `summarize_chunk` | 118 |
| 2141-2228 | Lifecycle: `handle_lifecycle_command`, `lifecycle_inspect`, `lifecycle_disk_status` | 88 |
| 2229-2363 | GC: `handle_gc_command`, `build_gc_policy_and_session`, `gc_plan`, `gc_run`, `print_gc_plan`, `print_gc_outcome`, `reason_label` | 135 |
| 2373-2402 | Events: `open_repository`, `handle_events_command`, `events_compact` | 30 |
| 2403-2488 | Mapping/printing: `map_branch_record`, `conversation_message_from_record`, `print_session_details`, `print_branch_record`, `print_transcript`, `print_message`, `print_pinned_memory_view` | 86 |
| 2489-2795 | Report formatting: `format_search_report`, `format_search_status`, `format_search_hit`, `print_swipe_group_snapshot` | 307 |
| 2827-2985 | Path/file utilities: `print_session_paths`, `print_optional_path`, `require_existing_file`, `read_utf8_file`, `write_output_file`, `render_transcript_text` | 159 |
| 2986-3165 | Parsing/utilities: `require_non_empty`, `optional_value`, `normalize_tags`, `format_tags`, `format_timestamp`, `format_timestamp_short`, `format_message_time`, `format_author_id`, `now_timestamp_ms`, `generate_*`, `generate_uuid_like`, `parse_session_id`, `parse_message_id`, `parse_memory_artifact_id`, `parse_branch_id`, `parse_swipe_group_id` | 180 |
| 3166-3780 | `#[cfg(test)]` module | 615 |

### Split Plan (6 files)

| Target File | Line Range | Description |
|---|---|---|
| **`main.rs`** (thin) | 1-65, 540-558, 560-598 | Module declarations, CLI root types, `main()`, `run()`, `run_cli()` dispatch |
| **`cli/mod.rs`** | 66-538 | All CLI argument structs and enums (the entire clap DSL) |
| **`cli/session.rs`** | 1214-1489 | Session lifecycle CLI commands: create, list, open, handoff, session metadata |
| **`cli/message.rs`** | 1554-1850 | Message/branch/swipe CLI commands: send, edit, transcript, branch ops, swipe ops |
| **`cli/memory.rs`** | 1851-2140 | Memory/search/summarize CLI: pin, note, search sessions/global, rebuild index, summarize |
| **`cli/admin.rs`** | 2141-2402 | Admin CLI: lifecycle, GC, events, export/import |
| **`cli/output.rs`** | 2403-2863 | Output/formatting: print helpers, search report formatting, path utilities, transcript rendering |
| **`cli/parse.rs`** | 2986-3165 | Parsing utilities: `require_non_empty`, `normalize_tags`, `format_tags`, `format_timestamp`, ID generation, ID parsing |
| **`cli/store.rs`** | 599-871 | `RepoConversationStore`, `Phase1bCliEngine` |
| **`main_test.rs` or `tests/`** | 3166-3780 | Test module |

### Public API to Preserve
- `fn main() -> ExitCode` — the binary entry point
- All CLI subcommands and their args (used by clap derive macros)
- `RepoConversationStore` — implements `ConversationStore` trait from `ozone_engine`
- `Phase1bCliEngine` — engine used for non-interactive CLI operations

### Dependencies
- Depends on: `ozone_core`, `ozone_engine`, `ozone_inference`, `ozone_persist`, `ozone_tui`, `clap`
- Internal: `context_bridge`, `hooks`, `hybrid_search`, `index_rebuild`, `inference_adapter`, `runtime`, `session_title`

---

## FILE 5: `apps/ozone-plus/src/runtime.rs` — 3,698 lines

### Current Structure

| Line Range | Section | Lines |
|---|---|---|
| 1-49 | Imports | 49 |
| 50-179 | Internal types: `WorkerEvent`, `PendingGeneration`, `PendingCompletion`, `PendingReroll`, `RerollBranchMode`, `RerollSource`, `SessionSnapshot`, command enums (`SessionCommand`, `MemoryCommand`, `SearchCommand`, `ShellCommand`, `SummarizeShellCommand`, `ThinkingCommand`, `TierBCommand`, `HooksCommand`, `SafeModeCommand`), `RecentSearchSection` | 130 |
| 180-197 | `impl PendingGeneration` | 18 |
| 198-215 | `Phase1dRuntime` struct definition | 18 |
| 216-1332 | `impl Phase1dRuntime` — THE CORE | **1,117** |
| 1333-1339 | `impl Drop for Phase1dRuntime` | 7 |
| 1340-2696 | `impl SessionRuntime for Phase1dRuntime` — trait implementation | **1,357** |
| 2697-2969 | Shell command parsing: `short_id`, `hit_suffix`, `parse_shell_command`, all `parse_*_subcommand`, `unknown_shell_command_message`, `require_non_empty`, `normalize_tags`, `format_tags`, `repository_template_dir` | 274 |
| 3010-3698 | `#[cfg(test)]` module | 689 |

### Split Plan (5 files)

| Target File | Line Range | Description |
|---|---|---|
| **`runtime/mod.rs`** | 198-215, 1340-2696 | `Phase1dRuntime` struct definition, `impl SessionRuntime` trait impl — the primary integration seam |
| **`runtime/types.rs`** | 50-179, 180-197 | Internal command enums, `PendingGeneration`, `PendingCompletion`, `RerollSource`, `SessionSnapshot`, etc. |
| **`runtime/generation.rs`** | 216-400, 620-730, 1143-1246, 1306-1332 | Generation lifecycle: `start_generation_task`, `complete_generation`, `resolve_reroll_source`, `ensure_reroll_swipe_group`, `complete_reroll_generation`, `mark_generation_failure`, `set_generation_state` |
| **`runtime/context.rs`** | 400-619, 730-1142, 1143-1306, 1683-1821 | Context building: `load_bootstrap`, `load_session_snapshot`, `load_persisted_draft`, `build_context_for_generation`, context dry run, refresh, session switching, inference loading, recall browser |
| **`runtime/commands.rs`** | 1822-2696, 2697-2969 | Command dispatch: `run_command`, shell command parsing (`parse_shell_command` and all sub-parsers), session/memory/search commands, character ops, settings ops, draft persistence |
| **`runtime/tests.rs`** | 3010-3698 | Test module |

### Public API to Preserve
- `pub(crate) struct Phase1dRuntime` — the runtime type
- `impl SessionRuntime for Phase1dRuntime` — the trait implementation that `ozone_tui::run_terminal_session` depends on
- `pub fn open(repo: SqliteRepository, session_id: SessionId) -> Result<Self, String>`
- `pub fn check_backend_health(&self) -> Result<(), String>`
- `pub fn release_lock(&mut self) -> Result<(), String>`
- `pub fn latest_context_plan_preview(&self) -> Option<&ContextPlanPreview>`
- `pub fn latest_context_dry_run(&self) -> Option<&DryRunContextBuild>`

### Dependencies
- Depends on: `ozone_core::engine`, `ozone_engine`, `ozone_inference`, `ozone_memory`, `ozone_persist`, `ozone_tui`, `tokio`
- Internal to ozone-plus: `context_bridge`, `hybrid_search`, `inference_adapter`, `session_title`, `crate::hooks`
- `main.rs` uses `Phase1dRuntime` via `use runtime::Phase1dRuntime`

---

## CROSS-FILE DUPLICATED CODE

| Function | Files | Lines (approximate) |
|---|---|---|
| `format_tags(tags: &[String]) -> String` | `main.rs:3011`, `runtime.rs:2989`, `render.rs:3687` | Identical 3-copy |
| `require_non_empty(label: &str, value: String) -> Result<String, String>` | `main.rs:2986`, `runtime.rs:2970` | Identical 2-copy |
| `normalize_tags(...)` | `main.rs:3002`, `runtime.rs:2979` | Near-identical 2-copy |
| `format_timestamp` / `format_timestamp_short` / `format_message_time` | `main.rs:3019-3080` | Only in main.rs, but could be shared |
| `short_id(...)` | `runtime.rs:2782` | Could be shared with `format_branch_id` in main.rs |

### Dedup Strategy
These belong in one of two places:
1. **`ozone-core/src/shared.rs`** — if they're conceptually shared between TUI and backend (like `format_tags`)
2. **`ozone-tui` utility module** — if they're UI-only formatting helpers
3. **`ozone-plus` utility module** — if they're only used by the plus binary

---

## CIRCULAR DEPENDENCY ANALYSIS

### Current dependency graph (clean — no cycles detected):
```
ozone-tui/render.rs --> ozone-tui/app.rs
ozone-tui/lib.rs --> ozone-tui/app.rs, ozone-tui/render.rs
ozone-plus/runtime.rs --> ozone-tui (trait types)
ozone-plus/main.rs --> ozone-plus/runtime.rs, ozone-tui
ozone-mcp/lib.rs --> ozone-core, ozone-persist (independent of TUI)
```

**No circular dependencies found.** The project is already organized into a clean DAG. This is good news — the refactoring can proceed without needing to break cycles first.

### Risk points:
1. `ozone-tui/app.rs` exports 39 types through `lib.rs` — moving them requires updating `lib.rs` re-exports
2. `ozone-tui/render.rs` imports 6 types from `app.rs` — any split must keep these imports working
3. `ozone-plus/runtime.rs` implements `SessionRuntime` trait from `ozone-tui` — the trait must remain at `ozone-tui` (currently in `mock.rs`)
4. `ozone-plus/main.rs` has `use runtime::Phase1dRuntime` — moving runtime modules requires updating this import

---

## IMPLEMENTATION ORDER

1. **Phase 1: `ozone-tui/render.rs`** — Lowest risk. Already has clear render model structs that are self-contained. No downstream consumers except `lib.rs`.
2. **Phase 2: `ozone-tui/app.rs`** — Highest priority (most imports). Requires updating `lib.rs` re-exports. Split state types first, then ShellState impl, then tests.
3. **Phase 3: `ozone-plus/runtime.rs`** — Independent. Can be done in parallel with Phase 1. Split types first, then the massive impl.
4. **Phase 4: `ozone-plus/main.rs`** — Depends on understanding of CLI patterns. Split CLI types first, then move command handlers.
5. **Phase 5: `ozone-mcp/lib.rs`** — Largest file but most self-contained. Split tool handlers first (they're already method-per-tool), then testing infrastructure.

---

## PREDICTED FILE COUNT AFTER REFACTORING

| Before | After | Reduction |
|---|---|---|
| `ozone-mcp/lib.rs`: 6,232 | ~7 files, avg ~800 lines | 7x smaller |
| `ozone-tui/app.rs`: 6,110 | ~8 files, avg ~600 lines | 8x smaller |
| `ozone-tui/render.rs`: 4,319 | ~7 files, avg ~500 lines | 7x smaller |
| `ozone-plus/main.rs`: 3,780 | ~6 files, avg ~600 lines | 6x smaller |
| `ozone-plus/runtime.rs`: 3,698 | ~5 files, avg ~600 lines | 5x smaller |
| **Total: 24,139** | **~33 files, avg ~600 lines** | **~7x improvement** |

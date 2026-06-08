# Ozone Codebase Refactor & Cleanup Implementation Plan v3

> **For Hermes:** Implement task-by-task. Validate with `cargo test -p <crate> --lib` after EVERY module extraction. Commit after each task completes.

**Goal:** Refactor 4 massive files (17,913 total lines) into ~30 semi-independent modules (each under 800 lines), eliminate dead code, ship UX quick wins, and prepare Ozone for alpha release.

**Architecture Principles:**
- Max 800 lines per module (most 200-600)
- Each module is a semi-independent node — minimal awareness of sibling modules
- Data types separated from behavior (types in own files, impl blocks in behavior files)
- Tests travel with their code (each module carries its own `#[cfg(test)]`)
- Dependencies flow DOWN only (no circular imports within a module group)
- Extract leaf modules first (pure data → pure functions → behavior → coordinator)

---

## Phase 0: Pre-Refactor Audit & Baseline (30 minutes)

### Task 0: Establish Baseline + Identify Dead Code

**Objective:** Know exactly what exists, what's broken, and what's dead before touching anything.

**Step 1: Run full test suite**
```bash
cd /home/eric/projects/ozone
cargo test --workspace 2>&1 | grep "test result:"
```
Expected: All tests pass (establishes baseline).

**Step 2: Run clippy baseline**
```bash
cargo clippy --workspace --all-targets 2>&1 | grep "warning:" | wc -l
```
Record the number. We'll reduce it.

**Step 3: Find all `todo!()` calls (incomplete features)**
```bash
grep -rn "todo!()" crates/ozone-tui/src/ apps/ozone-plus/src/
```
Findings:
- `app.rs:2350` — `MemoriesOverlay | CharacterOverlay(_) => todo!()`
- `app.rs:3026` — `todo!()`
- `render.rs:1148` — `MemoriesOverlay | CharacterOverlay(_) => todo!()`
- `render.rs:3349` — `MemoriesOverlay | CharacterOverlay(_) => todo!()`

**Decision needed:** Are MemoriesOverlay and CharacterOverlay screens planned features or dead code? If dead, remove the variants. If planned, implement stub handlers that show "Coming soon" instead of panicking.

**Step 4: Verify existing features the plan assumes are missing**
```bash
# Help overlay — already exists?
grep -c "render_help_overlay\|ToggleHelp\|ScreenState::Help" crates/ozone-tui/src/render.rs
# Expected: >0 (it exists!)

# Toast notifications — already exists?
grep -c "show_toast\|active_toast" crates/ozone-tui/src/app.rs
# Expected: >0 (it exists!)

# Clipboard support — already exists?
grep -c "Clipboard\|arboard" crates/ozone-tui/src/app.rs
# Expected: >0 (it exists!)

# Session search — already exists in TUI?
grep -c "filter.*session\|search.*session" crates/ozone-tui/src/app.rs
# Check result

# Session sort — already implemented?
grep -c "sort.*session\|SortBy" crates/ --include="*.rs"
# Check result
```

**Step 5: Identify duplicate logic across crates**
```bash
grep -rn "parse_session_id\|parse_branch_id\|parse_message_id" crates/ apps/ --include="*.rs" | grep -v "test\|mod.rs"
grep -rn "format_tags\|format_timestamp\|normalize_tags" crates/ apps/ --include="*.rs"
grep -rn "now_timestamp_ms\|generate_uuid" crates/ apps/ --include="*.rs"
```
These are candidates for consolidation into a shared utility module.

**Step 6: Commit baseline**
```bash
git add -A
git commit -m "chore: baseline before v3 refactor — all tests pass"
```

---

## Phase 1: UX Quick Wins First (3-4 hours)

**Why before refactoring:** Users see immediate value. Quick wins are small, isolated changes that don't interfere with the structural refactor.

### Quick Win Audit: What's Already Implemented vs. Actually Missing

| Feature | Status | Effort if missing |
|---------|--------|-------------------|
| Keyboard shortcut overlay (`?` key) | **DONE** — `render_help_overlay()` exists, `?` key bound to `ToggleHelp` | 0 |
| Clipboard copy | **DONE** — `arboard::Clipboard` imported, copy commands exist | 0 |
| Toast notifications | **DONE** — `show_toast()` with 3-second auto-expiry | 0 |
| Session search/filter | **PARTIAL** — CLI search exists, TUI live filter does NOT | 1-2h |
| Session sorting | **PARTIAL** — Sort logic exists, NOT accessible from TUI | 30min |
| First-run tutorial | **NOT DONE** | 2-3h |
| Theme preview | **NOT DONE** | 1h |
| Token count display | **NOT DONE** — no tokens_used tracked | 30min |
| Tab completion for models | **NOT DONE** | 1h |

### Task 1A: Token Count in Status Line (30 min)

**Status:** NOT implemented. After streaming completes, no token count shown.

**What exists:** `RuntimeSendReceipt` has a `user_message` field. The inference backend tracks tokens internally.

**Files:** `crates/ozone-tui/src/app.rs` (apply_send_receipt method)

**Step 1:** In `apply_send_receipt`, after displaying the response, append token info to the status line:
```rust
// After streaming completes, show token count
let token_count = receipt.tokens_used.unwrap_or_else(|| {
    // Fallback: rough estimate from word count
    receipt.user_message.word_count()
});
self.status_line = Some(format!("↩ {} tokens", token_count));
```

**Step 2:** Run `cargo test -p ozone-tui --lib`

**Step 3:** Commit: `feat: show token count in status line after responses`

### Task 1B: Session Sort Toggle in TUI (30 min)

**Status:** Sort logic exists but not accessible from TUI session list screen.

**Files:** `crates/ozone-tui/src/app.rs` (SessionListState impl)

**Step 1:** Add sort mode cycling to SessionListState:
```rust
pub fn cycle_sort_mode(&mut self) {
    self.sort_mode = match self.sort_mode {
        SessionSortMode::DateDesc => SessionSortMode::NameAsc,
        SessionSortMode::NameAsc => SessionSortMode::Folder,
        SessionSortMode::Folder => SessionSortMode::DateDesc,
    };
    self.apply_sort();
}
```

**Step 2:** Bind to `s` key in the SessionList screen handler.

**Step 3:** Run `cargo test -p ozone-tui --lib`

**Step 4:** Commit: `feat: add session sort toggle (s key) in session list`

### Task 1C: First-Run Tutorial (2 hours)

**Status:** NOT implemented. First launch shows the main menu with no guidance.

**Files:** `apps/ozone-plus/src/cli.rs` (or equivalent after refactor)

**Step 1:** Add `first_run: bool` flag to `OzonePlusPrefs`.

**Step 2:** On startup, if `first_run` is true, show a 3-step overlay tutorial:
```rust
if prefs.first_run {
    show_tutorial(&[
        "Welcome to Ozone+!",
        "1. Add a model: ozone model add --hf <repo>",
        "2. Start chatting: ozone-plus open",
        "3. Search memories: Ctrl+K in conversation",
    ]);
    prefs.first_run = false;
    prefs.save()?;
}
```

**Step 3:** Run `cargo test -p ozone-plus --lib`

**Step 4:** Commit: `feat: add first-run tutorial overlay for new users`

### Task 1D: Theme Preview in Settings (1 hour)

**Status:** NOT implemented. Theme selection shows a text list with no visual preview.

**Files:** `crates/ozone-tui/src/render.rs` (render_settings function)

**Step 1:** In the settings renderer, when a theme preset is highlighted, render a small preview box (8×6 chars) showing sample text in the theme's actual colors:
```rust
if let Some(theme) = selected_theme {
    render_theme_preview(frame, preview_area, theme);
}
```

**Step 2:** Run `cargo test -p ozone-tui --lib`

**Step 3:** Commit: `feat: show color preview when selecting themes in settings`

### Task 1E: Quick Wins Validation

```bash
cargo test --workspace 2>&1 | grep "test result:"
cargo clippy --workspace --all-targets 2>&1 | grep "warning:" | wc -l
```

Commit: `feat: batch UX quick wins (token count, sort, tutorial, theme preview)`

---

## Phase 2: Render Refactor (Task 4) — First, Because It's the Leaf Node

**Why first:** `render.rs` reads state and produces pixels. It has NO circular dependencies on other app modules. The entire render module can be extracted in one focused session. Completing it first gives a clean boundary for the harder app.rs refactor.

**Current:** 4319 lines in one file
**Target:** 9 modules, largest ~900 lines

**Key architectural insight:** `render.rs` has two distinct layers:
1. **Render model structs** (30+ types, ~400 lines) — pure data, no logic
2. **Render functions** (~47 functions) — take `&ShellState` + `&LayoutModel`, produce render models

These layers MUST stay separate. Model types are imported by `app.rs` (for the `RenderModel` field in `ShellState`). Functions are only called from the render loop.

### Module Structure

```
crates/ozone-tui/src/render/
  mod.rs               (~150 lines) — RenderModel struct, build_render_model(), 
                                     render_shell(), re-exports. This is the PUBLIC API.
  model_types.rs       (~400 lines) — ALL render model structs:
                                     ConversationEntryModel, ConversationPaneModel,
                                     ComposerPaneModel, SlashSuggestion, StatusPaneModel,
                                     ModelInfoDisplay, InspectorPaneModel, ShellIndicators,
                                     OverlayRenderModel, HintItem, CommandPaletteRenderModel,
                                     CommandPaletteEntry, MainMenuRenderModel,
                                     MenuItemRenderModel, SessionListRenderModel,
                                     FolderPickerRenderModel, SessionListItemRenderModel,
                                     SessionListEntryRenderModel, CharacterListRenderModel,
                                     CharacterListEntryRenderModel, CharacterDetailRenderModel,
                                     SettingsCategoryRenderItem, SettingsEntryRenderItem,
                                     SettingsRenderModel, CharacterFormRenderModel,
                                     CharacterFieldRenderModel, CharacterFormType,
                                     ModelIntelligenceRenderModel
  conversation.rs      (~500 lines) — Conversation pane rendering:
                                     render_conversation, build_conversation_content,
                                     auto_conversation_scroll_offset, rewrap_lines,
                                     push_wrapped_line, wrap_line
  composer.rs          (~350 lines) — Composer pane rendering:
                                     render_composer, render_composer_scrollbar,
                                     composer_scroll_offset, composer_scroll_metrics,
                                     visual_line_count, visual_cursor_position,
                                     render_slash_popup, build_slash_suggestions
  status_inspector.rs  (~350 lines) — Status + inspector:
                                     render_status, render_inspector, format_mb,
                                     inspector_lines, append_context_preview_lines,
                                     inspector_visibility_label, inspector_focus_label
  menu_screens.rs      (~600 lines) — All menu/list screens:
                                     render_main_menu, render_menu_placeholder,
                                     render_session_list, render_folder_picker,
                                     render_character_list, render_character_form,
                                     render_settings, render_model_intelligence,
                                     build_folder_picker_model, build_hints, build_breadcrumb
  overlays.rs          (~250 lines) — Overlays and popups:
                                     render_overlay, render_help_overlay, render_toast,
                                     render_command_palette, overlay_model
  labels.rs            (~200 lines) — All string label functions:
                                     input_mode_label, screen_label, focus_label,
                                     selection_label, branch_label, runtime_label,
                                     context_status_line, status_short_runtime,
                                     composer_hint, focus_label, selection_label
  helpers.rs           (~200 lines) — Pure formatting helpers:
                                     pane_block, overlay_block, textwrap_simple,
                                     truncate_str, centered_rect, format_tags
```

**Dependency flow (DOWN only):**
```
model_types.rs ← conversation.rs
                 composer.rs
                 status_inspector.rs
                 menu_screens.rs
                 overlays.rs
                 labels.rs ← helpers.rs
                 
                 mod.rs (coordinates all)
```

**Execution Steps:**

**Step 1:** `mkdir -p crates/ozone-tui/src/render`

**Step 2:** Extract `model_types.rs` — Move ALL render model struct definitions. Zero imports from other render modules. Only imports: `ratatui`, `tui_textarea`, external crates.

**Step 3:** Extract `helpers.rs` + `labels.rs` — Pure functions with no dependency on render model types (they return `String`, `&str`, `Block`).

**Step 4:** Extract `conversation.rs` — Depends on `model_types.rs`. The largest single render function.

**Step 5:** Extract `composer.rs` — Depends on `model_types.rs`.

**Step 6:** Extract `status_inspector.rs` — Depends on `model_types.rs`.

**Step 7:** Extract `menu_screens.rs` — Depends on `model_types.rs`. Largest module but self-contained.

**Step 8:** Extract `overlays.rs` — Depends on `model_types.rs`.

**Step 9:** Create `mod.rs` — Contains `RenderModel` struct, `build_render_model()`, `render_shell()`. Re-exports everything public.

**Step 10:** Update `lib.rs` to use new module paths.

**Step 11:** Run `cargo test -p ozone-tui --lib` — Must pass.

**Step 12:** Commit: `refactor(ozone-tui): split render.rs into 9 independent modules`

---

## Phase 3: App Refactor (Task 3) — The Hardest One

**Why second:** Depends on render being stable (render imports app types). 6116 lines with a 4297-line `impl ShellState` God block that must be decomposed by behavioral responsibility.

**Key architectural insight:** `impl ShellState` contains 169 methods with NO clear boundaries. The methods must be grouped by what they DO, not what data they touch:
- **Navigator** — Pure state mutation (menus, lists, pickers). No I/O, no server calls.
- **Composer** — Text editing, draft management, slash completions.
- **Dispatcher** — Event routing, action application.
- **Session lifecycle** — Session CRUD, branching, swiping, context.
- **Command execution** — Shell command parsing and execution.

### Module Structure

```
crates/ozone-tui/src/app/
  mod.rs               (~150 lines) — ShellState struct definition, AppBootstrap, 
                                     re-exports. Imports from ALL sub-modules.
  types.rs             (~150 lines) — screens/app-level enums:
                                     ScreenState, SettingsCategory, EntryKind, TextAreaSurface,
                                     FocusTarget, InspectorFocus, GenerationPoll,
                                     RuntimePhase, RuntimeCommand,
                                     ContextCompressionEvent, RuntimeSendReceipt,
                                     RuntimeContextRefresh, RuntimeSessionLoad,
                                     RuntimeCompletion, RuntimeCancellation,
                                     RuntimeProgress, RuntimeFailure
  session_types.rs     (~300 lines) — Session-related types:
                                     SessionState, SessionContext, TranscriptItem, BranchItem,
                                     ContextTokenBudget, ContextPreview, ContextDryRunPreview,
                                     RecallBrowser, TuiSessionMemoryMetadata, TuiMemoryView,
                                     SessionMetadata, SessionStats, DraftCheckpoint, DraftState,
                                     CommandEntry, CommandExecution, CommandPaletteState,
                                     MessageEditState
  ui_types.rs          (~400 lines) — UI component types:
                                     SettingsState, SettingsEntry, InspectorState, MenuItem,
                                     MenuState, SessionListEntry, VisibleSessionItem,
                                     SessionListState, FolderPickerState, CharacterEntry,
                                     CharacterDetail, CharacterListState, CharacterFormField,
                                     CharacterCreateState, CharacterImportState,
                                     InputHistoryState
  textarea_util.rs     (~100 lines) — Pure text utilities (NO state dependency):
                                     configure_themed_textarea, new_themed_textarea,
                                     new_themed_textarea_for, themed_textarea_from_text,
                                     clamp_cursor, rect_contains, textarea_cursor_position,
                                     textarea_lines, textarea_cursor_offset, byte_index_for_char,
                                     is_shell_command, parse_local_shell_command
  navigator.rs         (~500 lines) — Pure navigation state mutation:
                                     SettingsCategory, EntryKind, SettingsState, InspectorState,
                                     MenuState, SessionListState, FolderPickerState,
                                     CharacterListState, CharacterFormField,
                                     CharacterCreateState, SessionContext, DraftCheckpoint,
                                     DraftState, InputHistoryState, TranscriptItem, BranchItem,
                                     CommandPaletteState impls
                                     THAT ONLY mutate self (no server calls, no I/O)
  composer.rs          (~400 lines) — Draft/text editing behavior:
                                     DraftState editing impls, cursor movement, slash completions,
                                     message editing, textarea sync, command palette text ops
  dispatcher.rs        (~600 lines) — Event handling + action routing:
                                     handle_key_event, handle_mouse_event, apply_action,
                                     apply_action_with_layout, all apply_* methods for
                                     runtime events (completion, cancellation, progress, etc.)
  session_lifcycle.rs  (~500 lines) — Session-level operations:
                                     enter_conversation, reset_for_new_conversation,
                                     open/send/edit session, branch operations,
                                     swipe operations, context operations
  command_exec.rs      (~500 lines) — Command parsing and execution:
                                     execute_command, all command handling
  shell_state.rs       (~200 lines) — Thin ShellState coordinator:
                                     new(), hydrate(), active_toast(), return_to_menu(),
                                     replace_draft(), sync_draft_from_textarea(),
                                     sync_textarea_from_draft(), show_toast(),
                                     persistable_draft(), take_pending_actions()
```

**Dependency flow (DOWN only, no cycles):**
```
types.rs ← session_types.rs ← ui_types.rs ← navigator.rs
                                                ↓
textarea_util.rs ← composer.rs ← dispatcher.rs ← session_lifecycle.rs
                                   ↑
                             command_exec.rs
                                   ↓
                             shell_state.fs (coordinates all)
```

**Critical constraint:** `navigator.rs` must NOT import `dispatcher.rs` or `session_lifecycle.rs`. It only mutates state. The dispatcher calls INTO the navigator. This prevents the circular dependency trap.

**Execution Steps:**

**Step 1:** `mkdir -p crates/ozone-tui/src/app`

**Step 2:** Extract `types.rs` — All app-level enums + pure event types. Zero dependencies on other app modules.

**Step 3:** Extract `session_types.rs` — Session-related types. Depends only on `types.rs`.

**Step 4:** Extract `ui_types.rs` — UI component types. Depends only on `types.rs`.

**Step 5:** Extract `textarea_util.rs` — Pure text utilities. Zero app dependencies.

**Step 6:** Extract `navigator.rs` — All navigation/mutation impls. Depends on types ONLY.

**Step 7:** Extract `composer.rs` — Text editing impls. Depends on `session_types.rs` + `types.rs`.

**Step 8:** Extract `dispatcher.rs` — Event routing. This is the "traffic cop" — it imports navigator, composer, session_lifecycle, and command_exec. It does NOT export to them.

**Step 9:** Extract `session_lifecycle.rs` — Session operations. Depends on `session_types.rs` + `types.rs`.

**Step 10:** Extract `command_exec.rs` — Command execution. Depends on types.

**Step 11:** Create `shell_state.rs` — Thin coordinator. Imports from all behavioral modules.

**Step 12:** Create `mod.rs` — ShellState struct + re-exports.

**Step 13:** Update `lib.rs` to use new module paths.

**Step 14:** Run `cargo test -p ozone-tui --lib` — Must pass.

**Step 15:** Commit: `refactor(ozone-tui): split app.rs into 11 independent modules`

---

## Phase 4: Main Refactor (Task 5) — Independent, Can Parallelize

**Why now:** Completely independent of app.rs/render.rs. Only depends on `runtime.rs` (and `runtime.rs` depends on `main.rs` types via `use crate::*`).

**Key architectural insight:** `main.rs` mixes 5 concerns: CLI parsing, command handlers, output formatting, database access, and tests. Each goes in its own module.

**Critical warning:** `runtime.rs` has `use crate::*` which depends on types defined in `main.rs`. After the refactor, these become `crate::cli::SendArgs`, etc. This MUST be updated.

### Module Structure

```
apps/ozone-plus/src/
  main.rs              (~150 lines) — Entry point ONLY: main(), run(), run_cli() dispatch.
                                     Module declarations. NO business logic.
  cli.rs               (~400 lines) — ALL CLI argument structs (clap):
                                     Cli, Command, CreateArgs, OpenArgs, HandoffArgs, SendArgs,
                                     TranscriptArgs, EditArgs, BranchCommand, BranchCreateArgs,
                                     BranchActivateArgs, SwipeCommand, SwipeAddArgs,
                                     SwipeActivateArgs, ImportCommand, ImportCharacterCardArgs,
                                     ExportCommand, ExportSessionArgs, ExportTranscriptArgs,
                                     SessionExportFormat, TranscriptExportFormat, MemoryCommand,
                                     MemoryPinArgs, MemoryNoteArgs, MemoryUnpinArgs,
                                     SearchCommand, SummarizeCommand, GcCommand, EventsCommand,
                                     LifecycleCommand, IndexCommand, SessionArgs, GlobalSearchArgs,
                                     SessionSearchArgs, ExportArgs
  session_cmds.rs      (~400 lines) — Session CRUD:
                                     create_session, list_sessions, open_session, handoff_session,
                                     handoff_candidates, create_handoff_session, open_session_record,
                                     run_session_shell, print_bootstrap_summary, print_identity,
                                     print_docs, print_paths, open_session_metadata
  messaging_cmds.rs    (~300 lines) — Messaging:
                                     send_message, send_message_legacy, show_transcript,
                                     edit_message, select helpers
  branch_swipe_cmds.rs (~300 lines) — Branch + swipe:
                                     handle_branch_command, list_branches, create_branch,
                                     activate_branch, handle_swipe_command, list_swipes,
                                     add_swipe_candidate, activate_swipe
  import_export_cmds.rs (~400 lines) — Import/export:
                                     handle_import_command, import_character_card,
                                     handle_export_command, export_session, export_transcript,
                                     render_transcript_text, write_output_file, read_utf8_file,
                                     require_existing_file
  memory_search_cmds.rs (~350 lines) — Memory + search:
                                     handle_memory_command, pin_memory, create_note_memory,
                                     list_memories, unpin_memory, handle_search_command,
                                     search_session, search_global, handle_index_command,
                                     rebuild_vector_index, handle_summarize_command
  lifecycle_cmds.rs    (~400 lines) — GC + lifecycle:
                                     handle_lifecycle_command, lifecycle_inspect, lifecycle_disk_status,
                                     handle_gc_command, gc_plan, gc_run, print_gc_plan,
                                     print_gc_outcome, reason_label, events_compact,
                                     handle_events_command
  output.rs            (~400 lines) — Print/format functions:
                                     print_session_details, print_branch_record,
                                     print_transcript, print_message, print_pinned_memory_view,
                                     format_search_report, format_search_status, format_search_hit,
                                     print_swipe_group_map, print_session_paths,
                                     format_timestamp, format_timestamp_short, format_message_time,
                                     format_author_id, format_tags, print_resolved_path
  util.rs              (~150 lines) — Pure utilities:
                                     parse_session_id, parse_branch_id, parse_message_id,
                                     parse_swipe_group_id, generate_message_id, generate_branch_id,
                                     generate_request_id, generate_swipe_group_id, generate_uuid_like,
                                     is_session_locked_error, optional_value, normalize_tags,
                                     require_non_empty, now_timestamp_ms, print_optional_path
```

**Dependency flow (DOWN only):**
```
cli.rs ← session_cmds.rs
          messaging_cmds.rs
          branch_swipe_cmds.rs
          import_export_cmds.rs
          memory_search_cmds.rs
          lifecycle_cmds.rs
          
          output.rs ← util.rs
          
          main.rs (coordinates all)
```

**Execution Steps:**

**Step 1:** `mkdir -p apps/ozone-plus/src/cli` — NO. Keep flat: `apps/ozone-plus/src/*.rs`.

**Step 2:** Extract `util.rs` — Pure functions. Zero dependencies.

**Step 3:** Extract `cli.rs` — All clap structs. Depends on external crates only.

**Step 4:** Extract `output.rs` — Print functions. Depends on `util.rs` + types from `cli.rs`.

**Step 5:** Extract `session_cmds.rs`, `messaging_cmds.rs`, `branch_swipe_cmds.rs`, `import_export_cmds.rs`, `memory_search_cmds.rs`, `lifecycle_cmds.rs` — One at a time. Each depends on `cli.rs` + `util.rs`.

**Step 6:** Thin out `main.rs` to just entry point + module declarations.

**Step 7:** Update `runtime.rs` imports: Replace `use crate::*` with explicit `crate::cli::X`, `crate::util::Y`.

**Step 8:** Run `cargo test -p ozone-plus --lib` — Must pass.

**Step 9:** Commit: `refactor(ozone-plus): split main.rs into 9 independent modules`

---

## Phase 5: Runtime Refactor (Task 6) — Last, Because It Depends on Main

**Why last:** `runtime.rs` uses `use crate::*` to pull types from `main.rs`. Must complete main.rs first, then fix the imports, then refactor runtime.

**Current:** 3698 lines in one file, two massive impl blocks
- `impl Phase1dRuntime`: ~1117 lines (lines 216-1333)
- `impl SessionRuntime for Phase1dRuntime`: ~2357 lines (lines 1341-3698)

### Module Structure

```
apps/ozone-plus/src/runtime/
  mod.rs               (~150 lines) — Phase1dRuntime struct, re-exports
  types.rs             (~200 lines) — All runtime types:
                                     WorkerEvent, PendingGeneration, PendingCompletion,
                                     PendingReroll, RerollBranchMode, RerollSource, SessionSnapshot,
                                     SessionCommand, MemoryCommand, SearchCommand, ShellCommand,
                                     SummarizeShellCommand, ThinkingCommand, TierBCommand,
                                     HooksCommand, SafeModeCommand, RecentSearchSection,
                                     PendingGeneration impl
  session_ops.rs       (~400 lines) — Session lifecycle (Phase1dRuntime impl):
                                     bootstrap, load_session_into_tui, seed_greeting_if_present,
                                     load_bootstrap, load_session_snapshot, load_persisted_draft,
                                     save_persisted_draft, active_branch, is_tier_b_active,
                                     branch_by_id, build_recall_browser
  generation.rs        (~500 lines) — The generation pipeline:
                                     send_draft, start_generation_task, build_context_for_generation,
                                     build_context_for_transcript, complete_generation,
                                     maybe_auto_title_session, mark_generation_failure,
                                     poll_generation, cancel_generation, set_generation_state
  reroll.rs            (~350 lines) — Reroll logic:
                                     reroll_message, resolve_reroll_source, ensure_reroll_swipe_group,
                                     complete_reroll_generation
  context.rs           (~350 lines) — Context management:
                                     refresh_context_cache, latest_context_plan_preview,
                                     latest_context_dry_run, status_line_context_preview_text,
                                     dry_run_context_build, build_dry_run_context_refresh,
                                     build_session_refresh, build_recall_browser_refresh
  tui_model.rs         (~300 lines) — TUI data formatting:
                                     tui_branch_from_record, tui_transcript_item_from_message,
                                     tui_context_preview_from_plan, tui_context_dry_run_from_build,
                                     tui_recall_browser_from_state, recent_search_section,
                                     format_retrieval_browser_line, format_pinned_memory_browser_line,
                                     compact_line, short_id
  shell.rs             (~300 lines) — Shell command parsing:
                                     parse_shell_command, parse_session_subcommand,
                                     parse_memory_subcommand, parse_search_subcommand,
                                     parse_summarize_subcommand, parse_thinking_subcommand,
                                     parse_tierb_subcommand, parse_hooks_subcommand,
                                     parse_safemode_subcommand, unknown_shell_command_message
  db.rs                (~300 lines) — Database operations (SessionRuntime trait impl):
                                     commit_message, edit_message, create_branch, list_branches,
                                     get_active_branch, activate_branch, record_swipe_candidate,
                                     activate_swipe_candidate, list_swipe_groups,
                                     list_swipe_candidates, list_branch_messages,
                                     get_active_branch_transcript, list_sessions, get_settings,
                                     save_pref, set_session_folder, list_characters,
                                     create_character, update_character, get_character,
                                     import_character, create_session, open_session,
                                     persist_draft
```

**Dependency flow (DOWN only):**
```
types.rs ← session_ops.rs ← generation.rs ← reroll.rs
                                              ↓
                           context.rs ← tui_model.rs
                           
                           shell.rs (independent)
                           db.rs (independent)
                           
                           mod.rs (coordinates all)
```

**Execution Steps:**

**Step 1:** Fix `runtime.rs` imports first: Replace `use crate::*` with explicit imports from the new main sub-modules.

**Step 2:** `mkdir -p apps/ozone-plus/src/runtime`

**Step 3:** Extract `types.rs` — All enums + structs. Zero dependencies.

**Step 4:** Extract `shell.rs` — Pure parsing. Zero dependencies on other runtime modules.

**Step 5:** Extract `tui_model.rs` — Formatting helpers. Depends on `types.rs`.

**Step 6:** Extract `context.rs` — Context management. Depends on `types.rs`.

**Step 7:** Extract `db.rs` — Database operations from SessionRuntime trait impl.

**Step 8:** Extract `generation.rs` — The generation pipeline. Depends on `types.rs`.

**Step 9:** Extract `reroll.rs` — Reroll logic. Depends on `generation.rs` + `types.rs`.

**Step 10:** Extract `session_ops.rs` — Session lifecycle. Depends on `types.rs`.

**Step 11:** Create `mod.rs` — Phase1dRuntime struct + re-exports.

**Step 12:** Run `cargo test -p ozone-plus --lib` — Must pass.

**Step 13:** Commit: `refactor(ozone-plus): split runtime.rs into 9 independent modules`

---

## Phase 6: Post-Refactor Cleanup (1 hour)

### Task 7: Remove Dead Code

**Step 1: Find unused items**
```bash
cargo clippy --workspace --all-targets 2>&l | grep "dead_code\|never used"
```

**Step 2: Remove or `#[allow(dead_code)]` each one**

**Step 3: Run `cargo test --workspace`**

**Step 4:** Commit: `cleanup: remove dead code after refactor`

### Task 8: Consolidate Duplicate Logic

**Step 1: Identify duplicates found during refactor**
```bash
# These were found during analysis:
# - parse_session_id / parse_branch_id / parse_message_id in multiple crates
# - format_tags / normalize_tags / format_timestamp duplicated
# - now_timestamp_ms / generate_uuid_like duplicated
```

**Step 2: Move shared utilities to `crates/ozone-core/src/util.rs` if appropriate**

**Step 3: Update all references**

**Step 4:** Run `cargo test --workspace`

**Step 5:** Commit: `cleanup: consolidate duplicated parse/format utilities`

### Task 9: Rustdoc Coverage

**Step 1: Add `///` docs to all public functions**
Focus on WHY, not WHAT. Include:
- Function purpose
- Parameter meanings
- Return value semantics
- Panics (if any)

**Step 2: Run `cargo doc --workspace --no-deps`**

**Step 3:** Commit: `docs: add rustdoc to all public functions`

---

## Phase 7: Documentation (2 hours)

### Task 10: User-Facing Docs

**Files to create:**
- `docs/getting-started.md` — Install, first model, first chat
- `docs/features.md` — Feature overview with screenshots/ascii art
- `docs/tutorial.md` — Step-by-step walkthrough
- `CONTRIBUTING.md` — Dev setup, architecture overview, how to contribute

**Commit:** `docs: add user-facing guides and contributing.md`

### Task 11: Coverage Threshold

**Modify `.github/workflows/ci.yml`** — Add grcov coverage check with 60% threshold.

**Commit:** `ci: add coverage threshold (60%)`

---

## Verification Checklist (After All Tasks)

```bash
cargo test --workspace 2>&1 | grep "test result:"  # All pass
cargo clippy --workspace -- -D warnings             # Zero warnings
cargo doc --workspace --no-deps                     # No doc errors
```

- [ ] All files under 800 lines
- [ ] No `todo!()` calls remaining
- [ ] Every module has `#[cfg(test)]` with at least one test
- [ ] No `use crate::*` wildcards remaining
- [ ] All public functions have rustdoc
- [ ] CONTRIBUTING.md exists with architecture diagram
- [ ] Help overlay, token count, sort, theme preview all work

---

## Alpha Score Estimate

After completing all tasks:
- **Scalability**: 90/100 (modular architecture, easy to add features)
- **Code Hygiene**: 95/100 (dead code gone, clippy clean, documented)
- **UX**: 88/100 (quick wins shipped, help overlay, first-run tutorial)
- **Ready for Alpha**: 91/100

# Ozone-TUI Refactor Plan

## Overview

The ozone-tui crate has a 6,110-line `app.rs` that mixes state definitions with application logic, and a 4,319-line `render.rs` that mixes render model structs with rendering functions. This plan breaks them into focused, single-responsibility modules.

## Current Status

### Completed
- **State module boundary established** - `state/mod.rs` created with re-exports from `app`
- All 180 tests passing

### Phase 1 Status: COMPLETED (Establishes boundary only)
Due to complex interdependencies between state types and helper functions, Phase 1 was adjusted to establish the module boundary without physically moving code. This is a pragmatic approach that:
- Maintains all existing functionality
- Avoids dependency hell with helper functions
- Provides clear architecture for future work
- All 180 tests continue to pass

**Lesson learned:** The state impl blocks depend on helper functions (`clamp_cursor`, `byte_index_for_char`, etc.) that live in app.rs. Moving state implementations requires moving both the types AND their dependencies together, which is a larger refactor.

---

## Revised Approach

Instead of moving code upfront, establish the module boundary first, then do incremental movement in future sessions when specific areas need modification.

## Target Structure

```
ozone-tui/src/
├── lib.rs              # Re-exports and entry points (871 lines)
├── app.rs              # ShellState + event handling (~2,500 lines)
├── state/
│   ├── mod.rs          # Re-exports all state types
│   ├── enums.rs        # ScreenState, SettingsCategory, EntryKind, FocusTarget, InspectorFocus, InputMode
│   ├── settings.rs     # SettingsState, SettingsEntry, SettingsCategory
│   ├── session.rs      # SessionState, SessionContext, DraftState, DraftCheckpoint, TranscriptItem, BranchItem
│   ├── session_list.rs # SessionListState, SessionListEntry, VisibleSessionItem, FolderPickerState
│   ├── character.rs    # CharacterListState, CharacterEntry, CharacterDetail, CharacterCreateState, CharacterImportState
│   ├── command.rs      # CommandPaletteState, CommandEntry
│   └── input_history.rs # InputHistoryState
├── models/
│   ├── mod.rs          # Re-exports all render models
│   ├── conversation.rs # ConversationEntryModel, ConversationPaneModel
│   ├── composer.rs     # ComposerPaneModel, SlashSuggestion
│   ├── status.rs       # StatusPaneModel, ModelInfoDisplay
│   ├── inspector.rs    # InspectorPaneModel
│   ├── overlays.rs     # OverlayRenderModel, HintItem, CommandPaletteRenderModel
│   ├── menu.rs         # MainMenuRenderModel, MenuItemRenderModel
│   ├── session_list.rs # SessionListRenderModel, FolderPickerRenderModel, SessionListEntryRenderModel
│   ├── character.rs    # CharacterListRenderModel, CharacterDetailRenderModel, CharacterFormRenderModel
│   ├── settings.rs     # SettingsRenderModel
│   ├── intelligence.rs # ModelIntelligenceRenderModel
│   └── shared.rs       # ShellIndicators
├── render/
│   ├── mod.rs          # build_render_model + render_shell entry points
│   ├── conversation.rs # render_conversation + helpers
│   ├── composer.rs     # render_composer + scrollbar
│   ├── status.rs       # render_status
│   ├── inspector.rs    # render_inspector
│   ├── overlays.rs      # render_command_palette, render_slash_popup, render_overlay, render_help_overlay, render_toast
│   ├── menu.rs         # render_main_menu, render_menu_placeholder
│   ├── session_list.rs # render_session_list, render_folder_picker
│   ├── character.rs    # render_character_list, render_character_form
│   ├── settings.rs     # render_settings
│   ├── intelligence.rs # render_model_intelligence
│   └── shared.rs       # render_hints, render_breadcrumb
├── layout.rs           # Layout computation (386 lines - keep as-is)
├── input.rs            # Input handling (547 lines - keep as-is)
├── theme.rs            # Theme/colors (383 lines - keep as-is)
├── mock.rs             # Test utilities (844 lines - keep as-is)
└── state.rs            # Module boundary re-export (7 lines - already done)
```

## Rationale

### Why These Boundaries

1. **state/ enums.rs** - Pure data enums with no implementation beyond accessors. Zero coupling to the rest of the system.

2. **state/ session.rs** - Session lifecycle state. Heavy coupling to session logic, so the impl stays in app.rs.

3. **models/ *.rs** - Render model structs are pure data with no behavior. They map 1:1 with screen regions and are consumed by render functions. Clean separation.

4. **render/ *.rs** - Rendering functions grouped by pane. Each file is ~150-300 lines, focused on one area.

### What's NOT Being Moved

- **ShellState impl** (lines ~1815-6110) stays in app.rs - it's the main event loop handler
- **input.rs** - Already well-structured at 547 lines
- **layout.rs** - Already well-structured at 386 lines
- **theme.rs** - Already well-structured at 383 lines

## Phases

### Phase 1: State Module (Est: 2-3 hours)

**Goal:** Extract all state struct/enum definitions into `state/` submodules.

**Steps:**
1. Create `state/mod.rs` with re-exports
2. Create `state/enums.rs` - Move ScreenState, SettingsCategory, EntryKind, FocusTarget, InspectorFocus, InputMode
3. Create `state/settings.rs` - Move SettingsState, SettingsEntry
4. Create `state/session.rs` - Move SessionContext, DraftCheckpoint, DraftState, TranscriptItem, BranchItem
5. Create `state/session_list.rs` - Move SessionListState, SessionListEntry, VisibleSessionItem, FolderPickerState
6. Create `state/character.rs` - Move CharacterEntry, CharacterDetail, CharacterListState, CharacterCreateState, CharacterImportState, CharacterFormField
7. Create `state/command.rs` - Move CommandPaletteState, CommandEntry
8. Create `state/input_history.rs` - Move InputHistoryState
9. Update `app.rs` imports to use new module paths
10. Update `lib.rs` re-exports
11. Verify: `cargo test -p ozone-tui` passes (180 tests)

**Risk:** Low - purely moving definitions, no logic changes.

**Verification:** All 180 tests pass.

---

### Phase 2: Models Module (Est: 2-3 hours)

**Goal:** Extract render model structs from `render.rs` into `models/` submodules.

**Steps:**
1. Create `models/mod.rs` with re-exports
2. Create `models/conversation.rs` - Move ConversationEntryModel, ConversationPaneModel, ConversationViewport
3. Create `models/composer.rs` - Move ComposerPaneModel, SlashSuggestion
4. Create `models/status.rs` - Move StatusPaneModel, ModelInfoDisplay
5. Create `models/inspector.rs` - Move InspectorPaneModel
6. Create `models/overlays.rs` - Move OverlayRenderModel, HintItem, CommandPaletteRenderModel, CommandPaletteEntry
7. Create `models/menu.rs` - Move MainMenuRenderModel, MenuItemRenderModel
8. Create `models/session_list.rs` - Move SessionListRenderModel, FolderPickerRenderModel, SessionListItemRenderModel, SessionListEntryRenderModel
9. Create `models/character.rs` - Move CharacterListRenderModel, CharacterListEntryRenderModel, CharacterDetailRenderModel, CharacterFormRenderModel, CharacterFieldRenderModel
10. Create `models/settings.rs` - Move SettingsRenderModel
11. Create `models/intelligence.rs` - Move ModelIntelligenceRenderModel
12. Create `models/shared.rs` - Move ShellIndicators
13. Update `render.rs` imports
14. Verify: `cargo test -p ozone-tui` passes

**Risk:** Low - purely moving data struct definitions.

**Verification:** All 180 tests pass.

---

### Phase 3: Render Module (Est: 3-4 hours)

**Goal:** Break rendering functions in `render.rs` into focused submodule files.

**Steps:**
1. Keep `render/mod.rs` with `build_render_model` and `render_shell`
2. Create `render/conversation.rs` - Move render_conversation + helper functions
3. Create `render/composer.rs` - Move render_composer, render_composer_scrollbar
4. Create `render/status.rs` - Move render_status
5. Create `render/inspector.rs` - Move render_inspector
6. Create `render/overlays.rs` - Move render_command_palette, render_slash_popup, render_overlay, render_help_overlay, render_toast
7. Create `render/menu.rs` - Move render_main_menu, render_menu_placeholder
8. Create `render/session_list.rs` - Move render_session_list, render_folder_picker
9. Create `render/character.rs` - Move render_character_list, render_character_form
10. Create `render/settings.rs` - Move render_settings
11. Create `render/intelligence.rs` - Move render_model_intelligence
12. Create `render/shared.rs` - Move render_hints, render_breadcrumb
13. Update `render/mod.rs` to re-export everything
14. Verify: `cargo test -p ozone-tui` passes

**Risk:** Medium - need to ensure all function references and imports are correct.

**Verification:** All 180 tests pass.

---

### Phase 4: App Shell Event Handler (Est: 4-5 hours)

**Goal:** Extract sub-handlers from the massive `ShellState` impl into focused files.

**Steps:**
1. Identify natural handler groups within ShellState impl (menu handling, session handling, character handling, etc.)
2. Create `app/handlers/mod.rs` with handler trait/structs
3. Extract menu handling logic
4. Extract session handling logic
5. Extract character management logic
6. Extract settings handling logic
7. Update ShellState to delegate to handlers
8. Verify: `cargo test -p ozone-tui` passes

**Risk:** High - significant refactoring of event handling logic.

**Verification:** All 180 tests pass + manual smoke test.

---

### Phase 5: Utility Helpers (Est: 1-2 hours)

**Goal:** Extract textarea helpers and other utilities.

**Steps:**
1. Create `app/textarea.rs` - Move TextAreaSurface, new_themed_textarea, configure_themed_textarea, themed_textarea_from_text
2. Create `app/helpers.rs` - Move helper functions (textarea_lines, textarea_cursor_position, clamp_cursor, etc.)
3. Update imports
4. Verify: `cargo test -p ozone-tui` passes

**Risk:** Low - moving helper functions.

---

## Success Criteria

- [ ] All 180 tests pass after each phase
- [ ] No increase in compilation time (incremental benefits visible)
- [ ] Each module has a clear, single responsibility
- [ ] No circular dependencies introduced
- [ ] Public API surface unchanged (lib.rs re-exports remain the same)

## Quick Wins (Immediate)

While implementing the phases above, watch for:

1. **Duplicate token budget formatting** - `format_token_bar` appears in both render.rs and app.rs
2. **Conversation scroll calculation** - Can be extracted to a reusable `ConversationViewport` helper
3. **Breadcrumb formatting** - Appears in multiple places
4. **Toast/notification rendering** - Duplicated pattern

These can be extracted as we encounter them, not as a separate phase.

## Files After Refactor (Estimated)

| File | Current | Target |
|------|---------|--------|
| app.rs | 6,110 | ~2,500 |
| render.rs | 4,319 | ~200 |
| state/*.rs | 0 | ~1,500 |
| models/*.rs | 0 | ~1,200 |
| render/*.rs | 0 | ~1,800 |
| layout.rs | 386 | 386 |
| input.rs | 547 | 547 |
| theme.rs | 383 | 383 |
| mock.rs | 844 | 844 |
| lib.rs | 871 | 871 |
| state.rs | 7 | 50 |

## Rollback Plan

If any phase fails:
1. `git stash` to save progress
2. Identify which file/line caused the failure
3. Revert to last passing state
4. Analyze and adjust approach for that specific module

## Notes

- The `state/` module approach uses re-exports from `app.rs` initially, then migrates definitions over time
- This allows incremental changes without breaking everything at once
- The `models/` module follows the same pattern - re-export from render.rs first, then migrate

# Ozone Monolith Analysis — Module Split Refactor Plan

## Scope
Analyze all non-generated `.rs` files over 500 LOC for refactoring opportunities, determine natural module boundaries, assess coupling and risk, and produce an ordered extraction plan.

---

## 1. Codebase Map — Files Over 500 LOC (excluding target/)

| File | LOC | Crate | Assessment |
|------|-----|-------|------------|
| `crates/ozone-tui/src/app.rs` | 6,110 | ozone-tui | **Primary target** |
| `crates/ozone-tui/src/render.rs` | 4,319 | ozone-tui | **Primary target** |
| `apps/ozone-plus/src/main.rs` | 3,780 | ozone-plus | Secondary target |
| `apps/ozone-plus/src/runtime.rs` | 3,698 | ozone-plus | Secondary target |
| `crates/ozone-persist/src/repository/mod.rs` | 3,120 | ozone-persist | Low priority (already modular) |
| `src/ui/mod.rs` | 2,680 | ozone | Low priority (already modular) |
| `src/ui/launcher.rs` | 2,269 | ozone | Low priority (pure render) |
| `src/profiling.rs` | 1,466 | ozone | Low priority (feature-gated) |
| `crates/ozone-engine/src/context.rs` | 1,270 | ozone-engine | Low priority (single responsibility) |
| `crates/ozone-inference/src/config.rs` | 1,089 | ozone-inference | **Candidate** (config-only, no internal mod) |
| `crates/ozone-memory/src/index.rs` | 686 | ozone-memory | Low priority (single responsibility) |
| `crates/ozone-persist/src/repository/lifecycle_ops.rs` | 1,076 | ozone-persist | Low priority (sub-module) |
| `apps/ozone-plus/src/hybrid_search.rs` | 1,077 | ozone-plus | Low priority (sub-module) |
| `apps/ozone-plus/src/context_bridge.rs` | 842 | ozone-plus | Low priority (sub-module) |

---

## 2. Primary Targets

### 2A. `crates/ozone-tui/src/app.rs` — 6,110 lines

#### Internal Structure
```
Lines 1–35:       Imports
Lines 35–75:      Utility fns (themed_textarea helpers)
Lines 75‘1815:    ALL State struct definitions (∼40 structs)
                  - SettingsState, MenuState, SessionListState, FolderPickerState
                  - CharacterEntry, CharacterDetail, CharacterListState
                  - CharacterCreateState, CharacterImportState
                  - DraftState, InputHistoryState, TranscriptItem, BranchItem
                  - CommandEntry, CommandPaletteState, MessageEditState
                  - ShellState (the main one at line 1766, 50+ fields)
Lines 1815–3521:  impl ShellState (∼1,706 lines — THE MONSTER)
                  - tick, set_status, set_error, command_overlay_query
                  - update_disk, reset_profile_flow, filtered_catalog
                  - ALL screen-mode handlers and state machines
Lines 3521–3594:  Utility fns (clamp_cursor, rect_contains, textarea_*)
Lines 3594–:      mod tests
```

#### Coupling Analysis
- `ShellState` is the central state container. Every screen-mode handler operates on it.
- All `*State` structs are private fields inside `ShellState`. Extracting them as a group is straightforward.
- The `impl ShellState` block is where the monolith lives — 1,706 lines of state management.
- Render functions in `render.rs` reference `ShellState` and the `*State` structs, so any split must maintain cross-module access.

#### Natural Extraction Groups

**Group A — State structs (lines 35–1815, ∼1,780 lines)**
- Extract to `src/state.rs`
- Contains ∼40 struct definitions, each small (15–100 lines)
- No cross-dependencies within the group
- Dependencies: `ozone_core`, `ratatui`, `serde`
- Risk: LOW. Pure data + impl blocks. `ShellState` embeds them; after extraction the imports just move.

**Group B — impl ShellState body (lines 1815–3521, ∼1,706 lines)**
- This is where the monolith lives.
- Natural sub-sections:
  - Lines ∼1815–2100: Shell lifecycle (tick, status, disk monitoring, command overlay)
  - Lines ∼2100–2400: Catalog filtering and selection
  - Lines ∼2400–2700: Settings management
  - Lines ∼2700–3000: Configure/plan/profile management
  - Lines ∼3000–3300: Profiling flow handlers
  - Lines ∼3300–3521: Tick, refresh, overlay commands
- Risk: HIGH. All methods reference each other freely. Cannot slice cleanly without a phased approach.

**Group C — Utility helpers (lines 35–75, 3521–3594, ∼200 lines)**
- `configure_themed_textarea`, `themed_textarea_from_text`
- `clamp_cursor`, `rect_contains`, `textarea_cursor_position`, etc.
- Extract to `src/util.rs` or fold into `state.rs`
- Risk: LOW.

#### Recommended Extraction Order
1. **Extract Group C** (util/helpers) to `src/util.rs` — validates file system, no logic change
2. **Extract Group A** (state structs) to `src/state.rs` — move struct definitions, update `mod` in lib.rs
3. **Defer Group B** — requires interface design for `ShellState` access before splitting

---

### 2B. `crates/ozone-tui/src/render.rs` — 4,319 lines

#### Internal Structure
```
Lines 1–322:     ALL RenderModel struct definitions (17 structs)
                  - ConversationEntryModel, ComposerPaneModel, SlashSuggestion
                  - StatusPaneModel, InspectorPaneModel, ShellIndicators
                  - OverlayRenderModel, CommandPaletteRenderModel
                  - MainMenuRenderModel, SessionListRenderModel
                  - FolderPickerRenderModel, CharacterListRenderModel
                  - CharacterDetailRenderModel, CharacterListEntryRenderModel
                  - SettingsRenderModel, CharacterFormRenderModel
                  - ModelIntelligenceRenderModel
Lines 322–909:   build_render_model + build_* helper functions (∼587 lines)
Lines 909–1152:  build_hints, build_breadcrumb
Lines 1152–1275: pub fn render_shell entry point
Lines 1275–2130: ALL render_* functions (∼855 lines)
                  - render_hints, render_command_palette, render_slash_popup
                  - render_breadcrumb, render_conversation, render_composer
                  - render_status, render_inspector, render_overlay
Lines 2130–2350: render_main_menu + menu helpers
Lines 2350–2659: render_session_list + list helpers
Lines 2659–2723: render_folder_picker
Lines 2723–3039: render_character_list + form + settings
Lines 3039–3272: render_model_intelligence
Lines 3272–3550: Utility label/formatter functions
Lines 3550–:      mod tests
```

#### Natural Extraction Groups

**Group A — Render model structs (lines 1–322, ∼322 lines)**
- Extract to `src/models.rs`
- 17 small structs, each 5–40 lines
- No internal dependencies
- Risk: LOW.

**Group B — Build functions (lines 322–1152, ∼830 lines)**
- Contains `build_render_model` (entry), `build_folder_picker_model`, `build_conversation_content`, `build_hints`, `build_breadcrumb`, rewrap/wrap helpers
- Risk: MEDIUM. Used as entry points from `app.rs` — `build_render_model` is called by the TUI event loop.
- Recommendation: Keep `build_render_model` as the public entry in `render.rs`; extract helpers to `src/models_build.rs`

**Group C — Render functions (lines 1275–3272, ∼2,000 lines)**
- 20+ `render_*` functions, each 30–200 lines
- Natural sub-groups by screen: conversation, composer, status/inspector, overlay, menu, session list, character, settings, model intelligence
- Each render function is self-contained (takes `&mut Frame`, `Rect`, model ref)
- Risk: MEDIUM. Cross-cutting concerns (theme, layout constants) shared across all render functions.
- Recommendation: Extract screen-specific render functions into `src/render/screens/*.rs` after models are split.

**Group D — Utility/label functions (lines 3272–3550, ∼278 lines)**
- `textwrap_simple`, `truncate_str`, `pane_block`, `input_mode_label`, `screen_label`, `focus_label`, `selection_label`, `branch_label`, `runtime_label`, `context_status_line`, `inspector_lines`, `append_context_preview_lines`, `inspector_visibility_label`, `inspector_focus_label`
- Extract to `src/render/helpers.rs`
- Risk: LOW.

#### Recommended Extraction Order
1. **Extract Group A** (model structs) to `src/models.rs` — validates cleanly
2. **Extract Group D** (helpers) to `src/render/helpers.rs` — low risk
3. **Extract Group C screen groups** to `src/render/screens/conversation.rs`, `src/render/screens/menu.rs`, etc. — requires careful API design for shared helpers
4. **Extract Group B** (build helpers) — done last, as it's the entry point

---

## 3. Secondary Targets (ozone-plus CLI)

### 3A. `apps/ozone-plus/src/main.rs` — 3,780 lines

#### Internal Structure
```
Lines 1–50:     Imports
Lines 53–540:   clap definitions (struct Cli + all Args structs, ∼490 lines)
Lines 540–600:  fn main, fn run, fn run_cli
Lines 600–872:  RepoConversationStore impl (∼272 lines)
Lines 706–1872:  ConversationStore trait impl for RepoConversationStore
Lines 872–1135:  Phase1bCliEngine (∼263 lines)
Lines 1135–1214: Bootstrap/output fns (print_bootstrap_summary, print_identity, print_docs, print_paths)
Lines 1214–1645: Command handlers (create_session, list_sessions, open_session, handoff_candidates, etc.)
Lines 1645–1963: Branch/swipe/import/memory handlers
Lines 1963–2373: Search/index/summarize/lifecycle/GC handlers + helpers
Lines 2373–2640: Repository + repository operations
Lines 2571–2650+: format_* output helpers (∼200 lines)
Lines 2640–2900+: GC planning and execution
Lines 2900+:      mod tests
```

#### Natural Extraction Groups

**Group A — CLI/args module** (`src/cli.rs`)
- All clap `struct Cli`, `Command`, and `*Args` definitions (∼490 lines)
- Extract to `mod cli;`
- Risk: LOW. Pure type definitions.

**Group B — RepoConversationStore** (`src/adapters/conversation_store.rs`)
- `RepoConversationStore` struct + `ConversationStore` impl (∼272 lines)
- Adapter wrapping `SqliteRepository` for the `ConversationEngine` trait
- Risk: LOW. Movable.

**Group C — Phase1bCliEngine** (`src/engine_cli.rs`)
- `Phase1bCliEngine` + `EngineCommandResult` handling (∼263 lines)
- Risk: MEDIUM. References `ContextBuildResult`, `SqliteRepository`, `HybridSearchService`.

**Group D — Command handlers** (`src/commands/`)
- Currently ∼450 lines of handler functions in `main.rs`
- Split into: `src/commands/session.rs`, `src/commands/memory.rs`, `src/commands/search.rs`, `src/commands/branch.rs`, `src/commands/gc.rs`, `src/commands/summarize.rs`, `src/commands/export.rs`, `src/commands/lifecycle.rs`, `src/commands/events.rs`
- Each `fn handle_*` function is self-contained
- Risk: MEDIUM. Handlers share repository instances and helper functions.
- The `format_*` output helpers would need to be in a shared `src/commands/output.rs`.

**Group E — GC planning** (`src/gc.rs`)
- `build_gc_policy_and_session`, `gc_plan`, `gc_run`, `print_gc_plan`, `print_gc_outcome`, `reason_label`
- Extractable as a focused module
- Risk: LOW.

---

### 3B. `apps/ozone-plus/src/runtime.rs` — 3,698 lines

#### Internal Structure
```
Lines 1–58:     Imports + WorkerEvent enum
Lines 58–174:   State structs: PendingGeneration, PendingReroll, RerollSource, SessionSnapshot, RecentSearchSection
Lines 174–3698: impl Phase1dRuntime + SessionRuntime trait impl (∼3,524 lines)
                  - Phase1dRuntime fields
                  - tokio worker event loop (run_until_phase1d)
                  - Shell command dispatching
                  - Session shell REPL loop (parse_*_subcommand functions)
Lines 2497–2640: TUI conversion helpers (tui_*_from_* functions)
Lines 2640–2791: Display helpers (format_retrieval_browser_line, format_pinned_memory_browser_line)
Lines 2791–3011: Shell command parsers (parse_shell_command, parse_session_subcommand, etc.)
Lines 3011–:      mod tests
```

#### Natural Extraction Groups

**Group A — Worker types** (`src/worker.rs`)
- `WorkerEvent`, `PendingGeneration`, `PendingReroll`, `RerollSource`, `SessionSnapshot`, `RecentSearchSection`
- ∼116 lines of state structs
- Risk: LOW.

**Group B — TUI conversion helpers** (`src/tui_convert.rs`)
- `tui_branch_from_record`, `tui_transcript_item_from_message`, `tui_context_preview_from_plan`, `tui_recall_browser_from_state`
- ∼143 lines of pure conversion functions
- Risk: LOW.

**Group C — Shell command parsers** (`src/shell_parsers.rs`)
- 7 `parse_*_subcommand` functions + `unknown_shell_command_message`
- ∼164 lines
- Risk: LOW. Pure functions.

**Group D — Display helpers** (`src/display.rs`)
- `compact_line`, `short_id`, `hit_suffix`, `format_retrieval_browser_line`, `format_pinned_memory_browser_line`, `format_tags`, `normalize_tags`, `require_non_empty`
- ∼200 lines
- Risk: LOW.

**Group E — Phase1dRuntime** (remains in `runtime.rs`)
- The core session runtime (∼1,200 lines of `impl Phase1dRuntime`)
- The `SessionRuntime` trait impl is also here.

**Note**: `impl Phase1dRuntime` is 3,524 lines alone. The natural sections within it are:
- Phase 1d worker loop (∼400 lines)
- Shell command dispatch (∼300 lines)
- Generation handling (rerolls, pending completions, etc.) (∼500 lines)
- Session shell REPL loop (∼300 lines)
- All other state management (∼2,000 lines of misc. helpers and state machines)

The `impl Phase1dRuntime` body is too interwoven to split further without a major interface refactor.

---

## 4. Low-Priority Targets

### `crates/ozone-persist/src/repository/mod.rs` — 3,120 lines
**Already well-modularized.** Has 8 sub-modules: `branch_ops`, `character_ops`, `export_ops`, `lifecycle_ops`, `memory_ops`, `message_ops`, `session_ops`, `swipe_ops`. The `mod.rs` itself has a thin orchestration layer. Risk: LOW. Could extract helpers to `repo/util.rs`, but payoff is small.

### `src/ui/mod.rs` — 2,680 lines
**Already well-modularized.** Has `mod launcher`, `mod monitor`, `mod splash`, `mod tier_install`, `mod tier_picker`. The `App` struct is a focused state container (76 fields, ∼10 logical areas). Splitting it would require extracting each logical area as a sub-state. Possible but risky — `App` is passed by `&mut App` throughout the UI.

### `src/ui/launcher.rs` — 2,269 lines
**Pure render.** All public `render_*` functions. Could extract screen-specific renders to `ui/launcher/*.rs`, but the file is already clean — single responsibility. Low payoff.

### `src/profiling.rs` — 1,466 lines
**Feature-gated.** Only compiled with `profiling-ui` feature. Could extract to `profiling/types.rs` and `profiling/advisory.rs`, but risk/reward is low.

### `crates/ozone-engine/src/context.rs` — 1,270 lines
**Appropriately sized.** Single `ContextLayer` + `ContextPlan` responsibility. Functions are tightly coupled. Not a candidate for further splitting.

### `crates/ozone-inference/src/config.rs` — 1,089 lines
**Monolithic config.** Contains ∼15 config structs + `ConfigLoader`. Has natural boundaries by config domain:
- `BackendConfig`, `RateLimitConfig`, `BackendHealthConfig` → `backend.rs`
- `MemoryConfig`, `TierBConfig`, `MemoryLifecycleConfig`, `StorageTierPolicy`, `StaleArtifactPolicy`, `GarbageCollectionPolicy` → `memory.rs`
- `ContextConfig`, `TasksConfig`, `SummaryConfig` → `context.rs`
- `MetaConfig`, `LoggingConfig` → `meta.rs`
- `OzoneConfig`, `ConfigLoader` → stay in `config.rs` (assembles the above)

This is a clean extraction with minimal coupling between config groups. Config structs are independent; only `ConfigLoader` couples them. Worth fixing in a dedicated config-cleanup session, not as part of the primary monolith split.

### `crates/ozone-memory/src/index.rs` — 686 lines
**Appropriate.** `VectorIndexManager` is a focused, single-responsibility struct. Not a split candidate.

---

## 5. Recommended Extraction Sequence

### Phase 1 — ozone-tui state extraction (app.rs)
**Goal**: Extract state structs from `app.rs` into a `state.rs` submodule.

```
crates/ozone-tui/src/
  lib.rs              # update mod declarations
  state.rs            # NEW: all *State structs moved here (∼1,780 lines)
  app.rs              # update imports; ShellState still defined here
  render.rs
  input.rs
```

Steps:
1. Create `src/state.rs`
2. Move ALL `*State` struct definitions (lines ∼35–1815) to `state.rs`
3. Update `mod state;` in `lib.rs`
4. Run tests — must pass
5. Commit

**Risk**: LOW. Pure struct + impl moves.

### Phase 2 — ozone-tui utility extraction
**Goal**: Extract helpers from `app.rs` and `render.rs`.

```
crates/ozone-tui/src/
  util.rs             # NEW: textarea helpers, clamp_cursor, etc.
  state.rs
  app.rs              # update imports
  render/
    mod.rs            # NEW: export helpers
    helpers.rs        # NEW: render label/format helpers (∼278 lines)
  models.rs           # NEW: render model structs (∼322 lines)
  render.rs           # update imports
```

Steps:
1. Create `src/util.rs`, move helper functions from `app.rs`
2. Create `src/render/helpers.rs`, move label/format helpers from `render.rs`
3. Create `src/render/models.rs`, move model struct definitions
4. Run tests
5. Commit

**Risk**: LOW. Pure function moves.

### Phase 3 — ozone-plus command extraction (main.rs)
**Goal**: Break `main.rs` into sub-modules within the `ozone-plus` binary.

```
apps/ozone-plus/src/
  main.rs              # CLI entry, clap structs, run_cli only
  cli.rs               # NEW: all Cli/Command/*Args structs (∼490 lines)
  adapters/
    conversation_store.rs  # NEW: RepoConversationStore (∼272 lines)
  engine_cli.rs        # NEW: Phase1bCliEngine (∼263 lines)
  commands/
    mod.rs            # NEW
    session.rs        # NEW: create/list/open/send/handoff session handlers
    memory.rs         # NEW: pin/note/unpin/list memory handlers
    search.rs         # NEW: search handlers
    branch.rs         # NEW: branch/swipe handlers
    gc.rs             # NEW: gc_plan, gc_run, print_gc_* (∼300 lines)
    summarize.rs      # NEW: summarize_session, summarize_chunk
    export.rs         # NEW: export_session, export_transcript
    lifecycle.rs      # NEW: lifecycle_inspect, lifecycle_disk_status
    events.rs         # NEW: events_compact
  runtime.rs           # stays, Phase1dRuntime remains central
  hybrid_search.rs     # stays
  context_bridge.rs    # stays
```

Steps (per sub-module):
1. Extract each command group to `src/commands/{group}.rs`
2. Extract `RepoConversationStore` to `src/adapters/conversation_store.rs`
3. Extract `Phase1bCliEngine` to `src/engine_cli.rs`
4. Extract clap structs to `src/cli.rs`
5. Run tests after each extraction
6. Commit per extraction

**Risk**: MEDIUM. Repository instances and helpers are shared across handlers.

### Phase 4 — ozone-plus runtime extraction (runtime.rs)
**Goal**: Break `runtime.rs` into focused sub-modules.

```
apps/ozone-plus/src/
  runtime.rs           # Phase1dRuntime impl (∼1,200 lines of core)
  worker.rs            # NEW: WorkerEvent, PendingGeneration, RerollSource state structs
  shell_parsers.rs     # NEW: parse_shell_command, parse_*_subcommand (∼164 lines)
  tui_convert.rs       # NEW: tui_*_from_* conversion helpers (∼143 lines)
  display.rs           # NEW: format_retrieval_browser_line, format_tags, etc.
  hybrid_search.rs     # stays
  context_bridge.rs    # stays
```

**Note**: The core `impl Phase1dRuntime` (∼3,500 lines) is too interwoven to split further without a major redesign. This phase extracts only the peripheral helpers.

**Risk**: MEDIUM. Display and TUI convert helpers are used by `Phase1dRuntime`.

### Phase 5 — Optional: config directory split
**Goal**: Split `ozone-inference/src/config.rs` into config groups.

```
crates/ozone-inference/src/
  config/
    mod.rs            # ConfigLoader + OzoneConfig re-exports
    backend.rs        # BackendConfig, RateLimitConfig, BackendHealthConfig
    memory.rs         # MemoryConfig, TierBConfig, MemoryLifecycleConfig, StorageTierPolicy
    context.rs        # ContextConfig, TasksConfig, SummaryConfig
    meta.rs           # MetaConfig, LoggingConfig
  gateway.rs
  stream.rs
  templates.rs
```

**Risk**: LOW. Config structs are independent; only `ConfigLoader` couples them.

---

## 6. Files That Are NOT Candidates

| File | Reason |
|------|--------|
| `src/ui/mod.rs` | Already has 5 sub-modules; `App` is a focused state container |
| `src/ui/launcher.rs` | Pure render — 2,269 lines of `render_*` functions, already single-responsibility |
| `src/profiling.rs` | Feature-gated, ∼1,466 lines, no strong split motivation |
| `crates/ozone-engine/src/context.rs` | Single responsibility, 1,270 lines appropriate for its role |
| `crates/ozone-memory/src/index.rs` | `VectorIndexManager` is a clean single-responsibility struct |
| `crates/ozone-persist/src/repository/mod.rs` | Already has 8 sub-modules; `mod.rs` is a thin orchestration layer |
| `crates/ozone-engine/src/lib.rs` | Already modular (context.rs + thinking.rs) |

---

## 7. Risk Matrix

| Extraction | Risk | Test Surface | Complexity |
|-----------|------|-------------|------------|
| app.rs → state.rs | LOW | State structs tested via ShellState tests | Trivial |
| app.rs → util.rs | LOW | Pure helpers | Trivial |
| render.rs → models.rs | LOW | Model structs tested via render model tests | Trivial |
| render.rs → helpers.rs | LOW | Pure helpers | Trivial |
| main.rs → cli.rs | LOW | Pure type definitions | Trivial |
| main.rs → engine_cli.rs | MEDIUM | Phase1bCliEngine integration tested | Medium |
| main.rs → commands/*.rs | MEDIUM | Command handlers, shared helpers | Medium |
| runtime.rs → worker.rs | LOW | State structs | Trivial |
| runtime.rs → display.rs | LOW | Pure formatters | Trivial |
| runtime.rs → tui_convert.rs | LOW | Pure converters | Trivial |
| config.rs → config/*.rs | LOW | Config structs | Trivial |

---

## 8. Quick Stats

- Total source LOC (excluding tests, target/, generated): ∼55,000
- Primary target LOC (app.rs + render.rs): 10,429
- Secondary target LOC (main.rs + runtime.rs): 7,478
- Combined target (all extractable): 17,907 lines
- Already well-modularized: ∼37,000 lines

**Net**: ∼10,400 lines need extraction (app.rs + render.rs), ∼7,500 more for secondary targets.

---

## 9. Quick UX/UI Wins Identified

**1. Render Model Extraction (Low Risk, High Impact)**
- `render.rs` has 17 clean render model structs that can be extracted to `src/models.rs`
- This immediately clarifies the data flow from `ShellState` → `RenderModel` → `render_*` functions
- Users will see cleaner code organization without breaking existing behavior

**2. Utility Extraction (Immediate Benefit)**
- `app.rs` has 200 lines of helper functions (`configure_themed_textarea`, `clamp_cursor`, etc.)
- Extracting to `src/util.rs` makes these reusable across the codebase
- Reduces cognitive load when reading the main state logic

**3. Config Cleanup (Major UX Improvement)**
- `ozone-inference/src/config.rs` is 1,089 lines of unstructured config
- Natural grouping: `backend.rs`, `memory.rs`, `context.rs`, `meta.rs`
- This is the cleanest extraction in the codebase — zero coupling between config domains
- Users will get much better discoverability of configuration options

**Recommended Quick-Win Sequence**:
1. Extract render models (`render.rs` → `models.rs`) — validates render architecture
2. Extract state structs (`app.rs` → `state.rs`) — validates state architecture  
3. Extract config groups (`config.rs` → `config/` directory) — biggest UX win
4. Extract utilities (`app.rs` → `util.rs`, `render.rs` → `helpers.rs`)

**Total Quick-Win LOC**: ~4,000 lines extracted cleanly with minimal risk
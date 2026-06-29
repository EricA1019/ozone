# Ozone+ Production Hardening Plan

> **For Hermes:** Use subagent-driven-development skill to implement this plan task-by-task.

**Goal:** Fix the character greeting system, make memories visible and manageable, and evolve the TUI toward a genuine Vim/Helix-style editor feel for chat sessions.

**Architecture:** Four interlocking improvements: (1) fix character→session greeting injection so every new chat starts with the character's opening line, (2) add a first-class Memory Inspector panel that sits alongside the conversation and shows all pinned memories with edit/unpin controls, (3) add a context counter bar showing real-time context usage with compression/freespace indicators, (4) extend the existing input mode system (Normal/Insert/Command) with Vim/Helix-style quality-of-life: mode indicators, count prefixes, text objects, window commands, and better Normal-mode motions.

---

## PART 0: Pre-flight

Before any task, run:
```bash
cd /home/eric/projects/ozone-rs
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```
Fix every warning and failing test before touching anything else.

---

## PART 1: Fix Character Greeting Injection

**Root cause:** `runtime.rs::create_session()` calls `repo.create_session()` which only writes a `SessionSummary` record with the character name. It never looks up the character card's `greeting` field and never seeds it as the first assistant message in a new branch. The greeting-seeding logic only exists in `import_character_card()` which is a completely separate flow.

**Fix:** Make `create_session` in the runtime do what `import_character_card` does: look up the character card by name, and if it has a greeting, seed it as an assistant message with a new active branch.

### Task 1: Add `get_character_by_name` to SqliteRepository

**Objective:** Query a character card by name from the global library.

**Files:**
- Modify: `crates/ozone-persist/src/repository/character_ops.rs`

**Step 1: Write failing test**
```rust
#[test]
fn get_character_by_name_returns_card_when_found() {
    let repo = SqliteRepository::new_in_memory();
    repo.create_character("Alice", "A cheerful guide", "You are Alice.").unwrap();
    let found = repo.get_character_by_name("Alice").unwrap();
    assert_eq!(found.name, "Alice");
}

#[test]
fn get_character_by_name_returns_none_when_not_found() {
    let repo = SqliteRepository::new_in_memory();
    let found = repo.get_character_by_name("Nobody").unwrap();
    assert!(found.is_none());
}
```

**Step 2: Run test to verify failure**
```
cargo test -p ozone-persist get_character_by_name
# Expected: compile error — function does not exist
```

**Step 3: Implement the function**
Add to `crates/ozone-persist/src/repository/character_ops.rs`:
```rust
pub fn get_character_by_name(&self, name: &str) -> Result<Option<StoredCharacter>> {
    let conn = self.ensure_global_connection()?;
    let mut stmt = conn.prepare(
        "SELECT card_id, name, description, system_prompt, personality, scenario, greeting, example_dialogue, created_at, updated_at
         FROM character_cards
         WHERE name = ?1
         LIMIT 1",
    )?;
    let result = stmt
        .query_row([name], |row| {
            Ok(StoredCharacter {
                card_id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                system_prompt: row.get(3)?,
                personality: row.get(4)?,
                scenario: row.get(5)?,
                greeting: row.get(6)?,
                example_dialogue: row.get(7)?,
                created_at: row.get(8)?,
                updated_at: row.get(9)?,
            })
        })
        .optional()?;
    Ok(result)
}
```

**Step 4: Run tests to verify pass**
```bash
cargo test -p ozone-persist get_character_by_name
# Expected: 2 passed
```

**Step 5: Commit**
```bash
git add crates/ozone-persist/src/repository/character_ops.rs
git commit -m "feat(persist): add get_character_by_name to SqliteRepository"
```

---

### Task 2: Add `seed_greeting_if_present` helper to SqliteRepository

**Objective:** Extract the greeting-seed logic from `import_character_card` into a reusable function.

**Files:**
- Modify: `crates/ozone-persist/src/repository/session_ops.rs`

**Step 1: Write failing test**
```rust
#[test]
fn seed_greeting_if_present_creates_branch_with_greeting_message() {
    let repo = SqliteRepository::new_in_memory();
    let session = repo.create_session(CreateSessionRequest::new("Test")).unwrap();
    let character = repo.create_character_full(
        "Bob", "A robot", "You are Bob.",
        "", "", "", "Hello, human!", "",
    ).unwrap();

    let (branch_id, message_id) = repo
        .seed_greeting_if_present(&session.session_id, &character)
        .unwrap()
        .expect("character has greeting");

    let branch = repo.get_branch(&session.session_id, &branch_id).unwrap().unwrap();
    assert_eq!(branch.branch.tip_message_id.as_str(), message_id.as_str());
    let messages = repo.list_messages(&session.session_id).unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].author_kind, "assistant");
    assert_eq!(messages[0].content, "Hello, human!");
}
```

**Step 2: Run test — expect failure (function does not exist)**

**Step 3: Implement**
Extract from `import_character_card` into a new `pub fn seed_greeting_if_present`:
```rust
/// If the character has a non-empty greeting, creates an assistant message with
/// that greeting and a new Active branch with the message as its tip.
/// Returns (branch_id, message_id) if greeting was seeded, None otherwise.
pub fn seed_greeting_if_present(
    &self,
    session_id: &SessionId,
    character: &StoredCharacter,
) -> Result<Option<(BranchId, MessageId)>> {
    let greeting = character.greeting.trim();
    if greeting.is_empty() {
        return Ok(None);
    }

    let message = self.insert_message(
        session_id,
        CreateMessageRequest {
            parent_id: None,
            author_kind: "assistant".to_owned(),
            author_name: Some(character.name.clone()),
            content: greeting.to_owned(),
        },
    )?;
    let message_id = MessageId::parse(message.message_id.clone())?;
    let branch_id = BranchId::parse(generate_uuid_like())?;
    let mut branch = ConversationBranch::new(
        branch_id.clone(),
        session_id.clone(),
        "main",
        message_id.clone(),
        message.created_at,
    );
    branch.state = BranchState::Active;
    self.create_branch(CreateBranchCommand {
        branch,
        forked_from: message_id.clone(),
    })?;
    Ok(Some((branch_id, message_id)))
}
```

**Step 4: Run test — expect pass**

**Step 5: Commit**
```bash
git add crates/ozone-persist/src/repository/session_ops.rs
git commit -m "feat(persist): extract seed_greeting_if_present helper"
```

---

### Task 3: Wire greeting seeding into `create_session` flow

**Objective:** When the runtime calls `create_session` with a character name, look up the character and seed the greeting.

**Files:**
- Modify: `apps/ozone-plus/src/runtime.rs`

**Step 1: Write integration test stub**
Add a test in `apps/ozone-plus/src/runtime.rs` (in the existing `#[cfg(test)]` module or a new one) that creates a session with a character that has a greeting, then verifies the resulting transcript has one assistant message.

**Step 2: Run test — expect failure**

**Step 3: Modify `create_session` in Phase1dRuntime**
Find the `create_session` fn (~line 2385) and update it:
```rust
fn create_session(
    &mut self,
    character_name: Option<&str>,
) -> Result<TuiRuntimeSessionLoad, Self::Error> {
    let mut request = CreateSessionRequest::new(session_title::DEFAULT_SESSION_TITLE);
    let char_name = character_name
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_owned);
    request.character_name = char_name.clone();

    let session = self
        .repo
        .create_session(request)
        .map_err(|error| error.to_string())?;

    // Seed greeting if character has one
    if let Some(ref name) = char_name {
        if let Ok(Some(character)) = self.repo.get_character_by_name(name) {
            if let Ok(Some((_branch_id, _message_id))) =
                self.repo.seed_greeting_if_present(&session.session_id, &character)
            {
                // Greeting was seeded — branch already created
            }
        }
    }

    self.load_session_into_tui(session.session_id)
}
```

**Step 4: Run tests to verify pass**

**Step 5: Commit**
```bash
git add apps/ozone-plus/src/runtime.rs
git commit -m "fix(ozone-plus): seed character greeting when creating session"
```

---

## PART 2: Memory Inspector Panel

**Root cause:** Pinned memories are invisible by default. `Ctrl+K` toggles them for a selected message, `/memory list` shows a recall browser in the status bar, and the Inspector pane has a one-line placeholder. Nothing gives the user a dedicated, always-visible panel showing all session memories with edit/unpin capability.

**Fix:** Extend the Inspector pane with a dedicated **Memory** focus tab. `Tab` cycles Inspector focus between Summary / Branches / **Memory** / Message / Recall. The Memory tab shows all pinned memories (with provenance, creation time, and text) and note memories in a scannable list, with `d` to unpin and `e` to edit.

### Task 4: Add `InspectorFocus::Memory` variant

**Objective:** Extend the Inspector focus enum to include a Memory tab.

**Files:**
- Modify: `crates/ozone-tui/src/app.rs` (find `InspectorFocus` enum)

**Step 1: Find and modify the enum**
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InspectorFocus {
    Summary,
    Branches,
    Memory,   // NEW
    Message,
    Recall,
}
```

Add a helper:
```rust
impl InspectorFocus {
    pub fn next(&mut self) {
        *self = match self {
            InspectorFocus::Summary => InspectorFocus::Branches,
            InspectorFocus::Branches => InspectorFocus::Memory,
            InspectorFocus::Memory => InspectorFocus::Message,
            InspectorFocus::Message => InspectorFocus::Recall,
            InspectorFocus::Recall => InspectorFocus::Summary,
        };
    }
}
```

**Step 2: Add keybinding — Tab cycles Inspector focus**
In `crates/ozone-tui/src/input.rs`, add to Normal mode:
```rust
KeyCode::Tab => KeyAction::CycleInspectorFocus,
```

Add `CycleInspectorFocus` to `KeyAction` enum, then handle it in `app.rs`:
```rust
KeyAction::CycleInspectorFocus => {
    if state.inspector.visible {
        state.inspector.focus.next();
        state.status_line = Some(format!("inspector: {:?}", state.inspector.focus));
    }
}
```

**Step 3: Verify compilation**
```bash
cargo check --workspace
```

**Step 4: Commit**
```bash
git add crates/ozone-tui/src/app.rs crates/ozone-tui/src/input.rs
git commit -m "feat(tui): add InspectorFocus::Memory and Tab cycling"
```

---

### Task 5: Populate memory data for the Inspector Memory tab

**Objective:** When Inspector focus is `Memory`, the `inspector_lines()` function should return full pinned memory entries, not just a placeholder.

**Files:**
- Modify: `crates/ozone-tui/src/render.rs`

**Step 1: Extend `InspectorPaneModel`**
In the struct definition, add a new field (add to existing struct near line 88):
```rust
pub memory_lines: Option<Vec<String>>,
```

**Step 2: Populate it in `build_render_model`**  
Find where `inspector` is constructed in `render.rs` (~line 521) and add:
```rust
let memory_lines = if state.inspector.focus == InspectorFocus::Memory {
    Some(build_memory_inspector_lines(&state))
} else {
    None
};
let inspector = layout.inspector.map(|_| InspectorPaneModel {
    // ... existing fields ...
    memory_lines,
});
```

**Step 3: Write `build_memory_inspector_lines`**
Add new function near `inspector_lines`:
```rust
fn build_memory_inspector_lines(state: &ShellState) -> Vec<String> {
    let mut lines = vec![];

    // Pinned message memories
    let pinned = state
        .session_metadata
        .as_ref()
        .and_then(|m| m.pinned_memories.as_ref());

    match pinned {
        Some(memories) if !memories.is_empty() => {
            lines.push("── Pinned Memories ──────────────────".into());
            for (i, mem) in memories.iter().enumerate() {
                lines.push(format!("{}. {}", i + 1, mem.text));
                lines.push(format!("   pinned {} · provenance: {}", mem.created_at, mem.provenance));
            }
        }
        _ => {
            lines.push("── Pinned Memories ──────────────────".into());
            lines.push("  (no pinned memories)".into());
            lines.push("  Use Ctrl+K on a message to pin it".into());
        }
    }

    // Note memories
    let notes = state
        .session_metadata
        .as_ref()
        .and_then(|m| m.note_memories.as_ref());

    match notes {
        Some(notes) if !notes.is_empty() => {
            lines.push("".into());
            lines.push("── Notes ────────────────────────────".into());
            for (i, note) in notes.iter().enumerate() {
                lines.push(format!("{}. {}", i + 1, note.text));
            }
        }
        _ => {}
    }

    lines
}
```

**Note:** You need to add `pinned_memories: Vec<PinnedMemoryView>` and `note_memories: Vec<PinnedMemoryView>` to `SessionMetadata` in `app.rs` (or fetch them from the runtime). Since the runtime already has access to these via `repo.list_pinned_memories()`, the cleanest path is to add them to `TuiBootstrap` so they're populated at session load time.

**Step 4: Add to TuiBootstrap**
In `apps/ozone-plus/src/runtime.rs`, in `load_bootstrap()`, add:
```rust
let pinned_memories = self.repo.list_pinned_memories(&context.session_id).unwrap_or_default();
let note_memories = self.repo.list_note_memories(&context.session_id).unwrap_or_default();
let memory_metadata = TuiSessionMemoryMetadata { pinned_memories, note_memories };
```

Add `TuiSessionMemoryMetadata` to the `TuiBootstrap` struct.

**Step 5: Pass through the pipeline**  
Ensure `SessionMetadata` in `crates/ozone-tui/src/app.rs` carries `memory_metadata: Option<TuiSessionMemoryMetadata>` and populate it from the bootstrap.

**Step 6: Verify compilation and render**
```bash
cargo check --workspace
```

**Step 7: Commit**
```bash
git add crates/ozone-tui/src/render.rs crates/ozone-tui/src/app.rs apps/ozone-plus/src/runtime.rs
git commit -m "feat(tui): Memory Inspector tab with full pinned memory list"
```

---

### Task 6: Add `d` to unpin and `e` to edit memory in Memory Inspector

**Objective:** Make memories actionable from the Inspector.

**Files:**
- Modify: `crates/ozone-tui/src/input.rs`, `crates/ozone-tui/src/app.rs`

**Step 1: Add actions to `KeyAction`**
```rust
MemoryUnpin,
MemoryEdit,
```

**Step 2: Map `d` and `e` in Normal mode when Inspector is focused**
In `dispatch_key`, when `state.inspector.visible` and focus is Memory:
```rust
KeyCode::Char('d') => KeyAction::MemoryUnpin,
KeyCode::Char('e') => KeyAction::MemoryEdit,
```

**Step 3: Handle in app.rs**
```rust
KeyAction::MemoryUnpin => {
    if let Some(selected) = state.inspector.selected_memory_index {
        if let Some(ref mem) = state.session_metadata.as_ref()
            .and_then(|m| m.memory_metadata.as_ref())
            .and_then(|m| m.pinned_memories.get(selected))
        {
            self.runtime_commands.push(RuntimeCommand::UnpinMemory {
                artifact_id: mem.artifact_id.clone(),
            });
        }
    }
}
```

Similar for `MemoryEdit` — open an edit form in the overlay.

**Step 4: Commit**
```bash
git add crates/ozone-tui/src/input.rs crates/ozone-tui/src/app.rs
git commit -m "feat(tui): Memory Inspector: d=unpin e=edit actions"
```

---

## PART 3: VIM/Helix-Style Input Quality of Life

**Root cause:** The input system has the right bones (Normal/Insert/Command modes, hjkl) but lacks the polish that makes Vim/Helix feel ergonomic: mode indicator in the status bar, count prefixes, proper text objects, operator-motion, window commands.

### Task 7: Add mode indicator to status bar

**Objective:** The status bar should always show the current input mode with distinctive styling, like Vim's `-- INSERT --` or Helix's mode tag.

**Files:**
- Modify: `crates/ozone-tui/src/render.rs` (status bar rendering)

**Step 1: Find status bar rendering**
In `render.rs`, locate `fn render_status` (~line 1811). Extend the `StatusPaneModel` or add a mode label field.

**Step 2: Add mode label to StatusPaneModel**
Add to the struct:
```rust
pub input_mode_label: String,
pub input_mode_style: String, // "normal" | "insert" | "command"
```

**Step 3: Populate in `build_render_model`**
```rust
let (mode_label, mode_style) = match state.input_mode {
    InputMode::Normal => ("NORMAL", "cyan"),
    InputMode::Insert => ("INSERT", "green"),
    InputMode::Command => ("COMMAND", "yellow"),
};
```

**Step 4: Render in status bar**
In `render_status`, add at the left edge of the status bar:
```rust
Line::from(vec![
    Span::styled(format!(" {} ", mode_label),
        Style::default()
            .fg(match mode_style {
                "cyan" => Color::Cyan,
                "green" => Color::Green,
                "yellow" => Color::Yellow,
                _ => Color::White,
            })
            .add_modifier(Modifier::Bold))
])
```

**Step 5: Commit**
```bash
git add crates/ozone-tui/src/render.rs
git commit -m "feat(tui): add mode indicator (NORMAL/INSERT/COMMAND) to status bar"
```

---

### Task 8: Count prefix support (e.g., `3j`, `5k`)

**Objective:** Allow prefixing navigation commands with a count in Normal mode, matching Vim/Helix behavior. When user types `3j`, scroll down 3 lines.

**Files:**
- Modify: `crates/ozone-tui/src/app.rs` (add count state), `crates/ozone-tui/src/input.rs`

**Step 1: Add count to ShellState**
In `ShellState` (app.rs ~line 1710), add:
```rust
pub normal_mode_count: Option<u32>,
```

Initialize to `None` in `ShellState::new()`.

**Step 2: Modify `dispatch_key` for Normal mode**
Change Normal mode so digits accumulate the count:
```rust
InputMode::Normal => match key.code {
    KeyCode::Char(ch) if ch.is_ascii_digit() && ch != '0' => {
        KeyAction::AccumulateCount(ch.to_digit(10).unwrap())
    }
    KeyCode::Char('j') => KeyAction::ScrollConversationDown,
    // ...
}
```

**Step 3: Add `AccumulateCount` to KeyAction and handle it**
```rust
KeyAction::AccumulateCount(digit) => {
    state.normal_mode_count = Some(
        state.normal_mode_count.unwrap_or(0) * 10 + digit
    );
    state.status_line = Some(format!("{}: ", state.normal_mode_count.unwrap()));
}
```

**Step 4: Apply count to scroll actions**
When handling `ScrollConversationUp/Down`, multiply by `state.normal_mode_count.unwrap_or(1)`. Clear the count after use.

**Step 5: Commit**
```bash
git add crates/ozone-tui/src/app.rs crates/ozone-tui/src/input.rs
git commit -m "feat(tui): count prefix support (3j, 5k) in Normal mode"
```

---

### Task 9: Window/pane commands (`Ctrl+W` + `h/j/k/l`)

**Objective:** Add `Ctrl+W` prefix for pane navigation, matching Vim's window commands. `Ctrl+W h` focuses the pane to the left, `Ctrl+W j` focuses the one below, etc. This is critical for Helix-style multi-pane navigation.

**Files:**
- Modify: `crates/ozone-tui/src/input.rs`, `crates/ozone-tui/src/app.rs`

**Step 1: Add pane navigation actions**
```rust
KeyAction::PaneLeft,
KeyAction::PaneDown,
KeyAction::PaneUp,
KeyAction::PaneRight,
KeyAction::PanePrefix,  // Ctrl+W received — next key is pane target
```

**Step 2: Implement `PanePrefix` state in app state**
```rust
pub pane_prefix_active: bool,
```

When `KeyAction::PanePrefix` fires, set `state.pane_prefix_active = true` and set a short timer or expect the next key immediately. If next key is `h/j/k/l`, dispatch pane movement. If `Ctrl+C`, cancel.

**Step 3: Implement pane focus**
```rust
KeyAction::PaneLeft => {
    state.focus = match state.focus {
        FocusTarget::Inspector => FocusTarget::Conversation,
        FocusTarget::Composer => FocusTarget::Conversation,
        _ => state.focus,
    };
}
```
For full Helix behavior, track pane positions and navigate relative to the current one.

**Step 4: Commit**
```bash
git add crates/ozone-tui/src/input.rs crates/ozone-tui/src/app.rs
git commit -m "feat(tui): Ctrl+W pane navigation (Vim-style window commands)"
```

---

### Task 10: Text object support (`v`, `aw`, `iw`)

**Objective:** In Normal mode, `v` enters visual mode (character-wise selection), `aw` / `iw` select a word. This is a prerequisite for proper editing.

**Files:**
- Modify: `crates/ozone-tui/src/input.rs`, `crates/ozone-tui/src/app.rs`

**Step 1: Add Visual mode to InputMode enum**
```rust
pub enum InputMode {
    Normal,
    Insert,
    Command,
    Visual,   // NEW
}
```

**Step 2: Add Visual selection state**
```rust
pub visual_selection_start: Option<(u16, u16)>,  // cursor (row, col)
pub visual_selection_end: Option<(u16, u16)>,
```

**Step 3: Map keys in Visual mode**
```rust
InputMode::Visual => match key.code {
    KeyCode::Char('h') | KeyCode::Left => {
        KeyAction::VisualMoveLeft
    }
    KeyCode::Char('l') | KeyCode::Right => {
        KeyAction::VisualMoveRight
    }
    // ...
    KeyCode::Esc => KeyAction::LeaveInputMode,  // back to Normal
    KeyCode::Char('d') | KeyCode::Char('x') => KeyAction::VisualDelete,
    KeyCode::Char('y') => KeyAction::VisualYank,
}
```

**Step 4: Wire visual delete/yank to the composer**  
Visual selection + `d` should delete the selected text into the composer buffer. `y` yanks to a register.

**Step 5: Commit**
```bash
git add crates/ozone-tui/src/input.rs crates/ozone-tui/src/app.rs
git commit -m "feat(tui): Visual mode (v) with text objects and yank/d delete"
```

---

## PART 4: `/memories` dedicated overlay

**Root cause:** Users have to remember `Ctrl+K` or `/memory list`. There's no persistent, always-accessible view of what the AI has been told to remember.

### Task 11: `:memories` command opens a full-screen overlay panel

**Objective:** When user types `:memories` (or `/memories`), open a full-screen overlay showing all pinned and note memories for the current session in a scrollable list. This is faster than going to the Inspector and switching tabs.

**Files:**
- Modify: `crates/ozone-tui/src/app.rs`, `crates/ozone-tui/src/render.rs`

**Step 1: Add `MemoriesOverlay` to `ScreenState`**
```rust
MemoriesOverlay,
```

**Step 2: Add memories data to `TuiBootstrap`**
In `apps/ozone-plus/src/runtime.rs`, populate memories in bootstrap:
```rust
let pinned_memories = self.repo.list_pinned_memories(&context.session_id)?;
let note_memories = self.repo.list_note_memories(&context.session_id)?;
TuiBootstrap {
    // ... existing fields ...
    memories: Some(MemoriesPayload { pinned: pinned_memories, notes: note_memories }),
}
```

**Step 3: Add `MemoriesPayload` and wire through ShellState**
```rust
pub memories_overlay: Option<MemoriesPayload>,
```

**Step 4: Handle `:memories` command**
In the command palette or slash handler:
```rust
":memories" | "/memories" => {
    self.screen = ScreenState::MemoriesOverlay;
    self.status_line = Some("q or Esc close · d unpin · e edit".into());
}
```

**Step 5: Render the memories overlay**
Add `fn render_memories_overlay` in `render.rs`:
```rust
fn render_memories_overlay(frame: &mut Frame, pane: &PaneLayout, model: &MemoriesOverlayModel) {
    let mut lines = vec![];
    lines.push(Line::from(Span::styled("── Pinned Memories ──────────────────────", theme::muted_style())));
    for (i, mem) in model.pinned.iter().enumerate() {
        lines.push(Line::from(format!("{} │ {}", i+1, mem.text)));
        lines.push(Line::from(Span::styled(
            format!("    {} · {}", mem.provenance, mem.created_at),
            theme::dim_style()
        )));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled("── Notes ──────────────────────────────────", theme::muted_style())));
    for (i, note) in model.notes.iter().enumerate() {
        lines.push(Line::from(format!("{} │ {}", i+1, note.text)));
    }
    // ...
}
```

**Step 6: Navigate with `j/k`, unpin with `d`**
In Normal mode while in `MemoriesOverlay`, `j/k` moves selection, `d` unpins the selected memory, `e` opens edit, `q` or `Esc` closes.

**Step 7: Commit**
```bash
git add crates/ozone-tui/src/app.rs crates/ozone-tui/src/render.rs apps/ozone-plus/src/runtime.rs
git commit -m "feat(tui): :memories overlay with full session memory view"
```

---

## PART 5: Character System Depth — Persona Visibility

**Root cause:** The character form has all the right fields but the character card's contents are invisible to the user during chat. The model sees them (via context assembly) but the user doesn't see what persona is loaded.

### Task 12: Show active character card summary in the status bar

**Objective:** When a session has a character attached, show the character name and a short tagline in the status bar. This gives immediate feedback about what persona is active.

**Files:**
- Modify: `crates/ozone-tui/src/render.rs`, `crates/ozone-tui/src/app.rs`

**Step 1: Add character info to `ShellIndicators`**
```rust
pub character_label: Option<String>,  // "Alice — a cheerful guide"
```

**Step 2: Populate from session metadata**
In `build_render_model`:
```rust
character_label: state
    .session_metadata
    .as_ref()
    .and_then(|m| m.character_name.as_deref())
    .map(|name| format!("{} ", name)),
```

**Step 3: Add to status bar render**
In `render_status`, add a character indicator section:
```rust
if let Some(label) = &indicators.character_label {
    spans.push(Span::styled(label, theme::accent_style()));
}
```

**Step 4: Commit**
```bash
git add crates/ozone-tui/src/render.rs
git commit -m "feat(tui): show active character in status bar"
```

---

### Task 13: `:character` command to view/edit loaded character card

**Objective:** Allow user to type `:character` (or `:char`) to open an overlay showing the current session's character card fields — name, description, personality, scenario, greeting, example dialogue — with `e` to edit any field.

**Files:**
- Modify: `crates/ozone-tui/src/app.rs`, `crates/ozone-tui/src/render.rs`

This reuses the existing `CharacterDetail` type already defined in `app.rs` and the existing `render_character_form` renderer, just presented as an overlay instead of a full menu screen.

**Step 1: Add `CharacterOverlay` screen state**
```rust
CharacterOverlay(CharacterDetail),  // holds the card data to display
```

**Step 2: Populate and open**
When user enters `:character` command, fetch the card via runtime:
```rust
":character" => {
    if let Some(name) = self.session_metadata.as_ref()
        .and_then(|m| m.character_name.as_deref())
    {
        self.runtime_commands.push(RuntimeCommand::GetCharacter(name.to_string()));
    }
}
```

Handle the response to populate `ScreenState::CharacterOverlay(detail)`.

**Step 3: Render as overlay**  
Use the existing `render_character_form` function but call it from within the shell layout rather than as a standalone menu screen.

**Step 4: Add navigation**
`j/k` to move between fields, `e` or `Enter` to edit the active field (switches to inline edit), `q`/`Esc` to close.

**Step 5: Commit**
```bash
git add crates/ozone-tui/src/app.rs crates/ozone-tui/src/render.rs
git commit -m "feat(tui): :character overlay to view/edit active character card"
```

---

## PART 6: Production Hardening

These are the operational gaps I identified.

### Task 14: Hook `cargo test` into CI

**Files:**
- Modify: `.github/workflows/ci.yml`

**Step 1: Add test step**
```yaml
- name: cargo test
  run: cargo test --workspace --all-targets
```

**Step 2: Commit**
```bash
git add .github/workflows/ci.yml
git commit -m "ci: add cargo test step to CI pipeline"
```

---

### Task 15: Commit or stash the `dev` branch changes

**Files:**
- Root workspace

**Step 1: Review what changed**
```bash
git diff --stat dev -- Cargo.toml src/ crates/ apps/
```

**Step 2: Either:**
- If the changes are a coherent sprint: commit them with a descriptive message
- If they're in-progress: `git stash` with a note

The branch is 1 commit ahead of `main` (feat(reroll)). Merge `dev` into `main` once the sprint is stable.

---

### Task 16: Add `crates/ozone-engine/src/tests.rs` to the crate test targets

**Files:**
- Modify: `crates/ozone-engine/Cargo.toml`

**Step 1: Check if tests are being compiled**
The crate has `[[test]]` sections? The `tests.rs` exists but `cargo test -p ozone-engine` may not be picking it up. Verify:
```bash
cargo test -p ozone-engine -- --list
```

**Step 2: If not listed**, ensure the tests module is compiled:
```toml
[dev-dependencies]
# ...
```

And in `src/lib.rs`:
```rust
#[cfg(test)]
mod tests;
```

**Step 3: Run all tests**
```bash
cargo test --workspace
```

**Step 4: Commit**
```bash
git add crates/ozone-engine/src/lib.rs
**QW7: `Shift+Tab` for reverse Inspector tab cycling** (2 min)
Currently `Tab` cycles forward through InspectorFocus. `Shift+Tab` cycling backward is a standard expectation and requires trivial additions to `input.rs` and `app.rs`.

**QW8: `Shift+Tab` reverse cycling** (2 min)
Already covered in QW7 above.

**QW9: Session list — last message preview column** (15 min)
Session list currently shows (character name, message count, last active time). Add a `last_message_preview` truncated to ~40 chars. This is the single biggest "which session was I in?" aid. See `SessionListItem` in app.rs and its render at ~session_list rendering in render.rs.

**QW10: Status bar — show memory count alongside message count** (5 min)
Status bar notifications already show `message_count · branch_count · bookmark_count`. Add `pinned_count`. One line change in `StatusPaneModel` construction in `build_render_model`.

---

## PART 7: Context Bar — Token Usage & Compression Visibility

**Root cause:** The context bar is the single most important real-time feedback for LLM chat — users need to know how much context headroom remains and when compression kicks in. Currently the data IS available (`ShellState.context_preview.token_budget`) but is only shown as a raw `N / M tokens` string in the status line notifications. A visual fill bar would make this immediately legible.

**Bug:** `used_tokens` is hardcoded to `0` in the fallback context path (`build_from_transcript_internal` at `context_bridge.rs:~231`). The engine plan path (`apply_engine_plan_output`) correctly populates `used_tokens`, but the production path uses the fallback. Until real token counting is hooked up, the bar would show `0 / 8192`.

**Two-layer fix:** (A) Display what we have now — even if `used_tokens=0`, the `max_tokens` from the model config is accurate and the bar fills relative to it. (B) Fix the token counting bug in a separate task.

### Task 17: Add `context_bar` field to `StatusPaneModel`

**Files:**
- Modify: `crates/ozone-tui/src/render.rs`

**Step 1: Add field to StatusPaneModel** (near line 75)
```rust
pub struct StatusPaneModel {
    // ... existing fields ...
    /// Context token usage bar: used / max, rendered as a string like "[████░░░░ 50%]".
    /// None if no budget data is available yet.
    pub context_bar: Option<String>,
    /// Raw token budget for programmatic checks (e.g. warning color when > 80%).
    pub token_budget: Option<(u32, u32)>, // (used, max)
}
```

**Step 2: Populate in `build_render_model`** (in the `let status = StatusPaneModel { ... }` block, ~line 506)
```rust
let token_budget = state
    .context_preview
    .as_ref()
    .and_then(|preview| preview.token_budget.as_ref())
    .map(|b| (b.used_tokens, b.max_tokens));

let context_bar = token_budget.map(|(used, max)| {
    if max == 0 {
        return String::new();
    }
    let pct = (used as f64 / max as f64 * 100.0).min(100.0) as u8;
    let filled = (pct as usize * 20) / 100; // 20-block bar
    let empty = 20 - filled;
    let bar: String = "█".repeat(filled) + &"░".repeat(empty);
    let color = if pct >= 90 { "!" } else if pct >= 75 { "+" } else { "" };
    format!("[{bar}] {pct:3}%{color}", pct = pct)
});

let status = StatusPaneModel {
    // ... existing fields ...
    context_bar,
    token_budget,
};
```

**Step 3: Add to `ShellIndicators` or pass directly**  
The mode badge, vram_hint, and context_bar can all be rendered together at the right end of the status bar footer.

**Step 4: Commit**
```bash
git add crates/ozone-tui/src/render.rs
git commit -m "feat(tui): add context token bar to status pane"
```

---

### Task 18: Render the context bar in `render_status`

**Files:**
- Modify: `crates/ozone-tui/src/render.rs`

**Step 1: Find `render_status` function** (around line 1811)
The function receives `StatusPaneModel`. Add rendering of `model.context_bar` at the rightmost end of the footer bar (before the `vram_hint` if present, or as the rightmost element).

```rust
fn render_status(
    frame: &mut Frame,
    model: &StatusPaneModel,
    layout: &LayoutModel,
    textarea: Option<&TextArea<'static>>,
) {
    // ... existing header render ...
    // Footer bar: [mode_badge] session_title · message_count [context_bar] [vram_hint]
    let footer_line = Line::from(vec![
        // Left: mode badge (if present)
        model.mode_badge.as_ref().map(|badge| {
            Span::styled(format!(" {} ", badge), theme::mode_badge_style())
        }).unwrap_or_else(Span::empty),
        // Middle: session info
        Span::raw(format!(
            " {} · {} msgs",
            model.session_title,
            model.message_count,
        )),
        // Right: context bar
        model.context_bar.as_ref().map(|bar| {
            let style = model.token_budget.map(|(used, max)| {
                let pct = used as f64 / max as f64;
                if pct >= 0.9 {
                    theme::error_style()
                } else if pct >= 0.75 {
                    theme::warning_style()
                } else {
                    theme::dim_style()
                }
            }).unwrap_or_else(theme::dim_style);
            Span::styled(format!("  {bar}"), style)
        }).unwrap_or_else(Span::empty),
        // Far right: VRAM hint
        model.vram_hint.as_ref().map(|vram| {
            Span::styled(format!("  {vram}"), theme::muted_style())
        }).unwrap_or_else(Span::empty),
    ]);
    // ... rest of render_status
}
```

**Step 2: Commit**
```bash
git add crates/ozone-tui/src/render.rs
git commit -m "feat(tui): render context bar in status footer with color warnings"
```

---

### Task 19: Fix `used_tokens = 0` bug — connect real token counting

**Files:**
- Modify: `apps/ozone-plus/src/context_bridge.rs`

**Root cause:** `build_from_transcript_internal` (the fallback path, used in production) sets `used_tokens: 0`. The engine plan path correctly uses `used_tokens: 900` (from the plan). The fallback path needs the same token counting.

**Step 1: Identify where to count**
The `render_prompt` call at line 207 in `context_bridge.rs` produces the final prompt string. After this, estimate tokens using the same estimator the engine uses:

```rust
// After: let prompt = inference.render_prompt(&turns)?;

// Count tokens using the template's encoder
let used_tokens = inference
    .estimator()
    .estimate(&prompt)
    .try_into()
    .unwrap_or(0);
```

**But wait:** The `InferenceAdapter` doesn't expose an `estimator()` method. Two options:

**Option A (quick):** Use the heuristic estimator directly on the prompt string:
```rust
let used_tokens = crate::heuristic_token_estimate(prompt.len()); // rough char/4 approximation
```

**Option B (correct):** Expose the token estimator from the engine or inference config and use it:
```rust
// In InferenceAdapter, add:
pub fn estimate_tokens(&self, text: &str) -> usize {
    self.config
        .context
        .token_estimation
        .map(|cfg| heuristic_token_estimate(text, cfg))
        .unwrap_or_else(|| text.chars().count() / 4)
}
```

**Step 2: Update the preview construction** (line 231 area)
```rust
token_budget: Some(ContextTokenBudgetPreview {
    used_tokens,  // was: 0
    max_tokens: u32::try_from(inference.config().context.max_tokens)
        .unwrap_or(u32::MAX),
}),
```

**Step 3: Commit**
```bash
git add apps/ozone-plus/src/context_bridge.rs
git commit -m "fix(context): count tokens in fallback context build path"
```

---

### Task 20: Show compression/freespace events in the context bar

**Concept:** When the context engine truncates messages (soft layer budget exceeded), it injects a `SessionSynopsis` and marks older messages as "truncated." The context bar should briefly flash when compression happens — showing a "+freed N tokens" indicator that fades after 3 seconds.

**Files:**
- Modify: `crates/ozone-tui/src/app.rs`, `crates/ozone-tui/src/render.rs`

**Step 1: Track last compression event in ShellState**
```rust
pub struct ShellState {
    // ... existing fields ...
    pub last_context_compression: Option<ContextCompressionEvent>,
}

pub struct ContextCompressionEvent {
    pub freed_tokens: usize,
    pub remaining_tokens: usize,
    pub timestamp: i64, // for fade-out timing
}
```

**Step 2: Emit event from runtime**  
In `TuiRuntimeSendReceipt` or a new `TuiRuntimeEvent` variant, add `context_compression: Option<(usize, usize)>` representing `(freed_tokens, remaining_tokens)`.

When the runtime detects that `context_build.preview.selected_items` decreased compared to the previous build, or that a synopsis was generated, emit the compression event.

**Step 3: Render freed indicator**  
In `render_status`, when `last_context_compression` is `Some` and `< 3 seconds old`:
```rust
if let Some(evt) = &model.last_compression {
    let age = now_ms() - evt.timestamp;
    if age < 3000 {
        let alpha = 1.0 - (age as f32 / 3000.0);
        let freed_str = format!("+{} freed", evt.freed_tokens);
        spans.push(Span::styled(
            format!(" {freed_str}"),
            theme::success_style().opacity(alpha),
        ));
    }
}
```

**Step 4: Commit**
```bash
git add crates/ozone-tui/src/app.rs crates/ozone-tui/src/render.rs apps/ozone-plus/src/runtime.rs
git commit -m "feat(tui): show context compression freed-tokens flash in status bar"
```

---

## KNOWN GAPS IN THIS PLAN (to address before execution)

1. **`list_note_memories` does not exist.** Task 5 assumes `repo.list_note_memories()` works but it doesn't — only `list_pinned_memories` exists. Either add `list_note_memories` to `memory_ops.rs` (nearly identical to `list_pinned_memories` but filtering by `kind = 'note_memory'`) or simplify Task 5 to only show pinned memories until notes are implemented.

2. **`TuiSessionMetadata` (runtime.rs) and `SessionMetadata` (app.rs) have no `pinned_memories`/`note_memories` fields.** Task 5 populates these through the bootstrap but the struct fields don't exist yet. Add them first.

3. **`get_character_by_name` does not exist.** Only `get_character(card_id)` exists in `character_ops.rs`. Task 1 correctly identifies this — verify the gap before assuming Task 1 is a 3-line addition.

4. **Task 7 (mode indicator) is 80% done.** `mode_badge: Option<String>` already exists in `StatusPaneModel`, already populated from `input_mode_label(state.input_mode).to_uppercase()`, and already rendered in `render_status` via `short_badge` at line 1816. Verify what, if anything, is actually missing before re-implementing.

5. **Count prefix clearing is unspecified.** Task 8 accumulates digits but never defines when to reset `normal_mode_count`. Add: "Count clears after any non-digit action is dispatched, or after `Esc`."

6. **Visual mode selection anchor is unspecified.** Task 10 says "v enters visual mode" but doesn't define: is selection character-wise or line-wise? What's the anchor? Add: "v = character-wise from current cursor position; V = line-wise."

7. **`used_tokens = 0` bug confirmed.** `context_bridge.rs:~231` hardcodes `0` in the fallback path. This is Task 19 in the new Part 7 above.

8. **Task 12 (`character_label` in status bar) needs explicit code path.** The plan says "populate from session metadata" but doesn't show adding `character_label: Option<String>` to `ShellIndicators` in `build_render_model`. Add this explicitly.

---

## SUMMARY: QUICK WINS (Part 0.5 — do first, before any task)

These are 1–15 min each and make the app feel immediately more professional:

| # | Quick Win | Effort | Impact |
|---|-----------|--------|--------|
| QW1 | Context bar (Tasks 17-18) | 20 min | High — real-time feedback |
| QW2 | `?` help — context-aware per screen | 10 min | Medium — discoverability |
| QW3 | Split hint footer by mode (Normal/Insert/Inspector) | 10 min | Medium — reduced wall-of-text |
| QW4 | `/` in Normal mode → command palette | 2 min | High — Helix parity |
| QW5 | Session list — last message preview column | 15 min | High — "which session?" |
| QW6 | Status bar — memory count alongside msg count | 5 min | Low — low-hanging fruit |
| QW7 | `Shift+Tab` reverse Inspector cycling | 2 min | Low — standard UX |
| QW8 | Verify mode indicator renders (Task 7 gap) | 5 min | Low — may already work |
| QW9 | `list_note_memories` stub for Task 5 | 15 min | Medium — unlocks Task 5 |
| QW10 | Add `pinned_memories`/`note_memories` to struct fields for Task 5 | 10 min | Medium — unlocks Task 5 |

---

## Summary of All Tasks

| # | Part | Task | Files |
|---|------|------|-------|
| 1 | 1 | `get_character_by_name` | `character_ops.rs` |
| 2 | 1 | `seed_greeting_if_present` helper | `session_ops.rs` |
| 3 | 1 | Wire greeting into `create_session` | `runtime.rs` |
| 4 | 2 | `InspectorFocus::Memory` + Tab cycling | `input.rs`, `app.rs` |
| 5 | 2 | Memory lines in Inspector + pass memory data through bootstrap | `render.rs`, `app.rs`, `runtime.rs` |
| 6 | 2 | `d`=unpin `e`=edit in Memory Inspector | `input.rs`, `app.rs` |
| 7 | 3 | Mode indicator in status bar (verify — may be done) | `render.rs` |
| 8 | 3 | Count prefix (`3j`, `5k`) | `app.rs`, `input.rs` |
| 9 | 3 | `Ctrl+W` pane navigation | `input.rs`, `app.rs` |
| 10 | 3 | Visual mode (`v`, text objects) | `input.rs`, `app.rs` |
| 11 | 4 | `:memories` overlay | `app.rs`, `render.rs`, `runtime.rs` |
| 12 | 5 | Character name in status bar | `render.rs` |
| 13 | 5 | `:character` overlay | `app.rs`, `render.rs` |
| 14 | 6 | Hook `cargo test` into CI | `.github/workflows/ci.yml` |
| 15 | 6 | Merge or stash `dev` branch | root |
| 16 | 6 | Fix ozone-engine test binary | `ozone-engine/lib.rs` |
| 17 | 7 | Add `context_bar` field to `StatusPaneModel` | `render.rs` |
| 18 | 7 | Render context bar in `render_status` with color warnings | `render.rs` |
| 19 | 7 | Fix `used_tokens = 0` bug — connect real token counting | `context_bridge.rs` |
| 20 | 7 | Show compression/freespace flash event in context bar | `app.rs`, `render.rs`, `runtime.rs` |

**Total: 20 tasks across 7 parts.** Quick wins (QW1-QW10) can be done in parallel before the main task chain.

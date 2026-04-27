# Ozone UX Hardening & Doc Fix — Holistic Plan

> **For Hermes:** Use subagent-driven-development skill to implement this plan task-by-task.

**Goal:** Fix all failing tests, correct documentation drift between README and actual behavior, and address the top UX pain points identified in the UI/UX assessment.

**Architecture:** Three-phase approach: (1) Fix broken tests and doc drift (2) Address largest UX gaps with tested implementations (3) Polish and hardening.

**Tech Stack:** Rust (ratatui, crossterm, tui-textarea), cargo test, tokio-test

---

## PHASE 0: Pre-flight — Verify Baseline

Before any work, confirm the current state:

```bash
cd /home/eric/projects/ozone-rs
cargo clippy --workspace --all-targets -- -D warnings   # must pass
cargo test -p ozone-tui --                              # must show 175 passed
cargo test -p ozone --                                    # must show 38 passed
cargo test -p ozone-memory --                            # must show 48 passed
cargo test -p ozone-persist --                           # must show 34 passed
cargo test -p ozone-plus --                              # must show FAIL (4 broken)
```

---

## PART 1: FIX FAILING TESTS (ozone-plus binary)

### Task 1: Fix `index_rebuild_command_persists_embeddings_and_builds_vector_index`

**Objective:** The test asserts `first_records.len() == 2` but gets `1`. The second record (note memory embedding) is not being created/persisted during `index rebuild`.

**Files:**
- `apps/ozone-plus/src/hybrid_search.rs:1049` (related test context)
- `apps/ozone-plus/src/main.rs:3726` (failing assertion)

**Step 1: Run failing test with verbose output**
```bash
cd /home/eric/projects/ozone-rs
cargo test -p ozone-plus index_rebuild_command_persists_embeddings_and_builds_vector_index -- --nocapture 2>&1 | tail -30
```

**Step 2: Read the test to understand what it creates**
- Creates a session with 1 user message
- Creates 1 note memory: "Pack the spare lens before leaving camp."
- Runs `index rebuild`
- Expects 2 embedding artifacts (1 message + 1 note memory)
- Gets only 1

**Step 3: Investigate `index rebuild` command implementation**
```bash
cd /home/eric/projects/ozone-rs
grep -n "index.*rebuild" apps/ozone-plus/src/main.rs | head -20
```

**Step 4: Check `note_memory` embedding path** — the issue is likely that note memories aren't being indexed. Read:
- `crates/ozone-persist/src/repository/memory_ops.rs` — look for `create_note_memory` and whether it triggers embedding
- `apps/ozone-plus/src/index.rs` or wherever `index rebuild` CLI command is implemented

**Step 5: Fix the embedding artifact creation for note memories** — likely missing call to persist embedding artifact for note memories during rebuild.

**Step 6: Run test to verify pass**
```bash
cargo test -p ozone-plus index_rebuild_command_persists_embeddings_and_builds_vector_index
```
Expected: PASS

**Step 7: Commit**
```bash
git add apps/ozone-plus/src/
git commit -m "fix(ozone-plus): persist embedding artifacts for note memories during index rebuild"
```

---

### Task 2: Fix `stale_embeddings_are_filtered_and_inactive_memories_are_downranked`

**Objective:** Test asserts `downranked_embeddings == 1` but gets `0`. The downranking of inactive memories is not happening.

**Files:**
- `apps/ozone-plus/src/hybrid_search.rs:1049`

**Step 1: Run failing test with full output**
```bash
cd /home/eric/projects/ozone-rs
RUST_BACKTRACE=1 cargo test -p ozone-plus stale_embeddings_are_filtered_and_inactive_memories_are_downranked -- --nocapture 2>&1 | tail -50
```

**Step 2: Read the test** — it:
- Creates a session, inserts user message "Remember the brass lantern..."
- Creates a pinned memory with that text
- Sets `is_active = false` on the pinned memory
- Edits the message (creating stale embedding)
- Inserts assistant message
- Runs hybrid search for "observatory code"
- Expects `downranked_embeddings == 1` (the inactive memory should be downranked)

**Step 3: Find `downranked_embeddings` computation** in hybrid_search.rs around line 1049:
```bash
grep -n "downranked" apps/ozone-plus/src/hybrid_search.rs
```

**Step 4: The test expects that inactive memories are DOWNRANKED (appear in results but below active ones). The assertion `left: 0` means the downranking filter isn't being applied. Read around line 1049:**
```bash
sed -n '1030,1060p' apps/ozone-plus/src/hybrid_search.rs
```

**Step 5: Find `RetrievalSourceState::InactiveMemory` handling** — the downranking logic needs to check `hit.source_state == InactiveMemory` and decrement the count.

**Step 6: Fix the downranking counter** — likely missing increment in `HybridSearchService::search_session`.

**Step 7: Run test to verify pass**
```bash
cargo test -p ozone-plus stale_embeddings_are_filtered_and_inactive_memories_are_downranked
```
Expected: PASS

**Step 8: Commit**
```bash
git add apps/ozone-plus/src/
git commit -m "fix(ozone-plus): count inactive-memory downranking in hybrid search"
```

---

### Task 3: Fix test isolation issues (PoisonError)

**Objective:** `index_rebuild_fails_cleanly_when_provider_is_disabled` and `memory_and_search_commands_execute_against_xdg_repo` fail with `PoisonError` when run with other tests, indicating a shared global lock or state is being poisoned.

**Files:**
- `apps/ozone-plus/src/main.rs:3752` and `3625`

**Step 1: Confirm isolation issue** — run each test individually:
```bash
cd /home/eric/projects/ozone-rs
cargo test -p ozone-plus index_rebuild_fails_cleanly_when_provider_is_disabled   # should pass alone
cargo test -p ozone-plus memory_and_search_commands_execute_against_xdg_repo      # should pass alone
cargo test -p ozone-plus index_rebuild                                           # run together
```

**Step 2: Find `ENV_LOCK` and how it's used:**
```bash
grep -n "ENV_LOCK\|PoisonError" apps/ozone-plus/src/main.rs | head -20
```

**Step 3: Look at the test sandbox setup** — the PoisonError typically means a `Mutex` was poisoned by a panic in a previous test. Check if `TestSandbox::new` or `open_repository` uses a shared lock that isn't being properly cleaned up.

**Step 4: Read the failing test lines:**
```bash
sed -n '3750,3780p' apps/ozone-plus/src/main.rs
sed -n '3620,3660p' apps/ozone-plus/src/main.rs
```

**Step 5: The fix likely involves using `ScopedEnvVar` properly or ensuring `ENV_LOCK` is unlocked before panicking.** Check if there's a `Drop` impl missing on test helpers.

**Step 6: Run full ozone-plus test suite to verify:**
```bash
cargo test -p ozone-plus --
```
Expected: all pass (51 passed, 0 failed)

**Step 7: Commit**
```bash
git add apps/ozone-plus/src/
git commit -m "fix(ozone-plus): resolve test poisoning from shared state in sandbox tests"
```

---

## PART 2: FIX DOCUMENTATION DRIFT

### Task 4: Fix README.md `/thinking` command

**Root cause:** README.md line 368 says `/thinking immersive` but the actual command is `/thinking hidden|assisted|debug`. "immersive" does not exist.

**Files:**
- `README.md:368`

**Step 1: Confirm what the code actually supports:**
```bash
grep -A5 "parse_thinking_subcommand" apps/ozone-plus/src/runtime.rs
```

**Step 2: Fix README.md line 368:**
Replace:
```
| `/thinking immersive` | Show AI thinking blocks inline |
```
With:
```
| `/thinking hidden` | Suppress AI thinking block display |
```

**Step 3: Commit**
```bash
git add README.md
git commit -m "docs: fix /thinking immersive -> /thinking hidden in README"
```

---

### Task 5: Fix README.md `/summarize` command

**Root cause:** README.md line 367 lists `/summarize chunk` but only `/summarize session` is implemented.

**Files:**
- `README.md:367`

**Step 1: Confirm what the code supports:**
```bash
grep -A5 "parse_summarize_subcommand" apps/ozone-plus/src/runtime.rs
```

**Step 2: Fix README.md** — remove `/summarize chunk` line:
```
| `/summarize chunk` | Summarize the current context window |
```
This feature doesn't exist yet — do NOT document it.

**Step 3: Commit**
```bash
git add README.md
git commit -m "docs: remove /summarize chunk from README (not implemented)"
```

---

### Task 6: Fix README.md `/tierb` command

**Root cause:** README.md lines 371-372 say `/tierb on` and `/tierb off` but the code only supports `status` and `toggle`.

**Files:**
- `README.md:371-372`

**Step 1: Confirm what the code actually supports:**
```bash
grep -A5 "parse_tierb_subcommand" apps/ozone-plus/src/runtime.rs
```

**Step 2: Fix README.md lines 371-372:**
Replace:
```
| `/tierb on` | Enable Tier B assistive features (importance scoring, keyword extraction) |
| `/tierb off` | Disable Tier B |
```
With:
```
| `/tierb status` | Show Tier B feature status |
| `/tierb toggle` | Toggle Tier B features on/off |
```

**Step 3: Commit**
```bash
git add README.md
git commit -m "docs: fix /tierb on/off -> /tierb status/toggle in README"
```

---

## PART 3: UX IMPROVEMENTS — QUICK WINS

### Task 7: Show actual error messages instead of debug `{:?}` strings

**Root cause:** Runtime errors surface as `{:?}` debug strings in the status line. This is user-hostile.

**Files:**
- `crates/ozone-tui/src/app.rs` — look for `status_line = Some(format!("{:?}", error))` patterns

**Step 1: Find all places that format errors as `{:?}`:**
```bash
grep -n 'format!.*error.*{:?}\|format!.*:?.*error' crates/ozone-tui/src/app.rs crates/ozone-tui/src/lib.rs
```

**Step 2: Replace with user-friendly messages:**
- `Failed to create session: {:?}` → `Failed to create session — check that the backend is running`
- For each error variant, display a human-readable message

**Step 3: Run tests to verify nothing broke:**
```bash
cargo test -p ozone-tui --
```
Expected: 175 passed

**Step 4: Commit**
```bash
git add crates/ozone-tui/src/
git commit -m "fix(ozone-tui): show user-friendly error messages instead of debug output"
```

---

### Task 8: Implement `jj` keybinding to exit Insert mode (like Vim's escape)

**Root cause:** Vim users expect `jj` or `jk` to exit insert mode without reaching for `Esc`. Only `Esc` works currently.

**Files:**
- `crates/ozone-tui/src/input.rs` — `dispatch_key` for `InputMode::Insert`
- `crates/ozone-tui/src/app.rs` — state tracking for `jj` sequence

**Step 1: Add a state field to track partial `jj` sequence:**
In `ShellState` or a dedicated `InsertEscapeState` struct, add:
```rust
pub struct InsertEscapeState {
    /// Characters accumulated since entering Insert mode for potential escape sequence
    escape_buffer: Vec<char>,
}
```

**Step 2: In `dispatch_key` for `InputMode::Insert`, after processing a char key:**
- If the char is `j`, add to escape buffer
- If escape buffer ends with "jj" or "jk", return `KeyAction::LeaveInputMode`
- After any non-j char, clear escape buffer

**Step 3: Write test first:**
```rust
#[test]
fn jj_exits_insert_mode() {
    let state = ShellState::new();
    state.input_mode = InputMode::Insert;
    // Simulate typing 'j' then another 'j'
    let action = state.handle_key_event(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
    // Should be Noop, buffer 'j'
    let action2 = state.handle_key_event(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
    assert_eq!(action2, KeyAction::LeaveInputMode);
}
```

**Step 4: Run tests**
```bash
cargo test -p ozone-tui --
```
Expected: 175+ passed

**Step 5: Commit**
```bash
git add crates/ozone-tui/src/
git commit -m "feat(ozone-tui): jj/jk escape sequence exits Insert mode"
```

---

### Task 9: Add context hint bar to Inspector pane

**Root cause:** The Inspector pane shows model info but context/token budget is only in the 1-row footer. Users can't see both simultaneously.

**Files:**
- `crates/ozone-tui/src/render.rs` — `render_inspector_pane`
- `crates/ozone-tui/src/app.rs` — `InspectorPaneModel` fields

**Step 1: Look at current inspector render:**
```bash
grep -n "render_inspector" crates/ozone-tui/src/render.rs | head -5
```

**Step 2: Find what `InspectorPaneModel` currently has:**
```bash
grep -A10 "pub struct InspectorPaneModel" crates/ozone-tui/src/render.rs
```

**Step 3: Add `context_bar` and `token_budget` fields to `InspectorPaneModel` in render.rs**

**Step 4: Update `build_render_model` to populate these fields from `state.session.context.token_budget`**

**Step 5: Render the context bar in the inspector** — reuse the existing `context_bar` string rendering from the footer, but render it inside the inspector pane above model info.

**Step 6: Run tests and verify inspector renders at 80x24 and 120x40**

**Step 7: Commit**
```bash
git add crates/ozone-tui/src/
git commit -m "feat(ozone-tui): show context token budget in inspector pane"
```

---

## PART 4: UX IMPROVEMENTS — LARGER GAPS

### Task 10: Split oversized modules

**Root cause:** `app.rs` (5,890 lines) and `render.rs` (4,187 lines) are too large for single files. This makes the code hard to navigate, test, and modify safely.

**Files:**
- `crates/ozone-tui/src/app.rs`
- `crates/ozone-tui/src/render.rs`

**Step 1: Audit `app.rs` — identify logical sub-modules:**
- Screen state machine → `app/screens/*.rs`
- Input handling → `app/input_handler.rs`  
- Session management → `app/session.rs`
- Draft management → `app/draft.rs`

**Step 2: Audit `render.rs` — identify logical sub-modules:**
- `render/conversation.rs` — conversation pane
- `render/composer.rs` — composer pane
- `render/inspector.rs` — inspector pane
- `render/menus.rs` — main menu, session list, character screens

**Step 3: Move code in logical chunks, keeping tests passing at each step**

**Step 4: Commit each logical chunk separately**

> **Note:** This is a refactoring task — no new features, just better organization. Do not attempt this until all failing tests (Part 1) and doc drift fixes (Part 2) are complete.

---

### Task 11: Character greeting seeding on session creation

**Root cause:** `create_session()` in the runtime never looks up the character card's `greeting` field and never seeds it as the first assistant message. This is documented in the plans and partially in flight — verify current state.

**Files:**
- `crates/ozone-persist/src/repository/session_ops.rs`
- `crates/ozone-persist/src/repository/character_ops.rs`

**Step 1: Check if `get_character_by_name` was implemented:**
```bash
grep -n "get_character_by_name" crates/ozone-persist/src/repository/
```

**Step 2: Check if `seed_greeting_if_present` was implemented:**
```bash
grep -n "seed_greeting_if_present" crates/ozone-persist/src/repository/
```

**Step 3: If either is missing, implement from the plan** (ozone-plus-vimify-and-character-system.md has the full spec)

**Step 4: Run integration test to verify session creation with character seeds greeting**

**Step 5: Commit**
```bash
git add crates/ozone-persist/src/
git commit -m "feat(ozone-persist): seed character greeting on session creation"
```

---

### Task 12: Implement clipboard integration for Visual yank

**Root cause:** `VisualYank` says `TODO: Implement actual clipboard integration` — the yanked text is logged to status but not actually copied to the system clipboard.

**Files:**
- `crates/ozone-tui/src/app.rs:2921-2927`

**Step 1: Check if `arboard` or `copier` crate is in Cargo.toml:**
```bash
grep -n "arboard\|copier\|clipboard" crates/ozone-tui/Cargo.toml
```

**Step 2: If not, add a clipboard dependency:**
```toml
# crates/ozone-tui/Cargo.toml
arboard = "3"
```

**Step 3: In `VisualYank` handler:**
```rust
use arboard::Clipboard;
let mut clipboard = Clipboard::new().map_err(|e| format!("clipboard unavailable: {e}"))?;
clipboard.set_text(&self.draft.text)?;
self.status_line = Some(format!("Yanked {} chars to clipboard", self.draft.text.chars().count()));
```

**Step 4: Run tests**

**Step 5: Commit**
```bash
git add crates/ozone-tui/
git commit -m "feat(ozone-tui): actual clipboard integration for visual yank"
```

---

## SUMMARY: QUICK WINS (Part 0.5)

| # | Quick Win | Effort | Impact | Depends |
|---|-----------|--------|--------|---------|
| QW1 | Fix README `/thinking immersive` → `/thinking hidden` | 5 min | Low (doc fix) | None |
| QW2 | Fix README `/tierb on/off` → `status/toggle` | 5 min | Low (doc fix) | None |
| QW3 | Remove `/summarize chunk` from README | 5 min | Low (doc fix) | None |
| QW4 | Fix user-facing error messages | 30 min | Medium | None |
| QW5 | `jj`/`jk` escape from Insert mode | 45 min | High | None |
| QW6 | Show context bar in Inspector pane | 1 hr | Medium | None |
| QW7 | Fix failing tests (Part 1) | 2-3 hrs | High (blocking) | None |
| QW8 | Clipboard integration | 30 min | Medium | None |

## KNOWN GAPS IN THIS PLAN

1. **Test failures may have deeper root causes.** Tasks 1-3 investigate test failures that could be related to the index rebuild and hybrid search logic, not just test setup. Budget extra time if the fix requires changes to the actual search/index logic.

2. **Character greeting seeding (Task 11) may already be done.** The ROUTER.md says Phase 2 is complete and the plan document was written when the fix was pending. Verify first before implementing.

3. **Module split (Task 10) is a large refactor.** Should be done last, after all features are stable. It will touch many files and could introduce regressions if rushed.

4. **No visual screenshot tests exist.** All tests are unit/integration tests. The plan doesn't add visual regression testing because there's no established baseline. Consider adding `screenshot_tool` automation as a follow-up.

5. **Onboarding UX (guided tour) is not addressed.** This is a larger feature that deserves its own plan. It's marked as a UX gap but not scoped here because it requires significant design work beyond "fix what's broken."

---

## EXECUTION ORDER

```
Phase 0: Pre-flight baseline          → Done in current session
Part 1: Fix failing tests (7-11)      → Tasks 1-3
Part 2: Fix doc drift (12-14)         → Tasks 4-6  ← Can parallelize with Part 1
Part 3: Quick Win UX fixes (15-17)     → Tasks 7-9
Part 4: Larger UX gaps (18-19)        → Tasks 10-11
        Clipboard (20)                → Task 12
```

After all tasks complete, run full test suite:
```bash
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

All tests should pass, all doc drift should be corrected.

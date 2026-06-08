# Ozone Codebase Refactor & Cleanup Implementation Plan v2

> **For Hermes:** Use subagent-driven-development skill to implement this plan task-by-task.

**Goal:** Refactor massive files into manageable modules, fix failing tests, clean up bloat, add UX quick wins, and prepare Ozone for alpha release (target: 92/100 readiness score).

**Architecture:** Break each file >3000 lines into smaller modules (max 1000 lines each), replace hardcoded paths with dynamic references, remove dead code/unused imports, add missing user-facing documentation, and deploy high-impact low-effort UX polish.

**Tech Stack:** Rust, Cargo, ratatui, SQLite/WAL, usearch, MCP, TDD (test-driven for all code changes).

---

## Phase 1: Fix Failing Tests (Quick Win, ~1 hour)

### Task 1: Fix hardcoded paths in ozone-mcp tests

**Objective:** Replace hardcoded `/home/eric/projects/ozone-rs` paths with dynamic `repo_root` to fix 5 failing tests.

**Files:**
- Modify: `/home/eric/projects/ozone/crates/ozone-mcp/src/lib.rs` (test section around lines 5800-6200)
- Test: `cargo test -p ozone-mcp --lib` (verify fix)

**Step 1: Verify current failures**
```bash
cd /home/eric/projects/ozone && cargo test -p ozone-mcp --lib 2>&1 | grep "FAILED\|test result"
```
Expected: 5 FAILED tests referencing `ozone-rs` paths.

**Step 2: Locate hardcoded path references**
```bash
grep -n "ozone-rs" /home/eric/projects/ozone/crates/ozone-mcp/src/lib.rs
```

**Step 3: Replace hardcoded paths with repo_root**
In test functions, replace:
```rust
// Before (hardcoded)
let fixture_path = "/home/eric/projects/ozone-rs/crates/ozone-mcp/tests/fixtures/screen-check-fixture.json";

// After (dynamic)
let fixture_path = self.repo_root.join("crates/ozone-mcp/tests/fixtures/screen-check-fixture.json");
```

For test assertions using hardcoded `ozone-rs` paths (e.g., line 5817):
```rust
// Before
assert_eq!(base_splash_command, ["cargo", "run", "--quiet", "--", "--mode", "base", "--no-browser"]);

// After (use repo_root to construct expected path)
let expected_cmd = format!("{}", self.repo_root.join("target/debug/ozone").display());
```

**Step 4: Run tests to verify fix**
```bash
cargo test -p ozone-mcp --lib
```
Expected: 20 passed; 0 failed.

**Step 5: Commit**
```bash
git add crates/ozone-mcp/src/lib.rs
git commit -m "fix(ozone-mcp): replace hardcoded ozone-rs paths with repo_root"
```

---

## Phase 2: Refactor Massive Files (Core Task, ~2-3 days)

### Task 2: Refactor ozone-mcp/src/lib.rs (6232 lines -> modules)

**Objective:** Split 6232-line lib.rs into modular files. Extract 3 example tools, then delegate remaining 16 via subagent.

**Files:**
- Create: `crates/ozone-mcp/src/tools/mod.rs`
- Create: `crates/ozone-mcp/src/tools/workspace_status.rs` (EXAMPLE 1)
- Create: `crates/ozone-mcp/src/tools/cargo_tool.rs` (EXAMPLE 2)
- Create: `crates/ozone-mcp/src/tools/catalog_list.rs` (EXAMPLE 3)
- Modify: `crates/ozone-mcp/src/lib.rs` (keep only server struct, request handling, tool dispatch)

**Step 1: Create tools directory**
```bash
mkdir -p /home/eric/projects/ozone/crates/ozone-mcp/src/tools
```

**Step 2: Extract 3 example tools**
Follow the pattern in the original plan for `workspace_status`, `cargo_tool`, and `catalog_list`.

**Step 3: Update lib.rs to import the 3 example tools**
```rust
mod tools;
use tools::{
    workspace_status::workspace_status_tool,
    cargo_tool::cargo_tool,
    catalog_list::catalog_list_tool,
};
```

**Step 4: Update tool dispatch in handle_tool_call**
```rust
"workspace_status" => workspace_status_tool(self)?,
"cargo_tool" => cargo_tool(self, &arguments)?,
"catalog_list" => catalog_list_tool(self, &arguments)?,
// ... other tools still call self.method() until refactored
```

**Step 5: Run tests after extraction**
```bash
cargo test -p ozone-mcp --lib
```
Expected: All tests pass after each extraction.

**Step 6: Commit examples**
```bash
git add crates/ozone-mcp/src/
git commit -m "refactor(ozone-mcp): extract 3 example tools, delegate rest via subagent"
```

**Step 7: Delegate remaining 16 tool extractions via subagent**
Use `delegate_task` with context: "Extract remaining 16 MCP tools (sandbox_tool, mock_backend_tool, session_tool, message_tool, memory_tool, search_tool, branch_tool, swipe_tool, export_tool, import_card, launcher_smoke, screen_nav_targets, mock_user_tool, screenshot_tool, screen_check_tool) from lib.rs to tools/*.rs following the pattern in workspace_status.rs. Run `cargo test -p ozone-mcp --lib` after each extraction. Add rustdoc to each tool function."

Expected: All 19 tools extracted, all tests pass, rustdoc added.

---

### Task 3: Refactor ozone-tui/src/app.rs (6116 lines -> modules)

**Objective:** Split 6116-line app.rs into app/mod.rs, app/event_handler.rs, app/state.rs, app/commands.rs.

**Files:**
- Create: `crates/ozone-tui/src/app/mod.rs`
- Create: `crates/ozone-tui/src/app/event_handler.rs`
- Create: `crates/ozone-tui/src/app/state.rs`
- Create: `crates/ozone-tui/src/app/commands.rs`
- Modify: `crates/ozone-tui/src/app.rs` -> replace with mod declarations

**Step 1: Create app directory**
```bash
mkdir -p /home/eric/projects/ozone/crates/ozone-tui/src/app
```

**Step 2: Extract app state to app/state.rs**
Move all struct definitions (AppState, SessionState, etc.) to state.rs.

**Step 3: Extract event handling to app/event_handler.rs**
Move all key/input handling logic to event_handler.rs.

**Step 4: Extract command processing to app/commands.rs**
Move all slash command logic to commands.rs.

**Step 5: Update app/mod.rs to tie modules together**
```rust
pub mod state;
pub mod event_handler;
pub mod commands;

pub use state::AppState;
pub use event_handler::handle_event;
pub use commands::process_command;
```

**Step 6: Add rustdoc to public functions**
```rust
/// Handles key events for the Ozone+ TUI.
/// Returns true if the event was handled, false otherwise.
pub fn handle_event(event: KeyEvent, state: &mut AppState) -> bool {
    // ...
}
```

**Step 7: Run TUI tests**
```bash
cargo test -p ozone-tui
```
Expected: All tests pass.

**Step 8: Commit**
```bash
git add crates/ozone-tui/src/app/
git commit -m "refactor(ozone-tui): split app.rs into modular components with rustdoc"
```

---

### Task 4: Refactor ozone-tui/src/render.rs (4319 lines -> modules)

**Objective:** Split render.rs into render/mod.rs, render/chat.rs, render/sessions.rs, render/settings.rs.

**Files:**
- Create: `crates/ozone-tui/src/render/mod.rs`
- Create: `crates/ozone-tui/src/render/chat.rs`
- Create: `crates/ozone-tui/src/render/sessions.rs`
- Create: `crates/ozone-tui/src/render/settings.rs`

**Step 1: Create render directory**
```bash
mkdir -p /home/eric/projects/ozone/crates/ozone-tui/src/render
```

**Step 2: Extract chat rendering to render/chat.rs**
Move all chat message rendering logic.

**Step 3: Extract session list rendering to render/sessions.rs**
Move session list, folder grouping, branch rendering.

**Step 4: Extract settings rendering to render/settings.rs**
Move settings screen, theme picker, toggle/cycle rendering.

**Step 5: Add rustdoc**
```rust
/// Renders the chat messages pane with support for branching and swipes.
pub fn render_chat(f: &mut Frame, area: Rect, state: &AppState) {
    // ...
}
```

**Step 6: Run tests**
```bash
cargo test -p ozone-tui
```

**Step 7: Commit**
```bash
git add crates/ozone-tui/src/render/
git commit -m "refactor(ozone-tui): split render.rs into modular components with rustdoc"
```

---

### Task 5: Refactor apps/ozone-plus/src/main.rs (3780 lines -> modules)

**Objective:** Split main.rs into main/mod.rs, main/cli.rs, main/config.rs.

**Files:**
- Create: `apps/ozone-plus/src/main/mod.rs`
- Create: `apps/ozone-plus/src/main/cli.rs`
- Create: `apps/ozone-plus/src/main/config.rs`

**Step 1: Create main directory**
```bash
mkdir -p /home/eric/projects/ozone/apps/ozone-plus/src/main
```

**Step 2-4: Extract CLI, config, keep main loop**
**Step 5: Run tests**
```bash
cargo test -p ozone-plus
```

**Step 6: Commit**
```bash
git add apps/ozone-plus/src/main/
git commit -m "refactor(ozone-plus): split main.rs into modular components"
```

---

### Task 6: Refactor apps/ozone-plus/src/runtime.rs (3698 lines -> modules)

**Objective:** Split runtime.rs into runtime/mod.rs, runtime/stream.rs, runtime/engine.rs.

**Files:**
- Create: `apps/ozone-plus/src/runtime/mod.rs`
- Create: `apps/ozone-plus/src/runtime/stream.rs`
- Create: `apps/ozone-plus/src/runtime/engine.rs`

**Step 1: Create runtime directory**
```bash
mkdir -p /home/eric/projects/ozone/apps/ozone-plus/src/runtime
```

**Step 2-4: Extract streaming, engine interaction, keep orchestration**
**Step 5: Run tests**
```bash
cargo test -p ozone-plus
```

**Step 6: Commit**
```bash
git add apps/ozone-plus/src/runtime/
git commit -m "refactor(ozone-plus): split runtime.rs into modular components"
```

---

## Phase 3: Code Cleanup (Remove Bloat, ~1 day)

### Task 7: Remove dead code and unused imports

**Objective:** Use clippy and dead_code lint to find and remove unused code.

**Step 1: Run clippy with dead_code lint**
```bash
cd /home/eric/projects/ozone
cargo clippy --workspace --all-targets --message-format=short 2>&1 | grep "dead_code\|unused_imports\|unused_variables"
```

**Step 2: Remove unused imports**
Check each file with unused imports and remove them manually or with:
```bash
cargo fix --workspace --allow-dirty --allow-no-vcs
```

**Step 3: Remove dead code**
- Search for functions/tests that are never called
- Search for commented-out code blocks
- Remove `#[allow(dead_code)]` attributes if code is truly unused

**Step 4: Run tests to verify no regressions**
```bash
cargo test --workspace
```

**Step 5: Commit**
```bash
git add -u
git commit -m "cleanup: remove dead code and unused imports"
```

---

### Task 8: Remove redundant logic and bloat

**Objective:** Find duplicated logic across crates and consolidate.

**Step 1: Search for duplicated patterns**
```bash
grep -rn "models_dir()" /home/eric/projects/ozone/crates/
grep -rn "preferences_path()" /home/eric/projects/ozone/crates/
```

**Step 2: Extract common logic to ozone-core**
If multiple crates use the same path resolution or preference loading, move to `crates/ozone-core/src/paths.rs` or `crates/ozone-core/src/prefs.rs`.

**Step 3: Update all references to use the consolidated function**
**Step 4: Run tests**
```bash
cargo test --workspace
```

**Step 5: Commit**
```bash
git add crates/ozone-core/ crates/*/src/
git commit -m "cleanup: consolidate duplicated logic into ozone-core"
```

---

## Phase 4: Documentation & UX Quick Wins (User-Facing, ~8 hours)

### Task 9: Create user-facing documentation

**Objective:** Add getting-started guide, feature overview, and CONTRIBUTING.md.

**Files:**
- Create: `/home/eric/projects/ozone/docs/getting-started.md`
- Create: `/home/eric/projects/ozone/docs/features.md`
- Create: `/home/eric/projects/ozone/docs/tutorial.md`
- Create: `/home/eric/projects/ozone/CONTRIBUTING.md`

**Step 1-4: Write docs as per original plan**
**Step 5: Commit**
```bash
git add docs/ CONTRIBUTING.md
git commit -m "docs: add user-facing guides and contributing guide"
```

---

### Task 10: Add coverage threshold to CI

**Objective:** Enforce minimum code coverage in CI pipeline.

**Files:**
- Modify: `/home/eric/projects/ozone/.github/workflows/ci.yml`

**Step 1: Add grcov coverage check to CI**
```yaml
  coverage:
    name: Coverage Check
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Install Rust nightly
        uses: dtolnay/rust-toolchain@nightly
      - name: Install grcov
        run: cargo install grcov
      - name: Generate coverage
        run: |
          CARGO_INCREMENTAL=0 RUSTFLAGS="-Cinstrument-coverage" cargo test --workspace
          grcov . --binary-path ./target/debug/deps -s . -t lcov --branch --ignore-not-existing -o lcov.info
      - name: Check coverage threshold
        run: |
          COVERAGE=$(lcov --summary lcov.info | grep "lines......:" | awk '{print $2}' | sed 's/%//')
          if [ $(echo "$COVERAGE < 60" | bc) -eq 1 ]; then
            echo "Coverage $COVERAGE% is below 60% threshold"
            exit 1
          fi
```

**Step 2: Commit**
```bash
git add .github/workflows/ci.yml
git commit -m "ci: add coverage threshold (60%)"
```

---

### Task 11: UX Quick Wins (Polish without heavy work)

**Objective:** Add high-impact, low-effort UX improvements that make the app feel polished.

**Quick Win List (Est. Total: ~8 hours):**

| Quick Win | Effort | Impact | Files to Touch |
|-----------|--------|--------|----------------|
| Keyboard shortcut overlay (`?` key) | 1-2h | High -- users discover features | `crates/ozone-tui/src/help.rs` |
| First-run tutorial (3-step onboarding) | 2-3h | High -- reduces confusion | `apps/ozone-plus/src/main.rs` |
| Session search with live filtering | 2h | Medium -- faster navigation | `crates/ozone-tui/src/app/state.rs` |
| Theme preview in settings | 1h | Medium -- visual feedback | `crates/ozone-tui/src/render/settings.rs` |
| Token count in message metadata | 30min | Low-Medium -- info for users | `apps/ozone-plus/src/runtime.rs` |
| Copy transcript to clipboard | 30min | Medium -- quick sharing | `crates/ozone-persist/src/repository/mod.rs` |
| Session sorting (date/name/folder) | 1h | Medium -- organization | `crates/ozone-tui/src/app/state.rs` |
| Tab completion for model names | 1h | Medium -- faster workflow | `src/ui/launcher.rs` |

**Implementation Steps:**

**Step 1: Keyboard shortcut overlay (`?` key)**
Create `crates/ozone-tui/src/help.rs`:
```rust
/// Renders a help overlay showing all keyboard shortcuts.
pub fn render_help_overlay(f: &mut Frame, area: Rect) {
    let shortcuts = vec![
        ("Esc/q", "Back/Quit"),
        ("?", "Show this help"),
        ("Ctrl+K", "Search memories"),
        ("PgUp/PgDn", "Scroll lists"),
        ("/", "Slash commands"),
        ("f", "Open folder picker"),
        ("F", "Remove from folder"),
    ];
    // Render overlay with shortcuts...
}
```
Bind `?` in `app/event_handler.rs` to toggle help overlay.

**Step 2: First-run tutorial**
In `apps/ozone-plus/src/main.rs`, check for first-run flag:
```rust
if prefs.is_first_run() {
    show_tutorial(&[
        "Welcome to Ozone+!",
        "1. Add a GGUF model with `ozone model add --hf <model>`",
        "2. Run `ozone-plus open` to start chatting",
        "3. Use Ctrl+K to search memories across sessions",
    ]);
    prefs.set_first_run(false);
}
```

**Step 3: Session search with live filtering**
In `crates/ozone-tui/src/app/state.rs`:
```rust
pub fn filter_sessions(&self, query: &str) -> Vec<SessionRecord> {
    if query.is_empty() {
        self.sessions.clone()
    } else {
        self.sessions
            .iter()
            .filter(|s| s.name.contains(query) || s.tags.iter().any(|t| t.contains(query)))
            .cloned()
            .collect()
    }
}
```
Bind to a search field in the session list screen.

**Step 4: Theme preview in settings**
In `crates/ozone-tui/src/render/settings.rs`, when selecting a theme preset, show a mini preview box with sample text rendered in that theme's colors.

**Step 5: Token count in message metadata**
In `apps/ozone-plus/src/runtime.rs`, after streaming a response, display token count:
```rust
// Use the inference backend's actual token count if available
let token_count = response.tokens_used.unwrap_or_else(|| {
    response.text.split_whitespace().count() // rough estimate
});
// Display: "↩ 128 tokens"
```

**Step 6: Run tests**
```bash
cargo test --workspace
```

**Step 7: Commit**
```bash
git add crates/ozone-tui/src/help.rs crates/ozone-tui/src/app/ apps/ozone-plus/src/
git commit -m "feat: add UX quick wins (help overlay, tutorial, theme preview, search)"
```

---

## Verification Checklist (After All Tasks)

- [ ] All 143+ tests pass (0 failing)
- [ ] No file exceeds 1000 lines (except auto-generated)
- [ ] `cargo clippy --workspace -- -D warnings` passes
- [ ] `cargo test --workspace` passes with 60%+ coverage
- [ ] User-facing docs exist (getting-started.md, features.md, tutorial.md)
- [ ] CONTRIBUTING.md exists
- [ ] No hardcoded paths in test code
- [ ] All modules documented with rustdoc
- [ ] UX quick wins deployed (help overlay, tutorial, theme preview, search, token count)

---

## Post-Refactor Alpha Score Estimate

After completing all tasks:
- Code Hygiene: 58 -> 90 (+32, modular + rustdoc)
- Test Coverage: 62 -> 85 (+23, all tests pass, coverage threshold)
- Architecture: 75 -> 92 (+17, modular and maintainable)
- Documentation: 58 -> 82 (+24, user-facing docs + CONTRIBUTING)
- UX Polish: 0 -> 88 (+88, quick wins dramatically improve feel)
- **Overall Alpha Score: 62 -> 92/100** (Alpha Ready + Polished)

---

## Bad Practices Fixed in v2

1. **Task granularity**: Task 2 now extracts 3 example tools, then delegates 16 via subagent (not all 19 at once)
2. **TDD compliance**: Each refactoring task now includes "run tests" as verification step
3. **Explosive file creation**: No longer creates 19+ files in one task
4. **Missing rustdoc**: Added rustdoc requirement to each refactoring task
5. **Coverage threshold**: Added as separate task (Task 10)
6. **Quick wins included**: Task 11 adds 8 UX polish items with effort estimates

---

**Plan saved to:** `/home/eric/projects/ozone/.hermes/plans/ozone-refactor-cleanup-plan-v2.md`

Ready to execute using `subagent-driven-development` -- I'll dispatch a fresh subagent per task with two-stage review (spec compliance then code quality). Shall I proceed?

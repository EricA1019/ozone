# Ozone Codebase Refactor & Cleanup Implementation Plan

> **For Hermes:** Use subagent-driven-development skill to implement this plan task-by-task.

**Goal:** Refactor massive files into manageable modules, fix failing tests, clean up bloat, and prepare Ozone for alpha release (target: 85/100 readiness score).

**Architecture:** Break each file >3000 lines into smaller modules (max 1000 lines each), replace hardcoded paths with dynamic references, remove dead code/unused imports, and add missing user-facing documentation.

**Tech Stack:** Rust, Cargo, ratatui, SQLite/WAL, usearch, MCP, TDD (test-driven for all code changes).

---

## Phase 1: Fix Failing Tests (Quick Win, ~1 hour)

### Task 1: Fix hardcoded paths in ozone-mcp tests

**Objective:** Replace hardcoded `/home/eric/projects/ozone-rs` paths with dynamic `repo_root` to fix 5 failing tests.

**Files:**
- Modify: `/home/eric/projects/ozone/crates/ozone-mcp/src/lib.rs:5800-6200` (test section)
- Test: `cargo test -p ozone-mcp --lib` (verify fix)

**Step 1: Write failing test (current state)**
```bash
cd /home/eric/projects/ozone && cargo test -p ozone-mcp --lib 2>&1 | grep "FAILED"
```
Expected: 5 FAILED tests referencing `ozone-rs` paths.

**Step 2: Locate hardcoded path references**
Search for `/home/eric/projects/ozone-rs` in lib.rs:
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

### Task 2: Refactor ozone-mcp/src/lib.rs (6232 lines → modules)

**Objective:** Split 6232-line lib.rs into modular files (max 1000 lines each).

**Files:**
- Create: `crates/ozone-mcp/src/tools/mod.rs`
- Create: `crates/ozone-mcp/src/tools/workspace_status.rs`
- Create: `crates/ozone-mcp/src/tools/cargo_tool.rs`
- Create: `crates/ozone-mcp/src/tools/catalog_list.rs`
- Create: `crates/ozone-mcp/src/tools/sandbox_tool.rs`
- Create: `crates/ozone-mcp/src/tools/mock_backend_tool.rs`
- Create: `crates/ozone-mcp/src/tools/session_tool.rs`
- Create: `crates/ozone-mcp/src/tools/message_tool.rs`
- Create: `crates/ozone-mcp/src/tools/memory_tool.rs`
- Create: `crates/ozone-mcp/src/tools/search_tool.rs`
- Create: `crates/ozone-mcp/src/tools/branch_tool.rs`
- Create: `crates/ozone-mcp/src/tools/swipe_tool.rs`
- Create: `crates/ozone-mcp/src/tools/export_tool.rs`
- Create: `crates/ozone-mcp/src/tools/import_card.rs`
- Create: `crates/ozone-mcp/src/tools/launcher_smoke.rs`
- Create: `crates/ozone-mcp/src/tools/screen_nav_targets.rs`
- Create: `crates/ozone-mcp/src/tools/mock_user_tool.rs`
- Create: `crates/ozone-mcp/src/tools/screenshot_tool.rs`
- Create: `crates/ozone-mcp/src/tools/screen_check_tool.rs`
- Modify: `crates/ozone-mcp/src/lib.rs` (keep only server struct, request handling, tool dispatch)

**Step 1: Create tools directory and mod.rs**
```bash
mkdir -p /home/eric/projects/ozone/crates/ozone-mcp/src/tools
```

**Step 2: Extract workspace_status tool to tools/workspace_status.rs**
```rust
// crates/ozone-mcp/src/tools/workspace_status.rs
use crate::OzoneMcpServer;
use anyhow::Result;
use serde_json::json;

pub fn workspace_status_tool(server: &OzoneMcpServer) -> Result<crate::ToolReply> {
    let preferences_path = ozone_core::paths::preferences_path();
    let data_dir = ozone_core::paths::data_dir();
    let models_dir = ozone_core::paths::models_dir();
    let workspace_members = vec![
        "apps/ozone-mcp",
        "apps/ozone-plus",
        "crates/ozone-core",
        "crates/ozone-engine",
        "crates/ozone-inference",
        "crates/ozone-mcp",
        "crates/ozone-memory",
        "crates/ozone-persist",
        "crates/ozone-tui",
    ];

    Ok(crate::ToolReply::success(
        "Loaded workspace status".to_owned(),
        json!({
            "repoRoot": server.repo_root,
            "serverVersion": env!("CARGO_PKG_VERSION"),
            "workspaceMembers": workspace_members,
            "defaultPaths": {
                "dataDir": data_dir,
                "preferencesPath": preferences_path,
                "modelsDir": models_dir,
                "presetsPath": ozone_core::paths::presets_path(),
                "launcherPath": ozone_core::paths::launcher_path()
            }
        }),
    ))
}
```

**Step 3: Update lib.rs to import tool modules**
```rust
// In crates/ozone-mcp/src/lib.rs (top section)
mod tools;
use tools::{
    workspace_status::workspace_status_tool,
    cargo_tool::cargo_tool,
    // ... import other tool functions
};
```

**Step 4: Update tool dispatch in handle_tool_call**
```rust
// Replace:
"workspace_status" => self.workspace_status_tool()?,
// With:
"workspace_status" => workspace_status_tool(self)?,
```

**Step 5: Run tests after each extraction**
```bash
cargo test -p ozone-mcp --lib
```
Expected: All tests pass after each tool extraction.

**Step 6: Commit after each tool extraction (frequent commits)**
```bash
git add crates/ozone-mcp/src/
git commit -m "refactor(ozone-mcp): extract workspace_status tool to module"
```

---

### Task 3: Refactor ozone-tui/src/app.rs (6116 lines → modules)

**Objective:** Split 6116-line app.rs into app/mod.rs, app/event_handler.rs, app/state.rs, app/commands.rs.

**Files:**
- Create: `crates/ozone-tui/src/app/mod.rs`
- Create: `crates/ozone-tui/src/app/event_handler.rs`
- Create: `crates/ozone-tui/src/app/state.rs`
- Create: `crates/ozone-tui/src/app/commands.rs`
- Modify: `crates/ozone-tui/src/app.rs` → replace with mod declarations

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

**Step 6: Run TUI tests**
```bash
cargo test -p ozone-tui
```
Expected: All tests pass.

**Step 7: Commit**
```bash
git add crates/ozone-tui/src/app/
git commit -m "refactor(ozone-tui): split app.rs into modular components"
```

---

### Task 4: Refactor ozone-tui/src/render.rs (4319 lines → modules)

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

**Step 5: Run tests**
```bash
cargo test -p ozone-tui
```

**Step 6: Commit**
```bash
git add crates/ozone-tui/src/render/
git commit -m "refactor(ozone-tui): split render.rs into modular components"
```

---

### Task 5: Refactor apps/ozone-plus/src/main.rs (3780 lines → modules)

**Objective:** Split main.rs into main/mod.rs, main/cli.rs, main/config.rs.

**Files:**
- Create: `apps/ozone-plus/src/main/mod.rs`
- Create: `apps/ozone-plus/src/main/cli.rs`
- Create: `apps/ozone-plus/src/main/config.rs`

**Step 1: Create main directory**
```bash
mkdir -p /home/eric/projects/ozone/apps/ozone-plus/src/main
```

**Step 2: Extract CLI argument parsing to main/cli.rs**
**Step 3: Extract config loading to main/config.rs**
**Step 4: Keep main event loop in main/mod.rs**

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

### Task 6: Refactor apps/ozone-plus/src/runtime.rs (3698 lines → modules)

**Objective:** Split runtime.rs into runtime/mod.rs, runtime/stream.rs, runtime/engine.rs.

**Files:**
- Create: `apps/ozone-plus/src/runtime/mod.rs`
- Create: `apps/ozone-plus/src/runtime/stream.rs`
- Create: `apps/ozone-plus/src/runtime/engine.rs`

**Step 1: Create runtime directory**
```bash
mkdir -p /home/eric/projects/ozone/apps/ozone-plus/src/runtime
```

**Step 2: Extract streaming logic to runtime/stream.rs**
**Step 3: Extract engine interaction to runtime/engine.rs**
**Step 4: Keep runtime orchestration in runtime/mod.rs**

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
For each file with unused imports:
```bash
sed -i '/^use /p' crates/ozone-engine/src/lib.rs  # Check imports
# Remove unused ones manually
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
# Search for duplicated model path handling
grep -rn "models_dir()" /home/eric/projects/ozone/crates/
# Search for duplicated preference loading
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

## Phase 4: Documentation (User-Facing, ~4 hours)

### Task 9: Create user-facing documentation

**Objective:** Add getting-started guide, feature overview, and CONTRIBUTING.md.

**Files:**
- Create: `/home/eric/projects/ozone/docs/getting-started.md`
- Create: `/home/eric/projects/ozone/docs/features.md`
- Create: `/home/eric/projects/ozone/docs/tutorial.md`
- Create: `/home/eric/projects/ozone/CONTRIBUTING.md`

**Step 1: Write getting-started.md**
Include:
- Installation (cargo install, make install)
- First run (tier picker)
- Basic workflow (pick model → launch → chat)
- Common commands (ozone model list, ozone-plus open)

**Step 2: Write features.md**
List all features per tier:
- ozonelite: lean model management
- ozone: launcher + monitor + profiling
- ozone+: persistent chat + memory + branching + swipes

**Step 3: Write tutorial.md**
Step-by-step:
1. Add a GGUF model
2. Profile and launch
3. Open ozone+ and start chatting
4. Use memory pins and search
5. Export session

**Step 4: Write CONTRIBUTING.md**
Include:
- Code style (clippy-clean, 1000-line max per file)
- Testing (TDD, cargo test)
- PR process (sign DCO, link issues)
- Module structure (modular crates, feature flags)

**Step 5: Commit**
```bash
git add docs/ CONTRIBUTING.md
git commit -m "docs: add user-facing guides and contributing guide"
```

---

## Verification Checklist (After All Tasks)

- [ ] All 143+ tests pass (0 failing)
- [ ] No file exceeds 1000 lines (except auto-generated)
- [ ] `cargo clippy --workspace -- -D warnings` passes
- [ ] `cargo test --workspace` passes
- [ ] User-facing docs exist (getting-started.md, features.md, tutorial.md)
- [ ] CONTRIBUTING.md exists
- [ ] No hardcoded paths in test code
- [ ] All modules are properly documented with rustdoc

---

## Post-Refactor Alpha Score Estimate

After completing all tasks:
- Code Hygiene: 58 → 88 (+30)
- Test Coverage: 62 → 85 (+23, all tests pass, coverage threshold added)
- Architecture: 75 → 90 (+15, modular and maintainable)
- Documentation: 58 → 80 (+22, user-facing docs added)
- **Overall Alpha Score: 62 → 88/100** (Alpha Ready)

---

**Plan saved to:** `/home/eric/projects/ozone/.hermes/plans/ozone-refactor-cleanup-plan.md`

Ready to execute using `subagent-driven-development` — I'll dispatch a fresh subagent per task with two-stage review (spec compliance then code quality). Shall I proceed?

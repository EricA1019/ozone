# Ozone v0.5 — Flash Handoff Plan (Revised)

**Date:** 2026-06-25
**For:** Flash model execution via ForgeWrite MCP tools
**Prerequisite:** Server on :8989, ForgeWrite tools enabled in VS Code
**Rule:** Use ForgeWrite tools for ALL file operations. Do NOT use direct file tools.

---

## Tool Call Guide

Every task uses this pattern. Copy the JSON exactly, filling in only the
`<PLACEHOLDER>` values:

### Step 1: Validate contracts
```
TOOL: fw_validate_handoff
PARAMS: {"handoff": {"schema_id":"forgerwrite.handoff.v1","project":"ozone","language":"rust","description":"<SLICE DESCRIPTION>"}}
```
```
TOOL: fw_validate_slice
PARAMS: {"slice_contract": {"schema_id":"forgerwrite.slice.v1","slice_id":"<SLICE_ID>","allowed_files":["<FILE1>","<FILE2>",...],"operation_types":["<OP_TYPE>"],"description":"<DESCRIPTION>"}}
```

### Step 2: Validate operations
```
TOOL: fw_validate_operations
PARAMS: {"operation_batch": {"batch_id":"<SLICE_ID>","slice_id":"<SLICE_ID>","operations":[...]},"slice_contract": {"schema_id":"forgerwrite.slice.v1","slice_id":"<SLICE_ID>","allowed_files":["<FILE>",...],"operation_types":["<OP_TYPE>"]}}
```

### Step 3: Preview
```
TOOL: fw_preview_operations
PARAMS: {"operation_batch": {...},"slice_contract": {...},"run_id": "<SLICE_ID>"}
```

### Step 4: Apply
```
TOOL: fw_apply_approved_operations
PARAMS: {"operation_batch": {...},"slice_contract": {...},"run_id": "<SLICE_ID>"}
```

### Step 5: Verify
```
TERMINAL: cd /home/eric/projects/ozone && cargo check 2>&1
```
After Phase A: also run `cargo test 2>&1`

### If `fw_preview_operations` fails with "Worktree is not clean":
```
TERMINAL: cd /home/eric/projects/ozone && git stash && echo "stashed"
```
Then retry the preview.

### If a slice fails validation:
Read the error message. Fix only what it says is wrong. Retry from Step 2.

---

## Phase A: Atomic ozone+ Removal (1 slice, 3 files)

**This ONE slice removes `Tier::Plus` and `FrontendPreference::OzonePlus` from
all source files simultaneously. Do not split this. Do all 3 files in one batch.**

### Slice A — Remove ozone+ from prefs.rs + main.rs + launcher.rs

**Slice ID:** `A-ozone-plus-removal`
**Allowed files:** `src/prefs.rs`, `src/main.rs`, `src/ui/launcher.rs`
**Operation types:** `replace_line_range`

**Operation 1 — `src/prefs.rs`: Remove Tier::Plus variant**
Type: `replace_line_range`
Path: `src/prefs.rs`
Start line: 17
End line: 17
Content: (empty — removes the line `    Plus,`)

**Operation 2 — `src/prefs.rs`: Remove OzonePlus variant**
Type: `replace_line_range`
Path: `src/prefs.rs`
Start line: 23
End line: 24
Content: (empty)

**Operation 3 — `src/prefs.rs`: Replace Plus coercion with compile error**
Type: `replace_line_range`
Path: `src/prefs.rs`
Start line: 113
End line: 124
Content:
```
        Some(Tier::Plus) => {
            // ozone+ tier removed in v0.5. This arm exists only to
            // prevent deserialization panics on legacy prefs files.
            // Users with Tier::Plus in saved prefs will be reset to Base.
            eprintln!("ozone+ tier was removed in v0.5. Resetting to Base.");
            None
        }
```

**Operation 4 — `src/prefs.rs`: Replace test with deprecation test**
Type: `replace_line_range`
Path: `src/prefs.rs`
Start line: 524
End line: 570
Content:
```
        fn load_prefs_rejects_legacy_plus_tier() {
            use crate::test_support::TestSandbox;
            let sandbox = TestSandbox::new("reject-plus-tier");
            let raw = r#"{
                "version": 1,
                "last_model_name": "",
                "no_browser": false,
                "side_by_side_monitor": false,
                "preferred_frontend": "ozone-plus",
                "preferred_tier": "plus"
            }"#;
            sandbox.write_prefs(raw);
            let prefs = super::load_prefs(&sandbox.path).expect("should load");
            assert_eq!(prefs.preferred_tier, None,
                "legacy plus tier should be rejected, not coerced to Base");
            assert_eq!(prefs.preferred_frontend, None,
                "ozone-plus frontend should be rejected");
        }
```

**Operation 5 — `src/main.rs`: Remove Plus from TierArg enum**
Type: `replace_line_range`
Path: `src/main.rs`
Start line: 36
End line: 40
Content:
```
pub enum TierArg {
    Lite,
    Base,
}
```

**Operation 6 — `src/main.rs`: Remove Plus from from-impl**
Type: `replace_line_range`
Path: `src/main.rs`
Start line: 41
End line: 47
Content:
```
impl From<TierArg> for prefs::Tier {
    fn from(arg: TierArg) -> Self {
        match arg {
            TierArg::Lite => prefs::Tier::Lite,
            TierArg::Base => prefs::Tier::Base,
        }
    }
}
```

**Operation 7 — `src/main.rs`: Update mode help text**
Type: `replace_line_range`
Path: `src/main.rs`
Start line: 70
End line: 78
Content:
```
    /// Override product tier (lite, base).
    /// Also detectable via binary name (e.g. `oz`).
    #[arg(long, value_enum)]
    mode: Option<TierArg>,
```

**Operation 8 — `src/ui/launcher.rs`: Remove ozone-plus action**
Type: `replace_line_range`
Path: `src/ui/launcher.rs`
Start line: 2140
End line: 2155
Content:
```
        // ozone+ actions removed in v0.5
```

**Verify after this slice:**
```
TERMINAL: cd /home/eric/projects/ozone && cargo check 2>&1 && cargo test 2>&1
```
Expected: `cargo check` passes. `cargo test` passes with the new
`load_prefs_rejects_legacy_plus_tier` test.

---

## Phase B: Delete ozone-plus app (2 slices)

### Slice B1 — Delete core + runtime files (30 files)

**Slice ID:** `B1-delete-ozone-plus-core`
**Allowed files:** All listed below (prefix each with `apps/ozone-plus/src/`)
**Operation types:** `delete_file`

Delete these 30 files (one operation per file):
```
apps/ozone-plus/src/main.rs
apps/ozone-plus/src/lib.rs
apps/ozone-plus/src/config.rs
apps/ozone-plus/src/hooks.rs
apps/ozone-plus/src/context_bridge.rs
apps/ozone-plus/src/hybrid_search.rs
apps/ozone-plus/src/index_rebuild.rs
apps/ozone-plus/src/inference_adapter.rs
apps/ozone-plus/src/session_title.rs
apps/ozone-plus/src/store.rs
apps/ozone-plus/src/runtime.rs
apps/ozone-plus/src/runtime/bootstrap.rs
apps/ozone-plus/src/runtime/commands.rs
apps/ozone-plus/src/runtime/context.rs
apps/ozone-plus/src/runtime/generation.rs
apps/ozone-plus/src/runtime/management.rs
apps/ozone-plus/src/runtime/message_actions.rs
apps/ozone-plus/src/runtime/persisted_draft.rs
apps/ozone-plus/src/runtime/recall_helpers.rs
apps/ozone-plus/src/runtime/reroll.rs
apps/ozone-plus/src/runtime/shell_commands.rs
apps/ozone-plus/src/runtime/tests.rs
apps/ozone-plus/src/runtime/types.rs
apps/ozone-plus/src/cli/mod.rs
apps/ozone-plus/src/cli/args.rs
apps/ozone-plus/src/cli/branch.rs
apps/ozone-plus/src/cli/create.rs
apps/ozone-plus/src/cli/edit.rs
apps/ozone-plus/src/cli/events.rs
apps/ozone-plus/src/cli/export.rs
```

### Slice B2 — Delete CLI files + Cargo.toml (16 files)

**Slice ID:** `B2-delete-ozone-plus-cli`
**Allowed files:** All listed below
**Operation types:** `delete_file`

```
apps/ozone-plus/src/cli/gc.rs
apps/ozone-plus/src/cli/identity.rs
apps/ozone-plus/src/cli/import.rs
apps/ozone-plus/src/cli/index.rs
apps/ozone-plus/src/cli/lifecycle.rs
apps/ozone-plus/src/cli/memory.rs
apps/ozone-plus/src/cli/open.rs
apps/ozone-plus/src/cli/prefs.rs
apps/ozone-plus/src/cli/print.rs
apps/ozone-plus/src/cli/search.rs
apps/ozone-plus/src/cli/send.rs
apps/ozone-plus/src/cli/summarize.rs
apps/ozone-plus/src/cli/swipe.rs
apps/ozone-plus/src/cli/transcript.rs
apps/ozone-plus/src/cli/util.rs
apps/ozone-plus/Cargo.toml
```

**Verify after B2:**
```
TERMINAL: cd /home/eric/projects/ozone && cargo check 2>&1
```

---

## Phase C: Strip ozone-tui chat code (4 slices)

### Slice C1 — Delete chat-only files (6 files)

**Slice ID:** `C1-delete-tui-chat`
**Allowed files:** All listed below (prefix with `crates/ozone-tui/src/`)
**Operation types:** `delete_file`

```
crates/ozone-tui/src/app/shell_state/slash.rs
crates/ozone-tui/src/render/conversation.rs
crates/ozone-tui/src/render/composer.rs
crates/ozone-tui/src/render/overlays.rs
crates/ozone-tui/src/mock.rs
crates/ozone-tui/src/app/shell_state/draft_input.rs
```

**Verify:**
```
TERMINAL: cd /home/eric/projects/ozone && cargo check -p ozone-tui 2>&1
```
This WILL fail because other files import these deleted modules.
That is expected. Continue to C2.

### Slice C2 — Delete lifecycle files + fix imports (4 files)

**Slice ID:** `C2-delete-tui-lifecycle`
**Allowed files:**
- `crates/ozone-tui/src/app/shell_state/lifecycle.rs`
- `crates/ozone-tui/src/app/shell_state/runtime_events.rs`
- `crates/ozone-tui/src/runtime/reroll.rs` (if exists in tui crate)
- `crates/ozone-tui/src/app/shell_state/mod.rs`
**Operation types:** `delete_file` (first 3), `replace_file` (mod.rs)

**Operation 1-3:** Delete lifecycle.rs, runtime_events.rs, and any reroll equivalent.

**Operation 4 — `crates/ozone-tui/src/app/shell_state/mod.rs`:**
Replace the entire file with:
```rust
pub mod key_handling;
pub mod utils;

pub use key_handling::*;
pub use utils::*;
```

**Verify:**
```
TERMINAL: cd /home/eric/projects/ozone && cargo check -p ozone-tui 2>&1
```
Expected: compilation errors are reduced but may still exist from C1 deletions.
Continue to C3.

### Slice C3 — Clean up enums_runtime.rs (remove chat variants)

**Slice ID:** `C3-clean-tui-enums`
**Allowed files:** `crates/ozone-tui/src/state/enums_runtime.rs`
**Operation types:** `replace_line_range`

**IMPORTANT — USER ACTION REQUIRED:**
Before running this slice, examine `crates/ozone-tui/src/state/enums_runtime.rs`
line by line. Identify which enum variants are chat/roleplay-specific.
Delete ONLY those variants. Keep all infrastructure variants (screen state,
focus, theme, layout). This file is 1,368 lines — do not attempt a full
replace. Use targeted `replace_line_range` operations for each chat variant
you identify.

After removing each chat variant, also remove any `match` arms that reference
the deleted variant throughout the same file.

**Verify:**
```
TERMINAL: cd /home/eric/projects/ozone && cargo check -p ozone-tui 2>&1
```

### Slice C4 — Clean up key_handling.rs (remove chat keybinds)

**Slice ID:** `C4-clean-tui-keybinds`
**Allowed files:** `crates/ozone-tui/src/app/shell_state/key_handling.rs`
**Operation types:** `replace_line_range`

**USER GUIDANCE NEEDED:**
Identify keybinds that are chat-specific (send message, reroll, swipe, branch,
edit message, etc.). Remove only those match arms. Keep navigation keybinds
(up/down, tab, esc, enter for menu selection).

**Verify:**
```
TERMINAL: cd /home/eric/projects/ozone && cargo check -p ozone-tui 2>&1 && cargo test 2>&1
```

---

## Phase D: Documentation (1 slice)

### Slice D — Move docs + update README

**Slice ID:** `D-docs-archive`
**Allowed files:** `README.md`, `ozone+/README.md`, `ozone+/ozone_v0.4_design.md`,
  `ozone+/ozone_plus_documentation_stack.md`, `ozone+/ozonelite_scope.md`
**Operation types:** `create_file`, `replace_line_range`

**Operation 1 — Create deprecation notice:**
Type: `create_file`
Path: `docs/archive/ozone-plus/_DEPRECATED.md`
Content:
```
# Ozone+ — DEPRECATED (v0.5.0-alpha)

The ozone+ chat/roleplay features were deprecated in v0.5.0-alpha.
Ozone now focuses on backend management, eval/benchmark, and profiling.

These files are kept for historical reference only.
They will be removed in a future release.
```

**Operation 2-5 — Not possible via ForgeWrite (file moves are not supported).**
Instead, use the terminal:
```
TERMINAL:
cd /home/eric/projects/ozone
mkdir -p docs/archive/ozone-plus
cp ozone+/README.md docs/archive/ozone-plus/
cp ozone+/ozone_v0.4_design.md docs/archive/ozone-plus/
cp ozone+/ozone_plus_documentation_stack.md docs/archive/ozone-plus/
cp ozone+/ozonelite_scope.md docs/archive/ozone-plus/
```

**Operation 6 — Update README.md product tiers table:**
Type: `replace_line_range`
Path: `README.md`
Find the product tiers table (search for "ozone+" or "Plus"). Replace:
- The ozone+ row with nothing (remove it)
- Update the header to show 2 tiers instead of 3

**Operation 7 — Update README.md quick start:**
Remove any line containing `ozone --mode=plus` or `ozone-plus`.

**Operation 8 — Update README.md philosophy:**
Replace "Chat shell with memory & sessions" text. Update "local-LLM conversation experience" references to "eval/benchmark workflows".

---

## Phase E: Feature Gates (1 slice)

### Slice E — Remove #[cfg(feature)] gates

**Slice ID:** `E-remove-feature-gates`
**Allowed files:** `src/main.rs`
**Operation types:** `replace_line_range`

**Operation 1 — Remove bench gate on line 1:**
Type: `replace_line_range`
Path: `src/main.rs`
Start line: 1
End line: 2
Content:
```
mod analyze;
mod bench;
```

**Operation 2 — Remove gate on line 10:**
Type: `replace_line_range`
Path: `src/main.rs`
Start line: 10
End line: 11
Content:
```
mod db;
```

**Operation 3 — Remove gate on line 12:**
Type: `replace_line_range`
Path: `src/main.rs`
Start line: 12
End line: 13
Content:
```
mod gguf;
```

**Operation 4 — Remove gate on line 16:**
Type: `replace_line_range`
Path: `src/main.rs`
Start line: 16
End line: 17
Content:
```
mod model;
```

**Operation 5 — Remove gate on line 21 (profiling-ui):**
Type: `replace_line_range`
Path: `src/main.rs`
Start line: 21
End line: 22
Content:
```
mod profiling;
```

**Operation 6 — Remove gate on line 23 (sweep):**
Type: `replace_line_range`
Path: `src/main.rs`
Start line: 23
End line: 24
Content:
```
mod sweep;
```

**Remove #[cfg] annotations from all bench/sweep/analyze CLI commands.**
In `src/main.rs`, find every `#[cfg(feature = "bench")]`, `#[cfg(feature = "sweep")]`,
and `#[cfg(feature = "analyze")]` on CLI command definitions and delete those lines.

**Update src/main.rs line 60 (after_help):**
Type: `replace_line_range`
Path: `src/main.rs`
Start line: 58
End line: 60
Content:
```
    after_help = "All eval, benchmark, sweep, and profiling features are always available.",
```

**Verify:**
```
TERMINAL: cd /home/eric/projects/ozone && cargo check --all-features 2>&1 && cargo test 2>&1
```

---

## Summary: 9 Slices

| Slice | What | Files | Ops |
|-------|------|-------|-----|
| A | Remove Tier::Plus + OzonePlus | 3 | 8 |
| B1 | Delete ozone-plus core | 30 | 30 |
| B2 | Delete ozone-plus CLI | 16 | 16 |
| C1 | Delete chat TUI files | 6 | 6 |
| C2 | Delete lifecycle + fix imports | 4 | 4 |
| C3 | Clean enums_runtime.rs | 1 | N (user-guided) |
| C4 | Clean key_handling.rs | 1 | N (user-guided) |
| D | Archive docs + update README | 6 | 8 |
| E | Remove feature gates | 1 | ~10 |

## Validation Checklist (complete after all slices)

- [ ] `Tier::Plus` appears 0 times in `src/` and `crates/`
- [ ] `FrontendPreference::OzonePlus` appears 0 times
- [ ] `apps/ozone-plus/` directory does not exist
- [ ] `cargo check` passes on base crate
- [ ] `cargo check -p ozone-tui` passes
- [ ] `cargo test` passes with zero new failures
- [ ] `oz --help` output does not mention `plus`
- [ ] `oz list --json` works
- [ ] `README.md` has 2 product tiers (not 3)

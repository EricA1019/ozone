# Phase 1G: Journey Builder Extraction Strategy

**Status:** Complete (2026-05-12)
**Baseline:** All 20 tests passing, compilation clean
**Scope:** Move 22 journey builder functions (~665 lines) from lib.rs to testing/journey.rs

## Functions to Extract

### Core Orchestrators (4 functions)
- `build_mock_user_journey()` — lines 402-480, calls build_capturable_screen_journey
- `build_capturable_screen_journey()` — lines 578-586, core routing function
- `capturable_screen_definition()` — lines 588-601, looks up screen definitions
- `screen_nav_target_data()` — lines 602-621, builds screen metadata

### Base UI Builders (16 functions)
All `build_base_*_screen_journey()` functions (lines 622-902):
1. `build_base_splash_screen_journey()` — 622-641
2. `build_base_tier_picker_screen_journey()` — 642-662
3. `build_base_launcher_screen_journey()` — 663-686
4. `build_base_exit_confirm_screen_journey()` — 687-701
5. `build_base_settings_screen_journey()` — 702-721
6. `build_base_model_picker_launch_screen_journey()` — 722-736
7. `build_base_confirm_launch_screen_journey()` — 737-752
8. `build_base_frontend_choice_screen_journey()` — 753-768
9. `build_base_launching_screen_journey()` — 769-788
10. `build_base_monitor_screen_journey()` — 789-815
11. `build_base_model_picker_profile_screen_journey()` — 816-837
12. `build_base_profile_advisory_screen_journey()` — 838-853
13. `build_base_profile_confirm_screen_journey()` — 854-869
14. `build_base_profile_running_screen_journey()` — 870-885
15. `build_base_profile_failure_screen_journey()` — 886-901
16. `build_base_ozone_plus_shell_journey()` — 902-936

### OzonePlus UI Builders (6 functions)
All `build_ozone_plus_*_screen_journey()` functions (lines 937-1051):
1. `build_ozone_plus_main_menu_screen_journey()` — 937-954
2. `build_ozone_plus_sessions_screen_journey()` — 955-970
3. `build_ozone_plus_characters_screen_journey()` — 971-986
4. `build_ozone_plus_settings_screen_journey()` — 987-1002
5. `build_ozone_plus_character_create_screen_journey()` — 1003-1018
6. `build_ozone_plus_character_import_screen_journey()` — 1019-1034
7. `build_ozone_plus_conversation_screen_journey()` — 1035-1050
8. `build_ozone_plus_help_screen_journey()` — 1051-1066

### Binary Command Helper (1 function)
- `front_door_binary_command()` — lines 1067-1092

## Key Technical Considerations

### Dependency on `self`
All functions use `&self` to access:
- `self.repo_root` — PathBuf for repository root (used by ALL functions)
- `self.front_door_binary_command()` — method called by all builders

**Solution:** Convert `&self` parameter to `repo_root: &Path` parameter in extracted functions.

### Extraction Pattern

#### Current (in lib.rs):
```rust
impl OzoneMcpServer {
    fn build_base_splash_screen_journey(&self, journey_name: &str, _args: &Value) -> Result<MockUserJourneySpec> {
        Ok(MockUserJourneySpec {
            name: journey_name.to_owned(),
            cwd: self.repo_root.to_string_lossy().into_owned(),
            command: append_args(
                &self.front_door_binary_command("ozone", &["--mode", "base"]),
                &["--no-browser"],
            ),
            ...
        })
    }
}
```

#### Target (in testing/journey.rs):
```rust
pub fn build_base_splash_screen_journey(
    repo_root: &Path,
    journey_name: &str,
    _args: &Value,
) -> Result<MockUserJourneySpec> {
    Ok(MockUserJourneySpec {
        name: journey_name.to_owned(),
        cwd: repo_root.to_string_lossy().into_owned(),
        command: append_args(
            &front_door_binary_command(repo_root, "ozone", &["--mode", "base"]),
            &["--no-browser"],
        ),
        ...
    })
}
```

### Call Site Updates in lib.rs

Update all call sites from:
```rust
self.build_base_splash_screen_journey(name, args)
```

To:
```rust
testing::build_base_splash_screen_journey(&self.repo_root, name, args)
```

## Implementation Roadmap

### Phase 1G-1: Extract Helper (front_door_binary_command)
1. Move `front_door_binary_command` to testing/journey.rs, add `repo_root` parameter
2. Update all call sites (22 locations within journey builders)
3. Verify cargo check passes

### Phase 1G-2: Extract Core Orchestrators
1. Move `build_capturable_screen_journey` (calls front_door_binary_command and capturable_screen_definition)
2. Move `capturable_screen_definition` (calls capturable_screen_journey_builders)
3. Move `screen_nav_target_data` (calls build_capturable_screen_journey)
4. Move `build_mock_user_journey` (calls build_capturable_screen_journey)
5. Verify tests pass

### Phase 1G-3: Batch Extract Base Builders
1. Move all 16 `build_base_*` functions as a group (line 622-902)
2. Update dependencies within group (some call others)
3. Verify tests pass
4. Verify no regressions in journey execution

### Phase 1G-4: Batch Extract OzonePlus Builders
1. Move all 8 `build_ozone_plus_*` functions (lines 937-1066)
2. Verify tests pass

### Phase 1G-5: Final Validation
1. Verify all 20 tests still pass
2. Verify lib.rs line count reduced from 3477 → ~2812 lines (~665 line reduction)
3. Verify no dead code warnings

## Files to Modify

- `crates/ozone-mcp/src/testing/journey.rs` — add 665 lines of extracted functions
- `crates/ozone-mcp/src/lib.rs` — remove 665 lines, update 25+ call sites
- No changes needed to:
  - testing/mod.rs (already exports journey module)
  - testing/types.rs (already has all needed types)
  - testing/screen.rs (independent)
  - Tests (should continue to work as-is)

## Testing Strategy

After each sub-phase:
1. Run `cargo check -p ozone-mcp` to verify syntax
2. Run `cargo test -p ozone-mcp --lib` to verify all 20 tests pass
3. Verify call site updates are correct by checking compiler errors

## Success Criteria

- ✅ All 20 tests pass
- ✅ Clean compilation with no warnings
- ✅ Journey builder implementations moved from `lib.rs` into `testing/journey.rs`
- ✅ Core orchestrator logic (`build_mock_user_journey`, capturable-screen dispatch/lookup, screen-nav metadata) extracted into `testing/journey.rs`
- ✅ `capturable_screen_journey_builders()` now points directly at extracted testing builders
- ✅ Validation passed with `cargo test -p ozone-mcp --quiet`

## Approximate Time Investment

- Phase 1G-1 (helper): 10 minutes
- Phase 1G-2 (core orchestrators): 15 minutes
- Phase 1G-3 (base builders): 20 minutes
- Phase 1G-4 (ozonePlus builders): 15 minutes
- Phase 1G-5 (validation): 10 minutes
- **Total:** ~70 minutes for complete Phase 1G

## Notes for Next Session

- Use `multi_replace_string_in_file` for batch call-site updates
- Extract helper functions FIRST to minimize call-site changes
- Group related builders for batch extraction to reduce context switching
- Always validate with `cargo test --lib` rather than just cargo check

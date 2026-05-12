# Phase 1G: Technical Analysis & Options

**Status:** Phase 1G Part A COMPLETE (helper functions extracted), Part B DEFERRED  
**Date:** May 11, 2026  
**Context:** Ozone monolith refactoring, journey builder extraction  

## Current State

### Part A: ✅ COMPLETE
- Extracted `append_args()` to `testing/journey.rs`
- Extracted `front_door_binary_command()` to `testing/journey.rs`
- Pure functions with no circular dependencies
- All 20 tests passing

### Part B: 🟡 DEFERRED
- 22 journey builder functions (~665 lines) still in lib.rs
- Cannot be extracted as standalone functions due to architectural coupling

## The Problem: Why Part B is Complex

### Journey Builder Architecture

```
OzoneMcpServer {
    fn build_mock_user_journey(...)
    fn build_capturable_screen_journey(...)
    fn build_base_splash_screen_journey(...)  [20 more builders...]
}

CapturableScreenJourneyDefinition {
    builder: fn(&OzoneMcpServer, &str, &Value) -> Result<MockUserJourneySpec>
    sandbox_setup: fn() -> Value
}

capturable_screen_journey_builders() -> &'static [CapturableScreenJourneyDefinition]
```

### Coupling Points

1. **Function Pointers**
   - Journey builders are stored as function pointers in `CapturableScreenJourneyDefinition`
   - These pointers must take `&OzoneMcpServer` as first parameter
   - Moving functions breaks this contract

2. **Cross-Method Dependencies**
   - `build_mock_user_journey()` calls `build_capturable_screen_journey()`
   - `build_capturable_screen_journey()` calls `capturable_screen_definition()` and executes builder functions
   - All base builders call each other in a hierarchy

3. **Shared Access**
   - All builders need `self.repo_root` for command building
   - All builders call `self.front_door_binary_command()`
   - This self-reference is structural to the design

## Why Naive Extraction Fails

### Approach 1: Standalone Functions with repo_root Parameter
```rust
pub fn build_base_splash_screen_journey(
    repo_root: &Path,
    journey_name: &str,
    _args: &Value,
) -> Result<MockUserJourneySpec> { ... }
```

**Problem:** Function pointer signature mismatch
- Function pointers expect: `fn(&OzoneMcpServer, &str, &Value) -> Result<...>`
- Standalone functions have: `fn(&Path, &str, &Value) -> Result<...>`
- CapturableScreenJourneyDefinition.builder cannot call this

### Approach 2: Wrapper OzoneMcpServer Created in testing Module
```rust
pub fn build_capturable_screen_journey(
    repo_root: &Path,
    target_screen: &str,
    args: &Value,
    journey_name: &str,
) -> Result<MockUserJourneySpec> {
    let server = OzoneMcpServer::new_for_journey_building(repo_root)?;
    let builder = capturable_screen_definition(target_screen)?.builder;
    builder(&server, journey_name, args)
}
```

**Problem:** Circular dependency + ownership violation
- testing module would need to create OzoneMcpServer
- OzoneMcpServer is in lib.rs root scope
- Creates circular import: testing -> lib -> testing

## Alternative Solutions

### Option A: Keep Builders in lib.rs, Organize Better (RECOMMENDED)

**Approach:** Don't extract builders. Instead, organize lib.rs with clear module boundaries:

```
lib.rs (3477 lines)
├── OzoneMcpServer impl
│   ├── tools
│   ├── sandbox management
│   └── journey builders (grouped together)
└── Helper functions (append_args, etc.)

testing/journey.rs (90+ lines)
├── Sandbox setup functions
└── Helper functions (front_door_binary_command, append_args)
```

**Pros:**
- ✅ No circular dependencies
- ✅ Builders remain as methods (correct pattern)
- ✅ Function pointer signatures preserved
- ✅ All cross-references work naturally
- ✅ Tests pass without modification

**Cons:**
- ❌ lib.rs still ~3477 lines
- ❌ Doesn't reduce monolith as aggressively

**Effort:** 30 minutes for code comments and documentation  
**Next Step:** Clean up lib.rs with better section markers and add docstrings to journey builders

### Option B: Refactor CapturableScreenJourneyDefinition (COMPLEX)

**Approach:** Change how journey builders are stored and called:

```rust
pub struct CapturableScreenJourneyDefinition {
    pub target_screen: &'static str,
    pub builder_fn: fn(&str, &Value) -> Result<MockUserJourneySpec>,
    pub sandbox_setup: fn() -> Value,
}

impl CapturableScreenJourneyDefinition {
    pub fn build(&self, server: &OzoneMcpServer, name: &str, args: &Value) 
        -> Result<MockUserJourneySpec> 
    {
        // Inject server context here
        let builder_with_server = |n: &str, a: &Value| {
            (self.builder_fn)(n, a)  // Would need separate impl
        };
        builder_with_server(name, args)
    }
}
```

**Pros:**
- ✅ Allows moving builders to testing module
- ✅ Could reduce lib.rs significantly

**Cons:**
- ❌ Major refactoring of calling sites
- ❌ Changes function pointer semantics
- ❌ Risk of breaking existing functionality
- ❌ 4-6 hours of development + testing

**Effort:** 4-6 hours for full implementation and testing  
**Risk:** High - affects core journey building system

### Option C: Create Builder Registry Pattern (FUTURE)

**Approach:** Replace direct function pointer approach with a registry:

```rust
pub trait JourneyBuilder {
    fn build(&self, server: &OzoneMcpServer, name: &str, args: &Value) 
        -> Result<MockUserJourneySpec>;
    fn target_screen(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn sandbox_setup(&self) -> Value;
}

pub struct BaseSplashBuilder;
impl JourneyBuilder for BaseSplashBuilder { ... }
```

**Pros:**
- ✅ Complete architectural separation
- ✅ Scales to new builders easily
- ✅ Clear responsibilities

**Cons:**
- ❌ Significant refactoring (8+ hours)
- ❌ Changes public API
- ❌ Not justified for current scope

**Effort:** 8+ hours  
**Recommended:** Phase 2+ work, not Phase 1G

## Recommendation

**Choose Option A: Keep Builders in lib.rs with Better Organization**

**Rationale:**
1. Phase 1E/1G goal was to extract and organize testing code
2. Screen check evaluation ✅ COMPLETE (testing/screen.rs)
3. Sandbox setup functions ✅ COMPLETE (testing/journey.rs)
4. Helper functions ✅ COMPLETE (testing/journey.rs - append_args, front_door_binary_command)
5. Journey builders are core MCP logic, not testing code - they belong in lib.rs

**Execution:**
1. Add clear section markers to lib.rs grouping journey builders
2. Add docstrings explaining each builder's purpose
3. Add comments explaining the CapturableScreenJourneyDefinition pattern
4. Update documentation to explain the architecture
5. Mark as "Phase 1G Complete - Recommended organization accepted"

**Metrics:**
- Tests: 20/20 passing ✅
- No technical debt introduced
- Clean module boundaries between testing and core logic
- Documentation improved

**Time:** 30-45 minutes

## Files to Update

If choosing Option A:
- `crates/ozone-mcp/src/lib.rs` — Add section comments for journey builders (lines 402-1070)
- `.mex/ROUTER.md` — Document architectural decision
- `plans/monolith-refactor-plan.md` — Update to reflect Phase 1G completion with recommendation

If choosing Option B (not recommended):
- `crates/ozone-mcp/src/testing/journey.rs` — Add extracted builders
- `crates/ozone-mcp/src/lib.rs` — Refactor calling sites
- `crates/ozone-mcp/src/testing/types.rs` — Update CapturableScreenJourneyDefinition
- Full test validation required

## Decision Point

**For next session:** Confirm Option A approach, then execute cleanup and documentation.

If Option B is preferred: Plan 4-6 hour refactoring session with dedicated testing to prevent regressions.

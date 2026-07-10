# Wave 1 — Monolith Splitting (Detailed Plan)

> Target: Architecture +2, Maintainability +1
> 3 sub-items, ~25h estimated total

---

## Item 1.1 — Split `crates/ozone-mcp/src/lib.rs` (2,287L → target ~400L)

**Current structure** of `lib.rs` (2,287 lines):

| Section | Lines | What |
|---|---|---|
| Imports + constants + helpers | 1-147 | Module decls, `env_flag_enabled`, `run_stdio_server`, `OzoneMcpServer::new()` |
| `OzoneMcpServer` impl (tool methods) | 148-689 | `screenshot_tool`, `screen_check_tool`, `build_mock_user_journey`, `prepare_mock_user_sandbox`, `run_mock_user_journey`, `run_python_vte_helper`, `with_repo`, `with_sandbox_env`, `run_workspace_command` |
| `Drop for OzoneMcpServer` | 690-697 | Drop cleanup |
| `capturable_screen_journey_builders()` | 698-1197 | Journey definitions + `CapturableScreenJourneyDefinition` struct + `CapturableScreenDefinition` |
| `PtyVteCaptureConfig` + `PtyVteCaptureArtifacts` | 1198-1236 | PTY capture config structs + impls |
| Screenshot/mock-user helpers | 1237-1376 | `screenshot_capture_config`, `mock_user_capture_settings`, `screenshot_file_stem` |
| `ToolDefinition` struct + `tool_definitions()` | 1377-1844 | All 19 tool definitions (~460 lines!) |
| `ToolReply` + `CommandOutput` + `EnvOverrideGuard` | 1845-1913 | Reply/output structs |
| `required_*` / `optional_*` helpers | 1914-1981 | Arg parsing helpers |
| ID + preferences + JSON helpers | 1982-2285 | `parse_session_id`, `now_timestamp_ms`, `merge_json_objects`, `session_summary_json`, etc. |
| `mod tests` | 2286-end | External `tests.rs` file |

**Target file layout** (6 files + 1 existing expanded):

```
crates/ozone-mcp/src/
  lib.rs              (~400L)  — Thin: imports, module decls, run_stdio_server(), OzoneMcpServer::new()
  server.rs           (~550L)  — OzoneMcpServer impl: tool methods, journey methods, sandbox methods, Drop
  tool_defs.rs        (~460L)  — ToolDefinition struct + tool_definitions() + all 19 schema entries
  types.rs            (~130L)  — ToolReply, CommandOutput, EnvOverrideGuard
  arg_helpers.rs      (~180L)  — required_string, optional_string, command_output_data, etc.
  persist_helpers.rs  (~300L)  — parse_session_id, merge_json_objects, session_summary_json, etc.
  testing/screen.rs   (expand) — Move PtyVteCaptureConfig, screenshot_capture_config here
  testing/journey.rs  (expand) — Move capturable_screen_journey_builders here
```

### Step-by-step

#### Step 1.1.1 — Create `src/server.rs` (2h)
1. Move `OzoneMcpServer` struct (line 148)
2. Move `impl OzoneMcpServer { }` block (lines 153-689)
3. Move `impl Drop for OzoneMcpServer` (lines 690-697)
4. Make `OzoneMcpServer` public (`pub(crate)` or `pub`)
5. Re-export from `lib.rs`: `mod server; pub use self::server::OzoneMcpServer;`
6. Fix visibility: some methods on OzoneMcpServer are currently `pub` — change to `pub(crate)` where not part of MCP protocol

**Functions to move:**
- `screenshot_tool`, `screen_check_tool`
- `build_mock_user_target_journey`, `prepare_mock_user_sandbox`, `prepare_target_sandbox`
- `annotate_prepared_sandbox`, `run_mock_user_journey`
- `run_python_vte_helper`, `with_repo`, `with_sandbox_env`, `run_workspace_command`
- `capturable_target_sandbox_setup`, `recommended_mock_user_journey_sandbox_setup`
- `prepare_sandbox_from_setup`, `build_capturable_screen_journey`
- `capturable_screen_definition`, `screen_nav_target_data`

#### Step 1.1.2 — Create `src/tool_defs.rs` (1.5h)
1. Move `ToolDefinition` struct (line 1377)
2. Move `tool_definitions()`, `scoped_tool_definitions()`, `all_tool_definitions()` (lines 1386-1844)
3. Keep `tool_definitions()` as the public entry point
4. Make `ToolDefinition` `pub(crate)` — it's an internal type
5. Re-export from `lib.rs`

#### Step 1.1.3 — Create `src/types.rs` (0.5h)
1. Move `ToolReply` struct + impl (lines 1839-1880)
2. Move `CommandOutput` struct (line 1880)
3. Move `EnvOverrideGuard` struct + impl + Drop (lines 1888-1913)
4. Change visibility to `pub(crate)` where possible
5. Re-export from `lib.rs`

#### Step 1.1.4 — Create `src/arg_helpers.rs` (0.5h)
1. Move: `command_output_data`, `required_string`, `optional_string`, `optional_bool`
2. Move: `optional_u64`, `required_u64`, `optional_i64`, `optional_string_array`
3. Move: `optional_object`, `host_toolchain_dir`, `checked_u16`, `checked_usize`
4. Keep all as `pub` since they're used by tools
5. Re-export from `lib.rs`

#### Step 1.1.5 — Create `src/persist_helpers.rs` (1h)
1. Move: `parse_session_id`, `parse_branch_id`, `parse_message_id`, `parse_swipe_group_id`
2. Move: `now_timestamp_ms`, `parse_prefixed_field`
3. Move: `default_preferences_json`, `merge_json_objects`
4. Move: `sanitize_prefix`, `probe_session_lock`
5. Move: `session_summary_json`, `branch_record_json`, `message_json`
6. Move: `pinned_memory_record_json`, `pinned_memory_view_json`
7. Move: `swipe_group_json`, `swipe_candidate_json`, `render_transcript_text`
8. Move: `normalize_preferences_json`, `normalize_preferences_json_for_key`, `normalize_preferences_key`
9. Gate behind `#[cfg(feature = "legacy-tools")]` where applicable (for persist types)

#### Step 1.1.6 — Expand `testing/screen.rs` (1h)
1. Move `PtyVteCaptureConfig` struct + impl (lines 1198-1236)
2. Move `PtyVteCaptureArtifacts` struct + impl (already in testing/screen.rs? Check)
3. Move `screenshot_capture_config` (line 1237)
4. Move `mock_user_capture_settings` (line 1272)
5. Move `mock_user_capture_config` (line 1321)
6. Move `resolve_mock_user_output_dir` (line 1346)
7. Move `screenshot_file_stem` (line 1361)

#### Step 1.1.7 — Expand `testing/journey.rs` (1h)
1. Move `capturable_screen_journey_builders()` (line 698)
2. Move `CapturableScreenJourneyDefinition` struct if present in lib.rs
3. Move `CapturableScreenDefinition` struct if present

#### Step 1.1.8 — Thin `lib.rs` (0.5h)
After all extractions, `lib.rs` should contain:
- Imports (all remaining shared types)
- `mod` declarations for all sub-modules
- `pub use` re-exports for the public API
- `env_flag_enabled()` (line 89) — small helper, can stay
- `mock_user_journey_names()` (line 122) — small, can stay
- `run_stdio_server()` (line 130) — **must** stay, it's the entry point
- `OzoneMcpServer::new()` (lines 154-167) — thin constructor, stays with struct or stays in lib.rs
- `handle_request()` (line 166) — dispatch glue, stays in lib.rs

**Verification:**
```bash
cargo build -p ozone-mcp-app
cargo clippy -p ozone-mcp --all-targets -- -D warnings
cargo test -p ozone-mcp
```

**Total: ~8h**

---

## Item 1.2 — Extract `src/ui/dispatch.rs` + `src/ui/plan_builder.rs` (1,672L → target ~700L)

**Current structure** of `src/ui/mod.rs` (1,672 lines):

| Section | Lines | What |
|---|---|---|
| Imports + module decls + re-exports | 1-293 | Module declarations, `pub use`, `use` statements |
| `App` struct + impl | 296-674 | `App::new()`, `tick()`, `set_status()`, `update_disk()`, `filtered_catalog()`, etc. |
| Free functions | 675-792 | `next_screen_after_splash()`, `queue_launch()`, `selected_record()`, `run_monitor()` |
| Tests | 793-1672 | All inline test functions (~880 lines) |

**What stays in `mod.rs` after extraction:**
1. Imports + module declarations + re-exports (~293 lines)
2. `App` struct + impl (~378 lines) — `new()`, `tick()`, `set_status()`, `set_error()`, `command_overlay_query()`, `update_disk()`, `reset_profile_flow()`, `start_profile_workflow()`, profile navigation methods, bench_eval methods, result file methods, `filtered_catalog*()`
3. Tests module (~880 lines) — stays in mod.rs because it tests `super::*` (the App)

**What moves to `src/ui/plan_builder.rs`:**
1. `next_screen_after_splash()` (line 675)
2. `queue_launch()` (line 683)
3. `LauncherActionOutcome` enum (line 688)
4. `selected_record()` (line 692)
5. *Future: `build_effective_plan()` if it exists in mod.rs (check)*

**What moves to `src/ui/monitor_loop.rs` (or `monitor_flow.rs`):**
1. `run_monitor()` (line 702) — complete event loop for the monitor screen
   - This is a top-level async function that starts its own TUI loop
   - It doesn't belong in mod.rs — it's a standalone entry point

### Step-by-step

#### Step 1.2.1 — Create `src/ui/plan_builder.rs` (1h)
```rust
//! Plan building helpers — extracted from `ui/mod.rs`.
//!
//! These helpers construct, validate, and navigate launcher plans.
//! They are the glue between the App state and the launch/configure flow.

use crate::catalog::CatalogRecord;
use crate::launch_config::LaunchPlan;
use super::App;
use super::Screen;

/// Determine the next screen after splash completes.
pub(super) fn next_screen_after_splash(app: &App) -> Screen { ... }

/// Queue a pending launch for the currently selected plan.
pub(super) fn queue_launch(app: &mut App) { ... }

/// Outcome of a launcher screen action.
pub(super) enum LauncherActionOutcome { Continue, Exit }

/// Get the currently selected catalog record from the app's current plan.
pub(super) fn selected_record(app: &App) -> Option<CatalogRecord> { ... }
```

**Verification** after extraction:
```bash
cargo build -p ozone
cargo test -p ozone
```

#### Step 1.2.2 — Move `run_monitor()` to `monitor_flow.rs` (0.5h)

`monitor_flow.rs` currently exists at 26 lines. It's a thin module. Move the full `run_monitor()` function there.

Update `lib.rs` line 424: `Some(Commands::Monitor) => ui::run_monitor().await,`
→ `Some(Commands::Monitor) => ui::monitor_flow::run_monitor().await,`

Update `ui/mod.rs`: remove `pub async fn run_monitor()`, add `pub use self::monitor_flow::run_monitor;` to maintain backward compatibility.

Wait — `run_monitor()` is called from `lib.rs` as `ui::run_monitor()`. So either:
- Keep a re-export in `ui/mod.rs`: `pub use self::monitor_flow::run_monitor;`
- Or change the call site in `lib.rs` to `ui::monitor_flow::run_monitor().await`

**Recommended**: Change call site in `lib.rs` to `ui::monitor::run_monitor().await` since `monitor.rs` is the render module. Or better: rename `monitor_flow.rs` to `monitor_loop.rs` since it contains the main loop.

Actually, the simplest approach: add `run_monitor()` to the existing `monitor_flow.rs` and update the call site in `lib.rs`. The module is already `mod monitor_flow;` in `ui/mod.rs`.

**Verification:**
```bash
cargo build -p ozone
cargo clippy -p ozone --all-targets -- -D warnings
```

**Total: ~2h**

---

## Item 1.3 — Extract Inline CLI Handlers from `src/lib.rs` (948L → target ~550L)

**Current state**: `src/lib.rs` is 948 lines. The `run()` function is ~250 lines with 6 inline dispatch arms:

| Handler | Lines in `run()` | Complexity |
|---|---|---|
| `Commands::Bench` | 427-527 (101L) | Medium: builds `BenchmarkRunRequest`, calls bench module, prints + stores result |
| `Commands::Sweep` | 527-640 (114L) | Medium: builds sweep config, calls sweep module, prints result |
| `Commands::ThreadSweep` | 640-699 (60L) | Medium: builds thread-specific config, calls sweep module |
| `Commands::EvalRun` | 704-825 (122L) | **High**: builds `EvalRunConfig` with two branches (managed vs unmanaged), env-var fallback, policy construction |
| `Commands::CreativeWrite` | 827-858 (32L) | Low: loads prompt bank, runs eval, builds report |
| `Commands::Model` | 858-865 (8L) | Trivial: delegates to `model::run()` |

**Already extracted** (in `src/commands/mod.rs`, 252L):
- `cmd_clear`, `cmd_purge_last_model`, `cmd_import_specs`, `cmd_list`, `cmd_analyze`, `cmd_eval`, `cmd_export_server`, `cmd_eval_list`, `cmd_profiles`

### Step-by-step

#### Step 1.3.1 — Extract `Commands::Bench` → `src/commands/cmd_bench.rs` (1h)

The Bench arm (lines 427-527) does:
1. Check model exists at path
2. Resolve `quant_k`/`quant_v` with `--quant-kv` fallback
3. Print CLI header with field info
4. Call `bench::run_benchmark()`
5. Print result via `bench::print_result()`
6. Store result via `bench::store_result()`
7. Optionally save profile

Create `src/commands/cmd_bench.rs`:
```rust
use anyhow::Result;

pub async fn cmd_bench(
    model: String,
    gpu_layers: i32,
    context: u32,
    quant_k: u8,
    quant_v: Option<u8>,
    quant_kv: Option<u8>,
    threads: Option<u32>,
    save_profile: Option<String>,
) -> Result<()> {
    // ... body from lib.rs lines 427-527
}
```

Update `lib.rs` dispatch arm to:
```rust
#[cfg(feature = "bench")]
Some(Commands::Bench { model, gpu_layers, context, quant_k, quant_v, quant_kv, threads, save_profile }) =>
    commands::cmd_bench(model, gpu_layers, context, quant_k, quant_v, quant_kv, threads, save_profile).await,
```

#### Step 1.3.2 — Extract `Commands::Sweep` → `src/commands/cmd_sweep.rs` (1h)

Read the Sweep arm (lines 527-640) and extract to:
```rust
pub async fn cmd_sweep(/* params */) -> Result<()> { ... }
```

#### Step 1.3.3 — Extract `Commands::ThreadSweep` → `src/commands/cmd_thread_sweep.rs` (0.5h)

Read the ThreadSweep arm (lines 640-699) and extract to:
```rust
pub async fn cmd_thread_sweep(/* params */) -> Result<()> { ... }
```

#### Step 1.3.4 — Extract `Commands::EvalRun` → `src/commands/cmd_eval_run.rs` (2h)

**This is the most complex extraction.** The EvalRun arm (lines 704-825) does:
1. Creates `resolve_cache_type` closure for env-var fallback
2. Resolves `effective_cache_k` and `effective_cache_v`
3. Builds `ContextPolicy`
4. Constructs `EvalRunConfig` in TWO nearly identical branches (managed vs unmanaged)
5. Calls `runner::run_eval()`
6. Prints result

**DRY fix opportunity**: Extract a helper `build_eval_run_config()` shared by both branches, eliminating the duplicated struct construction.

Create helper in `cmd_eval_run.rs`:
```rust
fn build_eval_run_config(
    model_path: &str,
    backend: &str,
    base_url: Option<String>,
    context_length: u32,
    skip_warmup: bool,
    skip_health_gate: bool,
    sweep_level: crate::runner::SweepLevel,
    gate_attempts: u32,
    attempts: u32,
    gpu_layers: i32,
    threads: Option<u32>,
    manage_server: bool,
    server_path: Option<PathBuf>,
    cache_type_k: u8,
    cache_type_v: u8,
    flash_attn: bool,
    no_thinking: bool,
) -> crate::runner::EvalRunConfig { ... }
```

#### Step 1.3.5 — Extract `Commands::CreativeWrite` → `src/commands/cmd_creative_write.rs` (0.5h)

Small extraction (lines 827-858). Straightforward.

#### Step 1.3.6 — Extract `Commands::Model` → `src/commands/cmd_model.rs` (0.25h)

Trivial — just delegates to `model::run()`. But it's gated behind `#[cfg(feature = "model-mgmt")]`.

#### Step 1.3.7 — Update `src/commands/mod.rs` (0.5h)

Add module declarations for all new files:
```rust
#[cfg(feature = "bench")]
mod cmd_bench;
#[cfg(feature = "sweep")]
mod cmd_sweep;
#[cfg(feature = "sweep")]
mod cmd_thread_sweep;
#[cfg(feature = "eval")]
mod cmd_eval_run;
#[cfg(feature = "eval")]
mod cmd_creative_write;
#[cfg(feature = "model-mgmt")]
mod cmd_model;

// Re-exports
#[cfg(feature = "bench")]
pub use cmd_bench::cmd_bench;
#[cfg(feature = "sweep")]
pub use cmd_sweep::cmd_sweep;
#[cfg(feature = "sweep")]
pub use cmd_thread_sweep::cmd_thread_sweep;
#[cfg(feature = "eval")]
pub use cmd_eval_run::cmd_eval_run;
#[cfg(feature = "eval")]
pub use cmd_creative_write::cmd_creative_write;
#[cfg(feature = "model-mgmt")]
pub use cmd_model::cmd_model;
```

#### Step 1.3.8 — Update `src/lib.rs` dispatch arms (0.5h)

Replace each inline arm with a single-line call to the corresponding `commands::cmd_*` function.

**After extraction**, `lib.rs` shrinks to ~550L:
- Module declarations + constants: ~170L
- `run()` function (thin dispatch): ~80L
- Tests: ~300L

### Verification for all extractions:
```bash
# Default features — everything must compile
cargo build --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

# Lite build — must still be clean
cargo build --no-default-features -p ozone

# Full build — must work
cargo build --workspace --features full
```

**Total: ~6h**

---

## Execution Order

```
Item 1.1 (MCP split, ~8h)
  ├── Step 1.1.1: server.rs
  ├── Step 1.1.2: tool_defs.rs
  ├── Step 1.1.3: types.rs
  ├── Step 1.1.4: arg_helpers.rs
  ├── Step 1.1.5: persist_helpers.rs
  ├── Step 1.1.6: expand testing/screen.rs
  ├── Step 1.1.7: expand testing/journey.rs
  └── Step 1.1.8: thin lib.rs

Item 1.3 (lib.rs handlers, ~6h) — can overlap with Item 1.2
  ├── Step 1.3.1: cmd_bench.rs
  ├── Step 1.3.2: cmd_sweep.rs
  ├── Step 1.3.3: cmd_thread_sweep.rs
  ├── Step 1.3.4: cmd_eval_run.rs ← includes DRY fix
  ├── Step 1.3.5: cmd_creative_write.rs
  ├── Step 1.3.6: cmd_model.rs
  ├── Step 1.3.7: update commands/mod.rs
  └── Step 1.3.8: update lib.rs dispatch

Item 1.2 (ui/mod.rs split, ~2h) — can overlap with 1.3
  ├── Step 1.2.1: plan_builder.rs
  └── Step 1.2.2: move run_monitor to monitor_flow.rs
```

**Parallelism**: Items 1.2 and 1.3 don't share files with each other or
with the MCP crate. They can be done in parallel. Total wall-clock time
with parallelism: ~16h (vs ~25h sequential).

---

## Expected Outcomes

| Metric | Before | After | Δ |
|---|---|---|---|
| `crates/ozone-mcp/src/lib.rs` | 2,287L | ~400L | -1,887L |
| `src/lib.rs` | 948L | ~550L | -398L |
| `src/ui/mod.rs` | 1,672L | ~700L | -972L |
| Files in `ozone` crate `src/` | 74 | ~82 | +8 |
| Files in `ozone-mcp` crate `src/` | 8 | ~14 | +6 |
| Largest file in workspace | 1,672L (ui/mod.rs) | <800L | -50% |
| Inline dispatch arms in `lib.rs::run()` | 6 | 0 | -100% |
| `lib.rs` `run()` function length | ~250L | ~80L | -68% |

# Ozone Bug Log

Running log of all discovered bugs, silent failures, structural issues, and UX gaps.
Each entry stays until fixed, then moves to `HISTORY` with date and commit.

**Fix plan:** `docs/bug-reports/TDD-FIX-PLAN.md` — phased TDD approach, easiest first.
**Phase 1+2 status: ✅ COMPLETE (2026-06-08)** — 7 fixes (eval names, threads, dead code, error swallowing, timeout, dead flag).
**Phase 3 status: ✅ COMPLETE (2026-06-08)** — KV cache quantization across 5 bugs.
**Phase 4 status: ✅ COMPLETE (2026-06-08)** — TUI integration (wrong-model check, creative writing/export wiring, model picker).
**Phase 5 status: ✅ COMPLETE (2026-06-08)** — eval architecture unification. All 7 remaining bugs resolved.

**Convention:** When fixing an entry, move it to HISTORY — don't delete it.
Add `(**FIXED** yyyy-mm-dd <commit-hash>)` and append to the HISTORY section at bottom.

--------------------------------------------------------------------

## ACTIVE

### BUG-011 — llama-server auto n_parallel=4 exhausts VRAM on mid-size GPUs (**FIXED** 2026-06-10)
Ozone never passes `--n-parallel` to llama-server, so the server auto-detects `n_parallel = 4`.
On 12GB GPUs with 12B models at 24K+ context, the 4x KV cache allocation exceeds VRAM,
causing `vk::DeviceLostError` (Vulkan backend) or CUDA OOM. Observed with
TheDrummer/Rocinante-12B Q4_K_M at 24K context with q8_0 KV cache.

**Fix:** Default `--n-parallel 1` in `build_llama_args()` and `build_llamacpp_bench_args()`.
The `LaunchPlan` now carries `n_parallel: u32` (default 1). Sweep/bench commands also force 1.

### BUG-012 — Log file truncated on restart loses crash evidence (**FIXED** 2026-06-10)
`start_llamacpp()` opened the log with `.truncate(true)`, so every restart overwrote the
previous session's log. Crashes mid-session (e.g., VRAM exhaustion at high context) left no trace.

**Fix:** Timestamped log filenames (`llamacpp-YYYYMMDDTHHMMSS.log`) with a symlink
`llamacpp.log → latest` for convenience. Previous logs preserved across restarts.

### STRUCT-006 — K and V cache quantization must be independently controllable (**FIXED** 2026-06-10)
Previously `quant_kv: u8` forced K and V to use the same quantization. But K-cache is
more important for attention quality, while V-cache can be more aggressive (e.g., K=q8_0, V=q4_0)
saving VRAM without noticeable quality loss.

**Fix:** Split `quant_kv` → `quant_k` + `quant_v` across the entire codebase:
- `LaunchPlan`, `Recommendation`, `BenchmarkRun`, `ModelLaunchOverride`, `SavedLaunchProfile`
- `kv_cache_args(quant_k, quant_v)` generates independent `--cache-type-k`/`--cache-type-v`
- CLI: `--quant-k`, `--quant-v`, `--quant-kv` (shorthand sets both)
- DB: `quant_v` column added, `COALESCE(quant_v, quant_k)` for legacy rows
- All planner estimators use `asymmetric_kv_factor()` averaging K and V factors
- 159 tests pass
--------------------------------------------------------------------

## HISTORY

### BUG-001 — TruthfulQA lm-eval task name mismatch (**FIXED** 2026-06-08)
Changed `"truthfulqa"` → `"truthfulqa_gen"` in `src/eval.rs` dispatch.
Added regression tests: `eval_registry_truthfulqa_has_correct_lm_eval_task_name`,
`eval_dispatch_task_names_match_registry`.

--------------------------------------------------------------------

### BUG-002 — BBH lm-eval task name mismatch (**FIXED** 2026-06-08)
Changed `"bbh"` → `"bigbench_hard"` in `src/eval.rs` dispatch.
Added regression test: `eval_registry_bbh_has_correct_lm_eval_task_name`.

--------------------------------------------------------------------

### BUG-009 — Export server hardcodes 8 threads, ignores `plan.threads` (**FIXED** 2026-06-08)
Changed `threads = 8` → `plan.threads.unwrap_or(8)` in `src/export_server.rs`.
Added tests: `generate_serve_script_uses_plan_threads`,
`generate_serve_script_defaults_to_8_threads_when_none`.

--------------------------------------------------------------------

### BUG-008 — `is_url_ready` fallback client has no timeout (**FIXED** 2026-06-08)
Replaced fallback `Client::new()` with a builder that also sets a timeout (5s).
Added test: `is_url_ready_returns_false_for_unreachable_port`.

--------------------------------------------------------------------

### STRUCT-005 — `read_context_length` has a dead fallback path (**FIXED** 2026-06-08)
Removed the dead `.or_else()` chain that mapped `llama.embedding_length` to `0`
only for the `.filter(|v| *v > 0)` to immediately discard it.
Added regression test: `read_context_length_returns_none_when_key_missing`.

--------------------------------------------------------------------

### SILENT-001 — `.ok()` swallows report builder errors (**FIXED** 2026-06-08)
Replaced `.ok()` with `build_report_or_warn()` helper that sends stderr events
on report build failure instead of silently dropping the error.
Added tests: `build_report_or_warn_sends_error_event_on_failure`,
`build_report_or_warn_returns_report_on_success`.

--------------------------------------------------------------------

### SILENT-003 — `--profile` flag for ExportServer is unimplemented (**FIXED** 2026-06-08)
Removed dead `--profile` flag from ExportServer CLI. Simplified handler to always
use catalog recommendation directly.

--------------------------------------------------------------------

### BUG-003 — `quant_kv` explicitly ignored in benchmarks (**FIXED** 2026-06-08)
Added `kv_cache_args()` helper to `processes.rs`. Added `quant_kv` param to
`build_llamacpp_bench_args()`. Removed `let _ = quant_kv;`. Now generates
`--cache-type-k`/`--cache-type-v` flags for q8_0 (quant_kv=2) and q4_0 (quant_kv=3).

--------------------------------------------------------------------

### BUG-004 — Launcher TUI ignores `quant_kv` at runtime (**FIXED** 2026-06-08)
`build_llama_args()` in `src/ui/backend_args.rs` now appends `kv_cache_args(plan.quant_kv)`.
Existing test updated to expect KV cache flags when quant_kv=2.

--------------------------------------------------------------------

### BUG-005 — `run_context_sweep` hardcodes `quant_kv=1` (**FIXED** 2026-06-08)
Added `quant_kv: u8` parameter to `run_context_sweep()`. Passed through from CLI
via `--quant-kv` flag (default 1). No longer hardcodes 1.

--------------------------------------------------------------------

### BUG-006 — Full sweep claims to test multiple quant_kv levels but doesn't (**FIXED** 2026-06-08)
Auto-fixed by BUG-003. `run_sweep()` already passed `qkv` through to
`run_benchmark_with_progress` → `build_llamacpp_bench_args(gpu_layers, context_size, quant_kv, threads)`
→ now uses `kv_cache_args(quant_kv)`. Chain is complete end-to-end.

--------------------------------------------------------------------

### BUG-010 — Export server missing `--cache-type-k`/`--cache-type-v` (**FIXED** 2026-06-08)
Added `{cache_flags}` to the generated bash script template in `src/export_server.rs`.
Flags computed via `kv_cache_args(plan.quant_kv)`. Only included when quant_kv > 1.
Added 2 regression tests.

--------------------------------------------------------------------

### SILENT-005 — `start_llamacpp` returns Ok for wrong model (**FIXED** 2026-06-08)
Added `model_name_matches_running()` helper. `start_llamacpp` now verifies the running
model matches the requested one before returning Ok. If mismatched, kills the old
server and proceeds with launch. Added 4 unit tests.

--------------------------------------------------------------------

### SILENT-002 — Creative Writing and Export Server are TUI dummies (**FIXED** 2026-06-08)
Wired `EvalCreativeWriting` to spawn `run_creative_writing_eval()` via tokio::spawn.
Wired `ExportServer` to generate serve script to `~/models/serve-<model>.sh`.
Both now report progress via eprintln! instead of silent CLI hints.

--------------------------------------------------------------------

### UX-001 — No model picker inside Bench+Eval screen (**FIXED** 2026-06-08)
Added `ModelPickerMode::BenchEval` variant. `[m]` key binding in Bench+Eval opens
model picker; selecting a model returns directly to Bench+Eval. Updated hint bar.

--------------------------------------------------------------------

### BUG-007 — Two parallel diverging eval code paths (**FIXED** 2026-06-08)
`run_eval()` now delegates to `run_eval_task()` via EVAL_TASKS registry lookup.
8-arm enum dispatch replaced with 3-line delegation. Registry is the single
source of truth. Removed duplicate code paths.

--------------------------------------------------------------------

### SILENT-004 — Creative writing eval in registry path bails out (**FIXED** 2026-06-08)
`run_eval_task()` CreativeWriting arm now calls `run_creative_writing_eval()`.
Registry path no longer bails out — fully functional.

--------------------------------------------------------------------

### STRUCT-001 — 6 dead-code items in eval chain (**FIXED** 2026-06-08)
Removed 4 `#[allow(dead_code)]` annotations from eval.rs. Deleted `start_eval()`
and `run_bench_eval_workflow()` (had no callers). 200+ lines of maintained but
dead code now live or removed.

--------------------------------------------------------------------

### STRUCT-002 — Output directory strings hardcoded in 3 places (**FIXED** 2026-06-08)
`build_eval_report_for_preset()` now derives output directories from EVAL_TASKS
registry metadata instead of hardcoded per-preset strings.

--------------------------------------------------------------------

### STRUCT-003 — EvalPreset enum should be generated from task registry (**FIXED** 2026-06-08)
`EvalPreset::report_label()` now delegates to EVAL_TASKS lookup. TUI entries
generated from EVAL_TASKS iteration. `description()` removed (was dead code).
`EvalPreset` kept as clap::ValueEnum for CLI backward compatibility.

--------------------------------------------------------------------

### STRUCT-004 — Subprocess approach adds fragility (**FIXED** 2026-06-08)
Removed dead `run_bench_eval_workflow()` (subprocess-spawning version).
Only `run_bench_eval_workflow_with_cli_name()` remains — calls `run_eval()`
directly instead of spawning `current_exe` as subprocess.

--------------------------------------------------------------------

### UX-002 — tokenizer_backend=None may degrade prompts (**FIXED** 2026-06-08)
Added documentation comment above `tokenizer_backend=None` identifying
potential degradation risk and recommended fix path (huggingface tokenizer).
No behavior change — marked as known limitation.

--------------------------------------------------------------------

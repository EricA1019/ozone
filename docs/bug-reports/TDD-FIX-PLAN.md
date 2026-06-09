# TDD Bug-Fix Plan

22 issues catalogued in `BUGLOG.md`, ordered by ascending complexity.
Each phase produces green tests before moving on.

--------------------------------------------------------------------

## Phase 1: Quick Wins (trivial, single-line)

### BUG-002 — BBH task name
**Fix:** `src/eval.rs` line ~449: `"bbh"` → `"bigbench_hard"`
**Test:** `oz eval --preset bbh --limit 1 --base-url http://127.0.0.1:8989 <model>`
(lm-eval should run BIG-Bench Hard, not fail with "unknown task bbh")

### BUG-001 — TruthfulQA task name
**Fix:** `src/eval.rs` line ~440: `"truthfulqa"` → `"truthfulqa_gen"`
**Test:** Same pattern — `oz eval --preset truthfulqa --limit 1` should run truthfulness probe

### BUG-009 — Export server hardcoded threads
**Fix:** `src/export_server.rs` line ~85: `threads = plan.threads.unwrap_or(8)`
**Test:** Generate script with plan having `threads: Some(4)`, verify script contains `--threads 4`

### STRUCT-005 — Dead fallback in read_context_length
**Fix:** `src/gguf.rs` lines 247-251: remove the `llama.embedding_length` fallback
**Test:** Existing GGUF tests still pass

**Phase 1 gate:** `cargo test -p ozone --quiet` green, `cargo clippy` clean

--------------------------------------------------------------------

## Phase 2: Simple Fixes

### SILENT-001 — .ok() swallows report errors
**Fix:** `src/ui/bench_eval_workflow.rs` lines 149, 234:
```rust
let report = match crate::eval_report::build_eval_report_for_preset(...) {
    Ok(r) => Some(r),
    Err(e) => {
        let _ = tx.send(BenchEvalWorkflowEvent::Output {
            is_stderr: true,
            line: format!("Report generation failed: {e}"),
        });
        None
    }
};
```
**Test:** Run eval with no lm-eval venv → should see stderr line, not silent success

### BUG-008 — is_url_ready fallback has no timeout
**Fix:** `src/processes.rs` lines 55-65:
```rust
let client = reqwest::Client::builder()
    .timeout(Duration::from_secs(2))
    .build()
    .unwrap_or_else(|_| {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new())
    });
```
**Test:** Unit test `is_url_ready_has_timeout` — verify client has timeout configured

### SILENT-003 — --profile flag unimplemented
**Fix:** `src/main.rs` line 496: implement profile lookup via `crate::prefs::load_prefs()`
or remove the `--profile` flag from the CLI struct and update help text.
**Decision needed:** Implement or remove?
**If implementing:** Load saved profile by name, apply to plan.
**If removing:** Remove `#[arg(long)] profile` from ExportServer struct.

**Phase 2 gate:** `cargo test -p ozone --quiet` green, `cargo clippy` clean

--------------------------------------------------------------------

## Phase 3: KV Cache Quantization (shared root cause)

This is one fix applied to 4 locations. The quant_kv value (1=f16, 2=q8_0, 3=q4_0)
must be translated to `--cache-type-k`/`--cache-type-v` flags for llama-server.

### Shared helper (new)
Add to `src/processes.rs` or `src/bench.rs`:
```rust
fn kv_cache_args(quant_kv: u8) -> Vec<String> {
    let quant = match quant_kv {
        2 => "q8_0",
        3 => "q4_0",
        _ => return vec![], // default f16, no args needed
    };
    vec![
        "--cache-type-k".into(), quant.into(),
        "--cache-type-v".into(), quant.into(),
    ]
}
```
**Test:** `kv_cache_args(1)` → empty vec, `kv_cache_args(2)` → `["--cache-type-k", "q8_0", ...]`

### BUG-003 — Bench ignores quant_kv
**Fix:** `src/bench.rs`: add `quant_kv` param to `build_llamacpp_bench_args()`, call `kv_cache_args(quant_kv)`, remove `let _ = quant_kv;`
**Test:** Run benchmark with quant_kv=2, verify `--cache-type-k q8_0` in log

### BUG-004 — Launcher ignores quant_kv
**Fix:** `src/ui/backend_args.rs` `build_llama_args()`: append `kv_cache_args(plan.quant_kv)`
**Test:** Launch model with quant_kv=2 saved, verify process args include `--cache-type-k q8_0`

### BUG-005 — Context sweep hardcodes quant_kv=1
**Fix:** `src/sweep.rs`: add `quant_kv` param to `run_context_sweep()`, pass through to `run_benchmark`, expose as `--quant-kv` CLI flag
**Test:** `oz sweep --context-sweep --quant-kv 2` passes 2 to benchmark

### BUG-006 — Full sweep results are duplicate
**Fixed automatically by BUG-003** — once bench respects quant_kv, sweep results diverge

### BUG-010 — Export server missing KV cache args
**Fix:** `src/export_server.rs`: add `--cache-type-k`/`-v` to generated script template
**Test:** Generated script includes correct cache flags for quant_kv > 1

**Phase 3 gate:** `cargo test -p ozone --quiet` green, `cargo clippy` clean

--------------------------------------------------------------------

## Phase 4: TUI Integration

### SILENT-002 — Creative Writing / Export Server are TUI dummies
**Fix:** `src/ui/bench_eval_flow.rs`:
- `EvalCreativeWriting`: spawn `creative_writing::run_creative_writing_eval()` in tokio task
- `ExportServer`: open a file-save dialog or generate to default location and show path
**Test:** Press Enter on Creative Writing → see progress, not just "Use CLI..."

### SILENT-005 — start_llamacpp returns Ok for wrong model
**Fix:** `src/processes.rs` lines 337-339: before returning Ok, verify the running model
matches the requested model via `get_llamacpp_model()`
```rust
if is_url_ready(&paths::llamacpp_ready_url()).await {
    // Verify it's the right model
    if let Some(running) = get_llamacpp_model().await {
        if running == model_name || running.contains(model_name) {
            return Ok(());
        }
    }
    // Wrong model or couldn't verify — kill and restart
    clear_gpu_backends().await?;
}
```
**Test:** Launch model A, try to launch model B without killing A → should kill A first

### UX-001 — No model picker inside Bench+Eval
**Fix:** Add a model selector to the Bench+Eval screen, or add a shortcut key
that opens the model picker in Launch mode
**Test:** From Bench+Eval, press a key → model picker opens, select model → returns to Bench+Eval

**Phase 4 gate:** `cargo test -p ozone --quiet` green, `cargo clippy` clean

--------------------------------------------------------------------

## Phase 5: Architecture Cleanup (complex)

### BUG-007 + STRUCT-001 + STRUCT-002 + STRUCT-003 + SILENT-004
**Unified fix:** Make `EVAL_TASKS` + `run_eval_task()` the single code path.
1. Remove `EvalPreset` enum → CLI `--preset` accepts string, validated against `EVAL_TASKS`
2. `run_eval()` becomes a thin wrapper around `run_eval_task()`
3. `build_eval_report_for_preset()` uses task registry metadata, not hardcoded strings
4. TUI entries generated from `EVAL_TASKS` iteration, not per-task enum variants
5. Remove all 6 dead-code items
6. Creative Writing `run_eval_task` dispatch calls `creative_writing::run_creative_writing_eval()`

**Test strategy:**
- `run_eval_task` unit tests for each task kind (mock lm-eval venv)
- Registry-driven TUI entry generation test
- Report builder: test all 8 presets produce correct paths

### STRUCT-004 — Subprocess approach
**Fix:** `run_bench_eval_workflow` calls eval functions directly instead of spawning
`current_exe` as a subprocess. The workflow becomes:
```
tokio::spawn(async {
    eval::run_eval(...)  // direct call, not subprocess
    // then build report, send events
})
```
**This is optional** — the subprocess approach works, it's just fragile.
Consider deferring if Phase 5 scope is too large.

### UX-002 — tokenizer_backend=None
**Investigation needed:** Test MMLU and TruthfulQA results with current settings.
If degraded, switch to `tokenizer_backend=huggingface` with a local tokenizer.
May require adding a tokenizer dependency or documenting the limitation.

**Phase 5 gate:** Full `cargo test --workspace --quiet` green, `cargo clippy --workspace` clean,
manual smoke: run each of the 8 presets via CLI, verify results + report generation.

--------------------------------------------------------------------

## Dependency Graph

```
Phase 1 (independent) ─────────────────────────────────────────────┐
Phase 2 (independent, depends on Phase 1 only for git history) ─────┤
Phase 3 (depends on Phase 1+2 for clean base) ──────────────────────┤
Phase 4 (depends on Phase 3, uses kv_cache_args helper) ────────────┤
Phase 5 (depends on Phase 4, rewrites eval architecture) ───────────┘
```

Phases 1-2 can be done in any order within each phase.
Phase 3 must be done together (shared kv_cache_args helper).
Phase 5 is the largest single change but resolves 6 issues at once.

--------------------------------------------------------------------

## Per-Phase Verification

| Phase | Clippy | Tests | Manual Smoke |
|-------|--------|-------|-------------|
| 1 | ✅ | 100 pass | `oz eval --preset bbh --limit 1` |
| 2 | ✅ | 100 pass | Eval subprocess fails → error visible in TUI |
| 3 | ✅ | 100 pass | `oz sweep --context-sweep --quant-kv 2` shows q8_0 in log |
| 4 | ✅ | 100 pass | Creative Writing runs from TUI, model switch kills old |
| 5 | ✅ | 100 pass | All 8 presets run, reports generated, dead code removed |

# BUG-B6: Sweep tests multiple quant_kv levels but produces identical results

- **Severity:** 🔴 Bug
- **File(s):** `src/sweep.rs:170`, `src/bench.rs:159`
- **Found:** 2026-06-08
- **Status:** Open (depends on B3)

## What's Wrong
`run_sweep()` iterates over `config.quant_kv_levels` (e.g. `[1, 2]`) and passes each to `run_benchmark_with_progress()`. The progress messages show `qkv=1`, `qkv=2`, but the benchmark ignores the value (B3). Both levels produce identical results because the server always runs at f16.

## Impact
Sweep Pareto frontier computed on duplicate data. User thinks they've tested q8_0 cache quantization but they haven't.

## Suggested Fix
Fix B3 first, then verify sweep produces different VRAM and speed numbers for different quant_kv levels.

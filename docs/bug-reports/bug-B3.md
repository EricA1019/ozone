# BUG-B3: `quant_kv` explicitly ignored in benchmarks

- **Severity:** 🔴 Bug
- **File(s):** `src/bench.rs:159`
- **Found:** 2026-06-08
- **Status:** Open

## What's Wrong
`run_benchmark_with_progress()` accepts `quant_kv: u8` but immediately discards it with `let _ = quant_kv;`. The helper `build_llamacpp_bench_args()` doesn't accept `quant_kv` and never generates `--cache-type-k` or `--cache-type-v` flags for llama-server.

## Evidence
```rust
// src/bench.rs:159
let _ = quant_kv;

// src/bench.rs:68-80 — no quant_kv parameter
fn build_llamacpp_bench_args(
    gpu_layers: i32,
    context_size: u32,
    threads: Option<u32>,
) -> Vec<String> {
    // ... no --cache-type-k or --cache-type-v
}
```

llama-server supports `-ctk`/`-ctv` with values like `f16`, `q8_0`, `q4_0`. These are never generated.

## Impact
Every benchmark and sweep runs with default f16 KV cache. The sweep tests `quant_kv_levels: [1, 2]` but both produce identical results. Users get incorrect VRAM measurements and miss opportunities to find larger viable contexts with q8_0.

## Suggested Fix
Map `quant_kv` (1→f16, 2→q8_0, 3→q4_0) and add `--cache-type-k`/`--cache-type-v` to `build_llamacpp_bench_args()`.

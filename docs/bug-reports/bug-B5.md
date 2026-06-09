# BUG-B5: `run_context_sweep()` hardcodes `quant_kv=1`

- **Severity:** 🔴 Bug
- **File(s):** `src/sweep.rs:428`
- **Found:** 2026-06-08
- **Status:** Open

## What's Wrong
`run_context_sweep()` calls `bench::run_benchmark()` with a hardcoded `1` for `quant_kv`, and doesn't accept a `quant_kv` parameter.

## Evidence
```rust
let result = bench::run_benchmark(
    model_name, model_path,
    &crate::bench::BenchBackend::LlamaCpp {..},
    gpu_layers, ctx, 1, threads,  // ← hardcoded 1
).await;
```

Function signature has no `quant_kv`: `pub async fn run_context_sweep(model_name, model_path, server_path, gpu_layers, threads, quick)`.

## Impact
Context sweep always measures f16 KV cache. Can't test whether q8_0 cache allows larger contexts within the same VRAM budget.

## Suggested Fix
Add `quant_kv: u8` parameter to `run_context_sweep()`, expose via CLI `--quant-kv` flag, and pass through to `run_benchmark()`.

# BUG-B4: Launcher `build_llama_args()` never passes `--cache-type-k`/`-ctv`

- **Severity:** 🔴 Bug
- **File(s):** `src/ui/backend_args.rs:18-42`
- **Found:** 2026-06-08
- **Status:** Open

## What's Wrong
`build_llama_args()` constructs the llama-server argument list from `LaunchPlan` but doesn't include `--cache-type-k TYPE` or `--cache-type-v TYPE`. `plan.quant_kv` exists and is saved to preferences, but is never translated to runtime server flags.

## Evidence
```rust
// src/ui/backend_args.rs — no cache-type flags
pub(super) fn build_llama_args(plan: &LaunchPlan) -> Vec<String> {
    vec![
        "--host", "127.0.0.1",
        "--port", "8989",
        "--ctx-size", plan.context_size.to_string(),
        "--gpu-layers", gpu_layers,
        "--no-webui",
        // threads optional...
        // MISSING: --cache-type-k, --cache-type-v
    ]
}
```

Zero occurrences of `--cache-type` anywhere in the codebase (grep confirmed).

## Impact
Every launch from the TUI runs with f16 KV cache, using 2-3× more VRAM than necessary. The `quant_kv` value in preferences is an illusion — it affects planner estimates but not runtime behavior.

## Suggested Fix
Map `plan.quant_kv` (1→f16, 2→q8_0, 3→q4_0) and append `--cache-type-k q8_0`/`--cache-type-v q8_0` to args.

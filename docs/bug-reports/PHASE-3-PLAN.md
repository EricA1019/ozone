# Phase 3 — KV Cache Quantization: Detailed Implementation Plan

Five bugs share one root cause: `quant_kv` values (1=f16, 2=q8_0, 3=q4_0) are never
translated to llama-server's `--cache-type-k`/`--cache-type-v` flags. Every benchmark,
launch, sweep, and export runs with default f16 KV cache, wasting 2-3x VRAM.

This plan contains exact diffs, line-accurate, for a cheaper model to implement
without drift. Write each test first, then apply the production change.

--------------------------------------------------------------------

## Step 0: Add the `kv_cache_args` helper

**File:** `src/bench.rs` — add after line 86 (after the `build_llamacpp_bench_args` function),
before `run_benchmark`.

```rust
/// Map quant_kv to llama-server --cache-type-k / --cache-type-v flags.
/// 1 = f16 (default, no flags needed)
/// 2 = q8_0
/// 3 = q4_0
fn kv_cache_args(quant_kv: u8) -> Vec<String> {
    let quant = match quant_kv {
        2 => "q8_0",
        3 => "q4_0",
        _ => return vec![],
    };
    vec![
        "--cache-type-k".into(),
        quant.into(),
        "--cache-type-v".into(),
        quant.into(),
    ]
}

#[cfg(test)]
mod kv_cache_tests {
    use super::kv_cache_args;

    #[test]
    fn kv_cache_args_default_to_empty_for_f16() {
        assert!(kv_cache_args(1).is_empty(), "quant_kv=1 (f16) needs no flags");
        assert!(kv_cache_args(0).is_empty(), "quant_kv=0 should default to no flags");
        assert!(kv_cache_args(99).is_empty(), "unknown quant_kv should default to no flags");
    }

    #[test]
    fn kv_cache_args_maps_to_q8_0_for_quant_2() {
        let args = kv_cache_args(2);
        assert_eq!(args, vec!["--cache-type-k", "q8_0", "--cache-type-v", "q8_0"]);
    }

    #[test]
    fn kv_cache_args_maps_to_q4_0_for_quant_3() {
        let args = kv_cache_args(3);
        assert_eq!(args, vec!["--cache-type-k", "q4_0", "--cache-type-v", "q4_0"]);
    }
}
```

**Verify:** `cargo test -p ozone -- kv_cache --quiet` — 3 tests pass.

---

## Step 1: BUG-003 — Bench ignores `quant_kv`

**File:** `src/bench.rs`

### 1A: Add `quant_kv` to `build_llamacpp_bench_args`

**Line 73-86.** The function signature currently:
```rust
fn build_llamacpp_bench_args(
    gpu_layers: i32,
    context_size: u32,
    threads: Option<u32>,
) -> Vec<String> {
```
CHANGE TO:
```rust
fn build_llamacpp_bench_args(
    gpu_layers: i32,
    context_size: u32,
    quant_kv: u8,
    threads: Option<u32>,
) -> Vec<String> {
```

**Then** at the end of the function body, after `threads.unwrap_or(8).to_string()` (line 85), add:
```rust
    args.extend(kv_cache_args(quant_kv));
    args
```

The function body should look like (replace lines 78-86):
```rust
    let mut args = vec![
        "--host".into(),
        BENCH_LLAMACPP_HOST.into(),
        "--port".into(),
        BENCH_LLAMACPP_PORT.into(),
        "--n-gpu-layers".into(),
        gpu_layers.to_string(),
        "--ctx-size".into(),
        context_size.to_string(),
        "--threads".into(),
        threads.unwrap_or(8).to_string(),
    ];
    args.extend(kv_cache_args(quant_kv));
    args
}
```

### 1B: Pass `quant_kv` to `build_llamacpp_bench_args`

**Line 137.** Change:
```rust
            let args = build_llamacpp_bench_args(gpu_layers, context_size, threads);
            let _ = quant_kv;
```
TO:
```rust
            let args = build_llamacpp_bench_args(gpu_layers, context_size, quant_kv, threads);
```

**Verify:** `cargo check -p ozone --features bench` compiles. The `let _ = quant_kv;` line is gone — search the file to confirm no remaining dead `let _ = quant_kv;`.

---

## Step 2: BUG-004 — Launcher TUI ignores `quant_kv`

**File:** `src/ui/backend_args.rs`

### 2A: Import the helper

**After** `use crate::planner::LaunchPlan;` (line 1), add:
```rust
use crate::bench::kv_cache_args;
```

### 2B: Append KV cache args to `build_llama_args`

**At the end of `build_llama_args()`**, after the `if let Some(t) = plan.threads` block (around line 46), before the closing `args` return, append:
```rust
    args.extend(kv_cache_args(plan.quant_kv));
    args
}
```

The full function should end with (replace lines 42-48):
```rust
    if let Some(t) = plan.threads {
        args.push("--threads".to_string());
        args.push(t.to_string());
    }
    args.extend(kv_cache_args(plan.quant_kv));
    args
}
```

### 2C: Make `kv_cache_args` public

**File:** `src/bench.rs` line ~87. Change:
```rust
fn kv_cache_args(quant_kv: u8) -> Vec<String> {
```
TO:
```rust
pub fn kv_cache_args(quant_kv: u8) -> Vec<String> {
```

### 2D: Add test for launcher path

**File:** `src/ui/backend_args.rs`, in the existing `#[cfg(test)] mod tests` block. Add after the existing tests:

```rust
    #[test]
    fn build_llama_args_includes_kv_cache_flags_for_q8_0() {
        let plan = test_launch_plan_with_quant_kv(2);
        let args = build_llama_args(&plan);
        // Should contain --cache-type-k q8_0 --cache-type-v q8_0
        let all_args = args.join(" ");
        assert!(all_args.contains("--cache-type-k q8_0"), "missing KV cache flags: {all_args}");
        assert!(all_args.contains("--cache-type-v q8_0"), "missing KV cache flags: {all_args}");
    }

    #[test]
    fn build_llama_args_omits_kv_cache_flags_for_f16() {
        let plan = test_launch_plan_with_quant_kv(1);
        let args = build_llama_args(&plan);
        let all_args = args.join(" ");
        assert!(!all_args.contains("--cache-type"), "f16 should not have cache-type flags: {all_args}");
    }

    fn test_launch_plan_with_quant_kv(quant_kv: u8) -> LaunchPlan {
        use crate::planner::RecommendationMode;
        LaunchPlan {
            model_name: "test.gguf".into(),
            context_size: 4096,
            gpu_layers: 32,
            total_layers: 40,
            cpu_layers: 8,
            quant_kv,
            threads: Some(4),
            blas_threads: None,
            mode: RecommendationMode::MixedMemory,
            rationale: "test".into(),
            estimated: false,
            estimated_vram_mb: 4096,
            estimated_ram_mb: 2048,
            source: "test".into(),
            layer_source_label: "test".into(),
            layer_source_note: None,
        }
    }
```

**Verify:** `cargo test -p ozone -- build_llama_args --quiet` — new tests pass.

---

## Step 3: BUG-005 — Context sweep hardcodes `quant_kv=1`

**File:** `src/sweep.rs`

### 3A: Add `quant_kv` parameter to `run_context_sweep`

**Line 387.** Change the function signature from:
```rust
pub async fn run_context_sweep(
    model_name: &str,
    model_path: &Path,
    server_path: &Path,
    gpu_layers: i32,
    threads: Option<u32>,
    quick: bool,
) -> Result<(PathBuf, u32)> {
```
TO:
```rust
pub async fn run_context_sweep(
    model_name: &str,
    model_path: &Path,
    server_path: &Path,
    gpu_layers: i32,
    quant_kv: u8,
    threads: Option<u32>,
    quick: bool,
) -> Result<(PathBuf, u32)> {
```

### 3B: Pass `quant_kv` instead of hardcoded `1`

**Line 429.** Change:
```rust
            gpu_layers, ctx, 1, threads,
```
TO:
```rust
            gpu_layers, ctx, quant_kv, threads,
```

### 3C: Add `--quant-kv` flag to Sweep CLI

**File:** `src/main.rs` line ~138 (in the `Sweep` struct). Add after `context_sweep: bool,`:
```rust
        #[arg(long, default_value = "1", help = "KV cache quantization: 1=f16, 2=q8_0, 3=q4_0")]
        quant_kv: u8,
```

### 3D: Pass `quant_kv` from CLI to `run_context_sweep`

**File:** `src/main.rs` line ~373. The Sweep match arm destructuring needs to include `quant_kv`:

Change:
```rust
            context_sweep,
```
TO:
```rust
            context_sweep,
            quant_kv,
```

Then change the `run_context_sweep` call at line ~380:
```rust
                let (csv_path, sweet_spot) = sweep::run_context_sweep(
                    &model, &model_path, &server_path, -1, None, quick,
                ).await?;
```
TO:
```rust
                let (csv_path, sweet_spot) = sweep::run_context_sweep(
                    &model, &model_path, &server_path, -1, quant_kv, None, quick,
                ).await?;
```

**Verify:** `cargo check -p ozone --features sweep` compiles. `oz sweep --help` shows `--quant-kv`.

---

## Step 4: BUG-010 — Export server missing KV cache args

**File:** `src/export_server.rs`

### 4A: Import the helper

Add after `use crate::planner::LaunchPlan;` (line 5):
```rust
use crate::bench::kv_cache_args;
```

### 4B: Add cache args to the script template

In the `generate_serve_script` function, after computing `threads`, compute the cache flags.
Add after line 84 (`threads = plan.threads.unwrap_or(8),`):

```rust
        cache_flags = kv_cache_args(plan.quant_kv)
            .iter()
            .map(|s| format!("  \\\n    {s}"))
            .collect::<Vec<_>>()
            .join(""),
```

### 4C: Insert `{cache_flags}` into the script template

In the script template (the `format!(r#"..."#)` block), add `{cache_flags}` after
the `--threads {threads}` line. Change lines 74-84 from:
```rust
{server_path} \
  --model "$MODEL" \
  --host 127.0.0.1 \
  --port "$PORT" \
  --n-gpu-layers {gpu} \
  --ctx-size {context} \
  --threads {threads} &
```
TO:
```rust
{server_path} \
  --model "$MODEL" \
  --host 127.0.0.1 \
  --port "$PORT" \
  --n-gpu-layers {gpu} \
  --ctx-size {context} \
  --threads {threads}{cache_flags} &
```

### 4D: Add test

**File:** `src/export_server.rs`, in the existing `#[cfg(test)] mod tests` block. Add:

```rust
    #[test]
    fn generate_serve_script_includes_kv_cache_flags_for_q8_0() {
        let dir = std::env::temp_dir().join("ozone-export-kv-test");
        let _ = std::fs::create_dir_all(&dir);
        let model = dir.join("test.gguf");
        std::fs::write(&model, "dummy").ok();
        let mut plan = test_plan(Some(4));
        plan.quant_kv = 2;

        let script = generate_serve_script(
            &plan,
            &model,
            Path::new("/usr/bin/llama-server"),
            8989,
            &dir.join("serve-kv-test.sh"),
        ).expect("generate_serve_script should succeed");

        let text = std::fs::read_to_string(&script).expect("read script");
        assert!(text.contains("--cache-type-k q8_0"),
            "q8_0 should add cache-type flags:\n{text}");
        assert!(text.contains("--cache-type-v q8_0"),
            "q8_0 should add cache-type flags:\n{text}");

        let _ = std::fs::remove_file(&script);
        let _ = std::fs::remove_file(&model);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn generate_serve_script_omits_kv_cache_flags_for_f16() {
        let dir = std::env::temp_dir().join("ozone-export-f16-test");
        let _ = std::fs::create_dir_all(&dir);
        let model = dir.join("test.gguf");
        std::fs::write(&model, "dummy").ok();
        let plan = test_plan(Some(4)); // quant_kv defaults to 1 (f16)

        let script = generate_serve_script(
            &plan,
            &model,
            Path::new("/usr/bin/llama-server"),
            8989,
            &dir.join("serve-f16-test.sh"),
        ).expect("generate_serve_script should succeed");

        let text = std::fs::read_to_string(&script).expect("read script");
        assert!(!text.contains("--cache-type"),
            "f16 should not include cache-type flags:\n{text}");

        let _ = std::fs::remove_file(&script);
        let _ = std::fs::remove_file(&model);
        let _ = std::fs::remove_dir(&dir);
    }
```

**Verify:** `cargo test -p ozone -- generate_serve_script --quiet` — all 4 tests pass (2 existing + 2 new).

---

## Step 5: BUG-006 (auto-fixed)

No code change needed. `run_sweep()` at `src/sweep.rs` line 179 already passes `qkv` through to `bench::run_benchmark_with_progress`. Once BUG-003 is fixed (Step 1), the full sweep automatically uses the correct KV cache quantization.

---

## Final Verification

```bash
cargo test -p ozone --features full --quiet    # all tests pass
cargo clippy -p ozone --features full --all-targets -- -D warnings  # clean
```

Search for any remaining dead `let _ = quant_kv;` in the codebase:
```bash
grep -rn "let _ = quant_kv" src/
# Should produce NO output
```

---

## Summary: Files Touched

| Step | File | Lines changed |
|------|------|---------------|
| 0 | `src/bench.rs` | +20 (helper + tests) |
| 1 | `src/bench.rs` | 4 lines changed (signature + call site + remove dead discard) |
| 2 | `src/ui/backend_args.rs` | +3 lines args, +30 lines tests |
| 2 | `src/bench.rs` | `fn` → `pub fn` (1 char) |
| 3 | `src/sweep.rs` | 2 lines changed (signature + call) |
| 3 | `src/main.rs` | +2 lines (CLI flag + match destructure) |
| 4 | `src/export_server.rs` | +10 lines (import, template var, format), +50 lines tests |
| 5 | None | Auto-fixed |

**Total:** 5 files, ~120 lines added/changed, 9 new tests, 0 behavior regressions.

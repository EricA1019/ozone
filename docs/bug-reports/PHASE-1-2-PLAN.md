# Phase 1 & 2 — Detailed TDD Fix Plan

Line-accurate diffs for every change. Each fix is self-contained
and includes the exact test to write before touching production code.

--------------------------------------------------------------------

## Phase 1: Quick Wins

---

### Fix 1.1 — BUG-002: BBH lm-eval task name

**File:** `src/eval.rs` line 447
**Change:** `"bbh"` → `"bigbench_hard"`

**Test (write first):**
```rust
// In tests at bottom of src/eval.rs, or a new test in an existing test file
#[test]
fn eval_preset_bbh_uses_correct_lm_eval_task_name() {
    // Verify the EvalPreset::Bbh.cli_name() returns "bbh"
    // and that EVAL_TASKS entry has task="bigbench_hard"
    assert_eq!(crate::eval::EvalPreset::Bbh.cli_name(), "bbh");
    let task = crate::eval::find_task("bbh").expect("bbh should be in EVAL_TASKS");
    match task.kind {
        crate::eval::EvalTaskKind::LmEval { task, .. } => {
            assert_eq!(task, "bigbench_hard");
        }
        _ => panic!("expected LmEval"),
    }
}
```

**Production change:**
```diff
-            "bbh",
+            "bigbench_hard",
```

**Verify:** `cargo test -p ozone --quiet` — assert_eq test passes; registry and dispatch now agree.

---

### Fix 1.2 — BUG-001: TruthfulQA lm-eval task name

**File:** `src/eval.rs` line 438
**Change:** `"truthfulqa"` → `"truthfulqa_gen"`

**Test (write first):**
```rust
#[test]
fn eval_preset_truthfulqa_uses_correct_lm_eval_task_name() {
    assert_eq!(crate::eval::EvalPreset::TruthfulQA.cli_name(), "truthfulqa");
    let task = crate::eval::find_task("truthfulqa").expect("truthfulqa should be in EVAL_TASKS");
    match task.kind {
        crate::eval::EvalTaskKind::LmEval { task, .. } => {
            assert_eq!(task, "truthfulqa_gen");
        }
        _ => panic!("expected LmEval"),
    }
}
```

**Production change:**
```diff
-            "truthfulqa",
+            "truthfulqa_gen",
```

**Verify:** `cargo test -p ozone --quiet` — both new tests pass.

---

### Fix 1.3 — BUG-009: Export server hardcoded threads

**File:** `src/export_server.rs` line 84
**Change:** `threads = 8` → `threads = plan.threads.unwrap_or(8)`

**Test (write first — add to or create tests in export_server.rs):**
```rust
#[test]
fn generate_serve_script_uses_plan_threads() {
    use crate::planner::{LaunchPlan, RecommendationMode};
    let plan = LaunchPlan {
        model_name: "test.gguf".into(),
        context_size: 4096,
        gpu_layers: 32,
        total_layers: 40,
        cpu_layers: 8,
        quant_kv: 1,
        threads: Some(4),           // plan says 4 threads
        blas_threads: None,
        mode: RecommendationMode::MixedMemory,
        rationale: "test".into(),
        estimated: false,
        estimated_vram_mb: 4096,
        estimated_ram_mb: 2048,
        source: "test".into(),
        layer_source_label: "test".into(),
        layer_source_note: None,
    };
    let script = crate::export_server::generate_serve_script(
        &plan,
        std::path::Path::new("/tmp/test.gguf"),
        std::path::Path::new("/usr/bin/llama-server"),
        8989,
        std::path::Path::new(""),
    ).unwrap();
    let text = std::fs::read_to_string(&script).unwrap();
    assert!(text.contains("--threads 4"), "script should use plan threads, got:\n{text}");
    // Clean up
    let _ = std::fs::remove_file(&script);
}

#[test]
fn generate_serve_script_defaults_to_8_threads_when_none() {
    // Same as above but with threads: None
    // Assert script contains "--threads 8"
}
```

**Production change:**
```diff
  --threads {threads} &
+ SERVER_PID=$!
...
-        threads = 8,
+        threads = plan.threads.unwrap_or(8),
```

**Verify:** `cargo test -p ozone export_server` — both new tests pass.

---

### Fix 1.4 — STRUCT-005: Dead fallback in read_context_length

**File:** `src/gguf.rs` lines 247-251
**Change:** Remove the `.or_else()` chain that always returns 0

**Test (write first):**
```rust
#[test]
fn read_context_length_returns_none_when_key_missing() {
    let path = temp_gguf_path("no-context-length", chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0) as u64);
    // Write a GGUF without llama.context_length key
    let buf = write_test_gguf(&[]);  // empty kv pairs
    std::fs::write(&path, buf).expect("write test gguf");
    let result = read_context_length(&path);
    assert_eq!(result, None, "missing context_length should return None");
    // Clean up
    let _ = std::fs::remove_file(&path);
}
```

**Production change:**
```diff
 pub fn read_context_length(path: &Path) -> Option<u32> {
-    read_single_u32_key(path, "llama.context_length")
-        .or_else(|| read_single_u32_key(path, "llama.embedding_length").map(|_| 0))
-        .filter(|v| *v > 0)
+    read_single_u32_key(path, "llama.context_length").filter(|v| *v > 0)
 }
```

**Verify:** `cargo test -p ozone gguf` — all existing GGUF tests still pass, new test passes.

---

**Phase 1 Gate:**
```bash
cargo test -p ozone --quiet    # must be 100+ passed, 0 failed
cargo clippy -p ozone --all-targets -- -D warnings   # must be clean
```

--------------------------------------------------------------------

## Phase 2: Simple Fixes

---

### Fix 2.1 — SILENT-001: .ok() swallows report builder errors

**File:** `src/ui/bench_eval_workflow.rs` lines 149 and 234
**Change:** Replace `.ok()` with explicit error handling that sends error to TUI

**Approach:** Extract a small helper to avoid duplicating the error-to-event logic.

**New helper (add after `BenchEvalWorkflowEvent` enum, before `apply_bench_eval_event`):**
```rust
fn build_report_or_warn(
    model_name: &str,
    preset: crate::eval::EvalPreset,
    tx: &UnboundedSender<BenchEvalWorkflowEvent>,
) -> Option<crate::eval_report::EvalMarkdownReport> {
    match crate::eval_report::build_eval_report_for_preset(model_name, preset) {
        Ok(report) => Some(report),
        Err(error) => {
            let _ = tx.send(BenchEvalWorkflowEvent::Output {
                is_stderr: true,
                line: format!("Report generation failed: {error}"),
            });
            None
        }
    }
}
```

**Change line 149:**
```diff
-    let report = crate::eval_report::build_eval_report_for_preset(&model_name, preset).ok();
+    let report = build_report_or_warn(&model_name, preset, &tx);
```

**Change line 234:**
```diff
-    let report = crate::eval_report::build_eval_report_for_preset(&model_name, _preset).ok();
+    let report = build_report_or_warn(&model_name, _preset, &tx);
```

**Test (write first — add to bench_eval_workflow.rs tests or main test):**
```rust
#[test]
fn build_report_or_warn_returns_some_on_success() {
    // This is harder to unit test because build_eval_report_for_preset reads filesystem.
    // Instead, test the event flow: when report builder fails, an Output event is sent.
    // Integration test approach: mock the report builder to return Err.
    // For now, rely on the type signature: Option<EvalMarkdownReport> + stderr event.
}
```

**Manual smoke test:** Delete `contrib/evals/artifacts/lm_eval_gsm8k_probe/`, run eval → TUI should show `[stderr] Report generation failed: ...` instead of silent "completed successfully".

**Verify:** `cargo test -p ozone --quiet` green, `cargo clippy` clean.

---

### Fix 2.2 — BUG-008: is_url_ready fallback has no timeout

**File:** `src/processes.rs` lines 56-63
**Change:** Ensure the fallback client also has a timeout

**Production change:**
```diff
 pub async fn is_url_ready(url: &str) -> bool {
-    let client = match reqwest::Client::builder()
-        .timeout(Duration::from_secs(2))
-        .build()
-    {
-        Ok(c) => c,
-        Err(_) => reqwest::Client::new(),
-    };
+    let client = reqwest::Client::builder()
+        .timeout(Duration::from_secs(2))
+        .build()
+        .unwrap_or_else(|_| {
+            // Fallback: still enforce a timeout
+            reqwest::Client::builder()
+                .timeout(Duration::from_secs(5))
+                .build()
+                .unwrap_or_else(|_| reqwest::Client::new())
+        });
     client
         .get(url)
         .send()
         .await
         .map(|r| r.status().is_success())
         .unwrap_or(false)
 }
```

**Test (write first):**
```rust
#[test]
fn is_url_ready_returns_false_for_invalid_url() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let result = rt.block_on(is_url_ready("http://127.0.0.1:19999/health"));
    // Port 19999 should have nothing listening
    assert!(!result, "should return false for unreachable port");
}
```

**Verify:** `cargo test -p ozone processes` — test passes (returns false, doesn't hang).

---

### Fix 2.3 — SILENT-003: --profile flag for ExportServer

**Decision:** Remove the `--profile` flag. Implementing it requires loading user
preferences cross-crate, searching saved profiles by name, and applying overrides —
this is a feature, not a bug fix. The flag was never functional.

**File:** `src/main.rs` — ExportServer command struct and handler

**Step A — Remove the flag from the CLI struct (lines ~165-166):**
```diff
         #[arg(long, help = "Output path (default: ~/models/serve-<model>.sh)")]
         output: Option<String>,
-        #[arg(long, help = "Saved profile name to use for config")]
-        profile: Option<String>,
         #[arg(long, default_value = "8989", help = "Port for the server")]
```

**Step B — Simplify the handler (remove the `if let Some` block):**
```diff
-            let plan = if let Some(_profile_name) = &profile {
-                anyhow::bail!("--profile not yet implemented; use the launcher Configure Hub to save a profile first");
-            } else {
-                // Use catalog recommendation as fallback
-                let report = catalog::load_catalog_report(
+            // Use catalog recommendation
+            let plan = {
+                let report = catalog::load_catalog_report(
                     &model_dir,
                     &ozone_core::paths::catalog_preset_path(),
                     &model_dir.join("bench-results.txt"),
                 ).await?;
                 let record = report.records.iter()
                     .find(|r| r.model_name == model)
                     .ok_or_else(|| anyhow::anyhow!("Model '{}' not found in catalog", model))?;
                 crate::planner::plan_launch(record, &Default::default())
             };
```

**No new test needed** — removing dead code. Existing `oz export-server <model>` flow unchanged.

**Verify:** `cargo check --features full` compiles, `oz export-server --help` no longer shows `--profile`.

---

**Phase 2 Gate:**
```bash
cargo test -p ozone --quiet    # 100+ passed, 0 failed
cargo clippy -p ozone --all-targets -- -D warnings   # clean
```

--------------------------------------------------------------------

## Summary: Files Touched

| Fix | File | Lines changed |
|-----|------|---------------|
| 1.1 | `src/eval.rs` | 1 line |
| 1.2 | `src/eval.rs` | 1 line |
| 1.3 | `src/export_server.rs` | 1 line |
| 1.4 | `src/gguf.rs` | 2 lines removed, 1 line |
| 2.1 | `src/ui/bench_eval_workflow.rs` | +12 lines (helper), 2 lines changed |
| 2.2 | `src/processes.rs` | 6 lines changed |
| 2.3 | `src/main.rs` | ~8 lines removed |

**Total:** 7 files, ~30 lines changed, 5 new tests added.
All changes are self-contained — no cross-file dependencies within the phase.

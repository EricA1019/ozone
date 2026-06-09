# BUG-B1: TruthfulQA lm-eval task name mismatch

- **Severity:** 🔴 Bug
- **File(s):** `src/eval.rs:440`
- **Found:** 2026-06-08
- **Status:** Open

## What's Wrong
`EvalPreset::TruthfulQA` dispatch in `run_eval()` passes `"truthfulqa"` to `run_lm_eval()` as the lm-eval task name. The correct lm-eval task name is `"truthfulqa_gen"`.

## Evidence
```rust
// src/eval.rs:440 — WRONG
EvalPreset::TruthfulQA => run_lm_eval(
    &venv_bin, model, "truthfulqa", limit,
    &artifacts_dir.join("lm_eval_truthfulqa_probe"),
    base_url, temperature,
)?,

// src/eval.rs:129 — EVAL_TASKS registry (CORRECT, but dead code)
kind: EvalTaskKind::LmEval {
    task: "truthfulqa_gen",
    output_dir: "lm_eval_truthfulqa_probe"
},
```

## Impact
Running `oz eval --preset truthfulqa` (CLI or TUI) will fail — lm-eval cannot find a task named `"truthfulqa"`, only `"truthfulqa_gen"`.

## Suggested Fix
Change `"truthfulqa"` → `"truthfulqa_gen"` in the `EvalPreset::TruthfulQA` dispatch.

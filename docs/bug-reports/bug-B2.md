# BUG-B2: BBH lm-eval task name mismatch

- **Severity:** 🔴 Bug
- **File(s):** `src/eval.rs:449`
- **Found:** 2026-06-08
- **Status:** Open

## What's Wrong
`EvalPreset::Bbh` dispatch in `run_eval()` passes `"bbh"` to `run_lm_eval()`. The correct lm-eval task name is `"bigbench_hard"`.

## Evidence
```rust
// src/eval.rs:449 — WRONG
EvalPreset::Bbh => run_lm_eval(
    &venv_bin, model, "bbh", limit,
    &artifacts_dir.join("lm_eval_bbh_probe"),
    base_url, temperature,
)?,

// src/eval.rs:136 — EVAL_TASKS registry (CORRECT, but dead code)
kind: EvalTaskKind::LmEval {
    task: "bigbench_hard",
    output_dir: "lm_eval_bbh_probe"
},
```

## Impact
Running `oz eval --preset bbh` will fail — lm-eval cannot find a task named `"bbh"`, only `"bigbench_hard"`.

## Suggested Fix
Change `"bbh"` → `"bigbench_hard"` in the `EvalPreset::Bbh` dispatch.

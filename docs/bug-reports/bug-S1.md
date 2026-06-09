# BUG-S1: `.ok()` silently drops report builder errors

- **Severity:** 🟡 Silent
- **File(s):** `src/ui/bench_eval_workflow.rs:149,234`
- **Found:** 2026-06-08
- **Status:** Open

## What's Wrong
```rust
let report = crate::eval_report::build_eval_report_for_preset(&model_name, preset).ok();
```
`.ok()` converts `Result<EvalMarkdownReport, Error>` to `Option<EvalMarkdownReport>`, silently dropping the error. If the eval subprocess failed (no JSON output), the report builder returns `Err(...)`, `report` becomes `None`, and the TUI sends `Completed { report: None }` which displays "Evaluation completed successfully" with no indication of failure.

## Impact
User sees success but no report. No error message. Confusing UX — user thinks everything worked but no results exist.

## Suggested Fix
Log the error rather than dropping it. Consider differentiating "completed with report" vs "completed but report generation failed" in the `Completed` event.

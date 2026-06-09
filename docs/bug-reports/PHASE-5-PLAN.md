# Phase 5 — Eval Architecture Unification: Detailed Implementation Plan

Seven interconnected bugs share one root cause: the eval system has two parallel
code paths — an `EvalPreset` enum-based dispatch (live, 8 match arms) and an
`EVAL_TASKS` registry (dead code, correct but unused). Every new task requires
touching 5+ files with duplicated strings.

The fix unifies both paths while preserving the stable `--preset <name>` CLI.

**Strategy:** Make `EvalPreset` a thin shim over the task registry. Each variant
delegates to `EVAL_TASKS` lookup. This eliminates ALL dead code and reduces the
7 bugs to 0 without breaking any existing functionality.

--------------------------------------------------------------------

## Step 1: Add CreativeWriting support to `run_eval_task`

**File:** `src/eval.rs` line ~187

### Problem (SILENT-004)
`run_eval_task()` bails out with "not yet implemented" for CreativeWriting,
even though creative_writing.rs is fully implemented.

### Fix
Replace the `CreativeWriting` arm in `run_eval_task()` (around line 187):

```rust
        EvalTaskKind::CreativeWriting => {
            bail!("Creative writing eval not yet implemented (Phase 2)");
        }
```
WITH:
```rust
        EvalTaskKind::CreativeWriting => {
            let root = resolve_project_root()?;
            let prompt_bank = crate::creative_writing::load_prompt_bank(&root)?;
            let output_dir = artifacts_dir.join("creative_writing");
            let csv_path = crate::creative_writing::run_creative_writing_eval(
                model, &prompt_bank, base_url, &output_dir,
            )?;
            let report_md = crate::creative_writing::build_creative_report(&csv_path)?;
            let report_path = csv_path.with_extension("md");
            std::fs::write(&report_path, &report_md)?;
            ozone_core::cli::success(&format!("Creative writing eval complete for '{model}'"));
            ozone_core::cli::field("CSV:", &csv_path.display());
            ozone_core::cli::field("Report:", &report_path.display());
            return Ok(());
        }
```

Note: `run_creative_writing_eval` is `async` but `run_eval_task` is currently
sync. This conversion requires making `run_eval_task` async. Since all its
existing branches are sync (subprocess spawn-and-wait), they'll work fine in an
async context. Add `async` to the function and `.await` the creative writing call.

**Change `run_eval_task` signature** (line ~149):
```rust
pub async fn run_eval_task(    // was: pub fn
```

And in `write_eval_csv` (line ~211), where it matches `CreativeWriting`:
The existing `write_eval_csv` already handles `EvalTaskKind::CreativeWriting`
at line ~214 — the path is `artifacts_dir.join("creative_writing").join(model)`.
This is already correct.

**No new tests** — creative writing has existing integration flow from CLI.

---

## Step 2: Make `run_eval()` delegate to `run_eval_task()`

**File:** `src/eval.rs` lines 370-480

### Problem (BUG-007 + STRUCT-001 items 3-6)
`run_eval()` has an 8-arm match against `EvalPreset` with hardcoded task names
and output directories. `run_eval_task()` is the equivalent registry-based
version marked `#[allow(dead_code)]`. The live path (run_eval) has all the bugs.

### Fix
Replace the entire `run_eval()` function body with a delegation to the task registry.
Find the code from `let status = match preset {` through the status check block
(lines 383-470) and replace with:

```rust
    // Look up the task from the registry using the preset's CLI name
    let task = EVAL_TASKS
        .iter()
        .find(|t| t.cli_name == preset.cli_name())
        .ok_or_else(|| anyhow::anyhow!(
            "Preset '{}' not found in eval task registry",
            preset.cli_name()
        ))?;

    run_eval_task(task, model, limit, base_url, temperature)?;
```

The rest of `run_eval()` after the status check (lines 472-488 — the success message,
report building, and report writing) can be deleted since `run_eval_task` already
handles success messages. Actually, `run_eval_task` does its own success message
and CSV writing. But `run_eval` also builds the markdown report. We should keep
that part.

So the replacement should be:
```rust
    // Delegate to the task registry — single source of truth for dispatch
    let task = EVAL_TASKS
        .iter()
        .find(|t| t.cli_name == preset.cli_name())
        .ok_or_else(|| anyhow::anyhow!(
            "Internal error: preset '{}' not in EVAL_TASKS registry",
            preset.cli_name()
        ))?;

    run_eval_task(task, model, limit, base_url, temperature)?;

    // Build markdown report (uses preset for backward-compatible reporting)
    match crate::eval_report::build_eval_report_for_preset(model, preset) {
        Ok(report) => {
            if let Err(error) = crate::eval_report::write_eval_report(&report) {
                eprintln!("Markdown report could not be written: {error}");
            } else {
                ozone_core::cli::field("Markdown report:", &report.markdown_path.display());
            }
        }
        Err(error) => {
            eprintln!("Markdown report could not be generated: {error}");
        }
    }
    Ok(())
```

**Also remove the dead code annotations:**
- Remove `#[allow(dead_code)]` from `EvalTaskKind::CreativeWriting` (line ~67)
- Remove `#[allow(dead_code)]` from `EvalTask` struct (line ~75)
- Remove `#[allow(dead_code)]` from `run_eval_task` (line ~148)
- Remove `#[allow(dead_code)]` from `write_eval_csv` (line ~211)

**Verify:** `cargo check -p ozone` compiles. `oz eval --preset gsm8k --limit 1 <model>` works same as before.

---

## Step 3: Make `build_eval_report_for_preset` use registry metadata

**File:** `src/eval_report.rs` lines 15-65

### Problem (STRUCT-002 + STRUCT-003)
`build_eval_report_for_preset()` has an 8-arm match on `EvalPreset` with hardcoded
output directory strings. These strings are already in the `EVAL_TASKS` registry.

### Fix
Replace the entire match block inside `build_eval_report_for_preset()` (lines 22-64):

```rust
pub(crate) fn build_eval_report_for_preset(
    model: &str,
    preset: EvalPreset,
) -> Result<EvalMarkdownReport> {
    let root = crate::eval::resolve_project_root()?;
    let artifacts_dir = root.join("contrib/evals/artifacts");

    // Look up output directory from the task registry
    let task = crate::eval::EVAL_TASKS
        .iter()
        .find(|t| t.cli_name == preset.cli_name())
        .with_context(|| format!("preset '{}' not in registry", preset.cli_name()))?;

    let output_dir = match task.kind {
        crate::eval::EvalTaskKind::LmEval { output_dir, .. } => output_dir,
        crate::eval::EvalTaskKind::EvalPlus { output_dir } => output_dir,
        crate::eval::EvalTaskKind::CreativeWriting => "creative_writing",
    };

    let title = format!("{} eval report", preset.report_label());

    match task.kind {
        crate::eval::EvalTaskKind::LmEval { .. } => {
            build_lm_eval_report(&title, &artifacts_dir.join(output_dir).join(model))
        }
        crate::eval::EvalTaskKind::EvalPlus { .. } => {
            build_evalplus_report(&title, &artifacts_dir.join(output_dir).join("humaneval"), model)
        }
        crate::eval::EvalTaskKind::CreativeWriting => {
            // Creative writing reports are generated alongside the CSV by the runner
            let csv_path = artifacts_dir
                .join(output_dir)
                .join(format!("{model}_creative.csv"));
            let csv_path = std::path::PathBuf::from(csv_path);
            let markdown = crate::creative_writing::build_creative_report(&csv_path)?;
            let markdown_path = csv_path.with_extension("md");
            Ok(EvalMarkdownReport {
                title,
                markdown,
                source_path: csv_path,
                markdown_path,
            })
        }
    }
}
```

The old 8-arm match (lines 22-64) is **completely replaced** by the above.

**Verify:** `cargo test -p ozone -- eval_report --quiet` — existing tests pass.
`cargo check -p ozone` compiles.

---

## Step 4: Generate TUI eval entries from `EVAL_TASKS`

**File:** `src/ui/bench_eval.rs` lines 38-53

### Problem (STRUCT-003)
`entries()` hardcodes each eval task as a separate `BenchEvalEntry`. Adding a
new task requires touching this file. It should be generated from the registry.

### Fix
Replace the hardcoded eval entries in `entries()` (lines 40-48) with
registry-driven generation. Keep the non-eval entries (ProfileModel, ExportServer,
ViewResults, ViewReport, Back) hardcoded since they're not in the registry.

Change the `entries()` function body from:
```rust
pub(super) fn entries() -> Vec<BenchEvalEntry> {
    vec![
        BenchEvalEntry { action: BenchEvalAction::EvalGsm8k, ... },
        // ... 8 hardcoded eval entries ...
        BenchEvalEntry { action: BenchEvalAction::ProfileModel, ... },
        // ... non-eval entries ...
    ]
}
```

TO (conceptually — the exact mapping of cli_name → BenchEvalAction still uses the
8-action match since actions are typed. This step is OPTIONAL and low-priority;
the main value is Steps 1-3 above):

```rust
pub(super) fn entries() -> Vec<BenchEvalEntry> {
    let mut entries: Vec<BenchEvalEntry> = Vec::new();
    
    // Generate eval entries from the task registry
    for task in crate::eval::EVAL_TASKS {
        let action = eval_action_for_cli_name(task.cli_name);
        entries.push(BenchEvalEntry {
            action,
            label: format!("Eval {}", task.report_label),
            description: task.description.to_string(),
            command: format!("eval-{}", task.cli_name),
        });
    }
    
    // Add non-eval entries
    entries.push(BenchEvalEntry { action: BenchEvalAction::ProfileModel, /* ... */ });
    entries.push(BenchEvalEntry { action: BenchEvalAction::EvalCreativeWriting, /* ... */ });
    entries.push(BenchEvalEntry { action: BenchEvalAction::ExportServer, /* ... */ });
    entries.push(BenchEvalEntry { action: BenchEvalAction::ViewResults, /* ... */ });
    entries.push(BenchEvalEntry { action: BenchEvalAction::ViewReport, /* ... */ });
    entries.push(BenchEvalEntry { action: BenchEvalAction::Back, /* ... */ });
    entries
}

fn eval_action_for_cli_name(name: &str) -> BenchEvalAction {
    match name {
        "gsm8k" => BenchEvalAction::EvalGsm8k,
        "instruction" => BenchEvalAction::EvalInstruction,
        "math" => BenchEvalAction::EvalMath,
        "humaneval" => BenchEvalAction::EvalHumaneval,
        "mmlu" => BenchEvalAction::EvalMmlu,
        "hellaswag" => BenchEvalAction::EvalHellaSwag,
        "truthfulqa" => BenchEvalAction::EvalTruthfulQA,
        "bbh" => BenchEvalAction::EvalBbh,
        _ => BenchEvalAction::Back, // fallback — shouldn't happen
    }
}
```

Note: This still has an 8-arm match for `BenchEvalAction` conversion, but now the
entry *generation* is registry-driven. A future pass could replace `BenchEvalAction`
variants with a single `BenchEvalAction::RunEval(String)` to eliminate this too,
but that's outside Phase 5 scope.

**Verify:** `cargo check -p ozone` compiles. The Bench+Eval screen shows the same
entries as before, in the same order, with the same actions.

---

## Step 5: Remove 6 dead-code items (STRUCT-001)

After Steps 1-4, these items are no longer dead code — they're the live path:

| Item | Status after Step 1-4 |
|------|----------------------|
| `EvalTaskKind::CreativeWriting` | ✅ Live — used by run_eval_task |
| `EvalTask` struct | ✅ Live — used by run_eval for delegation |
| `run_eval_task()` | ✅ Live — THE dispatch function |
| `write_eval_csv()` | ✅ Live — called by run_eval_task |
| `start_eval(preset)` in bench_eval_flow.rs | Still dead — was already superseded by `start_eval_with_cli_name`. Keep `#[allow(dead_code)]` or remove. |
| `run_bench_eval_workflow()` in bench_eval_workflow.rs | Still dead — was superseded by `run_bench_eval_workflow_with_cli_name`. Keep `#[allow(dead_code)]` or remove. |

**Action:** Remove `#[allow(dead_code)]` from the first 4 items. For items 5 and 6,
either remove the functions entirely (they have no callers) or keep them with
`#[allow(dead_code)]`. Prefer removal since they're unused and `_with_cli_name`
variants are the live path.

### Remove `start_eval` (bench_eval_flow.rs)
Search for `#[allow(dead_code)] async fn start_eval` and delete the entire function
(lines ~131-165 in bench_eval_flow.rs). It's `#[allow(dead_code)]` and has no callers.

### Remove `run_bench_eval_workflow` (bench_eval_workflow.rs)
Search for `#[allow(dead_code)] pub(super) async fn run_bench_eval_workflow` and
delete the entire function (lines ~73-158 in bench_eval_workflow.rs).
`run_bench_eval_workflow_with_cli_name` is the live replacement.

**Verify:** `cargo check -p ozone` compiles with no dead_code warnings for these
items. Search for `#[allow(dead_code)]` in `src/eval.rs` — zero remaining.

---

## Step 6: Clean up `EvalPreset` (keep but simplify)

**File:** `src/eval.rs` lines 7-62

### Strategy
`EvalPreset` is a `clap::ValueEnum` used by the CLI `--preset` flag. We keep
it for backward compatibility but remove redundant methods that duplicate registry data.

**Remove** `description()` method (lines 19-30) — it duplicates `EvalTask.description`.
**Remove** `report_label()` method (lines 48-61) — it duplicates `EvalTask.report_label`.

Replace any callers of `preset.description()` with a registry lookup:
```rust
EVAL_TASKS.iter().find(|t| t.cli_name == preset.cli_name()).map(|t| t.description).unwrap_or("")
```

Replace any callers of `preset.report_label()` similarly:
```rust
EVAL_TASKS.iter().find(|t| t.cli_name == preset.cli_name()).map(|t| t.report_label).unwrap_or("")
```

Actually, let's be pragmatic: these methods are called in several places. Instead
of removing them, have them delegate to the registry:

```rust
impl EvalPreset {
    pub(crate) fn description(self) -> &'static str {
        EVAL_TASKS.iter()
            .find(|t| t.cli_name == self.cli_name())
            .map(|t| t.description)
            .unwrap_or("unknown")
    }

    pub(crate) fn report_label(self) -> &'static str {
        EVAL_TASKS.iter()
            .find(|t| t.cli_name == self.cli_name())
            .map(|t| t.report_label)
            .unwrap_or("unknown")
    }
}
```

This eliminates the 8-arm match in each method, replacing it with a registry lookup.
The old match arms (lines 19-30, 48-61) are replaced by the 4-line delegates above.

**Verify:** `cargo check -p ozone` compiles. All existing CLI and report behavior unchanged.

---

## Step 7: UX-002 — Tokenizer backend investigation

**Not a code change.** Document the finding:

`tokenizer_backend=None` in `run_lm_eval()` at `src/eval.rs` line ~495 means
lm-eval skips local tokenization. For GSM8K/Math tasks this is fine (simple
input format). For MMLU and TruthfulQA it may cause degraded prompt formatting.

**Action:** Add a comment above the `tokenizer_backend=None` in the `model_args`:
```rust
    // tokenizer_backend=None is safe for simple completion tasks (GSM8K, Math).
    // For MMLU/TruthfulQA with complex prompt formatting, consider using
    // tokenizer_backend=huggingface with a local tokenizer in the future.
    let model_args = format!(
        "model={model},base_url={completions_url},tokenizer_backend=None,temperature={temperature}"
    );
```

No behavior change. The flag is intentionally `None` as documented.

---

## Final Verification

```bash
cargo test -p ozone --quiet                        # all tests pass
cargo test -p ozone --features full --quiet        # all feature-gated tests  
cargo clippy -p ozone --all-targets -- -D warnings # clean (default)
cargo clippy -p ozone --features full --all-targets -- -D warnings  # clean (full)

# Verify zero remaining dead_code annotations in eval chain
grep -rn "allow(dead_code)" src/eval.rs
# Should produce NO output (or only on items we intentionally kept)

# Manual smoke: run each of the 8 presets
oz eval --preset gsm8k --limit 1 <model> --base-url http://127.0.0.1:8989
oz eval --preset hellaswag --limit 1 <model> --base-url http://127.0.0.1:8989
oz eval --preset mmlu --limit 1 <model> --base-url http://127.0.0.1:8989
oz eval --preset truthfulqa --limit 1 <model> --base-url http://127.0.0.1:8989
oz eval --preset bbh --limit 1 <model> --base-url http://127.0.0.1:8989
oz eval --preset humaneval --limit 1 <model> --base-url http://127.0.0.1:8989
oz eval --preset instruction --limit 1 <model> --base-url http://127.0.0.1:8989
oz eval --preset math --limit 1 <model> --base-url http://127.0.0.1:8989
```

---

## Summary: Files Touched

| Step | File | Lines changed |
|------|------|---------------|
| 1 | `src/eval.rs` | ~15 (creative writing arm in run_eval_task) |
| 2 | `src/eval.rs` | ~100 removed, +15 (delegation) |
| 3 | `src/eval_report.rs` | ~45 removed, +30 (registry-driven) |
| 4 | `src/ui/bench_eval.rs` | ~20 changed (registry-driven entries) |
| 5 | `src/eval.rs`, `bench_eval_flow.rs`, `bench_eval_workflow.rs` | ~80 removed (dead code) |
| 6 | `src/eval.rs` | ~40 removed, +10 (delegating methods) |
| 7 | `src/eval.rs` | +5 (comment only) |

**Total:** 5 files, ~200 lines removed, ~80 added. Net reduction of ~120 lines.
7 bugs resolved with 0 behavior changes.

**Risk:** Medium. Steps 1-3 touch the core eval dispatch. Step 2 specifically
replaces the entire live dispatch with the previously-dead registry path.
The CLI flag, all 8 presets, and every caller must produce identical results.

# Phase 4 — TUI Integration: Detailed Implementation Plan

Three independent fixes for TUI gaps. Each is self-contained.

--------------------------------------------------------------------

## Step 1: SILENT-005 — start_llamacpp returns Ok for wrong model

**File:** `src/processes.rs` lines 357-361

### Problem
If a llama-server is already running (from a previous launch), `start_llamacpp`
returns `Ok(())` without checking whether the running model matches the requested
model. The user thinks they launched a new model but are actually talking to the old one.

### Test (write first)

Add to the `llamacpp_tests` module in `src/processes.rs`. Since this requires a
running server to test, write a focused unit test that exercises the model-matching
logic.

Find the `mod llamacpp_tests` block (around line 750) and add:

```rust
    #[test]
    fn model_name_matches_running_detects_exact_match() {
        // Extract the matching logic into a testable helper.
        // The helper is added in Step 1B below.
        assert!(model_name_matches_running("my-model.gguf", Some("my-model.gguf")));
    }

    #[test]
    fn model_name_matches_running_detects_containment() {
        // llama.cpp API may return the full path or just the filename
        assert!(model_name_matches_running("my-model.gguf", Some("/models/my-model.gguf")));
    }

    #[test]
    fn model_name_matches_running_returns_false_for_mismatch() {
        assert!(!model_name_matches_running("model-b.gguf", Some("model-a.gguf")));
    }

    #[test]
    fn model_name_matches_running_returns_false_when_no_model_loaded() {
        // If get_llamacpp_model() returns None, server is running but no model info
        assert!(!model_name_matches_running("anything.gguf", None));
    }
```

### Production change (two parts)

**1A:** Add a pure helper function near the top of `src/processes.rs`, after the existing
`get_llamacpp_model` function (around line 127, after `fn llamacpp_config_fingerprint`):

```rust
/// Check whether the model currently loaded by llama-server matches the
/// requested model_name. Handles partial matches (API may return full path).
fn model_name_matches_running(requested: &str, running: Option<String>) -> bool {
    match running {
        Some(id) => id == requested || id.contains(requested),
        None => false,
    }
}
```

**1B:** Replace lines 357-361 in `start_llamacpp`:
```rust
    if is_url_ready(&paths::llamacpp_ready_url()).await {
        return Ok(());
    }
```
WITH:
```rust
    if is_url_ready(&paths::llamacpp_ready_url()).await {
        // Verify the running model matches the requested one.
        // If not, kill the old server and proceed with launch.
        let running = get_llamacpp_model().await;
        if model_name_matches_running(model_name, running) {
            return Ok(());
        }
        // Wrong model or couldn't verify — kill and restart
        clear_gpu_backends().await?;
    }
```

**Verify:** `cargo test -p ozone -- model_name_matches --quiet` — 4 tests pass.
`cargo check -p ozone` compiles.

---

## Step 2: SILENT-002 — Creative Writing and Export Server are TUI dummies

**File:** `src/ui/bench_eval_flow.rs` lines 89-98

### Problem
`EvalCreativeWriting` and `ExportServer` actions only show CLI hints like
`"Use CLI: oz creative-write \"{model_name}\""` — they don't do any real work.
No progress, no output, just a status message that fades.

### 2A: Wire Creative Writing to spawn real eval

**Production change:** In `src/ui/bench_eval_flow.rs`, replace the `EvalCreativeWriting` match arm (lines 89-93):
```rust
        BenchEvalAction::EvalCreativeWriting => {
            let Some(model_name) = resolve_bench_eval_model(app) else {
                app.set_error("No model selected. Select or launch a model first.".into());
                return;
            };
            app.set_status(format!("Use CLI: oz creative-write \"{model_name}\""));
        }
```
WITH:
```rust
        BenchEvalAction::EvalCreativeWriting => {
            let Some(model_name) = resolve_bench_eval_model(app) else {
                app.set_error("No model selected. Select or launch a model first.".into());
                return;
            };
            let model_name = model_name.to_string();
            app.set_status("Running creative writing eval…".into());
            tokio::spawn(async move {
                eprintln!("Starting creative writing eval for {model_name}…");
                let prompts = crate::creative_writing::load_prompts(
                    crate::creative_writing::DEFAULT_PROMPT_BANK_PATH,
                );
                // The creative writing probe writes its own CSV + emits its own progress
                // via eprintln!. We don't use the bench_eval_workflow channel here
                // because creative_writing has its own output format.
                let output_dir = std::path::PathBuf::from("contrib/evals/artifacts")
                    .join("creative_writing");
                let _ = crate::creative_writing::run_creative_writing_eval(
                    &model_name,
                    &prompts,
                    &ozone_core::paths::llamacpp_base_url(),
                    &output_dir,
                ).await;
            });
        }
```

### 2B: Wire Export Server to generate script to default location

**Production change:** Replace the `ExportServer` match arm (lines 94-98):
```rust
        BenchEvalAction::ExportServer => {
            if let Some(model_name) = resolve_bench_eval_model(app) {
                app.set_status(format!("Use CLI: oz export-server \"{model_name}\""));
            } else {
                app.set_error("No model selected. Launch or select a model first.".into());
            }
        }
```
WITH:
```rust
        BenchEvalAction::ExportServer => {
            let Some(model_name) = resolve_bench_eval_model(app) else {
                app.set_error("No model selected. Launch or select a model first.".into());
                return;
            };
            let model = model_name.to_string();
            app.set_status("Generating server script…".into());
            tokio::spawn(async move {
                let model_dir = ozone_core::paths::models_dir();
                let model_path = model_dir.join(&model);
                let server_path = match crate::processes::resolved_llamacpp_server_path() {
                    Ok(p) => p,
                    Err(e) => {
                        eprintln!("Failed to resolve server path: {e}");
                        return;
                    }
                };
                // Use catalog recommendation as the plan
                let plan = match crate::catalog::load_catalog_report(
                    &model_dir,
                    &ozone_core::paths::catalog_preset_path(),
                    &model_dir.join("bench-results.txt"),
                ).await {
                    Ok(report) => {
                        match report.records.iter().find(|r| r.model_name == model) {
                            Some(record) => crate::planner::plan_launch(record, &Default::default()),
                            None => {
                                eprintln!("Model '{model}' not found in catalog — cannot export");
                                return;
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("Failed to load catalog: {e}");
                        return;
                    }
                };
                let output = model_dir.join(format!("serve-{model}.sh"));
                match crate::export_server::generate_serve_script(
                    &plan, &model_path, &server_path, 8989, &output,
                ) {
                    Ok(path) => eprintln!("Server script written to {}", path.display()),
                    Err(e) => eprintln!("Failed to generate script: {e}"),
                }
            });
        }
```

**Verify:** `cargo check -p ozone` compiles. Manual smoke: Press Enter on "Eval Creative Writing" or "Export Server" in the TUI — see progress messages in terminal (via eprintln!), not just "Use CLI…".

---

## Step 3: UX-001 — Model picker from Bench+Eval screen

**File:** `src/ui/bench_eval_flow.rs` — `handle_bench_eval_key()` function

### Problem
The Bench+Eval screen has no way to pick a model. If `resolve_bench_eval_model()`
returns None (no catalog entry, no active plan, no last model name), the user gets
an error with no way to fix it from within the screen.

### Production change

**3A:** Add a new model picker mode variant in `src/ui/mod.rs` (add after line 140, after the `#[cfg(feature = "profiling-ui")] Profile` variant):

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum ModelPickerMode {
    Launch,
    Configure,
    #[cfg(feature = "profiling-ui")]
    Profile,
    /// Open from Bench+Eval screen — returns to BenchEval after selection
    BenchEval,
}
```

**3B:** Update the model picker Enter handler in `src/ui/model_picker_flow.rs` to handle the new mode. In the `ModelPickerMode::Launch | ModelPickerMode::Configure => {` block (around line 42), add `ModelPickerMode::BenchEval` to the match. Replace:
```rust
                    ModelPickerMode::Launch | ModelPickerMode::Configure => {
```
WITH:
```rust
                    ModelPickerMode::Launch | ModelPickerMode::Configure
                    | ModelPickerMode::BenchEval => {
```
And inside that block, add a check to return to BenchEval if in that mode. After the `app.screen = Screen::ConfigureHub;` line (line 48), add:
```rust
                        if app.model_picker_mode == ModelPickerMode::BenchEval {
                            app.screen = Screen::BenchEval;
                            return;
                        }
```
Wait — actually for BenchEval mode we should NOT go to ConfigureHub. We should just select the model and return. Let me reconsider.

Actually, the simplest approach: When BenchEval mode, just store the selected model name and return to BenchEval. We don't need to go through ConfigureHub at all.

Replace the entire `ModelPickerMode::Launch | ModelPickerMode::Configure => { ... }` block in `src/ui/model_picker_flow.rs`:

```rust
                    ModelPickerMode::BenchEval => {
                        // Just store the selected model name and return to Bench+Eval
                        app.current_plan = None;
                        app.configure_recommended_plan = None;
                        app.screen = Screen::BenchEval;
                    }
                    ModelPickerMode::Launch | ModelPickerMode::Configure => {
```

(Insert the new BenchEval arm BEFORE the existing Launch|Configure arm, inside the same `match app.model_picker_mode` block around lines 40-50.)

**3C:** Add a key binding in `handle_bench_eval_key()` in `src/ui/bench_eval_flow.rs`. Add a new `KeyCode::Char('m')` handler after the `KeyCode::Char(ch) if ch.is_ascii_digit()` block (around line 46), before the `KeyCode::Enter` handler:

```rust
        KeyCode::Char('m') => {
            if !app.catalog.is_empty() {
                app.model_picker_mode = ModelPickerMode::BenchEval;
                app.screen = Screen::ModelPicker;
            } else {
                app.set_error("No models available. Add models first.".into());
            }
        }
```

**3D:** Update the status bar hint to show the 'm' shortcut. Find the Bench+Eval status rendering — this is in the BenchEval render at the bottom of the screen. In `src/ui/bench_eval.rs` (the render file), add `[m] model  ` to the hint line. This is a cosmetic change; the key binding from 3C is the functional one.

Look for the status/hint line in the BenchEval render function (around line ~100 in `src/ui/bench_eval.rs` — the line that renders `[q] quit  [↑↓] select  [Enter] run`). Add `[m] model` before `[q] quit`:

Find the line that renders the bottom hint bar (likely something like):
```rust
        Line::from(Span::styled(" [q] quit  [↑↓] select  [Enter] run  [1-9] quick", ...))
```
Change to:
```rust
        Line::from(Span::styled(" [m] model  [q] quit  [↑↓] select  [Enter] run  [1-9] quick", ...))
```
(If the exact text differs, add `[m] model  ` at the start of the hint string.)

**Verify:** `cargo check -p ozone` compiles. Manual smoke: From Bench+Eval, press `m` → model picker opens → select model → returns to Bench+Eval.

---

## Final Verification

```bash
cargo test -p ozone --quiet                    # all tests pass
cargo test -p ozone --features full --quiet    # all feature-gated tests
cargo clippy -p ozone --all-targets -- -D warnings   # clean (default)
cargo clippy -p ozone --features full --all-targets -- -D warnings  # clean (full)
```

Manual smoke tests:
1. `oz` → Enter launcher → press 'b' for Bench+Eval → press 'm' → model picker opens → select → returns to Bench+Eval
2. From Bench+Eval → select "Eval Creative Writing" → Enter → see progress in terminal
3. From Bench+Eval → select "Export Server" → Enter → generated script appears in ~/models/
4. `oz launch` model A → kill terminal → `oz launch` model B → should kill A first (or start B alongside if same model)

---

## Summary: Files Touched

| Step | File | Lines changed |
|------|------|---------------|
| 1 | `src/processes.rs` | +20 (helper + fix + 4 tests) |
| 2 | `src/ui/bench_eval_flow.rs` | ~30 (two arm replacements) |
| 3 | `src/ui/mod.rs` | +1 (ModelPickerMode variant) |
| 3 | `src/ui/model_picker_flow.rs` | +5 (BenchEval mode arm) |
| 3 | `src/ui/bench_eval_flow.rs` | +8 (key binding) |
| 3 | `src/ui/bench_eval.rs` | ~1 (hint text update) |

**Total:** 4 files, ~65 lines added/changed, 4 new tests, 0 behavior regressions.
All three fixes are independent — can be implemented in any order.

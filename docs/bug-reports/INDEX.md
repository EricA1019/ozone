# Bug Reports

Generated 2026-06-08 during eval/bench/export audit.  
Format: `severity-id.md` — see `TEMPLATE.md` for submission format.

## Active (18)

### 🔴 Bugs — Wrong Results or Crashes
| ID | File | Summary |
|----|------|---------|
| [B1](bug-B1.md) | `src/eval.rs:440` | TruthfulQA lm-eval task name mismatch (`truthfulqa` vs `truthfulqa_gen`) |
| [B2](bug-B2.md) | `src/eval.rs:449` | BBH lm-eval task name mismatch (`bbh` vs `bigbench_hard`) |
| [B3](bug-B3.md) | `src/bench.rs:159` | `let _ = quant_kv;` — KV cache quantization ignored |
| [B4](bug-B4.md) | `src/ui/backend_args.rs` | Launcher `build_llama_args()` never passes `--cache-type-k`/`-ctv` |
| [B5](bug-B5.md) | `src/sweep.rs:428` | `run_context_sweep()` hardcodes `quant_kv=1` |
| [B6](bug-B6.md) | `src/sweep.rs` + `src/bench.rs` | Sweep tests multiple `quant_kv` levels but they produce identical results |
| [B7](bug-B7.md) | `src/eval.rs` | Two parallel diverging eval code paths (registry dead, enum live) |

### 🟡 Silent Failures — Errors Hidden, Actions Don't Work
| ID | File | Summary |
|----|------|---------|
| [S1](bug-S1.md) | `src/ui/bench_eval_workflow.rs:149,234` | `.ok()` swallows report builder errors |
| [S2](bug-S2.md) | `src/ui/bench_eval_flow.rs:83-94` | Creative Writing & Export Server are TUI dummies |
| [S3](bug-S3.md) | `src/main.rs:496` | `--profile` flag for ExportServer unimplemented |
| [S4](bug-S4.md) | `src/eval.rs:187` | Creative writing eval in registry path bails out |

### 🟣 Structural / Design Issues
| ID | File | Summary |
|----|------|---------|
| [D1](bug-D1.md) | `src/eval.rs`, flow, workflow | 6 dead-code items in eval chain (200+ lines) |
| [D2](bug-D2.md) | `src/eval.rs`, `src/eval_report.rs` | Output directory strings hardcoded in 3 places, no shared constant |
| [D3](bug-D3.md) | `src/eval.rs` | `EvalPreset` should be generated from task registry |
| [D4](bug-D4.md) | `src/ui/bench_eval_workflow.rs` | Subprocess approach is fragile |

### 🟢 Minor / UX
| ID | File | Summary |
|----|------|---------|
| [U1](bug-U1.md) | `src/ui/bench_eval_flow.rs:162` | No model picker inside Bench+Eval screen |
| [U2](bug-U2.md) | `src/eval.rs:495` | `tokenizer_backend=None` may degrade MMLU/TruthfulQA results |
| [U3](bug-U3.md) | `src/eval.rs:582` | `ensure_executable` error is clear but no auto-install offer |

## Fixed (since 2026-06-08)
| ID | Summary | Fix |
|----|---------|-----|
| F1 | `EvalPreset` incomplete (4 variants, need 8) | Added Mmlu, HellaSwag, TruthfulQA, Bbh variants |
| F2 | TUI `cli_name` → `EvalPreset` all mapped to Gsm8k | Corrected each to its own variant |
| F3 | `DEFAULT_LLAMACPP_PORT` 8080 vs managed port 8989 | Changed to 8989 |
| F4 | `bench.rs` passed short name instead of full path | Now passes `model_path.to_string_lossy()` |
| F5 | Ozone aliases scattered across codebase | Consolidated to single `oz` |

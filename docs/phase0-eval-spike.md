# Phase 0 Eval Spike

## Target Endpoint

- Base URL: `http://127.0.0.1:8989`
- Health endpoint: `http://127.0.0.1:8989/health`
- OpenAI-compatible models endpoint: `http://127.0.0.1:8989/v1/models`
- Verified live model id: `gemma-4-E4B-it-UD-Q8_K_XL.gguf`
- Verified live server behavior: `/health` returns `{"status":"ok"}` once the model finishes loading.

## Runner Environment Contract

- Supported environment: repo-local virtualenv at `contrib/evals/.venv`
- Bootstrap entrypoint: `contrib/evals/bootstrap.sh`
- Python requirement source: `contrib/evals/requirements-evalplus.txt` and `contrib/evals/requirements-lm-eval.txt`
- Verified package versions:
  - `evalplus==0.3.1`
  - `lm-eval==0.4.12`
  - `openai==2.38.0`
  - `tenacity==9.1.4`
  - `langdetect==1.0.9`
  - `immutabledict==4.3.1`
  - `math-verify==0.9.0`
  - `sympy==1.14.0`
  - `antlr4-python3-runtime==4.11.0`

## Runner A: EvalPlus

### Verified command

```bash
OPENAI_API_KEY=none \
contrib/evals/.venv/bin/evalplus.codegen \
  gemma-4-E4B-it-UD-Q8_K_XL.gguf humaneval \
  --backend openai \
  --base_url http://127.0.0.1:8989/v1 \
  --root contrib/evals/artifacts/evalplus_probe \
  --n_samples 1 \
  --temperature 0.0 \
  --greedy \
  --id_range '[0,1]'
```

### Verified artifacts

- Sanitized code output:
  `contrib/evals/artifacts/evalplus_probe/humaneval/gemma-4-E4B-it-UD-Q8_K_XL.gguf_openai_temp_0.0.jsonl`
- Raw code output:
  `contrib/evals/artifacts/evalplus_probe/humaneval/gemma-4-E4B-it-UD-Q8_K_XL.gguf_openai_temp_0.0.raw.jsonl`

### Findings

- The OpenAI-compatible `openai` backend works against llama.cpp when `base_url` is set to the `/v1` root.
- `id_range` must be passed as a two-item list literal such as `'[0,1]'`.
- `evalplus.evaluate` does not accept a narrow subset artifact as a valid complete split and fails with `Missing problems in samples`. The full split or a dedicated reduced split is required for end-to-end evaluation.

### Pass criteria status

- Runnable command shape: confirmed
- Stable artifact paths: confirmed
- Parseable output files: confirmed
- Full split evaluation on a subset probe: not confirmed; documented incompatibility

## Runner B: lm-evaluation-harness

### Verified command family

All successful probes used:

- model backend: `local-completions`
- model args:
  `model=gemma-4-E4B-it-UD-Q8_K_XL.gguf,base_url=http://127.0.0.1:8989/v1/completions,tokenizer_backend=None`

### Verified probes

#### GSM8K

```bash
OPENAI_API_KEY=none \
contrib/evals/.venv/bin/lm-eval run \
  --model local-completions \
  --model_args model=gemma-4-E4B-it-UD-Q8_K_XL.gguf,base_url=http://127.0.0.1:8989/v1/completions,tokenizer_backend=None \
  --tasks gsm8k \
  --limit 1 \
  --output_path contrib/evals/artifacts/lm_eval_gsm8k_probe
```

#### Instruction-following

```bash
OPENAI_API_KEY=none \
contrib/evals/.venv/bin/lm-eval run \
  --model local-completions \
  --model_args model=gemma-4-E4B-it-UD-Q8_K_XL.gguf,base_url=http://127.0.0.1:8989/v1/completions,tokenizer_backend=None \
  --tasks leaderboard_instruction_following \
  --limit 1 \
  --output_path contrib/evals/artifacts/lm_eval_instruction_probe
```

#### Math

```bash
OPENAI_API_KEY=none \
contrib/evals/.venv/bin/lm-eval run \
  --model local-completions \
  --model_args model=gemma-4-E4B-it-UD-Q8_K_XL.gguf,base_url=http://127.0.0.1:8989/v1/completions,tokenizer_backend=None \
  --tasks leaderboard_math_hard \
  --limit 1 \
  --output_path contrib/evals/artifacts/lm_eval_math_probe
```

### Findings

- `local-completions` expects the concrete completions endpoint, not the `/v1` root.
- `tokenizer_backend=None` is required for this llama.cpp server because remote tokenizer endpoints are not available.
- `lm-eval` API-backed local runs required `tenacity` even though the base package install did not provide it.
- The instruction-following task family also needed `langdetect`, `immutabledict`, and runtime NLTK data (`punkt_tab`).
- The math task family also needed `math-verify`, `sympy`, and `antlr4-python3-runtime==4.11.0`.

### Pass criteria status

- Runnable command shape: confirmed
- Stable artifact paths: confirmed
- Parseable summary file: confirmed
- Dependency shim surface: confirmed and pinned

## Lite Kernel Contract

- Boundary: Lite remains the lightweight, mostly integratable kernel layer for launch/runtime config, managed-server identity, profile resolution primitives, and eval orchestration plumbing.
- Primary `ozone` surface: the shipped `ozone` binary owns the active CLI and TUI workflow, benchmark UX, and higher-level product commands built on that kernel.
- Exposure mode: Lite survives as an internal kernel boundary plus minimal-build distinction, not as a separate Plus-style peer runtime tier.
- Why: this preserves reuse across the family without carrying dead tier-picker complexity or duplicating product behavior.
# Ozone Eval Runner Environment

This directory is the only supported Python runner environment for `ozone eval`.

## Supported environment

- Virtual environment: `contrib/evals/.venv`
- Bootstrap entrypoint: `contrib/evals/bootstrap.sh`
- EvalPlus requirements: `contrib/evals/requirements-evalplus.txt`
- lm-eval requirements: `contrib/evals/requirements-lm-eval.txt`

Do not use ad-hoc user-global installs, Poetry, uv, Conda, or system-package Python for runner execution. If the `.venv` is missing or out of date, recreate it with `bootstrap.sh`.

## Verified local endpoint contract

- Health: `http://127.0.0.1:8989/health`
- Models: `http://127.0.0.1:8989/v1/models`
- EvalPlus base URL: `http://127.0.0.1:8989/v1`
- lm-eval local-completions base URL: `http://127.0.0.1:8989/v1/completions`

## Verified working harness commands

### lm-eval GSM8K probe

```bash
OPENAI_API_KEY=none \
contrib/evals/.venv/bin/lm-eval run \
  --model local-completions \
  --model_args model=gemma-4-E4B-it-UD-Q8_K_XL.gguf,base_url=http://127.0.0.1:8989/v1/completions,tokenizer_backend=None \
  --tasks gsm8k \
  --limit 1 \
  --output_path results/lm_eval_gsm8k_probe
```

### lm-eval instruction-following probe

```bash
OPENAI_API_KEY=none \
contrib/evals/.venv/bin/lm-eval run \
  --model local-completions \
  --model_args model=gemma-4-E4B-it-UD-Q8_K_XL.gguf,base_url=http://127.0.0.1:8989/v1/completions,tokenizer_backend=None \
  --tasks leaderboard_instruction_following \
  --limit 1 \
  --output_path results/lm_eval_instruction_probe
```

### lm-eval math probe

```bash
OPENAI_API_KEY=none \
contrib/evals/.venv/bin/lm-eval run \
  --model local-completions \
  --model_args model=gemma-4-E4B-it-UD-Q8_K_XL.gguf,base_url=http://127.0.0.1:8989/v1/completions,tokenizer_backend=None \
  --tasks leaderboard_math_hard \
  --limit 1 \
  --output_path results/lm_eval_math_probe
```

### EvalPlus codegen probe

```bash
OPENAI_API_KEY=none \
contrib/evals/.venv/bin/evalplus.codegen \
  gemma-4-E4B-it-UD-Q8_K_XL.gguf humaneval \
  --backend openai \
  --base_url http://127.0.0.1:8989/v1 \
  --root results/evalplus_probe \
  --n_samples 1 \
  --temperature 0.0 \
  --greedy \
  --id_range '[0,1]'
```

## Known compatibility notes

- `lm-eval` local OpenAI-compatible runs need `tokenizer_backend=None` for this llama.cpp server because remote tokenizer endpoints are not exposed.
- `leaderboard_instruction_following` also requires NLTK `punkt_tab` data. `bootstrap.sh` downloads it.
- `evalplus.evaluate` expects a complete sample set for the selected split. A narrow `id_range` codegen subset is useful for probe generation, but subset evaluation fails with `Missing problems in samples`.

## Artifact expectations

- lm-eval writes timestamped result JSON under `results/<probe>/<model>/`.
- EvalPlus codegen writes:
  - sanitized JSONL: `.../<dataset>/<model>_openai_temp_0.0.jsonl`
  - raw JSONL: `.../<dataset>/<model>_openai_temp_0.0.raw.jsonl`
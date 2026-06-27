# Ozone

**Use local AI smarter. Not bigger.**

Ozone is a terminal-native tool for profiling local LLM model files and runtime
configs. It helps answer a practical question:

```text
Given this exact model, quant, context length, KV quant, backend, sampler, and
hardware setup, what is this configuration actually useful for?
```

Ozone is being prepared for an RC around model launch, hardware-aware profiling,
benchmarking, sweeps, analysis, and capability evaluation. The former ozone+
chat shell is deprecated and archived under `docs/archive/ozone-plus/`.

## Current Scope

Active RC scope:

- launch and monitor a managed llama.cpp server
- inventory, import, link, and inspect local GGUF models
- profile model/hardware fit and produce launch profiles
- benchmark and sweep runtime settings
- run native capability evaluations with warm-up, calibration, health gates,
  canary tasks, lane-specific suites, artifacts, and CSV/report output
- show clear TUI status for what passed, failed, skipped, and why

Out of scope for RC:

- chat, roleplay, character cards, memories, branches, swipes, or transcripts
- ozone+ as a shipping end-user binary
- cloud-only benchmark flows
- full SWE-bench or Terminal-Bench by default
- one global score as the main result

## Install

```bash
git clone https://github.com/EricA1019/ozone.git
cd ozone
./contrib/sync-local-install.sh
```

The sync helper builds the active release binary and installs it as `oz` in
`~/.cargo/bin` and `~/.local/bin` when the checksum changed.

Build manually:

```bash
cargo build --release -p ozone --features full
```

Quick alias update (rebuild + install to `~/.local/bin/oz`):

```bash
make update-oz      # from the project directory
oz-update           # from anywhere (after adding alias to .zshrc)
```

During development:

```bash
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

## Backend

Ozone's active managed backend path is llama.cpp.

Install `llama-server` and `llama-cli` from a release, package manager, or
source build, then confirm both are available:

```bash
llama-server --version
llama-cli --version
```

If they are not on `PATH`, set:

```bash
OZONE_LLAMACPP_SERVER=/path/to/llama-server
OZONE_LLAMACPP_CLI=/path/to/llama-cli
```

Ozone launches managed llama.cpp on `http://127.0.0.1:8989`.

## Models

By default Ozone looks for `.gguf` files in `~/models/`. Override this with the
Settings screen or:

```bash
OZONE_MODELS_DIR=/path/to/models
```

Model commands:

```bash
oz model list
oz model list --json
oz model info <model>.gguf
oz model add --hf <repo> [filename.gguf]
oz model add --ollama <model-name>
oz model add --link /path/to/model.gguf
oz model remove <model>.gguf
```

Broken symlinks appear as broken entries in `oz model list` and in the model
picker. Ozone reports them instead of crashing.

## TUI Navigation

The terminal launcher supports keyboard-driven navigation across all screens:

| Key | Action |
|-----|--------|
| `j` / `k` or `↑` / `↓` | Navigate lists |
| `Enter` | Select / confirm |
| `Esc` | Go back |
| `q` | Quit |
| `/` | Open command palette |
| `Tab` | Switch focus (where supported) |

The launcher shows a hint bar at the bottom of each screen with available keys.
Press `?` for help on supported screens.

Three theme presets are available: **DarkMint** (default), **OzoneDark** (original),
and **HighContrast** (accessibility). Configure in the Settings screen.

## Main Commands

```bash
oz                  # open the TUI launcher
oz --mode=lite      # select the lightweight backend-control tier
oz --mode=base      # select the profiling/eval tier
oz --pick           # show the tier picker again
oz monitor          # live monitor dashboard
oz clear            # stop the managed llama.cpp backend
oz purge-last-model # stop tracked managed llama.cpp state
```

Feature commands in the full build:

```bash
oz bench <model>
oz sweep <model>
oz analyze
oz profiles
```

The legacy `oz list` command still works as a lightweight catalog view, but
`oz model list` is the canonical model inventory command.

## Evaluation

Evals run against a running llama.cpp server on `http://127.0.0.1:8989`. The eval
launcher checks server health before running and offers to launch a model if none
is running.

### Sweep Levels

| Level | Scope | ~Tasks |
|-------|-------|--------|
| Quick | Health + canary gates | ~17 |
| Standard | Quick + code micro | ~21 |
| Full | All 5 suites | ~36 |

### All Registered Evals

Each eval appears in the launcher with a category bracket and description:

| Eval | Category | Framework | What It Tests |
|------|----------|-----------|---------------|
| **Native Pipeline** | `[Sweep]` | ozone | Health gates, canary, code, format, math (36 tasks in Full) |
| **GSM8K** | `[Math]` | lm-eval | Grade-school arithmetic word problems |
| **Math Hard** | `[Math]` | lm-eval | Competition-level problem solving |
| **MMLU** | `[Reasoning]` | lm-eval | Multi-subject QA across 57 academic domains |
| **BBH** | `[Reasoning]` | lm-eval | Multi-step logic across 23 hard tasks |
| **HumanEval** | `[Code]` | EvalPlus | Python function completion (164 problems) |
| **Instruction** | `[Follow]` | lm-eval | Multi-constraint instruction adherence |
| **TruthfulQA** | `[Safety]` | lm-eval | Factual accuracy & misconception resistance |
| **HellaSwag** | `[Safety]` | lm-eval | Commonsense reasoning & adversarial filtering |
| **Creative Writing** | `[Creative]` | ozone | Diversity & coherence in long-form generation |

### Error Handling

- **Server check**: If no server is running, the eval launcher shows a helpful
  message instead of cryptic backend errors.
- **Error logs**: All eval failures are written to `results/logs/eval-errors.log`
  with timestamps and model name.
- **Recovery**: On failure, the UI returns to the Bench+Eval screen so you can
  adjust and retry.

### Policies

- quality evaluation starts at 16k context by default
- warm-up generations are discarded and never scored
- calibration health gates run before expensive suites
- tasks must fit the configured context before running
- skipped work must be recorded with a reason
- CSV and reports are human-facing views, not the only source of truth

## Data Locations

| Path | Contents |
| --- | --- |
| `~/.local/share/ozone/` | preferences, launch state, benchmark data, logs |
| `~/models/` or `OZONE_MODELS_DIR` | GGUF model library and symlinks |
| `results/` | eval artifacts, generated reports, and error logs |
| `results/logs/` | timestamped eval error logs |
| `docs/archive/ozone-plus/` | deprecated chat documentation |

## Developer Automation

`crates/ozone-mcp` contains developer automation for repo-aware workflows and
smoke testing. It is not part of the end-user launcher flow and still contains
some archived ozone+ helpers that should not define RC product scope.

## Troubleshooting

### No models found

Check the active model directory:

```bash
oz model list
```

If your models live elsewhere, set `OZONE_MODELS_DIR` or update the launcher
Settings screen.

### llama.cpp does not launch

Confirm the server binary is visible:

```bash
llama-server --version
OZONE_LLAMACPP_SERVER=/path/to/llama-server oz
```

### Hugging Face import fails

The HF import path uses `llama-cli`:

```bash
llama-cli --version
OZONE_LLAMACPP_CLI=/path/to/llama-cli oz model add --hf <repo> <filename>.gguf
```

### Managed server is still running

Use:

```bash
oz clear
```

This stops only Ozone-managed llama.cpp state or a strict managed-port match. It
does not kill unrelated user-managed backend processes.

## Requirements

- Linux, tested on Ubuntu 24.04
- Rust stable toolchain for source builds
- llama.cpp `llama-server` and `llama-cli`
- NVIDIA GPU with `nvidia-smi` for best profiling data; CPU-only operation is
  supported but slower

## License

MIT

Contact: [ScribeALB@proton.me](mailto:ScribeALB@proton.me)

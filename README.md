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

The authoritative RC scope is tracked in `docs/RC_SCOPE.md`.

## Install

```bash
git clone https://github.com/EricA1019/ozone.git
cd ozone
./contrib/sync-local-install.sh
```

The sync helper builds the active release artifacts and installs `ozone`, `oz`,
and `ozone-mcp` in `~/.cargo/bin` and `~/.local/bin` when checksums changed.

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

Additional Makefile targets:

```bash
make doc              # build documentation with private items
make feature-matrix   # print feature gate counts per module
make check-all        # check all feature permutations
make outdated         # list outdated dependencies (requires cargo-outdated)
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

The terminal launcher shows a **context-sensitive hint bar** at the bottom of
each screen with available keys for the current context:

| Screen | Keys |
|--------|------|
| Launcher | `↑↓/jk` navigate, `Enter` select, `m` model picker, `p` profile, `Esc` back, `q` quit, `/` command |
| Model Picker | `↑↓/jk` navigate, `Enter` select, `Esc` back, `/` filter |
| Bench+Eval | `↑↓/jk` navigate, `Enter` run, `Esc` back, `r` refresh results |
| Confirm | `Enter` confirm, `Esc` back, `c` configure |
| Monitor | `q/Esc` exit, `s` clear & exit |

Three theme presets are available: **DarkMint** (default), **OzoneDark** (original),
and **HighContrast** (accessibility). Configure in the Settings screen or override
colors at runtime by creating a `theme.toml` in the data directory:

```toml
# ~/.local/share/ozone/theme.toml
lime = "#45AF82"
cyan = "#4ED2A5"
violet = "#643AC8"
gray = "#647373"
green = "#22C55E"
red = "#EF4444"
```

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

Evals are grouped by category in the Bench+Eval launcher for easier navigation:

| Category | Evals | Framework |
|----------|-------|-----------|
| **Standard Benchmarks** | GSM8K, MATH, HumanEval, MBPP, IFEval | lm-eval, EvalPlus |
| **Knowledge** | MMLU, MMLU-Pro, TruthfulQA, DROP | lm-eval |
| **Reasoning** | BBH, ARC-Challenge, HellaSwag, BBH Causal Judgement | lm-eval |
| **Safety & Ethics** | MMLU Philosophy, Hendrycks Ethics, BBH Formal Fallacies | lm-eval |
| **Hard (opt-in)** | GPQA | lm-eval |

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
| `docs/archive/` | archived ozone+ chat documentation (INDEX.md) |

## Developer Automation

`crates/ozone-mcp` contains developer automation for repo-aware workflows and
smoke testing. It is not part of the end-user launcher flow and still contains
some archived ozone+ helpers that should not define RC product scope.
By default MCP `tools/list` exposes only active RC tools. Set
`OZONE_MCP_ENABLE_LEGACY_TOOLS=1` only when you intentionally need archived
ozone+ automation during migration or forensic testing.

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

```
 ██████  ███████  ██████  ███    ██ ███████
██    ██    ███  ██    ██ ████   ██ ██
██    ██   ███   ██    ██ ██ ██  ██ █████
██    ██  ███    ██    ██ ██  ██ ██ ██
 ██████  ███████  ██████  ██   ████ ███████
```

**⬡ local AI stack operator**

Configure, launch, and monitor [KoboldCpp](https://github.com/LostRuins/koboldcpp) + [SillyTavern](https://github.com/SillyTavern/SillyTavern) from a single terminal command. Ozone reads your hardware at startup, picks a layer split that fits in available VRAM (spilling to RAM if needed), then gets out of the way.

Built in Rust. Single binary. No Node.js, no Python, no daemon.

---

## What it does

- Splash screen with live VRAM and RAM gauges on startup
- Scrollable model list — each entry shows size, source (`Tuned` / `Bench` / `Heur`), and a fit indicator
- Hardware-aware planner: checks free VRAM, computes GPU layer count, falls back to mixed-memory or CPU-only automatically
- Confirm screen shows the exact KoboldCpp flags before anything launches
- Launches KoboldCpp, waits for the API to come up, then opens SillyTavern in a browser app window
- Live monitor screen: VRAM %, RAM %, disk I/O sparkline, detected token/s
- `ozone clear` stops KoboldCpp and any Ollama runner processes and frees VRAM

---

## Requirements

- Linux (tested on Ubuntu 24.04)
- NVIDIA GPU — `nvidia-smi` must be in `$PATH`
- [KoboldCpp](https://github.com/LostRuins/koboldcpp) binary at `~/koboldcpp/koboldcpp`
- A launch wrapper script at `~/models/launch-koboldcpp.sh` (see [Setup](#setup))
- [SillyTavern](https://github.com/SillyTavern/SillyTavern) running or launchable on `localhost:8000`
- Rust toolchain (`cargo`) for building

Models are read from `~/models/`. Symlinks work — if you use Ollama, symlink the `.gguf` blobs into `~/models/`.

---

## Install

```bash
git clone https://github.com/EricA1019/ozone.git
cd ozone
cargo build --release
cp target/release/ozone ~/.local/bin/    # or anywhere on $PATH
```

---

## Setup

### Launch script

Ozone delegates to `~/models/launch-koboldcpp.sh` when starting KoboldCpp. This script receives the model path and any flags Ozone computed (e.g. `--gpulayers 28 --contextsize 8192 --quantkv 1`). A minimal version:

```bash
#!/usr/bin/env bash
KCPP="$HOME/koboldcpp/koboldcpp"
exec "$KCPP" --model "$1" --usecuda "${@:2}"
```

A fuller version that handles model-size categories and preset files is included in `contrib/launch-koboldcpp.sh`.

### Preset file

Create `~/models/koboldcpp-presets.conf` to lock specific settings per model. Presets override all heuristics.

```
# filename | gpu_layers | context_size | quant_kv | note
my-model-7b.gguf      | -1 | 32768 | 1 | Full VRAM, 32K context
big-model-13b.gguf    | 28 | 8192  | 2 | Mixed memory — 28 of 40 layers on GPU
huge-model-30b.gguf   |  0 | 4096  | 1 | CPU-only
```

| Field | Values | Notes |
|---|---|---|
| `gpu_layers` | `-1` = all layers, `0` = CPU-only, `N` = pin N layers | Split point between VRAM and RAM |
| `context_size` | `2048` – `131072` | KV cache context window |
| `quant_kv` | `1` – `3` | KV cache quantization — higher = less VRAM, less precision |
| `note` | any text | Shown on the confirm screen |

### Benchmark file

After a model has run well, record the result in `~/models/bench-results.txt`. Ozone uses these as a secondary source, ranked below Tuned presets and above heuristics.

```
---
model: my-model-7b.gguf
context: 32768
gen_speed: 14.2
gpu_layers: -1
quant_kv: 1
vram_mb: 7800
size_gb: 7.0
```

---

## Usage

```bash
ozone                   # interactive launcher (default)
ozone monitor           # live monitor dashboard
ozone clear             # stop KoboldCpp + Ollama runner, free VRAM
ozone list              # list models with size and VRAM estimate
ozone list --json       # machine-readable output
ozone --no-browser      # launch without opening the browser
```

### Launcher keys

| Key | Action |
|---|---|
| `↑` `↓` | Navigate |
| `Enter` | Select |
| `Esc` | Back / cancel |
| `q` | Quit |

### Monitor keys

| Key | Action |
|---|---|
| `s` | Return to launcher |
| `r` | Force refresh |
| `q` / `Esc` | Quit |

---

## Hardware modes

The planner outputs one of three modes for each launch:

| Mode | What it means |
|---|---|
| **VRAM** | All model layers fit in GPU memory |
| **Mixed** | Layers are split — part on GPU, part spilling to system RAM |
| **CPU** | No GPU offload; all layers run on CPU |

For models with no preset or benchmark (`Heur` source), Ozone checks free VRAM at launch time and steps down the layer count until the estimate fits. For `Tuned` and `Bench` sources, it uses the stored values as-is.

The confirm screen shows the resolved plan before anything launches:

```
 Model    mn-12b-mag-mell-r1.gguf
 Mode     VRAM  (est. 8832 MiB)
 Context  28672
 Layers   -1 / 32
 QuantKV  1
 Source   Tuned — high-context preset (>=10 T/s)

 Launch?  [y] yes   [n] cancel
```

---

## Data locations

| Path | Contents |
|---|---|
| `~/.local/share/ozone/preferences.json` | Last-used model, context size, GPU layers |
| `~/.local/share/ozone/koboldcpp.log` | KoboldCpp stdout / stderr |

---

## Tested on

- RTX 3060 12 GB VRAM + 32 GB RAM
- Ubuntu 24.04, KoboldCpp 1.111+, SillyTavern (latest)
- Models from 7 B to 30 B parameters

---

## License

MIT

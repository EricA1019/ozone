```
 ██████  ███████  ██████  ███    ██ ███████
██    ██    ███  ██    ██ ████   ██ ██
██    ██   ███   ██    ██ ██ ██  ██ █████
██    ██  ███    ██    ██ ██  ██ ██ ██
 ██████  ███████  ██████  ██   ████ ███████
```

**⬡ Use AI smarter. Not bigger.**

![Version](https://img.shields.io/badge/v0.4.0--alpha-a8e600?style=for-the-badge)
![License](https://img.shields.io/badge/MIT-7c3aed?style=for-the-badge)
![Local-first](https://img.shields.io/badge/local--first-06b6d4?style=for-the-badge)

---

## ⬡ The Ozone Family

One codebase, three tiers. Choose your level:

| Tier | What it does | Who it's for |
|---|---|---|
| **ozonelite** | Launch + monitor only | Minimal footprint, SSH boxes, power users |
| **ozone** | Bench · Sweep · Analyze | Repeatable tuning and hardware-aware launch workflows |
| **ozone+** | Chat shell with memory & sessions | Full local-LLM chat experience in one TUI |

```bash
ozone --mode=lite   # ozonelite mode
ozone               # ozone base mode  
ozone --mode=plus   # ozone+ mode
ozone --pick        # force tier picker
```

Binary name detection: `ozone-lite`, `ozone+`, `ozoneplus` auto-select their tier.

---

## ⬡ Quick Start

```bash
git clone https://github.com/EricA1019/ozone.git
cd ozone
cargo build --workspace --release

# Install both binaries
cp target/release/ozone ~/.local/bin/
cp target/release/ozone-plus ~/.local/bin/

# Run
ozone              # launches tier picker on first run
ozone-plus         # direct to chat shell
```

---

## ⬡ What Ozone Does

**All tiers:**
- Splash screen with live VRAM/RAM gauges
- Hardware-aware model recommendations
- KoboldCpp + Ollama backend support

**ozone (base):**
- `ozone bench` — measure model/settings combinations
- `ozone sweep` — explore context/quantization space
- `ozone analyze` — surface good configs, generate profiles
- `ozone analyze --export` — write profiles to `koboldcpp-presets.conf`
- Live monitor: VRAM %, RAM %, disk I/O sparkline, token/s

**ozone+:**
- Full chat TUI with conversations and characters
- Persistent memory with semantic recall
- Session isolation — each session has its own context
- Export conversations to Markdown

---

## ⬡ Requirements

- Linux (tested on Ubuntu 24.04)
- NVIDIA GPU with `nvidia-smi` in `$PATH`
- [KoboldCpp](https://github.com/LostRuins/koboldcpp) at `~/koboldcpp/koboldcpp`
- Launch wrapper at `~/models/launch-koboldcpp.sh` (see below)
- Models in `~/models/` (symlinks work for Ollama blobs)
- Rust toolchain for building

---

## ⬡ Setup

### Launch Script

Create `~/models/launch-koboldcpp.sh`:

```bash
#!/usr/bin/env bash
KCPP="$HOME/koboldcpp/koboldcpp"
exec "$KCPP" --model "$1" --usecuda "${@:2}"
```

### Preset File (Optional)

Create `~/models/koboldcpp-presets.conf` to lock specific settings:

```
# filename | gpu_layers | context_size | quant_kv | note
my-model-7b.gguf      | -1 | 32768 | 1 | Full VRAM, 32K context
big-model-13b.gguf    | 28 | 8192  | 2 | Mixed memory — 28 of 40 layers on GPU
```

---

## ⬡ Commands

```bash
ozone                   # interactive launcher (default)
ozone monitor           # live monitor dashboard
ozone clear             # stop KoboldCpp + Ollama, free VRAM
ozone list              # list models with VRAM estimates
ozone list --json       # machine-readable output
ozone bench <model>     # benchmark specific model
ozone sweep <model>     # parameter sweep
ozone analyze           # review benchmark history
ozone analyze --export  # write profiles to presets file
```

### Launcher Keys

| Key | Action |
|---|---|
| `↑` `↓` | Navigate |
| `Enter` | Select |
| `Esc` | Back |
| `q` | Quit |

---

## ⬡ Data Locations

| Path | Contents |
|---|---|
| `~/.local/share/ozone/` | Preferences, logs |
| `~/.local/share/ozone-plus/` | Sessions, memory index |
| `~/models/` | Model files, presets, benchmarks |

---

## ⬡ Tested On

- RTX 3060 12 GB VRAM + 32 GB RAM
- Ubuntu 24.04, KoboldCpp 1.111+
- Models 7B – 30B parameters

---

## ⬡ License

MIT

**Contact:** ScribeALB@proton.me

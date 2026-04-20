```
 ██████  ███████  ██████  ███    ██ ███████
██    ██    ███  ██    ██ ████   ██ ██
██    ██   ███   ██    ██ ██ ██  ██ █████
██    ██  ███    ██    ██ ██  ██ ██ ██
 ██████  ███████  ██████  ██   ████ ███████
```

**⬡ Use AI smarter. Not bigger.**

![Version](https://img.shields.io/badge/v0.4.5--alpha-2daf82?style=for-the-badge)
![License](https://img.shields.io/badge/MIT-7c3aed?style=for-the-badge)
![Local-first](https://img.shields.io/badge/local--first-06b6d4?style=for-the-badge)

---

## ⬡ Philosophy

Ozone is built on one conviction: **you don't need bigger hardware — you need smarter tooling.**

Local AI users are routinely told to buy more VRAM, download bigger models, and accept opaque cloud services. Ozone pushes back on that. It is a tooling family built around four principles:

- **Local-first** — your models, your hardware, your data, no cloud account required
- **Efficiency over brute force** — intelligent layer splitting and profiling let smaller hardware punch above its weight
- **Transparency** — every recommendation is explainable; you see what's happening and why
- **User agency** — no hidden magic, no opaque scoring, every decision is yours to override

---

## ⬡ The Ozone Family

One codebase, three tiers. Choose your level of engagement:

| Tier | Purpose | Best for |
|---|---|---|
| **⬡ ozonelite** | Lean backend launcher | Minimal footprint, SSH boxes, direct-control users |
| **⬡ ozone** | Autoprofile · Bench · Sweep · Analyze | Hardware optimization and repeatable tuning workflows |
| **⬡ ozone+** | Chat shell with memory & sessions | Full local-LLM conversation experience |

All three ship as a single binary:

```bash
ozone --mode=lite   # ozonelite — launch/monitor only
ozone               # ozone base — launch + profiling
ozone --mode=plus   # ozone+ — full chat TUI
ozone --pick        # force tier picker
```

Binary name detection: symlinking to `ozone-lite`, `ozone+`, or `oz+` auto-selects the tier.

**Choose ozonelite if** you want the smallest possible footprint, you already know your backend settings, or you're running on a constrained or remote system.

**Choose ozone if** you want benchmark-driven tuning, autoprofiling to find a starting layer split, or reusable profiles across sessions.

**Choose ozone+ if** you want a full local-LLM chat experience — persistent sessions, pinned memory, cross-session search, character cards, and branches — all in a single TUI.

---

## ⬡ Quick Start

### 1. Install a Local Backend

Ozone supports both [KoboldCpp](https://github.com/LostRuins/koboldcpp) and [llama.cpp](https://github.com/ggml-org/llama.cpp) as first-class backends. Both backends support the full profiling workflow (QuickSweep, FullSweep, SingleBenchmark, GenerateProfiles) and are interchangeable for ozone+ chat sessions.

**KoboldCpp setup**

**Option A — prebuilt release (recommended for most users):**

```bash
mkdir -p ~/koboldcpp
cd ~/koboldcpp
# Download the latest release for your platform from:
# https://github.com/LostRuins/koboldcpp/releases
# Pick the CUDA build if you have an NVIDIA GPU, otherwise the CPU build.
chmod +x koboldcpp
```

**Option B — source build (required for CUDA on some systems):**

```bash
git clone https://github.com/LostRuins/koboldcpp.git ~/koboldcpp
cd ~/koboldcpp
make LLAMA_CUBLAS=1    # with NVIDIA CUDA
# or: make             # CPU-only build
```

The binary must exist at `~/koboldcpp/koboldcpp`. To use a different path, set `OZONE_KOBOLDCPP_LAUNCHER` (see [Environment Variables](#-environment-variables)).

**llama.cpp setup**

Install `llama-server` and `llama-cli` from a release, package manager, or source build, then make sure both binaries are on your `PATH`.

```bash
llama-server --version
llama-cli --version
```

If your install lives outside `PATH`, set `OZONE_LLAMACPP_SERVER` and/or `OZONE_LLAMACPP_CLI`.

### 2. Set Up the Launch Wrapper

Create `~/models/launch-koboldcpp.sh`:

```bash
#!/usr/bin/env bash
KCPP="$HOME/koboldcpp/koboldcpp"
exec "$KCPP" --model "$1" --usecuda "${@:2}"
```

```bash
chmod +x ~/models/launch-koboldcpp.sh
```

For CPU-only systems, omit `--usecuda`:

```bash
exec "$KCPP" --model "$1" "${@:2}"
```

### 3. Get Models

Ozone looks for `.gguf` files in `~/models/`. GGUF is the standard format for local quantized models.

**Download from HuggingFace via llama.cpp**:

```bash
ozone model add --hf <repo-id> [filename.gguf]
# Example: ozone model add --hf ggml-org/gemma-3-1b-it-GGUF gemma-3-1b-it-Q4_K_M.gguf
```

This uses llama.cpp's built-in Hugging Face downloader, then symlinks the resolved GGUF from the HF cache into `~/models/` so `ozone model list`, the launcher picker, and `ozone model info` keep working as before.

**Use Ollama models** (Ollama must be installed and the model pulled):

```bash
# Find the Ollama blob path
ollama show --modelfile <model-name> | grep FROM
# Symlink into ~/models/
ln -s /path/to/ollama/blob ~/models/<model-name>.gguf
```

> Note: Broken symlinks appear in the model picker as "issue report" entries and in `ozone model list` as `⚠ broken` rows. This is expected behavior, not a crash. Ozone reports the broken path so you can fix it.

### 4. Build and Install Ozone

```bash
git clone https://github.com/EricA1019/ozone.git
cd ozone
./contrib/sync-local-install.sh
```

This helper builds the current release artifacts for `ozone`, `ozone-plus`, and
`ozone-mcp`, then updates both `~/.cargo/bin` and `~/.local/bin` only when the
SHA-256 checksum changed.

Once you've synced from a repo once, launching an installed `ozone` or
`ozone-plus` binary will notice when that repo's `target/release` artifact is
newer than the installed copy and prompt:

```text
Update installed binaries now? [Y/n]
```

### 5. First Run

```bash
ozone
```

On first launch, the **tier picker** appears. Use `↑↓` to move between the three tiers and `Enter` to select. Your choice is saved. You can always change it via `ozone --pick` or the Settings screen inside the launcher.

After selecting a tier, the **splash screen** loads your hardware stats (VRAM, RAM). Press `Enter` to continue to the main launcher.

### 6. Manage Models

The base `ozone` binary now includes first-class local model management:

```bash
ozone model list
ozone model list --json
ozone model info <model>.gguf
ozone model add --hf <repo> [filename.gguf]
ozone model add --ollama <model-name>
ozone model add --link /path/to/model.gguf
ozone model remove <model>.gguf
```

`ozone model list` is now the canonical model-inventory command. The older `ozone list` view still works as a lightweight catalog output, but it is deprecated in favor of `ozone model list`.

### 7. Developer automation with `ozone-mcp` (optional)

`ozone-mcp` is a developer-facing stdio MCP server for repo-aware automation and ozone+/launcher smoke tests. It is not part of the end-user launcher flow.

```bash
ozone-mcp
# or, from the workspace:
cargo run -p ozone-mcp-app --bin ozone-mcp
```

First-cut tools include:

- workspace/repo ops: `workspace_status`, `cargo_tool`, `catalog_list`, `preferences_get`
- ozone+ app-aware ops: `session_tool`, `message_tool`, `memory_tool`, `search_tool`, `branch_tool`, `swipe_tool`, `export_tool`, `import_card`
- smoke/helpers: `sandbox_tool`, `mock_backend_tool`, `launcher_smoke`, `mock_user_tool`

The server prefers direct crate APIs for persistence-heavy session work and uses explicit subprocess wrappers only for seams that still live in the end-user CLIs, such as runtime-backed `send`, `search`, and launcher PTY smoke flows.

`mock_user_tool` is the front-door layer: it launches the real terminal binaries in a PTY and plays named scripted journeys like a user would, using only keys/text plus visible terminal markers instead of repo/API back doors.

---

## ⬡ ozone — Autoprofiling

Autoprofiling is the key feature that separates ozone base from just launching a backend manually. It reads your model, your hardware, and produces a concrete starting point for manual tuning. Both **KoboldCpp** and **llama.cpp** backends are fully supported — ozone auto-detects which backend to use, preferring KoboldCpp when both are available.

### How it works

From the launcher main menu, choose **Profile**. Ozone will:

1. Read your model's GGUF metadata to find the real layer count (e.g., 56 layers for a 22B model)
2. Calculate how many layers fit in your current free VRAM at the model's quantization level
3. Show you a **layer split advisory**: `GPU 42/56 · CPU 14`
4. Estimate RAM pressure for the CPU-resident layers
5. Let you run a single benchmark or a full sweep from that starting point

### Reading the advisory

```
Model: Pantheon-RP-Pure-22B-Q4_K_M.gguf
Layers: 56 total
  GPU: 42 layers (fits within 10.2 GB free VRAM)
  CPU: 14 layers (~3.1 GB RAM)
Mode: Mixed memory — GPU primary
```

**GPU layers** are fast. **CPU-resident layers** are slow but let you run models that exceed your VRAM. A 22B model with 12 GB VRAM might only fit 42 of 56 layers on the GPU — the remaining 14 layers run in RAM. That's still faster than running entirely on CPU.

Autoprofiling gives you a practical *starting point*, not a guaranteed optimal configuration. Use it to seed your first benchmark, then adjust `gpu_layers` up or down based on results.

### CPU-only mode

For models too large to fit any layers on GPU, or on CPU-only systems:

From the profiling advisory screen, ozone will detect when the recommended split is 0 GPU layers and automatically suggest CPU-only mode. KoboldCpp runs with `--usecpu` instead of `--usecuda`.

CPU-only inference is slow (seconds per token for large models) but fully functional. Use the smallest model that meets your quality needs.

### Commands

```bash
ozone bench <model>     # benchmark a specific model/settings combination
ozone sweep <model>     # explore context/quantization space
ozone analyze           # review benchmark history and surface good configs
ozone analyze --export  # write top configs to koboldcpp-presets.conf
ozone clear             # stop GPU backends / runner processes
```

### Presets (optional)

Create `~/models/koboldcpp-presets.conf` to lock in tested settings:

```
# filename | gpu_layers | context_size | quant_kv | note
my-model-7b.gguf      | -1 | 32768 | 1 | Full VRAM, 32K context
big-model-22b.gguf    | 42 | 8192  | 1 | Mixed memory — 42/56 on GPU
```

---

## ⬡ ozone+ — Chat Shell

ozone+ is a full terminal chat application built for local LLM workflows. It is persistent-first: your conversations, memories, and characters survive restarts.

### Sessions

```bash
ozone-plus create "Session name"   # create a new session
ozone-plus list                    # list all sessions
ozone-plus open <session-id>       # open a session in the TUI
```

Or use the ozone launcher: from the main menu choose **Open ozone+** to jump directly into the chat shell.

If you prefer a short shell symlink, pointing `oz+` at the `ozone` binary also selects the ozone+ tier automatically.

### TUI Keyboard Reference

| Key | Action |
|---|---|
| `Enter` | Send message |
| `Esc` | Normal mode / cancel |
| `i` | Insert mode (start typing) |
| `q` | Quit session |
| `↑` `↓` | Scroll transcript |
| `PgUp` `PgDn` | Scroll long lists (model picker, session list) |
| `/` | Open slash-command autocomplete popup |
| `Ctrl+K` | Pin selected message to memory |
| `Ctrl+D` | Dry-run context preview (see what goes into the prompt) |
| `?` | Help overlay |
| `Tab` | Cycle inspector pane |

### Slash Commands

Type these in the input box:

| Command | Effect |
|---|---|
| `/memory pin <text>` | Pin a freeform fact to persistent memory |
| `/memory note <text>` | Create a keyword note memory |
| `/memory list` | Show active pinned memories |
| `/memory unpin <id>` | Remove a pinned memory |
| `/search session <query>` | Full-text search this session's transcript |
| `/search global <query>` | Full-text search across all sessions |
| `/summarize session` | Generate a session synopsis |
| `/summarize chunk` | Summarize the current context window |
| `/thinking immersive` | Show AI thinking blocks inline |
| `/thinking assisted` | Show thinking as a collapsed summary |
| `/thinking debug` | Show raw thinking output |
| `/tierb on` | Enable Tier B assistive features (importance scoring, keyword extraction) |
| `/tierb off` | Disable Tier B |
| `/safemode on` | Disable all Tier B and assistive features |
| `/safemode off` | Re-enable assistive features |
| `/hooks status` | Show loaded shell hooks |
| `/session export` | Export this session to JSON |

### Memory system

ozone+ memory is **explicit and transparent**. Nothing is automatically retrieved without you seeing it.

- **Pinned memories** persist across sessions and are injected into every prompt context. Use them for facts, character details, or world-state you want the model to always know.
- **Note memories** are keyword-tagged notes you can search and retrieve on demand.
- **Session search** finds messages in the current session by keyword.
- **Global search** finds messages across all sessions, with session attribution.

All memories show their **provenance** — where they came from, when they were created, and how long they have been active.

### Character cards

```bash
ozone-plus import-character <card.json>   # import a SillyTavern V2 character card
```

The character card is attached to the session and injected into context. Sessions are isolated — character A in session 1 never leaks into session 2.

### Export

```bash
ozone-plus export <session-id>            # JSON export
ozone-plus export <session-id> --format markdown   # Markdown transcript
```

### Settings

The in-TUI Settings screen (accessible from the ozone launcher main menu) has **interactive entries** as of v0.4.5-alpha:

- **Appearance** — cycle through theme presets: *Dark Mint* (default, `#2DAF82`), *Ozone Dark*, *High Contrast*
- **Launch** — toggle side-by-side monitor mode; toggle inspector-on-start
- **Display** — cycle timestamp style (Relative / Absolute / Off) and message density (Compact / Comfortable)

Press `Enter` on any editable entry to advance its value. Changes persist to `~/.local/share/ozone/prefs.json` immediately.

---

## ⬡ Environment Variables

| Variable | Effect |
|---|---|
| `OZONE_KOBOLDCPP_LAUNCHER` | Override the KoboldCpp executable path. Use this when your KoboldCpp install is not at `~/koboldcpp/koboldcpp`, or when you need to point at a repaired or alternate build. Example: `OZONE_KOBOLDCPP_LAUNCHER=/opt/koboldcpp/koboldcpp ozone` |
| `OZONE_LLAMACPP_SERVER` | Override the `llama-server` executable path used by the base launcher when `Backend = LlamaCpp`. Example: `OZONE_LLAMACPP_SERVER=/opt/llama.cpp/bin/llama-server ozone` |
| `OZONE_LLAMACPP_CLI` | Override the `llama-cli` executable path used by `ozone model add --hf`. Example: `OZONE_LLAMACPP_CLI=/opt/llama.cpp/bin/llama-cli ozone model add --hf ggml-org/gemma-3-1b-it-GGUF` |

---

## ⬡ Data Locations

| Path | Contents |
|---|---|
| `~/.local/share/ozone/` | Preferences, benchmark history, logs |
| `~/.local/share/ozone-plus/` | Sessions, memory index, vector index |
| `~/models/` | Model files, launch wrapper, presets |

---

## ⬡ Troubleshooting

**"Model not found" or broken model picker entries**

Your `~/models/` directory probably contains symlinks pointing to files that no longer exist (common with Ollama after a model is removed). Ozone reports these as `⚠ broken` rows in `ozone model list` and as "issue" entries in the picker rather than silently failing. Check the symlink targets:

```bash
ls -la ~/models/
```

**KoboldCpp not launching**

Check the path: ozone expects `~/koboldcpp/koboldcpp` by default. If your install is elsewhere, set `OZONE_KOBOLDCPP_LAUNCHER`. If the binary exists but fails to start, run it directly to see the error:

```bash
~/koboldcpp/koboldcpp --version
```

**llama.cpp not launching from the base launcher**

Make sure `llama-server` is available on `PATH` or point ozone at it explicitly:

```bash
llama-server --version
OZONE_LLAMACPP_SERVER=/path/to/llama-server ozone
```

**`ozone model add --hf` cannot find llama-cli**

The HF import path now uses llama.cpp instead of `huggingface_hub`. Make sure `llama-cli` is installed and visible:

```bash
llama-cli --version
OZONE_LLAMACPP_CLI=/path/to/llama-cli ozone model add --hf <repo> <filename>.gguf
```

**"VRAM over budget" warning in autoprofiling**

This is expected for large models — it means the recommended GPU layer count exhausts free VRAM. Ozone shows it so you can lower `gpu_layers` manually. Start the benchmark anyway; KoboldCpp will report the actual VRAM used and you can compare.

**Ollama backend still looks active after `ozone clear`**

`ozone clear` stops KoboldCpp and any directly managed Ollama runner subprocesses, but a supervised `ollama serve` daemon may still be listening afterward. Validate the actual process or port state instead of assuming the listener is gone.

**ozone+ chat is slow or tokens appear one at a time with a long gap**

This usually means the model is running in mixed-memory or CPU mode. Use autoprofiling to check whether your GPU layer recommendation fits within VRAM, or try a smaller/more quantized model.

---

## ⬡ Upgrading

```bash
cd ozone-rs
git pull
./contrib/sync-local-install.sh
```

If you rebuild `target/release` later without resyncing the installed binaries,
the installed `ozone` / `ozone-plus` launch path will offer a `Y/n` refresh
prompt automatically.

Check your current version:

```bash
ozone --version
ozone-plus --version
```

See [CHANGELOG.md](CHANGELOG.md) for what changed.

---

## ⬡ Requirements

- Linux (tested on Ubuntu 24.04)
- NVIDIA GPU with `nvidia-smi` in `$PATH` (CPU-only mode works without GPU)
- KoboldCpp and/or Ollama
- Rust toolchain for building

---

## ⬡ Tested On

- RTX 3060 12 GB VRAM + 32 GB RAM
- Ubuntu 24.04, KoboldCpp 1.111+
- Models 7B–30B parameters

---

## ⬡ License

MIT

**Contact:** ScribeALB@proton.me

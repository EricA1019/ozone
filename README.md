# ⬡ Ozone

**Local AI stack operator** — KoboldCpp + SillyTavern TUI launcher

A single-binary terminal tool for managing KoboldCpp and SillyTavern with
hardware-aware mixed-memory recommendations.

## Features

- Branded splash screen with live hardware detection
- VRAM-first / mixed-memory / CPU-only planner
- Scrollable model picker with fit indicators (✓ ~ ✗)
- Live monitor with disk I/O sparkline and token/s display
- Single binary, no Node.js required

## Install

```bash
cargo build --release
cp target/release/ozone ~/.cargo/bin/ozone
```

## Usage

```bash
ozone              # interactive launcher TUI
ozone monitor      # live monitor dashboard
ozone clear        # clear GPU backends
ozone list         # list models
ozone list --json  # machine-readable output
```

## System

Designed for: NVIDIA GPU + large RAM (e.g. RTX 3060 11.5 GB + 31 GB RAM)
Tested on: Linux with KoboldCpp + SillyTavern

# Changelog

All notable changes to Ozone are documented here.

Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)

---

## [Unreleased]

---

## [0.1.0] — 2026-04-12  *(MIT License)*

Initial release. Full Rust rewrite of the original Node.js Ozone TUI.

### Added

- Splash screen with live VRAM and RAM gauges
- Scrollable model picker — reads `~/models/*.gguf`, follows symlinks
- Hardware-aware launch planner: VRAM-first, mixed-memory, and CPU-only modes
- Preset system: `~/models/koboldcpp-presets.conf` (pipe-delimited, per-model settings)
- Benchmark source: `~/models/bench-results.txt` (ranked above heuristics, below presets)
- Confirm screen showing exact KoboldCpp flags before launch
- Launching screen with live log tail from `~/.local/share/ozone/koboldcpp.log`
- Live monitor: VRAM %, RAM %, disk I/O sparkline, token/s display
- `ozone clear` — stops KoboldCpp and Ollama runner processes, frees VRAM
- `ozone list` / `ozone list --json` — model catalog with VRAM estimates
- `ozone monitor` — standalone monitor dashboard
- `--no-browser` flag to skip opening SillyTavern in a browser app window
- Preferences persisted to `~/.local/share/ozone/preferences.json`

### Fixed

- Model sizes showed as 0.0 GB when `~/models/` entries are symlinks — now follows symlinks to real file
- Clear GPU widget showed stale service status after stopping backends — now refreshes immediately
- `ozone clear` silently did nothing due to `ps` padding PIDs with leading spaces — fixed with `.trim()`
- `ozone clear` falsely reported `ollama serve` as stopped when the signal was rejected — now only reports processes that were actually stopped
- `ollama serve` (system daemon) excluded from the clear target list; only `koboldcpp` and `ollama runner` sub-processes are targeted

[Unreleased]: https://github.com/EricA1019/ozone/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/EricA1019/ozone/releases/tag/v0.1.0

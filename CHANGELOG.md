<!-- Versioning: bump PATCH at named sprints; git hash covers small changes; 0.5.x = beta. See .mex/conventions/versioning.md -->

# Changelog

All notable changes to Ozone are documented here.

Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)

---

## [0.4.7-alpha] — ozone-lite Kernel + Branch Setup

### Added
- Cargo feature flags: `default=[]` (lite), `full` (base), with `bench`, `sweep`, `analyze`, `profiling-ui`, `model-mgmt` sub-features
- Heavy modules (`bench`, `sweep`, `analyze`, `profiling`, `model`, `db`) gated behind feature flags — ozone-lite builds lean by default
- `make install-lite` / `make install-base` / `make install-plus` Makefile targets
- GitHub-fetch install flow: tier picker now offers to download ozone-base/ozone-plus from GitHub releases on demand
- `[profile.release-lite]` Cargo profile: `opt-level="z"`, `lto="fat"`, `codegen-units=1`, `strip="symbols"`, `panic="abort"`
- `dev` integration branch created and pushed to origin
- `.mex/conventions/versioning.md` — versioning rules and bump checklist
- Branch workflow table in `.mex/ROUTER.md`

### Changed
- Version skips 0.4.6 (never shipped) — goes directly 0.4.5 → 0.4.7
- `rusqlite` is now an optional dependency, only compiled when `bench`, `analyze`, or `profiling-ui` features are enabled
- Lite build: 23/23 tests pass; Full build: 37/37 tests pass

### Rules
See `.mex/conventions/versioning.md` for when to bump vs. rely on git hash.

---

## [Unreleased]

---

## [0.4.5-alpha] — 2026-04-19

### Fixed
- Settings crash in ozone+: usize underflow when entry list is empty (render.rs)
- Settings crash: out-of-bounds category index (app.rs `current_category()`)
- Settings silent data loss: `"Context"` entries were dropped — now mapped to Model tab
- Session category entries now visible in settings (was missing from enum)

### Changed
- Default theme shifted to **Dark Mint** (`#2DAF82`) — away from blue-leaning teal
- "Open ozone+" launcher action now uses saved side-by-side preference
- Launcher label adapts: shows `[new window]` when side-by-side pref is on

### Added
- **Theme presets**: Dark Mint (default), Ozone Dark, High Contrast — selectable in Settings > Appearance
- **Editable settings**: Settings screen now has interactive Toggle and Cycle entries
  - Appearance: Theme preset cycle
  - Launch: Side-by-side monitor toggle, Inspector-on-start toggle
  - Display: Timestamp style (Relative/Absolute/Off), Message density (Compact/Comfortable)
- New prefs fields: `theme_preset`, `show_inspector`, `timestamp_style`, `message_density`
- `[✓]`/`[ ]` toggle indicators and `< val >` cycle indicators in settings render

---

## [0.4.3-alpha] — TUI QOL · llama.cpp Profiling · Version Hash

### Added

- **TUI scrollable lists**: model picker and session list now use ratatui `ListState` + `Scrollbar`; `PgUp`/`PgDn` navigate long lists
- **Slash autocomplete**: typing `/` in the ozone+ input box opens a popup list of available commands
- **Settings drill-down**: Settings screen now navigates into sub-sections instead of displaying a flat list
- **Spinner**: braille spinner (⠋⠙⠹…) animates in the footer while the model is streaming a reply
- **Message separators**: visual dividers between conversation turns in the ozone+ TUI
- **Colored mode badges**: INS/CMD mode badge now has a colored background (violet for insert mode, amber for command mode)
- **Model Intelligence screen**: new main-menu screen showing hardware-aware model recommendations
- **Side-by-side monitor**: base `ozone` can now launch the TUI alongside the backend monitor
- **1-row footer**: footer compressed from two rows to one, freeing vertical space for the message pane
- **llama.cpp profiler support**: llama.cpp (`llama-server`) can now be profiled with QuickSweep, FullSweep, SingleBenchmark, and GenerateProfiles — full parity with KoboldCpp
- **llama.cpp token rate**: profiler reads `timings.predicted_n / predicted_ms` from the `/completion` response for accurate tok/s
- **llama.cpp profile export**: after profiling, `llamacpp-profiles.sh` (executable) and `llamacpp-profiles.json` are written to `~/.local/share/ozone/`
- **Auto backend detection**: `resolved_backend_for_profiling()` prefers KoboldCpp when available, falls back to llama-server automatically
- **llama.cpp startup diagnostics**: launch failures are now classified — `GgmlAbort`, `CudaOom`, `CudaError`, `ModelLoadFailed`, `MissingSharedLibrary`, `RuntimeCrash`, `Timeout` — each with specific recovery advice
- **Structured launcher args**: profiled settings (`-ngl`, `--ctx-size`, `--threads`) are saved to prefs and applied at next launch
- **Version hash embedding**: `ozone --version` and `ozone-plus --version` now report `0.4.3-alpha+<git-hash>` (e.g. `0.4.3-alpha+7b46264`)
- **`make install`**: new `Makefile` target builds and installs both `ozone` and `ozone-plus` in one command
- **Build hash auto-update**: `build.rs` re-runs on `.git/HEAD` change so the embedded hash stays current without manual bumping
- **Install update prompt**: the stale-install refresh prompt now shows `build-id:` (SHA-256 prefix of the pending artifact) alongside the version string

### Changed

- **Profiling UI copy**: progress messages and labels no longer hardcode "KoboldCpp" — strings are backend-agnostic
- **Version**: bumped to `0.4.3-alpha` across all workspace crates

---

## [0.4.2-alpha] — Quality & Brand Hardening

### Fixed

- **Sub-screen `q` behavior**: pressing `q` on Sessions, Characters, or Settings screens now navigates back to the main menu instead of quitting the application
- **Stale "coming soon" copy**: removed placeholder "coming soon" text from Characters and Settings status lines; screens are fully implemented
- **Stop-token leaks**: stream decoder now filters `<|im_end|>`, `</s>`, `<|eot_id|>` and other stop tokens before they reach the UI

### Added

- **WAL journal mode**: SQLite connections now use Write-Ahead Logging for better concurrent read performance
- **HTTP client pooling**: inference streaming reuses the connection pool instead of creating a new HTTP client per request
- **Session lock auto-release**: `Phase1dRuntime` Drop impl automatically releases session locks on exit (normal, panic, SIGTERM)
- **`--force` flag**: `ozone-plus open --force` unconditionally clears stale session locks

### Changed

- **Version**: bumped to `0.4.2-alpha` across all workspace crates
- **Hint bars**: sub-screen hint bars now show `q back` instead of only `Esc back`, matching actual keybindings
- **Stale lock timeout**: reduced from 60s to 15s for faster recovery after abnormal exits

### Added

- **Autoprofiling** in base Ozone's `Profile` workflow: GGUF-aware GPU/CPU layer recommendations, RAM estimates, and benchmark/sweep seeding now provide a practical starting point for manual layer tweaking — `README.md`, `src/gguf.rs`, `src/planner.rs`, `src/profiling.rs`

### Changed

- **Model inventory UX**: `ozone model list` is now the canonical model-management view, supports `--json`, and clearly surfaces broken symlinks; the older `ozone list` view now shows headers and a deprecation hint — `README.md`, `src/main.rs`, `src/model.rs`
- **Release artifacts**: release builds now strip symbols and use thin LTO by default to keep the shipped binary smaller — `Cargo.toml`
- **Base ozone theme**: the launcher, splash, monitor, and shared secondary copy now use a teal-family palette so text remains readable against the dark background — `src/theme.rs`, `README.md`
- **Version metadata**: the workspace, internal crates, ozone+ app, product-tier labels, and current docs now target `v0.4.1-alpha` as the active development version — `Cargo.toml`, `crates/*/Cargo.toml`, `apps/ozone-plus/Cargo.toml`, `crates/ozone-core/src/lib.rs`, `README.md`, `ozone+/README.md`

### Fixed

- **CLI consistency**: base ozone subcommands now route errors through the shared `✗` formatter, and `ozone analyze <model>` no longer prints duplicate "no benchmarks" blocks — `src/main.rs`, `src/analyze.rs`
- **Model-path validation**: `ozone model info` / `remove` now reject empty or path-like names instead of resolving directories or traversal-like inputs — `src/model.rs`
- **KoboldCpp launch args**: benchmark launches no longer duplicate `--gpulayers` / `--contextsize` / `--quantkv` flags when the wrapper script also injects defaults — `src/bench.rs`, `contrib/launch-koboldcpp.sh`
- **Plus-tier short name**: invoking the base binary through an `oz+` symlink now selects the ozone+ tier like `ozone+` does — `src/main.rs`
- **ozone -> ozone+ handoff**: launcher handoff now opens an actual ozone+ shell session instead of execing the non-interactive `ozone-plus list` command, and it can create a fallback launcher session when none exist yet — `src/ui/mod.rs`, `apps/ozone-plus/src/main.rs`

---

## [0.4.0-alpha] — 2025-07-16  *(MVP — ozone+ memory shell)*

First alpha of ozone+, the chat-first tier of the Ozone family. Local-LLM conversations with persistent memory across sessions.

### Added — ozone+ (chat shell)

- **Session management**: create, open, list, and lock sessions with UUID-based isolation
- **Streaming inference**: real-time token streaming via KoboldCpp or Ollama backends
- **Persistent memory**: pin important messages, create freeform notes, recall with keyword or semantic search
- **Hybrid search**: full-text search (session-scoped and global) with FTS5, plus vector-index semantic recall via usearch
- **Branching**: create named branches in any transcript; switch, list, and manage alternate conversation paths
- **Swipes**: seed multiple candidate replies, group and activate alternatives inline
- **Character cards**: import SillyTavern-format character cards (JSON/PNG) with system prompt extraction
- **Export**: transcript (plain text) and full session snapshot (JSON `ozone-plus.session-export.v1` format)
- **Draft persistence**: unsent text survives session restarts
- **TUI shell**: ratatui-based chat interface with lime-green (#a8e600) brand palette and ⬡ hex identity

### Added — ozone (base launcher)

- **Tier picker**: first-run product selection (ozonelite / ozone / ozone+)
- **`--mode=lite|base|plus`** and **`--pick`** CLI flags for tier override
- **`ozone model` subcommand**: list, add (--hf / --ollama / --link), remove, and info for local .gguf models
- **Brand refresh**: lime-green palette across launcher, monitor, and all UI surfaces

### Changed

- Human-readable timestamps throughout ozone+ (e.g. "2025-07-16 14:30 (2h ago)")
- Updated all stale "Phase 1F/2B" references to reflect shipped MVP state
- Updated all Cargo.toml descriptions to describe actual capabilities (no phase numbers)
- Product tier status labels now show version instead of "Upcoming"/"Current repo"

### Fixed

- FTS search queries are properly escaped via `plain_text_fts_query()` to prevent SQL injection
- Session isolation: memories and search results never bleed across sessions

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

[Unreleased]: https://github.com/EricA1019/ozone/compare/v0.4.0-alpha...HEAD
[0.4.0-alpha]: https://github.com/EricA1019/ozone/compare/v0.1.0...v0.4.0-alpha
[0.1.0]: https://github.com/EricA1019/ozone/releases/tag/v0.1.0

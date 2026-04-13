---
name: stack
description: Technology stack, library choices, and the reasoning behind them. Load when working with specific technologies or making decisions about libraries and tools.
triggers:
  - "library"
  - "package"
  - "dependency"
  - "which tool"
  - "technology"
edges:
  - target: context/decisions.md
    condition: when the reasoning behind a tech choice is needed
  - target: context/conventions.md
    condition: when understanding how to use a technology in this codebase
last_updated: 2026-04-13
---

# Stack

## Core Technologies

- **Rust 2021 edition** — primary language, now organized as a Cargo workspace with the root `ozone` package plus `ozone-core`, `ozone-engine`, `ozone-memory`, `ozone-inference`, `ozone-persist`, `ozone-tui`, and the `ozone-plus` app
- **ratatui 0.29** — TUI framework (crossterm backend)
- **crossterm 0.28** — terminal event handling (key input, raw mode)
- **tokio 1 (full)** — async runtime for HTTP polling and process management
- **clap 4 (derive)** — CLI argument parsing with subcommands
- **SQLite via rusqlite 0.31** — benchmark result storage (bundled, no system dep)

## Key Libraries

- **ratatui** (not tui-rs) — actively maintained fork, used for all rendering
- **reqwest 0.12** — HTTP client for polling KoboldCpp/SillyTavern APIs
- **sysinfo 0.33** — CPU core count and basic system info
- **serde + serde_json** — JSON serialization for preferences and API responses
- **directories 5** — XDG-compliant data/config paths under the user's home directory
- **chrono 0.4** — timestamps in benchmark records and log formatting
- **anyhow** — error handling with context (all public functions return `anyhow::Result`)
- **libc 0.2** — low-level process signaling (kill syscall)
- **Cargo workspaces** — the repo now uses a shared workspace layout so the root `ozone` app plus `ozone-core`, `ozone-engine`, `ozone-memory`, `ozone-persist`, `ozone-tui`, and `apps/ozone-plus` can evolve independently
- **rusqlite FTS5 + WAL** — Phase 1A persistence uses SQLite content-sync FTS tables and WAL-backed session/global databases; Phase 2A manual recall reuses those FTS surfaces for session-local and cross-session keyword search
- **tokio broadcast** — Phase 1B engine events use a lightweight broadcast channel so future UI layers can subscribe without owning writes
- **ratatui TestBackend** — Phase 1C shell layout/render tests use `TestBackend` to verify 80x24 and 120x40 shell behavior without requiring a live terminal
- **config + TOML layering** — Phase 1D inference config merges baked defaults, XDG global config, per-session config, and environment overrides
- **minijinja** — Phase 1D prompt rendering for built-in ChatML / Alpaca / Llama-3-style templates plus optional overrides
- **tokio-util codecs + tokio mpsc/oneshot** — Phase 1D streaming decode, cancellation, and app-runtime/background-worker coordination between `ozone-inference` and `apps/ozone-plus`
- **fastembed 5.13** — optional CPU embedding backend in `ozone-memory`, kept behind a feature-gated provider seam so ozone+ can still run in FTS-only mode
- **usearch 2.24** — disk-backed vector index backend for Phase 2B embedding rebuilds and hybrid retrieval

## What We Deliberately Do NOT Use

- No web framework — this is a terminal-only tool, no HTTP server
- No ORM — rusqlite with raw SQL, the schema is simple enough
- No async TUI — ratatui render loop is synchronous, async is only for HTTP/process ops
- No automatic background embedding jobs yet — Phase 2B uses explicit `ozone-plus index rebuild` plus visible FTS-only fallback instead of hidden refresh behavior
- No serde for config files — preset files use pipe-delimited format (legacy compatibility with KoboldCpp ecosystem)
- No ncurses/termion — crossterm is cross-platform and doesn't need C deps

## Version Constraints

- Rust stable (no nightly features required)
- rusqlite uses `bundled` feature — compiles its own SQLite, no system sqlite3 needed
- ratatui 0.29 API — `Frame` no longer needs lifetime parameter (changed from 0.26)

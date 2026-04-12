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
last_updated: 2025-07-12
---

# Stack

## Core Technologies

- **Rust 2021 edition** — primary language, single compiled binary
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
- **directories 5** — XDG-compliant data/config paths (`~/.local/share/ozone/`)
- **chrono 0.4** — timestamps in benchmark records and log formatting
- **anyhow** — error handling with context (all public functions return `anyhow::Result`)
- **libc 0.2** — low-level process signaling (kill syscall)

## What We Deliberately Do NOT Use

- No web framework — this is a terminal-only tool, no HTTP server
- No ORM — rusqlite with raw SQL, the schema is simple enough
- No async TUI — ratatui render loop is synchronous, async is only for HTTP/process ops
- No serde for config files — preset files use pipe-delimited format (legacy compatibility with KoboldCpp ecosystem)
- No ncurses/termion — crossterm is cross-platform and doesn't need C deps

## Version Constraints

- Rust stable (no nightly features required)
- rusqlite uses `bundled` feature — compiles its own SQLite, no system sqlite3 needed
- ratatui 0.29 API — `Frame` no longer needs lifetime parameter (changed from 0.26)

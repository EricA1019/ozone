---
name: conventions
description: How code is written in this project — naming, structure, patterns, and style. Load when writing new code or reviewing existing code.
triggers:
  - "convention"
  - "pattern"
  - "naming"
  - "style"
  - "how should I"
  - "what's the right way"
edges:
  - target: context/architecture.md
    condition: when a convention depends on understanding the system structure
last_updated: 2026-04-12
---

# Conventions

## Naming

- Files: snake_case (`main.rs`, `profiling.rs`, `src/ui/launcher.rs`)
- Functions: snake_case, verb-first (`estimate_vram_mb`, `clear_gpu_backends`)
- Structs: PascalCase (`CatalogRecord`, `HardwareProfile`, `LaunchPlan`)
- Constants: SCREAMING_SNAKE_CASE (`VRAM_HEADROOM_RATIO`, `HEX_CURSOR`)
- Colors: named palette constants in theme.rs (`VIOLET`, `CYAN`, `GREEN`, `AMBER`, `RED`)

## Structure

- The repo is a Cargo workspace:
  - current `ozone` package stays at the repo root in `src/`
  - shared crates live under `crates/`
  - app targets beyond the root package live under `apps/`
- UI rendering is separated: `src/ui/launcher.rs` (render functions), `src/ui/mod.rs` (App state + event loop)
- Data logic is in dedicated modules: `catalog.rs` (file parsing), `planner.rs` (computation), `prefs.rs` (persistence)
- External process management: `processes.rs` — all `Command::new()` calls live here
- Theme/style: `theme.rs` — all Color/Style constants and helpers, never inline `Color::Rgb()` elsewhere
- Database: `db.rs` — all SQLite queries, never use `Connection` directly outside this module
- Shared product metadata and ozone filesystem path helpers belong in `crates/ozone-core`, not scattered across app modules
- ozone+ conversation sequencing, snapshots, and event fanout belong in `crates/ozone-engine`
- ozone+ persistence schema, migrations, and repository code belong in `crates/ozone-persist`, not in the root `ozone` app or ad hoc in `apps/ozone-plus`
- `apps/ozone-plus` should talk to ozone+ persistence through an engine-facing facade or local store adapter; raw `SqliteRepository` mutations should stay inside that adapter layer
- `crates/ozone-tui` should stay backend-agnostic: it owns shell state, key handling, layout, render, and the terminal loop, while app-specific `SessionRuntime` adapters live in `apps/ozone-plus`
- If the ozone+ shell needs real persistence or engine work, keep it behind the `SessionRuntime` boundary or a local adapter in `apps/ozone-plus` rather than coupling `ozone-tui` directly to `ozone-persist`

## Patterns

All public functions return `anyhow::Result<T>` — never panic, never unwrap in library code:
```rust
// Correct
pub fn load_catalog(...) -> Result<Vec<CatalogRecord>> {
    let data = fs::read_to_string(path)?;
    Ok(parse(data))
}

// Wrong
pub fn load_catalog(...) -> Vec<CatalogRecord> {
    let data = fs::read_to_string(path).unwrap();
    parse(data)
}
```

Hardware queries gracefully degrade — return `None` or defaults when nvidia-smi/proc is unavailable:
```rust
pub async fn query_gpu() -> Option<GpuInfo> {
    // Returns None if nvidia-smi not found, not an error
}
```

TUI state lives in `App` struct (`src/ui/mod.rs`) — render functions take `&App` and never mutate state. Key handlers in the event loop mutate `App`.

## Verify Checklist

Before presenting any code:
- [ ] No `.unwrap()` in library code — use `?` or `.unwrap_or_default()`
- [ ] Colors use theme.rs constants, not inline `Color::Rgb()`
- [ ] Process management goes through processes.rs, not direct `Command::new()` elsewhere
- [ ] New CLI subcommands registered in main.rs `Commands` enum
- [ ] External paths use `directories` crate or `$HOME`, never hardcoded absolute paths
- [ ] Render functions take `&App`, never `&mut App`
- [ ] All async HTTP uses reqwest with timeout

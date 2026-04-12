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
last_updated: 2025-07-12
---

# Conventions

## Naming

- Files: snake_case (`bench_protocol.rs`, not `benchProtocol.rs`)
- Functions: snake_case, verb-first (`estimate_vram_mb`, `clear_gpu_backends`)
- Structs: PascalCase (`CatalogRecord`, `HardwareProfile`, `LaunchPlan`)
- Constants: SCREAMING_SNAKE_CASE (`VRAM_HEADROOM_RATIO`, `HEX_CURSOR`)
- Colors: named palette constants in theme.rs (`VIOLET`, `CYAN`, `GREEN`, `AMBER`, `RED`)

## Structure

- All source in `src/` — flat module structure, no deep nesting
- UI rendering is separated: `src/ui/launcher.rs` (render functions), `src/ui/mod.rs` (App state + event loop)
- Data logic is in dedicated modules: `catalog.rs` (file parsing), `planner.rs` (computation), `prefs.rs` (persistence)
- External process management: `processes.rs` — all `Command::new()` calls live here
- Theme/style: `theme.rs` — all Color/Style constants and helpers, never inline `Color::Rgb()` elsewhere
- Database: `db.rs` — all SQLite queries, never use `Connection` directly outside this module

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

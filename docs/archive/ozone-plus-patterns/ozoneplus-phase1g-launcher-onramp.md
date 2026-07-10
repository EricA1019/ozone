---
name: ozoneplus-phase1g-launcher-onramp
description: Pattern for the Phase 1G frontend-choice on-ramp in the ozone launcher — FrontendChoice screen, FrontendMode enum, --frontend CLI flag, and exec handoff to ozone-plus.
applies_to: ["src/main.rs", "src/ui/mod.rs", "src/ui/launcher.rs"]
---

# Phase 1G: Launcher On-Ramp to ozone+

## What was added

A **frontend-choice step** inserted between `Screen::Confirm` and the backend launch in the existing `ozone` TUI.

## Screen Flow

```
Splash → Launcher → ModelPicker → Confirm → [FrontendChoice] → Launching / Monitor
                                                              ↘ exec → ozone-plus handoff --launcher-session
```

`Screen::FrontendChoice` only appears when no `--frontend` flag was passed. When a flag is given, the code jumps directly to the appropriate path from `Screen::Confirm`.

## Key symbols

| Symbol | Location | Purpose |
|---|---|---|
| `Screen::FrontendChoice` | `src/ui/mod.rs` | New screen variant |
| `FrontendMode` | `src/ui/mod.rs` | `SillyTavern` \| `OzonePlus` enum |
| `App::preferred_frontend` | `src/ui/mod.rs` | Set from `--frontend` flag |
| `App::frontend_choice_index` | `src/ui/mod.rs` | 0 = SillyTavern, 1 = ozone+ |
| `App::ozone_plus_handoff` | `src/ui/mod.rs` | Set true when ozone+ chosen |
| `render_frontend_choice` | `src/ui/launcher.rs` | Renders the choice list |
| `run_launcher(no_browser, preferred_frontend)` | `src/ui/mod.rs` | Updated signature |

## CLI flag

```
ozone --frontend sillyTavern   # bypass choice, go straight to ST path
ozone --frontend ozonePlus     # bypass choice, exec ozone-plus handoff --launcher-session
```

`FrontendMode` derives `clap::ValueEnum` with kebab-case values.

## Handoff bridge

After the TUI exits normally with `ozone_plus_handoff = true`:

```rust
// src/ui/mod.rs (end of run_launcher)
if app.ozone_plus_handoff {
    let ozone_plus_bin = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|dir| dir.join("ozone-plus")))
        .filter(|p| p.exists())
        .unwrap_or_else(|| std::path::PathBuf::from("ozone-plus"));
    use std::os::unix::process::CommandExt;
    let mut command = std::process::Command::new(ozone_plus_bin);
    command.arg("handoff").arg("--launcher-session");
    match app.prefs.preferred_backend {
        Some(BackendMode::KoboldCpp) => {
            command.env("OZONE__BACKEND__TYPE", "koboldcpp");
            command.env("OZONE__BACKEND__URL", ozone_core::paths::koboldcpp_base_url());
        }
        Some(BackendMode::LlamaCpp) => {
            command.env("OZONE__BACKEND__TYPE", "llamacpp");
            command.env("OZONE__BACKEND__URL", ozone_core::paths::llamacpp_base_url());
        }
        Some(BackendMode::Ollama) | None => {}
    }
    let err = command.exec();
    return Err(anyhow::anyhow!("Failed to exec ozone-plus: {err}"));
}
```

The `exec()` call **replaces the current process** — terminal state is clean because `disable_raw_mode` + `LeaveAlternateScreen` run just before. No subprocess overhead.

The newer handoff path makes two policy choices explicit instead of implicit:

1. The launcher asks ozone+ for a dedicated `Launcher Session` via `--launcher-session`.
2. The launcher only exports backend env overrides for backends it actually starts itself (`KoboldCpp` / `LlamaCpp`).

## Backend is always started first

When the user picks ozone+, the selected local backend is **still started** (same as the ST path) before handing off. Today that means `KoboldCpp` or `LlamaCpp`; `Ollama + ozone+` is still rejected. The backend start code in `Screen::Confirm`'s Enter handler is shared.

## Key behaviour differences from ST path

| | SillyTavern | ozone+ |
|---|---|---|
| Backend start | yes | yes |
| Browser open | yes (unless `--no-browser`) | **no** |
| Monitor screen | yes | **no** (exec to ozone-plus) |
| Return to ozone | yes (Esc in monitor) | **no** (exec replaces process) |

## Current guardrail

The current ozone+ runtime path supports **KoboldCpp and llama.cpp**, but still not `Ollama + ozone+`. Reject the unsupported combo clearly instead of silently pretending the guided launch path is wired end-to-end.

## Adding a new frontend option

1. Add variant to `FrontendMode` enum (auto-derives `ValueEnum`).
2. Add list item to `render_frontend_choice` in `src/ui/launcher.rs`.
3. Add match arm in `pending_launch_choice` handler in `src/ui/mod.rs`.
4. Add handoff flag/exec at end of `run_launcher`.

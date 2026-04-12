---
name: router
description: Session bootstrap and navigation hub. Read at the start of every session before any task. Contains project state, routing table, and behavioural contract.
edges:
  - target: context/architecture.md
    condition: when working on system design, integrations, or understanding how components connect
  - target: context/stack.md
    condition: when working with specific technologies, libraries, or making tech decisions
  - target: context/conventions.md
    condition: when writing new code, reviewing code, or unsure about project patterns
  - target: context/decisions.md
    condition: when making architectural choices or understanding why something is built a certain way
  - target: context/setup.md
    condition: when setting up the dev environment or running the project for the first time
  - target: patterns/INDEX.md
    condition: when starting a task — check the pattern index for a matching pattern file
last_updated: 2026-04-12
---

# Session Bootstrap

If you haven't already read `AGENTS.md`, read it now — it contains the project identity, non-negotiables, and commands.

Then read this file fully before doing anything else in this session.

## Current Project State

**Working:**
- Launcher dashboard, model picker, launch confirm flow, and live monitor all work in the ratatui TUI
- CLI profiling commands work: `bench`, `sweep`, `analyze`, profile generation, and preset export
- New TUI profiling flow works: advisory -> confirm -> running -> success/failure report
- Profiling now gives actionable warnings for risky plans and issue reports for invalid model paths / launcher failures
- Success reports keep the UX review-first: generate/export/launch are explicit follow-up actions, not automatic side effects
- Product-family docs now include a top-level family guide plus dedicated `ozonelite`, `ozone`, `ozone+`, and shared compatibility documents under `ozone+/`

**Not yet built:**
- TUI cancellation for in-flight profiling runs
- A dedicated in-TUI browser for existing benchmark history and Pareto data
- Rich per-substep sweep visualizations beyond streamed text progress
- Better startup/launcher flow for "Start SillyTavern only" (still status-only)
- Dedicated ozone+ workflow / PRD docs and phase-to-implementation breakdowns beyond the current baseline design

**Known issues:**
- Broken `.gguf` symlinks can still appear in the catalog/list and are only surfaced as issues when selected
- Hardware guidance is still NVIDIA-centric because GPU memory detection depends on `nvidia-smi`
- Test coverage is still intentionally light; only profiling-domain helpers have unit tests right now

## Routing Table

Load the relevant file based on the current task. Always load `context/architecture.md` first if not already in context this session.

| Task type | Load |
|-----------|------|
| Understanding how the system works | `context/architecture.md` |
| Working with a specific technology | `context/stack.md` |
| Writing or reviewing code | `context/conventions.md` |
| Making a design decision | `context/decisions.md` |
| Setting up or running the project | `context/setup.md` |
| Any specific task | Check `patterns/INDEX.md` for a matching pattern |

## Behavioural Contract

For every task, follow this loop:

1. **CONTEXT** — Load the relevant context file(s) from the routing table above. Check `patterns/INDEX.md` for a matching pattern. If one exists, follow it. Narrate what you load: "Loading architecture context..."
2. **BUILD** — Do the work. If a pattern exists, follow its Steps. If you are about to deviate from an established pattern, say so before writing any code — state the deviation and why.
3. **VERIFY** — Load `context/conventions.md` and run the Verify Checklist item by item. State each item and whether the output passes. Do not summarise — enumerate explicitly.
4. **DEBUG** — If verification fails or something breaks, check `patterns/INDEX.md` for a debug pattern. Follow it. Fix the issue and re-run VERIFY.
5. **GROW** — After completing the task:
   - If no pattern exists for this task type, create one in `patterns/` using the format in `patterns/README.md`. Add it to `patterns/INDEX.md`. Flag it: "Created `patterns/<name>.md` from this session."
   - If a pattern exists but you deviated from it or discovered a new gotcha, update it with what you learned.
   - If any `context/` file is now out of date because of this work, update it surgically — do not rewrite entire files.
   - Update the "Current Project State" section above if the work was significant.

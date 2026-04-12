# Ozone Product Family Guide

Ozone is no longer being defined as one monolithic product.

It is now a **family of local-first tools** with three distinct builds:
- **ozonelite** for lean backend control
- **ozone** for backend tuning, profiling, and custom profile creation
- **ozone+** for the full local-LLM workflow, including a polished terminal frontend

This file is the onboarding entrypoint for that family. It explains the intent, the problem being addressed, the scope of each build, and which build is for you.

**Contact:** ScribeALB@proton.me

---

## 1. Why Ozone Exists

Local LLM users often have very different needs:
- some want the lightest possible backend manager on weak or remote systems
- some want benchmark-driven tuning and reusable launch profiles
- some want a full workflow that goes from backend control to a frontend built for long local sessions

Trying to force all of that into one product creates scope bleed:
- lean users inherit features they do not want
- tuning users inherit frontend complexity they may never use
- full-pipeline users inherit unclear boundaries between infrastructure tooling and product UX

The Ozone family exists to solve that by making the layers explicit.

---

## 2. Product Philosophy

Across all three builds, Ozone should stay:
- **local-first**
- **low-overhead**
- **transparent**
- **power-user friendly**
- **modular instead of bloated**

The family should scale by **adding layers only when the user needs them**:
- start with the smallest possible backend-management core
- add profiling and custom profile workflows in the middle tier
- add the full frontend and roleplay-oriented workflow only in the top tier

This keeps intent clear:
- **ozonelite** optimizes for lean control
- **ozone** optimizes for repeatable tuning and management
- **ozone+** optimizes for end-to-end local usage, especially local-LLM frontend work

---

## 3. The Problem Being Addressed

The project is addressing three related but different problems:

| Build | Core problem it solves |
|------|-------------------------|
| **ozonelite** | "I need a minimal way to manage local backends without extra layers." |
| **ozone** | "I need to tune, benchmark, compare, and build custom profiles for local backends." |
| **ozone+** | "I want the whole local pipeline: backend management plus a serious TUI/frontend designed for local LLM use." |

The key decision is that these should not be treated as the same product with optional clutter piled on top. They are related builds with shared DNA and different scope ceilings.

---

## 4. Build Overview

| Build | Primary role | Best for | Includes | Explicitly does not try to be |
|------|---------------|----------|----------|-------------------------------|
| **ozonelite** | Ultra-lean backend manager | Constrained systems, SSH boxes, power users who want maximum control | Basic backend launch/control/inspection, minimal overhead, essential operational tooling | A profiling suite, custom profile authoring system, or full frontend |
| **ozone** | Backend tuning and management layer | Users who want repeatable performance tuning and better operator workflows | Everything in ozonelite, plus profiling, benchmarking, custom profile creation, and saved profile workflows | A full local-LLM conversation frontend |
| **ozone+** | Full local-LLM pipeline | Users who want one cohesive workflow from backend control to polished terminal UX | Backend management, profiling foundations, and a frontend/TUI built for local LLM workflows | A generic cloud chat app or browser-first product |

---

## 5. Which Build Is for You?

Choose **ozonelite** if:
- you care most about the smallest footprint
- you already know what backend settings you want
- you prefer direct control over guided workflows
- you want something comfortable on weak hardware or remote shells

Choose **ozone** if:
- you want to benchmark and compare backend configurations
- you want reusable profiles instead of hand-tuning every launch
- you want profile creation to be a first-class workflow
- you want stronger management features without committing to a full frontend

Choose **ozone+** if:
- you want the full local workflow in one tool
- you want backend management and frontend interaction to feel unified
- you want a TUI designed around local LLM usage rather than generic chat
- you are willing to accept the largest scope in exchange for the most complete experience

---

## 6. Scope Boundaries

The boundary between builds should stay sharp:

- **ozonelite** stops at lean backend management
- **ozone** adds tuning intelligence and profile workflows, but stops short of becoming the full frontend product
- **ozone+** is the only build that carries the full pipeline and polished end-user TUI scope

This matters because it protects all three products:
- ozonelite stays fast and disciplined
- ozone stays focused on tuning and repeatability
- ozone+ can go deeper on UX and local-LLM workflow design without dragging that complexity into the lower tiers

---

## 7. Shared Direction

These builds should still feel like one family:
- shared terminology where possible
- compatible backend definitions where practical
- an upgrade path from **ozonelite -> ozone -> ozone+**
- clear feature-gating instead of hidden coupling

The user should be able to understand what they gain by moving up the ladder without feeling like the smaller builds are artificially crippled.

---

## 8. Documentation Map

### Start here
- `README.md` — onboarding, philosophy, product boundaries, build selection

### Product scope documents
- `ozonelite_scope.md` — goals, exclusions, and boundary rules for the lean backend-control build
- `ozone_scope.md` — goals, included workflows, and explicit ceilings for the tuning-and-management build

### Ozone+ build-prep documents
- `ozone_plus_documentation_stack.md` — how the ozone+ docs fit together and how to read them
- `ozone_v0.4_design.md` — current **ozone+ baseline** architecture and roadmap

### Shared family documents
- `compatibility_and_migration.md` — portability, upgrade-path, and compatibility direction across the family

### Historical documents
- `ozone_revised_design.md` — earlier umbrella redesign
- `ozone_v0.3_design.md` — previous detailed monolithic design

### Next planned documents
- ozone+ workflow / PRD-level docs
- deeper implementation breakdowns mapped from v0.4 phases
- more explicit compatibility notes once shared schemas exist

---

## 9. Contact

For project questions, collaboration, or direct contact:

- **Email:** ScribeALB@proton.me

---

## 10. Current Intent

The immediate goal is still **documentation clarity before implementation**.

This documentation pass now establishes the missing family scaffolding:
- what belongs in ozonelite
- what belongs in ozone
- what belongs only in ozone+
- how `ozone_v0.4_design.md` should be used in the ozone+ doc stack
- how portability should work across the family

That clarity is the foundation for the next ozone+ workflow docs and for the eventual implementation plan.

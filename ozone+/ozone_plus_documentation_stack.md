# Ozone+ Documentation Stack

**Build:** ozone+  
**Status:** Documentation routing and build-prep guide  
**Primary baseline:** `ozone_v0.4_design.md`

---

## 1. Purpose

This document explains how the ozone+ documentation should be read now that Ozone is defined as a product family instead of one monolithic product.

The main job of this file is to separate:

- family-level positioning
- ozone+-specific architecture and roadmap
- shared compatibility assumptions across the family
- historical design material kept for reference

---

## 2. Canonical Reading Order

Read the ozone+ documentation in this order:

1. `README.md`  
   Start here for the family overview, build boundaries, and "which build is for you" guidance.

2. `ozone_plus_documentation_stack.md`  
   Use this file to understand which document answers which ozone+ question.

3. `compatibility_and_migration.md`  
   Read this for family-wide portability and upgrade assumptions.

4. `ozone_v0.4_design.md`  
   This is the current ozone+ baseline architecture and roadmap.

5. Historical documents (`ozone_revised_design.md`, `ozone_v0.3_design.md`)  
   Read these only for lineage, rationale history, or source-material comparison.

---

## 3. Role of Each Current Document

| Document | Role |
|----------|------|
| `README.md` | Family onboarding, philosophy, scope boundaries, build selection |
| `ozone_plus_documentation_stack.md` | Routing layer for ozone+ docs and build-prep context |
| `compatibility_and_migration.md` | Family-level compatibility direction, portability rules, and upgrade path |
| `ozone_v0.4_design.md` | ozone+ baseline architecture, data model, roadmap, and first implementation order |
| `ozone_revised_design.md` | Earlier redesign that shaped the deterministic-first direction |
| `ozone_v0.3_design.md` | Earlier detailed monolithic design kept as historical source material |

---

## 4. What `ozone_v0.4_design.md` Is

`ozone_v0.4_design.md` should be treated as the current baseline for:

- ozone+ product intent
- architecture and subsystem contracts
- data and persistence model
- TUI/UX structure
- phased implementation roadmap

It is **not** the umbrella family guide.

It is also not yet a fully split documentation stack by itself. It is the dense baseline document that deeper ozone+-specific docs should build from.

---

## 5. What to Use for Build Preparation

If the goal is to prepare for ozone+ implementation, the most important sections in `ozone_v0.4_design.md` are:

- **Sections 1-4** for product goal, non-goals, principles, and scope tiers
- **Sections 19-21** for backend strategy, config system, and TUI/UX design
- **Section 29** for the phased roadmap
- **Section 31** for the first implementation order

The intended execution shape remains:

- build family-level documentation clarity first
- then use the v0.4 roadmap as the ozone+ implementation baseline

---

## 6. What Still Needs Expansion

The ozone+ MVP (Phases 1–3, v0.4.0-alpha) is fully implemented. Items previously listed as "still needing expansion" in this section are now complete:

- ✅ Screen-by-screen TUI interaction notes — implemented in `crates/ozone-tui/`
- ✅ Import/export compatibility — character card import and session export implemented
- ✅ Implementation breakdowns mapped from roadmap phases — all phases 1–3 executed

**What genuinely still needs expansion:**

- **Group chat / multi-character scenes (Phase 4)** — scene state model, multi-actor card storage, `/as` speaker routing, narrator system, and context assembly for scenes are not yet implemented. Deferred to v0.5.
- **Mention-based speaker auto-detection (Phase 5)** — assistive speaker suggestions based on message content are not yet implemented.
- **Release packaging** — the binary ships from source build only; no prebuilt GitHub release artifact or installer yet.
- **macOS/Windows support** — not yet tested or supported.
- **Embedding model assets** — the optional vector/embedding recall path works but requires downloading `fastembed` model assets at runtime; first-run experience for this path is not yet documented.

Any new ozone+ documents should still be written as ozone+ docs, not mixed back into the family overview. The boundary reminder in Section 7 still applies.

---

## 7. Boundary Reminder

Keep these distinctions explicit:

- `ozonelite` = lean backend control
- `ozone` = backend tuning and management
- `ozone+` = full local-LLM workflow with the polished terminal frontend

Any ozone+ document that starts redefining the whole family should move that content back to `README.md` or a shared family-level document instead.

# Ozonelite Scope Document

**Build:** ozonelite  
**Status:** Product-family scope definition  
**Role in the family:** lean backend control

---

## 1. Purpose

ozonelite exists for users who want the smallest possible Ozone build that can safely manage local backends without carrying profiling or frontend-product complexity.

It is the "run and control the backend" layer of the family.

---

## 2. Product Goal

Ship a low-overhead, terminal-friendly backend manager that:
- starts quickly
- works well on weak hardware and remote shells
- exposes the operational controls a user actually needs
- stays scriptable and understandable

ozonelite should feel comfortable over SSH, tmux, and constrained Linux boxes.

---

## 3. What Belongs in Ozonelite

The build should include only the capabilities needed for direct backend operation:

- backend launch, stop, restart, and status inspection
- model-path selection or discovery required for launching
- basic operational configuration needed to run safely
- lightweight health and resource visibility
- backend-oriented logs or issue surfaces that help users recover from failure
- exportable backend definitions if those are shared across the family
- CLI-first workflows that are easy to script

If a feature helps a user run one backend safely and predictably, it is a strong ozonelite candidate.

---

## 4. What Does Not Belong in Ozonelite

The following are explicitly outside ozonelite scope:

- profiling, benchmarking, sweep, or comparative tuning workflows
- custom profile authoring systems
- saved management layers that exist primarily for repeatable tuning
- full conversation, session, or roleplay UX
- transcript persistence, branches, swipes, memory, or context assembly
- browser-first or frontend-heavy experiences
- hidden automation that increases footprint without improving core backend control

ozonelite should not become a "small version of everything." It should stay narrow on purpose.

---

## 5. UX Surface

ozonelite should be:

- **CLI-first**
- terminal-native and remote-shell friendly
- readable in narrow terminals
- comfortable for power users who prefer direct control

It may expose minimal terminal status views if they remain lightweight, but it should not depend on a richer conversation-oriented TUI.

---

## 6. Dependency and Performance Budget

ozonelite should optimize for:

- low startup cost
- minimal runtime overhead
- simple config surfaces
- small dependency footprint
- predictable operator workflows

Do not add a dependency to ozonelite if its only justification is a profiling suite, a deep UI layer, or higher-tier workflow polish.

---

## 7. Boundary Rules

Use these rules when deciding where a feature belongs:

- If it helps **operate a backend directly**, it can belong in ozonelite.
- If it helps **compare, benchmark, or tune backends across runs**, it belongs in `ozone`.
- If it supports **conversation UX, session state, characters, memory, or roleplay workflows**, it belongs in `ozone+`.

---

## 8. Relationship to the Other Builds

- **ozonelite -> ozone** adds tuning intelligence and profile workflows.
- **ozonelite -> ozone+** is not a direct jump in philosophy; ozone+ assumes a much larger workflow surface.
- ozonelite must remain useful on its own, not as an artificially crippled starter edition.

---

## 9. Scope Test

Before adding anything to ozonelite, ask:

1. Does this help the user run or inspect a backend right now?
2. Would remote-shell and low-overhead users still want it?
3. Can it be explained without introducing benchmarking or frontend-product concepts?

If the answer is "no" to any of those, the feature likely belongs in a higher tier.

# Ozone Scope Document

**Build:** ozone  
**Status:** Product-family scope definition  
**Role in the family:** tuning and management layer

---

## 1. Purpose

ozone is the operator build for users who want repeatable backend tuning, benchmarking, and profile workflows without stepping up to the full conversation product.

It sits between ozonelite and ozone+:

- more capable than a lean backend controller
- intentionally narrower than a full local-LLM frontend

---

## 2. Product Goal

Ship the Ozone build that helps users:

- benchmark and compare backend configurations
- create and save reusable profiles
- understand hardware and launch tradeoffs
- move from ad-hoc tuning to repeatable operator workflows

ozone should feel like a management-and-tuning environment, not a chat application.

---

## 3. What Ozone Inherits from Ozonelite

ozone should inherit the core backend-management foundation:

- launch, stop, restart, and inspect backend processes
- model and path selection needed for launch workflows
- basic operational configuration
- terminal-native control and health/status visibility
- low-level backend definitions that remain portable upward in the family

The difference is not that ozone replaces the lean layer. It builds on it.

---

## 4. What Belongs in Ozone

The following capabilities define the middle tier:

- profiling, benchmarking, and sweep workflows
- comparative analysis across configurations
- launch-planning and hardware-aware recommendation surfaces
- custom profile creation and editing
- saved presets/profiles and reuse flows
- issue reports and review-first recommendations for operators
- management-oriented dashboards, monitors, or TUIs that support tuning work

If a feature exists mainly to help a user make better backend decisions across multiple runs, it belongs here.

---

## 5. What Does Not Belong in Ozone

These remain outside middle-tier scope:

- full conversation UX
- transcript/session persistence for interactive chat
- roleplay-specific TUI flows
- character cards, lorebooks, message branches, or swipes
- context assembly and context inspection for chat generation
- long-session memory systems
- polished end-user frontend features whose main value appears during active conversation

ozone must stop short of becoming the full local-LLM pipeline product.

---

## 6. UX Surface

ozone can be richer than ozonelite, but it should stay management-oriented:

- CLI plus operational TUI flows are in scope
- review-first workflows are preferred over hidden automation
- dashboards and live monitors are appropriate if they support tuning decisions
- the product should still feel transparent and power-user friendly

The moment the UX is primarily about "using the model in conversation," the feature has crossed into ozone+ territory.

---

## 7. Boundary Rules

Use these rules when deciding where a feature belongs:

- If it helps **run one backend right now**, it may belong in `ozonelite`.
- If it helps **tune, benchmark, compare, or save backend behavior**, it belongs in `ozone`.
- If it helps **chat, roleplay, manage transcript state, or inspect prompt context**, it belongs in `ozone+`.

---

## 8. Relationship to Ozone+

ozone+ can reuse tuning foundations from ozone, but ozone should not inherit ozone+'s frontend scope by default.

That means:

- profiling and profile creation belong in ozone first
- ozone+ may consume those capabilities as part of the full workflow
- ozone should remain useful for users who never want a chat frontend

---

## 9. Scope Test

Before adding something to ozone, ask:

1. Is this fundamentally a tuning or management feature?
2. Would the feature still matter if the user never opened a chat session?
3. Does it avoid pulling transcript, memory, or roleplay systems into the middle tier?

If not, the feature likely belongs in ozonelite or ozone+ instead.

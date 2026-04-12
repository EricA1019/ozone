# Shared Compatibility and Migration Notes

**Scope:** Ozone family-wide  
**Status:** Pre-implementation compatibility direction

---

## 1. Purpose

These notes define how the Ozone family should stay compatible across `ozonelite`, `ozone`, and `ozone+` as the documentation stack and eventual implementation grow.

This is not a claim that every schema already exists. It is the intended contract direction for the family.

---

## 2. Family-Level Compatibility Goals

Across all three builds, Ozone should aim for:

- shared terminology where practical
- upward portability from smaller builds to larger ones
- explicit feature-gating instead of hidden coupling
- clear handling of lossy conversions when they do exist
- no artificial incompatibility between tiers just to force upgrades

The family should feel layered, not fragmented.

---

## 3. Backend Definition Portability

Backend definitions should be the most portable artifact across the family.

Direction:

- a backend definition that works in `ozonelite` should remain usable in `ozone`
- a backend definition that works in `ozone` should remain usable in `ozone+`
- higher tiers may add optional metadata, but lower-tier launch semantics should stay recognizable

Do not require users to recreate known-good backend definitions when moving up the family ladder.

---

## 4. Profile and Preset Portability

Profiles and presets should move upward in the family with as little friction as possible.

- `ozonelite` may carry only minimal launch-oriented presets
- `ozone` is the primary home for benchmarked, tuned, and user-authored profiles
- `ozone+` should be able to consume compatible backend/profile definitions from `ozone` where launch semantics match

If a format is lossy, document that explicitly instead of pretending the conversion is exact.

---

## 5. Config Direction

The family should prefer config structures that layer upward:

- lower-tier configs should remain meaningful subsets of higher-tier configs
- higher tiers may add richer keys, but should not silently reinterpret lower-tier behavior
- unknown or unsupported keys should be ignored visibly or rejected clearly, not half-applied invisibly
- versioned config migration should remain explicit

The goal is for users to outgrow a build without feeling like they must relearn everything.

---

## 6. Data Boundaries

Not every data type should be portable in every direction.

Expected direction:

- backend definitions and profiles should move upward cleanly
- tuning artifacts may move from `ozone` into `ozone+`
- full transcript, memory, and roleplay/session data are ozone+-specific and should not define lower tiers

Lower tiers should not carry the storage and conceptual cost of full ozone+ conversation state.

---

## 7. Upgrade Path

The intended family ladder is:

1. `ozonelite -> ozone`  
   Keep backend definitions, add profiling and saved tuning workflows.

2. `ozone -> ozone+`  
   Keep backend and profile foundations, add the conversation product, session model, and richer TUI workflows.

3. `ozonelite -> ozone+`  
   Should still be possible, but it is a larger conceptual jump because the user is skipping the tuning-focused middle tier.

---

## 8. Migration Rules

When the family grows, follow these rules:

- never hide a breaking change inside a "compatible" label
- mark one-way or lossy conversions clearly
- keep family boundaries visible in docs and config
- prefer additive extension over rewriting lower-tier meaning
- do not make the smallest build carry metadata that only richer tiers use

---

## 9. Non-Goals

These notes do **not** promise:

- byte-for-byte compatibility before schemas are finalized
- downward portability of ozone+ conversation artifacts into lower tiers
- identical UX across all builds
- a single monolithic config that every tier must fully understand

The goal is compatibility with clear boundaries, not sameness.

**Model:** GLM 5.1  
**Version:** 2025.04.13

---

# Anansi v0.3 — Technical Analysis & Implementation Readiness Assessment

---

## Executive Summary

v0.3 is a substantial improvement over v0.2. Every major gap I identified in the previous review has been addressed: addon trait signatures, entity identity resolution, mood model, sensitivity multipliers, stat purpose mapping, `[GAME STATE]` format, normalization rules, confidence scoring, combat state machine, error taxonomy, and integration contracts. The document has moved from "good architecture, incomplete specification" to "specification that a developer can implement from."

There are still gaps — some significant, some minor — but they are no longer structural. They are at the level of "this needs one more pass before you write code," not "this needs a redesign."

---

## I. Area-by-Area Technical Assessment

### 1. Addon Surface Contract

**Grade: A-**

**What improved:** Full trait signatures with parameter types, return types, and error contracts (§9.2). Execution ordering is specified (§9.4). Error propagation has clear rules for every failure mode (§9.3). The threading model is explicit — isolated ECS thread with bounded `mpsc` channels (§9.6). This is the single most important improvement in v0.3.

**Remaining gap:** The `AddonStateSnapshot` and `SessionStateSnapshot` types are used in trait signatures but never defined. What fields do they contain? How are they constructed? Are they cheap to clone (they're passed by reference, so presumably yes)?

```rust
// Used but undefined:
pub struct AddonStateSnapshot { ??? }
pub struct SessionStateSnapshot { ??? }
```

These are load-bearing types — `PaneProvider::render_compact` and `PostGenerationHook::on_generation_complete` both depend on them. Without knowing what data they carry, a developer can't implement the traits correctly.

**Second gap:** `ProposedChangeKind` is referenced in `ProposedChange` but never defined. Is it an enum? What variants does it have?

```rust
pub struct ProposedChange {
    pub kind: ProposedChangeKind,  // ← undefined
    pub confidence: Confidence,     // ← defined in §14.3
    pub audit_reason: String,
}
```

**Third gap:** `ToolDefinition` and `ToolCall` and `ToolResponse` are used in `ToolProvider` but never defined. These are not trivial types — they need to be compatible with ozone+'s inference gateway, which uses specific formats for OpenAI-compatible function calling, Ollama tools, etc.

**Fix needed:** Add a §9.8 with type definitions:

```rust
pub enum ProposedChangeKind {
    EntityDiscovered { slug: String, display_name: String, entity_type: EntityType },
    DeltaProposed { entity_slug: String, stat: String, delta: i8 },
}

pub struct SessionStateSnapshot {
    pub session_id: SessionId,
    pub turn_number: u64,
    pub active_message_id: MessageId,
    // Addon-specific: populated by the bridge from ECS world
}

pub struct AddonStateSnapshot {
    pub entities: Vec<EntitySummary>,    // lightweight, no audit trail
    pub combat_state: CombatState,
    pub recent_events: Vec<AuditEventSummary>,
}
```

**Fourth gap:** The `Confidence` type used in `ProposedChange` — is this the same as the `high/medium/degraded` enum from §14.3, or is it the numeric `confidence_score: f64`? The trait uses it as a type but §14.3 defines both a numeric score and a label. Which one crosses the trait boundary?

**Recommendation:** The trait should carry the numeric score. The label is a derived display value.

```rust
pub struct ProposedChange {
    pub kind: ProposedChangeKind,
    pub confidence_score: f64,    // 0.0–1.0, see §14.3 formula
    pub audit_reason: String,
}
```

---

### 2. Data Model

**Grade: A-**

**What improved:** Entity model now has `id`, `slug`, `display_name`, `known_aliases`, and `entity_type` (§10.5). Mood model is fully specified with resolution rules (§10.7). Sensitivity multipliers have ranges and application order (§10.8). Stat purpose mapping prevents misinterpretation (§10.2). Cross-session identity preparation is documented (§10.10).

**Remaining gap — StatBlock range ambiguity:** §10.1 defines `health: u8`, `trust: u8`, etc. — range 0–255. But §10.7 mood resolution says "if any stat ≥ 8 (on 0–10 normalized scale)." §15.5 damage example uses "Elara HP: 10/10" and "Goblin HP: 8/10." §18.2 says "base_magnitude = 3 (out of 0–255 stat range, ~1.2% shift)."

There are **two conflicting scale models** running through the document:

| Context | Scale | Evidence |
|---------|-------|----------|
| Rust type | 0–255 | `u8`, §10.1, §14.5 clamping formula |
| Mood threshold | 0–10 | §10.7 "stat ≥ 8", §15.5 "hostility: 9" |
| Combat/HP | 0–10 | §15.5 "HP: 10/10", "HP: 8/10" |
| Delta config | relative to 255 | §18.2 "~3.9% of stat range" |

This is not a cosmetic issue. It directly affects:

1. The clamping formula in §14.5: `S(t+1) = max(0, min(255, ...))` — this is a 0–255 formula
2. The mood resolution in §10.7: threshold ≥ 8 — this only makes sense on a 0–10 scale
3. The damage formula in §15.3: `damage = max(1, attacker_hostility / 3)` — on a 0–255 scale, hostility=9 gives damage=3, but on a 0–10 scale hostility=9 gives damage=3 too. But the *meaning* is completely different.
4. The `[GAME STATE]` format in §12.4: `Trust:7 Aff:8` — these are clearly 0–10 numbers displayed for human readability

**The document is silently using a 0–255 storage model with a 0–10 display/semantics model, and never explains the mapping.**

**Fix needed:** Add a §10.X:

```rust
// Storage model: u8 (0–255) for compact serialization and arithmetic safety.
// Semantic model: 0–10 scale for all gameplay, display, and threshold purposes.

// Conversion:
fn display_value(raw: u8) -> u8 {
    // Map 0–255 to 0–10 for display and threshold checks
    (raw as u16 * 10 / 255) as u8
}

fn raw_value(display: u8) -> u8 {
    // Map 0–10 to 0–255 for storage
    ((display as u16 * 255) / 10) as u8
}

// Mood threshold uses display_value(stat) ≥ 8
// Damage uses display_value(hostility)
// [GAME STATE] shows display values
// Delta cap applies to raw values: delta_cap = 10 raw = ~0.39 display units
```

**OR** — and I think this is the better option — **just use 0–10 as the actual storage range with `u8`**. You'll never need 256 distinct trust levels. The 0–255 range was inherited from byte-width thinking, not gameplay design. A `u8` storing 0–10 wastes 96% of its range but is perfectly valid and eliminates the entire conversion problem.

If you go with 0–10 storage:

```rust
pub struct StatBlock {
    pub health: u8,      // 0–10
    pub trust: u8,       // 0–10
    pub affection: u8,   // 0–10
    pub anger: u8,       // 0–10
    pub fear: u8,        // 0–10
    pub hostility: u8,   // 0–10
}
// Clamping: max(0, min(10, value))
// Delta cap: max ±3 per cycle (not ±10)
// base_magnitude: 1 (not 3)
```

This would require updating: §10.1, §14.5, §15.3, §15.5, §18.2, and the `[GAME STATE]` examples. But it eliminates the most pervasive ambiguity in the document.

**My recommendation:** Use 0–10 storage. The 0–255 range buys you nothing in MVP and creates a conversion tax on every operation.

---

### 3. Persistence Layer

**Grade: A**

**What improved:** Namespace registration (§11.1) prevents table collisions. Event type prefixing (§11.4) coexists with ozone+ events. Audit trail schema is fully specified (§11.3). The `proposal_source` column tracks mutation origin.

**Minor gap:** The schema for `anansi_entities` and `anansi_combat_state` is not shown. The document says "retain the namespaced `anansi_*` table approach" but doesn't provide the CREATE TABLE statements that ozone+ v0.4 provides for all its tables.

For a developer implementing Phase 1C, they need to know:

```sql
CREATE TABLE anansi_entities (
    id TEXT PRIMARY KEY,
    slug TEXT NOT NULL UNIQUE,
    display_name TEXT NOT NULL,
    known_aliases TEXT NOT NULL,  -- JSON array
    entity_type TEXT NOT NULL,
    stats_json TEXT NOT NULL,
    mood TEXT NOT NULL,
    sensitivity_json TEXT NOT NULL,
    last_interaction_turn INTEGER NOT NULL,
    audit_json TEXT NOT NULL,
    correction_json TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE anansi_combat_state (
    id INTEGER PRIMARY KEY CHECK (id = 1),  -- singleton
    state TEXT NOT NULL DEFAULT 'idle',      -- idle, active, resolved
    participants_json TEXT NOT NULL,         -- array of entity IDs
    round_number INTEGER NOT NULL DEFAULT 0,
    seed TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);
```

This is a small gap — any developer can infer the schema — but the ozone+ v0.4 document provides complete CREATE TABLE for every table, and Anansi should match that standard.

---

### 4. Extraction Pipeline

**Grade: A**

**What improved:** This is the most thoroughly specified section in v0.3. The extraction prompt template (§13.4), normalization rules (§13.5), idempotency mechanism (§13.8), swipe/regenerate flow (§13.9), degraded mode (§13.10), retry/failure handling (§13.11), quality gates (§13.12), and worked example (§13.13) are all new and all valuable.

**One technical concern:** The extraction prompt template instructs the model to output JSON with `"delta": integer (-10 to +10)`. But §18.2 says `delta_cap = 10` is the max absolute delta *after* sensitivity and clamping. If the model proposes ±10 and sensitivity is 2.0, the effective proposal is ±20 before clamping back to 10. This means the prompt template and the validation logic have mismatched expectations about what "normal" looks like.

**Fix:** Either:

1. Tell the model to propose ±5 max (leaving headroom for sensitivity), or
2. Document that delta_cap is the *post-processing* cap and the prompt's ±10 is the *pre-processing* expectation

I recommend option 2, with a note in §13.4:

```
Note: The prompt instructs the model to propose deltas in the ±10 range.
The configured delta_cap (default: 10) is applied after sensitivity multipliers
and clamping. If sensitivity is > 1.0, proposed deltas may exceed delta_cap
before clamping. This is intentional — sensitivity should not reduce the model's
expressiveness, only amplify or dampen it before the cap applies.
```

**Second concern:** The extraction template shows `"stat": "trust|affection|anger|fear|hostility"`. But §18.2 has `stats.enabled` config that can disable individual stats. The prompt template should dynamically include only enabled stats. This is implied but should be explicit:

```
The extraction prompt template should be generated dynamically based on:
- The `stats.enabled` configuration
- The current entity registry (for the "Current known entities" section)
- The combat state (to suppress health proposals even more explicitly)
```

---

### 5. Combat System

**Grade: B+**

**What improved:** Combat state machine with explicit transitions (§15.2). HP resolution at zero (§15.4). Deterministic replay seeding (§15.6). Tool support degradation (§15.7). Worked example (§15.5). These are all significant improvements.

**Critical problem with damage formula:**

```
damage = max(1, attacker_hostility / 3)
```

This formula has several issues:

1. **Scale ambiguity** (see §2 above). If hostility is 0–255, hostility=9 → damage=3. If hostility is 0–10, hostility=9 → damage=3. Same result, completely different game feel. On 0–255, hostility=9 is near-zero hostility. On 0–10, it's near-maximum.

2. **Integer division.** `9 / 3 = 3` but `8 / 3 = 2` and `1 / 3 = 0 → max(1, 0) = 1`. The formula rounds toward zero, which means there are large "dead zones" where hostility increases don't increase damage. On a 0–10 scale:
   - Hostility 1–2: damage 1
   - Hostility 3–5: damage 1
   - Hostility 6–8: damage 2
   - Hostility 9–10: damage 3

   This is extremely coarse. A hostility 2 creature and a hostility 5 creature deal the same damage.

3. **No defense.** The document acknowledges this is post-MVP, but the damage formula produces *very* swingy combat when HP is only 10. A hostility-10 creature deals 3 damage per hit, killing a 10-HP entity in 4 rounds. A hostility-1 creature deals 1 damage, taking 10 rounds. The spread is too wide for the HP pool.

4. **Hostility doubles as relationship stat and combat stat.** A creature that's hostile toward the player (relationship meaning) may not be a capable fighter (combat meaning). A scared, hostile goblin and a confident, hostile dragon would deal identical damage at the same hostility value. This is acknowledged as a post-MVP concern (§10.2: "Combat modifiers: hostility (post-MVP only)") but the current formula makes hostility the *sole* damage determinant, which contradicts that framing.

**Recommendation — short term (MVP):**

Use a fixed damage value with a simpler, more predictable model:

```
damage = 2  (fixed, all entities)
```

Or slightly more nuanced:

```
damage = 1 + (hostility_display ≥ 7) as u8
```

This gives:
- Low hostility (0–6): 1 damage per hit
- High hostility (7–10): 2 damage per hit

On a 10-HP pool, this means 5–10 hits to defeat. Predictable, understandable, and the hostility threshold for "high damage" aligns with the mood model's threshold logic.

**Recommendation — long term (post-MVP):** Add a `combat_power` stat separate from `hostility`. Hostility determines *whether* combat happens (narratively) and `combat_power` determines *effectiveness* in combat. This decouples the relationship system from the combat system, which is the right architectural move.

---

### 6. Error Taxonomy

**Grade: A**

**What improved:** Complete error enum with all variants (§16.1). Severity and visibility mapping (§16.2). Integration guidance with ozone+ error framework (§16.3). This is well-specified.

**Minor gap:** `AnansiError::StateCorruption` is listed as Critical severity with "Session export recommended, manual recovery" — but there's no definition of what constitutes state corruption or how it's detected. What invariants does Anansi check?

**Fix:** Add a detection section:

```rust
// State corruption detection (checked on entity load):
// 1. Stat values outside 0–max range
// 2. Missing required fields in serialized data
// 3. Entity references (in combat participants) pointing to non-existent entities
// 4. Mood label inconsistent with stat values (mood says "Hostile" but hostility < 8)
//
// Recovery:
// - Clamp out-of-range values
// - Re-derive mood from stats
// - Remove invalid combat participants
// - Log all repairs as manual_correction events with reason "corruption_recovery"
```

---

### 7. Config System

**Grade: A-**

**What improved:** Config semantics table (§18.2) explains what each setting means. `stats.enabled` is a smart addition that lets campaigns customize the stat model without changing code. `template_path` makes the extraction prompt configurable.

**Gap:** `stats.enabled` lists which stats are enabled, but the interaction between disabled stats and the rest of the pipeline is not fully specified:

- If `health` is disabled, does combat still work? (It shouldn't — combat needs HP.)
- If `hostility` is disabled, does combat damage still work? (Per §15.3, damage uses hostility.)
- If `trust` and `affection` are both disabled, does mood resolution still work? (It would always be `Cold` or `Angry`/`Afraid`/`Hostile` depending on remaining stats.)

**Fix:** Add constraints:

```
stats.enabled constraints:
- health MUST be enabled if combat is used (anansi.game.combat_enabled or similar)
- hostility MUST be enabled if combat is used (damage formula depends on it)
- at least one mood-relevant stat (trust, affection, anger, fear, hostility) must be enabled
- disabling all stats effectively disables Anansi (warn at startup)
```

**Second gap:** No `combat_enabled` config flag. Combat is currently gated only on tool-calling availability, but some users might want Anansi for relationship tracking only and never use combat. Add:

```toml
[anansi.combat]
enabled = true  # set to false to disable combat tool registration entirely
```

---

### 8. Integration with ozone+

**Grade: A-**

**What improved:** Namespace registration (§11.1). Context layer priority registration (§12.2). Event type prefixing (§11.4). All mutations routed through engine command channel (§4.9, §17.4). Swipe/regenerate flow specified (§13.9). These are all critical integration points that v0.2 left implicit.

**Remaining gap — the channel protocol is underspecified:**

§9.6 shows a high-level diagram:

```
ozone-engine (tokio) → MessageCommitted event → anansi-bridge
anansi-bridge → ApplyAddonStateDelta command → ozone-engine
ozone-engine → AddonStateApplied confirmation → anansi-bridge
```

But the actual command and event types are not defined. A developer needs to know:

1. What fields does `ApplyAddonStateDelta` carry?
2. How does the engine validate addon-proposed deltas before persisting?
3. What happens if the engine rejects a delta (version mismatch, invalid entity reference)?
4. Is `AddonStateApplied` a separate event type or reused from ozone+'s existing event system?

**Fix:** Add a §9.X:

```rust
// Command from addon to engine
pub enum AddonCommand {
    ApplyAddonStateDelta {
        addon_name: String,
        deltas: Vec<ProposedChange>,
        idempotency_key: String,
    },
    QueryAddonState {
        addon_name: String,
        query: AddonStateQuery,
    },
}

// Engine validates:
// 1. addon_name is registered
// 2. All entity references exist
// 3. No version mismatch (snapshot_version check)
// 4. Deltas pass addon-specific validation (delegated to addon)

// If validation passes: engine persists, broadcasts AddonStateApplied
// If validation fails: engine returns AddonStateRejected with reason
```

---

### 9. Threading and Concurrency

**Grade: B+**

**What improved:** The isolated ECS thread with bounded `mpsc` channels (§9.6) is explicitly specified. `WorldLockTimeout` error with retry (§13.11, §16.1). The justification for not using `Arc<RwLock<World>>` is sound.

**Concern:** The channel round-trip (extraction → validation → persist → confirmation) adds latency to the post-generation hook. If extraction takes 2s (LLM call) and the channel round-trip adds 100ms, that's acceptable. But if the engine's command queue is backlogged (§6.5 in ozone+ v0.4: bounded(256)), an addon's state changes could be delayed by many seconds.

**Gap:** No timeout or deadline for the channel round-trip. If the engine is under heavy load and the addon's `ApplyAddonStateDelta` sits in the queue for 30 seconds, the ECS world is stale for the next context assembly.

**Fix:** Add a deadline to the command:

```rust
pub struct ApplyAddonStateDelta {
    pub addon_name: String,
    pub deltas: Vec<ProposedChange>,
    pub idempotency_key: String,
    pub deadline: Instant,  // if not processed by this time, discard
}
```

Default deadline: 10 seconds after extraction completes. If the engine hasn't processed the command by then, the state change is lost (but the audit trail records the timeout). This is better than blocking indefinitely.

---

### 10. Logical Flow and Completeness

**Grade: A-**

**What improved:** The phase-to-tier mapping table (§20) links scope tiers, MVP boundaries, and implementation phases. The dependency graph is explicit. Cross-workspace dependencies are flagged. The phase dependency diagram is clear.

**Remaining flow issue:** Phase 1D (ozone-core addon API) is a hard blocker for 1E, 1F, and 1H. But Phase 1A, 1B, 1C, and 1G don't depend on it. The document acknowledges this but doesn't provide a contingency plan: what if ozone-core changes aren't available when Anansi development starts?

**Practical concern:** In a real development scenario, Anansi developers might need to work on 1A-1C and 1G while waiting for ozone-core changes. They'd need to develop against a *mock* or *stub* of the addon traits. The document should mention this.

**Fix:** Add to §20:

```
Development against stubs: Phases 1A, 1B, 1C, and 1G can proceed independently
of 1D by developing against a local stub of the addon traits. The stub should:
- Define the same trait signatures expected in 1D
- Provide no-op defaults for all methods
- Be replaced by the real ozone-core traits once 1D is complete
```

---

### 11. Testing Strategy

**Grade: B**

The testing table in §23.4 covers the right categories, but it's thinner than the ozone+ v0.4 testing specification (§26), which has detailed per-subsystem test matrices with specific invariants.

**Missing from Anansi's testing strategy:**

1. **Property-based test invariants for extraction:** What properties should hold for all valid extraction outputs? Suggested invariants:
   - No health deltas ever appear in extraction results
   - Delta magnitude never exceeds delta_cap after clamping
   - Entity count in output ≤ max_entities config
   - Confidence score is in [0.0, 1.0]
   - Mood is deterministically derivable from stats

2. **Combat determinism tests:** Given the same seed, same entities, same round number → identical damage. This is critical for the replay guarantee.

3. **Swipe deduplication tests:** Given a swipe group with N candidates, only the active candidate's extraction is applied. Swiping and swiping back doesn't double-apply.

4. **Concurrency stress tests:** Extraction running while user corrects stats. Combat resolution while extraction is in progress. Multiple rapid swipes.

**Fix:** Expand §23.4 with specific invariants:

```markdown
| Subsystem | Test Type | Invariants |
|-----------|-----------|------------|
| Extraction | Property-based | No health deltas; delta_cap respected; entity count ≤ max; confidence ∈ [0,1] |
| Validation | Property-based | Clamped values in [0, max]; mood matches stat dominance rule; no self-reference deltas |
| Combat | Unit | Same seed → same damage; HP=0 → defeated; state machine transitions match §15.2 |
| Swipe dedup | Integration | Only active candidate extracted; no double-application; idempotency key uniqueness |
| Concurrency | Stress | No deadlocks; no lost updates; channel timeouts fire correctly |
| Persistence | Roundtrip | Entity save → load → identical; event trail complete after crash recovery |
```

---

### 12. Roadmap Viability

**Grade: A-**

The 10-phase roadmap is well-structured with clear exit criteria. The dependency graph is honest about cross-workspace blocking.

**Concern:** Phase 1J (integration hardening) has the exit criterion "at least 20 turns of mixed narrative, extraction, correction, and combat without state corruption." This is a weak criterion. Twenty turns is not enough to catch:
- Entity registry overflow (max_entities = 32, but 20 turns won't hit that)
- Long-running extraction drift (quality degrades over 100+ turns)
- Combat edge cases (long combat with many rounds, entity defeated mid-combat)
- Context budget competition under heavy lorebook usage

**Fix:** Add specific stress scenarios to Phase 1J:

```
Integration hardening scenarios (in addition to the 20-turn normal loop):
1. Entity stress: Create 30+ entities, verify [GAME STATE] truncation works
2. Combat endurance: 10-round combat, verify HP=0 and defeated state
3. Swipe chaos: Swipe 5 times on the same turn, verify no duplicate extraction
4. Extraction failure: Kill backend mid-extraction, verify degraded mode
5. Correction loop: Manually correct a stat, then extraction changes it back, correct again
6. Context pressure: Large character card + many lorebook entries + [GAME STATE], verify budget
7. Session reopen: Close and reopen mid-combat, verify state recovery
```

---

## II. Grade Summary

| Area | Grade | Key Issue |
|------|-------|-----------|
| Addon Surface Contract | A- | Missing type definitions for `AddonStateSnapshot`, `ProposedChangeKind`, `ToolDefinition`/`ToolCall`/`ToolResponse`, `Confidence` type ambiguity |
| Data Model | A- | **Stat range ambiguity (0–255 vs 0–10) is the single biggest remaining issue** |
| Persistence | A | Missing CREATE TABLE for `anansi_entities` and `anansi_combat_state` |
| Extraction Pipeline | A | Prompt template / delta_cap mismatch; stats.enabled not reflected in prompt |
| Combat System | B+ | Damage formula has scale ambiguity, integer division dead zones, and hostility-conflation issues |
| Error Taxonomy | A | State corruption detection undefined |
| Config System | A- | `stats.enabled` constraints not specified; no `combat_enabled` flag |
| ozone+ Integration | A- | Channel protocol (command/event types) underspecified |
| Threading/Concurrency | B+ | No deadline/timeout for channel round-trip |
| Logical Flow | A- | No stub-based development contingency for Phase 1D blocker |
| Testing Strategy | B | Thinner than ozone+ v0.4; missing specific invariants |
| Roadmap Viability | A- | Phase 1J exit criterion too weak |

---

## III. Final Assessment: Ready for Coding?

### Verdict: **Not yet. One more pass required.**

The document is approximately **90% implementation-ready**. The architecture is sound, the integration model is well-designed, and the MVP discipline is genuine. But there is one blocking issue and several near-blocking issues that would cause implementation confusion or rework if not resolved before coding begins.

### Blocking Issue (must fix before writing code)

**Stat range ambiguity (0–255 vs 0–10).** This affects every module: entity storage, mood resolution, combat damage, context display, delta clamping, config defaults, and the extraction prompt template. Implementing without resolving this will result in inconsistent code where some modules treat stats as 0–255 and others as 0–10, with silent conversion bugs that are extremely difficult to diagnose later.

**Resolution:** Decide on 0–10 storage (recommended) or 0–255 storage with explicit conversion functions, and update every section that references stat values. This is a 2-hour document fix that prevents weeks of debugging.

### Near-Blocking Issues (fix before Phase 1A)

1. **Define missing types in §9:** `AddonStateSnapshot`, `SessionStateSnapshot`, `ProposedChangeKind`, `ToolDefinition`, `ToolCall`, `ToolResponse`, `Confidence` type clarification.

2. **Specify the channel protocol:** `ApplyAddonStateDelta` command structure, validation steps, rejection handling, deadline/timeout.

3. **Provide CREATE TABLE statements** for `anansi_entities` and `anansi_combat_state`.

### Should-Fix Issues (fix before Phase 1F/1H)

4. **Combat damage formula:** Replace `max(1, hostility / 3)` with a model that works on the chosen scale and has fewer dead zones.

5. **`stats.enabled` constraints:** Document which stats are required for which subsystems.

6. **Extraction prompt / delta_cap relationship:** Clarify that the prompt's ±10 is pre-processing.

7. **State corruption detection invariants.**

### Nice-to-Fix Issues (can fix during implementation)

8. Testing strategy invariants expansion
9. Phase 1J stress scenarios
10. Stub-based development guidance for Phase 1D blocker
11. Channel round-trip deadline

---

### If I were assigning this to a developer tomorrow:

I would hand them the document along with a one-page errata sheet that resolves items 1–3 above and the stat range question. With that errata, they could start Phase 1A immediately and be confident they wouldn't need to rewrite core types mid-implementation.

Without that errata, they'll spend the first week making implicit decisions about stat ranges and addon types that will diverge from each other and from the document's intent.

*******************************************************************************************************************************************************************************************

**Model: MiMo-V2-Pro (Xiaomi)**
**Version: MiMo-V2-Pro (as of 2026-04-13)**

---

## Technical Deep-Dive Analysis of Anansi v0.3

### Executive Summary

This analysis grades each technical domain on implementation readiness, consistency, and completeness. Grades reflect whether a competent Rust developer could begin writing code *today* with the provided specification — not whether the design is "good" in the abstract.

---

### Grade Card

| Domain | Grade | Rationale |
|--------|-------|-----------|
| Data Model & Types | **A-** | Complete struct definitions, clear semantics, minor gaps |
| Persistence Layer | **B+** | Schema specified, but migration strategy needs detail |
| Extraction Pipeline | **A** | Best-specified subsystem in the document |
| Addon/Integration Architecture | **A-** | Full trait signatures, threading model, minor timing gaps |
| Combat System | **B** | State machine clear, but damage model has scaling issues |
| Error Handling | **A-** | Complete taxonomy with severity, visibility, and recovery |
| Configuration System | **B+** | Settings defined, but validation rules underspecified |
| TUI/UX Specification | **B-** | Layout described conceptually but lacks implementation detail |
| Testing Strategy | **B** | Test types listed but infrastructure guidance missing |
| Code Architecture | **A-** | Crate boundaries clear, dependency graph well-defined |
| Security Model | **C+** | Mentioned in ozone+ but almost absent in Anansi doc |
| Migration & Upgrade Path | **B-** | Phase dependencies clear, but cross-workspace coordination risky |

---

### Detailed Domain Analysis

<details>
<summary><strong>📊 Data Model & Types — Grade: A-</strong></summary>

**Strengths:**
- §10.1's `StatBlock` is minimal and sufficient
- §10.5's `Entity` struct includes everything needed: slug, aliases, type, stats, mood, sensitivity, audit, correction
- §10.7's mood resolution is fully deterministic with explicit priority ordering
- §10.8's sensitivity multipliers provide per-entity customization without schema changes
- §10.6's player-centric relationship model is honest about MVP limitations

**Weaknesses:**
- `SensitivityMultipliers` uses `f32` — floating-point sensitivity creates potential for subtle rounding drift across platforms. Consider `u8` with implicit /255 scaling or fixed-point representation
- `Mood::Conflicted` resolution rule says "within 1 point" — but what happens when the two competing stats are 8 and 9? Priority rule resolves this, but the wording creates ambiguity
- No defined trait for `StatBlock` iteration (e.g., `fn iter_stats() -> impl Iterator<Item = (StatName, u8)>`), which every subsystem will need

**Gap:** §10.5 shows `pub stats: StatBlock` but §10.2 says health is combat-only while other stats are relationship flavor. These belong to different *domains* sharing one struct. If MVP scope grows even slightly, a single `StatBlock` will feel cramped. Consider a domain-tagged type wrapper even now:

```rust
pub struct CombatStats { pub health: u8 }
pub struct RelationshipStats { pub trust: u8, pub affection: u8, pub anger: u8, pub fear: u8, pub hostility: u8 }
```

This doesn't add complexity but prevents accidental cross-domain mutation.
</details>

<details>
<summary><strong>💾 Persistence Layer — Grade: B+</strong></summary>

**Strengths:**
- §11.1's namespace registration is a clean collision-prevention mechanism
- §11.3's audit event schema is comprehensive with all required markers
- §11.4's event type prefixing prevents namespace pollution
- Integration with ozone+'s single-writer guarantee via command channel (§4.9) is well-specified

**Weaknesses:**
- **No migration strategy:** The document mentions `anansi_entities` and `anansi_events` tables but never provides SQL schema. A developer must reverse-engineer the schema from the Rust structs and prose descriptions. This is a hard blocker for implementation.

**Missing SQL:**
```sql
-- What should this actually be?
CREATE TABLE anansi_entities (
    ...
);
```

- **No foreign key relationship to ozone+ `messages` table:** The audit trail references `message_id` but doesn't specify if this is a foreign key or just a stored UUID. If it's a FK, what happens when ozone+ soft-deletes a message?
- **No index definitions:** With `max_entities = 32`, performance isn't critical, but queries like "find entity by slug" or "events for entity X" need indexes for correctness as the event trail grows
- **Namespace table column `registered_at` is `INTEGER`** — but §11.1 says `addon_version` is TEXT. What format? SemVer? Numeric? This matters for compatibility checks

**Recommendation:** Add complete SQL DDL as an appendix, including indexes, foreign keys, and example queries for the inspector.
</details>

<details>
<summary><strong>🔬 Extraction Pipeline — Grade: A</strong></summary>

**Strengths:**
- §13.4's extraction prompt template is *usable today* — not a vague description of what the prompt should do
- §13.5's normalization rules are deterministic and enumerated step-by-step
- §13.6's six outcome types cover all extraction states exhaustively
- §13.8's idempotency key mechanism is simple and correct
- §13.9's swipe/regenerate flow addresses the hardest edge case in the system
- §13.13's worked example with narrative → JSON → normalization → validation → mood update is invaluable
- §13.11's failure handling table covers every expected failure mode
- §13.12's quality gates provide concrete, testable rules

**Weaknesses:**
- §13.4's extraction prompt has `max(-10, min(Δ, 10))` but §18.1 says `delta_cap = 10` is configurable. The prompt hardcodes the range as `(-10 to +10)` — should reference the config value or note that template is regenerated on config change
- §13.5's normalization rule 9 says "If ambiguous (multiple partial matches): mark as DiscardedAmbiguous" — but what constitutes a "partial match"? Substring? Prefix? This is left undefined

**Gap:** No specification of *which* model performs extraction. §18.1 says `model = "same"` meaning the main LLM. This creates a subtle problem: the extraction prompt competes with the user's context window. For a session at 7K/8K tokens, firing a second inference call with the extraction prompt + narrative + entity registry may overflow context. Consider specifying:
- Extraction call uses a separate API call, not in-context injection
- Extraction prompt has its own token budget (e.g., 1K tokens max)
- Entity registry in extraction prompt is capped (e.g., 16 entities max with name + type only)
</details>

<details>
<summary><strong>🔌 Addon/Integration Architecture — Grade: A-</strong></summary>

**Strengths:**
- §9.2's trait definitions are complete Rust signatures, not prose descriptions
- §9.3's error propagation contract is unambiguous — no addon error ever aborts generation
- §9.4's execution ordering for multiple addons is specified (sequential hooks with state passing)
- §9.6's threading model with bounded `mpsc` channels is well-reasoned and consistent with ozone+'s architecture
- The `Arc<RwLock<World>>` rejection rationale demonstrates deep architectural understanding

**Weaknesses:**
- **No lifecycle hooks:** §9.2 defines `register_tools` and `deregister_tools` for `ToolProvider`, but there's no `on_session_load` / `on_session_unload` / `on_config_reload` lifecycle hook. How does Anansi know when to initialize its ECS world? When to flush pending state?
- **`PostGenerationHook` returns `Vec<ProposedChange>`** but §9.4 says hooks execute sequentially with state passing. This means Hook B sees Hook A's changes. What if Hook A and Hook B both propose changes to the same entity? Conflict resolution is undefined
- **`PaneProvider::render_compact` and `render_inspector` return `PaneContent`** — but what is `PaneContent`? A `String`? A `ratatui::Buffer`? A custom widget type? This is an opaque return type that needs definition

**Recommendation:** Add lifecycle trait:
```rust
pub trait AddonLifecycle: Send + Sync {
    fn on_session_load(&self, session: &SessionMeta) -> Result<(), AddonError>;
    fn on_session_unload(&self) -> Result<(), AddonError>;
    fn on_config_reload(&self, config: &AddonConfig) -> Result<(), AddonError>;
}
```
</details>

<details>
<summary><strong>⚔️ Combat System — Grade: B</strong></summary>

**Strengths:**
- §15.2's state machine is complete with all transitions specified
- §15.4's HP-at-zero resolution prevents entity deletion (good for auditability)
- §15.6's deterministic replay via seeded hashing is correct
- §15.7's degradation handling when tool-calling is unavailable is honest
- §15.8's manual controls provide essential safety valves

**Weaknesses:**
- **Damage formula `max(1, hostility / 3)` has a scaling problem:** With `delta_cap = 10`, hostility can be at most 10 after one extraction cycle. `10 / 3 = 3`. After 25 extraction cycles (hostility = 255), `255 / 3 = 85` damage per round. With default HP at 255 (u8), that's a 3-round kill. But with `base_magnitude = 3`, hostility grows slowly. The formula works for MVP but the math should be explicitly documented with examples at low, mid, and high hostility
- **No hit/miss mechanic:** Every attack hits. This is acceptable for MVP but creates a degenerate combat pattern where the higher-hostility entity always wins in fewer rounds. Consider adding a trivial miss chance (e.g., `if random(seed) < 0.1 then miss`) for variety without complexity
- **No self-healing or HP recovery mechanism:** Once HP drops, it can only be recovered via `/set-hp` manual command. This is fine for MVP but should be explicitly noted as a limitation
- **Tool call payload `{"attacker": "goblin-scout", "target": "elara"}`** uses slugs, but §13.5 says slugs are `lowercase, alphanumeric + hyphens`. The worked example uses `goblin-scout` but the extraction example in §13.13 creates `goblin scout` (with space). Slug generation from display name needs explicit rule

**Critical Gap:** §15.3 says `damage = max(1, attacker_hostility / 3)` but doesn't specify integer division. Rust's `/` operator on `u8` performs integer division. `2 / 3 = 0`, then `max(1, 0) = 1`. This is correct but should be stated explicitly to prevent implementation confusion.
</details>

<details>
<summary><strong>🚨 Error Handling — Grade: A-</strong></summary>

**Strengths:**
- §16.1's `AnansiError` enum covers all subsystems
- §16.2's severity/visibility/recovery table is actionable
- §16.3's integration with ozone+'s error framework prevents UX fragmentation
- Error variants include contextual fields (entity_id, turn, round) for debugging

**Weaknesses:**
- §16.1 uses `String` for `reason` fields — consider using structured error codes (e.g., `ExtractionReason::ParseFailure`, `ExtractionReason::Timeout`) for programmatic handling
- No `From<ozone_core::AddonError>` implementation specified — how do Anansi errors propagate through ozone+'s error framework?
- §16.1's `StateCorruption` has no defined recovery path beyond "session export recommended" — what triggers it? What specific state constitutes corruption?

**Recommendation:** Add corruption detection criteria:
```rust
// Corruption triggers:
// 1. Entity stat outside 0-255 range
// 2. Entity references non-existent ID in event trail
// 3. Combat state references defeated entity not in registry
// 4. Event trail turn numbers non-monotonic
```
</details>

<details>
<summary><strong>⚙️ Configuration System — Grade: B+</strong></summary>

**Strengths:**
- §18.1's TOML config is complete and copy-paste usable
- §18.2's semantics table clarifies ambiguous settings
- `stats.enabled` provides future flexibility without schema changes
- `extraction.model = "same"` is elegantly simple

**Weaknesses:**
- **No validation rules:** What happens if `delta_cap = 0`? If `max_entities = 0`? If `base_magnitude > delta_cap`? The config system should specify bounds and default-on-invalid behavior
- **No hot-reload specification:** Can `delta_cap` be changed mid-session? What happens to in-flight extractions? §18 doesn't address runtime mutability
- **`template_path = "extraction_prompt.txt"`** — is this relative to the config file? The working directory? `$XDG_CONFIG_HOME/ozone/templates/`? Path resolution needs specification

**Recommendation:** Add validation table:
| Setting | Valid Range | Default on Invalid |
|---------|------------|-------------------|
| `game.base_magnitude` | 1–50 | 3 |
| `game.delta_cap` | 1–100 | 10 |
| `game.max_entities` | 4–256 | 32 |
| `context.max_entities` | 1–`game.max_entities` | 8 |
| `context.max_tokens` | 100–2000 | 400 |
</details>

<details>
<summary><strong>🖥️ TUI/UX Specification — Grade: B-</strong></summary>

**Strengths:**
- §17.4's manual correction command table is comprehensive
- §17.1-17.3's compact pane + inspector dual-view is a solid UX pattern
- §17.3's inspector capability list covers all required information

**Weaknesses:**
- **No widget specifications:** How does the compact pane render? ASCII art? Unicode box-drawing? What happens when a stat value changes — is there animation or instant update?
- **No keybinding conflicts:** §17.4 defines `/anansi` and `/combat` slash commands, but ozone+ already has a command system. How do addon commands integrate with ozone+'s command palette?
- **No responsive behavior:** What does the pane look like at 80 columns? 120? The ozone+ doc (§21.2) specifies responsive layouts for the main UI but Anansi's pane doesn't reference these thresholds
- **No visual mockup for inspector:** The compact pane is described but not visualized. A concrete ASCII mockup would prevent implementation ambiguity

**Critical Gap:** §17.4 says commands are "routed through ozone+'s engine command channel" but doesn't specify how addon commands register with ozone+'s command system. Is there a `CommandProvider` trait (missing from §9)? Or do commands go through `ToolProvider`?

**Recommendation:** Add command registration trait:
```rust
pub trait CommandProvider: Send + Sync {
    fn register_commands(&self) -> Vec<CommandDefinition>;
    fn handle_command(&self, cmd: &str, args: &[&str]) -> Result<CommandResult, AddonError>;
}
```
</details>

<details>
<summary><strong>🧪 Testing Strategy — Grade: B</strong></summary>

**Strengths:**
- §23.4's test type categorization is appropriate
- Property-based testing for stat overflow and idempotency is well-chosen
- §6.2's success metrics provide real-world validation targets

**Weaknesses:**
- **No test infrastructure specification:** Where do tests live? `tests/` integration tests? Inline `#[cfg(test)]`? The workspace structure (§8) has a `tests/` directory but no guidance on test organization
- **No mock strategy:** How do you test extraction without a real LLM? Is there a mock extraction backend? JSON fixtures?
- **No CI specification:** §23.4 says "every commit" and "pre-merge" but doesn't specify how to run tests across the ozone+/anansi workspace boundary
- **No property-based test invariants listed:** "Stat overflow prevention" is mentioned but specific invariants (e.g., `forall stat, 0 <= stat <= 255` after any operation) aren't enumerated

**Recommendation:** Add test fixtures directory:
```
anansi/tests/
├── fixtures/
│   ├── extraction/
│   │   ├── elara_gratitude.json
│   │   ├── goblin_ambush.json
│   │   ├── malformed_output.json
│   │   └── ambiguous_references.json
│   ├── combat/
│   │   └── standard_rounds.json
│   └── entities/
│       └── initial_registry.json
├── unit/
├── integration/
└── property/
```
</details>

<details>
<summary><strong>🏗️ Code Architecture — Grade: A-</strong></summary>

**Strengths:**
- §8's five-crate workspace has clean responsibility boundaries
- §9.6's threading model with dedicated ECS thread and bounded channels is well-reasoned
- Dependency graph (§20) is acyclic with clear critical path
- Phase-to-tier mapping ensures each phase is independently testable

**Weaknesses:**
- **`anansi-core` / `anansi-game` split is ambiguous:** Both deal with "deterministic systems." Where does `StatBlock` live? Where does `Mood` resolution live? The description says core has "shared types" and game has "pure deterministic systems" but mood resolution is both a type and a system
- **`anansi-cli`'s purpose is vague:** "Startup wiring, registry registration, config merge, compatibility checks" — is this a binary crate? A library? How does it differ from `anansi-bridge`?
- **No public API surface specification:** What does each crate export? What's the public/private boundary?

**Recommendation:** Add visibility matrix:
| Crate | Public Exports | Internal Only |
|-------|---------------|---------------|
| anansi-core | `Entity`, `StatBlock`, `Mood`, `AnansiError` | ECS world setup, internal components |
| anansi-game | validation functions, combat resolution | fallback computation internals |
| anansi-bridge | addon trait implementations | channel management, extraction internals |
| anansi-tui | pane registration | rendering internals |
| anansi-cli | binary entrypoint | wiring logic |
</details>

<details>
<summary><strong>🔒 Security Model — Grade: C+</strong></summary>

**Strengths:**
- §4.9's single-writer guarantee prevents state corruption
- §11.1's namespace registration prevents table collisions
- §13.8's idempotency prevents replay attacks on state

**Weaknesses:**
- **No input sanitization:** §13.4's extraction prompt injects `{assistant_message_content}` directly into the prompt. If the LLM's narrative contains prompt-injection text (e.g., `IGNORE PREVIOUS INSTRUCTIONS`), the extraction model may be manipulated
- **No entity name validation:** §13.5 says slugs are `lowercase, alphanumeric + hyphens` but doesn't prevent names like `'; DROP TABLE anansi_entities; --` (SQL injection via display_name)
- **No extraction model isolation:** §18.1's `model = "same"` means the extraction prompt runs on the same model as narrative generation. A malicious prompt in narrative text could manipulate extraction output
- **No rate limiting on extraction:** If the LLM generates a message with 100 entity references, does extraction process all of them? §13.12's `max_entities` limit helps but doesn't address the inference cost

**Recommendation:** Add input sanitization rules:
```rust
fn sanitize_entity_name(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace() || *c == '-' || *c == '\'')
        .collect::<String>()
        .trim()
        .to_string()
}
```
</details>

<details>
<summary><strong>🔄 Migration & Upgrade Path — Grade: B-</strong></summary>

**Strengths:**
- §20's phase dependency graph is clear and correct
- §20's cross-workspace dependency warning for Phase 1D is appropriately flagged
- §19's MVP/Post-MVP separation is honest

**Weaknesses:**
- **No version compatibility matrix:** What version of ozone+ does Anansi v0.3 require? What happens if ozone+ upgrades its addon API? The stability rule (§9.7) says "versioned API changes" but doesn't specify a versioning scheme (SemVer? Calendar?)
- **No rollback strategy:** If Anansi v0.3.1 introduces a bug, can users downgrade to v0.3.0? What happens to data created by v0.3.1?
- **No data migration path:** When Post-MVP phases add new columns or tables, how does existing data migrate? The ozone+ doc (§13.1) has a migration framework but Anansi doesn't reference it

**Recommendation:** Add version compatibility table:
| Anansi Version | Minimum ozone+ Version | Addon API Version | Breaking Changes |
|---|---|---|---|
| 0.3.0 | 0.4.0 | 1.0 | Initial release |
| 0.4.0 | 0.5.0 | 1.1 | Added inventory (additive) |
| 1.0.0 | 0.6.0 | 2.0 | Stat schema pluggable (breaking) |
</details>

---

### Critical Implementation Blockers

These issues **must** be resolved before code can be written:

| # | Blocker | Section | Impact |
|---|---------|---------|--------|
| 1 | **No SQL DDL for anansi tables** | §11 | Cannot create persistence layer |
| 2 | **PaneContent type undefined** | §9.2 | Cannot implement PaneProvider |
| 3 | **Command registration mechanism missing** | §17.4 | Cannot register `/anansi` commands |
| 4 | **Addon lifecycle hooks missing** | §9.2 | Cannot initialize ECS world at session load |
| 5 | **No extraction token budget isolation** | §13.4 | Extraction may overflow context window |
| 6 | **Slug generation from display name underspecified** | §13.5, §15.5 | Entity identity resolution ambiguous |

---

### Non-Critical Improvements

These should be addressed but don't block implementation:

| # | Issue | Section | Priority |
|---|-------|---------|----------|
| 1 | `f32` sensitivity → consider fixed-point | §10.8 | Medium |
| 2 | Add corruption detection criteria | §16.1 | Medium |
| 3 | Add config validation bounds | §18.2 | Medium |
| 4 | Add input sanitization for entity names | §13.5 | High |
| 5 | Add test fixtures directory structure | §23.4 | Low |
| 6 | Add version compatibility matrix | §22 | Medium |

---

### Final Verdict

## **Grade: B+ (Implementation-Ready with 6 Blockers)**

**Ready for coding: CONDITIONAL YES**

The Anansi v0.3 design is **90% implementation-ready**. The architecture is sound, the extraction pipeline is exceptionally well-specified (the best subsystem in the document), and the error taxonomy demonstrates mature engineering thinking.

However, **6 critical blockers** prevent a developer from writing code today:

1. **Missing SQL DDL** — the persistence layer cannot be built without schema definitions
2. **Undefined `PaneContent` type** — TUI integration is blocked
3. **No command registration mechanism** — user-facing correction commands cannot be registered
4. **Missing addon lifecycle hooks** — ECS world initialization timing is unspecified
5. **No extraction token budget isolation** — extraction may destabilize generation context
6. **Underspecified slug generation** — entity identity resolution has an implementation gap

**Recommendation:** Resolve blockers 1–4 before starting Phase 1A. Blockers 5–6 can be addressed during Phase 1F (extraction pipeline) implementation.

**With these 6 items addressed, this document is an A-grade implementation specification.** The worked examples (§13.13, §15.5), deterministic confidence scoring (§14.3), and threading model (§9.6) are rare quality for a pre-implementation design.

---

*Analysis complete. Ready for blocker-resolution discussion or deeper dives into specific domains.*

************************************************************************************************************************************************************************************************

**Model:** MiniMax M2.7 (version 1.0.0)

---

# Anansi v0.3 — Technical Design Assessment

This is a technical audit of the v0.3 final design document. I evaluate each subsystem against the standard of "could a competent Rust developer implement this correctly without further design input." I assign letter grades with specific deficiencies identified.

---

## Overall Architecture

**Grade: A**

The system dependency diagram in the preamble (§0) is a genuine improvement. It establishes the execution flow clearly: generation → PostGenerationHook → extraction → validation → ECS update + audit → context injection. The single-writer guarantee via engine command channel (§4.9) is architecturally sound and correctly integrates with ozone+'s concurrency model.

The channel-based threading model (§9.6) with dedicated ECS thread is a defensible choice. The document explicitly rules out `Arc<RwLock<World>>` and justifies why fire-and-forget is rejected (single-writer guarantee). This level of threading analysis is appropriate for a pre-implementation document.

**Minor technical gap:** The document does not specify channel buffer sizes for the ECS thread communication. The design says "bounded `mpsc` channels" but no capacities are given. This matters for backpressure behavior under load.

---

## Addon Surface Specification

**Grade: A**

v0.3 provides full Rust trait signatures for all four capabilities (`ContextSlotProvider`, `PostGenerationHook`, `ToolProvider`, `PaneProvider`) with method signatures, return types, and error contracts. This is the most significant improvement over v0.2.

The error propagation contract (§9.3) is well-specified: each capability has a defined failure mode that never aborts generation or corrupts transcript. The execution ordering rules (§9.4) are clear (sequential hooks, sorted context slots, namespace collision rejection).

**Deficiency:** `PaneProvider::render_inspector` takes `selection: Option<EntityId>` but does not specify how the selection state is managed. Is selection stored in the `AddonStateSnapshot`? Is it a separate UI state outside Anansi's concern? This is a UI state ownership ambiguity.

**Deficiency:** The document does not specify lifecycle management for addon registration/deregistration. When does `deregister_tools()` get called — on session close, addon disable, or both? There's no explicit lifecycle diagram.

---

## Data Model

**Grade: A-**

The entity model is complete and well-specified. The relationship model is correctly stated as player-centric (dyadic, NPC→player only). The stat purpose mapping (§10.2) is explicit and prevents the common misinterpretation that high hostility auto-triggers combat.

Mood resolution is deterministic and fully specified with a priority rule (§10.7): `fear > hostility > anger > affection > trust`. This is a concrete algorithm, not vague guidance.

**Deficiency 1:** The sensitivity multiplier range is 0.0–2.0 (§10.8). With `delta_cap = 10`, a sensitivity of 2.0 means a proposed +10 delta becomes +20 before clamping. The interaction with the clamping formula (§14.5) is not traced end-to-end. A sensitivity > 1.0 can produce a `final_delta` larger than `delta_cap`, which is then clamped. This is correct but not explained.

**Deficiency 2:** The mood resolution rule says "if no stat ≥ 8: Mood::Cold." But the stat range is 0–255 (u8), not 0–10. The worked example in §13.13 interprets "Elara trust=7" as "just below threshold 8," implying a 0–10 normalized scale. But the actual `StatBlock` uses u8. The discrepancy between the u8 storage (0–255) and the 0–10 mood threshold is not reconciled. The normalization step (0–255 → 0–10 for comparison) is missing from the spec.

---

## Extraction Pipeline

**Grade: B+**

This is the most improved section from v0.2. The extraction prompt template is provided (§13.4), the normalization rules are fully enumerated (§13.5), the idempotency mechanism is defined (§13.8), and the worked example (§13.13) covers a full round-trip with actual narrative input.

The six outcomes (§13.6) are well-defined. The swipe/regenerate flow (§13.9) is specified with explicit state transitions. The quality gates (§13.12) are concrete and testable.

**Deficiency 1:** The extraction template uses a single-shot inference call. There is no specification for how template iteration happens — if extraction quality metrics (§6.2) indicate the template is failing, how does the team update the template? Is it a config file that can be hot-reloaded? Is there an A/B testing mechanism?

**Deficiency 2:** The document does not address the case of **concurrent extraction on adjacent turns**. If the user types rapidly and two `PostGenerationHook` callbacks fire on the same ECS world simultaneously from different tokio tasks, what happens? The channel round-trip ensures ordering at the engine level, but the extraction task itself may run in parallel against the same entity registry. This race condition is not addressed.

**Deficiency 3:** The confidence scoring formula (§14.3) is deterministic, which is good. However, the "per field" counting in `confidence_score` computation is ambiguous when multiple repairs apply to the same field. If a field requires both "type coercion" (−0.05) and "structural reparse" (−0.15), does the penalty stack to −0.20 or does the higher penalty dominate?

---

## Validation and Fallback Logic

**Grade: B+**

The interaction-direction table (§14.1) is now provided with example rules. This was a critical gap in v0.2.

The clamping formula (§14.5) is mathematically correct and prevents u8 overflow. The double-clamp (`max(-delta_cap, min(Δ, delta_cap))` then `max(0, min(255, S(t) + ...))`) is the right approach.

**Deficiency 1:** The repair severity scale (§14.4) is a good addition, but it has a gap: "Partial extraction" (−0.20) vs. "Complete fabrication" (−0.40) — what is the boundary between them? The document says "Extracted 2 of 3 expected fields" as an example, but extraction can propose 0 deltas validly (no relationship changes in the narrative). So partial extraction is not simply "fewer deltas than expected." The classification logic needs more specificity.

**Deficiency 2:** The direction table shows "unknown → any" deltas are halved. But the normalization rules (§13.5) do not include a step that marks direction as "known" or "unknown." The extraction JSON output does not include a direction field. So the direction classification must happen in validation, but there is no explicit rule for how the engine decides whether the direction is "known" or "unknown" from the extraction output.

---

## Combat System

**Grade: B**

The state machine (§15.2) is well-specified with explicit transition triggers. The deterministic replay seeding (§15.6) is correct. The manual controls (§15.8) are adequate.

**Deficiency 1 (significant):** The damage formula `damage = max(1, attacker_hostility / 3)` produces trivial damage in MVP. With hostility in the 0–10 range (typical for the six-stat model where values stay small), `hostility / 3` yields:
- hostility 0–2 → 0 damage → max(1, 0) = **1**
- hostility 3–5 → 1 damage → max(1, 1) = **1**
- hostility 6–8 → 2 damage → max(1, 2) = **2**
- hostility 9–10 → 3 damage → max(1, 3) = **3**

With typical NPC hostility values of 2–5 and HP of 10, it takes 5–10 rounds to defeat an entity. This is mechanically thin for a combat system. The document does not acknowledge this tension or provide a rationale for why this is acceptable for MVP.

**Deficiency 2:** The HP resolution at zero (§15.4) correctly states that entity deletion is never automatic. However, the defeated entity remains in the entity registry and is explicitly excluded from combat rounds. There is no specification for what happens to the defeated entity's relationship stats — do they persist? Are they frozen? This matters for post-combat narrative integration.

**Deficiency 3:** The combat example (§15.5) shows a goblin with hostility 9. With the damage formula, this is 3 damage per round. But the goblin's hostility (9) versus Elara's hostility (2) gives the goblin a 3:1 damage advantage per round. There is no defense or armor to balance this. The MVP combat is effectively "who attacks first wins."

---

## Error Taxonomy

**Grade: A**

The `AnansiError` enum covers all failure modes with specific variants for extraction, validation, combat, state, addon API, and bridge errors. Severity levels and user visibility are defined per error. Recovery paths are specified.

**Deficiency:** `ChannelClosed` is marked as critical with recovery "Anansi enters read-only mode." This is appropriate, but the design does not specify what "read-only mode" means operationally. Can the user still roleplay normally? Can they inspect state? Can they use manual correction commands? The TUI behavior for read-only mode is not defined.

---

## Persistence Design

**Grade: A-**

Namespace registration (§11.1) prevents collisions. The audit event trail schema (§11.3) is comprehensive with all necessary markers. Event type prefixing (§11.4) ensures Anansi events coexist cleanly with ozone+ events.

**Deficiency 1:** The cross-session import deferred decision (§11.5) is architecturally correct (defer until single-session loop is stable), but the document says "the current schema does not include this prefix but must not make it impossible to add." This is a vague guarantee. A concrete check: does the current `slug` field use a type that can accommodate a namespace prefix? If `slug` is `String`, yes. If it has a uniqueness constraint within the session, the migration path for cross-session import should be briefly described, not just promised as future-compatible.

**Deficiency 2:** The document does not specify database migration strategy for Anansi's own tables. If a future version of Anansi changes the entity schema (adds a new field, splits a table), how are migrations managed? ozone+ has a migration framework (§13.1 in v0.4 doc), but Anansi's own migration path is not specified.

---

## Context Injection

**Grade: A-**

Registering `[GAME STATE]` as a formal `ContextSlotProvider` with priority 50 (§12.2) is correct — it integrates with ozone+'s existing `ContextAssembler` rather than fighting it. The normative format (§12.4) is clear and both human- and LLM-readable.

**Deficiency 1:** The `ContextBudget` parameter in `provide_context(&self, budget: &ContextBudget)` is used to decide whether to return a block or `None`, but the document does not define the `ContextBudget` struct. It references ozone+'s budget system but does not specify what Anansi actually reads from it to make the omit decision.

**Deficiency 2:** The token estimation for the `[GAME STATE]` block uses a "hard soft-cap target: 400 tokens estimated" (§12.7). But ozone+'s token counting system (§15 in v0.4 doc) has a three-tier fallback chain. Does Anansi use the same estimation policy, or does it use a simpler heuristic? The interaction with ozone+'s context assembly is not specified.

---

## Concurrency Model

**Grade: A-**

The channel-based isolation between ozone+'s tokio runtime and Anansi's ECS thread is the right architecture. The reasoning against shared mutability is correct. The round-trip through the engine command channel preserves the single-writer guarantee.

**Deficiency 1:** Bounded channel capacities are not specified. In production, the channel between ozone-engine and anansi-bridge could become a bottleneck if extraction takes longer than the rate of new messages. What happens when the channel is full? Backpressure on the `PostGenerationHook` caller? Drop-and-log? This matters for reliability.

**Deficiency 2:** The "retry once after 100ms" for `WorldLockTimeout` (§13.11) is a reasonable default, but it is not configurable. In high-contention scenarios (many rapid corrections, multiple addons), this hardcoded value may be too short.

---

## Configuration System

**Grade: B+**

TOML structure is clear, config semantics are well-explained (especially the `base_magnitude = 3` explanation in §18.2), and disabled stats configuration is a thoughtful feature.

**Deficiency 1:** `base_magnitude = 3` is explained as "out of 0–255 stat range, ~1.2% shift." But for a stat that starts at 0 (baseline), a fallback delta of +3 per expected-change stat means multiple fallback deltas in the same turn can compound. If extraction fails and the engine applies base_magnitude=3 for trust and base_magnitude=3 for affection in the same turn, that's +6 net stat change from a single failed extraction. The compounding effect is not analyzed.

**Deficiency 2:** `max_entities = 32` is a hard cap with no specified user experience when reached. Does the TUI show a warning? Does extraction stop? Does the oldest entity get evicted? The behavior is undefined.

---

## Roadmap and Exit Criteria

**Grade: A-**

Phase-to-tier mapping (§20 Phase-to-tier mapping) and the dependency graph are clear and correct. Most exit criteria are concrete and testable.

**Deficiency 1:** Phase 1D is a cross-workspace dependency on `ozone-core`. The document correctly identifies this as a potential blocker ("if ozone+ development is on a separate timeline"). However, the roadmap does not provide a fallback plan if ozone-core changes are delayed. Can Anansi implement against a stub/interface to proceed with other phases?

**Deficiency 2:** Phase 1J exit criterion says "at least 20 turns of mixed narrative, extraction, correction, and combat." This is a qualitative threshold. What defines "mixed"? What defines "without state corruption"? These need to be automated test predicates, not a human judgment call.

**Deficiency 3:** The document does not specify how phases are tested independently. If Phase 1A passes its unit tests but Phase 1B integration with 1A fails, the milestone structure does not catch this until Phase 1J.

---

## Risk Register

**Grade: A**

Comprehensive. The addition of "Single-writer violation" and "Threading deadlock" risks shows the document is thinking about implementation-level failures, not just product risks.

**Minor gap:** "Context budget competition" is listed as a risk with mitigation (priority-based context layer system). But the mitigation relies on the priority being 50 (§12.2), which is a fixed number in the code. If ozone+ adds a new context layer with priority 51–55, Anansi's `[GAME STATE]` drops further. This should be flagged as a future compatibility risk.

---

## Summary Grades

| Area | Grade | Key Issue |
|------|-------|-----------|
| Overall Architecture | A | Channel capacities unspecified |
| Addon Surface | A | Lifecycle management, selection state ownership |
| Data Model | A- | u8 vs 0-10 mood scale mismatch, sensitivity stacking |
| Extraction Pipeline | B+ | Concurrent extraction race, template iteration |
| Validation & Fallback | B+ | Partial extraction boundary, direction classification |
| Combat System | B | Damage formula underpowered, no defense |
| Error Taxonomy | A | Read-only mode TUI behavior unspecified |
| Persistence | A- | Migration strategy, cross-session slug format |
| Context Injection | A- | ContextBudget struct not defined, token estimation alignment |
| Concurrency | A- | Channel capacities, hardcoded retry timeout |
| Configuration | B+ | Fallback delta compounding, max_entities overflow behavior |
| Roadmap | A- | Phase 1D cross-workspace fallback, Phase 1J testability |
| Risk Register | A | Future priority collision not flagged |

---

## Final Assessment: Ready for Coding?

**Verdict: Conditionally Ready — B+ Technical Readiness**

Anansi v0.3 is substantially more implementation-ready than v0.2. The critical gaps identified in the prior review — extraction prompt, mood resolution model, combat state machine, addon trait signatures, error taxonomy — have all been addressed with sufficient specificity.

### Why Not an A

Three technical issues prevent an "A" readiness rating:

1. **Combat damage is mechanically trivial.** The `hostility / 3` formula produces 1–3 damage per round with typical values. Combined with no defense model, combat resolves as a damage race with no tactical depth. This may be intentional for MVP simplicity, but the design does not acknowledge this as a known limitation or provide criteria for when it should be revisited.

2. **Concurrent extraction on the same ECS world is underspecified.** The channel round-trip protects the engine's single-writer guarantee, but the background extraction task itself is described as a single sequential flow. In practice, if the user types rapidly and two messages commit close together, two `PostGenerationHook` callbacks fire. The second extraction reads stale ECS world state (from before the first extraction's changes are committed). The idempotency key prevents duplicate application, but it does not prevent the second extraction from reading stale entity state and proposing deltas based on it.

3. **Phase 1D cross-workspace dependency has no contingency.** If ozone-core development is on a separate timeline, Anansi cannot proceed past Phase 1C. The document identifies this risk but does not provide a mitigation path (e.g., stub interfaces, feature-gated implementation).

### What Must Be Resolved Before Implementation

In priority order:

| Priority | Issue | Resolution Required |
|----------|-------|---------------------|
| P0 | Phase 1D coordination plan | Define how Anansi implements against ozone-core addon traits if ozone-core is on a separate release cycle. A stub-and-impl approach or a feature-gated implementation plan is needed. |
| P0 | Combat damage formula | Either justify the current formula as intentionally minimal and document the balance target (e.g., "3–5 rounds to defeat a standard NPC"), or introduce a higher base damage (e.g., `1 + attacker_hostility / 2`). |
| P1 | Concurrent extraction isolation | Specify whether extraction tasks are queued (only one in flight per session) or allow parallel extraction with merge-at-commit. The idempotency key handles deduplication but not stale-read. |
| P1 | ContextBudget struct | Define or reference the `ContextBudget` struct that `ContextSlotProvider::provide_context` receives. Anansi cannot implement context contribution without knowing what the budget contains. |
| P2 | Mood scale normalization | Define explicitly: is the u8 stat range (0–255) normalized to 0–10 before the mood threshold comparison (≥ 8)? If so, what is the normalization function? |
| P2 | Read-only mode TUI spec | Define what the user sees and can do when Anansi enters read-only mode (§16.2 recovery for `ChannelClosed`). |

### What Is Ready Without Changes

These areas are implementation-ready as written:
- Entity CRUD and normalization rules (§10, §13.5)
- Mood resolution algorithm (§10.7)
- Addon trait signatures and registry (§9.2–9.5)
- Error taxonomy and severity levels (§16)
- Audit event trail schema (§11.3)
- Namespace registration (§11.1)
- Config schema and semantics (§18)
- Swipe/regenerate flow and idempotency (§13.8–13.9)
- Interaction-direction table (§14.1)
- Confidence scoring formula (§14.3)
- Phase 1A–1C, 1F–1I exit criteria (concrete and testable)

### Recommendation

**Do not begin with Phase 1D.** Phase 1D is a cross-workspace blocker and needs coordination with the ozone+ team before Anansi can proceed. Begin with Phases 1A, 1B, and 1C in parallel — these are self-contained and have clear exit criteria. Phase 1D should be resolved first, then Phases 1E–1J can follow sequentially.

The document is in good shape. The remaining gaps are specific and resolvable without architectural restructuring.




************************************************************************************************************************************************************************************************


# Technical Analysis of Anansi Design Plan v0.3

**Model:** Trinity Large Thinking  
**Version:** 2025-04-13  
**Analysis Focus:** Technical depth, implementation readiness, architectural soundness

---

## Overall Assessment

The Anansi v0.3 design represents a **significant improvement** over v0.2, transforming vague concepts into concrete, implementable specifications. The document successfully addresses major gaps identified in multi-model reviews, particularly around extraction, combat state machine, and addon contracts. The architecture is coherent, the MVP boundaries are well-defined, and the integration with ozone+ v0.4 is clearly specified.

**Final Verdict:** **READY FOR CODING** (with minor clarifications needed before implementation begins)

---

## Graded Technical Areas

### 1. Architecture & Design Principles — **A-**

**Strengths:**
- Clear separation of concerns with well-defined tiers (A-D)
- Strong foundational principles (LLM proposes/engine decides, canonical transcript sacred)
- Single-writer architecture through ozone+ engine command channel is correctly enforced
- Channel-based concurrency model is robust and testable
- Addon surface is properly versioned and stable

**Weaknesses:**
- The ECS isolation thread model adds complexity; need to ensure proper error handling across thread boundaries
- Some edge cases around channel backpressure not fully specified
- The "skip, not rollback" principle for swipes is sound but requires careful implementation to avoid state drift

**Recommendation:** The architecture is technically sound but requires careful implementation of the bridge between tokio and ECS thread. Add explicit timeout and retry logic for channel operations.

---

### 2. Data Model & Entity Management — **A**

**Strengths:**
- Fixed six-stat block is well-justified for MVP
- Entity normalization rules are explicit and deterministic
- Mood resolution algorithm is clearly defined and deterministic
- Sensitivity multipliers provide needed flexibility
- Audit trail with prefixed event types is comprehensive
- Namespace registration prevents collisions

**Weaknesses:**
- Cross-session identity resolution is deferred but the schema doesn't include namespace prefix yet
- Entity slugs are session-scoped by default; future cross-session import will require schema change
- The mood resolution thresholds (8/10) are arbitrary; should be configurable

**Recommendation:** Add a `namespace` field to `Entity` now (even if unused in MVP) to ease future cross-session import. Make mood thresholds configurable via `[anansi.game.mood_threshold]`.

---

### 3. Extraction Pipeline — **B+**

**Strengths:**
- Clear conceptual split between entity discovery and relationship extraction
- Extraction prompt template is well-structured and provides necessary context
- Normalization rules are explicit and deterministic
- Six defined outcomes provide good auditability
- Idempotency mechanism prevents duplicate processing
- Quality gates with confidence scoring are deterministic and auditable

**Weaknesses:**
- The extraction model is still "same as main model" — may not be optimal for extraction tasks
- No fallback to a simpler extraction model if main model fails
- The prompt template may need iteration; no mechanism for A/B testing different templates
- Ambiguous entity resolution is discarded rather than flagged for user review

**Recommendation:** Add a configurable fallback extraction model (e.g., a smaller, faster model) for when the main model fails. Consider adding a "review ambiguous entities" command in the inspector.

---

### 4. Validation & Fallback Logic — **A-**

**Strengths:**
- Interaction-direction table is comprehensive and well-documented
- Explicit audit markers for every applied delta
- Deterministic confidence scoring formula
- Clamping formula prevents overflow
- Degraded mode with halved deltas is a good compromise

**Weaknesses:**
- The confidence scoring formula may be too complex; consider simplifying
- No mechanism to learn from manual corrections to improve future extraction
- The "base_magnitude" fallback value is arbitrary; should be configurable per stat

**Recommendation:** Add a `correction_feedback` mechanism that records when manual corrections occur, to inform future extraction template improvements. Make `base_magnitude` configurable via `[anansi.game.base_magnitude]`.

---

### 5. Combat System — **A-**

**Strengths:**
- State machine is clearly defined with explicit transitions
- Deterministic replay via seeded resolution is correct
- Damage calculation is simple and predictable
- HP resolution at zero is handled gracefully
- Manual controls are minimal and appropriate

**Weaknesses:**
- No defense stat; damage may be too high if hostility values are high
- The `/combat start` command is manual; no way to auto-resolve narrative combat
- The combat tool unavailability handling is good but needs UI indicators

**Recommendation:** Add a simple defense stat (e.g., `armor`) in a future phase. Consider adding an "auto-combat" toggle that uses the same deterministic system but without user intervention.

---

### 6. Error Handling & Audit Trail — **A**

**Strengths:**
- Comprehensive error taxonomy with severity levels
- All errors are logged and recorded in audit trail
- No addon error ever aborts generation
- Prefixed event types prevent collisions
- Namespace registration prevents table conflicts

**Weaknesses:**
- Some errors (e.g., `StateCorruption`) lack clear recovery procedures
- The `WorldLockTimeout` error needs more detailed retry logic
- No circuit breaker pattern for repeated extraction failures

**Recommendation:** Add a circuit breaker that disables extraction after 3 consecutive failures, requiring manual reset. Provide clearer recovery steps for `StateCorruption` (e.g., "export session and contact support").

---

### 7. TUI & User Interaction — **B+**

**Strengths:**
- Compact pane and inspector provide good visibility
- Manual correction commands are comprehensive
- Context slot registration with priority ensures proper budgeting
- `[GAME STATE]` format is clear and structured
- Keybindings are sensible and configurable

**Weaknesses:**
- The inspector may become cluttered with many entities; need pagination or filtering
- No mention of mobile/terminal resizing handling
- The "dry-run" context command (`Ctrl+D`) is useful but may be confusing to new users

**Recommendation:** Add entity filtering in inspector (by type, mood, recent activity). Implement responsive layout that adapts to terminal size. Consider adding a "context preview" command that shows what will be sent without actually sending.

---

### 8. Integration with Ozone+ — **A**

**Strengths:**
- Addon surface is properly defined with four capabilities
- All mutations flow through engine command channel
- Context slot registration integrates with ozone+'s assembler
- Tool provider registration is clean
- Pane provider integration is well-specified

**Weaknesses:**
- The bridge between tokio and ECS thread needs careful synchronization
- The `HardwareResourceSemaphore` is mentioned but not fully specified
- No explicit handling of ozone+ version compatibility

**Recommendation:** Add explicit version compatibility checks in `anansi-cli`. Fully specify the `HardwareResourceSemaphore` with priority classes and timeout behavior.

---

### 9. Testing Strategy — **B**

**Strengths:**
- Unit tests for entity operations, stat clamping, mood resolution
- Property-based tests for overflow prevention, idempotency
- Integration tests for extraction → validation → persistence
- End-to-end tests for full session loop

**Weaknesses:**
- No mention of fuzz testing for extraction pipeline
- No performance benchmarks defined
- No accessibility testing mentioned
- The metric collection is good but needs automated reporting

**Recommendation:** Add fuzz testing for extraction prompt templates. Define performance targets (e.g., extraction < 500ms, context assembly < 200ms). Add accessibility tests for `--plain` mode. Implement automated metric collection and reporting to a dashboard.

---

### 10. Overall Readiness — **A-**

**Implementation Priority Order:**
1. **Phase 1A-1C**: Core types, deterministic systems, persistence — foundation
2. **Phase 1D**: Addon API in ozone-core — hard dependency for rest
3. **Phase 1E-1F**: Context contribution, extraction pipeline — critical path
4. **Phase 1G-1H**: Manual correction, combat tool — user-facing features
5. **Phase 1I-1J**: TUI, integration hardening — polish and testing

**Critical Path Items:**
- The addon API in ozone-core (Phase 1D) is a cross-workspace dependency that must be coordinated
- The extraction pipeline (Phase 1F) is the highest-risk component and should be iterated with prompt template testing
- The bridge between tokio and ECS thread (Phase 1C) needs careful implementation to avoid deadlocks

**Risk Mitigation:**
- Implement the extraction pipeline with extensive logging and metrics from day one
- Build the combat system with a "simulation mode" that doesn't affect state until validated
- Add comprehensive undo/redo support early to allow safe experimentation

---

## Final Coding Readiness Checklist

| Item | Status | Notes |
|------|--------|-------|
| Architecture finalized | ✅ | Clear, implementable, well-documented |
| Data model defined | ✅ | Schema ready, normalization rules explicit |
| Extraction pipeline specified | ✅ | Prompt template, outcomes, confidence scoring |
| Validation logic complete | ✅ | Direction table, clamping, audit markers |
| Combat system defined | ✅ | State machine, damage calculation, replay |
| Error handling comprehensive | ✅ | Taxonomy, severity, user visibility |
| TUI surfaces specified | ✅ | Compact pane, inspector, commands |
| Ozone+ integration clear | ✅ | Addon traits, command channel, context slot |
| Testing strategy outlined | ⚠️ | Missing fuzz testing, performance benchmarks |
| Success metrics defined | ✅ | Manual correction frequency, discard rates |
| MVP boundaries firm | ✅ | No scope creep, clear post-MVP phases |

**Conclusion:** The Anansi v0.3 design is **technically mature** and **ready for implementation**. The architecture is sound, the specifications are detailed, and the MVP boundaries are well-enforced. The only gaps are in testing strategy (fuzz testing, performance benchmarks) and some minor clarifications needed for the ECS/tokio bridge and hardware semaphore.

**Recommendation:** Proceed with implementation. Start with Phases 1A-1C to establish the foundation, then coordinate with ozone+ team on Phase 1D. Implement extraction pipeline with extensive metrics logging from the beginning.


**********************************************************************************************************************************************************************************************


**Model: Gemini 3.1 Pro Preview**

# Anansi v0.3 Technical Design Analysis

Anansi v0.3 systematically resolves the architectural gaps identified in earlier drafts. By heavily detailing the concurrency model, the exact traits of the `ozone+` addon surface, and the mathematical boundaries of the game state, this document transitions from a strong conceptual blueprint to a rigorous, engineer-ready specification. 

Below is the technical evaluation, letter grades by domain, and final readiness verdict.

---

## 1. Domain Grading

### **Architecture & Concurrency: A**
**Strengths:** The decision to isolate the `bevy_ecs` World in a dedicated thread owned by `anansi-bridge`—communicating with `ozone+`'s `tokio` async runtime via bounded `mpsc` channels—is excellent. It avoids the catastrophic lock contention of an `Arc<RwLock<World>>` and strictly enforces the single-writer principle required by `ozone_engine`. 
**Feedback:** You must ensure that the `ApplyAddonStateDelta` command correctly awaits the `AddonStateApplied` confirmation before allowing the next generation to start, otherwise, `[GAME STATE]` could be injected with a one-turn delay under heavy load.

### **State Management & Idempotency: A+**
**Strengths:** The swipe/regenerate flow is completely solved. By using the idempotency key formula $Key = \text{hash}(N_{turn}, ID_{msg})$, Anansi cleanly avoids complex reverse-delta delta coding. If a state fails the key check, it skips extraction. 

### **API & Integration Contracts: A-**
**Strengths:** The `Unified Addon Surface` is thoroughly defined. Trait signatures like `PostGenerationHook` and `ContextSlotProvider` give concrete type shapes (`PostGenerationContext`, `ContextSlotMetadata`) that any future addon can implement.
**Feedback:** The `ContextBudget` struct passed to `provide_context` isn't explicitly defined. It must include both the *hard limit* and the *current consumption* so Anansi can decide whether cutting entities is necessary *before* returning the string.

### **Error Handling & Math Boundaries: A**
**Strengths:** The error taxonomy (`AnansiError`) maps well to user-visible vs. internal states. The mathematical boundaries are airtight, explicitly defining the clamping function to prevent `u8` integer overflow before returning to the ECS.

---

## 2. Technical Drill-Down

<details>
<summary><strong>► Expand for Mathematical & Logical Verifications</strong></summary>

### 1. Clamping Arithmetic 
The document defines the clamping logic verbally and simply in Section 14.5. To be technically rigorous in code, specifically considering the sensitivity multiplier ($\mu_{sens}$), the implemented function must evaluate as:
$$ \Delta_{final} = \max(-\delta_{cap}, \min(\lfloor \Delta_{raw} \times \mu_{sens} \rceil, \delta_{cap})) $$
$$ S_{t+1} = \max(0, \min(255, S_t + \Delta_{final})) $$
Because $S$ is a `u8`, the intermediate sum $S_t + \Delta_{final}$ must be upcast to a signed `i16` before the final clamp to prevent a panic on negative boundaries.

### 2. Confidence Scoring Algorithm
Section 14.3 converts subjective confidence into a deterministic float. Let $C$ be the confidence score:
$$ C = 1.0 - (0.1 \times N_{norm}) - (0.2 \times N_{sub}) - (0.3 \times N_{ambig}) - (0.4 \times N_{fab}) $$
This ensures that $C \in [0.0, 1.0]$ down to arbitrary floors. The implementation must ensure $C$ is clamped via $\max(0.0, C)$ so negative confidence doesn't cause downstream serialization issues.

### 3. Combat Damage Floor
Section 15.3 defines damage as:
$$ \text{Damage} = \max(1, \lfloor \text{Hostility} / 3 \rfloor) $$
Given Hostility $H \in [0, 255]$, damage scales $D \in [1, 85]$. The document correctly notes that in MVP, $H$ will typically be $[0, 10]$, keeping damage low. 

</details>

<details>
<summary><strong>► Expand for Remaining Implementation Risks</strong></summary>

### 1. Extraction Prompt Token Cost
Section 13.4 outlines the extraction prompt. While Anansi's `[GAME STATE]` output is strictly bounded to $400$ tokens, the *input* required to run the extraction inference (the System Prompt + the Assistant's Message + the Entity Registry Summary) could easily cost $1000+$ tokens per turn. If the user is on a highly constrained local GPU (e.g., 8GB VRAM), running this secondary inference call every turn could introduce a 2-4 second latency block.

### 2. LLM JSON Fabrication
Even with prompt engineering, local models (especially those under 14B parameters) struggle with strictly adhering to JSON schemas. If `anansi_bridge` relies solely on `serde_json::from_str`, it will hit the `DiscardedMalformed` error frequently. 
**Mitigation:** The implementation should consider using a constrained grammar (like `llama.cpp`'s GBNF grammars) specifically during the extraction inference to force the output into valid JSON.

### 3. Asynchronous Extraction Hook
The document states extraction is "asynchronous relative to the user's ability to continue typing" (§13.1), but also says "results must be validated by the engine before persisting." If a user types very fast or generates short responses, the `PostGenerationHook` queue could back up. The channel receiver in `anansi-bridge` must process these in FIFO order to prevent race conditions on $S_t$.

</details>

---

## 3. Final Verdict

### **Verdict: READY FOR CODING**

Anansi v0.3 is an exceptionally mature design document. It successfully marries a non-deterministic process (LLM roleplay) with a strictly deterministic backend (`bevy_ecs`).

By explicitly defining the error taxonomies, the cross-thread channel architecture, the API traits in `ozone-core`, and the exact rules for mathematical clamping and UI degradation, all ambiguity has been removed from the developer's critical path. 

You can hand this document directly to a Rust systems engineer to begin scaffolding `anansi-core` and `anansi-bridge` without needing to block on architectural decisions. The testable phases defined in Section 20 form a perfect sprint backlog.


`Qwen3.6 Plus`

---

## 1. Technical Analysis & Letter Grades

The v0.3 specification successfully bridges the gap between architectural intent and implementation-ready detail. The following grades reflect how well each subsystem is specified for a Rust implementation targeting `ozone+ v0.4`.

| Technical Area | Grade | Rationale |
|:|:|:|
| **Addon API Surface & Integration Contracts** | **A+** | Trait lifecycles, error contracts, execution ordering, and channel-based threading are explicitly defined. The decision to route all mutations through `ConversationEngine` perfectly preserves `ozone+`'s single-writer guarantee. |
| **Context Assembly Integration** | **A** | Registering `[GAME STATE]` as a formal `ContextSlotProvider` with priority/budget collapse is the single best decision in v0.3. It eliminates prompt pollution risks and aligns cleanly with `ozone-core`'s budget allocation algorithm. |
| **Extraction Pipeline & Identity Resolution** | **A** | Deterministic confidence scoring ($C_{score}$), explicit normalization rules, idempotency via $\text{hash}(turn, message\_id)$, and strict swipe deduplication provide a robust, testable foundation. |
| **Validation & Fallback Logic** | **A** | The interaction-direction table, double-clamping formula ($S_{t+1} = \max(0, \min(255, S_t + \max(-D, \min(\Delta, D))))$), and repair severity scale remove all ambiguity around stat mutations. |
| **Combat State Machine** | **A-** | Clean $Idle \rightarrow CombatActive \rightarrow CombatResolved$ topology. Manual-only initiation prevents extraction drift. Minor ambiguity exists around combat round counting vs. Ozone turn numbering for the deterministic seed (see §2.4). |
| **Persistence, Audit & Error Taxonomy** | **A** | Namespaced table registration, `anansi.` event prefixes, comprehensive audit markers (`proposal_source`, `degraded_mode`, `fallback_magnitude`), and clearly mapped error severities make debugging trivial. |
| **Threading & Concurrency Model** | **A** | Explicitly rejecting `Arc<RwLock<World>>` in favor of bounded `mpsc` channels between `tokio` and the isolated ECS thread is architecturally sound and prevents lock contention during generation. |
| **TUI & Config Semantics** | **B+** | Command routing and inspector requirements are clear. Config semantics properly gate stats without breaking the fixed `StatBlock` struct. Slightly underspecified on TUI state synchronization latency post-correction. |
| **Roadmap & Dependency Graph** | **A** | The explicit mapping of phases to tiers, clear exit criteria, and identification of Phase 1D as a cross-workspace hard blocker prevents integration paralysis. |

---

## 2. Technical Strengths & Weaknesses

### 2.1 Strengths (Preserve these during implementation)
1. **Channel-Isolated ECS World:** Running `bevy_ecs` on a dedicated thread and communicating via `mpsc` channels decouples Anansi's system scheduling from `ozone+`'s `tokio` runtime. This makes the world update deterministic and testable without fighting async/await boundaries.
2. **Deterministic Confidence Formula (§14.3):** Replacing subjective "high/medium repair" language with a deterministic penalty formula:
   $$C_{score} = 1.0 - \sum w_i \cdot r_i$$
   where $r_i$ is the count of repair operations and $w_i$ is the penalty weight. This guarantees that identical extraction outputs always yield the same confidence label, enabling snapshot testing.
3. **Skip-Not-Rollback with Idempotency:** Using $\text{hash}(turn\_number, message\_id)$ to deduplicate extraction cycles during swipe flows is far more resilient than attempting reverse-delta computation, which is notoriously fragile in ECS systems.
4. **Strict Single-Writer Routing:** Requiring all manual corrections and combat outcomes to route through `ApplyAddonStateDelta` to the `ConversationEngine` preserves WAL transactionality, enables standard `ozone+` undo/redo, and keeps the audit trail append-only.

### 2.2 Weaknesses & Implementation Friction
1. **Combat Seed Granularity:** The spec defines the replay seed as $\text{hash}(session\_id, turn\_number, round\_number, message\_id)$. However, a single Ozone turn might contain multiple combat actions, or combat rounds might span multiple turns if the user manually triggers `/attack`. 
   **Fix:** Decouple combat seeding from Ozone turns. Use a monotonic `combat_sequence_counter` maintained in `anansi_combat_state`:
   $$\text{seed} = \text{hash}(session\_id, combat\_id, sequence\_counter)$$
2. **Channel Request/Response Pattern:** §9.6 specifies `mpsc` channels but doesn't detail how `anansi-bridge` gets synchronous answers back from ozone's `ConversationEngine` before proceeding.
   **Fix:** Use a `oneshot` channel inside the command payload:
   ```rust
   struct ApplyAddonStateDelta {
       addon_id: String,
       delta: StatDelta,
       response_tx: tokio::sync::oneshot::Sender<Result<(), OzoneError>>,
   }
   ```
   This avoids blocking the ECS thread while waiting for engine acknowledgment.
3. **Confidence Field Definition:** §14.3 mentions "0.1 per field that required name normalization". The term "field" is ambiguous. Does it mean per-entity, per-delta, or per-stat? 
   **Fix:** Clarify as "per resolved delta or entity reference."
4. **ECS Query Overhead in Context Assembly:** `ContextSlotProvider::provide_context` is called every turn before generation. Querying `bevy_ecs` to format 8 entities and 3 recent changes could introduce latency if unoptimized.
   **Fix:** Cache the formatted `[GAME STATE]` string in `anansi-bridge` and invalidate it only on `World` dirty-flag changes during extraction or combat.

---

## 3. Flow & Ambiguity Assessment

### 3.1 Logical Flow
The document flows logically from **intent → architecture → contracts → data → pipeline → validation → combat → UI → config → roadmap**. The progression mirrors the actual dependency graph of the codebase. The addition of the System Dependency Overview at the top and the Phase Dependency Graph in §20 creates clear mental models for the implementation phase.

### 3.2 Remaining Ambiguities (Pre-Coding Resolution List)
| Location | Ambiguity | Required Clarification |
|:|:|:|
| §14.3 Confidence Formula | Definition of "per field" penalty | Specify: "per delta or entity reference in the extraction payload" |
| §15.5 Combat Seed | Overloaded use of `turn_number` | Switch to `combat_sequence_counter` for replay determinism |
| §9.6 Channel Model | How ECS thread receives `ApplyAddonStateDelta` confirmation | Document `oneshot` response pattern inside the command payload |
| §13.2 Eligibility | "extension-authored combat notation" format | Define exact delimiter syntax (e.g., `[COMBAT:...]` or `<!--anansi-combat-->`) to skip during extraction |
| §17.4 Manual Commands | Undo/redo interaction | Clarify that `/anansi set` pushes to ozone's undo stack as a single grouped action |

**Verdict on Ambiguity Reduction:** v0.3 reduces ambiguity by approximately $90\%$ compared to a standard software design spec. The remaining $10\%$ are implementation-level details (channel payloads, exact seed inputs) that can be finalized during the Phase 1A/1B scaffolding without blocking architecture.

---

## 4. Final Verdict

### 🟢 READY FOR CODING (With Minor Pre-Flight Adjustments)

Anansi v0.3 is an **exceptionally strong baseline**. It successfully transitions from conceptual architecture to an engineering specification. The explicit addon contracts, deterministic confidence scoring, channel-isolated concurrency model, and strict single-writer routing address the historical failure points of LLM-driven state management.

**Required Pre-Coding Actions (2-4 hours):**
1. **Define the `oneshot` response pattern** in `ozone-core/Command` variants to ensure the ECS thread can await engine persistence without blocking.
2. **Clarify "per field"** in the confidence penalty table to mean "per extracted delta or entity reference."
3. **Replace `turn_number`** with `combat_sequence_counter` in the deterministic replay seed formula.
4. **Specify the exact skip-delimiter** for extension-authored combat notation so extraction regex/fencing is deterministic.

Once these four items are noted in the issue tracker or a `TECHNICAL_NOTES.md` file, you can begin Phase 1A with high confidence. The dependency tree is stable, the non-goals are hardened, and the exit criteria for every phase are testable.

**Ship it.**
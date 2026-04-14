**Model:** GLM 5.1  
**Version:** 2025.04.13

---

# Anansi v0.2 — Design Analysis & Ozone Product Line Fit Assessment

---

## I. Product Line Positioning Analysis

### The Family Hierarchy

The Ozone product line establishes a clear tiering model:

```
ozonelite  →  ozone  →  ozone+  →  anansi
(lean)       (tuned)   (full RP)  (mechanical RP)
```

Anansi's positioning as a **dependent build** on ozone+ is architecturally sound and strategically important. Here's why it works:

| Aspect | Assessment |
|--------|-----------|
| **Dependency direction** | Correct — unidirectional. Anansi depends on ozone+, never the reverse. |
| **Market segmentation** | Clean — users who want narrative-only RP stay on ozone+; users who want mechanics layer on Anansi. No feature confusion. |
| **Addon vs. fork** | The v0.2 revision correctly moves from "ozone+ is unaware of Anansi" to "ozone+ is Anansi-agnostic with a general addon surface." This is the right call. |

### The Critical Design Decision: Unified Addon Surface

The shift from three separate traits + hidden UI exception to a **four-capability unified surface** is the single most important architectural decision in this document. It means:

1. **ozone-core owns the contract** — not Anansi, not ozone+
2. **Future addons get the same surface** — no special cases
3. **Version stability is enforced** — API changes are versioned, not casual refactors

This is well-designed, but there's a gap I'll detail in the weaknesses section.

---

## II. Strengths Assessment

### S1. MVP Discipline Is Genuine and Well-Enforced

This is Anansi's strongest quality. The document doesn't just say "we're keeping MVP small" — it provides **concrete enforcement mechanisms**:

- Hard non-goals with explicit rejection language (§3.1)
- Deferred-not-rejected list with "must not produce MVP complexity" guard clause (§3.2)
- Scope tier rule: "Tier A must remain useful even if Tier B and C are disabled" (§5)
- MVP success defined by **trustworthiness**, not feature count (§6)
- Practical rule: "If a proposed feature adds more state categories than it adds recovery and clarity, it probably belongs after MVP" (§22.3)

This is not performive minimalism. It's genuinely wired into the design.

### S2. The Core Architectural Promise Is Sound

> **The LLM proposes. The engine decides. The user can inspect and correct the result.**

This single sentence is load-bearing. It establishes:

- **Authority boundary**: LLM never has write access to game state
- **Transparency contract**: every mutation is inspectable
- **Recovery guarantee**: user correction is first-class, not bolted on

The downstream principles (§4.1–4.8) all derive cleanly from this promise. There are no contradictions.

### S3. The Extraction Outcome Taxonomy Is Excellent

The addition of explicit extraction outcomes (§13.4):

```
Accepted | AcceptedDegraded | DiscardedMalformed | DiscardedAmbiguous |
SkippedAlreadyProcessed | SkippedLockContention
```

This is a meaningful improvement over "parsed or didn't parse." It gives the audit trail real diagnostic value and makes `DiscardedAmbiguous` vs. `DiscardedMalformed` a genuinely useful distinction during debugging.

### S4. Combat Degradation Is Honest

The design refuses to fake deterministic combat when tool-calling is unavailable (§15.2). The UI must show that combat tooling is unavailable rather than silently pretending. This is the right call — silent degradation in a deterministic system destroys trust.

### S5. Manual Correction Is First-Class

The v0.2 revision correctly identifies that without a full inspector and correction commands, the compact pane would feel arbitrary and opaque. The minimum inspector capabilities (§16.3) and command list (§16.4) are the single biggest trust improvement in the revision.

### S6. Skip-Not-Rollback Is Pragmatic

Given the swipe/regenerate architecture of ozone+, attempting automatic rollback of extraction effects would be fragile. The "skip, not rollback" principle (§4.6) combined with manual correction tools is the correct trade-off for MVP.

---

## III. Weaknesses Assessment

### W1. The Addon Surface Contract Is Underspecified

The unified addon surface (§9) is the right idea, but the specification has gaps:

**Problem 1: No lifecycle contract.** The four capabilities (`ContextSlotProvider`, `PostGenerationHook`, `ToolProvider`, `PaneProvider`) are listed but never defined with method signatures or contracts. What does `PostGenerationHook` receive? What can it return? Can it mutate state? Can it veto a generation?

**Problem 2: No ordering or priority model.** When multiple addons register `PostGenerationHook`s, what's the execution order? Is it guaranteed? Is it configurable? This matters because Anansi's extraction runs post-generation, and future addons might also want that slot.

**Problem 3: No error propagation contract.** If `ContextSlotProvider` fails, what happens? Does the entire generation abort? Does it degrade? Is the error surfaced?

**Recommendation:**

```rust
pub trait PostGenerationHook: Send + Sync {
    /// Called after assistant message commit, in a background task.
    /// Receives the committed message and current session state snapshot.
    /// Returns a list of proposed state changes, which the engine validates
    /// before applying. Errors are logged but never abort the generation.
    fn on_generation_complete(
        &self,
        context: &PostGenerationContext,
    ) -> Result<Vec<ProposedChange>, AddonError>;
}

pub struct PostGenerationContext {
    pub session_id: SessionId,
    pub message: Message,
    pub turn_number: u64,
    pub current_state_snapshot: Arc<SessionStateSnapshot>,
}

pub struct ProposedChange {
    pub kind: ProposedChangeKind,
    pub confidence: Confidence,
    pub audit_reason: String,
}
```

This would make the contract implementable without guessing.

### W2. The Entity Model Has No Identity Resolution Strategy

The entity model (§10.4) includes a "stable slug ID" and "display name," but the document acknowledges duplicate entity creation as a risk (§20) and provides only a `merge` command as recovery. This is a **design-level gap**, not just an implementation detail.

**The core problem:** LLM extraction will produce "Elara," "Elara the Mage," "the mage," and "she" across turns. The slug ID can't stabilize without some identity resolution strategy, and "exact normalization in MVP, merge command for recovery" (§20) is under-specified.

**What's missing:**

1. **Normalization rules** — What does "exact normalization" mean? Lowercasing? Whitespace normalization? Title stripping? This needs to be explicit, not implied.
2. **Discovery vs. update decision** — When extraction finds "Elara the Mage" and the registry has "Elara," what happens? Is this a new entity? An alias? A merge candidate? The design says extraction "must not decide fuzzy identity merges" (§13.6), but it doesn't say what it *does* decide.
3. **Alias model** — Even in MVP, a minimal alias model (one canonical slug → zero or more display name variants) would make the merge command actually useful instead of a DB surgery tool.

**Recommendation:**

Add to §10.4:

```rust
pub struct Entity {
    pub slug: String,          // canonical: lowercase, alphanumeric + hyphens
    pub display_name: String,  // latest name used in narrative
    pub known_aliases: Vec<String>,  // previously seen names for this entity
    pub entity_type: EntityType,
    pub stats: StatBlock,
    pub mood: Mood,
    pub sensitivity: SensitivityMultipliers,
    pub last_interaction_turn: u64,
    pub audit: EntityAudit,
    pub correction: CorrectionMetadata,
}
```

And add a normalization section to §13:

```
Normalization rules (MVP):
1. Lowercase the name
2. Strip articles ("the", "a", "an") from the beginning
3. Collapse whitespace to single spaces
4. Match against slug first, then known_aliases
5. If no match: create new entity with slug = normalized name
6. If match: update display_name if different, add previous display_name to aliases
7. If ambiguous (multiple matches): mark as DiscardedAmbiguous
```

This is still "exact normalization" — no fuzzy matching, no pronoun resolution — but it's *specified* exact normalization rather than aspirational exact normalization.

### W3. The Extraction/Combat Authority Boundary Is Ambiguous

The design says:

- "Health is combat-only" (§4.2)
- "Extraction never mutates health" (§4.2)
- "All HP changes flow through combat or future explicitly-defined deterministic systems" (§4.2)
- "Extraction must not decide... HP changes" (§13.6)
- "Extraction must not decide... whether combat begins automatically" (§13.6)

But then the extraction pipeline (§13) produces relationship deltas, and the stat block includes `hostility`. If extraction increases `hostility` to 10, what's the relationship between `hostility` and combat initiation? The design is silent.

**The ambiguity:** Is `hostility` a relationship stat that happens to be named aggressively, or is it a combat pre-condition? If extraction pushes `hostility` to maximum, does that trigger combat? The design says no automatic combat triggering, but doesn't explain what `hostility` *does* outside combat.

**Recommendation:**

Add explicit stat-purpose mapping:

```
Stat purposes in MVP:
- trust, affection, anger, fear: relationship flavor, inform mood, visible in context
- hostility: relationship stat + combat readiness indicator. Does NOT trigger combat.
- health: combat resource ONLY. Never touched by extraction.

Mood resolution uses: trust, affection, anger, fear.
Combat damage uses: health.
Combat hit chance modifiers use: hostility (future, post-MVP).
```

This prevents the most common misinterpretation (that high hostility should auto-start combat).

### W4. The `[GAME STATE]` Context Block Lacks Structural Specification

§12 provides token budgets and priority ordering, but no actual structure for the block. The LLM needs a predictable format to "rely on it" (§12.5), and the user needs to be able to read it. Without a schema, different implementations will produce different blocks, and the block becomes unreliable context.

**Recommendation:**

```markdown
[GAME STATE]
Combat: active | Round 2 | Elara HP: 8/10 | Goblin HP: 3/10
Entities:
  Elara (player) — Mood: determined | Trust:7 Aff:6 Ang:2 Fear:3 Host:1
  Goblin (creature) — Mood: aggressive | Host:9
Recent: Elara trust +2 (conversation), Goblin hostility +3 (threat)
```

This is compact, scannable, and structured enough for an LLM to parse reliably. Add this as a normative example to §12.

### W5. Sensitivity Multipliers Are Undefined

§10.4 mentions "sensitivity multipliers" but never defines what they are, what they modify, or what their range is. §14.3 mentions "whether sensitivity altered it" as an audit marker, but the mechanism is invisible.

**Recommendation:**

```rust
pub struct SensitivityMultipliers {
    pub trust: f32,       // default: 1.0, range: 0.0–2.0
    pub affection: f32,   // default: 1.0, range: 0.0–2.0
    pub anger: f32,       // default: 1.0, range: 0.0–2.0
    pub fear: f32,        // default: 1.0, range: 0.0–2.0
    pub hostility: f32,   // default: 1.0, range: 0.0–2.0
}
// Multiplier applied to proposed delta before clamping:
// final_delta = clamp(proposed_delta * sensitivity, -delta_cap, delta_cap)
```

### W6. The Mood Model Is Referenced But Never Defined

Mood appears in: §5 (Tier A scope), §10.4 (entity model), Phase 1A (mood model), Phase 1B (mood resolution). But nowhere is the mood type defined. Is it an enum? A string? A composite of stat thresholds?

**Recommendation:**

```rust
pub enum Mood {
    Friendly,      // trust + affection high
    Warm,          // affection high
    Trusting,      // trust high
    Anxious,       // fear high
    Hostile,       // hostility high
    Angry,         // anger high
    Afraid,        // fear very high
    Cold,          // all stats near baseline
    Conflicted,    // competing high stats
}

// Resolution rule (deterministic, MVP):
// 1. If any stat ≥ 8, that stat's mood wins (priority: fear > hostility > anger > affection > trust)
// 2. If multiple stats ≥ 8, highest wins
// 3. If no stat ≥ 8, Mood::Cold
```

Even a simple rule like this is better than "mood resolution" as a black box.

### W7. Cross-Session Import Is Too Vaguely Deferred

§11.3 says cross-session import is "post-MVP optional" and gives good reasons. But it also says "the core design should preserve it." What does "preserve" mean? If the schema doesn't account for import at all, adding it later requires migration. If it does account for it, the design should say how.

**Recommendation:**

Add a single sentence to §10.4:

```
Entity slugs are session-scoped by default. Cross-session import (post-MVP) will require
a namespace prefix (e.g., "session_uuid::entity_slug") to avoid collisions. The current
schema does not include this prefix but should not make it impossible to add.
```

This costs nothing in MVP but prevents a migration headache later.

---

## IV. Logical Flow Analysis

### Overall Structure: Strong

The document flows in a logical dependency chain:

```
What → Goals → Non-Goals → Principles → Scope → MVP → Tech → Structure →
Contract → Data → Persistence → Context → Extraction → Validation →
Combat → TUI → Config → Roadmap → Risks
```

This is the right order. Each section builds on the previous without forward references that create confusion.

### Specific Flow Issues

**Issue 1: §2 (Product Fit) comes before §3 (Non-Goals), but §2 references the addon surface that isn't defined until §9.**

The section on product fit mentions "ozone-core exposes a general addon surface" and "Anansi is the first concrete addon build using it." But the addon surface isn't specified until seven sections later. A reader encountering this for the first time has no idea what "addon surface" means yet.

**Fix:** Add a forward reference in §2.2:

```
Host/addon clarification (see §9 for the full addon surface specification):
```

**Issue 2: §5 (Scope Tiers) defines Tier D as "post-MVP only," but §18 (MVP/Post-MVP Separation Map) already handles this.**

The scope tier table and the separation map have overlapping but slightly different categorizations. Tier D in §5 is "inventory, item modifiers, conditions, world flags, richer simulation." §18 splits this across Phases 2, 3, and 4. These should be clearly linked.

**Fix:** Add to §5:

```
Note: Tier D maps to Post-MVP Phases 2–4 as detailed in §18.
```

**Issue 3: The roadmap (§19) has 10 phases (1A–1J), but they don't map to the scope tiers (§5) or the MVP/Post-MVP map (§18).**

§5 defines Tiers A–D. §18 defines MVP vs. Post-MVP Phases 2–4. §19 defines implementation phases 1A–1J. A reader must mentally cross-reference three different grouping systems.

**Fix:** Add a mapping table:

```
| Roadmap Phase | Scope Tier | MVP/Post-MVP |
|--------------|------------|--------------|
| 1A           | A          | MVP          |
| 1B           | A          | MVP          |
| 1C           | A          | MVP          |
| 1D           | N/A (ozone-core) | MVP     |
| 1E           | B          | MVP          |
| 1F           | B          | MVP          |
| 1G           | A          | MVP          |
| 1H           | C          | MVP          |
| 1I           | A          | MVP          |
| 1J           | All        | MVP          |
```

**Issue 4: Phase 1D (unified addon API in ozone-core) is listed as an Anansi phase, but it modifies ozone-core.**

This is a cross-workspace dependency that could be a bottleneck. If Anansi development is blocked waiting for ozone-core changes, the roadmap is misleading. The document should acknowledge this.

**Fix:** Add to Phase 1D:

```
Note: This phase requires changes to the ozone+ workspace. If ozone+ development
is on a separate timeline, this phase should be coordinated with the ozone+ team
and may need to be split: define the addon traits in ozone-core first (ozone+ team),
then implement the Anansi side (Anansi team).
```

---

## V. Ambiguity Reduction

### A1. "Confidence Marking" Needs a Scoring Rule

§13.4 adds extraction outcomes, and §14.3 adds confidence levels (high/medium/degraded). But what determines the confidence level? The design says:

- `high`: clean parse, low repair, no major overrides
- `medium`: valid parse with normalization or limited fallback
- `degraded`: heavy repair, ambiguity, or multiple fallback replacements

"Low repair," "limited fallback," "heavy repair" are subjective. For a deterministic engine, this should be a rule.

**Proposed scoring rule:**

```
confidence_score = 1.0
- 0.1 per field that required normalization (rename, retype)
- 0.2 per field that required fallback magnitude substitution
- 0.3 per field that was ambiguous and resolved by heuristic
- 0.4 per field that was completely fabricated by fallback

high:   confidence_score ≥ 0.8
medium: 0.5 ≤ confidence_score < 0.8
degraded: confidence_score < 0.5
```

### A2. "Base Magnitude" and "Delta Cap" Need Units and Semantics

§17.1 config shows:

```toml
base_magnitude = 3
delta_cap = 10
```

But what do these mean? Is base_magnitude the default proposed delta? The fallback delta? The expected average? Is delta_cap per-stat or per-extraction-cycle?

**Proposed specification:**

```
base_magnitude: the delta value used when the engine falls back to conservative
estimation (i.e., extraction failed or was discarded). Applied per stat that
was expected to change. Default: 3 (out of 0-255 stat range, ~1.2% shift).

delta_cap: maximum absolute delta applied to any single stat in one extraction
cycle, regardless of what extraction proposed. Clamped after sensitivity
multiplier. Default: 10 (~3.9% of stat range).
```

### A3. "Idempotency Keys" Are Mentioned But Never Explained

§7.1 lists `uuid` as a dependency for "IDs, idempotency keys." But the extraction pipeline mentions `SkippedAlreadyProcessed` without explaining the idempotency mechanism.

**Proposed specification:**

```
Each extraction cycle generates an idempotency key composed of:
  turn_number + message_id_hash

If a key matches a previously processed extraction, the result is
SkippedAlreadyProcessed. This prevents double-application during
swipe/regenerate flows without requiring rollback.
```

### A4. The Interaction Direction Table Is Referenced But Never Shown

§14.1 says "The interaction-direction table remains a strong design choice and should stay." But the table itself is never presented in this document. It's presumably inherited from an earlier draft.

**Fix:** Include the table inline:

```rust
// Interaction direction rules for relationship deltas
// Format: (proposer_direction, target_direction) → allowed delta sign
//
// user → npc:      trust/affection +allowed, anger/fear/hostility +allowed
// npc → user:      trust/affection +allowed, anger/fear/hostility +allowed  
// npc → npc:       all deltas +allowed (extraction-proposed, not auto-applied)
// user → user:     no relationship deltas (self-reference invalid)
// narrator → any:  no relationship deltas (narrator has no relationship)
// unknown → any:   delta halved (degraded confidence in direction)
```

### A5. "Repair" Is Used Throughout But Never Defined

"Repair and clamping" (§13.4), "whether repair is substantial" (§13.5), "low repair" (§14.3). What constitutes repair?

**Proposed specification:**

```
Repair operations (in order of severity):
1. Type coercion: "7" → 7 (string to int) — trivial, no confidence penalty
2. Range clamping: 260 → 255 (overflow) — minor, -0.05 confidence
3. Missing field substitution: null → base_magnitude — moderate, -0.1 confidence
4. Structural reparse: {"trust": {"value": 5}} → {"trust": 5} — moderate, -0.15 confidence
5. Partial extraction: extracted 2 of 3 expected fields — heavy, -0.2 confidence
6. Complete fabrication: no parseable data, all fields fallback — severe, -0.4 confidence
```

---

## VI. Ozone+ Integration Concerns

### IC1. Persistence: Shared Database, Separate Namespaces

Anansi uses "namespaced `anansi_*` tables" in the same SQLite session database. This is correct but needs a migration coordination mechanism.

**Risk:** If ozone+ adds a table called `anansi_entities` for its own purposes in v0.5, it collides with Anansi's namespace.

**Fix:** Register namespace ownership in `ozone-core`:

```sql
CREATE TABLE addon_namespaces (
    namespace TEXT PRIMARY KEY,  -- e.g., "anansi"
    addon_version TEXT NOT NULL,
    registered_at INTEGER NOT NULL
);
```

Anansi checks for namespace collision at startup. ozone+ refuses to create tables in a registered addon's namespace.

### IC2. Context Budget Competition

Anansi's `[GAME STATE]` block (§12) competes for soft context budget alongside ozone+ lorebook entries, retrieved memories, and thinking summaries. The 400-token soft-cap is reasonable, but the **priority** relative to other soft context sources is undefined.

ozone+ v0.4 defines context layers with explicit priorities (§11.6). Anansi's context slot needs to declare its priority relative to these layers.

**Fix:** Add to §12:

```
[GAME STATE] registers as a ContextLayer with:
  kind: AddonContext { addon_name: "anansi" }
  priority: 50  (between LorebookEntries=40 and RetrievedMemory=60)
  is_hard_context: false
  collapse_strategy: TruncateTail (remove oldest entities first)
```

This makes Anansi's context contribution participate in ozone+'s existing budget algorithm rather than being a special case.

### IC3. Event Table Compatibility

Anansi's audit events (§11.2) and ozone+'s system events (§13.4, §24.1) both use an events table. If they share the same table, the `event_type` namespace needs coordination.

**Fix:** Prefix Anansi event types:

```
anansi.entity_created
anansi.delta_applied
anansi.combat_round
anansi.extraction_completed
anansi.manual_correction
```

### IC4. Swipe/Regenerate Behavior

ozone+ supports swipe groups with multiple candidates per generation (§9). When the user swipes to an alternate response, Anansi must:

1. Not re-extract from the swiped-away message
2. Extract from the newly active message
3. Not duplicate state changes

The design mentions `SkippedAlreadyProcessed` and idempotency keys, but doesn't explicitly describe the swipe interaction flow.

**Fix:** Add to §13:

```
Swipe/regenerate flow:
1. User swipes to alternate candidate
2. Anansi receives BranchChanged event (or equivalent)
3. Anansi marks all extractions from the deactivated message as superseded
4. Anansi runs extraction on the newly active message
5. Idempotency key = (turn_number, active_message_id) — different for each swipe
6. No rollback of previously applied deltas; manual correction if needed
```

---

## VII. Summary Assessment

### Overall Grade: **B+**

Anansi v0.2 is a well-disciplined design document with genuine MVP commitment and a sound core architecture. The revision significantly improves on the earlier draft by adding the inspector surface, extraction outcome taxonomy, and proper addon surface unification.

### Scorecard

| Dimension | Grade | Notes |
|-----------|-------|-------|
| Product fit | A | Clean positioning, honest dependency model |
| MVP discipline | A+ | Best quality of the document — genuinely enforced |
| Core architecture | A- | Sound, but addon contract underspecified |
| Data model | B | Missing identity resolution, sensitivity, mood definitions |
| Persistence | B+ | Good audit trail, needs namespace coordination |
| Context injection | B- | Good priorities, no structural spec |
| Extraction pipeline | B | Good outcomes, undefined normalization rules |
| Combat | B+ | Honest scoping, clear degradation |
| TUI | B | Good correction commands, needs alias model |
| Logical flow | B+ | Strong overall, cross-reference gaps |
| Ambiguity | C+ | Multiple undefined terms that should be rules |
| Ozone+ integration | B | Good principles, missing detail on budget and events |

### Priority Fixes (Before Implementation)

1. **Define the addon surface traits** with method signatures (W1)
2. **Specify entity identity resolution rules** (W2)
3. **Define the mood model** explicitly (W6)
4. **Specify the `[GAME STATE]` block format** normatively (W4)
5. **Define sensitivity multipliers** (W5)
6. **Clarify the hostility/combat boundary** (W3)
7. **Add context priority registration** (IC2)
8. **Specify normalization rules** for extraction (A2 from W2)
9. **Define repair severity scale** (A5)
10. **Specify swipe interaction flow** (IC4)

These are all specification gaps, not architectural flaws. The architecture is sound — it just needs to be written down completely before implementation begins.


*******************************************************************************************************************************************************************

**Model: MiMo-V2-Pro (Xiaomi)**
**Version: MiMo-V2-Pro (as of 2026-04-13)**

---

## Anansi Design Plan Analysis

I'll analyze the Anansi design document, assess its fit within the Ozone product line, evaluate strengths and weaknesses, and examine the plan's logical flow.

### 1. How Anansi Fits Into the Ozone Product Line

Based on the provided documentation, Anansi is positioned as a **specialized downstream addon** for the Ozone family. Here's the hierarchy:

| Product | Purpose | Target User | Relationship to Ozone+ |
|---------|---------|-------------|------------------------|
| **ozonelite** | Minimal backend control | Developers seeking lightweight inference | Standalone |
| **ozone** | Backend tuning and session management | Power users needing optimization | Standalone |
| **ozone+** | Full local-LLM workflow with polished TUI | Roleplay enthusiasts wanting narrative control | Core platform |
| **anansi** | Mechanical RPG layer atop narrative roleplay | Users wanting deterministic game mechanics | **Dependent addon to ozone+** |

**Key Architectural Relationship:**
```
ozone+ (Host Platform)
├── Provides: Session management, TUI, LLM workflow, persistence
├── Exposes: Unified addon surface (ContextSlotProvider, PostGenerationHook, ToolProvider, PaneProvider)
└── Hosts: Zero or more addons
    └── Anansi (First concrete addon)
        ├── Leverages: ozone+ infrastructure
        ├── Adds: Entity tracking, stat validation, combat resolution
        └── Maintains: Clear separation of concerns
```

**Value Proposition:**
- **ozone+ users** can continue pure narrative RP without mechanical complexity
- **Anansi users** get bounded RPG mechanics layered seamlessly into narrative flow
- The architecture prevents "feature creep" in ozone+ while enabling specialized extensions

### 2. Strengths of the Anansi Design

<details>
<summary><strong>🎯 Excellent Architectural Boundaries</strong></summary>

- **Clear MVP Discipline**: The 8-point success criteria in §6 are well-scoped and testable
- **Explicit Non-Goals**: §3 clearly separates what Anansi isn't from what it is
- **Host/Addon Separation**: The clarification that ozone+ is "Anansi-agnostic" prevents architectural coupling
- **Tiered Scope**: A/B/C/D tiers allow incremental development with fallback options
</details>

<details>
<summary><strong>🔍 Transparency-First Philosophy</strong></summary>

- **Audit Trail**: §14.2's explicit audit markers make every engine decision inspectable
- **Confidence Markers**: §14.3's three-level confidence system provides visibility into extraction quality
- **Manual Correction Surface**: §16.4's correction commands are the single biggest trust improvement
- **Inspector View**: §16.2-16.3 ensures no hidden state from users
</details>

<details>
<summary><strong>🛡️ Robust Degradation Handling</strong></summary>

- **Deterministic Core**: Tier A works even if extraction (Tier B) or combat (Tier C) fails
- **Graceful Extraction**: §13.4's six explicit outcomes (Accepted, Degraded, Discarded, etc.) prevent silent failures
- **Skip, Not Rollback**: §4.6's approach to swipe/regenerate flows avoids complex state reversal
- **Context Safety**: §12.2's rule to drop `[GAME STATE]` before narrative context protects core functionality
</details>

<details>
<summary><strong>🧪 Technical Pragmatism</strong></summary>

- **bevy_ecs Choice**: §7.3 wisely excludes bevy_app, avoiding unnecessary game-loop overhead
- **Fixed Schema**: §10.2's six-stat block reduces validation complexity for MVP
- **Additive-Only Persistence**: §11.1's namespaced tables prevent schema conflicts with ozone+
- **Versioned Addon Contract**: §9.4's API stability rule protects future addon development
</details>

### 3. Weaknesses and Improvement Opportunities

<details>
<summary><strong>⚠️ Extraction Pipeline Ambiguity</strong></summary>

**Issue**: §13's extraction pipeline mentions "two logical jobs" but lacks concrete examples of how entity discovery and relationship extraction interact with narrative text.

**Recommendation**:
1. Add a worked example showing raw narrative → extraction → validation → state change
2. Define boundary cases: How does extraction handle ambiguous pronouns? Metaphorical language?
3. Specify confidence thresholds: When does "medium" confidence trigger manual review?
</details>

<details>
<summary><strong>⚠️ Combat System Scalability Concerns</strong></summary>

**Issue**: §15's minimal combat (one round, no initiative) may feel too restrictive even for MVP, potentially limiting user engagement.

**Recommendation**:
1. Consider adding optional "combat stance" tracking (aggressive/defensive) without full initiative
2. Allow simple status effects (prone, stunned) as binary flags rather than complex conditions
3. Implement basic environmental modifiers (cover, high ground) as simple multipliers
</details>

<details>
<summary><strong>⚠️ Missing Error Recovery Examples</strong></summary>

**Issue**: While §20's risk register identifies threats, the document lacks concrete recovery workflows.

**Recommendation**:
1. Add "Recovery Scenarios" section with step-by-step guides for:
   - Duplicate entity creation
   - Bad extraction corrupting relationships
   - Session corruption recovery
   - Version mismatch between ozone+ and Anansi
</details>

<details>
<summary><strong>⚠️ Performance Considerations Under-Specified</strong></summary>

**Issue**: §12.4's token budget (400 tokens) and §12.3's entity limits (8) are reasonable but lack empirical justification.

**Recommendation**:
1. Add benchmarks: "400 tokens ≈ 15% of 8K context, leaving 85% for narrative"
2. Provide guidance for scaling these numbers based on hardware capabilities
3. Consider adaptive limits based on available context window
</details>

### 4. Logical Flow Analysis

<details>
<summary><strong>✅ Strengths in Logical Flow</strong></summary>

1. **Clear Progression**: Goals → Non-Goals → Principles → Scope → Implementation flows naturally
2. **Dependency Management**: §7.2's explicit dependency on ozone+ crates prevents circular references
3. **Phase Separation**: §18's MVP/Post-MVP map provides clear development boundaries
4. **Roadmap Testability**: §19's "Exit criterion" approach ensures each phase delivers value
</details>

<details>
<summary><strong>⚠️ Areas for Improved Clarity</strong></summary>

**Issue 1: Addon Registration Process**
The document describes what Anansi does but not *how* it registers with ozone+. The addon surface (§9) needs a concrete registration workflow.

**Recommendation**: Add a "Registration & Discovery" subsection explaining:
- How ozone+ discovers available addons
- The initialization sequence
- Version compatibility checks
- Fallback behavior if addon fails to load

**Issue 2: Context Contribution Timing**
§12 mentions `[GAME STATE]` injection but doesn't specify when this happens in the generation pipeline.

**Recommendation**: Clarify the sequence:
1. Context assembly (ozone+)
2. Anansi contributes `[GAME STATE]` as soft context
3. Generation occurs
4. Post-generation extraction (Anansi)

**Issue 3: Cross-Session Import Clarity**
§11.3 defers cross-session import but doesn't explain how entities would be identified across sessions.

**Recommendation**: Even for post-MVP, outline the entity identity strategy:
- UUID vs. name-based matching
- Conflict resolution rules
- User override capabilities
</details>

<details>
<summary><strong>🔄 Suggested Document Reordering</strong></summary>

Current flow is generally logical but could be improved by:

1. **Move §2 (Product Fit) earlier**: Position after "What Anansi Is" to immediately establish context
2. **Merge §4 (Principles) with §5 (Scope)**: Principles inform scope decisions, combining them shows rationale
3. **Add "User Workflows" section**: Between architecture and implementation to bridge technical design with user experience
4. **Create "Integration Sequence" diagram**: Visual showing ozone+ → Anansi → User interaction flow
</details>

### 5. Ambiguity Reduction Recommendations

| Section | Ambiguity | Clarification Suggestion |
|---------|-----------|--------------------------|
| §9.1 | Addon capabilities "four first-class" | Specify which capabilities are required vs. optional for different addon types |
| §10.4 | "audit metadata about most recent applied change" | Define exact fields: timestamp, source, confidence, user_override flag |
| §13.6 | "Extraction must not decide: absolute stat values" | Clarify: Does extraction propose deltas only? How are new entities' initial stats determined? |
| §15.2 | "tool-calling is unavailable" | Specify: Does combat become narrative-only, or does the engine apply minimal deterministic resolution? |
| §16.3 | "last few recent changes" | Define "few": 3, 5, or configurable? |
| §17.1 | `[anansi.extraction] model = "same"` | Explain what "same" means: same as ozone+ main model, or a separate utility model? |

### 6. Key Improvement Priorities

1. **Add Concrete Examples**: Especially for extraction pipeline and error recovery
2. **Specify Registration Workflow**: How Anansi plugs into ozone+ at runtime
3. **Define Performance Boundaries**: Token budgets, entity limits with empirical justification
4. **Expand Combat System**: Minimal but not trivial additions for MVP
5. **Create Integration Diagrams**: Visual representation of addon registration, context flow, and error recovery

### 7. Overall Assessment

The Anansi design is **exceptionally well-scoped** for an MVP addon, with strong boundaries that prevent feature creep while delivering meaningful value. The transparency-first approach and robust degradation handling address the core challenge of LLM-driven game mechanics: maintaining user trust when automated systems make mistakes.

The primary weaknesses are not architectural but **implementation-level ambiguities** that can be resolved with concrete examples and workflow specifications. The document's logical flow is sound but could benefit from visual aids and reordering to emphasize the user experience alongside technical architecture.

**Verdict**: The design is ready for implementation with minor clarifications. The phased roadmap (§19) provides a realistic path to delivery, and the separation from ozone+ protects both products' integrity.

---

*Analysis complete. Ready for follow-up questions or deeper dives into specific sections.*

*************************************************************************************************************************************************************************************

**Model:** MiniMax M2.7

Let me analyze both documents you've provided.


# Model Identification

**Model:** MiniMax M2.7 (internal model)
**Version:** 1.0.0

---

# Anansi Design Review — Critical Analysis

This review synthesizes the Anansi v0.2 design document against the ozone+ v0.4 baseline and provides a structural assessment. It covers product fit, logical flow, ambiguities, and actionable improvement suggestions.

> **Scope note:** "Anansi" in this document refers exclusively to the RPG game-engine addon described in the design. An unrelated Unity toolkit called "Anansi" (ShiJbey/Anansi on GitHub) and a browser game "Ozone Layer" exist in open-source ecosystems. These are separate projects. The Anansi in the Ozone family is a Rust-based addon for ozone+.

---

## 1. Product Fit Within the Ozone Family

### 1.1 How Anansi Integrates

The Ozone product family has the following tiered structure, per the design documents:

| Build | Niche | Core Value |
|-------|-------|------------|
| `ozonelite` | Lean backend control | Minimal footprint |
| `ozone` | Backend tuning + session management | Core engine |
| `ozone+` | Full local-LLM workflow, polished TUI | Narrative-first RP |
| `anansi` | `ozone+` + deterministic game layer | Mechanical roleplay |

Anansi is positioned as the **first concrete addon** built on the unified addon surface that ozone+ exposes. Its dependency chain is clean:

```
ozone-core (shared types, traits, protocols)
    └── ozone+ (narrative-first host)
            └── anansi (game layer, ECS, extraction, TUI addon)
```

The positioning rule in §2.1 of the Anansi doc is correctly stated: **Anansi is a dependent build, not a replacement.** This boundary is architecturally honest and prevents scope inflation into the core product.

### 1.2 Why This Fit Works

The separation is well-justified because:

- `ozone+` targets users who want freeform narrative RP without mechanics
- Anansi targets users who want bounded RPG mechanics layered under that same narrative RP
- The `ozone+` → `anansi` path is additive, not transformative

This is analogous to how a tabletop RPG system (e.g., D&D 5e) can be played as pure theater-of-the-mind narrative or with full minis-and-grid combat. Anansi adds the mechanical layer without replacing the narrative layer.

### 1.3 Fit Assessment: ✅ Strong

The positioning is clear, the dependency chain is honest, and the family-level documentation correctly distinguishes these roles.

---

## 2. Logical Flow of the Document

### 2.1 Overall Structure

The document follows a **definition → context → constraints → technical → execution** progression that is largely sound:

1. **What Anansi Is** (identity + core promise)
2. **Design Goal** (build goals)
3. **Product Fit** (family positioning)
4. **Non-Goals** (hard scope limits)
5. **Foundational Principles** (invariant design rules)
6. **Scope Tiers** (phase-gated capability map)
7. **Honest MVP Boundary** (success criteria)
8. **Technology Stack** (dependency inventory)
9. **Workspace Structure** (crate layout)
10. **Unified Addon Surface** (interface contract)
11. **Data Model** (types, entities)
12. **Persistence** (DB schema, audit)
13. **`[GAME STATE]` Context Injection** (soft context)
14. **Extraction Pipeline** (LLM → engine bridging)
15. **Validation and Fallback Logic** (deterministic resolution)
16. **Combat System** (bounded deterministic resolution)
17. **TUI Surfaces** (UI affordances)
18. **Config System** (TOML settings)
19. **MVP / Post-MVP Separation Map** (phase gates)
20. **Revised Roadmap** (10-phase implementation plan)
21. **Risk Register** (failure modes and mitigations)
22. **Open Design Decisions** (known deferred items)
23. **Implementation Guidance** (optimization priorities)
24. **Closing Direction**

### 2.2 What Works Well

The **non-goals section** (§3) is one of the strongest parts of the document. It explicitly distinguishes hard non-goals from deferred-but-desirable features, which is the single most important scope-discipline mechanism in any design document.

The **scope tiers table** (§5) is a good mechanism. The rule that "Tier A must remain useful even if Tier B and C are disabled" is the right constraint.

The **MVP boundary** (§6) is concrete with seven numbered success criteria. This is actionable in a way that "scope creep" warnings are not.

The **phased roadmap** (§19, Phases 1A–1J) is structured as independently testable milestones, which is the correct approach for phased implementation.

The **risk register** (§20) covers the most important failure modes with concrete mitigations.

### 2.3 Structural Weaknesses

Despite the overall sound structure, several sections have ordering or clarity issues:

**Issue A: Sections 14 and 15 depend on concepts from §§10–13, but the dependency chain is implicit.**

The reader must cross-reference across many sections to understand that:
- The extraction pipeline (§13) proposes deltas
- Validation (§14) bounds and corrects those deltas
- The event trail records what happened
- The mood model (§10) influences validation
- Combat (§15) consumes the validated state

This dependency chain would benefit from a **forward reference diagram** early in the document, so the reader knows where each piece fits before reading the detail.

**Issue B: The roadmap (§19) has 10 phases (1A–1J) that are listed but not mapped to scope tiers or MVP criteria.**

The roadmap phases do not directly reference which scope tier they correspond to, nor which of the seven MVP success criteria they satisfy. This disconnect means a reviewer cannot quickly answer "which phase delivers success criterion #6?" without cross-referencing manually.

**Issue C: The data model section (§10) introduces `mood` and `sensitivity multipliers` as fields but these are not explained.**

`StatBlock` shows six fields but entity attributes include `mood` and `sensitivity_multipliers` that are not in the struct definition shown in §10.2. The full entity model is described in prose after the StatBlock, but the reader only sees the StatBlock in a highlighted code block, creating an inconsistency in how the document presents its own data model.

**Issue D: Section 16 (TUI Surfaces) introduces the "inspector" as a missing piece from an earlier draft, but does not fully specify it.**

The document acknowledges "the old design had a good compact pane, but it needed a fuller inspection surface" and states what MVP should include. However, the inspector is not tied to a specific roadmap phase, nor is there a wireframe, state diagram, or interaction model for it. This leaves a significant UX-facing gap in the design.

---

## 3. Critical Ambiguities

These are points where the document either contradicts itself, uses undefined terms, or leaves a decision genuinely open that should be resolved or explicitly flagged.

### 3.1 Ambiguity: Who Owns Mood Resolution?

The document repeatedly references `mood` as an entity attribute (entity model in §10.4, `MoodResolution` as a deterministic system mentioned in the Phase 1B roadmap entry). But the document also states in §13.6:

> **What extraction must not decide** [includes] final mood labels

If extraction cannot decide mood labels, but the mood is tracked as an entity attribute and MoodResolution is a deterministic system, the question becomes: **who or what provides the mood label?**

The document does not answer this. Possible answers (not mutually exclusive) include:

- The user manually sets mood via a correction command
- The engine infers mood from the six-stat block state (e.g., anger > 50 → mood = hostile)
- A separate inference pass (not extraction) determines mood
- Mood is purely derived from stat values, not a separate label

**Recommendation:** Clarify the mood resolution model explicitly. Define whether mood is a user-settable label, a system-derived label, or both.

### 3.2 Ambiguity: When Does Combat Begin?

The document states in §15.1 that Anansi has "one deterministic round at a time" and that combat outcomes are not decided by extraction (§13.6). It also lists "start combat" as a manual command in §15.3.

But the document does not clarify:

- Can the LLM narrate combat *begin* without the user explicitly starting it?
- If the LLM narrates a fight, does Anansi automatically detect combat state from narrative text, or does it require a manual `/combat` command?
- If an entity reaches 0 HP in the stat block, what happens narratively?

**Recommendation:** Define an explicit combat state machine with clear transitions: `Idle` → `CombatActive` → `CombatResolved`. Specify which events trigger which transitions (user command, LLM proposal, stat block reaching 0 HP).

### 3.3 Ambiguity: Swipe/Regenerate Rollback (Partial)

The document addresses the swipe case in §4.6 ("Skip, not rollback") and in §13.2 (extraction eligibility). But the implications are not fully traced:

- If a turn produces an extraction that applies +5 to trust and +3 to fear, and the user swipes back and regenerates the turn, what state does Anansi show for that turn?
- Is the corrected turn treated as "already processed" in future extraction passes?
- Is there an audit trail entry for the skipped turn, or does it disappear from the trail?

The document says extraction must not duplicate state changes and must have a `SkippedAlreadyProcessed` outcome. But the interaction between swipe, extraction eligibility, audit trail, and stat state is not modeled end-to-end.

**Recommendation:** Add a section specifically covering the swipe/regenerate flow end-to-end, including the state of the entity registry before and after the swipe, and the state of the audit trail.

### 3.4 Ambiguity: The Six-Stat Block's Relationship to Narrative

The six stat block contains `trust`, `affection`, `anger`, `fear`, `hostility` — all relational attributes between entities. But there is no defined relationship model:

- Are these always between the player and the NPC?
- Do NPCs track trust toward *each other*?
- Does the stat block represent dyadic (pairwise) relationships only?
- If two NPCs interact, do their relational stats update?

The document explicitly lists "NPC↔NPC relationship graphing" as a non-goal (§3.1), so dyadic NPC-to-NPC tracking is deferred. But even the player-to-NPC dyad is underspecified: does Anansi track the player's stats toward the NPC, or the NPC's stats toward the player (or both)?

**Recommendation:** Define the base relationship model explicitly: "Each entity has a stat block. When the player interacts with an NPC, the deltas apply to the relevant dyad." If the relationship is always player-centric, say so.

### 3.5 Ambiguity: Tier B "Confidence" vs. Tier B Degraded Mode

Section §13.4 defines six explicit extraction outcomes (Accepted, AcceptedDegraded, etc.) and confidence levels (high, medium, degraded). Section §14.3 defines confidence levels with a table showing what each level means.

But the relationship between the "six outcomes" and the "three confidence levels" is not explained. Specifically:

- Does `AcceptedDegraded` map to confidence level `degraded`?
- Does `Accepted` with heavy fallback use confidence level `degraded`?
- Is confidence a per-entity property or a per-extraction-cycle property?

The document states each extraction cycle emits one confidence label (§14.3), but it also says each delta should have audit markers about whether fallback was used (§14.2). This creates confusion about whether confidence is a system-level label or an audit-level detail.

**Recommendation:** Disambiguate the relationship between extraction outcomes (which describe individual delta handling) and confidence levels (which describe the overall cycle quality). Consider making confidence a first-class struct field in the extraction cycle output.

### 3.6 Ambiguity: `[GAME STATE]` Injection — Who Reads It?

Section §12 describes `[GAME STATE]` as "a soft-context summary, never hard context." The bounds are well-specified (max 8 entities, max 3 recent changes, ~400 token soft cap). But the document does not address:

- Is this block injected as a system prompt prefix, a lorebook entry, or a special context layer?
- Is it injected into the LLM's context on every turn, or only when the context assembler decides it's relevant?
- What is the actual format string? Is it plain text, a structured JSON snippet, or something else?

The ozone+ v0.4 design document has a full `ContextLayerPolicy` system (§11.6, §14) for managing context layers with priority, budget, and collapse strategies. Anansi's `[GAME STATE]` should either integrate into that system or explicitly diverge from it. The design document does not address this.

**Recommendation:** Either (a) define `[GAME STATE]` as a new `ContextLayerKind` in the ozone+ context assembler, or (b) define it as a lorebook-scoped injection with a specific keyword trigger. Do not leave the injection mechanism ambiguous.

---

## 4. Strengths

### 4.1 Core Design Principle: "The LLM Proposes, The Engine Decides"

This is the single most important design decision in the document. It establishes a clean separation of responsibilities:

- **LLM**: Narrator, proposer of intent, generator of prose
- **Engine**: Validator, bounded-applier of state changes, enforcer of constraints
- **User**: Inspector, corrector, final authority

This principle, if implemented faithfully, prevents the most common failure mode in LLM-integrated game systems: the model silently overwriting or inventing game state.

### 4.2 Bounded Stat Schema

The fixed six-stat block (health, trust, affection, anger, fear, hostility) is a strong MVP decision. It makes:

- Validation rules understandable and auditable
- Context injection bounded and predictable
- TUI presentation compact (read-only pane can show all six stats cleanly)
- Persistence schema simple
- Testing tractable (bounded input/output space)

The acknowledgment that this is "an MVP strength and a long-term limitation" (§10.3) is honest and correctly defers generalization.

### 4.3 Tiered Scope with Explicit Degradation Paths

Each tier in §5 has a defined dependency structure. The rule that Tier A must function without Tier B or C means the product degrades gracefully rather than collapsing. This is architecturally sound.

### 4.4 Extraction Pipeline's Explicit Outcomes

Six distinct extraction outcomes (§13.4) with clear semantics (Accepted, AcceptedDegraded, DiscardedMalformed, DiscardedAmbiguous, SkippedAlreadyProcessed, SkippedLockContention) are a significant improvement over a monolithic "extraction succeeded/failed" model. The addition of `SkippedLockContention` specifically handles the swipe/regenerate case, which was a genuine gap in earlier designs.

### 4.5 Manual Correction as First-Class Feature

The explicit list of correction commands (§16.3) — create entity, rename, merge, set stat, set HP, clear bad change, combat control — is the right approach for a system where extraction quality cannot be guaranteed. The design recognizes that the best extraction system is one that also has excellent recovery tools.

### 4.6 The Anansi-Agnostic vs. Anansi-Specific Distinction

The clarification in §2.2 — that ozone+ is "Anansi-agnostic" rather than "unaware of Anansi" — is an important architectural precision. It means ozone+ exposes a general addon surface, and Anansi is the first concrete implementation. This distinction future-proofs the addon architecture for other addons.

### 4.7 The Non-Goals Section as Scope Discipline Tool

By explicitly listing hard non-goals (§3.1) and explicitly deferred items (§3.2), the document creates a mechanism for saying "no" to feature requests. The guidance in §18.5 (inventory sequence) is an excellent example of how to defer something without rejecting it: it gives a preferred implementation order that preserves MVP discipline.

---

## 5. Weaknesses and Improvement Recommendations

### 5.1 Weakness: The Extraction Pipeline is the Most Critical System and the Least Specified

The extraction pipeline is the interface between the LLM's narrative output and Anansi's deterministic state. This is the highest-risk component in the system — it is the source of the "LLM proposes" half of the core design principle. Yet §13 provides only 6 paragraphs covering trigger, eligibility, the two-job conceptual split, normalization pipeline, degraded mode, and a list of what extraction must not decide.

**Specific gaps:**

| Gap | Why It Matters |
|-----|----------------|
| Extraction trigger definition | "Runs after assistant message commit" is a trigger, but the document does not specify whether it's synchronous or async, fire-and-forget or blocking |
| Prompt/prompt template for extraction | The document does not specify what prompt is sent to the LLM for extraction. This is critical because extraction quality directly depends on the prompt. The ozone+ v0.4 doc has full prompt template infrastructure (§19.4) that Anansi should reuse or reference |
| Quality gates | If extraction produces output that is linguistically plausible but semantically wrong (e.g., "the stranger smiled warmly" extracted as +5 trust AND +5 anger), there is no described mechanism for catching this |
| Error propagation | If extraction fails mid-turn (LLM crashes, times out), what is the failure mode? The document says "narrative continues" but does not specify whether the extraction is retried, skipped, or queued |
| Duplicate prevention | `SkippedAlreadyProcessed` is listed as an outcome, but the idempotency key mechanism is not described. What makes a turn "already processed"? Is it the turn number, the message ID, the transcript hash? |

**Recommendation:** Expand §13 into a dedicated section with:

- The extraction prompt template (or a reference to a template file)
- The idempotency key definition
- Retry/failure handling
- Quality assessment (how the engine decides to accept vs. discard ambiguous output)
- An example walkthrough: "LLM generates X → extraction produces Y → normalization produces Z → engine validates → delta applied"

### 5.2 Weakness: The Interaction-Direction Table is Referenced but Not Defined

The document references an "interaction-direction table" as a strong design choice in §14.1:

> "The interaction-direction table remains a strong design choice and should stay."

But the document does not show, define, or describe this table anywhere. It appears to be a mechanism that validates whether a proposed relationship delta makes sense based on the direction of the interaction (e.g., player initiates trust toward NPC → trust increases on NPC's stat block toward player). Without seeing the table, the reader cannot assess whether this mechanism is sound.

**Recommendation:** Either include the interaction-direction table definition in §14.1 or reference it as an external file. At minimum, provide 2–3 example rules to illustrate the mechanism.

### 5.3 Weakness: The Combat System is Thin Relative to Its Complexity

Combat is a Tier C feature (degrades gracefully if unavailable) and the document correctly bounds it to "no initiative, no conditions, no weapon tables, no range logic, one deterministic round at a time." However, even within these bounds, several mechanics are unspecified:

| Gap | Description |
|-----|-------------|
| Damage calculation | How is damage computed? (e.g., $D = \text{weapon\_value} - \text{target\_defense}$, or purely fixed) |
| HP resolution at 0 | What happens when HP reaches 0? Is the entity deleted, marked defeated, or flagged? |
| Combat resolution finality | Does Anansi resolve combat and then the LLM narrates the outcome? Or does Anansi merely apply the stat changes and the LLM narrates the full combat? |
| Turn-order in "one round" | If multiple combatants are involved, does a "round" mean one action per entity or one attack exchange? |

**Recommendation:** Add a combat resolution state machine and at least one worked example. For example:

> "Combat round N: User attacks with sword (weapon value 3) against NPC (defense value 1). Damage = 3 - 1 = 2. NPC HP reduced by 2. If NPC HP ≤ 0, NPC enters `defeated` state."

### 5.4 Weakness: The Addon Surface Lacks Interface Specifications

Section §9 defines four first-class addon capabilities:

1. `ContextSlotProvider`
2. `PostGenerationHook`
3. `ToolProvider`
4. `PaneProvider`

These are named but not defined with Rust trait signatures. The ozone+ v0.4 document has a full type system (§11) that defines identifiers, messages, branches, events, and error taxonomy. Anansi's addon surface should either:

- Include the full trait signatures for each capability, or
- Reference a specific section of `ozone_v0.4_design.md` where the addon traits are defined

Without the interface definitions, the unified addon surface described in §9 is a conceptual promise, not a technical contract.

**Recommendation:** Add the four trait definitions to §9 (or to an appendix), following the same rigor as the ozone+ type definitions. Include method signatures, return types, error handling expectations, and lifecycle notes (e.g., when each capability is called).

### 5.5 Weakness: No Discussion of Multi-Entity Extraction Conflicts

The document assumes a single player interacting with one NPC at a time. In practice, narrative can involve:

- Two NPCs interacting (Anansi-agnostic per §3.1 non-goals)
- Multiple named entities in a single narrative turn
- Entities that the LLM conflates or splits in extraction

The document does not describe how the extraction pipeline handles a single turn that references 5 named entities with 10 relationship deltas simultaneously. Specifically:

- Is there a maximum deltas per turn?
- What happens if two deltas conflict (e.g., "+3 trust" and "-5 trust" for the same entity in the same turn)?
- Does the engine apply deltas in order, batch them, or merge them first?

**Recommendation:** Add a delta conflict resolution policy. The simplest approach: deltas for the same stat of the same entity in the same turn are summed (e.g., "+3 and -5 = -2 net").

### 5.6 Weakness: The Roadmap Has No Testability Criteria

Phases 1A–1J have "Exit criterion" text for each phase. These are improvement over v0.3's monolithic milestones. However, several exit criteria are vague:

| Phase | Exit Criterion (as written) | Problem |
|-------|---------------------------|---------|
| 1A | "entity CRUD and serialization work" | What does "work" mean? Unit tests pass? Integration tests pass? |
| 1D | "ozone+ compiles with zero behavior change when no addons are loaded" | This is good. |
| 1E | "bounded soft-context injection works and can be omitted cleanly" | "Works" is undefined. What test verifies correctness? |
| 1F | "new entities and relationship changes appear correctly across normal RP turns without duplicate processing" | This is the most specific and best exit criterion in the list. |
| 1J | "stable full-session loop" | "Stable" is undefined. |

**Recommendation:** Replace vague exit criteria with concrete test predicates. For example, Phase 1E should read: "The `[GAME STATE]` block is injected on every generation turn. When the token budget is reduced to 200 tokens, the block is omitted. When the block is omitted, no game state changes are lost from the generation context."

### 5.7 Weakness: No Error Taxonomy for Anansi-Specific Errors

The ozone+ v0.4 document has a comprehensive error taxonomy (§12) with severity levels, user visibility, and retry policies. Anansi has its own error surface (extraction failures, validation rejections, combat resolution errors, audit trail corruption) that is not taxonomized.

**Recommendation:** Add an `AnansiError` enum analogous to `OzoneError`, or define Anansi-specific variants within the ozone+ error framework. At minimum, cover: extraction failure, validation rejection, combat resolution failure, state corruption, and addon API mismatch.

---

## 6. Summary Assessment

### 6.1 Fitness Within Ozone Family: ✅ Strong

The positioning is correct, the dependency chain is honest, and the addon surface future-proofs the family architecture.

### 6.2 Logical Flow: ⚠️ Good with Targeted Gaps

The document's macro-structure is sound. The micro-structure has gaps in cross-referencing, data model presentation, and traceability from roadmap to success criteria.

### 6.3 Ambiguity Level: ⚠️ Moderate

The most critical ambiguities are in extraction pipeline mechanics, mood resolution ownership, combat trigger conditions, and the `[GAME STATE]` injection mechanism. These are not cosmetic — they are the core interfaces of the system.

### 6.4 Overall Design Quality: Strong Foundation, Implementation-Ready After Clarification

The design correctly identifies the hardest problems (extraction uncertainty, LLM trust, scope management) and makes defensible choices. The six-stat block, deterministic core, manual correction surface, and tiered degradation model are all sound. The document is not ready for implementation as-is because the extraction pipeline and combat system are underspecified, but it is ready for a design review iteration that fills those gaps.

---

## 7. Recommended Next Steps

**High Priority (blocking implementation):**

1. **Expand §13** with extraction prompt template, idempotency key mechanism, quality gates, and error handling
2. **Define the mood resolution model** (who sets it, how it interacts with the stat block, whether it's user-settable or derived)
3. **Define the combat state machine** with explicit triggers, transitions, and at least one worked example
4. **Add the four addon capability trait definitions** to §9 (or reference them from ozone+ baseline)

**Medium Priority (implementation quality):**

5. **Add the interaction-direction table** to §14.1 with 3–5 example rules
6. **Replace vague roadmap exit criteria** with concrete test predicates
7. **Trace roadmap phases to MVP success criteria** (§6 ↔ §19)
8. **Define the `[GAME STATE]` injection mechanism** (context layer integration vs. lorebook injection)
9. **Add Anansi-specific error taxonomy** (or reference ozone+'s error framework)

**Lower Priority (documentation quality):**

10. Fix the data model presentation: the `StatBlock` code block should show all entity fields, not just the stat fields
11. Add a system dependency diagram early in the document showing how extraction → validation → audit → context injection → TUI surfaces connect
12. Add a section specifically covering the swipe/regenerate flow end-to-end

***********************************************************************************************************************************************************



# Analysis of Anansi Design Plan and Its Fit in the Ozone Product Line

**Model:** Trinity Large Thinking  
**Version:** 2025-04-13

---

## Overview

The Anansi design document presents a well-structured plan for a deterministic game-engine layer atop ozone+. It clearly positions Anansi as an addon for mechanical roleplay while maintaining ozone+ as the narrative-first host. The document excels in defining MVP boundaries, principles, and a phased roadmap, though it has some ambiguity in key areas like extraction and combat integration.

---

## How Anansi Fits Into the Ozone Product Line

| Build | Niche | Dependencies | Relationship to Anansi |
|-------|-------|--------------|------------------------|
| `ozonelite` | Lean backend control, minimal footprint | — | Independent, no overlap |
| `ozone` | Backend tuning and session management | — | Independent, no overlap |
| `ozone+` | Full local-LLM workflow, polished TUI, long-form roleplay | — | Host platform; Anansi runs as an addon |
| `anansi` | ozone+ plus deterministic game layer for mechanical RP | ozone+ | Dependent build; adds mechanical layer |

### Key Architectural Points
- **Addon Model**: Anansi uses ozone-core's general addon surface, making it the first concrete addon build. This keeps family boundaries honest and allows future addons.
- **Single Writer**: Both ozone+ and Anansi respect the `ConversationEngine` as the sole writer to canonical state, ensuring consistency.
- **Layered Integration**: Anansi injects game-state summaries into context, validates LLM proposals, and resolves deterministic combat—all without modifying the transcript.

### Positioning Rule
> Anansi is a **dependent build**, not a replacement for ozone+ and not a redefinition of the whole family.

This is a strong design choice that prevents scope creep and maintains the integrity of the ozone+ core product.

---

## Honest Assessment: Strengths and Weaknesses

### Strengths ✅

1. **Exceptional MVP Discipline**  
   The document clearly defines what is in and out of scope for the first release. The six-stat block, bounded context injection, and manual correction commands keep the initial implementation focused and testable.

2. **Strong Foundational Principles**  
   Principles like *"LLM proposes; the engine decides"*, *"Health is combat-only"*, and *"Canonical transcript stays sacred"* provide a solid ethical and technical foundation. They ensure transparency, determinism, and user control.

3. **Comprehensive Roadmap with Testable Phases**  
   The 13-phase implementation plan breaks down the work into independently testable artifacts. This is a significant improvement over monolithic milestones and reduces integration risk.

4. **Attention to Auditability and Recovery**  
   The event trail with confidence/fallback markers, manual correction commands, and graceful degradation make the system trustworthy and recoverable from errors.

5. **Unified Addon Surface**  
   Defining a single addon contract (`ContextSlotProvider`, `PostGenerationHook`, `ToolProvider`, `PaneProvider`) in ozone-core makes Anansi reusable and future-proof.

6. **Clear Technology Stack**  
   The use of Bevy ECS (standalone), Serde, and ozone+ crates is well-justified. Excluding `bevy_app` to avoid a continuous game loop is a smart move for an event-driven addon.

### Weaknesses & Areas for Improvement ⚠️

1. **Fixed Six-Stat Block Limitation**  
   While MVP discipline is good, the rigid schema may prove too narrow even for early adopters. Consider allowing optional extensions via configuration or a future major version to avoid a second refactor.

2. **Extraction Pipeline Vagueness**  
   The document mentions two logical jobs (entity discovery, relationship-change extraction) but lacks detail on implementation. How will ambiguous references be resolved? What about pronoun resolution and fuzzy merging?

3. **Combat System Integration**  
   The combat tool is described as "simple deterministic combat through a tool interface when backend support exists," but the integration with narrative flow is unclear. Examples of combat notation and trigger conditions would help.

4. **Missing Manual Correction Commands in MVP**  
   The roadmap mentions manual correction commands, but they are not explicitly listed in the MVP section. Clarify exactly which commands will be available at launch (e.g., `create entity`, `rename entity`, `set stat`, `set HP`, `merge`, `start/end combat`).

5. **No Explicit Testing Strategy**  
   While the roadmap is testable, the document lacks a dedicated testing section. Add unit, integration, and property-based tests for deterministic systems (e.g., validation, fallback logic).

6. **Length and Redundancy**  
   The document is lengthy (over 5,000 words) with some repetitive content (principles appear multiple times). Consider a tighter executive summary and move detailed rationales to appendices.

7. **Performance Considerations**  
   The MVP should be efficient, but no performance targets are defined. Add basic metrics (e.g., context assembly < 500ms, TUI frame time < 33ms) to guide implementation.

8. **Success Metrics Missing**  
   How will you know the MVP is successful? Define measurable criteria: user retention, manual correction frequency, error rates, or qualitative feedback on trustworthiness.

---

## Analysis of the Plan Itself

### Logical Flow

The document follows a logical progression:
1. **Definition & Purpose** → 2. **Goals & Fit** → 3. **Scope & Principles** → 4. **Implementation Details** → 5. **Roadmap & Risks**

However, some sections could be reordered for clarity:
- Move **Technology Stack** and **Workspace Structure** earlier (after Design Goals) to ground the reader in the implementation reality.
- Combine **TUI Surfaces** and **Config System** after **Data Model** and **Persistence** for a more cohesive flow.
- The **Risk Register** fits well at the end, but a **Metrics for Success** section would logically precede it.

### Ambiguity Reduction

The document does a good job clarifying earlier ambiguities (e.g., ozone+ is Anansi-agnostic, not unaware). Remaining ambiguities:

1. **Extraction Pipeline Details**  
   - What exactly triggers extraction? (After every assistant turn? Only on explicit commands?)
   - How are entities merged when names are slightly different? (The document says "exact normalization in MVP, merge command for recovery" but doesn't define "exact".)
   - What is the fallback when extraction fails? (Degraded mode halves deltas, but is that clearly communicated to the user?)

2. **Combat Tool Dependencies**  
   - What does "backend support exists" mean? (Function calling? A specific model?)
   - If tool-calling is unavailable, does combat simply not happen, or is there a fallback? (The document says "Anansi should not attempt fake deterministic combat," which is good, but the UI should reflect this clearly.)

3. **Context Injection Bounds**  
   - The `[GAME STATE]` block has strict token caps, but how are entities and changes selected when over budget? (Oldest entities removed first, then recent changes—this should be explicit.)

4. **Manual Correction Scope**  
   - Which stats can be manually set? (All six? HP only?) The document says "set stat, set HP" in the roadmap but not in the MVP definition.

### Recommendations for Clarity

1. **Add a "How It Works" Section**  
   Provide a concrete example:  
   - User: *"Elara swings her sword at the goblin."*  
   - LLM proposes: `"combat: attack, target=goblin, damage=5"`  
   - Anansi validates: checks `hostility` stat, applies clamped delta, records audit trail.  
   - Output: `"Elara hits the goblin for 5 damage (HP: 12/20)."`

2. **Define Extraction Rules Precisely**  
   - Entity discovery: exact name match, case-insensitive, from last 10 messages.  
   - Relationship changes: extracted via pattern matching (e.g., `"trust increased"` → `delta = +2`).  
   - Fallback: if parsing fails, log `DiscardedMalformed` and skip.

3. **Specify Combat Notation**  
   - Example: `"[combat] attack goblin 5"` triggers a deterministic round using seeded resolution.  
   - If backend lacks tool support, the notation is ignored and narrative continues.

4. **List MVP Commands Explicitly**  
   In the MVP section, include:
   ```markdown
   - `/anansi create-entity <name> <type>`
   - `/anansi set <entity> <stat> <value>`
   - `/anansi set-hp <entity> <value>`
   - `/anansi merge <entity1> <entity2>`
   - `/anansi start-combat`, `/anansi end-combat`
   ```

---

## Implementation Guidance Summary

### Build First (Phases 1A–1F)
- **Phase 1A–1C**: Foundation, engine, TUI—ensure core conversation works without Anansi.
- **Phase 1D–1F**: Add Anansi integration, context assembly, and manual corrections. This is the critical path for MVP.

### Build Second (Phases 2A–2C)
- Memory system: pinned + keyword search → vector + hybrid → summaries + lifecycle.

### Build Later (Phases 3+)
- Assistive layer, group chat, plugins, release polish.

### Anti-Goals for Early Versions
- Full hidden-intelligence orchestration
- Automatic narrator authority
- Complex auto-world-building
- Regex lorebook matching (substring is sufficient)
- Multi-modal content

---

## Closing Evaluation

The Anansi design is **strong and trustworthy** for an MVP. It avoids the common pitfall of over-engineering and focuses on proving the core loop: *propose → validate → decide → inspect*. The phased roadmap and clear boundaries make it a low-risk addition to the Ozone family.

**To ship a successful v0.2:**
1. Tighten the extraction and combat specifications with concrete examples.
2. Explicitly list manual correction commands in the MVP definition.
3. Add a testing strategy and success metrics.
4. Consider a slight extension to the six-stat block (e.g., configurable via config) to future-proof without complicating MVP.

With these tweaks, Anansi will deliver on its promise: a **trustworthy mechanical layer** that enhances roleplay without overshadowing the narrative.

---

## Model and Version

**Model:** Trinity Large Thinking  
**Version:** 2025-04-13  
**Based on:** Analysis of Anansi Design Document v0.2 and Ozone+ v0.4 Baseline

************************************************************************************************************************************************************************************************

**Model: Gemini 3.1 Pro Preview**

# Anansi Integration & Design Analysis

Anansi v0.2 serves as a deterministic, mechanical roleplaying layer built on top of the narrative-focused `ozone+` host. Its foundational philosophy—"The LLM proposes. The engine decides"—is an excellent safeguard against the volatility of local LLM extraction. 

Below is an analysis of how Anansi fits into the Ozone product line, an honest assessment of its strengths and weaknesses, and actionable feedback to reduce ambiguity in the design plan.

---

## 1. Fit Within the Ozone Product Line

Anansi maps perfectly to the `ozone+` v0.4 architectural paradigm. The `ozone+` design strictly enforces that the core application remains a text-first narrative engine where the canonical transcript is "sacred." 

Anansi respects this by acting as a true downstream consumer and non-destructive contributor:
*   **Single-Writer Compliance:** Anansi does not violate the `ConversationEngine`'s single-writer architecture. It utilizes the unified unified addon surface (`PostGenerationHook`, `ContextSlotProvider`) to observe the generated text and inject bounded `[GAME STATE]` context without mutating the raw `messages` table.
*   **System Degradation:** `ozone+` is deeply focused on graceful degradation (e.g., falling back from vector search to FTS5, or exact counting to heuristic). Anansi mirrors this exactly with its confidence markers and `AcceptedDegraded` rules. If Anansi's extraction fails, pure `ozone+` roleplay continues uninterrupted.
*   **Hardware and Concurrency:** By relying on `bevy_ecs` purely for data-modeling and omitting the real-time `bevy_app` game loop, Anansi avoids competing with the host application's async `tokio` runtime or the `HardwareResourceSemaphore`.

---

## 2. Strengths of the Design

*   **Ruthless MVP Discipline:** By strictly limiting the MVP to a fixed six-stat block, no inventory, and no complex initiative loops, the document practically guarantees that the first version will actually ship. 
*   **Auditability:** The decision to log *why* a stat changed (direct parser, magnitude fallback, bounded cap) solves the "hidden GM" problem that plagues LLM-driven games. 
*   **Stateless Replay in Swipes:** Tying combat outcomes to a seeded deterministic key ensures that when users use `ozone+`'s swipe/retry mechanism ($\text{Swipe} = \text{Regenerate}$), the UI doesn't continuously inflict stacked damage on the entity.
*   **Idempotency & Skip over Rollback:** Instead of trying to write a complex reverse-delta system for undone turns, simply preventing duplicate processing and relying on manual user corrections shifts the complexity burden from code to UI, which is much safer for $v1.0$.

---

## 3. Weaknesses & Areas for Improvement

While the plan is strong, there are structural friction points when interfacing with the `ozone+` engine.

*   **Rigid Six-Stat MVP vs Context Window:** The six-stat array (`health`, `trust`, `affection`, `anger`, `fear`, `hostility`) might overlap or clash semantically. For example, high `affection` and high `hostility` might confuse smaller local LLMs. 
*   **Resource Contention Ambiguity:** Anansi uses background extraction after generation. However, local LLMs are resource-bound. If the user fires off another generation immediately, does Anansi's extraction (which uses inference) yield to `ozone+`'s `HardwareResourceSemaphore`?
*   **Combat TUI Degradation:** The document states that if a combat tool call fails, Anansi will "run without mechanical enforcement." It is unclear *how* this is communicated to the user without breaking immersion.

<details>
<summary><strong>► Expand for Technical & Mathematical Edge Cases</strong></summary>

### Edge Case: Context Token Budgeting
Anansi specifies a soft-context cap of $400$ tokens (`[GAME STATE]`). However, if `ozone+` is operating under a strict budget formula where:
$$\text{Remaining Budget} = \text{Total Limit} - \sum (\text{Hard Context Layers})$$
There is no explicit logic on how Anansi's context competes with `ozone+` soft context (like Lorebook entries). Does `[GAME STATE]` take priority over a `LorebookEntry`? 

### Edge Case: Normalization Magnitude Caps
The config specifies `base_magnitude = 3` and `delta_cap = 10`. The math for applying deltas needs explicit clamping rules to avoid overflow logic in `u8` (where max is $255$).
Let $S_t$ be the current stat, and $\Delta$ be the proposed model change. The clamping function must be rigorously enforced natively:
$$ S_{t+1} = \max(0, \min(255, S_t + \max(-\text{delta\_cap}, \min(\Delta, \text{delta\_cap})))) $$
This should be explicitly documented so developers don't accidentally permit a delta that overflows the $u8$.

</details>

---

## 4. Flow of the Design & Ambiguity Reduction

The document flows exceptionally well. The progression from Non-Goals $\rightarrow$ Core Principles $\rightarrow$ Tiers $\rightarrow$ Checklists is a masterclass in product design. However, there are two areas of ambiguity that need tightening.

### Ambiguity 1: Interaction with Swipes and "Skipping"
Section 4.6 states: *"Anansi does not attempt automatic rollback... Instead it prevents duplicate processing..."*
**The Fix:** You need to explicitly define the idempotency key. In `ozone+`, every generation has a unique `MessageId`. Anansi should state clearly: 
```text
"Extraction jobs are keyed to the `MessageId`. If a user swipes to a new candidate (new `MessageId`), Anansi runs extraction for the new candidate. If a user swipes back to an already-processed candidate, the engine checks the `event` table via `MessageId` and skips extraction."
```

### Ambiguity 2: Unified Addon Registry Bootstrapping
Section 9 defines the `AddonRegistry`, but `ozone+` operates on an asynchronous `tokio` event loop while Anansi uses a standalone `bevy_ecs` world. 
**The Fix:** Clarify the threading boundary. Define whether the `bevy_ecs` world lives inside an `Arc<RwLock<World>>` shared across the `ozone-engine` threads, or if `anansi-bridge` communicates with an isolated Anansi thread via `mpsc` channels similar to `ozone+`'s primary architecture.

<details>
<summary><strong>► Expand for Suggested Document Flow Tweaks</strong></summary>

1.  **Move Config Up:** Move section 17 (`Config System`) to directly follow Section 10 (`Data Model`). Understanding the hard limits (like max entities and delta caps) helps contextualize the ingestion rules in Section 13.
2.  **Explicit DB Triggers:** In Section 11 (`Persistence`), since `ozone+` relies heavily on FTS5 triggers and $O(1)$ closure tables, explicitly state if Anansi's `anansi_*` tables will require their own FTS indexing, or if Anansi state is completely exempt from search functionality.
3.  **Cross-reference Phase Mapping:** In Section 19 (`Revised Roadmap`), explicitly map Anansi's phases to `ozone+`'s phases. For instance, Anansi's *Phase 1D (Unified Addon API)* logically cannot exist until `ozone+` *Phase 1E (Context Assembly)* is finished. Marking these hard dependencies will save engineering headaches.

</details>





***************************************************************************************************************************************************************************************************

`Qwen3.6 Plus`

---

## 1. Product Line Fit Analysis

Anansi is positioned correctly as a **dependent addon** to `ozone+`, not a parallel product or a core feature bundle. Its architectural alignment with the Ozone family is strong, provided the implementation strictly observes the host/addon boundaries outlined in both documents.

| Dimension | Ozone+ Baseline | Anansi Integration | Fit Verdict |
|-----------|----------------|-------------------|-------------|
| **Architecture** | Single-writer `ConversationEngine`, channel-based event flow | Reads committed transcript events, proposes deltas via `PostGenerationHook` | ✅ Aligned |
| **Persistence** | One SQLite DB per session, append-only transcript | Namespaced `anansi_*` tables, audit event trail | ✅ Aligned |
| **Context Assembly** | `ContextLayerPolicy` with hard/soft budgeting | `[GAME STATE]` soft context injection | ⚠️ Requires explicit layer integration |
| **TUI** | `ratatui` layout, responsive inspector, configurable panes | Right-side compact pane + inspector commands | ✅ Aligned |
| **Inference Gateway** | Capability-based backend probing, tool/provider surface | `ToolProvider` for combat resolution | ⚠️ Needs dynamic registration contract |
| **Core Philosophy** | Deterministic first, optional intelligence, graceful degradation | "LLM proposes, engine decides", fallback halving, skip-not-rollback | ✅ Strongly aligned |

**Key Positioning Strength:** Anansi does not attempt to replace `ozone+`'s narrative engine. It consumes committed narrative turns, applies mechanical validation, and injects bounded state back into the context window. This preserves `ozone+`'s deterministic core while enabling a mechanical RPG layer.

<details>
<summary><strong>Extended Technical Mapping (Ozone+ → Anansi Bridges)</strong></summary>

- `ozone-core` addon traits → `anansi-bridge` implements `ContextSlotProvider`, `PostGenerationHook`, `ToolProvider`, `PaneProvider`
- `ozone-engine` event bus → Anansi listens to `OzoneEvent::MessageCommitted` to trigger extraction
- `ozone-persist` session DB → Anansi writes to `anansi_entities`, `anansi_events` within the same WAL transaction via engine command routing
- `ozone-tui` pane registry → Anansi registers compact view + inspector via `PaneProvider`
- `ozone-inference` gateway → Receives tool definitions from Anansi at session initialization; routes combat tool calls to Anansi's ECS resolver
</details>

---

## 2. Strengths Assessment

Anansi v0.2 is a highly disciplined design document. Its strongest attributes are:

1. **Explicit MVP Containment:** The hard non-goals and tiered scope prevent the classic "addon bloat" trap. Keeping inventory, world flags, and NPC↔NPC relationships firmly post-MVP is the right call.
2. **Trust & Auditability Focus:** Confidence markers, fallback halving, audit trails, and manual correction commands directly address the #1 failure point of LLM-driven game systems: silent state corruption.
3. **ECS Without the Loop:** Choosing `bevy_ecs` standalone for data modeling and system execution while rejecting `bevy_app` is architecturally sound. Anansi is event-driven by `ozone+`, not a real-time tick loop.
4. **Combat Degradation Handling:** Refusing to fake combat when tool-calling is unavailable protects user expectations. The UI explicitly surfaces unavailability instead of masking it.
5. **Roadmap Exit Criteria:** Every phase (1A–1J) has a concrete, testable exit condition. This enables incremental shipping and prevents integration paralysis.

---

## 3. Weaknesses & Risk Assessment

Despite its strengths, several architectural friction points exist between Anansi and `ozone+`'s existing guarantees:

| Weakness | Impact | Root Cause |
|----------|--------|------------|
| **Context Budget Collision** | `[GAME STATE]` competes with `ozone+`'s hard/soft layers, risking prompt overflow | Anansi treats context injection as standalone, not as a formal `ContextLayer` |
| **Single-Writer Violation Risk** | Manual correction commands could bypass `ConversationEngine`, breaking transactional guarantees | Unclear write path for user-applied stat deltas |
| **Entity Normalization Fragility** | Exact-string matching will create duplicates from pronoun/case/typo drift | LLMs rarely output identical entity names across turns |
| **Combat Tool Lifecycle Ambiguity** | No explicit handoff for tool registration, parameter validation, or response routing | `ToolProvider` trait is declared but not wired to inference gateway init/teardown |
| **Fixed 6-Stat Schema Rigidity** | Limits MVP adoption for genres needing fewer/different mechanics | Hardcoded `StatBlock` lacks a configuration fallback hook |

---

## 4. Actionable Feedback & Improvements

### 4.1 Context Injection Alignment
Anansi's `[GAME STATE]` should register as a formal soft context layer rather than a manual injection string.

```rust
// Proposed integration into ozone-core context types
enum ContextLayerKind {
    // ... existing variants ...
    AddonSoftContext { priority: u8, budget_pct: f32, content: String },
}
```
**Why:** This lets `ozone+`'s `ContextAssembler` handle budgeting, compression, and drop-first policies automatically. Anansi loses no control but gains predictability.

### 4.2 Manual Correction Routing
Corrections must flow through the engine's command channel to preserve the single-writer guarantee:

```rust
enum Command {
    // ... existing ...
    ApplyAddonStateDelta { addon_id: &str, entity_id: Uuid, delta: StatDelta, reason: String },
}
```
The `anansi-bridge` crate should emit this command. `ConversationEngine` validates, persists, and broadcasts `AddonStateApplied`. This keeps undo/redo, audit trails, and WAL transactions intact.

### 4.3 Lightweight Alias Fallback
Even in MVP, exact normalization will break under LLM variance. Add a simple string-distance fallback:
- Normalize to lowercase, strip punctuation
- Cache last $N$ seen aliases per entity
- If new name has Levenshtein distance $\leq 2$ (or substring match $> 80\%$), queue for merge confirmation rather than creating a new entity
- Keep this deterministic, avoid fuzzy pronoun resolution

### 4.4 Combat Tool Registration Contract
Define explicit lifecycle hooks in `ToolProvider`:
```rust
trait ToolProvider {
    fn register_tools(&self, gateway: &mut InferenceGateway) -> Vec<ToolDefinition>;
    fn handle_tool_call(&self, call: ToolCall) -> ToolResponse;
    fn deregister_tools(&self, gateway: &mut InferenceGateway);
}
```
Combat results should inject back into the transcript as a system message with `[GAME STATE: COMBAT]` prefix, ensuring they're captured by the extraction pipeline but marked as non-reproposable text.

### 4.5 Stat Schema Extensibility Hook
Keep the 6-stat block for v0.2, but allow a config override that disables unused stats:
```toml
[anansi.stats]
enabled = ["health", "trust", "hostility"] # disables unused stats from validation/context
```
This preserves MVP simplicity while preventing prompt waste for campaigns that only care about combat or social dynamics.

---

## 5. Document Flow & Ambiguity Reduction

### 5.1 Logical Flow Assessment
The document is well-structured. It progresses logically from **purpose → fit → boundaries → principles → tech → data → pipeline → validation → combat → UI → config → roadmap → risks**. The tiered scope and explicit non-goals prevent conceptual drift.

### 5.2 Ambiguities to Resolve Before Implementation

| Section | Ambiguity | Suggested Clarification |
|---------|-----------|-------------------------|
| **§10.5 Correction Metadata** | How does `lock_name` interact with Ozone+ message editing? | Specify that locked names survive entity normalization but are overwritten by explicit merge commands. |
| **§13.2 Eligibility** | "extension-authored combat notation" is undefined | Define exact prefix/syntax (e.g., `[COMBAT:ATTACK]...[/COMBAT]`) so extraction can reliably skip it. |
| **§14.2 Audit Markers** | "whether the model proposed it directly" | Clarify this refers to LLM-extracted delta vs. engine-fallback computed delta. Add a `proposal_source` enum. |
| **§15.2 Tool Degradation** | How does combat resume when tool support returns? | Add explicit re-initialization hook: combat state persists, but new turns use tool only after capability probe succeeds. |
| **§16.4 Manual Commands** | Conflict with Ozone+ `/undo`? | Document that addon state changes are tracked as a separate undo scope. `/undo` reverts last message; `/addon-revert` reverts last stat delta. |
| **§19 Roadmap** | Phase ordering assumes linear dependency | Add dependency graph: `1D (Addon API) → 1E/F/G/H (Anansi features) → 1I/J (Integration)`. Phase 1D is a hard blocker for 1E+. |

### 5.3 Structural Recommendations
1. **Add a "Hook Execution Order" diagram:** Show exactly when `PostGenerationHook` runs relative to `ozone+`'s memory jobs, context assembly, and persistence. A simple sequence diagram prevents race conditions.
2. **Define State Versioning for Swipes:** Section 4.6 says "skip, not rollback," but doesn't specify how swipe/regenerate prevents duplicate extraction. Add a `turn_sequence_id` or `generation_fingerprint` that the extraction pipeline checks before applying deltas.
3. **Clarify Tool Parameter Validation:** Combat tool calls from the LLM will be JSON. Define a strict schema (e.g., `{"action": "attack", "target_id": "...", "weapon": "sword"}`) and specify who validates it (Anansi's validation layer, not the LLM).

---

## Summary Verdict

Anansi v0.2 is a **strong, implementation-ready baseline** that correctly respects `ozone+`'s architectural boundaries. Its focus on bounded scope, auditability, and trust-first design directly addresses the historical failures of LLM-driven RPG layers. 

**Primary action items before coding:**
1. Wire `[GAME STATE]` into `ozone+`'s `ContextAssembler` as a formal soft layer.
2. Route all manual corrections through `ConversationEngine`'s command channel.
3. Add a lightweight alias fallback and explicit tool registration lifecycle.
4. Clarify swipe/regenerate deduplication using generation fingerprints.

If these integration points are resolved, Anansi will ship as a highly trustworthy, deterministic mechanical layer that scales cleanly into Phase 2+ world simulation.


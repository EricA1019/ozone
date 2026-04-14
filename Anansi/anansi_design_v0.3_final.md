# Anansi — Design Document v0.3

**Product:** anansi  
**Version:** 0.3  
**Status:** Final pre-implementation baseline  
**Depends on:** ozone+ (ozone v0.4+)  
**Companion baseline:** `ozone_v0.4_design.md`  
**Supersedes:** Anansi Design v0.2 (Revised)  
**Purpose of this revision:** fill specification gaps identified during multi-model review — extraction pipeline, addon contracts, entity identity resolution, combat state machine, error taxonomy, and integration contracts — while preserving the v0.2 architecture and MVP discipline that were unanimously praised.

---

## What Anansi Is

Anansi is a deterministic game-engine layer that runs underneath freeform roleplay in ozone+. It tracks entities, governs relationship math, validates LLM-proposed state changes, and resolves simple combat while leaving the LLM responsible for narration rather than rule authority.

Anansi is **not** a default ozone+ feature bundle. It is a downstream product build for users who want mechanical roleplay layered under narrative play. Users who want clean local-LLM roleplay without game mechanics stay on ozone+ alone. Users who want a bounded RPG layer install Anansi.

The core product promise is simple:

**The LLM proposes. The engine decides. The user can inspect and correct the result.**

### System Dependency Overview

```
User message → ozone+ generation → assistant message commit
                                        │
                                        ▼
                              PostGenerationHook fires
                                        │
                                        ▼
                    ┌───────────────────────────────────┐
                    │      Anansi Extraction Pipeline     │
                    │  (entity discovery + delta extract) │
                    └───────────────────┬───────────────┘
                                        │
                                        ▼
                    ┌───────────────────────────────────┐
                    │     Validation & Fallback Logic     │
                    │  (direction table, clamping, audit) │
                    └───────────────────┬───────────────┘
                                        │
                          ┌─────────────┴─────────────┐
                          ▼                           ▼
                   ECS World Update            Audit Event Trail
                   (entities, stats)           (confidence, markers)
                          │                           │
                          ▼                           ▼
                   [GAME STATE] block          Inspector / TUI
                   (injected next turn)        (user can inspect)
```

---

## 1. Design Goal

Build Anansi as an addon-style product on top of ozone+ that:

- Tracks named entities discovered from the narrative with a lightweight identity resolution strategy
- Maintains a fixed six-stat relational/combat block per entity in v0.3
- Validates and bounds LLM-proposed stat changes before committing them
- Resolves simple deterministic combat through a tool interface when backend support exists
- Injects a bounded, structured game-state summary into context as a registered soft context layer
- Persists state to the same session database used by ozone+ with namespaced tables
- Exposes a narrow, versioned addon contract with full trait signatures reusable by future Ozone-family addons
- Makes state visible, inspectable, and manually correctable when extraction gets things wrong
- Routes all state mutations through ozone+'s engine command channel to preserve single-writer guarantees
- Separates MVP from later expansion so the first implementation can ship without inventory, world simulation, or heavy system creep

---

## 2. Product Fit Within the Ozone Family

| Build | Niche | Depends On |
|------|------|------------|
| `ozonelite` | Lean backend control, minimal footprint | — |
| `ozone` | Backend tuning and session management | — |
| `ozone+` | Full local-LLM workflow, polished TUI, long-form roleplay | — |
| `anansi` | ozone+ plus deterministic game layer for mechanical RP | ozone+ |

### 2.1 Positioning rule

Anansi is a **dependent build**, not a replacement for ozone+ and not a redefinition of the whole family.

### 2.2 Host/addon clarification

Earlier drafts described ozone+ as "unaware of Anansi." The correct architectural statement is narrower:

- ozone+ is **Anansi-agnostic**, not Anansi-specific
- ozone-core exposes a general addon surface (see §9 for the full addon surface specification)
- ozone+ hosts zero or more addons through that surface
- Anansi is the first concrete addon build using it

This keeps family boundaries honest:
- family-level positioning remains outside the Anansi doc
- ozone+ remains the narrative-first host
- Anansi remains a mechanical specialization layered on top

---

## 3. Non-Goals

### 3.1 Hard non-goals for MVP

- Web UI or graphical client
- Multiplayer or shared world state
- Open-ended rules engine
- Full CRPG combat stack
- Full itemization, inventory grids, loot tables, or economy simulation
- NPC↔NPC relationship graphing
- World-state gates, quest graph systems, or faction simulation
- Group-chat actor attribution
- Fuzzy pronoun resolution and deep alias inference
- Fine-tuned extraction models as a requirement for viability
- Rewriting or mutating ozone+'s canonical transcript

### 3.2 Explicitly deferred, not rejected

These are desirable later, but **must not produce MVP complexity**:

- Initiative system
- Conditions and status effects
- Social skill checks separate from extraction
- Export/import improvements beyond core entity import
- Inventory and equipment
- Item modifiers and traits
- World flags and gate logic
- NPC↔NPC relationships
- Fuzzy alias merging / pronoun resolution
- Group chat attribution
- In-TUI configuration panels

---

## 4. Foundational Principles

### 4.1 The LLM proposes; the engine decides

The LLM never writes game state directly. It can propose entities, interactions, and deltas. The engine validates, clamps, repairs, overrides, or discards those proposals.

### 4.2 Health is combat-only

Extraction never mutates `health`. All HP changes flow through combat or future explicitly-defined deterministic systems.

### 4.3 Deterministic replay

Combat uses a seeded resolution key so swipes, retries, and replays do not double-apply damage.

### 4.4 Canonical transcript stays sacred

Anansi consumes committed transcript events. It does not edit, reorder, compress, or replace conversation history.

### 4.5 Graceful degradation

When extraction, repair, or tool-calling fails, the narrative continues and game state remains stable.

### 4.6 Skip, not rollback

Anansi does not attempt automatic rollback of prior extraction effects during swipe/regenerate flows. Instead it prevents duplicate processing via idempotency keys (see §13.8) and provides manual correction tools.

### 4.7 Transparency over hidden mechanics

If Anansi affects narrative context or state, the user must be able to inspect what changed, why it changed, and whether the engine used fallback logic.

### 4.8 MVP discipline over feature hunger

A good first release proves trustworthiness, bounded state, and mechanical usefulness. It does not attempt to become a full simulation layer.

### 4.9 Single-writer through the engine

All state mutations — whether from extraction, combat, or manual correction — flow through ozone+'s `ConversationEngine` command channel. Anansi never writes directly to the ECS world or database from an external thread. This preserves transactional guarantees, undo/redo integrity, and audit trail consistency.

---

## 5. Scope Tiers

| Tier | Scope | Notes |
|------|------|-------|
| **A** | Entity registry, six-stat block, validation, mood resolution, persistence, manual correction | Deterministic core |
| **B** | Extraction pipeline, normalization, confidence marking, `[GAME STATE]` context injection | Inference-assisted, degradation-safe |
| **C** | Combat tool registration and resolution | Depends on tool-calling support |
| **D** | Inventory, item modifiers, conditions, world flags, richer simulation | Post-MVP only |

**Rule:** Tier A must remain useful even if Tier B and C are disabled or partially failing.

**Mapping note:** Tier D maps to Post-MVP Phases 2–4 as detailed in §19.

---

## 6. Honest MVP Boundary

### 6.1 Functional success criteria

MVP is successful if Anansi can do all of the following reliably:

1. Track a small registry of narrative entities with lightweight identity resolution
2. Apply bounded, validated relationship deltas after assistant turns
3. Persist those entities and deltas per session
4. Render a compact read-only game-state pane
5. Let the user inspect and manually correct obvious extraction mistakes
6. Resolve one deterministic combat round when tool-calling exists
7. Never corrupt transcript integrity or duplicate state changes during swipe/regenerate

MVP is **not** successful because it contains more systems. It is successful when it becomes trustworthy enough to keep enabled during normal roleplay.

### 6.2 Measurable success metrics

Beyond functional correctness, track these signals to evaluate whether the MVP is *working for users*:

| Metric | Target | Why it matters |
|--------|--------|---------------|
| Manual correction frequency | Decreasing over a session | Indicates extraction is learning-compatible or at least not getting worse |
| `DiscardedMalformed` rate | < 20% of extraction cycles | High discard = prompt template needs rework |
| `AcceptedDegraded` rate | < 40% of accepted cycles | High degradation = normalization rules too loose |
| Duplicate entity creation rate | < 1 per 20-entity session | Measures identity resolution effectiveness |
| User-initiated entity merges | Track, no target | Baseline for future fuzzy matching value |
| Combat tool availability | Binary per session | Track how often backends support tool-calling |

These are not pass/fail gates. They are signals for iterating extraction quality and UX after launch.

---

## 7. Technology Stack

### 7.1 Anansi-specific dependencies

| Crate | Purpose | Notes |
|------|---------|-------|
| `bevy_ecs` | ECS world, components, systems, events | Standalone only |
| `serde` + `serde_json` | Serialization | Shared convention |
| `thiserror` | Error types | Shared convention |
| `tracing` | Structured logging | Shared convention |
| `uuid` | IDs, idempotency keys | Shared convention |

### 7.2 ozone+ crates depended on

| Crate | What Anansi uses |
|------|-------------------|
| `ozone-core` | Shared types, addon traits, error taxonomy |
| `ozone-engine` | Post-commit event path, engine command channel |
| `ozone-inference` | Tool registration surface, backend requests |
| `ozone-persist` | Session DB access, migrations |
| `ozone-tui` | Pane registration, inspector integration |

### 7.3 Why `bevy_ecs` standalone

Anansi needs ECS data modeling and system execution, not a real-time game loop. `bevy_app` is deliberately excluded to avoid pretending the addon is a continuously ticking game runtime when it is actually event-driven by ozone+.

---

## 8. Workspace Structure

Anansi remains a separate workspace that depends on versioned ozone+ crates.

```text
anansi/
├── Cargo.toml
├── crates/
│   ├── anansi-core/
│   ├── anansi-game/
│   ├── anansi-bridge/
│   ├── anansi-tui/
│   └── anansi-cli/
├── tests/
├── docs/
│   └── anansi_design.md
└── anansi.toml.example
```

### 8.1 Crate responsibilities

**anansi-core**  
Shared types, ECS world setup, entity CRUD, event types, game-state snapshots, error taxonomy.

**anansi-game**  
Pure deterministic systems: validation, fallback computation, mood resolution, combat math, manual correction application.

**anansi-bridge**  
All ozone+ integration: extraction, normalization, persistence, command routing, tool registration, context contribution, audit metadata. Communicates with the Anansi ECS world via message channels (see §9.6).

**anansi-tui**  
Read-only projections, pane widgets, inspector surfaces, correction command affordances.

**anansi-cli**  
Startup wiring, registry registration, config merge, compatibility checks.

---

## 9. Unified Addon Surface

Earlier drafts described three traits while also relying on a pane integration concept. That split was honest but incomplete. The revised contract is a **single addon surface** with four first-class capabilities, each with a defined trait signature, lifecycle, and error contract.

### 9.1 Addon capabilities

1. `ContextSlotProvider` — contributes soft context to the generation pipeline
2. `PostGenerationHook` — observes committed messages and proposes state changes
3. `ToolProvider` — registers tool definitions and handles tool calls
4. `PaneProvider` — registers TUI panes and inspector surfaces

### 9.2 Trait definitions

```rust
/// Contributes a bounded soft-context block to the generation pipeline.
pub trait ContextSlotProvider: Send + Sync {
    /// Returns the context block content and metadata.
    /// Called during context assembly, before each generation.
    /// Returning None omits the block for this turn.
    fn provide_context(
        &self,
        budget: &ContextBudget,
    ) -> Result<Option<ContextSlot>, AddonError>;

    /// Declares this slot's priority and budget preferences.
    fn slot_metadata(&self) -> ContextSlotMetadata;
}

pub struct ContextSlotMetadata {
    pub addon_name: &'static str,
    pub priority: u8,              // 0-255, higher = kept longer under pressure
    pub is_hard_context: bool,     // false for Anansi
    pub max_tokens: usize,         // soft cap
    pub collapse_strategy: CollapseStrategy,
}

pub enum CollapseStrategy {
    TruncateTail,    // remove oldest items first
    OmitEntirely,    // all or nothing
}

/// Observes committed assistant messages and proposes state changes.
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
    pub message_id: MessageId,
    pub current_state_snapshot: Arc<SessionStateSnapshot>,
}

pub struct ProposedChange {
    pub kind: ProposedChangeKind,
    pub confidence: Confidence,
    pub audit_reason: String,
}

/// Registers tool definitions with the inference gateway and handles calls.
pub trait ToolProvider: Send + Sync {
    /// Called at session initialization. Returns tool definitions to register.
    fn register_tools(&self) -> Vec<ToolDefinition>;

    /// Called when the LLM invokes a registered tool.
    fn handle_tool_call(&self, call: ToolCall) -> Result<ToolResponse, AddonError>;

    /// Called at session teardown for cleanup.
    fn deregister_tools(&self);
}

/// Registers TUI panes and inspector surfaces.
pub trait PaneProvider: Send + Sync {
    /// Returns pane registration metadata.
    fn pane_metadata(&self) -> PaneMetadata;

    /// Renders the compact pane content for the current state.
    fn render_compact(&self, state: &AddonStateSnapshot) -> PaneContent;

    /// Renders the full inspector view for a selected entity or overview.
    fn render_inspector(
        &self,
        state: &AddonStateSnapshot,
        selection: Option<EntityId>,
    ) -> PaneContent;
}
```

### 9.3 Error propagation contract

- If `ContextSlotProvider::provide_context` returns `Err`, the context slot is omitted for that turn. The generation proceeds. The error is logged with `tracing::warn`.
- If `PostGenerationHook::on_generation_complete` returns `Err`, no state changes are applied. The narrative continues. The error is logged and a `degraded` confidence marker is emitted.
- If `ToolProvider::handle_tool_call` returns `Err`, the tool call fails gracefully. The LLM receives an error response. The TUI surfaces tool unavailability.
- If `PaneProvider::render_compact` returns `Err`, the pane shows a placeholder error state.

**No addon error ever aborts a generation or corrupts the transcript.**

### 9.4 Execution ordering

When multiple addons register the same capability:

- `ContextSlotProvider`: all providers are called. Slots are sorted by priority. Budget overflow drops lowest-priority slots first.
- `PostGenerationHook`: hooks execute sequentially in registration order. Each hook receives the state snapshot *after* the previous hook's changes were applied. This prevents race conditions.
- `ToolProvider`: tools from different providers share a namespace. Name collisions are rejected at registration time.
- `PaneProvider`: panes are listed in registration order. Users can reorder via config.

### 9.5 Suggested registry shape

```rust
pub struct AddonRegistry {
    pub context_slots: Vec<Arc<dyn ContextSlotProvider>>,
    pub post_generation_hooks: Vec<Arc<dyn PostGenerationHook>>,
    pub tool_providers: Vec<Arc<dyn ToolProvider>>,
    pub pane_providers: Vec<Arc<dyn PaneProvider>>,
}
```

### 9.6 Threading and concurrency model

Anansi's `bevy_ecs` World lives in an **isolated thread** owned by `anansi-bridge`. Communication between ozone+'s async `tokio` runtime and Anansi's ECS world uses bounded `mpsc` channels, consistent with ozone+'s existing channel-based architecture:

```
ozone-engine (tokio)                    anansi-bridge (dedicated thread)
       │                                        │
       │── MessageCommitted event ──────────►   │
       │                                        ├── extraction
       │                                        ├── validation
       │                                        ├── ECS world update
       │   ◄── ApplyAddonStateDelta command ────┤
       │                                        │
       │── AddonStateApplied confirmation ──►   │
       │                                        ├── audit trail write
```

**Why not `Arc<RwLock<World>>`:** Sharing the ECS world across threads would require the ozone-engine to understand Bevy's scheduling model and risk lock contention during extraction. Channel isolation keeps the concurrency model simple and testable.

**Why not fire-and-forget:** Extraction results must be validated by the `ConversationEngine` before persisting, to preserve the single-writer guarantee. The channel round-trip ensures this.

### 9.7 Stability rule

Changes to any addon capability are versioned API changes, not casual internal refactors.

---

## 10. Data Model

### 10.1 Universal six-stat block

```rust
pub struct StatBlock {
    pub health: u8,
    pub trust: u8,
    pub affection: u8,
    pub anger: u8,
    pub fear: u8,
    pub hostility: u8,
}
```

### 10.2 Stat purpose mapping

Each stat has a defined role. This prevents misinterpretation (e.g., that high hostility should auto-start combat):

```
Stat purposes in MVP:
- trust:      relationship flavor, informs mood, visible in context
- affection:  relationship flavor, informs mood, visible in context
- anger:      relationship flavor, informs mood, visible in context
- fear:       relationship flavor, informs mood, visible in context
- hostility:  relationship stat + combat readiness indicator. Does NOT trigger combat.
- health:     combat resource ONLY. Never touched by extraction.

Mood resolution uses: trust, affection, anger, fear, hostility
Combat damage uses:   health
Combat modifiers:     hostility (post-MVP only)
```

### 10.3 Why the schema stays fixed in MVP

The six-stat block is intentionally narrow so that:
- validation rules stay understandable
- context injection stays bounded
- TUI presentation stays compact
- persistence stays simple
- the first release proves the loop before generalization

### 10.4 Known limitation

This fixed schema is an MVP strength and a long-term limitation. Expansion into inventory, traits, corruption, loyalty, or faction reputation belongs to later versions, not the first implementation.

### 10.5 Entity model

```rust
pub struct Entity {
    pub id: Uuid,
    pub slug: String,                  // canonical: lowercase, alphanumeric + hyphens
    pub display_name: String,          // latest name used in narrative
    pub known_aliases: Vec<String>,    // previously seen names for this entity
    pub entity_type: EntityType,       // Player, Npc, Creature
    pub stats: StatBlock,
    pub mood: Mood,
    pub sensitivity: SensitivityMultipliers,
    pub last_interaction_turn: u64,
    pub audit: EntityAudit,
    pub correction: CorrectionMetadata,
}

pub enum EntityType {
    Player,
    Npc,
    Creature,
}
```

### 10.6 Relationship model

Stat blocks in MVP are **player-centric**. Each NPC/creature entity's stat block represents that entity's disposition *toward the player*.

- When the player interacts with an NPC, deltas apply to the NPC's stat block.
- NPCs do not track stats toward each other (hard non-goal §3.1).
- If a future version adds NPC↔NPC relationships, the model would shift to dyadic pairs. The current schema does not prevent this but does not optimize for it.

### 10.7 Mood model

Mood is a **system-derived label**, not an extraction output (see §13.6). Mood resolution is deterministic:

```rust
pub enum Mood {
    Friendly,    // trust + affection high
    Warm,        // affection dominant
    Trusting,    // trust dominant
    Anxious,     // fear high
    Hostile,     // hostility high
    Angry,       // anger high
    Afraid,      // fear very high
    Cold,        // all stats near baseline
    Conflicted,  // competing high stats
}
```

**Resolution rule (deterministic):**

1. For each stat (excluding health), compute a "dominance score" = stat value.
2. If any stat ≥ 8 (on 0–10 normalized scale), that stat's mood wins.
3. Priority when multiple stats ≥ 8: `fear > hostility > anger > affection > trust`.
4. If two non-priority stats are within 1 point of each other and both ≥ 8: `Mood::Conflicted`.
5. If no stat ≥ 8: `Mood::Cold`.

The user cannot directly set mood. Mood changes automatically when stats change. The user *can* set stats, which indirectly changes mood.

### 10.8 Sensitivity multipliers

Sensitivity multipliers modify how much proposed deltas affect each stat. They allow entities to be more or less reactive to specific relationship dynamics.

```rust
pub struct SensitivityMultipliers {
    pub trust: f32,       // default: 1.0, range: 0.0–2.0
    pub affection: f32,   // default: 1.0, range: 0.0–2.0
    pub anger: f32,       // default: 1.0, range: 0.0–2.0
    pub fear: f32,        // default: 1.0, range: 0.0–2.0
    pub hostility: f32,   // default: 1.0, range: 0.0–2.0
}
```

**Application order:**

```
final_delta = clamp(proposed_delta * sensitivity, -delta_cap, delta_cap)
stat_new    = clamp(stat_old + final_delta, 0, 255)
```

Sensitivity is user-settable per entity via manual correction commands. Default sensitivity for all new entities is `1.0` across all stats.

### 10.9 Manual correction state

```rust
pub struct CorrectionMetadata {
    pub last_manual_edit_at: Option<i64>,
    pub last_manual_edit_reason: Option<String>,
    pub lock_name: bool,
}
```

Locked names survive entity normalization during extraction but can be overwritten by explicit merge commands.

### 10.10 Cross-session identity preparation

Entity slugs are session-scoped by default. Cross-session import (post-MVP) will require a namespace prefix (e.g., `session_uuid::entity_slug`) to avoid collisions. The current schema does not include this prefix but must not make it impossible to add.

---

## 11. Persistence

### 11.1 Namespace registration

Addons register their table namespace at startup to prevent collisions:

```sql
CREATE TABLE IF NOT EXISTS addon_namespaces (
    namespace TEXT PRIMARY KEY,
    addon_name TEXT NOT NULL,
    addon_version TEXT NOT NULL,
    registered_at INTEGER NOT NULL
);
```

Anansi registers namespace `anansi` at startup. ozone+ refuses to create tables in a registered addon's namespace. Anansi refuses to start if its namespace is claimed by a different addon.

### 11.2 Live state tables

Retain the namespaced `anansi_*` table approach:

- `anansi_entities` — entity registry
- `anansi_events` — audit event trail
- `anansi_combat_state` — active combat state (if any)

### 11.3 Audit-first event trail

The event table explicitly supports audit inspection:

| Column | Type | Purpose |
|--------|------|---------|
| `event_id` | TEXT (UUID) | Unique event identifier |
| `event_type` | TEXT | Prefixed type (see §11.4) |
| `entity_id` | TEXT (UUID) | Entity involved |
| `turn` | INTEGER | Turn number |
| `message_id` | TEXT | Associated message for idempotency |
| `payload_json` | TEXT | Structured event payload |
| `created_at` | INTEGER | Unix timestamp |
| `confidence` | TEXT | high / medium / degraded |
| `used_fallback` | BOOLEAN | Whether fallback logic was applied |
| `degraded_mode` | BOOLEAN | Whether degraded mode halved deltas |
| `proposal_source` | TEXT | llm_extraction / engine_fallback / manual_correction / combat_resolution |

### 11.4 Event type prefixing

All Anansi event types use the `anansi.` prefix to coexist with ozone+ system events:

```
anansi.entity_created
anansi.entity_renamed
anansi.entity_merged
anansi.delta_applied
anansi.delta_discarded
anansi.extraction_completed
anansi.combat_round
anansi.combat_started
anansi.combat_ended
anansi.manual_correction
anansi.mood_changed
```

### 11.5 Cross-session import decision

Cross-session import remains **post-MVP optional**, not a launch-critical feature. The core design should preserve it, but implementation should come only after the single-session loop is stable.

Reason:
- it adds merge edge cases early
- it is not required to prove the core value
- it is much easier to add later than to unwind from MVP scope

---

## 12. `[GAME STATE]` Context Injection

### 12.1 Role

`[GAME STATE]` is a soft-context summary, never hard context.

### 12.2 Context layer integration

`[GAME STATE]` registers as a formal soft context layer in ozone+'s `ContextAssembler` rather than being a manually injected string:

```rust
// Registration in ContextSlotProvider::slot_metadata()
ContextSlotMetadata {
    addon_name: "anansi",
    priority: 50,         // between LorebookEntries (40) and RetrievedMemory (60)
    is_hard_context: false,
    max_tokens: 400,
    collapse_strategy: CollapseStrategy::TruncateTail,
}
```

This lets ozone+'s `ContextAssembler` handle budgeting, compression, and drop-first policies automatically. Anansi loses no control but gains predictability within the context pipeline.

### 12.3 Contribution timing

The `[GAME STATE]` block is provided during context assembly, before each generation:

1. ozone+ begins context assembly
2. `ContextAssembler` calls all registered `ContextSlotProvider`s
3. Anansi's provider reads the current ECS world snapshot
4. Provider returns the formatted `[GAME STATE]` block (or `None` if empty/over-budget)
5. `ContextAssembler` slots it according to priority and budget
6. Generation occurs with the assembled context
7. Post-generation, `PostGenerationHook` fires for extraction

### 12.4 Normative format

The `[GAME STATE]` block uses a compact, structured plaintext format that is both LLM-parseable and human-readable:

```
[GAME STATE]
Combat: inactive
Entities:
  Elara (npc) — Mood: warm | Trust:7 Aff:8 Ang:1 Fear:2 Host:0 | HP:10/10
  Goblin (creature) — Mood: hostile | Trust:0 Aff:0 Ang:3 Fear:1 Host:9 | HP:8/10
Recent:
  Turn 14: Elara trust +2 (kind words) [high confidence]
  Turn 13: Goblin hostility +3 (threatened) [medium confidence]
[/GAME STATE]
```

When combat is active:
```
[GAME STATE]
Combat: active | Round 2
  Elara (npc) HP: 8/10 — attacking
  Goblin (creature) HP: 3/10 — defending
Entities:
  Elara (npc) — Mood: determined | Trust:7 Aff:8 Ang:3 Fear:2 Host:2
Recent:
  Turn 15: Combat round 1 — Elara hit Goblin for 2 damage [combat resolution]
[/GAME STATE]
```

### 12.5 Safety rule

If context is tight, `[GAME STATE]` is dropped before narrative-critical hard context.

### 12.6 Revised content priorities

The block should prioritize:

1. active combat state
2. recently interacted entities
3. most recent applied changes
4. nothing else

### 12.7 Revised bounds

- max 8 entities in generation summary
- max 3 recent changes
- hard soft-cap target: 400 tokens estimated
- if over budget, remove oldest entities first
- if still over budget, remove recent changes
- if still over budget, omit the block entirely

### 12.8 Design warning

This block is useful only if it is both:
- small enough to avoid prompt pollution
- stable enough that the model can rely on it

MVP should therefore resist adding inventory, traits, conditions, or world flags into this block.

---

## 13. Extraction Pipeline

The extraction pipeline is the highest-risk component in Anansi. It is the interface between the LLM's narrative output and the deterministic state engine. This section specifies it in detail.

### 13.1 Trigger

Extraction runs after assistant message commit in a background task, triggered by the `PostGenerationHook` mechanism (§9.2). Extraction is **asynchronous** relative to the user's ability to continue typing, but its results must be validated by the engine before persisting (§9.6).

### 13.2 Eligibility

A message is eligible for extraction if:

- it is an assistant message (not user, not system)
- it has been committed (not a draft or cancelled generation)
- it has not already been processed (idempotency check, §13.8)
- it is not extension-authored combat notation or tool text

### 13.3 Conceptual split

Even if implemented as a single inference call at first, Anansi should treat extraction as **two logical jobs**:

1. **Entity discovery** — identify named entities in the text
2. **Relationship-change extraction** — identify stat deltas for discovered entities

This separation reduces future refactor pain. One can improve discovery later without destabilizing stat-change interpretation.

### 13.4 Extraction prompt template

The extraction call uses a structured prompt that instructs the model to produce JSON output. The template should be stored as a configurable file, not hardcoded:

```
You are a game-state extraction engine. Given the following roleplay narrative turn, identify:

1. Named entities mentioned (name, type: player/npc/creature)
2. Relationship changes between the player and any entity

Output JSON only. Do not narrate. Do not invent entities not mentioned in the text.

Schema:
{
  "entities": [
    { "name": "string", "type": "player|npc|creature" }
  ],
  "deltas": [
    {
      "entity_name": "string",
      "stat": "trust|affection|anger|fear|hostility",
      "delta": integer (-10 to +10),
      "reason": "brief phrase"
    }
  ]
}

Rules:
- Never propose health changes. Health is combat-only.
- Never propose absolute values. Only propose deltas (changes).
- If no relationship changes occurred, return empty deltas array.
- If unsure about a change, omit it rather than guessing.
- Entity names should match how they appear in the narrative text.

Narrative turn:
"""
{assistant_message_content}
"""

Current known entities:
{entity_registry_summary}
```

This template is a starting point. It will be iterated based on extraction quality metrics (§6.2).

### 13.5 Entity normalization rules

When extraction returns entity names, the normalization pipeline resolves them against the existing registry:

```
Normalization rules (MVP):
1. Lowercase the name
2. Strip leading articles ("the", "a", "an")
3. Collapse whitespace to single spaces
4. Trim leading/trailing whitespace
5. Match against slug first, then known_aliases (exact match)
6. If exact match found: update display_name if different, add previous display_name to aliases
7. If no match found: create new entity with slug = normalized name
8. If locked name (CorrectionMetadata.lock_name = true): skip normalization, use slug as-is
9. If ambiguous (multiple partial matches): mark as DiscardedAmbiguous, do not create or update
```

This is deterministic "exact normalization" — no fuzzy matching, no pronoun resolution — but it is *specified* exact normalization rather than implied.

### 13.6 Extraction outcomes

Each extraction cycle produces one of six outcomes:

| Outcome | Meaning |
|---------|---------|
| `Accepted` | Clean parse, all deltas valid |
| `AcceptedDegraded` | Valid parse with normalization or fallback applied |
| `DiscardedMalformed` | JSON parse failure or schema mismatch |
| `DiscardedAmbiguous` | Parseable but entity references unresolvable |
| `SkippedAlreadyProcessed` | Idempotency key match — this turn was already extracted |
| `SkippedLockContention` | ECS world or DB locked by another operation |

### 13.7 What extraction must not decide

Extraction must not decide:

- final mood labels (mood is system-derived, §10.7)
- combat outcomes
- HP changes
- absolute stat values (only deltas)
- whether combat begins automatically
- fuzzy identity merges
- user correction overrides

### 13.8 Idempotency mechanism

Each extraction cycle generates an idempotency key:

```
idempotency_key = hash(turn_number, active_message_id)
```

- If a key matches a previously processed extraction in the `anansi_events` table, the result is `SkippedAlreadyProcessed`.
- The `active_message_id` changes when the user swipes to a different candidate, so a regenerated turn gets a fresh extraction.
- The `turn_number` prevents cross-turn collisions.

### 13.9 Swipe/regenerate flow

When the user swipes to an alternate response candidate:

1. User swipes to alternate candidate (new `MessageId`)
2. ozone+ fires `BranchChanged` event (or equivalent)
3. Anansi marks all extractions from the deactivated message as `superseded` in the event trail
4. Anansi runs extraction on the newly active message
5. Idempotency key = `hash(turn_number, new_active_message_id)` — different from original
6. No automatic rollback of previously applied deltas from the old message
7. Manual correction available if stat drift occurs from the swipe

**Design rationale:** Automatic rollback of the old message's deltas would require reverse-delta computation, which is fragile when multiple stats were modified with fallback logic. The skip-not-rollback principle (§4.6) applies here. Users who notice drift can use correction commands.

### 13.10 Degraded mode

If extraction produces output that requires substantial repair (confidence < 0.5), apply conservative state impact:

- All proposed deltas are halved (rounded toward zero)
- Confidence is marked `degraded`
- Audit trail records that degraded mode was active

This is better than either accepting bad data at full strength or discarding potentially useful signal entirely.

### 13.11 Retry and failure handling

| Failure mode | Behavior |
|-------------|----------|
| LLM timeout or crash during extraction | Log error, emit `DiscardedMalformed`, narrative continues. Do **not** retry automatically — the next turn's extraction will capture any ongoing relationship dynamics. |
| JSON parse failure | Attempt one structural reparse (strip markdown fences, fix trailing commas). If still invalid: `DiscardedMalformed`. |
| Schema valid but semantically suspicious | Accept with degraded confidence. Quality gates (§13.12) may reclassify. |
| ECS world locked | `SkippedLockContention`. Retry once after 100ms. If still locked, skip. |

### 13.12 Quality gates

After extraction produces a result but before deltas are applied, the engine checks:

1. **Magnitude sanity:** Any delta exceeding `delta_cap` (default: 10) is clamped.
2. **Direction consistency:** Deltas are checked against the interaction-direction table (§14.1).
3. **Contradictory deltas:** If two deltas for the same entity and same stat appear in the same extraction (e.g., "+3 trust" and "−5 trust"), they are summed: net delta = −2.
4. **Entity cap:** If extraction proposes more than `max_entities` (config) new entities in a single turn, excess entities are discarded (oldest-named first).
5. **Self-reference check:** Deltas targeting the player entity from the player are invalid and discarded.

### 13.13 Worked example

**Narrative turn (assistant message):**

> Elara looks up from the map, her expression softening. "I didn't think anyone would come looking for me out here," she says quietly, a faint smile crossing her face. Behind her, the goblin scout watches from the shadows, its grip tightening on its blade.

**Extraction output (JSON):**

```json
{
  "entities": [
    { "name": "Elara", "type": "npc" },
    { "name": "goblin scout", "type": "creature" }
  ],
  "deltas": [
    { "entity_name": "Elara", "stat": "trust", "delta": 3, "reason": "grateful for being found" },
    { "entity_name": "Elara", "stat": "affection", "delta": 2, "reason": "softening expression, smile" },
    { "entity_name": "goblin scout", "stat": "hostility", "delta": 4, "reason": "grip tightening, watching from shadows" },
    { "entity_name": "goblin scout", "stat": "fear", "delta": 1, "reason": "hiding in shadows" }
  ]
}
```

**Normalization:**

1. "Elara" → lowercase "elara" → matches existing slug `elara` → update display_name (no change needed)
2. "goblin scout" → lowercase "goblin scout" → strip articles (none) → no match → create new entity with slug `goblin-scout`

**Validation:**

1. Elara trust +3: within delta_cap (10), sensitivity 1.0 → final delta +3. New trust: 4 + 3 = 7.
2. Elara affection +2: within delta_cap, sensitivity 1.0 → final delta +2. New affection: 3 + 2 = 5.
3. Goblin scout hostility +4: within delta_cap, sensitivity 1.0 → final delta +4. New hostility: 0 + 4 = 4.
4. Goblin scout fear +1: within delta_cap, sensitivity 1.0 → final delta +1. New fear: 0 + 1 = 1.
5. Health not touched. ✓
6. No absolute values. ✓
7. No combat triggering. ✓

**Result:** `Accepted` with `high` confidence. Four audit events recorded.

**Mood updates:**
- Elara: trust=7 (highest stat ≥ 8? No) → all below 8 → `Mood::Cold`. (Trust is 7, just below threshold.)
- Goblin scout: all below 8 → `Mood::Cold`.

---

## 14. Validation and Fallback Logic

### 14.1 Interaction-direction table

The direction table validates whether a proposed delta makes sense given who is interacting with whom:

| Proposer → Target | trust | affection | anger | fear | hostility | Notes |
|---|:---:|:---:|:---:|:---:|:---:|---|
| player → npc | ±allowed | ±allowed | ±allowed | ±allowed | ±allowed | Standard interaction |
| npc → player | ±allowed | ±allowed | ±allowed | ±allowed | ±allowed | NPC reacts to player |
| npc → npc | — | — | — | — | — | Non-goal (§3.1) |
| narrator → any | — | — | — | — | — | Narrator has no relationship |
| unknown → any | ±halved | ±halved | ±halved | ±halved | ±halved | Degraded confidence in direction |

When the extraction does not clearly indicate direction (e.g., passive voice, ambiguous subject), the delta is classified as `unknown → target` and halved.

### 14.2 Explicit audit markers

For each applied delta, the engine records:

| Marker | Type | Meaning |
|--------|------|---------|
| `proposal_source` | enum | `llm_extraction` / `engine_fallback` / `manual_correction` / `combat_resolution` |
| `direction_override` | bool | Whether direction validation overrode the proposed direction |
| `fallback_magnitude` | bool | Whether fallback magnitude was substituted for a missing/invalid delta |
| `degraded_halved` | bool | Whether degraded mode halved the delta |
| `sensitivity_applied` | bool | Whether sensitivity multiplier != 1.0 was applied |
| `clamped` | bool | Whether the final value was clamped to the 0–255 range |

### 14.3 Confidence scoring

Each extraction cycle emits one confidence score, computed deterministically:

```
confidence_score = 1.0
  - 0.1  per field that required name normalization (rename, alias match)
  - 0.2  per field that required fallback magnitude substitution
  - 0.3  per field that was ambiguous and resolved by heuristic
  - 0.4  per field that was completely fabricated by fallback

Confidence levels:
  high:     confidence_score ≥ 0.8
  medium:   0.5 ≤ confidence_score < 0.8
  degraded: confidence_score < 0.5
```

This replaces the subjective "low repair / heavy repair" language from v0.2 with a deterministic, auditable formula.

**Note:** Confidence does not block state changes. It makes them visible. A `degraded` extraction still applies (with halved deltas per §13.10), but the audit trail and TUI clearly mark the degradation.

### 14.4 Repair severity scale

When extraction output requires repair before validation, each repair operation carries a defined severity:

| Repair operation | Severity | Confidence penalty | Example |
|---|---|---|---|
| Type coercion | Trivial | −0.05 | `"7"` → `7` (string to int) |
| Range clamping | Minor | −0.05 | `260` → `255` (overflow), see §14.5 |
| Missing field substitution | Moderate | −0.10 | `null` → `base_magnitude` |
| Structural reparse | Moderate | −0.15 | `{"trust": {"value": 5}}` → `{"trust": 5}` |
| Partial extraction | Heavy | −0.20 | Extracted 2 of 3 expected fields |
| Complete fabrication | Severe | −0.40 | No parseable data, all fields use fallback |

### 14.5 Clamping formula

All stat mutations use the following clamping formula to prevent `u8` overflow:

```
S(t+1) = max(0, min(255, S(t) + max(-delta_cap, min(Δ, delta_cap))))
```

Where:
- `S(t)` is the current stat value
- `Δ` is the proposed delta (after sensitivity multiplier)
- `delta_cap` is the configured maximum absolute delta (default: 10)

This double-clamp ensures that no delta, however large, can cause arithmetic overflow.

---

## 15. Combat System

### 15.1 MVP combat scope

Keep combat intentionally small:

- no initiative
- no conditions
- no weapon tables
- no range logic beyond narrative labeling
- one deterministic round at a time

### 15.2 Combat state machine

Combat has three explicit states with defined transitions:

```
                    /combat start (manual)
                            │
                            ▼
┌──────────┐    ┌───────────────────────┐    ┌─────────────────┐
│   Idle   │───►│    CombatActive       │───►│ CombatResolved  │
│          │    │  (rounds in progress) │    │  (outcome known)│
└──────────┘    └───────────────────────┘    └────────┬────────┘
      ▲                    │                          │
      │                    │ /combat end (manual)     │ auto-transition
      │                    ▼                          │ after display
      │              ┌──────────┐                     │
      └──────────────┤  Idle    │◄────────────────────┘
                     └──────────┘
```

**Transition triggers:**

| Transition | Trigger | Notes |
|---|---|---|
| Idle → CombatActive | User issues `/combat start` command | Combat never starts automatically from extraction or stat thresholds |
| CombatActive → CombatActive | Tool call resolves a round | Rounds accumulate until user ends combat |
| CombatActive → CombatResolved | Entity reaches 0 HP | Automatic transition |
| CombatResolved → Idle | Result displayed, user acknowledges | Can also use `/combat end` |
| CombatActive → Idle | User issues `/combat end` | Manual abort |

**Critical rule:** High `hostility` does **not** trigger combat. Combat initiation is always a user action. This prevents the extraction pipeline from inadvertently starting combat by raising hostility.

### 15.3 Damage calculation

MVP uses the simplest deterministic model:

```
damage = max(1, attacker_hostility / 3)
```

- Minimum damage is always 1 (no zero-damage rounds)
- `hostility / 3` gives a 0–85 damage range from the 0–255 hostility range, which is too wide for MVP. In practice, hostility will be in the 0–10 range during MVP, yielding 1–3 damage.
- Defense is not modeled in MVP (post-MVP Phase 2).
- Damage applies directly to target's `health` stat.

### 15.4 HP resolution at zero

When an entity's health reaches 0:

1. Entity enters `defeated` state (a flag, not entity deletion)
2. Entity is excluded from further combat rounds
3. The `[GAME STATE]` block shows `HP: 0/max — defeated`
4. The LLM is expected to narrate the defeat in its next turn
5. Entity remains in the registry (it may be revived via manual correction)

Entity **deletion** is never automatic. A defeated entity is still trackable, inspectable, and correctable.

### 15.5 Worked combat example

> User: `/combat start`  
> Combat state: Idle → CombatActive  
> Participants: Elara (HP: 10/10, hostility: 2), Goblin (HP: 8/10, hostility: 9)

> LLM generates: tool call `anansi_combat_round` with `{"attacker": "goblin-scout", "target": "elara"}`  
> Resolution: damage = max(1, 9 / 3) = 3. Elara HP: 10 − 3 = 7.  
> Audit: `anansi.combat_round` event, `proposal_source: combat_resolution`, seed key logged.  
> `[GAME STATE]` updates: `Elara (npc) HP: 7/10`

> Next round: tool call `anansi_combat_round` with `{"attacker": "elara", "target": "goblin-scout"}`  
> Resolution: damage = max(1, 2 / 3) = 1. Goblin HP: 8 − 1 = 7.

### 15.6 Deterministic replay

Combat uses a seeded resolution key: `seed = hash(session_id, turn_number, round_number, message_id)`. If the user swipes and the same combat round is recalculated, the same seed produces the same result.

### 15.7 Tool support degradation

If tool-calling is unavailable:
- Anansi should not attempt fake deterministic combat behind the model's back
- The system simply runs without mechanical combat enforcement
- UI should show that combat tooling is unavailable, not silently pretend everything is normal

If tool-calling becomes available mid-session (backend upgrade, model switch):
- Combat can be started normally
- Previously unresolved narrative combat is not retroactively mechanized
- The capability probe runs on each generation cycle, not just at session start

### 15.8 Manual controls

MVP should include minimal manual commands:

- `/combat start` — transition to CombatActive
- `/combat end` — transition to Idle
- `/set-hp <entity> <value>` — override HP
- `/combat clear` — reset all combat state

These are safety valves, not a full GM toolset.

---

## 16. Error Taxonomy

Anansi defines its own error types that integrate with ozone+'s error framework.

### 16.1 Error enum

```rust
#[derive(Debug, thiserror::Error)]
pub enum AnansiError {
    // Extraction errors
    #[error("extraction failed: {reason}")]
    ExtractionFailed { reason: String, turn: u64 },

    #[error("extraction parse error: {detail}")]
    ExtractionParseError { detail: String, raw_output: String },

    // Validation errors
    #[error("validation rejected delta: {reason}")]
    ValidationRejected { entity_id: Uuid, stat: String, reason: String },

    // Combat errors
    #[error("combat resolution failed: {reason}")]
    CombatResolutionFailed { reason: String, round: u32 },

    #[error("combat not active")]
    CombatNotActive,

    #[error("combat entity not found: {entity_slug}")]
    CombatEntityNotFound { entity_slug: String },

    // State errors
    #[error("entity not found: {slug}")]
    EntityNotFound { slug: String },

    #[error("duplicate entity slug: {slug}")]
    DuplicateEntity { slug: String },

    #[error("state corruption detected: {detail}")]
    StateCorruption { detail: String },

    // Addon API errors
    #[error("addon API mismatch: expected {expected}, got {actual}")]
    AddonApiMismatch { expected: String, actual: String },

    #[error("namespace collision: {namespace}")]
    NamespaceCollision { namespace: String },

    // Bridge errors
    #[error("engine command channel closed")]
    ChannelClosed,

    #[error("ECS world lock timeout")]
    WorldLockTimeout,
}
```

### 16.2 Error severity and visibility

| Error | Severity | User visible? | Recovery |
|---|---|---|---|
| `ExtractionFailed` | Warning | Degraded marker in TUI | Narrative continues, manual correction available |
| `ExtractionParseError` | Warning | Degraded marker in TUI | Logged for template iteration |
| `ValidationRejected` | Info | Audit trail only | Delta discarded, other deltas still apply |
| `CombatResolutionFailed` | Warning | TUI shows combat error | Combat paused, user can retry or end |
| `CombatNotActive` | Info | Command feedback | User informed, no state change |
| `EntityNotFound` | Info | Command feedback | User can create entity |
| `DuplicateEntity` | Warning | TUI notification | User can merge |
| `StateCorruption` | Critical | TUI alert | Session export recommended, manual recovery |
| `AddonApiMismatch` | Critical | Startup error | Anansi refuses to load |
| `NamespaceCollision` | Critical | Startup error | Anansi refuses to load |
| `ChannelClosed` | Critical | TUI alert | Anansi enters read-only mode |
| `WorldLockTimeout` | Warning | Audit trail | Operation retried once, then skipped |

### 16.3 Integration with ozone+ error framework

Anansi errors that reach the user surface (TUI, command feedback) should be wrapped in the ozone+ error display system to maintain consistent UX. Errors that are internal (validation rejections, extraction parse failures) are logged via `tracing` and recorded in the audit trail but do not interrupt the user's workflow.

---

## 17. TUI Surfaces

### 17.1 Compact pane remains

The right-side pane stays narrow, collapsible, and read-only by default.

### 17.2 Missing piece: inspector

The old design had a good compact pane, but it needed a fuller inspection surface.

MVP should therefore include:

- **compact pane** for at-a-glance state
- **entity inspector view or command** for full registry and recent events

Without this, silent omissions and bounded summaries feel arbitrary.

### 17.3 Minimum inspector capabilities

The inspector should show:

- all tracked entities
- full six-stat block for selected entity
- current mood (derived, with explanation of which stat dominated)
- last few recent changes (configurable, default: 5)
- confidence/degraded markers per change
- whether fallback logic was used
- whether a value was manually corrected
- sensitivity multipliers if non-default

### 17.4 Manual correction commands

MVP should include slash or command-palette access for:

| Command | Effect |
|---------|--------|
| `/anansi create <name> <type>` | Create a new entity |
| `/anansi rename <entity> <new_name>` | Rename entity, update slug |
| `/anansi merge <entity1> <entity2>` | Merge duplicates, keep higher stats |
| `/anansi set <entity> <stat> <value>` | Override a stat value |
| `/anansi set-hp <entity> <value>` | Override HP specifically |
| `/anansi clear-change <entity> <event_id>` | Mark a recent change as invalid |
| `/anansi sensitivity <entity> <stat> <multiplier>` | Set sensitivity multiplier |
| `/combat start` | Begin combat |
| `/combat end` | End combat |
| `/combat clear` | Reset all combat state |

All correction commands are routed through ozone+'s engine command channel (§4.9) as `ApplyAddonStateDelta` commands. The engine validates, persists, and broadcasts `AddonStateApplied`. This preserves undo/redo, audit trails, and WAL transactions.

This is the single biggest trust improvement over the earlier draft.

---

## 18. Config System

Anansi keeps its separate layered config file.

### 18.1 Required MVP settings

```toml
[anansi]
enabled = true

[anansi.extraction]
model = "same"            # "same" = same as ozone+ main model
backend = "same"          # "same" = same as ozone+ main backend
enabled = true
template_path = "extraction_prompt.txt"  # path to extraction prompt template

[anansi.game]
base_magnitude = 3        # fallback delta when extraction fails (per stat expected to change)
delta_cap = 10            # max absolute delta per stat per extraction cycle
max_entities = 32         # hard cap on entity registry size

[anansi.stats]
enabled = ["health", "trust", "affection", "anger", "fear", "hostility"]
# Campaigns that only use combat or only social dynamics can disable irrelevant stats.
# Disabled stats are excluded from extraction prompts, validation, and [GAME STATE].

[anansi.context]
max_entities = 8          # max entities in [GAME STATE] block
max_recent_changes = 3    # max recent changes shown
max_tokens = 400          # soft token cap for context block

[anansi.audit]
show_confidence = true
show_fallback_markers = true

[anansi.tui]
pane_width = 28
pane_default_open = false
pane_toggle_key = "alt+g"
inspector_recent_changes = 5  # how many recent changes the inspector shows
```

### 18.2 Config semantics

| Setting | Meaning |
|---------|---------|
| `extraction.model = "same"` | Use the same model ozone+ is using for generation. Not a separate utility model. |
| `game.base_magnitude = 3` | The delta value used when the engine falls back to conservative estimation (extraction failed or was discarded). Applied per stat that was expected to change. Default: 3 (out of 0–255 stat range, ~1.2% shift). |
| `game.delta_cap = 10` | Maximum absolute delta applied to any single stat in one extraction cycle, regardless of what extraction proposed. Clamped after sensitivity multiplier. Default: 10 (~3.9% of stat range). |
| `stats.enabled` | Controls which stats participate in the full pipeline. Disabled stats are never extracted, validated, or shown in context. The six-stat struct remains fixed in code; the config controls visibility. |

### 18.3 Explicit non-MVP config

Do not add a huge config matrix for inventory, conditions, item rarity, world flags, or faction weights in the first release.

---

## 19. MVP / Post-MVP Separation Map

### 19.1 MVP (ship this first)

- fixed six-stat entity model with identity resolution
- deterministic validation and fallback with confidence scoring
- compact pane + inspector
- manual correction commands routed through engine
- bounded `[GAME STATE]` as registered context layer
- simple combat tool with deterministic replay and state machine
- event trail with confidence/fallback markers and prefixed event types
- error taxonomy with severity levels
- namespace-registered persistence

### 19.2 Post-MVP Phase 2

- initiative
- conditions / temporary status effects
- social skill checks separate from extraction
- export and richer import flows
- in-TUI settings
- richer audit browsing
- defense stat / damage model improvements

### 19.3 Post-MVP Phase 3

- inventory and equipment
- item modifiers / affixes / temporary item effects
- trait-driven combat bonuses
- entity templates beyond player/npc/creature
- optional stat-display compression for larger rosters
- fuzzy alias matching (Levenshtein distance ≤ 2, substring > 80%)

### 19.4 Post-MVP Phase 4

- world flags
- gate/trigger system
- group-chat attribution
- pronoun resolution
- NPC↔NPC relationships
- cross-session entity import/export

### 19.5 Inventory and modifiers guidance

When inventory arrives, it should **not** enter as a full loot framework first.

Preferred sequence:
1. temporary effects / modifier primitives
2. simple equipped-item slots
3. inventory ownership and transfer
4. only then broader itemization

This sequence preserves MVP discipline and uses the combat/effect model you already have instead of exploding the state graph immediately.

---

## 20. Revised Roadmap

### Phase-to-tier mapping

| Phase | Scope Tier | MVP/Post-MVP | Hard dependency |
|-------|-----------|-------------|-----------------|
| 1A | A | MVP | None |
| 1B | A | MVP | 1A |
| 1C | A | MVP | 1A |
| 1D | N/A (ozone-core) | MVP | None (cross-workspace) |
| 1E | B | MVP | 1D |
| 1F | B | MVP | 1A, 1D, 1E |
| 1G | A | MVP | 1A |
| 1H | C | MVP | 1B, 1D |
| 1I | A | MVP | 1A, 1G |
| 1J | All | MVP | All above |

### Cross-workspace dependency

Phase 1D modifies `ozone-core`, not the Anansi workspace. If ozone+ development is on a separate timeline, this phase should be coordinated and may need to be split:

1. Define addon traits in ozone-core (ozone+ side)
2. Implement anansi-bridge against those traits (Anansi side)

Phase 1D is a **hard blocker** for Phases 1E, 1F, and 1H. It should be prioritized first.

```
Phase dependency graph:

1A ──────────┬── 1B ──── 1H (combat)
             │
             ├── 1C (persistence)
             │
1D ──────────┼── 1E (context) ──── 1F (extraction)
(ozone-core) │
             ├── 1G (correction) ──── 1I (TUI)
             │
             └── 1J (integration hardening) ← depends on ALL above
```

### Phase 1A — Core types and ECS world

Define types, entity storage, stat block, mood model, sensitivity multipliers, entity normalization rules, event types, error taxonomy.

**Exit criterion:** Entity CRUD works: create, read, update, delete, merge, rename. Mood resolution produces correct labels for known stat inputs. Serialization round-trips cleanly. Unit tests for all entity operations pass.

### Phase 1B — Deterministic systems

Implement validation, fallback, mood resolution, combat math, clamping formula, interaction-direction table.

**Exit criterion:** Deterministic system tests pass: given identical inputs, validation and combat always produce identical outputs. Clamping never produces overflow. Direction table correctly halves unknown-direction deltas. Property-based tests for stat boundaries pass.

### Phase 1C — Persistence and audit events

Implement namespaced tables, namespace registration, state load/save, event trail with audit markers and prefixed event types.

**Exit criterion:** State and audit trail survive session close and reopen. Namespace registration prevents collisions. Event trail records all marker types (proposal_source, confidence, fallback flags). Query-based assertion: events for a given turn match expected outcomes.

### Phase 1D — Unified addon API in ozone-core

Add `ContextSlotProvider`, `PostGenerationHook`, `ToolProvider`, `PaneProvider` trait definitions, `AddonRegistry`, error propagation contracts, and execution ordering rules.

**Exit criterion:** ozone+ compiles with zero behavior change when no addons are loaded. A no-op test addon can register all four capabilities without errors. Hook execution ordering is verified by test.

### Phase 1E — Context contribution

Generate bounded `[GAME STATE]` summary. Register as `ContextSlotProvider` with priority 50. Implement normative format, budget overflow handling, and collapse strategy.

**Exit criterion:** `[GAME STATE]` block appears in assembled context when entities exist. When token budget is reduced to 200 tokens, the block is omitted. When the block is omitted, no game state changes are lost. Format matches §12.4 normative example.

### Phase 1F — Extraction pipeline

Implement post-commit extraction via `PostGenerationHook`, normalization rules, idempotency keys, confidence scoring, delta application via engine command channel, and swipe/regenerate deduplication.

**Exit criterion:** New entities and relationship changes appear correctly across normal RP turns without duplicate processing. Swipe to a new candidate triggers re-extraction. Swipe back to a processed candidate is skipped. Confidence scores are deterministic for identical inputs.

### Phase 1G — Manual correction surface

Implement slash/command-palette controls per §17.4 table. All corrections routed through engine command channel.

**Exit criterion:** A user can recover from extraction mistakes without database surgery. Each correction generates an audit event with `proposal_source: manual_correction`. Undo/redo of corrections works via ozone+'s standard undo mechanism.

### Phase 1H — Combat tool

Implement deterministic combat tool registration via `ToolProvider`, state machine (§15.2), damage calculation, HP resolution, replay keying, and persistence.

**Exit criterion:** Repeated same-turn combat calls replay cached results (deterministic seeding). HP=0 transitions entity to defeated state. Combat state machine transitions match §15.2 table. Tool unavailability is surfaced in TUI.

### Phase 1I — Pane + inspector

Implement compact pane plus full inspector surface per §17.3.

**Exit criterion:** Compact view is readable at configured width. Inspector shows all fields per §17.3 list. Manually corrected values are visually distinguished. Confidence markers are visible per-change.

### Phase 1J — Integration hardening

Live end-to-end pass: generation → extraction → validation → correction → combat → context injection → reopen.

**Exit criterion:** Complete session loop — at least 20 turns of mixed narrative, extraction, correction, and combat — without state corruption, duplicate entities from swipes, or lost audit events. All §6.2 metrics are being tracked (not necessarily at target, but instrumented).

---

## 21. Risk Register

| Risk | Why it matters | Mitigation |
|------|----------------|-----------|
| Extraction drift | Narrative understanding may not match user expectation | Confidence markers, manual correction, bounded deltas, worked example in docs |
| Prompt pollution | `[GAME STATE]` can crowd out better context | Strict token cap, registered context layer with priority, soft-context drop-first policy |
| Hidden overrides | Fallback logic can feel arbitrary | Audit markers, inspector, confidence formula is public |
| Tool support variance | Local backends differ on function calling | Explicit degraded combat mode, capability probe per cycle |
| Duplicate entity creation | Names in prose are messy | Normalization rules (§13.5), alias tracking, merge command, metrics tracking (§6.2) |
| Scope creep | Inventory and simulation can swamp MVP | Hard separation of Phase 1 vs later phases |
| Host/addon API drift | Addon surface instability hurts all future addons | Versioned unified addon API, stability rule (§9.7) |
| User distrust after bad parse | One wrong extraction can poison perceived quality | Visible audit trail + correction commands |
| Single-writer violation | Manual corrections bypassing engine corrupt state | All mutations routed through engine command channel (§4.9) |
| Resource contention | Extraction competes with generation for LLM access | Extraction runs in background after generation completes; yields to `HardwareResourceSemaphore` |
| Threading deadlock | Channel miscommunication between tokio and ECS thread | Bounded channels with timeout, `WorldLockTimeout` error and retry (§13.11) |
| Namespace collision | Future addons or ozone+ versions claim `anansi_*` tables | Namespace registration table (§11.1) |
| Context budget competition | `[GAME STATE]` displaces lorebook entries | Priority-based context layer system (§12.2), user-configurable priority |

---

## 22. Open Design Decisions to Preserve for Later

These are **not blockers** for MVP, but the design should leave room for them:

- optional pluggable stat schema in a future major version
- effect system reusable by inventory and conditions
- richer combat action taxonomy (defense, initiative, conditions)
- cross-session entity import/export once the single-session loop is proven
- broader addon interoperability inside ozone+
- NPC↔NPC relationship model (dyadic pairs)
- fuzzy alias merging with user confirmation
- configurable mood resolution rules
- extraction model specialization (utility model separate from main model)

---

## 23. Implementation Guidance

### 23.1 What to optimize for first

- trustworthiness
- inspectability
- deterministic boundaries
- recovery from mistakes
- low prompt overhead

### 23.2 What not to optimize for first

- content richness
- number of mechanics
- simulation breadth
- deep item systems
- automatic "smart" inference

### 23.3 Practical rule

If a proposed feature adds more state categories than it adds recovery and clarity, it probably belongs after MVP.

### 23.4 Testing strategy

| Test type | What it covers | When to run |
|-----------|---------------|-------------|
| Unit tests | Entity CRUD, stat clamping, mood resolution, normalization rules, direction table | Every commit |
| Property-based tests | Stat overflow prevention, idempotency, deterministic replay | Every commit |
| Integration tests | Extraction → validation → persistence round-trip, swipe deduplication | Pre-merge |
| End-to-end tests | Full session loop: generation, extraction, correction, combat, reopen | Pre-release |
| Metric collection | §6.2 success metrics, extraction quality tracking | Post-launch |

---

## 24. Closing Direction

Anansi should ship as a **trustworthy mechanical layer**, not an overgrown speculative simulation system.

The right first release is the one where:

- the player can keep roleplaying normally
- the engine quietly tracks a few meaningful numbers
- combat resolves predictably when enabled
- errors are survivable
- state is legible
- corrections are easy
- every mutation is auditable

That gives you a real foundation for inventory, item modifiers, and broader world logic later.

**Build the bounded loop first. Expand only after the loop earns trust.**

---

*Anansi v0.3 final design document. Incorporates findings from six-model consolidated review of v0.2.*

# Anansi v0.2 Design — Consolidated Analysis Report

**Input:** 6 independent AI model reviews of the Anansi v0.2 design document  
**Models analyzed:** GLM 5.1 · MiMo-V2-Pro (Xiaomi) · MiniMax M2.7 · Trinity Large Thinking · Gemini 3.1 Pro Preview · Qwen3.6 Plus

---

## 1. Cross-Model Consensus

These findings were independently raised by all or nearly all models. When six separate analyses converge on the same conclusion, it carries high confidence.

### Universal Agreement (6/6 models)

- **"The LLM proposes, the engine decides" is the right core principle.** Every model identified this as the load-bearing design sentence. It establishes authority boundaries, transparency contracts, and recovery guarantees in one statement.

- **MVP discipline is genuine and well-enforced.** Hard non-goals, deferred-not-rejected lists, tiered scope with degradation rules, and success defined by trustworthiness rather than feature count — all models praised this as the document's strongest quality.

- **Product fit within the Ozone family is correct.** The dependent-build positioning, unidirectional dependency chain (`ozone-core → ozone+ → anansi`), and "Anansi-agnostic" host clarification earned unanimous approval.

- **The extraction pipeline is the most critical system and the most under-specified.** All six models flagged §13 as the highest-risk gap. The pipeline is the interface between narrative and deterministic state, yet it lacks prompt templates, idempotency key definitions, quality gates, retry behavior, and worked examples.

- **The six-stat fixed schema is a sound MVP choice with acknowledged long-term limitations.** All models agreed the bounded stat block makes validation, testing, and context injection tractable. All also noted it will need extensibility eventually.

- **Manual correction commands are essential for trust.** The v0.2 addition of inspector view and correction surface was recognized by every model as the single biggest trust improvement over earlier drafts.

- **Combat degradation handling — refusing to fake it — is the right call.** Explicitly surfacing combat unavailability rather than silently degrading was universally praised.

- **Skip-not-rollback for swipe/regenerate is pragmatic.** Given ozone+'s swipe architecture, every model agreed that preventing duplicate processing beats attempting automatic state reversal.

### Strong Agreement (5–6 models)

- **The addon surface needs trait-level specification** (GLM, MiniMax, Qwen, MiMo, Gemini). Four capabilities are named but none have method signatures, lifecycle contracts, or error propagation rules.

- **Entity identity resolution is a design-level gap** (GLM, MiniMax, Qwen, MiMo, Trinity). LLMs will produce name variants ("Elara," "Elara the Mage," "the mage") and exact-string matching with manual merge is insufficient even for MVP.

- **The `[GAME STATE]` block needs structural specification and context layer integration** (GLM, MiniMax, Qwen, Gemini, MiMo). The injection mechanism, priority relative to other soft context, and the actual format string are all undefined.

- **The mood model is referenced throughout but never defined** (GLM, MiniMax, MiMo). It appears in the entity model, phase roadmap, and extraction constraints, but no type definition, resolution rule, or ownership model exists.

- **The document would benefit from visual dependency diagrams** (MiniMax, MiMo, Qwen, Gemini, Trinity). The extraction → validation → audit → context → TUI chain is implicit and requires cross-referencing across many sections.

---

## 2. Model-Specific Strengths and Weaknesses

### GLM 5.1 — The Specification Engineer

**What it did best:**
- Most detailed and actionable response overall. Every weakness came with concrete Rust code, struct definitions, or rule specifications.
- Proposed a deterministic confidence scoring formula (numeric weights per repair type) to replace subjective labels.
- Provided a 6-step normalization rule set and an alias model for entity identity resolution.
- Reconstructed the missing interaction-direction table from context clues.
- Graded scorecard (B+ overall) with per-dimension breakdowns gives the design author a clear priority map.

**Where it was weaker:**
- Some recommendations are implementation-level detail that may over-constrain the design space (e.g., the mood enum with specific threshold rules).
- The 10 priority fixes are well-ordered but the report's length (~4,500 words) makes the signal hard to extract quickly.

### MiMo-V2-Pro (Xiaomi) — The Quick Scanner

**What it did best:**
- Cleanest visual hierarchy — product tables, collapsible detail sections, and numbered priorities make it the most scannable.
- Identified the "context contribution timing" gap: when exactly in the generation pipeline does Anansi inject `[GAME STATE]`?
- Good document reordering suggestions (move Product Fit earlier, merge Principles with Scope).

**Where it was weaker:**
- Shortest and least detailed response. Several areas get only a sentence where other models provide paragraphs.
- Some combat suggestions (stances, status effects, environmental modifiers) actively contradict the document's stated MVP minimalism.
- Performance concerns are raised but not substantiated.

### MiniMax M2.7 — The Structural Analyst

**What it did best:**
- Deepest structural analysis of the document itself. Traced implicit dependency chains across sections and identified where the reader must cross-reference without guidance.
- Best ambiguity identification: 6 critical ambiguities, each genuinely important. The mood ownership question ("who or what provides the mood label?") and the stat block relational model question ("are stats always player-centric?") are unique and incisive.
- Most thorough roadmap exit criteria evaluation — showed exactly which phases have vague vs. concrete test predicates.
- Uniquely identified the missing Anansi-specific error taxonomy.

**Where it was weaker:**
- More analytical than prescriptive — identifies gaps clearly but rarely provides code-level solutions.
- No overall score or priority ordering, making it harder to know where to start.

### Trinity Large Thinking — The Product Manager

**What it did best:**
- Most balanced executive-summary style. A non-technical stakeholder could read this and understand the state of the design.
- Only model to call for measurable success metrics (user retention, manual correction frequency, error rates).
- Only model to call for an explicit testing strategy.
- Identified document length and redundancy as a weakness — something no other model raised.
- Suggested performance targets (context assembly < 500ms, TUI frame time < 33ms).

**Where it was weaker:**
- Least technical depth. Many suggestions are directionally correct but generic ("add testing," "add success metrics") without specifying what to test or measure.
- The suggestion to make the stat block configurable via config somewhat undermines the MVP philosophy the same review praises.

### Gemini 3.1 Pro Preview — The Integration Specialist

**What it did best:**
- Highest signal-to-word ratio. Concise but dense, with every paragraph carrying unique insight.
- Unique mathematical formalization: the explicit clamping formula for u8 overflow prevention ($S_{t+1} = \max(0, \min(255, S_t + \max(-\delta_{cap}, \min(\Delta, \delta_{cap}))))$).
- Unique resource contention insight: does Anansi's background extraction yield to ozone+'s `HardwareResourceSemaphore`?
- Unique threading boundary question: does the bevy_ecs World live in `Arc<RwLock<World>>` shared across ozone-engine threads, or does anansi-bridge communicate via `mpsc` channels?
- Suggested cross-referencing Anansi phases with ozone+ phase dependencies.

**Where it was weaker:**
- Shortest analysis. Covers only integration and edge cases, leaving large areas (entity model, mood, TUI, roadmap) untouched.
- Only 2 ambiguities identified vs. 5–6 by other models.

### Qwen3.6 Plus — The Bridge Architect

**What it did best:**
- Best technical integration mapping between ozone+ and Anansi (event bus, persistence, TUI registry, inference gateway).
- Uniquely identified the single-writer violation risk: manual correction commands could bypass `ConversationEngine` unless routed through its command channel.
- Most actionable alongside GLM — provided `ContextLayerKind` enum, `Command` enum extension, and `ToolProvider` trait with full lifecycle hooks.
- Practical alias fallback (Levenshtein distance ≤ 2, substring match > 80%) that stays deterministic.
- Elegant stat schema extensibility hook: `[anansi.stats] enabled = [...]` preserves MVP simplicity while reducing prompt waste.

**Where it was weaker:**
- Some code suggestions are speculative, depending on ozone+ internals not visible in the design documents.
- The impact/root cause table format sometimes conflates architectural issues with implementation concerns.

---

## 3. Improvement Suggestions with Attribution

These are specific, actionable improvements extracted from the reviews, grouped by theme and credited to originating models.

### 3.1 Extraction Pipeline (All 6 models)

| Improvement | Primary credit | Supporting models |
|---|---|---|
| Add extraction prompt template or reference to template file | MiniMax, GLM | All |
| Define idempotency key mechanism (turn_number + message_id) | GLM, Gemini | MiniMax, Qwen |
| Add worked example: narrative → extraction → normalization → validation → delta applied | MiniMax, MiMo | Trinity, GLM |
| Define retry/failure handling for mid-turn extraction crashes | MiniMax | GLM |
| Specify quality gates for semantically plausible but wrong extractions | MiniMax | — |
| Add entity normalization rules (lowercase, strip articles, alias matching) | GLM | Qwen |
| Define the "repair" severity scale with numeric confidence penalties | GLM | — |

### 3.2 Addon Surface Contract (5 models)

| Improvement | Primary credit | Supporting models |
|---|---|---|
| Add full Rust trait signatures for all 4 capabilities | GLM (PostGenerationHook), Qwen (ToolProvider) | MiniMax, MiMo |
| Define execution ordering when multiple addons register the same hook | GLM | — |
| Define error propagation contract (does addon failure abort generation?) | GLM | Qwen |
| Add lifecycle hooks: register, handle, deregister | Qwen | GLM |
| Define threading boundary (shared ECS world vs. channel isolation) | Gemini | Qwen |

### 3.3 `[GAME STATE]` Context Integration (5 models)

| Improvement | Primary credit | Supporting models |
|---|---|---|
| Define actual format string with normative example | GLM | MiMo |
| Register as formal `ContextLayerKind` in ozone+ assembler | Qwen | GLM, MiniMax |
| Specify priority relative to lorebook, retrieved memory, etc. | GLM, Qwen | Gemini |
| Define context contribution timing in the generation pipeline | MiMo | MiniMax |
| Add token budget competition analysis vs. other soft layers | Gemini | GLM |

### 3.4 Combat System (5 models)

| Improvement | Primary credit | Supporting models |
|---|---|---|
| Define explicit combat state machine (Idle → Active → Resolved) | MiniMax | GLM |
| Add at least one worked damage calculation example | MiniMax | Trinity |
| Define HP=0 resolution (defeated state, not entity deletion) | MiniMax | — |
| Specify what happens when tool-calling becomes available mid-session | Qwen | — |
| Clarify hostility stat's role: relationship indicator, NOT combat trigger | GLM | MiniMax |

### 3.5 Data Model & Entity Resolution (5 models)

| Improvement | Primary credit | Supporting models |
|---|---|---|
| Add `known_aliases: Vec<String>` to Entity struct | GLM | Qwen |
| Define whether stat blocks are player-centric or support all dyads | MiniMax | — |
| Add lightweight string-distance fallback for near-miss matching | Qwen | GLM |
| Define mood type (enum, string, or derived from stat thresholds) | GLM, MiniMax | MiMo |
| Define sensitivity multiplier struct, range, and application formula | GLM | — |

### 3.6 Roadmap & Documentation (4 models)

| Improvement | Primary credit | Supporting models |
|---|---|---|
| Map roadmap phases to scope tiers and MVP success criteria | GLM | MiniMax, Trinity |
| Replace vague exit criteria with concrete test predicates | MiniMax | GLM |
| Add cross-phase dependency graph (especially Phase 1D as hard blocker) | Qwen, Gemini | GLM |
| Add forward-reference diagram early in document | MiniMax | MiMo |

### 3.7 ozone+ Integration (3 models)

| Improvement | Primary credit | Supporting models |
|---|---|---|
| Route all manual corrections through ConversationEngine command channel | Qwen | — |
| Register addon namespace ownership to prevent table collisions | GLM | — |
| Prefix Anansi event types in shared event table | GLM | Qwen |
| Document HardwareResourceSemaphore interaction for background extraction | Gemini | Qwen |
| Define swipe/regenerate end-to-end flow with state before/after | GLM, MiniMax | Qwen |

---

## 4. Top Novel Suggestions

These are ideas that appeared in only one or two models and represent genuinely new angles that complement the consensus.

### 4.1 Single-Writer Violation Risk — *Qwen3.6 Plus*

Manual stat corrections, if applied directly to the ECS world, bypass `ConversationEngine`'s single-writer guarantee. This breaks transactional integrity, undo/redo, and audit trails. **Fix:** Route all corrections as `ApplyAddonStateDelta` commands through the engine's existing command channel. This is architecturally simple but critical — no other model noticed it.

### 4.2 Threading Boundary Definition — *Gemini 3.1 Pro Preview*

Does Anansi's `bevy_ecs` World live in `Arc<RwLock<World>>` shared across ozone-engine threads, or does `anansi-bridge` communicate with an isolated Anansi thread via `mpsc` channels? This is a foundational concurrency decision that affects every interaction pattern. The design document is silent on it.

### 4.3 Mathematical Clamping Guard — *Gemini 3.1 Pro Preview*

The explicit overflow-prevention formula for `u8` stats ensures no delta, however large, can cause arithmetic overflow:

$$S_{t+1} = \max(0, \min(255, S_t + \max(-\delta_{cap}, \min(\Delta, \delta_{cap}))))$$

This should be in the spec, not left to implementation discovery.

### 4.4 Deterministic Confidence Scoring — *GLM 5.1*

Replace subjective repair labels ("low," "heavy") with a numeric scoring rule: start at 1.0, subtract per repair type (−0.1 for normalization, −0.2 for fallback substitution, −0.3 for heuristic resolution, −0.4 for fabrication). High ≥ 0.8, Medium ≥ 0.5, Degraded < 0.5. This makes confidence deterministic and auditable.

### 4.5 Namespace Collision Prevention — *GLM 5.1*

An `addon_namespaces` registration table in ozone-core prevents future addons (or ozone+ itself) from accidentally claiming Anansi's table names. Simple, forward-looking, zero MVP cost.

### 4.6 Measurable Success Metrics — *Trinity Large Thinking*

The only model to look beyond technical exit criteria and ask: "How will you know the MVP is successful?" Suggested metrics: manual correction frequency (lower = better extraction), error rates, user retention, and qualitative trustworthiness feedback. Technical correctness and product success are different things.

### 4.7 Stat Schema Extensibility via Config — *Qwen3.6 Plus*

```toml
[anansi.stats]
enabled = ["health", "trust", "hostility"]
```

Campaigns that only care about combat or only about social dynamics can disable irrelevant stats, reducing prompt waste in the `[GAME STATE]` block without adding schema complexity. The six-stat block stays fixed; the config controls visibility.

### 4.8 Cross-Phase Dependency Graph — *Qwen3.6 Plus, Gemini 3.1 Pro Preview*

Anansi's Phase 1D (unified addon API in ozone-core) is a hard blocker for Phases 1E+ and modifies a different workspace. This cross-workspace dependency should be explicitly mapped, with fallback plans if ozone-core changes are on a different timeline.

---

## 5. Synthesis: What It All Means

### The Architecture Is Sound. The Specification Isn't Complete.

All six models converge on the same fundamental verdict: Anansi v0.2's architecture is well-designed, its scope discipline is genuine, and its core promise — deterministic engine, transparent mutations, user authority — is the right foundation for an LLM-integrated game layer. No model found architectural flaws. Every identified weakness is a *specification gap*, not a structural defect.

### The Three Critical Gaps

If I collapse all 6 analyses into the three things that matter most before implementation begins:

1. **The extraction pipeline is under-specified at exactly the point where it most needs to be rigorous.** It's the highest-risk component (the LLM↔engine bridge), yet it has the least specification. Six models independently reached this conclusion. The pipeline needs: a prompt template, idempotency keys, normalization rules, quality gates, retry behavior, and at least one end-to-end worked example.

2. **The ozone+ integration contracts are conceptual, not technical.** The addon surface, context layer integration, manual correction routing, and threading model are all described in prose but lack the trait signatures, lifecycle hooks, and concurrency decisions that an implementer needs. Qwen's single-writer observation and Gemini's threading boundary question both highlight that getting these contracts wrong has cascading consequences.

3. **The entity identity problem is harder than the document admits.** "Exact normalization + manual merge" is a recovery strategy, not a prevention strategy. GLM's normalization rules and Qwen's Levenshtein fallback show that even deterministic approaches can do better without adding fuzzy AI inference.

### The Hidden Theme: Trust Is the Product

Reading across all six reviews, the word "trust" appears more than any other design concept — not trust as an entity stat, but trust as the user's relationship with the system. The extraction outcome taxonomy, the audit trail, the confidence markers, the manual correction surface, the refusal to fake combat — these are all trust mechanisms. The models that went deepest (GLM, MiniMax, Qwen) consistently praised these mechanisms and consistently flagged gaps that would *erode* trust: undefined mood changes, opaque extraction, invisible context competition, correction commands that bypass auditing.

The implication is clear: **Anansi's value proposition isn't "RPG mechanics." It's "RPG mechanics you can see, understand, and fix."** Every specification gap that makes the engine less inspectable or less predictable directly undermines the product's reason to exist.

### Recommended Reading Order for the Design Author

The six models serve different purposes:

1. **Start with MiniMax M2.7** for the structural view — it identifies the gaps most clearly.
2. **Read GLM 5.1** for the fixes — it provides the most concrete specifications to fill those gaps.
3. **Read Qwen3.6 Plus** for the integration architecture — it maps the actual ozone+ bridge points.
4. **Read Gemini 3.1 Pro Preview** for the edge cases — it catches mathematical and concurrency issues others missed.
5. **Read Trinity Large Thinking** for the product perspective — it asks "is this enough to succeed?" not just "is this correct?"
6. **Skim MiMo-V2-Pro** for the quick validation — it confirms the consensus but adds less unique signal.

### Final Verdict

Anansi v0.2 is **implementation-ready after one focused revision pass** that fills the specification gaps identified above. The architecture doesn't need rethinking. The scope doesn't need expanding. The design just needs to be written down completely — the same quality of precision that went into the non-goals section (which every model praised) needs to be applied to the extraction pipeline, addon contracts, and entity resolution system.

The design author should have high confidence that the structural decisions are correct. The work remaining is specification, not redesign.

---

*Report generated using the response-consolidation methodology. All attributions are to the model that first or most substantively raised each point.*


# Ozone Revised Design Document
## Terminal-Native Roleplay Frontend for Local LLMs
### Version 0.2 (Revised Architecture Draft)

**Status:** Pre-development redesign  
**Primary Language:** Rust  
**License:** MIT (proposed)

---

## Table of Contents

1. Executive Summary
2. Product Thesis
3. Core Design Principles
4. Revised Product Scope
5. Major Issues in the Original Design
6. Revised System Architecture
7. Ownership Boundaries
8. Runtime Pipelines
9. Data Model
10. Context Assembly System
11. Memory System
12. Group Chat Architecture
13. Thinking and Reasoning Block Policy
14. Backend Strategy
15. TUI / UX Design
16. Reliability, Debugging, and Transparency
17. Performance Strategy
18. Revised Roadmap
19. Technical Risks
20. Recommendations for First Implementation
21. Closing Direction

---

## 1. Executive Summary

Ozone should remain a terminal-native roleplay frontend for local LLMs, but its revised design should emphasize **deterministic foundations first** and **assistive intelligence second**.

The original design contains a strong product thesis:
- local-first
- low-overhead
- privacy-preserving
- roleplay-focused
- transparent and inspectable

Those qualities should remain unchanged.

What should change is the implementation philosophy.

The original design proposes a large number of automated interpretation features very early:
- automated speaker selection
- automatic memory scoring
- keyword anchoring
- lore gap detection
- tone classification
- narrator firing logic
- thinking summaries
- fine-tuned utility model workflows

Individually these ideas are good. Collectively they create a risk: too much hidden intelligence too early. That makes the system harder to test, harder to debug, and harder for users to form a stable mental model of.

The revised design therefore changes the center of gravity:

**Ozone should be a deterministic conversation engine with optional assistive intelligence.**

That means:
- canonical conversation state must be stable and explainable
- context assembly must be inspectable
- memory must be derived from, not replace, the transcript
- model-driven helpers should produce proposals or enrichments, not silently rewrite the truth
- sophisticated adaptive behavior should arrive only after the core loop is proven reliable

---

## 2. Product Thesis

Ozone exists to provide a serious roleplay frontend for local models without the overhead of a browser-heavy stack.

The main differentiators remain:

- Terminal-native operation
- Excellent performance on constrained hardware
- Deep roleplay support rather than general chat tooling
- Transparency around memory and context
- Strong support for local and private workflows
- Smooth operation over SSH, tmux, headless boxes, and low-resource desktops

This revised design keeps the original thesis intact but sharpens the product definition:

**Ozone is not trying to be “the smartest orchestrator.”  
It is trying to be the most reliable, inspectable, low-overhead local RP frontend.**

---

## 3. Core Design Principles

### 3.1 Deterministic core first
The system should produce a correct, understandable result even if all utility intelligence is disabled.

### 3.2 Assistive intelligence, not silent authority
Utility-model outputs should enrich or suggest. They should not become invisible sources of truth.

### 3.3 Canonical transcript is sacred
The message history is the source of truth. Summaries, embeddings, scores, and retrieval artifacts are derived layers.

### 3.4 Transparency is a feature
Users should be able to inspect:
- what entered the context
- what was omitted
- what memories were retrieved
- why a speaker was selected
- what summaries exist
- whether any background jobs are stale or degraded

### 3.5 Graceful degradation
If embeddings fail, utility model is offline, or retrieval is stale, Ozone must still function as a strong RP frontend.

### 3.6 Immersion remains primary
Debuggability is important, but the default user experience should not bury the story under diagnostics.

---

## 4. Revised Product Scope

To reduce risk, Ozone should explicitly separate functionality into three tiers.

### Tier A: Deterministic Core
This is the minimum product foundation.
- session storage
- message persistence
- branches
- swipes
- TUI chat loop
- manual persona support
- manual lorebooks
- author's note
- token budgeting
- sliding context window
- pinned memory
- session import/export
- context inspector

### Tier B: Assistive Automation
These features are useful, but should remain optional or clearly surfaced.
- retrieval suggestions
- automatic importance scoring suggestions
- speaker selection suggestions
- summary generation
- keyword extraction
- context recommendations
- one-line thinking summaries

### Tier C: Adaptive Intelligence
These features are higher-risk and should not be part of the first stable architecture promise.
- fine-tuned utility model as a dependency
- lore gap detection
- auto narrator triggering
- tone classification pipelines
- cross-session learning / flywheel as core behavior
- complex per-character hidden retrieval scopes
- genre-specific utility adapters

### Scope Rule
The first serious implementation should ship mostly Tier A, with a narrow set of Tier B features layered on top.

---

## 5. Major Issues in the Original Design

### 5.1 Too much automation too early
The original document front-loads several intelligent helpers. This creates a system that is elegant in theory but risky in real-world debugging.

### 5.2 State ownership is underspecified
Background tasks appear able to influence future context indirectly, but there is no strict contract describing who is allowed to commit state that affects generation.

### 5.3 Message record is overloaded
The original message model mixes:
- canonical transcript data
- retrieval metadata
- generation artifacts
- swipe relationships
- UI state

This will create schema churn and maintenance difficulty.

### 5.4 Compression is described too close to canonical storage
Old messages should not be rewritten into degraded storage forms as if the transcript itself has changed. Compression should be a retrieval and assembly concern, not a truth mutation.

### 5.5 Group chat complexity may arrive too soon
Per-character context isolation and private memories are powerful, but they multiply complexity before the product's base interaction model is proven.

### 5.6 Fine-tuning is on the critical path too early
A custom utility model is interesting, but the product should not depend on it before prompt-based and rule-based strategies have been validated.

---

## 6. Revised System Architecture

The revised architecture is organized around ownership and pipelines rather than just modules.

```text
┌──────────────────────────────────────────────────────────────────┐
│                           ozone binary                           │
├──────────────────────────────────────────────────────────────────┤
│                                                                  │
│  UI State Layer                                                  │
│  - chat view                                                     │
│  - inspectors                                                    │
│  - command palette                                               │
│  - memory browser                                                │
│  - branch viewer                                                 │
│                                                                  │
│  Conversation Engine                                             │
│  - sessions                                                      │
│  - messages                                                      │
│  - branches                                                      │
│  - swipes                                                        │
│  - canonical transcript                                          │
│                                                                  │
│  Context Assembler                                               │
│  - budget calculation                                            │
│  - layer selection                                               │
│  - context plan                                                  │
│  - truncation / omission report                                  │
│                                                                  │
│  Memory Engine                                                   │
│  - embeddings                                                    │
│  - summaries                                                     │
│  - retrieval                                                     │
│  - memory artifacts                                              │
│                                                                  │
│  Inference Gateway                                               │
│  - chat completion capability                                    │
│  - embedding capability                                          │
│  - tokenization capability                                       │
│  - grammar / constrained output capability                       │
│                                                                  │
│  Task Orchestrator                                               │
│  - job queues                                                    │
│  - deadlines                                                     │
│  - retries                                                       │
│  - cancellation                                                  │
│  - backpressure                                                  │
│                                                                  │
└──────────────────────────────────────────────────────────────────┘
```

### Architectural Rule
**Only the Conversation Engine and Context Assembler may commit active state that affects the next generation.**

All other systems may:
- generate artifacts
- compute proposals
- provide cached derivations
- return suggestions
- update derived indexes

They may not silently mutate active prompt state.

---

## 7. Ownership Boundaries

### 7.1 Conversation Engine
Owns canonical truth:
- sessions
- messages
- branches
- swipe groups
- bookmarks
- user edits
- current active branch

This engine must be usable without any retrieval, embeddings, or utility model.

### 7.2 Context Assembler
Owns prompt construction.
It decides:
- what goes in
- what stays out
- how budgets are enforced
- what was truncated
- what context plan was used this turn

No other system should directly inject content into the prompt.

### 7.3 Memory Engine
Owns derived memory artifacts:
- embeddings
- retrieval units
- summaries
- importance proposals
- memory indexes

It does not own the transcript.

### 7.4 Inference Gateway
Owns normalized access to inference backends.

### 7.5 Task Orchestrator
Owns execution policy:
- priority classes
- rate limits
- retries
- job visibility
- stale state warnings

### 7.6 UI State Layer
Owns rendering and inspection:
- focus
- panes
- command palette
- display mode
- collapse / expand state
- diagnostics visibility

---

## 8. Runtime Pipelines

### 8.1 Foreground Chat Pipeline
1. User submits input
2. Conversation Engine appends canonical user message
3. Context Assembler builds `ContextPlan`
4. Inference Gateway streams main-model completion
5. Conversation Engine stores assistant response
6. UI renders result
7. Background jobs are scheduled

This path must remain reliable even with all automation disabled.

### 8.2 Background Derivation Pipeline
After assistant message commit:
- token count finalization
- embedding job
- summary generation job
- importance proposal job
- retrieval index update
- optional thinking summary generation

These jobs create artifacts. They do not alter the already-committed message.

### 8.3 Retrieval Pipeline
Before generation:
- Context Assembler requests candidate memory units
- Memory Engine returns ranked candidates with provenance and scores
- Context Assembler decides what enters prompt based on budget and policy
- omitted candidates remain visible for inspection

### 8.4 Speaker Selection Pipeline
When group chat is enabled:
- deterministic rules are evaluated first
- if no rule resolves speaker, a suggestion task may run
- suggestion is surfaced as proposal with score and reason class
- Context Assembler / Conversation Engine commits final speaker choice

---

## 9. Data Model

The revised data model separates canonical data from derived artifacts.

### 9.1 Canonical Message

```rust
struct Message {
    id: MessageId,
    session_id: SessionId,
    branch_id: BranchId,
    parent_id: Option<MessageId>,
    author_id: AuthorId,
    role: Role,
    content: String,
    created_at: DateTime<Utc>,
}
```

This is the truth record.

### 9.2 Generation Metadata

```rust
struct GenerationRecord {
    id: GenerationId,
    message_id: MessageId,
    backend_id: String,
    sampler_preset: String,
    stop_reason: StopReason,
    token_count: Option<usize>,
    latency_ms: Option<u64>,
}
```

### 9.3 Memory Artifact

```rust
struct MemoryArtifact {
    id: MemoryArtifactId,
    source_kind: MemorySourceKind,
    source_ref: String,
    artifact_kind: MemoryArtifactKind,
    content: String,
    embedding_ref: Option<String>,
    importance_score: Option<f32>,
    provenance: Provenance,
    created_at: DateTime<Utc>,
}
```

### 9.4 Swipe Structure

```rust
struct SwipeGroup {
    id: SwipeGroupId,
    source_user_message_id: MessageId,
    active_generation_id: GenerationId,
}

struct SwipeCandidate {
    id: SwipeCandidateId,
    swipe_group_id: SwipeGroupId,
    generation_id: GenerationId,
    ordinal: u16,
    created_at: DateTime<Utc>,
}
```

### 9.5 Branch Structure

```rust
struct Branch {
    id: BranchId,
    session_id: SessionId,
    root_message_id: MessageId,
    head_message_id: MessageId,
    label: String,
    created_at: DateTime<Utc>,
}
```

### 9.6 Context Plan

```rust
struct ContextPlan {
    id: ContextPlanId,
    session_id: SessionId,
    branch_id: BranchId,
    generated_for_message_id: MessageId,
    total_budget: usize,
    reserved_budget: usize,
    selected_items: Vec<ContextItem>,
    omitted_items: Vec<ContextOmission>,
    truncation_report: TruncationReport,
    created_at: DateTime<Utc>,
}
```

This object is critical. It is the explainability backbone of the system.

---

## 10. Context Assembly System

### 10.1 Hard Context vs Soft Context

#### Hard Context
These are stable and directly user-legible.
- system prompt
- character card
- active persona
- pinned memory
- latest messages
- active author's note

#### Soft Context
These are heuristic or budget-sensitive.
- retrieved summaries
- semantic memory units
- lorebook matches
- keyword anchors
- thinking summaries
- speaker hints
- narrative summaries

The Context Assembler should always prefer preserving Hard Context integrity over packing more Soft Context in.

### 10.2 Assembly Order

Recommended order:

1. system prompt
2. character card
3. active persona
4. pinned memory
5. mandatory lore entries
6. selected retrieved memories
7. recent transcript window
8. author's note
9. optional hint blocks
10. generation marker

### 10.3 Why a Context Plan Matters
For each turn, the system should be able to answer:
- What did the model know?
- What did it not know?
- Why was this memory included?
- Why was this lore omitted?
- Which budget constraint removed content?

### 10.4 Budget Policy
Budgets should be configurable, but the product should expose both:
- target budgets
- observed budgets

If token count falls back to estimation rather than exact count, the plan should record that uncertainty.

### 10.5 Recommended Change
The original design described rolling summary injection and keyword anchoring as silent behavior. These should become configurable soft-context enrichments, visible in the context plan and easy to disable.

---

## 11. Memory System

### 11.1 Canonical transcript remains untouched
The transcript is never replaced by compressed forms.

### 11.2 Memory is artifact-based
The Memory Engine creates artifacts derived from the transcript:
- chunk summaries
- embeddings
- synopsis snapshots
- importance proposals
- retrieval keys

### 11.3 Retrieval Units
Do not tie retrieval strictly to fixed 5-message blocks.
Instead use:
- message-count threshold
- branch checkpoints
- scene/topic boundaries when detectable
- idle-time batching

### 11.4 Summary Types

#### Local Summary
Covers a recent chunk or scene.

#### Session Synopsis
High-level session memory artifact.

#### Branch Synopsis
Optional summary for a particular branch.

### 11.5 Retrieval Scoring
The original scoring concept is strong, but should add provenance weighting.

Suggested formula:

```text
retrieval_score =
    (0.35 × semantic_similarity)
  + (0.25 × importance_score_normalized)
  + (0.20 × recency_decay)
  + (0.20 × provenance_weight)
```

Where provenance weight favors:
- user-authored pinned memory
- explicit card facts
- manually curated entries
over
- speculative auto-generated summaries
- inferred relationships
- stale background artifacts

### 11.6 Importance Scoring
Importance scoring should begin as advisory, not authoritative.

Use it to:
- rank retrieval candidates
- suggest what might deserve pinning
- highlight likely key moments

Do not let it decide permanence by itself.

### 11.7 Compression Policy
Compression should be a context-delivery concern, not a storage mutation. The system may choose shorter memory artifacts for prompt insertion, but it should always preserve the full transcript and older artifacts.

---

## 12. Group Chat Architecture

### 12.1 Revised rollout strategy
Group chat should arrive in phases.

#### Phase 1
- shared scene history
- per-character cards
- user-directed speaker control
- simple round robin

#### Phase 2
- assistive speaker suggestions
- per-character pinned facts
- relationship overlays

#### Phase 3
- optional private knowledge scopes
- narrator policies
- advanced turn routing

#### Phase 4
- hidden memory domains
- unreliable knowledge
- scene-level reasoning helpers

### 12.2 Speaker Selection Strategy
Use hybrid logic.

1. deterministic rules first
   - direct user mention
   - forced `/as Character`
   - round-robin rotation
   - cooldown / recent speaker rules

2. optional assistive ranking second
   - candidate name
   - confidence
   - reason class

Example:

```text
Elara   0.72   direct address
Daren   0.18   relevant stake
Narrator 0.10  transition pressure
```

This is more robust than a single black-box answer.

### 12.3 Narrator
Narrator should begin as:
- explicit command
- optional user-trigger
- maybe scene-tool later

Do not make automatic narrator firing a core early feature.

### 12.4 Per-character memory isolation
Do not make full private retrieval a first-wave requirement.
It is expensive and multiplies ambiguity.

---

## 13. Thinking and Reasoning Block Policy

### 13.1 Keep parser support
The stream parser concept is solid and worth keeping.

### 13.2 Add display modes
Users should be able to choose:

#### Immersive
Hide reasoning unless manually inspected.

#### Assisted
Show one-line summaries of thinking blocks.

#### Debug
Expose raw reasoning blocks, summaries, and parser events.

### 13.3 Elicited thinking
This should remain experimental.
It can:
- increase verbosity
- distort voice
- add latency
- contaminate roleplay style

It should not be assumed beneficial across models.

### 13.4 Summary source policy
A one-line thinking summary is a derived artifact, not canonical content.
It should be treated as optional UI assistance.

---

## 14. Backend Strategy

### 14.1 Capability-based abstraction
Instead of assuming every backend supports the same shape, model backends through capabilities.

```rust
trait ChatCompletionCapability {}
trait EmbeddingCapability {}
trait TokenizationCapability {}
trait GrammarSamplingCapability {}
trait ModelMetadataCapability {}
```

### 14.2 Why this is better
Some backends may:
- stream but not embed
- tokenize poorly
- support grammar constraints inconsistently
- differ in API semantics

Capability-based design allows graceful fallback.

### 14.3 Fine-tuned utility model policy
Do not place a custom fine-tuned utility model on the critical path.
Instead:
- ship with prompt-based utility support
- collect real examples
- benchmark utility failure modes
- decide later whether fine-tuning is justified

### 14.4 Backend degradation rules
If utility backend is down:
- main chat still works
- retrieval may use cached artifacts
- UI surfaces degraded status clearly
- no hidden failure should occur

---

## 15. TUI / UX Design

### 15.1 Keep the terminal-native visual identity
This is one of the product's strengths.

### 15.2 Add a command palette
Do not rely only on slash commands and hotkeys.
A fuzzy palette improves discoverability.

### 15.3 Add a context inspector
This should be separate from the memory browser.
Users need to inspect the assembled context for a turn:
- selected items
- omitted items
- budget use
- warnings
- exact prompt layers

### 15.4 Add a degraded-state indicator
The UI should clearly indicate:
- utility model offline
- retrieval stale
- embedding backlog
- summary backlog
- token estimate mode

### 15.5 Add policy toggles
Examples:
- Auto retrieval: on/off
- Auto speaker suggestions: on/off
- Thinking summaries: on/off
- Narrator assist: on/off
- Diagnostic overlays: on/off

### 15.6 Add a session timeline
Show:
- branch points
- bookmarks
- summary checkpoints
- pinned moments
- narrator interventions
- note changes

This would greatly improve long RP usability.

---

## 16. Reliability, Debugging, and Transparency

### 16.1 Event logging
Important actions should emit structured events:
- message committed
- branch created
- swipe activated
- context plan assembled
- retrieval candidates ranked
- background job failed
- utility suggestion ignored
- stale artifact detected

### 16.2 Reproducibility
Given:
- transcript state
- selected branch
- config snapshot
- context plan
- backend parameters

the system should be able to explain or approximately reproduce the generation setup for a turn.

### 16.3 Proposal vs commit distinction
Every assistive output should be clearly classified as:
- proposal
- accepted proposal
- committed state
- derived artifact

This distinction is essential for debugging.

### 16.4 No invisible mutation
Background jobs must never silently rewrite active conversational state.

---

## 17. Performance Strategy

### 17.1 Performance philosophy
Performance gains should come from:
- low UI overhead
- efficient prompt assembly
- bounded background work
- cached derivations
- graceful fallback
not from fragile intelligence chains

### 17.2 Job priority classes

#### Foreground-critical
- stream completion
- cancel generation
- load active session

#### Foreground-assistive
- context plan generation
- speaker proposal generation

#### Background-low-latency
- embeddings
- importance proposal
- thinking summary proposal

#### Background-batch
- chunk summarization
- retrieval index rebuild
- session synopsis updates

#### Background-idle-only
- cleanup
- export support
- analytics-like maintenance if any local stats exist

### 17.3 Backpressure
The orchestrator should cap concurrency and mark jobs stale rather than allowing uncontrolled queue growth.

### 17.4 Caching
Cache:
- token counts
- embeddings
- prompt assembly fragments where safe
- utility proposals
- session metadata

---

## 18. Revised Roadmap

### Milestone 1: Reliable Core Chat
- TUI shell
- single-character sessions
- SQLite persistence
- branch-safe conversation engine
- swipes
- basic import/export
- author's note
- deterministic context assembler
- context inspector

### Milestone 2: Deterministic Memory
- pinned memory
- retrieval artifact storage
- summary artifacts
- basic semantic retrieval
- retrieval browser
- provenance labels

### Milestone 3: Assistive Layer
- optional importance proposals
- optional keyword extraction
- optional thinking summaries
- optional retrieval recommendations
- degraded-state indicators

### Milestone 4: Group Chat Foundation
- shared scene context
- per-character cards
- user-directed turn control
- round robin mode
- speaker suggestion prototype

### Milestone 5: Advanced Scene Support
- narrator as explicit system actor
- relationship overlays
- improved turn routing
- more advanced inspectors

### Milestone 6: Adaptive Intelligence Experiments
- fine-tune evaluation
- flywheel logging as opt-in
- auto narrator experiments
- per-character private memory experiments

### Milestone 7: Public Release
- stable config
- solid docs
- import tooling
- polished terminal UX
- measured expansion based on real failures, not wishlist pressure

---

## 19. Technical Risks

### Risk 1: Intelligence sprawl
Too many helper systems can make failures hard to diagnose.

**Mitigation:** proposal-based assistive layer, deterministic core.

### Risk 2: Schema churn
A monolithic message table will become unstable.

**Mitigation:** separate canonical messages from generation metadata and artifacts.

### Risk 3: Retrieval drift
Summary artifacts may distort long-running sessions.

**Mitigation:** preserve transcript, show provenance, inspect context plans.

### Risk 4: Group chat explosion
Per-character memory scopes can multiply complexity.

**Mitigation:** phased rollout, shared context first.

### Risk 5: Backend mismatch
Different backends support different features.

**Mitigation:** capability-based abstraction and graceful degradation.

### Risk 6: Premature fine-tuning investment
Fine-tune pipeline may consume effort before actual utility pain points are known.

**Mitigation:** make it optional and late-stage.

---

## 20. Recommendations for First Implementation

### Build first
- canonical conversation engine
- branch model
- swipe system
- context assembler
- context inspector
- pinned memory
- stable TUI loop
- configuration system
- capability-aware backend layer

### Build second
- summary artifact generation
- embeddings
- retrieval artifact viewer
- retrieval insertion policy
- optional importance scoring

### Build later
- speaker selection helpers
- narrator policies
- private character memory scopes
- fine-tuned utility model
- adaptive correction flywheel

### Explicit anti-goals for early versions
- full hidden-intelligence orchestration
- automatic narrator authority
- complex auto-world-building logic
- depending on custom training to make the product viable

---

## 21. Closing Direction

Ozone already has a compelling identity. The original document proves there is a strong product instinct behind it. The key improvement is architectural discipline.

The strongest version of Ozone is not the version with the most automated cleverness. It is the version that:
- remains lightweight
- stays stable under constrained hardware
- makes context and memory legible
- supports excellent roleplay
- behaves predictably
- can gracefully grow into more intelligence over time

The project should therefore proceed with this principle:

**Build a trustworthy conversation engine first.  
Layer intelligence on top only where it clearly improves roleplay without compromising clarity.**

That approach will produce a better product, a better codebase, and a much stronger foundation for every later experiment.

---

Ozone revised design document complete.

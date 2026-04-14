# Ozone Design Document
## Terminal-Native Roleplay Frontend for Local LLMs
### Version 0.3 (Implementation-Ready Architecture)

**Status:** Pre-development — implementation-ready specification
**Primary Language:** Rust 1.75+
**License:** MIT (proposed)
**Lineage:** Builds on v0.2 (Revised Architecture Draft), incorporating cross-model analysis from Gemini 3 Flash, GLM 5.1, Trinity Large Thinking, Qwen 3.6 Plus, and MiMo V2 Pro.

---

## Table of Contents

1. Executive Summary
2. Product Thesis
3. Core Design Principles
4. Product Scope (Tiered)
5. Dependency Selection
6. Workspace & Crate Structure
7. System Architecture
8. Ownership Boundaries
9. Concurrency Model *(new)*
10. Runtime Pipelines
11. Data Model (Complete Type Definitions)
12. Error Taxonomy *(new)*
13. Persistence & Schema Strategy *(new)*
14. Context Assembly System
15. Token Counting Strategy *(new)*
16. Memory System
17. Group Chat Architecture
18. Thinking & Reasoning Block Policy
19. Backend Strategy
20. Configuration System *(new)*
21. TUI / UX Design (Layout, Input, Keybindings, Modes)
22. Security Model *(new)*
23. Reliability, Debugging, & Transparency
24. Performance Strategy (Concrete Targets)
25. Testing Strategy *(new)*
26. Revised Roadmap
27. Technical Risks & Mitigations
28. Recommendations for First Implementation
29. Closing Direction

---

## 1. Executive Summary

Ozone is a terminal-native roleplay frontend for local LLMs. Its revised design emphasizes **deterministic foundations first** and **assistive intelligence second**.

The original v0.2 design established the correct architectural philosophy:
- canonical conversation state must be stable and explainable
- context assembly must be inspectable
- memory must be derived from, not replace, the transcript
- model-driven helpers produce proposals, not silent rewrites
- sophisticated adaptive behavior arrives only after the core loop is proven reliable

This v0.3 document preserves that philosophy and fills the implementation gaps identified by five independent analyses. Specifically, it adds:

- **Complete type definitions** for every referenced struct, enum, and trait
- **Concurrency model** specification (tokio, channels, cancellation contracts)
- **Error taxonomy** with severity levels and recovery policies
- **Persistence schema** with versioning and migration strategy
- **Token counting fallback chain** with confidence tracking
- **Configuration system** with format, hierarchy, and hot-reload semantics
- **TUI layout specification** with wireframes, keybindings, and input model
- **Security model** for untrusted imports and credential storage
- **Performance targets** with concrete, testable numbers
- **Testing strategy** with required test types per subsystem
- **Dependency selection** resolving all major ecosystem decisions

The document is now implementation-ready: a developer should be able to begin coding Milestone 1 without ambiguity about types, concurrency, persistence, or UX.

---

## 2. Product Thesis

Ozone exists to provide a serious roleplay frontend for local models without the overhead of a browser-heavy stack.

The main differentiators:

- Terminal-native operation
- Excellent performance on constrained hardware
- Deep roleplay support rather than general chat tooling
- Transparency around memory and context
- Strong support for local and private workflows
- Smooth operation over SSH, tmux, headless boxes, and low-resource desktops

**Ozone is not trying to be "the smartest orchestrator."
It is trying to be the most reliable, inspectable, low-overhead local RP frontend.**

---

## 3. Core Design Principles

### 3.1 Deterministic core first
The system must produce a correct, understandable result even if all utility intelligence is disabled.

### 3.2 Assistive intelligence, not silent authority
Utility-model outputs enrich or suggest. They never become invisible sources of truth.

### 3.3 Canonical transcript is sacred
The message history is the source of truth. Summaries, embeddings, scores, and retrieval artifacts are derived layers.

### 3.4 Transparency is a feature
Users can inspect: what entered the context, what was omitted, what memories were retrieved, why a speaker was selected, what summaries exist, and whether any background jobs are stale or degraded.

### 3.5 Graceful degradation
If embeddings fail, utility model is offline, or retrieval is stale, Ozone still functions as a strong RP frontend.

### 3.6 Immersion remains primary
Debuggability matters, but the default experience should not bury the story under diagnostics.

### 3.7 No information by color alone *(new)*
Every status indicator must have a text or symbol component. The TUI must be fully usable in monochrome mode.

### 3.8 Errors are first-class *(new)*
Every failure mode has a defined severity, user visibility, retry policy, and fallback behavior. Errors are modeled in the type system, not handled ad-hoc.

### 3.9 Hardware-aware scheduling *(new)*
On single-GPU systems, no background inference jobs (summaries, embeddings) may run while the foreground chat pipeline is active. The Task Orchestrator must respect hardware constraints.
*[Source: Gemini 3 Flash — GPU Mutex]*

---

## 4. Product Scope (Tiered)

### Tier A: Deterministic Core
- session storage, message persistence
- branches, swipes
- TUI chat loop with layout, keybindings, input model
- manual persona support, manual lorebooks
- author's note
- token budgeting with fallback chain
- sliding context window
- pinned memory
- session import/export
- context inspector with dry-run mode
- configuration system
- error handling and degraded-state indicators

### Tier B: Assistive Automation
- retrieval suggestions
- automatic importance scoring suggestions
- speaker selection suggestions
- summary generation
- keyword extraction
- context recommendations
- one-line thinking summaries

### Tier C: Adaptive Intelligence
- WASM plugin interface for user-authored analysis scripts
- fine-tuned utility model as a dependency
- lore gap detection (via plugins)
- auto narrator triggering
- tone classification pipelines
- cross-session learning / flywheel as core behavior
- complex per-character hidden retrieval scopes
- genre-specific utility adapters

### Scope Rule
The first implementation ships mostly Tier A, with a narrow set of Tier B features. Tier C uses a WASM plugin interface rather than baking intelligence into core.
*[Source: Gemini 3 Flash — WASM for Tier C]*

---

## 5. Dependency Selection

These decisions are resolved and should not be revisited without strong cause.

| Concern | Choice | Rationale |
|---------|--------|-----------|
| **TUI Framework** | `ratatui` | Most active Rust TUI library, best ecosystem |
| **Terminal I/O** | `crossterm` | Cross-platform, async-compatible, pairs with ratatui |
| **Async Runtime** | `tokio` | Dominant Rust async runtime, best library compatibility |
| **SQLite Bindings** | `rusqlite` | Direct control, simpler than sqlx for this use case |
| **HTTP Client** | `reqwest` | Async-native, well-maintained, tokio-native |
| **Serialization** | `serde` + `toml` (config) + `serde_json` (data) | Standard Rust serialization ecosystem |
| **Config Format** | TOML | Human-readable, no indentation sensitivity, good Rust support |
| **Embeddings (local)** | `fastembed-rs` (CPU) | Avoids GPU contention with inference backend |
| **Vector Storage** | `usearch` (disk-backed) | Lightweight, Rust bindings, avoids in-memory bloat |
| **Keyword Search** | SQLite FTS5 | Built-in, no additional dependency |
| **CLI Parsing** | `clap` | Standard Rust CLI library |
| **Streaming Parser** | `nom` | Parser-combinator for think-block streaming detection |

*[Sources: Consensus across 4+ models for ratatui/crossterm/tokio/reqwest. Gemini for fastembed-rs. Qwen for usearch/nom. Trinity for SQLite FTS.]*

---

## 6. Workspace & Crate Structure

The architecture maps to a Rust workspace with clear module boundaries.

```
ozone/
├── Cargo.toml                 # workspace root
├── ozone-core/                # Conversation Engine + Data Model + Error Types
│   └── src/
│       ├── lib.rs
│       ├── types.rs           # all type definitions
│       ├── error.rs           # OzoneError taxonomy
│       ├── engine.rs          # ConversationEngine trait + impl
│       ├── branch.rs          # branch operations
│       └── swipe.rs           # swipe operations
├── ozone-context/             # Context Assembler
│   └── src/
│       ├── lib.rs
│       ├── assembler.rs       # ContextAssembler trait + impl
│       ├── policy.rs          # ContextLayerPolicy (data-driven)
│       └── budget.rs          # token budget enforcement
├── ozone-memory/              # Memory Engine
│   └── src/
│       ├── lib.rs
│       ├── artifacts.rs       # MemoryArtifact CRUD
│       ├── retrieval.rs       # hybrid BM25 + vector retrieval
│       ├── scoring.rs         # configurable retrieval scoring
│       └── lifecycle.rs       # tiering, compaction, GC
├── ozone-inference/           # Inference Gateway
│   └── src/
│       ├── lib.rs
│       ├── capabilities.rs    # capability traits
│       ├── backends/          # per-backend implementations
│       ├── tokenizer.rs       # fallback tokenizer chain
│       └── streaming.rs       # SSE/streaming response handling
├── ozone-tasks/               # Task Orchestrator
│   └── src/
│       ├── lib.rs
│       ├── scheduler.rs       # priority queue, backpressure
│       ├── gpu_mutex.rs       # hardware-aware scheduling
│       └── jobs.rs            # job definitions
├── ozone-persist/             # Persistence Layer
│   └── src/
│       ├── lib.rs
│       ├── schema.rs          # schema definitions + migrations
│       ├── repository.rs      # PersistenceLayer trait + impl
│       └── migrations/        # versioned SQL migration files
├── ozone-tui/                 # UI State Layer
│   └── src/
│       ├── lib.rs
│       ├── app.rs             # main app state machine
│       ├── layout.rs          # layout engine
│       ├── input.rs           # InputMode state machine
│       ├── keybindings.rs     # configurable keybindings
│       ├── views/
│       │   ├── chat.rs
│       │   ├── inspector.rs   # context inspector
│       │   ├── memory.rs      # memory browser
│       │   ├── branches.rs    # branch viewer
│       │   ├── timeline.rs    # session timeline
│       │   └── palette.rs     # command palette (fuzzy)
│       └── themes.rs          # color/theme system
└── ozone-cli/                 # Binary entrypoint
    └── src/
        └── main.rs
```

*[Source: MiMo V2 Pro — Workspace Crate Structure]*

---

## 7. System Architecture

```text
┌──────────────────────────────────────────────────────────────────┐
│                           ozone binary                           │
├──────────────────────────────────────────────────────────────────┤
│                                                                  │
│  UI State Layer (ozone-tui)                                      │
│  ← receives immutable state snapshots via broadcast channel      │
│  → sends user actions via mpsc command channel                   │
│                                                                  │
│  Conversation Engine (ozone-core)                                │
│  ← single writer, owns all canonical state mutations             │
│  → emits events via broadcast channel                            │
│                                                                  │
│  Context Assembler (ozone-context)                               │
│  ← reads canonical state + memory artifacts (read-only)          │
│  → produces ContextPlan (immutable output)                       │
│                                                                  │
│  Memory Engine (ozone-memory)                                    │
│  ← reads canonical transcript (read-only)                        │
│  → produces derived artifacts (stored via persist layer)         │
│                                                                  │
│  Inference Gateway (ozone-inference)                             │
│  ← receives assembled prompt                                     │
│  → streams tokens back via channel                               │
│                                                                  │
│  Task Orchestrator (ozone-tasks)                                 │
│  ← owns execution policy and GPU mutex                           │
│  → dispatches background jobs with priority and backpressure     │
│                                                                  │
│  Persistence Layer (ozone-persist)                                │
│  ← single-writer SQLite in WAL mode                              │
│  → concurrent read access for all subsystems                     │
│                                                                  │
└──────────────────────────────────────────────────────────────────┘
```

### Architectural Rule
**Only the Conversation Engine and Context Assembler may commit active state that affects the next generation.**

All other systems may: generate artifacts, compute proposals, provide cached derivations, return suggestions, update derived indexes. They may not silently mutate active prompt state.

---

## 8. Ownership Boundaries

| System | Owns | May Read | May Not Do |
|--------|------|----------|------------|
| **Conversation Engine** | Canonical transcript, sessions, messages, branches, swipe groups, bookmarks, user edits, active branch | Everything | — |
| **Context Assembler** | Prompt construction, budget enforcement, context plans | Transcript, memory artifacts, config | — |
| **Memory Engine** | Derived artifacts: embeddings, retrieval units, summaries, importance proposals, indexes | Transcript (read-only) | Mutate transcript |
| **Inference Gateway** | Backend connections, capability registry, token streaming | Config, capability matrix | Commit state |
| **Task Orchestrator** | Execution policy, priority queues, GPU mutex, rate limits, retries, cancellation | Job status | Commit state |
| **Persistence Layer** | SQLite database, schema migrations, file I/O | — | Business logic |
| **UI State Layer** | Rendering, focus, panes, display mode, keybindings | Immutable state snapshots | Inject prompt content |

In Rust's ownership model, the read-only / write-only split is naturally expressible: `&MemoryEngine` references from the Context Assembler enforce this at compile time.

*[Source: MiMo V2 Pro — ownership table]*

---

## 9. Concurrency Model

### 9.1 Runtime
Ozone uses **tokio** as its async runtime. All subsystems are async-compatible.

### 9.2 Communication Architecture

```text
┌──────────┐   mpsc commands    ┌──────────────────┐
│ TUI Loop │ ──────────────────→│ Conversation      │
│          │←──broadcast events──│ Engine (writer)   │
└──────────┘                    └──────────────────┘
                                        │
                                        │ events
                                        ▼
                                ┌──────────────────┐
                                │ Task Orchestrator │
                                │ (schedules jobs)  │
                                └──────────────────┘
                                        │
                                        │ spawns
                                        ▼
                                ┌──────────────────┐
                                │ Background Jobs   │
                                │ (embeddings,      │
                                │  summaries, etc.) │
                                └──────────────────┘
```

- **TUI → Engine:** `tokio::sync::mpsc` command channel. User actions are sent as typed `Command` enums.
- **Engine → TUI:** `tokio::sync::broadcast` for state change events. TUI receives immutable `Arc<AppState>` snapshots.
- **Engine → Jobs:** Events trigger background work via the Task Orchestrator.
- **Job → Persistence:** Background jobs write derived artifacts through the persistence layer. They never touch canonical state.

### 9.3 Write Serialization
The Conversation Engine is the single writer for canonical state. All mutations go through it via the command channel. This eliminates lock contention on the canonical transcript.

SQLite uses **WAL mode** with a single writer thread. Background jobs use separate read connections.

### 9.4 Cancellation Contract

```rust
enum GenerationState {
    Idle,
    Streaming { tokens_so_far: String, cancel_token: CancellationToken },
    Committed { message_id: MessageId },
    Cancelled { partial: Option<String>, reason: CancelReason },
    Failed { error: OzoneError },
}

enum CancelReason {
    UserInitiated,
    Timeout,
    BackendError,
}
```

**Rules:**
- User-initiated cancellation is always honored immediately
- Partial generation on cancel becomes a **discarded swipe candidate**, not a committed message
- Background job cancellation is best-effort with a 5-second timeout
- The TUI remains responsive during cancellation (never blocks on cleanup)

*[Sources: MiMo V2 Pro — GenerationState/CancelReason. Qwen 3.6 Plus — channel architecture. Consensus — tokio choice.]*

### 9.5 GPU Mutex

```rust
struct GpuMutex {
    permit: Arc<tokio::sync::Semaphore>,  // capacity: 1
}

impl GpuMutex {
    /// Acquire before any inference call (foreground or background).
    /// Foreground tasks take priority — background jobs yield.
    async fn acquire_foreground(&self) -> SemaphorePermit;
    async fn try_acquire_background(&self) -> Option<SemaphorePermit>;
}
```

On single-GPU systems, no background inference jobs (embedding generation, summary generation) may run while the foreground chat pipeline is active. Background jobs use `try_acquire_background` and re-queue if the GPU is busy.

*[Source: Gemini 3 Flash — GPU Mutex]*

---

## 10. Runtime Pipelines

### 10.1 Foreground Chat Pipeline
1. User submits input via TUI
2. TUI sends `Command::SendMessage { content }` via mpsc
3. Conversation Engine appends canonical user message
4. Context Assembler builds `ContextPlan` (may use dry-run first)
5. GPU Mutex acquired (foreground priority)
6. Inference Gateway streams main-model completion
7. On stream completion: Conversation Engine commits assistant response
8. GPU Mutex released
9. UI renders result
10. Background jobs scheduled via Task Orchestrator

This path must remain reliable even with all automation disabled.

### 10.2 Context Dry-Run Pipeline *(new)*
Before step 4 commits to generation, the user may request a dry-run:

1. Context Assembler builds `ContextPlan` without triggering inference
2. TUI renders the plan in the Context Inspector
3. User reviews: included items, omitted items, budget usage
4. User may "force include" an omitted item (by unpinning something else)
5. User confirms → generation proceeds with the adjusted plan

This solves the #1 user frustration: wasting tokens on a generation where critical lorebook entries were cut.

*[Source: Gemini 3 Flash — Context Sandbox]*

### 10.3 Background Derivation Pipeline
After assistant message commit, the Task Orchestrator schedules (respecting GPU mutex and backpressure):
- token count finalization
- embedding job (CPU via fastembed-rs — no GPU contention)
- summary generation job (requires GPU mutex)
- importance proposal job
- retrieval index update
- optional thinking summary generation

These jobs create artifacts. They do not alter the already-committed message.

### 10.4 Retrieval Pipeline
Before generation:
- Context Assembler requests candidate memory units
- Memory Engine returns ranked candidates using **hybrid BM25 + vector retrieval**
- Context Assembler decides what enters prompt based on budget and policy
- Omitted candidates remain visible for inspection

### 10.5 Speaker Selection Pipeline
When group chat is enabled:
- deterministic rules evaluated first (mention, `/as`, round-robin, cooldown)
- if no rule resolves speaker, a suggestion task may run
- suggestion is surfaced as proposal with score and reason class
- Context Assembler / Conversation Engine commits final speaker choice

---

## 11. Data Model (Complete Type Definitions)

All types referenced anywhere in this document are fully defined here.

### 11.1 Identity Types

```rust
use uuid::Uuid;

type SessionId = Uuid;
type MessageId = Uuid;
type BranchId = Uuid;
type SwipeGroupId = Uuid;
type SwipeCandidateId = Uuid;
type GenerationId = Uuid;
type MemoryArtifactId = Uuid;
type ContextPlanId = Uuid;
type BackendId = String;
```

### 11.2 Author

```rust
enum AuthorId {
    User,
    Character { name: String, card_id: String },
    Narrator,
    System,
}

enum Role {
    User,
    Assistant,
    System,
}
```

### 11.3 Canonical Message

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
    edited_at: Option<DateTime<Utc>>,
}
```

This is the truth record. It is never replaced by summaries or compressed forms.

### 11.4 Generation Metadata (Enhanced)

```rust
struct GenerationRecord {
    id: GenerationId,
    message_id: MessageId,
    context_plan_id: ContextPlanId,       // links to the plan that produced this
    backend_id: BackendId,
    model_identifier: String,             // exact model name + version
    sampling_params: SamplingParameters,  // actual values used, not just preset name
    sampler_preset: Option<String>,       // human-readable preset name if applicable
    seed: Option<u64>,                    // for deterministic reproduction
    stop_reason: StopReason,
    token_count: Option<usize>,
    latency_ms: Option<u64>,
    created_at: DateTime<Utc>,
}

struct SamplingParameters {
    temperature: f32,
    top_p: f32,
    top_k: u32,
    min_p: Option<f32>,
    repeat_penalty: Option<f32>,
    frequency_penalty: Option<f32>,
    presence_penalty: Option<f32>,
    max_tokens: Option<usize>,
}

enum StopReason {
    EndOfSequence,
    MaxTokens,
    StopString,
    UserCancelled,
    Error(String),
}
```

*[Source: GLM 5.1 — reproducibility fields. Consensus — all models flagged this.]*

### 11.5 Memory Artifact (Complete)

```rust
struct MemoryArtifact {
    id: MemoryArtifactId,
    source_kind: MemorySourceKind,
    source_ref: String,                     // message_id, range, or external ref
    artifact_kind: MemoryArtifactKind,
    content: MemoryContent,                 // sum type, not just String
    importance_score: Option<f32>,
    provenance: Provenance,
    stale: bool,                            // flagged by StaleArtifactDetector
    created_at: DateTime<Utc>,
    last_accessed_at: Option<DateTime<Utc>>,
}

enum MemorySourceKind {
    SingleMessage,
    MessageRange,
    BranchSegment,
    UserPinned,
    CharacterCard,
    Lorebook,
    External,
}

enum MemoryArtifactKind {
    Summary,
    Embedding,
    ImportanceProposal,
    KeywordSet,
    SessionSynopsis,
    BranchSynopsis,
}

/// Sum type: text-based artifacts vs. dense vectors are stored differently.
enum MemoryContent {
    Text(String),
    Embedding { vector_ref: String, dimensions: u16 },
    Keywords(Vec<String>),
}

enum Provenance {
    UserAuthored,           // weight: 1.0 — user explicitly created this
    CharacterCard,          // weight: 0.9 — from the card definition
    Lorebook,               // weight: 0.8 — curated lore entry
    ManualPin,              // weight: 0.85 — user pinned this memory
    AutoSummary,            // weight: 0.5 — background-generated summary
    AutoEmbedding,          // weight: 0.4 — auto-computed embedding
    InferredRelationship,   // weight: 0.3 — speculative
}
```

*[Source: GLM 5.1 — identified every missing type. Qwen 3.6 Plus — StaleArtifactDetector.]*

### 11.6 Swipe Structure (Enhanced)

```rust
struct SwipeGroup {
    id: SwipeGroupId,
    /// References the message the assistant was *responding to*, not just the user message.
    /// Handles: editing user messages after swiping, multi-turn regen, group chat ambiguity.
    parent_context_message_id: MessageId,
    active_ordinal: u16,
    created_at: DateTime<Utc>,
}

struct SwipeCandidate {
    id: SwipeCandidateId,
    swipe_group_id: SwipeGroupId,
    generation_id: GenerationId,
    ordinal: u16,
    discarded: bool,         // true if from a cancelled generation
    created_at: DateTime<Utc>,
}
```

*[Source: GLM 5.1 — SwipeGroup references parent context]*

### 11.7 Branch Structure (Clarified)

The message tree (via `parent_id: Option<MessageId>`) IS the branch structure. A `Branch` is a **named bookmark** — a shortcut pointing to a path through the tree from root to head. This avoids storing the tree twice.

```rust
struct Branch {
    id: BranchId,
    session_id: SessionId,
    root_message_id: MessageId,
    head_message_id: MessageId,
    label: String,
    is_active: bool,
    created_at: DateTime<Utc>,
}
```

Branches are created by forking from any existing message. They cannot be merged or rebased. Derived artifacts (embeddings, summaries) are **not** copied on fork — they reference source messages and are shared.

*[Source: GLM 5.1 — clarification that Branch is a named path through the tree]*

### 11.8 Context Plan (Complete)

```rust
struct ContextPlan {
    id: ContextPlanId,
    session_id: SessionId,
    branch_id: BranchId,
    generated_for_message_id: MessageId,
    total_budget: usize,
    used_budget: usize,
    reserved_budget: usize,
    safety_margin: usize,                    // 10% reserve for estimation error
    token_estimation_policy: TokenEstimationPolicy,
    selected_items: Vec<ContextItem>,
    omitted_items: Vec<ContextOmission>,
    truncation_report: TruncationReport,
    is_dry_run: bool,
    created_at: DateTime<Utc>,
}

struct ContextItem {
    source: ContextItemSource,
    content_preview: String,         // first 100 chars for inspection
    token_count: usize,
    layer: ContextLayerKind,
    required: bool,                  // hard vs. soft context
}

enum ContextItemSource {
    SystemPrompt,
    CharacterCard { card_id: String },
    Persona { name: String },
    PinnedMemory { artifact_id: MemoryArtifactId },
    LorebookEntry { entry_id: String, keyword: String },
    TranscriptMessage { message_id: MessageId },
    RetrievedMemory { artifact_id: MemoryArtifactId, score: f32 },
    AuthorsNote,
    SpeakerHint,
    ThinkingSummary,
}

struct ContextOmission {
    source: ContextItemSource,
    token_count: usize,
    reason: OmissionReason,
}

enum OmissionReason {
    BudgetExhausted { budget_remaining: usize },
    ScoreBelowThreshold { score: f32, threshold: f32 },
    LayerBudgetExceeded { layer: ContextLayerKind, layer_budget_pct: f32 },
    DisabledByPolicy,
    StaleArtifact,
    DuplicateContent,
}

struct TruncationReport {
    truncated_items: Vec<TruncatedItem>,
    total_tokens_removed: usize,
}

struct TruncatedItem {
    source: ContextItemSource,
    original_tokens: usize,
    remaining_tokens: usize,
    truncation_strategy: TruncationStrategy,
}

enum TruncationStrategy {
    TrimFromStart,
    TrimFromEnd,
    Summarize,
}
```

*[Source: GLM 5.1 — identified every missing type. MiMo V2 Pro — TokenEstimationPolicy.]*

---

## 12. Error Taxonomy

```rust
/// Top-level error type. All subsystems use this.
type OzoneResult<T> = Result<T, OzoneError>;

enum OzoneError {
    // ── Fatal: cannot continue ──
    CorruptTranscript { session_id: SessionId, detail: String },
    DatabaseCorrupt { path: PathBuf },
    MigrationFailed { version: u32, reason: String },

    // ── Degraded: feature unavailable but chat works ──
    BackendUnavailable { backend_id: BackendId },
    BackendCapabilityMissing { backend_id: BackendId, capability: &'static str },
    InferenceTimeout { deadline: Duration },
    RetrievalBackendOffline,
    EmbeddingServiceTimeout,
    BudgetOverflow { required: usize, available: usize },
    TokenizationMismatch { expected: usize, actual: usize },

    // ── Advisory: informational only ──
    TokenEstimateFallback { method: TokenEstimationPolicy },
    StaleArtifact { artifact_id: MemoryArtifactId, age: Duration },
    EmbeddingFailed { source: String },

    // ── Session ──
    BranchConflict { branch_a: BranchId, branch_b: BranchId },
    SessionNotFound { session_id: SessionId },
    MessageNotFound { message_id: MessageId },

    // ── I/O ──
    SerializationFailed { format: &'static str, detail: String },
    ImportFailed { path: PathBuf, reason: String },
    ConfigInvalid { key: String, reason: String },
}

/// Every error has a defined policy.
impl OzoneError {
    fn severity(&self) -> ErrorSeverity { /* ... */ }
    fn user_visibility(&self) -> UserVisibility { /* ... */ }
    fn retry_policy(&self) -> RetryPolicy { /* ... */ }
}

enum ErrorSeverity { Fatal, Degraded, Advisory }

enum UserVisibility {
    Modal,      // blocks interaction, requires acknowledgment
    StatusBar,  // shown in status bar, non-blocking
    LogOnly,    // recorded but not surfaced unless in Developer mode
}

enum RetryPolicy {
    NoRetry,
    Immediate { max_attempts: u32 },
    ExponentialBackoff { base_ms: u64, max_attempts: u32, max_delay_ms: u64 },
    CircuitBreaker { threshold: u32, reset_after: Duration },
}
```

*[Sources: GLM 5.1 — error taxonomy concept. MiMo V2 Pro — detailed error enum. Qwen 3.6 Plus — retry policies.]*

---

## 13. Persistence & Schema Strategy

### 13.1 File Organization
- **One SQLite database per session.** Simplifies deletion, export, backup, and concurrent access.
- Character cards stored as **JSON files on disk** (interoperable with SillyTavern and other tools).
- Attachments (images) stored in a session-local directory alongside the DB.
- Global config is a TOML file, not in SQLite.

### 13.2 Database Location
```
~/.local/share/ozone/
├── config.toml                       # global config
├── characters/                       # character card JSON files
│   ├── elara.json
│   └── daren.json
├── sessions/
│   ├── <session-uuid>/
│   │   ├── session.db                # SQLite database
│   │   ├── config.toml               # session-level config overrides
│   │   └── attachments/              # images, exports
│   └── ...
└── backups/
```

### 13.3 Schema Versioning

```sql
-- First table created in every database.
CREATE TABLE schema_version (
    version INTEGER NOT NULL,
    applied_at TEXT NOT NULL DEFAULT (datetime('now')),
    description TEXT
);

INSERT INTO schema_version (version, description) VALUES (1, 'Initial schema');
```

**Migration rules:**
- Every migration is a numbered SQL file: `001_initial.sql`, `002_add_importance.sql`, etc.
- Every migration is wrapped in a transaction (`BEGIN; ... COMMIT;`)
- Migrations are forward-only (no rollback scripts) but backup before migration is mandatory
- The application checks `schema_version` on startup and applies pending migrations
- A failed migration leaves the database untouched (transaction rollback)

### 13.4 SQLite Configuration

```sql
PRAGMA journal_mode = WAL;           -- write-ahead logging for concurrent reads
PRAGMA synchronous = NORMAL;         -- safe with WAL, good performance
PRAGMA foreign_keys = ON;
PRAGMA busy_timeout = 5000;          -- 5s wait before SQLITE_BUSY
```

### 13.5 Core Tables (Schema v1)

```sql
CREATE TABLE messages (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    branch_id TEXT NOT NULL REFERENCES branches(id),
    parent_id TEXT REFERENCES messages(id),
    author_kind TEXT NOT NULL,         -- 'user', 'character', 'narrator', 'system'
    author_name TEXT,                  -- null for user/system
    role TEXT NOT NULL,                -- 'user', 'assistant', 'system'
    content TEXT NOT NULL,
    created_at TEXT NOT NULL,
    edited_at TEXT
);

CREATE TABLE branches (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    root_message_id TEXT NOT NULL REFERENCES messages(id),
    head_message_id TEXT NOT NULL REFERENCES messages(id),
    label TEXT NOT NULL,
    is_active INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL
);

-- Closure table for efficient ancestry queries
CREATE TABLE message_ancestry (
    ancestor_id TEXT NOT NULL REFERENCES messages(id),
    descendant_id TEXT NOT NULL REFERENCES messages(id),
    depth INTEGER NOT NULL,
    PRIMARY KEY (ancestor_id, descendant_id)
);

CREATE TABLE generation_records (
    id TEXT PRIMARY KEY,
    message_id TEXT NOT NULL REFERENCES messages(id),
    context_plan_id TEXT,
    backend_id TEXT NOT NULL,
    model_identifier TEXT NOT NULL,
    sampling_params_json TEXT NOT NULL,   -- serialized SamplingParameters
    sampler_preset TEXT,
    seed INTEGER,
    stop_reason TEXT NOT NULL,
    token_count INTEGER,
    latency_ms INTEGER,
    created_at TEXT NOT NULL
);

CREATE TABLE swipe_groups (
    id TEXT PRIMARY KEY,
    parent_context_message_id TEXT NOT NULL REFERENCES messages(id),
    active_ordinal INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL
);

CREATE TABLE swipe_candidates (
    id TEXT PRIMARY KEY,
    swipe_group_id TEXT NOT NULL REFERENCES swipe_groups(id),
    generation_id TEXT NOT NULL REFERENCES generation_records(id),
    ordinal INTEGER NOT NULL,
    discarded INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL
);

CREATE TABLE memory_artifacts (
    id TEXT PRIMARY KEY,
    source_kind TEXT NOT NULL,
    source_ref TEXT NOT NULL,
    artifact_kind TEXT NOT NULL,
    content_text TEXT,                     -- for text-based artifacts
    content_keywords_json TEXT,            -- for keyword sets
    embedding_ref TEXT,                    -- reference to vector store
    embedding_dimensions INTEGER,
    importance_score REAL,
    provenance TEXT NOT NULL,
    stale INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    last_accessed_at TEXT
);

CREATE TABLE context_plans (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    branch_id TEXT NOT NULL,
    generated_for_message_id TEXT NOT NULL,
    total_budget INTEGER NOT NULL,
    used_budget INTEGER NOT NULL,
    reserved_budget INTEGER NOT NULL,
    safety_margin INTEGER NOT NULL,
    token_estimation_policy TEXT NOT NULL,
    plan_json TEXT NOT NULL,               -- full serialized ContextPlan
    is_dry_run INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL
);

-- Indexes
CREATE INDEX idx_messages_branch ON messages(branch_id, created_at);
CREATE INDEX idx_messages_parent ON messages(parent_id);
CREATE INDEX idx_artifacts_source ON memory_artifacts(source_ref);
CREATE INDEX idx_artifacts_kind ON memory_artifacts(artifact_kind, stale);
CREATE INDEX idx_generation_message ON generation_records(message_id);

-- Full-Text Search for keyword retrieval
CREATE VIRTUAL TABLE messages_fts USING fts5(content, content=messages, content_rowid=rowid);
CREATE VIRTUAL TABLE artifacts_fts USING fts5(content_text, content=memory_artifacts, content_rowid=rowid);
```

*[Sources: Gemini 3 Flash — Closure Table. GLM 5.1 — one DB per session, schema versioning, migration strategy. Qwen 3.6 Plus — WAL mode. MiMo V2 Pro — persistence trait. Trinity — SQLite FTS.]*

### 13.6 Data Durability Classification

| Data | Durability | Recovery |
|------|-----------|----------|
| Messages | **Canonical** — must survive any failure | Unrecoverable if lost |
| Branches | **Canonical** | Unrecoverable |
| Generation Records | **Important** — aids reproducibility | Regenerable (lose history) |
| Context Plans | **Ephemeral-persisted** — keep last N per session | Fully regenerable |
| Memory Artifacts | **Derived** — regenerable from transcript | Background re-derivation |
| Embeddings | **Derived** — stored in vector index | Re-embed from messages |

**Transaction rule:** All state transitions affecting generation go through a single SQLite transaction. The Conversation Engine's `commit_message` operation atomically writes: message + generation record + branch head update + swipe candidate (if applicable).

---

## 14. Context Assembly System

### 14.1 Hard Context vs Soft Context

**Hard Context** (always preserved, never cut for budget):
- system prompt
- character card
- active persona
- pinned memory
- latest messages (configurable window)
- active author's note

**Soft Context** (heuristic, budget-sensitive, may be omitted):
- retrieved summaries
- semantic memory units
- lorebook matches
- keyword anchors
- thinking summaries
- speaker hints
- narrative summaries

The Context Assembler always preserves Hard Context integrity over packing more Soft Context.

### 14.2 Data-Driven Assembly Policy

The assembly order is **configurable**, not hardcoded. Each layer has budget constraints.

```rust
struct ContextLayerPolicy {
    layers: Vec<ContextLayerSpec>,  // ordered list — first = highest priority
}

struct ContextLayerSpec {
    kind: ContextLayerKind,
    required: bool,              // hard context = true
    max_budget_pct: f32,         // max % of total budget this layer may consume
    min_budget_pct: f32,         // min % guaranteed if content available
    enabled: bool,               // can be toggled by user
}

enum ContextLayerKind {
    SystemPrompt,
    CharacterCard,
    ActivePersona,
    PinnedMemory,
    MandatoryLore,
    RetrievedMemories,
    RecentTranscript,
    AuthorsNote,
    OptionalHints,
    GenerationMarker,
}
```

**Default policy:**

| Priority | Layer | Required | Max Budget | Min Budget |
|----------|-------|----------|-----------|-----------|
| 1 | System Prompt | yes | 10% | 5% |
| 2 | Character Card | yes | 15% | 10% |
| 3 | Active Persona | yes | 5% | 2% |
| 4 | Pinned Memory | yes | 10% | 5% |
| 5 | Mandatory Lore | yes | 10% | 5% |
| 6 | Retrieved Memories | no | 15% | 0% |
| 7 | Recent Transcript | yes | 30% | 20% |
| 8 | Author's Note | yes | 5% | 2% |
| 9 | Optional Hints | no | 5% | 0% |
| 10 | Generation Marker | yes | 1% | 1% |

Users can customize this policy via config. The Context Inspector shows which policy was active.

*[Source: GLM 5.1 — ContextLayerPolicy making assembly order data-driven]*

### 14.3 Context Plan as Explainability Backbone

For each turn, the system answers:
- What did the model know?
- What did it not know?
- Why was this memory included?
- Why was this lore omitted?
- Which budget constraint removed content?

### 14.4 Dry-Run Mode

Users can generate a `ContextPlan` without triggering inference. The Context Inspector renders it. The user reviews budget allocation, force-includes omitted items (by freeing budget elsewhere), and confirms before generation proceeds.

*[Source: Gemini 3 Flash — Context Sandbox]*

### 14.5 Budget Policy
Budgets are configurable. The product exposes both target and observed budgets. If token count falls back to estimation, the plan records the `TokenEstimationPolicy` and a safety margin is enforced.

---

## 15. Token Counting Strategy

### 15.1 Fallback Chain

```rust
enum TokenEstimationPolicy {
    /// Most accurate. Calls the backend's tokenizer endpoint.
    ExactBackendTokenizer,

    /// Fast, may diverge ±10%. Uses a local tokenizer matched to the model family.
    LocalApproximateTokenizer { model_family: String },

    /// Emergency fallback. Characters × model-specific multiplier.
    CharacterCountHeuristic { chars_per_token: f32 },
}
```

The system attempts each level in order, falling back on failure:

1. **Exact backend tokenizer** — query the backend for precise token count
2. **Local approximate tokenizer** — use a cached `.json` tokenizer file matched to the model family (Llama, Mistral, etc.)
3. **Character-count heuristic** — `content.len() as f32 * chars_per_token` with model-specific multiplier (default: 0.25 tokens/char for English)

### 15.2 Safety Margin
When using approximate or heuristic counting, the Context Assembler reserves a **10% safety margin** to prevent context overflow. The margin is recorded in the `ContextPlan`.

### 15.3 Confidence Tracking
Every `ContextPlan` records which estimation policy was used. The Context Inspector visually distinguishes exact counts from estimates.

*[Sources: MiMo V2 Pro — TokenEstimationPolicy. Gemini 3 Flash — tokenizer accuracy. Consensus — all 5 models flagged this.]*

---

## 16. Memory System

### 16.1 Canonical transcript remains untouched
The transcript is never replaced by compressed forms.

### 16.2 Memory is artifact-based
The Memory Engine creates artifacts derived from the transcript: chunk summaries, embeddings, synopsis snapshots, importance proposals, retrieval keys.

### 16.3 Hybrid Retrieval (BM25 + Vector)

Standard vector embeddings often fail on specific names (e.g., "Xylo-7"). Ozone uses **hybrid retrieval**:

- **BM25 (keyword):** SQLite FTS5 for exact name/term matching
- **Vector (semantic):** `usearch` disk-backed index via `fastembed-rs` embeddings

For RP, finding the exact name of a sword is often more important than finding "something similar to a weapon."

**Scoring combination:**
```rust
fn hybrid_score(bm25_score: f32, vector_score: f32, alpha: f32) -> f32 {
    // alpha = 0.5 by default, configurable
    (alpha * bm25_score) + ((1.0 - alpha) * vector_score)
}
```

*[Source: Gemini 3 Flash — BM25 + Vector hybrid]*

### 16.4 Retrieval Scoring (Configurable)

```rust
struct RetrievalWeights {
    semantic: f32,      // default 0.35
    importance: f32,    // default 0.25
    recency: f32,       // default 0.20
    provenance: f32,    // default 0.20
}

fn compute_retrieval_score(
    candidate: &RetrievalCandidate,
    weights: &RetrievalWeights,
) -> f32 {
    let semantic = candidate.semantic_similarity.unwrap_or(0.0);
    let importance = candidate.importance_score.unwrap_or(0.5); // neutral default
    let recency = candidate.recency_decay();
    let provenance = candidate.provenance_weight();

    (weights.semantic * semantic
        + weights.importance * importance
        + weights.recency * recency
        + weights.provenance * provenance)
        .clamp(0.0, 1.0)
}
```

All terms are normalized to `[0.0, 1.0]`. Weights are configurable per session or per character. The sum of weights should equal 1.0 (validated at config load).

*[Source: Consensus — all 5 models required configurable weights. GLM 5.1 — code example.]*

### 16.5 Provenance Decay

Auto-generated summaries lose **15% weight** for every retrieval cycle without user interaction. This prevents stale AI-generated content from dominating retrieval over time.

```rust
fn adjusted_provenance_weight(base: f32, cycles_since_interaction: u32) -> f32 {
    base * (0.85_f32).powi(cycles_since_interaction as i32)
}
```

*[Source: Qwen 3.6 Plus — Provenance Decay]*

### 16.6 Memory Storage Tiering

| Age | Storage Level | What's Kept |
|-----|--------------|-------------|
| Recent (< 100 messages) | Full | All artifacts: embeddings, summaries, importance scores |
| Older (100–1000 messages) | Reduced | Summaries + embeddings only. Raw importance proposals pruned. |
| Archive (> 1000 messages) | Minimal | Session synopsis + key pinned memories only |

Thresholds are configurable. The UI shows a storage usage indicator. Automatic cleanup runs as a `Background-idle-only` job.

*[Source: Trinity Large Thinking — Memory Storage Tiering]*

### 16.7 Stale Artifact Detection

```rust
struct StaleArtifactPolicy {
    max_age_messages: usize,    // default: 500
    max_age_hours: u64,         // default: 168 (1 week)
}
```

The `StaleArtifactDetector` flags artifacts exceeding either threshold. The UI marks them `⚠ stale` rather than silently omitting them. Users can manually refresh or dismiss.

*[Source: Qwen 3.6 Plus — StaleArtifactDetector]*

### 16.8 Garbage Collection

```rust
struct GarbageCollectionPolicy {
    max_active_embeddings: usize,     // default: 10_000
    archive_after_n_turns: usize,     // default: 1_000
    purge_unreferenced_backlog: bool, // default: true
    compaction_interval_hours: u64,   // default: 24
}
```

Background compaction periodically: merges stale embeddings, clears orphaned `MemoryArtifact` rows, and regenerates `SessionSynopsis` without blocking foreground generation.

*[Source: Qwen 3.6 Plus — explicit GC policies]*

### 16.9 Summary Types

- **Local Summary:** Covers a recent chunk or scene.
- **Session Synopsis:** High-level session memory artifact.
- **Branch Synopsis:** Optional summary for a particular branch.

### 16.10 Importance Scoring
Importance scoring begins as advisory, not authoritative. Used to: rank retrieval candidates, suggest what might deserve pinning, highlight likely key moments. It does not decide permanence by itself.

### 16.11 Compression Policy
Compression is a context-delivery concern, not a storage mutation. The system may choose shorter artifacts for prompt insertion but always preserves the full transcript and older artifacts.

---

## 17. Group Chat Architecture

### 17.1 Phased Rollout

#### Phase 1 (Enhanced MVP)
- shared scene history
- per-character cards
- user-directed speaker control (`/as Character`)
- simple round robin
- **mention-based speaker auto-detection** (if message contains a character name)
- **simple relationship hints in context** (configurable per-character)
- **narrator toggle** for explicit scene descriptions

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

*[Source: Trinity Large Thinking — enhanced Phase 1 MVP]*

### 17.2 Speaker Selection Strategy
Hybrid logic:
1. Deterministic rules first: direct mention, `/as Character`, round-robin, cooldown
2. Optional assistive ranking second: candidate name, confidence, reason class

### 17.3 Narrator
Narrator begins as explicit command / optional user-trigger. Auto narrator firing is Tier C.

### 17.4 Per-character memory isolation
Not a first-wave requirement. Expensive and multiplies ambiguity.

---

## 18. Thinking & Reasoning Block Policy

### 18.1 Streaming Parser
Use a **`nom`-based streaming state machine** to detect `<think>`/`</think>` boundaries during streaming without buffering the full response. Emit UI events in real-time to update display panes.

*[Source: Qwen 3.6 Plus — streaming parser]*

### 18.2 Display Modes
- **Immersive:** Hide reasoning unless manually inspected
- **Assisted:** Show one-line summaries of thinking blocks
- **Debug:** Expose raw reasoning blocks, summaries, and parser events

### 18.3 Elicited Thinking
**Explicit opt-in only** with model-specific warnings. The UI should note:
- "This works well with [model X], but may degrade with [model Y]"
- Potential impacts: increased verbosity, voice distortion, added latency

*[Source: Trinity Large Thinking — explicit opt-in with model warnings]*

### 18.4 Summary Source Policy
A one-line thinking summary is a derived artifact, not canonical content. Treated as optional UI assistance.

---

## 19. Backend Strategy

### 19.1 Capability-Based Abstraction

```rust
trait ChatCompletionCapability {
    async fn complete(&self, prompt: &str, params: &SamplingParameters)
        -> OzoneResult<impl Stream<Item = OzoneResult<String>>>;
}

trait EmbeddingCapability {
    async fn embed(&self, texts: &[&str]) -> OzoneResult<Vec<Vec<f32>>>;
}

trait TokenizationCapability {
    fn count_tokens(&self, text: &str) -> OzoneResult<usize>;
    fn model_family(&self) -> &str;
}

trait GrammarSamplingCapability {
    async fn complete_with_grammar(&self, prompt: &str, grammar: &str, params: &SamplingParameters)
        -> OzoneResult<impl Stream<Item = OzoneResult<String>>>;
}

trait ModelMetadataCapability {
    fn model_name(&self) -> &str;
    fn context_length(&self) -> usize;
    fn supported_stop_strings(&self) -> &[String];
}
```

### 19.2 Capability Registry

```rust
struct CapabilityMatrix {
    chat: Box<dyn ChatCompletionCapability>,
    embedding: Option<Box<dyn EmbeddingCapability>>,
    tokenizer: Box<dyn TokenizationCapability>,
    grammar: Option<Box<dyn GrammarSamplingCapability>>,
    metadata: Option<Box<dyn ModelMetadataCapability>>,
}
```

At startup, capability probes run and populate the matrix. The system uses the fallback chain for missing capabilities.

*[Source: Qwen 3.6 Plus — CapabilityMatrix]*

### 19.3 Tokenizer Fallback Chain
Exact backend tokenizer → local approximate tokenizer → character-count heuristic (see §15).

### 19.4 Fine-Tuned Utility Model Policy
Not on the critical path. Ship with prompt-based utility support. Collect real examples. Benchmark failure modes. Decide later.

### 19.5 Backend Degradation Rules
If utility backend is down: main chat still works, retrieval may use cached artifacts, UI surfaces degraded status clearly, no hidden failure occurs.

---

## 20. Configuration System

### 20.1 Format
TOML. Human-readable, no indentation sensitivity, strong Rust ecosystem support via `toml` + `serde`.

### 20.2 File Hierarchy

```
1. Hardcoded defaults (in code)
2. ~/.config/ozone/config.toml            (global user config)
3. <session_dir>/config.toml              (per-session overrides)
4. Character card embedded settings        (per-character)
5. CLI flags                              (override all)
```

Each layer overrides the previous. `serde` deserialization merges layers.

### 20.3 XDG Compliance
- Config: `$XDG_CONFIG_HOME/ozone/` (default: `~/.config/ozone/`)
- Data: `$XDG_DATA_HOME/ozone/` (default: `~/.local/share/ozone/`)
- Cache: `$XDG_CACHE_HOME/ozone/` (default: `~/.cache/ozone/`)

### 20.4 Hot-Reload vs. Immutable

| Setting | Mutable at Runtime? | Requires Restart? |
|---------|--------------------|--------------------|
| Backend URL | No | Yes |
| Database path | No | Yes |
| Theme / colors | Yes | No |
| Retrieval weights | Yes | No |
| Context layer policy | Yes | No |
| Keybindings | Yes | No (reload via command) |
| Token budget | Yes | No |
| Backpressure limits | Yes | No |
| Embedding model | No | Yes |

*[Source: MiMo V2 Pro — hot-reload distinction. Consensus — TOML + hierarchy.]*

### 20.5 Validation
Config is validated at load time. Invalid values produce `OzoneError::ConfigInvalid` with specific key and reason. The system refuses to start with a fatal config error.

### 20.6 Example Config

```toml
[backend]
url = "http://localhost:5001"
type = "koboldcpp"   # "koboldcpp", "ollama", "openai-compatible"

[context]
max_tokens = 8192
safety_margin_pct = 10
default_policy = "standard"   # references a named policy

[context.weights]
semantic = 0.35
importance = 0.25
recency = 0.20
provenance = 0.20

[memory]
max_active_embeddings = 10000
archive_after_turns = 1000
compaction_interval_hours = 24

[ui]
theme = "dark"
mode = "standard"   # "minimal", "standard", "developer"
message_collapse_lines = 20

[ui.keybindings]
send = "Enter"
newline = "Alt+Enter"
command_palette = "Ctrl+p"
context_inspector = "Ctrl+i"
cancel_generation = "Ctrl+c"
branch_viewer = "Ctrl+b"
swipe_left = "Ctrl+Left"
swipe_right = "Ctrl+Right"
quit = "Ctrl+q"
help = "?"

[tasks]
max_concurrent_jobs = 3
max_queue_size = 20
stale_job_timeout_secs = 300
```

---

## 21. TUI / UX Design

### 21.1 Default Layout

```text
┌─────────────────────────────────────────────────────────────────┐
│ Ozone v0.3          [Session: Dragon's Rest]  [Elara]  🔗0 📝0 │
├────────────────────────────────────┬────────────────────────────┤
│                                    │ Context Inspector          │
│  Chat Area                         │ ┌──────────────────────┐  │
│  (scrollable, virtualized)         │ │ Budget: 3847/8192    │  │
│                                    │ │ ████████░░░░ 47%     │  │
│  [System] You are Elara, a         │ ├──────────────────────┤  │
│           wandering mage...        │ │ ✓ System Prompt  120 │  │
│                                    │ │ ✓ Character Card 340 │  │
│  [Elara] The forest path twisted   │ │ ✓ Pinned Memory  89  │  │
│          ahead, ancient oaks       │ │ ✓ Recent msgs   2100 │  │
│          forming a canopy...       │ │ ✓ Lore: Xylo-7   45 │  │
│                                    │ │ ✗ Lore: Old War      │  │
│  [You] I draw my sword and step    │ │   (budget exceeded)  │  │
│        forward cautiously.         │ │ ✗ Summary #42        │  │
│                                    │ │   (score: 0.31)      │  │
│  [Elara] Your blade catches a      │ └──────────────────────┘  │
│          shaft of pale moonlight.. │ [Force Include] [Dry Run] │
├────────────────────────────────────┴────────────────────────────┤
│ > Type your message...                                  [Enter] │
├─────────────────────────────────────────────────────────────────┤
│ Tokens: 3847/8192 │ llama.cpp │ Retrieval: OK │ Jobs: 0 │ Std │
└─────────────────────────────────────────────────────────────────┘
```

- Right pane (Context Inspector) is **toggleable** — chat takes full width when closed
- Status bar is always visible
- Minimum terminal size: **80×24** (SSH-friendly)
- Layout adapts: inspector auto-hides below 120 columns

*[Sources: GLM 5.1 and MiMo V2 Pro — both provided layout wireframes.]*

### 21.2 UI Modes

| Mode | Description | Default Visibility |
|------|-------------|-------------------|
| **Minimal** | Immersive RP. Chat + input only. No inspector, no status details. | Chat, input |
| **Standard** | Balanced. Chat + toggleable inspector + status bar. | Chat, input, status bar |
| **Developer** | Full transparency. All panes, all diagnostics, raw reasoning blocks. | Everything |

Users switch modes via command palette or config. Layout presets available: "Immersive," "Debugging," "Memory Review."

*[Source: Trinity Large Thinking — Multiple UI Modes]*

### 21.3 Input Model

```rust
enum InputMode {
    Normal,           // Typing a message. Enter sends, Alt+Enter = newline.
    Command,          // After typing "/" — autocomplete slash commands
    Palette,          // Command palette open — fuzzy search
    Inspector,        // Focus in context inspector pane — navigation keys
    Search,           // Ctrl+R history search / Ctrl+F in-message search
    EditorLaunch,     // $EDITOR opened for long-form composition
}
```

**Input support:**
- Single-line input with `Enter` to send
- `Alt+Enter` for newline (multi-line composition)
- `$EDITOR` escape for long messages (launch external editor)
- Input history with up-arrow recall
- Paste handling (multi-line pastes auto-enter multi-line mode)

*[Source: MiMo V2 Pro — InputMode state machine. GLM 5.1 — $EDITOR support.]*

### 21.4 Default Keybindings

| Action | Binding | Mode |
|--------|---------|------|
| Send message | Enter | Normal |
| Newline | Alt+Enter | Normal |
| Command palette | Ctrl+P | Any |
| Context inspector toggle | Ctrl+I | Any |
| Cancel generation | Ctrl+C | Any |
| Branch viewer | Ctrl+B | Any |
| Swipe right | Ctrl+→ | Normal |
| Swipe left | Ctrl+← | Normal |
| Scroll up | PageUp / Shift+Up | Any |
| Scroll down | PageDown / Shift+Down | Any |
| Regenerate | Ctrl+R | Normal |
| Quick help | ? | Normal (when input empty) |
| Quit | Ctrl+Q | Any |
| Focus switch | Tab | Any |
| Search | Ctrl+F | Any |
| History search | Ctrl+H | Normal |
| Dry-run context | Ctrl+D | Normal |

All keybindings are **user-configurable** via `config.toml` `[ui.keybindings]`.

*[Sources: GLM 5.1 and MiMo V2 Pro — independently proposed overlapping sets.]*

### 21.5 Color / Theme System

Three tiers of terminal color support:

| Tier | Capability | Rendering Strategy |
|------|-----------|-------------------|
| **Monochrome** | No color | Bold, underline, dim, reverse for emphasis |
| **16-color** | Named ANSI colors | Semantic color names (primary, secondary, warning, error) |
| **Truecolor** | 24-bit hex | Full theme support with custom hex values |

All information must be readable in monochrome mode. Auto-detected at startup, overridable in config.

*[Source: GLM 5.1 — three-tier color system]*

### 21.6 Long Message Handling

- Messages auto-collapse above a configurable line threshold (default: 20 lines)
- Collapsed messages show a "▸ expand (47 lines)" indicator
- Messages are rendered lazily (only visible messages are formatted) — virtualized list
- "Jump to message" command for timeline navigation
- Swipe comparison uses a **diff-style side-by-side view** when terminal is wide enough

*[Sources: GLM 5.1 — long message handling. Gemini 3 Flash — vim-diff swipe comparator. Qwen 3.6 Plus — virtualized rendering.]*

### 21.7 Context Inspector UX

- **Split-pane view** (right side, toggleable)
- **Left section:** Assembled prompt as it will be sent, scrollable, with token counts per section
- **Right section:** Omitted items with reasons and a **"Force Include"** action
- **Header:** Budget bar showing used/total with percentage
- **Diff mode:** Between turns, highlight added/omitted items (`+` green, `-` red, `~` yellow)
- **Dry-run button:** Generate ContextPlan without spending tokens

*[Sources: GLM 5.1 — split-pane + force include. Qwen 3.6 Plus — diff view.]*

### 21.8 Status Bar

```text
│ Tokens: 3847/8192 │ llama.cpp │ Retrieval: OK │ 🔗0 📝0 ⚠0 │ Standard │
```

- Token count with budget
- Active backend name
- Retrieval status (OK / stale / offline)
- Background job indicators: 🔗 embedding queue, 📝 summary queue, ⚠ degradation flags
- Current UI mode

### 21.9 Onboarding / First-Run Experience

On first launch (no config file detected):

1. **Backend setup wizard:** "Where is your LLM running?" → auto-detect Ollama/KoboldCpp/llama.cpp on common ports
2. **Character card:** Offer to import existing card or use built-in sample character
3. **Quick tutorial:** Highlight key features (branches, swipes, context inspector) with a sample session
4. **Feature discovery:** First session shows subtle `?` hint overlay

*[Source: GLM 5.1 — onboarding flow. Trinity Large Thinking — tutorial mode.]*

### 21.10 Notifications for Background Jobs

- Status bar icons update in real-time with queue depth
- When a summary becomes available, a non-intrusive "📝 New summary available" appears in status bar for 5 seconds
- Degradation flags persist until resolved
- In Developer mode, a dedicated "Jobs" panel shows full queue state

### 21.11 Accessibility

- `--plain` CLI flag strips all ANSI styling, uses ASCII borders, outputs linear text for screen readers
- Terminal width detection with content-boundary wrapping (preserves copy-paste integrity)
- All status indicators have text/symbol component alongside color
- High-contrast theme available

*[Sources: Qwen 3.6 Plus — `--plain` mode. GLM 5.1 — no color-only information principle.]*

---

## 22. Security Model

### 22.1 Untrusted Input

Character cards and lorebooks are **untrusted input**. They may contain prompt injection attempts.

**Rules:**
- Character card fields are sanitized before display and before injection into context
- Lorebook entries are scoped to **soft context only** — they cannot override hard context (system prompt, pinned memory)
- Imported files undergo strict JSON/YAML schema validation via `serde` + `schemars`
- No executable code in character cards or lorebooks (Tier C WASM plugins are sandboxed separately)

### 22.2 Credential Storage

- API keys stored in **OS keychain** when available (`keyring` crate on macOS/Linux/Windows)
- Fallback: encrypted config file section (user-provided passphrase)
- API keys are never logged, never included in exports, never displayed in UI

### 22.3 Backend Communication

- All remote backend communication uses **TLS** when connecting to non-localhost endpoints
- Localhost connections (127.0.0.1, ::1) may use plain HTTP
- No authentication assumed for local backends (user's responsibility to secure)

### 22.4 File Permissions

- SQLite databases: `0600` (owner read/write only)
- Config files: `0600`
- Session directories: `0700`
- Export files: `0644` (readable, since explicitly shared)

### 22.5 Data at Rest

- SQLite databases are **not encrypted by default** (performance tradeoff)
- Optional `SQLCipher` support for users who require encryption (compile-time feature flag)
- Documented that SSH operation should use encrypted tunnels

*[Sources: GLM 5.1 — untrusted input, lorebook scoping, API key storage. MiMo V2 Pro — SecurityLevel enum. Qwen 3.6 Plus — schema validation.]*

---

## 23. Reliability, Debugging, & Transparency

### 23.1 Event Sourcing

Important actions emit **structured, append-only events**. This aligns naturally with the canonical transcript philosophy and enables deterministic replay.

```rust
enum OzoneEvent {
    MessageCommitted { message_id: MessageId, branch_id: BranchId },
    BranchCreated { branch_id: BranchId, forked_from: MessageId },
    SwipeActivated { swipe_group_id: SwipeGroupId, ordinal: u16 },
    ContextPlanGenerated { plan_id: ContextPlanId, is_dry_run: bool },
    RetrievalCandidatesRanked { count: usize, top_score: f32 },
    BackgroundJobCompleted { job_type: String, duration_ms: u64 },
    BackgroundJobFailed { job_type: String, error: String },
    UtilitySuggestionIgnored { suggestion_type: String },
    StaleArtifactDetected { artifact_id: MemoryArtifactId },
    DegradationStateChanged { subsystem: String, degraded: bool },
}
```

Events are stored in an append-only `events` table in SQLite. They enable: debugging ("what happened before this bad generation?"), replay for testing, and export for analysis.

*[Source: Qwen 3.6 Plus — Event Sourcing]*

### 23.2 Reproducibility
Given transcript state, selected branch, config snapshot, context plan, and backend parameters — the system can explain or approximately reproduce the generation setup for any turn. The enhanced `GenerationRecord` (§11.4) stores all necessary parameters.

### 23.3 Proposal vs Commit Distinction
Every assistive output is classified as: proposal → accepted proposal → committed state → derived artifact.

### 23.4 No Invisible Mutation
Background jobs must never silently rewrite active conversational state.

---

## 24. Performance Strategy (Concrete Targets)

### 24.1 Performance Philosophy
Gains come from: low UI overhead, efficient prompt assembly, bounded background work, cached derivations, and graceful fallback — not fragile intelligence chains.

### 24.2 Concrete Targets

| Metric | Target | Constraint |
|--------|--------|-----------|
| TUI frame time | **< 33ms** | 30 FPS minimum |
| Context assembly | **< 500ms** | 8K token budget, 1000-message session |
| Per-message render | **< 10ms** | Single message formatting |
| Cold startup | **< 2s** | 10 sessions in data directory |
| Memory usage | **< 200MB** | 10K-message active session, no active generation |
| SQLite DB size | **< 500MB** | 100K-message session with all artifacts |
| Foreground overhead | **< 50ms** | Excluding model inference time |

*[Sources: GLM 5.1 — performance target table. MiMo V2 Pro — PerformanceBudget struct.]*

### 24.3 Job Priority Classes and Backpressure

| Priority | Class | Examples |
|----------|-------|---------|
| P0 | Foreground-critical | Stream completion, cancel generation, load session |
| P1 | Foreground-assistive | Context plan generation, speaker proposal |
| P2 | Background-low-latency | Embeddings, importance proposal, thinking summary |
| P3 | Background-batch | Chunk summarization, retrieval index rebuild, synopsis |
| P4 | Background-idle-only | Cleanup, compaction, export support |

**Backpressure limits:**
- Maximum concurrent background jobs: **3**
- Maximum job queue size: **20**
- Stale job auto-cancellation: **5 minutes**
- P2+ jobs only execute when no P0/P1 job is active
- P4 jobs only execute when application is idle (no user input for 30s)

*[Source: Trinity Large Thinking — concrete backpressure numbers]*

### 24.4 Caching
Cache: token counts, embeddings, prompt assembly fragments, utility proposals, session metadata. Use LRU eviction with configurable size limits.

---

## 25. Testing Strategy

### 25.1 Required Test Types

| Subsystem | Test Type | What It Validates |
|-----------|----------|-------------------|
| Conversation Engine | **Unit tests** | Message CRUD, branch creation, swipe activation, message ordering |
| Conversation Engine | **Integration tests** | Concurrent access, SQLite transactions, crash recovery |
| Context Assembler | **Property-based tests** | Budget invariants: `used_tokens ≤ budget`, hard context never dropped, deterministic output for same input |
| Context Assembler | **Snapshot tests** | Compare ContextPlan JSON across versions to catch silent budget shifts |
| Memory Engine | **Unit tests** | Retrieval scoring, artifact lifecycle, GC policies |
| Memory Engine | **Property-based tests** | Scores always in `[0, 1]`, ranking is deterministic |
| Inference Gateway | **Mock backend tests** | Fixed-token, latency-simulated responders for streaming, cancellation, backpressure |
| TUI | **Render tests** | `ratatui::backend::TestBackend` for layout verification |
| Stream Parser | **Fuzz tests** | Malformed think blocks, interrupted streams, mixed encoding |
| Persistence | **Migration tests** | Forward migration, failed migration rollback, schema version consistency |

### 25.2 CI Pipeline
- All tests run on every PR
- Property-based tests use at least 1000 cases
- Snapshot tests fail loudly on any diff (must be explicitly updated)
- Fuzz tests run nightly with extended iteration count

*[Sources: GLM 5.1, Qwen 3.6 Plus, MiMo V2 Pro — all proposed testing strategies.]*

---

## 26. Revised Roadmap

### Milestone 1: Reliable Core Chat
- Canonical conversation engine with SQLite persistence (WAL mode, migrations)
- Branch model with closure table
- Swipe system (enhanced: parent context reference, discarded swipes)
- Deterministic context assembler with data-driven layer policy
- Context inspector with dry-run mode
- Token counting with fallback chain
- TUI shell (Standard mode layout, keybindings, input model)
- Configuration system (TOML, layered hierarchy)
- Error taxonomy and degraded-state indicators
- Capability-aware backend layer (KoboldCpp + Ollama at minimum)
- Basic import/export (SillyTavern-compatible JSON format)
- Onboarding / first-run wizard

### Milestone 2: Deterministic Memory
- Pinned memory
- Hybrid retrieval (BM25 + vector) with configurable scoring
- Summary artifact generation
- Memory storage tiering and GC policies
- Stale artifact detection
- Retrieval browser
- Provenance labels and provenance decay
- Event sourcing (structured event log)

### Milestone 3: Assistive Layer
- Optional importance proposals
- Optional keyword extraction
- Optional thinking summaries (streaming parser)
- Optional retrieval recommendations
- Enhanced degraded-state indicators
- "Safe mode" toggle (disable all Tier B)

### Milestone 4: Group Chat Foundation
- Shared scene context
- Per-character cards
- User-directed turn control + mention detection
- Round robin mode with narrator toggle
- Speaker suggestion prototype
- Relationship hints

### Milestone 5: Advanced Scene Support
- Narrator as explicit system actor
- Relationship overlays
- Improved turn routing
- Multiple UI mode presets (Minimal, Standard, Developer)
- Swipe diff comparator

### Milestone 6: Adaptive Intelligence Experiments
- WASM plugin interface for Tier C
- Fine-tune evaluation
- Flywheel logging as opt-in
- Auto narrator experiments
- Per-character private memory experiments

### Milestone 7: Public Release
- Stable config, solid docs
- Import tooling (SillyTavern, Chub, others)
- Polished terminal UX
- Accessibility: `--plain` mode, high-contrast theme
- Security hardening: keychain integration, optional SQLCipher
- Performance benchmarking against targets
- Measured expansion based on real failures, not wishlist pressure

---

## 27. Technical Risks & Mitigations

| Risk | Mitigation in Design | Additional Mitigation |
|------|---------------------|----------------------|
| **Intelligence sprawl** | Tiered scope, proposal vs commit, WASM for Tier C | Feature flags for all Tier B/C features |
| **Schema churn** | Separated data models, versioned migrations | Backup-before-migrate policy |
| **Retrieval drift** | Preserve transcript, provenance tracking, provenance decay | User-editable memory artifacts, stale detection |
| **Group chat explosion** | Phased rollout, shared context first | Enhanced Phase 1 MVP with narrator toggle |
| **Backend mismatch** | Capability-based abstraction, fallback chains | Graceful degradation with user-visible indicators |
| **Premature fine-tuning** | Optional, late-stage, WASM plugins first | Prompt-based utilities validated before investment |
| **GPU contention** | GPU Mutex with foreground priority | CPU-only embeddings via fastembed-rs |
| **Token count inaccuracy** | Three-tier fallback chain with safety margin | Confidence tracking in ContextPlan |
| **Concurrency bugs** | Single-writer architecture, channel-based communication | No shared mutable state between subsystems |
| **Onboarding failure** | First-run wizard, sample character, quick help overlay | Progressive disclosure via UI modes |
| **Storage bloat** | Memory tiering, GC policies, compaction | Configurable thresholds, storage indicator in UI |
| **Prompt injection via imports** | Schema validation, soft-context scoping for lorebooks | Sandboxed WASM for any executable content |

---

## 28. Recommendations for First Implementation

### Build first (Milestone 1)
- Canonical conversation engine
- Branch model with closure table
- Swipe system
- Context assembler with data-driven policy and dry-run
- Context inspector
- Token counting fallback chain
- Pinned memory
- Stable TUI loop (Standard mode)
- Configuration system (TOML, validated)
- Error taxonomy
- Capability-aware backend layer
- Onboarding flow

### Build second (Milestone 2)
- Summary artifact generation
- Hybrid retrieval (BM25 + vector)
- Retrieval artifact viewer
- Retrieval scoring (configurable)
- Memory tiering and GC
- Event sourcing
- Optional importance scoring

### Build later (Milestones 3+)
- Speaker selection helpers
- Narrator policies
- Private character memory scopes
- WASM plugin interface
- Fine-tuned utility model
- Adaptive correction flywheel

### Explicit anti-goals for early versions
- Full hidden-intelligence orchestration
- Automatic narrator authority
- Complex auto-world-building logic
- Depending on custom training to make the product viable
- Encryption by default (optional feature, not blocking)
- Full accessibility compliance (progressive improvement)

---

## 29. Closing Direction

Ozone already has a compelling identity. The key improvement from v0.2 to v0.3 is **implementation specificity**: every referenced type is defined, every concurrency decision is made, every subsystem has testable targets, and every UX element has a concrete specification.

The strongest version of Ozone is not the version with the most automated cleverness. It is the version that:
- remains lightweight
- stays stable under constrained hardware
- makes context and memory legible
- supports excellent roleplay
- behaves predictably
- can gracefully grow into more intelligence over time

The project proceeds with this principle:

**Build a trustworthy conversation engine first.
Layer intelligence on top only where it clearly improves roleplay without compromising clarity.**

That approach will produce a better product, a better codebase, and a much stronger foundation for every later experiment.

---

## Appendix A: Attribution

Key improvements in this document are attributed to their source analyses:

| Improvement | Source |
|-------------|--------|
| Context Sandbox / Dry-Run | Gemini 3 Flash |
| WASM plugin interface for Tier C | Gemini 3 Flash |
| BM25 + Vector hybrid retrieval | Gemini 3 Flash |
| GPU Mutex for single-GPU systems | Gemini 3 Flash |
| fastembed-rs for CPU embeddings | Gemini 3 Flash |
| Vim-diff swipe comparator | Gemini 3 Flash |
| Closure Table for branching | GLM 5.1 |
| SwipeGroup references parent context | GLM 5.1 |
| Full reproducibility in GenerationRecord | GLM 5.1 |
| Data-driven context assembly policy | GLM 5.1 |
| One SQLite DB per session | GLM 5.1 |
| SillyTavern-compatible export | GLM 5.1 |
| Complete type definitions (all missing types) | GLM 5.1 |
| Three-tier color system | GLM 5.1 |
| Schema versioning and migration strategy | GLM 5.1 |
| Simplified default context assembly | Trinity Large Thinking |
| Memory storage tiering | Trinity Large Thinking |
| Enhanced Group Chat Phase 1 MVP | Trinity Large Thinking |
| Thinking block as explicit opt-in | Trinity Large Thinking |
| Concrete backpressure numbers | Trinity Large Thinking |
| Multiple UI modes (Minimal/Standard/Developer) | Trinity Large Thinking |
| Safe mode toggle | Trinity Large Thinking |
| SQLite FTS for keyword retrieval | Trinity Large Thinking |
| Event sourcing | Qwen 3.6 Plus |
| usearch for disk-backed vector storage | Qwen 3.6 Plus |
| Provenance decay | Qwen 3.6 Plus |
| StaleArtifactDetector | Qwen 3.6 Plus |
| Explicit garbage collection policies | Qwen 3.6 Plus |
| Streaming think-block parser (nom) | Qwen 3.6 Plus |
| Background compaction | Qwen 3.6 Plus |
| Terminal width-aware wrapping | Qwen 3.6 Plus |
| `--plain` accessibility mode | Qwen 3.6 Plus |
| Channel-based architecture + immutable snapshots | Qwen 3.6 Plus |
| Input mode state machine | MiMo V2 Pro |
| GenerationState enum / cancellation contract | MiMo V2 Pro |
| Workspace crate structure | MiMo V2 Pro |
| PersistenceLayer trait | MiMo V2 Pro |
| TokenEstimationPolicy enum | MiMo V2 Pro |
| SecurityLevel model | MiMo V2 Pro |
| Hot-reload vs. immutable config distinction | MiMo V2 Pro |
| Error taxonomy with severity + retry policy | MiMo V2 Pro |
| Ownership boundary table | MiMo V2 Pro |
| Dependency selection (ratatui, tokio, rusqlite, crossterm) | Consensus (4+ models) |
| Configurable retrieval weights | Consensus (5/5 models) |
| Concurrency model specification | Consensus (5/5 models) |
| Performance targets with concrete numbers | Consensus (3+ models) |
| Testing strategy | Consensus (3+ models) |

---

Ozone v0.3 design document complete.

# Ozone+ v0.4 — Baseline Design for the Full Local-LLM Pipeline

**Product:** ozone+  
**Version:** 0.4  
**Updated:** 2025-07-14  
**Based on:** v0.3 with Round 2 cross-LLM synthesis (GLM 5.1, Qwen 3.6 Plus, Trinity Large, MiMo V2 Pro, Gemini 3.1 Pro)

**Product-family note:** This document now serves as the **ozone+ baseline design**. Earlier design iterations used **Ozone** as the umbrella product name. In this document, references to **Ozone** should be read as **ozone+** unless a broader family-level distinction is being made elsewhere.

**Companion docs:** Start with `README.md` for the family overview, `ozone_plus_documentation_stack.md` for ozone+ document routing, and `compatibility_and_migration.md` for family-level portability and upgrade direction.

---

## What Changed in v0.4

**Critical fixes (6):**
1. Added prompt formatting templates — ChatML, Alpaca, Llama-3-Instruct via `minijinja` *(Gemini)*
2. Specified `tracing` + `tracing-subscriber` logging framework *(GLM, MiMo)*
3. Added streaming error recovery with `GenerationState::FailedMidStream` *(GLM)*
4. Specified character card schema — SillyTavern V2, required fields, validation *(GLM, Trinity, MiMo)*
5. Specified session export format with field mapping and lossy conversion rules *(MiMo, Trinity)*
6. Clarified derived artifact write path — routed through engine channel *(MiMo, Qwen, Gemini)*

**Schema fixes (4):**
7. Timestamps use `INTEGER` Unix epoch UTC *(GLM)*
8. Added FTS5 synchronization triggers *(GLM)*
9. Added advisory `session_lock` table *(GLM)*
10. Added events retention policy *(GLM, Qwen)*

**Corrected claims (3):**
11. Config merging uses `config` crate deep merge, not raw serde *(Qwen)*
12. Streaming parser uses `tokio-util::codec::Decoder`, not `nom` *(Qwen)*
13. Memory tiering explicit: derived artifacts only, transcript sacred *(MiMo)*

**Design decisions resolved (4):**
- Closure table: **KEEP** with explicit maintenance contract *(GLM)* — CTEs too slow for deep trees
- Memory complexity: **DEFER** hybrid retrieval to Phase 2 *(Qwen, Trinity)* — Phase 1 uses FTS5 only
- Group Chat Phase 1: **Explicit-only** control *(Trinity)* — defer mention detection
- Hard context overflow: hard items **exceed** max_pct, compress soft context

**Milestones restructured:** 7 coarse milestones → 13 independently testable phases *(Qwen, Trinity)*

---

## 1. Design Goal

Build the **ozone+** edition as a terminal-native frontend and workflow layer for local LLMs that:
- Works correctly with all intelligence disabled (deterministic core)
- Layers assistive features that can be toggled without breaking the core
- Makes context assembly and memory visible and controllable
- Runs efficiently on consumer hardware (single GPU, 16GB RAM)
- Operates via SSH / tmux / mosh without degradation

ozone+ is not a general-purpose chat UI. It is purpose-built for long-form interactive fiction and roleplay with local language models.

---

## 2. Non-Goals

- Web UI or Electron wrapper
- Cloud-first or SaaS deployment
- Real-time multi-user collaboration
- Image generation or multi-modal inference (text-only; explicitly out of scope for v0.4)
- Depending on custom training to make the product viable
- Full accessibility compliance (progressive improvement)
- Encryption by default (optional feature flag, not blocking)

---

## 3. Foundational Principles

### 3.1 Deterministic core, optional intelligence
Every feature that uses LLM inference beyond the primary chat completion is **optional** and **degradation-safe**. The application must function correctly with all intelligence features disabled.

### 3.2 Transparency over magic
Users must be able to see exactly what is sent to the model, why each token was spent, and what was excluded. The Context Inspector is the primary tool for this.

### 3.3 Canonical transcript is sacred
The original message history is **never** replaced, compressed, or pruned. Derived artifacts (summaries, embeddings, importance scores) are stored separately and may be regenerated, tiered, or garbage-collected. The `messages` table is append-only for insertions and supports only metadata updates (`edited_at`, soft-delete flag) — never content mutations that destroy the original.

**Clarification (v0.4):** Memory Storage Tiering (§16.6) applies **exclusively** to derived artifacts. The canonical `messages` table is never pruned, truncated, or archived regardless of session age or size.

### 3.4 Single-writer architecture
The `ConversationEngine` is the sole writer to canonical session state. All other subsystems communicate via typed channels. Background jobs that produce derived artifacts route writes through the engine's command channel (§6.4), maintaining the single-writer guarantee without coupling.

### 3.5 Graceful degradation, not silent failure
When a subsystem fails (backend unreachable, embedding model down, token count uncertain), the system continues operating with reduced capability and surfaces the degradation visibly in the UI.

### 3.6 Proposal → Commit distinction
Every assistive output follows: **proposal** → **accepted proposal** → **committed state** → **derived artifact**. The user always has the final say.

---

## 4. Scope Tiers

| Tier | Scope | Intelligence Level | Can Be Disabled? |
|------|-------|--------------------|-----------------|
| **A** | Core conversation, context, persistence, TUI, config | None (deterministic) | No — this IS the product |
| **B** | Memory retrieval, summaries, importance scoring, thinking summaries | Assistive (proposals only) | Yes — each independently |
| **C** | WASM plugins, fine-tuned utility models, adaptive flywheel | Experimental | Yes — feature-flagged |

**Rule:** Tier A never depends on Tier B. Tier B never depends on Tier C.

---

## 5. Technology Stack

### 5.1 Core Dependencies

| Crate | Purpose | Version Policy |
|-------|---------|---------------|
| `ratatui` | TUI rendering framework | Latest stable |
| `crossterm` | Terminal backend (cross-platform) | Latest stable |
| `tokio` | Async runtime (multi-threaded) | 1.x LTS |
| `rusqlite` | SQLite with WAL mode + FTS5 | Latest stable, `bundled` feature |
| `serde` + `serde_json` | Serialization | Latest stable |
| `toml` | TOML parsing | Latest stable |
| `config` | Layered config merging with deep merge | Latest stable |
| `reqwest` | HTTP client for backend APIs | Latest stable |
| `tracing` | Structured logging facade | Latest stable |
| `tracing-subscriber` | Log output (file, stderr, JSON) | Latest stable |
| `minijinja` | Prompt formatting templates | Latest stable |
| `arboard` | Cross-platform clipboard | Latest stable |
| `tokio-util` | Streaming codec (`Decoder` trait) | Latest stable |
| `uuid` | Unique identifiers | Latest stable, `v4` feature |
| `chrono` | Time handling (UTC) | Latest stable |
| `thiserror` | Error derive macros | Latest stable |
| `keyring` | OS keychain for credential storage | Latest stable |

### 5.2 Memory / Retrieval Dependencies (Phase 2+)

| Crate | Purpose |
|-------|---------|
| `fastembed-rs` | CPU-only embeddings (no GPU contention) |
| `usearch` | Disk-backed vector index |

### 5.3 Optional / Feature-Flagged

| Crate | Purpose | Feature Flag |
|-------|---------|-------------|
| `sqlcipher` | Encrypted SQLite | `encryption` |
| `notify-rust` | Desktop notifications | `desktop-notify` |

### 5.4 Workspace Structure

```
ozone/
├── Cargo.toml              # workspace root
├── crates/
│   ├── ozone-core/         # types, errors, traits, protocols
│   ├── ozone-persist/      # SQLite persistence, migrations, global index
│   ├── ozone-engine/       # ConversationEngine, context assembly
│   ├── ozone-inference/    # Backend gateway, streaming, prompt templates
│   ├── ozone-memory/       # Retrieval, embeddings, summaries (Phase 2+)
│   ├── ozone-tui/          # ratatui UI, input handling, themes
│   └── ozone-cli/          # CLI entrypoint, config loading, onboarding
├── templates/              # minijinja prompt format templates
├── tests/                  # integration tests, property-based tests
└── docs/                   # user documentation
```

*[Source: MiMo V2 Pro — workspace crate structure]*

---

## 6. Concurrency Model

### 6.1 Channel-Based Architecture

```
                    ┌──────────────────────────┐
                    │     TUI Event Loop        │
                    │  (crossterm + ratatui)     │
                    └──────┬───────────┬────────┘
                           │ Command   │ reads Arc<AppState>
                           ▼           │
                    ┌──────────────────┴────────┐
                    │   ConversationEngine       │
                    │   (single-writer, owns DB) │
                    │   - processes Commands     │
                    │   - emits Events           │
                    │   - serializes ALL writes  │
                    └──────┬───────────┬────────┘
                           │ Event     ▲ Command::PersistArtifact
                           ▼           │
                    ┌──────────────┐   │
                    │  Broadcast   │   │
                    │  (fan-out)   │   │
                    └──┬───┬───┬──┘   │
                       │   │   │      │
                  ┌────┘   │   └────┐ │
                  ▼        ▼        ▼ │
              ┌──────┐ ┌──────┐ ┌──────────────┐
              │ TUI  │ │Logger│ │ Background   │
              │update│ │      │ │ Jobs (P2-P4) │
              └──────┘ └──────┘ └──────────────┘
```

### 6.2 Communication Channels

| Channel | Type | Direction | Purpose |
|---------|------|-----------|---------|
| `command_tx` | `mpsc<Command>` | TUI/Jobs → Engine | All state mutations |
| `event_tx` | `broadcast<OzoneEvent>` | Engine → All | State change notifications |
| `inference_tx` | `mpsc<InferenceRequest>` | Engine → Gateway | Generation requests |
| `stream_tx` | `mpsc<StreamChunk>` | Gateway → TUI | Token-by-token streaming |

**Channel capacity limits:**
- `command_tx`: bounded(256) — backpressure on command submission
- `event_tx`: bounded(1024) — events are cheap, lag means drop
- `inference_tx`: bounded(4) — max concurrent inference requests
- `stream_tx`: bounded(64) — per-stream, flow-controlled

### 6.3 Ownership Boundary Table

| Data | Owner (Writer) | Readers | Access Pattern |
|------|---------------|---------|---------------|
| Canonical transcript | ConversationEngine | TUI, Memory, Export | Engine writes; readers use snapshots |
| Derived artifacts | ConversationEngine (via `Command::PersistArtifact`) | Memory, Context Assembler | Background jobs produce → engine persists |
| Context plans | Context Assembler (stateless, read-only DB access) | TUI, Logger | Assembler reads, produces plan |
| Active generation state | Inference Gateway | TUI (streaming display) | Gateway owns stream lifecycle |
| UI state | TUI | TUI only | Never persisted to DB |
| Configuration | Config loader (startup) | All subsystems | Immutable after load (hot-reload replaces atomically) |

### 6.4 Background Job Write Path (Resolved)

Background jobs (embedding generation, summary creation, importance scoring) produce derived artifacts but **do not write to SQLite directly**. Instead:

1. Job completes, produces artifact data
2. Job sends `Command::PersistArtifact { kind, data, session_id, snapshot_version }` through `command_tx`
3. Engine receives command, checks `snapshot_version` against current transcript version
4. If versions match: engine writes artifact to DB in its transaction
5. If versions diverge: engine discards artifact, logs warning, optionally re-queues job

This maintains the single-writer guarantee cleanly.

*[Source: MiMo V2 Pro — snapshot versioning. Qwen, Gemini — route through engine.]*

### 6.5 Hardware Resource Semaphore

```rust
struct HardwareResourceSemaphore {
    permits: tokio::sync::Semaphore,
    capacity: usize,  // default: 1, configurable for multi-GPU
}
```

Foreground inference (P0) acquires permits with priority. Background jobs (P2+) yield if a foreground request is waiting. Multi-GPU users can increase capacity.

*[Source: Gemini 3.1 Pro — HardwareResourceSemaphore with configurable capacity]*

---

## 7. Session Model

One **SQLite database per session**. Sessions are self-contained, portable, and independently backupable.

### 7.1 File Organization

```
$XDG_DATA_HOME/ozone/
├── global.db                     # Cross-session index (§13.6)
├── sessions/
│   ├── <session_uuid>/
│   │   ├── session.db            # Canonical transcript + artifacts
│   │   ├── session.db-wal        # WAL file (auto-managed)
│   │   ├── config.toml           # Per-session config overrides
│   │   ├── draft.txt             # Input buffer auto-save (debounced)
│   │   ├── vectors/              # usearch index files (Phase 2+)
│   │   └── attachments/          # Future: file attachments
│   └── templates/                # Session templates (character + config + lorebook)
├── characters/                   # Character card JSON files
├── lorebooks/                    # Global lorebook JSON files
└── logs/                         # Structured log files (rotated)
```

### 7.2 SQLite Configuration

```sql
PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;
PRAGMA foreign_keys = ON;
PRAGMA busy_timeout = 5000;
PRAGMA cache_size = -8000;  -- 8MB cache
PRAGMA wal_autocheckpoint = 1000;
```

---

## 8. Branch Model

### 8.1 Conceptual Model
A conversation is a **tree of messages**. A branch is a **named bookmark** pointing to a leaf node. The active branch determines which path through the tree is displayed.

### 8.2 Closure Table
The `message_ancestry` closure table pre-computes all ancestor-descendant relationships for O(1) depth queries.

**Maintenance contract:** Closure table rows are managed **exclusively** by `ConversationEngine` within the same transaction as message insertion. Direct SQL mutation outside the engine is unsupported and will corrupt ancestry data.

```sql
-- On message insert (within engine transaction):
INSERT INTO message_ancestry (ancestor_id, descendant_id, depth)
SELECT ancestor_id, NEW.message_id, depth + 1
FROM message_ancestry
WHERE descendant_id = NEW.parent_id
UNION ALL
SELECT NEW.message_id, NEW.message_id, 0;
```

For sessions exceeding 50K messages, a recursive CTE fallback is available if closure table performance degrades.

*[Source: GLM 5.1 — closure table + contract. Qwen 3.6 Plus — CTE fallback.]*

### 8.3 Branch States

```rust
enum BranchState {
    Active,       // Currently selected branch
    Inactive,     // Valid branch, not currently selected
    Archived,     // Hidden from default branch list, data preserved
    Deleted,      // Soft-deleted, eligible for cleanup after grace period
}
```

Branch deletion is soft-delete with configurable grace period (default: 7 days). Archived branches are excluded from default listings but accessible via `:branches --all`.

---

## 9. Swipe System

### 9.1 Data Model

```rust
struct SwipeGroup {
    id: SwipeGroupId,
    parent_message_id: MessageId,
    parent_context_message_id: MessageId,
    candidates: Vec<SwipeCandidate>,
    active_ordinal: u16,
}

struct SwipeCandidate {
    ordinal: u16,
    message_id: MessageId,
    generation_record: Option<GenerationRecord>,
    state: SwipeCandidateState,
}

enum SwipeCandidateState {
    Active,
    Discarded,
    FailedMidStream { partial_content: String, tokens_generated: usize },
}
```

### 9.2 Numeric Swipe Shortcuts
When multiple swipe candidates exist, the TUI shows numbered options:
```
[1] Elara: "The forest path twisted..."  ← active
[2] Elara: "Ancient oaks formed..."
[3] Elara: "A chill ran down..."
```
Press `1`–`9` to select directly, or `Ctrl+→`/`Ctrl+←` to cycle.

*[Source: Trinity Large — numeric swipe shortcuts]*

---

## 10. Generation Lifecycle

### 10.1 Generation States

```rust
enum GenerationState {
    Idle,
    Queued { request_id: RequestId },
    Streaming { request_id: RequestId, tokens_so_far: usize },
    Completed { message_id: MessageId, tokens_generated: usize, duration_ms: u64 },
    Cancelled { partial_content: Option<String>, tokens_generated: usize, reason: CancelReason },
    Failed { error: OzoneError },
    FailedMidStream {
        partial_content: String,
        tokens_generated: usize,
        error: OzoneError,
    },
}

enum CancelReason {
    UserRequested,
    BackpressureTimeout,
    BackendDisconnect,
    RateLimited,
}
```

### 10.2 Streaming Error Recovery

When a backend crashes or disconnects mid-stream:

1. The partial output is preserved as a `SwipeCandidate` with state `FailedMidStream`
2. The TUI shows the partial content with a `⚠ incomplete` indicator
3. The user can: (a) retry from scratch, (b) continue from partial (if backend supports), (c) accept partial as-is, (d) discard
4. The `GenerationRecord` logs the failure for debugging

*[Source: GLM 5.1 — streaming error recovery with partial preservation]*

### 10.3 Generation Record

```rust
struct GenerationRecord {
    request_id: RequestId,
    backend_id: BackendId,
    model_name: String,
    prompt_template_id: String,
    sampling_params: SamplingParameters,
    context_plan_id: ContextPlanId,
    token_count_method: TokenEstimationPolicy,
    started_at: i64,           // Unix epoch UTC ms
    completed_at: Option<i64>,
    tokens_generated: usize,
    generation_state: GenerationState,
    streaming_format: StreamingFormat,
}
```

### 10.4 Rate Limiting

```rust
struct RateLimitPolicy {
    min_interval_ms: u64,          // default: 500ms between requests
    max_pending_requests: usize,   // default: 2
    retry_after_429_ms: u64,       // default: respect server header, fallback 5000ms
    backoff_multiplier: f32,       // default: 1.5
    max_backoff_ms: u64,           // default: 30000
}
```

Rapid regeneration (Ctrl+R spam) is coalesced — only the last request within `min_interval_ms` fires. Remote API 429 responses use the server's `Retry-After` header.

*[Source: GLM 5.1, MiMo V2 Pro — rate limiting]*

### 10.5 Mid-Session Model Switching

When the active model changes between turns (user switches backend or model):

1. Emit `OzoneEvent::ModelContextChanged { old_model, new_model }`
2. Invalidate token count cache (tokenizer may differ)
3. Recalculate context budget (context length may differ)
4. Switch prompt template (format may differ)
5. Warn user if new model has significantly smaller context
6. The `GenerationRecord` for each turn records which model was used, enabling accurate replay

---

## 11. Core Type Definitions

### 11.1 Identifiers

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct MessageId(Uuid);
struct BranchId(Uuid);
struct SessionId(Uuid);
struct SwipeGroupId(Uuid);
struct MemoryArtifactId(Uuid);
struct ContextPlanId(Uuid);
struct RequestId(Uuid);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct BackendId(String);
```

### 11.2 Message

```rust
struct Message {
    id: MessageId,
    session_id: SessionId,
    parent_id: Option<MessageId>,
    author: AuthorId,
    content: String,
    created_at: i64,           // Unix epoch UTC (milliseconds)
    edited_at: Option<i64>,    // Unix epoch UTC (milliseconds)
    is_hidden: bool,
    metadata: MessageMetadata,
}

enum AuthorId {
    User,
    Character(String),
    System,
    Narrator,
}

struct MessageMetadata {
    token_count: Option<usize>,
    token_count_method: TokenEstimationPolicy,
    generation_record: Option<GenerationRecord>,
    thinking_block: Option<ThinkingBlock>,
    bookmarked: bool,
    bookmark_note: Option<String>,
}
```

### 11.3 Branch

```rust
struct Branch {
    id: BranchId,
    session_id: SessionId,
    name: String,
    tip_message_id: MessageId,
    created_at: i64,
    state: BranchState,
    description: Option<String>,
}
```

### 11.4 Memory Artifacts

```rust
struct MemoryArtifact {
    id: MemoryArtifactId,
    session_id: SessionId,
    kind: MemoryContent,
    source_message_range: Option<(MessageId, MessageId)>,
    provenance: Provenance,
    created_at: i64,
    snapshot_version: u64,  // transcript version when artifact was created
}

enum MemoryContent {
    ChunkSummary { text: String, source_count: usize },
    SessionSynopsis { text: String },
    BranchSynopsis { branch_id: BranchId, text: String },
    Embedding { vector: Vec<f32>, source_text_hash: u64 },
    ImportanceProposal { score: f32, justification: String },
    RetrievalKey { keywords: Vec<String> },
    PinnedMemory { text: String, pinned_by: AuthorId, expires_after_turns: Option<u32> },
}

enum Provenance {
    UserAuthored,
    CharacterCard,
    Lorebook,
    SystemGenerated,
    UtilityModel,
    ImportedExternal,
}
```

### 11.5 Provenance Weights (Configurable)

```rust
struct ProvenanceWeights {
    user_authored: f32,       // default: 1.0
    character_card: f32,      // default: 0.9
    lorebook: f32,            // default: 0.85
    system_generated: f32,    // default: 0.7
    utility_model: f32,       // default: 0.6
    imported_external: f32,   // default: 0.5
}
```

All weights configurable via `[memory.provenance_weights]` in config.toml.

*[Source: GLM 5.1, MiMo V2 Pro — configurable provenance weights]*

### 11.6 Context Policy Types

```rust
struct ContextLayerPolicy {
    layers: Vec<ContextLayer>,
}

struct ContextLayer {
    kind: ContextLayerKind,
    priority: u8,
    min_budget_pct: f32,
    max_budget_pct: f32,
    is_hard_context: bool,
    collapse_strategy: CollapseStrategy,
}

enum ContextLayerKind {
    SystemPrompt,
    CharacterCard,
    PinnedMemory,
    RecentMessages,
    RetrievedMemory,
    LorebookEntries,
    ThinkingSummary,
    SessionSynopsis,
}

enum CollapseStrategy {
    TruncateTail,
    TruncateHead,
    OmitOldest,
    Summarize,
    Never,  // hard context — error if exceeded
}
```

**Hard Context Overflow Policy:** Hard context items (`is_hard_context: true`) that exceed their `max_budget_pct` are **still included**. The assembler first reserves `min_budget_pct` for each hard item, then distributes remaining budget. If a hard item (e.g., a 400-token character card on a 2K budget) exceeds its max percentage, soft context items are compressed to make room. Hard items can exceed `max_budget_pct`; soft items cannot.

### 11.7 Token Estimation

```rust
enum TokenEstimationPolicy {
    Exact { tokenizer: String },
    Approximate { model_family: String, calibration_ratio: f32 },
    Heuristic { chars_per_token: f32, language_hint: Option<String> },
}
```

**Per-language calibration:**

| Language Family | chars_per_token | Safety Margin |
|----------------|-----------------|---------------|
| Latin/Roman | 4.0 | 10% |
| CJK | 1.5 | 20% |
| Cyrillic | 3.0 | 15% |
| Arabic/Hebrew | 2.5 | 15% |
| Mixed/Unknown | 3.0 | 20% |

Safety margins are configurable per estimation tier: Exact (0%), Approximate (5–10%), Heuristic (10–25%).

*[Source: GLM, Qwen, MiMo — per-language calibration]*

### 11.8 Streaming Format

```rust
enum StreamingFormat {
    ServerSentEvents,  // KoboldCpp, OpenAI-compatible
    JSONLines,         // Ollama
    Chunked,           // Raw HTTP chunked transfer
}
```

Detected via capability probing at startup or specified in backend config.

*[Source: Trinity Large — StreamingFormat capability enum]*

### 11.9 Lorebook Entry

```rust
struct LorebookEntry {
    id: Uuid,
    name: String,
    keywords: Vec<String>,
    content: String,
    enabled: bool,
    case_sensitive: bool,
    match_whole_word: bool,
    insertion_position: LorebookInsertionPosition,
    priority: u16,
    max_tokens: Option<usize>,
    comment: Option<String>,
}

enum LorebookInsertionPosition {
    BeforeCharacterCard,
    AfterCharacterCard,
    BeforeRecentMessages,
    AfterRecentMessages,
}
```

**Matching:** Substring search against last N messages (configurable, default: 10). Per-entry `case_sensitive` and `match_whole_word`. No regex in v0.4 (complexity vs value).

**Security:** Lorebook entries are **soft context only** — cannot override hard context. Prevents prompt injection via imported lorebooks.

---

## 12. Error Taxonomy

```rust
#[derive(Debug, thiserror::Error)]
enum OzoneError {
    // Persistence
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("Migration failed: {version} -> {target}: {reason}")]
    MigrationFailed { version: u32, target: u32, reason: String },
    #[error("Session locked by instance {instance_id} (since {acquired_at})")]
    SessionLocked { instance_id: String, acquired_at: i64 },

    // Inference
    #[error("Backend unreachable: {backend_id}: {reason}")]
    BackendUnreachable { backend_id: String, reason: String },
    #[error("Generation failed: {0}")]
    GenerationFailed(String),
    #[error("Stream interrupted after {tokens_generated} tokens: {reason}")]
    StreamInterrupted { tokens_generated: usize, reason: String },
    #[error("Rate limited: retry after {retry_after_ms}ms")]
    RateLimited { retry_after_ms: u64 },
    #[error("Prompt template error: {template}: {reason}")]
    PromptTemplate { template: String, reason: String },

    // Context Assembly
    #[error("Token budget exceeded: {used}/{budget}")]
    BudgetExceeded { used: usize, budget: usize },
    #[error("Hard context overflow: {layer} requires {needed}, budget allows {available}")]
    HardContextOverflow { layer: String, needed: usize, available: usize },

    // Configuration
    #[error("Config invalid: {key}: {reason}")]
    ConfigInvalid { key: String, reason: String },

    // Import/Export
    #[error("Character card invalid: {reason}")]
    CharacterCardInvalid { reason: String },
    #[error("Import failed: {format}: {reason}")]
    ImportFailed { format: String, reason: String },

    // Memory
    #[error("Embedding failed: {reason}")]
    EmbeddingFailed { reason: String },
    #[error("Vector index corrupt: {reason}")]
    VectorIndexCorrupt { reason: String },

    // General
    #[error("Internal error: {0}")]
    Internal(String),
}

impl OzoneError {
    fn severity(&self) -> ErrorSeverity { /* ... */ }
    fn user_visibility(&self) -> UserVisibility { /* ... */ }
    fn retry_policy(&self) -> RetryPolicy { /* ... */ }
}

enum ErrorSeverity { Fatal, Error, Warning, Info }
enum UserVisibility { Show, StatusBar, Log }
enum RetryPolicy { NoRetry, RetryImmediate, RetryWithBackoff { initial_ms: u64, max_retries: u32 }, UserDecision }
```

*[Source: MiMo V2 Pro — error taxonomy with severity + retry policy]*

---

## 13. Persistence Layer

### 13.1 Schema Version

```sql
CREATE TABLE schema_version (
    version INTEGER NOT NULL,
    applied_at INTEGER NOT NULL,  -- Unix epoch UTC ms
    description TEXT
);
```

Migrations are forward-only. Each migration is wrapped in a transaction. Before any migration, the engine creates an automatic backup: `session.db.bak.<version>`.

### 13.2 Session Lock (Advisory)

```sql
CREATE TABLE session_lock (
    id INTEGER PRIMARY KEY CHECK (id = 1),  -- singleton row
    instance_id TEXT NOT NULL,               -- UUID of owning Ozone instance
    acquired_at INTEGER NOT NULL,            -- Unix epoch UTC ms
    heartbeat_at INTEGER NOT NULL            -- Unix epoch UTC ms, updated every 30s
);
```

On session open:
1. Check if lock exists and `heartbeat_at` is within last 60 seconds
2. If stale (heartbeat > 60s old): take over the lock, log warning
3. If active: return `OzoneError::SessionLocked` with instance info
4. If no lock: insert lock row

A background task updates `heartbeat_at` every 30 seconds. On clean shutdown, the lock row is deleted.

*[Source: GLM 5.1 — advisory session lock, "50 lines of code"]*

### 13.3 Core Tables

```sql
CREATE TABLE messages (
    message_id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    parent_id TEXT REFERENCES messages(message_id),
    author_kind TEXT NOT NULL,         -- 'user', 'character', 'system', 'narrator'
    author_name TEXT,                  -- character name if author_kind='character'
    content TEXT NOT NULL,
    created_at INTEGER NOT NULL,       -- Unix epoch UTC ms
    edited_at INTEGER,                 -- Unix epoch UTC ms
    is_hidden INTEGER NOT NULL DEFAULT 0,
    token_count INTEGER,
    token_count_method TEXT,
    generation_record_json TEXT,
    thinking_block_json TEXT,
    bookmarked INTEGER NOT NULL DEFAULT 0,
    bookmark_note TEXT
);

CREATE TABLE branches (
    branch_id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    name TEXT NOT NULL,
    tip_message_id TEXT NOT NULL REFERENCES messages(message_id),
    created_at INTEGER NOT NULL,
    state TEXT NOT NULL DEFAULT 'inactive',  -- active, inactive, archived, deleted
    description TEXT
);

CREATE TABLE message_ancestry (
    ancestor_id TEXT NOT NULL REFERENCES messages(message_id),
    descendant_id TEXT NOT NULL REFERENCES messages(message_id),
    depth INTEGER NOT NULL,
    PRIMARY KEY (ancestor_id, descendant_id)
);

CREATE TABLE swipe_groups (
    swipe_group_id TEXT PRIMARY KEY,
    parent_message_id TEXT NOT NULL REFERENCES messages(message_id),
    parent_context_message_id TEXT REFERENCES messages(message_id),
    active_ordinal INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE swipe_candidates (
    swipe_group_id TEXT NOT NULL REFERENCES swipe_groups(swipe_group_id),
    ordinal INTEGER NOT NULL,
    message_id TEXT NOT NULL REFERENCES messages(message_id),
    state TEXT NOT NULL DEFAULT 'active',  -- active, discarded, failed_mid_stream
    partial_content TEXT,                  -- for failed_mid_stream
    tokens_generated INTEGER,
    PRIMARY KEY (swipe_group_id, ordinal)
);

CREATE TABLE memory_artifacts (
    artifact_id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    content_json TEXT NOT NULL,
    source_start_message_id TEXT,
    source_end_message_id TEXT,
    provenance TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    snapshot_version INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE bookmarks (
    bookmark_id TEXT PRIMARY KEY,
    message_id TEXT NOT NULL REFERENCES messages(message_id),
    note TEXT,
    created_at INTEGER NOT NULL
);

CREATE TABLE context_plans (
    plan_id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    branch_id TEXT NOT NULL,
    is_dry_run INTEGER NOT NULL DEFAULT 0,
    plan_json TEXT NOT NULL,
    total_tokens INTEGER NOT NULL,
    budget INTEGER NOT NULL,
    created_at INTEGER NOT NULL
);
```

**Timestamps:** All timestamps are `INTEGER` storing Unix epoch UTC in milliseconds. No timezone ambiguity, deterministic ordering across DST boundaries.

*[Source: GLM 5.1 — UTC integer timestamps]*

### 13.4 Events Table (with Retention)

```sql
CREATE TABLE events (
    event_id INTEGER PRIMARY KEY AUTOINCREMENT,
    event_type TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    created_at INTEGER NOT NULL
);

CREATE INDEX idx_events_type ON events(event_type);
CREATE INDEX idx_events_created ON events(created_at);
```

**Retention policy:**
- Default: keep events for 90 days (`max_event_age_days` in config)
- P4 background job runs compaction: delete events older than threshold
- Export events before deletion if `events.export_before_compaction = true`

*[Source: GLM 5.1, Qwen 3.6 Plus — events retention policy]*

### 13.5 FTS5 Full-Text Search (with Triggers)

```sql
CREATE VIRTUAL TABLE messages_fts USING fts5(
    content,
    content=messages,
    content_rowid=rowid
);

CREATE VIRTUAL TABLE artifacts_fts USING fts5(
    content_json,
    content=memory_artifacts,
    content_rowid=rowid
);

-- Synchronization triggers (REQUIRED for content-sync FTS5)
CREATE TRIGGER messages_fts_insert AFTER INSERT ON messages BEGIN
    INSERT INTO messages_fts(rowid, content) VALUES (NEW.rowid, NEW.content);
END;

CREATE TRIGGER messages_fts_update AFTER UPDATE OF content ON messages BEGIN
    INSERT INTO messages_fts(messages_fts, rowid, content) VALUES ('delete', OLD.rowid, OLD.content);
    INSERT INTO messages_fts(rowid, content) VALUES (NEW.rowid, NEW.content);
END;

CREATE TRIGGER messages_fts_delete AFTER DELETE ON messages BEGIN
    INSERT INTO messages_fts(messages_fts, rowid, content) VALUES ('delete', OLD.rowid, OLD.content);
END;

CREATE TRIGGER artifacts_fts_insert AFTER INSERT ON memory_artifacts BEGIN
    INSERT INTO artifacts_fts(rowid, content_json) VALUES (NEW.rowid, NEW.content_json);
END;

CREATE TRIGGER artifacts_fts_update AFTER UPDATE OF content_json ON memory_artifacts BEGIN
    INSERT INTO artifacts_fts(artifacts_fts, rowid, content_json) VALUES ('delete', OLD.rowid, OLD.content_json);
    INSERT INTO artifacts_fts(rowid, content_json) VALUES (NEW.rowid, NEW.content_json);
END;

CREATE TRIGGER artifacts_fts_delete AFTER DELETE ON memory_artifacts BEGIN
    INSERT INTO artifacts_fts(artifacts_fts, rowid, content_json) VALUES ('delete', OLD.rowid, OLD.content_json);
END;
```

Without these triggers, FTS5 content-sync tables go stale immediately. This was a schema bug in v0.3.

*[Source: GLM 5.1 — FTS5 synchronization triggers]*

### 13.6 Global Index Database

Stored at `$XDG_DATA_HOME/ozone/global.db`:

```sql
CREATE TABLE sessions (
    session_id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    character_name TEXT,
    created_at INTEGER NOT NULL,
    last_opened_at INTEGER NOT NULL,
    message_count INTEGER NOT NULL DEFAULT 0,
    db_size_bytes INTEGER,
    tags TEXT  -- JSON array of user tags
);

CREATE TABLE session_search (
    session_id TEXT NOT NULL,
    message_id TEXT NOT NULL,
    content TEXT NOT NULL,
    author_kind TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (session_id, message_id)
);

CREATE VIRTUAL TABLE session_search_fts USING fts5(
    content,
    content=session_search,
    content_rowid=rowid
);

-- Cross-session search triggers (same pattern as §13.5)
```

The global index is updated when:
- A session is opened (sync metadata)
- A session is closed (update message_count, db_size)
- A message is committed (async update to search index)

This enables cross-session search without opening every session DB.

*[Source: GLM, Qwen, MiMo, Gemini (4/5) — global index database]*

### 13.7 Indexes

```sql
CREATE INDEX idx_messages_parent ON messages(parent_id);
CREATE INDEX idx_messages_session ON messages(session_id);
CREATE INDEX idx_messages_created ON messages(created_at);
CREATE INDEX idx_branches_session ON branches(session_id);
CREATE INDEX idx_branches_state ON branches(state);
CREATE INDEX idx_ancestry_descendant ON message_ancestry(descendant_id);
CREATE INDEX idx_artifacts_session ON memory_artifacts(session_id);
CREATE INDEX idx_artifacts_kind ON memory_artifacts(kind);
CREATE INDEX idx_swipe_groups_parent ON swipe_groups(parent_message_id);
CREATE INDEX idx_context_plans_session ON context_plans(session_id, created_at);
```

---

## 14. Context Assembly

### 14.1 Assembly Pipeline

```
ContextLayerPolicy → for each layer:
  1. Resolve source data (system prompt, character card, pinned memory, messages, lorebook, etc.)
  2. Estimate token count using active TokenEstimationPolicy
  3. Check budget allocation (min_budget_pct, max_budget_pct)
  4. Apply Hard Context Overflow Policy (§11.6)
  5. Apply collapse strategy if soft context exceeds budget
  → Produce ContextPlan
```

### 14.2 Budget Allocation Algorithm

1. **Reserve hard context minimums:** Sum `min_budget_pct` for all hard context layers
2. **Reserve soft context minimums:** Sum `min_budget_pct` for all soft context layers
3. **If hard + soft minimums exceed 100%:** Error — invalid policy configuration
4. **Distribute remaining budget:** Proportional to each layer's `max_budget_pct - min_budget_pct`
5. **Hard context overflow:** If a hard item exceeds its allocated budget, compress soft context layers (lowest priority first) until the hard item fits
6. **If even after full soft compression, hard items don't fit:** Emit `OzoneError::HardContextOverflow` and let the user decide (via Context Inspector)

### 14.3 ContextPlan Output

```rust
struct ContextPlan {
    id: ContextPlanId,
    layers: Vec<ContextPlanLayer>,
    total_tokens: usize,
    budget: usize,
    safety_margin_tokens: usize,
    estimation_policy: TokenEstimationPolicy,
    is_dry_run: bool,
    created_at: i64,
}

struct ContextPlanLayer {
    kind: ContextLayerKind,
    content: String,           // actual text that will be sent
    token_count: usize,
    was_truncated: bool,
    truncation_reason: Option<String>,
    items_included: usize,
    items_omitted: usize,
    omitted_items: Vec<OmittedItem>,
}

struct OmittedItem {
    description: String,
    token_count: usize,
    reason: OmissionReason,
}

enum OmissionReason {
    BudgetExceeded,
    PriorityTooLow { score: f32 },
    StaleArtifact,
    UserExcluded,
}
```

Every `ContextPlan` records which estimation policy was used. The Context Inspector visually distinguishes exact counts from estimates.

### 14.4 Inline Context Preview

While typing, the status bar shows a real-time preview:
```
Tokens: 150/8192 | Context: [Elara Card] [Pinned: Sword of Doom] [Recent: 12 msgs]
```

This updates on each keystroke (debounced 200ms) using cached token counts.

*[Source: Trinity Large — inline context preview]*

---

## 15. Token Counting

### 15.1 Three-Tier Fallback Chain

1. **Exact:** Backend provides tokenizer (llama.cpp `/tokenize` endpoint). Zero safety margin.
2. **Approximate:** Local model-family tokenizer (e.g., SentencePiece for LLaMA family). 5–10% margin.
3. **Heuristic:** Character-count based with per-language calibration (§11.7). 10–25% margin.

### 15.2 Calibration

At startup (if exact tokenizer is available), Ozone optionally calibrates the heuristic:
1. Sample 10 representative messages from the session
2. Tokenize exactly, measure actual chars_per_token ratio
3. Store calibration ratio per model family in cache

This improves heuristic accuracy for sessions with unusual content (e.g., mixed CJK/Latin, code-heavy).

*[Source: MiMo V2 Pro — calibration step]*

### 15.3 TokenEstimationConfidence

```rust
enum TokenEstimationConfidence {
    Exact,
    High,    // approximate, calibrated
    Medium,  // approximate, uncalibrated
    Low,     // heuristic
}
```

The Context Inspector shows confidence alongside token counts: `340 tokens (exact)` vs `~350 tokens (heuristic)`.

---

## 16. Memory System

### 16.1 Canonical transcript remains untouched
The transcript is never replaced by compressed forms. All derived artifacts are stored in `memory_artifacts`.

### 16.2 Memory is artifact-based
The Memory Engine creates artifacts derived from the transcript: chunk summaries, embeddings, synopsis snapshots, importance proposals, retrieval keys.

### 16.3 Hybrid Retrieval (BM25 + Vector) — Phase 2

**Phase 1 (FTS5 only):** Keyword search via SQLite FTS5. Sufficient for exact name matching and simple lookups. Pinned memory available immediately.

**Phase 2 (Hybrid):** Add vector embeddings via `fastembed-rs` + `usearch`. Hybrid scoring combines BM25 and vector:

```rust
fn hybrid_score(bm25_score: f32, vector_score: f32, alpha: f32) -> f32 {
    // alpha = 0.5 by default, configurable
    (alpha * bm25_score) + ((1.0 - alpha) * vector_score)
}
```

For RP, finding the exact name of a sword is often more important than finding "something similar to a weapon."

*[Source: Gemini 3 Flash — BM25 + Vector hybrid. Qwen, Trinity — defer hybrid to Phase 2.]*

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
    let importance = candidate.importance_score.unwrap_or(0.5);
    let recency = candidate.recency_decay();
    let provenance = candidate.provenance_weight();

    (weights.semantic * semantic
        + weights.importance * importance
        + weights.recency * recency
        + weights.provenance * provenance)
        .clamp(0.0, 1.0)
}
```

All weights configurable per session or per character. Sum validated = 1.0 at config load.

### 16.5 Provenance Decay

Auto-generated summaries lose **15% weight** per retrieval cycle without user interaction:

```rust
fn adjusted_provenance_weight(base: f32, cycles_since_interaction: u32) -> f32 {
    base * (0.85_f32).powi(cycles_since_interaction as i32)
}
```

*[Source: Qwen 3.6 Plus — Provenance Decay]*

### 16.6 Memory Storage Tiering

**IMPORTANT:** Tiering applies **exclusively** to derived artifacts. The canonical `messages` table is never pruned.

| Age | Storage Level | What's Kept (Derived Artifacts Only) |
|-----|--------------|--------------------------------------|
| Recent (< 100 messages) | Full | All artifacts: embeddings, summaries, importance scores |
| Older (100–1000 messages) | Reduced | Summaries + embeddings only. Raw importance proposals pruned. |
| Archive (> 1000 messages) | Minimal | Session synopsis + key pinned memories only |

Thresholds configurable. UI shows storage usage indicator. Automatic cleanup runs as P4 background-idle-only job.

*[Source: Trinity Large — Memory Storage Tiering. MiMo V2 Pro — clarification.]*

### 16.7 Stale Artifact Detection

```rust
struct StaleArtifactPolicy {
    max_age_messages: usize,    // default: 500
    max_age_hours: u64,         // default: 168 (1 week)
}
```

Flagged artifacts shown as `⚠ stale` in UI. Users can refresh or dismiss.

### 16.8 Garbage Collection

```rust
struct GarbageCollectionPolicy {
    max_active_embeddings: usize,     // default: 10_000
    archive_after_n_turns: usize,     // default: 1_000
    purge_unreferenced_backlog: bool, // default: true
    compaction_interval_hours: u64,   // default: 24
}
```

P4 background compaction: merge stale embeddings, clear orphaned artifacts, regenerate synopsis.

### 16.9 Disk Space Monitoring

The persistence layer monitors available disk space:
- Warning at < 500MB free: status bar shows `⚠ Low disk`
- Critical at < 100MB free: pause background jobs, warn user prominently
- Emergency at < 50MB free: switch SQLite to `synchronous = FULL`, refuse new artifact creation

*[Source: GLM 5.1 — disk space monitoring]*

### 16.10 Vector Index Management

`ozone index rebuild` CLI command regenerates the vector index from stored embedding artifacts. This makes the vector index strictly derivable.

Index version tracking:
- Store `usearch` library version in `global.db` metadata
- On startup, compare stored version with linked version
- If mismatch, warn user and offer automatic rebuild

*[Source: GLM 5.1 — vector index rebuild CLI]*

---

## 17. Group Chat Architecture

### 17.1 Phased Rollout

#### Phase 1 (Minimal MVP — Explicit Control Only)
- Shared scene history
- Per-character cards
- User-directed speaker control (`/as Character`)
- Simple round robin
- Narrator toggle for explicit scene descriptions

#### Phase 2
- **Mention-based speaker auto-detection** (deferred from Phase 1 to avoid false positives)
- Assistive speaker suggestions
- Per-character pinned facts
- Simple relationship hints in context

#### Phase 3
- Relationship overlays
- Optional private knowledge scopes
- Narrator policies
- Advanced turn routing

#### Phase 4
- Hidden memory domains
- Unreliable knowledge
- Scene-level reasoning helpers

*[Source: Trinity Large — start explicit-only, defer mention detection to Phase 2]*

### 17.2 Speaker Selection Strategy
Hybrid logic:
1. Deterministic rules first: direct `/as`, round-robin, cooldown
2. Optional assistive ranking second (Phase 2+): candidate name, confidence, reason class

### 17.3 Narrator
Begins as explicit command only. Auto narrator firing is Tier C.

---

## 18. Thinking & Reasoning Block Policy

### 18.1 Streaming Parser

Use a **`tokio-util::codec::Decoder`** state machine to detect `<think>`/`</think>` boundaries during streaming. This handles:
- Partial UTF-8 chunks across TCP boundaries
- Async streaming without blocking
- Buffer management via the `Decoder` trait's built-in framing

The `nom` library (v0.3) is synchronous and not designed for async streaming — `tokio-util` codec is the correct pattern.

*[Source: Qwen 3.6 Plus — tokio-util codec instead of nom]*

### 18.2 Display Modes
- **Immersive:** Hide reasoning unless manually inspected
- **Assisted:** Show one-line summaries of thinking blocks
- **Debug:** Expose raw reasoning blocks, summaries, and parser events

### 18.3 Elicited Thinking
**Explicit opt-in only** with model-specific warnings. The UI should note potential impacts: increased verbosity, voice distortion, added latency.

*[Source: Trinity Large — explicit opt-in with model warnings]*

---

## 19. Backend Strategy

### 19.1 Capability-Based Abstraction

```rust
trait ChatCompletionCapability {
    async fn complete(&self, request: &CompletionRequest)
        -> OzoneResult<impl Stream<Item = OzoneResult<StreamChunk>>>;
}

trait EmbeddingCapability {
    async fn embed(&self, texts: &[&str]) -> OzoneResult<Vec<Vec<f32>>>;
}

trait TokenizationCapability {
    fn count_tokens(&self, text: &str) -> OzoneResult<usize>;
    fn model_family(&self) -> &str;
}

trait GrammarSamplingCapability {
    async fn complete_with_grammar(&self, request: &CompletionRequest, grammar: &str)
        -> OzoneResult<impl Stream<Item = OzoneResult<StreamChunk>>>;
}

trait ModelMetadataCapability {
    fn model_name(&self) -> &str;
    fn context_length(&self) -> usize;
    fn supported_stop_strings(&self) -> &[String];
    fn streaming_format(&self) -> StreamingFormat;
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

At startup, capability probes run and populate the matrix.

### 19.3 Tokenizer Fallback Chain
Exact backend tokenizer → local approximate tokenizer → character-count heuristic (see §15).

### 19.4 Prompt Formatting Templates

Local models are highly sensitive to exact control tokens. Ozone uses `minijinja` templates for prompt formatting:

```
templates/
├── chatml.jinja           # ChatML format (default)
├── alpaca.jinja           # Alpaca instruction format
├── llama3-instruct.jinja  # Llama-3-Instruct format
├── mistral-instruct.jinja # Mistral instruction format
├── vicuna.jinja           # Vicuna format
└── raw.jinja              # No formatting (passthrough)
```

**Example ChatML template:**
```jinja
{%- for message in messages -%}
<|im_start|>{{ message.role }}
{{ message.content }}<|im_end|>
{% endfor -%}
<|im_start|>assistant
```

**Configuration:**
```toml
[backend]
url = "http://localhost:5001"
type = "koboldcpp"
prompt_template = "chatml"  # references templates/chatml.jinja
```

**Custom templates:** Users can add custom `.jinja` files to `$XDG_CONFIG_HOME/ozone/templates/`. Custom templates override built-in ones with the same name.

**Template selection:** If `prompt_template` is not specified, Ozone attempts to auto-detect from the model metadata (model name heuristics). Falls back to `chatml` if unknown.

*[Source: Gemini 3.1 Pro — prompt formatting templates via minijinja. This is a hard blocker for local LLM usage.]*

### 19.5 Streaming Response Handler

Uses `tokio-util::codec::Decoder` to parse streaming responses:

```rust
struct StreamDecoder {
    format: StreamingFormat,
    buffer: BytesMut,
    think_block_state: ThinkBlockParserState,
}

impl Decoder for StreamDecoder {
    type Item = StreamChunk;
    type Error = OzoneError;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<StreamChunk>, OzoneError> {
        match self.format {
            StreamingFormat::ServerSentEvents => self.decode_sse(src),
            StreamingFormat::JSONLines => self.decode_jsonlines(src),
            StreamingFormat::Chunked => self.decode_chunked(src),
        }
    }
}
```

*[Source: Qwen 3.6 Plus — tokio-util Framed Codec]*

### 19.6 Backend Health Monitoring

Lightweight `/health` or `/api/v1/model` ping every 30 seconds:

| Status | Indicator | Behavior |
|--------|-----------|----------|
| Healthy | `✓ llama.cpp` | Normal operation |
| Slow (> 5s response) | `⚠ llama.cpp` | Warn user, continue |
| Unreachable | `✗ llama.cpp` | Block generation, show error, retry in background |
| Model changed | `⚡ llama.cpp` | Warn, recalculate context budget, invalidate token cache |

*[Source: Qwen, Trinity — backend health check polling]*

### 19.7 Model Alias Pre-Flight Checks

At startup or config reload:
1. Verify backend target is reachable
2. Query model name and context length
3. Compare with `model_name` stored in session metadata
4. If mismatch: warn via status bar, offer to update session metadata

*[Source: Gemini 3.1 Pro — model alias pre-flight checks]*

### 19.8 Backend Degradation Rules
If utility backend is down: main chat still works, retrieval may use cached artifacts, UI surfaces degraded status clearly, no hidden failure occurs.

---

## 20. Configuration System

### 20.1 Format
TOML. Human-readable, no indentation sensitivity, strong Rust ecosystem support.

### 20.2 File Hierarchy

```
1. Hardcoded defaults (in code)
2. ~/.config/ozone/config.toml            (global user config)
3. <session_dir>/config.toml              (per-session overrides)
4. Character card embedded settings        (per-character)
5. CLI flags                              (override all)
```

Each layer overrides the previous using the `config` crate's `Config::builder().add_source()` chain, which provides **deep merging** of nested TOML tables. This means changing `[context.weights.semantic]` in a session config only overrides that specific key, not the entire `[context.weights]` section.

**Note (v0.4 fix):** v0.3 claimed "serde deserialization merges layers" — this was incorrect. Serde replaces entire nested structs. The `config` crate provides the deep merge behavior we need.

*[Source: Qwen 3.6 Plus — use config crate for deep merging]*

### 20.3 XDG Compliance
- Config: `$XDG_CONFIG_HOME/ozone/` (default: `~/.config/ozone/`)
- Data: `$XDG_DATA_HOME/ozone/` (default: `~/.local/share/ozone/`)
- Cache: `$XDG_CACHE_HOME/ozone/` (default: `~/.cache/ozone/`)

### 20.4 Hot-Reload vs. Immutable

| Setting | Mutable at Runtime? | Requires Restart? |
|---------|--------------------|--------------------|
| Backend URL | No | Yes |
| Database path | No | Yes |
| Embedding model | No | Yes |
| Theme / colors | Yes | No |
| Retrieval weights | Yes | No |
| Context layer policy | Yes | No |
| Keybindings | Yes | No (reload via command) |
| Token budget | Yes | No |
| Backpressure limits | Yes | No |
| Provenance weights | Yes | No |
| Prompt template | Yes | No (reload via command) |

### 20.5 Validation
Config validated at load time. Invalid values produce `OzoneError::ConfigInvalid` with specific key and reason. System refuses to start with fatal config error.

### 20.6 Config Version Migration

```toml
[meta]
config_version = 1
```

When Ozone upgrades and the config schema changes, the loader:
1. Reads `config_version`
2. Applies forward migrations (e.g., rename keys, add defaults for new sections)
3. Writes updated config with new `config_version`
4. Logs all migration actions

*[Source: GLM 5.1 — config version migration]*

### 20.7 Config Presets

Built-in presets for common use cases:

| Preset | Description |
|--------|-------------|
| `minimal` | Smallest memory footprint, no background jobs, FTS5 only |
| `standard` | Balanced defaults (the default) |
| `aggressive-memory` | High embedding count, frequent summaries, low tiering thresholds |
| `low-resource` | Reduced cache, fewer concurrent jobs, larger safety margins |

Applied via: `ozone --preset minimal` or `preset = "minimal"` in config.toml. Preset values are overridden by explicit config settings.

*[Source: MiMo V2 Pro — config presets/profiles]*

### 20.8 Example Config

```toml
[meta]
config_version = 1

[backend]
url = "http://localhost:5001"
type = "koboldcpp"
prompt_template = "chatml"

[backend.health]
poll_interval_secs = 30
timeout_secs = 5

[backend.rate_limit]
min_interval_ms = 500
max_pending_requests = 2

[context]
max_tokens = 8192
safety_margin_pct = 10
default_policy = "standard"

[context.weights]
semantic = 0.35
importance = 0.25
recency = 0.20
provenance = 0.20

[memory]
max_active_embeddings = 10000
archive_after_turns = 1000
compaction_interval_hours = 24

[memory.provenance_weights]
user_authored = 1.0
character_card = 0.9
lorebook = 0.85
system_generated = 0.7
utility_model = 0.6
imported_external = 0.5

[ui]
theme = "dark"
mode = "standard"
message_collapse_lines = 20
draft_autosave_interval_ms = 2000

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
dry_run = "Ctrl+d"
bookmark = "Ctrl+k"
clipboard_copy = "Ctrl+y"

[events]
max_event_age_days = 90
export_before_compaction = false

[tasks]
max_concurrent_jobs = 3
max_queue_size = 20
stale_job_timeout_secs = 300

[logging]
level = "info"
file = true
stderr = false
json = false
subsystem_levels = { inference = "debug", persistence = "warn" }
rotation = "daily"
max_log_files = 7
```

---

## 21. TUI / UX Design

### 21.1 Default Layout

```text
┌─────────────────────────────────────────────────────────────────┐
│ Ozone v0.4          [Session: Dragon's Rest]  [Elara]  💾 🔗0 📝0 │
├────────────────────────────────┬────────────────────────────────┤
│                                │ Context Inspector              │
│  Chat Area                     │ ┌──────────────────────────┐  │
│  (scrollable, virtualized)     │ │ Budget: 3847/8192        │  │
│                                │ │ ████████░░░░ 47%         │  │
│  [System] You are Elara...     │ ├──────────────────────────┤  │
│                                │ │ ✓ System Prompt  120     │  │
│  [Elara] The forest path       │ │ ✓ Character Card 340     │  │
│          twisted ahead...      │ │ ✓ Pinned Memory  89      │  │
│                                │ │ ✓ Recent msgs   2100     │  │
│  [You] I draw my sword.        │ │ ✓ Lore: Xylo-7   45     │  │
│                                │ │ ✗ Lore: Old War          │  │
│  [Elara] Your blade catches    │ │   (budget exceeded)      │  │
│          moonlight...          │ └──────────────────────────┘  │
│                                │ [Force Include] [Dry Run]     │
├────────────────────────────────┴────────────────────────────────┤
│ > Type your message...                                  [Enter] │
├─────────────────────────────────────────────────────────────────┤
│ Tokens: 3847/8192 │ ✓ llama.cpp │ FTS: OK │ 🔗0 📝0 │ Standard │
└─────────────────────────────────────────────────────────────────┘
```

### 21.2 Responsive Layout

| Terminal Width | Layout |
|---------------|--------|
| ≥ 120 columns | Full: chat + side-by-side inspector |
| 100–119 columns | Inspector stacks vertically below chat (on toggle) |
| 80–99 columns | Inspector hidden; compact budget summary in status bar |
| < 80 columns | Minimal: chat + input + status bar only |

Inspector auto-hides below threshold. User can force-show via `Ctrl+I` at any width.

*[Source: Trinity Large — responsive context inspector]*

### 21.3 UI Modes

| Mode | Description | Default Visibility |
|------|-------------|-------------------|
| **Minimal** | Immersive RP. Chat + input only. | Chat, input |
| **Standard** | Balanced. Chat + toggleable inspector + status bar. | Chat, input, status bar |
| **Developer** | Full transparency. All panes, diagnostics, raw reasoning. | Everything |

### 21.4 Input Model

```rust
enum InputMode {
    Normal,           // Typing. Enter sends, Alt+Enter = newline.
    Command,          // After "/" — autocomplete slash commands
    Palette,          // Command palette — fuzzy search
    Inspector,        // Focus in context inspector — navigation
    Search,           // Ctrl+F in-message search
    EditorLaunch,     // $EDITOR opened for long-form composition
    Edit,             // Editing an existing message (Ctrl+E on selected message)
}
```

**Input support:**
- `Enter` to send, `Alt+Enter` for newline
- `$EDITOR` escape for long messages
- Input history with up-arrow recall
- Paste handling (auto-enters multi-line mode)
- **Draft persistence:** Input buffer auto-saved to `<session_dir>/draft.txt` every 2 seconds (debounced). Restored on session load.
- **Clipboard:** `Ctrl+Y` to yank selected message content. `Ctrl+Shift+V` to paste.

*[Source: MiMo V2 Pro — InputMode. MiMo, Gemini — draft persistence. Qwen, MiMo — clipboard.]*

### 21.5 Message Editing

`Ctrl+E` on a selected message enters `InputMode::Edit`:
1. Message content loaded into input area
2. User edits and presses `Enter` to confirm, `Escape` to cancel
3. On confirm: update `content` and `edited_at` in DB, show `(edited)` indicator
4. Re-derivation policy: editing a message invalidates derived artifacts (summaries, embeddings) that reference it. They are flagged `⚠ stale` and optionally re-queued.

*[Source: MiMo V2 Pro, Trinity Large — message editing UX]*

### 21.6 Default Keybindings

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
| Dry-run context | Ctrl+D | Normal |
| Bookmark message | Ctrl+K | Normal |
| Edit message | Ctrl+E | Normal (on selected) |
| Yank/copy | Ctrl+Y | Normal (on selected) |
| Quick help | ? | Normal (input empty) |
| Quit | Ctrl+Q | Any |
| Search | Ctrl+F | Any |
| Undo | Ctrl+Z | Normal |

All keybindings **user-configurable** via `[ui.keybindings]`.

### 21.7 Color / Theme System

| Tier | Capability | Rendering Strategy |
|------|-----------|-------------------|
| **Monochrome** | No color | Bold, underline, dim, reverse |
| **16-color** | Named ANSI | Semantic color names |
| **Truecolor** | 24-bit hex | Full theme support |

All information readable in monochrome. Auto-detected at startup, overridable in config.

### 21.8 Long Message Handling
- Auto-collapse above configurable threshold (default: 20 lines)
- `▸ expand (47 lines)` indicator
- Virtualized rendering (only visible messages formatted)
- "Jump to message" command
- Swipe diff-style side-by-side comparison (wide terminals)

### 21.9 Status Bar

```text
│ Tokens: 3847/8192 │ ✓ llama.cpp │ FTS: OK │ 💾 🔗0 📝0 ⚠0 │ Standard │
```

- Token count with budget
- Backend health status (✓/⚠/✗)
- Retrieval status
- 💾 Auto-save indicator (pulses briefly on save)
- Background job indicators
- Current UI mode

*[Source: GLM 5.1 — auto-save indicator]*

### 21.10 Notifications

- Status bar icons update in real-time with queue depth
- "📝 New summary available" non-intrusive notification (5s)
- Generation completion: terminal bell (`\x07`), optional desktop notification via `notify-rust`
- Degradation flags persist until resolved
- Developer mode: full "Jobs" panel

*[Source: MiMo V2 Pro — generation completion notifications]*

### 21.11 Onboarding / First-Run Experience

1. **Backend setup wizard:** "Where is your LLM running?" → auto-detect on common ports
2. **Character card:** Import existing or use built-in sample
3. **Quick tutorial:** Key features (branches, swipes, context inspector) with sample session
4. **Feature discovery:** First session shows subtle `?` hint overlay

### 21.12 Undo/Redo

Lightweight undo backed by the event sourcing system:
- Tracks last N reversible actions (default: 20)
- Reversible: message delete, branch switch, swipe activation, bookmark toggle
- NOT reversible: generation (creates new content — use branches instead)
- `Ctrl+Z` to undo, `Ctrl+Shift+Z` to redo

*[Source: GLM 5.1, MiMo V2 Pro — undo/redo via event sourcing]*

### 21.13 Accessibility
- `--plain` CLI flag: ASCII borders, no ANSI styling, linear text
- Terminal width detection with content-boundary wrapping
- All status indicators have text alongside color
- High-contrast theme available

---

## 22. Security Model

### 22.1 Untrusted Input
Character cards and lorebooks are **untrusted input**. Rules:
- Sanitized before display and context injection
- Lorebook entries scoped to soft context only
- Strict JSON schema validation via `serde` + custom validators
- No executable code in character cards

### 22.2 Character Card Schema (SillyTavern V2)

```json
{
  "spec": "chara_card_v2",
  "spec_version": "2.0",
  "data": {
    "name": "Elara",                          // REQUIRED
    "description": "A wandering mage...",     // REQUIRED
    "personality": "Curious, cautious...",     // optional
    "first_mes": "The forest path...",        // REQUIRED
    "mes_example": "<START>...",              // optional
    "scenario": "A dark forest...",           // optional
    "system_prompt": "",                      // optional, Ozone-specific override
    "creator_notes": "",                      // optional
    "tags": ["fantasy", "mage"],              // optional
    "alternate_greetings": [],                // optional
    "character_book": { ... },                // optional, embedded lorebook
    "extensions": {
      "ozone": {
        "card_version": 1,
        "context_policy_overrides": {},
        "preferred_prompt_template": "chatml"
      }
    }
  }
}
```

**Validation on import:**
1. `name`, `description`, `first_mes` are required — reject without
2. Warn if `description` > 1000 chars (suggest splitting to lorebook if > 2000)
3. Validate `character_book` entries if present
4. Strip unknown fields, preserve `extensions` for round-trip compatibility
5. Store with `ozone.card_version` for future migration

*[Source: GLM 5.1 — card version field. Trinity Large — validation on import. MiMo V2 Pro — required fields.]*

### 22.3 Credential Storage
- API keys in OS keychain (`keyring` crate) when available
- Fallback: encrypted config file section
- API keys never logged, never exported, never displayed

### 22.4 Backend Communication
- TLS for non-localhost remote backends
- Plain HTTP for localhost (127.0.0.1, ::1)
- No authentication assumed for local backends

### 22.5 File Permissions
- SQLite databases: `0600`
- Config files: `0600`
- Session directories: `0700`
- Export files: `0644`

### 22.6 Data at Rest
- Not encrypted by default (performance tradeoff)
- Optional `SQLCipher` via `encryption` feature flag
- SSH should use encrypted tunnels

---

## 23. Logging Framework

### 23.1 Stack

`tracing` (facade) + `tracing-subscriber` (output).

### 23.2 Configuration

```toml
[logging]
level = "info"              # global default
file = true                 # write to $XDG_DATA_HOME/ozone/logs/
stderr = false              # also write to stderr
json = false                # structured JSON output (for tooling)
rotation = "daily"          # daily, hourly, or size-based
max_log_files = 7           # keep last 7 rotated files
subsystem_levels = { inference = "debug", persistence = "warn", tui = "info" }
```

### 23.3 Per-Subsystem Levels

Each crate/module maps to a tracing target:
- `ozone_engine` → conversation engine operations
- `ozone_inference` → backend communication, streaming
- `ozone_persist` → SQLite operations, migrations
- `ozone_tui` → UI rendering, input handling
- `ozone_memory` → retrieval, embedding, summary (Phase 2+)

### 23.4 Structured Fields

All log entries include: `session_id`, `subsystem`, `timestamp` (UTC). Generation logs add: `request_id`, `backend_id`, `model_name`. This enables log correlation and filtering.

### 23.5 Log Location

```
$XDG_DATA_HOME/ozone/logs/
├── ozone-2025-07-14.log
├── ozone-2025-07-13.log
└── ...
```

*[Source: GLM 5.1, MiMo V2 Pro — tracing + tracing-subscriber with per-subsystem levels]*

---

## 24. Reliability, Debugging, & Transparency

### 24.1 Event Sourcing

Important actions emit **structured, append-only events**:

```rust
enum OzoneEvent {
    MessageCommitted { message_id: MessageId, branch_id: BranchId },
    MessageEdited { message_id: MessageId },
    BranchCreated { branch_id: BranchId, forked_from: MessageId },
    BranchStateChanged { branch_id: BranchId, old_state: BranchState, new_state: BranchState },
    SwipeActivated { swipe_group_id: SwipeGroupId, ordinal: u16 },
    ContextPlanGenerated { plan_id: ContextPlanId, is_dry_run: bool },
    RetrievalCandidatesRanked { count: usize, top_score: f32 },
    BackgroundJobCompleted { job_type: String, duration_ms: u64 },
    BackgroundJobFailed { job_type: String, error: String },
    BackgroundJobDiscarded { job_type: String, reason: String },
    StaleArtifactDetected { artifact_id: MemoryArtifactId },
    DegradationStateChanged { subsystem: String, degraded: bool },
    ModelContextChanged { old_model: String, new_model: String },
    BookmarkToggled { message_id: MessageId, bookmarked: bool },
    UndoAction { event_type: String },
}
```

Events stored in `events` table with retention policy (§13.4). Enable: debugging, replay, export for analysis.

### 24.2 Reproducibility
Given transcript state, branch, config, context plan, backend params, and prompt template — the system can explain or reproduce the generation setup for any turn.

### 24.3 No Invisible Mutation
Background jobs must never silently rewrite active conversational state.

---

## 25. Performance Strategy

### 25.1 Concrete Targets

| Metric | Target | Constraint |
|--------|--------|-----------|
| TUI frame time | **< 33ms** | 30 FPS minimum |
| Context assembly | **< 500ms** | 8K token budget, 1000-message session |
| Per-message render | **< 10ms** | Single message formatting |
| Cold startup | **< 2s** | 10 sessions in data directory |
| Memory usage | **< 200MB** | 10K-message active session, no generation |
| SQLite DB size | **< 500MB** | 100K-message session with all artifacts |
| Foreground overhead | **< 50ms** | Excluding model inference time |

### 25.2 Job Priority Classes

| Priority | Class | Examples |
|----------|-------|---------|
| P0 | Foreground-critical | Stream completion, cancel generation, load session |
| P1 | Foreground-assistive | Context plan generation, speaker proposal |
| P2 | Background-low-latency | Embeddings, importance proposal, thinking summary |
| P3 | Background-batch | Chunk summarization, index rebuild, synopsis |
| P4 | Background-idle-only | Cleanup, compaction, event retention, export |

**Backpressure limits:**
- Maximum concurrent background jobs: **3**
- Maximum job queue size: **20**
- Stale job auto-cancellation: **5 minutes**
- P2+ jobs only when no P0/P1 active
- P4 jobs only when idle (no user input for 30s)

### 25.3 Caching
Cache: token counts, embeddings, prompt assembly fragments, utility proposals, session metadata. LRU eviction with configurable size limits.

---

## 26. Testing Strategy

### 26.1 Required Test Types

| Subsystem | Test Type | What It Validates |
|-----------|----------|-------------------|
| Conversation Engine | Unit tests | Message CRUD, branch creation, swipe activation, message ordering |
| Conversation Engine | Integration tests | Concurrent access, SQLite transactions, crash recovery |
| Context Assembler | Property-based tests | Budget invariants: used_tokens ≤ budget, hard context never dropped |
| Context Assembler | Snapshot tests | Compare ContextPlan JSON across versions |
| Memory Engine | Unit tests | Retrieval scoring, artifact lifecycle, GC policies |
| Memory Engine | Property-based tests | Scores in [0, 1], ranking deterministic |
| Inference Gateway | Mock backend tests | Streaming, cancellation, backpressure, error recovery |
| TUI | Render tests | `ratatui::backend::TestBackend` for layout verification |
| Stream Parser | Fuzz tests | Malformed think blocks, interrupted streams, mixed encoding |
| Persistence | Migration tests | Forward migration, rollback, schema version consistency |
| Config | Validation tests | Invalid values, deep merge correctness, preset loading |
| Import/Export | Roundtrip tests | Import ST card → export → re-import = identical |

### 26.2 CI Pipeline
- All tests on every PR
- Property-based: 1000+ cases
- Snapshot tests fail loudly on diff
- Fuzz tests nightly with extended iterations

---

## 27. Session Export Format

### 27.1 Native Ozone Export (JSON)

Full-fidelity export preserving branches, swipes, metadata:

```json
{
  "ozone_export_version": 1,
  "session_id": "...",
  "session_name": "Dragon's Rest",
  "character_card": { ... },
  "branches": [
    {
      "branch_id": "...",
      "name": "main",
      "messages": [
        {
          "message_id": "...",
          "parent_id": "...",
          "author": { "kind": "character", "name": "Elara" },
          "content": "...",
          "created_at": 1720000000000,
          "metadata": { ... }
        }
      ],
      "swipe_groups": [ ... ]
    }
  ],
  "memory_artifacts": [ ... ],
  "events": [ ... ]
}
```

### 27.2 SillyTavern-Compatible Export (Lossy)

SillyTavern uses flat JSONL (one message per line). Conversion rules:

| Ozone Feature | SillyTavern Mapping | Loss |
|---------------|-------------------|------|
| Active branch messages | Exported as flat list | Branch structure lost |
| Inactive branches | Omitted | All non-active branches lost |
| Swipe candidates | `swipes` array on parent message | Swipe metadata lost |
| AuthorId::User | `is_user: true` | |
| AuthorId::Character | `is_user: false, name: "..."` | |
| AuthorId::System | `is_system: true` | |
| AuthorId::Narrator | `is_user: false, name: "[Narrator]"` | Role distinction lost |
| Bookmarks | Omitted | |
| Memory artifacts | Omitted | |
| Generation records | Omitted | |
| Thinking blocks | Omitted or inline | Depends on user preference |

### 27.3 Markdown Export

Human-readable export for archival:

```markdown
# Dragon's Rest
**Character:** Elara | **Created:** 2025-07-14

---

**[System]** You are Elara, a wandering mage...

**[Elara]** The forest path twisted ahead...

**[You]** I draw my sword and step forward.

**[Elara]** Your blade catches moonlight...
```

*[Source: MiMo V2 Pro, Trinity Large — export format specification]*

---

## 28. Shell-Based Extensibility (Pre-WASM)

Community contributions shouldn't wait for Tier C WASM plugins. Shell-based extensibility provides immediate value:

### 28.1 Custom Slash Commands

Users can add shell scripts to `$XDG_CONFIG_HOME/ozone/commands/`:

```
commands/
├── dice.sh        # /dice 2d6 → rolls dice, inserts result
├── weather.sh     # /weather → fetches weather, inserts as system message
└── lore.sh        # /lore add "..." → adds lorebook entry
```

Scripts receive: session_id, last_message, active_character as environment variables. Output is inserted as a system message or replaces input.

### 28.2 Pre/Post Generation Hooks

```toml
[hooks]
pre_generation = "~/.config/ozone/hooks/pre_gen.sh"
post_generation = "~/.config/ozone/hooks/post_gen.sh"
```

- **Pre-generation:** Receives context plan JSON on stdin. Can modify and return on stdout. Exit code 0 = use modified, non-zero = use original.
- **Post-generation:** Receives generated text on stdin. Informational only (cannot modify committed output).

### 28.3 Custom Themes

TOML theme files in `$XDG_CONFIG_HOME/ozone/themes/`:

```toml
# themes/solarized.toml
[colors]
background = "#002b36"
foreground = "#839496"
user_message = "#268bd2"
character_message = "#2aa198"
system_message = "#586e75"
error = "#dc322f"
warning = "#b58900"
```

*[Source: MiMo V2 Pro — shell-based extensibility before WASM]*

---

## 29. Revised Roadmap — Granular Testable Phases

Each phase produces a **testable, runnable artifact**. A developer can verify the phase works before moving on. This replaces the v0.3 monolithic milestone structure.

### Phase 1A: Foundation & Persistence

**Testable artifact:** Unit tests pass. CLI creates, lists, and opens sessions. Schema is correct.

- Workspace scaffold (all crate Cargo.tomls with dependencies)
- `ozone-core`: types.rs (all §11 types), error.rs (§12 taxonomy)
- `ozone-persist`: schema v1 (§13) with UTC timestamps, FTS5 triggers, session_lock, bookmarks, events
- `ozone-persist`: migration framework with backup-before-migrate
- `ozone-persist`: repository trait + SQLite implementation
- `tracing` + `tracing-subscriber` logging (§23)
- Global index DB stub (§13.6) — session metadata only
- `ozone-cli`: basic CLI entrypoint (create session, list sessions, open session, show version)
- **Tests:** Persistence unit tests (CRUD), migration forward/rollback, schema validation, session lock acquire/release

**Acceptance criteria:**
- `cargo test -p ozone-persist` passes with 100% of defined tests
- `ozone create-session "Test"` creates a valid session.db
- `ozone list-sessions` shows created sessions from global.db
- All timestamps are UTC integers, FTS5 triggers fire on insert

### Phase 1B: Conversation Engine

**Testable artifact:** Integration tests pass. CLI stores/retrieves messages, creates branches, navigates swipes.

- `ozone-engine`: ConversationEngine trait + implementation
- Message CRUD: create, read, edit (with edited_at), soft-delete
- Branch creation, switching, state transitions (Active/Inactive/Archived/Deleted)
- Closure table maintenance (transactional, engine-only, per §8.2 contract)
- Swipe system: SwipeGroup creation, candidate addition, activation, FailedMidStream state
- FTS5 keyword search (within session)
- Event sourcing: all OzoneEvent variants emitted and stored
- Channel-based command processing (mpsc<Command>)
- **Tests:** Engine unit tests, property-based invariant tests (ancestry consistency, swipe ordering), concurrent access integration tests

**Acceptance criteria:**
- 100 messages can be inserted and queried by branch path in < 100ms
- Branch creation correctly populates closure table
- Swipe activation changes active display message
- FTS5 search finds messages by content keyword
- Events table records all state changes

### Phase 1C: TUI Shell

**Testable artifact:** Launch app, see chat layout, type messages, navigate with keybindings (against mock backend).

- `ozone-tui`: app state machine, layout engine
- InputMode state machine (Normal, Command, Palette, Inspector, Search, EditorLaunch, Edit)
- Responsive layout (§21.2): 80x24 → 120+ column thresholds
- Chat area with virtualized rendering + long-message collapsing
- Status bar with all indicators
- Input area: Enter, Alt+Enter, $EDITOR, up-arrow history
- Configurable keybindings from TOML
- Three-tier color system (monochrome/16/truecolor auto-detect)
- Clipboard support (`arboard`)
- Draft persistence (auto-save input buffer to draft.txt, restore on load)
- **Tests:** TestBackend render tests (layout at various widths), input mode transition tests, keybinding dispatch tests

**Acceptance criteria:**
- `ozone open <session_id>` renders chat layout at 80x24 and 120x40
- Keybindings respond correctly (Enter sends, Ctrl+C cancels, Ctrl+I toggles inspector)
- Draft text survives session close/reopen
- Inspector auto-hides below 120 columns

### Phase 1D: Configuration & Backend

**Testable artifact:** Connect to real KoboldCpp or Ollama, generate text, see streaming output.

- `ozone-cli`: TOML config system with `config` crate deep merging (§20)
- 5-level hierarchy, hot-reload for mutable settings, validation
- Config version migration (§20.6)
- Config presets (§20.7)
- `ozone-inference`: Inference Gateway with capability probing (§19)
- Prompt formatting templates via `minijinja` (§19.4) — ChatML, Alpaca, Llama-3-Instruct, Mistral, raw
- StreamingFormat detection (SSE, JSONLines, Chunked)
- Streaming response handler via `tokio-util::codec::Decoder` (§19.5)
- Streaming error recovery: FailedMidStream preservation (§10.2)
- Backend health check polling every 30s (§19.6)
- Rate limiting (§10.4)
- HardwareResourceSemaphore with configurable capacity (§6.5)
- **Tests:** Mock backend tests (streaming, cancellation, mid-stream failure, rate limiting), config validation tests, config merge tests, template rendering tests

**Acceptance criteria:**
- Ozone connects to KoboldCpp and generates a response with correct prompt formatting
- Streaming tokens appear in TUI in real-time
- Killing KoboldCpp mid-generation preserves partial output with ⚠ indicator
- Config deep merge: session `[context.weights.semantic] = 0.5` only overrides that key
- Health check shows ✓/⚠/✗ in status bar

### Phase 1E: Context Assembly

**Testable artifact:** Dry-run shows correct ContextPlan, token counts match expectations, budget invariants hold.

- `ozone-engine`: ContextAssembler with data-driven ContextLayerPolicy (§14)
- Budget allocation algorithm with hard context overflow policy (§14.2)
- Token counting fallback chain (§15): exact → approximate → heuristic
- Per-language calibration for heuristic (§11.7)
- Configurable safety margins per estimation tier
- Context Inspector pane (§21.1): budget bar, included/omitted items, force-include action
- Dry-run mode (`Ctrl+D`): generate ContextPlan without spending tokens
- Context plan diff between turns (highlight added/omitted items)
- Inline context preview in status bar while typing (§14.4)
- **Tests:** Property-based tests (used_tokens ≤ budget always, hard context never dropped), snapshot tests (ContextPlan JSON stability), edge case tests (2K budget with large character card)

**Acceptance criteria:**
- Dry-run on a session with 100 messages produces a valid ContextPlan in < 500ms
- Hard context items are always present even if they exceed max_budget_pct
- Token counts match exact tokenizer within safety margin
- Context Inspector shows correct included/omitted breakdown

### Phase 1F: Import/Export & Polish

**Testable artifact:** Import SillyTavern character card, export session in all formats, first-run wizard works.

- Character card schema (§22.2): SillyTavern V2, validation rules
- Card validation on import (required fields, length warnings, lorebook extraction)
- Lorebook entry model (§11.9) with matching strategy
- Session export: native JSON (§27.1), SillyTavern-compatible JSONL (§27.2), Markdown (§27.3)
- Auto-session naming (after 3-5 messages, generate name from first user message)
- Undo/redo system (§21.12)
- Generation completion notifications (terminal bell, optional desktop)
- Message bookmarking UI (Ctrl+K)
- Session statistics (`:stats` — message count, branch count, token usage, DB size)
- Numeric swipe shortcuts (§9.2)
- Onboarding wizard (§21.11)
- **Tests:** Import/export roundtrip tests (import ST card → export → re-import = identical), card validation tests, export format conformance tests

**Acceptance criteria:**
- A SillyTavern V2 character card imports without errors
- Exported JSONL can be loaded by SillyTavern
- Undo reverses last message deletion
- `:stats` shows accurate session statistics
- First launch presents onboarding wizard

---

### Phase 2A: Pinned Memory & Keyword Search

**Testable artifact:** Pin memories, search by keyword within and across sessions.

- Pinned memory system (PinnedMemory variant in MemoryContent)
- Message pinning to Hard Context with auto-expiry after N turns (Ctrl+K)
- BM25 keyword search via FTS5 (session-local + cross-session via global index)
- Cross-session search via global index (§13.6)
- Memory/retrieval browser (`:memories` command)
- **Tests:** Pin/unpin lifecycle, cross-session search accuracy, auto-expiry countdown

### Phase 2B: Vector Retrieval & Hybrid Scoring

**Testable artifact:** Embeddings generated for messages, hybrid search returns relevant results.

- `fastembed-rs` CPU embedding integration
- `usearch` disk-backed vector index
- Vector index rebuild CLI (`ozone index rebuild`)
- Vector index version tracking with auto-detect stale indices
- Hybrid BM25 + vector retrieval with configurable alpha (§16.3)
- Retrieval scoring with configurable weights (§16.4)
- Configurable provenance weights (§11.5, §20.8)
- Snapshot versioning for background embedding jobs (§6.4)
- **Tests:** Embedding generation, hybrid scoring accuracy, retrieval determinism

### Phase 2C: Summary Artifacts & Memory Lifecycle

**Testable artifact:** Summaries generated for old messages, tiering applies correctly, GC runs.

- Summary artifact generation (ChunkSummary, SessionSynopsis)
- Memory storage tiering (§16.6) — derived artifacts only
- Stale artifact detection (§16.7)
- Garbage collection policies (§16.8)
- Provenance decay (§16.5)
- Events table retention (§13.4)
- Disk space monitoring (§16.9)
- Retrieval browser enhancements
- **Tests:** Summary quality, tiering transitions, GC correctness, disk space thresholds

---

### Phase 3: Assistive Layer

**Testable artifact:** Importance proposals appear, thinking summaries render, shell hooks work.

- Optional importance proposals (Tier B, independently disableable)
- Optional keyword extraction
- Optional thinking summaries via `tokio-util` streaming parser (§18)
- Optional retrieval recommendations
- "Safe mode" toggle (disable all Tier B)
- Shell-based extensibility (§28): custom slash commands, pre/post hooks, custom themes
- Enhanced degraded-state indicators
- **Tests:** Tier B on/off toggle, safe mode, hook execution, thinking block parsing fuzz tests

---

### Phase 4: Group Chat Foundation

- Shared scene context
- Per-character cards
- User-directed turn control (`/as Character`) — explicit only
- Round robin mode with narrator toggle
- **Tests:** Multi-character turn sequencing, narrator insertion

### Phase 5: Advanced Scene Support

- Mention-based speaker auto-detection (deferred from Phase 4)
- Speaker suggestion prototype
- Relationship hints and overlays
- Improved turn routing
- Swipe diff comparator (side-by-side)
- **Tests:** Mention detection accuracy, relationship rendering

### Phase 6: Adaptive Intelligence Experiments

- WASM plugin interface for Tier C (with sandboxing, capability whitelist, binary FFI)
- Fine-tune evaluation
- Flywheel logging as opt-in
- Auto narrator experiments
- Per-character private memory experiments

### Phase 7: Public Release

- Stable config schema, solid documentation
- Import tooling (SillyTavern, Chub, others)
- Polished terminal UX
- Accessibility: `--plain` mode, high-contrast theme
- Security hardening: keychain integration, optional SQLCipher
- Performance benchmarking against §25.1 targets
- Measured expansion based on real failures, not wishlist pressure

---

## 30. Technical Risks & Mitigations

| Risk | Mitigation in Design | Additional Mitigation |
|------|---------------------|----------------------|
| **Intelligence sprawl** | Tiered scope, proposal vs commit, WASM for Tier C | Feature flags for all Tier B/C |
| **Schema churn** | Separated data models, versioned migrations | Backup-before-migrate |
| **Retrieval drift** | Preserve transcript, provenance tracking, decay | User-editable artifacts, stale detection |
| **Group chat explosion** | Phased rollout, explicit control first | Defer mention detection to Phase 5 |
| **Backend mismatch** | Capability-based abstraction, fallback chains | Health monitoring, model pre-flight checks |
| **Premature fine-tuning** | Optional, late-stage, WASM plugins first | Prompt-based utilities validated first |
| **GPU contention** | HardwareResourceSemaphore with priority | CPU-only embeddings via fastembed-rs |
| **Token count inaccuracy** | Three-tier fallback with calibration | Per-language safety margins, confidence tracking |
| **Concurrency bugs** | Single-writer, channel communication | No shared mutable state between subsystems |
| **Storage bloat** | Memory tiering, GC policies, compaction | Disk space monitoring, configurable thresholds |
| **Prompt injection** | Schema validation, soft-context lorebook scoping | Sandboxed WASM for executable content |
| **Multi-instance corruption** | Advisory session lock with heartbeat | Stale lock detection (60s timeout) |
| **Mid-stream failures** | FailedMidStream with partial preservation | User choice: retry, continue, accept, discard |
| **Config complexity** | Presets, validation, version migration | Progressive disclosure via UI modes |
| **Large session import** | Background derivation with progress indicator | Session usable before derivation completes |

---

## 31. Recommendations for First Implementation

### Build first (Phases 1A–1F)
Start with Phase 1A (persistence) and proceed linearly through 1F. Each phase is independently testable. Do not skip phases — each builds on the previous.

### Build second (Phases 2A–2C)
Memory system in three increments: pinned + keyword → vector + hybrid → summaries + lifecycle.

### Build later (Phases 3+)
Assistive layer, group chat, plugins, release.

### Explicit anti-goals for early versions
- Full hidden-intelligence orchestration
- Automatic narrator authority
- Complex auto-world-building logic
- Depending on custom training
- Regex lorebook matching (substring is sufficient for v0.4)
- Multi-modal content

---

## Appendix A: Attribution

### Round 1 Contributions (v0.2 → v0.3)

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
| Complete type definitions | GLM 5.1 |
| Three-tier color system | GLM 5.1 |
| Schema versioning and migration strategy | GLM 5.1 |
| Simplified default context assembly | Trinity Large Thinking |
| Memory storage tiering | Trinity Large Thinking |
| Enhanced Group Chat Phase 1 MVP | Trinity Large Thinking |
| Thinking block as explicit opt-in | Trinity Large Thinking |
| Concrete backpressure numbers | Trinity Large Thinking |
| Multiple UI modes | Trinity Large Thinking |
| Safe mode toggle | Trinity Large Thinking |
| SQLite FTS for keyword retrieval | Trinity Large Thinking |
| Event sourcing | Qwen 3.6 Plus |
| usearch for disk-backed vector storage | Qwen 3.6 Plus |
| Provenance decay | Qwen 3.6 Plus |
| StaleArtifactDetector | Qwen 3.6 Plus |
| Garbage collection policies | Qwen 3.6 Plus |
| Background compaction | Qwen 3.6 Plus |
| `--plain` accessibility mode | Qwen 3.6 Plus |
| Channel-based architecture + snapshots | Qwen 3.6 Plus |
| Input mode state machine | MiMo V2 Pro |
| GenerationState enum / cancellation contract | MiMo V2 Pro |
| Workspace crate structure | MiMo V2 Pro |
| PersistenceLayer trait | MiMo V2 Pro |
| TokenEstimationPolicy enum | MiMo V2 Pro |
| SecurityLevel model | MiMo V2 Pro |
| Hot-reload vs immutable config distinction | MiMo V2 Pro |
| Error taxonomy with severity + retry policy | MiMo V2 Pro |
| Ownership boundary table | MiMo V2 Pro |
| Configurable retrieval weights | Consensus (5/5) |
| Concurrency model specification | Consensus (5/5) |
| Performance targets | Consensus (3+) |
| Testing strategy | Consensus (3+) |

### Round 2 Contributions (v0.3 → v0.4)

| Improvement | Source |
|-------------|--------|
| Prompt formatting templates (minijinja) | Gemini 3.1 Pro |
| HardwareResourceSemaphore (multi-GPU) | Gemini 3.1 Pro |
| Model alias pre-flight checks | Gemini 3.1 Pro |
| Advisory session lock table | GLM 5.1 |
| UTC integer timestamps | GLM 5.1 |
| FTS5 synchronization triggers | GLM 5.1 |
| Streaming error recovery (FailedMidStream) | GLM 5.1 |
| Config version migration | GLM 5.1 |
| Auto-save indicator | GLM 5.1 |
| Vector index rebuild CLI | GLM 5.1 |
| Disk space monitoring | GLM 5.1 |
| Session templates | GLM 5.1 |
| `config` crate for deep merging | Qwen 3.6 Plus |
| `tokio-util` codec for streaming parser | Qwen 3.6 Plus |
| `/dryrun` inline command | Qwen 3.6 Plus |
| Message pinning with auto-expiry | Qwen 3.6 Plus |
| Defer hybrid retrieval to Phase 2 | Qwen 3.6 Plus, Trinity Large |
| Granular testable milestones | Qwen 3.6 Plus, Trinity Large |
| Responsive context inspector | Trinity Large |
| Numeric swipe shortcuts | Trinity Large |
| StreamingFormat capability enum | Trinity Large |
| Character card validation on import | Trinity Large |
| Start group chat explicit-only | Trinity Large |
| Inline context preview while typing | Trinity Large |
| Snapshot versioning for background jobs | MiMo V2 Pro |
| Config presets / profiles | MiMo V2 Pro |
| Shell-based extensibility (pre-WASM) | MiMo V2 Pro |
| Input draft persistence | MiMo V2 Pro, Gemini 3.1 Pro |
| Generation completion notifications | MiMo V2 Pro |
| Auto-session naming | MiMo V2 Pro |
| Quick lorebook entry from selection | MiMo V2 Pro |
| Configurable provenance weights | GLM 5.1, MiMo V2 Pro |
| Events retention policy | GLM 5.1, Qwen 3.6 Plus |
| Rate limiting for backend requests | GLM 5.1, MiMo V2 Pro |
| Backend health monitoring | Qwen 3.6 Plus, Trinity Large |
| Undo/redo via event sourcing | GLM 5.1, MiMo V2 Pro |
| Clipboard/yank support | Qwen 3.6 Plus, MiMo V2 Pro |
| Message editing UX | MiMo V2 Pro, Trinity Large |
| Cross-session global index | GLM 5.1, Qwen 3.6 Plus, MiMo V2 Pro, Gemini 3.1 Pro |
| Derived artifact write path resolution | MiMo V2 Pro, Qwen 3.6 Plus, Gemini 3.1 Pro |
| Per-language token calibration | GLM 5.1, Qwen 3.6 Plus, MiMo V2 Pro |
| Logging framework (tracing) | GLM 5.1, MiMo V2 Pro |

---

## 32. Closing Direction

Ozone v0.4 resolves every critical gap identified by 5 independent LLM reviewers. The architecture is unanimously approved. What remains is implementation.

The strongest version of Ozone is not the version with the most automated cleverness. It is the version that:
- remains lightweight
- stays stable under constrained hardware
- makes context and memory legible
- supports excellent roleplay
- behaves predictably
- can gracefully grow into more intelligence over time

The restructured milestones ensure that every phase produces a testable, usable artifact. A developer can ship Phase 1A, verify it works, and proceed with confidence.

**Build a trustworthy conversation engine first.
Layer intelligence on top only where it clearly improves roleplay without compromising clarity.**

---

Ozone v0.4 design document complete.

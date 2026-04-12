# Ozone v0.2 Design Document — Cross-LLM Synthesis Report

**Models Analyzed:** Gemini 3 Flash, GLM 5.1, Trinity Large Thinking, Qwen 3.6 Plus, MiMo V2 Pro
**Source:** `consolidated_chat.md` — independent analyses of the Ozone Revised Design Document (v0.2)

---

## 1. Universal Consensus (All 5 LLMs Agree)

These points were independently raised by every model. They represent the strongest signal in the analysis.

### 1.1 The Design Is Exceptionally Strong

Every LLM rated the overall architecture highly and used superlatives:
- **Gemini 3 Flash:** "Greenlight-Ready"
- **GLM 5.1:** "Unusually mature pre-development specification"
- **Trinity Large Thinking:** "Significant improvement… disciplined approach"
- **Qwen 3.6 Plus:** "Exceptional architectural maturity"
- **MiMo V2 Pro:** "Exceptionally well-architected design document"

### 1.2 The Deterministic-First Philosophy Is Correct

All 5 models independently validated the pivot from "automated orchestration" to "deterministic transparency" as the right call. The reasoning converges: hidden AI intelligence creates unexplainable outputs, erodes user trust, and makes debugging impossible.

### 1.3 The Tiered Scope Model (A/B/C) Is the Key Structural Decision

Every model highlighted the three-tier system as the most important organizational choice:
- **Gemini 3 Flash:** Praised the "Tier A first" approach
- **GLM 5.1:** Called it "the single most important structural decision"
- **Trinity Large Thinking:** "Excellent… provides a clear implementation roadmap"
- **Qwen 3.6 Plus:** "Prevents intelligence sprawl before the core loop is stabilized"
- **MiMo V2 Pro:** "Explicit anti-goals for early versions section is rare and valuable"

### 1.4 The ContextPlan Is the Standout Innovation

All models singled out the `ContextPlan` as the document's most original and valuable contribution:
- **Gemini 3 Flash:** "Revolutionary UX feature for RP"
- **GLM 5.1:** "The document's most innovative contribution"
- **Trinity Large Thinking:** "Emphasis on inspectability… aligns perfectly with power users"
- **Qwen 3.6 Plus:** "Explainability backbone"
- **MiMo V2 Pro:** "Best UX idea in the document" (★★★★★)

### 1.5 Ownership Boundaries Are Clean and Enforceable

The separation between Conversation Engine (truth), Context Assembler (prompt construction), and Memory Engine (derived artifacts) was unanimously praised. The invariant that "only the Conversation Engine and Context Assembler may commit active state" was specifically called out by 4 of 5 models.

### 1.6 Canonical Transcript as Sacred Truth

All models validated the decision to treat the message history as inviolable and all AI-generated content (summaries, embeddings, importance scores) as derived, regenerable artifacts.

### 1.7 Capability-Based Backend Abstraction Is Sound

Modeling backends through Rust traits (`ChatCompletionCapability`, `EmbeddingCapability`, etc.) rather than monolithic adapters was praised by all as correctly handling the heterogeneous local LLM ecosystem.

### 1.8 Phased Group Chat Rollout Is Pragmatic

All models agreed that deferring full group chat and rolling it out in phases (shared context → assistive → private scopes → unreliable knowledge) avoids the complexity explosion that kills similar projects.

### 1.9 Proposal vs. Commit Distinction Is Essential

Every model highlighted that classifying AI outputs as proposals requiring user acceptance before becoming committed state is critical for the "no invisible mutation" principle.

---

## 2. Strong Consensus — Weaknesses (4–5 LLMs Agree)

These are gaps that the majority or all models independently flagged. They represent the highest-priority items to address.

### 2.1 🔴 Concurrency Model Is Unspecified (5/5)

**Flagged by:** Gemini 3 Flash, GLM 5.1, Trinity Large Thinking, Qwen 3.6 Plus, MiMo V2 Pro

Every model noted that the document describes foreground pipelines, background jobs, and streaming but never specifies the async runtime, lock strategy, cancellation semantics, or how the TUI event loop interacts with inference streams.

**Consensus recommendation:** Use `tokio` (4/5 explicitly recommend it). Adopt channel-based communication (`mpsc`/`broadcast`) rather than shared-state locking. Define cancellation contracts explicitly.

### 2.2 🔴 Token Counting/Budgeting Is Under-Specified (5/5)

**Flagged by:** All 5 models

All models noted that different backends use different tokenizers, and the design provides no strategy for accurate, cross-backend token counting. Budget enforcement assumes accuracy that isn't guaranteed.

**Consensus recommendation:** Implement a fallback chain: exact backend tokenizer → local approximate tokenizer → character-count heuristic. Include a safety margin (e.g., 10% budget reserve). Record which estimation method was used in the `ContextPlan`.

### 2.3 🔴 Error Taxonomy Is Absent (4/5)

**Flagged by:** GLM 5.1, Qwen 3.6 Plus, MiMo V2 Pro, (Gemini 3 Flash via race condition discussion)

Most models independently proposed an `OzoneError` enum with subsystem-specific variants. The design describes failure modes but never defines how errors are modeled in code, what's recoverable vs. fatal, or how errors surface to users.

**Consensus recommendation:** Define a top-level `OzoneError` enum early, covering Persistence, Inference, Assembly, Memory, and Config error categories. Each error should specify: user visibility level, retry policy, and fallback behavior.

### 2.4 🔴 Persistence/Schema Details Are Missing (4/5)

**Flagged by:** GLM 5.1, Qwen 3.6 Plus, MiMo V2 Pro, Trinity Large Thinking

The design says "SQLite" but doesn't specify: schema versioning, migration strategy, file organization (one DB per session vs. global), what data is persisted vs. ephemeral, or crash recovery semantics.

**Consensus recommendation:** Use SQLite in WAL mode (3/5 recommend this specifically). Define a `SchemaVersion` table and mandate transactional, reversible migrations. Decide on one-database-per-session vs. global early — **GLM 5.1** specifically recommends per-session DBs for simpler deletion, export, and concurrency.

### 2.5 🔴 TUI Layout Specification Is Missing (4/5)

**Flagged by:** GLM 5.1, Qwen 3.6 Plus, MiMo V2 Pro, Trinity Large Thinking

Multiple models noted that the design describes views (chat, inspector, memory browser, branch viewer, command palette) but never specifies how they're arranged, what's visible at startup, focus management, or minimum terminal size.

**Consensus recommendation:** Create at least an ASCII wireframe before writing code. Both **GLM 5.1** and **MiMo V2 Pro** provided example layouts showing a status bar, split chat/inspector, input area, and token budget display. Use `ratatui` + `crossterm` (4/5 recommend this stack).

### 2.6 🔴 Keybinding/Input Model Is Missing (4/5)

**Flagged by:** GLM 5.1, Qwen 3.6 Plus, MiMo V2 Pro, Gemini 3 Flash

For a terminal application, keyboard navigation *is* the product. No default bindings, no customization strategy, no modal vs. modeless decision, and no input mode specification exist.

**Consensus keybinding overlap** (proposed independently by 2+ models):
| Action | Binding | Models |
|--------|---------|--------|
| Send message | Enter | GLM, MiMo |
| Newline in input | Alt+Enter / Ctrl+Enter | GLM, MiMo |
| Command palette | Ctrl+P | GLM, MiMo |
| Context inspector | Ctrl+I | GLM, MiMo |
| Cancel generation | Ctrl+C | GLM, MiMo |
| Branch viewer | Ctrl+B | GLM, MiMo |
| Swipe navigation | Ctrl+←/→ | MiMo |

All recommend keybindings be user-configurable via config file.

### 2.7 🟡 Configuration System Is Under-Specified (4/5)

**Flagged by:** GLM 5.1, Qwen 3.6 Plus, MiMo V2 Pro, Trinity Large Thinking

No config format, file location, hierarchy, or hot-reload strategy is defined.

**Consensus recommendation:** TOML format (3/5 recommend), XDG-compliant paths (`~/.config/ozone/config.toml`), layered hierarchy (global → session → CLI flags). **MiMo V2 Pro** uniquely distinguishes between immutable-at-runtime settings (DB path, backend URL) vs. hot-reloadable ones (theme, retrieval weights).

### 2.8 🟡 Retrieval Scoring Weights Must Be Configurable (5/5)

**Flagged by:** All 5 models

The hardcoded weights (`0.35, 0.25, 0.20, 0.20`) were universally criticized. All models agreed they should be per-session or per-character configurable, with proper normalization (all terms clamped to `[0, 1]`, weights summing to 1).

### 2.9 🟡 Security Model Is Absent (3/5)

**Flagged by:** GLM 5.1, Qwen 3.6 Plus, MiMo V2 Pro

Character cards and lorebooks are untrusted user input that could carry prompt injection. API key storage, backend communication security, and data-at-rest encryption are unaddressed.

**Consensus recommendation:** Treat imported cards/lorebooks as untrusted. Lorebook entries should be scoped to soft context and cannot override hard context. API keys stored via OS keychain when available. SQLite databases should use restrictive file permissions (0600).

### 2.10 🟡 Testing Strategy Is Absent (3/5)

**Flagged by:** GLM 5.1, Qwen 3.6 Plus, MiMo V2 Pro

**Consensus test types:**
- **Property-based tests** for ContextAssembler (budget invariants, hard context never dropped)
- **Snapshot tests** for ContextPlan serialization
- **Integration tests** for Conversation Engine (branching, swipes, concurrent access)
- **Mock backends** for deterministic CI
- **TUI render tests** via `ratatui::backend::TestBackend`

### 2.11 🟡 Performance Targets Are Missing (3/5)

**Flagged by:** GLM 5.1, Trinity Large Thinking, MiMo V2 Pro

Without concrete numbers, "performance" is untestable. The closest to consensus targets:

| Metric | GLM 5.1 | MiMo V2 Pro |
|--------|---------|-------------|
| TUI frame time | < 33ms | — |
| Context assembly | < 500ms (8K budget, 1K msgs) | — |
| Startup time | < 2s | < 500ms |
| Memory usage | < 200MB (10K-msg session) | < 256MB |
| Max concurrent jobs | — | 4 |

---

## 3. Partial Consensus — Notable Recommendations (2–3 LLMs Agree)

### 3.1 Branch Model Needs Clarification (3/5)

**Flagged by:** GLM 5.1, Gemini 3 Flash, MiMo V2 Pro

The message tree (`parent_id: Option<MessageId>`) and the `Branch` struct (with `root_message_id` / `head_message_id`) represent the same data differently. GLM recommends clarifying that a Branch is just a named bookmark/path through the message tree. Gemini recommends a **Closure Table** or **Nested Sets** for efficient SQLite traversal.

### 3.2 Context Inspector Needs Detailed UX Design (3/5)

**Flagged by:** GLM 5.1, Qwen 3.6 Plus, MiMo V2 Pro

The flagship feature is described in prose but has no interaction design. Consensus points:
- Split-pane view (assembled prompt on left, omitted items with reasons on right)
- Budget bar showing used/total tokens
- Diff-based view between turns (**Qwen** unique)
- "Force include" action for omitted items (**GLM** unique)

### 3.3 Onboarding/First-Run Experience Is Missing (2/5)

**Flagged by:** GLM 5.1, Trinity Large Thinking

A first-time user needs guided setup: backend configuration, character card import, and feature discovery. Without this, the product is usable only by people who already understand the design document.

### 3.4 Accessibility Needs Attention (2/5)

**Flagged by:** GLM 5.1, Qwen 3.6 Plus

Both recommend: "No information conveyed by color alone" as a design principle. **GLM** proposes a three-tier color system (monochrome → 16-color → truecolor). **Qwen** proposes `--plain` mode stripping ANSI for screen readers.

### 3.5 Milestone 1 Scope (5/5 directional agreement)

All models agree on the same core v0.1 scope, though they phrase it differently:

1. Canonical Conversation Engine + SQLite persistence
2. Deterministic Context Assembler with basic policies
3. TUI shell with chat view + context inspector
4. Capability-aware backend layer (at least 2 backends)
5. Basic memory (pinned memory, simple retrieval)
6. Swipes and branches with full CRUD
7. Configuration system with sensible defaults

**Defer to v0.2:** Importance scoring, semantic embeddings, group chat, thinking summaries, advanced retrieval heuristics.

---

## 4. Unique Ideas and Outliers (1 LLM Only)

These are novel suggestions from individual models. Some are brilliant; all deserve consideration even though they weren't independently corroborated.

### From Gemini 3 Flash

| Idea | Description |
|------|-------------|
| **Context Sandbox / Dry-Run** | Before submitting a prompt, let the user preview the ContextPlan. If a favorite lorebook entry was cut, they can manually unpin a message to make room *before* wasting tokens. |
| **WASM Plugin Interface for Tier C** | Instead of building "Lore Gap Detection" into core, provide a WASM plugin system. Users write small Rust/AssemblyScript snippets to analyze transcripts without bloating the binary. |
| **BM25 + Vector Hybrid Retrieval** | Standard embeddings fail on specific names (e.g., "Xylo-7"). Use keyword (BM25) + semantic (vector) hybrid search. For RP, exact sword names matter more than "something similar to a weapon." |
| **"Snapshot" / Universe File** | A `git-commit`-style feature to save a specific state of the branch tree, lorebook, and character card as a single shareable "Universe File." |
| **GPU Mutex for Single-GPU Systems** | No background jobs (summaries, embeddings) should hit the GPU while the foreground chat pipeline is active. The Task Orchestrator needs hardware-aware scheduling. |
| **`fastembed-rs` for CPU Embeddings** | Run embeddings on CPU via fastembed-rs to avoid GPU contention entirely. |
| **Vim-Diff Swipe Comparator** | Side-by-side diff-style comparison of swipe candidates in the TUI. |

### From GLM 5.1

| Idea | Description |
|------|-------------|
| **Closure Table for Branching** | Use a Closure Table or Nested Sets pattern in SQLite for the message tree, enabling efficient full-branch retrieval in a single query without recursive CTEs. |
| **SwipeGroup References Parent Context** | SwipeGroup should reference the *message the assistant was responding to*, not just the user message. This handles edge cases: editing user messages after swiping, multi-turn regeneration, group chat ambiguity. |
| **Full Reproducibility in GenerationRecord** | Add `context_plan_id`, `model_identifier`, `sampling_params` (actual values, not just preset name), and `seed` to GenerationRecord. Without these, the reproducibility goal is aspirational. |
| **Context Assembly Order Should Be Data-Driven** | Define a `ContextLayerPolicy` struct making the assembly layer ordering configurable and testable, not hardcoded. Each layer gets `required`, `max_budget_pct`, and `min_budget_pct`. |
| **One SQLite DB Per Session** | Simplifies deletion, export, backup, and concurrent access. Character cards stored as JSON files on disk for interoperability. |
| **SillyTavern-Compatible Export as Priority** | The largest existing RP frontend ecosystem; compatibility matters for adoption. |

### From Trinity Large Thinking

| Idea | Description |
|------|-------------|
| **Simplified Default Context Assembly** | For v1, use a conservative default: system prompt → character card → pinned memory → last 10 messages → author's note. Let advanced users customize via config. Don't overwhelm with 10+ layers initially. |
| **Memory Storage Tiering** | Recent: full artifacts. Older: summaries + embeddings only. Very old: session synopsis and key memories only. Add storage usage indicator and automatic cleanup policies. |
| **Group Chat MVP Enhancement** | Even in Phase 1, add: mention-based speaker auto-detection, simple relationship hints in context, and a "narrator" toggle for scene descriptions. Makes initial group chat feel complete without major complexity. |
| **Thinking Block as Explicit Opt-In** | Make elicited thinking strictly opt-in with model-specific warnings (e.g., "works well with MythoMax, may degrade with Llama 3"). |
| **Concrete Backpressure Numbers** | Max 3 concurrent background jobs, queue size limit of 20, auto-cancel stale jobs after 5 minutes, execute background jobs only when app is idle. |
| **Multiple UI Modes** | "Minimal" (immersive RP), "Standard" (balanced), "Developer" (full transparency). Plus layout presets: "Immersive," "Debugging," "Memory Review." |
| **"Safe Mode"** | Disable all assistive features (Tier B/C) for troubleshooting. |
| **SQLite FTS for Retrieval** | Use SQLite's built-in Full-Text Search rather than custom retrieval solutions. |

### From Qwen 3.6 Plus

| Idea | Description |
|------|-------------|
| **Event Sourcing** | Append-only event streams (`MessageCommitted`, `BranchCreated`, `ContextPlanGenerated`) naturally align with the canonical transcript model. Enables deterministic replay, debugging, and easy SQLite replication. |
| **`usearch` or `tantivy` for Vector Storage** | Lightweight, disk-backed vector indices to avoid loading entire embedding matrices into RAM. Contradicts the "low-overhead" thesis if done in-memory. |
| **Provenance Decay** | Auto-generated summaries lose 15% weight for every retrieval cycle without user interaction. Prevents stale AI-generated content from dominating retrieval results over time. |
| **StaleArtifactDetector** | Flags embeddings/summaries older than N messages or T hours. UI shows `⚠ stale` rather than silently omitting. |
| **Explicit Garbage Collection Policies** | `MAX_ACTIVE_EMBEDDINGS`, `ARCHIVE_AFTER_N_TURNS`, `PURGE_UNREF_BACKLOG`. |
| **Streaming Think-Block Parser** | Use `nom` or a parser-combinator state machine to detect `<think>`/`</think>` boundaries during streaming without buffering the full response. Emit UI events in real-time. |
| **Background Compaction** | Periodically merge stale embeddings, clear orphaned MemoryArtifact rows, and regenerate SessionSynopsis without blocking foreground generation. |
| **Terminal Width-Aware Wrapping** | Wrap at the `Message::content` boundary, not the rendering layer, to preserve copy-paste integrity. |

### From MiMo V2 Pro

| Idea | Description |
|------|-------------|
| **Input Mode State Machine** | Define `InputMode` enum: `Normal`, `Command` (after `/`), `AuthorNote`, `SystemInject`, `Search` (Ctrl+R). Explicit state transitions prevent keybinding collisions. |
| **GenerationState Enum** | `Streaming { tokens_so_far }`, `Committed { message_id }`, `Cancelled { partial, reason }`, `Failed { error }`. Makes the generation lifecycle explicit and debuggable. |
| **Cancellation Contract** | User-initiated cancellation always honored. Partial generation on cancel becomes a *discarded swipe*, not a committed message. Background job cancellation is best-effort with timeout. |
| **Workspace Crate Structure** | `ozone-core`, `ozone-context`, `ozone-memory`, `ozone-inference`, `ozone-tasks`, `ozone-tui`, `ozone-cli`. Maps architecture to Rust workspace naturally. |
| **PersistenceLayer Trait** | Formal trait with `commit_message`, `commit_branch`, `store_artifact`, `store_context_plan` — distinguishing between durable canonical data, durable-but-regenerable artifacts, and ephemeral inspection data. |
| **TokenEstimationPolicy Enum** | `ExactBackendTokenizer`, `LocalApproximateTokenizer`, `CharacterCountHeuristic` — with confidence windows recorded in the ContextPlan. |
| **SecurityLevel Enum** | `None` (local trusted), `FilePerm` (0600), `Encrypted` (SQLCipher), `Authenticated` (remote token). Progressive security posture. |
| **Hot-Reload vs. Immutable Config** | Distinguish settings that can change at runtime (theme, retrieval weights) from those requiring restart (DB path, backend URL). |

---

## 5. Points of Divergence

### 5.1 SQLite Organization

- **GLM 5.1:** One database per session (simpler deletion, export, concurrency)
- **MiMo V2 Pro:** Asks the question but doesn't commit to an answer
- **Others:** Don't specify

### 5.2 Embedding Strategy

- **Gemini 3 Flash:** CPU-only via `fastembed-rs` to avoid GPU contention
- **Qwen 3.6 Plus:** Disk-backed vector index (`usearch`/`tantivy`) for scalability
- **GLM 5.1:** User-configurable, default `all-MiniLM-L6-v2` via ONNX, stored as BLOBs in SQLite
- **Trinity Large Thinking:** SQLite FTS as simpler alternative to custom vector search

### 5.3 Severity of UX Gaps

- **GLM 5.1** and **MiMo V2 Pro** rated UX specification as a critical blocker (★★★☆☆, "Cannot build UI without layout model")
- **Trinity Large Thinking** was more forgiving, rating the overall approach as sound with polish needed
- **Gemini 3 Flash** focused on specific UX features rather than systemic gaps

### 5.4 Group Chat Phase 1 Assessment

- **Trinity Large Thinking:** Phase 1 "might feel underwhelming"; suggests enhancing MVP with narrator toggle and mention-based detection
- **All others:** Phase 1 approach is pragmatic and correct as-is

---

## 6. Recommended Dependency Stack (Consensus)

| Concern | Recommendation | Models Agreeing |
|---------|---------------|-----------------|
| Language | Rust 1.75+ | All 5 |
| TUI Framework | `ratatui` | Gemini, GLM, Qwen, MiMo |
| Terminal I/O | `crossterm` | GLM, Qwen, MiMo |
| Async Runtime | `tokio` | Gemini, GLM, Qwen, MiMo |
| SQLite | `rusqlite` (MiMo) or `sqlx` (Gemini) | Split |
| HTTP Client | `reqwest` | Gemini, MiMo |
| Serialization | `serde` | Qwen, MiMo |
| Config Format | TOML | GLM, MiMo, Qwen |

---

## 7. Priority Matrix (Synthesized Across All Models)

### 🔴 Critical — Before Any Code

| # | Item | Models Flagging |
|---|------|----------------|
| 1 | Specify concurrency model (tokio, channels, cancellation) | All 5 |
| 2 | Define token counting fallback chain | All 5 |
| 3 | Create TUI wireframe / layout model | GLM, Qwen, MiMo, Trinity |
| 4 | Define error type taxonomy | GLM, Qwen, MiMo |
| 5 | Write persistence schema v1 + migration strategy | GLM, Qwen, MiMo |
| 6 | Select dependencies (ratatui, tokio, rusqlite, crossterm) | MiMo, Qwen |
| 7 | Define input model and default keybindings | GLM, MiMo |

### 🟡 High — During Milestone 1

| # | Item | Models Flagging |
|---|------|----------------|
| 8 | Make retrieval weights configurable | All 5 |
| 9 | Design context inspector UX in detail | GLM, Qwen, MiMo |
| 10 | Define configuration system (TOML, layered, hot-reload) | GLM, Qwen, MiMo |
| 11 | Clarify branch model (tree vs. named-path) | GLM, Gemini, MiMo |
| 12 | Add GenerationRecord reproducibility fields | GLM, MiMo |
| 13 | Define performance targets with concrete numbers | GLM, Trinity, MiMo |
| 14 | Design onboarding / first-run experience | GLM, Trinity |

### 🟠 Medium — Before Milestone 2

| # | Item | Models Flagging |
|---|------|----------------|
| 15 | Testing strategy (property, snapshot, integration, TUI) | GLM, Qwen, MiMo |
| 16 | Security model (untrusted imports, API keys, file perms) | GLM, Qwen, MiMo |
| 17 | Accessibility (no color-only info, plain mode) | GLM, Qwen |
| 18 | SillyTavern-compatible export format | GLM |
| 19 | Embedding model strategy (local vs. API, storage format) | GLM, Qwen |
| 20 | Background job notification UX | GLM, Trinity |

---

## 8. The Killer Unique Ideas Worth Highlighting

These are the most compelling suggestions from individual models that weren't echoed elsewhere but deserve serious consideration:

1. **Context Sandbox / Dry-Run** *(Gemini)* — Preview your ContextPlan before spending tokens. This directly solves the #1 user frustration in context-limited RP.

2. **Event Sourcing** *(Qwen)* — Append-only event streams align naturally with the canonical transcript philosophy and unlock deterministic replay for free.

3. **Provenance Decay** *(Qwen)* — Auto-summaries losing weight over time without user interaction prevents stale AI content from dominating retrieval. Elegant self-correcting mechanism.

4. **BM25 + Vector Hybrid Retrieval** *(Gemini)* — Keyword search for exact names + semantic search for concepts. Essential for RP where "Xylo-7" must match exactly, not approximately.

5. **Data-Driven Context Assembly Order** *(GLM)* — `ContextLayerPolicy` with per-layer budget percentages makes assembly configurable, testable, and user-adjustable instead of hardcoded.

6. **GPU Mutex / Hardware-Aware Scheduling** *(Gemini)* — Critical for single-GPU users: no background inference jobs while the foreground chat pipeline is active.

7. **Input Mode State Machine** *(MiMo)* — Explicit `InputMode` enum prevents the keybinding collision nightmare that plagues terminal apps.

8. **Memory Storage Tiering** *(Trinity)* — Recent conversations get full artifacts; older ones get summaries only; very old sessions keep just a synopsis. Prevents unbounded storage growth.

9. **Multiple UI Modes** *(Trinity)* — "Minimal" for immersive RP, "Standard" for regular use, "Developer" for full transparency. Serves all user types without overwhelming any of them.

10. **Cancellation → Discarded Swipe** *(MiMo)* — Partial generation on cancel becomes a discarded swipe candidate rather than a committed message or lost data. Clean lifecycle semantics.

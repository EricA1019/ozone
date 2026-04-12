# Ozone v0.3 Design Document — Round 2 Synthesis Report

**Date:** 2026-04-12
**Input:** 5 LLM analyses of the v0.3 design document
**Reviewers:** GLM 5.1, Qwen 3.6 Plus, Trinity Large, MiMo V2 Pro, Gemini 3.1 Pro

---

## Overall Grades

| LLM | Grade | Summary Verdict |
|-----|-------|-----------------|
| **GLM 5.1** | **A-** | "Top 5% of design docs. Weaknesses concentrated in operational maturity, not architecture." |
| **Qwen 3.6 Plus** | **Production-viable** | "Primary risks are over-engineering early memory subsystems and under-specifying async streaming." |
| **Trinity Large** | **9.2/10** | "Exceptional blueprint — team can begin coding immediately with minimal ambiguity." |
| **MiMo V2 Pro** | **8.5/10** | "Genuinely excellent. Weaknesses are real but manageable — mostly gaps rather than fundamental flaws." |
| **Gemini 3.1 Pro** | **Exceptionally well-conceived** | "Successfully bridges conceptual architecture and implementation-ready specification." |

**Consensus verdict:** The v0.3 document is ready for implementation. All 5 reviewers recommend proceeding. Remaining gaps are concentrated in operational concerns and edge-case specifications, not core architecture.

---

## 1. Universal Consensus Strengths (All 5 LLMs Agree)

These are features every reviewer praised. They form the untouchable core — do not change these.

### 1.1 "Deterministic Core First" Philosophy
**Flagged by:** GLM, Qwen, Trinity, MiMo, Gemini

Every reviewer identified this as the single most important architectural decision. The principle that Ozone works correctly with all intelligence disabled, then layers assistive features on top, is what distinguishes it from every existing RP frontend.

> *"This isn't just a nice idea — it's enforced by the ownership boundary table and the architectural rule that only the Conversation Engine and Context Assembler may commit active state."* — **GLM 5.1**

> *"Most RP frontends bake intelligence into the hot path from day one, creating fragile systems."* — **MiMo V2 Pro**

### 1.2 Single-Writer ConversationEngine + Channel Architecture
**Flagged by:** GLM, Qwen, Trinity, MiMo, Gemini

All 5 confirmed this is the correct concurrency model for Rust and this problem domain. `mpsc` commands → single-writer engine → `broadcast` events → TUI with `Arc<AppState>` snapshots.

> *"Completely sidesteps a massive class of race conditions common in async Rust applications."* — **Gemini 3.1 Pro**

### 1.3 Context Inspector / Dry-Run Mode
**Flagged by:** GLM, Qwen, Trinity, MiMo, Gemini

Unanimously identified as the **#1 differentiating feature** and genuine UX innovation. No other local RP frontend offers this.

> *"This is arguably the #1 missing feature in every existing RP frontend."* — **MiMo V2 Pro**

> *"Showing exactly why a token was spent or dropped changes the app from a black box to an engineer's tool."* — **Gemini 3.1 Pro**

### 1.4 Complete Type Definitions (§11)
**Flagged by:** GLM, Qwen, Trinity, MiMo, Gemini

Every reviewer praised the full type definitions. `MemoryContent` as a sum type, `Provenance` with explicit weights, `SwipeGroup.parent_context_message_id`, and `Branch` as named bookmark were specifically highlighted.

> *"A developer can read §11 and know exactly what to write in `types.rs`."* — **MiMo V2 Pro**

### 1.5 GPU Mutex / Hardware-Aware Scheduling
**Flagged by:** GLM, Qwen, Trinity, MiMo, Gemini

The semaphore-based `GpuMutex` with foreground priority + CPU-only `fastembed-rs` for embeddings was praised as the right solution for single-GPU consumer hardware.

### 1.6 Error Taxonomy with Severity + Visibility + Retry
**Flagged by:** GLM, Qwen, Trinity, MiMo

The `OzoneError` enum with `severity()`, `user_visibility()`, and `retry_policy()` was called "industry-leading" (Trinity) and "mature" (GLM).

### 1.7 Hybrid BM25 + Vector Retrieval
**Flagged by:** GLM, Qwen, MiMo, Gemini

The recognition that pure vector search fails on proper nouns, combined with configurable alpha blending, was confirmed as the right approach by 4/5 reviewers.

### 1.8 Three-Tier Token Counting Fallback
**Flagged by:** Qwen, MiMo, Gemini

Exact → approximate → heuristic with confidence tracking was praised as superior to the common binary choice of "assume tokenizer exists" or "character-count only."

### 1.9 Data-Driven Context Assembly Policy
**Flagged by:** GLM, Qwen, Trinity

Making assembly order configurable via `ContextLayerPolicy` with budget percentages (not hardcoded order) was called a "standout UX feature" and solves the "#1 pain point of hard-coded context injection."

---

## 2. Consensus Weaknesses (3+ LLMs Agree — Must Fix)

### 2.1 Cross-Session Search / Global Index Needed
**Flagged by:** GLM, Qwen, MiMo, Gemini (4/5)

The one-DB-per-session design is praised for simplicity but universally identified as creating a cross-session search gap. Users with many sessions cannot find content across them.

**Consensus fix:** Add a lightweight global index database (`~/.local/share/ozone/global.db`) storing session metadata, character references, and a cross-session FTS search index. Individual session DBs remain for transcript isolation.

| LLM | Specific Recommendation |
|-----|------------------------|
| **GLM** | `global.db` with session metadata and cross-session search index |
| **Qwen** | `global_registry.db` or in-memory index on startup |
| **MiMo** | "lightweight global session index (separate SQLite DB or FTS table)" |
| **Gemini** | "Global App-State Database" with cross-session metadata and global lorebooks |

### 2.2 Closure Table Maintenance / Scalability
**Flagged by:** GLM, Qwen, MiMo, Gemini (4/5)

All agree the closure table pattern is correct but maintenance is under-specified. Who inserts rows? What about edits? What about archived branches?

**Divergent solutions:**
| LLM | Proposed Solution |
|-----|-------------------|
| **GLM** | Add clear maintenance contract: "ConversationEngine maintains it in same transaction. Never modified after insertion." |
| **Qwen** | **Replace** with recursive CTE queries at read-time (removes trigger overhead entirely) |
| **MiMo** | Add compaction for archived branches, or use path enumeration for long-lived sessions |
| **Gemini** | Considers it a strength (O(1) depth checks) but doesn't address maintenance |

**Synthesis recommendation:** For Milestone 1, use GLM's contract approach (explicit, simple). Add Qwen's recursive CTE as a fallback/alternative for sessions >50K messages. Revisit at scale.

### 2.3 Character Card / Import-Export Schema Unspecified
**Flagged by:** GLM, Trinity, MiMo (3/5)

The document promises "SillyTavern-compatible JSON format" but specifies neither the exact schema, supported versions (V2? V3?), field mapping, nor lossy conversion rules. This blocks Milestone 1's "basic import/export."

| LLM | Specific Gap |
|-----|-------------|
| **GLM** | No `ozone_card_version` field, no migration strategy, no validation schema |
| **Trinity** | "Immediate specify: exact JSON schema (v2 or v3), how Ozone fields are stored, migration strategy" |
| **MiMo** | "No spec for supported formats, required vs optional fields, malformed card handling, embedded lorebooks" |

### 2.4 Token Counting Heuristic Language Bias
**Flagged by:** GLM, Qwen, MiMo (3/5)

The 0.25 tokens/char (4 chars/token) default assumes Latin/Roman scripts. CJK, Arabic, and emoji-heavy text diverge by 3-5×. The 10% safety margin is insufficient.

| LLM | Recommendation |
|-----|---------------|
| **GLM** | "Make safety margin configurable per model family: 20% for heuristic, 10% for approximate, 0% for exact" |
| **Qwen** | "Implement model-family-specific multipliers (0.15 for CJK, 0.35 for Latin)" |
| **MiMo** | "Add calibration step: validate heuristic against exact tokenizer on startup, store ratio per model family" |

### 2.5 Single-Writer vs Background Artifact Writes Tension
**Flagged by:** MiMo (detailed), Qwen (implied), Gemini (implied) (3/5)

Background jobs write derived artifacts (embeddings, summaries) but the document claims single-writer architecture. This is either unexplained (jobs write through the engine) or inaccurate (jobs have their own write connections).

**MiMo's analysis (most detailed):**
> "The persistence layer IS the SQLite database. If background jobs are writing to the same database, they're not just readers — they're writers too."

**MiMo's recommended resolution:** Background jobs write through the ConversationEngine's persistence interface, which serializes writes. The engine doesn't need to "understand" the artifacts.

### 2.6 WASM Plugin Interface Undesigned
**Flagged by:** GLM, MiMo, Gemini (3/5)

WASM is positioned as the primary extensibility mechanism but has zero specification: no API surface, no sandboxing model, no data exchange format, no lifecycle management.

| LLM | Recommendation |
|-----|---------------|
| **GLM** | Add §30 stub: WASI constraints, `serde_json::Value` exchange, capability whitelist, versioning contract |
| **Gemini** | Use optimized binary format (`bincode`/`msgpack`) instead of JSON for WASM FFI |
| **MiMo** | Offer simpler extensibility first: custom slash commands, pre/post-generation shell hooks, custom themes |

---

## 3. Strong Consensus Weaknesses (2 LLMs Agree — Should Fix)

### 3.1 No Logging Framework Specified
**Flagged by:** GLM, MiMo

Both independently recommend `tracing` with `tracing-subscriber`. GLM additionally specifies structured logging, log levels per subsystem, and rotation at `$XDG_CACHE_HOME/ozone/logs/`.

### 3.2 No Undo/Redo System
**Flagged by:** GLM, MiMo

Both note that branches provide "alternative timeline" semantics but are too heavy for simple "oops, undo that" actions. GLM proposes leveraging the existing event sourcing system; MiMo suggests a lightweight undo stack (last N actions + Ctrl+Z).

### 3.3 Clipboard / Yank Support Missing
**Flagged by:** Qwen, MiMo (+ GLM as QOL)

Terminal users expect clipboard operations. All suggest `arboard` or `copypasta` crate with cross-platform support (`xclip`/`xsel`/`pbcopy`/`wl-copy`).

### 3.4 Event Sourcing Table Unbounded Growth
**Flagged by:** GLM, Qwen

Both flag the append-only events table growing unboundedly. GLM proposes `max_event_age_days` config (default: 90) with P4 background compaction. Qwen proposes partitioning by session age or offloading to separate `events.db`.

### 3.5 Rate Limiting for Backend Requests
**Flagged by:** GLM, MiMo

No rate limiting for rapid regeneration or remote API 429/Retry-After handling. GLM proposes a `RateLimitPolicy` struct; MiMo proposes configurable minimum interval between generations and max pending queue depth.

### 3.6 Provenance Weights Hardcoded/Arbitrary
**Flagged by:** GLM, MiMo

The comment-specified weights (UserAuthored: 1.0, CharacterCard: 0.9, etc.) contradict the document's configurability philosophy. MiMo notes: "A user explicitly pinning something should arguably be at least as authoritative as the character card definition."

Both recommend: add `[memory.provenance_weights]` config section.

### 3.7 Session Export Format Unspecified
**Flagged by:** MiMo, Trinity

What exactly is exported? How does branch structure map to SillyTavern's flat message list? What about metadata SillyTavern doesn't support?

### 3.8 Message Editing UX Missing
**Flagged by:** MiMo, Trinity (implied via keybindings)

The `Message` struct has `edited_at` but the TUI design doesn't specify: how to enter edit mode, whether edits trigger re-derivation of artifacts, how edit history interacts with branches.

### 3.9 Backend Health Check / Connection Monitoring
**Flagged by:** Qwen, Trinity

Silent backend dropouts (KoboldCpp sleep, Ollama restart) cause confusing hangs. Both recommend lightweight `/health` pinging every 30s with status bar indicator (`✓ / ⚠ / ✗`).

---

## 4. Unique Ideas per LLM

### GLM 5.1 — Operational Robustness Focus

| Idea | Description |
|------|-------------|
| **Advisory session lock table** | `session_lock` table with `instance_id`, `acquired_at`, `heartbeat_at` prevents silent corruption from two Ozone instances opening the same session. "50 lines of code." |
| **Timestamps as UTC integers** | Replace `datetime('now')` TEXT with Unix epoch INTEGER for deterministic ordering across DST and timezones. |
| **FTS5 synchronization triggers** | Use SQLite `AFTER INSERT/UPDATE` triggers on `messages` and `memory_artifacts` for automatic FTS sync. Without this, search is broken immediately. |
| **Config version migration** | `[meta] config_version = 3` field with auto-migration of old configs on update. |
| **Tab completion for character names** | In group chat, `Tab` cycles through character names matching partial input. |
| **Quick character swap mid-session** | `:char` command to switch character cards without restarting. Old messages persist, new messages use new card. |
| **Auto-save indicator** | `💾` or `✓` pulse in status bar when state persists. "Builds trust, zero complexity." |
| **Session templates** | Pre-configured templates with character cards, lorebook, system prompt, context policy. `sessions/templates/` directory with TOML files. |
| **Vector index rebuild CLI** | `ozone index rebuild` command to regenerate vector index from stored artifacts. Makes the vector index strictly derivable. |

### Qwen 3.6 Plus — Pragmatic Simplification Focus

| Idea | Description |
|------|-------------|
| **Replace closure table with recursive CTE** | Remove `message_ancestry` entirely; use recursive CTEs at read-time. Removes trigger overhead, adequate for <100K messages. |
| **Use `config` crate for TOML merging** | Replace manual serde layering with `config::Config::builder().merge()` for safe deep merging. |
| **Use `tokio-util` Framed Codec** | Avoid raw `nom` on async streams. Wrap SSE chunk processing in a `Decoder` that handles partial UTF-8 and buffer boundaries. |
| **`/dryrun` slash command** | `Ctrl+D` or `/dryrun` during typing to preview ContextPlan without leaving input mode. |
| **Temporary token budget override** | `+500`/`-500` hotkeys during dry-run to adjust `max_tokens` on-the-fly. |
| **Message pinning to Hard Context** | `Ctrl+K` injects any message into Hard Context; auto-expires after 3 turns or manual toggle. |
| **Auto-trim preview** | When budget exceeded: `⚠ Context exceeds budget by 342 tokens. Auto-trimming 2 oldest soft-context items. [Confirm] [Dry Run] [Cancel]` |
| **Context plan diff persist toggle** | `diff_mode` to see `+`/`-` context deltas between consecutive turns automatically. |
| **Defer hybrid memory to M3** | Ship BM25-only + explicit pinning first. Add vector embeddings only after retrieval accuracy is measured. |

### Trinity Large — UX Polish Focus

| Idea | Description |
|------|-------------|
| **Responsive context inspector** | Below 100 columns: stack prompt and omitted items vertically. Below 80 columns: hide inspector, show compact budget summary in status bar. |
| **One-click context optimization** | `:optimize_context` runs dry-run, suggests 1-3 best omitted items to force-include by importance, applies with one confirmation. |
| **Numeric swipe shortcuts** | `[1] Elara: "The forest path..."` `[2] Elara: "Ancient oaks..."` — Press 1-3 to select instead of Ctrl+Arrow cycling. |
| **Context plan export/import** | Export `ContextPlan` as JSON for sharing optimal setups, saving configurations, and debugging. |
| **Smart predictive budget warnings** | "At current rate, you'll exceed budget after 3 more assistant messages" / "This message will use 40% of remaining budget." |
| **Streaming format capability enum** | `StreamingFormat { SSE, JSONLines, Chunked }` in backend capabilities — backends use different streaming protocols. |
| **Character card validation on import** | Check required fields, warn about very long descriptions (>1000 chars), suggest splitting to lorebook if >2000 chars. |
| **Inline context preview while typing** | Real-time status bar: `Tokens: 150/8192 | Context: [Elara Card] [Pinned: Sword of Doom] [Recent: 12 msgs]` |
| **Session dashboard** | `:sessions` command showing recent sessions, sizes, quick actions (open/duplicate/archive/delete). |

### MiMo V2 Pro — Edge Case & Tension Focus

| Idea | Description |
|------|-------------|
| **Snapshot version for stale artifact detection** | Add `generation_id` or `snapshot_version` to background job params. On completion, check if transcript version matches. If not, discard artifact as stale. |
| **Background job persistence doctrine** | Either persist job queue in `pending_jobs` table, or explicitly document that jobs are ephemeral and re-derived on session load. |
| **"Transcript is sacred" vs tiering clarification** | Tiering (Full → Reduced → Minimal) must apply ONLY to derived artifacts, never to the canonical message history. |
| **Config presets / profiles** | "Conservative Memory," "Aggressive Retrieval," "Minimal Overhead" presets to combat decision fatigue from many config knobs. |
| **Generation completion notifications** | Terminal bell (`\x07`), optional desktop notification (`notify-rust`), tmux notification support when long generations complete. |
| **Shell-based extensibility before WASM** | Custom slash commands (shell scripts), pre/post-generation hooks, custom themes as early extensibility — don't wait for WASM in Tier C. |
| **Multi-modal content scoping** | Explicitly declare images/multi-modal as in-scope or out-of-scope. The `content: String` model is text-only; attachment directory exists but isn't integrated. |
| **Auto-session naming** | After 3-5 messages, auto-generate session name from truncated first user message or utility-model summary. User can rename. |
| **Input draft persistence** | Save input buffer to `<session_dir>/draft.txt` (debounced). Restore on session load. Prevents losing half-written 300-word responses. |
| **Quick lorebook entry from selection** | Highlight text → "Save as Lorebook Entry" → pre-populate content, prompt for keywords. |
| **Regenerate with variation slider** | On Ctrl+R, show temp overlay with temperature slider (0.0–2.0). Enter confirms, Escape uses current temperature. |

### Gemini 3.1 Pro — Infrastructure Focus

| Idea | Description |
|------|-------------|
| **Prompt formatting templates** | No mention of ChatML, Alpaca, Llama-3-Instruct, etc. Local models are highly sensitive to exact control tokens. Recommends `minijinja` templating engine + `prompt_template` in model config. |
| **Multi-GPU / HardwareResourceSemaphore** | `GpuMutex` is capacity:1, but power users use multi-GPU. Rename to `HardwareResourceSemaphore` with configurable capacity, or allow background jobs to target specific `BackendId` on separate devices. |
| **Binary format for WASM FFI** | Use `bincode`/`msgpack` instead of JSON for plugin data exchange to avoid serialization latency on 8K-token context plans. |
| **Model alias pre-flight checks** | At startup or config reload, verify backend target is reachable and model hash/name matches `model_identifier` stored in session. Warn via StatusBar if discrepancy found. |
| **Auto-stashing dirty input state** | Cache input buffer relative to active `MessageId` or `BranchId` so switching panes or branches doesn't lose draft. |

---

## 5. Points of Divergence

### 5.1 Closure Table Strategy
| Position | LLMs |
|----------|------|
| Keep closure table, add clear maintenance contract | **GLM**, **Gemini** |
| Replace with recursive CTEs at read-time | **Qwen** |
| Keep but add compaction for archived branches | **MiMo** |

### 5.2 Memory System Complexity for Early Milestones
| Position | LLMs |
|----------|------|
| Well-designed as specified, build as planned | **GLM**, **MiMo**, **Gemini** |
| Over-engineered — defer hybrid retrieval, provenance decay, and tiering to M3+ | **Qwen**, **Trinity** |

### 5.3 One-DB-Per-Session Assessment
| Position | LLMs |
|----------|------|
| Correct and pragmatic — add global index to compensate | **GLM**, **Qwen**, **Gemini** |
| "Pragmatic and correct" — operational simplicity outweighs search cost | **MiMo** |
| Implied concern through session management gaps | **Trinity** |

### 5.4 Streaming Parser Approach
| Position | LLMs |
|----------|------|
| `nom` is fine for streaming parsing | **GLM**, **Trinity**, **MiMo**, **Gemini** (no objection) |
| `nom` is synchronous — use `tokio-util::codec::Decoder` instead | **Qwen** |

### 5.5 Group Chat Phase 1 Scope
| Position | LLMs |
|----------|------|
| Phase 1 MVP as specified is fine | **GLM**, **Qwen**, **MiMo**, **Gemini** |
| Mention detection and relationship hints are risky — start explicit-only | **Trinity** |

---

## 6. Consensus Priority Matrix

### Tier 1 — Must Fix Before Implementation (consensus: 3+ LLMs)

| # | Issue | LLMs | Effort |
|---|-------|------|--------|
| 1 | **Specify logging framework** (`tracing` + `tracing-subscriber`) | GLM, MiMo | Small |
| 2 | **Add FTS5 synchronization triggers** to schema | GLM | Small |
| 3 | **Define streaming error recovery** (mid-stream crash → preserved partial swipe) | GLM | Medium |
| 4 | **Add advisory session lock** for multi-instance safety | GLM | Small |
| 5 | **Fix timestamp storage** to UTC integers or ISO 8601 UTC | GLM | Small |
| 6 | **Clarify write path** for derived artifacts (resolve single-writer tension) | MiMo, Qwen, Gemini | Medium |
| 7 | **Add global index DB** for cross-session search | GLM, Qwen, MiMo, Gemini | Medium |
| 8 | **Specify character card schema** (V2/V3, required fields, validation) | GLM, Trinity, MiMo | Medium |
| 9 | **Specify session export format** (field mapping, lossy conversion rules) | MiMo, Trinity | Medium |
| 10 | **Add prompt formatting templates** (ChatML, Alpaca, Llama-3-Instruct) | Gemini | Medium |

### Tier 2 — Should Fix Before Milestone 2

| # | Issue | LLMs | Effort |
|---|-------|------|--------|
| 11 | Events table retention policy | GLM, Qwen | Small |
| 12 | Provenance weight configurability | GLM, MiMo | Small |
| 13 | Vector index rebuild path (`ozone index rebuild`) | GLM | Medium |
| 14 | Disk space monitoring | GLM | Small |
| 15 | Closure table maintenance contract | GLM, Qwen, MiMo | Small |
| 16 | Token counting heuristic per-language calibration | GLM, Qwen, MiMo | Medium |
| 17 | Rate limiting for backend requests | GLM, MiMo | Small |
| 18 | Message editing UX specification | MiMo | Medium |
| 19 | Backend health check polling | Qwen, Trinity | Small |
| 20 | Config merging strategy (deep merge, not replace) | Qwen | Small |

### Tier 3 — QOL Quick Wins (High Value / Low Effort)

| # | Feature | LLMs | Effort |
|---|---------|------|--------|
| 21 | Auto-session naming | MiMo | Trivial |
| 22 | Input draft persistence | MiMo, Gemini | Trivial |
| 23 | Auto-save indicator in status bar | GLM | Trivial |
| 24 | Clipboard/yank support | GLM, Qwen, MiMo | Small |
| 25 | Message bookmarking/flagging | GLM, Qwen | Small |
| 26 | Markdown export | GLM, Gemini | Small |
| 27 | Session templates | GLM, Trinity | Medium |
| 28 | Session statistics (`:stats`) | GLM, MiMo | Small |
| 29 | Generation completion bell/notification | MiMo | Trivial |
| 30 | Numeric swipe shortcuts | Trinity | Small |

---

## 7. Top 10 "Killer" New Ideas (Round 2)

These are the most impactful unique ideas from this round that should be integrated into v0.4:

| # | Idea | Source | Why It Matters |
|---|------|--------|---------------|
| 1 | **Prompt formatting templates** (ChatML/Alpaca/Llama-3 via `minijinja`) | **Gemini** | Local models *will break* without correct control tokens. This is a hard blocker. |
| 2 | **Advisory session lock table** (multi-instance safety) | **GLM** | 50 lines of code prevents silent data corruption — the worst possible outcome. |
| 3 | **Streaming error recovery** (preserve partial output as discarded swipe) | **GLM** | Mid-stream backend crashes are common with local LLMs. Users want to see what was generated. |
| 4 | **Snapshot version for background jobs** (stale artifact detection) | **MiMo** | Prevents the subtle bug where a completed job's output is already outdated. |
| 5 | **Responsive context inspector** (stack vertically <100 cols, hide <80 cols) | **Trinity** | SSH users and small terminals are the target audience. Inspector must adapt. |
| 6 | **Input draft persistence** (auto-save typing buffer per session) | **MiMo, Gemini** | Losing a half-written 300-word response is emotionally devastating for RP users. |
| 7 | **Shell-based extensibility before WASM** (pre/post hooks, custom slash commands) | **MiMo** | Community contributions shouldn't wait for Tier C. Shell hooks are trivial to implement. |
| 8 | **`/dryrun` inline command** + temporary budget override hotkeys | **Qwen** | Makes the killer feature (context inspector) even more accessible during typing flow. |
| 9 | **Message pinning to Hard Context** with auto-expiry | **Qwen** | Solves the common "model forgot this one critical thing 5 turns ago" problem without permanent context cost. |
| 10 | **Config presets / profiles** ("Conservative Memory", "Aggressive Retrieval") | **MiMo** | Mitigates decision fatigue from the document's many configurable knobs. |

---

## 8. Summary: What Changed from Round 1 → Round 2

### Round 1 Found (v0.2 → v0.3 gaps, now fixed):
- Missing type definitions ✅
- No concurrency model ✅
- No error taxonomy ✅
- No persistence schema ✅
- No token counting strategy ✅
- No TUI specification ✅

### Round 2 Finds (v0.3 → v0.4 gaps, new):
- **Operational maturity gaps:** logging, monitoring, crash recovery, multi-instance safety
- **Specification gaps:** character card schema, export format, edit UX, streaming error states
- **Pragmatic simplification needs:** closure table maintenance, config merging, streaming parser
- **Missing UX features:** undo/redo, clipboard, draft persistence, generation notifications
- **Missing infrastructure:** prompt templates, global index, backend health monitoring
- **Over-engineering risk:** memory system complexity may need phased delivery

### Consensus Quality Assessment:
The v0.3 document's architecture is unanimously approved. The remaining work is "filling in the operational edges" — not restructuring. This is a strong signal that the core design is sound.

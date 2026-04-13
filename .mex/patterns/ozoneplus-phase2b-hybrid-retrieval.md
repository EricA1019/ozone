---
name: ozoneplus-phase2b-hybrid-retrieval
description: Pattern for Phase 2B embeddings and hybrid retrieval - provider fallback, vector-index rebuilds, hybrid search, stale filtering, and RetrievedMemory context injection.
applies_to: ["crates/ozone-memory/src/*", "crates/ozone-persist/src/repository.rs", "apps/ozone-plus/src/main.rs", "apps/ozone-plus/src/runtime.rs", "apps/ozone-plus/src/context_bridge.rs", "apps/ozone-plus/src/hybrid_search.rs"]
---

# Phase 2B: Embeddings and Hybrid Retrieval

## Optional Embeddings First

Do **not** make ozone+ depend on embeddings to function.

Baseline behavior must remain:
- `memory.embedding.provider = "disabled"` by default
- normal CLI and TUI flows still work through explicit FTS-only fallback
- embedding-dependent work is opt-in and inspectable

## Provider and Index Seams

Reuse the explicit seams added in Phase 2B instead of creating hidden side channels:
- `ozone_memory::EmbeddingProvider` for `disabled`, `mock`, and feature-gated `fastembed`
- `ozone_memory::VectorIndexManager` for on-disk `memory.usearch` + JSON metadata under `<data_dir>/vector-index/`
- `ozone-plus index rebuild` as the explicit refresh path

Do **not** hide index refresh behind startup hooks or silent background jobs.

## Persistence Shape

Reuse `memory_artifacts` for embedding storage.

Rules:
- store embeddings as `MemoryContent::Embedding`
- persist provider/model/dimensions metadata alongside vectors
- use deterministic artifact IDs or replacement rules so rebuilds replace current embeddings instead of leaking duplicates
- keep `snapshot_version` and `source_text_hash` available for stale detection

## Hybrid Ranking

Hybrid search should stay inspectable.

Minimum behavior:
- fuse BM25/FTS and vector similarity with `memory.hybrid_alpha`
- apply `memory.retrieval_weights`
- apply `memory.provenance_weights`
- surface per-hit score breakdowns, provenance, source state, and search mode

Use typed score/result structs rather than app-local tuples or freeform maps.

## Stale Handling

Do not silently treat old embeddings as fresh truth.

Compare stored embedding state against current source state:
- `snapshot_version`
- `source_text_hash`
- current message/memory content

Then:
- filter or down-rank stale embeddings explicitly
- include stale-filter/down-rank counts in status text
- preserve a visible distinction between current and stale sources

## Search and Context Surfaces

CLI:
- `ozone-plus index rebuild`
- `ozone-plus search session <session-id> <query>`
- `ozone-plus search global <query>`

Runtime:
- `/search session ...`
- `/search global ...`
- recall browser stays in the existing inspector/status surfaces

Context:
- use `ContextLayerKind::RetrievedMemory` for top retrieved hits
- derive the retrieval query from the latest user turn during generation/dry run
- keep active pinned memories as separate hard context

## Fallback Rule

If embeddings are disabled, unavailable, the index is missing, or metadata is incompatible:
- do **not** fail normal search/context behavior
- fall back to FTS-only recall
- show the fallback mode/reason explicitly in status or summary text

## Validation

Required validation for this pattern:
- `cargo test -p ozone-memory --quiet`
- `cargo test -p ozone-persist --quiet`
- `cargo test -p ozone-plus --quiet`
- `cargo test -p ozone-tui --quiet`
- `cargo check -p ozone-plus --quiet`
- `cargo clippy -p ozone-memory --all-targets -- -D warnings`
- `cargo clippy -p ozone-persist --all-targets -- -D warnings`
- `cargo clippy -p ozone-plus --all-targets -- -D warnings`

Smoke expectations:
- temp-XDG search works in FTS-only mode before rebuild
- `index rebuild` succeeds with a configured mock or real provider
- session/global search switch to hybrid mode after rebuild

## PTY Note

Full-screen TUI verification is still limited by PTY automation.

For Phase 2B:
- rely on CLI smoke for fallback -> rebuild -> hybrid search
- rely on cargo tests for detailed runtime/context retrieved-memory coverage when TUI output is noisy

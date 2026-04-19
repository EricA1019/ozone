---
name: llamacpp-backend-integration
description: Add or extend llama.cpp model-import, runtime, and base-launcher support across Ozone.
triggers:
  - "llama.cpp"
  - "llamacpp"
  - "ozone model add --hf"
  - "llama-server"
  - "llama-cli"
  - "add backend support"
edges:
  - target: "context/architecture.md"
    condition: when deciding whether a change belongs in ozone-core, ozone-inference, apps/ozone-plus, or the root launcher
  - target: "ozoneplus-streaming-backend-runtime.md"
    condition: when the change touches ozone+ inference, gateway dispatch, or runtime behavior
  - target: "ozoneplus-phase1g-launcher-onramp.md"
    condition: when the base launcher or ozone+ handoff path also needs to change
last_updated: 2026-04-18
---

# llama.cpp Backend Integration

## Context

- Ozone now uses llama.cpp in two different ways:
  1. `ozone model add --hf` downloads through llama.cpp and then links the
     resolved GGUF into `~/models/`
  2. ozone+ / base Ozone can run against a real `llamacpp` backend
- The product still treats `~/models/` as the local model inventory root even
  though llama.cpp stores `-hf` downloads in the Hugging Face cache.
- The right seam is shared discovery + backend descriptors, not parallel
  one-off wrappers.

## Steps

1. Put stable defaults in `crates/ozone-core`:
   - default llama.cpp base URL / ready URL
   - shared path helpers for logs if needed
2. Keep root-binary executable discovery in the root app:
   - `src/llama.rs` owns `llama-cli` / `llama-server` discovery
   - honor `OZONE_LLAMACPP_CLI` and `OZONE_LLAMACPP_SERVER`
   - keep failures explicit when the binaries are missing
3. For model import in `src/model.rs`:
   - keep `ozone model add --hf` stable
   - run llama.cpp's HF flow
   - resolve the downloaded GGUF from the HF cache
   - symlink it into `~/models/` so existing list/info/picker flows keep working
4. For ozone+ runtime support:
   - add a `llamacpp` backend descriptor + client in `crates/ozone-inference`
   - extend the stream decoder if llama.cpp emits a slightly different SSE body
   - refactor the gateway so it dispatches by `backend.type`
   - expose backend-neutral probes like max-context inspection through the gateway
5. For base-launcher support:
   - add `BackendMode::LlamaCpp`
   - surface it in Settings, badges, services, and launching copy
   - start `llama-server` from `src/processes.rs`
   - wire base -> ozone+ handoff with `OZONE__BACKEND__TYPE=llamacpp`
6. Update docs and `.mex` memory in the same task so later sessions do not
   keep assuming ozone+ is KoboldCpp-only.

## Gotchas

- llama.cpp's `-hf` flow downloads into the Hugging Face cache, not `~/models/`.
- Native llama.cpp `/completion` SSE payloads do not exactly match KoboldCpp's
  token shape; extend the decoder explicitly instead of relying on luck.
- The current ozone+ runtime still does **not** support `Ollama + ozone+`; do
  not remove that guardrail while adding llama.cpp.
- This environment may not expose `llama-server` / `llama-cli` on `PATH`; the
  UX needs clean override/env guidance, not silent fallback behavior.

## Verify

- `cargo test -p ozone-inference --quiet`
- `cargo test -p ozone-plus --quiet`
- `cargo test -p ozone --quiet`
- `cargo clippy -p ozone-inference --all-targets --quiet -- -D warnings`
- `cargo clippy -p ozone-plus --all-targets --quiet -- -D warnings`
- `cargo clippy -p ozone --all-targets --quiet -- -D warnings`
- `cargo test --workspace --quiet`

## Debug

- If ozone+ still behaves as if it is KoboldCpp-only, trace:
  `BackendDescriptor` -> `InferenceGateway` -> `InferenceAdapter` -> runtime warning/probe calls.
- If the launcher shows `LlamaCpp` but cannot start it, inspect:
  `src/llama.rs` discovery, env overrides, and `~/.local/share/ozone/llamacpp.log`.
- If `ozone model add --hf` succeeds but the model picker cannot see the file,
  inspect the final symlink target in `~/models/` rather than the HF cache first.

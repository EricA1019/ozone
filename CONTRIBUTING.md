# Contributing to Ozone

Thanks for wanting to improve Ozone. This is a focused tool family — contributions should stay within that scope.

## Before opening a PR

- **Check existing issues first.** If there isn't one for your change, open one so we can discuss before you write code.
- **Keep it focused.** One fix or feature per PR. If your change touches five unrelated things, split it up.
- **Test on real hardware.** Ozone's core value is correct mixed-memory planning on an actual NVIDIA + RAM setup. Changes to `src/planner.rs`, `src/hardware.rs`, or `src/processes.rs` need to be tested against a real model, not just compiled.

## What's in scope

- Bug fixes in any tier (ozonelite, ozone base, ozone+)
- Hardware compatibility (different GPU counts, AMD ROCm support)
- New preset/benchmark format improvements
- Monitor metrics and display improvements
- CLI ergonomics for any ozone command
- ozone+ memory, session, or context assembly improvements

## What's out of scope

- Support for vLLM, llama.cpp-direct, or other inference backends beyond KoboldCpp and Ollama — this needs an abstraction layer first; open an issue to discuss
- GUI or web frontend
- Breaking changes to the preset, benchmark, or session file formats without a migration path

## Workspace structure

The project is a Cargo workspace:

```
src/                        # ozone base (launcher, profiling, bench, sweep)
  main.rs, planner.rs, hardware.rs, processes.rs, ...
apps/
  ozone-plus/               # ozone+ binary entry point
crates/
  ozone-core/               # shared product metadata and path helpers
  ozone-engine/             # conversation engine, context assembly
  ozone-inference/          # backend adapters, config, prompt templates
  ozone-memory/             # memory types, embeddings, retrieval
  ozone-persist/            # session and global databases, schema migrations
  ozone-tui/                # shared terminal UI shell and render layer
```

## Development

```bash
cargo build                            # debug build (all crates)
cargo build --workspace --release      # release build
./contrib/sync-local-install.sh        # release build + checksum-aware local install sync
cargo clippy --workspace --all-targets # lints
cargo test --workspace                 # all tests
```

Build just one target:

```bash
cargo build -p ozone            # base launcher only
cargo build -p ozone-plus       # ozone+ binary only
```

The project uses stable Rust. No nightly features.

## Code style

- Run `cargo clippy` before submitting. Fix all warnings.
- No `unwrap()` in paths that can fail at runtime — use `?` or log and continue.
- Keep `unsafe` out unless there's a real reason.
- Comments only where the code doesn't speak for itself.

## Commit messages

Use the conventional commit format:

```
<type>(<scope>): short summary in present tense

Optional longer body explaining why, not what.
```

Types: `fix`, `feat`, `refactor`, `docs`, `chore`
Scopes: `profiling`, `launcher`, `tui`, `memory`, `persist`, `inference`, `engine`, `core`

Examples:

```
fix(profiling): trim leading whitespace from ps output when parsing PIDs
feat(launcher): add ROCm GPU memory detection via rocm-smi
fix(memory): normalize FTS query terms to avoid hyphen operator parse errors
docs(readme): document CPU-only mode and OZONE_KOBOLDCPP_LAUNCHER
```

## Pull request checklist

Before marking a PR ready for review:

- [ ] `cargo clippy --workspace` passes with no warnings
- [ ] `cargo test --workspace` passes
- [ ] Tested against a real model on real hardware (for anything touching planner/hardware/processes/inference)
- [ ] Commit messages follow the format above
- [ ] PR description explains what changed and why

## License

By contributing, you agree that your code will be licensed under the [MIT License](LICENSE).

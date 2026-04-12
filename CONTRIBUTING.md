# Contributing to Ozone

Thanks for wanting to improve Ozone. This is a small focused tool — contributions should stay within that scope.

## Before opening a PR

- **Check existing issues first.** If there isn't one for your change, open one so we can discuss before you write code.
- **Keep it focused.** One fix or feature per PR. If your change touches five unrelated things, split it up.
- **Test on real hardware.** Ozone's core value is correct mixed-memory planning on an actual NVIDIA + RAM setup. Changes to `planner.rs`, `hardware.rs`, or `processes.rs` need to be tested against a real model, not just compiled.

## What's in scope

- Bug fixes
- Hardware compatibility (different GPU counts, AMD ROCm support, etc.)
- New preset/benchmark format improvements
- Monitor metrics and display improvements
- CLI ergonomics

## What's out of scope

- Support for non-KoboldCpp backends (vLLM, llama.cpp direct, etc.) — this would need its own abstraction layer; open an issue to discuss first
- GUI or web frontend
- Breaking changes to the preset or benchmark file formats without a migration path

## Development

```bash
cargo build          # debug build
cargo build --release
cargo clippy         # lints
cargo test           # unit tests (if any)
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
<type>: short summary in present tense

Optional longer body explaining why, not what.
```

Types: `fix`, `feat`, `refactor`, `docs`, `chore`

Examples:
```
fix: trim leading whitespace from ps output when parsing PIDs
feat: add ROCm GPU memory detection via rocm-smi
docs: document bench-results.txt format
```

## Pull request checklist

Before marking a PR ready for review:

- [ ] `cargo clippy` passes with no warnings
- [ ] Tested against a real model on real hardware (for anything touching planner/hardware/processes)
- [ ] Commit messages follow the format above
- [ ] PR description explains what changed and why

## License

By contributing, you agree that your code will be licensed under the [MIT License](LICENSE).

---
name: lite-build-cfg-gating
description: Pattern for gating eval-specific code behind `#[cfg(feature = "eval")]` to support `--no-default-features` (lite) builds.
---

# Lite Build CFG Gating

When adding code that depends on the `eval` feature:

## Gate the type/function definition
```rust
#[cfg(feature = "eval")]
pub fn eval_only_fn() { ... }
```

## Gate struct fields
```rust
pub struct MyState {
    pub always_present: u32,
    #[cfg(feature = "eval")]
    pub eval_only: Option<Receiver<...>>,
}
```

## Gate enum variants
If the variant is only used within eval code, it's better to **not** gate the variant itself (this breaks match exhaustiveness in other files). Instead:
1. Gate the match arms at the usage sites
2. Add a wildcard catch-all arm `#[cfg(not(feature = "..."))] _ => unreachable!()`

## Gate match arms on Screen enum
```rust
Screen::Normal => normal::render(f, &app),
#[cfg(feature = "eval")]
Screen::BenchEval => bench_eval::render(f, &app),
#[cfg(not(feature = "eval"))]
_ => unreachable!(),
```

## Gate `matches!` macro patterns
`matches!` does NOT support `#[cfg]` on individual patterns. Use separate `||` chains instead:
```rust
let need_refresh = matches!(app.screen, Screen::A | Screen::B)
    || {
        #[cfg(feature = "eval")]
        { matches!(app.screen, Screen::C) }
        #[cfg(not(feature = "eval"))]
        { false }
    };
```

## Gate imports
Gate the import itself if it's only used in gated code:
```rust
#[cfg(any(feature = "profiling-ui", feature = "eval"))]
use tokio::sync::mpsc::error::TryRecvError;
```

## Module gating
```rust
#[cfg(feature = "eval")]
mod bench_eval;
```

## Verification
Always verify both builds after gating:
```bash
cargo build --workspace                    # default features
cargo build --no-default-features -p ozone  # lite build
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

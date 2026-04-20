# Versioning Conventions

## Format
`MAJOR.MINOR.PATCH-CHANNEL+GITHASH`

- **CHANNEL**: `alpha` → `beta` → stable (no suffix)
- **GITHASH**: embedded at build time via `build.rs` — uniquely identifies the exact commit
- **PATCH** may exceed 9 (e.g., `0.4.10`, `0.4.11` are valid — no rollover to 0.5)

## Channel Meanings
| Channel | Meaning |
|---------|---------|
| `alpha` | Active development, frequent changes, may be unstable |
| `beta` | Feature-complete, polished, under final validation. **0.5.0 is the first beta.** |
| (none) | Stable release — tested and recommended for general use |

## When to Bump PATCH

**DO bump** when:
- A sprint lands a named, user-visible feature set (e.g., "session folders", "ozone-lite kernel")
- A breaking behavioral change lands, even if small
- A significant refactor changes the public UX surface

**DO NOT bump** when:
- Single bugfix, typo fix, or documentation change
- CI/tooling changes or internal refactors with no UX impact
- Mid-sprint incremental work — the `+GITHASH` suffix already differentiates these

## When to Bump MINOR
- **0.5.x is reserved exclusively for first public beta**
- MINOR bumps only happen at milestone releases
- Do not use `0.5.x` for alpha work, no matter how many PATCH versions accumulate

## Version Bump Checklist
Before bumping the version in any Cargo.toml:
- [ ] `cargo test` passes (all crates, 0 failures)
- [ ] `make install` succeeds and both binaries run
- [ ] CHANGELOG entry written for this sprint/feature-set
- [ ] All related PRs/branches merged to `dev` first

## Example Timeline
| Version | Trigger |
|---------|---------|
| `0.4.5-alpha` | Settings crash fix + theme presets + editable settings sprint |
| `0.4.7-alpha` | ozone-lite kernel + branch setup + versioning rules sprint (skips 0.4.6 — never shipped) |
| `0.4.8-alpha` | Session folders sprint (planned) |
| `0.4.9-alpha` or `0.4.10-alpha` | ozone-note plugin sprint (planned) |
| `0.5.0-beta` | First public beta — feature-complete, polished, no known blockers |

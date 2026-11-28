# Brick

Brick is an open source stop motion program. It runs on Linux, Windows, and MacOS. The supported package types for Linux are deb and rpm. The program is primarily designed to support DSLRs, specifically those made by Canon. But webcams are supported as well.

## Features

All features must have a spec written for it with a spec number. Currently all specs are in @docs/program.md but in future as the program grows the specs will be broken into several files.

## Commits

Follow @docs/commits.md for writing git commits.

## Validation

Before committing, run the same checks CI runs (`.github/workflows/ci.yml`) and make sure they all pass:

```sh
cargo fmt --all --check
RUSTFLAGS="-D warnings" cargo clippy --workspace --all-targets --locked
RUSTFLAGS="-D warnings" cargo test --workspace --locked
cargo xtask licences --check   # R6
```

- Warnings are errors in CI, so fix every compiler and clippy warning.
- These checks only cover the host platform. CI also builds Windows and macOS, where platform backends such as V4L2 are compiled out. Code that is only used from `#[cfg(target_os = ...)]` code becomes dead code on the other platforms and fails the build, so check every item a platform gated module uses.
- If dependencies change, also confirm the minimum supported Rust version (`rust-version` in `Cargo.toml`) still builds: `cargo +<rust-version> check --workspace --all-targets --locked`.
- New behavior needs tests. Use the simulated camera (C9) to test camera paths without hardware.
- Code in `crates/brick-v4l2` must document every `unsafe` block (`clippy::undocumented_unsafe_blocks`, R4).

## Common Pitfalls

- Brick is under active development. Do not add backward-compatibility code for obsolete settings, persisted data, or behavior unless a task explicitly requires it. Prefer removing obsolete paths and updating the current format.

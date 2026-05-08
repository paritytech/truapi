---
name: rust-checks
description: Run the full Rust workspace verification suite (build, fmt, clippy, test) for the TrUAPI workspace. Use after any change to rust/crates/* before declaring a Rust change done.
---

# Rust workspace static checks

Mirrors step 1 of `docs/local-e2e-testing.md`. Run from the repo root:

```bash
cargo build --workspace --all-targets --all-features
cargo +nightly fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

Each command must finish with `Finished` / `test result: ok`. Treat any
warning as a failure (clippy is `-D warnings` already; build warnings are
not, watch them).

## Iteration shortcut

If you only touched `truapi` types/traits, scoping to that crate is fine
during iteration:

```bash
cargo build -p truapi --all-features
cargo test  -p truapi --all-features
```

Always run the workspace-wide commands once before declaring done — the
codegen and macro crates depend on `truapi`.

## Failure modes

- `cargo +nightly` missing → install with `rustup toolchain install nightly`
  (also needed for codegen rustdoc JSON).
- rust-analyzer flags errors but `cargo build` is clean → ignore the editor
  diagnostic. The authoritative source is `cargo`.

## Codegen self-tests

If you touched `truapi-codegen`:

```bash
cargo test -p truapi-codegen --all-features
```

Covers wire-table property tests and `detect_versioned_wrapper` regressions.

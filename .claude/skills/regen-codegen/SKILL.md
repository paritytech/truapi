---
name: regen-codegen
description: Regenerate the @parity/truapi TypeScript client and playground/explorer metadata from the truapi crate's rustdoc JSON. Use whenever the Rust trait surface changes.
---

# Regenerate the TypeScript client

Mirrors step 2 of `docs/local-e2e-testing.md`. Skip if your change is
purely Rust-internal (e.g. versioned wrapper conversion logic that does
not change rustdoc output) or purely TS-side.

```bash
./scripts/codegen.sh
```

Expected output: `Generated client at js/packages/truapi/src/generated/`,
`Generated playground metadata ...`, `Generated explorer registry ...`.

The script is `cargo +nightly rustdoc --output-format json` →
`truapi-codegen` → `prettier --write` → `npm run build` in
`js/packages/truapi`.

## Verifying the result

```bash
git status js/packages/truapi/src/generated \
           js/packages/truapi/src/playground \
           js/packages/truapi/src/explorer
git diff   js/packages/truapi/src/generated/
```

The diff shape should match the Rust diff — new methods produce new
client stubs and wire-table entries. Commit the regenerated files
alongside the Rust changes.

## Failure modes

- Missing nightly toolchain → install with `rustup toolchain install nightly`.
- `unresolved link to ...` warnings from rustdoc break codegen quietly:
  `truapi-codegen` will still emit, but you may miss an item in the
  generated TS. Fix by turning the link into a fully-qualified path
  (`super::T`, `crate::vXY::T`) or dropping the link, then rerun.
- `truapi-codegen` panics on an unsupported rustdoc shape → the trait
  added a Rust feature codegen does not know how to project. Extend
  `truapi-codegen` rather than working around it in the trait.

## Refreshing the playground snapshot afterwards

`scripts/codegen.sh` rebuilds `dist/` for `@parity/truapi` but does NOT
refresh `playground/node_modules/@parity/truapi`. Run the
`refresh-playground-snapshot` skill next.

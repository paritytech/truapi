# dotli shared Rust core migration

**Status:** clean-sheet draft.  
**Base:** TrUAPI `origin/main` plus the runtime substrate proposed in PR 104.  
**Parity target:** current dotli main at `4611008` (2026-06-09), with the
older `~/github/dotli` checkout at `85c9733` used only as historical evidence
where the latest submodule has already removed Nova code.

This directory describes the migration from dotli's current
`@novasamatech/host-container` stack to a shared Rust core. It intentionally
stays at the design/API level: enough to split the work, review the host
contract, and check feature parity without repeating every byte-level protocol
detail.

## Source Inputs

- **TrUAPI main:** existing product wire traits and generated clients.
- **PR 104:** Rust dispatcher, `truapi-platform`, WASM/UniFFI/native bridge
  substrate, and the current unsupported-account/signing/statement-store gaps.
- **dotli main:** current web behavior implemented in
  `packages/ui/src/container.ts`, `packages/auth/src/auth.ts`, and supporting
  auth/signing/storage modules.

The older documents in `docs/design/host-contract-and-core-impl/` are useful
research notes, but this directory is a fresh spec. If the two conflict, this
directory should be treated as the cleaner migration plan.

## Docs

| Doc | Purpose |
|---|---|
| [01 - Target Architecture](<01-target-architecture.md>) | The target topology, ownership boundary, and data-flow diagrams. |
| [02 - dotli Parity Contract](<02-dotli-parity-contract.md>) | What "feature parity with dotli main" means and what remains intentionally unavailable. |
| [03 - Host Platform API](<03-host-platform-api.md>) | New or changed host capabilities required by the shared Rust core. |
| [04 - Core Services](<04-core-services.md>) | Core-owned services that replace the Nova JS runtime packages. |
| [05 - Migration Plan](<05-migration-plan.md>) | Workstreams, review slices, and acceptance gates. |

## Non-Goals

- Do not design a second wallet transport. Pairing and post-pairing messages use
  the SSO protocol over the People-chain statement store.
- Do not introduce independent nested-product Rust runtimes for v1. Nested
  dApp compatibility is a dotli adapter concern unless a future product
  contract explicitly requires different identity/storage semantics.
- Do not port unimplemented dotli payment or full account-proof behavior as a
  parity blocker.
- Do not preserve host-papp's storage format. One-time re-pair on cutover is
  acceptable.
- Do not treat the older V1 metadata-URL SSO QR as current-dotli parity.
  Current dotli main uses host-papp 0.8.6 SSO V2 proposals with host metadata
  entries, platform type/version, and `rootEntropySource`.

## Success Criteria

The migration is complete when:

- dotli products use `@parity/truapi` clients against `truapi-server` rather
  than `@novasamatech/host-container`;
- current dotli main product-visible behavior is preserved, except explicitly
  unavailable payment/full-proof surfaces;
- the same Rust core and generated API surface can be embedded through WASM
  for web/Electron and UniFFI for iOS/Android;
- dotli no longer has runtime dependencies on `@novasamatech/host-api`,
  `host-container`, `host-papp`, `statement-store`, `sdk-statement`, or
  `storage-adapter`.

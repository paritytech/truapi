# Deep Repo Review TODO

Read-only review performed for the `integrate-truapi-next` PR. This file captures the findings and suggested cleanup work. No code changes were made as part of the review itself.

## Highest Priority

- [x] Fix TypeScript wire-table code generation.
  - `truapi-codegen` now emits `wire-table.ts` alongside `types.ts`, `client.ts`, and `index.ts`.
  - The TypeScript wire-table generator validates missing IDs, duplicate IDs, and ID overflow before writing output.
  - Focused Rust unit tests cover sorted TS entries, duplicate ID rejection, and missing annotation rejection.
  - A broader repo-level generated-output freshness command is still tracked separately under Build / Tooling.
  - Relevant files:
    - `rust/crates/truapi-codegen/src/typescript.rs`
    - `rust/crates/truapi-codegen/src/rust_dispatcher.rs`
    - `scripts/codegen.sh`
    - `js/packages/truapi/src/generated/wire-table.ts`

- [ ] Fix the GitHub Actions deploy command.
  - The deploy job sets `defaults.run.working-directory: playground`, but the retry action command also runs `cd playground && ...`.
  - This likely deploys from `playground/playground`, which does not exist.
  - Relevant file: `.github/workflows/deploy.yml`

- [ ] Fix playground request payloads that are marked as no-parameter even though the generated client requires arguments.
  - `host_payment_top_up`, `host_payment_request`, `host_payment_status_subscribe`, `host_derive_entropy`, and `host_chat_create_simple_group` need real default payloads instead of `noParams: true`.
  - The generated client maps these to method signatures that require arguments.
  - Relevant files:
    - `playground/src/lib/services.ts`
    - `playground/src/lib/host-api-bridge.ts`
    - `rust/crates/truapi/src/api/payment.rs`
    - `rust/crates/truapi/src/api/entropy.rs`
    - `rust/crates/truapi/src/v02/chat.rs`

- [x] Fix stale or invalid playground default payloads.
  - `host_sign_payload` and `host_sign_raw` now use `account: ProductAccountId` instead of the removed `address` field.
  - `host_create_transaction` and `host_create_transaction_with_non_product_account` now use the generated `VersionedTxPayload` tagged-union envelope.
  - `remote_statement_store_subscribe` now uses the current `TopicFilter` object shape.
  - `remote_statement_store_submit` now defaults to SCALE-encoded signed statement bytes instead of a `SignedStatement` object.
  - The updated defaults were checked against the generated codecs.
  - Relevant files:
    - `playground/src/lib/services.ts`
    - `rust/crates/truapi/src/v02/signing.rs`
    - `rust/crates/truapi/src/v02/statement_store.rs`
    - `js/packages/truapi/src/generated/types.ts`

## Outdated References

- [x] Replace links to the removed `truapi-explorer` GitHub Pages docsite.
  - Removed the playground header link instead of replacing it with another destination.
  - Replaced README links with plain text.
  - Replaced method type-hint links with inline shape descriptions.
  - Relevant files:
    - `playground/src/app/page.tsx`
    - `playground/src/lib/services.ts`
    - `playground/README.md`

- [ ] Decide whether `docs/design/host-api-protocol.md` is canonical or archival.
  - It documents methods and names not implemented in the current Rust/truapi/playground surface:
    - `host_account_get_root`
    - `host_request_login`
    - `host_get_legacy_accounts`
    - `host_create_transaction_with_legacy_account`
    - `host_sign_raw_with_legacy_account`
    - `host_sign_payload_with_legacy_account`
    - `host_theme_subscribe`
  - Current implementation exposes names such as:
    - `host_get_user_id`
    - `host_get_non_product_accounts`
    - `host_create_transaction_with_non_product_account`
  - Either update the implementation to match the design doc or mark this document as historical and add a current generated/API reference.
  - Relevant files:
    - `docs/design/host-api-protocol.md`
    - `docs/design/v02-changes.md`
    - `rust/crates/truapi/src/api/*`
    - `playground/src/lib/services.ts`

- [ ] Clean RFC/doc metadata.
  - `docs/rfcs/_index.md` has duplicate `0010` entries.
  - `playground/README.md` links to `.github/workflows/deploy.yaml`, but the file is `.github/workflows/deploy.yml`.
  - Several RFC links still point to `paritytech/triangle-js-sdks` or `paritytech/truapi-explorer`; decide whether those remain historical references or should point to local docs.
  - Relevant files:
    - `docs/rfcs/_index.md`
    - `docs/rfcs/*.md`
    - `docs/design/v02-changes.md`
    - `playground/README.md`

- [x] Update package docs that mention packages/crates not present in this pared-down repo.
  - Removed `truapi-server` and `truapi-platform` references from `rust/crates/truapi/README.md`.
  - Removed the missing `rust/crates/truapi-server/src/generated` dispatcher-output example from `rust/crates/truapi-codegen/README.md`.
  - Removed `@truapi/host-shared`, `@truapi/host-web`, and `@truapi/host-electron` references from `js/packages/truapi/README.md`.
  - Relevant files:
    - `rust/crates/truapi/README.md`
    - `rust/crates/truapi-codegen/README.md`
    - `js/packages/truapi/README.md`
    - `js/packages/truapi/test/wire-equality.test.mjs`

- [x] Update playground architecture docs.
  - Resolved: docs now describe iframe `postMessage` and webview `window.__HOST_API_PORT__` transport.
  - Resolved: TypeScript codegen emits the V1 wrapper by default for host-product-sdk/dotli compatibility.
  - Resolved: inbound host handshakes are encoded from generated codecs instead of fixed legacy bytes.
  - Relevant files:
    - `playground/CLAUDE.md`
    - `playground/src/lib/transport.ts`
    - `rust/crates/truapi-codegen/src/typescript.rs`

- [ ] Add explicit outbound wrapper version selection before emitting V2 requests.
  - `host-product-sdk` product calls send `v1` by default and only dispatch newer method versions when explicitly wired.
  - The previous "pick highest `V<N>`" codegen policy made generated calls like `host_account_get` emit `V2`, which is incompatible with hosts that only register the legacy V1 shape.
  - Keep generated product-side calls on V1 until there is a negotiated or explicit per-method version-selection API.
  - Relevant files:
    - `rust/crates/truapi-codegen/src/typescript.rs`
    - `js/packages/truapi/src/generated/client.ts`
    - `playground/src/lib/services.ts`

## Dotli / Playground Integration

- [ ] Make the intended dotli validation path explicit.
  - The repo includes `hosts/dotli` as a submodule pointer, but no top-level command verifies the playground through dotli.
  - If dotli is the required consumer, add a documented smoke workflow or script that starts dotli + playground and exercises the app.
  - If dotli is only reference context, document that to avoid treating the submodule as an actively tested dependency.
  - Relevant files:
    - `.gitmodules`
    - `hosts/dotli`
    - `README.md`
    - `CLAUDE.md`
    - `playground/README.md`

- [ ] Align playground descriptions with current generated types.
  - Some service descriptions still describe older request shapes or external doc pages.
  - The playground should ideally derive method/type docs from generated Rust doc comments to avoid manual drift.
  - Relevant files:
    - `playground/src/lib/services.ts`
    - `js/packages/truapi/src/generated/client.ts`
    - `js/packages/truapi/src/generated/types.ts`

- [ ] Decide whether the diagnostic `/page` route should stay.
  - `playground/src/app/page/page.tsx` creates a static `/page` diagnostics route.
  - It is not linked from the main playground flow and only appears in CSS and its own route.
  - Remove it if it was temporary, or document/link it if dotli navigation diagnostics are intentionally part of the app.
  - Relevant files:
    - `playground/src/app/page/page.tsx`
    - `playground/src/app/globals.css`

## Simplification / Possible Removals

- [ ] Reconsider `rust_dispatcher.rs` in this repo.
  - The dispatcher generator compiles and has tests, but there is no current `truapi-server` consumer in this repo.
  - Options:
    - Restore the runtime consumer from `~/github/truapi-next`.
    - Keep the generator but mark `--rust-output` as future/optional and remove stale path examples.
    - Remove/defer dispatcher generation if this repo is only Rust contract + generated TS client + dotli playground.
  - Relevant files:
    - `rust/crates/truapi-codegen/src/rust_dispatcher.rs`
    - `rust/crates/truapi-codegen/src/main.rs`
    - `rust/crates/truapi-codegen/README.md`

- [ ] Review whether `hosts/dotli` should remain a submodule in this repo.
  - The submodule is large and contains its own app tree, generated/vendor outputs, and independent build setup.
  - If this repo only needs dotli as a live integration target, a pinned external reference plus documented setup may be cleaner than embedding the submodule.
  - If the submodule is kept, add explicit top-level commands showing how it participates in validation.

- [ ] Consider generating or validating `playground/src/lib/services.ts`.
  - Current service metadata and `methodMap` are hand-maintained and can drift from the generated client.
  - A generated method registry from `ApiDefinition` would reduce drift and make missing bindings intentional.
  - Relevant files:
    - `playground/src/lib/services.ts`
    - `playground/src/lib/host-api-bridge.ts`
    - `rust/crates/truapi-codegen/src/typescript.rs`

- [ ] Review unused exported helpers.
  - TypeScript no-unused checks passed, so there are no obvious local unused imports/locals.
  - Some public exports exist for consumers outside this repo (`createWebSocketProvider`, `structuredCloneCodecAdapter`, `getTransport`, `isCorrectEnvironment`), so do not remove them without deciding package API boundaries.
  - Relevant files:
    - `js/packages/truapi/src/index.ts`
    - `js/packages/truapi/src/transport.ts`
    - `playground/src/lib/transport.ts`

## Build / Tooling

- [ ] Migrate playground lint scripts away from deprecated `next lint`.
  - `yarn lint` passes today but prints a deprecation warning. Next.js says `next lint` will be removed in Next 16.
  - Switch to the ESLint CLI and update `lint` / `lint:fix`.
  - Relevant file: `playground/package.json`

- [ ] Decide whether CI should run all validation surfaces.
  - Current deploy workflow only installs/builds the playground.
  - It does not run:
    - `cargo test --workspace`
    - `npm run build && npm test` for `@parity/truapi`
    - playground lint
    - codegen freshness checks
  - Add separate CI jobs or fold them into PR checks.
  - Relevant files:
    - `.github/workflows/deploy.yml`
    - `Cargo.toml`
    - `js/packages/truapi/package.json`
    - `playground/package.json`

- [ ] Add a codegen freshness command.
  - The review manually ran a temp-output comparison and found `wire-table.ts` is not generated by the current TS generator.
  - A repo command should regenerate into a temporary directory and diff against committed generated files.
  - Relevant files:
    - `scripts/codegen.sh`
    - `rust/crates/truapi-codegen/src/typescript.rs`
    - `js/packages/truapi/src/generated/*`

## Verification From Review

The following commands passed during review:

```bash
cargo test --workspace
```

```bash
cd js/packages/truapi
npm run build
npm test
```

```bash
cd playground
yarn build
yarn lint
```

```bash
cd js/packages/truapi
npx tsc --noEmit --noUnusedLocals --noUnusedParameters
```

```bash
cd playground
npx tsc --noEmit --noUnusedLocals --noUnusedParameters
```

Notes:

- `yarn lint` passed, but it uses deprecated `next lint`.
- Fresh `truapi-codegen` output differed from committed generated output because `wire-table.ts` was present only in the committed generated directory, not in fresh generator output.
- No tracked source changes were present before this review file was added.

## Extra note 

> Our redesign: Provider has postMessage, subscribe, subscribeClose?, dispose
why subscribeClose?

> Resolved handshake response versioning: `createTransport` decodes the inbound `HostHandshakeRequest` wrapper and encodes a successful `HostHandshakeResponse` with the matching versioned variant via generated codecs. Latest hosts that send V2 get a V2 response, while V1 hosts remain decodable.

> Reopened outbound wrapper selection: TypeScript codegen must default to V1 for host-product-sdk/dotli compatibility. Emitting the highest `V<N>` wrapper should wait for explicit version selection or negotiation.

>   - Connection-status events — old API has transport.onConnectionStatusChange(...). We handle
>  this in the playground's transport.ts (subscribeConnectionStatus) outside the client. If we
>  expect every consumer of @parity/truapi to need this, lifting it into the client would match
>  parity.

Not sure about this

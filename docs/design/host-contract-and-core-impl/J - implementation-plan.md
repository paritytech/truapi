# J - Implementation plan

> Part of the [host-contract & core-impl spec](<index.md>). This is the concrete build order for turning
> PR 104's shared-core runtime substrate into a dotli-compatible Rust core with no `@novasamatech`
> runtime dependencies.

## Dependency graph

```
PR 104 runtime substrate
  |
  v
RuntimeConfig + host primitive deltas
  |
  v
Crypto/vector gate -----> product-account derivation
  |                              |
  v                              v
Statement-store client ----> account reads
  |
  v
SSO pairing + SessionStore restore/logout
  |
  v
Session-channel request/response ops
  |
  +--> signing / create_transaction / alias / resource allocation
  +--> statement-store proof / submit / subscribe
  +--> entropy
  |
  v
dotli bridge deletion + Nova dependency removal
```

## Work packages

| WP | Scope | Deliverables | Verification |
|---|---|---|---|
| 0 | Lock the transition boundary from PR 104 | Mark `setActiveSession` / `clearActiveSession` and raw account/signing/statement callbacks as transitional; no new feature should depend on them | `rg "setActiveSession|clearActiveSession"` shows only compatibility/removal notes until WP 3 removes the APIs |
| 1 | Runtime construction config | Add `RuntimeConfig` to `PlatformRuntimeHost`, WASM constructors, UniFFI constructors, and JS worker init; dotli passes product id/label, site id, SSO V2 host metadata, platform type/version, People genesis, deeplink scheme | Unit tests for config validation; worker protocol test proves config reaches WASM |
| 2 | Host primitive deltas | Add `PairingPresenter`, `SessionStore`, resource allocation confirmation, theme subscription, preimage callbacks, notification id/cancel shape | `truapi-platform` bounds tests; WASM/native callback tests; generated TS/Kotlin/Swift bindings updated; same capability surface exists for WASM and UniFFI |
| 3 | Crypto/vector gate | Add narrow WASM-safe crypto module/crate; capture JS/iOS vectors for HDKD, statement proof, QR SCALE, P-256/HKDF/AES-GCM, keyed BLAKE2b topics/session ids | Native + `wasm32-unknown-unknown` vector tests pass before pairing I/O starts |
| 4 | Statement-store client | Implement People-chain `statement_submit`, `statement_subscribeStatement`, query/dump/live subscription plumbing over `ChainProvider` | Mock JSON-RPC tests for submit, historical dump, live stream, unsubscribe, topic limits |
| 5 | SSO pairing, restore, logout | Implement `request_login`, QR presentation, host-papp 0.8.6 SSO V2 proposal/response handling, `rootEntropySource` persistence, `SessionInfo` persistence through `SessionStore`, current-then-change restore, public logout/disconnect | Pairing integration test with captured fixtures or wallet test peer; corrupted blob clears; cross-runtime store tick updates; logout clears store and pending waiters |
| 6 | Account + identity methods | Implement `get_account`, `get_legacy_accounts`, `get_user_id`, connection status from real restored session | Dotli parity vectors for product account derivation; permission-gated user id tests |
| 7 | Session-channel operations | Implement sign/raw sign, legacy signing, create transaction, alias, resource allocation; preserve current dotli error mapping and 180s request timeout semantics | Mock SSO peer tests for success/failure/timeout/disconnect; signer mismatch and permission denial tests |
| 8 | Product statement-store methods | Implement `create_proof`, `create_proof_authorized`, `subscribe`, `submit` using the same client/session key | Proof vectors match `createSr25519Prover`; subscription paging preserves `is_complete` semantics |
| 9 | Tier 3.5 dotli parity | Implement entropy, notification id/cancel, theme subscription, preimage host callbacks | Dotli handler parity tests for each currently implemented handler |
| 10 | dotli integration + dependency removal | Replace `packages/ui/src/container.ts` bridge with Rust worker bridge; remove Nova packages, mappings, debug hooks, auth/session adapter, raw callback routes | `rg "@novasamatech"` has no runtime value/type imports in dotli packages; dotli login/sign/statement/preimage/manual flows pass |

## Review slicing

Keep each PR independently useful and reviewable:

1. **Config + primitive surfaces:** WPs 1-2, no SSO crypto yet.
2. **Crypto vectors:** WP 3 only; no network I/O.
3. **Statement client + pairing:** WPs 4-5; core can pair/restore/logout but not yet sign.
4. **Account/signing/message exchange:** WPs 6-7.
5. **Statement-store + Tier 3.5 parity:** WPs 8-9.
6. **dotli cutover:** WP 10, deleting JS/Nova code after the Rust path is proven.

## Acceptance gates for the milestone

- `make check`.
- `cargo test --workspace --features ws-bridge`.
- `cargo check -p truapi-server --target wasm32-unknown-unknown`.
- `./scripts/codegen.sh && git diff --exit-code` for generated dispatcher/client/callback artifacts.
- Native and wasm crypto vector tests pass.
- dotli web can pair through SSO, restore across reload, logout, sign payload/raw, create transaction,
  derive alias/account, submit/subscribe statements, create statement proofs, request resource allocation,
  derive entropy, use preimage, receive/cancel notifications, and receive theme updates.
- `truapi-server` has no dotli package imports, browser-global assumptions, host storage-key constants, or
  UI component dependencies; dotli-specific behavior is confined to the dotli adapter/runtime config.
- WASM and UniFFI expose the same core lifecycle: construct with `RuntimeConfig`, provide the same host
  primitive set, restore through `SessionStore`, pair through `PairingPresenter`, and logout through the
  core API.
- dotli no longer has runtime dependencies on `@novasamatech/host-api`, `host-container`, `host-papp`,
  `statement-store`, `sdk-statement`, or `storage-adapter`.

## Deferred by design

- Payment, Chat, CoinPayment, and full `create_account_proof` remain unavailable for current dotli parity.
- Independent nested-product runtimes/sessions remain deferred; nested traffic, if kept, uses the shared
  top-level Rust core for v1.

# F - `@novasamatech` removal checklist (dotli side)

> Part of the [host-contract & core-impl spec](<index.md>).

Source of truth for the original audit was the current dotli checkout at `~/github/dotli` (`85c9733`,
dirty worktree ignored). Implementation status below tracks the migrated `hosts/dotli` submodule on this
branch.

Current dotli depends on:

- `@novasamatech/host-api` `0.8.2`
- `@novasamatech/host-container` `0.8.2`
- `@novasamatech/host-papp` `0.7.9`
- `@novasamatech/statement-store` `0.8.2`
- `@novasamatech/sdk-statement` `^0.6.0`
- `@novasamatech/storage-adapter` `0.8.2`

Handlers that current dotli explicitly leaves unimplemented, such as payment and full
`create_account_proof`, are not blockers for this removal. They stay unavailable until another host needs
them.

## Order of removal

```
  Tier 1-3.5 land  ->  all Nova packages drop together:
                       host-api + host-container                         (Rust host wire + generated
                          TrUAPI codecs replace the JS container)
                       host-papp                                          (pairing, signing,
                          transaction construction, alias, resource
                          allocation, entropy move to core/session channel)
                       statement-store + sdk-statement                    (submit/subscribe/proof and
                          mappings move to core + @parity/truapi types)
                       storage-adapter                                    (local shared-auth storage
                          interface becomes dotli/core-defined)
```

## Checklist (exact refs)

- [x] `packages/ui/src/container.ts`: replace `createIframeProvider`, `createContainer`,
      `createRateLimiter`, `deriveProductEntropy`, `Container`, `Provider`, and all `host-api` error/type
      imports with the Rust worker bridge and generated TrUAPI codecs. The migrated submodule no longer
      has `container.ts`; `packages/ui/src/bridge.ts` uses the Rust worker bridge.
- [x] Nested dApps: do not port `setupNestedBridgeDetector` as separate Rust runtimes/sessions/product
      identities for v1. If nested message forwarding remains, route it through the shared top-level Rust
      core. Track future nested-product usefulness in [I](<I - nested-dapps.md>).
- [ ] Preserve current `container.ts` handler coverage in Rust before removing the JS handlers:
      account get/legacy/user-id/login/alias, signing, legacy signing, `create_transaction`, resource
      allocation, local storage, entropy, navigation, device/product permissions, notifications,
      statement-store submit/subscribe/proof, preimage submit/lookup, theme subscription, feature support,
      and chain connection/status.
- [x] `js/packages/truapi-host-wasm`: delete transitional raw callback routes for methods now owned by
      Rust (`accountGet`, `accountGetAlias`, `accountCreateProof` if still unavailable, `getLegacyAccounts`,
      `getUserId`, `signPayload`, `signRaw`, `statementStoreSubscribe`, `statementStoreSubmit`,
      `statementStoreCreateProof`, and any preimage route that has moved behind the final host-side
      primitive). The final bridge should expose only true platform callbacks plus `chainConnect` and
      frame transport.
- [x] `truapi-server` WASM/UniFFI surfaces: remove `setActiveSession` / `clearActiveSession` as product
      lifecycle APIs after core-owned SSO restore/logout lands. They are account-only PR-104 scaffolding
      and cannot restore `ss_secret`, P-256 key material, peer keys, or session topics.
- [x] Keep explicit unavailable behavior for current dotli-unimplemented handlers: payment and full
      `create_account_proof`.
- [x] `packages/auth/src/auth.ts`: remove `createPappAdapter`, `PappAdapter`, `PairingStatus`,
      `Identity`, `UserSession`, `createLazyClient`, `createPapiStatementStoreAdapter`,
      `StatementStoreAdapter`, `Statement`, and `toHex` once pairing/session restore/statement-store are
      core-owned. The migrated submodule deletes `packages/auth`.
- [x] dotli logout/disconnect UI: call the Rust core public logout/disconnect API. Do not clear the
      `SessionStore` directly from UI code; core owns teardown, storage clear, and `Disconnected`
      broadcast. `packages/ui/src/topbar.ts` emits `dotli:truapi-disconnect-request`,
      `packages/ui/src/bridge.ts` routes that to `coreProvider.disconnect()`, and
      `packages/ui/tests/topbar.test.ts` locks the topbar event path.
- [x] `packages/auth/src/signing.ts`: remove `UserSession` and `host-api` error/type imports once signing,
      legacy signing, and `create_transaction` are core session-channel requests. Keep or rewrite only the
      dotli confirmation modals if they remain host UI around core calls. The migrated submodule deletes
      `packages/auth`; confirmation UI lives in Rust host callbacks.
- [x] `packages/auth/src/account.ts`: port product account derivation to Rust and vector-test it. Match the
      current chain-code rule exactly: numeric junctions use SCALE `u64`, string junctions use SCALE `str`,
      values longer than 32 bytes are `blake2b(..., dkLen=32)`, and shorter values are right-padded to 32.
      The migrated submodule deletes `packages/auth`; product derivation is in `truapi-server`.
- [x] `packages/auth/src/shared-storage.ts`: replace the type-only `StorageAdapter` import with a local or
      core-owned storage interface. Core session persistence is not host-papp session-list compatible and
      cutover requires one-time re-pair ([E5](<E - open-questions.md>)). The migrated submodule deletes
      `packages/auth` and uses the Rust-owned `SessionStore`.
- [x] `packages/ui/src/statement-store-mapping.ts`: delete the SDK<->host mapping once Rust emits/accepts
      the final TrUAPI statement-store wire types directly.
- [x] `packages/ui/src/permissions.ts`, `packages/ui/src/allocation-modal.ts`, and any modal code importing
      `@novasamatech/host-api`: replace SDK enum/error/types with generated Rust/TrUAPI-facing types.
- [x] `packages/ui/src/topbar.ts`: remove type-only `Identity`, `__NOVASAMATECH_VERSIONS__`, and the
      `NOVASAMATECH_ALLOWLIST` diagnostics once auth/debug data comes from the Rust bridge.
- [x] `packages/truapi-debug`: replace `onHostApiDebugMessage` and `onHostPappDebugMessage` with Rust
      bridge debug events, then drop the debug package dependencies on `host-api`, `host-container`, and
      `host-papp`. The migrated submodule no longer has this package or runtime Nova debug imports.
- [x] `apps/host/vite.config.ts`: delete `define.__NOVASAMATECH_VERSIONS__` and the `manualChunks` branch
      that returns `"nova-scale"` for `@novasamatech/scale`.
- [x] `packages/protocol/src/auth-storage.ts`: delete `EMPTY_SHARED_AUTH_SESSION_LIST = "0x00"` and
      host-papp session-list compatibility notes; the core uses its own persisted `SessionInfo` through a
      host-global `SessionStore`, not product-scoped local storage. dotli web should keep the shared
      host-origin storage route but move to a Rust-owned key/prefix such as `TRUAPI_SESSION_<siteId>`,
      not `PAPP_*` / `SsoSessions`, and preserve cross-tab change notifications for logout/re-pair.
- [x] `package.json` deps: remove `@novasamatech/host-api`, `host-container`, `host-papp`,
      `sdk-statement`, `statement-store`, and `storage-adapter` from `packages/ui`, `packages/auth`, and
      `packages/truapi-debug` when no `rg "@novasamatech"` value/type imports remain.
- [ ] Docs and README: update `README.md` host-container/debug descriptions and any dotli migration notes
      to describe the Rust bridge and core-owned SSO/session protocol.

# 02 - dotli Parity Contract

> Parent: [dotli shared Rust core migration](<index.md>).

Feature parity means product-visible behavior from the current
`~/github/dotli` checkout, not every internal Nova package behavior. Evidence
comes from dotli main `4611008`; the older `85c9733` checkout remains useful
only for historical JS handler shape where latest main has already removed
Nova code.

## Current Nova Runtime Packages

dotli main currently depends on:

| Package | Version on dotli main | Used for |
|---|---:|---|
| `@novasamatech/host-api` | `0.8.6` | product wire types and errors |
| `@novasamatech/host-container` | `0.8.6` | postMessage container runtime, rate limits, entropy helper |
| `@novasamatech/host-papp` | `0.8.6` | SSO V2 pairing, session restore, signing, alias, allocation |
| `@novasamatech/statement-store` | `0.8.6` | People-chain statement-store client and proof signing |
| `@novasamatech/sdk-statement` | `^0.6.0` | statement mapping/types |
| `@novasamatech/storage-adapter` | `0.8.6` | host-papp storage adapter |

The migration replaces their runtime responsibilities with Rust core services
plus dotli host adapters.

## Handler Parity Matrix

| Domain | dotli main behavior | Target owner |
|---|---|---|
| Feature support | `Chain` supported if dotli remote chain provider supports the genesis. Others false. | Core delegates to host chain support. |
| Chain connection | Product gets a chain provider. dotli shows a one-time direct-chain-access warning. | PR 104 chain runtime plus host `ChainProvider`. |
| Account get | Requires session. Validates product id. Derives product public key from wallet root account. | Core. |
| Legacy accounts | Disconnected returns `[]`; authenticated returns derived `(product_id, 0)` with lite username. | Core. |
| User id | Requires session, identity, and cached `GetUserId` permission. | Core plus permission service and identity lookup. |
| Request login | Starts SSO pairing and resolves when auth state settles. | Core SSO plus host QR presenter. |
| Connection status | Emits current session state immediately and on changes. | Core session state. |
| Account alias | Uses SSO alias request; cross-domain alias prompts first. | Core SSO plus host permission UI. |
| Sign payload/raw | Validates signer, gates `ChainSubmit`, shows host modal, sends SSO request, maps cancel/timeout/errors. | Core SSO plus host confirmation UI. |
| Create transaction | Same as signing, wallet constructs and signs transaction. | Core SSO plus host confirmation UI. |
| Legacy signing/transaction | Re-derives `(product_id, 0)`, validates signer, then reuses product-account flow. | Core. |
| Resource allocation | Requires session, shows allocation modal, sends SSO request, strips returned secrets. | Core SSO plus host allocation UI. |
| Product local storage | Product-scoped key/value storage. | Existing `Storage` host primitive. |
| Entropy | Derived from SSO V2 `rootEntropySource`, product id, and caller key via RFC-0007 `deriveProductEntropy`. | Core. |
| Navigation | dotli-aware URL normalization and `window.open`. | Core parses policy, host opens URL. |
| Device permissions | Cached tri-state prompts for enforceable browser permissions; notification/open-url special cases. | Core permission service plus host prompt. |
| Remote permissions | Cached submit-style permissions, WebRTC and broad remote requests auto-granted today. | Core permission service plus host prompt. |
| Notifications | Schedule/cancel by per-product id; immediate notifications display immediately. | Host notification scheduler behind Rust API. |
| Statement subscribe | Subscribes to People-chain statement store, bridges topic filters, sends signed statements. | Core statement-store client. |
| Statement submit | Submits signed statements through statement-store adapter. | Core statement-store client. |
| Statement proof | Signs proof with session statement-store secret, not product account key. | Core. |
| Preimage submit | Prompts, submits to selected backend, caches by content key. | Host backend behind Rust API. |
| Preimage lookup | Emits current cache/null immediately, then polls selected backend until unsubscribe. | Host backend behind Rust subscription API. |
| Theme | Emits current default light/dark theme immediately and on changes. | Host theme subscription. |
| Payments | Explicit typed "not supported in dot.li" behavior. | Keep unavailable for parity. |

## Out of Parity Scope

These are not blockers because dotli main does not implement them as working
features:

- Payment rails.
- Full `create_account_proof` ring-VRF proof.
- Chat and coin-payment APIs.
- Independent nested-product identities, sessions, or Rust runtimes.

## Compatibility Notes

- `get_legacy_accounts` returns `[]` when disconnected. When authenticated it
  returns the synthetic `(product_id, 0)` account plus lite username so legacy
  signing methods can round-trip the same signer.
- Entropy now uses the SSO V2 `rootEntropySource` returned by the wallet.
  The older `ssSecret` input is stale and must not be used for current-dotli
  parity vectors.
- Session persistence does not need to preserve the host-papp `SsoSessions`
  binary format. One-time re-pair during cutover is acceptable.
- Nested dApps are currently detected by dotli JS and assigned nested storage
  prefixes. The v1 Rust target should keep one shared core session and treat
  nested bridging as adapter compatibility unless a future API says otherwise.

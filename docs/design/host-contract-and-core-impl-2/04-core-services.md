# 04 - Core Services

> Parent: [dotli shared Rust core migration](<index.md>).

This doc names the Rust services that replace Nova runtime packages. It avoids
low-level crypto constants; those belong in implementation tests and vector
fixtures.

## Service Map

```
truapi-server
  |
  +-- RuntimeConfig
  +-- SessionService
  +-- SsoService
  +-- ProductAccountService
  +-- PermissionService
  +-- StatementStoreService
  +-- SigningService
  +-- ResourceAllocationService
  +-- EntropyService
  +-- PreimageFacade
  +-- NotificationFacade
  +-- ThemeFacade
```

## SessionService

Owns the active session lifecycle:

- restore from `SessionStore` at startup;
- publish connection status current-then-changes;
- persist after successful pairing;
- clear on local logout;
- clear when the SSO peer sends disconnect;
- fail pending session requests on logout/disconnect.

Session state must include enough data for dotli parity:

```rust
pub struct SessionInfo {
    pub session_id: Vec<u8>,
    pub root_account_id: [u8; 32],
    pub identity_account_id: [u8; 32],
    pub statement_secret: SessionStatementSecret,
    pub root_entropy_source: RootEntropySource,
    pub sso_channel_state: SsoChannelState,
}
```

Exact secret lengths and encoding are internal to the core and proven by vector
tests, not specified here.

## SsoService

Owns the People-chain statement-store SSO protocol:

1. generate host pairing material;
2. build a wallet deeplink;
3. ask `PairingPresenter` to display it;
4. subscribe to the bootstrap statement topic;
5. verify and decrypt the wallet response;
6. establish encrypted request/response channels;
7. send post-pairing messages for signing, alias, transaction creation,
   allocation, and disconnect.

The host provides chain access. The host does not own pairing state or a wallet
connection.

## ProductAccountService

Answers account methods from session plus runtime config:

- `get_account`: derive product public key from wallet root account,
  requested product identifier, and derivation index;
- `get_legacy_accounts`: return `[]` when disconnected; when authenticated,
  return derived `(product_id, 0)` with lite username for legacy signer
  round-trips;
- legacy signer validation: compare caller-supplied signer against
  `(product_id, 0)`;
- `get_user_id`: return identity username only after permission and identity
  lookup succeed.

## SigningService

Owns all signing and transaction product methods:

```
product request
  -> validate product account / legacy signer
  -> check cached permission where required
  -> ask host confirmation UI
  -> send encrypted SSO request
  -> map wallet response or error into TrUAPI result
```

Keep dotli's visible semantics:

- local cancel maps to the typed rejection error;
- permission denial maps to permission denied;
- SSO timeout/session failures map to unknown with a reason;
- create-transaction returns wallet-built signed transaction bytes.

## StatementStoreService

Uses the same People-chain statement-store client as SSO:

- submit signed statements;
- subscribe by topic filter;
- create statement proofs with the session statement secret;
- return only signed statements to products, matching dotli main filtering.

Product-derived accounts do not sign statement proofs in dotli main because the
session statement-store account has the allowance.

## ResourceAllocationService

Resource allocation is split:

```
product request
  -> host allocation confirmation modal
  -> encrypted SSO allocation request
  -> strip secret payloads from Allocated outcomes
  -> return product-visible outcomes
```

Do not model this as `remote_permission`. It is a consent UI around a session
operation, not just a cached permission decision.

## EntropyService

Dotli main derives entropy from:

- session `rootEntropySource`;
- product label/id scope;
- caller-provided key.

The Rust implementation must match that behavior with vector tests. Do not use
`statement_secret` for dotli main parity.

## Facades

Some behavior remains host-backed but routed through Rust:

- `NotificationFacade`: schedule/cancel and return ids.
- `ThemeFacade`: subscribe current-then-changes.
- `PreimageFacade`: host-selected backend for submit/lookup, with Rust wire
  mapping and subscription lifecycle.
- `Navigation`: Rust policy/normalization, host opens URL.
- `Chain`: PR 104 chain runtime over host `ChainProvider`.

## Validation Strategy

Use small, named fixtures:

- product account derivation vectors;
- entropy vectors using `rootEntropySource`;
- SSO pairing fixture or wallet test peer;
- statement proof vector;
- mocked SSO responses for sign/raw/transaction/alias/allocation;
- web adapter tests for notifications, theme, preimage, and session-store
  current-then-change behavior.

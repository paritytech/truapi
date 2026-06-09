# E - Decision log / deferred questions

> Part of the [host-contract & core-impl spec](<index.md>).

Resolved branches from the plan audit, plus deferred items that are not required for current dotli parity.
The rest of the spec assumes these decisions for v1.

## E1. StatementStore `subscribe`/`submit` transport: RESOLVED

The core builds a minimal People-chain statement submit/subscribe client for pairing anyway
([H](<H - sso-pairing-protocol.md>), Tier 2), so the product-facing `subscribe`/`submit` reuse it rather
than being a fresh pallet port. The resolved choice is where the **product-facing paging semantics**
(topic-filtered subscription, historical-dump-then-live driving `is_complete`) live. **Decision:**
in-core now that the client exists; the on-chain `Statement`
SCALE is known from the iOS peer. This is part of the `@novasamatech` removal critical path because
dotli exposes product-facing statement-store callbacks through `@novasamatech/statement-store` today.
Gates [B subscribe/submit](<B - core-impls.md>).

## E2. Wallet channel ownership (signing model): RESOLVED

Resolved by [H](<H - sso-pairing-protocol.md>): there is no separate wallet channel. Signing, ring-VRF
alias, and create-transaction are request/response messages the **core** sends over the encrypted SSO
session channel on the People-chain statement store; the wallet signs and replies. So the wallet
sign-protocol is **core-owned**, and `host-papp` has no residual client to keep. Sub-decision retained:
the `ChainSubmit` permission gate is enforced in the core via the existing `Permissions` trait before the
`signingRequest` is sent.

## E3. `create_account_proof` ring-VRF (full proof)

The alias (`get_account_alias`) is an `aliasRequest` the wallet answers with `deriveAlias` over the SSO
channel ([H §5](<H - sso-pairing-protocol.md>)). `create_account_proof` (#26) needs a **full ring-VRF
proof**, which the channel does not expose today (the wallet has `BandersnatchKeyManaging.createProof` but
no message routes to it). Options: add a new SSO message type the wallet answers, or compute it in-core
(`bandersnatch_vrfs`/`ark-*` + ring membership from the Chain runtime, XL). **Recommendation:** add an SSO
message (keeps the ring secret in the wallet). This is not a current dotli parity blocker because current
dotli does not implement `handleAccountCreateProof`, and the host-papp remote-message surface audited for
this plan has no full ring-VRF proof request/response variant.

## E4. Preimage: RESOLVED

Keep host-side (dotli polls an IPFS/Helia gateway; no `@novasamatech`), or move CID compute + poll
in-core with a content-fetch host callback. **Decision:** host-side for v1. Because current dotli does
implement preimage submit/lookup through the `host-container` bridge, Rust must still expose host-side
preimage callbacks before `host-container` can be removed; the IPFS/Bulletin implementation itself stays
out of `truapi-server`.

## E5. Session-persistence encoding: RESOLVED

The core defines its own persisted `SessionInfo` format (forces a one-time re-pair on cutover), or
matches `@novasamatech` byte-for-byte (the `SsoSessions` / `PAPP_<siteId>_<key>` keys and the
`EMPTY_SHARED_AUTH_SESSION_LIST = "0x00"` SCALE `Vec<Session>` sentinel, `auth-storage.ts:10,16,53-55`) so
already-logged-in users survive. The full `Vec<Session>` layout is host-papp-internal.
**Decision:** core-defined. A one-time re-pair on cutover is acceptable, so the implementation does not
need to reverse-engineer or preserve host-papp's internal session list encoding. Gates
[C persistence](<C - session-contract.md>) and the `auth-storage.ts` cleanup in [F](<F - novasamatech-removal.md>).

## E6. Relay endpoint + deeplink format: RESOLVED

There is no relay endpoint: the transport is the People-chain statement store reached via `ChainProvider`,
and the deeplink is `polkadotapp://pair?handshake=<hex(SCALE HostHandshakeData)>`
([H §1](<H - sso-pairing-protocol.md>)). Both are core-owned protocol constants; the only host input is the
People-chain genesis hash for `ChainProvider::connect` and the dApp metadata URL.

## E7. Resource allocation confirmation: RESOLVED

Current dotli shows a dedicated allowance modal before calling `session.requestResourceAllocation`; errors
from the SSO allocation attempt keep the modal open for retry. **Decision:** model this as a dedicated
host confirmation callback/trait before the core sends the encrypted `resourceAllocationRequest` over the
SSO session channel. Do not overload `Permissions::remote_permission`, which is a permission decision
surface rather than retry-capable operation UI.

## E8. Nested dApps: RESOLVED

Current dotli can create extra JS host-container bridges for nested iframe sources. **Decision:** do not
model nested dApps as separate Rust runtimes, sessions, product identities, or storage namespaces for v1.
Nested traffic, if forwarded at all, uses the shared top-level product core. Track the usefulness and any
future independent nested-product model in [I](<I - nested-dapps.md>).

## E9. Account-only session injection: RESOLVED

Existing `setActiveSession(pubkey, lite, full)`-style APIs cannot restore a channel-capable core session:
they omit `ss_secret`, P-256 key material, peer keys, and derived session ids. **Decision:** remove
`setActiveSession`/`clearActiveSession` worker-plumbing from the migration plan. Real restore/logout uses
the core-owned persisted `SessionInfo` store; cutover still requires one-time re-pair.

## E10. Session persistence storage surface: RESOLVED

The existing `Storage` trait is product-scoped local storage. A wallet session is host-global,
secret-bearing, and shared across product runtimes. **Decision:** add/use a separate host-global
`SessionStore` capability for opaque core-encoded bytes. Rust owns the `SessionInfo` schema/versioning;
the host only implements `read()`, `write(Vec<u8>)`, and `clear()`. Do not persist `ss_secret` or ECDH
material through product-scoped `Storage`, and do not expose typed session fields to JS/UniFFI.

## E11. Session cardinality: RESOLVED

Current dotli reads host-papp's `sessions` list but effectively restores the first session
(`sessions[0]`). **Decision:** v1 `SessionStore` is single-session only: one optional opaque blob, replaced
on successful pairing and cleared on logout. Do not preserve host-papp's multi-session `Vec<Session>` shape
as part of the migration.

## E12. Corrupted persisted sessions: RESOLVED

If `SessionStore.read()` returns bytes that are unreadable, schema/version-invalid, or fail decode,
the core clears the store, logs/emits diagnostics, and remains disconnected. **Decision:** invalid
persisted session data behaves like no session; it must not brick startup or create a partial connected
state.

## E13. Persisted-session encryption: RESOLVED

The persisted session blob contains secret material, but v1 relies on host storage security rather than
adding Rust-side encryption/MAC around the blob. **Decision:** `SessionStore` is a host trust boundary:
the host is responsible for confidentiality and tamper resistance. Rust owns schema encoding and restore
validation, but writes the opaque encoded blob as-is.

## E14. dotli web SessionStore key/path: RESOLVED

Current dotli routes host-papp storage through the shared host-origin path
(`host.<root-domain>` localStorage via the protocol iframe) using `PAPP_<siteId>_<key>` keys such as
`SsoSessions`. **Decision:** reuse the shared host-origin storage route for dotli web, but use a new
Rust-owned key/prefix such as `TRUAPI_SESSION_<siteId>`. Do not reuse `PAPP_*` or `SsoSessions`, because
the Rust session blob is single-session and core-defined.

## E15. SessionStore cross-tab updates: RESOLVED

Current dotli's storage adapter preserves cross-tab updates through `subscribeSharedAuthStorage` /
BroadcastChannel behavior. **Decision:** v1 `SessionStore` supports current-then-changes coarse change
notifications for both same-runtime writes/clears and cross-tab/process changes. The first tick tells the
core to call `read()` for startup state; future ticks do the same. Clear therefore maps to
`read() == None` -> in-memory `clear_session` + `Disconnected`; valid write replaces the current in-memory
session. Invalid read follows E12. The core may dedupe equivalent blobs/session ids before rebroadcasting.

## E16. Logout API ownership: RESOLVED

Hosts need a direct UI action for disconnect/logout. **Decision:** expose a public core logout/disconnect
API through WASM, UniFFI, and JS worker surfaces. The core owns lifecycle policy: tear down session-channel
subscriptions and pending request waiters, clear in-memory `SessionState`, clear `SessionStore`, broadcast
`Disconnected`, and tolerate idempotent logout. Hosts should not call `SessionStore.clear()` directly as
the public logout path.

## E17. SSO logout message: RESOLVED

The SSO message enum already has `disconnected`. There is no separate wallet channel or connection:
pairing and post-pairing peer communication happen only through the SSO session protocol over the
People-chain statement store. **Decision:** local public logout best-effort sends an SSO session-channel
`disconnected` message before teardown when the session channel is usable, but local cleanup must not hang
or fail if the SSO send fails.

## E18. Peer-originated SSO disconnect cleanup: RESOLVED

When the SSO peer sends `disconnected`, the core should not preserve the persisted session. **Decision:**
peer-originated SSO disconnect routes through the same cleanup path as local logout: tear down SSO channel
state, clear in-memory `SessionState`, clear `SessionStore`, and broadcast `Disconnected`.

## E19. Pending SSO requests on disconnect: RESOLVED

Current dotli does not maintain a separate UI-level cancel token for session-channel work. Local logout
calls `adapter.sessions.disconnect(session)` and then sets auth state to idle; dotli's debug contract says
session-channel teardown produces `session:host_action_failed` for host-originated actions and
`session:terminated` stops further peer/host actions for that session. The host-papp session wrapper
also applies a 180s queue timeout to signing, raw signing, create-transaction, and
resource-allocation requests; dotli's signing/transaction modals add a 300s outer fallback. Alias adds no
separate timeout in current dotli. Resource allocation keeps its modal open for retry after an SSO
failure. **Decision:** Rust owns this lifecycle explicitly: local logout or peer `disconnected` tears down
subscriptions and pending waiters so no operation can hang forever; per-method errors preserve current
dotli's typed shape (`Rejected` for local signing/transaction modal cancel, `Unknown { reason }` for
session-channel failure where no narrower host-api error exists).

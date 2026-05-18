# Dotli architecture change, visual reference

Companion to [dotli-rust-core-proposal.md](./dotli-rust-core-proposal.md). Shown as diagrams plus a deep dive on how the host-callback surface maps to the shared-core SDK vision.

The point of these diagrams: justify what is **in scope** for the dotli migration diff and what is explicitly **deferred**. The migration replaces the novasamatech/host-api stack with the TrUAPI Rust core; nothing else.

---

## 1. Where protocol logic lives (the headline change)

```
                       BEFORE (origin/main)
   ┌─────────────────────────────────────────────────────────────┐
   │ Product iframe (sandbox)                                    │
   │   @novasamatech/host-papp ─── product client                │
   └──────────────────────┬──────────────────────────────────────┘
                          │  postMessage (host-container wire)
                          ▼
   ┌─────────────────────────────────────────────────────────────┐
   │ dot.li main thread                                          │
   │ ┌─────────────────────────────────────────────────────────┐ │
   │ │ container.ts  +  statement-store-mapping.ts             │ │
   │ │ ─────────────────────────────────────────────────────── │ │
   │ │  routing • codecs • subscriptions • permissions service │ │
   │ │  topic encoding • statement mapping • dotns parsing     │ │
   │ │  rate limiting • feature flags • etc.                   │ │
   │ │            ALL OF THIS IS RE-IMPLEMENTED                │ │
   │ │            ON iOS / Android / Electron TOO              │ │
   │ └────────────────────────────┬────────────────────────────┘ │
   │                              │                              │
   │             OS primitives  (modals, localStorage,           │
   │             smoldot, host-papp, fetch, Notification API)    │
   └─────────────────────────────────────────────────────────────┘


                       AFTER (this refactor)
   ┌─────────────────────────────────────────────────────────────┐
   │ Product iframe ── origin: <cid>.app.dot.li ── (per-CID)     │
   │   @parity/truapi (codegen) ─── product client               │
   └──────────────────────┬──────────────────────────────────────┘
                          │  MessageChannel (TrUAPI wire bytes)
                          │  port handed off via the host shell
                          │  during the `truapi-init` handshake
                          ▼
   ┌─────────────────────────────────────────────────────────────┐
   │ Host shell ── origin: dot.li ── (user-visible UI)           │
   │ - top bar, modal prompts                                    │
   │ - creates the protocol iframe + product iframe              │
   │ - relays MessagePorts between them (no protocol logic)      │
   └──────────────────────┬──────────────────────────────────────┘
                          │  MessageChannel port (transferred)
                          ▼
   ┌─────────────────────────────────────────────────────────────┐
   │ Protocol iframe ── origin: host.dot.li ── (STABLE origin)   │
   │ (hidden iframe embedded by every dot.li tab)                │
   │                                                             │
   │  thin JS shim:                                              │
   │  - constructs the SharedWorker below                        │
   │  - exposes platform callbacks the WASM core can't make      │
   │    directly from a worker (modal UI prompts are routed      │
   │    back through the host shell at dot.li)                   │
   │  - migrates legacy localStorage sessions into IndexedDB     │
   │                                                             │
   │           ┌────────── SharedWorker (host.dot.li) ─────────┐ │
   │           │                                               │ │
   │           │  truapi-server (Rust → WASM)                  │ │
   │           │  ──────────────────────────                   │ │
   │           │  routing • SCALE codecs • subscriptions       │ │
   │           │  permissions service                          │ │
   │           │  statement mapping                            │ │
   │           │  dotns parsing • rate limit                   │ │
   │           │  embedded smoldot ── chain provider           │ │
   │           │  session state                                │ │
   │           │                                               │ │
   │           │  storage: IndexedDB on host.dot.li            │ │
   │           │  (stable across product CID changes; shared   │ │
   │           │   across every tab via SharedWorker semantics)│ │
   │           └───────────────────────────────────────────────┘ │
   └─────────────────────────────────────────────────────────────┘

   Same logic, written once in Rust, shared across iOS / Android / web.
```

### Origin model, why host.dot.li

Production nginx routes (see `dotli/nginx/nginx.polkadot`):

| Hostname                  | Build       | Role                                                              |
|---------------------------|-------------|-------------------------------------------------------------------|
| `dot.li` and `*.dot.li`   | host        | Main shell, user-visible UI, top bar, dApp loader               |
| `<cid>.app.dot.li`        | sandbox     | Product iframes, origin changes every CID update                |
| `host.dot.li`             | protocol    | Stable-origin protocol iframe, hidden, embedded by every tab    |

Product iframes can't host the protocol core: their origin changes with every app CID, so any `localStorage` / IndexedDB / OPFS state would be lost on every update. The host shell at `dot.li` is stable but cohabits with user-facing UI; running heavy crypto + smoldot there would block paint frames.

The protocol iframe at `host.dot.li` has neither problem: it's a stable origin and it has no UI, so a same-origin `SharedWorker` constructed from it runs the WASM core off the main thread while keeping `truapi`'s persistent state on a stable origin.

`SharedWorker` semantics give two further wins:

- **One core per browser, not per tab.** Session state, permission grants, and chain connections are implicit cross-tab state. Replaces the existing `BroadcastChannel` glue for shared auth.
- **Embedded smoldot.** Since the SharedWorker is already the single per-origin core, smoldot lives inside it. Dotli's separate `protocol-shared-worker.ts` smoldot SharedWorker collapses into this one.

`SharedWorker` does not expose `localStorage` (main-thread only). The `truapi-platform::Storage` impl persists to **IndexedDB** on the `host.dot.li` origin. The thin JS shim in the protocol iframe runs a one-time migration of the existing `PAPP_${siteId}_*` localStorage keys into IDB so sessions survive the cutover.

---

## 2. Module-level diff in `@dotli/ui`

```
   ──── DELETED ────                ──── NEW ─────────────────────────

   container.ts            930 LOC  host-callbacks/
   statement-store-mapping  170 LOC    ├─ Account.ts          host-papp
                                       ├─ Chain.ts            smoldot/RPC
                                       ├─ LocalStorage.ts     localStorage
   ──── DEPS DROPPED ───               ├─ OpenUrl.ts          window.open
                                       ├─ Preimage.ts         Helia (IPFS)
   @novasamatech/host-api              ├─ PromptPermission.ts modal
   @novasamatech/host-container        ├─ PushNotification.ts Notification
   @novasamatech/sdk-statement         ├─ Signing.ts          host-papp
   @novasamatech/statement-store       ├─ StatementStore.ts   sub-store
                                       └─ handlers.ts         glue

   ──── DEPS ADDED ────

   @parity/truapi-host-shared      SharedWorker entrypoint that imports
                                   the WASM core (smoldot embedded)
   @parity/truapi-host-web         protocol-iframe shim: constructs the
                                   SharedWorker, exposes the platform
                                   callbacks the worker can't make from
                                   its own context (modal UI, etc.),
                                   relays MessagePort handoffs from the
                                   host shell at dot.li
   @parity/truapi                  types from codegen

   ──── KEPT ──────────

   bridge.ts                rewritten: ~80 LOC, was ~120 LOC
   permissions.ts           kept (per-dApp grant storage)
   permission-modal.ts      kept (UI primitive)
   render.ts                kept (no-op for non-iframe content)
   topbar.ts                kept (UI)
   @novasamatech/host-papp  kept (account/signing; retired in D1)
```

---

## 3. The shrinking host-callback surface

```
                 BEFORE                            AFTER
              (15+ handlers,                  (5 callbacks the
               JS owns the logic)              core literally
                                                cannot make)

  ┌──────────────────────────┐         ┌──────────────────────────┐
  │ accountGet               │ ──────► │ accountGet      (D1*)    │
  │ accountGetAlias          │ ──────► │ accountGetAlias (D1*)    │
  │ getNonProductAccounts    │ ──────► │ getNonProduct…  (D1*)    │
  │ getUserId                │ ──────► │ getUserId       (D1*)    │
  │ accountConnectionStatus… │ ──────► │ accountConn…    (D1*)    │
  │ signPayload              │ ──────► │ signPayload     (D1*)    │
  │ signRaw                  │ ──────► │ signRaw         (D1*)    │
  │ statementStoreSubmit     │ ──────► │ statementStore… (D2*)    │
  │ statementStoreSubscribe  │ ──────► │ statementStore… (D2*)    │
  │ statementStoreCreateProof│ ──────► │ statementStore… (D2*)    │
  │ preimageLookupSubscribe  │ ──────► │ preimageLookup… (D2*)    │
  │                          │         ├──────────────────────────┤
  │ devicePermission ────────┼───┐     │ promptPermission         │ ◄ A3
  │ remotePermission ────────┼───┘     │                          │
  │ navigateTo (parsing) ────┼─────►   │ openUrl (already parsed) │ ◄ A2
  │ featureSupported         │ ──────► │ featureSupported (A5)    │
  │ localStorage*            │ ──────► │ localStorageRead/Write/  │
  │                          │         │ Clear                    │
  │ pushNotification         │ ──────► │ pushNotification         │
  │ chainConnection          │ ──────► │ chainConnect    (E1*)    │
  │ themeSubscribe (#366)    │ ──X     │ (out of scope)           │
  └──────────────────────────┘         └──────────────────────────┘

   * (D1/D2/E1) = host-papp, libp2p, layer-2 retirement issues already
                  documented in tracking issues, NOT this PR.
   ◄ A2/A3/A5  = already done in core (Phase A).
```

---

## 4. Adopting HostCallbacks per the vision doc

Every row in the middle block of diagram §3 is one piece of "logic that used to live in JS, now lives in Rust." The vision doc classifies each by the **capability trait** the host implements; everything else gets pulled out of the host into the core.

### 4.1 `devicePermission` + `remotePermission` → `promptPermission` (Phase A3)

**Before, host did the policy work.** Two separate callbacks:

- `devicePermission(name)`, for browser-mediated permissions (camera, mic, geolocation, push). The JS host:
  1. Maintained the per-dApp grant cache in `localStorage`.
  2. Classified which device permissions were even *enforceable* in a browser iframe (notifications and `openUrl` are not really gateable from a sandboxed iframe; mic and camera are).
  3. Showed the consent modal, persisted the result, dispatched a "permission changed" event so the iframe could reload with the updated `allow` attribute.
- `remotePermission(req)`, for protocol-level permissions (`TransactionSubmit`, `StatementSubmit`, `ChainSubmit`, `WebRtc`, a wildcard `Remote` variant). The JS host:
  1. Mapped `TransactionSubmit` → user-friendly "Sign Transactions" label.
  2. Decided which `Remote` variants were gated vs. auto-granted.
  3. Showed a different modal flow (the now-deleted `showRemotePermissionModal`).
  4. Rate-limited.

That is policy: classification, mapping, caching, rate limiting. By the vision doc's test ("why can't the Rust core do this directly?"), none of it is a syscall.

**After, host owns one trait: "ask the user yes or no."** A single `promptPermission(HostPermission) -> bool`. The Rust `host_logic::permissions` service in core:

- Knows the canonical wire tags and their human labels.
- Checks the cached decision (via `storageRead`) before calling back.
- Decides whether the permission is enforceable; auto-grants the unenforceable ones without bothering the host.
- Rate-limits.
- Only when *the user must actually be asked*, dispatches `promptPermission` and waits for the boolean.

In dotli, this is `host-callbacks/PromptPermission.ts`. Its sole job: render the modal, return `true` on grant, `false` on deny. Same trait will be implemented by Swift on iOS and Kotlin on Android, each rendering its native consent sheet. None of them re-implements the cache, the rate limiter, or the wire-tag mapping.

The dotli adapter still references `getPermissionStatus` / `setPermissionStatus` against a local `permissions.ts` store, that is a transitional wart. The cache should fully migrate to the core's `storageRead`/`storageWrite`. Phase A3 introduces the contract; the storage-key migration is still in flight and will let `permissions.ts` disappear from the dotli tree entirely.

### 4.2 `navigateTo (parsing)` → `openUrl (already parsed)` (Phase A2)

**Before, host was a URL parser.** `navigateTo(url)` handed a raw string to the host. JS had to:

1. Detect a `.dot` deep link (`testingout.dot/some/path`) → drive the dotli internal router (DOTNS resolution, swap iframe contents, push history state).
2. Detect a normal `https://` URL → `window.open(url, "_blank")`.
3. Detect malformed input → reject.

That is a parser plus a deep-link dispatcher. Three platforms, three parsers, three places to drift.

**After, host owns one trait: "hand this URL to the OS browser."** Two distinct surfaces emerge in the core (`host_logic::dotns`):

- Internal routing (deep links to other `.dot` apps) is handled entirely inside the core. It dispatches itself, no host roundtrip.
- External navigation surfaces as `openUrl(url)`, and `url` is *already validated* by the core. The host treats it as opaque.

In dotli, this is `host-callbacks/OpenUrl.ts`, which is essentially `window.open(url, "_blank")`. This maps to the vision doc's `NavigationProvider::open_url`. The vision doc also lists `handle_deep_link` on the same trait, but in dotli's split the core does not need to call back for deep links: it already knows the dApp graph and dispatches directly.

### 4.3 `featureSupported` (kept; planned for removal)

`featureSupported(genesisHash)` lets the core ask "does this host know about this chain?" before letting a product call it. The host answers yes/no from its supported-chain catalog.

The plan, tracked separately, is to drop this callback. The Rust core will bundle the chain catalog itself, so there is no question for the host to answer. That fits the vision doc's "why can't the Rust core do this directly?" test, the answer for `featureSupported` is "it can," so the callback should not exist.

### 4.4 `localStorage*` → `localStorageRead` / `localStorageWrite` / `localStorageClear`

**Before, implicit, scattered.** The novasamatech protocol had several scoped storage callbacks (one per dApp namespace), and the JS host computed prefixes (`dotli:<label>:<key>`), guarded against quota errors, and decided what counted as "scoped" vs. "global" state. The core did not own a storage abstraction; it asked the host for what it needed and trusted the host's scoping.

**After, three flat ops, no scoping in the host.** The vision doc's `StorageProvider`:

```rust
pub trait StorageProvider {
    fn read(&self, key: &str) -> Result<Option<Vec<u8>>, StorageError>;
    fn write(&self, key: &str, value: &[u8]) -> Result<(), StorageError>;
    fn clear(&self, key: &str) -> Result<(), StorageError>;
}
```

Three methods. The core:

- Owns the namespacing convention (`truapi:…` for core-owned state, per-dApp prefixes computed in core before calling back).
- Owns the cache invalidation rules.
- Owns the schema for any structured value stored.

The host, `host-callbacks/LocalStorage.ts` in dotli, just plumbs to `window.localStorage.getItem` / `setItem` / `removeItem`. On iOS that is `NSUserDefaults`. On Android `SharedPreferences`. None of those hosts cares what the keys mean; they just store bytes against strings.

This is also what makes the permission-cache cleanup in §4.1 tractable: once permissions migrate fully to `storageRead/Write`, the dotli adapter loses `permissions.ts` entirely and the core owns the grant cache the way it owns every other piece of state.

---

## 5. Mapping dotli `host-callbacks/` to vision-doc traits

Each `host-callbacks/*.ts` file in dotli is a one-trait implementation:

| dotli file              | vision-doc trait        | core owns                           | host owns              |
|-------------------------|-------------------------|-------------------------------------|------------------------|
| `LocalStorage.ts`       | `StorageProvider`       | namespacing, schema, invalidation   | `window.localStorage`  |
| `OpenUrl.ts`            | `NavigationProvider`    | URL parsing, deep-link dispatch     | `window.open`          |
| `PushNotification.ts`   | `NotificationProvider`  | rate limiting, dedupe, payload fmt  | `Notification` API     |
| `PromptPermission.ts`   | `ConsentProvider`       | classification, cache, mapping      | the consent modal      |

The "before" column for each row was a handler in `container.ts` that mixed all three concerns, protocol logic, policy, *and* the OS call, in JS. The refactor's whole purpose is to leave only the third column on the host side, and that is what makes iOS/Android able to share the rest with web.

The remaining dotli callbacks (`Account.ts`, `Signing.ts`, `Chain.ts`, `StatementStore.ts`, `Preimage.ts`) are the ones marked `(D1*)`, `(D2*)`, `(E1*)` in diagram §3. They do not fit the vision-doc model yet because they currently rely on JS-only libraries (`host-papp` for accounts/signing, smoldot, Helia). Each retirement issue describes how the underlying capability moves into Rust so the dotli callback can be dropped, leaving only the four vision-doc syscall traits above.

---

## 6. What this justifies about the migration diff

- **In scope:** anything between BEFORE and AFTER in §1 and §2 (delete `container.ts` / `statement-store-mapping.ts`, add `host-callbacks/`, swap `bridge.ts`, swap deps).
- **Out of scope, deferred to issues:** anything marked `(D1*)` / `(D2*)` / `(E1*)` in §3.
- **Out of scope and not coming back:** `themeSubscribe`, `requestLogin(reason, label)`, the remote-permission modal, these are #366 features that targeted the deleted `container.ts` and would need to be re-modeled as new Rust traits, not patched into the syscall layer.

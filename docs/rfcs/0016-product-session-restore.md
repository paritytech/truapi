---
title: "Product Session Restore Host API"
owner: "@replghost"
---

# RFC 0016 - Product Session Restore Host API

## Summary

This RFC defines a small Host API and storage convention for product session
restore. An immutable `.dot` product checkpoints semantic UI/session state
through the existing `host_local_storage_*` API and receives host lifecycle
events so it can flush state before normal app transitions such as app
switching, backgrounding, memory pressure, host restart, or mobile OS force
quit.

The goal is not to serialize a live JavaScript runtime, WebView process, or
in-memory browser heap. Instead, the product, or its framework/SDK layer,
writes a local, versioned session blob to a reserved product-storage key. The
host continues to own storage isolation, persistence policy, lifecycle
scheduling, package caching, encryption-at-rest, and retention.

The resulting user experience should feel like ordinary mobile app continuity:
open products remain visible in the host app switcher, cached packages reopen
quickly, and products that support this API resume to the route, draft,
selection, editor contents, scroll position, or workflow step where the user
left off.

## Motivation

TrUAPI products are immutable application packages, but users experience them
as apps. When a host shows a product in an app switcher, users expect that app
to survive the normal lifecycle of a phone or desktop host: switching away,
opening another product, backgrounding the host, losing a WebView under memory
pressure, force quitting, and reopening later.

Today a host can remember a list of open products and cold boot them again.
That restores the product set, but it does not restore the user's working
context. A crossword may reopen to the start page instead of the selected cell.
A merchant flow may lose its checkout step. A form draft may disappear unless
the product already used host storage for that specific field. A chat or editor
may reload to a default route even though the product package itself was cached
and unchanged.

The user-facing benefits of product session restore are:

- seamless app switching between multiple `.dot` products;
- continuity through backgrounding, host relaunch, and mobile OS process
  eviction;
- faster perceived startup when the host can reopen a cached package and
  restore product state without waiting for a full user journey to replay;
- fewer lost drafts and less repeated navigation;
- a clearer mental model for hosted products as apps rather than disposable web
  pages;
- better mobile ergonomics, where force quit and memory pressure are common
  parts of the platform lifecycle.

The host cannot infer arbitrary product state from outside the WebView. There
is no portable browser API for serializing JavaScript closures, pending
promises, timers, subscriptions, framework component internals, or the whole JS
heap. The right boundary is therefore small: the host owns scoped local
storage, lifecycle scheduling, encryption, retention, and package-cache
behavior; the product owns serialization and restoration of semantic state.

## Goals

- Allow products to checkpoint JSON-like semantic session state through the
  existing scoped local storage API.
- Allow hosts to request a checkpoint before suspend, close, memory eviction,
  background, or other lifecycle transitions.
- Allow products to receive saved session state early during startup.
- Keep session restore local to one host by default.
- Let hosts persist cached package identity and session state together.
- Provide best-effort restore with safe fallback to cold boot.
- Avoid exposing product session blobs to other products.
- Avoid making products implement native storage, encryption, file layout, or
  lifecycle heuristics.
- Avoid adding a second product-scoped blob storage API beside
  `host_local_storage_*`.

## Non-Goals

- Serialize a live WebView process, browser heap, DOM object graph, closures,
  pending promises, timers, or network subscriptions.
- Define cross-host session sync.
- Define product data sync. Products that need durable user data sync should
  use product-owned storage and sync APIs.
- Require every product to support session restore.
- Require products to serialize state as dCBOR directly.
- Guarantee exact process-like resurrection after crash or force quit.

## Detailed Design

### API Calls

The Product Session Host API is intentionally limited to host lifecycle
signals. Session bytes are stored through the existing scoped local storage
methods:

```rust
fn host_session_lifecycle_subscribe(
  request: SessionLifecycleSubscribeRequest,
  callback: fn(SessionLifecycleEvent)
) -> Result<Subscriber, SessionErr>;
```

The JavaScript SDK should expose local storage plus lifecycle events as a
higher-level registration helper:

```typescript
host.session.register({
  serialize: async () => ({
    route: router.currentPath(),
    selectedTab: store.selectedTab,
    formDrafts: store.formDrafts,
    scroll: ui.scrollPositions,
  }),
  restore: async (state) => {
    await router.replace(state.route);
    store.selectedTab = state.selectedTab;
    store.formDrafts = state.formDrafts;
    ui.restoreScrollPositions(state.scroll);
  },
});
```

The helper listens for lifecycle events, calls `serialize`, writes checkpoints
to the reserved local-storage key with debounce, and calls `restore` with the
decoded result of `host_local_storage_read` during startup.

The reserved storage key is:

```text
__truapi/session_restore/v1
```

Products should not use this key for unrelated durable data.

### Core Types

```rust
type ProductVersion = String;
type TimestampMs = u64;
type SessionLifecycleEventId = String;
type SessionSchemaVersion = u32;
type Bytes = Vec<u8>;

enum SessionStateCodec {
  Json,
  Cbor,
}

struct SessionStateBlob {
  codec: SessionStateCodec,
  bytes: Bytes,
}

struct SessionRestoreBlob {
  /// Version of this envelope format. This RFC defines version 1.
  envelope_version: u32,
  /// Product-defined schema version for this state blob.
  schema_version: SessionSchemaVersion,
  /// Optional product version or manifest version used for compatibility checks.
  product_version: Option<ProductVersion>,
  /// Product-side save timestamp.
  saved_at_ms: TimestampMs,
  /// Product state to persist. Must be structured-data encoded and size-limited.
  state: SessionStateBlob,
  /// Optional product hint for why the checkpoint was written.
  reason: SessionCheckpointReason,
  /// Optional product-side sequence number for deduplication.
  sequence: Option<u64>,
}

enum SessionCheckpointReason {
  UserAction,
  Periodic,
  RouteChange,
  DraftChanged,
  HostLifecycle,
  BeforeClose,
}

struct SessionLifecycleSubscribeRequest {
  replay_current_state: bool,
}

enum SessionLifecycleEvent {
  WillSuspend(SessionLifecycleRequest),
  WillEvict(SessionLifecycleRequest),
  WillClose(SessionLifecycleRequest),
}

struct SessionLifecycleRequest {
  event_id: SessionLifecycleEventId,
  reason: SessionLifecycleReason,
  deadline_ms: Option<u32>,
}

enum SessionLifecycleReason {
  AppSwitcher,
  HostBackgrounded,
  HostTerminating,
  MemoryPressure,
  UserClosedProduct,
  HostPolicy,
}

enum SessionErr {
  Unsupported,
  Unknown(GenericErr),
}
```

### Storage Convention

The session blob is stored by the product or SDK with:

```typescript
await host.localStorageWrite(
  "__truapi/session_restore/v1",
  encodeSessionRestoreBlob(blob),
);
```

The host's existing local storage implementation remains responsible for
product scoping, persistence, quotas, and at-rest protection. Hosts may encode
the underlying local storage database as dCBOR, JSON, SQLite rows, or another
local format. Products only see the byte value returned for their own reserved
key.

### Restore Status

No new product-facing status method is defined in V1. A product or SDK can
inspect restore availability by reading the reserved local-storage key. If the
key is absent, malformed, too new, or incompatible with the current product
version, the product cold boots and may clear the key.

Hosts that expose an app switcher may use their own local-storage metadata or
internal bookkeeping to annotate product cards. Such UI must not expose other
products' session state to the running product.

### Product Startup

On product startup, the product SDK should call `host_local_storage_read` for
`__truapi/session_restore/v1` as early as possible after the Host API is
available. If a compatible blob exists, the SDK calls the product's registered
`restore` function with the state value.

The product should treat restore as best effort:

- if no state exists, start normally;
- if the state schema is unsupported, clear or ignore it;
- if restore throws, start normally and optionally clear the failed checkpoint;
- if the product CID or product version changed, either migrate the state or
  reject it as incompatible.

Hosts may also inject an initial session-state hint at document start if the
ProductView implementation supports it. This is an optimization, not a
separate source of truth.

### Checkpoint Scheduling

Products may checkpoint after meaningful user actions, but must debounce
frequent changes. Hosts may request checkpoints through lifecycle events.

Recommended checkpoint moments:

- route changes;
- form draft changes after debounce;
- editor content changes after debounce;
- app switcher transition away from the product;
- host backgrounding;
- before ProductView eviction under memory pressure;
- before user closes a product;
- periodic idle checkpoint for long-running products.

Lifecycle events are a request for a best-effort checkpoint, not a guarantee
that the host can wait. `deadline_ms` tells the product how long the host
expects to keep the ProductView alive for this save opportunity. A missing
deadline means the host is making no useful timing guarantee.

When a product receives `WillSuspend`, `WillEvict`, or `WillClose`, it should
serialize quickly and write the reserved local-storage key. There is no
protocol acknowledgement in V1. Hosts may continue the lifecycle transition
after the deadline even if the product has not finished flushing state.

Hosts must not require a checkpoint to complete before continuing a critical OS
lifecycle transition. On mobile platforms, background and termination budgets
are limited. Checkpoints are best effort and should be small.

### Package Cache Interaction

Session restore is strongest when paired with package caching. The host should
store or reference:

- product domain;
- resolved product CID;
- package asset cache entry;
- product manifest version when available;
- last active timestamp;
- session restore blob metadata when available.

When restoring, a host should prefer the cached package/CID used by the saved
session when that package is still available and valid. The host may revalidate
DotNS in the background and surface an update separately. This avoids turning a
normal app restore into a full network-dependent cold boot.

If the cached package is unavailable, the host may re-resolve the product and
then decide whether the saved session state is compatible with the newly
resolved package.

### Local-Only Policy

Session restore is local to one host by default. Session blobs describe
transient device-local app continuity, not portable product data.

Hosts should not sync session blobs across devices or hosts unless a future
RFC defines an explicit handoff or sync policy. Product data that should move
between devices belongs in product-owned storage/sync APIs, not this local
session surface.

This local-only default avoids:

- cross-device conflict resolution;
- surprising restoration of sensitive drafts on another device;
- stale checkout, auth, or payment flows appearing elsewhere;
- complex encryption and key sharing requirements;
- confusing "open apps" semantics across host installations.

### Encoding and Encryption

The reserved local-storage value carries a `SessionRestoreBlob` containing a
`SessionStateBlob` with an explicit codec and byte payload. The initial
required codec is `Json`, encoded as UTF-8 JSON. `Cbor` is reserved for hosts
and SDKs that support binary structured values.

Product-facing SDKs should expose JSON-compatible structured values for the
common case. This keeps the TypeScript/Vite developer experience simple and
lets framework adapters serialize ordinary store/router state. The SDK is
responsible for encoding those values into the normative `SessionStateBlob`.

The host persistence format is host policy. Hosts should persist local storage
as deterministic binary data when useful, preferably dCBOR for session blobs,
and encrypt or seal it at rest.

Minimum host persistence requirements:

- encrypt or otherwise protect session blobs at rest;
- scope blobs by product ID and host user/account where relevant;
- never expose one product's session blob to another product;
- exclude local session blobs from cloud backup/sync by default;
- impose size limits;
- impose retention or TTL policy;
- clear blobs when the user closes a product or clears product data.

Products may provide sensitivity hints in a future extension, but the host
decides storage policy. Products must not be able to force plaintext storage.

### Size Limits and Retention

Hosts must enforce an implementation-defined maximum checkpoint size. A
recommended starting limit is 256 KiB per product session. Products needing
larger durable data should store that data through product storage or a
product-specific sync API, and checkpoint only references or lightweight UI
state.

Hosts should retain session blobs while a product appears in the local app
switcher. Hosts may also retain recently closed blobs for crash
recovery, but should clear them after a short TTL unless user policy says
otherwise.

### Framework Adapters

Most products should not hand-write session code. The TrUAPI JavaScript SDK or
Vite plugin should provide helpers for common product stacks:

- router path and query parameters;
- selected tabs and local UI mode;
- scroll restoration;
- form drafts;
- Zustand, Redux, Pinia, Svelte stores, or equivalent state containers;
- explicit deny lists for sensitive fields.

Adapters must remain opt-in. They should make the common case easy without
pretending arbitrary JavaScript runtime state can be serialized automatically.

## Semantics

### Host Responsibilities

The host must:

- authenticate the calling product;
- scope local storage to the calling product;
- persist local-storage writes durably when possible;
- encrypt or protect session blobs at rest;
- send lifecycle events before suspend/eviction/close when possible;
- include lifecycle deadlines when the host has a meaningful save budget;
- enforce size limits and retention policy;
- return saved state only to the same product scope;
- fall back to cold boot if restore is unavailable or fails;
- clear state when the user explicitly closes the product or clears product
  data, according to host policy.

### Product Responsibilities

The product must:

- serialize only structured semantic state supported by `SessionStateBlob`;
- avoid storing secrets that should remain in product memory only;
- debounce frequent checkpoints;
- version its state schema;
- handle missing, incompatible, or failed restore;
- avoid assuming restore is guaranteed;
- respond to lifecycle save requests quickly when possible;
- clear session state on logout or product reset when appropriate.

### User Experience Requirements

Hosts that expose an app switcher should use session restore to make product
cards correspond to meaningful app continuity. A product card that survives
host relaunch should restore to the user's previous context when the product
supports this API.

Hosts should not block product opening on restore failure. If restore fails,
the host should open the product normally and may log diagnostics for product
developers.

Hosts may show cached screenshots while a product is rehydrating, but
screenshots are not session state and must not be treated as proof that restore
succeeded.

Hosts may use reserved-key metadata or internal bookkeeping to show whether a
product supports session restore or has a local checkpoint available. The exact
UI is host-defined, but hosts should avoid presenting a cold-boot-only product
as if it has restorable state.

## Privacy and Security

Session blobs may contain sensitive user context: form drafts, search terms,
selected records, partial checkout state, private routes, local workflow
state, or editor contents. Hosts should treat them as sensitive by default.

The API must not be used to bypass host storage permissions. A product may only
read its own current session state. Products must not enumerate other product
sessions.

Hosts should consider clearing session state when:

- the user logs out of the host;
- the selected wallet/account changes in a way that invalidates the product
  context;
- the product CID changes and the product does not explicitly migrate state;
- the user clears product data;
- the product clears the reserved session key;
- the session exceeds retention policy.

## Drawbacks

- Adds a new lifecycle surface that products and SDKs must implement correctly
  to get the best experience.
- Restore is best effort and may create expectation mismatch if products do
  not support it.
- Session blobs can contain sensitive information and therefore require careful
  host storage policy.
- Framework adapters may encourage products to serialize too much state unless
  size limits and guidance are clear.
- Products still need to reconnect network subscriptions, timers, and external
  resources after restore.

## Alternatives

**Host only restores open product domains.** This is simple and works today,
but it cold boots products and loses user context.

**Host serializes the WebView or JavaScript heap.** WebKit does not provide a
portable, safe API for this. Even if it did, closures, native handles, pending
promises, and subscriptions would not map cleanly across process launches.

**Products use host storage with no lifecycle signal.** Products can already
store durable data, but that alone is not enough for app-switcher session
continuity. Session restore benefits from host lifecycle signals, package
identity guidance, TTL, and a clear "transient resume state" boundary.

**Dedicated checkpoint/state Host API.** A previous shape exposed separate
checkpoint, state-get, state-clear, and status methods. That duplicates the
existing product-scoped local storage API and creates two similar places for
products to store opaque bytes. This RFC instead standardizes a reserved
local-storage key and keeps the Host API focused on lifecycle signals.

**Lifecycle acknowledgement API.** A host could ask products to acknowledge
whether a lifecycle flush completed before the deadline. This is deferred from
V1 because mobile hosts often cannot wait reliably, and the durable success
signal is already the local-storage write. Hosts may add diagnostics in SDKs or
developer tooling before standardizing an acknowledgement extension.

**Sync session blobs across hosts.** This may be useful for an explicit future
handoff feature, but it is not the right default. Session blobs are transient,
device-local, and potentially sensitive.

**Require products to provide dCBOR.** This improves canonicality at the
product boundary but worsens developer experience. The host can canonicalize
and persist as dCBOR while products work with structured JavaScript values.

## Migration Strategy

Hosts can implement this incrementally:

1. Persist open product domains, active product, package CID, and cached assets.
2. Ensure `host_local_storage_*` is product-scoped, durable, and protected at
   rest.
3. Add SDK helpers that read, write, and clear the reserved session key.
4. Add lifecycle events, host-requested checkpoints, and deadlines.
5. Add Vite/framework adapters.
6. Add diagnostics for restore success, restore failure, checkpoint size, and
   restore latency.

Products can opt in gradually. Products that do not implement the API continue
to cold boot from cached package assets.

## Unresolved Questions

- What maximum checkpoint size should be mandated by the spec versus left to
  hosts?
- Should product CID mismatch default to reject, restore, or product-defined
  migration?
- Should a future RFC define explicit cross-host handoff of a session?
- Should sensitivity hints be added to `SessionRestoreBlob`, or should
  protection remain entirely host policy?

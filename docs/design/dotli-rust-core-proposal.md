# dot.li with a single Rust core — proposed architectures

Companion to [dotli-architecture-change.md](./dotli-architecture-change.md). This doc documents what dot.li runs today (host page + protocol iframe at `host.dot.li` + per-CID app subdomain iframe + product iframe) and explores how the topology changes if we adopt a single Rust core as the runtime, and what it would take to drop the per-CID app subdomain iframe.

## Goal

The current dot.li architecture predates TrUAPI's "Rust core is the application" stance. It carries a four-frame topology that grew out of layer-2 isolation requirements, content-delivery sandboxing, and JS-side smoldot ownership. Each frame carries weight: a vite bundle, a wildcard DNS record, an OOPIF process, and at least one postMessage hop on every host-API call.

If the Rust core owns *all* protocol logic — chain access (smoldot + RPC), DOTNS resolution, content fetching from IPFS, per-product storage, permission gating — most of what the app subdomain iframe gives us either becomes redundant or moves into the core. This doc explores what that endpoint looks like.

The two specific questions this doc answers:

1. Can we eliminate the app subdomain iframe (`<cid>.app.dot.li`)?
2. Can the Rust core load directly in the protocol iframe (`host.dot.li`) instead of in a Web Worker spawned by the host page?

## What the app subdomain iframe gives us today

Two things, doing each well:

1. **Origin isolation per dApp.** Each `.dot` product gets a unique DNS subdomain `<cid>.app.dot.li`. The browser scopes localStorage, IndexedDB, cookies, service workers, and the iframe's Permissions Policy storage to that origin. Product A cannot read Product B's storage even when they're loaded in adjacent iframes of the same dot.li tab.

2. **Content fetching + sandboxed mount.** `apps/sandbox` reads the CID from its own subdomain, fetches the archive over P2P/Helia, decrypts, packs into a `Blob`, and mounts the product iframe at `blob:https://<cid>.app.dot.li/...`. The blob URL inherits the sandbox's origin, so the product iframe is same-origin with the sandbox, isolated from everything else.

Removing the app subdomain iframe means losing both. So: how much of (1) and (2) do we still need in a Rust-core-centric world, and where does the rest go?

## What origin isolation actually buys, and what replaces it

Per-product origin separation today gives products:

| What | Today | If core owns it |
|---|---|---|
| `localStorage` per product | scoped by `<cid>.app.dot.li` origin | core's `host_local_storage_*` syscalls, namespaced by product label |
| IndexedDB per product | per-origin DB | core's KV (extends `Storage` trait if needed) |
| Cookies per product | per-origin | products don't use cookies in the TrUAPI model |
| Service Worker per product | per-origin SW registration | products are static content; SW is replaced by core-side caching |
| Cache API per product | per-origin Cache | core fetches and caches |
| Permissions Policy | per-origin `allow=…` attribute | iframe gets a generic `allow` attribute set by the host; core's `prompt_permission` is the actual gate |
| `postMessage` security | products can address each other only via `targetOrigin` | products can't address each other at all (no shared parent) |
| Cross-product storage leak | impossible (different origins) | impossible if products are forced through the core (see below) |

The pivot: in the Rust-core model, **origin separation isn't the security boundary anymore — the core is**. The browser-level origin still has to be *something* (every iframe has an origin), but it can be the same for every product as long as products can't bypass the core's API for storage, fetch, permissions, etc.

The mechanism for "products can't bypass the core's API" is the iframe `sandbox` attribute *without* `allow-same-origin`, which gives the iframe an **opaque origin**. Calls to `localStorage`, `indexedDB`, `Cache`, `serviceWorker.register`, etc. all throw or no-op on opaque origins. Network `fetch` still works (subject to CSP), but if we tighten CSP via the iframe's `csp` attribute we can restrict that too.

So the trade is: drop per-CID DNS subdomains, accept opaque origin for products, route all persistence and (where needed) fetch through the core.

This matches the vision-doc model exactly. It's also the only way to keep the architecture meaningful on native (iOS/Android), where DNS subdomains aren't a thing — products there already go through the core for everything.

## Content delivery without `<cid>.app.dot.li`

If the app subdomain iframe goes away, who fetches and unpacks the dApp archive, and how does it become an iframe?

Three viable options:

### Delivery option A — Service Worker on `host.dot.li`

Register a service worker on `host.dot.li` that intercepts fetch requests matching a path scheme, e.g. `/__product/<cid>/*`. The product iframe is mounted with `src="https://host.dot.li/__product/<cid>/index.html"`. The SW intercepts, asks the Rust core for the file, returns it.

Origin of the product iframe: `https://host.dot.li`. To prevent cross-product leakage on shared storage, also set `sandbox` without `allow-same-origin` → opaque origin → storage APIs no-op.

Pros:
- Product loads via a normal `src` URL; existing dApp tooling that expects a "real" load works.
- The SW gives us a uniform fetch interception layer, including for nested resources the dApp loads (CSS, sub-pages, images).
- Service Worker is broadly supported (yes Safari).

Cons:
- Service worker registration is per-origin and async; must be resolved before any product iframe loads.
- Range requests, streaming, Content-Type sniffing all have to be handled by the SW.
- The SW becomes a long-lived dependency on the host.dot.li origin; updates must be carefully coordinated.

### Delivery option B — Blob URL from the protocol iframe

The protocol iframe at `host.dot.li` runs the Rust core, which fetches the archive. JS in that iframe creates a `Blob` from the unpacked HTML and assigns the blob URL to the product iframe's `src`. Origin of the blob URL = `host.dot.li`.

Pros:
- No service worker. Simpler bootstrap.
- Blob URLs work everywhere.

Cons:
- Sub-resources (CSS, JS, images) loaded by relative URL inside the dApp resolve against the blob's URL, which is opaque. Either the dApp must inline everything (works for a static SPA bundle) or we add another mechanism for sub-resources.
- For dApps with non-trivial asset trees, this breaks down.

### Delivery option C — Service Worker on the product iframe's origin

Mount the product iframe at a URL on `host.dot.li`, register a service worker scoped to that path, let the SW handle content. Functionally the same as option A; differs only in scoping (per-product vs shared).

Trade-off: per-product SWs cost more (registration overhead, browser-quota implications) but contain failures.

**Recommended**: option A (single SW on host.dot.li) for non-trivial dApps; option B as a fast path for archive-of-one-HTML-file products. The two can coexist — the host shell picks based on archive shape.

## Architecture options

Four placements for the Rust core, in increasing ambition. All four eliminate `<cid>.app.dot.li` and use opaque-origin product iframes via `sandbox` without `allow-same-origin`.

### Option 1 — Rust core in the protocol iframe (per-tab)

The core lives inside a Web Worker spawned by the protocol iframe at `host.dot.li`. Product iframes are mounted as children of the protocol iframe (the protocol iframe becomes the visible surface; the host page renders the topbar via overlay or via a small wrapper layout above the iframe).

```
┌─ tab @ dot.li ──────────────────────────────────────────────────────────┐
│  apps/host — topbar + label parsing (thin shell)                        │
│                                                                         │
│  ┌─ protocol iframe @ host.dot.li (visible, full-bleed) ────────────┐   │
│  │  apps/protocol — orchestrates Rust core + product mount          │   │
│  │  ┌─ Web Worker (host.dot.li) ─ Rust core ─────────────────────┐  │   │
│  │  │  smoldot · DOTNS · content fetch · per-product storage     │  │   │
│  │  │  permission gate · all host-API methods                    │  │   │
│  │  └────────────────────────────────────────────────────────────┘  │   │
│  │                                                                  │   │
│  │  ┌─ product iframe (opaque origin via sandbox) ──────────────┐   │   │
│  │  │  src = "host.dot.li/__product/<cid>/..." (SW intercepts)  │   │   │
│  │  │  ↕ MessageChannel ↔ Web Worker (Rust core)                │   │   │
│  │  └───────────────────────────────────────────────────────────┘   │   │
│  └──────────────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────────┘
```

**Trade-offs:**

- One Rust core per tab. Smoldot runs once per tab.
- All chain state (smoldot warm-start IndexedDB, persisted permissions, content cache) lives on `host.dot.li`, isolated from `dot.li`'s storage.
- Product iframe parents the protocol iframe → direct postMessage / MessageChannel; host page is not in the message path.
- Layout: protocol iframe is positioned full-viewport with the host page rendering only the topbar above (z-indexed or via a flex layout where the topbar is in the host page DOM and the iframe occupies the content area).
- N concurrent tabs = N Rust cores = N smoldots. Same drawback as documented in the E2 chain-provider unification work.

### Option 2 — Rust core in a SharedWorker (cross-tab)

Move the Rust core into a `SharedWorker` scoped to `host.dot.li`. The protocol iframe in each tab becomes a thin relay that connects to the SharedWorker via `MessagePort` and forwards product → core traffic.

```
┌─ tab A @ dot.li ───────────────┐    ┌─ tab B @ dot.li ───────────────┐
│  apps/host — topbar            │    │  apps/host — topbar            │
│  ┌─ protocol iframe ─────────┐ │    │  ┌─ protocol iframe ─────────┐ │
│  │ (host.dot.li, thin relay) │ │    │  │ (host.dot.li, thin relay) │ │
│  │ ┌─ product iframe ──────┐ │ │    │  │ ┌─ product iframe ──────┐ │ │
│  │ │ opaque origin         │ │ │    │  │ │ opaque origin         │ │ │
│  │ └───────────────────────┘ │ │    │  │ └───────────────────────┘ │ │
│  └──────┬────────────────────┘ │    │  └──────┬────────────────────┘ │
└─────────┼──────────────────────┘    └─────────┼──────────────────────┘
          │                                     │
          └──────────────┬──────────────────────┘  MessagePort
                         ▼
┌─ SharedWorker @ host.dot.li ──────────────────────────────────────────┐
│  Rust core: smoldot · DOTNS · content fetch · per-product storage     │
│             permission gate · all host-API methods                    │
└───────────────────────────────────────────────────────────────────────┘
```

**Trade-offs:**

- One Rust core per origin (across all tabs). Smoldot syncs once for the user, not once per tab.
- Cross-tab session sync becomes a feature for free (same core sees all products on all tabs).
- Shared content cache, shared chain warm-start.
- The lifetime of the SharedWorker is "while at least one same-origin context is open" — when all dot.li tabs close, the core terminates. Chain warm-start state must live in IndexedDB to survive.
- **Browser support:** SharedWorker is now broadly available — Chrome, Firefox, Safari macOS, and Safari iOS 16.0+ all support it. The remaining caveat is iOS-specific lifecycle: iOS aggressively suspends backgrounded tabs, and a SharedWorker with no visible client is a candidate for termination. The amortization benefit ("one smoldot for the whole session") still holds while the user has at least one foreground tab; when all tabs are backgrounded the worker may die and re-sync on resume. This isn't worse than per-tab Workers (which die with their tab on iOS too), but the cross-tab payoff is smaller on iOS than on desktop.

### Option 3 — Rust core in a Web Worker on the host page (`dot.li`)

Skip the protocol iframe entirely. The host page at `dot.li` directly spawns a Web Worker containing the Rust core. Product iframes are children of the host page.

```
┌─ tab @ dot.li ──────────────────────────────────────────────────────────┐
│  apps/host — topbar + Rust core orchestration                           │
│                                                                         │
│  ┌─ Web Worker (dot.li) ─ Rust core ──────────────────────────────────┐ │
│  │  smoldot · DOTNS · content fetch · per-product storage             │ │
│  │  permission gate · all host-API methods                            │ │
│  └────────────────────────────────────────────────────────────────────┘ │
│                                                                         │
│  ┌─ product iframe (opaque origin via sandbox) ──────────────────────┐  │
│  │  src = "dot.li/__product/<cid>/..." (SW intercepts)               │  │
│  │  ↕ MessageChannel ↔ Web Worker (Rust core)                        │  │
│  └───────────────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────────┘
```

**Trade-offs:**

- Simplest. One frame, one origin, one worker.
- Smoldot's chain-state IndexedDB and WebSocket connections live on the user-visible `dot.li` origin. Slight UX wart (DevTools shows weird WSs on the user-visible domain) and a hosting concern (CDN behavior on `dot.li` storage).
- Cross-tab: per-tab core unless we promote the worker to a SharedWorker on `dot.li`.
- Removes the host.dot.li origin entirely from the topology. Simplifies DNS / nginx; removes a vite build.
- Topbar UI and chain state share an origin → topbar JS could (in principle) read the core's state directly without going through postMessage. Convenient but blurs the API/UI separation.

### Option 4 — Hybrid: per-tab Rust core, chain core in SharedWorker

Two cores. The per-tab Rust core handles product-specific work (account, signing, storage, permissions, content). It connects to a separate SharedWorker that owns *only* chain access (smoldot/RPC) and DOTNS resolution.

```
┌─ tab A @ dot.li ──────────────────────────┐
│  ┌─ protocol iframe @ host.dot.li ──────┐ │
│  │  ┌─ Web Worker — per-tab Rust core ┐ │ │     ┌─ SharedWorker @ host.dot.li ─┐
│  │  │  account · signing · storage     │ │ │     │  Rust core (chain only):     │
│  │  │  permissions · content delivery  │ │ │     │   • smoldot                  │
│  │  │  remote_chain_* ─ port ──────────┼─┼─┼────▶│   • DOTNS resolver           │
│  │  └──────────────────────────────────┘ │ │     │   • content fetch (IPFS)     │
│  │  ┌─ product iframe ─────────────────┐ │ │     └──────────────────────────────┘
│  │  │ opaque origin                    │ │ │             ▲
│  │  └──────────────────────────────────┘ │ │             │ same SW from
│  └───────────────────────────────────────┘ │             │ other tabs
└────────────────────────────────────────────┘             │
┌─ tab B @ dot.li ──────────────────────────┐              │
│  (same shape) ────────────────────────────┼──────────────┘
└────────────────────────────────────────────┘
```

**Trade-offs:**

- Best-of-both-worlds: chain expense shared across tabs (the heavy thing), per-product state isolated per-tab (the light thing).
- Two Rust cores compiled and shipped — but they can share the same source crate with feature flags ("chain-only" vs "products"). Bundle size goes up but not by much; chain core has most of the LOC anyway.
- Two postMessage hops for chain methods (product → per-tab core → shared core). Throughput is fine; latency is two `postMessage` (~0.1ms each on modern browsers).
- Lifetime: SharedWorker survives across tab closes; per-tab core dies with the tab.
- iOS lifecycle: as in Option 2, the SharedWorker's amortization benefit shrinks when all tabs are backgrounded (iOS may suspend it). No special fallback needed — the per-tab core handles its own work either way.
- Most complex of the four. Justified only if Option 1's per-tab smoldot cost is measured to be a real problem.

## Comparison

| Axis | Option 1 (per-tab core in protocol iframe) | Option 2 (SharedWorker core on host.dot.li) | Option 3 (per-tab core on dot.li) | Option 4 (hybrid) |
|---|---|---|---|---|
| Frames | host + protocol + product | host + protocol(thin) + product | host + product | host + protocol + product |
| Rust cores per origin | 1 per tab | 1 across all tabs | 1 per tab | 1 per tab + 1 shared |
| Smoldot instances | N (per tab) | 1 (across all tabs) | N (per tab) | 1 (across all tabs) |
| host.dot.li used | yes (visible iframe) | yes (SW host) | no | yes (both) |
| Cross-tab session/chain sync | manual (BroadcastChannel) | automatic (one core sees all tabs) | manual | automatic for chain, manual for session |
| Safari iOS path | works as-is | works (16.0+); cross-tab benefit reduced when all tabs backgrounded | works | works (16.0+); same iOS caveat as Option 2 |
| Complexity | low | medium | low | high |
| Eliminates `*.app.dot.li` | yes | yes | yes | yes |
| Eliminates `host.dot.li` | no | no | yes | no |

## Recommendation

**Land Option 1 first; promote to Option 2 if and only if N-tab measurements warrant it.**

Reasons:

1. Option 1 is the minimum viable deviation from today's topology. It removes `*.app.dot.li` (the user's primary ask), it puts the Rust core on its existing isolated origin (`host.dot.li`), and it preserves the per-tab core lifecycle that's easy to reason about. It's also what the wire-format unification and chain-provider unification work converges on naturally — those issues already assume a per-tab Rust core in a Web Worker; Option 1 just specifies *which* iframe spawns it.

2. Option 2's win is real but only if N is real. Today's typical dot.li session is N=1 to N=2 concurrent products. The smoldot cost of N=2 cores in Rust-WASM is roughly the cost of N=2 in JS today (which we've shipped without complaint). The SharedWorker plumbing buys nothing until N=5+ becomes a documented use case.

3. Option 3 trades architectural cleanliness for the wrong things: it eliminates `host.dot.li` (saving one DNS record and one nginx route — pennies) at the cost of putting smoldot's WebSocket churn on the user-visible origin and tying chain warm-start state to whatever `dot.li`'s storage policy looks like. The current isolation is worth keeping.

4. Option 4 is the right answer if and only if Option 1's per-tab smoldot becomes a measured problem. It's sized appropriately for that future. Designing it now is gold-plating.

## Migration considerations

To get from today's topology to Option 1:

1. **Ship the Rust core** (`truapi-server` WASM) loaded by the protocol iframe at `host.dot.li`. Today it's loaded by `apps/host` via a Worker spawned from the host page; move that spawn into `apps/protocol`. The Worker is on `host.dot.li`'s origin in either case if we're already importing `@parity/host-shared/dist/worker-runtime.js?worker` from the protocol iframe.

2. **Move content fetching into the Rust core**. Today `apps/sandbox` does P2P/Helia. Port that to Rust (libp2p in Rust-WASM, or HTTP gateway behind a feature flag). Add a `content_fetch(cid) → bytes` method to the protocol surface.

3. **Add a service worker on `host.dot.li`** that intercepts `/__product/<cid>/*` and serves from the core's content cache. Register at protocol-iframe boot.

4. **Switch the product iframe to opaque origin**. Use `sandbox="allow-scripts allow-forms allow-pointer-lock allow-popups"` (note: no `allow-same-origin`). Verify products handle the storage no-op correctly — they should already, in the TrUAPI model.

5. **Render the product iframe inside the protocol iframe**. The protocol iframe goes from `display: none` to full-viewport; the host page's topbar overlays it (or wraps via flex layout).

6. **Retire `apps/sandbox` and `*.app.dot.li`**. Drop the vite build, the nginx wildcard route, and the DNS record.

7. **DOTNS resolution moves into the core** (also covered by the layer-2 ownership work).

The order matters: 1, 2, 3, 4 can land independently and behind feature flags. 5, 6 require the previous steps to be working in production. 7 is parallel to 1.

To get from Option 1 to Option 2 later:

1. Wrap the per-tab Worker in a SharedWorker registration; feature-detect (`typeof SharedWorker !== "undefined"`) and fall back to a per-tab Worker if missing — the API is broadly available now (Safari iOS 16+ included), so this fallback is mainly for very old browsers.
2. The per-iframe protocol iframe becomes a thin port-relay.
3. The Rust core's storage layer needs to handle multiple concurrent product label namespaces; today's per-tab core already needs this, no change.

## What native (iOS / Android) keeps in mind

Native dotli runs the Rust core in-process via UniFFI. There are no iframes; the product runs in a WebView. The architecture decisions above are web-specific. Native parity is mostly automatic — UniFFI generates Swift/Kotlin bindings for the same `Platform` trait, the WebView talks to the in-process core via a local-loopback transport. The thing native doesn't get for free is **per-tab vs cross-tab smoldot sharing** — that's not a concept on mobile (one app instance), so the question is moot. Native always behaves like "one core per app instance", same as Option 3 conceptually.

## Open questions for the team

1. **Content fetching in Rust-WASM.** Do we want libp2p/Helia in the Rust core, or do we keep an HTTP gateway path? libp2p in Rust-WASM is doable but adds significant binary size. HTTP gateway is simpler but adds a server dependency.

2. **Service worker scope on `host.dot.li`.** A per-cid SW (option C in the delivery section) gives us tighter quota / failure isolation but more registrations. A single SW is simpler. Decide based on observed product behavior.

3. **Topbar layout.** Today the topbar is host-page DOM. If the protocol iframe goes full-viewport, the topbar either (a) stays on the host page and z-indexes above the iframe, or (b) the topbar moves into the protocol iframe and the host page becomes a degenerate wrapper. Option (a) is simpler but means the topbar can't read core state directly (has to postMessage in). Option (b) collapses host page → protocol iframe but raises questions about deep linking and URL parsing.

4. **Cross-product UX features.** If the user expects products to share auth (logged in once, all products see the session), that needs explicit support whether the core is per-tab or shared. Shared state via the core's KV; cross-tab sync via BroadcastChannel or SharedWorker. Decide before committing to per-tab vs shared.

# RFC-0021: Route Relay

|                 |                                                                                                                                                     |
| --------------- | --------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Start Date**  | 2026-05-15                                                                                                                                          |
| **Description** | Host API surface that lets an app publish its internal route to the Host's address bar, read the current route, and observe back/forward navigation |
| **Authors**     | @pgherveou                                                                                                                                          |

## Summary

Add three host calls (`host_route_get`, `host_route_set`, `host_route_changed`) that relay an opaque per-app route string between the embedded app and the Host shell. This makes in-app navigation deep-linkable, shareable, and reload-stable, and lets the app react to Host back/forward, without giving the app any access to the Host's URL bar.

## Motivation

Apps that run inside a Web host are loaded in a webview / iframe. The visible address bar belongs to the Host, not to the app. Today this means:

- An app can call `history.pushState` / mutate `window.location.hash` internally, but those changes are invisible to the user, are not shareable, and do not survive a reload — the Host re-launches the wrapper at `https://dot.li/<app>` with no fragment preserved.
- At bootstrap the app cannot tell which sub-route the user intended to open. There is no way to deep-link into, say, a specific method in the TrUAPI Playground, a specific chat in a messenger app, or a specific item in a marketplace app.
  We need a small, symmetric channel: the app owns its route format, the Host owns the address bar, and the two stay in sync.

## Stakeholders

- **Product developers** (consumers): want shareable deep links and reload-stable routes without re-implementing routing per host.
- **Host implementors**: own the address bar, history stack, and how routes are rendered to the user (path, fragment, query, etc.).
- **End users**: copy / share / reload URLs and expect them to land where they were.

## Explanation

### `host_route_get`

```rust
fn host_route_get() -> Result<HostRouteGetResponse, GenericErr>

struct HostRouteGetResponse {
    /// The current route the Host holds for this app.
    /// `None` if no route is set (app's home).
    route: Option<String>,
}
```

Returns the current route the Host holds for this app. At bootstrap this is the route the Host was launched with (e.g. `Permissions/host_device_permission`); afterwards it reflects the most recent `host_route_set` and any Host-driven changes (back/forward, pasted URL). The Host does not interpret the string; the app defines its own format.

Typical use is one call at bootstrap to restore deep-linked state.

### `host_route_set`

```rust
fn host_route_set(req: HostRouteSetRequest) -> Result<(), GenericErr>

struct HostRouteSetRequest {
    /// Opaque route segment defined by the app.
    route: String,
    /// `true` replaces the current history entry (analog of `history.replaceState`).
    /// `false` pushes a new entry (analog of `history.pushState`).
    replace: bool,
}
```

Called whenever the app navigates internally. The Host renders `route` as part of the user-visible URL so it can be copied, shared, and reloaded. The exact rendering (path segment, fragment, query parameter) is the Host's choice; the protocol does not constrain it.

Setting `route` to the empty string clears the route (app's "home").

### `host_route_changed`

```rust
fn host_route_changed() -> Stream<HostRouteChangedEvent, GenericErr>

struct HostRouteChangedEvent {
    /// New route. `None` when the user is at the app's home.
    route: Option<String>,
}
```

Emits when the route changes from outside the app: Host back/forward, or a pasted URL while the app is running. The Host MUST NOT emit for changes that originated from `host_route_set` in this app session (no echo loop). The stream does not emit the initial value; the app reads that from `host_route_get`.

### Lifecycle

1. App starts → calls `host_route_get` → restores deep-linked state.
2. App subscribes to `host_route_changed` → handles back/forward and pasted URLs.
3. On every internal navigation → calls `host_route_set` with `replace=false` (or `true` for redirects / non-history-worthy transitions).

### Semantics

- **Opaque.** The Host treats `route` as an opaque byte string and does not parse it. Apps define their own grammar.
- **Length / charset.** Routes MUST be valid UTF-8. Hosts MAY impose a maximum length (recommended: at least 2048 bytes) and MUST return `GenericErr` for over-long routes; apps should avoid stuffing application state into the route.
- **Permissioning.** No permission prompt. The route is information the app already has; relaying it to the address bar does not disclose anything new to the user. (The Host MAY still rate-limit `host_route_set` to mitigate history-stack abuse.)

### Web-host shim

A Web host MAY monkey-patch `history.pushState`, `history.replaceState`, and the `popstate` / `hashchange` events on the iframe's `window` to call these TrUAPI methods underneath. With that shim in place, apps written against the standard web History API "just work" — their existing router (Next.js, React Router, vanilla `pushState`, etc.) drives the Host address bar with no TrUAPI-specific code. The shim is a Host implementation detail, not part of the protocol; non-web hosts implement these methods natively.

## Drawbacks

- Hosts must implement the no-echo rule on `host_route_changed` correctly, or naive apps will loop.

## Compatibility

Purely additive.

## Future Directions

- `host_route_set_title` for per-route titles in the Host chrome.
- Fold `host_route_get` into the connection handshake to save a bootstrap round-trip.

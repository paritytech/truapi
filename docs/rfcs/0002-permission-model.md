---
title: "Permission Model for Host API"
owner: "@johnthecat"
---

# RFC 0002 — Permission Model for Host API

> **NOTE (2026-05-26): `remote_permission` reverted to a single permission.**
> This RFC below specifies batched remote-permission requests (`remote_permission` taking a `Vec<RemotePermission>`). That part has been rolled back: `remote_permission` again accepts a single `RemotePermission`. After the initial implementation it became clear that the batched API is hard to justify to the end user — a single prompt covering several distinct grants produces bad UX (the user cannot reason about or selectively approve what they are consenting to). The rest of this RFC (device permissions, lifecycle, persistence, implicit triggering) still stands; only the batching of remote permissions is reverted.

## Summary

The Host API currently has two underdefined permission calls — `host_device_permission` and `remote_permission` — that lack coverage for several device capabilities (NFC, Clipboard, OpenUrl, Biometrics), do not support batched remote-permission requests, and have no specified lifecycle for when prompts occur or how decisions are persisted. This RFC defines the complete set of device and remote permissions, updates the `remote_permission` signature to accept a batch, specifies that permission decisions are prompted once and then stored permanently, and establishes that business methods (`host_sign_raw`, `host_sign_payload`, `host_create_transaction`, `host_create_transaction_with_non_product_account`, `remote_statement_store_submit`, `remote_preimage_submit`, `remote_chain_transaction_broadcast`) implicitly trigger permission prompts if permission has not yet been granted.

## Motivation

The current Host API design document defines two permission functions:

```rust
fn host_device_permission(
  permission: DevicePermissionRequest
) -> Result<bool, GenericErr>;

fn remote_permission(
  permission: RemotePermission
) -> Result<bool, GenericErr>;
```

`DevicePermissionRequest` covers only `Camera`, `Microphone`, `Bluetooth`, and `Location`. This is insufficient: products that need clipboard access, the ability to open external URLs in the system browser, biometric authentication, or NFC have no mechanism to declare or request these capabilities.

`remote_permission` accepts a single `RemotePermission` value but the notes call for batched requests. A product that needs HTTP access to several endpoints, plus chain submission, would need to make multiple round-trips and prompt the user multiple times for what is logically a single decision.

Neither function has a documented lifecycle: it is unspecified when the Host prompts the user, whether the decision survives a session restart, or what happens when a business method is called without an explicit prior permission check.

The result is that products cannot predictably reason about which operations will succeed, and hosts cannot implement consistent permission UX without filling in unspecified behaviour themselves.

**Requirements for the solution:**

1. `DevicePermission` must cover: Camera, Microphone, Bluetooth, NFC, Location, Clipboard, OpenUrl, Biometrics.
2. `RemotePermission` must cover: HTTP/HTTPS/WS/WSS access (with domain patterns), chain transaction broadcasting, preimage/bulletin-chain submission, and statement-store submission.
3. `remote_permission` must accept a batch of permissions so a product can declare all its remote needs in a single call.
4. `host_device_permission` remains a single-permission call.
5. The Host prompts the user the first time a permission is requested; subsequent calls for the same permission resolve immediately from persisted state.
6. Business methods that require a permission must implicitly trigger the prompt if the permission has not yet been resolved, returning `PermissionDenied` only when the user actively denies.

## Detailed Design

### Note on Web APIs Integration

Since some interactions with the Web platform cannot be covered by the Host API, the permission system is coupled to the sandbox implementation.
That means that fetch requests, WebSockets, WebRTC, and device permissions should be handled by the Host's sandbox implementation.
The exact mechanism is out of scope for this RFC.

### Updated Type Definitions

#### DevicePermission

The existing `DevicePermissionRequest` enum is renamed to `DevicePermission` for consistency and extended:

```rust
enum DevicePermission {
  Notifications,
  Camera,
  Microphone,
  Bluetooth,
  NFC,
  Location,
  Clipboard,
  OpenUrl,
  Biometrics
}
```

- **Notifications** — added to support send native notifications to the user.
- **NFC** — added to support tap-to-pay and NFC tag interactions.
- **Location** — already present; `GPS` from earlier notes is treated as a duplicate and dropped.
- **Clipboard** — read/write access to the system clipboard.
- **OpenUrl** — permission to open URLs in the system browser (external navigation out of the host application).
- **Biometrics** — permission to trigger biometric authentication (fingerprint, Face ID, etc.).

The function signature changes only in the type name:

```rust
fn host_device_permission(
  permission: DevicePermission
) -> Result<bool, GenericErr>;
```

A single call requests a single permission. Batching is not supported for device permissions; each capability warrants its own prompt.

#### RemotePermission

```rust
enum RemotePermission {
  // Access to Web 2.0 APIs.
  // Each entry is a domain or wildcard subdomain pattern:
  //   "api.coingecko.com"  — exact domain match
  //   "*.coingecko.com"    — all subdomains of coingecko.com
  //   "*"                  — allow all HTTP/WS requests (wildcard)
  Remote(Vec<String>),
  // Access to WebRTC, can be potentially harmful for privacy because
  // it can expose the user's IP address.
  WebRTC,
  // Broadcast signed transactions to any Substrate chain via
  // remote_chain_transaction_broadcast.
  ChainSubmit,
  // Submit preimage data to the bulletin chain via
  // remote_preimage_submit.
  PreimageSubmit,
  // Submit statements to the statement store via
  // remote_statement_store_submit.
  StatementSubmit
}
```

#### Updated remote_permission Signature

`remote_permission` now accepts a `Vec<RemotePermission>` to allow products to declare multiple remote permissions in a single prompt:

```rust
fn remote_permission(
  permissions: Vec<RemotePermission>
) -> Result<bool, GenericErr>;
```

The return value is a single `bool`. A `true` result means all requested permissions were granted. A `false` result means the user denied at least one permission in the batch; the host MAY persist partial grants for those entries the user approved, but the function still returns `false`. Products that need to know which specific permissions were denied should call `remote_permission` with individual entries.

### HTTP/WS Domain Matching Semantics

Each string entry inside `Remote(Vec<String>)` is matched against the host portion of a request URL. The matching rules are:

- **Exact domain**: `"api.coingecko.com"` matches requests to `https://api.coingecko.com` only.
- **Wildcard subdomain**: `"*.coingecko.com"` matches any single subdomain level, e.g. `api.coingecko.com`, `cdn.coingecko.com`, but NOT `coingecko.com` itself or `deep.api.coingecko.com` (two levels).
- **Wildcard all**: `"*"` matches any HTTP(S) host. This is a broad grant and host implementations SHOULD present a more prominent warning to the user when this entry appears.

Matching is case-insensitive. The scheme is always HTTP, HTTPS, WS or WSS.

### Permission Lifecycle

1. **First request** — When a permission is requested for the first time (either via an explicit permission API call or implicitly by a business method), the Host prompts the user with an approval dialog.
2. **Decision persisted** — The user's decision (grant or deny) is stored by the Host and associated with the product identity. The persistence scope is indefinite; the decision survives app restarts and session boundaries.
3. **Subsequent requests** — All subsequent calls for the same permission resolve immediately from persisted state without showing a prompt. The product does not need to re-request a permission it has already obtained.
4. **Revocation** — Permission revocation is out of scope for this RFC. Hosts MAY provide a settings interface for users to revoke permissions, but the protocol does not define a revocation notification to the product.

Products MAY request permissions lazily (on first use) or upfront during initialization. Both patterns are valid. Requesting upfront is recommended when the product can predict its needs, as it provides a better user experience by batching consent into a single moment.

### Implicit Permission Triggering by Business Methods

The following business methods gate on a specific `RemotePermission` and MUST internally trigger a permission prompt if the permission has not yet been resolved:

| Business Method                      | Required Permission                 |
| ------------------------------------ | ----------------------------------- |
| `remote_chain_transaction_broadcast` | `RemotePermission::ChainSubmit`     |
| `remote_preimage_submit`             | `RemotePermission::PreimageSubmit`  |
| `remote_statement_store_submit`      | `RemotePermission::StatementSubmit` |

The following business methods relate to signing and require the user's active consent via their own approval flow (e.g. a signing confirmation dialog). They return `PermissionDenied` when the user cancels or denies that confirmation — this is distinct from the remote permission system but is documented here for completeness:

| Business Method                                    | Error on Denial                          |
| -------------------------------------------------- | ---------------------------------------- |
| `host_sign_raw`                                    | `SigningErr::PermissionDenied`           |
| `host_sign_payload`                                | `SigningErr::PermissionDenied`           |
| `host_create_transaction`                          | `CreateTransactionErr::PermissionDenied` |
| `host_create_transaction_with_non_product_account` | `CreateTransactionErr::PermissionDenied` |

For the remote-gated methods: if the user has already granted the relevant `RemotePermission`, the business method proceeds without prompting. If permission is not yet resolved, the Host presents the permission prompt first, then proceeds or returns `GenericErr` (wrapping a permission-denied reason) if the user denies.

> **Design rationale:** Requiring products to always call `remote_permission` before calling `remote_statement_store_submit` would add boilerplate with no safety benefit, since the Host already controls whether the operation proceeds. Implicit triggering lets simple products work correctly without explicit permission preambles while preserving full user control.

### Complete Updated Interface (Permissions Section)

```rust
// Updated DevicePermission enum (replaces DevicePermissionRequest)
enum DevicePermission {
  Notifications,
  Camera,
  Microphone,
  Bluetooth,
  NFC,
  Location,
  Clipboard,
  OpenUrl,
  Biometrics
}

// Updated RemotePermission enum
enum RemotePermission {
  Remote(Vec<String>),
  WebRTC,
  ChainSubmit,
  PreimageSubmit,
  StatementSubmit
}

// Single-permission device request (unchanged semantics, updated type name)
fn host_device_permission(
  permission: DevicePermission
) -> Result<bool, GenericErr>;

// Batched remote permission request (signature updated: Vec<RemotePermission>)
fn remote_permission(
  permissions: Vec<RemotePermission>
) -> Result<bool, GenericErr>;
```

### Serialization Impact

The `Payload` enum in the transport layer derives its action indices from the order of Host API methods. The changes here affect two existing entries:

- `host_device_permission_request` / `host_device_permission_response` — the payload type changes from `DevicePermissionRequest` to `DevicePermission`. This is a rename; the variant index in the `Payload` enum is unchanged.
- `remote_permission_request` / `remote_permission_response` — the payload changes from `Versioned<RemotePermission>` to `Versioned<Vec<RemotePermission>>`. This is a structural change.

Both changes are breaking at the wire level. See Compatibility in Alternatives below.

## Drawbacks

**Batch semantics are coarse.** `remote_permission` returns a single boolean for a batch. A product requesting `[Http(["api.example.com"]), ChainSubmit]` cannot distinguish "Http denied, ChainSubmit granted" from "both denied" without making two separate calls. This is a deliberate simplicity trade-off; products that need fine-grained feedback should issue individual permission requests.

**Wildcard `"*"` for HTTP is permissive.** Allowing a product to request HTTP access to any domain with a single entry reduces the safety value of the permission system for that specific case. The Host MUST communicate this clearly in its prompt UI.

**No revocation protocol.** Once a product has a permission, it retains it until the user manually revokes it through host settings. There is no push notification to the product when a permission is revoked. Products should handle `GenericErr` responses from business methods as a signal to re-prompt if desired.

**Implicit triggering couples permission UX to business-method UX.** When `remote_statement_store_submit` triggers a permission prompt inline, it interrupts the flow the product expected to be a simple network call. Products that want a controlled UX should call `remote_permission` proactively before entering the relevant flow.

## Alternatives

### Compatibility and Migration

This RFC introduces breaking changes:

1. **`DevicePermissionRequest` → `DevicePermission`**: Implementors referencing `DevicePermissionRequest` by name must update to `DevicePermission`. The wire encoding of the existing four variants (`Camera`, `Microphone`, `Bluetooth`, `Location`) is unchanged — variant indices are preserved.

2. **`remote_permission` argument type**: The payload changes from a single `RemotePermission` to `Vec<RemotePermission>`. Any host or product implementation that encodes or decodes `remote_permission_request` must be updated. A single-element `Vec` is the direct migration path for callers that previously passed one permission.

3. **New `DevicePermission` variants**: `NFC`, `Clipboard`, `OpenUrl`, and `Biometrics` are new enum variants appended after the existing four. Older hosts that receive an unrecognized variant SHOULD return `false` (permission not granted) rather than an error, to allow graceful degradation.

4. **New `RemotePermission` variants**: `PreimageSubmit` and `StatementSubmit` are new variants. Older hosts that receive an unrecognized variant in a batch SHOULD treat it as denied and return `false`.

Migration is straightforward for implementors following semantic versioning: bump the major version, update type names, and wrap single-permission calls in a `vec![...]`.

### Prior Art and References

- [Host API Design Document](../design/truapi-protocol.md) — the current API definition this RFC amends.
- [Web platform Permissions API](https://www.w3.org/TR/permissions/) — the W3C model for browser permission prompts: one-time prompt, persisted decision, queryable state. The lifecycle defined here follows the same pattern.
- [Android permission model](https://developer.android.com/guide/topics/permissions/overview) — similar grant-once-persist approach; also distinguishes install-time (declared) permissions from runtime (prompted) permissions.

## Unresolved Questions

1. **Partial-batch grants**: Should `remote_permission` return a structured result (e.g. `Vec<(RemotePermission, bool)>`) instead of a single `bool`? This would allow products to proceed with whichever permissions were granted. The current proposal opts for simplicity, but this trade-off should be confirmed by the fellowship.

2. **Permission query API**: Should there be a `remote_permission_status` / `host_device_permission_status` call that returns the current persisted state without prompting? This would allow products to check permission state on startup and adapt their UI accordingly without triggering a prompt.

3. **`OpenUrl` scope**: `OpenUrl` is modelled as a device permission (single prompt, persisted). An alternative interpretation is that it should be a remote permission variant (since the destination is a URL). Which namespace is more appropriate?

4. **HTTP/WS permission enforcement point**: The RFC specifies that `RemotePermission::Remote` governs outbound HTTP/WS requests, but the transport layer routes all network calls through the Host. How the Host enforces HTTP domain matching at the transport level (interception vs. validation before handing off) is an implementation detail left unspecified — should this RFC say more?

5. **Permission revocation notifications**: A follow-on RFC could define a `host_permission_revoked_subscribe` subscription that products receive when the user revokes a permission via Host settings.

6. **Permission expiry**: Time-bounded permissions (e.g. "allow for this session only") would give users finer control and reduce the risk of persistent over-permission.

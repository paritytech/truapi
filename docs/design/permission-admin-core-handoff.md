# Handoff: Move dotli Permission Admin to Core-Owned State

## Goal

Make the shared Rust core the source of truth for product permission decisions,
including the host admin screen.

Today the Rust core already owns protocol permission prompting and persistence
through `PermissionsService`, but dotli still has a parallel permission store
for the topbar admin UI and iframe `allow` handling. This causes two sources of
truth:

- Core permission state: stored through `truapi-platform::CoreStorage` under
  typed `CoreStorageKey::PermissionDecision` slots.
- dotli UI permission state: stored directly in browser `localStorage` under
  `dotli:permissions:<label>`.

The feature is to expose core permission administration APIs, then refactor the
dotli permission screen to read and update permissions through the core.
Browser-only derived behavior, such as rebuilding the iframe `allow` attribute
and reloading the iframe, should stay in dotli.

## Current State

### Core

The core already persists permission decisions.

Relevant files:

- `rust/crates/truapi-server/src/host_logic/permissions.rs`
- `rust/crates/truapi-server/src/runtime.rs`
- `rust/crates/truapi-server/src/host_core.rs`
- `rust/crates/truapi-server/src/wasm/mod.rs`

Current behavior:

- `PermissionsService::check_or_prompt_device(...)`
  - reads a cached device decision first;
  - prompts the host only when no decision exists;
  - persists a real user decision.
- `PermissionsService::check_or_prompt_remote(...)`
  - same flow for remote permissions.
- Decisions are stored as SCALE-encoded `Decision::{Granted, Denied}`.
- Keys are represented as typed `CoreStorageKey::PermissionDecision` slots.
  The inner storage key is still generated canonically by the permission
  service.
- Device and remote permissions use independent namespaces.
- Prompt callback errors fail closed for the current call but are not cached.

### dotli

dotli still owns a second permission store.

Relevant files:

- `hosts/dotli/packages/ui/src/permissions.ts`
- `hosts/dotli/packages/ui/src/host-callbacks/PromptPermission.ts`
- `hosts/dotli/packages/ui/src/topbar.ts`
- `hosts/dotli/packages/ui/src/bridge.ts`
- `hosts/dotli/packages/ui/tests/permissions.test.ts`

Current behavior:

- `permissions.ts` stores `ask | granted | denied` in `localStorage`.
- `PromptPermission.ts` reads/writes that store before/after modal prompts.
- `topbar.ts` renders and updates the admin screen from that store.
- `bridge.ts` uses `buildAllowAttribute(label)` from that store when creating
  the iframe sandbox.
- `dotli:device-permission-changed` reloads the iframe so a changed `allow`
  attribute takes effect.

This UI-side store should stop being authoritative.

## Target Model

The core owns the permission decision state:

```
Product request
  |
  v
Core permission service
  | reads/writes
  v
Host CoreStorage capability
```

The host admin screen uses core admin APIs:

```
dotli topbar admin
  |
  v
Core permission admin API
  | reads/writes
  v
Same core permission state used by product requests
```

dotli still owns browser-specific projection:

- Which device permissions map to iframe `allow` directives.
- When a changed permission requires iframe reload.
- Topbar rendering and events.
- Hiding permissions that dotli cannot enforce, such as `OpenUrl`.

## Proposed Core API

Expose host-side admin APIs on the embedding runtime. These are not product
TrUAPI methods. They are host control-plane methods, similar to
`disconnectSession`, `cancelPairing`, and `notifySessionStoreChanged`.

Suggested Rust API on `HostCore`:

```rust
pub async fn list_permissions(&self) -> Result<Vec<PermissionAdminEntry>, PermissionAdminError>;

pub async fn set_permission(
    &self,
    request: PermissionAdminRequest,
) -> Result<(), PermissionAdminError>;
```

Suggested data model:

```rust
pub enum PermissionAdminKind {
    Device(v01::HostDevicePermissionRequest),
    Remote(v01::RemotePermissionRequest),
}

pub enum PermissionAdminStatus {
    Ask,
    Granted,
    Denied,
}

pub struct PermissionAdminEntry {
    pub permission: PermissionAdminKind,
    pub status: PermissionAdminStatus,
}

pub struct PermissionAdminRequest {
    pub permission: PermissionAdminKind,
    pub status: PermissionAdminStatus,
}
```

Semantics:

- `Ask` means no stored decision. Implement by clearing the corresponding core
  permission key.
- `Granted` writes `Decision::Granted`.
- `Denied` writes `Decision::Denied`.
- `list_permissions` returns the full admin surface the host should show, with
  `Ask` for missing entries.

Open design choice:

- Full-surface `list_permissions` needs a canonical list of supported admin
  permissions. The least risky starting set should mirror dotli's existing
  `ALL_PERMISSIONS`:
  - device: `Notifications`, `Camera`, `Microphone`, `Location`, `Bluetooth`,
    `NFC`, `Clipboard`, `Biometrics`;
  - remote: `ChainSubmit`, `PreimageSubmit`, `StatementSubmit`.
- `OpenUrl` should remain hidden/auto-granted in dotli because dotli cannot
  enforce it.
- `Remote` and `WebRtc` should remain hidden/auto-granted in dotli unless the
  host can actually enforce them.

## Implementation Plan

### 1. Extend core permission service

File: `rust/crates/truapi-server/src/host_logic/permissions.rs`

Add helpers:

- `all_admin_permissions() -> Vec<PermissionAdminKind>` or equivalent.
- `permission_status(...) -> PermissionAdminStatus`
  - reads storage and maps missing to `Ask`.
- `set_permission_status(...)`
  - `Ask` clears storage key;
  - `Granted` / `Denied` write SCALE-encoded `Decision`.

Keep the key generation centralized. Do not duplicate key formatting outside
this module.

Tests to add:

- `list_permissions_returns_ask_for_missing_decisions`.
- `set_permission_status_granted_writes_core_decision`.
- `set_permission_status_denied_writes_core_decision`.
- `set_permission_status_ask_clears_core_decision`.
- Remote permission keys remain canonicalized.

### 2. Expose host runtime APIs in Rust

File: `rust/crates/truapi-server/src/host_core.rs`

Add methods:

- `list_permissions`
- `set_permission`

These should call into the underlying `TrUApiCore` / runtime host without going
through the product dispatcher.

Likely supporting files:

- `rust/crates/truapi-server/src/core.rs`
- `rust/crates/truapi-server/src/runtime.rs`

The runtime already has access to:

- the `Platform` implementation;
- `CoreStorage`;
- `PermissionsService`.

The admin APIs should not call prompt callbacks. They only read/update stored
decisions.

### 3. Expose WASM bindings

File: `rust/crates/truapi-server/src/wasm/mod.rs`

Add `wasm_bindgen` methods on `WasmHostCore`, for example:

- `listPermissions(): Promise<Uint8Array>`
- `setPermission(payload: Uint8Array): Promise<void>`

Use SCALE payloads rather than ad hoc JS objects, consistent with the rest of
the callback/runtime boundary.

Suggested encoding:

- Reuse the admin DTOs from Rust with `Encode` / `Decode`.
- Generate or manually expose TS codecs in `@parity/truapi-host/callbacks` or
  `@parity/truapi-host-wasm` if needed.

### 4. Thread methods through worker provider

Files:

- `js/packages/truapi-host-wasm/src/runtime.ts`
- `js/packages/truapi-host-wasm/src/worker-protocol.ts`
- `js/packages/truapi-host-wasm/src/worker-runtime.ts`
- `js/packages/truapi-host-wasm/src/web/create-worker-host-runtime.ts`

Add provider methods analogous to:

- `disconnectSession`
- `cancelPairing`
- `notifySessionStoreChanged`

Suggested provider surface:

```ts
type PermissionAdminStatus = "ask" | "granted" | "denied";

interface PermissionAdminEntry {
  permission: ...;
  status: PermissionAdminStatus;
}

interface TrUApiHostCoreProvider {
  listPermissions(): Promise<PermissionAdminEntry[]>;
  setPermission(entry: PermissionAdminEntry): Promise<void>;
}
```

Implementation detail:

- Worker protocol should carry request ids and responses, like
  `disconnectSession`.
- Decode/encode at the package boundary so dotli can consume typed objects.

### 5. Refactor dotli permission state

Files:

- `hosts/dotli/packages/ui/src/permissions.ts`
- `hosts/dotli/packages/ui/src/topbar.ts`
- `hosts/dotli/packages/ui/src/host-callbacks/PromptPermission.ts`
- `hosts/dotli/packages/ui/src/bridge.ts`

New responsibility split:

- `permissions.ts` should stop being the authority for grants/denials.
- Keep dotli-only metadata there:
  - permission display list;
  - labels/icons if needed;
  - whether a permission maps to an iframe Permissions Policy directive;
  - `buildAllowAttribute(...)`, but make it consume core permission entries
    rather than read localStorage directly.
- `topbar.ts` should:
  - call `currentCoreProvider.listPermissions()` when a product is loaded or
    the popover opens;
  - render statuses from the core;
  - call `currentCoreProvider.setPermission(...)` on admin changes;
  - dispatch the same reload/update events after the core update succeeds.
- `PromptPermission.ts` should stop reading/writing `dotli:permissions:*`.
  It should only render the modal and return the user's decision. The core will
  persist the decision after the callback returns.
- `bridge.ts` should build iframe `allow` from the core permission state.

Important sequencing for device permissions:

1. User changes a device permission in the admin screen.
2. dotli calls `setPermission`.
3. dotli refreshes its in-memory permission snapshot.
4. dotli dispatches `dotli:device-permission-changed`.
5. bridge reloads the iframe so `allow` is rebuilt from the latest snapshot.

Avoid rebuilding the iframe from stale permission state.

### 6. Keep a small dotli derived cache

The iframe `allow` attribute is synchronous at iframe creation time. The core
admin API is asynchronous.

Recommended dotli approach:

- Keep an in-memory `currentPermissionSnapshot` per product label.
- Hydrate it from `listPermissions()` before rendering a product iframe.
- Update it after `setPermission(...)`.
- `buildAllowAttribute(label, snapshot)` should be pure and synchronous.

Do not persist this snapshot separately. It is a projection of core state.

### 7. Migration / cleanup

Decide how to handle existing `dotli:permissions:<label>` data:

- Preferred for PR simplicity: no migration. The old UI-side store is ignored.
  Users may be prompted again once after the new core-owned admin lands.
- If migration is required: one-time read old `dotli:permissions:<label>` and
  write equivalent core decisions with `setPermission`, then stop reading the
  old key.

Remove or shrink tests that assert `dotli:permissions:*` persistence.

## Test Plan

### Rust

Run:

```sh
cargo fmt --all -- --check
cargo test -p truapi-server -p truapi-platform
```

Add targeted tests for:

- permission admin read-all;
- permission admin update to granted;
- permission admin update to denied;
- permission admin reset to ask;
- product request short-circuits after admin grant/deny;
- no prompt callback is called when admin state exists.

### JS / WASM host package

Run:

```sh
npm run build && npm test
```

At least for:

- `js/packages/truapi-host`
- `js/packages/truapi-host-wasm`

Add worker-provider tests for:

- `listPermissions` request/response;
- `setPermission` request/response;
- disposal / worker failure behavior.

### dotli UI

Run:

```sh
cd hosts/dotli/packages/ui
bun run typecheck
bun run test
```

Update/add tests:

- topbar renders permissions from provider `listPermissions`.
- changing dropdown calls provider `setPermission`.
- `has grants` indicator comes from the core snapshot.
- device permission changes reload iframe after provider update.
- `PromptPermission` does not persist to `dotli:permissions:*`.
- `buildAllowAttribute` uses the core snapshot.

### End-to-end

Run:

```sh
E2E_DOTLI_HOST_PORT=5178 E2E_DOTLI_PLAYGROUND_PORT=3005 make e2e-dotli
```

Manually verify in local dotli:

- Open product.
- Open permissions topbar screen.
- Grant a device permission.
- Confirm iframe reloads and `allow` reflects the grant.
- Deny/reset a permission.
- Confirm subsequent product permission calls use the core decision.

## Risks / Notes

- The current dotli `PromptPermission.ts` returns `false` after granting a
  device permission because the iframe must reload before the browser `allow`
  attribute takes effect. If the core persists that `false` as denied, the new
  flow would be wrong. This must be handled.

  Recommended fix: split browser reload behavior from the prompt decision.
  The prompt callback should return the real user decision to the core, and
  dotli should trigger reload after the core persists `Granted`.

- Some permissions are not enforceable by dotli (`OpenUrl`, possibly `Remote`
  and `WebRtc`). Do not show admin controls that imply enforcement if the host
  cannot enforce them.

- Product-scoping must stay correct. Core permission keys are stored through
  `CoreStorageKey::PermissionDecision`, and the permission service's canonical
  key must include the product context where needed. Verify that dotli passes
  the correct product identity before relying on core permission state for the
  admin UI.

- Avoid making permission admin a product-facing TrUAPI API. This is host
  control-plane functionality, not something products should call.

- Keep the old `dotli:permissions:*` localStorage keys untouched unless a
  migration is explicitly chosen. Removing user data silently is unnecessary
  for this feature.

## Definition of Done

- Core exposes host-side permission admin read/update APIs.
- Admin APIs use the same core permission storage as product permission calls.
- dotli topbar permission screen reads and writes through the core provider.
- dotli no longer treats `dotli:permissions:*` as authoritative.
- Browser-specific iframe `allow` behavior still works.
- Rust, JS/WASM, dotli UI tests pass.
- Local `make e2e-dotli` passes.

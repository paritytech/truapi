# TrUAPI iOS host adapter

*Thin Swift shell over the Rust TrUAPI core (UniFFI). Wire decoding, request routing, and subscription lifecycle stay in the Rust core; products connect through the localhost WebSocket bridge.*

The package lives in the truapi repo next to the Rust core it wraps. `Package.swift` sits at the **repo root** (SPM requires that for git-URL dependencies), with all target paths pointing into `ios/truapi-host/`; the build scripts regenerate the committed outputs from this repo's workspace.

## What this package is for

The `TrUAPIHost` SPM package an iOS host app imports directly. It carries:

- [`Sources/TrUAPIHost/TrUAPIHost.swift`](Sources/TrUAPIHost/TrUAPIHost.swift) — the hand-written shell: `TrUAPIHostCore` (owning wrapper around the UniFFI-generated `NativeTrUApiCore`, with the localhost WS bridge, session controls, and native change notifications), `TrUAPIHostCoreProtocol`, `RuntimeConfig`, and `LocalhostBridgeBootstrap`.
- the Rust core as a binary target — a GitHub release asset by default (`publishedBinaryURL` in the root `Package.swift`), or the locally built `Binaries/truapi_server.xcframework` when `useLocalBinary` is flipped to true.
- `Sources/TrUAPIHost/truapi_server.swift` and `Sources/truapi_serverFFI/include/` — the generated UniFFI bindings.
- [`js/container/`](../../js/container) — the TS lockdown container; built into `Sources/TrUAPIHost/Resources/truapi-container.js` and exposed via `ContainerScriptBundle.load()`.
- `Tests/` — WS-bridge round-trip tests that boot the real Rust core.

The generated bindings and the container bundle are committed build outputs; the xcframework is **gitignored** and distributed as a GitHub release asset. Two scripts split the lifecycle:

```bash
./scripts/rebuild.sh            # regenerate xcframework + bindings + container
                                # from this repo (make xcframework at the root)
./scripts/publish.sh <version>  # zip the built xcframework, upload it to the
                                # "@parity/ios-host <version>" GitHub release,
                                # and point the root Package.swift at it
                                # (URL + checksum)
```

Run `rebuild.sh` after changing anything host-visible — the `NativeTrUApiCore` methods, `HostCallbacks`, the native mirror types in `rust/crates/truapi-server/src/native*`, or `js/container/src` — and commit the regenerated bindings/container together with the source change. To publish from a release PR, add `@parity/ios-host <version>` to its `release:` title. After the release commit passes CI, the release workflow rebuilds and simulator-tests the XCFramework on macOS, uploads it, and makes the `Package.swift` follow-up commit only after the asset is live. `publish.sh` remains available for an ad hoc manual release.

For local iteration without publishing, flip `useLocalBinary = true` in the root `Package.swift` to build against `Binaries/` directly; flip it back before committing.

The embedding app implements the UniFFI-generated `HostCallbacks` protocol directly (defined in `truapi_server.swift`): navigation, push, permissions, auth state, paired-peer disconnects, scoped + core storage, chain JSON-RPC, confirmations, preimage, theme, and feature support. UI-decision callbacks are `async` and awaited by the Rust core.

## Integrating in an iOS app

Add the package as an SPM dependency and link the `TrUAPIHost` product into the app target:

```swift
.package(url: "https://github.com/paritytech/truapi.git", branch: "main")
```

```swift
.product(name: "TrUAPIHost", package: "truapi")
```

Release tags follow the repo-wide `@parity/ios-host@<version>` naming, which SPM's semver resolution does not consume — depend by `branch:` or `revision:` instead. SPM pins the resolved revision in the app's `Package.resolved`; update it (File > Packages > Update in Xcode, or `xcodebuild -resolvePackageDependencies`) after new commits land on the branch.

Run the package tests against an iOS simulator (the xcframework has no macOS slice):

```bash
# from the repo root
xcodebuild test -scheme TrUAPIHost -destination 'platform=iOS Simulator,name=iPhone 16'
```

## Architecture

```text
product app in WKWebView
  Uint8Array frames via @parity/truapi createWebSocketProvider
           |
           v   ws://127.0.0.1:<port>/?t=<token>
TrUAPIHostCore.startWsBridge()
  → libtruapi_server (tokio WS server)
  → Rust dispatcher
```

The product running in the `WKWebView` opens a `WebSocket` to the localhost port + token returned by `startWsBridge`. From there the Rust core handles the wire protocol directly. Outbound responses and host-side capability callbacks (`navigateTo`, `pushNotification`, `cancelNotification`, `devicePermission`, `remotePermission`, `authStateChanged`, core storage, chain JSON-RPC, confirmations, preimage, theme, `featureSupported`, `storage`) reach the embedder through `HostCallbacks`.

## Permissions split

The core's `Permissions` platform trait has two methods, and so does `HostCallbacks`:

- `devicePermission(request:)` - OS-scoped grants (camera, mic, location, push). `request` is a typed `HostDevicePermissionRequest`.
- `remotePermission(request:)` - per-product capabilities. `request` is a typed `RemotePermission`.

Both return a `Bool` granted flag; the host renders the typed request in its own prompt UI. The same typed values drive the `TrUAPIHostCore` permission admin API (`permissionAuthorizationStatus`, `setPermissionAuthorizationStatus`), which reads and updates the persisted decisions without prompting.

## Example

> **Threading:** the Rust core invokes every `HostCallbacks` method on a
> background thread it owns, never the main thread. Hop to the main thread
> (`MainActor` / `DispatchQueue.main`) before touching UIKit, WebKit, or the
> `WKWebView`. The `async` callbacks (`navigateTo`, `pushNotification`,
> `devicePermission`, `remotePermission`, `featureSupported`,
> `confirmUserAction`, `lookupPreimage`) are awaited by the core, so an
> implementation may suspend for as long as the user takes to decide (e.g.
> `await MainActor.run { ... }` or an `withCheckedContinuation` around a
> prompt); other TrUAPI traffic keeps flowing while you wait. The remaining
> sync callbacks (auth state, storage, core storage, chain, theme,
> `cancelNotification`) run inline on the dispatcher thread and must return
> promptly without blocking.

```swift
import Foundation
import WebKit
import TrUAPIHost

final class MyCallbacks: HostCallbacks, @unchecked Sendable {
    private var storage: [String: Data] = [:]
    private var coreStorage: [Data: Data] = [:]

    func onCoreLog(marker: String, detail: String) { /* log */ }

    func navigateTo(url: String) async throws {
        await MainActor.run { /* UIApplication.shared.open(...) */ }
    }

    func pushNotification(request: HostPushNotificationRequest) async throws -> UInt32 {
        let id: UInt32 = 1
        await MainActor.run { /* schedule request.text / request.deeplink / request.scheduledAt */ }
        return id
    }

    func cancelNotification(id: UInt32) throws {
        DispatchQueue.main.async { /* cancel notification */ }
    }

    func devicePermission(request: HostDevicePermissionRequest) async throws -> Bool {
        // Awaited by the core: present the prompt and suspend until the user
        // decides. Other TrUAPI traffic keeps flowing while suspended.
        await MainActor.run { /* show prompt for request (.camera, .microphone, ...); */ false }
    }

    func remotePermission(request: RemotePermission) async throws -> Bool {
        await MainActor.run { /* show prompt for request (.chainSubmit, .remote(domains:), ...); */ false }
    }

    // Core-owned auth state stream: render `.connected`/`.disconnected` as the
    // account badge and `.loginFailed` as a retryable error. This core is a
    // signing host, so it does not enter the remote-login flow: `.pairing` and
    // `.authenticating` are not emitted and `core.cancelLogin()` is inert.
    // Activate the signer with `core.activateLocalSession(secret:...)`; serving
    // paired product hosts uses the separate responder-pairing methods.
    func authStateChanged(state: AuthState) {
        DispatchQueue.main.async { /* render the state */ }
    }

    func pairingPeerDisconnected(peer: NativePairingPeer) {
        DispatchQueue.main.async { /* remove the persisted peer and update UI */ }
    }

    func coreStorageRead(key: Data) throws -> Data? { coreStorage[key] }
    func coreStorageWrite(key: Data, value: Data) throws { coreStorage[key] = value }
    func coreStorageClear(key: Data) throws { coreStorage.removeValue(forKey: key) }

    func chainConnect(genesisHash: Data) throws -> UInt32? {
        let id: UInt32 = 1
        DispatchQueue.main.async { /* open JSON-RPC connection, forward responses via core.notifyChainResponse */ }
        return id
    }

    func chainSend(connectionId: UInt32, request: String) throws {
        /* send JSON-RPC request on the host connection */
    }

    func chainClose(connectionId: UInt32) throws {
        /* close host connection */
    }

    func confirmUserAction(review: UserConfirmationReview) async throws -> Bool {
        // Switch on the review variant (.signPayload, .createTransaction, ...)
        // to render the confirmation prompt with its typed fields.
        await MainActor.run { /* render review; */ false }
    }

    func lookupPreimage(key: Data) async throws -> Data? { nil }

    func currentTheme() throws -> ThemeVariant { .dark }

    func featureSupported(request: HostFeatureSupportedRequest) async throws -> Bool { false }

    func localStorageRead(key: String) throws -> Data? { storage[key] }
    func localStorageWrite(key: String, value: Data) throws { storage[key] = value }
    func localStorageClear(key: String) throws { storage.removeValue(forKey: key) }
}

let callbacks = MyCallbacks()
let runtimeConfig = RuntimeConfig(
    productId: "my-product.dot",
    hostName: "My Host",
    hostIcon: "https://host.example/icon.png",
    peopleChainGenesisHash: Data(repeating: 0, count: 32),
    bulletinChainGenesisHash: Data(repeating: 0, count: 32),
    pairingDeeplinkScheme: .polkadotApp
)
let core = try TrUAPIHostCore(callbacks: callbacks, runtimeConfig: runtimeConfig)
try core.activateLocalSession(secret: entropyBytes, liteUsername: nil)
let endpoint = try core.startWsBridge()

// Call these from host/platform observers so native subscriptions see updates
// after their immediate current item.
core.notifyThemeChanged(theme: .dark)
core.notifyPreimageChanged(key: preimageKey, value: preimageBytesOrNil)
core.notifyChainResponse(connectionId: chainConnectionId, json: jsonRpcResponse)
core.notifyChainClosed(connectionId: chainConnectionId)

// Both scripts must be registered before the web view loads the product page,
// and in this order: the bootstrap publishes the bridge endpoint on
// `window.__truapi_localhost`; the container script then locks down the
// page's platform APIs and reads that endpoint at eval time.
let contentController = WKUserContentController()
let bootstrapScript = LocalhostBridgeBootstrap.script(port: endpoint.port, token: endpoint.token)
contentController.addUserScript(WKUserScript(
    source: bootstrapScript,
    injectionTime: .atDocumentStart,
    forMainFrameOnly: true
))
contentController.addUserScript(WKUserScript(
    source: try ContainerScriptBundle.load(),
    injectionTime: .atDocumentStart,
    forMainFrameOnly: true
))

let configuration = WKWebViewConfiguration()
configuration.userContentController = contentController
let webView = WKWebView(frame: .zero, configuration: configuration)
webView.load(URLRequest(url: URL(string: "https://your-product.example/")!))

// On logout:
core.disconnect()
```

The product page reads `window.__truapi_localhost.url` (set by the bootstrap script) and passes it to `@parity/truapi`'s `createWebSocketProvider(url)`.

## Build outputs in detail

`./scripts/rebuild.sh` orchestrates everything; the underlying pieces, should you need one in isolation:

- **xcframework** — `make xcframework` (repo root) builds `truapi-server` for `aarch64-apple-ios` and `aarch64-apple-ios-sim` and bundles `target/truapi_server.xcframework`; the script copies it into `Binaries/` and strips the per-slice `module.modulemap` (module resolution comes from the `systemLibrary` target; the slice copy collides with other xcframeworks in Xcode's flat include dir).
- **bindings** — `make uniffi` (run automatically by `make xcframework`) emits the Swift bindings into `target/uniffi-swift-out/` via the workspace `uniffi-bindgen-cli`; the script copies them into `Sources/TrUAPIHost/truapi_server.swift` and `Sources/truapi_serverFFI/include/`, renaming the emitted `truapi_serverFFI.modulemap` to `module.modulemap` so the SwiftPM `systemLibrary` target picks it up.
- **container** — `npm run build` in `js/container/` (repo root) bundles `src/index.ts` into `Sources/TrUAPIHost/Resources/truapi-container.js`.

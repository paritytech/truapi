# TrUAPI iOS host adapter

*Thin Swift shell over the Rust TrUAPI core (UniFFI). Wire decoding, request routing, and subscription lifecycle stay in the Rust core; products connect through the localhost WebSocket bridge.*

## What this package is for

The public surface lives in [`Sources/TrUAPIHost/TrUAPIHost.swift`](Sources/TrUAPIHost/TrUAPIHost.swift):

- `HostBridge` - callback bundle the embedding app implements. Split into device permissions, remote permissions, navigation, push, feature support, and scoped storage.
- `HostStorageBackend` - simple read/write/clear protocol the host backs with its own persistence.
- `TrUAPIHostCore` - owning wrapper around the UniFFI-generated `NativeTrUApiCore`. Holds the bridge alive for the lifetime of the core and exposes the localhost WebSocket bridge.
- `LocalhostBridgeBootstrap` - helper that produces a JS snippet publishing the WS bridge endpoint to the product page so it can dial back in.

The generated UniFFI bindings live alongside the shell in `Sources/TrUAPIHost/truapi_server.swift` and the C header / module map in `Sources/truapi_serverFFI/include/`. They are committed (they're large and consumers should not need a Rust toolchain).

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

The product running in the `WKWebView` opens a `WebSocket` to the localhost port + token returned by `startWsBridge`. From there the Rust core handles the wire protocol directly. Outbound responses and host-side capability callbacks (`navigateTo`, `pushNotification`, `devicePermission`, `remotePermission`, `featureSupported`, `storage`) reach the embedder through `HostBridge`.

## Permissions split

The core's `Permissions` platform trait has two methods, and so does the bridge:

- `devicePermission(request:)` - OS-scoped grants (camera, mic, location, push). `request` is a SCALE-encoded `v01::HostDevicePermissionRequest`.
- `remotePermission(request:)` - per-product capability bundles. `request` is a SCALE-encoded `v01::RemotePermissionRequest`.

Both return a `Bool` granted flag. SCALE decoding for the UI prompt is done by the `@parity/truapi` JS client (or any consumer that links the protocol crate's types directly).

## Example

> **Threading:** when the WS bridge is running, the Rust core invokes every
> `HostBridge` callback on the dedicated `truapi-ws-bridge` worker thread, never
> the main thread. Hop to the main thread (`DispatchQueue.main` / `MainActor`)
> before touching UIKit, WebKit, or the `WKWebView`. Permission callbacks return
> synchronously, so use `DispatchQueue.main.sync` (or a semaphore) to present
> the prompt on the main thread and block the worker until the user decides.

```swift
import Foundation
import WebKit
import TrUAPIHost

final class MyStorage: HostStorageBackend, @unchecked Sendable {
    private var map: [String: Data] = [:]
    func read(key: String) throws -> Data? { map[key] }
    func write(key: String, value: Data) throws { map[key] = value }
    func clear(key: String) throws { map.removeValue(forKey: key) }
}

final class MyBridge: HostBridge, @unchecked Sendable {
    let storage: HostStorageBackend = MyStorage()

    // Callbacks arrive on the `truapi-ws-bridge` worker thread, never the main
    // thread. Hop to the main thread before touching UIKit/WebKit.
    func navigateTo(url: String) throws {
        DispatchQueue.main.async { /* UIApplication.shared.open(...) */ }
    }

    func pushNotification(payload: Data) throws {
        DispatchQueue.main.async { /* schedule notification */ }
    }

    func devicePermission(request: Data) throws -> Bool {
        // Present synchronously on the main thread and return the decision.
        DispatchQueue.main.sync { /* show prompt; */ false }
    }

    func remotePermission(request: Data) throws -> Bool {
        DispatchQueue.main.sync { /* show prompt; */ false }
    }

    func featureSupported(request: Data) throws -> Bool { false }
}

let bridge = MyBridge()
let core = TrUAPIHostCore(bridge: bridge)
let endpoint = try core.startWsBridge()

let contentController = WKUserContentController()
let bootstrapScript = LocalhostBridgeBootstrap.script(port: endpoint.port, token: endpoint.token)
let userScript = WKUserScript(
    source: bootstrapScript,
    injectionTime: .atDocumentStart,
    forMainFrameOnly: true
)
contentController.addUserScript(userScript)

let configuration = WKWebViewConfiguration()
configuration.userContentController = contentController
let webView = WKWebView(frame: .zero, configuration: configuration)
webView.load(URLRequest(url: URL(string: "https://your-product.example/")!))
```

The product page reads `window.__truapi_localhost.url` (set by the bootstrap script) and passes it to `@parity/truapi`'s `createWebSocketProvider(url)`.

## Linking the cdylib

This package does not vendor `libtruapi_server` - integrators link a prebuilt static or dynamic library when building the app target. Typical workflow:

```bash
cargo build -p truapi-server --release --features ws-bridge \
  --target aarch64-apple-ios
cargo build -p truapi-server --release --features ws-bridge \
  --target aarch64-apple-ios-sim
```

Then either bundle the `.a` files as a `.xcframework` and add it under "Frameworks, Libraries, and Embedded Content" in the app target, or link directly via `OTHER_LDFLAGS`.

## Regenerating the bindings

The committed bindings under `Sources/TrUAPIHost/truapi_server.swift` and `Sources/truapi_serverFFI/include/` are produced from the workspace `uniffi-bindgen-cli`. The CLI emits `truapi_server.swift`, `truapi_serverFFI.h`, and `truapi_serverFFI.modulemap` into a single output directory; the modulemap is renamed to `module.modulemap` and the header is colocated under `Sources/truapi_serverFFI/include/` so SwiftPM's `systemLibrary` target picks them up.

```bash
cargo build -p truapi-server --release --features ws-bridge
mkdir -p /tmp/uniffi-swift-out
cargo run -p uniffi-bindgen-cli -- generate \
  --library target/release/libtruapi_server.so \
  --language swift \
  --out-dir /tmp/uniffi-swift-out
cp /tmp/uniffi-swift-out/truapi_server.swift \
   ios/truapi-host/Sources/TrUAPIHost/truapi_server.swift
cp /tmp/uniffi-swift-out/truapi_serverFFI.h \
   ios/truapi-host/Sources/truapi_serverFFI/include/truapi_serverFFI.h
cp /tmp/uniffi-swift-out/truapi_serverFFI.modulemap \
   ios/truapi-host/Sources/truapi_serverFFI/include/module.modulemap
```

Or run `make uniffi` from the repo root.

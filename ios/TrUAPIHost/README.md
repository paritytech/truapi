# TrUAPI iOS host adapter

*Thin Swift shell over the Rust TrUAPI core (UniFFI) plus a `WKWebView` byte transport. Wire decoding, request routing, and subscription lifecycle stay in the Rust core.*

## What this package is for

The public surface lives in [`Sources/TrUAPIHost/TrUAPIHost.swift`](Sources/TrUAPIHost/TrUAPIHost.swift):

- `HostBridge` - callback bundle the embedding app implements. Split into device permissions, remote permissions, navigation, push, feature support, and scoped storage.
- `TrUAPIHostCore` - owning wrapper around the UniFFI-generated `NativeTrUApiCore`. Implements `CoreInbound`, owns the bridge lifetime, exposes session and WS bridge controls.
- `WebViewTransport` - base64-over-`WKScriptMessageHandler` byte pipe between a `WKWebView` and any `CoreInbound`. Installs a `window.trUApi` shim that matches the JS host adapter shape.
- `LocalhostBridgeBootstrap` - script for the localhost WebSocket bridge mode (when the cdylib is built with `--features ws-bridge` and `TrUAPIHostCore.startWsBridge(...)` is invoked).

The generated UniFFI bindings live alongside the shell in `Sources/TrUAPIHost/truapi_server.swift` and the C header / module map in `Sources/truapi_serverFFI/include/`. They are committed (they're large and consumers should not need a Rust toolchain).

## Architecture

```text
product app in WKWebView
  Uint8Array frames via window.trUApi
           |
           v
WebViewTransport
  base64 over WKScriptMessageHandler
           |
           v
TrUAPIHostCore (CoreInbound)
  → uniffi → libtruapi_server
```

For embedded apps that prefer the localhost WebSocket bridge:

```text
product app in WKWebView
  binary frames via localhost WebSocket
           |
           v
Rust core WS bridge (started via startWsBridge)
           |
           v
Rust core dispatcher
```

## Permissions split

The core's `Permissions` platform trait has two methods, and so does the bridge:

- `devicePermission(request:)` - OS-scoped grants (camera, mic, location, push). `request` is a SCALE-encoded `v01::HostDevicePermissionRequest`.
- `remotePermission(request:)` - per-product capability bundles. `request` is a SCALE-encoded `v01::RemotePermissionRequest`.

Both return a `Bool` granted flag. SCALE decoding for the UI prompt is done by the `@parity/truapi` JS client (or any consumer that links the protocol crate's types directly).

## Example

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
    weak var transport: WebViewTransport?

    func onCoreResponse(frame: Data) {
        Task { @MainActor in transport?.sendToProduct(frame) }
    }

    func navigateTo(url: String) throws { /* open in browser */ }
    func pushNotification(payload: Data) throws { /* show notification */ }
    func devicePermission(request: Data) throws -> Bool { false }
    func remotePermission(request: Data) throws -> Bool { false }
    func featureSupported(request: Data) throws -> Bool { false }
}

let bridge = MyBridge()
let core = TrUAPIHostCore(bridge: bridge)

let contentController = WKUserContentController()
let configuration = WKWebViewConfiguration()
configuration.userContentController = contentController
let webView = WKWebView(frame: .zero, configuration: configuration)

let transport = WebViewTransport(webView: webView, core: core)
bridge.transport = transport
transport.attach(to: contentController)
```

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
   ios/TrUAPIHost/Sources/TrUAPIHost/truapi_server.swift
cp /tmp/uniffi-swift-out/truapi_serverFFI.h \
   ios/TrUAPIHost/Sources/truapi_serverFFI/include/truapi_serverFFI.h
cp /tmp/uniffi-swift-out/truapi_serverFFI.modulemap \
   ios/TrUAPIHost/Sources/truapi_serverFFI/include/module.modulemap
```

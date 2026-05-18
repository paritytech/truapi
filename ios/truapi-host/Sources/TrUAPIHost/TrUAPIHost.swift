// TrUAPIHost - iOS host adapter.
//
// The Rust core (compiled to `libtruapi_server`, surfaced through UniFFI in
// the sibling `truapi_server.swift` file) owns wire decoding, request
// routing, subscription lifecycle, and platform trait dispatch.
//
// This file exposes:
//
//   * `HostBridge` - a Swift-friendly callback bundle the embedding app
//     implements. It splits device and remote permissions, mirroring the
//     `Permissions` platform trait in the Rust core.
//   * `TrUAPIHostCore` - owning wrapper around the UniFFI-generated
//     `NativeTrUApiCore`. Holds the bridge alive for the lifetime of the
//     core and exposes session + WS-bridge controls.
//   * `LocalhostBridgeBootstrap` - small JS snippet that publishes the WS
//     bridge endpoint to the product page so it can dial back in.
//
// Products running inside a `WKWebView` connect to the Rust core via the
// localhost WebSocket bridge. The bootstrap script publishes the URL
// (`ws://127.0.0.1:<port>/?t=<token>`); products feed it to
// `@parity/truapi`'s `createWebSocketProvider(url)`.

import Foundation

/// Package metadata.
public enum TrUAPIHost {
    public static let version = "0.1.0"
}

/// Bootstrap helper for the native localhost WebSocket bridge that the Rust
/// core stands up via `NativeTrUApiCore.startWsBridge(bindPort:)` when the
/// cdylib is built with the `ws-bridge` feature.
public enum LocalhostBridgeBootstrap {
    /// Returns a `<script>`-injectable snippet that publishes the endpoint
    /// metadata on `window.__truapi_localhost` and fires a `truapi-native-ready`
    /// event. The product reads the URL and passes it to
    /// `createWebSocketProvider` from `@parity/truapi`.
    public static func script(port: UInt16, token: String) -> String {
        let url = "ws://127.0.0.1:\(port)/?t=\(token)"
        let safeUrl = escapeJavaScriptString(url)
        let safeToken = escapeJavaScriptString(token)
        return """
        (function() {
          window.__truapi_localhost = { url: '\(safeUrl)', token: '\(safeToken)' };
          window.dispatchEvent(new Event('truapi-native-ready'));
        })();
        """
    }

    private static func escapeJavaScriptString(_ value: String) -> String {
        value
            .replacingOccurrences(of: "\\", with: "\\\\")
            .replacingOccurrences(of: "'", with: "\\'")
    }
}

/// Storage backend the host provides to the Rust core. Throwing closures
/// can surface quota or unknown failures by raising `HostStorageError`
/// (defined in the generated bindings).
public protocol HostStorageBackend: AnyObject, Sendable {
    func read(key: String) throws -> Data?
    func write(key: String, value: Data) throws
    func clear(key: String) throws
}

/// Host-side callback bundle that the Rust core invokes for capabilities the
/// native shell owns. The permission split mirrors the Rust `Permissions`
/// trait:
///
///   * ``devicePermission(request:)`` handles OS-scoped grants (camera,
///     mic, location). `request` is a SCALE-encoded
///     `v01::HostDevicePermissionRequest`.
///   * ``remotePermission(request:)`` handles per-product capability
///     bundles. `request` is a SCALE-encoded `v01::RemotePermissionRequest`.
///
/// Embedders typically forward the SCALE payloads through the
/// `@parity/truapi` JS client for UI prompts, then return the boolean
/// granted flag.
public protocol HostBridge: AnyObject, Sendable {
    /// Lifecycle logger. Marker is a stable slug, detail is free-form.
    func onCoreLog(marker: String, detail: String)

    /// Forward an outbound SCALE-encoded protocol frame to the product.
    func onCoreResponse(frame: Data)

    /// Open a URL in the system browser.
    func navigateTo(url: String) throws

    /// Deliver a push notification (SCALE-encoded `HostPushNotificationRequest`).
    func pushNotification(payload: Data) throws

    /// Prompt for a device-level permission. Returns the granted flag.
    func devicePermission(request: Data) throws -> Bool

    /// Prompt for a remote (product-scoped) permission bundle.
    func remotePermission(request: Data) throws -> Bool

    /// Answer a feature-support query.
    func featureSupported(request: Data) throws -> Bool

    /// Scoped key-value storage for the Rust core.
    var storage: HostStorageBackend { get }
}

public extension HostBridge {
    /// Default no-op logger. Override to plumb into your logging framework.
    func onCoreLog(marker: String, detail: String) {}
}

/// Adapter that bridges the public `HostBridge` to the generated UniFFI
/// `HostCallbacks` protocol. Kept private so the generated names never
/// leak into consumers.
private final class HostCallbackAdapter: HostCallbacks, @unchecked Sendable {
    private let bridge: HostBridge

    init(bridge: HostBridge) {
        self.bridge = bridge
    }

    func onCoreLog(marker: String, detail: String) {
        bridge.onCoreLog(marker: marker, detail: detail)
    }

    func onCoreResponse(frame: Data) {
        bridge.onCoreResponse(frame: frame)
    }

    func navigateTo(url: String) throws {
        try bridge.navigateTo(url: url)
    }

    func pushNotification(payload: Data) throws {
        try bridge.pushNotification(payload: payload)
    }

    func devicePermission(request: Data) throws -> Bool {
        try bridge.devicePermission(request: request)
    }

    func remotePermission(request: Data) throws -> Bool {
        try bridge.remotePermission(request: request)
    }

    func featureSupported(request: Data) throws -> Bool {
        try bridge.featureSupported(request: request)
    }

    func localStorageRead(key: String) throws -> Data? {
        try bridge.storage.read(key: key)
    }

    func localStorageWrite(key: String, value: Data) throws {
        try bridge.storage.write(key: key, value: value)
    }

    func localStorageClear(key: String) throws {
        try bridge.storage.clear(key: key)
    }
}

/// Owning wrapper around the Rust-backed `NativeTrUApiCore`. Holds the bridge
/// adapter alive for the lifetime of the core and exposes session +
/// WS-bridge controls.
///
/// Hosts integrating with a `WKWebView`-based product call `startWsBridge`
/// and pass the resulting `ws://127.0.0.1:<port>/?t=<token>` URL to the
/// product via `LocalhostBridgeBootstrap.script(...)`. The product wires
/// that URL into `@parity/truapi`'s `createWebSocketProvider`. Direct
/// `receiveFromProduct` is available for tests and alternative transports.
public final class TrUAPIHostCore {
    private let inner: NativeTrUApiCore
    // Retained so the UniFFI callback vtable stays valid for the lifetime
    // of `inner`.
    private let callbackRetainer: HostCallbacks

    public init(bridge: HostBridge) {
        let adapter = HostCallbackAdapter(bridge: bridge)
        self.callbackRetainer = adapter
        self.inner = NativeTrUApiCore(callbacks: adapter)
    }

    /// Deliver an opaque SCALE-encoded wire frame into the Rust core. The
    /// WS bridge feeds the core internally; this entrypoint is exposed for
    /// tests and alternative transports.
    public func receiveFromProduct(_ frame: Data) {
        _ = inner.receiveFromProduct(frame: frame)
    }

    /// Set the currently-paired session. `pubkey` must be exactly 32 bytes.
    @discardableResult
    public func setActiveSession(
        pubkey: Data,
        liteUsername: String? = nil,
        fullUsername: String? = nil
    ) -> Bool {
        inner.setActiveSession(pubkey: pubkey, liteUsername: liteUsername, fullUsername: fullUsername)
    }

    /// Drop the currently-paired session.
    public func clearActiveSession() {
        inner.clearActiveSession()
    }

    /// Start the localhost WebSocket bridge. Requires the `ws-bridge`
    /// feature in the cdylib. Pair the returned `WsBridgeEndpoint` with
    /// `LocalhostBridgeBootstrap.script(...)` to hand the URL to the
    /// product page.
    public func startWsBridge(bindPort: UInt16 = 0) throws -> WsBridgeEndpoint {
        try inner.startWsBridge(bindPort: bindPort)
    }

    /// Stop the localhost WebSocket bridge (if running).
    public func stopWsBridge() {
        inner.stopWsBridge()
    }

    /// Smoke-test helper: returns a SCALE-encoded `feature_supported`
    /// request frame so the iOS shell can verify the wire path.
    public func debugSmokeFeatureRequestFrame() -> Data {
        inner.debugSmokeFeatureRequestFrame()
    }
}

// TrUAPIHost - iOS host adapter.
//
// The Rust core (compiled to `libtruapi_server`, surfaced through UniFFI in
// the sibling `truapi_server.swift` file) owns wire decoding, request
// routing, subscription lifecycle, and platform trait dispatch.
//
// This file layers two things on top of the generated bindings:
//
//   * `HostBridge` - a Swift-friendly callback bundle the embedding app
//     implements. It splits device and remote permissions, mirroring the
//     `Permissions` platform trait in the Rust core.
//   * `WebViewTransport` - a base64-over-`WKScriptMessageHandler` byte pipe
//     between a `WKWebView` and any `CoreInbound`.
//
// `LocalhostBridgeBootstrap` is retained from earlier iOS shells for hosts
// that prefer the localhost WebSocket bridge over the direct WK script
// bridge.

import Foundation
import WebKit

/// Package metadata.
public enum TrUAPIHost {
    public static let version = "0.1.0"
}

/// Bootstrap helper for the native localhost WebSocket bridge that the Rust
/// core can stand up via `NativeTrUApiCore.startWsBridge(bindPort:)` when
/// the cdylib is built with the `ws-bridge` feature.
public enum LocalhostBridgeBootstrap {
    /// Returns a `<script>`-injectable snippet that publishes the endpoint
    /// metadata on `window.__truapi_localhost` and fires a `truapi-native-ready`
    /// event. The product client reads this and dials back in.
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

/// Sink for opaque wire frames coming from the WebView. The Rust core
/// (via `TrUAPIHostCore`) is the typical implementor; tests may use a stub.
public protocol CoreInbound: AnyObject {
    func receiveFromProduct(_ frame: Data)
}

/// Owning wrapper around the Rust-backed `NativeTrUApiCore`. Implements
/// `CoreInbound` so a `WebViewTransport` can hand inbound frames over
/// directly, and exposes session and WS bridge controls.
///
/// Holds a strong reference to the bridge adapter so the UniFFI callback
/// vtable stays valid for the lifetime of the core.
public final class TrUAPIHostCore: CoreInbound {
    private let inner: NativeTrUApiCore
    // Retained so the UniFFI callback vtable stays valid for the lifetime
    // of `inner`. Not read directly; the suppression keeps -Wunused happy.
    private let callbackRetainer: HostCallbacks

    public init(bridge: HostBridge) {
        let adapter = HostCallbackAdapter(bridge: bridge)
        self.callbackRetainer = adapter
        self.inner = NativeTrUApiCore(callbacks: adapter)
        _ = self.callbackRetainer
    }

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

    /// Start the localhost WebSocket bridge.
    /// Requires the `ws-bridge` feature in the cdylib.
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

/// The iOS-side byte transport. Wraps a `WKWebView`; forwards bytes between
/// the JS bridge and a `CoreInbound` (typically a `TrUAPIHostCore`).
@MainActor
public final class WebViewTransport: NSObject, WKScriptMessageHandler {
    private weak var webView: WKWebView?
    private weak var core: AnyObject?
    private let coreSend: (Data) -> Void
    private let callbackName: String
    private let messageName: String

    public init(
        webView: WKWebView,
        core: CoreInbound,
        callbackName: String = "__trUApiReceive",
        messageName: String = "trUApi"
    ) {
        self.webView = webView
        self.core = core
        self.coreSend = { [weak core] frame in core?.receiveFromProduct(frame) }
        self.callbackName = callbackName
        self.messageName = messageName
        super.init()
    }

    /// JS bootstrap to inject as a `WKUserScript` so the page exposes a
    /// `window.trUApi` byte-pipe matching the JS host adapter shape.
    public var bootstrapScript: String {
        return """
        (function() {
          var listeners = [];
          window.\(callbackName) = function(b64) {
            try {
              var bin = atob(b64);
              var bytes = new Uint8Array(bin.length);
              for (var i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
              listeners.forEach(function(l) { l(bytes); });
            } catch (e) { console.error('trUApi recv error', e); }
          };
          function toB64(u8) {
            var s = '';
            for (var i = 0; i < u8.length; i++) s += String.fromCharCode(u8[i]);
            return btoa(s);
          }
          window.trUApi = {
            postMessage: function(bytes) {
              window.webkit.messageHandlers.\(messageName).postMessage(toB64(bytes));
            },
            subscribe: function(cb) {
              listeners.push(cb);
              return function() {
                var i = listeners.indexOf(cb);
                if (i >= 0) listeners.splice(i, 1);
              };
            }
          };
          window.dispatchEvent(new Event('truapi-native-ready'));
        })();
        """
    }

    public func attach(to controller: WKUserContentController) {
        controller.add(self, name: messageName)
        let script = WKUserScript(source: bootstrapScript, injectionTime: .atDocumentStart, forMainFrameOnly: true)
        controller.addUserScript(script)
    }

    public func detach(from controller: WKUserContentController) {
        controller.removeScriptMessageHandler(forName: messageName)
    }

    /// Called by the host (typically from `HostBridge.onCoreResponse`) when
    /// the core has bytes to push back into the product app.
    public func sendToProduct(_ frame: Data) {
        guard let webView else { return }
        let b64 = frame.base64EncodedString()
        let js = "window.\(callbackName) && window.\(callbackName)('\(b64)')"
        webView.evaluateJavaScript(js, completionHandler: nil)
    }

    public func userContentController(
        _ userContentController: WKUserContentController,
        didReceive message: WKScriptMessage
    ) {
        guard let b64 = message.body as? String,
              let data = Data(base64Encoded: b64) else { return }
        coreSend(data)
    }
}

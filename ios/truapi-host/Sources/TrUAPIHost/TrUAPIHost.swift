// TrUAPIHost - iOS host adapter.
//
// The Rust core (compiled to `libtruapi_server`, surfaced through UniFFI in
// the sibling `truapi_server.swift` file) owns wire decoding, request
// routing, subscription lifecycle, and platform trait dispatch.
//
// This file exposes:
//
//   * `TrUAPIHostCore` - owning wrapper around the UniFFI-generated
//     `NativeTrUApiCore`. Takes `HostCallbacks` directly and exposes
//     session + WS-bridge controls.
//   * `LocalhostBridgeBootstrap` - small JS snippet that publishes the WS
//     bridge endpoint to the product page so it can dial back in.
//
// Products running inside a `WKWebView` connect to the Rust core via the
// localhost WebSocket bridge. The bootstrap script publishes the URL
// (`ws://127.0.0.1:<port>/?t=<token>`) and a MessagePort-shaped compatibility
// object that proxies the product's existing webview transport onto it.

import Foundation

/// Package metadata.
public enum TrUAPIHost {
    public static let version = "0.1.0"
}

/// Deeplink scheme used when the Rust core builds SSO pairing payloads.
public enum PairingDeeplinkScheme: Sendable {
    case polkadotApp
    case polkadotAppDev

    fileprivate var native: NativePairingDeeplinkScheme {
        switch self {
        case .polkadotApp:
            return .polkadotApp
        case .polkadotAppDev:
            return .polkadotAppDev
        }
    }
}

/// Stable identity for a paired product host. Persist these two public keys so
/// the Rust responder can restore its encrypted Statement Store subscription
/// after the native app restarts.
public struct PairingPeer: Sendable, Hashable {
    public let statementAccountId: Data
    public let encryptionPublicKey: Data

    public init(statementAccountId: Data, encryptionPublicKey: Data) {
        self.statementAccountId = statementAccountId
        self.encryptionPublicKey = encryptionPublicKey
    }

    fileprivate init(native: NativePairingPeer) {
        statementAccountId = native.statementAccountId
        encryptionPublicKey = native.encryptionPublicKey
    }

    fileprivate var native: NativePairingPeer {
        NativePairingPeer(
            statementAccountId: statementAccountId,
            encryptionPublicKey: encryptionPublicKey
        )
    }
}

/// Static product and pairing config supplied before the Rust core handles
/// product calls. One core instance represents one product identity.
///
/// `hostName`, `hostIcon`, `hostVersion`, `platformType`, and
/// `platformVersion` describe the host to the wallet during SSO pairing.
/// `peopleChainGenesisHash` and `bulletinChainGenesisHash` must each be
/// exactly 32 bytes.
public struct RuntimeConfig: Sendable {
    public let productId: String
    public let hostName: String
    public let hostIcon: String?
    public let hostVersion: String?
    public let platformType: String?
    public let platformVersion: String?
    public let peopleChainGenesisHash: Data
    public let bulletinChainGenesisHash: Data
    public let localSessionSecret: Data?
    public let localSessionLiteUsername: String?
    public let pairingDeeplinkScheme: PairingDeeplinkScheme

    public init(
        productId: String,
        hostName: String,
        hostIcon: String? = nil,
        hostVersion: String? = nil,
        platformType: String? = nil,
        platformVersion: String? = nil,
        peopleChainGenesisHash: Data,
        bulletinChainGenesisHash: Data,
        localSessionSecret: Data? = nil,
        localSessionLiteUsername: String? = nil,
        pairingDeeplinkScheme: PairingDeeplinkScheme = .polkadotApp
    ) {
        self.productId = productId
        self.hostName = hostName
        self.hostIcon = hostIcon
        self.hostVersion = hostVersion
        self.platformType = platformType
        self.platformVersion = platformVersion
        self.peopleChainGenesisHash = peopleChainGenesisHash
        self.bulletinChainGenesisHash = bulletinChainGenesisHash
        self.localSessionSecret = localSessionSecret
        self.localSessionLiteUsername = localSessionLiteUsername
        self.pairingDeeplinkScheme = pairingDeeplinkScheme
    }

    fileprivate var native: NativeRuntimeConfig {
        NativeRuntimeConfig(
            productId: productId,
            hostName: hostName,
            hostIcon: hostIcon,
            hostVersion: hostVersion,
            platformType: platformType,
            platformVersion: platformVersion,
            peopleChainGenesisHash: peopleChainGenesisHash,
            bulletinChainGenesisHash: bulletinChainGenesisHash,
            localSessionSecret: localSessionSecret,
            localSessionLiteUsername: localSessionLiteUsername,
            pairingDeeplinkScheme: pairingDeeplinkScheme.native
        )
    }
}

/// Bootstrap helper for the native localhost WebSocket bridge that the Rust
/// core stands up via `NativeTrUApiCore.startWsBridge(bindPort:)` when the
/// cdylib is built with the `ws-bridge` feature.
public enum LocalhostBridgeBootstrap {
    /// Returns a `<script>`-injectable snippet that publishes the endpoint
    /// metadata on `window.__truapi_localhost`, exposes the legacy
    /// `window.__HOST_API_PORT__` webview transport shape, and fires a
    /// `truapi-native-ready` event.
    public static func script(port: UInt16, token: String) -> String {
        let url = "ws://127.0.0.1:\(port)/?t=\(token)"
        let safeUrl = jsStringLiteral(url)
        let safeToken = jsStringLiteral(token)
        return """
        (function() {
          var endpoint = { url: \(safeUrl), token: \(safeToken) };

          function createWebSocketMessagePort(url) {
            var socket = null;
            var started = false;
            var queue = [];

            var port = {
              onmessage: null,
              onmessageerror: null,

              postMessage: function(message) {
                if (socket && socket.readyState === WebSocket.OPEN) {
                  socket.send(message);
                } else {
                  queue.push(message);
                }
              },

              start: function() {
                if (started) return;
                started = true;

                socket = new WebSocket(url);
                socket.binaryType = "arraybuffer";

                socket.onopen = function() {
                  var pending = queue;
                  queue = [];
                  pending.forEach(function(message) {
                    socket.send(message);
                  });
                };

                socket.onmessage = function(event) {
                  if (typeof port.onmessage === "function") {
                    port.onmessage({ data: new Uint8Array(event.data) });
                  }
                };

                socket.onerror = function() {
                  if (typeof port.onmessageerror === "function") {
                    port.onmessageerror();
                  }
                };

                socket.onclose = function() {
                  if (typeof port.onmessageerror === "function") {
                    port.onmessageerror();
                  }
                };
              },

              close: function() {
                queue = [];
                if (socket) {
                  socket.close();
                }
              }
            };

            return port;
          }

          window.__truapi_localhost = endpoint;
          window.__HOST_WEBVIEW_MARK__ = true;
          window.__HOST_API_PORT__ = createWebSocketMessagePort(endpoint.url);
          window.dispatchEvent(new Event('truapi-native-ready'));
        })();
        """
    }

    /// Encodes `value` as a complete double-quoted JavaScript string literal,
    /// safe to embed inside a `<script>` body. `JSONEncoder` escapes quotes,
    /// backslashes, control characters, and forward slashes (closing `</script`
    /// tags); U+2028 / U+2029 are escaped explicitly because JSON leaves them
    /// raw while JS treats them as line terminators. Falls back to an empty
    /// literal if encoding ever fails.
    private static func jsStringLiteral(_ value: String) -> String {
        guard let data = try? JSONEncoder().encode(value),
              let encoded = String(data: data, encoding: .utf8)
        else {
            return "\"\""
        }
        return encoded
            .replacingOccurrences(of: "\u{2028}", with: "\\u2028")
            .replacingOccurrences(of: "\u{2029}", with: "\\u2029")
    }
}

/// Session + WS-bridge controls of the Rust core, abstracted so hosts and
/// runtimes can depend on the interface (and tests can mock it) without
/// booting the Rust cdylib.
public protocol TrUAPIHostCoreProtocol: AnyObject {
    func startWsBridge(bindPort: UInt16) throws -> WsBridgeEndpoint
    func stopWsBridge()
    func disconnect()
    func cancelLogin()
    func activateLocalSession(secret: Data, liteUsername: String?) throws
    func respondToPairing(deeplink: String) throws -> PairingPeer
    func resumePairing(peer: PairingPeer) throws
    func disconnectPairing(peer: PairingPeer) throws
    func suspendPairing(peer: PairingPeer) throws
    func suspendAllPairings()
    func permissionAuthorizationStatus(
        request: PermissionAuthorizationRequest
    ) throws -> PermissionAuthorizationStatus
    func setPermissionAuthorizationStatus(
        request: PermissionAuthorizationRequest,
        status: PermissionAuthorizationStatus
    ) throws
    func notifyThemeChanged(theme: ThemeVariant)
    func notifyPreimageChanged(key: Data, value: Data?)
    func notifyChainResponse(connectionId: UInt32, json: String)
    func notifyChainClosed(connectionId: UInt32)
}

/// Owning wrapper around the Rust-backed `NativeTrUApiCore`. Holds the
/// callbacks alive for the lifetime of the core and exposes session +
/// WS-bridge controls.
///
/// Hosts integrating with a `WKWebView`-based product call `startWsBridge`
/// and pass the resulting `ws://127.0.0.1:<port>/?t=<token>` URL to the
/// product via `LocalhostBridgeBootstrap.script(...)`. The product wires
/// that URL into `@parity/truapi`'s `createWebSocketProvider`.
public final class TrUAPIHostCore: TrUAPIHostCoreProtocol {
    let inner: NativeTrUApiCore

    // Rust holds the callback handle; this retainer pins the Swift side for
    // the core's lifetime.
    private let callbackRetainer: HostCallbacks

    /// Boot the Rust core against the host callbacks.
    public init(callbacks: HostCallbacks, runtimeConfig: RuntimeConfig) throws {
        callbackRetainer = callbacks
        inner = try NativeTrUApiCore.withRuntimeConfig(
            callbacks: callbacks,
            runtimeConfig: runtimeConfig.native
        )
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

    /// Core-owned logout/disconnect path. Best-effort notifies the SSO peer,
    /// clears in-memory session state, clears the persisted session via
    /// core storage, and broadcasts `Disconnected` to active
    /// account-status subscribers.
    public func disconnect() {
        inner.disconnect()
    }

    /// Cancel an in-flight pairing login.
    ///
    /// Inert on a native host: the core is a signing host with no pairing flow
    /// to cancel, so calling this emits no auth state and changes nothing.
    public func cancelLogin() {
        inner.cancelLogin()
    }

    /// Activate or replace the local signing-host session from host-held raw
    /// BIP-39 entropy.
    public func activateLocalSession(secret: Data, liteUsername: String? = nil) throws {
        try inner.activateLocalSession(secret: secret, liteUsername: liteUsername)
    }

    /// Answer a pairing deeplink and start serving the session in Rust. This
    /// returns once the handshake statement has been accepted; session traffic
    /// continues on the core's background pool.
    public func respondToPairing(deeplink: String) throws -> PairingPeer {
        PairingPeer(native: try inner.respondToPairing(deeplink: deeplink))
    }

    /// Restore the Rust responder for a persisted pairing host.
    public func resumePairing(peer: PairingPeer) throws {
        try inner.resumePairing(peer: peer.native)
    }

    /// Notify one pairing host of a local disconnect and stop serving it.
    public func disconnectPairing(peer: PairingPeer) throws {
        try inner.disconnectPairing(peer: peer.native)
    }

    /// Stop one responder without sending a disconnect. The peer can be
    /// resumed later from its persisted public keys.
    public func suspendPairing(peer: PairingPeer) throws {
        try inner.suspendPairing(peer: peer.native)
    }

    /// Stop every responder without changing persisted pairings.
    public func suspendAllPairings() {
        inner.suspendAllPairings()
    }

    /// Read a stored permission authorization status without prompting.
    public func permissionAuthorizationStatus(
        request: PermissionAuthorizationRequest
    ) throws -> PermissionAuthorizationStatus {
        try inner.permissionAuthorizationStatus(request: request)
    }

    /// Update a stored permission authorization status. `.notDetermined`
    /// clears the stored value so the next product request prompts again.
    public func setPermissionAuthorizationStatus(
        request: PermissionAuthorizationRequest,
        status: PermissionAuthorizationStatus
    ) throws {
        try inner.setPermissionAuthorizationStatus(request: request, status: status)
    }

    /// Push a host theme update to active TrUAPI theme subscriptions.
    public func notifyThemeChanged(theme: ThemeVariant) {
        inner.notifyThemeChanged(theme: theme)
    }

    /// Push a preimage lookup update to active subscriptions for `key`.
    public func notifyPreimageChanged(key: Data, value: Data?) {
        inner.notifyPreimageChanged(key: key, value: value)
    }

    /// Push a JSON-RPC response from a native chain connection into the core.
    public func notifyChainResponse(connectionId: UInt32, json: String) {
        inner.notifyChainResponse(connectionId: connectionId, json: json)
    }

    /// Notify the core that a native chain connection closed externally.
    public func notifyChainClosed(connectionId: UInt32) {
        inner.notifyChainClosed(connectionId: connectionId)
    }
}
